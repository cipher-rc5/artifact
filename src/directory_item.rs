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

    #[cfg(test)]
    fn with_modified(last_modified: Option<SystemTime>) -> Self {
        let rule = rules::RULES
            .first()
            .expect("at least one built-in rule must exist");
        Self::new(
            PathBuf::from("/tmp/x"),
            DirectoryType::new(rule),
            0,
            last_modified,
            None,
            None,
            false,
        )
    }

    /// Whole days between `last_modified` and now.
    ///
    /// A future mtime (clock skew, a restored backup, a file touched with a
    /// future timestamp) is reported as `0` days rather than a negative or
    /// wildly large value. The result saturates into `i64` instead of using a
    /// lossy `as` cast, so an absurd timestamp yields `i64::MAX` rather than a
    /// truncated/wrapped number (L1).
    pub fn days_since_modified(&self) -> Option<i64> {
        self.last_modified.map(
            |modified| match SystemTime::now().duration_since(modified) {
                // Normal case: mtime is in the past.
                Ok(elapsed) => i64::try_from(elapsed.as_secs() / 86_400).unwrap_or(i64::MAX),
                // mtime is in the future — clamp to 0 rather than going negative.
                Err(_) => 0,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn days_since_modified_none_when_missing() {
        assert_eq!(
            DirectoryItem::with_modified(None).days_since_modified(),
            None
        );
    }

    #[test]
    fn days_since_modified_past() {
        let three_days = SystemTime::now() - Duration::from_secs(3 * 86_400 + 100);
        let item = DirectoryItem::with_modified(Some(three_days));
        assert_eq!(item.days_since_modified(), Some(3));
    }

    #[test]
    fn days_since_modified_future_is_zero() {
        // A file dated a year in the future must report 0, never negative.
        let future = SystemTime::now() + Duration::from_secs(365 * 86_400);
        let item = DirectoryItem::with_modified(Some(future));
        assert_eq!(item.days_since_modified(), Some(0));
    }
}
