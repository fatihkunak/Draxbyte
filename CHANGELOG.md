# Changelog

## Unreleased
- **EVREN: terms of use.** EVREN answers 403 until the key's owner accepts its terms. Settings now shows *Show the terms of use* next to that error: it reads the text from the provider, shows it in full and sends the acceptance only when you click *I have read and accept*

Türkçe özet: EVREN 403 "kullanım şartlarını kabul edin" derse Ayarlar'da "Kullanım şartlarını göster" düğmesi çıkar; metin okunur, kabul yalnız senin tıklamanla gönderilir.

## 0.6.4 — 2026-10-19
- Fix: **Settings → AI chain: the fetched model list was almost invisible** (a tiny box next to the model field). It is now a clear *Choose a model* list under the row, and an empty model field takes the first model of the list (one local model is the common case). The *remove* button of a chain row shows a proper ×
- Fix: a refused connection to this computer says *nothing is listening at <address> — is the local server (Ollama, LM Studio…) running?* instead of "error sending request"

Türkçe özet: Ayarlar → AI zincirinde model listesi artık belirgin ("Model seç"), boş model alanına ilk model otomatik yazılıyor; yerel sunucu kapalıysa "orada hiçbir şey dinlemiyor, Ollama / LM Studio açık mı?" deniyor.

## 0.6.3 — 2026-10-19
- Fix: **Google Gemini 3.x timed out.** Gemini thinks for minutes by default (a one-word answer took 125 s in a real test, 5 s with thinking off), longer than the 60 s limit, so every question failed over to the next provider. Gemini requests now turn thinking off (`reasoning_effort: none`; a model that refuses it is asked again without), the chat waits up to 120 s (other requests 60 s instead of 30 s), and a timeout is reported as a timeout
- Fix: **AI Advisor answers show their Markdown** (bold, italic, `code`, bullets, headings) instead of raw asterisks
- Fix: with a single provider (no chain) the provider's own `DRAXBYTE_API_KEY_<PROVIDER>` environment variable now works too, as it already did in a chain; the "no API key" message names both variables

Türkçe özet: Google Gemini 3.x zaman aşımı düzeltildi (düşünme kapalı, sohbet 120 sn), AI Danışman cevapları Markdown olarak biçimli, tek sağlayıcıda `DRAXBYTE_API_KEY_<SAĞLAYICI>` ortam değişkeni de çalışıyor.

