# Netrunner Workspace Roadmap

Canonical single source of truth for engine mechanics, client-server infrastructure, single-player card data, and AI bot development across `netrunner_core`, `netrunner_bots`, `netrunner_single_player`, `netrunner_server`, `netrunner_cli`, `netrunner_gym`, `netrunner_selfplay`, and `netrunner_card_sync`.

**Current goal:** a solid single-player Netrunner, then expansion into network play.

**Health as of the last full review:** 739 tests passing, 0 failing, 1 ignored (network-gated live sync). `cargo clippy --workspace --all-targets` completely silent. The engine's purity boundary, determinism model, and masking layer all hold. Remaining work is about *seams* — the places single-player and network play currently diverge — not about engine rot.

---

## 🚀 Shipped & Verified Milestones

### Core Engine & Rules Primitives (`netrunner_core`)
- [x] **Phase & Priority State Machines:** `GamePhase` (`Mulligan`, `StartOfTurn`, `Action`, `Discard`, `GameOver`) and priority-based `PaidAbilityWindow` (ICE approach/encounter, pre-access, per-card access decisions, start/end of turn).
- [x] **ICE Stack & Jack-out Windows:** Dynamic `RunIce` resolution from installed Corp cards and four Netrunner-compliant jack-out legality windows.
- [x] **Access Phase Plumbing:** Multi-card access selection (`SelectNextCard`), post-access decisions (`StealAgenda`, `TrashAccessedCard`, `PassAccessedCard`), automatic on-access triggers, and interactive triggers (`PayToAvoidAccessTrigger`/`DeclineAccessTrigger`, e.g. *Fetal AI*).
- [x] **Deck-Out & Victory Resolution:** Start-of-turn deck-out checks, agenda point victory via `CardRegistry`, public Heap/Archives tracking.
- [x] **Icebreaker & Economy Primitives:** Rig state with `Encounter`/`Turn` strength buff tracking, subtype-gated subroutine breaking (`restrict_to`), `OnPlay` economy operations.
- [x] **Self-Reference Card Triggers:** `CardTarget::ThisCard` / `Cost::TrashSelf` dynamic resolution.
- [x] **Deterministic replay foundation:** `seed` + `rng_step` live inside `GameState`, so `apply_action` is pure and any recorded action history replays bit-identically (asserted by `single_player_test.rs`). This underpins replay, MCTS, and server authority alike.
- [x] **Legality by construction:** `rules::legal_actions` generates candidates and keeps only those `apply_action` actually accepts, so legality can never drift from enforcement. Costs a clone per candidate — do not "optimize" without a profile proving it matters.

### Network, Masking & Client Architecture (`netrunner_server` & `netrunner_cli`)
- [x] **Masked per-seat state (`ClientView`)** — perspective masking and fog of war for Corp/Runner hidden information, enforced at the engine boundary.
- [x] **Transport-Agnostic Channel Layer** — `ClientMessage`/`ServerMessage` decoupled from transport, uniform `MatchSession` execution.
- [x] **Real WebSocket Transport (`tokio-tungstenite`)** — `netrunner_server --serve` daemon, `netrunner_cli --mode remote` with `Connect` handshakes and two-human lobby pairing, verified live TUI socket driving.

> **Known shape, recorded deliberately:** the server sends **full masked `ClientView` snapshots** after every action, not deltas. That is the current design, not an oversight. See Phase 4 for when deltas become worth it.

---

## 📇 Phase 1: Single-Player Completeness (Immediate Focus)

Ordered by what stands between the engine and a person playing a full, satisfying game of their own deck.

### 1. Deck Import End-to-End — **top priority**
- [ ] **Convert a validated `Decklist` into a playable `rules::Deck`.** Nothing anywhere does this today, so a user still cannot paste a decklist and start a match. All the hard parts already exist:
  - `deck::Decklist` already deserializes the NetrunnerDB/community JSON export shape.
  - `deck::validator::validate_deck` already enforces influence, side, format legality, per-card `deck_limit`, and agenda-point ratios.
  - `rules::deck::validate_deck` already enforces copy limits, agenda points, and `is_playable`.
  - `decks::SampleDeck::to_deck()` is the conversion pattern to imitate.
