use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::config::playlist::PlaylistEntry;
use crate::playlist::models::{Playlist, PlaylistIndex};
use crate::sync::fetcher;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("fetch error: {0}")]
    Fetch(#[from] fetcher::FetchError),
}

/// Manages playlist data on disk under a base directory (e.g. ~/.termtube/).
pub struct PlaylistManager {
    base_dir: PathBuf,
}

impl PlaylistManager {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    fn validate_playlist_local_paths(&self, playlist: &mut Playlist) -> Result<bool, ManagerError> {
        let mut changed = false;
        for song in &mut playlist.songs {
            if let Some(path) = &song.local_path {
                if !std::path::Path::new(path).exists() {
                    song.local_path = None;
                    changed = true;
                }
            }
        }
        if changed {
            self.save_playlist(playlist)?;
        }
        Ok(changed)
    }

    /// Directory where individual playlist JSON files are stored.
    fn playlists_dir(&self) -> PathBuf {
        self.base_dir.join("playlists")
    }

    fn index_path(&self) -> PathBuf {
        self.base_dir.join("playlists.json")
    }

    fn playlist_cache_path(&self, name: &str) -> PathBuf {
        self.playlists_dir().join(format!("{name}.json"))
    }

    /// Ensure base directories exist.
    pub fn ensure_dirs(&self) -> Result<(), ManagerError> {
        fs::create_dir_all(self.playlists_dir())?;
        Ok(())
    }

    /// Load a cached playlist from disk, if it exists.
    pub fn load_cached(&self, name: &str) -> Result<Option<Playlist>, ManagerError> {
        let path = self.playlist_cache_path(name);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let mut playlist: Playlist = serde_json::from_str(&content)?;
        let _ = self.validate_playlist_local_paths(&mut playlist)?;
        Ok(Some(playlist))
    }

    /// Save a playlist to the cache on disk.
    pub fn save_playlist(&self, playlist: &Playlist) -> Result<(), ManagerError> {
        self.ensure_dirs()?;
        let path = self.playlist_cache_path(&playlist.name);
        let json = serde_json::to_string_pretty(playlist)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Save the playlist index to disk.
    pub fn save_index(&self, index: &PlaylistIndex) -> Result<(), ManagerError> {
        let json = serde_json::to_string_pretty(index)?;
        fs::write(self.index_path(), json)?;
        Ok(())
    }

    /// Load the playlist index from disk.
    pub fn load_index(&self) -> Result<Option<PlaylistIndex>, ManagerError> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let index: PlaylistIndex = serde_json::from_str(&content)?;
        Ok(Some(index))
    }

    /// Sync a single playlist entry: fetch metadata from YouTube and cache it.
    pub async fn sync_playlist(
        &self,
        entry: &PlaylistEntry,
        cookies_path: Option<&Path>,
    ) -> Result<Playlist, ManagerError> {
        let songs = fetcher::fetch_playlist_songs(&entry.url, cookies_path).await?;

        let playlist = Playlist {
            name: entry.name.clone(),
            url: entry.url.clone(),
            songs,
        };

        self.save_playlist(&playlist)?;
        Ok(playlist)
    }

    /// Sync all playlists from the given entries. Returns all synced playlists
    /// and updates the index on disk.
    pub async fn sync_all(
        &self,
        entries: &[PlaylistEntry],
        cookies_path: Option<&Path>,
    ) -> Result<Vec<Playlist>, ManagerError> {
        self.ensure_dirs()?;

        let mut playlists = Vec::with_capacity(entries.len());

        for entry in entries {
            match self.sync_playlist(entry, cookies_path).await {
                Ok(pl) => {
                    tracing::info!("Synced '{}': {} songs", pl.name, pl.songs.len());
                    eprintln!("  ✓ {} — {} songs", pl.name, pl.songs.len());
                    playlists.push(pl);
                }
                Err(e) => {
                    tracing::warn!("Failed to sync '{}': {e}", entry.name);
                    // Try to load cached version as fallback
                    if let Ok(Some(cached)) = self.load_cached(&entry.name) {
                        tracing::info!(
                            "Using cached version of '{}' ({} songs)",
                            cached.name,
                            cached.songs.len()
                        );
                        eprintln!(
                            "  ⚠ {} — fetch failed, using cache ({} songs)",
                            entry.name,
                            cached.songs.len()
                        );
                        playlists.push(cached);
                    } else {
                        eprintln!("  ✗ {} — {}", entry.name, e);
                    }
                }
            }
        }

        let index = PlaylistIndex::from_playlists(&playlists);
        self.save_index(&index)?;

        Ok(playlists)
    }

