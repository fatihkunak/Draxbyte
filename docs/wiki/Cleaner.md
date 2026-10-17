# Cleaner
Kod: `src/core/cleaner.rs`. Bağlı: [[Safety]], [[Db]], [[Ui]].

- `clean(items, opts{dry_run,permanent}, cancel, progress)` → `CleanReport`. Her öğe önce `pre_delete_check`.
- Varsayılan `trash::delete`; `permanent` ise `remove_file/remove_dir_all`.
- `undo(report)`: Windows/Linux'ta `trash::os_limited` ile geri yükler; macOS'ta desteklenmez.
- Gerçek Geri Dönüşüm Kutusu turu `tests/trash_roundtrip.rs` (`--ignored`).
- Windows kabuğu klasörü toptan iptal ederse (`Some operations were aborted`: kilitli iç dosya / uzun yol) `trash_children` çocukları tek tek taşır; hepsi taşınırsa `Trashed`, kalan varsa `Failed("kısmen temizlendi…")`. Kısmi taşınanlar `undo`da yer almaz (Çöp Kutusu’ndan elle).
