//! InnerTube requests not covered by ytmapi-rs, and their JSON parsers.

use std::borrow::Cow;

use serde_json::{Map, Value};
use ytmapi_rs::auth::BrowserToken;
use ytmapi_rs::parse::ParseFrom;
use ytmapi_rs::query::{PostMethod, PostQuery, Query};
use ytmapi_rs::ProcessedResult;

use super::models::{parse_duration, Account, Entry, ItemRef, Shelf, Track};

/// POST to an arbitrary InnerTube path; the response is read with `json_query`.
pub struct RawPost {
    path: &'static str,
    body: Map<String, Value>,
    params: Vec<(&'static str, String)>,
}

impl RawPost {
    /// Home feed, first page.
    pub fn home() -> Self {
        let mut body = Map::new();
        body.insert("browseId".into(), "FEmusic_home".into());
        Self {
            path: "browse",
            body,
            params: vec![],
        }
    }

    /// Next page of a browse response.
    pub fn continuation(token: &str) -> Self {
        Self {
            path: "browse",
            body: Map::new(),
            params: vec![
                ("ctoken", token.to_string()),
                ("continuation", token.to_string()),
                ("type", "next".to_string()),
            ],
        }
    }

    /// Accounts available for the current session cookie.
    pub fn accounts() -> Self {
        Self {
            path: "account/accounts_list",
            body: Map::new(),
            params: vec![],
        }
    }

    /// Any browse page by id (e.g. `VL<playlistId>`).
    pub fn browse(browse_id: &str) -> Self {
        let mut body = Map::new();
        body.insert("browseId".into(), browse_id.into());
        Self {
            path: "browse",
            body,
            params: vec![],
        }
    }

    /// Next page of a list that uses `continuationItemRenderer` tokens.
    pub fn browse_continuation(token: &str) -> Self {
        let mut body = Map::new();
        body.insert("continuation".into(), token.into());
        Self {
            path: "browse",
            body,
            params: vec![],
        }
    }
}

/// Unused typed output; raw requests are parsed from JSON by the functions below.
#[derive(Debug)]
pub struct RawOutput;

impl ParseFrom<RawPost> for RawOutput {
    fn parse_from(_: ProcessedResult<RawPost>) -> ytmapi_rs::Result<Self> {
        Ok(RawOutput)
    }
}

impl PostQuery for RawPost {
    fn header(&self) -> Map<String, Value> {
        self.body.clone()
    }
    fn params(&self) -> Vec<(&str, Cow<'_, str>)> {
        self.params
            .iter()
            .map(|(k, v)| (*k, Cow::Borrowed(v.as_str())))
            .collect()
    }
    fn path(&self) -> &str {
        self.path
    }
}

impl Query<BrowserToken> for RawPost {
    type Output = RawOutput;
    type Method = PostMethod;
}

/// Shelves of a home page plus the continuation token for the next page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HomePage {
    pub shelves: Vec<Shelf>,
    pub continuation: Option<String>,
}

/// Parses the first home page or a continuation response.
pub fn parse_home(json: &Value) -> HomePage {
    let section = json
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer")
        .or_else(|| json.pointer("/continuationContents/sectionListContinuation"));
    let Some(section) = section else {
        return HomePage::default();
    };
    let shelves = section
        .get("contents")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_shelf).collect())
        .unwrap_or_default();
    let continuation = section
        .pointer("/continuations/0/nextContinuationData/continuation")
        .and_then(Value::as_str)
        .map(str::to_string);
    HomePage {
        shelves,
        continuation,
    }
}

fn parse_shelf(section: &Value) -> Option<Shelf> {
    let shelf = section.get("musicCarouselShelfRenderer")?;
    let title = runs_text(shelf.pointer("/header/musicCarouselShelfBasicHeaderRenderer/title"));
    let entries = shelf
        .get("contents")?
        .as_array()?
        .iter()
        .filter_map(|item| {
            item.get("musicResponsiveListItemRenderer")
                .and_then(parse_responsive_item)
                .or_else(|| {
                    item.get("musicTwoRowItemRenderer")
                        .and_then(parse_two_row_item)
                })
        })
        .collect();
    Some(Shelf { title, entries })
}

/// List-style row: usually a song with title and artist columns.
fn parse_responsive_item(item: &Value) -> Option<Entry> {
    let column = |i: usize| {
        item.pointer(&format!(
            "/flexColumns/{i}/musicResponsiveListItemFlexColumnRenderer/text"
        ))
    };
    let title = runs_text(column(0));
    let video_id = item
        .pointer("/playlistItemData/videoId")
        .or_else(|| column(0)?.pointer("/runs/0/navigationEndpoint/watchEndpoint/videoId"))
        .and_then(Value::as_str)?;
    let subtitle = runs_text(column(1));
    let (artists, album) = split_song_subtitle(&subtitle);
    Some(Entry::Track(Track {
        video_id: video_id.to_string(),
        title,
        artists,
        album,
        duration: None,
        liked: None,
    }))
}

