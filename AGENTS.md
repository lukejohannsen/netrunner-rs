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

## Future Engine Considerations

Notes captured from Gemini-assisted reviews of `netrunner_core`'s access/turn logic (2026-08-23). The `GamePhase` state machine, Runner-selected access ordering, and automatic access-trigger resolution described below have since shipped; the remaining items are tracked before further engine work builds on top of those gaps.

### Per-Viewer Event Masking

`GameEvent::CardAccessed` (and every other `GameEvent` variant) currently emits unredacted card IDs. `rules::masking::mask_state_for_player` only masks `GameState` snapshots (`PublicGameState`) — there is no equivalent filtering of `Vec<GameEvent>` anywhere in the engine, and `netrunner_server` is still a stub. Once a server layer exists, it MUST filter/sanitize the `GameEvent` stream per recipient before broadcasting (e.g. strip `CardAccessed { card, .. }`'s `card` field for the non-accessing side when the accessed card wasn't revealed) rather than assuming raw engine events are safe to relay as-is. This matters most for HQ/R&D access, since those are hidden zones — Archives/Remote access was already effectively public.

### Interactive Access Resolution (Access Order + Post-Access Actions Shipped)

Both gaps originally tracked here have shipped:
- **Access order**: `run::access_server` now parks multi-card accesses at `AccessPhase::SelectNextCard { selectable_cards }` instead of a fixed walk order; the Runner picks resolution order via `PlayerAction::SelectCardToAccess` (`run::access::resolve_select_card`). A single accessed card still bypasses straight to `PendingChoice`, and the last remaining card in a multi-card access auto-bypasses `SelectNextCard` too, since there's nothing left to choose between.
- **Post-access actions**: `PlayerAction::StealAgenda`/`TrashAccessedCard`/`PassAccessedCard` (`run::access::resolve_steal`/`resolve_trash`/`resolve_pass`) resolve steal/trash/pass against the current `AccessPhase::PendingChoice` card, with `RunPhase::AccessingCard` as the dedicated run sub-state — no separate `GamePhase` variant needed (see "GamePhase State Machine" below for why).
- **"On access" triggers**: the *automatic* half now also ships — `Trigger::OnAccessed`/`Trigger::OnTrashedFromAccess` (`dsl/trigger.rs`) fire unconditionally via `ability::process_card_triggers`, hooked at every place `GameEvent::CardAccessed`/`CardTrashedFromAccess` fires (`run/access.rs`'s `access_server`, `resolve_select_card`, `advance_or_finish`'s auto-bypass arm, and `resolve_trash`). A mid-trigger flatline (or any other trigger-induced `GameOver`) is handled by a shared `finish_if_game_over` helper that clears `active_run` and halts further access presentation without double-emitting `GameOver` when the triggering effect (e.g. a flatlining `Effect::DealDamage`) already emitted one itself.

