# DiskMap
Kod: `src/core/diskmap.rs`, durum `src/map_dupes.rs`, arayüz `src/ui/diskmap_view.rs`, CLI `draxbyte map`. Bağlı: [[Scanner]], [[SysLocs]].

Alan neyin altında? Bir klasörün **boyut ağacı** + **squarified treemap** (Bruls–Huizing–van Wijk): kutu alanı boyutla orantılı, kutular mümkün olduğunca kareye yakın.
- **Sınırlı bellek:** her klasör yalnızca en büyük `max_children` (64) ve en az `min_keep` (8 MB) olan çocuklarını tutar, kalanı tek bir "küçük öğe" kutusunda toplar. Tam C: sürücüsü bile küçük bir ağaç olur. Toplam her zaman çocukların toplamına eşittir (testli).
- Sembolik bağ/junction izlenmez; iptal edilebilir; sürücü büyüklüğüne göre dakikalar sürebilir, hiçbir şey değişmez.
- **Etkileşim:** kutuya gel → ayrıntı satırı (yol, boyut, %, dosya sayısı); klasöre tıkla → içine gir; ekmek kırıntısı/Yukarı ile geri; altta en büyük 14 öğe listesi ("Aç", "Göster" = dosya yöneticisinde göster; Windows Explorer `/select`, macOS `open -R`, Linux klasörü). Renk: klasörler paletten, dosyalar türüne göre (video/resim/ses/…).
- Disk haritası **salt okunurdur**; silme yok (silmek için Tarama/Kopya Dosyalar/Programlar).
- Testler: `squarify` (alanlar orantılı, sınır içinde, çakışma yok, kareye yakın, kenar durumları) ve ağaç (toplamlar, katlama, çocuk sınırı, iptal) her platformda koşar.
