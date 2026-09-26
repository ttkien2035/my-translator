#!/usr/bin/env python3
"""Build the MeowLaoshi app icon and every size Tauri needs.

    pip install cairosvg pillow
    python3 scripts/build-icon.py [sky|cream|mint|ink]   # default: sky

Artwork: Noto Emoji "cat face" (U+1F431), Google, Apache-2.0 — vector source
and license in src-tauri/icons/source/. It is placed on a macOS-style rounded
square (1024 canvas, 824 px tile, the system icon grid) so macOS 26+ does not
wrap it in its grey fallback frame. Then `tauri icon` writes the sizes; only the
files the repo already tracks are copied back (no Android/iOS folders).
"""
import pathlib
import subprocess
import sys
import tempfile

import cairosvg
from PIL import Image, ImageDraw, ImageFilter

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri" / "icons"
SVG = ICONS / "source" / "emoji_u1f431.svg"
BACKGROUNDS = {
    "sky": ((222, 238, 255), (166, 203, 250)),
    "cream": ((255, 250, 238), (255, 226, 190)),
    "mint": ((228, 249, 240), (172, 228, 208)),
    "ink": ((58, 74, 110), (28, 36, 58)),
}
SIZE, TILE, RADIUS = 1024, 824, 185


def tile_mask() -> Image.Image:
    big = Image.new("L", (SIZE * 4, SIZE * 4), 0)
    o = (SIZE - TILE) // 2 * 4
    ImageDraw.Draw(big).rounded_rectangle([o, o, o + TILE * 4, o + TILE * 4], radius=RADIUS * 4, fill=255)
    return big.resize((SIZE, SIZE), Image.LANCZOS)


def gradient(top, bottom) -> Image.Image:
    col = Image.new("RGB", (1, SIZE))
    for y in range(SIZE):
        t = y / (SIZE - 1)
        col.putpixel((0, y), tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3)))
    return col.resize((SIZE, SIZE)).convert("RGBA")


def compose(background: str, out: pathlib.Path) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        hi = pathlib.Path(tmp) / "cat.png"
        cairosvg.svg2png(url=str(SVG), write_to=str(hi), output_width=2048, output_height=2048)
        cat = Image.open(hi).convert("RGBA")
    cat = cat.crop(cat.getchannel("A").getbbox())
    mask = tile_mask()
    canvas = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    shadow.putalpha(mask.point(lambda v: int(v * 0.28)))
    canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(14)), (0, 10))
    canvas.paste(gradient(*BACKGROUNDS[background]), (0, 0), mask)
    w = int(TILE * 0.74)
    h = int(cat.height * w / cat.width)
    canvas.alpha_composite(cat.resize((w, h), Image.LANCZOS), ((SIZE - w) // 2, (SIZE - h) // 2 + 18))
    canvas.save(out)


def main() -> None:
    background = sys.argv[1] if len(sys.argv) > 1 else "sky"
    with tempfile.TemporaryDirectory() as tmp:
        master = pathlib.Path(tmp) / "icon-1024.png"
        compose(background, master)
        out = pathlib.Path(tmp) / "out"
        subprocess.run(["npx", "tauri", "icon", str(master), "--output", str(out)], cwd=ROOT, check=True)
        for f in sorted(ICONS.iterdir()):
            if f.is_file() and f.suffix in {".png", ".icns", ".ico"} and (out / f.name).exists():
                f.write_bytes((out / f.name).read_bytes())
    print(f"icons rebuilt ({background})")


if __name__ == "__main__":
    main()
