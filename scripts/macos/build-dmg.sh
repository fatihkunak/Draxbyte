#!/bin/bash
# Builds dist/Draxbyte-<version>-macos.dmg (universal, ad-hoc signed). Run on macOS.
set -euo pipefail
cd "$(dirname "$0")/../.."
VER="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.+)".*/\1/')"

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

# Fresh temp dirs instead of cleaning old ones.
WORK="$(mktemp -d)"
STAGE="$WORK/stage"
APP="$STAGE/Draxbyte.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" dist
lipo -create -output "$APP/Contents/MacOS/draxbyte" \
  target/aarch64-apple-darwin/release/draxbyte target/x86_64-apple-darwin/release/draxbyte

# Icon: assets/icon_1024.png -> AppIcon.icns
ICONSET="$WORK/AppIcon.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon_1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) assets/icon_1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Draxbyte</string>
  <key>CFBundleDisplayName</key><string>Draxbyte</string>
  <key>CFBundleIdentifier</key><string>com.fatihkunak.draxbyte</string>
  <key>CFBundleExecutable</key><string>draxbyte</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VER</string>
  <key>CFBundleVersion</key><string>$VER</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

# Apple Silicon refuses completely unsigned code; ad-hoc is enough to run (not notarized).
codesign --force --deep --sign - "$APP"

ln -s /Applications "$STAGE/Applications"
cat > "$STAGE/BUNU-OKU-READ-ME.txt" <<TXT
Draxbyte $VER (macOS)

1. Draxbyte.app dosyasini Applications klasorune surukle.
2. Ilk acilista "gelistirici dogrulanamadi" derse: uygulamaya SAG TIKLA > Ac > Ac.
   Hala acilmazsa Terminal'e yaz:  xattr -dr com.apple.quarantine /Applications/Draxbyte.app
3. Geri alma: silinenler Cop Kutusu'na gider; macOS'ta uygulama icinden geri alma yok,
   dosyalari Cop Kutusu'ndan elle geri koy.

1. Drag Draxbyte.app into Applications.
2. If macOS says the developer cannot be verified: right-click the app > Open > Open.
   Or run:  xattr -dr com.apple.quarantine /Applications/Draxbyte.app
3. Undo: removed items go to the Trash; restore them by hand (no in-app undo on macOS).
TXT

DMG="dist/Draxbyte-$VER-macos.dmg"
hdiutil create -volname "Draxbyte" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
shasum -a 256 "$DMG" | tee "$DMG.sha256"
