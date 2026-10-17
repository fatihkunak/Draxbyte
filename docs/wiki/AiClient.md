# AiClient
Kod: `src/ai/client.rs`. Bağlı: [[AiCache]], [[AiPrompt]], [[TokenSavings]].

Kapılar (sıra): `enabled` → `cache_enabled` (zorunlu) → önbellek isabeti (ücretsiz) → günlük token limiti → API anahtarı → tek POST `/chat/completions` (stream yok, max_tokens=200, 30 sn timeout).
`AiService::new` kapalıyken hata döner ve hiçbir şey açmaz; UI'da AI kapalıyken servis hiç oluşmaz. Her istek UI'da onay modalından geçer.
