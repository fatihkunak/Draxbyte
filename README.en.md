<p align="center">
  <img src="docs/screenshots/logo.png" width="120" alt="Draxbyte logo">
</p>

<h1 align="center">Draxbyte</h1>
<p align="center"><b>Cleaner system. More space.</b></p>
<p align="center">
  Local-first disk scanner and cleaner — Rust + egui, one small binary, works offline.<br>
  <a href="README.md">Türkçe</a> · <b>English</b> · <a href="docs/USAGE.md">User guide</a> · <a href="docs/KULLANIM.md">Kullanım kılavuzu</a> · <a href="https://github.com/fatihkunak/Draxbyte/wiki">Wiki</a>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-10B981"></a>
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-1A1D21">
  <img alt="Platform" src="https://img.shields.io/badge/Windows-tested-1A1D21">
  <img alt="Languages" src="https://img.shields.io/badge/UI-English%20%7C%20T%C3%BCrk%C3%A7e-1A1D21">
</p>

<p align="center">
  <img src="docs/screenshots/en/home.png" alt="Draxbyte home screen" width="900">
</p>

## What is it?

Draxbyte scans your disk, finds **large and useless** files (old logs, caches, `node_modules`, build output,
installer images…), colours them **safe / careful / keep**, and **never deletes anything until you approve**.
Deleted items go to the Recycle Bin by default and can be restored.

- **Local and private.** Scanning and every decision happen on your machine. No telemetry, no account.
- **AI is optional.** It is **off by default**; while it is off, not a single network request is made. If you turn
  it on, every request needs your click, answers are cached locally, and there is a daily token limit.
- **Safety first.** System folders, secrets, locked files and files changed in the last 24 hours are never deleted.
- **One small file.** `draxbyte.exe` is about 16 MB; no installer or runtime needed.
- **Two languages.** English and Türkçe, chosen automatically from your system or in Settings.

## Features

- 🔎 **Fast parallel scan** — uses all your cores, cancellable, live counters
- 🧠 **Rule engine, no AI needed** — old logs/dumps, caches, `node_modules`, Gradle/Cargo build output, disk images, big or old archives, virtual disks
- 🚦 **Safe / Careful / Keep** colours with a plain-language reason for every row
- 🛡️ **Safety net** — protected system paths, secrets and links are never touched; every item is re-checked right before deletion
- ♻️ **Reversible** — Recycle Bin by default, one-click undo, dry run, permanent delete only after an extra confirmation
- 📊 **Dashboard** — disk usage ring, largest items, distribution by type, one-click actions
- 🤖 **Optional AI** — 14 providers (Ollama and LM Studio run locally for free), models fetched automatically, cached answers, daily token limit, every request approved by you
- ⌨️ **CLI** — `draxbyte scan` / `draxbyte clean` for scripts and servers (dry run by default)
- 🌍 **English and Türkçe** UI, automatic or your choice
- 📦 **One ~16 MB exe**, offline, no telemetry, MIT licensed

## How it works

1. **Scan** — the app walks your folders in parallel and asks the rule engine what each item is. Nothing is changed.
2. **Review** — you see every candidate with size, age and the reason. Tick what you want gone.
3. **Clean** — a confirmation shows the exact total. Items go to the Recycle Bin (or are only reported in a dry run). A report lists what happened to each one, and you can undo.

## Screenshots

| | |
|---|---|
| ![Results](docs/screenshots/en/results.png) | ![Confirm](docs/screenshots/en/confirm.png) |
| **Results** — colour-coded, sortable list | **Confirm** — summary, dry run, Recycle Bin / permanent |
| ![Cleanup](docs/screenshots/en/cleanup.png) | ![AI settings](docs/screenshots/en/settings-ai.png) |
| **Cleanup report** — what happened to each item, undo | **AI settings** — many providers, models fetched automatically |

> Screenshots were taken with a throw-away demo folder; they contain no personal files.

<details>
<summary><b>More screenshots</b></summary>

