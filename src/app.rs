// file: src/app.rs
// description: GPUI application state model
// reference: https://github.com/zed-industries/zed

use gpui::*;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Instant;
use tracing::{debug, error, info, warn};

use artifact::config::AppConfig;
use artifact::database::{DeletionDatabase, DeletionRecord};
use artifact::directory_item::DirectoryItem;
use artifact::scanner::Scanner;
use artifact::utils;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanState {
    Idle,
    Scanning,
    Complete,
}

#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub directories_scanned: usize,
    pub items_found: usize,
    pub current_path: String,
    pub total_size_found: u64,
    pub elapsed_secs: f64,
}

#[derive(Debug, Clone)]
pub struct BrowseEntry {
    pub name: String,
    pub path: PathBuf,
}

// ---------------------------------------------------------------------------
// Internal messages
// ---------------------------------------------------------------------------

enum ScanMessage {
    Progress(ScanProgress),
    Complete(Vec<DirectoryItem>),
    Error(String),
}

// ---------------------------------------------------------------------------
// App model
// ---------------------------------------------------------------------------

pub struct ArtifactApp {
    // Scan state
    scan_path: String,
    scan_node_modules: bool,
    scan_rust_target: bool,
    scan_state: ScanState,
    scan_progress_data: Option<ScanProgress>,
    scan_receiver: Option<Arc<Mutex<Receiver<ScanMessage>>>>,

    // Directory state
    directories: Vec<DirectoryItem>,
    total_size: u64,
    selected_size: u64,

    // Filters
    show_orphaned_only: bool,

    // Results
    deleted_count: usize,
    error_message: Option<String>,

    // Database
    database: Option<Arc<DeletionDatabase>>,

    // File browser
    show_file_browser: bool,
    browse_path: PathBuf,
    browse_entries: Vec<BrowseEntry>,
}

// ---------------------------------------------------------------------------
// Read-only getters
// ---------------------------------------------------------------------------

impl ArtifactApp {
    pub fn scan_state(&self) -> ScanState {
        self.scan_state
    }
    pub fn scan_progress_data(&self) -> Option<&ScanProgress> {
        self.scan_progress_data.as_ref()
    }
    pub fn scan_path(&self) -> &str {
        &self.scan_path
    }
    pub fn scan_node_modules(&self) -> bool {
        self.scan_node_modules
    }
    pub fn scan_rust_target(&self) -> bool {
        self.scan_rust_target
    }
    pub fn total_size(&self) -> u64 {
        self.total_size
    }
    pub fn selected_size(&self) -> u64 {
        self.selected_size
    }
    pub fn deleted_count(&self) -> usize {
        self.deleted_count
    }
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
    pub fn show_orphaned_only(&self) -> bool {
        self.show_orphaned_only
    }
    pub fn is_file_browser_open(&self) -> bool {
        self.show_file_browser
    }
    pub fn browse_path(&self) -> &PathBuf {
        &self.browse_path
    }
    pub fn browse_entries(&self) -> &[BrowseEntry] {
        &self.browse_entries
    }
}

// ---------------------------------------------------------------------------
// Construction & mutations
// ---------------------------------------------------------------------------

impl ArtifactApp {
    pub fn new(config: AppConfig, cx: &mut App) -> Entity<Self> {
        info!("Initializing ArtifactApp");

        let home = utils::get_home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .to_string_lossy()
            .to_string();

        let db_path = config.get_db_path();
        let database = match DeletionDatabase::new(Some(db_path)) {
            Ok(db) => {
                info!("Database initialized successfully");
                Some(Arc::new(db))
            }
            Err(e) => {
                error!("Failed to initialize database: {}", e);
                None
            }
        };

        let home_path = PathBuf::from(&home);

        cx.new(|_cx| Self {
            scan_path: home.clone(),
            scan_node_modules: true,
            scan_rust_target: true,
            scan_state: ScanState::Idle,
            scan_progress_data: None,
            scan_receiver: None,
            directories: Vec::new(),
            total_size: 0,
            selected_size: 0,
            show_orphaned_only: false,
            deleted_count: 0,
            error_message: None,
            database,
            show_file_browser: false,
            browse_path: home_path,
            browse_entries: Vec::new(),
        })
    }

    // -- Scan option toggles ------------------------------------------------

    pub fn toggle_node_modules(&mut self, cx: &mut Context<Self>) {
        self.scan_node_modules = !self.scan_node_modules;
        cx.notify();
    }

    pub fn toggle_rust_target(&mut self, cx: &mut Context<Self>) {
        self.scan_rust_target = !self.scan_rust_target;
        cx.notify();
    }

    pub fn toggle_orphaned_only(&mut self, cx: &mut Context<Self>) {
        self.show_orphaned_only = !self.show_orphaned_only;
        cx.notify();
    }

    // -- Scanning -----------------------------------------------------------

    pub fn start_scan(&mut self, cx: &mut Context<Self>) {
        info!("Starting scan at path: {}", self.scan_path);

        self.scan_state = ScanState::Scanning;
        self.directories.clear();
        self.total_size = 0;
        self.selected_size = 0;
        self.error_message = None;
        self.scan_progress_data = None;

        let (tx, rx) = channel();
        self.scan_receiver = Some(Arc::new(Mutex::new(rx)));

        let scan_path = self.scan_path.clone();
        let scan_node_modules = self.scan_node_modules;
        let scan_rust_target = self.scan_rust_target;
        let start_time = Instant::now();

        thread::spawn(move || {
            let scanner = Scanner::new(PathBuf::from(&scan_path));

            match scanner.scan_with_progress(
                scan_node_modules,
                scan_rust_target,
                |dirs_scanned, items_found, current_path, total_size| {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let _ = tx.send(ScanMessage::Progress(ScanProgress {
                        directories_scanned: dirs_scanned,
                        items_found,
                        current_path: current_path.to_string(),
                        total_size_found: total_size,
                        elapsed_secs: elapsed,
                    }));
                },
            ) {
                Ok(results) => {
                    info!("Scan completed with {} results", results.len());
                    let _ = tx.send(ScanMessage::Complete(results));
                }
                Err(e) => {
                    error!("Scan failed: {}", e);
                    let _ = tx.send(ScanMessage::Error(e.user_message()));
                }
            }
        });

        cx.notify();
    }

