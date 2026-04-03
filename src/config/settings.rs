use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub paths: PathSettings,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub keybindings: KeybindingSettings,
    #[serde(default)]
    pub visualizer: VisualizerSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    #[serde(default = "default_log_file")]
    pub log_file: String,
    #[serde(default = "default_preload_count")]
    pub preload_count: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathSettings {
    #[serde(default = "default_cookies_path")]
    pub cookies: String,
    #[serde(default = "default_playlists_path")]
    pub playlists: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSettings {
    #[serde(default = "default_theme_name")]
    pub name: String,
    #[serde(default = "default_primary_color")]
    pub primary: String,
    #[serde(default = "default_secondary_color")]
    pub secondary: String,
    #[serde(default = "default_visualizer_colors")]
    pub visualizer_colors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingSettings {
    #[serde(default = "default_play_pause")]
    pub play_pause: String,
    #[serde(default = "default_next")]
    pub next: String,
    #[serde(default = "default_previous")]
    pub previous: String,
    #[serde(default = "default_volume_up")]
    pub volume_up: String,
    #[serde(default = "default_volume_down")]
    pub volume_down: String,
    #[serde(default = "default_shuffle")]
    pub shuffle: String,
    #[serde(default = "default_repeat")]
    pub repeat: String,
    #[serde(default = "default_favorite")]
    pub favorite: String,
    #[serde(default = "default_quit")]
    pub quit: String,
    #[serde(default = "default_search")]
    pub search: String,
    #[serde(default = "default_queue")]
    pub queue: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizerSettings {
    #[serde(default = "default_bars")]
    pub bars: u16,
    #[serde(default = "default_fps")]
    pub fps: u16,
    #[serde(default = "default_decay")]
    pub decay: f64,
}

// Default value functions
fn default_cache_dir() -> String {
    "~/.termtube/cache".to_string()
}
fn default_log_file() -> String {
    "~/.termtube/termtube.log".to_string()
}
fn default_preload_count() -> u8 {
    5
}
fn default_cookies_path() -> String {
    "~/.termtube/cookies.txt".to_string()
}
fn default_playlists_path() -> String {
    "~/.termtube/playlist.txt".to_string()
}
fn default_theme_name() -> String {
    "default".to_string()
}
fn default_primary_color() -> String {
    "#61afef".to_string()
}
fn default_secondary_color() -> String {
    "#c678dd".to_string()
}
fn default_visualizer_colors() -> Vec<String> {
    vec![
        "#98c379".to_string(),
        "#e5c07b".to_string(),
        "#e06c75".to_string(),
    ]
}
fn default_play_pause() -> String {
    "space".to_string()
}
fn default_next() -> String {
    "n".to_string()
}
fn default_previous() -> String {
    "p".to_string()
}
fn default_volume_up() -> String {
    "+".to_string()
}
fn default_volume_down() -> String {
    "-".to_string()
}
fn default_shuffle() -> String {
    "s".to_string()
}
fn default_repeat() -> String {
    "r".to_string()
}
fn default_favorite() -> String {
    "f".to_string()
}
fn default_quit() -> String {
    "q".to_string()
}
fn default_search() -> String {
    "/".to_string()
}
fn default_queue() -> String {
    "Q".to_string()
}
fn default_bars() -> u16 {
    24
}
fn default_fps() -> u16 {
    30
}
fn default_decay() -> f64 {
    0.85
}

// Trait impls for Default
impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            cache_dir: default_cache_dir(),
            log_file: default_log_file(),
            preload_count: default_preload_count(),
        }
    }
}

impl Default for PathSettings {
    fn default() -> Self {
        Self {
            cookies: default_cookies_path(),
            playlists: default_playlists_path(),
        }
    }
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
            primary: default_primary_color(),
            secondary: default_secondary_color(),
            visualizer_colors: default_visualizer_colors(),
        }
    }
}

