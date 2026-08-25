//! Enumerates every `PlayerAction` currently legal for `state`, without
//! mutating it — the "what can I do right now" query a UI or a bot needs
//! that `apply_action` alone doesn't answer.
//!
//! Strategy: generate a bounded set of *candidate* actions from state that's
//! already visible (hand contents, installed cards, run/access state,
//! registry lookups), then keep only the candidates for which
//! `apply_action(state, registry, candidate)` actually succeeds on a cloned
//! state. This makes correctness free for everything `apply_action` already
//! enforces — phase, the `active_trace` global gate, per-handler
//! `paid_ability_window` gating, priority, costs, ability trigger/requirement
//! checks, hand-size limits — with zero duplicated rules logic. The one
//! exception is `install_card_candidates`: `engine::install_card` never
//! checks that a card's `CardType` matches the `InstallSlot`/`ServerId`
//! it's being installed into (see its doc comment there), so this module
//! applies that correspondence itself before probing.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardType, Trigger};
use crate::rules::action::PlayerAction;
use crate::rules::apply_action;
use crate::rules::run::{AccessPhase, RunPhase, ServerId, SubroutineStatus};
use crate::rules::state::{GamePhase, GameState, InstallSlot, Side};

pub fn legal_actions(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    candidate_actions(state, registry)
        .into_iter()
        .filter(|action| apply_action(state, registry, action.clone()).is_ok())
        .collect()
}

fn candidate_actions(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let mut candidates = static_candidates();
    candidates.extend(install_card_candidates(state, registry));
    candidates.extend(rez_ice_candidates(state));
    candidates.extend(initiate_run_candidates(state));
    candidates.extend(play_card_candidates(state, registry));
    candidates.extend(break_subroutine_candidates(state));
    candidates.extend(discard_candidates(state));
    candidates.extend(activate_ability_candidates(state, registry));
    candidates.extend(advance_score_trash_candidates(state, registry));
    candidates.extend(access_flow_candidates(state));
    candidates.extend(pass_priority_candidates(state));
    candidates.extend(trace_bid_candidates(state));
    candidates
}

/// Actions with no state-dependent parameters at all — legality (phase,
/// side, active-run/window/hand-size preconditions) is entirely the probe's
/// job.
fn static_candidates() -> Vec<PlayerAction> {
    vec![
        PlayerAction::GainCreditClick { side: Side::Corp },
        PlayerAction::GainCreditClick { side: Side::Runner },
        PlayerAction::DrawCardClick,
        PlayerAction::ContinueRun,
        PlayerAction::JackOut,
        PlayerAction::CompleteRun,
        PlayerAction::EndTurn,
        PlayerAction::KeepHand,
        PlayerAction::TakeMulligan,
        PlayerAction::RemoveTag,
    ]
}

