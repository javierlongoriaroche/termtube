use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::msg::Action;
use super::state::{Focus, Section};

/// Help lines shown in the `?` overlay: (keys, description).
pub const HELP: &[(&str, &str)] = &[
    ("1 2 3", "Home / Search / Library"),
    ("/", "Search"),
    ("j k ↑ ↓", "Move"),
    ("g G PgUp PgDn", "Jump"),
    ("Enter", "Play song / open album, playlist, artist"),
    ("Esc Backspace", "Back"),
    ("Space", "Play / pause"),
    ("n p", "Next / previous"),
    ("← →", "Seek -/+ 5 s"),
    ("+ -", "Volume"),
    ("s r", "Shuffle / repeat"),
    ("f", "Like / unlike current song"),
    ("a A", "Add to queue / play next"),
    ("R", "Start radio from selection"),
    ("Q l", "Queue panel / lyrics panel"),
    ("Tab", "Switch focus list ↔ queue"),
    ("K J d", "Queue: move up / down / remove"),
    ("v", "Level meter on/off"),
    ("Ctrl-r", "Reload page"),
    ("?", "Help"),
    ("q Ctrl-c", "Quit"),
];

/// Maps a key press to an action given the current focus.
pub fn map_key(key: KeyEvent, focus: Focus) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }
    if ctrl && key.code == KeyCode::Char('r') {
        return Some(Action::Refresh);
    }

    if focus == Focus::SearchInput {
        return match key.code {
            KeyCode::Enter => Some(Action::SearchSubmit),
            KeyCode::Esc => Some(Action::Back),
            KeyCode::Tab => Some(Action::SearchNextFilter),
            KeyCode::Backspace => Some(Action::SearchBackspace),
            KeyCode::Char('u') if ctrl => Some(Action::SearchClear),
            KeyCode::Down => Some(Action::Down),
            KeyCode::Char(c) if !ctrl => Some(Action::SearchChar(c)),
            _ => None,
        };
    }

    if focus == Focus::Queue {
        match key.code {
            KeyCode::Char('K') => return Some(Action::QueueMoveUp),
            KeyCode::Char('J') => return Some(Action::QueueMoveDown),
            KeyCode::Char('d') | KeyCode::Delete => return Some(Action::QueueRemove),
            _ => {}
        }
    }

    let action = match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('?') => Action::ToggleHelp,
        KeyCode::Char('1') => Action::GoTo(Section::Home),
        KeyCode::Char('2') => Action::GoTo(Section::Search),
        KeyCode::Char('3') => Action::GoTo(Section::Library),
        KeyCode::Char('/') => Action::FocusSearch,
        KeyCode::Tab => Action::ToggleFocus,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Home | KeyCode::Char('g') => Action::Top,
        KeyCode::End | KeyCode::Char('G') => Action::Bottom,
        KeyCode::Enter => Action::Activate,
        KeyCode::Esc | KeyCode::Backspace => Action::Back,
        KeyCode::Char(' ') => Action::PlayPause,
        KeyCode::Char('n') => Action::Next,
        KeyCode::Char('p') => Action::Previous,
        KeyCode::Right => Action::SeekForward,
        KeyCode::Left => Action::SeekBackward,
        KeyCode::Char('+') | KeyCode::Char('=') => Action::VolumeUp,
        KeyCode::Char('-') => Action::VolumeDown,
        KeyCode::Char('s') => Action::ToggleShuffle,
        KeyCode::Char('r') => Action::CycleRepeat,
        KeyCode::Char('f') => Action::ToggleLike,
        KeyCode::Char('a') => Action::AddToQueue,
        KeyCode::Char('A') => Action::PlayNext,
        KeyCode::Char('R') => Action::StartRadio,
        KeyCode::Char('Q') => Action::ToggleQueuePanel,
        KeyCode::Char('l') => Action::ToggleLyricsPanel,
        KeyCode::Char('v') => Action::ToggleMeter,
        _ => return None,
    };
    Some(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_global_keys() {
        assert_eq!(
            map_key(key(KeyCode::Char(' ')), Focus::Main),
            Some(Action::PlayPause)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('2')), Focus::Main),
            Some(Action::GoTo(Section::Search))
        );
        assert_eq!(map_key(KeyCode::Char('c').into(), Focus::Main), None);
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Focus::SearchInput
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn test_search_input_captures_letters() {
        assert_eq!(
            map_key(key(KeyCode::Char('q')), Focus::SearchInput),
            Some(Action::SearchChar('q'))
        );
        assert_eq!(
            map_key(key(KeyCode::Char(' ')), Focus::SearchInput),
            Some(Action::SearchChar(' '))
        );
        assert_eq!(
            map_key(key(KeyCode::Enter), Focus::SearchInput),
            Some(Action::SearchSubmit)
        );
        assert_eq!(
            map_key(key(KeyCode::Tab), Focus::SearchInput),
            Some(Action::SearchNextFilter)
        );
        assert_eq!(
            map_key(key(KeyCode::Esc), Focus::SearchInput),
            Some(Action::Back)
        );
    }

    #[test]
    fn test_queue_focus_has_reorder_keys() {
        assert_eq!(
            map_key(key(KeyCode::Char('K')), Focus::Queue),
            Some(Action::QueueMoveUp)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('d')), Focus::Queue),
            Some(Action::QueueRemove)
        );
        assert_eq!(map_key(key(KeyCode::Char('d')), Focus::Main), None);
        assert_eq!(
            map_key(key(KeyCode::Char('j')), Focus::Queue),
            Some(Action::Down)
        );
    }
}
