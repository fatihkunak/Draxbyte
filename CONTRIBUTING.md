# Contributing / Katkıda bulunma

Thanks for your interest! Bug reports, ideas and pull requests are welcome. · Hata bildirimi, fikir ve PR'lar hoş karşılanır.

## Report a bug / Hata bildir
Open an [issue](https://github.com/fatihkunak/Draxbyte/issues/new/choose) and include: Windows version, Draxbyte version (**About** screen), what you did, what you expected, what happened. A screenshot of the **Cleanup** report helps. **Please do not paste API keys or private paths.**

## Build and test
```powershell
git clone https://github.com/fatihkunak/Draxbyte.git
cd Draxbyte
cargo test                      # unit tests
cargo clippy --all-targets
cargo fmt
cargo run --release             # GUI
```
Optional checks: `cargo test --test trash_roundtrip -- --ignored` (real Recycle Bin), `DRAXBYTE_LIVE_URL=... cargo test --test live_provider -- --ignored` (a real OpenAI-compatible server).

## Ground rules
1. **Safety first.** Nothing that can delete must skip `pre_delete_check` (`src/core/safety.rs`). Red items stay unselectable. New "safe" rules need a test.
2. **No network without the user.** Scanning and rules stay offline; AI stays off by default, per-request approved, cached and limited. New network calls must be user-initiated.
3. **Two languages.** Every user-visible string needs Turkish and English (`t("TR", "EN")` / `tf!`). A new rule id needs an entry in `i18n::rule_reason` (a test enforces this).
4. **Small, tested changes.** Add or update unit tests; keep `cargo test` green and `cargo clippy` quiet.
5. Read the architecture notes in [`docs/wiki/Index.md`](docs/wiki/Index.md) and the reasons behind decisions in [`DECISIONS.md`](DECISIONS.md) first.

## Pull requests
Branch from `main`, keep the PR focused, describe *what* and *why*, mention how you tested it (and on which OS). By contributing you agree your work is licensed under the [MIT License](LICENSE).

## Ideas that would help
- Linux / macOS testing (the code is platform-aware but untried there)
- More rules (browser caches, package-manager caches, game launchers) with tests
- More languages (see `src/i18n.rs`)
- Authenticode signing with a trusted certificate (e.g. SignPath Foundation)
