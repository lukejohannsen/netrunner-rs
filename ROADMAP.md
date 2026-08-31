# Netrunner Workspace Roadmap

Canonical single source of truth for engine mechanics, client-server infrastructure, single-player card data, and AI bot development across `netrunner_core`, `netrunner_bots`, `netrunner_single_player`, `netrunner_server`, `netrunner_cli`, `netrunner_gym`, `netrunner_selfplay`, and `netrunner_card_sync`.

**Current goal:** a solid single-player Netrunner, then expansion into network play.

**Working order:** Phase 1 §2 (UI legibility — **done**) → Phase 1.5 (Session Unification — **done**) → Phase 1 §1 (Saved Decks — **done**) → Phase 1.75 (Learn to Play), which is next.

> Two items used to each claim primacy — §1 was labelled "top priority" while Phase 1.5 was "the highest-value architectural work in the repo right now." Both were accurate about their own axis and useless as an ordering. They are now scoped explicitly: §1 is the top **user-facing** gap, Phase 1.5 the highest-value **architectural** one, and the line above is the actual sequence.

**Health as of the last full review:** 822 tests passing, 0 failing, 1 ignored (network-gated live sync). `cargo clippy --workspace --all-targets` completely silent. The engine's purity boundary, determinism model, and masking layer all hold. Remaining work is about *seams* — the places single-player and network play currently diverge — not about engine rot.

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

### 1. Saved Decks End-to-End — DONE

**The framing changed, deliberately.** This was scoped as *deck import*: parse a NetrunnerDB JSON export into a playable `rules::Deck`. What shipped instead is a **saved deck format** — deckbuilding happens against the cards this engine actually implements, not against an external file a user can get wrong. `deck::Decklist` and `deck::validator` are kept, but reached by *converting* a saved deck into their numeric-keyed shape; nothing parses user-supplied NetrunnerDB JSON. Importing a decklist someone else built is a separate feature, and not one this bullet ever really described.

- [x] **`decks::DeckFile` is the one authored deck format — DONE.** The old `SampleDeck` grew `category`, `description` and `how_to_play` (markdown) and learned to load from disk, so the seven published NSG decks became ordinary decks that happen to ship embedded. One parser, one validator, one lister. `netrunner_core` still performs no I/O: it exposes `from_json`/`to_json` and the CLI's `deck_store` owns directories, reads and writes.
  - **`DeckCategory` defaults to `Custom`, not `Sample`, and that is load-bearing.** `decks::matchups()` now filters to `Sample`, and its consumers are all training or verification harnesses (`netrunner_selfplay`, `netrunner_gym`, both agent-driven sweeps). A deck file that forgets to state its category is therefore excluded from training rather than silently added to it — the safe direction. This lands Phase 1.75 §3's planned `kind` field early, under a different name.
- [x] **The three deck types and two validators are reconciled — DONE.** They are *not* merged, because they answer genuinely different questions; `decks::DeckFile::validate` is the single seam that runs both, so no caller has to know which to invoke. Gameplay executability is checked first — "references a card the engine cannot play" is more fundamental than "two influence over". The pipeline is documented in `deck/mod.rs`'s module doc. `agenda_point_range` stays the only shared rule; `MAX_COPIES_PER_CARD` stays duplicated on purpose.
  - **The deckbuilding validator had never run on anything.** `ValidationReport` had no caller in the workspace. It is now checked against all seven published decks, which spend 14-15 of their 15 influence — tight enough that an error in the influence model would show up as an over-budget deck rather than passing unnoticed.
- [x] **Wired to the CLI — DONE.** `--corp-deck`/`--runner-deck` resolve a built-in id, then a saved deck name, then a path, and validate with both validators before `GameState::setup`. **Play-time validation is a hard failure**: `setup` would catch an unplayable deck by itself but not an illegal one, so without this "legal" would mean something different depending on how it was checked. A saved deck may not reuse a built-in id — an error naming both, never a silent shadow.
  - New `deck` subcommand: `list`, `show` (description, how-to-play prose, cards grouped by printed type, legality), `validate`, `new`, `add`, `remove`. Editing re-validates and reports, but a failure after an edit is a **note, not a rejection** — a deck under construction is legitimately illegal, and refusing would make it impossible to build one card at a time.
  - Decks live in `dirs::data_dir()/netrunner/decks`, overridable by `--decks-dir` then `NETRUNNER_DECKS_DIR`. Writes are temp-file-plus-rename, same reasoning as `netrunner_card_sync`'s cache.
  - **Watch out when adding a CLI module:** `netrunner_cli` has no `lib.rs`, so `tests/onnx_opponent.rs` reaches source through `#[path = "../src/..."]` and the test binary *is* the crate root. Every module a path-included module reaches for must be declared there too.

