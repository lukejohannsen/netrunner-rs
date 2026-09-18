# Board art — the pictures on the table

A skin dresses the board's boxes in frames. **Board art is the picture
inside a box**: the building on a server's plate, the slab behind an
ICE's name, the badge beside a counter and a HUD readout's glyph. A frame is nine-sliced and
stretched to fit; a building keeps its proportions, so board art has its
own folder and its own rule.

**Every plate, tile and counter has a picture with no file anywhere.**
The client draws a default (`src/board_art.rs`): for a plate, a
silhouette in the Corp's colour against a dusk with lit windows in the
accent; for a tile, a grey pattern; for a counter, a ring. HUD glyphs
are optional and have no drawn default. They are
deliberately plain, so they read as places at plate size and never
pretend to be anybody's art. Draw a file and it replaces the drawn one.

## Where they go

First found wins:

| Tier | Path |
|---|---|
| The skin in use | `skins/<skin>/board/<key>.png` (bundled or yours) |
| Yours | `<data dir>/netrunner/assets/board/<key>.png` |
| Bundled | `crates/netrunner_desktop/assets/board/<key>.png` |
| Drawn | — (always works) |

On Linux `<data dir>` is `~/.local/share`. A skin that carries a `board/`
folder brings its buildings with it, so a table, its chrome and its
skyline can travel together.

## A picture is cropped to cover its box, never stretched

Every box below has a fixed shape at any card size, and your picture is
scaled until it covers the box and then cropped to the middle. **Draw to
the shape given** and nothing is lost. Draw another shape and the edges
go: a square picture on a 16:9 plate loses its top and bottom. Keep what
matters in the centre.

PNG, eight-bit sRGB, with or without alpha. Sizes are *logical* pixels,
measured at the largest card the board draws (`MAX_FACE`, 220). **Author
at 2× the logical size.** A larger picture is scaled down and looks fine;
a smaller one is scaled up and looks soft.

## The server plates

Each server's plate sits on the Corp's edge of its column: at the bottom
of the screen from the Corp's chair, at the top from the Runner's. The
server's name and count run along a band at its lower edge, so keep the
bottom sixth quiet.

| Key | Shows | Logical box | Draw at | Drawn default |
|---|---|---|---|---|
| `server.archives` | Archives | 224 × 126 (Corp chair), 169 × 95 (Runner chair) | **512 × 288** (16:9) | a low vault under a pediment, shelving in bands |
| `server.rnd` | R&D | same | same | a tower of data lines between two shorter ones |
| `server.hq` | HQ | same | same | an office block with a setback, a spire and a lit lobby |
| `server.remote` | every remote server | same | same | a server rack in the open, a mast |
| `server.archives.run` | Archives while a run is on it | same | same | none: `server.archives` |
| `server.rnd.run` | R&D under a run | same | same | none: `server.rnd` |
| `server.hq.run` | HQ under a run | same | same | none: `server.hq` |
| `server.remote.run` | a remote under a run | same | same | none: `server.remote` |

**A state borrows its base picture until you draw it**, one level deep,
as a skin slot does. Draw `server.hq.run.png` and HQ under attack looks
different; don't, and it looks like HQ.

The plate is `card width + 4` wide and 9/16 of that tall. The card width
is computed from the window, so on a smaller screen the box is smaller
and the same picture is scaled down. The opponent's side of the table is
drawn at three quarters of the person's own, which is why the Runner's
chair sees the smaller plate.

## The tiles

An ICE or a card in a server's root is a tile in its column. The tile
shows a picture of what kind of card it is, with its name and tokens on
a band across the middle. The picture is **cropped to cover**, like a
plate, but the tile's height changes with the ICE field: tall with one
ICE, down to 24 as the column fills. So draw wide, and keep the detail
in a band through the middle.

**A drawn tile is grey, washed in the tile's state colour.** That is the
faction's colour for a rezzed card and the Corp's colour dimmed for a
face-down one, the same colour as the tile's border. A file is drawn as
you painted it, not washed.

