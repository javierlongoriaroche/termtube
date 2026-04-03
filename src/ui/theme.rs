use ratatui::style::Color;

use crate::config::settings::ThemeSettings;

/// Resolved theme colors for rendering.
#[derive(Debug, Clone)]
pub struct Theme {
    pub primary: Color,
    pub secondary: Color,
    pub bg: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub border: Color,
    pub border_focused: Color,
    pub progress_bar: Color,
    pub visualizer_colors: Vec<Color>,
}

impl Theme {
    pub fn from_settings(settings: &ThemeSettings) -> Self {
        Self {
            primary: parse_hex_color(&settings.primary).unwrap_or(Color::LightBlue),
            secondary: parse_hex_color(&settings.secondary).unwrap_or(Color::Magenta),
            bg: Color::Reset,
            fg: Color::White,
            fg_dim: Color::DarkGray,
            highlight_bg: parse_hex_color(&settings.primary).unwrap_or(Color::LightBlue),
            highlight_fg: Color::Black,
            border: Color::DarkGray,
            border_focused: parse_hex_color(&settings.primary).unwrap_or(Color::LightBlue),
            progress_bar: parse_hex_color(&settings.primary).unwrap_or(Color::LightBlue),
            visualizer_colors: settings
                .visualizer_colors
                .iter()
                .filter_map(|c| parse_hex_color(c))
                .collect(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            primary: Color::LightBlue,
            secondary: Color::Magenta,
            bg: Color::Reset,
            fg: Color::White,
            fg_dim: Color::DarkGray,
            highlight_bg: Color::LightBlue,
            highlight_fg: Color::Black,
            border: Color::DarkGray,
            border_focused: Color::LightBlue,
            progress_bar: Color::LightBlue,
            visualizer_colors: vec![Color::Green, Color::Yellow, Color::Red],
        }
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#61afef"), Some(Color::Rgb(0x61, 0xaf, 0xef)));
        assert_eq!(parse_hex_color("c678dd"), Some(Color::Rgb(0xc6, 0x78, 0xdd)));
        assert_eq!(parse_hex_color("invalid"), None);
        assert_eq!(parse_hex_color("#fff"), None);
    }

    #[test]
    fn test_theme_from_settings() {
        let settings = ThemeSettings::default();
        let theme = Theme::from_settings(&settings);
        assert_eq!(theme.visualizer_colors.len(), 3);
    }
}
