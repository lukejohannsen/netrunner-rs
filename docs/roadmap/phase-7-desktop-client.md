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

**Where it stands (11 October 2026).** §1–§3 and §5–§7 are built; §4 has sixty-four lettered
entries built, one PR each, driven by what the person asked for after playing. Every closed entry is
one line below and whole in [the archive](archive/phase-7-desktop-client.md); the conventions they
settled are AGENTS.md §5, which is where to read them. **§4's movement, owed since §3, is
done (§4bm, 8 October 2026), and so are §8's replay notes (§4bn) and §9's search half in the desktop.
Open: §9's and §10's terminal share, the owed list, and the 11 October plan — §4bo, then §14–§16 with Phase 4 §8–§12.**

## Owed

- ~~**§4bo — a prompt says what it is for, and whose card asks**~~ Done (`feat/selection-says-what-and-whose`, 11 October 2026): the entry is below, in the closed list, and the bug it fixed — three Cultivate prompts all headed "Cultivate: choose 1 card", a deck listed deepest first with nothing numbered and "Choose none" offered, no prompt saying whose card asks — is in `docs/online-play-design-2026-10-11.md`.
- **§14, §15** (11 October 2026, below): the online dashboard with the profile and connections manager (the desktop's dashboard is built, 12 October 2026; the terminal's rows are its stage 5); the Local / P2P hub (the desktop's is built, 12 October 2026; the terminal's rows are its stage 2, and LAN discovery a later one). **§16 — the deck builder in three columns, the deck picker and the active decks — is done** (stages (a)–(c), 11 October 2026; the entry is in the archive). In the order §14 with Phase 4 §9–§10, then §15.
- **Movement** (§3 → §4): **built** (§4bm, `feat/card-movement`, 8 October 2026): every `CardMoved` transition is a flight over the board (`screens::flight`, `models::flight`), the card at its destination hidden until it lands; a rez is still a highlight rather than a flip. **Sound is built** (`feat/desktop-sound-and-tables`, 1 October 2026): `audio` plays the board's sounds off its `Transition`s (`models::sound::cues`), the interface's (toggle, switch, back, a deck's add) and the music — the menus' theme and a game's shuffled tracks — with six AI-assisted tracks (the sounds themselves were Kenney's CC0 recordings until §13 replaced them with a set synthesized here); five AI-assisted tables ship with it (a sixth was tried on the board and dropped). On Linux it needs ALSA to reach the speakers — `pipewire-alsa` on a PipeWire desktop, without which cpal falls back to a silent HDMI port (`assets/sfx/README.md`).
- **§3's other gaps**: ~~the log keeps 80 lines.~~ Done (`fix/desktop-log-keeps-the-match`, 9 October 2026): the log list remembers how many lines it drew and a fingerprint of them, so a view appends its new lines and only a log that no longer begins with the drawn ones (a take-back, a reconnect) is rebuilt; the whole match is readable, and the scroll follows the new lines only for a reader who was at the end, where every view used to snap them back. ~~A `ChooseCards` prompt's positions are reachable only from the panel.~~ Done in §4x (18 September 2026): the decision pop-up draws each candidate as its card, and the card is its own button; the line outlived the fix.
- **§4m**: a 3D or perspective board — deferred, not refused; revisit when a run should feel dramatic.
- **§4n, §4r — art still on the drawn tier**: panels' own art, the remaining icons, player bars, the fanned hand, the hovered card, the right-hand side (§4q delivered the contact shadows; §4t tried and removed a central's mark). ~~`"tint": "state"` is promised by the skin guide and not wired.~~ Done (`feat/skin-state-tint`, 9 October 2026): a slot's `"state"` is kept as a tint of its own kind and resolved in `Skin::dress` from the `Drawn` the call site hands over — its border where it has one (a tile's faction colour, a welcoming column's accent), else its fill — and a bare HUD readout now hands over its ink, so a white plate comes out red when a readout is alarmed.
- **§4p**: the control bar's buttons carry no affordance glow.
- **§4y**: per-identity board styles, deferred at the person's choice.
- ~~**§6c**: a lesson hint still echoes its prose.~~ Done (`fix/lesson-hint-echo`, 9 October 2026): the Corp's third lesson opened with the instruction its hint repeated, so the prose now teaches what a remote is and the hint names the move; the Runner's first lesson's draw step the same; and the gate also refuses a hint whose words are all in the prose's first sentence. ~~**§6d**: Cleaver's menu entries printed raw symbols ("1[credit]").~~ Done in §4bg (25 September 2026): `widgets::symbols::draw` splits every `Text` at its tokens, a menu entry's label among them; the line outlived the fix.
- **§7**: a resume over an iroh ticket is tested only over TCP.
- ~~**§8 item 4**: the take-back's server half ("the server can take (b) later").~~ Done in Phase 4 §5 stage (e) (8 October 2026): `ClientMessage::TakeBack`, the free kind in any game and an undo where the lobby does not rate, which `MatchHandle::take_back` sends to a host. ~~**§8 item 5**: replay notes — a file beside the record, and an editor.~~ Done in §4bn (8 October 2026).
- **§9**: decklists searched from NetrunnerDB — done in the desktop (8 October 2026), by a card's or an identity's name, newest first; the download half was §10 Stage 6 (1 October 2026). The terminal client's share of both stays here.
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
  - **§4bm** — Cards are seen moving: a drawn card flies from the deck to the hand, an install from the hand to its tile, a trash from its tile to the pile, as a copy over the board that hides the card it lands on until it has (`feat/card-movement`, 8 October 2026).
  - **§4bn** — A note is written on a replay's position, kept in a file beside the record, read on the rail in both clients, and stepped to from the bar (`feat/replay-notes`, 8 October 2026).
  - **§4bo** — A card-selection prompt says what the choice does and whose card asks — "Cultivate (your operation) asks: trash 1 of the top 5 cards of R&D", "… add 1 of the top 4 cards of R&D to HQ", "… arrange the top 3 cards of R&D" — and an arrangement lists the deck top first, numbers each pick (1 is the top) and offers "Leave them as they are" (`feat/selection-says-what-and-whose`, 11 October 2026). **The words are read off the parked decision's fields** — `source`, `destination`, `filter`, the bounds, `shuffle_after`, and `then` for a deck's bottom (Lethe) — in `netrunner_client::selection::purpose`, one function both clients call; a per-card text on `PromptChooseCards` was rejected, since it would be a second copy of what the fields say in 55 card files and under the clause gate, and a `then` the fields cannot word is read as prose under the verb ("Then: rez a card."). **Whose card asks** is `prose::asker`: the title first, since that is what the person recognises, then "(your identity)", "(the Corp's ice)" from the card's printed side against the viewer's chair, in every prompt's heading in both clients; a card the view masks stays "A card". **An arrangement** — cards off the top of a deck put back on it unshuffled, which the engine orders by pick, first pick on top — is told apart by its fields (Cultivate, Knowledge Seeker, Federal Fundraising, Precognition, Cataloguer, Indexing; Embedded Reporting's search back onto R&D is not one): its candidates run top first with each one's depth as its place ("2nd from the top of R&D", read off the candidates, never a count), every pick is numbered on the desktop's card and in the terminal's title, the toggles say "Put Enigma (top of R&D) 1st (on top)" and "Take … back out", the summary reads "Top first: Ice Wall, Hedge Fund — Enigma stays beneath", and the confirm with nothing chosen is "Leave them as they are". Any selection off the top of a deck reads top first, since that is the order the person looked at the cards in. `NETRUNNER_HOLD_SELECTION=arrange` holds the autoplay at the first arrangement for a screenshot. Tests: the purpose words over every shape in the pool, an arrangement's order, places, picks, labels and no folding of two copies (`selection`); the asker from both chairs and for a masked card (`prose`); Cultivate's three headings from both chairs (`action_map`); the pop-up's labels and title (`models::game`); the sweep over 24 seeds of every selection a real game parks admits the new label shapes. Left: a `Sequence` parked mid-way is re-queued without its `prompting_card` (`DeferredTrigger`), so a later prompt in it is credited to the acting card — Cultivate's three are top-level and unaffected; recorded in the entry's plan as a fix of its own.
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
  - **§7b** — The Server page: attached once, the lobbies browsed, one made, a chair and its decks chosen and a game found, the board leading back to it for the next (Phase 4 §7 stage 4c, 8 October 2026; the entry is there).
