# System Gateway Card Fidelity Audit — September 2026

Status and the findings list live in `ROADMAP.md` (§ System Gateway Card Fidelity Audit); this file is the per-card record. Every one of the 75 playable *System Gateway* cards was read three ways:

1. **Text ↔ DSL** — every clause of the NetrunnerDB `stripped_text` maps to a trigger/ability/subroutine/field, nothing extra, nothing missing; "may" is optional, "unless" is a real payable choice by the right side, "X or Y" is chosen by the side the card names.
2. **Rules semantics** — the primitives the card uses were read down to their engine code (timing window, who chooses, cost vs. effect, self-reference by `InstallId`, per-turn scoping), not trusted by name.
3. **Playable** — the card is reachable through `legal_actions`, has a per-card test exercising its distinctive clause, and shows activity in the 96-game random-vs-random coverage report (`target/coverage/*-random-random.json`). Where random play can't reach a clause (score triggers rarely fire in random-vs-random), the per-card test is the cited evidence.

**To re-run the audit's mechanical half**: `cargo test -p netrunner_core` (per-card tests, catalog gates), both deep sweeps (`NETRUNNER_SWEEP_SEEDS=256 cargo test -p netrunner_single_player --release` and `-p netrunner_session`), and the coverage measurement in `AGENTS.md`'s Testing Rule. The text comparison itself is a human pass: `data/{corp,runner}/*.json` against `data/cards/system_gateway.json`'s `stripped_text`, joined on `numeric_id`.

**Verdicts**: `Faithful` — matches print, as modelled. `Faithful*` — matches print through a documented approximation listed at the bottom; the observable difference is nil or named. `Fixed` — a deviation this audit found and fixed (branch `fix/sg-card-fidelity-audit`); the old behavior is described.

Every card below is in a sample deck, so both agent-driven sweeps exercise all of them.

## Identities

| Card | Verdict | Notes |
|---|---|---|
| René "Loup" Arcemont | Faithful | `OnTrashedFromAccess` + `OncePerTurn`; fires for both the paid trash and Carnivore's free trash (both dispatch `CardTrashedFromAccess`). |
| Zahya Sadeghi | **Fixed** | Her once-per-turn is consumed when the trigger fires, and `OnRunEnded` fires for bounced runs too — a 0-access HQ/R&D run burned it for a gain of 0. Gated by new `AccessedAnyCardDuringLastRun`. Faithful* on the residual "may". |
| Tāo Salonga | Faithful* | Swap keyed by `InstallId`; legal mid-run (`run::reconcile_ice`). The swap option no-ops with <2 installed ICE rather than being withheld. |
| Haas-Bioroid: Precision Design | Faithful | +1 hand size at setup; optional Archives→HQ on score (`PresentChoice` + `PromptChooseCards`). Score path pinned by test (random play rarely scores). |
| Jinteki: Restoring Humanity | Faithful | 1[c] **per** facedown card (`Amount::FacedownCardsInArchives`, fixed in `fix/card-fidelity`); fires on skipped discard phases too. |
| NBN: Reality Plus | **Fixed** | A tag taken as a **cost** (`Cost::TakeTags`, Funhouse) emitted `TagsGiven` without dispatching it — the printed NBN+Funhouse pairing never fired. The cost's tag events now dispatch, deferring if the choice's effect parked something. |
| Weyland: Built to Last | Faithful | `WasFirstAdvancementThisCard` off the event's own token count. Seamless Launch **places** counters (no `CardAdvanced` dispatch) and correctly does not pay Weyland. |

## Icebreakers & programs

| Card | Verdict | Notes |
|---|---|---|
| Buzzsaw | Faithful | Break up to 2 code gates; `DuringEncounter`-gated like every breaker. |
| Cleaver | Faithful | (Cost transposition fixed at set completion.) |
| Carmen | Faithful | Install discount via `install_cost_discount_if`, re-evaluated per install. |
| Marjanah | Faithful | Break-ability discount via `cost_discount_if`, not install. |
| Echelon | Faithful | `PerInstalledIcebreaker(1)`, counts itself. |
| Unity | Faithful | Pump is `BoostStrengthAmount(InstalledIcebreakerCount)`. |
| Mayfly | **Fixed** | Was trashed at the end of **every** run, used or not. The self-trash rider belongs to the break ability: the break now leaves a hosted counter as the used-this-run marker; `OnRunEnded` requires it. No `counter_kind`, so a purge never touches the marker; the counter leaves with the card. |
| Botulus | Faithful | Trojan; per-copy counters; break gated on `EncounteringHostIce`. |
| Leech | Faithful* | −1 applies to `RunIce::current_strength`, reset with the run; an ICE is encountered at most once per run in this pool, so run-scope ≡ encounter-scope. |
| Tranquilizer | Faithful | Derez via `CardTarget::HostIce` by install. |
| Fermenter | Faithful* | "click, trash:" — the trash is modelled as an effect after the payout, not a `Cost` (no compound cost primitive). No trash-prevention card exists to observe the difference. |
| Conduit | Faithful* | Its counter lands at `RunSucceeded` (pre-breach) rather than "when the run ends"; X for its own click-run was already fixed at activation, so the same run's breach is unaffected. |

