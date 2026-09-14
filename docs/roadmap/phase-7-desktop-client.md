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
grows. The engine jobs were unmoved (ubuntu `test` 2m59s).

**Verified.** `cargo test --workspace` green (36 in `netrunner_client`,
12 in `netrunner_card_sync`, 10 in `netrunner_desktop`), clippy silent,
`cargo deny check` and `cargo machete` clean, the window opens to the
menu on the integrated GPU. Still to be driven by hand: each menu entry
and Escape back, Profile against a real ratings file, a name and a
format set here and read by the terminal client's Settings with the
`desktop` block surviving its save.

**Open, for §2 onward:** the `Sfx`/audio, tween, and card-face modules
exist only in the plan; the stubs name the phase that replaces them.
