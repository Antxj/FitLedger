"""FitLedger icon: rising chart line with a heart-rate pulse on a blue tile.
Requires: pip install pillow

Run from the repository root:
    python tools/make_icon.py

Writes:
- assets/icon.ico        exe icon (16 to 256 px)
- assets/icon_64.rgba    raw 64x64 RGBA pixels for the window icon
- docs/logo.png          256 px logo for the README
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
TOP = (59, 130, 246)
BOTTOM = (29, 78, 216)
WHITE = (255, 255, 255, 255)


def draw(size: int) -> Image.Image:
    scale = 4  # draw large and scale down (anti-aliasing)
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for i in range(s):
        t = i / s
        c = tuple(int(TOP[k] + (BOTTOM[k] - TOP[k]) * t) for k in range(3))
        d.line([(0, i), (s, i)], fill=c + (255,))
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, s - 1, s - 1], radius=int(s * 0.22), fill=255)
    img.putalpha(mask)
    d = ImageDraw.Draw(img)

    def p(x, y):
        return (x * s, y * s)

    # baseline grid
    for y in (0.36, 0.56, 0.76):
        d.line([p(0.16, y), p(0.84, y)], fill=(255, 255, 255, 70), width=max(1, int(s * 0.012)))
    # rising line with a pulse in the middle
    pts = [p(0.17, 0.70), p(0.33, 0.62), p(0.41, 0.62), p(0.47, 0.44), p(0.53, 0.74), p(0.59, 0.55), p(0.70, 0.48), p(0.81, 0.31)]
    w = int(s * 0.062)
    d.line(pts, fill=WHITE, width=w, joint="curve")
    r = s * 0.065
    cx, cy = p(0.81, 0.31)
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=WHITE)
    return img.resize((size, size), Image.LANCZOS)


def main():
    (ROOT / "assets").mkdir(exist_ok=True)
    (ROOT / "docs").mkdir(exist_ok=True)
    big = draw(256)
    big.save(ROOT / "assets" / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    (ROOT / "assets" / "icon_64.rgba").write_bytes(draw(64).tobytes())
    big.save(ROOT / "docs" / "logo.png", optimize=True)
    print("icon written")


if __name__ == "__main__":
    main()