/// Distinct `Remote(n)` ids the Corp has already installed into, sorted.
fn existing_remote_ids(state: &GameState) -> Vec<u32> {
    let mut ids: Vec<u32> = state
        .corp
        .installed
        .iter()
        .filter_map(|c| match c.server {
            ServerId::Remote(n) => Some(n),
            _ => None,
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// One remote id beyond every existing one — offers "install into a new
/// remote" without the engine having any such concept itself (see
/// `ServerId`'s doc comment: `Remote(n)` for any `n` is accepted
/// unconditionally, nothing tracks which ids are "in use").
fn fresh_remote_id(existing: &[u32]) -> u32 {
    existing.iter().max().map_or(0, |max| max + 1)
}

/// `engine::install_card` never checks that `slot`/`zone` correspond to the
/// card's actual `CardType` (see its doc comment) — the only place this
/// module can't rely on the probe alone. ICE may protect any server
/// (central or remote); Agendas/Assets only ever install into a remote.
fn install_card_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let existing_remotes = existing_remote_ids(state);
    let fresh_remote = fresh_remote_id(&existing_remotes);
    let mut remote_zones: Vec<ServerId> = existing_remotes.iter().copied().map(ServerId::Remote).collect();
    remote_zones.push(ServerId::Remote(fresh_remote));

    let mut candidates = Vec::new();
    for card_id in &state.corp.hq {
        let Some(card) = registry.get(card_id) else { continue };
        match card.card_type {
            CardType::Ice(_) => {
                let mut zones = vec![ServerId::Hq, ServerId::RnD, ServerId::Archives];
                zones.extend(remote_zones.iter().copied());
                for zone in zones {
                    candidates.push(PlayerAction::InstallCard { card_id: card_id.clone(), zone, slot: InstallSlot::Ice });
                }
            }
            CardType::Agenda | CardType::Asset => {
                for zone in &remote_zones {
                    candidates.push(PlayerAction::InstallCard { card_id: card_id.clone(), zone: *zone, slot: InstallSlot::Root });
                }
            }
            _ => {}
        }
    }
    candidates
}

/// `rez_ice`'s name is misleading — it rezzes any unrezzed Corp install,
/// not just `InstallSlot::Ice` ones (confirmed in `engine::rez_ice`, which
/// never checks `slot`). Its own phase/rez-window legality is left to the
/// probe.
fn rez_ice_candidates(state: &GameState) -> Vec<PlayerAction> {
    state
        .corp
        .installed
        .iter()
        .filter(|c| !c.rezzed)
        .map(|c| PlayerAction::RezIce { ice_id: c.card.clone() })
        .collect()
}

/// Restricted to servers that actually exist (centrals + remotes the Corp
/// has installed into) — the engine would accept an arbitrary unused
/// `Remote(n)` too, but running a server with nothing ever installed there
/// isn't a meaningful choice.
fn initiate_run_candidates(state: &GameState) -> Vec<PlayerAction> {
    let mut servers = vec![ServerId::Hq, ServerId::RnD, ServerId::Archives];
    servers.extend(existing_remote_ids(state).into_iter().map(ServerId::Remote));
    servers.into_iter().map(|server| PlayerAction::InitiateRun { server }).collect()
}

/// `PlayEvent`/`InstallHardware`/`InstallProgram` (Runner grip) and
/// `PlayOperation` (Corp HQ), keyed off each card's registry `CardType`.
fn play_card_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let mut candidates = Vec::new();
    for card_id in &state.runner.grip {
        let Some(card) = registry.get(card_id) else { continue };
        match card.card_type {
            CardType::Event => candidates.push(PlayerAction::PlayEvent { card_id: card_id.clone() }),
            CardType::Hardware => candidates.push(PlayerAction::InstallHardware { card_id: card_id.clone() }),
            // No data-driven memory-unit stat exists on `Card` (see
            // `Card::strength`'s doc comment) — `memory_cost` is entirely
            // caller-chosen, so this only ever offers 0.
            CardType::Program => {
                candidates.push(PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 0 })
            }
            _ => {}
        }
    }
    for card_id in &state.corp.hq {
        if registry.get(card_id).is_some_and(|c| c.card_type == CardType::Operation) {
            candidates.push(PlayerAction::PlayOperation { card_id: card_id.clone() });
        }
    }
    candidates
}

/// Exact read of the ICE currently being encountered's pending subroutines
/// — no guessing needed, `RunIce`/`EncounteredSubroutine` give this
/// directly.
fn break_subroutine_candidates(state: &GameState) -> Vec<PlayerAction> {
    let Some(run) = &state.active_run else { return Vec::new() };
    if run.phase != RunPhase::EncounterIce {
        return Vec::new();
    }
    let Some(ice) = run.ice.get(run.position) else { return Vec::new() };
    ice.subroutines
        .iter()
        .filter(|s| s.status == SubroutineStatus::Pending)
        .map(|s| PlayerAction::BreakSubroutine { ice_id: ice.card_id.clone(), subroutine_index: s.id })
        .collect()
}

/// Exact read of `GamePhase::Discard`'s hand — no other phase makes
/// `DiscardCard` legal.
fn discard_candidates(state: &GameState) -> Vec<PlayerAction> {
    let GamePhase::Discard { side, .. } = state.phase else { return Vec::new() };
    let hand: &[CardId] = match side {
        Side::Corp => &state.corp.hq,
        Side::Runner => &state.runner.grip,
    };
    hand.iter().map(|card_id| PlayerAction::DiscardCard { card_id: card_id.clone() }).collect()
}

/// Every rezzed Corp install and every Runner rig card, for each
/// `Trigger::Paid` ability its registry definition carries. Priority
/// (during an open window), cost affordability, and any `EffectRequirement`
/// are all left to the probe.
fn activate_ability_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let mut candidates = Vec::new();
    for installed in state.corp.installed.iter().filter(|c| c.rezzed) {
        candidates.extend(paid_ability_candidates(&installed.card, registry));
    }
    for rig_card in &state.runner.rig {
        candidates.extend(paid_ability_candidates(&rig_card.card, registry));
    }
    candidates
}

