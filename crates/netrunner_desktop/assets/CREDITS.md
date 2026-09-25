# Asset credits — who owns every committed asset, and under what licence

**Every asset is licensed on its own.** The GPL-3.0-or-later in the
repository's `LICENSE` covers the project's source code. A picture, font
or sound committed under this directory is a separate work. Each one
carries its own licence and its own owner, recorded in the table below.
A test (`tests/credits.rs`) fails if a committed file has no row here,
or a row names a file that is gone. That is what keeps this list true as
assets come and go.

## The two origins

- **`project`**: made in this project, drawn by hand, painted by code, or
  generated with AI assistance for it. Owned by the netrunner-rs
  contributors and licensed **GPL-3.0-or-later**, like the code. The
  drawn defaults painted at runtime (card backs, the table ground, the
  server buildings, the tile patterns) are code and are already covered
  by the source licence, so they have no row.
- **`third-party`**: anything the project did not make. It stays its
  owner's, under the owner's licence, and **names who made it and where
  they publish**. The owner's licence text sits beside the file as
  `LICENSE-<name>.txt`. Any change made to it (a format conversion, a
  recolour) is listed in the row, because most licences require that.

## Art under no licence: shipped with credit, removed on request

**Some art is shipped with no licence at all.** An illustration marked
"all rights reserved" or the official card backs grant the project
nothing. They are committed anyway, the way jinteki.net ships the art on
its own table, because a client without them does not look like the game
(decided 24 September 2026). Such a row:

- has the licence **`All rights reserved`** and the licence text **`—`**,
  because there is no licence to put beside the file;
- still **names the owner and their website**, and for an illustration
  names the artist, so a person looking at it can find whose it is;
- is **removed when the owner asks**: delete the file and its row, and
  nothing else changes, because every asset has a drawn tier behind it.
  An owner can ask through the repository's issues.

## Adding an asset

1. **Find out who made it before it goes in.** Ask the person supplying
   it for the author, the website and the licence, and read the licence
   yourself. An unknown owner is a reason not to commit the file, not a
   blank in the table.
2. Put the file where it belongs, with the owner's licence text beside
   it if it is third-party.
3. Add its row. The `Website` column is the author's own site where they
   have one (`—` only when they have none). The `Changes` column is
   `none` or what was done to it.
4. **Art behind the start or menu screens**, or anywhere a person sees it
   as a picture rather than a glyph, is credited the same way: the artist
   by name, and their website.

**The client's About screen reads this file** (`src/credits.rs`), so a
row added here is credited in the client too, and the two tables below
the register credit what a player sees but the repository does not hold.

## Removing an asset

Delete the file and its row, and its licence text if nothing else
cites it. The client never depends on a committed asset: every one has
a drawn or synthesized tier that works without it (AGENTS.md §5).

## The register

