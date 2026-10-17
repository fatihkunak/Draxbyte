# SignPath Foundation application — draft answers

Apply at <https://signpath.org/apply> (free code signing for open-source projects).
Terms: <https://signpath.org/terms>. The form is filled in by the maintainer; the answers below are ready to paste.
Fields marked **«…»** need your own value.

> **Status:** not submitted. See "Honest assessment" at the bottom before applying.

## Project

| Field | Answer |
|---|---|
| Project name | **Draxbyte** |
| One-line description | Local-first disk scanner and cleaner for Windows. Finds large and useless files, never deletes without the user's approval, Recycle Bin + undo, optional AI (off by default). |
| Homepage | https://fatihgo.com/en/apps/draxbyte |
| Source repository | https://github.com/fatihkunak/Draxbyte |
| Download page | https://github.com/fatihkunak/Draxbyte/releases/latest |
| Documentation | https://github.com/fatihkunak/Draxbyte/wiki |
| License | MIT (OSI-approved), no dual licensing, no proprietary components |
| Language / stack | Rust (egui/eframe GUI, SQLite) |
| Artifact to sign | `draxbyte.exe` (single portable executable, ~16.6 MB, x86-64) |
| Product name in metadata | Draxbyte (embedded `ProductName`; version from `Cargo.toml`, currently 0.1.0) |
| Platforms | Windows (developed and tested on Windows 11) |
| Release status | Released: v0.1.0, 2026-10-17 |
| Maintainer / contact | Fatih Kunak · **«your e-mail»** · https://github.com/fatihkunak |

## Why the project needs signing

Draxbyte deletes files, so users rightly distrust an unknown executable. The v0.1.0 exe is currently signed with a
self-signed certificate (public certificate and SHA256 are published with the release). That proves integrity but
Windows SmartScreen still warns, which discourages exactly the careful users the tool is built for. A certificate
issued through SignPath Foundation would give the exe a publisher identity Windows can verify.

## Build process (SignPath requires an automated build from the repository)

- Source and build scripts are in the public repository; there are no binary dependencies checked in.
- `docs/signpath/ci.yml.example` is the CI workflow (GitHub-hosted `windows-latest`: `cargo fmt --check`, `cargo clippy`,
  `cargo test --locked`, `cargo build --release --locked`). **It is not active yet** — see the blocker below.
- `docs/signpath/release.yml.example` is the release workflow that will be enabled after approval: it builds the
  tagged commit on GitHub Actions, uploads the unsigned exe as an artifact, submits it to SignPath with
  `signpath/github-action-submit-signing-request` (manual approval of every release), and publishes the signed exe
  with a SHA256 checksum.
- The only binary produced is `draxbyte.exe`, built from the repository's own source (Rust crates are fetched from crates.io via the committed `Cargo.lock`).
- Build scripts and CI configuration live in the repository and are reviewed like source code.

## Team roles (SignPath asks for committers, reviewers, approvers)

| Role | Person |
|---|---|
| Owner, committer, reviewer, approver | Fatih Kunak (https://github.com/fatihkunak) — the project currently has a single maintainer |

Multi-factor authentication is enabled on **«GitHub account — confirm»** and will be enabled on the SignPath account.

## Privacy / network behaviour (to state on the download page)

Draxbyte does not collect telemetry, has no accounts and no update check. While the optional AI feature is off (the
default) it makes **no network requests**. When the user enables AI, only the buttons the user presses (fetch models,
test connection, send) contact the AI provider the user chose, and the exact text is shown before sending
(file path and metadata only, never file contents). Documented in the README ("Safety" and "AI") and the wiki page
"Safety and Privacy".

## Safety statement (no malware / PUA)

The program only reads files during a scan and only deletes after an explicit per-run confirmation; red-classified and
system paths cannot be deleted; every item is re-checked right before deletion; the default action is the Recycle
Bin. It contains no hacking, scanning or vulnerability tools.

## Reputation (links to put in the application)

- Repository: https://github.com/fatihkunak/Draxbyte (README in English and Turkish, user guide, wiki, CONTRIBUTING, SECURITY, CHANGELOG, issue templates)
- Release with checksum and public certificate: https://github.com/fatihkunak/Draxbyte/releases/tag/v0.1.0
- Author's site (apps portfolio, project page): https://fatihgo.com/en/apps/draxbyte
- 73 unit tests, CI on every push, MIT licence

## Honest assessment (read before applying)

- SignPath Foundation checks that **executables have a verifiable reputation**. Draxbyte is brand new (first release
  2026-10-17, no stars or third-party users yet), so an application today may be declined or deferred. It is cheap to
  try, but expect that answer. Growing real usage first (stars, issues, mentions, downloads) makes it much more likely.
- **Blocker: GitHub Actions does not run on the account right now.** The first CI run (2026-10-17) was refused with
  "The job was not started because your account is locked due to a billing issue." SignPath requires an automated,
  traceable build, so this must be resolved (GitHub account billing) before applying. Until then the CI file stays as an example so
  the repository does not show failing runs.
- The release workflow needs GitHub Actions + SignPath's GitHub integration. SignPath's pages describe it as the
  supported route, but check the current terms when applying.
- After approval, SignPath requires a **"Code signing policy"** statement on the home page and download/release
  pages, the credit line for SignPath.io / SignPath Foundation, a team-roles list and the privacy statement. A ready
  draft is in [`CODE_SIGNING_POLICY.draft.md`](CODE_SIGNING_POLICY.draft.md) — **do not publish it before approval**,
  because it claims signing that does not exist yet.
- The signed publisher name will be **SignPath Foundation**, not your own name.
- Alternatives while waiting: keep the self-signed signature + SHA256 (current), or distribute through the Microsoft
  Store (Microsoft signs the package).
