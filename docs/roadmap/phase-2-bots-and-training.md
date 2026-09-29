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

**Reopened for the Runner chair, 15 September 2026** (`fix/heuristic-runner-access-prospect`), after a person's first desktop game: the `operator` Runner "never does much more than make the same runs over and over — runs Archives even when it knows all the cards — and never draws or expands beyond its opening hand" (Phase 7 §3, item 3). Both are blindnesses of the Runner branch, and both were measurable on `main` before a line changed: over 96 heuristic-vs-heuristic games at seed 1 the Runner started **3,400 runs** against 314 credit clicks and 126 draws — nine clicks in ten — and 627 of those runs were on Archives. The run term was flat (`ACTIVE_RUN_WEIGHT` 0.6 for any breakable run on any server, `SUCCESSFUL_RUN_WEIGHT` 2.0 for any breach) and nothing read what the breach would find; a card in grip was worth nothing above `GRIP_FLOOR`; and the Runner branch had no term over the Corp's board, so a trash cost credits and bought nothing (12 trashes in 96 games). Two terms, two commits, each measured on pinned binaries at seeds 1–3 (the random-vs-random report is byte-identical throughout, since `random` never calls the evaluator):

| commit | blindness | fix | load-bearing delta (seed 1; seeds 2–3 agree) |
|---|---|---|---|
| a run is worth what the breach can find | ran any open server every click, Archives included; never trashed | `access_prospect`: `ACTIVE_RUN_WEIGHT` per *hidden* access (face-down Archives cards, R&D's top card, HQ's random card, unrezzed root cards), zero for a server already run this turn except HQ, whose repeat pays `((n−1)/n)^k`; `ADVANCED_CARD_PROSPECT_WEIGHT` 1.0 per token on a face-down root card; `KNOWN_AMBUSH_WEIGHT` 1.5 per face-up/rezzed access-punisher, subtracted; `OPPONENT_BOARD_WEIGHT` 0.5 over `visible_install_value` (never a sampled identity), which is the trash lever | runs 3,400 → 1,905 (Archives 627 → 46, HQ 565 → 821), credit clicks 314 → 1,129, trashes 12 → 88, steals 210 → 292, flatlines 18 → 8, **Runner wins 45 → 79 of 96** (seeds 2–3: 41 → 73, 47 → 73) |
| a card in grip is worth half its install | drew only after damage; settled at three cards | `HELD_CARD_WEIGHT` 0.5 × `install_delta` (presence, new coverage, minus cost and memory), floored at zero per card — so a draw is worth half the *sampled* top card and installing a live card keeps the other half | draws 76 → 332, programs 92 → 138, hardware 24 → 40, resources 30 → 38, events 50 → 112, Carmen 11 → 14, Runner wins 79 → 81 (seeds 2–3: 73 → 80, 73 → 77) |

The first cut of the HQ rule counted every repeat at full value and the Runner ran HQ 1,252 times in 96 games, thirteen a game, into a hand of one or two cards it had already read; the fresh-card probability is what brought it to 821. One earlier claim is corrected in place: `a_five_cost_breaker_beats_an_open_run_even_from_an_at_floor_grip` no longer holds — from an at-floor grip the install now loses the card's held half *and* the grip shortfall, and the open run wins; the weight that would keep it (0.13) leaves no draw worth a credit, and Carmen is installed more often, not less, so the test now pins the claim above the floor and the reversal at it. **What it did to the ladder is recorded under Phase 5 §2.** Follow-ups, not done here: a non-breaker is still worth `1.0 − 0.4 × cost` installed, so nothing costing 3+ is installed or drawn for, and once the rig covers all three subtypes the draws stop — a card-reading economy lever (declared `GainCredits`/`DrawCards` on installs, the shape `Trap`'s ambush term set) is the next term, measured on its own; and an event is worth nothing in hand.

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

Forty-three dated items, 4–13 September 2026: six volume runs, none promoted, and the chair
diagnostics that ended with item 43. Items 1–42 are one line each here and whole in
[the archive](archive/phase-2-bots-and-training.md); code cites items 21, 22, 25, 32 and 34–43 by
number, and the numbers are permanent. **Item 43 is the handover and stays whole below**, with the
standing open items after it.

