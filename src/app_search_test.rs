use std::path::PathBuf;
use crate::app::{App, AppScreen, SearchState};
use crate::config::settings::Settings;
use crate::config::playlist::PlaylistEntry;

#[test]
fn test_search_screen_switch_and_query_input() {
    let settings = Settings::default();
    let playlists = vec![PlaylistEntry { name: "Test".to_string(), url: "https://yt.com/playlist?list=123".to_string() }];
    let mut app = App::new(settings, playlists);
    assert_eq!(app.screen, AppScreen::Main);
    // Simular acción de búsqueda
    app.screen = AppScreen::Search;
    app.search.query = "lofi".to_string();
    assert_eq!(app.screen, AppScreen::Search);
    assert_eq!(app.search.query, "lofi");
    // Limpiar búsqueda
    app.search.query.clear();
    assert_eq!(app.search.query, "");
}

#[test]
fn test_search_state_clear_status() {
    let mut state = SearchState {
        query: "test".to_string(),
        results: vec![],
        is_loading: true,
        error: Some("err".to_string()),
        status: Some("status".to_string()),
        cache_hit: true,
        last_query: "test".to_string(),
    };
    state.clear_status();
    assert!(!state.is_loading);
    assert!(state.error.is_none());
    assert!(state.status.is_none());
    assert!(!state.cache_hit);
}
