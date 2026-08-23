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

## Future Engine Considerations

Notes captured from Gemini-assisted reviews of `netrunner_core`'s access/turn logic (2026-08-23). The `GamePhase` state machine below has since shipped; the other three items remain open and are tracked before further engine work builds on top of those gaps.

### Per-Viewer Event Masking

`GameEvent::CardAccessed` (and every other `GameEvent` variant) currently emits unredacted card IDs. `rules::masking::mask_state_for_player` only masks `GameState` snapshots (`PublicGameState`) — there is no equivalent filtering of `Vec<GameEvent>` anywhere in the engine, and `netrunner_server` is still a stub. Once a server layer exists, it MUST filter/sanitize the `GameEvent` stream per recipient before broadcasting (e.g. strip `CardAccessed { card, .. }`'s `card` field for the non-accessing side when the accessed card wasn't revealed) rather than assuming raw engine events are safe to relay as-is. This matters most for HQ/R&D access, since those are hidden zones — Archives/Remote access was already effectively public.

### Interactive Access Resolution

`run::access_server` is intentionally scoped to determine *which* cards are accessed, not the effects of accessing them (steal an Agenda, pay to trash an Asset/Upgrade, resolve "on access" triggers) — see its doc comment. Two related gaps to close together once card-ability hooks exist:
- **Access order**: real rules let the Runner choose the order they resolve multiple accessed cards (e.g. central hand/deck card vs. a Root-installed Upgrade); `access_server` currently returns them in a fixed order.
- **Post-access actions**: steal/trash/pay-cost resolution needs a new `PlayerAction` (e.g. `InteractWithAccessedCard`) and likely a dedicated phase/sub-state rather than resolving everything synchronously inside one `access_server` call.

### Run Context During Access

`engine::complete_run` clears `state.active_run = None` *before* calling `run::access_server`. This is fine today since nothing during access needs to know "a run against server X is/was in progress," but once card abilities check active-run state during access resolution (e.g. cards that react to "while a run is in progress"), `complete_run` will need to defer clearing `active_run` until access fully resolves.

### Deck-out Win Condition (Shipped)

`turn::enter_start_of_turn` now checks for this up front: if control is passing to the Corp and `corp.r_and_d` is empty, `phase` transitions straight to `GamePhase::GameOver(Side::Runner)` (no clicks refilled, no `TurnStarted` — the turn never starts) instead of silently skipping the draw. Paired with `win::check_win_conditions`, which separately handles agenda-point victory (Corp or Runner reaching 7+ points, checked from `run::access_server` after a steal) — deck-out is deliberately *not* folded into that function, since it's a momentary event (a draw attempt that just failed) rather than a standing condition safely re-derivable from `GameState` alone; see `check_win_conditions`'s doc comment for why. Agenda detection itself is still a placeholder: `win::agenda_value` is a hardcoded lookup for a couple of fixture card IDs, standing in for the real `CardRegistry` this engine still doesn't have (same gap noted throughout this file).

### Dynamic Hand-Size Re-Checks

`GamePhase::Discard { required }` (see `turn::end_turn`) locks in a fixed discard count computed once on phase entry, rather than re-checking hand size after each discard. Real rules require discarding until at/under the max, which matters if a future trigger causes a draw mid-discard (not possible yet — no trigger system is wired into the engine). Worth revisiting once triggers exist.

### Asynchronous Start-of-Turn Windows

`turn::enter_start_of_turn` sets `phase = GamePhase::StartOfTurn(next_side)` and resolves its triggers (currently just the mandatory Corp draw) fully synchronously before advancing to `Action(next_side)` — no `PlayerAction` or external observer ever actually sees `StartOfTurn` as the current phase. This is fine while `StartOfTurn` has no triggers requiring a player choice, but cards like "gain 1 credit or draw a card" at turn start will need `enter_start_of_turn` to actually pause and return control mid-phase instead of resolving everything inline.

### GamePhase State Machine (Shipped)

`GameState::active_turn: Side` has been replaced by `GameState::phase: GamePhase`, fully implemented across `state.rs`, `action.rs`, `event.rs`, `error.rs`, `engine.rs`, `turn.rs`, and `masking.rs` (104 tests passing, clippy clean). The shipped design diverges from the original draft in a few ways, noted inline below.

**Type** (`state.rs`):

```rust
pub enum GamePhase {
    StartOfTurn(Side),
    Action(Side),
    Discard { side: Side, required: usize },
    GameOver(Side),
}
```

`Discard` carries a `required: usize` countdown (set once on entry to `hand_size - max_hand_size`, decremented per discard) rather than the draft's bare `Discard(Side)`. `GameOver(Side)` was added beyond the original draft — carries the winning side, not yet reachable (no win-condition checks exist yet), included so a future win-condition check only has to set `state.phase = GamePhase::GameOver(winner)`: no handler matches `Action(_)`/`Discard { .. }` once phase is `GameOver`, so every action is rejected automatically with no extra guard code needed. There is no `GamePhase::active_side()` accessor as the draft sketched — see gating below.

**Gating** (`engine.rs`, `turn.rs`): the draft's single `active_side()`-based approach was split in two, since fixed-side and symmetric actions need different shapes:
- `engine::require_phase(state, expected: GamePhase)` — exact match, used by every fixed-side handler (`gain_credit_click`, `install_card`, `rez_ice`'s non-rez-window branch, `initiate_run`, `continue_run`, `jack_out`, `complete_run`, `play_event`, `install_hardware`, `install_program`, `break_subroutine`). Raises `RulesError::WrongPhase { expected, actual }`.
- `turn::require_action_phase(state)` / `turn::require_discard_phase(state)` — pattern-extract `side` (and `required`, for Discard) from whichever side's phase is currently active, for the symmetric `PlayerAction::EndTurn`/`DiscardCard`. Raise `RulesError::NotInActionPhase`/`NotInDiscardPhase`.

`RulesError::NotYourTurn` was **removed entirely** (not kept alongside the new variants as the draft hedged) — all 12 former call sites migrated cleanly.

**Transitions** (`turn.rs`): `end_turn` computes `over_by = hand_size - max_hand_size` (`CORP_MAX_HAND_SIZE`/`RUNNER_MAX_HAND_SIZE`, 5 each); if `> 0` it transitions to `Discard { side, required: over_by }` and emits `GameEvent::DiscardPending`, otherwise it hands control over immediately. `discard_card` removes the card from hand (`take_from_hand`, a small private `turn.rs` helper — not a reuse/rename of `engine.rs`'s `take_from_grip` as the draft suggested), pushes it to the discard pile (`discard_to_pile`), decrements `required`, and once it hits zero hands control over. Both paths converge on `enter_start_of_turn`, which refills clicks, resolves the mandatory Corp draw (only when `next_side == Side::Corp`), and lands on `GamePhase::Action(next_side)`.

**`RunnerState::heap`**: added as a prerequisite for `discard_card` — the Runner had no discard pile before. Mirrors `CorpState::archives`: fully public, never masked (`masking::PublicRunnerState`/`mask_runner_state`).

**Still deferred** (unchanged from the draft): an `Access` phase for interactive access resolution — see "Interactive Access Resolution" above. `RunPhase` did not become a `GamePhase` variant; a run still just requires `GamePhase::Action(Side::Runner)` at `InitiateRun`/`CompleteRun` time.
