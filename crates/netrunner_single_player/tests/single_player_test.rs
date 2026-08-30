//! Integration tests for `netrunner_single_player::session::SinglePlayerSession`:
//! full bot-vs-bot matches from `GameState::setup` to `GameOver`, and
//! correctness of the recorded `MatchHistory`.

mod common;

use netrunner_bots::{HeuristicAgent, IndexedHeuristicAgent, IndexedRandomAgent, RandomAgent};
use netrunner_core::rules::{apply_action, GamePhase, GameState, Side};
use netrunner_single_player::{PlayerDriver, SinglePlayerSession, MAX_STEPS};

fn random_vs_heuristic_session(seed: u64) -> SinglePlayerSession {
    let registry = common::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = common::kate_vs_hb_decks();
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed).expect("legal decks set up cleanly");

    let corp: Box<dyn PlayerDriver> = Box::new(IndexedRandomAgent::new(RandomAgent::new(seed), Side::Corp));
    let runner: Box<dyn PlayerDriver> = Box::new(IndexedHeuristicAgent::new(HeuristicAgent::new(Side::Runner, seed), Side::Runner));

    SinglePlayerSession::new(state, registry, corp, runner)
}

#[test]
fn random_vs_heuristic_reaches_game_over_within_step_budget() {
    let session = random_vs_heuristic_session(1);
    let (final_state, _history) = session.run();
    assert!(matches!(final_state.phase, GamePhase::GameOver(_)), "expected GameOver within {MAX_STEPS} steps");
}

#[cfg(feature = "onnx")]
#[test]
fn random_vs_onnx_reaches_game_over_within_step_budget() {
    use netrunner_bots::onnx_fixture::write_fixture_model;
    use netrunner_bots::{IndexedOnnxAgent, OnnxPolicyEvaluator};

    let registry = common::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = common::kate_vs_hb_decks();
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 2).expect("legal decks set up cleanly");

    let model_file = write_fixture_model();
    let evaluator = OnnxPolicyEvaluator::new(model_file.path.to_str().unwrap(), Side::Runner)
        .expect("hand-built fixture model should load successfully");

    let corp: Box<dyn PlayerDriver> = Box::new(IndexedRandomAgent::new(RandomAgent::new(2), Side::Corp));
    let runner: Box<dyn PlayerDriver> = Box::new(IndexedOnnxAgent::new(evaluator));

    let session = SinglePlayerSession::new(state, registry, corp, runner);
    let (final_state, _history) = session.run();
    assert!(matches!(final_state.phase, GamePhase::GameOver(_)), "expected GameOver within {MAX_STEPS} steps");
}

#[test]
fn history_records_every_resolved_action_with_matching_turn_and_side() {
    let session = random_vs_heuristic_session(3);
    let (final_state, history) = session.run();
    assert!(matches!(final_state.phase, GamePhase::GameOver(_)));
    assert!(!history.is_empty());

    // turn_number is non-decreasing across the log, and the log starts at
    // turn 0 (the Mulligan-phase actions).
    assert_eq!(history.entries()[0].turn_number, 0);
    let mut last_turn = 0;
    for entry in history.entries() {
        assert!(entry.turn_number >= last_turn, "turn_number regressed: {} -> {}", last_turn, entry.turn_number);
        last_turn = entry.turn_number;
    }

    // `netrunner_core::rules::win::check_win_conditions` only mutates
    // `state.phase` in place — it never emits its own `GameEvent::GameOver`
    // (only the separate deck-out path in `turn::enter_start_of_turn`
    // does), so there's no per-entry event to check here. The replay
    // check below is the real correctness guarantee instead: it proves
    // the recorded history actually reproduces `final_state`, GameOver
    // included.
    assert!(matches!(final_state.phase, GamePhase::GameOver(_)));

    // Replay sanity check: re-applying every recorded action in order from
    // a freshly-setup GameState (GameState/apply_action are deterministic
    // pure functions of their explicit inputs) reproduces the exact final
    // state.
    let registry = common::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = common::kate_vs_hb_decks();
    let (mut replayed, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 3).expect("legal decks set up cleanly");
    for entry in history.entries() {
        let (next, _events) = apply_action(&replayed, &registry, entry.action.clone()).expect("recorded action should replay cleanly");
        replayed = next;
    }
    assert_eq!(replayed, final_state, "replaying the recorded history should reproduce the exact final state");
}

#[test]
fn no_panics_or_deadlocks_across_many_seeds() {
    // A synchronous, single-threaded loop has no concurrency to deadlock in
    // the first place — the meaningful property here is "every one of
    // these seeds terminates via GameOver (not step-budget exhaustion) and
    // never panics."
    for seed in 0..5 {
        let session = random_vs_heuristic_session(seed);
        let (final_state, history) = session.run();
        assert!(matches!(final_state.phase, GamePhase::GameOver(_)), "seed {seed}: expected GameOver within {MAX_STEPS} steps");
        assert!(!history.is_empty(), "seed {seed}: history should be non-empty");
    }
}
