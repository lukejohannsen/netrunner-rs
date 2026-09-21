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

1. **Archives is kept, and shown, in arrival order (4.4.2 — deviates).** The
   rules say "Discard piles are not ordered. A player may freely arrange the
   cards in their discard pile in any order at any time." The engine keeps
   `CorpState::archives` as a `Vec` in arrival order, and the Runner's view
   (`masking::PublicArchivedCard`) keeps each card's place with its facedown
   flag. So after a breach turns them faceup in place, the Runner can tell
   which facedown card arrived when. This answers the question Rules Audit
   backlog item 10 put to the rules ("whether the Runner is entitled to the
   arrival order of facedown cards in Archives"): they are not. Owed as its
   own change. It touches masking and the clients' Archives sheet, so the
   fog gate and both sweeps apply.
2. **The board-layout rules are in 4.6 and 4.4.6b**, and nothing has read
   the desktop board against them yet (see the 4.6 row). The one most likely
   to matter is 4.6.6f: the *type* of a card in a remote's root must not be
   derivable from where or how it lies.

## Sections

### 1. Game Concepts

| § | Section | Status | Notes |
|---|---|---|---|
| 1.1 | General | unreviewed |  |
| 1.2 | Golden Rules | unreviewed |  |
| 1.3 | Symbols | n/a | Symbols. The house spellings (`[click]`, `[credit]`…) are how `rules_sync.py` renders them. |
| 1.4 | Deck Construction | read in part | Deck construction is enforced by `netrunner_core::format` and `every_sample_deck_is_legal`; not yet read against 1.4 rule by rule. |
| 1.5 | Extra Cards | unreviewed |  |
| 1.6 | Starting the Game | unreviewed |  |
| 1.7 | Ending the Game | unreviewed |  |
| 1.8 | Cards | unreviewed |  |
| 1.9 | Counters and Tokens | unreviewed |  |
| 1.10 | Credits | read in part | 1.10.5 recurring credits: `CardDefinition::recurring_credits`, `payment::place_recurring` / `refill` (Payment Rule). Cited: 1.10.5, 1.10.5a, 1.10.5b. |
| 1.11 | Clicks | unreviewed |  |
| 1.12 | Objects | read in part | 1.12.2a: an effect acts on the object a card became after it moved (Priority Construction). Cited: 1.12.2a. |
| 1.13 | Host, Hosted, and Hosting | unreviewed |  |
| 1.14 | Ownership and Control | unreviewed |  |
| 1.15 | Targets | unreviewed |  |
| 1.16 | Costs | read in part | Costs are `Cost`, never effects (Rules Audit item 8): 1.16.1a not prevented, 1.16.3 checkpoint after a cost, 1.16.11 nested costs. Cited: 1.16, 1.16.1, 1.16.11, 1.16.1a, 1.16.3. |
| 1.17 | Score, Scoring and Stealing | unreviewed |  |
| 1.18 | Advancing Cards | read in part | 1.18.1–1.18.2: placing an advancement counter is not advancing (`PlaceAdvancementCounters`, `listeners`). Cited: 1.18.1, 1.18.2. |
| 1.19 | Trashing | unreviewed |  |
| 1.20 | Memory | unreviewed |  |
| 1.21 | Card Visibility | unreviewed |  |

### 2. Parts of a Card

| § | Section | Status | Notes |
|---|---|---|---|
| 2.1 | Name | unreviewed |  |
| 2.2 | Unique Symbol | unreviewed |  |
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
| 3.6 | Upgrades | unreviewed |  |
| 3.7 | Events | unreviewed |  |
| 3.8 | Hardware | unreviewed |  |
| 3.9 | Programs | unreviewed |  |
| 3.10 | Resources | unreviewed |  |

### 4. Game Zones

| § | Section | Status | Notes |
|---|---|---|---|
| 4.1 | General | unreviewed |  |
| 4.2 | Deck | unreviewed |  |
| 4.3 | Hand | unreviewed |  |
| 4.4 | Discard Pile | deviates | **4.4.2: "Discard piles are not ordered."** The engine keeps Archives in arrival order and shows the Runner facedown cards by position (`PublicArchivedCard`), so the Runner can tell which facedown card arrived when. This answers the question Rules Audit backlog item 10 put to the rules: the Runner is not entitled to that order. The fix is owed as its own change. 4.4.6b (facedown Archives cards shown sideways so the Runner can see they are there) is a board-layout rule. |
| 4.5 | Score Area | unreviewed |  |
| 4.6 | Play Area | unreviewed | **The board-layout rules.** 4.6.5c: the Runner's cards have no set place, so the rig's three rows are the client's choice. 4.6.6f: the order of a server root is open information, but the *type* of a remote root card must not be derivable from where or how it lies. 4.6.7b–d and 4.6.8c: the centrals, then remotes in a row going away from them (`board::table_servers`). 4.6.9a: ice lies horizontally in front of its server, ordered outward. Read these against `netrunner_client::board` and `netrunner_desktop`'s `layout` before the next board PR. |
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
| 5.5 | Discard Phase | unreviewed |  |
| 5.6 | Steps of the Corp's Turn | unreviewed |  |
| 5.7 | Steps of the Runner's Turn | unreviewed |  |

### 6. Runs

| § | Section | Status | Notes |
|---|---|---|---|
| 6.1 | General | unreviewed |  |
| 6.2 | Position | unreviewed |  |
| 6.3 | Initiation | unreviewed |  |
| 6.4 | Approach Ice | unreviewed |  |
| 6.5 | Encounter Ice | unreviewed |  |
| 6.6 | Movement | unreviewed |  |
| 6.7 | Success | unreviewed |  |
| 6.8 | Run Ends Phase | unreviewed |  |
| 6.9 | Steps of a Run | read in part | 6.9.1 and 6.9.4 (the movement phase, Rules Audit item 7): jack-out only in movement, a window before every approach. Cited: 6.9.1, 6.9.4, 6.9.4b, 6.9.4d, 6.9.4e. |

### 7. Accessing Cards and Breaching Servers

| § | Section | Status | Notes |
|---|---|---|---|
| 7.1 | Accessing Cards | unreviewed |  |
| 7.2 | Steps of Accessing a Card | unreviewed |  |
| 7.3 | Breaching Servers | unreviewed |  |
| 7.4 | Determining Candidates | unreviewed |  |
| 7.5 | Steps of Breaching a Server | unreviewed |  |

### 8. Card Manipulation

| § | Section | Status | Notes |
|---|---|---|---|
| 8.1 | Faceup and Facedown Status | not modelled | 8.1.4: facedown Runner installs (Rules Audit backlog item 10). |
| 8.2 | Card Movements | unreviewed |  |
| 8.3 | Arranging and Rearranging Cards | unreviewed |  |
| 8.4 | Drawing Cards | unreviewed |  |
| 8.5 | Installing and Uninstalling Cards | unreviewed |  |
| 8.6 | Playing Events and Operations | unreviewed |  |
| 8.7 | Searching for Cards | unreviewed |  |
| 8.8 | Swapping Cards | unreviewed |  |

### 9. Abilities

| § | Section | Status | Notes |
|---|---|---|---|
| 9.1 | General | read in part | 9.1: a card's abilities work while it is active (`rules::active`). Cited: 9.1. |
| 9.2 | Timing and Priority | unreviewed |  |
| 9.3 | Interpreting Card Text | unreviewed |  |
| 9.4 | Static Abilities | unreviewed |  |
| 9.5 | Paid Abilities | unreviewed |  |
| 9.6 | Conditional Abilities | unreviewed |  |
| 9.7 | Play Abilities | unreviewed |  |
| 9.8 | Subroutines | unreviewed |  |
| 9.9 | Interrupts and Replacement Effects | read in part | Interrupts and prevention: `rules::prevention` (the Prevention Rule, Rules Audit item 4). Expose is deferred. |
| 9.10 | Lingering Effects | read in part | Lingering effects: `rules::lingering` (the Continuous Effect Rule). |
| 9.11 | Identifying Instructions | unreviewed |  |
| 9.12 | Other Rules and Terminology | unreviewed |  |

### 10. Additional Rules

| § | Section | Status | Notes |
|---|---|---|---|
| 10.1 | General | unreviewed |  |
| 10.2 | Information | unreviewed |  |
| 10.3 | Checkpoints | read in part | Checkpoints: `rules::checkpoint` (Rules Audit item 1). The listed steps have not been checked one by one. Cited: 10.3. |
| 10.4 | Damage | unreviewed |  |
| 10.5 | Tags | unreviewed |  |
| 10.6 | Bad Publicity | read in part | Bad publicity credits are a pool of their own for the run (Payment Rule). That matches v26.03's change keeping them out of the credit pool; the bad publicity fund's other rules are not yet read. |
| 10.7 | Link | unreviewed |  |
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
