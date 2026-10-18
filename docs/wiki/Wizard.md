# Wizard
Kod: `src/wizard.rs` (arayüz), `src/core/installer.rs` (iş mantığı). Bağlı: [[Update]], [[Programs]], [[Ui]].

**Kurulum ve kaldırma sihirbazı = aynı program.** `Draxbyte-Setup.exe`, `draxbyte.exe`'nin imzalı kopyasıdır; adında "setup" geçen dosya (ya da `--setup`) kurulum sihirbazını, `--uninstall` kaldırma sihirbazını açar. IExpress/PowerShell yok; tema, logo, iki dil uygulamayla aynı. Mod `main()`'de CLI ayrıştırılmadan önce seçilir (`detect_mode`).

## Windows
- **Kurulum** (kullanıcıya özel, yönetici gerekmez): Hoş geldin → Lisans (MIT, kabul kutusu) → Seçenekler (klasör, masaüstü/Başlat kısayolu, "bitince aç") → Kurulum → Bitti. Mevcut kurulum varsa **Güncelleme** kipi: Hoş geldin → Kurulum → Bitti (klasör ve kısayol seçimleri korunur).
- Adımlar: klasörü doğrula (`validate_dir`: korumalı/kişisel klasör olmaz) → exe'yi kopyala (açık sürüm **yeniden adlandırılıp** `.old`'a alınır, yenisi bir sonraki açılışta devreye girer) → kısayollar (PowerShell `WScript.Shell`) → "Uygulamalar ve özellikler" kaydı (`UninstallString = "<exe>" --uninstall`).
- **Kaldırma:** onay (+ "ayarları ve verileri de sil": `~/.draxbyte`) → çalışan kopyaları kapat → kısayol + kayıt → **yalnızca kurulumun koyduğu dosyalar** silinir (`draxbyte.exe`, `.old`, `uninstall.ps1`), klasör boşsa kaldırılır (içine başkasının dosyası konduysa kalır). Kaldırma, kendi klasörünü silebilmek için geçici bir kopyadan (`--stage2`) çalışır; bitince kopya `cmd` ile silinir.
- Uygulama içi güncelleme (`update.rs`) aynı `Draxbyte-Setup.exe`'yi indirir, SHA256 + imzayı doğrular, sihirbaz güncelleme kipinde açılır.

## macOS
- **Kurulum:** `Draxbyte.app` DMG'den (`/Volumes/…`) açılınca sihirbaz otomatik başlar; uygulamayı `/Applications` (ya da `~/Applications`) içine `ditto` ile kopyalar (eskisini değiştirir). İlk açılışta macOS onay isterse: sağ tık → Aç (karantina bayrağı **kaldırılmaz**).
- **Kaldırma:** `.app` silinir (root'a aitse sistem parola penceresi), "veriyi sil" işaretliyse `~/.draxbyte` ve `~/Library` altındaki `com.fatihkunak.draxbyte` artıkları (mevcut Mac artık mantığı yeniden kullanılır). Başka bundle id'li uygulamaya asla dokunmaz.
- Kısayol/kayıt defteri adımları yok (Mac'te karşılığı yok).

## Güvenlik ve test
- Testlerin gerçek kuruluma/`~/.draxbyte`'a dokunması **imkânsız** olsun diye dış konumlar `Target` ile enjekte edilir (`Target::real()` / `Target::sandbox(dir)`); `DRAXBYTE_WIZARD_SANDBOX=<klasör>` sihirbazı tamamen o klasörde çalıştırır (kayıt anahtarı `DraxbyteSandbox`).
- Windows: sahte hedefte kur → güncelle → kaldır turu; Mac mantığı (bundle bul, kur, güncelle, sahte `~/Library` ile kaldır, başka id'yi reddet) Windows'ta bile test edilir (`mac_flow`), gerçek Mac'te CI'da koşar.
- Arayüz gerçek pencerede (kum havuzu) kuruldu ve kaldırıldı; gerçek kurulum, kayıt ve veri klasörü dokunulmadı.
