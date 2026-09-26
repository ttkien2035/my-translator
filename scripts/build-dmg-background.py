#!/usr/bin/env python3
"""Draw the DMG window background: an arrow from the app to Applications and
the three steps a first-time user needs, in Vietnamese.

    pip install pillow
    python3 scripts/build-dmg-background.py

Writes src-tauri/dmg/background.png (660×420, the window size set in
tauri.conf.json › bundle.macOS.dmg) and src-tauri/dmg/background@2x.png for
docs. Icon positions below must match appPosition / applicationFolderPosition.
Font: Be Vietnam Pro (OFL), fetched by scripts/build-guide-pdf.py's cache.
"""
import pathlib
import urllib.request

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "src-tauri" / "dmg"
FONTS = pathlib.Path.home() / ".cache" / "my-translator-fonts"
W, H = 660, 420
APP_X, FOLDER_X, ICON_Y = 180, 480, 175  # icon centres (Finder coordinates)


def font(weight: str, size: int) -> ImageFont.FreeTypeFont:
    f = FONTS / f"BeVietnamPro-{weight}.ttf"
    if not f.exists():
        FONTS.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(f"https://raw.githubusercontent.com/google/fonts/main/ofl/bevietnampro/BeVietnamPro-{weight}.ttf", f)
    return ImageFont.truetype(str(f), size)


def draw(scale: int) -> Image.Image:
    s = scale
    img = Image.new("RGB", (W * s, H * s))
    top, bottom = (239, 246, 255), (214, 230, 252)
    px = img.load()
    for y in range(H * s):
        t = y / (H * s - 1)
        c = tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        for x in range(W * s):
            px[x, y] = c
    d = ImageDraw.Draw(img)
    ink, soft, accent = (29, 41, 61), (74, 90, 115), (10, 108, 224)

    title = "Cài MeowLaoshi 猫老师"
    try:
        tf = font("Bold", 22 * s)
        d.text((W * s // 2, 34 * s), "Cài MeowLaoshi", font=tf, fill=ink, anchor="mt")
    except OSError:
        d.text((W * s // 2, 34 * s), title, fill=ink, anchor="mt")

    # Arrow between the two icons (icons are 128 px; leave room for labels).
    y = ICON_Y * s
    x0, x1 = (APP_X + 78) * s, (FOLDER_X - 78) * s
    d.line([(x0, y), (x1 - 14 * s, y)], fill=accent, width=6 * s)
    d.polygon([(x1, y), (x1 - 22 * s, y - 14 * s), (x1 - 22 * s, y + 14 * s)], fill=accent)
    d.text(((x0 + x1) // 2, y - 30 * s), "kéo vào", font=font("SemiBold", 14 * s), fill=accent, anchor="mb")

    steps = [
        ("1", "Kéo MeowLaoshi vào thư mục Applications."),
        ("2", "Mở Applications › MeowLaoshi."),
        ("3", "macOS chặn? Cài đặt hệ thống › Quyền riêng tư & Bảo mật › Vẫn mở."),
    ]
    body, bold = font("Regular", 14 * s), font("Bold", 13 * s)
    y = 292 * s
    for n, text in steps:
        cx, cy, r = 70 * s, y + 9 * s, 11 * s
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=accent)
        d.text((cx, cy), n, font=bold, fill=(255, 255, 255), anchor="mm")
        d.text((92 * s, y), text, font=body, fill=ink)
        y += 32 * s
    d.text((W * s // 2, (H - 12) * s), "Bước 3 chỉ làm một lần. Xong thì đóng cửa sổ này.", font=font("Regular", 11 * s), fill=soft, anchor="mb")
    return img


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    draw(1).save(OUT / "background.png", optimize=True)
    draw(2).save(OUT / "background@2x.png", optimize=True)
    print(f"wrote {OUT.relative_to(ROOT)}/background.png (+@2x)")


if __name__ == "__main__":
    main()
