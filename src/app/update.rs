use rand::thread_rng;

use crate::player::{EndReason, PlayerEvent};
use crate::ytm::{Entry, ItemRef, Track};

use super::msg::{Action, DataEvent, Effect, MpvCmd, Msg};
use super::state::{App, Focus, LyricsState, Page, PageKind, Panel, Section};

const SEEK_STEP: f64 = 5.0;
const VOLUME_STEP: u8 = 5;
const PAGE_STEP: isize = 10;

/// Applies a message to the state and returns the side effects to run.
pub fn update(app: &mut App, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Action(action) => on_action(app, action),
        Msg::Data(event) => on_data(app, event),
        Msg::Player(event) => on_player(app, event),
        Msg::Tick => on_tick(app),
    }
}

/// Effects needed when the app starts.
pub fn init(app: &mut App) -> Vec<Effect> {
    app.root_mut(Section::Home).loading = true;
    vec![Effect::LoadHome { continuation: None }, Effect::LoadLiked]
}

fn on_action(app: &mut App, action: Action) -> Vec<Effect> {
    if app.show_help && !matches!(action, Action::Quit) {
        app.show_help = false;
        return vec![];
    }
    match action {
        Action::Quit => {
            app.should_quit = true;
            vec![]
        }
        Action::ToggleHelp => {
            app.show_help = true;
            vec![]
        }
        Action::GoTo(section) => go_to(app, section),
        Action::FocusSearch => {
            let mut effects = go_to(app, Section::Search);
            while app.pop_page() {}
            app.focus = Focus::SearchInput;
            effects.clear();
            effects
        }
        Action::ToggleFocus => {
            app.focus = match (app.focus, app.panel) {
                (Focus::Main, Panel::Queue) => Focus::Queue,
                _ => Focus::Main,
            };
            vec![]
        }
        Action::Up
        | Action::Down
        | Action::PageUp
        | Action::PageDown
        | Action::Top
        | Action::Bottom => navigate(app, action),
        Action::Activate => activate(app),
        Action::Back => {
            if app.focus == Focus::SearchInput || app.focus == Focus::Queue {
                app.focus = Focus::Main;
            } else if !app.pop_page() && app.section != Section::Home {
                return go_to(app, Section::Home);
            }
            vec![]
        }
        Action::PlayPause => {
            if app.now_playing().is_some() {
                vec![Effect::Mpv(MpvCmd::TogglePause)]
            } else {
                activate(app)
            }
        }
        Action::Next => skip(app, false),
        Action::Previous => {
            if app.player.position > 3.0 {
                return vec![Effect::Mpv(MpvCmd::Seek(-app.player.position))];
            }
            if app.queue.back().is_some() {
                play_current(app)
            } else {
                vec![]
            }
        }
        Action::SeekForward => seek(app, SEEK_STEP),
        Action::SeekBackward => seek(app, -SEEK_STEP),
        Action::VolumeUp => set_volume(app, app.player.volume.saturating_add(VOLUME_STEP).min(100)),
        Action::VolumeDown => set_volume(app, app.player.volume.saturating_sub(VOLUME_STEP)),
        Action::ToggleShuffle => {
            app.queue.toggle_shuffle(&mut thread_rng());
            app.set_status(if app.queue.is_shuffled() {
                "Shuffle on"
            } else {
                "Shuffle off"
            });
            prefetch_next(app)
        }
        Action::CycleRepeat => {
            app.queue.set_repeat(app.queue.repeat().cycle());
            vec![]
        }
        Action::ToggleLike => toggle_like(app),
        Action::AddToQueue | Action::PlayNext => enqueue_selected(app, action == Action::PlayNext),
        Action::StartRadio => start_radio(app),
        Action::ToggleQueuePanel => {
            app.panel = if app.panel == Panel::Queue {
                Panel::None
            } else {
                Panel::Queue
            };
            app.queue_selected = app.queue.current_index().unwrap_or(0);
            if app.panel != Panel::Queue && app.focus == Focus::Queue {
                app.focus = Focus::Main;
            }
            vec![]
        }
        Action::ToggleLyricsPanel => {
            app.panel = if app.panel == Panel::Lyrics {
                Panel::None
            } else {
                Panel::Lyrics
            };
            if app.focus == Focus::Queue {
                app.focus = Focus::Main;
            }
            request_lyrics(app)
        }
        Action::ToggleMeter => {
            app.meter_enabled = !app.meter_enabled;
            app.player.levels = Default::default();
            vec![Effect::Mpv(MpvCmd::Meter(app.meter_enabled))]
        }
        Action::QueueMoveUp | Action::QueueMoveDown => {
            app.queue_selected = app
                .queue
                .move_by(app.queue_selected, action == Action::QueueMoveUp);
            prefetch_next(app)
        }
        Action::QueueRemove => {
            if app.queue.remove(app.queue_selected).is_none() {
                app.set_status("Can't remove the song that is playing");
            }
            app.queue_selected = app
                .queue_selected
                .min(app.queue.tracks().len().saturating_sub(1));
            prefetch_next(app)
        }
        Action::SearchChar(c) => {
            app.search_input.push(c);
            vec![]
        }
        Action::SearchBackspace => {
            app.search_input.pop();
            vec![]
        }
        Action::SearchClear => {
            app.search_input.clear();
            vec![]
        }
        Action::SearchNextFilter => {
            app.search_filter = app.search_filter.next();
            search(app)
        }
        Action::SearchSubmit => {
            app.focus = Focus::Main;
            search(app)
        }
        Action::Refresh => {
            let page = app.page_mut();
            page.loaded = false;
            page.continuation = None;
            load_page(app)
        }
    }
}

