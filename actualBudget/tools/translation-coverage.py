#!/usr/bin/env python3
"""Report literal UI-key coverage; dynamic keys are deliberately not claimed."""
import json
import re
from pathlib import Path
root = Path(__file__).resolve().parents[1] / 'web'
keys = set()
for path in (root / 'src').rglob('*'):
    if path.suffix in ('.ts', '.svelte'):
        keys.update(re.findall(r'\bt\(\s*[\'"]([^\'"\n]+)[\'"]', path.read_text()))
source = (root / 'src/lib/i18n.svelte.ts').read_text()
alias_block = source.split('export const translationAliases:', 1)[1].split('};', 1)[0]
aliases = dict(re.findall(r'[\'"]([^\'"\n]+)[\'"]\s*:\s*[\'"]([^\'"\n]+)[\'"]', alias_block))
report = {'scope': 'Static literal t() keys only. Dynamic errors, feature names and interpolated selectors excluded. English natural-key fallback is intentional.', 'literal_keys': len(keys), 'languages': {}}
for path in sorted((root / 'src/locales').glob('*.json')):
    if path.stem in ('provenance', 'english-plurals'): continue
    catalog = json.loads(path.read_text())
    missing = sorted(key for key in keys if key not in catalog and aliases.get(key) not in catalog)
    report['languages'][path.stem] = {'released_keys':len(catalog),'covered_literal_keys':len(keys)-len(missing),'english_fallback_keys':missing}
(root / 'docs/translation-coverage.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(f'Reported {len(keys)} literal keys across {len(report["languages"])} released catalogs')
