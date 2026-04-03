use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::Frame;

use crate::ui::theme::Theme;

/// Info about the currently playing track.
pub struct NowPlayingInfo {
    pub title: String,
    pub artist: String,
    pub elapsed_secs: u64,
    pub total_secs: u64,
    pub is_playing: bool,
}

impl NowPlayingInfo {
    pub fn empty() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            elapsed_secs: 0,
            total_secs: 0,
            is_playing: false,
        }
    }
}

fn format_time(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m:02}:{s:02}")
}

/// Render the "Now Playing" panel with track info and progress bar.
pub fn render_now_playing(
    frame: &mut Frame,
    area: Rect,
    info: &NowPlayingInfo,
    theme: &Theme,
) {
    let block = Block::default()
        .title(" Now Playing ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_focused));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if info.title.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  No track playing",
            Style::default().fg(theme.fg_dim),
        )));
        frame.render_widget(empty, inner);
        return;
    }

    let status_icon = if info.is_playing { "▶" } else { "⏸" };
    let time_str = format!(
        "{} / {}",
        format_time(info.elapsed_secs),
        format_time(info.total_secs)
    );

    let title_line = Line::from(vec![
        Span::styled(
            format!(" {status_icon} "),
            Style::default().fg(theme.primary),
        ),
        Span::styled(
            info.title.clone(),
            Style::default()
                .fg(theme.fg)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let artist_line = Line::from(vec![
        Span::raw("   "),
        Span::styled(info.artist.clone(), Style::default().fg(theme.secondary)),
        Span::styled(
            format!("  {time_str}"),
            Style::default().fg(theme.fg_dim),
        ),
    ]);

    // Render title + artist (use first 2 lines of inner area)
    if inner.height >= 1 {
        let title_area = Rect {
            height: 1,
            ..inner
        };
        frame.render_widget(Paragraph::new(title_line), title_area);
    }

    if inner.height >= 2 {
        let artist_area = Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        };
        frame.render_widget(Paragraph::new(artist_line), artist_area);
    }

    // Progress bar on line 3
    if inner.height >= 3 {
        let ratio = if info.total_secs > 0 {
            (info.elapsed_secs as f64 / info.total_secs as f64).min(1.0)
        } else {
            0.0
        };

        let gauge = Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(theme.progress_bar)
                    .bg(theme.bg),
            )
            .ratio(ratio)
            .label("");

        let gauge_area = Rect {
            x: inner.x + 1,
            y: inner.y + 2,
            width: inner.width.saturating_sub(2),
            height: 1,
        };
        frame.render_widget(gauge, gauge_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0), "00:00");
        assert_eq!(format_time(65), "01:05");
        assert_eq!(format_time(3600), "60:00");
    }

    #[test]
    fn test_now_playing_empty() {
        let info = NowPlayingInfo::empty();
        assert!(info.title.is_empty());
        assert!(!info.is_playing);
    }
}
