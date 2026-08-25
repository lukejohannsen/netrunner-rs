//! A fixed-size `f32` feature encoding of a `ClientView`, for the `"obs"`
//! half of the Gymnasium `Dict` observation space (`"action_mask"` is
//! `netrunner_core::rules::get_action_mask` directly — see `crate::env`).
//!
//! Deliberately a simple, self-contained baseline: scalar counts and
//! normalized resource values only, no card-identity embedding. A user
//! wiring up a real policy network is expected to replace or extend this,
//! the same way `netrunner_bots::policy::UniformPolicyEvaluator` is a
//! baseline meant to be swapped for a trained network rather than the
//! final word on evaluation.

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{ActionSpace, GamePhase, GameState, ServerId, Side};
use netrunner_core::view::{build_client_view, ClientView};

// Normalization caps. Independently defined rather than imported from
// `netrunner_core::rules::action_mask`'s own `MAX_*` constants — that
// module is private, and only `ActionSpace`/`get_action_mask` are
// re-exported from `rules` — so these are this crate's own reasonable
// caps for scaling raw counts into a roughly-unit range, not a claim that
// they bound what's legal.
const MAX_HAND_SIZE: f32 = 12.0;
const MAX_INSTALLED_PER_SIDE: f32 = 20.0;
const MAX_REMOTE_SERVERS: f32 = 10.0;
const MAX_DECK_SIZE: f32 = 45.0;
const MAX_CREDITS: f32 = 30.0;
const MAX_CLICKS: f32 = 5.0;
const MAX_AGENDA_POINTS: f32 = 10.0;
const MAX_BAD_PUBLICITY: f32 = 10.0;
const MAX_TAGS: f32 = 10.0;
const MAX_BRAIN_DAMAGE: f32 = 10.0;
const MAX_MEMORY_UNITS: f32 = 10.0;
const MAX_LINK_STRENGTH: f32 = 5.0;
const MAX_RUN_ICE: f32 = 10.0;

const PHASE_COUNT: usize = 5;

/// side(1) + phase one-hot(5) + self/opp credits+clicks+agenda_points(6) +
/// corp bad publicity(1) + runner tags/brain damage/memory/link(4) +
/// zone counts: hq/rd/archives/grip/stack/heap/rig/installed-ice/
/// installed-root/remote-servers(10) + active-run flag + normalized run
/// position(2) + normalized legal action count(1).
pub const OBS_SIZE: usize = 1 + PHASE_COUNT + 6 + 1 + 4 + 10 + 2 + 1;

fn norm(value: f32, max: f32) -> f32 {
    if max <= 0.0 {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    }
}

fn phase_one_hot(phase: GamePhase) -> [f32; PHASE_COUNT] {
    let mut one_hot = [0.0; PHASE_COUNT];
    let index = match phase {
        GamePhase::Mulligan(_) => 0,
        GamePhase::StartOfTurn(_) => 1,
        GamePhase::Action(_) => 2,
        GamePhase::Discard { .. } => 3,
        GamePhase::GameOver(_) => 4,
    };
    one_hot[index] = 1.0;
    one_hot
}

/// Encodes `state` from `side`'s perspective into a fixed `OBS_SIZE`-length
/// vector, via `build_client_view` (so hidden zones are already correctly
/// Fog-of-War-collapsed to counts before anything here sees them).
pub fn encode_observation(state: &GameState, registry: &CardRegistry, side: Side) -> Vec<f32> {
    let view = build_client_view(state, registry, side);
    encode_view(&view)
}