## Runner events

| Card | Verdict | Notes |
|---|---|---|
| Wildcat Strike | Faithful | `PresentChoice { chooser: Corp }`. |
| Mutual Favor | **Fixed** | "If you made a successful run this turn, you may install that program" was unmodelled — the find always went to the grip and cost a click later. The search's `then` now offers `InstallRunnerCardFromGrip` (paying the cost) behind `MadeSuccessfulRunThisTurn`. Faithful*: an unaffordable find's install option no-ops. |
| Tread Lightly | Faithful | `rez_cost_delta` on the run; only the attacked server's ICE can be rezzed during a run anyway (approach-only rez), so "each piece of ice" is fully covered. |
| Creative Commission | Faithful | Click loss behind `RunnerClicksAtLeast(1)`, checked post-payment. |
| VRcation | Faithful | Same shape. |
| Jailbreak | Faithful | `allowed_servers` HQ/R&D; draw + extra access as the run's `on_success` rider, in place before the same breach. |
| Overclock | Faithful | 5 `bonus_run_credits`, first in the credit waterfall, gone at run end. |
| Sure Gamble | Faithful | Trivial `GainCredits`; no dedicated test needed. |

## Runner hardware & resources

| Card | Verdict | Notes |
|---|---|---|
| Carnivore | Faithful* | Access-window gating via `CurrentlyAccessingACard` + `ZoneHasAtLeast(grip, 2)`; the grip trash is the ability's selection rather than a `Cost` — equivalent in this pool. |
| Docklands Pass | Faithful* | "breach HQ" modelled as `OnSuccessfulRunOnHq`; no breach-without-run effect exists in System Gateway. |
| Pennyshaver | Faithful | Place 1 then `TakeAllCountersAsCredits`. |
| DZMZ Optimizer | Faithful | Program-only first-install discount (`InstallKind` split). |
| Pantograph | **Fixed** | "Then, you may install 1 card from your grip" was unmodelled. Now offered on both scored and stolen via `InstallRunnerCardFromGrip`, with `CardFilter::InstallableRunnerCard` keeping the offer to what's actually installable (type, affordability, MU, console limit). Faithful*: Trojans are excluded — their host is a choice no parked effect models. |
| T400 Memory Diamond | Faithful | +1 MU derived from the rig; +1 hand size. |
| Red Team | Faithful* | Payout rides its own run (`on_success` by install). **"a central server you have not run this turn" stays unmodelled** — needs a per-turn record of servers run (ROADMAP follow-up). |
| Telework Contract | Faithful | Per-install once-per-turn; self-trash on empty. |
| Smartware Distributor | Faithful | Not self-trashing at 0 — the card never says to. |
| Cookbook | Faithful | Optional counter on the *installed* virus (targeting dispatch); also reacts to effect installs. |
| Verbal Plasticity | Faithful | "instead draw 2" ≡ one extra card on the first basic draw. |

## Corp identities' cards — agendas

| Card | Verdict | Notes |
|---|---|---|
| Luminal Transubstantiation | Faithful | 3 clicks + `PreventScoringForRemainderOfTurn`, cleared next turn. |
| Longevity Serum | Faithful | Chained prompts through `then` (the documented `Sequence`-parking pattern). |
| Tomorrow's Headline | Faithful | Both scored and stolen; scored branch pinned by test. |
| Above the Law | **Fixed** | The printed "you **may** trash 1 installed resource" had been made mandatory by the previous fidelity pass (a misread); the opt-out `PresentChoice` is restored. |
| Offworld Office | Faithful | |
| Orbital Superiority | Faithful | Ordered `EffectIf` pair; tagged-at-score decides. |
| Send a Message | Faithful | `UnrezzedIce` filter; rez by `InstallId`, free, both scored and stolen. |
| Superconducting Hub | Faithful | Mandatory +2 hand size, optional draw. |

## Corp assets & upgrades

| Card | Verdict | Notes |
|---|---|---|
| Nico Campaign | Faithful | Per-copy pool; trash-and-draw on empty. |
| Urtica Cipher | **Fixed** | `OnAccessed` fired for R&D/HQ accesses too — flat 2 net from a deck access, and a first-match token read could size the hit by an installed copy. Gated by new `ThisCardIsInstalled`, answered strictly off the install the dispatch named. |
| Spin Doctor | Faithful | Draw on rez; `Cost::RemoveSelfFromGame` ability, rezzed-only like every paid ability. |
| Clearinghouse | Faithful | Damage sized before the self-trash; per-copy resolution by install. |
| Regolith Mining License | Faithful | |
| Manegarm Skunkworks | Faithful | `Cost::AnyOf([Clicks(2), Credits(5)])`; unaffordable branches unofferable, both unaffordable ⇒ the run ends. |
| Anoetic Void | Faithful | Real, declinable cost; not offered with <2 in HQ (fixed in `fix/card-fidelity`). |
| AMAZE Amusements | Faithful | `persistent_after_trash` + `StoleAgendaDuringLastRun`. |
| Malapert Data Vault | Faithful | Root-of-this-server audience on `AgendaScored`; optional non-agenda R&D search. |

