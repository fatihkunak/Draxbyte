# Rules
Kod: `src/core/rules.rs`. Bağlı: [[Safety]], [[Scanner]], [[AiClient]].

Sıra: korumalı → Red · kurulu uygulama (`AppData/Local/Programs`) → Red · `.sys/.vxd/.ko`+pagefile → Red · büyük + <24sa değişmiş → Red · güvenli uzantı+yaş → Green · bilinen temp konumu+yaş → Green · VM diski → Yellow(unknown) · iso/dmg → Yellow · eski arşiv → Yellow(unknown) · büyük dosya → Yellow(unknown).
Klasörler: node_modules / rust target / `.cxx` / Gradle build / `.gradle/caches` → Yellow (yeniden üretilir); py cache → Green; cache dizini+yaş → Green.
`unknown=true` yalnız "ne olduğunu bilmiyoruz" durumlarında → UI'da AI'ya sorulabilir.
GOTCHA: `.dll/.so` bilerek Red DEĞİL (build çıktısı olabilir; sistem olanlar zaten korumalı ağaçta).
