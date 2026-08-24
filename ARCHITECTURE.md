# Architecture Intent & Guidelines

This document outlines the architectural patterns, state boundary rules, and design principles governing `netrunner_core`.

---

## 1. Core Principles

* **Deterministic Core:** `netrunner_core` is a pure, side-effect-free state machine. All state mutations must occur deterministically through `apply_action(state, action, registry)`.
* **Static vs. Runtime State Separation:** 
  * `dsl::Card` / `CardRegistry`: Immutable, shared static definitions loaded once per `CardId`. **Never** place runtime counters, temp buffs, or status flags on `dsl::Card`.
  * `InstalledCard` / `InstalledRunnerCard`: Lightweight, per-instance runtime structs held inside `GameState` (e.g., `CorpState::installed`, `RunnerState::rig`).
* **Whole-Action Atomicity:** Actions operate on cloned/mutable working state. If an action fails validation or midway through evaluation, the state rolls back fully.
* **Explicit Gating over Soft Guards:** Action validity must be gated by explicit state structures (`GamePhase`, `PaidAbilityWindow`, `RunPhase`) rather than dynamic runtime assumptions.

---

## 2. State & Execution Model

### Game Phase vs. Paid Ability Window
`GameState.phase` defines the overarching turn step (`StartOfTurn`, `Action`, `Discard`, `GameOver`). Priority windows layered on top (`GameState.paid_ability_window`) suspend regular click-economy actions without altering the base phase.

[ GamePhase::Action ]
│
├── Paid Ability Window Opened (ApproachIce / EncounterIce / Success)
│         │
│         ├── Pass Priority (Both sides pass)
│         └── Window Closed ──► Resume Run Execution Step
│
└── Phase Complete ──► Transition to Discard / StartOfTurn

### Event & Trigger Pipeline
1. **Actions** validate inputs, deduct costs, and invoke `ability::evaluate_effect`.
2. **Effects** mutate `GameState` and return `Vec<GameEvent>`.
3. **Triggers** (`process_card_triggers`) listen to generated `GameEvents` to enqueue follow-up resolution without hardcoding card interactions into action handlers.

---

## 3. Privacy & Public Projections

* **Public Projection:** `rules::masking::mask_state_for_player` derives `PublicGameState` snapshots for network clients, stripping unrevealed cards (HQ, R&D, Grip, unrezzed ICE).
* **Event Stream Sanitization:** Engine `GameEvent`s must be sanitized at the server boundary before client broadcast to prevent hidden information leaks (e.g., accessed card IDs).
