# Models
Kod: `src/models.rs`. `ScanItem.ai_key()` = sha256(path|size|mtime) → [[AiCache]] anahtarı (değişen dosya eski yorumu almaz). Dizinler için mtime = en yeni alt dosya mtime'ı.
