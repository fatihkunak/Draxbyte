# Changelog

## 0.3.0 — 2026-10-18
- New: **Programs** screen and `draxbyte apps` / `draxbyte uninstall`. Installed programs with size and an estimated last use (UserAssist, Prefetch, BAM on Windows; Spotlight on macOS — background helpers never count as "used", no record shows "unknown"). Remove any program completely: the vendor's uninstaller first, then its leftovers (folders, shortcuts, registry keys, autostart entries, services, scheduled tasks). **Force removal** for programs with no uninstaller or that the vendor blocks (Edge, with a block so Windows Update does not restore it). Registry keys are exported before deletion (skipped if the export fails) and can be restored. "Probable" leftovers (shared vendor folders, browser profiles) are never pre-selected and need a second confirmation. The Home page suggests programs unused for 90+ days
- New: **Memory & Startup** screen, `draxbyte memory`, `draxbyte startup`. Memory per program, close what you pick (system programs and a personal protected list are refused; by PID, never by name), "Free memory" (temporary by nature, and the screen says so), switch startup programs off/on without deleting them
- New: **complete scan.** Every internal drive and the system folders (`Windows\Temp`, `Logs\CBS`, `SoftwareDistribution\Download`, `Prefetch`, `Package Cache`, `$PatchCache$`, `Windows.old`, WER…) are scanned by default (Settings can turn each off). Max depth default 10 → 64 (old configs move once). New Recycle Bin card and `draxbyte recycle [--empty]`
- New (macOS): `.app` bundles with their `~/Library` / `/Library` files by bundle identifier. Apps on the sealed system volume (Safari, Mail…) cannot be deleted by anything; only their caches are cleaned. Verified on a real macOS runner (tests and a removal smoke test), not by hand on a Mac
- Safety: removal has its own guards, checked when searching and again right before deleting (protected folders, shared runtimes, OS registry branches); the red rule for system paths is unchanged
- Not verified on a real machine: removing Windows services / HKLM keys (needs administrator), System Restore points (the service is off on the test PC), the macOS GUI
- Known limits: Windows — the WebView2 runtime is a separate program and is not removed together with Edge. macOS — no in-app update, no registry-style undo (files go to the Trash or are deleted as you choose)

Türkçe özet: Programlar (boyut, son kullanım tahmini, üreticinin kaldırıcısı + artıklar, zorla kaldırma, Edge), Bellek ve Başlangıç, eksiksiz tarama (her sürücü + sistem klasörleri + Geri Dönüşüm Kutusu), macOS `.app` desteği. Kayıt defteri önce yedeklenir; "olası" artıklar ayrıca onay ister.

## 0.2.2 — 2026-10-18
- New: **macOS build** (`Draxbyte-0.2.2-macos.dmg`, Intel + Apple Silicon, ad-hoc signed — not notarized: first launch right-click → Open). Unit tests and a scan against a fake Mac home run on GitHub's macOS runner (`scripts/macos/smoke.sh`)
- Safety (macOS): contents of `.app`, `.photoslibrary`, `.musiclibrary`, `.tvlibrary` packages and `~/Library/{Mobile Documents, CloudStorage, Keychains, Mail, Messages, Containers, Group Containers}` are never offered — deleting those would also delete cloud copies or break apps. `~/Library/Caches` stays cleanable
- Fix (macOS): the Docker check works when the app is started from Finder (GUI apps get a minimal PATH)
- Fix: a unit-test fixture used `/lib`, which is a protected system tree on Unix
- macOS limits: no in-app undo (restore from the Trash by hand), no in-app update. The GUI has not been tried by hand on a real Mac yet

Türkçe özet: macOS sürümü (.dmg) eklendi; `.app`, Fotoğraflar kitaplığı, iCloud/CloudStorage, Mail/Messages/Keychains asla önerilmez; Docker kontrolü Finder'dan açılınca da çalışır. Mac'te uygulama içi geri alma ve güncelleme yok.

## 0.2.1 — 2026-10-17
- New: in-app update. About → "Update now" downloads `Draxbyte-Setup.exe`, verifies its SHA256 (from the release's `SHA256SUMS.txt`) and the pinned signing certificate, then starts the installer and closes the app. Nothing runs before verification; any failure deletes the file and shows the error
- Setup retries copying the exe for up to 15 s so it can replace a Draxbyte that is still closing
- Note: 0.2.0 installs have no in-app update — install this version once by hand, later versions update from inside the app

Türkçe özet: uygulama içinden güncelleme (indir → SHA256 + sabitlenmiş imza doğrulaması → Setup'ı başlat); 0.2.0 kullananlar bu sürümü bir kez elle kurmalı.

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

