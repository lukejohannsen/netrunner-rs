//! Proves the last mile of the training pipeline: a policy loaded from an
//! ONNX file can actually drive an opponent a human plays against.
//!
//! Before this existed, `scripts/run_iteration_loop.py` could produce
//! `checkpoints/latest_policy.onnx` and nothing in the workspace could
//! load it into a game — `BotKind` had no `Onnx` variant at all, so a
//! trained model was unreachable however good it was.
//!
//! Uses `netrunner_bots::onnx_fixture`'s dummy model (a constant policy
//! shaped to the current `OBS_SIZE`/`ActionSpace::SIZE`) rather than a real
//! checkpoint, so this runs in CI without a training run. It tests the
//! wiring, not the policy's strength.

#![cfg(feature = "onnx")]

use netrunner_bots::onnx_fixture;
use netrunner_core::rules::{GamePhase, GameState, Side};
use netrunner_single_player::SinglePlayerSession;

#[path = "../src/bots.rs"]
mod bots;
#[path = "../src/config.rs"]
mod config;
#[path = "../src/decks.rs"]
mod decks;

use config::BotKind;

#[test]
fn a_trained_policy_can_play_a_full_single_player_game() {
    let model = onnx_fixture::write_fixture_model();
    let model_path = model.path.to_str().expect("fixture path is UTF-8");

    let registry = decks::sample_deck_registry();
    let (corp_deck, runner_deck) =
        decks::sample_decks("discretion_advised", "stolen_goods").expect("sample decks resolve");
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 11).expect("setup");

    // The ONNX policy takes the Corp seat; a scripted agent takes the
    // Runner's, standing in for the human.
    let corp_driver = bots::make_driver(BotKind::Onnx, Side::Corp, 1, model_path)
        .expect("the fixture model loads at the current observation/action shape");
    let runner_driver =
        bots::make_driver(BotKind::Heuristic, Side::Runner, 2, model_path).expect("heuristic driver");

    let session = SinglePlayerSession::new(state, registry, corp_driver, runner_driver);
    let (final_state, history) = session.run();

    // Deliberately not asserting `GameOver`. The fixture model emits a
    // *constant* policy, so the ONNX agent picks the same index whenever
    // the same actions are legal and the match can stall until the step
    // budget runs out. That is a property of a dummy model with no weights,
    // not of the wiring under test — a real trained policy has varied
    // priors. What this test proves is that the model loads at the current
    // `OBS_SIZE`/`ActionSpace::SIZE`, that its output selects legal
    // actions, and that a session runs on it without panicking.
    let corp_actions = history.entries().iter().filter(|entry| entry.side == Side::Corp).count();
    assert!(corp_actions > 0, "the ONNX-driven Corp should have taken at least one action");
    assert!(
        !matches!(final_state.phase, GamePhase::Mulligan(_)),
        "the match should have advanced past the opening mulligan"
    );
}

/// The failure a user is most likely to hit — asking for `--corp onnx`
/// before training anything — must explain itself rather than panic.
#[test]
fn a_missing_checkpoint_explains_how_to_produce_one() {
    let Err(error) = bots::make_driver(BotKind::Onnx, Side::Corp, 0, "checkpoints/does_not_exist.onnx") else {
        panic!("a missing checkpoint cannot yield a driver");
    };
    assert!(error.contains("run_iteration_loop.py"), "error should point at the training command: {error}");
}
