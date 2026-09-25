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

## 1. Foundations — DONE (14 September 2026)

`feat/desktop-foundations`. Three crates touched, one new client.

**`netrunner_client`** (new). `settings`, `deck_store`, `decks` and
`ratings` moved out of `netrunner_cli` with their tests; the CLI keeps
thin `Config` wrappers (`ratings::seat_rating`, `settings::apply`) so
its call sites changed by a line each and `netrunner_cli --help` is
byte-identical. `Settings` grew `desktop: DesktopPrefs` (animation
speed, two volumes, the image-download opt-in, a window size), all
`#[serde(default)]`, skipped while untouched so a terminal-only player's
file stays two fields long; `format` became `Option<NsgFormat>` with the
file's lowercase spelling kept by a field serializer, since changing the
engine's serde form for a client file's sake is the wrong direction.
`ratings` is `Config`-free: a plain `BotKind`, `player_name(given)`,
`SeatRating::open(SeatRatingSpec)`, `standing_lines(path, player)`.
`cards::format_pool` is the deck builder's private `legal_in`, made
public because the browser and the desktop's builder need the same
answer. Pinned: `settings_desktop_block_survives_a_save_by_a_client_that_ignores_it`
— the reason the struct is shared, as a test.

**`netrunner_card_sync`** gained the image cache. `CardImageStore` under
`<cache dir>/netrunner/images/<code>.jpg`, keyed by `numeric_id`;
`status` (missing / cached / failed this run), `cached_count`,
`download` (semaphore-bounded, temp-file-and-rename, a 404 remembered in
memory only so NetrunnerDB fixing a scan needs no fix here), and
`refresh_template`, which reads `imageUrlTemplate` off the cards
envelope — a key the envelope always carried and the DTO always dropped
— and keeps it in `images/manifest.json`, so a host move is picked up by
the next refresh rather than a release. The HTTP client gained the user
agent and the 30 s timeout it never had. `CardDefinition::image_url`
stays `None`: the store, not the card, knows where the picture is.

**`netrunner_desktop`** (new). The plugin-per-screen shape (recorded in
AGENTS.md §5 and `lib.rs`): `AppScreen` states, `nav::Navigate` as the
one way between them, `nav::screen_root` with `DespawnOnExit`, Escape
leading back everywhere but the menu. Screens: Boot (loads the client
core, starts the font, puts up the camera), Main Menu (eight entries,
the terminal's six plus Cards and Profile), Profile (name, the same
`standing_lines` the CLI prints, cached image count, every file path),
Settings (six rows, saved after every change, the name in a small
keyboard-driven field — Bevy's `EditableText` is a full parley editor,
and a name is one line), and a stub for every screen a later phase
builds, so no menu entry is a dead end. Widgets are Bevy's `Button` with
one shared feedback system writing `Pressed(entity)`; a screen tags its
buttons with its own marker and looks the entity up. Theme: Noto Sans
under its OFL notice (`assets/fonts/LICENSE-OFL.txt`), the Corp blue and
the Runner red, each faction its colour. `models/settings.rs` is the
first toolkit-free model with an `Intent` enum and five tests;
`tests/navigation.rs` drives the real plugin set under `MinimalPlugins`
with a `ClientCore` pointed at a temp directory — boot lands on the
menu, every screen enters under one root and leaves with none, Escape
leads back, a text field captures Escape until it closes.

**Found on the way.**

- **The discrete GPU cannot present here.** On this box (Wayland, NVIDIA
  550 beside an Intel iGPU) Bevy's default high-performance adapter
  enumerates, creates the window, and then fails every frame with
  `Surface::configure: Invalid surface`, under Wayland and under X11
  alike; `WGPU_POWER_PREF=low` renders cleanly on the Intel adapter.
  `main.rs` now asks for the low-power adapter unless the variable is
  set. A card game does not need the discrete GPU and the display is
  wired to the other one.
- **Bevy's `Font` asset type does not exist without the text plugin**,
  so boot loads the font only when `Assets<Font>` is registered; asking
  the server for a handle to an unregistered type is a panic, which is
  how the headless tests found it.
- **A held key is not a pressed one.** The test's first Escape helper
  only pressed; the input plugin then saw the key as held and the second
  Escape was never `just_pressed`. A tap is a press and a release.
- **Edition 2024 captures every in-scope lifetime in `impl Bundle`**,
  so a widget constructor taking `&Theme` had to declare `+ use<T, M>`
  to return an owned bundle.
- `netrunner_cli` no longer used `dirs`; `cargo-machete` said so.
- **A Bevy node shrinks by default.** The first build the user tried had
  every main-menu button at a different width: each row overflowed its
  panel and flex shrink squeezed each button by the length of its own
  blurb, Quit alone keeping its full width for having none. A button's
  `Node` now sets `flex_shrink: 0`, in the shared helper, and the blurb
  is the flexible thing in the row.