- **§12** — The first launch asks, once, whether to download the card images, and a no is told where the download lives (`feat/first-launch-card-images`, 4 October 2026).
- **§16** — The deck builder in three columns, the deck picker and the active decks: one picker over every screen with the identity's face and a badge per format, the chosen deck the active deck in the settings file, the desktop's builder in three columns, the terminal's builder on the shared editor with its list the picker's, and no test deck in any list a person sees (`feat/deck-picker-active-decks`, `feat/deck-builder-three-columns`, `feat/terminal-builder-shared-api`, 11 October 2026).
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
5. ~~**A replay viewer over `MatchHistory`**~~ — done in §4ah; ~~**notes
   are still owed**~~ — done in §4bn (8 October 2026).
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

## 9. Decklists searched and downloaded from NetrunnerDB — the download half DONE (1 October 2026), the search half DONE in the desktop (8 October 2026)

**The search half is built in the desktop (`feat/decklist-search`, 8
October 2026)**: Search NetrunnerDB… on the Decks screen takes a card's or
an identity's name, resolves it against the catalog
(`models::decks::search_for`: exact title, else the one it begins, else
the one it is in), asks `netrunner_card_sync::fetch_decklists` for the
newest lists that play it — `filter[card_id]`, or
`filter[identity_card_id]` for an identity, `sort=-created_at`, one page
of twenty, since `filter[search]` and `filter[format_id]` still answered
500 on 8 October — and lists the rows under the title searched, each a
button that saves and opens the list as a fetched one does
(`Intent::Fetched`), because a row carries its `card_slots`. The record
is in [the archive](archive/phase-7-desktop-client.md) under this
heading. **The terminal client's share of §9 and §10 stays here**, as
the §10 plan left it. What was recorded before it was built:

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

