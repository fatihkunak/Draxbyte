# Memory
Kod: `src/core/memory.rs`, `src/core/startup.rs`, `src/core/elevate.rs`, arayüz `src/ui/system_view.rs`. Bağlı: [[Programs]], [[Safety]].

- **Bellek özeti + programa göre gruplama** (`sysinfo`): bir tarayıcının onlarca işlemi tek satır.
- **Kapatma:** yalnızca kullanıcının seçtiği **PID'ler**; isimle toplu öldürme yok. `is_protected_process` sistem işlemlerini (lsass, svchost, explorer…), `System32` altındakileri ve Draxbyte'ı reddeder. Kullanıcı ayrıca `[memory] protected = ["servis.exe"]` listesiyle (satırdaki "Koru" düğmesi) bir programı hiçbir koşulda kapatılamaz yapar. Önce nazik kapatma (`taskkill` /F'siz, unix SIGTERM), "Zorla kapat" ayrı ve iki tıklamalı onaylı.
- **"Belleği boşalt"** = `EmptyWorkingSet` (Windows). **Dürüst not arayüzde yazılı:** boş bellek hemen artar ama programlar sayfaları diskten geri okur; kazanç geçici. Kalıcı çözüm: çok yiyeni kapatmak + başlangıcı azaltmak.
- **Başlangıç yöneticisi:** `Run` anahtarları (HKCU/HKLM/WOW6432) ve Startup klasörleri; kapatma silmez, Görev Yöneticisi'nin kullandığı `StartupApproved` değerini çevirir (geri açılır). HKLM için yönetici gerekir.
- **Yönetici:** `is_elevated`, `relaunch_elevated` (UAC).

Testler: `core::memory::*`, `core::startup::*` (geçici kayıt defteri anahtarıyla aç/kapa turu).