**Still open**: the *interactive* half of on-access triggers — a card pausing to ask a player a yes/no or payment question before its effect resolves (e.g. Fetal AI's real "Runner may pay 4c to prevent the damage" text, simplified away for the automatic-only cut that shipped) needs new `AccessPhase`/`PlayerAction` plumbing and is deliberately not built yet. Also still open: `CardTarget::ThisCard`/`Cost::TrashSelf` self-reference resolution for an accessed/trashed card's own trigger effects — both still hard-error with `RulesError::UnresolvedCardTarget`.

### Run Context During Access (Resolved)

This used to describe `engine::complete_run` clearing `state.active_run = None` *before* calling `run::access_server`, which would have hidden "a run against server X is in progress" from any card ability checking active-run state during access resolution. That's no longer how it works: `complete_run` now only reads `server` off `active_run` (a `Copy` field) before cloning state, and `access_server` itself owns clearing `active_run` — only once nothing was accessed, or once every accessed card is fully resolved (`advance_or_finish`)/a trigger ends the game (`finish_if_game_over`). `active_run` stays `Some` throughout `OnAccessed`/`OnTrashedFromAccess` trigger resolution today, so a future "while a run is in progress" card ability would already see correct state.

### Deck-out Win Condition (Shipped)

`turn::enter_start_of_turn` now checks for this up front: if control is passing to the Corp and `corp.r_and_d` is empty, `phase` transitions straight to `GamePhase::GameOver(Side::Runner)` (no clicks refilled, no `TurnStarted` — the turn never starts) instead of silently skipping the draw. Paired with `win::check_win_conditions`, which separately handles agenda-point victory (Corp or Runner reaching 7+ points, checked from `run::access_server` after a steal) — deck-out is deliberately *not* folded into that function, since it's a momentary event (a draw attempt that just failed) rather than a standing condition safely re-derivable from `GameState` alone; see `check_win_conditions`'s doc comment for why. Agenda detection reads from the real `CardRegistry`: `win::agenda_value` is `registry.get(card_id).and_then(|card| card.agenda_points)` — no placeholder/hardcoded lookup remains.

### Dynamic Hand-Size Re-Checks

`GamePhase::Discard { required }` (see `turn::end_turn`) locks in a fixed discard count computed once on phase entry, rather than re-checking hand size after each discard. Real rules require discarding until at/under the max, which matters if a future trigger causes a draw mid-discard. A general card-trigger dispatcher now exists (`ability::process_card_triggers`, added for `OnAccessed`/`OnTrashedFromAccess`), but nothing wires it into the discard flow and no shipped effect fires mid-discard yet — so this is still open, just no longer blocked on "no trigger system exists at all." Worth revisiting once a trigger can actually cause a draw here.

### Asynchronous Start-of-Turn Windows

`turn::enter_start_of_turn` sets `phase = GamePhase::StartOfTurn(next_side)` and resolves its triggers (currently just the mandatory Corp draw) fully synchronously before advancing to `Action(next_side)` — no `PlayerAction` or external observer ever actually sees `StartOfTurn` as the current phase. This is fine while `StartOfTurn` has no triggers requiring a player choice, but cards like "gain 1 credit or draw a card" at turn start will need `enter_start_of_turn` to actually pause and return control mid-phase instead of resolving everything inline.

### ICE Stack Population (Shipped)

`engine::initiate_run` now populates real `RunIce` from `CorpState::installed` + `CardRegistry` in install order (oldest install = outermost = index 0), instead of always building `ice: Vec::new()`. `dsl::Card` gained `strength`/`subroutines` fields as the data source. `RunIce` also gained a `rezzed` flag, seeded from `InstalledCard::rezzed` at run start and synced by `rez_ice` during the approach window — unrezzed ICE has no effect on the run (`run::engine::continue_run`'s `ApproachIce` transition auto-passes it via `pass_current_ice` rather than presenting subroutines), per Netrunner/Null Signal Games rules.

### Jack-out Legality Windows (Shipped)

`run::engine::jack_out` now gates on `RunState::jack_out_permitted`, implementing Netrunner/Null Signal Games' four jack-out windows: illegal during the initial approach of the outermost ICE and during any encounter/subroutine resolution; legal once an ICE has been passed (even an unrezzed one) or the server approach step (`RunPhase::Success`) is reached with no ICE remaining. `advance_run`'s top-of-function "already concluded" guard was narrowed for `JackOut` specifically so `Success` no longer blocks it outright — see `RulesError::IllegalJackOutWindow`.

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

`RunPhase` did not become a `GamePhase` variant, and still hasn't — a run still just requires `GamePhase::Action(Side::Runner)` at `InitiateRun`/`CompleteRun` time. Interactive access resolution ended up living entirely inside `RunPhase::AccessingCard` instead of a `GamePhase::Access` variant as the draft sketched — see "Interactive Access Resolution" above for how that shipped.

### Paid Ability Windows & Priority System (Shipped)

Netrunner/Null Signal Games play depends on formal priority windows: at defined checkpoints, both sides get a chance to fire paid abilities (rez ICE, pump an icebreaker, use a `Trigger::Paid` ability) before the engine auto-advances. `GameState` gains `paid_ability_window: Option<PaidAbilityWindow>` (`state.rs`), a sibling field to `active_run` rather than a `GamePhase` variant — `GamePhase` never changes mid-run (see "GamePhase State Machine" above), and the same reasoning applies here: layering window state on top rather than folding it in kept every existing `RunPhase` transition untouched.

```rust
pub struct PaidAbilityWindow {
    pub active_priority: Side,
    pub consecutive_passes: u8,
    pub return_phase: Box<GamePhase>,
}
```

Reuses the existing `Side` enum rather than introducing a duplicate — the original request specified a new `PlayerSide`, but `Side { Corp, Runner }` already covers that shape and is used everywhere else in the engine.

**New module** `rules/paid_ability.rs`: `open_window`/`open_window_if_at_checkpoint` (opens a window with the active-turn side holding priority first), `require_no_window` (blocks ordinary click-economy actions while a window is open), `note_window_action` (rule: any window-legal action that resolves resets `consecutive_passes` and toggles priority, giving the other side a fresh chance to respond), and `pass_priority`/`close_window` (toggles priority on a single pass; on the second consecutive pass, closes the window and auto-advances whatever run step was paused).

`close_window` keys off `state.active_run`'s *current* `RunPhase` — not a discriminant on `PaidAbilityWindow` itself — since nothing a window permits (`RezIce`, `BreakSubroutine`, `ActivateAbility`) mutates `RunPhase`. `return_phase` is captured on open for structural completeness but isn't currently load-bearing: `GamePhase` stays `Action(Side::Runner)` for a run's entire duration, so every window's `return_phase` is always that same value and nothing reads it back — it's forward-compatible groundwork for a hypothetical future non-run window, not wired into any control flow today.

**Integration** (`engine.rs`): windows open at three checkpoints — `continue_run` opens one on landing at `ApproachIce`/`EncounterIce` (via `open_window_if_at_checkpoint`), and `complete_run` opens one explicitly on reaching `RunPhase::Success` rather than accessing immediately. This is the biggest control-flow change: `continue_run` no longer performs the `ApproachIce → EncounterIce` commit or `EncounterIce → next-ICE` pass directly — it only drives `Initiation → ApproachIce` (or `→ Success` with no ICE) and opens a window; every subsequent step happens inside `close_window` once both sides pass. Likewise `complete_run` no longer calls `run::access_server` itself — access happens inside `close_window`'s `Success` arm. `PlayerAction::PassPriority { side }` is the new symmetric action (explicit `side`, since `state.phase` can't disambiguate whose priority it is mid-run) — dispatches to `pass_priority`, which takes `&CardRegistry` in addition to `state`/`side` (a deviation from a bare `(state, side)` signature) because closing a `Success` window must call `run::access_server`.

`RezIce` and `BreakSubroutine` stay priority-independent — neither is gated by `window.active_priority`, since both are existing free/special actions rather than `Trigger::Paid` abilities — but both call `note_window_action` on success. `ActivateAbility` is the one handler that *is* priority-gated: outside a window it's unchanged (side derived from `state.phase`); inside one, side is still resolved by zone (Corp `installed && rezzed` vs Runner `rig` — disjoint, so unambiguous) but must additionally match `window.active_priority` (`RulesError::NotYourPriority` otherwise). Seven ordinary handlers (`gain_credit_click`, `draw_card_click`, `install_card`, `play_event`, `install_hardware`, `install_program`, `advance_card`) call the new `require_no_window` guard and are rejected with `RulesError::BlockedByPaidAbilityWindow` while a window is open. `jack_out` additionally clears `paid_ability_window` on success — a window can be open when the Runner bails (e.g. mid-`ApproachIce` on the second+ ICE), and leaving it set would strand it with no run left to ever close it against.

**Still open**: windows only exist at the three run checkpoints above — there's no start-of-turn or end-of-turn window (real Netrunner/Null Signal Games opens one after essentially every action). `RezIce` and the direct/free `PlayerAction::BreakSubroutine` remain uncosted/hardcoded — a window gives both sides a chance to *respond*, but doesn't by itself make either of those two actions costed. Icebreaker credit costs *are* now charged, but only via a card's own data-driven `Trigger::Paid` `AbilityDef` (pump/break abilities like Corroder's) — see "Icebreaker Strength, Subroutine-Breaking & Runner Rig Instance State" below.

