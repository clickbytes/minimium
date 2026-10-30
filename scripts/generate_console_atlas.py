"""Rasterize the bundled OFL Roboto Mono into the client's alpha atlas.

Requires Pillow. Run from the repository root with:
    python scripts/generate_console_atlas.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
FONT_DIR = ROOT / "apps/client/assets/fonts"
CELL = 20
COLUMNS = 16
GLYPHS = [chr(code) for code in range(127)] + list("ĄąĆćĘęŁłŃńÓóŚśŹźŻż")
WHITE_CELL = len(GLYPHS)

image = Image.new("L", (CELL * COLUMNS, CELL * ((WHITE_CELL + 1 + COLUMNS - 1) // COLUMNS)))
draw = ImageDraw.Draw(image)
font = ImageFont.truetype(FONT_DIR / "RobotoMono.ttf", 18)
for index, glyph in enumerate(GLYPHS):
    if glyph.isprintable():
        x = index % COLUMNS * CELL
        y = index // COLUMNS * CELL
        draw.text((x + 4, y + 16), glyph, font=font, fill=255, anchor="ls")

white_x = WHITE_CELL % COLUMNS * CELL
white_y = WHITE_CELL // COLUMNS * CELL
draw.rectangle((white_x, white_y, white_x + CELL - 1, white_y + CELL - 1), fill=255)
(FONT_DIR / "console-atlas.bin").write_bytes(image.tobytes())
