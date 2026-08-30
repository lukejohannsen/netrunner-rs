# AGENTS.md — AI Engineering Guidelines for Cyberpunk Netrunner Engine

This repository contains an asynchronous, turn-based Netrunner card game built in Rust using a modular, decoupled architecture.

## Architecture Guidelines

1. **Decoupled Engine Rule (`netrunner_core`)**:
   - `netrunner_core` MUST be a pure, deterministic Rust library.
   - It MUST NOT depend on `bevy`, `tokio`, or any I/O framework.
   - All state mutations are deterministic transitions: `(GameState, PlayerAction) -> Result<(GameState, Vec<Event>), RulesError>`.
   - Never hardcode card rules in Rust functions. Cards are data-driven JSON objects parsed into AST primitives defined in `netrunner_core::dsl`.

2. **Server Architecture (`netrunner_server`)**:
   - The server is an authoritative host process running `netrunner_core`.
   - It validates incoming client `PlayerAction` intents, updates the global state, and broadcasts state deltas (`GameStateEvent`) back to connected clients.
   - Fog of War / Hidden State MUST be enforced at the server layer (e.g., hidden Runner hand cards sent as `None` / masked state to the Corp player).

3. **Client Architecture (`netrunner_client`)**:
   - Built using **Bevy Engine**. The client is a **dumb renderer terminal**.
   - The UI listens for state deltas from the server or local state machine and presents them visually.
   - User inputs (drag card, click credit, run server) generate a `PlayerAction` event; they NEVER mutate game rules state directly.

4. **Testing & AI Gym (`netrunner_gym`)**:
   - Evaluates card balances and runs headless self-play bot rollouts via Monte Carlo Tree Search (MCTS) or heuristic AI agents over `netrunner_core`.

---

## Code Style & Conventions

