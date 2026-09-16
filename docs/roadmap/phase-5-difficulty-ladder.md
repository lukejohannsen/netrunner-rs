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
`LevelSpec::describe`, and the one `ratings::LocalRatings::suggest`
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


## 4. Owed: re-space the Corp chair, reconsider how its top rungs are built, and teach the bots to play in phases — OPEN (16 September 2026)

Three jobs, opened by §3 and none attempted there. They are recorded
together because (b) may make (a) unnecessary, and (c) is the largest of
the three.

**(a) Re-space the Corp rungs.** §3 lifted `operator` (one ply) by +0.016
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
item and is the reason (a) is worth doing *after* it rather than before.
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
