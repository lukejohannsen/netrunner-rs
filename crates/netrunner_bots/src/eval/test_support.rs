//! The fixtures the evaluator's tests share: cards with one property
//! each, a rig card, a board with a fort, a run priced on it.

#![allow(dead_code)]

use super::*;
use netrunner_core::dsl::{AbilityDef, CardDefinition, CardId, DamageType, SubroutineBreakCount, Trigger, TriggeredEffect};
use netrunner_core::rules::{Credits, GameState, InstallId, InstalledRunnerCard};

pub(crate) fn empty() -> CardRegistry {
    CardRegistry::new()
}

/// Every playable card as printed, for the terms that read a card's
/// declared text: the economy is tested on the pool's own Hedge Fund,
/// PAD Campaign and Regolith Mining License rather than on stand-ins,
/// because the reading is of what those cards say.
pub(crate) fn pool() -> CardRegistry {
    let mut registry = CardRegistry::new();
    netrunner_core::cards::register_playable_cards(&mut registry);
    registry
}

/// A card of the pool by id, cloned.
pub(crate) fn printed(registry: &CardRegistry, id: &str) -> CardDefinition {
    registry.get(&CardId(id.to_string())).unwrap_or_else(|| panic!("{id} is in the pool")).clone()
}

/// The default weights at the guide's rate (Stage 5).
pub(crate) fn guide() -> Weights {
    Weights::default().at_the_guides_rate()
}

/// Every profile a Corp reference may score with, named: balanced and
/// each Corp plan's, for a property every one of them must have.
pub(crate) fn corp_profiles() -> Vec<(String, Weights)> {
    let mut profiles = vec![("balanced".to_string(), Weights::default())];
    profiles.extend(crate::plans::Plan::for_side(Side::Corp).map(|plan| (plan.name().to_string(), plan.weights())));
    profiles
}

/// The planner's weights for a Corp style, named by it.
pub(crate) fn planned(plans: &[crate::plans::Plan]) -> Weights {
    crate::plans::Style::new(plans).expect("a style").planned_weights(Side::Corp)
}

/// The balanced planner Corp's weights: the guide's rate with every
/// Corp's four plan terms and no plan's own.
pub(crate) fn every_corp() -> Weights {
    crate::plans::Style::BALANCED.planned_weights(Side::Corp)
}

pub(crate) fn ice(id: &str, cost: u32) -> CardDefinition {
    CardDefinition {
        id: CardId(id.to_string()),
        title: id.to_string(),
        side: Side::Corp,
        card_type: CardType::Ice(IceType::Barrier),
        cost,
        is_playable: true,
        ..Default::default()
    }
}

pub(crate) fn breaker(id: &str, restrict_to: Option<IceType>) -> CardDefinition {
    CardDefinition {
        id: CardId(id.to_string()),
        title: id.to_string(),
        side: Side::Runner,
        card_type: CardType::Program,
        abilities: vec![AbilityDef {
            text: None,
            trigger: Trigger::Paid,
            cost: None,
            requirement: None,
            effect: Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to },
            cost_discount_if: None, used_by: None, access: false, from_hand: false }],
        is_playable: true,
        ..Default::default()
    }
}

pub(crate) fn rig_card(id: &str) -> InstalledRunnerCard {
    InstalledRunnerCard { card: CardId(id.to_string()), ..Default::default() }
}

/// Two agendas, identical but for `dividends`, at the same tokens.
pub(crate) fn advancement_value_at(dividends: Option<u32>) -> impl Fn(u32) -> f64 {
    let mut agenda = ice("offworld_office", 0);
    agenda.card_type = CardType::Agenda;
    agenda.advancement_requirement = Some(3);
    agenda.dividends = dividends;
    let registry = CardRegistry::from_cards(vec![agenda]);
    move |tokens| {
        let mut state = GameState::new(0);
        state.corp.installed = vec![InstalledCard {
            card: CardId("offworld_office".to_string()),
            install_id: InstallId(1),
            advancement_tokens: tokens,
            ..Default::default()
        }];
        evaluate_state(&state, Side::Corp, &registry)
    }
}

pub(crate) fn advanceable(id: &str, required: u32) -> CardDefinition {
    CardDefinition {
        card_type: CardType::Agenda,
        advancement_requirement: Some(required),
        agenda_points: Some(2),
        ..ice(id, 0)
    }
}