### Icebreaker Strength, Subroutine-Breaking & Runner Rig Instance State (Shipped)

`dsl::Effect` gained `BoostStrength { amount, duration: BoostDuration }` (`Encounter` or `Turn`) and `BreakSubroutines { count: SubroutineBreakCount }` (`Fixed(n)` or `All`), giving icebreaker Programs (still just `CardType::Program` — no dedicated `Icebreaker` subtype exists) real pump/break `Trigger::Paid` `AbilityDef`s, e.g. `data/runner/corroder.json`. The existing `Effect::GainCredits(Side, u32)` was reused as-is for economy Events/Operations (`sure_gamble.json`/`hedge_fund.json`, both pre-existing and already matching spec) rather than adding a second, differently-shaped `GainCredits` variant.

This needed a real architecture fix, not just new `Effect` arms: `RunnerState.rig` was a bare `Vec<CardId>` with zero per-installed-card runtime state, so there was nowhere to record "this specific installed Corroder currently has +1 strength this encounter." Putting mutable buff fields directly on `dsl::Card` (as an earlier draft of this work assumed) would have been wrong — `Card` is a single shared/immutable definition held once per `CardId` in `CardRegistry`, not a per-instance object. Instead, `rig` is now `Vec<InstalledRunnerCard>`, mirroring the Corp side's existing `InstalledCard` pattern:

