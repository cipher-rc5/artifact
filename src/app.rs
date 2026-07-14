// file: src/app.rs
// description: GPUI application state model
// reference: https://github.com/zed-industries/zed

use gpui::*;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

use artifact::config::{AppConfig, DeleteMode};
use artifact::database::{DeletionDatabase, DeletionRecord};
use artifact::directory_item::DirectoryItem;
use artifact::history::{self, HistoryEntry as CoreHistoryEntry};
use artifact::rules;
use artifact::scanner::{Scanner, validate_artifact_path};
use artifact::utils;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[non_exhaustive]
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
    /// Total directories pre-counted before the scan; used to compute a real
    /// progress percentage instead of a fixed placeholder.
    pub total_dirs: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct BrowseEntry {
    pub name: String,
    pub path: PathBuf,
}

#[non_exhaustive]
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

// History types now live in the testable lib core (`artifact::history`). These
// re-exports preserve the `crate::app::HistoryRun` / `HistoryEntry` paths the
// view already imports. `HistoryEntry` is re-exported for the view even though
// this binary module does not name it directly.
#[allow(unused_imports)]
pub use artifact::history::{HistoryEntry, HistoryRun};

/// Detailed record of a single failed deletion (review finding M7). Surfaced to
/// the view via [`ArtifactApp::delete_errors`] so the UI can show *what* failed
/// and *why*, not just a count.
// Fields are consumed by the view agent (not yet wired at this point).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DeleteError {
    pub path: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Internal messages
// ---------------------------------------------------------------------------

enum ScanMessage {
    Progress(ScanProgress),
    Complete(Vec<DirectoryItem>),
    Error(String),
}

enum DeleteMessage {
    ItemDeleted(PathBuf),
    ItemFailed { path: String, reason: String },
    Complete { processed: usize, cancelled: bool },
}

enum BrowseMessage {
    Entries {
        path: PathBuf,
        entries: Vec<BrowseEntry>,
    },
    Error(String),
}

// Constructed on background threads and drained by `check_history_progress`,
// which the view agent will wire into its poll loop.
#[allow(dead_code)]
enum HistoryMessage {
    Loaded(Vec<HistoryRun>),
    Error(String),
}

#[derive(Debug, Serialize)]
struct DeletionManifest {
    operation_id: String,
    created_at: i64,
    scan_root: String,
    delete_mode: DeleteMode,
    total_items: usize,
    total_bytes: u64,
    items: Vec<DeletionManifestItem>,
}

#[derive(Debug, Serialize)]
struct DeletionManifestItem {
    path: String,
    rule_name: String,
    size_bytes: u64,
    project_root: Option<String>,
    is_orphaned: bool,
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

    // Async deletion
    is_deleting: bool,
    delete_receiver: Option<Arc<Mutex<Receiver<DeleteMessage>>>>,
    delete_errors: Vec<DeleteError>,
    delete_cancel: Option<Arc<AtomicBool>>,

    // File browser
    show_file_browser: bool,
    browse_path: PathBuf,
    browse_entries: Vec<BrowseEntry>,
    browse_back_stack: Vec<PathBuf>,
    browse_forward_stack: Vec<PathBuf>,
    browse_receiver: Option<Arc<Mutex<Receiver<BrowseMessage>>>>,
    browse_loading: bool,

    // Async history load (drained by view-agent-wired check_history_progress)
    #[allow(dead_code)]
    history_receiver: Option<Arc<Mutex<Receiver<HistoryMessage>>>>,
    #[allow(dead_code)]
    history_loading: bool,

