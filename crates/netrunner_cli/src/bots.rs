//! Bridges `config::BotKind` selection to `netrunner_bots` agents. Whose
//! decision is pending right now (`netrunner_core::rules::current_actor`)
//! now lives in `netrunner_core` itself — both
//! `netrunner_server::MatchSession` and this crate need it, so it no longer
//! belongs only here.
//!
//! Two factories, because the two agent families have different shapes.
//! [`make_agent`] returns a view-based `BotAgent` — what a
//! `netrunner_session::Seat::Agent` takes, and therefore what both the
//! server path and the local TUI's bot seat use. [`make_driver`] returns an
//! index-based `netrunner_bots::Agent` for the `SinglePlayerSession`
//! adapter, which is also the only place `BotKind::Onnx` works — a shape
//! restriction, not a privacy one.
//! `OnnxPolicyEvaluator` takes a whole `GameState` but encodes through
//! `encode_observation`, which builds a `ClientView` for its own side, so
//! its features are masked exactly like the view-based agents'. It simply
//! has no `BotAgent` form to hand a `PlayerSlot::Bot`.

use netrunner_bots::{
    BotAgent, BotAgentIndexAdapter, HeuristicAgent, MctsAgent, Personality, PuctAgent, PuctConfig, RandomAgent,
    UniformPolicyEvaluator,
};
use netrunner_core::rules::Side;
use netrunner_bots::Agent;

use crate::config::BotKind;

/// `Human => None` — no agent drives that side; the CLI hosts it as the
/// human seat instead (see `config::Config::corp`'s doc comment).
///
/// `Onnx => None` too, but for a different reason: it has no `BotAgent`
/// form. Callers on this path should reject it up front rather than treat
/// the `None` as a human seat — [`make_driver`] is the supported route.
///
/// `simulations` is the per-decision search budget for the two search
/// agents (`Mcts`, `Puct`); the others ignore it. `personality` biases
/// the evaluator every kind but `Random` scores with (`Random` has no
/// evaluator, and a network-backed `PuctOnnx` has its own value head).
pub fn make_agent(kind: BotKind, side: Side, seed: u64, simulations: usize, personality: Personality) -> Option<Box<dyn BotAgent>> {
    match kind {
        BotKind::Human | BotKind::Onnx | BotKind::PuctOnnx => None,
        BotKind::Random => Some(Box::new(RandomAgent::new(seed))),
        BotKind::Heuristic => Some(Box::new(HeuristicAgent::with_personality(side, seed, personality))),
        BotKind::Mcts => Some(Box::new(MctsAgent::with_iterations(side, seed, simulations).with_personality(personality))),
        BotKind::Puct => Some(Box::new(PuctAgent::with_config(
            side,
            seed,
            UniformPolicyEvaluator::with_personality(side, personality),
            PuctConfig { iterations: simulations, ..PuctConfig::default() },
        ))),
    }
}

/// [`make_agent`] for callers that also hold `--model`: the same kinds,
/// plus `PuctOnnx` — `PuctAgent` searching over `OnnxPolicyEvaluator`
/// instead of the uniform one. `Err` is the readable model-loading failure
/// (`make_driver`'s contract); `Ok(None)` still means "no `BotAgent` form"
/// (`Human`, `Onnx`).
pub fn make_agent_with_model(
    kind: BotKind,
    side: Side,
    seed: u64,
    simulations: usize,
    model_path: &str,
    personality: Personality,
) -> Result<Option<Box<dyn BotAgent>>, String> {
    match kind {
        BotKind::PuctOnnx => make_puct_onnx_agent(side, seed, simulations, model_path).map(Some),
        _ => Ok(make_agent(kind, side, seed, simulations, personality)),
    }
}

#[cfg(feature = "onnx")]
fn make_puct_onnx_agent(side: Side, seed: u64, simulations: usize, model_path: &str) -> Result<Box<dyn BotAgent>, String> {
    use netrunner_bots::OnnxPolicyEvaluator;

    let evaluator = OnnxPolicyEvaluator::new(model_path, side).map_err(|e| onnx_load_error(model_path, &e))?;
    Ok(Box::new(PuctAgent::with_config(side, seed, evaluator, PuctConfig { iterations: simulations, ..PuctConfig::default() })))
}

#[cfg(not(feature = "onnx"))]
fn make_puct_onnx_agent(_side: Side, _seed: u64, _simulations: usize, _model_path: &str) -> Result<Box<dyn BotAgent>, String> {
    Err(NO_ONNX_FEATURE.to_string())
}

