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

/// Whose decision is actually pending right now, if anyone's. `GameState::
/// phase` alone isn't enough mid-run: `phase` stays `Action(Runner)`
/// throughout a run even while a `PaidAbilityWindow` briefly hands priority
/// to the Corp (to rez ICE, say), or while a `TraceState` awaits the Corp's
/// bid. Precedence:
///
/// 1. An active trace awaits a bid — Corp first, then Runner.
/// 2. A parked `PendingPaidChoice` awaits its payer.
/// 3. A parked `PendingDecision` awaits its chooser.
/// 4. An open paid ability window holds priority for one side.
/// 5. Otherwise it's whichever side `GamePhase` names directly.
/// 6. `StartOfTurn`/`GameOver` — no player decision is pending.
///
/// Steps 1–3 must stay in exactly this order and ahead of step 4, because
/// they mirror `engine::apply_action`'s blocking guards: each of those
/// parked states rejects every action but its own resolution, so naming any
/// other side here produces a player who has no legal action at all. That
/// specifically bites when a decision is parked *while a window is open* —
/// e.g. activating a run-granting ability during a `StartOfTurn` window
/// leaves the window holding priority for the Corp while only the Runner
/// can resolve the parked choice. Found by
/// `no_panics_or_deadlocks_across_many_seeds_system_gateway`.
pub fn current_actor(state: &GameState) -> Option<Side> {
    if let Some(trace) = &state.active_trace {
        return Some(if trace.corp_bid.is_none() { Side::Corp } else { Side::Runner });
    }
    if let Some(side) = state.pending_paid_choice.as_ref().map(|choice| choice.side) {
        return Some(side);
    }
    if let Some(side) = crate::rules::pending_choice::pending_decision_chooser(state) {
        return Some(side);
    }
    if let Some(window) = &state.paid_ability_window {
        return Some(window.active_priority);
    }
    match state.phase {
        GamePhase::Mulligan(side) | GamePhase::Discard { side, .. } | GamePhase::Action(side) => Some(side),
        GamePhase::StartOfTurn(_) | GamePhase::GameOver(_) => None,
    }
}

/// `legal_actions(state, registry)`, filtered to only the actions `side` is
/// actually entitled to submit — the per-viewer slice a `ClientView`'s
/// `legal_actions` field needs. Every `PlayerAction` variant is structurally
/// single-side by the engine's own documented convention (see `action.rs`'s
/// doc comments) except the handful `action_owner` resolves explicitly.
///
/// This is deliberately *not* "gate everything by `current_actor`" — that
/// would be wrong in both directions here: `RezIce` is priority-independent
/// (legal for the Corp during `ApproachIce` regardless of whose priority
/// the open window currently holds), so a Runner-priority window would
/// wrongly exclude it from the Corp's list and wrongly include it in the
/// Runner's if filtering only looked at `current_actor`.
pub fn legal_actions_for(state: &GameState, registry: &CardRegistry, side: Side) -> Vec<PlayerAction> {
    legal_actions(state, registry).into_iter().filter(|action| action_owner(state, action) == side).collect()
}

