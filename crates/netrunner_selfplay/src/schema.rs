//! On-disk shape of one self-play game, written as a single JSON line per
//! `GameTrajectory` (see `main.rs`'s output writer) for an eventual
//! training pipeline to consume.

use serde::{Deserialize, Serialize};

/// One recorded decision point: the acting side's observation, the search's
/// resulting policy target, and which action was actually taken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfPlayStep {
    /// `netrunner_bots::observation::encode_observation`'s output verbatim
    /// (length `netrunner_bots::OBS_SIZE`).
    pub observation: Vec<f32>,
    /// Normalized `PuctAgent::search` visit counts, length
    /// `netrunner_core::rules::ActionSpace::SIZE`.
    pub policy_target: Vec<f32>,
    /// `ActionSpace` index of the action actually applied to the game.
    pub action_taken: usize,
    /// `Side::Corp as u8 == 0`, `Side::Runner as u8 == 1`.
    pub active_side: u8,
}

/// One full self-play game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameTrajectory {
    pub steps: Vec<SelfPlayStep>,
    /// `+1.0` Corp win, `-1.0` Runner win, `0.0` draw / step-limit cutoff.
    pub outcome_corp: f32,
}
