//! TOML-based application configuration with validated defaults.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level application configuration, read from `~/.config/artifact/config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// User-interface settings (window size, etc.).
    #[serde(default)]
    pub ui: UiConfig,
    /// Logging output settings.
    #[serde(default)]
    pub logging: LoggingConfig,
    /// Database storage settings.
    #[serde(default)]
    pub database: DatabaseConfig,
    /// Filesystem scan settings.
    #[serde(default)]
    pub scan: ScanConfig,
}

/// How detected artifact directories are removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    /// Move to the OS recycle bin / trash (default; recoverable).
    #[default]
    Trash,
    /// Permanently delete without recovery option.
    Permanent,
}

/// Settings that control which artifacts are found and how they are handled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    /// Optional whitelist of language names to scan. `None` means scan all.
    #[serde(default)]
    pub enabled_languages: Option<Vec<String>>,
    /// Whether deleted directories go to trash or are permanently removed.
    #[serde(default)]
    pub delete_mode: DeleteMode,
    /// Maximum number of results to display in the results panel.
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// When `true`, only show artifacts whose project root no longer exists.
    #[serde(default)]
    pub show_orphaned_only: bool,
}

fn default_max_results() -> usize {
    10_000
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            enabled_languages: None,
            delete_mode: DeleteMode::default(),
            max_results: default_max_results(),
            show_orphaned_only: false,
        }
    }
}

/// User-interface geometry settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Initial window width in logical pixels (clamped to [`MIN_WINDOW_WIDTH`]).
    #[serde(default = "default_window_width")]
    pub window_width: f32,
    /// Initial window height in logical pixels (clamped to [`MIN_WINDOW_HEIGHT`]).
    #[serde(default = "default_window_height")]
    pub window_height: f32,
}

/// Minimum window width in logical pixels.
pub const MIN_WINDOW_WIDTH: f32 = 1024.0;
/// Minimum window height in logical pixels.
pub const MIN_WINDOW_HEIGHT: f32 = 720.0;

fn default_window_width() -> f32 {
    1280.0
}

fn default_window_height() -> f32 {
    860.0
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
        }
    }
}

/// Settings that control how tracing events are captured and written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Minimum tracing level (`error`, `warn`, `info`, `debug`, `trace`).
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Write log events to a rolling file in the data directory.
    #[serde(default)]
    pub log_to_file: bool,
    /// Write log events to standard output.
    #[serde(default = "default_true")]
    pub log_to_stdout: bool,
    /// Emit log lines as JSON objects instead of human-readable text.
    #[serde(default)]
    pub json_format: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            log_to_file: true,
            log_to_stdout: true,
            json_format: false,
        }
    }
}

/// Settings that control where the redb database is stored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Override the default data directory path. `None` uses the platform data dir.
    #[serde(default)]
    pub data_dir: Option<String>,
}

impl AppConfig {
    /// Clamp all numeric fields to sane ranges and reset invalid string values
    /// to their defaults. Called after loading from TOML so that a hand-edited
    /// config cannot crash the application.
    pub fn apply_constraints(&mut self) {
        self.ui.window_width = self.ui.window_width.clamp(MIN_WINDOW_WIDTH, 16_000.0);
        self.ui.window_height = self.ui.window_height.clamp(MIN_WINDOW_HEIGHT, 8_000.0);

        // Log level: reset to "info" if the supplied string is not recognised.
        const VALID_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];
        if !VALID_LEVELS.contains(&self.logging.log_level.as_str()) {
            self.logging.log_level = "info".to_string();
        }

