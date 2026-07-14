//! End-to-end integration coverage for the artifact core pipeline.
//!
//! These tests exercise the real cross-module flow — scan → validate → delete →
//! record → query history — against a temporary filesystem and a temporary redb
//! database. They deliberately avoid the GPUI layer (which cannot be built in
//! headless CI without a GPU/Metal toolchain) and use `DeleteMode::Permanent`
//! on throwaway temp directories so the removal is observable without depending
//! on a platform trash implementation.

use std::fs;
use std::path::Path;

use artifact::config::DeleteMode;
use artifact::database::{DeletionDatabase, DeletionRecord};
use artifact::scanner::{Scanner, validate_artifact_path};
use artifact::utils;

/// Build a non-hidden scan root inside a tempdir (the scanner prunes dotted
/// roots such as the `.tmp*` names `tempfile` generates).
fn workspace(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    let root = tmp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    root
}

fn make_node_project(base: &Path, name: &str, blob_bytes: usize) {
    let project = base.join(name);
    fs::create_dir_all(project.join("node_modules").join("pkg")).unwrap();
    fs::write(project.join("package.json"), b"{}").unwrap();
    fs::write(
        project.join("node_modules").join("pkg").join("index.js"),
        vec![b'x'; blob_bytes],
    )
    .unwrap();
}

#[test]
fn full_scan_delete_history_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let root = workspace(&tmp);
    make_node_project(&root, "alpha", 8 * 1024);
    make_node_project(&root, "beta", 32 * 1024);

    // 1. Scan discovers both node_modules artifacts.
    let scanner = Scanner::with_enabled(root.clone(), ["node_modules"]);
    let found = scanner.scan().unwrap();
    assert_eq!(found.len(), 2, "expected two node_modules dirs: {found:?}");
    for item in &found {
        assert!(item.path.ends_with("node_modules"));
        assert!(item.size_bytes > 0, "sizing should report non-zero bytes");
    }

    // 2. Open a deletion database in an isolated temp dir.
    let db_dir = tmp.path().join("db");
    let db = DeletionDatabase::new(Some(db_dir)).unwrap();

    // 3. Validate → delete → record each discovered artifact.
    for item in &found {
        // Re-validation must accept a still-valid artifact before removal.
        validate_artifact_path(&item.path, item.dir_type.name(), item.is_orphaned).unwrap();

        // Mirror the app's C1 safety contract: delete the canonicalized path.
        let canonical = item.path.canonicalize().unwrap();
        assert!(canonical.exists());
        utils::remove_directory_checked(&canonical, DeleteMode::Permanent).unwrap();
        assert!(!item.path.exists(), "artifact should be gone after delete");

        let record = DeletionRecord::new(
            item.path.clone(),
            item.dir_type,
            item.size_bytes,
            item.project_root.clone(),
            item.project_name.clone(),
        );
        let id = db.record_deletion(&record).unwrap();
        assert!(id > 0);
    }

    // 4. History reflects both deletions, newest-first, with correct totals.
    let recent = db.get_recent_deletions(10).unwrap();
    assert_eq!(recent.len(), 2);

    let stats = db.get_deletion_statistics().unwrap();
    assert_eq!(stats.total_deletions, 2);
    assert!(stats.total_space_freed > 0);
    assert!(stats.deletions_by_type.contains_key("node_modules"));

    let total = db.get_total_space_freed().unwrap();
    assert_eq!(total, stats.total_space_freed);
}

#[test]
fn validation_refuses_artifact_after_marker_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = workspace(&tmp);
    make_node_project(&root, "proj", 4 * 1024);

    let node_modules = root.join("proj").join("node_modules");
    // A valid node_modules beside package.json validates.
    validate_artifact_path(&node_modules, "node_modules", false).unwrap();

    // node_modules with no package.json is instead an *orphan*; asserting the
    // non-orphan state must now fail (guards against deleting under changed
    // conditions between scan and delete).
    fs::remove_file(root.join("proj").join("package.json")).unwrap();
    assert!(validate_artifact_path(&node_modules, "node_modules", false).is_err());
}

#[test]
fn deletion_refuses_symlinked_target() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("real_node_modules");
    fs::create_dir_all(&real).unwrap();
    let link = tmp.path().join("link");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real, &link).unwrap();
        // remove_directory must refuse a symlinked path outright.
        assert!(utils::remove_directory(&link, DeleteMode::Permanent).is_err());
        // The real directory is untouched.
        assert!(real.exists());
    }
    let _ = &link;
}
