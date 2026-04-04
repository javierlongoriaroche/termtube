use std::path::PathBuf;

use crate::audio::queue::{PlaybackQueue, QueueItem};
use crate::config::playlist::PlaylistEntry;
use crate::config::settings::Settings;
use crate::playlist::favorites::Favorites;
use crate::playlist::models::{Playlist, Song};

/// Represents the different screens/modes of the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Main,
    Help,
    Search,
    QueueView,
}

/// Playback repeat mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepeatMode {
    #[default]
    None,
    One,
    All,
}

/// Central application state.
pub struct App {
    pub settings: Settings,
    pub playlists: Vec<PlaylistEntry>,
    pub screen: AppScreen,
    pub running: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub volume: u8,
    pub selected_playlist: usize,
    /// Cached synced playlists with full song data.
    pub cached_playlists: Vec<Playlist>,
    /// Playback queue.
    pub queue: PlaybackQueue,
    /// Favorites manager.
    pub favorites: Favorites,
    /// Whether audio is currently playing.
    pub is_playing: bool,
    /// Whether the visualizer is visible (vs logo).
    pub show_visualizer: bool,
    /// Search state for YouTube Music queries.
    pub search: SearchState,
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<Song>,
    pub is_loading: bool,
    pub error: Option<String>,
    pub status: Option<String>,
    pub cache_hit: bool,
    pub last_query: String,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            is_loading: false,
            error: None,
            status: None,
            cache_hit: false,
            last_query: String::new(),
        }
    }

    pub fn clear_status(&mut self) {
        self.is_loading = false;
        self.error = None;
        self.status = None;
        self.cache_hit = false;
    }
}

impl App {
    pub fn new(settings: Settings, playlists: Vec<PlaylistEntry>) -> Self {
        let favorites_path =
            PathBuf::from(shellexpand::tilde("~/.termtube/favorites.json").as_ref());
        let favorites =
            Favorites::load(&favorites_path).unwrap_or_else(|_| Favorites::empty(favorites_path));

        Self {
            settings,
            playlists,
            screen: AppScreen::Main,
            running: true,
            shuffle: false,
            repeat: RepeatMode::None,
            volume: 80,
            selected_playlist: 0,
            cached_playlists: Vec::new(),
            queue: PlaybackQueue::new(),
            favorites,
            is_playing: false,
            show_visualizer: true,
            search: SearchState::new(),
        }
    }

    /// Create App with a custom favorites path (for testing).
    #[cfg(test)]
    pub fn new_with_favorites_path(
        settings: Settings,
        playlists: Vec<PlaylistEntry>,
        favorites_path: PathBuf,
    ) -> Self {
        let favorites =
            Favorites::load(&favorites_path).unwrap_or_else(|_| Favorites::empty(favorites_path));

        Self {
            settings,
            playlists,
            screen: AppScreen::Main,
            running: true,
            shuffle: false,
            repeat: RepeatMode::None,
            volume: 80,
            selected_playlist: 0,
            cached_playlists: Vec::new(),
            queue: PlaybackQueue::new(),
            favorites,
            is_playing: false,
            show_visualizer: true,
            search: SearchState::new(),
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
    }

    pub fn toggle_visualizer(&mut self) {
        self.show_visualizer = !self.show_visualizer;
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            RepeatMode::None => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::None,
        };
    }

    pub fn volume_up(&mut self) {
        self.volume = (self.volume + 5).min(100);
    }

    pub fn volume_down(&mut self) {
        self.volume = self.volume.saturating_sub(5);
    }

    /// Get the songs for the currently selected playlist.
    pub fn current_playlist_songs(&self) -> &[Song] {
        if let Some(pl) = self.cached_playlists.get(self.selected_playlist) {
            &pl.songs
        } else {
            &[]
        }
    }

    /// Get all favorite songs from all cached playlists.
    pub fn favorite_songs(&self) -> Vec<&Song> {
        self.cached_playlists
            .iter()
            .flat_map(|pl| &pl.songs)
            .filter(|s| self.favorites.is_favorite(&s.video_id))
            .collect()
    }

    /// Add a song to the playback queue.
    pub fn add_to_queue(&mut self, song: &Song) {
        self.queue.enqueue(QueueItem {
            id: song.video_id.clone(),
            title: song.title.clone(),
            url: song.url(),
            duration: song.duration,
        });
    }

    /// Toggle favorite for a song and persist.
    pub fn toggle_favorite(&mut self, video_id: &str) -> bool {
        let result = self.favorites.toggle(video_id);
        let _ = self.favorites.save();
        result
    }

    /// Get queue item titles for display.
    pub fn queue_titles(&self) -> Vec<String> {
        self.queue
            .items()
            .iter()
            .map(|item| item.title.clone())
            .collect()
    }

