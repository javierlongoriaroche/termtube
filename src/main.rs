mod app;
mod audio;
mod config;
mod input;
mod playlist;
mod search;
mod sync;
mod ui;
mod visualizer;

use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tracing::{error, info, warn};

use app::{App, AppScreen};
use audio::engine::AudioEngine;
use audio::preloader::Preloader;
use config::playlist::parse_playlist_file;
use config::settings::Settings;
use input::handler::{self, Action};
use playlist::manager::PlaylistManager;
use search::SearchCache;
use ui::layout::{AppLayout, MainPanelLayout};
use ui::now_playing::NowPlayingInfo;
use ui::playlist_view::{PlaylistViewState, SidebarFocus};
use ui::queue_view::QueueViewState;
use ui::search_view::SearchViewState;
use ui::theme::Theme;
use visualizer::spectrum::SpectrumAnalyzer;

/// TermTube — TUI YouTube music player with visualizer
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Path to cookies.txt file
    #[arg(long)]
    cookies: Option<PathBuf>,

    /// Path to playlist.txt file
    #[arg(long)]
    playlists: Option<PathBuf>,

    /// Path to config.toml file
    #[arg(long)]
    config: Option<PathBuf>,

    /// Sync playlists and exit
    #[arg(long)]
    sync: bool,
}

fn main() {
    let cli = Cli::parse();

    // Load or create settings
    let mut settings = load_or_create_settings(&cli);
    settings.resolve_paths();

    // Initialize file logging
    init_logging(&settings.general.log_file);
    info!("TermTube v{} starting", env!("CARGO_PKG_VERSION"));

    // Validate cookies if file exists
    let cookies_path = PathBuf::from(&settings.paths.cookies);
    if cookies_path.exists() {
        match config::cookies::validate_cookies_file(&cookies_path) {
            Ok(_) => info!("Cookies file validated: {}", cookies_path.display()),
            Err(e) => warn!("Cookies warning: {e}"),
        }
    } else {
        info!("No cookies file at {}. Private playlists may not work.", cookies_path.display());
    }

    // Load playlists
    let playlist_path = PathBuf::from(&settings.paths.playlists);
    let playlists = match parse_playlist_file(&playlist_path) {
        Ok(p) => p,
        Err(e) => {
            error!("Error loading playlists: {e}");
            eprintln!("Error loading playlists from {}: {e}", playlist_path.display());
            eprintln!("Create a playlist.txt with format: name|https://youtube.com/playlist?list=...");
            process::exit(1);
        }
    };

    if playlists.is_empty() {
        error!("No playlists found in {}", playlist_path.display());
        eprintln!("No playlists found in {}", playlist_path.display());
        eprintln!("Add at least one line: name|https://youtube.com/playlist?list=...");
        process::exit(1);
    }

    info!("Loaded {} playlist(s)", playlists.len());

    // --sync mode: sync playlists and exit
    if cli.sync {
        run_sync_mode(&settings, &playlists);
        return;
    }

    let mut app = App::new(settings.clone(), playlists.clone());

    // Load cached playlists on startup
    let manager = PlaylistManager::new(&PathBuf::from(&settings.general.cache_dir));
    let cached = manager.load_all_cached(
        &playlists,
    );
    if !cached.is_empty() {
        info!("Loaded {} cached playlist(s)", cached.len());
        app.cached_playlists = cached;
    }

    if let Err(e) = run_tui(&mut app) {
        error!("TUI error: {e}");
        eprintln!("Error: {e}");
        process::exit(1);
    }

    info!("TermTube exiting normally");
}

/// Initialize file-based logging with tracing.
fn init_logging(log_file: &str) {
    let log_path = PathBuf::from(log_file);
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        use tracing_subscriber::fmt;
        use tracing_subscriber::EnvFilter;

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));

        let _ = fmt()
            .with_writer(file)
            .with_env_filter(filter)
            .with_ansi(false)
            .with_target(false)
            .try_init();
    }
}

/// Load settings from config file, CLI overrides, or create default config on first run.
fn load_or_create_settings(cli: &Cli) -> Settings {
    let config_path = cli.config.clone().unwrap_or_else(|| {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("termtube");
        config_dir.join("config.toml")
    });

    let mut settings = if config_path.exists() {
        match Settings::load(&config_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: failed to load config {}: {e}", config_path.display());
                eprintln!("Using default settings.");
                Settings::default()
            }
        }
    } else {
        // First run — run wizard and create config
        let settings = first_run_wizard(&config_path);
        settings
    };

    // CLI argument overrides
    if let Some(cookies) = &cli.cookies {
        settings.paths.cookies = cookies.display().to_string();
    }
    if let Some(playlists) = &cli.playlists {
        settings.paths.playlists = playlists.display().to_string();
    }

    settings
}

