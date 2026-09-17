# Skins — drawing the board's furniture

The board's tiles, server headers, buttons and chips are outlines: a
background colour, a one-pixel border, a corner radius. A **skin**
replaces them with pictures.

Nothing is committed here. The outlined look is the procedural tier and
always works, so the game looks finished with this directory empty.

## The two rules worth knowing before you draw

**A skin never changes the size of anything.** Every box is laid out by
the game — the board fits the window without scrolling, and the card size
is computed from what is on the table — so your picture is fitted to a box
it does not get to argue with. Draw to the sizes in the table below.

**You do not have to draw all of it.** Anything you have not drawn keeps
its outline, and any *state* you have not drawn borrows the base picture.
One `tile.png` dresses both a rezzed tile and an unrezzed one. A skin
holding a single file is a real skin.

## Where they go

Same three tiers as the tables and the card backs:

| Tier | Path |
|---|---|
| Drawn | — (the outlines; always works) |
| Bundled | `crates/netrunner_desktop/assets/skins/<name>/` |
| Yours | `<data dir>/netrunner/assets/skins/<name>/` |

On Linux that last one is `~/.local/share/netrunner/assets/skins/`. Use
it while you are working — nothing there is version-controlled, and it
overrides a bundled skin of the same name file by file.

## Choosing one

Settings → **Board art**, and the same row in the board's gear menu. It
cycles:

- **Follow the table** — the default. A table may name a skin in its
  `table.json` (`"skin": "neon-chrome"`), so a field and the chrome drawn
  for it travel together.
- **Drawn** — the outlines, whatever the table suggests.
- then each installed skin, chosen outright.

`auto` and `drawn` are reserved, so a folder by either name is ignored.

## `skin.json`

```json
{
  "name": "Neon Chrome",
  "shadows": false,
  "slots": {
    "tile":           { "file": "tile.png", "insets": 6 },
    "tile.rezzed":    { "file": "tile-lit.png", "insets": [4, 6, 4, 6] },
    "button":         { "file": "button.png", "insets": 8 },
    "button.hover":   { "tint": "#8ffff0" },
    "server.header": { "file": "header.png", "insets": 10 },
    "hud.cell":       { "file": "plate.png", "insets": 5, "tint": "state" }
  }
}
```

- `file` — a PNG beside the manifest. Leave it out and the entry only
  re-tints the base slot's picture, which is how you write a hover state
  that is the same shape in a different colour.
- `insets` — the nine-slice cut, in the picture's own pixels. One number
  for all four sides, or `[left, top, right, bottom]`. **This is what lets
  a small picture stretch to any width without smearing its corners**, and
  it is what a bubble or a frame wants. Leave it out and the picture is
  stretched corner to corner.
- `mode` — `sliced` (the default when you give insets), `stretch`, or
  `fit` for an icon that should keep its aspect inside its box.
- `tint` — `#rrggbb`, multiplied into the picture. Or the special value
  **`"state"`**, meaning *whatever colour the board would have used
  there*: a rezzed ice tile's faction colour, a HUD readout's alarm red,
  the accent on a server column welcoming a drag. Draw once in white and
  let the game colour it.
- `shadows` — set `false` when your pictures carry their own drop
  shadows, so the board does not add a second set.

A missing or malformed manifest costs the skin its pictures and never
stops the client. You will notice because the name stops changing.

## How big to draw

**Two pixel scales, and the difference matters.** The game lays out in
*logical* pixels and draws in *physical* ones; the ratio is your display
scale. On the machine these numbers were measured on it is **1.25**, so a
box the layout calls 44 tall is 55 real pixels. On a HiDPI laptop it is
often 2.

So: **the sizes below are logical. Author at 2× them.** A picture drawn
larger than its box is scaled down and looks fine; one drawn smaller is
scaled up and looks soft. 2× covers every scale factor in normal use and
costs a few kilobytes.

