# Draxbyte

Rust + egui yerel-öncelikli disk tarama/temizleme uygulaması. Kurulum, config, CLI: `README.md`.

## ⚠️ WIKI FIRST (MUTLAK KURAL)
Projeye her başladığında, herhangi bir kod okuma/grep/glob YAPMADAN ÖNCE `docs/wiki/` klasörünü tara (`Index.md` → ilgili node). Proje bilgisi orada; kod taramaya wiki'den sonra geç. `docs/wiki/_build/map.json` her node'u kod dosyasına eşler (`node docs/wiki/_build/map-nodes.mjs` ile üretilir). Kesinleşen her değişiklikten sonra ilgili wiki node'unu ve `Changelog.md`'yi güncelle.

## Komutlar
- `cargo test` (birim testleri) · `cargo test --test trash_roundtrip -- --ignored` (gerçek Çöp Kutusu, isteğe bağlı)
- `cargo build --release` → `target/release/draxbyte.exe`

## Değişmez kurallar
- Tarama/kural motoru ağ kullanmaz. AI varsayılan kapalı; her istek kullanıcı onaylı; önbellek zorunlu.
- Kırmızı öğe asla silinmez; silme öncesi `pre_delete_check` atlanamaz.