**Repo.** `Cargo.lock` 358 → 740 packages. `deny.toml` allows three more
licenses, each with its forcing crate: BSD-2-Clause (`arrayref`, under
winit's Wayland decorations), MIT-0 (`encase`, under everything that
renders), CC0-1.0 (`hexf-parse`, under the shader compiler). CI: a
`desktop` job on the three-OS matrix with `libasound2-dev libudev-dev
libwayland-dev libxkbcommon-dev` on Linux and its own cache key (the
composite action gained a `cache-key` input); `test`, `beta` and `docs`
run `--exclude netrunner_desktop`; the `ci` aggregate requires it. First
cold-cache run (PR #23): **ubuntu 16m03s, macos 16m42s, windows 33m35s**,
against a 45-minute timeout — Windows is the one to watch as the crate
grows. The engine jobs were unmoved (ubuntu `test` 2m59s). *Since 14
September 2026 the `desktop` job runs weekly in `platforms.yml` and the
`ci` aggregate no longer requires it; see the Infrastructure row of
`ROADMAP.md`.*

**Verified.** `cargo test --workspace` green (36 in `netrunner_client`,
12 in `netrunner_card_sync`, 10 in `netrunner_desktop`), clippy silent,
`cargo deny check` and `cargo machete` clean, the window opens to the
menu on the integrated GPU. Still to be driven by hand: each menu entry
and Escape back, Profile against a real ratings file, a name and a
format set here and read by the terminal client's Settings with the
`desktop` block surviving its save.

---

## 2. Card faces and the browser — DONE (14 September 2026)

`feat/desktop-card-faces-and-browser`. The first screen with content:
every printing as a face, the chosen one open beside it with its printed
text and how the engine reads it — what the menu entry promised. The
face is a widget, not browser furniture, because §3 puts the same one on
the board.

**`netrunner_client`** grew the toolkit-free half, tested under plain
`cargo test`. `prose` moved in from the CLI with `engine_reading`
beside it (`main.rs` swaps `mod prose;` for a `use`, so every
`crate::prose::` call site is untouched and the TUI's inspector is
byte-identical) — the desktop could not reach a module in a crate with
no `lib.rs`, and §1's rule is lift, not copy. `card_text::segments`
splits a printed text at its symbols (`[credit]`, `[click]`,
`[subroutine]`, `[trash]`, `[mu]`, `[recurring-credit]`, `[link]`,
`[interrupt]`), its line breaks and `Trace[N]` — a trace strength the
card prints as a superscript, the one bracket the first cut did not
know — and every catalog text parses with no bracket left over.
`card_face::Face::of` decides *where* a number goes — cost top-left, an
agenda's advancement requirement in its place, strength bottom-left on
ice and programs, memory and trash cost bottom-right, an identity's deck
size, influence limit and link along the bottom, influence as pips —
pinned per type and over every printing. `cards::catalog` is every
printing with the playable card standing in for its own (271 entries,
reprints separate: a Core Set and a System Gateway Hedge Fund are two
pictures), and the TUI builder's private `type_group` / `type_order` /
`faction_label` / `faction_order` are public here, so a group has one
spelling in both clients.

**`netrunner_desktop`.** A second font, **Noto Sans Symbols 2** under the
same OFL notice: Noto Sans has no arrows, geometric shapes or dingbats
(checked against its character map), so `◆ ● ⏱ ▸ 🗑 ▣ ⭮ ⛓ ⚡` would have
been tofu. Each glyph is named on `Symbol::glyph` and was checked against
the new font's map too — the arrows block is in neither, which is why
the subroutine is a triangle rather than the hooked arrow the card
prints; `Symbol::fallback` (`¢`, `»`, a word) is drawn when the font is
not loaded, which is the headless tests' path. `widgets::card_face`
draws a `Face` at two sizes (140×196 and 300×420, both 5:7): the text
layout when no picture is cached, tagged `WantsImage`, and
`card_images::poll_decoded` swaps the picture in place when one arrives,
so a download finishing while the browser is open fills the grid without
leaving it. Pictures are **bytes decoded with `Image::from_buffer` on
the compute pool**, not asset paths: the cache directory is outside the
asset root and `AssetPlugin` refuses absolute paths by default
(`UnapprovedPathMode::Forbid`), and a second `AssetSource` would have to
be registered before `DefaultPlugins`. `card_back::paint` draws the two
backs (side colour, rim, traces, a diamond) unless
`assets/cards/back-{corp,runner}.png` exists — the documented drop-in
for the official PNGs, never committed — through `assets::resolve`, the
three-tier rule in one function. `downloads::Downloads` runs
`refresh_template` then `download` on the tokio runtime, four in
flight, progress on a channel and the report on a oneshot, and is not
tied to the screen that started it. The browser
(`screens::card_browser`, `models::browser` with its `Intent`) is three
columns: a rail of chips (side, faction, type, "Startup only", Clear),
a search field open from the start that filters live, and the download
button, which says "Card images are off in Settings" until the opt-in
is on; a CSS-grid of thumb faces under `bevy_ui_widgets::ScrollArea`,
each a button; and the inspector — the large face, the numbers line the
terminal's `card_modal` prints, set and code, "Not implemented in the
engine yet" for a catalog-only card, the text, the flavour, "Engine
reads it as", and whether the picture is cached. Escape clears the
search, then closes the field, then leaves. `ClientCore` carries the
catalog, built once at boot.

**Found on the way, and fixed here.**

- **Core Set image URLs were wrong.** `CardId` is a `u32`, so NetrunnerDB's
  `"01001"` is `1001`, and `CardImageStore` asked the CDN for `1001.jpg`.
  Codes are five digits; `path_for` and `url_for` now pad, and a test
  pins `CardId(1001)` → `01001.jpg`. Nothing had called `download` yet.
- **List items ran together** in the catalog's `printed_text`:
  `strip_markup` dropped `<li>` without a separator, so Wildcat Strike
  read "choice:Gain 6[credit].Draw 4 cards." Each item is now its own
  bulleted line. The clause gate normalises to alphanumerics, so it stays
  green; five cards across sg and elev.
- **A bundle may not carry a component twice.** Wrapping the shared
  button helper in a chip and adding a `Node` panicked at spawn — the
  chip is now spawned then adjusted, and the same shape caught the
  inspector's panel and a label's second font.
- `CardDefinition::printed_text`'s doc said `stripped_text` while the
  code used `text`; `text` is right (the symbols are wanted) and the doc
  now says so.
- **An unoptimised build could not draw the browser.** With 271 faces on
  screen the second frame came 1.8 s after the first and the third never
  came inside ninety seconds: text shaping at opt-level 0, through
  taffy's measure calls, for some two thousand text nodes. §1's menu had
  eight. The workspace `Cargo.toml` now carries the Bevy book's
  `[profile.dev.package."*"] opt-level = 3` — dependencies optimised,
  the workspace's own crates not, so the engine keeps its
  `debug_assertions` and overflow panics and CI's
  `CARGO_PROFILE_DEV_OPT_LEVEL=2` is unaffected. One slow rebuild of the
  dependency graph, then `cargo run -p netrunner_desktop` is playable.
  That rebuild is heavier than before as well as slower: twenty
  parallel optimising `rustc`s on a 32 GB box ran it out of memory once,
  and `-j 6` did not.
- **A window nobody is looking at gets no frames.** With dependencies
  optimised the second frame came 0.2 s after the first — and the third
  still never came. A stack trace of the process showed the render
  thread inside Vulkan's `queue_present`, in `wl_display_dispatch_queue`:
  under Wayland the compositor withholds frame callbacks from a surface
  it is not showing (a window launched from a shell with no one to
  raise it), and with vsync on, present blocks on them; the main thread
  waits on the render thread, and the app sits still without a panic or
  a log line. The dev screenshot path now sets `PresentMode::AutoNoVsync`
  on the window, which does not wait (`MESA_VK_WSI_PRESENT_MODE=immediate`
  is the same from outside). The frame-time finding above was real and
  is fixed, but it was hidden behind this one until the trace.
- **Looking at a screen without eyes.** `dev.rs`: `NETRUNNER_SCREEN=cards`
  boots straight into a screen and `NETRUNNER_SCREENSHOT=<png>` saves the
  window after thirty frames and exits — how both findings above were
  made, and how a model checks what it drew. Neither is a feature; both
  are ignored when unset. The first picture of the browser found two
  more things: the text field's caret `▏` is not in Noto Sans (a box; now
  `|`), and both neutral factions were a chip labelled "Neutral" (now one
  chip that matches either side's neutral cards until a side is chosen).

**Verified.** `cargo test --workspace` green (52 in `netrunner_client`,
13 in `netrunner_card_sync`, 18 in `netrunner_desktop` — the headless
browser test enters, opens a face, types "tithe" and narrows the grid,
and leaves in three Escapes; `tests/faces.rs` draws every catalog card
at both sizes and reads each body back span by span), clippy silent
with one new crate-level allow (`too_many_arguments`: a system's
parameters are what it reads, and a `SystemParam` struct moves the list
rather than shortening it), `cargo machete` clean. Still to be driven by
hand at that point: the picture download end to end on this box, wheel
scrolling on the grid, and the drop-in backs.

**Driven by hand (15 September 2026), a second commit on the branch.**
The download ran end to end (271 scans cached) and the drop-in backs
are untried. Everything else the hand found is below, with what was
done about it.

- **The grid did not scroll.** Not the input: a new dev hook
  (`NETRUNNER_SCROLL=x,y,lines` puts the pointer at a window position
  and turns the wheel through the same `WindowEvent`s winit sends, then
  logs every scrolling node's size, content size and position) showed
  the wheel reaching the grid's `ScrollArea` observer, and the grid
  reporting its scrollable content as **one thumb** — 140×196 for 271
  of them — so the scroll range was zero. taffy 0.10 measures each grid
  item's content contribution from its *own grid area*
  (`align_and_position_item`: `Point { x: x - grid_area.left, y: y -
  grid_area.top }`), and the maximum over the items is one cell. The
  grid is no longer the scroll container: it sits inside a flex column
  that is, whose content is the grid's full height (11,220 px; three
  wheel lines move it 300). A first guess — that `ScrollAreaPlugin` was
  never added — was wrong twice over: `DefaultPlugins` brings it with
  the `ui` feature (adding it again is a panic at start-up), and the
  headless app now adds it and `ScrollbarPlugin` only where absent, so
  the tests run the same observers.
- **A scrollbar** beside the grid and one beside the inspector
  (`widgets::scrollbar`, Bevy's `Scrollbar`/`ScrollbarThumb`: the thumb
  is sized and placed by the plugin, and a drag or a click on the track
  moves the target).
- **The keyboard moves the selection**: the arrows by one or by a row
  (the column count read off the grid's laid-out width), Page Up and
  Down by three rows, Home and End to the ends — `Intent::Step(n)` on
  the model, clamped to the visible list, so a key can never open a
  card the grid does not show. The grid scrolls to keep a key-moved
  selection in view and never after a click. A selection change moves
  the outline and respawns the inspector only; respawning the grid for
  a held key was a slideshow.
- **The inspector's face grew** from 300×420 to 380×532 and the panel
  scrolls under it.
- **Drop-downs instead of chips** (`widgets::dropdown`: a head that
  reads "Label: choice ▾", a list spawned under it on press and lifted
  above the grid with `GlobalZIndex`, closed by a choice, a click
  elsewhere or Escape). Five filters in one toolbar row where the chips
  took a rail. Escape now has three claimants — an open list, the search
  field, the screen — and `nav::Captures` is the system set that orders
  them: the flag is reset at the start of `Update`, the widgets set it
  from inside the set (a list before the field, so one Escape closes
  the list and leaves the search alone), and the Escape rule reads it
  after. A keystroke in the search respawns the grid but not the
  drop-downs, so a list left open stays open while the grid narrows.
- **Set and format filters**, and a **"Legal in Startup · Standard ·
  Eternal · Snapshot"** line in the inspector (`cards::legal_formats`,
  by the same tables `legal_in` reads; `cards::set_name` for the three
  pack codes the catalog carries).
- **NetrunnerDB's icon font, fetched to the cache.** The site draws the
  factions, the sets and the printed symbols with one 38 KB TrueType
  face (`netrunnerfont.css`; code points U+E900–U+E935, its whole
  character map). Its repository is MIT but the marks are Null Signal
  Games' and its predecessor's, so it is treated as the scans are:
  `CardImageStore::download_icon_font` fetches it on the same opt-in as
  the pictures, checks for a TrueType header before keeping it (a CDN
  error page would load as a font and fail silently), and never ships
  it. `icon_font.rs` loads it from bytes (`Font::from_bytes`, what the
  asset loader does) and `Theme::symbol` is now the three tiers in one
  place: the icon font's glyph, else Noto Sans Symbols 2's, else the
  Latin-1 fallback. `card_text::faction_icon` / `set_icon` and
  `Symbol::icon` are the code points, tested to be distinct and inside
  the map; the faction's mark sits in its drop-down entry, on the
  inspector's faction line and in the text face's bottom row.
- **`rodio::stream` errors are filtered** (`main.rs`): the audio plugin
  opens the output stream at start-up and holds it idle, and on this box
  (PipeWire behind ALSA) the idle stream logged `alsa::poll() returned
  POLLERR` at random, as an ERROR line, over and over. A stream nothing
  has written to cannot have failed in a way a player would notice.
  `RUST_LOG` still overrides the whole filter; §4's sound bank revisits
  it if a playing stream shows the same.
- **The download button** moved to the status row beside its progress
  bar; with it in the toolbar the row wrapped and stranded Back.
- **"Engine reads it as" drew a box** where the prose joins a clause to
  its reading with `→` — the arrows block is in neither bundled font,
  as §2 already recorded for the subroutine. The inspector draws `›`
  there; the prose itself is unchanged, since the terminal has the
  arrow.

**Open, for §3 onward:** the `Sfx`/audio and tween modules exist only in
the plan; the stubs name the phase that replaces them. The board will
want `spawn_face` at a third size and a `WantsImage` that survives a
`Transition`.

---

## 3. The play area — DONE (15 September 2026)

`feat/desktop-play-area`. The novel and risky part of the client, and
the reason it exists: a game is played on a board, against a rung, from
a form, and the terminal client is unchanged for the person who uses it.
The order inside the PR was the order of risk — the match on its own
thread first, then what a click means, then what changed, then the two
screens over them — and every piece below the screens is tested under
plain `cargo test` with no window.

**Decisions taken, with the alternative rejected.**

- **A local match runs on its own thread behind channels**
  (`netrunner_client::play::MatchHandle`, `LocalMatchSpec`,
  `MatchMessage`). `Session::step` blocks while a `Seat::Agent`
  searches — seconds at `elite` — and a session pumped from a system
  would freeze the frame for as long as the bot thinks. The thread runs
  the loop `netrunner_cli::tui::drive_local` runs, one `step` at a time
  (never `run`, which swallows the bot's `Applied` steps: each log entry
  is the human's *masked* copy of that action, and `last_entry_for`
  reads concealment off the state the action left), and sends
  `Applied { entry, view }` after every action of either seat,
  `Awaiting { view }` when the person must act, `Rejected { reason }`
  with the engine's own words, and `Ended` with the rating report. The
  client sends a `PlayerAction` back, **unfiltered** — `Session::submit`
  is the only authority (the Session Rule), and a rejection comes back
  as a message with the same seat still awaiting. Everything that can
  fail — a deck that will not validate, a ratings file that will not
  load — fails in `start_local` before the thread exists, so a game that
  cannot start is a notice under the form rather than a board that dies
  on its first frame (Phase 6's rule, kept). `std::sync::mpsc` and
  `std::thread`, no new dependency; the receiver sits behind a mutex
  only because a `Receiver` is not `Sync` and a Bevy resource has to be.
  Dropping the handle quits the game the way the terminal's `q` does: a
  forfeit from turn 3 (`ratings::quit_outcome`), nothing before. A
  remote game (§7) and a lesson (§6) will feed the same messages, so the
  board cannot tell which it is playing.
- **`board::ActionMap` is the legal list, indexed by target.** Every
  element of `view.legal_actions`, in the engine's order, with its label
  (`actions::describe_action` — the terminal's words) and the targets a
  click could mean it by: a hand card, an install, a server, an
  identity, a selection position. The `for_*` lookups return indices into
  the one list, never a second list, so the flat panel is the contract
  and the board a convenience (AGENTS.md §5: a card the layout cannot
  place is still playable). An install names two targets — the card and
  the server — so either click offers it; a trojan the card and its host.
  `Prompt::of` is the heading over the panel: a card's own words where a
  card is asking (`option_texts`, `PendingPaidChoice::text` — the Linked
  Clause Rule), the run's phase, the trace, the access, the mulligan. A
  test over four random-vs-random games checks every legal action is
  exactly one entry and every target is on the viewer's own board. Two
  findings on the way: an identity's ability is activated by the
  engine's identity handle (`InstallId::CORP_IDENTITY`), which is on no
  server, so `Target::Identity(Side)` exists for it; and a
  `ChooseServerForPendingDecision` may name the remote it would create.
- **`board::diff::transitions` computes what changed, from the masked
  events and the two views.** The events say *what* (`CardDrawn`,
  `CardInstalled`, `IceRezzed`, `AgendaScored`, `DamageTaken`), the
  views say *where* (which install, which slot) and *how much* (credits
  and clicks before and after, tags, bad publicity, a run's position and
  phase, a run ending — successful if it had reached `Success` or a
  `RunSucceeded` is in the entry). A card the mask struck moves as
  `card: None`; a draw the viewer can see is a multiset difference of
  their hand, so a second copy of a card already held is still the card
  drawn. The test plays six games as both viewers, half with the
  heuristic Corp so agendas get scored, and checks every transition
  against the view it describes — every `to` zone holds the card, every
  number equals the after-view's — and that no transition names a card
  neither of the viewer's views showed. It found the one real bug of the
  module before it shipped: a program can be installed from the heap or
  the stack, so an install's origin is where the card *was*, not the
  hand. The screen consumes them as highlights (an `Outline` on each
  install or hand card a transition touched, for one redraw); §4 turns
  the same list into movement and sound.
- **The new-game form's state was lifted, not rebuilt**
  (`netrunner_client::start::StartMenu`, from `netrunner_cli::tui::start`).
  The terminal keeps its key bindings (`tui::start::key`, over an
  `Intent`), its drawing and the fold into `Config` (`apply_choice`);
  the desktop draws each pane as a drop-down and calls `set_cursor`. So
  both clients list the same decks with the same labels, suggest the
  same rung from the same book, default to the same decks
  (`DEFAULT_CORP_DECK` / `DEFAULT_RUNNER_DECK`, which the terminal's
  clap defaults now read), and reopen on the game just played with the
  rung moved to the new suggestion. The same lift for the words: every
  label and log line (`describe_action`, `explain_action`,
  `narrate_event`, `push_log_line`, `visible_zones`) moved verbatim from
  `netrunner_cli::app` to `netrunner_client::actions` with its tests,
  and `app.rs` keeps `pub use` re-exports so no call site there moved —
  the §2 `prose` precedent. `stall_message` and the style rule
  (`play::personality_for`: the flag, else the deck's own) went the same
  way, each now called from the terminal.
- **The board is `bevy_ui`, respawned on change, and ICE lies flat.**
  The opponent's strip and hand across the top, their area, the person's
  area, the person's hand beside their strip across the bottom; the
  Corp's servers are columns with the ICE as bars above the root — the
  way ICE lies on a table — because a rotated `UiTransform` is laid out
  as its unrotated box and would overlap its neighbours; the Runner's
  rig is three labelled rows. A face-down card is the side's back
  (`spawn_back`). The right rail is the prompt, the rejection if any,
  the action panel (one button per entry, in a scroll column) and the
  log, scrolled to its newest line. The board subtree is respawned when
  the view moves — at most once per applied action — and the rail and
  the overlay on their own, so a popup opening does not redraw the
  board. A click on a card or server with one entry submits it, with
  several opens a popup of them, with none opens the card's text; the
  panel is always there. Escape is the board's own (`nav::Captures`):
  it closes what is open, then asks before leaving, and the quit prompt
  says whether a forfeit will be recorded. The game-over overlay shows
  the winner, the reason and the rating lines, with Play again (the
  form, resumed) and Menu. `models::game::Game` is the toolkit-free
  state behind all of it, driven by an `Intent`, with the double-submit
  guard (`awaiting` goes off when an entry is chosen and comes back on
  `Rejected`) pinned by a test that plays a real match.
- **A third face size, `FaceSize::Board` (96×134).** A hand of eight
  and a rig of twelve have to fit beside the servers; at this size the
  face is a title, a cost and a number or two, and the inspector (the
  Large face, opened from any click with no action) is where the text
  is read. `tests/faces.rs` draws every catalog card at it.
- **Two dev hooks, neither a feature.** `NETRUNNER_GAME=corp|runner`
  starts an unrated game on the default decks against the middle rung
  at boot and lands on the board; `NETRUNNER_AUTOPLAY=<n>` has the
  person's seat take `n` decisions by itself, cycling through the list
  so the game develops. Together with `NETRUNNER_SCREENSHOT` they are
  how this board was looked at, sixty decisions in, by someone who
  cannot look — and how the layout problem below was found.

**Found on the way.**

- **Two strips of their own put the person's hand below the fold.** The
  first mid-game screenshot as the Runner (turn 6, sixty autoplayed
  decisions) had the board's content at 971 px in a 723 px viewport,
  and the thing that had scrolled off was the person's own hand. Three
  cuts, each read off the next screenshot: each side's hand beside its
  strip in one row (971 → 969, because the servers then wrapped), the
  server headers as compact buttons so seven columns fit across (→ 918),
  and the identity in each strip cropped to its top 72 px, the strip's
  text naming it anyway (→ 761, the board's height). An ICE bar lost
  its "(unrezzed)" suffix, which wrapped inside the bar; the dim border
  and text say it now.
- **Remotes are numbered as the engine numbers them.** The first header
  said "Remote 1" over a panel that said `Run Remote(0)`, the terminal's
  label — two names for one server. `server_name` now says `Remote 0`.
- **A `Receiver` is not `Sync`**, so a `MatchHandle` could not be a
  resource until its receiver went behind a mutex. One uncontended lock
  a frame.
- **An install is not always from hand.** `ProgramInstalled` fires for a
  program a card's text installs from the heap or the stack, and the
  diff's first cut said "from the hand". The transitions test caught it
  on seed 2 (`fermenter`, from the heap): the origin is now where the
  before-view showed the card, with the hidden hand as the default only
  for a viewer who could not see it there.
- **Autoplay as the Corp flatlined the Runner in six turns**: the
  cycling hand ran into Urtica Cipher twice. Which is to say the
  game-over overlay was seen before it was looked for.

**Verified.** `cargo test --workspace` green (`netrunner_client` 52 →
79, `netrunner_desktop` 18 in the library, 4 in `tests/game.rs` — the
form starts a match, the board offers every legal action, a press
submits one and the next decision comes round; Escape asks and a second
Escape withdraws; leaving ends the match; the board with no match says
so — 7 in `tests/navigation.rs`, `tests/faces.rs` at three sizes;
`netrunner_cli` 71, unchanged in count with the lifted tests moved),
clippy silent, `cargo machete` clean. No engine change, so no sweep and
no coverage report: `netrunner_core` is untouched. Screenshots of both
chairs at the mulligan and mid-game are what the layout was checked
against. **Still to be driven by hand:** a full game in each chair at
`operator`, one at `elite` to feel the frame stay live under PUCT, a
quit from turn 3 checked in `ratings.json` and in `netrunner_cli
ratings`, the popup on a card with several actions, the inspector, and
the picture-less text face at board size.

**Driven by hand (15 September 2026), a second commit on the branch.**
The first game a person played found one bug and a board that was not
yet a board to play on.

- **Every card click did nothing.** A face is a `Button` but not a
  `Themed` one, and the shared feedback system reports presses only for
  themed buttons — so no card, and no identity, ever reached the model,
  and the inspector the design promised never opened. The board's
  `controls` now reads a face's `Interaction` change itself; a test
  presses a hand card at the mulligan and sees the inspector.
- **Half a card reads as a broken card.** The identity crop and the
  cropped opponent backs, both cut to make the board fit, were the first
  thing the person saw; both are whole again and the board scrolls.
- **The cards were too small to tell apart.** `FaceSize::Board` was
  96 px wide so a hand and a rig would fit beside the servers; the
  person asked for double, and it is 180 px now.
- **The numbers were invisible.** Credits, clicks, points and tags were
  in the strips in the dim small face, and the person reported "no
  on-screen indication of points/credits/health". They are in the body
  size and the text colour now; a real HUD is item 6 below.

**Noted, to fix later — the person's list, kept in their order:**

1. **`Toggle selection of card N` is unusable**, in both clients: a
   position into a zone tells a person nothing. `describe_action` can
   resolve the position through the prompt's `source` zone when the
   viewer can see it (their own hand, the heap), and the board can put
   the toggle on the card itself.
   *Done in §4c, in both clients: `Select Hedge Fund`, `Confirm Hedge
   Fund`, `Select a different card`. The view alone could not do it — an
   R&D or stack search has no cards in the view — so the engine now
   publishes the prompt's candidates to the chooser. The toggle on the
   card itself is still to do.*
2. **A sole legal action should not need a click.** Passing priority
   over and over is the whole of a run from the other chair. The model
   can submit a lone `PassPriority` (and, arguably, any lone action) on
   arrival — a client policy, not a rule, and worth a short delay so
   the board is seen to change.
   *Done in §4e, in both clients: the lone pass only, not any lone
   action, and the desktop's run pacer is the delay.*
3. **The `operator` Runner ran Archives three times running**, into
   Urtica Cipher's damage, having seen what was there, and never drew a
   card — the stack stayed at 24 — until it flatlined itself. A bot
   blindness for Phase 5's ladder: the one-ply Runner's run choice does
   not read a known Archives, and its draw term is too weak to act.
   *Done in Phase 2 §5a (15 September 2026): a run is worth what the
   breach can find — a known ambush counts against it — and a card in
   grip is worth half of what installing it would be, so a draw can beat
   a credit. Archives runs 627 → 46 and draws 76 → 332 over 96 games.*
4. **The layout does not use the play field.** Two strips, a row of
   servers, a row of rig and a row of hand, stacked and scrolling, is a
   list, not a table; §4 owes the board a real arrangement — the
   opponent's side mirrored across the top, the servers spread, the hand
   fanned along the bottom.
5. **Actions should live on the cards.** A person playing a card game
   expects to pick up the card and see what it can do; the panel reads
   as a help menu. The popup already lists a card's entries; §4 anchors
   it at the card and makes the panel the fallback it was designed as.
6. **A HUD**: credits, clicks, points, tags, damage and hand size for
   both sides, always in the same place, big enough to read at a glance.
   *Done in §4f: a grid of large numbers in each side's strip.*
7. **The window must fit the whole game without scrolling.** With the
   faces made readable the board is 1,780 px tall in a 723 px viewport;
   a design is owed that puts every card and piece on one screen —
   fanned hands, overlapped ICE, a scale that follows the window.
   *Done in §4a, after it regressed there first: fullscreen, a computed
   card width, overlapped rows, no scroll container on the board.*
8. **A click to look at a card installed it.** The model submits a
   card's action when it has exactly one, so "examine" and "install"
   are the same click, and one game was lost to it. A click must never
   submit by itself: it opens the card's actions (item 5) with reading
   it among them, and a second, deliberate click acts. The one-entry
   shortcut in `models::game::Game::click` is the line to remove.
   *Done in §4's first PR: a click opens a sheet and never submits.*

Three more, from the second look (15 September 2026), kept in the
person's order:

9. **The official card backs, fetched and used.** Null Signal Games'
   backs are today a documented drop-in only (`assets/cards/README.md`);
   they should be downloaded into the cache on the same opt-in as the
   scans and the icon font, and drawn wherever `CardImages::back` is
   asked, with the painted back as the tier beneath.
   *Done (15 September 2026, `feat/official-card-backs-cached`).
   Null Signal Games publishes no backs — its print-and-play files omit
   them and its visual-assets page keeps them out of the public pack —
   so the source is the copies jinteki.net serves for its own table
   (`img/nsg-corp.png`, `img/nsg-runner.png`, 255 × 356 sixteen-bit
   PNGs), read on the images opt-in for the player's own screen the way
   the scans are read from NetrunnerDB, never committed.
   `CardImageStore::download_card_back` keeps them beside the scans as
   `back-corp.png` / `back-runner.png` — the drop-in's names, so one
   name means one file in every tier — after checking for a PNG
   signature. `card_images::load_backs` picks the best tier on disk at
   start (drop-in, bundled, cache, painted) and, for a back still
   painted, looks at the cache once a second and fetches once when the
   opt-in is on; the fetched image is decoded off the main thread and
   put **under the existing handle** (`Assets::insert`), so a hand of
   backs already on screen changes without a redraw and no screen
   knows the tier moved. One finding: a sixteen-bit PNG decodes to
   `Rgba16Unorm`, which has no sRGB variant, so the renderer read it as
   linear and drew the back washed out; `eight_bit_srgb` narrows it to
   `Rgba8UnormSrgb` on load, tested. Verified as both chairs forty
   decisions in: the fetch landed 0.3 s after boot, mid-autoplay, and
   the opponent's hand showed the official back at the screenshot; the
   second run read them from the cache at start.*
10. **The servers as a table reads them, right to left**: Archives, R&D,
    HQ, then the remotes — `spawn_servers`' sort key is the reverse of
    that today, HQ first from the left.
    *Done (15 September 2026, `feat/board-as-the-table-from-the-chair`).
    The rule is Null Signal Games' setup seen from the chair: the
    learn-to-play guide puts R&D at the left of the Corp's area with
    Archives to its left and the identity (HQ) to its right, ice "in a
    column out toward the Runner" and a root "between these areas and
    the ice". So the Corp's chair reads Archives, R&D, HQ, remotes left
    to right with the ice climbing away at the top of each column,
    outermost first; the Runner's chair is the mirror — remotes, HQ,
    R&D, Archives (the "right to left" asked for), header at the top,
    ice coming down to them with the outermost nearest. Three pure
    functions in `models::layout` (`servers_left_to_right`,
    `column_top_down`, `ice_top_down`), tested, and one desktop test
    that starts a game in each chair and reads the headers' order off
    the tree. Found on the way: the old column drew the ice
    innermost-first for both chairs while its comment said outermost
    (`ServerView::ice` is outermost-first, the engine's approach
    order), so a stacked server read backwards. The Runner's rig keeps
    its three groups in one order: the guide says an installed Runner
    card's position "does not matter", and three rows would cost board
    height for nothing. The columns line up along the Corp's edge —
    headers at the top from the Runner's chair, at the bottom from the
    Corp's — however tall their ice makes them, which the first Corp
    screenshot showed was needed: top-aligned, a bare HQ floated above
    a two-ice Archives. Verified as both chairs sixty decisions in, no
    board scroll area.*
11. **A deck is a stack of cards** with its count beneath, not a header
    with a number: R&D and the stack drawn as overlapped backs, the way
    a pile sits on a table.
    *Dropped (15 September 2026): no longer wanted. The header with its
    count stays as the deck's mark on the board, and the count is in
    the strip beside it; the numbering here is kept so the items above
    and below keep their addresses.*

And three from the look at §4a (15 September 2026), before it merged:

12. **The toggles fall out of the options window.** The gear menu's
    rows are the Settings screen's rows at their 220 + 260 px widths
    plus a control, in a 560 px panel; the control lands outside it.
    The rows want a width that follows the panel, or the panel the rows.
    *Done (15 September 2026, `fix/options-rows-fit-their-panel`): a
    row is as wide as its panel, the label takes what the value (170 px,
    wrapping) and the control leave. `NETRUNNER_OPTIONS=1` opens the
    gear menu over a dev game so it can be screenshotted.*
13. **A decision — the mulligan, an access, a choice a card asks —
    should be a pop-up in the middle of the screen**, not buttons on
    the rail at the top right: it is the thing the game is waiting on,
    and the middle is where the eyes are. The rail keeps the prompt's
    words; the decision's buttons move to a centred panel, the way the
    quit prompt already is.
    *Done (15 September 2026, `feat/decision-popup`): `DecisionPopup`,
    respawned with the rail whenever `ActionMap::decisions` is
    non-empty — the prompt's words as its heading, the rejection if
    any, one button per decision — in an accent-bordered panel centred
    over the board. It does not block the board: its full-window
    container is `Pickable::IGNORE`, so a card beneath can still be
    read, and it sits under the overlays so a sheet covers it. The rail
    keeps the prompt's words. One finding: a Bevy system takes sixteen
    parameters at most, and `redraw` was at the limit, so the overlay
    and pop-up entities share one `Has<_>` query.*
14. **Right-click on a card (or the Mac's equivalent) opens its
    actions** — install into a remote or a central, play, whatever the
    card can do — as a menu at the pointer, with the left click's sheet
    staying the way to read it. `Interaction` reports only the primary
    button, so this reads `ButtonInput<MouseButton>` with the hovered
    node.
    *Done (15 September 2026, `feat/right-click-actions-menu`). The
    menu is the sheet's list through a second door: `models::game::Menu`
    holds the target, `entries_for`'s indices and the box the clicked
    node was laid out in (`Anchor`, read off `ComputedNode` and
    `UiGlobalTransform`), so the two never disagree about what a card
    can do, and a press on a menu button is the same `Intent::Choose`
    as a sheet's. The menu sits just above that box, centred on it,
    not at the pointer and not on the card: the first cut used the
    pointer, and the person's verdict was that it jumped around with
    the click; the second centred it on the card, which was a misread
    of "above" — a card's menu is in one place for that card, and the
    card stays in view beneath it. When there is no room above (a
    header along the top edge, from the Runner's chair) it sits just
    below.
    The secondary click is the right button, or Ctrl held with the
    primary — the Mac's convention — and `controls` drops the primary
    press the card registers under Ctrl, so it opens no sheet. Read as
    planned: the focus system sets `Interaction` for the primary button
    only, so `secondary_click` takes the button from the input resource
    and the target from whichever `Click::Target` node is hovered —
    every card, ICE bar and header is a `Button`, which blocks, so the
    topmost is the one under the pointer. Kept on the window by an
    estimate of its height, since the layout has not run when it is
    spawned. It closes on Escape (before the sheet and the quit
    prompt), on a primary click that presses no part of it (the panel
    takes `Interaction` and blocks, so its ground counts as a part),
    when the board moves (an applied action — the card under the
    pointer may be gone; a sheet, by contrast, stays open), and when a
    card's primary click opens its sheet. It never opens through an
    overlay: `Node`'s default `FocusPolicy` is `Pass`, so an overlay's
    ground reports hovers on the cards beneath it, and the model
    refuses a menu while anything covers the board (`Game::covered`,
    now also what decides an overlay is drawn). Zones have the menu
    too — R&D's offers the run or the draw. Tested in the model and
    headless on the screen with `MouseButtonInput` messages and a
    hovered face; `NETRUNNER_MENU=1` opens it over a hand card for a
    screenshot.*

**Open, for §4 onward:** the transitions are highlights, not movement;
the sound bank and the tweens are §4. A `ChooseCards` prompt's positions
are reachable only from the panel (item 1). The opponent's hand is
drawn as whole backs capped at eight. The log keeps its last eighty
lines on screen.

---

## 4. Feel — IN PROGRESS

### 4a. Cards and zones as the way to act, a control bar, and a gear menu — DONE (15 September 2026)

`feat/desktop-cards-zones-and-options`. The second look at the board
asked for three things at once: no flat list of every action on screen
by default, a fixed set of buttons for the actions a turn is made of,
and the cards and zones as the way into everything else — a click on
R&D must never simply draw. Items 5 and 8 of §3's list are closed by
it; item 2 (a lone `Pass priority` taken without a click) is not, but
the pass is now one button in one place.

**Decisions taken, with the alternative rejected.**

- **Hiding the panel outright would have stranded `EndTurn` and
  `PassPriority`**, which have no card or server to click. So the
  target-less entries are split in two, in `netrunner_client` where a
  test can see it. `board::Control` is a fixed bar per side (Corp:
  credit, draw, purge, end turn, pass; Runner: credit, draw, remove tag,
  end turn, pass, continue, jack out, complete run — the run trio drawn
  greyed outside a run, so the bar never reflows), and
  `ActionMap::decisions` is everything else with no target — the
  mulligan, a bid, a paid choice, an access decision, a selection to
  confirm, and the `ToggleCardSelection` positions §3 item 1 still
  leaves unplaced — listed under the prompt as the thing it is asking.
  The map's test now checks, over four random-vs-random games as both
  viewers, that the bar, the decisions and the targeted entries cover
  every index: nothing is reachable only from the flat panel, which is
  the "play helper", off by default (`DesktopPrefs::play_helper`), on
  from the gear or the Settings screen. The log is the "play history",
  the same way (`play_history`). AGENTS.md §5's "reachable from the
  flat action panel" now says this.
- **A click never submits; it opens a sheet.** `models::game::Sheet`
  replaces the popup and the separate inspector: a card target is drawn
  as the Large face, its text and its legal actions as buttons (with no
  actions it is the inspector); a zone target — a server header, or the
  new `Target::Pile` for the Runner's stack and heap, which are now
  buttons in the Runner's strip — as its actions and its contents where
  the viewer may see them. Archives is every card for the Corp and, for
  the Runner, the face-up cards with backs for the rest (the
  `PublicArchivedCard` mask decides, never the screen); the heap is all
  face up; the Corp's own HQ is its hand; a remote is its ICE and root;
  R&D and the stack are a few backs and a count, because a deck's order
  is never shown, even to its owner. A face in a pile reads the card
  over the sheet (`Intent::Inspect`), and Escape closes the reading,
  then the sheet, then the options, then asks to quit. A sheet left
  open while the opponent acts stays open with its entries cleared and
  rebuilt on the next `Awaiting`, so reading Archives is not interrupted
  by the bot's turn. Anchoring a popup at the card (§3's note) is not
  done: the sheet shows the card, which is what anchoring was for. A
  draw is also on its deck (`DrawCardClick` targets R&D or the stack),
  so the zone's sheet offers it beside the bar — the one entry on two
  routes, which is the map's design.
- **The gear is painted.** Neither bundled font has U+2699 (Noto Sans
  Symbols 2 covers 2654–2668, 267f–268f and 269e–26a1 of the block,
  Noto Sans none of it — checked with `fc-query`), so `widgets::gear_image`
  paints a cog the way `card_back::paint` paints a back, and the button
  reads "Options" where `Assets<Image>` is not registered (the headless
  tests). The options overlay draws `models::settings::Row::GAME` — the
  two aids, the animation speed and the two volumes — through the
  Settings screen's own `spawn_rows`, so a row has one shape and one
  control wherever it appears, and a change is saved at once as there.
  A greyed control is `widgets::disabled_button`, the same node with dim
  text and a `Disabled` marker the feedback system skips.

- **The board fits the window, fullscreen, and never scrolls — a rule
  now, not an item.** The first cut of this PR kept the board as a
  scroll column at a static 180 px face and the person's hand went
  below the fold again, the third time a static size had done it; the
  person's words were that scrolling to see cards is unacceptable and
  that the cards must scale rather than the board scroll. So: the
  window opens `BorderlessFullscreen`; `models::layout::face_width`
  computes the widest card for which the four rows — the opponent's
  strip and hand, their area, the person's area, the person's strip
  and hand — fit `board_height`, by binary search over `rows_height`
  (which counts the tallest server's ICE bars, whether any root or rig
  card exists, and the strips' text, so an empty board gets larger
  cards and a full one smaller), capped by the server columns across
  and clamped to 72–220 px; `fit` recomputes it every frame from the
  primary window and the view and redraws the board when it moves; a
  hand, a row of backs or a rig wider than its room overlaps its cards
  by `layout::step` (never below a fifth of a card, so every card
  keeps an edge to click) instead of wrapping; the board root has no
  `ScrollArea` and `Overflow::clip` only as a backstop; and
  `FaceSize::Board` carries its width, its text scaled from the 180 px
  reference down to a floor. The control bar moved from under the top
  bar to centred along the bottom, beside the hand — at the top it
  made every action a mouse trip across the opponent's side. The first
  fit clipped the hand's bottom edge by 55 px: the height budget
  counted 0.84 of a card for a strip whose text is taller than that,
  and no server header; `rows_height` now sums what is drawn, with the
  strips at a fixed estimate (175 / 210 px) that errs long, since an
  over-estimate is slack between rows and an under-estimate is the
  thing that must not happen. Both chairs were screenshotted at 40 and
  60 decisions on a 2560×1600 screen with every card whole and the only
  scroll areas the rail's; AGENTS.md §5 carries the rule.

**Found on the way.**

- **`all` is vacuously true of no targets.** The first `decisions()`
  put `EndTurn` on the rail beside the bar: an entry with no targets
  satisfied "every target is a position". The map's coverage test
  caught it before the screen did.
- **The Runner is asked to pass priority during the Corp's turn**, so a
  headless test that waits for "the Runner's action phase" after
  keeping its hand waits forever unless it passes through the bar when
  asked — which is what a person does, and what the test helper now
  does.
- **An end of turn opens a paid-ability window before the phase moves**,
  and at the bottom rung the Corp's next decision arrives within the
  frame; a test that asserted `!awaiting` after pressing End turn saw
  it already true again. The applied count and the log line are the
  stable signals.

**Verified.** `cargo test --workspace` green (`netrunner_client` 79,
`netrunner_desktop` 23 in the library — three of them `layout`'s: the
face shrinks with the window and with ICE and grows on an empty board,
four rows at the computed width fit and one pixel more would not, a
row overlaps only when it must — and 8 in `tests/game.rs`: the
decisions at the mulligan, a hand card's sheet and its button, the bar
greyed then live then ending the turn, R&D and Archives and the stack
opening sheets with nothing applied, the gear's toggles saved to the
file and drawn), clippy silent, screenshots of the board with both aids
off and both on read back through the dev hooks. No engine change, so
no sweep and no coverage report.

### 4b. Server columns as tile stacks, and a run as a paced trail — DONE (15 September 2026)

`feat/server-tiles-and-run-trail`. Two things the person saw on the
board after #32: a card installed in a server's root was drawn as a
full face while the centrals were tiles and the ice was bars, so a
column was three kinds of thing and the one face on the board that was
not in a hand or a rig; and a run was invisible — an accent border on
the column, an outline on one bar, words on the rail — over in a frame
when the bot ran, with nothing to say what it had done to the Corp.

**Decisions taken, with the alternative rejected.**

- **A server column is a stack of tiles.** `spawn_tile` is the ice bar
  generalised: header, root cards and ice are the same block — title,
  strength or counters, faction-coloured border when rezzed, dim when
  not, `Card · 2 adv` for a face-down card the Runner cannot name
  (advancement is public) — each a click on its sheet, so the picture
  is read there and the column costs `layout::TILE` a piece
  (`Counts.pieces`, the tallest column's ice and root together, replaces
  `ice` and `roots`). The order was already the rule from #31 and is
  untouched: `column_top_down` and `ice_top_down` put the piece the
  Runner meets first nearest the middle of the screen from both chairs.
- **A run is a trail read off the entry's events, not a `Transition`.**
  One applied `ContinueRun` can pass an ice, approach the next and
  encounter it in one `PublicHistoryEntry`, which `diff` collapses to a
  single `RunMoved` naming only where the run ended up. So
  `netrunner_client::board::trail::RunTrail` (toolkit-free, beside
  `diff`) is built from the view's `active_run` (`begin`), fed the
  entry's events one at a time (`observe`: approach, encounter, each
  subroutine broken or fired, pass, bypass, the server, each access, the
  ending) and reconciled with the view once the action applied (`sync`),
  so it can be shown a step at a time and never disagree with the
  engine. It keeps a line of consequences in a few words each — the
  printed clause of a subroutine that fired, "stole Send a Message (3)",
  "2 net damage" — which is what the Corp wanted to see. Tested over six
  real games as both viewers: every step's install is in the view's run,
  the stage never runs past the ice, every ended run has an outcome, and
  the Corp sees a subroutine fire.
- **The pace is the client's, in `models::pace::Pacer`.** The screen's
  `poll` used to drain every message into the model at once; it now
  pushes them through the pacer, which splits an `Applied` in a run into
  a beat per step event and releases them `BEAT` (700 ms) apart, scaled
  by `DesktopPrefs::animation_speed` — the setting's first consumer; its
  documented 0 is instant, which the headless tests set. The person's
  own click answers at once; the bot's beats and every message during a
  run wait; a `Rejected` never does; outside a run nothing is paced. The
  message itself is the last beat, so the board moves only when the
  whole action has been shown. Tested with a fake clock over a real
  match as the Corp: step beats are spaced by the pause, messages
  outside a run are released at once, order is kept.
- **The run lane is a fifth board row, always reserved.** Between the
  servers and the rig from either chair — adjacent to the outermost ice
  — `layout::RUN_LANE` is counted in `rows_height` whether or not a run
  is on, so the cards keep their size when one begins; a lane that
  appeared with the run would have re-sized every card at the moment the
  person most wants to watch, and a floating overlay would have covered
  the middle. It draws the trail left to right — `Run on HQ`, a chip per
  ice with a dot per subroutine (hollow pending, accent broken, red
  fired; the dots stay on a passed ice), the server, `Accessing`, the
  outcome — and the last few consequences beneath. An ice chip is a
  click on the same sheet as its tile. The lane is refilled on a beat
  without the board (`relane`, `Dirty.lane`), and the trail lingers
  after the run until the next run or the next turn, so the Corp sees
  the whole run and what it did rather than a lane that emptied the
  instant it was over.
- **A tile says its rez state, and its sheet lists the card's facts.**
  The person's second look: a rezzable tile must say whether it is
  rezzed, tokens must show, and a click must give the state a person
  reasons from. `netrunner_client::board::facts` is the one place the
  words come from — `tile_label` for the tile (`Ansel 1.0 · unrezzed`,
  `Whitespace · rezzed · str 0`, `Superconducting Hub · 2/3 adv`, and
  from the chair that cannot name it `ICE · unrezzed` or `Card · face
  down · 4 adv`; an agenda is never "unrezzed") and `install_facts` for
  the sheet: where it sits and in what order the Runner meets it,
  rezzed or the rez cost, strength now and printed, each subroutine
  with its status in an encounter, tokens against the agenda's
  requirement, counters by kind, trash cost, what it hosts and what it
  is hosted on; a rig card gets its strength, counters and hosts the
  same way. The sheet is `install_sheet` for every install, known or
  hidden — a hidden one shows the back at the large size under
  "Face-down card" or "Unrezzed ice" with the facts the mask allows.
  Tested over real games as both viewers: every install on the board
  has a label with its state and facts starting with its place, a
  hidden card is never named, an ice met in a run says its strength
  now and its subroutines. `NETRUNNER_SHEET=1` opens the first Corp
  install's sheet for a screenshot. One glyph found: the bundled font
  has no `↳`, so a subroutine line uses the `»` the card text uses.
- **A line from the lane to the column was drawn and dropped.** The
  first cut placed an absolute 2 px node, rotated with `UiTransform`,
  from the lane's server chip to the column under run, off the laid-out
  nodes. The person's verdict on seeing it was that it was ugly and not
  needed; the column's border in the Runner's colour, which #31 already
  drew, says which server is under run. Gone the same day.

**Found on the way.**

- **The test helper's bool means greyed, not live.** `control_button`
  returns whether the bar's button is `Disabled`; the first lane test
  read it the other way and pressed nothing for two dozen rounds. Worth
  knowing before the next test that drives the bar.
- **A run on an unprotected server never encounters.** The dev hook that
  holds the pace for a screenshot first held only at an encounter, and
  the bot's runs on empty centrals never triggered it; it holds at the
  server's approach too.

**Dev hook.** `NETRUNNER_HOLD_RUN=1`: the pace stops at a run's first
encounter or server approach and the autoplay counts as done, so
`NETRUNNER_SCREENSHOT` catches the lane with a run in flight.

**Verified.** `cargo test --workspace` green (`netrunner_client` 82 —
two of them `trail`'s, one `facts`'; `netrunner_desktop` 28 in the
library — two `pace`'s, the `layout` invariants over five rows — and
12 in `tests/game.rs`, two new: a run from R&D's sheet fills the lane
with one chip per ice in the trail's order, a server column holds no
card face, and the trail stays with its outcome once the run is over;
a tile says its rez state and its sheet lists the facts), clippy
silent. Screenshotted on a 2560×1600 screen as both chairs forty
decisions in and held mid-run: tile stacks in one column with the ice,
the lane at `Run on Remote 0 › Palisade · encounter ○ › Remote 0` with
`Palisade rezzed` beneath from the Runner's chair, `Run on Archives ›
Tithe · passed ○○ › Archives` from the Corp's, and after a run the Corp
read `Run on R&D › R&D › Accessing › Successful` with `stole Send a
Message (3)`; no scroll area on the board. No engine change, so no
sweep and no coverage report.

### 4c. A card-selection prompt names its cards — DONE (15 September 2026)

`feat/selection-names-its-cards`. §3's noted item 1, raised again by the
person for both clients: a card that asks for cards offered `Toggle
selection of card N` — a position into a zone, which tells a human
nothing. What they asked for: `Select <card name>`, with the location
when there is more than one, then `Confirm` and `Select a different
card`. Both clients now read, for example, `Select Syailendra
(protecting Remote 1, outermost of 2)`, `Select unrezzed ice (protecting
Remote 2)`, `Confirm Marjanah` over `Select a different card`, and the
prompt says `Selected: Marjanah` or `Nothing selected yet`.

**Decisions taken, with the alternative rejected.**

- **The engine publishes the candidates, to the chooser only.** A label
  could resolve a position from the view for a hand, the heap, Archives
  and the board, but not for a search of R&D or the stack (Malapert,
  Mutual Favor, Poétrï, AU Co., Off the Books, Embedded Reporting),
  MuslihaT's top card or Touch-ups' revealed grip: `ClientView` keeps
  R&D and the stack count-only even for their owner. AGENTS.md §3 says
  what to do when the view lacks something — extend the masking layer
  with a rule about who may see it — so `ClientView::selection` lists one
  `SelectionCandidate { position, card, install }` per position the
  chooser can act on (the toggles `legal_actions` offers plus the cards
  already `selected`), built by `pending_choice::selection_positions`. It
  is empty for the opponent and a spectator. A card is shown because the
  card asking says so ("search", "look at", "reveal"), and only the
  positions the filter admits are listed, so a search publishes what it
  may take and not the deck. The one card a chooser still may not name,
  an opponent's unrezzed install (Tāo Salonga's swap), or a face-down
  Archives card for the Runner, is `None` on exactly the condition the
  masked board uses, read off `PublicGameState` rather than restated.
  `#[serde(default)]`, so an older view still reads. No bot reads it,
  and the observation and `ActionSpace` are unchanged. `determinize`
  could now reseat filter-matching cards at the prompt's positions
  (`reseat_selectable_cards` was deleted as unfixable without this), and
  that is left for a bot branch to measure.
- **Identical copies in one place are one button** (the person's call).
  A grip of two Sure Gambles offers `Select Sure Gamble` once, then
  `Select another Sure Gamble`; two chosen are one `Deselect Sure Gamble`.
  The alternative, a numbered button per copy, keeps every legal index on
  its own button and makes a person ask what the difference between copy
  1 and copy 2 is. So `netrunner_client::selection::Selection::hidden`
  names the positions a shown button stands for (same name, same place,
  same side of the selection). Both clients leave them out:
  `ActionMap::decisions` for the pop-up and `selection::shown` for the
  terminal's list. The play helper still lists every index. That changes
  §4a's rule that the decisions cover every index once, and the map's
  test now says so: a collapsed index must have a shown decision with
  the same words.
- **The place is shown only when it tells two cards apart.** `Select
  Hedge Fund` alone, `Select Ice Wall (protecting HQ)` beside another
  Ice Wall elsewhere, and a concealed install always by its place, since
  that is all that distinguishes it. The ice order uses `board::facts`'
  words.
- **`Select a different card` is the deselect when only one card may be
  chosen**; with room for several it is `Deselect <card>`. The confirm
  names what it confirms (`Confirm Hedge Fund and Sure Gamble (×2)`), or
  reads `Choose none` for an "up to" prompt left empty. The list is
  ordered cards to choose, then the confirm, then the way back
  (`Selection::rank`), so a full single choice reads `Confirm …`, `Select
  a different card`, in the person's order rather than the engine's.
- **The log says what a toggle did.** A log line is written against the
  view the action left, where a position just chosen is selected, so it
  reads `Selected Hedge Fund` / `Deselected Hedge Fund` rather than the
  button's words. The opponent reads `Selected a card`.

**Found on the way.**

- **The local terminal logged against the view the person chose from**,
  not the one the action left, unlike the server and the desktop. On
  that board a card just installed is not yet anywhere, so its log line
  fell back to `install #N`. `tui::log_last` and the lesson loop now pass
  the post-action view.
- **The comment on the old label was wrong.** It said the selection
  prompt drew the zone beside the list. Neither client ever did.

**Dev hook.** `NETRUNNER_HOLD_SELECTION=1`: the autoplay stops at the
first card-selection prompt the person is asked, so `NETRUNNER_SCREENSHOT`
catches the pop-up.

**Still to do.** The toggle on the card itself: a click on a hand card
or a tile during a selection, which `Target::Position` was reserved for
and nothing places yet.

**Verified.** `cargo test --workspace` green (1,469 tests: five new in
`netrunner_core`'s view tests; five in `netrunner_client`, one of them
over 24 random-vs-random games that checks 321 selection prompts as the
chooser reads them, with no button a position, every toggle a card or a
place, and no title the board conceals; one each in the terminal and the
desktop model), clippy silent. Both 256-seed sweeps green, with the fog
gate now checking at every step, for both seats and a spectator, that the
selection is the chooser's alone, names every position the chooser can
act on, and names no card the board masks. **The engine's flow did not
move:** `--headless --all-matchups --games 192 --corp random --runner
random --seed 1` gives a byte-identical JSON report on pinned `main` and
branch binaries. Screenshotted as the Corp with
`NETRUNNER_HOLD_SELECTION`, 2560×1600: Seamless Launch's pop-up reads
`Nothing selected yet` over `Select Whitespace`, `Select Ansel 1.0`,
`Select Palisade`, and no scroll area is the board.

### 4d. Where a card a card installs may go — DONE (15 September 2026)

`feat/where-to-install`, stacked on §4c. The person, as the Corp, saw the
Runner hit Scatter Field, chose a card from HQ for "You may install 1 card
from HQ", and could not tell where it could go. They already had a remote
with a rezzed asset making them credits, and it looked as if the only
choice was to install over it. They asked whether the rules allowed a new
remote, and for a way back.

**The rules, checked against the engine.** Scatter Field's subroutine is
optional (`min: 0`) and offers any installable card: ice (the filter's
`Ice: Barrier` is a placeholder; `CardType(Ice(_))` matches every
subtype), agenda, asset or upgrade, never an operation. It pays the
install cost, so ice pays 1 credit per piece already protecting the
server. `engine::corp_install_destinations` offers every existing remote
*and a new one* (centrals too for ice and upgrades, up to
`MAX_REMOTE_SERVERS`), and installing an agenda or asset into a remote
that already holds one trashes the one there (Rules Audit T5). All of it
is Null Signal Games' rule. **The engine was right and the clients hid
it.** The labels read `Choose Remote(0)` and `Choose Remote(1)`, with
nothing to say that Remote 1 did not exist yet or what Remote 0 would
lose. The desktop drew the choice on server columns only, and a server
that does not exist yet has no column, so the one free option was
reachable only from the play helper, which is off by default.

**Decisions taken, with the alternative rejected.**

- **Every destination says what it is and what it costs.**
  `netrunner_client::placement` reads the masked view: which card is
  going in (by its position in HQ or Archives; R&D's is not in the view,
  so it is "the card"), whether a remote is new, what an agenda or asset
  installed over would trash, and the ice tax (less Mercia B4LL4RD's
  discount, none when the card installs "ignoring all costs"). The
  labels: `Install in a new remote server`, `Install in Remote 0 —
  trashes Nico Campaign`, `Install protecting HQ — costs 1 credit`,
  `Install in the root of HQ` for an upgrade. The prompt:
  `Scatter Field: where to install PAD Campaign?` over `A new remote
  server is an option.` and, where it applies, the one-per-remote rule
  in a sentence. A server choice that starts a run reads `Run on HQ`.
- **A card effect's choice of server is under the prompt.**
  `ActionMap::decisions` now includes `ChooseServerForPendingDecision`,
  so the pop-up lists every destination. The server column still takes
  a click, but it is no longer the only way in.
- **No back-out to the card choice, the person's call.** Under the
  rules the card and its place are one act of installing, so a Back to
  the selection would be faithful. The engine cannot offer one: once the
  card is confirmed, the only legal actions are the servers. It would
  take a new `PlayerAction`, `ActionSpace` 1646 → 1647 and the exported
  policies with it. With the new remote always shown, installing over a
  card is a choice a person sees the price of, never a forced one, so
  the person chose words over the engine change.

**Found on the way.** Key Performance Indicators installs "ignoring all
costs", and the first cut of the prompt told the Corp ice would cost a
credit per piece; the detail now says the costs are ignored instead.

**Dev hooks.** `NETRUNNER_HOLD_INSTALL=1` stops the autoplay at the first
server choice a card's text installs into. `NETRUNNER_CORP_DECK=<id>` /
`NETRUNNER_RUNNER_DECK=<id>` replace the dev game's decks, because the
default Corp deck's one such card, Ansel 1.0, fires too rarely for an
autoplay to reach it.

**Verified.** `cargo test --workspace` green, clippy silent. New tests:
two in `placement` over installs the engine itself parks, from a real
selection confirmed through `apply_action` (an asset beside a rezzed
Nico Campaign is offered Remote 0 with what it trashes and a new remote
called new, ice names its tax, an upgrade trashes nothing, and the
terminal's pane title asks where); one over 32 random-vs-random games
that checks 25 card-effect installs as the Corp reads them (every choice
under the prompt and starting `Install`, none an engine name like
`Remote(1)`, every new remote called new, and at least one install-over
naming what it trashes); one in the desktop model (the pop-up's two
buttons and its heading). No engine file changed. Screenshotted as the
Corp on Fashion Lab (`NETRUNNER_CORP_DECK=fashion_lab
NETRUNNER_HOLD_INSTALL=1`, 2560×1600): Scatter Field fired on a run on
Archives, and the pop-up reads `Scatter Field: where to install Scatter
Field?` over `Install protecting HQ`, `… R&D`, `… Archives — costs 1
credit`, `… Remote 0 — costs 1 credit` and `Install protecting a new
remote server`; no scroll area is the board.

### 4e. A lone pass is taken without a click, in both clients — DONE (17 September 2026)

`feat/lone-pass-taken-without-a-click`. §3's item 2: passing priority
over and over is the whole of a run from the other chair, and a button
with no alternative asks the person nothing. Over one game from the
Runner's chair against the bottom rung (seed 3, the first legal action
taken each time) **96 of the 217 decisions were a lone `PassPriority`**,
every one a click for nothing.

**Decisions taken, with the alternative rejected.**

- **Only the lone pass, not any lone action.** §3's note floated taking
  every sole legal action. A lone `EndTurn`, a lone access decision or a
  lone mulligan answer is a moment worth seeing, and a pass beside
  anything else (a rez, an ability, a trace bid) is a real choice. So
  `netrunner_client::play::lone_pass` is `Some` exactly when
  `legal_actions` is one `PassPriority`, one predicate for both clients.
- **A client policy, not a rule, and not a `Seat`.** The engine still
  opens every window and still hears every pass; `Session` and
  `MatchHandle` are untouched, so the match thread, the sweeps and the
  RL path see the same actions they did. A `MatchHandle` that passed on
  its own would have hidden the pass from a screen that wants to show
  it.
- **No setting.** Nothing is lost by the pass being taken: the log line
  says it happened, and in a run the desktop's pacer already holds an
  `Awaiting` a beat before the model sees it, which is the "short delay
  so the board is seen to change" §3 asked for. A toggle can follow if
  a person asks for one.
- **Where each client takes it.** The terminal's `drive_local` submits
  the pass on the human's `Awaiting` without drawing a prompt, and logs
  it. The remote `App` answers a `StateUpdate` that lists only a pass
  (never while the connection is down; a spectator's view lists
  nothing). The desktop model returns `Outcome::Submit` from the
  `Awaiting` itself, with `awaiting` off so the bar stays greyed, and
  `poll` hands the pass to the match. The action map is still built, so
  a pass the engine rejects reopens the bar with the pass on it and
  cannot loop. **A lesson never takes it:** a step that teaches passing
  must be pressed.

**Verified.** `cargo test --workspace` green (1,500 tests), clippy
silent. New tests: `netrunner_client` plays that seed-3 game taking
every lone pass and checks each was the only action and the game ends;
the terminal's `App` sends a lone pass as it arrives, once, and does not
send a pass beside `EndTurn`; the desktop model submits a lone pass
without a click, only a lone one, offers nothing while it is in flight,
and puts the pass back on the bar when it is rejected. The desktop's
headless tests that press Pass during the Corp's turn still pass. No engine file changed,
so no sweep and no coverage report. No layout change, so no screenshot.

### 4f. A HUD: each side's numbers in fixed places — DONE (17 September 2026)

`feat/desktop-hud`. §3's item 6. The numbers were in the strip as
sentences — dim and small first, then body size — and "Credits 5 ·
Clicks 3 · Agenda points 2/7" is a line to read, not a place to glance.

**Decisions taken, with the alternative rejected.**

- **Every readout is always there, at zero too.** Credits, Clicks and
  Points in the same slots for both sides, the hand (HQ or Grip) in the
  fourth, then the side's own: Bad publicity for the Corp, Tags and
  Damage for the Runner. A Tags readout that appeared with the first tag
  would move the numbers after it at the moment the person needs them,
  so a live threat is drawn in the danger colour instead, and so is a
  point total two short of the win.
- **The words live in `netrunner_client::board::hud`**, beside `facts`,
  so the terminal can take the same set; its header line is left as it
  is for now.
- **In the strip, not a row of its own.** A HUD row would cost the board
  a row of card height; the grid (three columns, a heading-size number
  over a small word) replaces the four body lines inside the same
  `STRIP_CORP`/`STRIP_RUNNER` heights. The Runner's MU and Link, which
  a person looks up rather than watches, are one small line under it,
  and its Stack and Heap stay the pile buttons. **The Corp has no such
  line:** the first cut put "R&D 31 · Archives 2" there, and the person
  pointed out that both are on the play field already — each is a server
  column whose header carries its count — so it went.
- **"Agendas 5/7" is a button onto the score area** (asked for on the
  PR: "'Agendas' (click) -> open to show cards -> click card expand").
  The readout is named for what it opens rather than "Points", and
  `Pile::Agendas(side)` is the zone, beside the stack and heap. The sheet
  is a *list*, not the piles' spread of faces: a row per agenda (a small
  face, the title, its points), and a press opens that row in place —
  the large face, "Scored by the Corp · 2 points · advancement
  requirement 4", counters, the printed text, and for an agenda the
  viewer scored, its abilities by the handle it kept. One row open at a
  time (`Game::expanded`, `Intent::Expand`), closed on a second press and
  whenever a sheet opens; reading a card *over* the sheet, as the piles
  do, hid the rest of the list. The strip's "Stolen: …" line went with
  it — it was the one thing in a fixed-height strip that grew with the
  game, and the risk this entry's first cut flagged.

**Verified.** `cargo test --workspace` green (1,508 tests), clippy
silent. New tests: `netrunner_client` checks the fixed order for both
sides, that only Agendas opens anything, that a threat marks its slot
rather than adding one, the points alarm, and the score area's rows
(two copies are two rows, printed points, a stolen agenda has no
handle); the desktop model opens one row at a time and closes them on
reopen; the desktop's headless game checks each side has one HUD holding
exactly `hud::readouts`, and that a press on Agendas opens a row per
agenda, each row a press that opens its details and a second that
closes them. No engine file changed, so no
sweep and no coverage report. Screenshotted from both chairs forty
decisions in at 2560×1600 (the Runner opponent at 6/7 in red, its
stolen line wrapping): no hand is clipped and the only scroll area
logged is the hidden log's, at zero size. The score area screenshotted
with `NETRUNNER_AGENDAS=corp|runner` (opens it, first row expanded):
empty ("None yet · 0 of 7 points") and with two stolen agendas. The first
cut drew the disclosure triangle as a box — Noto Sans has neither
U+25B8/U+25BE nor U+2212 — so it is drawn in Noto Sans Symbols 2, which
covers U+25A0–2609, with a plus and an en dash beneath it.

**Seen, not caused here.** Two of eleven screenshot runs hung after
"Creating new window", before a first frame, and the same command
passed on a retry; both logged bevy's "Can't select current monitor"
warning, as every run did. Not investigated.

### 4g. A click opens a card's actions; a secondary click reads it — DONE (17 September 2026)

`feat/left-click-actions-right-click-reads`. The first item of the
person's second list (below, kept in their order). The primary click
opened the sheet — the card, its text and its actions — and the
secondary opened the same actions as a menu. The person asked for the
card-client convention: the everyday click acts, and reading is the
other button.

**Decisions taken, with the alternative rejected.**

- **The primary click opens the menu above the card** (§4a item 14's
  menu, unchanged in shape and anchor), a second click on the same card
  closes it, and a click on another card moves it. It still never
  submits. The two exceptions stay: a selection position submits its
  one entry, and the HUD's Agendas readout opens the score area, which
  is a door and not a card.
- **The secondary click (the right button, or Ctrl or Cmd with the
  primary) opens the sheet, and a sheet has no actions.** A hand card or
  an identity is the Large face alone: the scan, or the text layout,
  which carries the printed text, while no scan is cached. An install is
  its face beside its state (`board::facts::install_facts`), because the
  printed face cannot show whether it is rezzed or how many counters it
  holds. The person chose this over a strictly card-only reading. A zone
  is its contents. Keeping the actions on the sheet as well was
  rejected: two lists of the same thing is what the person asked to
  lose.
- **The score area keeps its buttons.** A scored agenda has no card on
  the board to click, so its abilities stay on its row.
- **Cmd joins Ctrl as the modifier.** On a Mac, winit already reports a
  two-finger tap or a Control-click as the right button. Cmd-click is
  there for a one-button mouse with a hand on the other key, and it
  means nothing else on the board.
- **`Sheet` lost its `entries`**, and with them the rebuild on every
  `Awaiting`. A sheet left open while the opponent acts has nothing to
  go stale.

**Verified.** `cargo test --workspace` green (1,508 tests), clippy
silent across the workspace. Rewritten tests: the desktop model opens a
menu on a click (and closes it on a second), never submits from it, and
opens an entry-less sheet on a secondary click that nothing opens
through, which the board moving keeps and a menu does not survive. The
headless game checks a press on a hand card opens its menu and the
menu's button submits; the right button and Ctrl+primary open the
sheet with no entry buttons; R&D's and the stack's menus offer the run
and the draw while their sheets show the contents; and a tile's facts
are reached by the right button. Screenshotted forty decisions in: the
Corp's menu over a hand card, and an install's sheet (Tithe beside its
state lines). The Runner's shot landed on the opponent's turn and
showed no sheet, which is the hook waiting as it should. The only
scroll area logged is the hidden log's, at zero size. No engine file
changed, so no sweep.

**Noted, to build next — the person's second list, kept in their order,
one PR each:**

1. **A click opens a card's actions; a secondary click reads it.**
   *Done in §4g.*
2. **Keyboard shortcuts for the common actions, and a list of them.**
   *Done in §4h, with one correction to the design below: Enter asks
   twice while clicks are left, because the engine lists `EndTurn` with
   clicks unspent, so the claim below that a stray Enter cannot throw a
   turn away was wrong. L is left for the phase bar's PR.*
   *Planned design:*
   - Space is the "go on" key: pass priority, or continue the run when
     that is what is offered. It never jacks out or ends the turn.
   - Enter ends the turn. The engine lists `EndTurn` only once the
     clicks are spent, so a stray Enter cannot throw a turn away.
   - C takes a credit, D draws, R removes a tag, P purges, J jacks out,
     A completes the run.
   - 1–9 press the decision pop-up's buttons in order.
   - I reads the hovered card and M opens its menu (the keyboard's two
     clicks).
   - Tab opens the score area, H toggles the play helper, L the phase
     bar (item 3), and ? or F1 shows the list.
   - Every key goes through `Intent::Control` or an existing intent, so
     the engine's list still decides what a key does. None acts while
     an overlay covers the board.
   - Rebinding waits until someone asks for it.
3. **A phase bar, toggled with L and from the gear menu, and
   remembered.**
   *Done in §4j.*
   - One row for the turn: the Corp's draw › actions (clicks left) ›
     discard, or the Runner's actions › discard.
   - During a run, a second segment: initiation › approach ice N ›
     encounter › access › outcome.
   - A marker for an open paid-ability window: who holds priority, at
     which checkpoint.
   - It reads `phase`, `active_run` and `paid_ability_window`. The words
     live in `netrunner_client::board`, beside `hud`, so the terminal
     can take them.
   - It takes a fixed row out of `face_width`'s budget, so the board
     still never scrolls.
4. **Reorder one's own hand by dragging a card along it.**
   *Done in §4k.*
   - The order is a client-side permutation per match, kept by `CardId`
     and reconciled with each view: a drawn card joins the end, and a
     card that left drops out. The engine and `ClientView` never see
     it.
   - This brings in the drag machinery (bevy_picking's `Pointer<Drag*>`
     observers), with a few pixels of travel before a press counts as a
     drag, so a still click is still a click.
5. **Drag a card to play it, with the places it may go lit.**
   *Done in §4l.*
   - Picking a card up lights the destinations its legal entries name,
     through `ActionMap::for_hand_card` and each entry's targets: a
     server, a new-remote column that appears only while dragging, or
     an ice host.
   - A card with no destination (an operation, an event, most Runner
     installs) lights the play area as a whole.
   - A drop on a place with one entry submits it. A drop on a place
     with several opens the menu of just those, and a drop anywhere
     else puts the card back.
   - This changes AGENTS.md §5 from "a click never submits" to "a click
     never submits; a completed drag onto a lit place does", and that
     sentence changes with the PR, not before.

### 4h. Keys for the buttons, and a list of them — DONE (17 September 2026)

`feat/board-keyboard-shortcuts`. Item 2 of the person's second list
(under §4g).

**Decisions taken, with the alternative rejected.**

- **A key presses a button that is already there.** Every shortcut goes
  through the intent its button raises (`Intent::Control`, `Choose`,
  `Click`, `Inspect`), so the engine's `legal_actions` still decides
  whether it does anything. A key that means nothing now does nothing,
  and no key acts under an overlay. The map and the list the person
  reads live in one Bevy-free module (`models::shortcuts`), and a test
  holds the letters and the list to each other.
- **C and D spend the click at once, as the bar's buttons do.** The
  person decided this after being asked about Shift or a confirmation.
- **Enter asks twice while clicks are left.** The recorded design said
  the engine lists `EndTurn` only once the clicks are spent. It does
  not (`legal_actions.rs` tests it with clicks left, correctly, since a
  player may end early), so one stray Enter would have thrown them
  away. The first Enter puts "N clicks left — press Enter again to end
  the turn" on the rail. The second ends the turn, and any other intent
  in between stands the first down. With no clicks left, one Enter ends
  the turn. The bar's End turn button is unchanged, because a pointer
  aimed at it is not a stray.
- **Space is the "go on" key:** pass priority if listed, else continue
  the run. It never jacks out or ends the turn (the person's choice).
- **1–9 press the open menu's buttons, else the pop-up's,** in drawn
  order (`ActionMap::decisions`). The buttons are not numbered on
  screen yet: numbering them changes every label a test finds a button
  by, and it is worth doing when a person asks for it.
- **Letters are read by what they type** (the logical key off
  `KeyboardInput`), so C is the key marked C on any layout. A key held
  with Ctrl, Cmd or Alt is left to the system, so Cmd-Q and Ctrl-C pass
  through.
- **M and I act on the hovered card** as its click and secondary click
  would. H turns the play helper on or off and saves it, as the options
  row does. Tab opens the person's score area and Shift-Tab the
  opponent's. `?` or F1 opens the list, which the same key or Escape
  closes.
- **L waits for the phase bar** (item 3), so the list never names a key
  that does nothing.

**Verified.** `cargo test --workspace` green (1,512 tests), clippy
silent across the workspace. New tests:
- The shortcut map: every listed letter is a key and no other letter
  is; Space, Enter, the digits, Tab with and without Shift, and `?`.
- The model: at the mulligan Space and C do nothing and 1 keeps; the
  list covers the board, and its key and Escape close it; C takes the
  credit; a digit presses the open menu's button; Enter arms, fires on
  the second press, and stands down on anything between; with no clicks
  left one Enter ends the turn; Tab opens a score area, under which no
  key acts.
- The headless game, through real keyboard messages: 1 keeps, C gains
  a credit, Ctrl-C sends nothing, `?` draws a row per listed key and
  nothing acts under it, M and I open the hovered card's menu and sheet,
  H toggles and saves the helper, and Enter puts the notice on the rail
  and the second Enter ends the turn.

Screenshotted with a new dev hook, `NETRUNNER_KEYS=1`, over a Corp board
forty decisions in. The first cut left blank lines under a wrapped row
and "? or F1", so the key column no longer wraps and the long line was
shortened. No engine file changed, so no sweep.

### 4i. The servers keep one order from both chairs, and the Corp HUD drops HQ — DONE (17 September 2026)

`feat/one-server-order-both-chairs`. The person asked for the Runner's
seat, in both clients, to read the servers as the Corp does:
`[Archives] [R&D] [HQ]`, then the remotes. They also pointed out that the
"HQ" readout on the Corp's HUD repeats the HQ column header's count.

**Decisions taken, with the alternative rejected.**

- **One left-to-right order, not the table mirrored.** §4a item 10 had
  the Runner see the table as it lies across from them: remotes, HQ,
  R&D, Archives. That is how the cards sit, but each remote the Corp
  made pushed the three centrals a column to the right, so the servers a
  Runner hits most moved all game. Now Archives, R&D, HQ, then the
  remotes by number, from either chair. The vertical rule is unchanged:
  the ice still comes down toward the Runner, outermost nearest.
- **The rule moved to `netrunner_client`** as `board::table_servers`,
  and the TUI uses it too. It also carries the "every central, even
  empty" rule the desktop had kept to itself.
  `layout::servers_left_to_right` is gone.
- **The TUI lists the three centrals always, each with its count:**
  `Archives (3): …`, `R&D (32): (empty)`, `HQ (4): …`, then the remotes.
  The count line above them went. Before, the lines followed the
  engine's `ServerView` order (HQ first) and listed a server only once
  something was installed on it, so an install on an empty central moved
  every line below it. The engine's order is a grouping key and is left
  alone.
- **The Corp HUD is Credits, Clicks, Agendas and Bad pub.** HQ's count
  is on its column header, the same reason §4f dropped the R&D and
  Archives line. The Runner's Grip stays, because the grip has no
  column. The shared first three slots are unchanged.

**Verified.** `cargo test --workspace` green (1,513 tests), clippy
silent across the workspace. Tests:
- `table_servers` keeps the centrals first and present on an empty
  table, and sorts remotes by number whatever order they came in.
- The desktop's headless game sees `[Archives, R&D, HQ]` from both
  chairs.
- The TUI render test finds Archives, R&D, HQ in that order with counts
  from both seats, and no old count line.
- The HUD test expects the Corp's four readouts.

Screenshotted sixty decisions in from both chairs. The Runner's board
reads Archives · 3, R&D · 32, HQ · 3, Remote 0, Remote 1, and the Corp's
opponent strip has no HQ readout. The Corp's board has five remotes
beside the same three centrals. The only scroll area logged is the
hidden log's, at zero size. No engine file changed, so no sweep.

### 4j. A phase bar: where the turn is, and where a run is — DONE (17 September 2026)

*Superseded in part by §4z: the bar is now a panel in the right column,
not a row of the board, and turning it off moves no card. Its words and
marks are unchanged.*

`feat/desktop-phase-bar`. Item 3 of the person's second list. The status
line named the phase the engine was in ("Turn 12 · Runner's turn") but
not what came before or after it, and a run's step was only in the
prompt's sentence.

**Decisions taken, with the alternative rejected.**

- **The words live in `netrunner_client::board::phase`**, beside `hud`
  and `facts`, read off the masked view's `phase`, `turn`, `active_run`
  and `paid_ability_window`. The terminal client can take the same set;
  its `run_phase_strip` is left as it is for now.
- **Every step is always listed, and the one in play is marked** — the
  HUD's rule, for its reason: a bar whose steps appeared as they were
  reached would be a different bar every time it was read. A turn is
  Draw (the Runner's "Turn begins", which has no mandatory draw),
  Actions with the clicks left on it, and Discard with how many must go.
- **A run is a second segment, not more steps on the turn's.** A run
  happens inside an action, and one flat row would have said the turn
  had left its actions behind. The run's four steps are initiation,
  approach, encounter, access, with the ice counted in the approach and
  encounter ("Approach ice 2 of 3"); an ended run keeps a fifth, "Run
  over", while its trail is still on the board. The engine's finer
  moments — the rez window, a subroutine resolving, passing the piece —
  are actions inside approach and encounter, not steps, or the bar would
  have been a different shape on every ice.
- **A paid-ability window is a line under the steps**, naming who holds
  priority and which window it is, and nothing when none is open: who
  may act in a window is the one thing a person cannot read off the
  board, and a line that was always there would say nothing four turns
  in five.
- **It is a row of the board, not a bar of the window.** `rows_height`
  charges `PHASE_BAR` when it is on, so the cards shrink by its height
  and the board still never scrolls; turning it off gives the cards the
  row back (the test asserts the difference is exactly the row and its
  gap). It sits below the person's hand and above the control bar:
  where the game is belongs beside what the person may do about it, and
  a row at the top would have sat among the opponent's cards.
- **On by default, off with L or the gear.** `DesktopPrefs::phase_bar`
  is saved like the play helper's, and L is the key §4h left for it.

**Verified.** `cargo test --workspace` green (1,517 tests), clippy
silent. New tests: the turn's three steps and their marks, the clicks
left and the discard count, the Runner's first step, the mulligan and
game-over segments, the run's steps with the ice counted and its ended
fifth, the window's line; the layout charging exactly one row for the
bar; and the headless board, which reads the marks at the mulligan and
on the Runner's turn, turns the bar off with L (the row goes, the face
width does not shrink, the setting is saved), back on, and finds the run
segment once a run is on. Screenshotted from the Runner's chair holding
at a run's initiation (`NETRUNNER_HOLD_RUN=1`) and from the Corp's: the
bar reads "Runner turn 6 · Turn begins · Actions · 3 clicks left ·
Discard · Run on HQ · Initiation …", the hand is not clipped, and the
only scroll area logged is the hidden log's. No engine file changed, so
no sweep.

### 4k. A hand card is dragged into place — DONE (17 September 2026)

`feat/drag-to-reorder-the-hand`, stacked on §4j. Item 4 of the person's
second list, and the machinery item 5 (dragging a card to play it) needs.

**Decisions taken, with the alternative rejected.**

- **A hand card's press is armed, not acted on** (`models::drag`). Every
  other target still opens its menu on the press; a hand card waits for
  the release, which is the click when the pointer never travelled and a
  drag when it did. Opening the menu on the press and closing it again
  once the pointer moved was the alternative, and it flashed a panel over
  the board on every drag.
- **Travel starts a drag, not time** (`drag::THRESHOLD`, six pixels): a
  person holding still is clicking however long they hold, and one who
  has moved six pixels is dragging however quickly. A timer would have
  made a slow click a drag and a fast drag a click.
- **The order is the client's** (`models::game::HandOrder`). The rules
  give a hand no order — it is a multiset — so what the view hands over
  is the order cards were drawn in, and a person sorting their hand is
  doing something the rules allow and the view cannot record. It is kept
  by card id (two copies are interchangeable, so nothing per-copy would
  buy anything), reconciled with every view — a drawn card joins the
  end, a played one drops out — and never leaves the client.
- **The drag state lives in the model**, not in a resource: `redraw` is
  already at Bevy's sixteen system parameters, and the press-travel-
  release logic is worth testing without a window either way.
- **The pointer is read from `CursorMoved`**, not the window, so a
  headless test can drive a drag; `GamePlugin` registers the message,
  which `WindowPlugin` normally owns and the tests have no window for.
- **The row does not re-flow under the pointer.** The card being dragged
  is outlined where it sits; a hand that re-ordered itself mid-drag moved
  the gap the person was aiming at.

**Verified.** `cargo test --workspace` green (1,522 tests), clippy
silent. New tests: the press/travel/release machine and the drop's insert
index; the hand order across draws, plays and two copies of a card; the
model's drag, which opens the menu when still and reorders when moved and
submits nothing either way; and the headless board, where a real press,
pointer move and release reorders the hand, plays nothing, opens no menu
and leaves the view's own order alone, while a still press still opens
the menu. Screenshotted the Corp's board forty decisions in: unchanged,
and the only scroll area is the hidden log's. No engine file changed, so
no sweep.

### 4l. A card dragged onto a lit place is played there — DONE (17 September 2026)

`feat/drag-a-card-to-play-it`, stacked on §4k. Item 5 of the person's
second list, and the last of it.

**Decisions taken, with the alternative rejected.**

- **A drag is the one gesture on the board that submits.** A click still
  never does — a click is what a person does to look — but a drag is
  deliberate in a way a click is not: the card was picked up, carried to
  a place that lit up, and let go. AGENTS.md §5 says so now.
- **The places come from the card's own entries**
  (`ActionMap::destinations_for_hand_card`), so the engine's
  `legal_actions` decides what lights, and a card that is played rather
  than placed — an operation, an event, a Runner's own install — lights
  nothing and is still played from its menu.
- **A remote the Corp has not made yet gets a column for the length of
  the drag.** The engine offers the install and the board had nothing to
  drop on; the column is headed "New remote 0", named for what dropping
  there would make. §4d's pop-up is still how a *card's text* asks the
  same question, because that one is a prompt rather than a place.
- **A place that offers two ways opens a menu of just those**
  (`ActionMap::for_hand_card_at`): an agenda dropped on a remote that
  already holds something can be installed over it or not, and a drop
  says *where*, not *which*.
- **The drop is hit-tested against the laid-out boxes**, innermost
  first, so an ice tile takes the drop rather than the column under it.
  Bevy's focus system does not follow a pointer with a button held, so
  the hovered node is not the thing to ask.
- **A drop anywhere else puts the card back** — on the hand row it is
  §4k's reorder, and elsewhere nothing happens.

**Verified.** `cargo test --workspace` green (1,524 tests), clippy
silent. New tests: `destinations_for_hand_card` and `for_hand_card_at`
over a real game (an install names its server, an operation names
nowhere), and the model's drop — nothing is lit until the press becomes a
drag, one entry submits, a place with two asks with a menu, and a drop on
a place the card does not name puts it back. Screenshotted with a new
`NETRUNNER_DRAG=1` hook, which picks up the first card that has somewhere
to go: an unrezzed Whitespace held over the Corp's board lights Archives,
R&D and HQ and adds the "New remote 0" column. No engine file changed, so
no sweep.

**With this, the person's second list is done** (§4g–§4l).

**Noted, to build next — the person's third list, kept in their order,
one PR each:**

The first two lists were about what the board *does*; this one is about
what it looks like. It came from three MTG Arena screenshots and one
sentence: the board is correct and boring — flat, evenly lit, a stack of
rows rather than a table seen from a chair. It also finally closes §3's
item 4, "the layout does not use the play field", the last item of the
first list still open. **Half of that item is dead and is not being
reinstated:** §4i settled that the servers keep one order from both
chairs, so "the opponent's side mirrored across the top" is gone; "the
servers spread, the hand fanned along the bottom" is what this list
delivers.

**Two decisions taken before the list, with the alternative rejected.**

- **No 3D, and the first reason given for that was wrong.** The claim
  was that a perspective camera means hundreds of crates. It does not:
  `netrunner_desktop` builds 57 bevy crates today and the `ui` feature
  already pulls the whole render stack — `bevy_render`,
  `bevy_core_pipeline`, `bevy_camera`, `bevy_mesh`, `bevy_material`,
  `bevy_shader`, `bevy_light`, `bevy_sprite`, `bevy_sprite_render`,
  `bevy_text`, `bevy_picking`, wgpu. Everything locked but unbuilt is
  nine crates, of which five are 3D, and a textured quad needs at most
  one of them. **Real perspective costs zero to one new crate.** (The
  Cargo.toml comment "Nothing 3D — no PBR, glTF or lighting" is already
  half untrue: `bevy_light` and `bevy_material` compile today. Correct
  it when something next touches that file.) The real cost is about
  1,000–1,500 lines of churn: the board proper is 576 draw lines across
  26 functions, not the file's 2,277; `models/` is 2,384 lines with no
  `bevy` reference at all and would not change; `anchor_of` is six lines
  and is the whole bridge from `bevy_ui` geometry to the neutral
  `Anchor`; an image card face is already one `ImageNode`, and Corp
  installs are `TILE` text blocks rather than faces, so the board has
  only four face spawn sites. The bill is the tests — 16 of the 17 in
  `tests/game.rs` touch `bevy_ui` through seven helpers called about 78
  times — and one of them, `clicks_in_tree_order`, *defines* server
  order as depth-first `Children` order, which is what §4i's rule stands
  on; in a 3D board that becomes x-position and has to be re-conceived
  rather than ported. Also worth knowing before anyone tries it:
  `bevy_sprite`'s picking backend matches `Projection::Orthographic` and
  returns `None` otherwise, so a perspective camera silently makes every
  card unclickable. **Deferred, not refused.** A painted field and
  shadows get the still frame; what 3D buys is motion in perspective —
  a card tilting as it is played, flipping on rez, the camera pushing in
  on a run. Revisit it when a run should feel dramatic, not to make a
  static screenshot less flat.
- **The depth is painted, not computed.** Every card on the board stays
  one size; the perspective lives in the table's art. The rejected
  alternative was a per-row face-width ramp, and it is rejected on more
  than taste: the natural "depth only on a big screen" form,
  `ratio = 1 − (1 − d)·t` with `t` rising across `MIN_FACE..MAX_FACE`,
  **is not monotone in the face width** — its derivative,
  `1 − (1 − d)(2·base − m)/(M − m)`, is negative at `base = M = 220`
  for any depth below about 0.60. `face_width`'s binary search is
  licensed *only* by `rows_height` being monotone, so that ramp would
  have returned a silently wrong width with every existing test still
  green. A safe form exists — an affine ramp,
  `MIN_FACE + (base − MIN_FACE) · DEPTH[row]`, whose derivative is
  `DEPTH[row] > 0` — and is written down here so it is not rediscovered
  from scratch if the ramp is ever wanted. It is not wanted now, and not
  having it means `rows_height` and `face_width` keep their exact
  present meaning. The known objection to uniform cards on a painted
  perspective is that they read as stickers on a photograph; the answers
  are a *shallow* painted perspective and a contact shadow under every
  card, which is why item 1 is two PRs rather than one.

1. **The play area has depth.** Closer objects larger, further ones
   smaller, the way a person sees a table. Two PRs: the painted field
   first, then the shadows and tint that sit the cards on it. Note the
   order of *building* differs from the order of this list — the table
   is item 6 below and is the first PR, because everything else reads
   against it and because it is what unblocks the person making art.
   *Done: §4m gave the field its perspective and §4q put the cards on
   it. The shadows came deliberately after the skin (§4n), because art
   with its own baked shadow would double up with a drawn one — which is
   why a skin can turn them off.*
2. **Each player has an avatar and a bar of quick data**, the active
   player's bar lit and the inactive one's muted grey. The avatar is the
   identity card's art cropped to a disc — the Netrunner-native answer,
   and `card_images` already fetches fronts; it falls back to the
   faction mark in the icon font, then to a faction-coloured disc, which
   is the three-tier rule again. The numbers are already
   `board::hud::readouts`, so this adds no data, only a shape.
3. **The grip arcs, and a hovered card lifts out of it.** Rotation only,
   no resizing. *The person's own constraint, worth keeping as a rule:*
   the lift is local to whoever is looking — it never enters a
   `PlayerAction`, the log or a `ClientView`, because considering a card
   is not a game event.
4. **The space is used**, including a turn indicator in the bottom
   right.
5. **The right side carries the card being read**, and gives its width
   back to the board when it is empty. The sheet rule holds: a reading
   surface carries no actions, so there is still one list of what a card
   can do, in the menu its click opens.
6. **The field of play has a background**, and there are several to pick
   between or have chosen at random. A table is a small folder — a JPEG
   ground painted in shallow perspective, an optional PNG overlay with
   alpha, and a `table.json` naming its accent colour — under the three
   asset tiers, with the person's own tables in the override directory.
   JSON rather than TOML because `toml` is not a workspace dependency
   and `deny.toml` is an allow-list; SVG is not an option at all,
   because Bevy has no rasteriser for it and `resvg` is a dependency for
   nothing. A `NETRUNNER_TABLE_GUIDE=1` hook draws the row bands over
   the current table, so a field can be painted to the real layout
   rather than guessed at.
   *Done in §4m.*
7. **The board's furniture is drawn rather than outlined.** Added on 17
   September 2026, after the person looked at §4m's board and named what
   was still flat: *"Archives, R&D, HQ, Remote Servers — are all just
   line drawn tiles."* They asked whether icons and "bubbles, with and
   without highlights" could be supplied as art with text placed on them
   dynamically. They can: `bevy_ui` has nine-slice with its own render
   pipeline, so a small picture stretches to any width with its corners
   intact and the text sits on top as an ordinary child.
   *Done in §4n.*

After this list: the transitions the board already computes turned into
movement and sound, which is what §4 has owed since §3. It comes after
rather than before, because tweens animate cards between positions and
this list moves every position.

---

### 4m. A table under the board, and the depth is painted on it — DONE (17 September 2026)

`feat/desktop-table` (#61). Item 6 of the person's third list, built
first because everything else reads against it and because the
scaffolding is what unblocks somebody making art. The board was a flat
dark ground; it now has a field, and that field is where the perspective
lives.

**Decisions taken, with the alternative rejected.**

- **A per-row face-width ramp is not used, and not on taste.**
  `face_width` binary-searches `rows_height`, which is licensed *only* by
  that function being monotone in the face width, and the obvious "depth
  only on a big screen" ramp is not monotone: the derivative of
  `base·(1 − (1 − d)(base − m)/(M − m))` is negative at `base = M = 220`
  for any depth below about 0.60. It would have returned a silently
  wrong width with **every existing test still green**. A safe form
  exists — affine, `MIN_FACE + (base − MIN_FACE)·DEPTH[row]`, derivative
  `DEPTH[row] > 0` — and is written down against the day the ramp is
  wanted. Today `rows_height`, `face_width` and `step` are untouched and
  the no-scroll invariant keeps its exact meaning.
- **A table is a folder, not a bare image**, so it can carry more than
  pixels: `base.jpg` (JPEG because a painted ground is photographic and
  has no alpha — about a megabyte at 2560×1600 where the PNG is six), an
  optional `overlay.png` (PNG because *that* one does need alpha), and an
  optional `table.json`. **SVG is refused outright**: Bevy has no
  rasteriser and adding one for a backdrop is a dependency for nothing.
- **The manifest lives in `netrunner_client::table`**, not the desktop
  crate: it is data rather than pixels, and testable without a window.
  Optional at every level — a folder holding only a `base.jpg` is a table
  named after itself, and a manifest with a typo costs the table its
  colours and never its picture. Being made to write JSON before seeing
  your own art is a reason not to bother.
- **The painted tier draws a real receding grid** rather than a flat
  wash: screen height maps to depth as `1/(v − horizon)`, the standard
  ground-plane mapping, so the default ground demonstrates the intent
  with no files at all. Lines fade with depth, because evenly-spaced
  lines converge faster than the pixels can hold and the result is moiré
  rather than distance.
- **`Table` is a bare string in the settings file** — `"table":
  "neon-alley"`, not a tagged enum — so a hand-edited file reads. That
  costs two reserved names, `painted` and `random`, and a folder by
  either is filtered out of the choices rather than offered and
  unselectable. An unknown name is a folder rather than an error, and a
  named table whose folder has gone falls back to the **painted ground,
  never to some other picture**: somebody who named a table meant that
  one.
- **Random draws once per match**, from the wall clock rather than the
  game's seed — seeding it from the deal would change the picture
  whenever a seed was replayed to look at a bug.

**Two things found by building it.** The `NETRUNNER_TABLE_GUIDE=1` hook
parented each band to the board and then read the board's children the
next frame, so it drew a band for every band — growing by a row a frame
(6, 12, 18 …) and presenting as a despawn that was not working. The bands
now hang off one container the loop skips, and they are outlined rather
than filled, because the point of the guide is to see the field *under*
the rows. And `widgets::dim` already carries a `TextColor`, so a second
one in the same bundle is Bevy's duplicate-component panic.

**Verified.** `cargo test --workspace` green (39 test binaries), clippy
silent. New tests: the settings round trip as a bare name, an unknown one
landing on a folder, the default still skipped from an untouched block;
the manifest read, defaulted and surviving a typo; accents in the
spellings a person would write; the painted ground's size and its
near > far > beyond ordering, which is the depth cue itself; a choice
resolving to a folder, to the painted ground when it is gone, and never
panicking on an empty list; the row cycling both ways, wrapping, offering
random only with something to shuffle, and restarting from a table since
deleted. Screenshotted from the Runner's chair forty decisions in, with
and without the guide: the grid converges on a vanishing point at the
centre, the six rows land where the guide says, and the only scroll area
logged is the hidden log's at zero size. No engine file changed, so no
sweep.

---

### 4n. The board's furniture is drawn instead of outlined, and a skin never changes a size — DONE (17 September 2026)

`feat/desktop-skin` (#62), stacked on §4m because a table may name the
skin it was drawn for. Item 7, and the item the third list grew for.

**Decisions taken, with the alternative rejected.**

- **A skin changes how a box is painted, never how big it is.** This is
  the rule the design serves rather than a nicety: `face_width` budgets
  the window to the pixel so the board never scrolls, and
  `spawn_actions_menu` places itself from the hard-coded height of a
  button. So `Dressing::apply` writes colours and an `ImageNode` and
  **never touches `Node`** — a dressed node keeps its border *width*,
  because width is layout, and has its border coloured away instead; the
  corner radius stays the caller's, because `border_radius` is a field of
  `Node` rather than a component of its own. Measured rather than
  asserted: the control bar's eight buttons are 180, 174, 191, 140, 178,
  188, 131, 193 skinned and undressed alike.
- **Two levels of fallback**, because a skin nobody has finished has to
  be worth starting: an undrawn slot is drawn as before, and an undrawn
  *state* borrows its base slot's picture. One `tile.png` dresses a
  rezzed tile and an unrezzed one.
- **The caller says what it would have drawn.** A tile's border is its
  card's faction colour and a column's is the accent when it welcomes a
  drag; those are runtime facts the skin module has no business knowing.
  `Skin::dress` takes the `Drawn` the call site already had and either
  overrides it or hands it back. The tint `"state"` then means *the
  colour the board would have used*, so one white picture serves every
  faction and every alarm.
- **The picture is applied by a system, not by the bundle.** A
  `widgets::` bundle is a plain function of the theme and cannot reach a
  resource; threading a `Skin` into `button`, `compact_button` and the
  rest would have touched every call site in the crate to say something
  none of them care about. They mark themselves `Dressed` and `dress`
  paints them.
- **Server marks are deliberately absent**, and they are the first thing
  anyone asks for. A mark needs a box of its own in the header; a header
  with one is wider; a wider header widens its column — which is a
  *layout* change, and not a skin's to make. Reserving that box for every
  server whether or not anybody has drawn a mark is the way to do it, and
  it belongs with the panels and the icons.

**Two things found by building it.** Registering `dress` at the head of
`WidgetsPlugin`'s existing chain moved `button_feedback` relative to every
screen's `controls`, and twelve board tests stopped seeing their presses;
dressing has no ordering requirement of its own, so it is registered
separately, with the reason in the code. And `button_feedback` resets to
`theme.button` on `Interaction::None`, so any `Themed` node resting on
another colour loses it after one hover — the drop-down's selected item
is the live case. Recorded where the code is rather than silently worked
around; the fix is for it to carry a `Dressed` of its own.

**A correction to both authoring guides, from measuring rather than
deriving.** The slot sizes were worked out from the layout constants and
two were wrong: a button is 44 logical pixels tall rather than 40,
because a text line is taller than its font size, and a server header is
31 rather than 36 — 36 is what the *fit* budgets for that row, not what
the button comes out at. Measuring also turned up what neither guide
mentioned: there are two pixel scales, and this machine runs a 1.25
display scale, so a box the layout calls 44 tall is 55 real pixels and a
field over a 2048×1280 logical window is 2560×1600 of picture. Both
guides now say to draw at 2×, and the tables guide says what it should
have said first — the field is stretched and never cropped, so the aspect
ratio is the part that matters.

**Verified.** `cargo test --workspace` green (39 test binaries), clippy
silent. New tests: the manifest read, defaulted, surviving a typo and
keeping a key from a later version; every slot's key unique and every
base terminating in one step; an empty skin handing back the caller's own
colours for all 33 slots; a state borrowing its base's picture; tint
precedence — state over base over plain — and not leaking back; insets
becoming a nine-slice and an icon keeping its aspect; `"state"`
recognised rather than parsed as a failed colour; `Auto` following the
table, `Drawn` refusing one it offers, and a named skin never falling
through to another. Proved end to end with a throwaway skin — a 32×32
plate with a 3px rim and corner notches dressed the columns, headers,
tiles, buttons, chips and HUD cells at eight different widths with no
smearing — then deleted; no art is committed. No engine file changed, so
no sweep.

**Open, for the rest of the list:** the contact shadows that sit the
cards on the field (item 1's other half), the panels, menus, sheets and
icons, a server's own mark, the fanned hand, the hovered card, the player
bars and the right-hand side.

### 4o. An access shows the card, not its name, in both clients — DONE (17 September 2026)

`feat/access-shows-the-card` (#64). The person asked for it plainly:
"when a player is allowed to access a card, instead of just displaying
the name of the card, display the card so the human knows what the card
is — and then the action(s) they can take after accessing the card."

They were right about how little was there. The desktop pop-up printed
`Accessing Send a Message` and, under it, at most `Trash cost 3`. The
terminal printed *nothing at all* about the access: it is not a
`PendingDecision`, so `prose::decision_prompt` returned `None` and the
actions pane kept its default title, leaving three labels — `Steal Send a
Message`, `Trash Send a Message`, `Pass on Send a Message` — and no card
on screen anywhere.

**Decisions taken, with the alternative rejected.**

- **No engine change, because the card was already on the wire.**
  `PublicAccessPhase::PendingChoice` has carried the accessed card's
  `CardId` all along, masked by `mask_run_state`'s
  `viewer.is(Side::Runner) || run.server == ServerId::Archives`. The new
  `netrunner_client::access` module reads the view and looks the card up
  in the registry every client already holds. Nothing was added to
  `ClientView`, and no `netrunner_core` file changed.
- **The pop-up carries the card and its actions, which is the second
  exception to "a reading surface carries no actions."** §4g settled that
  a sheet is read-only and a card's actions live on the menu its tile's
  click opens; the score area was the one exception, because a scored
  agenda is off the board. An accessed card is the same shape of problem
  and worse: it is in HQ or R&D, face down, and the *prompt is the only
  place it exists*. The alternative was opening the existing sheet
  beside the pop-up, which puts one card in two panels and still leaves
  the person reading a name in whichever one has the buttons. Recorded
  on `spawn_decision_popup` and in `netrunner_desktop/src/lib.rs`.
- **The terminal's panel covers the board region only, and carries no
  buttons.** It is deliberately not a `Modal`, which owns the keyboard
  until dismissed: the actions pane below it keeps Up/Down/Enter and
  already lists the three choices. Listing them inside the panel as well
  was the alternative and would have meant either a second key model for
  one prompt or two lists that could disagree. The panel is sized to its
  *wrapped* rows rather than its line count — counting lines clipped the
  facts off the bottom of a card whose text is a paragraph.
- **Only the side being asked is shown the card.** The mask also names an
  Archives card to the Corp, so a Corp-side modal was possible; it was
  rejected because the modal exists to help someone *decide*, and the
  Corp has no decision at a `PendingChoice`. The one access the Corp can
  be asked — a `PendingInteractiveTrigger` whose `decider` is the Corp —
  does show them the card. Screenshotting the Corp chair sixty decisions
  in confirms it: the pop-up there is an ordinary selection prompt with
  no face.
- **`SelectNextCard` gets no face.** Choosing which of several cards to
  access is a choice *between* cards rather than a decision about one,
  and its buttons already name each candidate. A face per candidate is
  the third list's item 5, not this.
- **`Prompt::of` defers to `access::Access` for an access.** Found by
  looking at the first screenshot: the rail read `An agenda: it must be
  stolen` while the pop-up two inches away read `An agenda — it must be
  stolen`. One set of words now, two separators — `·` for the rail's one
  line, newlines for the panel's stack.

**Three copies of the numbers line became one.** `Face::numbers_line`
and `Face::lines` are new on `card_face`, and the terminal's
`app::card_modal` and the desktop browser's private `numbers_line` both
go through them now. All three had built the line from `CardDefinition`
directly, and so all three printed `Cost 0` on an agenda — which prints
an advancement requirement where a cost would sit — and on an identity,
which prints no cost at all. Going through the slots fixed it for every
caller at once. The terminal's card text also now renders printed
symbols as `Symbol::fallback` (`¢`, `»`) instead of the raw `[credit]`
tokens of the card JSON; the stand-ins existed for exactly this and the
tokens were the data showing through.

**Dev hook.** `NETRUNNER_HOLD_ACCESS=1`, following
`NETRUNNER_HOLD_SELECTION` and `NETRUNNER_HOLD_INSTALL`: the autoplay
stops at the first card the person is asked to access, so the screenshot
catches the face in the pop-up with its buttons under it.

**Verified.** `cargo test --workspace` green (1,553 tests, three
consecutive full runs — the desktop access test was flaky at first
because it counted frames rather than waiting on `awaiting`, and a
loaded test runner answers later than a frame budget allows), clippy
silent across the workspace. New tests:
- `netrunner_client::access` — an agenda reports its mandatory steal and
  an asset its live trash cost; only the side being asked is shown the
  card, at Archives as much as at HQ, and never a spectator; a masked
  card and the `SelectNextCard` step show nothing; the facts count what
  is left of the breach and explain a blocked steal (Ansel 1.0); an
  interactive trigger names its payer and says when they cannot afford
  it; `ordered` sorts pass last, so a stray Enter cannot give a card
  away.
- `netrunner_client::card_face` — the numbers line in the face's own
  order, with no `Cost 0` on an agenda and none on an identity; `lines`
  is the printed card and never the DSL.
- `netrunner_desktop` — the Runner runs R&D, continues and completes the
  run, and the `DecisionPopup` holds a `BodyText` carrying the accessed
  card's own words plus a button per access decision. Asserted on the
  card's *words* rather than the rendered string, because which glyph a
  `[subroutine]` is drawn with is the theme's choice and would otherwise
  tie the test to the machine's fonts.
- `netrunner_cli` — a view parked at an access renders the panel with the
  card's title, its printed numbers, its own words and the live fact,
  and the actions pane is still drawn beneath it.

Screenshotted both chairs sixty decisions in. The Runner's pop-up shows
Offworld Office at the large size — the real scan, since `card_images`
had it cached — over `Accessing Offworld Office`, `An agenda — it must be
stolen`, and `Steal Offworld Office`. The only scroll area logged is the
hidden log's, at zero size. **No engine file changed, so no sweep.**

**Found and deliberately not fixed:** `legal_actions_for` is not masked,
so at a `PendingInteractiveTrigger` the Corp's own button already reads
`Pay to avoid Snare!'s trigger` even where `PublicAccessPhase` masked the
card to `None`. This change shows a face only when the *masked* card is
`Some`, which is the conservative rule; the discrepancy is pre-existing
and is an engine-boundary question rather than a rendering one, so it
wants its own entry.

---

### 4p. The board says what can act, and in which of two moods — DONE (18 September 2026)

`feat/playable-cards-glow` (#66). Not from any of the three lists — the
person asked for it on being told what was next, having expected to find
it there: *"when player has priority highlight around cards those that
have a playable action … green glow standard playable, yellow like during
a run to pump strength or use ability or rez … No glow for unplayable.
Help the player during the action phase when they own priority."* It is
the debt §4g opened. Once a click stopped submitting and started opening
a menu, nothing on the board said which cards *had* a menu worth opening,
so the only way to find out what was playable was to click everything or
turn on the play helper — which is the flat panel the whole of §4 was
built to stop being the way in.

**The knowledge was already there.** A target with entries in the
`ActionMap` is a target the engine will accept an action on, and both
clients already built one. So this is a rendering of `legal_actions` and
nothing else: no engine change, no masking change, no new `PlayerAction`,
and the 192-game random-vs-random report is untouched because no engine
file was touched.

**Decisions taken, with the alternative rejected.**

- **Two moods, classified per action rather than per moment.**
  `netrunner_client::board::affordance` gives every legal action a mood:
  `Usable` for a move at the person's own pace (play, install, run,
  advance, score) and `Conditional` for a moment that will pass (a run,
  an open paid-ability window, a prompt parked on the person). The
  rejected alternative was to read the mood off the clock alone — "a run
  or a window is open, so everything listed is conditional" — which is
  true of today's engine and would have been half the code. It colours by
  *when* rather than by *what*, so a `PlayerAction` added later would
  inherit the mood of whatever moment it was first listed in, with no test
  failing and nobody deciding. The match is exhaustive instead: a new
  variant does not compile until someone says which mood it is.
- **Two actions are genuinely both, and take the moment as a tie-break.**
  `RezIce` is an asset flipped up at leisure on the Corp's own turn and an
  ICE rezzed on the Runner's approach; `ActivateAbility` is a Corp asset's
  click ability and a breaker's pump. Same action, two moments, and they
  are worth telling apart precisely because one of them expires. Every
  other variant is context-free.
- **Purple and yellow, chosen by the person over the green first
  proposed.** Purple because the two colours already spoken for on a card
  are the sides' own — Corp blue, Runner red — and their mixture belongs
  to neither, so it reads the same from both chairs. Yellow is kept clear
  of `danger`: danger is a number that has gone wrong, this is an
  opportunity about to be lost. The person also asked that a pop-up's
  buttons count as conditional, which they do — a parked decision is the
  clearest case of a moment that will pass.
- **A `BoxShadow`, not an `Outline`.** `Outline` is spoken for three times
  over on this board already — the transition highlight, the dragged
  card's lift, and the ice being encountered — and a node has exactly one.
  A shadow is a second component, so a card can glow *and* be outlined in
  the same frame, which is the common case: the card just drawn is usually
  also a card that can be played. Neither costs any layout, so
  `face_width`'s budget and the no-scroll rule are untouched. `BoxShadow`
  is a `Vec<ShadowStyle>`, so the contact shadows still owed (the third
  list's item 1) can be a second entry in the same component rather than a
  fourth claimant on the same slot.
- **Gated on `awaiting`, not on the map being empty**, and the test is
  what forced the distinction. A seat off priority keeps legal actions of
  its own — the Corp's standing rez is in the Corp's `legal_actions` all
  through the Runner's turn, 57 times over six games — so a board that lit
  whatever the map held would glow at a person who cannot act. The
  side-agnostic `legal_actions` the Session Rule relies on is exactly why
  the gate has to be the client's own.

**In both clients**, because the rule is one rule: the classification and
its tests are in `netrunner_client`, the desktop draws the glow, and the
terminal colours each card's title on the hand and rig lines (Magenta and
Yellow — the pane's palette is the terminal's sixteen, and a truecolour
purple would be the only exception in the file). The terminal turns it off
wholesale in a replay (a recorded board is not the viewer's to act on) and
under a lesson (a lesson narrows the offered actions to the step's own,
and a board glowing at the rest would be arguing with it).

**Verified.** `cargo test --workspace` green, clippy silent across the
workspace. Over six real games, both viewers, the classification was
exercised 3,663 times as `Usable` (1,230 of them hand cards) and 494 as
`Conditional` (411 on installs), against 53,909 target-views with no glow
at all — the ratio the colour depends on, since a glow on everything says
nothing. New tests:
- `netrunner_client::board::affordance` — a target takes a mood exactly
  when the engine offers something on it, over six real games as both
  viewers; the two dual actions follow the moment and nothing else does;
  `Conditional` wins a target that offers both.
- `netrunner_desktop` — the board's glow *agrees with the model* for every
  clickable target on it, rather than being counted: a glow derived from a
  card type, a credit total or what a system can see on screen would fail
  there the first time the engine disagreed. Plus: a hand card is purple
  on the Runner's own turn, and nothing is yellow while no window is open.

Screenshotted both chairs forty decisions in, and the only scroll area
logged is the hidden log's at zero size. The Runner's board lights four
playable hand cards, every runnable server header and both piles in
purple. The Corp's landed on its discard step with all seven cards yellow
under `Discard down to your hand size`, which is the classification
reading correctly: the turn cannot end until they act. A second Corp shot
caught a parked `Send a Message: choose 1 card` with its one pop-up button
yellow and the rest of the board dark.

**Owed, and deliberately not done here:** the control bar is unchanged.
Its buttons are already greyed when the engine does not list them, which
is the same information in the shape that bar has used since §4a; glowing
them as well would be saying it twice.

---

### 4q. The cards sit on the table: contact shadows, and the one BoxShadow they share — DONE (18 September 2026)

`feat/desktop-contact-shadows` (#67, stacked on #66). The second half of
the third list's item 1, and the item's last owed piece: §4m painted the
perspective into the field, and this puts the cards *on* it. Held back
until after the skin (§4n) on purpose — art carrying its own baked shadow
would have doubled up with a drawn one, and §4n left `Manifest::shadows`
and `Skin::wants_shadows` in place for exactly this PR to consume. They
are consumed now; nothing else reads them.

**The depth that was rejected, and the one that was not.** The third list
rejected a per-row *face-width* ramp on more than taste: the natural form
is not monotone in the face width, `face_width`'s binary search is
licensed only by `rows_height` being monotone, and the ramp would have
returned a silently wrong width with every existing test still green. A
shadow has no such problem, **because it is paint and not layout** — a
`BoxShadow` is drawn outside the node and measured by nothing. So the
cards stay one size and their shadows say which row is nearer, which is
the half of "closer objects larger" that can be had for free.
`models::layout::Depth` is `spawn_board`'s four rows, top to bottom (the
opponent's strip and hand, their area, the person's area, the person's own
strip and hand), each with a `(y, blur, alpha)` that grows toward the
chair; `Depth::nearer` raises a tile off the column it is stacked on. The
ramp is tested for monotonicity in all three values, which is the property
the eye is actually reading.

**One `BoxShadow`, composed in one place.** A node has exactly one, and
§4p had already spent it on the glow. Two `insert`s would have meant
whichever system ran second silently won, and the board would have lost
either its depth or its affordances depending on system order — a bug that
would have looked like a rendering flicker rather than a logic error. So
the two are components that *declare* intent (`Contact(Depth)`,
`Glowing(Affordance)`) and a single `shadows` system builds the vector
from whichever are present. The glow goes first, which is the entry drawn
on top: a halo the contact shadow has washed grey is not a signal.
Anything wanting a third shadow adds it there.

**Registered like `widgets::dress`, and for its reasons**: on `Added<..>`
so a freshly spawned board is shadowed the frame after it appears, and
over everything when the `Skin` resource changes, so a skin that carries
its own baked shadows turns the drawn ones off without the person leaving
the screen. It is its own `add_systems` call rather than a link in the
board's existing `.chain()`, because §4n's lesson stands: adding a system
to an existing chain reorders everything after it, which moved
`button_feedback` and broke twelve board tests.

**What casts, and what does not.** Cards (hand, rig, identities, the
opponent's backs), server tiles and server columns — the objects on the
field. Not the control bar, the pop-up, the phase bar or the rail: those
are chrome, not things on a table, and shadowing everything is how a board
ends up looking like a web page from 2013.

**Tuned by looking, once.** The first values (alpha 0.30–0.45, blur 3–10)
were nearly invisible: this table is close to black, and a black shadow on
a near-black ground is a no-op. What reads on a dark field is *area*
rather than darkness, so the ramp went to alpha 0.50–0.65 with blur 5–16
and the cards sit. Recorded because the next person to add a light table
will find these too strong, and the fix is the table's business rather
than the shadow's.

**Verified.** `cargo test --workspace` green, clippy silent. New tests:
`models::layout` — a nearer row casts a bigger, softer, darker shadow, and
the rows are read from the person's own chair (the Corp sees their own
servers in the near row, the Runner sees the same servers in the far one);
`netrunner_desktop` — a glowing card carries *both* shadows in one
component with the glow first, and a card with nothing to do carries the
contact shadow alone.

Screenshotted both chairs forty decisions in; the only scroll area logged
is the hidden log's at zero size. The Corp shot landed mid-run with the
Runner holding priority and **nothing glowing at all** — which is §4p's
`awaiting` gate visible in a picture, since that board has an unrezzed
Tithe the Corp could rez and a map-based gate would have lit it. No engine
file changed, so no sweep.

### 4r. The sheet is the card: no name over it, no Close under it, and a panel a skin can paint — DONE (18 September 2026)

`feat/desktop-sheet-frame`. Not from the third list: the person looked at
a card's sheet and named three things at once — *"the name of the card is
displayed bold at the top - this seems redundant as it should be in the
image of the card"*, *"the 'Close' button doesn't need to be there - Esc
Key to exit or any mouse click off of the card"*, and a want for *"a
thematic background image to this pop-up"* because *"the plain black and
blah text box will (ultimately) need to look nicer"*. The first two are
the space back; the third is the slots §4n left owed.

**Decisions taken, with the alternative rejected.**

- **The card names itself, so the sheet does not.** A cached NetrunnerDB
  scan *is* the printed card, and the text tier draws the title in the
  face's own title row — so the heading was the name **twice** on the
  fallback face and once too often on the other. It survives in exactly
  one place, which is why it moved inside the match rather than going:
  a card the viewer cannot name is drawn as a card *back*, and a back
  says nothing, so `facts::hidden_title` is all that names it. A zone's
  sheet and the score area keep theirs for the same reason — there is no
  face there at all.
- **The browser's inspector took the same cut**, asked for once the
  sheet had it: it drew the title *below* the face, which is the same
  name twice whenever the picture had not arrived. The lines left are
  deliberately not the same case even though the text face also draws a
  type line and the printed text — with a scan cached the face is one
  picture and nothing else, so that block is the only place the type,
  the numbers and the text exist as text. The title is the one of them
  the picture always carries legibly at this size. It costs one thing,
  recorded rather than discovered later: that column *scrolls*, so the
  name is off-screen once you have scrolled past the face, and the
  grid's outline on the selected card is what says which card it is.
  Removing the heading only ever shortens the column, so nothing that
  fitted before scrolls now.
- **Close was a third door to a two-door rule.** Escape already closed
  the sheet; the second door is a click that misses the panel. Between
  them that is about eighty-six logical pixels of chrome off a panel
  whose whole content is a 380-wide face. **No hint line and no `×`**,
  which was the person's own call when asked — `shortcuts::LIST` already
  carries `Esc`, and the list of keys is a keypress away.
- **A form is not a reading surface.** `Game::dismissed_by_a_click_away`
  draws the line: the card, the install, the zone and the score area
  close on a miss; the options and the list of keys keep their Close,
  because a setting given up because the pointer landed an inch wide is
  a worse failure than a button nobody needed; and the end of the match,
  a stall and the quit prompt are *asking* something with nowhere to
  dismiss to — Escape does not close them either, so a wash that did
  would be the only way out and would mean something different there
  from everywhere else.
- **The wash blocks whether or not it acts.** This is the bug the item
  found, and it was already live on `main` for every overlay. `Node`
  **requires** `FocusPolicy`, whose default is `Pass` — so the wash let
  `ui_focus_system` walk straight past it into the board, whose tiles and
  control-bar buttons are `Themed` buttons that take a press. A click
  through an open sheet onto where "End turn" is drawn ended the turn.
  Worse, the fix that looks obvious does not work: `Button` requires
  `FocusPolicy::Block`, but a required component is inserted with
  `Keep`, and the spawn has already given the entity `Node`'s `Pass`, so
  a post-spawn `insert((Button, ..))` silently skips it. The `Block` is
  therefore in the spawn bundle, unconditionally, with the reason beside
  it. **Found by a test written to assert the opposite** — the first
  reading of `focus.rs:323`'s `unwrap_or(&FocusPolicy::Block)` took the
  default to be `Block`, and the code and its comment both said so until
  `assert_eq!(get::<FocusPolicy>(scrim), Some(&Block))` printed
  `Some(Pass)`. The comment now records the trap rather than the wrong
  fact.
- **Four slots, not one.** `panel` is the base and reaches *every*
  `widgets::panel` in the client, the main menu and settings included,
  because it is one function — the same reach `button` already has. So
  `panel.sheet`, `panel.decision` and `panel.menu` exist as states of it,
  or a skin could not dress the game's own surfaces without repainting
  the menus; the two-level fallback means a skin holding one `panel.png`
  still dresses all four. `overlay.scrim` is deliberately **not** a state
  of `panel`: it is the thing *behind* a panel, so it borrows nothing.
- **The caller still says what it would have drawn.** The decision
  pop-up and the actions menu draw their border in the accent, so each
  replaces `widgets::panel`'s `Dressed` with one naming its own slot —
  without that, `dress` repaints the accent in the panel's own colour a
  frame after the spawn. The immediate `BorderColor` write stays beside
  it, because `dress` only runs on `Added<Dressed>` after the commands
  flush. Two `Dressed` in one bundle is a duplicate-component panic, so
  it is an `insert` on the spawned entity, not a bundle member.

**Two things recorded rather than fixed**, both pre-existing and neither
this item's to take on. `"tint": "state"` is documented in the skins
guide but **not wired** — `skin::wants_state_tint` has no caller outside
its own unit tests, so `Skin::dress` hands back `Color::WHITE`; the new
rows therefore do not repeat the promise. And
`an_access_puts_the_card_in_the_decision_popup_above_its_actions` carries
a ten-second wall-clock deadline (`tests/game.rs`) that it can miss when
the machine is compiling at the same time: it failed twice on runs that
took 10.19s and was then clean six times in a row, and clean three times
on `main` and three on the branch under forced rebuilds. Timing, not the
change.

**Verified.** `cargo test --workspace` green, clippy silent across the
workspace. New tests: the card's name appears once under the overlay and
as a `TextSpan` rather than a heading (the split is the assertion); no
`Close` on the card, install or zone sheet, and the options *keep*
theirs; the wash carries `Click::CloseOverlay` and pressing it closes,
while a form's carries none and pressing it does not, and the quit
prompt stands against both; both washes and the panel carry
`FocusPolicy::Block`; the decision pop-up names `Slot::PanelDecision` and
keeps `theme.accent` on its border; the install sheet's heading is
present for a card the viewer cannot name and absent for one they can;
one `panel.png` dresses the sheet, the pop-up and the menu while
`overlay.scrim` stays drawn. Proved end to end with a throwaway skin — a
64×64 plate, orange rim and yellow corner notches, 12px insets — which
dressed the sheet, the decision pop-up and the actions menu at their
different widths with the notches intact, then deleted; no art is
committed. Screenshots taken from both chairs forty and sixty decisions
in; the dev log's one `scroll area` line is the hidden log at zero size,
never the board. No engine file changed, so no sweep.

**Open, for the rest of the list:** the panels' own art (the slots exist;
nobody has drawn one), a server's mark, the remaining icons, the player
bars, the fanned hand, the hovered card and the right-hand side — and,
separately, wiring `"tint": "state"` so the guide stops promising it.

### 4s. The table has a near side and a far side, and its middle holds still — DONE (18 September 2026)

`feat/desktop-board-depth-layout`. Not from the third list, though it
lands two of its items (3, the hovered card, and part of 4, the space
used). The person asked for the board space to be used better: once a
player knows the cards by name and picture the opponent's side can be
smaller, which gives the table depth; **"when an opponent adds to their
rig it doesn't adjust the middle of the play field"**; both grips shown
as the top third of a card; the actions bar above the cards; and the
Corp's servers pulled to the Corp's edge so the middle is where ICE
grows — with room for each server to carry a picture of its own, and a
documented list of every asset the client can load. This entry is the
layout; the plates' pictures and the tiles' art are the two PRs stacked
on it.

**Decisions taken, with the alternative rejected.**

- **The card width no longer reads what is installed.** `Counts` lost
  `pieces` (the tallest server's ICE and root) and `rig` (empty or not):
  both were in `rows_height`, so the Corp's third ICE or the Runner's
  first install shrank every card and redrew the board — the middle
  moved whenever anyone did anything. Now `face_width` is a function of
  the window, the chair, the servers and the phase bar; the rig's row is
  reserved at `layout::rig_height` empty or not, and the ICE grows into
  the **ICE field** — the one flexible row, between the server plates
  and the run lane — whose tiles share its height
  (`layout::tile_stack`: as tall as their share allows between 24px and
  0.3 of the server's width, then overlapping by `step`'s rule on its
  side). A new remote can still narrow the cards, because columns cannot
  overlap; that is the one count left.
- **The far side is one constant, not a ramp.** `OPPONENT_SCALE = 0.75`
  for the opponent's strip, hand and area: the Runner sees the Corp's
  servers at three quarters and their own rig at full size, the Corp the
  reverse. §4m and the third list's item 1 rejected a per-row width ramp
  because its natural form is not monotone in the face width, which
  would have silently broken `face_width`'s binary search. A constant
  factor keeps every row a non-decreasing function of one width — the
  affine form item 1 had already called safe — and the `Depth` doc,
  which said the board was getting no scale at all, now says why this one
  is allowed.
- **A hand is a peek, and the hovered card lifts out whole.** Both hands
  show `layout::PEEK` (a third) of each card — the person's own from the
  top, the opponent's backs hanging from the far edge. The two hands had
  been a full card each: the largest things on the board and the least
  looked at. The lift is a second, inert copy drawn in a floating layer
  (`lift_hovered`) rather than the face moved: the face in the row stays
  put, so a drag, a click and the hand's order measure the same box they
  always did, and the copy carries no `Button`, so the focus system
  passes through it and the hover holds. It is paint only — never an
  action, never the log, never a view (item 3's rule) — and nothing lifts
  while a card is dragged, a menu is open or the board is covered.
  **The peek window's width has to be said:** a clipping node
  contributes nothing to its parent's size, so the first cut showed one
  card of eight, as wide as the "Your hand" label.
- **The strips went to one row.** At 175 and 210 tall they would have
  swallowed what the peeks saved, so the HUD is one row of six
  (`hud::PER_ROW` 3 → 6: both sides' shared readouts still sit in the
  same columns) and the Runner's details share a line with the pile
  buttons: 84 and 120. The identity in a strip is 0.4 of its side's
  width, down from 0.6.
- **The control bar is a row of the board**, directly above the
  person's hand, instead of a full-width row under the board and the
  rail. It is respawned with the board, so a rail-only redraw refills
  it in place and a board redraw brings a fresh one — refilling the old
  one as well would have written to an entity the board had just
  despawned.
- **A server's header became its plate**: `card width + 4` wide and
  9/16 of that tall, on the Corp's edge of the column — the bottom from
  the Corp's chair, the top from the Runner's — with the name along its
  lower edge. The box is reserved for every server, which is the
  condition the skins guide set for a server's own picture: art in a box
  that already exists cannot change the layout.

**Measured, 2048×1280 logical (2560×1600 at 1.25), phase bar on.**
Before, the person's strip row was 330 tall and the opponent's 185, and
at two ICE on the tallest server the board was already at 1,163 of its
1,188 — so a Runner seat's cards fell below the 220 cap at the Corp's
third piece. After, the rows are 125 and 99, the fixed rows cost 937 of
1,204, and the ICE field has 267 to itself; the face stays at 220 from
the first decision to the last, and seven ICE on one server overlap in
the field rather than moving a card.

**Verified.** `cargo test --workspace` green and clippy silent across
the workspace. New tests: the far side is 0.75 of the near side from
either chair; the fixed rows at the computed width leave the ICE field
its minimum and one pixel more would not (replacing the old fit
invariant); tiles share the field within their bounds and overlap past
the floor; on a real match the card width holds between the first
decision and the Runner's first turn while the Corp installs, every
server has its plate and the bar is on the board; a hovered hand card
lifts out as a non-button copy, holds, and goes with the pointer, with
nothing applied and no menu opened. `NETRUNNER_LIFT=1` holds the first
card lifted for a screenshot. Screenshots taken from both chairs forty
decisions in; the dev log's one `scroll area` line is the hidden log at
zero size, never the board. No engine file changed, so no sweep. The
access test §4r recorded as timing-sensitive missed its ten-second
deadline once (10.22s) on a run that was compiling alongside it, then
passed three times alone and on the full re-run.

**Reading stays on the secondary click.** The request said a card is
inspected "with left-click"; the primary click is the actions menu
(§4g), and the lift now covers reading a card in hand without either.

### 4t. Every server has a picture, drawn until somebody draws one, and one list of every asset — DONE (18 September 2026)

`feat/desktop-server-plates`, stacked on §4s. The same request's second
half: the servers *"end up having asset art work of their own ... that is
bigger than the thin tile"*, so someone can draw *"buildings"* that are
protected and attacked, with *"a default set of assets in all cases"* and
the list of them *"along with expected sizes"* documented.

**Decisions taken, with the alternative rejected.**

- **Board art is its own tier, not a skin slot.** A slot is a
  nine-sliced frame stretched to its box, which is right for a border
  and wrong for a building, which has to keep its proportions. So
  `board_art` is a folder of pictures drawn *inside* the plate, and
  the plate's skin slot (`server.header`) still frames it. A skin can
  still bring its own buildings, as `skins/<skin>/board/<key>.png`,
  which wins over `board/<key>.png` in the override and then the
  bundled directory, which wins over the drawn default.
- **Cropped to cover, never stretched** (`cover_rect`, into
  `ImageNode::rect`). The plate is 16:9 at every card width, but a file
  need not be, and a squashed building is worse than a cropped one. The
  guide says to draw at 16:9 and keep the middle busy.
- **A drawn default for every base key, and none for a state.**
  Archives is a vault under a pediment, R&D a tower of data lines, HQ an
  office block with a lit lobby, a remote a rack under a mast, each in
  the Corp's colour against a dusk. They are plain on purpose, so they
  read as places at plate size and never pretend to be anybody's art.
  `server.<x>.run` exists only as a file: undrawn, it borrows its base,
  one level deep, as a skin slot's state does. A drawn "under attack"
  variant would have been a second silhouette nobody asked to see.
- **Loaded on a change, not per frame.** `board_pictures` loads the set
  when the skin in use differs from the one it was built for, and marks
  the board for a redraw, so changing the skin from the gear menu swaps
  the skyline without leaving the screen. It is absent in the headless
  tests, which have no `Assets<Image>`, so a plate there is its label.
- **One list of every asset** (`assets/README.md`): fonts, card backs,
  card fronts, tables, skins, server plates and sounds. Each row gives
  the path, the logical box, the size to draw at, how it is fitted, what
  always works without a file, and the folder guide that says the rest.
  `assets/board/README.md` is the new folder guide, and a test checks it
  names every key `board_art` asks for, so a new key cannot land
  undocumented.

**Verified.** `cargo test --workspace` green and clippy silent across
the workspace. New unit tests: the crop covers without stretching in
both directions and survives a zero-sized box; the four drawn plates
are 512 × 288 and pairwise different, and every key is either drawn or
falls back, never both; a state with no file finds its base's picture,
and a remote under a run finds the remote's; the guide lists every key.
Screenshots taken from both chairs forty decisions in, with the
buildings on the plates and the names legible on their bands; the dev
log's one `scroll area` line is the hidden log at zero size. No engine
file changed, so no sweep.

### 4u. A tile shows what kind of card it is, and a counter shows its kind — DONE (18 September 2026)

`feat/desktop-tile-art`, stacked on §4t. This is the rest of the same
request: *"different images for rezzed and unrezzed locations/viruses/ICE
— let the game space become prettier"*. Partway through, the person
pointed at Null Signal Games' public visual-assets pack (*"Null Signal
does allow access to some Glyphs"*).

**Decisions taken, with the alternative rejected.**

- **A tile's picture is its kind, never the card.** The kinds are:
  - `ice.unrezzed`: hatching. It is one picture for every face-down ICE, because a type would leak what the Runner may not know.
  - `ice.rezzed.barrier`, `.code-gate` and `.sentry`: bricks, a gated lock and rings. They fall back to `ice.rezzed`, which is scanlines.
  - `root.unrezzed`, `root.rezzed.asset` and `root.rezzed.upgrade`, plus `root.agenda`.

  The key is read from the registry through the viewer's own masked `card`, so a face-down card can only ever be `*.unrezzed`. The card's own picture is still the sheet's.
- **Drawn tiles are grey, washed in the state colour.** That is the faction's colour when rezzed and the Corp's dimmed when face down, the same colour as the border. So the drawn tier is one image per kind for every faction. A file is drawn as painted (`Picture::drawn`), because an artist's colours are not the client's to wash.
- **Tokens became badges, and the words stayed.** `facts::tile_label` split into `tile_title` and `tile_tokens` (a `Token` of a `TokenKind` and its amount), and `tile_label` is rebuilt from them, so the terminal client's words are byte-identical.
  - The desktop draws each token as its kind's glyph beside the number. The rig's `N ctr` chip became the card's counter kind (`CardDefinition::counter_kind`), which is what makes a virus program's counters read as virus.
  - With no pictures at all (the headless tests) a badge is the old words, so nothing the tests read changed. The tile's words now sit on a band over the picture, and the tile test reads them from anywhere under the tile.
- **The glyphs are Null Signal Games' own, committed under CC BY-ND 4.0.** This is the person's explicit decision, taken after being shown the two conflicts:
  - AGENTS.md's bar for a committed asset is a GPL-compatible grant, which "no derivatives" is not.
  - The pack's term 4 speaks against combining its symbols with NSG card art, and the client shows NetrunnerDB scans.

  Recorded as the one exception in AGENTS.md §5 and `assets/board/LICENSE-NSG.txt`, which credits each file and its source SVG. The changes are conversion to a 128 × 128 PNG and recolouring the black symbols `#eef1f8` for a dark board. NSG's terms name recolouring as not a derivative, and nothing was reshaped. The pack's SVGs are not committed; only the ten PNGs used are.
  - Counters use the advancement, virus, generic (power) and credit symbols, over a drawn ring for a kind with none.
  - The HUD uses credit, click, agenda, bad publicity, tag and core damage, before each readout's number. The word stays under it.

  The strip's text column widened 400 → 480 so an icon, "5/7" and the next number no longer collide.
- **A central's mark in the plate's corner was tried and removed.** It was the NSG Archives, R&D and HQ icons in the plate's top-left. The person's verdict on seeing it: *"looks stupid and wasteful — remove them"*. The plate's building already says which server it is. The files, the keys and the docs went with it. So "a server's mark", owed since §4n, is answered by the plate's picture rather than a badge.

**Verified.** `cargo test --workspace` is green and clippy is silent across the workspace.
- **New unit tests:**
  - The ten tile patterns are 512 × 128 and pairwise different.
  - A face-down ICE is `ice.unrezzed` whatever its type, and an agenda is `root.agenda` face up or not.
  - Every key reaches a drawing except the optional HUD glyphs.
  - A drawn picture takes its state's tint and a file does not.
  - Every counter kind reaches a picture.
  - The guide lists every key.
- **Screenshots** were taken from both chairs sixty decisions in: hatched face-down ICE, a rezzed asset's coins in its faction's red with its advancement badge, and the HUD glyphs. The dev log's one `scroll area` line is the hidden log.
- **No sweep.** No engine file changed; the client crate's facts split is covered by its existing tests, whose output is unchanged.

### 4v. Every asset is its own licensed work, with a register that cannot drift — DONE (18 September 2026)

`chore/asset-credits`. The person asked for asset art to be *"wholly
separate licenseable entities"* that can be added and removed as needed.
An imported asset has its maker found and credited; one made in the
project, *"as part of AI"*, is GPL; and art behind the start screen that
the project did not make is attributed to its artist and their website.

**Decisions taken, with the alternative rejected.**

- **One register, `assets/CREDITS.md`, replaces the licence bar.**
  - The old rule was that a committed asset had to be CC0 or OFL-style, compatible with the GPL. That made every other licence a special exception (§4u's NSG symbols were the first).
  - Now the GPL covers the code, and each asset is a work of its own. Its row gives the file, what it is, the owner, their website, the licence, where the licence text is, the origin and what was changed.
  - `project` means made here, by hand, in code or with AI assistance: the contributors', GPL-3.0-or-later. `third-party` keeps its owner's licence, and the licence text ships beside it.
  - The drawn defaults painted at runtime are code and need no row.
- **A test, not a promise.** `tests/credits.rs` walks `assets/` and fails in any of these cases:
  - a committed file has no row
  - a row outlives its file
  - a `project` row is anything but GPL-3.0-or-later and the contributors'
  - a `third-party` row has no owner, has a website that isn't a link or `—`, has no committed licence text, or leaves its changes blank

  Checked by adding a stray PNG, which it named.
- **Attribution is found out, never guessed.** AGENTS.md §5 now says to ask whoever supplied an asset for its maker, their website and the licence before committing it. An unknown owner means the file is not committed.
  - It says so in practice too: NSG's pack names only its old site (nisei.net). The register's link, `https://nullsignal.games/about/nsg-visual-assets/`, was checked against the live site rather than assumed.
- **About, in the client, reads the same register** (asked for mid-item: *"An 'About' page and credits should show all of 3rd party info too"*).
  - `crate::credits` parses `CREDITS.md`, compiled in, into three tables: the committed assets, the things fetched on the player's opt-in (card data, card scans, the icon font, the official backs), and the software (Bevy, and every crate under cargo-deny).
  - `screens::about` shows the committed assets grouped by owner and licence: Null Signal Games' ten symbols are one credit, and each change is listed once, without its source file.
  - The screen is a main-menu entry of its own. It scrolls, because it is a reading screen and not the board.
  - A second list in Rust would have been the one that fell behind, so there is none. A navigation test checks that every owner and website in the register appears on the screen.
- **The start screen's art** has no picture yet. When one comes, the rule is the same as for any picture a person looks at: the artist by name, with their website. The register says the client may later show these credits.
- The root README's licence section now says the assets are licensed one by one and points at the register. Every folder guide's "licensing" paragraph (board, tables, skins, sfx) now says the same.

This **supersedes §4u's framing** of the NSG symbols as "the one recorded exception" to a GPL-compatible bar. There is no bar left to be an exception to, only a register. The entries are unchanged.

**Verified.** `cargo test --workspace` is green and clippy is silent. The About screen was screenshotted through `NETRUNNER_SCREEN=about`.

### 4w. A card scan is 750 pixels wide wherever Null Signal Games printed it, and drawn at the width it covers — DONE (18 September 2026)

`feat/hires-card-scans`. The person asked for card art that looks good on screens with more pixels than their laptop. Their proposed source was Null Signal Games' print-and-play PDFs, but existing downloads were fine if they held up at high resolution. Every card was to be of similar quality, with any fallback listed so it could be targeted from the PDFs, and the focus was on Null Signal Games' sets.

**Measured first (18 September 2026).**

| Source | Per card | Notes |
|---|---|---|
| NetrunnerDB `v2/large/{code}.jpg`, what the store fetched | 300 × 420 | narrower than the 380-point `FaceSize::Large`, so it was stretched even at 1× |
| NetrunnerDB `v2/xlarge/{code}.webp` | 750 × 1050 | WebP only; `small` (116) and `medium` (165) are the other sizes |
| NSG's PnP PDFs (`access.nullsignal.games`, 54–129 MB a set) | ≈ 744 × 1039 | one 300 dpi CMYK JPEG per page of a 3 × 3 sheet, text baked in |

- **xlarge covers every Null Signal Games card in the catalog.** All 77 System Gateway codes and all 82 Elevation codes answer 200. Only the Fantasy Flight Core Set (01xxx) answers 403, and it has no NSG PnP.
- **The PDFs are not sharper.** A crop of René "Loup" Arcemont out of the System Gateway sheet, set beside the xlarge WebP, matched it in pixel density and sharpness.
- **The PDFs have other costs.** A card on a sheet can be matched to its code only by its place in the set's order. The pages need Adobe-CMYK inversion. NSG's pages allow printing for play ("print on your own printer") but grant no licence to redistribute.
- **So no PDF cutter was built.** The fallback list it would have been aimed at is empty for NSG sets. It is worth building only if a future NSG card has no xlarge scan.

**Decisions taken, with the alternative rejected.**

- **xlarge first, the API's template on a 403 or 404** (`netrunner_card_sync::images`).
  - The two files are `<code>.webp` and `<code>.jpg`. The WebP outranks the JPEG, and a new WebP deletes the JPEG it replaces, so an old cache upgrades on its next Download.
  - Codes with no xlarge are kept in the manifest (`no_hires`, defaulted so old manifests load). This means they are not probed again, and **`CardImageStore::low_res` is the fallback list** the person asked for.
  - `netrunner_cli cards images [--set sg --set elev] [--download]` prints that list by name. The desktop's download line says "N only at low resolution" and logs the codes.
  - Bytes are checked for a WebP or JPEG signature before they are kept, because the CDN's 403 body is XML.
- **Scans are still never committed.** CREDITS.md's "Fetched" row names the new size, and its owner and terms are unchanged.
- **A face gets a copy resampled to the width it covers, not the scan** (`card_images::{rung, fitted}`).
  - Drawn at the grid's 280 physical pixels, the 750-pixel scan's printed text broke into jagged strokes: a linear sampler reads four texels however far it shrinks. The old 300-pixel scan at under 2× had only softened.
  - A face now asks for the first of eleven rungs, each 1.25× the last, that is at least its logical width × the window's scale factor. Above 671 it takes the whole scan. The sampler therefore never shrinks a picture more than 1.25×.
  - Faces are cached per (card, rung) and are render-world only.
  - **A mip chain was the first cut and was rejected.** It holds 4 MB of GPU memory a scan, over a gigabyte for a browser of every card. A resampled grid cell at 2× is 344 px wide and under 0.7 MB.
- **The resample is `DynamicImage::resize_exact`, not `imageops::resize`.** The generic `imageops::resize` is monomorphised into `netrunner_desktop`, which is unoptimised in the dev profile, and took 446 ms a scan. The concrete method is compiled inside `image`, and takes 15.7 ms. `image` is now a direct dependency, the same version Bevy already builds.
- **Four decodes in flight, the looked-at card first, and each resampled copy kept on disk** (`CardImages::queue`, `load_front`). The person ran it on an upgraded cache: *"a few seconds to appear to load all the cards randomly. Selecting a card sometimes has a moment blip"*, and the same on the board.
  - Each WebP costs about 57 ms to decode. The browser queued every scan at once, so the grid filled in pool order and the inspector's card waited behind all of it. The screenshot hook also caught whole black frames while that flood was running.
  - Now at most four decodes run at a time. A `Large` face jumps the queue.
  - Each copy is kept as raw pixels behind a 16-byte header under `images/sized/<code>-<ext>-<rung>.rgba`, so only a card's first view pays for the decode. A copy older than its scan is made again, and the name carries the scan's format, so a new `.webp` never shows the `.jpg`'s copy.
  - Measured on this machine: the frame at 30 was black before this change. After it, a cold start is partly drawn at that frame and a warm one is complete. 120 faces' copies come to 22 MB.
- **`dev`'s screenshot waits for its save.** The hook exited a fixed thirty frames after asking, and with the scans decoding the exit came first and no file was written. It now exits once the observer has saved.

**Verified.**
- `netrunner_cli cards images --download`, run over a copy of a 300-pixel cache, wrote 159 `.webp` files at 750 × 1050 and left 112 Core `.jpg`s. A second run fetched nothing. `--set sg --set elev` lists 0 low-resolution cards.
- Browser and game screenshots (both chairs, forty decisions in) show sharp grid cells, sheets and board faces. The only `scroll area` line is the empty zero-sized one.
- `cargo test --workspace` is green, clippy is silent, and `cargo deny check` passes with `image-webp` added.

### 4x. A choice between cards shows the cards — DONE (18 September 2026)

`feat/choices-show-their-cards`. A report from play, the second of its
kind: *"I played the Operation card Top-Down Solutions and it drew 2
cards and then choices to pick… show the human the cards. How are we
supposed to make decisions without knowing details about what the cards
do?"* §4o had fixed exactly this for an access and left every other prompt
that names a card as words: a card selection offered `Select Anthill
Excavation Contract` and `Select Humanoid Resources`, and the install that
followed asked where to put a card it did not show.

**Decisions taken, with the alternative rejected.**

- **No engine change.** `ClientView::selection` has carried each
  candidate's `CardId` to the chooser since §4c, `Placement` knew the card
  going in since §4d, and every `PendingDecision` carries its asking card.
  The clients were drawing names from all three.
- **A selection draws every candidate as its card, and the card is its
  own button** (`spawn_decision_popup`, `ChoiceCard`). The `Select …`
  label stays under each card, so keys and a label to click are
  unchanged. The cards keep their positions' order, so the one just chosen
  stays where it is (outlined) rather than moving to the end of the list.
  Folded copies (§4c) are one card marked "2 copies". Pressing a card in
  the pop-up submits, as its button does: this is the pop-up's own
  choice, not a card on the board, so §4g's "a click on the board never
  submits" does not apply. A secondary click reads the card in the sheet,
  as it does on the board.
- **The cards give, the window does not** (`layout::choice_faces`).
  Every row count is tried and the one that draws the cards widest wins,
  capped at the sheet's 380. There is no floor, because a card cut off the
  window is no card at all. The alternative, a fixed thumbnail size, is
  what put the hand below the fold twice.
- **Otherwise the pop-up shows the one card the prompt is about**
  (`board::Prompt::card`): the card an install is placing, else the card
  whose text is asking (a text choice, a server choice, a pay-or-not, a
  prevention's offer). The access keeps its own card.
- **The terminal draws the card under the cursor** of a selection, and
  the card an install is placing, over the board, the way §4o draws an
  access (`card_in_question`). It does not draw the asking card: a run on
  a server of the Runner's choice would then cover the servers being
  chosen between.

**Verified.** A seeded headless game (`fashion_lab` as the Corp) plays to
a real card selection. The test asserts one `ChoiceCard` per button in
position order, and that a press on a card (not its label) selects it and
leaves it drawn and outlined. The terminal test asserts that the
highlighted card's printed words are on screen. Screenshots of
Top-Down Solutions' selection and of the install after it show both cards
and the card going in, with no scroll area. `cargo test --workspace` is
green and clippy is silent.

**Two follow-ups, the same day** (`fix/choice-popup-gap-and-scans`).
First, each row of cards carried about 64 px of empty space under its
buttons. That was cosmetic in a single row, but it made a two-row pop-up
(Seamless Launch offering six cards) run off the top and bottom of the
window, because `choice_faces` did not know about the space. The cause
was the caption button's `width: 100%`: while a wrapping row is being
measured, a percentage has nothing to resolve against, so the label was
measured one word to a line and the row kept that height. The button now
has the card's width in pixels. Second, a card in the pop-up drew its
text face until its own size of scan had decoded. It now draws the
sharpest copy the board has already decoded, which it almost always has
because the candidates are usually in hand, and swaps in its own size
when that lands (`CardImages::nearest_face`, `WantsImage`).

### 4y. Every place has an asset slot before its art exists, and the drawn tier is only the fallback — DONE (18 September 2026)

`feat/desktop-asset-slots`. The person asked for places for more art: a
splash at startup, a background for the start screen and a different one
for each sub-menu (the deck builder included), Corp-specific server
styles *"chosen based on the Corporation's Identity card"*, and several
shipped tables drawn at random. The standing instruction was to *"always
be keeping locations prepped to have assets made"*, and the correction
that shaped the table rule was that the drawn pictures are *"no frills.
It should only be fallback not in rotation"*.

**Decisions taken, with the alternative rejected.**

- **A screen's backdrop slot comes from its root, not its code.**
  `nav::screen_root` carries a `ScreenBackdrop`, and `backdrop::dress`
  inserts the picture and a dimming scrim as the root's first children.
  Every screen, the five stubs included, got a slot without a line of
  its own. `AppScreen::asset_key` names the slot with an exhaustive match,
  so a new screen does not compile until it has one, and
  `the_guide_lists_every_screen` fails until `assets/backdrops/README.md`
  lists it. That is the "always prepped" rule turned into a test rather
  than a habit.
- **One shared `menu.jpg` dresses every screen nobody drew for.** A
  screen's own picture improves on it rather than being needed before it
  (`netrunner_client::backdrop::candidates`). One picture is therefore a
  finished set of menus.
- **Cropped to cover, not stretched like a table.** A table's
  perspective was painted for the window, and cropping it would move the
  vanishing point. A backdrop has no vanishing point, and a menu's content
  is centred. The dim under the text is per key in an optional
  `backdrops.json`, because only the author knows how bright the picture
  is.
- **The splash is skippable and never shown to a window nobody sees.**
  It holds for 1.5 s and until the fonts load (5 s at most), and any key
  or click skips it. Boot goes there only with a primary window and no
  dev hook naming a screen. So the headless tests still boot to the menu,
  a `NETRUNNER_SCREEN` screenshot is unchanged, and no test-only flag was
  needed (the plan had one). Its logo is `backdrops/splash-logo.png`,
  beside the pictures rather than in a folder of its own, so one guide
  covers it.
- **A Corp faction may restyle every board key, not only the plates.**
  `board/corp/<faction>/` sits between the skin and the generic art.
  The faction comes from `CorpClientView::identity`, which is public to
  both chairs, so nothing reaches around masking.
  - **A state falls back to its base inside a layer before the next layer
    is tried.** Otherwise a Jinteki HQ would turn generic whenever it was
    run on, if a generic `server.hq.run.png` existed.
  - **The drawn plates are lit in the faction's colour**, so the style
    shows before anyone draws it. A neutral Corp keeps the Corp's blue
    rather than a grey.
  - **Per-identity styles were deferred**, at the person's choice. They
    would be one more layer, and no file would have to move.
- **The painted ground left the table choices.**
  - **The default is Random.** Random draws only from installed tables and
    avoids the previous match's table (`table::LastTable`) when there is
    another.
  - **`Table::Painted` is gone.** A settings file still carrying
    `painted` reads as Random: it was the old default, so it cannot say
    whether anybody meant it.
  - **The plain ground is now the fallback and one switch, Settings →
    Basic graphics (slow machines).** That switch loads no pictures:
    table, backdrops, splash logo, board art, skin.
  - **A skin on Auto follows the table the random pick drew**, not only
    a named table.

**Verified.** `cargo test --workspace` green and clippy silent. New tests:

- the backdrop candidates and dim fallback;
- every non-board screen has its own slot and the guide lists it;
- the Corp style layer's order and its guide rows;
- the drawn plate differs by faction;
- basic graphics loads only drawn pictures, for board art and backdrops alike;
- the table row never offers the ground, and Random never draws it with a
  table installed, over 64 nonces;
- a random pick avoids the last table;
- the splash moves on by a key and by itself.

End to end, throwaway files went under a scratch `XDG_DATA_HOME` and never
into the repo:

- a `cards.jpg` dressed the card browser and the shared `menu.jpg` dressed
  Settings and the splash;
- `board/corp/jinteki/server.hq.png` appeared on an Advanced Yomi board
  (Jinteki) and not on a Brutal Efficiency one (Haas-Bioroid);
- the drawn plates were red for the first and purple for the second;
- one of two test tables was drawn under both.

The only `scroll area` logged was the hidden log's, at zero size. No
engine file changed, so no sweep.

### 4z. The board runs the window's full height; the status, the phase and a run's Runner are the right column's — DONE (18 September 2026)

`feat/desktop-board-edge-to-edge`. Asked for in one message: too much
wasted space still. The "Game" heading and the turn line were in a bar
across the top, and the same kind of information sat in the right
column. The person wanted all of it on the right. They wanted cards on
the window's top edge, and on its bottom edge always ("it looks good
when the phase bar is gone"). The phase should be its own panel further
right. And during a run, the Runner's identity should appear on the
right "to indicate they are hacking into a system": the upper image
portion of the card if there is a scan, just the name if not.

**Decisions taken, with the alternative rejected.**

- **One right column, no top bar.** From top to bottom it holds the
  status line with Quit and the gear, the phase panel, the run panel,
  the rail and the log. The "Game" heading is gone because it named the
  screen and nothing else. The root pads only its left and right
  sides. The column carries its own vertical padding, so its buttons
  keep off the edges. `layout::board_height` is now the whole window.
- **The hands are on the edges.** The opponent's backs already hung
  from the top of their row. The person's strip row is now
  `FlexEnd`-aligned and is the board's last row. Height the fixed rows
  leave goes to the ICE field between the two rows, never under the
  hand.
- **The phase is a panel, not a row.** It keeps the same
  `board::phase::bar` words and the same `PhaseStep` marks. Each segment
  is a column (the turn in one, a run in the next) with its steps top
  to bottom, and the window's line sits under both. `Counts::phase_bar`
  and `PHASE_BAR` are gone, so `face_width` depends on the window, the
  chair and the servers only. L and the options row now hide the panel
  rather than despawning it, and no card moves either way. This
  reverses §4j's "turning it off gives the cards the row back": there
  is no row to give.
- **The Runner on a run follows the trail, not the view.** The panel
  appears on the run's first paced beat and goes when the trail ends,
  so it is never ahead of the lane. It reads "Hacking into <server>",
  then the top of the identity's scan, then the name.
  - **The crop** is the name banner and the art, stopping where the
    text box begins: `layout::IDENTITY_ART` = 0.63 of the card's
    height, measured off Null Signal Games' frame (the text box starts
    at 665 of 1050 on both Zahya Sadeghi and The Catalyst). It is an
    `ImageNode::rect` taken as a fraction of the decoded copy's own
    size, because the resampled copies differ in pixels.
  - **The panel asks for its own copy.** The first cut drew
    `nearest_face`, which was the strip's identity at a fifth of the
    width, and the result was blurry. The panel now requests a copy at
    its own width and redraws when that copy lands.
  - **With no scan** the panel shows the name alone, at heading size,
    in the Runner's colour.
  - **It is paint only:** no button and no action.
- **Two new skin slots**, `panel.phase` and `panel.run`. Both fall back
  to `panel`, and both have rows in the skins guide.
- **A text line in the right column states its width in pixels.** The
  column sizes a panel before a percentage has anything to resolve
  against, so each of the run panel's two lines was measured at one
  word per line. The panel kept 101 px of nothing under a one-line
  name. The layout was dumped to find this; the picture and the skin
  were ruled out first.

**Measured** (`face_width`, five servers, at HEAD with the phase bar on
against this branch):

| Window (logical) | Chair | Face before → after | ICE field before → after |
|---|---|---|---|
| 2048×1280 | Corp | 220 → 220 | 292 → 430 |
| 2048×1280 | Runner | 220 → 220 | 267 → 405 |
| 1920×1080 | Corp | 218 → 220 | 97 → 230 |
| 1920×1080 | Runner | 209 → 220 | 97 → 205 |
| 1366×768 | Corp | 72 → 134 | 59 → 97 |
| 1366×768 | Runner | 72 → 119 | 44 → 97 |

A laptop screen was at the floor width before and is not now.

**Verified.** `cargo test --workspace` green and clippy silent.

- **Changed test:** the phase-bar test now asserts that L hides and
  shows the panel with the face width unchanged.
- **New tests:**
  - a run on R&D puts the Runner's name and "Hacking into R&D" in the
    right column, with none before the run;
  - the root has no top or bottom padding, and the board's last row is
    the person's strip and hand, pinned to the bottom;
  - the layout unit test asserts that `board_height` is the window's.

Screenshotted at 2560×1600 from both chairs forty decisions in. The run
was screenshotted with `NETRUNNER_HOLD_RUN=1`, under a scratch
`XDG_DATA_HOME` with the phase panel on, because the person's own
settings have it off. The backs touch the top edge, the hand touches
the bottom, and the only `scroll area` logged is the hidden log's, at
zero size. No engine file changed, so no sweep.

### 4aa. The rig is three rows, programs next to the ICE, and the heap opens on a click — DONE (19 September 2026)

`feat/rig-rows-and-open-heap`, the first item of §8 (borrowed from
jinteki.net). The person pointed out that a real table lays a rig out
in three rows. The board drew one row of three side-by-side groups.

**Order.** From the Runner's chair, top to bottom: programs, hardware,
resources. That puts programs next to the ICE they break and resources
next to the Runner, which is jinteki's order and the physical table's.
From the Corp's chair the rig sits across the table with the ICE below
it, so the order is reversed and programs are still the row next to the
ICE. `netrunner_client::board::rig::rows_top_down` is that rule, and
both clients read it. The TUI draws a `Programs:` / `Hardware:` /
`Resources:` line each, in the same order.

**Size: three peeks, not three small cards.** Measured before choosing
(`face_width`, five servers):

| Window | Chair | One row (before) | 3 whole rows at ½ width | 3 rows, top third (chosen) |
|---|---|---|---|---|
| 1920×1080 | Runner | face 220, field 205 | face 205, rig card 102 | face 220, field 206 |
| 1920×1080 | Corp | face 220, field 230 | face 220, rig card 82 | face 220, field 229 |
| 1366×768 | Runner | face 119 | face 85, rig card 42 | face 119 |
| 1366×768 | Corp | face 134 | face 101, rig card 37 | face 133 |

Three whole rows left a laptop's rig 42 px wide and shrank every other
card on the board with it. A row that shows the top `layout::PEEK` of its
cards is the hand's rule. The title and cost are at the top of a card;
strength, hosted cards and counters are on the chip line under each row;
the rest is a secondary click away. Three rows are then the height of one
card, so the split cost the board nothing. Row labels sit at the row's
left (`RIG_LABEL_WIDTH`) rather than over it, because the rig has width
to spare and no height. Every row is reserved whether or not anything is
in it, so the first program moves no card.

**The heap.** A primary click on a pile opened its actions menu. No
action is ever on the heap (a card installed from it is offered on the
prompt), so the menu was always empty, and the list of its cards took a
secondary click. A plain click on the heap now opens its sheet, as the
Agendas readout does. The count stays as it was. jinteki also shows the
top card of the heap and of Archives face up on the board; the person
chose to keep the count.

**Verified.** `cargo test --workspace` green and clippy silent. New tests:
- `rig::the_rows_are_the_table_seen_from_the_chair`
- `layout::three_peeked_rig_rows_are_about_one_card_tall`, which also
  checks the rig height is monotone over every face width
- `a_click_on_the_heap_opens_every_card_in_it`, for both chairs, on
  priority or not
- the board test `the_rig_is_three_rows_with_programs_next_to_the_ice_from_either_chair`
- the zone-sheet test now clicks the Heap button
- the TUI board test checks the row order from both chairs

Screenshotted at 2560×1600 from both chairs forty decisions in. The only
`scroll area` logged is the hidden log's, at zero size. No engine file
changed, so no sweep.

### 4ab. A piece of ICE is broken with one press, at the price on the button — DONE (19 September 2026)

`feat/auto-pump-and-break`, item 2 of §8. Getting through a piece of ICE
used to take one press per pump and per break, each from the breaker's
own menu, with no total shown until the credits were gone. Mid-encounter
the right column now lists one route for each card that can break
**every** pending subroutine, with its price: "Break Wall of Static with
Corroder · 2 credits". One press, or a number key, carries the whole
route out. The terminal clients, local and remote, list the same routes
after the actions.

**A route is found by playing it, not by pricing it.** The first design
read each breaker's JSON (pump cost over pump amount, break cost over
break count), which is how `netrunner_bots::eval::break_cost` prices a
break for the evaluator. That design was rejected because the view does
not hold enough to price a route. `PublicInstalledRunnerCard::current_strength`
leaves out Echelon's and Rising Tide's `strength_modifier` and
GAMEDRAGON's bonus. Mayfly's break is the first of a `Sequence`.
Chromatophores gives the ICE a subtype, and Semak-samun has a
fracter-only subroutine. Instead, `netrunner_client::board::breaks`
searches each card's abilities on `netrunner_bots::determinize`'s sample
of the view, using `apply_action` with a Corp pass between steps. A
route therefore contains only what the engine accepts, and costs what
the engine charges. Nothing hidden can move the price, and the sample is
seeded from a constant. Echelon is the test: the view says strength 0,
the engine counts three icebreakers, and the route is two breaks for 2
credits. Priced off the view, it would have been two 3-credit pumps
first. The one blind spot is Sang Kancil's discount, whose condition
(who started the run) is not in the view, so its price can read high.

**One route per card, not the cheapest plan.** Mayfly breaks for the
same credits as Corroder and is trashed when the run ends. Botulus
spends a virus counter the next ICE might want. Which route is best is
the person's call. The list is sorted by price, and mixing two cards on
one ICE stays on the card menus.

**One legal action per view.** Every activation hands priority to the
Corp (`paid_ability::note_window_action`), so a route cannot go as a
batch. `AutoBreak::next` is asked on each view where the Runner holds
priority, re-plans from that view, and submits the next step only if it
is in `legal_actions` and the total has not risen past what the button
said. It stops, and says why, if the ICE grew, a credit went elsewhere,
the engine refused a step, or the encounter ended. It never passes at
the end: the subroutines are broken, and moving on is the person's
press. The Session Rule holds, since every step is an ordinary `submit`
of a listed action. `NETRUNNER_HOLD_BREAK` stops the autoplay at the
first encounter that offers a route.

**Verified.** `cargo test --workspace` green and clippy silent. New tests:
- seven in `board::breaks`: routes cheapest first with the wrong type
  left out, a card that cannot afford the whole ICE not offered, nothing
  off priority or outside an encounter, a two-subroutine break, Echelon's
  hidden strength, the driver end to end spending exactly the route's
  price, and the driver stopping when the ICE grows
- the desktop model: one press and one number key, carried to the end
  through the engine
- the local TUI: a route row with its price

No engine file changed, so no sweep.
Screenshotted at 2560×1600 from both chairs forty decisions in. The rail
change moves no card, and the only `scroll area` logged is the hidden
log's, at zero size. **The route buttons themselves were not shot.** Two
autoplay runs under `NETRUNNER_HOLD_BREAK` (600 and 2,000 decisions, 5
and 25 minutes) never reached an encounter the rig could break: the dev
game's seed is random, its autoplay walks the list without aiming at
anything, and the Operator Corp in the forty-decision shot had installed
no ICE by turn 8. The hook stays, since it is how the shot gets taken
once a seeded dev game exists. Until then the look of the rail is
unverified, and the first hand-played encounter is the check.

### 4ac. Broken and fired subroutines are marked where the encounter is decided — DONE (19 September 2026)

`feat/subroutines-on-the-ice`, item 3 of §8. §4ab made a piece of ICE
one press to get through; it did not make the result visible. The state
of an encounter — which subroutine is broken, which has fired, which is
still waiting — was only in the ICE's sheet, a click away and a click
back, or in the log. The right column now carries it: the ICE being
encountered, its strength, and a line per subroutine marked `[x]`
broken, `[!]` fired or `[ ]` pending, directly above the routes that
change it. The terminal clients, local and remote, list the same lines
under the ICE in the run block, with a broken one genuinely struck
through (`Modifier::CROSSED_OUT`).

**The words come off the view, not the registry.**
`netrunner_client::board::facts::encounter_subroutines` reads
`PublicRunIceIdentity::subroutines`, which carries the run's own
`SubroutineDef` and its `SubroutineStatus`. That is the copy the engine
is resolving, so a subroutine a card *added* to the ICE is listed and
one it removed is not — the registry's printed list would be wrong in
both cases. It also needs no masking rule of its own: `mask_run_ice`
already gives the identity only to the Corp or on a rezzed ICE, so a
derezzed ICE mid-encounter tells the Runner nothing, which is tested.

**`EncounterIce` only, not the approach.** Nothing has happened to a
subroutine at the approach, so the list would be a second copy of the
card's text with nothing to mark; the encounter is also exactly the
window `breaks::routes` offers a route in, so the marks and the buttons
that change them appear and disappear together.

**Not gated on `awaiting`**, unlike the glow and the routes, and the
Corp screenshot is why: Brân 1.0's first subroutine fires, the Corp is
asked where to install, and both sides want to see `[!]` against that
clause while the question is open. The marks are something to read, not
something to press.

**The marks are one vocabulary across three surfaces.** `subroutine_word`
gives "broken", "fired", "pending" and `install_facts`' sheet lines now
call it too, so the sheet and the board cannot drift apart — a test over
real games asserts the two produce the same strings for the encountered
ICE. The desktop uses the terminal's `[x]`/`[!]`/`[ ]` characters rather
than a tick glyph, for two reasons: the bundled Noto fallback is not
guaranteed to carry one, and a person moving between the clients should
not have to learn the marks twice. Colour is what the desktop adds.

**A pip per subroutine on the ICE tile was the other candidate and was
rejected.** The run lane already draws exactly that — a dot per
subroutine in these three colours, on the chip for each piece of ICE —
so a tile pip would be the same fact a third time and still not say
*which* subroutine. The words were what was missing, and they need a
column's width. The tiles are untouched, so no card moved.

**A dev hook was needed to see it at all, and that is the debt §4ab
recorded.** `NETRUNNER_HOLD_RUN` stops the *pacer*, which holds the
beats before the view has caught up — so the rail is still on the
previous decision — and it counts an undefended server's approach as an
encounter, which the sample Corps' first run very often is (the first
attempt here shot a run on an empty Archives at turn 2). `NETRUNNER_HOLD_ICE=1`
holds the *autoplay* on the settled view the way `NETRUNNER_HOLD_BREAK`
does, but without needing a rig that can break the whole piece — which
is what defeated §4ab's two attempts at 600 and 2,000 decisions.

**Verified.** `cargo test --workspace` green and clippy silent. New
tests:
- three in `board::facts`: the three marks in the subroutines' own order
  against the clauses Brân 1.0 prints, from both chairs; a derezzed ICE
  and every non-encounter phase giving nothing to the Runner while the
  Corp still reads its own card; and, over real games at six seeds, the
  board's marks and the sheet's lines agreeing string for string
- the desktop model test now reads `Game::encounter` before and after
  the route runs: `("End the run.", "pending")` then `("End the run.",
  "broken")`
- the terminal test renders the board at the same two moments and finds
  `[ ] End the run` then `[x] End the run`

**There is no headless test of the desktop rail's own drawing**, and the
reason is worth writing down rather than rediscovering: `redraw` is
gated on the private `Dirty` resource, so an integration test in
`tests/game.rs` cannot make the rail respawn without a real match
played into an encounter against a live bot. That is why the marks are
computed on `Game` — `Game::encounter`, a reading of the view, not a
field — where the model test can reach them, and why the drawing is
verified by screenshot. Marker components were written for a test to
find and then removed unused rather than left as a comment that lies.

Screenshotted at 2560×1600 from both chairs, at a real encounter under
`NETRUNNER_HOLD_ICE`. The Runner's chair: Brân 1.0 at strength 6 on
Archives with all three subroutines `[ ]`. The Corp's chair: the same
ICE on HQ with its first subroutine `[!]` in red, the install choice it
fired open in the pop-up, and the lane's first dot filled to match. The
only `scroll area` logged in either is the hidden log's, at zero size.
**`[x]` is the one mark not shot in the desktop** — it needs a break to
land while still encountering, which no hook stops at; the terminal
render test and the desktop model test both cover it, so what is
unverified is the dim colour, not the mark. No engine file changed, so
no sweep.

### 4ad. A menu and a pop-up never put an option off the window — DONE (20 September 2026)

`fix/menu-stays-on-the-window`. The person opened a card's menu and its
options ran off the screen: *"The left click action menu can go off
screen. The options should all always be visible."*

**Two faults, both of the same kind: a size guessed before the layout
ran.**

- `spawn_actions_menu` computed its `top` from an estimate of its own
  height — the panel's padding, a 22px heading, and **a 40px row per
  entry**. A row is about 36px on one line and 57 when its label wraps,
  and the labels do wrap: the text column inside the 280px panel is
  ~232px, so "Install Palisade into Archives (Ice)" is two lines. Four
  such rows are 308px against the 238 budgeted, and the panel's own
  border was never counted at all. Placed above a card the difference
  went out through the bottom of the panel; placed *below* one it went
  off the window, because the screen root is `Overflow::clip()` and ate
  it.
- The fallback branch — `below.min(window.y - height - PADDING)` — had
  no `.max(PADDING)`, so a menu taller than the window was given a
  negative `top` and lost its *first* options instead of its last.
- The decision pop-up is centred and so cannot be mispositioned, but it
  guessed its "chrome" the same way to decide how big the candidate
  cards may be, counting one line per line of text. A wrapped button
  label was a row nobody had budgeted, and the buttons under it went off
  the bottom.

**Decisions taken, with the alternative rejected.**

- **The panel is pinned by the edge that faces the card and grows away
  from it** (`models::layout::menu_box`). Above a card its *bottom* is
  fixed a gap above the card's top edge and it grows upward; below one
  its top is fixed and it grows down. **So its height is never needed**,
  and `max_height` — the room that side has — is what it may grow into.
  The estimate survives, demoted: it picks which side is *tried first*,
  which is what keeps §4a's "just above the card". An estimate that is
  wrong now costs a column, never an option.
- **Measuring the panel after the layout ran was the obvious fix and was
  rejected.** `ComputedNode` is written in `PostUpdate`, so a measured
  placement is a frame late by construction: it needs `Visibility::Hidden`
  and a reveal, which is a flicker on the most-used click on the board.
  It is also untestable here — `tests/game.rs` runs `MinimalPlugins` with
  no `UiPlugin`, so nothing is ever measured — and it would not even
  remove the guess, because the wrap below needs a `max_height` *before*
  the layout regardless.
- **The entries wrap into a second column when that room runs out**, so
  every option stays on the window. **A scrolling list was asked about
  and turned down**, in both shapes: an anchored menu that scrolls when
  tall, and a centred pop-up holding a scrolling list. A scroll bar makes
  an option *reachable*, not visible, and hides options in exactly the
  case being complained about; the centred variant also gives up §4a's
  anchor, which was settled after a pointer-anchored first cut was
  rejected for jumping around. No wheel fallback either, so no scroll
  container appears on the board and the dev log's `scroll area` check
  keeps its meaning.
- **The panel is itself the wrapping container.** An inner entries
  container cannot be: *its* auto width is measured under a max-content
  constraint, where taffy never wraps — which is the failure the pop-up's
  cards already recorded. The panel is `position: Absolute` and so is
  measured under a *definite* available space
  (`taffy/src/compute/flexbox.rs:2225`), where it does wrap, and its auto
  width comes out as the wrapped width. Hence `px` widths on every child,
  and `entry_button` taking its width from the caller. With two columns
  the heading sits atop the first rather than spanning both.
- **The pop-up is capped at the window and its cards give first.** The
  panel takes `max_height: window - 2·PADDING`; every text node and every
  button is rigid (`widgets::button` already sets `flex_shrink: 0.0`) and
  the card row alone may lose height, with `min_height: px(0)` and a clip
  to defeat flexbox's content-based minimum. This makes structural what
  `choice_faces` already said in words. Its chrome estimate also stopped
  under-counting — `layout::wrapped_lines`, deliberately over-counting at
  a wide glyph's advance, now measures the title, the detail *and* each
  button's label — but the cap is the guarantee and the estimate is only
  a first guess. Re-choosing the face size from a measurement was
  rejected: it needs a respawn keyed to the prompt, a visible resize a
  frame after every pop-up opens, and a fixed-point argument, all to buy
  card pixels.
- **A resize now rebuilds the rail's floating panels**, because the
  pop-up is sized from the window; the menu keeps its own entity and is
  re-anchored in place by `place_menu`, since a respawn would take the
  hover and the pressed state of the button under the pointer with it.
  `place_menu` skips a target whose `ComputedNode` is empty — not yet
  laid out, which headless is every frame — so a zero-size box never
  drags the menu into a corner.

**Verified.** `cargo test --workspace` green (1,633 tests), clippy silent
across the workspace. Nine new tests: seven in `models::layout` — a hand
card's menu opens upward from the card, one at the top edge flips below,
a tall one is capped rather than pushed off, one near an edge anchors by
that edge, a target taller than the window gets the menu over it, and a
property test over a 16×16 grid of anchors at three window sizes
asserting the box is always inside the window (the old math fails it on
day one) — plus `wrapped_lines` never under-counting; and two in
`tests/game.rs`, one driving a menu through a moved card box and a
window resize, one checking the pop-up's cap, its one shrinkable row and
its rigid buttons.

Screenshotted, with the two new dev hooks that made it visible at all
(`NETRUNNER_MENU=most|top` and `NETRUNNER_WINDOW=WxH`; the old hook took
the first hand card with an action, which on the sample decks is a menu
of *one* entry on the window's bottom edge — the one case that never
overflowed): Palisade's four two-line install options above a hand card
at 1100×520, all four on the window; the same menu at 1100×300 **wrapped
into two columns**, still all four; the Runner's own menu unchanged; a
zone's menu; and a six-card selection pop-up at 2560×1600 with every
button present. Both chairs at forty decisions, and the only `scroll
area` logged in any run is the hidden log's at zero size. No engine file
changed, so no sweep.

**This closes the fragility recorded under §4n** (a skin may not touch
`Node` because `spawn_actions_menu` places itself from the hard-coded
height of a button): no drawn size feeds the placement any more. The
rule still stands, for its own reasons.

### 4ae. A card says what it will ask before it is played — DONE (20 September 2026)

`feat/play-preview`, §8 item 4, part (a). A report from play: Red Team —
"[click]: Run a central server you have not run this turn" — spends its
click and only then shows which servers are left, so a person who does not
remember which they ran finds out by paying, and cannot put it down. Every
card whose text opens on a choice has the same shape: `engine::play_event`
spends the click, pays the cost, moves the card to the heap and *then*
parks the decision. jinteki is no better at the card (costs are paid
before its prompt too, and its run events are not cancellable); what it
has is an undo, which is parts (b) and (c) of the item.

**The button now carries the question**: "Use Red Team — then asks: Run on
HQ / Run on R&D". `netrunner_client::board::preview::Asks` applies each
playable card's action to `netrunner_bots::determinize`'s sample of the
view with the engine's own `apply_action`, as `board::breaks` prices a
route, and reads the answers off the viewer's masked view of what came
back, labelled by the same `describe_action` the real prompt will use.
**Rejected: reading `allowed_servers` off the card's JSON and subtracting
`servers_run_this_turn`** — that is a second legality check in a client,
and it would miss whatever else forbids a run.

**Only a choice made from public information is previewed** — a server, a
card's own printed options. A selection of cards is not (a stack search
would list the sample's invented stack), and an action that moves
`rng_step` on the sample is dropped, because the sample's hidden half was
read. A preview that might be wrong is worse than none.

**Once per view, never per frame.** The terminal client builds an
`ActionMap` every frame, so the preview is not in `ActionMap::build`:
`ActionMap::annotate` adds it to the one map the desktop keeps for a view,
and both terminal clients hold an `Asks` beside their routes and word
their lists through `Asks::label`. No engine file changed, so no sweep.

### 4af. A move can be taken back: free while it has taught nothing, an undo that ends the rating after — DONE (20 September 2026)

`feat/take-back-and-undo-click`, §8 item 4, parts (b) and (c), stacked on
§4ae. The same report: Red Team has spent its click by the time it asks
which server, and the person "can't put the card back down and select a
different option". §4ae lets them look first; this lets them go back.

**It is a restore in the session, not a `PlayerAction`.** §4d declined a
back-out from an install's destination because the engine could only offer
one as a new action — `ActionSpace` 1646 → 1647 and every exported policy
with it, a variant the coverage gate would demand a bot apply, an
`affordance` mood for something that is not a move. None of that is needed
to put back a state the engine itself produced. `Session::with_undo(depth)`
is opt-in (self-play, the gym and the server keep nothing and clone
nothing); with it on, the state each of an `External` seat's moves was made
from is kept, `UNDO_DEPTH` = 4 deep as jinteki's `/undo-click` is, and
`Session::rewind` restores the newest and truncates `MatchHistory` to match
— so the record still replays to the state beside it, which
`the_record_still_replays_after_a_take_back` checks. The step budget is not
refunded. **§4d's back-out comes with it**, since an install a card's text
offers parks the same `ChooseServer`.

**The line is what the move taught**, which is where this parts from
jinteki: its undo is online, unilateral, and rewinds a draw or an access
with no test at all. Here a take-back is `Rewind::Free` while the person is
still answering what their own move asked — a decision or a paid choice
still parked on them, no run begun, no window open — and the move showed
them nothing new: `GameState::rng_step` has not moved, no event in it says
otherwise (`GameEvent::may_teach_the_actor`, exhaustive so a new event does
not compile until someone decides), the other seat has not acted, and the
prompt is not over a zone that shows hidden cards
(`CardZoneRef::shows_the_chooser_hidden_cards` — a stack search is a look
at the stack). Free leaves a rated game rated. Anything past that is
`Rewind::Undo`: it goes back just as exactly, and the rating is dropped
unrecorded — not forfeited — with `Ended::notice` saying why. Both
classifiers live in `netrunner_core` because they are facts about the
engine's events and zones; the session only reads them, and owns no rule.

**Undo stays inside a turn.** A move starts from a clear table in the
person's own action phase — nothing parked, no run, no window — so a step
of a run, a prompt's answer and a rez in the other player's turn are part
of a move, never the start of one; and every kept state is dropped when
the turn number moves or the game ends.

**In the clients.** `MatchHandle::rewind` and two messages:
`MatchMessage::Back { rewind }`, sent only when it changes so a client that
ignores it misses nothing, and `Rewound { view, removed, kind }`, on which
the board snaps back with no `Transition` (nothing moved *to* here) and the
log drops what no longer happened (`actions::pop_log_entries`). On the
desktop the button is "Take it back" or "Undo last move"
(`Game::back_label`), **inside the decision pop-up** as its last, unlit,
unnumbered button — the pop-up's wash blocks the rail, so the way out of a
prompt has to be in the prompt — and on the rail beside the routes, never
on the control bar, which is the engine's actions. `U` is the key. A free
take-back goes at once because it gives nothing up; an undo asks twice, as
Enter does with clicks left, until the rating is already gone. The terminal
client has the same `u`, its notice in the list's title. Lessons keep no
undo: their steps are scripted against the actions actually taken.

**Local games only, by choice; the rule is where the server can take it.**
A free take-back would be fair against a person — the opponent has seen
which card was played, which costs only the one taking it back — and needs
one `ClientMessage` and the retraction of a log entry from the other seat.

**Corrected the same day (`fix/local-play-is-casual`, Phase 3 §2): an undo
costs nothing, because nothing local is rated.** This entry made an undo
past the line end the game's rating; the rating it protected was a number
in a file the player owns, and the cost fell on practice, which is where
an undo belongs. Both clients now take either kind back at one press under
one label, `UNRATED_BY_UNDO` and the double-press are gone, and an undone
game is recorded like any other. **The line itself is untouched** —
`Rewind::{Free, Undo}`, the two engine classifiers and `tests/rewind.rs` —
because the paragraph above is still right: it is the rule a rated game
between two people will be held to (Phase 4 §5).

**Measured.** Undo is off everywhere but the two local clients, so the
claim is *no drift*: 192 random-vs-random games over `--all-matchups`,
seed 1, on pinned release binaries of `main` and this branch — reports
**byte-identical**. `cargo test --workspace` green, clippy silent, both
256-seed sweeps green. Not screenshotted: no dev hook reaches a parked
prompt with a move to take back; the model test drives both buttons.

### 4ag. A bug report is the game, as a file that replays — DONE (22 September 2026)

`feat/bug-report-replays`, §8 item 15. Glacier's spread ICE (Phase 5 §19)
and the traps (§20) were both reports from play, and each time the
matchup, the rung and the moment were described in words and rebuilt by
hand. Now the gear's options offer **Save a bug report**, which writes the
match so far to `<data dir>/netrunner/reports/<UTC time>-seed<seed>.jsonl`
(`NETRUNNER_REPORTS_DIR` overrides it). `netrunner_cli replay <file>` opens
it at the moment it was saved, from the person's chair.

**Nearly all of it already existed.** `MatchHistory::write_jsonl` and
`MatchRecordHeader::setup` are the record `--headless --record` writes and
`replay` reads, and `apply_action` being pure is what makes it replay bit
for bit. What was missing was a way for a client to reach the record.
`MatchHandle` moved its `Session` into the match thread and kept neither
the seed nor the decks.

**Actions, not bots.** The report holds both seats' actions, and a replay
never asks a bot anything. Re-running the bot from its seed would be
smaller and wrong: `Session::rewind` restores the state but not the bot's
own random stream, so after a take-back the bot plays a different game.
The header's new `bot` field (`RecordedBot`: chair, rung and style, with
`Level` serialized by its name) is for the person reading the report. It
is `serde(default)` and skipped when empty, so every headless record on
disk reads and writes as it did.

**A mirror, not a request.** `MatchHandle::record` reads a copy of the
history that the match thread brings level after every applied action and
every take-back (`play::mirror`: cut back to the session's length, then
extend). Asking the thread instead would have frozen the window for as
long as a `puct@512` bot thinks. `the_record_replays_to_the_board_the_person_sees`
replays the record at each of 80 decisions, with take-backs, and compares
it with the view the person was sent.

**A stall saves one by itself.** A stall is the report that matters most,
and a person may not think to press anything, so the screen saves the
moment `Stalled` arrives. The client's notices are drawn on the main
menu, not over a game, so where the report went is `Game::saved_report`,
shown on the options panel and the stall panel. It is also pushed to the
notices, so the menu repeats it after the game.

**`replay` opens a report at its end.** `--at start|end|<n>` and a
`--side` that is now optional: a record that names the bot a person
played opens at the end from the person's chair (`replay::opening`). A
record between two bots opens as it always did, from the Corp's chair at
the setup.

**The dev hook that stalled (§19's note) is fixed here too.** At a card
selection, `NETRUNNER_AUTOPLAY` confirms once confirming is listed and
otherwise adds a card not yet picked. Its wandering `applied % len` had
toggled one card on and off until the stall guard fired. Measured headless
on the default decks, seeds 1–8 at 300 decisions each: with the old choice,
seed 6 livelocked ("Runner spent 256 actions inside mutual_favor's prompt")
and seeds 2 and 7 spent 243 and 190 decisions in one prompt before the
count ran out. With the new one, every selection is a pick and a confirm.
`the_autoplay_finishes_a_card_selection_and_never_stalls_on_one` plays seed
6, and it failed on the old choice before it passed on the new one.

**A stopped match ends the autoplay** (`fix/autoplay-ends-with-the-match`,
the follow-up). The loop was one bug and the hang another. The hook's
screenshot and exit waited for `autoplayed` to reach its count, so any
game that stalled or simply ended short of it left the window open for
good. A stopped match now marks the autoplay done and logs why, so the
shot is of how the match stopped and the client exits.

No key: the gear is one click away, and a letter would be a new row in a
crowded list for something pressed once a game at most. The terminal
client can call the same `netrunner_client::bug_report::save`, but it is
not wired up here.

### 4ah. A saved game steps on the board, from either chair — DONE (22 September 2026)

`feat/desktop-replay-viewer`, §8 item 5. §4ag made a report a file that
replays, and the only way to open one was the terminal. The desktop's
`Replay` screen was still a stub, and no menu entry led to it.

**The replay moved into the client core, and the terminal kept its side
panel.** `netrunner_client::replay` owns what `netrunner_cli/src/replay.rs`
had: every position cached up front, each chair's copy masked by
`HistoryEntry::for_viewer` against the state that entry produced, and
`opening` (a bug report opens at its end from the person's chair). The
terminal wraps it with its `Coaching` panel and `RenderableView`. **The
log is now the live log's** (`actions::push_log_line` against the view
the action produced). It used to be a one-line wording of its own, so the
terminal's replay log now carries the narrated lines under each action.
`the_log_at_each_position_is_the_live_log_to_that_point` holds the two to
the same lines and the same view at every position of a game.

**The board is the game screen's, not a second board.** Picking a record
puts an `ActiveReplay` where an `ActiveMatch` would be, and
`AppScreen::Game` draws it with the systems it draws a match with. The
plan was a board that ran under two states, `Game` and `Replay`. It was
dropped once it was clear that nearly every system — the escape, the
secondary click, the sheets — would need to run in both anyway, and that
switching from the list to the board inside one state has no clean
`DespawnOnExit`. What a replay changes is the source and one flag:

- `Game::replay` is set, so nothing awaits and no entry, glow or decision
  is ever offered.
- A primary click reads a card, as a secondary click does: a menu of
  nothing would be a click that did nothing twice.
- Leaving asks nothing and goes back to the list.
- The control bar's row holds the replay bar instead: start, back, next,
  end, the other chair. The keys are the arrows, Page Up/Down for ten,
  Home/End and S, none of which the board's own keys use.

**One step on is played as the match played it.** The step's masked entry
and view go through the pacer as `MatchMessage::Applied`, so a run walks a
beat at a time and the board lights what moved. Every other move — back,
either end, a second step while the first is still paced, the other chair
— puts the board at the new position at once (`Intent::Show`). Nothing
moved *to* there, and twenty beats queued behind a held key would be the
replay setting the pace instead of the person.

**Where a replay comes from.**

- **Replays** on the main menu lists `bug_report::list`, newest first
  (name order, then modification time within a second, because
  `…seed42-2` sorts below `…seed42`).
- **Watch it** sits beside a report the board just saved, in the options
  and on the stall panel.
- `NETRUNNER_REPLAY=<file>` opens a record at boot, with
  `NETRUNNER_REPLAY_AT=<start|end|n>`, for screenshots.

A record the engine no longer replays says so above the list, naming the
entry where it diverges.

**Not built: notes** — item 5's second half. They need a file beside the
record and an editor, and nothing else here needed either. They stay
owed.

Checked:

- `tests/replay.rs` saves a report from a real game and opens it with
  Watch it. The board equals the view it was saved over. Back and Next
  follow the record's view and log. S turns the board to the Corp's
  chair, whose hand is face up. A click on a pile opens a sheet and never
  a menu. Escape closes the sheet, then goes to the list, then the menu.
- Screenshots from a heuristic record at step 112 (mid-encounter) and at
  its end: the board does not scroll.

### 4ai. One Continue, and it says to what — DONE (22 September 2026)

`feat/one-continue-names-the-next-step`, §8 items 7 and 12 together. The
Runner's bar had three buttons that each moved the game on — Pass
priority, Continue run, Complete run — and the Corp's one, and none of
them said where. Would the ice be encountered next, would its
subroutines fire, would the server be breached? A person pressed to find
out. Space was already the key a person leaned on (§4h), but it pressed
Pass priority or else Continue run, never Complete run, so breaching
needed a key of its own (A).

**At most one of the three is ever legal for a seat, so one button stands
for all three.** `PassPriority` needs an open paid-ability window, and the
other two need none. `CompleteRun` is legal only at `RunPhase::Success`,
where `ContinueRun` never is. `JackOut` is the only action that ever sits
beside one of them (beside `ContinueRun`, in the movement phase), and it
keeps its own button because it gives something up. So `Control` lost
`PassPriority`, `ContinueRun` and `CompleteRun` and gained `Continue`,
which matches any of them. Both bars end in it: the Corp's `[credit,
draw, purge, end turn, Continue]` and the Runner's `[credit, draw, remove
tag, end turn, Continue, Jack out]`. Space is Continue, and A is gone:
breaching gives nothing up.

**The button names the step that follows if nobody does anything else**
(`netrunner_client::board::onward`). The step is read off the masked view,
like the phase bar: the window's checkpoint, the run's phase, its
position, `jack_out_permitted`, and the encountered ice's pending
subroutines. Examples: "Continue to Approach ice 2 of 3", "Continue to
Encounter Ice Wall", "Let 2 subroutines fire", "Continue to Movement",
"Continue to the Runner's jack-out decision", "Breach HQ", "Begin your
turn", "Continue to your actions", "End your turn", "Don't prevent it". A
pass in a window the other player has not yet passed in hands them
priority first, and they may rez or break instead. The button still names
where the game goes if they let it, because that is what the person
pressing it agrees to.

- **The words ride on the entry, not in `describe_action`.**
  `onward::offered_label` words a legal action for a list of what can be
  done now: the button, the play helper, the terminal client's list (local
  and online). The log keeps `describe_action`, because a log line is worded
  against the view the action *produced*, and there "Continue to Encounter"
  names a step already taken.
- **The button's width is fixed** (`layout::CONTINUE_WIDTH`, 360 px, room
  for "Continue to Encounter Wall of Static", the longest in the pool).
  Otherwise the centred bar would shift under the pointer between two
  presses.

**Checked against the engine, not only against a table.**
`the_label_names_what_the_engine_does_next` plays twelve sweep seeds with
random and heuristic seats. Wherever a seat is offered one of the three
actions, it asserts three things:
- exactly one is offered;
- the view names a step;
- taking it on a copy of the game, and letting every window it leaves
  open close, lands on the named step.

At 64 seeds that was about 15,800 presses over all eleven kinds of step,
none wrong and 2 interrupted by a card's text.

**Found on the way:** a paid choice declined into "end the run"
(`DeclinePendingPaidChoice`, `RunEndedByEffect`; seed 7) left the run's
paid-ability window open with no run. `note_window_action`'s own comment
says that state must not happen. It cost both players a pass and never
deadlocked, since closing it resumes nothing. §4ai's button named that
pass "Continue to the Runner's actions".

**Fixed** on `fix/declined-paid-choice-closes-the-run-window`: a run's
window now ends with the run, in `run::end_run`, which every way a run
ends comes through. `note_window_action` had cleared a stale window only
for an action taken *in* the window, and a resolution of something
parked is not one. `declining_into_the_end_of_the_run_closes_the_runs_window`
pins it. The client's special case for the state is gone, so
`the_label_names_what_the_engine_does_next` now fails on any path that
leaves such a window: none at 128 seeds (~38,000 presses).

Measured on pinned binaries (`scripts/coverage_identical.py`, 192 games a
report, seed 1). Random-vs-random, by view and by index: `PassPriority`
43,007 → 42,865 and steps 74,778 → 74,577. Heuristic: `PassPriority`
58,449 → 58,541, steps 93,962 → 94,134, which is trajectory drift (every
game re-rolls after the first removed pass). End reasons are identical in
every shape: random 76 flatlines, 113 Runner agenda wins, 3 deck-outs;
heuristic 43 / 16 / 133. Both 256-seed sweeps are clean.

**Moved down: item 6, the Corp's run auto-pass.** Asked for 22 September
2026, because the person did not see what it buys. It matters only when
the Corp holds unrezzed ICE and the credits to rez it, so every window of
a bot's run stops and asks; §4e already takes a pass that is the only
action offered. It stays on the list, after the others.

Checked:

- `cargo test --workspace` green, `cargo clippy --workspace --all-targets`
  silent. The desktop's run tests reach a breach pressing Continue alone,
  and assert the button reads its step, never the bare word.
- No engine change, so no sweep and no coverage report is owed.

### 4aj. The ICE being encountered takes the run panel: its art, its type line, its strength and every subroutine — DONE (22 September 2026)

`feat/encounter-panel`, §8 item 13. §4ac put the state of an encounter
in the rail: the ICE's name and strength, and a marked line per
subroutine. It left out the ICE's subtypes, left the reader to guess
whether the strength had moved, and mixed the lines in with the prompt,
so every notice or prompt title pushed them down. jinteki.net keeps the
whole ICE in view for as long as it is encountered.

**The ICE takes the Runner's place in the run panel, and the Runner comes
back when the ICE is passed.** Top to bottom, the panel shows
"Encountering · <server>", the art from the ICE's scan, its name, its
printed type line ("Ice: Sentry - Bioroid - Destroyer"), its strength,
and every subroutine marked `[x]`/`[!]`/`[ ]` in §4ac's colours. The
rail's copy of those lines is gone, so the routes and Take it back sit
directly under the panel. It has its own skin slot, `panel.encounter`,
framed in the Corp's colour, with a row in the skins guide.
- **Stacking it under the Runner was rejected.** The column is 380 px
  wide and already holds the header, the phase panel, the prompt, the
  routes and the log. During an encounter the Runner's picture says
  nothing the lane does not.
- **Asked for:** placement and the strength wording were settled with
  the person before building.

**The strength uses the sheet's words.** `board::facts::strength_words`
is now the only wording, used by the ICE sheet, the breaker sheet and
the panel alike:
- "Strength 4" when unmoved;
- "Strength 7 now, 4 printed" when the table or a lingering effect has
  moved it, because that is why a break costs more than the card
  suggests.

`Encounter` now carries the run's server and `strength_line`. The
terminal client prints the same words, type line and strength, under the
encountered ICE.

**The picture is the art, cropped, not the top of the card.** An ICE
prints its text box above its art, so the band a Runner identity
gives (§4z's `IDENTITY_ART`) would be rules text here.
`layout::ICE_ART` is `[0.12, 0.54, 0.88, 0.94]` of the scan.
- It was measured off seven Null Signal Games frames (Brân 1.0, Bumi
  1.0, Biawak, Palisade, Mycoweb, Funhouse, Pharos). On those the art
  runs from about 0.51 of the height to the bottom border, between the
  type strip and the subroutine track.
- It also falls inside the older frame's art (Ice Wall's text box ends
  by 0.37).
- It is drawn unrotated, since a rotated `UiTransform` lays out as its
  unrotated box.

**The art gives first on a short window** (`layout::encounter_art`), the
same rule as the pop-up's cards. The panel's words and the rail's prompt
with one button take their room first, and the art is scaled whole into
what is left.
- Below 72 px the art is dropped, and the name is drawn large in the
  Corp's colour, as the Runner's is when no scan is cached.
- **What was seen at 1366×768 before this rule:** the full-width art
  pushed the rail's prompt and Take it back off the window. An ICE with
  more subroutines would have lost its own last lines too.
- **How room is worked out:** the header and phase panel are measured
  as laid out, and the panel's words are estimated at 40 characters a
  line. The estimate errs short.
- **Redraw:** when the header or phase panel changes height mid-encounter
  (a note line appearing), the panel is redrawn. Neither height depends
  on the panel, so it cannot oscillate.

**Verified.** `cargo test --workspace` green and clippy silent. New tests:
- `board::facts`: an unadvanced Ice Wall reads "Strength 1", advanced
  twice it reads "Strength 3 now, 1 printed", from both chairs, with the
  run's server.
- `layout`: the art is whole on a tall window, a whole-scaled thumbnail
  at 768 px, and dropped when the phase panel leaves no room.
- The desktop model test reads the strength line and the server off the
  Wall of Static encounter. The run-panel test asserts no ICE is drawn
  while the Runner is.
- The terminal render test finds "Ice: Barrier · Strength 3" under the
  wall.

As in §4ac, the drawing is verified by screenshot, because a live
encounter cannot be reached headlessly (`NETRUNNER_HOLD_ICE`, under a
scratch `XDG_DATA_HOME` with the phase panel on).
- **2560×1600, Runner:** Ansel 1.0's art, "Ice: Sentry - Bioroid -
  Destroyer", "Strength 4", three `[ ]`.
- **2560×1600, Corp:** the same ICE with `[x]`, `[!]` in red and `[x]`,
  the install choice its fired subroutine opened in the pop-up. This is
  the first desktop shot of `[x]`, which §4ac could not take.
- **1366×768:** a centred thumbnail from the Corp's chair. From the
  Runner's chair, three long subroutines leave no room, so the name is
  drawn large, with the prompt and Take it back under it.

In every shot the only `scroll area` logged is the hidden log's. No
engine file changed, so no sweep.

### 4ak. The phase panel sits at the foot of the right column, so the run panel above it holds still — DONE (22 September 2026)

**The person asked for it:** the phase panel (L) sat under the header,
over the run panel. Its height moves with the phase — a step label wraps
differently, the window's line comes and goes — so the Runner's art, and
since §4aj the encountered ICE, bounced up and down on every step of a
run. The column is now the header, the run panel, the prompt, the log
and the phase panel. The run panel's top is the header's bottom, which
nothing changes during a run. The prompt's room is what gives.

The encounter art is still sized against the header and the phase panel
together (`layout::encounter_art`'s `above`), because both are still
height the column spends outside the run panel. On a window short enough
for that sum to bind, the art's *size* can still change with the phase
panel's note line. Its *position* no longer does.

**Verified.** `cargo test --workspace` green and clippy silent. The
existing test that turns the panel off and on still holds. The run was
checked in two screenshots from the Runner's chair at 2560×1600
(`NETRUNNER_AUTOPLAY=200` with `NETRUNNER_HOLD_RUN` and then
`NETRUNNER_HOLD_ICE`). One is Zahya hacking into R&D and the other is
Whitespace being encountered. In both, the run panel's top edge is at
the same place, right under the header, and the phase panel is at the
bottom right. The only `scroll area` logged is the hidden log's. No
engine file changed, so no sweep.

### 4al. Identical rig cards are one card with a count, and split the moment they differ — DONE (22 September 2026)

Item 9 of §8. Three Smartware Distributors used to take three card widths
in the resources row, and a busy row overlapped (`layout::step`) sooner
than it needed to. Now copies nothing tells apart are one face with
"×3" at the start of its chip line, in both clients. The terminal's rig
line reads `Cyberfeeder ×2`.

**What "identical" means is the whole decision**
(`netrunner_client::board::rig::stacked`). It is the same card, the same
strength and counters, and the same once-per-turn state. Nothing is
hosted on either copy, and neither is hosted on anything. When the
client has an `ActionMap`, the entries on each copy must also match by
kind and by the words the menu shows, with the same mood. The action
labels name a card by title and never by copy (`install_label`), so the
comparison does not split copies that really are the same. **Rejected:
stacking by card alone.** A Cyberfeeder with its credit spent would sit
under one with the credit unspent, and a click would mean whichever copy
the stack happened to show. The offer check is the backstop for any
per-copy difference the named fields miss, so a stack can never hide an
action. This is the same rule `selection` uses to fold a second copy's
button into the first.

The first copy stands for the stack. A click opens its menu and a
secondary click reads its sheet. The stack is outlined if any copy is
lit. **The count is on the chip line, not on the face:** a scan that
lands replaces a text face's children (`card_images`'s
`despawn_children`), and a badge drawn there would have gone with them.
Rig cards are not drop places, so dragging is untouched.

**Verified.** `cargo test --workspace` green and clippy silent. The rule
has its own tests in `rig.rs`: two fresh copies stack in first-copy
order, and a counter, a spent once-per-turn, a hosted card, a copy hosted
elsewhere or an action offered on one copy splits them. The existing
three-rows test still holds. Screenshots were taken from both chairs
against `dashing_mad` at 1920×1080. The board laid out as before, and
the only `scroll area` logged is a hidden one of size 0. **Neither game
happened to install two copies of anything** (the Runner was flatlined
at turn 8; the bot Runner won at turn 16 with one Smartware Distributor,
one Detente and one Fermenter). So the "×N" line has been seen only in
the tests, not on a screen. No engine file changed, so no sweep.

### 4am. Damage names what it discarded, the HUD says "Core damage", and a trap's rez is not a move — DONE (22 September 2026)

**A report from play** (the person was the Corp, against the apprentice
Aggressive Runner). The Runner accessed a face-down Urtica Cipher and,
as far as the person could tell, took no damage, and the game kept
asking whether to rez the Urtica Cipher. The engine had done everything
right: the record holds `DamageTaken { Net, 2 }` and the discards of
Docklands Pass and Carmen. All three faults were in how the client said
it.

- **The log names the cards** (`actions::narrate_events`). The damage
  line folds in the `CardDiscarded` events `rules::damage::apply_damage`
  records after it, which is always one per point: "the Runner took 2
  net damage: Docklands Pass and Carmen discarded". Before, it said only
  "the Runner took 2 net damage", and a discard narrated nothing. Net
  damage leaves no count anywhere, so the names are the only record of
  it. Every other `CardDiscarded` stays silent as before. Brain damage
  now reads "core" in the log, as the card text already did
  (`prose::damage_word`).
- **The HUD's readout is "Core damage"** (CR 1.9.5e). Labelled
  "Damage", it read 0 after net damage and looked like a count of all
  damage. The glyph key stays `hud.damage`.
- **A trap's rez stays legal and stops being presented as a move**
  (`board::rez`). The person decided the engine keeps it legal, since
  CR 5.7.1b lets the Corp rez non-ice cards in every paid ability window,
  and that the client never nudges them toward it: a trap works face down
  and rezzing it only shows the Runner what it is. `gains_nothing` reads
  that off the card, as a non-ice card with no ability, standing effect,
  subroutine or credits and no trigger but its own access. In the pool
  that is exactly Byte!, Snare! and Urtica Cipher, and a test holds it
  there. Such a rez earns no glow, carries "(it works face down; rezzing
  only reveals it)" on its menu entry, and is no reason for
  `play::lone_pass` to stop. Before, an installed trap held the Corp in
  every window of the Runner's turn: the glow invariant test's seed 0
  already had one. **Rejected**, both raised in the discussion: refusing
  the rez in the engine (the person chose to keep it legal), and a rezzed
  card that stays masked (a rezzed card is public in every reader of the
  view, and a state they all had to learn would leak through the first
  one that did not). Nothing in the pool rewards the Corp for rezzing any
  card; the module doc lists what was checked.

### 4an. A Trojan sits on its ice, and its place in the program row is a ghost — DONE (23 September 2026)

Phase 7 §8 item 8. Before this, a Trojan (Botulus, Tranquilizer,
Chromatophores; in nine sample decks and one sweep deck) was an ordinary
program face in the program row. Its ice showed nothing, and where it
was hosted was said only in two sheets' text ("Hosts …", "Hosted on …").
The view already carried the link: `PublicInstalledRunnerCard::
hosted_on_ice` is an `InstallId`, never masked, so no engine change was
needed and no new action exists.

- **The Trojan is on its ice** (`board::rig::hosted_on`). It is a chip
  on a line of its own under the ice tile's band, showing the Trojan's
  title and counters. It is a button inside the tile's button, with the
  Trojan's click, sheet and glow, and a `Button`'s `FocusPolicy::Block`
  keeps the press and the hover from the tile. It is inside the tile, so
  the tile keeps the height `layout::tile_stack` gave it. It is on its
  own line so the ice's title keeps the band's width. A first cut put it
  in the band, and the band squeezed Whitespace's title to "rezzed · str".
- **Its place in the program row is a ghost** (`board::rig::is_ghost`).
  The face is drawn at `GHOST_ALPHA` (a wash over a text face,
  `ImageNode` alpha on a scan, re-applied every frame by `fade_ghosts` so
  a scan landing later is faded too). It is **still the Trojan's
  button**, so the Trojan has two ways in to one menu. Its chip line says
  where it is, "on Tithe" or "on unrezzed ice on HQ"
  (`board::rig::host_label`, which names the ice only when the viewer
  may).
- **The terminal client says the same.** A server line adds
  "[hosts Botulus]" after the ice, and the program row's entry says
  "on <host>", dimmed unless it can act.
- **Rejected:** an *inert* ghost, jinteki's. It would make one card in
  the row a picture where every other card is a click (§4g). Also
  rejected: a mark on the ice with the real card left in the row, which
  says where a Trojan is without putting it there. The person chose the
  clickable ghost.
- **Dev hook: `NETRUNNER_HOLD_TROJAN=1`.** The autoplay hosts a Trojan
  whenever the engine offers one and stops once one is hosted. Plain
  autoplay hosted one in some games and not in others, so a screenshot
  could not be taken on demand. Seated as the Runner on `dashing_mad`,
  it hosted Botulus in each of three runs: on an unrezzed HQ ice once,
  and on a rezzed Tithe once. The ghost reads visibly dimmer than its
  row neighbours, and the dev log's only `scroll area` line is the
  zero-size one every board shot has. **The Corp's chair was not shot.**
  The bot Runner decides whether a Trojan gets hosted, and no hook drives
  it. The Corp's chair draws the same `spawn_tile` and `spawn_rig` at
  `layout::OPPONENT_SCALE`.
- Tests: `board::rig`'s `a_trojan_is_listed_under_the_ice_that_hosts_it_and_no_other`,
  `a_hosted_trojan_is_a_ghost_in_the_program_row` and
  `a_trojans_host_is_named_as_the_viewer_may_name_it`; the desktop's
  `a_trojan_is_a_chip_on_its_ice_and_a_ghost_in_the_row` (chip inside
  the ice's tile, the ghost still a button with the same click, the
  secondary click on the chip opening the Trojan's sheet); the terminal's
  `a_trojan_is_listed_on_its_ice_and_its_row_entry_names_the_host`.

### 4ao. The timing of the turn and the run, as the rules chart it, with the step in play lit — DONE (23 September 2026)

Phase 7 §8 item 10. The phase panel (§4j) is coarse on purpose: three
steps a turn and five a run. So it cannot answer the question a person
learning the game asks: why may the Corp rez now and not a moment later?
The answer is a window, which is exactly what the panel leaves out. The
rules already chart all of it in CR 11.1.1's appendix: the Corp's turn,
the Runner's, a run, a breach and an access, with each window tagged P, R
or S.

- **`netrunner_client::board::timing`** holds the charts and says which
  steps are lit. It reads only the masked view (`phase`, `active_run`,
  `paid_ability_window`), as the phase bar does. More than one step can be
  lit, because a run happens inside the turn's "take an action" and a
  breach inside the run's success. How the view maps to a step:
  - **The discard window runs under `GamePhase::Action`** (`turn.rs`), so
    the window's checkpoint is what places it.
  - An action phase with no clicks left and nothing under way lights the
    phase's end (CR 5.7.1h), where the game waits for End turn. The first
    screenshot lit "take an action, while clicks remain" with none left.
  - An encounter lights its window while one is open, and otherwise the
    subroutines resolving.
  - Movement lights the window before the jack-out decision, the decision
    itself, or the rez window after it, read off `jack_out_permitted`.
  - A breach's `access_state` lights the breach step and the access step
    beside it.
- **The words are ours; the numbers are the rules'.** `rules/NOTICE.md`
  says the committed rules text is Null Signal Games', kept for
  implementing the rules and nothing more. So the client never shows a
  sentence of it. Each step is a few words of this project's with its
  `CR` number, and every number is a citation the `rules_citations` gate
  checks.
- **One line says why a lit step can jump a window.** The engine asks
  nobody about a window nobody could act in (Rules Conformance D3, D4).
- **Opened by T or a press on the phase panel**, which is now a button
  (`Click::Timing`). It is a reading surface: no Close button, and
  Escape or a click away closes it.
  - **Runner's turn:** two columns in a 1000-px panel, the turn with the
    breach and the access on the left and the run on the right. On
    this 2560×1600 screen it measured about 870 pixels tall, so it fits
    a 1080-pixel window.
  - **Corp's turn:** its one chart in a 560-px panel.
  - The chart redraws with every view, so it follows the game while it
    is open.
- **Rejected:**
  - expanding the phase panel in place, where the right column is too
    narrow for the run's 30 steps;
  - a key alone, since the panel is where a person looks when they wonder.
- **Only the desktop draws it for now.** The model is in `netrunner_client`
  so the terminal client can draw it too, but the terminal client has no
  help overlay to hang it on.
- **Dev hook:** `NETRUNNER_TIMING=1` opens the chart once the autoplay is
  done. It does not wait for the person's decision: the first shot was
  taken while the bot held the window, and the chart never opened.
- **Screenshots checked:**
  - the Runner's turn after a jack-out (the action phase's end lit, after
    the fix above);
  - a run held at an encounter with `NETRUNNER_HOLD_ICE=1` (the turn's
    action step and the encounter window lit together);
  - the Corp's turn.
- **Tests:**
  - five in `board::timing`: each turn window, each run phase with the
    turn's step, the breach and the access, no run chart on the Corp's
    turn, and (S) only on the Corp's draw and action windows;
  - the model's `the_timing_chart_opens_on_t_and_closes_like_a_sheet`;
  - the desktop's
    `t_or_a_press_on_the_phase_panel_opens_the_timing_with_the_step_in_play_lit`.

### 4ap. A card's "you may" can be answered Always or Never, and the answer is kept — DONE (23 September 2026)

Phase 7 §8 item 14, from jinteki's `core/optional.clj` autoresolve. The
jinteki FAQ's quietest complaint is answering the same prompt the same
way forty times a game. Here the repeat offenders are The Zwicky Group's
"draw 1 card", Cookbook's counter, Dewi Subrotoputri on every successful
run, Conduit and Superconducting Hub.

- **`netrunner_client::standing`** is a client policy with
  `play::lone_pass`'s shape:
  - When the person has answered a prompt for good, the client submits
    the `ResolvePendingChoice` that answer names, taken from
    `legal_actions`, and does not ask.
  - The engine still parks every prompt and hears every answer.
  - `Session`, the bots, the sweeps and `ActionSpace` see nothing new,
    and nothing under `netrunner_core` changed.
- **What counts is read off the card.** The engine has no `optional`
  flag: a printed "may" is a `PresentChoice` whose declined option is an
  empty `Sequence` with the text `""`. A prompt counts when all of these
  hold:
  - it is the viewer's own and names its card;
  - exactly one of its texts is `""`;
  - the card has a non-`OnPlay` trigger holding a `PresentChoice` with
    exactly those texts.

  That leaves out an event's own "may" (the person just chose to play
  it), an ability's, and the choices the engine builds by hand.
- **The key is the card and its printed clauses**, never a trigger
  index. So Dewi's two triggers are two answers, and Pantograph's scored
  and stolen triggers print one sentence and share one answer.
- **Always needs exactly one yes.** Mitra Aman's swap offers two, so the
  pop-up offers it Never only.
- **Paid choices always ask** (decided with the person). Net Shield's
  "pay 1[credit]" and the agenda-counter upgrades would spend something
  every time the cost happened to be affordable. That excludes about 17
  cards' `OfferPaidChoice`s. The free "may"s are about 20 cards.
- **Kept across games**, in `Settings::answers`. That is a top-level
  field, not a `DesktopPrefs` one, so the terminal honours it too. The
  alternative was a match-only answer, as jinteki keeps; it would put the
  same forty questions back at the start of every game.
- **Desktop:**
  - The decision pop-up grows a second, smaller question under its
    buttons ("Answer this card's question the same way every time:"
    Always / Never).
  - It is rigid under the pop-up's cap like every button. The cards give
    first.
  - Pressing one answers now and saves.
  - The settings screen lists every answer beside the rows, each with an
    Ask button, plus a Forget all button.
- **Terminal:**
  - The notice line reads "y: always, n: never" (`a` was taken).
  - An answer taken without asking is logged under the action it took.
  - Settings has an Answers row, where Enter forgets them all.
  - The menu now re-reads the file before opening Settings. Its copy
    from launch would otherwise have saved over answers a game had
    written.
  - Remote play sends a remembered answer as it sends a lone pass. It
    does not do so while the connection is down.
- **A take-back asks again.** Taking back an answer given without asking
  would otherwise restore the prompt and answer it again at once. So the
  prompt a rewind restores is always put to the person, in both clients.
- **Dev hook:** `NETRUNNER_HOLD_MAY=1` stops the autoplay at the first
  such prompt. Dewi's "you may" needs a full rig, which autoplay seldom
  builds. The reliable seat is the Corp on `brutal_efficiency`
  (`NETRUNNER_GAME=corp NETRUNNER_CORP_DECK=brutal_efficiency`):
  Send a Message stolen on turn 4 asks the Corp.
- **Screenshots checked:**
  - that prompt, with Always / Never under "Rez 1 installed piece of
    ice" and "Do not", inside the capped pop-up; the dev log's only
    `scroll area` line is zero-sized, not the board's;
  - the settings screen with two answers beside the rows.
- **Tests:**
  - seven in `standing`: a triggered may either way; two yeses are Never
    only; an event's and an opponent's prompt, a masked card and a paid
    choice all ask; only a legal action is taken; the file's shape;
  - the model's
    `an_optional_trigger_answered_always_is_answered_without_a_click`,
    including the take-back;
  - the settings model's forgetting;
  - the remote app's `a_remembered_answer_is_sent_without_a_key`.

### 4aq. The Corp can pass the rest of a run with one press — DONE (24 September 2026)

Phase 7 §8 item 6, from jinteki's `toggle-auto-no-action`. It was moved
to the end of the list on 22 September because the person did not see
what it buys, and they asked for it next once the rest of the list was
down to the larger items. What it buys: `play::lone_pass` already takes a
pass that is the only action on offer. A Corp holding unrezzed ICE and
the credits to rez it is offered a rez beside the pass in every window
of the Runner's run, so each of those windows stops and asks. Over six
Corp games against the bottom rung (seeds 0–5, the Corp installing
before anything else and never rezzing), one press per run passed **139
windows, 116 of which would have stopped the person**.

- **`netrunner_client::run_pass`** is a client policy with
  `lone_pass`'s shape. `RunPass` is a flag the client keeps. While it is
  on, the client submits the Corp's `PassPriority` from `legal_actions`
  and does not ask. `Session`, the bots, the sweeps and `ActionSpace` see
  nothing new, and nothing under `netrunner_core` changed.
- **Only a window is passed, never a question.** A pass is taken only in
  a run's paid-ability window with nothing parked. A prompt, a trace, a
  payment split or a prevention window still asks. "No more actions" is
  not an answer to a card's text, and the Corp is asked in a prevention
  window only when it could prevent something.
- **It lasts one run.** The first view with no run turns it off, so the
  next run asks again. A flag held across runs would wave through a run
  on a server the person meant to defend. jinteki keeps its flag on the
  run for the same reason. A take-back turns it off too, or the pass it
  took would be taken again at once.
- **Not taken: jinteki's "pass on rez"**, a setting that passes as soon
  as the Corp rezzes a piece of ICE. It gives up the rest of a window the
  person just used (a second rez, an upgrade) without asking them, and
  nobody asked for it.
- **Desktop:**
  - The rail offers "Pass for the rest of this run" under the run's
    prompt. It sits on the rail rather than the bar, for Take it back's
    reason: the bar holds the basic actions, and this is not one.
  - While the flag is on, the rail shows "Stop passing: ask me again
    this run" above "Opponent is thinking…", which is where the person
    is while the run goes by.
  - W is the key for both (`models::shortcuts`); P was taken by Purge.
- **Terminal:**
  - `w` does the same on both paths, and the notice line names it.
  - The local game's bot run does not wait for a key, so there the flag
    ends at the run's end or at the next question the run asks. Remote
    play reads keys between updates, so `w` also stops it mid-run.
  - A lesson never offers it.
- **Dev hook:** `NETRUNNER_HOLD_RUN_PASS=1` stops the autoplay at the
  offer, and `=on` presses it there. The reliable seat is
  `NETRUNNER_GAME=corp NETRUNNER_CORP_DECK=pork_chops`.
- **Screenshots checked:**
  - the offer at Tithe's approach on HQ, turn 8;
  - the flag on during a run on Remote 1, with Semak-samun passed
    unrezzed and the Stop button on the rail.

  In both, the dev log's only `scroll area` line is zero-sized, not the
  board's.
- **Tests:**
  - `netrunner_client`'s
    `the_rest_of_a_run_is_passed_and_the_next_run_asks_again`: six whole
    games; every pass taken is legal and in a run, none answers a prompt,
    and the flag never outlives its run;
  - the desktop model's
    `the_corp_passes_the_rest_of_a_run_with_one_press` (a window left
    to the person fails it; the next run offers the button again) and
    `a_run_pass_stops_on_a_second_press`;
  - the remote app's `w_passes_the_rest_of_a_run`.

### 4ar. Archives and the Heap show cards large enough to read — DONE (24 September 2026)

Phase 7 §8 item 20, asked for by the person because the cards in both
piles' sheets were too small to see.

- **The press first, because it was a bug.** Each face in a zone's
  sheet carried `Click::Inspect`, and nothing read it: a face is an
  unthemed `Button`, so its press never becomes a `Pressed` message,
  and the loop in `controls` that reads unthemed faces knew board cards,
  pop-up cards and the phase panel but not this. The item asked to
  check before resizing; the test written to check
  (`a_card_in_a_zone_sheet_opens_large_at_a_press`) failed on `main`
  with `inspecting` still `None`. The arm is added, and Escape still
  goes back from the card to the sheet under it.
- **The size.** `layout::PILE_FACE` is 220 px, up from `Thumb`'s 140:
  the widest that keeps four to a row in the zone sheet's 960-wide
  panel (four faces and three gaps are 898 of the 912 the padding and
  scroll bar leave — the dev log measured the box at exactly 912), with
  body text at 12.8 px on the text tier. It is drawn as
  `FaceSize::Board(220)`, so the text face scales and the scan is
  resampled for that width like any board card; no new `FaceSize`.
- **The cap.** The fixed 460 px box became `layout::pile_height`:
  as many whole rows as fit under the sheet's chrome in the window,
  **never more than three** (`layout::PILE_ROWS` — four by three before
  the scroll bar, asked for by the person so a large pile on a tall
  window is not the whole screen), so a row is never cut while there is
  room for it. Two rows at 1280×800 (622 px), three at 2560×1600
  fullscreen (936 px) and on anything taller; a window too short for
  one row gets what is left, and the wheel reaches the rest. The sheet
  is an overlay, so the board still never scrolls.
- **The scroll, measured.** The Corp's opening HQ (always five cards)
  at 1280×560, where the box holds one row: the dev log reads the box
  at 912×308 over content 622 tall, and six lines of the wheel move it
  to 313.6, the end, with the fifth card in view.
- **Dev hook:** `NETRUNNER_PILE=archives|heap|hq` opens that zone's
  sheet once the person's decision has arrived; `heap+card` (and the
  others) also opens its first card over it.
- **Screenshots checked:** the Heap from the Runner's chair after 80
  autoplayed decisions at 1280×800 (five cards, two rows) and
  fullscreen (twelve cards, three rows), and a card opened over it.
  The dev log's scroll areas are the sheet's box and a zero-sized one,
  never the board.
- **Tests:** the desktop's
  `a_card_in_a_zone_sheet_opens_large_at_a_press` (the Corp reads its
  own HQ: the faces are `PILE_FACE` wide, a press opens the card and
  sends nothing, Escape returns to the sheet) and the layout's
  `a_pile_sheet_holds_whole_rows_and_fits_the_window`.

### 4as. A card opened to read sits at the right, and the field stays in view — DONE (24 September 2026)

Phase 7 §8 item 21, asked for by the person with play between two
people in mind: one chair reads a card while the other may still be
playing, and a panel over the middle of the board hid the field the
reader was watching.

- **The split was already drawn.** `Game::dismissed_by_a_click_away`
  names the reading surfaces a click that misses closes: the sheet a
  secondary click opens (a card, an install beside its state, a zone's
  contents), a card opened over a zone sheet, and the timing charts.
  That predicate now also decides placement
  (`layout::Placement::of`), so there is still one list of reading
  surfaces. The decision pop-up is not an overlay and is not moved;
  the forms (options, keys) and the three panels that ask a question
  (the end of the match, a stall, the quit prompt) stay centred.
- **Where it sits:** against the window's right edge, its right edge
  level with the right column's (`PADDING` in), vertically centred.
  Placed by flex rather than by an absolute left computed from the
  window, so a resize moves the panel without a respawn;
  `Placement::left` states the same geometry for the layout test.
- **The wash is light behind a reading surface** — 0.3 against the
  0.75 a form or a question keeps — because a panel at the side under
  a 75% wash would still have hidden the field. It is still there and
  still the press that closes the sheet, so the board under it takes
  no click while the card is open; reading does not become a way to
  play through the sheet.
- **What it covers at the smallest window (1280×800):** a card alone
  (`SHEET_CARD`, 414 px) covers the right column and 24 px of the
  board's right edge — the status, the prompt and the log, which is
  where the reader's eye already is. An install's sheet (800) covers
  410 of the board's 866 px, about half, and a zone's (960, four
  `PILE_FACE` cards across, item 20's width, kept) 570, about two
  thirds; both leave the board's left side and the whole field
  under the light wash visible. Narrowing the zone sheet would have
  undone item 20's four across, so the widths are unchanged and only
  their place moves.
- **The card browser's inspector already sits at the right**, as a
  column beside the grid, and overlaps nothing; no change there.
- **Screenshots checked** at 1280×800 from the Runner's chair: an
  install's sheet (`NETRUNNER_SHEET=1`), the Heap
  (`NETRUNNER_PILE=heap`), a card opened over it (`heap+card`), and an
  end of match that stayed centred under the full wash. The dev log's
  scroll areas are the pile sheet's box and a zero-sized one, never
  the board.
- **Tests:** the layout's
  `a_reading_surface_sits_over_the_right_column_not_the_field`
  (1280, 1366 and 1920 wide: the card sheet covers the whole right
  column and at most 40 px of the board, centred it sat over the
  field, and no sheet leaves the window or reaches the board's left
  edge), and the desktop's sheet and form tests, which now assert the
  wash's placement and alpha (a reading surface at the right under the
  light wash, the options in the middle).

### 4at. A card's name in the match log opens the card — DONE (24 September 2026)

Phase 7 §8 item 17, from the jinteki comparison. The log was a list of
strings, so "Smartware Distributor triggered" gave the person a name to
look for on the board and no way to read the card.

- **A log line knows which cards its names are.** `actions::LogLine` is
  the line's text with the byte ranges that name a card, written by
  `push_linked_log_line` beside the terminal's `push_log_line` (the same
  words, so both clients' logs still read alike; the terminal has
  nothing to click and keeps its strings). The replay stores both.
- **Names are found in the words, but only among the cards the viewer
  was shown** (`actions::nameable_cards`): every card id in the masked
  entry the line was written from and in the viewer's own view after
  it, read off their serialized form. A link can only mark words the
  masked line already prints, so it opens nothing the viewer could not
  already read, and a title that reads as a word ("Ping", "Unity")
  links only when that card is in front of them. Longer titles win
  where two overlap, and a match must be a whole word. **Rejected:**
  threading the id out beside every title through `describe_action`,
  `narrate_event` and the helpers they call — a second return value on
  about seventy call sites, which a new log line would have to
  remember to fill in. This rule needs nothing from a new line.
- **On the board a name is a span of its line**, drawn in the accent,
  and a press with either button opens the card to read (the sheet at
  the right, §4as). A line is still one `Text`, so it wraps at word
  boundaries as before; `bevy_ui`'s picking hits a single span, so no
  row of per-word nodes was needed. The press arrives through picking
  as an observer (`Interaction` is set for nodes only), so the headless
  tests, which run without picking, never fire it rather than failing
  on a message nobody registered.
- **Screenshot checked** from the Runner's chair forty decisions in,
  with the play history on: "Smartware Distributor triggered
  (OnTurnStart)" shows the name in the accent. The one scroll area in
  the dev log is the log's own, never the board.
- **Tests:** `a_line_links_whole_names_the_longest_first` (whole words,
  longest first, spans put the line back together, a title nobody was
  shown stays words), `every_linked_name_is_its_cards_title` (a whole
  recorded game from both chairs: every link is its card's printed
  title, and some exist), and the desktop's
  `a_name_in_the_log_opens_its_card` (the Runner plays a card, the
  log's span for it is its title in the accent, and a press on it opens
  that card).

### 4au. The mulligan is a start-of-game box, and the end of the match has a table — DONE (24 September 2026)

Phase 7 §8 item 16, from jinteki's start-of-game panel and end-of-game
stats. The mulligan asked CR 1.6.6a's question with a title and two
buttons over a hand drawn as the board's peek, and the end of the match
said who won and why and nothing about how.

- **The start-of-game box** (`netrunner_client::board::opening`): at
  the mulligan, the decision pop-up draws both identities, captioned
  "You · <side>" and the other side, over the opening hand turned up
  whole, over the same Keep hand and Mulligan buttons. Every card is
  one size (`layout::opening_faces`, `choice_faces`'s search with a row
  of identities above the hand), read with a secondary click and never
  pressed. The line under the heading says what a mulligan does and,
  for the Runner, what the Corp did with its hand ("The Corp took a
  mulligan."), read off the Runner's own log. Nothing the viewer could
  not already see: the hand is its own `hq_cards` / `grip_cards`, the
  identities are public, and the Corp's choice is in the log.
- **The end table** (`netrunner_client::tally`): under "You win" /
  "You lose", a row per count with the Corp's and the Runner's numbers
  in fixed columns, the person's own side lit: mulligans, turns, clicks
  spent, credits gained and spent, cards drawn and installed, agenda
  points, then the Corp's rezzes and the Runner's runs, successful runs,
  damage suffered and tags taken. A row that is not about a side leaves
  its cell empty. **Counted from events in the chair's masked log, never
  from `GameState`**, so every number is one both players saw happen,
  and a card's credits count as the basic action's do. The desktop
  keeps its log's entries so a take-back recounts from what is left
  (`Tally::of`); a count cannot be un-added from a total.
- **No "Cards accessed" row, on a measurement.** The mask drops an
  access out of HQ, R&D or a remote from the Corp's log whole, because
  the card is the whole of the event, so in the same match (seed 0) the
  Corp's chair counted 16 accesses and the Runner's 26. A number that
  depended on the chair would be the one number on the table nobody
  could trust. Counting accesses for the Corp is a masking rule to write
  first. **"Credits spent" is `CreditsSpent`**: what a payment took
  that did not come off a card, a run's bad-publicity and event credits
  included, hosted credits not. That definition is pinned against each
  side's final pool: 5 + gained − (spent − run credits) − lost equals
  the credits on the board, in six heuristic matches.
- **Rejected:** a separate screen after the match (the result, the
  record's lines and Play again are already one panel, and the table
  fits under them at 1280×800), and counting actions rather than events
  (that missed every credit a card's text gave).
- **Seen:** the Corp's box at 2560×1600 and the Runner's at 1280×800
  (the Runner's box names the Corp's mulligan); the end table from both
  chairs after an autoplayed match, numbers right-aligned in their
  columns.
- **Tests:** `the_table_is_the_same_from_either_chair`,
  `the_credits_add_up_to_the_pool_on_the_board` and
  `a_match_fills_the_rows_that_matter` (tally, heuristic Corp against a
  heuristic or random Runner);
  `each_side_sees_its_own_hand_and_the_runner_hears_what_the_corp_did`
  (opening); `the_opening_box_fits_its_cards_and_both_identities`
  (layout); `a_take_back_takes_its_counts_off_the_tally` (model); and
  the desktop's `the_mulligan_shows_both_identities_and_the_whole_hand`
  and `the_end_of_the_match_has_its_table`.

### 4av. The glows can be seen by a colour-blind player — DONE (24 September 2026)

Phase 7 §8 item 18, from jinteki's Okabe–Ito palette. **The two moods
could always be told apart. The problem was each mood against the border
it sits beside.** A glow is a halo right against a card's border, and
that border is the card's faction colour, or the side's colour when the
card is face down. A glow the same colour as its border just looks like
a thicker border. The colours were measured with each pair's OKLab
distance under Machado et al.'s (2009) full-severity simulations of
protanopia, deuteranopia and tritanopia, and under normal vision:

- **The yellow glow on an NBN card: 0.03 for every viewer.** A playable
  NBN card showed no glow at all, whether or not the viewer was colour
  blind.
- **The purple glow on a Criminal card: 0.02 under protanopia**; against
  the Corp's back 0.05; against Haas-Bioroid 0.09–0.10 for everyone.
- Purple against yellow: 0.24 at worst (tritanopia), so the moods
  themselves were fine.

**Okabe–Ito itself does not fix this:** its reddish purple, yellow, sky
blue and orange each come within 0.04–0.07 of a faction colour under at
least one simulation. That palette keeps its eight colours apart from one
another, and here the other colours are the seven factions', which
between them take the whole hue wheel. **Lightness is what is left.** A
search over the purple and yellow families found a light lavender
(`0.76, 0.55, 1.0`) and a pale yellow (`1.0, 0.99, 0.70`). Each is at
least 0.12 from every faction, side and `danger` colour under all four
views, and 0.24 from the other mood. The moods keep their meaning and
their hue families, so AGENTS.md §5's "purple … yellow" still describes
them.

- `theme::tests::the_glows_survive_colour_blindness` holds the bound at
  0.10 and names the pair and the viewer that break it. On the old
  colours it names five pairs.
  `a_broken_subroutine_and_a_fired_one_are_told_apart` holds the run
  lane's filled dots, which differ only in colour (`accent` against
  `danger`, 0.14 at worst under deuteranopia).
- **Not changed, with the reason:** the transition outline (`accent`) is
  0.07 from Shaper's green under tritanopia. The outline is a sharp line
  drawn one pixel off the border, not a halo on it, so its shape already
  sets it apart. Moving the whole client's accent colour to fix it is a
  separate decision. The terminal client's `Magenta` and `Yellow` are
  the terminal's own palette and are not the client's to measure.
- Screenshots on both chairs (Fine Print for the NBN board): the
  lavender reads on server plates and the Stack, and the pale yellow on a
  run's plates and pop-up buttons. No `scroll area` line is the board.

### 4aw. Menus are glass and buttons are pills, and the new-game form is laid out as choices — DONE (24 September 2026)

Asked for by the person: the menus looked "like a 90s game" — opaque
grey panels, small rectangular buttons, a teal accent — and the
new-game form was five 14 px drop-downs in a row at the top of the
screen. The ask was semi-transparent blue, oval buttons, roomier menus,
and one look across every button and menu.

- **One look, set in two files.** `theme.rs` gains the glass tokens
  (`glass`, `glass_strong`, `glass_border`), three button kinds' colours
  and the backdrop's; the accent moves from teal to a light sky blue.
  `widgets::panel` is translucent glass with an 18 px corner and a soft
  shadow; `widgets::button` is a pill (`BorderRadius::MAX`, 44 px least
  height), and `styled_button` names a `ButtonKind` — `Primary`
  (filled), `Secondary` (a tint over the glass), `Quiet` (no fill until
  hovered). Every screen built from the widgets took the look with no
  edit of its own.
- **Something behind the glass.** No backdrop picture is committed, so
  translucent panels would have sat on the flat ground. Every screen root
  but the board's now carries a `BackgroundGradient` — a deep blue to
  near black with two soft blooms (`nav::drawn_backdrop`) — as the drawn
  tier under the existing backdrop slot; a picture installed there draws
  over it.
- **The new-game form is laid out as choices.** The side is two cards in
  the sides' own colours; the rung and the style are rows of pills with
  the chosen one filled, the suggested rung marked and the chosen rung's
  description under the row; the two decks — lists too long for pills —
  stay drop-downs, each with its identity and style under it. Start is
  the one filled pill, Back is quiet. The model is unchanged
  (`StartMenu`, now exposing `suggested`, `own_decks` and
  `opponent_decks`).
- **In the game:** the decision pop-up, the actions menu and the sheets
  are `glass_strong` (readable over cards); the wash is the backdrop's
  blue rather than grey, at the same alphas; Continue is the control
  bar's one filled pill; `compact_button` is a small pill. The board's
  own chrome is not touched. `widgets::panel` keeps 16 px padding
  because `POPUP_PADDING` and the sheet widths count it; menu screens
  use `roomy_panel` (28 px).
- **A bug found on the way:** a drop-down's chosen row lost its
  highlight after the pointer passed over it — the comment in
  `button_feedback` said it carried its own colour, and it did not. An
  undressed themed node may now name its resting colour (`Resting`).
- `theme::tests::the_accent_is_not_the_corps_blue` holds the new accent
  at least 0.10 OKLab from the Corp's blue under all four views, because
  the accent rings the encountered ice and a changed card, whose border
  may be the Corp's. The glow and broken/fired-dot tests still pass.
- Screenshots: main menu, new game, settings, and the end of a match on
  the board. The board's layout is unchanged, and no `scroll area`
  line is the board.

### 4ax. The client wears the Android cover's colours, and a drop-down's list stays on the window — DONE (24 September 2026)

Asked for by the person, with two reports. First, the colours: more
purple, in a cyberpunk feel, and specifically the blues, purples and
whites behind FFG's *Android: Shadow of the Beanstalk* cover, which
they held up. Second, a bug: the deck drop-down "goes below the
screen and there is zero way to scroll or anything like that". The
deck lists sit at the foot of the new-game form, and a list always
opened downward (`top: 100%`, capped at 55vh), so every deck past the
first few was below the window's edge.

- **The palette** is only `Theme::default`'s tokens, plus
  `backdrop_bloom_violet`. The backdrop is a steel-teal sky falling to a
  violet night, with a blue bloom high on the left and a violet one low
  on the right. The glass is indigo-violet. The primary pill is neon
  violet with white text, the secondary tints are lavender, the accent
  is the title chrome's icy white-blue, and the board's chrome is dark
  indigo. The sides' blue and red, the faction colours, danger and both
  glows are untouched, because they mean something on a card. A new
  test, `the_accent_is_not_the_usable_glow`, joins the colour tests,
  since both colours ring a card. Every colour test passes at the
  unchanged 0.10 OKLab bound.
- **A list is measured against the window as it opens**
  (`layout::list_box`, pure, with five tests). It opens below when the
  whole list fits there, otherwise toward the side with more room, and
  never taller than that side. At 2560×1600 the Corp's list of 19 decks
  opens upward in full; at 1280×720 the Runner's opens upward, capped,
  with a scroll bar. It is also never wider than the room to its right.
  `anchor_of` moved from the board to `widgets`, so both share it.
- **A long list is browsed, not only scrolled.** It has a bar beside it
  (`widgets::scrollbar`), shown only when the list will scroll. The keys
  move a highlight: ↑ ↓, Page Up and Page Down (8 rows), Home and End.
  Enter chooses the highlighted row. `keep_highlight_in_view` scrolls
  the list from the laid-out boxes, so a list opens at the chosen deck
  and the keys never leave the highlight out of sight. A click on the
  bar, or anywhere in the list's box, no longer counts as a click away
  that closes the list.
- **The list is opaque** (`glass_strong` at full alpha). At 0.92, the
  side card and the pills it opened over read through its rows.
- **Nothing else acts on the same keys while a list is open.** The
  search field ignores Enter as it already ignored Escape, and the card
  browser's grid ignores the arrows.
- **Tests and hooks.** `navigation::a_deck_list_is_browsed_and_chosen_with_the_keys`
  checks the keys headlessly. `NETRUNNER_DROPDOWN=<n>` opens the nth
  drop-down for a screenshot. It sends the widget's `Pressed` message,
  because an inserted `Interaction` is reset by the focus system before
  the widget reads it.
- **Screenshots:** the new-game form (open both ways, at two window
  sizes), the card browser with a filter open, and both chairs forty
  decisions in. None of the `scroll area` lines is the board.

### 4ay. The client ships its art: an unlicensed work is committed with credit and removed on request — DONE (24 September 2026)

**The person's decision, 24 September 2026:** the client ships the art that
makes it look like the game, the way jinteki.net ships the art on its table
— credited, and removed if an owner asks. Until now the register admitted a
third-party file only under a licence the repository could carry, which left
every backdrop slot empty.

- `assets/CREDITS.md` gains **"Art under no licence"**: such a row has the
  licence `All rights reserved` (`credits::ALL_RIGHTS_RESERVED`) and the
  licence text `—`, and still names the owner, the artist and where they
  publish. `tests/credits.rs` holds it to that; everything else keeps its
  licence text beside the file. The manifests (`*.json`) are now furniture
  like the guides, not a credited work.
- The first two: Kirsten Zirngibl's *New Angeles in Neon/Smog*
  (https://www.kirstenzirngibl.com/projects/RgWqA), the smog version as
  `backdrops/splash.jpg` and the neon one as `backdrops/main-menu.jpg`,
  dimmed 0.3 and 0.35 (`backdrops.json`) — 0.1 lost the splash's white
  wordmark in the white sky, and the default 0.45 greyed the art.
- The About screen credits her with the rest of the bundled assets, and
  says a work marked all rights reserved is shipped with credit and removed
  if its owner asks.
- **Rejected: a credit list for the player's own folder** (#179, closed
  unmerged). The person wants one complete package; `<data dir>` stays the
  player's own business and is credited nowhere.

### 4az. Both printings' card backs ship, and Settings picks one — DONE (24 September 2026)

The person asked for jinteki.net's choice of backs. Both are now committed
under the "Art under no licence" rule of §4ay, as the copies jinteki.net
ships in its own repository (`mtgred/netrunner`,
`resources/public/img/card-backs/<side>/{nsg,ffg}.png`), unchanged:
`assets/cards/backs/{nsg,ffg}/back-{corp,runner}.png`.

- `netrunner_client::settings::CardBacks` (`Nsg`, the default, or `Ffg`)
  is the Settings row **Card backs** and is on the gear menu's list too,
  since it is a board property. Changing it mid-game refills the two back
  handles in place, so every back on screen turns at once.
- A player's drop-in `cards/back-<side>.png` still beats either, and the
  drawn back is still the fallback.
- **The fetch is gone.** The backs had been downloaded from jinteki.net
  into the cache on the images opt-in (§4a); with both shipped, that path
  (`CardImageStore::download_card_back`, `card_back`, the two URL
  constants and their two errors) had no caller and was removed, and the
  "Fetched" credit row for the backs became four register rows.

### 4ba. The deck builder, Play vs Computer and Play Online have backdrops, the rest a painted circuit board, and the menu's title a shadow — DONE (24 September 2026)

The person picked the pictures. The three new ones are committed under the
"Art under no licence" rule of §4ay, each credited to its artist with
their website (`assets/CREDITS.md`), unchanged:

- **Decks and the deck editor**: Aurore Folny's *The Personal Touch*
  (Revised Core Set), `backdrops/decks.jpg`.
- **Play vs Computer**: Adam Schumpert's *Maya* (Kala Ghoda),
  `backdrops/new-game.jpg`, 1629 × 1200 from his own ArtStation. The
  first copy was Fantasy Flight Games' 2016 *Treasures from the Net*
  wallpaper, 1920 × 1080 but with the *Android Universe* mark in its
  corner; the person asked for one without it, and the artist's copy is
  the bare painting.
- **Play Online stands on Alex Kim's *The Root*** (Upstalk), `online.jpg`,
  which Cards and Learn to Play borrow by `same_as`; 1200 × 1059, the copy jinteki.net ships at
  `resources/public/img/bg/TheRoot.jpg` (no larger copy was found), so
  the main menu keeps *New Angeles*. The history of this, for
  whoever reads the credits: JB Casacop's *Dyson Mem Chip* was committed
  behind Cards first and removed at the person's request; the neon *New
  Angeles* then became the shared picture, whose near-white sky made the
  light text on Cards and Online hard to read, and *The Root*, dark and
  quiet, replaced it there. Alex Kim's website is his ArtStation, the
  account that posts *Traffic Accident*, which NetrunnerDB also credits
  to him.
- **Profile, Settings, Replay and About stand on a painted circuit
  board**, the shared `menu.jpg` (2560 × 1600), which the person asked
  for in the spirit of Fantasy Flight Games' card back: blue, red and
  green traces with a thin metallic rail, raised plates, depth. It is
  the project's own (GPL-3.0-or-later), painted by
  `scripts/paint_circuit_backdrop.py` from one seed — parallel buses
  routed on a grid with 45-degree bends, never crossing, a groove shadow
  under each rail, bevelled plates lit from above, and two lens rings
  off the middle, the Corp's blue at the left and the Runner's red at
  the right. It is darker than the silver back it echoes, because light
  menu text sits over it, and it is not a copy of the back.

**A screen borrows another's picture by name, not by a copy.**
`backdrops.json` gained `same_as` (`netrunner_client::backdrop::Manifest`,
one step, the dim still the screen's own), which the deck editor uses to
show `decks`. A decoded picture is cached under the key its files came
from, so two screens showing one decode it once.

**The main menu's NETRUNNER has a drop shadow** (`widgets::title_shadow`):
the neon *New Angeles* has a near-white sky, and the accent's icy blue
went thin on it. Bevy's `TextShadow` has no blur, so it is dark and close
(3, 4 px, 85% black).

## 5. The deck builder — DONE (24 September 2026)

`feat/desktop-deck-builder`. Asked for in one list: save and import
decks, a good view of the person's own decks with editing, every
built-in deck shown to copy and build from, each deck's legality, no
refusal to save a deck that is illegal or cannot be played, and the
person's decks offered in a game for either chair — the person's or the
bot's.

**What was built.**

- **The Decks screen** (`screens::decks`, `models::decks::Shelf`): every
  deck as a tile — the identity's card, the name, the side, the size, the
  style a bot plays it in, and where it stands in the format Settings
  names — the person's own under "Your decks", then the built-in ones by
  category (sample, Learn to Play, boosted, test). A saved tile carries
  Edit, Copy, Export and Delete (with a question first); a built-in one
  carries View, Copy to edit and Export. New deck asks the side, then the
  identity as its card, and opens the empty deck. A side filter narrows
  every section.
- **The editor** (`screens::deck_editor`, `models::deck_editor::Editor`):
  the identity, the name (Rename), the running totals against the
  identity's limits, the verdict and every other format the deck is legal
  in, and the bot's style as a row of pills, along the top; the pool as
  faces on the left (a press adds a copy, a count sits on each card the
  deck holds, faction/type/search filters, "Only <format>" and "Not
  playable yet" switches); the deck on the right, grouped by type, with
  − and + on each row and its influence in words. A secondary click on
  any card reads it at the right. **Every edit is saved as it is made**,
  the terminal builder's rule. A built-in deck opens read-only — its
  notes and its cards as a spread, with Copy to edit, which rebuilds the
  screen on the copy.
- **Import and export go through the clipboard, and a dropped file.**
  `deck_builder::import` reads this project's deck file or a text list in
  the shapes NetrunnerDB, jinteki.net and a hand-typed list use ("3x
  Title", "3 Title", "Title x3", sets in brackets, influence dots, section
  headings, accents and apostrophes folded); `export_text` writes the
  NetrunnerDB shape, and every sample deck round-trips through the two. A
  `.txt` or `.json` dropped on the Decks screen imports the same way, in
  place of a native file dialog. Bevy's `system_clipboard` feature
  (arboard) is the one dependency added, and it brought the Boost licence
  into `deny.toml` for arboard's Windows backend.
- **The shared rules are in `netrunner_client::deck_builder`**, so the
  terminal builder and this one cannot drift: `status` (the two
  validators sorted into *legal*, *illegal in the format* and *cannot be
  played*, with card ids replaced by titles in the validators' words),
  `Draft` (add with the copy limit, remove, identity, rename, style),
  `pool`, `identities`, `unique_id` (moved out of the TUI), `copy_of`,
  `import`, `export_text`. The terminal builder now takes `unique_id`
  from it.

**Decisions taken, with the alternative rejected.**

- **A deck is saved whatever its standing; starting a game still refuses
  an illegal one.** Asked, and the person chose to keep the refusal: a
  saved deck that cannot start a game in the format is *listed* in the
  new-game form, marked "not playable", with the validator's reason
  under it, and Start refuses it by name (`StartMenu::choice_problem`,
  and `DeckRow::problem`). Playing any engine-runnable deck casually
  against a bot was the alternative offered.
- **A catalog-only card can be in a deck.** An import from a list built
  elsewhere keeps a card the engine does not play yet under its
  `nrdb_<code>` id, and the deck reads "Can't be played" until the card
  is implemented, when `Draft::resolve` swaps the id on the next open.
  Dropping the card on import would have lost the person's list.
- **The deck lists read leniently** (`deck_store::list_lenient`): one
  malformed file in the deck directory used to empty the whole new-game
  form; it is now named and the rest listed, on both screens and in the
  terminal's form.
- **The influence is words, not dots:** the client's font has no `●`,
  `−` or `✓`, and the first screenshot drew three boxes. The deck rows
  read "· 2 inf" and the switches are ringed rather than ticked.

**Verified.** `deck_builder` 12 tests, `models::decks` 6, `models::
deck_editor` 5, and four navigation tests that drive the real plugins:
Copy to edit → editor → a pool press is on disk at once → Escape back to
the shelf; View opens read-only with the deck's cards and no pool; New
deck → side → identity → an empty saved deck; Copy to edit inside the
viewer rebuilds on the copy. Screenshots of the shelf, the editor on an
illegal saved deck and the read-only viewer at 2560×1600; the shelf and
the editor are menu screens and scroll, the board is untouched.
`NETRUNNER_DECK=<id>` opens the editor on a deck for a screenshot.

### 5a. A card in the deck builder can be read — DONE (24 September 2026)

`feat/deck-builder-readable-cards`. Asked for after the first use of §5:
the pool's cards and both identity pickers were too small to read, so
the person could not tell what they were adding or which identity they
were choosing.

**What was built.**

- **The grids' cards are three times the area.** The editor's pool and a
  built-in deck's spread were `FaceSize::Thumb` (140); they are now as
  wide as fills the grid's row with whole cards no narrower than
  `layout::DECK_FACE` (260), up to `DECK_FACE_MAX` (340) —
  `layout::deck_face`, read off the grid's laid-out width by
  `fit_pool`, which respawns the grid when it changes. At a 2560×1600
  window with a 1.28 scale that is five across at 306 logical px, where a
  fixed width had left 380 px of the row empty. The page's cap went from
  1700 to 2400 so a wide window gives the pool its width. The identity
  pickers (the editor's Change identity and the Decks screen's New deck)
  draw their identities at `DECK_FACE`, six across in a panel sized to
  them (`IDENTITY_PANEL`).
- **A hover shows the card large** (`widgets::preview`, `Previews`): any
  pool card, spread card, deck row, the header's identity or an identity
  in a picker shows its card in the half of the window the pointer is
  not in, two thirds of the window's height tall, never wider than a
  scan (750) nor than that half (`layout::preview_box`). It is a picture
  only — `Pickable::IGNORE`, no actions — so the press still adds a copy
  and the pool can be swept, reading, without a click.
- **The secondary click's card is the preview's size**, not
  `FaceSize::Large` (380): a card opened to keep is no smaller than the
  one glanced at.

**Decision taken.** A preview on hover rather than grid cards large
enough to read in place: a card's text is legible at about 600 logical
px, which is two cards to a row and a pool of hundreds as a long scroll.
The grid is for finding a card and the preview for reading it — the
split NetrunnerDB's deck builder makes. The board's sheets keep
`Large`; nothing on the board changed.

**Verified.** `layout` tests for `deck_face` (a row filled by whole cards
at five window widths) and `preview_box` (opposite the pointer, on the
window, at four window shapes); the desktop crate's tests and clippy.
Screenshots at 2560×1600 of the pool with a card previewed and of the
identity picker with an identity previewed, through two new hooks:
`NETRUNNER_PREVIEW=<n>` holds the nth previewable card previewed, and
`NETRUNNER_IDENTITIES=1` opens the editor's identity picker. A scan at a
new width is resampled once (§4w), so the first open of the pool at a
new size shows the text face for a moment until the copies are cached.

### 5b. A card is read by a right-click, centred; the hover preview is gone — DONE (24 September 2026)

`fix/deck-builder-read-centred`. **This corrects §5a**, whose hover
preview the person found annoying and far too large on first use: "Right
click is sufficient and center it — clicking off the card makes the
larger version go away like on the play area." §5a's larger grid cards
stay; its preview and its reading size do not.

- **`widgets::preview` is removed**, and `layout::preview_box` with it.
  Nothing appears on a hover.
- **`widgets::reader` reads a card on a secondary click** (the right
  button, or Ctrl/Cmd with the primary) over anything marked
  `Readable`: the pool, a spread, the deck rows, the header's identity
  and both identity pickers. The card is `FaceSize::Large`, the board
  sheet's size, **centred** over a wash at the editor's pop-up
  strength (0.72); a click off the card or Escape closes it and nothing
  else. It is one layer above every screen's pop-up (`GlobalZIndex(30)`),
  so an identity read from inside a picker closes back to the picker —
  the reason it is a shared widget rather than a variant of each
  screen's one-slot `Popup`, which is where the editor's `Popup::Read`
  used to live. A Ctrl-click reads and neither adds nor picks.
- **Centred, not at the right.** §4as put the board's reading surfaces
  at the right so a second chair can keep playing in view; a deck is
  built by one person, and the person asked for the middle. The board
  is untouched. `AGENTS.md` §5 records the exception.
- `NETRUNNER_PREVIEW` became `NETRUNNER_READ=<n>` (opens the nth readable
  card).

**One false lead, recorded so it is not chased twice.** The first
screenshots showed no dimming at all, and a debug pass logged the wash
at full size, visible and at its alpha. A solid red wash drew; so did a
translucent one. Sampling pixels settled it: a card's colour fell from
(245,211,190) to (163,140,127) under the 0.6 wash, which the eye does not
see at a glance on a navy background. The wash was working; it is 0.72
now to read as a pop-up.

**Verified.** `a_right_click_reads_a_pool_card_and_a_click_away_closes_it`
drives the real plugins: a right-click on a hovered pool card reads it
and does not add it, a press on the wash closes it, and Escape closes
it without leaving the editor. The desktop crate's tests and clippy.
Screenshots at 2560×1600 of the reader over the pool and over the
identity picker.

### 5c. Decks are sorted and filtered, and the pool is narrowed by set, format and playability — DONE (24 September 2026)

`feat/deck-sort-and-filter`. Asked for: sort the person's decks by
side, faction and format, and a card pool that filters by set and by
format legality, because "Only Startup / Not playable yet" would not
survive older sets or new ones.

**The Decks shelf** (`models::decks::ShelfView`): beside the side pills,
a Faction filter (the factions the shelf's decks are built on), a
"Legal in" filter (Any format, or each format, read off each deck's
`DeckStatus::legal_in`), and "Sort by" Name, Side, Faction (the
identity's) or Format (the first format in Settings' order the deck is
legal in, decks legal nowhere last). Each section is sorted on its own,
so the person's decks stay first. The view is kept across a visit to
the editor: the shelf is re-read on every entry, and a sort that reset
each time a deck was opened would be set again and again.

**The pool** (`netrunner_client::deck_builder::PoolFilter`): the two
switches became four drop-downs beside Faction and Type — **Set** (All
sets, or each set in release order), **Legal in** (Any format, or each
format; the Settings format by default), **Show** (Playable, Not
playable yet, Every printing — `Playability`) and **Sort by** (Type,
Title, Faction, Cost, Influence, Set — `PoolSort`). Clear resets the
filters and keeps the sort, and the sort follows the person from one
deck to the next.

**Decisions taken.**

- **The set order is read off the cards** (`deck_builder::set_order`):
  a NetrunnerDB code is the set's place in release order and then the
  card's number, so a set's lowest code dates it (Core 01xxx before
  System Gateway 30xxx). A table of set codes would be one more list to
  extend with each new set; this one extends itself.
- **Set and format are asked of the printing, everything else of the
  card it is.** The Core Set's Hedge Fund is a catalog-only entry
  (`nrdb_1110`) beside System Gateway's playable one; judged on the
  shown card, the Core filter missed it. Now the Core printing stands
  for the playable card, so a set's pool finds its reprints and a
  format admits a card through whichever printing it allows.
- **"Cost" is the top corner's number**: an agenda's advancement
  requirement, every other card's cost.
- **Playability is three answers, not a switch**, so "what is waiting to
  be implemented in this set" is one choice, not a filter the pool
  cannot express.

**Verified.** `the_pool_filters_by_set_and_playability_and_sorts_every_way`
(release order, a set's reprints, the two playability halves make the
whole, every sort keeps the same cards and orders by its key),
`the_pool_narrows_by_set_and_format_and_clear_keeps_the_sort`, and
`the_shelf_sorts_by_side_faction_and_format_and_filters_by_faction_and_format`
(every section ordered by its key, a faction filter, Startup leaving the
Eternal-only test decks out). Workspace tests for the two crates,
workspace clippy silent. Screenshots at 2560×1600 of the shelf with
"Sort by" open and the editor with "Set" open.

## 8. Borrowed from jinteki — OPEN (19 September 2026)

From [`docs/jinteki-comparison.md`](../jinteki-comparison.md) §5, in the
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
