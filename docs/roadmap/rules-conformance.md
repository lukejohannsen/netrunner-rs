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

**B. Installing (8.5.6): what the rules make a trash is a refusal**
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

**C. The turn (5.6, 5.7)**
- **C1 ✔ The Corp draws before its turn begins.** `turn::enter_start_of_turn` draws, then refills
  recurring credits, then fires turn-begins abilities.
  - 5.6.1b–e: a (P)(R)(S) window, then the refill, then the turn begins, then the draw.
  - Au Co.'s look at the top 3 of R&D sees the wrong three.
  - Neither side has its draw-phase window (5.6.1b, 5.7.1b).
- **C2 End-of-turn order.** The rules go discard, then the window, then the clicks are lost
  (5.6.3, 5.7.2). The engine zeroes the clicks first. Cosmetic.
- **C3 The turn can be ended with clicks left** (5.6.2b, 9.2.6b: a player must take an action while
  clicks remain). Decided 21 September 2026: enforce the rule. `EndTurn` stops being legal while
  clicks remain, and the clients' "ask twice" path goes.

**D. Runs and access (6.9, 7.1, 7.2)**
- **D1 ✔ The Runner can pay to trash a card in Archives** (7.1.5b forbids it). Neither
  `legal_actions` nor `resolve_trash` checks, nor does the Gourmand/Carnivore path.
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
  Contract.
- **E2 A maximum hand size below 0 does not flatline** (1.7.2b). It is clamped at 0. Reached with
  Bumi 1.0's core damage.
- **E3 A simultaneous 7-point win is not a draw** (1.7.1a). No card reaches it; recorded only.

**F. Abilities and effects (9, 10)**
- **F1 ✔ [click] abilities can be used inside paid ability windows** (9.5.2a, 9.2.7b: such an
  ability is an action, and windows allow none). `engine::ActionKind`'s notes call this
  deliberate. Reached with Telework Contract and Pennyshaver mid-run.
- **F2 ✔ Every `DamageTaken` counts as damage the Corp did** (10.4.1). `listeners::moments` treats
  it that way, so Au Co.'s "whenever you do damage" counts Topan's and Semak-samun's self-inflicted
  damage.
- **F3 A forfeit does not lower `agenda_points`.** The view, the HUD and bot observations show a
  stale score. The win check recounts, so the result is right.
