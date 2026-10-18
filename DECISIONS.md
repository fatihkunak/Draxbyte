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

## Sihirbaz, kopya bulucu, disk haritası (10-18)
- **Kullanıcı kuralı: her değişiklik Mac + Windows.** `CLAUDE.md`'ye yazıldı; platforma özel kod `cfg` ile ayrılır, karşılığı ya da dürüst "desteklenmiyor" olur, saf mantık her yerde test edilir, Mac CI'da doğrulanır.
- **Setup.exe = aynı program.** IExpress + PowerShell yükleyici reddedildi: ikinci bir araç zinciri, MessageBox'lı arayüz ve markadan kopuk. Aynı ikili ad/bayrakla sihirbaz açar (tema, logo, iki dil bedava), güncelleyicinin SHA256 + imza doğrulaması aynen çalışır. Bedeli: Setup ~6 MB yerine ~18 MB.
- **Kaldırma yalnızca kendi dosyalarını siler,** seçilen klasörü komple değil: kullanıcı yanlışlıkla `Belgelerim`e kurarsa kaldırma onu silmesin. Kurulum klasörü ayrıca doğrulanır (kişisel/korumalı olmaz).
- **Çalışan sürümün üzerine kurulum = yeniden adlandır + kopyala.** Windows çalışan exe'nin üzerine yazmaz ama adını değiştirmeye izin verir; yeni sürüm bir sonraki açılışta devreye girer, uygulamayı zorla kapatmak gerekmez.
- **Mac'te kurulum = `.app`'i Applications'a kopyala** (DMG'den açılınca otomatik). Karantina bayrağı **kaldırılmaz**: Gatekeeper'ı sessizce atlatmak kullanıcının onayı olmadan güvenlik denetimini delmek olur.
- **Test hazards:** bir testim gerçek `~/.draxbyte`'ı silmeye kalktı (dosya kilitli olduğu için kurtuldu). Çözüm: silinecek veri klasörü ve ev klasörü `Target` ile enjekte edilir; sihirbaz için ayrı kum havuzu modu. Kural: **veri silen hiçbir kod yolu testte gerçek konumu görmemeli.**
- **Kopya bulucu:** boyut → ilk/son 64 KB → tam SHA-256. Geliştirme/derleme ara klasörleri varsayılan atlanır (gerçek tarama, en büyük "kopyaların" Android CMake çıktısı olduğunu gösterdi). Her gruptan en az bir kopya kalır (arayüz + çekirdek). Silme yalnızca görünen ilk 300 gruba uygulanır. Hardlink'ler ayrıştırılmaz (bilinen sınır).
- **Disk haritası salt okunur,** bellek sınırlı ağaç (en büyük 64 çocuk, ≥ 8 MB; kalanı "küçük öğe"). Reddedilen: tüm dosyaları bellekte tutmak (tam sürücüde yüzlerce MB).
- **Bırakılan:** kopya bulucuda içerik-bazlı "benzer resim/müzik" yakalama, disk haritasında silme, kopyaları sabit bağlantıya çevirme, kopya ekranının gerçek pencerede görsel doğrulanması (pencere küçültülmüştü).

## Erişim (10-19)
- **Windows'ta açılışta yönetici izni varsayılan** (`ui.always_admin = true`; kullanıcı kararı: "benim bilgisayarımda erişemediğim bölüm olmamalı"). Çalışma anında yeniden başlatma (`relaunch_elevated`), `requireAdministrator` manifesti **değil**: manifest her `cargo test`'i, terminaldeki her `draxbyte scan`'i ve (kullanıcıya özel) kurulum sihirbazını da UAC'ye takardı. UAC reddedilirse uygulama normal açılır; sihirbazlar ve CLI hiç yükseltmez; `DRAXBYTE_NO_ELEVATE=1` otomasyon içindir. Bilinen sınır: standart kullanıcı başka bir yönetici hesabının parolasıyla yükseltirse süreç o hesabın profilinde çalışır (bu bilgisayarda hesap yönetici, aynı profil).
- **"Erişilemeyen" artık bir sayı değil bir tanı.** Neden (izin / kullanımda / diğer), örnek yollar ve tek tıkla çözüm. Kaybolan dosya hata değildir. Mac'te karşılığı Tam Disk Erişimi yönlendirmesi (yönetici kavramı yok). Korumalı sistem yolları (Windows klasörü vb.) bilerek taranmaz ve "erişilemeyen" sayılmaz; Disk Haritası onları gösterir.

## Tam Optimizasyon (2026-10-19)
- Seçilen: mevcut araçları tek plana bağlayan sihirbaz; her silme `cleaner::clean` üzerinden (kırmızı + `pre_delete_check` atlanamaz). Onay kutusu olmadan başlatılamaz.
- Reddedilen: başlangıç programlarını/programları otomatik kapatma-kaldırma (geri dönüşü zor, karar kullanıcının); `docker system prune` (birimleri ve konteynerleri toplu siler); Docker vhdx sıkıştırma (WSL/Docker'ı kapatır, 7/24 konteynerleri öldürür): yalnız boyutu ve elle yolu gösterilir.
- Docker riskli parçalar (durmuş konteyner, birim) varsayılan KAPALI; listeler onay ekranında.

## Docker disk imajı küçültme (2026-10-19, önceki 'reddedildi' kararının düzeltmesi)
- İlk karar: eklenmedi (konteynerleri durdurur, kazanç az görünüyordu). Kullanıcı elle yapınca 60 → 20,7 GB (+39 GB boş yer) çıktı ve konteynerler `docker desktop stop` ile açıkça durduğu için `unless-stopped` olsalar da KENDİLİĞİNDEN gelmedi (6'sı elle başlatıldı).
- Seçilen: varsayılan KAPALI ayrı adım; önce konteyner listesi kaydedilir, sonda her biri eski sırayla başlatılıp doğrulanır (4 tur yeniden deneme); Docker kapanmazsa hiçbir şey yapılmaz; diskpart `detach` hata olsa da çalışır; sürece isimle öldürme yok (yalnız Docker'ın kendi `docker desktop stop` komutu ve `wsl --shutdown`).
- Bilinen sınır: sıkıştırmanın tam akışı Rust'tan canlı çalıştırılmadı (yalnız saf parçalar ve yeniden başlatma mantığı testli); aynı komutlar elle doğrulandı. macOS'ta Docker boşalan alanı kendisi geri verir, adım 'desteklenmiyor' der.

## AI Danışman (2026-10-19)
- Seçilen: ayrı `[[ai.chain]]` listesi (boşsa eski tekli sağlayıcı); her istek birincilden başlar (yapışkan 'son çalışan' yok, kullanıcı isteği böyle); tüm hatalar sıradakine geçirir, yalnız günlük limit geçirmez.
- Reddedilen: genel `DRAXBYTE_API_KEY`'i zincirde kullanmak (bir sağlayıcının anahtarını yedeğe gönderirdi); sohbet geçmişini diske yazmak; danışmana silme/komut yetkisi vermek; tarama özetini onaysız göndermek.
- Bilinçli sınır: gerçek bulut sağlayıcıyla (OpenAI vb.) canlı deneme yapılmadı, anahtarlarına dokunulmadı; istek biçimleri mevcut sağlayıcı katmanının (Anthropic + OpenAI-uyumlu) aynısı.
