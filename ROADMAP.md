# Netrunner Workspace Roadmap

**The single source of truth for status.** `AGENTS.md` is the rules of engagement, `ARCHITECTURE.md` the shape of the engine. This file is the index: where each area stands, what is next, and which area roadmap under `docs/roadmap/` holds the record. **Open an area roadmap only when working in that area** — each is the full log of decisions and measurements for its phase, and none is needed to orient.

Code and docs address entries as `ROADMAP <phase> §<n>` — "Phase 2 §5", "Rules Audit T8", "Phase 1 §8 Stage 5", "Phase 1.75 §6". Those addresses are permanent and resolve through the table below; an area roadmap keeps its section numbers even when an item is compressed to a line.

**Current goal:** a solid single-player Netrunner, then network play.

**Health (6 September 2026):** `cargo test --workspace` green (about 1,200 tests), `cargo clippy --workspace --all-targets` silent, both 256-seed sweeps clean under the rules-coverage gate, the card-conservation invariant and the fog gate, with **no stall tolerance** — every seating reaches `GameOver`. `ActionSpace::SIZE` 1646, `OBS_SIZE` 2262, `CARD_VOCAB` 192, `MAX_STEPS` 2,500, `DECISION_BUDGET` 256. **The shipped bots are complete and carry no data**: `Random`, `Heuristic` with its six `Personality` profiles, `Mcts` and `Puct` are pure code — `eval::Weights` constants compiled into `netrunner_bots`, no file read at run time. The one bot that consumes a file, `puct-onnx`, sits behind an off-by-default feature, and **no trained network has yet beaten the `Puct` search it would replace**: four volume runs, none promoted — the fourth (Phase 2 §5 item 20) was the first on the reworked observation and moved neither the value head's accuracy nor the Corp chair. The retired runs and their checkpoints are under `data/runs/`; the corpora are deleted.

## Where things stand

| Area | Status | Record |
|---|---|---|
| **Phase 1** — Single-player completeness | **Done.** §5 Format Support closed: mechanisms enforced, tables a documented seed. Saved decks, legible view, the rules gaps, *System Gateway* 77/77, *Elevation* 82/82 with all 28 published decks (192 matchups). | [phase-1-single-player.md](docs/roadmap/phase-1-single-player.md) |
| **Phase 1.5** — Session unification | Done. One `Session`, one `MAX_STEPS`, two seat kinds. | same file, tail |
| **Rules Audit** and the card fidelity audits | Closed. Twelve Tier-1 rules, Tier 2, six hazard classes; System Gateway 11 and Core Set 4 deviations fixed. | [rules-audit.md](docs/roadmap/rules-audit.md) |
| **Phase 1.75** — Learn to Play | Done. Seven lessons a side, the starter game at 6 points. | [phase-1-75-learn-to-play.md](docs/roadmap/phase-1-75-learn-to-play.md) |
| **Phase 2** — Bots, replay, gym, training | §1–§4 done. **§5 open: four volume runs and no candidate ever promoted.** The harness is trusted (unbiased arena, no livelocks, masked objective). The reworked observation (§5 item 18) bought no measurable value-head accuracy over the 990-wide one and did not move the Corp chair (§5 item 20). | [phase-2-bots-and-training.md](docs/roadmap/phase-2-bots-and-training.md) |
| **Phase 3** — Personalities and rating | Done, with one chair reopened. Five archetypes measured across seeds; Glicko-2 ladder. The Corp run term landed; the damage term is measured impossible at one ply. **PUCT's Runner chair was choosing by noise** — root-relative leaf values took it from 0.219 to 0.411 against the heuristic Corp and the Corp chair 0.417 → 0.510 (§1); it is still 0.17 short of the one-ply heuristic. | [phase-3-personalities-and-rating.md](docs/roadmap/phase-3-personalities-and-rating.md) |
| **Phase 4** — Network | §1–§3 done (masked log, reconnect, registry/lobby/clock/spectators, the published pool). §4 deltas deferred until profiled. | [phase-4-network.md](docs/roadmap/phase-4-network.md) |

## What is next, in order

1. **Choose what the fifth run changes** (Phase 2 §5 item 20). The fourth run ruled out the input: a 2,262-wide observation predicts outcomes no better than the 990-wide one (MSE 0.926 against 0.910) and the Corp chair still sits at 0.456. The four-leg arena on that engine (chair baseline Corp 0.693 / Runner 0.307) pins it: **the priors are neutral on both chairs and the value head alone loses the Corp chair by 0.29.** So the live suspect is **the value target** (about 40% of games end by flatline or deck-out, which no board feature and no point margin sees; half the target is the uniform search's own root belief), with **the bootstrap** the open loop question (four runs on uniform-search data; 0.55 never reached; a `--window 4` that never promotes redraws one distribution). Whatever changes, an iteration that cannot be promoted should stop paying for a 384-game arena: that leg was 5.00 h of the run's 9.95 h. `data/selfplay` and `data/checkpoints` are empty again; `launch_overnight.sh` is the fourth run's, unedited.
2. **Close the PUCT Runner's remaining gap** (Phase 3 §1, last bullet): 0.411 against the heuristic Runner's 0.583 on the same Corp; the jack-out churn and the 191-of-192 mulligan are the two named leads, each a five-minute measurement on the fixed-opponent seating.
3. Small open items, each recorded in its area file: `netrunner_gym` can still toggle-loop; the coverage card gate is inert at default seeds for decks the sweep has not played eight times; `netrunner_single_player/tests/common/mod.rs` still carries a filler fixture; Core Set cards have no sweep coverage.

## How this file is kept

- Status lives here or in an area roadmap, never in `AGENTS.md`. A new entry goes in the area file under its numbered section; then update the table row and the "next" list here.
- An entry records the decision, the alternative rejected, and the load-bearing numbers (before → after, seeds, game counts, branch). Narrative is compressed at the next pass; numbers and decisions are not.
- A later entry that corrects an earlier one says so in both places rather than deleting the earlier text.
