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
    "hud.cell":       { "file": "plate.png", "insets": 5, "tint": "state" },
    "panel":          { "file": "panel.png", "insets": 24 },
    "panel.sheet":    { "file": "sheet.png", "insets": 24 },
    "overlay.scrim":  { "file": "wash.png", "mode": "stretch" }
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
| `server.column` | card width + 22 × the whole ICE field | 9-slice | The frame around a server. Width is the card width + 22, so it shrinks on a small window |
| `server.column.welcomes` | same | 9-slice | A card is being dragged and could land here |
| `server.column.run` | same | 9-slice | A run is on this server |
| `server.header` | card width + 4 × **9/16 of that** (~169 × 95 from the Runner's chair, ~224 × 126 from the Corp's) | 9-slice | The server's plate on the Corp's edge of the table: "Archives · 2", "R&D · 35", "New remote 1" along its lower edge |
| `server.header.welcomes` | same | 9-slice | |
| `tile` | card width + 4 × **24 to 0.3 × card width** | 9-slice | An ice, or a card in a root. Its height is its share of the ICE field — tall with one ICE, down to 24 as the column fills, then overlapping. Wide and short — draw the detail at the ends |
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

### Panels, menus and the sheet

Every box that is not the board. `panel` is the base and it reaches
*every* panel in the client, the main menu and the settings screen
included, because `widgets::panel` is one function — so the five states
below exist to let you dress the game without repainting the menus. Draw
`panel.png` alone and all six are dressed; draw `panel.sheet.png` as
well and the sheet parts company with the main menu.

| Key | Logical box | Fit | Notes |
|---|---|---|---|
| `panel` | 414–960 wide | 9-slice | The base, and every panel that names no state of its own — the main menu, settings, profile, the new-game form |
| `panel.sheet` | 414–960 × 300–700 | 9-slice | The centred panel an overlay puts up: a card's sheet (414 × ~570), an install's (800 × ~550), a zone's (960), the options (560), the list of keys |
| `panel.decision` | **520** × 200–700 | 9-slice | The pop-up that asks you something — at an access it carries the card too, which is what makes it the tall one |
| `panel.menu` | **280** × grows per row | 9-slice | The menu a card's click opens. About 100 tall for two entries, 40 per entry after |
| `panel.phase` | **380** × ~150 | 9-slice | The phase panel at the top of the right column: the turn's steps in one column and a run's in the next |
| `panel.run` | **380** × ~300 | 9-slice | Where the Runner's identity appears while a run is on — its picture cropped to the name and the art, or its name alone. Frame it like a monitor someone is breaking into |
| `overlay.scrim` | the whole window | stretch | The wash over the board behind a sheet. **Not** a state of `panel` — it is what sits *behind* one, so it borrows nothing and stays a flat wash until you draw it. Keep it mostly transparent or the board vanishes |

The decision pop-up and the menu are drawn with the accent as their
border rather than the panel border, so a frame you draw for `panel`
will read a little differently on them. That is the only difference
between them.

**The read sheet has no Close button and nothing above the card**, so do
not leave room for either: a card says its own name on its face, and the
sheet closes on Escape or on a click that misses it. A *zone's* sheet,
the score area and the decision pop-up do still carry a heading, and the
options and the list of keys still carry a Close.

**A server's own picture — Archives as a vault, R&D as a tower — has its
box now**: the header became a plate, reserved at 16:9 for every server
whether or not anybody has drawn one, so a picture can never change the
layout. The picture itself is board art rather than a skin slot (a
nine-sliced frame is the wrong shape for a building): see
[`../board/README.md`](../board/README.md). A skin can still carry its
own buildings, in a `board/` folder beside its `skin.json`.

The remaining icons — the gear, the arrows, the card frame — are not
slots yet either, for less interesting reasons. Also next.

## Seeing it

Run the game with your skin selected. To look at a mid-game board without
playing one:

    NETRUNNER_GAME=runner NETRUNNER_AUTOPLAY=40 \
      NETRUNNER_SCREENSHOT=/tmp/board.png cargo run -p netrunner_desktop

`NETRUNNER_SHEET=1` opens a card's sheet over that board, and
`NETRUNNER_HOLD_ACCESS=1` stops at an access so the decision pop-up is
up — the only way to look at either dressed without playing to one.

`NETRUNNER_TABLE_GUIDE=1` draws the board's rows with their real sizes
over the top, which is also the quickest way to see how wide a tile
actually is on your monitor.

## Licensing

A file committed here is a separately licensed work and needs a row in
[`../CREDITS.md`](../CREDITS.md): its owner, their website, its licence
and anything changed. If the project made it, including with AI
assistance, it is GPL-3.0-or-later. If someone else did, it keeps their
licence, with the licence text beside it as `LICENSE-<name>.txt`, and
the artist is credited by name and website. Find out who made it before
committing it.

Your own skins in `<data dir>` are yours and are not covered by any of
this.