    pub fn check_scan_progress(&mut self, cx: &mut Context<Self>) {
        let rx = match self.scan_receiver.clone() {
            Some(rx) => rx,
            None => return,
        };

        let rx = rx.lock();
        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }
        drop(rx);

        for msg in messages {
            match msg {
                ScanMessage::Progress(progress) => {
                    self.scan_progress_data = Some(progress);
                    cx.notify();
                }
                ScanMessage::Complete(dirs) => {
                    self.directories = dirs;
                    self.total_size = self.directories.iter().map(|d| d.size_bytes).sum();
                    self.scan_state = ScanState::Complete;
                    self.scan_progress_data = None;
                    self.scan_receiver = None;
                    cx.notify();
                }
                ScanMessage::Error(err) => {
                    self.error_message = Some(err);
                    self.scan_state = ScanState::Idle;
                    self.scan_progress_data = None;
                    self.scan_receiver = None;
                    cx.notify();
                }
            }
        }
    }

    // -- Selection & deletion -----------------------------------------------

    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        info!("Deleting selected directories");

        let to_delete: Vec<_> = self
            .directories
            .iter()
            .filter(|d| d.selected)
            .cloned()
            .collect();

        info!("Preparing to delete {} directories", to_delete.len());

        let mut success_count = 0;
        let mut errors = Vec::new();

        for item in to_delete {
            debug!("Deleting directory: {}", item.path.display());

            match utils::delete_directory(&item.path) {
                Ok(_) => {
                    info!("Successfully deleted: {}", item.path.display());
                    success_count += 1;

                    if let Some(db) = &self.database {
                        let record = DeletionRecord::new(
                            item.path.clone(),
                            item.dir_type.clone(),
                            item.size_bytes,
                            item.project_root.clone(),
                            item.project_name.clone(),
                        );

                        let db_clone = Arc::clone(db);

                        thread::spawn(move || {
                            if let Err(e) = db_clone.record_deletion(&record) {
                                error!("Failed to record deletion in database: {}", e);
                            }
                        });
                    }

                    self.directories.retain(|d| d.path != item.path);
                }
                Err(e) => {
                    error!("Failed to delete {}: {}", item.path.display(), e);
                    errors.push(format!("{}: {}", item.path.display(), e));
                }
            }
        }

        self.deleted_count += success_count;
        self.total_size = self.directories.iter().map(|d| d.size_bytes).sum();
        self.selected_size = 0;

        if !errors.is_empty() {
            warn!("Deletion completed with {} errors", errors.len());
            self.error_message = Some(format!("Failed to delete {} directories", errors.len()));
        } else {
            info!("All deletions completed successfully");
        }

        cx.notify();
    }

    pub fn toggle_selection(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(dir) = self.directories.get_mut(index) {
            dir.selected = !dir.selected;
            self.update_selected_size();
            cx.notify();
        }
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        let show_orphaned_only = self.show_orphaned_only;

        for dir in &mut self.directories {
            let should_show = if show_orphaned_only {
                dir.is_orphaned
            } else {
                true
            };
            if should_show {
                dir.selected = true;
            }
        }
        self.update_selected_size();
        cx.notify();
    }

    pub fn select_none(&mut self, cx: &mut Context<Self>) {
        for dir in &mut self.directories {
            dir.selected = false;
        }
        self.selected_size = 0;
        cx.notify();
    }

    pub fn visible_entries(&self) -> Vec<(usize, &DirectoryItem)> {
        self.directories
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                if self.show_orphaned_only {
                    d.is_orphaned
                } else {
                    true
                }
            })
            .collect()
    }

    fn update_selected_size(&mut self) {
        self.selected_size = self
            .directories
            .iter()
            .filter(|d| d.selected)
            .map(|d| d.size_bytes)
            .sum();
    }

    // -- File browser -------------------------------------------------------

    pub fn open_file_browser(&mut self, cx: &mut Context<Self>) {
        self.browse_path = PathBuf::from(&self.scan_path);
        self.refresh_browse_entries();
        self.show_file_browser = true;
        cx.notify();
    }

    pub fn close_file_browser(&mut self, cx: &mut Context<Self>) {
        self.show_file_browser = false;
        cx.notify();
    }

    pub fn browse_navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.browse_path = path;
        self.refresh_browse_entries();
        cx.notify();
    }

    pub fn browse_select(&mut self, cx: &mut Context<Self>) {
        self.scan_path = self.browse_path.to_string_lossy().to_string();
        self.show_file_browser = false;
        cx.notify();
    }

    fn refresh_browse_entries(&mut self) {
        self.browse_entries.clear();

        // Parent entry
        if let Some(parent) = self.browse_path.parent() {
            self.browse_entries.push(BrowseEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
            });
        }

        // Child directories
        if let Ok(dirs) = utils::list_directories(&self.browse_path) {
            for (name, path) in dirs {
                self.browse_entries.push(BrowseEntry { name, path });
            }
        }
    }
}
