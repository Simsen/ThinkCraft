#!/bin/sh
# Builds ThinkCraft.app (a normal Mac app you can double-click) into dist/.
# Run from this folder:  sh make-mac-app.sh
set -e
cd "$(dirname "$0")"

CARGO="${CARGO:-cargo}"
command -v "$CARGO" >/dev/null 2>&1 || CARGO="$HOME/.cargo/bin/cargo"

echo "Building ThinkCraft (release)..."
"$CARGO" build --release

APP="dist/ThinkCraft.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/thinkcraft "$APP/Contents/MacOS/thinkcraft"

# Icon: draw it at every size macOS wants, then pack it into an .icns file
ICONSET="dist/ThinkCraft.iconset"
mkdir -p "$ICONSET"
target/release/thinkcraft --make-icon dist/icon-1024.png
for s in 16 32 128 256 512; do
  sips -z $s $s dist/icon-1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d dist/icon-1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/ThinkCraft.icns"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>ThinkCraft</string>
  <key>CFBundleDisplayName</key><string>ThinkCraft</string>
  <key>CFBundleIdentifier</key><string>app.thinkcraft.desktop</string>
  <key>CFBundleExecutable</key><string>thinkcraft</string>
  <key>CFBundleIconFile</key><string>ThinkCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Sign it for this Mac only (no Apple account needed). Extra file attributes
# (Finder info and the like) must be cleared first or signing fails.
xattr -cr "$APP" 2>/dev/null || true
if codesign --force --sign - "$APP" 2>/dev/null && codesign -v "$APP" 2>/dev/null; then
  echo "Signed for this Mac."
else
  echo "Note: couldn't sign the app; it still runs, but macOS may ask you to confirm the first time (right-click > Open)."
fi

echo ""
echo "Done: $(pwd)/$APP"
echo "Double-click it to start, or drag it into your Applications folder."
