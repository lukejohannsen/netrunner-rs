# Core Set (implemented subset) Card Fidelity Audit — September 2026

The System Gateway audit (`docs/system-gateway-card-audit.md`), repeated over the **19 implemented Core Set cards** — the hand-authored baseline set that predates System Gateway. Same three-way rubric: text ↔ DSL against the NetrunnerDB catalog (`data/cards/core.json`, joined on `numeric_id`), engine semantics of every primitive read down to the code, and playability evidence. Status and findings live in `ROADMAP.md`; this file is the per-card record.

**Two structural differences from the SG audit:**

- **No completeness gate applies.** Core is deliberately partial (19 of 250+ printed cards); `every_system_gateway_card_is_implemented_or_explicitly_excluded` has no Core counterpart, and none is owed until someone declares the set a target.
- **No sweep coverage.** No sample deck (`data/decks/*.json`) contains a Core card, so the agent-driven sweeps and the random-vs-random coverage report never touch them. **The per-card tests are the play evidence**, and engine changes made for Core cards show as RNG-level noise in the SG measurement. Recorded as a follow-up in `ROADMAP.md`: a Core-flavored sample deck pair would put these cards under the same sweep pressure as SG's.

**Verdicts** as in the SG audit: `Faithful`, `Faithful*` (documented approximation, listed at bottom), `Fixed` (branch `fix/core-set-card-fidelity-audit`).

## Identities

| Card | Verdict | Notes |
|---|---|---|
| Haas-Bioroid: Engineering the Future | Faithful | `OnInstall` + `FirstInstallThisTurn`; `CardInstalled` dispatches for every Corp install, including Ansel 1.0's effect install — which the printed "each time you install a card" does count. |
| Jinteki: Personal Evolution | Faithful | Both scored and stolen; a mid-access steal's net damage can flatline mid-run (`finish_if_game_over`). |
| NBN: Making News | Faithful | The identity recurring pool, spent ahead of the wallet in trace bids, refilled at turn start — the one recurring-credit card the engine models (ROADMAP §4). |
| Weyland Consortium: Building a Better World | **Fixed** | The identity was faithful; its *targets* were not — Hedge Fund, Hansei Review and Predictive Planogram are printed Transactions but lacked the engine-facing `subtypes` field, so the identity never fired on them. Found here, fixed in those three JSONs. |
| Gabriel Santiago | Faithful | `OnSuccessfulRunOnHq` + first-per-turn flag. |
| Kate "Mac" McCaffrey | Faithful | Program-or-Hardware first-install discount (`InstallKind` split), never Resources. |
| Noise | Faithful | `OnVirusInstalled` → Corp mills 1 (facedown to Archives); also reacts to effect installs (Cookbook parity). |

## Corp cards

| Card | Verdict | Notes |
|---|---|---|
| Enigma | Faithful | "The Runner loses [click]" saturates at 0, per the loses-never-fail convention. |
| Hostile Takeover | Faithful | |
| Ice Wall | **Fixed** | Neither printed clause was modelled: no `advancement_requirement: 0` marker, so `AdvanceCard` refused it, and no per-counter strength — `WhileHostedAdvancementsAtLeast` is a threshold, not a rate. New `StrengthModifier::PerHostedAdvancement(1)`, baked at encounter like its siblings (advancement cannot change mid-run). |
| PAD Campaign | Faithful | |
| Scorched Earth | Faithful | `IsTagged` play requirement; 4 meat. |
| Snare! | Faithful* | `interactive_on_access` (`CorpPaysToApply`, `Not(AccessingArchives)`), unaffordable ⇒ not offered. The R&D "must reveal it" clause is implicit — both sides observe the access in this engine; no separate reveal is modelled. |
| Wall of Static | Faithful | |

## Runner cards

| Card | Verdict | Notes |
|---|---|---|
| Account Siphon | **Fixed** | Two deviations. The replacement was mandatory — printed "you **may** force…": `SetAccessReplacement` gains `optional` (old wire format unchanged), and `try_replace_access` parks the Runner's siphon-or-breach choice; declining consumes the replacement and the next `CompleteRun` breaches normally. And the gain was a flat 10 — printed "2[c] **for each credit lost**": against a 3-credit Corp the Runner gains 6. New `Amount::CreditsLostThisResolution` reads `ResolutionContext::credits_lost`, recorded by `Effect::LoseCredits` (whose `CreditsLost` event now reports the actual amount removed); the ×2 is composed as two `GainCreditsAmount`s. Faithful*: "up to 5" is always forced at 5 (saturating), and taking the siphon is recorded as `RunEndedByEffect`. |
| Corroder | Faithful | The pump's printed bare "+1 strength" defaults to encounter duration under Null Signal Games' rules — correct as authored. |
| Diesel | Faithful | |
| Gordian Blade | **Fixed** | The pump is printed "for the remainder of **this run**" but was Encounter-scoped, expiring between two ICE of the same run. New `BoostDuration::Run` backed by `InstalledRunnerCard::run_strength_buff`, cleared only in `run::engine::end_run` — the one choke point every run conclusion passes. |
| The Maker's Eye | Faithful* | The +2 access is granted at play time rather than "when you breach"; an unsuccessful run accesses nothing either way (same pattern as Conduit). |

## Documented approximations (deliberate)

- **Snare!** — "must reveal it" in R&D is implicit in the engine's access model; nothing distinguishes a revealed access from a seen one.
- **Account Siphon** — "up to 5" always forces the maximum (choosing less is never right in this pool); the siphon conclusion is logged as `RunEndedByEffect` rather than a distinct replaced-breach event.
- **The Maker's Eye** — access bonus timing, as above.
- **Corroder/Gordian Blade pump wording** — bare "+1 strength" = encounter duration is NSG's own default, not an approximation; noted here so nobody "fixes" Corroder to match Gordian.
