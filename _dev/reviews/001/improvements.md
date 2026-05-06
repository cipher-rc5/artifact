# Improvements Checklist

**Generated from review:** _dev/reviews/001/critical_analysis.md
**Date:** 2026-05-05

---

## P0 — Blockers

- [ ] **[Conventions]** Fix 7 clippy errors failing `-D warnings` in `src/scanner.rs` and `src/utils.rs` — `sort_by_key`, `.flatten()`, collapsible ifs, needless borrow — `src/scanner.rs:194,268,316,320,321,407`, `src/utils.rs:61` — Effort: S
- [ ] **[Correctness]** Apply `config.scan.max_results` to the scanner in `start_scan`: call `.with_max_results(self.config.scan.max_results)` on the Scanner builder — `src/app.rs:398` — Effort: S

---

## P1 — Pre-release

- [ ] **[Docs]** Add `///` rustdoc to all 279 public items across `scanner.rs`, `database.rs`, `app.rs`, `components.rs`, `theme.rs`, `directory_item.rs`, `utils.rs`, `rules.rs` — Effort: L
- [ ] **[Docs]** Add module-level `//!` documentation to each `src/*.rs` file summarising its purpose and key invariants — Effort: M
- [ ] **[Testing]** Add test for `load_history` run-grouping algorithm (records within/outside 60-second window, empty DB, single record) — `src/app.rs:199-248` — Effort: M
- [ ] **[Testing]** Add test for `delete_selected` success path and partial-failure path (mock or tempdir-based) — `src/app.rs:520` — Effort: M
- [ ] **[CI/CD]** Add Linux runner (`ubuntu-latest`) to CI matrix — Linux `remove_dir_all` and `trash` behavior differs from macOS — `.github/workflows/ci.yml` — Effort: S
- [ ] **[API]** Replace `pub scan_log: Vec<String>` with a getter returning `&[String]` to match the encapsulation pattern used by all other `ArtifactApp` fields — `src/app.rs:129` — Effort: S
- [ ] **[API]** Remove the public `id: 0` default from `DeletionRecord` or make `id` private/`Option<u64>` to prevent callers from observing an invalid transient state — `src/database.rs:64` — Effort: M

---

## P2 — Should-fix

- [ ] **[Docs]** Add CHANGELOG (start with current `0.1.0` entry covering scan engine, redb persistence, GPUI UI) — Effort: S
- [ ] **[CI/CD]** Add `--all-features` to the clippy step in CI to match the `just clippy` recipe — `.github/workflows/ci.yml:43` — Effort: S
- [ ] **[Conventions]** Remove the four `#[allow(dead_code)]` methods from `ArtifactApp` (`cancel_scan`, `select_all`, `select_none`, `is_scan_cancellable`) — either wire them up to UI actions or delete them — `src/app.rs:188,495,609,627` — Effort: S
- [ ] **[Conventions]** Remove `#[allow(dead_code)]` from `HistoryEntry.dir_type` and `project_name` fields — either use them or delete them — `src/app.rs:66,70` — Effort: S
- [ ] **[Conventions]** Resolve or document `#![allow(unexpected_cfgs)]` in `main.rs:5` — add a comment explaining which cfg is unexpected and why suppression is safe — `src/main.rs:5` — Effort: S
- [ ] **[API]** Add `#[non_exhaustive]` to `ArtifactError`, `DeleteMode`, `ScanState`, and `NoticeKind` to avoid future semver breaks — `src/error.rs:9`, `src/config.rs:20`, `src/app.rs:28,50` — Effort: S
- [ ] **[Conventions]** Fix `Cargo.toml` `rust-version` to match README ("Rust 1.85+" in README, `rust-version = "1.95"` in Cargo.toml) — `Cargo.toml:8` — Effort: S
- [ ] **[Testing]** Add test for `has_marker` with extension-based markers (`.csproj`, `.sln`) and missing parent directory — `src/scanner.rs:264` — Effort: S
- [ ] **[Error Handling]** Unify error type usage: convert `config.rs` and `logging.rs` to return `ArtifactError` instead of `anyhow::Result` — `src/config.rs:139`, `src/logging.rs:20` — Effort: M

---

## P3 — Nice-to-have

- [ ] **[Performance]** Cache `hostname::get()` result in `ArtifactView` state instead of calling it on every render frame — `src/view.rs:238` — Effort: S
- [ ] **[Performance]** Replace `scan_log: Vec<String>` with `VecDeque<String>` to make `pop_front` O(1) instead of O(n) — `src/app.rs:129,451` — Effort: S
- [ ] **[Performance]** Cache `has_marker` results per parent path within a single scan to avoid repeated `read_dir` calls for the same directory — `src/scanner.rs:264` — Effort: M
- [ ] **[Concurrency]** Replace `std::sync::Mutex` with `parking_lot::Mutex` (already a project dependency) in scanner hot path for infallible locking — `src/scanner.rs:109,112,131` — Effort: S
- [ ] **[Performance]** Add benchmarks (`benches/`) for `parallel_dir_size` and the outer scan traversal to capture regressions — Effort: L
- [ ] **[Dependency]** Evaluate replacing `serde_json` metadata blob with a `format!` string — the metadata is written but never read back as structured JSON — `src/database.rs:58` — Effort: S
- [ ] **[CI/CD]** Add tag-triggered release workflow that runs `just build-all && just package` and uploads artifacts to a GitHub Release — Effort: M
- [ ] **[Docs]** Add rustdoc examples to `Scanner::new`, `Scanner::with_enabled`, and `DeletionDatabase::new` — Effort: S

---

## Progress

**Total items:** 27
**P0:** 2 | **P1:** 7 | **P2:** 9 | **P3:** 9
