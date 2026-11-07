use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use super::meter::{parse_levels, Levels, METER_FILTER};

/// Request id reserved for meter polls, so replies can be told apart.
/// mpv stores ids as signed 64-bit integers, so it must stay well below `i64::MAX`.
const METER_REQUEST_ID: u64 = 1_000_000_000;
/// Properties mirrored into the app through `property-change` events.
const OBSERVED: [&str; 5] = ["time-pos", "duration", "pause", "volume", "idle-active"];

#[derive(Debug, Error)]
pub enum MpvError {
    #[error("could not start mpv ({0}); is it installed? `brew install mpv`")]
    Spawn(std::io::Error),
    #[error("could not connect to mpv IPC socket: {0}")]
    Connect(std::io::Error),
}

/// Why mpv stopped playing a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndReason {
    Eof,
    Error(String),
    Stopped,
}

/// Player state changes reported by mpv.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    TimePos(f64),
    Duration(f64),
    Paused(bool),
    Volume(f64),
    Idle(bool),
    FileLoaded,
    EndFile(EndReason),
    Meter(Levels),
    Exited,
}

/// Options used to launch mpv.
#[derive(Debug, Clone)]
pub struct MpvConfig {
    pub binary: String,
    pub ytdlp: String,
    pub socket: PathBuf,
    pub cookies: PathBuf,
    pub volume: u8,
    pub meter: bool,
    pub extra_args: Vec<String>,
}

/// Handle to a running mpv process controlled over JSON IPC.
pub struct Mpv {
    cmd_tx: mpsc::UnboundedSender<(Vec<Value>, Option<u64>)>,
    _child: Child,
}

