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

1. **One event queue, one audience rule, one checkpoint — DONE (20 September
   2026, five PRs; the last stage's entry closes it)** (§6.1, absorbing
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

   **Taken up 20 September 2026, in five PRs, the first of which is the
   measurement** (`chore/coverage-identical-script`). Every stage but the
   last claims to move no game, and that claim had only ever been checked
   by hand — build, `--report`, md5 — which has gone wrong here three
   ways: a binary that was not pinned, a `--games` short of the pool, and
   one seating taken for all of them. `scripts/coverage_identical.py
   <base> [<head>]` pins each ref's binary by sha, plays one full pass of
   the pool (192 matchups) as random and as heuristic, by view and by
   `--index-path`, and reports `identical` or the sections and
   `triggers_fired` keys that moved. Checked three ways before anything leans on it: `main` against
   itself is identical four times (2m24s with a cold build, seconds once
   the binaries are kept); the commit before #85 against `main` differs
   in exactly the four numbers that entry recorded by hand (`CardAdvanced`
   514 → 425, 89 `AdvancementCountersPlaced`, the effect's rename, and no
   `triggers_fired` key); and an uncommitted docs-only checkout is
   identical to `main`. Found on the way: the two random reports share one
   md5 — a random bot plays the same game by view and by index — so the
   index path's own evidence is the heuristic pair, which do differ; and
   AGENTS.md said one pass of the pool was 132 games when it is 192. The stages after it: one
   audience rule (a listener scan, run in shadow against the hand-written
   audiences before it replaces them; `TriggeredEffect::subject`), one
   queue (emission is dispatch), one checkpoint (the unique rule's eight
   call sites and the win check's two), and last the `Trigger` variants
   that are one event and a filter — the only stage that cannot be
   byte-identical, since `triggers_fired` is keyed by variant, which is
   what `--expect-renames` is for.

   **The audience rule — DONE (20 September 2026, `feat/one-audience-rule`),
   and it corrects the plan above: this stage was never going to be
   byte-identical.** `rules::listeners` is the rule — *the subject of an
   event always hears it, wherever it is; every other card must be active;
   the active player's cards first* — and `dispatch_event` is now that plan
   fired a side at a time. `listeners::moments` is exhaustive over
   `GameEvent` (no `_` arm; the CR 1.18.2 arm moved there), the 574-line
   `match` and its six audience helpers are gone, and the special cases it
   carried fall out of the first clause: an unrezzed trap hears its own
   access, an operation its own play, a stolen agenda its own leaving.
   What the DSL could not say is now on the card: `TriggeredEffect::subject`,
   `This` or `Any`, on all 128 entries whose trigger is about a card or a
   server (`CardDefinition::validate` refuses a file without it, or with it
   where there is nothing to name), and `Trigger::hears` for "your".

   *Why not identical.* The old table fired one event's audience in up to
   four separate steps — the scored agenda, the identity, the rezzed table,
   the Runner's side — and a side was offered the order of its triggers
   only *within* a step. No rule reproduces that segmentation (`TurnStarted`
   put the identity and the installs in one step, `AgendaScored` in two), so
   keeping it meant keeping a per-event table, which is the thing being
   deleted. One plan per side is also what the rules say.

   *So the audience was proven separately, in shadow.* With both dispatchers
   compiled into a debug build, every `dispatch_event` computed the plan and
   compared it with what the hand-written arm asked, over both 256-seed
   sweeps (768 games each). **The two agreed on who hears every event, for
   every card in the pool, except where the old table was wrong:**

   | | view path | index path |
   |---|---|---|
   | Orbital Superiority, installed faceup under BANGUN, hearing a *different* agenda scored | **6** | 0 |
   | an installed faceup agenda asked `OnDiscardPhaseEnd` (Off the Books) | **133** | — |
   | cross-side order: `AgendaStolen` (the stolen agenda before the Runner's cards) | 367 | 198 |
   | cross-side order: `IceRezzed` (the rezzed card before Baz) | 301 | 40 |

   The first is a live bug that was on `main`: the rezzed table was asked
   `OnAgendaScored`, BANGUN installs agendas faceup, and "when you score
   **this** agenda" had no way to say *this* — 4 meat damage or a tag for
   scoring something else. The second never resolved anything (the agenda's
   requirement wants counters it only gets when scored) but is the same
   mistake, and the rule now says an agenda is active in a score area and
   not on the table. The last two are the active player's triggers not
   coming first during the Runner's turn. The shadow also found a *fixture*
   doing what the field exists to prevent: two rezzed copies of a test trap,
   no subject named, both hearing one access.

   *Two decisions the switch forced.* **`OnPlay` is a step of its own**,
   ahead of its side's triggers: it is how the DSL spells a card's
   resolution, not a triggered ability, and without that every Hedge Fund
   under Building a Better World was a question. **An order is offered only
   among triggers whose requirement passes now** (`ability::would_fire`, a
   read). `declares_trigger` counted every declaring card on the argument
   that a no-op choice was harmless; with a side's whole plan in one place
   it asked the Corp to order Hostile Takeover against a Malapert Data Vault
   in another server on every score. The plan is not thinned — an entry
   that does not count still fires in its turn and re-checks.
   `DeferredTrigger::heard` carries what a card heard the event *as*
   (subject, bystander, both), because a deferred entry fires against a
   state that has moved.

   *Measured* (`scripts/coverage_identical.py main --head-worktree`, 192
   games a report, seed 1; every game in all four reports reaches
   `GameOver`, the same 133 `triggers_fired` keys on the random seating):

   | | `TriggerOrderPending` | steps | Corp wins | triggers fired |
   |---|---|---|---|---|
   | random, by view and by index (one md5) | 466 → 473 | 69,254 → 68,714 | 92 → 94 | 5,779 → 5,738 |
   | heuristic by view | 263 → 277 | 83,562 → 82,813 | 37 → 39 | 4,518 → 4,462 |
   | heuristic by index | 263 → 277 | 83,283 → 82,534 | 36 → 38 | 4,503 → 4,447 |

   Seven to fourteen more order choices in 192 games is the net of two
   opposite moves — more offered across what used to be steps, fewer
   offered over a trigger that would not fire — and the outcomes sit
   inside the 0.026–0.047 seed-spread band, which is all a re-rolled
   trajectory can claim. Both 256-seed sweeps clean under the coverage
   gate; five card tests now say which simultaneous trigger they mean
   (`resolve_first`) instead of relying on the agenda's going first.

   *Still open from this stage:* `Trigger`'s per-variant doc comments still
   name the audiences they were first written for (the enum now says they
   are history, not limits); a Runner-side score area is not a listener,
   because no stolen agenda acts from there yet.

   **The event queue — DONE (20 September 2026, `feat/one-event-queue`), as
   a check rather than a rewrite.** The defect was never that events are
   returned as data; it was that whether a returned event is *also
   dispatched* was up to whoever wrote the line, with nothing to notice an
   omission. `dispatcher::audit` notices: in every debug build, when an
   action ends, every event in its record that `listeners::moments` calls an
   occurrence of something must have been through `dispatch_event`, or the
   action panics naming the event. Both agent-driven sweeps run it over
   every action of every game, so giving an existing event a moment makes
   each site that produces it undispatched a named failure that day — which
   is the guarantee item 1 asked of a queue. `dispatcher::emit` is the pair
   as one call, and replaces the 22 sites that were exactly
   `push(e.clone()); extend(dispatch_event(&e)?)`.

   *Rejected:* a sink that dispatches on push — the structural version of
   the same guarantee, at the cost of rewriting all 105 `evaluate_effect`
   call sites and every `Ok(vec![..])` an effect returns, to change nothing
   a card can observe. Also rejected: moving `DamageTaken`'s dispatch into
   `damage::apply_damage` (public API with no registry, and it reorders the
   record); `ability::dispatch_damage_taken` and the two approach filters in
   `run/engine.rs` stay, and stop being side doors, because the audit is now
   what holds them to their job.

   *What it found on its first run:* **an install made by a card's text was
   never heard.** `Effect::InstallFromZoneIgnoringCost` (Brân 1.0) returned
   its `CardInstalled` bare, so Haas-Bioroid: Engineering the Future missed
   it — and, its per-turn flag unspent, paid for the turn's *second* install
   as the first. Emitted now, with a test. The one legitimate deferral is
   written into the audit rather than allowed past it: an access is recorded
   when the card is presented and its `OnAccessed` fires when its choice is
   entered, which for Snare! is a later action, and the parked
   `AccessPhase::PendingInteractiveTrigger` is that debt on `GameState`.
   This also closes #85's handed-over hole in kind: `OnAdvance` is reachable
   only from the basic action, and the day a card advances by ability the
   audit says so.

   *Measured:* `scripts/coverage_identical.py main --head-worktree` —
   **identical four times** (the fix moves no game because no sample deck
   plays that identity; the md5s are the previous stage's). The audit clean
   over both 256-seed sweeps in a debug build (1,536 games).

   **The checkpoint — DONE (20 September 2026, `feat/one-checkpoint`), and
   it is two rules corrections, not a refactor.** `rules::checkpoint` runs
   the standing checks of CR 10.3 — a side at the agenda-point threshold has
   won; an older *active* copy of a unique card is trashed — from one place:
   at the top of `dispatch_event`, **before** a single trigger is planned,
   and again in `apply_action`'s tail after the drain, for whatever changed
   them without an event a card can hear. The ten hand calls are gone (the
   win check's two, the unique rule's eight).

   *The win was checked after the reactions.* A steal pushed the agenda,
   dispatched `AgendaStolen`, and only then looked at the score area — so a
   Runner who stole the winning agenda out of Jinteki: Personal Evolution
   with an empty grip was flatlined by the identity's reaction to a steal
   that had already won them the game. The checkpoint comes first now, and a
   finished game plans nothing.

   *The unique rule was an install rule, and it is a rule about active
   cards.* `trash_earlier_unique_copy` ran in eight install handlers (and
   not in a ninth, `Effect::InstallFromZoneIgnoringCost`), trashing the
   earlier copy whatever its state — so a second *facedown* Spin Doctor
   destroyed the first, which the rules leave alone: a facedown card is not
   active. Now the copy that most recently became active stays — the one a
   rez just turned faceup, otherwise the newest install — and the rest go;
   an agenda installed faceup (BANGUN) is no more active for this than for
   `listeners`. The Runner's side is unchanged in effect: a rig card is
   active from install.

   *Left where they were, each for a reason now written in the module:*
   deck-out and flatline (failed attempts, not standing conditions), the
   memory limit (it parks a decision, so it belongs at the end of the
   action), the console limit (a restriction on installing), empty remotes
   (derived), expired durations (backlog item 2's).

   *Measured* (`scripts/coverage_identical.py main --head-worktree`, 192
   games a report, seed 1, all to `GameOver`) — not identical, and the
   movement is the unique rule's: **Spin Doctor rezzed 120 → 151 and
   trashed 23 → 58** on the heuristic seating, because two facedown copies
   now coexist and each is rezzed for its draw in turn; Manegarm Skunkworks
   trashed 0 → 6 the same way.

   | | steps | Corp wins | `CardTrashed` | triggers fired |
   |---|---|---|---|---|
   | random (view = index) | 68,714 → 68,364 | 94 → 93 | 1,201 → 1,169 | 5,738 → 5,785 |
   | heuristic by view | 82,813 → 79,437 | 39 → 42 | 158 → 212 | 4,462 → 4,081 |
   | heuristic by index | 82,534 → 79,587 | 38 → 42 | 158 → 211 | 4,447 → 4,086 |

   Outcomes inside the 0.026–0.047 band; games about 4% shorter on the
   heuristic seating because the Corp draws more. Both 256-seed sweeps clean
   in release, and again in a debug build with `dispatcher::audit` on.
   **Triggers take a filter — DONE (20 September 2026,
   `feat/triggers-take-a-filter`), which closes item 1.** A trigger matched
   by equality alone, so every condition on the *event* had become a
   variant: `OnSuccessfulRunOnHq`, `OnSuccessfulRunOnRnD`,
   `OnSuccessfulRunOnCentralServer`, `OnTransactionPlayed`,
   `OnVirusInstalled` — five names for three triggers, one `RunSucceeded`
   an occurrence of up to three of them. `TriggeredEffect::when` is an
   `EventFilter` over what the moment is about — `Card(CardFilter)` or
   `Server([..])` — evaluated in `listeners`' scan, and the five variants
   are gone (`Trigger` 34 → 29; twelve entries in eleven card files, Nebula
   Talent Management's two flips merged into one "on HQ or R&D").

   *`when` is not `requirement`, and the printed sentence says which.*
   "Whenever you make a successful run on HQ" is the trigger condition: a
   run on R&D is not an occurrence of it, and nothing can make it one
   later. "…if you have not already this turn" is an intervening if, asked
   when the trigger resolves. The plan argued the split from
   `ChooseTriggerOrder` — a condition in `requirement` would add prompts —
   and **that argument was already stale**: since the audience stage an
   order is offered only over triggers `would_fire` admits. The reason that
   holds is the one above, and its visible half is that a refused card is
   no longer queued at all.

   *Two things the plan did not list, both the same idea.*
   `EffectRequirement::TriggeringCardMatches` **is deleted**: it was the
   filter said as an "if" (Barry "Baz" Wong's ice, The Zwicky Group's agenda
   or operation), so Baz was a queued listener on every rez of an asset.
   Both cards carry `when` now. And **the last audience rule in the
   dispatcher left it**: "an *installed* card hearing `OnVirusInstalled`
   acts on the virus; the identity does not" was Cookbook's text and
   Noise's, kept in Rust under a trigger's name. It is
   `TriggeredEffect::acts_on_subject` on Cookbook's file, applied per
   effect. `validate` refuses a filter or an "it" that does not fit what
   the trigger is about (`Trigger::about`, exhaustive; `names_a_subject` is
   now read off it).

   *One granularity given up, knowingly:* a queued trigger is per card and
   trigger, so a card with an unfiltered `OnSuccessfulRun` **and** an
   HQ-filtered one resolves them as one entry, in the order it lists them,
   where two variants were orderable apart. No card in the pool has both;
   one that does wants an index on `DeferredTrigger`. The inspector's
   "Engine reads it as" says the filter, since "on HQ" left the name.

   *Measured* (`scripts/coverage_identical.py main --head-worktree
   --expect-renames …`, twice — the collapse alone, then with the two
   requirement cards moved): **identical but for the expected renames, four
   reports of four, both times.** Random: nine `triggers_fired` keys become
   eight with equal counts (Leech 87, Cookbook 16, Docklands Pass 14, Nebula
   8 + 6 → 14, Devadatta Drone 7, Maglectric Rapid 5, Conduit 4, Détente 3;
   5,785 firings either side); Baz 34 and Zwicky 27 unmoved, 43 and 41 on
   the heuristic seating. `ActionSpace` unchanged at 1646.
2. **A continuous-effect layer, with a target and a payload — DONE (20
   September 2026, six PRs, #96–#101)** (§2.1 as corrected by §6.2; was
   item 1). Not `{ kind, value: Amount, while }`: a
   closed enum with a payload per kind — a number, a subtype, a subroutine,
   a cost, a prohibition — plus `applies_to: CardFilter` and `while:
   EffectRequirement`, with a `GameState` list for lingering ones. Replaces
   `StrengthModifier`, the nine one-off `CardDefinition` fields (now
   including `host_ice_gains_subtypes`) and the three `*_strength_buff`
   fields. Prohibitions (§2.5) are its boolean kinds. It is also where **ICE
   gaining or losing subroutines** and **"cannot be broken"** would live,
   neither of which the engine models today.

   **Taken up 20 September 2026, in six PRs, item 1's way**: the old path
   run in shadow beside the new over both 256-seed sweeps before it is
   deleted, `scripts/coverage_identical.py` at every stage, and a stage
   that is a rules correction says so and is measured apart from the
   refactor it rides on. The stages: the declared layer and every field
   that was already derived live (the first entry below); hand size, which is folded
   in once at install and never taken back; a `GameState` list of
   lingering effects for the three `*_strength_buff` fields; the view's
   strength, which has never included what the table adds; ICE strength,
   which is baked when a run's ice is built and goes stale when Syailendra
   places a counter on an Ice Wall mid-run; and the prohibitions.

   **The layer is declared — DONE (20 September 2026,
   `feat/continuous-effects-are-declared`).** `CardDefinition::continuous`
   is a list of `ContinuousEffect { kind, applies_to, while, text }`
   (`dsl::continuous`), and `rules::continuous` is the one scan that reads
   it: for a question about a target, walk the active cards and ask each
   effect whether it is the kind asked about, whether the target is one it
   applies to, and whether it is on. Seven fields and two `StrengthModifier`
   variants went into it — `memory_bonus` (seven consoles),
   `install_cost_discount_if` (Carmen), `install_cost_discount_amount`
   (Principia), `first_install_discount` (Kate, DZMZ Optimizer),
   `ice_rez_cost_modifier` (Fransofia Ward), `root_asset_trash_cost_bonus`
   (Mahkota Langit Grid), `host_ice_gains_subtypes` (Chromatophores),
   `hosted_breaker_bonus` with its `HostedBreakerBonus` struct (GAMEDRAGON™
   Pro), `PerInstalledIcebreaker` (Echelon) and `PerFracterInHeap` (Rising
   Tide) — with the six board scans that read them, `InstallKind`, and
   `RunnerState::first_install_discount_used_this_turn`. Sixteen card
   files; `ActionSpace` unchanged at 1646.

   *`applies_to` is a `Scope`, not the `CardFilter` this item first wrote.*
   A filter variant is legal in every `PromptChooseCards` and every
   `EventFilter::Card`, where "the card I am hosted on" and "the root of my
   server" mean nothing. `Scope` carries the relation to the source —
   `This`, `Host`, `Controller`, `Installing(filter)`, `Ice`,
   `RootOfThisServer(filter)` — and wraps a filter where the sentence goes
   on to say what kind of card. A number is `{ per: i32, of: Amount }`:
   signed because a discount and a penalty are one kind, over an unsigned
   `Amount` because every other reader of one deals damage or draws cards.
   Only the kinds a pool card prints exist (`Strength`, `Memory`,
   `InstallCost`, `RezCost`, `TrashCost`, `GainSubtype`,
   `BoostsLastTheRun` — the last a fact with no payload, the kind the
   numbers-only proposal had no room for); additional subroutines, "cannot
   be broken" and the rest are named in the enum's doc and added when a
   card prints them. One new read word, `Amount::InHeapWithSubtype`, where
   a `StrengthModifier` variant used to be — not a general
   `CountInZone { zone, filter }`, because `Amount` is `Copy` and a
   `CardFilter` is not.

   *Who is asked is the Listener Rule's sentence again.* What a card says
   about itself (`Scope::This`) applies wherever it is — Carmen and
   Principia price themselves from the grip, where they are not active —
   and everything else needs an active source, plus the one `listeners`
   also reads off the run: a persistent upgrade trashed earlier in it.
   "Active" was written out in `listeners` and again in `checkpoint`, each
   with a comment pointing at the other; it is `rules::active` now, and all
   three read it. Nothing is cached: the scan runs at each question, the
   way `memory::memory_balance` always did, and `validate` refuses an
   effect that cannot reach anything (a `Strength` on a card that prints
   none, a `Host` on a card that is never hosted), because a misfit parses
   and then applies to nothing, which looks like a card that works. A
   continuous effect's `text` is in `printed_clauses_are_quoted_from_the_card`
   — nobody chooses one, but it is the only part of a card an erratum can
   change without a trigger or an ability failing there — and the
   inspector's "Engine reads it as" says them, which it never did for any
   of the fields: a console's "+1[mu]" was invisible to it.

   *In shadow first.* With both compiled into a debug build, each of the
   six old scans asserted the layer's answer equal to its own at every
   call, over both 256-seed sweeps (1,536 games) and the 964 unit tests:
   **no disagreement**. Then the old code was deleted and measured
   (`scripts/coverage_identical.py main --head-worktree`, 192 games a
   report, seed 1): **identical, four reports of four.**

   *Then two rules corrections the fields had been hiding, each measured
   on its own.* (1) **Mahkota Langit Grid taxed an asset accessed out of
   HQ.** "Each asset in the root of this server" was asked of the server
   being run, so with the grid rezzed in a central's root a PAD Campaign
   in hand cost 6 to trash. The layer asks about the install being
   accessed (`AccessState::pending_install`), and a card from a hidden zone
   has none. Heuristic reports identical (a heuristic Corp does not put the
   grid on a central); random: steps 68,364 → 68,212, end reasons unmoved
   at 93 / 85 / 14. (2) **Two DZMZ Optimizers are two abilities.** The field
   returned the first source it found and spent one flag for every source,
   so the second copy did nothing — in the nine sample decks that play two —
   and Kate's would have silenced a DZMZ's. Each is `while:
   OncePerTurn(..)` now, keyed by install like every other once-per-turn,
   and the install that uses a discount is what spends it
   (`continuous::pay_install_cost_of`). Random: steps 68,212 → 68,586, end
   reasons 93 / 85 / 14 → 94 / 84 / 14, inside the band; heuristic: 41 paths
   move by one game's worth (steps 79,437 → 79,440, end reasons unmoved),
   because a heuristic Runner rarely has the second copy out.

   *What the scan costs.* On the two pinned binaries, one pass of the pool:
   random 5.3 s → 5.5 s, heuristic 15.5 s → 16.0 s, about 3%. Most of it
   was `memory::refresh`, which asks after every action and walked both
   sides' tables; a question about a player reaches only that player's
   cards (`Scope::Controller`), so it walks one (the four reports' hashes
   unmoved by that change). Not memoised: the next thing to try, if a
   search profile ever names it, is one `apply_action`'s worth, never a
   field on the state a search clones.

   **Hand size is derived — DONE (20 September 2026,
   `feat/hand-size-is-derived`).** "You get +1 maximum hand size" was the
   one standing number still *stored*: `max_hand_size_bonus` on both
   players' states, folded in at three moments — `GameState::setup` for an
   identity (Haas-Bioroid: Precision Design), a hardware install (T400
   Memory Diamond, at both install sites), `Effect::GainMaxHandSize` from
   a score trigger (Superconducting Hub) — and never taken out. The field's
   own doc said why: agendas and identities contribute as well as the rig,
   "so summing the rig would not reproduce it". `rules::active` is exactly
   that sum, so the three cards print `ContinuousKind::HandSize` on
   `Scope::Controller` and `turn::max_hand_size` asks
   `continuous::hand_size` each time, the way the memory limit is asked.
   Out: both state fields, `CardDefinition::max_hand_size_bonus`, the setup
   and install folds, `Effect::GainMaxHandSize` (`Effect` 74 → 73) and
   `GameEvent::MaxHandSizeGained`, which nothing else emitted and no
   client put into words.

   *In shadow first, and this time the two were expected to disagree.*
   With both compiled into a debug build, every ask of the limit logged
   stored against derived over both 256-seed sweeps (1,536 games). On the
   real game state: **five games, all in the index sweep (seeds 41, 106,
   151, 218, 252), 60 end-of-turn checks between them, every one the
   Runner at stored 6 and derived 5** — a T400 the Corp had trashed
   (Retribution) still paying. No Corp disagreement on a real state: no
   sweep game forfeits a Hub. Every other disagreement, 379 asks, was the
   other way round and on a bot's sample: `determinize` builds its states
   with the stored bonus at zero, so every sample of a game with one of
   the three cards in play had the wrong limit (stored 5, derived 6 or 7).
   A derived limit needs nothing carried into a sample.

   *Measured* (`scripts/coverage_identical.py main --head-worktree`, 192
   games a report, seed 1): random identical in both shapes; the two
   heuristic reports differ **only** in the keys that no longer exist
   (`effects_seen/GainMaxHandSize` 3 → 0, `events/MaxHandSizeGained`
   3 → 0) — no step count, end reason or trigger count moved. So the
   correction is real and rare: one pass of the pool never trashes a T400
   with a sixth card in the grip at end of turn, and the heuristic does
   not search, so its samples' hand limit never reached a decision. It
   will reach PUCT's. An old `MatchHistory` replays differently only
   across one of those five-in-768 games; the header carries no engine
   version, which is Phase 7 §8 item 5's to decide.

   *A test that could not fail.* T400's "end-to-end proof" applied
   `EndTurn` and asserted the phase was not `Discard` — but `EndTurn`
   opens the end-of-turn window and leaves the phase where it was, so the
   assertion held at any hand limit. The three cards' tests close the
   window and read `DiscardPending` now, each from both sides of the
   line: installed and trashed, scored and forfeited, the identity and
   none. Precision Design's bonus had no test at all — it was applied in
   `setup`, which no `base_state()` test goes through.

   **Lingering effects — DONE (20 September 2026,
   `feat/lingering-effects`).** An effect with a duration is the other
   kind of standing effect: made once by something that resolved,
   outliving it, and so — by the State Hygiene Rule's test — stored.
   `GameState::lingering` is a list of `LingeringEffect { what, on, until,
   source }` (`rules::lingering`), each **resolved when it is made**: a
   flat `Strength(i32)` on an install handle, no `Amount` and no `Box`, so
   a search clone copies a few words and an empty list allocates (and
   serializes) nothing. `Effect::BoostStrength` and `ModifyStrength` push
   one. Out: `encounter_strength_buff`, `run_strength_buff` and
   `turn_strength_buff` on every rig card, `effective_strength()`, the
   three `reset_*` functions and their five call sites. GAMEDRAGON™ Pro
   needs nothing new: it already lengthened the duration when the boost
   was made.

   *Whether an effect still holds is asked of the state at every read*
   (`LingeringEffect::holds`): an `EndOfEncounter(ice)` while that ice is
   the one being encountered, an `EndOfRun` while there is a run, an
   `EndOfTurn(n)` while `GameState::turn` is `n`. The plan was a derived
   retain in `checkpoint::expire_durations` replacing the reset calls;
   that is there, but as garbage collection — a list whose correctness
   depends on the sweep running has the bug T8 was (the one way of ending
   a run that forgot to reset), so the read filters and the sweep only
   tidies. The one thing a derived answer cannot tell apart is this run
   from the next, which is why `run::engine::end_run` sweeps as well.
   Only what the pool uses exists: one `Lingering` kind, `on` an install
   handle and nothing wider. The prohibitions and Tread Lightly's rez
   cost (the last stage) are about a player and a server and widen it
   then.

   *Leech's -1 lasted the run.* "The ice you are encountering gets -1
   strength for the remainder of this encounter" was written into
   `RunIce::current_strength`, where nothing could take it back out.
   It is an `EndOfEncounter` entry on the ice now, and
   `lingering::ice_strength` — the break contest, the view, the bots'
   pricing — is what the ice was built with plus what is lingering on
   it. No card in the pool brings the Runner back to a piece of ice it
   has passed (the one move that rebuilds the run's ice rebuilt the -1
   away too), so no break contest changes; what changes is the strength
   a view shows for ice already passed.

   *In shadow first.* With the three fields and their resets still
   compiled in, every read of a rig card's strength — each break contest
   and each masked view — compared stored with derived over both
   256-seed sweeps in a debug build (1,536 games): **no disagreement.**
   With the old code deleted (`scripts/coverage_identical.py main
   --head-worktree`, 192 games a report, seed 1): **identical, four
   reports of four**, across 288 pumps and 30 Leech activations in the
   random reports and 441 pumps in the heuristic ones. `determinize` is
   unchanged in effect: a sample still folds the displayed strength into
   `base_strength` and starts with an empty list, so a pump in flight
   never expires inside a sample, exactly as before — carrying the list
   in the view is the next stage's. `eval` reads a sample's strengths
   through `rules::lingering`, because a pump bought *inside* a search is
   a lingering effect of the sample.

   **The view shows the strength the engine uses — DONE (20 September
   2026, `feat/the-view-shows-the-strength-the-engine-uses`).** A rig
   card's `current_strength` in every `ClientView` was the stored half
   alone — printed plus the boosts paid for — and left out what the table
   adds: Echelon's +1 for each icebreaker, Rising Tide's fracters in the
   heap, a GAMEDRAGON™ Pro's +1 to its host. So the number a person read,
   the number the observation encoded and the number `eval::break_cost`
   priced a pump against could all be short of the one the break contest
   used. The reason on the masking function — that a `CardRegistry` would
   ripple into every consumer crate — was wrong: `view::build_client_view`
   is the one production caller and always held one.
   `mask_state_for_player` takes the registry and shows
   `continuous::breaker_strength`.

   *Why it was a stage of its own.* `determinize` took the shown number
   as the sample's `base_strength`. With the view corrected, that alone
   would count Echelon twice; and since stage 3 it had also been turning
   every pump in flight into printed strength that never expired inside a
   rollout. So the view now carries the lingering effects that hold
   (`PublicGameState::lingering`, `ClientView::lingering` — public: a
   flat number on an install both players see, made by a card both
   watched resolve), and a sample takes a shown number apart again: the
   printed part from the registry the way `engine::seed_rig_card` seeds
   it, the boosts from the view's list, the table's part derived from its
   own rig. A visible ice's stored strength is the shown number with the
   lingering part taken back out (stage 5 deletes that field). **A debug
   build asserts that every sample agrees with its view about every rig
   card's and every visible ice's strength** (`determinize`'s
   `debug_assert_strengths_agree`) — that is every test and both sweeps,
   and on its first run it found a search fixture whose rig card
   disagreed with its own registered card (its twin in `mcts` was
   corrected with it). `eval` prices a break with
   `continuous::breaker_strength` and tests a subtype the way
   `BreakSubroutines` does (`continuous::ice_gains_subtype`), so
   Chromatophores' host is something the evaluator can break.
   `netrunner_client::board::breaks` loses one of its three reasons for
   playing a route rather than pricing it, and keeps the other two.

   *Measured.* In shadow, old number beside new at every masked rig card
   over both 256-seed sweeps in a debug build (1,536 games): **80,082
   maskings of a rig card showed a strength short of the engine's** — by 1
   (17,116 of the view sweep's 28,202), 2, 3 or 4 — Echelon in 75,943 of
   them, Rising Tide in 2,150, and five other breakers when a GAMEDRAGON™
   Pro was hosted on them; the sample-agrees-with-view assertion never
   fired. `scripts/coverage_identical.py main --head-worktree`, 192 games
   a report, seed 1: **random identical, both shapes** (the engine did not
   move — only what is shown); **heuristic moves, as it should, because
   the bot reads the view.** By view: pumps bought 441 → 414 while
   subroutines broken went 952 → 965 over the same encounters (1,261 →
   1,260) and ice passed 1,408 → 1,427 — the Runner stopped paying for
   strength it already had. By index the same shape: 441 → 415, 959 →
   978, 1,414 → 1,440. Outcomes do not move: end reasons 32 / 10 / 150 →
   30 / 11 / 151, a Runner win rate of 0.781 → 0.786, which is a fifth of
   the seed-spread band's lower edge (0.026) and is claimed as nothing.
   The observation's *values* drift for the same cards (its shape does
   not: `OBS_SIZE` 2262, `ActionSpace::SIZE` 1646); no promoted policy
   exists to retrain. Owed, in order: ICE strength derived, prohibitions.

   **ICE strength is derived — DONE (20 September 2026,
   `feat/ice-strength-is-derived`).** A piece of ice's strength was a
   number on the run's copy of it, `RunIce::current_strength`, baked by
   `build_run_ice` from the four `StrengthModifier` variants left after
   stage 1 (Palisade, Pharos, Ice Wall, Scatter Field) and kept for the
   run by `reconcile_ice`, whose "same install, same card" arm clones the
   existing entry. The comment on the bake said its conditions are fixed
   for the run — "advancement cannot change mid-run", "nothing in the
   pool installs ice mid-run". Syailendra's subroutine places an
   advancement counter mid-run, on an Ice Wall or a Pharos behind it; and
   Scatter Field's own first subroutine installs a Barrier, which the Corp
   may put on Scatter Field's server. And `IceEncountered { strength }`
   read the stored number raw, without what was lingering: a third
   reading beside the contest's and the view's.

   `continuous::ice_strength(state, registry, ice)` is the one question
   now, the twin of `breaker_strength`: what the card prints, what the
   table adds (its own text first, rezzed or not, as wherever a card
   speaks about itself), what is lingering. The break contest,
   `IceEncountered`, `IceStrengthModified`, the view and `eval`'s pricing
   all put it. `StrengthModifier`, `CardDefinition::strength_modifier`,
   `RunIce::current_strength` and `lingering::ice_strength` are gone, and
   `determinize` no longer takes a shown ice strength apart — it carries
   none, and `debug_assert_strengths_agree` now proves a sample has the
   counters and the neighbours the view's number was made from. The wire
   field `PublicRunIceIdentity::current_strength` keeps its name and is
   computed in masking, so no client and no observation code changed
   (`OBS_SIZE` 2262, `ActionSpace::SIZE` 1646).

   *The vocabulary it cost: two reads, no `Effect`.* Ice Wall is
   `Strength { per: 1, of: HostedAdvancementTokens }` and Pharos is
   `Strength { per: 5 }` `while AmountAtLeast(HostedAdvancementTokens, 3)`
   — both already sayable, the second in the words Syailendra's own
   trigger uses. Palisade needed `EffectRequirement::ProtectingRemote`
   (no requirement read where the acting card is installed, and "a
   remote" is every server that is not one of three, which a list of ids
   cannot say). Scatter Field needed `Amount::IceProtectingThisServer`,
   used as `Not(AmountAtLeast(.., 2))`: a count rather than an "only ice"
   requirement, because "for each piece of ice protecting this server" is
   the sentence the next card prints. `validate` refuses
   `ProtectingRemote` on a card that is not ice. *Rejected:* re-baking the
   number in `reconcile_ice`. It would have fixed Syailendra and been a
   third copy of a number whose other two copies are asked, correct only
   at the steps that happen to reconcile.

   *Fixtures.* Thirty-odd `RunIce` literals across five crates lost a
   field, and the helpers that took a `strength` lost the parameter: a
   fixture whose contest turns on a number registers a card that prints
   it (`rules::test_support::ice_printing`), which four engine tests and
   the evaluator's run-term tests needed and the rest never had.

   *Measured.* In shadow, stored-plus-lingering beside derived at every
   read over both 256-seed sweeps in a debug build (1,536 games): **two
   reads disagreed, both in one game** (view sweep, seed 243, turn 24) —
   an unrezzed Scatter Field shown to the Corp at 4 after a second ice
   had joined its server mid-run, where the derived answer is 0. **No
   break contest and no `IceEncountered` ever disagreed**: the
   Syailendra path is real (the new card test walks it for Ice Wall, 1 →
   2, and Pharos, 5 → 10) and no sample deck's play reached it. With the
   old path deleted, `scripts/coverage_identical.py main
   --head-worktree`, 192 games a report, seed 1: **identical, four
   reports of four.** So this stage is a correction the pool can reach
   and the sample decks, at this depth, do not. *Replays:* a recorded
   `MatchHistory` re-simulates, and one in which a stale strength decided
   a break would diverge from here on; none in the sweeps would.

   **Prohibitions and Tread Lightly's rez cost are lingering effects — DONE
   (20 September 2026, `feat/prohibitions-are-continuous-effects`). This
   closes the item.** Three "for the remainder of…" effects were still a
   field each, with a reset each: `CorpState::cannot_score_agendas_this_turn`
   (Luminal Transubstantiation, cleared when the Corp's *next* turn began),
   `RunState::runner_cannot_steal_or_trash` (Ansel 1.0) and
   `RunState::ice_rez_cost_modifier` (Tread Lightly). The first and the
   last were on fields no view carried, and `determinize` wrote `false`
   and `0` for them: no bot sample was ever bound by the score lock, and
   every sample of a Tread Lightly run priced the rez 3 short.

   They are entries on `GameState::lingering` now, which stage 3 built and
   stage 4 put in the view. `LingeringEffect::on` widened from an install
   handle to `On::{Install, EachIce, Player}` and `Lingering` from one
   kind to three (`Strength`, `RezCost`, `Cannot(Prohibition)`);
   `lingering::until` is the one place a card's duration becomes an
   `Until`. `continuous::rez_cost_delta` is table plus lingering, the twin
   of `ice_strength`, and **`continuous::cannot` is the one predicate** the
   guard in `apply_action` and `legal_actions` both ask — six reads of two
   flags before. The wire field `PublicRunState::runner_cannot_steal_or_trash`
   is gone too: `ClientView::cannot` reads the view's list, and the
   observation encoder fills the same slot from it (`OBS_SIZE` 2262,
   `ActionSpace::SIZE` 1646). Nothing has to end any of them: the turn
   code's clear and the two "naturally discarded with the `RunState`"
   comments went with the fields.

   *The vocabulary: one `Effect` fewer.* `PreventScoringForRemainderOfTurn`
   and `PreventStealAndTrashForRemainderOfRun`, single-use both, are
   `Effect::Prohibit { what: Prohibition, until: EffectDuration }`, used
   by both cards (`BoostDuration` renamed, since a boost is no longer the
   only thing with one; card JSON unchanged by the rename). `validate`
   refuses `Encounter`. Re-counted over the 178 card files: **26 of 72
   `Effect` variants single-use, 5 unused** (28 of 73 before; the 29 of 74
   in AGENTS.md predated stage 2's deletion of `GainMaxHandSize`). Tread
   Lightly keeps `PromptChooseServer.rez_cost_delta`, and
   `resolve_choose_server` makes the entry when the run starts.
   *Rejected:* `On::Server`, which the plan for this list had — the card
   says "each piece of ice", and a server named when the run began is the
   wrong one after `redirect_on_approach`. *Rejected:* a "make a lingering
   rez cost" `Effect` run from `on_start` — one card, one variant. *Not
   built:* a declared `ContinuousKind::Cannot`; every prohibition in the
   pool has a duration, so the kind is named on the enum as deferred and
   `continuous::cannot` is where its scan would join.

   *The rules bug the field hid: Tread Lightly taxed assets and upgrades.*
   "During that run, the rez cost of each piece of **ice** is increased by
   3[credit]." `engine::rez_price` prices every rez, and added the run's
   number whenever the card being rezzed was in the attacked server — so a
   Nico Campaign or a Manegarm Skunkworks rezzed in that server's root
   mid-run cost 3 more than it prints. The entry is about `EachIce`.

   *Measured.* Old beside new at all seven reads, both 256-seed sweeps in a
   debug build (1,536 games). **Steal-or-trash: no read disagreed.**
   **Rez price: 43 reads disagreed** — 32 on the authoritative state, all
   the bug above (Nico Campaign 6, Mahkota Langit Grid 4, AMAZE Amusements
   18, Manegarm Skunkworks 4, each at +3 and now +0), and 11 inside bot
   samples, ice priced +0 by a sample whose run had lost the modifier and
   +3 now (first taken for an entry outliving its run; the run in those
   lines has `initiated_by: None`, which only `determinize` builds, and
   `start_run` never once found an entry waiting). **Score lock: 5,422
   reads disagreed, every one on a Runner turn** — the flag stood until the
   Corp's next turn began and "the remainder of the turn" does not; the
   Corp scores on its own turn, so none could matter. With the old path
   deleted, `scripts/coverage_identical.py main --head-worktree`, 192
   games a report, seed 1: **all four reports identical but for the
   renamed key** — `effects_seen` `PreventStealAndTrash…` 12 → `Prohibit`
   12 (random), 20 + 2 → 22 (heuristic). The heuristic seatings did not
   move either: the sweep agents search too little under a lock or a Tread
   Lightly run for the repaired samples to change a choice; the search
   rungs are where it would show, and nothing here claims an effect
   there. *Stored states:* `on` serializes as an enum where it was a bare
   handle, so a saved `GameState` with a pump in flight no longer reads; a
   `MatchHistory` replays from its actions and is unaffected, except one
   in which an asset was rezzed at +3, which diverges from there.

   *Still open from the item* (since its first stage). "The first time each
   turn you install a program" is still "once a turn, when it applies": a DZMZ installed after
   the turn's first program discounts the second, as it did under the
   field. That is backlog item 3's query, and the `OncePerTurn` on these
   two cards is what it replaces. `netrunner_bots::determinize` still
   clears `once_per_turn_used` in every sample, as it cleared the flag.
   **Both closed by item 3** (its stages 2 and 4, below).
3. **"The first time each turn" as a query — DONE (20 September 2026, four
   PRs; the last stage's entry closes it)** (§6.3; new). Roughly ten per-turn
   fields on `GameState`, an `EffectRequirement` apiece, where jinteki
   filters a turn log (`first-event?` alone is called 136 times).
   Candidate, as the second pass wrote it: constant-size per-turn and
   per-run counters keyed by an event-kind enum with a filter — not an
   event `Vec`, which every search clone would pay for.

   **The shape, and what it was chosen over.** `rules::turn_log`:
   `GameState::this_turn`, a flat `Copy` table with a row per `Trigger` and
   a column per `Class` of thing the moment was about (a card's type, a
   server with the remotes as one, or whose moment it was), and
   `GameState::last_turn`, its row totals for the turn that ended most
   recently. The event-kind enum is `Trigger` and the classifier is
   `listeners::moments`, both of which existed: `turn_log::record` is one
   call at the top of `dispatcher::dispatch_event`, the door
   `dispatcher::audit` already holds every hearable event to, and
   `turn_log::rotate` is the one reset, at every turn start, both sides.
   *Rejected:* a list of named facts (`TurnFact::SuccessfulRunOnHq` is
   `Trigger::OnSuccessfulRunOnHq` coming back one enum over, the variant
   item 1's last stage deleted, and every new kind of occurrence would be
   a Rust edit for a card with no new mechanic in it); and the event
   `Vec`. **A class holds only what both players saw:** the Corp installs
   facedown and an advanced card is masked, so a count keyed by *that*
   card's type would tell the Runner an agenda went down —
   `turn_log::concealed`, exhaustive over `Trigger`, counts those as
   `Kind::Unseen`, which is what will let the log ride in a view whole.
   *Out of scope, deliberately:* `servers_run_this_turn` (Red Team and the
   evaluator need the remote's own number — which servers is a list, how
   many times is the log), `installed_this_turn`,
   `discarded_this_discard_phase`, `extra_clicks_next_turn`,
   `last_completed_run` and the `RunState` counters (no card in the pool
   prints "the first time each run"; it would be this struct on
   `RunState`).

   **The stages.** (1) the log, and the "this turn / last turn" flags
   onto it; (2) the view carries the log and each side's once-per-turn
   uses, so `determinize` stops zeroing them, and `OncePerTurn` loses its
   free-form tag; (3) "the first time each turn" becomes a word in the
   trigger *condition* beside `when`, judged in the scan on a count that
   already includes the occurrence, as a refactor for the cards whose
   source is active all turn; (4) the correction — a card that arrives after
   the turn's first occurrence missed it (DZMZ Optimizer, Docklands Pass,
   Détente, Verbal Plasticity, Cacophony, Phật Gioan Baotixita, Aggressive
   Trendsetting), which closes the item. Ryō "Phoenix" Ōno's "after a
   subroutine resolved during that run" is the one named deferral.

   **Stage 1 — DONE (20 September 2026),
   `feat/a-turn-is-counted-where-it-is-heard`.** Five fields went into
   the log — `made_successful_run_this_turn` and `…_last_turn`,
   `played_operation_this_turn`, `agenda_points_scored_this_turn`,
   `actions_taken_this_turn` — with their three write sites in three
   handlers, five reset and snapshot sites, and three requirements
   (`MadeSuccessfulRunThisTurn`, `PlayedOperationThisTurn`,
   `RunnerMadeSuccessfulRunLastTurn`), which are
   `AmountAtLeast(TimesThisTurn(trigger), 1)` and `TimesLastTurn` on the
   six cards that used them. `Amount` is `Copy`, so the two new amounts
   name a `Trigger` and no filter. `NoActionTakenThisTurn` and
   `AgendaPointsScoredThisTurn` stay and read the log: finishing an action
   is not a moment any card hears, and the points are a sum, taken off
   `AgendaScored` at the same door. The view keeps its shape, its two
   fields derived from the log, and `determinize` rebuilds a log from
   those two facts alone, so a sample is exactly as blind as it was;
   stage 2 is where that changes. *One deviation from the plan:* the
   token that proves a trigger is judged after its own occurrence was
   counted moves to stage 3, where it is first read.

   *Measured.* Old beside new at every read and at the two view fields,
   both 256-seed sweeps in a debug build (1,536 games). **The actions
   count, the operation flag, the agenda points and last turn's run: no
   read disagreed. No event was dispatched without a record to match** —
   `audit` checked "at least once" and a count needs "exactly once", so
   that check is now permanent. **"A successful run this turn": 12,470
   requirement reads and 173,786 view builds disagreed, every one on the
   Corp's turn** (8,868 + 123,797 in its action phase, the rest at its
   turn start, its discard and nine finished games) — the flag was reset
   only when the *Runner's* turn began, so it stood through the Corp turn
   that followed a run. The requirement reads there are Carmen's price
   being shown for a card the Runner cannot install on that turn; the view
   field is what `eval`'s run term read through a whole Corp turn. With
   the old path deleted, `scripts/coverage_identical.py main
   --head-worktree`, 192 games a report, seed 1: **identical, four reports
   of four** — the evaluator's stale term moved no heuristic game. *Stored
   states:* a saved `GameState` loses five fields and gains two that
   default; a `MatchHistory` replays from its actions and is unaffected.
   The pool fingerprint moves (six card files), so a corpus recorded
   before this is refused by the trainer.

   **Stage 2 — DONE (20 September 2026), `feat/the-view-carries-the-turn`.**
   `ClientView::this_turn` and `last_turn` are the state's, whole and the
   same to every viewer — they need no mask, because the log never
   counted what one player did not see — and each side's
   `once_per_turn_used` rides beside them. `determinize` copies all of
   it where it rebuilt a log from two facts and started every sample with
   every once-per-turn ability unspent. The view's two derived fields
   (`actions_taken_this_turn`, `made_successful_run_this_turn`) are gone;
   the observation reads the log into the same slot (`OBS_SIZE` 2262,
   `ActionSpace` 1646, both unmoved). **`OncePerTurn` lost its free-form
   tag**: the key is the card and which copy (`OncePerTurnKey { card,
   install }`), where every card file had spelled the tag as its own id
   and a shared string was a shared use. Naming the card is also what
   makes the set maskable: to anyone but the Corp an entry is kept only
   when it is about no install, a rezzed one or a scored agenda, since a
   use by a facedown install would name it. That is the one approximation
   a Runner's sample keeps. The sets are `BTreeSet`s, so a view carries
   them in an order that does not depend on a hasher. Twenty-one card
   files change spelling and nothing else.

   *Measured.* What a sample used to lose, counted in `determinize` over
   both 256-seed sweeps in a debug build: of 197,130 samples taken after
   the first turn began, **32,575 had a successful run last turn that the
   sample forgot (25,926 of them the Corp's, the seat that plays Public
   Trail and Measured Response), 3,563 an operation played this turn,
   5,521 agenda points scored this turn, and 29,034 a spent once-per-turn
   ability the sample believed unspent** (25,490 the Runner's, 4,857 the
   Corp's). `scripts/coverage_identical.py main --head-worktree`, 192
   games a report, seed 1: **random identical, both shapes — the engine
   did not move; heuristic differs, both shapes, as it should.** Runner
   wins 151 → 149 of 192 (0.786 → 0.776, inside the seed-spread band, so
   no strength claim), steps 78,706 → 76,454. The attributable deltas:
   Neurospike played 0 → 1 (in a sample it dealt 0, so it was never worth
   a click), Zahya Sadeghi's trigger 274 → 250 and Carmen's ability 154 →
   109 (samples that knew a once-per-turn was spent stopped planning
   around it). **Public Trail is still never played by the heuristic (0 →
   0)**: it is legal inside a Corp sample now, and the evaluator does not
   value a tag; that is the bot's, and nothing here claims otherwise.
   *Wire:* `ClientView` loses two fields and gains four, so a client and
   a server must be built from the same side of this commit.

   **Stage 3 — DONE (20 September 2026),
   `feat/the-first-time-is-part-of-the-trigger`.** "The first time each
   turn" is one word beside a trigger and its `when`
   (`TriggeredEffect::first_each_turn`), and what it counts is what the
   entry listens for — its trigger, the classes its `when` admits, its
   controller's moments where the trigger is phrased about its controller
   (`turn_log::Occurrences`) — so a card file never names the fact a second
   time. It is judged in the listener scan, like `when`: `turn_log::record`
   hands back the log as it stood with the event just counted (`AsOf`),
   `listeners::plan_for` cannot be called without one, and the verdict
   rides on the queued trigger (`DeferredTrigger::not_the_first_this_turn`)
   the way `heard` does, because by the time a queued entry fires the turn
   has counted more. So a trigger is never judged before its own
   occurrence is in the count nor after a nested event has added a second,
   and the two questions are on two types: a trigger asks `AsOf::is_first`
   (exactly one, itself) and a price asks `TurnLog::none_yet`, since an
   install is priced before it happens
   (`ContinuousEffect::first_each_turn`, legal only on `Installing`). No
   card file writes a 0 or a 1. A card's first-time entries share one
   count — one printed ability in as many entries as it has triggers.
   *Rejected:* `requirement: Not(AmountAtLeast(TimesThisTurn(..), 2))`,
   the Scatter Field idiom — asked at resolution rather than in the scan,
   and `validate` now refuses it on an entry's own trigger. **The log's
   key gained whose moment it was**, because a card's type stops saying
   whose card it is once the type is `Unseen`, and "the first time **you**
   install" is the Corp's installs on either player's turn.

   Eight cards are respelled, the ones whose source is active all turn —
   Engineering the Future, Gabriel Santiago, Reality Plus, Synapse Global,
   the Zwicky Group, René "Loup" Arcemont, Nebula Talent Management's flip
   side (keeps `IdentityFlipped`) and Kate (loses her `while`) — and
   `FirstInstallThisTurn`, `FirstSuccessfulHqRunThisTurn`, their two
   fields and two reset sites are gone. `validate` refuses a `when` finer
   than a class (a subtype, an ice type) or any filter where the card was
   concealed, `first_each_turn` with `OncePerTurn` or `Subject::This`, two
   first-time triggers one event is an occurrence of, and the word on a
   continuous effect outside `Installing`.

   *Measured.* Both spellings on the card and the old one deciding, both
   256-seed sweeps in a debug build (1,536 games), every firing of a
   first-time entry and every install *payment* compared: **the eight
   cards never disagreed.** A ninth did, and left the stage for it:
   **"Knickknack" O'Brian fired on a run that was not the turn's first,
   155 firings over six game-turns** — a resource, so it can arrive after
   the turn's first run, which is stage 4's correction and not this
   refactor. Engineering the Future's Runner-turn install (the flag was
   reset only when the Corp's turn began, so a Brân 1.0 install paid
   nothing) is real and unreached: no sample deck plays the identity; a
   test pins it. With the old spelling deleted,
   `scripts/coverage_identical.py main --head-worktree`, 192 games a
   report, seed 1: **identical, four reports of four.** *Wire and stored
   states:* the log's sparse cells gain a field, `DeferredTrigger` gains
   one that defaults; the pool fingerprint moves (eight card files).

   **Stage 4 — DONE (20 September 2026),
   `feat/a-card-that-arrives-late-missed-the-first-time`. The correction,
   and it closes the item.** The eight cards that can become active
   mid-turn say `first_each_turn` and lose their `OncePerTurn`: DZMZ
   Optimizer (on its `Installing` effect), Docklands Pass, Détente, Verbal
   Plasticity, "Knickknack" O'Brian, Cacophony and Phật Gioan Baotixita
   (both entries of each, one shared count) and Aggressive Trendsetting.
   While the first time was a once-per-turn *use of the card*, a copy that
   arrived after the turn's first occurrence still had its use, and so
   did one whose first trigger stood down. **`continuous::
   pay_install_cost_of` is gone** — the second scan at the real install
   that spent each discount — and all seven Runner install paths ask
   `install_cost_of`, the question a price shown already asked, so the
   two cannot disagree. `validate` now refuses a `OncePerTurn` on a
   continuous effect (nothing that reads one *uses* the card, so it would
   never be spent) and two once-per-turn abilities on one card (the key
   is the card and which copy).

   *Aggressive Trendsetting* prints "the first time the Runner trashes an
   **installed** Corp card", and "installed" had been an intervening if
   (`CurrentlyAccessingInstalledCard`). Left there, a trash out of HQ
   would have been the turn's first of what the entry counts. It is in
   the condition now: `EventFilter::InstalledCard(filter)`, read off
   `listeners::About::Card::installed`, which `CardTrashedFromAccess`
   states (`install`, new, public like the access) — kept apart from the
   moment's `install`, which stays `None` for a card with no handle left
   or `dispatcher::still_applies` would stand its trigger down. The log's
   card classes split by it. One `EventFilter` variant, single-use, with
   its reason on the variant. **Ryō "Phoenix" Ōno is the one named
   deferral**: "the first time each turn a run becomes successful *after
   a subroutine resolved during that run*" narrows the first time by the
   state of the run, which no class holds; it stays `And(OncePerTurn,
   SubroutineResolvedThisRun)`, exact for an identity with no "may".

   *Measured.* Both spellings on the card and the old one deciding, both
   256-seed sweeps in a debug build (1,536 games). The sweep's one-ply
   agents try actions on clones of the real state, so a disagreement
   prints many times; **the honest unit is the game-turn: DZMZ Optimizer
   discounted a program that was not the turn's first in 13 game-turns,
   Verbal Plasticity drew an extra card on a later draw in 12, "Knickknack"
   O'Brian offered its trash on a later run in 6, Docklands Pass granted an
   access on a later HQ breach in 2.** Détente, Cacophony, Phật Gioan
   Baotixita and Aggressive Trendsetting never disagreed — the last
   because a scored agenda cannot arrive on the Runner's turn, so its
   change is the spelling alone. With the old spelling deleted,
   `scripts/coverage_identical.py main --head-worktree`, 192 games a
   report, seed 1: **heuristic identical, both shapes** (in one pass the
   heuristic never installs one of these after the turn's first
   occurrence); **random differs, both shapes, as it must** — steps 68,586
   → 68,557, end reasons 94 / 84 / 14 → 95 / 83 / 14 (one game of 192),
   `knickknack_obrian/OnRunStart` 59 → 51, `ProgramInstalled` 407 → 404,
   `CreditsSpent` 4,561 → 4,540; 26 of 132 trigger keys moved, the rest
   trajectory drift after the first divergence.

   *The ledger (DSL Growth Rule), re-measured across the item:* `Effect`
   72 → 72; `EffectRequirement` 41 → 36 (five gone, three of them
   single-use); `Amount` 14 → 16 (`TimesThisTurn` four cards,
   `TimesLastTurn` two); `EventFilter` 2 → 3 (`InstalledCard`, one card).
   `first_each_turn` is on sixteen cards and `OncePerTurn` is left on the
   seven that print "Once per turn →" or are Ryō. Seven state fields, seven
   reset sites, one second scan and twenty-three free-form tag strings
   are gone; `GameState` gained two, both fixed-size.

4. **Generic prevention — DONE (20–21 September 2026, three PRs; the last
   stage's entry closes it)**
   (§2.2; was item 2). As the second pass wrote it: give the existing
   `WindowCheckpoint::Prevention` window a kind parameter, so tags,
   end-the-run, jack-out and expose use the same window that damage and
   trash use, rather than a third special case.

   **What reading it found first.** `Effect::PreventDamage` and
   `PreventTrash` are both on the DSL Growth Rule's unused list: **no card
   in the pool had ever opened the window, so neither sweep had.** Shred is
   the one pool card that prints "prevent", and it went round the window
   through a field on the run. So the item is three stages: the mechanism
   and what was wrong with it (this one), Shred onto it, then the Core
   Set's three interrupts — Decoy, Net Shield, Sacrificial Construct — with
   an Eternal-format deck pair so both sweeps reach them, since no Core
   card is Startup-legal and the sample decks are. Crash Space (payment
   sources, item 5) and Zaibatsu Loyalty (expose, item 10) are deferred by
   name.

   **Stage 1 — what is prevented is a word, and one door asks it
   (`feat/what-is-prevented-is-a-word`).** `rules::prevention`: the effect
   that deals damage, gives a tag or trashes an installed card calls
   `prevention::would`, which parks a `WouldHappen` in
   `GameState::pending_prevention`, asks, and makes what is left happen.
   What a card prevents is `dsl::Preventable` — `Damage { kind, up_to }`,
   `Tags(n)`, `Trash(filter)` — the payload of one `Effect::Prevent`;
   `PreventDamage`, `PreventTrash`, `PendingPreventionKind`,
   `PreventionKind`, two errors and four events (now `AboutToResolve` and
   `Prevented`, each carrying the `WouldHappen`) are gone, and
   `Trigger::OnTrashAboutToResolve` with them, which no card had ever
   declared (`Effect` 72 → 71, `Trigger` 29 → 28). *Rejected:* the "kind
   parameter on the window" the item asked for — the window never needed
   to know; what did was the parked thing and the word that matches it.
   **Five things were wrong with the mechanism nobody had reached**, each
   now a test that drives real actions against fixture cards:
   (1) *the window took the one window slot and never gave it back* —
   `paid_ability_window` is a single `Option`, so a prevention opened
   during a run's window replaced it, and the next window the flow opened
   (`open_window_if_at_checkpoint` after an access, the turn's own)
   replaced the prevention with the damage still parked; it nests now
   (`PendingPrevention::interrupted`, written by
   `paid_ability::open_window_for`), and the toggle for the action that
   opened it goes to the window that action was taken in.
   (2) *anything could be done in it* — an ability that dealt damage
   inside the window would have replaced the parked damage; only an
   interrupt (`Effect::prevents`) and a pass are legal now, by one guard in
   `apply_action`. (3) *it opened for a card nobody could use* — the gate
   was "some card in play prints a prevention", for either player, at any
   price; it is `prevention::could_prevent` (the word matches this, the
   requirement is met, the cost is affordable), and the window closes by
   itself once all of it is prevented or nobody can prevent more. The
   same ability counted as "usable" for the post-action window, which
   would have opened one after every action of a Decoy's owner's opponent.
   (4) *most trashes never reached it* — every card in the pool that
   trashes a Runner program does it through a selection (Ansel 1.0,
   Ballista, Biawak, Bumi 1.0, Retribution), which moved the card itself
   and said so in a comment ("parity is a follow-up"). (5) *tags had no
   door at all.* And one found on the way: **"the subroutines are not
   finished" was passed on by hand at five sites, to a parked decision and
   a parked paid choice and nothing else**, so a tag (or a trace) parked
   out of a subroutine's *choice* ended the encounter with the ice's later
   subroutines unfired — one function now,
   `pending_choice::mark_parked_resume_subroutines`, for all four parked
   states, and the test for it fails without the line. A parked trash
   names an install handle and no card, so it rides in a view and an event
   unmasked; the masking arm that dropped the old event for a facedown
   install is gone with the field it guarded. **A cost is not prevented**
   (`Cost::TakeTags`, a card trashed to pay, the Runner's paid trash on
   access, the memory-limit trash, a player choosing among their own
   installs), and "up to 3" prevents as much as is left — a number to
   choose is item 6. *Measured:* `scripts/coverage_identical.py main
   --head-worktree`, 192 games a report: **identical ×4** (random and
   heuristic, by view and by index) — as it must be, with no pool card
   able to open the window; that it *is* unreachable is the finding, and
   stage 3 is what ends it. `OBS_SIZE` 2262 and `ActionSpace` 1646
   unmoved (the two prevention slots now read amount and prevented for
   every kind). New: the **Prevention Rule** in `AGENTS.md`.

   **Stage 2 — a prevention that stands is a lingering effect
   (`feat/shred-is-a-lingering-prevention`).** Shred's "The first time the
   Corp would end that run, prevent the run from ending unless…" was
   `Effect::ArmRunEndPrevention` writing `RunState::end_run_prevention`: a
   per-duration field on the run, the third of its kind after the score
   lock and Tread Lightly's +3 (item 2 stage 6), and like them in no view —
   `determinize` wrote `end_run_prevention: None` under a comment that
   called it "a search-quality limit". It is an entry on
   `GameState::lingering` now (`Lingering::PreventRunEnding`, about the
   Corp, until the end of the run), which a view already carries whole and
   `determinize` already copies, and `prevention::run_ending` is what
   `Effect::EndTheRun` asks, first — the way jinteki puts its static
   preventions ahead of the ones a player chooses. Nobody *uses* it, so it
   has no window and is not an interrupt; "the first time" is the function
   taking the entry off the list when it is asked, paid or not, as
   `take()` on the field did. The effect keeps its name (it still arms a
   prevention for the run) and the enum its one clause; the field, its
   default and `determinize`'s `None` are gone. *Rejected:* routing it
   through `would` as a fourth `WouldHappen` — nothing is parked for a
   player to answer with an interrupt, and the "unless" is the Corp's paid
   choice, which `OfferPaidChoice` already is. *Shadow, both 256-seed
   sweeps in a debug build:* of **198,562** samples, **64** were taken with
   Shred armed — every one a heuristic Corp deciding mid-run (12 distinct
   situations, none on the index path), each of which had believed the next
   "End the run" would end it. Small, because the heuristic Runner rarely
   plays Shred; the same shape as Tread Lightly's 11. *Measured:*
   `coverage_identical.py main --head-worktree`, 192 games a report:
   **identical ×4** — the 64 samples are in the sweeps' 1,536 games, and
   one pass of the pool holds none, so this stage moves no measurement and
   claims none. Tests: the armed entry is in both players' views, survives
   determinization, is asked once (a second "End the run" ends the run),
   and an empty root prevents nothing and leaves nothing armed.

   **Stage 3 — the Core Set prevents, and both sweeps reach the window
   (`feat/the-core-set-prevents`); closes item 4.** Decoy ("[trash]:
   Prevent 1 tag"), Sacrificial Construct ("[trash]: Prevent a player from
   trashing 1 installed program or piece of hardware") and Net Shield ("The
   first time each turn you would suffer net damage, you may pay 1[credit]
   to prevent 1 net damage") — the first cards in the pool ever to open the
   prevention window, 22 Core Set cards implemented now. The first two are
   interrupts a player *uses* and needed nothing stage 1 had not built.
   **Net Shield is a triggered interrupt, and writing it found that a
   trigger on damage about to resolve could never have fired in play**: the
   announcement is dispatched with the damage already parked, and
   `dispatcher::fire_plan` queues a plan rather than fire it into a parked
   state, so the trigger would have resolved after the damage it was about —
   green in every test, because every test dispatched the event by hand.
   The cards that hear an announcement now resolve in `prevention::settle`,
   one at a time, before the players are asked. And **damage is always
   announced** (`GameEvent::AboutToResolve`, whether or not anybody
   listens), because "the first time each turn" has to count the net damage
   that came before the card was installed (the Turn History Rule); a tag
   or a trash about to happen is an occurrence of nothing and is announced
   only when the players will be asked. "**Net** damage" is a third thing a
   moment can be about — `TriggerAbout::Damage`, `EventFilter::Damage`,
   `turn_log::Class::Damage` — where it would otherwise have been an
   intervening if, under which a point of meat damage is the turn's first.
   **A list of effects is one sentence** (`ability::evaluate_sequence`):
   an `Effect::Sequence`, a trigger's `effects` and an access interaction's
   each had a loop of their own, one dropping the rest at a parked decision
   and one not stopping at all, so Snare!'s "give the Runner 1 tag and do 3
   net damage" would have dealt its damage underneath the window its tag
   opened — unannounced, and Net Shield never asked. (No pool card's list
   parks early in a way the old loops got wrong — Predictive Planogram's
   two `EffectIf`s are exclusive — which is why this is a refactor there.)
   *The decks.* No Core card is Startup-legal and every sample deck is, so
   `DeckCategory::Sweep`: lists built here to reach rules no published
   deck prints, Eternal-legal (`DeckCategory::format`), rotated by
   `sweep_decks_for_seed` beside the samples and yielded by `matchups()`
   never — nothing trains on one, and no measurement over "one pass of the
   pool" moves. *Safety Net* (Kate "Mac" McCaffrey, 45 cards, 10 influence:
   three of each interrupt in an ordinary Shaper rig) and *A Thousand Cuts*
   (Jinteki: Personal Evolution, 45 cards, 20 agenda points, 12 influence:
   Snare!, Urtica Cipher, Neurospike, Public Trail, Retribution, Ansel 1.0,
   Ballista), both on identities no sample deck uses so `determinize`'s
   guess at a sample list never lands on one, and neither listed on the
   new-game form. They put 11 of the 22 Core cards under an agent for the
   first time; the clause gate promptly asked Corroder and Gordian Blade
   for their printed clauses. **The observation vocabulary would have
   reindexed:** ranked as `core`, the three cards' `01xxx` numbers put them
   inside slots 77..=95 and moved every *Elevation* card three along under
   any trained policy — the *Elevation* bug from the other direction. They
   are a later wave of their set (`CORE_AFTER_ELEVATION`, slots 178..=180,
   pinned by a test); `OBS_SIZE` 2262 and `ActionSpace` 1646 unmoved.
   *Gate:* `EVENTS_RARE_WITH_SWEEP_DECKS` demands a `Prevented` in any
   batch of 512 games or more, so the window is held to having been *used*.
   *Measured, 192 games of the two sweep decks against each other, seed 1,
   `--format eternal`:* random-vs-random — 719 announcements, **66
   preventions**, Net Shield's trigger fired 142 times and was paid for 62,
   Decoy used 7 times and Sacrificial Construct twice, every game ended
   (Corp 156 by flatline; Runner 34 on points, 2 by deck-out).
   Heuristic-vs-heuristic — **0 preventions: the heuristic Runner installed
   none of the three in 192 games** and was flatlined in 85. That is a bot
   blindness, not an engine gap — the evaluator has no term for a card
   whose value is damage, tags or a program not lost — and it is recorded
   here as owed (Phase 5) rather than fixed in a rules PR; the random seats
   are what reach the window in both sweeps, and both 256-seed sweeps pass
   with the `Prevented` gate on. **One rules correction rode along and is
   measured apart: none of it is nothing.** `apply_damage` recorded 0
   damage as `DamageTaken { amount: 0 }` and dispatched it like any other —
   Urtica Cipher with no counters on it, a Neurospike after no score — so
   "whenever you do damage" heard it, and announced it would have been the
   turn's first net damage for a Net Shield to be asked about. Zero of
   anything is not an occurrence now (`prevention::would`).
   *`coverage_identical.py main --head-worktree`, 192 games a report, taken
   three ways because the first answer was not the engine's:* with the
   whole stage, random differs in two keys and nothing else —
   `events/AboutToResolve` 0 → 438 and `events/DamageTaken` 371 → 343, the
   28 zero-damage records, same games otherwise — while heuristic moves
   broadly (Runner wins 149 → 157 of 192, +0.042, inside Phase 3's
   0.026–0.047 band). With the three card files and the two decks *moved
   out of the build* and the engine left as it is, heuristic differs in the
   same two keys and nothing else (`AboutToResolve` 0 → 244, `DamageTaken`
   247 → 233), game for game. So the heuristic movement is attribution,
   not effect: `determinize` shuffles a registry-wide pool with the
   sampler's own rng, and three more cards in the registry re-roll every
   sample — exactly the drift the Testing Rule says a heuristic seating
   carries — and nothing about how the bots play changed. (A first try at
   the attribution set the cards `is_playable: false` and proved nothing:
   `register_playable_cards` registers every embedded card whatever the
   flag says.) Across the item: `Effect` 72 → 71 with two of its five
   unused variants now one that three cards use (26 of 71 single-use over
   181 card files, 3 unused), `Trigger` 29 → 28, `EventFilter` 3 → 4, one
   field of the run and four events gone, one error where there were two.
5. **Where a payment comes from — DONE (21 September 2026)**
   (§6.4; new). As the second pass wrote it: pools that compete and a
   player who chooses between them — stealth is the family that forces it;
   today every pool is spent automatically in a fixed order.

   **What reading it found first.** There is no stealth card in the
   catalog, and the fixed order was the smaller problem. Credits that are
   not the credit pool lived in six places — bad publicity's and a run
   event's on `RunState`, Making News's on `CorpState`, and three
   `hosted_credits_usable_for` purposes — and **each was spent by a
   different piece of code with its own affordability sum written beside
   it, and every such sum forgot a pool the payment then took.** So the
   item is four stages: one door for a payment (this one);
   "N[recurring-credit]" as a declaration refilled as a step of the turn
   rather than an `OnTurnStart` trigger a player can be asked to order;
   the payer's choice of pool, asked only when two pools are incomparable
   and answered by the existing `ResolvePendingChoice` (no `ActionSpace`
   growth — splitting one payment credit by credit waits on item 6); then
   the three Core cards whose pools compete — Cyberfeeder, The Toolbox and
   Crash Space, the last deferred here by name from item 4 — into the
   Eternal sweep decks.

   **Stage 1 — a payment has one door**
   (`feat/a-payment-has-one-door`). `rules::payment::sources` is the one
   scan: the site that pays says what the payment is *for*
   (`payment::Purpose`: an install, a rez, a trash cost, a trace, or
   `Other`), and every place whose credits may be spent on that is a
   source, the credit pool last. `available` and `pay` both read it, so
   whether a cost can be paid and paying it cannot disagree. What a card's
   hosted credits pay for is a word it prints — `dsl::PaysFor`, a list on
   `CardDefinition::pays_for`, matched against the purpose in one place
   (`payment::covers`) — where it was `hosted_credits_usable_for`, one
   `Option` drained by whichever handler knew the purpose. Open Market's
   "connection and job resources" is an ordinary `CardFilter` now
   (`Installing(filter)`), so Cyberfeeder's "virus programs" will be a card
   file. `ability::pay_cost` takes the registry and the purpose, which it
   deliberately did not across forty call sites; `validate` refuses a
   `pays_for` on a card that hosts no credits and a `trash_when_empty` no
   payment can empty. The order pools are spent in is unchanged.
   *Five sums that disagreed with the payment, each with a test:* (1) **a
   trash cost could not be paid with bad publicity or Overclock's
   credits** — `access::resolve_trash` summed the credit pool and Azimat,
   refused, and never reached the `pay_cost` that takes both; legal actions
   are probed, so the trash was never offered. (2) A Runner traced mid-run
   could not bid Overclock's credits (the bid range knew bad publicity
   only). (3) `ability::cost_is_affordable` — the question a paid-ability
   window and a prevention are opened on — forgot Overclock's credits.
   (4) Open Market paid for a click install and not for one made by a
   card's text. (5) An access trigger and a text install priced ahead of
   time counted the credit pool alone. The first two were confirmed
   failing on `main` at play level in a worktree of it.
   *Measured, one pass of the pool (192 games, seed 1) on a shadow build
   printing each moment an old sum would have refused what the scan
   allows:* random-vs-random — **2 game-turns, both a trash on an
   Overclock run** (old 1 and 0 credits, new 6 and 5, trash cost 2);
   heuristic-vs-heuristic — 0; `cost_is_affordable` never disagreed in
   play. Live and reachable, and rare: Overclock's credits were spent 13
   times in the random pass at all, and **two of the six pools are never
   reached by an agent**: Hostile Takeover is the only card that gives bad
   publicity and no deck plays it, and no deck is on NBN: Making News
   (`BadPublicityGiven` 0, `RecurringCreditsSpent` 0) — which is why the
   bad-publicity half of (1) is a scripted test and not a measurement, and
   is owed to the sweep decks in stage 4.
   *`coverage_identical.py main`, 192 games a report, pinned binaries:*
   heuristic **identical**, by view and by index; random **differs**, both
   shapes — and a game-by-game run of the two binaries (`--verbose`) says
   by how much: **191 of 192 games have the same length and outcome, and
   one diverges**, seed 68 (`fashion_lab` vs `professional_opportunities`),
   one of the two the shadow named — Corp by flatline in 605 steps on
   `main`, Runner on points in 294 once the trash on its Overclock run is
   offered. That one game is the whole aggregate movement: steps 68,557 →
   68,246 is its −311, and end reasons move by it alone (flatline 95 → 94,
   agenda threshold 83 → 84). The other shadow game-turn (seed 14) was
   offered the trash and the random seat's line is unchanged. Both
   256-seed sweeps pass, `cargo test --workspace` green, clippy silent.
   **Stage 2 — recurring credits are declared**
   (`feat/recurring-credits-are-declared`). "N[recurring-credit]" is
   `CardDefinition::recurring_credits` on any card — it was identity-only —
   hosted as the card's own counters, and what happens to it is two steps
   of the game rather than two triggers on the card. Comprehensive Rules
   v26.03, read from rules.nullsignal.games: 1.10.5a, "When this card
   becomes active, place N credits on it. Before abilities meet their
   trigger conditions for your turn beginning, if there are fewer than N
   credits on this card, place credits on it until there are N credits on
   it"; 1.10.5c, refilled "before other abilities that apply at the start
   of the turn resolve"; 1.10.5d, they do not accumulate. (The lettered
   substeps 5.6.1c / 5.7.1c that 1.10.5c names were not read — the page
   truncated — and are not relied on.)
   **Azimat and Mahkota Langit Grid did it as an `OnInstall`/`OnRez` and an
   `OnTurnStart` trigger resolving `Effect::RefillCountersTo`**, which is
   wrong twice by that text: the refill resolved *among* the turn-begins
   abilities it is meant to precede, and — because `dispatcher::
   offer_trigger_order` counts any trigger whose requirement passes, and a
   refill has none — **a player with one other turn-start ability was asked
   which to resolve first.** Confirmed on `main`, not read off the code: a
   Runner with Azimat and Open Market is parked at `StartOfTurn(Runner)` on
   a `ChooseTriggerOrder` of exactly those two, and the same test on this
   branch finds no decision and Azimat at 2. NBN: Making News did it a
   third way — `CorpState::recurring_credits` and `_max`, refilled by a
   line in the Corp's turn start, spendable because `pay_cost` asked
   whether a trace was active.
   `payment::refill` is the step of the turn (`turn`, ahead of the
   `TurnStarted` dispatch) and `payment::place_recurring` the step of
   becoming active: `engine::install_into_rig`, the one way a Runner card
   now reaches the rig — seven install handlers each seeded and pushed a
   card themselves — `engine::rez_install`, the one place a Corp card is
   turned faceup (every Corp install is constructed facedown; BANGUN's
   faceup install goes through it), and `setup` for the identity. The
   credits are placed *ahead* of the `*Installed` / `IceRezzed` dispatch,
   where a "when you install" ability finds them, and after a card's own
   rez cost, so a region's credits never pay for the region. Making News
   hosts its credits on the identity (`identity_counters`) with a word of
   its own, `PaysFor::TraceAttempts`. Gone: `Effect::RefillCountersTo`
   (**`Effect` 71 → 70; re-measured over 181 card files, 26 of 70
   single-use, 3 unused**), both `CorpState` fields, and
   `GameEvent::RecurringCreditsSpent` — an identity's credits leave as
   counters, like every other hosted pool's. `validate` refuses recurring
   credits with nowhere to go and on a Runner identity, which hosts
   nothing in this engine.
   *The view keeps `recurring_credits` and `recurring_credits_max`,
   derived* (`masking::mask_corp_state` takes the registry): the
   observation encoding has a slot for each and slots never shift, so
   `OBS_SIZE` and every slot's meaning are unmoved. One value does change:
   for a Making News Corp the adjacent `identity_counters` slot reads the
   hosted credits where it read 0. No deck is on that identity, so no sweep
   or corpus observation moves. `determinize` lost two lines — a sample
   carries the credits as the identity counter it already copied.
   **What the suite did not cover.** Deleting Making News's fields broke no
   test but two fixtures: the identity was named in no Rust file, so a
   green run said nothing about it. It has a play-level test now (refilled
   before the Corp's turn, not a source for a rez, first source for a
   trace, bids offered up to 5 + 2). The inspector reads the declaration
   out ("refill to 2 hosted credits", "may be spent to pay trash costs") —
   what hosted credits pay for had no words there before; Open Market's
   line is a raw dump of its `CardFilter`, accurate and ugly, left as it is.
   *`coverage_identical.py main`, 192 games a report, pinned binaries — all
   four differ, as they must: a removed decision shifts every decision
   after it.* What was expected was written down before the run and each
   red flag checked against the report JSON. **Order prompts: random
   `ChooseTriggerToResolve` 581 → 437 (−144, a quarter of them) and
   `TriggerOrderPending` 468 → 375; heuristic 261 → 188 and 220 → 168**,
   the same by view and by index. `triggers_fired` loses
   `azimat/{OnInstall, OnTurnStart}` (13, 125 — random only: **the
   heuristic Runner never installed Azimat in 192 games**, before or
   after, so its half of this stage is measured by the random seats alone;
   a bot blindness of the kind item 4 recorded, owed to Phase 5) and
   `mahkota_langit_grid/{OnRez, OnTurnStart}` (49, 274; heuristic 45,
   240) and `effects_seen/RefillCountersTo` its 461 (285), and **the
   refill still reaches the cards in play: `CountersAdded` 2,609 → 2,617
   random, 1,065 → 1,069 heuristic.** Every game ends, no new end reason.
   Outcomes drift as re-rolled games do — random flatline 94 → 99 and
   Runner on points 84 → 79, five games of 192, under one binomial
   standard deviation (√(192 · ¼) ≈ 6.9); heuristic Runner wins 157 → 156
   by view, 157 → 157 by index. *One flag tripped and was run down:*
   `poetri_luxury_brands_all_the_rage/OnAgendaScored` fired once in the
   random base and not at all in the head. It needs a random Corp to score
   on the one deck that plays the identity; the heuristic reports fire it
   11 times before and 11 after, in both shapes, and its sibling
   `OnAgendaStolen` went 27 → 33 in random. Drift in a count of one, not a
   trigger lost. Both 256-seed sweeps pass, `cargo test --workspace`
   green, clippy silent.
   **Stage 3 — the payer chooses the pool**
   (`feat/the-payer-chooses-the-pool`). `payment::plan` is a pure function
   over the sources. A pool has a class — what its credits may be spent on
   (`Breadth`: the words its card prints, or anything) and how long they
   last (`Life`: the run, the turn for recurring credits, or until spent) —
   and **the payer is asked only when the answer changes what they are
   left with**: a pool whose credits are never worth more than another's is
   spent before it unasked (every pool before the credit pool), two pools
   of one class go in table order, and a payment large enough to empty
   every pool that could go first has no order to choose. What is left is
   two pools neither the other's lesser and a payment too small for both —
   today, Azimat's (trash costs only, back next turn) against a run's
   (anything, gone with it), where the old fixed order drained Azimat on a
   run whose own credits were about to vanish. The answer is the existing
   `ResolvePendingChoice`, one option per class, capped at
   `MAX_PENDING_CHOICE_OPTIONS`: **no `ActionSpace` growth.** *Named
   simplification:* the chosen class is emptied as far as the payment goes;
   splitting one payment credit by credit needs a number for an answer,
   which is item 6.
   **Parked by replay, not by continuation.** A payment is owed from deep
   inside some thirty handlers. It unwinds the whole action with
   `RulesError::PaymentChoiceNeeded`; `engine::apply_action` — now a
   wrapper over `apply_action_once` — returns the *untouched* state with a
   `PendingPayment { action, answers, amount, options }` on it, and the
   answer applies the action again with the answers recorded
   (`GameState::payment_answers`, taken from the front by `payment::pay`).
   `apply_action` is a pure function of its inputs, so the second
   application reaches the same payment in the same state. Rejected:
   splitting every paying handler into pay-then-continue, and a payment
   field on every `PlayerAction` (an `ActionSpace` break). *What made it
   viable was checked before it was built:* in `rules/`, two sites handle
   an error without propagating it, and neither can eat a new variant —
   `EffectRequirement::Not` is inside the read-only requirement check, and
   `Effect::RezInstalled` matches only `NotEnoughCredits` and ends `other =>
   other` — which is why the signal is a variant of its own. A parked
   payment comes ahead of any decision parked beneath it (`current_actor`,
   the action list, both clients' prompt), because the parked action can
   itself be the answer to a card's choice.
   **Two things reading found that no test would have.** (1) *The planner
   could loop forever:* an answer naming a pool that exists but was not on
   offer took nothing and changed nothing. An answer must be one of the
   classes offered, or the question is put again. (2) **The log was a second
   channel.** A parked action has been submitted, not applied, so only its
   payer may see it (`masking::PublicPendingPayment { side, own }` — the
   first parked state that is not fully public, because it is parked
   *ahead* of the thing rather than by it). But the action is logged at the
   step it was submitted in, and `mask_action_for_player` shows a
   `PlayEvent` or a Runner install whole, on the ground that a faceup play
   is public — so the other seat's log would have named a card still in the
   grip while their view withheld it. `mask_logged_action_for_player` reads
   the step's own events (a park's are `PaymentChoiceOffered` and nothing
   else) and conceals the entry (`ConcealedAction::ChoosingPayment`), which
   keeps the log mask's contract of never reading the state. Not reachable
   with today's pool — a payment parks only mid-run, on actions that name
   public cards — and reachable the moment Cyberfeeder pays for an install
   from the grip. Both masks are tested on the serialised form, and each
   test was shown able to fail by breaking its mask on purpose.
   The view carries the payment, so `determinize` — whose `GameState`
   literal is exhaustive and would not compile until told — copies it from
   the payer's own view, the only one a decision is sampled from while one
   is parked. The words are `netrunner_client`'s, for both clients: "Pay 4
   credits", a button a pool named by its card ("Spend credits from Azimat
   first"), and for the other chair "The Runner is choosing which credits
   to spend". `Session` counts the question as the person still answering
   their own move, as it does any prompt — **untested, and not testable by
   play today**: a take-back is never free mid-run, and mid-run is the only
   place a payment parks until stage 4's cards.
   *Measured against what was written down first.* The premise was checked
   before building — two decks play both Azimat and Overclock, and the
   heuristic Runner never installs Azimat — and the prediction recorded
   before any run: heuristic reports identical, random ones differing by a
   handful of questions at most. `coverage_identical.py main`, 192 games a
   report, pinned binaries: **heuristic identical, by view and by index;
   random differs by one question in 192 games and by nothing else** —
   `PaymentChoiceOffered` 0 → 1, steps 67,597 → 67,598 (the park),
   `ResolvePendingChoice` 1,002 → 1,003 (the answer), and the random seat
   answered "the run first": `BonusRunCreditsSpent` 10 → 11,
   `CountersRemoved` 1,455 → 1,454, Azimat keeping what the fixed order
   took. End reasons and every other key unmoved. Both 256-seed sweeps
   pass; `cargo test --workspace` green, clippy silent. **So the mechanism
   is carried by its scripted tests, not by the sweeps:** one question in
   192 games is reach, not coverage, and the gate entry that holds the
   prompt to having been *used* belongs to stage 4, whose decks must make
   pools compete on every break. (The sweeps are release builds, where the
   `debug_assert` that a replay used every answer is compiled out; the
   debug-build suite is what exercises it.)
   **What it costs, measured because the PR had to admit it was not.**
   Parking needs a copy of the action made *before* it is applied — what
   asks is known only once the action has been moved into a handler — and
   the first version copied it on every application. On a byte-identical
   heuristic pass (192 games, seed 1, one report hash across every binary,
   three alternating rounds, pinned binaries): `main` 17.19–17.39 s, stage
   3 **+2.4% and +3.8%** in two sessions, the ranges never overlapping. A
   throwaway build with only the clone removed ran at 0.9998 of `main`:
   **the clone was the whole cost** — the planner, the classes and the two
   new `GameState` fields cost nothing measurable — because the
   legal-action probe applies every candidate and most fail at the first
   guard. `payment::could_ask` is an allocation-free *necessary* condition
   (a question needs two non-wallet pools of different classes, and every
   such pool is credits on the run, the Corp identity, a rezzed install or
   a rig card; fewer than two on either side and nothing can ask), and the
   copy is made only where it holds. It over-counts on purpose — a wrong
   yes costs a clone, a wrong no would lose the question, which a debug
   build refuses (forced to "no", the parking test fails on that
   assertion). With it: **+1.0%** (17.29–17.40 s against 17.15–17.22 s),
   still not overlapping — the scan itself, left there.
   **Stage 4 — the Core Set pays from pools, which closes the item**
   (`feat/the-core-set-pays-from-pools`). Cyberfeeder ("1[recurring-credit]
   … to use icebreakers or install virus programs"), The Toolbox (+2[mu],
   +2[link], 2[recurring-credit] for icebreakers) and Crash Space
   (2[recurring-credit] for removing tags, and a [trash] interrupt on meat
   damage — the card item 4 deferred here by name) are card files. They
   cost two words, `PaysFor::UsingIcebreakers` and `RemovingTags`, two
   purposes, `Purpose::Ability(def)` and `Purpose::RemoveTag`, and one
   `ContinuousKind::Link`. **A purpose is stated at every site that pays
   for a thing or asks ahead of time whether anybody could:**
   `Purpose::Ability` at `engine::activate_ability`,
   `paid_ability::has_usable_paid_ability` and
   `prevention::could_prevent`, because one of the three left at `Other` is
   an ability a console's credits could pay for and no window ever opens
   for. **Link is asked, never kept:** `continuous::link` is the identity's
   printed link plus what the rig declares; `RunnerState::link_strength`,
   written once at setup, is gone, the view derives its number, and
   `determinize` dropped its copy. (An earlier note in this work that The
   Toolbox's link would be "printed-only" was a guess, and wrong.) The
   three cards append to the observation vocabulary as a third wave (slots
   181..=183, pinned) rather than reindex every Elevation card; **seven
   free slots of 191 remain, so the next set forces a reshape.**
   *Two sweep decks make the pools compete:* Pay As You Go (Noise, three
   of each pool card) and Hostile Bid (Weyland: Building a Better World —
   Hostile Takeover is the pool's only source of bad publicity, Measured
   Response its meat damage), `DeckCategory::Sweep`, Eternal-legal,
   yielded by `matchups()` never.
   **What the deep sweep found, and the default bar and CI did not.** The
   256-seed view sweep failed at seed 173 with no legal action for the
   Runner — **a deadlock in stage 3, merged.** A handler pays and *then*
   does the thing, so a payment's question arrives before an error in the
   thing: Gordian Blade was asked which credits pay to break a barrier it
   cannot break; parked, the break was offered as legal, and every answer
   replayed into the error. 6 of 192 random games of the two decks
   stalled, and the headless CLI exited 0 on each — read `end_reasons`,
   never the exit code. **An action parks only if some sequence of answers
   completes it** (`engine::settle_payment`), else it fails with its real
   error, and an answer is legal on the same terms. Believed unreachable
   on `main`, where no ability payment can ask — argued, not proven. The
   re-measurement then found a seventh stall that was **not a payment at
   all** (seed 181): Noise's "the Corp trashes the top card of R&D"
   returned `EmptyZone` against an empty R&D, Botulus's own install trigger
   and Noise's parked a trigger order, and both choices replayed into the
   error. `MillRnDAmount` had always treated an empty R&D as nothing to
   trash; `TrashCard(TopOfStack)` does now. Noise had never been in a deck,
   so no agent had ever resolved its trigger — which is what a sweep deck
   on an unused identity is for. After it: **192 random games, 0 stalled**,
   and every game's line but seed 181's unchanged (a stall at 684 steps
   became a Runner deck-out win at 694).
   **`board::breaks` lost its routes wherever pools compete.** It plays
   routes on the engine, got the parked state back, had its next step
   refused and offered no route at all. Its purse counts a hosted credit
   as a credit, its search answers a question with the first option, and
   its driver waits for the person's answer. *Anything that applies an
   action and goes on must expect a parked state back.*
   *Measured against what was written down first* (`8967d5c`, pinned
   binaries, 192 games a report). **Random identical to `main`, by view
   and by index** (`c41129d8…`), as predicted: no sample deck holds one of
   the three cards and a random seat never consults the registry.
   Heuristic reports differ broadly (steps 80,023 → 82,579; Runner wins
   156 → 152 of 192, −0.021 and inside the band), predicted too, and
   **attributed rather than assumed:** a throwaway build of the same commit
   with the three card files and two decks moved out is **identical to
   `main`, four reports of four** (`8ae3c774…`, `45edb505…`) — the
   movement is the registry growing under `determinize`'s registry-wide
   pool, as it was for item 4's cards, and none of it is the engine: not
   the parking rule, not the derived link, not the new purposes.
   Over 192 random games of the two decks: Cyberfeeder installed 98 times,
   Crash Space 95 (activated 4 times), The Toolbox 23;
   `BadPublicityCreditsSpent` 127 — a pool no agent had reached before —
   and `BonusRunCreditsSpent` 8; Hostile Takeover scored 31, Measured
   Response played 19, Noise's trigger fired 289 times;
   **`PaymentChoiceOffered` 41**, against 1 in 192 games of the sample
   pool. The prediction that the fix would lower that count from 16 was
   wrong — a game that stalled had stopped asking. **But in the sweeps'
   own rotation the question is about 1 in 768 games**, so it is on
   `EVENTS_RARE_WITH_SWEEP_DECKS` at 4,096 games and The Toolbox (a 9[c]
   console only a random seat installs) on `CARDS_RARE_WITH_SWEEP_DECKS`
   at 2,048: reach, still not coverage, and recorded as that. Both
   256-seed sweeps pass; `cargo test --workspace` green, clippy silent.
   **Owed elsewhere.** The heuristic Runner installs none of the three
   cards, nor Azimat, and never plays Overclock in these decks (0 of 192):
   the evaluator has no term for a pool card (Phase 5). **No card in the
   pool starts a trace**, so NBN: Making News's credits and The Toolbox's
   link are reached by scripted tests and by no agent; a Making News sweep
   deck would reach nothing and was not built. Splitting one payment
   credit by credit is item 6. Across the item: `Effect` 71 → 70 (26 of
   70 single-use, 3 unused, 184 card files), `PaysFor` six words of which
   four are single-use, one `ContinuousKind` (single-use), two `CorpState`
   fields, one `RunnerState` field, one event and five affordability sums
   gone.
6. **A numeric decision — DONE (21 September 2026)** (§6.4; new).
   `PendingDecision` has no "choose a number", so X costs and "pay up to
   N" have nowhere to park. One variant, and an `ActionSpace` segment
   appended at the end (the append-never-shift rule), so it is a
   retraining event to plan for rather than stumble into.

   **What reading it found first.** The pool already prints chosen
   numbers, and every card that does had been written round the gap by
   taking the most its text allows: Bigger Picture's "remove **any number
   of** tags" removed all of them (`RemoveTags 99`), Account Siphon's
   "lose **up to 5**[credit]" was always 5, Lie Low's "remove **up to 2**
   tags" always 2. For two of them the most is nearly always right. For
   Bigger Picture it gave away the decision the card is — how tagged to
   leave the Runner, with a second copy in hand playable only against a
   tagged one. Phật Gioan Baotixita says its "up to 2" as two nested paid
   choices, which is correct and stays (one prompt would need `1 +` the
   number, which `Amount` cannot say); Crash Space's "prevent up to 3" is
   never worth less than the most; IP Enforcement's X is fixed by the
   agenda chosen. **No pool card prints an X cost** — Psychographics and
   Corporate Troubleshooter are in the catalog and not the pool — so none
   is built (the DSL Growth Rule). Two stages: the decision and the three
   cards (this one), then the payment split item 5 left as a named
   simplification.

   **Stage 1 — a number is a decision**
   (`feat/a-number-is-a-decision`). `Effect::ChooseNumber { chooser, min,
   max, of, then, text }` parks `PendingDecision::ChooseNumber` and is
   answered by `PlayerAction::ChooseNumber { amount }`, every number in
   the range a legal action exactly as a trace offers every bid — so both
   clients, the bots and the masks took it as they take a bid, and the
   prompt is the card's printed clause over a row of numbers. `max` is an
   `Amount`, resolved and capped (`MAX_CHOSEN_NUMBER`, 30, a trace's cap)
   when the decision parks, and `of` caps it again — "up to 2" *of* the
   tags there are — so a number that could do nothing is never offered,
   and **a range of one number asks nobody.** Its own action rather than
   `ResolvePendingChoice` read as a number: that segment means "the nth
   thing the card lists", and a policy should not have to learn that slot
   3 is sometimes the number 3. **`ActionSpace` 1646 → 1677, appended:**
   31 slots after what was the last segment, pinned by a test that the
   segment starts at 1646, so every recorded index means what it meant.
   `OBS_SIZE` is unmoved at 2262 **on purpose**: a parked-kind flag for
   the new decision would shift every feature after it, and what it
   offers is already in the mask as a contiguous run of the new segment —
   to revisit with the next deliberate reshape, which the seven free
   vocabulary slots already force. `Effect` 70 → 71 (26 single-use, 3
   unused, 184 card files), `Amount` 16 → 17; `RemoveTags` takes an
   `Amount` where it took a number.
   **The number is written into the effect that waits.** Inside `then` a
   card file writes `Amount::ChosenNumber`, a placeholder, and
   `Effect::with_chosen_number` writes `Fixed(n)` over it when the number
   is chosen — the convention `PromptChooseServer::on_success` follows for
   its server. Rejected: a field on `ResolutionContext`, where the rest of
   the per-resolution scratch lives and which does not survive a park.
   That is not theoretical in the pool: Bigger Picture's "the Runner loses
   5[credit] for each tag removed this way" comes *after* the tag removal,
   which Synapse Global: Faster than Thought answers with an install
   prompt of its own — a sample deck (*Gimbatul*) plays exactly that pair,
   and the test of it would have had the Runner lose nothing. A
   continuation is an `Effect`, so a number written into it rides through
   any park with no field anywhere. The substitution ends in `other =>
   other`, so it is held to the pool rather than to the match:
   `a_chosen_number_reaches_every_amount_a_card_writes` requires every
   card's `then` to come out naming no placeholder, and `validate` refuses
   one written outside a `then`, where it parses and is 0.
   **What the first card test found.** Account Siphon's "you may …
   instead of breaching" resolved with no acting card, and
   `evaluate_sequence` pins a continuation to a card — so the moment its
   effect could park, the `EndTheRun` the engine puts behind it was
   dropped, and the run stood open after the siphon. Latent since the
   continuation was added, for any replacement that parks; reached by
   making this one ask. `RunState::access_replacement_card` remembers the
   card that set the replacement, as `on_success_card` does for a run's
   rider (and its credit gain is now attributed to a card ability, which
   it is). *Found while reading and not fixed:* no view carries a pending
   access replacement, so a bot sample taken mid-run on an Account Siphon
   has none. Siphon is in no deck; owed when one plays it.
   *Measured against what was written down first* (`507d6e0` against
   `main` `47d62c8`, pinned binaries, 192 games a report). Predicted:
   heuristic identical — no card file added, and the heuristic plays
   neither card; random differing in a handful of games, view and index
   still equal to each other. **Heuristic identical, by view and by index;
   random differs in exactly 3 games of 192** (seeds 73–75, all *Fine
   Print*'s Bigger Picture), `ChooseNumber` 0 → 3, steps 67,598 → 67,480,
   end-reason totals unmoved at 99 / 79 / 14, and the two random reports
   hash alike. **The predicted risk also landed: 3 in 192 is reach, not
   coverage.** Lie Low is played 15 times in that pass and almost never
   with a tag to remove, so it asks nothing; the default 32-seed sweeps
   never apply the action, and at 256 seeds the view sweep reached it and
   the index sweep did not (the index *path* does — the `--index-path`
   report counts the same 3). It is on `ACTIONS_RARE_WITH_SAMPLE_DECKS` at
   2,048 games with that reason, and the mechanism is held by volume
   instead: **101 numbers chosen over 576 random games** of the three deck
   pairs that hold both cards (*Gimbatul* / *Dashing Mad* 17, *Fine Print*
   / *Tickets Please* 54, *Not So Subtle* / *Professional Opportunities*
   30; Synapse Global's trigger 408 times in the first), none stalled.
   Both 256-seed sweeps pass; `cargo test --workspace` green, clippy
   silent. *Not looked at:* the desktop pop-up with a long row of numbers
   — it is the trace bid's row, which wraps, but no screenshot was taken
   of this prompt.

   **Stage 2 — a payment is split by number, which closes the item**
   (`feat/a-payment-is-split-by-number`). Item 5 asked "which pool
   first?" and emptied the chosen class as far as the payment went — a
   simplification it named, because a split needs a number for an answer.
   The question is now `payment::Question { pool, min, max }`: **how many
   of the credits come from the first class that could go first**,
   answered by `PlayerAction::ChooseNumber`. The range is what the payment
   allows — no more than the class holds or the payment needs, no fewer
   than the other such classes would leave uncovered — so every number
   offered is a payment that can be made, the class asked about is then
   done with (what it did not give stays on its cards), and the last
   class is never asked about: two classes are one question, three are
   two. A number says both of the old answers (all of it, none of it) and
   the split between them, so `ResolvePendingChoice` left the payment
   path rather than staying for half of it. A range a `ChooseNumber`
   cannot carry is cut to 30, and one whose *fewest* is past 30 is not
   asked (table order); no pool in the game is near either. Everything
   else item 5 built stands as it was: parked by replay, an action parks
   only if some sequence of answers completes it (the search now runs
   over the numbers in the range), masked in the view and in the log — a
   step that parks again is concealed whatever action it was, so a number
   given to a first question is not shown before the action it pays for
   is. **While a payment is parked its numbers are the only candidates:**
   a number decision parked beneath it offers the same action, and the
   payment answers first. `board::breaks` answers a question with the
   first number that goes through, which by the rule above is one the
   person could give, and still waits for theirs when the route is
   carried out. The words are `netrunner_client`'s for both clients: "Pay
   4 credits — how many from Azimat?", and a button a split ("1 from
   Azimat, 3 from the rest").
   *Measured against what was written down first* (`f6a0541` against
   `main` `72cd4f5`, pinned binaries, 192 games a report). Predicted:
   heuristic identical; random differing in at most the one game of the
   pool that asks. **Heuristic identical, by view and by index; random
   differs by one answer and what it paid with, and by nothing else** —
   `ChooseNumber` 3 → 4, `ResolvePendingChoice` 1,001 → 1,000, and the
   random seat took Azimat's credit where it had taken the run's
   (`CountersRemoved` 1,444 → 1,445, `BonusRunCreditsSpent` 11 → 10);
   steps, end reasons and every other key unmoved, view and index alike.
   On the deck pair that asks (*Hostile Bid* against *Pay As You Go*, 192
   random games): **35 questions, 35 numbers, 0 stalled**, end reasons 32
   / 109 / 51 against 32 / 110 / 50; the heuristic pass of the same pair
   is unmoved at 66,472 steps, asking nothing. Both 256-seed sweeps pass;
   `cargo test --workspace` green, clippy silent.
   **Across the item:** one `Effect` and one `Amount`, one `PlayerAction`
   on an appended `ActionSpace` segment (1646 → 1677), one
   `PendingDecision`, no observation change; three cards that took the
   most their text allows now ask, and a payment is split to the credit.
   *Deferred by name:* Phật Gioan Baotixita as one prompt (needs `1 +` the
   number, which `Amount` cannot say; its two paid choices are correct),
   and X costs, which no pool card prints.
7. **A movement phase in the run — DONE (21 September 2026)** (§2.6; was
   item 4), before a card needs "when the Runner passes ICE".
   (`feat/run-movement-phase`, stacked on two fixes it surfaced.)

   **What reading it found first.** The missing phase was not only
   vocabulary. It was two rules deviations from CR 6.9.4 (a pass, b may
   jack out, c move inward, d paid-ability window where the Corp may rez
   what is not ice, e approach the next ice or the server):
   `pass_current_ice` landed straight on the next `ApproachIce` with
   `jack_out_permitted` true, and the `JackOut` handler has no window
   guard, so **the Runner could watch the Corp rez the next ice and then
   leave**. And an iceless server was approached in the action that
   began the run, so **an unrezzed Anoetic Void or Manegarm Skunkworks
   behind no ice could never be rezzed before the approach** it exists to
   punish. No pool card prints "when the Runner passes", so no `Trigger`
   is added (the DSL Growth Rule): `IcePassed` stays an occurrence of
   nothing, and the phase is what a later `OnIcePassed` hangs on.

   **The shape.** `RunPhase::Movement`, entered from every pass, from an
   initiation with no ice, from ice that leaves the table while it is
   approached or encountered (`reconcile_ice`, which no longer approaches
   anything itself and says it moved with an `Option` rather than a
   non-empty event list), and from Proprionegation onto an iceless
   Archives. **Its two moments are told apart by `jack_out_permitted`,
   with no new field:** true, no window, the Runner owes `ContinueRun`
   or `JackOut` (6.9.4b); the `ContinueRun` shuts it and the handler's
   `open_window_if_at_checkpoint` opens the ordinary run window (6.9.4d);
   its close approaches (`approach_next`, 6.9.4e) and the approach
   events are dispatched as before. **Jack-out is movement's only:**
   false at every approach and **no longer legal at `Success`**, so
   `advance_run`'s guard lost its special case. `CompleteRun` stays the
   one step into `RunSucceeded`: an `OnApproachServer` trigger can park a
   decision (Manegarm's), and an explicit action means nothing has to
   auto-resume; `ActionSpace` is unmoved at 1677. **`OBS_SIZE` 2262 →
   2263**: a seventh run-phase slot, appended to the one-hot so the six
   before it keep their meaning. No promoted network reads it.

   **Two bugs it surfaced, each fixed in its own commit beneath it.**
   The 256-seed view sweep stalled at seed 86 (Peculiarity vs
   Enthusiasm, random vs random): **Red Team**, trashed mid-run, left a
   rider whose "take 3[c] from this resource" failed with
   `CardNotEligibleForCounters`, which made `CompleteRun` illegal. On
   `main` the Runner could still jack out at the server, which hid the
   bug and cost a run that should have succeeded; with that gone the
   position had no legal action. The rider is now `EffectIf
   ThisCardIsInstalled` (card data only). And the client's
   `every_legal_action_is_one_entry_and_every_target_is_on_the_board`
   walked seed 3 into a **Petty Cash in Archives**, a legal
   `PlayOperation` the action map pointed at a hand card that was not
   there, so no click on the board offered it; an operation not in HQ is
   now reached from Archives.

   **Measured** (`scripts/coverage_identical.py main HEAD`, one pass of
   the pool, 192 games a report; seeds 1 and 2; view and index shapes
   agree). Not identical, as predicted. Random-vs-random, seed 1 / 2:
   `ServerApproached` **2,573 → 1,322** / **2,767 → 1,297**, while
   `RunSucceeded` holds (1,270 → 1,293 / 1,309 → 1,266). The random
   Runner used to reach every approach and then jack out of half of them
   at the door; it now makes that choice in movement, before the
   approach, and an approach is nearly always a success (per server
   kind the gap is the runs an upgrade ended there). So Anoetic Void and
   Manegarm fire about half as often at random (67 → 31 and 102 → 37 on
   seed 1). **Predicted the other way and wrong:** I expected more
   upgrade firings from the new rez window, and missed that the
   jack-out moving ahead of the approach halves how many approaches a
   random Runner makes. `IceRezzed` 1,246 → 1,198: a random Runner no
   longer sees a rez before it leaves. Heuristic-vs-heuristic: the
   Runner stopped jacking out (18 → 1 / 15 → 6); its wins 152 → 147 and
   159 → 161, opposite signs and inside the 0.026–0.047 seed-spread
   band, so **no strength effect is claimed**. Steps per game +8–16%
   (heuristic 430 → 468 on seed 1), far under `MAX_STEPS`. **Observed
   and not explained:** heuristic Manegarm firings fall on both seeds
   (120 → 89, 77 → 55) with its install and rez counts flat and
   approaches down only 2.6%. A card test shows the engine is right (it
   hears the approach behind ice, once:
   `manegarm_skunkworks_hears_the_approach_after_the_last_ice_is_passed`),
   so it is which runs the heuristic plays, and it is left there. Both
   256-seed sweeps clean, coverage gate included; `cargo test
   --workspace` green (desktop included), clippy silent, the `onnx`
   feature's tests green. **Four lessons** said the Runner may jack out
   at the server; *Tread Lightly* taught exactly that and now teaches
   it after Palisade is passed.

   *Deferred by name:* the approach-server step as a phase of its own (it
   stays `Success`'s entry, which nothing needs apart), and a trigger on
   passing ice, until a card prints one.
8. **Cost types — IN PROGRESS (21 September 2026)** (§2.4; was item 5).
   jinteki's 50 are the backlog, taken as cards need them.

   **What reading it found first: no card needs a cost type to exist, and
   ten write one as an effect.** Both set gates' exclusion lists are empty,
   so nothing waits on a missing variant. What the pool has instead is
   printed costs written as effects, which is wrong three ways. *The cost
   is paid after the thing it pays for* (Fermenter and Rent Rioters
   trashed themselves as the ability's last step). *A cost can be
   prevented*, which Comprehensive Rules 1.16.1a forbids: Fermenter's
   trash went through `prevention::would`, so a Runner with Sacrificial
   Construct was asked and kept the card it had cashed in, and Semak-samun's
   "unless the Runner suffers 3 net damage" can be met by Net Shield.
   *Affordability is a requirement beside the payment*
   (`ZoneHasAtLeast`, `ScoreAreaHasAtLeast` on Carnivore, LEO Construction,
   Anoetic Void, Plutus), and `Effect::ForfeitAgendas` never asks the Corp
   which agenda. The reason is one constraint: `ability::pay_cost_ctx`
   could not ask a question, so every cost that picks a card became an
   effect. The plan, in stacked PRs: the costs that pick nothing
   (self-trash, remove tags, suffer damage), then a cost that picks cards
   parked by replay the way a payment split already is
   (`RulesError::PaymentChoiceNeeded`), then forfeit.

   **Stage 1: a self-trash is a cost** (`fix/trash-self-is-a-cost`).
   Fermenter is `AllOf[Clicks(1), TrashSelf]` and Rent Rioters
   `AllOf[Clicks(3), TrashSelf]`, as Humanoid Resources already was.
   Fermenter's "for each hosted virus counter" then reads a card its cost
   has trashed, so the payer remembers the install's numbers before it pays
   (`ResolutionContext::last_known`, taken in `engine::activate_ability`)
   and `counters_of`/`advancement_tokens_of` read them only when the named
   install has left play — the same object's memory, never a sibling copy's
   (`a_trashed_fermenter_counts_its_own_counters_not_a_sibling_copys`). On
   the context, not on `GameState`: the State Hygiene Rule's test, since
   it is read within one resolution and nothing that reads it parks first.
   `fermenter_is_trashed_as_its_cost_and_sacrificial_construct_is_not_asked`
   failed on `main`. **Measured** (`coverage_identical.py main`, 192 games,
   all four shapes): no game moved; only `effects_seen` did, `TrashCard` and
   `Sequence` each down by the cash-outs now paid as a cost (61 random, 1
   heuristic). Predicted identical, and missed that `effects_seen` counts
   the trash that left the effect. Sacrificial Construct is in a Sweep deck
   only, which no matchup pairs with Fermenter, so no game could show the
   bug.

   **Stage 2: Clearinghouse's trash is a cost**
   (`fix/clearinghouse-trash-is-a-cost`). "You may trash this asset **to**
   do 1 meat damage for each hosted advancement counter" is a cost before
   "to" (CR 1.16.11), and it was a `PresentChoice` whose first option
   dealt the damage and *then* trashed. It is now an `OfferPaidChoice` with
   `cost: TrashSelf`, as Maglectric Rapid's "you may trash this hardware to
   derez" already was, and the damage counts the tokens on the trashed
   asset through the same `last_known`, taken in
   `pending_choice::resolve_accept`. **"If you do" is not a cost**, so
   Idiosyncresis, Mitra Aman and Knickknack stay effects: they read their
   numbers before their trash, and no pool card prevents a Corp card
   trashing itself. **Measured** against stage 1: heuristic, both shapes,
   the same games — 79 Clearinghouse decisions moved from
   `ResolvePendingChoice` to Accept/Decline, and Clearinghouse trashed 13 →
   16, which is the three flatlines where the trash now lands before the
   damage that ends the game rather than never. Random differs throughout,
   by trajectory: the random Corp picks among different actions (39 of its
   decisions moved), and Clearinghouse's own numbers are within that drift.

   **Stage 3: removing a tag is a cost** (`feat/cost-remove-tags`).
   `Cost::RemoveTags(n)`, payable only with n tags to remove, and Synapse
   Global: Faster than Thought's "[click], remove 1 tag: Gain 2[credit]" is
   `AllOf[Clicks(1), RemoveTags(1)]` with the `IsTagged` requirement gone —
   the requirement was affordability, written beside the payment. The
   removal is the identity's own trigger ("the first time each turn a tag
   is removed"), so a cost's events now reach the listeners: **`pay_cost_ctx`
   dispatches nothing, and the payer calls `ability::dispatch_cost_events`
   after the effect**, which generalises the one special case
   `resolve_accept` had (a `TagsGiven` paid as Funhouse's cost, which NBN:
   Reality Plus hears) to every cost event a card can hear. *After the
   effect, not between:* Comprehensive Rules 1.16.3 puts a checkpoint after
   a payment, which would resolve Synapse's install before its credits; a
   dispatch there would leave the effect to run under whatever the
   reaction parks, which only a `Sequence` knows how to wait behind, and no
   pool card can tell the orders apart. So Synapse's 2[credit] now land
   before its install prompt rather than after. `ClearTags` is used by no
   card, and is kept as the word for "remove all tags" beside the new one.
   **Measured** against stage 2: heuristic identical in both shapes (the
   heuristic Corp never activates it); random, no game moved — only
   `effects_seen/RemoveTags` 52 → 31, the 21 activations whose removal is
   now a cost, and the `Sequence` each one no longer needs.
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
