use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, PlayerAction, Side};
use netrunner_core::view::ClientView;

use crate::agent::BotAgent;
use crate::determinize::determinize;
use crate::eval::evaluate_state;

/// Tiny random jitter added to each candidate's score, purely to break ties
/// between otherwise-equal actions without always picking the first one in
/// `legal_actions` order.
const TIE_BREAK_JITTER: f64 = 1e-3;

/// A greedy one-ply planner: determinizes one concrete `GameState`
/// consistent with the current `ClientView`, then for each of `view.
/// legal_actions` actually applies it against that sample (cheap — this is
/// exactly what `netrunner_core::rules::legal_actions` itself already does
/// internally to validate candidates) and scores the result with
/// `evaluate_state`, picking the best.
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
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");

        let sample = determinize(view, registry, &mut self.rng);

        let mut best: Option<(f64, usize)> = None;
        for (index, action) in view.legal_actions.iter().enumerate() {
            let Ok((next, _events)) = apply_action(&sample, registry, action.clone()) else { continue };
            let score = evaluate_state(&next, self.side, registry) + self.rng.random::<f64>() * TIE_BREAK_JITTER;
            if best.is_none_or(|(best_score, _)| score > best_score) {
                best = Some((score, index));
            }
        }

        // `view.legal_actions` came from `legal_actions_for`, whose
        // ownership filtering doesn't depend on hidden info (see its doc
        // comment), so every candidate above should already succeed
        // against the determinized `sample` too — falling back to the
        // first entry only guards against a hypothetical future
        // divergence, not an expected case.
        best.map_or_else(|| view.legal_actions[0].clone(), |(_, index)| view.legal_actions[index].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType};
    use netrunner_core::rules::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, GameState, InstallId, InstalledCard, MemoryUnits,
        PlayerResources, RunnerState, ServerId,
    };
    use netrunner_core::view::build_client_view;

    fn blank_card(id: &str, card_type: CardType) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type,
            is_playable: true,
            ..Default::default()
        }
    }

    fn empty_runner() -> RunnerState {
        RunnerState {
            resources: PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) },
            memory_units: MemoryUnits(0),
            ..Default::default()
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
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            installed: vec![InstalledCard {
                card: CardId("winning_agenda".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                advancement_tokens: 3,
                ..Default::default()
            }],
            ..Default::default()
        };
        state
    }

    #[test]
    fn prefers_scoring_a_ready_agenda_over_an_idle_click() {
        let mut registry = CardRegistry::new();
        let state = corp_state_with_scorable_agenda(&mut registry);
        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::ScoreAgenda { target: InstallId(1) }));

        let mut agent = HeuristicAgent::new(Side::Corp, 1);
        let chosen = agent.select_action(&view, &registry);

        assert_eq!(chosen, PlayerAction::ScoreAgenda { target: InstallId(1) });
    }

    /// The Runner-side counterpart: a rezzed ICE the rig cannot break
    /// makes a run worth less than a credit, and an unrezzed one does not
    /// (ROADMAP Phase 2 §5's eagerness item).
    #[test]
    fn prefers_a_credit_to_running_into_rezzed_ice_it_cannot_break() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
        wall.strength = Some(1);
        wall.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::EndTheRun }];
        registry.insert(wall);

        let state_with_ice = |rezzed| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.runner = empty_runner();
            state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
            state.corp.resources.credits = Credits(5);
            for (index, server) in [ServerId::Hq, ServerId::RnD, ServerId::Archives].into_iter().enumerate() {
                state.corp.installed.push(InstalledCard {
                    card: CardId("wall".to_string()),
                    install_id: InstallId(index as u32 + 1),
                    server,
                    slot: InstallSlot::Ice,
                    rezzed,
                    ..Default::default()
                });
            }
            state
        };

        let rezzed = state_with_ice(true);
        let view = build_client_view(&rezzed, &registry, Side::Runner);
        assert!(view.legal_actions.iter().any(|a| matches!(a, PlayerAction::InitiateRun { .. })));
        let chosen = HeuristicAgent::new(Side::Runner, 3).select_action(&view, &registry);
        assert!(!matches!(chosen, PlayerAction::InitiateRun { .. }), "ran into rezzed ICE with no breaker: {chosen:?}");

        let unrezzed = state_with_ice(false);
        let view = build_client_view(&unrezzed, &registry, Side::Runner);
        let chosen = HeuristicAgent::new(Side::Runner, 3).select_action(&view, &registry);
        assert!(matches!(chosen, PlayerAction::InitiateRun { .. }), "unrezzed ICE is no reason not to run: {chosen:?}");
    }

    /// With a breaker in grip it cannot yet afford and nothing but open
    /// servers to run, the Runner clicks for the credit rather than
    /// running (ROADMAP Phase 2 §5's savings item).
    #[test]
    fn saves_for_a_breaker_in_grip_instead_of_running_an_open_server() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineBreakCount, Trigger};
        use netrunner_core::dsl::AbilityDef;
        let mut registry = CardRegistry::new();
        let mut cleaver = blank_card("cleaver", CardType::Program);
        cleaver.side = Side::Runner;
        cleaver.cost = 3;
        cleaver.memory_cost = Some(1);
        cleaver.abilities = vec![AbilityDef {
            trigger: Trigger::Paid,
            cost: None,
            requirement: None,
            effect: Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: Some(IceType::Barrier) },
            cost_discount_if: None,
        }];
        registry.insert(cleaver);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(1), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("cleaver".to_string())];
        let view = build_client_view(&state, &registry, Side::Runner);
        assert!(view.legal_actions.iter().any(|a| matches!(a, PlayerAction::InitiateRun { .. })));
        assert!(view.legal_actions.contains(&PlayerAction::GainCreditClick { side: Side::Runner }));

        let chosen = HeuristicAgent::new(Side::Runner, 3).select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::GainCreditClick { side: Side::Runner }, "should save for Cleaver");
    }

    #[test]
    fn always_returns_a_member_of_legal_actions() {
        let mut registry = CardRegistry::new();
        let state = corp_state_with_scorable_agenda(&mut registry);
        let view = build_client_view(&state, &registry, Side::Corp);

        let mut agent = HeuristicAgent::new(Side::Corp, 2);
        let chosen = agent.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&chosen));
    }
}
