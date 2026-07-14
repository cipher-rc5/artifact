// file: src/scanner.rs
// description: Parallel filesystem scanner driven by the rule registry.
//
// Performance design:
//   - jwalk fans out the outer traversal across rayon workers.
//   - When a rule matches, we record the hit and tell jwalk to stop descending
//     into that directory — the heavy interior (e.g. node_modules) is walked
//     exactly once, by the sizing pass, instead of twice.
//   - Sizing each match is itself a parallel jwalk; metadata is the cached
//     value from jwalk's DirEntry (no extra stat() per file).
//   - Progress events are throttled: at most one update per 50 ms in the hot
//     loop, plus one event when each artifact is added.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use jwalk::WalkDirGeneric;
use parking_lot::Mutex;
use tracing::{debug, info};

use crate::directory_item::{DirectoryItem, DirectoryType};
use crate::error::{ArtifactError, Result};
use crate::rules::{self, ArtifactRule};

/// How often the scanner emits a "still working" progress event during the
/// outer traversal. Item-discovery events bypass this throttle.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// When `max_results` is set, we still collect more candidates than the cap so
/// that the largest-first top-N picked *after* sizing is accurate even if a few
/// large artifacts are discovered late in the traversal. We keep up to
/// `COLLECTION_SLACK * max_results` candidates before we start refusing new ones.
///
/// Tradeoff: candidate collection happens before sizing, so we cannot rank by
/// on-disk size while collecting. Once the slack buffer is full we drop *newly
/// discovered* candidates (traversal order), which on a pathological tree could
/// in principle discard a large artifact found very late. The slack makes this
/// vanishingly unlikely in practice while keeping memory bounded — the previous
/// behaviour was to collect and size *every* match with no bound at all.
const COLLECTION_SLACK: usize = 16;

/// Soft cap on `marker_cache` size. The cache keys on `(parent, marker)`; the
/// distinct-parent count is bounded by the number of directories that contain a
/// rule-named child, which is small in practice. We still cap it so a
/// pathological tree cannot grow it without bound: once full we stop inserting
/// (lookups continue to hit existing entries).
const MARKER_CACHE_CAP: usize = 100_000;

