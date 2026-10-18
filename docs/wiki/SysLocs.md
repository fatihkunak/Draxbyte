# SysLocs
Kod: `src/core/syslocs.rs`, `src/core/recycle.rs`. Bağlı: [[Scanner]], [[Rules]], [[Safety]].

**Tarama artık atlamaz.** Varsayılan kökler: yapılandırılan yollar (`~`) + **her dahili sürücü** (`all_drives`) + **sistem klasörleri** (`include_system`); iç içe kökler `dedupe_roots` ile birleştirilir (korumalı bir klasör araya giriyorsa iç kök korunur: `C:\Windows\Temp`).
- Windows sistem klasörleri: `Windows\Temp`, `Logs\CBS`, `Logs\DISM`, `SoftwareDistribution\Download`, `Prefetch`, `Minidump`, `LiveKernelReports`, `Installer\$PatchCache$`, `ProgramData\Package Cache`, `...\WER`, `Windows.old`. `safety.rs` bunları korumalı `C:\Windows` ağacının **istisnası** yapar.
- `Package Cache` / `$PatchCache$` **sarı** (`installer_cache`: silinirse bazı programların onarımı kaynak ister), `Windows.old` sarı tek öğe (`windows_old`); diğerleri yaş kuralıyla yeşil.
- Sürücü kökü artık taranır (çocukları); kök kendisi hâlâ korumalı.
- `max_depth` varsayılanı 10 → 64; eski config'ler `config_version` 2 ile **bir kez** taşınır (10 = eski varsayılan, seçim değil).
- GUI'de yol alanı değişmemişse tam kapsam, düzenlenmişse harfiyen o yollar.

**Geri Dönüşüm Kutusu** (`recycle.rs`): taramada listelenmez (`$Recycle.Bin` korumalı); Ana Sayfa kartı + `draxbyte recycle [--empty]`. Windows `SHQueryRecycleBin`/`SHEmptyRecycleBin`, macOS Finder, Linux `Trash/`. Boşaltma kalıcıdır, onaylıdır; testler asla `empty()` çağırmaz.