## 14. The online dashboard, and the profile and connections manager — OPEN (11 October 2026)

The person's notes of 11 October 2026 ([`docs/online-play-design-2026-10-11.md`](../online-play-design-2026-10-11.md)) draw Play Online as a dashboard: the **active decks** always in view (one Corp, one Runner — §16), three doors — **Ranked**, **Casual**, **Tournaments & Events** — and a profile tag; a door opens a form rather than a page — the format as pills, greyed where the active decks are not legal with the reason, and the role preference (Any, Prefer Corp, Prefer Runner) — then the queue state (what is being looked for, the time waited, Abort), then the board. Casual's form adds "Play a friend": make a friend queue, or join one by its code (Phase 4 §9). The tournament pages (Phase 4 §7 stage 6b) keep their place behind the third door, with events beside them (Phase 4 §11). A **server profile page** shows the titles and badges earned, the active title and badge, and Save (Phase 4 §10).

**The profile and connections manager** replaces the Join form's one address field: named server profiles (a name, the address, the relay), one marked Default that Play Online connects to on opening, Connect and Disconnect one at a time, and under each server what it holds of the person — the active title and badge, the rank per format — beside the local facts (the key file's path, the card cache, the record against the bots). `known_servers.json` (address → key, no name, no UI) becomes a row of this; `Settings::server` (the last address) goes.

**Decided (11 October 2026):** the dashboard supersedes the Server page's layout and keeps its design — the connection is a resource the board plays on and comes back to (`screens::online::Connected`, Phase 4 §7 stage 4c), a deck is asked for only when a game is looked for, the queue's word on rating is the lobby's. Built after Phase 4 §9 and §10, each half with its server half; the terminal client draws the same as rows.

**Built, stage 1 (12 October 2026, `feat/server-book`): one list of servers.** `netrunner_client::servers::ServerBook` in `servers.json` beside the key: a row a server — its name (its address until named), the address dialled, the key it proved on first contact, when it was last connected to — and one row the Default. It replaced `known_servers.json` (the keys by address, no name, no UI) and `Settings::server` (the last address) outright. A row is made the first time a connection to a server is made or its key met, and a host's ticket, one game's, is never kept. Join and the terminal's `--server` start from the Default, else the server connected to last. **Every change is read, change, write in one call** (`ServerBook::update`), because the connection driver writes a key from its own task while a screen edits a name, and a copy saved later would lose the other's change; the list is beside the key rather than in `settings.toml` for the same reason. **Not the relay**: the notes gave each server profile one, but the relay is what hosting asks n0 for and a connection to a server never uses it, so it stays the one setting it is. The terminal menu now carries its identity directory, so its tests write a scratch one rather than beside the developer's key. Next: the desktop's manager page and the server profile page (stage 2), then the terminal's rows (stage 3).