## Corp operations

| Card | Verdict | Notes |
|---|---|---|
| Seamless Launch | Faithful | `NotInstalledThisTurn` per-instance; advances the chosen install. Places counters — is not "advancing" (no Weyland payout). |
| Sprint | Faithful | Draw 3, then choose 2 of HQ into R&D, shuffled. |
| Hansei Review | **Fixed** | The previous pass made the HQ trash **random** ("removes the drawback") — but the printed card says nothing about random, and a player trashing from their own hand chooses. Reverted to a Corp `PromptChooseCards`; `CardTarget::RandomFromHq` deleted with it (no user left). |
| Neurospike | Faithful | `AgendaPointsScoredThisTurn` (printed points, reset each turn). |
| Predictive Planogram | **Fixed** | Tagged, it forced both halves; the printed "you **may** resolve both instead" is now a three-option choice (gain, draw, both). |
| Public Trail | Faithful | Hard `play_requirement`; 8[c]-or-tag paid choice by the Runner. |
| Government Subsidy | Faithful | |
| Retribution | Faithful | `IsTagged` play requirement; program-or-hardware selection. |
| Hedge Fund | Faithful | Trivial; no dedicated test needed. |

## Corp ICE

| Card | Verdict | Notes |
|---|---|---|
| Ansel 1.0 | **Fixed** | Its install subroutine used Brân's cost-ignoring, fixed-destination effect: free, always into Ansel's own server, agendas into central roots, no install-over, Operations selectable. Now `Effect::PromptInstallCorpCard`: the Corp picks the card (installable types only) and then the destination (`PendingDecision::ChooseServer` reused, `PendingInstallFromZone` by position so no HQ pick leaks through the view), paying the ICE tax; agendas/assets remote-only via `engine::corp_install_destinations`. |
| Brân 1.0 | Faithful | "directly inward, ignoring all costs" — positional `insert_after`, by install. |
| Diviner | Faithful | `LastDamageTrashedOddCostCard` reads the same resolution's discards; printed cost from the registry; 0 is even. |
| Karunā | **Fixed** | "The Runner may jack out" was a flag (`PermitJackOut`) no loop paused for — both subroutines always fired in one batch. The first subroutine now parks a Runner choice between the two; taking it ends the run with subroutine 2 unfired. Faithful*: recorded as `RunEndedByEffect`, not `RunJackedOut`. `Effect::PermitJackOut` deleted with it. |
| Funhouse | Faithful | Encounter tag-or-ETR (`TakeTags` cost); subroutine tag-unless-4[c]. |
| Ping | Faithful | `RezzedDuringRunAgainstThisServer`; the approach-only rez rule makes the window exact. |
| Ballista | Faithful* | Corp `PresentChoice`; the Corp may pick the trash option with no program installed (a no-op) instead of ETR — a legal self-punt the rules arguably disallow; never advantageous. |
| Pharos | Faithful | Advanceable ICE; `WhileHostedAdvancementsAtLeast{3, +5}`. |
| Palisade | Faithful | `WhileProtectingRemote(+2)`. |
| Tithe | Faithful | |
| Whitespace | Faithful | Lose 3, then ETR at ≤6 — checked after the loss. |

**Engine-wide finding fixed alongside Ansel:** every ICE install appended to `corp.installed`, whose per-server vec order is the run's approach order — so each new piece landed *innermost*, reversing the approach order of every stacked server. New ICE now installs **outermost** (Null Signal Games' install rule); Brân's inward install keeps its own positioning.

## Documented approximations (deliberate, do not "fix" without a reason)

- **Red Team** — "a central server you have not run this turn" needs a per-turn record of servers run; tracked in `ROADMAP.md`.
- **Docklands Pass** — "breach" modelled as successful run; equivalent until a breach-without-run card is implemented.
- **Zahya Sadeghi** — the gated payout is mandatory; the printed "may" is only observable as declining a ≥1[c] gain to hold the once-per-turn for a bigger run the same turn. Consumption-at-fire is `TriggeredEffect::requirement`'s design.
- **Mutual Favor** — an unaffordable found breaker's install option resolves to a no-op (the card stays in the grip) rather than being withheld.
- **Trojan effect installs** (Pantograph) — excluded from `InstallableRunnerCard`: the host is a choice no parked effect models; a Trojan installs with a click.
- **Fermenter / Carnivore** — a printed trash-/discard-cost modelled as part of the effect; no prevention/replacement card exists to observe the difference.
- **Conduit** — counter placed at run success rather than run end; unobservable in this pool.
- **Tāo Salonga / Ballista** — a choice option that would do nothing is offerable and no-ops, rather than being withheld.
- **Karunā** — jacking out through its choice is recorded as `RunEndedByEffect`.
- **`OncePerTurn` on identities/events** is tag-only; **recurring credits** are identity-only; **`AccessState`** is `CardId`-keyed (two copies of one upgrade in one root) — all pre-existing, recorded in `ROADMAP.md`.
