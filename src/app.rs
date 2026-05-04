// file: src/app.rs
// description: GPUI application state model
// reference: https://github.com/zed-industries/zed

use gpui::*;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use artifact::config::{AppConfig, DeleteMode};
use artifact::database::{DeletionDatabase, DeletionRecord};
use artifact::directory_item::DirectoryItem;
use artifact::rules;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusNotice {
    pub kind: NoticeKind,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub path: String,
    #[allow(dead_code)]
    pub dir_type: String,
    pub size_bytes: i64,
    pub deleted_at: i64,
    #[allow(dead_code)]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HistoryRun {
    pub started_at: i64,
    pub entries: Vec<HistoryEntry>,
    pub total_bytes: i64,
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
    config: AppConfig,

    // Scan state
    scan_path: String,
    enabled_rules: HashSet<String>,
    scan_state: ScanState,
    scan_progress_data: Option<ScanProgress>,
    scan_receiver: Option<Arc<Mutex<Receiver<ScanMessage>>>>,
    scan_cancel: Option<Arc<AtomicBool>>,

    // Directory state
    directories: Vec<DirectoryItem>,
    total_size: u64,
    selected_size: u64,

    // Filters
    show_orphaned_only: bool,

    // Results
    deleted_count: usize,
    error_message: Option<String>,
    notice: Option<StatusNotice>,
    notice_set_at: Option<Instant>,
    pending_delete: bool,

    // Database
    database: Option<Arc<DeletionDatabase>>,

    // File browser
    show_file_browser: bool,
    browse_path: PathBuf,
    browse_entries: Vec<BrowseEntry>,

    // Live scan log (capped at 60 entries for the log panel)
    pub scan_log: Vec<String>,
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
    pub fn is_rule_enabled(&self, name: &str) -> bool {
        self.enabled_rules.contains(name)
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
    pub fn notice(&self) -> Option<&StatusNotice> {
        self.notice.as_ref()
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
    pub fn delete_mode(&self) -> DeleteMode {
        self.config.scan.delete_mode
    }

    pub fn scan_elapsed_secs(&self) -> Option<f64> {
        self.scan_progress_data.as_ref().map(|p| p.elapsed_secs)
    }

    pub fn pending_delete(&self) -> bool {
        self.pending_delete
    }

    #[allow(dead_code)]
    pub fn is_scan_cancellable(&self) -> bool {
        self.scan_cancel.is_some()
    }

    pub fn directories_scanned(&self) -> Option<usize> {
        self.scan_progress_data
            .as_ref()
            .map(|p| p.directories_scanned)
    }

    pub fn load_history(&self, limit: usize) -> Result<Vec<HistoryRun>, String> {
        let Some(db) = self.database.as_ref() else {
            return Ok(Vec::new());
        };

        let records = match db.get_recent_deletions(limit.max(1)) {
            Ok(r) => r,
            Err(e) => return Err(e.to_string()),
        };

        if records.is_empty() {
            return Ok(Vec::new());
        }

        // Group records that fall within the same run window. We treat any pair
        // of deletions within RUN_WINDOW_SECS of each other as part of the same
        // run. Records are descending by deleted_at from the DB.
        const RUN_WINDOW_SECS: i64 = 60;
        let mut runs: Vec<HistoryRun> = Vec::new();
        for rec in records {
            let entry = HistoryEntry {
                path: rec.path,
                dir_type: rec.dir_type,
                size_bytes: rec.size_bytes,
                deleted_at: rec.deleted_at,
                project_name: rec.project_name,
            };

            match runs.last_mut() {
                Some(run)
                    if (run.started_at - entry.deleted_at).abs() <= RUN_WINDOW_SECS =>
                {
                    if entry.deleted_at < run.started_at {
                        run.started_at = entry.deleted_at;
                    }
                    run.total_bytes += entry.size_bytes;
                    run.entries.push(entry);
                }
                _ => {
                    runs.push(HistoryRun {
                        started_at: entry.deleted_at,
                        total_bytes: entry.size_bytes,
                        entries: vec![entry],
                    });
                }
            }
        }

        Ok(runs)
    }
}

const NOTICE_TTL: Duration = Duration::from_secs(8);

impl ArtifactApp {
    fn set_notice(&mut self, kind: NoticeKind, title: impl Into<String>, message: impl Into<String>) {
        self.notice = Some(StatusNotice {
            kind,
            title: title.into(),
            message: message.into(),
        });
        self.notice_set_at = Some(Instant::now());
    }

    pub fn dismiss_notice(&mut self, cx: &mut Context<Self>) {
        if self.notice.is_some() {
            self.notice = None;
            self.notice_set_at = None;
            cx.notify();
        }
    }

    pub fn expire_notice_if_stale(&mut self, cx: &mut Context<Self>) {
        if let Some(set_at) = self.notice_set_at {
            if set_at.elapsed() >= NOTICE_TTL {
                self.notice = None;
                self.notice_set_at = None;
                cx.notify();
            }
        }
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

        let enabled_rules = enabled_rules_from_config(&config);
        let show_orphaned_only = config.scan.show_orphaned_only;

        cx.new(|_cx| Self {
            config,
            scan_path: home.clone(),
            enabled_rules,
            scan_state: ScanState::Idle,
            scan_progress_data: None,
            scan_receiver: None,
            scan_cancel: None,
            directories: Vec::new(),
            total_size: 0,
            selected_size: 0,
            show_orphaned_only,
            deleted_count: 0,
            error_message: None,
            notice: None,
            notice_set_at: None,
            pending_delete: false,
            database,
            show_file_browser: false,
            browse_path: home_path,
            browse_entries: Vec::new(),
            scan_log: Vec::new(),
        })
    }

    // -- Scan option toggles ------------------------------------------------

    pub fn toggle_orphaned_only(&mut self, cx: &mut Context<Self>) {
        self.show_orphaned_only = !self.show_orphaned_only;
        self.config.scan.show_orphaned_only = self.show_orphaned_only;
        if let Err(e) = self.config.save() {
            warn!("Failed to persist orphaned filter preference: {}", e);
        }
        cx.notify();
    }

    pub fn set_language_enabled(&mut self, language: &str, enabled: bool, cx: &mut Context<Self>) {
        for rule in rules::RULES.iter().filter(|rule| rule.language == language) {
            if enabled {
                self.enabled_rules.insert(rule.name.to_string());
            } else {
                self.enabled_rules.remove(rule.name);
            }
        }

        self.persist_settings(cx);
        cx.notify();
    }

    pub fn set_delete_mode(&mut self, delete_mode: DeleteMode, cx: &mut Context<Self>) {
        if self.config.scan.delete_mode == delete_mode {
            return;
        }

        self.config.scan.delete_mode = delete_mode;
        self.persist_settings(cx);
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
        self.notice = None;
        self.notice_set_at = None;
        self.scan_progress_data = None;
        self.scan_log.clear();

        let (tx, rx) = channel();
        self.scan_receiver = Some(Arc::new(Mutex::new(rx)));

        let scan_path = self.scan_path.clone();
        let enabled_rules: Vec<String> = self.enabled_rules.iter().cloned().collect();
        let start_time = Instant::now();

        let cancel = Arc::new(AtomicBool::new(false));
        self.scan_cancel = Some(Arc::clone(&cancel));
        let cancel_for_cb = Arc::clone(&cancel);

        thread::spawn(move || {
            let scanner = Scanner::with_enabled(PathBuf::from(&scan_path), enabled_rules);
            let tx_cb = tx.clone();

            match scanner.scan_with_progress(
                cancel,
                move |dirs_scanned, items_found, current_path: &str, total_size| {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    if tx_cb
                        .send(ScanMessage::Progress(ScanProgress {
                            directories_scanned: dirs_scanned,
                            items_found,
                            current_path: current_path.to_string(),
                            total_size_found: total_size,
                            elapsed_secs: elapsed,
                        }))
                        .is_err()
                    {
                        cancel_for_cb.store(true, Ordering::Relaxed);
                    }
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
                    if !progress.current_path.is_empty() {
                        if self.scan_log.len() >= 60 {
                            self.scan_log.remove(0);
                        }
                        self.scan_log.push(progress.current_path.clone());
                    }
                    self.scan_progress_data = Some(progress);
                    cx.notify();
                }
                ScanMessage::Complete(dirs) => {
                    self.scan_cancel = None;
                    self.directories = dirs;
                    self.total_size = self.directories.iter().map(|d| d.size_bytes).sum();
                    self.scan_state = ScanState::Complete;
                    self.scan_progress_data = None;
                    self.scan_receiver = None;
                    self.set_notice(
                        NoticeKind::Success,
                        "SCAN COMPLETE",
                        format!(
                            "Found {} artifacts totaling {}.",
                            format_number(self.directories.len()),
                            utils::format_size(self.total_size)
                        ),
                    );
                    cx.notify();
                }
                ScanMessage::Error(err) => {
                    self.scan_cancel = None;
                    self.error_message = Some(err);
                    self.set_notice(
                        NoticeKind::Error,
                        "SCAN FAILED",
                        self.error_message.clone().unwrap_or_default(),
                    );
                    self.scan_state = ScanState::Idle;
                    self.scan_progress_data = None;
                    self.scan_receiver = None;
                    cx.notify();
                }
            }
        }
    }

    // -- Scan cancellation --------------------------------------------------

    #[allow(dead_code)]
    pub fn cancel_scan(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.scan_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.scan_state = ScanState::Idle;
        self.scan_progress_data = None;
        self.scan_receiver = None;
        cx.notify();
    }

    // -- Selection & deletion -----------------------------------------------

    pub fn request_delete_confirm(&mut self, cx: &mut Context<Self>) {
        if self.selected_size > 0 {
            self.pending_delete = true;
            cx.notify();
        }
    }

    pub fn cancel_delete_confirm(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = false;
        cx.notify();
    }

    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = false;
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
        let delete_mode = self.config.scan.delete_mode;

        for item in to_delete {
            debug!("Deleting directory: {}", item.path.display());

            match utils::remove_directory(&item.path, delete_mode) {
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
                        if let Err(e) = db.record_deletion(&record) {
                            error!("Failed to record deletion in database: {}", e);
                            errors.push(format!("[db write] {}: {}", item.path.display(), e));
                        }
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
            self.set_notice(
                NoticeKind::Error,
                "CLEANUP INCOMPLETE",
                self.error_message.clone().unwrap_or_default(),
            );
        } else {
            info!("All deletions completed successfully");
            self.error_message = None;
            let action_label = match delete_mode {
                DeleteMode::Trash => "Moved selected artifacts to Trash.",
                DeleteMode::Permanent => "Permanently deleted selected artifacts.",
            };
            self.set_notice(
                NoticeKind::Success,
                "CLEANUP COMPLETE",
                format!(
                    "{} {} items processed.",
                    action_label,
                    format_number(success_count)
                ),
            );
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
        match std::fs::symlink_metadata(&path) {
            Err(e) => {
                warn!("browse_navigate: cannot read {}: {}", path.display(), e);
                return;
            }
            Ok(meta) if meta.file_type().is_symlink() => {
                warn!("browse_navigate: refusing symlink {}", path.display());
                return;
            }
            Ok(meta) if !meta.is_dir() => {
                warn!("browse_navigate: not a directory {}", path.display());
                return;
            }
            Ok(_) => {}
        }
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

    fn persist_settings(&mut self, cx: &mut Context<Self>) {
        self.config.scan.enabled_languages = Some(enabled_language_labels(&self.enabled_rules));

        match self.config.save() {
            Ok(()) => {
                self.error_message = None;
                self.set_notice(
                    NoticeKind::Success,
                    "SETTINGS SAVED",
                    "Scan preferences were updated for future runs.",
                );
            }
            Err(err) => {
                self.set_notice(NoticeKind::Error, "SETTINGS NOT SAVED", err.to_string());
                self.error_message = Some("Failed to save settings".to_string());
            }
        }

        cx.notify();
    }
}

fn enabled_rules_from_config(config: &AppConfig) -> HashSet<String> {
    let Some(enabled_languages) = config.scan.enabled_languages.as_ref() else {
        return rules::RULES
            .iter()
            .map(|rule| rule.name.to_string())
            .collect();
    };

    rules::RULES
        .iter()
        .filter(|rule| {
            enabled_languages
                .iter()
                .any(|language| language == rule.language)
        })
        .map(|rule| rule.name.to_string())
        .collect()
}

fn enabled_language_labels(enabled_rules: &HashSet<String>) -> Vec<String> {
    let mut languages = Vec::new();

    for rule in rules::RULES {
        if enabled_rules.contains(rule.name)
            && !languages
                .iter()
                .any(|language: &String| language == rule.language)
        {
            languages.push(rule.language.to_string());
        }
    }

    languages
}

fn format_number(n: usize) -> String {
    if n < 1_000 {
        return n.to_string();
    }

    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