/// A filesystem scanner that walks a root directory, applies the rule registry
/// to detect artifact directories (e.g. `node_modules`, `target`), and returns
/// them sorted by on-disk size (largest first).
pub struct Scanner {
    root: PathBuf,
    enabled_rules: Vec<&'static ArtifactRule>,
    max_results: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuleMatch {
    rule: &'static ArtifactRule,
    is_orphaned: bool,
}

impl Scanner {
    /// Create a new `Scanner` rooted at `root` with all built-in rules enabled.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use artifact::scanner::Scanner;
    ///
    /// let scanner = Scanner::new(PathBuf::from("/home/user"));
    /// let results = scanner.scan().unwrap();
    /// ```
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            enabled_rules: rules::RULES.iter().collect(),
            max_results: None,
        }
    }

    /// Cap the number of matches returned.
    ///
    /// The traversal still runs to completion (so `dirs_scanned` and the progress
    /// denominator stay meaningful), but the set of *candidate* matches is bounded
    /// during collection to `COLLECTION_SLACK * limit` entries, then reduced to the
    /// largest-first top-N after sizing. This keeps memory bounded on pathological
    /// trees (tens of thousands of `node_modules`) instead of collecting and sizing
    /// every match and truncating only at the end. See [`COLLECTION_SLACK`].
    pub fn with_max_results(mut self, limit: usize) -> Self {
        self.max_results = Some(limit);
        self
    }

    /// Build a scanner restricted to a specific set of rule names. Unknown
    /// names are silently skipped.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use artifact::scanner::Scanner;
    ///
    /// let scanner = Scanner::with_enabled(PathBuf::from("/home/user"), ["node_modules"]);
    /// let results = scanner.scan().unwrap();
    /// ```
    pub fn with_enabled<I, S>(root: PathBuf, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let allow: HashSet<String> = names.into_iter().map(|s| s.as_ref().to_string()).collect();
        let enabled_rules = rules::RULES
            .iter()
            .filter(|r| allow.contains(r.name))
            .collect();
        Self {
            root,
            enabled_rules,
            max_results: None,
        }
    }

    /// Run a synchronous scan returning all detected artifact directories.
    ///
    /// This is a convenience wrapper around [`Scanner::scan_with_progress`] that
    /// provides no progress feedback and no cancellation support.
    pub fn scan(&self) -> Result<Vec<DirectoryItem>> {
        use std::sync::atomic::AtomicBool;
        let cancel = Arc::new(AtomicBool::new(false));
        self.scan_with_progress(cancel, |_, _, _, _| {})
    }

    /// Scan with a cancellation flag and a progress callback.
    ///
    /// The scan checks `cancel` after each directory entry is processed. When
    /// `cancel` is set to `true` the scan stops at the next opportunity.
    ///
    /// `on_progress(dirs_scanned, items_found, current_path, total_size_found)`
    /// is invoked from the scanner thread. Keep the closure cheap.
    pub fn scan_with_progress(
        &self,
        cancel: Arc<std::sync::atomic::AtomicBool>,
        on_progress: impl Fn(usize, usize, &str, u64) + Send + Sync,
    ) -> Result<Vec<DirectoryItem>> {
        info!("Scanning from root: {}", self.root.display());

        if !self.root.exists() {
            return Err(ArtifactError::Scan(format!(
                "Path does not exist: {}",
                self.root.display()
            )));
        }
        if self.enabled_rules.is_empty() {
            info!("No rules enabled; returning empty result");
            return Ok(Vec::new());
        }

        let dirs_scanned = Arc::new(AtomicUsize::new(0));
        let total_size_found = AtomicU64::new(0);

        // Collected matches, one entry per detected artifact root. A Mutex is
        // fine here: jwalk only contends on it when a rule actually matches,
        // which is rare relative to the number of directories visited.
        let matches: Arc<Mutex<Vec<(PathBuf, RuleMatch)>>> = Arc::new(Mutex::new(Vec::new()));

        let last_progress = Arc::new(Mutex::new(Instant::now()));
        let on_progress = Arc::new(on_progress);
        let max_results = self.max_results;
        // Bound candidate collection so a home directory full of `node_modules`
        // cannot grow `matches` without limit. `None` = unbounded (no cap set).
        let collection_cap = max_results.map(|n| n.saturating_mul(COLLECTION_SLACK).max(1));

        let walker = self.build_walker(matches.clone(), dirs_scanned.clone(), collection_cap);

        'outer: for entry in walker {
            // Honour the cancellation flag.
            if cancel.load(Ordering::Relaxed) {
                debug!("Scan cancelled by caller");
                break 'outer;
            }

            match entry {
                Ok(de) => {
                    if !de.file_type.is_dir() {
                        continue;
                    }
                    let count = dirs_scanned.load(Ordering::Relaxed);
                    let mut last = last_progress.lock();
                    if last.elapsed() >= PROGRESS_INTERVAL {
                        *last = Instant::now();
                        drop(last);
                        let path = de.path();
                        let path_str = path.display().to_string();
                        on_progress(
                            count,
                            matches.lock().len(),
                            &path_str,
                            total_size_found.load(Ordering::Relaxed),
                        );
                    }
                }
                Err(e) => debug!("Skipping inaccessible entry: {e}"),
            }
        }

        // Size each match in parallel.
        let raw_matches: Vec<(PathBuf, RuleMatch)> = {
            let mut guard = matches.lock();
            std::mem::take(&mut *guard)
        };
        info!(
            "Discovered {} candidate directories; sizing",
            raw_matches.len()
        );

        let mut results: Vec<DirectoryItem> = Vec::with_capacity(raw_matches.len());
        let final_dirs = dirs_scanned.load(Ordering::Relaxed);

        for (path, rule_match) in raw_matches {
            // Honour cancellation during the (potentially long) sizing pass so a
            // user who cancels does not wait for every artifact to be sized (H7).
            if cancel.load(Ordering::Relaxed) {
                debug!("Scan cancelled during sizing pass");
                break;
            }

            on_progress(
                final_dirs,
                results.len(),
                &format!("Sizing: {}", path.display()),
                total_size_found.load(Ordering::Relaxed),
            );

            if let Some(item) = build_item(path, rule_match) {
                total_size_found.fetch_add(item.size_bytes, Ordering::Relaxed);
                let path_str = item.path.display().to_string();
                results.push(item);
                on_progress(
                    final_dirs,
                    results.len(),
                    &path_str,
                    total_size_found.load(Ordering::Relaxed),
                );
            }
        }

        on_progress(
            final_dirs,
            results.len(),
            "",
            total_size_found.load(Ordering::Relaxed),
        );

        results.sort_by_key(|b| Reverse(b.size_bytes));
        if let Some(limit) = max_results {
            results.truncate(limit);
        }
        info!(
            "Scan complete: found {} directories ({} dirs visited)",
            results.len(),
            final_dirs
        );
        Ok(results)
    }

    /// Do a fast pre-pass to count how many directories the main scan will
    /// visit. Applies the same retention/pruning logic as `build_walker` so
    /// the count closely matches `dirs_scanned` reported during the real scan.
    pub fn count_directories(&self) -> usize {
        let count = Arc::new(AtomicUsize::new(0));
        let enabled = self.enabled_rules.clone();
        let count_clone = Arc::clone(&count);
        let marker_cache = Arc::new(Mutex::new(HashMap::new()));

        let walker = WalkDirGeneric::<((), ())>::new(&self.root)
            .follow_links(false)
            .skip_hidden(false)
            .process_read_dir(move |_depth, parent_path, _state, children| {
                count_clone.fetch_add(1, Ordering::Relaxed);

                // Share the exact retain + rule-match logic used by the real
                // scan so this count cannot drift from `dirs_scanned` and the
                // progress denominator stays honest (M3).
                retain_scannable(children, &enabled);
                for child in children.iter_mut() {
                    let Ok(entry) = child else { continue };
                    if entry.file_type.is_dir()
                        && match_child(entry, parent_path, &enabled, marker_cache.as_ref())
                            .is_some()
                    {
                        // Matched artifacts are not descended into by the real
                        // scan, so we must prune here too or the count would
                        // include their (never-visited) interiors.
                        entry.read_children_path = None;
                    }
                }
            });

        for _ in walker {}
        count.load(Ordering::Relaxed)
    }

    fn build_walker(
        &self,
        matches: Arc<Mutex<Vec<(PathBuf, RuleMatch)>>>,
        dirs_scanned: Arc<AtomicUsize>,
        collection_cap: Option<usize>,
    ) -> WalkDirGeneric<((), ())> {
        let enabled = self.enabled_rules.clone();
        let marker_cache = Arc::new(Mutex::new(HashMap::new()));

        WalkDirGeneric::<((), ())>::new(&self.root)
            .follow_links(false)
            .skip_hidden(false)
            .process_read_dir(move |_depth, parent_path, _state, children| {
                // Account for the directory we're entering. process_read_dir is
                // called once per directory whose contents will be enumerated.
                dirs_scanned.fetch_add(1, Ordering::Relaxed);

                // Drop entries we never want to descend into (system bundles,
                // hidden roots that aren't in the rule registry). Shared with
                // `count_directories` so the two cannot drift.
                retain_scannable(children, &enabled);

                // Match enabled rules; on a hit, record the match and prune.
                for child in children.iter_mut() {
                    let Ok(entry) = child else { continue };
                    if !entry.file_type.is_dir() {
                        continue;
                    }
                    if let Some(rule_match) =
                        match_child(entry, parent_path, &enabled, marker_cache.as_ref())
                    {
                        // Bound candidate collection (H6). Once the slack buffer
                        // is full we stop recording new matches, but we still
                        // prune the subtree so the traversal cost stays the same.
                        {
                            let mut guard = matches.lock();
                            if collection_cap.is_none_or(|cap| guard.len() < cap) {
                                guard.push((entry.path(), rule_match));
                            }
                        }
                        // Don't walk into matched artifacts during the outer
                        // traversal — sizing handles their interior.
                        entry.read_children_path = None;
                    }
                }
            })
    }
}

