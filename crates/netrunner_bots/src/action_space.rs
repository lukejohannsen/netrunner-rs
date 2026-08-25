//! Ergonomic `0..ActionSpace::SIZE` index-space helpers built on
//! `netrunner_core::rules::{ActionSpace, get_action_mask}` — the bridge
//! `crate::puct::PuctAgent` (and any future fixed-shape policy/value
//! network) uses instead of working with `PlayerAction` directly.
//!
//! Legality itself is never re-derived here: `legal_indices` is exactly
//! `get_action_mask`'s `true` positions, and `step_index` is exactly
//! `ActionSpace::action_at` followed by `GameState::step`.

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{get_action_mask, ActionSpace, GameEvent, GameState, RulesError};

/// Every index in `0..ActionSpace::SIZE` currently legal for `state` — the
/// `true` positions of `get_action_mask`, as a compact list rather than a
/// full boolean vector. Cheap to call per-node during search: `PuctNode::
/// expand` doesn't use this directly (it needs the full mask to place
/// priors at their real indices), but callers that only need "what can I
/// do" — e.g. a uniform-random baseline over indices — can use this
/// instead of scanning the mask themselves.
pub fn legal_indices(state: &GameState, registry: &CardRegistry) -> Vec<usize> {
    get_action_mask(state, registry)
        .into_iter()
        .enumerate()
        .filter_map(|(index, legal)| legal.then_some(index))
        .collect()
}

/// Errors from `step_index`: on top of whatever `GameState::step` itself
/// can reject, `index` might not decode to any action at all against
/// `state` (out of range, or a slot `ActionSpace::action_at` can't
/// currently resolve — see its doc comment).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IndexedActionError {
    #[error("index {0} does not decode to any action for the current state")]
    NoActionAtIndex(usize),
    #[error(transparent)]
    Rules(#[from] RulesError),
}

/// The fixed-index-space counterpart of `state.step(registry, action)`:
/// decodes `index` via `ActionSpace::action_at`, then applies the result.
/// Agrees exactly with `GameState::step` on which `(index, action)` pairs
/// succeed, since `action_at` is the only thing standing between them.
pub fn step_index(
    state: &GameState,
    registry: &CardRegistry,
    index: usize,
) -> Result<(GameState, Vec<GameEvent>), IndexedActionError> {
    let action = ActionSpace::action_at(state, index).ok_or(IndexedActionError::NoActionAtIndex(index))?;
    state.step(registry, action).map_err(IndexedActionError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{Card, CardId, CardType};
    use netrunner_core::rules::{
        apply_action, legal_actions, AgendaPoints, Clicks, CorpState, Credits, GamePhase, MemoryUnits,
        PlayerResources, RunnerState, Side,
    };

    fn blank_card(id: &str, side: Side, card_type: CardType, cost: u32) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type,
            cost,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None,
            subtypes: Vec::new(),
            play_requirement: None,
            recurring_credits: None,
            first_install_discount: None,
        }
    }

    fn empty_runner() -> RunnerState {
        RunnerState {
            identity: None,
            scored_agendas: Vec::new(),
            resources: PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) },
            memory_units: MemoryUnits(0),
            brain_damage: 0,
            tags: 0,
            grip: Vec::new(),
            stack: Vec::new(),
            rig: Vec::new(),
            heap: Vec::new(),
            link_strength: 0,
            first_hq_run_used_this_turn: false,
            first_install_discount_used_this_turn: false,
        }
    }

    /// A Corp click phase with a mix of hand cards (an Operation and an
    /// Asset) giving several distinct legal action *kinds* — not just
    /// `GainCreditClick` — so the index/raw comparison below actually
    /// exercises more than one `ActionSpace` segment.
    fn sample_corp_turn_state() -> (CardRegistry, GameState) {
        let mut registry = CardRegistry::new();
        registry.insert(blank_card("hedge_fund", Side::Corp, CardType::Operation, 5));
        registry.insert(blank_card("pad_campaign", Side::Corp, CardType::Asset, 2));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = CorpState {
            identity: None,
            bad_publicity: 0,
            first_install_used_this_turn: false,
            recurring_credits: 0,
            recurring_credits_max: 0,
            scored_agendas: Vec::new(),
            resources: PlayerResources { credits: Credits(10), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            hq: vec![CardId("hedge_fund".to_string()), CardId("pad_campaign".to_string())],
            r_and_d: Vec::new(),
            archives: Vec::new(),
            installed: Vec::new(),
        };
        (registry, state)
    }

    #[test]
    fn legal_indices_matches_true_entries_in_the_mask() {
        let (registry, state) = sample_corp_turn_state();
        let mask = get_action_mask(&state, &registry);
        let indices = legal_indices(&state, &registry);

        assert_eq!(indices.len(), mask.iter().filter(|&&legal| legal).count());
        for index in &indices {
            assert!(mask[*index]);
        }
    }

    #[test]
    fn step_index_matches_state_step_for_every_legal_index() {
        let (registry, state) = sample_corp_turn_state();
        for index in legal_indices(&state, &registry) {
            let action = ActionSpace::action_at(&state, index).unwrap();
            let via_index = step_index(&state, &registry, index).unwrap();
            let via_action = state.step(&registry, action).unwrap();
            assert_eq!(via_index, via_action);
        }
    }

    #[test]
    fn index_based_expansion_produces_identical_transitions_as_raw_legal_actions_iteration() {
        let (registry, state) = sample_corp_turn_state();

        let mut raw: Vec<GameState> =
            legal_actions(&state, &registry).into_iter().map(|action| apply_action(&state, &registry, action).unwrap().0).collect();

        let mut via_index: Vec<GameState> =
            legal_indices(&state, &registry).into_iter().map(|index| step_index(&state, &registry, index).unwrap().0).collect();

        assert_eq!(raw.len(), via_index.len());
        // `GameState` has no `Ord`, so compare as multisets via mutual
        // containment rather than sorting.
        raw.retain(|candidate| {
            if let Some(position) = via_index.iter().position(|other| other == candidate) {
                via_index.remove(position);
                false
            } else {
                true
            }
        });
        assert!(raw.is_empty(), "raw legal_actions produced transitions missing from index-based stepping");
        assert!(via_index.is_empty(), "index-based stepping produced transitions missing from raw legal_actions");
    }

    #[test]
    fn step_index_out_of_range_reports_no_action_at_index() {
        let (registry, state) = sample_corp_turn_state();
        let result = step_index(&state, &registry, ActionSpace::SIZE + 1);
        assert_eq!(result, Err(IndexedActionError::NoActionAtIndex(ActionSpace::SIZE + 1)));
    }

    #[test]
    fn step_index_propagates_rules_errors_for_a_currently_illegal_action() {
        // Index 0 is `DrawCardClick`, which is Runner-only — illegal here
        // during the Corp's own Action phase, so this should surface
        // `GameState::step`'s own rejection rather than a bogus success.
        let (registry, state) = sample_corp_turn_state();
        let mask = get_action_mask(&state, &registry);
        assert!(!mask[0], "DrawCardClick should be illegal for the Corp");

        let result = step_index(&state, &registry, 0);
        assert!(matches!(result, Err(IndexedActionError::Rules(_))));
    }
}
