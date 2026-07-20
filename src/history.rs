// file: src/history.rs
// description: Pure, `cx`-free core logic extracted from the app model so it
//              can be unit-tested with `cargo test --lib`. Holds deletion-run
//              grouping, number formatting, and deletion-manifest retention.

/// One deleted directory as surfaced in the history view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub path: String,
    pub size_bytes: i64,
    pub deleted_at: i64,
}

/// A group of deletions that happened close together in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRun {
    pub started_at: i64,
    pub entries: Vec<HistoryEntry>,
    pub total_bytes: i64,
}

/// Two deletions within this many seconds of each other are treated as part of
/// the same "run".
pub const RUN_WINDOW_SECS: i64 = 60;

/// Keep at most this many deletion manifests on disk; older ones are pruned.
pub const MANIFEST_RETENTION: usize = 50;

/// Group deletion entries into runs.
///
/// Entries are expected to be in descending order by `deleted_at` (as returned
/// by the DB). Any pair of deletions within [`RUN_WINDOW_SECS`] of the current
/// run's start is folded into that run.
///
/// This is decoupled from `DeletionRecord` on purpose: the app maps its DB rows
/// into [`HistoryEntry`] values first (a trivial field copy), which keeps this
/// grouping logic pure and directly unit-testable without touching the DB.
pub fn group_into_runs(entries: impl IntoIterator<Item = HistoryEntry>) -> Vec<HistoryRun> {
    let mut runs: Vec<HistoryRun> = Vec::new();
    for entry in entries {
        match runs.last_mut() {
            Some(run) if (run.started_at - entry.deleted_at).abs() <= RUN_WINDOW_SECS => {
                if entry.deleted_at < run.started_at {
                    run.started_at = entry.deleted_at;
                }
                run.total_bytes += entry.size_bytes;
                run.entries.push(entry);
            }
            _ => {
                runs.push(HistoryRun {
                    started_at: entry.deleted_at,
                    total_bytes: entry.size_bytes,
                    entries: vec![entry],
                });
            }
        }
    }
    runs
}

/// Format an integer with thousands separators (e.g. `1234567` -> `"1,234,567"`).
pub fn format_number(n: usize) -> String {
    if n < 1_000 {
        return n.to_string();
    }

    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Given a list of manifest file names and a retention count, decide which
/// files to delete so that at most `keep` remain.
///
/// This is the pure decision half of manifest pruning; the caller performs the
/// filesystem removal. Manifests are named `delete-<nanos>.toml`, so a
/// lexicographic sort on the file name is also a chronological sort (fixed-width
/// nanosecond timestamps). The most recent `keep` names are retained; every
/// older name is returned for deletion.
///
/// Input order does not matter — the function sorts internally. Names that do
/// not match the expected `delete-*.toml` shape are ignored (never returned for
/// deletion) so unrelated files in the directory are left untouched.
pub fn manifests_to_prune(mut file_names: Vec<String>, keep: usize) -> Vec<String> {
    file_names.retain(|name| name.starts_with("delete-") && name.ends_with(".toml"));

    if file_names.len() <= keep {
        return Vec::new();
    }

    // Ascending sort: oldest first. Because the timestamp is a zero-padded-ish
    // nanosecond count of equal-or-growing width, lexicographic order tracks
    // chronological order closely; equal widths sort exactly. We additionally
    // sort by length first so shorter (older/smaller) numbers precede longer
    // ones regardless of leading digits.
    file_names.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

    let prune_count = file_names.len() - keep;
    file_names.into_iter().take(prune_count).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(path: &str, size: i64, at: i64) -> HistoryEntry {
        HistoryEntry {
            path: path.to_string(),
            size_bytes: size,
            deleted_at: at,
        }
    }

    #[test]
    fn format_number_small_values_unchanged() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(42), "42");
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn format_number_inserts_separators() {
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(12_345), "12,345");
        assert_eq!(format_number(1_234_567), "1,234,567");
    }

    #[test]
    fn group_into_runs_empty() {
        assert!(group_into_runs(Vec::new()).is_empty());
    }

    #[test]
    fn group_into_runs_folds_close_deletions() {
        // Descending by deleted_at, all within the window.
        let records = vec![rec("/a", 10, 1_000), rec("/b", 20, 990), rec("/c", 5, 980)];
        let runs = group_into_runs(records);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].entries.len(), 3);
        assert_eq!(runs[0].total_bytes, 35);
        assert_eq!(runs[0].started_at, 980);
    }

    #[test]
    fn group_into_runs_splits_distant_deletions() {
        let records = vec![
            rec("/a", 10, 1_000),
            rec("/b", 20, 500), // > 60s away -> new run
        ];
        let runs = group_into_runs(records);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].total_bytes, 10);
        assert_eq!(runs[1].total_bytes, 20);
    }

    #[test]
    fn manifests_to_prune_under_limit_keeps_all() {
        let names = vec!["delete-100.toml".into(), "delete-200.toml".into()];
        assert!(manifests_to_prune(names, 50).is_empty());
    }

    #[test]
    fn manifests_to_prune_removes_oldest_over_limit() {
        // 5 manifests, keep 2 -> prune the 3 oldest.
        let names: Vec<String> = (0..5)
            .map(|i| format!("delete-{}.toml", 1000 + i))
            .collect();
        let mut pruned = manifests_to_prune(names, 2);
        pruned.sort();
        assert_eq!(
            pruned,
            vec![
                "delete-1000.toml".to_string(),
                "delete-1001.toml".to_string(),
                "delete-1002.toml".to_string(),
            ]
        );
    }

    #[test]
    fn manifests_to_prune_sorts_by_width_then_lex() {
        // A shorter (older) timestamp must be pruned before a longer (newer) one
        // even if its leading digit is larger.
        let names = vec![
            "delete-9999.toml".into(),     // 4 digits -> older
            "delete-10000000.toml".into(), // 8 digits -> newer
            "delete-10000001.toml".into(),
        ];
        let pruned = manifests_to_prune(names, 2);
        assert_eq!(pruned, vec!["delete-9999.toml".to_string()]);
    }

    #[test]
    fn manifests_to_prune_ignores_unrelated_files() {
        let names = vec![
            "delete-1.toml".into(),
            "delete-2.toml".into(),
            "delete-3.toml".into(),
            "README.md".into(),
            "notes.txt".into(),
        ];
        let pruned = manifests_to_prune(names, 1);
        // Only delete-* files considered; keep 1 newest -> prune delete-1, delete-2.
        let mut sorted = pruned.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["delete-1.toml".to_string(), "delete-2.toml".to_string()]
        );
        assert!(!pruned.iter().any(|n| n == "README.md" || n == "notes.txt"));
    }
}