- **Item 1** — Retrain on the new evaluator (`feat/puct-onnx-seat-and-retrain`).
- **Item 2** — Arena gating and the per-game split (`feat/arena-promotion-gating`).
- **Item 3** — Volume (`feat/selfplay-volume`).
- **Item 4** — First 2,400-game run, stopped after five iterations.
- **Item 5** — A cycle break in `select_action` (`fix/puct-select-cycle-break`).
- **Item 6** — Value target and weight.
- **Item 7** — Second 2,400-game run: nine iterations, died at iteration 10 (`fix/training-loop-pins-its-engine`).
- **Item 8** — Third run (`-g 1200 -s 128`), stopped at nine.
- **Item 9** — A hand is a multiset (`fix/action-space-aliases`).
- **Item 10** — The cycle guard strikes the set (`fix/cycle-escape-strikes-the-set`).
- **Item 11** — Both heads asked from the side to move (`fix/policy-head-perspective`).
- **Item 12** — The policy head diagnosed offline.
- **Item 13** — The arena plays both chairs (`fix/arena-chair-confound`).
- **Item 14** — Where the Corp's mass goes (`diag/policy-head-by-segment`).
- **Item 15** — Masking the policy objective.
- **Item 16** — "Stalls" were livelocks inside card-selection prompts (`fix/prompt-livelock-g0`, 5 September 2026).
- **Item 17** — The masked model on the fixed arena (5 September 2026).
- **Item 18** — The observation shows the value head the board (`feat/observation-rework`, 5 September 2026).
- **Item 19** — Loop hygiene (`feat/loop-hygiene`, 5 September 2026).
- **Item 20** — The fourth volume run: the observation rework measured (6 September 2026).
- **Item 21** — What the fifth run should change: the network never got the search fix (`feat/value-target-screen`, 7 September 2026).
- **Item 22** — The anchor screen: the fifth run's blocking question, sized at 2.5 hours (`chore/fifth-run-anchor-screen`, 8 September 2026).
- **Item 23** — Self-play can seat half a network (`feat/priors-only-self-play`, 8 September 2026).
- **Item 24** — The promotion gate was about to measure the configuration gap, not the candidate (`fix/incumbent-ablated-like-the-run`, 8 September 2026).
- **Item 25** — A network-in-the-loop iteration costs 4.2 h, and the reason is `batch = 1` (`fix/onnx-sessions-oversubscribe-the-machine`, 8 September 2026).
- **Item 26** — The fifth run: a priors-only loop does not compound (8–9 September 2026).
- **Item 27** — Batching across games: self-play is 2.6× faster and the corpus is byte-identical (`feat/batched-onnx-evaluation`, 9 September 2026).
- **Item 28** — The promotion gate reads both chairs, not only the blend (`feat/chair-aware-promotion-gate`, 9 September 2026).
- **Item 29** — Chair-balanced training weights: the Runner chair moves, the seat trade does not close (`feat/chair-balanced-objective`, 9 September 2026).
- **Item 30** — The sixth run: `--chair-balance` does not survive being put in a loop (9–10 September 2026).
- **Item 31** — Root Dirichlet noise: the generator's chair balance moves about a quarter of the way, and the specification is not the reason it doesn't move further (`feat/root-dirichlet-noise`, 10 September 2026).
- **Item 32** — The policy head is not failing the Runner chair, and the seat trade was the chair null (`diag/policy-head-by-chair`, 10 September 2026).
- **Item 33** — The chair floors are deltas from a measured null (`fix/chair-floors-from-measured-null`, 10 September 2026).
- **Item 34** — The pool's Corp share is a function of search budget, and seating a policy head is not what moved it (`diag/corp-share-vs-strength`, 10 September 2026).
- **Item 35** — The Runner chair is uncertainty-bound and the Corp chair is depth-bound, and that one fact explains both searches (`diag/mcts-runner-chair`, 10 September 2026).
- **Item 36** — PUCT's leaf cannot read a determinization at all — not "barely", *at all* — and that is a property of `evaluate_state_with` (`diag/leaf-hidden-state-sensitivity`, 10 September 2026).
- **Item 37** — Every unrezzed ICE in a determinized sample was a toothless Barrier, and nothing downstream repaired it (`fix/sampled-unrezzed-ice-is-toothless`, 11 September 2026).
- **Item 38** — The Runner's leaf can read the hidden state now, and reading it this way is worth nothing (`feat/runner-leaf-reads-unrezzed-ice`, 11 September 2026).
- **Item 39** — The Corp does rez it. The ICE just does not stop the run (`diag/corp-rez-rate`, 12 September 2026).
- **Item 40** — The term counts the right ICE now, and it is still worth nothing (`feat/unrezzed-threat-only-when-it-ends-the-run`, 12 September 2026).
- **Item 41** — `mcts`'s tree count is a decision, not a core count (`fix/mcts-tree-count-is-a-decision-not-a-core-count`, 12 September 2026).
- **Item 42** — A random playout validates only the move it plays (`feat/rollouts-validate-only-the-action-they-play`, 13 September 2026).

