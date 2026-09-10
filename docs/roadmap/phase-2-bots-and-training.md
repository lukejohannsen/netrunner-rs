# Phase 2 — Bot Intelligence, Replay, Gym and Training

Area roadmap; the index is `ROADMAP.md`. Addresses: "Phase 2 §1"–"§5". §5 is the training-loop log: entries are dated, and later ones correct earlier ones — read to the end before acting on a claim.

## 1. Determinization — DONE
`determinize` samples a concrete `GameState` from a masked `ClientView`; `HeuristicAgent`, `MctsAgent` (per tree — basic IS-MCTS) and `PuctAgent` use it.

## 2. Action Replay & Match Logging — DONE
`MatchHistory` records `(turn, side, action, events)`; replay is asserted bit-identical. **JSON-Lines** (`feat/jsonl-history`): line 1 is a `MatchRecordHeader { seed, corp_deck, runner_deck, rules }` (decks whole, no footer), then one entry per line; `--headless --record <dir>`. **Replay viewer** (`feat/replay-viewer`): `netrunner_cli replay <file> [--side]` recomputes every position through `apply_action` (a diverged record fails on load), renders each frame as the chosen chair's `ClientView`, `s` swaps chairs. Open: per-variant event narration.

## 3. Training Decks & Self-Play Data — DONE
The 28 published NSG decklists are embedded; `matchups()` is the 16 × 12 cross product. Real decks replaced a blank-filler fixture whose 5,000 games all ended in a Corp loss. `OBS_SIZE = 990` with five card-identity planes over a 192-slot vocabulary by `numeric_id` (append-only). `--corp onnx --model` seats a policy. Bugs found by real decks: Send a Message's rez targets, `ToggleCardSelection` by `CardId` (superseded by positions), `determinize` resampling under a parked selection, the greedy toggle two-cycle (`MAX_GREEDY_REPEATS`). Smoke run: budget hits 8/12 → 0/12, median length 6,674 → 171.

**Running the loop:**
```bash
source scripts/venv/bin/activate
python3 scripts/run_iteration_loop.py --iterations 50 --games-per-iter 100 --simulations 200
```
Each iteration ends with an arena (`netrunner_selfplay --arena-candidate <onnx> [--arena-incumbent <onnx>] -n 384 -s 128`, every matchup from both chairs); promotion at `--promote-threshold` 0.55; verdicts in `data/checkpoints/promotions.log`, refused checkpoints kept as `rejected_iter_NNN.onnx`. **Layout (September 2026):** everything generated lives under `data/` and nowhere else — `data/selfplay` and `data/checkpoints` are the live pair the scripts default to; a finished run is moved to `data/runs/<name>/` keeping its checkpoints and logs (`promotions.log`, `iterations.log`, `overnight.log`, `launch_overnight.sh`) and its corpus is deleted once this file records what it showed, because every corpus so far was recorded under a layout a later fix invalidated. Measure a checkpoint the way self-play plays it: `--headless --corp puct-onnx --runner puct --model …`.

## 4. Gym & Self-Play Harness — DONE
Observations are encoded from a `ClientView` (fog-respecting). **Benchmark suite** (`feat/rating-and-bench`): `netrunner_cli bench --bots random,heuristic,puct --games 12 --seed 1 --simulations 32 [--threads] [--report] [--ratings] [--label]` plays every ordered pairing (self-pairings included), rated in play order after all games finish so the ladder reproduces across thread counts. First ladder: heuristic 1905 / 1904, `puct@32` 1838 / 1759, random 1219 / 1384 (Corp / Runner).

## 5. Bot and Training-Data Quality — OPEN

### 5a. The heuristic evaluator — DONE (six branches, September 2026)
All surfaced by Phase 1 §3 letting a Runner install a program. Each is a `Weights` term with its arithmetic on the constant. Heuristic-vs-heuristic, 96 games, seed 1:

| branch | blindness | fix | load-bearing delta |
|---|---|---|---|
| Rules Audit §0.5 | a breaker was unvaluable (installing cost C scored 0.5 − 0.4C against +0.4 for a credit) | breaker coverage per ICE subtype, run and advancement terms | encounters 0 → 723 |
| `fix/heuristic-runner-run-term` | ran into ICE it could not break | `ACTIVE_RUN_WEIGHT` only when `run_is_breakable` (affordability over rezzed ICE ahead) | `SubroutineFired` 1,006 → 493, flatlines 58 → 31 |
| `fix/heuristic-runner-savings-term` | — | `SAVINGS_SHORTFALL_WEIGHT` 0.3 per credit short of the cheapest useful grip breaker | **inert** — installs were draw-limited |
| `fix/heuristic-runner-draw-term` | drew once in 96 games | `GRIP_SHORTFALL_WEIGHT` 0.7 below `GRIP_FLOOR` 3 | draws 1 → 388, programs 64 → 90, flatlines 32 → 6 |
| `fix/heuristic-corp-draw-and-rez` | the Corp never drew; no 2-cost asset was ever rezzed | `UNREZZED_INSTALL_WEIGHT` 1.0, `REZZED_ICE_WEIGHT` 1.4, `REZZED_ASSET_WEIGHT` 1.0, `HQ_FLOOR` 3 guarded by `RD_DRAW_RESERVE` 5; `UNRESOLVED_DECISION_WEIGHT` 2.0 after the floor exposed a toggle stall (seed 77) | Corp draws 0 → 138, Nico rezzed 0 → 35 of 37 |
| `fix/heuristic-carmen-and-agenda-placement` | Carmen never installed; agendas in naked remotes | `BREAKER_COVERAGE_WEIGHT` 3.0; `AGENDA_PROTECTION_WEIGHT` 0.5 per ICE, cap 2, read on the agenda | Carmen 0 → 22, Corp wins 37 → 50 |

Net: 0.7 → 1.3 programs a game, 86 / 1,006 → 199 / 519 broken / fired, 58 → 5 flatlines. The evaluator is still one-ply; what remained was personality work (Phase 3 §1).

### 5b. Measurement hygiene — DONE
- **`determinize`'s pools were `HashMap` order** (`fix/determinize-pool-order`): two runs of one seed differed in 282 keys. Sorted by `CardId`; heuristic seatings are byte-identical since. Identities are filtered from the pools (they had been sampled as hidden cards).
- **The seed-spread band** (heuristic-vs-heuristic, 96 games, seeds 1–5 at `fe5ee02`; `target/coverage/noise-hh-seed{1..5}.json`) — what a heuristic delta must beat (AGENTS.md Testing Rule):

  | metric | spread over seeds 1–5 |
  |---|---|
  | steps | 48,256 – 58,221 (19%) |
  | `RunInitiated` / `RunSucceeded` | 27% / 27% |
  | `IceEncountered` / `SubroutineFired` / `SubroutineBroken` | 21% / 27% / 48% |
  | `AgendaStolen` / `AgendaScored` | 14% / 13% |
  | `ProgramInstalled` / `CardInstalled` | 8% / 11% |
  | flatlines / Corp point wins / Runner point wins | ±2 / ±3 / ±5 games |

  Event counts swing a fifth to a half between seeds; the outcome split moves 3–5 games. Run-to-run drift was an order of magnitude smaller — seed choice, not nondeterminism, was the noise. A small heuristic effect shows across several seeds or not at all.

### 5c. The training loop, chronologically — OPEN

1. **Retrain on the new evaluator** (`feat/puct-onnx-seat-and-retrain`): six ungated iterations collapsed to "the Runner wins" (86–6 by iteration 2), and the trained network was worse than uniform search on both sides. New `puct-onnx` bot kind. Needed: promotion gating, a per-game validation split, then volume.
2. **Arena gating and the per-game split** (`feat/arena-promotion-gating`): verdicts 0.24–0.42, all refused; self-play stayed balanced. The honest validation loss was 4.3–5.1 against 2.2 under the step split — the network memorised games.
3. **Volume** (`feat/selfplay-volume`): un-promoted iterations had replayed the *same* games (seed = index; now `--seed-offset`, the seed is recorded and duplicates are refused); trajectories were 97–99% zeros as text (now sparse `[index, value]` pairs, 190 MB → 11 MB per 96 games); `--window N`. **Scaling on a 2,400-game corpus:** policy loss over the entropy floor 1.66 → 1.39 → 1.18 → 1.06 nats at 96 / 288 / 960 / 2,400 games; **the value head never beat predicting zero** (MSE 1.04–1.30 against 1.00). ONNX thread pinning measured 33s → 41s and was reverted; the ~2 ms per evaluation is per-call overhead.
4. **First 2,400-game run, stopped after five iterations**: refused at 0.27–0.43; **14–23 of 48 arena games were draws** — `MAX_STEPS`, each worth half a point. Checkpoints and logs under `data/runs/uniform_volume_v1/`; the corpus is deleted.
5. **A cycle break in `select_action`** (`fix/puct-select-cycle-break`): `netrunner_bots::pick_action` strikes the repeated action and samples the rest by visits. Arena draws 14 → 2, score 0.271 → 0.208 (the honest verdict). `PuctSearchStats::root_value` added; `MctsAgent` got the same fix, defensively.
6. **Value target and weight** (same branch): `--value-target-mix λ` (outcome vs `search_value`), `--value-loss-weight w`. Grid on 960 games:

   | λ | w | MSE vs outcome (predict-zero 1.00) | sign accuracy |
   |---|---|---|---|
   | 0 | 1.0 | 1.167 | 64.7% |
   | **0.5** | **0.25** | **0.896** | **66.1%** |
   | 1.0 | 1.0 | 1.140 | 46.3% |

   Defaults λ 0.5, w 0.25. **Neither search negated at opponent nodes — fixed** (`select_edge`/`uct` flip the exploitation sign where the opponent decides); it changed little at these budgets. **The root value tracks the point margin (r = 0.92), but the margin predicts the outcome only 45–47% of the time**: about 40% of games end by flatline or deck-out, which the margin does not see. No value target teaches the head much until self-play positions decide games. In the arena the two trainers were indistinguishable (0.219 each).
