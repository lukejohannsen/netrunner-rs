use crate::cards::CardRegistry;
use crate::dsl::CardId;
use crate::rules::state::{GamePhase, GameState, Side};

/// Agenda points either side needs to win the game outright.
const WINNING_AGENDA_POINTS: u32 = 7;

/// Looks up `card_id`'s printed agenda point value from `registry`. Returns
/// `None` if the card isn't in the registry, or is registered but isn't an
/// Agenda (`dsl::Card::agenda_points` is `None`) — that `None` is the gate
/// `run::access_server` uses to decide whether an accessed card is even an
/// Agenda at all, not just a defaulted value, so it stays `Option<u32>`
/// rather than collapsing to a bare `u32`.
pub(crate) fn agenda_value(card_id: &CardId, registry: &CardRegistry) -> Option<u32> {
    registry.get(card_id).and_then(|card| card.agenda_points)
}

/// Sums the registry-defined agenda point value of every card in
/// `scored_agendas`. A card that's landed in that list by construction
/// should always resolve to `Some`, but `filter_map` treats a hypothetical
/// unregistered/non-Agenda entry as a 0-point contribution rather than
/// panicking.
fn total_agenda_points(scored_agendas: &[CardId], registry: &CardRegistry) -> u32 {
    scored_agendas.iter().filter_map(|id| agenda_value(id, registry)).sum()
}

/// Checks whether either side's score area has reached the winning
/// agenda-point threshold and, if so, transitions `state.phase` to
/// `GamePhase::GameOver(winner)`. Safe to call repeatedly/idempotently from
/// anywhere `scored_agendas` might change (currently just
/// `run::access_server` after a steal) — score areas only ever grow, so
/// re-deriving this from `GameState`+`registry` alone is always correct,
/// unlike deck-out below.
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
pub fn check_win_conditions(state: &mut GameState, registry: &CardRegistry) {
    if matches!(state.phase, GamePhase::GameOver(_)) {
        return;
    }
    if total_agenda_points(&state.corp.scored_agendas, registry) >= WINNING_AGENDA_POINTS {
        state.phase = GamePhase::GameOver(Side::Corp);
    } else if total_agenda_points(&state.runner.scored_agendas, registry) >= WINNING_AGENDA_POINTS {
        state.phase = GamePhase::GameOver(Side::Runner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{Card, CardType};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, MemoryUnits, PlayerResources, RunnerState,
    };

    /// A minimal Agenda `Card` worth `points` — everything besides id, side,
    /// and `agenda_points` is irrelevant to these tests.
    fn agenda_card(id: &str, side: Side, points: u32) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type: CardType::Agenda,
            cost: 0,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: Some(points),
            agenda_points: Some(points),
            min_deck_size: None,
        }
    }

    fn game_state(corp_scored: Vec<CardId>, runner_scored: Vec<CardId>) -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
                scored_agendas: corp_scored,
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
                scored_agendas: runner_scored,
            },
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            seed: 0,
            rng_step: 0,
        }
    }

    #[test]
    fn corp_reaching_seven_agenda_points_wins() {
        let registry = CardRegistry::from_cards(vec![agenda_card("hostile_takeover", Side::Corp, 7)]);
        let mut state = game_state(vec![CardId("hostile_takeover".to_string())], Vec::new());
        check_win_conditions(&mut state, &registry);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
    }

    #[test]
    fn runner_reaching_seven_agenda_points_wins() {
        let registry =
            CardRegistry::from_cards(vec![agenda_card("priority_requisition", Side::Corp, 7)]);
        let mut state = game_state(Vec::new(), vec![CardId("priority_requisition".to_string())]);
        check_win_conditions(&mut state, &registry);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
    }

    #[test]
    fn below_threshold_does_not_end_the_game() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("hostile_takeover", Side::Corp, 6),
            agenda_card("priority_requisition", Side::Corp, 6),
        ]);
        let mut state = game_state(
            vec![CardId("hostile_takeover".to_string())],
            vec![CardId("priority_requisition".to_string())],
        );
        check_win_conditions(&mut state, &registry);
        assert_eq!(state.phase, GamePhase::Action(Side::Corp));
    }

    #[test]
    fn already_concluded_game_is_not_reevaluated() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("hostile_takeover", Side::Corp, 7),
            agenda_card("priority_requisition", Side::Corp, 7),
        ]);
        let mut state = game_state(
            vec![CardId("hostile_takeover".to_string())],
            vec![CardId("priority_requisition".to_string())],
        );
        state.phase = GamePhase::GameOver(Side::Runner);
        check_win_conditions(&mut state, &registry);
        // Corp is checked first and would otherwise win — confirms the
        // early-return guard actually short-circuits rather than merely
        // agreeing by coincidence.
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
    }

    #[test]
    fn corp_wins_by_summing_several_distinct_agenda_values() {
        // 3 + 3 + 1 == 7: the win only triggers once the running total
        // actually crosses the threshold, not on any single scored agenda.
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", Side::Corp, 3),
            agenda_card("government_takeover", Side::Corp, 3),
            agenda_card("hostile_takeover", Side::Corp, 1),
        ]);
        let mut state = game_state(
            vec![
                CardId("priority_requisition".to_string()),
                CardId("government_takeover".to_string()),
            ],
            Vec::new(),
        );

        // Only 6 points scored so far — not a win yet.
        check_win_conditions(&mut state, &registry);
        assert_eq!(state.phase, GamePhase::Action(Side::Corp));

        // Scoring the third agenda pushes the total to 7.
        state.corp.scored_agendas.push(CardId("hostile_takeover".to_string()));
        check_win_conditions(&mut state, &registry);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
    }

    #[test]
    fn agenda_value_looks_up_registered_agenda_points() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", Side::Corp, 3),
            Card {
                id: CardId("hedge_fund".to_string()),
                title: "Hedge Fund".to_string(),
                side: Side::Corp,
                card_type: CardType::Operation,
                cost: 5,
                triggers: Vec::new(),
                abilities: Vec::new(),
                trash_cost: None,
                steal_cost: None,
                advancement_requirement: None,
                agenda_points: None,
                min_deck_size: None,
            },
        ]);

        assert_eq!(
            agenda_value(&CardId("priority_requisition".to_string()), &registry),
            Some(3)
        );
        // Registered, but not an Agenda.
        assert_eq!(agenda_value(&CardId("hedge_fund".to_string()), &registry), None);
        // Not registered at all.
        assert_eq!(agenda_value(&CardId("unregistered".to_string()), &registry), None);
    }
}
