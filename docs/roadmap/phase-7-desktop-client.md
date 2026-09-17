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

After this list: the transitions the board already computes turned into
movement and sound, which is what §4 has owed since §3. It comes after
rather than before, because tweens animate cards between positions and
this list moves every position.
