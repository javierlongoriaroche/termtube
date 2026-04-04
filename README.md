# TermTube 🎵

A TUI YouTube music player with a real-time FFT audio visualizer, built in Rust.

![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange)
![License](https://img.shields.io/badge/license-MIT-blue)

## Features

- **YouTube Playlist Playback** — Stream music from YouTube playlists via yt-dlp + symphonia + cpal
- **Audio Visualizer** — Real-time FFT spectrum analyzer with configurable bars, decay, and gradient colors
- **Playlist Management** — Navigate multiple playlists, browse songs, cached metadata
- **Favorites** — Toggle favorites per song (♥), persistent across sessions
- **Queue** — Add songs to queue, reorder with Shift+K/J, remove with d
- **Customizable** — Keybindings, theme colors, visualizer settings via TOML config
- **Offline Cache** — Playlist metadata cached locally for instant startup
- **First-Run Wizard** — Interactive setup on first launch

## Prerequisites

- **Rust** 1.70+ (for building)
- **yt-dlp** — Required for fetching audio streams
- **ALSA dev libraries** (Linux): `sudo apt install libasound2-dev`

### Install yt-dlp

```bash
# pip
pip install yt-dlp

# or Homebrew
brew install yt-dlp

# or direct download
curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o ~/.local/bin/yt-dlp
chmod +x ~/.local/bin/yt-dlp
```

## Installation

```bash
git clone https://github.com/your-user/termtube.git
cd termtube
cargo build --release
# Binary at target/release/termtube
```

## Linux AppImage release

TermTube provides ready-to-run Linux AppImages that bundle `yt-dlp` and `ffmpeg`, so users can run the app without installing those dependencies separately.

### Available builds
- `termtube-x86_64.AppImage` — Linux x86_64
- `termtube-aarch64.AppImage` — Linux ARM64

### Run the AppImage

```bash
chmod +x termtube-x86_64.AppImage
./termtube-x86_64.AppImage --help
```

```bash
chmod +x termtube-aarch64.AppImage
./termtube-aarch64.AppImage --help
```

### Build locally from source

```bash
bash packaging/build-appimage.sh
```

### Build ARM64 locally

```bash
TARGET_ARCH=aarch64 bash packaging/build-appimage.sh
```

### Build ARM64 with Docker

If you don't have a native ARM64 machine, you can build the ARM64 AppImage from an x86_64 host using Docker and Buildx:

```bash
bash packaging/build-arm64-docker.sh
```

This will build an ARM64 container image, then run the AppImage packaging process inside that container.

## Quick Start

1. **Create a playlist file** (`playlist.txt`):

```
lofi-beats|https://www.youtube.com/playlist?list=PLxxxxxxx
synthwave|https://music.youtube.com/playlist?list=PLyyyyyyy
```

2. **(Optional) Export cookies** for private/age-restricted playlists:
   Use the "Get cookies.txt LOCALLY" browser extension, save as `cookies.txt`.

3. **Sync playlist metadata**:

```bash
termtube --sync
```

4. **Launch the player**:

```bash
termtube
```

## Usage

```
termtube [OPTIONS]

Options:
  --cookies <PATH>      Path to cookies.txt file
  --playlists <PATH>    Path to playlist.txt file
  --config <PATH>       Path to config.toml file
  --sync                Sync playlists from YouTube and exit
  -h, --help            Print help
  -V, --version         Print version
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Space` | Play / Pause |
| `n` | Next track |
| `p` | Previous track |
| `s` | Toggle shuffle |
| `r` | Cycle repeat (off → all → one) |
| `f` | Toggle favorite |
| `a` | Add selected song to queue |
| `+` / `-` | Volume up / down |
| `Tab` | Switch focus (playlists ↔ songs) |
| `j` / `k`, `↑` / `↓` | Navigate lists |
| `Enter` | Select / play |
| `Q` | Toggle queue view |
| `Shift+K` / `Shift+J` | Move queue item up / down |
| `d` | Remove from queue (in queue view) |
| `/` | Search |
| `?` | Toggle help |
| `q` / `Esc` | Quit |

## Configuration

On first run, TermTube creates a default config at `~/.config/termtube/config.toml`.

```toml
[general]
cache_dir = "~/.termtube/cache"
log_file = "~/.termtube/termtube.log"
preload_count = 2

[paths]
cookies = "./cookies.txt"
playlists = "./playlist.txt"

[theme]
name = "default"
primary = "#61afef"
secondary = "#c678dd"
visualizer_colors = ["#98c379", "#e5c07b", "#e06c75"]

[keybindings]
play_pause = "space"
next = "n"
previous = "p"
volume_up = "+"
volume_down = "-"
shuffle = "s"
repeat = "r"
favorite = "f"
quit = "q"
search = "/"
queue = "Q"

[visualizer]
bars = 24
fps = 30
decay = 0.85
```

## File Layout

```
~/.config/termtube/
  config.toml          # Main configuration

~/.termtube/
  cache/
    playlists/
      lofi-beats.json  # Cached playlist metadata
    playlists.json     # Playlist index
  favorites.json       # Favorite song IDs
  termtube.log         # Application log
```

## Architecture

```
src/
├── main.rs              # Entry point, CLI, TUI loop, first-run wizard
├── app.rs               # Application state (queue, favorites, playlists)
├── config/
│   ├── settings.rs      # TOML config parsing
│   ├── cookies.rs       # Netscape cookies.txt validation
│   └── playlist.rs      # playlist.txt parsing (name|url)
├── audio/
│   ├── engine.rs        # yt-dlp → symphonia → cpal audio pipeline
│   ├── queue.rs         # Playback queue with shuffle, repeat, reorder
│   └── preloader.rs     # Background preloading of next tracks
├── playlist/
│   ├── models.rs        # Song, Playlist, PlaylistIndex structs
│   ├── manager.rs       # Playlist sync & disk cache
│   └── favorites.rs     # Favorites persistence
├── visualizer/
│   └── spectrum.rs      # FFT spectrum analyzer (Hann window, log scale)
├── ui/
│   ├── layout.rs        # Panel layout (header, sidebar, main, controls)
│   ├── theme.rs         # Theme colors from config
│   ├── playlist_view.rs # Sidebar: playlists + songs with ♥ indicator
│   ├── now_playing.rs   # Now playing panel with progress bar
│   ├── visualizer_view.rs # Unicode bar visualizer (▁▂▃▄▅▆▇█)
│   ├── queue_view.rs    # Queue panel with reorder + key hints
│   └── controls.rs      # Bottom control bar
└── input/
    └── handler.rs       # Keybinding → Action mapping
```

## License

MIT
