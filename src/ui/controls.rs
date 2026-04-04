use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::RepeatMode;
use crate::ui::theme::Theme;

/// Render the bottom controls bar showing playback state and keybinding hints.
pub fn render_controls(
    frame: &mut Frame,
    area: Rect,
    is_playing: bool,
    shuffle: bool,
    repeat: RepeatMode,
    volume: u8,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let play_icon = if is_playing { "▶" } else { "⏸" };
    let shuffle_icon = if shuffle { "🔀" } else { "  " };
    let repeat_icon = match repeat {
        RepeatMode::None => "  ",
        RepeatMode::All => "🔁",
        RepeatMode::One => "🔂",
    };

    let vol_filled = (volume as usize * 8) / 100;
    let vol_empty = 8usize.saturating_sub(vol_filled);
    let vol_bar = format!("{}{}", "█".repeat(vol_filled), "░".repeat(vol_empty));

    let controls_line = Line::from(vec![
        Span::raw("  "),
        Span::styled("⏮ ", Style::default().fg(theme.fg_dim)),
        Span::styled(
            format!(" {play_icon} "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ⏭", Style::default().fg(theme.fg_dim)),
        Span::raw("  "),
        Span::styled(shuffle_icon, Style::default().fg(if shuffle { theme.primary } else { theme.fg_dim })),
        Span::raw(" "),
        Span::styled(repeat_icon, Style::default().fg(match repeat {
            RepeatMode::None => theme.fg_dim,
            _ => theme.primary,
        })),
        Span::raw("  Vol: "),
        Span::styled(vol_bar, Style::default().fg(theme.primary)),
        Span::raw("  "),
        Span::styled(
            "[q]uit  [?]help  [Tab]focus  [n]ext  [p]rev  [s]huffle  [r]epeat  [space]play/pause  [Shift+F]search  [Ctrl+D]download song  [Ctrl+Shift+D]download playlist",
            Style::default().fg(theme.fg_dim),
        ),
    ]);

    let paragraph = Paragraph::new(controls_line);
    frame.render_widget(paragraph, inner);
}