    /// Current queue index (for highlight in queue view).
    pub fn queue_current_index(&self) -> Option<usize> {
        self.queue.current_index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_app() -> (App, TempDir) {
        let tmp = TempDir::new().unwrap();
        let fav_path = tmp.path().join("favorites.json");
        let app = App::new_with_favorites_path(Settings::default(), vec![], fav_path);
        (app, tmp)
    }

    #[test]
    fn test_initial_state() {
        let (app, _tmp) = test_app();
        assert!(app.running);
        assert!(!app.shuffle);
        assert_eq!(app.repeat, RepeatMode::None);
        assert_eq!(app.volume, 80);
        assert_eq!(app.screen, AppScreen::Main);
    }

    #[test]
    fn test_quit() {
        let (mut app, _tmp) = test_app();
        app.quit();
        assert!(!app.running);
    }

    #[test]
    fn test_toggle_shuffle() {
        let (mut app, _tmp) = test_app();
        assert!(!app.shuffle);
        app.toggle_shuffle();
        assert!(app.shuffle);
        app.toggle_shuffle();
        assert!(!app.shuffle);
    }

    #[test]
    fn test_cycle_repeat() {
        let (mut app, _tmp) = test_app();
        assert_eq!(app.repeat, RepeatMode::None);
        app.cycle_repeat();
        assert_eq!(app.repeat, RepeatMode::All);
        app.cycle_repeat();
        assert_eq!(app.repeat, RepeatMode::One);
        app.cycle_repeat();
        assert_eq!(app.repeat, RepeatMode::None);
    }

    #[test]
    fn test_volume_bounds() {
        let (mut app, _tmp) = test_app();
        app.volume = 100;
        app.volume_up();
        assert_eq!(app.volume, 100);

        app.volume = 0;
        app.volume_down();
        assert_eq!(app.volume, 0);
    }

    #[test]
    fn test_volume_up_down() {
        let (mut app, _tmp) = test_app();
        let initial = app.volume;
        app.volume_up();
        assert_eq!(app.volume, initial + 5);
        app.volume_down();
        assert_eq!(app.volume, initial);
    }

    #[test]
    fn test_current_playlist_songs_empty() {
        let (app, _tmp) = test_app();
        assert!(app.current_playlist_songs().is_empty());
    }

    #[test]
    fn test_current_playlist_songs_with_data() {
        let (mut app, _tmp) = test_app();
        app.cached_playlists.push(Playlist {
            name: "test".into(),
            url: "https://example.com".into(),
            songs: vec![Song {
                title: "Song 1".into(),
                video_id: "vid1".into(),
                duration: Some(120),
                artist: "Artist".into(),
            }],
        });
        app.selected_playlist = 0;
        assert_eq!(app.current_playlist_songs().len(), 1);
        assert_eq!(app.current_playlist_songs()[0].title, "Song 1");
    }

    #[test]
    fn test_add_to_queue() {
        let (mut app, _tmp) = test_app();
        let song = Song {
            title: "Test Song".into(),
            video_id: "abc123".into(),
            duration: Some(180),
            artist: "Test".into(),
        };
        app.add_to_queue(&song);
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue_titles(), vec!["Test Song"]);
    }

    #[test]
    fn test_toggle_favorite() {
        let (mut app, _tmp) = test_app();
        let is_fav = app.toggle_favorite("vid1");
        assert!(is_fav);
        assert!(app.favorites.is_favorite("vid1"));

        let is_fav = app.toggle_favorite("vid1");
        assert!(!is_fav);
        assert!(!app.favorites.is_favorite("vid1"));
    }

    #[test]
    fn test_favorite_songs() {
        let (mut app, _tmp) = test_app();
        app.cached_playlists.push(Playlist {
            name: "test".into(),
            url: "https://example.com".into(),
            songs: vec![
                Song {
                    title: "Song A".into(),
                    video_id: "a".into(),
                    duration: Some(100),
                    artist: "".into(),
                },
                Song {
                    title: "Song B".into(),
                    video_id: "b".into(),
                    duration: Some(200),
                    artist: "".into(),
                },
            ],
        });
        app.toggle_favorite("a");
        let favs = app.favorite_songs();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].video_id, "a");
    }

    #[test]
    fn test_queue_current_index_none_initially() {
        let (app, _tmp) = test_app();
        assert_eq!(app.queue_current_index(), None);
    }

    #[test]
    fn test_initial_state_has_queue_and_favorites() {
        let (app, _tmp) = test_app();
        assert!(app.queue.is_empty());
        assert_eq!(app.favorites.count(), 0);
        assert!(app.cached_playlists.is_empty());
        assert!(!app.is_playing);
    }
}