/// jwalk's per-directory child list, as handed to `process_read_dir`.
type WalkChildren = Vec<jwalk::Result<jwalk::DirEntry<((), ())>>>;

/// Shared retain filter used by both `build_walker` and `count_directories`.
/// Drops hidden directories that aren't themselves rules, and platform-specific
/// hazardous/system directories we must never descend into (M1, M3).
fn retain_scannable(children: &mut WalkChildren, enabled: &[&'static ArtifactRule]) {
    children.retain(|child| {
        let Ok(child) = child else { return true };
        let name = child.file_name().to_string_lossy();
        if name.starts_with('.') {
            // Allow `.next`/`.venv`/`.gradle`/etc — they're rules.
            return enabled.iter().any(|r| r.dir_name == name.as_ref());
        }
        !is_excluded_dir(name.as_ref())
    });
}

/// Whether a (non-hidden) directory basename names a platform system location
/// the scanner must never descend into. Platform-gated so macOS bundle names
/// don't clobber legitimate Linux directories and vice versa (M1).
fn is_excluded_dir(name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        if matches!(name, "Library" | "Applications" | "System") {
            return true;
        }
    }
    #[cfg(windows)]
    {
        if matches!(
            name,
            "Windows" | "Program Files" | "Program Files (x86)" | "$Recycle.Bin"
        ) {
            return true;
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Virtual/pseudo filesystems: walking them is pointless and can hang.
        // They only ever appear at the root, but a basename match is harmless.
        if matches!(name, "proc" | "sys" | "dev") {
            return true;
        }
    }
    let _ = name;
    false
}

/// Match a single directory entry against the enabled rules, returning the
/// `RuleMatch` on a hit. Shared by `build_walker` and `count_directories` so the
/// count and the real scan agree on what constitutes a match (M3).
fn match_child(
    entry: &jwalk::DirEntry<((), ())>,
    parent_path: &Path,
    enabled: &[&'static ArtifactRule],
    marker_cache: &Mutex<HashMap<(PathBuf, &'static str), bool>>,
) -> Option<RuleMatch> {
    let name = entry.file_name().to_string_lossy();
    enabled.iter().find_map(|rule| {
        if rule.dir_name != name.as_ref() {
            return None;
        }
        match_rule(parent_path, rule, marker_cache)
    })
}

pub fn validate_artifact_path(
    path: &Path,
    expected_rule_name: &str,
    expected_orphaned: bool,
) -> Result<()> {
    if !path.exists() {
        return Err(ArtifactError::Scan(format!(
            "Path no longer exists: {}",
            path.display()
        )));
    }
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(ArtifactError::Scan(format!(
            "Refusing symlink path: {}",
            path.display()
        )));
    }
    if !meta.is_dir() {
        return Err(ArtifactError::Scan(format!(
            "Path is no longer a directory: {}",
            path.display()
        )));
    }

    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Err(ArtifactError::Path(format!(
            "Path has no directory name: {}",
            path.display()
        )));
    };
    let Some(parent) = path.parent() else {
        return Err(ArtifactError::Path(format!(
            "Path has no parent: {}",
            path.display()
        )));
    };
    let Some(rule) = rules::find(expected_rule_name) else {
        return Err(ArtifactError::Scan(format!(
            "Unknown artifact rule: {}",
            expected_rule_name
        )));
    };
    if rule.dir_name != name {
        return Err(ArtifactError::Scan(format!(
            "Directory name no longer matches rule {}: {}",
            expected_rule_name,
            path.display()
        )));
    }

    let cache = Mutex::new(HashMap::new());
    let Some(rule_match) = match_rule(parent, rule, &cache) else {
        return Err(ArtifactError::Scan(format!(
            "Path no longer satisfies rule {}: {}",
            expected_rule_name,
            path.display()
        )));
    };
    if rule_match.is_orphaned != expected_orphaned {
        return Err(ArtifactError::Scan(format!(
            "Path orphan status changed before delete: {}",
            path.display()
        )));
    }
    Ok(())
}

fn match_rule(
    parent_path: &Path,
    rule: &'static ArtifactRule,
    marker_cache: &Mutex<HashMap<(PathBuf, &'static str), bool>>,
) -> Option<RuleMatch> {
    if rule.markers.is_empty() {
        return Some(RuleMatch {
            rule,
            is_orphaned: false,
        });
    }
    let has_any_marker = rule
        .markers
        .iter()
        .any(|m| has_marker_cached(parent_path, m, marker_cache));
    if has_any_marker {
        return Some(RuleMatch {
            rule,
            is_orphaned: false,
        });
    }
    if rule.allow_orphan_without_marker {
        return Some(RuleMatch {
            rule,
            is_orphaned: true,
        });
    }
    None
}

fn has_marker_cached(
    parent: &Path,
    marker: &'static str,
    marker_cache: &Mutex<HashMap<(PathBuf, &'static str), bool>>,
) -> bool {
    // Probe with a borrowed key so a cache hit allocates no `PathBuf` (M11). We
    // only build an owned key when we actually need to insert on a miss.
    {
        let cache = marker_cache.lock();
        if let Some(&value) = cache.get(&(parent, marker) as &dyn MarkerKey) {
            return value;
        }
    }
    // Miss: compute outside the lock (a filesystem read), then insert once.
    let value = has_marker(parent, marker);
    let mut cache = marker_cache.lock();
    if cache.len() < MARKER_CACHE_CAP {
        cache.insert((parent.to_path_buf(), marker), value);
    }
    value
}

/// Borrow shim letting us look up an owned `(PathBuf, &str)` map key with a
/// borrowed `(&Path, &str)` probe, so cache hits don't allocate a `PathBuf`.
/// This is the standard `Borrow<dyn Trait>` pattern for composite keys.
trait MarkerKey {
    fn key(&self) -> (&Path, &'static str);
}
impl MarkerKey for (PathBuf, &'static str) {
    fn key(&self) -> (&Path, &'static str) {
        (self.0.as_path(), self.1)
    }
}
impl MarkerKey for (&Path, &'static str) {
    fn key(&self) -> (&Path, &'static str) {
        (self.0, self.1)
    }
}
impl<'a> std::borrow::Borrow<dyn MarkerKey + 'a> for (PathBuf, &'static str) {
    fn borrow(&self) -> &(dyn MarkerKey + 'a) {
        self
    }
}
impl PartialEq for dyn MarkerKey + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}
impl Eq for dyn MarkerKey + '_ {}
impl std::hash::Hash for dyn MarkerKey + '_ {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

/// Test whether `parent` contains a sibling matching `marker`. Marker tokens
/// starting with '.' are treated as file extensions — the parent directory is
/// scanned for any file with that extension.
fn has_marker(parent: &Path, marker: &str) -> bool {
    if let Some(ext) = marker.strip_prefix('.').filter(|s| !s.contains('/')) {
        // Build the `.ext` suffix once, not once per directory entry (L2).
        let needle = format!(".{ext}");
        if let Ok(rd) = std::fs::read_dir(parent) {
            for entry in rd.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && name.ends_with(&needle)
                {
                    return true;
                }
            }
        }
        return false;
    }
    parent.join(marker).exists()
}

fn build_item(path: PathBuf, rule_match: RuleMatch) -> Option<DirectoryItem> {
    let size = parallel_dir_size(&path);
    let last_modified = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok());

    let project_root = path.parent().map(|p| p.to_path_buf());
    let project_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string());

    Some(DirectoryItem::new(
        path,
        DirectoryType::new(rule_match.rule),
        size,
        last_modified,
        project_root,
        project_name,
        rule_match.is_orphaned,
    ))
}