        // Max results: must be at least 1.
        if self.scan.max_results == 0 {
            self.scan.max_results = 1;
        }
    }

    /// Load configuration from the platform config directory.
    ///
    /// Returns `Ok(AppConfig::default())` if no config file exists yet.
    /// Constraint-clamping ([`AppConfig::apply_constraints`]) is applied after
    /// a successful parse.
    ///
    /// A malformed config file is **non-fatal**: it is backed up alongside the
    /// original (with a `.corrupt` suffix) and defaults are returned, so a
    /// hand-edited or partially-written config can never prevent startup.
    pub fn load() -> crate::error::Result<Self> {
        Self::load_from(&Self::config_path())
    }

    /// Load configuration from an explicit path. See [`AppConfig::load`].
    ///
    /// Split out from [`AppConfig::load`] so the load logic can be exercised in
    /// tests against a temporary directory without touching the real platform
    /// config location.
    fn load_from(config_path: &std::path::Path) -> crate::error::Result<Self> {
        if !config_path.exists() {
            return Ok(AppConfig::default());
        }

        let content = std::fs::read_to_string(config_path)?;
        match toml::from_str::<AppConfig>(&content) {
            Ok(mut config) => {
                config.apply_constraints();
                Ok(config)
            }
            Err(e) => {
                // Non-fatal: preserve the malformed file for the user to inspect
                // and fall back to defaults so startup can proceed. We use a
                // fixed `.corrupt` suffix (no clock/random access in the lib).
                let backup_path = Self::corrupt_backup_path(config_path);
                if let Err(rename_err) = std::fs::rename(config_path, &backup_path) {
                    tracing::warn!(
                        error = %rename_err,
                        original = %config_path.display(),
                        "failed to back up malformed config file"
                    );
                } else {
                    tracing::warn!(
                        parse_error = %e,
                        backup = %backup_path.display(),
                        "config file was malformed; backed it up and loaded defaults"
                    );
                }
                Ok(AppConfig::default())
            }
        }
    }

    /// Path used to preserve a malformed config file (`<name>.corrupt`).
    fn corrupt_backup_path(config_path: &std::path::Path) -> PathBuf {
        let mut file_name = config_path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| std::ffi::OsString::from("config.toml"));
        file_name.push(".corrupt");
        config_path.with_file_name(file_name)
    }

    /// Serialize this configuration to TOML and write it to disk.
    ///
    /// Creates the config directory if it does not already exist.
    ///
    /// The write is **atomic**: the serialized content is first written to a
    /// temporary file in the same directory and then `rename`d into place, so a
    /// crash or full disk mid-write can never leave a truncated `config.toml`.
    pub fn save(&self) -> crate::error::Result<()> {
        self.save_to(&Self::config_dir())
    }

    /// Serialize this configuration into `config.toml` inside `config_dir`.
    ///
    /// Split out from [`AppConfig::save`] so the atomic-write logic can be
    /// tested against a temporary directory. Writes to a temp file in the same
    /// directory then atomically renames it over the destination.
    fn save_to(&self, config_dir: &std::path::Path) -> crate::error::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::error::ArtifactError::Configuration(e.to_string()))?;

        let dest = config_dir.join("config.toml");
        let tmp = config_dir.join("config.toml.tmp");

        // Write to a temp file in the SAME directory (so the rename is atomic
        // on the same filesystem), then rename over the destination.
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &dest)?;
        Ok(())
    }

    /// Return the directory where rolling log files are written.
    pub fn get_log_dir(&self) -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("artifact")
            .join("logs")
    }

    /// Return the configured log level string (e.g. `"info"`).
    pub fn get_log_level(&self) -> String {
        self.logging.log_level.clone()
    }

    /// Return the path to the redb database directory.
    pub fn get_db_path(&self) -> PathBuf {
        if let Some(ref dir) = self.database.data_dir {
            PathBuf::from(dir)
        } else {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("artifact")
                .join("db")
        }
    }

    fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("artifact")
    }

    fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = AppConfig::default();
        assert_eq!(config.ui.window_width, 1280.0);
        assert_eq!(config.ui.window_height, 860.0);
        assert_eq!(config.logging.log_level, "info");
        assert!(config.logging.log_to_file);
        assert!(config.logging.log_to_stdout);
        assert_eq!(config.scan.max_results, 10_000);
    }

    #[test]
    fn scan_config_default_delete_mode_is_trash() {
        let config = ScanConfig::default();
        assert_eq!(config.delete_mode, DeleteMode::Trash);
    }

    #[test]
    fn apply_constraints_clamps_window_dimensions() {
        let mut config = AppConfig::default();
        config.ui.window_width = -100.0;
        config.ui.window_height = 99999.0;
        config.apply_constraints();
        assert_eq!(config.ui.window_width, MIN_WINDOW_WIDTH);
        assert_eq!(config.ui.window_height, 8_000.0);
    }

    #[test]
    fn apply_constraints_resets_invalid_log_level() {
        let mut config = AppConfig::default();
        config.logging.log_level = "INVALID_LEVEL".to_string();
        config.apply_constraints();
        assert_eq!(config.logging.log_level, "info");
    }

    #[test]
    fn apply_constraints_accepts_valid_log_levels() {
        for level in &["error", "warn", "info", "debug", "trace"] {
            let mut config = AppConfig::default();
            config.logging.log_level = level.to_string();
            config.apply_constraints();
            assert_eq!(&config.logging.log_level, level);
        }
    }

    /// Create a unique temp directory for a test, returning its path.
    /// Uses only stdlib + a thread-local counter (no external test-only crates).
    fn unique_temp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "artifact-config-test-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = unique_temp_dir("roundtrip");

        let mut config = AppConfig::default();
        config.ui.window_width = 1500.0;
        config.scan.delete_mode = DeleteMode::Permanent;
        config.scan.max_results = 42;
        config.logging.json_format = true;

        config.save_to(&dir).expect("save_to failed");

        let loaded = AppConfig::load_from(&dir.join("config.toml")).expect("load_from failed");
        assert_eq!(loaded.ui.window_width, 1500.0);
        assert_eq!(loaded.scan.delete_mode, DeleteMode::Permanent);
        assert_eq!(loaded.scan.max_results, 42);
        assert!(loaded.logging.json_format);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_to_is_atomic_and_leaves_no_temp_file() {
        let dir = unique_temp_dir("atomic");
        AppConfig::default().save_to(&dir).expect("save_to failed");

        assert!(dir.join("config.toml").exists(), "config.toml should exist");
        assert!(
            !dir.join("config.toml.tmp").exists(),
            "temp file should be renamed away after save"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = unique_temp_dir("missing");
        let loaded = AppConfig::load_from(&dir.join("config.toml")).expect("load_from failed");
        assert_eq!(loaded.ui.window_width, AppConfig::default().ui.window_width);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_config_loads_default_and_is_backed_up() {
        let dir = unique_temp_dir("corrupt");
        let config_path = dir.join("config.toml");
        std::fs::write(&config_path, "this is not = valid toml [[[").expect("write bad config");

        let loaded = AppConfig::load_from(&config_path).expect("load should not error");

        // Fell back to defaults.
        assert_eq!(loaded.ui.window_width, AppConfig::default().ui.window_width);
        // Malformed file preserved as backup, original removed.
        assert!(
            !config_path.exists(),
            "malformed original should have been renamed away"
        );
        assert!(
            dir.join("config.toml.corrupt").exists(),
            "backup of malformed config should exist"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_minimal_toml() {
        let toml = r#"
[ui]
window_width = 1400.0

[scan]
delete_mode = "permanent"
"#;
        let config: AppConfig = toml::from_str(toml).expect("parse failed");
        assert_eq!(config.ui.window_width, 1400.0);
        assert_eq!(config.scan.delete_mode, DeleteMode::Permanent);
        // Unset fields get defaults
        assert_eq!(config.ui.window_height, 860.0);
    }
}