/// PT Untaian's shape: a parked selection over the Corp's own
/// advanceable installs whose `then` puts a token on whichever it
/// picks. Before this term the prompt was pure cost, so the paid
/// choice that hands it over was declined every time it was offered
/// (332 of 332 across 192 heuristic-vs-heuristic games).
pub(crate) fn parked_advancement_prompt(installed: Vec<InstalledCard>) -> GameState {
    use netrunner_core::rules::PendingChoiceResume;
    let mut state = GameState::new(0);
    state.phase = GamePhase::Action(Side::Corp);
    state.corp.installed = installed;
    state.pending_decision = Some(PendingDecision::ChooseCards {
        side: Side::Corp,
        source: CardZoneRef::OwnInstalled,
        filter: CardFilter::All(vec![CardFilter::Advanceable, CardFilter::Unrezzed]),
        min: 1,
        max: 1,
        reveal: false,
        shuffle_after: false,
        destination: None,
        then: Some(Box::new(Effect::PlaceAdvancementCounters(Amount::Fixed(1)))),
        selected: Vec::new(),
        source_card: None,
        prompting_card: None,
        source_install: None,
        resume: PendingChoiceResume::None,
    });
    state
}

pub(crate) fn under_requirement() -> InstalledCard {
    InstalledCard { card: CardId("under".to_string()), install_id: InstallId(1), ..Default::default() }
}

/// A breaker priced like a real one: `break_cost` per activation
/// breaking `break_count` subroutines, `pump_cost` per `pump_amount`
/// strength — Cleaver is `(1, 2), (2, 1)`.
pub(crate) fn priced_breaker(
    id: &str,
    restrict_to: Option<IceType>,
    (break_cost, break_count): (u32, u32),
    (pump_cost, pump_amount): (u32, u32),
) -> CardDefinition {
    use netrunner_core::dsl::EffectDuration;
    let mut def = breaker(id, restrict_to);
    def.abilities[0].cost = Some(Cost::Credits(break_cost));
    def.abilities[0].effect = Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(break_count), restrict_to };
    def.abilities.push(AbilityDef {
        text: None,
        trigger: Trigger::Paid,
        cost: Some(Cost::Credits(pump_cost)),
        requirement: None,
        effect: Effect::BoostStrength { amount: pump_amount, duration: EffectDuration::Encounter },
        cost_discount_if: None, used_by: None, access: false, from_hand: false });
    def
}

/// One piece of ICE on a run, all subroutines pending. Its strength is
/// what its card prints — a run's ice stores none — so the card is
/// named for the number and `with_printed_ice` registers it.
pub(crate) fn run_ice(strength: i32, ice_type: IceType, subroutines: usize, rezzed: bool) -> RunIce {
    use netrunner_core::rules::{EncounteredSubroutine, InstallId};
    RunIce {
        install_id: InstallId::PLACEHOLDER,
        card_id: CardId(format!("ice{strength}")),
        ice_type,
        subroutines: (0..subroutines)
            .map(|id| EncounteredSubroutine {
                id,
                definition: netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
                status: SubroutineStatus::Pending,
                gained: false,
            })
            .collect(),
        rezzed,
    }
}

/// `registry` with a card for each of `run_ice`'s pieces that prints the
/// strength it was asked for.
pub(crate) fn with_printed_ice(registry: &CardRegistry, ice: &[RunIce]) -> CardRegistry {
    let mut registry = registry.clone();
    for ice in ice {
        if let Some(strength) = ice.card_id.0.strip_prefix("ice").and_then(|n| n.parse().ok()) {
            registry.insert(CardDefinition {
                id: ice.card_id.clone(),
                title: ice.card_id.0.clone(),
                side: Side::Corp,
                card_type: CardType::Ice(ice.ice_type),
                strength: Some(strength),
                ..CardDefinition::default()
            });
        }
    }
    registry
}

/// `evaluate_state` for the Runner, `credits` in hand, approaching the
/// outermost ICE of a run over `ice` — minus the same board with no run,
/// so the result is exactly what the run term contributed.
pub(crate) fn run_term(rig: Vec<InstalledRunnerCard>, credits: u32, ice: Vec<RunIce>, position: usize, registry: &CardRegistry) -> f64 {
    use netrunner_core::rules::ServerId;
    let registry = &with_printed_ice(registry, &ice);
    let mut idle = GameState::new(0);
    idle.runner.resources.credits = Credits(credits);
    idle.runner.rig = rig;
    // One card in HQ, so the breach has one hidden access to be worth.
    idle.corp.hq = corp_cards("hq", 1);
    let mut running = idle.clone();
    running.active_run = Some(RunState { server: ServerId::Hq, ice, position, ..Default::default() });
    let term = evaluate_state(&running, Side::Runner, registry) - evaluate_state(&idle, Side::Runner, registry);
    round3(term) // the other terms cancel, up to float noise
}