#[cfg(feature = "onnx")]
fn onnx_load_error(model_path: &str, error: &dyn std::fmt::Display) -> String {
    format!(
        "could not load the ONNX policy at {model_path:?}: {error}\n\
         Train one first with:\n  \
         python3 scripts/run_iteration_loop.py --iterations 50 --games-per-iter 100 --simulations 200"
    )
}

#[cfg(not(feature = "onnx"))]
const NO_ONNX_FEATURE: &str = "this binary was built without the `onnx` feature; rebuild with \
     `cargo run -p netrunner_cli --features onnx -- ...` to play against a trained policy";

/// An index-based `netrunner_bots::Agent` for the `SinglePlayerSession`
/// path.
///
/// `Err` carries a message meant to be shown to the user: an unsupported
/// `BotKind` for this path, or an ONNX model that could not be loaded
/// (missing file, wrong input/output shape for the current `OBS_SIZE` /
/// `ActionSpace::SIZE`).
pub fn make_driver(
    kind: BotKind,
    side: Side,
    seed: u64,
    simulations: usize,
    model_path: &str,
    personality: Personality,
) -> Result<Box<dyn Agent>, String> {
    match kind {
        BotKind::Human => Err("make_driver was asked for a bot driver for the human seat".to_string()),
        BotKind::Onnx => make_onnx_driver(side, model_path),
        _ => {
            let agent = make_agent_with_model(kind, side, seed, simulations, model_path, personality)?
                .expect("every kind but Human and Onnx yields a BotAgent");
            Ok(Box::new(BotAgentIndexAdapter::new(agent, side)))
        }
    }
}

#[cfg(feature = "onnx")]
fn make_onnx_driver(side: Side, model_path: &str) -> Result<Box<dyn Agent>, String> {
    use netrunner_bots::{IndexedOnnxAgent, OnnxPolicyEvaluator};

    let evaluator = OnnxPolicyEvaluator::new(model_path, side).map_err(|e| onnx_load_error(model_path, &e))?;
    Ok(Box::new(IndexedOnnxAgent::new(evaluator)))
}

#[cfg(not(feature = "onnx"))]
fn make_onnx_driver(_side: Side, _model_path: &str) -> Result<Box<dyn Agent>, String> {
    Err(NO_ONNX_FEATURE.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_and_onnx_have_no_bot_agent_form() {
        assert!(make_agent(BotKind::Human, Side::Corp, 0, 8, Personality::Balanced).is_none());
        assert!(make_agent(BotKind::Onnx, Side::Corp, 0, 8, Personality::Balanced).is_none());
        assert!(make_agent(BotKind::PuctOnnx, Side::Corp, 0, 8, Personality::Balanced).is_none(), "needs a model path — see make_agent_with_model");
    }

    /// `puct-onnx` is the one kind whose `BotAgent` form can fail to build,
    /// and it must fail readably whether the feature is on (no such file)
    /// or off.
    #[test]
    fn puct_onnx_without_a_model_is_a_readable_error() {
        let Err(error) = make_agent_with_model(BotKind::PuctOnnx, Side::Corp, 0, 8, "/nonexistent/model.onnx", Personality::Balanced) else {
            panic!("a missing model cannot produce an agent");
        };
        assert!(!error.is_empty());
        let Err(error) = make_driver(BotKind::PuctOnnx, Side::Corp, 0, 8, "/nonexistent/model.onnx", Personality::Balanced) else {
            panic!("a missing model cannot produce a driver");
        };
        assert!(!error.is_empty());
    }

    #[test]
    fn the_scripted_kinds_all_produce_drivers() {
        for kind in [BotKind::Random, BotKind::Heuristic, BotKind::Mcts, BotKind::Puct] {
            assert!(make_driver(kind, Side::Corp, 7, 8, "unused.onnx", Personality::Balanced).is_ok(), "{kind:?}");
        }
    }

    #[test]
    fn asking_for_a_driver_for_the_human_seat_is_an_error() {
        assert!(make_driver(BotKind::Human, Side::Corp, 0, 8, "unused.onnx", Personality::Balanced).is_err());
    }

    /// Whether the feature is on or off, a missing model must surface as a
    /// readable message rather than a panic.
    #[test]
    fn a_missing_onnx_model_is_a_readable_error() {
        let Err(error) = make_driver(BotKind::Onnx, Side::Corp, 0, 8, "/nonexistent/model.onnx", Personality::Balanced) else {
            panic!("a missing model cannot produce a driver");
        };
        assert!(!error.is_empty());
    }
}
