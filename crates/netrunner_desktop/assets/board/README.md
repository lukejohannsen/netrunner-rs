# Board art — the pictures on the table

A skin dresses the board's boxes in frames. **Board art is the picture
inside a box**: the building on a server's plate, and, once the tiles
have art, the slab behind an ICE's name. A frame is nine-sliced and
stretched to fit; a building keeps its proportions, so board art has its
own folder and its own rule.

**Every key has a picture with no file anywhere.** The client draws a
default for each base key (`src/board_art.rs`): a silhouette in the
Corp's colour against a dusk, with lit windows in the accent. They are
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

## Seeing it

    NETRUNNER_GAME=corp NETRUNNER_AUTOPLAY=40 \
      NETRUNNER_SCREENSHOT=/tmp/plates.png cargo run -p netrunner_desktop

Run it through `cargo run`, not the binary in `target/`, or the bundled
folder is not found.

## Licensing

Nothing goes in *this* directory, the one inside the repository, without
a `LICENSE-<name>.txt` beside it granting redistribution under terms
compatible with this project's GPL-3.0-or-later. CC0 or an OFL-style
grant is the bar; `assets/fonts/LICENSE-OFL.txt` is the worked example.
Your own directory under `<data dir>` has no such rule. It is yours.
