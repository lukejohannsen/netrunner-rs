use crate::dsl::{CardId, DamageType};
use crate::rules::event::GameEvent;
use crate::rules::state::{GameState, Side};

/// Applies `amount` points of `damage_type` damage to the Runner.
///
/// If `amount` exceeds the Runner's current grip size, the Runner cannot
/// discard enough cards to pay it off and flatlines: this is a momentary
/// event exactly like Corp deck-out (see `turn::enter_start_of_turn`'s doc
/// comment), not a standing predicate re-derivable from `GameState` alone —
/// so it's handled inline here rather than via `win::check_win_conditions`.
/// The remaining grip is discarded to the heap, `phase` transitions
/// straight to `GamePhase::GameOver(Side::Corp)`, and only `RunnerFlatlined`
/// and `GameOver` are emitted (no `DamageTaken`/`CardTrashed` — the Runner
/// never actually "takes" damage they didn't survive).
///
/// Otherwise, `amount` cards are trashed from the grip at pseudo-random
/// indices via `GameState::next_u64` (mirroring `run::access_server`'s HQ
/// access roll — see its doc comment), one roll per card against the
/// shrinking grip. Brain damage additionally permanently increments
/// `runner.brain_damage` (see `turn::max_hand_size`).
///
/// Never fails: mirrors `run::access_server`'s "never fails" convention —
/// there's no illegal `(state, damage_type, amount)` combination to reject.
///
/// Returns the discarded cards alongside the events rather than recording
/// them on `GameState`. Their only consumer is
/// `EffectRequirement::LastDamageTrashedOddCostCard` (*Diviner*), which
/// reads them from the `ability::ResolutionContext` of the very `Sequence`
/// that dealt the damage — a strictly narrower lifetime than the old
/// `GameState::last_discarded_cards` field, which outlived the resolution
/// and could be read stale.
///
/// Returned explicitly rather than re-derived from the events: the
/// flatline path empties the grip without emitting a `CardTrashed` per
/// card, so the event stream alone would under-report it.
///
/// Each card is *trashed* by the player responsible (CR 10.4.2a: "the
/// player responsible for the damage trashes 1 randomly-chosen card from
/// the grip"), a `CardTrashed` out of the hand, and not discarded — a
/// trashed card is not a discarded one (CR 1.19.3). It was recorded as a
/// `CardDiscarded`, an occurrence of nothing, so no card could hear a card
/// that damage took: Strike Fund's "when this event is trashed from your
/// grip" is the first to. The caller dispatches them with the damage
/// (`ability::dispatch_damage_taken`, or a cost's payer).
///
/// `responsible` is who did the damage (CR 10.4.1), carried on the
/// `DamageTaken` it records — see that event.
///
/// **A card can say who chooses the cards** (`ContinuousKind::
/// ChoosesDamageTrash`: Titanium Ribs every point, Chronos Protocol the
/// first point of the turn's first net damage). Then the points are
/// selected one at a time from the grip as it stood when the damage began
/// and trashed together (CR 10.4.3a), each chosen point asked by the
/// payment's replay (`payment::Ask::Damage`, answered from
/// `GameState::payment_answers`) and each other point rolled among the
/// cards not yet selected. That is the only reason this returns a
/// `Result`: `RulesError::PaymentChoiceNeeded`, which unwinds the action.
/// Nobody is asked when every card left goes anyway. Damage no card says
/// anything about takes the old path unchanged, roll for roll.
pub fn apply_damage(
    state: &mut GameState,
    registry: &crate::cards::CardRegistry,
    damage_type: DamageType,
    amount: usize,
    responsible: Option<Side>,
) -> Result<(Vec<GameEvent>, Vec<CardId>), crate::rules::error::RulesError> {
    if amount > state.runner.grip.len() {
        let discarded: Vec<_> = std::mem::take(&mut state.runner.grip);
        for card in &discarded {
            state.runner.heap.push(card.clone());
        }
        let mut events = vec![GameEvent::RunnerFlatlined];
        events.extend(crate::rules::win::end_game(state, Side::Corp));
        return Ok((events, discarded));
    }

    let choosers = if amount == 0 || amount == state.runner.grip.len() {
        [None, None]
    } else {
        crate::rules::continuous::damage_choosers(state, registry, damage_type)
    };
    if choosers.iter().any(Option::is_some) {
        return chosen(state, damage_type, amount, responsible, &choosers);
    }

    if damage_type == DamageType::Brain {
        state.runner.brain_damage += amount;
    }

    let mut events = vec![GameEvent::DamageTaken { damage_type, amount, responsible }];
    let mut discarded = Vec::new();
    for _ in 0..amount {
        let roll = state.next_u64();
        let index = (roll as usize) % state.runner.grip.len();
        let card = state.runner.grip.remove(index);
        state.runner.heap.push(card.clone());
        discarded.push(card.clone());
        events.push(GameEvent::CardTrashed { side: Side::Runner, card, from: crate::dsl::TrashedFrom::Hand, by: responsible, install: None, damage: Some(damage_type) });
    }
    Ok((events, discarded))
}

