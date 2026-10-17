"""Rebuilds assets/icon.ico + assets/icon_256.rgba from the brand concept sheet.

Usage: python -I scripts/make_icon.py "<concept.png>"
The D + bolt mark is traced from the sheet (colour masks, smoothed), then drawn on the
dark rounded tile with clean gradients, so it stays sharp at every icon size.
"""
import sys
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

SRC = sys.argv[1]
OUT = "assets"
S = 1024

sheet = Image.open(SRC).convert("RGB").crop((60, 50, 210, 185))
big = sheet.resize((sheet.width * 8, sheet.height * 8), Image.LANCZOS)
a = np.asarray(big).astype(int)
r, g, b = a[..., 0], a[..., 1], a[..., 2]
white = (np.minimum(np.minimum(r, g), b) > 140).astype(np.uint8) * 255
green = ((g > r + 45) & (g > 110)).astype(np.uint8) * 255


def crisp(mask):
    m = Image.fromarray(mask).filter(ImageFilter.GaussianBlur(5))
    return m.point(lambda v: 255 if v > 128 else 0)


white_m, green_m = crisp(white), crisp(green)
bbox = Image.fromarray(np.maximum(white, green)).getbbox()
white_m, green_m = white_m.crop(bbox), green_m.crop(bbox)

target = int(S * 0.60)
scale = target / max(white_m.size)
size = (round(white_m.width * scale), round(white_m.height * scale))
white_m, green_m = white_m.resize(size, Image.LANCZOS), green_m.resize(size, Image.LANCZOS)


def gradient(size, top, bottom):
    t = np.linspace(0, 1, size[1])[:, None, None]
    top, bottom = np.array(top)[None, None, :], np.array(bottom)[None, None, :]
    row = (top * (1 - t) + bottom * t).astype(np.uint8)
    return Image.fromarray(np.repeat(row, size[0], axis=1), "RGB")


tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(tile)
d.rounded_rectangle((8, 8, S - 9, S - 9), radius=int(S * 0.22), fill=(0x1A, 0x1D, 0x21, 255), outline=(0x2B, 0x2F, 0x36, 255), width=10)

ox, oy = (S - size[0]) // 2, (S - size[1]) // 2
tile.paste(gradient(size, (0xF3, 0xF4, 0xF6), (0xB4, 0xBA, 0xC4)), (ox, oy), white_m)
tile.paste(gradient(size, (0x34, 0xD3, 0x99), (0x10, 0xB9, 0x81)), (ox, oy), green_m)

tile.save(f"{OUT}/icon_1024.png")
tile.save(f"{OUT}/icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
open(f"{OUT}/icon_256.rgba", "wb").write(tile.resize((256, 256), Image.LANCZOS).tobytes())
print("ok", tile.size, size)
