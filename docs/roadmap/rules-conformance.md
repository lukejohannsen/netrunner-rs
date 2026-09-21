# Rules Conformance — the engine and the board against the Comprehensive Rules

**Checked against v26.03** (effective 02 March 2026). The copy is
[`rules/comprehensive-rules.md`](../../rules/comprehensive-rules.md), © Null
Signal Games ([NOTICE](../../rules/NOTICE.md)). When `rules/manifest.json`
names a later version than this line, every status below is stale until it
has been re-read against the rules that version changed.

The Rules Audit ([rules-audit.md](rules-audit.md)) was organised by the
engine's gaps, and it read the rules where a gap led it. This file is
organised by the rules, so the question "have we read section X against the
engine?" has one answer, in one place.

## How this is kept

- **`scripts/rules_sync.py` owns the copy.** It writes
  `rules/comprehensive-rules.md` (one line per rule, number and anchor first,
  so a grep lands on one rule) and `rules/manifest.json` (each rule's number
  and a hash of its text, keyed by the page's anchor, which does not change
  when rules are renumbered). Never edit either by hand.
- **`--check` is the watch.** `.github/workflows/rules-watch.yml` runs it
  every Monday and opens an issue when Null Signal Games' page differs. The
  report names the rules added, removed, changed or renumbered, and every
  `CR <number>` in the workspace that points at one of them.
- **Adopting a version is one PR:** run the script, fix what
  `rules_citations` names, re-read each changed rule against the engine,
  update the statuses below and the version line above.
- **Citations are checked.** `crates/netrunner_core/tests/rules_citations.rs`
  fails on any `CR <number>` or `Comprehensive Rules <number>` the committed
  copy does not contain. Its first run found one: Priority Construction's
  example was cited as 1.12.3a and is 1.12.2a.

**Statuses.** *conforms*: read rule by rule against the engine, and it
matches. *read in part*: some rules were read (the notes say which), and the
section as a whole has not been. *deviates*: a known difference, with the
fix owed. *not modelled*: no card in the pool needs it. *n/a*: nothing for
software to implement. *unreviewed*: not yet read. Adding a "Cited: …" note
to a row does not make it *conforms*; a row gets that status only after its
section has been read end to end.

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
- **D3 Window permissions.**
  - A non-ice rez is allowed in the encounter window, which is (P) only (6.9.3b).
  - The initiation window is missing (6.9.1e).
  - The Corp gets no rez window after most Runner actions (5.7.1e).

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
  - Zwicky's "may" is forced.
  - An operation's reactions resolve after its play abilities rather than before (8.6.7).
  - The operation is filed in Archives before it resolves.

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
7. D2 and D3: the run and access windows as 6.9 and 7.2 list them.

Each follows the Testing Rule's bar for engine work. Each also moves its rows below to *conforms*,
with the rule quoted.

## Sections

### 1. Game Concepts

