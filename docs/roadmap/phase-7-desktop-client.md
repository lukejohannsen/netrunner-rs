# Phase 7 — A graphical desktop client

**The only client is a terminal.** Every way of playing exists — a rated
game against a rung, the lesson tracks, a deck builder, network play —
and all of it is reached through ratatui. The goal is a desktop client
in the shape of MTG Arena: animated card movement, graphics, sound, a
Netrunner-themed play area; card images downloaded from NetrunnerDB on
request with a text-only card face in the printed card's layout as the
fallback; the Runner's red and the Corp's blue card backs; menus for
every way to start a game; a profile; deck building; a browser over
every card. Built in phases a person can play with and redirect, on a
base explicit enough that any model can extend it.

**Decisions taken up front, with the alternative rejected:**

- **Bevy 0.19**, not egui/eframe, Iced or Tauri. Only a game engine gives
  tweened movement, particles and audio without a second language; egui
  and Iced are tool toolkits with animation bolted on, and Tauri adds a
  JavaScript toolchain and an IPC boundary to a Rust repo. Built with the
  feature subset `ui, audio, png, jpeg` — no 3D — and `bevy_ui` only for
  the board. `bevy_feathers`/BSN are marked unstable and `bevy_tweening`
  is a version lock-in for thirty lines, so neither is used.
  *Two corrections, both found while costing the third list: ICE is not
  "a rotated `UiTransform`" as this bullet first said — a rotated node is
  laid out as its unrotated box and would overlap its neighbours, which
  is exactly why ICE are horizontal tiles (`screens/game.rs`). And the
  feature subset is not as narrow as "no 3D" suggests: `ui` already
  pulls the entire render stack, `bevy_light` and `bevy_material`
  included. See the third list's preamble for what 3D would actually
  cost, measured.*
- **A toolkit-agnostic core, `netrunner_client`, lifted out of the
  terminal client** rather than copied into the desktop. The settings
  file has no `deny_unknown_fields` (a hand-edited file with a stray key
  should load), so a client that loads it into a struct missing a field
  drops that field on its next save; two structs would erase each
  other's preferences and one cannot. The same crate owns the deck store
  and the rating book, so a deck built in either client plays in both
  and a rating earned in either moves in both.
- **A local match runs on its own thread** behind a channel-shaped
  `MatchHandle` (§3). `Session::step()` blocks while a `Seat::Agent`
  searches — seconds at a high rung — and a resource pumped from a
  system would freeze the frame. It also makes local, lesson and remote
  look identical to the game screen.
- **The play area before the deck builder.** The board, the match handle
  and the view-diff are the novel and risky part and the reason for the
  client; the deck screens port a state machine the terminal client has
  already got right.
- **Three-tier assets everywhere**: procedural or synthesized always
  works, an optional file under `assets/` is prettier, a user override
  under `<data dir>/netrunner/assets/` wins. Card backs are drawn in-app
  with the official Null Signal Games PNGs as a documented drop-in;
  card fronts are downloaded into the cache directory and never
  committed. Nothing whose license is not the project's to give goes in
  the tree.
- **A separate CI job** for the desktop crate, on its own cache, with
  the engine jobs excluding it — so Bevy's several hundred crates never
  slow the engine's signal or evict its cache.

Seven sections, in order: §1 foundations, §2 card faces and the browser,
§3 the play area, §4 feel, §5 the deck builder, §6 lessons and replay, §7
online. *This said "seven PRs" until the third list, and that was only
ever true of §1–§3. §4 is one PR per lettered item, driven by what the
person asks for after playing — three lists and eighteen letters so far —
so a section is a heading, not a branch. The numbering still holds: §5–§7
keep their addresses however long §4's alphabet runs.*

---

**Where it stands (29 September 2026).** §1–§3 and §5–§7 are built; §4 has sixty-four lettered
entries built, one PR each, driven by what the person asked for after playing. Every closed entry is
one line below and whole in [the archive](archive/phase-7-desktop-client.md); the conventions they
settled are AGENTS.md §5, which is where to read them. **Open: §4's movement, owed since
§3 (its sound is done); §8's remaining items; §9's search half, not started; and the owed list.**

## Owed

