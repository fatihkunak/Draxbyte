# Code signing policy — DRAFT (publish only after SignPath Foundation approves the project)

When approved: publish this text as `CODE_SIGNING_POLICY.md`, link it from the README, the project home page
(https://fatihgo.com/en/apps/draxbyte) and every release page, and add the credit line below to the README.

---

## Code signing policy

Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).

### What is signed
Only `draxbyte.exe`, built from the source code in this repository by the GitHub Actions workflow
`.github/workflows/release.yml` on a GitHub-hosted runner, from a tagged commit. Nothing built elsewhere is signed.
The signed file carries the product name **Draxbyte** and the version from `Cargo.toml`.

### Team roles

| Role | Member |
|---|---|
| Committers and reviewers | [Fatih Kunak](https://github.com/fatihkunak) |
| Approvers (every release is approved manually in SignPath) | [Fatih Kunak](https://github.com/fatihkunak) |

All members use multi-factor authentication for GitHub and SignPath.

### Privacy policy
This program will not transfer any information to other networked systems unless specifically requested by the user
or the person installing or operating it. The optional AI feature is off by default; when the user turns it on, only the
actions the user triggers (fetching a model list, testing the connection, sending a request they approved) contact the
AI provider the user selected, and the text to be sent is shown first. No telemetry, accounts or update checks.
See also: [Safety and Privacy](https://github.com/fatihkunak/Draxbyte/wiki/Safety-and-Privacy).

### Installation and removal
Draxbyte is a single portable executable with no installer; it does not change system settings. The optional
`scripts/install.ps1` copies it to `%LOCALAPPDATA%\Programs\Draxbyte` and creates shortcuts; to remove it, delete that
folder and the shortcuts. Settings and data live in `~/.draxbyte` (or `DRAXBYTE_HOME`) and can be deleted at any time.

### Reporting a problem
Report suspected policy violations or security issues privately via the repository's
[security advisories](https://github.com/fatihkunak/Draxbyte/security/advisories/new). See [SECURITY.md](../../SECURITY.md).