- [ ] **Reconcile the three deck types and two validators.** `deck::Decklist` (import), `rules::Deck` (setup input), and `decks::SampleDeck` (embedded samples) coexist with two independent validators that duplicate `MAX_COPIES_PER_CARD` on purpose. Decide the intended pipeline — import shape → validation → runtime shape — and document which validator owns which rule, so a caller knows which to invoke.
- [ ] **Wire it to the CLI:** a `--deck <path>` flow that loads, validates with readable errors, and starts a local match.

### 2. Make Game State Legible to a UI
- [ ] **Expose card counters through `masking`/`ClientView`.** `InstalledCard`/`InstalledRunnerCard` both carry `counters: u32`, but `PublicInstalledCard` has no counters field at all — so a Runner cannot see virus counters on their own *Botulus*, and neither side sees credits on a *Nico Campaign*. Blocks any real UI. The masking rule is straightforward: always visible to the owner, and visible to the opponent once the card is rezzed/faceup; hidden on an unrezzed Corp card (the original reason for the omission).
- [ ] **Add a turn counter to `GameState`.** There is none — `netrunner_single_player::history` reconstructs turn numbers externally. Needed for UI display, replay scrubbing, and any "on turn N" effect.

### 3. Rules Gaps Worth Closing
- [x] **Purge virus counters as a basic Corp action — DONE.** `PlayerAction::PurgeVirusCounters`: 3 clicks (the Corp's whole turn, since `CORP_CLICKS_PER_TURN` is exactly 3), zeroing `counters` on every installed/rigged card whose registry `counter_kind` is `CounterKind::Virus`. Scans **both** sides — the rule is about the counter kind, not who controls the card — though only Runner Programs qualify in the current pool. Closes the hole where the Corp had no counterplay to *Botulus*/*Leech*/*Fermenter*/*Conduit*/*Tranquilizer*.
  - Deliberately has no "nothing to purge" error, unlike `RemoveTag`'s `RunnerNotTagged`: purging an empty board is legal, just pointless. `legal_actions` therefore offers it whenever the clicks are there, keeping legality a mirror of the rules; if self-play ever suffers from bots burning turns on it, the fix belongs in `netrunner_bots`' evaluation, not here. (Measured: the System Gateway sweep went 5.95s → 6.26s, i.e. no step-count blowup.)
  - `ActionSpace::SIZE` 1024 → **1025**. The slot was **appended** as its own trailing segment rather than added to the payload-free `UNIT` block where it naturally belongs, so every pre-existing index still decodes to the same action — an exported policy's first 1024 outputs stay meaningful and only the head width changes. Same append-never-shift principle as `observation::CARD_VOCAB`. **Keep that property for future additions.**
  - Guarded at the data end by `purge_clears_counters_on_every_real_system_gateway_virus`, which pins the full virus roster against the real embedded cards — the engine-level tests use synthetic fixtures and would still pass if a shipped virus card were missing `counter_kind`.
- [x] **Rename `Cost::PurgeTags` → `Cost::ClearTags` — DONE.** Also `GameEvent::TagsPurged` → `TagsCleared`, so "purge" now means exactly one thing in the codebase (virus counters) with no near-miss neighbour. Rust-only; no card JSON referenced either name.
- [x] **Dynamic discard re-checks — DONE (defensive).** `discard_card` no longer decrements the count stored in `GamePhase::Discard`; both it and `finish_end_turn` now call a single `turn::cards_over_hand_limit`, re-derived from live hand size and max hand size every step. The stored `required` became a *report* rather than the authority, so `require_discard_phase` no longer returns it.
  - **Not a bug that was reachable.** `GameEvent::CardDiscarded` has no `dispatcher::dispatch_event` arm, so nothing fires between discards and the count cannot currently go stale. This is insurance for the first card that draws, changes max hand size, or deals brain damage mid-discard.
- [x] **Simultaneous trigger order — cross-side consistency DONE (defensive).** `both_sides_candidates` now tags each candidate with its side and routes through `order_active_first` via a new `dispatcher::turn_active_side` (reads `GamePhase`, which is total — unlike `current_actor`, which is `None` during `StartOfTurn`). Previously it emitted Corp-before-Runner regardless of whose turn it was, at the only two dispatch sites whose audience spans both sides.
  - **Not reachable either:** **zero** cards declare `Trigger::OnDamageAboutToResolve`/`OnTrashAboutToResolve`.
  - **Deferred-trigger queue — DONE, and it fixed a live bug.** Trigger dispatch had no equivalent of `Effect::Sequence`'s "stop if something parked" guard, so when a trigger parked a decision the rest of the dispatch resolved *underneath* it. Reachable in ordinary play: *Clearinghouse*'s `OnTurnStart` is an `Effect::PresentChoice`, so with any second Corp `OnTurnStart` asset installed (*PAD Campaign*, *Nico Campaign*), that second card resolved during Clearinghouse's pending choice.
    - `GameState::deferred_triggers` now queues the untouched remainder, drained by `dispatcher::drain_deferred_triggers` from **one** choke point: `engine::apply_action`, after every handler. Centralized for the same reason the `active_trace` guard is — and it covers trace and prevention-window resolution too, which a `pending_choice`-only drain would miss.
    - Every dispatch site funnels through one guarded primitive, `dispatcher::fire_plan`, over a flat `(card, trigger, target)` plan. Flat rather than a card list because `RunSucceeded` fires up to four triggers per card and a blockage can land between two of them.
    - `GameState::is_resolution_blocked()` is now the **single** definition of "something is parked" — it was previously spelled out inline in two places and, by omission, missing from the dispatch loops entirely, which was the bug.
    - **This is the engine's only continuation mechanism, and it is trigger-level only.** `Effect::Sequence` still abandons effects after a parking one; its doc comment's "don't chain two independently-parking effects" rule still stands for card authors.
  - **Player-chosen ordering among your own simultaneous triggers — DONE.** `PendingDecision::ChooseTriggerOrder` + `PlayerAction::ChooseTriggerToResolve`: when 2 or more of one side's own cards react to the same event, their controller picks which resolves next, repeatedly, until one is left (which fires automatically — N triggers cost N-1 decisions, not N). Cross-side order is deliberately excluded: it's fixed by rule via `order_active_first`, not the player's to choose.
    - **Cost guard:** a decision is parked only when the order is genuinely contestable. Candidates are pre-filtered by `dispatcher::declares_trigger` — a cheap registry lookup, deliberately *not* a dry run of `TriggeredEffect::requirement` — so a lone reacting card fires directly. Over-counting (offering a choice where one option turns out to no-op) is the safe direction; silently picking an order the player was entitled to choose is not. Measured: the System Gateway sweep went 5.90s → 6.45s.
    - `ActionSpace::SIZE` 1025 → **1045** (`MAX_INSTALLED_PER_SIDE` slots), appended — same append-never-shift rule as the purge slot.

> **Deadlock found and fixed while landing the above** — pre-existing on `main`, not introduced by it. *Red Team*'s run-initiating paid ability was activatable during the Runner's **end-of-turn window**, which deliberately keeps `phase == Action(Runner)`. The run started, the window closed, and `finish_end_turn` handed the turn over with `active_run` still set — leaving the Corp with **no legal action at all**: `EndTurn` rejected by `CannotEndTurnWhileRunActive`, and the run not theirs to advance. Confirmed pre-existing by reproducing it at the previous commit (seed 9 of a widened sweep); the committed sweep's seeds 0..8 simply never reached it.
> - Fixed by `run::check_run_may_begin` — now the **single** definition of "may a run start", shared by `start_run` and `Effect::PromptChooseServer`'s park-time check. Those two must agree: `PromptChooseServer` parks a decision only `start_run` can resolve, so a narrower copy in one of them parks an unresolvable decision and deadlocks outright. That is exactly how the original bug arose (the park-time check tested only `active_run`), and it recurred mid-fix when the two briefly disagreed again.
> - **Sweep seed range widened 8 → 32 — DONE.** Two of the three deadlocks in this area were only visible outside seeds 0..8, so coverage was the binding constraint, not the test's design. 32 costs ~31s in a debug build (measured; ~1s/seed, linear), taking the whole workspace suite from **27s to 40s** — less than the naive sum, since other crates' tests run alongside it. Cheap enough to keep running constantly, which is what matters given the repo has no CI and `cargo test --workspace` is the only gate.
>   - Depth is `NETRUNNER_SWEEP_SEEDS`-overridable on the same test body, rather than a second `#[ignore]`d deep copy that would never run. `NETRUNNER_SWEEP_SEEDS=256 cargo test -p netrunner_single_player --release` verified clean at 87s — the first coverage beyond the 60 seeds checked while fixing the Red Team bug. See AGENTS.md's Testing Rule for when to run it deep.
- [x] **Post-action paid-ability window — DONE.** `WindowCheckpoint::PostAction { side }` opens after a basic click action so the **non-active** player can respond. Only their half was missing: `activate_ability` already permits the acting player's own paid abilities throughout `Action(side)`, so a window nobody but them could use would be pure overhead.
  - **Gated on the opponent actually having something usable** (`paid_ability::has_usable_paid_ability` — requirement met and cost affordable, via the new non-mutating `ability::cost_is_affordable`). Deliberately not implemented by probing `legal_actions`, which would recurse through `apply_action` without bound.
  - **Which actions count is an exhaustive `match` on `PlayerAction`**, not a check for a `ClickSpent` event — exhaustive so a new action fails to compile until classified, explicit so a click-*costing* paid ability isn't mistaken for an action. That is also what prevents a cascade: closing a window is a `PassPriority`, which is not an action.
  - **Measured, since step cost was the whole risk: none.** Sweep 31.11s → 30.79s; 256-seed deep sweep 87s → 85s; 40 headless games unchanged. `post_action_windows_occur_but_stay_rare` pins both failure modes with generous bounds — 63 openings across 8,956 steps in 16 games, i.e. real but ~0.7% of steps.
- [x] **Icebreaker abilities require an encounter — DONE, and it is what made the above affordable.** Real Netrunner only permits an icebreaker's abilities while encountering ICE; the engine offered Cleaver's "2[c]: +1 strength" as a legal action on the Corp's turn — affordable, permitted, and pointless. With a rig of breakers always answering "yes", *any* opponent-has-something gate fired on essentially every action.
  - New `EffectRequirement::DuringEncounter` on the `Paid` abilities of all 10 breakers (Botulus keeps the stricter `EncounteringHostIce`; Leech was caught by sweeping every card rather than a hand-written list). `Effect::BoostStrength` also now errors `NotInEncounter` at resolution, matching what `BreakSubroutines` and `ModifyStrength` already did — the requirement gates *offering*, the effect gates *doing*.

- [x] **Unspent clicks are lost at end of turn — DONE.** `end_turn` used to leave the ending side's clicks in place, reasoning that "every click-spending action is already gated by `engine::require_phase`, so leftover clicks are inert." True for *actions*, false for *paid abilities*: `activate_ability` resolves the acting side from card ownership whenever a window is open, deliberately bypassing phase — so *Regolith Mining License*'s `[click]: take 3[c]` was payable on the opponent's turn out of clicks that should no longer exist. Now zeroed in `end_turn`, before its own `EndOfTurn` window, since that window is part of the turn ending rather than more action phase.

> **Two things this surfaced, both worth knowing.**
> - **The post-action window's apparent activity was mostly this bug.** Its bounds test recorded 63 openings across ~9,000 steps; with clicks correctly cleared, *Spin Doctor* (cost `RemoveSelfFromGame`, no requirement) is the **only** System Gateway card either side can use on the opponent's turn — one copy, which must be drawn, installed and rezzed. The test now asserts only the upper bound (a too-loose gate would tax every action); "does it fire when someone qualifies" is covered deterministically by an engine unit test instead, because in-game occurrence is genuine luck.
> - **`MAX_INSTALLED_PER_SIDE` was too small: raised 20 → 32.** Real System Gateway matchups reach 23-24 installed Corp cards, past which `RezIce` had no `ActionSpace` index — and a legal action with no index is invisible to `get_action_mask`, so when it is the *only* legal action the mask is empty and `netrunner_bots`' index adapter panics. Pre-existing and confirmed reachable on `main` at other seeds; this change only altered trajectories enough for the committed seeds to find it. `action_mask.rs`'s claim that "real games essentially never reach these caps" was simply wrong, and its own advice — widen the constant — is what was followed.
>   - **`ActionSpace::SIZE` 1045 → 1357, and unlike every previous growth this one SHIFTS indices rather than appending** — the constant sizes 26 segments spread through the space. An exported policy needs **retraining**, not just a wider head. Keep appending wherever there is a choice; there wasn't one here.

> **Note — the prevention subsystem is currently dead code.** `Effect::PreventDamage`/`PreventTrash`, `state::PendingPrevention`, and `WindowCheckpoint::Prevention` are fully implemented, wired into `evaluate_effect`, and tested, but **zero cards use them**, so no real game reaches any of it. Worth knowing before extending it — and worth a card to exercise it before trusting it.

### 4. Engine Hygiene Before the Next Set
- [ ] **Effect resolution context.** `last_discarded_cards`, `last_completed_run`, and `last_advancement_was_first` are scratchpad fields on `GameState` that exist only because a `Sequence`'s evaluation loop cannot thread context between effects. Replace with a resolution-context struct passed through `evaluate_effect` before a fourth one is added. See AGENTS.md's State Hygiene Rule.
- [ ] **Per-card recurring credits.** `CorpState::recurring_credits` is a single Corp-wide pool sourced only from the identity. Real recurring credits are per-card (*Cyberfeeder*, *Net Mercur*). The pool needs to move onto `InstalledCard`/`InstalledRunnerCard` before any such card can be implemented.

### 5. Format Support
- [ ] **Null Signal Games Format Support.** Enforce the official [supported formats](https://nullsignal.games/players/supported-formats/) (Startup, Standard, Eternal, Snapshot) — rotation tracking, banlists, points restrictions, legality checks before match start. `format.rs` currently holds a deliberately small illustrative seed scoped to the embedded sets, and says so honestly; it is not a claim of NSG's authoritative current rotation.

### 6. Card Data & Ingestion — DONE
- [x] **Unified Card Model:** the two previously-parallel card systems collapsed into one `dsl::CardDefinition` + `CardRegistry`, cross-referenceable by registry `id` or `numeric_id`. `rules::deck::validate_deck` rejects any deck referencing an `is_playable: false` card, so `GameState::setup` can never receive a catalog-only card.
- [x] **JSON Card Loading:** cards authored one-per-file under `data/{corp,runner}/`, concatenated by `build.rs` and embedded via `include_str!`. `cards::register_playable_cards` serves every playable card to every consumer with no feature flag and no runtime I/O. The hardcoded Rust card builders are deleted — JSON is the single source of truth. Printed metadata is joined from the embedded NetrunnerDB catalog on `numeric_id` rather than restated per card (this parity test caught a real bug on first run: *Malapert Data Vault* rezzing for 0 instead of 1).
- [x] **Bundled Gateway & Elevation sets**, embedded for offline, zero-I/O availability.
- [x] **Dedicated Sync & Cache Crate (`netrunner_card_sync`):** async NetrunnerDB API v2 sync and cross-platform disk caching (`dirs::cache_dir()`), wired into `netrunner_cli cards {list-sets,sync}`, without polluting `netrunner_core`.
- [x] **Schema additions, all built on demand against real cards:** memory cost, generic counters (`Virus`/`Power`/`Credit` + `AddCounters`/`RemoveCounters`/`Cost::RemoveCounters`), and hosting (`InstalledRunnerCard.hosted_on_ice`, `PlayerAction::InstallProgramOnIce`, cascade-trash-on-unhost) for Trojan programs. Upgrades hosted on ICE remain unmodeled — no card needs it.

### 7. System Gateway Card Set — DONE
All 75 playable *System Gateway* cards are implemented, tested, and `is_playable: true`, authored JSON-only. Coverage is enforced by `every_system_gateway_card_is_implemented_or_explicitly_excluded`, which requires each of the 77 printed cards to have an implementation or an `SG_UNIMPLEMENTED` entry stating why. **That test is the gate for calling any future set complete.**
- [x] **DSL primitives added, in dependency order:** foundational fixes (`CardDefinition::validate()`'s agenda-field bug, Upgrade installability, generalized `OncePerTurn`); new triggers (`OnRez`, `OnApproachServer`, `OnRunEnded`, `OnBasicDrawAction`, `OnAdvance`, `OnDiscardPhaseEnd`); decision primitives (`OfferPaidChoice`/`PresentChoice`/`PromptChooseCards`/`PromptChooseServer` parking a `PendingPaidChoice`/`PendingDecision`); `dsl::zone::{CardZoneRef,CardFilter}`; hosted credit pools; MU and max-hand-size bonuses; console-singleton enforcement; conditional cost discounts; hosting for Trojans; bioroid click-to-break; dynamic amounts and conditional strength; persistent-after-trash upgrades; facedown Archives tracking (with its own masking rule — orientation and count public, a facedown card's identity hidden from the Runner); and remove-from-game. `ActionSpace::SIZE` grew 724 → 1024.
- [x] **Two identities permanently out of scope:** *The Catalyst: Convention Breaker* and *The Syndicate: Profit over Principle* carry `stripped_text: "Starter game only."` — no rules text exists to implement. They are the only two `SG_UNIMPLEMENTED` entries.
- [x] **Three reprints share ids with baseline cards:** *Sure Gamble*, *Hedge Fund*, *Cleaver* — handled by keeping the single existing definition. *Cleaver*'s pre-existing definition had transposed paid-ability costs; fixed as a data-correctness bug.
- [x] **Engine bugs found by the bot-driven sweep** (all reachable in ordinary play, none by any per-card test):
  - `current_actor` ignored `pending_paid_choice`/`pending_decision`, so a decision parked *while a paid-ability window was open* named the window's priority holder instead of the decision's chooser — leaving a player with no legal action at all. Its precedence now mirrors `apply_action`'s blocking guards exactly (trace → paid choice → decision → window → phase). **These two must stay in sync; that is how this was found.**
  - `Effect::PromptChooseServer` parked a choice resolvable only by `run::start_run` without checking no run was active — an unresolvable decision blocking every action.
  - `ToggleCardSelection` enforced eligibility but not `max`, so a selection could grow past the bound `ConfirmCardSelection` requires.

**DSL growth baseline:** at System Gateway completion, **14 of 66 `Effect` variants are used by exactly one card**, against a healthily reused core (`PromptChooseCards` 17 cards, `PresentChoice` 16, `EffectIf` 12). Measure the next set against this ratio — see AGENTS.md's DSL Growth Rule.

---

## 🔗 Phase 1.5: Session Unification (the single-player → network bridge)

**This is the highest-value architectural work in the repo right now.** Four crates independently re-implement the same match loop — `current_actor` → get action → `apply_action` → check `GameOver` — each with its own copy of `MAX_STEPS = 10_000`:

| Driver | Sync/async | Sees | Action shape | Seat trait |
|---|---|---|---|---|
| `netrunner_single_player::SinglePlayerSession` | sync | **raw `GameState`** | `ActionSpace` index | `PlayerDriver` |
| `netrunner_server::MatchSession` | async | masked `ClientView` | `PlayerAction` | `BotAgent` |
| `netrunner_cli::headless` | async | — | — | — |
| `netrunner_gym::env` | sync | masked (via `encode_observation`) | index | `BotAgent` |

Their own doc comments admit the duplication ("Mirrors the decision-loop shape of `MatchSession`", "Shape mirrors `MatchSession::run`"). Two consequences are already live:

- **The local path masks by client convention, not by interface.** `PlayerDriver::select_action` receives the raw `&GameState`, so the seat boundary itself enforces nothing. The TUI's human seat (`tui/mod.rs`) is well-behaved and calls `build_client_view` itself before rendering — but that is the client choosing to be polite, exactly the anti-pattern AGENTS.md §2 now bans. The network path gets this right structurally: a `MatchSession` channel seat *cannot* see anything but a `ClientView`. Porting the local path onto the same seat interface is what makes local masking structural rather than voluntary.
- **The network path cannot show a game log.** `MatchSession` discards each action's `Vec<GameEvent>` after one-shot use in `classify_end_reason`. `MatchHistory` lives in `netrunner_single_player` and is unreachable from the server, so the TUI's event log only populates on the local path.

### Target design: one core-owned decision loop, four seat types, masked by default

- [ ] **Extract the shared loop** into a session driver owning `current_actor` → action → `apply_action` → `GameOver` **once**, with `MAX_STEPS` as a single constant. Home it in `netrunner_core` if it stays dependency-free, or a thin `netrunner_session` crate if the async split demands it.
- [ ] **Unify the seat interface on `ClientView` + `PlayerAction`.** `BotAgent` already has this shape. `PlayerDriver`'s `(&GameState, mask, index)` shape becomes an adapter layered on top, for the RL path — the only caller that genuinely needs `ActionSpace` indices. This closes the unmasked-state hole in the local human path.
- [ ] **Collapse seats into one enum:** `Bot`, `LocalHuman`, `Channel`, `Indexed` (RL). Sync vs. async is a property of how the driver is *pumped*, not a reason to fork rules flow — a step-function shape (`fn step(&mut self) -> SessionStep`) lets `netrunner_cli` pump it synchronously and `netrunner_server` pump it inside `tokio`.
- [ ] **Move `MatchHistory` into the shared driver** so both paths record actions and events. This simultaneously delivers the structured match log below and gives the network path a game log.

**Migration order, each step leaving the tree green:** extract the shared loop → port `MatchSession` → port `SinglePlayerSession` (the adapter keeps `PlayerDriver` working for `netrunner_gym`/`netrunner_selfplay`) → port `netrunner_cli::headless` → collapse `netrunner_gym::env`'s fast-forward onto it.

---

## 🧠 Phase 2: Bot Intelligence, Replay & Gym Harness

### 1. Determinization — DONE
- [x] **State determinization in `netrunner_bots`.** `determinize` samples a plausible concrete `GameState` from a masked `ClientView` and is wired into `HeuristicAgent`, `MctsAgent`, and `PuctAgent`. `MctsAgent` resamples independently per parallel tree, making it genuine (if basic) Information Set MCTS.

### 2. Action Replay Protocol & Match Logging — mostly done
- [x] **Action/event recording and verified replay.** `MatchHistory`/`HistoryEntry` record `(turn_number, side, action, events)`; an integration test re-applies a recorded history and asserts a bit-identical final state.
- [ ] **Emit it as JSON-Lines.** Needs `Serialize` on `HistoryEntry`/`MatchHistory` and a writer. Blocked on nothing — but do it as part of Phase 1.5 so both drivers get it, rather than wiring it to `SinglePlayerSession` alone.
- [ ] **TUI Replay Viewer** in `netrunner_cli` for step-by-step post-match review.

### 3. Training Decks & Self-Play Data — DONE
Null Signal Games' seven published System Gateway sample decklists are embedded as data (`data/decks/*.json`, compiled in by `build.rs`) and reachable via `netrunner_core::decks`. Three Runner and four Corp decks give twelve matchups; self-play rotates through them and local play selects with `--corp-deck`/`--runner-deck`. `every_sample_deck_is_legal` asserts all seven validate; `every_sample_deck_matchup_finishes` plays all twelve to a result.
- [x] **Real decks replaced the blank-filler fixture.** Self-play previously used a synthetic pair — an 18-blank-agenda Corp deck behind 6 ICE. All 5,000 recorded games ended in a Corp loss, so the value head trained against a constant and no System Gateway card ever appeared.
- [x] **Card-identity observation.** `OBS_SIZE = 990`: scalars plus five card-identity planes over a fixed 192-slot vocabulary ordered by `numeric_id`, so a future set appends rather than shifting slots out from under an exported model. The old 30-scalar encoding had no card-identity features at all — two different hands of the same size were literally the same input.
- [x] **A trained policy is playable.** `netrunner_cli --corp onnx --model <path>` (feature `onnx`) seats a trained network via `SinglePlayerSession`.
- [x] **Engine and search bugs found by running real decks** — none reachable with the old fixture: *Send a Message* offering already-rezzed ice as rez targets (fixed with `CardFilter::UnrezzedIce`); `ToggleCardSelection` keyed by `CardId` so two copies of a card could never both be selected, deadlocking any "choose N" over a zone with fewer than N *distinct* cards (*Carnivore* against two identical grip cards) — toggling now cycles through copy counts; `determinize` resampling the stack out from under a parked card-selection; and greedy self-play action selection being a perfect two-cycle when toggling one card on and off (now falls back to sampling after `MAX_GREEDY_REPEATS`).
- Combined effect on a 12-game smoke run: games hitting the 10,000-step budget went **8 of 12 → 0 of 12**, median game length **6,674 → 171 steps**, and outcomes went from a single constant value to a real win/loss split.

**Running a training loop** (self-play → PyTorch → ONNX → promote, repeat):
```bash
source scripts/venv/bin/activate    # torch + onnx already installed
python3 scripts/run_iteration_loop.py --iterations 50 --games-per-iter 100 --simulations 200
```
Then play the result:
```bash
cargo run -p netrunner_cli --features onnx -- --runner human --corp onnx \
  --corp-deck discretion_advised --runner-deck stolen_goods
```

### 4. Gym & Self-Play Harness (`netrunner_gym`)
- [x] **Fog-of-war-respecting observations.** `encode_observation` builds a `ClientView` internally and encodes only that — training already runs under masking. (`env.rs` still holds a raw `GameState` field for stepping; cosmetic, and subsumed by Phase 1.5.)
- [x] **Vectorized feature extraction.** Dense `OBS_SIZE`-length observations plus action masks over the fixed `ActionSpace`, exposed to Python via PyO3.
- [ ] **Headless Self-Play Benchmark Suite:** multi-threaded high-volume bot-vs-bot runs for Elo calibration and policy evaluation.

*Elevation* remains embedded as catalog-only metadata with no DSL implementations, so the fourteen SG+Elevation sample decklists on the same NSG page are not yet buildable.

---

## 🏆 Phase 3: Bot Personalities, Elo & Player Progression

### 1. Bot Personalities & Playstyle Archetypes
- [ ] **Configurable Bot Traits:** distinct archetypes (Fast-Rush Corp, Glacier/Late-Game Corp, Aggressive Runner, Trap/Net-Damage heavy).
- [ ] **Biased Evaluation Function:** adjust heuristic values and tree-search node evaluations to reflect personality traits.

### 2. Rating & Elo Engine
- [ ] **Persistent Multi-Track Elo/Glicko-2 System:** separate ratings for Human vs. Human (competitive rank), Human vs. Bot (single-player skill progression across difficulty tiers), and Bot Benchmark (internal algorithm comparison).
- [ ] **Role-Specific Asymmetry:** independent Corp and Runner ratings across all three tracks.

---

## 🌐 Phase 4: Network Resilience & Server Infrastructure

### 1. Event Stream to Clients
- [ ] **Send `GameEvent`s to clients at all.** Currently `ServerMessage` carries only `ClientView` snapshots; engine events never reach a client, so a networked UI cannot narrate or animate what happened. Add events to `ServerMessage` first.
- [ ] **Then mask them per viewer.** `GameEvent` variants stream raw card IDs and need recipient-specific sanitization (e.g. stripping unrevealed `CardAccessed` IDs for the non-accessing player). **Order matters:** this was previously scoped as sanitizing a stream that reaches clients — it doesn't yet. Build the stream, then the mask.

### 2. Reconnection & Session Recovery
- [ ] **Session Token Handshake:** allow dropped WebSocket clients to reconnect within $N$ seconds and re-sync from a fresh `ClientView`.

### 3. Multi-Match Daemon & Matchmaking
- [ ] **Multi-Room Server Daemon:** expand beyond single-slot lobby pairing into concurrent matches, spectator channels, and turn timers.

### 4. Transport Efficiency
- [ ] **State deltas instead of full snapshots** — only once profiling shows full `ClientView` broadcasts are actually a bottleneck. Full snapshots are simple and correct; do not trade that away speculatively.
