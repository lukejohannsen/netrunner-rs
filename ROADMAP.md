# Netrunner Core Engine Roadmap

Tracking shipped milestones and future technical engine work for `netrunner_core`.

---

## Shipped Features

* **Phase & Priority State Machines:** Fully integrated `GamePhase` state machine (`StartOfTurn`, `Action`, `Discard`, `GameOver`) and priority-based `PaidAbilityWindow` system.
* **ICE Stack & Jack-out Windows:** Dynamic `RunIce` resolution from installed Corp cards and four Netrunner-compliant jack-out legality windows.
* **Access Phase Plumbing:** Interactive multi-card access selection (`SelectNextCard`), post-access decisions (`StealAgenda`, `TrashAccessedCard`, `PassAccessedCard`), and automatic on-access triggers (`OnAccessed`, `OnTrashedFromAccess`).
* **Deck-Out & Victory Resolution:** Start-of-turn deck-out checks, agenda point victory detection via `CardRegistry`, and public Heap/Archives tracking.
* **Icebreaker & Economy Primitives:** 
  * `InstalledRunnerCard` per-instance rig state with `Encounter` and `Turn` strength buff tracking.
  * Strength- and subtype-gated subroutine breaking (`Effect::BreakSubroutines`'s `restrict_to: Option<IceType>`, e.g. Corroder restricted to Barriers; `None` for universal breakers).
  * `OnPlay` trigger resolution for Event/Operation economy boosters (*Sure Gamble*, *Hedge Fund*), via `PlayerAction::PlayEvent` (Runner) and `PlayerAction::PlayOperation` (Corp).

---

## Planned Work & Engine Gaps

### Phase 1: Card & Ability Primitives
- [ ] **Interactive Access Triggers:** Choice-driven on-access abilities requiring cost payments or decisions (e.g., paying credits to prevent damage).
- [ ] **Trigger Self-References:** Resolve `CardTarget::ThisCard` and `Cost::TrashSelf` for cards executing their own trigger effects during access or trash.

### Phase 2: Engine Windows & State Integrity
- [ ] **Asynchronous Start-of-Turn Windows:** Refactor `enter_start_of_turn` into a yielding state machine for start-of-turn paid ability windows and interactive triggers.
- [ ] **Expanded Priority Windows:** Expand priority window checkpoints beyond runs (start/end of turn, post-action windows).
- [ ] **Dynamic Discard Re-checks:** Dynamically evaluate hand size limit during `GamePhase::Discard` to account for mid-discard draws or hand-size modifications.

### Phase 3: Network & Presentation Layer
- [ ] **Per-Viewer Event Sanitization:** Filter `GameEvent` streams per recipient before server relay to sanitize unrevealed cards during access/draw steps.
