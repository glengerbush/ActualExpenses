#!/usr/bin/env python3
"""Reproduce the exact language catalogs shipped by locked @actual-app/web 26.10.0."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2]
package = root / 'actualExpenses/tools/compatibility/node_modules/@actual-app/web'
assert json.loads((package / 'package.json').read_text())['version'] == '26.10.0'
output = root / 'actualBudget/web/src/locales'
output.mkdir(parents=True, exist_ok=True)
entries = {}
for source_map in sorted((package / 'build/static/js').glob('*.map')):
    data = json.loads(source_map.read_text())
    for source, content in zip(data.get('sources', []), data.get('sourcesContent', [])):
        if source.startswith('../../../locale/') and source.endswith('.json'):
            language = Path(source).stem
            catalog = json.loads(content)
            (output / f'{language}.json').write_text(content)
            entries[language] = {'source': source, 'source_map': source_map.name,
                'source_map_sha256': hashlib.sha256(source_map.read_bytes()).hexdigest(),
                'sha256': hashlib.sha256(content.encode()).hexdigest(), 'keys': len(catalog)}
assert len(entries) == 15
lock = json.loads((root / 'actualExpenses/tools/compatibility/package-lock.json').read_text())
locked = lock['packages']['node_modules/@actual-app/web']
(output / 'provenance.json').write_text(json.dumps({'package': '@actual-app/web', 'version': '26.10.0', 'integrity': locked['integrity'], 'catalogs': entries}, indent=2) + '\n')
english = json.loads((output / 'en.json').read_text())
(output / 'english-plurals.json').write_text(json.dumps({key: value for key, value in english.items() if key.rsplit('_', 1)[-1] in ['one','other','zero','two','few','many']}, ensure_ascii=False, indent=2) + '\n')
(output / 'LICENSE.txt').write_bytes((root / 'actualBudget/reference/actual-26.10.0/LICENSE.txt').read_bytes())
notices = root / 'actualBudget/web/public/licenses'
notices.mkdir(parents=True, exist_ok=True)
(notices / 'actual-MIT.txt').write_bytes((output / 'LICENSE.txt').read_bytes())
print(f'Extracted {len(entries)} published catalogs without rewriting their contents.')
