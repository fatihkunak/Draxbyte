# Safety
Kod: `src/core/safety.rs`. Bağlı: [[Cleaner]], [[Scanner]].

- `is_protected` (ağaç): Windows dir (Temp hariç), Program Files, `/usr` `/etc`…, `~/.ssh/.gnupg/.aws/.kube`, kendi veri dizini, dosya sistemi kökü.
- `is_protected_item`: + home ve standart kullanıcı klasörlerinin kendisi (içeriği temizlenebilir).
- `is_locked`: Windows'ta exclusive open denemesi (sharing violation 32/33); diğer OS'ta false.
- `pre_delete_check`: Red/korumalı/yok/symlink/kilitli/taramadan sonra değişmiş (dosya: mtime+boyut; klasör: içinde daha yeni dosya) → atla.
- macOS: `*.app` / `*.photoslibrary` / `*.musiclibrary` / `*.tvlibrary` paketlerinin içi korumalı (`inside_mac_bundle`); `~/Library` altında Mobile Documents (iCloud), CloudStorage, Keychains, Mail, Messages, Containers, Group Containers ağaç olarak korumalı (silmek bulut kopyasını da siler). `~/Library/Caches` temizlenebilir kalır. Mac üzerinde henüz denenmedi: `.github/workflows/macos-dmg.yml` runner üzerinde test + `scripts/macos/smoke.sh` koşturur.
