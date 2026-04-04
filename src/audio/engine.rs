use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample};
use thiserror::Error;

const PLAYBACK_BUFFER_SECONDS: usize = 4;
const VISUALIZER_BUFFER_SECONDS: usize = 2;

#[derive(Debug, Error)]
pub enum AudioEngineError {
    #[error("no audio output device available")]
    NoOutputDevice,
    #[error("failed to get default output config: {0}")]
    DefaultConfig(#[from] cpal::DefaultStreamConfigError),
    #[error("failed to build output stream: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),
    #[error("failed to play output stream: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
    #[error("failed to spawn process: {0}")]
    Spawn(#[from] std::io::Error),
}

pub struct AudioEngine {
    playback_buffer: Arc<Mutex<VecDeque<f32>>>,
    visualizer_buffer: Arc<Mutex<VecDeque<f32>>>,
    stop_flag: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    decode_finished: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    stream: Option<cpal::Stream>,
    worker: Option<thread::JoinHandle<()>>,
    target_sample_rate: u32,
    target_channels: usize,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            playback_buffer: Arc::new(Mutex::new(VecDeque::new())),
            visualizer_buffer: Arc::new(Mutex::new(VecDeque::new())),
            stop_flag: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            decode_finished: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(AtomicU32::new(100)),
            stream: None,
            worker: None,
            target_sample_rate: 48_000,
            target_channels: 2,
        }
    }

    /// Set the playback volume as a percentage [0..=100].
    pub fn set_volume(&self, percent: u8) {
        self.volume
            .store(percent.min(100) as u32, Ordering::Relaxed);
    }

    /// Current playback volume percent.
    pub fn volume_percent(&self) -> u8 {
        self.volume.load(Ordering::Relaxed).min(100) as u8
    }

    pub fn play_url(&mut self, url: &str, cookies: Option<&Path>) -> Result<(), AudioEngineError> {
        self.stop();
        self.paused.store(false, Ordering::Relaxed);
        self.decode_finished.store(false, Ordering::Relaxed);

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioEngineError::NoOutputDevice)?;
        let default_config = device.default_output_config()?;

        self.target_sample_rate = default_config.sample_rate().0;
        self.target_channels = default_config.channels() as usize;

        let playback_buffer = Arc::clone(&self.playback_buffer);
        let stream_config: cpal::StreamConfig = default_config.clone().into();
        let paused_flag = Arc::clone(&self.paused);

        let stream = match default_config.sample_format() {
            SampleFormat::F32 => build_output_stream::<f32>(
                &device,
                &stream_config,
                playback_buffer,
                paused_flag,
                Arc::clone(&self.volume),
            ),
            SampleFormat::I16 => build_output_stream::<i16>(
                &device,
                &stream_config,
                playback_buffer,
                paused_flag,
                Arc::clone(&self.volume),
            ),
            SampleFormat::U16 => build_output_stream::<u16>(
                &device,
                &stream_config,
                playback_buffer,
                paused_flag,
                Arc::clone(&self.volume),
            ),
            _ => build_output_stream::<f32>(
                &device,
                &stream_config,
                playback_buffer,
                paused_flag,
                Arc::clone(&self.volume),
            ),
        }?;

        stream.play()?;
        self.stream = Some(stream);

        let stop_flag = Arc::clone(&self.stop_flag);
        stop_flag.store(false, Ordering::Relaxed);

        let url = url.to_string();
        let cookies = cookies.map(|p| p.to_path_buf());
        let playback_buffer = Arc::clone(&self.playback_buffer);
        let visualizer_buffer = Arc::clone(&self.visualizer_buffer);
        let target_sample_rate = self.target_sample_rate;
        let target_channels = self.target_channels;
        let decode_finished = Arc::clone(&self.decode_finished);

        let worker = thread::spawn(move || {
            if let Err(err) = decode_loop(
                &url,
                cookies.as_deref(),
                target_sample_rate,
                target_channels,
                playback_buffer,
                visualizer_buffer,
                stop_flag,
            ) {
                tracing::error!("audio decode loop error: {err}");
            }
            decode_finished.store(true, Ordering::Relaxed);
        });