    // Live scan log (capped at 60 entries for the log panel)
    scan_log: VecDeque<String>,
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
    pub fn can_browse_back(&self) -> bool {
        !self.browse_back_stack.is_empty()
    }
    pub fn can_browse_forward(&self) -> bool {
        !self.browse_forward_stack.is_empty()
    }
    pub fn is_deleting(&self) -> bool {
        self.is_deleting
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

    pub fn scan_log(&self) -> &VecDeque<String> {
        &self.scan_log
    }

    pub fn directories_scanned(&self) -> Option<usize> {
        self.scan_progress_data
            .as_ref()
            .map(|p| p.directories_scanned)
    }

    /// Synchronous history load. Kept as a shim for the view until it migrates
    /// to the async [`start_history_load`](Self::start_history_load) /
    /// [`check_history_progress`](Self::check_history_progress) pair (review
    /// finding H4). Runs a DB query on the calling thread.
    // Retained compatibility shim: the view migrated to the async
    // start/check_history_progress pair, so this is no longer called.
    #[allow(dead_code)]
    pub fn load_history(&self, limit: usize) -> Result<Vec<HistoryRun>, String> {
        let Some(db) = self.database.as_ref() else {
            return Ok(Vec::new());
        };

        let records = match db.get_recent_deletions(limit.max(1)) {
            Ok(r) => r,
            Err(e) => return Err(e.to_string()),
        };

        Ok(runs_from_records(records))
    }

    /// Detailed per-item delete failures from the most recent delete operation
    /// (review finding M7). Cleared when a new delete starts.
    // Not yet wired by the view; see "API CHANGES FOR VIEW AGENT".
    #[allow(dead_code)]
    pub fn delete_errors(&self) -> &[DeleteError] {
        &self.delete_errors
    }

    /// Whether a background delete can currently be cancelled by the user
    /// (review finding M8).
    #[allow(dead_code)]
    pub fn can_cancel_delete(&self) -> bool {
        self.is_deleting && self.delete_cancel.is_some()
    }

    /// Whether the file browser is currently loading directory entries on a
    /// background thread (review finding H4).
    #[allow(dead_code)]
    pub fn is_browse_loading(&self) -> bool {
        self.browse_loading
    }

    /// Whether an async history load is in flight (review finding H4).
    #[allow(dead_code)]
    pub fn is_history_loading(&self) -> bool {
        self.history_loading
    }
}

const NOTICE_TTL: Duration = Duration::from_secs(8);

impl ArtifactApp {
    fn set_notice(
        &mut self,
        kind: NoticeKind,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
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
        if let Some(set_at) = self.notice_set_at
            && set_at.elapsed() >= NOTICE_TTL
        {
            self.notice = None;
            self.notice_set_at = None;
            cx.notify();
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
            is_deleting: false,
            delete_receiver: None,
            delete_errors: Vec::new(),
            delete_cancel: None,
            show_file_browser: false,
            browse_path: home_path,
            browse_entries: Vec::new(),
            browse_back_stack: Vec::new(),
            browse_forward_stack: Vec::new(),
            browse_receiver: None,
            browse_loading: false,
            history_receiver: None,
            history_loading: false,
            scan_log: VecDeque::new(),
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

    pub fn reset_scan(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.scan_cancel.take() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.scan_state = ScanState::Idle;
        self.directories.clear();
        self.total_size = 0;
        self.selected_size = 0;
        self.error_message = None;
        self.notice = None;
        self.notice_set_at = None;
        self.scan_progress_data = None;
        self.scan_receiver = None;
        self.scan_log.clear();
        cx.notify();
    }

    pub fn start_scan(&mut self, cx: &mut Context<Self>) {
        info!("Starting scan at path: {}", self.scan_path);

        // Signal-cancel any prior in-flight scan so its thread stops walking the
        // filesystem instead of being orphaned (review finding M8). Dropping the
        // old receiver also means its stale `Complete` message is ignored.
        if let Some(cancel) = self.scan_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.scan_receiver = None;

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

        let max_results = self.config.scan.max_results;
        let cancel = Arc::new(AtomicBool::new(false));
        self.scan_cancel = Some(Arc::clone(&cancel));
        let cancel_for_cb = Arc::clone(&cancel);

        thread::spawn(move || {
            let scanner = Scanner::with_enabled(PathBuf::from(&scan_path), enabled_rules)
                .with_max_results(max_results);

            // Pre-count directories so the UI can show real progress rather
            // than a fixed placeholder. This mirrors the actual walker's
            // filter logic, so the count closely tracks dirs_scanned.
            let total_dirs = scanner.count_directories();
            let total_dirs_opt = if total_dirs > 0 {
                Some(total_dirs)
            } else {
                None
            };

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
                            total_dirs: total_dirs_opt,
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
                            self.scan_log.pop_front();
                        }
                        self.scan_log.push_back(progress.current_path.clone());
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
        if self.is_deleting {
            return;
        }

        let to_delete: Vec<_> = self
            .directories
            .iter()
            .filter(|d| d.selected)
            .cloned()
            .collect();

        if to_delete.is_empty() {
            return;
        }

        let operation_id = operation_id();
        let scan_root = PathBuf::from(&self.scan_path);
        let manifest = DeletionManifest::new(
            operation_id.clone(),
            self.scan_path.clone(),
            self.config.scan.delete_mode,
            &to_delete,
        );
        if let Err(e) = write_deletion_manifest(&self.config, &manifest) {
            self.error_message = Some("Could not write deletion manifest".to_string());
            self.set_notice(
                NoticeKind::Error,
                "CLEANUP BLOCKED",
                format!("Could not write deletion manifest: {}", e),
            );
            cx.notify();
            return;
        }

        info!(
            operation_id = %operation_id,
            "Spawning background deletion for {} directories",
            to_delete.len()
        );

        let delete_mode = self.config.scan.delete_mode;
        let database = self.database.clone();
        let (tx, rx) = channel::<DeleteMessage>();

        // User-invokable cancellation for the in-flight delete (review finding
        // M8). Checked at the top of each iteration; already-deleted items are
        // never un-deleted, but the remaining queue stops.
        let cancel = Arc::new(AtomicBool::new(false));
        self.delete_cancel = Some(Arc::clone(&cancel));

        self.is_deleting = true;
        self.delete_errors.clear();
        self.delete_receiver = Some(Arc::new(Mutex::new(rx)));

        thread::spawn(move || {
            let mut processed = 0;
            let mut cancelled = false;
            for item in to_delete {
                if cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    info!("Delete cancelled by user before completing all items");
                    break;
                }
                debug!("Deleting directory: {}", item.path.display());
                // Validate and obtain the exact canonical path proven to be
                // inside scan_root; that canonical path — not the raw
                // `item.path` — is what gets removed (review finding C1).
                let canonical = match validate_delete_candidate(&scan_root, &item) {
                    Ok(canonical) => canonical,
                    Err(e) => {
                        error!(
                            "Pre-delete validation failed for {}: {}",
                            item.path.display(),
                            e
                        );
                        let _ = tx.send(DeleteMessage::ItemFailed {
                            path: item.path.display().to_string(),
                            reason: e.to_string(),
                        });
                        continue;
                    }
                };
                match utils::remove_directory_checked(&canonical, delete_mode) {
                    Ok(_) => {
                        processed += 1;
                        info!("Deleted: {}", canonical.display());
                        if let Some(db) = &database {
                            let record = DeletionRecord::new(
                                item.path.clone(),
                                item.dir_type,
                                item.size_bytes,
                                item.project_root.clone(),
                                item.project_name.clone(),
                            );
                            if let Err(e) = db.record_deletion(&record) {
                                error!("DB write failed: {}", e);
                            }
                        }
                        let _ = tx.send(DeleteMessage::ItemDeleted(item.path));
                    }
                    Err(e) => {
                        error!("Failed to delete {}: {}", item.path.display(), e);
                        let _ = tx.send(DeleteMessage::ItemFailed {
                            path: item.path.display().to_string(),
                            reason: e.to_string(),
                        });
                    }
                }
            }
            let _ = tx.send(DeleteMessage::Complete {
                processed,
                cancelled,
            });
        });

        cx.notify();
    }

    /// Request cancellation of an in-progress background delete (review finding
    /// M8). Items already removed stay removed; the remaining queue stops. Safe
    /// to call when no delete is running.
    #[allow(dead_code)]
    pub fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.delete_cancel.as_ref() {
            cancel.store(true, Ordering::Relaxed);
            cx.notify();
        }
    }

    pub fn check_delete_progress(&mut self, cx: &mut Context<Self>) {
        let rx = match self.delete_receiver.clone() {
            Some(rx) => rx,
            None => return,
        };

        let rx = rx.lock();
        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }
        drop(rx);

        let mut changed = false;
        for msg in messages {
            match msg {
                DeleteMessage::ItemDeleted(path) => {
                    self.directories.retain(|d| d.path != path);
                    self.deleted_count += 1;
                    changed = true;
                }
                DeleteMessage::ItemFailed { path, reason } => {
                    self.delete_errors.push(DeleteError { path, reason });
                    changed = true;
                }
                DeleteMessage::Complete {
                    processed,
                    cancelled,
                } => {
                    self.is_deleting = false;
                    self.delete_receiver = None;
                    self.delete_cancel = None;
                    self.total_size = self.directories.iter().map(|d| d.size_bytes).sum();
                    self.selected_size = 0;
                    let delete_mode = self.config.scan.delete_mode;
                    if self.delete_errors.is_empty() {
                        self.error_message = None;
                        let action_label = match delete_mode {
                            DeleteMode::Trash => "Moved selected artifacts to Trash.",
                            DeleteMode::Permanent => "Permanently deleted selected artifacts.",
                        };
                        let (title, extra) = if cancelled {
                            ("CLEANUP CANCELLED", " Cancelled before finishing.")
                        } else {
                            ("CLEANUP COMPLETE", "")
                        };
                        self.set_notice(
                            NoticeKind::Success,
                            title,
                            format!(
                                "{} {} items processed.{}",
                                action_label,
                                format_number(processed),
                                extra
                            ),
                        );
                    } else {
                        let n = self.delete_errors.len();
                        self.error_message = Some(format!("Failed to delete {} directories", n));
                        // Reference that per-item detail is available via
                        // `delete_errors()` for the view to render (M7).
                        let cancel_note = if cancelled { " Cancelled." } else { "" };
                        self.set_notice(
                            NoticeKind::Error,
                            "CLEANUP INCOMPLETE",
                            format!(
                                "Failed to delete {} directories; see details below for the \
                                 affected paths and reasons.{}",
                                n, cancel_note
                            ),
                        );
                    }
                    changed = true;
                }
            }
        }

        if changed {
            cx.notify();
        }
    }

