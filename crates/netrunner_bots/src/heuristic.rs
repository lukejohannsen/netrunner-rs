use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, GameState, PlayerAction, Side};

use crate::agent::BotAgent;
use crate::eval::evaluate_state;

/// Tiny random jitter added to each candidate's score, purely to break ties
/// between otherwise-equal actions without always picking the first one in
/// `legal_actions` order.
const TIE_BREAK_JITTER: f64 = 1e-3;

/// A greedy one-ply planner: for each legal action, actually applies it
/// (cheap — this is exactly what `netrunner_core::rules::legal_actions`
/// itself already does internally to validate candidates) and scores the
/// resulting state with `evaluate_state`, picking the best.
pub struct HeuristicAgent {
    side: Side,
    rng: StdRng,
}

impl HeuristicAgent {
    pub fn new(side: Side, seed: u64) -> Self {
        Self { side, rng: StdRng::seed_from_u64(seed) }
    }
}

impl BotAgent for HeuristicAgent {
    fn select_action(&mut self, state: &GameState, registry: &CardRegistry, legal_actions: &[PlayerAction]) -> PlayerAction {
        assert!(!legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");

        let mut best: Option<(f64, usize)> = None;
        for (index, action) in legal_actions.iter().enumerate() {
            let Ok((next, _events)) = apply_action(state, registry, action.clone()) else { continue };
            let score = evaluate_state(&next, self.side) + self.rng.random::<f64>() * TIE_BREAK_JITTER;
            if best.is_none_or(|(best_score, _)| score > best_score) {
                best = Some((score, index));
            }
        }

        // `legal_actions` is `netrunner_core::rules::legal_actions`'s own
        // output, so every candidate above should already succeed against
        // `apply_action` — falling back to the first entry only guards
        // against a hypothetical future divergence, not an expected case.
        best.map_or_else(|| legal_actions[0].clone(), |(_, index)| legal_actions[index].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{Card, CardId, CardType};
    use netrunner_core::rules::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, InstalledCard, InstallSlot, MemoryUnits, PlayerResources,
        RunnerState, ServerId,
    };

    fn blank_card(id: &str, card_type: CardType) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type,
            cost: 0,
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

    /// A Corp state with 3 clicks, an installed Agenda already advanced to
    /// meet its scoring requirement, and one other legal click action
    /// (`GainCreditClick`) — `ScoreAgenda` should dominate `evaluate_state`
    /// since it's worth an immediate agenda-point swing while the other
    /// candidate is worth nothing.
    fn corp_state_with_scorable_agenda(registry: &mut CardRegistry) -> GameState {
        let mut agenda = blank_card("winning_agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);

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
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            hq: Vec::new(),
            r_and_d: Vec::new(),
            archives: Vec::new(),
            installed: vec![InstalledCard {
                card: CardId("winning_agenda".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Root,
                rezzed: false,
                advancement_tokens: 3,
            }],
        };
        state
    }

    #[test]
    fn prefers_scoring_a_ready_agenda_over_an_idle_click() {
        let mut registry = CardRegistry::new();
        let state = corp_state_with_scorable_agenda(&mut registry);
        let legal = vec![
            PlayerAction::GainCreditClick { side: Side::Corp },
            PlayerAction::ScoreAgenda { card_id: CardId("winning_agenda".to_string()) },
        ];

        let mut agent = HeuristicAgent::new(Side::Corp, 1);
        let chosen = agent.select_action(&state, &registry, &legal);

        assert_eq!(chosen, PlayerAction::ScoreAgenda { card_id: CardId("winning_agenda".to_string()) });
    }

    #[test]
    fn always_returns_a_member_of_legal_actions() {
        let mut registry = CardRegistry::new();
        let state = corp_state_with_scorable_agenda(&mut registry);
        let legal = vec![
            PlayerAction::GainCreditClick { side: Side::Corp },
            PlayerAction::ScoreAgenda { card_id: CardId("winning_agenda".to_string()) },
            PlayerAction::EndTurn,
        ];

        let mut agent = HeuristicAgent::new(Side::Corp, 2);
        let chosen = agent.select_action(&state, &registry, &legal);
        assert!(legal.contains(&chosen));
    }
}
