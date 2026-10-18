#!/bin/bash
# Scans a fake macOS home with the built binary and checks what is (not) offered.
#   scripts/macos/smoke.sh path/to/draxbyte
set -euo pipefail
BIN="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
# Outside the system temp dir (which the rules treat as a cache location), like a real home.
H="$(mktemp -d "$HOME/smoke.XXXXXX")/home"
mkdir -p "$H"
export HOME="$H" DRAXBYTE_HOME="$H/.draxbyte"

mk() { mkdir -p "$(dirname "$1")"; mkfile -n "$2" "$1"; touch -t 202001010000 "$1"; }

mk "$H/Library/Caches/com.example.app/old.cache.bin" 3m      # expected: offered
mk "$H/Downloads/oldinstaller.iso" 3m                         # expected: offered
mk "$H/Documents/ordinarybig.bin" 700m                        # expected: offered (large file)
mk "$H/Applications/Foo.app/Contents/MacOS/appbig.bin" 700m   # expected: NOT offered
mk "$H/Library/Mobile Documents/com~apple~CloudDocs/clouddoc.bin" 700m   # NOT offered
mk "$H/Library/CloudStorage/Dropbox/dropboxfile.bin" 700m     # NOT offered
mk "$H/Pictures/Photos Library.photoslibrary/originals/photo.bin" 700m   # NOT offered
mk "$H/Library/Mail/V10/mailbox.bin" 700m                     # NOT offered

OUT="$("$BIN" scan "$H" --limit 200)"
echo "$OUT"

fail=0
want()  { echo "$OUT" | grep -q "$1" || { echo "MISSING (should be offered): $1"; fail=1; }; }
avoid() { if echo "$OUT" | grep -q "$1"; then echo "LEAK (must not be offered): $1"; fail=1; fi; }
want Library/Caches; want oldinstaller.iso; want ordinarybig.bin
avoid appbig.bin; avoid clouddoc.bin; avoid dropboxfile.bin; avoid photo.bin; avoid mailbox.bin

# /tmp and /var/tmp are symlinks on macOS; scanning them must not crash.
"$BIN" scan /tmp --limit 5 >/dev/null
"$BIN" scan /var/tmp --limit 5 >/dev/null

# ---- program removal on a made-up app -------------------------------------------------------
mkapp() {
  mkdir -p "$1/Contents"
  cat > "$1/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$2</string>
<key>CFBundleName</key><string>$3</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
</dict></plist>
PLIST
  head -c 100000 /dev/zero > "$1/Contents/exe"
}
mkapp "$H/Applications/Zorb Notes.app" com.zorblabs.notes "Zorb Notes"
mkdir -p "$H/Library/Caches/com.zorblabs.notes" "$H/Library/Application Support/Zorb Notes" \
         "$H/Library/Caches/com.zorblabs.other" "$H/Library/Application Support/Some Other App" "$H/Library/Preferences"
echo x > "$H/Library/Caches/com.zorblabs.notes/c.bin"
echo x > "$H/Library/Application Support/Zorb Notes/db"
echo x > "$H/Library/Preferences/com.zorblabs.notes.plist"
echo keep > "$H/Library/Caches/com.zorblabs.other/keep"
echo keep > "$H/Library/Application Support/Some Other App/keep"

APPS="$("$BIN" apps --limit 400)"
echo "$APPS" | grep -q "Zorb Notes" || { echo "MISSING from apps list: Zorb Notes"; fail=1; }
# Safari lives on the sealed system volume: it must be listed as protected, not removable
echo "--- Safari as listed:"; echo "$APPS" | grep -i "safari" || echo "(Safari not in the list)"
if echo "$APPS" | grep -q "Safari"; then
  echo "$APPS" | grep "Safari" | grep -qiE "korumal|protected" || { echo "Safari should be shown as system-protected"; fail=1; }
  # the binary exits non-zero when it refuses, so capture first (pipefail would misreport)
  SAFARI_OUT="$("$BIN" uninstall "Safari" 2>&1 || true)"
  echo "$SAFARI_OUT" | head -3
  echo "$SAFARI_OUT" | grep -qiE "CANNOT|KALDIRILAMAZ|sealed|mühürlü" || { echo "Safari removal must be refused"; fail=1; }
fi

"$BIN" uninstall "Zorb Notes" --apply || true
[ ! -e "$H/Applications/Zorb Notes.app" ] || { echo "LEAK: app bundle still there"; fail=1; }
[ ! -e "$H/Library/Caches/com.zorblabs.notes" ] || { echo "LEAK: cache left behind"; fail=1; }
[ ! -e "$H/Library/Application Support/Zorb Notes" ] || { echo "LEAK: app support left behind"; fail=1; }
[ ! -e "$H/Library/Preferences/com.zorblabs.notes.plist" ] || { echo "LEAK: preferences left behind"; fail=1; }
[ -e "$H/Library/Caches/com.zorblabs.other/keep" ] || { echo "DAMAGE: another app's cache was removed"; fail=1; }
[ -e "$H/Library/Application Support/Some Other App/keep" ] || { echo "DAMAGE: another app's data was removed"; fail=1; }

"$BIN" memory --limit 5 >/dev/null || { echo "memory command failed"; fail=1; }

# ---- duplicate finder and disk map ---------------------------------------------------------
D="$H/Documents/dupes"
mkdir -p "$D/a" "$D/b"
head -c 5000000 /dev/urandom > "$D/a/photo.bin"
cp "$D/a/photo.bin" "$D/b/photo-copy.bin"
head -c 5000000 /dev/urandom > "$D/a/unique.bin"
DUP="$("$BIN" dupes "$D" --min-size-mb 1)"
echo "$DUP"
echo "$DUP" | grep -q "photo-copy.bin" || { echo "dupes: the copy was not found"; fail=1; }
if echo "$DUP" | grep -q "unique.bin"; then echo "dupes: a unique file was wrongly listed"; fail=1; fi
# really remove the extra copy (permanently: the Trash is not reliable on a CI runner)
printf 'DELETE\n' | "$BIN" dupes "$D" --min-size-mb 1 --apply --permanent || true
n=0
[ -e "$D/a/photo.bin" ] && n=$((n+1))
[ -e "$D/b/photo-copy.bin" ] && n=$((n+1))
[ "$n" -eq 1 ] || { echo "dupes: expected exactly one copy to remain, found $n"; fail=1; }
[ -e "$D/a/unique.bin" ] || { echo "dupes: the unique file was removed"; fail=1; }
MAP="$("$BIN" map "$H/Documents" --depth 2 --limit 5)"
echo "$MAP"
echo "$MAP" | grep -q "dupes" || { echo "map: the folder is missing from the map"; fail=1; }
exit $fail