    pub fn select_all_visible(&mut self, cx: &mut Context<Self>) {
        for dir in &mut self.directories {
            if !self.show_orphaned_only || dir.is_orphaned {
                dir.selected = true;
            }
        }
        self.update_selected_size();
        cx.notify();
    }

    pub fn deselect_all(&mut self, cx: &mut Context<Self>) {
        for dir in &mut self.directories {
            dir.selected = false;
        }
        self.update_selected_size();
        cx.notify();
    }

    /// Toggle the selection state of the item at the given canonical `path`
    /// (review finding M4). Selection is keyed by path, which is stable across
    /// the `retain(...)` that `check_delete_progress` runs mid-delete — unlike a
    /// vector index, which shifts when earlier items are removed.
    pub fn toggle_selection_by_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Some(dir) = self.directories.iter_mut().find(|d| d.path == path) {
            dir.selected = !dir.selected;
            self.update_selected_size();
            cx.notify();
        }
    }

    /// Set the selection state of the item at `path` explicitly (review finding
    /// M4). No-op if no item matches.
    #[allow(dead_code)]
    pub fn set_selected(&mut self, path: &Path, selected: bool, cx: &mut Context<Self>) {
        if let Some(dir) = self.directories.iter_mut().find(|d| d.path == path)
            && dir.selected != selected
        {
            dir.selected = selected;
            self.update_selected_size();
            cx.notify();
        }
    }

