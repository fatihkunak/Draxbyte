#!/bin/bash
# Scans a fake macOS home with the built binary and checks what is (not) offered.
#   scripts/macos/smoke.sh path/to/draxbyte
set -euo pipefail
BIN="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
H="$(mktemp -d)/home"
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
want old.cache.bin; want oldinstaller.iso; want ordinarybig.bin
avoid appbig.bin; avoid clouddoc.bin; avoid dropboxfile.bin; avoid photo.bin; avoid mailbox.bin

# /tmp and /var/tmp are symlinks on macOS; scanning them must not crash.
"$BIN" scan /tmp --limit 5 >/dev/null
"$BIN" scan /var/tmp --limit 5 >/dev/null
exit $fail