/// Interactive first-run wizard: creates default config and guides the user.
fn first_run_wizard(config_path: &PathBuf) -> Settings {
    eprintln!("╔══════════════════════════════════════════╗");
    eprintln!("║    Welcome to TermTube! 🎵              ║");
    eprintln!("║    First-time setup                      ║");
    eprintln!("╚══════════════════════════════════════════╝");
    eprintln!();

    let settings = Settings::default();

    // Create config directory and default config
    if let Some(parent) = config_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Warning: couldn't create config dir: {e}");
            return settings;
        }
    }

    // Write default config
    match toml::to_string_pretty(&settings) {
        Ok(toml_str) => {
            let header = "# TermTube Configuration\n\
                          # Edit this file to customize keybindings, theme, paths, etc.\n\
                          # See README.md for all available options.\n\n";
            let content = format!("{header}{toml_str}");
            match fs::write(config_path, &content) {
                Ok(_) => eprintln!("  ✓ Config created: {}", config_path.display()),
                Err(e) => eprintln!("  ✗ Couldn't write config: {e}"),
            }
        }
        Err(e) => eprintln!("  ✗ Couldn't serialize config: {e}"),
    }

    // Create cache directory
    let cache_dir = shellexpand::tilde(&settings.general.cache_dir).to_string();
    match fs::create_dir_all(&cache_dir) {
        Ok(_) => eprintln!("  ✓ Cache dir created: {cache_dir}"),
        Err(e) => eprintln!("  ✗ Couldn't create cache dir: {e}"),
    }

    eprintln!();
    eprintln!("  Next steps:");
    eprintln!("  1. Create a playlist.txt with your YouTube playlists:");
    eprintln!("     lofi|https://www.youtube.com/playlist?list=PLxxxxxxx");
    eprintln!("  2. (Optional) Export cookies.txt for private playlists");
    eprintln!("  3. Run: termtube --sync  (to download playlist metadata)");
    eprintln!("  4. Run: termtube         (to start the player)");
    eprintln!();

    settings
}

/// Run --sync mode: check yt-dlp, sync all playlists, then exit.
fn run_sync_mode(settings: &Settings, playlists: &[config::playlist::PlaylistEntry]) {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    rt.block_on(async {
        // Check yt-dlp availability
        eprint!("Checking yt-dlp... ");
        match sync::fetcher::check_ytdlp().await {
            Ok(_) => eprintln!("✓ found"),
            Err(e) => {
                eprintln!("✗");
                eprintln!("Error: {e}");
                eprintln!("Install yt-dlp: pip install yt-dlp  or  brew install yt-dlp");
                process::exit(1);
            }
        }

        let cache_dir = PathBuf::from(&settings.general.cache_dir);
        let manager = PlaylistManager::new(&cache_dir);

        let cookies_path = PathBuf::from(&settings.paths.cookies);
        let cookies = if cookies_path.exists() {
            Some(cookies_path.as_path())
        } else {
            None
        };

        eprintln!("Syncing {} playlist(s)...", playlists.len());

        match manager.sync_all(playlists, cookies).await {
            Ok(synced) => {
                let total: usize = synced.iter().map(|p| p.songs.len()).sum();
                eprintln!();
                eprintln!("Sync complete: {} playlists, {} total songs.", synced.len(), total);
            }
            Err(e) => {
                eprintln!("Sync error: {e}");
                process::exit(1);
            }
        }
    });
}

fn run_tui(app: &mut App) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let theme = Theme::from_settings(&app.settings.theme);
    let tick_rate = Duration::from_millis(33); // ~30fps

    let mut sidebar_state = PlaylistViewState::new();
    sidebar_state.set_playlist_count(app.playlists.len());

    let mut queue_state = QueueViewState::new();
    let mut search_state = SearchViewState::new();

    let mut spectrum = SpectrumAnalyzer::new(
        app.settings.visualizer.bars as usize,
        app.settings.visualizer.decay,
        48000,
    );

    let mut engine = AudioEngine::new();

    let preload_target = app
        .settings
        .general
        .preload_count
        .max(PRELOAD_SIZE as u8) as usize;
    let cookies_for_preload = {
        let cookies_path = PathBuf::from(&app.settings.paths.cookies);
        if cookies_path.exists() {
            Some(cookies_path)
        } else {
            None
        }
    };
    let mut preloader = if preload_target > 0 {
        match Preloader::new(
            PathBuf::from(&app.settings.general.cache_dir),
            cookies_for_preload,
            preload_target,
        ) {
            Ok(preloader) => Some(preloader),
            Err(err) => {
                warn!("preloader disabled: {err}");
                None
            }
        }
    } else {
        None
    };

    let result = main_loop(
        &mut terminal,
        app,
        &theme,
        tick_rate,
        &mut sidebar_state,
        &mut queue_state,
        &mut search_state,
        &mut spectrum,
        &mut engine,
        &preloader,
        preload_target,
    );

    // Stop audio before restoring terminal
    engine.stop();

    if let Some(preloader) = preloader.as_mut() {
        preloader.stop();
    }

    // Restore terminal
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

const PRELOAD_SIZE: usize = 5;
const SEARCH_LIMIT: usize = 10;
const SEARCH_CACHE_TTL_SECS: u64 = 600;