/// Sum the on-disk size of every regular file under `path`.
///
/// On Unix this reports *on-disk* usage (`st_blocks * 512`), not logical file
/// length, and deduplicates hardlinks by `(st_dev, st_ino)` so a file with N
/// links is counted once. This makes the reported reclaimable size match what
/// `df` shows after deletion — the previous `meta.len()` sum over-counted
/// hardlinked package caches (pnpm/npm/cargo) and ignored block rounding /
/// sparse files (C2). On non-Unix we fall back to logical length.
fn parallel_dir_size(path: &Path) -> u64 {
    let total = AtomicU64::new(0);

    #[cfg(unix)]
    let seen_inodes: Mutex<HashSet<(u64, u64)>> = Mutex::new(HashSet::new());

    for de in WalkDirGeneric::<((), ())>::new(path)
        .follow_links(false)
        .skip_hidden(false)
        .into_iter()
        .flatten()
    {
        if de.file_type.is_file()
            && let Ok(meta) = de.metadata()
        {
            #[cfg(unix)]
            let contribution = file_disk_size(&meta, &seen_inodes);
            #[cfg(not(unix))]
            let contribution = file_disk_size(&meta);
            total.fetch_add(contribution, Ordering::Relaxed);
        }
    }
    total.load(Ordering::Relaxed)
}

