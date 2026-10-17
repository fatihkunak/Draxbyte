# Ui
Kod: `src/ui/` (mod = tema+kabuk, scan_view = ana sayfa+ilerleme, result_view = tablo+modallar+temizlik, settings_view = ayarlar+önbellek, widgets). Palet marka sayfasından: #10B981 / #1A1D21 / #2B2F36.
GOTCHA: varsayılan egui fontunda `●` `▶` `▲▼` yok (kutucuk çıkar) → `•`, `^`, `v` kullan. Bağlı: [[App]], [[Cleaner]], [[AiClient]].
Hakkında ekranı: `about_view.rs` (geliştirici bilgisi sabitleri dosyanın başında). Yazı tipi: Inter (`assets/fonts`, OFL), `ui::bold()` SemiBold. Simge: `scripts/make_icon.py` konsept sayfasından üretir; `build.rs` exe'ye gömer.
