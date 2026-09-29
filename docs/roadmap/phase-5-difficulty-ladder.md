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

**Where it stands (29 September 2026).** Five rungs per chair, each ladder one ply at
five handicaps (`Level::spec`: `epsilon` 1.0 / 0.22 / 0.11 / 0.05 / 0.0 over
`heuristic:<deck's style>`), spaced by measurement at 768 games a cell, a deck's style kept by its
rung, and the Corp's top rung at 0.41–0.43 against the un-handicapped one-ply Runner (§24). §1–§24
are closed and their record is in [the archive](archive/phase-5-difficulty-ladder.md); what a bot
session needs of it is in [Reference](#reference) below. **§25 is the open work: bots that play the
strategy guide's precepts, in personalities that fit a deck's strategy and strength.**

## Open

- **§25 — bots that play the strategy guide's precepts**: the brief below, now the plan below it — the
  decision core rebuilt on the existing harness in eight stages. **Stages 0 and 1 built** (29 September
  2026: the Startup gate and format filter; the precepts report and its baseline). Stage 2, what a bot
  knows, is next, when the person asks.
- **§4 (a) on the Runner chair**: the Runner ladder has not been re-spaced since §23 moved every
  Corp profile's fort terms; the Corp's (a) and (b) closed with §24, (c) with §19 — this line
  corrects the archived §3 "IN PROGRESS" and §4 "OPEN" headings, which were never updated.
- **§22, unverified**: the one-ply Runner is the opponent in every Corp number here, and `glacier`
  may be exploiting its play against ICE rather than playing better.
- **A fifth Corp rung needs a stronger `elite`, not a handicap** (§22); `LevelKind::{Mcts, Puct}`
  stay in the code until a search beats one ply again (§24).
- **Housekeeping** (§25): `difficulty.rs`'s module doc says six personalities; there are eight.

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
