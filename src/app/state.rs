use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::player::{Levels, Resolver};
use crate::queue::Queue;
use crate::ytm::{Entry, ItemRef, SearchFilter, Track};

/// How long status messages stay visible.
const STATUS_TTL: Duration = Duration::from_secs(5);

/// Top-level sections shown in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Home,
    Search,
    Library,
}

impl Section {
    pub const ALL: [Section; 3] = [Section::Home, Section::Search, Section::Library];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Section::Home => "Home",
            Section::Search => "Search",
            Section::Library => "Library",
        }
    }
}

/// What a page in the navigation stack shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageKind {
    Root(Section),
    Collection(ItemRef),
}

/// A scrollable list page with its loading state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub kind: PageKind,
    pub title: String,
    pub subtitle: String,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    pub continuation: Option<String>,
}

impl Page {
    pub fn new(kind: PageKind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            subtitle: String::new(),
            entries: Vec::new(),
            selected: 0,
            loading: false,
            loaded: false,
            error: None,
            continuation: None,
        }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    /// Replaces entries and moves the cursor to the first selectable row.
    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        self.selected = 0;
        self.loading = false;
        self.loaded = true;
        self.error = None;
        self.skip_headers(true);
    }

    /// Moves the selection by `delta` rows, skipping section headers.
    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
        self.skip_headers(delta >= 0);
    }

    pub fn select_edge(&mut self, top: bool) {
        self.selected = if top {
            0
        } else {
            self.entries.len().saturating_sub(1)
        };
        self.skip_headers(top);
    }

    fn skip_headers(&mut self, forward: bool) {
        let is_header = |e: Option<&Entry>| matches!(e, Some(Entry::Header(_)));
        if self.entries.iter().all(|e| matches!(e, Entry::Header(_))) {
            return;
        }
        while is_header(self.entries.get(self.selected)) {
            match (forward, self.selected) {
                (true, i) if i + 1 < self.entries.len() => self.selected += 1,
                (false, i) if i > 0 => self.selected -= 1,
                _ => return self.skip_headers(!forward),
            }
        }
    }

    /// Tracks in this page and the index of the selected one among them.
    pub fn tracks_from_selection(&self) -> (Vec<Track>, usize) {
        let mut start = 0;
        let mut tracks = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if let Entry::Track(t) = e {
                if i == self.selected {
                    start = tracks.len();
                }
                tracks.push(t.clone());
            }
        }
        (tracks, start)
    }

    /// True when the cursor is close enough to the end to fetch the next page.
    pub fn near_end(&self) -> bool {
        self.selected + 5 >= self.entries.len()
    }
}

/// Which part of the screen receives navigation keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Main,
    SearchInput,
    Queue,
}

/// Optional right-hand panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    None,
    Queue,
    Lyrics,
}

/// Lyrics of the playing track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LyricsState {
    Loading,
    Loaded(String),
    Missing,
    Failed(String),
}

/// Mirror of mpv's playback state.
#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub volume: u8,
    /// A track was requested but audio has not started yet.
    pub loading: bool,
    pub levels: Levels,
}

/// Whole application state; mutated only by `update`.
pub struct App {
    pub section: Section,
    stacks: [Vec<Page>; 3],
    pub focus: Focus,
    pub panel: Panel,
    pub search_input: String,
    pub search_filter: SearchFilter,
    pub queue: Queue,
    pub queue_selected: usize,
    pub player: PlayerState,
    pub lyrics: Option<(String, LyricsState)>,
    pub resolver: Resolver,
    pub resolving: HashSet<String>,
    /// Video waiting for its stream URL before playback can start.
    pub pending_play: Option<String>,
    pub radio_loading: bool,
    pub meter_enabled: bool,
    pub show_help: bool,
    pub should_quit: bool,
    status: Option<(String, Instant)>,
}

