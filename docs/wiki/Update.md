# Update

- `src/update.rs`: `latest()` tek GET (`api.github.com/repos/fatihkunak/Draxbyte/releases/latest`), `parse_version` / `is_newer` (sayısal karşılaştırma: 0.10 > 0.9).
- Yalnızca kontrol; indirme/kurulum yok. Sonuç `App.update` (`UpdateState`): UpToDate / Available / Failed.
- Tetik: Hakkında → "Güncellemeleri kontrol et" (elle) veya `config.toml` `ui.check_updates = true` (açılışta). **Varsayılan kapalı** — "ağ isteği yok" vaadi korunur; kenar çubuğu metni ayara göre değişir.
- Yeni sürümde kenar çubuğunda "Yeni sürüm vX" bağlantısı.
- Gerçek ağ testi: `cargo test update::live -- --ignored`.
- Bağlı: [[Config]], [[Ui]], [[App]]
