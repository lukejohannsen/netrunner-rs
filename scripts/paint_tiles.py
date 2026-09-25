#!/usr/bin/env python3
"""Paints the server strips' frames for the desktop client's board art.

    scripts/venv/bin/python scripts/paint_tiles.py

Writes ten PNGs into crates/netrunner_desktop/assets/board/, one per tile
key board_art already names (ice.unrezzed, ice.rezzed, ice.rezzed.barrier,
ice.rezzed.code-gate, ice.rezzed.sentry, root.unrezzed, root.rezzed,
root.rezzed.asset, root.rezzed.upgrade, root.agenda).

A strip is one line of text on a server column, 26 to 40 logical pixels
tall and a card wide, so each frame is TILE_W x TILE_H with a CAP-wide end
cap at each side that the client keeps whole in a nine-slice; only the
channel between the caps is stretched, and it is plain along its length.
The look is the avatar bar's (scripts/paint_avatar_bar.py, whose helpers
this reuses): a brushed gunmetal rim, a dark recessed channel the strip's
words sit in, and circuit traces. The kind is the colour the traces and
the channel's edge are lit in — the colours Theme::tile gives the drawn
tier, chosen by the person: gold for a barrier (a wall), blue for a code
gate, red for a sentry, grey and unlit for a card face down — and a faint
etching in the channel that tells the kinds apart without the colour: a
course of bricks, lock bars, sight rings, coins, chevrons, an advancement
rail.

Deterministic (fixed seeds), and needs only numpy.
"""

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from paint_avatar_bar import OUT, blur, coverage, polyline_distance, smooth, write_png  # noqa: E402

TILE_W, TILE_H = 512, 96
CAP = 48

# Theme::tile, in sRGB.
KINDS = {
    "ice.unrezzed": (None, "hatch"),
    "ice.rezzed": ((0.86, 0.88, 0.92), "none"),
    "ice.rezzed.barrier": ((0.92, 0.66, 0.12), "bricks"),
    "ice.rezzed.code-gate": ((0.10, 0.36, 0.92), "bars"),
    "ice.rezzed.sentry": ((0.88, 0.14, 0.16), "rings"),
    "root.unrezzed": (None, "backhatch"),
    "root.rezzed": ((0.86, 0.88, 0.92), "rivets"),
    "root.rezzed.asset": ((0.70, 0.42, 0.22), "coins"),
    "root.rezzed.upgrade": ((0.20, 0.72, 0.62), "chevrons"),
    "root.agenda": ((0.42, 0.72, 0.20), "rail"),
}


