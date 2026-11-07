use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use thiserror::Error;
use tokio::process::Command;

/// Stream URLs closer than this to expiring are resolved again.
const EXPIRY_MARGIN_SECS: u64 = 300;
/// yt-dlp runs per video before giving up when YouTube rejects the stream URL.
const MAX_ATTEMPTS: usize = 3;
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("could not run yt-dlp: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("yt-dlp failed: {0}")]
    Failed(String),
    #[error("YouTube rejected the audio stream (HTTP {0})")]
    Rejected(u16),
}

/// A direct audio stream URL and when it stops being valid (unix seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamUrl {
    pub url: String,
    pub expires_at: Option<u64>,
}

impl StreamUrl {
    pub fn is_fresh(&self, now: u64) -> bool {
        self.expires_at
            .is_none_or(|exp| exp > now + EXPIRY_MARGIN_SECS)
    }
}

/// Resolves watch URLs into direct audio URLs with yt-dlp, caching results.
#[derive(Debug, Clone)]
pub struct Resolver {
    ytdlp: String,
    cookies: PathBuf,
    cache: HashMap<String, StreamUrl>,
    http: reqwest::Client,
}

impl Resolver {
    pub fn new(ytdlp: String, cookies: PathBuf) -> Self {
        Self {
            ytdlp,
            cookies,
            cache: HashMap::new(),
            http: reqwest::Client::builder()
                .timeout(PROBE_TIMEOUT)
                .build()
                .unwrap_or_default(),
        }
    }

    /// Cached URL for a video, if still fresh.
    pub fn cached(&self, video_id: &str) -> Option<&StreamUrl> {
        self.cache.get(video_id).filter(|u| u.is_fresh(now_secs()))
    }

    pub fn store(&mut self, video_id: String, url: StreamUrl) {
        self.cache.insert(video_id, url);
    }

    /// Drops a cached URL so the next play resolves it again.
    pub fn forget(&mut self, video_id: &str) {
        self.cache.remove(video_id);
    }

    /// Resolves a playable URL: runs yt-dlp (≈5 s) and checks that YouTube serves it,
    /// retrying because some URLs are randomly rejected with HTTP 403.
    pub fn resolve_task(
        &self,
        video_id: &str,
    ) -> impl std::future::Future<Output = Result<StreamUrl, ResolveError>> {
        let ytdlp = self.ytdlp.clone();
        let cookies = self.cookies.clone();
        let http = self.http.clone();
        let video_id = video_id.to_string();
        async move {
            let mut last_err = ResolveError::Failed("no attempts".into());
            for attempt in 1..=MAX_ATTEMPTS {
                let stream = match run_ytdlp(&ytdlp, &cookies, &video_id).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("resolve {video_id} attempt {attempt}: {e}");
                        last_err = e;
                        continue;
                    }
                };
                match probe(&http, &stream.url).await {
                    Ok(()) => return Ok(stream),
                    Err(e) => {
                        tracing::warn!("resolve {video_id} attempt {attempt}: {e}");
                        last_err = e;
                    }
                }
            }
            Err(last_err)
        }
    }
}

async fn run_ytdlp(
    ytdlp: &str,
    cookies: &PathBuf,
    video_id: &str,
) -> Result<StreamUrl, ResolveError> {
    let out = Command::new(ytdlp)
        .args([
            "-q",
            "--no-warnings",
            "--no-playlist",
            "-f",
            "bestaudio/best",
            "-g",
            "--cookies",
        ])
        .arg(cookies)
        .arg(format!("https://music.youtube.com/watch?v={video_id}"))
        .kill_on_drop(true)
        .output()
        .await?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(ResolveError::Failed(
            err.lines().last().unwrap_or("unknown error").to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let url = stdout
        .lines()
        .find(|l| l.starts_with("http"))
        .ok_or_else(|| ResolveError::Failed("no URL in output".into()))?;
    Ok(StreamUrl {
        url: url.to_string(),
        expires_at: parse_expire(url),
    })
}

/// Requests the first bytes of the stream to detect URLs YouTube will refuse.
async fn probe(http: &reqwest::Client, url: &str) -> Result<(), ResolveError> {
    let status = http
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-1")
        .send()
        .await
        .map_err(|e| ResolveError::Failed(format!("probe failed: {e}")))?
        .status();
    if status.is_success() {
        Ok(())
    } else {
        Err(ResolveError::Rejected(status.as_u16()))
    }
}

/// Reads the `expire=<unix>` query parameter from a googlevideo URL.
pub fn parse_expire(url: &str) -> Option<u64> {
    let query = url.split_once('?')?.1;
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("expire="))
        .and_then(|v| v.parse().ok())
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_expire() {
        assert_eq!(
            parse_expire("https://host/videoplayback?a=1&expire=1700000000&b=2"),
            Some(1700000000)
        );
        assert_eq!(parse_expire("https://host/videoplayback?a=1"), None);
        assert_eq!(parse_expire("https://host/no-query"), None);
    }

    #[test]
    fn test_freshness_uses_margin() {
        let now = 1_000_000;
        assert!(StreamUrl {
            url: String::new(),
            expires_at: Some(now + 3600)
        }
        .is_fresh(now));
        assert!(!StreamUrl {
            url: String::new(),
            expires_at: Some(now + 60)
        }
        .is_fresh(now));
        assert!(StreamUrl {
            url: String::new(),
            expires_at: None
        }
        .is_fresh(now));
    }

    #[test]
    fn test_cache_returns_only_fresh_entries() {
        let mut r = Resolver::new("yt-dlp".into(), PathBuf::from("/tmp/c"));
        r.store(
            "fresh".into(),
            StreamUrl {
                url: "u1".into(),
                expires_at: Some(now_secs() + 7200),
            },
        );
        r.store(
            "stale".into(),
            StreamUrl {
                url: "u2".into(),
                expires_at: Some(now_secs()),
            },
        );
        assert_eq!(r.cached("fresh").map(|u| u.url.as_str()), Some("u1"));
        assert!(r.cached("stale").is_none());
        assert!(r.cached("missing").is_none());
    }
}

/// Resolves `TERMTUBE_VIDEO` with real yt-dlp and network; needs `TERMTUBE_COOKIES`.
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore = "runs yt-dlp against YouTube"]
    async fn live_resolve_from_env() {
        let video = std::env::var("TERMTUBE_VIDEO").expect("TERMTUBE_VIDEO");
        let cookies = PathBuf::from(std::env::var("TERMTUBE_COOKIES").expect("TERMTUBE_COOKIES"));
        let r = Resolver::new("yt-dlp".into(), cookies);
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        match r.resolve_task(&video).await {
            Ok(stream) => println!("resolved {video}: expires {:?}", stream.expires_at),
            Err(e) => panic!("could not resolve {video}: {e}"),
        }
    }
}
