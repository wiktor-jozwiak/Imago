//! User configuration stored locally as JSON.
//!
//! Stored: last used settings (format, quality, auto mode, workers, output
//! folder, metadata, conflict policy, resizing, limits), scan options, theme,
//! language and ZIP preferences. Never stored: image data, file lists or any
//! history of processed files, personal data, telemetry.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cancel::CancelToken;
use crate::errors::{ImagoError, Result};
use crate::filesystem::ScanOptions;
use crate::filesystem::safe_write::{Placement, WriteOptions, write_atomic};
use crate::settings::OptimizeSettings;

/// Current configuration format version.
pub const CONFIG_VERSION: u32 = 1;
/// Configuration file name.
pub const CONFIG_FILE: &str = "config.json";
/// Largest configuration file accepted (protects against garbage files).
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Color theme of the user interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// Follow the operating system.
    #[default]
    System,
    /// Light theme.
    Light,
    /// Dark theme.
    Dark,
}

/// Interface preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct UiPreferences {
    /// Color theme.
    pub theme: Theme,
    /// Interface language (`pl`, `en`); `None` follows the system.
    pub language: Option<String>,
    /// The advanced settings panel is expanded.
    pub show_advanced: bool,
}

/// ZIP preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ZipPreferences {
    /// Default archive file name (without folder).
    pub file_name: String,
    /// Offer to create an archive automatically when a job finishes.
    pub create_after_job: bool,
}

impl Default for ZipPreferences {
    fn default() -> Self {
        ZipPreferences {
            file_name: "imago-optimized.zip".into(),
            create_after_job: false,
        }
    }
}

/// Everything Imago remembers between sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// Format version (for migrations).
    pub version: u32,
    /// Last used optimization settings.
    pub settings: OptimizeSettings,
    /// Folder scanning options.
    pub scan: ScanOptions,
    /// Interface preferences.
    pub ui: UiPreferences,
    /// ZIP preferences.
    pub zip: ZipPreferences,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            version: CONFIG_VERSION,
            settings: OptimizeSettings::default(),
            scan: ScanOptions::default(),
            ui: UiPreferences::default(),
            zip: ZipPreferences::default(),
        }
    }
}

/// Default location: the platform's per-user configuration folder
/// (`%APPDATA%\Imago`, `~/Library/Application Support/dev.imago.Imago`, `~/.config/imago`).
pub fn default_config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "imago", "Imago")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

/// Default location for log files.
pub fn default_log_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "imago", "Imago")
        .map(|dirs| dirs.data_local_dir().join("logs"))
}

/// Loads the configuration. A missing file gives the defaults.
pub fn load(path: &Path) -> Result<AppConfig> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppConfig::default());
        }
        Err(error) => return Err(ImagoError::io(path, error)),
    };
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(ImagoError::Config(
            "the configuration file is too large".into(),
        ));
    }
    let text = std::fs::read_to_string(path).map_err(|error| ImagoError::io(path, error))?;
    let config: AppConfig = serde_json::from_str(&text)
        .map_err(|error| ImagoError::Config(format!("cannot read {}: {error}", path.display())))?;
    Ok(migrate(config))
}

/// Loads the configuration, falling back to defaults on any problem (the
/// problem is returned so it can be logged and shown).
pub fn load_or_default(path: &Path) -> (AppConfig, Option<ImagoError>) {
    match load(path) {
        Ok(config) => (config, None),
        Err(error) => (AppConfig::default(), Some(error)),
    }
}

/// Saves the configuration atomically.
pub fn save(path: &Path, config: &AppConfig) -> Result<()> {
    config.settings.validate()?;
    let json =
        serde_json::to_vec_pretty(config).map_err(|error| ImagoError::Config(error.to_string()))?;
    let options = WriteOptions {
        placement: Placement::Replace,
        fsync: true,
    };
    write_atomic(path, &json, options, &CancelToken::new(), |temp| {
        let written = std::fs::read(temp).map_err(|error| ImagoError::io(temp, error))?;
        serde_json::from_slice::<AppConfig>(&written)
            .map(|_| ())
            .map_err(|error| ImagoError::VerificationFailed {
                path: temp.to_path_buf(),
                reason: error.to_string(),
            })
    })
}

/// Brings older configurations up to date and repairs invalid values.
fn migrate(mut config: AppConfig) -> AppConfig {
    if config.version > CONFIG_VERSION {
        tracing::warn!(
            version = config.version,
            "configuration from a newer Imago version; unknown fields ignored"
        );
    }
    config.version = CONFIG_VERSION;
    if let Err(error) = config.settings.validate() {
        tracing::warn!(%error, "invalid saved settings replaced with defaults");
        config.settings = OptimizeSettings::default();
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;
    use crate::settings::{FormatChoice, ImageFormat, Quality};

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load(&dir.path().join("config.json")).unwrap();
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn saved_configuration_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");
        let mut config = AppConfig::default();
        config.settings.format = FormatChoice::Fixed(ImageFormat::Avif);
        config.settings.compression.quality = Quality::new(70).unwrap();
        config.ui.theme = Theme::Dark;
        config.ui.language = Some("pl".into());
        config.scan.recursive = false;
        save(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap(), config);
    }

    #[test]
    fn corrupted_file_is_reported_and_defaults_are_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let (config, error) = load_or_default(&path);
        assert_eq!(config, AppConfig::default());
        assert_eq!(error.unwrap().code(), "config_error");
    }

    #[test]
    fn invalid_values_are_repaired_and_unknown_fields_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            br#"{"version": 99, "settings": {"workers": 0}, "somethingNew": true, "ui": {"theme": "dark"}}"#,
        )
        .unwrap();
        let config = load(&path).unwrap();
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.settings, OptimizeSettings::default());
        assert_eq!(config.ui.theme, Theme::Dark);
    }

    #[test]
    fn configuration_contains_no_file_history() {
        let json = serde_json::to_string(&AppConfig::default()).unwrap();
        assert!(!json.contains("files"));
        assert!(!json.contains("history"));
    }
}