## 0.6.2 — 2026-10-19
- New: **AI Advisor chat — pick the model on the screen.** The Model list shows every provider of the chain with its models (↻ fetches a provider's list, or type a name); picking a model switches to its provider at once, the others stay behind it as backups. The choice lasts for the session. The answer cache now also depends on provider, address and model
- New: **Settings → AI chain:** every provider has *Fetch models* and *Test connection*
- New: **About → automatic update check** box: when ticked, Draxbyte asks GitHub for the latest version number at start-up (same option as in Settings, saved at once). Nothing is downloaded or installed by itself
- Fix: **Memory & Startup** table: the program column is capped, so in a wide window the numbers and buttons stay next to the name
- Fix: an AI Advisor answer cut off by the token limit (reasoning models spend part of it on thinking) is flagged, is not cached, and the limit is 3000 instead of 800. *Test connection* gives reasoning models room for their thinking

Türkçe özet: AI Danışman sohbetinde sağlayıcı/model seçimi, zincirde "Modelleri getir" ve "Test", Hakkında'da açılışta otomatik güncelleme denetimi kutusu, kesilen AI yanıtı düzeltmesi, Bellek tablosu geniş pencerede toplu.

## 0.6.1 — 2026-10-19
- Fix: **Memory & Startup** — the action column was too narrow, so the third button (Protect) was cut off and the "Unsaved work is lost. Sure?" confirmation did not fit. The column is wider now

Türkçe özet: Bellek ve Başlangıç sayfasında kesilen "Koru" butonu ve sığmayan onay satırı düzeltildi.

## 0.6.0 — 2026-10-19
- New: **AI Advisor** — a chat about the last scan (what can go, how risky, how to do it) and a one-click "AI report". Several providers can be added in Settings → AI as a **failover chain**: every request starts with the primary provider and moves to the next one on a network error, timeout, rate limit, bad key, server error or unreadable answer; the answer shows who replied and who failed and why. The advisor only advises, never deletes. Off by default; the scan summary (sizes, rule names, shortened paths, never file contents) is shown before anything is sent, and every request starts with your Send click. In a chain each provider only ever gets its own key (`DRAXBYTE_API_KEY_<PROVIDER>`); the shared `DRAXBYTE_API_KEY` is not used there, so one provider's key can never reach another
- New: **Shrink the Docker disk image** as a part of Full Optimization (Windows, WSL2 backend, administrator rights, off by default; `draxbyte optimize --docker-compact`). Docker's clean-up frees space inside its disk image only; this gives it back to Windows. It records the running containers, stops Docker, shuts WSL down, compacts the disk with diskpart, starts Docker again and starts every container that was running before, in the old order, retrying and checking at the end. Nothing inside the disk is deleted. If Docker does not stop, nothing is changed; a container that does not come back is named in the result
- Change: the approval step lists unused Docker volumes with their sizes and marks named volumes (data you created on purpose) apart from anonymous ones

Türkçe özet: AI Danışman (sohbet + AI rapor, çoklu sağlayıcı zinciri ve yedekleme, sağlayıcıya özel anahtar, varsayılan kapalı + onay), Docker disk imajını küçültme (Tam Optimizasyon'da).

## 0.5.0 — 2026-10-19
- New: **Full Optimization wizard** (scope → read-only analysis → approval → run → result). Safe cleanup (green items only), duplicates (oldest copy stays), Docker piece by piece (build cache, untagged images, networks by default; stopped containers, unused images and volumes only when ticked, with the list shown first), Recycle Bin, memory (Windows). Nothing changes before the approval; files go to the Recycle Bin first. `draxbyte optimize` (dry run unless `--apply`)

## 0.4.0 — 2026-10-19
- New: **Disk Map** — a treemap of what takes the space (tile area = size). Click a folder to look inside, hover for details, jump to the file manager. Read-only. `draxbyte map` prints the same as text
- New: **Duplicate Files** — finds files with exactly the same content (size → first/last 64 KB → full SHA-256). Every group always keeps at least one copy; deletion goes through the same re-checking cleaner (Recycle Bin by default). Developer and build folders (`node_modules`, `.git`, `intermediates`, `.cxx`…) are skipped by default. `draxbyte dupes` (dry run unless `--apply`)
- New: **install and uninstall wizard**, same theme, logo and languages as the app. `Draxbyte-Setup.exe` is now a signed copy of the program itself and starts the wizard (Windows: per-user install, shortcuts, "Apps & features" entry, update mode keeps your choices; macOS: opening `Draxbyte.app` from the disk image installs it into Applications). Uninstall is a wizard too and removes only the files the installer put there, plus settings/data if you tick it. An installation that is running is replaced by renaming it, so no force-close is needed
- Change (Windows): **Draxbyte asks for administrator rights when it starts**, so no folder is out of reach. Decline the prompt and it simply opens without them; Settings can turn this off. The wizards and command-line commands never ask
- Fix: **"inaccessible" is now a diagnosis, not a number.** A scan reports every unreadable path with the reason (access denied / in use by another program / other), shows examples and the one-click fix (restart as administrator on Windows, Full Disk Access on macOS). Files that vanish during a scan (temp files, caches) are no longer counted as inaccessible; folder totals no longer hide read errors. The Disk Map and cleanup report give the same help. `draxbyte scan --show-errors` lists them
- Fix (macOS): personal folders (Documents, Desktop…) are no longer accepted as an install location, same as Windows
- Note: `Draxbyte-Setup.exe` is about 18 MB instead of 6 MB because it now contains the whole program. Installations from 0.3.0 and earlier update through About → Update now
- Not verified on a real machine: the macOS wizard window (its logic is tested on Windows and on a macOS runner), the Duplicate Files screen as a window (logic and CLI tested on real data)

Türkçe özet: Disk Haritası, Kopya Dosyalar, kurulum/kaldırma sihirbazı (Setup.exe artık programın kendisi), Windows'ta açılışta yönetici izni (reddedersen yine açılır), erişilemeyen yollar için neden + tek tıkla çözüm.

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

