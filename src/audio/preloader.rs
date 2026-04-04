use std::collections::{hash_map::DefaultHasher, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PreloaderError {
    #[error("failed to create cache dir: {0}")]
    CacheDir(std::io::Error),
    #[error("yt-dlp preload failed: {0}")]
    Spawn(std::io::Error),
}

enum PreloadCommand {
    Preload { url: String },
    Stop,
}

pub struct Preloader {
    tx: Sender<PreloadCommand>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Preloader {
    pub fn new(
        cache_dir: PathBuf,
        cookies: Option<PathBuf>,
        preload_count: usize,
    ) -> Result<Self, PreloaderError> {
        fs::create_dir_all(&cache_dir).map_err(PreloaderError::CacheDir)?;
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            let mut cached: VecDeque<PathBuf> = VecDeque::new();

            while let Ok(cmd) = rx.recv() {
                match cmd {
                    PreloadCommand::Preload { url } => {
                        if let Some(path) = make_cache_path(&cache_dir, &url) {
                            if !path.exists() {
                                if let Err(err) = download_to_path(&url, &path, cookies.as_deref())
                                {
                                    tracing::warn!("preload failed: {err}");
                                }
                            }
                            cached.push_back(path);
                            while cached.len() > preload_count {
                                if let Some(old) = cached.pop_front() {
                                    let _ = fs::remove_file(old);
                                }
                            }
                        }
                    }
                    PreloadCommand::Stop => break,
                }
            }
        });

        Ok(Self {
            tx,
            handle: Some(handle),
        })
    }

    pub fn enqueue(&self, url: &str) {
        let _ = self.tx.send(PreloadCommand::Preload {
            url: url.to_string(),
        });
    }

    pub fn stop(&mut self) {
        let _ = self.tx.send(PreloadCommand::Stop);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn make_cache_path(cache_dir: &Path, url: &str) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();
    Some(cache_dir.join(format!("{hash:x}.opus")))
}

fn download_to_path(url: &str, path: &Path, cookies: Option<&Path>) -> Result<(), PreloaderError> {
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--no-warnings")
        .arg("--quiet")
        .arg("-f")
        .arg("bestaudio")
        .arg("-o")
        .arg(path)
        .arg(url);

    if let Some(cookies) = cookies {
        cmd.arg("--cookies").arg(cookies);
    }

    let status = cmd.status().map_err(PreloaderError::Spawn)?;
    if !status.success() {
        tracing::warn!("yt-dlp preload exited with status {status}");
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("failed to create download directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to spawn yt-dlp: {0}")]
    YtdlpSpawn(std::io::Error),
}

/// Download a single song URL to the target directory in a background process.
pub fn download_song_to_dir(
    url: &str,
    target_dir: &Path,
    cookies: Option<&Path>,
) -> Result<PathBuf, DownloadError> {
    fs::create_dir_all(target_dir)?;

    let output_pattern = target_dir.join("%(title)s-%(id)s.%(ext)s");
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--no-warnings")
        .arg("--quiet")
        .arg("-f")
        .arg("bestaudio")
        .arg("-o")
        .arg(output_pattern.as_os_str())
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(cookies) = cookies {
        cmd.arg("--cookies").arg(cookies);
    }

    let _child = cmd.spawn().map_err(DownloadError::YtdlpSpawn)?;
    Ok(output_pattern)
}

/// Download multiple song URLs to the target directory in background processes.
pub fn download_playlist_to_dir(
    urls: &[String],
    target_dir: &Path,
    cookies: Option<&Path>,
) -> Result<Vec<PathBuf>, DownloadError> {
    fs::create_dir_all(target_dir)?;
    let mut paths = Vec::with_capacity(urls.len());

    for url in urls {
        let path = download_song_to_dir(url, target_dir, cookies)?;
        paths.push(path);
    }

    Ok(paths)
}
