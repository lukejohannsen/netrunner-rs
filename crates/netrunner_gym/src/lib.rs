//! A Gymnasium/Stable-Baselines3-facing RL environment over
//! `netrunner_core`'s fixed 724-slot `ActionSpace`/`get_action_mask`.
//!
//! Split into two layers:
//! - `env` (plus `fixtures`): a plain-Rust environment with no PyO3 types
//!   in its API, fully unit-testable via plain `cargo test`. Its
//!   observation encoder lives in `netrunner_bots::observation` (shared
//!   with `netrunner_bots::onnx_policy`, not owned here — `netrunner_gym`
//!   already depends on `netrunner_bots`, and the reverse would be a
//!   dependency cycle) and is re-exported below for convenience.
//! - `python`: a thin `#[pyclass]` shim translating to Python-native types
//!   only (no `numpy`), registered below as the `netrunner_gym` extension
//!   module. The real `gymnasium.Env` subclass (`action_space`/
//!   `observation_space`, numpy dtypes) is pure Python, layered on top of
//!   this in `python/netrunner_gym/env.py` — see that file, not this
//!   crate, for the Gymnasium-facing API surface.
//!
//! `extension-module` (a Cargo feature, off by default — see `Cargo.toml`)
//! is what `maturin` enables to build the real importable wheel; default
//! `cargo build/test/clippy` link against the system `libpython` instead,
//! so this compiles and its tests run with no Python packaging tooling
//! installed at all.

pub mod env;
pub mod fixtures;
mod python;

pub use env::{NetrunnerEnv, Opponent, OutOfRangeIndex, StepOutcome, ACTION_SPACE_SIZE};
pub use netrunner_bots::observation::{encode_observation, OBS_SIZE};

use pyo3::prelude::*;

// Named `_netrunner_gym` (not `netrunner_gym`) to match the compiled
// module's importable path per `pyproject.toml`'s `module-name =
// "netrunner_gym._netrunner_gym"` — PyO3 derives the C `PyInit_<name>`
// symbol from this function's name, and it must match the final
// component of that import path for Python to load it. The pure-Python
// `python/netrunner_gym/__init__.py` imports from `._netrunner_gym` and
// is the actual `netrunner_gym` package name users see.
#[pymodule]
fn _netrunner_gym(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    python::register(m)
}
