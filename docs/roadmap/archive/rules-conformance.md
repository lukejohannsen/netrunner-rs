# Rules Conformance — the findings' record

Moved verbatim from [`docs/roadmap/rules-conformance.md`](../rules-conformance.md) on 29 September 2026; the live file keeps one line per finding and the ledger. Letters are the originals, so "Rules Conformance B1" resolves here.

## Findings

A reading of chapters 1–10 against `crates/netrunner_core` on 21 September 2026, done as three
audits: chapters 1–4, 5–7 plus 11, and 8–10. **✔** marks a finding re-read in the code when it was
recorded. The rest carry the auditor's citation and are re-checked as the first step of the PR that
fixes them. The letters are this file's addresses: "Rules Conformance B1".

**A. Hidden information: the Runner sees what the rules hide**
- **A1 ✔ The Runner sees HQ and R&D cards before accessing them** (7.3.4a, 7.4.7).
  - The breach picks every random HQ card and the top N of R&D up front (`run/access.rs`).
  - `masking::mask_run_state` shows the Runner the whole pick (`card_visible`), so the
    `SelectNextCard` choices name cards not yet accessed.
  - The rules offer HQ "a random candidate", chosen one at a time, and R&D one card at a time,
    top down.
  - Reached with The Maker's Eye, Conduit, or any upgrade in a central root.
- **A2 ✔ Archives keeps arrival order** (4.4.2, "Discard piles are not ordered").
  - `CorpState::archives` is a `Vec`, and `PublicArchivedCard` keeps each facedown card's place.
  - The breach turns cards faceup where they sit, so the Runner can match every earlier facedown
    card to when it arrived.
  - This answers the question Rules Audit backlog item 10 asked: the Runner is not entitled to
    that order. The heap has the same order but is all public, so there it breaks only the
    wording of the rule.
- **A1 and A2 fixed** (`fix/access-one-at-a-time`).
  - **A candidate is named the way the Runner can point at it** (`run::AccessCandidate`):
    `Root(InstallId)` for a card in the root, `Zone` for "a random card from HQ" (7.3.4a) or
    "the top card of R&D" (7.4.7), and `Archived(CardId)` for a card in Archives, faceup since
    the breach began (7.3.2). `SelectCardToAccess` carries one, so the action names no hidden
    card and is shown whole in the other seat's log; `ConcealedAction::SelectCardToAccess` is
    gone.
  - **The HQ and R&D cards the breach will reach are in `AccessState::from_zone`, known to
    neither player.** A view carries only their number. They are drawn when the breach begins,
    when the random access limit is set (7.3.5): HQ's at random, R&D's from the top. Choosing
    `Zone` takes the next. With one candidate left there is nothing to choose, and it is
    accessed unasked, so a breach of R&D with an empty root presents its cards one at a time,
    top down, as 7.4.7 says. The Runner used to choose among the top three by name.
  - **The unrezzed cards in the root were named too.** A1 was filed about HQ and R&D, but
    `card_visible` showed the Runner every root card in the `SelectNextCard` list, rezzed or not.
    `Root(InstallId)` fixes it with the rest.
  - **A candidate that has left the server is no longer offered** (7.4.5). A root install
    trashed mid-breach used to be accessed anyway, as whatever the zone fallback found under its
    name. And only accesses performed are counted (7.3.6), for Zahya Sadeghi's credits: the count
    was the candidates the breach began with.
  - **A2: anyone but the Corp sees Archives in name order** (`masking::mask_archives`): the
    faceup cards by name, then the facedown ones. The breach offers Archives' cards in the same
    order. `CorpState::archives` itself is untouched, because the Corp's choices among its own
    Archives cards are positions into it, and no action of the Runner's is.
  - **The bots' samples draw `from_zone` from the sample's own HQ or R&D**
    (`determinize::draw_from_zone`). The Runner's sample used to see exactly which cards were
    coming.
  - **Measured** (`scripts/coverage_identical.py main`, 192 games a report, seed 1).
    `SelectCardToAccess` falls where a breach no longer asks: random-vs-random 1140 → 1096 (by
    view and by index alike), heuristic by view 701 → 656, because R&D with an empty root has
    nothing to choose. Accesses barely move (random 2462 → 2477, heuristic by view 4148 →
    4108), and neither do steals (random 480 → 476, heuristic 593 → 602). Everything else moves
    by trajectory drift: the Runner's choices differ, so every game after the first breach
    rolls differently. Both 256-seed sweeps are clean at release, coverage gate included, and
    the view sweep is clean at 64 seeds in a debug build, where `dispatcher::audit` runs.
