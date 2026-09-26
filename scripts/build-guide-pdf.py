#!/usr/bin/env python3
"""Build docs/huong-dan-cai-dat.pdf from docs/huong-dan-cai-dat.md.

    pip install weasyprint markdown
    python3 scripts/build-guide-pdf.py

Fonts: Be Vietnam Pro (OFL) is downloaded once into ~/.cache/my-translator-fonts;
emoji and Chinese characters fall back to whatever the system provides
(Noto Color Emoji / Apple Color Emoji, WenQuanYi / PingFang).
"""
import pathlib
import re
import urllib.request

import markdown
from weasyprint import HTML

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "docs" / "huong-dan-cai-dat.md"
OUT = ROOT / "docs" / "huong-dan-cai-dat.pdf"
FONTS = pathlib.Path.home() / ".cache" / "my-translator-fonts"
WEIGHTS = {"Regular": 400, "Medium": 500, "SemiBold": 600, "Bold": 700}


def ensure_fonts() -> str:
    FONTS.mkdir(parents=True, exist_ok=True)
    faces = []
    for name, weight in WEIGHTS.items():
        f = FONTS / f"BeVietnamPro-{name}.ttf"
        if not f.exists():
            url = f"https://raw.githubusercontent.com/google/fonts/main/ofl/bevietnampro/BeVietnamPro-{name}.ttf"
            urllib.request.urlretrieve(url, f)
        faces.append(f"@font-face {{ font-family: 'Be Vietnam Pro'; font-weight: {weight}; src: url('{f.as_uri()}'); }}")
    # Emoji and Chinese fonts only for their own ranges: an emoji font also
    # carries digits and would otherwise typeset "10" as two wide keycap glyphs.
    faces.append("@font-face { font-family: 'Guide Emoji'; src: local('Apple Color Emoji'), local('Noto Color Emoji'), local('NotoColorEmoji');"
                 " unicode-range: U+1F000-1FAFF, U+2600-26FF, U+2700-27BF, U+2B50, U+2753, U+25B6, U+FE0F; }")
    faces.append("@font-face { font-family: 'Guide CJK'; src: local('PingFang SC'), local('WenQuanYi Zen Hei'), local('Noto Sans CJK SC');"
                 " unicode-range: U+3000-303F, U+3400-4DBF, U+4E00-9FFF, U+FF00-FFEF; }")
    return "\n".join(faces)


CSS = """
@page {
  size: A4; margin: 16mm 17mm 16mm 17mm;
  @bottom-left { content: "MeowLaoshi 猫老师 — Hướng dẫn cài đặt và sử dụng"; font: 8pt 'Be Vietnam Pro'; color: #8a8f98; }
  @bottom-right { content: counter(page) " / " counter(pages); font: 8pt 'Be Vietnam Pro'; color: #8a8f98; }
}
@page :first { @bottom-left { content: none; } }
html { font-family: 'Be Vietnam Pro', 'Guide CJK', 'Guide Emoji', 'DejaVu Sans', 'Helvetica Neue', sans-serif;
       font-size: 10pt; line-height: 1.45; color: #1d1f23; }
h1 { font-size: 21pt; font-weight: 700; margin: 0 0 2pt; color: #0b5cad; letter-spacing: -0.2pt; }
h1 + p em { color: #6b7280; font-style: normal; font-size: 9pt; }
h2 { font-size: 13.5pt; font-weight: 600; color: #0b5cad; margin: 13pt 0 4pt; padding-bottom: 2.5pt;
     border-bottom: 1.2pt solid #d8e4f2; break-after: avoid; }
h3 { font-size: 11pt; font-weight: 600; margin: 11pt 0 3pt; break-after: avoid; }
p { margin: 0 0 6pt; }
hr { border: none; border-top: 0.8pt solid #e3e6ea; margin: 10pt 0; }
strong { font-weight: 600; }
a { color: #0b5cad; text-decoration: none; }
ol, ul { margin: 0 0 7pt; padding-left: 17pt; }
li { margin: 1.5pt 0; }
li > ol { margin: 2pt 0 2pt; list-style-type: lower-alpha; }
code { font-family: 'Menlo', 'DejaVu Sans Mono', monospace; font-size: 8.6pt; background: #f1f3f6;
       border: 0.5pt solid #e1e4e8; border-radius: 3pt; padding: 0.5pt 3pt; }
table { width: 100%; border-collapse: collapse; margin: 4pt 0 9pt; font-size: 9pt; break-inside: auto; }
tr { break-inside: avoid; }
th { text-align: left; font-weight: 600; background: #eef4fb; color: #0b3f78; }
th, td { padding: 3pt 6pt; border: 0.6pt solid #d7dde5; vertical-align: top; }
tbody tr:nth-child(even) td { background: #fafbfc; }
pre { margin: 4pt 0 8pt; padding: 6pt 9pt; background: #f1f3f6; border: 0.5pt solid #e1e4e8; border-radius: 4pt;
      white-space: pre-wrap; word-break: break-all; }
pre code { background: none; border: none; padding: 0; font-size: 8.2pt; line-height: 1.5; }
blockquote { margin: 6pt 0 8pt; padding: 5pt 9pt; background: #f6f8fa; border-left: 2.5pt solid #9db7d5;
             color: #3d434b; font-size: 9pt; }
blockquote p { margin: 0 0 2pt; }
"""


def main() -> None:
    text = SRC.read_text(encoding="utf-8")
    # The first horizontal rule separates the intro block from section 1 only visually.
    body = markdown.markdown(text, extensions=["tables", "sane_lists", "fenced_code"], output_format="html")
    # python-markdown keeps "4-space" nested ordered lists; nothing else to post-process.
    html = f"<!doctype html><html lang='vi'><head><meta charset='utf-8'><title>Hướng dẫn cài đặt và sử dụng MeowLaoshi</title>" \
           f"<style>{ensure_fonts()}\n{CSS}</style></head><body>{body}</body></html>"
    HTML(string=html, base_url=str(ROOT)).write_pdf(OUT, pdf_variant="pdf/ua-1")
    print(f"wrote {OUT.relative_to(ROOT)} ({OUT.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
