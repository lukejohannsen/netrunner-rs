use crate::cards::CardRegistry;
use crate::dsl::{CardId, Effect};
use crate::rules::ability;
use crate::rules::deck::{validate_deck, Deck};
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::state::{Credits, GamePhase, GameState, MemoryUnits, Side};
use crate::rules::turn;

const STARTING_CREDITS: u32 = 5;
const OPENING_HAND_SIZE: u32 = 5;
use crate::rules::memory::RUNNER_BASE_MEMORY_UNITS;

impl GameState {
    /// Validates both decks, builds a fresh, deterministically-seeded
    /// `GameState`, records each side's identity, gives each side
    /// `STARTING_CREDITS`, shuffles both draw decks, deals opening
    /// `OPENING_HAND_SIZE`-card hands, and enters
    /// `GamePhase::Mulligan(Side::Corp)`.
    ///
    /// Takes `seed: u64` rather than an external RNG — there is no `rand`
    /// dependency anywhere in this workspace, and this crate's sole,
    /// deliberately-deterministic RNG is `GameState::next_u64`, keyed on
    /// `(seed, rng_step)`. Matches `GameState::new(seed: u64)`'s existing
    /// constructor shape.
    pub fn setup(
        corp_deck: &Deck,
        runner_deck: &Deck,
        registry: &CardRegistry,
        seed: u64,
    ) -> Result<(GameState, Vec<GameEvent>), RulesError> {
        validate_deck(corp_deck, Side::Corp, registry)?;
        validate_deck(runner_deck, Side::Runner, registry)?;

        let mut state = GameState::new(seed);
        let mut events = Vec::new();

        state.corp.identity = Some(corp_deck.identity.clone());
        state.runner.identity = Some(runner_deck.identity.clone());

        state.corp.resources.credits = Credits(STARTING_CREDITS);
        state.runner.resources.credits = Credits(STARTING_CREDITS);
        // Seeded directly rather than through `memory::refresh` only
        // because the rig is provably empty here; every later change to it
        // goes through the refresh in `engine::apply_action`.
        state.runner.memory_units = MemoryUnits(RUNNER_BASE_MEMORY_UNITS);

        state.corp.r_and_d = expand_deck(corp_deck);
        state.runner.stack = expand_deck(runner_deck);

        shuffle_deck(&mut state, Side::Corp);
        shuffle_deck(&mut state, Side::Runner);

        events.extend(ability::evaluate_effect(
            &mut state,
            &Effect::DrawCards(Side::Corp, OPENING_HAND_SIZE),
            &mut ability::ResolutionContext::default(),
            registry,
        )?);
        events.extend(ability::evaluate_effect(
            &mut state,
            &Effect::DrawCards(Side::Runner, OPENING_HAND_SIZE),
            &mut ability::ResolutionContext::default(),
            registry,
        )?);

        let recurring_credits_max =
            registry.get(&corp_deck.identity).and_then(|c| c.recurring_credits).unwrap_or(0);
        state.corp.recurring_credits_max = recurring_credits_max;
        state.corp.recurring_credits = recurring_credits_max;

        // Identity-level max-hand-size bonus (e.g. Haas-Bioroid: Precision
        // Design's "+1 maximum hand size"), read once here — same pattern
        // as `recurring_credits_max` above — for whichever side's identity
        // declares one. `0` (the common case) for an identity with no such
        // trait.
        state.corp.max_hand_size_bonus =
            registry.get(&corp_deck.identity).and_then(|c| c.max_hand_size_bonus).unwrap_or(0);
        state.runner.max_hand_size_bonus =
            registry.get(&runner_deck.identity).and_then(|c| c.max_hand_size_bonus).unwrap_or(0);

        state.phase = GamePhase::Mulligan(Side::Corp);

        Ok((state, events))
    }
}

/// Flattens `deck.cards`' `(CardId, count)` pairs into a flat, unshuffled
/// `Vec<CardId>` with each id repeated `count` times. The identity itself
/// is never included here — it lives in `corp.identity`/`runner.identity`,
/// not the draw deck.
fn expand_deck(deck: &Deck) -> Vec<CardId> {
    let mut cards = Vec::new();
    for (card_id, count) in &deck.cards {
        for _ in 0..*count {
            cards.push(card_id.clone());
        }
    }
    cards
}