        self.worker = Some(worker);
        Ok(())
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Returns true if there is an active worker thread (playing or paused).
    pub fn is_active(&self) -> bool {
        self.worker.is_some()
    }

    /// Returns true if decoding has finished (song ended) but playback buffer may still have data.
    pub fn is_decode_finished(&self) -> bool {
        self.decode_finished.load(Ordering::Relaxed)
    }

    /// Returns true if decode finished AND playback buffer is drained.
    pub fn is_song_finished(&self) -> bool {
        if !self.decode_finished.load(Ordering::Relaxed) {
            return false;
        }
        match self.playback_buffer.lock() {
            Ok(buf) => buf.is_empty(),
            Err(_) => true,
        }
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.stream = None;
        if let Ok(mut buffer) = self.playback_buffer.lock() {
            buffer.clear();
        }
        if let Ok(mut buffer) = self.visualizer_buffer.lock() {
            buffer.clear();
        }
    }

    pub fn take_visualizer_samples(&self, max_samples: usize) -> Vec<f32> {
        let mut buf = match self.visualizer_buffer.lock() {
            Ok(b) => b,
            Err(_) => return Vec::new(),
        };
        let take = max_samples.min(buf.len());
        let mut out = Vec::with_capacity(take);
        for _ in 0..take {
            if let Some(sample) = buf.pop_front() {
                out.push(sample);
            }
        }
        out
    }
}

fn build_output_stream<T: SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    playback_buffer: Arc<Mutex<VecDeque<f32>>>,
    paused: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let channels = config.channels as usize;

    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            if paused.load(Ordering::Relaxed) {
                // Output silence when paused
                for sample in data.iter_mut() {
                    *sample = T::from_sample(0.0f32);
                }
                return;
            }

            let volume_factor = volume.load(Ordering::Relaxed) as f32 / 100.0;
            let mut buffer = match playback_buffer.lock() {
                Ok(b) => b,
                Err(poisoned) => poisoned.into_inner(),
            };

            for frame in data.chunks_mut(channels) {
                for sample in frame.iter_mut() {
                    let value = buffer.pop_front().unwrap_or(0.0) * volume_factor;
                    *sample = T::from_sample(value);
                }
            }
        },
        |err| tracing::error!("audio output error: {err}"),
        None,
    )
}

