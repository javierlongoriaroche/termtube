use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppScreen};
use crate::config::settings::KeybindingSettings;

/// Actions that the input handler can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    ToggleShuffle,
    CycleRepeat,
    ToggleFavorite,
    Search,
    ToggleQueue,
    NavigateUp,
    NavigateDown,
    Select,
    ToggleFocus,
    Back,
    Help,
    QueueMoveUp,
    QueueMoveDown,
    QueueRemove,
    AddToQueue,
    ToggleVisualizer,
    DownloadCurrentSong,
    DownloadSelectedItem,
    DownloadCurrentPlaylist,
    SearchInput(char),
    SearchBackspace,
    SearchClear,
    AddToPlaylist,
    None,
}

/// Map a key event to an Action based on the configured keybindings.
pub fn map_key_event(key: KeyEvent, keybindings: &KeybindingSettings, screen: AppScreen) -> Action {
    // Global shortcuts (Ctrl+C always quits)
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    // Global download shortcuts
    let is_shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let is_alt = key.modifiers.contains(KeyModifiers::ALT);
    if is_shift && matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D')) {
        if is_alt {
            return Action::DownloadCurrentPlaylist;
        }
        return Action::DownloadSelectedItem;
    }

    // Escape goes back or quits depending on screen
    if key.code == KeyCode::Esc {
        return match screen {
            AppScreen::Main => Action::Quit,
            _ => Action::Back,
        };
    }

    if screen == AppScreen::Search {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') => return Action::AddToQueue,
                KeyCode::Char('f') => return Action::ToggleFavorite,
                KeyCode::Char('l') => return Action::AddToPlaylist,
                KeyCode::Char('u') => return Action::SearchClear,
                _ => {}
            }
        }

        if key.code == KeyCode::Backspace {
            return Action::SearchBackspace;
        }

        if let KeyCode::Char(c) = key.code {
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                return Action::SearchInput(c);
            }
        }
    }

    // Queue-specific keys (Shift+K/J to reorder, d to remove, a to add)
    if screen == AppScreen::QueueView {
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Char('K') => return Action::QueueMoveUp,
                KeyCode::Char('J') => return Action::QueueMoveDown,
                _ => {}
            }
        }
        match key.code {
            KeyCode::Char('d') => return Action::QueueRemove,
            _ => {}
        }
    }

    // 'a' adds selected song to queue (from main screen song list)
    if screen == AppScreen::Main {
        if key.code == KeyCode::Char('a') {
            return Action::AddToQueue;
        }
        if key.code == KeyCode::Char('v') {
            return Action::ToggleVisualizer;
        }
    }

    // Navigation keys work on all screens
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => return Action::NavigateUp,
        KeyCode::Down | KeyCode::Char('j') => return Action::NavigateDown,
        KeyCode::Enter => return Action::Select,
        KeyCode::Tab => return Action::ToggleFocus,
        _ => {}
    }

    // Match against configurable keybindings
    let key_str = key_event_to_string(key);

    if key_str == keybindings.quit {
        return Action::Quit;
    }
    if key_str == keybindings.play_pause {
        return Action::PlayPause;
    }
    if key_str == keybindings.next {
        return Action::Next;
    }
    if key_str == keybindings.previous {
        return Action::Previous;
    }
    if key_str == keybindings.volume_up {
        return Action::VolumeUp;
    }
    if key_str == keybindings.volume_down {
        return Action::VolumeDown;
    }
    if key_str == keybindings.shuffle {
        return Action::ToggleShuffle;
    }
    if key_str == keybindings.repeat {
        return Action::CycleRepeat;
    }
    if key_str == keybindings.favorite {
        return Action::ToggleFavorite;
    }
    if key_str == keybindings.search || key_str == keybindings.search_alt {
        return Action::Search;
    }
    if key_str == keybindings.queue {
        return Action::ToggleQueue;
    }

    if key_str == "?" {
        return Action::Help;
    }

    Action::None
}

/// Convert a KeyEvent to a string that can match the config keybinding strings.
fn key_event_to_string(key: KeyEvent) -> String {
    match key.code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        _ => String::new(),
    }
}

