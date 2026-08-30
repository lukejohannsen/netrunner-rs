use crate::dsl::DamageType;
use crate::rules::event::GameEvent;
use crate::rules::state::{GamePhase, GameState, Side};

/// Applies `amount` points of `damage_type` damage to the Runner.
///
/// If `amount` exceeds the Runner's current grip size, the Runner cannot
/// discard enough cards to pay it off and flatlines: this is a momentary
/// event exactly like Corp deck-out (see `turn::enter_start_of_turn`'s doc
/// comment), not a standing predicate re-derivable from `GameState` alone —
/// so it's handled inline here rather than via `win::check_win_conditions`.
/// The remaining grip is discarded to the heap, `phase` transitions
/// straight to `GamePhase::GameOver(Side::Corp)`, and only `RunnerFlatlined`
/// and `GameOver` are emitted (no `DamageTaken`/`CardDiscarded` — the Runner
/// never actually "takes" damage they didn't survive).
///
/// Otherwise, `amount` cards are discarded from the grip at pseudo-random
/// indices via `GameState::next_u64` (mirroring `run::access_server`'s HQ
/// access roll — see its doc comment), one roll per card against the
/// shrinking grip. Brain damage additionally permanently increments
/// `runner.brain_damage` (see `turn::max_hand_size`).
///
/// Never fails: mirrors `run::access_server`'s "never fails" convention —
/// there's no illegal `(state, damage_type, amount)` combination to reject.
pub fn apply_damage(state: &mut GameState, damage_type: DamageType, amount: usize) -> Vec<GameEvent> {
    if amount > state.runner.grip.len() {
        let discarded: Vec<_> = std::mem::take(&mut state.runner.grip);
        state.last_discarded_cards = discarded.clone();
        for card in discarded {
            state.runner.heap.push(card);
        }
        state.phase = GamePhase::GameOver(Side::Corp);
        return vec![GameEvent::RunnerFlatlined, GameEvent::GameOver { winner: Side::Corp }];
    }

    if damage_type == DamageType::Brain {
        state.runner.brain_damage += amount;
    }

    let mut events = vec![GameEvent::DamageTaken { damage_type, amount }];
    state.last_discarded_cards.clear();
    for _ in 0..amount {
        let roll = state.next_u64();
        let index = (roll as usize) % state.runner.grip.len();
        let card = state.runner.grip.remove(index);
        state.runner.heap.push(card.clone());
        state.last_discarded_cards.push(card.clone());
        events.push(GameEvent::CardDiscarded { side: Side::Runner, card });
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardRegistry;
    use crate::dsl::CardId;
    use crate::rules::error::RulesError;
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, MemoryUnits, PlayerResources, RunnerState,
    };
    use std::collections::HashSet;

    /// See `turn::tests::close_all_windows`'s doc comment — same helper,
    /// duplicated here since that one lives in a private `mod tests`.
    fn close_all_windows(mut state: GameState, registry: &CardRegistry) -> (GameState, Vec<GameEvent>) {
        let mut events = Vec::new();
        while let Some(window) = &state.paid_ability_window {
            let side = window.active_priority;
            let (next, ev) = crate::rules::apply_action(&state, registry, crate::rules::PlayerAction::PassPriority { side })
                .expect("pass priority should succeed");
            state = next;
            events.extend(ev);
        }
        (state, events)
    }

    fn game_state(grip: Vec<CardId>, brain_damage: usize, seed: u64) -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                brain_damage,
                grip,
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            pending_prevention: None, pending_paid_choice: None, pending_decision: None, last_discarded_cards: Vec::new(), last_completed_run: None, last_advancement_was_first: false,
            seed,
            rng_step: 0,
        }
    }

    fn grip_of(n: usize) -> Vec<CardId> {
        (0..n).map(|i| CardId(format!("card_{i}"))).collect()
    }

    #[test]
    fn non_fatal_net_damage_discards_exact_amount_randomly_and_leaves_game_active() {
        let mut state = game_state(grip_of(5), 0, 42);
        let original_grip: HashSet<CardId> = state.runner.grip.iter().cloned().collect();

        let events = apply_damage(&mut state, DamageType::Net, 2);

        assert_eq!(state.runner.grip.len(), 3);
        assert_eq!(state.runner.heap.len(), 2);
        assert_eq!(state.runner.brain_damage, 0);
        assert_eq!(state.phase, GamePhase::Action(Side::Runner));

        // Discarded cards and remaining grip cards partition the original grip.
        let remaining: HashSet<CardId> = state.runner.grip.iter().cloned().collect();
        let discarded: HashSet<CardId> = state.runner.heap.iter().cloned().collect();
        assert!(remaining.is_disjoint(&discarded));
        assert_eq!(&remaining | &discarded, original_grip);

        assert_eq!(events[0], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2 });
        assert_eq!(events.len(), 3);
        for (event, card) in events[1..].iter().zip(state.runner.heap.iter()) {
            assert_eq!(event, &GameEvent::CardDiscarded { side: Side::Runner, card: card.clone() });
        }
    }

    #[test]
    fn brain_damage_permanently_reduces_runners_max_hand_size() {
        let mut state = game_state(grip_of(5), 0, 7);

        apply_damage(&mut state, DamageType::Brain, 2);
        assert_eq!(state.runner.brain_damage, 2);
        assert_eq!(state.runner.grip.len(), 3);

        // Simulate the Runner redrawing back up to 4 cards over subsequent
        // turns — under the unmodified limit (5) but over the brain-damage-
        // adjusted limit (5 - 2 = 3).
        state.runner.grip = grip_of(4);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];

        let registry = CardRegistry::new();
        let (next, _events) =
            crate::rules::turn::end_turn(&state, &registry).expect("ending turn should succeed");
        let (next, _close_events) = close_all_windows(next, &registry);

        assert_eq!(next.phase, GamePhase::Discard { side: Side::Runner, required: 1 });
    }

    #[test]
    fn damage_exceeding_grip_flatlines_immediately_and_rejects_subsequent_actions() {
        let mut state = game_state(grip_of(2), 0, 3);
        let original_grip = state.runner.grip.clone();

        let events = apply_damage(&mut state, DamageType::Meat, 5);

        assert!(state.runner.grip.is_empty());
        assert_eq!(state.runner.heap, original_grip);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![GameEvent::RunnerFlatlined, GameEvent::GameOver { winner: Side::Corp }]
        );

        let action = crate::rules::PlayerAction::GainCreditClick { side: Side::Runner };
        assert!(matches!(
            crate::rules::apply_action(&state, &crate::cards::CardRegistry::new(), action),
            Err(RulesError::WrongPhase { .. })
        ));
    }

    #[test]
    fn flatlining_from_brain_damage_does_not_increment_brain_damage_counter() {
        let mut state = game_state(grip_of(1), 0, 9);

        apply_damage(&mut state, DamageType::Brain, 5);

        assert_eq!(state.runner.brain_damage, 0);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
    }
}
