# Tables — the field the board is played on

A **table** is the picture under the board. It is where the board's sense
of depth comes from: every card on the board is drawn at one size, so the
perspective has to be in the art.

Nothing is committed here yet. The client paints a ground of its own
(`src/table.rs`) that needs no files at all, so the game looks finished
without this directory existing.

## Where they go

Three tiers, the same rule as the card backs and the fonts:

| Tier | Path | Wins |
|---|---|---|
| Painted | — | always works, needs no files |
| Bundled | `crates/netrunner_desktop/assets/tables/<name>/` | over painted |
| Yours | `<data dir>/netrunner/assets/tables/<name>/` | over both |

On Linux `<data dir>` is `~/.local/share`, so your own tables go in
`~/.local/share/netrunner/assets/tables/`. **That is the directory to use
while you are making one** — nothing there is version-controlled, and it
overrides a bundled table of the same name file by file, so you can
replace just the `base.jpg` of a bundled table and inherit the rest.

A folder is a table as soon as it has a `base.jpg` in it. The name of the
folder is the name of the table.

## What goes in one

    tables/neon-alley/
      base.jpg      the field. required.
      overlay.png   optional: an alpha layer over the field, under the cards
      table.json    optional: the name and its colours

**`base.jpg` — JPEG, not PNG.** The field is photographic and has no
transparency, and at 2560×1440 a JPEG is about a megabyte where the PNG
is six. Any size works; it is stretched to the window, so match your
monitor's aspect ratio or expect it to be squashed.

**`overlay.png` — PNG, because this one *does* need alpha.** A vignette,
a frame, a wash of colour, a logo in a corner. It is drawn over the field
and under the cards.

**SVG is not supported**, and will not be: Bevy has no SVG rasteriser,
and adding one for a backdrop would be a dependency for nothing. Author
in SVG if you like, and export.

### `table.json`

Every field is optional. A table with no manifest is named after its
folder and keeps the client's own colours. A manifest that fails to parse
costs the table its colours and never its picture — you will notice
because the name stops changing.

```json
{
  "name": "Neon Alley",
  "accent": "#1ec8b4",
  "ink": "dark"
}
```

- `name` — what the Table row in Settings calls it. The folder name if
  unset.
- `accent` — a colour picked out of the field, as `#rrggbb` (or `#rgb`,
  with or without the `#`), for the board to mark its live edges with.
- `ink` — `dark` or `light`: whether the field is dark enough to read
  light text on. `dark` by default, which is what this client is built
  for.

## Choosing one

Settings → **Table**, and the same row in the board's gear menu. It
cycles the painted ground, then each installed table, then **Random**,
which is offered once there is more than one table to shuffle.

Random draws **once per match**, not per frame — a ground that changed
under the cards mid-game would be a distraction rather than a flourish.

`painted` and `random` are reserved, so a folder by either name is
ignored: the setting is stored as a bare string, and a table by one of
those names could never be selected.

## Painting one that fits the board

The board is laid out to the window and never scrolls, so where the rows
fall depends on the window and on what is on the table. Rather than guess:

    NETRUNNER_GAME=runner NETRUNNER_AUTOPLAY=40 NETRUNNER_TABLE_GUIDE=1 \
      NETRUNNER_SCREENSHOT=/tmp/guide.png cargo run -p netrunner_desktop

That draws a band over every row of the board, labelled with its
position and height, on top of whichever table is selected. Paint your
horizon and vanishing point against those, then run it again without
`NETRUNNER_TABLE_GUIDE` to see the result.

Two things worth knowing while you paint:

- **Keep the perspective shallow.** The cards are flat rectangles at one
  size. A steeply-raked floor under them reads as stickers on a
  photograph; a gentle one reads as a table.
- **Leave the middle quiet.** The cards sit across the centre and the
  run lane runs between the two sides. Detail belongs at the edges, where
  the vignette is.

## Licensing

Nothing goes in *this* directory — the one inside the repository —
without a `LICENSE-<name>.txt` beside it granting redistribution under
terms compatible with this project's GPL-3.0-or-later. CC0 or an
OFL-style grant is the bar; `assets/fonts/LICENSE-OFL.txt` is the worked
example.

Your own tables in `<data dir>` are yours and are not covered by any of
this.
