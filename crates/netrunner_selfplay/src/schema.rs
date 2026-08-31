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
    /// Which sample-deck pairing produced this game
    /// (`"<corp_deck_id>_vs_<runner_deck_id>"`).
    ///
    /// Recorded so a training set stays attributable to the decks behind
    /// it. Without this, the previous pipeline's 5,000 games gave no way to
    /// tell from the data that every one of them came from a single broken
    /// fixture in which the Corp lost 100% of the time.
    pub matchup: String,
}