fn go_to(app: &mut App, section: Section) -> Vec<Effect> {
    app.section = section;
    if app.focus == Focus::SearchInput {
        app.focus = Focus::Main;
    }
    if section == Section::Search && app.page().entries.is_empty() {
        app.focus = Focus::SearchInput;
    }
    load_page(app)
}

/// Requests data for the current page if it has never been loaded.
fn load_page(app: &mut App) -> Vec<Effect> {
    let page = app.page_mut();
    if page.loaded || page.loading {
        return vec![];
    }
    let effect = match &page.kind {
        PageKind::Root(Section::Home) => Effect::LoadHome { continuation: None },
        PageKind::Root(Section::Library) => Effect::LoadLibrary,
        PageKind::Root(Section::Search) => return search(app),
        PageKind::Collection(item) => Effect::LoadCollection(item.clone()),
    };
    page.loading = true;
    vec![effect]
}

fn search(app: &mut App) -> Vec<Effect> {
    let query = app.search_input.trim().to_string();
    if query.is_empty() {
        return vec![];
    }
    let filter = app.search_filter;
    let page = app.root_mut(Section::Search);
    page.loading = true;
    page.title = format!("Search: {query}");
    page.subtitle = filter.label().to_string();
    vec![Effect::Search { query, filter }]
}

fn navigate(app: &mut App, action: Action) -> Vec<Effect> {
    if app.focus == Focus::Queue {
        let len = app.queue.tracks().len();
        let last = len.saturating_sub(1);
        app.queue_selected = match action {
            Action::Up => app.queue_selected.saturating_sub(1),
            Action::Down => (app.queue_selected + 1).min(last),
            Action::PageUp => app.queue_selected.saturating_sub(PAGE_STEP as usize),
            Action::PageDown => (app.queue_selected + PAGE_STEP as usize).min(last),
            Action::Top => 0,
            _ => last,
        };
        return vec![];
    }
    if app.focus == Focus::SearchInput {
        app.focus = Focus::Main;
        return vec![];
    }
    let page = app.page_mut();
    match action {
        Action::Up => page.move_selection(-1),
        Action::Down => page.move_selection(1),
        Action::PageUp => page.move_selection(-PAGE_STEP),
        Action::PageDown => page.move_selection(PAGE_STEP),
        Action::Top => page.select_edge(true),
        _ => page.select_edge(false),
    }
    if page.near_end() && !page.loading && page.kind == PageKind::Root(Section::Home) {
        if let Some(token) = page.continuation.clone() {
            page.loading = true;
            return vec![Effect::LoadHome {
                continuation: Some(token),
            }];
        }
    }
    vec![]
}