- **A3 Installing over a remote's root shows the old card's type** (4.6.6f).
  - The only install-time trash is the forced one of an agenda or asset (see B3).
  - So a card going to Archives means both cards were agendas or assets, and no trash means at
    least one of them is an upgrade.
  - **Closed by B** (`fix/install-trashes-cr-8-5-6`): the Corp may now trash any card in the
    root first (8.5.6a), so a trash no longer says what either card is. What is left, "no trash,
    so one of them is an upgrade", is the rules' own: 8.5.6a forces the trash at any table, and
    the engine shows no more than a table does. The facedown card's name stays masked in the
    event and in Archives.

**B. Installing (8.5.6): what the rules make a trash is a refusal** (fixed, B4 remains)
- **B1 ✔ A program over the memory limit is refused** (3.9.3b, 8.5.6c: the Runner trashes
  programs to make room). `engine::require_memory_for` returns `InsufficientMemory`. Reached in
  basic play.
- **B2 ✔ A second console is refused** (3.8.5b, 10.3.1d: all but the newest console are trashed).
  `ConsoleLimitExceeded`, and the checkpoint module's notes call this deliberate. Seven consoles
  are in the pool.
- **B3 The Corp's optional install trashes are never offered.**
  - Root cards (8.5.6a): not offered.
  - An existing region is refused where the rules trash it (3.6.5d, `RegionLimitExceeded`).
  - Ice on the server (8.5.6b, and trashed ice is not counted in the install cost): not offered.
  - Reached on every ice install.
- **B fixed** (`fix/install-trashes-cr-8-5-6`), in a module of its own, `rules::install_trash`,
  run at step 8.5.16c: after the destination, before the install cost.
  - **B1:** a program over the limit is installed, and the Runner trashes programs of their
    choice until it fits. "When the Runner installs a program that would increase the total
    memory cost of installed programs over their memory limit, they must trash one or more
    installed programs such that the total memory cost of installed programs including the new
    program will not exceed their memory limit" (3.9.3b). They are asked one program at a time
    (`payment::Ask::Install`), by the replay a payment is parked by. With one program there is
    one answer, and it is taken unasked. The new program is never a choice, because it is not yet
    installed (8.5.16a). `InsufficientMemory` is left only for a program that would not fit with
    every program gone.
  - **B2:** "If a player ever controls more than one installed console, all but the most
    recently active console are trashed. Trashing cards this way cannot be prevented" (3.8.5b).
    That is `checkpoint::enforce_consoles`, beside the ◆ rule it shares 10.3.1d with.
    `ConsoleLimitExceeded` is gone.
  - **B3:** "the Corp may first trash any number of other cards already installed in the root of
    that server … If the card to be installed is a region, the Corp must trash any other region
    from that server" (8.5.6a), and "the Corp may first trash any number of other ice already
    installed protecting that server. Ice trashed in this way will not be counted when
    determining the install cost of the new ice" (8.5.6b). The forced trashes are made unasked,
    since there is at most one of each. `RegionLimitExceeded` is gone.
  - **What the player may trash is a separate entry** (decided with the person, 21 September
    2026). Asked on every install, the question would come many times a game with "none" almost
    always the answer. So `InstallCard`, `InstallProgram` and `InstallProgramOnIce` carry
    `trash_first`. The plain install makes only the forced trashes. The install that trashes
    first is offered only where there is a card it could choose. It takes at least one, so the
    two entries are never the same move (`RulesError::NothingToTrashFirst`). After the first,
    "Trash no more" is an answer. `ActionSpace` 1677 → 2621, appended: the three install segments
    again, and no index moved.
  - Trashed cards go where 8.5.7 puts them: a Corp card to Archives as it was on the table, a
    Runner card to the heap with what it hosted. Nothing trashed here is prevented.
- **B4 An install by a card's text makes only the forced trashes.** 8.5.6 applies to every
  install. A card's text has no entry to carry `trash_first`, so the Corp cannot, for example,
  trash ice first when Mercia B4LL4RD or Ansel 1.0 installs one. No card in the pool installs
  over something on purpose, so this is recorded rather than owed. The forced trashes (memory,
  region, agenda or asset) apply to text installs as to any other. The memory question is asked
  inside a text install by replay too (`install_trash::could_ask`).

