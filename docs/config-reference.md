# Config Reference

ARTIFACT loads TOML configuration from the platform config directory under `artifact/config.toml`.

```toml
[ui]
window_width = 1280.0
window_height = 860.0

[logging]
log_level = "info"
log_to_file = true
log_to_stdout = true
json_format = false

[database]
# data_dir = "/custom/path/artifact-db"

[scan]
delete_mode = "trash"
max_results = 10000
show_orphaned_only = false
# enabled_languages = ["Node", "Rust", "Python"]
```

## Fields

- `ui.window_width`: Initial window width. Values are clamped to a safe range.
- `ui.window_height`: Initial window height. Values are clamped to a safe range.
- `logging.log_level`: One of `error`, `warn`, `info`, `debug`, or `trace`.
- `logging.log_to_file`: Enables rolling local log files.
- `logging.log_to_stdout`: Emits logs to stdout.
- `logging.json_format`: Reserved for structured output preference.
- `database.data_dir`: Optional database directory override.
- `scan.delete_mode`: `trash` or `permanent`.
- `scan.max_results`: Maximum displayed artifacts after sorting largest-first.
- `scan.show_orphaned_only`: Filters results to orphan-capable matches.
- `scan.enabled_languages`: Optional language allowlist; omitted means all languages.
