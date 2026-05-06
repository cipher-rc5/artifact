# Changelog

All notable changes to ARTIFACT are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.1.0] — 2026-05-05

### Added
- GPUI-based desktop UI with bento-box layout (dashboard, results, browser, history, settings views)
- Parallel filesystem scanner using jwalk with per-rule marker validation and cooperative cancellation
- 16 built-in artifact detection rules: Node.js, Rust, Python (venv + __pycache__), Next.js, Nuxt, Parcel, Gradle, .NET (bin/obj), Elixir, PHP (Composer), Xcode DerivedData, Terraform
- Orphaned artifact detection (artifacts whose parent project markers no longer exist)
- redb-backed deletion history with secondary indices for time-range and type-grouped queries
- Safe delete (move to Trash) and permanent delete modes
- File browser for selecting scan root directories
- Rolling log file output via tracing-appender with RUST_LOG-compatible level filtering
- TOML configuration file with sensible defaults and runtime constraint clamping
- Cross-platform distribution builds via cargo-zigbuild: macOS universal2, Linux x64/arm64 (glibc 2.17+), Windows x64
