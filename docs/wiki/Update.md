# Update

- `src/update.rs`: `latest()` tek GET (`api.github.com/repos/fatihkunak/Draxbyte/releases/latest`), `parse_version` / `is_newer` (sayısal karşılaştırma: 0.10 > 0.9).
- Yalnızca kontrol; indirme/kurulum yok. Sonuç `App.update` (`UpdateState`): UpToDate / Available / Failed.
- Tetik: Hakkında → "Güncellemeleri kontrol et" (elle) veya `config.toml` `ui.check_updates = true` (açılışta). **Varsayılan kapalı** — "ağ isteği yok" vaadi korunur; kenar çubuğu metni ayara göre değişir.
- Yeni sürümde kenar çubuğunda "Yeni sürüm vX" bağlantısı.
- Gerçek ağ testi: `cargo test update::live -- --ignored`.
- Bağlı: [[Config]], [[Ui]], [[App]]

## Uygulama içi güncelleme (indir + kur)
- Hakkında → yeni sürüm varsa "Şimdi güncelle" → satır içi onay → `download_setup`: `SHA256SUMS.txt` + `Draxbyte-Setup.exe` (yalnız `github.com/fatihkunak/Draxbyte/releases/download/` adresleri), akışlı indirme + SHA256, sonra Authenticode imza kontrolü (`Get-AuthenticodeSignature`; imzalayan parmak izi `SIGNER_THUMBPRINT` ile SABİTLENMİŞ, `NotSigned`/`HashMismatch` reddedilir; kendinden imzalı kök "UnknownError" olarak gelir, beklenen).
- Doğrulama geçerse `App.update_ready` → `launch_update` Setup'ı başlatır ve uygulamayı kapatır; `install-payload.ps1` eski sürümün çıkması için kopyalamayı 15 sn'ye kadar yeniden dener. Doğrulama başarısızsa dosya silinir, hata Hakkında'da görünür.
- Tarama/temizlik sürerken düğme devre dışı. Yalnız Windows.
- Sertifika yenilenirse `SIGNER_THUMBPRINT` güncellenmeli (aksi halde güncelleme reddedilir).
- Gerçek test: `cargo test update::live -- --ignored` (son release'in kurulum dosyasını indirir + doğrular, çalıştırmaz).
