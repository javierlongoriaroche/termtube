use serde::{Deserialize, Serialize};

/// A single song/track from a YouTube playlist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Song {
    pub title: String,
    pub video_id: String,
    /// Duration in seconds; None if unknown (e.g. livestream).
    pub duration: Option<u64>,
    /// Uploader / channel name.
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(skip, default)]
    pub download_status: Option<DownloadStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStatus {
    Downloading,
}

impl Song {
    pub fn url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.video_id)
    }

    pub fn duration_display(&self) -> String {
        match self.duration {
            Some(secs) => {
                let m = secs / 60;
                let s = secs % 60;
                format!("{m:02}:{s:02}")
            }
            None => "--:--".to_string(),
        }
    }

    pub fn is_local(&self) -> bool {
        self.local_path.as_ref().map_or(false, |path| !path.is_empty())
    }

    pub fn display_title(&self) -> String {
        let mut title = self.title.clone();
        if self.download_status == Some(DownloadStatus::Downloading) {
            title = format!("dwn>_ {title}");
        }
        if self.local_path.is_some() {
            title = format!("{title} (local)");
        }
        title
    }

    pub fn validate_local_path(&mut self) {
        if let Some(path) = &self.local_path {
            if !std::path::Path::new(path).exists() {
                self.local_path = None;
            }
        }
    }
}

/// A playlist with its songs resolved from YouTube.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub url: String,
    pub songs: Vec<Song>,
}

/// Index of all known playlists (lightweight, no songs).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaylistIndex {
    pub entries: Vec<PlaylistIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistIndexEntry {
    pub name: String,
    pub url: String,
    pub song_count: usize,
}

impl PlaylistIndex {
    pub fn from_playlists(playlists: &[Playlist]) -> Self {
        Self {
            entries: playlists
                .iter()
                .map(|p| PlaylistIndexEntry {
                    name: p.name.clone(),
                    url: p.url.clone(),
                    song_count: p.songs.len(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_song_url() {
        let song = Song {
            title: "Test".into(),
            video_id: "dQw4w9WgXcQ".into(),
            duration: Some(212),
            artist: "Rick Astley".into(),
            local_path: None,
            download_status: None,
        };
        assert_eq!(song.url(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    }

    #[test]
    fn test_duration_display() {
        let song = Song {
            title: "T".into(),
            video_id: "x".into(),
            duration: Some(185),
            artist: String::new(),
            local_path: None,
            download_status: None,
        };
        assert_eq!(song.duration_display(), "03:05");
    }

    #[test]
    fn test_duration_display_none() {
        let song = Song {
            title: "T".into(),
            video_id: "x".into(),
            duration: None,
            artist: String::new(),
            local_path: None,
            download_status: None,
        };
        assert_eq!(song.duration_display(), "--:--");
    }

    #[test]
    fn test_playlist_index() {
        let playlists = vec![
            Playlist {
                name: "lofi".into(),
                url: "https://www.youtube.com/playlist?list=PL1".into(),
                songs: vec![
                    Song {
                        title: "a".into(),
                        video_id: "1".into(),
                        duration: Some(60),
                        artist: String::new(),
                        local_path: None,
                        download_status: None,
                    },
                    Song {
                        title: "b".into(),
                        video_id: "2".into(),
                        duration: Some(120),
                        artist: String::new(),
                        local_path: None,
                        download_status: None,
                    },
                ],
            },
            Playlist {
                name: "empty".into(),
                url: "https://www.youtube.com/playlist?list=PL2".into(),
                songs: vec![],
            },
        ];
        let index = PlaylistIndex::from_playlists(&playlists);
        assert_eq!(index.entries.len(), 2);
        assert_eq!(index.entries[0].song_count, 2);
        assert_eq!(index.entries[1].song_count, 0);
    }

    #[test]
    fn test_song_serialization_roundtrip() {
        let song = Song {
            title: "Test Song".into(),
            video_id: "abc123".into(),
            duration: Some(300),
            artist: "Artist".into(),
            local_path: None,
            download_status: None,
        };
        let json = serde_json::to_string(&song).unwrap();
        let deserialized: Song = serde_json::from_str(&json).unwrap();
        assert_eq!(song, deserialized);
    }
}
