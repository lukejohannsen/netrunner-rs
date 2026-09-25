# Board art — the pictures on the table

A skin dresses the board's boxes in frames. **Board art is the picture
inside a box**: the nameplate a server's or a rig row's name sits in,
the slab behind an ICE's name, the badge beside a counter and a HUD
readout's glyph. It has its own folder so a Corp's faction and a run
can change it.

**Every plate, tile and counter has a picture with no file anywhere.**
The client draws a default (`src/board_art.rs`): for a nameplate, a
grey frame edged in the Corp's colour (or the Runner's, for a rig row);
for a tile, a grey pattern; for a counter, a ring. HUD glyphs
are optional and have no drawn default. They are
deliberately plain, so they read at board size and never
pretend to be anybody's art. Draw a file and it replaces the drawn one.

## Where they go

First found wins:

| Tier | Path |
|---|---|
| The skin in use | `skins/<skin>/board/<key>.png` (bundled or yours) |
| The Corp's style | `board/corp/<faction>/<key>.png` (bundled or yours) |
| Generic | `board/<key>.png` (yours, then bundled) |
| Drawn | — (always works) |

Each file tier is looked for under `<data dir>/netrunner/assets/` first
and then under `crates/netrunner_desktop/assets/`. On Linux `<data dir>`
is `~/.local/share`. A skin that carries a `board/` folder brings its
nameplates with it, so a table, its chrome and its frames can travel
together.

A state with no file (`plate.hq.run`) uses its base (`plate.hq`) **from
the same tier before the next tier is tried**, so a faction's HQ stays
that faction's HQ when it is run on, even if a generic run picture
exists.

**Under Settings → Basic graphics no file is read at all**: every nameplate,
tile and counter is drawn, and the HUD has no glyphs. That is the
no-frills mode for a slow machine; the drawn pictures are otherwise only
the fallback for a key nobody has drawn.

## Corp styles

The Corp's board can be styled by the **faction of the Corp's identity**,
so a Jinteki board wears Jinteki nameplates and a Weyland board
Weyland's. The identity is public, so both chairs see the same style.

| Folder | Chosen when the Corp's identity is |
|---|---|
| `corp/haas-bioroid/` | Haas-Bioroid |
| `corp/jinteki/` | Jinteki |
| `corp/nbn/` | NBN |
| `corp/weyland-consortium/` | Weyland Consortium |
| `corp/neutral-corp/` | a neutral Corp identity |

**Any key below can go in a faction's folder**: the nameplates, the ICE
and root tiles, the counters and the HUD's glyphs. A folder holding a
single picture is a real style, and every key it leaves out is the
generic one, so a style can be built one frame at a time. The
sizes are the same as the generic keys'.

The drawn server nameplates are edged in the Corp faction's colour (a
neutral Corp keeps the Corp's blue); the shipped frames are the Corp's
blue for every faction until a style draws its own.

## A picture is cropped to cover its box, never stretched

Every box below has a fixed shape at any card size, and your picture is
scaled until it covers the box and then cropped to the middle. **Draw to
the shape given** and nothing is lost. Draw another shape and the edges
go: a square picture in a wide box loses its top and bottom. Keep what
matters in the centre. (A nameplate or a strip frame is nine-sliced
instead — see below.)

PNG, eight-bit sRGB, with or without alpha. Sizes are *logical* pixels,
measured at the largest card the board draws (`MAX_FACE`, 220). **Author
at 2× the logical size.** A larger picture is scaled down and looks fine;
a smaller one is scaled up and looks soft.

## The nameplates