def etching(kind, xs, ys, ch):
    """A 0..1 mask of the kind's etch inside the channel `ch` (x0, y0, x1, y1)."""
    x0, y0, x1, y1 = ch
    cx, cy = xs + 0.5, ys + 0.5
    inside = (cx >= x0) & (cx <= x1) & (cy >= y0) & (cy <= y1)
    h = y1 - y0
    if kind == "bricks":
        # Two courses of bricks, the joints offset course to course.
        course = np.floor((cy - y0) / (h / 2.0))
        mortar_y = np.abs(((cy - y0) % (h / 2.0)) - h / 4.0) > h / 4.0 - 1.2
        brick = 44.0
        shift = np.where(course % 2 == 0, 0.0, brick / 2.0)
        mortar_x = np.abs(((cx - x0 + shift) % brick) - brick / 2.0) > brick / 2.0 - 1.2
        mark = (mortar_x | mortar_y).astype(float)
    elif kind == "bars":
        # Vertical lock bars, a keyhole bolt through the middle.
        bars = np.abs(((cx - x0) % 22.0) - 11.0) < 1.6
        bolt = np.abs(cy - (y0 + h / 2.0)) < 1.4
        mark = (bars | bolt).astype(float)
    elif kind == "rings":
        # Sight rings at intervals, each with a crosshair.
        mark = np.zeros_like(cx)
        for rx in np.arange(x0 + 34.0, x1, 68.0):
            d = np.hypot(cx - rx, cy - (y0 + h / 2.0))
            ring = np.clip(1.4 - np.abs(d - h * 0.34), 0.0, 1.0)
            cross = ((np.abs(cx - rx) < 1.0) | (np.abs(cy - (y0 + h / 2.0)) < 1.0)) & (d < h * 0.48)
            mark = np.maximum(mark, np.maximum(ring, cross.astype(float) * 0.8))
    elif kind == "coins":
        mark = np.zeros_like(cx)
        for rx in np.arange(x0 + 26.0, x1, 52.0):
            d = np.hypot(cx - rx, cy - (y0 + h / 2.0))
            mark = np.maximum(mark, np.clip(1.4 - np.abs(d - h * 0.30), 0.0, 1.0))
            mark = np.maximum(mark, np.clip(1.4 - np.abs(d - h * 0.16), 0.0, 1.0) * 0.6)
    elif kind == "chevrons":
        period = 30.0
        u = (cx - x0) % period
        v = np.abs(cy - (y0 + h / 2.0))
        mark = (np.abs(u - v * 0.8 - 6.0) < 1.6).astype(float)
    elif kind == "rail":
        # An advancement rail: a line along the bottom of the channel with
        # pips on it.
        rail = np.abs(cy - (y1 - 5.0)) < 1.0
        pips = np.zeros_like(cx)
        for px in np.arange(x0 + 20.0, x1, 40.0):
            pips = np.maximum(pips, np.clip(3.2 - np.hypot(cx - px, cy - (y1 - 5.0)), 0.0, 1.0))
        mark = np.maximum(rail.astype(float), pips)
    elif kind == "rivets":
        mark = np.zeros_like(cx)
        for px in np.arange(x0 + 16.0, x1, 32.0):
            for py in (y0 + 5.0, y1 - 5.0):
                mark = np.maximum(mark, np.clip(2.4 - np.hypot(cx - px, cy - py), 0.0, 1.0))
    elif kind in ("hatch", "backhatch"):
        sign = 1.0 if kind == "hatch" else -1.0
        mark = (np.abs(((cx + sign * cy) % 14.0) - 7.0) < 1.1).astype(float)
    else:
        mark = np.zeros_like(cx)
    return mark * inside


