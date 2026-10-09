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

**Where it stands (30 September 2026).** Five rungs per chair, every rung the turn planner at a
measured handicap (`Level::spec`: `epsilon` 1.0 / 0.22 / 0.11 / 0.05 / 0.0 for the Corp and
1.0 / 0.45 / 0.25 / 0.10 / 0.0 for the Runner, over the deck's style), spaced by measurement at
384 games a cell on two seeds, and the one-ply reference deleted (§25 Stage 8). §1–§27 are closed
and their record is in [the archive](archive/phase-5-difficulty-ladder.md); what a bot session
needs of it is in [Reference](#reference) below. **A bot change is measured as both chairs' win
share on pinned binaries, the same games paired by seed, and by `diag precepts`.**

## Open

- **The Runner's ladder is owed a re-spacing** (§47): re-taken after §44, the Corp's steps are still
  even but the Runner's are not — `operator → veteran` +0.146 / +0.190, `veteran → elite` +0.096 /
  +0.057 (1.6 sd on seed 2). Each seed's curve puts even steps at Runner ε ≈ 0.37 / 0.21 / 0.12 in
  place of 0.45 / 0.25 / 0.10; set them and re-take the Runner's square on both seeds. **Deferred
  until the card pool is complete and every card reachable** (decided 7 October 2026): the rungs
  move with every bot change, and the bots will be adjusted again once the pool is in, so the
  square is re-taken once, at the end, not after each change.
- **`LevelKind::{Mcts, Puct}` stay in the code** until a search beats the planner on either chair (§24, §25
  Stage 8); a sixth rung on either chair would need a stronger `elite`, not a handicap (§22).

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
- **§25** — Bots that play the strategy guide's precepts: the decision core rebuilt on the harness in nine stages, every rung the turn planner, both ladders re-taken on it and the one-ply reference deleted (25–30 September 2026; the last stage `feat/ladders-on-the-planner`). §4 (a), the Runner ladder's re-spacing, and §22's question about the one-ply Runner closed with it.
- **§26** — The beam keeps one line per position, and a line is judged by the free score it leads to: the glacier Corp that iced instead of scoring was a beam holding three positions in six slots, not a line judged too early — the judgment alone does not find it (`feat/beam-one-line-per-position`, 30 September 2026).
- **§27** — The stack overflow §26 recorded was a bot's sample and the engine together: the Corp's sample named a masked access as a Boto the Runner must steal, and a score asked the stolen ice's strength text whether threat was 4 — a scan now asks only effects of the kind in question, and a masked access is named as a card the sample holds that the decision is true of (`fix/stolen-non-agenda-score-recursion`, 30 September 2026).
- **§32** — A card that pays on a run is worth what its runs will pay: a click ability that begins a run pays its rider per use and is not charged its click (Red Team), a trigger on every successful run pays once a turn (Pennyshaver), the rig's breach accesses are the run's (Docklands Pass) and the counters a card's own runs place are R&D accesses promised (Conduit, under the rig plan) — the four installed and used where their decks hold them, off the blind list on every pass; the self-pairing flat and Runner-ward, the four card pairings 128 → 113 Runner wins, recorded (`feat/run-paying-installs`, 2 October 2026).
- **§33** — A sample carries both identities: an identity's ability was in the view's list and no sample's, so the planner never used one — Topan's install, Synapse Global's tag and LEO Construction's end-the-run now played, off the blind list on every pass; the self-pairing inside the band both ways; LEO's trade of a bioroid for an ended run measured as the Corp's own loss and owed next; Hit List takes two Malandragem for the sweep gate (`feat/samples-carry-identities`, 2 October 2026).
- **§34** — A run is priced with what both identities print about it: Gabriel Santiago's, Zahya Sadeghi's, Dewi Subrotoputri's and René "Loup" Arcemont's pay at the run leaf and is denied by the Corp's run term, Mercury Chrome's access joins the breach, BANGUN's punishment and a faceup agenda's steal are priced at the access; the self-pairing inside the band, the identities' triggers fired more in six of seven identity pairings (`feat/a-run-reads-both-identities`, 3 October 2026).
- **§35** — The seat's own turn's end is planned: the engine enters the other side's start of turn before a decision the discard step's triggers parked is answered, so every line ended there and the one-ply chooser declined PT Untaian's advance 5 times in 16 with a card to put it on; PT Untaian's advance taken 5–7× as often, Magdalene Keino-Chemutai's install from 0 to 85 uses in 48 games and the Sabbatical Runner's wins 12 → 17 and 10 → 21 of 32; the self-pairing moved toward the Runner, past the band on seed 2, most of it the Runner seat's (`feat/turn-end-identities`, 3 October 2026).
- **§36** — Steals, scores and tags across the table: core damage is a hand size on both chairs and Thule Subsea's tax is paid (6 → 79 and 14 → 77 times in 48 games); a steal under Jinteki: Personal Evolution is read toward the flatline; a selection the seat's own identity parks on it is planned, so Synapse Global's free install and Poétrï's install from HQ are taken (selections 4 → 61 and 5 → 54, 9 → 96 and 4 → 81); a faceup BANGUN agenda is worth its punishment, not an asset's weight; the self-pairing within the band (`feat/identities-steals-scores-tags`, 3 October 2026).
- **§37** — Standing effects and hosted counters: Issuaq Adaptics' counters are points and an agenda advanced to its requirement is held behind a wall for one; a hosted counter is half of what spending it buys, not a flat 2.0 (within the band, +0.008 on both seeds); AU Co.'s turn-start search is planned (taken 46 → 161 and 63 → 161 times in 48 games); Kate "Mac" McCaffrey's discount is in a held card's price; and an agenda in Archives is half its points to the Corp, which had trashed 68 agendas in a planner pass of the pool and now trashes 10 — the self-pairing +0.078 toward the Corp on both seeds for that alone, past the band (`feat/identity-standing-effects`, 4 October 2026).
- **§38** — Tāo Salonga's swap is priced by where each piece of ICE stands: the Runner reads what each server's breach is worth at the share its rezzed ICE shuts the rig out, off a run, at a tenth of the breach's rate (at one it pulled every Runner toward installs and moved Startup +50 / −32 toward the Corp), and the swap is planned as whole pairs — taken 0 → 520 times in 609 offers over four pairings of 48 games; the other identities' "may" ahead of a selection of their own cards stays one ply, measured (`feat/tao-salonga-swap`, 4 October 2026).
- **§39** — The "may" ahead of an identity's selection of its own cards is planned, as Tāo Salonga's swap was: Precision Design's, Méliès U.'s, Barry "Baz" Wong's, Magdalene Keino-Chemutai's and Sebastião Souza Pessoa's yes is the selection's best line, not its worst — Barry's install from the grip taken 0 → 144 times in 96 games, and only while the run can still break what was rezzed; Barry's deck loses games for it in self-play (35 → 46 Corp wins of 96, z +1.98) while the decision itself, played out both ways from the record, does not — recorded, not traced; the self-pairing flat (`feat/identity-may-planned`, 4 October 2026).
- **§40** — Agendas the Corp sends to Archives, measured (`diag precepts`' new counters): about 15 in 192 planner games, every hand-size discard forced, 1.2–1.6% of steals out of Archives — but where a card's text asks the Corp to choose among agendas and other cards, Hansei Review in its own planned turn trashes no agenda (0 / 41), AU Co.'s planned search 8 / 14 and Ryō "Phoenix" Ōno's out-of-turn prompt 5 / 17 (`diag/agendas-to-archives`, 5 October 2026).
- **§41** — One ply scores a card selection where its confirm leaves the board: a toggle moved no card, so every candidate tied and the jitter chose; Ryō "Phoenix" Ōno's prompt and Longevity Serum trash no agenda the Corp could keep (7 → 0 and 3 → 0 over two seeds of 192 games), agendas to Archives 17 / 16 → 10 / 11, the self-pairing inside the band (`fix/one-ply-scores-the-confirmed-selection`, 5 October 2026).
- **§42** — A selection is decided on the cards it shows: the sample put guesses where the view named the top 3 cards of R&D, and the line planned at AU Co.'s offer was followed into the selection, so the trash fell on the guesses; `determinize` now seats a selection's shown cards and the planner re-plans when one is first shown — AU Co.'s agenda trashes where it could keep one 8 → 0 over two seeds of 192 games, steals out of Archives 4 / 6 → 0 / 5, the self-pairing inside the band; §40's lead was wrong (`fix/au-co-search`, 5 October 2026).
- **§43** — A paid end-the-run waits for the run's last window and pays only for what the breach would take: a held end-the-run caps what a run costs the Corp at its price until CR 6.9.4e, a run let past it costs its stakes and no flat term, Mercia B4LL4RD's turn-by-turn install is read, and the fort and the taxing window are read between runs — LEO Construction's uses 28 → 17 and at a run's initiation 20 → 1 in 96 Agency games, the self-pairing inside the band with every changed game a Mercia deck; §33's LEO loss no longer reproduces on `main` (`feat/a-paid-end-the-run-waits-for-the-threat`, 5 October 2026).
- **§44** — One ply answers a decision of the seat's own before it scores the action that parked it: the evaluator charged a parked choice 2.0 and credited a lower bound of its upside, so passing into Brân 1.0's "you may install" tied LEO Construction's trash of Mercia B4LL4RD (5 of its 17 uses were jitter) and the Corp declined the free rezzes and installs a "may" offers it — Send a Message, Brân, Scatter Field, Plutus, Ballista, Ansel; the self-pairing moved past the band toward the Corp on both seeds (+0.052, +0.078), the Corp seat's, and the ladders are not re-taken (`fix/one-ply-looks-through-its-own-choice`, 5 October 2026).
- **§45** — The fort is read so the Corp cannot rebuild it by trashing its own cards: an upgrade sits in a fort's root beside its agenda (8 of LEO Construction's 15 uses were a Mercia B4LL4RD out of one, read as +3.0 of fort), and the "glacier, then fast advance" wall is beaten by the Runner's rig — every subtype, and the credits to break in — not by a piece count the Corp moves, with an agenda's exposure kept once it is (a Bumi 1.0 trashed at a run's initiation had read +13.0 of fort); LEO's ICE trashes at an initiation 2 → 1, the self-pairing inside the band; the view sweep's fog rule carries a card watched back to HQ across the turn (`fix/fort-root-holds-an-upgrade`, 5 October 2026).
- **§57** — An opponent's choice is answered inside the line, a break with no strength contest is priced, and the first run each turn pays (asked for by the person): Wildcat Strike's "the Corp chooses" ended every line on the parked choice with its cost paid, `break_cost` read no `BreakSubroutinesUnconditionally` (Boomerang, Botulus, Endurance, Poison Vial) and `about_every_run` refused a "first time each turn" (DreamNet); now `opponents_answer` takes the opponent's worst `ResolvePendingChoice`, an unconditional break is priced where its requirement reaches with a `[trash]` one use at the card's cost, and a successful run's cards are income — on the full seed-2 pass Wildcat Strike 0 → 21, DreamNet 0 → 19, Boomerang 0 → 7, Botulus 6 → 74, the quarter-ratio blind list 27 → 22 and the strict one down to Psychographics (read, its turn seldom comes), Cacophony (owed) and four cards at one use on `main`; Corp share +0.017 / −0.007 on two seeds (pooled z +0.45) (`claude/fix-bot-card-blindness-tik8w5`, 8 October 2026).
- **§58** — A lockdown in play is read as an identity is: the Corp's play area was read by nothing, so SYNC Rerouting and Argus Crackdown paid on a turn the line never reached and were a click for nothing; now `identities::heard` scans the lockdowns too (with `ProtectedByIce` and `ChosenServer` answered), the Corp prices one in play as what it takes from the Runner's next run and the Runner fears Argus Crackdown's damage on its run — over the Sweep decks SYNC Rerouting 0 → 23 / 1 → 30 and Argus Crackdown 0 → 15 / 0 → 16 on two seeds, Corp share pooled z −0.89; Hyoubu Precog Manifold's psi game owed (`claude/bot-blindness-lockdowns`, 8 October 2026).
- **§59** — The Runner's hand size is read, not only its core damage: both chairs weighed core damage as the hand size it takes (§36) and never what the table gives back or takes, so Marrow's "+3 maximum hand size" was read as its install's core damage for nothing; now `fundamentals::runner_hand_size_lost` reads `continuous::hand_size` beside the damage — over the Sweep decks Marrow 0 → 46 / 0 → 42 on two seeds, T400 Memory Diamond, Supercorridor and Hippocampic Mechanocytes played more, Corp share pooled z +1.35; sabotage (Cacophony, Nga) owed (`claude/bot-blindness-hand-size`, 9 October 2026).
- **§60** — A sabotage is read: a Corp selection it parks is the cards it will take, at a click each, and a card's sabotage a turn is cards at a credit over the turns left, bounded by the counters its uses remove — so Cacophony is installed and cashed (0 → 14 over the Sweep decks on two seeds), Corp share inside the band (pooled z +1.22); Nga still seldom installed, its three sabotages not worth a program's price and memory at that rate (`claude/bot-blindness-sabotage`, 9 October 2026).
- **§61** — A breach a lockdown denies is priced: Hyoubu Precog Manifold's psi game ends a successful run on its chosen server six times in nine, and the Corp reads it as the agenda points it takes from the Runner's best next run (`identities::breach_denied`, `corp::breach_denial`, at `run_stakes_weight`) — so Hyoubu is played on the server worth most to the Runner (1 → 10 / 1 → 7 over the Sweep decks on two seeds, random 6), Corp share unmoved (pooled z −1.34); the strict blind list re-taken on `main`, 46 cards (`claude/bot-blindness-psi`, 9 October 2026).
- **§62** — A click ability's draws are income: `declared_income` read a click ability's credits and never its cards, so Professional Contacts' "[click]: Gain 1[credit] and draw 1 card" was a credit for a click and never installed; now the cards are credits as a turn start's are, less the credits the cost asks, and a use is bounded by the counters its cost removes — Professional Contacts 0 → 198 / 0 → 116 uses over the Sweep decks on two seeds (random 13), Dr. Nuka Vrolyck 176 → 206 / 190 → 216, Corp share inside the band (pooled z −0.27) (`claude/bot-blindness-click-draws`, 9 October 2026).
- **§63** — A line parked on its own prompt is judged by its best answer, and a token on ice is the break it adds: the beam kept the line that declined Shipment from Kaguya's selection, and a token on ice was worth nothing; now Kaguya is played (0 → 17 / 1 → 11 over the Sweep decks on two seeds, random 13), cards under a quarter of the random rate 110 → 90 / 112 → 91, Corp share inside the band (pooled z −0.82) (`claude/bot-blindness-ice-advancement`, 9 October 2026).
- **§64** — A run-end effect inside a success rider is what it registers: Dirty Laundry's "when that run ends, if it was successful, gain 5[credit]" is a `SetRunEndedEffect` inside its success rider, which `tally` read as nothing; now it reads the effect registered — Dirty Laundry 0 → 27 / 0 → 31 over the Sweep decks on two seeds (random 8), Corp share inside the band (pooled z −0.83) (`claude/bot-blindness-run-end-riders`, 9 October 2026).
- **§56** — A rider that resolves some of its options is read as the chooser's best of them: Bahia Bands' "if successful, resolve 2 of the following" is a `ResolveSomeOf` the rider tally had no arm for, so the run read as a plain run 2[c] and a click dearer and the planner had it legal on 172 Runner turns of 48 games of its decks and played it 0 times; now the tally takes the chooser's best `count` options (the draw and the discounted install, read as two cards, a click and a credit) and the leaf pays a rider's clicks, so the planner plays it 25 times (Corp wins 25 → 23); on the full seed-2 pass 0 → 40 (random 17), the strict blind list eleven cards to five — Wildcat Strike, Cacophony and DreamNet left on the Runner side, four more installed through its rider's own choice and still owed a reading, and the self-pairing +0.005 / −0.021 / −0.008 on three seeds (pooled z −2.18, past the band toward the Runner), every discordant game in its two decks (`feat/a-rider-that-resolves-some-of-its-options`, 8 October 2026).
- **§55** — The Corp reads a trojan on its ice: §54's Tranquilizer taxed a piece for the rest of the game while the Corp, reading no virus counter and nothing hosted on its ice, began 111 of its turns in 48 games under one, 63 with it ripe, and purged 0 times; now `corp::trojans_on_ice` charges the host's rez for every turn the count is ripe within one count's length — the tax until the Corp's next purge, read on a rezzed host and a derezzed one alike — so the Corp purges (0 → 16, ripe turns 63 → 9, derez events 87 → 11, Corp wins 18 → 18) and installs over the host (2 → 4); a trojan one counter short on a rezzed piece was purged already, since the line ends where the derez fires; the full seed-2 pass 0.540 → 0.545 Corp with no card moving past drift, and the self-pairing +0.003 / +0.005 / +0.000 on three seeds (pooled z +1.13), every discordant game in the Tranquilizer decks (`feat/the-corp-reads-a-trojan-on-its-ice`, 8 October 2026).
- **§54** — A trojan's derez is its host's rez a turn: Tranquilizer read as a rig card under its price with no word for the derez, and the planner had a legal host on 232 Runner turns of 48 games of its decks and installed it 0 times; now `read::host_derez` reads the count off the triggers and `host_derez_value` prices the host's rez cost at the opponent's rate for every turn after it, discounted — held on the dearest rezzed piece, hosted with the counters it holds — so the planner hosts it 10 times on Brân, Funhouse, Ansel, Ballista, Doomscroll and Pharos (derez events 3 → 85, Corp wins 23 → 17), on the full seed-2 pass 0 → 5 against random's 18, off the strict blind list, and the self-pairing −0.003 / −0.005 / +0.003 on three seeds (pooled z −0.35), every discordant game in its two decks (`feat/a-trojan-derez-is-the-hosts-rez-a-turn`, 8 October 2026).
- **§53** — A run's end rider is paid, succeed or not: Bravado's "when that run ends, gain 6[credit] plus 1[credit] for each piece of ice you passed" is a `RunEndRider` on the run, and the leaf read only the success rider, so the planner had Bravado legal on 126 Runner turns of 48 games of its decks and played it 0 times; now `read::run_end_income` pays every rider in `on_end` on every run — the ice all passed when the rig gets in, those passed so far when it jacks out — and the planner plays it 34 times (Corp wins 25 → 23), on the full seed-2 pass Bravado 0 → 43 and Raindrops Cut Stone 0 → 22, both off the blind list, and the self-pairing −0.005 / +0.005 / −0.018 on three seeds (pooled z −1.26), every discordant game in a deck holding an end rider (`feat/a-run-ended-rider-is-paid`, 7 October 2026).
- **§52** — An access trash is worth a trash a turn to the plan that trashes, and a held console is dead while one is down: Carnivore read as a 4[c] console under its price and its "trash the card you are accessing" as nothing, installed 0 times on 252 legal turns of 96 games of its decks; now `access_trash_value` — the Dismantle plan's trash plus what Loup pays for it less two grip cards at a click each, over the horizon at the future discount, zero on any other plan — and the planner installs it 48 times and trashes 105 cards (Regolith Mining License 36, Spin Doctor 24), Corp wins 46 → 45; on the full seed-2 pass Carnivore 1 → 61, Gourmand 14 → 168, off the blind list; a held console's "+1[mu]" was tried and reverted, since the hand holding it at half weight left the install nothing (Hermes 29 → 3 installs in 48 paired games); the self-pairing +0.039 / −0.016 / +0.003 on three seeds (pooled z +0.76) (`feat/an-access-trash-is-worth-a-trash-a-turn`, 7 October 2026).
- **§51** — The tag's leverage is every Corp's, read off the punisher in HQ and not off the kill plan: Fine Print, Hyper Velocity, Gimbatul and Quick and Dirty are fast-advance decks holding Bigger Picture, IP Enforcement, Retribution and Orbital Superiority, and the planner had Public Trail legal on 140 turns of 96 games and played it once; now 10 (the Runner paid 8[c] 4 times, took the tag 6), and on the full seed-2 pass Public Trail 0 → 6, Retribution 0 → 2, Oppo Research 1 → 6 — the Corp side of the strict blind list is empty; the self-pairing +0.005 / −0.031 / +0.023 on three seeds (pooled z −0.11), discordant games only in decks that hold a punisher; a "before" built before a `git pull` was a different pool and was thrown away (`feat/tag-leverage-read-off-the-cards`, 7 October 2026).
- **§50** — Byte! is not blind, the instrument was: a hand trap's use is the spring, which emits no event naming the card, so `diag precepts` counted a hand trap as used only when installed — and §20 keeps it in HQ on purpose; `PayAccessTrigger` is now counted off the action, and on a full 391-game pass (seed 2) the planner's Byte! count is 2 → 11 (9 springs; random 72 → 74), ratio 0.15, while in 96 games of its three decks the planner sprang it 21 times and random 4; the strict list is unchanged at 11 cards, Public Trail first (`fix/a-sprung-trap-is-a-use`, 7 October 2026).
- **§49** — Barry "Baz" Wong's install costs his deck nothing, re-measured: the yes taken (473 times in 288 games) against the yes declined every time, on three seeds, paired by game — Corp 163 vs 166, z −0.31, per seed −1.35 / +0.33 / +0.38 — so §39's 35 → 46 on one seed was drift; since §44 one ply takes the same yes (465 of 1,138 offers) without the §39 rule, and Maglectric Rapid's derez is used (62 of 104 offers), so neither is a debt; no code changed (`diag/barry-install-costs-nothing`, 7 October 2026).
- **§46** — A game does not get younger: the stage was read off a board the Corp moves itself, so LEO Construction's trash of HQ's only ICE at a run's initiation read the game as early again and its income as worth more (+4.0, deciding the trade); past game turn 19 — where nine games in ten have left the early stage, and none in a planner pass ever went back — the stage is at least middle; LEO's ICE trashes 1 → 0, the self-pairing inside the band (`fix/stage-not-moved-by-own-ice`, 6 October 2026).
- **§48** — An install's trash picks are searched as sets, not in every order: within one install's picks a candidate is tried only above the last position picked, so ten pieces of ice is 2,045 applications (0.26 s) where it was 16,099,400 (about 49 minutes); every planner pass faster (384 games 283 → 247 s and 390 → 257 s), no win effect beyond the band; the engine still asks a person in their own order (`fix/trash-picks-searched-as-sets`, 7 October 2026).
- **§47** — The ladders after §44, measured: elite against elite the Corp wins 0.570 / 0.547 where it won 0.401 / 0.469; the Corp's steps are still even (every one a rise at 3.0 sd or more), the Runner's are not (`operator → veteran` +0.146 / +0.190, `veteran → elite` +0.057 on seed 2 at 1.6 sd), so the Runner's re-spacing is owed; and the trash-first payment search costs a(n) = n·a(n−1) + 2n applications for *n* pieces of ice on the server — 293 s at nine, about 49 minutes at ten — with real play reaching seven (`diag/ladders-after-44`, 6 October 2026).
- **§31** — A run a card's text began is priced with what the text put on it: the run's own credits break ICE and are worth the breaks they cover (Overclock), the rider pays on success (Clean Getaway, Red Team's run, Jailbreak's draw and access), the breach is of the server the run approaches (Maintenance Access), an armed prevention passes the first unbreakable piece (Shred) and a rez tax is what the forced rez costs (Tread Lightly) — Overclock 0 → 58, Clean Getaway 0 → 46, Shred 0 → 25, Maintenance Access 0 → 7 plays over a planner pass of the pool, the four off the blind list; the self-pairing moved within the band in opposite directions on two seeds (`feat/run-riders-at-the-leaf`, 2 October 2026).
- **§30** — A program hosted on Madani is one turn from the table (its install delta less a click) and the grip promises its installer half a click: the planner installs Madani beside programs that wait, hosts on it and installs from it — 1 → 37 installs and 0 → 24 hosts-or-free-installs over 192 games, Madani off the blind list; three readings measured and rejected first, one of them with a beam change that moved every game (`feat/madani-hosts-the-rig`, 2 October 2026).
- **§29** — The cards the planner never plays are the difference between seatings, measured: `scripts/blind_cards.py` over a random pass and a planner pass of `diag precepts` lists every card random seats use at least five times that the planner never does — 26 on `main` at #341, Madani first at 115–163 uses a pass — where "no seat used it" could not see a card the planner installs and never uses (`diag/blind-cards`, 2 October 2026).
- **§28** — A break is priced by what the card holds: a cost in counters or hosted copies is read as the engine charges it and bounded by the stock, budgeted across a server, so a Matryoshka with nothing hosted breaks nothing and the planner hosts a copy ahead of a run worth the two clicks — 0 → 11 hosts over 144 Hit List games; the evaluator had priced only `Cost::Credits` and read every `BreakSubroutines` as coverage (`feat/breaks-priced-by-what-the-card-holds`, 2 October 2026).

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

### The ladders after §44 (§47)

The latest tables, taken as Stage 8's were (below) on `main` at `0a1cb81`, 6 October 2026; reports
under `target/coverage/ladder44/`. The Corp's steps are even; the Runner's are owed a re-spacing (Open).

| rung | Corp, seed 1 / 2 | Runner, seed 1 / 2 | `epsilon` (Corp / Runner) |
|---|---|---|---|
| novice | 0.013 / 0.016 | 0.036 / 0.034 | 1.00 / 1.00 |
| apprentice | 0.156 / 0.128 | 0.107 / 0.094 | 0.22 / 0.45 |
| operator | 0.273 / 0.284 | 0.188 / 0.206 | 0.11 / 0.25 |
| veteran | 0.440 / 0.440 | 0.333 / 0.396 | 0.05 / 0.10 |
| elite | 0.570 / 0.547 | 0.430 / 0.453 | 0.00 / 0.00 |

A seed of the square cost 3,754 s alone on 20 threads.

### The ladders on the planner (§25 Stage 8)

Every rung is the turn planner at a handicap since 30 September 2026, and these are the tables a
ladder session starts from: 384 games a cell on each of two seeds, every seat in its deck's own
style, each rung against the un-handicapped planner on the other chair (`bench --bots
level:novice,level:apprentice,level:operator,level:veteran,level:elite --games 384 --deck-styles`,
`ladder_report.py --reference level:elite`; reports under `target/coverage/stage8/`). The full
entry, with the Runner's first cut and why it was re-spaced, is §25 Stage 8 in the archive.

| rung | Corp, seed 1 / 2 | Runner, seed 1 / 2 | `epsilon` (Corp / Runner) |
|---|---|---|---|
| novice | 0.018 / 0.008 | 0.034 / 0.039 | 1.00 / 1.00 |
| apprentice | 0.143 / 0.122 | 0.188 / 0.188 | 0.22 / 0.45 |
| operator | 0.224 / 0.224 | 0.344 / 0.320 | 0.11 / 0.25 |
| veteran | 0.315 / 0.328 | 0.435 / 0.424 | 0.05 / 0.10 |
| elite | 0.401 / 0.469 | 0.599 / 0.531 | 0.00 / 0.00 |

Every step a rise on each seed at 2.6 sd or more. The Corp's handicaps are §21's, unchanged; the
Runner's were 0.75 / 0.50 / 0.25 for one ply, whose curve was a line, and the planner's curve is
steep near zero like the Corp's. A seed of the square costs 31–37 minutes on 20 threads.
