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
3. **"The first time each turn" as a query — TAKEN UP (20 September 2026,
   four stages; the first is built)** (§6.3; new). Roughly ten per-turn
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
   already includes the occurrence, as a refactor for the cards where the
   two spellings agree; (4) the correction — a card that arrives after
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