/// Fisher-Yates shuffle of `side`'s draw deck (Corp's `r_and_d` or
/// Runner's `stack`), using `state.next_u64()` per swap — this crate's
/// sole deterministic RNG source (see `GameState::next_u64`'s doc
/// comment), matching the `let roll = state.next_u64(); let index = (roll
/// as usize) % len;` idiom already used by `run::access::
/// compute_accessed_cards` and `damage::apply_damage`.
///
/// Deliberately never holds a `&mut Vec<CardId>` borrowed from a `state`
/// field across a `state.next_u64()` call — that needs `&mut state` as a
/// whole, which would conflict with a live sub-borrow of one of its
/// fields. Each iteration calls `next_u64()` first for a plain owned
/// `u64`, then separately re-borrows the specific field for the swap.
fn shuffle_deck(state: &mut GameState, side: Side) {
    let len = match side {
        Side::Corp => state.corp.r_and_d.len(),
        Side::Runner => state.runner.stack.len(),
    };
    for i in (1..len).rev() {
        let roll = state.next_u64();
        let j = (roll as usize) % (i + 1);
        match side {
            Side::Corp => state.corp.r_and_d.swap(i, j),
            Side::Runner => state.runner.stack.swap(i, j),
        }
    }
}

/// Extracts `side` from `state.phase` if it's currently `Mulligan(side)`.
fn require_mulligan_phase(state: &GameState) -> Result<Side, RulesError> {
    match state.phase {
        GamePhase::Mulligan(side) => Ok(side),
        actual => Err(RulesError::NotInMulliganPhase { actual }),
    }
}

/// Moves all of `side`'s hand cards back into their draw deck, leaving the
/// hand empty. Used by `take_mulligan` before reshuffling and redrawing.
fn return_hand_to_deck(state: &mut GameState, side: Side) {
    match side {
        Side::Corp => state.corp.r_and_d.append(&mut state.corp.hq),
        Side::Runner => state.runner.stack.append(&mut state.runner.grip),
    }
}

