# TermTube 🎵

YouTube Music in your terminal. Browse your Home feed, search, open albums, artists and your playlists, play with radio "up next", like songs and read lyrics — using ~50 MB of RAM instead of a full browser.

- **Data**: YouTube Music's internal API (InnerTube) through [`ytmapi-rs`](https://docs.rs/ytmapi-rs), authenticated with your browser session.
- **Audio**: [mpv](https://mpv.io) controlled over JSON IPC (seeking, gapless, media keys, macOS Now Playing).
- **Streams**: [yt-dlp](https://github.com/yt-dlp/yt-dlp) resolves the next song in the background so it starts instantly.

## Features

- Home feed (Quick picks, mixes, listen again…) with pagination
- Search by songs, albums, artists or playlists
- Library: your playlists and *Liked music*
- Album, playlist and artist pages
- Radio: playing a single song keeps the queue going with similar music
- Queue panel: reorder, remove, play next, shuffle, repeat
- Like/unlike synced with your account (`f`), plays recorded in your history
- Lyrics panel (`l`)
- Stereo level meter in the player bar (toggle with `v` or in the config)

## Requirements

```bash
brew install mpv yt-dlp      # macOS (Linux: use your package manager)
brew install rustup && rustup default stable   # to build
```

## Install

Download the latest [release](https://github.com/javierlongoriaroche/termtube/releases/latest):

- **Linux**: `termtube-x86_64.AppImage` or `termtube-aarch64.AppImage` — self-contained (mpv, yt-dlp and deno included). `chmod +x` and run.
- **macOS**: `termtube-macos-universal.dmg` (Apple Silicon and Intel) — needs `brew install mpv yt-dlp`.

Or build from source:

```bash
cargo install --path .
```

## Log in

Sign in to https://music.youtube.com in your browser first, then:

```bash
termtube login                    # reads cookies from Brave by default
termtube login --browser chrome   # or firefox, safari, edge, ...
```

Only `youtube.com` cookies are kept, in `~/.termtube/cookies.txt` (mode 600). If several Google accounts are signed in, you'll be asked which one to use (or pass `--account N`). Run it again whenever the session expires.

## Usage

```bash
termtube
```

| Key | Action |
|-----|--------|
| `1` `2` `3` | Home / Search / Library |
| `/` | Search (type, `Tab` changes type, `Enter` searches) |
| `j` `k` `↑` `↓`, `g` `G`, `PgUp` `PgDn` | Move |
| `Enter` | Play song / open album, playlist or artist |
| `Esc` `Backspace` | Back |
| `Space` | Play / pause |
| `n` `p` | Next / previous |
| `←` `→` | Seek −/+ 5 s |
| `+` `-` | Volume |
| `s` `r` | Shuffle / repeat (off → all → one) |
| `f` | Like / unlike the current song |
| `a` `A` | Add to queue / play next |
| `R` | Start radio from the selected or current song |
| `Q` `l` | Queue panel / lyrics panel |
| `Tab` | Switch focus between list and queue |
| `K` `J` `d` | In the queue: move up / down / remove |
| `v` | Level meter on/off |
| `Ctrl-r` | Reload the page |
| `?` | Help |
| `q` `Ctrl-c` | Quit |

## Configuration

`~/.config/termtube/config.toml` — see [`config/default.toml`](config/default.toml). Logs go to `~/.termtube/termtube.log` (`RUST_LOG=termtube=debug` for more detail).

## Releasing

Pushing a tag like `v0.2.0` runs `.github/workflows/release.yml`, which tests the code, builds both AppImages (`scripts/build-appimage.sh` on native x86_64 and ARM64 runners) and the universal macOS `.dmg` (`scripts/build-macos-app.sh`), and publishes them in a GitHub release. Pull requests run the same builds without publishing.

## Development

```bash
cargo test                       # unit tests (no network)
cargo clippy --all-targets -- -D warnings
# Live API checks against your account:
TERMTUBE_COOKIES=~/.termtube/cookies.txt TERMTUBE_AUTHUSER=0 cargo test -- --ignored --nocapture
```

Architecture:

```
src/
├── main.rs            CLI (run, login), logging
├── login.rs           browser cookie import and account selection
├── app/
│   ├── state.rs       App state: sections, navigation stacks, player mirror
│   ├── msg.rs         Action / DataEvent / Effect types
│   ├── update.rs      pure state transitions → effects (unit tested)
│   ├── keymap.rs      key bindings
│   └── run.rs         async event loop, effect execution
├── ytm/               YouTube Music API client, models, raw InnerTube parsers
├── player/            mpv IPC client, yt-dlp stream resolver, level meter
├── queue.rs           play queue with shuffle/repeat/radio refill
└── ui/                ratatui rendering
```

## License

MIT