**C. The turn (5.6, 5.7)**
- **C1 ✔ The Corp draws before its turn begins.** `turn::enter_start_of_turn` draws, then refills
  recurring credits, then fires turn-begins abilities.
  - 5.6.1b–e: a (P)(R)(S) window, then the refill, then the turn begins, then the draw.
  - Au Co.'s look at the top 3 of R&D sees the wrong three.
  - Neither side has its draw-phase window (5.6.1b, 5.7.1b).
  - **Fixed** (`fix/turn-structure-cr-5-6-5-7`): clicks, a new `WindowCheckpoint::TurnBeginning`
    window, the refill, `TurnStarted`, the Corp's draw, then the window ahead of the first action.
    A Corp with an empty R&D now loses at the draw, after its turn has begun.
  - **Fixed again (TAI Stage 1, 28 September 2026):** the draw came after the turn-begins
    abilities were *dispatched*, not after they *resolved*. One that parked a decision (AU Co.'s
    dig, Clearinghouse's choice, Charlotte Caçador's and Cohort Guidance Program's) drew and opened
    the window underneath it, so AU Co. still looked at the three after the drawn card; its test
    said so ("the mandatory draw took Ice Wall"). The rest of the step is now owed while the phase
    is `StartOfTurn` with no window open, and paid when nothing is parked
    (`turn::finish_turn_beginning`, called by `engine::apply_action`). Found by Balanced Coverage,
    whose look at the top of R&D is the point of the card.
- **C2 End-of-turn order.** The rules go discard, then the window, then the clicks are lost
  (5.6.3, 5.7.2). The engine zeroes the clicks first. Cosmetic. **Fixed**: the action phase
  ends, the discard, the window, the clicks are lost, then `TurnEnded` and `DiscardPhaseEnded`.
- **C3 The turn can be ended with clicks left** (5.6.2b, 9.2.6b: a player must take an action while
  clicks remain). Decided 21 September 2026: enforce the rule. `EndTurn` stops being legal while
  clicks remain, and the clients' "ask twice" path goes. **Fixed**: `RulesError::ClicksRemain`;
  the desktop's Enter no longer arms, and AGENTS.md says so.

**D. Runs and access (6.9, 7.1, 7.2)**
- **D1 ✔ The Runner can pay to trash a card in Archives** (7.1.5b forbids it). Neither
  `legal_actions` nor `resolve_trash` checks, nor does the Gourmand/Carnivore path.
  **Fixed** (`fix/small-rules-deviations`): `access::accessing_in_the_discard_pile` withholds
  the trash cost and refuses the free trash; an upgrade in Archives' root is still trashable.
- **D2 The Corp can rez during access and after success.**
  - A normal (P)(R) window opens at each accessed card and after "successful".
  - Access has only the Runner's mid-access window (7.2, 9.2.10, 11.6).
  - Gourmand and Carnivore depend on the present window, so a real mid-access window must
    replace it.
  - **Fixed** (`fix/run-and-access-windows`): `CompleteRun` declares the run successful and the
    breach follows at the end of the action, or of the action that answers a decision a "when
    successful" ability parked (`RunState::declared_successful`, carried in the view so a bot's
    sample breaches too). No window opens at an accessed card. A breach refuses every paid ability
    but an interrupt (`RulesError::NotPermittedInThisWindow`), where the Runner's standing
    permission in their own action phase used to reach. The mid-access window is the decision
    about the card: "Access →" is a flag on the ability (`AbilityDef::access`, CR 9.3.6b), usable
    only by the Runner at `PendingChoice` (`RulesError::NotInMidAccessWindow`), and
    `CurrentlyAccessingACard` is gone (`EffectRequirement` 35 → 34). Every mid-access ability in
    the pool finishes the access, so "a single" one (9.2.10c) needs no count.
