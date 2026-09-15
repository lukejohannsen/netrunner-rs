# AGENTS.md — AI Engineering Guidelines for Cyberpunk Netrunner Engine

This repository contains an asynchronous, turn-based Netrunner card game built in Rust using a modular, decoupled architecture.

**This file is the rules of engagement. `ROADMAP.md` is the single source of truth for status** — what is done, what is open, what is next. It is an index; the per-phase record lives in `docs/roadmap/` (one file per phase, linked from the index), and an entry's address — "Phase 2 §5", "Rules Audit T8" — resolves through that index. Do not track status here; add it to the area roadmap and update `ROADMAP.md`.

---

## Architecture Guidelines

### 1. Decoupled Engine Rule (`netrunner_core`)

- `netrunner_core` MUST be a pure, deterministic Rust library. Its dependencies are exactly `serde`, `serde_json`, and `thiserror` — keep it that way.
- It MUST NOT depend on `tokio`, a rendering engine, or any I/O framework.
- All state mutations are deterministic transitions: `(GameState, PlayerAction) -> Result<(GameState, Vec<GameEvent>), RulesError>`.
- Randomness lives *inside* `GameState` as `seed` + `rng_step`, so `apply_action` stays a pure function of its two explicit inputs and any action history replays bit-identically. Never thread an external RNG into the engine.
- Never hardcode card rules in Rust functions. Cards are data-driven JSON objects parsed into AST primitives defined in `netrunner_core::dsl`.

**The one carve-out, already settled — do not re-litigate:** the `fs-loader` feature (off by default) adds `cards::load_registry_from_dirs` for *external* card directories (homebrew, custom sets, iterating on card JSON without recompiling). `std::fs` is the standard library, not an I/O framework, and it is only ever compiled in when a consumer opts into the feature. First-party sets never use it — they are embedded at compile time by `build.rs` and served by `cards::register_playable_cards`.

### 2. Server Architecture (`netrunner_server`)

- The server is an authoritative host process running `netrunner_core`. It owns the only real `GameState`.
- It validates incoming `ClientMessage::SubmitAction(PlayerAction)` intents against the engine and pushes `ServerMessage::StateUpdate(Box<ClientView>)` to every channel-backed seat.
- **These are full masked snapshots, not deltas.** `MatchSession::broadcast_state_updates` rebuilds a fresh per-side `ClientView` after every applied action. Deltas are a possible future optimization, not the current design — do not write code or comments that assume a delta stream exists.
- Fog of War / hidden state MUST be enforced at the engine boundary via `rules::masking` / `view::build_client_view`, never by asking the client to be polite. A seat receives a `ClientView` and a per-viewer `PublicHistoryEntry` (its copy of the action log, masked by `rules::masking::{mask_action_for_player, mask_event_for_player}` against the state that action produced) and nothing else. The raw `HistoryEntry` never leaves the host.

### 3. Client Contract (transport- and toolkit-agnostic)

No rendering engine is mandated. Any client — terminal, desktop, web — obeys the same contract:

- A client renders a `ClientView` and submits a `PlayerAction` **chosen from `view.legal_actions`**.
- A client NEVER touches `GameState`, NEVER re-derives legality, and NEVER mutates rules state directly.
- Anything a client needs to display must be reachable from `ClientView`. If it isn't, extend the masking layer with an explicit rule about who may see it — do not reach around it.

`netrunner_cli` (ratatui TUI) is the reference client for the *contract*; `netrunner_desktop` (Bevy) is the graphical one, and both stand on `netrunner_client` — see §5 for what that shares and the conventions the desktop follows.

### 4. Crate Map