43. **The Runner chair's plateau is not samples, not budget and not playout depth** (`diag/runner-chair-samples-and-depth`, 13 September 2026). Opened to build a stronger `elite` Runner: item 35's 1 → 2 → 4 trees (0.510 → 0.576 → 0.635) was still rising, `DEFAULT_TREES`' own doc said nothing had measured eight, and Phase 5's Runner ladder is capped by its best Runner (a 0.104 span against the Corp's 0.229). On item 42's cheaper rollouts the wider search was measured as far as 32 trees and 1,024 simulations. **It is not stronger, so the ladder is unchanged**; what is kept is the measurement and the dials that took it.

    **Method.** Every cell is 384 games of the one-ply `heuristic` Corp against `mcts` as Runner: `bench --bots heuristic,mcts --pairing heuristic/mcts --games 384 --seed 1 --threads 18`. `--pairing` is new here and keeps each game's index from the whole square, so every cell plays the identical (matchup, seed) games, and item 42's after-leg supplies the 4 × 32 reference game for game. Comparisons are paired (`scripts/paired_bench.py --by-kind`, also new: it keys a seat on the bot's kind so `mcts@1024` pairs with `mcts@128`). Scores are the Runner's.

    | trees × iterations a tree | total | Runner | paired vs 4 × 32 |
    |---|---|---|---|
    | 4 × 32 (the `elite` Runner) | 128 | **0.604** | — |
    | 8 × 32 | 256 | 0.609 | +0.005 (z +0.20) |
    | 16 × 32 | 512 | 0.622 | +0.018 (z +0.68) |
    | 32 × 32 | 1,024 | 0.604 | 0.000 (z 0.00) |
    | 4 × 128 | 512 | 0.615 | +0.010 (z +0.41) |

    **Samples saturate at four.** Item 35's curve bends exactly where it stopped measuring, and at 512 simulations it makes no difference whether the budget buys samples (16 × 32) or depth per sample (4 × 128) — 0.622 against 0.615, the paired gap −0.008 (z −0.31). Eight times the search buys nothing on this chair.

    **Playout depth** (`--mcts-depth`, new: plies from the root to where a playout stops and is scored, tree descent included; the default is 16):

    | | 8 plies | 16 | 32 | 48 |
    |---|---|---|---|---|
    | 1 × 32 | — | 0.531 | **0.406** (−0.125, z −4.18) | — |
    | 4 × 32 | **0.440** (−0.164, z −5.31) | 0.604 | 0.547 (−0.057, z −1.93) | **0.487** (−0.117, z −3.96) |
    | 16 × 32 | — | 0.622 | 0.635 (+0.013, z +0.46) | — |

    **Sixteen is a peak, on every tree count measured**: shorter playouts never reach the Corp's turn, and longer ones lose more to their own weighted-random moves than they gain in reach. **Depth and samples are complements, which is item 36's prediction confirmed in games** — one to four trees is worth +0.073 (z +2.69) at 16 plies and **+0.141 (z +4.56)** at 32, and at 32 plies the curve is still climbing at sixteen trees (0.406 → 0.547 → 0.635) where at 16 it had flattened. But the best cell measured, 16 × 32 at 32 plies, lands on the `elite` rather than above it: +0.031 over it on this binary (z +1.12, 114 discordant) and −0.005 against the same cell on the binary before item 42 (0.641). **That 0.037 gap between two measurements of one policy is the noise floor here, and no cell clears it.** Its cost is four times the trees and twice the playout.

    **What deeper, wider search does change is how the Runner loses.** Games the Corp won, of 384:

    | cell | flatlines | Corp agenda wins |
    |---|---|---|
    | 4 × 32, 16 plies | 55 | 97 |
    | 16 × 32, 16 plies | 41 | 104 |
    | 16 × 32, 32 plies | **26** | **114** |

    Long playouts over many samples see the Corp's damage coming and the Runner stops dying to it, then gives the same games back in agenda races. That is a real change of style at no change of strength, which is worth knowing if a future rung wants a Runner that plays differently rather than better.

    **What this hands over.** The Runner plateau against a one-ply Corp is **0.60–0.64 across samples 4–32, budget 128–1,024 and depth 16–32**. So ROADMAP "next" item 1 — making PUCT marginalize over hidden state that varies — is bounded above by what this search already does: `mcts` *is* a search whose leaf varies with the sample, and it sits on this plateau, so the best that item could deliver is a PUCT Runner as strong as the `mcts` Runner the ladder already seats. Everything the plateau's cells have in common is the **playout policy** (`mcts::action_weight`, seven weights playing both chairs) and the static leaf. The depth curve is evidence against the playout — its moves cost more than its reach buys past 16 plies — but that is a hypothesis about the next lever, not a finding.

    Scaffolding, permanent, no behaviour change: `bench --pairing CORP/RUNNER` (a filtered game is byte-identical to that game in the whole square, pinned by a test), `bench --mcts-depth` with `MctsAgent::with_max_depth`, `AgentSetup::mcts_depth`, and `paired_bench.py --by-kind`. The `--determinizations` help text also stopped saying `mcts` reads its tree count off the rayon pool, which item 41 ended.

**Standing open items:** the masked objective trains a never-visited legal action as illegal (record the true mask if simulations drop); `netrunner_gym` can still toggle-loop (no `progressive` filter on that path); the coverage card gate is inert at default seeds for decks the sweep has not played eight times; `t400_memory_diamond` was never installed by PUCT.
