#!/usr/bin/env bash
# Builds dist/termtube-<arch>.AppImage on Linux for the host architecture.
# Bundles termtube, mpv (with its libraries), yt-dlp and deno (JS runtime used by yt-dlp).
# Needs: cargo, mpv installed on the host, curl, unzip, libfuse2 (or FUSE-less extraction).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64) YTDLP_ASSET="yt-dlp_linux" ;;
    aarch64) YTDLP_ASSET="yt-dlp_linux_aarch64" ;;
    *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

DIST="$ROOT/dist"
WORK="$ROOT/target/appimage-$ARCH"
APPDIR="$WORK/TermTube.AppDir"
OUTPUT="$DIST/termtube-$ARCH.AppImage"

command -v mpv >/dev/null || { echo "mpv must be installed on the build host" >&2; exit 1; }

if [[ -z "${TERMTUBE_BIN:-}" ]]; then
    cargo build --release --manifest-path "$ROOT/Cargo.toml"
    TERMTUBE_BIN="$ROOT/target/release/termtube"
fi

rm -rf "$WORK" "$OUTPUT"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$DIST"

install -m 755 "$TERMTUBE_BIN" "$APPDIR/usr/bin/termtube"
install -m 755 "$(command -v mpv)" "$APPDIR/usr/bin/mpv"

echo "Downloading yt-dlp ($YTDLP_ASSET)…"
curl -fsSL --retry 5 -o "$APPDIR/usr/bin/yt-dlp" "https://github.com/yt-dlp/yt-dlp/releases/latest/download/$YTDLP_ASSET"
chmod 755 "$APPDIR/usr/bin/yt-dlp"

echo "Downloading deno…"
curl -fsSL --retry 5 -o "$WORK/deno.zip" "https://github.com/denoland/deno/releases/latest/download/deno-$ARCH-unknown-linux-gnu.zip"
unzip -q -o "$WORK/deno.zip" -d "$APPDIR/usr/bin"
chmod 755 "$APPDIR/usr/bin/deno"

# Bundled tools come first in PATH so termtube finds mpv, yt-dlp and deno.
cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/termtube" "$@"
EOF
chmod 755 "$APPDIR/AppRun"

cat > "$APPDIR/usr/share/applications/termtube.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=TermTube
Comment=YouTube Music in your terminal
Exec=termtube
Icon=termtube
Terminal=true
Categories=Audio;AudioVideo;Player;
EOF
cp "$ROOT/logo.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/termtube.png"

echo "Downloading linuxdeploy…"
LINUXDEPLOY="$WORK/linuxdeploy-$ARCH.AppImage"
curl -fsSL --retry 5 -o "$LINUXDEPLOY" "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$ARCH.AppImage"
chmod 755 "$LINUXDEPLOY"

echo "Packaging AppImage…"
(
    cd "$WORK"
    LDAI_OUTPUT="$OUTPUT" "$LINUXDEPLOY" --appimage-extract-and-run \
        --appdir "$APPDIR" \
        --executable "$APPDIR/usr/bin/termtube" \
        --executable "$APPDIR/usr/bin/mpv" \
        --desktop-file "$APPDIR/usr/share/applications/termtube.desktop" \
        --output appimage
)

chmod 755 "$OUTPUT"
echo "Built $OUTPUT"
