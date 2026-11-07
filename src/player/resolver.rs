use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;
use tokio::process::Command;

/// Stream URLs closer than this to expiring are resolved again.
const EXPIRY_MARGIN_SECS: u64 = 300;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("could not run yt-dlp: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("yt-dlp failed: {0}")]
    Failed(String),
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
}

impl Resolver {
    pub fn new(ytdlp: String, cookies: PathBuf) -> Self {
        Self {
            ytdlp,
            cookies,
            cache: HashMap::new(),
        }
    }

    /// Cached URL for a video, if still fresh.
    pub fn cached(&self, video_id: &str) -> Option<&StreamUrl> {
        self.cache.get(video_id).filter(|u| u.is_fresh(now_secs()))
    }

    pub fn store(&mut self, video_id: String, url: StreamUrl) {
        self.cache.insert(video_id, url);
    }

    /// Runs yt-dlp for one video (≈5 s); independent of the cache so it can run in a task.
    pub fn resolve_task(
        &self,
        video_id: &str,
    ) -> impl std::future::Future<Output = Result<StreamUrl, ResolveError>> {
        let ytdlp = self.ytdlp.clone();
        let cookies = self.cookies.clone();
        let watch = format!("https://music.youtube.com/watch?v={video_id}");
        async move {
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
                .arg(watch)
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