pub(crate) fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A breaker with a printed install cost and memory cost, for the
/// savings term.
pub(crate) fn costed_breaker(id: &str, restrict_to: Option<IceType>, cost: u32) -> CardDefinition {
    CardDefinition { cost, memory_cost: Some(1), ..breaker(id, restrict_to) }
}

pub(crate) fn asset(id: &str, cost: u32) -> CardDefinition {
    CardDefinition { card_type: CardType::Asset, ..ice(id, cost) }
}

pub(crate) fn corp_cards(prefix: &str, n: usize) -> Vec<CardId> {
    (0..n).map(|i| CardId(format!("{prefix}_{i}"))).collect()
}

/// An access-punishing asset the Runner can read: Urtica Cipher's shape.
pub(crate) fn ambush(id: &str) -> CardDefinition {
    let mut def = asset(id, 0);
    def.triggers = vec![TriggeredEffect {
        subject: None, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_heap: false, from_runner_score_area: false,
        text: None,
        trigger: Trigger::OnAccessed,
        effects: vec![Effect::DealDamage(DamageType::Net, 2)],
        requirement: None,
    }];
    def
}

/// The run term alone — the Runner's score in a breakable run on
/// `server` minus the same board idle.
pub(crate) fn prospect(state: &GameState, server: netrunner_core::rules::ServerId, registry: &CardRegistry) -> f64 {
    let mut running = state.clone();
    running.active_run = Some(RunState { server, ..Default::default() });
    let term = evaluate_state(&running, Side::Runner, registry) - evaluate_state(state, Side::Runner, registry);
    (term * 1000.0).round() / 1000.0
}

/// `w` with the fort terms off, for a test of a term they would
/// otherwise add to — every Corp profile carries them (Phase 5 §23).
pub(crate) fn without_fort(w: Weights) -> Weights {
    Weights { central_ice_weight: 0.0, fort_weight: 0.0, exposed_agenda_weight: 0.0, ..w }
}

/// A board for `fort_value`: ICE on the servers named, one piece per
/// entry, and an agenda and an asset in the roots named.
pub(crate) fn fort_board(ice: &[netrunner_core::rules::ServerId], roots: &[(&str, netrunner_core::rules::ServerId)]) -> GameState {
    use netrunner_core::rules::InstallSlot;
    let mut state = GameState::new(0);
    let mut n = 0;
    let mut next = || {
        n += 1;
        InstallId(n)
    };
    for server in ice {
        state.corp.installed.push(InstalledCard {
            card: CardId("wall".to_string()),
            install_id: next(),
            slot: InstallSlot::Ice,
            server: *server,
            ..Default::default()
        });
    }
    for (card, server) in roots {
        state.corp.installed.push(InstalledCard {
            card: CardId(card.to_string()),
            install_id: next(),
            slot: InstallSlot::Root,
            server: *server,
            ..Default::default()
        });
    }
    state
}

pub(crate) fn fort_registry() -> CardRegistry {
    let mut agenda = ice("plan", 0);
    agenda.card_type = CardType::Agenda;
    agenda.advancement_requirement = Some(3);
    let mut asset = ice("campaign", 0);
    asset.card_type = CardType::Asset;
    CardRegistry::from_cards(vec![ice("wall", 0), agenda, asset])
}

/// The planner's weights for a Runner style, named by it (Stage 7).
pub(crate) fn planned_runner(plans: &[crate::plans::Plan]) -> Weights {
    crate::plans::Style::new(plans).expect("a style").planned_weights(Side::Runner)
}

/// The balanced planner Runner's weights: the guide's rate with every
/// Runner's four plan terms and no plan's own.
pub(crate) fn every_runner() -> Weights {
    crate::plans::Style::BALANCED.planned_weights(Side::Runner)
}

/// A Corp identity of `faction`, for the terms that read the card
/// across the table.
pub(crate) fn corp_identity(id: &str, faction: netrunner_core::card::Faction) -> CardDefinition {
    CardDefinition {
        id: CardId(id.to_string()),
        title: id.to_string(),
        side: Side::Corp,
        card_type: CardType::Identity,
        faction: Some(faction),
        is_playable: true,
        ..Default::default()
    }
}

/// `state` with the Corp seated as `faction`.
pub(crate) fn against(mut state: GameState, faction: netrunner_core::card::Faction) -> (GameState, CardDefinition) {
    let id = format!("{faction:?}").to_lowercase();
    state.corp.identity = Some(CardId(id.clone()));
    (state, corp_identity(&id, faction))
}
