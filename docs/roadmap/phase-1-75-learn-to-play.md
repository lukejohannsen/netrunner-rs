# Phase 1.75 — Learn to Play — DONE

Area roadmap; the index is `ROADMAP.md`. Addresses: "Phase 1.75 §1"–"§9".

Two stages: **scripted lessons** — our addition; Null Signal's [Learn to Play](https://nullsignal.games/players/learn-to-play/) is a rulebook with training wheels and has no scripted turns, so do not "correct" stage 1 toward the source — and **a faithful NSG starter game** (preset decks, 6 points to win, booster staging). All 32 starter and 11 booster cards were already implemented System Gateway cards.

## 1. Configurable match rules — DONE (`feat/phase-175-foundations`)
`MatchRules { winning_agenda_points }` on `GameState` (`#[serde(default)]`), on `ClientView`, carried by `determinize`; `GameState::setup_with(…, MatchRules, DeckOrder)`. A struct, not a bare field — the State Hygiene Rule applied before the debt. `agenda_point_range` needed no threshold: the unflattened `2 + 2·⌊size/5⌋` gives 34 cards → 14–16. Random-vs-random byte-identical; the heuristic shift seen then is unproven (Phase 2 §5's seed-spread band).

## 2. Starter identities playable — DONE
`the_syndicate.json` / `the_catalyst.json`: blank, `min_deck_size` 30 (print), `unlimited_influence` (the catalog's `influence_limit: null`, which nothing had modelled). Reverses Phase 1 §7's exclusion deliberately; System Gateway reads 77 of 77.

## 3. Tutorial decks without polluting self-play — DONE
Four decklists (`the_syndicate_starter` 34 cards / 14 points, `the_catalyst_starter` 30, the `_boosted` pair 44 / 18 and 40) under `DeckCategory::Starter | Boosted`; `matchups()` filters to `Sample`, so tutorial decks never train the network. `DeckCategory::match_rules()` gives `Starter` 6 points.

## 4. Deterministic stacked openings — DONE
`DeckOrder::{Shuffled, Fixed { corp, runner }}`, authored **top-first**, validated as a permutation of the deck (`FixedOrderNotAPermutation`). Draws `pop()` from the end.

## 5. Lesson content as data — DONE (`feat/phase-175-lessons`)
`data/lessons/{corp,runner}/*.json`, embedded by `build.rs`, `deny_unknown_fields`. A lesson is `{ order: StackedOrder (a top-first prefix completed from the decklist), opening: Vec<PlayerAction>, opponent: Vec<ScriptedAction> (a `ScriptedAgent` that plays its script when legal and otherwise the quietest legal thing), steps }`; a step is `{ prose, hint, allow: ActionPredicate, advance_when: EventPredicate, solution }`. `LessonProgress` is the pure state machine shared by TUI and gate. The loop is `netrunner_session::lesson::LessonSession`.

## 6. Gating without breaking the client contract — DONE
**A lesson step narrows `view.legal_actions`; it never widens them** and never calls `apply_action`. Predicate plus escape hatch (`a` shows the full list; an empty filter falls back to it). Two decisions made for the learner: opening actions are submitted for them, and priority is passed when it is their only legal action or the step allows none of their actions but passing.

## 7. TUI work — DONE
Coaching panel (`build_layout(area, with_coach)`), `Modal`, `explain_action` (exhaustive over every action), a run-phase strip (Null Signal's six names; *Movement* is never lit), `last_rejection` rendered, `learn list | lesson | track | game [--boosted]`. Measuring this branch is what found the heuristic's run-to-run nondeterminism (Phase 2 §5).

## 8. The lesson tracks — DONE (`feat/phase-175-tracks`)
Seven lessons a side in NSG's concept order (Corp: clicks → Hedge Fund → install and rez → ice → advance and score → a breaker steals → Urtica Cipher; Runner: credits → Cleaver and MU → a first run → breaking Palisade → stealing off R&D → Jailbreak → Tread Lightly and jacking out). Graduation launches the starter game at 6 points against the heuristic. Authoring notes: remotes from `Remote(0)`; `InstallId` from 1 in install order; a run is `InitiateRun → ContinueRun → CompleteRun → access actions → RunCompleted`; the mandatory draw is the sixth prefix card; `Step::solution` is a preference list.

## 9. Test gates — DONE
`every_lesson_is_completable_through_its_own_gates`, `every_embedded_lesson_parses_and_validates`, every gated step offers at least one action, `the_starter_matchup_plays_to_a_result_at_six_points`, and replay determinism survives `MatchRules`.