- **F4 Corp cards hosted on Detente go to the heap when it is trashed** (1.19.1: a card goes to
  its owner's discard pile).
- **F5 A "for each" that comes to 0 still gains credits** (9.12.2b, new in v26.03), which triggers
  Zwicky. Edge case.
- **F6 Recorded only; no outcome changes today:**
  - Zwicky's "may" is forced.
  - An operation's reactions resolve after its play abilities rather than before (8.6.7).
  - The operation is filed in Archives before it resolves.

**G. Deck legality (1.4)**
- **G1 ✔ The agenda-point range is one point too wide.** `rules::deck::agenda_point_range` returns
  `min + 2`, where 1.4.6 says "18 or 19". Its test asserts the wrong range.
- **G2 Three more legality gaps:**
  - An identity is accepted inside a deck (1.4.4).
  - The Gateway identities pass the full formats (1.4.1a).
  - The setup gate ignores `deck_limit` (1.4.7).

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
| 1.4 | Deck Construction | deviates | G1, G2. Minimum deck size, influence per copy and out-of-faction agendas match. |
| 1.5 | Extra Cards | unreviewed |  |
| 1.6 | Starting the Game | read in part | Audit: 5 credits, 5 cards, Corp mulligans first, Corp goes first; matches. |
| 1.7 | Ending the Game | deviates | E1, E2, E3. 7 points checked at checkpoints matches. |
| 1.8 | Cards | unreviewed |  |
| 1.9 | Counters and Tokens | unreviewed |  |
| 1.10 | Credits | read in part | 1.10.5 recurring credits: `CardDefinition::recurring_credits`, `payment::place_recurring` / `refill` (Payment Rule). Cited: 1.10.5, 1.10.5a, 1.10.5b. |
| 1.11 | Clicks | read in part | Audit: 3 and 4 clicks match. |
| 1.12 | Objects | read in part | 1.12.2a: an effect acts on the object a card became after it moved (Priority Construction). Cited: 1.12.2a. |
| 1.13 | Host, Hosted, and Hosting | unreviewed |  |
| 1.14 | Ownership and Control | unreviewed |  |
| 1.15 | Targets | unreviewed |  |
| 1.16 | Costs | read in part | Costs are `Cost`, never effects (Rules Audit item 8): 1.16.1a not prevented, 1.16.3 checkpoint after a cost, 1.16.11 nested costs. Cited: 1.16, 1.16.1, 1.16.11, 1.16.1a, 1.16.3. |
| 1.17 | Score, Scoring and Stealing | deviates | F3 (the score shown after a forfeit). Scoring conditions match. |
| 1.18 | Advancing Cards | read in part | 1.18.1–1.18.2: placing an advancement counter is not advancing (`PlaceAdvancementCounters`, `listeners`). Cited: 1.18.1, 1.18.2. |
| 1.19 | Trashing | deviates | F4. |
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
| 3.6 | Upgrades | deviates | B3: a second region is refused (3.6.5d). |
| 3.7 | Events | unreviewed |  |
| 3.8 | Hardware | deviates | B2. |
| 3.9 | Programs | deviates | B1. |
| 3.10 | Resources | unreviewed |  |

### 4. Game Zones

| § | Section | Status | Notes |
|---|---|---|---|
| 4.1 | General | unreviewed |  |
| 4.2 | Deck | read in part | Audit: the view carries only R&D and stack counts; matches. |
| 4.3 | Hand | read in part | Audit: HQ and grip contents go to their owner only; matches. |
| 4.4 | Discard Pile | deviates | A2 (4.4.2). 4.4.6b: HQ discards and unrezzed installs go facedown, rezzed or revealed ones faceup; matches. |
| 4.5 | Score Area | unreviewed |  |
| 4.6 | Play Area | deviates | A3 (4.6.6f). The view and desktop mask a root card by identity only, so a slot shows no type. Remotes exist while occupied, and new ice goes outermost; these match. The other board-layout rules (4.6.5c, 4.6.7b–d, 4.6.8c, 4.6.9a) have not been read against the board yet. |
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
| 5.5 | Discard Phase | deviates | E2 (hand size below 0). HQ discards go facedown; matches. |
| 5.6 | Steps of the Corp's Turn | deviates | C1, C2, C3. |
| 5.7 | Steps of the Runner's Turn | deviates | C1 (no draw-phase window), C2, C3, D3 (5.7.1e). |

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
| 7.1 | Accessing Cards | deviates | D1. Steals mandatory, steal costs declinable (7.1.6a); matches. |
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
| 8.5 | Installing and Uninstalling Cards | deviates | B1, B2, B3. 1[c] per ice, trash then pay, trashed cards keep their status; matches. |
| 8.6 | Playing Events and Operations | read in part | F6 (order only). |
| 8.7 | Searching for Cards | unreviewed |  |
| 8.8 | Swapping Cards | unreviewed |  |

### 9. Abilities

| § | Section | Status | Notes |
|---|---|---|---|
| 9.1 | General | read in part | 9.1: a card's abilities work while it is active (`rules::active`). Cited: 9.1. |
| 9.2 | Timing and Priority | deviates | F1 (9.2.7b). Active player first and own-order simultaneous triggers match. |
| 9.3 | Interpreting Card Text | unreviewed |  |
| 9.4 | Static Abilities | unreviewed |  |
| 9.5 | Paid Abilities | deviates | F1 (9.5.2a). |
| 9.6 | Conditional Abilities | read in part | Audit: a trigger whose source left is dropped, a resolving ability finishes; matches. |
| 9.7 | Play Abilities | unreviewed |  |
| 9.8 | Subroutines | read in part | Audit: unbroken subroutines in printed order, stopping at an ended run; matches. 9.8.2–9.8.3 (new in v26.03): no pool card adds subroutines. |
| 9.9 | Interrupts and Replacement Effects | read in part | Interrupts and prevention: `rules::prevention` (the Prevention Rule, Rules Audit item 4). Expose is deferred. |
| 9.10 | Lingering Effects | read in part | Lingering effects: `rules::lingering` (the Continuous Effect Rule). |
| 9.11 | Identifying Instructions | unreviewed |  |
| 9.12 | Other Rules and Terminology | deviates | F5. |

### 10. Additional Rules

| § | Section | Status | Notes |
|---|---|---|---|
| 10.1 | General | unreviewed |  |
| 10.2 | Information | unreviewed |  |
| 10.3 | Checkpoints | deviates | B2 (10.3.1d, console), E3. The step order (durations, win, unique, triggers) matches. |
| 10.4 | Damage | deviates | F2 (10.4.1). Flatline, core-damage hand size, random trash match. |
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
