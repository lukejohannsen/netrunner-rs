# Architecture Intent & Guidelines

This document outlines the architectural patterns, state boundary rules, and design principles governing `netrunner_core`.

`AGENTS.md` holds the rules of engagement; `ROADMAP.md` holds status. This file explains *how the engine is shaped* and why.

---

## 1. Core Principles

* **Deterministic Core:** `netrunner_core` is a pure, side-effect-free state machine. All state mutations occur through `apply_action(state, registry, action) -> Result<(GameState, Vec<GameEvent>), RulesError>`.
* **Randomness lives inside the state.** `GameState.seed` is fixed at construction and `GameState.rng_step` counts draws. Nothing is threaded in from outside, so `apply_action` stays a pure function of its two explicit inputs, and replaying a recorded action history reproduces a bit-identical final state. Never introduce an external RNG.
* **Static vs. Runtime State Separation:**
  * `dsl::CardDefinition` / `cards::CardRegistry`: immutable, shared static definitions, one per `CardId`. **Never** place runtime counters, temp buffs, or status flags on `CardDefinition`.
  * `InstalledCard` / `InstalledRunnerCard`: lightweight per-instance runtime structs held inside `GameState` (`CorpState::installed`, `RunnerState::rig`). Counters, advancement tokens, rez status, and strength buffs belong here.
* **Whole-Action Atomicity:** every handler clones `state` into a local `next`, mutates only that, and returns it on success. An error at any point — validation, cost payment, or midway through effect evaluation — drops the clone, so the caller's state is never partially mutated. Atomicity is structural, not something a handler has to remember.
* **Explicit Gating over Soft Guards:** action validity is gated by explicit state structures (`GamePhase`, `PaidAbilityWindow`, `RunPhase`, the pending-choice fields) rather than dynamic runtime assumptions.
* **Legality by construction:** `rules::legal_actions` generates candidate actions and keeps only those `apply_action` actually accepts on a cloned state. There is no second copy of the rules, so what a UI or bot is offered can never drift from what the engine permits. This costs a clone per candidate — an accepted trade; do not "optimize" it without a profile proving it matters.

---

## 2. State & Execution Model

### Game Phase vs. Paid Ability Window

`GameState.phase` defines the overarching turn step — `Mulligan`, `StartOfTurn`, `Action`, `Discard`, `GameOver`. A `PaidAbilityWindow` layers priority passing on top, suspending regular click-economy actions **without altering the base phase**. This mirrors `RunPhase`'s existing precedent of never changing `state.phase` mid-run.

```
[ GamePhase::Action ]
│
├── Paid Ability Window opened at a checkpoint
│     │   WindowCheckpoint::Run        — run flow: ApproachIce, EncounterIce,
│     │                                  AccessingCard sub-phases, Success
│     │   WindowCheckpoint::StartOfTurn — after mandatory draw + OnTurnStart
│     │   WindowCheckpoint::EndOfTurn   — before the hand-size check
│     │   WindowCheckpoint::Prevention  — a parked DealDamage/TrashCard
│     │
│     ├── Both sides pass priority
│     └── Window closed ──► resume whatever the checkpoint was pausing
│
└── Phase complete ──► Discard / StartOfTurn
```

A `Prevention` window opens **only** when some installed or rigged card actually has a matching `PreventDamage`/`PreventTrash` paid ability — zero overhead otherwise.

### The Blocking-Guard Precedence Invariant

**This is the most important invariant in the engine.** Several states park a decision that blocks every action except its own resolution:

| Order | State | Resolved by |
|---|---|---|
| 1 | `active_trace` | `SubmitCorpTraceBid` / `SubmitRunnerTraceBid` |
| 2 | `pending_paid_choice` | `AcceptPendingPaidChoice` / `DeclinePendingPaidChoice` |
| 3 | `pending_decision` | `ResolvePendingChoice` / `ToggleCardSelection` / `ConfirmCardSelection` / `ChooseServerForPendingDecision` |
| 4 | `paid_ability_window` | `PassPriority` (plus the abilities the window exists to allow) |
| 5 | `phase` | ordinary phase actions |

`legal_actions::current_actor`'s precedence **must** mirror `engine::apply_action`'s blocking guards in exactly this order. Naming any other side produces a player with **no legal action at all** — an unrecoverable deadlock. This bites specifically when a decision is parked *while a window is open*: the window holds priority for one side while only the other can resolve the parked choice.

Two of the three deadlocks the bot-driven sweep found were this bug class. When you add a new parked-decision state, update both sites together.

### Event & Trigger Pipeline

