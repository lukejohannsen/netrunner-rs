//! Thin PyO3 binding over `crate::env::NetrunnerEnv`. Translates to
//! Python-native types only (`list`, `tuple`, `bool`, `f32`, `usize`,
//! `String`) — no `numpy` — so this can be exercised by real in-process
//! `Python::attach` tests using only the system interpreter, with no
//! `pip`/`venv`/`maturin`/`numpy`/`gymnasium` involved. The actual
//! `gymnasium.Env` subclass (which does need `numpy`/`gymnasium`) is pure
//! Python, layered on top of this in `python/netrunner_gym/env.py`.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use netrunner_core::rules::Side;

use crate::env::{NetrunnerEnv, Opponent, OutOfRangeIndex, ACTION_SPACE_SIZE};
use netrunner_bots::observation::OBS_SIZE;

fn parse_side(side: &str) -> PyResult<Side> {
    match side {
        "corp" => Ok(Side::Corp),
        "runner" => Ok(Side::Runner),
        other => Err(PyValueError::new_err(format!("unknown side {other:?}; expected \"corp\" or \"runner\""))),
    }
}

fn parse_opponent(opponent: &str) -> PyResult<Opponent> {
    opponent.parse().map_err(PyValueError::new_err)
}

/// `(observation, action_mask, reward, terminated, truncated,
/// invalid_action, message)` — see `PyNetrunnerEnv::step`'s doc comment.
type StepResult = (Vec<f32>, Vec<bool>, f32, bool, bool, bool, String);

// `unsendable`: `NetrunnerEnv` holds a `Box<dyn BotAgent>` (only `Send`,
// not `Sync` — `netrunner_bots::BotAgent` doesn't require `Sync`, and
// shouldn't need to just to be usable from Python). A Gym-style env is a
// single-consumer object by nature anyway — nothing here is meant to be
// shared across native threads — so this just restricts each instance to
// the Python thread that created it, which is already how it's used.
#[pyclass(name = "NetrunnerEnv", unsendable)]
pub struct PyNetrunnerEnv {
    inner: NetrunnerEnv,
}

#[pymethods]
impl PyNetrunnerEnv {
    #[new]
    #[pyo3(signature = (side="runner", seed=0, opponent="heuristic", max_episode_steps=200))]
    fn new(side: &str, seed: u64, opponent: &str, max_episode_steps: u32) -> PyResult<Self> {
        let side = parse_side(side)?;
        let opponent = parse_opponent(opponent)?;
        Ok(PyNetrunnerEnv { inner: NetrunnerEnv::new(side, seed, opponent, max_episode_steps) })
    }

    /// Returns `(observation, action_mask)`.
    #[pyo3(signature = (seed=None))]
    fn reset(&mut self, seed: Option<u64>) -> (Vec<f32>, Vec<bool>) {
        self.inner.reset(seed)
    }

    /// Returns `(observation, action_mask, reward, terminated, truncated,
    /// invalid_action, message)`. Raises `ValueError` only for `index`
    /// outside `0..ACTION_SPACE_SIZE`; an in-range but currently-illegal
    /// index instead comes back with `invalid_action=True` and a nonzero
    /// penalty `reward` — see `crate::env`'s module doc comment for why.
    fn step(&mut self, index: usize) -> PyResult<StepResult> {
        match self.inner.step_index(index) {
            Ok(outcome) => Ok((
                outcome.observation,
                outcome.action_mask,
                outcome.reward,
                outcome.terminated,
                outcome.truncated,
                outcome.invalid_action,
                outcome.message.unwrap_or_default(),
            )),
            Err(OutOfRangeIndex(index)) => {
                Err(PyValueError::new_err(format!("action index {index} is out of range for ACTION_SPACE_SIZE={ACTION_SPACE_SIZE}")))
            }
        }
    }

    fn action_mask(&self) -> Vec<bool> {
        self.inner.action_mask()
    }

    fn observation(&self) -> Vec<f32> {
        self.inner.observation()
    }