impl App {
    pub fn new(resolver: Resolver, volume: u8, meter_enabled: bool) -> Self {
        let root = |s: Section| vec![Page::new(PageKind::Root(s), s.label())];
        Self {
            section: Section::Home,
            stacks: [
                root(Section::Home),
                root(Section::Search),
                root(Section::Library),
            ],
            focus: Focus::Main,
            panel: Panel::None,
            search_input: String::new(),
            search_filter: SearchFilter::Songs,
            queue: Queue::default(),
            queue_selected: 0,
            player: PlayerState {
                volume,
                ..Default::default()
            },
            lyrics: None,
            resolver,
            resolving: HashSet::new(),
            pending_play: None,
            radio_loading: false,
            meter_enabled,
            show_help: false,
            should_quit: false,
            status: None,
        }
    }

    pub fn page(&self) -> &Page {
        self.stacks[self.section.index()]
            .last()
            .expect("stack never empty")
    }

    pub fn page_mut(&mut self) -> &mut Page {
        self.stacks[self.section.index()]
            .last_mut()
            .expect("stack never empty")
    }

    pub fn stack_depth(&self) -> usize {
        self.stacks[self.section.index()].len()
    }

    pub fn push_page(&mut self, page: Page) {
        self.stacks[self.section.index()].push(page);
    }

    /// Pops the current page; the root page always stays.
    pub fn pop_page(&mut self) -> bool {
        let stack = &mut self.stacks[self.section.index()];
        if stack.len() > 1 {
            stack.pop();
            true
        } else {
            false
        }
    }

    /// Finds the open page (in any section) matching `kind`.
    pub fn find_page_mut(&mut self, kind: &PageKind) -> Option<&mut Page> {
        self.stacks
            .iter_mut()
            .flat_map(|s| s.iter_mut())
            .find(|p| &p.kind == kind)
    }

    pub fn root_mut(&mut self, section: Section) -> &mut Page {
        &mut self.stacks[section.index()][0]
    }

    pub fn now_playing(&self) -> Option<&Track> {
        self.queue.current()
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        tracing::info!("status: {msg}");
        self.status = Some((msg, Instant::now()));
    }

    pub fn status(&self) -> Option<&str> {
        self.status
            .as_ref()
            .filter(|(_, at)| at.elapsed() < STATUS_TTL)
            .map(|(m, _)| m.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> Entry {
        Entry::Track(Track {
            video_id: id.into(),
            title: id.into(),
            artists: String::new(),
            album: None,
            duration: None,
            liked: None,
        })
    }

    #[test]
    fn test_selection_skips_headers_both_ways() {
        let mut p = Page::new(PageKind::Root(Section::Home), "Home");
        p.set_entries(vec![
            Entry::Header("A".into()),
            track("1"),
            Entry::Header("B".into()),
            track("2"),
        ]);
        assert_eq!(p.selected, 1, "initial selection skips leading header");
        p.move_selection(1);
        assert_eq!(p.selected, 3);
        p.move_selection(-1);
        assert_eq!(p.selected, 1);
        p.move_selection(-1);
        assert_eq!(p.selected, 1, "cannot land on the top header");
    }

    #[test]
    fn test_tracks_from_selection_ignores_non_tracks() {
        let mut p = Page::new(PageKind::Root(Section::Home), "Home");
        p.set_entries(vec![
            Entry::Header("H".into()),
            track("a"),
            track("b"),
            track("c"),
        ]);
        p.move_selection(2);
        let (tracks, start) = p.tracks_from_selection();
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[start].video_id, "c");
    }

    #[test]
    fn test_stack_keeps_root() {
        let mut app = App::new(Resolver::new("yt-dlp".into(), "/tmp/c".into()), 50, true);
        assert!(!app.pop_page());
        app.push_page(Page::new(PageKind::Root(Section::Home), "Extra"));
        assert_eq!(app.stack_depth(), 2);
        assert!(app.pop_page());
        assert_eq!(app.page().title, "Home");
    }
}
