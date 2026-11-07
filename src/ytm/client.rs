use std::sync::Arc;

use futures::StreamExt;
use thiserror::Error;
use ytmapi_rs::auth::BrowserToken;
use ytmapi_rs::common::{AlbumID, ArtistChannelID, LikeStatus, PlaylistID, VideoID, YoutubeID};
use ytmapi_rs::parse::{PlaylistItem, SearchResultPlaylist};
use ytmapi_rs::query::{GetArtistQuery, GetPlaylistTracksQuery};
use ytmapi_rs::{Client, YtMusic, YtMusicBuilder};

use super::models::{parse_duration, Account, Entry, ItemRef, Shelf, Track};
use super::raw::{
    parse_accounts, parse_home, parse_lyrics, parse_playlist_page, HomePage, RawPost,
};

/// Auto playlist with the songs you liked.
const LIKED_PLAYLIST_ID: &str = "LM";
/// Maximum pages fetched for long playlists (100 tracks per page).
const MAX_PLAYLIST_PAGES: usize = 20;

#[derive(Debug, Error)]
pub enum YtmError {
    #[error("YouTube Music request failed: {0}")]
    Api(#[from] ytmapi_rs::Error),
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid header value: {0}")]
    Header(#[from] reqwest::header::InvalidHeaderValue),
}

/// Search result category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFilter {
    Songs,
    Albums,
    Artists,
    Playlists,
}

impl SearchFilter {
    pub const ALL: [SearchFilter; 4] = [Self::Songs, Self::Albums, Self::Artists, Self::Playlists];

    pub fn label(self) -> &'static str {
        match self {
            Self::Songs => "Songs",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Playlists => "Playlists",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Contents of an album, playlist or artist page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub title: String,
    pub subtitle: String,
    pub entries: Vec<Entry>,
}

/// Authenticated YouTube Music API handle (cheap to clone).
#[derive(Clone)]
pub struct Ytm {
    inner: Arc<YtMusic<BrowserToken>>,
}

impl Ytm {
    /// Connects with a `Cookie` header for the given account index in the session.
    pub async fn connect(cookie_header: String, authuser: u8) -> Result<Self, YtmError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Goog-AuthUser", authuser.to_string().parse()?);
        // Google resets HTTP/2 streams for the crate's fixed legacy user agent; HTTP/1.1 is accepted.
        let http = reqwest::Client::builder()
            .http1_only()
            .default_headers(headers)
            .build()?;
        let inner = YtMusicBuilder::new_with_client(Client::new_from_reqwest_client(http))
            .with_browser_token_cookie(cookie_header)
            .build()
            .await?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    pub async fn accounts(&self) -> Result<Vec<Account>, YtmError> {
        Ok(parse_accounts(
            &self
                .inner
                .json_query(RawPost::accounts())
                .await?
                .into_inner(),
        ))
    }

    pub async fn home(&self, continuation: Option<&str>) -> Result<HomePage, YtmError> {
        let query = match continuation {
            Some(token) => RawPost::continuation(token),
            None => RawPost::home(),
        };
        Ok(parse_home(
            &self.inner.json_query(query).await?.into_inner(),
        ))
    }