fn activate(app: &mut App) -> Vec<Effect> {
    if app.focus == Focus::Queue {
        return if app.queue.jump(app.queue_selected).is_some() {
            play_current(app)
        } else {
            vec![]
        };
    }
    let page = app.page();
    match page.selected_entry().cloned() {
        Some(Entry::Track(track)) => {
            if matches!(page.kind, PageKind::Collection(_)) {
                let (tracks, start) = page.tracks_from_selection();
                app.queue.set(tracks, start);
                play_current(app)
            } else {
                app.queue.set(vec![track.clone()], 0);
                let mut effects = play_current(app);
                effects.push(radio_effect(app, &track.video_id));
                effects
            }
        }
        Some(Entry::Item(item)) => open_item(app, item),
        _ => vec![],
    }
}

fn open_item(app: &mut App, item: ItemRef) -> Vec<Effect> {
    let mut page = Page::new(PageKind::Collection(item.clone()), item.title());
    page.subtitle = item.subtitle().to_string();
    page.loading = true;
    app.push_page(page);
    vec![Effect::LoadCollection(item)]
}

fn radio_effect(app: &mut App, seed: &str) -> Effect {
    app.radio_loading = true;
    Effect::Radio {
        seed: seed.to_string(),
    }
}

fn start_radio(app: &mut App) -> Vec<Effect> {
    let seed = match app.page().selected_entry() {
        Some(Entry::Track(t)) if app.focus == Focus::Main => t.clone(),
        _ => match app.now_playing() {
            Some(t) => t.clone(),
            None => return vec![],
        },
    };
    app.queue.set(vec![seed.clone()], 0);
    app.set_status(format!("Radio: {}", seed.title));
    let mut effects = play_current(app);
    effects.push(radio_effect(app, &seed.video_id));
    effects
}

fn enqueue_selected(app: &mut App, next: bool) -> Vec<Effect> {
    let tracks: Vec<Track> = match app.page().selected_entry() {
        Some(Entry::Track(t)) => vec![t.clone()],
        _ => return vec![],
    };
    let was_empty = app.now_playing().is_none();
    let title = tracks[0].title.clone();
    if next {
        tracks.into_iter().for_each(|t| app.queue.play_next(t));
    } else {
        app.queue.append(tracks);
    }
    app.set_status(if next {
        format!("Playing next: {title}")
    } else {
        format!("Added to queue: {title}")
    });
    if was_empty {
        play_current(app)
    } else {
        prefetch_next(app)
    }
}

fn skip(app: &mut App, auto: bool) -> Vec<Effect> {
    if app.queue.advance(auto).is_some() {
        play_current(app)
    } else {
        app.player.loading = false;
        vec![Effect::Mpv(MpvCmd::Stop)]
    }
}

/// Starts playback of the queue's current track, resolving its URL if needed.
fn play_current(app: &mut App) -> Vec<Effect> {
    let Some(track) = app.now_playing().cloned() else {
        return vec![];
    };
    app.player.position = 0.0;
    app.player.duration = track.duration.unwrap_or(0) as f64;
    app.player.loading = true;
    app.queue_selected = app.queue.current_index().unwrap_or(0);
    let mut effects = request_lyrics(app);
    if let Some(stream) = app.resolver.cached(&track.video_id) {
        app.pending_play = None;
        effects.push(Effect::Mpv(MpvCmd::Load(stream.url.clone())));
    } else {
        app.pending_play = Some(track.video_id.clone());
        if app.resolving.insert(track.video_id.clone()) {
            effects.push(Effect::Resolve(track.video_id));
        }
    }
    effects
}

/// Resolves the next track ahead of time so it starts instantly.
fn prefetch_next(app: &mut App) -> Vec<Effect> {
    let Some(next) = app.queue.peek_next().map(|t| t.video_id.clone()) else {
        return vec![];
    };
    if app.resolver.cached(&next).is_some() || !app.resolving.insert(next.clone()) {
        return vec![];
    }
    vec![Effect::Resolve(next)]
}

fn request_lyrics(app: &mut App) -> Vec<Effect> {
    if app.panel != Panel::Lyrics {
        return vec![];
    }
    let Some(id) = app.now_playing().map(|t| t.video_id.clone()) else {
        return vec![];
    };
    if app.lyrics.as_ref().is_some_and(|(v, _)| *v == id) {
        return vec![];
    }
    app.lyrics = Some((id.clone(), LyricsState::Loading));
    vec![Effect::Lyrics(id)]
}

