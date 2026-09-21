# netrunner-rs beside jinteki.net

A study of [jinteki.net](https://github.com/mtgred/netrunner) — the Clojure
Netrunner server that people have been playing and testing on for over a
decade — held against this engine, to find where ours is thin. Read at
jinteki commit `d94d18cdc` (4 September 2026). **This is a design comparison,
not a port:** no jinteki code is copied into netrunner-rs. jinteki is MIT
licensed and netrunner-rs is GPL-3.0-or-later; what we take from it is ideas,
and any idea taken is written fresh in this codebase's own terms.

**A second pass on 20 September 2026 (§6) audited this file** against both
codebases. The six gaps of §2 hold; §2.1's proposed shape was too narrow,
§2.3 understated the largest gap, §5 item 4 was wrong about where an undo
may exist, and several things were missed outright. Each correction is
marked where the claim stands, and §6 carries the detail.

This file is a finding, not a status page. The gaps it names are tracked as
open items in `ROADMAP.md` (Rules Audit, *jinteki comparison*; Phase 7 §8),
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
| Card tests | 3,536 `deftest`s (first written as 3,553; recounted in the second pass) under `test/clj/game/cards/` | 247 in `crates/netrunner_core/src/cards/tests.rs`, plus the engine's own and the sweeps |

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

**Corrected in the second pass (§6.2): that shape is too narrow.** It has no
way to say *which* cards an effect applies to, and it assumes every value is
a number; five of jinteki's twelve most-used kinds are not.

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

*Done, 21 September 2026 (Rules Audit backlog item 4, three stages):* and
not as proposed. The window needed no kind; the parked thing and the word
that matches it did — `rules::prevention` is one door for damage, a tag and
the trash of an installed card, and what a card prevents is
`dsl::Preventable`, the payload of one `Effect::Prevent`. Their "static
reducers first" is `prevention::run_ending`: Shred's standing prevention is
a lingering effect asked before anything a player chooses. **What this
section missed is that ours had never run:** no card in the pool used either
effect, so neither sweep had opened the window, and it took the one window
slot without giving it back, admitted any action, opened for cards nobody
could use, was bypassed by every trash of a Runner program in the pool, and
could not have fired a "when you would suffer damage" trigger at all. Decoy,
Net Shield and Sacrificial Construct are the first cards to use it, in two
Eternal-legal decks the sweeps rotate. Jack-out, encounter, expose and bad
publicity stay unbuilt until a card prints one; "the Runner cannot jack
out" is a prohibition here (`continuous::cannot`), not a prevention.

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

**Understated — see §6.1.** The ordering is the smaller half. The larger is
that *which cards hear an event* is decided in Rust, event by event, and that
an event an effect produces is never offered to a trigger at all.

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

*Done, 20 September 2026 (Rules Audit backlog item 2, last stage):* the two
flag-setting effects are one `Effect::Prohibit { what, until }`, the flags are
entries on `GameState::lingering`, and `continuous::cannot` is the predicate.
Every prohibition in the pool has a duration, so none is a declared
`ContinuousKind` yet; `ArmRunEndPrevention` is prevention, backlog item 4.

### 2.6 The run has no movement phase

jinteki's run (`core/runs.clj`) is a multimethod over `:initiation`,
`:approach-ice`, `:encounter-ice`, `:movement` and `:success`, with comments
citing the CR step numbers. Our `RunPhase` has no movement phase and no
approach-server phase; `OnApproachServer` is a trigger with no phase of its
own. Every current card resolves correctly without them. Cards that act
"when the Runner passes a piece of ice" or during movement would need the
phase, and it is cheaper to add it while run-phase code is small than after
a set has built on its absence.

**Done (21 September 2026, Rules Audit item 7).** `RunPhase::Movement`
now sits between every pass and the next approach, and before the
approach of a server with no ice: the Runner decides to jack out, then
both players have a window, then the approach. Building it found that
its absence was not only vocabulary. The Runner could jack out after
seeing the next ice rezzed, and an unrezzed upgrade on a server with no
ice could never be rezzed before the approach. The approach-server step
is still the entry to `Success` rather than a phase of its own, and no
card yet hears a pass (no pool card prints "when the Runner passes").

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
   rate as unrated, and it never exists online. **Wrong on both counts —
   see §6.5:** jinteki's undo *is* online and unilateral, and an undo that
   reveals nothing has no reason to be unrated or offline.
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

---

## 6. Second pass (20 September 2026)

The first pass asked what jinteki's *engine shape* shows about ours. This one
asked whether those findings were right and complete, read the modules the
first did not (`access`, `installing`, `pick_counters`, `play_instants`,
`prompts`, `events`, `ice`, `turns`, `commands`, the newer-mechanic files, and
the whole ClojureScript client), and measured rather than estimated. jinteki
is still at `d94d18cdc`.

**What holds.** All six gaps of §2 are real and correctly described as far as
they go. The counts are right (2,056 `defcard`s, 50 cost types) bar the
`deftest` count, corrected above. `RunPhase` is `Initiation`, `ApproachIce`,
`EncounterIce`, `AccessingCard`, `Success`, `Ended` — no movement, no
approach-server. §3 stands.

### 6.1 Who hears an event is decided in Rust — the largest gap, and §2.3 missed it

In jinteki every active card may register a handler for any event
(`:events [{:event :play-operation :req … }]`), every state change goes
through `queue-event`, and `checkpoint` gathers the handlers whose `:req`
passes. *Audience* is never a question: a card listens, and its requirement
filters.

Ours decides the audience per event, by hand. `rules/dispatcher.rs`'s
`dispatch_event` is a `match` that, for each event, names the cards to ask:
`fire_direct` (22 call sites — this card, or this side's identity),
`fire_runner_side` (identity plus rig), `both_sides_candidates` (rezzed
installs plus rig, for the two prevention events). `OperationPlayed` offers
`OnTransactionPlayed` and `OnOperationPlayed` to **the Corp identity only**;
`fire_runner_side`'s own comment records the audience being "widened (M5)"
when a card needed it. And an event produced *by an effect* is returned to
the caller, not dispatched — `ability.rs`'s `dispatch_damage_taken` exists
because "`DamageTaken` is produced deep inside `damage::apply_damage` and
returned, never dispatched", and #85's roadmap entry turns on the same fact.

Two consequences:

- **A new card can need a Rust edit with no new mechanic in it.** An asset
  that reacts to operations being played is card text the DSL can already
  say and the dispatcher will not deliver. That is the "never hardcode card
  rules in Rust" rule bent quietly, one audience at a time.
- **`Trigger` grows by audience, not by event.** `OnSuccessfulRun`,
  `OnSuccessfulRunOnHq`, `OnSuccessfulRunOnRnD` and
  `OnSuccessfulRunOnCentralServer` are one event and a filter. It is the
  `StrengthModifier` pattern of §2.1 in the *when* vocabulary.

**The shape to aim for:** one listener scan — for a dispatched event, every
active card (rezzed installs, the rig and what it hosts, both identities,
scored agendas) whose `TriggeredEffect::trigger` matches and whose
requirement passes, ordered active-player-first as `order_active_first`
already does — with `Trigger` variants parameterised by a filter, events
from effects routed through the same queue, and the state-based checks of
§2.3 run at one point after it. §2.3's audit and this are one piece of work.
The risk is real and is why it goes first rather than never: every trigger
in 178 cards re-fires through a new path, so it is a change to measure
byte-for-byte on both seatings before and after.

**The audience half is built** (20 September 2026, `rules::listeners`; ROADMAP Rules Audit, item 1): the listener scan, `TriggeredEffect::subject`, and the measurement — which was a shadow run rather than a byte-identical one, because a side ordering its whole plan is itself a change. The queue and the checkpoint are the stages after it.

**All of it is built** (20 September 2026): the queue as a debug-build audit that every event a card can hear was dispatched, the checkpoint as `rules::checkpoint`, and the second consequence above as `TriggeredEffect::when` — the four success variants are `OnSuccessfulRun` and a server filter, `OnTransactionPlayed` and `OnVirusInstalled` are `OnOperationPlayed` and `OnCardInstalled` with a card filter, and the requirement that filtered by the triggering card is gone into the same field.

### 6.2 The continuous-effect layer needs a target and a payload

§2.1 proposed `{ kind, value: Amount, while: EffectRequirement }`. Counting
jinteki's `:type`s in use (86 distinct) shows what that leaves out:

| kind | uses | value is |
|---|---|---|
| `:ice-strength` | 31 | a number |
| `:additional-subroutines` | 23 | **a subroutine** |
| `:gain-subtype` | 19 | **a subtype** |
| `:steal-additional-cost` | 12 | **a cost** |
| `:rez-cost`, `:install-cost`, `:trash-cost`, `:hand-size` | 12, 9, 7, 7 | a number |
| `:can-host` | 12 | **a card filter** |
| `:disable-card` | 10 | **nothing — a fact about a card** |
| `:cannot-jack-out`, `:cannot-steal`, `:cannot-run-on-server`, … | 5, 4, 4 | a boolean |

and every one of them carries a `:req` that answers *which card is this
about* — this ICE, ICE protecting this server, the card being accessed — as
well as *is it on*. So the layer is a closed enum with a payload per kind
(`IceStrength(Amount)`, `GainSubtype(CardSubtype)`,
`AdditionalSubroutine(SubroutineDef)`, `AdditionalCost { of, cost: Cost }`,
`Cannot(Prohibition)`, …), plus `applies_to: CardFilter` and `while:
EffectRequirement`. That is still data, still `deny_unknown_fields`, and it
absorbs `host_ice_gains_subtypes` along with the numeric fields.

It also reaches two mechanics we do not model at all and the first pass did
not list: **ICE gaining or losing subroutines** (jinteki's
`get-expected-subroutines` recomputes the list and `reconcile-subroutines`
keeps each sub's broken and fired marks across the recomputation — ours
builds `RunIce::subroutines` from the definition once) and **"cannot be
broken"**. Durations in use: 18, of which `:end-of-run` is 125 of about 190
registrations, then `:end-of-turn` 25 and `:end-of-encounter` 14 — our three
`BoostDuration`s are the right three to start from.

### 6.3 "The first time each turn" is a query there and a field here

jinteki keeps the turn's and the run's events and asks them questions:
`first-event?` is called 136 times in card code, `no-event?` 38, `last-turn?`
30, `first-run-event?` 12, `event-count` 8 (`core/events.clj`). A card says
*the first time you install a program each turn* by filtering a log.

Ours answers each such question with its own field on `GameState` and its
own `EffectRequirement`: `first_install_used_this_turn`,
`first_install_discount_used_this_turn`, `first_hq_run_used_this_turn`,
`played_operation_this_turn`, `made_successful_run_this_turn` and
`…_last_turn`, `agenda_points_scored_this_turn`, `servers_run_this_turn`,
`actions_taken_this_turn`, against `FirstInstallThisTurn`,
`FirstSuccessfulHqRunThisTurn`, `MadeSuccessfulRunThisTurn`,
`PlayedOperationThisTurn`, `NoActionTakenThisTurn`, ….
`once_per_turn_used` was the right generalisation for *once per turn*;
nothing generalises *first time* or *how many times*. These fields rightly
live on `GameState` (they survive a parked decision), so the State Hygiene
Rule is not broken — the DSL Growth Rule is what they press on.

A full event `Vec` per turn is the obvious port and probably the wrong one:
`GameState` is cloned on every action and thousands of times per search. The
candidate is **per-turn and per-run counters keyed by a small event-kind
enum with a filter** (`Count { of: TurnFact, at_least / exactly }`), which is
constant-size, replaces the fields above, and makes `last turn` a second
copy of the same map.

**Done, 20 September 2026** (`docs/roadmap/rules-audit.md`, backlog item 3,
four PRs). The shape is the candidate's, with the event-kind enum and the
filter found rather than built: the enum is `Trigger` and the classifier is
`listeners::moments`, so `rules::turn_log` is one `record` at the top of
`dispatch_event` into a fixed-size table keyed by trigger, whose moment it
was, and a class of what it was about that holds only what both players saw.
"If you … this turn" is `Amount::TimesThisTurn(trigger)`; "the first time each
turn" is not a query a card writes at all but one word beside its trigger
and its `when` (`first_each_turn`), judged in the listener scan against the
count taken as the event was recorded — jinteki's `first-event?` is asked at
resolution, and ours cannot be. `last turn` is the second copy, as row totals.
The fields above are gone but `servers_run_this_turn`, which is a list of
servers rather than a count.

### 6.4 Smaller things the first pass did not see

- **Where a payment comes from.** §2.4 covered *what* is paid. jinteki's
  `core/pick_counters.clj` covers *from which pool, chosen by whom*: stealth
  credits, recurring credits restricted to a purpose, cost reducers
  (`pick-credit-providing-cards`, `pick-credit-reducers`,
  `pick-virus-counters-to-spend`). Ours spends automatically —
  `hosted_credits_usable_for`, recurring before the wallet, bad-publicity and
  bonus run credits on `RunState` — which is right while no two pools
  compete. Stealth is the card family that makes them compete.

  **Done, 21 September 2026** (`docs/roadmap/rules-audit.md`, backlog item 5,
  four PRs). Not the port this bullet sketched. There is no stealth card in
  the catalog, and "spent automatically in a fixed order" turned out to be
  the smaller problem: the pools were spent by different pieces of code,
  each with its own affordability sum, and every sum forgot a pool the
  payment then took. So the first thing built was one scan that both
  questions read (`rules::payment`), with what a pool pays for as a word the
  card prints (`dsl::PaysFor`). Where jinteki's pickers (above) put the
  choice of provider to the player, ours asks **only when the answer changes
  what the payer is left with** — a pool never worth more than another is
  spent first unasked — and answers with the existing choice action rather
  than a credit-by-credit prompt, which waited on a numeric decision (item 6, since built: the question is now how many credits come from which pool). (How
  often jinteki's prompt appears was not measured or read for this item.) It parks by *replaying* the action, which jinteki's continuation
  style has no need of and a pure `apply_action` makes nearly free.
  Recurring credits became a declaration refilled as a step of the turn
  (Comprehensive Rules 1.10.5) where they had been a trigger their owner
  was asked to order.
- **No numeric decision.** `PendingDecision` is `ChooseEffect`,
  `ChooseCards`, `ChooseServer`, `ChooseTriggerOrder`; the only number a
  player ever types is a trace bid. X costs and "pay up to N" have nowhere
  to park. jinteki has number, credit and counter prompts as kinds.

  **Stage 1 done, 21 September 2026** (`docs/roadmap/rules-audit.md`,
  backlog item 6). One kind, not three: a number from a range, offered as
  one legal action per number the way a trace bid is, so nothing about a
  client or a bot had to learn a new shape of prompt. What it found on the
  way in is that the pool already printed chosen numbers and each card had
  taken the most its text allows — Bigger Picture removing every tag is
  the one that mattered. The chosen number is written into the effect
  that resolves with it, which a continuation-passing engine like
  jinteki's gets for free from a closure and ours has to say. X costs are
  not built: no card in the pool prints one. **Stage 2, the same day:** a
  payment two pools could make is split to the credit by the same number
  ("how many from Azimat?"), which is jinteki's credit prompt reached from
  the other side — asked only when the split changes what the payer is
  left with.
- **One replacement per run.** `RunState::access_replacement` is a single
  `Option`; jinteki collects every `:successful-run` replacement and
  `choose-replacement-ability` asks the Runner which, adding "Breach" unless
  one is mandatory.
- **Concepts with no home yet**, each small, none needed by a shipped set:
  reveal as an event other cards can hear (`core/revealing.clj`; ours is a
  `reveal: bool` on a selection), a set-aside zone with per-viewer
  visibility (`set_aside.clj` — this one touches masking, so it wants an
  explicit who-may-see rule), expose, facedown Runner installs, the mark,
  charge, per-host limits (`:max-cards`, `:max-mu`), and agenda points or
  advancement requirements that change while installed (`agendas.clj`
  recomputes both every checkpoint — two more kinds for §6.2's enum).
- **One thing jinteki does that we should check we do:** when Archives is
  breached it shuffles the unseen cards before turning them up
  (`turn-archives-faceup`), so the order they were trashed in leaks nothing.
  Ours flips them in place (`run/access.rs`), and `PublicArchivedCard`
  already shows the Runner each facedown card's position, so a breach ties
  every card to the moment it arrived. Whether the rules entitle the Runner
  to that is a question for the Comprehensive Rules, not for jinteki; it is
  listed so that it gets asked.

### 6.5 Taking a move back — §5 item 4 corrected

**The report from play:** Red Team — "[click]: Run a central server you have
not run this turn" — spends its click and only then shows which servers are
left. A person who does not remember which they ran cannot look first and
cannot put it down. The events that park the same `ChooseServer` (Jailbreak,
Overclock, Tread Lightly, …) have already paid and gone to the heap by the
time the prompt is up (`engine::play_event`).

**jinteki does no better at the card, and worse at the undo.** Costs are
paid before the prompt (`play_instants.clj`, `continue-play-instant`), a
prompt has a Cancel only where the card's author wrapped its choices in
`cancellable`, and the run-event helpers do not. What it has instead is
`/undo-click`: the last four whole-state snapshots, taken before each click,
restored **unilaterally, online, with no guard on hidden information** —
undoing a draw or an access rewinds it, and the only cost is a log line.
`/undo-turn` needs both players. So "it never exists online" was simply
false, and "unrated" was half right.

**The line that matters is what the undo teaches.** Three things, in the
order a person meets them:

1. **Look first.** Before the card is played, its entry says what it will
   ask — which servers, which options. A determinized sample and the real
   `apply_action`, exactly as `board::breaks` prices a route; client only.
2. **A free take-back.** While the person is still on the prompt their own
   action opened, and nothing hidden was revealed, `rng_step` has not moved
   and the other seat has not acted, going back gives them nothing they did
   not have. It is fair in a rated game, and would be online — the opponent
   has seen which card was played, which costs only the person taking it
   back. This is also the install back-out Phase 7 §4d declined: its blocker
   was a new `PlayerAction` and `ActionSpace` 1646 → 1647, and a restore in
   the *session* needs neither.
3. **Undo a click, against a bot.** Past that line the undo has taught
   something, so the game stops being rated the first time it is used.

Tracked as Phase 7 §8 item 4.

### 6.6 More from the client, after §5's eleven

Read from `src/cljs/nr/gameboard/` and `nr/help.cljs`, whose FAQ is a decade
of support questions. In the order they would matter here:

12. **One Continue button that names what is next** — "Continue to Approach
    ice", "Breach server", "No further actions". Run timing made legible by
    the label, not by the player knowing the CR.
13. **The encounter panel always on during an encounter**: the ICE's name,
    subtypes, live strength and every subroutine. §4ac marks the subs; this
    is the rest.
14. **Per-card "always / never / ask" for an optional trigger**
    (`core/optional.clj`'s autoresolve) — the same prompt answered the same
    way forty times a game is the FAQ's quietest complaint.
15. **A report-a-bug bundle.** jinteki's `/bug` opens an issue with a log.
    Ours can attach the seed and the action record and the game *replays
    exactly* — the strongest form of this feature anyone could have, and the
    honest answer to the need behind jinteki's state-editing commands: a
    wedged game is a bug to reproduce, not a table to fix by hand.
16. **An end-of-game table** (clicks spent, credits, cards drawn, runs, by
    side) and **a start-of-game box** (both identities, the opening hand
    turned up, keep or mulligan).
17. **Card names in the log open the card** on hover or secondary click.
18. **A colour-blind-safe palette** for the card-state colours (jinteki ships
    Okabe–Ito). Ours are purple and yellow affordances and the transition
    outline; they should be checked against it.
19. **Open decklists** as a game option, and **hand sort** by name or type
    (ours has the person's own order; this is a second, offered one).

Looked at and **not** borrowed: the ± counters on every stat and drag between
zones (the same objection as the slash commands, §5), "indicate action" and
canned chat (human-to-human etiquette with nothing to say to a bot; revisit
with Phase 4), and the tournament and lobby machinery.

