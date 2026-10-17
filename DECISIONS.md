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
