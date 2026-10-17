"""Builds docs/screenshots/social-preview.png (1280x640, GitHub social preview) from the brand assets."""
from PIL import Image, ImageDraw, ImageFont

W, H = 1280, 640
BG, CARD, BORDER, TEXT, MUTED, ACCENT = (0x1A, 0x1D, 0x21), (0x22, 0x26, 0x2B), (0x2B, 0x2F, 0x36), (0xF3, 0xF4, 0xF6), (0x9C, 0xA3, 0xAF), (0x10, 0xB9, 0x81)

semibold = "assets/fonts/Inter-SemiBold.ttf"
regular = "assets/fonts/Inter-Regular.ttf"

img = Image.new("RGB", (W, H), BG)
d = ImageDraw.Draw(img)

# soft accent glow, bottom right
glow = Image.new("RGB", (W, H), BG)
gd = ImageDraw.Draw(glow)
gd.ellipse((760, 260, 1500, 900), fill=(0x0F, 0x3A, 0x30))
from PIL import ImageFilter
glow = glow.filter(ImageFilter.GaussianBlur(120))
img = Image.blend(img, glow, 0.9)
d = ImageDraw.Draw(img)

icon = Image.open("assets/icon_1024.png").convert("RGBA").resize((330, 330), Image.LANCZOS)
img.paste(icon, (90, 155), icon)

x = 470
d.text((x, 150), "Drax", font=ImageFont.truetype(semibold, 124), fill=TEXT)
dx = d.textlength("Drax", font=ImageFont.truetype(semibold, 124))
d.text((x + dx, 150), "byte", font=ImageFont.truetype(semibold, 124), fill=ACCENT)
d.text((x + 4, 300), "CLEANER SYSTEM. MORE SPACE.", font=ImageFont.truetype(regular, 30), fill=MUTED)
d.text((x + 4, 372), "Local-first disk scanner and cleaner.", font=ImageFont.truetype(semibold, 38), fill=TEXT)
d.text((x + 4, 424), "Never deletes without your approval.", font=ImageFont.truetype(regular, 34), fill=MUTED)

# chips
cx, cy = x + 4, 505
for label in ("Rust", "Windows", "MIT", "EN · TR"):
    f = ImageFont.truetype(semibold, 26)
    w = int(d.textlength(label, font=f)) + 44
    d.rounded_rectangle((cx, cy, cx + w, cy + 52), radius=14, fill=CARD, outline=BORDER, width=2)
    d.text((cx + 22, cy + 10), label, font=f, fill=ACCENT if label == "MIT" else TEXT)
    cx += w + 16

img.save("docs/screenshots/social-preview.png", optimize=True)
print("ok", img.size)
