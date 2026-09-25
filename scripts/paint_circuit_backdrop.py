#!/usr/bin/env python3
"""Paints the desktop client's shared menu backdrop: circuitry on gunmetal.

The picture behind every menu screen without one of its own
(`crates/netrunner_desktop/assets/backdrops/menu.jpg`), asked for in the
spirit of Fantasy Flight Games' card back: raised metal plates, thin
metallic traces with a blue, red or green core, pads and vias, some depth.
It is drawn here rather than taken from the back, so it is the project's
own (GPL-3.0-or-later, `assets/CREDITS.md`) and not a copy of FFG's art.

**Darker than the back it echoes.** The back is bright silver; a menu puts
light text on glass over its backdrop, and a near-white sky behind that
text was the reason the previous shared picture was replaced. So the metal
is gunmetal and the colour is in the traces.

**Deterministic:** one seed, one picture. Re-run to repaint:

    python3 scripts/paint_circuit_backdrop.py [--seed N] [--out PATH]

Needs numpy and Pillow (`pip install numpy pillow`); nothing in the build
runs it. Drawn at twice the size and downsampled, so every line is
antialiased.
"""

import argparse
import math
import random

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

WIDTH, HEIGHT = 2560, 1600
SCALE = 2
W, H = WIDTH * SCALE, HEIGHT * SCALE

# Trace colours: the Corp's blue, the Runner's red, and a circuit green.
BLUE = (70, 150, 255)
RED = (255, 70, 80)
GREEN = (60, 230, 150)

GRID = 14 * SCALE  # routing pitch
DIRS = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)]