### 2. Make Game State Legible to a UI — DONE
- [x] **Card counters reach `ClientView` — DONE.** `PublicInstalledCard::counters` is `Option<u32>`, masked on **exactly the same condition as the card's identity** (`identity_visible = owner_view || rezzed`) — `mask_installed_card` reuses that existing local rather than restating the rule, so the two cannot drift apart. `PublicInstalledRunnerCard::counters` is a bare `u32`: the rig is never masked, so there is no unrezzed state to leak from and no rule to express.
  - **`Option` rather than a `u32` defaulting to `0`,** so the view never collapses "concealed" into "rezzed and genuinely empty". The current TUI renders both as no badge, but the masking layer knows the difference and shouldn't lie about it; `zero_counters_on_a_rezzed_card_is_some_zero_not_none` pins that.
  - **The kind of counter is deliberately not in the view.** `counter_kind` is static `CardDefinition` data and every client already holds a `CardRegistry`; duplicating it would be two sources of truth. `netrunner_cli`'s `counter_label` resolves it registry-side and renders `", 3 virus"`.
  - **The doc comments were the real bug.** Both `counters` fields claimed *"no card uses this yet, so there's no visibility question to answer until one does."* **Twelve cards use counters** — verified end-to-end: a real System Gateway match reaches a rig holding `smartware_distributor` with 44 credit counters that its own owner previously could not see.
  - `PublicRunIce` still carries no counters field — no ICE uses them. Add it when one does.
- [x] **`GameState::turn` — DONE.** Counts **each side's turn separately** (`0` through both mulligans, `1` = Corp's opening turn), incremented at the single point a turn begins — `turn::enter_start_of_turn`, the only site emitting `GameEvent::TurnStarted`. Placed *after* that function's Corp deck-out return, so a Corp that cannot make its mandatory draw never counts the turn it failed to start; `turn` and `TurnStarted` can therefore never disagree.
  - **`netrunner_single_player` no longer reconstructs it.** `MatchHistory` records `self.state.turn` read from the **pre-action** state, which is what preserves the existing convention that a turn-ending action is logged under the turn it ended rather than the one it started. Recorded numbers are unchanged; the session-local counter and its `TurnStarted` watch are deleted.
  - `determinize` copies `view.turn` straight through rather than resampling — it is public information, and a search tree that disagreed with reality about the turn number would mis-evaluate any future "on turn N" effect.

> **Test-fixture cleanup landed alongside this.** `GameState`'s `Default` impl exists precisely so adding a field "fails to compile in exactly one place instead of across ~43 test literals" — but the literals had never been converted, so a new field broke 18 of them. They now use `..Default::default()`, per AGENTS.md's Testing Rule. The `Default` impl itself stays exhaustive, which is the whole point of it.

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

- [x] **Every playable card carries its catalog join key — DONE, and it corrected four cards.** 19 baseline Core Set cards had no `numeric_id`, and since faction, influence cost, deck limit and set code all join from the catalog on that key, those cards carried *none* of it — playable, but silently neutral/0-influence/no-set to any deckbuilding check. The Core Set catalog is now embedded (`data/cards/core.json`) and all 19 are joined, pinned by `every_playable_card_carries_a_numeric_id`. That is what makes the slug → numeric mapping behind `DeckFile::to_decklist` total.
  - **The parity test then ran against those cards for the first time and found four real bugs:** *Account Siphon* cost 2 → 0, *Gordian Blade* cost 2 → 4, and both *Corroder* and *Gordian Blade* were missing their 1 MU entirely. Same class as the *Malapert Data Vault* bug the test caught on its first run.
  - **`Data Mine` (01076) cannot be represented and is excluded explicitly.** It is ICE with keywords `"Trap - AP"` and no Barrier/Code Gate/Sentry subtype, while `CardType::Ice` carries a mandatory `IceType`. Filtered by a reasoned `CATALOG_UNMODELABLE` list *before* conversion, so an unexpected failure still aborts loudly — the same discipline `SG_UNIMPLEMENTED` applies to coverage.
  - **The observation vocabulary needed defending.** Core Set's `01xxx` codes sort below System Gateway's `30xxx`, so a plain `numeric_id` sort would have pushed all 75 SG cards 19 slots along, invalidating every exported policy for a change that added no new card. `observation::set_rank` keeps whole sets from interleaving; verified that **zero System Gateway slots moved**. Keep new sets appending.
