# Scanner
Kod: `src/core/scanner.rs`. Bağlı: [[Rules]], [[Safety]], [[Config]], [[Db]].

- Kökün alt dizinleri rayon ile paralel gezilir; her alt ağaç ardışık `walkdir` (follow_links=false).
- `RuleEngine::wants_aggregate` true dönen klasör (node_modules, `.cxx`, Gradle `build`, cache dizinleri, bilinen temp konumunun doğrudan çocukları) `dir_stats` ile toplanıp TEK öğe olur, içine inilmez.
- Korumalı yollar ve exclude desenleri (`globset`, `**/` önekli) budanır. Sembolik bağ/junction atlanır.
- `Progress` atomikleri UI'ya canlı sayaç verir; `cancel` bayrağı her girişte kontrol edilir.
- Çakışan kökler yola göre tekilleştirilir. `min_size_mb` altı elenir.
