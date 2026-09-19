"""Rebuild Coppet's shipped TTF from the editable, unhinted TTX source.

Font Software, SIL Open Font License 1.1; see OFL.txt.
Requires FontTools 4.65.0 and ttfautohint 1.8.4. Not part of cargo build.
"""
from pathlib import Path
import hashlib
import os
import subprocess
import tempfile

from fontTools.ttLib import TTFont
from fontTools.ttLib.tables.ttProgram import Program

HERE = Path(__file__).resolve().parent
OUTPUT = HERE / "Coppet-Regular.ttf"
font = TTFont(recalcTimestamp=False)
font.importXML(HERE / "Coppet-Regular.ttx")
source_dates = (font["head"].created, font["head"].modified)
with tempfile.TemporaryDirectory() as temporary:
    unhinted = Path(temporary) / "Coppet-Regular-unhinted.ttf"
    font.save(unhinted)
    subprocess.run([
        os.environ.get("TTFAUTOHINT", "ttfautohint"),
        "-n", "-x", "0", "-a", "qsq", str(unhinted), str(OUTPUT),
    ], check=True)

font = TTFont(OUTPUT, recalcTimestamp=False)
# ttfautohint stamps its output with the current time; restore source dates.
font["head"].created, font["head"].modified = source_dates
# At the guest's 9-ppem monochrome size, protect l's curved foot from
# collapsing into its stem column. Do not move phantom points (advances),
# or the 36-ppem retained outline used for sharp host presentation.
glyph = font["glyf"]["uni006C"]
points = len(glyph.coordinates)
hint = Program()
hint.fromAssembly([
    "SVTCA[1]", "MPPEM[ ]", "PUSHB[ ]", "9", "EQ[ ]", "IF[ ]",
    "PUSHB[ ]", "1", "SZP2[ ]", "PUSHB[ ]", str(points), "SLOOP[ ]",
    "NPUSHB[ ]", *map(str, range(points)), "20", "SHPIX[ ]", "EIF[ ]",
])
glyph.program.fromBytecode(bytes(glyph.program.getBytecode()) + bytes(hint.getBytecode()))
font["maxp"].maxStackElements = max(font["maxp"].maxStackElements, points + 1)
font.save(OUTPUT)
print(hashlib.sha256(OUTPUT.read_bytes()).hexdigest(), OUTPUT.name)