    /// Index-based selection toggle. **Shim** kept so `view.rs` keeps compiling
    /// until it migrates to [`toggle_selection_by_path`](Self::toggle_selection_by_path).
    /// Resolves the index to its path first so the toggle is applied to the
    /// stable path, not the raw index.
    // Retained compatibility shim: the view migrated to
    // `toggle_selection_by_path`, so this index-based entry point is unused.
    #[allow(dead_code)]
    pub fn toggle_selection(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(path) = self.directories.get(index).map(|d| d.path.clone()) {
            self.toggle_selection_by_path(&path, cx);
        }
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
        // Seed the back history with the scan root's ancestor chain so the "<"
        // button can climb up toward the filesystem root on a fresh open. The
        // immediate parent sits on top of the stack (popped first), the root at
        // the bottom; ">" then walks back down toward the scan root.
        self.browse_back_stack = self
            .browse_path
            .ancestors()
            .skip(1)
            .map(Path::to_path_buf)
            .collect();
        self.browse_back_stack.reverse();
        self.browse_forward_stack.clear();
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
        self.browse_back_stack.push(self.browse_path.clone());
        self.browse_forward_stack.clear();
        self.browse_path = path;
        self.refresh_browse_entries();
        cx.notify();
    }

    pub fn browse_back(&mut self, cx: &mut Context<Self>) {
        if let Some(prev) = self.browse_back_stack.pop() {
            self.browse_forward_stack.push(self.browse_path.clone());
            self.browse_path = prev;
            self.refresh_browse_entries();
            cx.notify();
        }
    }

