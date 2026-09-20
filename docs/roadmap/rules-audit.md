# Rules Audit (August–September 2026) and the Card Fidelity Audits

Area roadmap; the index is `ROADMAP.md`. Addresses used by code comments: "Rules Audit §0" (harness), "§0.5", "T1"–"T12" (Tier 1), "Tier 2" / "§2", "§3" (stale comments), "§4" (adjacent hazards), "Forfeit and Region", "follow-ups", and **"B.10"** — the layout-v2 item from the audit's original working list, closed under T11.

Phase 1 §3's memory-cost bug (no program installable, every test green) prompted three read-only passes: the candidate/handler seam, fidelity against Null Signal Games' rules, and what the bots actually measure. **Twelve Tier-1 findings fixed in ten branches; Tier 2 closed in seven more; six adjacent-hazard classes closed.** Every recorded game and exported policy before this is stale twice over: the rules changed, and `ActionSpace::SIZE` went 1357 → 1646.

**How twelve rules stayed hidden.** `legal_actions` keeps only what `apply_action` accepts, so a candidate never generated is never probed and an under-strict handler is laundered as legal. Sweeps asserted only reachability. Self-play recorded tensors, never a `PlayerAction`. And heuristic-vs-random — both sweeps' only seatings — **never produced one `IceEncountered`**.

## 0. The rules-coverage harness — DONE

- `netrunner_session::Coverage` aggregates a `MatchHistory` into counts per `PlayerAction` variant and side, per `GameEvent`, per card (installed / rezzed / played / activated / accessed / stolen / scored / trashed / subroutines fired / broken / click-broken; prompt cost since Phase 2 §5), per run outcome, `Effect`s seen, and `triggers_fired` (off `GameEvent::TriggerFired` since Tier 2). `BTreeMap`s so `to_json()` diffs. Homed in `netrunner_session` (it owns the record); core gained only `Effect::for_each_effect`/`variant_name` and `PlayerAction::VARIANT_NAMES`.
- `netrunner_cli --headless` is the driver: real sample decks, `--all-matchups`, `random|heuristic|mcts|puct` seats, `--index-path`, `--report`, `--verbose`. A stall is counted, never an error.
- **Gates in both sweeps** (`Coverage::gate_failures`): every action variant applied, every sample-deck card seen, every `LOAD_BEARING_EVENTS` entry emitted. Exclusions are claims with reasons: `ACTIONS_UNREACHABLE_WITH_SAMPLE_DECKS` (trace bids — no card has a `Trace`) and `ACTIONS_RARE_WITH_SAMPLE_DECKS` with a game-count threshold (`TrashResource` 512, `BreakSubroutineWithClick` 256, `RemoveTag` 128). Both sweeps gained a **random-vs-random seating** — the only one that reaches runs.
- **First-run findings:** a deadlock (seed 85 — Anoetic Void ended the run, the queued Skunkworks trigger fired against no run and parked an unaffordable, undeclinable choice); `dispatcher::still_applies` drops run-scoped triggers once `active_run` is gone, in `fire_one`. Baseline (96 games, seed 1): random/random 427 `IceEncountered`; **both heuristic seatings 0 encounters, 0 subroutines**; random Corp vs heuristic Runner ended 88 of 96 by Corp deck-out.

## 0.5 The heuristic can now play — DONE (`feat/heuristic-values-advancement-and-breakers`)