Every name on the table sits in a frame: each server's, on the Corp's
edge of its column (at the bottom from the Corp's chair, at the top from
the Runner's), and each rig row's, at the row's left. A nameplate is a
strip's frame (below) with the name over its channel, and is drawn the
same way: **512 × 96, nine-sliced, a 48-pixel end cap at each side kept
whole** and a middle stretched along its length, so keep the middle
plain along x, put the detail in the caps, and keep the channel dark —
the name sits there.

A server stood on a 16:9 picture of a building until 25 September 2026.
It was replaced by a nameplate a strip tall at the person's request, so
the height the pictures took goes to the installs.

| Key | Shows | Logical box | Draw at | Shipped frame | Drawn default |
|---|---|---|---|---|---|
| `plate.archives` | Archives' name | card width + 4 × 26 to 40 | **512 × 96**, 48 px caps | steel, Corp-blue traces, shelving etched in the channel | a grey frame edged in the Corp's faction colour |
| `plate.rnd` | R&D's name | same | same | the same, a stream of data lines | same |
| `plate.hq` | HQ's name | same | same | the same, office windows | same |
| `plate.remote` | every remote's name | same | same | the same, rack slots | same |
| `plate.archives.run` | Archives while a run is on it | same | same | the Archives frame in the Runner's red | none: `plate.archives` |
| `plate.rnd.run` | R&D under a run | same | same | the R&D frame in red | none: `plate.rnd` |
| `plate.hq.run` | HQ under a run | same | same | the HQ frame in red | none: `plate.hq` |
| `plate.remote.run` | a remote under a run | same | same | the remote frame in red | none: `plate.remote` |
| `plate.programs` | the rig's Programs row | 78 × 26 to 40 | same | steel, the Runner's red | a grey frame edged in the Runner's colour |
| `plate.hardware` | the rig's Hardware row | same | same | same | same |
| `plate.resources` | the rig's Resources row | same | same | same | same |

**A state borrows its base picture until you draw it**, one level deep,
as a skin slot does. The shipped frames draw every `.run` state; a style
that draws only `plate.hq` has HQ under attack look like its HQ.

## The tiles

An ICE or a card in a server's root is a strip in its column: one line
of words — its name, rezzed or not, its strength and tokens — on a band
across the middle of a frame that says what kind of card it is. A column
is a fixed number of strips (three on a 768-tall window, four at 1080,
five above), and a server holding more folds to its root, a "+N" strip
and its outermost ICE; the whole server is its stack sheet. A strip is
the card's width + 4 wide and 26 to 40 tall, whatever the column holds.

**A file is drawn nine-sliced:** 512 × 96, with a 48-pixel end cap at
each side that is kept whole and a middle that is stretched along its
length, so keep the middle plain along x and put the detail in the caps.
The words sit over the middle, so keep it dark. The shipped frames are
brushed steel with the kind's colour in their traces and a faint etch in
the channel, painted by `scripts/paint_tiles.py`. **The colours are the
person's** (25 September 2026): a barrier is gold, a code gate blue, a
sentry red, and a card face down grey — `Theme::tile` holds the same
colours for the drawn tier and the frame's border, and its test keeps
them clear of the glows under colour blindness. A drawn tile is grey and
covers the box, washed in the kind's colour; a file is drawn as you
painted it, not washed.

| Key | Shows | Logical box | Draw at | Shipped frame | Drawn default |
|---|---|---|---|---|---|
| `ice.unrezzed` | any face-down ICE (its type is hidden) | card width + 4 × 26 to 40 | **512 × 96**, 48 px caps | grey steel, unlit, hatched | diagonal hatching |
| `ice.rezzed` | a rezzed ICE of no type below | same | same | gunmetal, white traces | scanlines |
| `ice.rezzed.barrier` | a rezzed barrier | same | same | gold traces, a course of bricks | a wall of bricks |
| `ice.rezzed.code-gate` | a rezzed code gate | same | same | blue traces, lock bars | bars with a lock in the middle |
| `ice.rezzed.sentry` | a rezzed sentry | same | same | red traces, sight rings | rings around a sight |
| `root.unrezzed` | a face-down asset or upgrade | same | same | grey steel, hatched the other way | hatching the other way |
| `root.rezzed` | a rezzed root card of no type below | same | same | gunmetal, white traces, rivets | a riveted panel |
| `root.rezzed.asset` | a rezzed asset | same | same | bronze traces, coins | a row of coins |
| `root.rezzed.upgrade` | a rezzed upgrade | same | same | teal traces, chevrons | chevrons |
| `root.agenda` | an agenda, face up or not | same | same | green traces, an advancement rail | diamonds |

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
| `hud.damage` | the Core damage readout | same | same | NSG's core damage |

A glyph keeps its own shape (it is fitted, not cropped). Draw it square
on a transparent background.

**The bundled counters and glyphs are Null Signal Games' own game
symbols**, from [their visual-assets pack](https://nullsignal.games/about/nsg-visual-assets/),
under CC BY-ND 4.0. Each is recorded in [`../CREDITS.md`](../CREDITS.md),
and [`LICENSE-NSG.txt`](LICENSE-NSG.txt) holds the attribution and the two
changes made: conversion to PNG, and recolouring the black ones light.
They must not be edited into new symbols. A file of the same name in
your own directory replaces any of them.

## The avatar bar

Each side has a bar across the board at its edge of the table, above the
person's own hand and under the opponent's. The identity's art sits in
a disc in the middle of the bar, and the side's numbers run along the
plate on either side of it. The bar of the side whose turn it is is lit
(`.active`); the other side's is grey.

**The plate is one wing, drawn once.** It is nine-sliced: the outer end
(left) and the inner end (right, which runs in under the disc) are each
kept whole at a third of the picture's width, and the middle third is
stretched to whatever the board's width leaves. The right-hand wing is
the same picture mirrored, which works only because the two ends are
the same width, **so keep both ends 160 px wide** and put nothing in the
middle third that would show being stretched sideways: straight brushing
and straight traces stretch, while a jog or a via is a smear. The text
runs across the top two thirds; keep the lowest third for the channel,
and the outer 144 px clear of anything the name would sit on.

| Key | Shows | Logical box | Draw at | Bundled |
|---|---|---|---|---|
| `avatar.bar` | a wing of the bar, on the other side's turn | 56 tall on both sides, the board's width shared by two | **480 × 96**, ends 160 each | brushed steel, dark traces |
| `avatar.bar.active` | a wing of the bar, on this side's turn | same | same | brushed steel, lit purple traces |
| `avatar.frame` | the ring round the avatar, on the other side's turn | 84 square on both sides | **256 × 256**, transparent inside from 0.83 of the radius | a steel ring |
| `avatar.frame.active` | the ring, on this side's turn | same | same | a steel ring with a lit trace |

The bundled four are painted by `scripts/paint_avatar_bar.py` after the
shared menu backdrop's brushed steel and purple circuits. The drawn
defaults are a plain grey plate and ring, washed in the side's colour on
its turn.

## Seeing it

    NETRUNNER_GAME=corp NETRUNNER_AUTOPLAY=40 \
      NETRUNNER_SCREENSHOT=/tmp/plates.png cargo run -p netrunner_desktop

Run it through `cargo run`, not the binary in `target/`, or the bundled
folder is not found.

## Licensing

Every file in *this* directory, the one inside the repository, is a
separately licensed work with a row in [`../CREDITS.md`](../CREDITS.md).
If the project made it, including with AI assistance, it is
GPL-3.0-or-later. If someone else did, it keeps its owner's licence,
the licence text sits beside it, and the row names the owner and their
website. Find out who made a picture before committing it. Your own
directory under `<data dir>` has no such rule. It is yours.
