#!/usr/bin/env python3
"""Paints the avatar bar's pictures for the desktop client's board art.

    scripts/venv/bin/python scripts/paint_avatar_bar.py

Writes four PNGs into crates/netrunner_desktop/assets/board/:

- avatar.bar.png / avatar.bar.active.png — one wing of a side's status
  bar, the outer end at the left and the end that runs under the avatar at
  the right. The client draws the right-hand wing as the same picture
  mirrored, which a nine-slice mirrors exactly only when its two end caps
  are the same width, so both ends are CAP pixels and the middle is plain
  enough to be stretched to any length: straight brushing, straight traces.
- avatar.frame.png / avatar.frame.active.png — the ring round the avatar,
  transparent inside and out.

The look follows the shared menu backdrop (backdrops/menu.jpg): brushed
gunmetal plates with chamfered ends, and a recessed channel of purple
circuit traces. The active side's traces are lit; the idle side's are
dark and its steel greyer, so a glance says whose turn it is.

Deterministic (a fixed seed), and needs only numpy: the PNG is written by
hand with zlib, so no imaging library is required.
"""

import struct
import zlib
from pathlib import Path

import numpy as np

OUT = Path(__file__).resolve().parent.parent / "crates/netrunner_desktop/assets/board"

# The wing, at twice its logical size: 48 logical pixels tall.
WING_W, WING_H = 480, 96
CAP = 160
# The frame, at more than twice the largest disc (72 logical).
FRAME = 256

PURPLE = np.array([0.72, 0.36, 1.0])
PURPLE_CORE = np.array([0.93, 0.80, 1.0])