    pub async fn search(&self, query: &str, filter: SearchFilter) -> Result<Vec<Entry>, YtmError> {
        let yt = &self.inner;
        let entries = match filter {
            SearchFilter::Songs => yt
                .search_songs(query)
                .await?
                .into_iter()
                .map(|s| {
                    Entry::Track(Track {
                        video_id: s.video_id.get_raw().to_string(),
                        title: s.title,
                        artists: s.artist,
                        album: s.album.map(|a| a.name),
                        duration: parse_duration(&s.duration),
                        liked: None,
                    })
                })
                .collect(),
            SearchFilter::Albums => yt
                .search_albums(query)
                .await?
                .into_iter()
                .map(|a| {
                    Entry::Item(ItemRef::Album {
                        id: a.album_id.get_raw().to_string(),
                        title: a.title,
                        subtitle: join_nonempty(&[&a.artist, &a.year]),
                    })
                })
                .collect(),
            SearchFilter::Artists => yt
                .search_artists(query)
                .await?
                .into_iter()
                .map(|a| {
                    Entry::Item(ItemRef::Artist {
                        id: a.browse_id.get_raw().to_string(),
                        name: a.artist,
                        subtitle: a.subscribers.unwrap_or_default(),
                    })
                })
                .collect(),
            SearchFilter::Playlists => yt
                .search_playlists(query)
                .await?
                .into_iter()
                .filter_map(|p| match p {
                    SearchResultPlaylist::Featured(p) => Some((
                        p.playlist_id,
                        p.title,
                        join_nonempty(&[&p.author, &p.songs]),
                    )),
                    SearchResultPlaylist::Community(p) => Some((
                        p.playlist_id,
                        p.title,
                        join_nonempty(&[&p.author, &p.views]),
                    )),
                    _ => None,
                })
                .map(|(id, title, subtitle)| {
                    Entry::Item(ItemRef::Playlist {
                        id: id.get_raw().to_string(),
                        title,
                        subtitle,
                    })
                })
                .collect(),
        };
        Ok(entries)
    }

    /// Your playlists, with "Liked music" first.
    pub async fn library_playlists(&self) -> Result<Vec<Entry>, YtmError> {
        let mut entries = vec![Entry::Item(ItemRef::Playlist {
            id: LIKED_PLAYLIST_ID.into(),
            title: "Liked music".into(),
            subtitle: "Auto playlist".into(),
        })];
        entries.extend(
            self.inner
                .get_library_playlists()
                .await?
                .into_iter()
                .filter(|p| p.playlist_id.get_raw() != LIKED_PLAYLIST_ID)
                .map(|p| {
                    Entry::Item(ItemRef::Playlist {
                        id: p.playlist_id.get_raw().to_string(),
                        title: p.title,
                        subtitle: join_nonempty(&[&p.author, &p.tracks]),
                    })
                }),
        );
        Ok(entries)
    }

    pub async fn playlist(&self, id: &str) -> Result<Vec<Track>, YtmError> {
        let mut tracks = match self.playlist_via_crate(id).await {
            Ok(tracks) => tracks,
            Err(e) => {
                tracing::warn!("playlist {id}: crate parser failed ({e}), using raw parser");
                self.playlist_raw(id).await?
            }
        };
        if id == LIKED_PLAYLIST_ID {
            tracks.iter_mut().for_each(|t| t.liked = Some(true));
        }
        Ok(tracks)
    }

    async fn playlist_via_crate(&self, id: &str) -> Result<Vec<Track>, YtmError> {
        let query = GetPlaylistTracksQuery::new(PlaylistID::from_raw(playlist_browse_id(id)));
        let pages: Vec<_> = self
            .inner
            .stream(&query)
            .take(MAX_PLAYLIST_PAGES)
            .collect()
            .await;
        let mut tracks = Vec::new();
        for page in pages {
            tracks.extend(page?.into_iter().filter_map(playlist_item_to_track));
        }
        Ok(tracks)
    }

    /// Playlist parser tolerant to rows the crate rejects (e.g. artists without a channel).
    async fn playlist_raw(&self, id: &str) -> Result<Vec<Track>, YtmError> {
        let mut query = RawPost::browse(&playlist_browse_id(id));
        let mut tracks = Vec::new();
        for _ in 0..MAX_PLAYLIST_PAGES {
            let page = parse_playlist_page(&self.inner.json_query(query).await?.into_inner());
            tracks.extend(page.tracks);
            match page.continuation {
                Some(token) => query = RawPost::browse_continuation(&token),
                None => break,
            }
        }
        Ok(tracks)
    }

