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
///
/// How a searching agent is configured, as one value rather than a
/// widening argument list — `make_agent_with_model` was at clippy's limit
/// with these spread out, and every knob here is one a *measurement*
/// sets, so they travel together by nature.
///
/// [`make_driver`] deliberately does not take one: an index-based
/// `netrunner_bots::Agent` has no determinization of its own to
/// configure, and giving it a config it would ignore is worse than the
/// asymmetry.
#[derive(Debug, Clone, Copy)]
pub struct AgentSetup {
    /// Per-decision search budget for `Mcts` and the `Puct` kinds; the
    /// others ignore it.
    pub simulations: usize,
    /// How many independent samples of the hidden state one decision
    /// searches, the `simulations` budget split evenly across them.
    /// **One field, because it is one dial** — `MctsAgent` calls it
    /// `trees` (root-parallel searches) and `PuctConfig` calls it
    /// `samples`, and the two searches disagreeing about what it is worth
    /// is the whole of ROADMAP Phase 2 §5 item 35.
    ///
    /// `None` means each agent's own default — `MctsAgent::DEFAULT_TREES`
    /// (4) and `PuctConfig::default().samples` (1), both fixed numbers.
    /// Until ROADMAP Phase 2 §5 item 41 the `Mcts` default was
    /// `rayon::current_num_threads().clamp(1, 4)`, so **`mcts` was a
    /// different bot on a smaller box, and `--threads` changed it here**:
    /// 52 of 192 games moved between `--threads 2` and `--threads 18` on
    /// one seed. It no longer does, and this stays the way a measurement
    /// pins the dial rather than the way a run becomes reproducible.
    pub determinizations: Option<usize>,
    /// `Mcts` only: every tree searches one shared sample of the hidden
    /// state instead of its own. Diagnostic — see
    /// `MctsAgent::with_shared_sample` for what it separates.
    pub shared_sample: bool,
    /// Biases the evaluator every kind but `Random` scores with (`Random`
    /// has no evaluator, and a network-backed `PuctOnnx` has its own
    /// value head).
    pub personality: Personality,
}

impl AgentSetup {
    /// The defaults a seat gets when nobody is measuring anything: each
    /// agent's own determinization count, no shared sample, balanced.
    pub fn new(simulations: usize) -> Self {
        Self { simulations, determinizations: None, shared_sample: false, personality: Personality::Balanced }
    }

    pub fn with_personality(mut self, personality: Personality) -> Self {
        self.personality = personality;
        self
    }
}

pub fn make_agent(kind: BotKind, side: Side, seed: u64, setup: AgentSetup) -> Option<Box<dyn BotAgent>> {
    let AgentSetup { simulations, determinizations, shared_sample, personality } = setup;
    match kind {
        BotKind::Human | BotKind::Onnx | BotKind::PuctOnnx => None,
        BotKind::Random => Some(Box::new(RandomAgent::new(seed))),
        BotKind::Heuristic => Some(Box::new(HeuristicAgent::with_personality(side, seed, personality))),
        BotKind::Mcts => {
            let agent = match determinizations {
                Some(trees) => MctsAgent::with_trees(side, seed, simulations, trees),
                None => MctsAgent::with_iterations(side, seed, simulations),
            };
            Some(Box::new(agent.with_personality(personality).with_shared_sample(shared_sample)))
        }
        BotKind::Puct => Some(Box::new(PuctAgent::with_config(
            side,
            seed,
            UniformPolicyEvaluator::with_personality(side, personality),
            PuctConfig {
                iterations: simulations,
                samples: determinizations.unwrap_or(PuctConfig::default().samples),
                ..PuctConfig::default()
            },
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
    setup: AgentSetup,
    model_path: &str,
) -> Result<Option<Box<dyn BotAgent>>, String> {
    match kind {
        BotKind::PuctOnnx => make_puct_onnx_agent(side, seed, setup, model_path).map(Some),
        _ => Ok(make_agent(kind, side, seed, setup)),
    }
}

#[cfg(feature = "onnx")]
fn make_puct_onnx_agent(side: Side, seed: u64, setup: AgentSetup, model_path: &str) -> Result<Box<dyn BotAgent>, String> {
    use netrunner_bots::OnnxPolicyEvaluator;

    let evaluator = OnnxPolicyEvaluator::new(model_path, side).map_err(|e| onnx_load_error(model_path, &e))?;
    let config = PuctConfig {
        iterations: setup.simulations,
        samples: setup.determinizations.unwrap_or(PuctConfig::default().samples),
        ..PuctConfig::default()
    };
    Ok(Box::new(PuctAgent::with_config(side, seed, evaluator, config)))
}

#[cfg(not(feature = "onnx"))]
fn make_puct_onnx_agent(_side: Side, _seed: u64, _setup: AgentSetup, _model_path: &str) -> Result<Box<dyn BotAgent>, String> {
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
            let agent = make_agent_with_model(kind, side, seed, AgentSetup::new(simulations).with_personality(personality), model_path)?
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
        let setup = AgentSetup::new(8);
        assert!(make_agent(BotKind::Human, Side::Corp, 0, setup).is_none());
        assert!(make_agent(BotKind::Onnx, Side::Corp, 0, setup).is_none());
        assert!(make_agent(BotKind::PuctOnnx, Side::Corp, 0, setup).is_none(), "needs a model path — see make_agent_with_model");
    }

    /// `puct-onnx` is the one kind whose `BotAgent` form can fail to build,
    /// and it must fail readably whether the feature is on (no such file)
    /// or off.
    #[test]
    fn puct_onnx_without_a_model_is_a_readable_error() {
        let Err(error) = make_agent_with_model(BotKind::PuctOnnx, Side::Corp, 0, AgentSetup::new(8), "/nonexistent/model.onnx") else {
            panic!("a missing model cannot produce an agent");
        };
        assert!(!error.is_empty());
        let Err(error) = make_driver(BotKind::PuctOnnx, Side::Corp, 0, 8, "/nonexistent/model.onnx", Personality::Balanced) else {
            panic!("a missing model cannot produce a driver");
        };
        assert!(!error.is_empty());
    }

    /// The knobs exist so a measurement can pin what the defaults leave
    /// loose — the tree/sample count is the dial ROADMAP Phase 2 §5 item
    /// 35 measured. The guard here is only that both are accepted by
    /// every kind and both sides, since neither is observable through
    /// `BotAgent`; that the *default* no longer moves with the host is
    /// `the_default_tree_count_does_not_move_with_the_host_pool`.
    #[test]
    fn every_kind_accepts_an_explicit_determinization_count() {
        for kind in [BotKind::Random, BotKind::Heuristic, BotKind::Mcts, BotKind::Puct] {
            for side in [Side::Corp, Side::Runner] {
                for count in [1, 2, 4] {
                    let setup = AgentSetup { determinizations: Some(count), ..AgentSetup::new(8) };
                    assert!(make_agent(kind, side, 7, setup).is_some(), "{kind:?} {side:?}");
                    assert!(make_agent(kind, side, 7, AgentSetup { shared_sample: true, ..setup }).is_some(), "{kind:?} {side:?}");
                }
            }
        }
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
