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
///
/// # TOCTOU note
///
/// This is the legacy entry point kept for compatibility. It canonicalizes
/// `path` internally and delegates to [`remove_directory_checked`]. Prefer
/// calling [`remove_directory_checked`] directly with a path you have already
/// canonicalized and containment-checked, so the exact validated canonical path
/// is what gets removed (review finding C1).
pub fn remove_directory(path: &Path, mode: DeleteMode) -> anyhow::Result<()> {
    // Preserve the original contract: refuse if the caller-supplied path is
    // itself a symlink, *before* canonicalizing (canonicalize would silently
    // follow the link to its target). Callers must resolve to the real path
    // first.
    let meta = path
        .symlink_metadata()
        .context("failed to read path metadata")?;
    if meta.file_type().is_symlink() {
        anyhow::bail!("refusing to delete through a symlink: {}", path.display());
    }

    let canonical = path
        .canonicalize()
        .with_context(|| format!("path is no longer accessible: {}", path.display()))?;
    remove_directory_checked(&canonical, mode)
}

/// Delete or trash a **canonicalized** path according to `mode`, with a
/// tightened re-validation performed immediately before the removal to shrink
/// the TOCTOU window (review finding C1).
///
/// Callers must pass a path obtained from [`Path::canonicalize`] (or otherwise
/// fully resolved). This function:
///
/// 1. Refuses a path whose final component is a symlink (`symlink_metadata`),
///    performed as late as possible before the delete syscall.
/// 2. Re-verifies that **no ancestor component** of the path is itself a
///    symlink. Because the path is canonical, this should hold; re-checking here
///    catches a component being swapped for a symlink after the caller's
///    validation but before this call.
/// 3. Confirms the target is a directory, then trashes / removes it.
///
/// ## Residual window
///
/// Without holding an open directory file descriptor and using `unlinkat`-style
/// fd-relative removal (which would require a low-level syscall dependency such
/// as `libc`/`rustix`, intentionally not added here), a small residual TOCTOU
/// window remains between the final `symlink_metadata` check and the removal
/// call: `trash::delete` / `remove_dir_all` re-resolve the path by name. The
/// ancestor + final-component re-checks *narrow* this window substantially but
/// do not fully *close* it. For a hostile local filesystem, `DeleteMode::Trash`
/// (the hard default) is the safer posture, as the OS trash implementation does
/// not recursively unlink through the resolved path the way `remove_dir_all`
/// does.
pub fn remove_directory_checked(canonical: &Path, mode: DeleteMode) -> anyhow::Result<()> {
    // Late re-validation of the final component: refuse a symlink and require it
    // to still exist as a directory. Performing this as close as possible to the
    // removal minimizes the swap window.
    let meta = canonical
        .symlink_metadata()
        .with_context(|| format!("failed to read path metadata: {}", canonical.display()))?;
    if meta.file_type().is_symlink() {
        anyhow::bail!(
            "refusing to delete through a symlink: {}",
            canonical.display()
        );
    }
    if !meta.is_dir() {
        anyhow::bail!(
            "refusing to delete a non-directory: {}",
            canonical.display()
        );
    }

    // Re-verify that no ancestor component is a symlink. A canonical path
    // contains no symlinks by construction, so any symlink discovered here means
    // a component was swapped after the caller validated — refuse.
    verify_no_symlink_ancestors(canonical)?;

    match mode {
        DeleteMode::Trash => trash::delete(canonical).context("failed to move directory to trash"),
        DeleteMode::Permanent => {
            std::fs::remove_dir_all(canonical).context("failed to permanently delete directory")
        }
    }
}

