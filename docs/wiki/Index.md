# Draxbyte — Mimari Wiki

> Canlı mimari dokümantasyon. Her modül bir node; `[[wikilink]]` ile Obsidian graph view'da gezilir.
> **Kod mapping:** `_build/map-nodes.mjs` → `_build/map.json` (node → kod dosyası).
> **Son değişiklikler:** [[Changelog]]

## 🎯 Çekirdek (AI'sız, yerel)
- [[Scanner]] — paralel (rayon) iptal edilebilir tarama; node_modules/.cxx/build gibi klasörler TEK öğe olarak toplanır
- [[Rules]] — kural motoru: yeşil/sarı/kırmızı + `unknown` bayrağı (AI adayı)
- [[Safety]] — whitelist, kilit kontrolü, silme öncesi son kontrol (`pre_delete_check`)
- [[Cleaner]] — Çöp Kutusu varsayılan, kalıcı sil ayrı onay, dry-run, geri alma
- [[Models]] — ScanItem, Category, CleanReport, `ai_key` (path+size+mtime hash)

## 🧰 Sistem araçları
- [[SysLocs]] — tüm sürücüler + sistem klasörleri taranır; Geri Dönüşüm Kutusu
- [[Programs]] — kurulu programlar, son kullanım tahmini, kaldırıcı + artık temizliği, zorla kaldırma (Edge dahil), macOS .app
- [[DiskMap]] — boyut ağacı + treemap; alan neyin altında (salt okunur)
- [[Duplicates]] — birebir aynı dosyalar (boyut → kısmi hash → SHA-256), her gruptan en az bir kopya kalır
- [[Wizard]] — kurulum/kaldırma sihirbazı (Setup.exe = aynı program; Windows + macOS)
- [[Memory]] — bellek, program kapatma (korumalı liste), "belleği boşalt", başlangıç yöneticisi

## 💾 Veri & Ayar
- [[Config]] — `~/.draxbyte/config.toml`, eksik alanlar varsayılanla dolar
- [[Db]] — SQLite: scan_session / scan_item (son 5 oturum) + clean_log

## 🤖 AI (opsiyonel, varsayılan KAPALI)
- [[AiProviders]] — sağlayıcı kayıt defteri (14), anahtar/adres çözümü, model listesi çekme, istek biçimleri
- [[AiClient]] — OpenAI-uyumlu istemci; tüm maliyet korumaları ağ çağrısından ÖNCE
- [[AiCache]] — yanıt önbelleği + günlük token sayacı (cache temizlense de sayaç sıfırlanmaz)
- [[AiPrompt]] — minimal prompt (yalnız metadata+yol), max_tokens=200
- [[TokenSavings]] — token tasarruf prensiplerinin tek sayfa özeti

## 🖥️ Arayüz
- [[I18n]] — Türkçe/English metin altyapısı (`t`, `tf!`), otomatik dil, kural gerekçesi çevirisi
- [[App]] — uygulama durumu, arka plan işleri (thread + tokio), kanal polling
- [[Ui]] — egui görünümleri: Ana Sayfa, Tarama, Sonuçlar, Temizlik, AI Önbelleği, Ayarlar
- [[Cli]] — `scan` / `clean` başsız mod (clean varsayılan kuru çalıştırma)
- [[Update]] — GitHub'dan yeni sürüm kontrolü (elle veya açılışta; varsayılan kapalı, indirme yok)

## 📜 Süreç
- [[Changelog]] — wiki/kod değişiklik kaydı