| Key | Shows | Logical box | Draw at | Drawn default |
|---|---|---|---|---|
| `ice.unrezzed` | any face-down ICE (its type is hidden) | card width + 4 × 24 to 66 | **512 × 128** (4:1) | diagonal hatching |
| `ice.rezzed` | a rezzed ICE of no type below | same | same | scanlines |
| `ice.rezzed.barrier` | a rezzed barrier | same | same | a wall of bricks |
| `ice.rezzed.code-gate` | a rezzed code gate | same | same | bars with a lock in the middle |
| `ice.rezzed.sentry` | a rezzed sentry | same | same | rings around a sight |
| `root.unrezzed` | a face-down asset or upgrade | same | same | hatching the other way |
| `root.rezzed` | a rezzed root card of no type below | same | same | a riveted panel |
| `root.rezzed.asset` | a rezzed asset | same | same | a row of coins |
| `root.rezzed.upgrade` | a rezzed upgrade | same | same | chevrons |
| `root.agenda` | an agenda, face up or not | same | same | diamonds |

A type you haven't drawn borrows its base: `ice.rezzed.sentry` borrows
`ice.rezzed`, and `root.rezzed.asset` borrows `root.rezzed`. But every
key in this table has a drawn default of its own, so the base only
matters once you start replacing the drawings.

## Counters

A counter on a tile, or under a card in the rig, is a badge beside its
number: `2/3` advancement on an agenda, `3` virus counters on a virus
program.

| Key | Counts | Logical box | Draw at | Default that always works |
|---|---|---|---|---|
| `counter` | a counter of a kind the board has no picture for | 12 to 18 square | **64 × 64** | a drawn ring |
| `counter.advancement` | advancement tokens | same | same | bundled: NSG's advancement counter |
| `counter.virus` | virus counters | same | same | bundled: NSG's virus counter |
| `counter.power` | power counters | same | same | bundled: NSG's generic counter |
| `counter.credit` | credits hosted on a card | same | same | bundled: NSG's credit |

## The HUD's glyphs

Each HUD readout's glyph sits before its number, with the word still
underneath. These are optional: without a picture, the number stands
alone.

| Key | Shows | Logical box | Draw at | Bundled |
|---|---|---|---|---|
| `hud.credits` | the Credits readout | 18 square | **64 × 64** or larger | NSG's credit |
| `hud.clicks` | the Clicks readout | same | same | NSG's click |
| `hud.agendas` | the Agendas readout | same | same | NSG's agenda |
| `hud.bad-publicity` | the Bad pub. readout | same | same | NSG's bad publicity |
| `hud.tags` | the Tags readout | same | same | NSG's tag |
| `hud.damage` | the Damage readout | same | same | NSG's core damage |

A glyph keeps its own shape (it is fitted, not cropped). Draw it square
on a transparent background.

**The bundled counters and glyphs are Null Signal Games' own game
symbols**, from their public visual-assets pack, under CC BY-ND 4.0. See
[`LICENSE-NSG.txt`](LICENSE-NSG.txt) for the attribution, the source of
each file and the two changes made: conversion to PNG, and recolouring
the black ones light. They are the one committed asset in this client
that is not under a GPL-compatible grant, and they must not be edited
into new symbols. A file of the same name in your own directory
replaces any of them.

## Seeing it

    NETRUNNER_GAME=corp NETRUNNER_AUTOPLAY=40 \
      NETRUNNER_SCREENSHOT=/tmp/plates.png cargo run -p netrunner_desktop

Run it through `cargo run`, not the binary in `target/`, or the bundled
folder is not found.

## Licensing

Nothing goes in *this* directory, the one inside the repository, without
a `LICENSE-<name>.txt` beside it. The project's bar is a grant compatible
with its GPL-3.0-or-later: CC0 or an OFL-style licence, as
`assets/fonts/LICENSE-OFL.txt` shows. **NSG's symbols are the recorded
exception** (`LICENSE-NSG.txt`, CC BY-ND 4.0), bundled by the project
owner's decision. Anything new still meets the bar. Your own directory
under `<data dir>` has no such rule. It is yours.
