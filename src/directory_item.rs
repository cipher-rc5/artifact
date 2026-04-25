// file: src/directory_item.rs
// description: Directory item types representing scannable directories

use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryType {
    NodeModules,
    RustTarget,
}

impl std::fmt::Display for DirectoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirectoryType::NodeModules => write!(f, "node_modules"),
            DirectoryType::RustTarget => write!(f, "target"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectoryItem {
    pub path: PathBuf,
    pub dir_type: DirectoryType,
    pub size_bytes: u64,
    pub last_modified: Option<SystemTime>,
    pub project_root: Option<PathBuf>,
    pub project_name: Option<String>,
    pub is_orphaned: bool,
    pub selected: bool,
}

impl DirectoryItem {
    pub fn new(
        path: PathBuf,
        dir_type: DirectoryType,
        size_bytes: u64,
        last_modified: Option<SystemTime>,
        project_root: Option<PathBuf>,
        project_name: Option<String>,
        is_orphaned: bool,
    ) -> Self {
        Self {
            path,
            dir_type,
            size_bytes,
            last_modified,
            project_root,
            project_name,
            is_orphaned,
            selected: false,
        }
    }

    pub fn days_since_modified(&self) -> Option<i64> {
        self.last_modified.map(|modified| {
            let elapsed = SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default();
            (elapsed.as_secs() / 86400) as i64
        })
    }
}