/// State for the preloaded playback queue.
struct PlayState {
    current_song: Option<playlist::models::Song>,
    playback_start: Option<Instant>,
    paused_duration: Duration,
    pause_instant: Option<Instant>,
    /// Songs coming up next (preloaded).
    upcoming: VecDeque<playlist::models::Song>,
    /// Songs already played (for Previous).
    history: Vec<playlist::models::Song>,
    /// Source playlist index from which the queue was built.
    source_playlist: usize,
    /// The next linear index to pick from when refilling the upcoming queue (normal mode).
    next_linear_index: usize,
    /// Remaining shuffled indices for the current shuffle cycle.
    shuffle_pool: VecDeque<usize>,
}

impl PlayState {
    fn new() -> Self {
        Self {
            current_song: None,
            playback_start: None,
            paused_duration: Duration::ZERO,
            pause_instant: None,
            upcoming: VecDeque::new(),
            history: Vec::new(),
            source_playlist: 0,
            next_linear_index: 0,
            shuffle_pool: VecDeque::new(),
        }
    }
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    theme: &Theme,
    tick_rate: Duration,
    sidebar_state: &mut PlaylistViewState,
    queue_state: &mut QueueViewState,
    search_state: &mut SearchViewState,
    spectrum: &mut SpectrumAnalyzer,
    engine: &mut AudioEngine,
    preloader: &Option<Preloader>,
    preload_target: usize,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    let mut ps = PlayState::new();

