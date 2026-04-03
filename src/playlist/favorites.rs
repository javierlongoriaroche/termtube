use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FavoritesError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Manages a set of favorite video IDs, persisted to disk.
pub struct Favorites {
    path: PathBuf,
    ids: HashSet<String>,
}

impl Favorites {
    /// Load favorites from a JSON file, or start empty if file doesn't exist.
    pub fn load(path: &Path) -> Result<Self, FavoritesError> {
        let ids = if path.exists() {
            let content = fs::read_to_string(path)?;
            let content = content.trim();
            if content.is_empty() {
                HashSet::new()
            } else {
                let list: Vec<String> = serde_json::from_str(content)?;
                list.into_iter().collect()
            }
        } else {
            HashSet::new()
        };

        Ok(Self {
            path: path.to_path_buf(),
            ids,
        })
    }

    /// Create an empty favorites set at the given path.
    pub fn empty(path: PathBuf) -> Self {
        Self {
            path,
            ids: HashSet::new(),
        }
    }

    /// Save current favorites to disk.
    pub fn save(&self) -> Result<(), FavoritesError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let list: Vec<&String> = self.ids.iter().collect();
        let json = serde_json::to_string_pretty(&list)?;
        fs::write(&self.path, json)?;
        Ok(())
    }

    /// Toggle a video ID: add if not present, remove if present.
    /// Returns true if the video is now a favorite.
    pub fn toggle(&mut self, video_id: &str) -> bool {
        if self.ids.contains(video_id) {
            self.ids.remove(video_id);
            false
        } else {
            self.ids.insert(video_id.to_string());
            true
        }
    }

    pub fn is_favorite(&self, video_id: &str) -> bool {
        self.ids.contains(video_id)
    }

    pub fn count(&self) -> usize {
        self.ids.len()
    }

    pub fn all(&self) -> &HashSet<String> {
        &self.ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_toggle_add_remove() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("favorites.json");
        let mut favs = Favorites::load(&path).unwrap();

        assert!(!favs.is_favorite("abc"));
        assert_eq!(favs.count(), 0);

        // Add
        let is_fav = favs.toggle("abc");
        assert!(is_fav);
        assert!(favs.is_favorite("abc"));
        assert_eq!(favs.count(), 1);

        // Remove
        let is_fav = favs.toggle("abc");
        assert!(!is_fav);
        assert!(!favs.is_favorite("abc"));
        assert_eq!(favs.count(), 0);
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("favorites.json");

        {
            let mut favs = Favorites::load(&path).unwrap();
            favs.toggle("id1");
            favs.toggle("id2");
            favs.toggle("id3");
            favs.save().unwrap();
        }

        let favs = Favorites::load(&path).unwrap();
        assert_eq!(favs.count(), 3);
        assert!(favs.is_favorite("id1"));
        assert!(favs.is_favorite("id2"));
        assert!(favs.is_favorite("id3"));
    }

    #[test]
    fn test_load_nonexistent_starts_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("does_not_exist.json");
        let favs = Favorites::load(&path).unwrap();
        assert_eq!(favs.count(), 0);
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("dir").join("favorites.json");
        let mut favs = Favorites::load(&path).unwrap();
        favs.toggle("x");
        favs.save().unwrap();

        assert!(path.exists());
    }
}
