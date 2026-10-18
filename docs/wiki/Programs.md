# Programs
Kod: `src/core/apps/{mod,win,mac}.rs`, arayüz `src/ui/apps_view.rs`, iş mantığı `src/panels.rs`. Bağlı: [[Safety]], [[Memory]], [[Cli]].

Kurulu programları listeler, son kullanımını tahmin eder, **üreticinin kaldırıcısını çalıştırır, sonra artıkları bulup siler**; kaldırıcı yoksa ya da üretici engelliyorsa (Edge) zorla kaldırır.

## Akış (arayüz + `draxbyte uninstall`)
1. Önizleme: kaldırınca neye dokunulacağı (klasörler, kısayollar, kayıt defteri, otomatik başlatma, servis, görev).
2. Çalışan işlemler (yalnızca programın kendi klasöründeki exe'ler, **isimle değil yolla**) kapatılır.
3. Geri yükleme noktası (Windows `Checkpoint-Computer`, macOS `tmutil localsnapshot`); olmazsa uyarı, devam.
4. Üreticinin kaldırıcısı (`QuietUninstallString` istenirse). MSI `/I{}` → `/X{}`. Chromium kurucuları (Edge, WebView2) için `--force-uninstall`.
5. Artıklar yeniden taranır; kullanıcı işaretler; **kesin eşleşmeler seçili, "olası" olanlar seçili değil ve işaretlenirse ayrıca onay ister**.
6. Silme: her öğe korumalardan tekrar geçer; kayıt defteri anahtarı `.reg` olarak dışa aktarılır (dışa aktarma başarısızsa o anahtar silinmez); servis/görev yedeklenir. Kayıt defteri `restore_registry_backup` ile geri yüklenir.

## Windows
- **Envanter:** `HKLM/HKCU ...\Uninstall` (+WOW6432Node) ve Store paketleri (`Get-AppxPackage`, adlar `Get-StartApps` ile Türkçeleştirilir). `SystemComponent=1`+MSI alt bileşenleri gizlenir (Python'un 24 parçası); WebView2 gibi `SystemComponent` programlar "Sistem bileşenleri" kutusuyla görünür.
- **Son kullanım tahmini:** UserAssist (+kısayol/AUMID adı), Prefetch `.pf` değişme zamanı, BAM; **arka plan exe'leri (helper, updater, service…) sayılmaz**. Kayıt yoksa "bilinmiyor" (uydurma yok).
- **Artık bulma:** `ScanEnv` (testte sahte klasör/kayıt dalı). Ad eşleşmesi `name_variants`/`match_name`; `<Yayıncı>\<Ürün>`; Run değerleri, servisler, görevler **yol** ile. Edge: `EdgeUpdate` klasörü, `edgeupdate*` servisleri, `MicrosoftEdgeUpdate*` görevleri.
- **Korumalar:** `leftover_path_allowed` (Program Files/ProgramData/AppData kökleri, paylaşılan çalışma zamanları ve Windows klasörü yasak; kendi kurulum klasörü serbest), `reg_key_allowed` (yalnız `Software\<Satıcı>\…`, servis anahtarı ve programın kendi Uninstall girdisi; OS dalları yasak). Tarayıcı profili (`User Data`) "olası".
- **Edge:** `NoRemove=1` olduğu için "Zorla kaldır"; sonra `DoNotUpdateToEdgeWithChromium=1` yazılır (Windows Update geri getirmesin). **WebView2 ayrı programdır, Edge sihirbazı ona dokunmaz.**
- Yönetici: uygulama yöneticisiz çalışır; sistem geneli işlemler için "Yönetici olarak yeniden aç" (`core/elevate.rs`).

## macOS
- `.app` paketleri (`/Applications`, `~/Applications`, `/System/Applications`), `Info.plist` (`plist`), son kullanım Spotlight `kMDItemLastUsedDate`.
- Artıklar bundle id ile: `~/Library` {Application Support, Caches, Preferences, Containers, Group Containers, Saved Application State, HTTPStorages, WebKit, Logs, LaunchAgents, Cookies} ve `/Library` karşılıkları. **Containers "olası"** (belge içerebilir).
- **Mühürlü sistem uygulamaları** (Safari, Mail, Müzik… `/System` salt okunur birim): hiçbir uygulama silemez; yalnızca önbellek/log/saved-state temizlenir. Başka uygulama Apple'ın `com.apple.*` verisine dokunamaz.
- `.app` silme: önce doğrudan; root'a aitse sistem parola penceresiyle (`osascript … with administrator privileges`). TCC engelinde Tam Disk Erişimi önerilir.
- Test: mantık her platformda birim testli (sahte `~/Library`); gerçek Mac'te `scripts/macos/smoke.sh`.

Testler: `core::apps::*` (`full_leftover_cycle_on_a_scratch_program`, `guards_are_applied_again_at_removal_time`, `browser_profile_is_probable_and_not_preselected`, mac testleri).