    while app.running {
        // Auto-advance: when song finishes and buffer is drained, play next
        if engine.is_active() && !engine.is_paused() && engine.is_song_finished() {
            info!("Song finished, auto-advancing");
            if app.repeat == app::RepeatMode::One {
                if let Some(song) = ps.current_song.clone() {
                    play_song(app, engine, &song, &mut ps);
                    sync_sidebar_selection(app, sidebar_state, &song);
                }
            } else if let Some(next_song) = ps.upcoming.pop_front() {
                if let Some(cur) = ps.current_song.take() {
                    ps.history.push(cur);
                }
                refill_upcoming(app, &mut ps, 1);
                preload_upcoming(preloader, &ps.upcoming, preload_target);
                let song_ref = next_song.clone();
                play_song(app, engine, &next_song, &mut ps);
                sync_sidebar_selection(app, sidebar_state, &song_ref);
            } else {
                // No more songs — stop
                engine.stop();
                app.is_playing = false;
                ps.current_song = None;
                ps.playback_start = None;
            }
        }

        // Grab visualizer samples from the audio engine
        let vis_samples = engine.take_visualizer_samples(4096);

        // Draw
        terminal.draw(|frame| {
            draw_ui(
                frame,
                app,
                theme,
                sidebar_state,
                queue_state,
                search_state,
                spectrum,
                &ps.current_song,
                ps.playback_start,
                ps.paused_duration,
                ps.pause_instant,
                &vis_samples,
            );
        })?;

        // Event handling
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Only handle key press events (not release/repeat on some terminals)
                if key.kind == KeyEventKind::Press {
                    let action = handler::map_key_event(key, &app.settings.keybindings, app.screen);
                    handle_action(
                        app,
                        action,
                        sidebar_state,
                        queue_state,
                        search_state,
                        engine,
                        &mut ps,
                        preloader,
                        preload_target,
                    );
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn handle_action(
    app: &mut App,
    action: Action,
    sidebar_state: &mut PlaylistViewState,
    queue_state: &mut QueueViewState,
    search_state: &mut SearchViewState,
    engine: &mut AudioEngine,
    ps: &mut PlayState,
    preloader: &Option<Preloader>,
    preload_target: usize,
) {
    match action {
        Action::NavigateUp => {
            if app.screen == AppScreen::QueueView {
                queue_state.previous();
            } else if app.screen == AppScreen::Search {
                search_state.previous();
            } else {
                sidebar_state.previous();
            }
        }
        Action::NavigateDown => {
            if app.screen == AppScreen::QueueView {
                queue_state.next();
            } else if app.screen == AppScreen::Search {
                search_state.next();
            } else {
                sidebar_state.next();
            }
        }
        Action::ToggleFocus => {
            if app.screen != AppScreen::Search {
                sidebar_state.toggle_focus();
            }
        }
        Action::Select => {
            if app.screen == AppScreen::Search {
                if should_execute_search(&app.search) {
                    execute_search(app, search_state);
                } else {
                    play_selected_search(app, search_state, engine, ps);
                }
                return;
            }
            if sidebar_state.focus == SidebarFocus::Songs {
                if let Some(song_idx) = sidebar_state.selected_song() {
                    let songs = app.current_playlist_songs().to_vec();
                    if song_idx < songs.len() {
                        // Build the preload queue starting from the selected song
                        ps.history.clear();
                        ps.upcoming.clear();
                        ps.source_playlist = app.selected_playlist;

                        let song = songs[song_idx].clone();

                        if app.shuffle {
                            // Shuffle: fill upcoming from a randomized pool (excluding selected)
                            ps.shuffle_pool = build_shuffle_pool(&songs, Some(&song), &ps.upcoming);
                            fill_upcoming_from_pool(&songs, Some(&song), ps, PRELOAD_SIZE, app.repeat);
                        } else {
                            // Normal: fill upcoming with the next PRELOAD_SIZE songs after selected
                            ps.shuffle_pool.clear();
                            let start = song_idx + 1;
                            for i in 0..PRELOAD_SIZE {
                                let idx = start + i;
                                if idx < songs.len() {
                                    ps.upcoming.push_back(songs[idx].clone());
                                } else if app.repeat == app::RepeatMode::All {
                                    let wrap = idx % songs.len();
                                    ps.upcoming.push_back(songs[wrap].clone());
                                }
                            }
                            ps.next_linear_index = song_idx + 1 + PRELOAD_SIZE;
                        }

                        play_song(app, engine, &song, ps);
                        preload_upcoming(preloader, &ps.upcoming, preload_target);
                    }
                }
            } else {
                // Switch playlist
                if let Some(idx) = sidebar_state.selected_playlist() {
                    app.selected_playlist = idx;
                    let song_count = app.current_playlist_songs().len();
                    sidebar_state.set_song_count(song_count);
                }
            }
        }
        Action::PlayPause => {
            if engine.is_active() {
                if engine.is_paused() {
                    if let Some(pi) = ps.pause_instant.take() {
                        ps.paused_duration += pi.elapsed();
                    }
                    engine.resume();
                    app.is_playing = true;
                } else {
                    ps.pause_instant = Some(Instant::now());
                    engine.pause();
                    app.is_playing = false;
                }
            } else if let Some(song_idx) = sidebar_state.selected_song() {
                let songs = app.current_playlist_songs().to_vec();
                if let Some(song) = songs.get(song_idx).cloned() {
                    ps.history.clear();
                    ps.upcoming.clear();
                    ps.source_playlist = app.selected_playlist;
                    if app.shuffle {
                        ps.shuffle_pool = build_shuffle_pool(&songs, Some(&song), &ps.upcoming);
                        fill_upcoming_from_pool(&songs, Some(&song), ps, PRELOAD_SIZE, app.repeat);
                    } else {
                        ps.shuffle_pool.clear();
                        let start = song_idx + 1;
                        for i in 0..PRELOAD_SIZE {
                            let idx = start + i;
                            if idx < songs.len() {
                                ps.upcoming.push_back(songs[idx].clone());
                            } else if app.repeat == app::RepeatMode::All {
                                let wrap = idx % songs.len();
                                ps.upcoming.push_back(songs[wrap].clone());
                            }
                        }
                        ps.next_linear_index = song_idx + 1 + PRELOAD_SIZE;
                    }
                    play_song(app, engine, &song, ps);
                    preload_upcoming(preloader, &ps.upcoming, preload_target);
                }
            }
        }
        Action::Next => {
            if app.repeat == app::RepeatMode::One {
                // Repeat One: replay the same song
                if let Some(song) = ps.current_song.clone() {
                    play_song(app, engine, &song, ps);
                    sync_sidebar_selection(app, sidebar_state, &song);
                }
            } else if let Some(next_song) = ps.upcoming.pop_front() {
                // Push current to history
                if let Some(cur) = ps.current_song.take() {
                    ps.history.push(cur);
                }
                // Refill one more song
                refill_upcoming(app, ps, 1);
                play_song(app, engine, &next_song, ps);
                sync_sidebar_selection(app, sidebar_state, &next_song);
                preload_upcoming(preloader, &ps.upcoming, preload_target);
            }
        }
        Action::Previous => {
            if let Some(prev_song) = ps.history.pop() {
                // Push current song to the front of upcoming
                if let Some(cur) = ps.current_song.take() {
                    ps.upcoming.push_front(cur);
                    // Keep upcoming at max PRELOAD_SIZE
                    while ps.upcoming.len() > PRELOAD_SIZE {
                        ps.upcoming.pop_back();
                    }
                }
                play_song(app, engine, &prev_song, ps);
                sync_sidebar_selection(app, sidebar_state, &prev_song);
                preload_upcoming(preloader, &ps.upcoming, preload_target);
            }
        }
        Action::ToggleFavorite => {
            if app.screen == AppScreen::QueueView {
                if let Some(idx) = queue_state.selected() {
                    if let Some(item) = app.queue.items().get(idx) {
                        let id = item.id.clone();
                        app.toggle_favorite(&id);
                    }
                }
            } else if app.screen == AppScreen::Search {
                if let Some(song) = selected_search_song(app, search_state) {
                    app.toggle_favorite(&song.video_id);
                }
            } else if let Some(song_idx) = sidebar_state.selected_song() {
                let songs = app.current_playlist_songs();
                if let Some(song) = songs.get(song_idx) {
                    let vid = song.video_id.clone();
                    app.toggle_favorite(&vid);
                }
            }
        }
        Action::ToggleShuffle => {
            let was_shuffle = app.shuffle;
            app.toggle_shuffle();

            if ps.current_song.is_none() || was_shuffle == app.shuffle {
                return;
            }

            let songs: Vec<playlist::models::Song> = if ps.source_playlist < app.cached_playlists.len() {
                app.cached_playlists[ps.source_playlist].songs.clone()
            } else {
                return;
            };

            if songs.is_empty() {
                return;
            }

            let current = ps.current_song.clone();
            if let Some(current) = current.as_ref() {
                if app.shuffle {
                    ps.upcoming.clear();
                    ps.shuffle_pool = build_shuffle_pool(&songs, Some(current), &ps.upcoming);
                    fill_upcoming_from_pool(&songs, Some(current), ps, PRELOAD_SIZE, app.repeat);
                } else {
                    ps.shuffle_pool.clear();
                    rebuild_linear_upcoming(&songs, current, ps, app.repeat);
                }
                preload_upcoming(preloader, &ps.upcoming, preload_target);
            }
        }
        Action::AddToQueue => {
            if app.screen == AppScreen::Search {
                if let Some(song) = selected_search_song(app, search_state) {
                    app.add_to_queue(&song);
                    queue_state.set_count(app.queue.len());
                }
            } else if let Some(song_idx) = sidebar_state.selected_song() {
                let songs = app.current_playlist_songs();
                if let Some(song) = songs.get(song_idx).cloned() {
                    app.add_to_queue(&song);
                    queue_state.set_count(app.queue.len());
                }
            }
        }
        Action::AddToPlaylist => {
            if app.screen == AppScreen::Search {
                if let Some(song) = selected_search_song(app, search_state) {
                    add_song_to_selected_playlist(app, &song);
                    let song_count = app.current_playlist_songs().len();
                    sidebar_state.set_song_count(song_count);
                }
            }
        }
        Action::QueueMoveUp => {
            if let Some(idx) = queue_state.move_selection_up() {
                app.queue.move_up(idx);
            }
        }
        Action::QueueMoveDown => {
            if let Some(idx) = queue_state.move_selection_down() {
                app.queue.move_down(idx);
            }
        }
        Action::QueueRemove => {
            if let Some(idx) = queue_state.remove_selected() {
                app.queue.remove(idx);
            }
        }
        Action::Search => {
            app.screen = AppScreen::Search;
            app.search.clear_status();
            search_state.set_count(app.search.results.len());
            if !app.search.results.is_empty() {
                search_state.list_state.select(Some(0));
            }
        }
        Action::SearchInput(ch) => {
            app.search.query.push(ch);
            app.search.results.clear();
            app.search.last_query.clear();
            app.search.clear_status();
            search_state.set_count(0);
        }
        Action::SearchBackspace => {
            app.search.query.pop();
            app.search.results.clear();
            app.search.last_query.clear();
            app.search.clear_status();
            search_state.set_count(0);
        }
        Action::SearchClear => {
            app.search.query.clear();
            app.search.results.clear();
            app.search.last_query.clear();
            app.search.clear_status();
            search_state.set_count(0);
        }
        other => handler::apply_action(app, other),
    }
}

fn should_execute_search(search: &app::SearchState) -> bool {
    let query = search.query.trim();
    if query.is_empty() {
        return false;
    }
    search.results.is_empty() || search.last_query != query
}

fn execute_search(app: &mut App, search_state: &mut SearchViewState) {
    let query = app.search.query.trim().to_string();
    if query.is_empty() {
        app.search.error = Some("Type a search query".to_string());
        return;
    }

    app.search.clear_status();
    app.search.is_loading = true;

    let cache_dir = PathBuf::from(&app.settings.general.cache_dir);
    let cache = SearchCache::new(&cache_dir);
    if let Some(results) = cache.load(&query, SEARCH_CACHE_TTL_SECS) {
        app.search.results = results;
        app.search.cache_hit = true;
        app.search.is_loading = false;
        app.search.last_query = query.clone();
        search_state.set_count(app.search.results.len());
        if !app.search.results.is_empty() {
            search_state.list_state.select(Some(0));
        }
        return;
    }

    let cookies_path = PathBuf::from(&app.settings.paths.cookies);
    let cookies = if cookies_path.exists() {
        Some(cookies_path.as_path())
    } else {
        None
    };

    match tokio::runtime::Runtime::new() {
        Ok(rt) => match rt.block_on(sync::fetcher::search_youtube_music(
            &query,
            cookies,
            SEARCH_LIMIT,
        )) {
            Ok(results) => {
                app.search.results = results;
                app.search.cache_hit = false;
                app.search.error = None;
                app.search.status = None;
                let _ = cache.save(&query, &app.search.results);
            }
            Err(e) => {
                app.search.results.clear();
                app.search.error = Some(format!("Search failed: {e}"));
            }
        },
        Err(e) => {
            app.search.results.clear();
            app.search.error = Some(format!("Search runtime error: {e}"));
        }
    }

    app.search.is_loading = false;
    app.search.last_query = query;
    search_state.set_count(app.search.results.len());
    if !app.search.results.is_empty() {
        search_state.list_state.select(Some(0));
    }
}

fn selected_search_song(app: &App, search_state: &SearchViewState) -> Option<playlist::models::Song> {
    let idx = search_state.selected()?;
    app.search.results.get(idx).cloned()
}

fn play_selected_search(
    app: &mut App,
    search_state: &SearchViewState,
    engine: &mut AudioEngine,
    ps: &mut PlayState,
) {
    if let Some(song) = selected_search_song(app, search_state) {
        ps.history.clear();
        ps.upcoming.clear();
        ps.shuffle_pool.clear();
        ps.source_playlist = usize::MAX;
        ps.next_linear_index = 0;
        play_song(app, engine, &song, ps);
    }
}

fn add_song_to_selected_playlist(app: &mut App, song: &playlist::models::Song) {
    if app.selected_playlist >= app.cached_playlists.len() {
        app.search.error = Some("Select a playlist to add results".to_string());
        return;
    }

    let playlist = &mut app.cached_playlists[app.selected_playlist];
    if playlist.songs.iter().any(|s| s.video_id == song.video_id) {
        app.search.error = Some("Song already in playlist".to_string());
        return;
    }

    playlist.songs.push(song.clone());
    let cache_dir = PathBuf::from(&app.settings.general.cache_dir);
    let manager = PlaylistManager::new(&cache_dir);
    if let Err(err) = manager.save_playlist(playlist) {
        app.search.error = Some(format!("Failed to save playlist: {err}"));
        return;
    }

    let index = playlist::models::PlaylistIndex::from_playlists(&app.cached_playlists);
    let _ = manager.save_index(&index);
    app.search.error = None;
    app.search.status = Some("Added to playlist".to_string());
}

fn preload_upcoming(
    preloader: &Option<Preloader>,
    upcoming: &VecDeque<playlist::models::Song>,
    preload_target: usize,
) {
    if preload_target == 0 {
        return;
    }
    if let Some(preloader) = preloader.as_ref() {
        for song in upcoming.iter().take(preload_target) {
            preloader.enqueue(&song.url());
        }
    }
}

/// Sync the sidebar song selection to highlight the currently playing song.
fn sync_sidebar_selection(
    app: &App,
    sidebar_state: &mut PlaylistViewState,
    song: &playlist::models::Song,
) {
    // Only sync if we're viewing the same playlist that's playing
    let songs = app.current_playlist_songs();
    if let Some(idx) = songs.iter().position(|s| s.video_id == song.video_id) {
        sidebar_state.song_state.select(Some(idx));
    }
}

/// Build a shuffled pool of indices excluding the current song and upcoming entries.
fn build_shuffle_pool(
    songs: &[playlist::models::Song],
    exclude: Option<&playlist::models::Song>,
    upcoming: &VecDeque<playlist::models::Song>,
) -> VecDeque<usize> {
    use rand::seq::SliceRandom;
    use std::collections::HashSet;
    if songs.is_empty() {
        return VecDeque::new();
    }
    let mut rng = rand::thread_rng();
    let mut exclude_ids: HashSet<String> = HashSet::new();
    if let Some(excl) = exclude {
        exclude_ids.insert(excl.video_id.clone());
    }
    for song in upcoming {
        exclude_ids.insert(song.video_id.clone());
    }

    let mut indices: Vec<usize> = (0..songs.len()).collect();
    indices.retain(|&i| !exclude_ids.contains(&songs[i].video_id));
    indices.shuffle(&mut rng);
    VecDeque::from(indices)
}

/// Fill the upcoming queue from the shuffle pool until the target size is reached.
fn fill_upcoming_from_pool(
    songs: &[playlist::models::Song],
    current: Option<&playlist::models::Song>,
    ps: &mut PlayState,
    target_len: usize,
    repeat: app::RepeatMode,
) {
    while ps.upcoming.len() < target_len {
        if let Some(idx) = ps.shuffle_pool.pop_front() {
            ps.upcoming.push_back(songs[idx].clone());
            continue;
        }

        if repeat != app::RepeatMode::All {
            break;
        }

        ps.shuffle_pool = build_shuffle_pool(songs, current, &ps.upcoming);
        if ps.shuffle_pool.is_empty() {
            break;
        }
    }
}

/// Rebuild the upcoming queue in linear order starting after the current song.
fn rebuild_linear_upcoming(
    songs: &[playlist::models::Song],
    current: &playlist::models::Song,
    ps: &mut PlayState,
    repeat: app::RepeatMode,
) {
    ps.upcoming.clear();
    if let Some(song_idx) = songs.iter().position(|s| s.video_id == current.video_id) {
        let start = song_idx + 1;
        for i in 0..PRELOAD_SIZE {
            let idx = start + i;
            if idx < songs.len() {
                ps.upcoming.push_back(songs[idx].clone());
            } else if repeat == app::RepeatMode::All {
                let wrap = idx % songs.len();
                ps.upcoming.push_back(songs[wrap].clone());
            }
        }
        ps.next_linear_index = song_idx + 1 + PRELOAD_SIZE;
    }
}

/// Refill the upcoming queue with `count` more songs based on current mode.
fn refill_upcoming(app: &App, ps: &mut PlayState, count: usize) {
    let songs: Vec<playlist::models::Song> = if ps.source_playlist < app.cached_playlists.len() {
        app.cached_playlists[ps.source_playlist].songs.clone()
    } else {
        return;
    };

    if songs.is_empty() {
        return;
    }

    if app.shuffle {
        let target = ps.upcoming.len() + count;
        let current = ps.current_song.clone();
        fill_upcoming_from_pool(&songs, current.as_ref(), ps, target, app.repeat);
    } else {
        for _ in 0..count {
            let idx = ps.next_linear_index;
            if idx < songs.len() {
                ps.upcoming.push_back(songs[idx].clone());
                ps.next_linear_index += 1;
            } else if app.repeat == app::RepeatMode::All {
                let wrap = idx % songs.len();
                ps.upcoming.push_back(songs[wrap].clone());
                ps.next_linear_index += 1;
            }
        }
    }
}

/// Start playing a song via the audio engine.
fn play_song(
    app: &mut App,
    engine: &mut AudioEngine,
    song: &playlist::models::Song,
    ps: &mut PlayState,
) {
    let cookies_path = PathBuf::from(&app.settings.paths.cookies);
    let cookies = if cookies_path.exists() {
        Some(cookies_path.as_path())
    } else {
        None
    };

    let url = song.url();
    info!("Playing: {} ({})", song.title, url);

    match engine.play_url(&url, cookies) {
        Ok(()) => {
            app.is_playing = true;
            ps.current_song = Some(song.clone());
            ps.playback_start = Some(Instant::now());
            ps.paused_duration = Duration::ZERO;
            ps.pause_instant = None;
        }
        Err(e) => {
            error!("Failed to play {}: {e}", song.title);
            app.is_playing = false;
            ps.current_song = None;
            ps.playback_start = None;
            ps.paused_duration = Duration::ZERO;
            ps.pause_instant = None;
        }
    }
}

fn draw_ui(
    frame: &mut Frame,
    app: &App,
    theme: &Theme,
    sidebar_state: &mut PlaylistViewState,
    queue_state: &mut QueueViewState,
    search_state: &mut SearchViewState,
    spectrum: &mut SpectrumAnalyzer,
    current_song: &Option<playlist::models::Song>,
    playback_start: Option<Instant>,
    paused_duration: Duration,
    pause_instant: Option<Instant>,
    vis_samples: &[f32],
) {
    let layout = AppLayout::new(frame.area());

    // Header
    render_header(frame, layout.header, app, theme);

    // Sidebar
    let mut playlist_names: Vec<String> = app.playlists.iter().map(|p| p.name.clone()).collect();
    // Add virtual "★ Favoritos" playlist at the end
    let fav_count = app.favorites.count();
    playlist_names.push(format!("★ Favoritos ({fav_count})"));

    let songs = app.current_playlist_songs();
    let song_titles: Vec<String> = songs.iter().map(|s| s.title.clone()).collect();
    let song_video_ids: Vec<String> = songs.iter().map(|s| s.video_id.clone()).collect();
    let favorite_ids = app.favorites.all().clone();
    ui::playlist_view::render_sidebar(
        frame,
        layout.sidebar,
        &playlist_names,
        &song_titles,
        &song_video_ids,
        &favorite_ids,
        sidebar_state,
        theme,
    );

    // Main panel
    match app.screen {
        AppScreen::QueueView => {
            let queue_titles = app.queue_titles();
            let queue_ids: Vec<String> = app.queue.items().iter().map(|i| i.id.clone()).collect();
            let current_idx = app.queue_current_index();
            ui::queue_view::render_queue(
                frame,
                layout.main_panel,
                &queue_titles,
                current_idx,
                &favorite_ids,
                &queue_ids,
                queue_state,
                theme,
            );
        }
        AppScreen::Search => {
            let current_playlist = app
                .playlists
                .get(app.selected_playlist)
                .map(|p| p.name.as_str());
            ui::search_view::render_search(
                frame,
                layout.main_panel,
                &app.search.query,
                &app.search.results,
                search_state,
                app.search.is_loading,
                app.search.error.as_deref(),
                app.search.status.as_deref(),
                app.search.cache_hit,
                current_playlist,
                theme,
            );
        }
        AppScreen::Help => {
            render_help(frame, layout.main_panel, theme);
        }
        _ => {
            let mp_layout = MainPanelLayout::new(layout.main_panel);

            let now_playing = match current_song {
                Some(song) => {
                    let total_elapsed = playback_start.map(|s| s.elapsed()).unwrap_or(Duration::ZERO);
                    let current_pause = pause_instant.map(|pi| pi.elapsed()).unwrap_or(Duration::ZERO);
                    let mut elapsed = total_elapsed.saturating_sub(paused_duration + current_pause).as_secs();
                    let total = song.duration.unwrap_or(0);
                    // Cap elapsed at song duration
                    if total > 0 && elapsed > total {
                        elapsed = total;
                    }
                    NowPlayingInfo {
                        title: song.title.clone(),
                        artist: song.artist.clone(),
                        elapsed_secs: elapsed,
                        total_secs: total,
                        is_playing: app.is_playing,
                    }
                }
                None => NowPlayingInfo::empty(),
            };
            ui::now_playing::render_now_playing(frame, mp_layout.now_playing, &now_playing, theme);

            if app.show_visualizer {
                // Visualizer with real audio samples
                let bar_heights = spectrum.process(vis_samples).to_vec();
                ui::visualizer_view::render_visualizer(
                    frame,
                    mp_layout.visualizer,
                    &bar_heights,
                    theme,
                );
            } else {
                render_logo(frame, mp_layout.visualizer, theme);
            }
        }
    }

    // Controls
    ui::controls::render_controls(
        frame,
        layout.controls,
        app.is_playing,
        app.shuffle,
        app.repeat,
        app.volume,
        theme,
    );
}

fn render_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let shuffle_icon = if app.shuffle { " 🔀" } else { "" };
    let repeat_icon = match app.repeat {
        app::RepeatMode::None => "",
        app::RepeatMode::All => " 🔁",
        app::RepeatMode::One => " 🔂",
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " TermTube ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(theme.fg_dim),
        ),
        Span::raw(shuffle_icon),
        Span::raw(repeat_icon),
    ]));

    frame.render_widget(header, area);
}

