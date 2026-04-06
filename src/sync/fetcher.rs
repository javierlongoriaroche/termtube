use std::path::Path;
use std::process::Stdio;

use thiserror::Error;
use tokio::process::Command;

use crate::playlist::models::Song;

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("yt-dlp not found in PATH. Install it: https://github.com/yt-dlp/yt-dlp")]
    YtDlpNotFound,
    #[error("yt-dlp failed with exit code {code}: {stderr}")]
    YtDlpFailed { code: i32, stderr: String },
    #[error("failed to spawn yt-dlp: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("failed to parse yt-dlp JSON output: {0}")]
    Parse(#[source] serde_json::Error),
}

/// Metadata entry returned by yt-dlp --flat-playlist --dump-json
#[derive(Debug, serde::Deserialize)]
struct YtDlpEntry {
    #[serde(alias = "id")]
    id: String,
    title: Option<String>,
    duration: Option<f64>,
    uploader: Option<String>,
    channel: Option<String>,
}

impl YtDlpEntry {
    fn artist(&self) -> String {
        self.uploader
            .clone()
            .or_else(|| self.channel.clone())
            .unwrap_or_default()
    }
}

/// Check if yt-dlp is available in PATH.
pub async fn check_ytdlp() -> Result<(), FetchError> {
    let result = Command::new("yt-dlp")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(FetchError::YtDlpNotFound),
        Err(_) => Err(FetchError::YtDlpNotFound),
    }
}

/// Normalize YouTube Music URLs to standard YouTube URLs for yt-dlp compatibility.
fn normalize_url(url: &str) -> String {
    url.replace("music.youtube.com", "www.youtube.com")
}

/// Fetch song metadata from a YouTube playlist URL using yt-dlp.
/// Uses --flat-playlist for fast metadata-only fetching.
pub async fn fetch_playlist_songs(
    url: &str,
    cookies_path: Option<&Path>,
) -> Result<Vec<Song>, FetchError> {
    let normalized = normalize_url(url);
    let mut cmd = Command::new("yt-dlp");

    cmd.arg("--flat-playlist")
        .arg("--dump-json")
        .arg("--no-warnings")
        .arg("--ignore-errors");

    if let Some(cookies) = cookies_path {
        cmd.arg("--cookies").arg(cookies);
    }

    cmd.arg(&normalized);

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd.output().await.map_err(FetchError::Spawn)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(FetchError::YtDlpFailed {
            code: output.status.code().unwrap_or(-1),
            stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ytdlp_output(&stdout)
}

/// Search YouTube Music using yt-dlp and return a flat list of results.
pub async fn search_youtube_music(
    query: &str,
    cookies_path: Option<&Path>,
    limit: usize,
) -> Result<Vec<Song>, FetchError> {
    let mut cmd = Command::new("yt-dlp");
    let search = format!("ytsearch{limit}:{query}");

    cmd.arg("--flat-playlist")
        .arg("--dump-json")
        .arg("--no-warnings")
        .arg("--ignore-errors");

    if let Some(cookies) = cookies_path {
        cmd.arg("--cookies").arg(cookies);
    }

    cmd.arg(search);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd.output().await.map_err(FetchError::Spawn)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(FetchError::YtDlpFailed {
            code: output.status.code().unwrap_or(-1),
            stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ytdlp_output(&stdout)
}

/// Parse the NDJSON output from yt-dlp (one JSON object per line).
fn parse_ytdlp_output(output: &str) -> Result<Vec<Song>, FetchError> {
    let mut songs = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: YtDlpEntry = serde_json::from_str(line).map_err(FetchError::Parse)?;

        let artist = entry.artist();
        songs.push(Song {
            title: entry.title.unwrap_or_else(|| "Unknown".to_string()),
            video_id: entry.id,
            duration: entry.duration.map(|d| d as u64),
            artist,
            local_path: None,
            download_status: None,
        });
    }

    Ok(songs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ytdlp_output_single() {
        let json = r#"{"id": "dQw4w9WgXcQ", "title": "Never Gonna Give You Up", "duration": 212.0, "uploader": "Rick Astley"}"#;
        let songs = parse_ytdlp_output(json).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].video_id, "dQw4w9WgXcQ");
        assert_eq!(songs[0].title, "Never Gonna Give You Up");
        assert_eq!(songs[0].duration, Some(212));
        assert_eq!(songs[0].artist, "Rick Astley");
    }

    #[test]
    fn test_parse_ytdlp_output_multi() {
        let json = r#"{"id": "a1", "title": "Song A", "duration": 100.0, "uploader": "X"}
{"id": "b2", "title": "Song B", "duration": 200.5, "uploader": "Y"}
{"id": "c3", "title": "Song C", "duration": null, "channel": "Z"}"#;
        let songs = parse_ytdlp_output(json).unwrap();
        assert_eq!(songs.len(), 3);
        assert_eq!(songs[2].duration, None);
    }

    #[test]
    fn test_parse_ytdlp_output_missing_fields() {
        let json = r#"{"id": "x1"}"#;
        let songs = parse_ytdlp_output(json).unwrap();
        assert_eq!(songs[0].title, "Unknown");
        assert_eq!(songs[0].artist, "");
        assert_eq!(songs[0].duration, None);
    }

    #[test]
    fn test_parse_ytdlp_output_empty() {
        let songs = parse_ytdlp_output("").unwrap();
        assert!(songs.is_empty());
    }

    #[test]
    fn test_parse_ytdlp_output_blank_lines() {
        let json = "\n{\"id\": \"a1\", \"title\": \"Song\"}\n\n";
        let songs = parse_ytdlp_output(json).unwrap();
        assert_eq!(songs.len(), 1);
    }

    #[test]
    fn test_parse_ytdlp_output_invalid_json() {
        let result = parse_ytdlp_output("not json at all");
        assert!(result.is_err());
    }
}
