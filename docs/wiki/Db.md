# Db
Kod: `src/db.rs` (rusqlite bundled). `~/.draxbyte/draxbyte.db`. `save_session` son 5 oturumu tutar (ON DELETE CASCADE). `log_clean` her temizlik girdisini yazar. Açılışta [[App]] son taramayı buradan yükler; `draxbyte scan --save` ([[Cli]]) buraya yazar.
