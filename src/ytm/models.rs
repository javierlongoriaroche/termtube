/// A playable song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub video_id: String,
    pub title: String,
    pub artists: String,
    pub album: Option<String>,
    /// Duration in seconds, when known.
    pub duration: Option<u32>,
    /// Like status as reported by YouTube Music (`None` when unknown).
    pub liked: Option<bool>,
}

impl Track {
    pub fn watch_url(&self) -> String {
        format!("https://music.youtube.com/watch?v={}", self.video_id)
    }
}

/// A browsable collection or artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemRef {
    Album {
        id: String,
        title: String,
        subtitle: String,
    },
    Playlist {
        id: String,
        title: String,
        subtitle: String,
    },
    Artist {
        id: String,
        name: String,
        subtitle: String,
    },
}

impl ItemRef {
    pub fn title(&self) -> &str {
        match self {
            ItemRef::Album { title, .. } | ItemRef::Playlist { title, .. } => title,
            ItemRef::Artist { name, .. } => name,
        }
    }

    pub fn subtitle(&self) -> &str {
        match self {
            ItemRef::Album { subtitle, .. }
            | ItemRef::Playlist { subtitle, .. }
            | ItemRef::Artist { subtitle, .. } => subtitle,
        }
    }

    pub fn kind_label(&self) -> &'static str {
        match self {
            ItemRef::Album { .. } => "Album",
            ItemRef::Playlist { .. } => "Playlist",
            ItemRef::Artist { .. } => "Artist",
        }
    }
}

/// One row in any list view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Track(Track),
    Item(ItemRef),
    /// Non-selectable section header (e.g. a home shelf title).
    Header(String),
}

/// A titled group of entries (home shelf, artist section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shelf {
    pub title: String,
    pub entries: Vec<Entry>,
}

/// Flattens shelves into a single list with a header row per shelf.
pub fn flatten_shelves(shelves: &[Shelf]) -> Vec<Entry> {
    shelves
        .iter()
        .filter(|s| !s.entries.is_empty())
        .flat_map(|s| {
            std::iter::once(Entry::Header(s.title.clone())).chain(s.entries.iter().cloned())
        })
        .collect()
}

/// A Google account available in the browser session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub byline: String,
}

/// Parses "m:ss" or "h:mm:ss" into seconds.
pub fn parse_duration(text: &str) -> Option<u32> {
    let mut total = 0u32;
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    for p in parts {
        total = total.checked_mul(60)?.checked_add(p.parse().ok()?)?;
    }
    Some(total)
}

/// Formats seconds as "m:ss" (or "h:mm:ss" for long durations).
pub fn format_duration(secs: u32) -> String {
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_formats() {
        assert_eq!(parse_duration("3:57"), Some(237));
        assert_eq!(parse_duration("1:02:03"), Some(3723));
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("42"), None);
    }

    #[test]
    fn test_format_duration_roundtrip() {
        assert_eq!(format_duration(237), "3:57");
        assert_eq!(format_duration(3723), "1:02:03");
        assert_eq!(format_duration(5), "0:05");
    }

    #[test]
    fn test_flatten_shelves_adds_headers_and_skips_empty() {
        let t = Track {
            video_id: "vid1".into(),
            title: "Song".into(),
            artists: "Artist".into(),
            album: None,
            duration: None,
            liked: None,
        };
        let shelves = vec![
            Shelf {
                title: "Empty".into(),
                entries: vec![],
            },
            Shelf {
                title: "Picks".into(),
                entries: vec![Entry::Track(t.clone())],
            },
        ];
        assert_eq!(
            flatten_shelves(&shelves),
            vec![Entry::Header("Picks".into()), Entry::Track(t)]
        );
    }
}
