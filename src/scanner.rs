// file: src/scanner.rs
// description: Filesystem scanner for node_modules and target directories

use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};
use walkdir::WalkDir;

use crate::directory_item::{DirectoryItem, DirectoryType};
use crate::error::{ArtifactError, Result};

pub struct Scanner {
    root: PathBuf,
}

impl Scanner {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn scan(
        &self,
        scan_node_modules: bool,
        scan_rust_target: bool,
    ) -> Result<Vec<DirectoryItem>> {
        self.scan_with_progress(scan_node_modules, scan_rust_target, |_, _, _, _| {})
    }

    /// Scan with a progress callback.
    ///
    /// `on_progress(dirs_scanned, items_found, current_path, total_size_found)`
    /// is called periodically and whenever a new build artifact is discovered.
    pub fn scan_with_progress(
        &self,
        scan_node_modules: bool,
        scan_rust_target: bool,
        on_progress: impl Fn(usize, usize, &str, u64),
    ) -> Result<Vec<DirectoryItem>> {
        info!("Scanning from root: {}", self.root.display());

        if !self.root.exists() {
            return Err(ArtifactError::Scan(format!(
                "Path does not exist: {}",
                self.root.display()
            )));
        }

        let mut results = Vec::new();
        let mut dirs_scanned: usize = 0;
        let mut total_size_found: u64 = 0;

        for entry in WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.') && name != "Library" && name != "Applications"
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    debug!("Skipping inaccessible entry: {}", e);
                    continue;
                }
            };

            if !entry.file_type().is_dir() {
                continue;
            }

            dirs_scanned += 1;
            let path_str = entry.path().display().to_string();

            if dirs_scanned.is_multiple_of(100) {
                on_progress(dirs_scanned, results.len(), &path_str, total_size_found);
            }

            let name = entry.file_name().to_string_lossy();
            let path = entry.path().to_path_buf();

            let matched = if scan_node_modules && name == "node_modules" {
                Some(DirectoryType::NodeModules)
            } else if scan_rust_target && name == "target" {
                if entry
                    .path()
                    .parent()
                    .is_some_and(|p| p.join("Cargo.toml").exists())
                {
                    Some(DirectoryType::RustTarget)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(dir_type) = matched {
                on_progress(
                    dirs_scanned,
                    results.len(),
                    &format!("Sizing: {}", path_str),
                    total_size_found,
                );

                if let Some(item) = self.create_item(path, dir_type) {
                    total_size_found += item.size_bytes;
                    results.push(item);
                    on_progress(dirs_scanned, results.len(), &path_str, total_size_found);
                }
            }
        }

        on_progress(dirs_scanned, results.len(), "", total_size_found);

        results.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        info!("Scan complete: found {} directories", results.len());
        Ok(results)
    }

    fn create_item(&self, path: PathBuf, dir_type: DirectoryType) -> Option<DirectoryItem> {
        let size = Self::dir_size(&path);
        let last_modified = fs::metadata(&path).ok().and_then(|m| m.modified().ok());

        let project_root = path.parent().map(|p| p.to_path_buf());
        let project_name = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        let is_orphaned = match &dir_type {
            DirectoryType::NodeModules => path
                .parent()
                .map(|p| !p.join("package.json").exists())
                .unwrap_or(true),
            DirectoryType::RustTarget => path
                .parent()
                .map(|p| !p.join("Cargo.toml").exists())
                .unwrap_or(true),
        };

        Some(DirectoryItem::new(
            path,
            dir_type,
            size,
            last_modified,
            project_root,
            project_name,
            is_orphaned,
        ))
    }

    fn dir_size(path: &PathBuf) -> u64 {
        WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    }
}
