# Phase 5 — A difficulty ladder

**The goal is not a stronger bot.** Everything in Phase 2 and Phase 3
asks "how strong is this?", and nothing asks the question a person
actually has, which is "give me something I can nearly beat, and then
something a little harder." That needs an **order**, a **name** a player
can ask for, and a guarantee the order is real — none of which falls out
of a bot ladder sorted by rating.

**Why it is a phase and not a flag.** The three knobs a player could
already reach (`--corp`, `--corp-personality`, `--simulations`) build
combinations nobody has measured, and two of the three do not move
strength monotonically at all. A difficulty level is a promise about
*relative* strength on one chair, and this workspace had no such promise
anywhere.

---

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

## 25. Bots that play the strategy guide's precepts — OPEN, a brief (25 September 2026)

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
