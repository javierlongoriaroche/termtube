use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::state::{App, Focus, Page, PageKind, Section};
use crate::ytm::client::SearchFilter;
use crate::ytm::models::format_duration;
use crate::ytm::{Entry, Track};

use super::theme::Theme;

/// Draws the current page (with the search box on the Search section).
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let page = app.page();
    let area = if page.kind == PageKind::Root(Section::Search) {
        let [search, rest] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(area);
        draw_search_box(frame, search, app, theme);
        rest
    } else {
        area
    };

    let focused = app.focus == Focus::Main;
    let mut title = vec![Span::from(format!(" {} ", page.title))
        .bold()
        .fg(theme.primary)];
    if !page.subtitle.is_empty() {
        title.push(Span::from(format!("· {} ", page.subtitle)).fg(theme.fg_dim));
    }
    if app.stack_depth() > 1 {
        title.push(Span::from("· Esc back ").fg(theme.fg_dim));
    }
    let block = Block::bordered()
        .title(Line::from(title))
        .border_style(Style::new().fg(if focused {
            theme.border_focused
        } else {
            theme.border
        }));

    if page.entries.is_empty() {
        frame.render_widget(
            Paragraph::new(empty_message(page, app).fg(theme.fg_dim)).block(block),
            area,
        );
        return;
    }

    let playing = app.now_playing().map(|t| t.video_id.as_str());
    let width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = page
        .entries
        .iter()
        .map(|e| entry_item(e, app, playing, width, theme))
        .collect();
    let mut state = ListState::default().with_selected(Some(page.selected));
    let footer = if page.loading { " loading… " } else { "" };
    let list = List::new(items)
        .block(block.title_bottom(Line::from(footer.fg(theme.fg_dim))))
        .highlight_style(
            Style::new()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, area, &mut state);
}

fn empty_message(page: &Page, app: &App) -> String {
    if page.loading {
        " Loading…".into()
    } else if let Some(err) = &page.error {
        format!(" {err}")
    } else if page.kind == PageKind::Root(Section::Search) && app.search_input.is_empty() {
        " Type to search YouTube Music, Enter to search, Tab to change type".into()
    } else {
        " Nothing here".into()
    }
}

fn draw_search_box(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let editing = app.focus == Focus::SearchInput;
    let mut spans = vec![
        Span::from(" › "),
        Span::from(app.search_input.clone()).fg(theme.fg),
    ];
    if editing {
        spans.push(Span::from("▏").fg(theme.primary));
    }
    let filters: Vec<Span> = SearchFilter::ALL
        .iter()
        .flat_map(|f| {
            let style = if *f == app.search_filter {
                Style::new().fg(theme.primary).bold()
            } else {
                Style::new().fg(theme.fg_dim)
            };
            [Span::styled(f.label(), style), Span::from(" ")]
        })
        .collect();
    let block = Block::bordered()
        .title(Line::from(filters).right_aligned())
        .border_style(Style::new().fg(if editing {
            theme.border_focused
        } else {
            theme.border
        }));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn entry_item<'a>(
    entry: &'a Entry,
    app: &App,
    playing: Option<&str>,
    width: usize,
    theme: &Theme,
) -> ListItem<'a> {
    match entry {
        Entry::Header(title) => ListItem::new(Line::from(vec![Span::from(format!("── {title} "))
            .bold()
            .fg(theme.secondary)])),
        Entry::Track(t) => ListItem::new(track_line(
            t,
            playing == Some(t.video_id.as_str()),
            app.is_liked(&t.video_id),
            width,
            theme,
        )),
        Entry::Item(item) => ListItem::new(Line::from(vec![
            Span::from(format!("{:<9}", item.kind_label())).fg(theme.secondary),
            Span::from(item.title().to_string()).fg(theme.fg),
            Span::from(format!("  {}", item.subtitle())).fg(theme.fg_dim),
        ])),
    }
}

/// One song row: playing marker, like marker, title, artists/album and duration.
pub fn track_line<'a>(
    t: &Track,
    is_playing: bool,
    liked: bool,
    width: usize,
    theme: &Theme,
) -> Line<'a> {
    let icon = if is_playing { "▶" } else { " " };
    let heart = if liked { "♥ " } else { "  " };
    let mut meta = t.artists.clone();
    if let Some(album) = &t.album {
        meta = format!("{meta} · {album}");
    }
    let duration = t.duration.map(format_duration).unwrap_or_default();
    let used = 3 + t.title.chars().count() + 3 + duration.len();
    let meta = truncate(&meta, width.saturating_sub(used));
    let pad = width.saturating_sub(used + meta.chars().count());
    let title_style = if is_playing {
        Style::new().fg(theme.primary).bold()
    } else {
        Style::new().fg(theme.fg)
    };
    Line::from(vec![
        Span::from(icon).fg(theme.primary),
        Span::from(heart).fg(theme.liked),
        Span::styled(t.title.clone(), title_style),
        Span::from("   "),
        Span::from(meta).fg(theme.fg_dim),
        Span::from(" ".repeat(pad)),
        Span::from(duration).fg(theme.fg_dim),
    ])
}

/// Truncates to `max` characters, adding an ellipsis when cut.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::Resolver;
    use crate::ytm::ItemRef;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 6), "hello…");
        assert_eq!(truncate("abc", 0), "");
    }

    #[test]
    fn test_renders_entries_and_selection() {
        let mut app = App::new(Resolver::new("yt-dlp".into(), "/tmp/c".into()), 50, true);
        let t = Track {
            video_id: "v".into(),
            title: "Sample Song".into(),
            artists: "Sample Band".into(),
            album: None,
            duration: Some(185),
            liked: Some(true),
        };
        app.root_mut(Section::Home).set_entries(vec![
            Entry::Header("Quick picks".into()),
            Entry::Track(t),
            Entry::Item(ItemRef::Album {
                id: "a".into(),
                title: "Some Record".into(),
                subtitle: "2001".into(),
            }),
        ]);
        app.liked_ids.insert("v".into());
        let mut term = Terminal::new(TestBackend::new(80, 12)).unwrap();
        term.draw(|f| super::super::draw(f, &app, &Theme::default()))
            .unwrap();
        let screen = format!("{:?}", term.backend().buffer());
        for needle in [
            "Quick picks",
            "Sample Song",
            "Sample Band",
            "3:05",
            "Some Record",
            "1 Home",
            "Nothing playing",
            "♥ Sample Song",
        ] {
            assert!(screen.contains(needle), "missing {needle}");
        }
    }
}