**Built, stage 2 (12 October 2026, `feat/connections-manager-desktop`): the desktop's manager and profile page.** Join a server opens **Servers** (`Page::Servers`, which Join became): a row a server — its name, Default and Connected tags, its address, what it keeps of the person (each met server asked on its own socket every time the page opens, `remote::profile` with no key made to look; the one attached answers on its own connection), and Connect (Disconnect for the one attached), Make default, Edit… and Remove; then **Add a server…** (a form checked against the list, so a name taken is said there; a new address forgets the old key), **On this machine** (the key's fingerprint and file, the card images' folder, the record against the bots in one line, `record::summary_line`), and **Connect once**, the old address field, which is how a host's address or ticket is joined. **One connection at a time**: a row's Connect lets the one attached go. **The Default is connected to on opening the screen afresh** (`AutoConnect`, one frame, only from Home with nothing attached), and a Default that does not answer leaves the person at Home with the reason. The edits are `servers::ServerEdit`, decided by the model and written by the screen through `ServerBook::update`, so the terminal's rows (stage 3) make the same four. **The server profile page** (`Page::Profile`, Your profile… on the Server page; not on a hosted server, which keeps nothing): the standing per format; the title and the badge worn, each a line with how many were earned and **Change…**, which opens a pop-up of what was earned — titles a list of rows, badges a grid of their glyphs, None first and the worn one lit, scrolling by the wheel and its bar, closed by Escape or a click away — where **a pick is worn at once** (`SetProfile`, the other kind left as it is worn); and the ladder of a format, the lobby's first, the person's own row marked. **A pill each was built first and replaced the same day at the person's word**: a year of events is a hundred titles, and a wall of pills is not a choice. **A badge is a glyph** (`widgets::badge`): `badges/<stem>.png` in the three tiers (the stem is the name the server grants, made a file's), else a ring in a colour the name picks with its initials inside; the slot and its guide (`assets/badges/README.md`) are in place before any art. Both headings name the server by its name in the list. Tests: the model's (the page asks each met server, a row connects one at a time and a failure comes back to the list, the Default toggled and connected to from Home alone, a checked add and edit, the pop-up opens only on something earned, a pick is worn at once and Escape closes the pop-up before the page, and a ladder is read), and a headless one against a daemon with a store: a client opened with a Default lands on its Server page, wears a title the store says was earned, picked from the pop-up, and finds its row connected. Screenshotted on a virtual compositor against a scratch daemon with forty titles, eighteen badges and four ladder rows seeded, and the titles' list scrolled by the wheel (`NETRUNNER_SCROLL`). `NETRUNNER_ONLINE=servers|profile|profile-titles|profile-badges` are the hooks (`join` still opens Servers). A glyph the font lacks draws as a box: "Default ✓" and "← you" became words.

