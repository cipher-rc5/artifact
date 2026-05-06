// file: src/utils.rs
// description: Utility functions for ARTIFACT

use crate::config::DeleteMode;
use anyhow::Context as _;
use std::path::{Path, PathBuf};

/// Return the current user's home directory, or `None` if it cannot be
/// determined on this platform.
pub fn get_home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Delete or trash `path` according to `mode`.
///
/// # Errors
///
/// Returns an error if:
/// - `path` does not exist.
/// - `path` is a symbolic link (callers must resolve to the real path first).
/// - The underlying delete / trash operation fails.
pub fn remove_directory(path: &Path, mode: DeleteMode) -> anyhow::Result<()> {
    // Refuse to operate on a path that does not exist.
    if !path.exists() {
        anyhow::bail!("path does not exist: {}", path.display());
    }

    // Refuse to follow symlinks — callers must resolve to the real path first.
    let meta = path
        .symlink_metadata()
        .context("failed to read path metadata")?;
    if meta.file_type().is_symlink() {
        anyhow::bail!("refusing to delete through a symlink: {}", path.display());
    }

    match mode {
        DeleteMode::Trash => trash::delete(path).context("failed to move directory to trash"),
        DeleteMode::Permanent => {
            std::fs::remove_dir_all(path).context("failed to permanently delete directory")
        }
    }
}

/// Format a byte count as a human-readable binary string (e.g. `"1.50 GiB"`).
pub fn format_size(bytes: u64) -> String {
    humansize::format_size(bytes, humansize::BINARY)
}

/// Format an elapsed time in seconds as a short human-readable string.
///
/// Values under 60 seconds are rendered as `"Xs"` (e.g. `"42s"`).
/// Longer values are rendered as `"Xm Ys"` (e.g. `"1m 30s"`).
pub fn format_elapsed(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else {
        format!("{}m {:.0}s", (secs / 60.0) as u64, secs % 60.0)
    }
}

/// List visible subdirectories of `path`, sorted alphabetically (case-insensitive).
///
/// Returns `(name, full_path)` pairs. Hidden directories (names starting with
/// `.`) are excluded. Returns an `io::Error` if `path` cannot be read.
pub fn list_directories(path: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if let Ok(ft) = entry.file_type()
            && ft.is_dir()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                entries.push((name, entry.path()));
            }
        }
    }
    entries.sort_by_key(|a| a.0.to_lowercase());
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1 KiB");
        assert_eq!(format_size(1024 * 1024), "1 MiB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1 GiB");
    }

    #[test]
    fn format_elapsed_seconds() {
        assert!(format_elapsed(0.0).ends_with('s'));
        assert!(format_elapsed(30.0).ends_with('s'));
        assert!(format_elapsed(90.0).contains('m'));
    }

    #[test]
    fn remove_directory_rejects_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let real_dir = tmp.path().join("real");
        fs::create_dir(&real_dir).unwrap();
        let link = tmp.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_dir, &link).unwrap();
        #[cfg(unix)]
        {
            let result = remove_directory(&link, crate::config::DeleteMode::Permanent);
            assert!(result.is_err(), "should refuse to delete through a symlink");
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("symlink"),
                "error should mention symlink, got: {msg}"
            );
        }
    }

    #[test]
    fn remove_directory_rejects_nonexistent_path() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does_not_exist");
        let result = remove_directory(&missing, crate::config::DeleteMode::Permanent);
        assert!(result.is_err());
    }

    #[test]
    fn remove_directory_permanent_deletes_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target_dir");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("file.txt"), b"hello").unwrap();
        let result = remove_directory(&target, crate::config::DeleteMode::Permanent);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert!(!target.exists(), "directory should be gone");
    }

    #[test]
    fn list_directories_returns_sorted_subdirs() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("zebra")).unwrap();
        fs::create_dir(tmp.path().join("apple")).unwrap();
        fs::create_dir(tmp.path().join(".hidden")).unwrap();
        let entries = list_directories(tmp.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["apple", "zebra"],
            "hidden dirs should be excluded; got: {names:?}"
        );
    }
}
