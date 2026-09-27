use crate::cards::CardRegistry;
use crate::dsl::CardId;
use crate::rules::event::GameEvent;
use crate::rules::state::{GamePhase, GameState, ScoredAgenda, Side};

/// The single transition into `GamePhase::GameOver` — every win, by any
/// route (agenda points, flatline, deck-out), goes through here.
///
/// Sets the phase **and clears everything that was mid-resolution**: the
/// run (through `run::end_run`, so `last_completed_run` bookkeeping is
/// the same as for a run that ended in access), the paid-ability window,
/// the trace, the prevention, the paid choice, the decision and the
/// deferred-trigger queue. Before this existed, `phase` was assigned at
/// four sites and none of them cleared any of that. `current_actor` puts a
/// parked window ahead of the phase, so after *Clearinghouse* flatlined the
/// Runner from its start-of-turn choice the start-of-turn window was still
/// open, `PassPriority` was legal, and two passes later `close_window`
/// wrote `phase = Action(Corp)` — **the Corp's win was silently reverted**
/// (ROADMAP Rules Audit §4). The same leftovers let post-game effects and
/// triggers keep firing, and an `Err` from one of those rejected the very
/// action that had ended the game.
///
/// Idempotent, and the `GameOver` event is emitted here and only here, so
/// two paths ending the game in one action (a steal whose identity
/// reaction flatlines, then the access machinery noticing) cannot emit it
/// twice. Returns the event on the transition, nothing if already over.
pub(crate) fn end_game(state: &mut GameState, winner: Side) -> Vec<GameEvent> {
    if state.is_over() {
        return Vec::new();
    }
    state.phase = GamePhase::GameOver(winner);
    crate::rules::run::end_run(state);
    state.paid_ability_window = None;
    state.active_trace = None;
    state.pending_prevention = None;
    state.pending_paid_choice = None;
    state.pending_decision = None;
    state.deferred_triggers.clear();
    vec![GameEvent::GameOver { winner }]
}

/// What `card_id` is worth in `side`'s score area: what it prints and what
/// its own text changes there (`continuous::agenda_points_in`, Let Them
/// Dream's "while this agenda is in the Runner's score area, it is worth 1
/// less agenda point"). 0 for a card that is not a registered agenda. The
/// one number for a scored or stolen agenda — the win check, the stored
/// tally, a forfeit and a card that counts points all read it.
pub fn agenda_value_in(state: &GameState, registry: &CardRegistry, card_id: &CardId, side: Side) -> u32 {
    registry.get(card_id).map_or(0, |card| crate::rules::continuous::agenda_points_in(state, registry, card, side))
}

/// What one entry in the Corp's score area is worth: an agenda's
/// [`agenda_value_in`], or, for a card added "as an agenda", the points
/// that addition gave it and nothing it prints (CR 10.1.3) — which may be
/// negative (Word on the Street).
pub fn scored_value(state: &GameState, registry: &CardRegistry, scored: &ScoredAgenda) -> i32 {
    match scored.as_agenda {
        Some(as_agenda) => as_agenda.points,
        None => agenda_value_in(state, registry, &scored.card, Side::Corp) as i32,
    }
}

/// `side`'s score (CR 1.17.1): the sum of what every card in its score
/// area is worth, signed, since a card added as an agenda can be worth
/// less than nothing. An unregistered or non-agenda entry counts 0 rather
/// than panicking. The one sum the win check, the Amount that reads a
/// score and a client's number all agree on.
pub fn score(state: &GameState, registry: &CardRegistry, side: Side) -> i32 {
    match side {
        Side::Corp => state.corp.scored_agendas.iter().map(|scored| scored_value(state, registry, scored)).sum(),
        Side::Runner => state.runner.scored_agendas.iter().map(|card| agenda_value_in(state, registry, card, Side::Runner) as i32).sum(),
    }
}

/// Checks whether either side's score area has reached the winning
/// agenda-point threshold and, if so, transitions `state.phase` to
/// `GamePhase::GameOver(winner)`. Safe to call repeatedly/idempotently from
/// anywhere — its one caller is `rules::checkpoint`, which runs it before
/// an event's triggers and at the end of every action — score areas being
/// public state, so
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
///
/// Returns the `GameOver` event when this call ended the game (via
/// [`end_game`]), so callers append it like any other event.
pub fn check_win_conditions(state: &mut GameState, registry: &CardRegistry) -> Vec<GameEvent> {
    if state.is_over() {
        return Vec::new();
    }
    // The threshold is a match rule (7 in Standard, 6 in the starter game),
    // read off the state rather than a const — see `MatchRules`.
    let winning = state.rules.winning_agenda_points;
    let winning = i32::try_from(winning).unwrap_or(i32::MAX);
    if score(state, registry, Side::Corp) >= winning {
        end_game(state, Side::Corp)
    } else if score(state, registry, Side::Runner) >= winning {
        end_game(state, Side::Runner)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardDefinition, CardType};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, MemoryUnits, PlayerResources, RunnerState, ScoredAgenda,
    };

    /// A minimal Agenda `CardDefinition` worth `points` — everything besides id, side,
    /// and `agenda_points` is irrelevant to these tests.
    fn agenda_card(id: &str, side: Side, points: u32) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type: CardType::Agenda,
            advancement_requirement: Some(points),
            agenda_points: Some(points),
            is_playable: true,
            ..Default::default()
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
                scored_agendas: corp_scored.into_iter().map(ScoredAgenda::plain).collect(),
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                scored_agendas: runner_scored,
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Corp),
            ..Default::default()
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
        state.corp.scored_agendas.push(ScoredAgenda::plain(CardId("hostile_takeover".to_string())));
        check_win_conditions(&mut state, &registry);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
    }

    #[test]
    fn agenda_value_in_looks_up_registered_agenda_points() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", Side::Corp, 3),
            CardDefinition {
                id: CardId("hedge_fund".to_string()),
                title: "Hedge Fund".to_string(),
                side: Side::Corp,
                card_type: CardType::Operation,
                cost: 5,
                is_playable: true,
                ..Default::default()
            },
        ]);

        let state = GameState::default();
        let value = |card: &str| agenda_value_in(&state, &registry, &CardId(card.to_string()), Side::Corp);
        assert_eq!(value("priority_requisition"), 3);
        // Registered, but not an Agenda.
        assert_eq!(value("hedge_fund"), 0);
        // Not registered at all.
        assert_eq!(value("unregistered"), 0);
    }

    /// The threshold is a match rule: six points win a starter game and
    /// leave a Standard one running.
    #[test]
    fn the_winning_threshold_comes_from_the_match_rules() {
        let mut registry = CardRegistry::new();
        registry.insert(CardDefinition {
            id: CardId("two_pointer".to_string()),
            title: "two_pointer".to_string(),
            side: Side::Corp,
            card_type: CardType::Agenda,
            agenda_points: Some(2),
            is_playable: true,
            ..Default::default()
        });
        let mut state = game_state(vec![CardId("two_pointer".to_string()); 3], Vec::new());
        assert!(check_win_conditions(&mut state, &registry).is_empty(), "six points do not win at Standard's 7");
        state.rules = crate::rules::MatchRules { winning_agenda_points: 6 };
        assert!(!check_win_conditions(&mut state, &registry).is_empty(), "six points win a starter game");
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
    }
}