7. **Second 2,400-game run: nine iterations, died at iteration 10** (`fix/training-loop-pins-its-engine`): scores 0.23–0.43, mean 0.31, no trend. **Nothing was ever promoted, so every iteration was uniform-search data** — 24,000 games asking whether volume alone beats the search that generated it; it does not. The value head's apparent gain was the baseline moving: **stalled games were 0.8% of games and 24% of recorded decisions**. The loop shelled out to `cargo run` per stage while *Elevation* landed in the same tree: the deck pool went 12 → 20 → 36 matchups mid-window, `set_rank` reindexed the Core Set, then a half-edit failed to compile. **Fixes:** a run builds once and pins its binary (`<ckpt-dir>/bin/`, sha and commit in `iterations.log`, a `"failed"` line with the resume command); `netrunner_core::pool_fingerprint()` recorded per trajectory, mixed corpora refused; `set_rank` orders `sg, core, elev`; the trainer drops `stall_*` games. Checkpoints and logs under `data/runs/mixed_pool_v2/`; the corpus is deleted.
8. **Third run (`-g 1200 -s 128`), stopped at nine**: 8 of 8 refused, aggregate 141–233–10, 0.380 ± 0.049 (≈ −85 Elo). **Ablation** (`netrunner_bots::SplitEvaluator`, `--candidate-uses both|value-only|priors-only`) on `rejected_iter_008.onnx` at n=192: both 0.3255, value-only 0.4141, priors-only 0.1875 — read at the time as "the priors are the harmful half" (z = 5.1). *Superseded by the chair split in 13.*
9. **A hand is a multiset** (`fix/action-space-aliases`): three copies of Hedge Fund were three identical legal actions, six mask bits and 3× the uniform prior mass **in every search this repo had run**. `legal_actions` deduplicates; `get_action_mask` is built forwards through `index_of`. 162 → 527 decisions/s. Checkpoints stay structurally valid but were fit against the aliased mask.
10. **The cycle guard strikes the set** (`fix/cycle-escape-strikes-the-set`): `CycleGuard`, `MAX_CYCLE_WIDTH` 2, a three-rung escape. **Corrected 5 September: it cannot see the real 4–5-wide toggle livelock and never fired on it** (16 removed the cycle instead). No stall reduction claimed.
11. **Both heads asked from the side to move** (`fix/policy-head-perspective`): the trainer signs the value target by `active_side` and self-play encodes from the awaiting side, so an off-turn observation is out of distribution for *both* heads. One forward pass from `current_actor`, value negated when it is not the agent's side. **Nothing moved** (priors-only 0.1875 → 0.1745, both 0.3255 → 0.2865, value-only 0.4141 → 0.4323; all p > 0.3). Merged as correct-by-distribution; 34.6% of evaluator calls are at opponent nodes.
12. **The policy head diagnosed offline** (`scripts/diagnose_policy_head.py`, 47,327 held-out steps, no `onnxruntime` needed): top-1 agreement 44.4% against 30.1% uniform; 58.8% of mass on legal slots (the unmasked objective is waste, not the story); **peak prior 0.452 against the search's 0.349** — overconfident, starving the right move (median prior 0.110 when wrong) with **no root Dirichlet noise** in `puct.rs` to recover it. **The ε sweep** (`MixedPriorEvaluator`, `--candidate-prior-mix`; priors-only, n=192): 0.1745 / 0.1771 / 0.2031 / 0.2813 / **0.5755** at ε = 0 / .25 / .5 / .75 / 1 — monotone, no sweet spot. **The ε = 1 control should have scored exactly 0.5**: the chair confound.
13. **The arena plays both chairs** (`fix/arena-chair-confound`): the matchup index was `game % 192` and the chair `game % 2`, and 12 is even, so the candidate played Corp only against even-indexed Runner decks — the "both sides" doc comment had been false since the pool outgrew 12 matchups. Now `game_index / 2` with the seed per pair; a null candidate scores **exactly** 0.5 (the test asserts `==`); `ArenaSummary { as_corp, as_runner }`. **The chair baseline is Corp 0.742 / Runner 0.258** — about 180 Elo before anyone plays. Re-measured over 384 games:

    | uses | overall | as Corp (vs 0.742) | as Runner (vs 0.258) | draws C / R |
    |---|---|---|---|---|
    | both | 0.2878 | 0.232 (**−0.510**) | 0.344 (+0.086) | 25 / 4 |
    | priors-only | 0.1589 | 0.076 (−0.667) | 0.242 (−0.016) | 1 / 1 |
    | value-only | 0.3841 | 0.438 (−0.305) | 0.331 (+0.073) | 44 / 17 |

    **The network was broken as the Corp and neutral-to-good as the Runner.** Every earlier "the priors are harmful" verdict averaged two chairs that behave nothing alike. The Corp's failure mode was failing to close — the draws.