/// Card-style item: album, playlist, artist or a single song.
fn parse_two_row_item(item: &Value) -> Option<Entry> {
    let title = runs_text(item.get("title"));
    let subtitle = runs_text(item.get("subtitle"));
    if let Some(video_id) = item
        .pointer("/navigationEndpoint/watchEndpoint/videoId")
        .and_then(Value::as_str)
    {
        let (artists, album) = split_song_subtitle(&subtitle);
        return Some(Entry::Track(Track {
            video_id: video_id.to_string(),
            title,
            artists,
            album,
            duration: None,
            liked: None,
        }));
    }
    let browse = item.pointer("/navigationEndpoint/browseEndpoint")?;
    let id = browse.get("browseId")?.as_str()?.to_string();
    let page_type = browse
        .pointer("/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let item = match page_type {
        "MUSIC_PAGE_TYPE_ALBUM" | "MUSIC_PAGE_TYPE_AUDIOBOOK" => ItemRef::Album {
            id,
            title,
            subtitle,
        },
        "MUSIC_PAGE_TYPE_PLAYLIST" => ItemRef::Playlist {
            id: strip_vl(&id),
            title,
            subtitle,
        },
        "MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL" => ItemRef::Artist {
            id,
            name: title,
            subtitle,
        },
        _ => return None,
    };
    Some(Entry::Item(item))
}

/// Tracks of one playlist page plus the token for the next page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaylistPage {
    pub tracks: Vec<Track>,
    pub continuation: Option<String>,
}

/// Parses a playlist page (first page or continuation), skipping unplayable rows.
pub fn parse_playlist_page(json: &Value) -> PlaylistPage {
    let items = json
        .pointer("/contents/twoColumnBrowseResultsRenderer/secondaryContents/sectionListRenderer/contents/0/musicPlaylistShelfRenderer/contents")
        .or_else(|| json.pointer("/onResponseReceivedActions/0/appendContinuationItemsAction/continuationItems"))
        .and_then(Value::as_array);
    let Some(items) = items else {
        return PlaylistPage::default();
    };
    let tracks = items
        .iter()
        .filter_map(|i| i.get("musicResponsiveListItemRenderer"))
        .filter_map(parse_playlist_row)
        .collect();
    let continuation = items
        .iter()
        .find_map(|i| {
            i.pointer("/continuationItemRenderer/continuationEndpoint/continuationCommand/token")
        })
        .and_then(Value::as_str)
        .map(str::to_string);
    PlaylistPage {
        tracks,
        continuation,
    }
}

fn parse_playlist_row(row: &Value) -> Option<Track> {
    let video_id = row.pointer("/playlistItemData/videoId")?.as_str()?;
    let column = |i: usize| {
        row.pointer(&format!(
            "/flexColumns/{i}/musicResponsiveListItemFlexColumnRenderer/text"
        ))
    };
    let album = runs_text(column(2));
    let duration = row
        .pointer("/fixedColumns/0/musicResponsiveListItemFixedColumnRenderer/text")
        .map(|t| text_of(Some(t)))
        .and_then(|d| parse_duration(&d));
    Some(Track {
        video_id: video_id.to_string(),
        title: runs_text(column(0)),
        artists: runs_text(column(1)),
        album: (!album.is_empty()).then_some(album),
        duration,
        liked: None,
    })
}

/// Parses the accounts list, in session order (index = `X-Goog-AuthUser`).
pub fn parse_accounts(json: &Value) -> Vec<Account> {
    let mut out = Vec::new();
    collect_accounts(json, &mut out);
    out
}

fn collect_accounts(v: &Value, out: &mut Vec<Account>) {
    match v {
        Value::Object(map) => {
            if let Some(item) = map.get("accountItem") {
                out.push(Account {
                    name: text_of(item.get("accountName")),
                    byline: text_of(item.get("accountByline")),
                });
            }
            map.values().for_each(|child| collect_accounts(child, out));
        }
        Value::Array(items) => items.iter().for_each(|child| collect_accounts(child, out)),
        _ => {}
    }
}