**For anything nine-sliced, the source size barely matters.** Only the
corners and edges are drawn at a fixed size — the middle is stretched as
far as it needs to go. A 64×64 picture with 16px insets dresses a box of
any width. Draw the corner detail you want at 16–24px and stop worrying
about the rest.

These were measured on a real board, not derived from the constants:
2048×1280 logical, 1.25 scale, cards at their maximum width. On a smaller
window the *scaling* boxes shrink and the fixed ones do not.

### The servers — what you asked about first

| Key | Logical box | Fit | Notes |
|---|---|---|---|
| `server.column` | 242 × grows with its tiles | 9-slice | The frame around a server. Width is the card width + 22, so it shrinks on a small window |
| `server.column.welcomes` | same | 9-slice | A card is being dragged and could land here |
| `server.column.run` | same | 9-slice | A run is on this server |
| `server.header` | fits its text (~97) × **31** | 9-slice | "Archives · 2", "R&D · 35", "New remote 1" |
| `server.header.welcomes` | same | 9-slice | |
| `tile` | card width + 4 (~224) × **26** | 9-slice | An ice, or a card in a root. Wide and short — draw the detail at the ends |
| `tile.rezzed` / `tile.unrezzed` | same | 9-slice | `"tint": "state"` gives you the faction colour free |

### Buttons and chips

| Key | Logical box | Fit | Notes |
|---|---|---|---|
| `button` | 74–154 × **44** | 9-slice | The control bar, the menus, every screen |
| `button.hover` / `.pressed` / `.disabled` | same | 9-slice | |
| `compact.button` | ~85 × **31** | 9-slice | Stack, Heap, and the base a server header builds on |
| `compact.button.hover` / `.pressed` | same | 9-slice | |
| `run.chip` | 47–91 × **28** | 9-slice | A pill in the run lane |
| `run.chip.origin` / `.upcoming` / `.current` / `.done` / `.success` / `.ended` | same | 9-slice | The server, the ice ahead, where the run is, what it did |
| `phase.chip` | 86–149 × **25** | 9-slice | A step of the turn. The widest is "Actions · 3 clicks left" |
| `phase.chip.past` / `.now` / `.ahead` | same | 9-slice | |
| `hud.cell` | ~89 × **48** | 9-slice | The plate behind Credits, Clicks, Agendas… |
| `hud.cell.alarm` | same | 9-slice | A live tag, damage, bad publicity |
| `hud.cell.opens` | ~95 × **48** | 9-slice | The Agendas readout, which opens the score area |
| `sub.dot` | **8 × 8** | plain | A subroutine on an ice chip. Draw at 32×32 and let it shrink |
| `sub.dot.pending` / `.broken` / `.resolved` | same | plain | |

**A server's own mark — an Archives badge, an R&D badge — is not a slot
yet**, and it is the first thing you will want. The reason is the first
rule on this page: a mark needs a box of its own in the header, a header
with one is wider, and a wider header widens its column, which is a
layout change rather than a skin. Reserving that box for every server
whether or not anybody has drawn a mark is the way to do it, and it is a
change to the board. It comes with the panels and the other icons, next.

The panels, menus and sheets, and the remaining icons — the gear, the
arrows, the card frame — are not slots yet either, for less interesting
reasons. Also next.

## Seeing it

Run the game with your skin selected. To look at a mid-game board without
playing one:

    NETRUNNER_GAME=runner NETRUNNER_AUTOPLAY=40 \
      NETRUNNER_SCREENSHOT=/tmp/board.png cargo run -p netrunner_desktop

`NETRUNNER_TABLE_GUIDE=1` draws the board's rows with their real sizes
over the top, which is also the quickest way to see how wide a tile
actually is on your monitor.

## Licensing

Nothing goes in *this* directory — the one inside the repository —
without a `LICENSE-<name>.txt` beside it granting redistribution under
terms compatible with this project's GPL-3.0-or-later.
`assets/fonts/LICENSE-OFL.txt` is the worked example.

Your own skins in `<data dir>` are yours and are not covered by any of
this.
