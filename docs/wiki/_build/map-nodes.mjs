// Wiki node -> code file mapping (Draxbyte). Run: node docs/wiki/_build/map-nodes.mjs
import { writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const map = {
  Scanner: ['src/core/scanner.rs'],
  Rules: ['src/core/rules.rs'],
  Safety: ['src/core/safety.rs'],
  Cleaner: ['src/core/cleaner.rs', 'tests/trash_roundtrip.rs'],
  Models: ['src/models.rs'],
  Config: ['src/config.rs'],
  Db: ['src/db.rs'],
  AiClient: ['src/ai/client.rs'],
  AiCache: ['src/ai/cache.rs'],
  AiProviders: ['src/ai/providers.rs', 'src/ai/error.rs', 'tests/live_provider.rs'],
  AiPrompt: ['src/ai/prompt.rs'],
  TokenSavings: ['src/ai/client.rs', 'src/ai/cache.rs', 'src/ai/prompt.rs'],
  App: ['src/app.rs', 'src/main.rs'],
  Ui: ['src/ui/mod.rs', 'src/ui/scan_view.rs', 'src/ui/result_view.rs', 'src/ui/settings_view.rs', 'src/ui/widgets.rs', 'src/ui/about_view.rs'],
  Cli: ['src/cli.rs'],
};
let missing = 0;
for (const [node, paths] of Object.entries(map)) {
  for (const p of paths) {
    if (!existsSync(join(ROOT, p))) { missing++; console.error(`MISSING: ${node} -> ${p}`); }
  }
}
writeFileSync(join(ROOT, 'docs/wiki/_build/map.json'), JSON.stringify(map, null, 2));
console.log(`${Object.keys(map).length} node, ${missing} eksik dosya`);