/// On-disk size contribution of a single regular file, deduplicating hardlinks.
#[cfg(unix)]
fn file_disk_size(meta: &std::fs::Metadata, seen_inodes: &Mutex<HashSet<(u64, u64)>>) -> u64 {
    use std::os::unix::fs::MetadataExt;
    // Only count each inode once. A hardlinked file appears under multiple
    // paths; the first occurrence pays for it, later links contribute 0.
    if meta.nlink() > 1 && !seen_inodes.lock().insert((meta.dev(), meta.ino())) {
        return 0;
    }
    // `blocks()` is in 512-byte units by POSIX convention, independent of the
    // filesystem's logical block size. This reflects real allocation, including
    // block rounding and sparse-file holes.
    meta.blocks().saturating_mul(512)
}

/// On non-Unix platforms we lack block counts / inode identity, so fall back to
/// logical file length.
#[cfg(not(unix))]
fn file_disk_size(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Create a non-hidden scan root inside a tempdir.
    ///
    /// `tempfile::tempdir()` creates directories whose name starts with `.tmp`,
    /// which the scanner's hidden-dir filter removes when jwalk's
    /// `process_read_dir` is called on the parent. We work around this by
    /// creating an explicit, non-hidden subdirectory ("workspace") inside the
    /// tempdir and scanning from there.
    fn scan_root(tmp: &tempfile::TempDir) -> std::path::PathBuf {
        let root = tmp.path().join("workspace");
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn setup_node_project(base: &std::path::Path) {
        // myproject/package.json
        // myproject/node_modules/some_pkg/index.js
        let project = base.join("myproject");
        fs::create_dir_all(project.join("node_modules").join("some_pkg")).unwrap();
        fs::write(project.join("package.json"), b"{}").unwrap();
        fs::write(
            project
                .join("node_modules")
                .join("some_pkg")
                .join("index.js"),
            b"module.exports = {};",
        )
        .unwrap();
    }

    fn setup_rust_project(base: &std::path::Path) {
        let project = base.join("rustproject");
        fs::create_dir_all(project.join("target").join("debug")).unwrap();
        fs::write(project.join("Cargo.toml"), b"[package]\nname = \"test\"").unwrap();
        fs::write(
            project.join("target").join("debug").join("binary"),
            b"\x7fELF",
        )
        .unwrap();
    }

    #[test]
    fn scan_finds_node_modules() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        setup_node_project(&root);

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        let found = results
            .iter()
            .any(|item| item.path.ends_with("node_modules"));
        assert!(
            found,
            "expected node_modules to be detected; got: {results:?}"
        );
    }

    #[test]
    fn scan_finds_rust_target() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        setup_rust_project(&root);

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        let found = results.iter().any(|item| item.path.ends_with("target"));
        assert!(
            found,
            "expected Rust target/ to be detected; got: {results:?}"
        );
    }

    #[test]
    fn scan_does_not_match_without_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        // Create a directory named "target" but with no Cargo.toml sibling
        let dir = root.join("project_no_marker");
        fs::create_dir_all(dir.join("target").join("debug")).unwrap();
        // NO Cargo.toml

        let scanner = Scanner::with_enabled(root, ["rust_target"]);
        let results = scanner.scan().unwrap();
        assert!(
            results.is_empty(),
            "should not match target/ without Cargo.toml; got: {results:?}"
        );
    }

    #[test]
    fn cancel_flag_stops_scan_early() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        // Create many subdirectories to give the scanner something to traverse.
        for i in 0..50 {
            fs::create_dir_all(root.join(format!("dir_{i:03}"))).unwrap();
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);

        let scanner = Scanner::new(root);
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        // Cancel immediately on first progress callback.
        let result = scanner.scan_with_progress(cancel, move |_, _, _, _| {
            if call_count_clone.fetch_add(1, Ordering::Relaxed) == 0 {
                cancel_clone.store(true, Ordering::Relaxed);
            }
        });

        // Scan should complete (not panic) even when cancelled.
        assert!(result.is_ok());
    }

    #[test]
    fn max_results_cap_is_respected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        // Create 5 separate node projects.
        for i in 0..5 {
            let project = root.join(format!("proj{i}"));
            fs::create_dir_all(project.join("node_modules")).unwrap();
            fs::write(project.join("package.json"), b"{}").unwrap();
        }

        let scanner = Scanner::new(root).with_max_results(2);
        let results = scanner.scan().unwrap();
        assert!(
            results.len() <= 2,
            "expected at most 2 results, got {}",
            results.len()
        );
    }

    #[test]
    fn orphan_detection_marks_node_modules_without_marker_orphaned() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        let project = root.join("orphan_project");
        fs::create_dir_all(project.join("node_modules")).unwrap();

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].is_orphaned);
    }

    #[test]
    fn generic_target_without_marker_is_not_orphan_matched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        fs::create_dir_all(root.join("not_rust").join("target")).unwrap();

        let scanner = Scanner::with_enabled(root, ["rust_target"]);
        let results = scanner.scan().unwrap();
        assert!(results.is_empty(), "generic target should remain excluded");
    }

    #[test]
    fn has_marker_extension_based() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();

        // No files yet → should return false
        assert!(!has_marker(parent, ".csproj"));

        // Create a .csproj file → should return true
        fs::write(parent.join("MyApp.csproj"), b"<Project/>").unwrap();
        assert!(has_marker(parent, ".csproj"));

        // Different extension → still false
        assert!(!has_marker(parent, ".fsproj"));
    }

    #[test]
    fn has_marker_plain_filename() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();

        // Cargo.toml doesn't exist yet
        assert!(!has_marker(parent, "Cargo.toml"));

        // Create it
        fs::write(parent.join("Cargo.toml"), b"[package]").unwrap();
        assert!(has_marker(parent, "Cargo.toml"));
    }

    #[test]
    fn max_results_keeps_largest_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        // Sizes must be separated by many filesystem blocks so that on-disk
        // (block-based) sizing ranks them deterministically — sub-block
        // differences round to the same allocation and make top-N ambiguous.
        for (name, size) in [
            ("small", 64 * 1024),
            ("large", 1024 * 1024),
            ("medium", 256 * 1024),
        ] {
            let project = root.join(name);
            fs::create_dir_all(project.join("node_modules")).unwrap();
            fs::write(project.join("package.json"), b"{}").unwrap();
            fs::write(project.join("node_modules").join("blob"), vec![b'x'; size]).unwrap();
        }

        let results = Scanner::new(root).with_max_results(2).scan().unwrap();
        assert_eq!(results.len(), 2);
        let names: Vec<_> = results
            .iter()
            .filter_map(|item| item.project_name.as_deref())
            .collect();
        assert!(names.contains(&"large"));
        assert!(names.contains(&"medium"));
    }

    #[test]
    fn parallel_dir_size_is_reasonable() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sizeme");
        fs::create_dir_all(&dir).unwrap();
        // ~40 KiB of real data.
        fs::write(dir.join("a.bin"), vec![b'x'; 20_000]).unwrap();
        fs::write(dir.join("b.bin"), vec![b'y'; 20_000]).unwrap();

        let size = parallel_dir_size(&dir);
        // On-disk size should cover the logical bytes written and stay within a
        // sane multiple of it (block rounding, not a runaway over-count).
        assert!(size >= 40_000, "size {size} should cover written bytes");
        assert!(size < 40_000 * 4, "size {size} unexpectedly large");
    }

    #[cfg(unix)]
    #[test]
    fn parallel_dir_size_dedups_hardlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hardlinks");
        fs::create_dir_all(&dir).unwrap();

        let original = dir.join("original.bin");
        fs::write(&original, vec![b'z'; 50_000]).unwrap();
        let baseline = parallel_dir_size(&dir);

        // Add three hardlinks to the same inode. They must not inflate the total.
        for i in 0..3 {
            std::fs::hard_link(&original, dir.join(format!("link{i}.bin"))).unwrap();
        }
        let with_links = parallel_dir_size(&dir);

        assert_eq!(
            baseline, with_links,
            "hardlinks to the same inode must be counted once (baseline {baseline}, with links {with_links})"
        );
    }

    #[test]
    fn cancel_flag_stops_sizing_loop() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        // Several node projects so there's more than one artifact to size.
        for i in 0..8 {
            let project = root.join(format!("proj{i}"));
            fs::create_dir_all(project.join("node_modules")).unwrap();
            fs::write(project.join("package.json"), b"{}").unwrap();
            fs::write(project.join("node_modules").join("blob"), vec![b'x'; 1024]).unwrap();
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        // Cancel as soon as the sizing pass starts emitting "Sizing:" progress.
        let scanner = Scanner::new(root);
        let result = scanner.scan_with_progress(cancel, move |_, _, path, _| {
            if path.starts_with("Sizing:") {
                cancel_clone.store(true, Ordering::Relaxed);
            }
        });
        let results = result.unwrap();
        // Cancelling mid-sizing must return early: strictly fewer than all 8.
        assert!(
            results.len() < 8,
            "expected sizing to stop early, got {} results",
            results.len()
        );
    }

    #[test]
    fn count_directories_matches_dirs_visited() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        // A mixed tree: node + rust projects plus some plain dirs.
        setup_node_project(&root);
        setup_rust_project(&root);
        fs::create_dir_all(root.join("plain").join("nested").join("deep")).unwrap();
        fs::create_dir_all(root.join("another")).unwrap();

        let scanner = Scanner::new(root);
        let counted = scanner.count_directories();

        // Run the real scan and capture the peak dirs_scanned reported.
        let cancel = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicUsize::new(0));
        let peak_clone = Arc::clone(&peak);
        scanner
            .scan_with_progress(cancel, move |dirs, _, _, _| {
                peak_clone.fetch_max(dirs, Ordering::Relaxed);
            })
            .unwrap();
        let visited = peak.load(Ordering::Relaxed);

        assert_eq!(
            counted, visited,
            "count_directories ({counted}) must equal dirs actually visited ({visited})"
        );
    }

    #[test]
    fn validate_artifact_path_rejects_changed_rule_state() {
        let tmp = tempfile::tempdir().unwrap();
        let root = scan_root(&tmp);
        let project = root.join("myproject");
        fs::create_dir_all(project.join("node_modules")).unwrap();
        fs::write(project.join("package.json"), b"{}").unwrap();

        validate_artifact_path(&project.join("node_modules"), "node_modules", false).unwrap();
        fs::remove_file(project.join("package.json")).unwrap();
        assert!(
            validate_artifact_path(&project.join("node_modules"), "node_modules", false).is_err()
        );
    }
}