- **State Immutability**: Prefer returning fresh updated states or using controlled mutation wrappers in `netrunner_core`.
- **Serde Serialization**: All `PlayerAction` and `GameStateEvent` enums must derive `Serialize` and `Deserialize`.
- **Error Handling**: Use explicit `Result<T, GameError>` return types over `panic!` or `unwrap()` in engine code.
- **Bevy Plugins**: Split client rendering code cleanly into modular Bevy plugins (`CardRenderPlugin`, `RunPhaseUIPlugin`, `NetworkClientPlugin`).
- **Terminology**: Refer to the game as "Netrunner" and its current rules maintainer as "Null Signal Games" in code and comments — never "NISEI" (Null Signal Games' predecessor).

---

## Core Cargo Commands

### Build & Run
- `cargo build --workspace`: Build the entire monorepo.
- `cargo run -p netrunner_client`: Run the Bevy desktop game client.
- `cargo run -p netrunner_server`: Run the standalone headless server.
- `cargo run -p netrunner_gym`: Execute the AI Deck Testing harness.

### Testing & Quality
- `cargo test --workspace`: Run unit and integration tests across all crates.
- `cargo test -p netrunner_core`: Test engine rule logic and DSL card parsing.
- `cargo clippy --workspace`: Run the linter across the workspace.

---

## Claude Code Context Strategy

- When editing game engine rules or mechanics, operate strictly inside `crates/netrunner_core/`.
- When building UI components, visual card layouts, or drag-and-drop systems, restrict context to `crates/netrunner_client/`.
- Do not add visual rendering dependencies to `netrunner_core` or `netrunner_server`.

---

## Agent Workflow / Guidelines

- Agents MUST consult ROADMAP.md at the start of complex tasks and keep ROADMAP.md updated upon completing architectural changes or feature milestones.

---

## Project Context & Workflows

* Refer to `ARCHITECTURE.md` for core design principles, state rules, and evaluation models.
* Refer to `ROADMAP.md` for active priorities and remaining engine gaps.

---

## Open Engine Gaps & Next Steps

This section tracks open architectural gaps in `netrunner_core` to address before higher-level engine features build on top of them.

### Priority 2: Engine Windows & State Integrity

* **Expanded Paid Ability Windows — DONE:** Paid ability windows (`state::WindowCheckpoint`) now exist at run checkpoints `ApproachIce`, `EncounterIce`, `Success`, and `AccessingCard`'s `PendingChoice`/`PendingInteractiveTrigger` sub-phases (`SelectNextCard` intentionally excluded — no resource is at stake there), **and** at turn boundaries: `turn::end_turn` opens `WindowCheckpoint::EndOfTurn` before the mandatory hand-size check, and `turn::enter_start_of_turn` opens `WindowCheckpoint::StartOfTurn` after the mandatory draw and `Trigger::OnTurnStart` reactions resolve, before handing control to `GamePhase::Action`. Remaining gap: a generic "post-action" window (after any ordinary click action) doesn't exist yet — only the checkpoints above open one.
* **Generalized Prevent/Replace Effects — partially done:** `Effect::DealDamage`/`TrashCard` now park a `state::PendingPrevention` and open a `WindowCheckpoint::Prevention` window if (and only if) some installed/rigged card has a matching `Trigger::Paid` `Effect::PreventDamage`/`PreventTrash` ability — zero-overhead otherwise. Access-time avoidance/replacement (`InteractiveOnAccess`, `Effect::SetAccessReplacement`) are unchanged, deliberately left on their existing narrow mechanisms rather than folded into the new one. The generic "one side may pay X, or else Y happens" primitive this bullet used to defer now exists as `Effect::OfferPaidChoice` + `state::PendingPaidChoice`, built for System Gateway's *Funhouse*/*Manegarm Skunkworks*/*Anoetic Void*. The earlier analysis held up: it wanted a direct single-side decision, not `PendingPrevention`'s two-side priority-passing window. It ships alongside `Effect::PresentChoice` (choose 1 of N effects) and `PendingDecision::ChooseCards`/`ChooseServer` (zone search and target selection), all resolved by dedicated `PlayerAction`s and living in `rules::pending_choice`.
  * A parked choice blocks every other action, so `legal_actions::current_actor`'s precedence **must** mirror `engine::apply_action`'s blocking guards (trace → paid choice → decision → window → phase). Getting that wrong produces a player with no legal action at all, which is how it was originally found.
* **Dynamic Hand-Size Discard Re-checks:** `GamePhase::Discard` locks in a count on entry. Needs dynamic re-checking if mid-discard triggers alter hand size or max hand size.

### Priority 3: Server & Network Layer

* **Per-Viewer Event Masking:** `GameEvent` variants currently stream raw card IDs. While `PublicGameState` masks state snapshots, the event stream needs recipient-specific sanitization (e.g., stripping unrevealed `CardAccessed` IDs for the non-accessing player) before `netrunner_server` relays them.

### Priority 4: Data-Driven Cards & Schema

* **JSON Card Loading — DONE:** Cards are authored one-per-file as JSON under `crates/netrunner_core/data/{corp,runner}/`. `build.rs` concatenates them into `OUT_DIR` at compile time and `cards::embedded` bakes the result in via `include_str!`, so `cards::register_playable_cards` — the single entry point every consumer (`netrunner_cli`/`netrunner_server`/`netrunner_gym`/`netrunner_selfplay`/`netrunner_single_player`) calls — serves every playable card with no feature flag and no runtime I/O. The hardcoded Rust card builders (`cards/corp.rs`, `runner.rs`, `identities.rs`) and their JSON-vs-Rust parity test are **deleted**; JSON is the single source of truth, per this file's "never hardcode card rules in Rust functions" rule.
  * Printed metadata (faction, keywords, influence, deck limit, artist, set) is **not** restated in card files — `cards::embedded::fill_catalog_metadata` joins it from the embedded NetrunnerDB catalog on `numeric_id`. Card files own the join key plus everything the rules engine runs on. `printed_values_agree_with_the_netrunnerdb_catalog` guards drift between the two.
  * `every_system_gateway_card_is_implemented_or_explicitly_excluded` is the gate for calling a set complete: every printed card must have an implementation or an `SG_UNIMPLEMENTED` entry stating why. Add cards there rather than silencing the test.
  * `CardDefinition` and the DSL structs carry `#[serde(deny_unknown_fields)]`, so a misspelled key is a parse error instead of a silently defaulted field. Use `..CardDefinition::default()` in test fixtures rather than restating every field.
  * `cards::load_registry_from_dirs` (behind the `fs-loader` feature, still off by default) remains for **external** card directories — user homebrew, custom sets, iterating on card JSON without recompiling. It is not how the first-party sets load.
* **Schema: Memory Cost — DONE.** `Card::memory_cost: Option<u32>`; `engine::install_program` validates the caller-supplied `PlayerAction::InstallProgram::memory_cost` against it when set (mismatch errors `RulesError::MismatchedMemoryCost`), otherwise leaves the caller free to name any value (unchanged behavior for a card with no `memory_cost` declared). `RunnerState::memory_units` now seeds from a real `RUNNER_BASE_MEMORY_UNITS` constant at `GameState::setup` instead of defaulting to 0. `base_link`/identity-level MU overrides remain out of scope — no consumer reads a base link value anywhere yet.
* **Schema: Generic Counters — DONE.** `Card::counter_kind: Option<CounterKind>` (`Virus`/`Power`/`Credit`, descriptive only); `InstalledCard`/`InstalledRunnerCard` both gained a real `counters: u32` field; `Effect::AddCounters`/`RemoveCounters` (saturating, targets `acting_card` in either zone) are wired into `evaluate_effect`, and `Cost::RemoveCounters` spends them as an ability cost. Widely used across System Gateway — virus counters (*Botulus*, *Leech*, *Fermenter*), hosted credit pools (*Nico Campaign*, *Regolith Mining License*, *Telework Contract*), and the `Amount::HostedCounters` dynamic formula. Still not exposed through `masking`/`ClientView`: counters on an unrezzed Corp card would leak information, so that needs a masking rule before any UI shows them.
* **Schema: Hosting — DONE.** Built against System Gateway's Trojan programs (*Botulus*, *Tranquilizer*) once real cards needed it, exactly as this section's build-on-demand rule intends. `InstalledRunnerCard.hosted_on_ice: Option<CardId>` keeps a hosted program in `RunnerState.rig` — so all existing strength/counter/ability machinery applies unchanged — while recording which Corp ICE it is attached to. `CardDefinition.installs_on_ice` flags a Trojan, `PlayerAction::InstallProgramOnIce` installs one (the ordinary install path won't offer it), and trashing a host cascades to everything hosted on it. Upgrades hosted on ICE are still not modeled; no card needs it.
