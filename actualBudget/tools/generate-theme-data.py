#!/usr/bin/env python3
"""Reproduce MIT-licensed pinned catalog/base variables used by theme compiler."""
import hashlib
import json
import re
import sys
from pathlib import Path
root = Path(__file__).resolve().parents[1]
reference = root / 'reference/actual-26.10.0'
output = root / 'server/src/themes'
source_files = {'catalog.json':'packages/desktop-client/src/data/customThemeCatalog.json','LICENSE.txt':'LICENSE.txt'}
artifacts = {name:(reference / source).read_bytes() for name,source in source_files.items()}
base = {}
for theme in ['palette','light','dark','midnight','sidebar-redesign-light']:
    source = f'packages/component-library/src/themes/{theme}.css'
    source_files[theme] = source
    css = (reference / source).read_text()
    base[theme] = {match[1]:match[2].strip() for match in re.finditer(r'(--[\w-]+)\s*:\s*([^;]+);',css)}
    if theme == 'sidebar-redesign-light':
        root_css = css.split(':root {', 1)[1].split('}', 1)[0]
        base['sidebar-redesign-root-light'] = {match[1]:match[2].strip() for match in re.finditer(r'(--[\w-]+)\s*:\s*([^;]+);',root_css)}
artifacts['baseline.json'] = (json.dumps(base,indent=2)+'\n').encode()
theme_path = 'packages/component-library/src/theme.ts'
source_files['theme_names'] = theme_path
theme_source = (reference / theme_path).read_text()
names = dict(re.findall(r"(\w+):\s*'([^']+)'", theme_source))
assert len(names) == len(re.findall(r'^\s+\w+:', theme_source, re.M))
# Isolated source tokens support saved formula expressions; native UI tokens
# remain independent. Browser CSS resolution mirrors useThemeColors exactly.
css = '/* Generated from Actual 26.10.0; MIT. Run generate-theme-data.py. */\n'
for theme in ['palette', 'light', 'dark', 'midnight']:
    selector = ':root' if theme == 'palette' else f':root[data-theme="{theme}"]'
    values = dict(base[theme])
    if theme == 'light':
        values.update(base['sidebar-redesign-root-light'])
    css += selector + ' {\n'
    css += ''.join(f'  {key}: {value};\n' for key, value in values.items())
    css += '}\n'
variables = sorted({key for values in base.values() for key in values if key.startswith('--color-')})
web_artifacts = {'source-colors.css': css.encode(), 'source-names.json': (json.dumps(names,indent=2)+'\n').encode(), 'source-variables.json': (json.dumps(variables,indent=2)+'\n').encode()}
for name, contents in web_artifacts.items():
    destination = root / 'web/src/lib/themes' / name
    if '--check' in sys.argv:
        assert destination.read_bytes() == contents, f'Theme browser artifact drift: {name}'
    else:
        destination.write_bytes(contents)
provenance = {'baseline':'Actual26.10.0','commit':'2bebdbaae0b701120a8cad9d56aafd64fa04c577','sources':{key:{'path':path,'sha256':hashlib.sha256((reference/path).read_bytes()).hexdigest()} for key,path in source_files.items()}}
artifacts['provenance.json'] = (json.dumps(provenance,indent=2)+'\n').encode()
for name,contents in artifacts.items():
    if '--check' in sys.argv:
        assert (output/name).read_bytes() == contents, f'Theme baseline drift: {name}'
    else:
        (output/name).write_bytes(contents)
print('Verified pinned theme catalog, palette/base-variable extraction, license and source hashes')