impl Default for KeybindingSettings {
    fn default() -> Self {
        Self {
            play_pause: default_play_pause(),
            next: default_next(),
            previous: default_previous(),
            volume_up: default_volume_up(),
            volume_down: default_volume_down(),
            shuffle: default_shuffle(),
            repeat: default_repeat(),
            favorite: default_favorite(),
            quit: default_quit(),
            search: default_search(),
            queue: default_queue(),
        }
    }
}

impl Default for VisualizerSettings {
    fn default() -> Self {
        Self {
            bars: default_bars(),
            fps: default_fps(),
            decay: default_decay(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            general: GeneralSettings::default(),
            paths: PathSettings::default(),
            theme: ThemeSettings::default(),
            keybindings: KeybindingSettings::default(),
            visualizer: VisualizerSettings::default(),
        }
    }
}

impl Settings {
    /// Load settings from a TOML file. Missing fields use defaults.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let content = fs::read_to_string(path)?;
        Self::from_str(&content)
    }

    /// Parse settings from a TOML string. Missing fields use defaults.
    pub fn from_str(content: &str) -> Result<Self, SettingsError> {
        let settings: Settings = toml::from_str(content)?;
        Ok(settings)
    }

    /// Expand shell paths like ~ to absolute paths.
    pub fn resolve_paths(&mut self) {
        self.general.cache_dir = shellexpand::tilde(&self.general.cache_dir).to_string();
        self.general.log_file = shellexpand::tilde(&self.general.log_file).to_string();
        self.paths.cookies = shellexpand::tilde(&self.paths.cookies).to_string();
        self.paths.playlists = shellexpand::tilde(&self.paths.playlists).to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.general.preload_count, 5);
        assert_eq!(settings.visualizer.bars, 24);
        assert_eq!(settings.keybindings.quit, "q");
        assert_eq!(settings.theme.visualizer_colors.len(), 3);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r##"
[general]
cache_dir = "~/.termtube/cache"
log_file = "~/.termtube/termtube.log"
preload_count = 2

[paths]
cookies = "./cookies.txt"
playlists = "./playlist.txt"

[theme]
name = "default"
primary = "#61afef"
secondary = "#c678dd"
visualizer_colors = ["#98c379", "#e5c07b", "#e06c75"]

[keybindings]
play_pause = "space"
next = "n"
previous = "p"
volume_up = "+"
volume_down = "-"
shuffle = "s"
repeat = "r"
favorite = "f"
quit = "q"
search = "/"
queue = "Q"

[visualizer]
bars = 24
fps = 30
decay = 0.85
"##;
        let settings = Settings::from_str(toml).unwrap();
        assert_eq!(settings.general.preload_count, 2);
        assert_eq!(settings.visualizer.fps, 30);
        assert_eq!(settings.theme.primary, "#61afef");
    }

    #[test]
    fn test_parse_partial_config() {
        let toml = r#"
[general]
preload_count = 3

[visualizer]
bars = 32
"#;
        let settings = Settings::from_str(toml).unwrap();
        assert_eq!(settings.general.preload_count, 3);
        assert_eq!(settings.visualizer.bars, 32);
        // Defaults for missing fields
        assert_eq!(settings.keybindings.quit, "q");
        assert_eq!(settings.theme.name, "default");
    }

    #[test]
    fn test_parse_empty_config() {
        let settings = Settings::from_str("").unwrap();
        assert_eq!(settings.general.preload_count, 5);
        assert_eq!(settings.visualizer.bars, 24);
    }

    #[test]
    fn test_resolve_tilde_paths() {
        let mut settings = Settings::default();
        settings.resolve_paths();
        assert!(!settings.general.cache_dir.contains('~'));
        assert!(settings.general.cache_dir.starts_with('/'));
    }

    #[test]
    fn test_invalid_toml() {
        let result = Settings::from_str("this is not valid [toml");
        assert!(result.is_err());
    }
}
