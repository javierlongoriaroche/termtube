#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ARCH="${TARGET_ARCH:-x86_64}"
YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"

case "$TARGET_ARCH" in
  x86_64)
    BUILD_TRIPLE="x86_64-unknown-linux-gnu"
    OUTPUT="$ROOT/termtube-x86_64.AppImage"
    FFMPEG_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"
    LINUXDEPLOY_URL="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
    ;;
  aarch64)
    BUILD_TRIPLE="aarch64-unknown-linux-gnu"
    OUTPUT="$ROOT/termtube-aarch64.AppImage"
    FFMPEG_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-arm64-static.tar.xz"
    LINUXDEPLOY_URL="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-aarch64.AppImage"
    ;;
  *)
    echo "Unsupported TARGET_ARCH: $TARGET_ARCH" >&2
    exit 1
    ;;
esac

BUILD_BINARY="$ROOT/target/$BUILD_TRIPLE/release/termtube"
APPDIR="$ROOT/packaging/TermTube-${TARGET_ARCH}.AppDir"
ARTIFACT_DIR="$ROOT/packaging/artifacts"

if [[ ! -f "$BUILD_BINARY" ]]; then
  echo "Building TermTube release binary for target $BUILD_TRIPLE..."
  rustup target add "$BUILD_TRIPLE"
  cargo build --release --target "$BUILD_TRIPLE"
fi

rm -rf "$APPDIR"
rm -rf "$ARTIFACT_DIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$ARTIFACT_DIR"

echo "Copying TermTube binary..."
cp "$BUILD_BINARY" "$APPDIR/usr/bin/termtube"
chmod +x "$APPDIR/usr/bin/termtube"

cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/termtube" "$@"
EOF
chmod +x "$APPDIR/AppRun"

cat > "$APPDIR/termtube.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=TermTube
Exec=termtube
Icon=termtube
Terminal=true
Categories=Audio;AudioVideo;
EOF

base64 -d > "$APPDIR/usr/share/icons/hicolor/256x256/apps/termtube.png" <<'EOF'
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAAWgmWQ0AAAAA
SUVORK5CYII=
EOF

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading yt-dlp..."
curl -L --fail -o "$APPDIR/usr/bin/yt-dlp" "$YTDLP_URL"
chmod +x "$APPDIR/usr/bin/yt-dlp"

echo "Downloading ffmpeg static build..."
curl -L --fail -o "$TMPDIR/ffmpeg.tar.xz" "$FFMPEG_URL"

echo "Extracting ffmpeg..."
mkdir -p "$TMPDIR/extract"
tar -xJf "$TMPDIR/ffmpeg.tar.xz" -C "$TMPDIR/extract"
FFMPEG_BIN="$(find "$TMPDIR/extract" -type f -name ffmpeg -perm /u+x | head -n 1)"
if [[ -z "$FFMPEG_BIN" ]]; then
  echo "Failed to locate ffmpeg binary in the downloaded archive." >&2
  exit 1
fi
cp "$FFMPEG_BIN" "$APPDIR/usr/bin/ffmpeg"
chmod +x "$APPDIR/usr/bin/ffmpeg"

echo "Downloading linuxdeploy..."
curl -L --fail -o "$TMPDIR/linuxdeploy-${TARGET_ARCH}.AppImage" "$LINUXDEPLOY_URL"
chmod +x "$TMPDIR/linuxdeploy-${TARGET_ARCH}.AppImage"

cd "$ARTIFACT_DIR"

echo "Packaging AppImage..."
"$TMPDIR/linuxdeploy-${TARGET_ARCH}.AppImage" \
  --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/termtube" \
  --desktop-file "$APPDIR/termtube.desktop" \
  --output appimage

APPIMAGE_FILE="$(find "$ARTIFACT_DIR" -maxdepth 1 -name '*.AppImage' | head -n 1)"
if [[ -z "$APPIMAGE_FILE" ]]; then
  echo "AppImage creation failed: no .AppImage file found." >&2
  exit 1
fi
mv "$APPIMAGE_FILE" "$OUTPUT"
chmod +x "$OUTPUT"

echo "AppImage generated: $OUTPUT"
