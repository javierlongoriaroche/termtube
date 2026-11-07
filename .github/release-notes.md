YouTube Music in your terminal: Home feed, search, library, albums, artists, radio, likes and lyrics, using ~50 MB of RAM. Data comes from YouTube Music's internal API and audio is played by mpv.

## Downloads

| File | Platform |
|------|----------|
| `termtube-x86_64.AppImage` | Linux x86_64 (includes mpv, yt-dlp and deno) |
| `termtube-aarch64.AppImage` | Linux ARM64 (includes mpv, yt-dlp and deno) |
| `termtube-macos-universal.dmg` | macOS 12+ on Apple Silicon and Intel |

## Linux

```bash
chmod +x termtube-x86_64.AppImage
./termtube-x86_64.AppImage login     # sign in to music.youtube.com in your browser first
./termtube-x86_64.AppImage
```

If FUSE is not available, run it with `--appimage-extract-and-run`. Use `login --browser firefox` (or `chrome`, `chromium`, `brave`…) to choose the browser to read the session from.

## macOS

```bash
brew install mpv yt-dlp
```

Open the `.dmg`, drag **TermTube** to Applications and open it (the first time: right-click → Open, since the app is not notarized). It opens a Terminal window; run `termtube login` once from a terminal to import your session (`/Applications/TermTube.app/Contents/Resources/termtube login`).

Press `?` inside the app for the key bindings.