| § | Section | Status | Notes |
|---|---|---|---|
| 1.1 | General | unreviewed |  |
| 1.2 | Golden Rules | unreviewed |  |
| 1.3 | Symbols | n/a | Symbols. The house spellings (`[click]`, `[credit]`…) are how `rules_sync.py` renders them. |
| 1.4 | Deck Construction | conforms | Read rule by rule (21 September 2026) and G1–G2 fixed: the agenda band is `min`–`min + 1` (1.4.6), an identity cannot be a deck card (1.4.4), a *Learn to Play* identity is legal only with its published lists (1.4.1a, `DeckFile::validate`), and both validators read a card's own copy limit (1.4.7). Every out-of-faction non-agenda card in the catalog prints an influence cost, so 1.4.4's last clause holds for the pool. 1.4.8 (tournament rules) is n/a. |
| 1.5 | Extra Cards | unreviewed |  |
| 1.6 | Starting the Game | read in part | Audit: 5 credits, 5 cards, Corp mulligans first, Corp goes first; matches. |
| 1.7 | Ending the Game | deviates | E3 only, which no card reaches. E1 and E2 fixed. 7 points checked at checkpoints matches. |
| 1.8 | Cards | unreviewed |  |
| 1.9 | Counters and Tokens | unreviewed |  |
| 1.10 | Credits | read in part | 1.10.5 recurring credits: `CardDefinition::recurring_credits`, `payment::place_recurring` / `refill` (Payment Rule). Cited: 1.10.5, 1.10.5a, 1.10.5b. |
| 1.11 | Clicks | read in part | Audit: 3 and 4 clicks match. |
| 1.12 | Objects | read in part | 1.12.2a: an effect acts on the object a card became after it moved (Priority Construction). Cited: 1.12.2a. |
| 1.13 | Host, Hosted, and Hosting | unreviewed |  |
| 1.14 | Ownership and Control | unreviewed |  |
| 1.15 | Targets | unreviewed |  |
| 1.16 | Costs | read in part | Costs are `Cost`, never effects (Rules Audit item 8): 1.16.1a not prevented, 1.16.3 checkpoint after a cost, 1.16.11 nested costs. Cited: 1.16, 1.16.1, 1.16.11, 1.16.1a, 1.16.3. |
| 1.17 | Score, Scoring and Stealing | conforms | F3 fixed: a forfeit lowers the shown score (1.17.1). Scoring conditions match. |
| 1.18 | Advancing Cards | read in part | 1.18.1–1.18.2: placing an advancement counter is not advancing (`PlaceAdvancementCounters`, `listeners`). Cited: 1.18.1, 1.18.2. |
| 1.19 | Trashing | conforms | F4 fixed: a hosted card goes to its owner's discard pile (1.19.1). |
| 1.20 | Memory | unreviewed |  |
| 1.21 | Card Visibility | unreviewed |  |

### 2. Parts of a Card

| § | Section | Status | Notes |
|---|---|---|---|
| 2.1 | Name | unreviewed |  |
| 2.2 | Unique Symbol | read in part | Audit: newest active copy survives, Corp copy trashed faceup; matches. |
| 2.3 | Play Cost, Install Cost, or Rez Cost | unreviewed |  |
| 2.4 | Advancement Requirement | unreviewed |  |
| 2.5 | Agenda Points | unreviewed |  |
| 2.6 | Trash Cost | unreviewed |  |
| 2.7 | Strength | unreviewed |  |
| 2.8 | Memory Cost | unreviewed |  |
| 2.9 | Base Link | unreviewed |  |
| 2.10 | Starting Memory Limit | unreviewed |  |
| 2.11 | Minimum Deck Size | unreviewed |  |
| 2.12 | Influence Limit | unreviewed |  |
| 2.13 | Faction Affiliation | unreviewed |  |
| 2.14 | Influence Cost | unreviewed |  |
| 2.15 | Card Type | unreviewed |  |
| 2.16 | Subtypes | unreviewed |  |
| 2.17 | Text Box | unreviewed |  |
| 2.18 | Server Indicator | unreviewed |  |

### 3. Card Types

| § | Section | Status | Notes |
|---|---|---|---|
| 3.1 | Identities | unreviewed |  |
| 3.2 | Agendas | unreviewed |  |
| 3.3 | Assets | unreviewed |  |
| 3.4 | Ice | unreviewed |  |
| 3.5 | Operations | unreviewed |  |
| 3.6 | Upgrades | conforms | Read for B. A second region trashes the first as part of the install (3.6.5d, 8.5.6a). 3.6.5e (a swap or move putting two regions in one root) has no card in the pool. |
| 3.7 | Events | unreviewed |  |
| 3.8 | Hardware | conforms | Read for B. A second console trashes the older one at the checkpoint, unpreventably (3.8.5b). |
| 3.9 | Programs | conforms | Read for B. An install over the limit trashes programs of the Runner's choice first (3.9.3b). A limit that drops later is the checkpoint's (3.9.3c, `memory::enforce_limit`). Memory cost is not a cost (3.9.3d). |
| 3.10 | Resources | unreviewed |  |

### 4. Game Zones