/// `apply_damage` where a card says who chooses (`choosers`: the first
/// point's, then every later point's). The grip is not touched until every
/// point is selected, so a candidate's position is the one it had when the
/// damage began, whatever was selected before it.
fn chosen(
    state: &mut GameState,
    damage_type: DamageType,
    amount: usize,
    responsible: Option<Side>,
    choosers: &[Option<(Side, CardId)>; 2],
) -> Result<(Vec<GameEvent>, Vec<CardId>), crate::rules::error::RulesError> {
    use crate::rules::error::RulesError;
    use crate::rules::payment::{Ask, DamageCandidate, DamageQuestion};
    let mut left: Vec<usize> = (0..state.runner.grip.len()).collect();
    let mut picked: Vec<usize> = Vec::with_capacity(amount);
    for point in 0..amount {
        let to_go = amount - point;
        match &choosers[usize::from(point > 0)] {
            // Every card left goes: there is nothing to choose.
            Some(_) if to_go == left.len() => picked.append(&mut left),
            Some((side, card)) => {
                if state.payment_answers.is_empty() {
                    let eligible = left.iter().map(|&position| DamageCandidate { position: position as u32, card: state.runner.grip[position].clone() }).collect();
                    let question = DamageQuestion { card: card.clone(), kind: damage_type, eligible, remaining: to_go as u32 };
                    return Err(RulesError::PaymentChoiceNeeded { side: *side, amount: to_go as u32, question: Ask::Damage(Box::new(question)) });
                }
                let answer = state.payment_answers.remove(0) as usize;
                let at = left.iter().position(|&position| position == answer).ok_or(RulesError::CardNotEligibleForSelection(answer))?;
                picked.push(left.remove(at));
            }
            None => {
                let roll = state.next_u64();
                let at = (roll as usize) % left.len();
                picked.push(left.remove(at));
            }
        }
    }
    if damage_type == DamageType::Brain {
        state.runner.brain_damage += amount;
    }
    let mut events = vec![GameEvent::DamageTaken { damage_type, amount, responsible }];
    let discarded: Vec<CardId> = picked.iter().map(|&position| state.runner.grip[position].clone()).collect();
    let mut descending = picked.clone();
    descending.sort_unstable_by(|a, b| b.cmp(a));
    for position in descending {
        state.runner.grip.remove(position);
    }
    for card in &discarded {
        state.runner.heap.push(card.clone());
        events.push(GameEvent::CardTrashed { side: Side::Runner, card: card.clone(), from: crate::dsl::TrashedFrom::Hand, by: responsible, install: None, damage: Some(damage_type) });
    }
    Ok((events, discarded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardRegistry;
    use crate::dsl::CardId;
    use crate::rules::error::RulesError;
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, MemoryUnits, PlayerResources, RunnerState,
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
            seed,
            ..Default::default()
        }
    }

    fn grip_of(n: usize) -> Vec<CardId> {
        (0..n).map(|i| CardId(format!("card_{i}"))).collect()
    }

    #[test]
    fn non_fatal_net_damage_discards_exact_amount_randomly_and_leaves_game_active() {
        let mut state = game_state(grip_of(5), 0, 42);
        let original_grip: HashSet<CardId> = state.runner.grip.iter().cloned().collect();

        let (events, returned_discards) = apply_damage(&mut state, &CardRegistry::new(), DamageType::Net, 2, Some(Side::Corp)).expect("no card chooses");

        assert_eq!(state.runner.grip.len(), 3);
        assert_eq!(state.runner.heap.len(), 2);
        assert_eq!(state.runner.brain_damage, 0);
        assert_eq!(state.phase, GamePhase::Action(Side::Runner));

        // Discarded cards and remaining grip cards partition the original grip.
        let remaining: HashSet<CardId> = state.runner.grip.iter().cloned().collect();
        let discarded: HashSet<CardId> = state.runner.heap.iter().cloned().collect();
        assert!(remaining.is_disjoint(&discarded));
        assert_eq!(&remaining | &discarded, original_grip);

        // The returned list is what `ability::ResolutionContext` records for
        // `LastDamageTrashedOddCostCard`; it must be exactly what moved.
        assert_eq!(returned_discards.len(), 2);
        assert_eq!(returned_discards.iter().cloned().collect::<HashSet<_>>(), discarded);

        assert_eq!(events[0], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2, responsible: Some(Side::Corp) });
        assert_eq!(events.len(), 3);
        for (event, card) in events[1..].iter().zip(state.runner.heap.iter()) {
            assert_eq!(event, &GameEvent::CardTrashed { side: Side::Runner, card: card.clone(), from: crate::dsl::TrashedFrom::Hand, by: Some(Side::Corp), install: None, damage: Some(DamageType::Net) });
        }
    }

    #[test]
    fn brain_damage_permanently_reduces_runners_max_hand_size() {
        let mut state = game_state(grip_of(5), 0, 7);

        apply_damage(&mut state, &CardRegistry::new(), DamageType::Brain, 2, Some(Side::Corp)).expect("no card chooses");
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

        let (events, returned_discards) = apply_damage(&mut state, &CardRegistry::new(), DamageType::Meat, 5, Some(Side::Corp)).expect("no card chooses");

        assert!(state.runner.grip.is_empty());
        assert_eq!(state.runner.heap, original_grip);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![GameEvent::RunnerFlatlined, GameEvent::GameOver { winner: Side::Corp }]
        );
        // The flatline path emits no per-card `CardDiscarded`, so the
        // returned list is the only report of what was dumped — the reason
        // `apply_damage` returns it rather than leaving callers to derive
        // it from the events.
        assert_eq!(returned_discards, original_grip);

        let action = crate::rules::PlayerAction::GainCreditClick { side: Side::Runner };
        assert!(matches!(
            crate::rules::apply_action(&state, &crate::cards::CardRegistry::new(), action),
            Err(RulesError::GameIsOver { winner: Side::Corp })
        ));
    }

    #[test]
    fn flatlining_from_brain_damage_does_not_increment_brain_damage_counter() {
        let mut state = game_state(grip_of(1), 0, 9);

        apply_damage(&mut state, &CardRegistry::new(), DamageType::Brain, 5, Some(Side::Corp)).expect("no card chooses");

        assert_eq!(state.runner.brain_damage, 0);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
    }
}
