use crate::player::{PlayerEvent, StreamUrl};
use crate::ytm::client::Collection;
use crate::ytm::raw::HomePage;
use crate::ytm::{Entry, ItemRef, SearchFilter, Track};

use super::state::Section;

/// User intents produced by the keymap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    ToggleHelp,
    GoTo(Section),
    FocusSearch,
    ToggleFocus,
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Activate,
    Back,
    PlayPause,
    Next,
    Previous,
    SeekForward,
    SeekBackward,
    VolumeUp,
    VolumeDown,
    ToggleShuffle,
    CycleRepeat,
    ToggleLike,
    AddToQueue,
    PlayNext,
    StartRadio,
    ToggleQueuePanel,
    ToggleLyricsPanel,
    ToggleMeter,
    QueueMoveUp,
    QueueMoveDown,
    QueueRemove,
    SearchChar(char),
    SearchBackspace,
    SearchClear,
    SearchSubmit,
    SearchNextFilter,
    Refresh,
}

/// Results of background work, delivered back to `update`.
#[derive(Debug, Clone)]
pub enum DataEvent {
    Home {
        append: bool,
        result: Result<HomePage, String>,
    },
    Search {
        query: String,
        filter: SearchFilter,
        result: Result<Vec<Entry>, String>,
    },
    Library(Result<Vec<Entry>, String>),
    Collection {
        item: ItemRef,
        result: Result<Collection, String>,
    },
    Radio {
        seed: String,
        result: Result<Vec<Track>, String>,
    },
    Lyrics {
        video_id: String,
        result: Result<Option<String>, String>,
    },
    Rated {
        video_id: String,
        liked: bool,
        result: Result<(), String>,
    },
    Resolved {
        video_id: String,
        result: Result<StreamUrl, String>,
    },
    /// Songs in "Liked music", used to show the ♥ everywhere.
    Liked(Result<Vec<Track>, String>),
}

/// Everything that can change the app state.
#[derive(Debug, Clone)]
pub enum Msg {
    Action(Action),
    Data(DataEvent),
    Player(PlayerEvent),
    Tick,
}

/// Commands for the mpv process.
#[derive(Debug, Clone, PartialEq)]
pub enum MpvCmd {
    Load(String),
    TogglePause,
    Seek(f64),
    Volume(u8),
    Stop,
    Meter(bool),
    PollMeter,
}

/// Side effects requested by `update`, executed by the runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    LoadHome { continuation: Option<String> },
    Search { query: String, filter: SearchFilter },
    LoadLibrary,
    LoadLiked,
    LoadCollection(ItemRef),
    Radio { seed: String },
    Lyrics(String),
    Rate { video_id: String, liked: bool },
    AddHistory(String),
    Resolve(String),
    Mpv(MpvCmd),
}
