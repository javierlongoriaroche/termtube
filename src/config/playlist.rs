use std::fmt;
use std::fs;
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlaylistConfigError {
    #[error("failed to read playlist file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid line format at line {line}: expected 'name|url', got '{content}'")]
    InvalidFormat { line: usize, content: String },
    #[error("empty playlist name at line {0}")]
    EmptyName(usize),
    #[error("invalid URL at line {line}: '{url}'")]
    InvalidUrl { line: usize, url: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistEntry {
    pub name: String,
    pub url: String,
}

impl fmt::Display for PlaylistEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{}", self.name, self.url)
    }
}

/// Parse a playlist.txt file with format `name|url` per line.
/// Blank lines and lines starting with `#` are ignored.
pub fn parse_playlist_file(path: &Path) -> Result<Vec<PlaylistEntry>, PlaylistConfigError> {
    let content = fs::read_to_string(path)?;
    parse_playlist_content(&content)
}

pub fn parse_playlist_content(content: &str) -> Result<Vec<PlaylistEntry>, PlaylistConfigError> {
    let mut entries = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();
        let line_num = idx + 1;

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, '|').collect();
        if parts.len() != 2 {
            return Err(PlaylistConfigError::InvalidFormat {
                line: line_num,
                content: line.to_string(),
            });
        }

        let name = parts[0].trim();
        let url = parts[1].trim();

        if name.is_empty() {
            return Err(PlaylistConfigError::EmptyName(line_num));
        }

        if !url.starts_with("https://www.youtube.com/")
            && !url.starts_with("https://youtube.com/")
            && !url.starts_with("https://music.youtube.com/")
        {
            return Err(PlaylistConfigError::InvalidUrl {
                line: line_num,
                url: url.to_string(),
            });
        }

        entries.push(PlaylistEntry {
            name: name.to_string(),
            url: url.to_string(),
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_playlist() {
        let content = "\
lofi-beats|https://www.youtube.com/playlist?list=PLxxxxxx
synthwave|https://music.youtube.com/playlist?list=PLyyyyyy
";
        let entries = parse_playlist_content(content).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "lofi-beats");
        assert_eq!(
            entries[0].url,
            "https://www.youtube.com/playlist?list=PLxxxxxx"
        );
        assert_eq!(entries[1].name, "synthwave");
        assert_eq!(
            entries[1].url,
            "https://music.youtube.com/playlist?list=PLyyyyyy"
        );
    }

    #[test]
    fn test_skip_comments_and_blanks() {
        let content = "\
# This is a comment
lofi-beats|https://www.youtube.com/playlist?list=PLxxxxxx

# Another comment

synthwave|https://music.youtube.com/playlist?list=PLyyyyyy
";
        let entries = parse_playlist_content(content).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_invalid_format_no_pipe() {
        let content = "lofi-beats https://www.youtube.com/playlist?list=PLxxxxxx";
        let result = parse_playlist_content(content);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlaylistConfigError::InvalidFormat { .. }
        ));
    }

    #[test]
    fn test_empty_name() {
        let content = "|https://www.youtube.com/playlist?list=PLxxxxxx";
        let result = parse_playlist_content(content);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlaylistConfigError::EmptyName(_)
        ));
    }

    #[test]
    fn test_invalid_url() {
        let content = "lofi|https://example.com/not-youtube";
        let result = parse_playlist_content(content);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlaylistConfigError::InvalidUrl { .. }
        ));
    }

    #[test]
    fn test_display_format() {
        let entry = PlaylistEntry {
            name: "lofi".to_string(),
            url: "https://www.youtube.com/playlist?list=PL123".to_string(),
        };
        assert_eq!(
            entry.to_string(),
            "lofi|https://www.youtube.com/playlist?list=PL123"
        );
    }

    #[test]
    fn test_empty_content() {
        let entries = parse_playlist_content("").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_whitespace_trimming() {
        let content = "  lofi-beats  |  https://www.youtube.com/playlist?list=PLxxxxxx  ";
        let entries = parse_playlist_content(content).unwrap();
        assert_eq!(entries[0].name, "lofi-beats");
        assert_eq!(
            entries[0].url,
            "https://www.youtube.com/playlist?list=PLxxxxxx"
        );
    }
}