fn seek(app: &mut App, delta: f64) -> Vec<Effect> {
    if app.now_playing().is_none() {
        return vec![];
    }
    vec![Effect::Mpv(MpvCmd::Seek(delta))]
}

fn set_volume(app: &mut App, volume: u8) -> Vec<Effect> {
    app.player.volume = volume;
    vec![Effect::Mpv(MpvCmd::Volume(volume))]
}

fn toggle_like(app: &mut App) -> Vec<Effect> {
    let Some(track) = app.now_playing().cloned() else {
        return vec![];
    };
    let liked = !app.is_liked(&track.video_id);
    set_liked(app, &track.video_id, liked);
    app.set_status(if liked {
        format!("♥ Liked: {}", track.title)
    } else {
        format!("Removed like: {}", track.title)
    });
    vec![Effect::Rate {
        video_id: track.video_id,
        liked,
    }]
}

fn set_liked(app: &mut App, video_id: &str, liked: bool) {
    if liked {
        app.liked_ids.insert(video_id.to_string());
    } else {
        app.liked_ids.remove(video_id);
    }
}

fn tracks_of(entries: &[Entry]) -> impl Iterator<Item = &Track> {
    entries.iter().filter_map(|e| match e {
        Entry::Track(t) => Some(t),
        _ => None,
    })
}

fn on_data(app: &mut App, event: DataEvent) -> Vec<Effect> {
    match event {
        DataEvent::Home { append, result } => {
            let page = app.root_mut(Section::Home);
            match result {
                Ok(home) => {
                    let entries = crate::ytm::models::flatten_shelves(&home.shelves);
                    if append {
                        page.entries.extend(entries);
                        page.loading = false;
                    } else {
                        page.set_entries(entries);
                    }
                    page.continuation = home.continuation;
                }
                Err(e) => fail_page(page, e),
            }
            vec![]
        }
        DataEvent::Search {
            query,
            filter,
            result,
        } => {
            if query != app.search_input.trim() || filter != app.search_filter {
                return vec![];
            }
            let page = app.root_mut(Section::Search);
            match result {
                Ok(entries) if entries.is_empty() => {
                    page.set_entries(vec![]);
                    page.error = Some("No results".into());
                }
                Ok(entries) => page.set_entries(entries),
                Err(e) => fail_page(page, e),
            }
            vec![]
        }
        DataEvent::Library(result) => {
            let page = app.root_mut(Section::Library);
            match result {
                Ok(entries) => page.set_entries(entries),
                Err(e) => fail_page(page, e),
            }
            vec![]
        }
        DataEvent::Collection { item, result } => {
            if let Ok(c) = &result {
                let tracks: Vec<Track> = tracks_of(&c.entries).cloned().collect();
                app.merge_likes(&tracks);
            }
            if let Some(page) = app.find_page_mut(&PageKind::Collection(item)) {
                match result {
                    Ok(c) => {
                        if !c.title.is_empty() {
                            page.title = c.title;
                        }
                        if !c.subtitle.is_empty() {
                            page.subtitle = c.subtitle;
                        }
                        page.set_entries(c.entries);
                    }
                    Err(e) => fail_page(page, e),
                }
            }
            vec![]
        }
        DataEvent::Radio { seed, result } => {
            app.radio_loading = false;
            match result {
                Ok(tracks) => {
                    if app.queue.tracks().iter().all(|t| t.video_id != seed) {
                        return vec![];
                    }
                    let known: std::collections::HashSet<String> = app
                        .queue
                        .tracks()
                        .iter()
                        .map(|t| t.video_id.clone())
                        .collect();
                    app.queue
                        .append(tracks.into_iter().filter(|t| !known.contains(&t.video_id)));
                    prefetch_next(app)
                }
                Err(e) => {
                    app.set_status(format!("Radio failed: {e}"));
                    vec![]
                }
            }
        }
        DataEvent::Lyrics { video_id, result } => {
            if let Some((id, state)) = &mut app.lyrics {
                if *id == video_id {
                    *state = match result {
                        Ok(Some(text)) => LyricsState::Loaded(text),
                        Ok(None) => LyricsState::Missing,
                        Err(e) => LyricsState::Failed(e),
                    };
                }
            }
            vec![]
        }
        DataEvent::Rated {
            video_id,
            liked,
            result,
        } => {
            if let Err(e) = result {
                set_liked(app, &video_id, !liked);
                app.set_status(format!("Could not update like: {e}"));
            }
            vec![]
        }
        DataEvent::Liked(result) => {
            match result {
                Ok(tracks) => app.merge_likes(&tracks),
                Err(e) => tracing::warn!("could not load liked songs: {e}"),
            }
            vec![]
        }
        DataEvent::Resolved { video_id, result } => {
            app.resolving.remove(&video_id);
            let is_pending = app.pending_play.as_deref() == Some(video_id.as_str());
            match result {
                Ok(stream) => {
                    let url = stream.url.clone();
                    app.resolver.store(video_id, stream);
                    if is_pending {
                        app.pending_play = None;
                        return vec![Effect::Mpv(MpvCmd::Load(url))];
                    }
                }
                Err(e) if is_pending => {
                    // Let mpv's own yt-dlp hook try the watch URL as a fallback.
                    tracing::warn!("resolve failed for {video_id}: {e}");
                    app.pending_play = None;
                    if let Some(t) = app.now_playing() {
                        return vec![Effect::Mpv(MpvCmd::Load(t.watch_url()))];
                    }
                }
                Err(e) => tracing::warn!("prefetch failed for {video_id}: {e}"),
            }
            vec![]
        }
    }
}

