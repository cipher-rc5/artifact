use artifact::scanner::Scanner;
use criterion::{Criterion, criterion_group, criterion_main};
use std::fs;
use std::hint::black_box;
use std::path::Path;

/// A flat set of `projects` node projects, each with a tiny `node_modules`.
fn populate(root: &Path, projects: usize) {
    for i in 0..projects {
        let project = root.join(format!("project-{i}"));
        fs::create_dir_all(project.join("node_modules").join("pkg")).unwrap();
        fs::write(project.join("package.json"), b"{}").unwrap();
        fs::write(
            project.join("node_modules").join("pkg").join("index.js"),
            b"x",
        )
        .unwrap();
    }
}

/// A deeply nested chain of plain directories with a single node project at the
/// bottom — exercises traversal depth rather than fan-out.
fn populate_deep(root: &Path, depth: usize) {
    let mut cur = root.to_path_buf();
    for i in 0..depth {
        cur = cur.join(format!("level-{i}"));
    }
    fs::create_dir_all(cur.join("node_modules").join("pkg")).unwrap();
    fs::write(cur.join("package.json"), b"{}").unwrap();
    fs::write(cur.join("node_modules").join("pkg").join("index.js"), b"x").unwrap();
}

/// Node projects whose `node_modules` interior is full of hardlinks to a single
/// inode — exercises the hardlink-dedup path in sizing.
fn populate_hardlinks(root: &Path, projects: usize, links: usize) {
    for i in 0..projects {
        let project = root.join(format!("hl-{i}"));
        let nm = project.join("node_modules");
        fs::create_dir_all(&nm).unwrap();
        fs::write(project.join("package.json"), b"{}").unwrap();
        let original = nm.join("original.bin");
        fs::write(&original, vec![b'z'; 4096]).unwrap();
        for l in 0..links {
            #[cfg(unix)]
            let _ = std::fs::hard_link(&original, nm.join(format!("link-{l}.bin")));
            #[cfg(not(unix))]
            let _ = fs::write(nm.join(format!("link-{l}.bin")), vec![b'z'; 4096]);
        }
    }
}

/// .NET-style projects that match via extension markers (`.csproj`) rather than
/// a plain filename — exercises the `has_marker` extension-scan path.
fn populate_ext_markers(root: &Path, projects: usize) {
    for i in 0..projects {
        let project = root.join(format!("dotnet-{i}"));
        fs::create_dir_all(project.join("bin").join("Debug")).unwrap();
        fs::create_dir_all(project.join("obj")).unwrap();
        fs::write(project.join("App.csproj"), b"<Project/>").unwrap();
        fs::write(project.join("bin").join("Debug").join("App.dll"), b"MZ").unwrap();
    }
}

fn make_root(tmp: &tempfile::TempDir) -> std::path::PathBuf {
    let root = tmp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    root
}

fn scan_artifact_tree(c: &mut Criterion) {
    c.bench_function("scan_node_projects_100", |b| {
        b.iter_batched(
            || {
                let tmp = tempfile::tempdir().unwrap();
                let root = make_root(&tmp);
                populate(&root, 100);
                (tmp, root)
            },
            |(_tmp, root)| black_box(Scanner::new(root).scan().unwrap()),
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("scan_deep_nesting_64", |b| {
        b.iter_batched(
            || {
                let tmp = tempfile::tempdir().unwrap();
                let root = make_root(&tmp);
                populate_deep(&root, 64);
                (tmp, root)
            },
            |(_tmp, root)| black_box(Scanner::new(root).scan().unwrap()),
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("scan_hardlinks_20x50", |b| {
        b.iter_batched(
            || {
                let tmp = tempfile::tempdir().unwrap();
                let root = make_root(&tmp);
                populate_hardlinks(&root, 20, 50);
                (tmp, root)
            },
            |(_tmp, root)| black_box(Scanner::new(root).scan().unwrap()),
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("scan_ext_markers_100", |b| {
        b.iter_batched(
            || {
                let tmp = tempfile::tempdir().unwrap();
                let root = make_root(&tmp);
                populate_ext_markers(&root, 100);
                (tmp, root)
            },
            |(_tmp, root)| black_box(Scanner::new(root).scan().unwrap()),
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("scan_max_results_10_of_200", |b| {
        b.iter_batched(
            || {
                let tmp = tempfile::tempdir().unwrap();
                let root = make_root(&tmp);
                populate(&root, 200);
                (tmp, root)
            },
            |(_tmp, root)| black_box(Scanner::new(root).with_max_results(10).scan().unwrap()),
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, scan_artifact_tree);
criterion_main!(benches);