    pub fn browse_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(next) = self.browse_forward_stack.pop() {
            self.browse_back_stack.push(self.browse_path.clone());
            self.browse_path = next;
            self.refresh_browse_entries();
            cx.notify();
        }
    }

    pub fn browse_select(&mut self, cx: &mut Context<Self>) {
        self.scan_path = self.browse_path.to_string_lossy().to_string();
        self.show_file_browser = false;
        cx.notify();
    }

    /// Kick off a background directory listing for the current `browse_path`
    /// (review finding H4). The blocking `read_dir` runs off the UI thread; the
    /// result is delivered via [`check_browse_progress`](Self::check_browse_progress).
    ///
    /// Entries are cleared immediately and `browse_loading` is set so the view
    /// can show a spinner while the listing is in flight. Only the most recent
    /// request's results are applied (results are keyed by path).
    fn refresh_browse_entries(&mut self) {
        self.browse_entries.clear();
        self.browse_loading = true;

        let path = self.browse_path.clone();
        let (tx, rx) = channel::<BrowseMessage>();
        self.browse_receiver = Some(Arc::new(Mutex::new(rx)));

        thread::spawn(move || {
            let mut entries = Vec::new();

            // Parent entry.
            if let Some(parent) = path.parent() {
                entries.push(BrowseEntry {
                    name: "..".to_string(),
                    path: parent.to_path_buf(),
                });
            }

            match utils::list_directories(&path) {
                Ok(dirs) => {
                    for (name, child) in dirs {
                        entries.push(BrowseEntry { name, path: child });
                    }
                    let _ = tx.send(BrowseMessage::Entries { path, entries });
                }
                Err(e) => {
                    let _ = tx.send(BrowseMessage::Error(format!(
                        "cannot read {}: {}",
                        path.display(),
                        e
                    )));
                }
            }
        });
    }

    /// Drain any pending background directory-listing results and apply them
    /// (review finding H4). Mirror of `check_scan_progress`; the view should
    /// call this from its poll/observe loop.
    #[allow(dead_code)]
    pub fn check_browse_progress(&mut self, cx: &mut Context<Self>) {
        let rx = match self.browse_receiver.clone() {
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
                BrowseMessage::Entries { path, entries } => {
                    // Ignore stale results from a superseded navigation.
                    if path == self.browse_path {
                        self.browse_entries = entries;
                        self.browse_loading = false;
                        self.browse_receiver = None;
                        cx.notify();
                    }
                }
                BrowseMessage::Error(err) => {
                    warn!("browse listing failed: {}", err);
                    self.browse_loading = false;
                    self.browse_receiver = None;
                    cx.notify();
                }
            }
        }
    }

    // -- Async history load -------------------------------------------------

    /// Start loading recent deletion history on a background thread (review
    /// finding H4). Results arrive via
    /// [`check_history_progress`](Self::check_history_progress). Prefer this over
    /// the synchronous [`load_history`](Self::load_history), which runs the DB
    /// query on the calling thread.
    #[allow(dead_code)]
    pub fn start_history_load(&mut self, limit: usize, cx: &mut Context<Self>) {
        let Some(db) = self.database.clone() else {
            // No DB: deliver an empty result synchronously-ish via channel.
            let (tx, rx) = channel::<HistoryMessage>();
            let _ = tx.send(HistoryMessage::Loaded(Vec::new()));
            self.history_receiver = Some(Arc::new(Mutex::new(rx)));
            self.history_loading = true;
            cx.notify();
            return;
        };

        let (tx, rx) = channel::<HistoryMessage>();
        self.history_receiver = Some(Arc::new(Mutex::new(rx)));
        self.history_loading = true;

        thread::spawn(move || match db.get_recent_deletions(limit.max(1)) {
            Ok(records) => {
                let _ = tx.send(HistoryMessage::Loaded(runs_from_records(records)));
            }
            Err(e) => {
                let _ = tx.send(HistoryMessage::Error(e.to_string()));
            }
        });

        cx.notify();
    }

    /// Drain a completed async history load, if any. Returns:
    /// - `Some(Ok(runs))` when a load just completed successfully,
    /// - `Some(Err(msg))` when it failed,
    /// - `None` when nothing is ready yet.
    ///
    /// (review finding H4)
    #[allow(dead_code)]
    pub fn check_history_progress(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Result<Vec<HistoryRun>, String>> {
        let rx = self.history_receiver.clone()?;
        let rx = rx.lock();
        let msg = rx.try_recv().ok();
        drop(rx);

        match msg {
            Some(HistoryMessage::Loaded(runs)) => {
                self.history_loading = false;
                self.history_receiver = None;
                cx.notify();
                Some(Ok(runs))
            }
            Some(HistoryMessage::Error(e)) => {
                self.history_loading = false;
                self.history_receiver = None;
                cx.notify();
                Some(Err(e))
            }
            None => None,
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

impl DeletionManifest {
    fn new(
        operation_id: String,
        scan_root: String,
        delete_mode: DeleteMode,
        items: &[DirectoryItem],
    ) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        Self {
            operation_id,
            created_at,
            scan_root,
            delete_mode,
            total_items: items.len(),
            total_bytes: items.iter().map(|item| item.size_bytes).sum(),
            items: items
                .iter()
                .map(|item| DeletionManifestItem {
                    path: item.path.display().to_string(),
                    rule_name: item.dir_type.name().to_string(),
                    size_bytes: item.size_bytes,
                    project_root: item.project_root.as_ref().map(|p| p.display().to_string()),
                    is_orphaned: item.is_orphaned,
                })
                .collect(),
        }
    }
}

fn operation_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("delete-{nanos}")
}

fn write_deletion_manifest(config: &AppConfig, manifest: &DeletionManifest) -> anyhow::Result<()> {
    let dir = config.get_db_path().join("deletion-manifests");
    std::fs::create_dir_all(&dir)?;
    let content = toml::to_string_pretty(manifest)?;
    std::fs::write(dir.join(format!("{}.toml", manifest.operation_id)), content)?;

    // Prune old manifests so the directory can't grow unbounded (review finding
    // M6). Best-effort: a prune failure must not fail the delete.
    if let Err(e) = prune_deletion_manifests(&dir, history::MANIFEST_RETENTION) {
        warn!("failed to prune old deletion manifests: {}", e);
    }
    Ok(())
}

/// Delete all but the most recent `keep` deletion manifests in `dir`. The
/// decision of which files to prune is the pure, tested
/// [`history::manifests_to_prune`]; this wrapper only performs the filesystem
/// reads/removals.
fn prune_deletion_manifests(dir: &Path, keep: usize) -> anyhow::Result<()> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }

    for name in history::manifests_to_prune(names, keep) {
        let path = dir.join(&name);
        if let Err(e) = std::fs::remove_file(&path) {
            warn!("failed to remove old manifest {}: {}", path.display(), e);
        }
    }
    Ok(())
}

