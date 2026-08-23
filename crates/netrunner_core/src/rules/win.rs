use crate::dsl::CardId;
use crate::rules::state::{GamePhase, GameState, Side};

/// Agenda points either side needs to win the game outright.
const WINNING_AGENDA_POINTS: u32 = 7;

/// Placeholder agenda-point lookup standing in for a real `CardRegistry`
/// (see `run::access_server`'s doc comment for why one doesn't exist yet —
/// the engine has no way to look up a `CardId`'s `dsl::CardType`/value from
/// data). A couple of fixture IDs are hardcoded here purely so
/// `access_server` and its tests have something to steal; swap this for a
/// real registry lookup with no change needed to `check_win_conditions`'s
/// own signature.
pub(crate) fn agenda_value(card: &CardId) -> Option<u32> {
    match card.0.as_str() {
        "priority_requisition" => Some(3),
        "hostile_takeover" => Some(1),
        _ => None,
    }
}

/// Checks whether either side has reached the winning agenda-point
/// threshold and, if so, transitions `state.phase` to
/// `GamePhase::GameOver(winner)`. Safe to call repeatedly/idempotently from
/// anywhere agenda points might change (currently just `run::access_server`
/// after a steal) — agenda points only ever increase, so re-deriving this
/// from `GameState` alone is always correct, unlike deck-out below.
///
/// Deliberately does NOT check deck-out (the Corp being unable to make
/// their mandatory draw) even though it's also a win condition — deck-out
/// is a momentary *event* (a draw attempt that just failed), not a standing
/// condition safely re-derivable from `GameState` alone: an empty R&D
/// doesn't by itself mean the Corp has lost (they may simply have drawn
/// their last card last turn and play continued normally). Checking "R&D is
/// empty" as a general predicate here — reachable from `access_server` too
/// — would end the game a turn early. Deck-out is handled inline at the one
/// place that actually attempts the draw: `turn::enter_start_of_turn`.
pub fn check_win_conditions(state: &mut GameState) {
    if matches!(state.phase, GamePhase::GameOver(_)) {
        return;
    }
    if state.corp.resources.agenda_points.0 >= WINNING_AGENDA_POINTS {
        state.phase = GamePhase::GameOver(Side::Corp);
    } else if state.runner.resources.agenda_points.0 >= WINNING_AGENDA_POINTS {
        state.phase = GamePhase::GameOver(Side::Runner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, MemoryUnits, PlayerResources, RunnerState,
    };

    fn game_state(corp_agenda_points: u32, runner_agenda_points: u32) -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(corp_agenda_points),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(runner_agenda_points),
                },
                memory_units: MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
            },
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            seed: 0,
            rng_step: 0,
        }
    }

    #[test]
    fn corp_reaching_seven_agenda_points_wins() {
        let mut state = game_state(7, 0);
        check_win_conditions(&mut state);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
    }

    #[test]
    fn runner_reaching_seven_agenda_points_wins() {
        let mut state = game_state(0, 7);
        check_win_conditions(&mut state);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
    }

    #[test]
    fn below_threshold_does_not_end_the_game() {
        let mut state = game_state(6, 6);
        check_win_conditions(&mut state);
        assert_eq!(state.phase, GamePhase::Action(Side::Corp));
    }

    #[test]
    fn already_concluded_game_is_not_reevaluated() {
        let mut state = game_state(7, 7);
        state.phase = GamePhase::GameOver(Side::Runner);
        check_win_conditions(&mut state);
        // Corp is checked first and would otherwise win — confirms the
        // early-return guard actually short-circuits rather than merely
        // agreeing by coincidence.
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
    }

    #[test]
    fn agenda_value_recognizes_fixture_agendas_and_nothing_else() {
        assert_eq!(agenda_value(&CardId("priority_requisition".to_string())), Some(3));
        assert_eq!(agenda_value(&CardId("hostile_takeover".to_string())), Some(1));
        assert_eq!(agenda_value(&CardId("hedge_fund".to_string())), None);
    }
}