impl Mpv {
    /// Starts mpv in idle mode and forwards its events to `events`.
    pub async fn spawn(
        cfg: &MpvConfig,
        events: mpsc::UnboundedSender<PlayerEvent>,
    ) -> Result<Self, MpvError> {
        let _ = std::fs::remove_file(&cfg.socket);
        let child = Command::new(&cfg.binary)
            .args(launch_args(cfg))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(MpvError::Spawn)?;
        let stream = connect(&cfg.socket).await.map_err(MpvError::Connect)?;
        let (read, mut write) = stream.into_split();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<(Vec<Value>, Option<u64>)>();

        tokio::spawn(async move {
            let mut next_id = 1u64;
            while let Some((args, id)) = cmd_rx.recv().await {
                let id = id.unwrap_or_else(|| {
                    next_id = next_id % (METER_REQUEST_ID - 1) + 1;
                    next_id
                });
                if write
                    .write_all(encode_command(&args, id).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(event) = parse_message(&line) {
                    if events.send(event).is_err() {
                        return;
                    }
                }
            }
            let _ = events.send(PlayerEvent::Exited);
        });

        let mpv = Self {
            cmd_tx,
            _child: child,
        };
        for (i, name) in OBSERVED.iter().enumerate() {
            mpv.send(vec![json!("observe_property"), json!(i + 1), json!(name)]);
        }
        Ok(mpv)
    }

    fn send(&self, args: Vec<Value>) {
        let _ = self.cmd_tx.send((args, None));
    }

    /// Replaces the current file and starts playing it.
    pub fn load(&self, url: &str) {
        self.send(vec![json!("loadfile"), json!(url), json!("replace")]);
        self.set_paused(false);
    }

    pub fn set_paused(&self, paused: bool) {
        self.send(vec![json!("set_property"), json!("pause"), json!(paused)]);
    }

    pub fn toggle_pause(&self) {
        self.send(vec![json!("cycle"), json!("pause")]);
    }

    pub fn seek_relative(&self, seconds: f64) {
        self.send(vec![json!("seek"), json!(seconds), json!("relative")]);
    }

    pub fn set_volume(&self, volume: u8) {
        self.send(vec![json!("set_property"), json!("volume"), json!(volume)]);
    }

    pub fn stop(&self) {
        self.send(vec![json!("stop")]);
    }

    /// Adds or removes the level meter filter at runtime.
    pub fn set_meter(&self, enabled: bool) {
        if enabled {
            self.send(vec![json!("af"), json!("add"), json!(METER_FILTER)]);
        } else {
            self.send(vec![json!("af"), json!("remove"), json!("@meter")]);
        }
    }

    /// Requests current meter levels; the reply arrives as `PlayerEvent::Meter`.
    pub fn poll_meter(&self) {
        let _ = self.cmd_tx.send((
            vec![json!("get_property"), json!("af-metadata/meter")],
            Some(METER_REQUEST_ID),
        ));
    }

    pub fn quit(&self) {
        self.send(vec![json!("quit")]);
    }
}

/// Command line used to start mpv as an audio-only IPC server.
pub fn launch_args(cfg: &MpvConfig) -> Vec<String> {
    let mut args = vec![
        "--idle=yes".to_string(),
        "--no-video".into(),
        "--no-terminal".into(),
        "--no-config".into(),
        "--force-window=no".into(),
        "--gapless-audio=weak".into(),
        "--cache=yes".into(),
        format!("--volume={}", cfg.volume),
        format!("--input-ipc-server={}", cfg.socket.display()),
        format!("--script-opts=ytdl_hook-ytdl_path={}", cfg.ytdlp),
        "--ytdl-format=bestaudio/best".into(),
        format!("--ytdl-raw-options=cookies={}", cfg.cookies.display()),
    ];
    if cfg.meter {
        args.push(format!("--af={METER_FILTER}"));
    }
    args.extend(cfg.extra_args.iter().cloned());
    args
}

/// Serialises an IPC command as a single JSON line.
pub fn encode_command(args: &[Value], request_id: u64) -> String {
    format!("{}\n", json!({ "command": args, "request_id": request_id }))
}

/// Maps one IPC line (event or reply) to a `PlayerEvent`, if relevant.
pub fn parse_message(line: &str) -> Option<PlayerEvent> {
    let msg: Value = serde_json::from_str(line).ok()?;
    if msg.get("request_id").and_then(Value::as_u64) == Some(METER_REQUEST_ID) {
        return msg
            .get("data")
            .and_then(parse_levels)
            .map(PlayerEvent::Meter);
    }
    match msg.get("event")?.as_str()? {
        "property-change" => {
            let data = msg.get("data")?;
            match msg.get("name")?.as_str()? {
                "time-pos" => data.as_f64().map(PlayerEvent::TimePos),
                "duration" => data.as_f64().map(PlayerEvent::Duration),
                "pause" => data.as_bool().map(PlayerEvent::Paused),
                "volume" => data.as_f64().map(PlayerEvent::Volume),
                "idle-active" => data.as_bool().map(PlayerEvent::Idle),
                _ => None,
            }
        }
        "file-loaded" => Some(PlayerEvent::FileLoaded),
        "end-file" => Some(PlayerEvent::EndFile(
            match msg.get("reason").and_then(Value::as_str) {
                Some("eof") => EndReason::Eof,
                Some("error") => EndReason::Error(
                    msg.get("file_error")
                        .and_then(Value::as_str)
                        .unwrap_or("playback error")
                        .to_string(),
                ),
                _ => EndReason::Stopped,
            },
        )),
        _ => None,
    }
}

/// Waits for mpv to create its socket and connects to it.
async fn connect(socket: &Path) -> std::io::Result<UnixStream> {
    let mut last_err = None;
    for _ in 0..50 {
        match UnixStream::connect(socket).await {
            Ok(s) => return Ok(s),
            Err(e) => last_err = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::other("timeout")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(meter: bool) -> MpvConfig {
        MpvConfig {
            binary: "mpv".into(),
            ytdlp: "yt-dlp".into(),
            socket: PathBuf::from("/tmp/x.sock"),
            cookies: PathBuf::from("/tmp/c.txt"),
            volume: 50,
            meter,
            extra_args: vec!["--foo".into()],
        }
    }

    #[test]
    fn test_launch_args_include_ipc_cookies_and_optional_meter() {
        let args = launch_args(&cfg(true));
        assert!(args.contains(&"--input-ipc-server=/tmp/x.sock".to_string()));
        assert!(args.contains(&"--ytdl-raw-options=cookies=/tmp/c.txt".to_string()));
        assert!(args.contains(&"--volume=50".to_string()));
        assert!(args.iter().any(|a| a.starts_with("--af=@meter")));
        assert_eq!(args.last().unwrap(), "--foo");
        assert!(!launch_args(&cfg(false))
            .iter()
            .any(|a| a.starts_with("--af=")));
    }

    #[test]
    fn test_encode_command_is_json_line() {
        let line = encode_command(&[json!("seek"), json!(5.0), json!("relative")], 7);
        assert!(line.ends_with('\n'));
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["request_id"], 7);
        assert_eq!(v["command"][0], "seek");
    }

    #[test]
    fn test_parse_property_changes() {
        let p = |l: &str| parse_message(l);
        assert_eq!(
            p(r#"{"event":"property-change","id":1,"name":"time-pos","data":12.5}"#),
            Some(PlayerEvent::TimePos(12.5))
        );
        assert_eq!(
            p(r#"{"event":"property-change","id":3,"name":"pause","data":true}"#),
            Some(PlayerEvent::Paused(true))
        );
        assert_eq!(
            p(r#"{"event":"property-change","id":5,"name":"idle-active","data":false}"#),
            Some(PlayerEvent::Idle(false))
        );
        assert_eq!(
            p(r#"{"event":"property-change","id":1,"name":"time-pos"}"#),
            None
        );
    }

    #[test]
    fn test_parse_end_file_reasons() {
        assert_eq!(
            parse_message(r#"{"event":"end-file","reason":"eof"}"#),
            Some(PlayerEvent::EndFile(EndReason::Eof))
        );
        assert_eq!(
            parse_message(r#"{"event":"end-file","reason":"error","file_error":"loading failed"}"#),
            Some(PlayerEvent::EndFile(EndReason::Error(
                "loading failed".into()
            )))
        );
        assert_eq!(
            parse_message(r#"{"event":"end-file","reason":"stop"}"#),
            Some(PlayerEvent::EndFile(EndReason::Stopped))
        );
    }

    #[test]
    fn test_parse_meter_reply_and_ignores_other_replies() {
        let line = format!(
            r#"{{"request_id":{METER_REQUEST_ID},"error":"success","data":{{"lavfi.astats.1.RMS_level":"-30"}}}}"#
        );
        assert!(matches!(parse_message(&line), Some(PlayerEvent::Meter(_))));
        assert_eq!(parse_message(r#"{"request_id":3,"error":"success"}"#), None);
        assert_eq!(parse_message("not json"), None);
    }

    /// Commands written by the handle reach a fake mpv socket in order.
    #[tokio::test]
    async fn test_commands_reach_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("fake.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<(Vec<Value>, Option<u64>)>();
        let writer = tokio::spawn({
            let socket = socket.clone();
            async move {
                let mut stream = connect(&socket).await.unwrap();
                while let Some((args, id)) = cmd_rx.recv().await {
                    stream
                        .write_all(encode_command(&args, id.unwrap_or(1)).as_bytes())
                        .await
                        .unwrap();
                }
            }
        });
        let (server, _) = listener.accept().await.unwrap();
        cmd_tx
            .send((vec![json!("cycle"), json!("pause")], None))
            .unwrap();
        drop(cmd_tx);
        writer.await.unwrap();
        let mut lines = BufReader::new(server).lines();
        let first: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(first["command"], json!(["cycle", "pause"]));
    }
}