/// Validate a delete candidate and return the exact **canonical** path proven to
/// be inside `scan_root` (review finding C1). Callers must delete this returned
/// canonical path — not `item.path` — so the containment guarantee applies to
/// the path actually removed.
fn validate_delete_candidate(scan_root: &Path, item: &DirectoryItem) -> anyhow::Result<PathBuf> {
    let canonical_root = scan_root
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("scan root is no longer accessible: {e}"))?;
    let canonical_path = item
        .path
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("path is no longer accessible: {e}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        anyhow::bail!("path is outside the scanned root: {}", item.path.display());
    }
    validate_artifact_path(&canonical_path, item.dir_type.name(), item.is_orphaned)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(canonical_path)
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

// Thin wrapper over the canonical implementation in the testable lib core
// (review findings H3/L3). Kept as a private helper only to avoid churning the
// many in-file call sites; the canonical public path is
// `artifact::utils::format_number`.
fn format_number(n: usize) -> String {
    utils::format_number(n)
}

// Map DB records into the pure history-core representation and group them into
// runs (review finding H3). The grouping logic lives in `artifact::history`
// where it is unit-tested; here we only adapt `DeletionRecord` fields.
fn runs_from_records(records: Vec<artifact::database::DeletionRecord>) -> Vec<HistoryRun> {
    history::group_into_runs(records.into_iter().map(|rec| CoreHistoryEntry {
        path: rec.path,
        size_bytes: rec.size_bytes,
        deleted_at: rec.deleted_at,
    }))
}