| § | Section | Status | Notes |
|---|---|---|---|
| 4.1 | General | unreviewed |  |
| 4.2 | Deck | read in part | Audit: the view carries only R&D and stack counts; matches. |
| 4.3 | Hand | read in part | Audit: HQ and grip contents go to their owner only; matches. |
| 4.4 | Discard Pile | deviates | A2 (4.4.2). 4.4.6b: HQ discards and unrezzed installs go facedown, rezzed or revealed ones faceup; matches. |
| 4.5 | Score Area | unreviewed |  |
| 4.6 | Play Area | read in part | A3 closed by B (4.6.6f). The view and desktop mask a root card by identity only, so a slot shows no type. Remotes exist while occupied, and new ice goes outermost; these match. The other board-layout rules (4.6.5c, 4.6.7b–d, 4.6.8c, 4.6.9a) have not been read against the board yet. |
| 4.7 | Bank | unreviewed |  |
| 4.8 | Set Aside | not modelled | No set-aside zone (Rules Audit backlog item 10); masking needs an explicit who-may-see rule first. |
| 4.9 | Remove from the Game | unreviewed |  |

### 5. Turns

| § | Section | Status | Notes |
|---|---|---|---|
| 5.1 | General | unreviewed |  |
| 5.2 | Actions | unreviewed |  |
| 5.3 | Draw Phase | unreviewed |  |
| 5.4 | Action Phase | unreviewed |  |
| 5.5 | Discard Phase | conforms | E2 fixed: a hand size below 0 flatlines at the discard step. HQ discards go facedown; matches. |
| 5.6 | Steps of the Corp's Turn | deviates | D3 only: what each window permits. C1, C2 and C3 fixed; the steps run in the listed order. |
| 5.7 | Steps of the Runner's Turn | deviates | D3 only (5.7.1e). C1, C2 and C3 fixed; the steps run in the listed order. |

### 6. Runs

| § | Section | Status | Notes |
|---|---|---|---|
| 6.1 | General | unreviewed |  |
| 6.2 | Position | unreviewed |  |
| 6.3 | Initiation | read in part | Audit: bad publicity credits arrive before the run begins and go when it ends; matches. |
| 6.4 | Approach Ice | unreviewed |  |
| 6.5 | Encounter Ice | unreviewed |  |
| 6.6 | Movement | unreviewed |  |
| 6.7 | Success | unreviewed |  |
| 6.8 | Run Ends Phase | unreviewed |  |
| 6.9 | Steps of a Run | deviates | D2, D3. The movement phase (6.9.4c–g), an iceless server and unrezzed ice passed match. |

### 7. Accessing Cards and Breaching Servers

| § | Section | Status | Notes |
|---|---|---|---|
| 7.1 | Accessing Cards | conforms | D1 fixed: nothing in Archives is trashed (7.1.5b). Steals mandatory, steal costs declinable (7.1.6a); matches. |
| 7.2 | Steps of Accessing a Card | deviates | D2. |
| 7.3 | Breaching Servers | deviates | A1 (7.3.4a). Archives turned faceup at breach (7.3.2); matches. |
| 7.4 | Determining Candidates | deviates | A1 (7.4.7). |
| 7.5 | Steps of Breaching a Server | unreviewed |  |

### 8. Card Manipulation

| § | Section | Status | Notes |
|---|---|---|---|
| 8.1 | Faceup and Facedown Status | not modelled | 8.1.4: facedown Runner installs (Rules Audit backlog item 10). |
| 8.2 | Card Movements | unreviewed |  |
| 8.3 | Arranging and Rearranging Cards | unreviewed |  |
| 8.4 | Drawing Cards | unreviewed |  |
| 8.5 | Installing and Uninstalling Cards | deviates | B4 only. B1–B3 fixed: like cards are trashed at 8.5.16c, before the install cost, with the forced ones unasked and the player's own behind `trash_first`. 1[c] per ice not counting trashed ice, trashed cards keep their status (8.5.7), and a server emptied by its own install's trash keeps its identity (8.5.9); these match. |
| 8.6 | Playing Events and Operations | read in part | F6 (order only). |
| 8.7 | Searching for Cards | unreviewed |  |
| 8.8 | Swapping Cards | unreviewed |  |