fn encode_view(view: &ClientView) -> Vec<f32> {
    let (self_credits, self_clicks, self_agenda_points, opp_credits, opp_clicks, opp_agenda_points) = match view.side {
        Side::Corp => (
            view.corp.credits,
            view.corp.clicks,
            view.corp.agenda_points,
            view.runner.credits,
            view.runner.clicks,
            view.runner.agenda_points,
        ),
        Side::Runner => (
            view.runner.credits,
            view.runner.clicks,
            view.runner.agenda_points,
            view.corp.credits,
            view.corp.clicks,
            view.corp.agenda_points,
        ),
    };

    let installed_ice_count: usize = view.corp.servers.iter().map(|server| server.ice.len()).sum();
    let installed_root_count: usize = view.corp.servers.iter().map(|server| server.root.len()).sum();
    let remote_server_count = view.corp.servers.iter().filter(|server| matches!(server.server, ServerId::Remote(_))).count();

    let mut features = Vec::with_capacity(OBS_SIZE);
    features.push(if view.side == Side::Corp { 1.0 } else { 0.0 });
    features.extend(phase_one_hot(view.phase));
    features.push(norm(self_credits as f32, MAX_CREDITS));
    features.push(norm(self_clicks as f32, MAX_CLICKS));
    features.push(norm(self_agenda_points as f32, MAX_AGENDA_POINTS));
    features.push(norm(opp_credits as f32, MAX_CREDITS));
    features.push(norm(opp_clicks as f32, MAX_CLICKS));
    features.push(norm(opp_agenda_points as f32, MAX_AGENDA_POINTS));
    features.push(norm(view.corp.bad_publicity as f32, MAX_BAD_PUBLICITY));
    features.push(norm(view.runner.tags as f32, MAX_TAGS));
    features.push(norm(view.runner.brain_damage as f32, MAX_BRAIN_DAMAGE));
    features.push(norm(view.runner.memory_units as f32, MAX_MEMORY_UNITS));
    features.push(norm(view.runner.link_strength as f32, MAX_LINK_STRENGTH));
    features.push(norm(view.corp.hq_count as f32, MAX_HAND_SIZE));
    features.push(norm(view.corp.rd_count as f32, MAX_DECK_SIZE));
    features.push(norm(view.corp.archives.len() as f32, MAX_DECK_SIZE));
    features.push(norm(view.runner.grip_count as f32, MAX_HAND_SIZE));
    features.push(norm(view.runner.stack_count as f32, MAX_DECK_SIZE));
    features.push(norm(view.runner.heap.len() as f32, MAX_DECK_SIZE));
    features.push(norm(view.runner.rig.len() as f32, MAX_INSTALLED_PER_SIDE));
    features.push(norm(installed_ice_count as f32, MAX_INSTALLED_PER_SIDE));
    features.push(norm(installed_root_count as f32, MAX_INSTALLED_PER_SIDE));
    features.push(norm(remote_server_count as f32, MAX_REMOTE_SERVERS));
    match &view.active_run {
        Some(run) => {
            features.push(1.0);
            features.push(norm(run.position as f32, MAX_RUN_ICE));
        }
        None => {
            features.push(0.0);
            features.push(0.0);
        }
    }
    features.push(norm(view.legal_actions.len() as f32, ActionSpace::SIZE as f32));

    debug_assert_eq!(features.len(), OBS_SIZE);
    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::cards::CardRegistry;
    use netrunner_core::rules::GameState;

    #[test]
    fn observation_length_matches_obs_size_for_both_sides() {
        let registry = CardRegistry::new();
        let state = GameState::new(0);

        assert_eq!(encode_observation(&state, &registry, Side::Corp).len(), OBS_SIZE);
        assert_eq!(encode_observation(&state, &registry, Side::Runner).len(), OBS_SIZE);
    }

    #[test]
    fn all_features_are_finite_and_within_a_sane_range() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.corp.resources.credits = netrunner_core::rules::Credits(999);
        state.runner.tags = 999;

        for side in [Side::Corp, Side::Runner] {
            for value in encode_observation(&state, &registry, side) {
                assert!(value.is_finite());
                assert!((-1.0..=1.0).contains(&value), "feature {value} out of expected range");
            }
        }
    }

    #[test]
    fn side_indicator_is_the_first_feature_and_differs_by_viewer() {
        let registry = CardRegistry::new();
        let state = GameState::new(0);

        let corp_obs = encode_observation(&state, &registry, Side::Corp);
        let runner_obs = encode_observation(&state, &registry, Side::Runner);
        assert_eq!(corp_obs[0], 1.0);
        assert_eq!(runner_obs[0], 0.0);
    }
}