    pub async fn album(&self, id: &str) -> Result<Collection, YtmError> {
        let album = self
            .inner
            .get_album(AlbumID::from_raw(id.to_string()))
            .await?;
        let artists = album
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let entries = album
            .tracks
            .into_iter()
            .map(|t| {
                Entry::Track(Track {
                    video_id: t.video_id.get_raw().to_string(),
                    title: t.title,
                    artists: artists.clone(),
                    album: Some(album.title.clone()),
                    duration: parse_duration(&t.duration),
                    liked: like_flag(&t.like_status),
                })
            })
            .collect();
        Ok(Collection {
            subtitle: join_nonempty(&[&artists, &album.year]),
            title: album.title,
            entries,
        })
    }

    pub async fn artist(&self, id: &str) -> Result<Collection, YtmError> {
        let artist = self
            .inner
            .get_artist(GetArtistQuery::new(ArtistChannelID::from_raw(
                id.to_string(),
            )))
            .await?;
        let mut shelves = Vec::new();
        if let Some(songs) = artist.top_releases.songs {
            shelves.push(Shelf {
                title: "Top songs".into(),
                entries: songs
                    .results
                    .into_iter()
                    .map(|s| {
                        Entry::Track(Track {
                            video_id: s.video_id.get_raw().to_string(),
                            title: s.title,
                            artists: s
                                .artists
                                .iter()
                                .map(|a| a.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                            album: Some(s.album.name),
                            duration: None,
                            liked: like_flag(&s.like_status),
                        })
                    })
                    .collect(),
            });
        }
        for (title, group) in [
            ("Albums", artist.top_releases.albums),
            ("Singles", artist.top_releases.singles),
        ] {
            if let Some(group) = group {
                shelves.push(Shelf {
                    title: title.into(),
                    entries: group
                        .results
                        .into_iter()
                        .map(|a| {
                            Entry::Item(ItemRef::Album {
                                id: a.album_id.get_raw().to_string(),
                                title: a.title,
                                subtitle: a.year,
                            })
                        })
                        .collect(),
                });
            }
        }
        Ok(Collection {
            title: artist.name,
            subtitle: artist.subscribers.unwrap_or_default(),
            entries: super::models::flatten_shelves(&shelves),
        })
    }

    /// Radio ("up next") for a song.
    pub async fn radio(&self, video_id: &str) -> Result<Vec<Track>, YtmError> {
        let tracks = self
            .inner
            .get_watch_playlist_from_video_id(VideoID::from_raw(video_id.to_string()))
            .await?;
        Ok(tracks
            .into_iter()
            .map(|t| Track {
                video_id: t.video_id.get_raw().to_string(),
                title: t.title,
                artists: t.author,
                album: None,
                duration: parse_duration(&t.duration),
                liked: None,
            })
            .collect())
    }

    /// Lyrics text, or `None` when the song has no lyrics.
    pub async fn lyrics(&self, video_id: &str) -> Result<Option<String>, YtmError> {
        let Ok(lyrics_id) = self
            .inner
            .get_lyrics_id(VideoID::from_raw(video_id.to_string()))
            .await
        else {
            return Ok(None);
        };
        match self.inner.get_lyrics(lyrics_id.clone()).await {
            Ok(lyrics) => Ok(Some(format!("{}\n\n{}", lyrics.lyrics, lyrics.source))),
            Err(e) => {
                // The crate rejects pages it does not know, such as "Lyrics not available".
                let json = self
                    .inner
                    .json_query(RawPost::browse(lyrics_id.get_raw()))
                    .await?
                    .into_inner();
                match parse_lyrics(&json) {
                    Some(result) => Ok(result),
                    None => Err(e.into()),
                }
            }
        }
    }

    pub async fn rate(&self, video_id: &str, liked: bool) -> Result<(), YtmError> {
        let status = if liked {
            LikeStatus::Liked
        } else {
            LikeStatus::Indifferent
        };
        self.inner
            .rate_song(VideoID::from_raw(video_id.to_string()), status)
            .await?;
        Ok(())
    }

    /// Records a play in the account history (feeds recommendations).
    pub async fn add_history(&self, video_id: &str) -> Result<(), YtmError> {
        let url = self
            .inner
            .get_song_tracking_url(VideoID::from_raw(video_id.to_string()))
            .await?;
        self.inner.add_history_item(url).await?;
        Ok(())
    }
}

fn playlist_item_to_track(item: PlaylistItem) -> Option<Track> {
    match item {
        PlaylistItem::Song(s) if s.is_available => Some(Track {
            video_id: s.video_id.get_raw().to_string(),
            title: s.title,
            artists: s
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            album: Some(s.album.name),
            duration: parse_duration(&s.duration),
            liked: like_flag(&s.like_status),
        }),
        PlaylistItem::Video(v) => Some(Track {
            video_id: v.video_id.get_raw().to_string(),
            title: v.title,
            artists: v.channel_name,
            album: None,
            duration: parse_duration(&v.duration),
            liked: like_flag(&v.like_status),
        }),
        _ => None,
    }
}

/// Playlist pages are browsed as `VL<playlistId>`; ytmapi-rs sends the id as is.
fn playlist_browse_id(id: &str) -> String {
    if id.starts_with("VL") {
        id.to_string()
    } else {
        format!("VL{id}")
    }
}

fn like_flag(status: &LikeStatus) -> Option<bool> {
    Some(matches!(status, LikeStatus::Liked))
}

fn join_nonempty(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|p| !p.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" • ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playlist_browse_id_adds_prefix_once() {
        assert_eq!(playlist_browse_id("LM"), "VLLM");
        assert_eq!(playlist_browse_id("PLabc"), "VLPLabc");
        assert_eq!(playlist_browse_id("VLPLabc"), "VLPLabc");
    }