- [x] **Snare! models its printed text — DONE, and it gave `interactive_on_access` its first user.** The card had `cost: 4` (its pay-to-fire cost misfiled as a *rez* cost), an invented `trash_cost: 3`, and an unconditional on-access trigger. Real text: rez 0, trash 0, "when the Runner accesses this asset anywhere except in Archives, you may pay 4[c]. If you do, give the Runner 1 tag and do 3 net damage."
  - **`InteractiveOnAccess` was dead code before this** — fully implemented and tested, used by **zero cards**, exactly like the prevention subsystem below. Its semantics were also only half the story: Runner-pays-to-*avoid* (Fetal AI). Snare! is Corp-pays-to-*apply*.
  - New `dsl::AccessInteraction` captures both as mirror images — opposite payer, opposite polarity — so `resolve_access_trigger` is **one** function taking `paid` rather than two that could drift apart. `AccessPhase::PendingInteractiveTrigger` carries a `decider` because `current_actor` takes no `CardRegistry` and must still name the right player; `current_actor` gained a step for it, after the paid-ability window and before the phase (which is `Action(Runner)` throughout a run and would otherwise hand the Corp's decision to the Runner).
  - The "except in Archives" clause is a new `InteractiveOnAccess.requirement` plus an `AccessingArchives` requirement, spelled `Not(AccessingArchives)` with the existing combinator. Checked at *presentation*, not resolution: parking a decision that resolves to nothing would announce a trap that cannot fire. The "must reveal it in R&D" clause needs nothing — access already reveals.
  - `PlayerAction::PayToAvoidAccessTrigger` → `PayAccessTrigger`, since paying no longer implies avoiding. Same "one word, one meaning" cleanup as `PurgeTags` → `ClearTags`; no `ActionSpace` index moved.
- [ ] **Basic click actions are legal in the middle of a run.** `engine::draw_card_click` (and its siblings) guard on `GamePhase` and open paid-ability windows but never on `active_run`, so the Runner can draw cards and take credits while parked mid-run — including while the Corp owes a decision. Pre-existing and unrelated to the access work above; found by a Snare! test asserting the Runner had nothing to do. Needs a decision on which actions a run should suspend before it is fixed.

> **Note — the prevention subsystem is currently dead code.** `Effect::PreventDamage`/`PreventTrash`, `state::PendingPrevention`, and `WindowCheckpoint::Prevention` are fully implemented, wired into `evaluate_effect`, and tested, but **zero cards use them**, so no real game reaches any of it. Worth knowing before extending it — and worth a card to exercise it before trusting it.

### 4. Engine Hygiene Before the Next Set
- [x] **Effect resolution context — DONE.** `ability::ResolutionContext` is threaded through `evaluate_effect` and `check_requirement`, carrying the acting card (absorbed from the old `acting_card` parameter, so arity is unchanged), the triggering event, and any cards a `DealDamage` discarded earlier in the same `Sequence`. Built at the top of a resolution, dropped when it ends, never serialized. See AGENTS.md's State Hygiene Rule for the current guidance.
  - **Only two of the three fields were scratchpad.** `last_discarded_cards` and `last_advancement_was_first` are gone. **`last_completed_run` stays on `GameState` and is not debt:** `Trigger::OnRunEnded` can be deferred into `deferred_triggers` and fire on a *later* `PlayerAction`, by which point no resolution context exists — and it is the dispatcher's only handle on `persistent_after_trash` cards the Runner trashed mid-run, since `active_run` is already cleared. Moving it would have broken *Zahya Sadeghi* and *AMAZE Amusements* in exactly the cases the deferred-trigger queue was built to fix.
  - **`WasFirstAdvancementThisCard` needed no context field at all.** `GameEvent::CardAdvanced` already carries `advancement_tokens`, and `== 1` *is* "this was the first" — the `GameState` field held nothing the event didn't. Making the triggering event visible deleted it outright and left a general mechanism behind: the next trigger needing its own event's payload reads `ctx.triggering_event`.
  - **`DeferredTrigger` gained `event: Option<GameEvent>`**, so a deferred trigger rebuilds the context it would have had. Without it, deferring an `OnAdvance` would silently report "not first" for an advancement that was — the same stale-read class this refactor removed, reappearing at the defer boundary. Guarded by `a_deferred_trigger_still_sees_the_event_that_fired_it`.
  - `damage::apply_damage` now returns `(Vec<GameEvent>, Vec<CardId>)` rather than writing discards to state. Returned explicitly, not derived from the events: the flatline path empties the grip without emitting a `CardDiscarded` per card.
  - Pure refactor — 774 tests passing before and after, with no test's expectations changed.
- [ ] **Per-card recurring credits — deliberately deferred, not forgotten.** `CorpState::recurring_credits` is a single Corp-wide pool sourced only from the identity. Real recurring credits are per-card (*Cyberfeeder*, *Net Mercur*), and the pool would need to move onto `InstalledCard`/`InstalledRunnerCard`. **Do not build this until a card needs it:**
  - **No card needs it.** `recurring_credits` is declared by exactly one card — *NBN: Making News*, an identity — and spent in exactly one place, the Corp's trace bid.
  - **The design is not knowable yet.** Real per-card recurring credits are *purpose-restricted* (*Cyberfeeder*: install/icebreaker costs; *Net Mercur*: stealth). That needs a restriction vocabulary in the DSL, and inventing one with no card to drive it is a guess — against §6's own "built on demand against real cards" principle.
  - **The precedent is right here in this document:** the prevention subsystem is fully implemented, wired into `evaluate_effect`, and tested, and *zero cards use it*. This would be the second such dead subsystem.
  - **The seam is already located** for whenever a card arrives: `Cost::Credits` in `ability.rs` resolves a waterfall (bad publicity → bonus run → recurring → wallet); per-card pools slot in as a list at the recurring step. Recorded so the analysis isn't redone.

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
- [x] **Two identities out of scope *for competitive play*:** *The Catalyst: Convention Breaker* and *The Syndicate: Profit over Principle* carry `stripped_text: "Starter game only."` — no rules text exists to implement. They are the only two `SG_UNIMPLEMENTED` entries.
  - **This was recorded as permanent; Phase 1.75 reverses it, deliberately.** The decision was correct while no tutorial mode existed — a blank identity legal in Standard is a deckbuilding hole, not a feature. But these are exactly the identities Null Signal's *Learn to Play* starter decks are built on, and "no rules text to implement" is precisely what makes them **trivial** to implement once there is a use for them. See Phase 1.75 §2.
- [x] **Three reprints share ids with baseline cards:** *Sure Gamble*, *Hedge Fund*, *Cleaver* — handled by keeping the single existing definition. *Cleaver*'s pre-existing definition had transposed paid-ability costs; fixed as a data-correctness bug.
- [x] **Engine bugs found by the bot-driven sweep** (all reachable in ordinary play, none by any per-card test):
  - `current_actor` ignored `pending_paid_choice`/`pending_decision`, so a decision parked *while a paid-ability window was open* named the window's priority holder instead of the decision's chooser — leaving a player with no legal action at all. Its precedence now mirrors `apply_action`'s blocking guards exactly (trace → paid choice → decision → window → phase). **These two must stay in sync; that is how this was found.**
  - `Effect::PromptChooseServer` parked a choice resolvable only by `run::start_run` without checking no run was active — an unresolvable decision blocking every action.
  - `ToggleCardSelection` enforced eligibility but not `max`, so a selection could grow past the bound `ConfirmCardSelection` requires.

**DSL growth baseline:** at System Gateway completion, **14 of 66 `Effect` variants are used by exactly one card**, against a healthily reused core (`PromptChooseCards` 17 cards, `PresentChoice` 16, `EffectIf` 12). Measure the next set against this ratio — see AGENTS.md's DSL Growth Rule.

---

## 🔗 Phase 1.5: Session Unification (the single-player → network bridge) — DONE

Five places independently re-implemented the same match loop — `current_actor` → get action → `apply_action` → check `GameOver` — each with its own step budget. There is now **one `MAX_STEPS`, in `netrunner_session`**, and every caller pumps the same driver.

- [x] **Extracted the shared loop** into `netrunner_session` (new crate, deps `netrunner_core` + `netrunner_bots` + `serde` + `thiserror`), sitting between `netrunner_bots` and every consumer. Homed in its own crate rather than `netrunner_core`: the seat enum holds a `Box<dyn BotAgent>`, which core cannot name.
- [x] **Unified the seat interface on `ClientView` + `PlayerAction`.** The local human seat is now `Seat::External` and *structurally* cannot see `GameState` — the client-politeness hole is closed. `PlayerDriver` was **deleted outright**, not adapted: it was a blanket-impl'd clone of `netrunner_bots::Agent` with an identical signature, and every `Box<dyn PlayerDriver>` in the tree was already an `Indexed*Agent`. The index-based shape survives only inside `SinglePlayerSession`, which is the RL path.
- [x] **Two seat variants, not four.** The plan called for `Bot`/`LocalHuman`/`Channel`/`Indexed`. Under a **pull-shaped** loop (`fn step(&mut self) -> SessionStep`, which yields `Awaiting { side, view }` and stops) the last three collapse into one: a seat the session cannot resolve itself. They differ only in who pumps and in the `PlayerAction` ↔ index conversion at the boundary, which is `ActionSpace::action_at`'s job. **Do not re-split them.**
- [x] **`MatchHistory` moved into the driver**, so every path records actions and events — which is what gave the network path a game log (`ServerMessage::ActionLog`, one message per action; `App::action_log` renders it). Both TUI paths now share one four-region layout; the log-less `build_layout` is gone.
- [x] **`GameEndReason`/`classify_end_reason` moved to `netrunner_session::outcome`**, re-exported from `netrunner_server` so the wire protocol is unchanged. `netrunner_cli` used to depend on the whole server crate purely to classify end reasons on its *offline* path.

**Ported:** `netrunner_server::MatchSession` (now purely the async pump), `SinglePlayerSession` (thin index adapter), `netrunner_cli`'s local TUI, `netrunner_gym::env`, `netrunner_selfplay`, and the fifth copy that lived in `post_action_windows_stay_rare`.

**Two behavioural changes worth knowing:**
- `netrunner_gym`'s budget was `1_000` *per fast-forward call*, reset on every call — so a pathological episode was effectively unbounded. It is now one `MAX_STEPS` for the whole episode. `max_episode_steps` truncation stays in the env; it is an RL concern.
- `StallReason` splits `NoCurrentActor` / `NoLegalActions { side }` / `BudgetExhausted`, which the old `else { break }` conflated. `NoLegalActions` also stops a deadlocked position from *panicking*: every `BotAgent` asserts a non-empty `legal_actions`.

### Deadlock and crash: "a run can outlive the game it belongs to" — BOTH FIXED

Two bugs, one invariant. `damage::apply_damage` flatlines the Runner by setting `phase = GameOver` **without clearing `active_run`**, so a run can sit parked at `EncounterIce` after the game has ended. Two places then acted as though it were still live:

- [x] **Crash.** `open_window_if_at_checkpoint` asked `open_window` for a window, which reads the priority side straight off `phase` and hit its `unreachable!()`. It surfaced inside `build_client_view` — `legal_actions` probes candidates through `apply_action` — so merely *rendering* such a position brought the process down, putting `MatchSession::broadcast_state_updates` on the same hook. Guarded on `phase` being `Action(_)`.
- [x] **Deadlock.** One statement earlier, `resolve_encounter_ice`'s advance guard checked `active_run` and the four parked-state fields but never `phase`. Since `resolve_unbroken_subroutines` breaks its loop at `GameOver`, the rest of a multi-subroutine ICE stays `Pending`; advancing then handed `continue_run` the one thing it refuses (`SubroutinesStillPending`), and that `Err` propagated through `close_window` into `pass_priority`. Because `legal_actions` keeps only candidates `apply_action` accepts, that made the priority holder's **own `PassPriority`** illegal while `current_actor` still named them — no legal action at all, deterministically, forever. Reproduced at `decks::matchups()[0]` seed 2.

**The real lesson is the detection gap, now closed.** Both were trivially reachable on ordinary sample decks at low seeds, and both were invisible to every existing sweep, because those drive *index-based* agents whose `ActionSpace` round trip does not reach the path. `crates/netrunner_session/tests/no_deadlock_sweep.rs` now drives view-based `Seat::Agent`s, so each side chooses only from `legal_actions_for` — the per-seat `ClientView` slice a real client gets. See AGENTS.md's Testing Rule for the division of labour between the two sweeps.

### Known adjacent hazard: `Effect::Sequence` keeps resolving after the game ends

`Sequence` (`ability.rs`) breaks only on `is_resolution_blocked()`, not on `GameOver` — the same "kept going after the world ended" class as the two above. The one reachable case in the current pool (Karunā's `Sequence[DealDamage(Net,2), PermitJackOut]`) is harmless: the flatline leaves `active_run` set, so `PermitJackOut` succeeds. **Deliberately not "fixed"** — no current card turns it into an error, so a guard would be speculative. Revisit if a card ever puts a run- or encounter-dependent effect after a potentially lethal one in a `Sequence`.

---

## 🎓 Phase 1.75: Learn to Play

**The repo can play Netrunner but cannot teach it.** A new player launching `netrunner_cli` gets a flat list of legal actions and no explanation of clicks, servers, runs, or why any of it matters. Netrunner is famously hard to learn cold, and its asymmetry means learning it twice. This phase closes that, from both sides.

Two stages, in order:

1. **Scripted lessons** — gated, one-concept-at-a-time scenarios, a Corp track and a Runner track.
2. **A faithful NSG starter game** — the official preset starter decks, 6 agenda points to win, booster-pack staging — so a graduate gets a feel for a real game unguided.

> **The scripted lessons are our addition, not Null Signal's.** [Learn to Play](https://nullsignal.games/players/learn-to-play/) is a rulebook with training wheels, not a tutorial: fixed preset decks, a lowered 6-point win threshold, mechanics deferred to a booster pack (viruses, tags, meat damage, modal ice), and per-card clarifications keyed to the fixed deck. There are no scripted turns and no puzzles. Stage 2 is faithful to that; stage 1 is scaffolding NSG leaves to a human teacher. Don't "correct" stage 1 toward the source material later — the divergence is the point.

**The card pool is already done.** All 32 starter-deck cards and all 11 booster cards are implemented, playable System Gateway cards. Nothing needs authoring — verified card by card against `data/{corp,runner}/`.

**Placement, and why it is here rather than in Phase 1:** it does not need Phase 1's deck import, and its one hard prerequisite — Phase 1 §2's counter visibility, needed because the booster stage teaches viruses — **is now satisfied**. More importantly, the lesson driver was to be a `PlayerDriver` — the trait Phase 1.5 has now deleted. That ordering rationale has been discharged: build the lesson driver as a `netrunner_session::Seat::External` pumped by the lesson script, the same shape the local TUI now uses.

### 1. Configurable match rules (`netrunner_core`)

- [ ] **Replace the `WINNING_AGENDA_POINTS: u32 = 7` const (`rules/win.rs`) with a `MatchRules { winning_agenda_points: u32 }` struct on `GameState`** — `Default` = 7, `#[serde(default)]` so recorded histories still deserialize, read by `win::check_win_conditions`.
  - **A struct rather than a bare `u32` field, deliberately.** This is the State Hygiene Rule applied *before* the debt accrues rather than after: the starter game already varies one rule, and the second variant knob should extend a struct instead of becoming another loose field. Note that a match rule is not cross-effect context — it is fixed at setup and never threaded between effects — so this is not the scratchpad pattern that rule bans.
- [ ] **Make `deck::agenda_point_range` a function of the win threshold as well as deck size.** It currently hardcodes the 7-point assumption, which is why the starter deck fails validation today (see §3). The constraint: **34 cards at a 6-point win must admit 14 agenda points**, while `agenda_point_range_matches_size_derived_examples`' existing cases — `(40, 44) → (18, 20)`, `(45) → (20, 22)` — stay unchanged at 7.
- This touches `win.rs`, so it is engine-level by AGENTS.md's Testing Rule: **run `NETRUNNER_SWEEP_SEEDS=256 cargo test -p netrunner_single_player --release` before merging.**

### 2. Make the starter identities playable (`netrunner_core`)

Reverses the decision recorded under Phase 1 §7 — see there for why.

- [ ] **Author `data/corp/the_syndicate.json` and `data/runner/the_catalyst.json`** as hand-authored blank identities carrying `numeric_id` 30077/30076 and `min_deck_size` 34/30. Printed metadata joins from the catalog on `numeric_id` as usual — **do not edit the `data/cards/system_gateway.json` dump.** Hand-authored-alongside-catalog is the shape `sg_reprint_dedup_tests` already documents for *Hedge Fund*/*Sure Gamble*/*Cleaver*.
- [ ] **Rewrite `starter_only_identities_have_no_rules_text_and_stay_permanently_unplayable`** (`cards/mod.rs`) to assert they are playable, **blank** (no abilities, no triggers), and carry the right `min_deck_size`. The blankness assertion is the one that matters — it is what keeps them honest as tutorial identities.
- [ ] **Remove both `SG_UNIMPLEMENTED` entries** (`cards/embedded.rs`). `every_system_gateway_card_is_implemented_or_explicitly_excluded` requires an implementation *or* an exclusion, never both, so leaving them fails the gate.
  - Its count assertion is written as `sg_total - SG_UNIMPLEMENTED.len()`, so the arithmetic self-adjusts — but only if **both** identities actually land. Implementing one and excluding the other fails it, which is the desired behaviour.
  - **System Gateway then reads 77 of 77 rather than 75 of 77.** Update Phase 1 §7's headline count when this ships; it is the first set in the repo to reach genuinely complete coverage.

### 3. Tutorial decks, without polluting self-play (`netrunner_core`)

- [ ] **Add four decklists under `data/decks/`:** starter Corp (34) / Runner (30), boosted Corp (44) / Runner (40), from NSG's published lists.
- [x] **The category field and its `matchups()` filter — DONE, landed early with Phase 1 §1.** `DeckCategory` (`Sample` | `Starter` | `Boosted` | `Custom`) is on `decks::DeckFile`, `matchups()` filters to `Sample`, and the trap this bullet warned about — starter decks silently **training the policy network on tutorial decks** — is closed. `Starter` and `Boosted` already exist as variants; the four decklists just need authoring against them.
- [ ] **Extend `every_sample_deck_is_legal` to validate the starter decks under their own `MatchRules`**, which implies a deck record carries its variant's win threshold rather than the validator guessing. Note the starter identities carry `influence_limit: null` in the catalog, where `deck::validator` assumes the flat `DEFAULT_INFLUENCE_LIMIT` of 15 — worth checking that assumption before relying on it for a 34-card starter deck.

### 4. Deterministic stacked openings (`netrunner_core`)

- [ ] **Add a fixed-order setup path** — e.g. `GameState::setup_with_order(..., DeckOrder::{Shuffled, Fixed { corp, runner }})`, with today's `setup` becoming `Shuffled`. A lesson cannot teach a specific play if it cannot pin which cards are drawn.
  - **`Fixed` must be validated as a permutation of the expanded deck**, so a lesson cannot smuggle in a card the decklist does not contain. Cheap, and it keeps `validate_deck` authoritative rather than bypassed.
  - Draw convention, for whoever implements it: `corp.r_and_d` / `runner.stack` are plain `Vec<CardId>` and draws `pop()` from the **end** — the last element is the top card.

### 5. Lesson content as data (`netrunner_core::tutorial`)

- [ ] **Lessons live in `data/lessons/{corp,runner}/*.json`**, embedded by `build.rs` exactly like cards and decks, with `#[serde(deny_unknown_fields)]`. Never hardcoded in Rust — same house rule as cards, and for the same reason.
- [ ] **A lesson is `{ stacked deck order, scripted opening, ordered steps }`.** The scripted opening fast-forwards to the position being taught (e.g. "turn 3, a rezzed *Palisade* protects a remote holding a 2-advanced agenda") via a `ScriptedDriver: PlayerDriver` returning canned `ActionSpace` indices, then handing over to the human.
- [ ] **A step is `{ prose, allow: ActionPredicate, advance_when: EventPredicate }`**, advancing off the observed `Vec<GameEvent>` — reachable on the local path through `SinglePlayerSession::with_observer`.
- Homed in `netrunner_core` because it depends only on `GameEvent`/`PlayerAction`/`ClientView` and must be reachable by any client, not just the TUI — the same reasoning that already puts `decks` there. It adds no dependencies.

### 6. Gating that does not break the client contract (`netrunner_cli`)

**This is the item most likely to be got wrong, so state the rule up front:**

> **A lesson step narrows `view.legal_actions`; it never widens them.** Presenting a subset is a UI affordance, like sorting — it cannot make an illegal action legal, so it does not violate AGENTS.md §3's ban on a client re-deriving legality. Gating logic must never call `apply_action` and must never reimplement a legality check.

- [ ] **Gate on a predicate plus an escape hatch, never on a fixed action index.** A step whose `allow` predicate matches nothing leaves the player with no action at all — the exact failure mode of all four deadlocks the sweep has found, reintroduced at the UI layer. A key that reveals the full action list is the escape.

### 7. TUI work (`netrunner_cli`)

- [ ] **A coaching/prose panel.** `tui/layout.rs` has two hardcoded builders (`build_layout`, `build_layout_with_log`); add a third or parameterize them.
- [ ] **Build a modal system — there isn't one.** Today there is only the `centered_rect` + `Clear` + bordered-`Paragraph` idiom, with no popup stack and no key routing: both `App::handle_key` and `prompt_human`'s inline match dispatch keys unconditionally to the action list. A lesson intro/outro card needs a "modal open, swallow keys" branch that does not currently exist.
- [ ] **`explain_action(action, registry) -> String`**, the mechanical sibling of `describe_action` (`app.rs`), which already matches exhaustively over all 42 `PlayerAction` variants. `PurgeVirusCounters`' cost-spelling label is the existing precedent for a pedagogical string.
- [ ] **A run-phase strip** showing NSG's six run phases with the current one highlighted. High value, nearly free — `view.active_run.phase` is already on `ClientView`, and the [Run Timing Guide](https://nullsignal.games/players/learn-to-play/run-guide/) is the one reference both NSG role guides share, because runs are the fulcrum of each.
- [ ] **Render `App::last_rejection`.** It is set from `ServerMessage::ActionRejected` and then **displayed nowhere**. Tolerable today; indefensible in a tutorial.
- [ ] **A `Learn` subcommand** selecting side, lesson, and starter-vs-boosted: extend `config::Command` and the `match` in `main.rs`, which today has only `Cards` and `None` arms.

### 8. The lesson tracks

Intended scope, not a frozen spec — both sides, following NSG's own concept order.

- [ ] **Corp** — clicks & the mandatory draw → playing an operation (*Hedge Fund*) → servers & installing (HQ/R&D/Archives vs. remotes) → ice & rezzing (*Palisade*) → advancing & scoring (*Offworld Office*) → defending a scripted run → traps & net damage (*Urtica Cipher*) → graduation.
- [ ] **Runner** — clicks & credits (*Sure Gamble*) → programs & MU (*Cleaver*) → a first run on empty Archives, walking the six phases → breaking ice (boost strength, break the barrier) → accessing & stealing → R&D vs. Archives breaches and multi-access (*Jailbreak*) → jacking out, and when not to run (*Tread Lightly*) → graduation.
- [ ] **Graduation hands off to the starter game:** starter decks, 6 points to win, no gating. The booster unlock is stage two and introduces viruses, tags, meat damage, and modal ice — the four things NSG withholds from a first game.

### 9. Test gates

- [ ] **Every lesson is completable** — drive each with a scripted driver playing the intended solution; assert every step advances and the lesson reaches its end state. The lesson analogue of `every_sample_deck_matchup_finishes`.
- [ ] **Every gated step offers at least one action** at every point it is live. The deadlock analogue, and the reason §6 gates on a predicate rather than an index.
- [ ] **Starter decks validate** under starter `MatchRules`, and the starter matchup plays to a result at 6 points.
- [ ] **Replay determinism survives** the new `MatchRules` field — the existing bit-identical-replay assertion must still hold.

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