/// Which side may submit `action` against `state`. `action` is assumed to
/// already be a member of `legal_actions(state, registry)` — i.e. it's
/// already known to be legal for *someone*; this only resolves *who*.
fn action_owner(state: &GameState, action: &PlayerAction) -> Side {
    match action {
        PlayerAction::GainCreditClick { side } | PlayerAction::PassPriority { side } => *side,

        PlayerAction::DrawCardClick
        | PlayerAction::InitiateRun { .. }
        | PlayerAction::ContinueRun
        | PlayerAction::JackOut
        | PlayerAction::CompleteRun
        | PlayerAction::PlayEvent { .. }
        | PlayerAction::InstallHardware { .. }
        | PlayerAction::InstallProgram { .. }
        | PlayerAction::InstallResource { .. }
        | PlayerAction::InstallProgramOnIce { .. }
        | PlayerAction::BreakSubroutine { .. }
        | PlayerAction::BreakSubroutineWithClick { .. }
        | PlayerAction::RemoveTag
        | PlayerAction::SelectCardToAccess { .. }
        | PlayerAction::StealAgenda { .. }
        | PlayerAction::TrashAccessedCard { .. }
        | PlayerAction::PassAccessedCard { .. }
        | PlayerAction::PayToAvoidAccessTrigger { .. }
        | PlayerAction::DeclineAccessTrigger { .. }
        | PlayerAction::SubmitRunnerTraceBid { .. } => Side::Runner,

        PlayerAction::InstallCard { .. }
        | PlayerAction::RezIce { .. }
        | PlayerAction::PlayOperation { .. }
        | PlayerAction::AdvanceCard { .. }
        | PlayerAction::ScoreAgenda { .. }
        | PlayerAction::TrashResource { .. }
        | PlayerAction::PurgeVirusCounters
        | PlayerAction::SubmitCorpTraceBid { .. } => Side::Corp,

        // Symmetric, but only ever legal when `phase` names exactly one
        // side — this action already passed the `legal_actions` probe, so
        // `phase` is guaranteed to match one of these arms.
        PlayerAction::EndTurn | PlayerAction::DiscardCard { .. } | PlayerAction::KeepHand | PlayerAction::TakeMulligan => {
            match state.phase {
                GamePhase::Action(side) | GamePhase::Discard { side, .. } | GamePhase::Mulligan(side) => side,
                GamePhase::StartOfTurn(side) | GamePhase::GameOver(side) => side,
            }
        }

        // Symmetric *and* can fire off-priority mid-window (a Corp ability
        // during the Runner's own priority, or vice versa) — `phase`/
        // `current_actor` can't resolve this; ownership is a card-location
        // lookup instead.
        PlayerAction::ActivateAbility { card_id, .. } => {
            if state.corp.installed.iter().any(|c| c.card == *card_id) {
                Side::Corp
            } else if state.runner.rig.iter().any(|c| c.card == *card_id) {
                Side::Runner
            } else {
                unreachable!("ActivateAbility({card_id:?}) passed legal_actions but owns no matching installed/rig card")
            }
        }

        PlayerAction::AcceptPendingPaidChoice { .. } | PlayerAction::DeclinePendingPaidChoice => state
            .pending_paid_choice
            .as_ref()
            .map(|p| p.side)
            .unwrap_or_else(|| unreachable!("{action:?} passed legal_actions but no PendingPaidChoice is parked")),

        PlayerAction::ResolvePendingChoice { .. }
        | PlayerAction::ToggleCardSelection { .. }
        | PlayerAction::ConfirmCardSelection
        | PlayerAction::ChooseServerForPendingDecision { .. }
        | PlayerAction::ChooseTriggerToResolve { .. } => {
            crate::rules::pending_choice::pending_decision_chooser(state)
                .unwrap_or_else(|| unreachable!("{action:?} passed legal_actions but no PendingDecision is parked"))
        }
    }
}

fn candidate_actions(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let mut candidates = static_candidates();
    candidates.extend(install_card_candidates(state, registry));
    candidates.extend(rez_ice_candidates(state));
    candidates.extend(initiate_run_candidates(state));
    candidates.extend(play_card_candidates(state, registry));
    candidates.extend(install_program_on_ice_candidates(state, registry));
    candidates.extend(break_subroutine_candidates(state));
    candidates.extend(break_subroutine_with_click_candidates(state, registry));
    candidates.extend(discard_candidates(state));
    candidates.extend(activate_ability_candidates(state, registry));
    candidates.extend(advance_score_trash_candidates(state, registry));
    candidates.extend(access_flow_candidates(state));
    candidates.extend(pass_priority_candidates(state));
    candidates.extend(trace_bid_candidates(state));
    candidates.extend(pending_paid_choice_candidates(state));
    candidates.extend(pending_decision_candidates(state, registry));
    candidates
}

/// `AcceptPendingPaidChoice`'s `cost_option_index` only matters when the
/// pending cost is `Cost::AnyOf`; every other cost shape ignores it, so a
/// single `None` candidate (plus one per `AnyOf` alternative) fully covers
/// what's actually distinguishable.
fn pending_paid_choice_candidates(state: &GameState) -> Vec<PlayerAction> {
    let Some(pending) = &state.pending_paid_choice else { return Vec::new() };
    let mut candidates = vec![PlayerAction::DeclinePendingPaidChoice];
    match &pending.cost {
        crate::dsl::Cost::AnyOf(options) => {
            candidates.extend(
                (0..options.len()).map(|i| PlayerAction::AcceptPendingPaidChoice { cost_option_index: Some(i) }),
            );
        }
        _ => candidates.push(PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }),
    }
    candidates
}

