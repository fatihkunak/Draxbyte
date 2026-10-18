# Scanner
Kod: `src/core/scanner.rs`. Bağlı: [[Rules]], [[Safety]], [[Config]], [[Db]].

- Kökün alt dizinleri rayon ile paralel gezilir; her alt ağaç ardışık `walkdir` (follow_links=false).
- `RuleEngine::wants_aggregate` true dönen klasör (node_modules, `.cxx`, Gradle `build`, cache dizinleri, bilinen temp konumunun doğrudan çocukları) `dir_stats` ile toplanıp TEK öğe olur, içine inilmez.
- Korumalı yollar ve exclude desenleri (`globset`, `**/` önekli) budanır. Sembolik bağ/junction atlanır.
- `Progress` atomikleri UI'ya canlı sayaç verir; `cancel` bayrağı her girişte kontrol edilir.
- Çakışan kökler yola göre tekilleştirilir. `min_size_mb` altı elenir.

- **Erişilemeyen yollar artık çıplak sayı değil** (`ScanErrors`, `classify_io_error`): her okunamayan yol nedeniyle kaydedilir — izin yok / başka program kullanıyor / diğer — ve ilk 400 örnek tutulur. **Tarama sırasında kaybolan dosya (geçici dosya, önbellek) hata sayılmaz** (eskiden sayılıyordu, "erişilemeyen" sayısını şişiriyordu). Klasör toplayıcı (`dir_stats_report`) da artık hatayı yutmaz. Windows'un ham hata kodları (5/32/33) yalnızca Windows'ta okunur (Unix'te 5 = EIO). Sonuçlar ekranı nedenleri ve çözümü gösterir: Windows'ta "Yönetici olarak yeniden aç", macOS'ta "Tam Disk Erişimi ayarlarını aç"; aynı yardım Disk Haritası ve temizlik raporunda da var. CLI: `draxbyte scan --show-errors`.
