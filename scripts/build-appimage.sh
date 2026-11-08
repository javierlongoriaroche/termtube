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

# Downloads a GitHub release asset. With GH_TOKEN set (CI) it uses the authenticated
# API through `gh`, which avoids the timeouts GitHub applies to anonymous downloads.
# Usage: fetch_asset <owner/repo> <tag|latest> <asset name> <output file>
fetch_asset() {
    local repo="$1" tag="$2" asset="$3" out="$4" dir
    if [[ -n "${GH_TOKEN:-}" ]] && command -v gh >/dev/null; then
        dir="$(mktemp -d)"
        if [[ "$tag" == latest ]]; then
            gh release download --repo "$repo" --pattern "$asset" --dir "$dir"
        else
            gh release download "$tag" --repo "$repo" --pattern "$asset" --dir "$dir"
        fi
        mv "$dir/$asset" "$out"
        rmdir "$dir"
    else
        local url="https://github.com/$repo/releases/download/$tag/$asset"
        [[ "$tag" == latest ]] && url="https://github.com/$repo/releases/latest/download/$asset"
        curl -fsSL --retry 8 --retry-all-errors --retry-delay 5 -o "$out" "$url"
    fi
}

if [[ -z "${TERMTUBE_BIN:-}" ]]; then
    cargo build --release --manifest-path "$ROOT/Cargo.toml"
    TERMTUBE_BIN="$ROOT/target/release/termtube"
fi

rm -rf "$WORK" "$OUTPUT"
APP_ID="io.github.javierlongoriaroche.termtube"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/metainfo" "$DIST"

install -m 755 "$TERMTUBE_BIN" "$APPDIR/usr/bin/termtube"
install -m 755 "$(command -v mpv)" "$APPDIR/usr/bin/mpv"

echo "Downloading yt-dlp ($YTDLP_ASSET)…"
fetch_asset yt-dlp/yt-dlp latest "$YTDLP_ASSET" "$APPDIR/usr/bin/yt-dlp"
chmod 755 "$APPDIR/usr/bin/yt-dlp"

echo "Downloading deno…"
fetch_asset denoland/deno latest "deno-$ARCH-unknown-linux-gnu.zip" "$WORK/deno.zip"
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

cat > "$APPDIR/usr/share/applications/$APP_ID.desktop" <<'EOF'
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

# AppStream metadata, read by software centers and AppImageHub.
cat > "$APPDIR/usr/share/metainfo/$APP_ID.appdata.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<component type="console-application">
  <id>$APP_ID</id>
  <name>TermTube</name>
  <summary>Lightweight YouTube Music client for the terminal</summary>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>MIT</project_license>
  <developer id="io.github.javierlongoriaroche"><name>javierlongoriaroche</name></developer>
  <description>
    <p>TermTube plays YouTube Music from the terminal using about 50 MB of RAM: Home feed,
    search, your library and liked songs, radio, likes and lyrics, with audio played by mpv.</p>
  </description>
  <launchable type="desktop-id">$APP_ID.desktop</launchable>
  <url type="homepage">https://javierlongoriaroche.github.io/termtube/</url>
  <url type="bugtracker">https://github.com/javierlongoriaroche/termtube/issues</url>
  <screenshots>
    <screenshot type="default">
      <image>https://javierlongoriaroche.github.io/termtube/screenshot.png</image>
    </screenshot>
  </screenshots>
  <provides><binary>termtube</binary></provides>
  <categories><category>Audio</category><category>Player</category></categories>
  <content_rating type="oars-1.1"/>
  <releases><release version="$VERSION"/></releases>
</component>
EOF

echo "Downloading linuxdeploy…"
LINUXDEPLOY="$WORK/linuxdeploy-$ARCH.AppImage"
fetch_asset linuxdeploy/linuxdeploy continuous "linuxdeploy-$ARCH.AppImage" "$LINUXDEPLOY"
chmod 755 "$LINUXDEPLOY"

echo "Packaging AppImage…"
(
    cd "$WORK"
    LDAI_OUTPUT="$OUTPUT" "$LINUXDEPLOY" --appimage-extract-and-run \
        --appdir "$APPDIR" \
        --executable "$APPDIR/usr/bin/termtube" \
        --executable "$APPDIR/usr/bin/mpv" \
        --desktop-file "$APPDIR/usr/share/applications/$APP_ID.desktop" \
        --output appimage
)

chmod 755 "$OUTPUT"
echo "Built $OUTPUT"
