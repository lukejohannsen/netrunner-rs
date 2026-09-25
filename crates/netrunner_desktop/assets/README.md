# Assets — everything the client can load, and how big to draw it

Every picture, font and sound the desktop client uses is listed here. Each
comes in the three tiers AGENTS.md §5 sets out:

1. **Drawn or synthesized**: made in code, needs no file, always works.
   The client ships looking finished with every folder below empty.
2. **Bundled**: a file in this directory. Each is a separately licensed
   work, with its owner and licence recorded in [CREDITS.md](CREDITS.md).
3. **Yours**: the same path under `<data dir>/netrunner/assets/`
   (`~/.local/share/netrunner/assets/` on Linux). It wins over both and
   is never version-controlled, so it is where to work.

Card fronts are the exception. They are fetched into the cache directory
on the player's opt-in and never committed. The official card backs and
NetrunnerDB's icon font used to be fetched the same way; both are now
committed and credited in `CREDITS.md`.

**Sizes are logical pixels. Author at 2× them.** The game lays out in
logical pixels and draws in physical ones. The ratio is the display
scale: 1.25 on the machine these were measured on, 2 on many laptops.

## The list

| What | Path | Box (logical) | Draw at | Fit | Default that always works | Guide |
|---|---|---|---|---|---|---|
| Body font | `fonts/NotoSans-Regular.ttf` | — | — | — | committed (OFL) | `fonts/LICENSE-OFL.txt` |
| Symbol font | `fonts/NotoSansSymbols2-Regular.ttf` | — | — | — | committed (OFL); the icon font draws over it | `fonts/LICENSE-OFL.txt` |
| Icon font: the printed symbols, the factions' and sets' marks | `fonts/NetrunnerDB-Icons.ttf` | — | — | — | committed (MIT; the marks are Null Signal Games'); without it the symbol font's glyphs, and factions named in words | `fonts/LICENSE-NetrunnerDB.txt` |
| Card backs | `cards/back-corp.png`, `cards/back-runner.png` | a card: 72–220 wide on the board, 380 in a sheet, always 5:7 | **500 × 700** | stretch | drawn circuit backs (`src/card_back.rs`, 250 × 350); the official backs are fetched on opt-in | [cards/](cards/README.md) |
| Card fronts | — (cache only) | as a card back | NetrunnerDB's 750 × 1050, resampled to each face's width | stretch | the text face, drawn from the card's data | [cards/](cards/README.md) |
| Splash | `backdrops/splash.jpg`, optional `backdrops/splash-logo.png` | the whole window; the logo up to 720 × 360 | **2560 × 1600**; logo **1440 × 720** | cover; logo fit | the flat ground and the drawn wordmark (`src/screens/splash.rs`) | [backdrops/](backdrops/README.md) |
| Screen backdrops | `backdrops/<screen>.jpg`, shared `backdrops/menu.jpg`, optional `backdrops.json` | the whole window, behind each non-board screen (the deck builder included) | **2560 × 1600** | cover (cropped, never stretched), dimmed under the text | the theme's flat ground (`src/backdrop.rs`) | [backdrops/](backdrops/README.md) |
| Table | `tables/<name>/base.jpg`, optional `overlay.png` and `table.json` | the whole window | the monitor's full physical resolution (2560 × 1600 here) | stretch, not cropped | a painted perspective grid (`src/table.rs`, 640 × 360): the fallback only, never in the random rotation | [tables/](tables/README.md) |
| Skin | `skins/<name>/skin.json` and its PNGs | per slot: 33 slots, from an 8 × 8 dot to a 960-wide panel | per slot, usually a small nine-slice | nine-slice, stretch or fit | the outlines: flat colours and one-pixel borders | [skins/](skins/README.md) |
| Server plates | `board/server.<archives\|rnd\|hq\|remote>[.run].png` | 224 × 126 at the largest card, 16:9 | **512 × 288** | cover (cropped, never stretched) | drawn buildings (`src/board_art.rs`) | [board/](board/README.md) |
| Corp styles | `board/corp/<faction>/<key>.png`, any board key below | as the generic key | as the generic key | as the generic key | the generic key; drawn plates lit in the faction's colour | [board/](board/README.md#corp-styles) |
| ICE and root tiles | `board/ice.*.png`, `board/root.*.png` | card width + 4 × 24 to 66, 4:1 at the tallest | **512 × 128** | cover | drawn grey patterns, washed in the tile's state colour | [board/](board/README.md) |
| Counter badges | `board/counter[.advancement\|.virus\|.power\|.credit].png` | 12 to 18 square | **64 × 64** | fit | bundled NSG symbols over a drawn ring | [board/](board/README.md) |
| HUD glyphs | `board/hud.<credits\|clicks\|agendas\|bad-publicity\|tags\|damage>.png` | 18 square | **64 × 64** or larger | fit | bundled NSG symbols (optional) | [board/](board/README.md) |
| Avatar bar | `board/avatar.bar[.active].png` | 48 tall, half the board wide each wing (36 on the far side) | **480 × 96**, ends 160 each | nine-sliced, the right wing mirrored | bundled steel; drawn grey plate washed in the side's colour | [board/](board/README.md#the-avatar-bar) |
| Avatar ring | `board/avatar.frame[.active].png` | 72 square (54 on the far side) | **256 × 256** | stretched to the disc | bundled steel ring; drawn grey ring | [board/](board/README.md#the-avatar-bar) |
| Sound | `sfx/<effect>.ogg` | — | — | — | synthesized; planned, nothing loads yet | [sfx/](sfx/README.md) |

A skin may carry its own `board/` folder, which wins over the board art
tiers above, so a skin can bring its buildings with it. The Corp's style
comes next, chosen by the faction of the Corp's identity, then the
generic board art.

**The drawn tier is the fallback, not the look.** The client is meant to
ship with pictures in every slot: several tables, drawn at random per
match, a backdrop for every screen, and buildings for every Corp. The
drawn pictures appear only where nothing is installed, or everywhere
when the player turns on Settings → **Basic graphics (slow machines)**,
which loads no pictures at all. They are never one of Random's picks.

**Every place in the client has its slot before its art exists.** A new
screen gets a backdrop key and a row in [backdrops/](backdrops/README.md)
in the same change (a test enforces it), and a new board element gets a
key in [board/](board/README.md) the same way.

## Rules every asset follows

- **A picture never changes a size.** Every box is laid out by the game
  so the board fits the window without scrolling. Your picture is fitted
  to a box it does not get to argue with.
- **Anything undrawn falls back.** An undrawn state borrows its base
  picture, and an undrawn base is drawn. A folder holding one file is a
  real skin, table or set of plates.
- **SVG is not supported.** Bevy has no SVG rasteriser. Author in SVG if
  you like, and export PNG (or JPEG for a table).
- **Every file committed here has a row in [CREDITS.md](CREDITS.md)**:
  its owner, their website, its licence and anything changed. An asset
  made in the project, including with AI assistance, is GPL-3.0-or-later.
  Anyone else's keeps its owner's licence, with the licence text beside
  it. Find out who made a file before it goes in. A test fails on a file
  without a row, or a row without a file.