/// Resolves `PlayerAction::KeepHand`, per its doc comment. Legal only
/// during `GamePhase::Mulligan(side)` for whichever side is deciding.
pub(crate) fn keep_hand(
    state: &GameState,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = require_mulligan_phase(state)?;
    let mut next = state.clone();
    let mut events = vec![GameEvent::HandKept { side }];
    advance_past_mulligan(&mut next, &mut events, side, registry)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::TakeMulligan`, per its doc comment: returns the
/// current hand to the deck, reshuffles, redraws a fresh
/// `OPENING_HAND_SIZE`-card hand, then advances exactly like `keep_hand`.
pub(crate) fn take_mulligan(
    state: &GameState,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = require_mulligan_phase(state)?;
    let mut next = state.clone();
    let mut events = vec![GameEvent::MulliganTaken { side }];

    return_hand_to_deck(&mut next, side);
    shuffle_deck(&mut next, side);
    events.extend(ability::evaluate_effect(
        &mut next,
        &Effect::DrawCards(side, OPENING_HAND_SIZE),
        &mut ability::ResolutionContext::default(),
        registry,
    )?);

    advance_past_mulligan(&mut next, &mut events, side, registry)?;
    Ok((next, events))
}

/// Corp's decision advances to `Mulligan(Side::Runner)`; the Runner's
/// decision hands off into Corp's first turn (3 clicks + mandatory R&D
/// draw) via `turn::enter_start_of_turn`.
///
/// This reuse means the Corp's first turn performs the mandatory R&D draw
/// like every other, which is the real rule: under Null Signal Games'
/// rules the Corp draws on turn one. (An earlier comment here claimed the
/// opposite and called the draw a deliberate divergence — an invitation to
/// "fix" a correct implementation, caught by the Rules Audit.)
fn advance_past_mulligan(
    next: &mut GameState,
    events: &mut Vec<GameEvent>,
    side: Side,
    registry: &CardRegistry,
) -> Result<(), RulesError> {
    match side {
        Side::Corp => next.phase = GamePhase::Mulligan(Side::Runner),
        Side::Runner => turn::enter_start_of_turn(next, events, Side::Corp, registry)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardDefinition, CardType};
    use crate::rules::action::PlayerAction;
    use crate::rules::engine::apply_action;

    /// See `turn::tests::close_all_windows`'s doc comment — same helper,
    /// duplicated here since that one lives in a private `mod tests`.
    fn close_all_windows(mut state: GameState, registry: &CardRegistry) -> (GameState, Vec<GameEvent>) {
        let mut events = Vec::new();
        while let Some(window) = &state.paid_ability_window {
            let side = window.active_priority;
            let (next, ev) = apply_action(&state, registry, PlayerAction::PassPriority { side })
                .expect("pass priority should succeed");
            state = next;
            events.extend(ev);
        }
        (state, events)
    }

    fn identity(id: &str, side: Side, min_deck_size: u32) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type: CardType::Identity,
            min_deck_size: Some(min_deck_size),
            is_playable: true,
            ..Default::default()
        }
    }

    fn filler(id: &str, side: Side) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type: if side == Side::Corp { CardType::Asset } else { CardType::Event },
            is_playable: true,
            ..Default::default()
        }
    }

    /// Registers `total` filler cards for `side`, split across as many
    /// distinct `CardId`s as needed to respect the 3-copy limit, and
    /// returns the matching `Deck.cards` entries.
    fn filler_stack(registry: &mut CardRegistry, side: Side, prefix: &str, total: u32) -> Vec<(CardId, u32)> {
        let mut entries = Vec::new();
        let mut remaining = total;
        let mut i = 0;
        while remaining > 0 {
            let copies = remaining.min(3);
            let id = format!("{prefix}_{i}");
            registry.insert(filler(&id, side));
            entries.push((CardId(id), copies));
            remaining -= copies;
            i += 1;
        }
        entries
    }

    /// A registry plus a legal 40-card Corp deck (identity `min_deck_size:
    /// 40`, landing it in the flat 18-20 agenda-point band; 4 distinct
    /// 5-point Agendas = 20 points, at the top of that range) and a legal
    /// 45-card Runner deck.
    fn setup_fixtures() -> (CardRegistry, Deck, Deck) {
        let mut registry = CardRegistry::new();
        registry.insert(identity("corp_id", Side::Corp, 40));
        registry.insert(identity("runner_id", Side::Runner, 45));

        let mut corp_cards: Vec<(CardId, u32)> = (0..4)
            .map(|i| {
                let id = format!("corp_agenda_{i}");
                let mut agenda = filler(&id, Side::Corp);
                agenda.card_type = CardType::Agenda;
                agenda.agenda_points = Some(5);
                registry.insert(agenda);
                (CardId(id), 1)
            })
            .collect();
        corp_cards.extend(filler_stack(&mut registry, Side::Corp, "corp_filler", 36));

        let runner_cards = filler_stack(&mut registry, Side::Runner, "runner_filler", 45);

        let corp_deck = Deck { identity: CardId("corp_id".to_string()), cards: corp_cards };
        let runner_deck = Deck { identity: CardId("runner_id".to_string()), cards: runner_cards };

        (registry, corp_deck, runner_deck)
    }

    #[test]
    fn setup_gives_each_side_five_starting_credits() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(5));
        assert_eq!(state.runner.resources.credits, Credits(5));
    }

    #[test]
    fn setup_gives_runner_base_memory_units() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();

        assert_eq!(state.runner.memory_units, crate::rules::state::MemoryUnits(RUNNER_BASE_MEMORY_UNITS));
    }

    #[test]
    fn setup_records_each_sides_identity() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();

        assert_eq!(state.corp.identity, Some(CardId("corp_id".to_string())));
        assert_eq!(state.runner.identity, Some(CardId("runner_id".to_string())));
    }

    #[test]
    fn setup_deals_five_card_opening_hands_to_both_sides() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();

        assert_eq!(state.corp.hq.len(), 5);
        assert_eq!(state.runner.grip.len(), 5);
    }

    #[test]
    fn setup_removes_dealt_cards_from_the_draw_deck() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();

        assert_eq!(state.corp.r_and_d.len(), 40 - 5);
        assert_eq!(state.runner.stack.len(), 45 - 5);
    }

    #[test]
    fn setup_is_deterministic_for_a_fixed_seed_and_differs_across_seeds() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state_a, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 7).unwrap();
        let (state_b, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 7).unwrap();
        let (state_c, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 99).unwrap();

        assert_eq!(state_a.corp.r_and_d, state_b.corp.r_and_d);
        assert_eq!(state_a.corp.hq, state_b.corp.hq);
        assert_ne!(state_a.corp.r_and_d, state_c.corp.r_and_d);
    }

    #[test]
    fn setup_enters_mulligan_corp_phase() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();

        assert_eq!(state.phase, GamePhase::Mulligan(Side::Corp));
    }

    #[test]
    fn setup_propagates_deck_validation_errors() {
        let (registry, _corp_deck, runner_deck) = setup_fixtures();
        let bad_corp_deck = Deck { identity: CardId("missing_id".to_string()), cards: Vec::new() };

        let result = GameState::setup(&bad_corp_deck, &runner_deck, &registry, 42);

        assert_eq!(result, Err(RulesError::CardNotFoundInRegistry(CardId("missing_id".to_string()))));
    }

    #[test]
    fn keep_hand_as_corp_advances_to_mulligan_runner() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();

        let (next, events) = apply_action(&state, &registry, PlayerAction::KeepHand).unwrap();

        assert_eq!(next.phase, GamePhase::Mulligan(Side::Runner));
        assert_eq!(events, vec![GameEvent::HandKept { side: Side::Corp }]);
    }

    #[test]
    fn keep_hand_as_runner_advances_to_corp_first_turn_with_three_clicks_and_mandatory_draw() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();
        let (after_corp, _) = apply_action(&state, &registry, PlayerAction::KeepHand).unwrap();

        let (next, mut events) = apply_action(&after_corp, &registry, PlayerAction::KeepHand).unwrap();
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        assert_eq!(next.phase, GamePhase::Action(Side::Corp));
        assert_eq!(next.corp.resources.clicks.0, 3);
        assert_eq!(next.corp.hq.len(), 6); // 5 opening hand + 1 mandatory draw
        assert!(events.contains(&GameEvent::HandKept { side: Side::Runner }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }));
        assert!(events.contains(&GameEvent::CardDrawn { side: Side::Corp }));
    }

    #[test]
    fn take_mulligan_as_corp_reshuffles_and_redraws_five_then_advances_to_mulligan_runner() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();
        let original_hand = state.corp.hq.clone();

        let (next, events) = apply_action(&state, &registry, PlayerAction::TakeMulligan).unwrap();

        assert_eq!(next.phase, GamePhase::Mulligan(Side::Runner));
        assert_eq!(next.corp.hq.len(), 5);
        assert_eq!(next.corp.r_and_d.len(), 40 - 5);
        // Total cards conserved across the mulligan (returned + reshuffled + redrawn).
        assert_eq!(next.corp.hq.len() + next.corp.r_and_d.len(), original_hand.len() + state.corp.r_and_d.len());
        assert!(events.contains(&GameEvent::MulliganTaken { side: Side::Corp }));
    }

    #[test]
    fn take_mulligan_as_runner_reshuffles_and_redraws_five_then_advances_to_corp_first_turn() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();
        let (after_corp, _) = apply_action(&state, &registry, PlayerAction::KeepHand).unwrap();

        let (next, mut events) = apply_action(&after_corp, &registry, PlayerAction::TakeMulligan).unwrap();
        let (next, close_events) = close_all_windows(next, &registry);
        events.extend(close_events);

        assert_eq!(next.phase, GamePhase::Action(Side::Corp));
        assert_eq!(next.runner.grip.len(), 5);
        assert!(events.contains(&GameEvent::MulliganTaken { side: Side::Runner }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }));
    }

    #[test]
    fn mulligan_actions_outside_mulligan_phase_return_not_in_mulligan_phase() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();
        let (after_corp, _) = apply_action(&state, &registry, PlayerAction::KeepHand).unwrap();
        let (after_runner, _) = apply_action(&after_corp, &registry, PlayerAction::KeepHand).unwrap();
        let (after_runner, _) = close_all_windows(after_runner, &registry);

        let result = apply_action(&after_runner, &registry, PlayerAction::KeepHand);

        assert_eq!(
            result,
            Err(RulesError::NotInMulliganPhase { actual: GamePhase::Action(Side::Corp) })
        );
    }

    #[test]
    fn standard_action_during_mulligan_returns_wrong_phase() {
        let (registry, corp_deck, runner_deck) = setup_fixtures();
        let (state, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 42).unwrap();

        let result = apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Corp });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Mulligan(Side::Corp),
            })
        );
    }
}