    #[test]
    fn test_search_filter_cycles() {
        assert_eq!(SearchFilter::Playlists.next(), SearchFilter::Songs);
        assert_eq!(SearchFilter::Songs.next(), SearchFilter::Albums);
    }
}

/// Checks against the real API; needs `TERMTUBE_COOKIES` (Netscape file) and `TERMTUBE_AUTHUSER`.
#[cfg(test)]
mod live {
    use super::*;

    async fn client() -> Ytm {
        let path = std::env::var("TERMTUBE_COOKIES").expect("TERMTUBE_COOKIES");
        let authuser = std::env::var("TERMTUBE_AUTHUSER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let header = crate::config::cookies::load_header(std::path::Path::new(&path)).unwrap();
        Ytm::connect(header, authuser).await.unwrap()
    }

    #[tokio::test]
    #[ignore = "hits the real YouTube Music API"]
    async fn live_lyrics_for_radio() {
        let ytm = client().await;
        let mut failed = 0;
        for t in ytm.radio("9RfVp-GhKfs").await.unwrap() {
            match ytm.lyrics(&t.video_id).await {
                Ok(Some(_)) => {}
                Ok(None) => println!("NONE {} {}", t.video_id, t.title),
                Err(e) => {
                    failed += 1;
                    println!("FAIL {} {}: {e}", t.video_id, t.title);
                }
            }
        }
        println!("failed: {failed}");
    }

    /// Loads the playlist given in `TERMTUBE_PLAYLIST`.
    #[tokio::test]
    #[ignore = "hits the real YouTube Music API"]
    async fn live_playlist_from_env() {
        let id = std::env::var("TERMTUBE_PLAYLIST").expect("TERMTUBE_PLAYLIST");
        let tracks = client().await.playlist(&id).await.unwrap();
        println!(
            "{id}: {} tracks, first = {:?}",
            tracks.len(),
            tracks.first().map(|t| &t.title)
        );
        assert!(!tracks.is_empty());
    }

    #[tokio::test]
    #[ignore = "hits the real YouTube Music API"]
    async fn live_library_playlists_load() {
        let ytm = client().await;
        let entries = ytm.library_playlists().await.unwrap();
        for entry in entries {
            if let Entry::Item(ItemRef::Playlist { id, title, .. }) = entry {
                match ytm.playlist(&id).await {
                    Ok(tracks) => println!("OK   {title}: {} tracks", tracks.len()),
                    Err(e) => println!("FAIL {title}: {e}"),
                }
            }
        }
    }
}
