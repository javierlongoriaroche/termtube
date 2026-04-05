#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ARCH="${TARGET_ARCH:-aarch64}"
BUILD_TRIPLE="aarch64-unknown-linux-gnu"
OUTPUT="$ROOT/termtube-aarch64.AppImage"
FFMPEG_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-arm64-static.tar.xz"
YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
BUILD_BINARY="$ROOT/target/$BUILD_TRIPLE/release/termtube"
APPDIR="$ROOT/packaging/TermTube-${TARGET_ARCH}.AppDir"
ARTIFACT_DIR="$ROOT/packaging/artifacts"

if [[ ! -f "$BUILD_BINARY" ]]; then
  echo "ARM64 binary not found at $BUILD_BINARY. Run packaging/build-arm64-docker.sh first." >&2
  exit 1
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

cp "$ROOT/logo.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/termtube.png"
cp "$ROOT/logo.png" "$APPDIR/termtube.png"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "Preparing cross-strip helper in temporary directory..."
docker run --rm --platform linux/amd64 -v "$TMPDIR":/out ghcr.io/rust-lang/rust:1.94-slim-bullseye /bin/sh -c 'apt-get update >/dev/null 2>&1 && apt-get install -y --no-install-recommends binutils-aarch64-linux-gnu >/dev/null 2>&1 && cp /usr/bin/aarch64-linux-gnu-strip /out/aarch64-linux-gnu-strip && cp /usr/lib/x86_64-linux-gnu/libbfd-2.35.2-arm64.so /out/libbfd-2.35.2-arm64.so && chmod +x /out/aarch64-linux-gnu-strip'
STRIP_BIN="$TMPDIR/aarch64-linux-gnu-strip"

if [[ ! -x "$STRIP_BIN" ]]; then
  echo "Failed to prepare aarch64 cross-strip helper at $STRIP_BIN" >&2
  exit 1
fi

cd "$TMPDIR"

echo "Downloading yt-dlp..."
if ! curl -L --fail --retry 5 --retry-delay 2 -o "$APPDIR/usr/bin/yt-dlp" "$YTDLP_URL"; then
  echo "GitHub download failed; installing yt-dlp via pip fallback..."
  python3 -m pip install --prefix "$TMPDIR/yt-dlp" yt-dlp
  cp "$TMPDIR/yt-dlp/bin/yt-dlp" "$APPDIR/usr/bin/yt-dlp"
fi
chmod +x "$APPDIR/usr/bin/yt-dlp"

echo "Downloading ffmpeg static build..."
curl -L --fail --retry 5 --retry-delay 2 -o "ffmpeg.tar.xz" "$FFMPEG_URL"

mkdir -p "extract"
tar -xJf "ffmpeg.tar.xz" -C "extract"
FFMPEG_BIN="$(find "extract" -type f -name ffmpeg -perm /u+x | head -n 1)"
if [[ -z "$FFMPEG_BIN" ]]; then
  echo "Failed to locate ffmpeg binary in the downloaded archive." >&2
  exit 1
fi
cp "$FFMPEG_BIN" "$APPDIR/usr/bin/ffmpeg"
chmod +x "$APPDIR/usr/bin/ffmpeg"

echo "Downloading linuxdeploy x86_64..."
curl -L --fail -o "linuxdeploy-x86_64.AppImage" "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
chmod +x "linuxdeploy-x86_64.AppImage"

echo "Extracting linuxdeploy..."
"$TMPDIR/linuxdeploy-x86_64.AppImage" --appimage-extract >/dev/null

if [[ ! -d "$TMPDIR/squashfs-root/usr/bin" ]]; then
  echo "Failed to extract linuxdeploy AppImage." >&2
  exit 1
fi

cp "$STRIP_BIN" "$TMPDIR/squashfs-root/usr/bin/strip"
chmod +x "$TMPDIR/squashfs-root/usr/bin/strip"

if [[ -f "$TMPDIR/libbfd-2.35.2-arm64.so" ]]; then
  mkdir -p "$TMPDIR/squashfs-root/usr/lib"
  cp "$TMPDIR/libbfd-2.35.2-arm64.so" "$TMPDIR/squashfs-root/usr/lib/"
  export LD_LIBRARY_PATH="$TMPDIR/squashfs-root/usr/lib:${LD_LIBRARY_PATH:-}"
fi

cd "$TMPDIR/squashfs-root"

echo "Deploying dependencies with linuxdeploy x86_64..."
./AppRun \
  --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/termtube" \
  --desktop-file "$APPDIR/termtube.desktop"

echo "Downloading ARM64 runtime for AppImage..."
curl -L --fail --retry 5 --retry-delay 2 -o "$TMPDIR/runtime-aarch64" "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-aarch64"

echo "Creating AppImage with appimagetool and ARM64 runtime..."
./plugins/linuxdeploy-plugin-appimage/usr/bin/appimagetool \
  --runtime-file "$TMPDIR/runtime-aarch64" \
  "$APPDIR" "$OUTPUT"

chmod +x "$OUTPUT"

echo "AppImage generated: $OUTPUT"
