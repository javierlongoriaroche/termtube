use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Main layout areas for the TUI.
pub struct AppLayout {
    pub header: Rect,
    pub sidebar: Rect,
    pub main_panel: Rect,
    pub controls: Rect,
}

/// Right panel sub-areas.
pub struct MainPanelLayout {
    pub now_playing: Rect,
    pub visualizer: Rect,
}

impl AppLayout {
    /// Compute the main layout from a given terminal area.
    ///
    /// ```text
    /// ┌─────────────────────────────────────────┐
    /// │  Header (1 line)                        │
    /// ├─────────────┬───────────────────────────┤
    /// │  Sidebar    │  Main panel               │
    /// │  (30%)      │  (70%)                    │
    /// │             │                           │
    /// ├─────────────┴───────────────────────────┤
    /// │  Controls (3 lines)                     │
    /// └─────────────────────────────────────────┘
    /// ```
    pub fn new(area: Rect) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // header
                Constraint::Min(8),    // body
                Constraint::Length(3), // controls
            ])
            .split(area);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // sidebar
                Constraint::Percentage(70), // main panel
            ])
            .split(vertical[1]);

        Self {
            header: vertical[0],
            sidebar: body[0],
            main_panel: body[1],
            controls: vertical[2],
        }
    }
}

impl MainPanelLayout {
    /// Split the main panel into now-playing info and visualizer area.
    pub fn new(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5), // now playing
                Constraint::Min(4),   // visualizer
            ])
            .split(area);

        Self {
            now_playing: chunks[0],
            visualizer: chunks[1],
        }
    }
}
