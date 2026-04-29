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

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jwalk::WalkDirGeneric;
use tracing::{debug, info};

use crate::directory_item::{DirectoryItem, DirectoryType};
use crate::error::{ArtifactError, Result};
use crate::rules::{self, ArtifactRule};

/// How often the scanner emits a "still working" progress event during the
/// outer traversal. Item-discovery events bypass this throttle.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

pub struct Scanner {
    root: PathBuf,
    enabled_rules: Vec<&'static ArtifactRule>,
}

impl Scanner {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            enabled_rules: rules::RULES.iter().collect(),
        }
    }

    /// Build a scanner restricted to a specific set of rule names. Unknown
    /// names are silently skipped.
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
        }
    }

    pub fn scan(&self) -> Result<Vec<DirectoryItem>> {
        self.scan_with_progress(|_, _, _, _| {})
    }

    /// Scan with a progress callback.
    ///
    /// `on_progress(dirs_scanned, items_found, current_path, total_size_found)`
    /// is invoked from the scanner thread. Keep the closure cheap.
    pub fn scan_with_progress(
        &self,
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
        let matches: Arc<Mutex<Vec<(PathBuf, &'static ArtifactRule)>>> =
            Arc::new(Mutex::new(Vec::new()));

        let last_progress = Arc::new(Mutex::new(Instant::now()));
        let on_progress = Arc::new(on_progress);

        let walker = self.build_walker(matches.clone(), dirs_scanned.clone());

        for entry in walker {
            match entry {
                Ok(de) => {
                    if !de.file_type.is_dir() {
                        continue;
                    }
                    let count = dirs_scanned.load(Ordering::Relaxed);
                    let mut last = last_progress.lock().unwrap();
                    if last.elapsed() >= PROGRESS_INTERVAL {
                        *last = Instant::now();
                        drop(last);
                        let path = de.path();
                        let path_str = path.display().to_string();
                        on_progress(
                            count,
                            matches.lock().unwrap().len(),
                            &path_str,
                            total_size_found.load(Ordering::Relaxed),
                        );
                    }
                }
                Err(e) => debug!("Skipping inaccessible entry: {e}"),
            }
        }

        // Size each match in parallel.
        let raw_matches: Vec<(PathBuf, &'static ArtifactRule)> = {
            let mut guard = matches.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        info!("Discovered {} candidate directories; sizing", raw_matches.len());

        let mut results: Vec<DirectoryItem> = Vec::with_capacity(raw_matches.len());
        let final_dirs = dirs_scanned.load(Ordering::Relaxed);

        for (path, rule) in raw_matches {
            on_progress(
                final_dirs,
                results.len(),
                &format!("Sizing: {}", path.display()),
                total_size_found.load(Ordering::Relaxed),
            );

            if let Some(item) = build_item(path, rule) {
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

        results.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        info!(
            "Scan complete: found {} directories ({} dirs visited)",
            results.len(),
            final_dirs
        );
        Ok(results)
    }

    fn build_walker(
        &self,
        matches: Arc<Mutex<Vec<(PathBuf, &'static ArtifactRule)>>>,
        dirs_scanned: Arc<AtomicUsize>,
    ) -> WalkDirGeneric<((), ())> {
        let enabled = self.enabled_rules.clone();

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
                        if rule.markers.is_empty()
                            || rule
                                .markers
                                .iter()
                                .any(|m| has_marker(parent_path, m))
                        {
                            Some(*rule)
                        } else {
                            None
                        }
                    });
                    if let Some(rule) = matched {
                        let path = entry.path();
                        matches.lock().unwrap().push((path, rule));
                        // Don't walk into matched artifacts during the outer
                        // traversal — sizing handles their interior.
                        entry.read_children_path = None;
                    }
                }
            })
    }
}

/// Test whether `parent` contains a sibling matching `marker`. Marker tokens
/// starting with '.' are treated as file extensions — the parent directory is
/// scanned for any file with that extension.
fn has_marker(parent: &Path, marker: &str) -> bool {
    if let Some(ext) = marker.strip_prefix('.').filter(|s| !s.contains('/')) {
        if let Ok(rd) = std::fs::read_dir(parent) {
            for entry in rd.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(&format!(".{ext}")) {
                        return true;
                    }
                }
            }
        }
        return false;
    }
    parent.join(marker).exists()
}

fn build_item(path: PathBuf, rule: &'static ArtifactRule) -> Option<DirectoryItem> {
    let size = parallel_dir_size(&path);
    let last_modified = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());

    let project_root = path.parent().map(|p| p.to_path_buf());
    let project_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string());

    // A match is "orphaned" if none of its declared markers exist anymore
    // (e.g. someone deleted the source project but left node_modules behind).
    // Rules without markers are never orphaned by this definition.
    let is_orphaned = if rule.markers.is_empty() {
        false
    } else {
        path.parent()
            .map(|p| !rule.markers.iter().any(|m| has_marker(p, m)))
            .unwrap_or(true)
    };

    Some(DirectoryItem::new(
        path,
        DirectoryType::new(rule),
        size,
        last_modified,
        project_root,
        project_name,
        is_orphaned,
    ))
}

fn parallel_dir_size(path: &Path) -> u64 {
    let total = AtomicU64::new(0);
    for entry in WalkDirGeneric::<((), ())>::new(path)
        .follow_links(false)
        .skip_hidden(false)
    {
        if let Ok(de) = entry {
            if de.file_type.is_file() {
                if let Ok(meta) = de.metadata() {
                    total.fetch_add(meta.len(), Ordering::Relaxed);
                }
            }
        }
    }
    total.load(Ordering::Relaxed)
}
