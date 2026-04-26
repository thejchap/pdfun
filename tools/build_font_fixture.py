"""Regenerate tests/fixtures/font.ttf — a renamed printable-ASCII subset of Lato.

Vendored under SIL OFL 1.1; rename the family so we don't claim the
reserved Lato name on a Modified Version.

Run with:  uv run --with fonttools python3 tools/build_font_fixture.py
"""

from pathlib import Path

from fontTools.subset import Options, Subsetter
from fontTools.ttLib import TTFont

SRC = Path("/usr/share/fonts/truetype/lato/Lato-Regular.ttf")
DST = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "font.ttf"

if not SRC.exists():
    raise SystemExit(f"missing source font {SRC} — install fonts-lato")

font = TTFont(str(SRC))

opts = Options()
opts.layout_features = []
opts.hinting = False
opts.drop_tables += ["GSUB", "GPOS", "GDEF", "DSIG"]
sub = Subsetter(options=opts)
sub.populate(unicodes=list(range(0x20, 0x7F)))
sub.subset(font)

for n in list(font["name"].names):
    if n.nameID in (1, 4, 6, 16):
        n.string = "PdFunTestFont"
    elif n.nameID == 2:
        n.string = "Regular"

DST.parent.mkdir(parents=True, exist_ok=True)
font.save(str(DST))
print(f"wrote {DST} ({DST.stat().st_size} bytes)")