def brushed_metal(rng):
    """Gunmetal ground with a horizontal brushed grain and a soft sheen."""
    y = np.linspace(0.0, 1.0, H)[:, None]
    x = np.linspace(0.0, 1.0, W)[None, :]
    # A diagonal sheen: light falling from the upper left.
    sheen = 0.5 + 0.5 * np.cos((x * 0.9 + y * 0.6 - 0.35) * math.pi * 1.4)
    base = 26 + 22 * sheen
    grain = rng.normal(0.0, 1.0, (H, W // 64 + 2))
    grain = np.repeat(grain, 64, axis=1)[:, :W]
    grain = np.array(Image.fromarray(((grain * 18) + 128).clip(0, 255).astype(np.uint8)).filter(ImageFilter.BoxBlur(24)), dtype=np.float32) - 128
    lum = base[..., None] + grain[..., None] * 0.35
    tint = np.array([0.92, 0.98, 1.08])  # cool steel
    return (lum * tint).clip(0, 255)


def plates(rng):
    """Raised plates: a few large rounded panels, as one mask."""
    mask = Image.new("L", (W, H), 0)
    draw = ImageDraw.Draw(mask)
    cols, rows = 5, 3
    cw, rh = W / cols, H / rows
    for r in range(rows):
        for c in range(cols):
            if rng.random() < 0.3:
                continue
            span_c = 2 if rng.random() < 0.35 and c < cols - 1 else 1
            pad = 26 * SCALE
            x0 = c * cw + pad
            y0 = r * rh + pad
            x1 = (c + span_c) * cw - pad
            y1 = (r + 1) * rh - pad
            draw.rounded_rectangle([x0, y0, x1, y1], radius=30 * SCALE, fill=255)
    return mask


def emboss(canvas, mask, height, light=1.0):
    """Lifts `mask` off `canvas`: a highlight on its upper-left edge, a
    shadow on its lower-right and under it, the body a shade lighter."""
    m = np.array(mask, dtype=np.float32) / 255.0
    blur = lambda im, r: np.array(im.filter(ImageFilter.GaussianBlur(r)), dtype=np.float32) / 255.0
    shifted = lambda dx, dy: Image.fromarray((np.roll(np.roll(m, dy, 0), dx, 1) * 255).astype(np.uint8))
    drop = blur(shifted(height * 3, height * 4), height * 5)
    canvas *= (1.0 - 0.55 * drop * (1.0 - m))[..., None]
    lit = np.clip(m - np.array(shifted(height, height), dtype=np.float32) / 255.0, 0, 1)
    dark = np.clip(m - np.array(shifted(-height, -height), dtype=np.float32) / 255.0, 0, 1)
    lit = blur(Image.fromarray((lit * 255).astype(np.uint8)), 1.5 * SCALE)
    dark = blur(Image.fromarray((dark * 255).astype(np.uint8)), 1.5 * SCALE)
    # The plate's face: steel lit from above, a shade brighter than the
    # ground, with the grain kept.
    y = np.linspace(1.0, 0.0, H)[:, None]
    canvas += (m * (14 + 26 * y) * light)[..., None]
    canvas += (lit * 90 * light)[..., None]
    canvas *= (1.0 - 0.6 * dark)[..., None]
    return canvas


def colour_at(x, y, rng):
    """Blue to the left, red to the right, green through the middle and
    along the bottom, with some mixing so no band is a wall."""
    u = x / W + rng.uniform(-0.18, 0.18)
    v = y / H
    if v > 0.78 and rng.random() < 0.5:
        return GREEN
    if u < 0.38:
        return BLUE
    if u > 0.62:
        return RED
    return GREEN


def route(rng, occupied, cols, rows):
    """A bundle of parallel traces, the way a bus runs on a board: two
    to five tracks side by side that walk together on the routing grid
    in eight directions, keep their heading, bend by 45 degrees together
    now and then, and stop together when any track is blocked. Mostly
    vertical, as the card back's buses are. Never crosses another trace.
    """
    for _ in range(60):
        c, r = rng.randrange(cols), rng.randrange(rows)
        if not occupied[r][c]:
            break
    else:
        return []
    d = rng.choice((2, 2, 6, 6, 0, 4))
    width = rng.choice((1, 2, 3, 3, 4, 5))
    pc, pr = -DIRS[d][1], DIRS[d][0]  # perpendicular to the heading
    tracks = [[(c + pc * 2 * k, r + pr * 2 * k)] for k in range(width)]
    if any(not (0 <= tc < cols and 0 <= tr < rows) or occupied[tr][tc] for tc, tr in (t[0] for t in tracks)):
        return []
    for t in tracks:
        occupied[t[0][1]][t[0][0]] = True
    run = 0
    for _ in range(rng.randint(20, 110)):
        run += 1
        if run > 6 and rng.random() < 0.08:
            d = (d + rng.choice((-1, 1))) % 8
            run = 0
        dc, dr = DIRS[d]
        moves = []
        for t in tracks:
            tc, tr = t[-1]
            nc, nr = tc + dc, tr + dr
            if not (0 <= nc < cols and 0 <= nr < rows) or occupied[nr][nc]:
                moves = None
                break
            if dc and dr and occupied[tr][nc] and occupied[nr][tc]:
                moves = None
                break
            moves.append((nc, nr))
        if moves is None:
            break
        for t, (nc, nr) in zip(tracks, moves):
            occupied[nr][nc] = True
            t.append((nc, nr))
    return [t for t in tracks if len(t) > 5]


def simplify(path):
    """Keeps only the corners of a grid walk."""
    out = [path[0]]
    for a, b, c in zip(path, path[1:], path[2:]):
        if (b[0] - a[0], b[1] - a[1]) != (c[0] - b[0], c[1] - b[1]):
            out.append(b)
    out.append(path[-1])
    return out


def paint(seed):
    rng = random.Random(seed)
    nrng = np.random.default_rng(seed)
    canvas = brushed_metal(nrng)

    plate_mask = plates(rng)
    canvas = emboss(canvas, plate_mask, 3 * SCALE)

    cols, rows = W // GRID, H // GRID
    occupied = [[False] * cols for _ in range(rows)]
    traces = []
    for _ in range(260):
        bundle = route(rng, occupied, cols, rows)
        if not bundle:
            continue
        head = bundle[0][0]
        colour = colour_at(head[0] * GRID, head[1] * GRID, rng)
        lit = rng.random() < 0.4  # a bus carries current or is bare
        for p in bundle:
            pts = [(c * GRID + GRID / 2, r * GRID + GRID / 2) for c, r in simplify(p)]
            traces.append((pts, colour, lit))

    groove = Image.new("L", (W, H), 0)
    metal = Image.new("L", (W, H), 0)
    core = Image.new("RGB", (W, H), (0, 0, 0))
    glow = Image.new("RGB", (W, H), (0, 0, 0))
    gd, md, cd, gl = (ImageDraw.Draw(i) for i in (groove, metal, core, glow))
    for pts, colour, lit in traces:
        shadow = [(x + 2 * SCALE, y + 3 * SCALE) for x, y in pts]
        gd.line(shadow, fill=255, width=4 * SCALE, joint="curve")
        md.line(pts, fill=255, width=3 * SCALE, joint="curve")
        if lit:
            cd.line(pts, fill=colour, width=int(1.5 * SCALE), joint="curve")
            gl.line(pts, fill=colour, width=6 * SCALE, joint="curve")
        for (x, y), end in ((pts[0], True), (pts[-1], True)):
            r = rng.choice((4, 5, 6)) * SCALE
            gd.ellipse([x - r + 2 * SCALE, y - r + 3 * SCALE, x + r + 2 * SCALE, y + r + 3 * SCALE], fill=255)
            md.ellipse([x - r, y - r, x + r, y + r], fill=255)
            hole = r * 0.45
            cd.ellipse([x - hole, y - hole, x + hole, y + hole], fill=colour if lit else (18, 20, 26))
            if lit:
                gl.ellipse([x - r * 1.6, y - r * 1.6, x + r * 1.6, y + r * 1.6], fill=colour)

    # Two lenses after the card back's: concentric broken rings around a
    # dark eye, one each side of the middle, where a menu's panel does
    # not cover them.
    for cx, colour in ((0.12 * W, BLUE), (0.88 * W, RED)):
        cy = rng.uniform(0.3, 0.7) * H
        eye = 34 * SCALE
        gd.ellipse([cx - eye + 3 * SCALE, cy - eye + 4 * SCALE, cx + eye + 3 * SCALE, cy + eye + 4 * SCALE], fill=255)
        md.ellipse([cx - eye, cy - eye, cx + eye, cy + eye], fill=255)
        cd.ellipse([cx - eye * 0.7, cy - eye * 0.7, cx + eye * 0.7, cy + eye * 0.7], fill=(12, 14, 20))
        cd.ellipse([cx - eye * 0.3, cy - eye * 0.3, cx + eye * 0.3, cy + eye * 0.3], fill=colour)
        gl.ellipse([cx - eye, cy - eye, cx + eye, cy + eye], fill=colour)
        for k, (rr, width) in enumerate(((70, 10), (104, 5), (150, 14), (178, 3))):
            rr *= SCALE
            box = [cx - rr, cy - rr, cx + rr, cy + rr]
            sbox = [box[0] + 3 * SCALE, box[1] + 4 * SCALE, box[2] + 3 * SCALE, box[3] + 4 * SCALE]
            start = rng.uniform(0, 360)
            for g in range(rng.choice((2, 3))):
                a0 = start + g * (360 / 3)
                a1 = a0 + rng.uniform(70, 105)
                gd.arc(sbox, a0, a1, fill=255, width=(width + 2) * SCALE)
                md.arc(box, a0, a1, fill=255, width=width * SCALE)
                if k % 2 == 0:
                    cd.arc(box, a0, a1, fill=colour, width=max(1, width // 4) * SCALE)
                    gl.arc(box, a0, a1, fill=colour, width=8 * SCALE)

    # Groove: the trace's shadow, cut into the metal.
    g = np.array(groove.filter(ImageFilter.GaussianBlur(3 * SCALE)), dtype=np.float32) / 255.0
    canvas *= (1.0 - 0.6 * g)[..., None]
    # Metal: a bright rail with a darker edge, so it reads as a thin
    # rounded wire rather than a flat stroke.
    m = np.array(metal, dtype=np.float32) / 255.0
    mi = np.array(metal.filter(ImageFilter.MinFilter(3)), dtype=np.float32) / 255.0
    rail = 0.55 * m + 0.45 * mi
    steel = np.array([150, 160, 176], dtype=np.float32)
    canvas = canvas * (1.0 - rail[..., None]) + steel * rail[..., None]
    # Core and glow, added as light.
    c = np.array(core, dtype=np.float32)
    ca = (c.max(axis=2) > 0)[..., None]
    canvas = np.where(ca, canvas * 0.25 + c * 0.95, canvas)
    gw = np.array(glow.filter(ImageFilter.GaussianBlur(9 * SCALE)), dtype=np.float32)
    canvas += gw * 0.45

    # Vignette, so the edges fall away and the middle, where the menus
    # sit, is evenly lit.
    y = np.linspace(-1.0, 1.0, H)[:, None]
    x = np.linspace(-1.0, 1.0, W)[None, :]
    vignette = 1.0 - 0.45 * np.clip(x * x * 0.7 + y * y * 0.9, 0, 1)
    canvas *= vignette[..., None]

    image = Image.fromarray(canvas.clip(0, 255).astype(np.uint8), "RGB")
    return image.resize((WIDTH, HEIGHT), Image.LANCZOS)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--seed", type=int, default=2026)
    parser.add_argument("--out", default="crates/netrunner_desktop/assets/backdrops/menu.jpg")
    args = parser.parse_args()
    paint(args.seed).save(args.out, quality=90, optimize=True, progressive=True)


if __name__ == "__main__":
    main()