    fn is_over(&self) -> bool {
        self.inner.is_over()
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ACTION_SPACE_SIZE", ACTION_SPACE_SIZE)?;
    m.add("OBS_SIZE", OBS_SIZE)?;
    m.add_class::<PyNetrunnerEnv>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_resets_and_steps_without_a_python_process() {
        Python::attach(|py| {
            let env = Py::new(py, PyNetrunnerEnv::new("runner", 1, "random", 50).unwrap()).unwrap();
            let bound = env.bind(py);

            let (obs, mask): (Vec<f32>, Vec<bool>) = bound.borrow_mut().reset(Some(1));
            assert_eq!(obs.len(), OBS_SIZE);
            assert_eq!(mask.len(), ACTION_SPACE_SIZE);

            let legal_index = mask.iter().position(|&legal| legal).expect("fresh episode has a legal action");
            let (obs, mask, _reward, _terminated, _truncated, invalid_action, _message) =
                bound.borrow_mut().step(legal_index).unwrap();
            assert_eq!(obs.len(), OBS_SIZE);
            assert_eq!(mask.len(), ACTION_SPACE_SIZE);
            assert!(!invalid_action);
        });
    }

    #[test]
    fn out_of_range_index_raises_value_error() {
        Python::attach(|py| {
            let env = Py::new(py, PyNetrunnerEnv::new("corp", 2, "random", 50).unwrap()).unwrap();
            let bound = env.bind(py);

            let result = bound.borrow_mut().step(ACTION_SPACE_SIZE + 1);
            assert!(result.is_err());
            let error = result.unwrap_err();
            assert!(Python::attach(|py| error.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    fn masked_out_index_does_not_raise() {
        Python::attach(|py| {
            let env = Py::new(py, PyNetrunnerEnv::new("corp", 3, "random", 50).unwrap()).unwrap();
            let bound = env.bind(py);

            let (_obs, mask) = bound.borrow_mut().reset(Some(3));
            let illegal_index = mask.iter().position(|&legal| !legal).expect("mulligan phase never has every slot legal");

            let (_obs, _mask, reward, terminated, _truncated, invalid_action, message) =
                bound.borrow_mut().step(illegal_index).unwrap();
            assert!(invalid_action);
            assert!(reward < 0.0);
            assert!(!terminated);
            assert!(!message.is_empty());
        });
    }

    #[test]
    fn unknown_side_raises_value_error() {
        let result = PyNetrunnerEnv::new("corporation", 1, "random", 50);
        assert!(result.is_err());
    }

    /// Pinned deliberately: the Python-side observation/action shapes are
    /// built against this constant, so a change here is a breaking change
    /// for any trained policy and must be an explicit decision, not a
    /// silent consequence of adding a card mechanic. Grew 724 → 1024 across
    /// the System Gateway work, which added several new player-facing
    /// decisions (card selection, server choice, paid-choice accept/decline,
    /// install-on-ice, click-to-break), then 1024 → 1025 for the Corp's
    /// basic purge-virus-counters action, then 1025 → 1045 for
    /// `ChooseTriggerToResolve` (ordering your own simultaneous triggers),
    /// then 1045 → 1357 when `MAX_INSTALLED_PER_SIDE` went 20 → 32 after
    /// real games overflowed it.
    ///
    /// The first three growths were *appended*, so every pre-existing
    /// index kept its meaning and only the head width changed. The
    /// `MAX_INSTALLED_PER_SIDE` one could not be: that constant sizes 26
    /// segments spread through the space, so indices **shift** and an
    /// exported policy needs retraining rather than a resize. Keep
    /// appending where there is a choice.
    #[test]
    fn action_space_size_constant_is_pinned() {
        assert_eq!(ACTION_SPACE_SIZE, 1357);
    }

    /// Pinned for the same reason as `ACTION_SPACE_SIZE`: it is the model's
    /// input width, baked into every exported ONNX file, so a change must
    /// be deliberate rather than a side effect of touching the encoder.
    /// Grew 30 → 990 when card-identity planes were added — the scalar-only
    /// encoding made every same-size hand look identical to the network,
    /// which capped a trained policy at generic tempo play.
    #[test]
    fn obs_size_constant_is_pinned() {
        assert_eq!(OBS_SIZE, 990);
    }
}