**Built, stage 3 (12 October 2026, `feat/connections-manager-terminal`): the terminal's rows.** Join a server is the list of servers (`Mode::Servers`): a row a server — its name and tags, its address, what it keeps of the person, asked on its own socket as the desktop asks — then **Connect once** (the address form, which is how a host's address or ticket is joined) and **Add a server…**; Enter connects a row (the one attached is its Server page again), `d` makes it the Default or not, `e` edits it, `x` or Delete removes it, each a `servers::ServerEdit` written through `ServerBook::update` as the desktop writes them, and under the list the key and its file. Opening Play Online with a Default connects to it (`OnlineScreen::connect_default`). The Server page gains **Your profile here…** (not on a hosted server) and **Servers…**; the profile page is the standing, a Title row and a Badge row (a badge marked by its initials in brackets, `identity::badge_initials`, which the desktop's drawn glyph uses too) and the Ladder row, ← → stepping its format; Enter on Title or Badge opens the list of what was earned over the page, which scrolls with its cursor (↑ ↓, Page Up/Down, Home, End), and Enter wears the row at once. Disconnecting from a server of the person's leads back to the list rather than the address form. Tests: the list edited on its rows and written to the file (a name taken refused on the form), and against a daemon with a store, the Default dialled as the screen opens, a title worn from the list, the list drawn at 80 × 24 and 160 × 48, and the list of servers showing the Default connected. **§14's connections manager and profile page are done; the dashboard (Ranked, Casual, Tournaments & Events) waits for Phase 4 §9.**

**Built, stage 4 (12 October 2026, `feat/online-dashboard`): the desktop's dashboard.** Phase 4 §9's desktop half had bolted the queues onto the old Server page — ten Ranked/Casual pills in a list of "lobbies", a Corp/Runner/Either row, a preference row, a bot row — and the person said it followed neither the wireframes nor the path ("I don't think the main entry are 'lobbies'"). **Decided (12 October 2026):** the main menu stays, and **Play Online → the Server Dashboard** (`Page::Dashboard`), not the app's front page: the dashboard is about one server — the profile tag, the rank, the titles and the queues are that server's — and offline play stays free of the network. The screen connects to the Default and opens on it (with no Default, on the servers page); **the active decks** (Phase 7 §16) at the top, each with its identity's face, its name and a badge per format it is legal in, and Change… to the picker (`Purpose::Active`); **three doors** as tiles (`widgets::tile_button`) — Ranked ladder, Casual / unranked, Tournaments & events — Ranked dim with the reason where the server rates nothing; and the column of ways elsewhere: the deck builder, the cards, the server profile, watching, hosting. **A door opens the queue form over the dashboard**, centred on a wash as a choice is: the format as pills, **greyed with the reason beside them when an active deck is not legal there** (`online::queue_problem`, read off the deck's standing per format, so a format is never sent that the server would refuse — the server's own check is the backstop for a deck edited after the check); the role soft filter, Prefer Corp / **Any / Balanced (Recommended)** / Prefer Runner; under Casual, "Play with friends" — make a friend lobby, or join one by its code. **Find match** joins that queue and, once the server says the person is in it, looks for a game with **both decks in Either chair** and the side preferred, so the queue never deadlocks on two Corps; the chair as a choice went. **The search** replaces the form — "Jacking into the mainnet…", a pulse (`widgets::pulse_image`, turned by a `UiTransform` with the clock so a redraw does not set it back), what is looked for and in what format, the place in the queue, the time waited, redrawn every second — until the board, or Abort, which stops the search and leaves the queue: the dashboard is not a queue, so the server's own are left after a search and after a game, and a refusal of what Find match sent reopens the form with the reason. **A friend lobby has a page of its own** (`Page::FriendLobby`): its name and code with Copy, who is there, the preference, a bot where the maker let bots in, Find a game among them, Leave. **Bots sit only in a friend lobby that allows one** (narrowed from §9's "Casual" on the same day: the person's path names no bot, and "requested in a Lobby" read as the friend lobby) — the server's own queues list `bots: []` and refuse a bot seek there, the terminal's `--bot` flag went, and every test that seated a bot in the Casual queue makes a friend lobby first. Back leaves the screen and the connection with it (the Default is dialled again next time); Disconnect, or a host's Stop hosting, lets it go and stays at the servers. The terminal's Server page is unchanged by this stage and is stage 5. Tests: the model's (the Default to the dashboard and no Default to the servers; a greyed format with its reason, refused with it when pressed; Find match as a join then a seek; a missing deck greying every format; Ranked shut; a refusal reopening the form; Escape closing the form, then aborting, then leaving; a friend lobby joined by code, made from its page, offering a bot only where it lets one in; the host's dashboard) and the headless ones against real servers (the host finds a match from the Casual door and the guest is seated against it; a guest lands on the dashboard and makes a friend lobby; the tournaments from the dashboard; the Default's profile tag). Screenshotted on a virtual compositor against a scratch daemon with a store, at 1600×1000 and 1280×720: the dashboard, both forms, the search, a friend lobby (`NETRUNNER_ONLINE=dashboard|queue|queue-ranked|searching|make-lobby|friend-lobby`).

## 15. The Local / P2P hub — stage 1 built (12 October 2026), the terminal's rows open

From the same notes: "Local / P2P Play" as a hub with two cards, **Host a session** and **Join a session**, and under them **saved peers and recent connections** with Quick Connect. Hosting: the decks (via the picker, §16), the protocol — LAN only, direct address and port, or an iroh ticket — then the waiting room: the ticket with Copy and the direct address, the participants with a ready state, Start when both are ready. Joining: the decks, one target field with the protocol detected (an address or a ticket), Connect.

**Decided (11 October 2026):** the hub stands on what §7 and Phase 4 §6 built — `hosting::Reach`, the dual-stack socket, the router asked, `peer`'s ticket — and adds the saved peers (a file beside `known_servers.json`), the ready state and the detected protocol. "LAN only, zero-config broadcast" means discovery on the local network (mDNS), which nothing has today (`Reach::Network` needs the address typed) and is a later stage of this entry. Built after §16.

**Built, stage 1 (12 October 2026, `feat/p2p-hub`): the desktop's hub, host, join and room, over the person's four wireframes, the shape shown first.** Before it, hosting was a form off the dashboard's column that landed the host on their own server's dashboard, and a guest who pasted the address or ticket on the servers page landed on the same dashboard — two friends who had already agreed to play then both went through Casual → Find match. **Decided (12 October 2026):** **Local / P2P Play is a main-menu entry of its own** (`Entry::Peer`, the Play Online screen opened on `Page::Hub` by the `OpenHub` resource), because the dashboard is one server's and a game between two friends has none; the dashboard's column and the servers page lead to the hub too. **The hub** is two tiles, Host a session and Join a session, over the **saved peers** (`netrunner_client::peers`, `peers.json` beside `servers.json`, never rows of the servers: a server is a place, a peer is a person) — each a Quick connect, Rename… and Remove, most recent first, remembered as a room is joined and named by the host's name once the room lists them. **Hosting** is the active decks in the dashboard's tiles, the format as pills greyed by them with the reason (`OnlineForm::host_choices`, the queue form's rule, so a greyed format is never hosted in), the reach as three pills — This machine only, Your network, The internet (`Reach::title`, `line`) — and the port; "LAN only, zero-config broadcast" stays a later stage, since nothing discovers a host on the network. **Joining** is the decks and one field, with what it holds said under it as it is typed — a ticket, an address with its host and port, nothing yet, neither (`online::Target`) — and Connect. **The room** (`Page::Room`) is where both land: for the host, what to give out (the ticket with Copy ticket, the addresses, what the router said); for both, the participants — each their decks, a chair preference, Prefer Corp / Either / Prefer Runner, and **Ready**, which is a seek with both decks in the hosted server's one queue, taken back by Not ready after all. **Both Ready is the start:** the host's own server seats them the moment the second Ready lands, so the wireframe's locked Start went — it would only have delayed the game. Ready is greyed with the reason while an active deck is not legal in the room's format, and while the opponent is ready and wants the chair this person insists on (`ready_problem`): the server would seat them by a coin, which nobody asked for, so the person is told to give way or choose Either. Back from the board is the room again, Ready cleared on both sides, so a rematch is two presses; Escape takes Ready back first, then leaves the room — the host's Stop hosting, the guest's Leave — to the hub. **A connection is a room when it is hosting, came through the hub, or is by a ticket from anywhere** (`ServerState::room`): a ticket is always a hosted game's, so one pasted on the servers page's Connect once opens the room too; a server by its address is the dashboard. **A host's ticket is the host's key** (`peer::PeerHost::start`'s `key`, `Identity::secret_bytes`, both clients' hosts): the ticket names the key the joiner dials and the relay finds that key wherever the machine is, so a ticket made on the player's identity is the same every time they host and a friend who saved it Quick connects whenever a room is open — with the relay off, only where its direct addresses still reach; a test, a dev hook and a machine with no key host on a throwaway one, as every host did before. Rejected: keeping the throwaway key and saving tickets anyway, which would have made a saved peer a row that never connects twice. **The lobby lists who is in it** (`LobbyInfo::people`, a `Presence` each — the name, whether looking, the side preferred while looking — from which `players()` and `seeking()` are read; the server's `Registry::members` keeps each lobby's links and names), and the room asks for the list every second (`room_ticks`), since the server pushes nothing about a lobby's people. The terminal's hub rows are stage 2, as §14's dashboard rows are its stage 5; the terminal already hosts on its key. Tests: the model's (the host from the hub into the room and its queue; Ready as the seek, taken back, greyed by a chair conflict and by an illegal deck; the room after a game; Escape not ready then out to the hub; a guest's join with the field's four answers, the host remembered by name — or by the target until the room names them — Quick connect, rename and remove; the host page greying a format and refusing to host in it), the peers file's, the target detector's, a host on its own key giving the same ticket twice (`remote`), and the headless ones against real servers: the host in its room and ready, a guest seated by the driver, the board, the concession, the room again; a guest through the hub with the host named in the room and saved as a peer, both ready, the board for each, both rooms again, both out to the hub; a ticket pasted on the servers page opening the room and kept as a peer, never as a server. Screenshotted on a virtual compositor at 1600×1000 and 1280×720: the hub with peers and empty, the host page, the room as host and as guest with an opponent ready, the join page (`NETRUNNER_ONLINE=hub|host|join|room|room-guest`).
