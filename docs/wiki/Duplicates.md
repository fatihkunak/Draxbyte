# Duplicates
Kod: `src/core/dupes.rs`, durum/işler `src/map_dupes.rs`, arayüz `src/ui/dupes_view.rs`, CLI `draxbyte dupes`. Bağlı: [[Cleaner]], [[Safety]].

Birebir aynı içerikli dosyaları bulur. Algoritma ucuzdan pahalıya: **boyut → ilk+son 64 KB'ın SHA-256'sı → tüm dosyanın SHA-256'sı**; her aşamada tek kalan elenir, yani çoğu dosya hiç tam okunmaz. `rayon` ile paralel, iptal edilebilir, canlı aşama sayaçları.
- Varsayılan: ≥ 1 MB, `~` altında; korumalı ağaçlar (`is_protected`) gezilmez. **Geliştirme klasörleri atlanır** (`node_modules`, `.git`, `target`, `__pycache__`, `.gradle`, **`intermediates`, `.cxx`, `DerivedData`, `Pods`, `.next`…**): oradaki kopyalar bilerek vardır ve silmek derlemeyi bozar. Gerçek tarama bunu gösterdi (en büyük "kopyalar" Android CMake çıktılarıydı).
- **Her gruptan en az bir kopya kalır:** arayüzde son kalan kopyanın kutusu işaretlenemez (`GroupUi::set`), CLI `keep_index` ile birini her zaman ayırır, `to_delete` korunanı asla içermez.
- Kural: en eskiyi / en yeniyi / en kısa yolu koru (`Keep`).
- Silme mevcut `cleaner` üzerinden (Çöp Kutusu varsayılan, kalıcı ayrı onay); öğeler sarı `ScanItem`, `pre_delete_check` **taramadan sonra değişen dosyayı atlar** (testli).
- Arayüz yalnızca en çok yer harcayan ilk **300 grubu** gösterir ve silme **tam olarak görünenlere** uygulanır (görmediğin hiçbir şey silinmez).
- Bilinen sınır: sabit bağlantılar (hardlink) aynı veri sayılmaz, ayrı kopya görünebilir.
- Mac/Windows: saf Rust, platformdan bağımsız; CI duman testi gerçek Mac'te bir kopya çiftini bulur, kaldırır, benzersiz dosyaya dokunmaz.