- **D3 Window permissions.**
  - A non-ice rez is allowed in the encounter window, which is (P) only (6.9.3b).
  - The initiation window is missing (6.9.1e).
  - The Corp gets no rez window after most Runner actions (5.7.1e).
  - **Fixed** (`fix/run-and-access-windows`): `paid_ability::window_permits_rez` refuses a non-ice
    rez in the encounter's window. The initiation's window opens once the run has begun and
    nothing is parked (`engine::resume_run`, which covers a run a card's text starts), and closing
    it takes the step `ContinueRun` from `Initiation` used to. The post-action window opens for a
    Corp that could rez a non-ice card it can pay for, not only for a Corp with a paid ability,
    and after a run ends, since the run was the action (CR 5.2.2a).
- **D4 The movement phase has no window before the jack-out decision** (6.9.4b, paid abilities
  only). Found while fixing D2 and D3; recorded, not owed. The Runner already uses paid abilities
  there, as the active player outside a window. Only the Corp's are missing, and a window before
  every jack-out decision would cost two passes per piece of ice passed. Opening it needs the
  movement phase to tell a third moment apart from the two `jack_out_permitted` already
  distinguishes.
  **Fixed** (`fix/jack-out-window`), with no third moment stored. The window opens when the run
  arrives in the movement phase and the Corp has a paid ability it could use
  (`paid_ability::has_usable_paid_ability`); the Runner needs none, as the active player outside
  a window. Closing it leaves the decision with the Runner, and nothing reopens it, because every
  caller asks only when the run has just arrived. `JackOut` waits for it to close, and it admits
  no rez (`window_permits_rez`). The six access handlers' calls to `open_window_if_at_checkpoint`,
  dead since D2, are gone.
  Measured (`coverage_identical`, 192 games per shape, seed 1): windows opened 20,860 → 21,069
  random and 29,423 → 31,089 heuristic, where the Corp rezzes more assets with paid abilities.
  Jack-outs 1,790 → 1,783 random. The heuristic's outcomes move by 3 games of 192 and are not
  claimed. Both 256-seed sweeps are clean at release, coverage gate included.
- **Measured, D2 and D3** (`scripts/coverage_identical.py main`, 192 games per shape, seed 1):
  - The view and index paths stay identical to each other, and `ActionSpace` is unmoved.
  - **Windows about balance:** random `PaidAbilityWindowOpened` 20,997 → 20,744. The initiation's
    windows and the new post-run and rez windows roughly replace the success window and the
    per-card access windows. `ContinueRun` 5,954 → 1,744, since the initiation's step is now two
    passes. Random steps 79,067 → 74,000.
  - **The mid-access window is reached:** random Gourmand activations 6 → 2 and Carnivore's
    9 → 8. `TrashAccessedCard` 132 → 131.
  - **Heuristic, not claimed.** Corp point wins were 26 → 36 on seed 1, just past the
    0.026–0.047 band, but 30 → 30 on seed 2 and 30 → 35 on seed 3: +0.026 on average, inside the
    band.
  - Both 256-seed sweeps are clean at release, coverage gate included. The view sweep is also
    clean at 128 seeds in a debug build, where the dispatch audit and the payment assertions run.

**E. Winning and losing (1.7)**
- **E1 A card that makes the Corp draw from an empty R&D does not end the game** (1.7.2c).
  `Effect::DrawCards` stops silently. Reached with Sprint, Spin Doctor and Anthill Excavation
  Contract. **Fixed** (`fix/small-rules-deviations`): the Runner wins; the Runner's own draw
  still stops silently.
- **E2 A maximum hand size below 0 does not flatline** (1.7.2b). It is clamped at 0. Reached with
  Bumi 1.0's core damage. **Fixed** (`fix/small-rules-deviations`): the hand size is signed,
  and asked when the Runner's discard step begins.
- **E3 A simultaneous 7-point win is not a draw** (1.7.1a). No card reaches it; recorded only.

**F. Abilities and effects (9, 10)**
- **F1 ✔ [click] abilities can be used inside paid ability windows** (9.5.2a, 9.2.7b: such an
  ability is an action, and windows allow none). `engine::ActionKind`'s notes call this
  deliberate. Reached with Telework Contract and Pennyshaver mid-run. **Fixed**
  (`fix/click-abilities-and-damage-responsibility`): `AbilityDef::is_action` (a trigger cost
  that begins with [click]) classifies `ActivateAbility` as `ActionKind::Action`. So the run
  guard refuses it mid-run (5.2.2a), `activate_ability` refuses it in any window, it counts as
  a finished action, and the opponent's post-action window follows it. A window no longer
  opens for a player whose only usable ability is an action.
- **F2 ✔ Every `DamageTaken` counts as damage the Corp did** (10.4.1). `listeners::moments` treats
  it that way, so Au Co.'s "whenever you do damage" counts Topan's and Semak-samun's self-inflicted
  damage. **Fixed** (same branch): `DamageTaken::responsible` is the side of the card whose text
  dealt the damage, or the Runner for damage paid as a cost. In the pool a Corp card "does"
  damage and a Runner card makes the Runner "suffer" it, so the card's side is the verb's. A
  parked prevention reads it off `source_card`. "Whenever you do damage" hears only its own
  side's.
- **F3 A forfeit does not lower `agenda_points`.** The view, the HUD and bot observations show a
  stale score. The win check recounts, so the result is right. **Fixed**
  (`fix/small-rules-deviations`).
- **F4 Corp cards hosted on Detente go to the heap when it is trashed** (1.19.1: a card goes to
  its owner's discard pile). **Fixed** (`fix/small-rules-deviations`): one
  `ability::trash_hosted_card` for Bling's "trash all hosted cards" and for the host leaving.
- **F5 A "for each" that comes to 0 still gains credits** (9.12.2b, new in v26.03), which triggers
  Zwicky. Edge case. **Fixed** (`fix/small-rules-deviations`): a gain of 0 emits nothing.
- **F6 Recorded only; no outcome changes today:**
  - Zwicky's "may" is forced. **Fixed** (`fix/zwicky-may`): the draw is a `PresentChoice`
    with the declined option empty, as Superconducting Hub's is, and a declined draw is still
    the turn's first time. It is also the one F6 item that changed an outcome, since a forced
    draw can deck the Corp out. Measured: Zwicky's trigger 21 → 20 random, 17 → 21 heuristic,
    192 games per shape; nothing outside Zwicky games moved.
  - An operation's reactions resolve after its play abilities rather than before (8.6.7).
  - The operation is filed in Archives before it resolves. **It changed an outcome with
    Corporate Hospitality** (RWR Stage 1, 27 September 2026): "Add 1 card from Archives to
    HQ" offered the Corporate Hospitality resolving it, a free replay. Its own copy is now
    never eligible while it resolves (`pending_choice::resolving_operation_in`, 8.6.7a); the
    filing itself still happens early, because the end of a resolution parked across actions
    is marked nowhere.

**G. Deck legality (1.4) — fixed (`fix/deck-legality-matches-cr-1-4`)**
- **G1 ✔ The agenda-point range was one point too wide.** `agenda_point_range` returned
  `min + 2`, where 1.4.6 says "18 or 19". It is `min + 1` now.
- **G2 ✔ Three legality gaps, all closed:**
  - An identity inside a deck is refused by both validators (1.4.4).
  - The Gateway identities are legal only with their published starter and boosted lists
    (1.4.1a). This is checked against the lists themselves, because a brought deck names its
    own category.
  - The setup gate reads `deck_limit` (1.4.7).
- Every embedded sample, starter, boosted and sweep deck stays legal. The tests that broke were
  fixtures carrying 20 points in 40 cards. One of them was the index sweep's hand-built System
  Gateway Corp deck, whose third Orbital Superiority became a second Palisade (18 points).
  `NETRUNNER_SWEEP_SEEDS=256` on the index sweep afterwards: clean, coverage gate included.

**Measured, D1–F5** (`scripts/coverage_identical.py main`, 192 games a report, seed 1): all
four reports differ, as any change to play re-rolls them. Two deltas are the fixes themselves:
- **The heuristic Corp stopped forfeiting:** `AgendaForfeited` 14 → 0 by view and 13 → 0 by
  index, with Greenmail's `OnForfeit` 7 → 0 and 6 → 0. This is F3 reaching the bots. The
  evaluator scores `agenda_points` (`eval.rs`), so a forfeit to rez Biawak or Plutus was free
  while the score kept the agenda, and it now costs the agenda's points. The random seatings
  forfeit as before, so the event is still reached.
- **The Zwicky Group draws less** (F5): 31 → 28 random, 40 → 30 and 38 → 30 heuristic.

The rest is trajectory drift (steps 76,818 → 77,338 random). Both 256-seed sweeps are clean,
coverage gate included.

**Measured, C1–C3** (`scripts/coverage_identical.py main`, 192 games a report, seeds 1–3):
- **The random Corp scores about twice as often:** `AgendaScored` 13 → 29, 16 → 31 and
  15 → 25. That is C3. A random Corp used to pick `EndTurn` among its options and gave up
  about a quarter of its clicks (2.22 click actions a turn, 2.71 now), so it advanced less
  (`AdvanceCard` 523 → 708 on seed 1). The random Runner decks out less for the same reason
  (26 → 21, 22 → 13).
- **The heuristic seatings show nothing:** `AgendaScored` 183 → 145, 158 → 154 and 150 → 172,
  and Corp point wins 32 → 22, 24 → 31 and 24 → 28, with opposite signs across seeds. The
  heuristic Corp already spent its clicks (2.95 of 3 a turn before and after).
- **Steps rise about 9% heuristic, 15% random:** the new window adds two passes at each
  turn's start (`PaidAbilityWindowOpened` 23,295 → 27,502 heuristic, seed 2).

Both 256-seed sweeps are clean, coverage gate included.

**Measured, F1–F2** (`scripts/coverage_identical.py main`, 192 games a report, seeds 1–3):
- **Card abilities are used about a third as often by the random seatings:** `ActivateAbility`
  3902 → 1335, 4024 → 1298 and 3725 → 1267. That is F1. Smartware Distributor (1258 → 97),
  Madani (824 → 200), Pennyshaver (312 → 29) and Regolith Mining License (281 → 70) were
  mostly used in windows and mid-run, where a random seat met them at every priority pass.
- **The random Runner runs instead, and games are shorter.** On seed 3, `InitiateRun` went
  3530 → 4082. Across the three seeds, turns fell 4639 → 3911, 4575 → 3998 and 4308 → 3718.
  Runner point wins rose 85 → 104, 79 → 100 and 89 → 101, and Runner deck-outs fell 30 → 9,
  21 → 6 and 13 → 3. So the random Corp has fewer turns and scores less: `AgendaScored`
  29 → 10, 31 → 14 and 25 → 11. The C3 measurement above had doubled it. This one undoes a
  distortion of the same kind, clicks spent at the wrong time.
- **The heuristic Runner never uses Smartware Distributor now** (672 → 0, seed 3). It only
  ever used it inside windows, where the alternative was a pass. In its own action window the
  one-ply evaluator ranks the click below every other action, because it does not value
  credits held on a card. That is a bot blindness, owed to Phase 5, not an engine deviation.
  Heuristic Corp point wins fell on all three seeds (22 → 19, 31 → 21, 28 → 22), with the
  Runner's clicks going to runs (`InitiateRun` 3650 → 3760, seed 3).
- **AU Co. counts less where it counted the Runner's damage (F2):** `OnDamageDealt` 35 → 28,
  28 → 15 and 45 → 34 random. Heuristic moved 24 → 27, 24 → 27 and 31 → 23, trajectory drift
  in both directions.

Both 256-seed sweeps are clean, coverage gate included.

**Measured for B**, one full pass of the pool (192 games), main against the branch on pinned
binaries (`scripts/coverage_identical.py`). The random seating reads the same by view and by
index.
- **The random seat trashes like cards now.** `CardTrashed` 1048 → 1653: it picks the install
  that trashes first about as often as any other entry, and then trashes at random.
- **A full rig makes room instead of stopping.** Random `ProgramInstalled` 435 → 503 and
  `InstallProgram` 325 → 399. Heuristic `ProgramInstalled` rose on all three seeds: 266 → 279,
  280 → 282 and 267 → 277. That is B1: the fifth program used not to be offered at all.
- **A second console goes in.** Random `HardwareInstalled` 133 → 150.
- **The heuristic never takes an install that trashes first**: 0 of 192 games on seed 1, by the
  games' own records. Its one-ply evaluator scores what a card is worth on the table, and a
  trash only removes one. Its other moves are trajectory drift. The extra legal entries re-roll
  its tie-breaks, and Corp point wins 19 → 29 on seed 1 did not hold on seeds 2 and 3 (21 → 21,
  22 → 20).
- **Nothing is refused that the rules allow.** `MemoryLimitExceeded` stays 0 in every shape.
  The checkpoint's memory rule (3.9.3c) is still reached only when memory drops later, and no
  install goes over the limit any more.

Both 256-seed sweeps are clean at release, coverage gate included. At 128 seeds they are also
clean in a debug build, where `engine::apply_action`'s assertion checks that `payment::could_ask`
foresaw every question an install asked, text installs included.

**Fix order, one PR each:**
1. G, which does not touch play.
2. D1, E1, E2, F3, F4, F5.
3. C: the turn as 5.6 and 5.7 list it.
4. F1 and F2.
5. B: installing as 8.5 lists it, which also closes A3.
6. A1 and A2: access one card at a time, and an Archives with no order.
7. D2 and D3: the run and access windows as 6.9 and 7.2 list them. **Done**, then D4 and F6's
   first item (Zwicky's "may"). B4, E3 and F6's other two items are recorded, not built: none
   changes an outcome a pool card reaches, and each is a model change (`trash_first` on a text
   install, a drawn game, a play-area zone).

Each follows the Testing Rule's bar for engine work. Each also moves its rows below to *conforms*,
with the rule quoted.