```rust
pub struct InstalledRunnerCard {
    pub card: CardId,
    pub base_strength: i32,           // seeded from registry.get(card).strength at install time
    pub encounter_strength_buff: i32, // reset when the current ICE encounter ends
    pub turn_strength_buff: i32,      // reset at end of the Runner's turn
}
```
`effective_strength()` sums all three. `install_hardware`/`install_program` now take `&CardRegistry` (previously they didn't) to seed `base_strength` on install. `ability::evaluate_effect` gained an `acting_card: Option<&CardId>` parameter so `BoostStrength`/`BreakSubroutines` know which rig card is acting — `None` from subroutine-resolution call sites, `Some(&card_id)` from `activate_ability`/`process_card_triggers`. `BreakSubroutines` gates on `breaker.effective_strength() >= ice.current_strength()`, erroring the new `RulesError::BreakerStrengthTooLow` otherwise (whole-action atomicity holds — `activate_ability` clones into `next` and only returns it on success, so a failed break also rolls back the credit cost that was paid). Cleanup hooks: `run::engine::continue_run`'s `EncounterIce`-exit arm resets `encounter_strength_buff`; `turn::end_turn` resets `turn_strength_buff` when the Runner's own turn ends. `masking::PublicInstalledRunnerCard` exposes `current_strength` unmasked, matching Rig's existing "always public" treatment.

While auditing `play_event` for this work, found and fixed an unrelated pre-existing gap: it paid the click/credit cost and moved the card out of the grip, but never actually called `ability::process_card_triggers(..., Trigger::OnPlay)` — so Sure Gamble's `OnPlay → GainCredits` was unreachable through the real `apply_action` path (only proven to parse, never to fire). Now fixed with one call using existing machinery.

**Still open**: `BreakSubroutines`/`BoostStrength` don't restrict by ICE subtype — Corroder's real "Barrier only" text is unenforced (breaks any pending subroutine on the encountered ICE, strength-gated only); adding that needs `&CardRegistry` threaded into `evaluate_effect` or a `restrict_to: Option<IceType>` field, deliberately deferred as a separate primitive. Corp still has no "play an Operation from HQ" `PlayerAction` at all — Hedge Fund's `OnPlay` trigger remains covered only at the fixture-parse level, unlike Sure Gamble's now-fixed Runner-side path.