def tile(key, seed):
    colour, pattern = KINDS[key]
    lit = colour is not None
    colour = np.array(colour if lit else (0.55, 0.55, 0.55))
    core = colour * 0.75 + 0.25
    rng = np.random.default_rng(seed)
    w, h = TILE_W, TILE_H
    ys, xs = np.mgrid[0:h, 0:w].astype(float)

    # The plate: the full box less a chamfer at each corner, cut on the
    # slant as the bar's outer end is.
    chamfer = 14.0

    def plate(x, y):
        return (x + y >= chamfer) & ((w - x) + y >= chamfer) & (x + (h - y) >= chamfer) & ((w - x) + (h - y) >= chamfer)

    alpha = coverage(plate, w, h)

    # Brushed gunmetal, streaked along x so the middle stretches cleanly.
    streaks = rng.normal(0.0, 1.0, (h, 1)) * np.ones((1, w))
    streaks = smooth(streaks, 3, 0) * 0.045 + rng.normal(0.0, 0.012, (h, w))
    sheen = 0.10 * np.exp(-((ys - 26.0) / 20.0) ** 2)
    base = 0.30 + sheen + streaks
    steel = np.stack([base * 0.96, base * 0.95, base * 1.04], axis=-1)
    if not lit:
        steel = steel.mean(axis=-1, keepdims=True) * 0.82 * np.ones(3)

    # Bevels: a lit top edge, a shadowed bottom, the chamfers caught.
    steel += np.clip(1.0 - np.abs(ys - 1.5) / 2.0, 0.0, 1.0)[..., None] * 0.28
    steel -= np.clip(1.0 - np.abs(ys - (h - 2.0)) / 2.0, 0.0, 1.0)[..., None] * 0.2
    for d in (xs + ys, (w - xs) + ys):
        steel += (np.clip(1.0 - np.abs(d - chamfer - 1.5) / 2.0, 0.0, 1.0) * 0.2)[..., None]

    # The channel: the dark recess the words sit in, running cap to cap.
    ch = (CAP - 18.0, 16.0, w - CAP + 18.0, h - 16.0)
    x0, y0, x1, y1 = ch
    in_channel = coverage(lambda x, y: (x >= x0) & (x <= x1) & (y >= y0) & (y <= y1), w, h)
    groove = np.array([0.045, 0.04, 0.06]) if lit else np.array([0.07, 0.07, 0.075])
    steel = steel * (1.0 - in_channel[..., None]) + groove * in_channel[..., None]
    lip_shadow = np.clip(1.0 - np.abs(ys - y0) / 1.5, 0.0, 1.0) * (xs >= x0) * (xs <= x1)
    lip_light = np.clip(1.0 - np.abs(ys - y1 - 1.0) / 1.5, 0.0, 1.0) * (xs >= x0) * (xs <= x1)
    steel -= lip_shadow[..., None] * 0.2
    steel += lip_light[..., None] * 0.18

    # The etch in the channel, faint so the words over it read.
    etch = etching(pattern, xs, ys, ch)
    etch_ink = colour * 0.35 if lit else np.array([0.20, 0.20, 0.21])
    steel = steel * (1.0 - etch[..., None] * 0.55) + etch_ink * etch[..., None] * 0.55

    # The channel's edge: a thin line round it in the kind's colour.
    edge = np.minimum(
        np.minimum(np.abs(ys + 0.5 - (y0 - 3.0)), np.abs(ys + 0.5 - (y1 + 3.0))) + np.where((xs + 0.5 >= x0 - 3.0) & (xs + 0.5 <= x1 + 3.0), 0.0, 1e9),
        np.minimum(np.abs(xs + 0.5 - (x0 - 3.0)), np.abs(xs + 0.5 - (x1 + 3.0))) + np.where((ys + 0.5 >= y0 - 3.0) & (ys + 0.5 <= y1 + 3.0), 0.0, 1e9),
    )
    edge_line = np.clip(1.3 - edge, 0.0, 1.0)

    # In each cap, a trace climbing out of the channel's end to a via, and
    # a status light on the rim — the detail in the part never stretched.
    traces = np.full((h, w), 1e9)
    vias = np.full((h, w), 1e9)
    for mirror in (False, True):
        def fx(x):
            return w - x if mirror else x
        pts = [(fx(x0 - 3.0), 30.0), (fx(18.0), 30.0), (fx(12.0), 24.0)]
        traces = np.minimum(traces, polyline_distance(xs + 0.5, ys + 0.5, pts))
        pts = [(fx(x0 - 3.0), h - 30.0), (fx(18.0), h - 30.0), (fx(12.0), h - 24.0)]
        traces = np.minimum(traces, polyline_distance(xs + 0.5, ys + 0.5, pts))
        for vy in (24.0, h - 24.0):
            vias = np.minimum(vias, np.hypot(xs + 0.5 - fx(12.0), ys + 0.5 - vy))
    lamp = coverage(lambda x, y: ((np.abs(x - 12.0) < 3.0) | (np.abs(x - (w - 12.0)) < 3.0)) & (np.abs(y - h / 2.0) < 12.0), w, h)

    line = np.clip(1.4 - traces, 0.0, 1.0)
    via_ring = np.clip(1.2 - np.abs(vias - 3.2), 0.0, 1.0)
    mark = np.maximum(np.maximum(line, via_ring), np.maximum(edge_line, lamp))
    if lit:
        glow = blur(mark, 7) * 1.3 + blur(mark, 15) * 0.7
        steel += glow[..., None] * colour * 0.5
        steel = steel * (1.0 - mark[..., None]) + core * mark[..., None]
    else:
        steel = steel * (1.0 - mark[..., None] * 0.7) + np.array([0.22, 0.22, 0.24]) * mark[..., None] * 0.7

    return np.concatenate([np.clip(steel, 0.0, 1.0), alpha[..., None]], axis=-1)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for seed, key in enumerate(KINDS):
        write_png(OUT / f"{key}.png", tile(key, 21 + seed))


if __name__ == "__main__":
    main()
