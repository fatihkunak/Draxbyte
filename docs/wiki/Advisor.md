# Advisor
Kod: `src/ai/advisor.rs` (zincir, failover, bağlam, istem), `src/ai/providers.rs` (`resolve_with_key`, `send_chat_turns`), `src/config.rs` (`ChainProvider`, `advisor_targets`, `chain_key`), durum `src/advisor.rs`, arayüz `src/ui/advisor_view.rs`, ayarlar `ui/settings_view.rs::chain_section`. Bağlı: [[AiProviders]], [[AiClient]], [[AiCache]], [[Config]], [[Optimize]].

**AI Danışman**: son taramayı konuşmak (ne silinebilir, risk, nasıl yapılır) için sohbet penceresi ve tek tıkla "AI rapor iste". Yalnızca öneri verir; silme/komut çalıştırma yetkisi yok. Kırmızı (korumalı) öğeleri silmeyi önermez.

## Zincir ve failover
- Ayarlar → AI → "sağlayıcı zinciri": istediğin kadar sağlayıcı, sıralı (▲▼), tek tek açılıp kapanır. Her istek HER ZAMAN en üstteki (birincil) sağlayıcıyla başlar; ağ hatası, zaman aşımı, kota (429), geçersiz anahtar (401), 5xx, okunamayan yanıt ya da eksik anahtar/ayar olursa sıradakine geçilir. Kimin neden yanıt vermediği cevabın altında yazar; hepsi başarısızsa her hata listelenir ve "Yeniden dene" çıkar.
- Zincir boşsa `[ai]` altındaki tekli sağlayıcı kullanılır (eski ayarlar bozulmaz).
- Günlük token limiti tüm zincir için ortak (`[ai].daily_token_limit`); yalnızca TESLİM EDİLEN yanıt sayılır. Limit dolunca hiç istek atılmaz (failover ile aşılmaz).
- **Anahtar yalıtımı:** zincirde her sağlayıcı yalnızca kendi anahtarını alır: `DRAXBYTE_API_KEY_<SAĞLAYICI>` ortam değişkeni → girdinin kendi anahtarı → `api_keys.<sağlayıcı>`. Genel `DRAXBYTE_API_KEY` zincirde KULLANILMAZ (bir sağlayıcının anahtarı yedeğe sızardı); yalnız tekli kipte geçerli. Anahtarsız sağlayıcı başkasınınkini ödünç almaz. Hata metinlerinde anahtar `***` olur.

## Gizlilik ve onay
- AI varsayılan KAPALI; kapalıyken ağ isteği yok. Önbellek zorunlu (aynı soru + aynı bağlam ücretsiz, yerel). Her istek kullanıcının "Gönder"i ya da "AI rapor iste"si ile başlar; ilk kullanımda (her oturum) ne gönderileceği yazılı onay kartıyla anlatılır.
- Giden: yazdığın mesajlar ve (açıksa) tarama özeti: boyutlar, kural adları, kısaltılmış yollar (ev klasörü `~`, uzun yollar ortadan kesilir), en büyük 20 öğe, gruplar. Korumalı (kırmızı) öğeler tek rakamdır. Dosya içeriği ASLA gitmez. Gidecek özet ekranda açılır listede görünür.
- İstemde tarama özeti `<scan_summary>` içinde VERİ olarak işaretlenir (dosya adı üzerinden istem enjeksiyonuna karşı). Son 12 tur gönderilir.
- Sohbet geçmişi yalnızca bellekte; diske yazılmaz.

## Notlar
- Ana Sayfa "Temizlenebilir öğelerin dağılımı" kartında "AI Danışman'la tartış →" en büyük grubu soran bir soru hazırlar (göndermez).
- Demo/test: `DRAXBYTE_VIEW=advisor`, `DRAXBYTE_ADVISOR_DEMO=<soru>` (onayı geçer ve soruyu hemen gönderir).
- Failover testleri gerçek yerel HTTP sunucularıyla (500 → yedek, ulaşılamaz → 429 → üçüncü, hepsi hatalı, bozuk girdi atlanır, önbellek, günlük limit).
