use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::playlist::models::Song;
use crate::ui::theme::Theme;

pub struct SearchViewState {
    pub list_state: ListState,
    item_count: usize,
}

impl SearchViewState {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            list_state,
            item_count: 0,
        }
    }

    pub fn set_count(&mut self, count: usize) {
        self.item_count = count;
        if count > 0 && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        } else if count == 0 {
            self.list_state.select(None);
        } else if let Some(selected) = self.list_state.selected() {
            if selected >= count {
                self.list_state.select(Some(count - 1));
            }
        }
    }

    pub fn next(&mut self) {
        if self.item_count == 0 {
            return;
        }
        let i = self.list_state.selected().map_or(0, |i| (i + 1) % self.item_count);
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.item_count == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map_or(0, |i| if i == 0 { self.item_count - 1 } else { i - 1 });
        self.list_state.select(Some(i));
    }

    pub fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }
}

pub fn render_search(
    frame: &mut Frame,
    area: Rect,
    query: &str,
    results: &[Song],
    state: &mut SearchViewState,
    is_loading: bool,
    error: Option<&str>,
    status: Option<&str>,
    cache_hit: bool,
    current_playlist_name: Option<&str>,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(2)])
        .split(area);

    let input_block = Block::default()
        .title(" Search (YouTube Music) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused));
    let input_inner = input_block.inner(chunks[0]);
    frame.render_widget(input_block, chunks[0]);

    let query_display = if query.is_empty() {
        Span::styled("Type to search...", Style::default().fg(theme.fg_dim))
    } else {
        Span::styled(query.to_string(), Style::default().fg(theme.fg))
    };

    let input_line = Line::from(vec![Span::raw(" "), query_display, Span::raw("|")]);
    frame.render_widget(Paragraph::new(input_line), input_inner);

    let list_block = Block::default()
        .title(" Results ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let items: Vec<ListItem> = results
        .iter()
        .map(|song| {
            let duration = song.duration_display();
            let subtitle = if song.artist.is_empty() {
                duration
            } else {
                format!("{} - {}", song.artist, duration)
            };
            ListItem::new(Line::from(vec![
                Span::styled(song.title.clone(), Style::default().fg(theme.fg)),
                Span::raw("  "),
                Span::styled(subtitle, Style::default().fg(theme.fg_dim)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(list_block)
        .highlight_style(
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, chunks[1], &mut state.list_state);

    let status = if let Some(msg) = error {
        Line::from(Span::styled(msg, Style::default().fg(theme.secondary)))
    } else if is_loading {
        Line::from(Span::styled("Searching...", Style::default().fg(theme.primary)))
    } else if let Some(msg) = status {
        Line::from(Span::styled(msg, Style::default().fg(theme.fg_dim)))
    } else if results.is_empty() {
        let msg = if query.is_empty() {
            "Type a query and press Enter"
        } else {
            "No results"
        };
        Line::from(Span::styled(msg, Style::default().fg(theme.fg_dim)))
    } else if cache_hit {
        Line::from(Span::styled("Cached results", Style::default().fg(theme.fg_dim)))
    } else {
        Line::from(Span::raw(""))
    };

    let playlist_hint = current_playlist_name.unwrap_or("(no playlist)");
    let hints = Line::from(vec![
        Span::styled(" Enter ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("play  ", Style::default().fg(theme.fg_dim)),
        Span::styled("Up/Down ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("navigate  ", Style::default().fg(theme.fg_dim)),
        Span::styled("Ctrl+Q ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("queue  ", Style::default().fg(theme.fg_dim)),
        Span::styled("Ctrl+F ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("favorite  ", Style::default().fg(theme.fg_dim)),
        Span::styled("Ctrl+L ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled(format!("add to {playlist_hint}  "), Style::default().fg(theme.fg_dim)),
        Span::styled("Esc ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("back", Style::default().fg(theme.fg_dim)),
    ]);

    let status_block = Paragraph::new(vec![status, Line::from(""), hints]);
    frame.render_widget(status_block, chunks[2]);
}