def write_png(path, rgba):
    """An 8-bit RGBA PNG from a float array in 0..1."""
    h, w, _ = rgba.shape
    data = (np.clip(rgba, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)
    raw = b"".join(b"\x00" + data[y].tobytes() for y in range(h))

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


def smooth(a, n, axis):
    """A box blur of width n along one axis, wrapping."""
    out = np.zeros_like(a)
    for k in range(-(n // 2), n // 2 + 1):
        out += np.roll(a, k, axis=axis)
    return out / (2 * (n // 2) + 1)


def blur(a, n):
    return smooth(smooth(a, n, 0), n, 1)


def coverage(mask_fn, w, h, ss=4):
    """Anti-aliased coverage of a shape given as a predicate on pixel centres."""
    ys, xs = np.mgrid[0:h * ss, 0:w * ss]
    m = mask_fn((xs + 0.5) / ss, (ys + 0.5) / ss).astype(float)
    return m.reshape(h, ss, w, ss).mean(axis=(1, 3))


def segment_distance(px, py, a, b):
    ax, ay = a
    bx, by = b
    dx, dy = bx - ax, by - ay
    t = np.clip(((px - ax) * dx + (py - ay) * dy) / max(dx * dx + dy * dy, 1e-9), 0.0, 1.0)
    return np.hypot(px - (ax + t * dx), py - (ay + t * dy))


def polyline_distance(px, py, points):
    d = np.full(px.shape, 1e9)
    for a, b in zip(points, points[1:]):
        d = np.minimum(d, segment_distance(px, py, a, b))
    return d


def wing(active):
    rng = np.random.default_rng(7)
    w, h = WING_W, WING_H
    ys, xs = np.mgrid[0:h, 0:w].astype(float)

    # The plate: chamfered at the outer (left) end, square at the inner.
    top, bottom = 4.0, 92.0

    def plate(x, y):
        # The outer end is cut on the slant, with a small step, as the
        # backdrop's plates are.
        left = np.where(y < 48.0, 34.0 - (y - top) * 0.55, 10.0 + (y - 48.0) * 0.0)
        return (y >= top) & (y <= bottom) & (x >= left)

    alpha = coverage(plate, w, h)

    # Brushed gunmetal: horizontal streaks, smooth along x so the middle
    # stretches without showing it.
    streaks = rng.normal(0.0, 1.0, (h, 1)) * np.ones((1, w))
    streaks = smooth(streaks, 3, 0) * 0.045 + rng.normal(0.0, 0.012, (h, w))
    # A broad sheen across the plate, brighter above the middle.
    sheen = 0.10 * np.exp(-((ys - 30.0) / 22.0) ** 2)
    base = 0.30 + sheen + streaks
    steel = np.stack([base * 0.96, base * 0.95, base * 1.04], axis=-1)
    if not active:
        grey = steel.mean(axis=-1, keepdims=True)
        steel = grey * np.array([0.98, 0.98, 1.0]) * 0.78

    # Bevels: a lit top edge, a shadowed bottom edge, and the slanted end
    # caught by the light.
    edge_top = np.clip(1.0 - np.abs(ys - (top + 1.5)) / 2.0, 0.0, 1.0)
    edge_bottom = np.clip(1.0 - np.abs(ys - (bottom - 1.5)) / 2.0, 0.0, 1.0)
    steel += edge_top[..., None] * (0.32 if active else 0.20)
    steel -= edge_bottom[..., None] * 0.18
    slant_x = np.where(ys < 48.0, 34.0 - (ys - top) * 0.55, 10.0)
    edge_slant = np.clip(1.0 - np.abs(xs - (slant_x + 1.5)) / 2.0, 0.0, 1.0) * (ys >= top) * (ys <= bottom)
    steel += edge_slant[..., None] * 0.22

    # A seam across the plate at the cap, as the backdrop's panels meet.
    seam = np.clip(1.0 - np.abs(xs - (CAP - 40.0)) / 1.0, 0.0, 1.0) * (ys > 10) * (ys < 66)
    steel -= seam[..., None] * 0.15

    # The channel: a recessed groove near the bottom, dark, with its lip
    # lit on the lower edge and shadowed on the upper.
    ch_top, ch_bottom = 68.0, 86.0
    channel_left = 44.0
    in_channel = coverage(lambda x, y: (y >= ch_top) & (y <= ch_bottom) & (x >= channel_left), w, h)
    groove = np.array([0.05, 0.04, 0.07]) if active else np.array([0.07, 0.07, 0.08])
    steel = steel * (1.0 - in_channel[..., None]) + groove * in_channel[..., None]
    lip_shadow = np.clip(1.0 - np.abs(ys - ch_top) / 1.5, 0.0, 1.0) * (xs >= channel_left)
    lip_light = np.clip(1.0 - np.abs(ys - ch_bottom - 1.0) / 1.5, 0.0, 1.0) * (xs >= channel_left)
    steel -= lip_shadow[..., None] * 0.2
    steel += lip_light[..., None] * 0.18

    # The traces: three straight lines along the channel, which in the
    # outer cap climb out of it and end in vias on the plate — the only
    # detail in the wing, and in the part that is never stretched.
    traces = np.full((h, w), 1e9)
    vias = np.full((h, w), 1e9)
    lanes = [72.0, 77.0, 82.0]
    ends = [(60.0, 20.0), (84.0, 30.0), (108.0, 40.0)]
    for lane, (turn, via_y) in zip(lanes, ends):
        pts = [(w + 4.0, lane), (turn + (lane - via_y) * 0.6, lane), (turn, via_y)]
        traces = np.minimum(traces, polyline_distance(xs + 0.5, ys + 0.5, pts))
        vias = np.minimum(vias, np.hypot(xs + 0.5 - turn, ys + 0.5 - via_y))
    # Two short stubs on the plate, as the backdrop's etched marks.
    for x0, x1, y in [(122.0, 146.0, 14.0), (126.0, 138.0, 20.0)]:
        traces = np.minimum(traces, polyline_distance(xs + 0.5, ys + 0.5, [(x0, y), (x1, y)]))

    line = np.clip(1.4 - traces, 0.0, 1.0)
    via_ring = np.clip(1.2 - np.abs(vias - 3.2), 0.0, 1.0)
    via_core = np.clip(2.0 - vias, 0.0, 1.0)
    mark = np.maximum(line, np.maximum(via_ring, via_core * 0.8))
    if active:
        glow = blur(mark, 7) * 1.4 + blur(mark, 17) * 0.9
        steel += glow[..., None] * PURPLE * 0.55
        steel = steel * (1.0 - mark[..., None]) + (PURPLE * 0.55 + PURPLE_CORE * 0.45) * mark[..., None]
    else:
        steel = steel * (1.0 - mark[..., None] * 0.7) + np.array([0.20, 0.20, 0.23]) * mark[..., None] * 0.7

    rgba = np.concatenate([np.clip(steel, 0.0, 1.0), alpha[..., None]], axis=-1)
    return rgba


def frame(active):
    rng = np.random.default_rng(11)
    s = FRAME
    c = s / 2.0
    ys, xs = np.mgrid[0:s, 0:s].astype(float)
    r = np.hypot(xs + 0.5 - c, ys + 0.5 - c)
    theta = np.arctan2(ys + 0.5 - c, xs + 0.5 - c)
    outer, inner = 126.0, 106.0

    alpha = coverage(lambda x, y: (np.hypot(x - c, y - c) <= outer) & (np.hypot(x - c, y - c) >= inner), s, s)

    # Brushed round the ring, lit from above.
    bins = 720
    streak = rng.normal(0.0, 1.0, bins)
    streak = np.convolve(np.concatenate([streak[-3:], streak, streak[:3]]), np.ones(7) / 7, mode="valid")
    idx = ((theta + np.pi) / (2 * np.pi) * bins).astype(int) % bins
    light = -np.sin(theta)  # 1 at the top
    base = 0.34 + 0.12 * light + streak[idx] * 0.035 + rng.normal(0.0, 0.01, (s, s))
    # A conical sheen, as the backdrop's centre hub has.
    base += 0.10 * np.cos(2.0 * theta) ** 8
    steel = np.stack([base * 0.96, base * 0.95, base * 1.04], axis=-1)
    if not active:
        steel = steel.mean(axis=-1, keepdims=True) * 0.8 * np.ones(3)

    # Bevels on both rims.
    outer_rim = np.clip(1.0 - np.abs(r - (outer - 1.5)) / 2.0, 0.0, 1.0)
    inner_rim = np.clip(1.0 - np.abs(r - (inner + 1.5)) / 2.0, 0.0, 1.0)
    steel += (outer_rim * np.clip(light, 0, 1) * 0.3 - outer_rim * np.clip(-light, 0, 1) * 0.2)[..., None]
    steel += (inner_rim * np.clip(-light, 0, 1) * 0.25 - inner_rim * np.clip(light, 0, 1) * 0.2)[..., None]

    # Four notches at the diagonals, dark cuts into the ring.
    notch = np.zeros((s, s))
    for k in range(4):
        a = np.pi / 4 + k * np.pi / 2
        d_ang = np.abs(np.angle(np.exp(1j * (theta - a))))
        notch = np.maximum(notch, np.clip(1.0 - (d_ang * r - 4.0) / 1.5, 0.0, 1.0) * (r > inner + 3) * (r < outer - 3))
    steel = steel * (1.0 - notch[..., None] * 0.75)

    # The trace: a thin line round the inside of the ring, lit on the
    # active side, with a via in each notch.
    ring_line = np.clip(1.3 - np.abs(r - (inner + 6.0)), 0.0, 1.0)
    vias = np.zeros((s, s))
    for k in range(4):
        a = np.pi / 4 + k * np.pi / 2
        vx, vy = c + np.cos(a) * (inner + 12.0), c + np.sin(a) * (inner + 12.0)
        d = np.hypot(xs + 0.5 - vx, ys + 0.5 - vy)
        vias = np.maximum(vias, np.clip(2.6 - d, 0.0, 1.0))
    mark = np.maximum(ring_line, vias)
    if active:
        glow = blur(mark, 7) * 1.2
        steel += glow[..., None] * PURPLE * 0.5
        steel = steel * (1.0 - mark[..., None]) + (PURPLE * 0.5 + PURPLE_CORE * 0.5) * mark[..., None]
        # A soft purple light spilling onto the avatar's edge from the ring.
        spill = np.clip(1.0 - (inner - r) / 8.0, 0.0, 1.0) * (r < inner) * 0.55
        alpha = np.maximum(alpha, spill * 0.6)
        steel = steel * (1.0 - spill[..., None]) + PURPLE * spill[..., None]
    else:
        steel = steel * (1.0 - mark[..., None] * 0.6) + np.array([0.18, 0.18, 0.2]) * mark[..., None] * 0.6

    return np.concatenate([np.clip(steel, 0.0, 1.0), alpha[..., None]], axis=-1)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    write_png(OUT / "avatar.bar.png", wing(active=False))
    write_png(OUT / "avatar.bar.active.png", wing(active=True))
    write_png(OUT / "avatar.frame.png", frame(active=False))
    write_png(OUT / "avatar.frame.active.png", frame(active=True))


if __name__ == "__main__":
    main()