fn fail_page(page: &mut Page, error: String) {
    page.loading = false;
    page.error = Some(error);
}

fn on_player(app: &mut App, event: PlayerEvent) -> Vec<Effect> {
    match event {
        PlayerEvent::TimePos(p) => app.player.position = p,
        PlayerEvent::Duration(d) => app.player.duration = d,
        PlayerEvent::Paused(p) => app.player.paused = p,
        PlayerEvent::Volume(v) => app.player.volume = v.round().clamp(0.0, 100.0) as u8,
        PlayerEvent::Meter(levels) => app.player.levels = levels,
        PlayerEvent::Idle(_) => {}
        PlayerEvent::FileLoaded => {
            app.player.loading = false;
            if let Some(id) = app.now_playing().map(|t| t.video_id.clone()) {
                app.retried.remove(&id);
            }
            let mut effects = prefetch_next(app);
            if let Some(t) = app.now_playing() {
                effects.push(Effect::AddHistory(t.video_id.clone()));
            }
            if app.queue.needs_radio() && !app.radio_loading {
                if let Some(seed) = app.now_playing().map(|t| t.video_id.clone()) {
                    effects.push(radio_effect(app, &seed));
                }
            }
            return effects;
        }
        PlayerEvent::EndFile(EndReason::Eof) => return skip(app, true),
        PlayerEvent::EndFile(EndReason::Error(e)) => {
            let Some(track) = app.now_playing().cloned() else {
                return vec![];
            };
            if app.retried.insert(track.video_id.clone()) {
                // Stream URLs are sometimes rejected; a fresh one usually works.
                tracing::warn!("playback failed for {}: {e}; retrying", track.video_id);
                app.resolver.forget(&track.video_id);
                app.set_status(format!("Retrying \"{}\"…", track.title));
                return play_current(app);
            }
            app.retried.remove(&track.video_id);
            app.set_status(format!("Could not play \"{}\": {e}", track.title));
            return skip(app, false);
        }
        PlayerEvent::EndFile(EndReason::Stopped) => {}
        PlayerEvent::Exited => {
            app.set_status("mpv exited");
            app.should_quit = true;
        }
    }
    vec![]
}