fn pending_decision_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    match &state.pending_decision {
        None => Vec::new(),
        Some(crate::rules::state::PendingDecision::ChooseEffect { options, .. }) => {
            (0..options.len()).map(|i| PlayerAction::ResolvePendingChoice { option_index: i }).collect()
        }
        Some(crate::rules::state::PendingDecision::ChooseCards { side, source, filter, .. }) => {
            let mut candidates: Vec<PlayerAction> =
                crate::rules::pending_choice::eligible_cards(state, registry, *side, source, filter)
                    .into_iter()
                    .map(|card_id| PlayerAction::ToggleCardSelection { card_id })
                    .collect();
            candidates.push(PlayerAction::ConfirmCardSelection);
            candidates
        }
        Some(crate::rules::state::PendingDecision::ChooseServer { allowed_servers, .. }) => {
            let existing = existing_remote_ids(state);
            let mut zones = vec![ServerId::Hq, ServerId::RnD, ServerId::Archives];
            zones.extend(existing.iter().copied().map(ServerId::Remote));
            zones.push(ServerId::Remote(fresh_remote_id(&existing)));
            // e.g. Jailbreak's "Run HQ or R&D" — mirrors the same check in
            // `pending_choice::resolve_choose_server`.
            if let Some(allowed) = allowed_servers {
                zones.retain(|server| allowed.contains(server));
            }
            zones.into_iter().map(|server| PlayerAction::ChooseServerForPendingDecision { server }).collect()
        }
        // Every still-unresolved trigger is a legal next pick — the choice
        // is purely which order they resolve in, so none can be illegal.
        Some(crate::rules::state::PendingDecision::ChooseTriggerOrder { pending, .. }) => {
            pending.iter().map(|due| PlayerAction::ChooseTriggerToResolve { card_id: due.card.clone() }).collect()
        }
    }
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
        // Always a candidate: purging an empty board is legal (just
        // pointless), so the only thing that filters this out is the
        // `apply_action` probe rejecting it on phase/clicks/window.
        PlayerAction::PurgeVirusCounters,
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
            // Upgrades protect a server's root the same way an Asset does,
            // but — unlike Asset/Agenda — may root on a central server too
            // (e.g. Manegarm Skunkworks/Anoetic Void, System Gateway), so
            // they share Ice's full zone list rather than Asset's remote-only one.
            CardType::Upgrade => {
                let mut zones = vec![ServerId::Hq, ServerId::RnD, ServerId::Archives];
                zones.extend(remote_zones.iter().copied());
                for zone in zones {
                    candidates.push(PlayerAction::InstallCard { card_id: card_id.clone(), zone, slot: InstallSlot::Root });
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

/// `PlayEvent`/`InstallHardware`/`InstallProgram`/`InstallResource` (Runner
/// grip) and `PlayOperation` (Corp HQ), keyed off each card's registry
/// `CardType`.
fn play_card_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let mut candidates = Vec::new();
    for card_id in &state.runner.grip {
        let Some(card) = registry.get(card_id) else { continue };
        match card.card_type {
            CardType::Event => candidates.push(PlayerAction::PlayEvent { card_id: card_id.clone() }),
            CardType::Hardware => candidates.push(PlayerAction::InstallHardware { card_id: card_id.clone() }),
            // No data-driven memory-unit stat exists on `CardDefinition` (see
            // `CardDefinition::strength`'s doc comment) — `memory_cost` is entirely
            // caller-chosen, so this only ever offers 0.
            //
            // A Trojan (`installs_on_ice: true`, e.g. Botulus) can't be
            // installed via the ordinary Rig-install flow at all — see
            // `install_program_on_ice_candidates` instead.
            CardType::Program if !card.installs_on_ice => {
                candidates.push(PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 0 })
            }
            CardType::Resource => candidates.push(PlayerAction::InstallResource { card_id: card_id.clone() }),
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

/// `PlayerAction::InstallProgramOnIce` — every Trojan Program in the
/// Runner's grip (`installs_on_ice: true`), paired with every Corp
/// installed ICE (rezzed or not — real rules allow hosting on unrezzed
/// ICE), mirroring `install_card_candidates`'s "every hand card × every
/// matching zone" shape.
fn install_program_on_ice_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let host_ice: Vec<CardId> = state
        .corp
        .installed
        .iter()
        .filter(|c| c.slot == InstallSlot::Ice)
        .map(|c| c.card.clone())
        .collect();
    let mut candidates = Vec::new();
    for card_id in &state.runner.grip {
        let Some(card) = registry.get(card_id) else { continue };
        if card.card_type == CardType::Program && card.installs_on_ice {
            for host_ice_id in &host_ice {
                candidates.push(PlayerAction::InstallProgramOnIce {
                    card_id: card_id.clone(),
                    host_ice_id: host_ice_id.clone(),
                    memory_cost: 0,
                });
            }
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

/// Same shape as `break_subroutine_candidates`, gated additionally on the
/// currently-encountered ICE being `click_breakable` and the Runner having
/// at least 1 click left to spend — e.g. Ansel 1.0, Brân 1.0.
fn break_subroutine_with_click_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let Some(run) = &state.active_run else { return Vec::new() };
    if run.phase != RunPhase::EncounterIce {
        return Vec::new();
    }
    if state.runner.resources.clicks.0 < 1 {
        return Vec::new();
    }
    let Some(ice) = run.ice.get(run.position) else { return Vec::new() };
    if !registry.get(&ice.card_id).is_some_and(|c| c.click_breakable) {
        return Vec::new();
    }
    ice.subroutines
        .iter()
        .filter(|s| s.status == SubroutineStatus::Pending)
        .map(|s| PlayerAction::BreakSubroutineWithClick { ice_id: ice.card_id.clone(), subroutine_index: s.id })
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
        // Luminal Transubstantiation's lockout — mirrors the same guard in
        // `engine::score_agenda` so the mask never offers an action the
        // engine would reject.
        if card.card_type == CardType::Agenda && !state.corp.cannot_score_agendas_this_turn {
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
            // `Effect::PreventStealAndTrashForRemainderOfRun` (e.g. Ansel
            // 1.0) blocks both actions outright for the rest of this run —
            // kept out of the mask entirely, matching `resolve_steal`/
            // `resolve_trash`'s own hard error, rather than offering an
            // action that would just fail.
            let steal_and_trash_blocked = run.runner_cannot_steal_or_trash;
            if !steal_and_trash_blocked && (*mandatory_steal || steal_cost.is_some()) {
                candidates.push(PlayerAction::StealAgenda { card_id: card_id.clone() });
            }
            if !steal_and_trash_blocked && *can_trash && trash_cost.is_some() {
                candidates.push(PlayerAction::TrashAccessedCard { card_id: card_id.clone() });
            }
            // Also offered when a mandatory steal was itself blocked above —
            // otherwise this access point would have no legal action at all.
            if !*mandatory_steal || steal_and_trash_blocked {
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
    use crate::dsl::{AbilityDef, CardDefinition, Cost, Effect, IceType, SubroutineDef};
    use crate::rules::run::{AccessState, EncounteredSubroutine, RunIce, RunState};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, InstalledCard, InstalledRunnerCard, MemoryUnits,
        PaidAbilityWindow, PlayerResources, RunnerState, TraceResume, TraceState, WindowCheckpoint,
    };

    fn assert_same_actions(actual: &[PlayerAction], expected: &[PlayerAction]) {
        assert_eq!(actual.len(), expected.len(), "actual: {actual:?}\nexpected: {expected:?}");
        for action in expected {
            assert!(actual.contains(action), "missing {action:?} in {actual:?}");
        }
    }

    fn empty_corp() -> CorpState {
        CorpState {
            resources: PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) },
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

    fn base_state() -> GameState {
        GameState {
            corp: empty_corp(),
            runner: empty_runner(),
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            pending_prevention: None, pending_paid_choice: None, pending_decision: None, last_discarded_cards: Vec::new(), last_completed_run: None, last_advancement_was_first: false, deferred_triggers: Vec::new(),
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
    fn install_card_candidates_offers_upgrade_into_every_zone_including_centrals() {
        let mut registry = CardRegistry::new();
        let mut upgrade = blank_card("manegarm_skunkworks", CardType::Upgrade);
        upgrade.cost = 0;
        registry.insert(upgrade);

        let mut state = corp_state(3, 5);
        state.corp.hq = vec![CardId("manegarm_skunkworks".to_string())];
        // One existing remote so both "existing remote" and "fresh remote"
        // zones are exercised, mirroring Ice's own zone list exactly.
        state.corp.installed = vec![InstalledCard {
            card: CardId("ice_wall".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];

        let legal = legal_actions(&state, &registry);
        for zone in [ServerId::Hq, ServerId::RnD, ServerId::Archives, ServerId::Remote(0), ServerId::Remote(1)] {
            assert!(
                legal.contains(&PlayerAction::InstallCard {
                    card_id: CardId("manegarm_skunkworks".to_string()),
                    zone,
                    slot: InstallSlot::Root,
                }),
                "expected Upgrade to be installable into {zone:?}"
            );
        }
        // Never offered into `InstallSlot::Ice` — that's Ice's slot, not Upgrade's.
        assert!(!legal.iter().any(|a| matches!(
            a,
            PlayerAction::InstallCard { card_id, slot: InstallSlot::Ice, .. }
                if *card_id == CardId("manegarm_skunkworks".to_string())
        )));
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
            cost_discount_if: None,
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
            ..Default::default()
        }];
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];
        state.active_run = Some(RunState {
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
            ..Default::default()
        });
        state.paid_ability_window =
            Some(PaidAbilityWindow { active_priority: Side::Runner, consecutive_passes: 0, return_phase: Box::new(state.phase), checkpoint: WindowCheckpoint::Run });

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
            access_state: Some(AccessState {
                server: ServerId::Archives,
                unaccessed_cards: vec![CardId("a".to_string()), CardId("b".to_string())],
                phase: AccessPhase::SelectNextCard {
                    selectable_cards: vec![CardId("a".to_string()), CardId("b".to_string())],
                },
                ..Default::default()
            }),
            jack_out_permitted: true,
            ..Default::default()
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
            phase: RunPhase::AccessingCard,
            access_state: Some(AccessState {
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("agenda".to_string()),
                    can_trash: false,
                    trash_cost: None,
                    mandatory_steal: true,
                    steal_cost: None,
                },
                ..Default::default()
            }),
            jack_out_permitted: true,
            ..Default::default()
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
            phase: RunPhase::AccessingCard,
            access_state: Some(AccessState {
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("asset".to_string()),
                    can_trash: true,
                    trash_cost: Some(2),
                    mandatory_steal: false,
                    steal_cost: None,
                },
                ..Default::default()
            }),
            jack_out_permitted: true,
            ..Default::default()
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
        // `enter_start_of_turn` always opens a `WindowCheckpoint::StartOfTurn`
        // window before returning (see `start_of_turn_window_offers_only_pass_priority`
        // for that reachable case) — a *bare* `StartOfTurn` phase with no
        // window at all is a hand-built edge case only, but should still be
        // a dead end, not a panic.
        let mut state = base_state();
        state.phase = GamePhase::StartOfTurn(Side::Corp);
        assert!(legal_actions(&state, &CardRegistry::new()).is_empty());
    }

    #[test]
    fn start_of_turn_window_offers_only_pass_priority() {
        let mut state = base_state();
        state.phase = GamePhase::StartOfTurn(Side::Corp);
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::StartOfTurn { side: Side::Corp },
            return_phase: Box::new(state.phase),
        });

        assert_eq!(legal_actions(&state, &CardRegistry::new()), vec![PlayerAction::PassPriority { side: Side::Corp }]);
    }

    #[test]
    fn current_actor_prefers_active_trace_over_window_and_phase() {
        let mut state = corp_state(3, 5);
        state.phase = GamePhase::Action(Side::Runner);
        state.paid_ability_window =
            Some(PaidAbilityWindow { active_priority: Side::Runner, consecutive_passes: 0, return_phase: Box::new(state.phase), checkpoint: WindowCheckpoint::Run });
        state.active_trace = Some(TraceState {
            initiating_card: None,
            base_strength: 0,
            corp_bid: None,
            effect_on_success: Effect::GiveTags(1),
            resume: TraceResume::None,
        });
        assert_eq!(current_actor(&state), Some(Side::Corp));

        state.active_trace.as_mut().unwrap().corp_bid = Some(2);
        assert_eq!(current_actor(&state), Some(Side::Runner));
    }

    #[test]
    fn current_actor_prefers_paid_ability_window_over_phase() {
        let mut state = corp_state(3, 5);
        state.phase = GamePhase::Action(Side::Runner);
        state.paid_ability_window =
            Some(PaidAbilityWindow { active_priority: Side::Corp, consecutive_passes: 0, return_phase: Box::new(state.phase), checkpoint: WindowCheckpoint::Run });
        assert_eq!(current_actor(&state), Some(Side::Corp));
    }

    #[test]
    fn current_actor_falls_back_to_phase_and_is_none_for_start_of_turn_and_game_over() {
        let mut state = corp_state(3, 5);
        assert_eq!(current_actor(&state), Some(Side::Corp));

        state.phase = GamePhase::Discard { side: Side::Runner, required: 1 };
        assert_eq!(current_actor(&state), Some(Side::Runner));

        state.phase = GamePhase::StartOfTurn(Side::Corp);
        assert_eq!(current_actor(&state), None);

        state.phase = GamePhase::GameOver(Side::Runner);
        assert_eq!(current_actor(&state), None);
    }

    /// Priority-independent `RezIce` is legal for the Corp even while the
    /// Runner holds priority in an open window — `legal_actions_for` must
    /// keep it out of the Runner's slice and put it in the Corp's, which a
    /// naive "gate by `current_actor`" filter would get backwards. Mirrors
    /// `engine::tests::rez_ice_by_non_priority_side_during_window_still_succeeds_and_resets_passes`.
    #[test]
    fn legal_actions_for_gives_priority_independent_rez_ice_to_the_owning_side_not_the_priority_holder() {
        let mut state = runner_state(3, 5);
        state.corp.installed = vec![InstalledCard {
            card: CardId("ice_wall".to_string()),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce {
                card_id: CardId("ice_wall".to_string()),
                current_strength: 1,
                ice_type: IceType::Barrier,
                subroutines: Vec::new(),
                rezzed: false,
            }],
            ..Default::default()
        });
        state.paid_ability_window =
            Some(PaidAbilityWindow { active_priority: Side::Runner, consecutive_passes: 0, return_phase: Box::new(state.phase), checkpoint: WindowCheckpoint::Run });

        let registry = CardRegistry::from_cards(vec![blank_card("ice_wall", CardType::Ice(IceType::Barrier))]);
        let rez = PlayerAction::RezIce { ice_id: CardId("ice_wall".to_string()) };
        assert!(legal_actions(&state, &registry).contains(&rez));

        assert!(legal_actions_for(&state, &registry, Side::Corp).contains(&rez));
        assert!(!legal_actions_for(&state, &registry, Side::Runner).contains(&rez));

        // The Runner still gets their own priority-holder action.
        assert!(legal_actions_for(&state, &registry, Side::Runner).contains(&PlayerAction::PassPriority { side: Side::Runner }));
    }

    #[test]
    fn legal_actions_for_activate_ability_resolves_ownership_by_card_location() {
        let mut registry = CardRegistry::new();
        let mut breaker = blank_card("corroder", CardType::Program);
        // A plain credit gain rather than a strength boost: this test is
        // about `action_owner` resolving ownership from card *location*,
        // and an icebreaker's real abilities are gated to encounters
        // (`EffectRequirement::DuringEncounter`), which would make the
        // action illegal here for reasons unrelated to what is asserted.
        breaker.abilities = vec![AbilityDef {
            trigger: Trigger::Paid,
            cost: Some(Cost::Credits(1)),
            requirement: None,
            effect: Effect::GainCredits(Side::Runner, 1),
            cost_discount_if: None,
        }];
        registry.insert(breaker);

        let mut state = runner_state(3, 5);
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];

        let activate = PlayerAction::ActivateAbility { card_id: CardId("corroder".to_string()), ability_index: 0 };
        assert!(legal_actions_for(&state, &registry, Side::Runner).contains(&activate));
        assert!(!legal_actions_for(&state, &registry, Side::Corp).contains(&activate));
    }

    #[test]
    fn legal_actions_for_splits_symmetric_click_actions_by_phase() {
        let corp = corp_state(3, 5);
        let registry = CardRegistry::new();
        assert!(legal_actions_for(&corp, &registry, Side::Corp).contains(&PlayerAction::GainCreditClick { side: Side::Corp }));
        assert!(!legal_actions_for(&corp, &registry, Side::Runner).contains(&PlayerAction::GainCreditClick { side: Side::Corp }));
        assert!(legal_actions_for(&corp, &registry, Side::Corp).contains(&PlayerAction::EndTurn));
        assert!(!legal_actions_for(&corp, &registry, Side::Runner).contains(&PlayerAction::EndTurn));
    }
}