fn render_logo(frame: &mut Frame, area: Rect, theme: &Theme) {
    use ratatui::widgets::{Block, Borders};

    let block = Block::default()
        .title(" TermTube ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let logo_lines = [
        ".............................................",
        ". ▓▓▓▓▓▓▓  ▓▓▓▓▓▓▓  ▓▓    ▓▓ ▓▓▓▓▓▓  ▓▓▓▓▓▓▓.",
        ".    ▓▓      ▓▓     ▓▓    ▓▓ ▓▓   ▓▓ ▓▓     .",
        ".    ▓▓      ▓▓     ▓▓    ▓▓ ▓▓▓▓▓▓  ▓▓▓▓▓  .",
        ".    ▓▓      ▓▓     ▓▓    ▓▓ ▓▓   ▓▓ ▓▓     .",
        ".    ▓▓      ▓▓      ▓▓▓▓▓▓  ▓▓▓▓▓▓  ▓▓▓▓▓▓▓.",
        ".............................................",
        "          TERMINAL  PLAYER",
    ];

    let height = inner.height as usize;
    let width = inner.width as usize;
    let total_lines = logo_lines.len();
    let y_offset = if height > total_lines {
        (height - total_lines) / 2
    } else {
        0
    };

    let buf = frame.buffer_mut();
    for (i, line) in logo_lines.iter().enumerate() {
        let y = inner.y + (y_offset + i) as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let char_count = line.chars().count();
        let x_offset = if width > char_count {
            (width - char_count) / 2
        } else {
            0
        };
        let x = inner.x + x_offset as u16;
        for (j, ch) in line.chars().enumerate() {
            let cx = x + j as u16;
            if cx < inner.x + inner.width {
                let cell = &mut buf[(cx, y)];
                cell.set_char(ch);
                cell.set_style(Style::default().fg(theme.primary));
            }
        }
    }
}

