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
