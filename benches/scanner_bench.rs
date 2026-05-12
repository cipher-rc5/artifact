use artifact::scanner::Scanner;
use criterion::{Criterion, criterion_group, criterion_main};
use std::fs;

fn populate(root: &std::path::Path, projects: usize) {
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

fn scan_artifact_tree(c: &mut Criterion) {
    c.bench_function("scan_node_projects_100", |b| {
        b.iter_batched(
            || {
                let tmp = tempfile::tempdir().unwrap();
                let root = tmp.path().join("workspace");
                fs::create_dir_all(&root).unwrap();
                populate(&root, 100);
                (tmp, root)
            },
            |(_tmp, root)| Scanner::new(root).scan().unwrap(),
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, scan_artifact_tree);
criterion_main!(benches);
