use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::playlist::models::Song;

#[derive(Debug, Serialize, Deserialize)]
struct SearchCacheEntry {
    query: String,
    fetched_at: u64,
    results: Vec<Song>,
}

/// Disk cache for YouTube Music search results.
pub struct SearchCache {
    base_dir: PathBuf,
}

impl SearchCache {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    fn cache_dir(&self) -> PathBuf {
        self.base_dir.join("search")
    }

    fn cache_path(&self, query: &str) -> PathBuf {
        let key = cache_key(query);
        self.cache_dir().join(format!("{key}.json"))
    }

    pub fn load(&self, query: &str, ttl_secs: u64) -> Option<Vec<Song>> {
        let path = self.cache_path(query);
        if !path.exists() {
            return None;
        }

        let content = fs::read_to_string(&path).ok()?;
        let entry: SearchCacheEntry = serde_json::from_str(&content).ok()?;
        if entry.query != query {
            return None;
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        if now.saturating_sub(entry.fetched_at) > ttl_secs {
            return None;
        }

        Some(entry.results)
    }

    pub fn save(&self, query: &str, results: &[Song]) -> Result<(), std::io::Error> {
        fs::create_dir_all(self.cache_dir())?;
        let entry = SearchCacheEntry {
            query: query.to_string(),
            fetched_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            results: results.to_vec(),
        };
        let json = serde_json::to_string_pretty(&entry).unwrap_or_else(|_| "{}".to_string());
        fs::write(self.cache_path(query), json)?;
        Ok(())
    }
}

fn cache_key(query: &str) -> String {
    let normalized = query.trim().to_lowercase();
    let mut slug = String::new();
    for ch in normalized.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if ch.is_ascii_whitespace() || ch == '-' || ch == '_' {
            if !slug.ends_with('_') {
                slug.push('_');
            }
        }
        if slug.len() >= 40 {
            break;
        }
    }

    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    let hash = hasher.finish();
    if slug.is_empty() {
        format!("query-{hash:016x}")
    } else {
        format!("{slug}-{hash:016x}")
    }
}
