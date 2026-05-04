# Production Grade Improvements Checklist

Tasks ordered by priority. Complete all Critical and High items before any public release.

---

## Critical (Blockers)

- [x] **Add deletion confirmation dialog**  
  Gate `app.delete_selected()` behind a modal that requires explicit confirmation. The existing warning copy can be surfaced inside the dialog. No click should trigger deletion in a single step.  
  _Files: `src/view.rs`, `src/app.rs`_

- [x] **Replace fire-and-forget DB threads with observable futures**  
  DB write is now synchronous and inline; failures are pushed into `errors` and surfaced to the user via the existing notice system.  
  _Files: `src/app.rs`_

- [x] **Replace `lock().unwrap()` in scanner with `parking_lot::Mutex`**  
  All `std::sync::Mutex` usages in `scanner.rs` replaced with `parking_lot::Mutex`. No more poison-panic risk in parallel walker threads.  
  _Files: `src/scanner.rs`_

- [x] **Implement scan cancellation**  
  `Arc<AtomicBool>` cancel flag wired through scanner and app. `cancel_scan()` and `is_scan_cancellable()` methods ready. UI cancel button still needs wiring (`cancel_scan` exists but has no trigger yet).  
  _Files: `src/scanner.rs`, `src/app.rs`_

- [x] **Write unit and integration tests**  
  27 tests passing across all modules (27/27 green).
  - [x] `scanner.rs` — rule matching, orphan detection, cancel flag, max_results cap
  - [x] `database.rs` — insert, query by range, cleanup, statistics
  - [x] `utils.rs` — `remove_directory` (symlink rejection, permanent delete, temp dirs), `format_size`
  - [x] `config.rs` — load defaults, parse valid TOML, reject invalid values, constraint clamping
  - [x] `rules.rs` — uniqueness of rule names, expected matches  
  _Files: all `src/*.rs`_

---

## High Priority

- [x] **Fix hard-coded version string**  
  Replaced with `concat!("BUILD CLEANUP V", env!("CARGO_PKG_VERSION"))` so it always tracks `Cargo.toml`.  
  _Files: `src/view.rs:538`_

- [x] **Add symlink guard before deletion**  
  `remove_directory` now calls `symlink_metadata()` first and bails if path is a symlink or not a directory before any deletion operation.  
  _Files: `src/utils.rs`_

- [x] **Fix clock regression panics**  
  Both `.unwrap()` calls on `duration_since(UNIX_EPOCH)` replaced with `.unwrap_or_else` that logs a warning and falls back to `Duration::ZERO`.  
  _Files: `src/database.rs:55, 306`_

- [x] **Make orphan detection failure-safe**  
  `has_marker()` now returns `Option<bool>` — `None` on I/O error. Callers treat `None` conservatively: build_walker won't match, orphan check won't flag.  
  _Files: `src/scanner.rs:237–250`_

- [x] **Cap scan results memory**  
  `max_results: usize` (default 10,000) added to `ScanConfig` and `Scanner`. Walker stops accumulating when the cap is hit; capacity guard in `process_read_dir` prevents further growth.  
  _Files: `src/scanner.rs`, `src/config.rs`_

---

## Medium Priority

- [x] **Add config schema validation on load**  
  `apply_constraints()` called after deserialization: clamps window dimensions (400–16000×300–8000), resets invalid `log_level` to "info", enforces `max_results >= 1`.  
  _Files: `src/config.rs`_

- [x] **Validate file browser paths**  
  `browse_navigate()` calls `symlink_metadata()` first; bails with warning notice on symlink, non-directory, or unreadable path.  
  _Files: `src/app.rs`_

- [x] **Surface channel send failures**  
  `tx_cb.send(...).is_err()` sets cancel flag to stop the scanner thread when receiver is dropped. Prevents ghost scans.  
  _Files: `src/app.rs`_

- [x] **Surface history load failures**  
  `load_history` returns `Result<Vec<HistoryRun>, String>`; History tab renders a red "HISTORY UNAVAILABLE: …" banner on DB error.  
  _Files: `src/app.rs`, `src/view.rs`_

- [x] **Add selection bounds guard**  
  Index bounds checked before toggle/delete; stale out-of-range events are logged and discarded.  
  _Files: `src/app.rs`_

- [x] **Persist UI filter preferences**  
  `toggle_orphaned_only()` persists `show_orphaned_only` to `AppConfig` and calls `config.save()`. Init reads from config at startup.  
  _Files: `src/app.rs`, `src/config.rs`_

---

## Low Priority / Polish

- [x] **Fix `ProgressBar::render_indeterminate`**  
  Added `render_progress(f32)` method using proportional `relative()` fill width and `overflow_hidden()` containment.  
  _Files: `src/components.rs`_

- [ ] **Add custom rule support**  
  Allow users to define additional artifact rules in `config.toml` with `name`, `dir_name`, `marker_files`, and `language` fields. Merge with the static rule list at startup.  
  _Files: `src/rules.rs`, `src/config.rs`_

- [ ] **Add basic keyboard navigation**  
  Arrow key up/down to move through results, Space to toggle selection, Enter to expand, Delete/Backspace to trigger deletion (with confirmation). This is a minimum accessibility baseline.  
  _Files: `src/view.rs`_

- [ ] **Add system light theme**  
  Detect `prefers-color-scheme` via GPUI or an OS query. Provide a light palette variant in `theme.rs` and toggle between them.  
  _Files: `src/theme.rs`_

- [x] **Set up CI pipeline**  
  GitHub Actions workflow runs on every push/PR: `cargo check`, `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`, `cargo audit`.  
  _Files: `.github/workflows/ci.yml`_

- [x] **Pin zig and cargo-zigbuild versions in justfile**  
  Pinned: zig 0.14.0, cargo-zigbuild 0.19.8, just 1.40.0 in justfile header comment + `verify-tools` recipe.  
  _Files: `justfile`_

- [ ] **Add WCAG contrast check to theme**  
  Audit `text_tertiary` and other subtle colors against their backgrounds. Ensure all text meets WCAG AA (4.5:1 for normal text, 3:1 for large text).  
  _Files: `src/theme.rs`_