fn paid_ability_candidates(card_id: &CardId, registry: &CardRegistry) -> Vec<PlayerAction> {
    let Some(card) = registry.get(card_id) else { return Vec::new() };
    card.abilities
        .iter()
        .enumerate()
        .filter(|(_, ability)| ability.trigger == Trigger::Paid)
        .map(|(ability_index, _)| PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index })
        .collect()
}

/// `AdvanceCard`/`ScoreAgenda` (Corp installed cards) and `TrashResource`
/// (Runner rig), keyed off each card's registry definition.
fn advance_score_trash_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let mut candidates = Vec::new();
    for installed in &state.corp.installed {
        let Some(card) = registry.get(&installed.card) else { continue };
        if card.advancement_requirement.is_some() {
            candidates.push(PlayerAction::AdvanceCard { card_id: installed.card.clone() });
        }
        if card.card_type == CardType::Agenda {
            candidates.push(PlayerAction::ScoreAgenda { card_id: installed.card.clone() });
        }
    }
    for rig_card in &state.runner.rig {
        if registry.get(&rig_card.card).is_some_and(|c| c.card_type == CardType::Resource) {
            candidates.push(PlayerAction::TrashResource { card_id: rig_card.card.clone() });
        }
    }
    candidates
}

/// Exact read of the active run's `AccessPhase` — precise legal targets are
/// already materialized in state, no guessing needed.
fn access_flow_candidates(state: &GameState) -> Vec<PlayerAction> {
    let Some(run) = &state.active_run else { return Vec::new() };
    let Some(access) = &run.access_state else { return Vec::new() };

    match &access.phase {
        AccessPhase::SelectNextCard { selectable_cards } => selectable_cards
            .iter()
            .map(|card_id| PlayerAction::SelectCardToAccess { card_id: card_id.clone() })
            .collect(),
        AccessPhase::PendingInteractiveTrigger { card_id, .. } => vec![
            PlayerAction::PayToAvoidAccessTrigger { card_id: card_id.clone() },
            PlayerAction::DeclineAccessTrigger { card_id: card_id.clone() },
        ],
        AccessPhase::PendingChoice { card_id, can_trash, trash_cost, mandatory_steal, steal_cost } => {
            let mut candidates = Vec::new();
            if *mandatory_steal || steal_cost.is_some() {
                candidates.push(PlayerAction::StealAgenda { card_id: card_id.clone() });
            }
            if *can_trash && trash_cost.is_some() {
                candidates.push(PlayerAction::TrashAccessedCard { card_id: card_id.clone() });
            }
            if !*mandatory_steal {
                candidates.push(PlayerAction::PassAccessedCard { card_id: card_id.clone() });
            }
            candidates
        }
    }
}

/// Exact read: only the side actually holding priority can legally pass.
fn pass_priority_candidates(state: &GameState) -> Vec<PlayerAction> {
    match &state.paid_ability_window {
        Some(window) => vec![PlayerAction::PassPriority { side: window.active_priority }],
        None => Vec::new(),
    }
}