- **Movement** (§3 → §4): the transitions are highlights, not tweened movement, and the tween module exists only in the plan. **Sound is built** (`feat/desktop-sound-and-tables`, 1 October 2026): `audio` plays the board's sounds off its `Transition`s (`models::sound::cues`), the interface's (toggle, switch, back, a deck's add) and the music — the menus' theme and a game's shuffled tracks — with six AI-assisted tracks (the sounds themselves were Kenney's CC0 recordings until §13 replaced them with a set synthesized here); five AI-assisted tables ship with it (a sixth was tried on the board and dropped). On Linux it needs ALSA to reach the speakers — `pipewire-alsa` on a PipeWire desktop, without which cpal falls back to a silent HDMI port (`assets/sfx/README.md`).
- **§3's other gaps**: a `ChooseCards` prompt's positions are reachable only from the panel; the log keeps 80 lines.
- **§4m**: a 3D or perspective board — deferred, not refused; revisit when a run should feel dramatic.
- **§4n, §4r — art still on the drawn tier**: panels' own art, the remaining icons, player bars, the fanned hand, the hovered card, the right-hand side; and `"tint": "state"` is promised by the skin guide and not wired (§4q delivered the contact shadows; §4t tried and removed a central's mark).
- **§4p**: the control bar's buttons carry no affordance glow.
- **§4y**: per-identity board styles, deferred at the person's choice.
- **§6c**: a lesson hint still echoes its prose. **§6d**: Cleaver's menu entries printed raw symbols ("1[credit]") — check against §4bg before fixing.
- **§7**: a resume over an iroh ticket is tested only over TCP.
- **§8 item 4**: the take-back's server half ("the server can take (b) later"). **§8 item 5**: replay notes — a file beside the record, and an editor.
- **§9**: decklists searched from NetrunnerDB, not started. The download half — a published decklist by its link — is done (§10 Stage 6, 1 October 2026); the search half stays here.
- **§10**: the deck builder by format, decks as files and from NetrunnerDB, art per printing. Planned 30 September 2026 and built on NSG pool Stage 0d; Stages 3–6 are done (1 October 2026). What the plan left for later: the person's own art (`Art::Custom`, room made in Stage 3), and the terminal client's share of Stages 5–6 (its builder still imports from a pasted list).

## Closed — one line each

