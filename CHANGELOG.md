# Changelog

## 0.2.0 — 2026-10-17
- Fix: Ctrl+C / Ctrl+V / Ctrl+X did nothing (clipboard support was not compiled in) — pasting an API key now works
- Fix: moving a folder to the Recycle Bin failed with "Some operations were aborted" when one file inside was locked; it now retries item by item and reports partial results clearly
- New: read-only Docker disk check (Home card and `draxbyte docker`) plus a confirmed "Clean build cache" button (`docker builder prune -f` only — containers, images and volumes are never touched)
- New: `Draxbyte-Setup.exe` single-file installer (per-user, shortcuts, uninstall entry)
- New: update check (About → Check for updates; optional at start-up in Settings). Off by default, one request to GitHub, nothing is downloaded or installed

Türkçe özet: yapıştırma kısayolları düzeldi, kilitli dosyalı klasörler artık parça parça Çöp Kutusu'na taşınıyor, Docker disk kontrolü + onaylı build cache temizleme, tek dosya kurulum (Setup.exe), güncelleme kontrolü (varsayılan kapalı).

## 0.1.0 — 2026-10-17
First public release.

- Parallel local scan with a rule engine (safe / careful / keep); folders such as `node_modules`, Gradle `build`, `target` and caches are counted as one item
- Safety: protected paths, symlinks/junctions never followed, re-check right before every delete (exists, not a link, not locked, unchanged since the scan)
- Cleanup: Recycle Bin by default, undo (Windows/Linux), dry run, permanent delete behind an extra confirmation
- Dashboard with disk ring, quick actions, largest items and distribution chart
- Optional AI (off by default): 14 providers incl. Ollama and LM Studio, automatic model listing, local cache, daily token limit, per-request approval
- CLI: `draxbyte scan` / `draxbyte clean` (dry run by default)
- UI in English and Türkçe (automatic or Settings → Dil / Language), Inter typeface, app icon
- Signed release exe (self-signed certificate) with SHA256

Türkçe özet: ilk herkese açık sürüm — yerel paralel tarama, güvenli/dikkat/silme kuralları, silmeden önce son kontrol, Çöp Kutusu + geri alma + kuru çalıştırma, grafikli ana sayfa, isteğe bağlı çoklu sağlayıcılı AI, komut satırı, iki dilli arayüz, imzalı exe.