/// `ability::pay_cost`'s `Cost::Credits` arm draws Corp trace bids from
/// `recurring_credits` before the wallet, and Runner trace bids from an
/// active run's `bad_publicity_credits` before the wallet — bounding the
/// candidate range by wallet credits alone would miss legal higher bids.
fn trace_bid_candidates(state: &GameState) -> Vec<PlayerAction> {
    let Some(trace) = &state.active_trace else { return Vec::new() };
    match trace.corp_bid {
        None => {
            let max = state.corp.resources.credits.0 + state.corp.recurring_credits;
            (0..=max).map(|amount| PlayerAction::SubmitCorpTraceBid { amount }).collect()
        }
        Some(_) => {
            let bad_publicity_credits = state.active_run.as_ref().map_or(0, |r| r.bad_publicity_credits);
            let max = state.runner.resources.credits.0 + bad_publicity_credits;
            (0..=max).map(|amount| PlayerAction::SubmitRunnerTraceBid { amount }).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardRegistry;
    use crate::dsl::{AbilityDef, Card, Cost, Effect, IceType, SubroutineDef};
    use crate::rules::run::{AccessState, EncounteredSubroutine, RunIce, RunState};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, InstalledCard, InstalledRunnerCard, MemoryUnits,
        PaidAbilityWindow, PlayerResources, RunnerState, TraceResume, TraceState,
    };

    fn assert_same_actions(actual: &[PlayerAction], expected: &[PlayerAction]) {
        assert_eq!(actual.len(), expected.len(), "actual: {actual:?}\nexpected: {expected:?}");
        for action in expected {
            assert!(actual.contains(action), "missing {action:?} in {actual:?}");
        }
    }

    fn empty_corp() -> CorpState {
        CorpState {
            identity: None,
            bad_publicity: 0,
            first_install_used_this_turn: false,
            recurring_credits: 0,
            recurring_credits_max: 0,
            scored_agendas: Vec::new(),
            resources: PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) },
            hq: Vec::new(),
            r_and_d: Vec::new(),
            archives: Vec::new(),
            installed: Vec::new(),
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

    fn base_state() -> GameState {
        GameState {
            corp: empty_corp(),
            runner: empty_runner(),
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            seed: 0,
            rng_step: 0,
        }
    }

    fn corp_state(clicks: u32, credits: u32) -> GameState {
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(clicks);
        state.corp.resources.credits = Credits(credits);
        state
    }

    fn runner_state(clicks: u32, credits: u32) -> GameState {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(clicks);
        state.runner.resources.credits = Credits(credits);
        state
    }

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

    #[test]
    fn click_count_constraint() {
        let with_clicks = legal_actions(&corp_state(3, 5), &CardRegistry::new());
        assert!(with_clicks.contains(&PlayerAction::GainCreditClick { side: Side::Corp }));
        assert!(with_clicks.contains(&PlayerAction::EndTurn));

        let without_clicks = legal_actions(&corp_state(0, 5), &CardRegistry::new());
        assert!(!without_clicks.contains(&PlayerAction::GainCreditClick { side: Side::Corp }));
        assert!(without_clicks.contains(&PlayerAction::EndTurn));
    }

    #[test]
    fn credit_constraint_on_install_card() {
        let mut registry = CardRegistry::new();
        let mut ice = blank_card("ice_wall", CardType::Ice(IceType::Barrier));
        ice.cost = 1;
        registry.insert(ice);

        let mut poor = corp_state(3, 0);
        poor.corp.hq = vec![CardId("ice_wall".to_string())];
        let poor_legal = legal_actions(&poor, &registry);
        assert!(!poor_legal.iter().any(|a| matches!(a, PlayerAction::InstallCard { .. })));

        let mut rich = corp_state(3, 1);
        rich.corp.hq = vec![CardId("ice_wall".to_string())];
        let rich_legal = legal_actions(&rich, &registry);
        assert!(rich_legal.iter().any(|a| matches!(a, PlayerAction::InstallCard { .. })));
    }

    #[test]
    fn phase_specific_windows_exclude_the_other_sides_actions() {
        let registry = CardRegistry::new();

        let runner_legal = legal_actions(&runner_state(4, 10), &registry);
        for action in &runner_legal {
            assert!(
                !matches!(
                    action,
                    PlayerAction::PlayOperation { .. }
                        | PlayerAction::InstallCard { .. }
                        | PlayerAction::RezIce { .. }
                        | PlayerAction::AdvanceCard { .. }
                        | PlayerAction::ScoreAgenda { .. }
                        | PlayerAction::TrashResource { .. }
                ),
                "unexpected Corp-only action during the Runner's turn: {action:?}"
            );
        }
        assert!(runner_legal.contains(&PlayerAction::GainCreditClick { side: Side::Runner }));
        assert!(runner_legal.contains(&PlayerAction::DrawCardClick));
        assert!(runner_legal.contains(&PlayerAction::EndTurn));

        let corp_legal = legal_actions(&corp_state(3, 5), &registry);
        for action in &corp_legal {
            assert!(
                !matches!(action, PlayerAction::DrawCardClick | PlayerAction::InitiateRun { .. }),
                "unexpected Runner-only action during the Corp's turn: {action:?}"
            );
        }
    }

    #[test]
    fn initiate_run_absent_with_zero_clicks() {
        let legal = legal_actions(&runner_state(0, 10), &CardRegistry::new());
        assert!(!legal.iter().any(|a| matches!(a, PlayerAction::InitiateRun { .. })));
    }

    #[test]
    fn active_trace_blocks_everything_but_the_awaited_bid() {
        let mut awaiting_corp = corp_state(3, 5);
        awaiting_corp.active_trace = Some(TraceState {
            initiating_card: None,
            base_strength: 0,
            corp_bid: None,
            effect_on_success: Effect::GiveTags(1),
            resume: TraceResume::None,
        });
        let legal = legal_actions(&awaiting_corp, &CardRegistry::new());
        assert!(!legal.is_empty());
        assert!(legal.iter().all(|a| matches!(a, PlayerAction::SubmitCorpTraceBid { .. })));
        assert!(legal.contains(&PlayerAction::SubmitCorpTraceBid { amount: 0 }));
        assert!(legal.contains(&PlayerAction::SubmitCorpTraceBid { amount: 5 }));

        let mut awaiting_runner = corp_state(3, 5);
        awaiting_runner.runner.resources.credits = Credits(4);
        awaiting_runner.active_trace = Some(TraceState {
            initiating_card: None,
            base_strength: 0,
            corp_bid: Some(2),
            effect_on_success: Effect::GiveTags(1),
            resume: TraceResume::None,
        });
        let legal = legal_actions(&awaiting_runner, &CardRegistry::new());
        assert!(!legal.is_empty());
        assert!(legal.iter().all(|a| matches!(a, PlayerAction::SubmitRunnerTraceBid { .. })));
        assert!(legal.contains(&PlayerAction::SubmitRunnerTraceBid { amount: 4 }));
    }

    #[test]
    fn paid_ability_window_blocks_ordinary_actions_but_allows_window_legal_ones() {
        let mut registry = CardRegistry::new();
        let mut breaker = blank_card("corroder", CardType::Program);
        breaker.abilities = vec![AbilityDef {
            trigger: Trigger::Paid,
            cost: Some(Cost::Credits(1)),
            requirement: None,
            effect: Effect::BoostStrength { amount: 1, duration: crate::dsl::BoostDuration::Encounter },
        }];
        registry.insert(breaker);

        // Phase stays `Action(Runner)` throughout a run regardless of who
        // currently holds priority in the open window — matches
        // `GamePhase`'s doc comment ("`phase` never changes mid-run").
        let mut state = corp_state(3, 5);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.installed = vec![InstalledCard {
            card: CardId("unrezzed_asset".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 0,
        }];
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            encounter_strength_buff: 0,
            turn_strength_buff: 0,
        }];
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                card_id: CardId("wall_of_static".to_string()),
                current_strength: 3,
                ice_type: IceType::Barrier,
                subroutines: vec![EncounteredSubroutine {
                    id: 0,
                    definition: SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun },
                    status: SubroutineStatus::Pending,
                }],
                rezzed: true,
            }],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
            bad_publicity_credits: 0,
            additional_rd_access: 0,
            additional_hq_access: 0,
            access_replacement: None,
        });
        state.paid_ability_window =
            Some(PaidAbilityWindow { active_priority: Side::Runner, consecutive_passes: 0, return_phase: Box::new(state.phase) });

        let legal = legal_actions(&state, &registry);

        assert!(legal.contains(&PlayerAction::PassPriority { side: Side::Runner }));
        assert!(legal.contains(&PlayerAction::BreakSubroutine { ice_id: CardId("wall_of_static".to_string()), subroutine_index: 0 }));
        assert!(legal.contains(&PlayerAction::ActivateAbility { card_id: CardId("corroder".to_string()), ability_index: 0 }));
        assert!(!legal.iter().any(|a| matches!(
            a,
            PlayerAction::GainCreditClick { .. }
                | PlayerAction::InstallCard { .. }
                | PlayerAction::PlayOperation { .. }
                | PlayerAction::AdvanceCard { .. }
        )));
        // Not this side's priority: the Corp's RezIce is still window-exempt
        // (priority-independent, per `engine::rez_ice`), but the Runner's
        // ActivateAbility for a Corp-owned ability would not be — no Corp
        // abilities are registered here, so nothing to assert beyond the
        // priority-holder's own actions above.
    }

    #[test]
    fn mulligan_phase_offers_exactly_keep_or_mulligan() {
        let mut state = base_state();
        state.phase = GamePhase::Mulligan(Side::Corp);
        let legal = legal_actions(&state, &CardRegistry::new());
        assert_same_actions(&legal, &[PlayerAction::KeepHand, PlayerAction::TakeMulligan]);
    }

    #[test]
    fn discard_phase_offers_one_discard_per_hand_card() {
        let mut state = base_state();
        state.phase = GamePhase::Discard { side: Side::Runner, required: 1 };
        state.runner.grip = vec![CardId("a".to_string()), CardId("b".to_string())];
        let legal = legal_actions(&state, &CardRegistry::new());
        assert_same_actions(
            &legal,
            &[
                PlayerAction::DiscardCard { card_id: CardId("a".to_string()) },
                PlayerAction::DiscardCard { card_id: CardId("b".to_string()) },
            ],
        );
    }

    #[test]
    fn select_next_card_offers_one_per_selectable_card() {
        let mut state = runner_state(3, 5);
        state.active_run = Some(RunState {
            server: ServerId::Archives,
            phase: RunPhase::AccessingCard,
            ice: Vec::new(),
            position: 0,
            access_state: Some(AccessState {
                server: ServerId::Archives,
                unaccessed_cards: vec![CardId("a".to_string()), CardId("b".to_string())],
                resolved_cards: Vec::new(),
                phase: AccessPhase::SelectNextCard {
                    selectable_cards: vec![CardId("a".to_string()), CardId("b".to_string())],
                },
            }),
            jack_out_permitted: true,
            bad_publicity_credits: 0,
            additional_rd_access: 0,
            additional_hq_access: 0,
            access_replacement: None,
        });
        let legal = legal_actions(&state, &CardRegistry::new());
        // Basic clicks (`GainCreditClick`/`DrawCardClick`) are still legal
        // mid-run in this engine — neither handler checks `active_run`, and
        // no `PaidAbilityWindow` is open here (`SelectNextCard` isn't a
        // checkpoint, per `paid_ability::open_window_if_at_checkpoint`).
        assert_same_actions(
            &legal,
            &[
                PlayerAction::SelectCardToAccess { card_id: CardId("a".to_string()) },
                PlayerAction::SelectCardToAccess { card_id: CardId("b".to_string()) },
                PlayerAction::GainCreditClick { side: Side::Runner },
                PlayerAction::DrawCardClick,
            ],
        );
    }

    #[test]
    fn mandatory_steal_offers_only_steal() {
        let mut state = runner_state(3, 5);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::AccessingCard,
            ice: Vec::new(),
            position: 0,
            access_state: Some(AccessState {
                server: ServerId::Hq,
                unaccessed_cards: Vec::new(),
                resolved_cards: Vec::new(),
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("agenda".to_string()),
                    can_trash: false,
                    trash_cost: None,
                    mandatory_steal: true,
                    steal_cost: None,
                },
            }),
            jack_out_permitted: true,
            bad_publicity_credits: 0,
            additional_rd_access: 0,
            additional_hq_access: 0,
            access_replacement: None,
        });
        let legal = legal_actions(&state, &CardRegistry::new());
        // See the analogous comment in `select_next_card_offers_one_per_selectable_card`.
        assert_same_actions(
            &legal,
            &[
                PlayerAction::StealAgenda { card_id: CardId("agenda".to_string()) },
                PlayerAction::GainCreditClick { side: Side::Runner },
                PlayerAction::DrawCardClick,
            ],
        );
    }

    #[test]
    fn trashable_non_agenda_offers_trash_and_pass_not_steal() {
        let mut state = runner_state(3, 5);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::AccessingCard,
            ice: Vec::new(),
            position: 0,
            access_state: Some(AccessState {
                server: ServerId::Hq,
                unaccessed_cards: Vec::new(),
                resolved_cards: Vec::new(),
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("asset".to_string()),
                    can_trash: true,
                    trash_cost: Some(2),
                    mandatory_steal: false,
                    steal_cost: None,
                },
            }),
            jack_out_permitted: true,
            bad_publicity_credits: 0,
            additional_rd_access: 0,
            additional_hq_access: 0,
            access_replacement: None,
        });
        state.runner.resources.credits = Credits(5);
        let legal = legal_actions(&state, &CardRegistry::new());
        // See the analogous comment in `select_next_card_offers_one_per_selectable_card`.
        assert_same_actions(
            &legal,
            &[
                PlayerAction::TrashAccessedCard { card_id: CardId("asset".to_string()) },
                PlayerAction::PassAccessedCard { card_id: CardId("asset".to_string()) },
                PlayerAction::GainCreditClick { side: Side::Runner },
                PlayerAction::DrawCardClick,
            ],
        );
    }

    #[test]
    fn game_over_offers_nothing() {
        let mut state = base_state();
        state.phase = GamePhase::GameOver(Side::Corp);
        assert!(legal_actions(&state, &CardRegistry::new()).is_empty());
    }

    #[test]
    fn start_of_turn_offers_nothing() {
        // Never externally observable in practice (`enter_start_of_turn`
        // collapses it synchronously before returning), but a hand-built
        // state exercising it should still be a dead end, not a panic.
        let mut state = base_state();
        state.phase = GamePhase::StartOfTurn(Side::Corp);
        assert!(legal_actions(&state, &CardRegistry::new()).is_empty());
    }
}
