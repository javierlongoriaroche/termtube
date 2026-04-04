use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::ui::theme::Theme;

/// State for the queue view panel.
pub struct QueueViewState {
    pub list_state: ListState,
    item_count: usize,
}

impl QueueViewState {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            item_count: 0,
        }
    }

    pub fn set_count(&mut self, count: usize) {
        self.item_count = count;
        if count > 0 && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        } else if count == 0 {
            self.list_state.select(None);
        }
    }

    pub fn next(&mut self) {
        if self.item_count == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map_or(0, |i| (i + 1) % self.item_count);
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.item_count == 0 {
            return;
        }
        let i =
            self.list_state
                .selected()
                .map_or(0, |i| if i == 0 { self.item_count - 1 } else { i - 1 });
        self.list_state.select(Some(i));
    }

    pub fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    /// Move selected item up in the visual list. Returns new selected index.
    pub fn move_selection_up(&mut self) -> Option<usize> {
        if let Some(idx) = self.list_state.selected() {
            if idx > 0 {
                let new_idx = idx - 1;
                self.list_state.select(Some(new_idx));
                return Some(idx);
            }
        }
        None
    }

    /// Move selected item down in the visual list. Returns current selected index.
    pub fn move_selection_down(&mut self) -> Option<usize> {
        if let Some(idx) = self.list_state.selected() {
            if idx + 1 < self.item_count {
                let new_idx = idx + 1;
                self.list_state.select(Some(new_idx));
                return Some(idx);
            }
        }
        None
    }

    /// Remove the selected item. Adjusts selection.
    pub fn remove_selected(&mut self) -> Option<usize> {
        if let Some(idx) = self.list_state.selected() {
            if self.item_count == 0 {
                return None;
            }
            self.item_count -= 1;
            if self.item_count == 0 {
                self.list_state.select(None);
            } else if idx >= self.item_count {
                self.list_state.select(Some(self.item_count - 1));
            }
            return Some(idx);
        }
        None
    }
}

/// Render the queue overlay/panel.
pub fn render_queue(
    frame: &mut Frame,
    area: Rect,
    queue_titles: &[String],
    current_index: Option<usize>,
    favorite_ids: &std::collections::HashSet<String>,
    queue_ids: &[String],
    state: &mut QueueViewState,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(area);

    let block = Block::default()
        .title(" Queue ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused));

    let items: Vec<ListItem> = queue_titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let playing = if Some(i) == current_index {
                "▶ "
            } else {
                "  "
            };
            let fav_icon = if queue_ids
                .get(i)
                .map_or(false, |id| favorite_ids.contains(id))
            {
                "♥ "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::styled(playing, Style::default().fg(theme.primary)),
                Span::styled(fav_icon, Style::default().fg(theme.secondary)),
                Span::styled(title.clone(), Style::default().fg(theme.fg)),
            ]))
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(theme.highlight_bg)
            .fg(theme.highlight_fg)
            .add_modifier(Modifier::BOLD),
    );

    frame.render_stateful_widget(list, chunks[0], &mut state.list_state);

    // Key hints
    let hints = Paragraph::new(Line::from(vec![
        Span::styled(
            " Shift+K/J ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("move  ", Style::default().fg(theme.fg_dim)),
        Span::styled(
            "d ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("remove  ", Style::default().fg(theme.fg_dim)),
        Span::styled(
            "f ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("favorite  ", Style::default().fg(theme.fg_dim)),
        Span::styled(
            "q/Esc ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("back", Style::default().fg(theme.fg_dim)),
    ]));
    frame.render_widget(hints, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_view_navigation() {
        let mut state = QueueViewState::new();
        state.set_count(3);

        assert_eq!(state.list_state.selected(), Some(0));
        state.next();
        assert_eq!(state.list_state.selected(), Some(1));
        state.next();
        assert_eq!(state.list_state.selected(), Some(2));
        state.next();
        assert_eq!(state.list_state.selected(), Some(0));
    }

    #[test]
    fn test_queue_empty() {
        let mut state = QueueViewState::new();
        state.set_count(0);
        state.next(); // should not panic
        assert_eq!(state.list_state.selected(), None);
    }

    #[test]
    fn test_move_selection_up() {
        let mut state = QueueViewState::new();
        state.set_count(3);
        state.next(); // select index 1

        let idx = state.move_selection_up();
        assert_eq!(idx, Some(1)); // returns the old index (the item to move)
        assert_eq!(state.selected(), Some(0)); // selection moved up
    }

    #[test]
    fn test_move_selection_up_at_top() {
        let mut state = QueueViewState::new();
        state.set_count(3);
        // at index 0
        let idx = state.move_selection_up();
        assert_eq!(idx, None); // can't move up from 0
    }

    #[test]
    fn test_move_selection_down() {
        let mut state = QueueViewState::new();
        state.set_count(3);
        // at index 0
        let idx = state.move_selection_down();
        assert_eq!(idx, Some(0)); // returns old index
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn test_move_selection_down_at_bottom() {
        let mut state = QueueViewState::new();
        state.set_count(3);
        state.next(); // 1
        state.next(); // 2

        let idx = state.move_selection_down();
        assert_eq!(idx, None); // can't move down from last
    }

    #[test]
    fn test_remove_selected() {
        let mut state = QueueViewState::new();
        state.set_count(3);
        state.next(); // select 1

        let removed = state.remove_selected();
        assert_eq!(removed, Some(1));
        // count is now 2, selection stays at 1
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn test_remove_selected_last_item() {
        let mut state = QueueViewState::new();
        state.set_count(2);
        state.next(); // select 1 (last)

        let removed = state.remove_selected();
        assert_eq!(removed, Some(1));
        // count is now 1, selection should move to 0
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn test_remove_selected_only_item() {
        let mut state = QueueViewState::new();
        state.set_count(1);

        let removed = state.remove_selected();
        assert_eq!(removed, Some(0));
        assert_eq!(state.selected(), None);
    }
}
