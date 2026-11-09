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
artifacts['baseline.json'] = (json.dumps(base,indent=2)+'\n').encode()
provenance = {'baseline':'Actual26.10.0','commit':'2bebdbaae0b701120a8cad9d56aafd64fa04c577','sources':{key:{'path':path,'sha256':hashlib.sha256((reference/path).read_bytes()).hexdigest()} for key,path in source_files.items()}}
artifacts['provenance.json'] = (json.dumps(provenance,indent=2)+'\n').encode()
for name,contents in artifacts.items():
    if '--check' in sys.argv:
        assert (output/name).read_bytes() == contents, f'Theme baseline drift: {name}'
    else:
        (output/name).write_bytes(contents)
print('Verified pinned theme catalog, palette/base-variable extraction, license and source hashes')
