use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// User configuration loaded from `~/.config/termtube/config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub account: AccountSettings,
    #[serde(default)]
    pub mpv: MpvSettings,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub meter: MeterSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralSettings {
    /// Directory for cookies, logs and caches.
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    /// Initial volume (0-100).
    #[serde(default = "default_volume")]
    pub volume: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountSettings {
    /// Browser used by `termtube login` to read cookies (as understood by yt-dlp).
    #[serde(default = "default_browser")]
    pub browser: String,
    /// Index of the Google account inside the browser session (`X-Goog-AuthUser`).
    #[serde(default)]
    pub authuser: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MpvSettings {
    #[serde(default = "default_mpv_path")]
    pub path: String,
    #[serde(default = "default_ytdlp_path")]
    pub ytdlp_path: String,
    /// Extra arguments appended to the mpv command line.
    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeSettings {
    #[serde(default = "default_primary_color")]
    pub primary: String,
    #[serde(default = "default_secondary_color")]
    pub secondary: String,
    #[serde(default = "default_meter_colors")]
    pub meter_colors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeterSettings {
    /// Show the stereo level meter in the player bar.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_data_dir() -> String {
    "~/.termtube".to_string()
}
fn default_volume() -> u8 {
    70
}
fn default_browser() -> String {
    "brave".to_string()
}
fn default_mpv_path() -> String {
    "mpv".to_string()
}
fn default_ytdlp_path() -> String {
    "yt-dlp".to_string()
}
fn default_primary_color() -> String {
    "#61afef".to_string()
}
fn default_secondary_color() -> String {
    "#c678dd".to_string()
}
fn default_meter_colors() -> Vec<String> {
    vec!["#98c379".into(), "#e5c07b".into(), "#e06c75".into()]
}
fn default_true() -> bool {
    true
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            volume: default_volume(),
        }
    }
}
impl Default for AccountSettings {
    fn default() -> Self {
        Self {
            browser: default_browser(),
            authuser: 0,
        }
    }
}
impl Default for MpvSettings {
    fn default() -> Self {
        Self {
            path: default_mpv_path(),
            ytdlp_path: default_ytdlp_path(),
            extra_args: vec![],
        }
    }
}
impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            primary: default_primary_color(),
            secondary: default_secondary_color(),
            meter_colors: default_meter_colors(),
        }
    }
}
impl Default for MeterSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Settings {
    /// Parses settings from TOML text; missing sections fall back to defaults.
    pub fn from_toml(content: &str) -> Result<Self, SettingsError> {
        Ok(toml::from_str(content)?)
    }

    /// Loads settings from `path`, or returns defaults when the file does not exist.
    pub fn load_or_default(path: &Path) -> Result<Self, SettingsError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::from_toml(&fs::read_to_string(path)?)
    }

    /// Writes settings to `path`, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Data directory with `~` expanded.
    pub fn data_dir(&self) -> PathBuf {
        expand(&self.general.data_dir)
    }

    pub fn cookies_path(&self) -> PathBuf {
        self.data_dir().join("cookies.txt")
    }

    pub fn log_path(&self) -> PathBuf {
        self.data_dir().join("termtube.log")
    }
}

/// Default location of the config file.
pub fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("termtube")
        .join("config.toml")
}

/// Expands `~` and environment variables in a path.
pub fn expand(path: &str) -> PathBuf {
    PathBuf::from(
        shellexpand::full(path)
            .map(|p| p.into_owned())
            .unwrap_or_else(|_| path.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_toml_uses_defaults() {
        let s = Settings::from_toml("").unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.account.browser, "brave");
        assert!(s.meter.enabled);
    }

    #[test]
    fn test_partial_toml_overrides_only_given_fields() {
        let s = Settings::from_toml("[account]\nauthuser = 1\n[meter]\nenabled = false\n").unwrap();
        assert_eq!(s.account.authuser, 1);
        assert_eq!(s.account.browser, "brave");
        assert!(!s.meter.enabled);
        assert_eq!(s.general.volume, 70);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut s = Settings::default();
        s.account.authuser = 2;
        s.save(&path).unwrap();
        assert_eq!(Settings::load_or_default(&path).unwrap(), s);
    }

    #[test]
    fn test_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::load_or_default(&dir.path().join("none.toml")).unwrap();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn test_paths_are_inside_data_dir() {
        let mut s = Settings::default();
        s.general.data_dir = "/tmp/tt".into();
        assert_eq!(s.cookies_path(), PathBuf::from("/tmp/tt/cookies.txt"));
        assert_eq!(s.log_path(), PathBuf::from("/tmp/tt/termtube.log"));
    }
}
