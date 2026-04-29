// file: src/directory_item.rs
// description: Directory item types representing detected build artifacts.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::rules::{self, ArtifactRule};

/// The detected kind of a build artifact directory. Wraps a static reference
/// to the rule that matched, so callers get the rule's display name, language,
/// markers, and color hint without copying.
#[derive(Debug, Clone, Copy)]
pub struct DirectoryType {
    pub rule: &'static ArtifactRule,
}

impl DirectoryType {
    pub fn new(rule: &'static ArtifactRule) -> Self {
        Self { rule }
    }

    /// Resolve a stable rule name (as stored in the database) back into a kind.
    /// Returns None if the rule is unknown — e.g. a record from an older build.
    pub fn from_name(name: &str) -> Option<Self> {
        rules::find(name).map(Self::new)
    }

    /// Stable identifier — used as the database key for this kind.
    pub fn name(&self) -> &'static str {
        self.rule.name
    }
}

impl PartialEq for DirectoryType {
    fn eq(&self, other: &Self) -> bool {
        self.rule.name == other.rule.name
    }
}

impl Eq for DirectoryType {}

impl std::fmt::Display for DirectoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.rule.dir_name)
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