| File | What | Owner | Website | Licence | Licence text | Origin | Changes |
|---|---|---|---|---|---|---|---|
| `fonts/NotoSans-Regular.ttf` | Noto Sans, the body font | The Noto Project Authors | https://notofonts.github.io | OFL-1.1 | `fonts/LICENSE-OFL.txt` | third-party | none |
| `fonts/NotoSansSymbols2-Regular.ttf` | Noto Sans Symbols 2, the printed-icon fallback | The Noto Project Authors | https://notofonts.github.io | OFL-1.1 | `fonts/LICENSE-OFL.txt` | third-party | none |
| `fonts/NetrunnerDB-Icons.ttf` | NetrunnerDB's icon font (`netrunner.ttf`, Null-Signal-Games/netrunnerdb `web/fonts/` at `ee095c6`): the printed symbols, the factions' and the sets' marks, which are Null Signal Games' and Fantasy Flight Games' | NetrunnerDB contributors (Cédric Bertolini, Jason Gessner) | https://netrunnerdb.com | MIT | `fonts/LICENSE-NetrunnerDB.txt` | third-party | renamed from `netrunner.ttf` |
| `backdrops/main-menu.jpg` | New Angeles in Neon/Smog, neon version: the main menu's backdrop | Kirsten Zirngibl | https://www.kirstenzirngibl.com/projects/RgWqA | All rights reserved | — | third-party | none |
| `backdrops/menu.jpg` | Purple circuitry between brushed-steel plates, generated with AI assistance (Google Gemini) for this project: the shared backdrop, behind every menu screen without a picture of its own | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `backdrops/online.jpg` | *The Root*, card art (Upstalk), © Fantasy Flight Games, the copy jinteki.net ships at mtgred/netrunner `resources/public/img/bg/TheRoot.jpg`: behind Play Online, Cards and Learn to Play | Alex Kim | https://www.artstation.com/alexkim | All rights reserved | — | third-party | none |
| `backdrops/splash.jpg` | New Angeles in Neon/Smog, smog version: the splash | Kirsten Zirngibl | https://www.kirstenzirngibl.com/projects/RgWqA | All rights reserved | — | third-party | none |
| `backdrops/decks.jpg` | *The Personal Touch*, card art (Revised Core Set), © Fantasy Flight Games: the deck list's and deck editor's backdrop | Aurore Folny | https://aurorefolny.com/albums/592963 | All rights reserved | — | third-party | none |
| `backdrops/new-game.jpg` | *Maya*, card art (Kala Ghoda), © Fantasy Flight Games, the artist's own copy from his ArtStation (`adamschumpert.artstation.com/projects/Wg5x2`): the Play vs Computer backdrop | Adam Schumpert | https://adamschumpert.artstation.com/projects/Wg5x2 | All rights reserved | — | third-party | none |
| `cards/backs/ffg/back-corp.png` | the Corp card back, Fantasy Flight Games' printing, the copy jinteki.net ships at mtgred/netrunner `resources/public/img/card-backs/corp/ffg.png` | Fantasy Flight Games | https://www.fantasyflightgames.com | All rights reserved | — | third-party | none |
| `cards/backs/ffg/back-runner.png` | the Runner card back, Fantasy Flight Games' printing, the copy jinteki.net ships at mtgred/netrunner `resources/public/img/card-backs/runner/ffg.png` | Fantasy Flight Games | https://www.fantasyflightgames.com | All rights reserved | — | third-party | none |
| `cards/backs/nsg/back-corp.png` | the Corp card back, Null Signal Games' printing, the copy jinteki.net ships at mtgred/netrunner `resources/public/img/card-backs/corp/nsg.png` | Null Signal Games | https://nullsignal.games | All rights reserved | — | third-party | none |
| `cards/backs/nsg/back-runner.png` | the Runner card back, Null Signal Games' printing, the copy jinteki.net ships at mtgred/netrunner `resources/public/img/card-backs/runner/nsg.png` | Null Signal Games | https://nullsignal.games | All rights reserved | — | third-party | none |
| `board/avatar.bar.active.png` | The avatar bar's wing, lit for the side whose turn it is, painted by code (`scripts/paint_avatar_bar.py`) after the shared menu backdrop's brushed steel and purple circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/avatar.bar.png` | The avatar bar's wing, grey for the side waiting, painted by code (`scripts/paint_avatar_bar.py`) after the shared menu backdrop's brushed steel and purple circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/avatar.frame.active.png` | The ring round the avatar, lit, painted by code (`scripts/paint_avatar_bar.py`) after the shared menu backdrop's brushed steel and purple circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/avatar.frame.png` | The ring round the avatar, grey, painted by code (`scripts/paint_avatar_bar.py`) after the shared menu backdrop's brushed steel and purple circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/ice.unrezzed.png` | A face-down ICE's strip frame, grey and unlit, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/ice.rezzed.png` | A rezzed ICE's strip frame (no type below), white traces, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/ice.rezzed.barrier.png` | A rezzed barrier's strip frame, gold, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/ice.rezzed.code-gate.png` | A rezzed code gate's strip frame, blue, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/ice.rezzed.sentry.png` | A rezzed sentry's strip frame, red, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/root.unrezzed.png` | A face-down root card's strip frame, grey and unlit, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/root.rezzed.png` | A rezzed root card's strip frame (no type below), white traces, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/root.rezzed.asset.png` | A rezzed asset's strip frame, bronze, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/root.rezzed.upgrade.png` | A rezzed upgrade's strip frame, teal, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/root.agenda.png` | An agenda's strip frame, green, painted by code (`scripts/paint_tiles.py`) in the avatar bar's brushed steel and circuits | netrunner-rs contributors | https://github.com/lukejohannsen/netrunner-rs | GPL-3.0-or-later | the repository's `LICENSE` | project | none |
| `board/counter.advancement.png` | advancement counter | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_ADVANCEMENT_COUNTER.svg` converted to a 128 × 128 PNG, recoloured `#eef1f8` |
| `board/counter.credit.png` | credit counter | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_CREDIT.svg` converted to a 128 × 128 PNG, recoloured `#eef1f8` |
| `board/counter.power.png` | power counter (NSG's generic counter) | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_GENERIC_COUNTER_BLUE.svg` converted to a 128 × 128 PNG |
| `board/counter.virus.png` | virus counter | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_VIRUS_COUNTER.svg` converted to a 128 × 128 PNG |
| `board/hud.agendas.png` | Agendas readout glyph | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_AGENDA.svg` converted to a 128 × 128 PNG, recoloured `#eef1f8` |
| `board/hud.bad-publicity.png` | Bad pub. readout glyph | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_BAD_PUBLICITY.svg` converted to a 128 × 128 PNG |
| `board/hud.clicks.png` | Clicks readout glyph | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_CLICK.svg` converted to a 128 × 128 PNG, recoloured `#eef1f8` |
| `board/hud.credits.png` | Credits readout glyph | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_CREDIT.svg` converted to a 128 × 128 PNG, recoloured `#eef1f8` |
| `board/hud.damage.png` | Damage readout glyph | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_CORE_DAMAGE.svg` converted to a 128 × 128 PNG |
| `board/hud.tags.png` | Tags readout glyph | Null Signal Games | https://nullsignal.games/about/nsg-visual-assets/ | CC BY-ND 4.0 | `board/LICENSE-NSG.txt` | third-party | `NSG_TAG.svg` converted to a 128 × 128 PNG |

## Fetched on the player's opt-in, never committed

These belong to others and are downloaded into the player's own cache
for their own screen, never shipped. They are listed so the About screen
credits everything a player sees, not only what is in the repository.

| What | Owner | Website | Terms | Fetched from |
|---|---|---|---|---|
| Card data: titles, rules and flavour text, illustrator credits | Null Signal Games and the card's illustrators, compiled by NetrunnerDB | https://netrunnerdb.com | theirs, not licensed by this project | NetrunnerDB's public API v2 |
| Card scans, the printed cards | Null Signal Games and each card's illustrator, named on the card | https://nullsignal.games | theirs, not licensed by this project | card-images.netrunnerdb.com (750 × 1050 where it has one, else 300 × 420) |

## Software

| What | Owner | Website | Terms | Fetched from |
|---|---|---|---|---|
| Bevy, the engine the client is drawn with | the Bevy contributors | https://bevy.org | MIT or Apache-2.0 | crates.io, at build time |
| Every other Rust crate in `Cargo.lock` | each crate's authors | https://crates.io | each its own licence, checked against `deny.toml` by cargo-deny | crates.io, at build time |

This client is a free, open-source fan implementation of Netrunner. It is not
affiliated with, authorized by, or endorsed by Null Signal Games,
Fantasy Flight Games, or Wizards of the Coast.
