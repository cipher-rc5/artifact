# Changelog

All notable changes to ARTIFACT are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Pre-delete manifests for cleanup operations.
- Deletion path revalidation before filesystem mutation.
- CI platform matrix, release checksums, SBOM generation, and provenance attestation hooks.
- macOS `.app` bundling, hardened-runtime code signing, notarization, and stapling in the release workflow (shipped as a `.dmg`).
- Windows Authenticode signing of the release executable via `signtool`.
- Fail-closed release policy: tagged releases fail if signing/notarization secrets are absent.
- Single-sourced advisory-ignore list (`audit.toml` + `scripts/advisories.json`) with a CI check that fails when a "Review by" date lapses.
- Production operation docs for recovery, safe defaults, config, privacy, accessibility, and release requirements.
- Scanner benchmark coverage.

### Changed

- Documented Rust 1.96 as the supported minimum toolchain.
- Result caps are applied after sorting so the largest artifacts are retained.
- Marker-required orphan matching is explicit per rule, keeping generic directory names conservative.
- Minimum window size lowered for better small-display usability.

## [0.1.0] — 2026-05-05

### Added

- GPUI-based desktop UI with bento-box layout (dashboard, results, browser, history, settings views)
- Parallel filesystem scanner using jwalk with per-rule marker validation and cooperative cancellation
- 16 built-in artifact detection rules: Node.js, Rust, Python (venv + **pycache**), Next.js, Nuxt, Parcel, Gradle, .NET (bin/obj), Elixir, PHP (Composer), Xcode DerivedData, Terraform
- Orphaned artifact detection (artifacts whose parent project markers no longer exist)
- redb-backed deletion history with secondary indices for time-range and type-grouped queries
- Safe delete (move to Trash) and permanent delete modes
- File browser for selecting scan root directories
- Rolling log file output via tracing-appender with RUST_LOG-compatible level filtering
- TOML configuration file with sensible defaults and runtime constraint clamping
- Cross-platform distribution builds via cargo-zigbuild: macOS universal2, Linux x64/arm64 (glibc 2.17+), Windows x64
