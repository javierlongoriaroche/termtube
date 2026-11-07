use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, LineGauge, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::state::{App, Focus, LyricsState, Panel};
use crate::queue::Repeat;
use crate::ytm::models::format_duration;

use super::main_view::{track_line, truncate};
use super::theme::Theme;

const METER_WIDTH: u16 = 22;

/// Draws the bottom player: now playing, controls state, progress and meter.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = Block::bordered().border_style(Style::new().fg(theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let meter_width = if app.meter_enabled { METER_WIDTH } else { 0 };
    let [info, meter] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(meter_width)]).areas(inner);
    let [line1, line2] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(info);

    let Some(track) = app.now_playing() else {
        frame.render_widget(
            Paragraph::new(" Nothing playing — pick a song and press Enter".fg(theme.fg_dim)),
            line1,
        );
        return;
    };

    let p = &app.player;
    let state = if p.loading {
        "…"
    } else if p.paused {
        "⏸"
    } else {
        "▶"
    };
    let like = if app.is_liked(&track.video_id) {
        " ♥"
    } else {
        ""
    };
    let modes = format!(
        "{}{} vol {}% ",
        if app.queue.is_shuffled() { "⤮ " } else { "" },
        match app.queue.repeat() {
            Repeat::Off => "",
            Repeat::All => "🔁 ",
            Repeat::One => "🔂 ",
        },
        p.volume
    );
    let title_width = (line1.width as usize).saturating_sub(modes.chars().count() + 6);
    let title = truncate(&format!("{} — {}", track.title, track.artists), title_width);
    let pad = (line1.width as usize)
        .saturating_sub(3 + title.chars().count() + like.chars().count() + modes.chars().count());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::from(format!(" {state} ")).fg(theme.primary).bold(),
            Span::from(title).fg(theme.fg).add_modifier(Modifier::BOLD),
            Span::from(like).fg(theme.liked),
            Span::from(" ".repeat(pad)),
            Span::from(modes).fg(theme.fg_dim),
        ])),
        line1,
    );

    let ratio = if p.duration > 0.0 {
        (p.position / p.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let label = if p.loading {
        "loading…".to_string()
    } else {
        format!(
            "{} / {}",
            format_duration(p.position as u32),
            format_duration(p.duration as u32)
        )
    };
    frame.render_widget(
        LineGauge::default()
            .ratio(ratio)
            .label(Span::from(format!(" {label} ")).fg(theme.fg_dim))
            .filled_style(Style::new().fg(theme.progress_bar))
            .unfilled_style(Style::new().fg(theme.border)),
        line2,
    );

    if app.meter_enabled {
        draw_meter(frame, meter, app, theme);
    }
}

/// Two horizontal bars (L/R) for RMS level with a peak marker.
fn draw_meter(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let l = &app.player.levels;
    let width = area.width.saturating_sub(3) as usize;
    let bar = |rms: f64, peak: f64, label: &str| -> Line<'static> {
        let filled = (rms * width as f64).round() as usize;
        let peak_at = ((peak * width as f64).round() as usize).min(width.saturating_sub(1));
        let mut spans = vec![Span::from(format!(" {label}")).fg(theme.fg_dim)];
        for i in 0..width {
            let color = meter_color(&theme.meter_colors, i as f64 / width as f64);
            let (ch, style) = if i < filled {
                ("▮", Style::new().fg(color))
            } else if i == peak_at && peak > 0.0 {
                ("▏", Style::new().fg(color))
            } else {
                ("·", Style::new().fg(theme.border))
            };
            spans.push(Span::styled(ch, style));
        }
        Line::from(spans)
    };
    frame.render_widget(
        Paragraph::new(vec![
            bar(l.left_rms, l.left_peak, "L "),
            bar(l.right_rms, l.right_peak, "R "),
        ]),
        area,
    );
}

fn meter_color(colors: &[Color], position: f64) -> Color {
    match colors.len() {
        0 => Color::Green,
        n => colors[((position * n as f64) as usize).min(n - 1)],
    }
}

/// Draws the right-hand panel (queue or lyrics).
pub fn draw_panel(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    match app.panel {
        Panel::Queue => draw_queue(frame, area, app, theme),
        Panel::Lyrics => draw_lyrics(frame, area, app, theme),
        Panel::None => {}
    }
}

fn draw_queue(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let focused = app.focus == Focus::Queue;
    let tracks = app.queue.tracks();
    let current = app.queue.current_index();
    let width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            ListItem::new(track_line(
                t,
                Some(i) == current,
                app.is_liked(&t.video_id),
                width,
                theme,
            ))
        })
        .collect();
    let hint = if focused {
        " K/J move · d remove · Enter play "
    } else {
        " Tab to focus "
    };
    let block = Block::bordered()
        .title(Line::from(
            format!(" Queue ({}) ", tracks.len())
                .bold()
                .fg(theme.primary),
        ))
        .title_bottom(Line::from(hint.fg(theme.fg_dim)))
        .border_style(Style::new().fg(if focused {
            theme.border_focused
        } else {
            theme.border
        }));
    let mut state = ListState::default().with_selected(focused.then_some(app.queue_selected));
    if !focused {
        *state.offset_mut() = current.unwrap_or(0).saturating_sub(2);
    }
    let list = List::new(items).block(block).highlight_style(
        Style::new()
            .bg(theme.highlight_bg)
            .fg(theme.highlight_fg)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_lyrics(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let text = match &app.lyrics {
        None => "Play a song to see its lyrics".to_string(),
        Some((_, LyricsState::Loading)) => "Loading lyrics…".to_string(),
        Some((_, LyricsState::Missing)) => "No lyrics available for this song".to_string(),
        Some((_, LyricsState::Failed(e))) => format!("Could not load lyrics: {e}"),
        Some((_, LyricsState::Loaded(text))) => text.clone(),
    };
    let block = Block::bordered()
        .title(Line::from(" Lyrics ".bold().fg(theme.primary)))
        .border_style(Style::new().fg(theme.border));
    frame.render_widget(
        Paragraph::new(text)
            .fg(theme.fg)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}