    /// Load or synchronize cached playlists for a set of playlist entries.
    ///
    /// If the cache contains an entry for every playlist, it returns the cache.
    /// If any playlist is missing, it synchronizes all playlists and refreshes the
    /// local cache using `sync_all`.
    pub async fn load_or_sync_cached_playlists(
        &self,
        entries: &[PlaylistEntry],
        cookies_path: Option<&Path>,
    ) -> Result<Vec<Playlist>, ManagerError> {
        self.ensure_dirs()?;

        let mut cached = self.load_all_cached(entries);
        for playlist in &mut cached {
            let _ = self.validate_playlist_local_paths(playlist);
        }
        if cached.len() == entries.len() {
            return Ok(cached);
        }

        self.sync_all(entries, cookies_path).await
    }

    /// Load all playlists from cache (no network). Returns whatever is cached.
    pub fn load_all_cached(&self, entries: &[PlaylistEntry]) -> Vec<Playlist> {
        let mut playlists: Vec<Playlist> = entries
            .iter()
            .filter_map(|e| self.load_cached(&e.name).ok().flatten())
            .collect();

        for playlist in &mut playlists {
            let _ = self.validate_playlist_local_paths(playlist);
        }

        playlists
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::models::Song;
    use tempfile::TempDir;

    fn make_entry(name: &str) -> PlaylistEntry {
        PlaylistEntry {
            name: name.to_string(),
            url: format!("https://www.youtube.com/playlist?list=PL{name}"),
        }
    }

    fn make_playlist(name: &str, num_songs: usize) -> Playlist {
        Playlist {
            name: name.to_string(),
            url: format!("https://www.youtube.com/playlist?list=PL{name}"),
            songs: (0..num_songs)
                .map(|i| Song {
                    title: format!("Song {i}"),
                    video_id: format!("{name}_{i}"),
                    duration: Some(180 + i as u64),
                    artist: "Test".to_string(),
                    local_path: None,
                    download_status: None,
                })
                .collect(),
        }
    }

    #[test]
    fn test_save_and_load_playlist() {
        let tmp = TempDir::new().unwrap();
        let mgr = PlaylistManager::new(tmp.path());

        let playlist = make_playlist("lofi", 3);
        mgr.save_playlist(&playlist).unwrap();

        let loaded = mgr.load_cached("lofi").unwrap().unwrap();
        assert_eq!(loaded.name, "lofi");
        assert_eq!(loaded.songs.len(), 3);
        assert_eq!(loaded.songs[0].title, "Song 0");
    }

    #[test]
    fn test_load_nonexistent_returns_none() {
        let tmp = TempDir::new().unwrap();
        let mgr = PlaylistManager::new(tmp.path());

        let result = mgr.load_cached("nope").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_and_load_index() {
        let tmp = TempDir::new().unwrap();
        let mgr = PlaylistManager::new(tmp.path());
        mgr.ensure_dirs().unwrap();

        let playlists = vec![make_playlist("a", 2), make_playlist("b", 5)];
        let index = PlaylistIndex::from_playlists(&playlists);
        mgr.save_index(&index).unwrap();

        let loaded = mgr.load_index().unwrap().unwrap();
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].song_count, 2);
        assert_eq!(loaded.entries[1].song_count, 5);
    }

    #[test]
    fn test_load_all_cached() {
        let tmp = TempDir::new().unwrap();
        let mgr = PlaylistManager::new(tmp.path());

        // Save only one
        mgr.save_playlist(&make_playlist("exists", 2)).unwrap();

        let entries = vec![make_entry("exists"), make_entry("missing")];
        let loaded = mgr.load_all_cached(&entries);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "exists");
    }

    #[tokio::test]
    async fn test_load_or_sync_cached_playlists_returns_existing_cache_when_complete() {
        let tmp = TempDir::new().unwrap();
        let mgr = PlaylistManager::new(tmp.path());

        let playlist_a = make_playlist("a", 2);
        let playlist_b = make_playlist("b", 3);

        mgr.save_playlist(&playlist_a).unwrap();
        mgr.save_playlist(&playlist_b).unwrap();

        let entries = vec![make_entry("a"), make_entry("b")];
        let loaded = mgr
            .load_or_sync_cached_playlists(&entries, None)
            .await
            .unwrap();

        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().any(|pl| pl.name == "a"));
        assert!(loaded.iter().any(|pl| pl.name == "b"));
    }
}
