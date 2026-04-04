use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use crate::search::SearchCache;
use crate::playlist::models::Song;

#[test]
fn test_search_cache_save_and_load() {
    let dir = tempdir().unwrap();
    let cache_dir = dir.path().to_path_buf();
    let cache = SearchCache::new(&cache_dir);
    let query = "lofi beats";
    let songs = vec![
        Song {
            title: "Test Song".to_string(),
            video_id: "abc123".to_string(),
            duration: Some(123),
            artist: "Test Artist".to_string(),
        },
        Song {
            title: "Another Song".to_string(),
            video_id: "def456".to_string(),
            duration: None,
            artist: "".to_string(),
        },
    ];
    // Save to cache
    cache.save(query, &songs).unwrap();
    // Load from cache (should succeed)
    let loaded = cache.load(query, 3600).unwrap();
    assert_eq!(loaded, songs);
    // Load with expired TTL (should fail)
    let loaded_expired = cache.load(query, 0);
    assert!(loaded_expired.is_none());
}

#[test]
fn test_search_cache_key_uniqueness() {
    use crate::search::cache_key;
    let k1 = cache_key("lofi beats");
    let k2 = cache_key("LOFI BEATS");
    let k3 = cache_key("lofi  beats");
    let k4 = cache_key("different query");
    assert_eq!(k1, k2);
    assert_eq!(k1, k3);
    assert_ne!(k1, k4);
}
