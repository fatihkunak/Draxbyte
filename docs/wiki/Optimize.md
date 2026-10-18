# Optimize
Kod: `src/core/optimize.rs` (plan, analiz, uygulama), `src/core/docker.rs` (Docker parçaları), durum `src/optimize.rs`, arayüz `src/ui/optimize_view.rs`, CLI `draxbyte optimize`. Bağlı: [[Cleaner]], [[Duplicates]], [[SysLocs]], [[Memory]], [[Safety]].

**Tam Optimizasyon sihirbazı**: ayrı araçları tek akışta çalıştırır. 1 Kapsam → 2 Analiz → 3 Onay → 4 Uygulama → 5 Sonuç. Analiz yalnız okur; onay kutusu işaretlenmeden "Başlat" açılmaz.

## Parçalar (çalışma sırası)
1. **Güvenli temizlik**: tarama sonucundaki yalnızca YEŞİL öğeler, Çöp Kutusu'na (`select_cleanup`). Sarı/kırmızıya dokunulmaz; her öğe `pre_delete_check`'ten geçer.
2. **Kopya dosyalar**: her gruptan EN ESKİ kalır, fazlalar Çöp Kutusu'na; en çok yer harcayan `DUPE_GROUP_CAP` (300) grupla sınırlı, yani onaylanan sayı = silinen sayı.
3. **Docker temizliği** (parça parça, `docker::Part`): durmuş konteynerler*, build önbelleği (`builder prune -a`), etiketsiz imajlar, kullanılmayan tüm imajlar*, ağlar, birimler*. `*` = veri kaybı riski var, varsayılan KAPALI; listesi onay ekranında. Hiçbir zaman `system prune` yok, çalışan konteynere dokunulmaz. Docker'ın disk imajı (vhdx/raw) temizlikle küçülmez; arayüz bunu boyutuyla birlikte söyler.
4. **Çöp Kutusu'nu boşalt**: kalıcı, geri alma yok (arayüz kırmızı uyarır). Önceki adımlar bunun için Çöp Kutusu'nda biriktiği için sondan bir önce çalışır.
5. **Belleği boşalt**: yalnız Windows, geçici etki, program kapatmaz.

Başlangıç programları ve kullanılmayan programlar bilerek otomatik değiştirilmez; sonuç ekranı ilgili ekranlara götürür.

## Notlar
- `docker_estimate`: Docker'ın "reclaimable" rakamlarından tahmin; etiketsiz imajlar "tüm kullanılmayan imajlar" seçiliyken iki kez sayılmaz.
- Sonuç ekranı "Geri al" (yalnız Çöp Kutusu boşaltılmadıysa).
- Mac: bellek adımı "desteklenmiyor" olarak atlanır; Docker disk imajı `~/Library/Containers/com.docker.docker/...`; Çöp Kutusu boşaltma `recycle` modülünde.
- CLI: `draxbyte optimize [--apply] [--skip-cleanup|--skip-dupes|--skip-docker|--keep-bin|--skip-memory] [--docker-parts build,dangling,networks,stopped,images,volumes] [--dupe-path P] [--dupe-min-mb N]`; `--apply` yoksa kuru çalıştırma.
- Demo/test: `DRAXBYTE_VIEW=optimize`, `DRAXBYTE_OPT_DEMO=1` (Docker + Çöp Kutusu analizini hemen başlatır).
