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
  the board (ICE is a rotated `UiTransform`). `bevy_feathers`/BSN are
  marked unstable and `bevy_tweening` is a version lock-in for thirty
  lines, so neither is used.
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

Seven PRs, in order: §1 foundations, §2 card faces and the browser, §3
the play area, §4 feel, §5 the deck builder, §6 lessons and replay, §7
online.

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
2. **A sole legal action should not need a click.** Passing priority
   over and over is the whole of a run from the other chair. The model
   can submit a lone `PassPriority` (and, arguably, any lone action) on
   arrival — a client policy, not a rule, and worth a short delay so
   the board is seen to change.
3. **The `operator` Runner ran Archives three times running**, into
   Urtica Cipher's damage, having seen what was there, and never drew a
   card — the stack stayed at 24 — until it flatlined itself. A bot
   blindness for Phase 5's ladder: the one-ply Runner's run choice does
   not read a known Archives, and its draw term is too weak to act.
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
    as a sheet's. The menu is centred over that box, not put at the
    pointer: the first cut used the pointer, and the person's verdict
    was that it jumped around with the click — a card's menu is in one
    place for that card.
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
