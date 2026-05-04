// file: src/config.rs
// description: Application configuration with defaults

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub scan: ScanConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    #[default]
    Trash,
    Permanent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    #[serde(default)]
    pub enabled_languages: Option<Vec<String>>,
    #[serde(default)]
    pub delete_mode: DeleteMode,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_window_width")]
    pub window_width: f32,
    #[serde(default = "default_window_height")]
    pub window_height: f32,
}

pub const MIN_WINDOW_WIDTH: f32 = 880.0;
pub const MIN_WINDOW_HEIGHT: f32 = 640.0;

fn default_window_width() -> f32 {
    1200.0
}

fn default_window_height() -> f32 {
    760.0
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub log_to_file: bool,
    #[serde(default = "default_true")]
    pub log_to_stdout: bool,
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
            log_to_file: false,
            log_to_stdout: true,
            json_format: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatabaseConfig {
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

    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let mut config: AppConfig = toml::from_str(&content)?;
            config.apply_constraints();
            Ok(config)
        } else {
            Ok(AppConfig::default())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let config_dir = Self::config_dir();
        std::fs::create_dir_all(&config_dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), content)?;
        Ok(())
    }

    pub fn get_log_dir(&self) -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("artifact")
            .join("logs")
    }

    pub fn get_log_level(&self) -> String {
        self.logging.log_level.clone()
    }

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
        assert_eq!(config.ui.window_width, 1200.0);
        assert_eq!(config.ui.window_height, 760.0);
        assert_eq!(config.logging.log_level, "info");
        assert!(!config.logging.log_to_file);
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
        assert_eq!(config.ui.window_height, 760.0);
    }
}
