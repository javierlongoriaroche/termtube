pub mod main_view;
pub mod player_bar;
pub mod theme;

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::keymap::HELP;
use crate::app::state::{App, Panel, Section};
use theme::Theme;

const SIDEBAR_WIDTH: u16 = 16;
const PLAYER_HEIGHT: u16 = 4;

/// Draws the whole screen.
pub fn draw(frame: &mut Frame, app: &App, theme: &Theme) {
    let [body, player, status] = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(PLAYER_HEIGHT),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [sidebar, content] =
        Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(20)]).areas(body);

    draw_sidebar(frame, sidebar, app, theme);
    if app.panel == Panel::None {
        main_view::draw(frame, content, app, theme);
    } else {
        let [main, side] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                .areas(content);
        main_view::draw(frame, main, app, theme);
        player_bar::draw_panel(frame, side, app, theme);
    }
    player_bar::draw(frame, player, app, theme);
    draw_status(frame, status, app, theme);
    if app.show_help {
        draw_help(frame, theme);
    }
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut lines = vec![
        Line::from(" TermTube".bold().fg(theme.primary)),
        Line::default(),
    ];
    for (i, section) in Section::ALL.iter().enumerate() {
        let selected = *section == app.section;
        let marker = if selected { "▌" } else { " " };
        let style = if selected {
            Style::new().fg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme.fg)
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::new().fg(theme.primary)),
            Span::styled(format!("{} {}", i + 1, section.label()), style),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(" ? help".fg(theme.fg_dim)));
    let block = Block::bordered().border_style(Style::new().fg(theme.border));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let line = match app.status() {
        Some(msg) => Line::from(format!(" {msg}").fg(theme.secondary)),
        None => Line::from(" Space play/pause · n/p next/prev · / search · Q queue · l lyrics · f like · ? help · q quit".fg(theme.fg_dim)),
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_help(frame: &mut Frame, theme: &Theme) {
    let height = HELP.len() as u16 + 2;
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(frame.area());
    let [area] = Layout::horizontal([Constraint::Length(58)])
        .flex(Flex::Center)
        .areas(area);
    let rows = HELP.iter().map(|(k, d)| {
        Row::new([
            Span::from(*k).fg(theme.primary).bold(),
            Span::from(*d).fg(theme.fg),
        ])
    });
    let table = Table::new(rows, [Constraint::Length(16), Constraint::Min(10)]).block(
        Block::bordered()
            .title(" Keys ")
            .border_style(Style::new().fg(theme.primary)),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(table, area);
}
