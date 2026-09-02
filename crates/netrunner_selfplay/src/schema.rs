//! On-disk shape of one self-play game, written as a single JSON line per
//! `GameTrajectory` (see `main.rs`'s output writer) for
//! `scripts/train_alpha_netrunner.py` to consume.
//!
//! Both per-step vectors are stored **sparse**, as `[index, value]` pairs
//! of their nonzero entries. Measured on the September 2026 corpus, an
//! observation has ~29 nonzero entries of `OBS_SIZE` 990 and a policy
//! target ~8 of `ActionSpace::SIZE` 1,646, so the dense form was ~97–99%
//! zeros written out as text: 1.9 MB per game, 190 MB per 96-game
//! iteration, and a trainer that held every zero as a `float32` — the
//! constraint that capped an iteration at 96 games when the open item is
//! more data per iteration (ROADMAP Phase 2 §5). The widths the pairs index
//! into are recorded once per game so a reader densifies against the
//! layout that produced the file and can refuse to mix layouts, which is
//! how August's 1,357-wide targets would have been caught instead of moved
//! aside by hand.

use serde::{Deserialize, Serialize};

/// The nonzero entries of a dense vector, in ascending index order.
pub type SparseVec = Vec<(usize, f32)>;

/// `dense` as its nonzero `(index, value)` pairs.
pub fn sparse(dense: &[f32]) -> SparseVec {
    dense.iter().enumerate().filter(|(_, v)| **v != 0.0).map(|(i, v)| (i, *v)).collect()
}

/// One recorded decision point: the acting side's observation, the search's
/// resulting policy target, and which action was actually taken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfPlayStep {
    /// `netrunner_bots::observation::encode_observation`'s output, sparse;
    /// dense length is `GameTrajectory::observation_size`.
    pub observation: SparseVec,
    /// Normalized `PuctAgent::search` visit counts keyed in the real
    /// state's `ActionSpace`, sparse; dense length is
    /// `GameTrajectory::action_space_size`.
    pub policy_target: SparseVec,
    /// `ActionSpace` index of the action actually applied to the game.
    pub action_taken: usize,
    /// `Side::Corp as u8 == 0`, `Side::Runner as u8 == 1`.
    pub active_side: u8,
}

/// One full self-play game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameTrajectory {
    /// `netrunner_bots::OBS_SIZE` when this game was recorded.
    pub observation_size: usize,
    /// `netrunner_core::rules::ActionSpace::SIZE` when this game was recorded.
    pub action_space_size: usize,
    /// The seed `GameState::setup` was given — `--seed-offset` plus the
    /// game's index — so a game is re-playable and a corpus can be checked
    /// for the duplicate seeds that `--seed-offset` exists to prevent.
    pub seed: u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_keeps_exactly_the_nonzero_entries_in_index_order() {
        assert_eq!(sparse(&[0.0, 0.5, 0.0, 0.0, 0.25, -1.0]), vec![(1, 0.5), (4, 0.25), (5, -1.0)]);
        assert!(sparse(&[0.0; 8]).is_empty());
        assert!(sparse(&[]).is_empty());
    }

    /// A pair serializes as a two-element JSON array, which is what the
    /// trainer's reader expects (`[index, value]`), not an object.
    #[test]
    fn a_sparse_vector_serializes_as_index_value_pairs() {
        let json = serde_json::to_string(&sparse(&[0.0, 0.5, 0.0, 1.0])).unwrap();
        assert_eq!(json, "[[1,0.5],[3,1.0]]");
    }
}
