# Safety
Kod: `src/core/safety.rs`. Bağlı: [[Cleaner]], [[Scanner]].

- `is_protected` (ağaç): Windows dir (Temp hariç), Program Files, `/usr` `/etc`…, `~/.ssh/.gnupg/.aws/.kube`, kendi veri dizini, dosya sistemi kökü.
- `is_protected_item`: + home ve standart kullanıcı klasörlerinin kendisi (içeriği temizlenebilir).
- `is_locked`: Windows'ta exclusive open denemesi (sharing violation 32/33); diğer OS'ta false.
- `pre_delete_check`: Red/korumalı/yok/symlink/kilitli/taramadan sonra değişmiş (dosya: mtime+boyut; klasör: içinde daha yeni dosya) → atla.
