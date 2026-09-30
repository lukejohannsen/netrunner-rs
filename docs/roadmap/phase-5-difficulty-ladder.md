# Phase 5 — A difficulty ladder

**The goal is not a stronger bot.** Everything in Phase 2 and Phase 3
asks "how strong is this?", and nothing asks the question a person
actually has, which is "give me something I can nearly beat, and then
something a little harder." That needs an **order**, a **name** a player
can ask for, and a guarantee the order is real — none of which falls out
of a bot ladder sorted by rating.

**Why it is a phase and not a flag.** The three knobs a player could
already reach (`--corp`, `--corp-style`, `--simulations`) build
combinations nobody has measured, and two of the three do not move
strength monotonically at all. A difficulty level is a promise about
*relative* strength on one chair, and this workspace had no such promise
anywhere.

---

**Where it stands (29 September 2026).** Five rungs per chair, each ladder one ply at
five handicaps (`Level::spec`: `epsilon` 1.0 / 0.22 / 0.11 / 0.05 / 0.0 over
`heuristic:<deck's style>`), spaced by measurement at 768 games a cell, a deck's style kept by its
rung, and the Corp's top rung at 0.41–0.43 against the un-handicapped one-ply Runner (§24). §1–§24
are closed and their record is in [the archive](archive/phase-5-difficulty-ladder.md); what a bot
session needs of it is in [Reference](#reference) below. **§25 is the open work: bots that play the
strategy guide's precepts, in plans that fit a deck's strategy and strength.**

## Open

- **§25 — bots that play the strategy guide's precepts**: the brief below, now the plan below it — the
  decision core rebuilt on the existing harness in eight stages. **Stages 0–5 built** (29 September
  2026: the Startup gate and format filter; the precepts report and its baseline; what a bot knows —
  the format's pool, a faction-and-seen-copies prior over the opponent's deck, memory; the evaluator
  as modules with a board-read stage and the stage dial deleted; the turn planner, measured against
  the one-ply reference; economy at the guide's rate — eight terms the planner alone scores with,
  which brought the Corp chair back level with the reference and took the Runner chair past it, with
  the Corp's hand measured as not worth holding; the Corp's plans a deck stacks — `Personality`
  replaced by `plans::{Plan, Style}`, a deck's style a list the reference plays the first plan of
  and the planner the whole of, with the stakes of a run, the rez held for it, the ICE order, the
  never-advance line, a kill plan with its Sweep deck, the traps plan's bluff and the fort falling
  away once the rig beats the wall). Stage 7, the Runner's plans and the identity both chairs read,
  is next, when the person asks. Nothing a person meets has moved: every rung is still the one-ply
  chooser until Stage 8.
- **§4 (a) on the Runner chair**: the Runner ladder has not been re-spaced since §23 moved every
  Corp profile's fort terms; the Corp's (a) and (b) closed with §24, (c) with §19 — this line
  corrects the archived §3 "IN PROGRESS" and §4 "OPEN" headings, which were never updated.
- **§22, unverified**: the one-ply Runner is the opponent in every Corp number here, and `glacier`
  may be exploiting its play against ICE rather than playing better.
- **A fifth Corp rung needs a stronger `elite`, not a handicap** (§22); `LevelKind::{Mcts, Puct}`
  stay in the code until a search beats one ply again (§24).
- **Samples carry no identity** (§25 Stage 4): `determinize` sets `identity: None` on every sample, a
  measured decision, so an identity's continuous effects — A Teia's remote limit, every printed
  link and hand size — are invisible to any search; the planner checks each step against the real
  list, so a line the identity forbids is dropped and re-planned, never played, and the cost is the
  beam it spent on it. Carrying the identity into the sample is a measured change, owed when a
  search needs it.

## Closed — one line each

- **§1** — Search budget is not a difficulty dial (12 September 2026).
- **§2** — Rungs are a strong bot handicapped (13 September 2026).
- **§3** — The Corp rezzed its cheapest ICE, not the ICE that held (`feat/corp-reads-the-rig`, 16 September 2026).
- **§4** — Owed: re-space the Corp chair, reconsider how its top rungs are built, and teach the bots to play in phases (16 September 2026).
- **§5** — The tempo instrument, and a baseline in which both chairs play their stance backwards (`diag/tempo`, 16 September 2026).
- **§6** — The stance dial, and the endpoint it travels to is the wrong one (`feat/runner-plays-in-phases`, 16 September 2026).
- **§7** — The build endpoint was the worst Runner profile, and with it repaired no stance travel beats standing on it (`feat/build-endpoint-covers`, 16 September 2026).
- **§8** — The turn-counter fallback leg: a clock does no better than the board, and neither beats not travelling (`diag/turn-counter-stage`, 16 September 2026).
- **§9** — The Corp profiles audited: `glacier` was burying its hand, and repaired it is the strongest Corp (`feat/corp-profile-audit`, 16 September 2026).
- **§10** — The Corp ladder in `glacier` style: every rung that reads the style gains, and `operator → veteran` goes flat (`diag/glacier-corp-ladder`, 16 September 2026).
- **§11** — `glacier`'s `veteran` Corp gets its own handicap: `epsilon` 0.02, and the glacier ladder climbs evenly at the top (`feat/glacier-veteran-epsilon`, 16 September 2026).
- **§12** — The Corp ladder in `rush` style has no top: search buys it +0.012, so no handicap can space it (`diag/rush-corp-ladder`, 16 September 2026).
- **§13** — The Corp ladder in `trap` style: stronger than `Balanced` at every rung, and its flat top step is `Balanced`'s own (`diag/trap-corp-ladder`, 16 September 2026).
- **§14** — `Balanced`'s `veteran` Corp plays at `epsilon` 0.20, and every other Corp style needed its own (`feat/balanced-veteran-epsilon`, 16 September 2026).
- **§15** — A `rush` deck's top two Corp rungs play `Balanced`: its ladder climbs, and the top of it is not a rush (`feat/rush-top-rungs-play-balanced`, 16 September 2026).
- **§16** — The Runner profiles audited: `aggressive` was getting itself flatlined, and `cautious` and `wary` have nothing to repair (`feat/runner-profile-audit`, 16 September 2026).
- **§17** — The Runner ladder in every Runner style climbs at every step, so no Runner style needs a handicap of its own (16 September 2026).
- **§18** — A Corp with both `glacier`'s fort and `trap`'s ambush terms is the strongest one-ply Corp, and search takes the gain back (`diag/corp-fort-and-ambush`, 17 September 2026).
- **§19** — `glacier` builds the fort it is named for: centrals first, one scoring remote, then the agenda (`feat/glacier-builds-its-fort`, 22 September 2026).
- **§20** — Traps: hidden until sprung, played like an agenda, and not run into twice (22 September 2026).
- **§21** — `glacier`'s Corp ladder is one ply at five handicaps, and it climbs at every step (`feat/glacier-corp-ladder-is-one-ply`, 22 September 2026).
- **§22** — The ladders re-taken after the trap terms: the Corp's `apprentice` moves to 0.25, and no handicap can space `Balanced`'s or `trap`'s top (`feat/corp-ladder-retaken-after-traps`, 23 September 2026).
- **§23** — Where the fort goes is how a Corp plays, not a style: every Corp profile builds one (`feat/fort-in-every-corp-profile`, 23 September 2026).
- **§24** — Every Corp ladder is one ply at five handicaps, `glacier`'s, and no Corp style carries an exception (`feat/every-corp-ladder-is-one-ply`, 23 September 2026).

## Reference

The measurements a bot session starts from, moved here whole from the entries that took them.

### The 768-games-a-cell calibration, and the ε rule (§2)

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

### The Corp chair's baseline and why search is not its lever (§2)

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

### The tempo baseline: both chairs played their stance backwards (§5)

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

### Where the Runner profiles stood (§16)

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

### The Runner ladder in every style (§17)

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

### The fort terms (§19)

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

### The Runner ladder, latest (§22)

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

### The style matrix (§22)

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

### The shipped Corp ladder: every style one ply at five handicaps (§24)

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
/ 0.11 / 0.05 / 0.0.** `LevelSpec::with_personality` (now `with_style`) now changes the style
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
