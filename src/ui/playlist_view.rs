use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::ui::theme::Theme;

/// Which pane within the sidebar is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarFocus {
    Playlists,
    Songs,
}

/// State for the sidebar: playlist list + song list.
pub struct PlaylistViewState {
    pub focus: SidebarFocus,
    pub playlist_state: ListState,
    pub song_state: ListState,
    playlist_count: usize,
    song_count: usize,
}

impl PlaylistViewState {
    pub fn new() -> Self {
        let mut playlist_state = ListState::default();
        playlist_state.select(Some(0));
        Self {
            focus: SidebarFocus::Playlists,
            playlist_state,
            song_state: ListState::default(),
            playlist_count: 0,
            song_count: 0,
        }
    }

    pub fn set_playlist_count(&mut self, count: usize) {
        self.playlist_count = count;
        if count > 0 && self.playlist_state.selected().is_none() {
            self.playlist_state.select(Some(0));
        }
    }

    pub fn set_song_count(&mut self, count: usize) {
        self.song_count = count;
        if count > 0 && self.song_state.selected().is_none() {
            self.song_state.select(Some(0));
        } else if count == 0 {
            self.song_state.select(None);
        }
    }

    pub fn selected_playlist(&self) -> Option<usize> {
        self.playlist_state.selected()
    }

    pub fn selected_song(&self) -> Option<usize> {
        self.song_state.selected()
    }

    pub fn next(&mut self) {
        let (state, count) = match self.focus {
            SidebarFocus::Playlists => (&mut self.playlist_state, self.playlist_count),
            SidebarFocus::Songs => (&mut self.song_state, self.song_count),
        };
        if count == 0 {
            return;
        }
        let i = state.selected().map_or(0, |i| (i + 1) % count);
        state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let (state, count) = match self.focus {
            SidebarFocus::Playlists => (&mut self.playlist_state, self.playlist_count),
            SidebarFocus::Songs => (&mut self.song_state, self.song_count),
        };
        if count == 0 {
            return;
        }
        let i = state
            .selected()
            .map_or(0, |i| if i == 0 { count - 1 } else { i - 1 });
        state.select(Some(i));
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            SidebarFocus::Playlists => SidebarFocus::Songs,
            SidebarFocus::Songs => SidebarFocus::Playlists,
        };
    }
}

/// Render the sidebar with playlists (top) and songs (bottom).
pub fn render_sidebar(
    frame: &mut Frame,
    area: Rect,
    playlist_names: &[String],
    song_titles: &[String],
    song_video_ids: &[String],
    favorite_ids: &std::collections::HashSet<String>,
    state: &mut PlaylistViewState,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Playlists list
    let pl_focused = state.focus == SidebarFocus::Playlists;
    let pl_border_color = if pl_focused {
        theme.border_focused
    } else {
        theme.border
    };

    let pl_items: Vec<ListItem> = playlist_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let prefix = if Some(i) == state.playlist_state.selected() {
                "▶ "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(name.clone(), Style::default().fg(theme.fg)),
            ]))
        })
        .collect();

    let pl_block = Block::default()
        .title(" Playlists ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pl_border_color));

    let pl_list = List::new(pl_items)
        .block(pl_block)
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(pl_list, chunks[0], &mut state.playlist_state);

    // Songs list — with favorite indicator
    let song_focused = state.focus == SidebarFocus::Songs;
    let song_border_color = if song_focused {
        theme.border_focused
    } else {
        theme.border
    };

    let song_items: Vec<ListItem> = song_titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let fav = if song_video_ids.get(i).map_or(false, |id| favorite_ids.contains(id)) {
                "♥ "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::styled(fav, Style::default().fg(theme.secondary)),
                Span::styled(format!("{title}"), Style::default().fg(theme.fg)),
            ]))
        })
        .collect();

    let song_block = Block::default()
        .title(" Songs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(song_border_color));

    let song_list = List::new(song_items)
        .block(song_block)
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(song_list, chunks[1], &mut state.song_state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playlist_view_navigation() {
        let mut state = PlaylistViewState::new();
        state.set_playlist_count(3);

        assert_eq!(state.selected_playlist(), Some(0));
        state.next();
        assert_eq!(state.selected_playlist(), Some(1));
        state.next();
        assert_eq!(state.selected_playlist(), Some(2));
        state.next(); // wraps
        assert_eq!(state.selected_playlist(), Some(0));
    }

    #[test]
    fn test_playlist_view_previous_wraps() {
        let mut state = PlaylistViewState::new();
        state.set_playlist_count(3);

        assert_eq!(state.selected_playlist(), Some(0));
        state.previous(); // wraps to end
        assert_eq!(state.selected_playlist(), Some(2));
    }

    #[test]
    fn test_toggle_focus() {
        let mut state = PlaylistViewState::new();
        assert_eq!(state.focus, SidebarFocus::Playlists);
        state.toggle_focus();
        assert_eq!(state.focus, SidebarFocus::Songs);
        state.toggle_focus();
        assert_eq!(state.focus, SidebarFocus::Playlists);
    }

    #[test]
    fn test_song_navigation() {
        let mut state = PlaylistViewState::new();
        state.set_song_count(5);
        state.focus = SidebarFocus::Songs;

        assert_eq!(state.selected_song(), Some(0));
        state.next();
        assert_eq!(state.selected_song(), Some(1));
    }

    #[test]
    fn test_empty_list_navigation() {
        let mut state = PlaylistViewState::new();
        state.set_playlist_count(0);
        state.next(); // should not panic
        assert_eq!(state.selected_playlist(), Some(0)); // initial default
    }
}
