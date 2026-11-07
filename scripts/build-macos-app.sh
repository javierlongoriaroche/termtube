#!/usr/bin/env bash
# Builds dist/TermTube.app (opens Terminal running termtube) and dist/TermTube.dmg.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/TermTube.app"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"

cargo build --release --manifest-path "$ROOT/Cargo.toml"

rm -rf "$APP" "$DIST/TermTube.dmg" "$DIST/dmg"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/termtube" "$APP/Contents/Resources/termtube"

# Launcher: opens a Terminal window running the bundled binary, then exits.
cat > "$APP/Contents/MacOS/TermTube" <<'LAUNCHER'
#!/bin/bash
BIN="$(cd "$(dirname "$0")/../Resources" && pwd)/termtube"
/usr/bin/osascript <<OSA
tell application "Terminal"
    activate
    set w to do script "clear; '$BIN'; exit"
    set custom title of w to "TermTube"
end tell
OSA
LAUNCHER
chmod +x "$APP/Contents/MacOS/TermTube"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>TermTube</string>
    <key>CFBundleDisplayName</key><string>TermTube</string>
    <key>CFBundleIdentifier</key><string>com.termtube.app</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleExecutable</key><string>TermTube</string>
    <key>CFBundleIconFile</key><string>TermTube</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSAppleEventsUsageDescription</key><string>TermTube opens a Terminal window to run the player.</string>
</dict>
</plist>
PLIST

# Icon from logo.png (all sizes required by iconutil).
ICONSET="$DIST/TermTube.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    sips -z $size $size "$ROOT/logo.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$ROOT/logo.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/TermTube.icns"
rm -rf "$ICONSET"

# Disk image with a shortcut to /Applications for drag-and-drop install.
mkdir -p "$DIST/dmg"
cp -R "$APP" "$DIST/dmg/"
ln -s /Applications "$DIST/dmg/Applications"
hdiutil create -quiet -volname "TermTube $VERSION" -srcfolder "$DIST/dmg" -ov -format UDZO "$DIST/TermTube.dmg"
rm -rf "$DIST/dmg"

echo "Built $APP and $DIST/TermTube.dmg"