Prerequisite for T1. `evaluate_state` takes the registry: an unrezzed install 0.6, rezzed ICE 2.0, advancement 1.5 up to the requirement; the Runner 2.0 per breakable ICE subtype, +0.6 during a run, −1.0 per unbroken subroutine, −0.5 with an own decision parked (that last term fixed Tāo's toggle livelock at seed 82). Measured: heuristic Corp `IceEncountered` 0 → 723; heuristic Runner `RunInitiated` 0 → 1,858; deck-outs 88 → 0. **The conservation invariant then found `remove_installed_card` double-pushing every selection-trashed card** (marjanah ×3 of 2); removal now returns the card, keyed by `InstallId`.

## 1. Tier 1 — a core rule non-functional in ordinary play — DONE

- **T1 Any subroutine breakable for free** (`fix/no-free-subroutine-break`). `PlayerAction::BreakSubroutine`, its handler, generator, `ActionSpace` arms and `Effect::BreakSubroutine` **deleted** (zero cards). Breaks are `BreakSubroutines` via a breaker's ability or `BreakSubroutineWithClick`. Its segment stayed a documented hole (reclaimed under T11 / B.10); that hole exposed the `in_segment` underflow. Random: `SubroutineBroken` 747 → 130, `SubroutineFired` 670 → 1,230; heuristic `RunEndedByEffect` 0 → 535 — ICE stopped a run for the first time. `every_sample_deck_matchup_finishes` lost its card gate (48 games is too small a sample).
- **T2 A stolen agenda never left the Corp's zone** (`fix/run-and-access-bookkeeping`). `remove_from_corp_zone` shared by steal and trash, preferring the zone the access named. **The card-conservation invariant** (every card in exactly one zone, checked every step of the fog sweep) is the real gate.
- **T3 Rez cost paid at install and at rez; ICE had no install cost** (`fix/corp-install-economy`). Root installs free, ICE 1[c] per ICE already protecting the server, only `rez_ice` pays print. Seven tests had pinned the double charge. `CardInstalled` 1,123 → 1,678, `IceEncountered` 533 → 890.
- **T4 No Corp click-to-draw** (`feat/action-space-v2`). `DrawCardClick { side }`; refused on an empty R&D. `Corp/DrawCardClick` 0 → 334.
- **T5 No one-agenda-or-asset-per-remote, no install-over** (`fix/one-card-per-remote`). Installing over trashes the occupant (faceup if rezzed); `fresh_remote_id` recycles ids under `MAX_REMOTE_SERVERS = 10`. `CardTrashed` 87 → 279.
- **T6 Scoring cost a click** (`fix/scoring-is-free`). Free, legal on zero clicks; stays in `classify_action`'s click group for the two guards it shares.
- **T7 Uniqueness unmodelled** (`feat/uniqueness`). `CardDefinition::unique` joined from the catalog; `trash_earlier_unique_copy` at install — an install-time rule, decks stay legal.
- **T8 Encounter strength buffs survived an end-the-run**. **`run::end_run` is the only way a run leaves `GameState`** (six hand-rolled sites folded in). Found alongside: `close_window`'s `Success` arm pushed `RunCompleted` and never dispatched it — Mayfly never fired on an empty-Archives run.
- **T9 `RunSucceeded` fired at approach, before the breach** (`fix/approach-server-before-success`). `ServerApproached` carries `OnApproachServer`; success and everything keyed to it fire in `complete_run`, which also **closes the jack-out window** (a random Runner jacked out after half its successful runs). `RunCompleted` 409 → 585 = `RunSucceeded`.
- **T10 ICE rezzable at any window and on the Corp's own turn** (`fix/ice-rezzes-only-when-approached`). ICE only while that install is approached (`IceNotBeingApproached`); assets and upgrades at any priority. `RunIce.install_id` is public. `AgendaScored` 22 → 54 — credits no longer burned on unthreatened ICE.
- **T11 `MAX_PENDING_CHOICE_OPTIONS = 2` against Ansel/Brân's three** (`feat/action-space-v2`). Now 4; `every_card_fits_the_action_space_caps` walks every card's JSON against every cap. **Layout v2, `SIZE` 1357 → 1646 (B.10)**, taken as one break: the sided draw, `MAX_HAND_SIZE` 16, `MAX_ACCESS_SELECTION` 32, `MAX_DECK_ZONE = 50` for `ToggleCardSelection`, T1's hole reclaimed, purge folded into the unit block.
- **T12 An upgrade on Archives was never accessed**. Two lines in `compute_accessed_cards`.
- **Played Events now go to the Heap** — `play_event` put them nowhere; caught by conservation.

## 2. Tier 2 — narrower gaps, caps, aliasing — DONE

Re-reading against the code found three items live, four already fixed or mis-described.
- Index caps: raised by T11. `MAX_TRACE_BID` stays 30 — `Trace` is card-dead.
- **Programs never trashed when memory dropped** (`fix/memory-limit-enforced`): `memory_balance` is a signed ledger; `enforce_limit` at the choke point parks a 1-of-N `ChooseCards` over programs until the rig fits, emitting `MemoryLimitExceeded { over_by }`.
- **The run's ICE list was a snapshot** (`fix/run-ice-reconciliation`): Brân's inward install was never encountered. `run::reconcile_ice` rebuilds from `corp.installed` by `InstallId`, keeping per-run state, at the choke point and before subroutines fire. `CannotSwapIceDuringActiveRun` deleted. 27 fixtures now call `test_support::install_the_runs_ice`.
- **Access bookkeeping** (`fix/access-bookkeeping`): `can_trash` was frozen at presentation (now live credits); facedown Archives cards never turned faceup on a breach; `AdditionalAccessGranted` no longer emitted for the no-op servers.
- **Link was structurally 0** (`feat/printed-link`): `base_link` joined from the catalog, seeded at setup (Kate is the sole non-zero).
- **Trigger coverage observed, not inferred** (`feat/trigger-fired-event`): `GameEvent::TriggerFired` from `fire_card_triggers`; 61 card/trigger pairs where inference saw 34.
- Recurring credits: correct as coded, identity-only. Multi-access: the two-counter design is right (HQ and R&D only). **Forfeit and Region:** absent from System Gateway; both closed at Elevation Stages 6–7.
- **Loose ends** (`fix/audit-loose-ends`): `install_program` accepted a trojan (`TrojanMustBeHostedOnIce`); a selection-trash of ICE stranded its trojans (cascade); selection-trashes now emit `CardTrashed` (20 cards newly visible); two hand-rolled `run.ice` rez syncs deleted.
- **`ChooseTriggerToResolve` aliased by `CardId`** (`fix/trigger-order-by-index`): carries `index` into the parked list, like `ToggleCardSelection`.
- **`apply_action` did not own the card-type rule** (`fix/apply-action-owns-card-type-guards`): an agenda could be installed as ICE and encountered as a 0-strength barrier; every handler checks type (`CardTypeMismatch`, `NotInstallableInCentralServer`), `_ => {}` dispatches spelled out. Also Brân/Ansel's "this ice" by first-match `CardId` → the encountered `InstallId`; `has_usable_paid_ability` no longer counts the identity.

## 3. Stale comments — DONE

The failure mode that hid the memory-cost bug: comments true when written. Each corrected with what is true *and* what it used to claim (`action.rs`, `setup.rs`'s backwards mandatory-draw note, `action_mask.rs`'s "real games never reach these caps"). AGENTS.md's DSL ratio was recounted.

## 4. Adjacent hazards — the classes — DONE (six branches)

Passes for "kept resolving after the game ended", "first match by `CardId`", "silent fallback / catch-all arm".
- **A central access removed the Corp's installed copy** (`fix/access-removal-zone-gate`): the fallback to "any root install with that id" now applies to remotes only. Offworld Office ×3 made it ordinary; conservation held, so the invariant could not see it.
- **A win could be silently reverted** (`fix/game-over-hardening`): Clearinghouse's parked choice under the start-of-turn window survived a flatline and play resumed. **`win::end_game` is the single transition** (clears every parked field, emits `GameOver` once); `apply_action` rejects everything with `GameIsOver`; `GameState::resolution_halted()` stops every loop. `GameOver` events 104 → 96.
- **`InstallId` never reached effect resolution** (`feat/install-id-resolution-context`): every "this card" found the first copy — two Fermenters shared a pool, Nico #2 loaded #1, three Telework Contracts shared one once-per-turn. `ResolutionContext::acting_install`; every parked struct carries `source_install`; `OncePerTurn` keyed `(tag, install)`; `DeferredTrigger { install, target_install }`; `hosted_on_ice: Option<InstallId>`.
- **Catch-all decode arms** (`fix/decode-catch-alls-and-noop-costs`): the unit block decoded every surplus index as `DeclinePendingPaidChoice`; `decode_install_slot` made any non-zero `Root`. Both exhaustive. Carnivore's empty `min: 2` prompt consumed a click and a once-per-turn → `ZoneHasAtLeast` gates the offer.
- **Five card JSONs diverged from print** (`fix/card-fidelity`): Anoetic Void (a real, declinable 2[c] paid choice), Jinteki: Restoring Humanity (`Amount::FacedownCardsInArchives`), Red Team (payout as its own run's `on_success`; `RunState::on_success_card/install`), Hansei Review, Above the Law — the last two were *wrong* here and were reversed by the SG audit below.
- **Follow-ups** (`fix/audit-followups`): `AccessState::pending_install` pins the instance being accessed (two upgrades in one root); Red Team offers only centrals not run this turn (`servers_run_this_turn`, `PromptChooseServer.exclude_servers_run_this_turn`, failing with `NoServerLeftToRun` before parking). Still by design: `CardTarget::CorpInstalled { card, server }` is DSL-authored; `OncePerTurn` on identities and events is tag-only.

## System Gateway Card Fidelity Audit — September 2026

All 75 playable SG cards read against `stripped_text`, the engine and play evidence. Per-card record: `docs/system-gateway-card-audit.md`. **64 faithful (11 via documented approximations), 11 deviations fixed** (`fix/sg-card-fidelity-audit`), one engine-wide finding. Lesson: verify against print and the rules document, never against "removes a drawback" — two deviations had been introduced by `fix/card-fidelity`.
- Hansei Review (the Corp's choice; `RandomFromHq` deleted); Above the Law ("may" restored); Predictive Planogram (three-option choice); Mayfly (trashed only after its break was used — a hosted counter as the marker); Karunā (`PermitJackOut` deleted; a real Runner choice); Zahya (`AccessedAnyCardDuringLastRun`, 191 → 75 firings); Urtica Cipher (`ThisCardIsInstalled`); NBN: Reality Plus (cost-tag events now dispatch); Mutual Favor and Pantograph (`InstallRunnerCardFromGrip` + `InstallableRunnerCard`); Ansel 1.0 (`PromptInstallCorpCard` over `ChooseServer`, named by position so the pick does not leak).
- **Engine-wide: new ICE landed innermost**, reversing every stacked server's approach order. Fixed in `engine::place_corp_card`. `IcePassed` 949 → 1,045.
- Still open by design: Docklands Pass's "breach" ≈ successful run; Trojan effect-installs. (Red Team's "not run this turn" was closed by §4's follow-ups.)

## Core Set (implemented subset) Fidelity Audit — September 2026

19 cards; record in `docs/core-set-card-audit.md`. 15 faithful, **4 fixed** (`fix/core-set-card-fidelity-audit`): Weyland: Building a Better World never fired (three Transactions lacked `subtypes`); Ice Wall (`StrengthModifier::PerHostedAdvancement`); Gordian Blade (`BoostDuration::Run`, `run_strength_buff`); Account Siphon (`SetAccessReplacement.optional`, `Amount::CreditsLostThisResolution`). Random-vs-random bit-identical — no sample deck fields a Core card, so these 19 have per-card tests only. A Core-flavoured sample deck pair is sized as its own task, not started.

## Deckbuilding: out-of-faction agendas — DONE (13 September 2026)

`fix/out-of-faction-agendas`, found building the TUI deck builder (Phase 6 §2). **The deckbuilding validator had no out-of-faction agenda rule.** Agendas print no influence, so the influence check priced a Weyland *Above the Law* in a Haas-Bioroid deck at 0 and the deck validated — in `deck add`, in the builder, and at the start of a game. Netrunner allows a Corp deck only its own faction's agendas and neutral ones. Now `DeckValidationError::OutOfFactionAgenda`, a rule of its own rather than an influence charge, beside `RunnerDeckContainsAgenda`. All 28 published sample decks still validate (none fields another faction's agenda), so play and the sweeps are untouched.

## Masking: the Corp was asked to pay for a card it could not see — DONE (17 September 2026)

`fix/access-trigger-names-its-card`, found while building Phase 7 §4o (an
access shows the card) and deliberately left out of that PR, because it is
an engine-boundary question rather than a rendering one.

**The finding.** `masking::PublicAccessPhase::PendingInteractiveTrigger`
masked the accessed card's identity by the rule that covers the whole
breach — the Runner always, the Corp only on Archives. But the two cards
that ask the *Corp* to pay, Snare! and Byte!, both carry
`Not(AccessingArchives)`. So the question was only ever put in the cases
the rule blanked: the Corp was asked "pay 4 credits?" about a card their
own `ClientView` refused to name, in every instance that can occur.

Meanwhile `legal_actions_for` handed them
`PayAccessTrigger { card_id }` / `DeclineAccessTrigger { card_id }`
carrying the real identity — it filters by `action_owner`, so only the
decider receives them, but it does not mask. **The mask was therefore not
protecting the identity from anybody; it was withholding it from the panel
that had to render the decision.** A client showing the card (which §4o
now does) could read the name off the action while the view said `None`.

**The fix is to widen the mask, not to narrow the actions.** `card` is now
named to the `decider` whatever zone the access is in. Narrowing the
actions was considered and rejected twice over: `apply_action` needs the
`card_id` to resolve, so a masked action is unsubmittable; and a player
asked to pay a cost for a specific card's ability cannot answer without
knowing which card, so hiding it does not model the printed card. An
on-access ability that asks the other player a question cannot resolve in
secret — the access is what triggers it, and the answer is public either
way. Spectators and the non-deciding side are unchanged.

**Measured, 192 games `--all-matchups` at seed 1, pinned binaries.**
- **Random-vs-random is byte-identical** (same md5), which is the claim
  that no rule changed: `RandomAgent` ignores the view's contents.
- **Heuristic Corp vs random Runner is where it shows, and the delta is
  the mechanism itself: `PayAccessTrigger` 8 → 2, `DeclineAccessTrigger`
  6 → 12.** The same 14 decisions, 6 of them flipped, because
  `determinize` no longer has to guess the card the Corp is paying for and
  the evaluator now scores the real one. Corp wins are unchanged at
  184/192, with one moving from `Flatline` (121 → 120) to
  `AgendaThreshold` (63 → 64). Everything else in the diff — `steps`
  58,662 → 59,383 and a drift of ±1–2 across most counters — is trajectory
  downstream of those six flips, not an effect in its own right.
- **Worth a look later, and not fixed here:** better-informed, the
  heuristic Corp *declines* Byte! far more often than it paid blind. Four
  credits for 3 net damage and a tag is usually a strong play, so this
  looks like the Corp evaluator undervaluing the payment rather than the
  information helping. That is an evaluator question (Phase 2 §5), and
  192 games cannot settle it.

**Two guards, because the first one nearly missed it.** The sweep already
asserted that no view names a card it conceals
(`assert_no_concealed_card_is_named`), and it did not catch this: that
check starts from the installs a view masks, so it only ever covered cards
*on the table*, and a card accessed out of R&D is in no install and in no
zone the Corp's view renders.
- `assert_actions_name_only_what_the_view_shows` is its complement and is
  strictly wider — every card a viewer's own `legal_actions` name must be
  one their own view shows them, scanned over the `Debug` rendering
  against the two decks' card ids so a new `PlayerAction` carrying a
  `CardId` is covered the day it is added. Verified by reverting the fix:
  it fails at **seed 45, `pork_chops vs shootin_n_lootin`**, with
  `Player(Corp)'s legal_actions name byte`. Note the action there is
  `DeclineAccessTrigger` alone — the Corp could not afford the 4 credits,
  so it was made to decline a card it could not see.
- It needs the 256-seed run: only 3 of 16 Corp decks field a Byte! (no
  sample deck fields a Snare!), and the leak needs an **R&D** access
  specifically, since from HQ or a remote the Corp already reads the card
  elsewhere in its own view. 32 seeds do not reach it. So
  `the_corp_is_named_the_rnd_card_it_is_asked_to_pay_for`
  (`rules::run::access`) pins the same thing deterministically, and
  `an_interactive_trigger_names_the_card_to_whoever_must_pay`
  (`rules::masking`) pins the rule per viewer.

**One test changed meaning.** `netrunner_bots`'
`search_reports_every_legal_action_even_when_the_sample_disagrees` built
its disagreement out of exactly this state — an off-Archives interactive
trigger the Corp could not identify — and asserted "off-Archives access
must stay masked from the Corp, or this test proves nothing". That state
no longer exists: the Corp having access actions now implies it can see
the card. The disagreement now comes from the Runner's grip and stack,
which are masked from the Corp in *every* position rather than only this
one, so the test is sturdier than it was.

**Verified.** `cargo test --workspace` green, clippy silent, and both
256-seed sweeps green.

## jinteki comparison: the engine gaps it shows — OPEN (19 September 2026; re-ordered by the second pass, 20 September 2026)

`docs/jinteki-comparison`, then `docs/jinteki-second-pass`. The finding is
[`docs/jinteki-comparison.md`](../jinteki-comparison.md) §2 and §6. These are open items for the next card set, not bugs:
every *System Gateway* and *Elevation* card resolves correctly without them.
Each would be one design decision, taken before a set builds on its absence.

**The list was six items on 19 September 2026 and is ten now.** A second
pass audited the first against both codebases: the six held, one was
understated and moved to the top, one was proposed too narrowly, and four
were missed. Nothing had cited the old numbers but this file and the doc, so
they are renumbered here once, with the old number on each item that had
one.

1. **One event queue, one audience rule, one checkpoint** (§6.1, absorbing
   §2.3; was item 3, and was only an ordering audit). Which cards hear an
   event is decided per event in `rules/dispatcher.rs` — `fire_direct`,
   `fire_runner_side`, `both_sides_candidates` — and an event an effect
   produces is returned, never dispatched (`ability::dispatch_damage_taken`
   is the side door for one of them). So a new card can need a Rust edit
   with no new mechanic in it, and `Trigger` has four variants for one
   successful run. The shape: a listener scan over every active card,
   `Trigger`s parameterised by a filter, effect-produced events through the
   same queue, and the CR 10.3 state-based checks (MU, unique, wins, empty
   remotes, expired durations) at one point after it. **First because it is
   the one that gets more expensive with every card**, and measured
   byte-for-byte on both seatings, because every trigger in the pool
   re-fires through the new path.
2. **A continuous-effect layer, with a target and a payload** (§2.1 as
   corrected by §6.2; was item 1). Not `{ kind, value: Amount, while }`: a
   closed enum with a payload per kind — a number, a subtype, a subroutine,
   a cost, a prohibition — plus `applies_to: CardFilter` and `while:
   EffectRequirement`, with a `GameState` list for lingering ones. Replaces
   `StrengthModifier`, the nine one-off `CardDefinition` fields (now
   including `host_ice_gains_subtypes`) and the three `*_strength_buff`
   fields. Prohibitions (§2.5) are its boolean kinds. It is also where **ICE
   gaining or losing subroutines** and **"cannot be broken"** would live,
   neither of which the engine models today.
3. **"The first time each turn" as a query** (§6.3; new). Roughly ten
   per-turn fields on `GameState`, an `EffectRequirement` apiece, where
   jinteki filters a turn log (`first-event?` alone is called 136 times).
   Candidate: constant-size per-turn and per-run counters keyed by an
   event-kind enum with a filter — not an event `Vec`, which every search
   clone would pay for.
4. **Generic prevention** (§2.2; was item 2). Give the existing
   `WindowCheckpoint::Prevention` window a kind parameter, so tags,
   end-the-run, jack-out and expose use the same window that damage and
   trash use, rather than a third special case.
5. **Where a payment comes from** (§6.4; new). Pools that compete and a
   player who chooses between them — stealth is the family that forces it.
   Today every pool is spent automatically in a fixed order.
6. **A numeric decision** (§6.4; new). `PendingDecision` has no "choose a
   number", so X costs and "pay up to N" have nowhere to park. One variant,
   and an `ActionSpace` segment appended at the end (the append-never-shift
   rule), so it is a retraining event to plan for rather than stumble into.
7. **A movement phase in the run** (§2.6; was item 4), before a card needs
   "when the Runner passes ICE".
8. **Cost types** (§2.4; was item 5). jinteki's 50 are the backlog, taken
   as cards need them.
9. **A scenario builder for card tests** (§4; was item 6). A deck-and-hand
   spec that reaches a real state through `setup` and actions, plus helpers
   that address cards by name. It is test code only.
10. **Concepts with no home yet, taken as cards need them** (§6.4; new):
    several run-replacement effects and the Runner's choice among them
    (`RunState::access_replacement` is one `Option`), reveal as an event,
    a set-aside zone (masking: needs an explicit who-may-see rule), expose,
    facedown Runner installs, the mark, charge, per-host card and MU
    limits, agenda points and advancement requirements that change while
    installed. **And one question to put to the Comprehensive Rules:**
    whether the Runner is entitled to the arrival order of facedown cards
    in Archives, which `PublicArchivedCard` positions plus an in-place flip
    at breach give them, and which jinteki shuffles away.

## Advancing a card and placing a counter on it were one event — DONE (20 September 2026)

`fix/placing-a-counter-is-not-advancing` (#85), from a report while
playing: *"some card for the Corp says 'Play and Advance two any card
that hasn't been advanced'… when I play it seems as if every card is
available to choose (including ICE) but that can't be correct because
the only cards that can 'Advance' are Agendas and those that explicitly
say 'can be advanced'."*

**The card is Seamless Launch, and the prompt is right.** Its printed
text is "Place 2 advancement counters on 1 installed card that you did
not install this turn" — no "you can advance" clause. Null Signal Games'
Comprehensive Rules:

> **1.18.2.** Resolving an instruction that directly places an
> advancement counter onto a card is not the same as advancing a card.
>
> **1.18.3.** The Corp can only advance certain installed cards. Agendas
> can always be advanced. If a card other than an agenda says that it
> "can be advanced" or that "you can advance" that card, the card can be
> advanced even while it is unrezzed.

1.18.3 restricts *advancing*, so it never reaches Seamless Launch, and
the rules' own worked example places counters on ice (§1.12.3a, Priority
Construction installing a piece of ice and placing counters on it). The
DSL already had the keyword — `CardFilter::Advanceable`, which is
`advancement_requirement.is_some()` — on exactly the six cards that print
the clause. Nothing pinned it in either direction, which is why the
prompt read as a bug; both directions now have a test.

**The finding, and a claim this entry corrects.** `Effect::
AddAdvancementTokens` emitted `GameEvent::CardAdvanced`, the same event
`PlayerAction::AdvanceCard` emits. The first reading of that was that
`Trigger::OnAdvance` fired on a placement and Weyland Consortium: Built
to Last was being paid 2 credits when Key Performance Indicators or
Syailendra merely placed one — **and that was wrong.** An effect's events
are *returned*, never put through `dispatcher::dispatch_event`, which
`engine::advance_card` calls by hand (`engine.rs:2071`), so the trigger
never saw a placement at all. Measured rather than assumed, both ways:

| | before → after |
|---|---|
| 192 random-vs-random games over `matchups()`, seed 1 | games, steps, end reasons, actions, cards, `triggers_fired` **all identical** |
| `CardAdvanced` | 514 → 425, the other 89 now `AdvancementCountersPlaced` |
| `built_to_last/OnAdvance` | 52 → 52 |
| 96 heuristic games on `brick_stack` (Built to Last + KPI + Syailendra), seed 3 | 66 counters placed; the identity's trigger 47/66/64/67 → unchanged on all four runner decks |

So this was a **trap, not a live bug**: the plumbing was the only thing
holding the two rules apart, and one `dispatch_event` added for another
card's sake would have turned it into a scoring bug with no test
watching. Split anyway, on that reasoning.

**Decisions taken, with the alternative rejected.**

- **A new `GameEvent`, not a new `Effect`.** All seven cards that touch
  advancement counters *place* them, so an advancing effect would have
  had nothing to do. Growth goes into the vocabulary of *when* something
  happens, which is the shape the DSL Growth Rule asks for, and the
  variant ratio is unmoved.
- **The effect is renamed `PlaceAdvancementCounters`**, so the card JSON
  says which of the two rules it means. `deny_unknown_fields` made the
  rename a parse error rather than a silent default in any file it
  missed — seven card files.
- **The dispatcher gets an explicit arm that fires nothing**, rather than
  falling through its catch-all, so the next person to write an "on
  advance" card finds the rule at the dispatch site instead of deriving
  it again.
- **`board::diff` had to name the new event explicitly**: its match ends
  in `_ => {}`, so a placed counter would have appeared on the board with
  no animation. The log says which of the two happened, because a reader
  wondering why Built to Last did not pay is reading that line to find
  out.
- **No blanket advanceability check inside the effect.** That would break
  Seamless Launch, which is the whole point above. Whether a target must
  be advanceable is the card's business, expressed as its prompt's
  filter.

**A real error one card over.** Sericulture Expansion prints "place 2
advancement counters on 1 installed card" with no clause and filtered
`Advanceable` anyway, so it refused a PAD Campaign or a plain piece of
ice — the opposite mistake, from the same confusion, found looking for
the reported one. Its filter is now `Any`; the source stays
`OwnInstalled`, because only a Corp install can hold an advancement
counter (`acting_corp_install_mut`).

**Still open, noticed here and not fixed:** `Trigger::OnAdvance` is
reachable *only* from the basic action. CR 1.18.1 says "card abilities
can also advance cards", and if one ever does, its trigger will not fire
until the advancing path dispatches its event. No card in the set
advances by ability today.