fn render_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let help_text = vec![
        Line::from(Span::styled(
            " Keyboard Shortcuts",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(" Space      Play / Pause"),
        Line::from(" n          Next track"),
        Line::from(" p          Previous track"),
        Line::from(" s          Toggle shuffle"),
        Line::from(" r          Cycle repeat (off → all → one)"),
        Line::from(" f          Toggle favorite"),
        Line::from(" +/-        Volume up / down"),
        Line::from(" Tab        Switch focus (playlists ↔ songs)"),
        Line::from(" j/k, ↑/↓   Navigate lists"),
        Line::from(" Enter      Select / play"),
        Line::from(" Q          Toggle queue view"),
        Line::from(" v          Toggle visualizer / logo"),
        Line::from(" /, F       Search"),
        Line::from(" Search: Enter = search/play, Ctrl+Q queue, Ctrl+F favorite"),
        Line::from(" Search: Ctrl+L add to playlist, Ctrl+U clear"),
        Line::from(" ?          Toggle this help"),
        Line::from(" q / Esc    Quit"),
    ];

    let block = ratatui::widgets::Block::default()
        .title(" Help ")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused));

    let paragraph = Paragraph::new(help_text).block(block);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Ignorar por defecto, ejecutar manualmente con --ignored
    async fn test_real_youtube_search_with_cookies() {
        use std::path::Path;
        use crate::sync::fetcher::{search_youtube_music, check_ytdlp};

        // Verificar que yt-dlp esté disponible
        check_ytdlp().await.expect("yt-dlp debe estar instalado");

        // Usar cookies.txt si existe
        let cookies_path = Path::new("cookies.txt");
        let cookies = if cookies_path.exists() {
            Some(cookies_path)
        } else {
            panic!("cookies.txt no encontrado. Necesitas un archivo de cookies válido para YouTube.");
        };

        // Realizar búsqueda
        let query = "lofi beats";
        let limit = 5;
        let results = search_youtube_music(query, cookies, limit).await.expect("La búsqueda debe ser exitosa");

        // Verificar resultados
        assert!(!results.is_empty(), "Debe haber al menos un resultado");
        assert!(results.len() <= limit, "No debe haber más resultados que el límite");

        // Verificar que cada canción tenga campos requeridos
        for song in &results {
            assert!(!song.title.is_empty(), "El título no debe estar vacío");
            assert!(!song.video_id.is_empty(), "El video_id no debe estar vacío");
            assert!(!song.artist.is_empty(), "El artista no debe estar vacío");
            // duration puede ser None
        }

        // Verificar que los resultados sean relevantes (opcional, pero útil)
        let titles_lower: Vec<String> = results.iter().map(|s| s.title.to_lowercase()).collect();
        let has_relevant = titles_lower.iter().any(|t| t.contains("lofi") || t.contains("beats"));
        assert!(has_relevant, "Al menos un resultado debe ser relevante para la query");
    }
}