fn decode_loop(
    url: &str,
    cookies: Option<&Path>,
    target_sample_rate: u32,
    target_channels: usize,
    playback_buffer: Arc<Mutex<VecDeque<f32>>>,
    visualizer_buffer: Arc<Mutex<VecDeque<f32>>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<(), std::io::Error> {
    let (mut ytdlp, mut ffmpeg) =
        spawn_pipeline(url, cookies, target_sample_rate, target_channels)?;

    // Capture ffmpeg stderr for diagnostics
    let ffmpeg_stderr = ffmpeg.stderr.take();
    let stderr_handle = ffmpeg_stderr.map(|se| {
        thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(se);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if !line.is_empty() {
                        tracing::error!("ffmpeg stderr: {line}");
                    }
                }
            }
        })
    });

    // Capture yt-dlp stderr for diagnostics
    let ytdlp_stderr = ytdlp.stderr.take();
    let ytdlp_stderr_handle = ytdlp_stderr.map(|se| {
        thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(se);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if !line.is_empty() {
                        tracing::error!("yt-dlp stderr: {line}");
                    }
                }
            }
        })
    });

    let mut stdout = ffmpeg.stdout.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "ffmpeg stdout not captured")
    })?;

    // Read raw PCM s16le data from ffmpeg
    let mut buf = [0u8; 8192];
    let mut total_samples: u64 = 0;
    let mut first_logged = false;
    let pb_max = target_sample_rate as usize * target_channels * PLAYBACK_BUFFER_SECONDS;
    let vis_max = target_sample_rate as usize * target_channels * VISUALIZER_BUFFER_SECONDS;
    // Threshold at which we pause reading to let playback catch up (75% full)
    let pb_high_water = pb_max * 3 / 4;

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // Backpressure: wait while the playback buffer is nearly full
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            let len = match playback_buffer.lock() {
                Ok(b) => b.len(),
                Err(p) => p.into_inner().len(),
            };
            if len < pb_high_water {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(20));
        }
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let n = match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };

        // Convert raw s16le bytes to f32 samples
        let sample_count = n / 2;
        let mut samples = Vec::with_capacity(sample_count);
        for i in (0..n - 1).step_by(2) {
            let s = i16::from_le_bytes([buf[i], buf[i + 1]]);
            samples.push(s as f32 / 32768.0);
        }

        total_samples += samples.len() as u64;
        if !first_logged && !samples.is_empty() {
            tracing::info!(
                "first audio data: {} PCM samples ({}Hz {}ch s16le via ffmpeg)",
                samples.len(),
                target_sample_rate,
                target_channels
            );
            first_logged = true;
        }

        push_samples(&playback_buffer, &samples, pb_max);
        push_samples(&visualizer_buffer, &samples, vis_max);
    }

    let _ = ffmpeg.kill();
    let _ = ffmpeg.wait();
    let _ = ytdlp.kill();
    let _ = ytdlp.wait();
    if let Some(h) = stderr_handle {
        let _ = h.join();
    }
    if let Some(h) = ytdlp_stderr_handle {
        let _ = h.join();
    }
    tracing::info!("decode loop finished: {} total samples", total_samples);
    Ok(())
}

/// Spawns yt-dlp piped into ffmpeg. ffmpeg decodes any audio format to raw PCM s16le.
fn spawn_pipeline(
    url: &str,
    cookies: Option<&Path>,
    sample_rate: u32,
    channels: usize,
) -> Result<(Child, Child), std::io::Error> {
    let mut ytdlp_cmd = Command::new("yt-dlp");
    ytdlp_cmd
        .arg("--no-warnings")
        .arg("--quiet")
        .arg("-f")
        .arg("bestaudio/best")
        .arg("-o")
        .arg("-");

    if let Some(path) = cookies {
        ytdlp_cmd.arg("--cookies").arg(path);
    }

    ytdlp_cmd
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut ytdlp = ytdlp_cmd.spawn()?;
    let ytdlp_stdout = ytdlp.stdout.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "yt-dlp stdout not captured")
    })?;

    let ffmpeg = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg("pipe:0")
        .arg("-f")
        .arg("s16le")
        .arg("-acodec")
        .arg("pcm_s16le")
        .arg("-ar")
        .arg(sample_rate.to_string())
        .arg("-ac")
        .arg(channels.to_string())
        .arg("pipe:1")
        .stdin(ytdlp_stdout)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    Ok((ytdlp, ffmpeg))
}

fn push_samples(buffer: &Arc<Mutex<VecDeque<f32>>>, samples: &[f32], max_len: usize) {
    let mut buffer = match buffer.lock() {
        Ok(b) => b,
        Err(poisoned) => poisoned.into_inner(),
    };

    for &sample in samples {
        if buffer.len() >= max_len {
            // For visualizer buffer this acts as a ring; for playback
            // buffer, backpressure in decode_loop prevents reaching here
            // under normal conditions.
            buffer.pop_front();
        }
        buffer.push_back(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_engine_default_volume() {
        let engine = AudioEngine::new();
        assert_eq!(engine.volume_percent(), 100);
    }

    #[test]
    fn test_audio_engine_set_volume_clamps() {
        let engine = AudioEngine::new();
        engine.set_volume(120);
        assert_eq!(engine.volume_percent(), 100);
        engine.set_volume(0);
        assert_eq!(engine.volume_percent(), 0);
        engine.set_volume(55);
        assert_eq!(engine.volume_percent(), 55);
    }
}
