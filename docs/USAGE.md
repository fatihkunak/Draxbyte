# Draxbyte User Guide

Everything a first-time user needs, in order. (Türkçe: [KULLANIM.md](KULLANIM.md))

## Contents
1. [Install and launch](#1-install-and-launch)
2. [Screens](#2-screens)
3. [Scanning](#3-scanning)
4. [Reading the results](#4-reading-the-results)
5. [Cleaning](#5-cleaning)
6. [Undo](#6-undo)
7. [AI](#7-ai)
8. [Settings](#8-settings)
9. [Command line](#9-command-line)
10. [FAQ / troubleshooting](#10-faq--troubleshooting)

---

## 1. Install and launch

**Download:** take `draxbyte.exe` from the [latest release](https://github.com/fatihkunak/Draxbyte/releases/latest)
and double-click it. (Windows may show a SmartScreen warning for the self-signed publisher — see the README.)

**Or build it:**

```powershell
git clone https://github.com/fatihkunak/Draxbyte.git
cd Draxbyte
powershell -File scripts\install.ps1
```

This builds the app, installs it to `%LOCALAPPDATA%\Programs\Draxbyte\draxbyte.exe` and puts a **Draxbyte**
shortcut on the Desktop and in the Start Menu. To update: `git pull`, then run `install.ps1` again (close the app first).

The interface language follows your system (English or Türkçe). Change it any time in **Settings → Dil / Language**.

## 2. Screens

| Screen | What it does |
|---|---|
| **Home** | Disk usage, quick-action cards, largest items, distribution chart, last scan summary |
| **Scan** | Live progress of a running scan and a **Cancel** button |
| **Results** | Everything found; select, filter, sort |
| **Cleanup** | Report of the last cleanup and **Undo** |
| **AI cache** | Cache statistics and today's token usage (when AI is on) |
| **Settings** | Language, scan, rules, AI and cleanup settings |
| **About** | Version, developer, data folder, licences |

The text at the bottom of the sidebar shows the AI state: *“AI off · no network requests”* or *“AI on · you approve”*.

## 3. Scanning

**Home → Quick scan.** In “Paths to scan” you can type any folders separated by `;` (`~` means your home folder).
**Default** restores the paths from Settings.

While scanning:
- Files are walked in parallel; counters (files, size, candidates, inaccessible) update live.
- **Cancel** shows partial results but does not save them.
- Nothing is deleted — scanning only reads.

When it finishes you land on **Results** and the scan is saved (the last 5 scans are kept). Close and reopen the
app and the last scan is still on Home.

> Folders such as `node_modules`, `build`, `target` and cache folders appear as **one row** with their total size,
> so a project with 60,000 files does not fill the list.

## 4. Reading the results

Each row: **box · category · size · age · reason · path · AI**.

**Category colours**

| | Meaning |
|---|---|
| 🟢 Safe | Old log/temp/cache files and folders. Deleting them breaks nothing. |
| 🟡 Careful | Regenerable (`node_modules`, build output) or big/old files *we cannot identify*. Take a look. |
| 🔴 Keep | System/app files, big files changed in the last 24 h. **The box is disabled.** |

**Reason** explains the rule (for example “Gradle build output; regenerated on the next build”).

**Toolbar**
- **All / Safe / Careful / Keep** — filter by category.
- **Search path** — filter by text in the path.
- **Select all green** — tick every safe item.
- **Review yellow** — show only yellow items (you decide).
- **Clear selection** — untick everything.
- Click a column header (**Category, Size, Age, Path**) to sort; click again to reverse.

At the bottom you see *“5 selected · 745 MB”*; **Clean…** is enabled when something is selected.

## 5. Cleaning

**Clean…** opens a confirmation window:

- **Summary:** how many items, total size, green/yellow split. A warning appears if yellow items are included.
- **Dry run:** if ticked, *nothing is deleted*; you only get a “what would happen” report. **Try this first** if unsure.
- **Move to Recycle Bin** (default): reversible.
- **Delete permanently:** irreversible, and you must type `DELETE` (can be turned off in Settings).

The **Cleanup** screen then shows progress and a report:

| Status | Meaning |
|---|---|
| deleted / moved to Bin | Success |
| would delete | Dry run |
| skipped: … | The safety check stopped it (locked, changed since the scan, protected path, already gone…) |
| ERROR: … | The OS refused or something else went wrong |

**Every item is re-checked right before it is touched.** If a file changed between the scan and the cleanup, or
another program is using it, it is **skipped** — that is protection, not a bug.

## 6. Undo

**Cleanup → Undo (from Recycle Bin)** puts the items moved to the Recycle Bin in the last cleanup back where they were.

- If you emptied the Recycle Bin, they cannot be restored.
- Permanently deleted items cannot be restored.
- On macOS the button does not work; restore from the Trash by hand.

## 7. AI

When the rule engine cannot identify something (“Large file; check what it is”) you can ask AI. It is **fully optional
and off by default.**

**Turn it on:** Settings → *Enable AI analysis*.

1. Choose a **Provider**. To spend no money pick **Ollama** or **LM Studio** (they run on your computer, no key).
2. Enter the key if needed (the **get a key** link opens the provider's page).
3. **Fetch models** — the provider's real model list appears (costs no tokens). Pick one or type its name.
4. **Test connection** — sends a tiny request (~30 tokens) to verify the key and model.
5. **Save**.

**Use it:** press **Ask AI** on a row.
- If you asked before, the answer comes **free from the cache** (marked *(cached)*).
- Otherwise the **exact text to be sent** is shown; press **Send** to go ahead. File *contents* are never sent —
  only path, type, size, age and the rule hint.
- The answer starts with *SAFE / CAUTION / KEEP* and a short reason. **The decision is still yours.**

**Cost control:** the **AI cache** screen shows today's token usage, cache size and hit rate. When the daily limit
is reached AI turns off for the day.

> Keys are stored as plain text in `~/.draxbyte/config.toml`. The `DRAXBYTE_API_KEY` environment variable is safer
> (and takes priority over the key in Settings).

## 8. Settings

| Section | Field | Description |
|---|---|---|
| Language | Dil / Language | Auto (follow system), Türkçe, English. Applies immediately; **Save** keeps it |
| Scan | Default paths | One path per line |
| | Excluded patterns | e.g. `*.git/*` — these paths are not entered |
| | Max depth / Min. size | Do not go too deep / hide tiny items |
| Rules | Safe extensions | Extensions that count as green once old |
| | Safe age (days) | What “old” means (default 30) |
| | Large file threshold (MB) | Above this is a “large file” (default 500) |
| AI | (see 7) | |
| Cleanup | Default: Recycle Bin | If off, the confirm window opens on “Delete permanently” |
| | Require typing DELETE | Extra confirmation for permanent delete |

**Save** writes `config.toml`. **Reset to defaults** resets the form (not applied until you save).

## 9. Command line

It also runs without the GUI (for servers or automation):

```powershell
draxbyte scan C:\Users\Me\Downloads --limit 20       # list
draxbyte scan --save                                  # save the scan (becomes the "last scan" in the GUI)
draxbyte clean C:\Temp                                # DRY RUN — deletes nothing
draxbyte clean C:\Temp --apply                        # move green items to the Recycle Bin
draxbyte clean C:\Temp --yellow --apply               # include yellow items
draxbyte clean C:\Temp --apply --permanent            # delete for good (asks you to type DELETE)
```

Environment variables: `DRAXBYTE_HOME` (config/data folder), `DRAXBYTE_API_KEY` (AI key).

## 10. FAQ / troubleshooting

**Why is `docker_data.vhdx` red?**
Big files changed in the last 24 h are protected as “possibly in use”. Docker/WSL disks are written constantly, so
they are always red. Shrink them with Docker's own tools.

**A file says “skipped: file is locked / in use”.**
Another program holds it open. Close that program and rescan.

**“skipped: changed since the scan”.**
The file changed between scan and cleanup; Draxbyte will not delete based on stale information. Rescan.

**A folder I expected is missing.**
It is below `min_size_mb`, matches an excluded pattern, or is a protected path (system folders are never shown).
Lower **Min. size** in Settings.

**The Age column shows “—”.**
The file's modified date was reset (some apps do this), so its age cannot be computed.

**Where are my settings?**
**About** shows the *Config file* and *Data folder*; **Open data folder** opens it. To reset, delete `config.toml`.

**Does anything go to the network?**
Not while AI is off. With AI on, only the buttons you press (**Fetch models / Test connection / Send**) make a
request. Links (like “get a key”) open in your browser only when you click them.
