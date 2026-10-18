#!/usr/bin/env bash
# macOS only: builds the app, wraps it in "Video Mixer.app" (with the icon) and
# packs a .dmg with an Applications shortcut for drag-and-drop installing.
#
#   ./scripts/build-dmg.sh              host architecture
#   ./scripts/build-dmg.sh --universal  Apple Silicon + Intel (also --arm64, --x86_64)
#
# Output: dist/macos/Video Mixer.app and dist/Video-Mixer-<version>-<arch>.dmg
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$ROOT"

[ "$(host_os)" = "macos" ] || only_on "a .dmg" "a Mac"

APP_NAME="Video Mixer"
BUNDLE_ID="com.videomixer.app"
VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
ARCH_FLAG="${1:-}"

# 1. The binary (and its shaders folder) from the regular macOS build.
./scripts/build-macos.sh ${ARCH_FLAG:+"$ARCH_FLAG"}
case "$ARCH_FLAG" in
  --universal) ARCH=universal ;;
  --x86_64) ARCH=x86_64 ;;
  --arm64) ARCH=arm64 ;;
  *) ARCH="$(uname -m)" ;;
esac

# 2. The icon, drawn on first use.
[ -f assets/icon.icns ] || python3 scripts/make-icon.py

# 3. The bundle.
APP="dist/macos/$APP_NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "dist/macos/$BIN" "$APP/Contents/MacOS/$BIN"
cp -R shaders "$APP/Contents/Resources/shaders"
cp assets/icon.icns "$APP/Contents/Resources/AppIcon.icns"

# The program reads and writes shaders/, settings and sessions in its working
# directory, which for a double-clicked app is "/" (read-only). The launcher
# moves into a folder of the user's own and seeds it with the bundled shaders
# (never overwriting ones the user edited, but adding new ones).
cat > "$APP/Contents/MacOS/launcher" <<'LAUNCHER'
#!/bin/bash
HERE="$(cd "$(dirname "$0")" && pwd)"
DATA="$HOME/Library/Application Support/Video Mixer"
mkdir -p "$DATA/shaders"
cp -Rn "$HERE/../Resources/shaders/." "$DATA/shaders/" 2>/dev/null
cd "$DATA" || exit 1
exec "$HERE/nannou_sketch"
LAUNCHER
chmod +x "$APP/Contents/MacOS/launcher"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>launcher</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSCameraUsageDescription</key><string>Video Mixer uses the camera as a video source.</string>
</dict>
</plist>
PLIST

# Ad-hoc signature: enough to run on this Mac and to ask for camera access.
# (Distributing to others needs a Developer ID signature and notarization.)
codesign --force --deep --sign - "$APP"

# 4. The disk image.
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="dist/Video-Mixer-$VERSION-$ARCH.dmg"
rm -f "$DMG"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null

echo "==> Done: $APP"
echo "==> Done: $DMG"
