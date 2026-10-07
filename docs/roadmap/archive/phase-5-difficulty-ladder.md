# Phase 5 — A difficulty ladder: the closed record (§1–§26)

Moved verbatim from [`docs/roadmap/phase-5-difficulty-ladder.md`](../phase-5-difficulty-ladder.md) on 29 September 2026, and added to by every entry that closes since; the live file keeps one sentence per item. Headings are the originals, so an address a code comment cites resolves here.

## 1. Search budget is not a difficulty dial — DONE (12 September 2026)

The first cut spaced five rungs by search: random, one ply, `puct@32`,
`puct@128`, then the strongest configuration measured on each chair.
Calibrated at **48 games a pairing over the full 5 × 5 cross product
(1,200 games)**, each rung scored against a fixed one-ply opponent on the
other chair — the neutral-reference method every chair figure in Phase 2
uses:

| rung | as Corp | as Runner |
|---|---|---|
| novice (random) | 0.042 | 0.167 |
| apprentice (one ply) | 0.562 | 0.438 |
| operator (`puct@32`) | 0.500 | 0.354 |
| veteran (`puct@128`) | 0.458 | 0.604 |
| elite (`puct@512` / `mcts@128`×4) | 0.604 | 0.792 |

**The Corp chair had three strengths wearing five names**: rungs 2-4 at
0.562 / 0.500 / 0.458 are flat and sloping the wrong way. It is not
sampling noise — Phase 2 §5 item 34 measured the same curve
independently, `puct` as Corp scoring **0.458 → 0.581 → 0.714** at 32 →
128 → 512 simulations against a one-ply Runner that the heuristic Corp
beats at **0.583**. **PUCT does not overtake one ply on the Corp chair
until somewhere past 128 simulations**, so turning the search budget down
produces rungs that are weaker *and* indistinguishable.

**The monotonicity check passed it, which was the second bug.** The first
version of `scripts/ladder_report.py` asked whether any rung *fell*
significantly, and none did — a flat ladder is not an inverted one. It
now classifies each step `rise` / `flat` / `INVERT` against the sd of the
difference and demands a rise, because a flat step is a level selector
that does nothing. Re-run against this same report it says `climbs: NO`.

Also found and fixed here: the draft table gave its second rung
`Personality::Cautious`, which is a **Runner** archetype — and a profile
on the wrong chair "touches only the shared terms, which is harmless and
useless" (`personality`'s own doc comment). So that rung was a real
change as the Runner and a no-op as the Corp: a difficulty step that
differed by chair for a reason unrelated to difficulty. Every rung is
`Balanced`; style stays a separate axis, to be crossed with this one if
it is ever wanted.

## 2. Rungs are a strong bot handicapped — DONE (13 September 2026)

`HandicapAgent` wraps any `BotAgent` and replaces an `epsilon` share of
its decisions with a uniformly random legal action. That makes the ladder
**monotone by construction**: `epsilon` 1.0 is exactly `RandomAgent`, 0.0
is exactly the inner agent, and the same base bot with less handicap is
strictly stronger, so only the *spacing* has to be measured.

Rejected: perturbing `eval::Weights`. A mis-weighted bot is
*consistently* wrong, which reads as a strange opponent rather than a
beatable one, and its strength becomes a function of six numbers nobody
has measured. A random legal action is a recognisable mistake and it is
one dial.

Two properties are pinned by tests rather than intended:

- **Every rung is the one below it with less handicap, or a base the
  roadmap measured as stronger on that chair.** This failed immediately
  on the draft, where `novice` was a *different* base (`RandomAgent`)
  with *more* handicap — so `novice` is now one ply at `epsilon` 1.0,
  identical in play because the inner agent is never consulted at 1.0,
  and the bottom three rungs became one base family at 1.00 / 0.35 /
  0.00. `LevelKind::Random` had no remaining user and is gone.
- **The low rungs do not run the deep search.** Handicapping `puct@512`
  at rung 2 would make a beginner wait as long as an expert.

**The spans are the load-bearing measurement, and they differ by chair.**
From one ply to the best bot available is **0.229** of win rate on the
Corp chair and **0.104** on the Runner's. The handicap is also steeper
than it looks: `epsilon` 0.17 on `puct@512` cost the Corp **0.187**,
nearly its whole span, landing `veteran` on top of `operator`. Re-spaced
to 0.10 it scores 0.562 — the linear inversion predicted 0.61, so
**`epsilon` is not linear in win rate either**, which is worth knowing
before anyone tunes it again.

Calibrated at 48 games a pairing, against a fixed un-handicapped one-ply
opponent (`level:operator`):

| rung | Corp | Runner | `epsilon` (Corp / Runner) |
|---|---|---|---|
| novice | 0.042 | 0.146 | 1.00 / 1.00 |
| apprentice | 0.292 | 0.292 | 0.35 / 0.35 |
| operator | 0.500 | 0.500 | 0.00 / 0.00 |
| veteran | 0.562 | 0.542 | 0.10 / 0.17 |
| elite | 0.729 | 0.604 | 0.00 / 0.00 |

**The bottom half is a ladder and the top half is unresolved.** The first
two steps are +0.250 and +0.208 on the Corp chair and +0.146 and +0.208
on the Runner's — all four resolved rises, all four large enough for a
person to feel. The remaining steps are **0.042 to 0.167 against a
sampling sd of 0.10**, so at this sample size only `veteran → elite` as
Corp (+0.167) is resolved. **Tuning a 0.06 step with a measurement whose
sd is 0.10 is fitting noise, and the next move is games, not `epsilon`:**
192 games a pairing (~80 minutes) puts the sd at 0.05.

**The Runner chair's top is capped and it is not a spacing problem.**
With a span of 0.104, any two rungs inside it are ~0.05 apart however
`epsilon` is set — `veteran → elite` is worth ~0.19 as Corps and ~0.06 as
Runners. Five distinguishable Runner rungs do not exist until there is a
better Runner, and Phase 2 §5 items 36, 38 and 40 have each failed to
find one (the leaf was blind; it can read the sampled ICE and that is
worth nothing; it can read it *correctly* and that is worth nothing
either). **That work is now the blocker on half of this phase**, which is
the first time it has had a user-facing consequence.

**A wider `elite` Runner was measured and it is not stronger** (Phase 2
§5 item 43, 13 September 2026). The obvious candidate was more of what
`elite` already is — item 35's samples were still rising at four trees —
and it plateaus: 4 → 32 trees, 128 → 1,024 simulations and playouts of
16 → 32 plies all land at 0.60–0.64 against a one-ply Corp, the best
cell (16 × 32 at 32 plies) +0.031 over `elite` at z +1.12 and eight times
its cost. So the Runner rungs stay as calibrated, and the cap is now
measured from the search side as well as the leaf side.

**The Runner chair's cap inverted, and the Runner ladder is now one
ply at five handicaps** (15 September 2026, `fix/heuristic-runner-access-prospect`,
Phase 2 §5a reopened). The better Runner the paragraphs above were
waiting for turned out to be the one-ply bot with a leaf that reads the
board: pricing a run by what its breach can find, and a card in grip at
half its install value, took the heuristic Runner from **0.417 to 0.865**
against the fixed heuristic Corp over 192 games (paired, z −8.7 on the
Corp side). The search Runners inherited the leaf and not the policy:
on the same binary and games `puct@128` scores 0.760 and `mcts@128` —
the `elite` base — 0.677 (`level:elite` as seated 0.630, up from 0.542,
+0.089 at z −2.25 for the Corp), so the table above would have seated a
rung 5 that loses to rung 3. The spec is therefore
`Heuristic` at `epsilon` 1.0 / 0.5 / 0.25 / 0.10 / 0.0 on the Runner
chair — monotone by construction, every rung cheap, `operator` no longer
the un-handicapped bot there — and the Corp chair is untouched (its
evaluator did not move). **The Runner spacing is uncalibrated**: the
calibration table above was taken on the old evaluator and the old
bases, and the 192-game overnight `scripts/ladder_report.py` run is now
owed rather than optional. The search Runners go back on top when one
beats one ply again; the recorded lever is a one-ply playout policy
(Phase 2 §5, "next" item 1), whose prize just grew from 0.06 to 0.19.

**Both chairs now climb at every step, and the top of the Corp ladder is
the new cap** (`calibrate-runner-rung-spacing`, 15 September 2026). The
owed calibration ran at **768 games a cell** — the full 5 × 5 square at
`--games 384` on each of seeds 1 and 2, 19,200 games a spec, ~34 minutes
a seed on 18 threads, which is what the "overnight job" above costs now
that no rung on either chair runs `mcts`. Both tables are against a fixed
**un-handicapped one-ply** opponent on the other chair, which on the
Runner chair is `level:operator` and on the Corp chair is now
`level:elite` — the re-seating moved which rung that is, and the two
references are the same bot:

| rung | Corp | Runner (first cut) | Runner (re-spaced) | `epsilon` (Corp / Runner) |
|---|---|---|---|---|
| novice | 0.012 | 0.125 | 0.125 | 1.00 / 1.00 |
| apprentice | 0.078 | 0.449 | 0.258 | 0.35 / 0.75 |
| operator | 0.167 | 0.634 | 0.453 | 0.00 / 0.50 |
| veteran | 0.221 | 0.789 | 0.664 | 0.10 / 0.25 |
| elite | 0.266 | 0.833 | 0.833 | 0.00 / 0.00 |

**The first cut's Runner rungs were crammed at the top.** Its steps are
+0.324, +0.185, +0.155 and **+0.044** against an even step of 0.177, and
the last one is `flat` on seed 2 alone (+0.018, sd 0.028) — `veteran →
elite` was a level selector a player could not feel, which is the same
defect §1 caught on the Corp chair and the reason this run happened.
**`epsilon` turned out to be near-linear in win rate on this chair**
(w ≈ 0.833 − 0.70ε fits all five points to 0.034), so the fix was
arithmetic rather than a search: interpolating the measured curve for four
even steps gives ε = 0.73 / 0.46 / 0.23, and the round numbers **1.00 /
0.75 / 0.50 / 0.25 / 0.00** are within 0.03 of it. Re-measured on the same
two seeds, the steps are **+0.133, +0.195, +0.211, +0.169** — every one a
rise at z ≥ 6.7, every one a rise on each seed taken alone, and all four
within 0.045 of even. The apparatus check is that **exactly the 10 cells
whose Runner is `novice` or `elite` are byte-identical** between the two
runs, and no others: the two rungs whose `epsilon` did not change did not
move a single game.

**The Corp chair was already even and is left alone**: +0.066, +0.089,
+0.055, +0.045 against an even step of 0.064, all rises, z +6.4 / +5.3 /
+2.7 / +2.0. It spaces unevenly for a reason the Runner chair does not
have — it changes *base* between rungs 3 and 4 — and 0.025 of drift is
not worth a spec change.

**What the run found that it was not looking for: the Corp chair has no
top.** `puct@512` scores **0.266** against the un-handicapped one-ply
Runner, where §1 measured that same cell at 0.714. Nothing about the Corp
rungs changed; the Runner's evaluator did (Phase 2 §5a), and it moved the
whole pool — the 25 cells run **0.385 Corp** on the first-cut spec and
0.471 on the re-spaced one, against the engine's 0.548 baseline. So a
player sitting as the **Runner** has no hard opponent at any rung: rung 5
loses three games in four. This is the Runner chair's old cap, transferred
to the Corp, and the lever is the same kind of thing — that chair's
evaluator, not this table and not more search budget, which Phase 2 §5
item 34 showed saturates from 512 simulations on. **It replaces the
calibration as this phase's standing open item.**

Also worth recording because it is cheap to misread: the Corp column
above is **not** comparable to §1's or to the first cut's. Those were
taken against a one-ply Runner that no longer exists, and against
`level:operator`, which on the Runner chair is now handicapped. Only the
`level:elite` reference is the same bot across the two runs here.

**How a player reaches it.** `--corp-level` / `--runner-level` take a
name or a rung number and override the kind, the personality *and*
`--simulations` — one shared `bots::make_seat_agent`, so the TUI, the
headless runner and `bench` cannot disagree about what a rung is.
`bench --bots level:elite` seats and rates one, under its own name rather
than the bot it is built from, so both chairs feed one participant and
the per-role rating book does what it was built for.

**A rung keeps its style** (`feat/personality-crosses-the-ladder`, 13
September 2026): `LevelSpec::with_personality` crosses the two axes, so
`--corp-level 4` plays its deck's own style (`DeckFile::style`) or the
`--corp-personality` given — until then a rung silently played
`Balanced`. The calibration figures above are for `Balanced`; a style is
a bias on the same evaluator and does not change the order. Recorded in
Phase 3 §1.

**The daemon seats a rung** (`feat/serve-levels`, 13 September 2026):
`netrunner_server --serve --bot-level veteran` seats
`Level::spec(side).with_personality(..).agent(seed)` — the same bot the
TUI seats for `--corp-level veteran`, rated under the same id
(`bot:veteran`, `Level::rating_id`), so a daemon's book and a local
`ratings.json` describe one participant. An `Option<Level>` on
`ServeOptions` beside `bot_runner` rather than a `ServeBotKind`
variant, because that enum is a clap `ValueEnum` and a rung carries a
value; `--bot-level` with `--bot-runner none` is refused at `bind`.

**A start screen** (`feat/start-screen`, 13 September 2026): with no
`--corp`/`--runner` flag the TUI opens a picker instead of the old
"both sides are human" error — chair, rung (each row is
`LevelSpec::describe`, and the one `ratings::LocalRatings::suggest` —
`record::LocalRecord::suggest` since 20 September 2026, when the rating
book beside the log was removed and the suggestion, which only ever read
the log, was not touched (Phase 3 §2) —
points at is marked and pre-selected), style (the deck's own, or any
profile written for the bot's chair, or balanced), the opponent's deck
and your own (name · style · identity, saved decks included). **It is the
flag path, not a second one**: `StartChoice::apply` folds the choices
into the `Config` the flags would have produced, pinned by a test that
the folded form equals `--corp heuristic --corp-level veteran
--corp-personality glacier --corp-deck brick_stack ...`, so the seating
rule, the rating rule and the deck resolver stay singular. Any side flag
skips the screen, so every existing invocation is unchanged. The state
machine is a plain struct with `key(KeyCode)`, tested without a terminal
the way `replay_key` is; not driven by hand in this session.
**Superseded as the entry point** (same day, Phase 6 §1): tested by hand,
the client was "driven too much by switches", and the screen played one
game and exited. It is now the main menu's Play vs Computer form, reopened
after every game with the rung moved to the new suggestion; the fold into
`Config` is unchanged.

## 3. The Corp rezzed its cheapest ICE, not the ICE that held — IN PROGRESS (16 September 2026)

`feat/corp-reads-the-rig`, stacked on §2's calibration. The Corp chair's
ceiling from §2 is the target; this is the first cut at the evaluator
lever, and it moves the chair without closing the item.

**The structural finding.** Every `state.runner` read in
`eval::evaluate_state_with` sat in the `Side::Runner` arm. The Corp arm
read its own board and the Runner's *credit count*, and nothing else — so
the Corp priced ICE by `REZZED_ICE_WEIGHT − 0.4 × cost`, a formula that
**falls with cost**. It therefore rezzed its weakest ICE and left its best
face-down. Over 192 heuristic-vs-heuristic games it installed 1,201 ICE
and rezzed 493, split exactly backwards:

| ICE | cost | strength | ETR subs | installed | rezzed (before) | rezzed (after) |
|---|---|---|---|---|---|---|
| Tithe | 1 | 1 | 0 | 143 | 71 (0.50) | 75 (0.52) |
| Palisade | 3 | 2 | 1 | 67 | 34 (0.51) | 32 (0.48) |
| Brân 1.0 | 6 | 6 | 2 | 59 | 14 (**0.24**) | 21 (0.35) |
| Pharos | 7 | 5 | 2 | 43 | 11 (**0.26**) | 23 (**0.52**) |
| Empiricist | 7 | — | 0 | 36 | 4 (0.11) | 8 (0.22) |

A run met half a piece of ICE — 2,050 `IceApproached` over 3,781 runs —
and **86% of runs completed** (HQ 0.912, R&D 0.924, remote 0.793). The
Runner stole 615 agendas to the Corp's 158 scored.

**Two terms, both mirrors of ones the Runner already had.**
`ETR_SUBROUTINE_WEIGHT` 1.0 per run-ending subroutine on a rezzed piece
(the Corp's `PENDING_SUBROUTINE_WEIGHT`) and `UNBREAKABLE_ICE_WEIGHT` 1.2
for a subtype `rig_coverage` does not cover (the Corp's
`BREAKER_COVERAGE_WEIGHT`).

**What they are worth, stated carefully, because the first number taken
here was wrong.** `--corp-personality` unset is *the deck's own style*,
not `balanced`, so an early reading compared a deck-style baseline against
a forced-balanced result and claimed 0.161 → 0.204 at z = 2.18. Measured
like for like over 768 games on four seeds against the un-handicapped
one-ply Runner:

| Corp style | before | after | delta | z |
|---|---|---|---|---|
| forced `balanced` | 0.188 | 0.204 | +0.017 | 0.84 |
| deck's own | 0.161 | 0.194 | +0.033 | 1.67 |

Neither clears the usual bar on its own. What carries the change is the
second half of the Testing Rule's clause rather than the first: it is a
**rise on all eight seed-pairings** (four seeds × both style modes), which
is p ≈ 0.004 on a sign test, and the mechanism below is not statistical at
all.

**Both are paid only on a face-up card, and that is arithmetic, not
visibility.** The first cut paid them on the face-down piece too, where
they sit on *both sides of the rez* and cancel out of the decision they
exist to win: measured that way, `RezIce` went 1,073 → 1,053 and Corp wins
31 → 29 of 192 — a term that did nothing at all.

**What it did to the ladder, which is the point and is not all good.**
The full 5 × 5 square re-run at 768 games a cell, Corp column against the
un-handicapped one-ply Runner:

| rung | before | after | step before | step after |
|---|---|---|---|---|
| `novice` | 0.012 | 0.012 | — | — |
| `apprentice` | 0.078 | 0.078 | +0.066 (z 6.4) | +0.066 (z 6.4) |
| `operator` | 0.167 | **0.182** | +0.089 (z 5.3) | +0.104 (z 6.1) |
| `veteran` | 0.221 | 0.221 | +0.055 (z 2.7) | **+0.039 (z 1.9)** |
| `elite` | 0.266 | **0.277** | +0.045 (z 2.0) | +0.056 (z 2.5) |

**A better leaf lifts only the rungs that do not search.** `operator` is
one ply and takes the whole gain (+0.016); `veteran` is `puct@512` at
`epsilon` 0.10 and takes **+0.000**; `novice` is the random agent and does
not read the evaluator at all, moving in neither column, which is the
check that only evaluator-using rungs shifted. The third step therefore
*compressed*: the rung below caught up to the one above. Pooled it is
still a rise, but at 384 games a seed it reads `flat` on seed 2 (+0.018,
sd 0.029) where **both** seeds were `rise` before. So the change costs the
Corp ladder margin at exactly its weakest joint, and the remedy is a Corp
re-spacing of the kind §2 did for the Runner — the `operator` rung wants a
small handicap of its own now — or a `veteran` rung that is not simply
more search over the same leaf. Phase 2 §5 item 34 already says search
saturates from 512 simulations on, and this is the same fact seen from the
ladder: lookahead finds the rez decision the leaf term encodes, so
encoding it buys the searching rungs nothing.

The Runner column is unchanged within noise (−0.016 to +0.004 a rung) and
its calibrated spacing survives, which is what makes the Corp column above
attributable.

**Three things measured and rejected**, each recorded on the constant it
would have touched:

- **A Corp `HELD_CARD_WEIGHT`.** The inviting symmetry — the term that
  fixed the Runner chair in §2's predecessor, and the Corp clicks to draw
  1.1 times a game against the Runner's 3.6. Tried at 0.42, 0.45 and 0.48,
  all three gave the *same* 1,623 draws against a baseline 425 and the
  same **0.083** against 0.182: above `OWN_CREDIT_WEIGHT` it is a switch,
  not a dial. Installs did not move (4,959 → 4,961), so the cards never
  reached the table; the clicks came out of `GainCreditClick` and rezzes
  fell 2,156 → 1,583. **The Corp is not card-starved, it is
  credit-starved** — it cannot pay to rez what it has already installed.
- **More fort.** `--corp-personality glacier` is literally "build the fort
  first" (ICE at 1.8, agenda protection 1.0 at cap 3, credits at 0.5) and
  scores **0.174** against balanced's 0.198. More ICE is not the lever.
- **A bigger rez weight.** Once the two terms above are in, raising
  `REZZED_ICE_WEIGHT` from 1.4 to 100.0 produces **byte-identical games**:
  the rez decision is saturated, every affordable rez already happens, and
  what is left is the credits to pay for them.

**A measurement trap found on the way — and the first diagnosis of it was
wrong.** Sweeping the three ambush constants read 0.208 where the same
Corp weights applied through `--corp-personality trap` read 0.259, and
this entry first blamed a cross-chair leak: both chairs share one
`Weights`, so the Corp's ambush preferences were supposedly reaching the
Runner through `visible_install_value`. **They were not.** Each agent
holds its own `Weights` (`HeuristicAgent::with_personality`), and the
Runner evaluates with the Runner's, so a Trap Corp is invisible to a
Balanced Runner. The real cause is duller and matters more: editing a
*constant* moves `Weights::default()`, and in a `heuristic`-vs-`heuristic`
pairing **both** agents are `Personality::Balanced`, so the sweep taught
the **Runner** to respect ambushes at the same time. A one-line fix that
forced the balanced constants inside `visible_install_value` was written,
measured as a no-op — no Runner profile sets an ambush term, so there was
nothing to force — and reverted. What survives is the measurement rule:
**a constant sweep moves both chairs and `--corp-personality` moves one**,
so a chair figure must come from the flag.

**The personalities need re-auditing against this, and one of them has a
named defect.** Rez value is now `rezzed_ice_weight + 1.0 × etr + 1.2 ×
uncovered − own_credit_weight × cost`, and the profiles were tuned before
the first three terms existed. Measured rez rate by ICE cost, 384 games a
profile:

| profile | cost 1–2 | cost 3–4 | cost 5–7 | mean cost rezzed |
|---|---|---|---|---|
| balanced | 0.478 | 0.395 | 0.324 | 3.15 |
| rush | 0.487 | 0.385 | **0.366** | **3.24** |
| glacier | **0.538** | 0.487 | 0.342 | **3.12** |
| trap | 0.483 | 0.406 | 0.322 | 3.16 |

**`Glacier` — "build the fort first" — rezzes the cheapest ICE of any Corp
profile, and `Rush` the most expensive.** The arithmetic says why:
Glacier's `rezzed_ice_weight` 1.8 is a *flat* bonus that helps cheap and
expensive ICE alike, while its `own_credit_weight` 0.5 is a *per-cost*
penalty that bites hardest on exactly the ICE the profile exists for. The
two knobs fight and the per-cost one wins; `Rush` takes that axis by
accident at 0.3. `own_credit_weight` is doing double duty — it makes the
Corp gain credits *and* makes it reluctant to spend them — and a profile
that wants to rez needs the first without the second.

**The obvious fix does not work, which is why this is recorded rather than
taken.** Dropping Glacier's `own_credit_weight` to balanced's 0.4 scores
**0.156** and to Rush's 0.3 **0.151**, against **0.174** as it stands —
both worse, on separate binaries checked by hash. So the rez-rate story is
real and the one-knob repair is not the repair. Glacier also advances at
1.2 against balanced's 1.5, and balanced beats it outright (0.198 against
0.174), so what the profile costs itself may not be the fort at all. A
profile retune is its own measured job with its own before/after, not a
rider on this branch.

**The next lever, with its number already taken.** `--corp-personality
trap` — three card-reading terms, `ambush_weight` 0.0 → 3.0,
`ambush_advancement_weight` 1.0 → 2.8, its cap 3 → 7 — scores **0.259 ±
0.032** against the same build's forced `balanced` **0.204 ± 0.029** over
768 games each, z = 2.6, with flatlines 32 → 79 as the mechanism. Both
sides of that comparison name a personality, so it is the one figure here
the style confound above cannot touch, and it is larger than anything §3
itself bought. The
balanced Corp has `AMBUSH_WEIGHT` at zero *by design* ("the balanced Corp
does not play for the flatline"), which was right against the Runner that
existed when it was written and is wrong against one that makes 20 runs a
game. Moving the default is not a free +0.055, because the same constants
also teach the Runner to respect ambushes: with both chairs moved the
gain is +0.026. Deciding what balanced should be — and whether Trap stays
a distinct archetype afterwards — is the open work.


## 4. Owed: re-space the Corp chair, reconsider how its top rungs are built, and teach the bots to play in phases — OPEN (16 September 2026); (c) CLOSED on both chairs (22 September 2026, §19); (b) and (a) CLOSED on the Corp chair (23 September 2026, §24)

Three jobs, opened by §3 and none attempted there. They are recorded
together because (b) may make (a) unnecessary, and (c) is the largest of
the three.

**(a) Re-space the Corp rungs.** *Done for `veteran` in §14, on
re-taken numbers — `operator` was not out of place; `veteran` was, and
plays at `epsilon` 0.20 (`trap` 0.15, `rush` kept at 0.10).* §3 lifted `operator` (one ply) by +0.016
and `veteran` (`puct@512`) by +0.000, so the `operator → veteran` step
fell +0.055 (z 2.7) → **+0.039 (z 1.9)** and now reads `flat` on one seed
of two at 384 games a cell, where both seeds were `rise` before. Pooled at
768 it still climbs, so this is a **margin** problem, not a broken ladder
— but it is the chair's weakest joint and it got weaker. The method is
§2's, applied to the Corp: measure the `epsilon` → win-rate curve on this
chair, interpolate for four even steps, take round numbers. The Corp
column is 0.012 / 0.078 / 0.182 / 0.221 / 0.277, so an even step is 0.066
and the rung that is out of place is `operator`, at 0.182 against a target
of 0.144 — it wants a small handicap of its own now, where it has had
`epsilon` 0.0 since the ladder was built. **Do not assume the curve is
linear**: the Runner chair's was (w ≈ 0.833 − 0.70ε to 0.034), and this
one is visibly not — `epsilon` 0.35 already costs the Corp more than half
its margin over random (0.182 → 0.078), so the interpolation needs its own
measured points rather than the Runner's shape.

**(b) Reconsider how the top two rungs are built**, which is the deeper
item and is the reason (a) is worth doing *after* it rather than before. *`glacier`'s answer is §21: its whole ladder is one ply at five
handicaps, because one ply beat its search rungs once it built the fort.
`Balanced` and `trap` are next, on numbers re-taken after §20.* *§24 closes it: once every Corp profile built the fort (§23), one ply beat
`puct@512` in every style, and every Corp ladder is `glacier`'s one ply at five handicaps.*
`veteran` and `elite` are `puct@512` at `epsilon` 0.10 and 0.0 — **more
search over the same leaf** — and §3 showed that a better leaf buys them
nothing at all (+0.000 for `veteran`). That is Phase 2 §5 item 34's
saturation finding arriving from a new direction: lookahead already finds
what the leaf term encodes, so the two levers the ladder has for its top
rungs (search budget, evaluator quality) are *the same lever twice*, and
both are spent. The consequences to work through:

- **The Corp ceiling will not yield to evaluator work alone.** §3's whole
  gain at `elite` was 0.266 → 0.277. Another term of the same kind buys
  another hundredth.
- **Handicapping may be the wrong mechanism for the Corp's top half.**
  The ladder's founding decision (§1) was that rungs are one strong bot
  handicapped, because search budget is not a difficulty dial. That holds.
  But it assumed a strong bot *exists* to handicap, and on this chair the
  strongest thing available is 0.277 — so the top of the Corp ladder is
  handicapping something that is not strong.
- **The measured candidate is not more search.** §3's own numbers put
  `--corp-personality trap` at 0.259 ± 0.032 against balanced's 0.204 ±
  0.029 over 768 games each, chair-isolated, z = 2.6, flatlines 32 → 79 —
  larger than anything §3 itself bought, and it is a *card-reading* lever
  rather than a deeper search. The balanced Corp switches it off by design
  ("does not play for the flatline"), which was right against the Runner
  that existed when it was written and is wrong against one making 20 runs
  a game. Taking it is not a free +0.055: the same constants also teach
  every `Balanced` Runner to respect ambushes, and with both chairs moved
  the net is +0.026. Deciding what `balanced` should be — and whether
  `Trap` survives as a distinct archetype afterwards — is the work.
- **The Corp personalities want a retune against §3's rez arithmetic**
  regardless, and `Glacier` has a named defect there (§3): the fort
  profile rezzes the *cheapest* ICE of any Corp personality. The one-knob
  repair was measured and is not the repair.

**(c) The bots have no notion of *when* in the game they are.**
`evaluate_state_with` is a static function of the position: it scores a
board the same way on turn 1 and turn 20, so the Corp interleaves building
and scoring every single turn instead of doing one and then the other.
Measured in §3: 13.0 installs and 6.3 advancements a game spread over 11.4
turns, continuously. A person plays the Corp in phases — bank economy and
build the wall, *then* push an agenda behind it — and nothing in this
evaluator can express the "then".

**The striking part is that the archetypes already name the two phases.**
`Glacier` is "build the fort first" and `Rush` is "the agenda goes on the
table first"; they are exactly the early and late halves the idea calls
for, and they are *static profiles for a whole game* rather than phases of
one. That they cannot be sequenced is the gap, and the numbers hint that
neither extreme is right on its own: balanced beats both (0.198, against
`Glacier`'s 0.174 and `Rush`'s 0.109 over 384 games).

The **deck half is already built** and is worth not rediscovering:
`Personality::for_deck` reads a style off the `DeckFile`, every embedded
deck names one (`every_embedded_deck_style_is_a_personality_for_its_side`),
and an unset `--corp-personality` means *the deck's own style* — which is
the flag behaviour §3 first got wrong. So "different decks play
differently" is wired; what is missing is that a deck's strategy is fixed
for the whole game.

**The design question, and it is a real one.** `GameState::turn` exists, so
a phase schedule is implementable as weights that vary with it. But the
lesson recorded on `Personality::Trap` cuts against exactly that: the
profile's first cut was six generic knobs around a wish, it lost to
balanced, and it only worked once it was three terms that **read the
cards**. A turn-number switch is a generic knob of the same kind. The
alternative is terms whose value moves with the board on its own — what
the agenda points still needed are worth, whether the credits on hand
cover the rez costs already sitting face-down on the table (§3: the Corp
is credit-starved and cannot pay for what it installed), whether a scoring
remote exists yet — so that "build now, score later" *falls out of* the
position rather than being scheduled against a clock. Try the board-reading
form first and keep the turn counter as the fallback to beat, not the
first thing tried.

**§5 is the first work against (c)**, and it is the instrument rather than
the change: a per-turn tempo profile, because nothing in this repo could
have told a phased Corp from an unphased one. Its baseline confirms (c)'s
prediction on the Corp chair, finds the Runner chair's stance *inverted*
(most aggressive when its rig is emptiest), and corrects §3's
"credit-starved" reading to its early-game form — all three of which bear
on which stage signal is worth reading.

**(c) closed, 22 September 2026.** Staging the Corp from `glacier`
toward pressure lost on every leg (§19, #134 closed unmerged), as
staging the Runner did in §6–§8. What the Corp was missing was a
*where*, not a *when*: §19's fort terms answer that one.

**Sequencing:** (c), then (b), then (a). Each one moves the numbers the
next one would be measured against, and re-spacing rungs around a top rung
that is about to be rebuilt would be measured twice and thrown away once —
which is exactly what happened to the Runner chair's first calibration in
§2, and the note is here so it does not happen again.

**Standing open items:** the **Corp chair's ceiling** — still open, and
§3 is the first cut at it: the one-ply Corp gains +0.017 (forced
`balanced`) to +0.033 (deck styles) against the un-handicapped one-ply
Runner, a rise on all eight seed-pairings but under the bar on either
alone, so a Runner-seated player still runs out of ladder at rung 5. The
lever remains the Corp evaluator, not this table, and §3 names the next
one — the ambush terms balanced switches off — with its number already
measured and larger than what §3 itself bought. *Closed:* the calibration that would resolve
the top steps — it ran at 768 games a cell and both chairs climb at every
step; the server offers levels; local play is rated and the modal names
the next rung (`feat/local-rating`, Phase 3 §2); the start screen.


## 5. The tempo instrument, and a baseline in which both chairs play their stance backwards — DONE (16 September 2026)

`diag/tempo`, the first branch against §4(c). Its product is a
measurement: `netrunner_cli diag tempo` and the profile it takes off an
unmodified `main`.

**Why an instrument first.** Every figure in §3 and §4 is a win rate, and
a win rate cannot tell "plays in phases now" from "plays the same and wins
slightly more". §4(c)'s own evidence — 13.0 installs and 6.3 advancements
over 11.4 turns — is a whole-game mean, and a whole-game mean is precisely
the statistic that cannot distinguish a Corp that installs five times and
*then* advances five times from one that alternates ten times. So the
change §4(c) asks for is not measurable by anything this repo had.

One record per turn per chair: that turn's clicks split by what they
bought, the no-click tempo actions (rez, score, steal) beside them, and a
snapshot of the board they were spent on. `HistoryEntry` already carries
`turn_number` and `side`, so nothing reconstructs either. Verified
complete the way `rez-rate` is — turns recorded against `GameState::turn`
at the end of each game, 9,079 of 9,079 over 384 games.

**Both chairs name a personality by construction.** `--corp`/`--runner`
take a `BotSpec`, whose personality defaults to `Balanced` and not to the
deck's style, so §3's measurement trap cannot be sprung through this
command. A rung is spelled `level:elite` and is seated through
`make_seat_agent`, so profiling the ladder — what §4(a) and §4(b) will
want — is already wired.

**The baseline, `heuristic:balanced` both chairs, 384 games, seed 1
(seed 2 reproduces every column within 0.02):**

| Corp turn | 1 | 2 | 3 | 5 | 8 | 12 | 13+ |
|---|---|---|---|---|---|---|---|
| installs | 2.46 | 1.65 | 1.57 | 1.14 | 0.82 | 0.76 | 0.72 |
| advances | 0.17 | **0.93** | 0.46 | 0.40 | 0.47 | 0.63 | 0.73 |
| rezzes | **1.04** | 0.78 | 0.60 | 0.45 | 0.41 | 0.42 | 0.34 |
| credits at start | 5.0 | 4.6 | 3.2 | 4.5 | 9.1 | 16.1 | **27.0** |
| face-down installs | 0.00 | 1.12 | 1.58 | 2.81 | 3.67 | 4.35 | **5.61** |

| Runner turn | 1 | 2 | 3 | 5 | 8 | 12 | 13+ |
|---|---|---|---|---|---|---|---|
| installs | 0.82 | 0.28 | 0.20 | 0.11 | 0.08 | 0.12 | 0.07 |
| runs | 1.85 | **2.20** | 1.98 | 1.78 | 1.43 | 1.15 | **1.00** |
| rig coverage (of 3) | 0.00 | 0.79 | 0.90 | 1.04 | 1.17 | 1.30 | 1.43 |

**There is no phase anywhere in either chair.** The Corp installs on every
turn of the game and advances on every turn from the second — its
*advancement peak is turn 2*, before any fort exists — which is §4(c)'s
prediction confirmed at the resolution it was made at. Nothing in the
curve marks a transition; both columns simply decay.

**The Runner's stance is inverted, and that is the new finding.** It
installs almost everything it will ever install on turn 1, stops by turn
3, and never gets past **1.43 of 3** ICE subtypes covered — while its runs
*peak at turn 2 and fall by half* over the game. So it is at its most
aggressive when its rig is emptiest and its most passive when its rig is
best: the exact reverse of "build a sweet rig first, then get aggressive",
and the reverse of `Builder`'s and `Aggressive`'s own doc comments. A dial
that moved the right way would be pushing against a baseline that is
currently running the wrong way, which makes the Runner chair a *larger*
target than the Corp one rather than the secondary chair §4 assumed.

**A correction to §3, and it is load-bearing for the Corp stage scalar.**
§3 concluded "the Corp is not card-starved, it is credit-starved — it
cannot pay to rez what it already installed", and read as a statement
about the whole game that is false. Credits at turn start run **5.0 → 3.2
→ 27.0**: the Corp *is* credit-starved for the first four turns, and from
about turn 8 it is sitting on money it does not spend, while face-down
installs climb monotonically to **5.61 and never come down** and its rez
rate *falls* from 1.04 a turn to 0.34. Late in a game this Corp is rich,
holding five unrezzed cards, and rezzing less than at any earlier point.
So a "credits against the rez costs already on the table" term will be
inert exactly where §3's sentence implied it would bite — the shortfall it
measures closes by turn 8 on its own — and the real defect is later and
different: a rez that is affordable and still not taken. §3's sentence is
corrected to its early-game form rather than deleted.

**A bug the tests found before the measurement did.** A rez is the Corp's
tempo and happens on the *Runner's* turn, at an ICE approach. Attributing
an action by `(turn_number, side)` therefore dropped every ICE rez in the
game, because the Corp owns no record for an even turn — the count read
4.6 rezzes a game against a true 7.5, a 40% loss, and the profile would
have shown the Corp barely rezzing at all. An action is now attributed to
the *acting side's own current row*, which is where a reader looking for
"when does this Corp start rezzing" will look. `a_rez_on_the_runners_turn_is_the_corps_tempo`
is that bug.

**One caveat on reading the tail.** Rows are per-turn means over the games
that reached that turn, and `n` falls 384 → 165 by turn 12, so the late
rows are the long games — the ones neither side closed. The decay in
installs and runs is therefore partly selection and the tail is not
evidence on its own. What is not selection is the *early* shape, where
every row has all 384 games: the Corp advancing at its peak on turn 2, and
the Runner making 1.85 runs on turn 1 with no rig at all.

`breaker_coverage` is public for this, on `is_unrezzed_threat`'s
precedent: a diagnostic that re-derived "coverage" itself would measure
its own copy rather than the term the Runner reads.

Reports under `target/coverage/tempo-baseline-seed{1,2}.json`. Workspace
tests green, clippy silent; no engine or evaluator behaviour changed, so
nothing here can move a game.


## 6. The stance dial, and the endpoint it travels to is the wrong one — DONE, at gain 0.0 (16 September 2026)

`feat/runner-plays-in-phases`, stacked on §5. The mechanism §4(c) asked
for, built, measured on the Runner chair, and **shipped off**: it costs
that chair 0.159 to 0.180 of win share, monotone in the dial and
reproduced on two seeds. `STAGE_GAIN` is 0.0 and the branch is
byte-identical to §5 game for game.

**The Runner chair first, against §4's order**, because §5's baseline
found its stance *inverted* rather than merely flat — most aggressive when
its rig is emptiest — which is a sharper target than the Corp's. §4's
reason for sequencing (do not calibrate the ladder twice) is unaffected by
which chair goes first.

**The mechanism, and why it is an interpolation rather than new terms.**
Four of the seven personalities describe themselves with a temporal word
they cannot act on — `Glacier` "build the fort, *then* score behind it",
`Rush` "score early, protect late", `Builder` "the rig first… *before* the
runs start", `Cautious` "a full rig *before* a run" — and each pair moves
*the same fields in opposite directions*. They are the two ends of one
dial and the game is played standing still on it, which is also the first
explanation §3's unexplained number has had: balanced beating both
`Builder` and `Aggressive` is what a fixed midpoint of a moving dial looks
like against its own ends. So the endpoints were already measured and
already shipped, and only the scalar was missing. Six new `Weights` terms
would have grown the surface for the same claim and left the profiles
still unable to sequence.

`stage_weights` lerps `Weights` from the build archetype to the pressure
one and then blends that toward the caller's own weights by `stage_gain`,
so 0.0 is exactly the static evaluator. It is hoisted *above* the `match
side`, not inside the arm, because `Aggressive` moves
`opponent_credit_weight` and that term is read in the shared prefix.
Counts round rather than truncate, so `grip_floor` travelling 3 → 2
crosses at the halfway point.

**The scalar** is `runner_stage`: the rig over the evaluator's own
`breaker_coverage`, overridden by the Corp's clock
(`corp.agenda_points / rules.winning_agenda_points`, not a hard-coded 7 —
the starter format plays to 6). The larger of the two wins rather than the
sum, which makes urgency an override rather than a bonus, so a Runner that
never finds its breakers still plays instead of banking until it decks.
Nothing reads a sampled card. It is continuous everywhere, because
`UniformPolicyEvaluator::evaluate_from` scores leaf minus root and a stage
that jumped inside one search would make two leaves of one tree
incomparable.

**One `--stage-gain` flag isolates the chair**, and that is structural
rather than a discipline: only the Runner arm is staged, so a Corp seat
handed the same gain is byte-identical to one handed zero
(`the_corp_is_not_staged_so_one_flag_isolates_the_runner_chair`). §3's
constant-sweep trap cannot be sprung through it.

**What it is worth: less than nothing.** `heuristic` both chairs, 384
games a leg, paired by `scripts/paired_bench.py` over discordant games:

| leg | Corp win rate | Runner delta | z | discordant |
|---|---|---|---|---|
| seed 1, gain 0.0 → 0.5 | 0.143 → 0.224 | **−0.081** | 3.32 | 87 |
| seed 1, gain 0.0 → 1.0 | 0.143 → 0.302 | **−0.159** | 5.90 | 107 |
| seed 2, gain 0.0 → 1.0 | 0.133 → 0.312 | **−0.180** | 6.49 | 113 |

Monotone in the gain and far outside the 0.026–0.047 seed-spread band in
the wrong direction. This is not a term that bought nothing; it is one
that cost a great deal.

**And §5's instrument says why, which is the part worth keeping.** At gain
1.0 the Runner does change stance as designed — credit clicks **10.3 →
6.1** a game, draws 3.3 → 5.0, installs 1.9 → 2.6, trashes 0.9 → 1.4 —
so it banks less and builds more, exactly what the build endpoint asks
for. But its **rig coverage falls while its rig grows**: 0.83 → 0.30 at
turn 2, 1.05 → 0.71 at turn 8, against a rig *size* rising 1.55 → 1.71.
It is installing more cards and fewer breaker subtypes.

**That is `Glacier`'s named defect (§3), on the Runner's side of the
table.** `Builder`'s `board_presence_weight` 1.6 is a flat bonus on *any*
rig card, and it fights the `breaker_coverage_weight` 4.0 that is the
profile's actual purpose; the flat one wins, because there are always more
non-breakers to install. §3 found the same shape in `Glacier` — a flat
`rezzed_ice_weight` 1.8 losing to a per-cost `own_credit_weight` 0.5 — and
recorded that the one-knob repair is not the repair.

**The dial makes that defect self-reinforcing, which is the new part.**
The scalar is gated on coverage, so a Runner that fills its rig with
non-breakers never raises its coverage, never leaves the build stance, and
goes on filling its rig with non-breakers. A static `Builder` merely plays
badly; a staged one is trapped. That is a fact about this *pair* of
endpoints, not about staging.

**What it hands over.** The suspect is the endpoints, not the scalar. The
dial demonstrably moves stance in the direction asked of it, and the
stance it moves to is one that does not build the thing it is building
for. The next cut is the endpoint repair that §4(b) already owed for the
Corp, now owed for both chairs and with a measured reason: a build
endpoint has to reward *coverage* rather than *presence*, or it is not a
build endpoint. Until then the travel is off.

Ships at `STAGE_GAIN = 0.0` — apparatus with its reason recorded, on
Phase 2 §5 items 38 and 40's precedent. Verified byte-identical: a
384-game `heuristic` bench at gain 0.0 matches the parent commit's binary
**game for game**, both `games` and `pairings` arrays equal. Workspace
tests green, clippy silent. Reports under
`target/coverage/stage-g{0.0,0.5,1.0}-seed{1,2}.json`.


## 7. The build endpoint was the worst Runner profile, and with it repaired no stance travel beats standing on it — DONE, dial still at gain 0.0 (16 September 2026)

`feat/build-endpoint-covers`. §6's handover taken: fix the build
endpoint, then re-measure the dial. Both parts landed, and the second
closes the question §6 left open in a different place from where §6
pointed.

**The endpoint was not a mediocre profile; it was the worst one.** Before
touching a weight, every Runner profile was seated against the fixed
`heuristic` Corp (`bench --bots heuristic,heuristic:X --pairing
heuristic/heuristic:X`, 384 games a seed). Corp win share against
`balanced` 0.143 / 0.133 on seeds 1–2, `cautious` 0.151 / 0.172,
`aggressive` 0.185 / 0.164 — and **`builder` 0.331 / 0.336**. §6's dial
was travelling *from* a profile that loses a third of its games *toward*
one that loses a sixth, past the balanced midpoint that beat both. That
was the whole of its cost, and the reading §6 gave of the same numbers
("what a fixed midpoint of a moving dial looks like against its own
ends") is withdrawn in `stage_weights`' doc comment.

**One knob carried it, found by ablation rather than by reasoning.** A
throwaway environment override on `Personality::Builder` (never
committed) swept each of its three terms alone, two seeds × 384:

| variant | Corp win share |
|---|---|
| as shipped (presence 1.6, memory 0.8, coverage 4.0) | 0.331 / 0.336 |
| all three at balanced — the control | 0.148 / 0.130 |
| `board_presence_weight` 1.0 | 0.247 / 0.242 |
| `board_presence_weight` 0.6 | 0.130 / 0.138 |
| `memory_weight` 0.25 | 0.260 / 0.253 |
| `breaker_coverage_weight` 5.0 | 0.326 / 0.357 |
| `savings_shortfall_weight` 0.9 | 0.320 / 0.352 |
| `held_card_weight` 0.25 | 0.302 / 0.307 |

Presence is monotone down to about 0.6 and flat below it (0.2: 0.123,
0.4: 0.123, 0.6: 0.125 over four seeds); memory, coverage, savings and
the held-card fraction are inert or worse once it is fixed (memory 0.25:
0.119, 0.5: 0.120; savings 0.6: 0.121; coverage 3.0: 0.107, lower on four
seeds of four but inside the band). **The change is
`board_presence_weight` 1.6 → 0.4 and nothing else** — exactly
`own_credit_weight`, so a rig card that breaks nothing is worth the credit
it costs and no more.

**Why a flat presence bonus wrecks a builder, which is the part worth
keeping.** At 1.6 a 1[c] resource installs at 1.6 − 0.4 = +1.2, and the
click that saves for a breaker in grip is worth 0.4 + 0.3 shortfall =
+0.7, so `SAVINGS_SHORTFALL_WEIGHT`, which exists to win exactly that
decision, loses it.
`diag tempo`, seed 1, 192 games, static profile:

| `builder` | installs / game | credit clicks | credits at turn 3 | rig at turn 5 | coverage at turn 5 |
|---|---|---|---|---|---|
| presence 1.6 | 3.7 | 6.4 | **3.5** | 1.98 | 1.22 |
| presence 0.4 | 1.7 | 13.6 | 4.8 | 1.14 | **1.12** |

The shipped profile spent its credits on the table and could not pay for
the breakers its coverage term asked for — one subtype per 1.6 rig
cards. Repaired, the rig *is* breakers (1.12 of 1.14), and the Runner
banks and runs (18.2 runs a game against 15.1). §6 said a build endpoint
"has to reward coverage rather than presence"; the measurement says the
coverage term was already big enough, and what had to go was the
presence.

**What the repair is worth, on the pinned binary** (verified game for
game against the sweep at the same setting), paired by
`scripts/paired_bench.py`:

| leg | before | after | delta | z | discordant |
|---|---|---|---|---|---|
| static `builder`, six seeds × 384 | 0.305 | **0.121** | −0.184 | 16.4 | 672 |

Against every Corp profile, four seeds × 384: `rush` 0.253 → 0.066,
`glacier` 0.301 → 0.134, `trap` 0.344 → 0.188 — more than halved in each,
so it is not tuned to the balanced Corp. `builder` goes from the worst
Runner profile to the best, below `balanced` (0.148 in the same
schedule). Three sample decks name the style (`bowel_movements`,
`party_hard`, `prick_thyself`), so this reaches deck-style play directly.

**The dial, re-measured**, `heuristic` both chairs, four seeds × 384:

| leg | before (§6 endpoint) | after | delta | z |
|---|---|---|---|---|
| gain 0.5 | 0.242 | 0.146 | −0.096 | 8.2 |
| gain 1.0 | 0.309 | 0.149 | −0.160 | 11.8 |

and against the static evaluator on the new binary, **gain 0.0 → 0.5
is +0.006 (z 0.57) and 0.0 → 1.0 is +0.009 (z 0.83)**. §6 was right that
the endpoint and not the scalar was the cost: the cost is gone. And the
travel still buys nothing. Gain 0.0 is byte-identical to `main` on all
four seeds.

**Why nothing, which is the finding.** With the scaffold again, the
dial's two endpoints were set to other profiles so that the same games
could be played with the travel going elsewhere — including nowhere:

| leg, gain 1.0 | Corp win share | against static | z |
|---|---|---|---|
| repaired `builder` all game (both endpoints `builder`) | **0.113** | −0.027 | 2.81 |
| `builder` early → `balanced` late | 0.125 | −0.015 | 1.51 |
| `balanced` early → `builder` late (inverted) | 0.137 | −0.003 | 0.39 |
| `builder` early → `aggressive` late (the dial as built) | 0.149 | +0.009 | 0.83 |
| static `balanced` | 0.140 | — | — |

Standing on the repaired endpoint beats the dial as built by 0.036 (z
3.63) and beats the inverted travel by 0.023 (z 2.65). **Every leg that
moves loses to the one that does not, in both directions**, and the legs
order by how much of the game they spend on the better profile, not by
when they spend it. On this chair and at this strength there is no
"build, *then* press": the stance that builds is also the better stance
late. §4(c)'s phase hypothesis is not supported on the Runner chair, and
`STAGE_GAIN` stays **0.0** — now for that reason rather than §6's.
(These five legs ran on the uncommitted override; the static-profile and
dial legs above did not.)

**Handed over, not taken here:**

- **`balanced`'s own `BOARD_PRESENCE_WEIGHT`.** The same knob on the
  default would be worth something like the 0.027 above to the Runner, but
  it is a constant: it moves the Corp arm too (`corp_install_value` reads
  it), which is §3's constant-sweep trap, and the Runner ladder was
  calibrated on the balanced Runner at 768 games a cell (§2), so moving it
  re-spaces the ladder. Its own branch, chair-isolated.
- **`Glacier` has the same shape of defect on the Corp chair**, a flat
  `rezzed_ice_weight` 1.8 fighting a per-cost term (§3). §3's one-knob
  repair moved the per-cost term and lost; the flat term — the analogue
  of what worked here — is untried.
- **The pressure endpoint is also a cost as a static profile**:
  `aggressive` 0.179 against balanced 0.144 over four seeds. Ablated, the
  largest single piece is `grip_floor` 2 (3: 0.158); restoring it moved
  no dial leg by more than 0.001, because the travel never reached it
  once `builder` was worth standing on.

Workspace tests green, clippy silent. Reports under
`target/coverage/builder-endpoint-{static,dial}-*.json`.


## 8. The turn-counter fallback leg: a clock does no better than the board, and neither beats not travelling — DONE, measurement only (16 September 2026)

`diag/turn-counter-stage`, stacked on §7. §4(c) said to try a board-read
stage first "and keep the turn counter as the fallback to beat", and §6
measured only the board scalar. This is the owed leg: the same `lerp`
between the same endpoints, driven by the clock instead of the board.

**The scalar**, on a scaffold in `stage_weights` that is not committed:
`((turn / 2).max(1) − 1) / (H − 1)`, clamped to `0..=1` — 0 on the
Runner's first turn, 1 from its `H`th. `turn / 2` rather than `(turn + 1)
/ 2` because `GameState::turn` counts each chair's turns separately and
the Runner's are the even ones. That way the value holds through the
Corp's next turn and cannot change between a Runner decision and the
state its `EndTurn` produces. A one-ply agent scoring leaf against leaf
would otherwise see the stance jump inside one comparison, which is the
continuity §6 insisted on. No urgency override: that reads the board,
and the point of the leg is a scalar that reads nothing. The horizon
is a free parameter a clock needs and the board does not, so it was
swept rather than chosen: 4, 8, 12 and 16 Runner turns, against games
that run about 11. Both destinations §7 measured were run: `aggressive`
(the dial as built) and `balanced` (the best travel §7 found).

`heuristic` both chairs, repaired `builder` as the build endpoint, four
seeds × 384, paired game for game. With no variable set, the scaffold
binary replays #41's dial reports exactly. With the pressure endpoint set
to `balanced`, it replays §7's builder-to-balanced leg exactly, so both
board-scalar rows below are the same games as §7's.

| leg | Corp win share | vs static 0.140 | vs board scalar | vs `builder` all game 0.113 |
|---|---|---|---|---|
| **→ `aggressive`**, board, gain 1.0 | 0.149 | +0.009 (z 0.83) | — | +0.036 (z 3.63) |
| turn, H 4 | 0.189 | +0.049 (z 4.46) | **+0.040 (z 3.56)** | +0.076 (z 7.06) |
| turn, H 8 | 0.160 | +0.020 (z 1.83) | +0.010 (z 0.95) | +0.046 (z 4.71) |
| turn, H 12 | 0.153 | +0.013 (z 1.25) | +0.004 (z 0.38) | +0.040 (z 4.22) |
| turn, H 16 | 0.163 | +0.023 (z 2.17) | +0.014 (z 1.30) | +0.049 (z 5.22) |
| turn, H 12, gain 0.5 | 0.135 | −0.005 (z 0.49) | −0.014 (z 1.29) | +0.022 (z 2.30) |
| **→ `balanced`**, board, gain 1.0 | 0.125 | −0.015 (z 1.51) | — | +0.012 (z 1.45) |
| turn, H 4 | 0.138 | −0.002 (z 0.25) | +0.013 (z 1.42) | +0.025 (z 2.88) |
| turn, H 8 | 0.116 | −0.024 (z 2.80) | −0.009 (z 1.06) | +0.003 (z 0.34) |
| turn, H 12 | 0.116 | −0.024 (z 2.68) | −0.009 (z 1.06) | +0.003 (z 0.34) |
| turn, H 16 | 0.124 | −0.016 (z 1.81) | −0.001 (z 0.15) | +0.010 (z 1.44) |
| turn, H 12, gain 0.5 | 0.128 | −0.012 (z 1.48) | +0.003 (z 0.36) | +0.015 (z 1.74) |

**What it says.**

- **The clock never beats the board.** Its best showing against the board
  scalar is −0.014 at z 1.29, and it is the best of ten turn legs, so a
  selection effect by construction. The one resolved difference goes
  the other way: a short clock, full pressure by turn 4, costs 0.040
  (z 3.56). That is §5's inverted Runner again, forced by a schedule.
- **Nothing that travels beats standing still.** The closest any leg
  comes to `builder` all game is +0.003 (turn → `balanced` at H 8 or 12).
  The legs that beat the static evaluator (z 2.7–2.8) do it by the same
  margin standing on `builder` does, while spending most of a game there.
  That is §7's finding again, not a new one.
- **Toward `aggressive`, every leg loses to standing still** (z 2.3 to
  7.1), whether the scalar is board or clock, full or half gain.

So §4(c)'s ordering, board first and clock as the fallback, has now run
both halves on the Runner chair, and neither schedule earns a non-zero
`STAGE_GAIN`. On this chair and at this strength the lever was never
*when*. It was that the build endpoint was broken (§7), and with it
fixed the best schedule is none. The phase question stays open for the
Corp chair, where §5 found the stance flat rather than inverted and no
leg has run.

No behaviour change. The only code touched is the `STAGE_GAIN` doc
comment, which now records this leg. Reports live in the session
scratchpad and are summarised above. They are not kept under
`target/coverage/`, because the scaffold that produced them is not in the
tree.


## 9. The Corp profiles audited: `glacier` was burying its hand, and repaired it is the strongest Corp — DONE (16 September 2026)

`feat/corp-profile-audit`. §7's method, applied to the Corp chair. Each
Corp profile was seated against the fixed `heuristic` balanced Runner,
and each profile's knobs were put back to balanced one at a time on a
throwaway override that is not committed. Six seeds × 384, paired game
for game, Corp win share (higher is a stronger Corp).

**Where the profiles stood.** `trap` 0.212, `glacier` 0.168, `balanced`
0.148, `rush` 0.090. The order has moved since §3, where balanced
(0.198) beat `glacier` (0.174): the Runner got stronger in between, and
`glacier` now beats balanced by 0.020 (z 2.2) in the same games.

**`glacier`**, knob by knob against the shipped profile:

| knob, shipped → tried | Corp win share | delta | z |
|---|---|---|---|
| `unrezzed_install_weight` 1.2 → 1.0 | 0.185 | +0.017 | |
| `unrezzed_install_weight` 1.2 → **0.9** | 0.240 | **+0.072** | 7.3 |
| `unrezzed_install_weight` 1.2 → 0.8 / 0.7 | 0.239 / 0.238 | +0.071 | 7.2 |
| `unrezzed_install_weight` 1.2 → 0.4 or 0.0 | **0.000** | | |
| `agenda_protection_weight` 1.0 → 0.5 | 0.141 | −0.026 | 4.1 |
| `rezzed_ice_weight` 1.8 → 1.4 | 0.158 | −0.010 | 3.1 |
| `own_credit_weight` 0.5 → 0.4 | 0.158 | −0.009 | 1.1 |
| `advancement_weight` 1.2 → 1.5 | 0.164 | −0.003 | 0.5 |
| `agenda_protection_cap` 3 → 2 | 0.168 | 0.000 | |

**The install weight is a switch, and the tempo instrument says what it
switches.** The step falls between 1.0 and 0.9, which is where a
face-down install out of a thin HQ, `unrezzed_install_weight` −
`hq_shortfall_weight` 0.5, stops beating the profile's own credit click
at `own_credit_weight` 0.5. `diag tempo`, 384 games, seed 1:

| `glacier` | installs, turn 2 | credit clicks, turn 2 | face down, turn 3 | credits, turn 3 | installs / game | scores / game | turns / game |
|---|---|---|---|---|---|---|---|
| 1.2 | 2.29 | 0.16 | 2.32 | 3.6 | 13.9 | 0.9 | 12.2 |
| 0.9 | 1.17 | 0.86 | 1.42 | 4.6 | 13.8 | 1.2 | 14.0 |

At 1.2 the Corp put its hand face down on turns 2 and 3 with no money
to rez it. At 0.9 it banks first and installs the same number of cards
over the game, just later, when it can pay. That is §5's "credit-starved
early" (credits at turn start 5.0 → 3.2) written into a profile, and it
is the lever §3's `HELD_CARD_WEIGHT` experiment pointed at without
reaching: the Corp is short of credits, not cards, so the fix is to stop
spending them on cards. Raising `hq_shortfall_weight` to 0.8 while
leaving the install weight alone recovers **0.216**, about two thirds of
the effect. So the floor decision is most of the switch. The likeliest
remainder, not separately measured, is the other install at the same
−0.5 offset: a second unprotected ICE on a server, whose 1[c] install
cost puts it level with a credit click. At 0.4 and below nothing unprotected is
installed, agendas included, and the Corp never scores (the Runner wins
all 384 games on points).

**Protection was already the engine, and it wanted more.** With the
switch set at 0.8: `agenda_protection_weight` 1.0 → 0.239, 1.5 → 0.249,
2.0 → 0.265, **3.0 → 0.282**, 4.0 → 0.282, 6.0 → 0.273, 10.0 → 0.268.
The cap is inert (2: 0.288, 4: 0.281). Without the switch, protection
4.0 alone reads 0.217, so the two effects are close to additive.

**§3's named defect is withdrawn as a cost.** §3 read `glacier` rezzing
the cheapest ICE of any Corp profile as its flat `rezzed_ice_weight` 1.8
fighting a per-cost credit term, and took that for the defect. Measured,
the flat term *helps*: back at 1.4 the profile loses 0.010 (z 3.1), and
2.4 reads 0.170. The rez-rate pattern may be real. What cost games was
the install weight, which §3 never touched.

**The change:** `unrezzed_install_weight` 1.2 → **0.8** and
`agenda_protection_weight` 1.0 → **3.0**. On the pinned binary, which
replays the sweep game for game on all six seeds:

| leg | before | after | delta | z | discordant |
|---|---|---|---|---|---|
| `glacier` vs balanced Runner, six seeds × 384 | 0.168 | **0.282** | +0.114 | 10.9 | 585 |

Against every Runner profile, four seeds × 384: `aggressive` 0.200 →
0.322, `cautious` 0.180 → 0.287, `builder` (§7's repair) 0.134 → 0.231,
`wary` 0.173 → 0.286. In the same games it beats `trap` by **0.070
(z 6.1)**, which makes it the strongest Corp profile in the pool. A test
now pins the switch: the profile's install from the floor must not beat
its own credit click. Balanced-vs-balanced games are byte-identical to
`main`.

**It survives search, which §3's terms did not.** §3's better scoring
terms lifted the one-ply `operator` and gave `puct@512` +0.000. The
same `puct@512` that the `veteran` and `elite` rungs are built on,
un-handicapped, seated as Corp against the one-ply Runner, one seed ×
384 (sampling sd about 0.024 a leg), paired:

| Corp, `puct@512` | Corp win share | delta | z | discordant |
|---|---|---|---|---|
| `balanced` | 0.245 | — | | |
| `glacier` shipped | 0.284 | +0.039 over balanced | 1.5 | 95 |
| `glacier` repaired | **0.357** | +0.073 over shipped | 2.5 | 122 |
| | | +0.112 over balanced | 4.0 | 115 |

One seed, so it is the direction and rough size, not a calibration. But
it matters for §4(b): the "better leaf buys search nothing" finding was
about terms that encode a decision a tree can find by looking ahead. A
likely reason this one differs, not tested: bank now to rez later pays
off turns away, past what a 512-simulation search reaches. The top of
the Corp ladder has a lever that is not more search. The run needed
`--threads 10`: the first attempt at full parallelism, alongside a
workspace build, was killed for memory.

**`rush`, audited, not repaired.** It loses to balanced by 0.058 (z
7.0), and the cost sits in the two knobs that make it a rush:
`installed_agenda_weight` 1.0 → 0 is +0.041 (z 5.8), `advancement_weight`
2.5 → 1.5 is +0.037 (z 5.5). Nothing else moves it beyond noise except
the install switch at 0.6 (+0.019, z 2.5), because its `own_credit_weight`
of 0.3 puts its switch lower. Against this Runner, "score early" is the
defect rather than a knob inside it. A rush that wins would be a
different archetype, which is a design question and not a tuning one.

**`trap`, audited, unchanged.** Its ambush terms are its whole value:
`ambush_weight` 3.0 → 0 costs 0.044 (z 7.7) and the cap 7 → 3 costs
0.013 (z 4.8), and `ambush_advancement_weight` 2.8 → 1.0 is within noise
(+0.009, z 2.0). `glacier`'s switch carried over (install 0.8 with
credits 0.5) reads +0.023 (z 2.4), inside the seed-spread band, and the
profile's own test pins it as "its ambush terms and nothing else", so it
is recorded and not taken.

**Handed over:**

- **A Corp that is both.** `trap`'s ambush terms and `glacier`'s
  install switch and protection come from different profiles and were
  never measured together. The two best Corp levers found so far could
  compose, or could fight over the same clicks.
- **Where this reaches.** The calibrated ladder rungs are
  `Personality::Balanced` (`difficulty.rs`) and are untouched. But local
  and server play seat a bot with no chosen personality in its *deck's*
  style (`netrunner_client::play`, `netrunner_server::serve`), and five
  Corp decks name `glacier` (`agency`, `brick_stack`, `discretion_advised`,
  `hidden_funds`, `pork_chops`). So a person meets this Corp at every
  rung with those decks, and the rung's calibration does not describe
  it. Whether rungs should be calibrated per style, or the Corp's top
  rung should *be* this profile (§4(b)), is the next decision.
- **The same switch on balanced.** The balanced Corp at install 0.8 reads
  0.160 against 0.148 (+0.012), a smaller effect because its credit
  click is worth 0.4. As a constant it moves the Runner's reading of the
  Corp board too: §3's trap again.

Workspace tests green, clippy silent. Reports under
`target/coverage/glacier-audit-{before,after}-s{1..6}.json`.

## 10. The Corp ladder in `glacier` style: every rung that reads the style gains, and `operator → veteran` goes flat — DONE, measurement only (16 September 2026)

`diag/glacier-corp-ladder`. §9 handed over that the rungs are calibrated
as `Balanced` while play seats a rung in its deck's style, and five Corp
decks are `glacier`. This measures what that person meets. `bench` could
not seat a styled rung (a `level:` spec was always `Balanced`), so it now
takes `level:elite:glacier` — the same `with_personality` cross a seat
makes — and rates it under that id; `scripts/ladder_report.py --style`
reads one.

Each Corp rung, `Balanced` and `glacier`, against the fixed un-handicapped
one-ply balanced Runner (`heuristic`), 384 games a cell on seeds 1 and 2.
The two arms share one bot-list layout, so they play the same matchups on
the same seeds and are paired game for game. No stalls in any cell.

| Corp rung | `Balanced` | `glacier` | delta | z (McNemar) | discordant |
|---|---|---|---|---|---|
| novice | 0.012 | 0.012 | +0.000 | | 0 |
| apprentice | 0.062 | 0.092 | +0.030 | 2.4 | 91 |
| operator | 0.125 | **0.257** | **+0.132** | 7.2 | 195 |
| veteran | 0.236 | 0.277 | +0.042 | 2.1 | 226 |
| elite | 0.258 | **0.363** | **+0.105** | 5.0 | 259 |

`novice` is byte-identical, which is the apparatus check: at `epsilon`
1.0 the inner agent, and so its style, is never consulted.

**The style is worth as much at the top as at one ply**: `elite`
(`puct@512`) gains +0.105 against `operator`'s +0.132, and `glacier`
`elite` at 0.363 is the strongest Corp rung measured since the Runner's
evaluator moved the pool. §9's one-seed `puct@512` reading (0.245 →
0.357) reproduces on both seeds.

**It is `veteran` that does not carry it.** Steps, pooled over 768 games
a cell (sd of the difference in brackets):

| step | `Balanced` | `glacier` |
|---|---|---|
| novice → apprentice | +0.051 (0.010) | +0.081 (0.011) |
| apprentice → operator | +0.062 (0.015) | **+0.164** (0.019) |
| operator → veteran | **+0.111** (0.019) | **+0.021 (0.023), flat** |
| veteran → elite | +0.022 (0.022) | +0.086 (0.024) |

A person playing a `glacier` deck meets a `veteran` no harder than
`operator` — flat pooled, and inverted on seed 2 alone (0.271 → 0.255) —
and then a large step to `elite`. `veteran` is `elite` with `epsilon`
0.10, so the one knob between them costs `glacier` 0.086 and `Balanced`
0.022: the handicap bites the style roughly four times harder. A likely
reason, not tested: a banking plan pays off turns later, and a random
action every tenth decision spends the credits or buries the card it was
banking for, where a balanced Corp's plan is shorter and survives it.

**The `Balanced` ladder's top step is now the weak one.** `veteran →
elite` is +0.022 at z 1.0 — `rise` by the script's one-sd rule, `flat` on
seed 1 alone (+0.005) — where §2 recorded +0.045. Its `operator` also
reads 0.125, against the 0.182 §4(a) quotes. The reference is not the
cause: `level:elite` and `heuristic` on the Runner chair produce the same
winner in 384 of 384 games, and `operator` against either reads 0.148 /
0.130 on its own schedule — in line with §7's and §9's 0.148. §4(a)'s
column was taken before §7–§9 and does not reproduce on this binary; it
should be re-measured before (a) interpolates from it.

**What this decides for §4.** Per-style calibration is needed, not
optional: the order survives the style (no step inverts pooled), but the
spacing does not, and the rung a `glacier` deck makes meaningless is a
different one from the rung that is weakest as `Balanced`. Two routes,
neither taken here: give `veteran` a per-style `epsilon` (on these
numbers `glacier` wants a smaller one, since 0.10 already costs most of
its top step), or build the top rungs on `glacier` itself (§4(b)) and
calibrate that one ladder, since as a Corp base it is +0.105 on `elite`
and the strongest top the chair has.

Reports under `target/coverage/corp-ladder-{balanced,glacier}-s{1,2}.json`.

## 11. `glacier`'s `veteran` Corp gets its own handicap: `epsilon` 0.02, and the glacier ladder climbs evenly at the top — DONE (16 September 2026)

`feat/glacier-veteran-epsilon`. §10 found the `glacier` Corp's
`operator → veteran` step flat (+0.021) because `veteran`'s `epsilon`
0.10 costs this style 0.086 against `Balanced`'s 0.022. Of §10's two
routes this takes the per-style handicap: `LevelSpec::with_personality`
now sets `epsilon` for `(Veteran, Corp, Glacier)` and reads every other
pairing back off `Level::spec`, so every seat (TUI, desktop, daemon,
`bench`) gets it and no other rung or style moves.

**The measurement.** `glacier` `veteran` against the same one-ply
balanced Runner, 384 games on each of seeds 1 and 2 on §10's schedule, so
every leg is paired with §10's `epsilon` 0.10 games. The value was set on
a throwaway environment override that is not committed; the control leg
at 0.10 replayed §10 game for game.

| `epsilon` | `glacier` `veteran` | vs 0.10 (z) | `operator → veteran` | `veteran → elite` |
|---|---|---|---|---|
| 0.10 (was) | 0.277 | | +0.021 (z 0.9) | +0.086 |
| 0.07 | 0.257 | −0.020 (−1.2) | +0.000 | +0.107 |
| 0.05 | 0.277 | +0.000 | +0.021 | +0.086 |
| 0.03 | 0.301 | +0.024 (1.2) | +0.044 (1.9) | +0.062 (2.6) |
| **0.02** | **0.309** | +0.032 (1.6) | **+0.052 (2.3)** | **+0.055 (2.3)** |
| 0.01 | 0.324 | +0.047 (2.3) | +0.068 (2.9) | +0.039 (1.6) |
| 0.00 (`elite`) | 0.363 | | | |

**The curve is not linear, and not the `Balanced` Corp's either.** From
0.10 down to 0.05 it does not move beyond noise, and the whole climb to
`elite` happens below 0.03. So an interpolation from §2's `Balanced`
inversion (0.17 → 0.10) would have landed on the flat part and changed
nothing a person could feel. A likely reading, not tested: a banking plan
survives no blunder at all, so any rate above a few in a hundred costs
it the whole plan, and only a very rare blunder leaves most games whole.

**0.02 is the measured midpoint** (target 0.310 between `operator`
0.257 and `elite` 0.363). Both steps are rises pooled and on each seed
alone: `operator → veteran` +0.068 / +0.036, `veteran → elite` +0.062 /
+0.047. The `glacier` Corp ladder now reads **0.012 / 0.092 / 0.257 /
0.309 / 0.363**, steps +0.081 / +0.164 / +0.052 / +0.055. Its uneven step
is now `apprentice → operator`, which is a one-ply base at 0.35 and 0.0
and is the same shape as the `Balanced` ladder's; that is (a)'s, not
this entry's.

`LevelSpec::describe` said "throws away about 0 decisions in 10" at this
rate; below 0.1 it now says "about one decision in 50". Tests pin that
only this pairing carries its own handicap, that styling twice cannot
leak it into another style, and that the ladder is monotone in every
style rather than only `Balanced`.

Not done: the other Corp styles (`trap`, `rush`) and every Runner style
still ride `Balanced`'s spacing unmeasured, and `Balanced`'s own top step
(+0.022, z 1.0 in §10) is untouched.

## 12. The Corp ladder in `rush` style has no top: search buys it +0.012, so no handicap can space it — DONE, measurement only (16 September 2026)

`diag/rush-corp-ladder`. `rush` is the most common Corp style a person
meets (6 of the 16 sample Corp decks) and the weakest Corp profile (§9,
−0.058 against balanced), so after §10–§11 it was the style most likely to
break the ladder's spacing. §10's method on §10's schedule: each rung as
`rush` against the fixed one-ply balanced Runner, 384 games on each of
seeds 1 and 2, paired game for game with the `Balanced` reports. No stalls.

| Corp rung | `Balanced` | `rush` | delta | z (McNemar) | discordant |
|---|---|---|---|---|---|
| novice | 0.012 | 0.012 | +0.000 | | 0 |
| apprentice | 0.062 | 0.061 | −0.001 | −0.1 | 73 |
| operator | 0.125 | 0.100 | −0.025 | −1.8 | 117 |
| veteran | 0.236 | 0.109 | **−0.126** | −7.3 | 179 |
| elite | 0.258 | 0.112 | **−0.146** | −8.1 | 190 |

`novice` is byte-identical again, as it must be.

| step | `Balanced` | `rush` (s1 / s2) |
|---|---|---|
| novice → apprentice | +0.051 | +0.049 (+0.057 / +0.042) |
| apprentice → operator | +0.062 | +0.039 (+0.018 / +0.060) |
| operator → veteran | +0.111 | **+0.009** (+0.016 / +0.003), flat |
| veteran → elite | +0.022 | **+0.003** (+0.018 / −0.013), flat |

**The style costs one ply little and the search nearly everything.**
`operator` loses 0.025 to the style (not significant), but `puct@512` as
`rush` scores 0.112 against one ply's 0.100, where as `Balanced` the same
search is worth +0.133. So `veteran` and `elite` are the same opponent as
`operator` to a person playing a rush deck, and the planned repair — a
per-style `epsilon`, as §11 gave `glacier` — cannot apply: the span it
would space is 0.012 against a sampling sd of 0.016, and `epsilon` only
takes strength away. The search rungs do play differently (games 365 → 295
steps a game, 5 → 18 flatlines a seed, both `operator` → `elite`); they do
not win more.

This is §9's `rush` finding seen from the ladder. §9 put the profile's cost
in the two knobs that make it a rush (`installed_agenda_weight` 1.0,
`advancement_weight` 2.5) and read "score early" as the defect against
this Runner. A likely reading, not tested: a deeper search over that leaf
finds the rush line more reliably and the line itself does not win, so
the leaf caps the search. §3's "a better leaf does not lift search" does
not run in reverse.

**Handed over, not done.** Three routes, each a decision rather than a
tuning (*taken in §15: the second*):

- **Repair `rush`** so its search converts — §9 called a rush that wins a
  different archetype, which makes this a design question.
- **Seat the search rungs in another style for rush decks** (e.g. the
  deck's style up to `operator`, `Balanced` or `glacier` above it). That
  keeps the ladder but tells the player the top of it is not a rush.
- **Say so**: the start screen could mark a rush deck's top rungs as
  equivalent. Honest, and no stronger.

Reports under `target/coverage/corp-ladder-rush-s{1,2}.json`.

## 13. The Corp ladder in `trap` style: stronger than `Balanced` at every rung, and its flat top step is `Balanced`'s own — DONE, measurement only (16 September 2026)

`diag/trap-corp-ladder`. The last Corp style (2 of 16 sample Corp decks:
`advanced_yomi`, `peculiarity`). §10's method on §10's schedule: each rung
as `trap` against the fixed one-ply balanced Runner, 384 games on each of
seeds 1 and 2, paired game for game with the `Balanced` reports. No stalls.

| Corp rung | `Balanced` | `trap` | delta | z (McNemar) | discordant |
|---|---|---|---|---|---|
| novice | 0.012 | 0.012 | +0.000 | | 0 |
| apprentice | 0.062 | 0.085 | +0.022 | 2.8 | 37 |
| operator | 0.125 | 0.189 | +0.064 | 6.2 | 63 |
| veteran | 0.236 | 0.284 | +0.048 | 4.3 | 75 |
| elite | 0.258 | 0.297 | +0.039 | 3.3 | 82 |

| step | `Balanced` (s1 / s2) | `trap` (s1 / s2) |
|---|---|---|
| novice → apprentice | +0.051 | +0.073 (+0.078 / +0.068) |
| apprentice → operator | +0.062 | +0.104 (+0.078 / +0.130) |
| operator → veteran | +0.111 | +0.095 (+0.112 / +0.078) |
| veteran → elite | +0.022 (+0.005 / +0.039) | **+0.013 (−0.003 / +0.029), flat** |

**`trap` is `Balanced` plus its ambush terms, and the games say so.** The
two arms disagree on only 257 of 3,840 games, and the two `trap` decks
carry 119 of them (`advanced_yomi` 81, `peculiarity` 38): the terms read
ambush cards, and a deck without them plays close to balanced. Unlike
`rush` (§12) the style survives search — +0.039 at `elite` — and unlike
`glacier` (§10) it does not break the `operator → veteran` step.

**Its one flat step is not the style's, so it gets no handicap of its
own.** `veteran → elite` is +0.013 for `trap` and +0.022 for `Balanced`,
flat on seed 1 in both (−0.003 and +0.005): the same `epsilon` 0.10 on
the same `puct@512` costs both about as little. A `trap`-only `epsilon`
would space this one style around a defect every unstyled game shares,
and would stop inheriting the fix once the base spec is re-spaced. That
fix is §4(a)'s — re-measure `Balanced`'s `veteran` `epsilon` curve and
move it — and `trap`, reading `Level::spec` for every rung, follows it.
§11's `glacier` arm stays the one exception, because there the step was
the style's (0.086 of cost against 0.022).

**Where the four Corp ladders stand**, 768 games a cell, all against the
same Runner:

| rung | `Balanced` | `glacier` (§11) | `rush` | `trap` |
|---|---|---|---|---|
| novice | 0.012 | 0.012 | 0.012 | 0.012 |
| apprentice | 0.062 | 0.092 | 0.061 | 0.085 |
| operator | 0.125 | 0.257 | 0.100 | 0.189 |
| veteran | 0.236 | 0.309 | 0.109 | 0.284 |
| elite | 0.258 | 0.363 | 0.112 | 0.297 |

So the owed Corp work is two items, both already named: `Balanced`'s
`veteran → elite` step (§4(a), which `trap` inherits) and `rush`'s missing
top (§12, a decision).

*Corrected in §14:* `trap` did not inherit the re-spacing. At `Balanced`'s
new 0.20 its `operator → veteran` step went flat, so it carries its own
`epsilon` (0.15).

Reports under `target/coverage/corp-ladder-trap-s{1,2}.json`.

## 14. `Balanced`'s `veteran` Corp plays at `epsilon` 0.20, and every other Corp style needed its own — DONE (16 September 2026)

`feat/balanced-veteran-epsilon`. §4(a), on the numbers §10 re-took rather
than the stale ones it quotes. `Balanced`'s Corp ladder read 0.012 /
0.062 / 0.125 / 0.236 / 0.258: the lower three rungs sit on an even step
of 0.062 already, and `operator` is not out of place (§4(a)'s 0.182 does
not reproduce). The rung out of place is `veteran`, at 0.236 against a
midpoint of 0.192, leaving `veteran → elite` at +0.022 (z 1.0) — a step a
person cannot feel.

**The measurement.** `veteran` as the Corp against the fixed one-ply
balanced Runner, 384 games on each of seeds 1 and 2, `bench --pairing
level:veteran/heuristic` inside §10's bot list, so every game keeps §10's
index and seed and pairs with it. The value was set through an
uncommitted environment override on one pinned binary; the control leg at
0.10 replayed §10 in 768 of 768 games (winner and step count). No stalls
in any leg.

| `epsilon` | `veteran` | vs 0.10 (z McNemar) | `operator → veteran` | `veteran → elite` |
|---|---|---|---|---|
| 0.10 (was) | 0.236 | | +0.111 (z 5.7) | +0.022 (z 1.0) |
| 0.15 | 0.219 | −0.017 (−1.0) | +0.094 | +0.039 |
| 0.18 | 0.176 | −0.060 (−3.2) | +0.051 | +0.082 |
| **0.20** | **0.180** | −0.056 (−3.1) | **+0.055 (z 3.0)** | **+0.078 (z 3.7)** |
| 0.25 | 0.146 | −0.090 (−5.0) | +0.021, flat | +0.112 |
| 0.30 | 0.111 | −0.125 (−7.2) | −0.014, inverted | +0.147 |

**The curve drops steeply between 0.15 and 0.18 and is flat from there to
0.20** — the third Corp handicap curve measured, and the third shape
(glacier's was flat from 0.10 to 0.05 and climbed only below 0.03). 0.20
is the round number on the midpoint: both steps rise on each seed alone
(+0.049 / +0.060 and +0.081 / +0.076). The `Balanced` Corp ladder now
reads **0.012 / 0.062 / 0.125 / 0.180 / 0.258**, steps +0.051 / +0.062 /
+0.055 / +0.078.

**§13 was wrong that `trap` would inherit it.** A styled rung reads
`Level::spec`'s handicap unless it carries its own, so both styles still
riding it were re-measured at 0.20 on §12's and §13's schedules:

| style | `operator` | `veteran` at 0.10 | at 0.20 | `elite` | `operator → veteran` at 0.20 |
|---|---|---|---|---|---|
| `trap` | 0.189 | 0.284 | 0.199 (z −4.5) | 0.297 | +0.010, flat; −0.013 on seed 2 |
| `rush` | 0.100 | 0.109 | 0.083 (z −2.2) | 0.112 | **−0.017, inverted on both seeds** |

0.20 costs `trap` 0.085 where it costs `Balanced` 0.056 — §13 compared the
styles at 0.10, where the handicap barely bites either, and read the
shared flat step as one defect. So both get an entry in
`LevelSpec::with_personality`, as `glacier` did in §11:

- **`trap` plays at 0.15.** Measured at 0.10 / 0.12 / 0.15 / 0.20: 0.284 /
  0.263 / 0.232 / 0.199, target 0.243. 0.15 is nearest, with steps +0.043
  (z 2.1) and +0.065 (z 2.9) pooled. **Not every seed agrees**: at 0.15
  `operator → veteran` is +0.016 on seed 2, and at 0.12 `veteran → elite`
  is +0.010 on seed 1. The style's whole `operator → elite` span is 0.108,
  so two steps of it sit inside one seed's noise at 384 games; no value
  clears §11's each-seed-alone bar, and more `epsilon` points would not
  change that. Ladder: 0.012 / 0.085 / 0.189 / 0.232 / 0.297.
- **`rush` keeps 0.10.** It is not spaced at 0.10 either — §12's
  `puct@512` is worth +0.012 to this style and no handicap can space that
  — but 0.10 leaves `veteran` level with `operator` and 0.20 puts it below.
  A rung easier than the one beneath it is worse than a flat one. The
  ladder stays §12's 0.012 / 0.061 / 0.100 / 0.109 / 0.112 until §12's
  decision is taken.

`glacier` (0.02, §11) was never riding the base value and is unchanged.
Every Corp style but `Balanced` now carries its own `veteran` handicap,
which says the base value is a calibration of one style rather than of the
chair. The test pins all three and that no other rung or style moves.

**Where the four Corp ladders stand**, 768 games a cell against the same
Runner:

| rung | `Balanced` | `glacier` | `rush` | `trap` |
|---|---|---|---|---|
| novice | 0.012 | 0.012 | 0.012 | 0.012 |
| apprentice | 0.062 | 0.092 | 0.061 | 0.085 |
| operator | 0.125 | 0.257 | 0.100 | 0.189 |
| veteran | **0.180** | 0.309 | 0.109 | **0.232** |
| elite | 0.258 | 0.363 | 0.112 | 0.297 |

Owed Corp ladder work is now `rush`'s missing top (§12, a decision) and
§4(b)–(c). Reports under `target/coverage/veteran-eps-*.json`.

*Superseded for `rush` in §15:* its top two rungs now play `Balanced`, so
its `veteran` entry here is gone.

## 15. A `rush` deck's top two Corp rungs play `Balanced`: its ladder climbs, and the top of it is not a rush — DONE (16 September 2026)

`feat/rush-top-rungs-play-balanced`. §12's decision, taken by the person:
of its three routes, **seat another style above `operator`**.
`LevelSpec::with_personality` now hands back `Balanced`'s own rung for
`(Veteran | Elite, Corp, Rush)`, so every seat (TUI, desktop, daemon,
`bench`) gets it, and `rush`'s §14 `epsilon` entry is gone with it.

**Why it was needed, seen on the decks that seat it.** §12 measured the
style over all sixteen Corp decks, but play only seats `rush` on the six
rush decks (an unset style is the deck's own). Read from §12's and §14's
reports on those decks alone, 288 games a cell, the `rush` ladder did not
just stall — **it ran backwards**: `operator` 0.101, `veteran` 0.062,
`elite` 0.035. A deeper search over the rush leaf loses more.

**Why `Balanced` rather than `glacier`.** `Balanced`'s search rungs are
already calibrated (§14), and from `rush`'s `operator` they climb in two
near-even steps; `glacier`'s `veteran` (0.309 over all decks, measured at
0.10 on rush decks as 0.260) would be one jump from 0.100.

| rung | all 16 decks | the 6 rush decks |
|---|---|---|
| novice (`rush`) | 0.012 | 0.000 |
| apprentice (`rush`) | 0.061 | 0.035 |
| operator (`rush`) | 0.100 | 0.101 |
| veteran (was `rush`) | 0.109 → **0.180** | 0.062 → **0.149** |
| elite (was `rush`) | 0.112 → **0.258** | 0.035 → **0.271** |

Over all decks the steps from `operator` are +0.080 and +0.078; on rush
decks +0.048 and +0.122. **`Balanced`'s `epsilon` is kept, not re-fitted
to rush decks**: at 0.15 those decks read 0.198, which would even their
two steps (+0.097 / +0.073), but over all decks — the method every other
cell uses — 0.15 does the opposite (+0.119 / +0.039), and a separate value
would rest on 288 games a cell, whose step noise (sd ≈ 0.03) is the size
of the difference. So a rush deck's top rungs are `Balanced`'s exactly.

**The apparatus check.** A pinned binary benched `level:veteran:rush` and
`level:elite:rush` on §10's bot layout, 384 games on each of seeds 1 and
2: each replays `Balanced`'s games (§14's `veteran` at 0.20, §10's
`elite`) in **768 of 768** — winner, step count and matchup. No stalls.

**The cost, on the record.** The top of a rush deck's ladder does not
play like a rush, and nothing on the start screen says so yet: its rung
list is drawn from the unstyled spec before a style is resolved. The
`rush` profile itself is unrepaired, and still costs its one-ply rungs
nothing measurable (`operator` 0.101 on rush decks against `Balanced`'s
0.118, inside the noise).

Reports under `target/coverage/rush-top-balanced-s{1,2}.json`.

## 16. The Runner profiles audited: `aggressive` was getting itself flatlined, and `cautious` and `wary` have nothing to repair — DONE (16 September 2026)

`feat/runner-profile-audit`. §9's method, on the Runner chair §7 left
three-quarters unaudited: each Runner profile seated against the fixed
`heuristic` balanced Corp (`bench --bots heuristic,heuristic:X --pairing
heuristic/heuristic:X`, six seeds × 384), each knob put back to balanced
one at a time on an uncommitted override, paired game for game on
`(game seed, matchup)`. Corp win share, so **lower is a stronger Runner**.
The override was checked against itself first: `aggressive` with every
knob at balanced replays balanced's games 2,304 of 2,304.

**Where the profiles stood.** `builder` 0.121 (§7's repair, −0.027 against
balanced, z 3.7), `balanced` 0.148, `wary` 0.148 (+0.000, z 0.0),
`cautious` 0.151 (+0.003, z 0.4), **`aggressive` 0.188 (+0.040, z 4.4)**.
Five sample decks name `aggressive`, the most of any Runner style.

**`aggressive` was paying for its pressure in damage.** Its Corp wins were
**140 flatlines** of 434, where balanced's were 36 of 341. Knob by knob
against the shipped profile:

| knob, shipped → balanced | Corp win share | delta | z |
|---|---|---|---|
| `grip_floor` 2 → 3 | 0.168 | −0.020 | 3.8 |
| `grip_shortfall_weight` 0.4 → 0.7 | 0.178 | −0.010 | 3.1 |
| both | **0.154** | **−0.034** | 5.6 |
| `active_run_weight` 1.2 → 0.6 | 0.191 | +0.003 | |
| `pending_subroutine_weight` 0.7 → 1.0 | 0.186 | −0.002 | |
| `savings_shortfall_weight` 0.15 → 0.3 | 0.200 | +0.012 | |
| `tag_weight` 2.5 → 4.0 | 0.191 | +0.003 | |
| `opponent_credit_weight` 0.4 → 0.2 | 0.188 | 0.000 | |

With the grip back at balanced, nothing else beat noise against the
balanced Corp (run 0.6: 0.155, 0.9: 0.162, 1.8: 0.171; savings 0.0: 0.157;
tag 1.5: 0.151; subroutine 0.4: 0.178; grip floor 4: 0.153).

**Against the other Corp profiles a second knob showed.** Four seeds × 384
each, on the grip repair: `pending_subroutine_weight` back to 1.0 is
−0.011 against `rush` (z 3.1) and −0.010 against `glacier` (z 2.3); and
`active_run_weight` back to 0.6 is **−0.041 against `rush`** (z 4.4) and
−0.023 against `glacier` (z 1.8). The run weight is the archetype — a
Runner that does not run more is not this profile — so it is recorded as
the profile's cost, as `rush`'s were in §9, and not taken. 0.9 was
tried as a midpoint and trades `glacier` (−0.022) for `trap` (+0.008)
without closing `rush`.

**The change:** `grip_floor` 2 → 3, `grip_shortfall_weight` 0.4 → 0.7
and `pending_subroutine_weight` 0.7 → 1.0 — all three back to balanced,
so the profile is now its run weight, its savings, its tags and the
opponent's credits. On the pinned binary, which replays the override 0
discordant of 2,304 and 1,536 on every leg:

| Corp | before | after | delta | z | against balanced Runner, before → after |
|---|---|---|---|---|---|
| `balanced`, six seeds | 0.188 | **0.155** | −0.033 | 5.3 | +0.040 → +0.007 |
| `rush`, four seeds | 0.155 | 0.118 | −0.036 | 4.7 | +0.073 → +0.036 |
| `glacier`, four seeds | 0.322 | 0.299 | −0.023 | 2.6 | +0.042 → +0.018 |
| `trap`, four seeds | 0.260 | 0.218 | −0.042 | 4.8 | +0.048 → +0.007 |

Flatlines 140 → 58. **It is still aggressive**: `diag tempo`, 384 games,
seed 1 — runs a game 21.6 → 20.7 against balanced's 17.9, credit clicks
9.8 → 10.5, draws 3.0 → 3.2. Balanced-against-balanced games are
byte-identical to `main`.

**`cautious`, audited, unchanged.** No knob costs it beyond noise: run
0.4 → 0.6 −0.007 (z 0.8), subroutine, grip weight, savings, coverage and
tag all within ±0.003. The one knob that matters helps: its `grip_floor`
4 back to 3 costs **+0.015** (z 2.2), and 5 reads the same as 4. It is
the safety profile its doc comment says, and it plays level with balanced.

**`wary`, audited, unchanged.** Its one term, `unrezzed_threat_weight`
1.5, is level with balanced (208 discordant games, net zero); 0.75 is
identical, and 3.0 and 6.0 cost +0.007 and +0.009 (z 1.8, 2.3). It
changes which games the Runner wins, not how many.

**Where it reaches.** Every Runner rung is the one-ply bot, so a style
reaches every rung its deck seats; §17 measures that ladder.

Workspace tests green, clippy silent. Reports under
`target/coverage/runner-audit-*.json`.

## 17. The Runner ladder in every Runner style climbs at every step, so no Runner style needs a handicap of its own — DONE, measurement only (16 September 2026)

Same branch as §16, on its pinned binary. The Runner half of §10–§15:
every Runner rung in each of the five styles (`bench --bots
heuristic,level:novice:S,…,level:elite:S --pairing heuristic/level:R:S`)
against the fixed un-handicapped one-ply balanced Corp, 384 games on
each of seeds 1–4, so **1,536 games a cell**. One bot-list layout per
style, so the arms play the same matchups on the same seeds. No stalls.
Runner win share, so **higher is a harder rung for the Corp player**:

| Runner style | novice | apprentice | operator | veteran | elite | steps |
|---|---|---|---|---|---|---|
| `balanced` | 0.126 | 0.281 | 0.454 | 0.664 | 0.829 | +0.156 / +0.173 / +0.210 / +0.165 |
| `aggressive` | 0.126 | 0.294 | 0.472 | 0.632 | 0.831 | +0.168 / +0.178 / +0.160 / +0.199 |
| `cautious` | 0.126 | 0.275 | 0.443 | 0.644 | 0.827 | +0.149 / +0.168 / +0.201 / +0.184 |
| `builder` | 0.126 | 0.259 | 0.461 | 0.657 | **0.870** | +0.133 / +0.202 / +0.196 / +0.214 |
| `wary` | 0.126 | 0.279 | 0.447 | 0.656 | 0.839 | +0.154 / +0.168 / +0.209 / +0.183 |

**Every step of every style is a rise at z ≥ 9.0, and on each of the four
seeds alone** — no step is flat or inverted in any of the 80 per-seed
readings. The narrowest is `builder`'s `novice → apprentice`, +0.133
against an even step of about 0.176. `novice` is byte-identical across
styles, the apparatus check (at `epsilon` 1.0 the style is never
consulted), and `balanced` reproduces §2's calibration (0.833 at `elite`
then, 0.829 now).

**Why the Runner chair did not need what the Corp chair needed.** §10
found a style broke the Corp's spacing because that ladder changes base
between rungs 3 and 4 (one ply → `puct@512`) and the handicap on the
search rungs bit a banking style four times harder. The Runner ladder is
one base bot at five handicaps, `epsilon` is near-linear in win rate on
this chair (§2), and a style moves the whole line by about as much as it
moves any one rung — so the steps survive it.

**On the decks that seat each style** (play seats a Runner rung in its
deck's style, §9), read from the same reports: `aggressive` on its five
decks 0.120 / 0.320 / 0.494 / 0.647 / 0.841 (640 games a cell),
`builder` on its three 0.115 / 0.224 / 0.419 / 0.635 / 0.846 (384),
`wary` on its two 0.121 / 0.258 / 0.465 / 0.668 / 0.801 (256) — every
step a rise at z ≥ 3.4 and on each seed. `cautious` has one deck, 128
games a cell: 0.188 / 0.289 / 0.484 / 0.617 / 0.875, every step a rise
pooled (the first at z 1.9, as is `balanced`'s on the same deck, 1.6),
one step flat on one seed of 32 games.

**§16's repair reaches the rungs, and most at the top.** `aggressive`
before and after, paired on the same games: `apprentice` 0.282 → 0.294
(z 2.0), `operator` 0.445 → 0.472 (z 3.2), `veteran` 0.598 → 0.632
(z 3.4), **`elite` 0.782 → 0.831** (z 5.9). Before it, a person playing
the Corp against one of the five `aggressive` decks met an `elite` 0.047
softer than the calibrated one; now it is level with it. The shipped
profile's ladder climbed too (smallest step +0.154), so the repair
changed how hard the top is, not whether the ladder was one.

**Nothing changes in `difficulty.rs`.** No Runner entry is added to
`LevelSpec::with_personality`, and the Runner ladder's per-style spacing
is measured rather than assumed.

Reports under `target/coverage/runner-ladder-{style}-s{1..4}.json` and
`runner-ladder-aggressive-before-s{1..4}.json`.

## 18. A Corp with both `glacier`'s fort and `trap`'s ambush terms is the strongest one-ply Corp, and search takes the gain back — DONE, measurement only (17 September 2026)

`diag/corp-fort-and-ambush`. §4(b)'s candidate and §9's handover: the
two best Corp levers found so far — `glacier`'s install switch and
protection (§9) and `trap`'s three card-reading ambush terms (Phase 3
§1) — came from different profiles and had never been measured together.
`glacier`'s weights plus `trap`'s `ambush_weight` 3.0,
`ambush_advancement_weight` 2.8 and `ambush_advancement_cap` 7, on an
uncommitted override, against the fixed one-ply balanced Runner, paired
game for game on `(game seed, matchup)`. Corp win share, so higher is a
stronger Corp.

**At one ply they compose** (six seeds × 384):

| Corp | Corp win share | against it | z | discordant |
|---|---|---|---|---|
| `balanced` | 0.148 | | | |
| `trap` | 0.212 | | | |
| `glacier` | 0.282 | | | |
| **`glacier` + ambush** | **0.326** | +0.045 over `glacier` | 6.7 | 239 |
| | | +0.115 over `trap` | 10.5 | 632 |

It wins both ways: 22.7% of games on agendas (`glacier` 25.8%) and 9.9%
by flatline (`trap` 8.6%, `glacier` 2.3%). **`ambush_weight` is a
switch, and it is nearly all of it**: alone on `glacier` it is +0.040
(z 6.5), flat from 1.5 to 8.0 (0.318 / 0.322 / 0.326 / 0.325), while the
advancement term alone *costs* `glacier` −0.013 (z 2.6) and at 1.5 or
2.8 on top of the switch moves nothing (0.324, 0.326; 5.0 with 2.8 reads
0.330). `trap`'s own finding ran the other way — the advancement term was
its engine — because `trap` has no fort for an ambush to sit beside.
Against every Runner profile, four seeds × 384, it is +0.039 to +0.053
over `glacier` (z ≥ 5.0) and +0.092 to +0.134 over `trap`: `aggressive`
0.299 → 0.352, `cautious` 0.287 → 0.326, `builder` 0.231 → 0.279, `wary`
0.286 → 0.334.

**Under search it does not survive.** The `elite` rung, `puct@512`
un-handicapped (`bench --bots level:elite:glacier,heuristic`, `--threads
10`), two seeds × 384 on one layout:

| `elite` Corp | seed 1 | seed 2 | pooled | against `glacier` | z | discordant |
|---|---|---|---|---|---|---|
| `balanced` | 0.245 | 0.276 | 0.260 | | | |
| `glacier` | 0.357 | 0.357 | 0.357 | | | |
| `glacier` + ambush | 0.362 | 0.378 | **0.370** | **+0.013** | 1.1 | 88 |

Flatlines rise 33 → 80 of 768 and agenda wins fall 241 → 204: the search
does play the ambushes it is told to value, and it wins no more often for
it. This is §3's finding again from a new lever — a term that encodes a
decision a tree can find by looking ahead lifts the one-ply rungs and
not the search rungs — and it contrasts with §9's install switch, which
did survive `puct@512` (+0.073 on one seed, reproduced here as `glacier`
+0.096 over `balanced`, z 4.7, on both seeds). A plausible reason, not
tested: an ambush pays off the moment the Runner accesses it, inside a
512-simulation horizon, where banking to rez later pays off turns away.

**What it decides for §4(b): nothing ships.** The combination does not
raise the Corp's ceiling, which is what §4(b) is for. And adding the
terms to `glacier` would break §11's calibration from below: `glacier`'s
`operator` is one ply and would gain about +0.045 toward its `veteran`'s
0.309, where `veteran`'s `epsilon` is already 0.02 and has no room left to
restore the step. The one top-rung lever §4(b) has measured stays §9's:
`glacier` at `elite` is +0.096 over `balanced` on both seeds, and the
decks whose top rungs do not play it (`balanced`, `trap`, and `rush` since
§15) are the open question — a choice about how a deck's top rung plays,
not a tuning.

Reports under `target/coverage/fort-ambush-{c,x,e}-*.json`.


## 19. `glacier` builds the fort it is named for: centrals first, one scoring remote, then the agenda — DONE (22 September 2026)

`feat/glacier-builds-its-fort`, from a report from play. The person was
the Runner against `discretion_advised`, a `glacier` deck, at `operator`
(one ply, no handicap). They reported that the Corp "makes ICE for days
horizontally across as many servers as it can without ever once putting
down an agenda". What they wanted is the fort the profile is named for:
HQ and R&D one to two deep, Archives at least one, then one remote one to
two deep, and *then* an agenda in it, played to win.

**First, the phase dial (#134, closed unmerged).** §4(c) on the Corp
chair was measured before this report arrived: `stage_weights` staged
from `glacier` under a fort-reading scalar. Every leg that travelled lost
to standing on `glacier`, six seeds × 384 paired:

- to `balanced`: −0.072 (z 7.1), and −0.016 at half gain
- to `rush`: −0.086 (z 8.4)
- to `trap`: −0.030 (z 2.8)
- the inverted leg (`balanced` until the fort is up, then `glacier`):
  +0.003, a tie

One piece of remote ICE was a third of the travel, so the staged Corp
pushed earlier rather than later. The person's rule is "if it doesn't
move it, don't merge it", so the dial was closed and §4(c) is closed on
both chairs. The report explains why a dial could not help: the static
profile it travelled from had no notion of *where* a fort goes, so there
was nothing for the dial to sequence.

**The instrument: `netrunner_cli diag fort`.** It reads the Corp's
installs off the state after every step, by `InstallId`, so an install
from any source counts. It reports:

- the ICE on an agenda's server at the moment the agenda arrived
- the ICE on each central at the first agenda
- the remotes opened, and the remotes that ever got ICE
- the ICE per central and per remote
- agendas installed and scored
- the Corp win share

`--matchup CORP/RUNNER` pins the reported games.

**The report was true, and the cause was a missing vocabulary.** On
`main`, `heuristic:glacier` (which is `operator`) against the balanced
Runner, six seeds × 384:

- 23% of agendas were installed behind no ICE, and only 30% behind two.
- At the first agenda, all three centrals were iced in 14% of games and
  none in 37%.
- About 0.7 pieces of ICE went on each central, and ICE landed on 3.2
  remotes a game.

The agendas were installed (2.9 a game), but naked or one deep in a new
remote. No Corp term knew which server a piece of ICE was on:

- `corp_install_value` prices ICE the same way everywhere.
- `protected_agenda_ice` counts only the ICE in front of an agenda that
  is already installed.
- So placement fell to tie-breaking, and a naked agenda install (+0.8)
  outbid the profile's own credit click (+0.5).

**Three terms, all `glacier`'s** (`eval::fort_value`). They are 0.0 in
every other profile, which is held by
`only_glacier_prices_where_its_ice_stands`.

- `central_ice_weight` 2.0 a piece: up to `central_ice_cap` 2 on HQ and
  R&D, and one on Archives.
- `fort_weight` 1.5 a piece: up to `fort_cap` 2 on the deepest remote
  whose root is empty or holds an agenda. It is priced before the agenda
  exists, and it stays priced after the agenda is scored.
- `exposed_agenda_weight` 5.0: subtracted per missing piece, per
  installed agenda.

At one ply, the order is:

- a first piece on HQ: 2.8
- the fort's first piece: 2.3
- a piece in front of an asset: 0.8
- an agenda behind the finished fort: 6.8
- the credit click: 0.5
- a naked agenda: −9.2

The grid ran on a pinned binary with an uncommitted override, heuristic
Corp against the balanced Runner, two seeds × 384 paired against `main`:

| central / fort / exposed | Corp | Δ | behind ≥2 | all centrals at first agenda |
|---|---|---|---|---|
| `main` | 0.219 | | 0.28 | 0.16 |
| 2 / 1.5 / 0 | 0.180 | −0.039 | 0.27 | 0.55 |
| 0 / 1.5 / 1.5 | 0.298 | +0.079 | 0.59 | 0.06 |
| 2 / 0 / 1.5 | 0.318 | +0.099 | 0.33 | 1.00 |
| 2 / 1.5 / 1.5 | 0.370 | +0.151 | 0.43 | 1.00 |
| 1 / 1.5 / 1.5 | 0.421 | +0.202 | 0.64 | 0.15 |
| 2 / 1.5 / 3 | 0.410 | +0.191 | 0.55 | 1.00 |
| **2 / 1.5 / 5** | **0.499** | **+0.280** | **1.00** | **1.00** |
| 2.5 / 2 / 4 | 0.497 | +0.279 | 1.00 | 1.00 |
| 2 / 1.5 (fort cap 3) / 3 | 0.492 | +0.273 | 1.00 | 1.00 |

**The exposure term carries it.** Without it the other two cost 0.039,
because a fort nobody waits for is only ICE. Below about 4.0, an agenda
still goes down one deep in half its installs. A weaker central term
(1.0) wins as much but starts agendas before the centrals are iced,
which is not the order the person asked for, so it was not taken. The
neighbours of the shipped point play the same games, so it sits on a
plateau.

**Shipped, six seeds × 384 against the balanced Runner, paired:**

| | `main` | `glacier` with the fort |
|---|---|---|
| Corp win share | 0.247 | **0.462** (+0.215, z 16.6) |
| agendas behind ≥1 / ≥2 ICE | 0.77 / 0.30 | **1.00 / 1.00** |
| all three centrals iced at the first agenda | 0.14 | **1.00** |
| ICE installed on HQ / R&D / Archives a game | 0.70 / 0.70 / 0.72 | 1.79 / 1.80 / 0.98 |
| remotes given ICE a game | 3.16 | **2.18** |
| agendas installed / scored a game | 2.93 / 1.25 | 2.28 / **1.68** |

Against every other Runner profile, two seeds × 384 each: `aggressive`
+0.227, `cautious` +0.223, `builder` +0.233, `wary` +0.237 (z ≥ 10.0).

**Search keeps it, and the `glacier` ladder is inverted at the top.** The
search rungs play `glacier` through `puct@512` (`bench --bots
level:veteran:glacier,level:elite:glacier,heuristic --threads 10`),
against the balanced one-ply Runner, seed 1 × 384, paired:

| `glacier` rung | `main` | with the fort | Δ | z | discordant |
|---|---|---|---|---|---|
| `operator` (one ply; `diag fort`'s schedule) | 0.216 | **0.495** | +0.279 | | |
| `veteran` (`puct@512`, ε 0.02) | 0.190 | 0.349 | +0.159 | 5.5 | 121 |
| `elite` (`puct@512`) | 0.214 | 0.354 | +0.141 | 4.9 | 122 |

Unlike §18's ambush terms, search keeps most of this gain. The fort pays
off over turns, beyond a 512-simulation horizon, like §9's install
switch did. But the rungs no longer climb. **They already did not on
`main`**: §11 set `glacier`'s ladder at `operator` 0.257, `veteran`
0.309, `elite` 0.363. Re-taken today, on the engine the rules work has
changed since, it is 0.216 / 0.190 / 0.214. The first figure comes from
`diag fort` and the other two from `bench`: both are 384 games over the
same pool on seed 1, but in a different order. So before this entry `glacier`'s
`operator` was already no weaker than its search rungs, and the fort
opens that into a gap. The person chose to ship the fort and rebuild the ladder
separately. That is §4(b) with a measured answer: `glacier`'s top rungs
should be the one-ply fort Corp at small handicaps, as the Runner ladder
already is (one ply at five handicaps), with `operator` handicapped to
sit below them. Seed 2 of this table is being taken, and it goes in
that PR.

**A dev hook that stalls, found while screenshotting** (not fixed here).
`NETRUNNER_AUTOPLAY` presses entry `applied % len` of the legal-action
list. At a card-selection prompt (Mutual Favor) that cycles select and
deselect without confirming, until the session's 256-decision stall
guard fires. The notice ends the autoplay short of its count, so the
screenshot is never taken and the window looks hung. A person's clicks
cannot reach it. It is its own fix.

**Corrected 22 September 2026 (Phase 7 §4ag).** The person reported this
livelock message while watching the hook, and was first told it had not
really happened. It had: seed 6 of the default decks reproduces it
exactly. The loop is fixed in §4ag. The window did not only *look* hung:
the hook waited for a count a stopped match can never reach, so it
never took the shot and never exited. A stopped match now ends the
autoplay (`the_autoplay_is_done_when_the_match_stops`). "A person's clicks
cannot reach it" was never checked and should not have been written.

Reports are under `target/coverage/fort/`.

## 20. Traps: hidden until sprung, played like an agenda, and not run into twice — DONE (22 September 2026)

From a report from play. The Corp should play a trap like an agenda: ICE
in front of it, advance it, and never rez it, because a trap turned face
up is a known quantity the Runner simply avoids. The Runner should not
run back into a trap it has already sprung. And a trap that cannot be
advanced (Snare!, Byte!) belongs in HQ and R&D, not in a remote. The
engine already fires Urtica Cipher, Snare! and Byte! face down on access
(the Listener Rule's "the subject always hears it"). So springing a trap
is the access itself, and a rez only ever gives it away.

**The instrument: `netrunner_cli diag trap`** (`diag/trap.rs`). It tracks
each trap install from the step it arrives, and splits traps into two
kinds, read off the card:

- a *lure* trap grows with its tokens (`eval::damage_grows_with_advancement`,
  Urtica Cipher);
- a *hand* trap cannot take a token (Snare!, Byte!).

Per install it records whether the trap was rezzed before any access, the
ICE in front at install and at the first access, its peak tokens, its
accesses and repeat accesses, whether the Runner trashed it, and the
damage it dealt. That damage is the Corp's damage between the trap's
access and the next card accessed, so Snare!'s paid damage on a later
action still counts. For hand traps met in HQ, R&D and Archives it
records the access and whether the Corp could pay. It also counts the
Runner's runs on empty remotes, and on remotes whose root is only traps
it has already seen: the engine forgets an access, so the diagnostic
remembers it by `InstallId`.

By default it plays the matchups whose Corp deck carries a trap: 6 decks
× 12 = 72. `--deck-styles` seats each chair in its deck's own style, as
play does. Without it a bare `heuristic` is `Balanced`, and every trap in
the pool is rezzed.

**Baseline on `main`**, heuristic against heuristic in deck styles, 216
games on each of seeds 1 and 2:

| | `advanced_yomi`, `peculiarity` (trap) | `discretion_advised` (glacier) | Byte! in `fine_print`, `glyph_of_warding`, `pork_chops` |
|---|---|---|---|
| installs, seeds 1 + 2 | 121 | 116 | 147 |
| rezzed before any access | 1 | **116** | **147** |
| ICE in front at install | 20 | 15 | 14 |
| ever accessed | 115 | 7 | 1 |
| damage dealt | 422 | 22 | 0 |

- **A trap Corp keeps its Urtica face down, and it works.** The trap
  decks' Urticas are accessed 95% of the time and deal about 3.5 net
  damage each. That is most of the pool's 0.17 flatline share, of which
  0.10 of all games end while a trap resolves.
- **Every other profile gives its trap away.** A rezzed non-ICE card is
  worth `board_presence_weight` + `rezzed_asset_weight` (2.0). A
  face-down one is worth `unrezzed_install_weight` (1.0, glacier 0.8)
  plus `ambush_weight`, which only `trap` sets. The rez costs 0, so
  Balanced, Glacier and Rush rez every trap at their first window. The
  glacier deck's 116 Urticas were accessed 7 times.
- **Traps sit naked.** Only 13% of trap installs have ICE in front.
  `fort_value` counts a remote only if its root is all agendas, so a
  glacier trap goes to a bare remote.
- **A hand trap never springs, wherever it is.** Every installed Byte!
  is rezzed first. Met in HQ or R&D (0.15 a game), the Corp could pay in
  0.11 a game and paid **0** times. `CorpPaysToApply` compares 4 credits
  against 3 net damage and a tag, and a Corp evaluator outside `trap`
  has no `opponent_grip_weight`, so the damage is worth nothing to it.
  Every played deck with a hand trap is in one of those profiles. Snare!
  is only in the `a_thousand_cuts` sweep deck, which `matchups()` never
  yields.
- **The Runner walks back into a known trap** 0.10–0.11 times a game.
  Its run valuation forgets an access, so on the next turn a sprung
  Urtica is a hidden card again, worth 1.0 more per token.

**The Runner sees the card it accessed** (`feat/runner-sees-what-it-accessed`).
`InstalledCard::seen_by_runner` is set when an installed card is
accessed, and when it is rezzed. From then on the Runner's view names it
even face down, and a spectator's does not. The flag itself is public,
because the Corp watched the access.

CR 7.3.1a keeps an accessed card visible "for the remainder of the
breach", and CR 4.6.3 makes a facedown card secret afterwards. This is
what the Runner remembers, not a card they may examine; jinteki.net
shows the same. The log's concealment predicate reads the same flag
(`corp_card_concealed_from`), so the view and the log agree.

The order of the fixes changed here, and the reason is a finding. With
traps no longer rezzed, the concealment sweep failed twice:

- A face-down Urtica firing put `TriggerFired { card: urtica_cipher }`
  in a *spectator's* log. That was a leak on `main` too, unreached only
  because every trap outside `Trap` was rezzed first. A concealed card's
  trigger is now dropped, except to the Runner for `OnAccessed`.
- A Runner flatlined by that Urtica had its access end in the same
  action. Their log then named a card their view had already hidden
  again. That is exactly the memory this flag is. So the flag lands
  before the never-rez fix, and the never-rez fix is stacked on it.

Both 256-seed sweeps are green. The one-ply heuristic plays 163 and 164
of 216 games identically on seeds 1 and 2 (win share 0.343 → 0.333 and
0.319 → 0.315): it does not read a face-down card's identity yet, which
is the Runner PR. The observation encoding sees more named cards.

**Never rez a trap, measured on pinned binaries before the reorder**
(main against the fix, six seeds × 216 games, deck styles). Traps
rezzed before any access fall from 0.44–0.51 to 0.01–0.04. The Corp in
trap matchups goes **0.311 → 0.362, +0.051 on every seed** (sd 0.014,
t 8.9). Flatlines go from 0.15 to 0.22. Repeat accesses of a trap
already sprung rise, because a trap that stays hidden is worth running
into; the Runner PR is what stops that. Re-taken on the flag
(`fix/corp-never-reveals-a-trap`, pinned, six seeds × 216): lure traps
rezzed before access 0.44–0.50 → 0.02–0.04, hand traps 1.00 → 0.00–0.03.
Corp **0.307 → 0.350, +0.043** (sd 0.016, t 6.6), higher on every seed.
Flatlines 0.15 → 0.22. Repeat accesses of a sprung Urtica go 0.13 →
0.43 per install, and runs on a remote of known traps 0.07 → 0.20 a
game: that is the next PR's to stop. `REVEALED_TRAP_WEIGHT` is priced in
the Corp's own board, not in `corp_install_value`, which is also the
Runner's reading of a face-up Corp card.

**The Runner reads the end of the server**
(`feat/runner-reads-the-end-of-the-server`). In `access_prospect` a root
card is *known* when it is rezzed **or** seen, and is then read for what
it is rather than counted as one hidden access:

- a known trap costs `known_ambush_weight` plus
  `KNOWN_TRAP_DAMAGE_WEIGHT` (0.5) per point of damage it would do — an
  Urtica with three tokens is five net damage, not the same 1.5 as a
  bare one — and `LETHAL_TRAP_WEIGHT` (50.0) instead when that damage
  would flatline (CR 1.7.2b). Its tokens stop counting as a prospect;
- a known asset is worth its trash and nothing else, and only when
  affordable;
- an unseen card is a hidden access, as before, so the Corp's bluff
  still works on a card the Runner has not met.

Six seeds × 216 against the never-rez base: repeat accesses of a sprung
trap **0.42 → 0.000**, runs on a remote of known traps **0.20 → 0.00**,
flatlines **0.22 → 0.13**, and the Corp **0.350 → 0.283** (−0.068,
t 9.7). The Corp's trap gain from not rezzing is given back once the
Runner stops donating, which is the point: both chairs now play the
card correctly.

**The ICE on the way in, measured in halves.** `run_is_breakable`
becomes `remaining_break_cost`, so one number gates the run and pays for
it:

- **Kept:** what the Runner can afford to trash at the end is what it
  has left *after* breaking in. It could break in and arrive unable to
  trash the asset it came for. On the pool this is byte-identical in
  three seeds × 192 — a correctness fix with no measured effect.
- **Rejected:** charging the run `remaining_break_cost ×
  own_credit_weight` as well. The Runner stopped running — 17.2 runs a
  game → 14.6, Corp 0.239 → 0.259 over the pool and +0.029 on the trap
  decks (t 4.4). A breach is worth more than the credits it costs,
  because the credits come back and the agenda does not. The number is
  recorded on `remaining_break_cost` rather than left as folklore.

**The Corp plays a lure trap as an agenda, and keeps a hand trap in HQ**
(`feat/corp-plays-traps-as-agendas`). Two kinds, read off the card, never
a card list (`eval::is_lure_trap` / `is_hand_trap`):

- `ambush_weight` now pays only for an unseen **lure** trap. It used to
  pay for any face-down trap, which is what installed 147 Byte!s.
- `HELD_TRAP_WEIGHT` (1.5) per **hand** trap in HQ, so the install loses
  to keeping it where the Runner meets it while hunting agendas.
- `LURE_ICE_WEIGHT` (1.0) per piece of ICE, capped at one, on a remote
  holding an unseen lure trap — for a Corp with no fort term; for
  `glacier`, `fort_value`'s root test now admits an unseen lure trap, so
  the trap goes *in* the fort instead of costing it. One piece, not two:
  a trap has to be reachable to be worth anything.
- **A sprung trap is sunk:** once `seen_by_runner` is set, neither
  `ambush_weight` nor the advancement term pays, so the Corp stops
  spending clicks on a remote the Runner will not enter.
- `OPPONENT_GRIP_SHORTFALL_WEIGHT` (1.0, floor 5) — the Corp's reading of
  a thin grip. **This is the term the "no Corp damage term" note in
  `eval` says to add once a lever exists, and the lever now exists**: a
  hand trap is a paid interaction, so springing one is a Corp action that
  makes the grip smaller. The note's measurement stands for every other
  case, which is why the term is a shortfall below a floor rather than a
  linear count.

Six seeds × 216 against the Runner PR: hand traps installed **0.35 →
0.00** a game, hand traps met in HQ/R&D **0.21 → 0.44**, and the damage
they deal **0.00 → 0.62** a game — the Corp had declined every single
paid trap before. Lure traps with ICE in front at install **0.15 →
0.31**, peak tokens 1.86 → 1.44 (the sunk rule).

**The win rate does not move**: trap decks 0.297 → 0.289 (t −0.85), the
whole pool 0.240 → 0.247 over three seeds × 192. The person was asked
and chose to merge it: the number is flat because both chairs improved
at once, and what they reported was the *play*, not the rate. Traps are
about half a card a game in this pool.

Reports are under `target/coverage/trap/`. The fixes follow as their
own PRs:

1. the Runner sees the card it accessed (masking, #137);
2. no profile rezzes a trap (#138);
3. the Runner reads the end of the server (#139);
4. the Corp plays a lure trap as an agenda, keeps a hand trap in HQ, and
   springs it (#140).

**Owed:** the Corp's `lure_ice_weight` and `held_trap_weight` are the
first trap terms every profile carries, so the Corp ladder's trap rungs
(§13, §14) were calibrated on the old behaviour and should be re-taken
before they are trusted.

## 21. `glacier`'s Corp ladder is one ply at five handicaps, and it climbs at every step — DONE (22 September 2026)

`feat/glacier-corp-ladder-is-one-ply`, the follow-up §19 owed and the
first of §4(b)'s answers. §19 left `glacier`'s ladder inverted at the
top: one ply at `operator` beat both search rungs, `puct@512` at
`veteran` (ε 0.02) and `elite`, on both seeds (768 games a cell against
the one-ply balanced Runner):

| `glacier` rung on `main` | seed 1 | seed 2 | pooled |
|---|---|---|---|
| `novice` (random) | 0.010 | 0.018 | 0.014 |
| `apprentice` (one ply, ε 0.35) | 0.042 | 0.052 | 0.047 |
| `operator` (one ply) | 0.461 | 0.385 | **0.423** |
| `veteran` (`puct@512`, ε 0.02) | 0.349 | 0.336 | 0.343 |
| `elite` (`puct@512`) | 0.354 | 0.391 | 0.372 |

The three one-ply rows are today's `main`. The two search rows are §19's
run (`target/coverage/fort/ladder/`), taken before the trap PRs
(#141–#144): they were not re-taken because they no longer ship, and
nothing in §20 made a search Corp stronger than one ply.

The fort pays off over turns, past a 512-simulation horizon, so search
cannot keep what one ply does (§19). More handicap on `puct` could only
push those rungs further down, and nothing on it lifts `elite` above one
ply. **So the style's ladder takes the Runner chair's shape: one base,
five handicaps.** `LevelSpec::with_personality` gives a `glacier` Corp
`LevelKind::Heuristic` at every rung. It now reads the base and the
handicap back off `Level::spec` too, so restyling a `glacier` rung to
`Balanced` gives `puct@512` back; the first draft kept the one-ply base,
and the new test caught it.

**The curve**, one ply `glacier` against the one-ply balanced Runner, two
seeds × 384 on one pinned binary with an uncommitted override:

| ε | 0 | .03 | .04 | .05 | .10 | .12 | .15 | .20 | .22 | .25 | .30 | .40 | .50 | .60 | .75 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Corp | 0.443 | 0.365 | 0.329 | 0.311 | 0.251 | 0.208 | 0.189 | 0.135 | 0.121 | 0.096 | 0.079 | 0.049 | 0.030 | 0.031 | 0.016 |

It is steep near 0 and nowhere near linear, so the handicaps were read off
it for four even steps from random to ε 0, not fitted: **1.0 / 0.22 /
0.11 / 0.05 / 0.0.**

**Confirmed on the shipped table** (`bench`, `ladder_report.py --style
glacier --reference heuristic`):

| `glacier` rung | seed 1 | seed 2 | pooled | step |
|---|---|---|---|---|
| `novice` | 0.013 | 0.010 | 0.012 | |
| `apprentice` (ε 0.22) | 0.154 | 0.130 | 0.142 | +0.130 |
| `operator` (ε 0.11) | 0.219 | 0.227 | 0.223 | +0.081 |
| `veteran` (ε 0.05) | 0.349 | 0.310 | 0.329 | +0.107 |
| `elite` (one ply) | 0.422 | 0.438 | 0.430 | +0.101 |

Every step is a `rise` on each seed alone. **`veteran` was first set at
0.04**, which read 0.372 in this run against the sweep's 0.329. The top
step was then +0.058, only 1.7 sd on each seed, so it moved to 0.05. The
ladder's top is now 0.430 against `main`'s best rung of 0.423, which was
`operator`. Its middle is the part that moved: the old `apprentice` at ε
0.35 sat at 0.047, a step of +0.376 below `operator`.

A person will also notice the speed: no `glacier` Corp rung searches, so
`elite` answers as fast as `operator` did. `describe()` reads the base
off the spec, so the start screen and the record's "next" line say "looks
one move ahead" with no change of their own.

**Still owed:** §20's re-take of `Balanced`'s and `trap`'s rungs, which
the trap terms moved. That is overnight work, because those rungs search.
Reports are under `target/coverage/glacier-ladder/`.

## 22. The ladders re-taken after the trap terms: the Corp's `apprentice` moves to 0.25, and no handicap can space `Balanced`'s or `trap`'s top — DONE (23 September 2026)

`feat/corp-ladder-retaken-after-traps`, the re-take §20 and §21 owed. Every
cell is 384 games on each of two seeds (four for the Runner chair) against
the fixed un-handicapped one-ply opponent on the other chair, on pinned
builds of `main` at `6769002`, with an uncommitted `NETRUNNER_EPS` /
`NETRUNNER_SIMS` override for the sweeps. Reports are under
`target/coverage/{corp-ladder-retake,ladder-retune,style-matrix}/`. No stalls.

**The Corp ladders on `main`** (Corp win share):

| rung | `Balanced` | §14 | `trap` | §13/§14 |
|---|---|---|---|---|
| `novice` | 0.012 | 0.012 | 0.012 | 0.012 |
| `apprentice` (ε 0.35) | 0.048 | 0.062 | 0.052 | 0.085 |
| `operator` | 0.155 | 0.125 | 0.165 | 0.189 |
| `veteran` (ε 0.20 / 0.15) | **0.156** | 0.180 | **0.160** | 0.232 |
| `elite` | 0.223 | 0.258 | 0.212 | 0.297 |

`veteran` had become `operator` in both styles, on each seed alone.

**`apprentice` → 0.25.** The curve is flat and noisy between 0.35 and
0.15 (`Balanced` 0.064 / 0.062 / 0.082 / 0.077 / 0.091, `trap` within
0.006 at each), and 0.25 is the first value at the midpoint (0.083). On
the shipped table: `Balanced` 0.012 / **0.081** / 0.155, `trap` 0.012 /
**0.086** / 0.165, `rush` 0.012 / **0.051** / 0.082 — every step below
`operator` a rise at z ≥ 2.5 and on each seed alone.

**`veteran` is left where it was, because no setting spaces it.**
`operator → elite` is only 0.068 (`Balanced`) and 0.047 (`trap`):

| `veteran` | 0.12 | 0.10 | 0.08 | 0.05 | 0.03 | 0.02 | `puct@128` | `puct@256` |
|---|---|---|---|---|---|---|---|---|
| `Balanced` | 0.160 | 0.168 | 0.181 | 0.167 | 0.194 | 0.197 | 0.161 | 0.169 |
| `trap` | 0.163 | 0.169 | 0.173 | 0.184 | 0.199 | 0.203 | 0.173 | 0.185 |

Any handicap costs `puct@512` most of its edge, and a smaller budget at
none is worse. The best candidate, 0.03 shared by both styles (the person's
choice for `trap`), was confirmed on the shipped table and **failed**: it
read 0.208 / 0.211 there against the sweep's 0.194 / 0.199, putting the top
step at +0.014 (z 0.7) and +0.001. At this span two reads of one setting
land it on `operator` or on `elite` by chance. The person chose to ship
`apprentice` alone and leave `veteran` at 0.20 / 0.15. **A fifth Corp rung
needs a stronger `elite`, not a handicap** (§4(b)).

**`glacier` still holds** (§21's table, re-taken): 0.012 / 0.134 / 0.227 /
0.333 / 0.431, within 0.008 of §21 at every rung, every step a rise on
each seed.

**The Runner ladder needs nothing.** All five styles still climb at every
step, and each step rises on each of four seeds alone (`balanced` 0.225 /
0.399 / 0.562 / 0.736 / 0.863; the other styles within 0.035 at every
rung). `novice` rose from §17's 0.126 to 0.225. A 768–1,536-game scan over
`main`'s history put the rise on two merges: #127, the turn order (+0.017),
and **#128, a [click] ability is an action (+0.057, about 5 sd)**. Before
#128 the random Runner could spend clicks on Smartware Distributor and
Pennyshaver in every paid ability window, and did (1,066 and 259
activations over 192 games, against 139 and 30 after). With that option
gone it ran 16% more (3,695 → 4,303 runs) and stole 20% more. The heuristic
Corp's own actions moved by under 7%. So a correct rule made the random
baseline stronger, and no bot changed. The trap PRs (#141–#144) sit in the
flat stretch of that scan.

**The style matrix, which is where the top rung is** (one ply, both
chairs, 1,536 games a cell). Corp win share, Corp style against Runner style:

| Corp \ Runner | balanced | aggressive | cautious | builder | wary |
|---|---|---|---|---|---|
| `glacier` | 0.466 | 0.454 | 0.464 | 0.475 | 0.469 |
| `trap` | 0.136 | 0.170 | 0.150 | 0.139 | 0.155 |
| `Balanced` | 0.137 | 0.156 | 0.119 | 0.128 | 0.152 |
| `rush` | 0.090 | 0.125 | 0.077 | 0.098 | 0.109 |

Grouped by the Corp *deck's* own style, played as `glacier` against as its
own style: `Balanced` decks 0.344 against 0.106, `trap` 0.481 against 0.164,
`rush` 0.463 against 0.092. No deck's own style is its best one. The Runner
styles sit within 0.07 of each other on every deck group. §19's fort terms
look like general Corp strength rather than a style, so 11 of 16 Corp decks
are played by a much weaker bot than they could be. **Owed next, one of:**
the fort terms in every Corp profile, or `glacier` seated as every deck's
top rung. Either changes these ladders again. Not yet ruled out: every
opponent here is the one-ply Runner, and `glacier` may be exploiting its
play against ICE rather than playing better.

## 23. Where the fort goes is how a Corp plays, not a style: every Corp profile builds one — DONE (23 September 2026)

`feat/fort-in-every-corp-profile`, the top-rung work §22 left owed
(§4(b)). §19's three fort terms (`central_ice_weight` 2.0 to two pieces
on HQ and R&D and one on Archives, `fort_weight` 1.5 to two pieces on the
scoring remote, `exposed_agenda_weight` 5.0 a piece short of it) were
`glacier`'s alone, and §22's style matrix put `glacier` at 0.466 against
0.100–0.150 for every other Corp style.

**First, the confound §22 named: it is not an exploit of the one-ply
Runner.** The same Corps against two Runners that play ICE differently,
768 games a cell (seeds 1–2, `puct@128`), pinned §22 binary:

| Corp | random Runner | `puct@128` Runner |
|---|---|---|
| `glacier` | **0.954** | **0.596** |
| `rush` | 0.853 | 0.154 |
| `trap` | 0.814 | 0.229 |
| `Balanced` | 0.759 | 0.220 |

**Then the terms, unchanged and alone, in `Balanced`, `trap` and
`rush`** — the §22 matrix re-played game for game (one ply, both chairs,
seeds 1–4 × 384, 1,536 games a cell, sd about 0.011). Corp win share,
mean over the five Runner styles:

| Corp style | §22 | with the fort terms |
|---|---|---|
| `Balanced` | 0.138 | **0.421** |
| `trap` | 0.150 | **0.427** |
| `rush` | 0.100 | **0.439** |
| `glacier` (untouched — the control) | 0.466 | 0.466 |

Every one of the fifteen changed cells rose, against every Runner style;
the `glacier` row matches §22 to the third decimal, which is what says
the games pair. By the Corp deck's own style, played in that style:
`Balanced` decks 0.106 → 0.349, `trap` decks 0.164 → 0.439, `rush` decks
0.092 → 0.409. `glacier` still leads on the `trap` (0.481) and `rush`
(0.463) decks, by 0.04–0.05 rather than 0.3.

**Rush takes all three**, the person's choice. The exposure term says an
agenda waits in HQ for a two-deep remote, which is the opposite of "the
agenda goes on the table first" — and the style that did put it there
first was the weakest Corp in the workspace. What still makes it `rush`
is what it does once the fort stands: it advances where `glacier` adds a
third piece (`a_rush_corp_advances_where_a_glacier_corp_installs_ice`,
moved from a naked agenda to a finished fort).

**How it is written.** The terms are the balanced constants
(`eval::CENTRAL_ICE_WEIGHT`, `FORT_WEIGHT`, `EXPOSED_AGENDA_WEIGHT`), so
`Balanced` is still `Weights::default()` and every profile inherits them;
`every_corp_profile_prices_where_its_ice_stands` holds every Corp profile
to the one set of values. That reaches two callers the matrix did not
measure: the gym's shaped reward for a Corp agent and `diag
leaf-sensitivity`, both on the default weights. **§20's lure term is
removed**: it priced ICE in front of an unseen lure trap "for a Corp with
no fort term", `fort_value` already counts such a remote as the fort, and
with a fort term everywhere it could not fire (it was already off in all
four measured profiles).

Reports and readers are under `target/coverage/style-confound/` and
`target/coverage/fort-all/`, pinned binary `target/pinned/fort-all-A`.

**Owed next: every Corp ladder is re-taken.** `operator` is one ply in
the deck's style, so a non-`glacier` deck's `operator` rose from about
0.1 to about 0.4 here, and §22's `veteran`/`elite` rungs for `Balanced`
and `trap` (`puct@512`) were calibrated around a leaf that did not build
a fort. Whether those styles now want §21's shape — one ply at five
handicaps — is the first question for that PR.

## 24. Every Corp ladder is one ply at five handicaps, `glacier`'s, and no Corp style carries an exception — DONE (23 September 2026)

`feat/every-corp-ladder-is-one-ply`, the re-take §23 owed. Every cell is
384 games on each of two seeds against the fixed un-handicapped one-ply
balanced Runner. The before binary is a pinned build of §23 (`8dace25`);
the sweep binary has an uncommitted `NETRUNNER_EPS` override. Reports and
readers are under `target/coverage/{ladder-s23,ladder-s24}/`. No stalls.

**Once every profile built the fort, one ply beat both search rungs in
every style**, as it had beaten `glacier`'s in §19. The shipped table on
§23 (Corp win share):

| rung | `Balanced` | `trap` | `rush` |
|---|---|---|---|
| `novice` | 0.012 | 0.012 | 0.010 |
| `apprentice` (ε 0.25) | 0.132 | 0.118 | 0.133 |
| `operator` (one ply) | **0.426** | **0.410** | **0.422** |
| `veteran` (`puct@512`, ε 0.20 / 0.15) | 0.151 | 0.178 | `Balanced`'s |
| `elite` (`puct@512`) | 0.324 | 0.326 | `Balanced`'s |

`rush`'s cells are one seed, and its top two rungs were `Balanced`'s by
§15. The `operator → veteran` step is −0.275 and −0.232, at z > 10.

**The one-ply `epsilon` curve is `glacier`'s in every style**, so the
handicaps §21 read off it serve all four:

| ε | 0.30 | 0.22 | 0.15 | 0.11 | 0.08 | 0.05 | 0.03 |
|---|---|---|---|---|---|---|---|
| `Balanced` | 0.086 | 0.128 | 0.172 | 0.225 | 0.268 | 0.331 | 0.368 |
| `trap` | 0.079 | 0.109 | 0.193 | 0.221 | 0.272 | 0.320 | 0.359 |
| `rush` | 0.090 | 0.139 | 0.212 | 0.246 | 0.319 | 0.358 | 0.402 |
| `glacier` (§21) | 0.079 | 0.121 | 0.189 | — | — | 0.311 | 0.365 |

**So the Corp's `Level::spec` is `glacier`'s table: one ply at 1.0 / 0.22
/ 0.11 / 0.05 / 0.0.** `LevelSpec::with_personality` now changes the style
and nothing else. Its three Corp exceptions had nothing left to except:
`trap`'s own `veteran` ε (§14), `rush`'s top two rungs played as `Balanced`
(§15, so the top of a rush deck's ladder is a rush again), and `glacier`'s
own base and handicaps (§21). **Confirmed on the shipped table:**

| rung | `Balanced` | `trap` | `rush` | `glacier` |
|---|---|---|---|---|
| `novice` | 0.012 | 0.012 | 0.012 | 0.012 |
| `apprentice` (ε 0.22) | 0.139 | 0.122 | 0.139 | 0.134 |
| `operator` (ε 0.11) | 0.214 | 0.221 | 0.258 | 0.227 |
| `veteran` (ε 0.05) | 0.324 | 0.322 | 0.327 | 0.333 |
| `elite` (one ply) | 0.431 | 0.431 | 0.409 | 0.431 |

Every step in every style is a rise on each seed alone, and pooled at z ≥
3.0. The thinnest is `rush`'s `operator → veteran` (+0.069; +0.047 on seed
1). The `glacier` column is §22's to the third decimal and matches the old
build win for win, which is what says the new build pairs with the old.
`Balanced`'s and `trap`'s `elite` read the same 0.414 / 0.448 on each seed.
That is a coincidence of totals: the two styles disagree on 14 and 24 games'
winners, and those disagreements cancel exactly.

**The top of the Corp ladder moved from 0.258–0.297 (§14/§13) to 0.41–0.43,
and it is now the cheap rung.** No Corp rung searches, so `elite` answers as
fast as `operator`. `LevelKind::{Mcts, Puct}` stay: the day a search beats
one ply again on either chair, its top rungs go back to it.
`describe()` reads the base off the spec, so the start screen says "looks
one move ahead" with no change of its own.

**§4(b) is closed.** "Rebuild the top rungs" turned out to mean "give every
Corp profile the fort", then let one ply be the top. What remains of §4 is
(a), which this table already answers for the Corp: it is spaced by
measurement, like the Runner's.

## 25. Bots that play the strategy guide's precepts — DONE (a brief, 25 September 2026; nine stages, 29–30 September 2026)

Asked for by the person: retool the bots so they play toward the stages of a game and the plans a person would recognise, **and** play stronger. The plans are the ones the strategy guide now teaches (Phase 1.75 §10, `docs/strategy-guide.md`); this entry restates them as things a bot can be measured against, with what the code does today, so whoever takes the work up starts from the record rather than from the idea. Nothing here is built.

### What the record already says, before any of it is tried

- **A stage dial has lost on both chairs, and why matters more than that it lost.** §6–§8 interpolated the Runner's `Weights` from a build profile to a pressure profile by a board-read stage; once the build endpoint was repaired (§7), standing on it all game beat every schedule (the Corp's win share against it 0.113, against 0.149 for the dial as built), because the legs ranked by how much of the game they spent on the better profile, not by when they switched; a turn-count clock (§8) never beat the board read. The Corp's version (§19, #134 closed) lost toward every endpoint but one tie. `STAGE_GAIN` is 0.0. **So "play toward the stages" must not be built as a dial between profiles again.** What worked instead was a *where*, not a *when*: §19's fort terms (`fort_value`) took `glacier` from 0.247 to 0.462, and §23 put them in every Corp profile. The guide's own framing agrees — a stage is read off the board — which suggests the stages belong *inside* terms, as conditions on the board (the Runner's credits against the cost of a server, the points each side needs), not as a switch between weight sets.
- **Card-reading terms beat generic knobs** (Phase 3 §1: `trap` lost with six knobs and won with three terms that read the cards; §20).
- **One ply is the strongest bot on both chairs** (§24: one ply beat `puct@512` in every Corp style; the Runner's search plateau is Phase 2 §5 item 43). Strength has come from the evaluator, so a precept is most likely to pay as a term the one-ply evaluator reads.
- **Neither bot reads the opponent's identity or faction**, and nothing in the evaluator reads a stage beyond resource floors (the grip floor, the HQ floor) and this turn's runs.
- **The Runner ladder has not been re-taken since §23's fort terms**, which moved every Corp; the last Runner numbers (0.865 one ply, `elite` 0.863 in §22) predate them.

### The precepts, and where the bots stand

Each is the guide's advice as a behaviour a report can count. "Met" means a measured entry already shows it; "gap" means no term reads it; "unmeasured" means nobody has looked.

**Corp.**
1. *Ice the centrals first, then build one scoring remote and score behind it.* Met: §19 (every agenda behind 2 ICE, all three centrals iced by the first agenda), §23 for every style.
2. *Score when the Runner cannot afford the run* — the taxing remote's window. Gap: no term sets the Runner's credits against what the remote costs to get into.
3. *Never advance: install one turn, finish the next* with Seamless Launch, Touch-ups or Key Performance Indicators. Gap, and a hard one for one ply: the install is only worth it for what a later turn does.
4. *Score from hand*: a 2-advancement agenda (Greenmail, Hostile Takeover) installed and advanced twice in three clicks; a 3-advancement one with Nanomanagement's clicks. Unmeasured: one ply values each click's position, so whether it finds the three-click line is a question for a report, and a turn-level planner over the Corp's own clicks (the opponent frozen) is the experiment if it does not.
5. *Bluff with installs*: advance traps (Urtica Cipher, Clearinghouse), leave agendas unadvanced, put an asset in the scoring server now and then. Partly met: §20's traps are played like agendas; nothing puts an asset in the remote to tax a run.
6. *ICE order and rez timing*: stopping ICE outside, punishing ICE inside (a habit the guide calls contested); rez when the run matters. Gap for order: no term reads it. Unmeasured for rez timing.
7. *Kill*: tag, then punish (Scorched Earth, Orbital Superiority, Retribution, the tagged resource trash); flatline when the grip is short. Partly met: the opponent-grip-shortfall term. No lethal check is known. **Pool constraint: Scorched Earth is in the pool and in no sample deck**, so a kill plan needs a deck that holds its payoff (a sample or Sweep list, or Phase 1 §9).

**Runner.**
8. *Run the centrals before they are iced, and make the Corp rez.* Probably met: §5's tempo instrument found the Runner most aggressive when its rig was emptiest — the guide's advice, arrived at by accident; rezzes forced are unmeasured.
9. *Run HQ by agenda density*: when the Corp holds cards and is not scoring, and the turn before its window. Gap: `access_prospect` prices an HQ access by its card count, not by how long the Corp has held those cards unscored.
10. *Multi-access R&D when the rig allows it.* Unmeasured.
11. *Economy first, then the breakers the Corp has shown.* Partly met: `builder`'s repair (§7) and the coverage terms; whether a breaker comes before the ICE type it answers is rezzed is unmeasured.
12. *Trash the Corp's economy on access.* Partly met: `access_prospect` counts the net gain of an affordable trash; whether the bot pays for an economy asset when that leaves it short is unmeasured.
13. *Stay alive*: keep the grip above the damage in reach, clear tags, and do not run on the last click against a Corp that punishes runs. Partly met: the grip floor and the tag weight. Gap: nothing reads the Corp's identity, so Jinteki and NBN are feared no more than anyone.
14. *Play the faction's plan*: a Criminal's HQ economy runs (Account Siphon, Transfer of Wealth, Docklands Pass), an Anarch's trashing and sabotage (Cacophony, Carnivore), a Shaper's scaling rig and R&D access. Gap: nothing reads the bot's own identity either, beyond the deck's style tag.

### How to take it up

1. **Instrument first, as §5 did for tempo** — a `diag/` branch whose product is a *precepts report*: each precept above counted per game off the masked logs, for the shipped seatings (heuristic against heuristic, random against random) and per style. It turns "gap" and "unmeasured" into numbers and picks the order.
2. **One precept per branch, as a term that reads the board or the cards**, never a new profile blend. Each measured the way this phase measures a chair: the bot on one chair against the fixed one-ply balanced bot on the other, 384 games on each of two seeds, on pinned binaries, against the seed-spread band — and the precepts report, so a change that wins without playing more like the guide (or plays more like it and loses) is visible as such. Strength is the bar; the precept is the reason and the signature.
3. **Precept 4's planner and precept 13's identity reading are the two structural pieces**: the first is the only one one ply may be unable to express, the second changes what a bot knows (its opponent's public identity, which the view already carries), and both want their own entry.
4. **Then re-take both ladders**, the Runner's first, since it has not moved since §23.

Housekeeping found while writing this: `difficulty.rs`'s module doc says six `Personality` profiles (there are eight), and the ROADMAP's "next" item 3 still leads with the Corp `elite` at 0.266, which §21–§24 superseded.

### The plan: rebuild the decision core on the harness (29 September 2026)

The person read the brief, said the bots "don't entirely align with the strategy guide and how people play", and asked to be told if a full redo was needed. **The answer, and the decision: the decision core, yes; the harness, no.** What a person meets is a one-ply greedy chooser over a static position score — a linear sum of about fifty `Weights` in one 4,300-line file, a "personality" a few of those weights moved — and it cannot express a plan that spans clicks ("install now, score next turn"), a stacked style, a faction, or what it has seen; the §5 tempo table is what that looks like from outside. The brief's incremental route (one term per precept into that sum) was considered and set aside: it keeps bolting conditions onto a shape that has no room for a turn, and the person would feel each term one at a time. Kept: the session loop and sweeps, the rungs-as-handicaps ladder, `determinize`'s sampling mechanics, the measured card-reading helpers (`fort_value`, `access_prospect`, the trap reading, `rig_coverage`, the break costs), the RL paths, the dormant searches, `bench`/`diag`/`coverage_identical.py`/`paired_bench.py`. Replaced, one stage at a time, each a branch and a PR with its numbers here:

0. **Startup is a test and a filter, not a claim** — done, below.
1. **The precepts report** (`diag precepts`): the fourteen precepts counted per game off the logs, by style, faction and a board-read stage, with a per-card *reach* section (what a heuristic seat ever plays; the hand list "Bot debts" in `nsg-card-pool.md` becomes its output), and the baseline on `main` in both formats.
2. **What a bot knows** (`Knowledge`: the match's format and its own deck): the hidden-card pool is the format's pool; the opponent's deck is a weighted prior — the identity's faction and influence, a seen card's remaining copies (3 − seen, ◆ 1) weighted up and struck as they are seen, remembered across decisions through `BotAgent::observe`; the identity→published-list guess is removed for the opponent (decided by the person, 29 September 2026: a guess presented as knowledge). The seat's own deck stays exact.
3. **The evaluator as modules**, byte-identical (`eval/{read,fundamentals,corp,runner}`), a `Stage { Early, Middle, Late }` read off the guide's three board signals, and the stage *dial* (`runner_stage`, `stage_weights`, `lerp`, `STAGE_GAIN`) deleted — a stage is a condition inside a term, never a switch between weight sets (§6–§8, §19).
4. **The turn planner** (`PlanningAgent`): the whole turn's clicks planned on the sample with the opponent frozen (a beam over lines, a run priced as a leaf), re-planned when the board diverges; the old chooser kept as `OnePly`, the fixed reference every later stage is measured against, until Stage 8.
5. **Economy at the guide's rate**: a click as a currency, a card worth what its text declares (Phase 2 §5a's owed lever), an installed economy card worth its yield over the turns the stage expects, and each side's credits read against what the other can afford (the taxing window, "make the Corp rez").
6. **The Corp's plans, stacked by a deck** (`Plan::{Glacier, FastAdvance, Kill, Traps}`; `DeckFile::style` a list; `Personality` deleted outright): rez timing and ICE order, the never-advance line, a lethal check with a Sweep deck that holds its payoffs, and "glacier then fast advance" as one condition — the fort terms fall away once the Runner's rig beats the wall.
7. **The Runner's plans and the identity both chairs read** (`Plan::{Pressure, Dismantle, Rig}` from the seat's faction): fear by the Corp's faction, HQ by agenda density, multi-access R&D, breakers for the ICE shown.
8. **The handover**: every rung on the planner, both ladders re-taken (the Runner's first, §4(a)), `OnePly` deleted once each chair's top rung beats it.

Every behaviour stage is measured as this phase measures a chair — the bot on one chair against the fixed reference on the other, 384 games on each of two seeds, on pinned binaries, `paired_bench.py`'s z against the seed-spread band — **and** by the precepts report, so a change that wins without playing more like the guide, or plays like it and loses, is visible as such. The risk is stated up front: a bot that plays more like the guide may at first lose to the greedy one, and both numbers are the bar.

#### Stage 0 — Startup is a test and a filter, not a claim — DONE (`chore/startup-pool-gate`, 29 September 2026)

"All the cards for Startup are available for play" was true and nowhere checked. Now: `every_card_in_a_complete_formats_pool_is_built_and_playable` (`cards/embedded.rs`) reads each format's pool off `formats.json` and holds every format on `COMPLETE_FORMATS` (`cards/unimplemented.rs`, today `[Startup]`) to having a playable card for every printing the catalog knows — built under its own code or its title, the reprint fold the per-pack gates use — and names the codes the catalog does not carry (`STARTUP_POOL_CODES_OUTSIDE_THE_CATALOG`: four printings of Sure Gamble and Hedge Fund from packs that are not embedded). Its other half holds the list honest: a format not on it must be short of a card, so the day Standard completes the gate says so. `scripts/pool_status.py` prints the same count per format (`startup 225/225 cards built (231 codes, 4 codes not in the catalog) — complete`; Standard 374/613, Eternal 399/797). A per-pack gate said a *pack* was built; this says a *format* is, which is what a player choosing Startup, and a bot told the format in Stage 2, are promised.

The filter: **only 90 of the 192 sample matchups are Startup-legal** (9 Corp × 10 Runner decks; NetrunnerDB's list bans cards nine of the published decks hold, Phase 1 §9 Stage 0b), and every measuring tool played all 192, so there was no "Startup play" number to take. `decks::matchups_in(format, registry)` keeps `matchups()`'s pairings both decks are legal in, in its order — a Casual pass is `matchups()` game for game, so every number recorded before it reproduces — and the one global `--format` flag now chooses the pass for `--headless --all-matchups`, `bench` and every `diag` (`Config::matchups`), read from the flag alone (`settings::applies_to` already kept the settings file out of measurements; `deck matchups`, which lists a pass one pairing a line, joins that exemption because it describes one). `scripts/coverage_identical.py --format startup` asks each pinned binary for its pass through `deck matchups`, so both refs must carry this stage. Checked: the four Casual reports identical against `main`; a Startup pass is 90 games.

#### Stage 1 — The precepts report, and where the bots stand on `main` — DONE (`diag/precepts-report`, 29 September 2026)

`netrunner_cli diag precepts` (`crates/netrunner_cli/src/diag/precepts.rs`), on `diag tempo`'s pattern: one counter per precept of the fourteen above, counted off each applied entry and the states around it, grouped by Corp style, Runner style, each side's faction and a **board-read stage** (`precepts::stage`: *late* when either side is within two points of the target — the pool's common agenda — *early* while HQ or R&D is unprotected or no remote has ICE in front of it, *middle* between; the Runner's tools are not in the reading yet, and Stage 3 owns refining it before a term reads it); every ratio derived once at report time from summed counters with its numerator and denominator named (`DERIVED`, numbered as the brief numbers the precepts), and a **reach** section — every card a seat used over the pass by its owner's own choice (played, installed, rezzed, activated, advanced, scored) and the cards in the decks played that no seat ever used. `--deck-styles` seats each chair in its deck's own style, `--format` picks the pass (Stage 0), `--report` writes every group and every game's record as JSON. `eval::server_break_cost` is the one new engine-side reader: the rig's price to break every rezzed piece of ICE on a server, `remaining_break_cost`'s reading taken off the table, for the taxing window and the Runner's turn-start affordability. No behaviour moved.

**The baseline, `main` at #297.** One full pass a run (192 Casual, 90 Startup), seeds 1 and 2, the shipped seating (`heuristic` with deck styles) and random against random. The precepts as ratios, the Corp win share first (`target/coverage/precepts/*.json`, on the branch's release build):

| ratio | heur C s1 | heur C s2 | heur S s1 | heur S s2 | rand C s1 | rand C s2 |
|---|---|---|---|---|---|---|
| Corp win share | 0.458 | 0.401 | 0.544 | 0.522 | 0.385 | 0.370 |
| `corp.01.centrals_iced_before_first_remote` | 0.561 | 0.505 | 0.523 | 0.558 | 0.164 | 0.090 |
| `corp.01.ice_per_agenda_install` | 2.104 | 2.139 | 2.195 | 2.213 | 0.343 | 0.439 |
| `corp.02.scores_runner_could_not_afford` | 0.491 | 0.455 | 0.778 | 0.500 | 0.000 | 0.000 |
| `corp.03.scores_after_unadvanced_install_turn` | 0.191 | 0.241 | 0.273 | 0.321 | 0.700 | 0.800 |
| `corp.03.advancement_by_cards_per_game` | 0.365 | 0.406 | 0.400 | 0.533 | 0.422 | 0.526 |
| `corp.04.scores_same_turn_as_install` | 0.017 | 0.027 | 0.000 | 0.000 | 0.000 | 0.000 |
| `corp.05.advance_clicks_on_non_agendas` | 0.062 | 0.069 | 0.137 | 0.086 | 0.306 | 0.280 |
| `corp.05.asset_installs_in_iced_remote_per_game` | 0.286 | 0.286 | 0.300 | 0.233 | 0.766 | 1.182 |
| `corp.06.etr_installed_over_non_etr` | 0.205 | 0.229 | 0.264 | 0.246 | 0.246 | 0.240 |
| `corp.06.non_etr_installed_over_etr` | 0.251 | 0.257 | 0.236 | 0.242 | 0.244 | 0.236 |
| `corp.06.rezzes_the_runner_could_break` | 0.423 | 0.398 | 0.364 | 0.368 | 0.228 | 0.211 |
| `corp.07.tags_given_per_game` | 1.370 | 1.240 | 1.956 | 2.133 | 1.083 | 1.021 |
| `corp.07.tagged_resource_trashes_per_game` | 0.000 | 0.000 | 0.000 | 0.000 | 0.141 | 0.120 |
| `corp.07.flatlines` | 0.089 | 0.052 | 0.144 | 0.167 | 0.385 | 0.370 |
| `runner.08.central_runs_on_unprotected` | 0.177 | 0.228 | 0.227 | 0.230 | 0.400 | 0.438 |
| `runner.08.runs_that_forced_a_rez` | 0.638 | 0.649 | 0.652 | 0.689 | 0.209 | 0.200 |
| `runner.09.hq_runs_into_a_full_hand` | 0.251 | 0.204 | 0.190 | 0.235 | 0.263 | 0.293 |
| `runner.10.rnd_runs_with_multi_access` | 0.094 | 0.113 | 0.079 | 0.051 | 0.056 | 0.057 |
| `runner.11.breakers_for_ice_already_shown` | 0.441 | 0.411 | 0.443 | 0.432 | 0.151 | 0.159 |
| `runner.11.economy_installs_per_game` | 0.375 | 0.422 | 0.267 | 0.289 | 1.651 | 1.630 |
| `runner.12.economy_assets_trashed_when_accessed` | 0.160 | 0.155 | 0.103 | 0.154 | 0.139 | 0.171 |
| `runner.13.runs_on_the_last_click` | 0.139 | 0.138 | 0.148 | 0.136 | 0.218 | 0.228 |
| `runner.13.tags_cleared_per_tag_taken` | 0.939 | 0.861 | 0.886 | 0.896 | 0.260 | 0.281 |
| `runner.14.remote_runs_share` | 0.227 | 0.209 | 0.192 | 0.197 | 0.354 | 0.357 |
| `economy.corp.credit_click_share` | 0.411 | 0.413 | 0.375 | 0.362 | 0.128 | 0.127 |
| `economy.corp.credits_at_turn_start` | 17.2 | 16.6 | 17.1 | 14.9 | 4.1 | 3.9 |
| `economy.runner.credit_click_share` | 0.511 | 0.507 | 0.527 | 0.519 | 0.114 | 0.110 |
| `economy.runner.credits_at_turn_start` | 15.3 | 14.8 | 14.0 | 13.6 | 4.4 | 4.4 |
| `stage.corp.early_turns` / `middle` / `late` | 0.45 / 0.31 / 0.24 | 0.43 / 0.31 / 0.26 | 0.38 / 0.31 / 0.32 | 0.41 / 0.33 / 0.26 | 0.70 / 0.15 / 0.16 | 0.68 / 0.14 / 0.18 |
| `stage.corp.early.advance_clicks_per_turn` → `late` | 0.025 → 0.783 | 0.021 → 0.636 | 0.047 → 0.663 | 0.036 → 0.782 | 0.199 → 0.233 | 0.219 → 0.232 |
| `stage.runner.early.runs_per_turn` → `late` | 1.094 → 0.893 | 1.253 → 0.817 | 1.211 → 0.698 | 1.202 → 0.706 | 2.042 → 1.997 | 2.113 → 2.169 |
| `stage.runner.early.install_clicks_per_turn` → `late.credit_clicks` | 0.252 → 1.977 | 0.293 → 2.032 | 0.262 → 2.311 | 0.258 → 2.118 | 0.511 → 0.394 | 0.494 → 0.385 |

**What the baseline says, precept by precept** (the seeds agree to within a few hundredths everywhere it matters, so one number is quoted):

- **Met, and measured for the first time:** the fort (1: 2.1 ICE per agenda install, no naked agenda in 384 games, against random's 0.4 and 0.7); the taxing window (2) *happens* half the time — 0.49 of scores while the Runner could not afford the remote (0.78 on Startup seed 1) — with no term reading it, so it is the fort's side effect and the term Stage 5 writes has a number to beat; forcing rezzes (8: 0.64 of runs into unrezzed ICE draw one, random 0.21); tags cleared (13: 0.9 per tag, random 0.26).
- **Not played at all:** score from hand (4: 0.017, 0.000 on Startup; no three-click line is ever found, which is Stage 4's planner); the tag punish (7: **zero** tagged-resource trashes in 384 heuristic games where random takes 0.13 a game — the click is priced at nothing); an asset in the scoring remote (5: 0.29 a game, a third of random's).
- **Half-played, by accident:** never-advance (3: 0.19–0.32 of scores follow an unadvanced install turn, but `advancement_by_cards` is 0.4 a game either seating — Seamless Launch, Touch-ups and KPI are on the unused list, so those scores are the Corp being slow, not the play); ICE order (6: ETR-over-non-ETR 0.21–0.26 against non-ETR-over-ETR 0.24–0.26 — no preference at all, identical to random); rez timing (6: 0.40 of in-run rezzes are of ICE the rig already breaks).
- **The Runner's economy is inverted:** 0.51 of the heuristic Runner's clicks are the credit click (random 0.11) and it installs 0.4 economy cards a game (random 1.65) — it clicks for money it would rather have from a card it will not install (11; Phase 2 §5a's owed lever, Stage 5). Breakers come after the ICE they answer is shown 0.44 of the time (11), and economy assets are trashed when accessed 0.16 of the time (12), 0.10 on Startup.
- **The stage, read off the board, shows the §5 finding in one column each:** the Corp's advance clicks go 0.03 → 0.78 a turn from early to late, which is the fort working; the Runner's runs *fall* 1.1 → 0.9 (Startup 1.2 → 0.7) from early to late while its late credit clicks reach 2.0–2.3 of 4 — the Runner is at its most passive when a person would be going for the last agenda "even if it takes every credit they have".
- **By style and faction (heuristic, Casual, seed 1):** the Runner styles are within 0.07 of each other on every line, as §22 found; by faction the Criminal runs HQ most (0.43 of runs, Shaper 0.36, Anarch 0.35) and the Shaper plays the most economy events (3.8 a game against 1.1) — both the deck's cards, since nothing reads the faction. The trap Corp deals 6.3 net damage a game and scores 0.75; NBN gives 3.0 tags a game and punishes none of them.

**Reach — the card-testing product.** Over the Casual pass (157 cards in the 28 sample decks, identities included), the heuristic seating used **91** and never used **66**; random used 139 and never used 18 (identities without a usable ability). The 66 the heuristic never touches, on both seeds: *Seamless Launch, Touch-ups, Key Performance Indicators, Nanomanagement, Predictive Planogram, Public Trail, Retribution, Neurospike (Startup), IP Enforcement, Top-Down Solutions, Tread Lightly, Wildcat Strike, Measured Response, Bigger Picture; Docklands Pass, Jailbreak, Conduit, Overclock, Red Team, Transfer of Wealth, Clean Getaway, Fransofia Ward, Tranquilizer (Startup), Botulus, Cacophony, Carnivore, Gourmand, Cookbook, Madani, GAMEDRAGON™ Pro, Telework Contract, Pennyshaver, Rent Rioters, Side Hustle, Sprint, Scrounge, Mutual Favor, Lie Low, Shred, Detente, Azimat, Open Market, Byte!, Illumination, Knickknack O'Brian, Maglectric Rapid, Verbal Plasticity* and 18 identities. Sixteen of them are cards the strategy guide names by title as the way the plan is played. On Startup (136 cards in the 19 decks) the heuristic used 76–79 and never 57–60. `nsg-card-pool.md`'s hand list "Bot debts" now points here.

Random seatings identical to `main` in all four `coverage_identical.py` shapes (no behaviour moved; the diag is read-only over the session).

#### Stage 2 — What a bot knows: the format, its own deck, and what it has seen — DONE (`feat/bot-knowledge`, 29 September 2026)

`netrunner_bots::knowledge::Knowledge { format, own_deck, seen }` rides with every determinizing agent (`HeuristicAgent`, `MctsAgent`, `PuctAgent`, each `with_knowledge`; `LevelSpec::agent(seed, knowledge)`; `AgentSetup::knowledge` in the CLI, `Config::knowledge(deck)` the one place it is built there; `LocalMatchSpec::format` in the client; the daemon's dealt deck and lobby format; the gym, self-play and both sweeps seat their decks) and is read by `determinize` alone. **The sampler drew the opponent's hidden cards from an embedded published list matched by identity** — right for every sample deck, wrong for every deck a person builds, and "a guess presented as knowledge" — **and, failing a match, from the whole registry**, every set of every format, so a Startup game imagined Core Set cards behind the ICE. Now:

- **The format's pool.** `Prior::over` admits a card the format's rules hold (`FormatRules::in_pool` by the card's printing, minus the ban list; a card with no printing code is in no pool but Casual's), sorted by id before any draw — the reproducibility the sorted registry pools bought is kept. `Casual` is every playable card, which is what the fallback was.
- **A weighted prior over the opponent's deck**, drawn by weight *without replacement*: each card's copies are its playset (its own `deck_limit`, else three; **one for a ◆ card**, which a deck runs one or two of and the table holds one of) less the copies the view shows, so a sample never holds a fourth copy — which the cycling pool did not promise — and a card in the heap or the score area is one fewer to draw. Weight 1 for an unseen card of the opponent identity's faction or a neutral one; for an unseen out-of-faction card, the deck's remaining influence (the identity's budget less what every seen out-of-faction copy has spent) over the card's cost and the number of out-of-faction candidates — about an eighth of an in-faction card under fifteen influence over sixty-odd imports at two each, the ratio a real deck's six or seven imports bear to its thirty faction cards; a starter identity with no budget weighs every card alike. **A seen card's remaining copies weigh `SEEN_COPY_WEIGHT` (3)** — a deck that holds a card holds two or three far more often than one — and its influence counts as spent for the rest.
- **The seat's own deck is exact:** `own_deck` minus what the view shows of its own cards is the rest of its stack, up to order. A seat built without one still finds a published list by its identity (`remaining_decklist`, now asked for the viewer's own side only) — the person's decision: the match stays where it is knowledge and goes where it was not. A spectator's sample draws both sides from the prior.
- **Memory.** `BotAgent::observe(&ClientView)` replaces the `GameEvent` hook nobody called; `Session::step` shows the acting seat its view before asking it, and `BotAgentIndexAdapter` does the same on the index path, so `HandicapAgent` forwards it whether or not the decision is a blunder. `Knowledge::observe` keeps, per card, **the most copies the seat has ever seen at once** — monotone, and by copies visible together rather than by sightings, because an HQ card accessed on turn three and rezzed on turn eight is one card and the memory must never say two. A card accessed in HQ, looked at on top of R&D or revealed is in that view and no later one; it is remembered. `Session::last_view` keeps the view an in-process seat was shown, for a driver that keeps a `Knowledge` beside the bot's (`diag precepts`, `diag leaf-sensitivity`).

**The guess-quality line** (`diag precepts`, `knowledge.*`): once a turn, on the turn side's first decision, the seat's view is sampled with its knowledge and the opponent's hidden cards in the sample — the other hand and deck, and for the Runner the Corp's facedown installs and Archives too — are scored against the cards really there: `guess_in_deck` is the share of sampled cards the opponent's deck holds at all, `guess_overlap` the share that match the hidden cards as a multiset; `naive_*` is the same view sampled by a seat that knows nothing (Casual, no deck, no memory — the faction prior alone). The old sampler scored 1.0 on both lines by construction for a sample deck (a permutation of the real deck) and the registry fallback on any other; neither is a number a person's deck would ever see.

**Measured** (pinned binaries, `main` at #299 against the branch; one full pass a run, seeds 1 and 2):

| line | heur Casual s1 | heur Casual s2 | heur Startup s1 | heur Startup s2 |
|---|---|---|---|---|
| Corp win share, before → after | 0.458 → 0.469 | 0.401 → 0.469 | 0.544 → 0.433 | 0.522 → 0.467 |
| `knowledge.corp.guess_in_deck` / `naive_in_deck` | 0.290 / 0.286 | 0.285 / 0.286 | **0.448** / 0.293 | **0.447** / 0.292 |
| `knowledge.corp.guess_overlap` / `naive_overlap` | 0.248 / 0.246 | 0.244 / 0.246 | **0.386** / 0.256 | **0.376** / 0.249 |
| `knowledge.runner.guess_in_deck` / `naive_in_deck` | 0.322 / 0.300 | 0.318 / 0.302 | **0.447** / 0.296 | **0.448** / 0.298 |
| `knowledge.runner.guess_overlap` / `naive_overlap` | 0.288 / 0.268 | 0.282 / 0.269 | **0.394** / 0.266 | **0.396** / 0.268 |

**What the line says.** Under Startup a seat's sample of the opponent's hidden cards is a card the opponent's deck holds **0.45** of the time and the right card in the right count 0.38–0.40, against 0.29 and 0.25–0.27 for the faction prior over every card the registry knows: **the format's pool is the whole of the gain**, and it is the same on both chairs and both seeds. The memory is worth 0.00–0.02 on top (Casual, where the pool is the registry either way: the Corp 0.290 against 0.286, the Runner 0.322 against 0.300) — as it should be, because nearly everything a seat sees stays in view (the heap, Archives, the rig, the score area), and the prior already boosts a card's copies for what the view shows; what the memory adds is the card that *left* the view, an HQ access or a look at R&D, which is a few cards a game. The Runner, who accesses, gains more from it than the Corp, who mostly does not. **The old sampler scored 1.0 on both lines for a sample deck**, by handing each chair the other's exact list; what the honest number costs is in the win share: Casual moves inside the band (0.458 / 0.401 → 0.469 / 0.469) while **the Startup Corp falls 0.544 / 0.522 → 0.433 / 0.467**, on both seeds — the Corp was the chair cashing the omniscient list (the Runner's grip and stack told it what could be broken and what could be killed; `corp.07.net_damage_per_game` 4.3 → 3.7–3.8 and `tags_given` 2.0–2.1 → 1.5–1.9 on Startup go with it). Not tuned back: the number it replaces was a bot that knew a person's deck, and Stage 5's opponent-affordance terms are where a Corp learns to read the Runner from what it can see. Nothing else moved the same way on both seeds and both formats beyond the band except `corp.03.advancement_by_cards_per_game` (0.37–0.53 → 0.50–0.70, more Seamless Launch and KPI plays with a smaller guessed pool of what the Runner holds); the reach lists are unchanged to one card either way (66 and 60 unused).

Random seatings identical to `main` in both `coverage_identical.py` random shapes (the random bot samples nothing); both heuristic shapes moved, as a sampling change must (Corp wins 88 → 90 of 192 by view). Both 256-seed sweeps green in release. **Cost:** the prior is rebuilt from the registry at every sample, and a 96-game heuristic Casual pass at eight threads went 7.05 → 8.35 s wall (50.4 → 60.1 s user), a fifth more — well under a millisecond a decision, recorded here so the day it matters the per-format candidate list is the thing to cache. **Not measured per chair:** a sampling change moves both chairs in one binary, and the old sampler is deleted rather than kept behind a flag, so there is no fixed-reference seating for it; the guess-quality line is this stage's signature, as the plan says, and the win share is reported for what it is.

#### Stage 3 — The evaluator as modules, the stage as a reading, the dial deleted — DONE (`chore/eval-in-modules`, 29 September 2026)

`crates/netrunner_bots/src/eval.rs` (4,358 lines) is `eval/{mod,fundamentals,read,corp,runner,stage}.rs` (966, 532, 542, 932, 888 and 92 lines, tests included, plus 273 of shared fixtures in `test_support.rs`), **byte-identical in all four `coverage_identical.py` shapes** — the refactor's only claim, and the only check that can make it. `mod.rs` keeps `Weights` and the constants whose doc comments are each term's measurement record, and `evaluate_state_with` adds the shared prefix and then one arm — `corp::score` and `runner::score` take the accumulator by `&mut` and add the same terms in the same order the one file did, because a floating-point sum follows its order and a reordered sum would have been a different bot by a rounding. `fundamentals` is the parked payment and the parked decision's lower bound; `read` is what both arms read off the cards and the board (break costs, the trap and ambush recognisers, rig coverage); `corp` and `runner` are the arms with their own terms; the tests moved with their terms, the fixtures they share into `test_support`.

**`eval::stage::Stage { Early, Middle, Late }`** is the reading `diag precepts` bins by, moved from the diag so a term that conditions on a stage and the report can never disagree: late once either side is within two points of the target, middle once HQ, R&D and some remote are iced, early otherwise. **The definition is Stage 1's, unchanged**, and deliberately not the guide's third signal yet (each side's economy on the table): folding it in would move the baseline's stage columns without a term needing it, and it goes in with the first term that reads it, measured — the plan's "quoted against the §5 tempo table" is that measurement, owed to the term rather than to this refactor.

**Deleted:** `runner_stage`, `stage_weights`, `lerp`, `STAGE_GAIN`, `Weights::stage_gain`, `HeuristicAgent`/`MctsAgent`/`UniformPolicyEvaluator::with_stage_gain`, `AgentSetup::stage_gain`, `bench --stage-gain` and `diag tempo --stage-gain`, and the five tests of the dial. The record of why stays in §6–§8 and §19 (twice zero, for two different reasons; a clock never beat the board) and in the archive; the module doc says it in one paragraph. Nothing measured moves: the dial was at 0.0 in every shipped seat, which is why the deletion is inside the byte-identical claim.

`cargo test --workspace` green, clippy silent; the sweeps play the same games as `main` (identical reports), so the 256-seed runs were not re-taken for a refactor that moved no decision.

#### Stage 4 — The turn planner — DONE (`feat/turn-planner`, 29 September 2026)

`netrunner_bots::planner::PlanningAgent` (`--corp planner`, `bench --bots planner`, `record` id `planner`), beside the one-ply `HeuristicAgent`, which stays as it was: **the fixed reference every later stage is measured against, and the base of every ladder rung until Stage 8** — so nothing a person meets has moved. The plan's rename of the reference to `OnePly` was not done: it is deleted in Stage 8, and every recorded command in this file, the scripts and the sweeps names `heuristic`; a rename that costs forty files to be undone four stages later is churn twice.

**What it does.** At the first decision of its own action phase — the seat's `Action` phase, nothing parked, no window open — it draws one sample of the hidden state as the chooser does and searches a beam of lines over it with the opponent frozen: every priority the opponent is handed is a pass (`settle`), a decision only the opponent can make ends the line, and **a run ends the line the moment it starts** — priced as a leaf by the evaluator's mid-run terms and then played one decision at a time, as today, because the ICE the sample imagined is not the ICE the run will meet. Lines end at the end of the turn (through the discard phase), and the best is played **one action per decision for as long as the view is the one the line predicted**: each step carries the legal actions the sample offered where it was chosen, and a view whose list differs — a draw brought another card, the opponent rezzed, a prompt the sample did not foresee — drops the rest and plans again. Everything else (inside a run, inside a prompt the plan did not make, the opponent's turn) is the one-ply chooser's, through one function (`heuristic::choose_one_ply`), so the planner differs from the reference only where it plans.

**Decisions, and what was rejected.** (1) *The beam keeps the best line under every first action* beside the `PLAN_BEAM` (6) best partial lines: the from-hand line's first action, the install, is the worst-scoring first action on the board by a static reading, and a beam pruned by that reading drops it at the first ply and never learns what it was for; a greedy continuation from each first action is what finds it. A two-ply look-ahead to rank the beam was rejected — the same lines at the branching factor's cost. (2) *A click left over is worth a credit and a hair more* (`click_floor`, `KEPT_CLICK_EDGE` 0.05): a line that ends before its clicks are spent — a run at the first click — is scored where it stands plus one credit's worth per unspent click, the least a click buys, so a run at the first click competes with a credit as it does under the chooser rather than losing to four credit clicks by three; the hair goes to the click kept, because the turn after a run is planned again knowing what the run found. The guide's rate for a click is Stage 5's term. (3) *A parked payment of the seat's own is answered inside the line, never left for the beam* (`answer_payment`): an install that trashes first, a cost that takes cards and a split between pools are each parked as a question by the engine, and the evaluator prices a parked state by resolving every chain of answers (`fundamentals::through_parked_payment`) — a product over the asks. Left as nodes, the beam expanded that tree while the evaluator re-priced its remainder at every node: a Runner with Smartware Distributors and Telework Contracts on the table spent **30 s on one plan**. Now the chain is answered once, as the evaluator would (the pool split alone is not enumerated: the evaluator prices the credit pool only and always keeps it), each answer a forced step, and that game plays in 12 s. (4) *The sample installs under the real ids*: `ClientView::next_install_id` is new (public — every install is an event both players see — and `engine's` in the client ledger), because a sample that started past the highest id it could see was short by every install trashed since, and the line's "advance the card I just installed" broke at the install every time. (5) *One apply per legal move*: `rules::legal_transitions{,_for}` keep what proving each candidate produced, so a search that expands every move no longer pays for the list twice; `legal_actions` is the same loop with the states dropped. (6) A plan that survives a divergence by skipping the step it cannot play was rejected: a line whose premise changed is a line whose end was scored on a board that no longer exists. (7) The whole search is capped at `PLAN_BUDGET` (2,500) counted applications a plan, `PLAN_MAX_LINE` (24) actions a line.

**Cost** (release, one thread, an idle machine, a 96-game Casual pass): heuristic both chairs **36.6 s**; the planner as Corp 128 s (3.5×), as Runner 125 s (3.4×), on both chairs 211 s (5.8×). Per game with the planner on both chairs, 0.4–3 s, the slowest decision 0.6 s (`planner::cost::planner_cost`, ignored; six games). A decision in the desktop client is therefore under a second, and it is not on the ladder yet in any case. **How the lines hold:** over those games the Corp made 10–31 plans a game and played 3.0 actions a plan from them, re-planning on a divergence 3–8 times a game; the Runner made 19–58 (one a run, since a run ends the line) and played 1.6–3.2 a plan. Two thirds of the seats' decisions are the chooser's (runs, prompts, the opponent's windows).

**Measured** (pinned binaries, the branch against itself: the planner on one chair against the reference on the other, 384 games a leg on each of two seeds, `bench --bots planner,heuristic --pairing planner/heuristic --games 384 --seed s` against `--bots heuristic --games 384 --seed 384+s`, which is the same games — bench seeds a cell by its index in the square — paired game by game, McNemar's z over the discordant games):

| leg | reference (heuristic both) | with the planner | delta for the planner's chair | z | discordant |
|---|---|---|---|---|---|
| planner Corp, seed 1 | Corp 0.432 | Corp **0.312** | −0.120 | −3.78 | 148 |
| planner Corp, seed 2 | Corp 0.453 | Corp **0.346** | −0.107 | −3.67 | 125 |
| planner Runner, seed 1 | Runner 0.568 | Runner **0.586** | +0.018 | +0.69 | 103 |
| planner Runner, seed 2 | Runner 0.547 | Runner **0.607** | +0.060 | +2.14 | 115 |

**The planner loses the Corp chair to the chooser it contains, by 0.11–0.12 on both seeds, well past the band; the Runner chair gains, past the band on one seed and inside it on the other.** The plan stated this risk up front — "a bot that plays more like the guide may at first lose to the greedy one, and both numbers are the bar" — and the precepts report says which it is. Both chairs the planner, deck styles, `diag precepts` on the branch against `main`'s heuristic reports (Stage 2's), Casual seeds 1 / 2 and Startup seeds 1 / 2:

| line | heuristic Casual | planner Casual | heuristic Startup | planner Startup |
|---|---|---|---|---|
| Corp win share | 0.469 / 0.469 | 0.339 / 0.250 | 0.433 / 0.467 | 0.411 / 0.422 |
| `corp.04.scores_same_turn_as_install` | 0.020 / 0.024 | **0.064 / 0.040** | 0.000 / 0.000 | 0.000 / 0.009 |
| `corp.03.scores_after_unadvanced_install_turn` | 0.243 / 0.226 | **0.387 / 0.446** | 0.358 / 0.313 | **0.455 / 0.496** |
| `corp.01.naked_agenda_installs` | 0.000 / 0.000 | 0.120 / 0.116 | 0.000 / 0.000 | 0.068 / 0.091 |
| `corp.01.ice_per_agenda_install` | 2.105 / 2.166 | 1.816 / 1.795 | 2.160 / 2.207 | 1.975 / 1.876 |
| `corp.05.agendas_unadvanced_at_turn_end_per_turn` | 0.032 / 0.029 | 0.127 / 0.136 | 0.050 / 0.049 | 0.153 / 0.183 |
| `corp.04.scores_per_game` | 1.583 / 1.521 | 1.224 / 1.052 | 1.478 / 1.522 | 1.367 / 1.300 |
| `economy.corp.credit_click_share` | 0.406 / 0.409 | **0.136 / 0.150** | 0.352 / 0.370 | **0.136 / 0.132** |
| `economy.corp.draw_click_share` | 0.027 / 0.023 | **0.118 / 0.123** | 0.033 / 0.031 | **0.117 / 0.117** |
| `economy.corp.credits_at_turn_start` | 15.1 / 15.8 | **7.8 / 7.4** | 15.6 / 17.1 | **8.5 / 7.7** |
| `economy.corp.turn_starts_able_to_rez_everything` | 0.698 / 0.700 | 0.463 / 0.455 | 0.682 / 0.681 | 0.462 / 0.439 |
| `economy.runner.credit_click_share` | 0.511 / 0.514 | **0.262 / 0.264** | 0.487 / 0.505 | **0.310 / 0.288** |
| `economy.runner.credits_at_turn_start` | 15.1 / 14.2 | 7.3 / 6.6 | 13.8 / 14.9 | 8.7 / 7.1 |
| `runner.11.economy_installs_per_game` | 0.422 / 0.479 | **1.240 / 1.068** | 0.344 / 0.278 | **1.200 / 1.078** |
| `runner.11.first_breaker_turn` | 2.92 / 2.57 | **2.00 / 2.04** | 3.41 / 2.69 | **2.51 / 1.95** |
| `runner.08.runs_per_game` | 14.5 / 14.0 | 15.7 / 15.9 | 13.9 / 15.5 | 16.0 / 15.9 |
| `runner.13.runs_on_the_last_click` | 0.142 / 0.133 | 0.391 / 0.379 | 0.156 / 0.162 | 0.388 / 0.405 |
| `runner.14.steals_per_game` | 2.54 / 2.54 | 2.84 / 2.99 | 2.49 / 2.56 | 2.64 / 2.67 |
| `stage.runner.early.runs_per_turn` → `late` | 1.10 → 0.76 / 1.12 → 0.76 | **1.57 → 1.22 / 1.58 → 1.32** | 1.32 → 0.74 / 1.28 → 0.71 | **1.49 → 1.06 / 1.53 → 1.05** |
| `stage.corp.early.advance_clicks_per_turn` | 0.024 / 0.028 | 0.136 / 0.119 | 0.049 / 0.046 | 0.089 / 0.121 |

**What the lines say.** The planner plays the guide's turn and not yet the guide's economy:

- **Precept 4 is played for the first time** — a score in the turn of the install 0.064 / 0.040 of scores on Casual (0.020 before; still 0.000 / 0.009 on Startup, where the pool's cheap agendas are fewer), and the never-advance line (3) rises from a quarter to 0.39–0.50 of scores on both formats — and the reach list shows why: **Seamless Launch, Touch-ups, Key Performance Indicators, Nanomanagement and Predictive Planogram**, on the never-used list since Stage 1, are all played now, with Jailbreak, Red Team, Transfer of Wealth, Side Hustle, Telework Contract, Rent Rioters, Sprint, Scrounge, Lie Low, Mutual Favor, Open Market, Fransofia Ward, Illumination, Verbal Plasticity, Byte! and Neurospike. **Casual's never-used list falls 66 → 45, Startup's 60 → 42** (Maintenance Access is the one card that fell off; T400 Memory Diamond on Startup). Sixteen of the 66 were cards the guide names as the way the plan is played; twelve of those sixteen are used now.
- **"Draw first, then act" and "install economy first" both happen.** The Corp's draw click goes 0.03 → 0.12 of clicks, the Runner installs economy cards 1.1–1.2 a game against 0.3–0.5, the first breaker lands a turn earlier (2.0 against 2.6–3.4), and the Runner runs more in every stage, most of all late (1.2–1.3 a turn against 0.76 — the §5 "most passive when it should be going for the last agenda" column, which was Stage 1's sharpest finding, is gone) and steals more (2.8–3.0 a game against 2.5).
- **The credit click is no longer half the turn** — 0.14 for the Corp and 0.26–0.31 for the Runner, against 0.41 and 0.51 — **and that is where the Corp chair is lost.** A click is worth a credit to the planner and an installed card is worth more than a credit to the evaluator, so a line that draws and installs beats a line that clicks for credits every time, and the Corp arrives at its turn with **7–8 credits where the chooser had 15–17**, able to rez everything it holds 0.46 of turn starts against 0.70; it scores 1.05–1.37 agendas a game against 1.5, gives up naked agendas (0.12 of agenda installs, 0.000 before) and leaves 0.13–0.18 unadvanced agendas on the table a turn (0.03 before) — the from-hand line begun on the third click and finished by nobody, which a frozen opponent never punishes and the real one steals. The Runner has the same halved economy (7 credits at turn start against 15) and gains anyway, because it spends them on runs and economy cards. Every one of those is a price the evaluator does not carry — a click at the guide's rate, a card at what its text declares, an economy card's yield over the turns left, the opponent's affordability (the Runner's credits against the remote's break cost, which is what makes an unadvanced install safe or not) — and every one is Stage 5's term by name; the never-advance line's condition ("an unadvanced install the Corp can finish next turn") is Stage 6's. **They are not tuned here**, because a dial in the planner (a click floor above a credit, a penalty on an unspent install) would be the stage dial again: a number that stands in for a reading the evaluator should make.
- **Precept 13 goes the other way** — runs on the last click 0.14 → 0.39, against Jinteki and NBN alike — and for a reason the planner is right about under this evaluator: "credit, credit, credit, run" outscores "run, credit, credit, credit" whenever the three credits are what make the run affordable (`remaining_break_cost` against the pool), and the fear that makes the last click the wrong one is Stage 7's term. Rez timing (6) and ICE order (6) are unmoved, as they should be: no term reads them yet.
- **The knowledge lines are unmoved** (Startup 0.45 both chairs, Casual 0.29 / 0.32), as a chooser change must leave them.

The engine's random seatings are identical to `main` in all four `coverage_identical.py` shapes — the heuristic shapes too, since `HeuristicAgent`'s choice is the same function and a sample's install counter reaches no decision the chooser makes. Both sweeps now seat the planner in their purposeful chairs (`PlannerCorpRandomRunner`, `RandomCorpPlannerRunner`): it reaches the lines one ply never found, and the chooser it falls back to inside a run or a prompt still reaches those states. **The 256-seed sweeps found one bug before they were green:** at seed 106 (Second Site, whose identity is A Teia, against Hit List) the planner Corp submitted a third remote and the engine refused it — the sample carries no identity (`determinize` says why), so the sample offered what the real state forbids, and the plan's first action was played unchecked; the root now expands only the view's own legal actions, the one-ply chooser's own rule, and the rest of the line is checked against the real list when it is played. Green after that, in release: the index sweep 85 s and the view sweep 108 s at 256 seeds (both together were about 36 s with the chooser seated), and the two at the default 32 seeds in debug 88 s and 87 s where they were 72 s together — a planner seat is about three times the chooser in the sweeps as in play, which the inner loop bears because the planner is what a person will meet. `cargo test --workspace` green, clippy silent.

#### Stage 5 — Economy at the guide's rate — DONE (`feat/click-and-card-economy`, 29 September 2026)

Eight terms in the evaluator, each a sentence of the strategy guide's "Clicks, credits and cards", "Tempo" and "Economy is the war" written as a reading of the board, and **all eight zero in `Weights::default()`**: `Weights::at_the_guides_rate` sets them, and `PlanningAgent` scores with it over whatever personality it is given. So the one-ply `HeuristicAgent` scores exactly as it did — the fixed reference, and the base of every ladder rung, have not moved by a bit (all four `coverage_identical.py` shapes identical to `main`), and the planner is measured against a chooser that has not changed under it. Stage 8 makes the eight the default when the reference is deleted.

**The terms.** (1) *A click is a credit* (`CLICK_WEIGHT` 0.4, each click its owner has left): "a click can always be turned into 1 credit or 1 card, so a click is worth at least one of either" — every action is judged against the basic rate, a free action keeps the click's worth, a card that gives clicks is worth them and one that takes them pays for them. The planner's own floor (`click_floor`, Stage 4) now adds only what the evaluator does not carry, which at this rate is the kept-click edge alone. (2) *A card is worth what its text declares* (`read::declared_income`, read off the DSL as the trap recognisers are, never off a name): a play's net credits, its cards at a click each and its clicks, over the click that plays it (`fundamentals::play_value`: Hedge Fund +1.2, "four credits for one click"; Diesel +0.8; Nanomanagement −1.2, its clicks worth what the line does with them) — and an event in the Runner's grip at half of that (`DECLARED_VALUE_WEIGHT` 0.5, the Runner's `HELD_CARD_WEIGHT` shape). **The Corp's hand is not held**, and that is the stage's finding (below). (3) *An installed economy card pays over the turns left* (`FUTURE_CREDIT_WEIGHT` 0.25 a credit, `read::future_credits` over `stage::horizon` — 9, 5 and 2 more own turns from an early, middle and late board, read off the baseline's own-turn counts a stage): a turn-start credit a turn (PAD Campaign), a click's use a turn for what it takes over the credit the click would have bought (Regolith Mining License, Telework Contract), both bounded by the counters the credits come off, a cashout at the printed rate (Fermenter), recurring credits — the same reading priced into the Runner's `install_delta`, so an economy resource is live in hand exactly when the Runner would install it, the breaker's arithmetic for money. The first term to read the stage, and the stage is a condition inside it. **Five-eighths of a present credit**, because the term banks the card's future and using the card has to beat banking it: taking 3[c] off Regolith is +0.3 over the credit click at 0.25 and −0.4 at 0.4, where the Corp would never take what it installed; clicking three counters onto Smartware Distributor is +0.35; a PAD Campaign installed early pays 2.25 for its 1.6 of install and rez, and late 0.5 — the same card, a different horizon. The Runner reads the same yield off the Corp's rezzed assets at its half weight, which is precept 12 (trash the Corp's economy) with no term of its own: an early PAD Campaign is worth 4[c] to trash and a late one is not. (4) *The Corp keeps the rez that stops a run* (`REZ_RESERVE_WEIGHT` 0.3 a credit short of `read::rez_reserve`, its dearest face-down piece): the Runner's `SAVINGS_SHORTFALL_WEIGHT`, same shape, same reason — a penalty on the shortfall vanishes when the rez is affordable and never fights it. (5) *A face-down ICE the Corp cannot rez is worth half* (`UNAFFORDABLE_ICE_WEIGHT` 0.5 off its `UNREZZED_INSTALL_WEIGHT`): the price of the promise, not of the rez; the first piece on an open central still goes down as a bluff, because the fort term carries it. (6) *The taxing window, read by the Corp* (`TAXING_WINDOW_WEIGHT` 1.5 an installed agenda in a server the Runner cannot afford to break into — `read::taxing_cost`, the rezzed ICE plus the face-down ICE the Corp's own credits would rez, in the order the Runner meets it — against the Runner's public credits): precept 2, which Stage 1 measured happening 0.49 of scores with nothing reading it. (7) *"Make the Corp rez"* (`FORCED_REZ_WEIGHT` 0.12 a credit the Corp would spend rezzing the unrezzed ICE ahead of the run, `TYPICAL_REZ_COST` 4 a piece out of what it holds): the Corp's credit is worth 0.2 to the Runner and a run into unrezzed ICE drew a rez 0.64 of the time at the baseline; the product. (8) *"Only a threat if the Corp can afford to finish it"* (`FINISHABLE_INSTALL_WEIGHT` 0.5, a face-down root card in the run's server when the Corp's credits and its tokens reach `TYPICAL_ADVANCEMENT_REQUIREMENT` 3): half a token's worth on top of the hidden access, so a fresh install in front of a rich Corp is a remote run over a central and one in front of a broke Corp is a hidden card like any other.

**Measured, and what the first measurement said.** The first cut priced both hands held — a Hedge Fund in HQ at half its play, so the Corp would draw toward it — and the Corp chair lost as much as at Stage 4 (−0.057 / −0.115) with its credits at turn start down again, 7.8 → 4.2. Adding the unaffordable-ICE term moved nothing (−0.068 / −0.104, 4.5 credits). So every term was measured alone, on the same 384 games as the reference leg, one term off at a time (a temporary hook, removed): on the Corp leg, seed 1, the chair with every term on was −0.068, and with the click off −0.096, the future credit off −0.042, the reserve off −0.094, the cover off −0.057, the window off −0.081, **and the held card off +0.031**; in the planner-against-planner precepts pass the same switch took the Corp from 0.286 to 0.396 of games and its economy operations from 2.7 to 3.6 a game, with its HQ full at a third fewer of the Runner's HQ runs. A planner sees a draw's value through the line that plays what it draws, so a held value is not a reason to draw but a reason to hold: the Corp kept the Hedge Funds it could not afford at 4[c] and installed instead. On the Runner leg (seeds 1 / 2, every term on +0.083 / +0.083) the future credit is the gain (off, +0.021), the held event is inside the band (off, +0.060 / +0.081), and the click (off, +0.122 / +0.112), the forced rez (off, +0.099 / +0.099) and the finishable install (off, +0.094 / +0.094) each read a little against the Runner on both seeds, the first past the band's floor and the other two under it — kept, because they are the guide's readings, the Corp gains what the click costs the Runner, and the lines they exist for moved the way they say (below); recorded on each constant. The Corp's hand is priced by the line that plays it, and the term is the Runner's events'.

**Measured on pinned binaries** (the planner on one chair against the reference on the other, 384 games a leg on each of two seeds, paired game by game, McNemar's z over the discordant games; Stage 4's numbers beside them):

| leg | reference (heuristic both) | Stage 4 planner | Stage 5 planner | delta for the planner's chair | z |
|---|---|---|---|---|---|
| planner Corp, seed 1 | Corp 0.432 | Corp 0.312 (−0.120) | Corp **0.464** | **+0.031** | +1.04 |
| planner Corp, seed 2 | Corp 0.453 | Corp 0.346 (−0.107) | Corp **0.448** | **−0.005** | −0.17 |
| planner Runner, seed 1 | Runner 0.568 | Runner 0.586 (+0.018) | Runner **0.651** | **+0.083** | +2.70 |
| planner Runner, seed 2 | Runner 0.547 | Runner 0.607 (+0.060) | Runner **0.630** | **+0.083** | +2.81 |

**The Corp chair is level with the reference again, inside the band on both seeds, and the Runner chair gains past it on both.** Both chairs the planner, deck styles, `diag precepts` on the branch against the reference (`after-heuristic-*`) and against Stage 4's planner reports, Casual seeds 1 / 2 and Startup seeds 1 / 2:

| line | reference Casual | Stage 4 Casual | Stage 5 Casual | reference Startup | Stage 5 Startup |
|---|---|---|---|---|---|
| Corp win share | 0.469 / 0.469 | 0.339 / 0.250 | **0.365 / 0.427** | 0.433 / 0.467 | 0.433 / 0.367 |
| `corp.04.scores_per_game` | 1.58 / 1.52 | 1.22 / 1.05 | **1.46 / 1.58** | 1.48 / 1.52 | 1.31 / 1.22 |
| `economy.corp.economy_operations_per_game` | 2.84 / 2.70 | 3.04 / 2.70 | **3.54 / 3.35** | 3.29 / 3.50 | **3.91 / 3.96** |
| `economy.corp.credits_at_turn_start` | 15.1 / 15.8 | 7.8 / 7.4 | 6.1 / 5.5 | 15.6 / 17.1 | 6.3 / 6.0 |
| `economy.corp.credit_click_share` | 0.41 / 0.41 | 0.14 / 0.15 | 0.24 / 0.23 | 0.35 / 0.37 | 0.21 / 0.21 |
| `economy.corp.turn_starts_able_to_rez_everything` | 0.70 / 0.70 | 0.46 / 0.46 | 0.51 / 0.49 | 0.68 / 0.68 | **0.56 / 0.57** |
| `corp.02.scores_runner_could_not_afford` | 0.47 / 0.45 | 0.55 / 0.60 | **0.68 / 0.69** | 0.32 / 0.58 | **0.94 / 0.88** |
| `corp.03.scores_after_unadvanced_install_turn` | 0.24 / 0.23 | 0.39 / 0.45 | 0.44 / 0.47 | 0.36 / 0.31 | 0.60 / 0.58 |
| `corp.05.agendas_unadvanced_at_turn_end_per_turn` | 0.03 / 0.03 | 0.13 / 0.14 | 0.26 / 0.23 | 0.05 / 0.05 | 0.26 / 0.32 |
| `corp.06.rezzes_per_game` | 9.0 / 8.8 | 8.2 / 8.2 | 10.4 / 10.2 | 7.7 / 7.8 | 10.5 / 10.1 |
| `economy.runner.credits_at_turn_start` | 15.1 / 14.2 | 7.3 / 6.6 | 5.6 / 5.3 | 13.8 / 14.9 | 5.8 / 5.8 |
| `economy.runner.credit_click_share` | 0.51 / 0.51 | 0.26 / 0.26 | **0.17 / 0.19** | 0.49 / 0.51 | **0.19 / 0.18** |
| `runner.11.economy_installs_per_game` | 0.42 / 0.48 | 1.24 / 1.07 | **1.98 / 1.85** | 0.34 / 0.28 | **1.96 / 1.99** |
| `runner.11.first_breaker_turn` | 2.92 / 2.57 | 2.00 / 2.04 | 1.98 / 2.20 | 3.41 / 2.69 | 2.09 / 2.61 |
| `runner.12.economy_assets_trashed_when_accessed` | 0.14 / 0.18 | 0.23 / 0.17 | **0.31 / 0.25** | 0.10 / 0.15 | **0.30 / 0.34** |
| `runner.12.trashes_on_access_per_game` | 0.85 / 0.85 | 1.11 / 1.01 | **1.93 / 1.73** | 0.74 / 0.80 | **1.59 / 1.47** |
| `runner.08.runs_per_game` | 14.5 / 14.0 | 15.7 / 15.9 | **18.6 / 18.3** | 13.9 / 15.5 | — |
| `runner.08.runs_into_unrezzed_ice` | 0.45 / 0.46 | 0.55 / 0.54 | **0.58 / 0.59** | 0.47 / 0.42 | **0.62 / 0.61** |
| `runner.14.remote_runs_share` | 0.22 / 0.21 | 0.28 / 0.27 | **0.38 / 0.38** | 0.20 / 0.18 | **0.39 / 0.40** |
| `runner.13.runs_on_the_last_click` | 0.14 / 0.13 | 0.39 / 0.38 | 0.48 / 0.47 | 0.16 / 0.16 | 0.49 / 0.50 |
| `runner.14.steals_per_game` | 2.54 / 2.54 | 2.84 / 2.99 | 2.69 / 2.60 | 2.49 / 2.56 | 2.62 / 2.67 |
| `stage.runner.late.runs_per_turn` | 0.76 / 0.76 | 1.22 / 1.32 | 1.39 / 1.34 | 0.74 / 0.71 | 1.37 / 0.92 |

**What the lines say.**

- **The Corp plays its economy and scores again.** Economy operations a game rise past the reference's (3.5 against 2.8, 3.9 against 3.3 on Startup — a Hedge Fund is priced by the line that plays it and no longer worth holding), scores a game are the reference's (1.46 / 1.58 against 1.58 / 1.52), and the Corp can rez what it holds at half its turn starts against Stage 4's 0.46. Its credits at turn start are still six where the chooser's were fifteen: **it spends them** — 10.4 rezzes a game against 9.0, its assets rezzed for their income, and scores under the window 0.68 / 0.69 of the time (0.94 / 0.88 on Startup) against the reference's 0.47 — which is precept 2 played on purpose. The unadvanced install at turn end (0.26 a turn) is the never-advance line waiting for its Seamless Launch, and its condition — "an install the Corp can finish next turn" — is Stage 6's.
- **The Runner plays economy first, trashes the Corp's, and runs where the Corp is scoring.** Economy installs a game 0.4 → 2.0 on both formats (precept 11: Phase 2 §5a's owed lever, closed), the credit click a sixth of the turn where it was half, the first breaker a turn earlier, economy assets trashed on access 0.14 → 0.31 and trashes a game doubled (precept 12), and a run into unrezzed ICE 0.45 → 0.58 (precept 8) — with the remote's share of runs 0.22 → 0.38 (the finishable-install reading) and 18.6 runs a game. Late-stage runs stay at Stage 4's 1.4 a turn.
- **Precept 13 goes further the wrong way** (runs on the last click 0.48, 0.14 at the reference) for Stage 4's reason: the credits first make the run affordable, and the fear that makes the last click the wrong one is Stage 7's term.
- **Reach**: Casual's never-used list falls 66 → **42** (Stage 4: 45; Azimat, IP Enforcement, Knickknack O'Brian, Pennyshaver and Retribution newly played; Fransofia Ward and Red Team fell off), Startup's 60 → 43 (Stage 4: 42; Azimat, Bigger Picture, Maglectric Rapid, Scrounge and T400 Memory Diamond newly played; Botulus, Charm Offensive, Fransofia Ward, Neurospike, Red Team and Verbal Plasticity fell off).
- **The knowledge lines are unmoved**, as an evaluator change must leave them.

**Rejected.** A flat value per card in HQ (measured, a switch; `RD_DRAW_RESERVE`), and now a declared one (measured, a reason to hold). Pricing the future at the present credit's weight (using an economy card becomes a loss against holding it; the arithmetic is on `FUTURE_CREDIT_WEIGHT`). Counting a click that places counters (Smartware Distributor) as future clicks as well as the stock it places — the click is priced by the stock it adds when it is taken. A rez reserve over the *sum* of unrezzed rez costs — that is the chooser's own hoarding, and the guide's sentence is "the rez that stops it". Reading the unrezzed ICE's real cost for the forced-rez term — it is the sampled card, and the term would read the guess. Turning the two Runner affordance terms off at −0.01 on two seeds — under the band, and `UNREZZED_THREAT_WEIGHT` was shipped off for losing 0.04–0.06, not 0.01.

**Cost** (release, one thread, an idle machine, a 96-game Casual pass): heuristic both chairs 37.3 s; the planner as Corp 175 s (4.7×; Stage 4 128 s), as Runner 191 s (5.1×; 125 s), on both chairs 348 s (9.3×; 211 s). Not the evaluator's terms: the slowest decision seen is **0.42 s** against Stage 4's 0.6 (`planner::cost::planner_cost`, six games at 1.9–6.6 s), and what grew is the number of decisions — the Runner runs 18.6 times a game against 15.7, each run a handful of one-ply decisions on a fresh sample, and games run longer (one of the six reached turn 38). A decision in the desktop client stays well under a second, and the planner is not on the ladder in any case. The 256-seed sweeps, with the planner seated, run 113 s (index) and 171 s (view) in release, against 85 s and 108 s at Stage 4, green. `cargo test --workspace` green, clippy silent; the engine's random and heuristic seatings identical to `main` in all four `coverage_identical.py` shapes.

#### Stage 6 — The Corp's plans, stacked by a deck — DONE (`feat/corp-plans`, 29 September 2026)

**A style is a list of plans; a plan is the terms that exist for it.** `Personality` is gone, replaced outright by `netrunner_bots::plans::{Plan, Style}`: a `Plan` is `Glacier`, `FastAdvance`, `Kill` or `Traps` for the Corp (the guide's "Corp deck styles" chapter) and, until Stage 7 reads them off the faction, `Aggressive`, `Cautious`, `Builder` or `Wary` for the Runner; a `Style` is the plans a deck stacks, up to three, one chair's, no plan twice — `DeckFile::style` is now a list (`["glacier", "fast-advance"]`), `--corp-style`/`--runner-style`, the daemon's `--bot-style` and `bench --bots planner:glacier+fast-advance` spell it with `+`, and `rush` and `trap` are `fast-advance` and `traps`. The eight profiles' numbers are kept exactly under the plans' names (`Plan::weights`), and **the one-ply reference scores with the first plan's profile and nothing else** (`Style::weights`), so a deck that now stacks two plans is played by the reference as it played its one style and the reference has not moved by a bit: the planner's Runner leg on seed 1 is the same 384 games to the game (0 discordant against Stage 5's), and `coverage_identical.py` says identical in all four shapes. Only `PlanningAgent` reads the whole list (`Style::planned_weights(side)`, `Weights::with_plans`): a plan after the first adds its own levers — `Traps`' ambush terms, `FastAdvance`'s installed agenda — and not its profile's dials on the shared terms, which are the first plan's. Four of the 28 sample decks stack two (*Agency*, *Brick Stack* and *Discretion Advised* glacier then fast advance, *Pork Chops* glacier and traps), read off their own how-to-play notes.

**The terms** — every one zero in the default and in every profile, switched on by `with_plans` for a planner seat (the same pin as Stage 5's). Four are every Corp's, because the guide's sentences for them are in its "Playing the Corp" chapter, which every Corp plays, so a balanced planner Corp carries them too: (1) *the stakes of a run* (`RUN_STAKES_WEIGHT` 2.0 an agenda point the breach would reach — the remote's root, HQ's agendas at one access in its size, R&D's at the deck's density, all of Archives — subtracted while the run is breakable, and the credits the Runner still has to spend breaking the rezzed ICE ahead counted as the Corp's at `OPPONENT_CREDIT_WEIGHT`: the reference's run term is flat, so a rez that stops a run on an empty remote was worth the same as one that stops a run on a 3-point agenda); (2) *the rez held* (`REZ_HELD_WEIGHT`, a face-down affordable piece **in the server being run**, kept: "rez it when the run matters, not the moment it is approached" — the reference rezzes every affordable piece at its approach; paid only in the run's server because paid on every face-down piece it was also a reason to install every affordable piece in hand, and the Corp does not install during a run; **measured at 1.5 and shipped at zero**, below); (3) *the ICE order* (`ICE_ORDER_WEIGHT` 0.3 a taxing piece outside a stopping one, the guide's "starting point, not a law"); (4) *the never-advance line's condition* (`NEVER_ADVANCE_WEIGHT` 1.0 an installed agenda the Corp can finish next turn — `read::next_turn_advancements`: three clicks at a token and a credit, and what the operations in HQ declare, Seamless Launch's two for a click and a credit — and −1.0 one it cannot: "an agenda you cannot finish next turn is an agenda sitting in the open for a second turn", the condition Stage 5 said was this stage's). The rest are one plan's: the kill plan's *lethal threat* (`LETHAL_THREAT_WEIGHT` 3.0 when the Runner's grip is under the damage the operations in HQ could deal next turn, their play requirements met now — Scorched Earth's "if the Runner is tagged" — within the Corp's clicks and credits) and *tag leverage* (`TAG_LEVERAGE_WEIGHT` 1.5 a tag, up to two, while the Corp holds a card whose text asks for one, `read::punishes_tags`); the traps plan's *bluff* (`BLUFF_WEIGHT` 1.0 for one face-down card that is no agenda in the scoring remote, which `fort_value` reads as the fort still, so the asset goes down behind the wall and the agenda goes in over it); and the stack, *glacier then fast advance* (`Weights::fort_until_beaten`, one condition inside the fort terms: they fall away once there is a wall of the fort's depth in front of the scoring remote, the Runner's rig covers every piece of it and their credits cover the break, `read::fort_beaten` — "score the last points from hand, where that rig is no use"). Traps beside glacier is "traps behind the wall": glacier's dials, the ambush terms on.

**The planner answers an opponent's yes-or-no the way that is worst for the seat, and goes on** (`planner::opponents_answer`). Found by the kill deck: Public Trail — "give the Runner 1 tag unless they pay 8[c]" — was played 0 times in 116 turns the planner held it, under the kill plan or any other, because a decision only the opponent can make ended the line and the line was scored on the parked state, where no tag had been given and no credits paid, with its clicks unspent at a credit each against lines that spent them. Now each of the opponent's answers is applied and settled, the one that leaves the seat worst off is taken as theirs, and the line continues from there; if the real answer is the other one, the view is not the one predicted and the turn is planned again, which is the rule every step already plays under. One node of minimax, never a search of the opponent's turn. With it, a kill Corp holding Scorched Earth against a grip of three tags first (`a_kill_corp_plays_public_trail_because_the_runners_answer_is_priced`); a balanced Corp, reading no leverage in a tag, does not; and neither does the kill Corp against a grip the damage would not reach, because a tag's leverage alone is under Public Trail's price.

**A Sweep deck holds the kill plan's payoffs** (`tag_youre_it`, *Tag, You're It*: Scorched Earth, Retribution, Public Trail, Clearinghouse and Orbital Superiority behind Ping and Funhouse, on Building a Better World, Eternal-legal, `style: ["kill"]`), because no sample deck holds Scorched Earth (§25's brief, precept 7) and the plan is otherwise a plan with nothing to play. It is played by both agent sweeps and by no measurement over the pool, like every Sweep deck.

**Measured, and what the measurements said.** Three things moved during the stage, each found by a measurement. (1) *The fort fell away before there was a fort*: the first styled legs lost on the stacked decks on both seeds (−0.10 / −0.08 for *glacier then fast advance*) and the precepts pass showed the Corp installing agendas naked from turn two, because `fort_beaten` read a remote with no ICE as covered (nothing to cover) and affordable (nothing to pay) — it now needs a wall of the fort's depth first, and the condition measures level (stack on against stack off on the same games: −0.097 / 0.000 against −0.111 / +0.014). (2) *The kill plan had no lever*: on the kill deck the planner played Public Trail 0 times in 116 turns it held it under either style, because the line ended at the Runner's choice — the planner now answers an opponent's yes-or-no the worst way for the seat and continues (above), and Public Trail is played 10–23 times in 48 games under either style, with flatlines 8 → 14–16 of 48. (3) *The rez held costs the chair*, one term at a time on the balanced Corp legs (a temporary hook, removed): with every term on, +0.109 / +0.021 against the reference on seeds 1 / 2; the stakes off −0.060 / −0.081 (the term carries **+0.17 / +0.10**); the never-advance line off +0.060 / +0.010 (carries +0.05 / +0.01); the order off +0.096 / +0.026 (neutral); **the rez held off +0.122 / +0.073** (it costs 0.013 / 0.052, the second past the band) — it does what it says (rezzes a game 10.4 → 9.3, rezzes the Runner could break 0.46 → 0.41) and what it holds is the tax, so it ships at zero with the record on its constant, as `UNREZZED_THREAT_WEIGHT` does.

**Measured on pinned binaries** (the planner on one chair against the reference on the other, 384 games a leg on each of two seeds, paired game by game, McNemar's z over the discordant games; the styled legs seat both chairs in their decks' own styles, `bench --deck-styles`, new, against the reference seated the same way):

| leg | reference | Stage 5 planner | Stage 6 planner | delta for the planner's chair | z |
|---|---|---|---|---|---|
| planner Corp, balanced, seed 1 | Corp 0.432 | 0.464 (+0.031) | Corp **0.555** | **+0.122** | +3.90 |
| planner Corp, balanced, seed 2 | Corp 0.453 | 0.448 (−0.005) | Corp **0.526** | **+0.073** | +2.30 |
| planner Corp, deck styles, seed 1 | Corp 0.435 | — | Corp **0.557** | **+0.122** | +4.14 |
| planner Corp, deck styles, seed 2 | Corp 0.440 | — | Corp **0.534** | **+0.094** | +3.09 |
| planner Runner, seed 1 / 2 | Runner 0.568 / 0.547 | 0.651 / 0.630 | the same 384 games to the game | +0.083 / +0.083 | — |

By the Corp deck's style, on the styled legs (seed 1 / seed 2; the reference plays the first plan): balanced decks (72 games) +0.278 / +0.181; *fast-advance* (144) +0.097 / +0.056; *traps* (48) +0.167 / +0.021; *glacier* (24) +0.083 / +0.042; *glacier then fast advance* (72) −0.111 / +0.014 — the three decks the reference already plays best (0.63 / 0.54 as glacier), and inside the band at 72 games; *glacier and traps* (24, *Pork Chops*: BANGUN's sixteen damage effects behind the fort) **+0.458 / +0.500**, the ambush terms on beside glacier's dials.

**The Corp chair is past the reference on both seeds, in both seatings, for the first time since the planner existed** (Stage 4 −0.12, Stage 5 level). Both chairs the planner, deck styles, `diag precepts` against the reference (`after-heuristic-*`) and Stage 5's planner (`planner5-*`), Casual seeds 1 / 2 and Startup seeds 1 / 2:

| line | reference Casual | Stage 5 Casual | Stage 6 Casual | reference Startup | Stage 6 Startup |
|---|---|---|---|---|---|
| Corp win share | 0.469 / 0.469 | 0.365 / 0.427 | **0.432 / 0.479** | 0.433 / 0.467 | **0.556 / 0.544** |
| `corp.04.scores_per_game` | 1.58 / 1.52 | 1.46 / 1.58 | 1.53 / 1.48 | 1.33 / 1.63 | 1.41 / 1.67 |
| `corp.02.scores_runner_could_not_afford` | 0.47 / 0.45 | 0.68 / 0.69 | **0.74 / 0.72** | 0.32 / 0.58 | **0.88 / 0.85** |
| `corp.03.scores_after_unadvanced_install_turn` | 0.24 / 0.23 | 0.44 / 0.47 | **0.54 / 0.46** | 0.36 / 0.31 | 0.50 / 0.52 |
| `corp.03.advancement_by_cards_per_game` | 0.49 / 0.51 | 0.57 / 0.67 | **0.73 / 0.62** | 0.53 / 0.70 | **0.82 / 0.74** |
| `corp.05.agendas_unadvanced_at_turn_end_per_turn` | 0.03 / 0.03 | 0.26 / 0.23 | 0.22 / 0.20 | 0.05 / 0.05 | 0.23 / 0.25 |
| `corp.05.asset_installs_in_iced_remote_per_game` | 0.32 / 0.20 | 0.22 / 0.20 | 0.23 / 0.35 | 0.24 / 0.31 | 0.34 / 0.29 |
| `corp.06.non_etr_installed_over_etr` | 0.24 / 0.25 | 0.27 / 0.24 | **0.18 / 0.15** | 0.24 / 0.22 | **0.14 / 0.18** |
| `corp.06.rezzes_per_game` | 9.0 / 8.8 | 10.4 / 10.2 | 10.1 / 9.9 | 7.7 / 7.8 | 9.9 / 9.6 |
| `corp.07.meat_damage_per_game` | 0.09 / 0.06 | 0.34 / 0.36 | 0.28 / 0.28 | 0.13 / 0.13 | 0.49 / 0.42 |
| `corp.07.flatlines` | 0.08 / 0.08 | 0.08 / 0.09 | **0.17 / 0.18** | 0.11 / 0.08 | **0.29 / 0.20** |
| `corp.07.tags_given_per_game` | 1.36 / 1.32 | 1.68 / 1.81 | 1.70 / 1.59 | 1.48 / 1.94 | 2.28 / 2.57 |
| `economy.corp.economy_operations_per_game` | 2.84 / 2.70 | 3.54 / 3.35 | 3.47 / 3.29 | 3.29 / 3.50 | 3.86 / 4.08 |
| `economy.corp.credits_at_turn_start` | 15.1 / 15.8 | 6.1 / 5.5 | 4.9 / 5.2 | 15.6 / 17.1 | 5.7 / 5.2 |
| `economy.corp.turn_starts_able_to_rez_everything` | 0.70 / 0.70 | 0.51 / 0.49 | 0.50 / 0.50 | 0.68 / 0.68 | 0.51 / 0.51 |
| `runner.08.runs_per_game` | 14.5 / 14.0 | 18.6 / 18.3 | 17.8 / 18.2 | 13.9 / 15.5 | 17.7 / 16.8 |
| `runner.14.remote_runs_share` | 0.22 / 0.21 | 0.38 / 0.38 | 0.35 / 0.36 | 0.20 / 0.18 | 0.40 / 0.38 |
| `runner.13.runs_on_the_last_click` | 0.14 / 0.13 | 0.48 / 0.47 | 0.48 / 0.48 | 0.16 / 0.16 | 0.47 / 0.47 |
| `runner.14.steals_per_game` | 2.54 / 2.54 | 2.69 / 2.60 | 2.52 / 2.61 | 2.49 / 2.56 | 2.39 / 2.36 |

**What the lines say.**

- **The Corp scores under the window and finishes what it installed.** Scores the Runner could not afford 0.68 → 0.74 / 0.72 (0.88 / 0.85 on Startup, against the reference's 0.32 / 0.58); scores the turn after an unadvanced install 0.44 → 0.54 / 0.46, advancement placed by cards 0.57 → 0.73 / 0.62 a game (Seamless Launch, Touch-ups and Key Performance Indicators all reached, on both formats); unadvanced agendas left at turn end 0.26 → 0.22 / 0.20 a turn and 0.23 / 0.25 on Startup — the never-advance line's condition doing what it says, an install made when it can be finished. Scores a game are the reference's (1.53 / 1.48 against 1.58 / 1.52; 1.41 / 1.67 against 1.33 / 1.63), and the Runner steals fewer (2.52 / 2.61 against Stage 5's 2.69 / 2.60, 2.39 / 2.36 against 2.62 / 2.67 on Startup).
- **ICE goes down in the guide's order** — a taxing piece installed outside a stopping one 0.27 / 0.24 → 0.18 / 0.15 (0.14 / 0.18 on Startup) — and the Corp still rezzes on approach (10.1 / 9.9 a game; the rez held ships at zero, above).
- **The Corp plays for the kill it holds.** Flatlines 0.08 → 0.17 / 0.18 of games (0.29 / 0.20 on Startup), meat damage 0.28 / 0.28 a game against the reference's 0.09 / 0.06, on the same tags (1.70 / 1.59 given) — the lethal check inside the turn, which every planner sees now that an opponent's answer is priced, and Pork Chops' damage behind the wall.
- **Precept 13 is still the Runner's**: runs on the last click 0.48, unchanged since Stage 5, for Stage 7's fear term; the Runner's chair did not move.
- **Reach**: Casual's never-used list is 42, as Stage 5's (Carnivore, Fransofia Ward and Measured Response newly played; IP Enforcement, Pennyshaver and Tranquilizer fell off); Startup's 44 (Stage 5: 43). Public Trail is played on Casual for the first time (the styled seating's *Not So Subtle* and *Paid Content* hold it).

**The kill plan on its deck** (*Tag, You're It* as the planner against the reference Runner on *Stolen Goods* and *Dashing Mad*, 48 games each, `--corp-style kill` against `balanced` — the same planner without the two kill terms): *kill* 31 / 38 Corp wins of 48 against *balanced*'s 35 / 37, Public Trail played 16 / 20 times against 12 / 12, flatlines 11 / 17 against 17 / 14, tags given 107 / 96 against 102 / 88. **The two kill terms measure level on their own deck** (−0.04 / +0.02 of 48 games each, inside the band), and the plan's lever was the planner's answer to the Runner's choice, which every planner Corp has: before it, Public Trail was played 0 times in 116 turns and the deck flatlined 8 of 48 under either style; after it, 12–20 plays and 14–17 flatlines. The terms stay at the values that win the unit-test decision (a tag when the punishment is in hand and the grip is under its damage), with this recorded, because a kill plan that reads no leverage in a tag is not a plan.

**Rejected.** The rez held on every face-down affordable piece (it was a reason to install every affordable piece in hand; the run's server alone has no such side). A rez-timing term on the rezzed piece, subtracted while the rig breaks it (it cannot be undone, and it read the same on a piece worth rezzing). The fort falling away for any remote (a remote with no ICE is not a wall anyone beat: read without a depth, every fresh remote was "beaten" the turn it was made, the fort terms fell away before there was a fort, and the Corp installed agendas naked from turn two — 0.15 of agenda installs, the first measurement's number). Pricing the opponent's answer as a leaf rather than continuing the line from it (the tag line ended with two clicks unspent at a credit each and lost to any line that spent them). Extending the existing NBN Sweep deck with the kill payoffs (23 influence against 15: the three cards over are the ones that deck exists to reach).

**Cost** (release, one thread, a 96-game Casual pass): the planner as Corp 169 s (Stage 5 175 s), as Runner 183 s (191 s), on both chairs 249 s (348 s: games end sooner, with the Corp scoring under its window and the kill it holds). The reference read 46.5 s in this run against 37.3 s in Stage 5's on byte-identical code — timing noise on the machine, so the ratios are this run's (3.6× / 3.9× / 5.4×) and the absolute numbers are the comparison with Stage 5. The slowest decision seen is **0.34 s** against Stage 5's 0.42 (`planner::cost::planner_cost`, six games at 0.4–7.4 s, one to turn 47), the opponent's answer included. The 256-seed sweeps, with the planner seated, run 116 s (index) and 161 s (view) in release, against 113 s and 171 s at Stage 5, green. `cargo test --workspace` green, clippy silent; the engine's random and heuristic seatings identical to `main` in all four `coverage_identical.py` shapes.

#### Stage 7 — The Runner's plans, and the identity both chairs read — DONE (`feat/runner-plans`, 29 September 2026)

**The Runner's plans are the guide's three factions, and the seat's own identity names one when the deck does not.** `Plan::{Dismantle, Pressure, Rig}` — the "Anarch: tear it down", "Criminal: take their money" and "Shaper: build the perfect rig" chapters — replace `Aggressive`, `Cautious`, `Builder` and `Wary` outright (`aggressive`, `cautious`, `builder`, `wary` are no longer names a deck file, a flag or a bench spec accepts). Two of the old profiles keep their numbers under the plans' names for the reference — `Pressure` is `Aggressive`'s, `Rig` is `Builder`'s — and `Dismantle`, like `Kill`, is the planner's terms and no profile; `Cautious` and `Wary` were one deck's and two decks' dials with no chapter behind them, and those three decks (*Enthusiasm*, *Planning Ahead*, *Sabbatical*, all Shaper) now name no style, so the reference plays them balanced and the planner plays them as a rig. **A Runner seat given no style reads its own identity off the first view it acts on and plays that faction's plan** (`Plan::for_faction`, `Style::or_faction`, `PlanningAgent::weights_for`): the identity is public in both chairs' views, so a seat with no deck to read still knows its chapter, and a saved deck with no style is not a balanced one to the planner. The deck's list overrides the identity; a Corp faction names nothing (the Corp's plans are ways to make a window, not a faction's); the reference reads no identity and stays balanced on an unstyled deck, so it has not moved there. The sixteen styled Runner decks are re-tagged from their factions and their own notes: the six Criminal run-event decks `["pressure"]`, *Dashing Mad* (Anarch, ten run events) `["pressure", "dismantle"]`, *Flow and Ebb* (Shaper, "runs early") `["pressure", "rig"]`, the five Anarch program decks `["rig", "dismantle"]`, the three Shaper Sweep decks `["rig"]`.

**A Runner plan is its lever alone to the planner, and its profile is the reference's** (`Style::planned_weights`: the Corp's dials are the first plan's, the Runner's are balanced). This was the first thing the measurement found. With the faction default seating the old profiles on every unstyled deck, the planner's Runner leg against the reference was 0.03 and 0.07 of win share under the same leg with no profile (384 games a seed, the last-click and stakes terms off in both), and by faction the Criminal and Shaper decks were each about 0.1 under Stage 6's while the Anarch decks, whose plan has no profile, were level or up. The profiles were tuned against a chooser with no economy term — a run at 1.2 for a seat that already prices what a run finds, a rig card at 0.4 for a seat that prices what an economy card pays — and under the guide's rate each is a dial turned the wrong way. So the Corp's profiles carry into the planner as Stage 6 measured them and the Runner's do not, and both chairs' references are pinned as before.

**The terms** — every one zero in the default and in every profile, switched on by `with_plans` for a planner Runner seat, four every Runner's (the "Playing the Runner" chapter) and one each plan's: (1) *the feared flatline* (`FEARED_FLATLINE_WEIGHT` 3.0 while the grip is no larger than the damage one access could deal, `read::damage_feared` — the most a trap the Runner has seen would do, and against a **Jinteki** identity at least Snare!'s three; paid on the table, not the run, so the draw comes before the run and a run under way is never left for it); (2) *the unshown breaker* (`UNSHOWN_BREAKER_WEIGHT` 1.5 off each covered subtype the Corp has never shown — no piece rezzed, none once rezzed and face down again, none face up in Archives, `read::ice_shown` — on the table and in hand alike, and such a breaker is not saved for: "install breakers for the ICE the Corp has actually rezzed"; Cleaver for a Barrier nobody has seen is +0.8 rather than +2.3, behind an economy card at the guide's rate and ahead of a credit); (3) *the last click* (`LAST_CLICK_RUN_WEIGHT`: a run begun on the last click against a Corp that punishes runs — a Jinteki or NBN identity, or a rezzed piece of ICE that tags or damages, `read::corp_punishes_runs` — with something unknown in the way, read at the run's initiation only so no jack-out is ever worth it; **measured at 1.0 and 0.4 and shipped at zero**, below); (4) *the stakes as the Runner counts them* (`RUNNER_STAKES_WEIGHT`: each agenda point a central breach is expected to reach — R&D at the deck's density, the points a deck its size must hold over its cards; HQ at what the Corp has drawn less what it has scored, lost and shown, spread over the drawn cards the Runner has not seen, `read::agenda_points_expected` — the mirror of the Corp's `RUN_STAKES_WEIGHT` under ignorance, "HQ when the Corp is holding cards without scoring"; **measured at 1.0 and 0.5 and shipped at zero**, below); the pressure plan's *HQ* (`HQ_PRESSURE_WEIGHT` 0.4 a fresh HQ access: "they punish an exposed HQ"); the dismantle plan's *trash* (`DISMANTLE_WEIGHT` 0.5 off each rezzed asset or upgrade on the Corp's table: the balanced Runner trashes at 2[c] and not 3[c], an Anarch at 3[c] and not 4[c], and the run that reaches one is worth starting for it); the rig plan's *R&D* (`RD_ACCESS_WEIGHT` 0.4 an R&D access beyond the first, on the run that makes it and on the card that promises it, `read::rd_accesses` — The Maker's Eye's two, a counter on Conduit; breakers that scale need no term, since `continuous::breaker_strength` reads the rig). Every reading is public: the identity across the table, the ICE the Corp has turned face up, the counts of what it has drawn and scored.

**Measured, one term at a time on the planner's Runner legs against the reference** (384 games a seed, seeds 1 and 2, pinned binaries, paired; Stage 6's leg is +0.083 / +0.083; a temporary hook, removed): with every term on and the profiles seated by the faction default, **+0.021 / +0.029**; the faction default off, +0.042 / +0.021; then the profiles taken out of the planner (above) and the terms one at a time: everything on +0.023 / +0.047; the last click off +0.044 / +0.062; the stakes off +0.055 / +0.081; both off **+0.073 / +0.109**; both off and the plans' levers off +0.078 / +0.122; the stakes at half a point with the last click off +0.083 / +0.076. So the last click costs 0.02 and 0.02 (0.02 and 0.03 with the profiles in, 0.00 and 0.03 at a click's 0.4: six legs, every one negative — the runs it forbids are the poor Runner's, for which the third credit is the run, and the reference Corp punishes too few of them for the click kept to pay); the stakes cost 0.03 and 0.03 at a point and 0.01 and 0.03 at half (by faction at half, the Anarch decks +0.06 / +0.04 and the Criminal and Shaper decks −0.01 / −0.08 and −0.02 / −0.06 — **not explained**: a stake counted once a run rather than once an access played byte-identical games, because a run event's extra accesses resolve behind the initiation window, past the leaf, so it is not those decks' multi-access cards); the three levers together are level (−0.005 / −0.013, inside the band, and each wins its unit decision); the feared flatline and the unshown breaker together are Stage 6's leg or a little over it. Both cost terms ship at zero with their records on their constants, as `REZ_HELD_WEIGHT` did, and their readings stay.

**Measured on pinned binaries**, the shipped configuration (the planner on one chair against the reference on the other, 384 games a leg on each of two seeds, paired game by game, McNemar's z; the styled legs seat both chairs in their decks' own styles against a reference re-taken on this branch, because three decks' styles changed):

| leg | reference | Stage 6 planner | Stage 7 planner | delta for the planner's chair | z |
|---|---|---|---|---|---|
| planner Runner, unstyled (the planner in its faction's plan), seed 1 | Runner 0.568 | 0.651 (+0.083) | Runner **0.641** | **+0.073** | +2.47 |
| planner Runner, unstyled, seed 2 | Runner 0.547 | 0.630 (+0.083) | Runner **0.656** | **+0.109** | +3.66 |
| planner Runner, deck styles, seed 1 / 2 | Runner 0.552 / 0.531 | — | Runner **0.617 / 0.602** | **+0.065 / +0.070** | +2.11 / +2.43 |
| planner Corp, balanced, seed 1 / 2 | Corp 0.432 / 0.453 | 0.555 / 0.526 | the same 384 games to the game | +0.122 / +0.073 | — |
| planner Corp, deck styles, seed 1 / 2 | Corp 0.448 / 0.469 | 0.557 / 0.534 (against Stage 6's reference) | Corp **0.557 / 0.539** | **+0.109 / +0.070** | +3.55 / +2.32 |

By the Runner's faction on the unstyled legs (seed 1 / seed 2; Stage 6's in brackets): Anarch **+0.094 / +0.094** (+0.078 / +0.031), Criminal +0.023 / +0.141 (+0.094 / +0.109), Shaper **+0.102 / +0.094** (+0.078 / +0.109). By the Runner deck's style on the styled legs: the three unstyled Shaper decks and the two neutral ones (128 games) +0.102 / +0.047; *pressure* (96) 0.000 / +0.042; *pressure then dismantle* (32) +0.094 / −0.031; *pressure then rig* (32) +0.062 / +0.219; *rig then dismantle* (96) +0.073 / +0.115. The styled reference itself moved only on the three decks that lost their style — as balanced they win 0.04 and 0.09 fewer of their 96 games than as cautious and wary — and on no other deck (0 discordant games in every other group), which is why the Corp's styled legs are re-quoted against it: the Corp planner's weights and its balanced legs are byte-identical to Stage 6's.

**The Runner chair is where Stage 6 left it, in every faction's plan.** Both chairs the planner, deck styles, `diag precepts` against Stage 6's planner (`planner6-*`) and the reference (`after-heuristic-*`), Casual seeds 1 / 2 and Startup seeds 1 / 2 — the lines this stage names:

| line | reference Casual | Stage 6 Casual | Stage 7 Casual | Stage 6 Startup | Stage 7 Startup |
|---|---|---|---|---|---|
| Corp win share | 0.469 / 0.469 | 0.432 / 0.479 | **0.411 / 0.411** | 0.556 / 0.544 | **0.478 / 0.500** |
| `runner.11.breakers_for_ice_already_shown` | 0.51 / 0.50 | 0.49 / 0.48 | **0.70 / 0.73** | 0.47 / 0.52 | **0.70 / 0.68** |
| `runner.11.economy_installs_before_first_breaker` | 0.26 / 0.28 | 0.27 / 0.31 | **0.43 / 0.46** | 0.31 / 0.33 | **0.48 / 0.42** |
| `runner.11.first_breaker_turn` | 2.2 / 2.3 | 2.1 / 2.3 | **3.4 / 3.5** | 2.1 / 2.5 | **3.7 / 3.5** |
| `runner.11.breaker_installs_per_game` | 2.2 / 2.2 | 2.3 / 2.4 | 2.2 / 2.2 | 2.3 / 2.3 | 2.1 / 2.1 |
| `runner.12.trashes_on_access_per_game` | 1.1 / 1.1 | 1.7 / 1.7 | **2.6 / 2.5** | 1.5 / 1.2 | **2.3 / 2.0** |
| `runner.12.economy_assets_trashed_when_accessed` | 0.24 / 0.22 | 0.32 / 0.29 | 0.36 / 0.36 | 0.33 / 0.31 | **0.44 / 0.44** |
| `runner.13.flatlined` | 0.08 / 0.08 | 0.17 / 0.18 | **0.14 / 0.12** | 0.29 / 0.20 | 0.30 / 0.27 |
| `runner.13.flatlined`, against Jinteki | — | 0.48 / 0.33 | **0.31 / 0.29** | — | — |
| `runner.13.runs_on_the_last_click` | 0.14 / 0.13 | 0.48 / 0.48 | 0.47 / 0.47 | 0.47 / 0.47 | 0.46 / 0.47 |
| `runner.09.hq_runs_share` | 0.26 / 0.27 | 0.28 / 0.28 | 0.30 / 0.31 | 0.27 / 0.27 | 0.28 / 0.29 |
| `runner.10.rnd_runs_share` | 0.30 / 0.30 | 0.26 / 0.25 | 0.19 / 0.21 | 0.27 / 0.29 | 0.23 / 0.23 |
| `runner.14.remote_runs_share` | 0.22 / 0.21 | 0.35 / 0.36 | 0.40 / 0.37 | 0.40 / 0.38 | 0.38 / 0.36 |
| `runner.08.runs_per_game` | 14.5 / 14.0 | 17.8 / 18.2 | 19.4 / 18.8 | 17.7 / 16.8 | 18.4 / 17.7 |
| `runner.14.steals_per_game` | 2.54 / 2.54 | 2.52 / 2.61 | 2.63 / 2.64 | 2.39 / 2.36 | 2.53 / 2.54 |
| `corp.06.rezzes_the_runner_could_break` | 0.45 / 0.46 | 0.46 / 0.49 | **0.37 / 0.35** | 0.45 / 0.43 | **0.37 / 0.37** |
| `corp.04.scores_per_game` | 1.58 / 1.52 | 1.53 / 1.48 | 1.57 / 1.47 | 1.41 / 1.67 | 1.44 / 1.58 |

**What the lines say.**

- **Breakers come after economy and for the ICE shown.** A breaker installed for a subtype the Corp had already rezzed 0.49 → 0.70 / 0.73 (0.70 / 0.68 on Startup), economy installs made before the first breaker 0.27 → 0.43 / 0.46, and the first breaker on turn 3.4 / 3.5 instead of 2.1 / 2.3 — with as many breakers a game by the end (2.2 against 2.3) and the Corp's rezzes the rig could break at the rez 0.46 → 0.37 / 0.35, because the rig is bought for what is on the table. Precept 11.
- **The Anarch dismantles.** Trashes on access 1.7 → 2.6 / 2.5 a game (the reference's 1.1), economy assets trashed when accessed 0.32 → 0.36 / 0.36 and 0.44 / 0.44 on Startup; Gourmand and (on Startup) Carnivore are reached for the first time. Precept 12.
- **Fewer flatlines against Jinteki**: 0.48 / 0.33 → 0.31 / 0.29 of those games, and 0.17 → 0.14 / 0.12 of all — the feared flatline draws a card first. Precept 13's grip.
- **The last click is unchanged** (0.48 → 0.47), because its term measured as a cost and ships at zero; and the Runner runs more, not less (17.8 → 19.4 / 18.8 a game), with R&D's share down (0.26 → 0.19 / 0.21) and the remote's up (0.35 → 0.40 / 0.37) — the rig plan's R&D lever notwithstanding, and level on the bench.
- **The Corp chair reads lower in this pass** (0.432 / 0.479 → 0.411 / 0.411; 0.556 / 0.544 → 0.478 / 0.500 on Startup) because the Runner across from it is this stage's, not because the Corp moved: its balanced legs against the reference are byte-identical to Stage 6's.
- **Reach**: Casual's never-used list 42 → 38 (Bigger Picture, Docklands Pass, GAMEDRAGON™ Pro, Gourmand, IP Enforcement and Public Trail newly played; Byte! and Carnivore fell off); Startup's 44 → 38 (Botulus, Carnivore, Gourmand, Knickknack O'Brian, Maglectric Rapid, Scrounge and Verbal Plasticity newly played; Byte! fell off).

**Rejected.** The old Runner profiles as the planner's dials (above: 0.03 and 0.07 of the chair). The last click and the stakes at any weight tried (above). A per-run stake (byte-identical). A last-click fear that fires later in the run — it opened the jack-out door, since the term would have vanished on leaving; read at initiation it decides the run and nothing inside it. An HQ estimate that attributed every unaccounted agenda to the hand (it was every agenda the Corp had installed face down, and the Runner ran HQ for the cards on the table). Pricing the run-money events (Account Siphon, Transfer of Wealth) — their gain is at the breach, past the leaf the planner prices, and a reading of an access replacement is a later stage's if the report says they are held. A Runner plan read off the Corp's faction (the Runner reads the Corp's identity for what it fears, not for what it plays).

**Cost** (release, one thread, a 96-game Casual pass): the planner as Corp 170 s (Stage 6 169 s), as Runner **213 s** (183 s), on both chairs 298 s (249 s), the reference 36 s (46.5 s in Stage 6's run, 37 s in Stage 5's — the machine's noise, so the ratios are this run's: 4.7× / 5.9× / 8.3×). The Runner's pass is a sixth slower for two reasons the report shows: it plays more of the game (runs a game 17.8 → 19.4, trashes 1.7 → 2.6, and games run longer with fewer flatlines) and every evaluation now reads the Corp's identity, the ICE it has shown and the traps it has seen. The slowest decision seen is **0.26 s** against Stage 6's 0.34 (`planner::cost::planner_cost`, six games at 0.3–5.6 s). The 256-seed sweeps, with the planner seated, run 113 s (index) and 158 s (view) in release, against 116 s and 161 s at Stage 6, green. `cargo test --workspace` green, clippy silent; the engine's random seatings identical to `main` in both `coverage_identical.py` shapes (`468aa0cd…`), and the heuristic seatings moved (`45b28fb8…` → `9ef36cb6…`) on the three Shaper decks alone, whose reference profile went from cautious or wary to balanced — the one place the reference moved, recorded above.

#### Stage 8 — The handover: every rung on the planner, both ladders re-taken, the reference deleted — DONE (`feat/ladders-on-the-planner`, 30 September 2026)

**Every rung is the turn planner, and the one-ply reference is gone.** `Level::spec` seats `PlanningAgent` at five handicaps on each chair (`LevelKind::Planner`); `HeuristicAgent`, the fixed reference every stage from 4 to 7 was measured against, is deleted, once the planner had beaten it on both chairs (Stage 6's Corp legs +0.122 / +0.073, Stage 7's Runner legs +0.073 / +0.109). What it did survives as the planner's one-ply choice (`planner::one_ply`, the same function it always played inside a run and a prompt), and the word `heuristic` is gone as a bot kind everywhere it named one: `--corp`/`--runner` default to `planner`, `bench --bots` to `random,planner`, the daemon's `--bot-runner` to `planner` ("planner bot" in the match list), the gym's opponent to `"planner"`, a record kept by kind to `bot:planner`, `IndexedHeuristicAgent` to `IndexedPlanningAgent`, `coverage_identical.py`'s two heuristic shapes to `planner-view` and `planner-index`. The two Runner profiles kept for the reference (`Pressure` as `Aggressive`'s numbers, `Rig` as `Builder`'s) go with it: no Runner plan has a profile, and `Style::planned_weights` starts from `Style::weights()` on both chairs — the first plan's profile for a Corp, the balanced dials for a Runner. The planner did not move by a game (below). `describe()` names the plans a rung plays: "plans its whole turn as glacier then fast-advance, and throws away about one decision in 20"; a balanced Runner rung plays its identity's faction's plan, which no spec knows before the deck is dealt, so it names none.

**The reference's position tests are the planner's now** (`planner::positions`, ported from `heuristic::tests`), and three of them changed what they pin, each a decision the planner makes differently on purpose: a ready agenda is scored *this turn* rather than on the first click, because the score is free and the lines tie to the jitter; a breaker in grip is saved for once a piece of the ICE it breaks is on the table, Stage 7's rule, where the reference saved for Cleaver against no ICE at all; and the rush-against-glacier contrast holds with one piece of ICE in front of the agenda, not two (below). The Dividends test found the planner banking the counter before the score at once — the decision a 512-iteration search once took to find and one ply never could.

**Measured: the full 5 × 5 square per chair, 384 games a cell on each of two seeds, every seat in its deck's own style, against the un-handicapped planner on the other chair** (`bench --bots level:novice,…,level:elite --deck-styles`, `ladder_report.py --reference level:elite`; the pinned binary before the Runner's re-spacing, then after it). The Corp's table stands: §21's handicaps, read off `glacier`'s one-ply curve, space the planner the same way.

| Corp rung (ε) | seed 1 | seed 2 | step, seed 1 / 2 |
|---|---|---|---|
| `novice` (1.00) | 0.018 | 0.008 | |
| `apprentice` (0.22) | 0.143 | 0.122 | +0.125 / +0.115 |
| `operator` (0.11) | 0.224 | 0.224 | +0.081 / +0.102 |
| `veteran` (0.05) | 0.315 | 0.328 | +0.091 / +0.104 |
| `elite` (0.00) | **0.401** | **0.469** | +0.086 / +0.141 |

**The Runner's was re-spaced, because its curve is no longer a line.** One ply's Runner win rate fell linearly in `epsilon` (§2: w ≈ 0.833 − 0.70ε), so its rungs sat at even handicaps, 0.75 / 0.50 / 0.25. On that table the planner scored 0.034 / 0.063 / 0.141 / 0.310 / 0.599 (seed 1) and 0.039 / 0.089 / 0.180 / 0.328 / 0.531 (seed 2): steps of +0.029, +0.078, +0.169, +0.289 and +0.049, +0.091, +0.148, +0.203, crammed at the bottom, the first of them flat (+0.029, sd 0.015). A plan a random action breaks is planned again from the board it left, so a blunder costs the planner more than it cost a chooser that never looked past one action, and the curve is steep near zero like the Corp's. Interpolating each seed's measured curve for four even steps gives ε ≈ 0.45 / 0.25 / 0.10 (seed 1) and 0.55 / 0.32 / 0.15 (seed 2); the first was taken and re-measured on both seeds:

| Runner rung (ε) | seed 1 | seed 2 | step, seed 1 / 2 |
|---|---|---|---|
| `novice` (1.00) | 0.034 | 0.039 | |
| `apprentice` (0.75 → **0.45**) | 0.062 → **0.188** | 0.089 → **0.188** | +0.154 / +0.148 |
| `operator` (0.50 → **0.25**) | 0.141 → **0.344** | 0.180 → **0.320** | +0.156 / +0.133 |
| `veteran` (0.25 → **0.10**) | 0.310 → **0.435** | 0.328 → **0.424** | +0.091 / +0.104 |
| `elite` (0.00) | **0.599** | **0.531** | +0.164 / +0.107 |

Every step a rise on each seed at 2.6 sd or more, all within 0.05 of even (0.141 and 0.123); the smallest, `operator → veteran`, is +0.091 and +0.104. The apparatus check is §2's: **exactly the ten cells whose Runner is `novice` or `elite` are byte-identical between the two runs** on each seed — to the game — and the fifteen whose Runner's `epsilon` moved are the ones that moved, which also says the Runner profiles' deletion moved no planner game. The two chairs' tops are 0.40 / 0.47 (Corp) and 0.60 / 0.53 (Runner) against each other, which is the pool's Corp share under the planner (Stage 7's precepts: 0.411 / 0.411 Casual). §4 (a), the Runner ladder owed a re-spacing since §23, closes here; so does §22's question whether `glacier` was exploiting the one-ply Runner's ICE play, moot now that no rung seats it.

**The planner is unmoved, and the checks say so.** `coverage_identical.py main --head-worktree`: all four shapes identical (random `468aa0cd…`, planner `6400f59d…`, view and index) — the engine untouched, and the planner playing the same 192 games as `main`'s, since the profiles it lost never reached it. `diag precepts` on both formats, seed 1, byte-identical to Stage 7's `planner7-*` reports. So the precept lines are Stage 7's, and this stage's signature is the ladder above.

**Found, and recorded rather than fixed** (the Open list): porting the rush-against-glacier test showed the glacier planner, with three clicks, 5[c] and an unadvanced 3/2 behind *two* pieces of ICE, playing *advance, install ICE, advance* and scoring next turn, where fast advance plays *advance, advance, advance, score* and wins the point now. Not the beam's width (`PLAN_BEAM` 12 tried) and not the budget (306 of 2,500 applications spent; 20,000 tried): `prune` keeps the best line under each first action, and after glacier's first advance its best-scoring second step is the ICE, so *advance, advance* is dropped one step before the free score. Behind one piece the two plans differ as the test meant (glacier ices first, fast advance advances first), which is where the test now stands; a free action after the last click, walked through before a line is judged, is a planner item for a later session.

**Rejected.** Keeping the reference as a `LevelKind` nobody seats, for measurement: a bot change is measured from here as both chairs' win share on pinned binaries, the same games paired by seed — what `corp_share_curve.py` and the precepts report already do — and a chooser kept for nobody would drift from every term added after it. Keeping the Runner profiles' numbers for the searches: `mcts` and `puct` never seated a styled Runner in any measurement, and the numbers were the reference's (§16, §17). Moving the Runner's `veteran` to ε 0.08 for a more even step: its step is a rise at 2.6 and 3.0 sd on the two seeds, and 0.05 of drift was §2's own bar for leaving a table alone.

**Cost.** A seed of the 5 × 5 at 384 games a cell is 31–37 minutes on 20 threads (1,866 s, 2,191 s, 2,108 s, 1,901 s), about what §2's one-ply calibration cost, because the handicapped rungs are cheap and the whole square is a fifth un-handicapped games. The 256-seed sweeps, taken while the calibration ran, 154 s (index) and 219 s (view) in release, green; `cargo test --workspace` 1,092 s under the same load, green; clippy silent; CI green.

## 26. The beam keeps one line per position, and a line is judged by the free score it leads to — DONE (`feat/beam-one-line-per-position`, 30 September 2026)

**The finding it closes** (§25 Stage 8's Open entry): a glacier Corp with three clicks, 5[c] and an unadvanced 3/2 behind *two* pieces of ICE played *advance, install ICE, advance* and scored next turn, where fast advance played *advance, advance, advance, score* and won the point now. Stage 8 read it as `prune` keeping the best line under each first action only, and named a free action walked through before a line is judged as the fix.

**What the beam was actually doing.** The position printed ply by ply: at the second ply glacier's six beam slots held *advance-then-ICE* beside *ICE-then-advance*, *ICE-then-credit* beside *credit-then-ICE*, and *ICE-on-the-new-remote-then-advance* beside its reverse — three positions in six slots — and *advance, advance* (35.70) missed the last slot (35.80) by a tenth of a point. The first-action rule kept *advance* only as *advance, ICE*. So the record's diagnosis was right about the rule and wrong about the remedy: **the free score is a ply too late to help**, because *advance, advance* is dropped before any free action exists. Two orders of the same clicks reach one state and one future, and a beam that holds both has paid twice for one position.

**Two changes, measured one at a time.**

1. **One line per position** (`planner::prune`). A line that reaches the state a kept line has already reached is dropped, its first action counted as kept — the twin stands for it, and a weaker line that merely begins the same way would be no addition — and the slot goes to a line that reaches somewhere else. Two states are compared only when their scores are within the jitter (the evaluator is a function of the state), so the comparison costs nothing measurable. **The engine had to agree that the two orders are one state:** `GameState` derives `PartialEq`, and the turn log's same-action table (`TurnLog::same_actions`, CR 5.2.5a's count) was a fixed array in insertion order, so *advance, ICE* and *ICE, advance* compared unequal on a table that holds the same counts. It is kept sorted now (`SameAction` derives `Ord`); nothing reads its order, the random seatings are byte-identical, and a multiset that compared by order was a defect of its own. On the position this alone finds the line: *advance, advance* is third at the second ply, *advance × 3* fourth at the third, and the score follows.
2. **A line still being built is judged by the free score it leads to** (`Search::judged`). An open line's score for the beam is the better of where it stands and where a `ScoreAgenda` from there would leave it, tried on each installed agenda with a token on (the engine refuses the rest for nothing); the score is still a step of its own at the next ply, and a finished line is scored where it ends. Alone it does not find the recorded line — the two-ICE test fails with the dedup gated off, as reasoned above — and over the dedup it lifts *advance × 3* from fourth to the top of the third ply (64.6 against 38.1), which is what a beam crowded with real choices needs. The Runner is judged where it stands: the pool's [click]-less abilities are a breaker's inside a run, an interrupt, or a trash-self, and nothing of the kind waits after its last click.

**Measured** the way §25 Stage 8 said a bot change is measured from here: the planner in both chairs on pinned binaries (`bench --bots planner --deck-styles --games 384`, seeds 1 and 2), the same games paired by (matchup, seed), McNemar's z over the discordant games; and `diag precepts` on both formats and seeds.

| leg (both chairs changed) | seed 1, Corp share | seed 2, Corp share |
|---|---|---|
| `main` (`5903ab6`) | 0.427 | 0.438 |
| one line per position alone | 0.432 (+0.005, z +0.16, 148 discordant) | 0.404 (−0.034, z −1.13, 133) |
| both | 0.461 (+0.034, z +1.10, 139) | 0.469 (+0.031, z +1.03, 136) |
| the judgment over the dedup — **a Corp-only change, so a Corp-chair reading** | +0.029 (z +1.27, 75 discordant) | +0.065 (z +3.20, 61) |

The self-pairing cannot separate the chairs for the first change, which touches both: its reading is neutral on the sum. The judgment touches the Corp alone, and its reading on the same games is a Corp gain on both seeds. A leg of 384 games costs what it did (151 / 171 s on `main`, 153 / 151 s with the dedup, 146 / 141 s with both, on 20 threads): the dedup's comparison and the score tries cost nothing the clock can see.

**Against a fixed opponent, one chair at a time.** The search seats read the evaluator and never the beam, so `mcts@32` is an opponent this change cannot move (a 24-game mcts-vs-mcts leg is byte-identical between the two binaries), and each chair was seated against it at 192 games (`bench --bots planner,mcts --pairing planner/mcts` and `mcts/planner`, seeds 1 and 2). The reading is at the ceiling — the planner wins about 0.84 of its Corp games and 0.95 of its Runner games against a 32-simulation search — and moves nothing: the Corp chair 0.839 → 0.849 (+0.010, z +0.35) and 0.844 → 0.849 (+0.005, z +0.19); the Runner chair 0.953 → 0.938 (−0.016, z −0.69) on seed 1. **Seed 2's Runner-chair leg could not be paired:** `main`'s binary aborts with a stack overflow in it, deterministically, the 180th game of `bench --bots planner,mcts --pairing mcts/planner --games 180 --seed 2` on `5903ab6` — the search Corp on `quick_and_dirty` against the planner Runner on `tickets_please`, seed 565 — where the 179-game prefix completes — a crash on `main` that this change's binary does not reach on the same schedule because its Runner plays differently, recorded under "known to be broken" in `ROADMAP.md` and not this item's. *(Corrected in §27: the game is index 538 — seed 540, `peculiarity` against `stolen_goods`, the 179th of that block, not its 180th; `quick_and_dirty` against `tickets_please` at seed 565 is index 563 and plays through. And the crash was neither the search's nor, alone, the engine's: a sample named a masked access wrongly, and the engine's score recursed on what it named.)* So a fixed opponent weak enough to be fixed says nothing beyond the ceiling, and the self-pairing and the precepts are the measurement, as Stage 8 said they would be.

**Apparatus.** The two changes were measured from one binary with each gated behind an environment variable; the shipped binary, with the gates removed, plays the seed-1 self-pairing byte-identically to it (384 games), and so does the binary rebuilt after the turn log's sort was made to put its entries before its empty slots (the serde round trip had caught the first order). `coverage_identical.py main --head-worktree`: both random shapes identical to `main` (`468aa0cd…`, the engine's one behaviour change being an order nothing reads), both planner shapes moved, as a beam change must.

**The precepts report** (planner both chairs, deck styles, `planner7-*` as the before — Stage 8's reports were byte-identical to them — and `planner26-*` after; casual 192 games, startup 90; seed 1 / seed 2):

| line | casual before → after | startup before → after |
|---|---|---|
| Corp win share | 0.411 / 0.411 → 0.464 / 0.448 | 0.478 / 0.500 → 0.478 / 0.544 |
| scores a game (`corp.04.scores_per_game`) | 1.568 / 1.469 → 1.615 / 1.693 | 1.511 / 1.411 → 1.611 / 1.656 |
| scores in the turn after an unadvanced install, a game | 0.760 / 0.771 → 0.865 / 0.880 | 0.811 / 0.711 → 0.778 / 0.900 |
| scores in the turn of the install, a game | 0.036 / 0.026 → 0.052 / 0.042 | 0.011 / 0.000 → 0.011 / 0.000 |
| advance clicks a game | 7.05 / 6.47 → 7.19 / 7.44 | 7.61 / 6.82 → 7.83 / 8.01 |
| middle-stage advance clicks a turn | 0.672 / 0.647 → 0.745 / 0.714 | 0.724 / 0.630 → 0.678 / 0.734 |
| steals a game | 2.630 / 2.641 → 2.505 / 2.505 | — |

The Corp scores more and advances more where the guide says the scoring happens, and the Runner steals less; nothing on the Runner's own lines moved beyond the band on both seeds the same way. The reach list moved by a handful of cards either way, as any re-roll does.

**The tests that changed, and why.** The two-ICE position is the test now (`behind_two_pieces_of_ice_every_corp_plan_scores_the_point_its_third_advance_wins`: glacier, fast advance and traps each advance three times, score, and follow one plan whole), and porting the finding exposed four pins that were never what they said:

- *A fast-advance Corp advances where a glacier Corp installs ICE* pinned a first click on a three-click turn. Once the beam could see the free score, both plans won the point this turn — the play for any plan — and on every variant tried with the point out of reach (two clicks; a 4/2; a 5/2; the agenda naked or behind one piece) the four Corp plans ended the turn in the same place. Its old pin was the jitter between two orders of one line (*advance, ICE, advance* and *ICE, advance, advance* both 38.1). It is now `every_corp_plan_finishes_the_fort_around_a_point_it_cannot_win_this_turn`, judged by where a two-click turn ends: one token on and the second piece in front, for every plan — §23's fort terms.
- *Draws with an empty grip instead of running an open server* and *draws at the floor when the stack holds a breaker and not when it holds junk* left the Corp's R&D empty, so the Corp decked out at its next turn start and every turn-ending line scored as a won game (1000.0008 against 1000.0008): the draw-or-credit pins were the jitter's since the tests were ported in Stage 8. The first has an R&D now; the second has a rezzed "End the run" barrier on each central, so nothing is worth running and the choice is the one the test names — and it pins as it always claimed to.
- *Installs an agenda behind ICE rather than into a naked remote* is judged over the turn: the planner may draw first (its sample puts one of the registry's three cards on top of R&D, and a wall there is worth the draw), and which click the install falls on is the jitter's once two orders are one line; where the agenda goes is not.

**Rejected.** Walking every free action through by probing each open node's legal list: the probe applies every candidate, a dozen a node over some two hundred nodes a ply, several times `PLAN_BUDGET`. Walking the score through only at zero clicks ("after the last click", as the finding was phrased): the third advance is the last click here, but a free score two clicks in is the same blindness. Widening the beam: twelve was tried in Stage 8 and holds the same twins twice over. A tie-break among twins for the order a person would play (the draw first, for the information): the order within a turn moves nothing in the sample, the plan is re-planned when the real draw differs, and a rule with no measurement behind it is the kind §6–§8 spent a week on.

**Verified.** `cargo test --workspace` green (1,164 s under the measurement's load), clippy silent, both 256-seed sweeps green in release (135 s and 152 s of tests under load), CI green.

## 27. A score never asks a card's other text whether it is on, and a sample names a masked access as a card it holds — DONE (`fix/stolen-non-agenda-score-recursion`, 30 September 2026)

**The crash it closes** (§26's line under "known to be broken"): `bench --bots planner,mcts --pairing mcts/planner --games 192 --seed 2` on `5903ab6` aborts with a stack overflow, deterministically, and with a 1 GiB thread stack just as with the default — an unbounded recursion, not a deep one. `gdb` showed the cycle: `win::score` → `continuous::sum` → `for_each_applying` → `check_requirement` → `resolve_amount` (`Amount::ThreatLevel`) → `win::score`, entered from `resolve_steal` inside an `mcts` rollout.

**Which game, corrected.** §26 named `quick_and_dirty` against `tickets_please` at seed 565; that is index 563, and it plays through. The crashing game is **index 538, seed 540, `peculiarity` (PT Untaian) against `stolen_goods`**: the sampled state's rig (Carmen, two Smartware Distributors) and heap (Buzzsaw) are only `stolen_goods`'s, and the one masked mandatory steal logged in a patched run was PT Untaian against Zahya on turn 14. "`--games 180` aborts and `--games 179` completes" stands — block 360–539 holds index 538 and 358–536 does not — but it is that block's 179th game, not its 180th. §26 is annotated.

**What happened, in order.**
1. A Jailbreak breach of HQ stole Send a Message, whose "you may rez 1 installed piece of ice" parked a decision on the Corp, and the breach presented the next card of HQ — the second Send a Message — as a steal the Runner must make. The Corp, `mcts@32`, was asked about the rez with the breach standing there.
2. The Corp's view does not name a card accessed out of HQ or R&D (`masking::mask_run_state`: the Corp learns it when it lands somewhere public) and does carry the decision's public half, `mandatory_steal: true`. `determinize_access_phase` drew the name from the pool, `Slot::CorpAny`. For the Corp the pool is its own deck, every card of which was already dealt into the sample's R&D, so the draw fell through to the prior and named any Corp card of the format: **Boto**, with `mandatory_steal` copied beside it. A patched run of the same schedule logged 63 masked-access samples before the crash, among them Hedge Fund and Corporate Hospitality beside a trash cost of 2 that neither prints, so the rollout's Runner could trash an operation out of HQ.
3. The rollout stole it. `resolve_steal` trusts the parked decision; `remove_from_corp_zone` found no Boto in HQ and removed nothing; the ice entered the Runner's score area as a plain `ScoredAgenda`.
4. `agenda_value_in` asked the scan for Boto's agenda points, and the scan asked Boto's own text — "Threat 4 → this ice gets +2 strength" — **whether its `while` held before learning it was a strength**. The threat level is the greater score, a score asks every scored card its worth, and Boto was asked again.

**The fix, both halves.** Either alone stops this crash; each fixes something that was wrong without the other.
- **Engine: the kind is asked first.** `continuous::for_each_applying` takes `asked`, the kinds the question is about, and checks it before the scope, the first-each-turn gate and the `while` — the order the module doc already claimed ("is it the kind being asked about, is the target one it applies to, is it on"). Every caller states its kind. `check_requirement` has no side effects, so skipping it for other kinds changes no answer (the random coverage shapes below are identical). `CardDefinition::validate` refuses an `AgendaPoints` effect whose number or `while` reads `ThreatLevel` (`EffectRequirement::reads_the_score`) — the one cycle left, and no card prints it.
- **Bots: a masked access is named as a card the sample holds.** `determinize` names it once the zones exist (`name_the_accessed_card`, as `from_zone` is): the install the access pinned, else a card of the sample's HQ or R&D the public decision is true of (`fits_the_access`: an agenda exactly when a steal is offered, and the printed trash cost, which is exact for a card in no root; an interactive trigger at its own cost), else any Corp card of the registry the decision is true of (sorted, for a seat whose zone came from the prior), and only a registry with none keeps the pool draw.

**Rejected.** A depth guard on `win::score`, or `resolve_steal` refusing a non-agenda: each hides the sample's inconsistency and leaves the search valuing states no game reaches, and the scan's order was the bug. Unmasking the card to the Corp: the mask is right. A pool draw constrained to fit: for the Corp the pool is empty by then, and the real card is in the sample's own zone.

**Measured.** `5903ab6` with this change applied plays the crashing schedule through: 192 games, `mcts@32` Corp 12 – planner Runner 180, no stalls. `coverage_identical.py main --head-worktree`: both random shapes identical to `main` (`468aa0cd…`); both planner shapes moved, as a change to the samples must. Planner self-paired on pinned binaries, `--deck-styles`, 384 games paired by index: Corp share 0.461 → 0.461 (seed 1, 382 of 384 games identical in winner and length) and 0.469 → 0.469 (seed 2, 383 of 384), no discordant pair on either — the samples it corrects are rare, and correcting them costs the planner nothing.

**Verified.** The engine test (`a_score_never_asks_an_effect_that_is_not_about_agenda_points_whether_it_is_on`) overflows the stack without the fix; the sample test (`a_masked_access_is_named_as_a_card_of_the_samples_own_zone_that_the_decision_is_true_of`) fails on seed 0 with the pool draw (it named Valentão for a card with a trash cost of 4). `cargo test --workspace` green (2,404 tests), clippy silent, both 256-seed sweeps green in release.

## 28. A break is priced by what the card holds: a cost in counters or hosted copies is read as the engine charges it and bounded by the stock, so the planner hosts a copy of Matryoshka — DONE (`feat/breaks-priced-by-what-the-card-holds`, 2 October 2026)

**The blindness it closes** (Parhelion Stage 7's record, `archive/nsg-card-pool.md`): in Hit List, random seats hosted a copy of Matryoshka 28 times in 288 games and the planner never did in 144 (`prompts_offered` 0), so its Matryoshka broke nothing. The planner *did* enumerate the host — `expand` takes every legal transition and `prune` keeps the best line under every first action — and walked the one-card prompt as forced steps; it was the evaluator that gave a hosted copy no value, in two readings of `eval::read` that disagreed with each other. `covers(def)` read `BreakSubroutines { restrict_to: None }` as "breaks every subtype" with or without a copy hosted, so an installed Matryoshka already earned the full coverage term and hosting added nothing; and `break_cost` priced only `Cost::Credits` and skipped every other cost with `Some(_) => continue`, so Matryoshka's `AllOf[CreditsX, TurnHostedFacedown]` was never priced, `cheapest_break_cost` was `None`, every rezzed piece read unbreakable to the run term and `access_prospect` was gated off — and again hosting changed nothing. Nothing in `netrunner_bots` read `hosted_cards`, `faceup_hosted()` or `turned_facedown` but `determinize`, which copied them into the sample. The host line was a click for nothing, and a credit click won.

**The same unpriced shape covered four pool breakers and a pump**, read off every break and pump ability whose cost is not plain `Credits`: Matryoshka (`AllOf[CreditsX, TurnHostedFacedown]`), Lobisomem (`AllOf[CreditsX, RemoveCounters(1)]`), Audrey v2 (`RemoveCounters(1)`, an AI breaking two a counter), Tremolo (`CreditsAmount(Reduced{3, RunnerInstalls(cybernetic)})`) and Hantu's pump (`RemoveCounters(1)` for +2). Botulus and Poison Vial print `BreakSubroutinesUnconditionally`, which neither `covers` nor `break_cost` ever read, and Botulus's `EncounteringHostIce` requirement would have to be read with it or it prices every piece on the table; both stay as they were. Audrey v2's pump pays in a grip card and stays unpriced, so a shortfall against it reads unbreakable.

**What changed, all of it in the evaluator** (`eval::read`; the engine, the view, `ActionSpace` and `OBS_SIZE` do not move):
- **`price_of`**: a printed cost read the way the engine charges it, for a break of `pending` subroutines. `Credits(c)` → `c`; `CreditsX` → the X is chosen to cover them all, so one activation costs `pending`; `CreditsAmount` → `amount_on_table`, the engine's own reader (Tremolo's reduction); `RemoveCounters(n)` → free, as many times as the counters cover; `TurnHostedFacedown` → free, as many times as the card has a copy **still faceup** — `faceup_hosted`, never every hosted copy, because a copy turned facedown on the turn's first run is not back until the Runner's next turn begins; `AllOf` → the credits summed, the tightest stock. Anything else is still unpriced, which skips the ability as it always has.
- **`break_cost` prices every ability through it**, break and pump alike, and an ability whose stock does not cover the activations it would take is not a break the card has. `breaks_subtype` — and so `strength_shortfall`, the pump term — asks the same question (`stocked`): a Matryoshka with every copy facedown matches nothing, so there is nothing to pump.
- **The stock is budgeted across a server** (`read::Stock`, one count per rig position, the shape `taxing_cost`'s rez budget already had): `cheapest_break_cost` prices each piece with what the earlier pieces left and adds the chosen card's spend to the ledger, so one hosted copy no longer prices a two-ICE server as breakable twice over — and a second copy is what turns that server from unbreakable to breakable, which is the reason to host it. `remaining_break_cost`, `server_break_cost` and `taxing_cost` each walk with a fresh ledger; `is_unrezzed_threat` prices one piece on its own. Greedy, piece by piece in the order the run meets them; a server of two or three pieces needs no better. Ties go to the card that spends the least stock, so a plain credit breaker is used ahead of a counter it could save.
- **`rig_coverage` and `breaker_coverage` keep the printed reading**, on purpose. Making the rig term read stock while `install_delta` credits the grip card its printed three subtypes would make installing Matryoshka with no second copy in grip a loss — the held-Cleaver-worth-more-than-installed trap `install_delta`'s doc records — and `rig_coverage` also feeds the Corp's `corp_install_value` and `advancement_upside`, which would move Corp scores for a Runner card's sake. `covers(def)` is untouched in the observation and in `diag precepts`' `is_breaker`.

**What that buys, and what it does not.** Off-run no Runner term reads the break cost, and a hosted copy has no standing value of its own, so the host click is paid for only when the planned line reaches a run leaf in the same turn: "host, run" beats "run" because the piece turns breakable, and a host on the last click with no run never scores. At the balanced rate a central's one hidden access (0.6) does not buy the host click and the run click (0.8), so the planner hosts ahead of a run worth both — a remote with an advanced card in it, or a central under the pressure plan's HQ term (Hit List's style). The test `hosts_a_copy_of_matryoshka_before_running` pins that position: every server behind a rezzed piece and a remote with two tokens on a face-down card, the Runner hosts and runs the remote. A standing term for stock on the rig is the follow-up if the per-card counts say this is too seldom; it would be measured on its own.

**Measured.**
- `coverage_identical.py main --head-worktree`: both random shapes identical to `main` (`57e361e0…`); both planner shapes moved, as an evaluator change must.
- Planner self-paired on pinned binaries (`bench --bots planner --deck-styles --games 384`, seeds 1 and 2, the same games paired by index, McNemar's z over the discordant games): Corp share **0.453 → 0.445** (seed 1, −0.008, z −1.73, 3 discordant, 376 of 384 games identical in winner and length) and **0.445 → 0.435** (seed 2, −0.010, z −2.00, 4 discordant, 375 identical). Every discordant game is Bowel Movements (René "Loup" Arcemont: Hantu and Botulus) winning where it lost — the one sample deck whose breaker this reading changes, Hantu's pump now bounded by its counters and priced where it was skipped — and the sample pass holds no other card of the five. Under the seed-spread band, in one direction on both seeds.
- `diag precepts --deck-styles` (planner both chairs; casual 192, startup 90; seeds 1 and 2): no derived ratio moved past noise; Corp share 0.453 / 0.448 → 0.453 / 0.438 casual, 0.478 / 0.544 → 0.478 / 0.533 startup; reach unchanged (Hantu used 35 → 43 on casual seed 2).
- The cards themselves, seed 2, planner both chairs over 48 games a pairing, `prompts_offered` the hosts and `activated` every ability of the card; random 96 games beside:

  | pairing | card | planner before | planner after | random |
  |---|---|---|---|---|
  | Permafrost vs Hit List | Matryoshka hosts / activations | 0 / 193 | **8** / 50 | 8 / 48 |
  | Hostile Bid vs Hit List | Matryoshka hosts / activations | 0 / 87 | **3** / 75 | 11 / 31 |
  | Undertow vs Hit List | Matryoshka hosts / activations | 0 / 91 | 0 / 2 | 8 / 35 |
  | the three above | Tremolo activations | 8 / 11 / 1 | 18 / 28 / 20 | 1 / 12 / 11 |
  | Undertow vs Grassroots | Audrey v2 activations | 94 | 17 | 15 |
  | Undertow vs Safety Net / Street Gallery | Lobisomem activations | 6 / 12 | 9 / 30 | 2 / 0 |

  The planner hosts 11 copies over the 144 games where it hosted none, at random's rate against the two trap decks and not at all against the glacier one, whose remotes the run term does not reach at two clicks. Matryoshka's activations *fall* from 371 to 127 and Audrey v2's from 94 to 17: those were pumps bought inside encounters the card could never break — `strength_shortfall` read a stockless breaker as a breaker short on strength — and a pump against nothing is no longer offered a term. Tremolo's and Lobisomem's rise is their breaks, priced for the first time. `subroutines_broken` is the ICE's count, not the breaker's, so it is not quoted.

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release. `diag precepts`' `remote_break_cost_at_turn_start` now reads `None` for an empty Matryoshka rig where it priced before, noted at the counter.

## 29. The cards the planner never plays are the difference between seatings, measured — DONE (`diag/blind-cards`, 2 October 2026)

**Why a second instrument.** `diag precepts` ends with every card a seat used over a pass and the cards of the decks played that no seat used (`reach.unused_in_pass`, §25 Stage 1), and `nsg-card-pool.md`'s "Bot debts" has pointed at it since. That reading is "no seat used it", for one seating, and §28 showed what it cannot see: an installed Matryoshka counted as *used* while the planner never hosted a copy on it, so the planner's blindness to the card was invisible to the list meant to name it. The person's brief for §28 — "there are probably many more cards that also need to be trained" — is a question that reading cannot answer either, because a card no seat uses may be one the decks rarely draw.

**The instrument.** `scripts/blind_cards.py random.json planner.json` takes two `diag precepts --deck-styles --report` passes on the same games — random seats first, planner seats second — and prints, per side, every card the random seats used at least `--min` (5) times that the planner used at most `--ratio` (0: never) as often, with both counts; and the cards only the planner uses, for completeness. The random seat's count is what makes the list a judgment: a card random reaches forty times is one the engine offers in ordinary play, so the planner's zero is the evaluator's doing and not the decks'. `--coverage` runs the same diff over two `--headless --report` coverage files and adds a second table over `prompts_offered` alone — reach counts a card as used when it is installed, prompts count what its *text* was asked to do, which is the reading for a card whose use is an ability (Matryoshka's host, Madani's). A card's side is read off the deck files, since neither report says. Same shape as `paired_bench.py`: a docstring, argparse, no dependencies.

**Measured, on `main` at #341** (casual 192 games and startup 90, seeds 1 and 2, planner both chairs against random both chairs): **26 cards** random seats use at least five times a pass that the planner never does, 20 Runner and 6 Corp. Madani leads by a distance (115 / 163 casual, 71 / 77 startup) — the other host-then-use card, in two sample decks, and Phase 5's next. Then, Runner: Pennyshaver, Topan "Ormas" Leader's install, Overclock, Red Team, Tread Lightly, Conduit, Shred, Clean Getaway, Maintenance Access, Docklands Pass, Carnivore, GAMEDRAGON™ Pro, Tranquilizer, Maglectric Rapid 748 Mod, Cacophony, Verbal Plasticity, Cookbook, Botulus, Détente; Corp: Byte!, Public Trail, Leo Construction Labor Solutions, Synapse Global's ability, Bigger Picture, Neurospike. The full table with each pass's count is under "Bot debts" in `nsg-card-pool.md`, which now names the command in place of the hand list's preamble. The hand list's entries are kept below it for the reasons they record; Byte! and Red Team are on both. On the §28 Hit List games the `--coverage` mode names Poison Vial, Word on the Street, Conduit, Kompromat, Info Bounty, WAKE Implant and Red Team on the Runner's side and Retribution's prompt on the Corp's.

**Rejected.** Growing `diag precepts` with a second seating of its own: the report is one seating by design (its groups are the seating's styles and factions), and two JSONs it already writes are the honest input. Splitting `activated` by ability index in the coverage report: a per-ability count would say which ability, but the host is the only prompt Matryoshka's and Madani's text opens, so `prompts_offered` already says it for the cards in question; the split is owed when a card's two abilities both matter.

**Verified.** The script runs on the four §29 pairs and on the §28 coverage pairs; no Rust changed, so no sweep or coverage comparison applies.

## 30. A program hosted on Madani is one turn from the table, and the grip promises its installer half a click: the planner installs Madani, hosts on it and installs from it — DONE (`feat/madani-hosts-the-rig`, 2 October 2026)

**The card §29's list put first.** Madani — "[click]: host any number of programs from your grip faceup on this hardware", "once per turn → 0[credit]: install 1 hosted program" — was the card random seats used most that the planner never touched (115 / 163 casual, 71 / 77 startup, planner 0), in two sample decks. Nothing in the evaluator read a hosted program: `held_cards_value` summed the grip, so Madani's host was a click that made the grip's programs vanish, and an empty Madani was a 2[c] hardware worth its presence and nothing.

**What it is now** (`eval::runner::hosted_installs_value`, read off the DSL — a rig card whose `Paid` ability holds `Effect::InstallRunnerCardFromHost` is an installer, never by name; Matryoshka's hosted copies are a break's stock and are read by nothing here):
- **A hosted program is a program one turn from the table:** its `install_delta` less a click — the turn's wait at the guide's rate — not the grip's half, because its install is free and certain, and not the whole, because it is not on the table yet.
- **A grip program promises its installer half a click:** the install will cost none, once a host click still to be paid has been. Over the programs the Runner would install at all (`install_delta` above zero), capped at the turns the stage expects. That half is what puts Madani on the table: beside programs the Runner cannot yet afford, the click that would have been a credit installs Madani instead (`installs_madani_for_its_promise_and_installs_a_hosted_program_free`).
- Then one strong breaker hosted is worth half its delta over the host click, and the free install that follows is worth the click. A breaker the Runner can afford is still installed outright — a strong card on the table now beats one parked — so Madani's place is the programs that are waiting: for credits, for memory, for the turns to install them one a click.

**Three readings were measured and rejected on the way, each on pinned binaries at 384 games paired by index on two seeds, with the four Madani deck pairings at 48 planner games (random 96):**
1. *Hosted programs at a click each, the grip promising nothing:* Madani installed in 1 of 192 games. An installer with nothing hosted promised nothing, so it never reached the table.
2. *Grip and hosted programs promising a click each:* Madani installed at random's rate (22–33 of 48) and hosted on in 1–10 games against random's 29–48 — a host was a click for nothing once the grip already promised the click — and the self-pairing read a Runner loss: every discordant game a Madani deck, the Corp winning 19 of 28 (seed 1 +0.021, z +1.89; seed 2 +0.010, z +1.26).
3. *Hosted a whole click, grip half a click:* Madani installed in 4–17 of 48 and hosted on in **0**. Hosting two programs was a tie with a credit click and three a fifth of a click, which the grip floor outbid.
Reading 2 was first measured with a beam change beside it — a line parked inside the seat's own card selection judged by its confirm, so that "host both" was not dropped a ply before the confirm that made it the better line — which moved every game (121 and 142 discordant of 384, the two seeds in opposite directions) and made the view path and the index path of `coverage_identical.py` disagree for the first time; it was reverted, the paths agree again, and partial hosting ties with full hosting under the reading that landed, so nothing needed it.

**Measured, the reading that landed.**
- `coverage_identical.py main --head-worktree`: both random shapes identical to `main` (`57e361e0…`); both planner shapes moved, to one hash (`7b88db04…`), as an evaluator change must.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.458 → 0.458** (seed 1, 10 discordant, 5 : 5, 342 of 384 games identical in winner and length) and **0.417 → 0.398** (seed 2, −0.018, z −1.94, 13 discordant, the Runner winning 10 of them, 344 identical). Every discordant game is Professional Opportunities or Enthusiasm, the two sample decks that hold Madani. Under the seed-spread band; the Runner's way where it moves.
- `diag precepts --deck-styles` (planner both chairs; casual 192, startup 90; seeds 1 and 2): no derived ratio moved past noise; Corp share 0.464 / 0.406 → 0.453 / 0.406 casual, 0.478 / 0.533 → 0.444 / 0.533 startup. **`blind_cards.py` no longer lists Madani** (used 99 / 70 casual, 116 / 5 startup); the list is 20 / 22 / 15 / 14 cards, Pennyshaver first.
- The card itself, seed 2, planner both chairs over 48 games a pairing, random 96 beside:

  | pairing | Madani installed | prompts (hosts and free installs) |
  |---|---|---|
  | Brutal Efficiency vs Professional Opportunities | 1 → **8** (random 25) | 0 → **14** (random 39) |
  | Peculiarity vs Professional Opportunities | 0 → **4** (random 21) | 0 → **1** (random 29) |
  | Brutal Efficiency vs Enthusiasm | 0 → **17** (random 36) | 0 → **3** (random 41) |
  | Peculiarity vs Enthusiasm | 0 → **8** (random 36) | 0 → **6** (random 48) |

  Installed in 37 of 192 games where it was installed in 1, hosted on and installed from 24 times where it was never — about half random's rate of installs and a third of its prompts, because the planner installs a breaker it can afford outright and parks only what waits. `activated` (84–248 a pairing) is not quoted: Madani's free install is a 0[c] ability the engine accepts with nothing hosted, which resolves to nothing, and both seatings take it once a turn.

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 31. A run a card's text began is priced with what the text put on it: the planner plays Overclock, Clean Getaway, Shred and Maintenance Access — DONE (`feat/run-riders-at-the-leaf`, 2 October 2026)

**The run cards on §29's list.** Overclock, Clean Getaway, Shred, Maintenance Access and Tread Lightly — every run event in the sample decks, in nine of them — were used by random seats 16–53 times a pass and by the planner never (Jailbreak, which draws a card, was the one run event it played). The planner prices a run as a leaf the moment it starts (§25 Stage 4), and the leaf read the Runner's own credits, the server attacked and the ICE in the way: nothing the card that began the run had put on it. So an Overclock run through ICE the Runner could not otherwise afford read as unbreakable — the card's one reason — Clean Getaway's 6[credit] was a 3[credit] event that did what a click did, Maintenance Access was a run on Archives, Shred's armed prevention was nothing, and each lost to the plain run by its play cost.

**What it is now** — each read off the `RunState` the engine built when the search began the run, so none is a sampled guess (a run in progress when the view was taken carries no rider, `determinize` says why):
- **The run's own credits break ICE** (`read::run_pool`, Overclock's `bonus_run_credits`): counted with the Runner's credits and the bad-publicity pool at the breakability gate, when the card's word lets them pay for a break (`run_credits_pay_for`: none, or `UsingIcebreakers`). **And they are worth the breaks they cover** (`due.min(run credits)` at a credit each): the run is never charged for its breaks (§20 rejected that), so what the run's credits pay instead of the Runner's is the one reading of them — what the Runner keeps — and credits the run leaves unspent are worth nothing, as they are.
- **The rider pays on success** (`read::rider_income` over `on_success_effect`, tallied as a play is: a `Sequence` summed, an `EffectIf` as if it held, the opponent's choice at its worst): Clean Getaway's 6[credit] at a credit each, Red Team's 3[credit], Joy Ride's and Jailbreak's draws at a click each; and the accesses it adds (`rider_accesses`, Jailbreak's "access 1 additional card") are counted with the run's own. Inside the breakability gate, where the access prospect is: no rider pays on a run that cannot get in.
- **The breach is of the server the run approaches** (`access_prospect`, `redirect_on_approach`): Maintenance Access's run on Archives that becomes a run on HQ is the HQ access, through Archives' ICE.
- **A run-ending prevention passes the first unbreakable piece** (`read::run_ending_prevented`, Shred): over a root with a card in it — the engine prevents nothing over an empty root, where X is nothing to pay — the first rezzed piece no rig card can break costs nothing in `remaining_break_cost`, and a second is unbreakable as ever. The Corp either pays X cards from HQ or the run goes on; the reading takes the run as going on.
- **A rez tax is what the forced rez costs** (`forced_rez_credits`, Tread Lightly's `Lingering::RezCost` on each piece): `TYPICAL_REZ_COST` plus the tax a piece, out of what the Corp has. At `FORCED_REZ_WEIGHT` 0.12 that is 0.36 an unrezzed piece against the card's 1[credit], so it pays at two unrezzed pieces in front of a Corp that can pay them — rarely, as measured below, and the honest reading at the guide's rate.

`plays_overclock_to_run_through_ice_it_cannot_otherwise_afford` is the planner line; `a_run_a_card_began_is_priced_with_what_the_card_put_on_it` holds every reading above to its number.

**Measured.**
- `coverage_identical.py main --head-worktree`: both random shapes identical to `main` (`57e361e0…`); both planner shapes moved, to one hash (`7b88db04…` → `aac136ba…`). Over that one planner pass of the pool (192 games), `OnPlay` fired: Overclock 0 → **58**, Clean Getaway 0 → **46**, Shred 0 → **25**, Maintenance Access 0 → **7**, Tread Lightly 0 → **3**, Jailbreak 49 → **99**.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.458 → 0.435** (seed 1, −0.023, z −1.13, 63 discordant, the Runner winning 36 of them) and **0.398 → 0.419** (seed 2, +0.021, z +0.96, 70 discordant, the Corp winning 39). Opposite directions, both under the seed-spread band: no net effect on the self-pairing, and most games moved (173 and 179 of 384 identical in winner and length), because every Runner deck but a few holds a run event.
- `diag precepts --deck-styles` (planner both chairs; casual 192, startup 90; seeds 1 and 2): Corp share 0.453 / 0.406 → 0.427 / 0.432 casual, 0.444 / 0.533 → 0.444 / 0.578 startup — within the band each way. **`blind_cards.py` no longer lists Overclock, Clean Getaway, Shred or Maintenance Access on any pass**, and Tread Lightly on three of four (startup seed 2 keeps it at 24 / 0); the list is 20 / 22 / 15 / 14 → **14 / 14 / 11 / 11** cards, Pennyshaver and Topan's install first.
- The cards themselves, seed 2, planner both chairs over 48 games a pairing, random 96 beside (`played`):

  | pairing | Overclock | Clean Getaway | Maintenance Access | Shred | Tread Lightly | Jailbreak |
  |---|---|---|---|---|---|---|
  | Brutal Efficiency vs Professional Opportunities | 0 → **29** (48) | 0 → **40** (29) | – | – | – | – |
  | Peculiarity vs Shootin' n' Lootin | 0 → **27** (57) | 0 → **39** (32) | 0 → **18** (45) | 0 → **10** (41) | 0 → 0 (45) | 12 → **35** (61) |
  | Brutal Efficiency vs Dashing Mad | 0 → **17** (29) | – | – | 0 → **21** (57) | – | 33 → **42** (61) |
  | Peculiarity vs Tickets Please | – | 0 → **53** (46) | 0 → **13** (37) | – | 0 → 1 (27) | 17 → **43** (50) |

  Clean Getaway is played past random's rate (it is a 6[credit] run), Overclock at half to all of it, Maintenance Access and Shred at a third — the planner plays them where the plain run is not available or not worth it, which is what the readings say. Over the four pairings the Runner won 126 → 116 of 192, the Shootin' n' Lootin pairing against a kill Corp carrying it (28 → 20, flatlines 14 → 20): a Runner that plays its run events runs more and holds fewer credits against the kill, which the self-pairing above does not show across the pool. Recorded, not explained.

**Still on the list, and why.** Red Team and Conduit are run *abilities* on installed cards — the rider and the extra accesses are read here once the run begins, but nothing prices the install, so neither reaches the table; with Pennyshaver and Docklands Pass they are the standing value of a card that pays on a run, the next reading. Tread Lightly's value at the guide's rate is the forced-rez term's and small. Topan "Ormas" Leader's install is the other kind of blindness (an ability, not a run) and is not read here.

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 32. A card that pays on a run is worth what its runs will pay: the planner installs Red Team, Pennyshaver, Docklands Pass and Conduit — DONE (`feat/run-paying-installs`, 2 October 2026)

**The installs §31 left on the list.** Red Team, Pennyshaver, Docklands Pass and Conduit were used by random seats 5–80 times a pass and by the planner never. §31 priced Red Team's and Conduit's runs once they began, but a card is installed for what it will do, and `read::declared_income` — the one reading of what an installed card pays — read none of them: a run's rider and a trigger on a successful run were nobody's income, and Docklands Pass pays at the breach, after the run leaf has priced the run. So each was its printed cost against its presence, and lost to a credit click.

**What it is now:**
- **A click ability that begins a run pays its rider per use** (`Income::click_runs`, `read::run_rider`): Red Team's "take 3[credit] from this resource" is the click's credits, bounded to four uses by the twelve it holds, as Regolith Mining License's are by its fifteen. **The click is not charged**, because it buys the run: the Runner makes more than one run a turn (18.6 in 13.1 turns, the planner's casual pass at §31), and a run this card begins is one it would have made. Charged, Red Team's install was worth 0.1 more than a credit click (a held Red Team is half of its install value), and the beam's other terms decide a tie that close. A run with no rider (Baker, Conduit, Debbie "Downtown" Moreira) pays nothing here.
- **A successful run pays a card's trigger once a turn** (`Income::run_credits`): a trigger on every successful run — no server, no condition, not "this server" — that gains credits, or places counters on a card whose text cashes them (Pennyshaver's credit a run, taken by its click), at one successful run a turn over the horizon (the planner makes 0.81–0.88 a turn). One narrowed to a server (Gabriel Santiago's HQ) or to its own (Stowaway) is counted as nothing, the cheaper direction, as `Income` counts every computed amount.
- **The rig's breach accesses are the run's** (`read::rig_breach_accesses`, in `access_prospect`): a rig card's trigger on a breach or a successful run of the server attacked that adds accesses outright — Docklands Pass's on HQ, read against the turn log for its "first time each turn", as the engine judges it. An access behind a condition (Manuel Lattes de Moura's tag, Pretty Mary da Silva's access limit) or a cost (Rotary's tag, Devadatta Drone's counter, Cupellation's trash) is nothing here.
- **The counters a card's own runs place are R&D accesses promised** (`read::rd_accesses`, under the rig plan's `RD_ACCESS_WEIGHT`): Conduit's counter per successful R&D run, at a run a turn over the horizon, in hand and on the table alike so the install takes nothing the hand had. Before, Conduit in hand promised the counters it places on install — none — and the rig plan never installed it. The other plans read no R&D access beyond the first on the table (§25 Stage 7's decision), so a pressure deck's Conduit is still never installed: that is the style system, not this reading.

`installs_the_cards_that_pay_on_a_run` is the planner line (four credit clicks on `main`, the install on this branch); `a_card_that_pays_on_a_run_is_read_at_a_run_a_turn` and `the_rigs_breach_accesses_are_the_runs_the_first_time_each_turn` hold the readings to their numbers.

**Measured** (pinned binaries, `main` at `feeffc6`):
- `coverage_identical.py main --head-worktree`: both random shapes identical (`57e361e0…`); both planner shapes moved, to one hash (`0122f60e…` → `42b620da…`). Over that planner pass of the pool (192 games): Pennyshaver's successful-run trigger fired 5 → **347** times, Docklands Pass's breach 0 → **90**, Red Team's install 0 → **34**, Conduit's counter 0 → **36**.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.427 → 0.427** (seed 1, z 0.00, 70 discordant) and **0.456 → 0.438** (seed 2, −0.018, z −0.76, 85 discordant) — under the seed-spread band.
- `diag precepts --deck-styles` (planner both chairs; casual 192, startup 90; seeds 1 and 2), both binaries this time, because `main` had taken Midnight Sun cards since §31: Corp share 0.411 / 0.453 → 0.406 / 0.417 casual, 0.444 / 0.578 → 0.533 / 0.600 startup — within the band each way, in opposite directions on the two formats. The Runner's runs fell 0.1–1.5 a game on all four passes (casual 19.4 → 17.9, 19.0 → 18.0; startup 16.7 → 16.6, 17.2 → 15.8), and its credits gained rose 1.2–3.5. Its basic run clicks fell 0.9–2.3 and its credit clicks 1.5–2.4, while its ability clicks — Red Team's runs, Pennyshaver's cash-outs — rose about 1.8. **`blind_cards.py` (against random passes re-taken on `main`) no longer lists Red Team, Pennyshaver, Docklands Pass or Conduit on any pass**: the list is 14 / 13 / 12 / 12 → **13 / 9 / 8 / 11** cards. Six cards entered it on two passes — Byte!, Illumination, Retribution on casual seed 1, and Retribution, Transfer of Wealth, Verbal Plasticity on startup seed 2 — each from 1–4 planner uses on `main` to 0, trajectory drift at counts that small. Tranquilizer left it on casual seed 1. The planner's uses over the four passes: Red Team 0 → 159 / 169 / 81 / 87, Pennyshaver 3 / 0 / 0 / 0 → 300 / 388 / 262 / 188, Docklands Pass 0 → 28 / 29 / 7 / 12, Conduit 0 → 54 / 62 / 12 / 21.
- The cards themselves, seed 2, planner both chairs over 48 games a pairing, random 96 beside (installed; uses in brackets):

  | pairing | Red Team | Pennyshaver | Docklands Pass | Conduit | Runner wins |
  |---|---|---|---|---|---|
  | Brutal Efficiency vs Stolen Goods | 0 → **45** [186] (40 [89]) | 0 → **42** [258] (67 [159]) | 0 → **20** (44) | 0 → 0 (27 [48]) | 39 → 33 |
  | Peculiarity vs Planning Ahead | – | – | – | 0 → **36** [69] (35 [61]) | 27 → 23 |
  | Brutal Efficiency vs Tickets Please | – | 0 → **35** [251] (44 [160]) | 0 → **16** (50) | 0 → 0 (27 [41]) | 36 → 33 |
  | Peculiarity vs Professional Opportunities | 0 → **23** [93] (16 [28]) | – | 0 → **4** (9) | – | 26 → 24 |

  Red Team and Pennyshaver go in at random's rate or past it and are used three to five times a game; Conduit in the rig deck at random's rate; Docklands Pass at a third to half of it, in a turn with an HQ run to make. **Over the four pairings the Runner won 128 → 113 of 192**, down in each (2–6 games), the same direction as §31's 126 → 116 over its four. The self-pairing across the pool does not show it, and the precepts passes split by format. Recorded, not explained: the measured part is that the Runner runs less and holds more credits. Whether the clicks Pennyshaver's cash-out and Red Team's runs take cost the Runner runs it should have made is not shown, and an ablation of each term over these pairings is what would say.

**Still on the list, and why.** Topan "Ormas" Leader's install leads it (61 / 60 casual, 20 / 18 startup): an ability, not a run, and the next reading. Then, Runner: Carnivore, GAMEDRAGON™ Pro, Maglectric Rapid 748 Mod, Botulus (startup), Illumination, Cacophony, Verbal Plasticity, Tread Lightly (startup seed 2 only, as at §31), Fransofia Ward, Transfer of Wealth, Détente. Corp: Byte!, Leo Construction Labor Solutions, Synapse Global's ability, Neurospike, Retribution, Charm Offensive. Public Trail and Bigger Picture are on no pass of this branch's.

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 33. A sample carries both identities: the planner uses Topan's install, Synapse Global's tag and LEO Construction's end-the-run — DONE (`feat/samples-carry-identities`, 2 October 2026)

**Three of the blind list's cards were one blindness.** Topan "Ormas" Leader's install (61 / 60 casual, 20 / 18 startup), LEO Construction Labor Solutions (12 / 10) and Synapse Global's ability (7 / 6, startup 9) are identities, and `determinize` built every sample with `identity: None` — a default kept since Phase 3 §1, where carrying it measured inside MCTS's seed-spread band, and recorded as owed under this phase's Open list "when a search needs it". The planner takes the root from the view's list and every later step, and every judgment of a line, from the sample's; an identity's ability was in the view's list and in no sample's, so the root dropped it before a line was built. The same `None` was read by every evaluator term that reads the Corp's identity across the table (`read::corp_faction`) at every leaf the planner scored: the feared-flatline term against a Jinteki Corp (`FEARED_FLATLINE_WEIGHT` 3.0) read no Jinteki, and the last-click term, which ships at zero, would have read none either.

**What it is now:** `determinize` carries `view.corp.identity` and `view.runner.identity`, both public in every view. `identities_shape_the_prior_and_are_carried_into_the_sample` replaces the test that pinned the old decision, and `plays_its_identitys_ability` is the planner line: Topan on 1[credit] with Pennyshaver in the grip, four credit clicks or two and a full-cost install on `main`, Topan's install on this branch. An identity's triggers and continuous effects (link, hand size, A Teia's remote limit, Weyland Consortium: Built to Last's 2[credit] on a first advance) now run in every sample too.

**The Hit List sweep deck takes two Malandragem.** The 256-seed view sweep's coverage gate failed on Malandragem, the one copy in Grassroots: on `main` it was reached in 2 of 768 games — trashed both times, installed by no seat, random or planner — and in none once the planner's games re-rolled. A second copy there spends 19 influence of Sebastião's 15; Hit List is Gabriel Santiago's, Malandragem's faction (Criminal), so two copies there cost none, take it to 50 cards, and leave Grassroots untouched. With them it is reached in 7 of Hit List's 33 view-sweep games and installed in 2. Sweep decks are never in `matchups()`, so no bench, precepts or coverage report below moves for it.

**Measured** (pinned binaries, `main` at `b381a61`):
- `coverage_identical.py main --head-worktree`: both random shapes identical (`57e361e0…`); both planner shapes moved, to one hash (`42b620da…` → `7409f8fd…`). Over that planner pass, Built to Last's first-advance credits fired 65 → **120** times and PT Untaian's discard-step advance 104 → **25**.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.427 → 0.453** (seed 1, +0.026, z +0.90, 124 discordant) and **0.438 → 0.396** (seed 2, −0.042, z −1.41, 128 discordant) — opposite signs, mean −0.008, each inside the seed-spread band. More games changed hands than at §32 (70 and 85 discordant), as a change to every sample should.
- `diag precepts --deck-styles` (planner both chairs; casual 192, startup 90; seeds 1 and 2): Corp share 0.406 / 0.417 → 0.474 / 0.375 casual, 0.533 / 0.600 → 0.556 / 0.544 startup, in both directions again. Runs on the last click did not fall against a Corp that punishes runs (Jinteki 0.48–0.51 → 0.52–0.54, NBN 0.42–0.49 → 0.45–0.51): the term that would have read the identity there is zero. **`blind_cards.py` no longer lists Topan, LEO Construction or Synapse Global on any pass**: the list is 12 / 8 / 7 / 10 → **8 / 7 / 4 / 4** cards. The planner's uses over the four passes: Topan 0 → 46 / 58 / 37 / 30 (random 61 / 60 / 20 / 18), Synapse Global 0 → 15 / 18 / 16 / 13 (random 7 / 6 / 3 / 9), LEO Construction 0 → 4 / 6 on the casual passes (no startup deck holds it). Others left the list from 0–4 planner uses to 1–7 — Byte!, Neurospike, Retribution and Illumination on casual seed 1, GAMEDRAGON™ Pro and Verbal Plasticity on startup seed 1, Retribution, Tread Lightly, Verbal Plasticity, Transfer of Wealth and Charm Offensive on startup seed 2 — and others entered from 1–8 to 0: Gourmand (8 → 0), Cookbook and Knickknack O'Brian on casual seed 1, Tranquilizer and Cookbook on casual seed 2, Byte! on startup seed 2. Gourmand's 8 → 0 is the largest, and on casual seed 2 it went 7 → 11: trajectory drift, at counts this small, as §32's entrants were.
- The cards themselves, seed 2, planner both chairs over 48 games a pairing (activations; random's in 96 beside):

  | pairing | identity | used | Runner wins |
  |---|---|---|---|
  | Brutal Efficiency vs Prick Thyself | Topan | 0 → **202** (330) | 31 → 28 |
  | Agency vs Stolen Goods | LEO Construction | 0 → **17** (101) | 30 → **40** |
  | Agency vs Tickets Please | LEO Construction | 0 → **29** (88) | 33 → **38** |
  | Gimbatul vs Tickets Please | Synapse Global | 0 → **59** (74) | 25 → 31 |
  | Gimbatul vs Stolen Goods | Synapse Global | 0 → **52** (61) | 25 → 27 |

**Which carry moved the Corp pairings, ablated.** A scratch build read an environment switch in `determinize` (not committed): with only the Runner's identity carried, all four Corp-identity pairings reproduced `main` game for game; with only the Corp's, they reproduced the branch. Splitting the Corp's identity by whose sample took it: carried into the Corp seat's samples alone, Agency reproduced the branch exactly (40 and 38 Runner wins) and Gimbatul did *better* for the Corp than `main` (25 → 20 and 25 → 23); carried into the Runner's alone, both stayed at `main`'s numbers within a game. So Synapse Global's use is no loss, and LEO's is the Corp's own: **the planner trades a bioroid for an ended run it should not.** In a recorded 48 games of Agency against Stolen Goods, 14 uses resolved (three more parked on a payment): 9 before the Runner had approached any ICE, 2 ending Archives runs, 10 trashing Mercia B4LL4RD and 4 a piece of ice (Bumi 1.0 three times, Bran 1.0 once). The reason is in the weights: ending a run returns `ACTIVE_RUN_AGAINST_WEIGHT` (1.5) and twice the run's stakes, whatever the run would have reached, and Mercia — whose text installs a barrier from HQ at 1[credit] less every turn, a value no evaluator term reads — is worth its board presence. Over the pool the bench barely sees it (Agency's Corp wins 4 → 3 and 7 → 5 of 24); it is the next debt, not this change's to hide by leaving LEO's ability out of the sample.

**Still on the list, and why.** Runner: Carnivore (12 / 27, startup 6), GAMEDRAGON™ Pro (15 / 13), Maglectric Rapid 748 Mod (on all four passes), Botulus (startup, both seeds), Cacophony, Cookbook, Gourmand, Tranquilizer, Knickknack O'Brian, Fransofia Ward, Détente. Corp: Byte! (startup, both seeds), Neurospike (casual seed 2). And a paid end-the-run priced against what the run threatens, with Mercia B4LL4RD's turn-by-turn install read, before LEO Construction is played well.

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release (after the Hit List copies).

## 34. A run is priced with what both identities print about it: Gabriel Santiago's, Zahya Sadeghi's, Dewi Subrotoputri's and René "Loup" Arcemont's pay at the leaf, and BANGUN's punishment at the access — DONE (`feat/a-run-reads-both-identities`, 3 October 2026)

**The first of four stages of the Open item the person flagged on 3 October 2026 as a huge miss**: §33 put both identities into every sample, but no evaluator term read what an identity *does*. What an identity pays inside the planner's own line lands in the state the line produces, so it needed no reader; what it pays at a moment the line does not reach had none. The run is the largest such moment, because the planner prices a run as a leaf at its initiation, and most of the pool's run-facing identities are about the run's success, breach, accesses or end: Gabriel Santiago's 2[credit] for the turn's first successful HQ run, Zahya Sadeghi's credit for each card a run on HQ or R&D accessed, Dewi Subrotoputri's credit or card, René "Loup" Arcemont's credit and card for the turn's first trash while accessing, Mercury Chrome's extra access, and on the Corp's side BANGUN: When Disaster Strikes' 2 meat damage and a tag when the Runner accesses a faceup agenda.

**What it is now:** a module, `eval::identities`, reads both identities' triggers off the DSL — never the name — at a moment priced ahead of time, the way the engine's listener scan would hear them: the trigger's `when` against the server, "the first time each turn" against the turn's log (as `rig_breach_accesses` reads Docklands Pass), and the intervening "if" through the engine's own `check_requirement`, now public for this and only this. The one difference is the moment: a condition about a run that has not finished is answered for the run being priced — "the last run was on HQ or R&D" is this run's server, "a card was accessed" the breach's count, "breaching HQ" the server it will breach, "accessing an installed card" the card the access finds. What is counted is what a click is weighed against (credits and cards to either side, accesses, damage and tags to the Runner); a choice is its chooser's best option, and a selection's `then` and a flip are not read, the cheaper direction (Pravdivost Consulting's counter, Ryō "Phoenix" Ōno's discard). Three readings use it:
- **The Runner's run leaf** adds what the run's success pays, at the rider's rates (a credit at `own_credit_weight`, a card at `click_weight`; the Corp's at `opponent_credit_weight`), and the breach's added accesses join the rider's and the rig's.
- **The Corp's run term** subtracts what the Runner's identity takes off the run it is paying to stop, and adds what its own takes — so ending Gabriel's first HQ run of the turn is worth two of the Runner's credits more to the Corp.
- **The access**: a rezzed or seen card in the root is read with what the Corp's identity does to its access — BANGUN's damage counted with a trap's toward the flatline it can be, its tag at `tag_weight` — and the first trash's lift is added once (René). **A faceup agenda, or one the Runner accessed and could not steal, is now a steal the breach cannot miss**, as a face-up agenda in Archives already was: before, an agenda in a root was worth nothing once it was not face down, so a BANGUN Corp's faceup agendas were runs priced at nothing and stolen anyway, into 2 meat damage and a tag the Runner had never weighed.

**Measured** (pinned binaries, `main` at `e7fc0d9`):
- `coverage_identical.py main --head-worktree`: both random shapes identical (`57e361e0…`); both planner shapes moved, to one hash (`7409f8fd…` → `799ec40e…`). Over that pass Dewi Subrotoputri's trigger fired 10 → 16, René's 108 → 125, Zahya's 190 → 199, BANGUN's 28 → 41.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.453 → 0.440** (seed 1, −0.013, z −0.78, 41 discordant) and **0.396 → 0.391** (seed 2, −0.005, z −0.32, 40 discordant) — inside the seed-spread band, toward the Runner on both seeds.
- `diag precepts --deck-styles` (planner both chairs): Corp share casual **0.474 → 0.443** and **0.375 → 0.344** (192 games, seeds 1 and 2, the Runner's way both times), startup 0.556 → 0.622 and 0.544 → 0.500 (90 games, both ways). The blind list moved by drift only (casual 8 / 7 → 9 / 8, startup 4 / 4 → 4 / 6, every entrant and leaver at 0–5 uses).
- The identities, seed 2, planner both chairs, 48 games a pairing (triggers fired, Runner wins):

  | pairing | identity | fired | Runner wins |
  |---|---|---|---|
  | Undertow vs Hit List | Gabriel Santiago | 331 → **359** | 35 → 39 |
  | Fine Print vs Enthusiasm | Dewi Subrotoputri | 38 → **95** | 31 → 29 |
  | Hidden Funds vs Party Hard | René "Loup" Arcemont | 141 → **159** | 36 → 33 |
  | Glyph of Warding vs Bowel Movements | René "Loup" Arcemont | 173 → **218** | 27 → 26 |
  | Brick Stack vs Tickets Please | Zahya Sadeghi | 277 → **302** | 29 → 29 |
  | Pork Chops vs Stolen Goods | BANGUN / Zahya | 108 → 113 / 247 → 218 | 12 → 13 |
  | Pork Chops vs Dashing Mad | BANGUN / Ryō | 123 → 156 / 106 → 109 | 11 → 15 |

  The identities are used more in six of the seven, and the wins move within a few games either way: the reading is the Runner's use of its own identity, not a new line of play.

**What the BANGUN games are, recorded.** On `main` the BANGUN deck beat Stolen Goods 36 games in 48, 32 of them by flatline, and the first guess was that the Runner walked into the identity's damage. Twelve recorded games say otherwise: seven ended on Measured Response's "do 4 meat damage unless the Runner pays 8[credit]", declined by a Runner without the 8, one on an access. The faceup agendas' 2 meat damage thins the grip; the operation kills. The Runner now prices the punishment and the steal, and still steals (a point of agenda is 20.0 against a tag's 4.0), and keeps its grip above the access's damage — but nothing it reads fears a Weyland operation in HQ, which is the feared-flatline term's gap and not an identity's.

**Still owed, as stages 2–4 of the Open item:** the turn's end (PT Untaian's advance, Nebula Talent Management's credit, Jinteki: Restoring Humanity's credit, Magdalene Keino-Chemutai's install), steals, scores and tags across the table (Jinteki: Personal Evolution's damage, Thule Subsea's tax, Poétri's and Tāo Salonga's triggers, tags as Synapse Global's and NBN: Reality Plus's credits, and the Corp's side of BANGUN — it turns every agenda faceup for the rezzed-asset weight, not for the punishment), and standing effects and hosted counters (Issuaq Adaptics' points, AU Co.'s and Epiphany Analytica's counters, Precision Design's hand size, Kate "Mac" McCaffrey's discount).

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 35. The seat's own turn's end is planned: PT Untaian advances at its discard step and Magdalene Keino-Chemutai installs from what she discarded — DONE (`feat/turn-end-identities`, 3 October 2026)

**The second stage of the Open item on the identities' text, and it was not a reading the evaluator lacked.** The stage was written as "what an identity pays when a turn or a step ends, past the planner's last click", and the first measurement said most of it was already paid: the planner's line runs through `EndTurn` (`settle` passes the opponent's priority until the other side's turn), so an automatic turn-end credit lands in the state the line is scored at — Jinteki: Restoring Humanity's fired 196 times in 48 planner games against random's 491 in 96, Nebula Talent Management's 118 against random's 177 in 96. What did not land was every **decision** the turn's end hands its seat. PT Untaian's "you may pay 1[credit] to place 1 advancement counter" was accepted by the planner 27 times in 108 offers (random 115 in 463), and twelve recorded games of Peculiarity against Stolen Goods said why, case by case: 8 declines with no card to advance (right), 11 accepts, and **5 declines with a card to put the counter on**.

**Why, traced.** `turn::finish_turn` ends a turn in one step: the discard phase's end is dispatched, then the other side's start of turn is entered — its phase and the turn counter — before a decision those triggers parked is answered. So PT Untaian's offer is asked of the Corp in the *Runner's* start of turn, and to the planner's `settle` the turn was over: it tried to pass for the Corp, could not, and ended the line there with the offer unanswered, so every line scored the same unanswered offer and none chose. The decision then fell to the one-ply chooser — the planner had made no plan there and followed none, at each of the five declines replayed from its first click on four seeds — and the one-ply bound on the selection that follows takes the worst card the filter could match, ignoring "unrezzed" (`fundamentals::advancement_upside`): one sprung trap on the table made it zero, and accepting was a credit and a parked prompt for nothing. The same held for Magdalene Keino-Chemutai's "you may install 1 program or piece of hardware from among those cards" (0 uses in 48 planner games, random 19 in 96) and Méliès U.'s number.

**What it is now:** a decision parked on the seat in the other side's start of turn, on the turn its own ended or the next, with no run, is that turn's end (`planner::owes_its_turns_end`): `settle` keeps it a step of the line (`Standing::Open`) rather than passing, and `plannable` plans from it when no standing plan reached it (a plan is dropped at a turn the view's counter has left). `takes_its_identitys_advance_at_the_end_of_its_turn` is a declined board's shape — Send a Message with a counter on it, a sprung Urtica Cipher beside it — and fails on `main` (the counter is not placed).

**Measured** (pinned binaries, `main` at `682802e`):
- `coverage_identical.py main --head-worktree`: both random shapes identical (`57e361e0…`); planner shapes `799ec40e…` → `501c7f5d…`. Over that pass PT Untaian's offer fired 26 → **102** times. Other discard-step decisions moved with it: Sericulture Expansion's 0 → 15, Off the Books' 21 → 7 — the planner now judges Off the Books' "remove 1 hosted agenda counter to search R&D" by the evaluator, which holds a counter at `AGENDA_COUNTER_WEIGHT` (2.0) and the search it buys at less, so it keeps it (in the BANGUN deck that holds it: 0 → 0 and 0 → 3 uses, 13 and 15 Runner wins of 48 on both builds).
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.440 → 0.419** (seed 1, −0.021, z −1.41, 32 discordant) and **0.391 → 0.354** (seed 2, −0.036, z **−2.86**, 24 discordant) — past the band on seed 2, toward the Runner on both. **Split by seat on seed 2**, a scratch build switching the change off for one seat (not committed): on the Runner's seat alone 0.391 → **0.365** (z −2.67, 14 discordant), on the Corp's alone 0.391 → 0.380 (z −1.15, 12). The Runner's part is one deck: **Sabbatical (Magdalene) won 12 → 17 and 10 → 21 of its 32 games** on the two seeds, 10 of the Runner-seat's 14 discordant games. Peculiarity (PT Untaian) went 8 → 8 and 8 → 5 of 24 in the bench and 20 → 26 of 96 in its pairings, 36 → 39 of 144 together: the advance is taken five to seven times as often and the deck neither gains nor loses for it.
- `diag precepts --deck-styles` (planner both chairs): Corp share casual 0.443 → 0.432 and 0.344 → 0.302 (192 games), startup 0.622 → 0.611 and 0.500 → 0.533 (90). The blind list by drift: casual 9 / 8 → 10 / 6, startup 4 / 6 → 3 / 5 (Byte!, Neurospike, GAMEDRAGON™ Pro and Botulus each off one pass, Tread Lightly on one).
- The identities, seed 2, planner both chairs, 48 games a pairing:

  | pairing | identity | uses | Runner wins |
  |---|---|---|---|
  | Peculiarity vs Stolen Goods | PT Untaian | offers 108 → 303, advances 27 → **140** | 36 → 37 |
  | Peculiarity vs Dashing Mad | PT Untaian | offers 132 → 380, advances 24 → **167** | 40 → 33 |
  | Brick Stack vs Sabbatical | Magdalene Keino-Chemutai | installs 0 → **85** | 25 → 29 |
  | Honor Roll vs Tickets Please | Méliès U. | prompts 25 → 20 | 27 → 30 |
  | Not So Subtle vs Stolen Goods | Nebula Talent Management | 118 → 118 | 22 → 20 |
  | Hidden Funds vs Stolen Goods | Jinteki: Restoring Humanity | 196 → 203 | 31 → 28 |

  PT Untaian's offer comes three times as often because the planner now keeps HQ at three cards or fewer for it — the identity's condition, read through the line it plans.

**Still owed, as stages 3 and 4 of the Open item**, and one thing found here: Off the Books' counter (and any stored counter whose use is a search) is held rather than spent once the planner judges the offer, which is `AGENDA_COUNTER_WEIGHT`'s question, not the turn's end's.

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 36. Steals, scores and tags across the table: core damage is a hand size, Thule Subsea's tax is paid, Synapse Global's and Poétrï's installs are taken, and a faceup BANGUN agenda is worth its punishment — DONE (`feat/identities-steals-scores-tags`, 3 October 2026)

**The third stage of the Open item on the identities' text.** Measured first, on `main` (`fda92c8`), planner both chairs, 48 games a pairing on seed 2, the identities that act when an agenda is scored or stolen or a tag is given or removed:

- **Thule Subsea**: of its "do 1 core damage unless they spend [click] and 2[credit]" the Runner paid 6 and 14 times and took the damage 114 and 118. Nothing on either chair read core damage — `RunnerState::brain_damage` had no term — so the payment was a click and two credits against nothing.
- **Synapse Global**: its "you may reveal and install 1 card from HQ, ignoring all costs" was offered 129 and 122 times and taken about 5 and 4 (the prompt's actions beyond its confirm). **Poétrï Luxury Brands**: 127 and 98 offers, about 9 and 4 taken. Both are asked in the Runner's turn, where the planner played one ply, and `fundamentals::pending_decision_upside` prices a "may" at its worst resolution — choosing nothing — so a toggle left the selection open and lost to a confirm with nothing chosen.
- **Jinteki: Personal Evolution**: every Corp win in its pairings was a flatline (4 and 7 of 48), and no leaf priced the net damage a steal deals.
- **BANGUN**: agendas went faceup 124 and 118 times, priced as rezzed assets (`REZZED_ASSET_WEIGHT`), not for the punishment the identity deals.
- **NBN: Reality Plus**'s first-tag credits and **Tāo Salonga**'s swap: see below.

**What it is now.**
- `CORE_DAMAGE_WEIGHT` (1.5): each point of core damage the Runner has taken is a hand size, subtracted by the Runner and added by the Corp (CR 5.5.3b). Above the guide's click and two credits (1.2), so a Runner who can pay Thule does.
- `eval::identities` reads a steal and a tag: `on_steal` (Personal Evolution's damage, Thule's tax as the payer's cheaper side of the ones it can afford, Poétrï's install when HQ holds a card the selection would offer — the engine's own `eligible_positions`, now exported beside `check_requirement`) and `on_tags` (Reality Plus, the turn's first tag). `access_prospect` charges a steal's pays for every agenda the breach cannot miss and, at its chance (the expected points over `TYPICAL_AGENDA_POINTS`, the pool's 44 agendas averaging 1.9), for each card it has not seen; a steal's damage that would empty the grip is the flatline at that chance — an empty grip on an R&D run under Personal Evolution.
- `planner::its_identitys_selection`: a selection the seat's own identity parks on it, over its own cards, is a root to plan from and, while the seat still owes the decision, a step of the line — the selection, its confirm and the server the install asks for — wherever it stands (`Search::deciding`). Over its own cards only, because that is what the evaluator prices where it stands.
- A faceup agenda is an install whose text is not active, worth what its access does to the Runner at the Corp's rates (`corp::faceup_agenda_punishment`: the grip below `opponent_grip_floor`, the hand size core damage takes, a tag the Corp holds a punishment for) — not a rezzed asset's weight.

Tests: `takes_its_identitys_free_install_when_the_runner_removes_a_tag` and `installs_from_hq_when_its_agenda_is_stolen` fail with the planning branch switched off (nothing is installed); `personal_evolution_and_thule_subsea_charge_a_steal`, `poetri_installs_on_a_steal_when_hq_holds_a_card_to_install`, `reality_plus_is_paid_for_the_turns_first_tag`, `a_run_on_rnd_under_personal_evolution_with_an_empty_grip_risks_the_flatline`, `core_damage_is_a_hand_size_both_chairs_read`, `a_faceup_agenda_is_worth_its_punishment_and_not_an_assets_weight`.

**Measured** (pinned binaries, `main` at `fda92c8`):
- `coverage_identical.py main --head-worktree`: both random shapes identical (`57e361e0…`); planner shapes `501c7f5d…` (§35's, reproduced) → `6d72a95a…`.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.419 → 0.393** (seed 1, −0.026, z −1.96, 26 discordant) and **0.354 → 0.365** (seed 2, +0.010, z +0.69, 34 discordant) — opposite directions, inside the band.
- `diag precepts --deck-styles` (planner both chairs): Corp share casual 0.432 → 0.391 and 0.302 → 0.333 (192 games), startup 0.611 → 0.611 and 0.533 → 0.500 (90). The blind list by drift: casual 9 / 6 → 9 / 8 (Neurospike and Tread Lightly on seed 2), startup 3 / 5 → 3 / 3 (Byte! and Transfer of Wealth off). The pool's one core-damage card in these passes, Bumi 1.0, was used about as often (138 / 134 / 60 / 47 → 130 / 134 / 57 / 51).
- The identities, seed 2, planner both chairs, 48 games a pairing:

  | pairing | identity | uses | Corp wins |
  |---|---|---|---|
  | Undertow vs Stolen Goods | Thule Subsea | Runner pays 14 → **77**, takes it 118 → 51 | 2 → 1 |
  | Undertow vs Dashing Mad | Thule Subsea | Runner pays 6 → **79**, takes it 114 → 49 | 2 → 4 |
  | Gimbatul vs Stolen Goods | Synapse Global | cards chosen 5 → **54** in 127 offers | 22 → **26** |
  | Gimbatul vs Dashing Mad | Synapse Global | cards chosen 4 → **61** in 124 offers | 12 → **16** |
  | Fashion Lab vs Stolen Goods | Poétrï | cards chosen 9 → **96** | 14 → 14 |
  | Fashion Lab vs Dashing Mad | Poétrï | cards chosen 4 → **81** | 6 → **11** |
  | A Thousand Cuts vs Stolen Goods | Jinteki: Personal Evolution | flatlines 7 → 6 | 7 → 6 |
  | A Thousand Cuts vs Dashing Mad | Jinteki: Personal Evolution | flatlines 4 → 2 | 4 → 2 |
  | Pork Chops vs Stolen Goods | BANGUN | faceup 118 → 119, scored 49 → 35 | 35 → 33 |
  | Pork Chops vs Dashing Mad | BANGUN | faceup 124 → 109, scored 49 → 30 | 33 → 33 |
  | Hyper Velocity vs Stolen Goods, Fine Print vs Dashing Mad | NBN: Reality Plus | identical | 14, 15 |
  | Peculiarity vs Planning Ahead, Brick Stack vs Flow and Ebb | Tāo Salonga | identical / 183 → 179 offers | 15, 20 → 22 |

  BANGUN's Corp scores fewer agendas and wins as often: its wins are flatlines (29 → 31 and 31 → 31). Why it scores fewer was not traced; recorded rather than explained. NBN: Reality Plus's pairings are byte-identical: its 2[credit] on the tags a breach deals is read and changed no decision.

**Not done, and why.**
- **Tāo Salonga's swap** stays one ply (an Open item of its own): what two pieces of ICE are worth in each other's places is where each stands against the rig, which no term reads off a run, so planned, the swap and the decline would tie and the jitter would swap at random. Personal Evolution's damage on a *score* needed nothing: it lands in the Corp's own line, and the Runner's feared flatline already reads a Jinteki Corp at three.
- **Synapse Global's tags as standing credits** were ruled out by arithmetic rather than measured: a carried tag worth the cash-out to the Corp makes cashing it in the line a tie with holding it (0.8 either way at the guide's rate), when the Runner clears a held tag on its turn. The click itself was already in the Corp's line (§33).

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 37. Standing effects and hosted counters: Issuaq Adaptics' counters are points, a hosted counter is half of what it buys, AU Co.'s search is planned, Kate "Mac" McCaffrey's discount is in a held card's price — and an agenda in Archives is a loss to the Corp — DONE (`feat/identity-standing-effects`, 4 October 2026)

**The fourth and last stage of the Open item on the identities' text.** Measured first, on `main` (`b040f11`), planner both chairs, 48 games a pairing on seed 2, the identities whose text stands or hosts counters:

- **AU Co.: The Gold Standard in Clones**: its "when your turn begins, you may remove 2 hosted power counters to look at the top 3 cards of R&D. Trash 1 of those cards and add the rest to HQ" was declined 530 and 390 times and taken 46 and 63 (the Corp's paid choices, almost all of them this one). It is asked in the Corp's own start of turn, ahead of the action phase a plan starts from, so the one-ply chooser answered it, and the accepted side was a selection `pending_decision_upside` prices at its worst.
- **Issuaq Adaptics: Sustaining Diversity**: its counter — "you need 1 less agenda point to win the game" for each — was placed 0 times in either pairing, the Corp scoring 5 and 6 agendas in 48 games and winning 1 of each. Nothing read a counter as a point, and an agenda advanced to its requirement was scored the turn it was advanced, which the identity's "an agenda that you did not install or advance this turn" excludes.
- **Epiphany Analytica: Nations Undivided**: its "[click], hosted power counter: look at the top 3 cards of R&D. You may install 1 of those cards" was used 22 and 15 times against about 100 counters gained.
- **Haas-Bioroid: Precision Design**: its scoring trigger was taken (prompts 20 and 42), and its "+1 maximum hand size" is the engine's discard at the turn's end, a step of the planned line since §35.
- **Kate "Mac" McCaffrey**: the discount on the turn's first program or hardware is the engine's price when the line installs; a held card was priced at its printed cost, and so was the breaker the Runner saves for.
- **Off the Books** (§35's finding): a scored agenda's counter was worth `AGENDA_COUNTER_WEIGHT` (2.0) whatever it bought, and the search it buys less, so once the planner judged its offer it held the counter.

**What it is now.**
- **A hosted counter is worth half of what spending it buys** (`identities::counter_worth`, `COUNTER_USE_SHARE`): the best use that removes counters — a `Paid` ability, or an offer a trigger makes the Corp — read as a trigger's pays are, at the evaluator's own rates (an install `unrezzed_install_weight`, an advancement counter `advancement_weight`, a card into HQ what `zone_size_value` prices it at, a credit `own_credit_weight`), less the clicks and credits the use costs, over the counters it spends. Half, because a counter held is a use deferred and the use scores whole where it is made: at all of it spending and holding tie, above it a counter is never spent. The identity's counters (AU Co., Epiphany Analytica) and a scored agenda's (Off the Books, Sericulture Expansion, Project Ingatan) are read so, and a Dividends agenda's promised counters with them; a counter no reading prices (Embedded Reporting's operation set on R&D, Proprionegation's moved run) keeps `AGENDA_COUNTER_WEIGHT`. `identities::paid` reads a selection of the Corp's whole for it: the cards it takes into HQ from R&D or Archives, the advancement counters its `then` places, and its `then` after it.
- **A point a side's cards spare it of the target is a point** (`evaluate_state_with`, both chairs), read off the engine's own `continuous::points_to_win`, and the stage reads each side's own target (`stage(state, registry)`).
- **An agenda advanced this turn under Issuaq is worth its points and the counter's a turn later** (`corp::held_for_a_later_score`), where the server would keep the Runner out for a turn (`holds_a_turn`: ICE the rig cannot break, or a break it cannot pay for out of its credits and a turn's clicks), at `LATER_SCORE_SHARE` (0.9 — any share in (3/4, 1) makes the same choices over the pool's agendas). Nothing where a score now places the counter, so an agenda ready since an earlier turn is scored. The trigger is read off the identity's `when` (`identities::counters_on_score`), with the install's own count of advances (`CopyTurn::count`, now public).
- **A paid choice the seat's own identity offers it is planned** (`planner::its_identitys_offer`, beside §36's selection): AU Co.'s search is a step of the turn's line, and the cards it brings are cards the line can play.
- **A held card's price is the engine's** (`runner::held_price`, `continuous::install_cost_of`, now public), in `install_delta` and `breaker_savings_shortfall`.
- **An agenda in Archives is half its points to the Corp** (`ARCHIVED_AGENDA_WEIGHT`, 10.0 a point). Found here and not an identity's: once AU Co.'s search was taken, its "trash 1 of those cards" sent agendas to Archives — agendas trashed 58 → 80 and 22 → 58 in the two AU Co. pairings, agendas scored 65 → 38 — because nothing on the Corp's side read where its agendas went, and every choice of which card to trash was the jitter's whenever one was an agenda.

Tests: `takes_au_cos_search_at_its_turn_start_and_keeps_the_agenda` (taken in 0 of 4 seeds with the planning switched off), `holds_an_agenda_it_advanced_for_issuaqs_counter_and_scores_one_ready_since` (scores it the turn it is advanced with the hold switched off), `a_hosted_counter_is_worth_half_of_what_spending_it_buys`, `issuaq_counts_a_score_now_and_a_score_later`, `a_ready_agenda_is_held_for_issuaqs_counter_behind_a_wall_that_holds`, `issuaqs_counters_are_points_on_both_chairs`, `an_agenda_in_archives_costs_the_corp_half_its_points`, `kates_discount_is_in_a_held_cards_price`. The AU Co. test plays below the HQ floor: above it a card in HQ is worth nothing to the evaluator (`zone_size_value`, a decision recorded long before this stage), and the search is a tie the line breaks by what it would play.

**Measured** (pinned binaries, `main` at `b040f11`):
- `coverage_identical.py main --head-worktree`: both random shapes identical (`57e361e0…`); planner shapes `7ade4422…` → `c7a130b7…`.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.404 → 0.458** (seed 1, +0.055, z +1.82, 133 discordant) and **0.372 → 0.458** (seed 2, +0.086, z +3.05, 117 discordant) — toward the Corp on both, past the band on seed 2. **Attributed by ablation**, each binary the branch with one piece switched off, paired against the branch: without the Archives charge 0.404 / 0.380 → 0.458 / 0.458 (**+0.078 on both seeds, z +2.65 and +2.76**); with counters at the flat 2.0, +0.008 on both (11 and 7 discordant); without AU Co.'s offer planned, −0.008 and +0.008 (5 discordant each). So the move is the Archives charge, a Corp chair that stopped giving agendas away: over a planner pass of the pool (192 games, seed 1, the comparison's own reports) agendas trashed **68 → 10**, stolen 506 → 475, scored 327 → 340, Corp wins 76 → 90. Which effects did the trashing on `main` was not traced; the count is the event's (`CardTrashed`), not the Corp's discards at hand size.
- `diag precepts --deck-styles` (planner both chairs): Corp share casual 0.396 → 0.469 and 0.370 → 0.464 (192 games), startup 0.611 → 0.544 and 0.500 → 0.578 (90). The blind list against random passes retaken on `b040f11` (Midnight Sun changed the pool): casual 9 / 6 → 6 / 8, startup 3 / 3 → 5 / 7, every name one the list already held.
- The identities, seed 2, planner both chairs, 48 games a pairing:

  | pairing | identity | uses | Corp wins |
  |---|---|---|---|
  | Glyph of Warding vs Dashing Mad | AU Co. | search taken 46 → **161**, declined 530 → 183 | 19 → 15 |
  | Glyph of Warding vs Stolen Goods | AU Co. | search taken 63 → **161**, declined 390 → 214 | 24 → 21 |
  | Permafrost vs Dashing Mad | Issuaq Adaptics | counters 0 → 4, scored 5 → 8 | 1 → 2 |
  | Permafrost vs Stolen Goods | Issuaq Adaptics | counters 0 → 0, scored 6 → 5 | 1 → 0 |
  | Grand Opening vs Dashing Mad | Epiphany Analytica | ability 22 → 13 | 4 → 5 |
  | Grand Opening vs Stolen Goods | Epiphany Analytica | ability 15 → 11 | 1 → 3 |
  | Brutal Efficiency vs Dashing Mad | Precision Design | prompts 20 → 19 | 15 → 16 |
  | Discretion Advised vs Stolen Goods | Precision Design | prompts 42 → 36 | 19 → 20 |
  | Undertow vs Safety Net | Kate "Mac" McCaffrey | — | 4 → 2 |
  | A Thousand Cuts vs Safety Net | Kate "Mac" McCaffrey | — | 17 → 13 |

  AU Co.'s two pairings lost wins on seed 2, so the deck was played wider: against four Runner decks on seeds 1 and 3 (384 games), Corp wins **164 → 173**, Dashing Mad the one deck it lost to more (18 → 15, 20 → 16). The search is still declined about half the time: above the HQ floor the evaluator prices its cards at nothing. Issuaq's deck is a Sweep traps deck that scores a handful of agendas in 48 games; its counter is placed where the hold applies, and the deck is no stronger for it. §36's fourteen identity pairings, retaken: Corp wins 228 → 255 of 672, every §36 behaviour held (Thule's tax paid 78 / 79 → 88 / 77; Synapse Global's selections 197 / 180 → 212 / 176 actions on 129 / 127 → 138 / 123 offers, Poétrï's 176 / 183 → 159 / 234 on 97 / 100 → 85 / 128).

**Not done, and why.**
- **AU Co.'s search above the HQ floor** is a tie: a card in HQ above `HQ_FLOOR` is worth nothing to the Corp's evaluator, by a decision older than this series ("a draw into a healthy hand is correctly credited zero rather than optimistically"). Pricing the Corp's held cards is its own change.
- **AU Co.'s and Epiphany Analytica's counter moments** — the damage and HQ trashes that give AU Co. one, the steal or trash that gives Epiphany one — are not read at the Runner's leaf: a counter is worth a few tenths of a point where it is read at all, and the Runner's own line never holds the moment.
- **Tāo Salonga's swap** stays the Open item it was (§36).

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 38. Tāo Salonga's swap is priced by where each piece of ICE stands: the Runner reads the doors the Corp's ICE shuts, off a run, and the swap is planned — DONE (`feat/tao-salonga-swap`, 4 October 2026)

**The last of the identities' text** (§34–§37 read the rest on both chairs in four stages). Measured first, on `main` (`11447bc`), planner both chairs, 48 games a pairing on seed 2: Tāo Salonga's "whenever an agenda is scored or stolen, you may swap 2 installed pieces of ice" was offered 634 times over four pairings (Peculiarity, Brick Stack, Gimbatul and Undertow against Planning Ahead and Flow and Ebb) and **taken 0 times** — not one `IceSwapped`. Two things stood in the way:

- **Nothing read where a piece stood off a run.** The Runner priced the ICE only on a run, as the leaf's break cost, so a swap changed nothing the evaluator could see until a run went through the server it touched.
- **The choice was one ply.** Its yes parks a selection, which `fundamentals::pending_decision_upside` prices at its worst resolution — for Tāo, nothing — so the yes never beat the no.

**What it is now.**
- `runner::shut_doors`: for every server behind rezzed ICE, what a breach of it is worth (`access_prospect`'s reading, as a run to come would find it, nothing there seen yet) at the share its ICE shuts the rig out — all of it behind a piece no rig card breaks, `cost / (cost + RUNNER_TURN_CLICKS)` behind ICE the rig breaks for `cost`, so a door that costs a turn's clicks in credits is half shut. Subtracted at `SHUT_DOOR_WEIGHT`, one of every Runner planner seat's general terms (zero in `Weights::default()`, so the one-ply reference and MCTS do not move). The Runner's credits are not read, so a credit click opens no door; rezzed ICE only, as `server_break_cost` reads it.
- `planner::its_identitys_choice`: a choice the seat's own identity parks on it whose yes is a selection of the other side's installed cards is a root to plan from, and `its_identitys_selection` admits that selection (`OpponentInstalled`) beside the seat's own cards.
- `planner::whole_sets`: while the seat decides for its identity, a selection of an exact count of two or more is expanded as its whole sets (up to `WHOLE_SETS`, 120), each scored where its swap leaves the board. Toggled a card a ply, every first card ties and the beam keeps six of them at random, so of the pairs among ten pieces the best was out of reach about one time in eight.

Tests: `tao_salonga_swaps_the_piece_the_rig_cannot_break_off_rnd_and_never_onto_it` (six seeds, both boards: the code gate the rig cannot break is swapped off R&D, and never onto it) fails with either the reading or the choice's planning switched off — the reading off, the swap and the decline tie exactly and the jitter took the swap that shuts R&D; the choice one ply, the swap is never taken. `a_server_behind_ice_the_rig_cannot_break_is_a_shut_door`. With clicks left after the steal, the line goes on to run the opened server and its run leaf already prices the swap; the reading is what decides it on the last click and on the Corp's turn.

**The weight is a tenth of the breach's rate, and that is the measurement.** At one, the reading also pulled every Runner toward installs (+0.3 install clicks a game, +0.1 breakers, 3 to 5 fewer credits summed over its turn starts), and the Startup pass moved toward the Corp: **+50 games to −32 over three seeds of 90** (z +1.99; Corp share 0.544 / 0.578 / 0.511 → 0.656 / 0.622 / 0.556), in Enthusiasm, Sabbatical, Flow and Ebb and Dashing Mad, while the Casual pass leaned the other way (0.469 → 0.458, 0.464 → 0.438) and the paired bench did not move (−0.008, z −0.30; +0.005, z +0.18). At a tenth the Startup pass is +30 to −29 (z +0.13) and Tāo swaps as often — a swap is taken on the reading's sign, and the gain a swap was taken on (median 0.44 at one, in a probe of 146 choices over two pairings: 91 above the jitter, 53 within it, 1 below) is still about forty times the jitter at a tenth. A quarter measured as close on Startup (0.567 / 0.544 / 0.544); a tenth is the one shipped.

**Narrowed once, measured.** The first cut planned every "may" an identity parks ahead of a selection — Haas-Bioroid: Precision Design's, Méliès U.'s, Barry "Baz" Wong's, Magdalene Keino-Chemutai's and Sebastião Souza Pessoa's over their own cards as well as Tāo's. With the reading switched off so that nothing else moved, the Startup pass went toward the Corp, +15 to −4 over two seeds (z +2.52), in Barry's, Magdalene's and Tāo's decks; Barry's install from the grip, offered when the Corp rezzes a barrier mid-run, was made 37 → 122 times in 48 games. Why that costs the Runner was not traced, so the rule is Tāo's kind only, and the other five stay one ply (an Open item).

**Measured** (pinned binaries, `main` at `11447bc`):
- `coverage_identical.py main`: both random shapes identical (`57e361e0…`); planner shapes `c7a130b7…` (§37's, reproduced) → `910417e6…`.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.458 → 0.432** (seed 1, −0.026, z −1.25, 64 discordant) and **0.458 → 0.451** (seed 2, −0.008, z −0.33, 85 discordant) — toward the Runner, inside the band.
- `diag precepts --deck-styles` (planner both chairs): Corp share casual 0.469 → 0.411 and 0.464 → 0.438 (192 games; seed 1 +9 / −20, z −2.04, of which the Tāo decks' 32 games are +1 / −8 and the other 160 +8 / −12, z −0.89; seed 2 +20 / −25, the Tāo decks +4 / −5); startup 0.544 / 0.578 / 0.511 → 0.567 / 0.556 / 0.522 (90 games, seeds 1–3, paired +30 / −29). The blind list by drift: casual 6 / 8 → 7 / 5, startup 5 → 3 (seed 1).
- Tāo Salonga and the identities the first cut touched, seed 2, planner both chairs, 48 games a pairing:

  | pairing | identity | swaps | Corp wins |
  |---|---|---|---|
  | Brick Stack vs Flow and Ebb | Tāo Salonga | 0 → **137** of 169 offers | 24 → 17 |
  | Gimbatul vs Planning Ahead | Tāo Salonga | 0 → **146** of 162 | 20 → 20 |
  | Peculiarity vs Planning Ahead | Tāo Salonga | 0 → **133** of 147 | 21 → 19 |
  | Undertow vs Flow and Ebb | Tāo Salonga | 0 → **104** of 131 | 5 → 4 |
  | Brutal Efficiency vs Stolen Goods, Discretion Advised vs Dashing Mad | Precision Design | — | 13 → 12, 24 → 24 |
  | Honor Roll vs Stolen Goods | Méliès U. | — | 17 → 17 |
  | Gimbatul vs Professional Opportunities, Sabbatical, Grassroots | Barry, Magdalene, Sebastião | — | 28 → 28, 19 → 19, 37 → 38 |

  The Tāo Corp's wins 70 → 60 of 192; each pairing is inside 48 games' noise.
- **Rechecked after the rebase onto `6aa5389`** (#354, Uprising Stages 1–3, merged while this was measured; its new cards change what every sample draws): both random shapes identical (`9389f8b1…`), planner shapes `8fe528ef…` → `50c72ac6…`; paired bench **0.451 → 0.435** (z −0.72, 70 discordant) and **0.445 → 0.443** (z −0.11, 77); Tāo's swaps 0 → 134, 159, 130, 117 in the same four pairings, and the Tāo Corp's wins 69 → 69 — the 70 → 60 above was noise, and the swap's worth to the Tāo Runner is not shown at 48 games a pairing.

**Not done, and why.**
- **A swap with nothing to gain is the jitter's.** A swap within one server, or between two servers shut alike, moves nothing the reading sees and ties with the decline; in the probe above that was 53 of 146 choices, and the jitter takes one of the many tied swaps over the one decline. Recorded, not fixed: nothing measured says such a swap costs either side.
- **Face-down ICE costs nothing to the reading**, as it costs nothing to a run's gate, so a face-down piece swapped in front of a server is read as open.
- **The "may" ahead of an identity's selection of its own cards** stays one ply (above).

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 39. The "may" ahead of an identity's selection of its own cards is planned: Barry "Baz" Wong installs from the grip while the run can still break — and his deck loses games for it, recorded — DONE (`feat/identity-may-planned`, 4 October 2026)

**The last of the identities' owed text** (§38's Open item). §38 planned the "may" ahead of a selection only when its yes selects the other side's installed cards, Tāo Salonga's swap, because the first cut — every identity's "may" ahead of a selection — moved the Startup pass toward the Corp (+15 / −4 over two seeds, z +2.52) and why was not traced. The other five stayed one ply: Haas-Bioroid: Precision Design's add from Archives, Méliès U.'s, Barry "Baz" Wong's install from the grip when the Corp rezzes a piece of ICE, Magdalene Keino-Chemutai's install from her discards and Sebastião Souza Pessoa's connection install. One ply prices a parked selection at its worst resolution, choosing nothing (`fundamentals::pending_decision_upside`), so the yes never beat the no: on `main`, with the planner in both chairs, **Barry took his install 0 times in 596 offers** over 96 games against Agency, Brick Stack, Gimbatul and Hidden Funds.

**What it is now.** `planner::its_identitys_choice` admits a choice whose yes selects any zone the evaluator prices where it leaves the board (`priced_zone`, the list `its_identitys_selection` already used), so the yes is the selection's best line and the no is the no. Barry's yes is priced against the run it comes in: the line stands mid-run once the install is made, and the run's leaf reads what is left to break with. Test: `barry_installs_from_the_grip_while_the_run_can_still_break` — Corroder against a freshly rezzed Ice Wall, Open Market in the grip: installed at 3[c] and 4[c], declined at 2[c] where it would spend the breaking credit, the run successful every time, over four seeds; under §38's rule it is never installed.

**Tracing §38's cost.** Reproduced first, one binary with the widening behind a switch, the same games both ways:
- Startup pass, 90 games, seeds 1–3: **+5 / −1 toward the Corp** (z +1.63), four of the six discordant games Professional Opportunities (Barry) losing — 12 → 8 Runner wins of its 27. Magdalene's deck did not move: her decision was already planned as the turn's end (§35), so the widening adds nothing there.
- Barry against Agency, Brick Stack, Gimbatul and Hidden Funds, 24 games each, seed 2, planner both chairs, paired by game: **Corp 35 → 46 of 96, +21 / −10, z +1.98**; Barry's install 0 → 144 times in 587 offers. The extra Corp wins are mostly agendas (32 → 40; flatlines 3 → 6), steals down 0.3–0.6 a game against Gimbatul and Hidden Funds.

What it is not, each checked on the records:
- **Not the card file.** Barry's `when` is `CardType(Ice(Barrier))`, which matches any piece of ICE (`card_matches_filter` ignores the subtype); §38's "rezzes a barrier" was a misreading.
- **Not the run the install comes in.** After a yes the run succeeds 0.37 of the time, after a no 0.41 (137 and 184 runs); 55% of offers come at ICE that ends the run anyway. A later rez in the same run follows 5 of 142 yeses, so the leaf charging only rezzed ICE is not where the credits go.
- **Not the decision itself, as far as eight seeds can see.** A scratch tool replayed each record to the first planned yes of each game, applied the yes and the no, and played the rest out with the planner in both chairs over eight seeds: **the Corp won 175 of 352 playouts after the yes and 166 after the no** (Gimbatul 107 vs 96, Hidden Funds 68 vs 70) — inside the noise, about ±13. A pass over every yes (up to three a game) was stopped unread.
- **Part of the trajectory is drift.** In seed 6 of the Startup pass the games part at a planned *no*, the answer one ply gave: planning draws its own sample from the seat's generator, so every later sample re-rolls.

What the yeses buy: about 3[c] a game — Maglectric Rapid 41 times (1[c], never used to derez, a blind-list card), Side Hustle 31, Open Market 24, Red Team 15, Madani 14, DZMZ Optimizer 8, Docklands Pass 6, Fransofia Ward 5; hardware installs +0.6–0.9 a game, draws up in two pairings. Each install read in a record is a sound play — Open Market for no click at Flyswatter, which ended the run anyway; Open Market for 2[c] with Ping still broken after — so no rule here excludes one. **Shipped planned, with the cost recorded and owed**: the decision is the identity's printed text taken where it is worth taking, the self-pairing does not move, and the loss is one deck's over a game, not a decision's.

**Measured** (pinned binaries, `main` at `ac3a517`):
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.435 → 0.438** (seed 1, +0.003, z +0.38, 7 discordant) and **0.443 → 0.443** (seed 2, 10 discordant) — 13 of the 17 discordant games Professional Opportunities, 8 to the Corp and 5 to the Runner.
- `diag precepts --deck-styles` (planner both chairs): casual, 192 games, Corp share **0.422 → 0.422** (seed 1, +1 / −1) and **0.427 → 0.438** (seed 2, +3 / −1); startup, seeds 1–3, Corp share 0.567 / 0.556 / 0.522 → 0.589 / 0.578 / 0.522 (+5 / −1, above).

**Not done, and why.**
- **Why Barry's planned install costs his deck over a game** is not traced (Open). The per-decision playout says the first yes is not the mistake; a pass over every yes, or one per card, is the next instrument.
- **Maglectric Rapid** is installed and never used; its derez is a blind-list debt of its own (Open, the blind list).

**Verified.** `cargo test --workspace` green, clippy silent, both 256-seed sweeps green in release.

## 40. Agendas the Corp sends to Archives: rare, and where a card asks it to choose, AU Co.'s search and Ryō's prompt still trash agendas the Corp could have kept — DONE, measurement only (`diag/agendas-to-archives`, 5 October 2026)

**The question** (the person's, 5 October 2026): is the Corp bot discarding agendas to Archives, where a single run steals them? §37 had counted agendas *trashed* (`CardTrashed`) and brought them 68 → 10 over a planner pass of the pool; a discard at hand size is `CardDiscarded`, which it did not count.

**The instrument.** `diag precepts` reads the zone rather than an event: every entry that leaves more agendas in Archives than it found is counted (`corp.agendas_archived`) by route — the Corp's discard at hand size, a Corp action keyed by the card whose selection it confirmed, a Runner action — and as *avoidable* when a discard's HQ, or a selection's offered cards, held something that was not an agenda (a selection with no minimum always is). `corp.archive_choice.mixed.<card>` is the denominator: the Corp's selections into Archives whose offered cards were agendas and not. `runner.steals.archives` counts steals that left Archives with fewer agendas, `corp.agendas_in_archives_at_turn_end` the exposure the Corp ends each turn with.

**Measured** (`main` at `d1d3857`, `diag precepts --deck-styles`, planner both chairs, 192 games):

| | seed 1 | seed 2 |
|---|---|---|
| agendas to Archives | 15 | 15 |
| by a discard at hand size (avoidable) | 1 (0) | 3 (0) |
| steals out of Archives / all steals | 6 / 483 | 8 / 497 |
| agendas in Archives at the Corp's turn end, over Corp turns | 43 / 2,606 | 29 / 2,678 |

Every agenda discarded at hand size was discarded from an HQ of nothing but agendas. The rest were a card's selection, and where the cards offered were agendas and not:

| card | asked | agenda trashed / mixed choices, seeds 1 + 2 |
|---|---|---|
| Hansei Review | the Corp's own action phase, planned | 0 / 41 |
| AU Co.: The Gold Standard in Clones | its turn start, planned since §37 | 8 / 14 |
| Ryō "Phoenix" Ōno: Out of the Ashes | the Runner's turn, one ply | 5 / 17 |
| Longevity Serum | as it is scored, "any number" | 3 / 11 |

(Seed 2 also sent one agenda by an install's trash of the root it replaced and one by a cost's selection.)

**Ryō's is read off the code:** one ply (`planner::one_ply`) applies each legal action and scores the result, and a `ToggleCardSelection` marks a position without moving the card, so every candidate scores alike and `TIE_BREAK_JITTER` chooses — the reason `agent::is_regressive` already gives for skipping a deselect. 5 of 17 is about the share of agendas in those hands. Longevity Serum's three are a choice of "any number", which chooser answered them not traced, followed by "shuffle up to 3 from Archives into R&D", which can put them back; whether it did is not counted. **AU Co.'s is not traced:** its search has been a step of the turn's line since §37, so the line scores the search where it leaves the board. The cases it chose (Sericulture Expansion trashed over Phật Gioan Baotixita, Offworld Office over Spin Doctor or Byte!, Orbital Superiority over Anoetic Void with Offworld Office kept) show the evaluator preferring the other card at a cost of half the agenda's points; the Corp's evaluator gives an agenda held in HQ no worth of its own, which is the lead, unmeasured. **Corrected by §42: the lead was wrong.** The search was decided on cards the Corp was not looking at — the sample's guesses at the top of R&D, through a plan made at the offer and followed into the selection — not by the evaluator's reading of the real ones.

**Not done, and why.** Both are Open: the one-ply selection, first, since it is any one-card selection either chair answers out of its turn; AU Co., traced before any weight moves.

## 41. One ply scores a card selection where its confirm leaves the board: Ryō's prompt and Longevity Serum stop trashing agendas the Corp could keep — DONE (`fix/one-ply-scores-the-confirmed-selection`, 5 October 2026)

**The first of §40's two findings.** One ply (`planner::one_ply`, what the planner plays wherever it does not plan: a run, a prompt, the other side's turn) applies each legal action and scores the result. A `ToggleCardSelection` marks a position and moves no card, so every candidate of a selection scored alike and `TIE_BREAK_JITTER` chose: Ryō "Phoenix" Ōno's "the Corp trashes 1 card from HQ", answered in the Runner's turn, sent an agenda to Archives 5 of 17 times HQ held something else.

**What it is now.** A toggle is scored where the selection leaves the board once confirmed: one ply applies `ConfirmCardSelection` after the toggle and scores that state when the confirm applies. That is exact for one card, and greedy for "up to" or "any number", whose Confirm is already a candidate a toggle has to beat. A selection still owed two or more cards is scored as before. The beam's own lines are not one ply's: Hansei Review, played inside a planned turn, already trashed no agenda (0 / 41, §40). **Rejected:** treating every one-card selection as `whole_sets` does (§38). That lives in the beam and is asked only while a seat decides for its own identity, and Ryō's prompt is answered by the seat it is *not* the identity of.

Test: `one_ply_keeps_the_agenda_a_one_card_selection_could_trash` (Hansei Review's trash out of an agenda and two Hedge Funds keeps the agenda on 16 seeds; with the confirm switched off, seed 1 trashes it).

**Measured** (pinned binaries, `main` at `1c4c134` against `bcac4d3`):
- `coverage_identical.py`: both random shapes identical (`66ec5f5d…`); planner shapes `edea6abe…` → `87cce421…`.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.458 → 0.469** (seed 1, +0.010, z +0.60, 44 discordant) and **0.453 → 0.469** (seed 2, +0.016, z +0.97, 38 discordant). Toward the Corp on both seeds, inside the band. One ply is both chairs', so this is not a Corp-only lever.
- `diag precepts --deck-styles`, planner both chairs, 192 games, seeds 1 / 2:

  | | before | after |
  |---|---|---|
  | agendas to Archives | 17 / 16 | **10 / 11** |
  | Ryō's prompt, an agenda trashed / mixed choices | 3 of 7 / 4 of 11 | **0 of 8 / 0 of 8** |
  | Longevity Serum, the same | 3 of 6 / 0 of 4 | **0 of 6 / 0 of 3** |
  | AU Co.'s search, the same (planned, untouched) | 5 of 9 / 2 of 4 | 5 of 10 / 3 of 5 |
  | steals out of Archives / all steals | 5 / 482, 8 / 499 | 4 / 484, 6 / 493 |
  | Corp scores | 346 / 326 | 356 / 331 |
  | Corp wins | 87 / 85 | 87 / 87 |

**Not done, and why.** AU Co.'s search is §40's other finding and stays Open: planned, so not this tie.

**Verified.** `cargo test --workspace` green (2,743), clippy silent, both 256-seed sweeps green in release.

## 42. A selection is decided on the cards it shows: AU Co.'s search keeps the agendas it looks at — DONE (`fix/au-co-search`, 5 October 2026)

**The second of §40's findings.** AU Co.'s "look at the top 3 cards of R&D. Trash 1 of those cards" trashed an agenda 8 times in 14 where the three held something else, though the search has been planned since §37. Traced, it was not the evaluator (§40's lead, corrected there):

- **The sample guessed the cards.** The Corp's view names the three cards and their positions (`ClientView::selection`), and `determinize` drew all of R&D from the pool and never read that list, so whatever stood at those positions was a guess.
- **The guess was followed.** The line is planned at AU Co.'s offer, before the Corp has looked, and `PlanningAgent::follow` plays a standing line while the view's legal actions are the ones predicted. A toggle is a position, the same in the sample and at the table, so the plan's trash was played on the real cards without a look.

**What it is now.** `determinize::seat_selection` puts each card a parked selection shows its chooser out of a zone they cannot see (`CardZoneRef::shows_the_chooser_hidden_cards`: their own R&D or stack, the other side's hand or deck) at the position the view names it by, swapping in a copy from elsewhere in the zone where there is one so the counts hold, and writing over the guess where there is none, as `seat_revealed` does. And `follow` stops when such a selection shows cards the line was not made with (`Plan::shown`), so the seat plans again on them. **Both are needed:** with either alone the new test trashes the agenda 4 times in 7, as with neither. This also closes the residual `a_parked_selections_targets_stay_addressable_after_determinization` recorded since the selection became positional: an offered position now holds the card shown there, and the test asserts it.

Tests: `au_cos_search_trashes_a_card_it_looked_at_and_keeps_the_agenda` (an agenda and two cards that are not on top of R&D, twelve seeds, the agenda never trashed in the 7 that take the search), and the determinize test above. **§37's `takes_au_cos_search_at_its_turn_start_and_keeps_the_agenda` never offered the agenda:** its Offworld Office is fourth from the top of R&D, out of the search's reach and into HQ by the turn's draw, so it held the agenda kept whatever was trashed. It stays, for what it does show: the search is taken.

**Measured** (pinned binaries, `main` at `d2321e0` against `7c0692b`):
- `coverage_identical.py`: both random shapes identical (`66ec5f5d…`); planner shapes `87cce421…` → `69e4ca20…`.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.469 → 0.477** (seed 1, +0.008, z +0.48, 39 discordant) and **0.469 → 0.482** (seed 2, +0.013, z +0.76, 43 discordant), toward the Corp on both seeds and inside the band. `seat_selection` serves both chairs, so this is not a Corp-only lever.
- `diag precepts --deck-styles`, planner both chairs, 192 games, seeds 1 / 2:

  | | before | after |
  |---|---|---|
  | AU Co.'s search, agenda trashed / mixed choices | 5 of 10 / 3 of 5 | **0 of 4 / 0 of 8** |
  | agendas to Archives | 10 / 11 | **6 / 9** |
  | of them avoidable | 5 / 3 | **0 / 0** |
  | steals out of Archives / all steals | 4 / 484, 6 / 493 | **0 / 481, 5 / 488** |
  | Corp scores | 356 / 331 | 347 / 332 |
  | Corp wins | 87 / 87 | 88 / 89 |

  What still reaches Archives is forced: Ryō "Phoenix" Ōno's prompt (3 / 6) and Hansei Review's from an HQ of agendas, a discard at hand size from one (1 / 1), and an install's trash of the root it replaced (1 / 1).

**Verified.** `cargo test --workspace` green (2,744), clippy silent, both 256-seed sweeps green in release.


## 43. A paid end-the-run waits for the run's last window and pays only for what the breach would take: LEO Construction ends 1 run at its initiation where it ended 20 — DONE (`feat/a-paid-end-the-run-waits-for-the-threat`, 5 October 2026)

**The debt §33 left.** LEO Construction's "trash 1 rezzed bioroid card in the root of or protecting the attacked server: end the run" is used by one ply, which compares the board after the use with the board after a pass. Ending a run gave back the whole run term — `ACTIVE_RUN_AGAINST_WEIGHT` (1.5), the stakes and what the identities print about it — whatever the run would have reached, so the Corp paid the first time it could. Traced on `main` at `dc1ccb1` (Agency against Stolen Goods and Tickets Please, seed 2, 48 planner games each): **28 uses, 20 of them at the run's initiation**, before the Runner had approached any ICE that might have stopped them; half for less than a point of score, and half paying with Mercia B4LL4RD.

**What it is now — three readings, each found by tracing the uses the one before left:**

- **A held end-the-run answers the run until its last window** (`eval::corp::answered_by_a_held_end_the_run`). While the Corp holds a usable paid ability whose effect ends the run — LEO's, Event Horizon's and M.I.C.'s (The Red Room's counter and B-1001's tag are costs no reading prices, and answer nothing); "usable" asked of the engine through the new `rules::ability_is_usable`, the per-ability question `has_usable_paid_ability` already put — the run costs no more than the cheapest such ability's price (`end_the_run_cost`: a card it trashes at its install value, the cheapest the engine would let it choose, credits at the credit weight) less `HELD_END_THE_RUN_EDGE` (0.05), up to and including the run's last window (CR 6.9.4e, which always opens). So paying before then is worse than waiting by the edge. **Once the run is past that window, it costs what its breach reaches** — the flat term is given back, the stakes and the identities' pay are not — so at the last window the Corp pays exactly when the breach would take more than the card. The first version charged the run whole at the last window, and the flat term alone then outbid Mercia (2.0) against an R&D access worth 0.7: 39 uses, 26 at the last window, 19 of those on R&D and HQ.
- **A turn trigger that installs from HQ is read** (`eval::read::turn_installs`): Mercia's "when your action phase ends, you may install 1 piece of ice from HQ, paying 1[credit] less" and Warm Reception's "when your turn begins, you may install 1 card from HQ" save a click and the discount each turn, for as many turns as HQ holds a card the trigger may choose, at `future_credit_weight`. `declared_income` reads what a card pays, and an install is not a payment, so Mercia was worth what any rezzed upgrade is. (The card file's `{"Ice": "Barrier"}` matches any ice, which is what Mercia prints.)
- **The fort and the taxing window are read between runs** (`eval::corp::between_runs`): what a run lends the rig lasts the run — a breaker pumped for the encounter, the credits a run event brought — and read during one, the rig beat the "glacier, then fast advance" wall for as long as the run went on. At one LEO offer in an Overclock run with Carmen pumped +3 against Ansel 1.0 the fort terms were off (13.0 of score) until the run ended, so ending it read as rebuilding the fort and the Corp traded Ansel for a run worth nothing like it. `fort_beaten` and `agendas_under_the_window` are now asked of the board with the run taken off it.

Tests: `a_held_end_the_run_answers_the_run_until_its_breach` (LEO and Mercia in a remote with and without an agenda, at each phase and past the last window, and with no bioroid to trash), `a_turn_trigger_that_installs_from_hq_saves_a_click_and_its_discount`, `a_pump_that_lasts_the_run_does_not_beat_the_fort` (fails with the old reading, 15.4 against 8.4).

**Measured** (pinned binaries, `main` at `dc1ccb1` against `85a239d`):
- The two Agency pairings above, 96 games: LEO used **28 → 17** times, **20 → 1** at a run's initiation, **1 → 9** at the last window; Corp wins 27 → 30. On the same games a scratch build that never uses LEO wins 25, so **§33's finding — that the Corp's own LEO use was its loss (18 + 15 → 8 + 10 then) — no longer reproduces on `main`** (25 never using it, 27 using it), corrected here. One standard deviation on 96 games is about 4.5, so no win effect is claimed. Each reading off alone, on the version before the taxing window was read between runs (all on: 18 uses, Corp 29): without the held answer, 24 uses in Stolen Goods' 48 games, 13 at the initiation (its Tickets Please pass did not finish — below); without Mercia's reading, 16 uses and Corp 24; without the fort read between runs, 22 uses and Corp 28.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.477 → 0.469** (seed 1, −0.008, z −0.73, 17 discordant) and **0.482 → 0.484** (seed 2, +0.003, z +0.45, 5 discordant), inside the band. **Every changed game is a Mercia deck**: Agency 16 → 13 Corp wins of 48, Brutal Efficiency 17 → 15, Fashion Lab 12 → 15. The glacier-then-fast-advance decks without Mercia (Brick Stack, Discretion Advised) and the Sweep decks holding the other paid end-the-runs played the same games.
- `diag precepts --deck-styles` (planner both chairs): casual 192 games 0.458 / 0.464 → 0.464 / 0.469 (+5 / −3); startup 90 games identical game for game (no Startup deck holds these cards).
- `coverage_identical.py main`: both random shapes identical (`66ec5f5d…`), so the engine's new `ability_is_usable` moved nothing; both planner shapes moved, to one hash (`69e4ca20…` → `1acab6b9…`). Over that planner pass LEO Construction was used 9 → 3 times and Mercia B4LL4RD rezzed 21 → 26 times, her install offered 25 → 30.

**Found on the way, owed (Phase 5 Open):** 4 of the branch's 17 uses are one-ply ties — passing parks a choice of the Corp's own (Brân 1.0's "you may install 1 piece of ice"), which one ply charges `unresolved_decision_weight` (2.0) ahead of its upside, the price of a Mercia with no ice in HQ; trashing Mercia out of a remote's root reads as gaining a fort (+3.0), because `fort_remote` takes a root holding an upgrade for no fort; and trashing Brân 1.0 once read as +3.5 because the beaten wall turned off fort terms that were net negative (an exposed agenda). And the planner's `answer_payment` tries every order of an install's trash-first picks, so a server with ten pieces of ice is 11¹⁰ orders: the scratch build without the held answer piled ten on Archives and its game never finished.

**Verified.** `cargo test --workspace` green (2,747), clippy silent, both 256-seed sweeps green in release.


## 44. One ply answers a decision of the seat's own before it scores the action that parked it: the Corp takes the free rezzes and installs a "may" offers it — DONE (`fix/one-ply-looks-through-its-own-choice`, 5 October 2026)

**The debt §43 left.** Passing into Brân 1.0's subroutines parks "you may install 1 piece of ice from HQ or Archives" on the Corp ahead of its two "end the run", and one ply scored the pass there: `evaluate_state_with` charges a parked decision of the side's own `unresolved_decision_weight` (2.0) and credits only a lower bound on what resolving it delivers (`pending_decision_upside`), which for an install is nothing. So the pass cost 2.0 with the ETRs still waiting, LEO Construction's trash of a Mercia B4LL4RD with no ice in HQ cost the same, and the jitter chose. Traced on `main` at `eef2711` (Agency against Stolen Goods and Tickets Please, seed 2, 48 planner games each): of LEO's 17 uses, **6 were at a pass that parked a Corp choice and 5 of those were exact ties** — Brân's install, a subroutine's "trash 1 program" whose program the Corp picks, an advancement placed by the Corp's choice.

**What it is now** (`planner::one_ply_score`): an action that leaves a decision parked on the seat itself is scored by the seat's best answer to it — each of its legal answers applied and scored the same way, up to `OWN_DECISIONS_LOOKED_THROUGH` (2) decisions deep, so a "may" whose yes is a selection is answered through both — rather than where it stands. The charge stays in the evaluator, where it keeps a selection from walking: a toggle that leaves its selection open is not looked through, since that is the walk and every order of its cards would be priced, and one ply's §41 rule (a toggle scored where its confirm leaves the board) is the same function's. It is the seat's own answer by its own evaluator, so what it credits is what the seat would then play, not a bound; the beam, which plays its own lines through its decisions, is untouched.

**It reached much further than LEO.** Over a planner pass of the pool (192 games, seed 1, instrumented and replayed identically), the look-through strictly changed **360 one-ply choices**, and most were the Corp's "you may" declined because the yes parks a selection: Send a Message's free rez when an agenda is scored or stolen (57; its selection offered 25 → 76 times), Brân 1.0's install (35), Scatter Field (25), Plutus (14), Ballista (11), Ansel 1.0 (10), Humanoid Resources (7), Mercia's install (5), Mycoweb and Mitra Aman (5 each), Top-Down Solutions and Peer Review (4 each), Phát Gioan Baotixita's offer (4); the Corp also rezzed at an approach 11 times and used a paid end-the-run 13 times at an encounter where it had not. The Runner's were mostly a pass at an encounter whose subroutine parks the Runner's choice (68) and Pantograph's (19).

Test: `one_ply_lets_the_ice_end_the_run_rather_than_pay_to_end_it` (LEO, Mercia in HQ's root, Brân 1.0 rezzed in front of it and a Runner who breaks nothing, sixteen seeds: the ice ends the run and Mercia is kept; with the look-through off, seed 0 trashes her).

**Measured** (pinned binaries, `main` at `eef2711` against `4e801b1`):
- The two Agency pairings above, 96 games: LEO used **17 → 15** times, at a pass that parked a Corp choice **6 → 0**, exact ties **5 → 0**; Corp wins 30 → 34 (no win effect claimed on 96 games).
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.469 → 0.521** (seed 1, +0.052, z +2.21, 82 discordant) and **0.484 → 0.562** (seed 2, +0.078, z +3.44, 76 discordant) — **past the band, toward the Corp on both seeds, and the Corp seat's**: a scratch build that looks through the Runner's decisions alone moved seed 1 −0.008 (z −0.58, 27 discordant), and one that looks through the Corp's alone moved it +0.062 (0.531, z +2.87, 70 discordant). By the Corp deck's style, fast-advance moved most and on both seeds (144 games: +0.083, +0.097); balanced fell on seed 1 and rose on seed 2 (72 games: −0.028, +0.097); glacier + traps did not move (24 games, 0–2 discordant).
- `diag precepts --deck-styles` (planner both chairs): casual 192 games 0.464 / 0.469 → **0.521 / 0.573** (+26 / −15, +28 / −8); startup 90 games 0.556 / 0.533 → **0.678 / 0.644** (+18 / −7, +14 / −4); over all four, +86 / −34, z +4.7.
- `coverage_identical.py main`: both random shapes identical (`66ec5f5d…`); both planner shapes moved, to one hash (`1acab6b9…` → `6b55c35c…`), the Corp's wins 89 → 100 of 192.

**What it leaves.** A stronger Corp at every rung moves the self-pairing the ladders were spaced on (§25 Stage 8, §22), and they are not re-taken here: the handicaps are `epsilon` over the same planner, so every rung gained, and whether the steps are still even is a measurement owed before a ladder claim is made of them.

**Verified.** `cargo test --workspace` green (2,748), clippy silent, both 256-seed sweeps green in release.


## 45. The fort is read so the Corp cannot rebuild it by trashing its own cards: an upgrade sits in a fort's root, and the wall is beaten by the Runner's rig, not by a piece count — DONE (`fix/fort-root-holds-an-upgrade`, 5 October 2026)

**The debt §43 left**, two readings by which trashing one of the Corp's own cards scored as building its fort. Traced on `main` at `69b1ecc` (Agency against Stolen Goods and Tickets Please, seed 2, 48 planner games each; a temporary trace of which card each LEO Construction use trashed, and a per-weight bisection of the use against the pass): LEO was used 15 times, **8 of them a Mercia B4LL4RD out of the root of a remote holding an agenda**, and 2 a piece of ICE at a run's initiation.

**What it is now:**

- **An upgrade is what a fort's root holds beside its agenda** (`eval::corp::fort_remote`). A root counted as the fort only when everything in it was what the fort is for — an agenda, an unseen lure trap, the traps plan's bluff — so a remote with Project Ingatan and Mercia was no fort, and the trash of the Mercia read as gaining one: two pieces at `fort_weight`, +3.0.
- **The "glacier, then fast advance" wall is beaten by the Runner's side alone** (`eval::read::fort_beaten`): their rig breaks every subtype of ICE, and their credits cover the break into the fort (`taxing_cost`, nothing with no fort). It was the rig covering the pieces the fort has once there were at least the fort's depth of them, so the count was the Corp's to move: **one piece too few was no wall anyone had beaten, and the whole of the fort terms came back.** The bisection put LEO's trash of its own Bumi 1.0 at a run's initiation at +10.25 over the pass, all of it the switch (`fort_until_beaten` +13.0, central ICE +10.0 of it), and Brân 1.0's at an approach at +2.0, the switch +13.0 of it against what the trash cost; the reverse is that adding the piece that made the depth read as losing the fort. Every subtype is what the Corp could still put in front of the rig, and the credits are the one part the wall moves, in one direction only. The depth rule was Stage 6's answer to a remote with no ICE read as "covered" the turn it was made, which a rig that breaks every subtype is not.
- **What falls away with a beaten wall is the fort's worth, not an agenda's exposure** (`eval::corp::exposure`). The whole of `fort_value` was dropped, so a fort whose terms were net negative — an exposed agenda outweighing the ICE — read beating it as a gain, which is how §43 saw a trash of Brân 1.0 score +3.5. **This corrects Stage 6's design**, which dropped the exposure on purpose ("the naked agenda stops paying once the rig beats the wall", its test): an agenda short of the wall is no safer for the Runner having beaten it, and the never-advance line still prices one the Corp cannot finish. The test now says the opposite and why.

Tests: `a_root_holding_an_upgrade_is_still_the_fort` (fails without the first reading), and `the_fort_falls_away_once_the_runner_beats_the_wall_when_fast_advance_follows_glacier` rewritten — a barrier-only rig has not beaten a Corp that can still ice a code gate, a piece fewer does not unbeat the wall and scores less, the exposure stays; `a_pump_that_lasts_the_run_does_not_beat_the_fort` given a whole rig.

**The view sweep found a false positive in its own fog rule.** At 256 seeds, seed 152 (Hostile Bid against Burn Rate) failed `no_client_view_or_log_entry_ever_names_a_card_it_conceals`: Wall to Wall's "resolve up to 3 in any order" took "add this asset to HQ" as one entry and "gain 1[credit]" as the next, whose `AbilityGainedCredits` names the asset now in HQ. Both chairs watched the faceup asset go there; the rule already counted such a card as visible within the entry that moved it, and now carries it across the turn's later entries. No masking moved.

**Measured** (pinned binaries, `main` at `69b1ecc` against `683665e`):
- The two Agency pairings above, 96 games: LEO used **15 → 21** times, a piece of ICE at a run's initiation **2 → 1**, Mercia out of an agenda's remote 8 → 16 (all at the run's last window, the breach's agenda against the upgrade); Corp wins 34 → 35. The one use left at an initiation is a different reading, owed: Ansel 1.0 off HQ, +1.23 over the pass, of which +4.0 is future income — HQ left without ICE reads the game as early again (`stage`), and the horizon every declared income is counted over grows.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.521 → 0.497** (seed 1, −0.023, z −1.00, 81 discordant) and **0.562 → 0.568** (seed 2, +0.005, z +0.25, 64 discordant), inside the band. The "glacier, then fast advance" decks, the only ones the beaten wall reaches, +0.028 / −0.083 (72 games, z +0.53 / −1.73).
- `diag precepts --deck-styles` (planner both chairs): casual 192 games 0.521 / 0.573 → 0.469 / 0.573; startup 90 games 0.678 / 0.644 → 0.689 / 0.656; over all four +51 / −59, z −0.76.
- `coverage_identical.py main`: both random shapes identical (`66ec5f5d…`); both planner shapes moved, to one hash (`6b55c35c…` → `dc7c4489…`), Mercia installed 36 → 44 and rezzed 26 → 31 times.

**Verified.** `cargo test --workspace` green (2,749), clippy silent, both 256-seed sweeps green in release.


## 46. A game does not get younger: past the turn nine games in ten have left the early stage, a stripped board is read as the middle game — DONE (`fix/stage-not-moved-by-own-ice`, 6 October 2026)

**The debt §45 left.** One of LEO Construction's 21 uses in 96 Agency games was at a run's initiation: game turn 26, a run on HQ, and the cost the trash of HQ's only piece of ICE, Ansel 1.0. The bisection put it +1.23 over the pass, +4.0 of it future income. `eval::stage` read the board — middle once HQ, R&D and a remote have ICE, otherwise early — so the bare HQ read the game as early again, the horizon every declared income is counted over went 5 → 9, and the Corp's economy cards were worth more for the trash. **The person asked why the Corp stripped the ICE and did not put it back.** A temporary trace of the Corp's turns after it: no ICE in HQ and 0–4 credits at the start of each of its next three turns, so there was nothing to put back; one piece arrived on the fourth, and the game ended before HQ was iced again. The stripping was the bad move, and the misreading decided it.

**How often a game goes back, measured first** (a temporary trace of every chosen move that lowers the stage, and of the stage at each Corp turn's start, over a planner pass of the pool, 192 games, seed 1): **no game started a Corp turn back in the early stage once it had left it**, and three chosen moves in the pass lowered the stage, none of them the Corp trashing its own ICE: two Corp selections in a late game, which removed no ICE, and a Runner's steal. Games left the early stage by game turn 11 at the median, 13 at three in four and 19 at nine in ten; 17 ended in it. So the reversal is a one-move misreading, not something the game does.

**What it is now** (`eval::stage::EARLY_STAGE_ENDS`, 19): from that game turn the stage is at least middle, whatever the board. It is the measured nine-in-ten point, so it moves almost nothing a game's own board reaches first: the reading of a board stripped late, and of the one game in ten still building. The precepts report reads the same function, and its early stage is 0.3 of a Corp turn shorter a game (5.21 → 4.91 in the casual pass, seed 1).

Test: `a_stripped_board_late_in_a_game_is_not_early` (HQ bare is early on turn 18 and middle on turn 19, with a shorter horizon).

**Measured** (pinned binaries, `main` at `82f77e8` against `051253b`):
- The two Agency pairings, 96 games: LEO used 21 → 24 times, **every one Mercia B4LL4RD at a run's last window**; ICE trashed **1 → 0**, uses at an initiation **1 → 0**; Corp wins 35 → 36.
- Planner self-paired, `--deck-styles`, 384 games: Corp share **0.497 → 0.497** (seed 1, 6 discordant) and **0.568 → 0.581** (seed 2, +0.013, z +2.24, 5 discordant) — inside the band; a reading that changes late, stripped boards changes few games.
- `diag precepts --deck-styles` (planner both chairs): casual 0.469 / 0.573 → 0.474 / 0.589, startup 0.689 / 0.656 → 0.689 / 0.667; +6 / −1 over the four.
- `coverage_identical.py main`: both random shapes identical (`66ec5f5d…`); both planner shapes moved, to one hash (`dc7c4489…` → `2c430dfb…`).

**Verified.** `cargo test --workspace` green (2,750), clippy silent, both 256-seed sweeps green in release.


## 47. The ladders after §44: the Corp's steps are still even, the Runner's are not, and the trash-first payment search is exponential in the ice on the server — DONE, measurement only (`diag/ladders-after-44`, 6 October 2026)

**The question §44 left.** One ply took the free rezzes and installs a "may" offers the Corp, which moved the planner's self-pairing +0.052 / +0.078 toward the Corp, and every rung is that planner at an `epsilon`. The steps were spaced on the bot before it (§25 Stage 8), and the pool has grown since (the Standard tournament lists, tranche 8's first Runner cards).

**The ladders, re-taken** the way Stage 8 took them: the full 5 × 5 per chair, 384 games a cell on each of two seeds, every seat in its deck's own style, each rung against the un-handicapped planner on the other chair (`bench --bots level:novice,level:apprentice,level:operator,level:veteran,level:elite --games 384 --deck-styles`, `ladder_report.py --reference level:elite`), on a pinned binary of `main` at `0a1cb81`. Reports in `target/coverage/ladder44/`.

| rung (ε Corp / Runner) | Corp, seed 1 / 2 | Runner, seed 1 / 2 | Stage 8 Corp | Stage 8 Runner |
|---|---|---|---|---|
| novice (1.00 / 1.00) | 0.013 / 0.016 | 0.036 / 0.034 | 0.018 / 0.008 | 0.034 / 0.039 |
| apprentice (0.22 / 0.45) | 0.156 / 0.128 | 0.107 / 0.094 | 0.143 / 0.122 | 0.188 / 0.188 |
| operator (0.11 / 0.25) | 0.273 / 0.284 | 0.188 / 0.206 | 0.224 / 0.224 | 0.344 / 0.320 |
| veteran (0.05 / 0.10) | 0.440 / 0.440 | 0.333 / 0.396 | 0.315 / 0.328 | 0.435 / 0.424 |
| elite (0.00 / 0.00) | **0.570 / 0.547** | **0.430 / 0.453** | 0.401 / 0.469 | 0.599 / 0.531 |

- **The reference moved toward the Corp:** elite against elite, the Corp wins 0.570 / 0.547 where it won 0.401 / 0.469. What moved it is not measured here — §44's stronger Corp, §45 and §46, and the pool's growth all fall between the two tables — and no cause is claimed.
- **The Corp's ladder is still even:** steps +0.143 / +0.117 / +0.167 / +0.130 (seed 1) and +0.112 / +0.156 / +0.156 / +0.107 (seed 2), every one a rise at 3.0 sd or more and within 0.05 of an even step on its seed. §21's handicaps stand.
- **The Runner's is not:** steps +0.070 / +0.081 / **+0.146** / +0.096 and +0.060 / +0.112 / **+0.190** / **+0.057**. `operator → veteran` is 0.05 and 0.09 above an even step, and `veteran → elite` on seed 2 is +0.057 at sd 0.036 — 1.6 sd, short of Stage 8's 2.6. Interpolating each seed's measured curve for four even steps gives ε ≈ 0.38 / 0.20 / 0.10 (seed 1) and 0.37 / 0.22 / 0.14 (seed 2): **about 0.37 / 0.21 / 0.12** in place of 0.45 / 0.25 / 0.10. The re-spacing, and its re-take, is owed (Open).

**Cost.** A seed of the 5 × 5 took 3,754 s alone on 20 threads (seed 2), where Stage 8's took 1,866–2,191 s; seed 1's 6,307 s shared the machine with the measurement below. Not attributed.

**The trash-first payment search, measured** (§43's note: "ten pieces of ice on a server is 11¹⁰ orders, and a scratch build's game never finished"). A Corp ICE install that trashes first over a server holding *n* pieces is answered one pick at a time (`payment::Ask::Install`), and both the planner's `answer_payment` and the evaluator's `fundamentals::through_parked_payment` walk every order of every subset. A scratch test (not committed) parked the payment over *n* walls on HQ and timed both, and one whole `select_action` from the action phase:

| ICE on the server | applications | `answer_payment` | evaluator | `select_action` |
|---|---|---|---|---|
| 5 | 530 | 0.05 s | 0.12 s | 0.30 s |
| 6 | 3,192 | 0.50 s | 0.35 s | 0.35 s |
| 7 | 22,358 | 3.4 s | 3.9 s | 3.1 s |
| 8 | 178,880 | 30 s | 30 s | 29 s |
| 9 | 1,609,938 | 293 s | 287 s | 306 s |
| 10 | 16,099,400 | ≈ 49 min, not run | | |

The count is exact — a(n) = n·a(n−1) + 2n, every subset in every order — and the ninth row was predicted before it was run; the tenth is that count at the measured ≈ 180 µs an application. `PLAN_BUDGET` (2,500) does not bound it: the budget is checked between the beam's nodes, and the payment's recursion is inside one.

**How far real play gets** (a scratch counter on every first trash-first pick, the same binary otherwise; `diag precepts --deck-styles`, 391 planner games over the Casual pool, seed 1, sharing the machine with the ladder): **324,112 questions**, by candidates 2: 250,000, 3: 56,124, 4: 13,491, 5: 4,445, 6: 46, **7: 6** — the six at 10.6–17.1 s each; none at 8 or more, so no game stalled. All but 55 were the planner's own `answer_payment` (the evaluator's 55 were at 5 candidates or fewer). Answering them took **2,737 of about 10,400 thread-seconds, about a quarter of the pass**, most of it in the small questions (2 candidates: 829 s).

**Verified.** No code changed; the tree is `main`'s.


## 48. An install's trash picks are searched as sets, not in every order: ten pieces of ice on a server is 2,045 applications where it was 16,099,400 — DONE (`fix/trash-picks-searched-as-sets`, 7 October 2026)

**The debt §43 left and §47 measured.** An install that trashes first is asked one pick at a time (`payment::Ask::Install`), because the trashes are made in the order picked (CR 8.5.7) and a person chooses that order. The planner's `answer_payment` and the evaluator's `fundamentals::through_parked_payment` took the question as asked and walked every order of every subset: a(n) = n·a(n−1) + 2n applications for *n* pieces of ice, 293 s at nine, about 49 minutes at ten, outside `PLAN_BUDGET` — and about a quarter of a planner pass's thread time spent on the small questions.

**What it is now** (`eval::fundamentals::{searched_answers, picked_before}`, both searches): within one install's picks, a candidate is tried only at a higher position than the last one picked, so each subset is tried once, in ascending order — 2ⁿ rather than about n!·e. A question is the same install's next pick when it names the same card and offers what was left once the pick went; any other question, and the first after "no more", is searched whole, so a second install in the same action is not narrowed by the first. "No more" is always tried. The engine is unchanged: a person still picks from the full list, in their own order.

**Rejected.** Narrowing the engine's question to ascending positions: the order is the player's (CR 8.5.7), and a client would have lost it. A greedy pick — the best single card, then whether another helps, O(n²): sets are exact and 2ⁿ is small at the depths play reaches (seven in §47's pass), and a greedy answer is a reading of the board the evaluator was never asked to make. What a search gives up is only the order the same cards reach Archives, which no reading of a board prices.

Tests: `the_payment_search_tries_each_set_of_trash_picks_once` (ten walls: exactly 1,023 picks and 1,022 "no more"s, 0.26 s, and the chain found pays — the same search took about 49 minutes before), `the_evaluator_prices_a_trash_first_install_by_its_best_set` (over four walls the evaluator's price is the best of all fifteen subsets).

**Measured** (pinned binaries, `main` at `0a1cb81` against this branch; each run alone on 20 threads):

| | before | after |
|---|---|---|
| planner self-paired, 384 games, seed 1 | 283 s, Corp 0.500 | **247 s**, Corp 0.534 (+0.034, z +1.22, 113 discordant) |
| planner self-paired, 384 games, seed 2 | 390 s, Corp 0.576 | **257 s**, Corp 0.570 (−0.005, z −0.20, 98 discordant) |
| `diag precepts --deck-styles`, casual 391 games | 310 s, Corp 0.504 | **259 s**, Corp 0.537 (z +1.21 paired, 51 / 64) |
| `diag precepts --deck-styles`, startup 90 games | 73 s, Corp 0.689 | **64 s**, Corp 0.600 (z −1.51 paired, 18 / 10) |

Every run is faster. The games are not the same games — 15 of 391 casual records and 4 of 90 startup are identical — because the search no longer makes the same jitter draws, so the timings compare passes, not games. No win effect is claimed: the four paired shifts go both ways and none is beyond 1.6 sd, inside the band.

**Found on the way, fixed separately** (`fix/trigger-order-masked`, #376): the new trajectories reached a fog leak on `main` — `GameEvent::TriggerOrderChosen` passed the log mask unmasked, so the 256-seed view sweep's seed 92 named Lycian Multi-Munition to a spectator in the entry where its own trigger derezzed it. It is now withheld from whoever its card is concealed from, as `TriggerFired` is.

**Verified.** On the masking fix: `cargo test --workspace` green (2,839, the desktop crate included), clippy silent, both 256-seed sweeps green in release. Rebased onto tranche 8 Stages 2 and 3 (#373, #375): both 256-seed sweeps, `netrunner_bots`' tests and clippy again green. The timings above were taken before that rebase, on `0a1cb81`.

## 49. Barry "Baz" Wong's install costs his deck nothing: §39's loss was one seed's drift, and since §44 one ply takes the same yes — DONE, measurement only (`diag/barry-install-costs-nothing`, 7 October 2026)

**The question §39 left.** Planning the "may" ahead of Barry's install (`planner::its_identitys_choice`) took the install 0 → 144 times in 96 games and, on the record, cost Professional Opportunities games — Corp 35 → 46 of 96 against Agency, Brick Stack, Gimbatul and Hidden Funds on seed 2, z +1.98, with the Startup pass +5 / −1 — while the decision itself, replayed and played out both ways over eight seeds, was not worse (Corp 175 vs 166 of 352 playouts). Why the deck lost over a game was not traced, and the entry was Open.

**Re-measured, three times the sample.** One binary of `main` at `9afa52af` with a scratch gate behind an environment variable (never committed); the same four pairings, **24 games each on seeds 1, 2 and 3 — 288 games an arm**, every game its own process (`--headless --games 1 --seed <seed·1000 + index> --verbose --record`), paired by seed and index, the planner in both chairs in the decks' own styles:

| arm | Corp wins of 288 | discordant | z | per seed (96 each) |
|---|---|---|---|---|
| the yes planned (`main`) | 163 | — | — | 54 / 54 / 55 |
| the yes left to one ply (`its_identitys_choice` gated off) | 160 | +47 / −44 | +0.31 | 53 / 52 / 55 |
| **the yes declined every time** | **166** | **+44 / −47** | **−0.31** | 61 / 52 / 53 (z −1.35 / +0.33 / +0.38) |

By pairing, the yes against the decline: Agency 25 / 25, Brick Stack 43 / 49, Gimbatul 45 / 39, Hidden Funds 50 / 53 — the largest cell 1.41 sd, in the Runner's favour on one and the Corp's on another. **The yes costs the deck nothing a test of this size can see, and §39's number was drift at one seed**: a z of 2 at 96 games is one measurement in twenty, and this phase takes many.

**What the measurement also showed.**
- **§44 subsumed §39 for Barry.** With `its_identitys_choice` gated off, one ply took the yes 465 times in 1,138 offers — the planned arm 473 in 1,128 — because §44's look-through of a seat's own parked decisions answers the selection the yes leads into. Before §44, one ply priced that selection at its worst and declined every time (0 of 596), which is what §39 was built for. The rule still exists for the other identities' choices; whether it still decides any of them is not measured.
- **The yes buys installs, not credits.** The Runner installs 1.48 hardware and 2.84 resources a game with the yes against 0.72 and 2.28 without, programs 2.52 either way; gains 36.1 and spends 37.6 credits a game against 34.1 and 36.1; runs 21.4 against 22.3, with 12.0 and 12.4 successful; steals 2.32 against 2.35. The yes installs Maglectric Rapid 153 times (clicked 2), Side Hustle 104 (clicked 18), Open Market 66, Madani 51, Red Team 46, DZMZ Optimizer 30, Docklands Pass 17, Fransofia Ward 6. Maglectric and Side Hustle are the two the planner never spends a click on — at the reference's weights a 1[c] or 2[c] card with no declared income is worth `board_presence_weight` (1.0) less its price at 0.4 a credit, which beats a click (0.4) only when the click is free — and the measurement says they cost nothing when it is.
- **Maglectric Rapid is used.** §39 recorded it "installed 41 times in 96 games and never used to derez". In the 288 planned-arm games its HQ-run offer came 104 times and was accepted 62, a derez each — §44's look-through again, the accepted paid choice leading into a selection one ply now answers. It is not a debt; the blind list (`scripts/blind_cards.py`) remains the instrument that says so on a full pass.

**Not done, and why.** The §39 cost is closed as not reproduced rather than traced to a cause, because 288 paired games on three seeds found nothing to trace. The eight-seed playout tool and the per-game pairing are scratch and not committed; the archive entry records the apparatus. No code changed; the tree is `main`'s plus this record and the doc comment on `its_identitys_choice`.

**Verified.** No code changed.

## 50. Byte! is not blind, the instrument was: a hand trap's use is the spring, and `diag precepts` counts it off the action — DONE (`fix/a-sprung-trap-is-a-use`, 7 October 2026)

**The debt.** The blind list (§29) named Byte! first among the Corp's cards — random seats used it 51 times on the 192-game casual pass of seed 2, the planner never — and the card-pool ledger carried it as a card the planner never plays.

**Measured first, on the card's own decks.** Fine Print, Glyph of Warding and Pork Chops, the three sample decks that hold Byte!, against Bowel Movements, Enthusiasm, Stolen Goods and Sabbatical, eight seeds each — 96 games a seating, one process a game, the records kept — on `main` at `bcb36834`:

| seating | Byte! installed | accessed | offered (affordable, outside Archives) | sprung | declined |
|---|---|---|---|---|---|
| planner both chairs | 10 | 77 | 56 | **21** | 35 |
| random both chairs | 73 | 74 | 33 | 4 | 29 |

The planner plays Byte! the way §20 decided a hand trap is played — kept in HQ (`HELD_TRAP_WEIGHT`), where the Runner meets it looking for agendas — and springs it there, 21 times in 96 games, where random seats install it 73 times and spring it 4. **What the instrument counted was the install.** `diag precepts` read a use off the events a seat's action produced (installed, played, rezzed, activated, advanced, scored), and a paid access interaction's events are the cost and the damage and the tag — `CreditsSpent`, `DamageDealt`, `TagGiven` — none of which names the card. So a card whose whole use is being sprung from HQ read as never used by the seat that uses it right, and as used 73 times by the seat that installs it wrong.

**What it is now.** The watcher counts `PlayerAction::PayAccessTrigger { card_id }` as a use of `card_id`, off the action, beside the event-read uses. One arm, one test (`a_sprung_trap_counts_as_a_use_of_the_card`). Nothing in the bots changed.

**Measured.** `diag precepts --deck-styles --games 391 --seed 2`, random and planner seats, the binary before and after — the same games, every per-game record identical apart from `used`:
- Byte!: random 72 → 74, planner **2 → 11** (9 springs beside 2 installs in the 51 games its decks play; ratio 0.03 → 0.15). Behold!, the other paid access trap in the pool, gains its one spring on the random pass.
- `blind_cards.py` at the strict ratio is **unchanged, 11 cards**: Public Trail (19 / 0), Shipment from Vladisibirsk (7 / 0), Retribution (6 / 0); Carnivore (24 / 0), Bravado (18 / 0), Tranquilizer (16 / 0), Wildcat Strike (12 / 0), Cacophony (10 / 0), Boomerang (6 / 0), Raindrops Cut Stone (6 / 0), Pichação (5 / 0). Byte! was already off the strict list at this pass size on `main` (planner 2, two installs); the 51 / 0 was the 192-game pass at #341, where the planner's two installs had not happened yet. At `--ratio 0.2` it is on both lists, at 0.03 before and 0.15 after.

**Not done, and why.** Whether the planner springs Byte! *enough* — 21 of 56 affordable offers — is a question about `OPPONENT_GRIP_SHORTFALL_WEIGHT` and the tag's worth, not about blindness, and is not measured here: the decline is the term's reading of a grip above the floor, which §20 set on purpose. The three decks' 96 games are scratch and not committed; this entry records the apparatus.

**Verified.** `cargo test --workspace` green, `cargo clippy --workspace --all-targets` silent. The engine and the bots are untouched: both passes play the same games.

## 51. The tag's leverage is every Corp's, read off the punisher in HQ and not off the kill plan: Public Trail is played — DONE (`feat/tag-leverage-read-off-the-cards`, 7 October 2026)

**The debt.** The blind list (§29, re-taken in §50) named Public Trail first among the Corp's cards: random seats played it 19 times on the full seed-2 pass, the planner never. Four sample decks hold it — Fine Print and Hyper Velocity (NBN: Reality Plus, fast-advance), Gimbatul (Synapse Global) and Quick and Dirty (Weyland: Built to Last, fast-advance) — none of them styled `kill`.

**Measured first, on the card's own decks.** The four against Bowel Movements, Enthusiasm, Stolen Goods and Sabbatical, six seeds each, 96 games a seating, the records kept and replayed through the engine from the Corp's chair (a scratch example over `netrunner_client::replay`, not committed): Public Trail was a legal play on **140 Corp turns**, the Runner could not pay 8[c] on 84 of them, HQ held a card that punishes a tag on 90 — and the planner played it **once** (random seats 34, the Runner taking the tag 33 times).

**Why.** Outside the kill plan the Corp's evaluator gave a Runner's tag no value at all: `TAG_WEIGHT` (4.0) is subtracted on the Runner's side only, and `tag_leverage_weight` — each tag up to two, while HQ holds a card whose text asks for one (`read::punishes_tags`) — was `Plan::Kill`'s term (§25 Stage 6: "a tag is worth only what follows it, which is why the balanced Corp does not pay Public Trail's 4[c] for one"). So Public Trail, 4[c] and a click (2.0 at the guide's rate), bought either the Runner's −8[c] (1.6 at `opponent_credit_weight`) or a tag worth nothing, and was never played. But the gate was always the card in HQ: the plan said whether to read it and the card said whether it was there, and these decks hold Bigger Picture, IP Enforcement, Retribution and Orbital Superiority — the guide's "tag and punish" mixed into fast advance. The deck says which follow-up it holds by holding it; a style flag said it twice.

**What it is now.** `Weights::with_plans` sets `tag_leverage_weight` for every Corp seat, beside the four general Corp terms; `Plan::Kill` keeps the lethal check. The fixture test `a_kill_corp_plays_public_trail_because_the_runners_answer_is_priced` changes its claim: the balanced Corp holding Scorched Earth against a grip of three plans the tag on **117 of 180** seeds (78 before), and its control is now the same Corp holding Hedge Fund in Scorched Earth's place, which tags on **0 of 180** — so the two in five before §51 were samples that made Scorched Earth's own line pay, never the tag.

**Measured.** One binary of this branch with the term behind a scratch environment variable (the "before"), every pass the same games:

- The four decks, 96 games paired by seed: Public Trail **1 → 10** plays (the Runner paid 8[c] 4 times and took the tag 6); offers 140 → 125 turns; Corp wins 48 → 48 (+7 / −7 by deck: Fine Print +2 / −3, Hyper Velocity +0 / −2, Gimbatul +3 / −0, Quick and Dirty +2 / −2). Per game, tags given 2.85 → 3.01, Bigger Picture 0.09 → 0.16, Predictive Planogram 1.57 → 1.49, Orbital Superiority installs 0.75 → 0.71, scores 2.07 → 2.05, steals 2.26 → 2.11.
- `diag precepts --deck-styles --games 391 --seed 2`, planner both chairs, against §50's random pass: **Public Trail 0 → 6, Retribution 0 → 2, IP Enforcement 2 → 4, Oppo Research 1 → 6**; Bigger Picture 13 → 13, Funhouse 125 → 126, Ping 369 → 369, Orbital Superiority 447 → 432. `blind_cards.py` at the strict ratio: **the Corp side is empty** (it was Public Trail and Retribution); the Runner side keeps Bravado, Tranquilizer, Wildcat Strike and the rest. Corp share on that pass 0.583 → 0.552. No derived ratio moved past noise (tags given a game 1.86 → 2.01 the largest).
- Planner self-paired, `--deck-styles`, 384 games, three seeds: Corp share **0.508 → 0.513** (+0.005, z +0.37, 30 discordant), **0.581 → 0.549** (−0.031, z −2.27, 28), **0.542 → 0.565** (+0.023, z +1.62, 31); pooled +44 / −45, z −0.11. Every discordant game is in a deck that holds a punisher (Fine Print +4 / −10 and NBN: Reality Plus +0 / −4 on the first two seeds; Gimbatul +9 / −6), none in a deck that holds none — which is the apparatus check. Seed 2's −0.031 is one seed's; the third seed was taken because the first two disagreed.

**Found on the way: a "before" from the wrong tree.** The first before/after was `target/release/netrunner_cli` as the previous session left it against this branch's build, and Agency — a deck holding no punisher, whose Corp the term cannot reach — played 15 of 24 games differently, the self-pairing reading −0.016 / −0.031 with no cause. The binary had been built before a `git pull` brought four card files into the pool, and a larger pool re-draws every `determinize` sample. With the term gated in one binary the same Agency game was byte-identical either way, and every number above is against that before. **A before is the same tree with the change gated, or a pinned worktree build; never a binary left over from before a pull.**

**Not done, and why.** Whether 1.5 a tag is the right price for a fast-advance deck's follow-up, and whether a tag's tax on the Runner — a click and 2[c] to clear, which the guide names and which needs no card in HQ — is worth a term of its own, are not measured: Public Trail is played where the Corp holds the follow-up, which is the guide's sentence, and a Corp holding none still does not pay 4[c] for a bare tag. The replay counter and the gated binary are scratch and not committed.

**Verified.** `cargo test --workspace` green with the term live (the sweeps included), `cargo clippy --workspace --all-targets` silent. Engine untouched; `netrunner_bots` only.

## 52. An access trash is worth a trash a turn to the plan that trashes, and a held console is dead while one is down: Carnivore is installed — DONE (`feat/an-access-trash-is-worth-a-trash-a-turn`, 7 October 2026)

**The debt.** The blind list (§29, re-taken in §50 and §51) named Carnivore first among the Runner's cards once Public Trail was paid: random seats used it 24 times on the full seed-2 pass, the planner once (the card pool's lists 12 / 27 casual, – / 6 startup). Two sample decks hold it, Bowel Movements and Party Hard — both René "Loup" Arcemont, three copies each beside a DZMZ Optimizer.

**Measured first, on the card's own decks.** The two against Agency, Brick Stack, Gimbatul and Hidden Funds, twelve seeds each, 96 games, the records replayed through the engine from the Runner's chair (a scratch example over `netrunner_client::replay`, not committed): Carnivore was a legal install on **252 Runner turns**, the Runner had its 4[c] on every one of them and a program in the grip on 218 — and the planner installed it **0 times**. Random seats installed it 32 and used it 19.

**Why.** A 4[c] console read in the hand as a rig card under its price: board presence 1.0 less 1.6 at `own_credit_weight`, and nothing for "Access, once per turn → Trash 2 cards from your grip: Trash the card you are accessing." The evaluator read an access ability for nothing. The guide's Anarch chapter names Carnivore and Gourmand under "trash what you access", which is the Dismantle plan's lever (`dismantle_weight`, 0.5: what a rezzed asset or upgrade waiting to be trashed is worth).

**What it is now.** `read::access_trash_cards` reads the ability off the card — a `Paid` ability flagged `access` whose effect holds `Effect::TrashCurrentlyAccessedCard`, with the grip cards its `Cost::Trash` asks for — and `runner::access_trash_value` prices it: the plan's trash, plus what the identities pay for a trash while accessing (`identities::on_trash_while_accessing`: Loup's credit and card, 0.8), less the grip cards at a click a card, never below nothing, over the stage's horizon at `future_credit_weight`. Under Loup that is 0.5 a turn, discounted; on any other plan it is zero, so a balanced Runner still reads a 4[c] console as a rig card under its price. Read for the held card and for the installed one alike, so the card is live in hand exactly when the table credits it. At the full rate — no future discount — the card read as the best in the deck and the Runner installed it over its economy, Corp wins 46 → 65 of 96; the discount every declared income takes is what it takes. Beside it, `held_cards_value` reads a held console as dead while one is installed: the checkpoint's console limit (CR 3.8.5b) trashes the first for the second, and the three copies had been a promise each. The fixture test `a_dismantle_runner_under_loup_installs_carnivore_and_another_plan_does_not`: Loup, turn 4, 6[c] and one click, Carnivore and three blanks, the centrals iced — the Dismantle Runner installs it on 15 of 20 seeds, a Pressure Runner on 0 (a balanced seat under an Anarch identity is Dismantle, `Style::or_faction`).

**Tried and reverted: the held console's memory.** The first version also credited the held console its "+1[mu]" (`memory_weight` a unit, read off `ContinuousKind::Memory`), on the ground that the hand should read what the table will pay. It did, and the planner stopped installing consoles: on the full pass Hermes 25 → 1, Pantograph 26 → 9, T400 Memory Diamond 8 → 1. Paired on Hermes's two tournament decks against four Corps, six seeds, 48 games, three variants of one binary: Hermes installed **29** with the whole change gated, **29** with only the memory term gated, **3** with it on — the term, and nothing else in the change. A 2[c] console's delta is 0.7 with the memory read; the hand holds it at `held_card_weight` (0.35) and the install costs a click (0.4), so the install nets nothing. The memory is the install's surprise — the one asymmetry the guide's "install your console" needs until a console's text is read — and the doc comment on `install_delta` says so. On the final pass the consoles are unmoved: Hermes 25 / 25, Pantograph 26 / 26, T400 8 / 8, Pennyshaver 524 / 523, DZMZ Optimizer 160 / 160.

**Measured.** One binary of this branch with the change behind a scratch environment variable (the "before"), every pass the same games:

- The two decks against four Corps, 96 games paired by seed: Carnivore installed **0 → 48** (legal on 252 → 256 turns), its ability legal at 532 positions and used **105** times — Regolith Mining License 36, Spin Doctor 24, Otto Campaign 7, Malapert Data Vault 6, Manegarm Skunkworks 5, Anoetic Void 4, Mercia B4LL4RD 4, Idiosyncresis 4, Amaze Amusements 3, Ansel 1.0 2, Semak-samun 2, Kessleroid 1, six operations out of HQ (Hedge Fund 2, Hansei Review, Petty Cash, Key Performance Indicators, Doomscroll) and one agenda (Project Ingatan). Corp wins **46 → 45** (+17 / −18 discordant; Bowel Movements 22 → 20, Party Hard 24 → 25).
- `diag precepts --deck-styles --games 391 --seed 2`, planner both chairs: **Carnivore 1 → 61, Gourmand 14 → 168** (random 24 / 108; Gourmand fires on the trash), Fermenter 180 → 127 (the grip copies are the cost), Madani 312 → 370; Corp share 0.552 → 0.535. `blind_cards.py` at the strict ratio against §50's random pass: Carnivore is off; the Runner side keeps Bahia Bands, Bravado, Tranquilizer, Wildcat Strike, AirbladeX, Cacophony, WAKE Implant, Boomerang and Raindrops Cut Stone, and this pass puts Neurospike (11 / 0) and Retribution (6 / 0) on the Corp side, both 1–2 on §51's pass of the same seed — a pass's drift, not a finding of this change. Pinhole Threading stays 56 / 1.
- Planner self-paired, `--deck-styles`, 384 games, three seeds: Corp share **0.516 → 0.555** (+0.039, z +2.02, 55 discordant), **0.549 → 0.534** (−0.016, z −0.80, 56), **0.562 → 0.565** (+0.003, z +0.13, 61); pooled +91 / −81, z +0.76 — inside the band. Every Runner deck in the pool holds two or more copies of a console, so the dead-console reading reaches them all and no deck is the identity check this time (the Hermes pairing above is the check that the two gates differ by the memory term alone); the discordance sits where the reading bites most — the two Carnivore decks (22 and 25 of 69 games), Prick Thyself with three Bling (25) and Loup's tournament deck with two Keiko (26 of 66) — and Dashing Mad and Magdalene's deck, two copies each, moved in 0 of 69 and 0 of 66.

**Found on the way.** Carnivore's trash is taken under Ansel 1.0's "the Runner cannot steal or trash Corp cards": `legal_actions::access_flow_candidates` withholds the Runner's steal and paid trash under `Prohibition::StealOrTrash`, and `Effect::TrashCurrentlyAccessedCard` is never asked — right for the bot, which reads a card it trashes as a card, wrong for the rules, and recorded as an open deviation of 1.2.2 in `docs/roadmap/rules-conformance.md`.

**Not done, and why.** The ability's cost is read as two cards at a click each, not as what the two cards are: a breaker fed to it is read the same as a Ritual, and six of the 105 trashes were operations out of HQ for two grip cards. The plan's trash term does not read what the accessed card is either, which is where that would go. Gourmand's own text is read by nothing of its own; it rode with Carnivore. The replay counter and the gated binary are scratch and not committed.

**Verified.** `cargo test --workspace` green with the terms live (the sweeps included), `cargo clippy --workspace --all-targets` silent. Engine untouched; `netrunner_bots` only.
