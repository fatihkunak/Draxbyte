# AiProviders
Kod: `src/ai/providers.rs`. Bağlı: [[AiClient]], [[Config]], [[Ui]].

- Kayıt defteri (`PROVIDERS`): Ollama + LM Studio (yerel, anahtarsız, sıfır maliyet), Gemini, Groq, OpenRouter, NVIDIA NIM, Mistral (ücretsiz katman), Anthropic, OpenAI, Kimi, xAI (ücretli), EVREN, Alibaba, özel OpenAI-uyumlu adres. Liste yazarın başka bir projesindeki sağlayıcı listesiyle aynı kaynaktan; `default_model` yalnız başlangıç, asıl liste sağlayıcıdan çekilir.
- `resolve(cfg)` → `Creds` (adres, model, anahtar). Sabit adresli sağlayıcıda config'teki adres yok sayılır; yerel/özel/Alibaba'da kullanıcı adresi geçerli. https zorunlu (loopback ve yerel grup hariç), URL'de kullanıcı/parola yasak.
- Anahtar önceliği: `DRAXBYTE_API_KEY` → `[ai.api_keys.<sağlayıcı>]` → eski `api_key`.
- Model listesi `GET {base}/models` (`data[]`/`models[]`, `models/` öneki atılır; Anthropic `?limit=100`). Maliyeti yok. "Bağlantıyı test et" ~30 token'lık gerçek istek, günlük sayaca işlenir.
- İstek biçimi: OpenAI-uyumlu `chat/completions` (OpenAI'de `max_completion_tokens`, EVREN'de `X-API-Key`), Anthropic `messages` (`x-api-key` + `anthropic-version`).
- Hata metinlerinde anahtar `***` ile maskelenir.
- Canlı doğrulama: `tests/live_provider.rs` (`DRAXBYTE_LIVE_URL`, yerel llama-server ile geçti).
