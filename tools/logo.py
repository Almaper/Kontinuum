#!/usr/bin/env python3
"""Draws Kontinuum's logo and app icon as SVG, from strokes (no font needed, so they look the
same everywhere):

- assets/logo.svg: KONTINUUM in one simple line weight and one color, ink on cream; the K, the
  first N and the T are italic;
- assets/logo-dark.svg: the same in cream on walnut;
- assets/icon.svg: the italic KNT, the app icon.

usage: tools/logo.py [OUT_DIR]   (default: assets)
"""

import sys
from pathlib import Path

CREAM = "#efe3c8"
INK = "#2e2a25"
WALNUT = "#2b1b12"

# (letter, width, italic); the letter box is H high
WORD = [("K", 92, True), ("O", 104, False), ("N", 92, True), ("T", 96, True),
        ("I", 18, False), ("N", 92, False), ("U", 90, False), ("U", 90, False),
        ("M", 104, False)]
GAP = 38
H = 140
STROKE = 10
# the italic slant, degrees, and how far it moves the letter's top
SLANT = 13
SHIFT = H * 0.2309  # tan(13°) * H


def letter(ch: str, x: float, y: float, w: float, h: float = H) -> str:
    """The path of a letter whose box is (x, y, w, h)."""
    b = y + h
    if ch == "K":
        return (f"M{x},{y} V{b} M{x + w},{y} L{x + 6},{y + h * 0.56} "
                f"M{x + w * 0.30},{y + h * 0.40} L{x + w},{b}")
    if ch == "O":
        rx, ry = w / 2, h / 2
        cx, cy = x + rx, y + ry
        return (f"M{cx - rx},{cy} A{rx},{ry} 0 1 0 {cx + rx},{cy} "
                f"A{rx},{ry} 0 1 0 {cx - rx},{cy} Z")
    if ch == "N":
        return f"M{x},{b} V{y} L{x + w},{b} V{y}"
    if ch == "T":
        return f"M{x},{y} H{x + w} M{x + w / 2},{y} V{b}"
    if ch == "I":
        return f"M{x + w / 2},{y} V{b}"
    if ch == "U":
        r = w / 2
        return f"M{x},{y} V{b - r} A{r},{r} 0 0 0 {x + w},{b - r} V{y}"
    if ch == "M":
        return f"M{x},{b} V{y} L{x + w / 2},{y + h * 0.62} L{x + w},{y} V{b}"
    raise ValueError(ch)


def italic(path: str, x: float, baseline: float) -> str:
    """`path` slanted about its baseline, moved back by half the slant so it sits centered."""
    dx = -SHIFT / 2
    return (f'<path transform="translate({x + dx:.1f} {baseline}) skewX({-SLANT}) '
            f'translate({-x:.1f} {-baseline})" d="{path}"/>')


def word(x0: float, y: float) -> tuple[str, float]:
    out = []
    x = x0
    for ch, w, slanted in WORD:
        p = letter(ch, x, y, w)
        out.append(italic(p, x, y + H) if slanted else f'<path d="{p}"/>')
        x += w + GAP
    return "".join(out), x - GAP - x0


def logo(fg: str, bg: str) -> str:
    width, height = 1400, 420
    total = sum(w for _, w, _ in WORD) + GAP * (len(WORD) - 1)
    x0 = (width - total) / 2
    letters, _ = word(x0, 140)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">
  <rect width="{width}" height="{height}" rx="24" fill="{bg}"/>
  <g fill="none" stroke="{fg}" stroke-width="{STROKE}" stroke-linecap="round" stroke-linejoin="round">{letters}</g>
</svg>
"""


def icon() -> str:
    s = 512
    h = 170
    widths = [("K", 92), ("N", 92), ("T", 96)]
    gap = 40
    total = sum(w for _, w in widths) + gap * 2
    x = (s - total) / 2
    y = (s - h) / 2
    parts = []
    for ch, w in widths:
        p = letter(ch, x, y, w, h)
        dx = -h * 0.2309 / 2
        parts.append(f'<path transform="translate({x + dx:.1f} {y + h}) skewX({-SLANT}) '
                     f'translate({-x:.1f} {-(y + h)})" d="{p}"/>')
        x += w + gap
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {s} {s}" width="{s}" height="{s}">
  <rect width="{s}" height="{s}" rx="112" fill="{WALNUT}"/>
  <g fill="none" stroke="{CREAM}" stroke-width="16" stroke-linecap="round" stroke-linejoin="round">{"".join(parts)}</g>
</svg>
"""


def main() -> None:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "assets")
    out.mkdir(parents=True, exist_ok=True)
    (out / "logo.svg").write_text(logo(INK, CREAM))
    (out / "logo-dark.svg").write_text(logo(CREAM, WALNUT))
    (out / "icon.svg").write_text(icon())
    for n in ("logo.svg", "logo-dark.svg", "icon.svg"):
        print(out / n)


if __name__ == "__main__":
    main()