/// Joins `runs[].text` of an InnerTube text object.
fn runs_text(v: Option<&Value>) -> String {
    v.and_then(|v| v.get("runs"))
        .and_then(Value::as_array)
        .map(|runs| {
            runs.iter()
                .filter_map(|r| r.get("text").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}

/// Reads either `simpleText` or `runs` from an InnerTube text object.
fn text_of(v: Option<&Value>) -> String {
    v.and_then(|v| v.get("simpleText"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| runs_text(v))
}

/// Splits "Artist • Album • 3:45" style subtitles into artists and album.
fn split_song_subtitle(subtitle: &str) -> (String, Option<String>) {
    let parts: Vec<&str> = subtitle
        .split(" • ")
        .map(str::trim)
        .filter(|p| !p.is_empty() && parse_duration(p).is_none())
        .collect();
    match parts.as_slice() {
        [] => (String::new(), None),
        [artists] => (artists.to_string(), None),
        [kind, artists, ..] if matches!(*kind, "Song" | "Canción" | "Video" | "Vídeo") => {
            (artists.to_string(), None)
        }
        [artists, album, ..] => (artists.to_string(), Some(album.to_string())),
    }
}

fn strip_vl(id: &str) -> String {
    id.strip_prefix("VL").unwrap_or(id).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page_type(t: &str) -> Value {
        json!({"browseEndpointContextMusicConfig": {"pageType": t}})
    }

    fn sample_home() -> Value {
        json!({"contents": {"singleColumnBrowseResultsRenderer": {"tabs": [{"tabRenderer": {"content": {"sectionListRenderer": {
            "contents": [
                {"musicCarouselShelfRenderer": {
                    "header": {"musicCarouselShelfBasicHeaderRenderer": {"title": {"runs": [{"text": "Quick picks"}]}}},
                    "contents": [{"musicResponsiveListItemRenderer": {
                        "playlistItemData": {"videoId": "vidA"},
                        "flexColumns": [
                            {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Sample Song"}]}}},
                            {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [
                                {"text": "Sample Artist"}, {"text": " • "}, {"text": "Sample Album"}]}}}
                        ]}}]
                }},
                {"musicCarouselShelfRenderer": {
                    "header": {"musicCarouselShelfBasicHeaderRenderer": {"title": {"runs": [{"text": "Mixed"}]}}},
                    "contents": [
                        {"musicTwoRowItemRenderer": {"title": {"runs": [{"text": "Road Trip"}]}, "subtitle": {"runs": [{"text": "Playlist"}]},
                            "navigationEndpoint": {"browseEndpoint": {"browseId": "VLPLexample", "browseEndpointContextSupportedConfigs": page_type("MUSIC_PAGE_TYPE_PLAYLIST")}}}},
                        {"musicTwoRowItemRenderer": {"title": {"runs": [{"text": "First Album"}]}, "subtitle": {"runs": [{"text": "Album • Band"}]},
                            "navigationEndpoint": {"browseEndpoint": {"browseId": "MPREbexample", "browseEndpointContextSupportedConfigs": page_type("MUSIC_PAGE_TYPE_ALBUM")}}}},
                        {"musicTwoRowItemRenderer": {"title": {"runs": [{"text": "Band"}]}, "subtitle": {"runs": [{"text": "1M subscribers"}]},
                            "navigationEndpoint": {"browseEndpoint": {"browseId": "UCexample", "browseEndpointContextSupportedConfigs": page_type("MUSIC_PAGE_TYPE_ARTIST")}}}},
                        {"musicTwoRowItemRenderer": {"title": {"runs": [{"text": "Single"}]}, "subtitle": {"runs": [{"text": "Song • Band"}]},
                            "navigationEndpoint": {"watchEndpoint": {"videoId": "vidB"}}}},
                        {"musicTwoRowItemRenderer": {"title": {"runs": [{"text": "Unknown"}]},
                            "navigationEndpoint": {"browseEndpoint": {"browseId": "X", "browseEndpointContextSupportedConfigs": page_type("MUSIC_PAGE_TYPE_OTHER")}}}}
                    ]
                }},
                {"musicTastebuilderShelfRenderer": {}}
            ],
            "continuations": [{"nextContinuationData": {"continuation": "TOKEN1"}}]
        }}}}]}}})
    }

    #[test]
    fn test_parse_home_shelves_items_and_continuation() {
        let page = parse_home(&sample_home());
        assert_eq!(page.continuation.as_deref(), Some("TOKEN1"));
        assert_eq!(page.shelves.len(), 2);
        assert_eq!(page.shelves[0].title, "Quick picks");
        assert_eq!(
            page.shelves[0].entries,
            vec![Entry::Track(Track {
                video_id: "vidA".into(),
                title: "Sample Song".into(),
                artists: "Sample Artist".into(),
                album: Some("Sample Album".into()),
                duration: None,
                liked: None,
            })]
        );
        let mixed = &page.shelves[1].entries;
        assert_eq!(mixed.len(), 4, "unknown page types are skipped");
        assert_eq!(
            mixed[0],
            Entry::Item(ItemRef::Playlist {
                id: "PLexample".into(),
                title: "Road Trip".into(),
                subtitle: "Playlist".into()
            })
        );
        assert!(
            matches!(&mixed[1], Entry::Item(ItemRef::Album { id, .. }) if id == "MPREbexample")
        );
        assert!(matches!(&mixed[2], Entry::Item(ItemRef::Artist { id, .. }) if id == "UCexample"));
        assert!(
            matches!(&mixed[3], Entry::Track(t) if t.video_id == "vidB" && t.artists == "Band")
        );
    }

    #[test]
    fn test_parse_home_continuation_response() {
        let json = json!({"continuationContents": {"sectionListContinuation": {"contents": [
            {"musicCarouselShelfRenderer": {"header": {"musicCarouselShelfBasicHeaderRenderer": {"title": {"runs": [{"text": "More"}]}}}, "contents": []}}
        ]}}});
        let page = parse_home(&json);
        assert_eq!(page.shelves.len(), 1);
        assert_eq!(page.continuation, None);
    }

    #[test]
    fn test_parse_home_unexpected_json_is_empty() {
        assert_eq!(parse_home(&json!({"foo": 1})), HomePage::default());
    }

    #[test]
    fn test_parse_accounts_in_order() {
        let json = json!({"actions": [{"getMultiPageMenuAction": {"menu": {"multiPageMenuRenderer": {"sections": [{"accountSectionListRenderer": {"contents": [
            {"accountItemSectionRenderer": {"contents": [
                {"accountItem": {"accountName": {"simpleText": "Work Person"}, "accountByline": {"simpleText": "No channel"}}},
                {"accountItem": {"accountName": {"runs": [{"text": "Home Person"}]}, "accountByline": {"runs": [{"text": "@home"}]}}}
            ]}}
        ]}}]}}}}]});
        let accounts = parse_accounts(&json);
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].name, "Work Person");
        assert_eq!(
            accounts[1],
            Account {
                name: "Home Person".into(),
                byline: "@home".into()
            }
        );
    }

    fn playlist_row(id: Option<&str>, title: &str, artist: &str) -> Value {
        let mut row = json!({"musicResponsiveListItemRenderer": {
            "flexColumns": [
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": title}]}}},
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": artist}]}}},
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {}}}
            ],
            "fixedColumns": [{"musicResponsiveListItemFixedColumnRenderer": {"text": {"runs": [{"text": "2:05"}]}}}]
        }});
        if let Some(id) = id {
            row["musicResponsiveListItemRenderer"]["playlistItemData"] = json!({"videoId": id});
        }
        row
    }

    #[test]
    fn test_parse_playlist_first_page_with_unlinked_artist_and_unavailable_row() {
        let json = json!({"contents": {"twoColumnBrowseResultsRenderer": {"secondaryContents": {"sectionListRenderer": {"contents": [
            {"musicPlaylistShelfRenderer": {"contents": [
                playlist_row(Some("v1"), "Linked Song", "Linked Band"),
                playlist_row(None, "Removed Song", "Gone"),
                playlist_row(Some("v2"), "Plain Song", "Unlinked Artist"),
                {"continuationItemRenderer": {"continuationEndpoint": {"continuationCommand": {"token": "NEXT"}}}}
            ]}}
        ]}}}}});
        let page = parse_playlist_page(&json);
        assert_eq!(page.continuation.as_deref(), Some("NEXT"));
        assert_eq!(page.tracks.len(), 2);
        assert_eq!(page.tracks[1].artists, "Unlinked Artist");
        assert_eq!(page.tracks[1].duration, Some(125));
        assert_eq!(page.tracks[1].album, None);
    }

    #[test]
    fn test_parse_playlist_continuation_page() {
        let json = json!({"onResponseReceivedActions": [{"appendContinuationItemsAction": {"continuationItems": [
            playlist_row(Some("v3"), "Later Song", "Band")
        ]}}]});
        let page = parse_playlist_page(&json);
        assert_eq!(page.tracks.len(), 1);
        assert_eq!(page.continuation, None);
    }

    #[test]
    fn test_split_song_subtitle_variants() {
        assert_eq!(split_song_subtitle("Band"), ("Band".into(), None));
        assert_eq!(
            split_song_subtitle("Band • Record"),
            ("Band".into(), Some("Record".into()))
        );
        assert_eq!(
            split_song_subtitle("Song • Band • 3:20"),
            ("Band".into(), None)
        );
        assert_eq!(split_song_subtitle(""), (String::new(), None));
    }
}