| | |
|---|---|
| ![Language](docs/screenshots/en/settings-language.png) | ![About](docs/screenshots/en/about.png) |
| **Settings** — language, scan, rules | **About** — version, developer, licences |

</details>

## Getting started

### Download (Windows)

Grab `draxbyte.exe` from the [latest release](https://github.com/fatihkunak/Draxbyte/releases/latest) and run it.
See [Signing](#signing-and-smartscreen) if Windows shows a warning.

### Build from source

Requirements: [Rust](https://rustup.rs) (stable) and the Visual Studio C++ Build Tools.

```powershell
git clone https://github.com/fatihkunak/Draxbyte.git
cd Draxbyte
powershell -File scripts\install.ps1
```

`install.ps1` builds a release binary, copies it to `%LOCALAPPDATA%\Programs\Draxbyte` and creates **Desktop and
Start Menu shortcuts**. Run it again to update. To just try it: `cargo run --release`.

### First run (1 minute)

1. Press **Quick scan** (default: your home folder + the temp folder).
2. On **Results**, press **Select all green** to tick the safe items.
3. **Clean…** → check the summary → try **Dry run** first → then **Move to Bin**.
4. Changed your mind? **Cleanup → Undo**.

Full walkthrough: **[docs/USAGE.md](docs/USAGE.md)**.

## What do the colours mean?

| Colour | Meaning | Examples |
|---|---|---|
| 🟢 **Safe** | Deleting breaks nothing | Old `.log/.tmp/.dmp`, cache folders, `__pycache__` |
| 🟡 **Careful** | Deletable, but take a look | `node_modules`, Gradle/Cargo build output, disk images (`.iso`), big/old archives, virtual disks |
| 🔴 **Keep** | Cannot be selected or deleted | System files, installed-app files, big files changed in the last 24 h |

Folders such as `node_modules`, `build`, `target` are counted as **one item**, so thousands of files do not flood the list.

## Safety

- Never touched: the Windows folder, Program Files, `/usr` `/etc` and similar, `~/.ssh` `~/.gnupg` `~/.aws`,
  filesystem roots, your home folder itself, and Draxbyte's own data.
- Symlinks and junctions are not followed and not deleted.
- **Right before deleting, every item is checked again:** does it still exist, is it not a link, is another program
  using it (a lock test on Windows), did it change since the scan? If any check fails the item is skipped and the report says why.
- Red items cannot be selected in the UI and the code refuses them too (two layers).
- Recycle Bin by default; **Delete permanently** needs an extra confirmation (you type `DELETE`).

## AI (optional)

Settings → **AI analysis**. Pick a provider, enter its key, press **Fetch models** to list the models the provider
really offers (costs no tokens), choose one, then **Test connection**.

Providers: **Ollama** and **LM Studio** (local, no key, zero cost) · Google Gemini · Groq · OpenRouter · NVIDIA NIM ·
Mistral · Anthropic · OpenAI · Kimi · xAI · EVREN · Alibaba · any OpenAI-compatible custom URL.

| Guard | Behaviour |
|---|---|
| Off by default | With `ai.enabled = false` not even a client is created |
| Rules first | Scanning and classification are local; AI only runs when you ask “what is this?” |
| One approval per request | “Ask AI” shows the **exact text** that will be sent (no file contents, only path + metadata) |
| Cache first | Key = path + size + modified time; the same file is never sent twice |
| Cache is mandatory | AI does not run with the cache off |
| Daily limit | Tokens are counted per day; clearing the cache does not reset the counter |

> **Honest note:** model listing and chat were tested against a **real local OpenAI-compatible server**; the other
> providers are covered by unit tests (request shape, auth header, response parsing) but were not tried with live
> accounts. Keys are stored as plain text in `config.toml`; prefer the `DRAXBYTE_API_KEY` environment variable.

## Command line

```powershell
draxbyte scan [PATHS...] [--limit N] [--save]        # list (changes nothing)
draxbyte clean [PATHS...] [--yellow]                  # DRY RUN (default)
draxbyte clean [PATHS...] [--yellow] --apply          # move to the Recycle Bin
draxbyte clean [PATHS...] --apply --permanent         # delete for good (asks you to type DELETE)
```

Without arguments the GUI opens. `DRAXBYTE_HOME=<folder>` moves the config/data folder (portable use).
Language: `[ui] language = "auto" | "tr" | "en"` in the config, or Settings → Dil / Language.

## Config

`~/.draxbyte/config.toml` (also editable in Settings):

```toml
[ui]
language = "auto"         # auto | tr | en

[scan]
default_paths = ["~"]
max_depth = 10
min_size_mb = 1
exclude_patterns = ["*.git/*", "node_modules/*"]

[rules]
safe_extensions = ["tmp", "log", "cache", "dmp", "old", "bak"]
safe_age_days = 30
large_file_mb = 500

[ai]
enabled = false
provider = "openai"
model = "gpt-4o-mini"
base_url = ""             # only for custom / local / Alibaba
daily_token_limit = 50000
cache_enabled = true
cache_path = "~/.draxbyte/cache.db"
# [ai.api_keys]
# groq = "..."

[cleanup]
use_trash = true
confirm_permanent_delete = true
```

## Signing and SmartScreen

Release binaries are signed with a **self-signed code-signing certificate** (“Fatih Kunak”). That proves the file
was not modified after signing, but Windows does not trust a self-signed publisher, so SmartScreen may still show
“Windows protected your PC” — choose **More info → Run anyway**. You can verify the download against the
`SHA256` value in the release notes. The public certificate is in [`assets/draxbyte-codesign.cer`](assets/draxbyte-codesign.cer).
If you would rather not trust a binary, build it from source (see above).

## Platforms

Developed and tested on **Windows 11**. The code is platform-aware for Linux and macOS (Trash, lock check,
protected paths) but has not been built or tried there. On macOS there is no programmatic undo.

## Development

```powershell
cargo test                                          # unit tests
cargo test --test trash_roundtrip -- --ignored      # real Recycle Bin round trip (opt-in)
$env:DRAXBYTE_LIVE_URL="http://localhost:8080/v1"; cargo test --test live_provider -- --ignored --nocapture
cargo build --release                               # target\release\draxbyte.exe
powershell -File scripts\release.ps1                # build + sign + zip + checksums
python scripts\make_icon.py "<design.png>"          # regenerate the icon
```

```
src/core/    scanner, rule engine, safety, cleaner         (the product)
src/ai/      providers, client, cache, prompt              (optional)
src/i18n.rs  Turkish / English text
src/ui/      egui screens        src/app.rs  state + background jobs
docs/wiki/   architecture wiki (Obsidian-compatible)   docs/USAGE.md  user guide
```

Design decisions and why: [DECISIONS.md](DECISIONS.md) · module map: [docs/wiki/Index.md](docs/wiki/Index.md).

## Roadmap

Ideas, not promises: trusted code signing (SignPath), more cleanup rules (browser/package-manager/game-launcher caches), Linux and macOS testing, a scheduled "monthly scan" reminder, more UI languages. Have an idea? Open an [issue](https://github.com/fatihkunak/Draxbyte/issues).

## Contributing and support

Pull requests are welcome — read [CONTRIBUTING.md](CONTRIBUTING.md) first. Security issues: see [SECURITY.md](SECURITY.md). Release history: [CHANGELOG.md](CHANGELOG.md). Documentation also lives in the [Wiki](https://github.com/fatihkunak/Draxbyte/wiki).

## License

[MIT](LICENSE) © 2026 Fatih Kunak. Typeface **Inter** © The Inter Project Authors,
[SIL Open Font License 1.1](assets/fonts/Inter-LICENSE.txt).

## Developer

**Fatih Kunak** · [fatihgo.com](https://fatihgo.com) · [github.com/fatihkunak](https://github.com/fatihkunak)

Bug reports and suggestions: [Issues](https://github.com/fatihkunak/Draxbyte/issues).
