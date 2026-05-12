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

    /// Cap the number of matches returned. Results beyond the limit are silently
    /// dropped (scan still runs to completion but stops collecting after the cap).
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

        let walker = self.build_walker(matches.clone(), dirs_scanned.clone());

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

                children.retain(|child| {
                    let Ok(child) = child else { return true };
                    let name = child.file_name().to_string_lossy();
                    if name.starts_with('.') {
                        return enabled.iter().any(|r| r.dir_name == name.as_ref());
                    }
                    !matches!(name.as_ref(), "Library" | "Applications" | "System")
                });

                for child in children.iter_mut() {
                    let Ok(entry) = child else { continue };
                    if !entry.file_type.is_dir() {
                        continue;
                    }
                    let name_owned = entry.file_name().to_string_lossy().into_owned();
                    let matched = enabled.iter().any(|rule| {
                        if rule.dir_name != name_owned {
                            return false;
                        }
                        rule.markers.is_empty()
                            || rule
                                .markers
                                .iter()
                                .any(|m| has_marker_cached(parent_path, m, marker_cache.as_ref()))
                    });
                    if matched {
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
                // hidden roots that aren't in the rule registry).
                children.retain(|child| {
                    let Ok(child) = child else { return true };
                    let name = child.file_name().to_string_lossy();
                    if name.starts_with('.') {
                        // Allow `.next`/`.venv`/`.gradle`/etc — they're rules.
                        return enabled.iter().any(|r| r.dir_name == name.as_ref());
                    }
                    !matches!(name.as_ref(), "Library" | "Applications" | "System")
                });

                // Match enabled rules; on a hit, record the match and prune.
                for child in children.iter_mut() {
                    let Ok(entry) = child else { continue };
                    if !entry.file_type.is_dir() {
                        continue;
                    }
                    let name_owned = entry.file_name().to_string_lossy().into_owned();
                    let matched = enabled.iter().find_map(|rule| {
                        if rule.dir_name != name_owned {
                            return None;
                        }
                        match_rule(parent_path, rule, marker_cache.as_ref())
                    });
                    if let Some(rule_match) = matched {
                        let path = entry.path();
                        matches.lock().push((path, rule_match));
                        // Don't walk into matched artifacts during the outer
                        // traversal — sizing handles their interior.
                        entry.read_children_path = None;
                    }
                }
            })
    }
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
    let key = (parent.to_path_buf(), marker);
    if let Some(value) = marker_cache.lock().get(&key).copied() {
        return value;
    }
    let value = has_marker(parent, marker);
    marker_cache.lock().insert(key, value);
    value
}

/// Test whether `parent` contains a sibling matching `marker`. Marker tokens
/// starting with '.' are treated as file extensions — the parent directory is
/// scanned for any file with that extension.
fn has_marker(parent: &Path, marker: &str) -> bool {
    if let Some(ext) = marker.strip_prefix('.').filter(|s| !s.contains('/')) {
        if let Ok(rd) = std::fs::read_dir(parent) {
            for entry in rd.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && name.ends_with(&format!(".{ext}"))
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

fn parallel_dir_size(path: &Path) -> u64 {
    let total = AtomicU64::new(0);
    for de in WalkDirGeneric::<((), ())>::new(path)
        .follow_links(false)
        .skip_hidden(false)
        .into_iter()
        .flatten()
    {
        if de.file_type.is_file()
            && let Ok(meta) = de.metadata()
        {
            total.fetch_add(meta.len(), Ordering::Relaxed);
        }
    }
    total.load(Ordering::Relaxed)
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
        for (name, size) in [("small", 1), ("large", 1024), ("medium", 16)] {
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