- **§1** — Foundations (`feat/desktop-foundations`, 14 September 2026).
- **§2** — Card faces and the browser (`feat/desktop-card-faces-and-browser`, 14 September 2026).
- **§3** — The play area (`feat/desktop-play-area`, 15 September 2026).
- **§4** — Feel (15–25 September 2026).
  - **§4a** — Cards and zones as the way to act, a control bar, and a gear menu (`feat/desktop-cards-zones-and-options`, 15 September 2026).
  - **§4b** — Server columns as tile stacks, and a run as a paced trail (`feat/server-tiles-and-run-trail`, 15 September 2026).
  - **§4c** — A card-selection prompt names its cards (`feat/selection-names-its-cards`, 15 September 2026).
  - **§4d** — Where a card a card installs may go (`feat/where-to-install`, 15 September 2026).
  - **§4e** — A lone pass is taken without a click, in both clients (`feat/lone-pass-taken-without-a-click`, 17 September 2026).
  - **§4f** — A HUD: each side's numbers in fixed places (`feat/desktop-hud`, 17 September 2026).
  - **§4g** — A click opens a card's actions; a secondary click reads it (`feat/left-click-actions-right-click-reads`, 17 September 2026).
  - **§4h** — Keys for the buttons, and a list of them (`feat/board-keyboard-shortcuts`, 17 September 2026).
  - **§4i** — The servers keep one order from both chairs, and the Corp HUD drops HQ (`feat/one-server-order-both-chairs`, 17 September 2026).
  - **§4j** — A phase bar: where the turn is, and where a run is (`feat/desktop-phase-bar`, 17 September 2026).
  - **§4k** — A hand card is dragged into place (`feat/drag-to-reorder-the-hand`, 17 September 2026).
  - **§4l** — A card dragged onto a lit place is played there (`feat/drag-a-card-to-play-it`, 17 September 2026).
  - **§4m** — A table under the board, and the depth is painted on it (`feat/desktop-table`, 17 September 2026).
  - **§4n** — The board's furniture is drawn instead of outlined, and a skin never changes a size (`feat/desktop-skin`, 17 September 2026).
  - **§4o** — An access shows the card, not its name, in both clients (`feat/access-shows-the-card`, 17 September 2026).
  - **§4p** — The board says what can act, and in which of two moods (`feat/playable-cards-glow`, 18 September 2026).
  - **§4q** — The cards sit on the table: contact shadows, and the one BoxShadow they share (`feat/desktop-contact-shadows`, 18 September 2026).
  - **§4r** — The sheet is the card: no name over it, no Close under it, and a panel a skin can paint (`feat/desktop-sheet-frame`, 18 September 2026).
  - **§4s** — The table has a near side and a far side, and its middle holds still (`feat/desktop-board-depth-layout`, 18 September 2026).
  - **§4t** — Every server has a picture, drawn until somebody draws one, and one list of every asset (`feat/desktop-server-plates`, 18 September 2026).
  - **§4u** — A tile shows what kind of card it is, and a counter shows its kind (`feat/desktop-tile-art`, 18 September 2026).
  - **§4v** — Every asset is its own licensed work, with a register that cannot drift (`chore/asset-credits`, 18 September 2026).
  - **§4w** — A card scan is 750 pixels wide wherever Null Signal Games printed it, and drawn at the width it covers (`feat/hires-card-scans`, 18 September 2026).
  - **§4x** — A choice between cards shows the cards (`feat/choices-show-their-cards`, 18 September 2026).
  - **§4y** — Every place has an asset slot before its art exists, and the drawn tier is only the fallback (`feat/desktop-asset-slots`, 18 September 2026).
  - **§4z** — The board runs the window's full height; the status, the phase and a run's Runner are the right column's (`feat/desktop-board-edge-to-edge`, 18 September 2026).
  - **§4aa** — The rig is three rows, programs next to the ICE, and the heap opens on a click (`feat/rig-rows-and-open-heap`, 19 September 2026).
  - **§4ab** — A piece of ICE is broken with one press, at the price on the button (`feat/auto-pump-and-break`, 19 September 2026).
  - **§4ac** — Broken and fired subroutines are marked where the encounter is decided (`feat/subroutines-on-the-ice`, 19 September 2026).
  - **§4ad** — A menu and a pop-up never put an option off the window (`fix/menu-stays-on-the-window`, 20 September 2026).
  - **§4ae** — A card says what it will ask before it is played (`feat/play-preview`, 20 September 2026).
  - **§4af** — A move can be taken back: free while it has taught nothing, an undo that ends the rating after (`feat/take-back-and-undo-click`, 20 September 2026).
  - **§4ag** — A bug report is the game, as a file that replays (`feat/bug-report-replays`, 22 September 2026).
  - **§4ah** — A saved game steps on the board, from either chair (`feat/desktop-replay-viewer`, 22 September 2026).
  - **§4ai** — One Continue, and it says to what (`feat/one-continue-names-the-next-step`, 22 September 2026).
  - **§4aj** — The ICE being encountered takes the run panel: its art, its type line, its strength and every subroutine (`feat/encounter-panel`, 22 September 2026).
  - **§4ak** — The phase panel sits at the foot of the right column, so the run panel above it holds still (22 September 2026).
  - **§4al** — Identical rig cards are one card with a count, and split the moment they differ (22 September 2026).
  - **§4am** — Damage names what it discarded, the HUD says "Core damage", and a trap's rez is not a move (22 September 2026).
  - **§4an** — A Trojan sits on its ice, and its place in the program row is a ghost (23 September 2026).
  - **§4ao** — The timing of the turn and the run, as the rules chart it, with the step in play lit (23 September 2026).
  - **§4ap** — A card's "you may" can be answered Always or Never, and the answer is kept (23 September 2026).
  - **§4aq** — The Corp can pass the rest of a run with one press (24 September 2026).
  - **§4ar** — Archives and the Heap show cards large enough to read (24 September 2026).
  - **§4as** — A card opened to read sits at the right, and the field stays in view (24 September 2026).
  - **§4at** — A card's name in the match log opens the card (24 September 2026).
  - **§4au** — The mulligan is a start-of-game box, and the end of the match has a table (24 September 2026).
  - **§4av** — The glows can be seen by a colour-blind player (24 September 2026).
  - **§4aw** — Menus are glass and buttons are pills, and the new-game form is laid out as choices (24 September 2026).
  - **§4ax** — The client wears the Android cover's colours, and a drop-down's list stays on the window (24 September 2026).
  - **§4ay** — The client ships its art: an unlicensed work is committed with credit and removed on request (24 September 2026).
  - **§4az** — Both printings' card backs ship, and Settings picks one (24 September 2026).
  - **§4ba** — The deck builder, Play vs Computer and Play Online have backdrops, the rest a steel-and-circuit picture, and the menu's title a shadow (24 September 2026).
  - **§4bb** — The mulligan box draws its cards at a reading size, and a glowing button's label shows (24 September 2026).
  - **§4bc** — The run lane is gone, and the actions menu opens over the box that was clicked (24 September 2026).
  - **§4bd** — A hand card rises out of the row and follows the pointer, and a drag redraws the board (`fix/hand-lift-and-drag`, 25 September 2026).
  - **§4be** — A Runner install is dropped on the rig, and an event or an operation on the table (`feat/drop-on-rig-and-table`, 25 September 2026).
  - **§4bf** — A server is named as a person names it, on every button and in every log line (`fix/server-names`, 25 September 2026).
  - **§4bg** — A printed symbol on a button is the symbol, not its token (`fix/label-symbols`, 25 September 2026).
  - **§4bh** — NetrunnerDB's icon font is shipped, not fetched (`feat/ship-icon-font`, 25 September 2026).
  - **§4bi** — Each side has an avatar on a bar of its numbers, lit on its turn (`feat/desktop-avatar-bar`, 25 September 2026).
  - **§4bj** — A server is a few strips and a stack sheet, its strips framed by kind, and the rig takes the height (`feat/server-stacks-and-tile-frames`, 25 September 2026).
  - **§4bk** — Every name on the table is a nameplate, and the buildings are gone (`feat/server-nameplates`, 25 September 2026).
  - **§4bl** — A server shows up to ten strips before its stack sheet, and the servers take the spare height before the rig (`feat/server-slots-take-spare`, 25 September 2026).