fn on_tick(app: &mut App) -> Vec<Effect> {
    let audible = app.now_playing().is_some() && !app.player.paused && !app.player.loading;
    if app.meter_enabled && audible {
        vec![Effect::Mpv(MpvCmd::PollMeter)]
    } else {
        if !audible {
            app.player.levels = Default::default();
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{Resolver, StreamUrl};
    use crate::ytm::client::Collection;
    use crate::ytm::models::Shelf;
    use crate::ytm::raw::HomePage;
    use crate::ytm::SearchFilter;

    fn app() -> App {
        App::new(Resolver::new("yt-dlp".into(), "/tmp/c".into()), 50, true)
    }

    fn track(id: &str) -> Track {
        Track {
            video_id: id.into(),
            title: format!("Title {id}"),
            artists: "Artist".into(),
            album: None,
            duration: Some(200),
            liked: Some(false),
        }
    }

    fn stream(url: &str) -> StreamUrl {
        StreamUrl {
            url: url.into(),
            expires_at: None,
        }
    }

    fn act(app: &mut App, a: Action) -> Vec<Effect> {
        update(app, Msg::Action(a))
    }

    fn data(app: &mut App, d: DataEvent) -> Vec<Effect> {
        update(app, Msg::Data(d))
    }

    fn album() -> ItemRef {
        ItemRef::Album {
            id: "MPREbX".into(),
            title: "Record".into(),
            subtitle: "Band".into(),
        }
    }

    #[test]
    fn test_init_loads_home_and_fills_page() {
        let mut a = app();
        assert_eq!(
            init(&mut a),
            vec![Effect::LoadHome { continuation: None }, Effect::LoadLiked]
        );
        let home = HomePage {
            shelves: vec![Shelf {
                title: "Picks".into(),
                entries: vec![Entry::Track(track("a"))],
            }],
            continuation: Some("tok".into()),
        };
        data(
            &mut a,
            DataEvent::Home {
                append: false,
                result: Ok(home),
            },
        );
        assert_eq!(a.page().entries.len(), 2);
        assert_eq!(a.page().selected, 1);
        assert_eq!(a.page().continuation.as_deref(), Some("tok"));
        assert!(!a.page().loading);
    }

    #[test]
    fn test_playing_single_song_resolves_then_loads_and_starts_radio() {
        let mut a = app();
        a.root_mut(Section::Home)
            .set_entries(vec![Entry::Track(track("a"))]);
        let effects = act(&mut a, Action::Activate);
        assert_eq!(
            effects,
            vec![
                Effect::Resolve("a".into()),
                Effect::Radio { seed: "a".into() }
            ]
        );
        assert!(a.player.loading);
        let effects = data(
            &mut a,
            DataEvent::Resolved {
                video_id: "a".into(),
                result: Ok(stream("http://a")),
            },
        );
        assert_eq!(effects, vec![Effect::Mpv(MpvCmd::Load("http://a".into()))]);
        let effects = data(
            &mut a,
            DataEvent::Radio {
                seed: "a".into(),
                result: Ok(vec![track("a"), track("b"), track("c")]),
            },
        );
        assert_eq!(a.queue.tracks().len(), 3, "seed is not duplicated");
        assert_eq!(effects, vec![Effect::Resolve("b".into())]);
    }

    #[test]
    fn test_cached_stream_loads_immediately() {
        let mut a = app();
        a.resolver.store("a".into(), stream("http://cached"));
        a.queue.set(vec![track("a")], 0);
        assert_eq!(
            play_current(&mut a),
            vec![Effect::Mpv(MpvCmd::Load("http://cached".into()))]
        );
        assert!(a.pending_play.is_none());
    }

    #[test]
    fn test_collection_play_queues_all_tracks_from_selection() {
        let mut a = app();
        act(&mut a, Action::GoTo(Section::Library));
        let effects = open_item(&mut a, album());
        assert_eq!(effects, vec![Effect::LoadCollection(album())]);
        let col = Collection {
            title: "Record".into(),
            subtitle: "Band • 2001".into(),
            entries: vec![
                Entry::Track(track("1")),
                Entry::Track(track("2")),
                Entry::Track(track("3")),
            ],
        };
        data(
            &mut a,
            DataEvent::Collection {
                item: album(),
                result: Ok(col),
            },
        );
        assert_eq!(a.page().subtitle, "Band • 2001");
        act(&mut a, Action::Down);
        let effects = act(&mut a, Action::Activate);
        assert_eq!(a.queue.tracks().len(), 3);
        assert_eq!(a.now_playing().unwrap().video_id, "2");
        assert!(
            !effects.iter().any(|e| matches!(e, Effect::Radio { .. })),
            "albums play without radio"
        );
    }

    #[test]
    fn test_end_of_file_advances_and_errors_skip() {
        let mut a = app();
        a.queue.set(vec![track("1"), track("2"), track("3")], 0);
        a.resolver.store("2".into(), stream("http://2"));
        let effects = update(&mut a, Msg::Player(PlayerEvent::EndFile(EndReason::Eof)));
        assert_eq!(effects, vec![Effect::Mpv(MpvCmd::Load("http://2".into()))]);
        let fail = || Msg::Player(PlayerEvent::EndFile(EndReason::Error("boom".into())));
        let effects = update(&mut a, fail());
        assert_eq!(
            effects,
            vec![Effect::Resolve("2".into())],
            "first failure re-resolves"
        );
        assert_eq!(a.now_playing().unwrap().video_id, "2");
        assert!(a.resolver.cached("2").is_none(), "bad URL is forgotten");
        update(&mut a, fail());
        assert_eq!(
            a.now_playing().unwrap().video_id,
            "3",
            "second failure skips"
        );
        assert!(a.status().unwrap().contains("boom"));
        let effects = update(&mut a, Msg::Player(PlayerEvent::EndFile(EndReason::Eof)));
        assert_eq!(effects, vec![Effect::Mpv(MpvCmd::Stop)]);
    }

    #[test]
    fn test_retry_budget_resets_after_successful_load() {
        let mut a = app();
        a.queue.set(vec![track("1"), track("2")], 0);
        let fail = || Msg::Player(PlayerEvent::EndFile(EndReason::Error("403".into())));
        update(&mut a, fail());
        update(&mut a, Msg::Player(PlayerEvent::FileLoaded));
        update(&mut a, fail());
        assert_eq!(
            a.now_playing().unwrap().video_id,
            "1",
            "retried again instead of skipping"
        );
    }

    #[test]
    fn test_file_loaded_prefetches_records_history_and_refills_radio() {
        let mut a = app();
        a.queue.set(vec![track("1"), track("2")], 0);
        let effects = update(&mut a, Msg::Player(PlayerEvent::FileLoaded));
        assert_eq!(
            effects,
            vec![
                Effect::Resolve("2".into()),
                Effect::AddHistory("1".into()),
                Effect::Radio { seed: "1".into() }
            ]
        );
        assert!(!a.player.loading);
        let again = update(&mut a, Msg::Player(PlayerEvent::FileLoaded));
        assert!(
            !again
                .iter()
                .any(|e| matches!(e, Effect::Radio { .. } | Effect::Resolve(_))),
            "no duplicate requests"
        );
    }

    #[test]
    fn test_resolve_failure_falls_back_to_watch_url() {
        let mut a = app();
        a.queue.set(vec![track("x")], 0);
        play_current(&mut a);
        let effects = data(
            &mut a,
            DataEvent::Resolved {
                video_id: "x".into(),
                result: Err("nope".into()),
            },
        );
        assert_eq!(
            effects,
            vec![Effect::Mpv(MpvCmd::Load(
                "https://music.youtube.com/watch?v=x".into()
            ))]
        );
    }

    #[test]
    fn test_search_flow_and_stale_results_ignored() {
        let mut a = app();
        act(&mut a, Action::FocusSearch);
        assert_eq!(a.focus, Focus::SearchInput);
        for c in "abc".chars() {
            act(&mut a, Action::SearchChar(c));
        }
        let effects = act(&mut a, Action::SearchSubmit);
        assert_eq!(
            effects,
            vec![Effect::Search {
                query: "abc".into(),
                filter: SearchFilter::Songs
            }]
        );
        assert_eq!(a.focus, Focus::Main);
        data(
            &mut a,
            DataEvent::Search {
                query: "old".into(),
                filter: SearchFilter::Songs,
                result: Ok(vec![Entry::Track(track("z"))]),
            },
        );
        assert!(
            a.page().entries.is_empty(),
            "results for an old query are dropped"
        );
        data(
            &mut a,
            DataEvent::Search {
                query: "abc".into(),
                filter: SearchFilter::Songs,
                result: Ok(vec![Entry::Track(track("y"))]),
            },
        );
        assert_eq!(a.page().entries.len(), 1);
        let effects = act(&mut a, Action::SearchNextFilter);
        assert_eq!(
            effects,
            vec![Effect::Search {
                query: "abc".into(),
                filter: SearchFilter::Albums
            }]
        );
    }

    #[test]
    fn test_like_is_optimistic_and_reverted_on_error() {
        let mut a = app();
        a.queue.set(vec![track("1")], 0);
        let effects = act(&mut a, Action::ToggleLike);
        assert_eq!(
            effects,
            vec![Effect::Rate {
                video_id: "1".into(),
                liked: true
            }]
        );
        assert!(a.is_liked("1"));
        data(
            &mut a,
            DataEvent::Rated {
                video_id: "1".into(),
                liked: true,
                result: Err("401".into()),
            },
        );
        assert!(!a.is_liked("1"));
    }

    #[test]
    fn test_liked_songs_and_collections_feed_the_like_set() {
        let mut a = app();
        let mut liked = track("x");
        liked.liked = Some(true);
        data(
            &mut a,
            DataEvent::Liked(Ok(vec![liked.clone(), track("y")])),
        );
        assert!(a.is_liked("x"));
        assert!(!a.is_liked("y"), "liked: Some(false) is not added");
        let mut unliked = liked.clone();
        unliked.liked = Some(false);
        data(
            &mut a,
            DataEvent::Collection {
                item: album(),
                result: Ok(Collection {
                    title: String::new(),
                    subtitle: String::new(),
                    entries: vec![Entry::Track(unliked)],
                }),
            },
        );
        assert!(!a.is_liked("x"), "a fresher status from a page wins");
    }

    #[test]
    fn test_lyrics_requested_when_panel_opens_and_ignored_for_other_song() {
        let mut a = app();
        a.queue.set(vec![track("1")], 0);
        assert_eq!(
            act(&mut a, Action::ToggleLyricsPanel),
            vec![Effect::Lyrics("1".into())]
        );
        data(
            &mut a,
            DataEvent::Lyrics {
                video_id: "other".into(),
                result: Ok(Some("x".into())),
            },
        );
        assert_eq!(a.lyrics, Some(("1".into(), LyricsState::Loading)));
        data(
            &mut a,
            DataEvent::Lyrics {
                video_id: "1".into(),
                result: Ok(None),
            },
        );
        assert_eq!(a.lyrics, Some(("1".into(), LyricsState::Missing)));
    }

    #[test]
    fn test_previous_restarts_song_after_three_seconds() {
        let mut a = app();
        a.queue.set(vec![track("1"), track("2")], 1);
        a.player.position = 42.0;
        assert_eq!(
            act(&mut a, Action::Previous),
            vec![Effect::Mpv(MpvCmd::Seek(-42.0))]
        );
        a.player.position = 1.0;
        act(&mut a, Action::Previous);
        assert_eq!(a.now_playing().unwrap().video_id, "1");
    }

    #[test]
    fn test_tick_polls_meter_only_while_audible() {
        let mut a = app();
        assert!(update(&mut a, Msg::Tick).is_empty());
        a.queue.set(vec![track("1")], 0);
        assert_eq!(
            update(&mut a, Msg::Tick),
            vec![Effect::Mpv(MpvCmd::PollMeter)]
        );
        act(&mut a, Action::ToggleMeter);
        assert!(update(&mut a, Msg::Tick).is_empty());
    }

    #[test]
    fn test_back_pops_then_returns_home() {
        let mut a = app();
        act(&mut a, Action::GoTo(Section::Library));
        open_item(&mut a, album());
        act(&mut a, Action::Back);
        assert_eq!(a.section, Section::Library);
        assert_eq!(a.stack_depth(), 1);
        act(&mut a, Action::Back);
        assert_eq!(a.section, Section::Home);
    }

    #[test]
    fn test_home_pagination_when_near_end() {
        let mut a = app();
        let page = a.root_mut(Section::Home);
        page.set_entries(
            (0..6)
                .map(|i| Entry::Track(track(&i.to_string())))
                .collect(),
        );
        page.continuation = Some("next".into());
        let effects = act(&mut a, Action::Down);
        assert_eq!(
            effects,
            vec![Effect::LoadHome {
                continuation: Some("next".into())
            }]
        );
        assert!(
            act(&mut a, Action::Down).is_empty(),
            "no duplicate page request while loading"
        );
    }
}
