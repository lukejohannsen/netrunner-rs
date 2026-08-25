use netrunner_core::rules::{GamePhase, GameState, Side};

const WIN_SCORE: f64 = 1000.0;
const AGENDA_POINT_WEIGHT: f64 = 20.0;
const OWN_CREDIT_WEIGHT: f64 = 0.4;
const OPPONENT_CREDIT_WEIGHT: f64 = 0.2;
const BAD_PUBLICITY_WEIGHT: f64 = 3.0;
const TAG_WEIGHT: f64 = 4.0;
const BOARD_PRESENCE_WEIGHT: f64 = 1.0;
const MEMORY_WEIGHT: f64 = 0.5;

/// A rough static evaluation of `state` from `side`'s perspective: positive
/// favors `side`, negative favors the opponent. Shared by `HeuristicAgent`'s
/// one-ply scoring and `MctsAgent`'s rollout/leaf evaluation.
///
/// Reads `PlayerResources::agenda_points` directly rather than re-deriving
/// it from `scored_agendas`/`CardRegistry`: `engine::score_agenda` and
/// `run::access::resolve_steal` already keep that field in sync on every
/// point-scoring action, and `netrunner_core::rules::win::agenda_value`
/// (which does the re-derivation) is `pub(crate)` to `netrunner_core`
/// besides — this needs no `CardRegistry` at all.
pub fn evaluate_state(state: &GameState, side: Side) -> f64 {
    if let GamePhase::GameOver(winner) = state.phase {
        return if winner == side { WIN_SCORE } else { -WIN_SCORE };
    }

    let own = state.resources(side);
    let opponent = state.resources(side.other());
    let mut score = (own.agenda_points.0 as f64 - opponent.agenda_points.0 as f64) * AGENDA_POINT_WEIGHT;
    score += own.credits.0 as f64 * OWN_CREDIT_WEIGHT;
    score -= opponent.credits.0 as f64 * OPPONENT_CREDIT_WEIGHT;

    match side {
        Side::Corp => {
            score -= state.corp.bad_publicity as f64 * BAD_PUBLICITY_WEIGHT;
            score += state.corp.installed.iter().filter(|c| c.rezzed).count() as f64 * BOARD_PRESENCE_WEIGHT;
        }
        Side::Runner => {
            score -= state.runner.tags as f64 * TAG_WEIGHT;
            score += state.runner.rig.len() as f64 * BOARD_PRESENCE_WEIGHT;
            score += state.runner.memory_units.0 as f64 * MEMORY_WEIGHT;
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{AgendaPoints, GameState};

    #[test]
    fn game_over_returns_win_or_loss_constant_regardless_of_other_fields() {
        let mut state = GameState::new(0);
        state.phase = GamePhase::GameOver(Side::Corp);
        state.runner.tags = 10;
        state.corp.bad_publicity = 10;

        assert_eq!(evaluate_state(&state, Side::Corp), WIN_SCORE);
        assert_eq!(evaluate_state(&state, Side::Runner), -WIN_SCORE);
    }

    #[test]
    fn agenda_point_lead_favors_the_leading_side() {
        let mut state = GameState::new(0);
        state.corp.resources.agenda_points = AgendaPoints(4);

        assert!(evaluate_state(&state, Side::Corp) > 0.0);
        assert!(evaluate_state(&state, Side::Runner) < 0.0);
    }

    #[test]
    fn corp_bad_publicity_lowers_the_corp_score() {
        let clean = GameState::new(0);
        let mut dirty = GameState::new(0);
        dirty.corp.bad_publicity = 3;

        assert!(evaluate_state(&clean, Side::Corp) > evaluate_state(&dirty, Side::Corp));
    }

    #[test]
    fn runner_tags_lower_the_runner_score() {
        let clean = GameState::new(0);
        let mut tagged = GameState::new(0);
        tagged.runner.tags = 2;

        assert!(evaluate_state(&clean, Side::Runner) > evaluate_state(&tagged, Side::Runner));
    }
}
