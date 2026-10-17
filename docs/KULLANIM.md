# Draxbyte Kullanım Kılavuzu

Bu kılavuz Draxbyte'ı ilk kez açan birinin ihtiyaç duyacağı her şeyi sırayla anlatır. (English: [USAGE.md](USAGE.md))

## İçindekiler
1. [Kurulum ve açılış](#1-kurulum-ve-açılış)
2. [Ekranlar](#2-ekranlar)
3. [Tarama](#3-tarama)
4. [Sonuçları okumak](#4-sonuçları-okumak)
5. [Temizlik](#5-temizlik)
6. [Geri alma](#6-geri-alma)
7. [Yapay zekâ](#7-yapay-zekâ)
8. [Ayarlar](#8-ayarlar)
9. [Komut satırı](#9-komut-satırı)
10. [Sık sorulanlar / sorun giderme](#10-sık-sorulanlar--sorun-giderme)

---

## 1. Kurulum ve açılış

**Hazır exe:** [En son sürümden](https://github.com/fatihkunak/Draxbyte/releases/latest) `draxbyte.exe` indirip çift tıkla.
(Kendinden imzalı yayıncı nedeniyle Windows SmartScreen uyarısı çıkabilir: *Ek bilgi → Yine de çalıştır*.)

**Ya da kaynaktan:**

```powershell
git clone https://github.com/fatihkunak/Draxbyte.git
cd Draxbyte
powershell -File scripts\install.ps1
```

Bu komut uygulamayı derler, `%LOCALAPPDATA%\Programs\Draxbyte\draxbyte.exe` olarak kurar ve masaüstüne ile
Başlat menüsüne **Draxbyte** kısayolu koyar. Çift tıklayınca açılır.

Güncelleme: depoyu `git pull` ile güncelle, `install.ps1`'i tekrar çalıştır.

## 2. Ekranlar

Sol menüden gezilir:

| Ekran | Ne işe yarar |
|---|---|
| **Ana Sayfa** | Disk doluluğu, hızlı eylem kartları, en büyük öğeler, dağılım grafiği, son tarama özeti |
| **Tarama** | Çalışan taramanın canlı ilerlemesi ve **İptal** düğmesi |
| **Sonuçlar** | Bulunan her şeyin listesi; seçme, filtreleme, sıralama |
| **Temizlik** | Son temizliğin raporu ve **Geri al** |
| **AI Önbelleği** | Önbellek istatistikleri ve günlük token kullanımı (AI açıkken) |
| **Ayarlar** | Dil, tarama, kurallar, AI ve temizlik ayarları |
| **Hakkında** | Sürüm, geliştirici, veri klasörü, lisanslar |

Sol alttaki yazı AI durumunu gösterir: *"AI kapalı · ağ isteği yok"* ya da *"AI açık (her istek onaylı)"*.

## 3. Tarama

**Ana Sayfa → Hızlı tarama.** "Taranacak yollar" kutusuna `;` ile ayırarak istediğin klasörleri yazabilirsin
(`~` ev klasörün demektir). **Varsayılan** düğmesi ayarlardaki yolları geri getirir.

Tarama sırasında:
- Dosyalar paralel gezilir; sayaçlar (dosya, boyut, bulunan aday, erişilemeyen) canlı akar.
- **İptal**'e basarsan kısmi sonuçlar gösterilir ama kaydedilmez.
- Hiçbir şey silinmez; tarama yalnızca okur.

Bittiğinde otomatik olarak **Sonuçlar**'a geçilir ve tarama kaydedilir (son 5 tarama saklanır).
Uygulamayı kapatıp açsan da son tarama Ana Sayfa'da durur.

> `node_modules`, `build`, `target`, önbellek klasörleri gibi yerler **tek satır** olarak (toplam boyutuyla)
> gelir. Böylece 60 bin dosyalık bir proje listeyi doldurmaz.

## 4. Sonuçları okumak

Her satırda: **kutu · kategori · boyut · yaş · neden · yol · AI**.

**Kategori renkleri**

| | Anlam |
|---|---|
| 🟢 Güvenli | Eski log/geçici/önbellek dosyaları ve klasörleri. Silmek bir şeyi bozmaz. |
| 🟡 Dikkat | Yeniden üretilebilir (`node_modules`, derleme çıktısı) ya da *ne olduğunu bilmediğimiz* büyük/eski dosyalar. Bir göz at. |
| 🔴 Silme | Sistem/uygulama dosyaları, son 24 saatte değişen büyük dosyalar. **Kutusu pasiftir.** |

**Neden** sütunu kuralın gerekçesini söyler (ör. "Gradle derleme çıktısı; sonraki derlemede yeniden oluşur").

**Araç çubuğu**
- **Tümü / Güvenli / Dikkat / Silme** — kategoriye göre filtre.
- **Yolda ara** — yolun içinde geçen metne göre filtre.
- **Tüm yeşilleri seç** — güvenli olanların hepsini işaretler.
- **Sarıları incele** — yalnızca sarıları gösterir (kendin karar ver).
- **Seçimi temizle** — tüm işaretleri kaldırır.
- Sütun başlıklarına (**Kategori, Boyut, Yaş, Yol**) tıklayarak sırala; tekrar tıklayınca yön değişir.

Altta *"5 öğe seçili · 745 MB"* görünür; **Temizle…** düğmesi seçim varken açılır.

## 5. Temizlik

**Temizle…** bir onay penceresi açar:

- **Özet:** kaç öğe, toplam boyut, yeşil/sarı dağılımı. Sarı varsa uyarı çıkar.
- **Kuru çalıştırma:** işaretlersen *hiçbir şey silinmez*; yalnızca "ne olurdu" raporu üretilir.
  Emin değilsen **önce bunu dene**.
- **Çöp Kutusu'na taşı** (varsayılan): geri alınabilir.
- **Kalıcı sil:** geri alınamaz. Ayrıca `SIL` yazmanı ister (ayarlardan kapatılabilir).

Başlattığında **Temizlik** ekranı ilerlemeyi gösterir; bittiğinde rapor çıkar:

| Durum | Anlamı |
|---|---|
| silindi / çöpe taşındı | Başarılı |
| silinirdi | Kuru çalıştırma |
| atlandı: … | Güvenlik kontrolü durdurdu (kilitli, taramadan sonra değişmiş, korumalı yol, artık yok…) |
| HATA: … | İşletim sistemi izin vermedi / başka bir sorun |

**Silmeden hemen önce her öğe tekrar kontrol edilir.** Tarama ile temizlik arasında bir dosya değiştiyse ya da
başka bir program onu kullanıyorsa **atlanır** — bu bir hata değil, korumadır.

## 6. Geri alma

**Temizlik → Geri al (Çöp Kutusu'ndan)** son temizlikte Çöp Kutusu'na taşınan öğeleri özgün yerlerine geri getirir.

- Çöp Kutusu'nu boşalttıysan geri alınamaz.
- **Kalıcı silinenler** geri alınamaz.
- macOS'ta bu düğme çalışmaz; öğeleri Çöp Kutusu'ndan elle geri al.

## 7. Yapay zekâ

Kural motoru bir şeyi tanıyamazsa ("büyük dosya; ne olduğunu doğrula") bunu AI'ya sorabilirsin. **Tamamen
isteğe bağlıdır ve varsayılan olarak kapalıdır.**

**Açmak için:** Ayarlar → *AI analizini aç*.

1. **Sağlayıcı** seç. Para harcamak istemezsen **Ollama** veya **LM Studio**'yu seç (kendi bilgisayarında çalışır,
   anahtar gerekmez).
2. Anahtar gerekiyorsa gir (yanındaki **anahtar al** bağlantısı sağlayıcının sayfasını açar).
3. **Modelleri getir** — sağlayıcının gerçek model listesi gelir (token harcamaz). Listeden seç ya da adını yaz.
4. **Bağlantıyı test et** — ~30 token'lık küçük bir istek atıp anahtar + modeli doğrular.
5. **Kaydet**.

**Kullanım:** Sonuçlar'da bir satırın **AI'ya Sor** düğmesine bas.
- Daha önce sorduysan yanıt **önbellekten ücretsiz** gelir (yanında *(önbellek)* yazar).
- Sormadıysan gönderilecek **tam metin** bir pencerede gösterilir; **Gönder** dersen gider. Dosya *içeriği*
  gönderilmez; yalnızca yol, tür, boyut, yaş ve kural ipucu gider.
- Yanıt *SAFE / CAUTION / KEEP* ile başlayan kısa bir görüştür; **karar yine sende.**

**Maliyet kontrolü:** **AI Önbelleği** ekranında günlük token kullanımını, önbellek boyutunu ve isabet oranını
görürsün. Günlük limit dolarsa AI o gün için kapanır.

> Anahtarlar `~/.draxbyte/config.toml` içinde düz metin saklanır. Daha güvenlisi `DRAXBYTE_API_KEY` ortam
> değişkenidir (ayarlardaki anahtara göre önceliklidir).

## 8. Ayarlar

| Bölüm | Alan | Açıklama |
|---|---|---|
| Dil | Dil / Language | Otomatik (sistem dili), Türkçe, English. Hemen uygulanır; **Kaydet** kalıcı yapar |
| Tarama | Varsayılan yollar | Her satır bir yol |
| | Hariç tutulan desenler | Ör. `*.git/*` — bu yollara girilmez |
| | Maks. derinlik / Min. boyut | Çok derine inme / küçük öğeleri gösterme |
| Kurallar | Güvenli uzantılar | Eski olunca yeşil sayılan uzantılar |
| | Güvenli yaş (gün) | "Eski" kaç gün demek (varsayılan 30) |
| | Büyük dosya eşiği (MB) | Bunun üstü "büyük dosya" (varsayılan 500) |
| AI | (bkz. 7) | |
| Temizlik | Varsayılan Çöp Kutusu | Kapatırsan onay penceresi "Kalıcı sil" ile açılır |
| | "SIL" yazdır | Kalıcı silmede ek onay |

**Kaydet** ayarları `config.toml`'a yazar. **Varsayılana dön** formu fabrika değerlerine getirir (kaydedene kadar
geçerli olmaz).

## 9. Komut satırı

Arayüz olmadan da çalışır (sunucu/otomasyon için):

```powershell
draxbyte scan C:\Users\Ben\Downloads --limit 20      # listele
draxbyte scan --save                                  # taramayı kaydet (arayüzde "son tarama" olur)
draxbyte clean C:\Temp                                # KURU ÇALIŞTIRMA — hiçbir şey silmez
draxbyte clean C:\Temp --apply                        # yeşilleri Çöp Kutusu'na taşı
draxbyte clean C:\Temp --yellow --apply               # sarıları da dahil et
draxbyte clean C:\Temp --apply --permanent            # kalıcı sil ("SIL" yazmanı ister)
```

Ortam değişkenleri: `DRAXBYTE_HOME` (ayar/veri klasörü), `DRAXBYTE_API_KEY` (AI anahtarı).

## 10. Sık sorulanlar / sorun giderme

**Neden `docker_data.vhdx` gibi dev dosyalar kırmızı?**
Son 24 saatte değişmiş büyük dosyalar "kullanımda olabilir" diye korunur. Docker/WSL diskleri sürekli yazıldığı için
hep böyledir. Bunları Docker'ın kendi araçlarıyla küçült.

**Bir dosya "atlandı: dosya kilitli" dedi.**
Başka bir program o dosyayı açık tutuyor. Programı kapatıp yeniden tara.

**"atlandı: taramadan sonra değişmiş" dedi.**
Tarama ile temizlik arasında dosya değişti; Draxbyte eski bilgiyle silmez. Yeniden tara.

**Beklediğim klasör listede yok.**
Ya `min_size_mb` altında, ya hariç tutulan desenlerde, ya da korunan bir yolda (sistem klasörleri hiç gösterilmez).
Ayarlardan **Min. boyut**'u düşürebilirsin.

**Yaş sütununda "—" var.**
Dosyanın değişme tarihi sıfırlanmış (bazı uygulamalar yapar); yaş hesaplanamıyor.

**Ayarlarım nerede?**
`Hakkında` ekranında *Ayar dosyası* ve *Veri klasörü* yazar; **Veri klasörünü aç** ile açabilirsin. Sıfırlamak için
`config.toml`'u sil.

**Hiçbir şey ağa çıkıyor mu?**
Hayır — AI kapalıyken hiç. AI açıkken yalnızca senin bastığın **Modelleri getir / Bağlantıyı test et / Gönder**
düğmeleri istek atar. Bağlantılar (ör. "anahtar al") yalnızca tıklayınca tarayıcında açılır.