### 9. Abilities

| § | Section | Status | Notes |
|---|---|---|---|
| 9.1 | General | read in part | 9.1: a card's abilities work while it is active (`rules::active`). Cited: 9.1. |
| 9.2 | Timing and Priority | read in part | F1 fixed: no action in a paid ability window (9.2.7b). Active player first and own-order simultaneous triggers match. |
| 9.3 | Interpreting Card Text | unreviewed |  |
| 9.4 | Static Abilities | unreviewed |  |
| 9.5 | Paid Abilities | read in part | F1 fixed: a [click] ability is an action (9.5.2a), taken only in its user's action window. Cited: 9.5.2a. |
| 9.6 | Conditional Abilities | read in part | Audit: a trigger whose source left is dropped, a resolving ability finishes; matches. |
| 9.7 | Play Abilities | unreviewed |  |
| 9.8 | Subroutines | read in part | Audit: unbroken subroutines in printed order, stopping at an ended run; matches. 9.8.2–9.8.3 (new in v26.03): no pool card adds subroutines. |
| 9.9 | Interrupts and Replacement Effects | read in part | Interrupts and prevention: `rules::prevention` (the Prevention Rule, Rules Audit item 4). Expose is deferred. |
| 9.10 | Lingering Effects | read in part | Lingering effects: `rules::lingering` (the Continuous Effect Rule). |
| 9.11 | Identifying Instructions | unreviewed |  |
| 9.12 | Other Rules and Terminology | conforms | F5 fixed: a gain of 0 does not take place (9.12.2b). |

### 10. Additional Rules

| § | Section | Status | Notes |
|---|---|---|---|
| 10.1 | General | unreviewed |  |
| 10.2 | Information | unreviewed |  |
| 10.3 | Checkpoints | deviates | E3. B2 fixed: 10.3.1d's console half is `checkpoint::enforce_consoles`. The step order (durations, win, unique, triggers) matches. |
| 10.4 | Damage | read in part | F2 fixed: the responsible player is recorded (10.4.1). Flatline, core-damage hand size, random trash match. |
| 10.5 | Tags | read in part | Audit: both tag basic actions check tags and cost [click] + 2[credit]; matches. |
| 10.6 | Bad Publicity | read in part | Audit: bad publicity credits are a separate run pool fixed at run creation (10.6.3c); matches v26.03. |
| 10.7 | Link | read in part | Audit: link is identity plus installed; matches. |
| 10.8 | Traces | read in part | `Effect::Trace` exists, but no card in the pool starts a trace, so no agent reaches it. |
| 10.9 | Load and Empty | unreviewed |  |
| 10.10 | Charge | not modelled | Charge — Rules Audit backlog item 10. |
| 10.11 | Mark | not modelled | Mark — Rules Audit backlog item 10. |
| 10.12 | Sabotage | read in part | `Effect::Sabotage`; not yet read against 10.12. |
| 10.13 | Dividends | read in part | `CardDefinition::dividends`; not yet read against 10.13. |
| 10.14 | Bidding | unreviewed |  |

### 11. Appendix: Timing Structure Reference

| § | Section | Status | Notes |
|---|---|---|---|
| 11.1 | Overview | n/a | A quick reference to the timing structures of chapters 5–7; conformance is recorded there. |
| 11.2 | Timing Structure of The Corp's Turn (Section 5.6) | n/a | A quick reference to the timing structures of chapters 5–7; conformance is recorded there. |
| 11.3 | Timing Structure of The Runner's Turn (Section 5.7) | n/a | A quick reference to the timing structures of chapters 5–7; conformance is recorded there. |
| 11.4 | Timing Structure of a Run (Section 6.9) | n/a | A quick reference to the timing structures of chapters 5–7; conformance is recorded there. |
| 11.5 | Timing Structure of Breaching a Server (Section 7.5) | n/a | A quick reference to the timing structures of chapters 5–7; conformance is recorded there. |
| 11.6 | Timing Structure of Accessing a Card (Section 7.2) | n/a | A quick reference to the timing structures of chapters 5–7; conformance is recorded there. |
