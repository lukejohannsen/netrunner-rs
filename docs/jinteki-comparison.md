# netrunner-rs beside jinteki.net

A study of [jinteki.net](https://github.com/mtgred/netrunner) — the Clojure
Netrunner server that people have been playing and testing on for over a
decade — held against this engine, to find where ours is thin. Read at
jinteki commit `d94d18cdc` (4 September 2026). **This is a design comparison,
not a port:** no jinteki code is copied into netrunner-rs. jinteki is MIT
licensed and netrunner-rs is GPL-3.0-or-later; what we take from it is ideas,
and any idea taken is written fresh in this codebase's own terms.

This file is a finding, not a status page. The gaps it names are tracked as
open items in `ROADMAP.md` (Rules Audit, *jinteki comparison*; Phase 7 §5),
which point back here.

jinteki paths are relative to its repository root; ours to this one.

---

## 1. Two shapes of engine

| | jinteki | netrunner-rs |
|---|---|---|
| State | One mutable `atom` of Clojure records (`src/clj/game/core/state.clj`), changed in place with `swap!` | `GameState`, a value; `apply_action(&GameState, …) -> Result<(GameState, Vec<GameEvent>), RulesError>` |
| Cards | `defcard` (`game/core/def_helpers.clj`) registers a map whose values are **closures** — `:effect (effect …)`, `:req (req …)` — so a card is arbitrary code | Closed serde enums in `netrunner_core::dsl`, parsed from JSON with `deny_unknown_fields`; a card is data |
| Waiting on a player | Continuation-passing over an eid registry: `wait-for`, `continue-ability`, `effect-completed` (`game/macros.clj`, `core/eid.clj`) | A parked `PendingDecision` / `DeferredTrigger` on `GameState`; the next `PlayerAction` resumes it |
| Randomness | Host RNG | `seed` + `rng_step` inside the state |
| Wire format | Per-seat privatized **diffs** (`core/diffs.clj`, the `differ` library) with a sequence number and resync | Per-seat masked **snapshots** (`view::build_client_view`) |
| Card count | 2,056 `defcard`s — every card Null Signal Games and FFG printed | 178 card files: *System Gateway* 77/77, *Elevation* 82/82, a 19-card Core subset |
| Card tests | 3,553 `deftest`s under `test/clj/game/cards/` | 247 in `crates/netrunner_core/src/cards/tests.rs`, plus the engine's own and the sweeps |

**What each shape buys.** jinteki's closures can express any card the day it
is printed, which is how it covers the whole card pool; the cost is that a
card is only as checkable as the code in it, and the state cannot be replayed,
searched, or handed to a bot as a value. Ours can do all three — replay,
MCTS/PUCT, the RL gym and the deadlock sweeps are all consequences of
`apply_action` being a pure function over serializable data — and the price
is that every new *kind* of card text needs vocabulary in the DSL. The DSL
Growth Rule exists to keep that price honest; §2 is where jinteki shows what
vocabulary we have not yet grown.

---

## 2. Where jinteki shows gaps in our DSL

Ordered by how much of the next card set each would unlock.

### 2.1 No general layer for continuous effects

jinteki has one mechanism for "while X, Y": a card's `:static-abilities` are
`{:type :value :req}` maps registered for as long as the card is active, and
`register-lingering-effect` adds the same shape with a duration
(`:end-of-run`, `:end-of-turn`, `:end-of-encounter`, `:until-runner-turn-begins`, …). Every rules question
that could be modified asks the same registry — `get-effects`, `sum-effects`,
`any-effects` (`core/effects.clj`) — so `ice-strength`, `breaker-strength`,
`rez-cost`, `install-cost`, `hand-size`, `access-bonus`, `cannot-run-on-server`
and about seventy more types are all one mechanism.

We have no such layer. Each continuous effect is its own field, added for the
card that needed it:

- `StrengthModifier` — six variants, each a card's rule (`PerInstalledIcebreaker`, `WhileProtectingRemote`, `PerFracterInHeap`, …);
- on `CardDefinition` (`dsl/card.rs`): `memory_bonus`, `max_hand_size_bonus`, `install_cost_discount_if`, `install_cost_discount_amount`, `first_install_discount`, `ice_rez_cost_modifier`, `root_asset_trash_cost_bonus`, `hosted_breaker_bonus`, `host_ice_gains_subtypes`;
- on `InstalledRunnerCard`: `encounter_strength_buff`, `run_strength_buff`, `turn_strength_buff`, one per `BoostDuration`.

That is the single largest source of pressure on the DSL Growth Rule's ratio,
because every new "while" or "costs less" card adds a field or a variant
rather than a row. **A continuous-effect layer** — a card-declared list of
`{ kind, value: Amount, while: EffectRequirement }`, plus a `GameState` list
of lingering ones with a duration, consulted by the handful of functions that
compute strength, cost, MU, hand size and access count — would absorb every
field above. It is state that survives a parked decision, so lingering
entries belong on `GameState` by the State Hygiene Rule's own test, not on
`ResolutionContext`.

### 2.2 Prevention is two special cases, not a mechanism

jinteki's `core/prevention.clj` is one mechanism for everything that can be
prevented: a card declares `:prevention [{:prevents <kind> :type :ability|:event …}]`
and the kinds are damage, pre-damage, tags, trash, jack-out, end-the-run,
encounter, expose and bad publicity. Each resolver offers both sides a menu,
static reducers first.

Ours covers exactly two: `Trigger::OnDamageAboutToResolve` and
`OnTrashAboutToResolve`, feeding a `WindowCheckpoint::Prevention` paid-ability
window and `Effect::PreventDamage` / `PreventTrash`. Tag avoidance, "the run
cannot end", "the Runner cannot jack out" and expose prevention each have no
home; `ArmRunEndPrevention` is a one-card answer to the end-the-run case. The
next set with a tag-avoidance card (there are several in the pool) will need
this generalised rather than a third special case: a `PreventableEvent` kind
parameter on the existing window.

### 2.3 Checkpoints and when triggers fire

jinteki moved to the Comprehensive Rules' timing model: events are queued
(`queue-event`) and a `checkpoint` (`core/engine.clj`, CR 10.3) marks pending
abilities — one instance per context unless marked otherwise — removes
expired durations, checks agenda wins, the unique and console rules, MU,
clears empty remotes, and only then opens the reaction window, active player
first.

We fire triggers when the event is dispatched (`rules/dispatcher.rs`) and
defer the rest when one parks. Our ordering rules are right as far as they go
(active player first; the controller orders a side's own simultaneous
triggers through `ChooseTriggerToResolve`), but there is no single point at
which the state-based checks run in a defined order. `memory.rs` runs the MU
check on its own, and win checks happen in `win.rs`. **Worth an audit against
CR 10.3** before a set with more "when … is trashed" and "after you install"
interactions than *Elevation* has.

### 2.4 Costs

jinteki has 50 cost types behind four multimethods — `value`, `label`,
`payable?`, `handler` (`core/costs.clj`, `core/payment.clj`) — so a card
names `(->c :forfeit)` or `(->c :net 1)` and the payment, the label and the
"can I pay this?" check come with it. Ours are the ten variants of
`dsl::Cost` (`Credits`, `Clicks`, `TrashSelf`, `ClearTags`, `TakeTags`,
`AnyOf`, `AllOf`, `RemoveCounters`, `RemoveSelfFromGame`, `TrashRandomFromHq`).
The shape is already right: `Cost` is data, and `AnyOf`/`AllOf` compose. The
gap is only coverage, and **jinteki's list is the backlog**: forfeit an
agenda, take damage as a cost, trash an installed card of a type, spend a
power or virus counter from another card, X credits, add a card to the
bottom of the deck.

### 2.5 Prohibitions

jinteki asks one predicate before every restricted action — `can-run?`,
`can-rez?`, `can-steal?`, `can-trash?`, `can-advance?`, `can-score?`,
`can-host?` (`core/flags.clj`) — backed by turn, run and persistent flags and,
in newer cards, by the effects registry of §2.1 (`:cannot-jack-out` and
others). Ours are effects that each set their own flag:
`PreventScoringForRemainderOfTurn`, `PreventStealAndTrashForRemainderOfRun`,
`ArmRunEndPrevention`. They fall out of §2.1 for free: a prohibition is a
continuous effect whose value is a boolean, and the guard in `apply_action`
asks the layer.

### 2.6 The run has no movement phase

jinteki's run (`core/runs.clj`) is a multimethod over `:initiation`,
`:approach-ice`, `:encounter-ice`, `:movement` and `:success`, with comments
citing the CR step numbers. Our `RunPhase` has no movement phase and no
approach-server phase; `OnApproachServer` is a trigger with no phase of its
own. Every current card resolves correctly without them. Cards that act
"when the Runner passes a piece of ice" or during movement would need the
phase, and it is cheaper to add it while run-phase code is small than after
a set has built on its absence.

---

## 3. Where netrunner-rs is stronger — keep these

- **Determinism and replay.** Bit-identical replay of any action history,
  and a state bots can copy and search. jinteki's undo restores whole
  snapshots and its replay replays diffs; neither can search.
- **Masking at the engine boundary.** jinteki strips hidden state in
  `core/diffs.clj`, after the fact, by a whitelist of keys and
  `card/is-public?`. Ours builds each seat's `ClientView` from what that seat
  may see, and the sweep checks that no action names a card its view hides.
- **Card data that cannot misspell.** `deny_unknown_fields` makes a typo a
  parse error. A jinteki card map with a misspelled key is silently ignored.
- **The printed card is the authority** (the Linked Clause Rule), and the
  clause gate doubles as an erratum detector when NetrunnerDB's wording
  changes. jinteki's prompts are strings written in the card code.
- **The agent-driven sweeps and the coverage gate.** jinteki has thousands
  of scripted tests and no equivalent of "play thousands of games between
  real agents and prove nobody is ever left without a legal action". Five
  deadlocks and a crash were found that way here.
- **Bots, ratings and a difficulty ladder.** jinteki is human-versus-human
  only.

---

## 4. Testing: jinteki's card tests are shorter

jinteki's framework (`test/clj/game/test_framework.clj`) builds a game from a
sentence — `(new-game {:corp {:hand ["Ice Wall"]} :runner {:hand ["Corroder"]
:credits 10}})` — and plays it through the real action path with helpers that
name cards rather than indexes: `play-from-hand`, `run-on`, `run-continue`,
`card-ability`, `click-prompt`, `click-card`, `fire-subs`, `get-program`,
`auto-pump-and-break`. A card test is a dozen lines.

Ours (`crates/netrunner_core/src/cards/tests.rs`) start from `base_state()`
and assign fields by hand — `state.corp.installed = vec![InstalledCard { … }]`
— before calling `apply_action`. That is precise, and it lets a test reach a
state no play sequence could, but it is two to three times longer and
couples every test to `GameState`'s field layout.
`rules/test_support.rs` has four helpers. **A small scenario builder** — a
deck-and-hand spec in, a `GameState` reached through `setup` and real
actions out, plus name-addressed helpers for "install this", "run there",
"choose that" — would make the next set's per-card tests cheaper to write
and harder to get subtly wrong. It is test code, so it costs the engine
nothing.

**jinteki's tests are also an oracle.** For any card we implement next, its
`deftest`s record years of edge cases found by people playing: read them
before writing ours. The tests are only read, never copied.

---

## 5. Features and UI worth borrowing

In the order they would matter to a person playing:

1. **The rig in three rows** — programs, hardware, resources, programs
   nearest the ICE (`board-view-runner`, `src/cljs/nr/gameboard/board.cljs`).
   Taken up in Phase 7 §4aa. jinteki also shows the top card of the heap and
   of Archives face up on the board; we chose to keep counts, with the whole
   pile a click away.
2. **Auto-pump and auto-pump-and-break** (`breaker-auto-pump`,
   `core/ice.clj`): one button that raises a breaker to strength and breaks
   every subroutine it can, priced before it is pressed. Encounters are the
   most click-heavy part of our client. For us this is a client-side
   *sequence of legal actions*, not a new `PlayerAction`: the button submits
   pumps then breaks, each chosen from `legal_actions`.
3. **Broken and fired subroutines marked on the ICE**, struck through or
   ticked, so the state of an encounter is visible without the log.
4. **Undo a click, undo a turn** (`/undo-click`, `/undo-turn`). jinteki keeps
   the last few snapshots. For us state is a value and the history is
   already kept, so undo is cheap *in a local game against a bot*. It would
   rate as unrated, and it never exists online.
5. **Replays with notes and bookmarks** (`nr/gameboard/replay.cljs`). We have
   `MatchHistory` and a masked per-seat log; a replay viewer is a client
   over data we already store.
6. **The Corp's auto-pass during a run** (`toggle-auto-no-action`,
   `core/runs.clj`): a toggle that has the Corp pass every run window until
   they turn it off, plus a setting to pass priority as soon as they rez a
   piece of ICE (`:pass-on-rez`). Our auto-pass (Phase 7 §4e) only takes a
   pass when it is the only action offered.
7. **Space as the one "continue" key**, meaning whatever is next — continue
   the run, take a credit, end the turn — as a third door to a button that
   already exists (the §5 key rule).
8. **Ghost Trojans**: a program hosted on ICE shown faintly in the program
   row as well as on its host, so the rig still reads as the rig.
9. **Identical rig cards stacked** into one card with a count.
10. **Run and turn timing diagrams** (`nr/gameboard/diagrams.cljs`): the CR's
    steps drawn, with the current one lit. Our phase panel is a start.
11. **Spectators** with hidden-hands, Corp-only and Runner-only views. Phase 4
    has spectators on the server; the desktop client has no spectator seat.

**Not to borrow, and why:**

- **Slash commands that edit state** (`/credit`, `/draw`, `/move-hand`,
  `/rez-all`, `/summon` — `core/commands.clj`). They exist because jinteki's
  cards are sometimes wrong and players need to fix the table by hand. Here
  they would break the Session Rule and the Client Contract: a client never
  mutates rules state. A wrong card here is a bug to fix in the card's JSON.
- **Diffs on the wire.** Already recorded as a later optimization (Phase 4
  §4), to be profiled first. jinteki needs sequence numbers and a resync path
  because diffs can be lost; snapshots cannot.
