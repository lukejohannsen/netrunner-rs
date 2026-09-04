use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, PlayerAction, Side};
use netrunner_core::view::ClientView;

use crate::agent::BotAgent;
use crate::determinize::determinize;
use crate::eval::{evaluate_state_with, Weights};
use crate::personality::Personality;

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
    /// The evaluator's terms — `Weights::default()` unless a
    /// `Personality` was asked for.
    weights: Weights,
}

impl HeuristicAgent {
    pub fn new(side: Side, seed: u64) -> Self {
        Self::with_personality(side, seed, Personality::Balanced)
    }

    /// `new`, scoring with `personality.weights()`.
    pub fn with_personality(side: Side, seed: u64, personality: Personality) -> Self {
        Self { side, rng: StdRng::seed_from_u64(seed), weights: personality.weights() }
    }
}

impl BotAgent for HeuristicAgent {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");

        let sample = determinize(view, registry, &mut self.rng);

        let mut best: Option<(f64, usize)> = None;
        for (index, action) in view.legal_actions.iter().enumerate() {
            let Ok((next, _events)) = apply_action(&sample, registry, action.clone()) else { continue };
            let score = evaluate_state_with(&next, self.side, registry, &self.weights) + self.rng.random::<f64>() * TIE_BREAK_JITTER;
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

    /// The archetypes, on the same position: a naked installed agenda, an
    /// ICE in HQ and the clicks to do either. Rush advances it; Glacier
    /// puts the ICE in front of it first.
    #[test]
    fn a_rush_corp_advances_where_a_glacier_corp_installs_ice() {
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);
        registry.insert(blank_card("wall", CardType::Ice(netrunner_core::dsl::IceType::Barrier)));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = CorpState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            hq: vec![CardId("wall".to_string())],
            installed: vec![InstalledCard {
                card: CardId("agenda".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let view = build_client_view(&state, &registry, Side::Corp);
        let advance = PlayerAction::AdvanceCard { target: InstallId(1) };
        assert!(view.legal_actions.contains(&advance));
        let ice_in_front = view
            .legal_actions
            .iter()
            .find(|a| matches!(a, PlayerAction::InstallCard { zone: ServerId::Remote(0), .. }))
            .cloned()
            .expect("the ICE can be installed in front of the agenda");

        let mut rush = HeuristicAgent::with_personality(Side::Corp, 1, Personality::Rush);
        assert_eq!(rush.select_action(&view, &registry), advance);
        let mut glacier = HeuristicAgent::with_personality(Side::Corp, 1, Personality::Glacier);
        assert_eq!(glacier.select_action(&view, &registry), ice_in_front);
    }

    /// PT Untaian's discard-phase offer, which the Corp declined every
    /// one of the 332 times it was made across 192 heuristic-vs-heuristic
    /// games: pay 1[c] and put an advancement token on an installed card.
    /// Accepting hands the Corp a `PromptChooseCards`, so before
    /// `PENDING_DECISION_UPSIDE_WEIGHT` the accept was charged the
    /// prompt's penalty on top of the credit while declining resolved to
    /// nothing and was free.
    #[test]
    fn accepts_a_paid_choice_that_buys_an_advancement_token() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef, Cost, Effect};
        use netrunner_core::rules::{PendingPaidChoice, PendingPaidChoiceResume};
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = CorpState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            installed: vec![InstalledCard {
                card: CardId("agenda".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                ..Default::default()
            }],
            ..Default::default()
        };
        state.pending_paid_choice = Some(PendingPaidChoice {
            side: Side::Corp,
            cost: Cost::Credits(1),
            if_paid: Effect::PromptChooseCards {
                side: Side::Corp,
                source: CardZoneRef::OwnInstalled,
                filter: CardFilter::All(vec![CardFilter::Advanceable, CardFilter::Unrezzed]),
                min: 1,
                max: 1,
                reveal: false,
                shuffle_after: false,
                destination: None,
                then: Some(Box::new(Effect::AddAdvancementTokens(1))),
            },
            if_declined: Effect::Sequence(Vec::new()),
            source_card: None,
            source_install: None,
            resume: PendingPaidChoiceResume::None,
        });

        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::DeclinePendingPaidChoice));
        let chosen = HeuristicAgent::new(Side::Corp, 1).select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None });
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
        wall.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
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
            cost_discount_if: None, used_by: None }];
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

    /// With an empty grip, open servers and a stack to draw from, the
    /// Runner draws before it runs (ROADMAP Phase 2 §5's draw item).
    #[test]
    fn draws_with_an_empty_grip_instead_of_running_an_open_server() {
        let mut registry = CardRegistry::new();
        let mut filler = blank_card("filler", CardType::Resource);
        filler.side = Side::Runner;
        filler.cost = 9;
        registry.insert(filler);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.stack = vec![CardId("filler".to_string()); 5];
        let view = build_client_view(&state, &registry, Side::Runner);
        assert!(view.legal_actions.iter().any(|a| matches!(a, PlayerAction::InitiateRun { .. })));
        assert!(view.legal_actions.contains(&PlayerAction::DrawCardClick { side: Side::Runner }));

        let chosen = HeuristicAgent::new(Side::Runner, 3).select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::DrawCardClick { side: Side::Runner });
    }

    /// The Corp-side counterpart of the Runner's draw test: with an empty
    /// HQ and a stocked R&D, the Corp clicks to draw rather than for a
    /// credit (ROADMAP Phase 2 §5's Corp item).
    #[test]
    fn corp_draws_with_an_empty_hq_instead_of_clicking_for_a_credit() {
        let mut registry = CardRegistry::new();
        registry.insert(blank_card("filler", CardType::Asset));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("filler".to_string()); 10];
        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::DrawCardClick { side: Side::Corp }));
        assert!(view.legal_actions.contains(&PlayerAction::GainCreditClick { side: Side::Corp }));

        let chosen = HeuristicAgent::new(Side::Corp, 3).select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::DrawCardClick { side: Side::Corp });
    }

    /// With an ICE-protected remote and a naked one both open, an agenda
    /// goes behind the ICE (ROADMAP Phase 2 §5's placement item).
    #[test]
    fn installs_an_agenda_behind_ice_rather_than_into_a_naked_remote() {
        use netrunner_core::dsl::IceType;
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);
        registry.insert(blank_card("wall", CardType::Ice(IceType::Barrier)));
        registry.insert(blank_card("filler", CardType::Operation));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("filler".to_string()); 10];
        state.corp.hq = vec![CardId("agenda".to_string()), CardId("filler".to_string()), CardId("filler".to_string())];
        // Remote 0 has ICE and an empty root; remote 1 is a naked empty root
        // (an ICE-less remote is represented by nothing at all, so the
        // "naked" option is the fresh remote the engine always offers).
        state.corp.installed.push(InstalledCard {
            card: CardId("wall".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            slot: InstallSlot::Ice,
            ..Default::default()
        });
        let view = build_client_view(&state, &registry, Side::Corp);
        let agenda_installs: Vec<_> = view
            .legal_actions
            .iter()
            .filter(|a| matches!(a, PlayerAction::InstallCard { card_id, slot: InstallSlot::Root, .. } if card_id.0 == "agenda"))
            .collect();
        assert!(agenda_installs.len() >= 2, "expected both a protected and a naked remote on offer: {agenda_installs:?}");

        let chosen = HeuristicAgent::new(Side::Corp, 3).select_action(&view, &registry);
        assert!(
            matches!(&chosen, PlayerAction::InstallCard { card_id, zone: ServerId::Remote(0), slot: InstallSlot::Root } if card_id.0 == "agenda"),
            "should install the agenda behind the ICE: {chosen:?}"
        );
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
