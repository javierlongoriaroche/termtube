use std::fs;
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CookiesError {
    #[error("cookies file not found: {0}")]
    NotFound(String),
    #[error("failed to read cookies file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid cookies format: missing Netscape header")]
    InvalidFormat,
}

/// Validate that a cookies.txt file exists and has the Netscape format header.
pub fn validate_cookies_file(path: &Path) -> Result<(), CookiesError> {
    if !path.exists() {
        return Err(CookiesError::NotFound(path.display().to_string()));
    }

    let content = fs::read_to_string(path)?;
    validate_cookies_content(&content)
}

pub fn validate_cookies_content(content: &str) -> Result<(), CookiesError> {
    // Netscape cookies files typically start with this header line
    // or contain tab-separated fields with domain entries.
    // We check for common markers.
    let has_netscape_header = content.lines().any(|line| {
        line.contains("Netscape HTTP Cookie File") || line.contains("HTTP Cookie File")
    });

    if has_netscape_header {
        return Ok(());
    }

    // Fallback: check if there are valid cookie lines (tab-separated, 7 fields)
    let has_cookie_lines = content.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && !line.starts_with('#') && line.split('\t').count() >= 7
    });

    if has_cookie_lines {
        return Ok(());
    }

    Err(CookiesError::InvalidFormat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_netscape_header() {
        let content = "# Netscape HTTP Cookie File\n\
            .youtube.com\tTRUE\t/\tTRUE\t0\tSID\tabc123\n";
        assert!(validate_cookies_content(content).is_ok());
    }

    #[test]
    fn test_valid_cookie_lines_no_header() {
        let content = ".youtube.com\tTRUE\t/\tTRUE\t0\tSID\tabc123\n";
        assert!(validate_cookies_content(content).is_ok());
    }

    #[test]
    fn test_empty_file() {
        let content = "";
        assert!(validate_cookies_content(content).is_err());
    }

    #[test]
    fn test_invalid_format() {
        let content = "this is not a cookies file\njust some random text\n";
        assert!(validate_cookies_content(content).is_err());
    }

    #[test]
    fn test_only_comments() {
        let content = "# Just a comment\n# Another comment\n";
        assert!(validate_cookies_content(content).is_err());
    }

    #[test]
    fn test_file_not_found() {
        let result = validate_cookies_file(Path::new("/nonexistent/cookies.txt"));
        assert!(matches!(result, Err(CookiesError::NotFound(_))));
    }
}