- **§5** — The deck builder (`feat/desktop-deck-builder`, 24 September 2026).
  - **§5a** — A card in the deck builder can be read (`feat/deck-builder-readable-cards`, 24 September 2026).
  - **§5b** — A card is read by a right-click, centred; the hover preview is gone (`fix/deck-builder-read-centred`, 24 September 2026).
  - **§5c** — Decks are sorted and filtered, and the pool is narrowed by set, format and playability (`feat/deck-sort-and-filter`, 24 September 2026).
- **§6** — Lessons (24 September 2026).
  - **§6a** — A lesson is played on the board, with a coach in the right column (`feat/desktop-lessons`, 24 September 2026).
  - **§6b** — Each track ends in its starter games, and a finished lesson is ticked (`feat/desktop-starter-games`, 24 September 2026).
  - **§6c** — A lesson's words name the move, not a button (`fix/lesson-words`, 25 September 2026).
  - **§6d** — The coach says where on the board each move is made (`feat/lesson-where`, 25 September 2026).
- **§7** — Online (25 September 2026).
  - **§7a** — The spectator's board looked at, and the relay in Settings (26 September 2026).
- **§12** — The first launch asks, once, whether to download the card images, and a no is told where the download lives (`feat/first-launch-card-images`, 4 October 2026).
- **§13** — The sounds are a cyberpunk set synthesized in this project, in place of the recorded cards and chips, and a run, a panel, a rez, an advance, damage, a tag, an agenda, the turn and the end of the match are heard (`feat/cyberpunk-sound-theme`, 4 October 2026).

## 8. Borrowed from jinteki — OPEN (19 September 2026)

From [`docs/archive/jinteki-comparison.md`](../archive/jinteki-comparison.md) §5, in the
order they would matter to a person playing.

