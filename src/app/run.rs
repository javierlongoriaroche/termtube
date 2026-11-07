use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::config::settings::Settings;
use crate::player::{Mpv, MpvConfig, PlayerEvent, Resolver};
use crate::ui::{self, theme::Theme};
use crate::ytm::client::Collection;
use crate::ytm::{Entry, ItemRef, Ytm};

use super::keymap::map_key;
use super::msg::{DataEvent, Effect, MpvCmd, Msg};
use super::state::App;
use super::update::{init, update};

/// Redraw and meter refresh interval.
const TICK: Duration = Duration::from_millis(100);

/// Runs the interactive player until the user quits.
pub async fn run(settings: Settings, ytm: Ytm) -> Result<(), Box<dyn std::error::Error>> {
    let (data_tx, mut data_rx) = mpsc::unbounded_channel::<DataEvent>();
    let (player_tx, mut player_rx) = mpsc::unbounded_channel::<PlayerEvent>();

    let socket = settings
        .data_dir()
        .join(format!("mpv-{}.sock", std::process::id()));
    let mpv = Mpv::spawn(
        &MpvConfig {
            binary: settings.mpv.path.clone(),
            ytdlp: settings.mpv.ytdlp_path.clone(),
            socket: socket.clone(),
            cookies: settings.cookies_path(),
            volume: settings.general.volume,
            meter: settings.meter.enabled,
            extra_args: settings.mpv.extra_args.clone(),
        },
        player_tx,
    )
    .await?;

    let resolver = Resolver::new(settings.mpv.ytdlp_path.clone(), settings.cookies_path());
    let mut app = App::new(resolver, settings.general.volume, settings.meter.enabled);
    let theme = Theme::from_settings(&settings.theme);
    let ctx = Ctx {
        ytm,
        data_tx,
        mpv: &mpv,
    };

    let mut terminal = ratatui::init();
    let result = async {
        for effect in init(&mut app) {
            ctx.execute(&app, effect);
        }
        let mut keys = EventStream::new();
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Background events only mark the screen dirty; drawing happens at most once
        // per tick, or right away after a key press, to keep CPU usage low.
        let mut dirty = true;
        while !app.should_quit {
            let (msg, is_key) = tokio::select! {
                Some(Ok(event)) = keys.next() => match event {
                    Event::Key(k) if k.kind == KeyEventKind::Press => (map_key(k, app.focus).map(Msg::Action), true),
                    Event::Resize(..) => (None, true),
                    _ => (None, false),
                },
                Some(event) = data_rx.recv() => (Some(Msg::Data(event)), false),
                Some(event) = player_rx.recv() => (Some(Msg::Player(event)), false),
                _ = tick.tick() => (Some(Msg::Tick), false),
            };
            let is_tick = matches!(msg, Some(Msg::Tick));
            if let Some(msg) = msg {
                dirty = true;
                for effect in update(&mut app, msg) {
                    ctx.execute(&app, effect);
                }
            }
            if (is_key || is_tick) && dirty {
                terminal.draw(|f| ui::draw(f, &app, &theme))?;
                dirty = false;
            }
        }
        Ok::<_, std::io::Error>(())
    }
    .await;
    ratatui::restore();
    mpv.quit();
    let _ = std::fs::remove_file(&socket);
    result?;
    Ok(())
}

/// Everything needed to carry out effects.
struct Ctx<'a> {
    ytm: Ytm,
    data_tx: UnboundedSender<DataEvent>,
    mpv: &'a Mpv,
}

impl Ctx<'_> {
    fn execute(&self, app: &App, effect: Effect) {
        match effect {
            Effect::Mpv(cmd) => self.mpv_command(cmd),
            Effect::Resolve(video_id) => {
                let task = app.resolver.resolve_task(&video_id);
                let tx = self.data_tx.clone();
                tokio::spawn(async move {
                    let result = task.await.map_err(|e| e.to_string());
                    let _ = tx.send(DataEvent::Resolved { video_id, result });
                });
            }
            other => self.spawn_api(other),
        }
    }

    fn mpv_command(&self, cmd: MpvCmd) {
        match cmd {
            MpvCmd::Load(url) => self.mpv.load(&url),
            MpvCmd::TogglePause => self.mpv.toggle_pause(),
            MpvCmd::Seek(s) => self.mpv.seek_relative(s),
            MpvCmd::Volume(v) => self.mpv.set_volume(v),
            MpvCmd::Stop => self.mpv.stop(),
            MpvCmd::Meter(on) => self.mpv.set_meter(on),
            MpvCmd::PollMeter => self.mpv.poll_meter(),
        }
    }

    /// Runs a YouTube Music request in the background and reports its result.
    fn spawn_api(&self, effect: Effect) {
        let ytm = self.ytm.clone();
        let tx = self.data_tx.clone();
        tokio::spawn(async move {
            let err = |e: crate::ytm::YtmError| {
                tracing::warn!("api error: {e}");
                e.to_string()
            };
            let event = match effect {
                Effect::LoadHome { continuation } => DataEvent::Home {
                    append: continuation.is_some(),
                    result: ytm.home(continuation.as_deref()).await.map_err(err),
                },
                Effect::Search { query, filter } => {
                    let result = ytm.search(&query, filter).await.map_err(err);
                    DataEvent::Search {
                        query,
                        filter,
                        result,
                    }
                }
                Effect::LoadLibrary => {
                    DataEvent::Library(ytm.library_playlists().await.map_err(err))
                }
                Effect::LoadLiked => DataEvent::Liked(ytm.playlist("LM").await.map_err(err)),
                Effect::LoadCollection(item) => {
                    let result = load_collection(&ytm, &item).await.map_err(err);
                    DataEvent::Collection { item, result }
                }
                Effect::Radio { seed } => {
                    let result = ytm.radio(&seed).await.map_err(err);
                    DataEvent::Radio { seed, result }
                }
                Effect::Lyrics(video_id) => {
                    let result = ytm.lyrics(&video_id).await.map_err(err);
                    DataEvent::Lyrics { video_id, result }
                }
                Effect::Rate { video_id, liked } => {
                    let result = ytm.rate(&video_id, liked).await.map_err(err);
                    DataEvent::Rated {
                        video_id,
                        liked,
                        result,
                    }
                }
                Effect::AddHistory(video_id) => {
                    if let Err(e) = ytm.add_history(&video_id).await {
                        tracing::warn!("history not recorded for {video_id}: {e}");
                    }
                    return;
                }
                Effect::Resolve(_) | Effect::Mpv(_) => return,
            };
            let _ = tx.send(event);
        });
    }
}

async fn load_collection(ytm: &Ytm, item: &ItemRef) -> Result<Collection, crate::ytm::YtmError> {
    match item {
        ItemRef::Album { id, .. } => ytm.album(id).await,
        ItemRef::Artist { id, .. } => ytm.artist(id).await,
        ItemRef::Playlist { id, .. } => Ok(Collection {
            title: String::new(),
            subtitle: String::new(),
            entries: ytm
                .playlist(id)
                .await?
                .into_iter()
                .map(Entry::Track)
                .collect(),
        }),
    }
}