/// Verify that none of the ancestor components of `path` is a symbolic link.
///
/// `path` is expected to be canonical (no symlinks). If any ancestor now
/// resolves as a symlink, a component was swapped since the path was validated
/// and we refuse to proceed.
fn verify_no_symlink_ancestors(path: &Path) -> anyhow::Result<()> {
    // Skip the final component itself (checked by the caller) but walk every
    // parent up to the root.
    for ancestor in path.ancestors().skip(1) {
        // The filesystem root / prefix has no metadata worth checking and always
        // exists; stop if we can't read it rather than failing the delete.
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match ancestor.symlink_metadata() {
            Ok(meta) if meta.file_type().is_symlink() => {
                anyhow::bail!(
                    "refusing to delete: ancestor path is now a symlink: {}",
                    ancestor.display()
                );
            }
            Ok(_) => {}
            // A parent we cannot stat (permissions, or root prefix on some
            // platforms) is not evidence of tampering; keep walking.
            Err(_) => {}
        }
    }
    Ok(())
}

/// Format a byte count as a human-readable binary string (e.g. `"1.50 GiB"`).
pub fn format_size(bytes: u64) -> String {
    humansize::format_size(bytes, humansize::BINARY)
}

/// Format an integer with thousands separators (e.g. `1234567` -> `"1,234,567"`).
///
/// Canonical location for what used to be duplicated in `app.rs` and `view.rs`
/// (review finding L3). Delegates to [`crate::history::format_number`]; callers
/// should prefer `artifact::utils::format_number`.
pub fn format_number(n: usize) -> String {
    crate::history::format_number(n)
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
    fn remove_directory_checked_rejects_symlink_final_component() {
        let tmp = tempfile::tempdir().unwrap();
        let real_dir = tmp.path().join("real");
        fs::create_dir(&real_dir).unwrap();
        let link = tmp.path().join("link");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real_dir, &link).unwrap();
            // Pass the non-canonical symlink path directly to the checked
            // variant: it must refuse on the final-component symlink check.
            let result = remove_directory_checked(&link, crate::config::DeleteMode::Permanent);
            assert!(result.is_err(), "should refuse a symlink final component");
            assert!(
                result.unwrap_err().to_string().contains("symlink"),
                "error should mention symlink"
            );
            assert!(real_dir.exists(), "the real target must be untouched");
        }
    }

    #[test]
    fn remove_directory_checked_refuses_symlinked_ancestor() {
        // Simulate a TOCTOU swap: the validated canonical path pointed at
        // real_parent/target, but real_parent is replaced by a symlink to
        // evil_parent. The ancestor re-check must catch this and refuse.
        #[cfg(unix)]
        {
            let tmp = tempfile::tempdir().unwrap();
            let evil_parent = tmp.path().join("evil");
            fs::create_dir(&evil_parent).unwrap();
            fs::create_dir(evil_parent.join("target")).unwrap();

            // The path we "validated" earlier, expressed through a parent that is
            // now a symlink.
            let swapped_parent = tmp.path().join("parent");
            std::os::unix::fs::symlink(&evil_parent, &swapped_parent).unwrap();
            let target_via_symlink = swapped_parent.join("target");

            let result =
                remove_directory_checked(&target_via_symlink, crate::config::DeleteMode::Permanent);
            assert!(
                result.is_err(),
                "should refuse when an ancestor is a symlink"
            );
            assert!(
                evil_parent.join("target").exists(),
                "the evil target must be untouched"
            );
        }
    }

    #[test]
    fn remove_directory_checked_deletes_canonical_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target_dir");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("file.txt"), b"hello").unwrap();
        let canonical = target.canonicalize().unwrap();
        let result = remove_directory_checked(&canonical, crate::config::DeleteMode::Permanent);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert!(!target.exists(), "directory should be gone");
    }

    #[test]
    fn remove_directory_canonicalizes_before_delete() {
        // remove_directory should resolve a non-canonical (but real) path and
        // delete the resolved directory.
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("nested").join("..").join("target");
        fs::create_dir(tmp.path().join("nested")).unwrap();
        fs::create_dir(tmp.path().join("target")).unwrap();
        let result = remove_directory(&target, crate::config::DeleteMode::Permanent);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert!(
            !tmp.path().join("target").exists(),
            "resolved directory should be gone"
        );
    }

    #[test]
    fn format_number_matches_history_impl() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(1_234_567), "1,234,567");
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