**This list was numbered §5 when it was written on 19 September 2026, and
that was a collision.** §5, §6 and §7 were reserved for the deck builder,
lessons and replay, and online in this file's own header, which says in
as many words that they keep their addresses however long §4's alphabet
runs — so "Phase 7 §5" resolved to two different things for a day.
Renumbered to §8 on 19 September 2026, the next free address. **§4aa and
§4ab, and the commit messages and PR bodies of #81 and #82, say "item 1
of §5" and "item 2 of §5" and mean items 1 and 2 of this list**; those
are published and are not being rewritten. Nothing else ever cited it. Each follows the §5 client
rules in `AGENTS.md`: every one is a door to legal actions that already
exist, never a new action.

1. ~~**The rig in three rows**, programs nearest the ICE.~~ Done in §4aa.
2. ~~**Auto-pump and break**, with the price on the button.~~ Done in
   §4ab.
3. ~~**Broken and fired subroutines marked on the encountered ICE.**~~
   Done in §4ac.
4. **Look first, take it back, undo a click** — rewritten by the second
   pass (20 September 2026, doc §6.5), which found the first version wrong:
   jinteki's undo is online and unilateral, and an undo that teaches
   nothing need not be unrated. Three parts, from a report from play (Red
   Team spends its click before showing which servers are left):
   **(a)** a card's entry says what it will ask before it is played, off a
   determinized sample as `board::breaks` does; **(b)** a *free* take-back
   while the person is still on the prompt their own action opened and
   nothing hidden was revealed, `rng_step` has not moved and the other seat
   has not acted — a restore in `netrunner_session`, not a `PlayerAction`,
   so `ActionSpace` stays 1646, the game stays rated, and §4d's declined
   install back-out comes with it; **(c)** undo a click against a bot, past
   that line, which makes the game unrated the first time it is used
   (**corrected 20 September 2026**: it costs nothing — a game against a
   bot is casual, §4af's correction and Phase 3 §2).
   Local games now; the rule sits in the session so the server can take
   (b) later.
5. ~~**A replay viewer over `MatchHistory`**~~ — done in §4ah; **notes
   are still owed**.
6. ~~**The Corp's run auto-pass toggle.**~~ Done in §4aq.
7. ~~**Space as the one "continue" key.**~~ Done in §4ai.
8. ~~**Ghost Trojans in the program row.**~~ Done in §4an.
9. ~~**Identical rig cards stacked** with a count.~~ Done in §4al.
10. ~~**Run and turn timing diagrams.**~~ Done in §4ao.
11. ~~**A spectator seat.**~~ Dropped (24 September 2026, at the
    person's request): no longer on the list. Phase 4's server-side
    spectators are unaffected.

Added by the second pass (20 September 2026, doc §6.6), numbered on from
eleven so the addresses above do not move:

12. ~~**One Continue button that names what is next** ("Continue to Approach
    ice", "Breach server").~~ Done in §4ai.
13. ~~**The encounter panel always on during an encounter** — name, subtypes,
    live strength, every subroutine; §4ac's marks are the first half.~~
    Done in §4aj.
14. ~~**Per-card always / never / ask for an optional trigger.**~~ Done in
    §4ap.
15. ~~**A report-a-bug bundle**: the seed and the action record, which replay
    exactly. The answer to the need behind jinteki's state-editing
    commands.~~ Done in §4ag.
16. ~~**An end-of-game table and a start-of-game box.**~~ Done in §4au.
17. ~~**Card names in the log open the card.**~~ Done in §4at.
18. ~~**Check the affordance and transition colours against a
    colour-blind-safe palette.**~~ Done in §4av.
19. ~~**Open decklists as a game option; an offered hand sort** beside the
    person's own order.~~ Dropped (24 September 2026, at the person's
    request): not a feature they want.

Added by the person (24 September 2026), numbered on so the addresses
above do not move. Neither is borrowed from jinteki, but both belong to
the same kind of work, so they go on this list:

20. ~~**Archives and the Heap show cards large enough to read.**~~
    Done in §4ar. Asked
    for because the cards in both piles' sheets are too small to see.
    Today (`zone_sheet` in `screens/game.rs`) a pile's sheet draws each
    card at `FaceSize::Thumb` in a wrapping row inside a scroll area
    capped at 460 px. Each face is already a `Click::Inspect` that
    opens the card over the sheet. What is wanted:
    - the faces a size up from `Thumb`;
    - the box scrolls, so a large pile still fits the window at the
      new size (the cap is set to fit the larger faces, and the sheet
      is an overlay, so the board still never scrolls);
    - a click on a face opens that card large, as a click on any other
      card does.

    Check first whether that last click reaches the card in play today.
    If it does, the fix is only the size and the cap; if it does not,
    that is a bug to fix first.
21. ~~**A card opened only to read sits at the right-hand side of the
    window, not over the middle of the board.**~~ Done in §4as. Asked for with play
    between two people in mind: while one chair reads a card, the
    other may still be playing, and a panel over the middle hides the
    field the reader is watching. This covers every *reading* surface:
    the sheet a secondary click opens (the card, an install beside its
    state, a zone's contents), the card opened over a zone sheet, and
    the card browser's inspector if it overlaps the same way.
    **The decision pop-up stays centred.** A choice pauses the game on
    both sides for the moment it takes, so putting it in the middle is
    the point. The same split is already drawn in the code:
    `Game::dismissed_by_a_click_away` names the reading surfaces a
    click that misses closes, and the pop-up is not one of them. So
    this is a placement change for that one group, in
    `models::layout`. The right column (the status, the prompt and the
    run panel) is where the reader's eye already goes, so check what it
    covers there at the smallest window the board supports.

Not borrowed, with the reasons in the doc: slash commands that edit state,
and diffs on the wire.

## 9. Decklists searched and downloaded from NetrunnerDB — the download half DONE (1 October 2026), the search half not started

The person's idea, recorded rather than built. A person should be able to
search NetrunnerDB's published decklists (by identity, card, format or
name) from the deck builder in either client, and download one as a saved
deck. **The download half is built** (§10 Stage 6): a published decklist
comes by its link or uuid, fetched over v3 into the same save-and-open
path a file takes. The search half stays here, and the terminal client's
share of both.

- **NetrunnerDB has the API.** v2 has `/api/2.0/public/decklist/{id}` and
  `/decklists/by_date/{date}`, and the v3 API
  (`api.netrunnerdb.com/api/v3/public/decklists`) takes filters. Which of
  them searches by card or identity is read off the API when this is
  taken. Nothing here has been tried yet.
- **The fetch belongs in `netrunner_card_sync`**, the only crate that does
  network I/O for card data, and a downloaded list goes through the same
  `Imported` path a pasted one does. A list is then judged the way a pasted
  list is: legal in a format, or holding cards the engine does not play yet
  (`Standing::Unplayable`).
- **The same fetch serves the NSG pool plan**
  ([nsg-card-pool.md](nsg-card-pool.md)), where current Standard tournament
  lists join the `Sample` pool once Standard is complete. Whichever lands
  first builds the fetch for both.
- **Read off the API on 30 September 2026.** v3 is the one to use, by the
  person's decision ("everything v3").
  - `GET /api/v3/public/decklists/{uuid}` returns `name`, `notes` (HTML),
    `identity_card_id`, `side_id`, `faction_id`, and `card_slots`, keyed by
    card slug.
  - `filter[card_id]=<slug>` works: 1,205 decks name Hoshiko, and
    `sort=-created_at` puts the newest first.
  - `filter[search]` and `filter[format_id]` returned 500 that day.
  - The download half is §10 Stage 6, done; the search half stays here,
    on `filter[card_id]` newest first until the other filters answer.

## 10. The deck builder by format, decks as files and from NetrunnerDB, art per printing — DONE (1 October 2026)

**What the person asked for (30 September 2026):**
- **Playable cards only.** "'Playable' seems like the only reasonable
  option to build a deck." The Show drop-down goes.
- **"Legal in" is the leftmost filter, and it narrows Set**, so Startup
  never offers the Core Set.
- **Every set's icon**, in the builder and on the Cards screen.
- **Sets newest to oldest by release date.**
- **No clipboard import or export.** Import from a file instead, so people
  can share decks. The native OS dialog was chosen over an in-app browser,
  and `.txt` (NetrunnerDB's text shape) for export.
- **Import a NetrunnerDB deck** by link.
- **Choose a card's art among its printings**, with the person's own art
  later as a separate feature that this must not build into a corner.

The Cards screen gets the same format-first order.

All of it stands on the v3 catalog (NSG pool Stage 0d, done 30 September
2026), which gives sets their `date_release` and the cycle the icon font
names its marks by (`cards::catalog::CardSet::cycle`). The font's glyph
names (`netrunner.svg`) are v3's cycle ids, so every embedded set has a
mark. Stage 0d also left the Cards screen listing one entry per card
with its printings in the inspector.

**Stages, one PR each, all done; each whole in the archive under its heading:**
- **Stage 3 — art per printing:** done. A card's printings are a strip of
  pictures under the browser's inspector and beside a card opened to
  read, each the button that draws the card with that art everywhere;
  the choice is kept in the settings file, never sent, and asked through
  `netrunner_client::art::picture_for` (`feat/art-per-printing`, 30
  September 2026).
- **Stage 4 — the format-first builder:** done. The pool is playable
  cards only; "Legal in" is the first filter and decides the sets the Set
  filter offers (`deck_builder::sets`), each under its cycle's mark
  (`card_text::set_icon`, every embedded set), newest first; the Cards
  screen reads in the same order (`feat/deck-builder-format-first`, 30
  September 2026).
- **Stage 5 — decks as files:** done. Import from file… and Export to
  file… through the native dialog (`files`: `rfd` over the XDG portal,
  no GTK, on a blocking thread of the tokio runtime), export as `.txt`
  offered in Downloads, the clipboard buttons gone and the clipboard
  left to the Online screen's tickets (`feat/deck-files`, 30 September
  2026).
- **Stage 6 — a NetrunnerDB deck by link:** done. Import from
  NetrunnerDB… takes a published decklist's link or uuid,
  `netrunner_card_sync::fetch_decklist` asks the v3 API on the tokio
  runtime, and `deck_builder::from_published` matches every card by its
  v3 id, keeping the author and the link as the deck's description;
  §9's download half (`feat/netrunnerdb-deck-import`, 1 October 2026).

**Left for later, by the plan:** the person's own art (`Art::Custom`,
room made in Stage 3); the terminal client's share of Stages 5–6.


## 11. An AI opponent on the person's own key — DONE (1 October 2026)

A person configures a language model — Anthropic, OpenAI, Gemini,
Ollama, or any server that speaks the OpenAI chat shape — on their own
key, in the GUI, and seats it in the bot's chair instead of the planner.
Asked for on 1 October 2026 ("BYOK"), with the settings file as TOML and
every cost lever the client can pull.

**The decisions, each with what was rejected:**

- **The settings file is TOML** (`settings.toml`), the whole file and not
  a section, because the person asked for TOML and two formats in one
  directory would have been the worse surprise. No migration: the
  application is unreleased, so the old JSON is ignored (the person's
  call, after a migration had been planned). The record, the known
  servers, decks and bug reports stay JSON — none is edited by hand.
- **A key is never in the settings file.** Each profile's key lives in
  `secrets.toml` beside `identity.key`, created owner-only through
  `identity::write_secret` and rewritten by temp-and-rename so it is
  never on disk with wider access; `ApiKey` has a redacting `Debug`.
  Rejected: keys in `settings.toml`, which is the file a person pastes
  into a bug report — the rule `identity` set for the signing key.
- **The agent lives in `netrunner_client::llm`, not `netrunner_bots`.**
  A model has to be shown the board in words, and the words are this
  crate's (`actions`, `prose`, `card_text`, `board`); a second vocabulary
  in the bots crate would drift from what a person reads. It implements
  the bots' `BotAgent`, which `Seat::Agent` takes from anywhere, so the
  session loop is untouched (the Session Rule).
- **One request per decision, never a transcript**, with a system prompt
  built once per game that holds the rules in this project's words and
  the bot's own deck's card texts, so a provider that caches a repeated
  prefix serves it at its cache rate and the per-decision message names
  own cards by title alone. The model answers with a number off a
  numbered menu of the legal actions (`progressive` → `selection::shown`
  → `ActionMap` labels); whatever it says, what is played is the menu's
  entry, so the session never sees what the model wrote.
- **Token thrift as a design goal** (the person's question, 1 October):
  a menu of one is taken unasked; `Asks::Clicks` hands priority windows
  and payment splits to the planner (most of a game's decisions, so
  roughly a third of the requests), with the Runner's own encounter
  window kept as the model's because the breaks are the decision; the
  answer is `{"action": n}` with "why" off unless asked; `effort` low by
  default and sent only when set (Anthropic's `output_config.effort`,
  OpenAI's `reasoning_effort`, Ollama's `think: false`); one retry then
  the planner; every reply's usage summed, shown on the end table as a
  count (never a price), and an optional per-game token budget after
  which the planner finishes the game with a notice. Rejected: a 1-hour
  cache TTL by default (writes at twice the price; a person's turn is
  usually under five minutes), the Batch API (someone is waiting), a
  cheaper model as a router (the dial does the job in-process).
- **When the model fails, the planner plays that one decision** and a
  line reaches the board (`MatchMessage::Notice`): a bad key, a server
  down, a refusal, an answer naming no action after one retry. Rejected:
  ending the game as a stall — a person who set a model up should get a
  game. The model is asked again at the next decision.
- **Three wire protocols**, pure request builders and reply readers
  tested on canned bodies: Anthropic Messages (`x-api-key`,
  `anthropic-version: 2023-06-01`, the system block marked
  `cache_control`, no `thinking` field since adaptive thinking is the
  current models' default), OpenAI chat completions (the shape Gemini,
  Mistral, Groq, LM Studio, llama.cpp, vLLM and Ollama's compatible
  endpoint speak; sent without a key when there is none, so a local
  server works and a hosted provider's 401 is the notice), and Ollama's
  own `/api/chat`. Gemini is a preset over Google's OpenAI-compatible
  endpoint with a Google key.
- **The transport blocks the thread that pumps the session**, because
  `BotAgent::select_action` is synchronous: `Handle::block_on` on the
  desktop's match thread, `block_in_place` on the terminal's
  `#[tokio::main]` thread, chosen by `Handle::try_current`. Every test
  answers from a `ScriptedTransport`; the one HTTP test talks to a
  loopback listener.
- **The record names the model** (`llm:<profile>`), and a game against
  one moves no rung: `record::level_of` reads only `bot:` ids, so the
  suggestion is untouched by construction. `RecordedBot` carries the
  model's name beside the rung behind it and lost `Copy` for it.
- **The desktop gets the full UI; the terminal a flag.** Settings → AI
  opponents is its own screen (`AppScreen::Opponents`, a form in
  `models::opponents` driven by intents and tested pure): profiles added
  from presets, protocol and preset pills, name, URL and model fields, a
  masked key field that shows a dot per character and is never read
  back, the timeout, the Asks dial with its trade in a line, Explain,
  the budget, and Test, which sends one trivial request on
  `TokioRuntime` (the `netrunnerdb::Decklists` oneshot pattern, scripted
  in tests). The new-game form gains an Opponent pane when a profile
  exists and hides the rung and style for a model, carrying the chair's
  suggested rung as the planner behind it; the board says "<name> is
  thinking…". The terminal takes `--corp-model <profile>` /
  `--runner-model <profile>` (`--corp llm:x` could not carry a name:
  `BotKind` is a `ValueEnum`), writes the notices into its log, and its
  screen waits while the model answers — accepted. `bench` and
  `--headless` refuse a model by name: a measurement is not a thing to
  spend a person's key on.

**Tests:** the settings' every-section TOML round trip; profile and
secrets round trips with the mode asserted; the system prompt the same
bytes across a game and every deck card once; no own-card text in the
message; the Runner's message without HQ's cards; an encounter's
subroutines and routes; `is_routine`; the selection prompt folded; each
protocol built and read; the agent's retry, fallback, lone action,
Clicks dial, budget and recent moves; a model that always answers "1"
through whole games in both chairs without an illegal action; a scripted
model through a whole `MatchHandle` game with its notices and its record;
the start menu's panes with and without a model; the terminal's flag,
bench and headless refusals; the desktop form's intents; the AI
opponents screen driven headless with the key saved owner-only and never
in the settings; the new-game form offering the model; a scripted model
on the board with its log lines and its end-table count.

**Left for later:** tool-use answers (a `tool_use` block instead of a
number), streaming, a terminal editor for profiles, a price readout, a
native Gemini protocol if the OpenAI-compatible endpoint ever falls
short, and the model's own memory of the opponent's moves beyond the
view (it sees the board, not the log).