1. **Actions** validate inputs, deduct costs, and invoke `ability::evaluate_effect(state, effect, acting_card, registry)`.
2. **Effects** mutate the working state and return `Vec<GameEvent>`.
3. **Triggers** — `dispatcher::dispatch_event` maps a `GameEvent` to the cards that react to it, then `ability::process_card_triggers` fires them. Card *behavior* stays entirely data-driven (`dsl::TriggeredEffect` / `AbilityDef` / `Effect`); the dispatcher contributes only the event-to-audience mapping and firing order.

The candidate set for every event is **re-derived fresh from `GameState`** on each call — `CorpState::installed` and `RunnerState::rig` are already the single source of truth for what is in play, so there is deliberately no separate "active behaviors" registry to keep in sync. `win::check_win_conditions` follows the same pure-re-derivation convention.

**Known divergence:** simultaneous triggers fire in install order. The rules give the active player the choice of ordering among their own simultaneous triggers. Tracked in `ROADMAP.md`.

### Cross-Effect Context

`ability::ResolutionContext` threads information an effect or requirement needs but cannot read from `GameState` — the acting card, the triggering event, and cards a `DealDamage` discarded earlier in the same `Sequence`. It is passed through `evaluate_effect` and `check_requirement`, built at the top of a resolution and dropped when it ends. It is never serialized.

```
apply_action
 └─ handler ──► dispatch_event ──► process_card_triggers
                                    │  builds ResolutionContext { acting_card, triggering_event }
                                    └─ evaluate_effect ──► Sequence
                                         effect 1 (DealDamage) ──► records ctx.damage_discarded
                                         effect 2 (EffectIf)   ──► check_requirement reads it
```

**The dividing line is whether a value must survive a parked decision.** Within one resolution → the context. Readable by a *deferred* trigger on a later `PlayerAction` → `GameState`, because the context is gone by then. `last_completed_run` is the second case (deferred `OnRunEnded`, plus it is the dispatcher's only handle on `persistent_after_trash` cards once `active_run` is cleared) and is legitimate state, not debt. `DeferredTrigger::event` carries the triggering event across the defer boundary so a deferred trigger rebuilds the same context it would have had.

See AGENTS.md's State Hygiene Rule.

---

## 3. Privacy & Public Projections

Two layers, with distinct roles — do not conflate them:

* **`rules::masking::mask_state_for_player(state, side) -> PublicGameState`** is the low-level masking primitive: it strips unrevealed cards (HQ, R&D, Grip, Stack, unrezzed ICE, facedown Archives cards) from a snapshot. It reveals a side's own deck order to itself. It is **not** what reaches a client.
* **`view::build_client_view(state, registry, side) -> ClientView`** is the only projection that crosses a process boundary. It is a thin adapter over the masking primitives — reshaped into a friendlier wire format, carrying `legal_actions_for(side)` alongside the state, and applying one stricter policy: **draw-deck order is never revealed, not even to its owner.**

A client receives a `ClientView` and nothing else. Fog of war is enforced at this boundary, never by asking a client to be polite about what it renders.

**Current gaps, tracked in `ROADMAP.md`:**
* `PublicInstalledCard` carries no `counters` field, so a player cannot see counters on their own card. The omission was correctly conservative — counters on an unrezzed Corp card would leak — but it needs a real rule (always visible to the owner; visible to the opponent once faceup) rather than blanket omission.
* **Engine `GameEvent`s do not reach clients at all.** `ServerMessage` carries only `ClientView` snapshots. Per-viewer event sanitization (stripping unrevealed `CardAccessed` IDs from the non-accessing player) is a real requirement — but it is the *second* step. The stream has to exist first.

---

## 4. Card Data Flow

Card behavior is data, never Rust control flow:

```
data/{corp,runner}/*.json  ──build.rs──►  OUT_DIR  ──include_str!──►  cards::embedded
                                                                          │
                            NetrunnerDB catalog ──join on numeric_id──────┤
                                                                          ▼
                                              cards::register_playable_cards ──► CardRegistry
```

* Card files own the rules-engine data plus the `numeric_id` join key. Printed metadata (faction, keywords, influence, deck limit, artist, set) is **joined from the embedded catalog**, not restated per card.
* `#[serde(deny_unknown_fields)]` on `CardDefinition` and the DSL structs makes a misspelled key a parse error rather than a silently defaulted field.
* No runtime I/O in the default build. The `fs-loader` feature exists only for *external* card directories (homebrew, custom sets, no-recompile iteration).
* `is_playable: false` marks catalog-only entries; `rules::deck::validate_deck` rejects any deck referencing one, so `GameState::setup` can never receive an unimplemented card.
