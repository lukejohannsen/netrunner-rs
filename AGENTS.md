# AGENTS.md — AI Engineering Guidelines for Cyberpunk Netrunner Engine

This repository contains an asynchronous, turn-based Netrunner card game built in Rust using a modular, decoupled architecture.

**This file is the rules of engagement. `ROADMAP.md` is the single source of truth for status** — what is done, what is open, what is next. Do not track status here; add it to `ROADMAP.md` instead.

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

`netrunner_cli` (ratatui TUI) is the current reference client and the one to imitate.

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
| `netrunner_card_sync` | Async NetrunnerDB API sync and cross-platform disk caching — the only crate doing network I/O for card data. |
| `netrunner_rating` | Pure, engine-free Glicko-2 ratings: a `RatingBook` of one rating per track (human-vs-human, human-vs-bot, bot benchmark), participant and role, serializable whole. No I/O; the CLI's `bench` and the server own their files. |

Bot *logic* belongs in `netrunner_bots`, not in `netrunner_gym` or `netrunner_selfplay`; those are harnesses. `netrunner_session` is a **driver**, not a harness and not a rules authority — it owns the loop, never a rule.

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
3. **Watch the ratio, not the count.** Re-measured September 2026 at *Elevation* Stage 6, over the 147 card files in `data/{corp,runner}`: **30 of 68 `Effect` variants are used by exactly one card, and 5 by none** (Stages 1–6 added twenty variants for fifty-one cards and generalised one; the Runner half of the set front-loads its one-off mechanics, Stage 5's `ResolveSomeOf` is a composition primitive rather than a card, and Stage 6 grew an existing prompt by two fields where a third variant was the alternative — see ROADMAP Phase 1 §8 for which single-use variants have a second card coming). Before that, after the SG card-fidelity audit over 94 files: 21 of 48 single-use, 5 unused (`RemoveTags`, `RemoveBadPublicity`, `Trace`, `PreventDamage`, `PreventTrash` — the untouched prevention/tag block; the fidelity audit deleted two variants, `PermitJackOut` and `RandomFromHq`, and added two, `PromptInstallCorpCard` and `InstallRunnerCardFromGrip`, each with its reason on the variant). The widely-reused core is healthy (`Sequence` 55 cards, `PromptChooseCards` 39, `PresentChoice` 32, `GainCredits` 32, `EffectIf` 21, `EndTheRun` 19, `DrawCards` 19, `AddCounters` 19). If the next set adds single-use variants at a rate approaching its own card count, the DSL has started tracking cards rather than mechanics — stop and build a composition primitive instead of continuing.

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

**That the index path alone was not enough is settled, not theoretical.** Both bugs behind the "a run can outlive the game" entry in `ROADMAP.md` were reachable on ordinary sample decks at seeds 2, 3 and 6, and neither sweep-by-index could see them: the `ActionSpace` round trip does not reach the path. Do not delete the view-based sweep as redundant.

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

and `diff` the JSON against the previous report. Random-vs-random is the seating that reaches the most rules; a heuristic seating measures the bot as much as the engine. Quote the load-bearing deltas in the ROADMAP entry.

Heuristic seatings are byte-identical run to run since `determinize`'s pools were sorted (September 2026), so they are valid before/after measurements too — but reproducibility buys *attribution*, not *significance*: any code change re-rolls all 96 games, so a small heuristic delta can be pure trajectory drift. A change claiming a small heuristic effect must beat the **seed-spread band** recorded under `ROADMAP.md` Phase 2 §5, or show the effect across several seeds.

Related mechanical gates, all of which must stay green:

- `every_system_gateway_card_is_implemented_or_explicitly_excluded` — the gate for calling any set complete. Add cards there rather than silencing it.
- `printed_values_agree_with_the_netrunnerdb_catalog` — guards card JSON against the embedded catalog.
- `every_sample_deck_is_legal` / `every_sample_deck_matchup_finishes`.

`CardDefinition` and the DSL structs carry `#[serde(deny_unknown_fields)]`, so a misspelled card-JSON key is a parse error rather than a silently defaulted field. Use `..CardDefinition::default()` in test fixtures rather than restating every field.

The bar for any change: `cargo test --workspace` fully green and `cargo clippy --workspace --all-targets` completely silent. Both hold today.

---

## Core Cargo Commands

### Build & Run
- `cargo build --workspace`: Build the entire monorepo.
- `cargo run -p netrunner_cli`: Run the TUI client (local match vs. a bot).
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
- When building UI, restrict context to `crates/netrunner_cli/`.
- Do not add rendering or I/O dependencies to `netrunner_core` or `netrunner_server`.

---

## Agent Workflow / Guidelines

- Agents MUST consult `ROADMAP.md` at the start of complex tasks and keep it updated upon completing architectural changes or feature milestones.
- Status belongs in `ROADMAP.md` only. Do not reintroduce a status section into this file.
- Refer to `ARCHITECTURE.md` for core design principles, state rules, and evaluation models.
