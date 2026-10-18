# DECISIONS

- **lib + bin ayrımı.** `src/lib.rs` çekirdeği açar, GUI/CLI ikili dosyada. Neden: çekirdek GUI'siz test edilebilir (CLI `--save` ile gerçek veri üzerinde de denendi). İstenen dosya yapısından tek sapma budur.
- **Ad: Draxbyte** (config'teki `DISKSWEEPER_API_KEY` -> `DRAXBYTE_API_KEY`, `~/.disksweeper` -> `~/.draxbyte`).
- **egui 0.37 / eframe glow.** wgpu reddedildi: daha büyük binary, bu iş için gereksiz.
- **reqwest + rustls.** Native-tls Linux'ta OpenSSL dev paketi ister; tek-binary hedefine aykırı.
- **Klasör toplama (aggregate).** node_modules/Gradle build gibi klasörler tek öğe; içine inilmez. Reddedilen: her dosyayı ayrı listelemek (gerçek taramada 65k dosyalık projede onlarca "system_file" gürültüsü çıktı).
- **`.dll/.so` Red değil.** İlk sürümde Red'ti; gerçek taramada build çıktılarını sistem dosyası diye işaretlediği görüldü, kaldırıldı. Sistem olanlar korumalı ağaçlarla zaten dışarıda.
- **Kırmızı = seçilemez.** Kullanıcı hatasıyla bile silinemez (UI + `pre_delete_check` iki katman).
- **Token sayacı cache temizlenince sıfırlanmaz.** Aksi halde limit "Önbelleği temizle" ile atlatılırdı.
- **Bırakılan:** Inter fontu (dosya paketlenmedi), marka sayfasındaki D-şimşek logosu (düz "D" çizildi), macOS geri alma (`trash` crate'i desteklemiyor), AI'nın gerçek uç noktayla denenmesi (anahtar yok; yalnız birim testi).
- **Çoklu sağlayıcı (10-17).** Yazarın başka bir projesinde kullandığı sağlayıcı listesi alındı (+ Ollama, LM Studio). Model kimlikleri çabuk eskidiği için varsayılan yalnız başlangıç; liste `GET /models` ile çekilir. Reddedilen: modelleri koda gömmek.
- **Anahtarlar config.toml'da düz metin** (sağlayıcı başına). Reddedilen: Windows Credential Manager (platformlar arası tek-binary hedefini bozar); ortam değişkeni öncelikli ve UI'da öneriliyor.
- **Yerel sağlayıcıda http serbest**, diğerlerinde https zorunlu (anahtar düz metin gitmesin).
- **Test isteği sayaca işlenir** (~30 token) — "Bağlantıyı test et" sınırı atlatma yolu olmasın.
- **Inter Regular + SemiBold** gömüldü (~830 KB, OFL; lisans `assets/fonts/Inter-LICENSE.txt`). Değişken font reddedildi (egui'de ağırlık seçilemez).
- **Simge konsept sayfasından türetildi** (renk maskesiyle izlenip vektör gibi yeniden çizildi), kafadan tasarlanmadı.
- **Çift dil: satır içi `t("TR","EN")`, dosya tabanlı sözlük değil (10-17).** Neden: iki dil, ~250 metin; anahtarsız yaklaşım çeviriyi çağrı yerinde tutar ve eksik çeviri derleme anında görünür (iki argüman zorunlu). Reddedilen: gettext/fluent (bağımlılık + tek-binary hedefine ağır). Kural gerekçeleri DB'de Türkçe saklanır ama UI kural kimliğinden çevirir.
- **Dil otomatik algılanır** (Windows locale), `tr*` dışı her şey English. Reddedilen: sabit Türkçe — kamuya açık repoda İngilizce varsayılan daha doğru.
- **İmza: kendinden imzalı sertifika (10-17).** Gerçek sertifika yok (OV/EV ücretli, kimlik doğrulama ister). Kendinden imzalı imza yalnız bütünlüğü kanıtlar, SmartScreen'i susturmaz; README bunu açıkça söyler. Özel anahtar yalnız kullanıcının Windows sertifika deposunda, depoya/çıktıya girmez; açık sertifika `assets/draxbyte-codesign.cer`. Sonraki adım olası: SignPath Foundation (açık kaynak projelere ücretsiz imza).

## Sistem araçları (10-18)
- **Tarama kapsamı:** her dahili sürücü + sistem temp/log/cache klasörleri varsayılan açık (`scan.all_drives`, `scan.include_system`). Neden: kullanıcı "atlanan dizin olmasın" dedi; ölçülen açık ~3,6 GB'tı (Geri Dönüşüm Kutusu, CBS, Package Cache…). Tam C: taraması bu makinede 3,3 M dosya, ~1 dk 50 sn. Reddedilen: yalnız `~` + temp.
- **`max_depth` 10 → 64 + `config_version` göçü.** Eski config'te saklı "10" bir seçim değil eski varsayılandı; yoksa yeni varsayılan hiç devreye girmezdi. 10'dan başka değerlere dokunulmaz.
- **Kırmızı kural değişmedi:** `C:\Windows`, `System32`, sürücü kökü, `~/.ssh`… hâlâ silinmez. Program kaldırma bunu delmez; kaldırmanın kendi, ayrı ve çift (arama + silme anı) korumaları var (`leftover_path_allowed`, `reg_key_allowed`).
- **Kaldırma sırası:** üreticinin kaldırıcısı → artık taraması → kullanıcı seçimi → silme. Reddedilen: kaldırıcıyı atlayıp doğrudan silmeyi varsayılan yapmak (yalnız kaldırıcı yoksa/engelliyse "zorla").
- **Kayıt defteri her zaman yedekli:** `.reg` dışa aktarımı başarısızsa anahtar silinmez. Reddedilen: yedeksiz silme; sistem genelinde "temizleyici" tarzı kayıt defteri taraması (yalnız bir programa bağlı anahtarlar).
- **Kalıcı silme varsayılan açık** (kaldırma artıkları): amaç diski hemen boşaltmak; her öğe zaten tek tek onaylı ve kayıt defteri yedekli. Çöp Kutusu kutuyla seçilebilir.
- **"Olası" eşleşme (paylaşılan yayıncı klasörü, tarayıcı profili, sandbox Containers) seçili gelmez ve işaretlenirse ayrıca onay ister.** Test sırasında bir kullanıcı bu kutuları elle işaretledi ve silindi; ikinci onay tam bu kazaya karşı.
- **Edge kaldırılabilir** (`--force-uninstall` + Windows Update engeli); kullanıcı kararı. **WebView2 sihirbazın dışında**, çünkü Office/Teams/Steam gibi programlar ona bağlı. Reddedilen: sessiz/otomatik Edge kaldırma.
- **RAM "optimizasyonu" yalnız `EmptyWorkingSet`, arayüzde geçici olduğu yazılı.** Reddedilen: kalıcı iyileştirme vaadi. Gerçek kazanç: bellek yiyeni kapatmak + başlangıcı azaltmak.
- **Süreç kapatma PID bazlı, isimle toplu değil;** sistem listesi + kullanıcı "koru" listesi (`memory.protected`). Neden: bu makinede başka projelerin 7/24 servisleri çalışıyor.
- **Son kullanım tahmini:** UserAssist + Prefetch + BAM, arka plan exe'leri sayılmaz, kayıt yoksa "bilinmiyor". Reddedilen: kurulum tarihini kullanım saymak.
- **macOS mühürlü sistem uygulamaları (Safari, Mail…) silinmez;** yalnız önbellek. Reddedilen: SIP/mühür kapatma talimatı otomasyonu (Kurtarma modu gerektirir, güncellemeleri bozar, Mac'te denenemez).
- **Bırakılan:** çift dosya bulucu, disk haritası, geliştirici önbellek kuralları (npm/pip/cargo registry), Mac GUI'sinin gerçek Mac'te elle denenmesi, Windows'ta yönetici gerektiren servis/HKLM silmenin gerçek (yükseltilmiş) makinede denenmesi.
