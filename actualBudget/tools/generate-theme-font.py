#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["fonttools==4.60.1", "brotli==1.1.0"]
# ///
"""Original three-glyph test font; no third-party glyph data. Optional regeneration."""
from io import BytesIO
from pathlib import Path
import sys
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
font = FontBuilder(1000, isTTF=True)
font.setupGlyphOrder(['.notdef', 'space', 'A'])
font.setupCharacterMap({32: 'space', 65: 'A'})
empty = TTGlyphPen(None).glyph()
pen = TTGlyphPen(None)
pen.moveTo((100, 0)); pen.lineTo((300, 700)); pen.lineTo((500, 0)); pen.closePath()
font.setupGlyf({'.notdef': empty, 'space': empty, 'A': pen.glyph()})
font.setupHorizontalMetrics({'.notdef': (600, 0), 'space': (300, 0), 'A': (600, 0)})
font.setupHorizontalHeader(ascent=800, descent=-200)
font.setupNameTable({'familyName': 'ActualBudget Fixture', 'styleName': 'Regular', 'uniqueFontIdentifier': 'ActualBudgetFixture1', 'fullName': 'ActualBudget Fixture', 'psName': 'ActualBudgetFixture'})
font.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
font.setupPost(); font.setupMaxp()
font.font['head'].created = font.font['head'].modified = 2082844800
font.font.recalcTimestamp = False
font.font.flavor = 'woff2'
output = BytesIO(); font.save(output)
path = Path(__file__).resolve().parents[1] / 'web/tests/fixtures/theme-fixture.woff2'
if '--check' in sys.argv:
    assert path.read_bytes() == output.getvalue(), 'Theme font fixture drift'
else:
    path.write_bytes(output.getvalue())
print(f'Verified original theme font fixture ({len(output.getvalue())} bytes)')