14. **Where the Corp's mass goes** (`diag/policy-head-by-segment`; per-`ActionSpace`-segment model / search ratios): `SCORE AGENDA` **0.31** (legal on 430 of 24,548 Corp steps, where the search puts two thirds of its visits), `activate ability` 0.31, `rez ice` 0.49, `install` 0.82, against `pass priority` **1.73**, `draw` 1.62, `gain credit` 1.56. Passivity: mass leaves the wide segments for the narrow ones. The Runner's dominant segments (`basic action` 1.06, `initiate run` 1.10) are untouched, which is why that chair held up.
15. **Masking the policy objective** (`train_alpha_netrunner.py --masked-policy`, default off; `--segment-balance` also exists and is mildly harmful): the softmax renormalised over the target's support. A baseline retrain is **sha-identical** to the shipped checkpoint — training is deterministic given window, seed and hyperparameters. Every segment lands within 2% of the search (`SCORE AGENDA` 0.98, `pass priority` 0.99); peak prior 0.452 → 0.336; top-1 47.4%. Hazard: `0 * -inf = NaN` in the reported loss trained correctly but saved no checkpoint — guard with `torch.where`. The mask is the target's support, not the true legal mask.
16. **"Stalls" were livelocks inside card-selection prompts** (`fix/prompt-livelock-g0`, `fix/session-names-the-livelocked-card`, 5 September 2026). The longest legitimate game in 10,800 recorded was **1,992** actions; every stall was 98.7% `ToggleCardSelection` by one side over 3–5 slots inside a `min == max` prompt — deselect free and reversible, Confirm legal only at exactly `max` and taken once in 9,888. **G0** proved every exact-count subset resolves (41 cases: Plutus 3/3, Anoetic Void 2/2, Sprint 2/2) — a chooser pathology, so no rule changed. **A — bots never deselect** (`agent::{is_regressive, progressive}` at every agent's root and tree): every prompt resolves in at most `max + 1` actions; `ToggleCardSelection` 3,679 → 1,144 over 192 games. **B — `StallReason::DecisionLivelock { side, source_card, actions }`** fires at `DECISION_BUDGET` 256 consecutive actions inside one decision; self-play records `stall_livelock:<card>`, coverage keys `Stalled/DecisionLivelock/<card>`, **the arena scores it as a loss**. **C — prompt cost per card** (`PendingCardSelectionOffered.source`, `CardCoverage::{prompts_offered, prompt_actions, prompt_actions_max}`, `PROMPT_ACTIONS_GATE = 32`; the worst observed is now 4). **D — `MAX_STEPS` 10,000 → 2,500** (max observed 1,992; 1,401 per-seating) and both sweeps stopped tolerating `BudgetExhausted`. **Attribution** (`fix/prompt-attribution-and-cost`): a `then` after a selection acts *as the selected card* (load-bearing for Plutus and Seamless Launch), so parked decisions carry a separate `prompting_card` from `ResolutionContext::prompting_card`; AU Co.'s nested prompt no longer reads as `measured_response`. Rules-neutral by identical report counts. Old stalled seeds cannot be replayed on today's engine — pin the binary.
17. **The masked model on the fixed arena** (5 September 2026; `bin_arena` — built from `4e94665` — and `masked_iter008.onnx` pinned by sha, kept with the three result files under `data/checkpoints/masked_arena/`). The null control is exactly 0.5 with **zero draws**; the chair baseline on this binary is **Corp 0.724 / Runner 0.276**.

    | leg | overall | as Corp (vs 0.724) | as Runner (vs 0.276) | wall |
    |---|---|---|---|---|
    | both | 0.4089 (157–227–0) | 0.536 (**−0.188**; was −0.510) | 0.281 (+0.005) | 630s (was 6,246s) |
    | value-only | 0.3828 (147–237–0) | 0.469 (**−0.255**) | 0.297 (+0.021) | 619s (was 7,945s) |

    The 25 Corp "draws" were won games the network could not finish. **The value head carries the Corp deficit** (0.469 against the 0.60 decision line; the masked priors add back about +0.07 as Corp). The Runner is neutral in every leg. The old value-only figure matched by coincidence — 61 livelock half-points offset the unfinished games.

18. **The observation shows the value head the board** (`feat/observation-rework`, 5 September 2026). `OBS_SIZE` **990 → 2,262**. The old encoding was 30 scalars and five card-identity count planes: it saw *how many* installs and *which* cards, but not one advancement token, which install was rezzed, the run's target or phase, the encountered ICE's strength or pending subroutines, or what decision was parked — everything `eval::evaluate_state_with` reads and the Corp's game is about. New blocks, each read off `ClientView` only (printed values through a card id the view shows, so an unrezzed install still masks): 16 more scalars (turn, actions this turn, recurring credits, identity counters, agenda counters, servers run this turn); a **Corp install block of 32 slots × 23 indexed by `PublicInstalledCard::position`** — the same index the `AdvanceCard`/`ScoreAgenda`/`RezIce`/`TrashResource` segments use, so "advance slot 3" in the policy target and "slot 3 is an agenda at two of three tokens" in the observation are one index (pinned by `install_slots_match_the_action_space`; a per-server grouping was the alternative and would have left the policy head to learn the mapping); a rig block of 32 × 12 with `eval::covers`; a breaker-coverage summary; a run block (target, phase, 8 ICE slots with strength/subtype/pending-broken-resolved, the access state); and a decision block (which parked state, whose by `current_actor`'s precedence, selection progress). The five planes are unchanged, so the vocabulary tests stand. The rejected `search128_v3` run and its `masked_arena` measurement moved to `data/runs/search128_v3/`; its 2.8 GB width-990 corpus is deleted.

    **Smoke on a pinned binary** (24 games, 32 simulations, 2 epochs, 8-game arena, scratch directories): self-play recorded `observation_size` 2,262 and the trainer built `obs_dim` from it; **mean nonzero entries per step 29 → 105.9** (the sparse format's cost); self-play 4.5 s, training 3.2 s on CUDA, the whole iteration 22 s; the exported ONNX loaded in the pinned binary and the **null control scored exactly 0.5 with zero draws** (Corp 4–0, Runner 0–4: the chair baseline, unchanged). The toy candidate scored 0.0 — two epochs on 24 games is not a claim. `diagnose_policy_head.py` now reads the width from the games rather than a literal 990.

19. **Loop hygiene** (`feat/loop-hygiene`, 5 September 2026; `scripts/run_iteration_loop.py` only). Three defaults changed, each the fix for a way the earlier runs misread themselves. **The trainer is called with `--masked-policy`** unless `--unmasked-policy` — the loop's default flips while the trainer's own stays unmasked, because the trainer's default is what reproduces the recorded runs byte for byte and a new run has no reason to train the objective that put the Corp's prior mass on `pass priority` (items 14–15). **`--arena-games` 48 → 384**: the 48-game verdicts swung 0.22–0.48 with no trend, a 24-game chair against a 0.72/0.28 baseline; 384 is what items 13 and 17 measured at and about a tenth of an iteration's self-play at 1,200 games. **The verdict line carries both chairs** — `corp=W-L-D (score) runner=W-L-D (score)` in `promotions.log` and on the console — because the blend hid a Corp broken at 0.23 behind a Runner at 0.34 for three runs (item 13). Checked on a scratch iteration: the trainer reports "masked to the target support", and a null-shaped 8-game arena prints `corp=4-0-0 (1.000) runner=0-4-0 (0.000)`, the chair baseline in the open.

20. **The fourth volume run: the observation rework measured** (`data/runs/obs_rework_v4/`, 6 September 2026). Sixteen iterations at 1,200 games and 128 simulations, `--window 4`, the loop's new defaults from item 19, on a binary pinned to `0febac4` (sha `4255ced0…`, clean tree). 19,200 self-play games, 6,144 arena games, 9.95 h. **Every iteration was refused**; the last export is `rejected_iter_016.onnx` (byte-identical to `netrunner_policy.onnx`, sha `9378781…`).

    | | mean | sd | range | first 4 → last 4 |
    |---|---|---|---|---|
    | arena overall | 0.399 | 0.031 | 0.312–0.456 | 0.375 → 0.413 |
    | as Corp | 0.456 | 0.051 | 0.333–0.542 | 0.393 → 0.469 |
    | as Runner | 0.341 | 0.039 | 0.276–0.391 | 0.357 → 0.358 |
    | value MSE vs outcome (predict-zero 1.000) | 0.926 | 0.033 | 0.869–0.980 | 0.925 → 0.925 |

    **The observation rework did not move the value head, and the run's stated question was already stale.** The launch script asked whether `mse_vs_outcome` beats predict-zero; it does, in all sixteen iterations — but **it already did on the 990-wide observation**, and by slightly more. Read off the archived `iterations.log` files:

    | run | observation | MSE vs outcome (mean, range) | sign accuracy |
    |---|---|---|---|
    | `mixed_pool_v2` (item 7) | 990 | 0.856 (0.737–0.993) | 61.4% |
    | `search128_v3` (item 8) | 990 | 0.910 (0.863–0.958) | 64.7% |
    | `obs_rework_v4` (this run) | **2,262** | 0.926 (0.869–0.980) | 65.4% |

    Item 3's "the value head never beat predicting zero" (1.04–1.30) was true of the 2,400-game scaling study and has not been true since run 2; **nobody re-read the later runs' own logs before framing this one**, so a question with an answer already on disk cost ten hours. The honest reading of the three rows is a wash — 0.910 → 0.926 and 64.7% → 65.4% move opposite ways, both inside the per-iteration spread. Confounded in run 4's favour, at that: run 3 dropped 15–98 livelocked games an iteration (run 4 drops 4 in total) and trained an unmasked policy objective. **Widening the observation 990 → 2,262 bought no measurable value-head accuracy.**

    **And it bought nothing in the arena.** The Corp chair sat at 0.456 mean and crossed 0.5 twice in sixteen tries; the trend across iterations is flat (first four 0.375, last four 0.413, inside the 0.031 spread). The run's other question — "does `as_corp` move off 0.47" — is answered no. Item 18 is not thereby wrong (the old encoding could not see an advancement token, and the policy head's segment indices now line up with the observation's install slots), but **it is not the reason the Corp loses**, and the next change must come from somewhere else.

    **The chair baseline on this engine, and where the deficit sits** (`data/runs/obs_rework_v4/obs_arena/`, four 384-game legs on the run's own binary against `rejected_iter_016.onnx`; `run.sh` beside the results). The null control is **exactly 0.5 with zero draws**, and the chair baseline is **Corp 0.693 / Runner 0.307** — three points less lopsided than item 17's 0.724 / 0.276 on binary `4e94665`, which is the Corp pressure terms and format work since then, and why a deficit quoted against 0.724 would have been a comparison across engines.

    | leg | overall | as Corp (vs 0.693) | as Runner (vs 0.307) | wall |
    |---|---|---|---|---|
    | null | 0.5000 | 0.693 | 0.307 | 1,320s |
    | both | 0.4557 | 0.531 (**−0.162**) | 0.380 (+0.073) | 1,120s |
    | value-only | 0.3646 | 0.401 (**−0.292**) | 0.328 (+0.021) | 1,297s |
    | priors-only | 0.4766 | 0.672 (−0.021) | 0.281 (−0.026) | 1,203s |

    **The priors are neutral on both chairs now** (−0.02 / −0.03 — item 15's masked objective did what it claimed; compare item 13's priors-only Corp at −0.667). **The value head alone loses the Corp chair by 0.29**, and the priors claw a third of that back when both are seated. Item 17 measured value-only at −0.255 on the old observation; on the new one it is −0.292 on a slightly fairer chair — the wider observation did not help the value head *in search* any more than it helped it on the held-out split. The `both` leg reproduced iteration 16's own verdict byte for byte (175–209, 0.531 / 0.380), so the arena is deterministic on a pinned binary. **The null leg is not cheaper than a real one**: the network is still evaluated and then discarded, 1,320 s against 1,120 s.

    **Three facts about the loop, not the network.** (a) **Nothing was promoted, so `--window 4` saturates at 4,800 games from iteration 4 and iterations 4–16 are thirteen redraws of one distribution** — the same trap as item 7, and about seven of the ten hours re-answered what iteration 4 had settled. Four runs have now asked whether more uniform-search data beats uniform search. (b) **The arena is now the dominant cost**: 5.00 h against 2.42 h of self-play and 2.54 h of training, because item 19 raised it 48 → 384 games and it is a network-in-the-loop leg at ~2 ms an evaluation. A 384-game verdict on an iteration that cannot be promoted is the most expensive thing in the run. (c) **The trainer's best epoch was 1 or 2 in nine of sixteen iterations** and never later than 8; ten epochs on a saturated 4,800-game window is mostly overfitting past the checkpoint that gets exported.

    **The livelock work holds under volume** (item 16): **zero draws in all 6,144 arena games** and 4 dropped stall games in 19,200 (one each in iterations 4–7). Self-play stayed at the chair baseline throughout — 3,223 Corp wins to 1,577 Runner in the final window, 67%.

    Not comparable to earlier runs: the policy loss over the entropy floor (+0.088 to +0.108 nats) is a *masked* objective's number and the 1.06–1.66 nats of item 3 are an unmasked one. The 5.9 GB corpus is deleted; checkpoints, logs and the pinned binary are kept.

21. **What the fifth run should change: the network never got the search fix** (`feat/value-target-screen`, 7 September 2026). Item 20 named the value *target* as the next thing to change. Before generating a game, three of its premises were re-checked against HEAD, and two of them are gone.

    **A field changed meaning under one name.** `SelfPlayStep::search_value` is `PuctSearchStats::root_value`. `b251258` made the *static* evaluator's leaves root-relative (`policy.rs`, `tanh((leaf − root) / 5)`) — the change that took the PUCT Runner from 0.219 to 0.411 — while a network's evaluator inherits the default `evaluate_from` and still reports absolute values. So a uniform-search corpus records a mean *advantage* and an ONNX-driven one a mean *position value*, with nothing on disk to tell them apart, and the loop's `--value-target-mix 0.5` blends whichever it gets with the game's outcome. Fixed by recording both: `PuctSearchStats::root_value_absolute` (from `PolicyEvaluator::evaluate`, the one call that is absolute for every evaluator kind) and `SelfPlayStep::search_value_absolute`, `#[serde(default)]` so archived corpora load, with the trainer refusing a nonzero mix against a corpus that lacks it. Measured on the new corpus, the two are distinct quantities — relative mean 0.121 sd 0.301, absolute mean 0.106 sd **0.411**, median |x| 0.164 against 0.318. (An earlier reading of this entry said the relative value was "≈0" and that a 0.5 mix would halve the target; that was wrong — it has real spread, because `root_value` averages backed-up values including terminal ±1. It is the wrong *kind* of quantity, not a degenerate one.)

    **The chair baseline moved, so item 20's deficits cannot be quoted on HEAD.** The null leg on a pinned HEAD binary is exactly 0.5 with zero draws, at **Corp 0.635 / Runner 0.365** against item 20's 0.693 / 0.307 on `0febac4` — the Runner run term and the deck-aware determinization landing in between. Re-baseline per binary; the rule was already written into `obs_arena/run.sh` and is now demonstrated.

    **The deficit is a regime mismatch, not the value head.** Five legs on `rejected_iter_016.onnx`, 384 games each, same binary:

    | leg | overall | as Corp (baseline 0.635) | as Runner (0.365) |
    |---|---|---|---|
    | null | 0.500 | 0.635 | 0.365 |
    | **priors-only** | **0.430** | **0.594** | 0.266 |
    | value-only | 0.135 | 0.089 | 0.182 |
    | value-only, FPU = parent Q | 0.120 | 0.057 | 0.182 |
    | value-only, anchor unscaled | 0.122 | 0.073 | 0.172 |
    | value-only, anchor / 0.05 – 0.1 – 0.2 | 0.177 / 0.164 / 0.174 | 0.130 / 0.104 / 0.156 | 0.224 / 0.224 / 0.193 |
    | both | 0.138 | 0.078 | 0.198 |

    `priors-only` takes its value from the *uniform* evaluator and so still gets root-relative leaves; every leg seating the **network's** value runs the absolute regime its opponent has left behind. The separation is 0.430 against 0.135 — about 113 games of 384, ~12σ — and it is why `both` (0.138) collapsed to its `value-only` floor on HEAD where on `0febac4` the priors clawed back a third (0.456 against 0.365). **Item 20's "the value head is the Corp's whole deficit" was sound on a binary where both sides used absolute leaves and does not transfer.** Giving `OnnxPolicyEvaluator` a rescaled anchor is worth a consistent **+0.035** across scales 0.05–0.2 (each ~1.4σ alone; three independent scales landing together is the signal) — real, and a seventh of the gap. First-play urgency at the parent's Q, and an anchor without the rescale, both measured **nothing** (0.5–1.1σ, indistinguishable); the unscaled anchor failed for a nameable reason, that subtracting two already-squashed [−1, 1] outputs shrinks a within-decision gap to ~0.01–0.05 against an exploration term of ~0.13, where the uniform evaluator's divisor *expands* it.

    **Two of item 20's premises do not hold on HEAD**, measured over the 2,400-game corpus (uniform search both seats, 128 simulations, zero stalls): games end `agenda_threshold` 2,093 / `flatline` 301 / `deckout` 6, so **flatline-or-deckout is 12.8%, not the ~40%** item 20 argues from — that figure came from `puct-baseline.json`, puct@32 on an older engine — and self-play is **55.3% Corp, not 67%**. The 40% was the stated reason to suspect the target ("which no board feature and no point margin sees"); at 12.8% that argument is much weaker, and it is the whole motivation for an end-reason-aware target.

    **The honest null is the chair, not zero** (`value_diagnostics` now reports `chair_baseline_mse` beside `baseline_mse`). Predicting zero is the null for a *symmetric* game; a predictor knowing only which chair is to move scores `1 − (2p−1)²` per chair. On run 4's own logs that null is **0.882 and the head averaged 0.926 — it beat the chair in 2 iterations of 16.** Item 20's "the value head already beat predicting zero two runs ago" was measured against the easy null.

    **A control trained on the new corpus** (pure outcome, 6 epochs, `--select-on value`): best epoch **1**, held-out MSE **1.241** against chair-only 0.910 and predict-zero 1.000, sign 63.3%, train value loss 0.269 → 0.036 while validation climbed monotonically. It reproduces the failure this file already records for the outcome-alone target, and shows the mix in run 4 (0.926) was doing real smoothing rather than decoration.

    **Engineering landed on the branch**, all of it measured above or guarding it: the two root values and the schema field; `--value-target {outcome,mixed,discounted,discounted_unforeseeable}` with `--value-discount`, built at the single `set_value_target` seam and always diagnosed against the unchanged signed outcome; `--select-on {blended,value,policy}`, because at `--value-loss-weight 0.25` the exported epoch is chosen almost entirely by the policy term and a value-target change need not move it; `--early-stop-patience`; per-chair prediction means and sign accuracy split by end reason; **`--arena-pair-stride`**, because `matchups()` is corp-major and 48 pairs at stride 1 are the first *four* Corp decks — a short arena at stride 1 is a narrower arena, not a smaller one, which retro-explains the first three runs' 48-game verdicts swinging 0.22–0.48 with no trend; and a two-stage arena in the loop (a stride-4 screen, the full 384 only when it passes, promotion still decided by the full arena). **`OnnxPolicyEvaluator::anchor`/`evaluate_from` are implemented and `ANCHOR_SCALE` is `None`** — left in with its numbers on it the way `BREACH_OUTCOMES` is, because every measurement above is on a checkpoint fit to a 67%-Corp engine and so cannot separate the regime from the staleness; turn it on when a checkpoint trained on *this* engine says so. The FPU experiment is deleted rather than parked: it measured nothing, and its reasoning is recorded on `select_edge` so the next reader does not re-derive it.

    **Where this leaves the fifth run.** Its first change is not the value target: **the network needs the search fix the static evaluator got**, or it is judged in a regime measured to be broken and any target measured through it is measured through a broken instrument. The value-target screen — one corpus, one candidate per target, each on the same value-only leg — runs behind that. **Both halves of this paragraph are now answered, and both answers are no: item 22 ran the screen and neither the regime nor the staleness was the reason.**

**Decision and next step:** item 20 answered the fourth run's question and unasked it — the value head already beat predicting zero two runs ago, the wider observation did not improve it, and the Corp chair did not move — so **the input is ruled out and the next change is not another 1,200-game run of the same shape.** The legs separate what the run could not: **the priors are fixed and the value head is the Corp's whole deficit** (−0.29 alone, priors neutral). So the live suspect is **what the value head is trained toward**, not what it is shown: about 40% of games end by flatline or deck-out and neither a board feature nor the point margin sees that coming (item 6 measured the margin at 45–47% predictive), and half its target is the uniform search's own root belief. **The bootstrap** is the other open question — four runs have trained on nothing but uniform-search data because 0.55 against uniform search has never been reached — but with priors-only scoring 0.48 the priors are already almost the search they were distilled from, and a loop that adopts them unconditionally would be measuring whether self-play with a neutral prior and a harmful value drifts up or down. Either way the loop's economics change first: a 384-game arena on an iteration that cannot be promoted cost 5.00 h of 9.95 h (item 20), and ten epochs on a window that never turns over is overfitting past the exported checkpoint.

22. **The anchor screen: the fifth run's blocking question, sized at 2.5 hours** (`chore/fifth-run-anchor-screen`, 8 September 2026). Item 21 left `ANCHOR_SCALE` at `None` with its numbers on it, because the +0.035 a rescaled anchor is worth was measured on `rejected_iter_016.onnx` — a checkpoint fit to a 67%-Corp engine — and so cannot separate the regime from the staleness. **What is missing is a checkpoint trained on this engine to screen it against, not another volume run**, so `data/checkpoints/launch_overnight.sh` (still the fourth run's until now) is rewritten as a screen: one iteration at 2,400 games and 128 simulations, then six 384-game arena legs on two pinned binaries differing only in `ANCHOR_SCALE` (`None`, `Some(0.1)`) — `null`, `priors-only`, and `value-only`/`both` from each. Scales 0.05 and 0.2 are added only if 0.1 moves; three independent scales landing together was the signal on the stale checkpoint, where one scale alone was ~1.4σ.

    **The corpus is the fifth run's iteration 1, not a throwaway.** Self-play here seats the uniform evaluator (no `-m`) and `ANCHOR_SCALE` only ever touches `OnnxPolicyEvaluator`, so the corpus is byte-identical under either binary and the volume run resumes at `--start-iter 2`.

    **Two of item 20's three economics findings are closed here, and both were flags rather than code.** `--window 4` is *dropped* — the default is every iteration, and with nothing promoted a window of 4 saturates at iteration 4, which made iterations 4–16 of the fourth run thirteen redraws of one distribution and cost about seven of its ten hours. `--early-stop-patience 2` is the other half: the best epoch was 1 or 2 in nine of sixteen iterations and never later than 8, so ten epochs on a window that never turns over is overfitting past the exported checkpoint. The third, the arena's cost, item 21 had already closed with the two-stage screen.

    **The null leg is the gate on every number the screen produces** and it is in the script for the reason `obs_arena/run.sh` first wrote down: the candidate is the network with its priors replaced by the uniform search's own, so it *is* the incumbent and must score exactly 0.5 with zero draws. It also re-establishes the chair baseline on this binary — 0.635 / 0.365 on HEAD against 0.693 / 0.307 on `0febac4` — which is why item 20's deficits cannot be quoted here.

    **The question was checked against disk before the run was written**, which is now part of launching one: item 20 cost ten hours re-answering a question its own earlier `iterations.log` files had settled. Every `value-only` number on disk is from the stale checkpoint, and none of them answers this.

    **The screen ran (8 September 2026, engine `a7ec188`, checkpoint sha `1b743cb5…`, 2,400 games in 41 min, six legs in 1.9 h) and answered its question no — then made the question obsolete.** The null leg gates everything below it and passed exactly: 0.500 with **zero draws**, at Corp 0.635 / Runner 0.365, reproducing item 21's HEAD chair baseline to four places.

    | leg | overall | as Corp (base 0.635) | as Runner (base 0.365) |
    |---|---|---|---|
    | null | 0.500 | 0.635 | 0.365 |
    | **priors-only** | **0.617** | **0.766** | **0.469** |
    | value-only, anchor off | 0.141 | 0.109 | 0.172 |
    | value-only, anchor `Some(0.1)` | 0.148 | 0.125 | 0.172 |
    | both, anchor off | 0.359 | 0.318 | 0.401 |
    | both, anchor `Some(0.1)` | 0.258 | 0.224 | 0.292 |

    **`ANCHOR_SCALE` stays `None`, and now for a stronger reason than "did not reproduce".** Item 21 measured the rescaled anchor at +0.035 on the stale checkpoint and could not separate the regime from the staleness. On a checkpoint trained on this engine it is worth **+0.008** in `value-only` (0.3σ at 384 games — nothing) and **−0.101 in `both`**, the configuration promotion is actually decided in, which is about 3.9σ and a real harm. The two pinned binaries differ (sha `89cf7d72…` against `b631e70d…`), so this is not the compare-two-copies-of-the-same-code trap. The constant keeps its second set of numbers.

    **And staleness was not the explanation either, which was the whole point of the screen.** This head is much better calibrated than `rejected_iter_016.onnx`: held-out MSE **0.762 against a chair null of 0.911** (predict-zero 1.000) and sign accuracy 71.1%, where run 4's head averaged 0.926 against a chair null of 0.882 — it *lost* to the chair and beat it in 2 iterations of 16. Against its own null this head went from −0.044 to +0.149. In the search it scores **0.141 against the stale checkpoint's 0.135**. A markedly better value head is worth nothing in search, so the collapse is neither its accuracy nor the leaf regime it is judged in. **That is a new question and it was not on item 21's list.**

    **A third premise of item 20's value-target argument is also gone.** That argument was that ~40% of games end by flatline or deck-out and no board feature sees them coming; item 21 cut the 40% to 12.8%. This head's sign accuracy splits **0.835 on `unforeseeable` endings against 0.697 on `foreseeable`** — it is *better* on exactly the games the target was to be reshaped for. The end-reason-aware target should not be built on this reasoning.

    **The finding that redirects the fifth run is the priors.** `priors-only` scores **0.617 with both chairs above baseline** (Corp 0.766, Runner 0.469), against item 21's 0.430 on the stale checkpoint and item 13's −0.667. **This is the first time in five runs that any network component has beaten the search it was distilled from**, and it clears the 0.55 promotion threshold on its own. What sinks it is the value head: seating both drops 0.617 → 0.359. The corpus is the only thing that changed — 2,400 fresh games from this engine, masked objective, unsaturated window, `--early-stop-patience 2` stopping at epoch 4 on best epoch 2.

    **So the fifth run's shape is not item 21's.** It should seat the network's priors and keep its value head out of the search, which today is not expressible: `SplitEvaluator` and the `ablate` seam exist only on the **arena** path (`netrunner_selfplay/src/main.rs:322`), while self-play's `make_evaluator` (line 368, used at 476/478) seats the whole network whenever `-m` is passed. The self-play path needs the same ablation the arena already has, and `run_iteration_loop.py` needs to gate promotion on the configuration self-play actually used. That is the next engineering, and it is small. The corpus stays on disk as iteration 1.

23. **Self-play can seat half a network** (`feat/priors-only-self-play`, 8 September 2026). Item 22's finding was not actionable: `SplitEvaluator` and the `ablate` seam existed only on the **arena** path, while self-play's `make_evaluator` seated the whole network whenever `-m` was passed, so the configuration measured at 0.617 could not generate a single game.

    **`--model-uses {both,priors-only,value-only}`** on `netrunner_selfplay`, defaulting to `both`, reusing the same `ablate` the arena calls (the enum is renamed `NetworkUses`, since it is no longer arena-only). Both seats are ablated identically — the difference from the arena, where the asymmetry is the point because the incumbent is the bar; here the ablation *is* the player being measured, so a corpus comes from one configuration rather than two. **The default path is byte-identical to the pre-change binary across six games** (`ablate(Both)` returns the evaluator untouched), verified against the pinned screen binary rather than asserted.

    **`GameTrajectory::model_uses`** records it, `#[serde(default)]` like `pool_fingerprint` and `end_reason` beside it, for the same reason those exist: a priors-only corpus and a whole-network one are different distributions from the same binary at the same widths, and nothing else on the struct tells them apart — mixing them in one replay window would be invisible the way three *Elevation* deck pools once were. The four archived runs read empty, which is honest; they predate the flag and every one was `both`.

    **`run_iteration_loop.py --model-uses` drives both ends**: self-play seats it, and the arena gates the *candidate* with the same ablation. Promotion measuring a configuration the run never plays would be the same class of error as item 13's blended verdict hiding a broken chair, so `iterations.log` carries `model_uses` on every line.

    **Two guards, both because the failure mode is silence.** Ablating with no `-m` is refused outright — `SplitEvaluator(uniform, uniform)` is just the uniform evaluator, so the run would spend a full iteration producing an ordinary corpus under a label saying otherwise. And `main` now prints the error's *message* rather than its `Debug`: `fn main() -> Result<_, _>` reports with `{:?}`, so every `SelfPlayError` message in this binary had been reaching an operator as a bare variant name (`ModelUsesWithoutModel("priors-only")` in place of the sentence explaining it). A guard whose explanation never prints is not a guard.

    `launch_overnight.sh` is the fifth run: `--model-uses priors-only`, `--start-iter 2` over the screen's corpus, 12 iterations. **Its question is whether a priors-only loop compounds** — one iteration of it scores 0.617, and no run has ever had a promotion, so nobody knows whether iteration 2 trains a stronger prior still or plateaus at once. `--value-loss-weight` stays 0.25 deliberately: the value head is not seated in the search but keeps training, because its diagnostics are the only running measurement of the question item 22 opened and could not answer.

24. **The promotion gate was about to measure the configuration gap, not the candidate** (`fix/incumbent-ablated-like-the-run`, 8 September 2026). Caught in the pre-launch check of the fifth run, before a single game of it ran. Item 23 gave self-play `--model-uses`, but `ablate` was still applied to the **candidate only** — right for the diagnostic the arena was built for, where the incumbent is a fixed bar and moving it would make two legs incomparable, and wrong the moment the ablation became a *run's configuration*. From iteration 2 the incumbent is `latest_policy.onnx`, which self-play deploys priors-only and the arena would have seated whole.

    **Measured, same checkpoint on both sides of the table, 48 games:** candidate priors-only against that incumbent seated whole scores **0.583 — over the 0.55 gate — having improved nothing**; seated alike it is exactly **0.500**. So iteration 2 would have promoted an unchanged network on the 0.617-against-0.359 configuration gap, and every iteration after it would have inherited that promotion. The failure mode item 13 records (promoting unconditionally, six iterations of self-play turned into "the Runner wins") arriving by a new route.

    **`--incumbent-uses`**, defaulting to `both` so the diagnostic ablation keeps its fixed bar, set by `run_iteration_loop.py` to match `--model-uses` so a run gates on what it plays. `ArenaSummary::incumbent_uses` is on every verdict line beside `candidate_uses`, because the pair is what makes one readable: `candidate_uses` alone cannot tell "this candidate is better" from "this candidate was seated in a stronger configuration than the incumbent it beat".

    **The general lesson, and it is the reason this entry exists rather than a quiet fix.** A knob built as a diagnostic acquired a second life as a configuration, and its one-sided-by-design behaviour silently became a bias. The check that caught it is the cheapest one available and belongs before every run: **seat the same checkpoint on both sides and confirm the arena says 0.500.** It is the null leg `obs_arena/run.sh` already demanded for a model's own arena, applied to the flags instead of the network.

25. **A network-in-the-loop iteration costs 4.2 h, and the reason is `batch = 1`** (`fix/onnx-sessions-oversubscribe-the-machine`, 8 September 2026). The fifth run launched and its iteration 2 ran at **6.25 s/game against iteration 1's 0.38** — the same pinned binary, the same 2,400 games, the same 128 simulations, the only difference being a network seated. That is **17×**, not the ~6× the standing open item had recorded, and it put the 12-iteration run at ~55 h rather than an overnight.

    **The first explanation was wrong, and it is recorded because it was expensive to believe.** The self-play process carried **541 threads on 20 cores at load average 53**: every consumer builds one `Session` per side per game and runs games across a rayon pool, while ORT's default intra-op pool is sized to the whole machine and spin-waits. Nested pools oversubscribing a box is a real pathology and it looked like this one. Measured on an idle machine, 40 games at 128 simulations, two binaries pinned side by side: **238.5 s with the default pools against 235.5 s with one thread per session** — 1.2%, inside noise. The corpora came back **byte-identical game for game**, which settles the risk that actually mattered: ORT can reorder floating-point reductions when thread count changes, and here it does not.

    `with_intra_threads(1)` is kept anyway for the smaller reasons — 9% of resident memory (411 MB → 375 MB), and a session whose cost does not change shape with the host's core count, since the default *would* be pathological on a 96-core machine. Rejected: reverting it, which keeps the machine-dependent shape and loses the measurement.

    **The cost is arithmetic, and nothing about the loop's threading can reach it.** A decision is ~128 forward passes, a game ~205 decisions (492,638 steps over 2,400 games), and each pass re-reads all **1,151,471 parameters** (trunk 2262→256→256, policy head → 1,646): **~121 GB of weight traffic per game for 2.3 MFLOPs of arithmetic per pass**, which is memory-bandwidth bound. Per-call cost is **3.85 ms**, not the ~2 ms recorded. Scaling agrees — 32 games at 32 / 64 / 128 simulations cost **2.0 / 3.7 / 5.96 s/game**. There is also **no tree reuse to exploit**: `puct.rs:520` builds a fresh tree per determinization per decision, which is correct rather than an oversight, because a tree built on an earlier sample of hidden state is not valid for a new one.

    **The open lead is batching the leaves.** *(Corrected by item 27: the four-determinizations premise was wrong — `PuctConfig::samples` is 1 everywhere, so there are no sibling leaves in a search to batch. Batching across concurrent **games** is the route that exists, it needs no search change at all, and it was worth 2.6×. The claim below that this "cannot be threaded away" is also too strong: inference throughput peaks at four concurrent streams and collapses past it.)*

    **The run was reshaped rather than the code:** 1,200 games per iteration through iteration 7 instead of 2,400 through 12, ~3.0 h per iteration and ~16 h total. Iteration 2 keeps its 2,400-game corpus (already on disk; `run_iteration_loop.py:229` skips self-play when the directory holds its games), so it costs only training and the arena. 128 simulations is deliberately untouched — comparability with item 22's 0.617 is the point of the run. Five promotion decisions after iteration 2 is what "does a priors-only loop compound?" needs; twelve iterations was a budget, not the question.

26. **The fifth run: a priors-only loop trades the Runner seat for the Corp seat, at parity** (8–9 September 2026, stopped at iteration 5 of 7 once the answer stopped changing). Item 22 found the priors alone scoring 0.617 against the uniform search with both chairs above baseline, and this run asked the only question that could follow: **does a priors-only loop compound?** It does not, and the way it fails is the finding.

    **Three candidates, three rejections, and the blend hides what happened.**

    | iter | corpus | screen | full arena | as Corp | as Runner |
    |---|---|---|---|---|---|
    | 2 | 4,800 | 0.469 | 0.461 | 0.688 | 0.234 |
    | 3 | 6,000 | 0.542 | 0.523 | 0.729 | 0.318 |
    | 4 | 7,200 | 0.490 | 0.479 | 0.708 | 0.250 |

    **Pooled over all 1,152 arena games: 0.4878, which is −0.8σ from parity — while the Corp chair is 0.708 (+10.0σ) and the Runner chair 0.267 (−11.2σ).** The candidate is not a weak copy of the network that generated its data; it is violently different in both chairs and the two differences cancel. Every iteration reproduces the shape: about +0.21 as Corp, about −0.23 as Runner, net nothing. Both sides were seated `priors-only` (item 24), so 0.500 is the honest bar and this is priors against priors.

    **Corpus size is not the variable.** 4,800 → 7,200 games moved the blended score 0.461 → 0.523 → 0.479: a per-iteration spread of sd 0.032 against the 0.026 that 384-game sampling noise alone produces. Two of those points briefly looked like a trend toward the 0.55 gate and were read that way in the session; the third showed it was a flat line. This is the same conclusion item 20 reached by a different route, and it is why the run was stopped at iteration 5 rather than played to 7 — iterations 5–7 could only add samples of a quantity already at 0.488 ± 0.015.

    **The mechanism is on the self-play side, and it replicates.** Seating those priors on both seats moves the chair balance about ten points off the engine's own, over four disjoint seed ranges:

    | corpus | seeds | Corp win |
    |---|---|---|
    | iter_001 (uniform search) | 0–2399 | 54.8% |
    | iter_002 (priors-only) | 2400–4799 | 65.7% |
    | iter_003 (priors-only) | 4800–5999 | 64.9% |
    | iter_004 (priors-only) | 6000–7199 | 63.9% |

    A network trained on a corpus where the Corp wins ~65% learns the Corp seat and loses the Runner seat, which is exactly what the arena then measures. **The loop's input and its output are the same distortion seen twice.**

    **Two things the training logs got wrong about all this, both worth keeping.** The policy head's distance from its entropy floor is flat — **+0.124, +0.124, +0.125, +0.115** across 2,400 → 7,200 games — while the arena score moved 0.06 between two of those points, so **that loss gap does not track playing strength and should not be used to predict an arena result.** And the value head's raw MSE improved (0.863 → 0.772 → 0.762) while its edge over the chair null *collapsed* (0.048 → 0.095 → **0.018**), because a more chair-skewed corpus makes "predict the chair average" a better strategy and drags the null down with it. Quote the gap, never the MSE alone.

    **What this hands to the gate.** Promotion is a blended score, so "genuinely stronger" and "traded one chair for the other" are indistinguishable to it — a candidate on this trajectory clears 0.55 the moment its Corp chair runs far enough ahead, and would be promoted with a Runner chair near 0.30. That is item 13's failure (a blended verdict concealing a broken chair) still live in the gate, now with a concrete mechanism that produces it. The open work is a promotion rule that reads both chairs, and a chair-weighted training objective — the trainer has `--segment-balance` for rare `ActionSpace` segments but nothing for seats. The 7,200-game corpus is kept for exactly that screen, which is one arena leg rather than another overnight run.

    Also unanswered and now with a second run's worth of evidence behind it: **why a well-calibrated value head is worth nothing at a leaf in this search** (item 22).

27. **Batching across games: self-play is 2.6× faster and the corpus is byte-identical** (`feat/batched-onnx-evaluation`, 9 September 2026). Item 25 named `batch = 1` as the cost and pointed at the four determinizations in a root as the leaves to batch. **That premise was wrong**: `PuctConfig::samples` is `1` in the default, in every test, and in every caller — its own doc comment says "Left in at `1`" — so `search`'s `into_par_iter` is a one-element loop and a search evaluates strictly one leaf at a time. There are no sibling leaves inside a search.

    **The batching that does exist is across concurrent games**, and it needs no search change: self-play already runs one game per worker, each blocked on its own single-row inference, and rows from different games can share a `Session::run`. **A row's answer does not depend on what shares its batch** — verified bitwise against the real 1,151,471-parameter checkpoint on every one of 2 × 1,646 outputs — so this is invisible to every caller.

    **What the cost actually is, measured rather than inferred.** Item 25's "3.85 ms per forward pass" was per-*thread latency under load*, not the cost of a call; single-threaded a call is 199 µs, and single-threaded self-play is 4.36 s/game uniform against 11.1 s/game with the network, which reconciles at ~26,240 evaluations × 257 µs. The real defect is scaling: **uniform self-play scales 11.4× across ~18 workers, the network path 1.87×**, because inference throughput does not scale at all — 5,029 rows/s at one thread, 5,391 at seventeen. It peaks at **four** concurrent streams (13,022 rows/s) and *falls below the single-threaded rate* beyond that, as more copies of the 4.6 MB weights leave L3.

    **And capping concurrency does not fix it**, which is the measurement that found the real mechanism. A semaphore at four streams left wall time unchanged (237.9 s against 238.5 s) while dropping CPU from 1711% to 662%. The reason is cache residency, not parallelism: interleaving 8 MB of unrelated traffic between calls — what tree work does — collapses batch-1 throughput 9× and makes thread count irrelevant, while batching keeps paying:

    | | batch 1 | batch 4 | batch 8 | batch 16 | batch 32 |
    |---|---|---|---|---|---|
    | 1 runner | 2,565 | 9,643 | 13,099 | 18,005 | 19,072 |
    | 2 runners | 2,565 | 9,984 | 16,779 | 22,677 | 26,197 |
    | 4 runners | 1,753 | 8,203 | 18,171 | 36,683 | 46,229 |

    Batching is the only lever that works because it is the only one that amortizes a weight read over more than one row.

    **The result, on 40 games at 128 simulations, byte-identical corpus throughout:**

    | | wall | CPU | RSS |
    |---|---|---|---|
    | before | 238.5 s | 1711% | 411 MB |
    | batched, default threads | 101.4 s | 894% | 124 MB |
    | **batched, 64 game threads** | **92.5 s** | 919% | 175 MB |

    **2.6×**, and memory fell because ~36 sessions (one per side per game, 165 MB of duplicated weights and 8.0 ms of graph parsing each) became a shared pool of four. `MAX_RUNNERS` is 4 on measurement: 8 runners cost 129 s and 16 cost 218 s, each runner being another live copy of the weights. Game threads are oversubscribed to 64 only when a network is seated, because they park on the queue rather than run, and a deeper queue is what lets a runner fill a batch; the uniform path keeps one thread per core.

    **The byte-identity extends to the arena, measured after the fact** (9 September 2026, during item 29's screen). The equivalence quoted above was 40 self-play games; the chair-balance screen then had a freshly built batched binary replay a 384-game arena that the pre-batching pinned binary had recorded the night before, on a checkpoint that is byte-identical by sha256. It returned **184-200-0, Corp 136-56, Runner 48-144** — every count the same. Self-play and the arena share the evaluator but not the driver, so this is the second path checked rather than the same one twice.

    **Two bugs found in the building, both worth the entry.** A model whose outputs do not grow with the batch dimension — `onnx_fixture`'s `Constant` nodes, or any checkpoint exported with a static batch axis — would have been handed four rows and returned one row's answer four times; `probe_max_batch` runs one two-row probe at construction and turns batching off for such a model. And the first version let a panic inside a runner strand both its slot and every row queued behind it, converting a loud crash into a **silent hang** — the worst failure mode for an unattended overnight run. The runner now answers its batch with an error and returns its slot before re-raising, so inference failure stays exactly as loud as it was before rows shared a batch.

28. **The promotion gate reads both chairs, not only the blend** (`feat/chair-aware-promotion-gate`, 9 September 2026). Item 26 left the gate holding a number that cannot express its own finding. `promoted = candidate_score >= 0.55` is a blend over both seats, and the fifth run's three candidates scored **0.688/0.234, 0.729/0.318 and 0.708/0.250** by chair. All three were rejected — but on the blend (0.461, 0.523, 0.479), which is luck, not judgement: a loop continuing along that Corp trajectory clears 0.55 blended with a Runner chair near 0.30, and the gate would have promoted a network that had *lost a seat*. That is item 13's failure verbatim, still live in the gate that was written to close it.

    `--promote-chair-floor` (default **0.45**) is now checked beside the blend, and `chair_floor_failure` names *which* chair collapsed rather than returning a boolean, because that name is the reading worth keeping in `promotions.log`. Replayed against the three recorded verdicts it stops all three on the Runner chair.

    **0.45 rather than 0.50, and the arithmetic is the whole justification.** A chair is half the arena: at 192 games a chair score has sd 0.036, so a candidate genuinely at parity on its weak chair clears 0.45 about 92 times in 100, while one genuinely at 0.30 is stopped at 4.2σ. Demanding 0.50 — the intuitive floor — would reject an honest tie half the time and make the gate a coin flip on top of a real one. The alternative rejected was gating on `min(corp, runner)` alone and dropping the blend: it discards the information that a candidate is better *overall*, and two chairs at 0.55 is a real improvement that a min-only rule would rank below one chair at 0.90 and the other at 0.56.

    The cheap screen gained the same reading at a much lower floor (`--arena-screen-chair-floor`, default **0.30**) for the reason the blended screen sits at 0.45: a screen chair is 48 games, sd 0.072, so 0.30 is 2.8σ under parity — an honest tie survives 997 times in 1,000 while the fifth run's ~0.25 Runner chair is stopped about three times in four, saving the 49-minute full arena on exactly the trajectory this loop is on. Both floors take `0` to reproduce a verdict recorded before they existed.

    This is the gate half of ROADMAP "next" item 1. It does not fix the cause — priors-only self-play runs ~65% Corp against the engine's 54.8% — only the gate's blindness to it.

29. **Chair-balanced training weights: the Runner chair moves, the seat trade does not close** (`feat/chair-balanced-objective`, 9 September 2026). The input half of ROADMAP "next" item 1, screened on the 7,200-game corpus item 26 kept for it — four training runs and four 384-game arena legs, ~2.5 h, no volume run.

    **Measuring the corpus first changed the design, and would have saved the obvious version of this from doing nothing.** Item 1 proposed weighting seats. But the step split barely moves with the win rate:

    | corpus | Corp wins | Corp steps | Corp steps on the winning side |
    |---|---|---|---|
    | `iter_001` (uniform search) | 54.8% | 44.4% | 64.6% |
    | `iter_002` (priors-only) | 65.7% | 45.6% | 75.2% |
    | `iter_003` | 64.9% | 45.8% | 75.6% |
    | `iter_004` | 63.9% | 45.4% | 74.4% |

    A seat reweight corrects 45/55 to 50/50 and leaves the distortion untouched. The imbalance is in the **outcome**: a Runner step carries a loss about three times in four. It is also larger than the win rate suggests — 65.7% of games but 75.2% of Corp steps sit on the winning side — and the same +9.5 gap appears in the *uniform* corpus (54.8% → 64.6%), so that amplification is a property of the game (a game the Corp wins carries proportionally more Corp decisions), not of the loop. So the cell is `(chair, sign of outcome)`, and flattening those six equalizes seat mass and outcome-within-seat mass together. `--chair-balance` weights by `(mean_count / count) ** strength` normalized to mean 1.0, the idiom `--segment-balance` already uses.

    **The result, every leg trained on the identical corpus with one flag changed, each played against the same incumbent all three of item 26's verdicts were measured against, priors-only both sides:**

    | `--chair-balance` | blended | Corp | Runner | chair gap | epoch shipped |
    |---|---|---|---|---|---|
    | 0 (control) | 0.4792 | 0.7083 | **0.2500** | 0.458 | 3 |
    | 0.25 | 0.5182 | 0.7344 | **0.3021** | 0.432 | 2 |
    | **0.5** | **0.5312** | 0.7396 | **0.3229** | **0.417** | 2 |
    | 1.0 | 0.5208 | 0.7448 | **0.2969** | 0.448 | 7 |

    **The control is exact, not approximate.** Training is seeded (`np.random.seed` / `torch.manual_seed`), so the control checkpoint is byte-identical by sha256 to `rejected_iter_004.onnx` and its arena leg reproduced item 26's iteration-4 verdict digit for digit (184-200-0, Corp 136-56, Runner 48-144). The treatment legs therefore differ in exactly one flag.

    **The reading.** The Runner chair rises 0.250 → 0.302 → 0.323 and falls back at full inverse frequency, an interior maximum at 0.5 — the over-correction `segment_balance_weights` already warns of. The Runner move at 0.5 is +0.073, above the 0.026–0.047 seed-spread band (Phase 3 §1); the Corp moves (+0.026 / +0.031 / +0.037) sit inside or at it and are **not** claimed. Monotone-then-turnover over four points is the evidence here, not any single leg — there is no second arena seed schedule to average over, because arena seeds are fixed on purpose so verdicts stay comparable.

    **What it does not do is close the trade.** The chair gap goes 0.458 → 0.417, a 9% dent. At the best setting the candidate is still 0.740 as the Corp and 0.323 as the Runner, and **both gates reject it** — 0.531 under the 0.55 threshold and 0.323 under item 28's 0.45 chair floor. Chair balance is a real but small correction to a large asymmetry, not a fix for it.

    **The arena is priors-only, so this is a policy-head result.** `SplitEvaluator` seats the network's priors and the uniform evaluator's value, so the exported value head never plays; every number above is the policy head. It is reached through two paths that this screen cannot separate — the reweighted policy loss, and the reweighted value loss reshaping the shared trunk at `--value-loss-weight 0.25`. A `--value-loss-weight 0` leg would separate them and has not been run.

    **A third training-log reading recorded as unreliable**, joining item 26's two. `best_val_loss` orders the legs 0.25 (1.2185) < control (1.2197) < 0.5 (1.2239) < 1.0 (1.2264); the arena orders them 0.5 > 1.0 > 0.25 > control. The criterion that picks the shipped epoch ranks the control *above* the leg that beats it by 0.052. The policy floor gap is flatter still — +0.1152, +0.1159, +0.1155, +0.1144 across an arena spread of 0.052 — which is item 26's finding again on a tighter case. And `mse_vs_outcome` anti-predicts outright: the 0.5 leg is the only one of the first three *worse* than the 0.780 chair null (0.788) and it is the best in the arena, because that null is precisely the base rate the objective was told not to fit. Under `--chair-balance`, `chair_baseline_mse` stops being the honest null and no weighted null has been put in its place.

    Two confounds stated rather than controlled: early stop picked a different epoch per leg (3, 2, 2, 7), which is part of the pipeline but means the legs differ in more than the flag downstream of training; and `mean_pred_runner` moves −0.124 → −0.105 → −0.088 toward zero across 0 → 0.5, the mechanism visibly working, then jumps to −0.149 at 1.0 on that leg's quite different trajectory.

    **Left off by default**, like `--segment-balance` and for the same reason: the visit-count target is a proper scoring rule whose optimum is the target distribution, and reweighting by an outcome the search did not know moves that optimum. The next volume run should carry `--chair-balance 0.5` explicitly, and item 28's gate is now able to see it if it stops working.

30. **The sixth run: `--chair-balance` does not survive being put in a loop** (9–10 September 2026, six iterations, 3.9 h, no promotion). Item 29 measured `--chair-balance 0.5` worth +0.073 on the Runner chair in a single training pass on a fixed corpus. This run asked whether a *generator* carrying that correction compounds it. **It does not, and both halves of the mechanism moved the wrong way.**

    **The run had to be bootstrapped to be a loop at all, and this is the correction to how the fifth run was described.** `--chair-balance` is a trainer flag: it changes the candidate, never the generator. The generator only advances on a promotion, and the fifth run never had one — `latest_policy.onnx` stayed item 22's iteration-1 network for all four iterations. So what item 26 called a loop was **one fixed generator with a growing corpus**, which is why "corpus size is not the variable" was the only thing it could have found. Running that shape again with `--chair-balance` would have re-measured item 29's cb05 leg five times: 0.531 blended, 0.323 on the Runner chair, and 0.323 cannot reach a 0.45 floor. So the incumbent was seated as item 29's cb05 checkpoint (sha256 `0bae34db…`) — **a bootstrap, not a promotion claim**; cb05 was rejected by both gates and stays rejected. Every promotion decision from iteration 5 on was gated normally, floors live.

    **Six iterations, six rejections, every one on the Runner chair.** Only iteration 5 reached a full arena (0.438 / Corp 0.667 / Runner 0.208); the other five were stopped by item 28's screen chair floor, which is what took the run to 3.9 h.

    | iter | window | corrected | blend | Corp | Runner | gap | floor gap | value edge |
    |---|---|---|---|---|---|---|---|---|
    | 5 | 6,000 | 1/4 | 0.469 | 0.604 | 0.333 | 0.271 | 0.1092 | −0.060 |
    | 6 | 4,800 | 2/4 | 0.479 | 0.708 | 0.250 | 0.458 | 0.1093 | −0.099 |
    | 7 | 4,800 | 3/4 | 0.396 | 0.604 | 0.188 | 0.417 | 0.1072 | −0.095 |
    | 8 | 4,800 | 4/4 | 0.438 | 0.625 | 0.250 | 0.375 | 0.0995 | −0.055 |
    | 9 | 4,800 | 4/4 | 0.500 | 0.750 | 0.250 | 0.500 | 0.0980 | −0.102 |
    | 10 | 4,800 | 4/4 | 0.458 | 0.729 | 0.188 | 0.542 | 0.0979 | −0.050 |

    **Pooled over all 576 screen games: 0.457 blended, Corp 0.670 (+5.8σ), Runner 0.243 (−8.7σ), chair gap 0.427.** Item 26's seat trade is intact and undiminished.

    **The correction inverts on the self-play side.** Item 26's diagnosis was that the loop's input and output are the same distortion twice, the input being a ~65% Corp corpus against the engine's own 54.8%. A chair-balanced generator makes that **worse**:

    | generator | games | Corp win |
    |---|---|---|
    | uniform search (`iter_001`) | 2,400 | 54.8% |
    | uncorrected priors (`iter_002`–`004`) | 4,800 | 64.9% |
    | **chair-balanced priors, cb05 (`iter_005`–`010`)** | **7,200** | **70.5%** |

    Six disjoint 1,200-game seed ranges: 69.5 / 70.3 / 69.7 / 71.8 / 71.2 / 70.6. The +5.6-point move is 4.8σ and it replicates six times. **Narrowing the chair gap against a fixed opponent and reducing the seat asymmetry a network shows against itself are different quantities, and item 29 moved only the first.** Item 29's legs all rose on both chairs (Corp +0.031, Runner +0.073); in self-play what survives is the Corp seat improving relative to the network's own Runner seat.

    **Replacing the training window with corrected data changes nothing, which is the cleanest result here.** `--window 4` was carried so the 7,200 pre-correction games could not dominate; the window went 1/4 → 4/4 corrected-generator over iterations 5–8, and **every column's observed sd is below its own sampling sd** — blend 0.036 against 0.051, Corp 0.066 against 0.068, Runner 0.054 against 0.062. Six iterations are six samples of one fixed quantity. A monotone Runner decline over iterations 5–7 (0.333 → 0.250 → 0.188) was read as a possible trend in session and **is withdrawn**: iteration 8 returned to 0.250 and the spread is pure 48-game sampling.

    **The floor gap anti-predicted for a fourth time, and more sharply than before.** It *declined* monotonically after iteration 6 — 0.1093 → 0.1072 → 0.0995 → 0.0980 → 0.0979 — while the arena did not move at all, and `best_epoch` rose 2 → 4 → 5 → 5 → 5 → 6 alongside it. Items 26 and 29 found the gap flat across arena swings of 0.06 and 0.052; this run has it moving on its own while the arena stands still. **It is not a proxy for playing strength in either direction.** The value head's edge over its chair null was negative in all six iterations (−0.050 to −0.102), having been positive throughout the fifth run — though item 29's caveat holds and `chair_baseline_mse` is not an honest null under `--chair-balance`.

    **What this closes.** `--chair-balance` is a one-shot correction against a fixed opponent, not a loop-stable one, and ROADMAP "next" item 1 is answered no. The seat asymmetry is not a training-objective problem: reweighting the objective moved the candidate 0.052 once (item 29) and moved the loop nothing. **The remaining suspect is the thing neither run touched — the search that generates the data.** A prior distilled from a uniform PUCT search inherits whatever chair bias that search has, and Phase 3 §1 already records the PUCT Runner chair as the weak one (0.516 against the Corp's 0.510 only after two fixes, and the heuristic Runner at 0.417). No root Dirichlet noise (standing open item) means self-play has no exploration pressure to find Runner lines the prior already discounts, which is a concrete mechanism for a loop that concentrates on one seat and a change that does not need another volume run to screen.

    **Cost, for comparison with the fifth run's 4.2 h per iteration:** 0.58–0.60 h per iteration end to end, of which self-play was 0.40–0.46 h against the fifth run's 1.95 h for the same 1,200 games. Item 27's cross-game batching is doing better than the 2.6× it claimed, and item 28's screen chair floor skipped five full arenas at 0.31 h each.

31. **Root Dirichlet noise: the generator's chair balance moves about a quarter of the way, and the specification is not the reason it doesn't move further** (`feat/root-dirichlet-noise`, 10 September 2026). ROADMAP "next" item 1 and the last standing open item item 30 named — the one mechanism neither training-side run touched. **5,760 self-play games, ~3.3 h, no volume run and no training.**

    **What landed.** `PuctConfig::dirichlet_epsilon`/`dirichlet_alpha` mix AlphaZero's `P' = (1 − ε)P + ε·η`, `η ~ Dir(α)`, into the root priors in `PuctNode::expand_root` and nowhere else, drawn per decision off a salted stream so the agent stays a pure function of its construction seed. **`ε` defaults to 0.0**, because noise is a property of a generator and not of a player: a bench or arena verdict taken with a noisy searcher scores the dice, and this repo's before/after attribution depends on a heuristic seating being byte-identical run to run. `netrunner_selfplay` is the only caller that turns it on, through `selfplay_config`; `arena_config` is a separate function so that stays a decision rather than a forgotten field. **Verified rather than argued:** a binary built from `main` and one built from this branch produce the same 16-game corpus at `ε = 0`, `sha256 50237a62…`.

    **The screen.** Item 26's `iter_003` configuration exactly — `priors_only_v5/latest_policy.onnx` seated `priors-only`, 128 simulations, seeds from 4800 — with `--dirichlet-epsilon` the only variable. That control is published at **64.9% Corp** over 1,200 games and reproduces here at **64.1%** (384 games, seeds 4800–5183) and **64.8%** (1,536 games, seeds 5184–6719), z = +0.26 between the two blocks. The gap being hunted is the 10.1 points from there to the uniform search's own 54.8%.

    | arm | games | Corp win | Corp steps | Corp share of winning steps |
    |---|---|---|---|---|
    | *uniform search (item 28's reference)* | *2,400* | *54.8%* | *44.4%* | *64.6%* |
    | ε = 0 (control) | 1,920 | 64.6% | 45.7% | 72.6% |
    | **every noise arm pooled** | **3,840** | **61.8%** | **46.6%** | **69.2%** |

    **Pooled, root noise is worth −2.8 points of Corp win share, z = −2.08** — about a quarter of the gap, in the predicted direction, and too small to be the explanation. The other two columns move by the same fraction: the outcome-within-seat distortion item 28 named as the real one recovers 3.4 of its 8.0 points, and the seat split does not move at all (it drifts the wrong way, 45.7% → 46.6%).

    **Neither dial reaches further.** `ε` saturates immediately — 0.25 and 0.5 are 59.1% and 59.4% on the same 384 games — and **α is not the reason either**, which is the result worth keeping. This game's roots are narrow: over 11,900 recorded decisions the median holds **2 candidates** and 59% hold three or fewer, only the tail being wide (p90 17, max 59). `Dir(0.3)` over two candidates is about `(0.95, 0.05)`, a coin-flip override rather than exploration pressure, where AlphaZero's own `α ≈ 10/n` rule asks for `α = 5` there — nearly uniform. So `dirichlet_alpha_scale` was added and run as a third arm on the same 1,536 seeds. **It lands on the fixed α's number:**

    | arm (seeds 5184–6719) | games | Corp win | Corp share of winning steps |
    |---|---|---|---|
    | control | 1,536 | 64.8% | 72.5% |
    | ε = 0.25, α = 0.3 | 1,536 | 62.6% (z −1.28) | 71.0% |
    | ε = 0.25, α = 10/n | 1,536 | 62.4% (z −1.39) | 69.1% |

    Two regimes that perturb opposite halves of the decision distribution — the fixed one mostly the wide tail, the scaled one mostly the narrow majority — agree to 0.2 points. The effect is a property of adding root exploration at all, not of how it is shaped.

    **A 384-game block is not enough to price this and the session proved it the hard way.** The first noise arm read 59.1% against a 64.1% control, a 5.0-point move that on 4× the games became 2.2, and its winning-steps column read 65.6% — apparently back onto the uniform search's 64.6% — where the 1,536-game block reads 71.0%. **That reading was recorded in session and is withdrawn.** The control was stable across the same two block sizes, so the instability was entirely in the treatment arm. This is item 26's lesson again (two points that "looked like a trend"): at sd 2.4 points a 384-game block cannot resolve an effect smaller than about 7.

    **What this closes, and what it hands on.** Root noise is implemented, priced and available for free to any future loop, and it is **not** the account of the Corp skew — a quarter of the gap at z = −2.08 is not a mechanism, it is a contribution. The sharper reading of the numbers is the one the screen makes unavoidable: **the uniform PUCT search's own corpus is chair-balanced at 54.8%, and the skew appears only once the priors distilled from it are seated (64.9%).** So the distortion enters at *distillation*, not in the search's shape — a policy head trained on a balanced corpus plays one seat much worse than the other. Item 22 measured that head's top-1 agreement with the search at 44.4% overall and never split it by chair; doing so is the next lead, and it is a diagnostic over an existing checkpoint and an existing corpus rather than a run. No stalls in any arm — every one of the 5,760 games ended (`agenda_threshold` ~90%, `flatline` ~9%, a handful of `deckout`), so the noise costs nothing in reachability.

**Standing open items:** the masked objective trains a never-visited legal action as illegal (record the true mask if simulations drop); `netrunner_gym` can still toggle-loop (no `progressive` filter on that path); the coverage card gate is inert at default seeds for decks the sweep has not played eight times; `t400_memory_diamond` was never installed by PUCT.