/// Apply an action to the app state.
pub fn apply_action(app: &mut App, action: Action) {
    match action {
        Action::Quit => app.quit(),
        Action::VolumeUp => app.volume_up(),
        Action::VolumeDown => app.volume_down(),
        Action::ToggleShuffle => app.toggle_shuffle(),
        Action::CycleRepeat => app.cycle_repeat(),
        Action::ToggleQueue => {
            app.screen = if app.screen == AppScreen::QueueView {
                AppScreen::Main
            } else {
                AppScreen::QueueView
            };
        }
        Action::Help => {
            app.screen = if app.screen == AppScreen::Help {
                AppScreen::Main
            } else {
                AppScreen::Help
            };
        }
        Action::Back => {
            app.screen = AppScreen::Main;
        }
        Action::ToggleVisualizer => {
            app.toggle_visualizer();
        }
        // PlayPause, Next, Previous, Select, Navigate, Favorite, Search
        // are handled by the TUI run loop which has access to more state
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_kb() -> KeybindingSettings {
        KeybindingSettings::default()
    }

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_quit_mapping() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Char('q')), &kb, AppScreen::Main);
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn test_ctrl_c_always_quits() {
        let kb = default_kb();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = map_key_event(key, &kb, AppScreen::Main);
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn test_space_play_pause() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Char(' ')), &kb, AppScreen::Main);
        assert_eq!(action, Action::PlayPause);
    }

    #[test]
    fn test_download_selected_item_mapping() {
        let kb = default_kb();
        let key = KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT);
        let action = map_key_event(key, &kb, AppScreen::Main);
        assert_eq!(action, Action::DownloadSelectedItem);
    }

    #[test]
    fn test_download_current_playlist_mapping() {
        let kb = default_kb();
        let key = KeyEvent::new(
            KeyCode::Char('D'),
            KeyModifiers::ALT | KeyModifiers::SHIFT,
        );
        let action = map_key_event(key, &kb, AppScreen::Main);
        assert_eq!(action, Action::DownloadCurrentPlaylist);
    }

    #[test]
    fn test_search_alt_key_mapping() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Char('F')), &kb, AppScreen::Main);
        assert_eq!(action, Action::Search);
    }

    #[test]
    fn test_lowercase_f_still_favorite() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Char('f')), &kb, AppScreen::Main);
        assert_eq!(action, Action::ToggleFavorite);
    }

    #[test]
    fn test_navigation_keys() {
        let kb = default_kb();
        assert_eq!(
            map_key_event(make_key(KeyCode::Up), &kb, AppScreen::Main),
            Action::NavigateUp
        );
        assert_eq!(
            map_key_event(make_key(KeyCode::Down), &kb, AppScreen::Main),
            Action::NavigateDown
        );
        assert_eq!(
            map_key_event(make_key(KeyCode::Char('j')), &kb, AppScreen::Main),
            Action::NavigateDown
        );
        assert_eq!(
            map_key_event(make_key(KeyCode::Char('k')), &kb, AppScreen::Main),
            Action::NavigateUp
        );
    }

    #[test]
    fn test_tab_toggles_focus() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Tab), &kb, AppScreen::Main);
        assert_eq!(action, Action::ToggleFocus);
    }

    #[test]
    fn test_esc_on_main_quits() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Esc), &kb, AppScreen::Main);
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn test_esc_on_help_goes_back() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Esc), &kb, AppScreen::Help);
        assert_eq!(action, Action::Back);
    }

    fn make_test_app() -> (App, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let fav_path = tmp.path().join("favorites.json");
        let app = App::new_with_favorites_path(
            crate::config::settings::Settings::default(),
            vec![],
            fav_path,
        );
        (app, tmp)
    }

    #[test]
    fn test_apply_quit() {
        let (mut app, _tmp) = make_test_app();
        apply_action(&mut app, Action::Quit);
        assert!(!app.running);
    }

    #[test]
    fn test_apply_volume() {
        let (mut app, _tmp) = make_test_app();
        let initial = app.volume;
        apply_action(&mut app, Action::VolumeUp);
        assert_eq!(app.volume, initial + 5);
        apply_action(&mut app, Action::VolumeDown);
        assert_eq!(app.volume, initial);
    }

    #[test]
    fn test_apply_toggle_queue() {
        let (mut app, _tmp) = make_test_app();
        assert_eq!(app.screen, AppScreen::Main);
        apply_action(&mut app, Action::ToggleQueue);
        assert_eq!(app.screen, AppScreen::QueueView);
        apply_action(&mut app, Action::ToggleQueue);
        assert_eq!(app.screen, AppScreen::Main);
    }

    #[test]
    fn test_shift_k_moves_up_in_queue() {
        let kb = default_kb();
        let key = KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT);
        let action = map_key_event(key, &kb, AppScreen::QueueView);
        assert_eq!(action, Action::QueueMoveUp);
    }

    #[test]
    fn test_shift_j_moves_down_in_queue() {
        let kb = default_kb();
        let key = KeyEvent::new(KeyCode::Char('J'), KeyModifiers::SHIFT);
        let action = map_key_event(key, &kb, AppScreen::QueueView);
        assert_eq!(action, Action::QueueMoveDown);
    }

    #[test]
    fn test_d_removes_in_queue() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Char('d')), &kb, AppScreen::QueueView);
        assert_eq!(action, Action::QueueRemove);
    }

    #[test]
    fn test_a_adds_to_queue_on_main() {
        let kb = default_kb();
        let action = map_key_event(make_key(KeyCode::Char('a')), &kb, AppScreen::Main);
        assert_eq!(action, Action::AddToQueue);
    }

    #[test]
    fn test_a_does_not_add_in_queue_view() {
        let kb = default_kb();
        // 'a' in QueueView should NOT be AddToQueue (it's not a queue-specific action)
        let action = map_key_event(make_key(KeyCode::Char('a')), &kb, AppScreen::QueueView);
        assert_ne!(action, Action::AddToQueue);
    }

    #[test]
    fn test_shift_k_on_main_is_navigate() {
        let kb = default_kb();
        // Shift+K on main screen should be navigate up (k maps to NavigateUp)
        let key = KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT);
        let action = map_key_event(key, &kb, AppScreen::Main);
        // On main screen, Shift+K should not be QueueMoveUp
        assert_ne!(action, Action::QueueMoveUp);
    }
}