| Crate | Role |
|---|---|
| `netrunner_core` | Pure deterministic rules engine, card DSL, embedded card/deck data, masking. Everything else depends on this; it depends on nothing. |
| `netrunner_bots` | Automated players over a masked `ClientView`: `BotAgent`, random/heuristic/MCTS/PUCT agents, `determinize`, RL observation encoding, optional ONNX policy. |
| `netrunner_session` | **The one match decision loop.** `Session` (pull-shaped: `step` → `SessionStep`), `Seat`, the single `MAX_STEPS`, `MatchHistory`, and `GameEndReason`/`classify_end_reason`. Every driver in the workspace pumps this. |
| `netrunner_single_player` | Thin index-based adapter over `netrunner_session` (`SinglePlayerSession`) for the RL/`ActionSpace` path. |
| `netrunner_server` | Authoritative async host: `MatchSession`, `ClientMessage`/`ServerMessage` protocol, WebSocket transport. |
| `netrunner_cli` | Reference client: ratatui TUI, headless runner, local and remote modes, card/deck subcommands. |
| `netrunner_gym` | PyO3 RL environment over the fixed `ActionSpace`. |
| `netrunner_selfplay` | High-volume self-play data generation for training. |
| `netrunner_card_sync` | Async NetrunnerDB API sync and cross-platform disk caching, and the card-image cache (`CardImageStore`) — the only crate doing network I/O for card data. |
| `netrunner_rating` | Pure, engine-free Glicko-2 ratings: a `RatingBook` of one rating per track (human-vs-human, human-vs-bot, bot benchmark), participant and role, serializable whole. No I/O; the CLI's `bench` and the server own their files. |
| `netrunner_client` | The toolkit-agnostic client core both clients stand on: the settings file (one struct, so neither client drops the other's fields), the saved-deck store, the local rating book and its rung suggestion, the format's card pool and the browser's catalog, the words for the DSL (`prose`, with `engine_reading`), the printed card as a face lays it out (`card_face` decides which corner a number sits in, `card_text` splits the text at its icons), the words on an action and in the match log (`actions`), the new-game form's state (`start`), and the match itself: `play::MatchHandle` runs a local `Session` on its own thread behind channels (the one interface every game screen consumes; `submit` never filters), `board::ActionMap` ties each legal action to the card or server a click would mean it by, and `board::diff` computes the `Transition`s between two consecutive views. No rendering, no rules. |
| `netrunner_desktop` | The Bevy graphical client. Renders `ClientView` only; a plugin per screen; consumes `netrunner_client`. Its own weekly CI job (`platforms.yml`), because Bevy is several hundred crates; the local `cargo test --workspace` is what checks it before a merge. |

Bot *logic* belongs in `netrunner_bots`, not in `netrunner_gym` or `netrunner_selfplay`; those are harnesses. `netrunner_session` is a **driver**, not a harness and not a rules authority — it owns the loop, never a rule.

### 5. Desktop Client Conventions (`netrunner_desktop`)

The graphical client is built so that the next screen looks like the last one. These are the rules a screen follows; `crates/netrunner_desktop/src/lib.rs` restates them where the code is.

- **One `States` variant and one `Plugin` per screen.** `OnEnter(AppScreen::X)` spawns the screen under `nav::screen_root(AppScreen::X, ..)`, whose `DespawnOnExit` takes the whole tree down; `Update` systems run under `run_if(in_state(AppScreen::X))`. A screen never sets `NextState` itself — it writes `nav::Navigate`, so Escape and Back are one rule in one place (`nav::back_from`). Nothing is spawned outside the root.
- **No rules and no `GameState`.** The board renders a `ClientView` and submits through `netrunner_client`'s `MatchHandle::submit`, which never filters by `legal_actions` (the Session Rule). Anything a screen needs to *know* comes from `netrunner_client`; anything it needs to *decide* is not its to decide.
- **Animations come from `netrunner_client::board::diff`'s `Transition`s**, computed from two consecutive views and the masked history entry — never inferred inside a system from what it sees on screen.
- **Every legal action is reachable without the board.** The basic actions are a fixed control bar (`netrunner_client::board::Control`, greyed when the engine does not list them), the prompt's decisions are buttons under the prompt (`ActionMap::decisions`), and a card's or a zone's actions are on the sheet its click opens, and again on the menu a secondary click (the right button, or Ctrl with the primary) opens just above the card — the same list, `models::game::Game::entries_for`, through two doors — so a card the layout cannot place is still playable, and a click on the board never submits by itself (a click meant to examine a card once installed it). The flat panel of every legal action is the play helper, off by default and an aid, not the contract.
- **The board never scrolls, and the game is fullscreen.** The play area holds every card and number on one screen: `models::layout::face_width` computes the card width the window has room for from the window and what is on the board, a row that is still too wide overlaps its cards (`layout::step`), and nothing on the board is ever a scroll container — a static card size put the person's hand below the fold twice, and a scroll bar was the wrong answer both times. The control bar sits centred along the bottom, beside the hand. Before a board PR, screenshot both chairs forty decisions in (`NETRUNNER_GAME`, `NETRUNNER_AUTOPLAY`, `NETRUNNER_SCREENSHOT`) and check the dev log's `scroll area` lines: none may be the board.
- **The board is the table seen from the person's chair.** Null Signal Games' setup is the layout: the Corp reads their own servers Archives, R&D, HQ, then the remotes, left to right, with a server's root above its header and its ice climbing away toward the Runner, outermost at the top; the Runner sees the same table across from them, mirrored — remotes, HQ, R&D, Archives, the ice coming down to them. `models::layout::{servers_left_to_right, column_top_down, ice_top_down}` are that rule, tested, so no screen decides an order itself. The Runner's installed cards have no position in the rules, so the rig keeps one order for both chairs.
- **Screen state that can be tested lives in `models/`**, as a plain struct driven by an `Intent` enum — the terminal client's state-struct-plus-`key()` pattern minus the key codes. The Bevy systems translate input into intents and draw the result. `tests/navigation.rs` drives the real plugin set under `MinimalPlugins`, so nothing in the test suite needs a window or a GPU, and CI sets up no display.
- **Bevy runs on the main thread.** A bot's search, a socket, an image download: each runs on `core::TokioRuntime` or a thread and reaches a system through a channel it `try_recv`s. `block_in_place` and `Handle::current()` panic off a tokio worker, which is why the terminal client's blocking `Reconnector` stays in the terminal client.
- **Assets come in three tiers.** A procedural or synthesized tier that always works (drawn card backs, synthesized sounds, the text card face), an optional file under `assets/` that is prettier, and a user override under `<data dir>/netrunner/assets/`. Card fronts are the exception with no first tier — they are downloaded from NetrunnerDB into the cache directory on request and **never committed**, and so are NetrunnerDB's icon font (the factions', sets' and symbols' marks; `Theme::symbol` falls back to the bundled Noto Sans Symbols 2, then to Latin-1) and the official card backs (fetched from the copies jinteki.net serves, put under the drawn back's handle when they land); nor is anything else whose license is not the project's to give (the Noto font ships under its OFL notice, and that is the bar).
- **The renderer asks for the low-power adapter** unless `WGPU_POWER_PREF` is set (`main.rs`): on a two-GPU laptop the discrete adapter can enumerate and then fail to present to the compositor's surface, and a card game does not need it.

### Session Rule

There is exactly one match loop, in `netrunner_session::Session`, and exactly one `MAX_STEPS`. **Do not hand-roll `current_actor` → `apply_action` → `GameOver` anywhere, including in tests** — five copies of it is what Phase 1.5 removed.

A seat is either `Seat::Agent` (resolved in-process from a masked `ClientView`) or `Seat::External` (the pump supplies the action). Sync vs. async is a property of *who pumps*, never a reason to fork rules flow. Two things follow, both load-bearing:

- **`Session::submit` does not re-derive legality.** `get_action_mask` is side-agnostic on purpose (`RezIce` is legal for the Corp during a Runner-priority window), and the RL env submits straight off that mask without consulting `current_actor`. Filtering `submit` by the awaiting side's `legal_actions` would reject actions the engine accepts and silently shift the training distribution. `apply_action`'s own guards are the only authority.
- **Only an applied action consumes budget.** A TUI polls `step` on its render tick and a server re-enters after a stray message; neither may exhaust `MAX_STEPS` by waiting.

---

## Code Style & Conventions

- **State Immutability**: Prefer returning fresh updated states or using controlled mutation wrappers in `netrunner_core`.
- **Serde Serialization**: `PlayerAction`, `GameEvent`, `ClientView`, `ClientMessage`, and `ServerMessage` must all derive `Serialize` and `Deserialize` — they cross a process boundary.
- **Error Handling**: Use explicit `Result<T, RulesError>` return types over `panic!` or `unwrap()` in engine code.
- **Terminology**: Refer to the game as "Netrunner" and its current rules maintainer as "Null Signal Games" in code and comments — never "NISEI" (Null Signal Games' predecessor).
- **Doc comments record decisions.** This codebase's comments explain *why* a design was chosen and what alternative was rejected. That is the house style — match it. A comment that only restates the signature is not worth writing.

### DSL Growth Rule

Adding an `Effect` or `EffectRequirement` variant is the expensive move: it grows the engine's permanent surface for one card's benefit.

1. First try to compose existing primitives. `Sequence`, `EffectIf`, `PresentChoice`, `PromptChooseCards`, and `OfferPaidChoice` cover most "new" card text.
2. If a new variant is genuinely needed, its doc comment must say in one line why composition didn't work.
3. **Watch the ratio, not the count.** Re-measured September 2026 with *Elevation* complete, over the 178 card files in `data/{corp,runner}`: **29 of 74 `Effect` variants are used by exactly one card, and 5 by none** — a *better* ratio than the 21-of-48 the set started from, over 84 more cards. The whole set cost twenty-six variants for eighty-two cards, with one generalised and two replaced in place (`RezInstalled` took over `RezInstalledIgnoringCost` when Mycoweb's discounted rez forced the free and paid rez paths to converge on `engine::rez_install`; `PlayOperation { from }` took over `PlayOperationFromHq` when Plutus replayed a transaction out of Archives). Stages 8 and 10 are the shape to aim for — nine cards and no new `Effect` at all, then eight cards and one — because their growth went into the vocabulary of *when* something happens (`Trigger`), *whether* it may (`EffectRequirement`) and *what it reads* (`Amount`, `CardFilter`) rather than *what it does*. Three variants also *left* the unused list as later cards found them (`RemoveTags`, `ForfeitAgendas`, `MillRnDAmount`'s neighbours), which is the other half of watching a ratio. Before the set, after the SG card-fidelity audit over 94 files: 21 of 48 single-use, 5 unused (`RemoveTags`, `RemoveBadPublicity`, `Trace`, `PreventDamage`, `PreventTrash` — the untouched prevention/tag block; the fidelity audit deleted two variants, `PermitJackOut` and `RandomFromHq`, and added two, `PromptInstallCorpCard` and `InstallRunnerCardFromGrip`, each with its reason on the variant). The widely-reused core is healthy (`Sequence` 71 cards, `PromptChooseCards` 55, `GainCredits` 39, `PresentChoice` 36, `EffectIf` 27, `DrawCards` 24, `AddCounters` 24, `EndTheRun` 22). If the next set adds single-use variants at a rate approaching its own card count, the DSL has started tracking cards rather than mechanics — stop and build a composition primitive instead of continuing.

### Linked Clause Rule

The printed card is the authority on what a card does, and the client shows the card's own words, never a rendering of the DSL. So a DSL node that a person will be asked to choose between carries the printed clause it implements, quoted from the card: `PresentChoice::texts` and `ResolveSomeOf::texts` (one per option, `""` for the "may" declined, which is always an empty `Sequence`), `OfferPaidChoice::text`, `AbilityDef::text`, and `SubroutineDef::text` (which always had it). `TriggeredEffect::text` is optional. `printed_clauses_are_quoted_from_the_card` (`crates/netrunner_core/src/cards/embedded.rs`) requires every sample-deck card's choices and abilities to carry a clause and checks each is a substring of the catalog's printed text, ignoring case and punctuation; a clause may end in ` — <note>` to tell two options apart that one printed phrase covers, and a card whose options are words it does not print goes on `CLAUSE_QUOTE_EXEMPT` with the reason. **That gate is also the erratum detector**: when a NetrunnerDB sync changes a card's wording, every clause the new text no longer contains fails, which is the list of cards whose DSL needs re-reading against the new text. `netrunner_cli::prose` renders the DSL as sentences for the one place both are shown together — the card inspector's "Engine reads it as" — and as the label for a card with no clause linked; it is not a substitute for the clause.

### State Hygiene Rule

Cross-effect context does NOT belong as new public fields on `GameState`. **`ability::ResolutionContext` is where it goes** — threaded through `evaluate_effect` and `check_requirement`, built at the top of a resolution and dropped when it ends. Add a field there, never a scratchpad field on `GameState`.

It currently carries the acting card, the triggering event (if the resolution is a trigger), and any cards a `DealDamage` discarded earlier in the same `Sequence`. Before reaching for a new field, check whether the answer is already in the triggering event: `WasFirstAdvancementThisCard` needed no field at all, because `GameEvent::CardAdvanced` already carries `advancement_tokens`.

**The test of where something belongs is whether it must survive a parked decision.** Anything read only within one resolution goes on the context. Anything a *deferred* trigger might read on a later `PlayerAction` has to be on `GameState`, because the context is gone by then — `last_completed_run` is exactly that case and legitimately stays a field. A deferred trigger rebuilds its context from `DeferredTrigger::event`, so keep that populated when queueing one.

### Testing Rule

Per-card tests verify a card. They do not find interaction bugs.

New mechanics MUST also be exercised through the two agent-driven sweeps. They hit the whole mechanic surface through real agents rather than scripted `apply_action` calls, and between them have caught five deadlocks and one crash that were reachable in ordinary play and invisible to every per-card test — each deadlock a state where a player had no legal action at all.

**They are not interchangeable, and both must run.** The split is the action shape each seat sees:

| Sweep | Seat shape | Covers |
|---|---|---|
| `no_panics_or_deadlocks_across_many_seeds_system_gateway` (`crates/netrunner_single_player/tests/system_gateway_delivery.rs`) | index-based `netrunner_bots::Agent` | the `ActionSpace` round trip and the side-agnostic `get_action_mask` — the RL path |
| `view_based_agents_never_reach_a_state_with_no_legal_action` (`crates/netrunner_session/tests/no_deadlock_sweep.rs`) | `netrunner_session::Seat::Agent` | `legal_actions_for` — the per-seat `ClientView` slice every real client gets |

**That the index path alone was not enough is settled, not theoretical.** Both bugs behind the "a run can outlive the game" entry in `docs/roadmap/phase-1-single-player.md` (Phase 1.5) were reachable on ordinary sample decks at seeds 2, 3 and 6, and neither sweep-by-index could see them: the `ActionSpace` round trip does not reach the path. Do not delete the view-based sweep as redundant.

**Run both deep before merging engine-level work.** Each deadlock found was one specific RNG path, so coverage scales with seed count. The default (32 seeds) is sized for the inner loop; raise it with `NETRUNNER_SWEEP_SEEDS`:

```bash
NETRUNNER_SWEEP_SEEDS=256 cargo test -p netrunner_single_player --release
NETRUNNER_SWEEP_SEEDS=256 cargo test -p netrunner_session --release
```

This matters because the range was once 8, and two of the deadlocks sat outside it — one of them live on `main` while the committed sweep stayed green. A failure names its `seed` and seating, so re-running just that case is a one-line override.

**Both sweeps play `netrunner_session::sweep_decks_for_seed`'s schedule** — the `seed`th Corp deck against the `seed`th Runner deck, each modulo its own list — so every sample deck is played within a few seeds however large the pool grows; the cross product `decks::matchups()` stays what self-play, `bench` and `--all-matchups` rotate. **Both sweeps also carry the rules-coverage gate** (`netrunner_session::Coverage::gate_failures`): across the sweep, every `PlayerAction` variant must be applied, every card of every sample deck the sweep played for at least eight seeds seen in play (`played_pool_card_ids` — at 256 seeds that is every deck), and every load-bearing `GameEvent` emitted. The report's per-card trigger counts (`triggers_fired`) come off `GameEvent::TriggerFired`, which the engine emits only when a trigger's requirement passed and its effects resolved — observed, never inferred from the event that would have offered it. Reachability — "the game ended" — is what let `InstallProgram` be silently unreachable for months, and let the whole encounter machinery go untested: heuristic-vs-random play never produces an `IceEncountered`, because the heuristic never runs and never installs ICE. That is why each sweep has a **random-vs-random seating** — the only unbiased one — alongside the two heuristic pairings that find deadlocks. A gate failure names the variant, card or event that was never reached; the fix is an engine bug, a bot blindness, or a *reasoned* allowlist entry (`ACTIONS_UNREACHABLE_WITH_SAMPLE_DECKS`, `ACTIONS_RARE_WITH_SAMPLE_DECKS` with its game-count threshold) — never a deleted assertion. Rare actions are demanded only at the deep seed count, which is one more reason the 256-seed run is not optional.

**Measure a rules change before and after it**, the way the memory-cost fix was measured ("0 program installs → 3"):

```bash
cargo run --release -p netrunner_cli -- --headless --all-matchups --games 96 \
  --corp random --runner random --seed 1 --report target/coverage/<branch>-random-random.json
```

`--all-matchups` plays `matchups[index % len]`, so **`--games` below `decks::matchups().len()` leaves the tail of the pool unplayed** — at 16 Corp × 12 Runner, 96 games stop eight Corp decks in and a whole deck's cards read as zero. Size the run to at least one pass of the cross product (132 today) whenever the point of the measurement is per-card coverage.

and `diff` the JSON against the previous report. Random-vs-random is the seating that reaches the most rules; a heuristic seating measures the bot as much as the engine. Quote the load-bearing deltas in the area roadmap entry.

Heuristic seatings are byte-identical run to run since `determinize`'s pools were sorted (September 2026), so they are valid before/after measurements too — but reproducibility buys *attribution*, not *significance*: any code change re-rolls all 96 games, so a small heuristic delta can be pure trajectory drift. A change claiming a small heuristic effect must beat the **seed-spread band** recorded under Phase 2 §5 (`docs/roadmap/phase-2-bots-and-training.md`), or show the effect across several seeds.

Related mechanical gates, all of which must stay green:

- `every_system_gateway_card_is_implemented_or_explicitly_excluded` — the gate for calling any set complete. Add cards there rather than silencing it.
- `printed_values_agree_with_the_netrunnerdb_catalog` — guards card JSON against the embedded catalog.
- `every_sample_deck_is_legal` / `every_sample_deck_matchup_finishes`.

`CardDefinition` and the DSL structs carry `#[serde(deny_unknown_fields)]`, so a misspelled card-JSON key is a parse error rather than a silently defaulted field. Use `..CardDefinition::default()` in test fixtures rather than restating every field.

The bar for any change: `cargo test --workspace` fully green and `cargo clippy --workspace --all-targets` completely silent. Both hold today.

**CI enforces that bar** (`.github/workflows/ci.yml`), and the `ci` aggregate check is **required on `main`** — a red run blocks the merge, so a branch is not landable until it is green. Every push and pull request runs both commands **on Linux only**, excluding the desktop crate, plus the security checks: `cargo-deny` (advisories, licenses, bans, sources — see `deny.toml`), gitleaks over the history, and zizmor and actionlint over the workflows. Everything that only *builds* — both commands on Windows and macOS, the desktop crate on all three platforms, `cargo doc` with warnings denied (one exception, below), each optional feature one crate at a time, the beta toolchain and `cargo-machete` — runs weekly and on demand in `.github/workflows/platforms.yml` and gates nothing, because the three-OS matrix was what every merge waited on. **So the desktop crate is checked by your local `cargo test --workspace` and `cargo clippy --workspace --all-targets`, not by CI**, and `gh workflow run platforms.yml --ref <branch>` is how to see Windows and macOS before a merge rather than on the next Monday. The two 256-seed sweeps run weekly and on demand in `.github/workflows/deep-sweep.yml`; they are still the thing to run locally before merging engine-level work, because a weekly job is not a pre-merge gate.

One rustdoc lint is allowed, and it is a domain collision rather than laziness: rustdoc reads Netrunner's printed symbols — `[click]`, `[c]`, `[credit]`, `[mu]`, `[trash]`, which these comments quote verbatim from card text — as intra-doc links, so `broken_intra_doc_links` is passed `-A` in the `docs` job. Every other rustdoc lint is denied, including `private_intra_doc_links`; **keep quoting card text, and do not escape the brackets.**

**Two deliberate omissions, so neither reads as an oversight.** There is no `cargo fmt --check`: this codebase has never been rustfmt-formatted, `cargo fmt --all --check` reports about 3,860 diff hunks, and it is not a width setting — 2,088 hunks remain at `max_width = 200`. Gating on it means a mass-reformat commit that buries every `git blame` in the repo, which is a trade to make deliberately or not at all. And nothing anywhere uses `--all-features`: it would enable `netrunner_gym`'s `extension-module` (which by design does not link `libpython`, so the test binary cannot run) at the same time as `ort`'s build-time binary download.

---

## Core Cargo Commands

### Build & Run
- `cargo build --workspace`: Build the entire monorepo.
- `cargo run -p netrunner_cli`: Run the TUI client — the main menu (Phase 6); side flags such as `--runner human --corp-level 3` skip straight to a game.
- `cargo run -p netrunner_desktop`: Run the Bevy client. Dependencies are optimised even in the dev profile (`[profile.dev.package."*"]` in the workspace `Cargo.toml`, because an unoptimised Bevy could not draw the card browser); the first build is slow and the client is then playable. `NETRUNNER_SCREEN=cards NETRUNNER_SCREENSHOT=out.png` boots into a screen, saves the window and exits — how to look at a screen from a session with no eyes; `NETRUNNER_SCROLL=x,y,lines` turns the wheel at a window position first and logs every scrolling node's content size. Run it through `cargo run`, not the binary in `target/`, or the fonts under `assets/` are not found.
- `cargo run -p netrunner_server -- --serve`: Run the standalone headless WebSocket server.
- `cargo run -p netrunner_selfplay`: Generate self-play training data.
- `cargo run --release -p netrunner_cli -- bench --bots random,heuristic,puct --games 12 --seed 1`: Rate every seating of a set of bots on the Glicko-2 benchmark ladder (`--report` for JSON, `--bots puct,puct-onnx --model X` to place a trained policy on it).

### Testing & Quality
- `cargo test --workspace`: Run unit and integration tests across all crates.
- `cargo test -p netrunner_core`: Test engine rule logic and DSL card parsing.
- `cargo clippy --workspace --all-targets`: Run the linter across the workspace.

---

## Claude Code Context Strategy

- When editing game engine rules or mechanics, operate strictly inside `crates/netrunner_core/`.
- When editing card behavior, prefer `crates/netrunner_core/data/{corp,runner}/*.json` over Rust — that is the point of the DSL.
- When building UI, restrict context to the client crate you are editing — `crates/netrunner_cli/` or `crates/netrunner_desktop/` — plus `crates/netrunner_client/` for what both share.
- Do not add rendering or I/O dependencies to `netrunner_core` or `netrunner_server`.

---

## Git Hygiene

These are this repository's own conventions, read off its history — not generic
advice. A commit here is a record someone will read in a year to find out why a
number moved, so the rules below are mostly about making that possible.

### Branches

- **One branch per landed idea**, named `<type>/<kebab-subject>`. The types in
  use are `feat/`, `fix/`, `docs/`, `chore/` and — distinctive to this repo —
  **`diag/`, a branch whose product is a *measurement* rather than a behaviour
  change** (`diag/policy-head-by-segment`, `diag/value-only-rerun`). Use it when
  the finding is the deliverable and the code change is scaffolding; it keeps
  "we learned this" out of `feat/`.
- Name the subject, not the activity: `feat/corp-pressure-terms`, not
  `feat/update-eval`. A long series splits by stage
  (`feat/elevation-stage-8-quick-returns-glyph-of-warding`).
- Branch from current `main`. If `main` moves under you, **rebase** — do not
  merge `main` into the branch, or the `--no-ff` merge stops being a single
  readable unit.

### Commits

- **One commit per branch** is the norm — every merge in recent history is a
  single commit. Squash locally before merging if the work took several.
- **The subject states what is now true, not what you did.** "A hand is a
  multiset", "The Corp had no run term at all", "PUCT over a static evaluator
  was choosing by noise: root-relative leaf values". Imperative
  "Add/Update/Fix X" is not the house style, and the 50-character rule is not
  observed — subjects run 55–110 characters, with a colon separating the claim
  from its mechanism.
- **The body carries the decision, the alternative rejected, and the
  load-bearing numbers** — before → after, with seeds and game counts — wrapped
  at ~75 characters. Same standard as a roadmap entry, because it usually
  becomes one. A body that only restates the diff is not worth writing.
- A change that corrects an earlier claim says so, in the commit and in the
  roadmap entry, rather than quietly replacing it.
- Never commit generated artifacts. Everything generated lives under `data/`
  (corpora, checkpoints, run logs) or `target/`, and corpora are deleted once
  the roadmap records what they showed.

### Merging

- Merge to `main` with **`git merge --no-ff`**, keeping the default
  `Merge branch '<name>'` subject, then push and delete the branch. The
  no-fast-forward is what preserves which commits were one idea.
- **The bar before merging** is the one in the Testing Rule above:
  `cargo test --workspace` green, `cargo clippy --workspace --all-targets`
  silent, and for engine-level work both 256-seed sweeps. A branch that claims
  a measured effect also carries the before/after numbers, taken on **pinned
  binaries** — see the Testing Rule for why. CI checks the first two on
  Linux for every crate but the desktop one, and runs the sweeps only at the
  default seed count: **the desktop crate and the 256-seed runs are still
  yours to do** before merging, and a green PR is not evidence they
  happened.
- Never force-push `main` and never rewrite published history.

### Pull requests

Local `--no-ff` merges are the repo's history to date and remain fine for solo
work. Use `gh` to open a PR when a change wants review before it lands, when it
is large enough that the diff is the discussion, or when the user asks. PR title
and body follow the commit rules above — the body is the commit body, since that
is where the numbers are. `gh` needs `gh auth login` first, which is interactive.

---

## Agent Workflow / Guidelines

- Agents MUST consult `ROADMAP.md` at the start of complex tasks, and the area roadmap it points to for the phase being worked on; keep both updated upon completing architectural changes or feature milestones.
- Status belongs in `ROADMAP.md` only. Do not reintroduce a status section into this file.
- Refer to `ARCHITECTURE.md` for core design principles, state rules, and evaluation models.
