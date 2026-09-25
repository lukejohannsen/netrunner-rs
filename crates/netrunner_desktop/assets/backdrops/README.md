# Backdrops: the picture behind every screen

A **backdrop** is the picture behind a screen that is not the board: the
splash, the main menu, and every screen off it, including the deck
builder. The board has a [table](../tables/README.md) instead.

**Every screen has a slot, whether or not anybody has drawn for it yet.**
A screen gets its slot by being built on the root every screen shares,
and a test fails if a screen's key is missing from this page. So when a
screen is added, its row appears here in the same change, and there is
somewhere to put its picture before the picture exists.

Four pictures are committed here: Kirsten Zirngibl's *New Angeles in
Neon/Smog*, the smog version behind the splash (`splash.jpg`) and the
neon one as the shared picture (`menu.jpg`), behind the main menu and
every screen without a picture of its own; Aurore Folny's *The Personal
Touch* behind the deck list and the deck editor (`decks.jpg`); and Adam
Schumpert's *Maya* behind Play vs Computer (`new-game.jpg`).
`backdrops.json` sets how much each is dimmed and which screens borrow
another's picture. All are
all rights reserved, shipped with credit and removed if their owners ask
(`../CREDITS.md`). A screen with no
picture of its own and no `menu.jpg` is the
drawn backdrop (`nav::drawn_backdrop`): a deep blue falling to near
black, with two soft blooms of light for the menus' glass to catch. That
is the fallback and the basic-graphics mode, not the look: **this folder
is where the shipped backdrops go.** A picture here draws over the
gradient, under the same glass, so a menu shows it through.

## Where they go

First found wins:

| Tier | Path |
|---|---|
| Yours | `<data dir>/netrunner/assets/backdrops/<file>` |
| Bundled | `crates/netrunner_desktop/assets/backdrops/<file>` |
| Drawn | the blue gradient (always works) |

On Linux `<data dir>` is `~/.local/share`, so your own backdrops go in
`~/.local/share/netrunner/assets/backdrops/`. **Use that directory while
you work on one.** Nothing there is version-controlled, and a file there
beats a bundled file of the same name.

For each screen the client looks for:

1. `<key>.jpg`, then `<key>.png`: that screen's own picture;
2. `menu.jpg`, then `menu.png`: the shared picture, which dresses
   **every** screen that has no picture of its own.

One `menu.jpg` is therefore a complete set of menus, and each screen's
own picture improves on it rather than being needed before it.

## The slots

| Key | Screen | Notes |
|---|---|---|
| `menu` | every screen below without its own picture | the shared default |
| `splash` | the title card at startup | shown for a moment and skipped by any key; see below |
| `main-menu` | the main menu | |
| `new-game` | Play vs Computer: the new-game form | |
| `online` | Play Online | |
| `learn` | Learn to Play | |
| `decks` | Decks: the deck list | deck building |
| `deck-editor` | the deck editor | deck building; a busy screen, so keep this quiet or dim it |
| `cards` | Cards: the card browser | a grid of card faces sits over it, so a strong dim is wise |
| `profile` | Profile | |
| `settings` | Settings | |
| `replay` | Replay | |
| `about` | About | whose work is in the client |

The board (`Game`) has no slot here. It stands on a table.

## Size and shape

**JPEG** for a picture: a full-window picture is photographic and has no
transparency, and at 2560 × 1600 a JPEG is about a megabyte where a PNG
is six. **PNG** only when you want alpha over the flat ground.

**Draw at the monitor's full physical resolution: 2560 × 1600** on the
machine these notes were measured on (a 2048 × 1280 logical window at a
1.25 display scale). When in doubt, draw it larger.

**It is cropped to cover the window, never stretched.** The picture is
scaled until it covers the window and then cropped to the middle, so any
shape works and only the edges are lost. Keep what matters inside a
centred 16:10 area and let the edges go. Unlike a table, a backdrop has
no perspective to keep.

**SVG is not supported**, because Bevy has no SVG rasteriser. Export to
JPEG or PNG.

## Dimming, and `backdrops.json`

Every screen draws light text over its backdrop, so the picture is
dimmed under a wash of the theme's ground: 45% by default. The optional
`backdrops.json` beside the pictures sets the dim per key, from `0` (the
picture as drawn) to `1` (the flat ground). A key it does not name takes
`menu`'s value, then the default:

```json
{ "dim": { "menu": 0.5, "cards": 0.75, "splash": 0.0 } }
```

A file with a typo in it costs the dimming and never the picture.

**A screen can show another screen's picture** without a copy of the
file: `same_as` names, per key, the key whose files it is looked up
under. Its dim is still its own.

```json
{ "same_as": { "deck-editor": "decks" } }
```

That is different from `menu.jpg`, which dresses *every* screen with no
picture of its own. It is one step only: the key named is looked up as
itself.

## The splash

The splash is the `splash` slot plus an optional mark drawn over it:

| File | Box (logical) | Draw at | Fit | Drawn default |
|---|---|---|---|---|
| `splash.jpg` (or the shared `menu.jpg`) | the whole window | 2560 × 1600 | cover | the flat ground |
| `splash-logo.png` | up to 720 × 360, centred | **1440 × 720**, with alpha | fit, never enlarged past 1:1 | the word NETRUNNER in the theme's face over an accent bar |

The splash holds for 1.5 seconds and until the fonts have loaded, never
longer than 5 seconds, and any key or click skips it. It is never shown
when a dev hook (`NETRUNNER_SCREEN`, `NETRUNNER_GAME`) names a screen, or
when there is no window.

## Basic graphics

Settings → **Basic graphics (slow machines)** loads no backdrop at all:
every screen is its flat ground and the splash is the drawn wordmark.

## Seeing it

    NETRUNNER_SCREEN=cards NETRUNNER_SCREENSHOT=/tmp/cards.png cargo run -p netrunner_desktop
    NETRUNNER_SCREEN=splash NETRUNNER_SCREENSHOT=/tmp/splash.png cargo run -p netrunner_desktop

Run it through `cargo run`, not the binary in `target/`, or the bundled
folder is not found.

## Licensing

A file committed here is a separately licensed work and needs a row in
[`../CREDITS.md`](../CREDITS.md): its owner, their website, its licence
and anything changed. If the project made it, including with AI
assistance, it is GPL-3.0-or-later. If someone else made it, it keeps
their licence, with the licence text beside it as `LICENSE-<name>.txt`,
or, when it is under no licence, is shipped on credit alone and removed
if its owner asks. **Art behind the start or menu screens is credited to
its artist by name, with their website.** Find out who made it before
committing it.

Your own backdrops in `<data dir>` are yours and are not covered by any
of this.
