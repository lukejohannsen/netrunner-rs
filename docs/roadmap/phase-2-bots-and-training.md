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

**Decision and next step:** the observation rework for the value head — give it what `eval.rs` reads and the network cannot see (advancement tokens, rez state, ICE strength and subroutine status, the run target, the pending decision) — in a fresh run directory on a pinned binary; it invalidates every checkpoint and corpus. Then loop hygiene: `--masked-policy` by default, the arena at 384 games, per-chair scores in `iterations.log`.

**Standing open items:** ONNX per-call cost (~2 ms, ~6× uniform) makes a network-in-the-loop iteration ~90 min; no root Dirichlet noise in `puct.rs`; the masked objective trains a never-visited legal action as illegal (record the true mask if simulations drop); `netrunner_gym` can still toggle-loop (no `progressive` filter on that path); the coverage card gate is inert at default seeds for decks the sweep has not played eight times; `t400_memory_diamond` was never installed by PUCT.
