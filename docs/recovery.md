# Recovery Guide

ARTIFACT stores all operational state locally. No scan or deletion history is sent to a remote service.

## Platform Paths

Default config path:

- macOS: `~/Library/Application Support/artifact/config.toml` or the platform config directory returned by `dirs`
- Linux: `~/.config/artifact/config.toml`
- Windows: `%APPDATA%\artifact\config.toml`

Default data path:

- macOS: `~/Library/Application Support/artifact/`
- Linux: `~/.local/share/artifact/`
- Windows: `%APPDATA%\artifact\`

Database directory:

- Default: `<platform data dir>/artifact/db/`
- Override: `[database].data_dir` in `config.toml`

Deletion manifests:

- Default: `<database directory>/deletion-manifests/*.toml`

Logs:

- Default: `<platform data dir>/artifact/logs/`
- File logging is controlled by `[logging].log_to_file`.

## Failure Recovery

- If config parsing fails, move `config.toml` aside and restart; defaults will be used.
- If history loading fails, preserve `artifact.redb` for inspection and start with a fresh database directory.
- If a trash operation fails, review the user-facing error and retained logs before retrying.
- If permanent deletion was used, inspect the deletion manifest to identify the exact intended paths and operation timestamp.

## Pre-Delete Evidence

Every cleanup writes a manifest before filesystem mutation starts. The manifest includes operation ID, timestamp, scan root, delete mode, total selected bytes, and selected paths. Treat this manifest as the authoritative record for support and recovery investigations.
