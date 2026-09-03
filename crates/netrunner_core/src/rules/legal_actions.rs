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
//! checks, hand-size limits — with zero duplicated rules logic. The type
//! dispatch in `install_card_candidates`/`play_card_candidates` decides
//! only *which action to propose* for a card; whether the card fits the
//! action is checked again by the handler (`engine::install_card` and
//! friends), so a remote client that skips this module gets the same
//! answer. This module used to be the only place that rule lived.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardType, Trigger};
use crate::rules::action::PlayerAction;
use crate::rules::apply_action;
use crate::rules::run::{AccessPhase, RunPhase, ServerId, SubroutineStatus};
use crate::rules::state::{GamePhase, GameState, InstallId, InstallSlot, Side};

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
/// 5. A parked interactive access trigger awaits its `decider`.
/// 6. Otherwise it's whichever side `GamePhase` names directly.
/// 7. `StartOfTurn`/`GameOver` — no player decision is pending.
///
/// Step 5 sits *after* the window rather than with the other parked states,
/// because a paid-ability window legitimately opens at
/// `AccessPhase::PendingInteractiveTrigger` and `engine::pay_access_trigger`
/// refuses to resolve while one is open — so the window really does hold
/// priority first. It sits *before* the phase because `GamePhase` is
/// `Action(Runner)` throughout a run, which would name the Runner even when
/// the card asks the Corp to pay (Snare!).
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
    if let Some(side) = pending_access_trigger_decider(state) {
        return Some(side);
    }
    match state.phase {
        GamePhase::Mulligan(side) | GamePhase::Discard { side, .. } | GamePhase::Action(side) => Some(side),
        GamePhase::StartOfTurn(_) | GamePhase::GameOver(_) => None,
    }
}

/// The side owing a decision on a parked `AccessPhase::
/// PendingInteractiveTrigger`, if a run is parked on one.
fn pending_access_trigger_decider(state: &GameState) -> Option<Side> {
    match &state.active_run.as_ref()?.access_state.as_ref()?.phase {
        AccessPhase::PendingInteractiveTrigger { decider, .. } => Some(*decider),
        _ => None,
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
        PlayerAction::GainCreditClick { side }
        | PlayerAction::DrawCardClick { side }
        | PlayerAction::PassPriority { side } => *side,

        PlayerAction::InitiateRun { .. }
        | PlayerAction::ContinueRun
        | PlayerAction::JackOut
        | PlayerAction::CompleteRun
        | PlayerAction::PlayEvent { .. }
        | PlayerAction::InstallHardware { .. }
        | PlayerAction::InstallProgram { .. }
        | PlayerAction::InstallResource { .. }
        | PlayerAction::InstallProgramOnIce { .. }
        | PlayerAction::BreakSubroutineWithClick { .. }
        | PlayerAction::RemoveTag
        | PlayerAction::SelectCardToAccess { .. }
        | PlayerAction::StealAgenda { .. }
        | PlayerAction::TrashAccessedCard { .. }
        | PlayerAction::PassAccessedCard { .. }
        | PlayerAction::SubmitRunnerTraceBid { .. } => Side::Runner,

        // The only access actions that are not structurally the Runner's:
        // whose decision this is depends on the card (Fetal AI asks the
        // Runner, Snare! asks the Corp), so it is read from the parked
        // state rather than assumed. Falls back to the Runner if nothing is
        // parked, which cannot happen for an action already known legal.
        PlayerAction::PayAccessTrigger { .. } | PlayerAction::DeclineAccessTrigger { .. } => {
            pending_access_trigger_decider(state).unwrap_or(Side::Runner)
        }

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
                GamePhase::Action(side)
                | GamePhase::Discard { side, .. }
                | GamePhase::Mulligan(side)
                | GamePhase::StartOfTurn(side) => side,
                // `GameOver`'s payload is the *winner*, not a side to act;
                // it used to share the arm above and be read as one. Moot,
                // since `apply_action` rejects every action once the game
                // is over and this is only called on actions it accepted.
                GamePhase::GameOver(_) => unreachable!("no action passes the legal_actions probe once the game is over"),
            }
        }

        // Symmetric *and* can fire off-priority mid-window (a Corp ability
        // during the Runner's own priority, or vice versa) — `phase`/
        // `current_actor` can't resolve this; ownership is a card-location
        // lookup instead.
        PlayerAction::ActivateAbility { target, .. } => {
            // The score area counts as the Corp's too — Proprionegation's
            // ability is used from there, and it is the one Corp card an
            // installed-only lookup could not place.
            if *target == InstallId::CORP_IDENTITY
                || state.find_corp_install(*target).is_some()
                || state.corp.find_scored(*target).is_some()
            {
                Side::Corp
            } else if *target == InstallId::RUNNER_IDENTITY || state.find_rig_install(*target).is_some() {
                Side::Runner
            } else {
                unreachable!("ActivateAbility({target:?}) passed legal_actions but owns no matching installed/rig card")
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
        Some(crate::rules::state::PendingDecision::ChooseCards { side, source, filter, source_install, .. }) => {
            let mut candidates: Vec<PlayerAction> =
                crate::rules::pending_choice::eligible_positions(state, registry, *side, source, filter, *source_install)
                    .into_iter()
                    .map(|position| PlayerAction::ToggleCardSelection { position })
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
            (0..pending.len()).map(|index| PlayerAction::ChooseTriggerToResolve { index }).collect()
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
        PlayerAction::DrawCardClick { side: Side::Corp },
        PlayerAction::DrawCardClick { side: Side::Runner },
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
pub(crate) fn existing_remote_ids(state: &GameState) -> Vec<u32> {
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

/// The smallest remote id not currently in use — offers "install into a
/// new remote" without the engine having any such concept itself (see
/// `ServerId`'s doc comment: `Remote(n)` for any `n` is accepted
/// unconditionally, nothing tracks which ids are "in use").
///
/// Smallest unused rather than `max + 1`: an emptied remote is a remote
/// that no longer exists, and the next new server may take its number.
/// `max + 1` ratcheted ids upward all game, and `ActionSpace` indexes
/// only `MAX_REMOTE_SERVERS` of them — past that, installs into and runs
/// on the new remote were legal but invisible to the RL mask.
pub(crate) fn fresh_remote_id(existing: &[u32]) -> u32 {
    (0..).find(|id| !existing.contains(id)).expect("the naturals are unbounded")
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
            // Spelled out rather than `_ => {}` so a new `CardType` is a
            // compile error here instead of a card class that silently
            // never gets offered — which is the shape the memory-cost bug
            // had (`play_card_candidates`). Operations are played, not
            // installed; the Runner types cannot be in HQ.
            CardType::Operation
            | CardType::Identity
            | CardType::Event
            | CardType::Hardware
            | CardType::Program
            | CardType::Resource => {}
        }
    }
    candidates
}

/// `rez_ice`'s name is misleading — it rezzes any unrezzed Corp install,
/// not just `InstallSlot::Ice` ones. *When* each kind may be rezzed (ICE
/// only while approached; assets and upgrades at any priority) is the
/// handler's rule, and the probe applies it.
fn rez_ice_candidates(state: &GameState) -> Vec<PlayerAction> {
    state
        .corp
        .installed
        .iter()
        .filter(|c| !c.rezzed)
        .map(|c| PlayerAction::RezIce { ice: c.install_id })
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
    // The grip plus any card hosted "as if it were in your grip" (Bling).
    for card_id in &state.runner.playable_hand() {
        let Some(card) = registry.get(card_id) else { continue };
        match card.card_type {
            CardType::Event => candidates.push(PlayerAction::PlayEvent { card_id: card_id.clone() }),
            CardType::Hardware => candidates.push(PlayerAction::InstallHardware { card_id: card_id.clone() }),
            // A Trojan (`installs_on_ice: true`, e.g. Botulus) can't be
            // installed via the ordinary Rig-install flow at all — see
            // `install_program_on_ice_candidates` instead. This guard only
            // picks which action to propose; `engine::install_program`
            // refuses a trojan itself (`TrojanMustBeHostedOnIce`).
            //
            // How much memory this reserves is not named here, and that is
            // the whole point: this used to offer `memory_cost: 0` against a
            // handler that demanded the registry's declared value, so every
            // program with a cost — which is all of them — was rejected by
            // the `apply_action` probe below and never offered at all. The
            // comment that stood here justified the `0` by saying no
            // memory-unit stat existed on `CardDefinition`. That was true
            // when written and stopped being true when the field was added.
            CardType::Program if !card.installs_on_ice => {
                candidates.push(PlayerAction::InstallProgram { card_id: card_id.clone() })
            }
            CardType::Resource => candidates.push(PlayerAction::InstallResource { card_id: card_id.clone() }),
            // A trojan (`installs_on_ice`) — offered by
            // `install_program_on_ice_candidates` instead.
            CardType::Program => {}
            // Corp types cannot be in the grip. Listed, not `_`, so a new
            // `CardType` must decide here whether it is playable from hand.
            CardType::Agenda
            | CardType::Asset
            | CardType::Operation
            | CardType::Ice(_)
            | CardType::Identity
            | CardType::Upgrade => {}
        }
    }
    // HQ plus any card playable from Archives (Petty Cash).
    for card_id in &state.corp.playable_hand() {
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
    // Hosts are collected as `InstallId`s, never `CardId`s. Unrezzed ICE is
    // a legal host, so a `CardId` here put the identity of a card the
    // Runner's own `ClientView` masks straight into their `legal_actions`.
    let host_ice: Vec<InstallId> = state
        .corp
        .installed
        .iter()
        .filter(|c| c.slot == InstallSlot::Ice)
        .map(|c| c.install_id)
        .collect();
    let mut candidates = Vec::new();
    for card_id in &state.runner.playable_hand() {
        let Some(card) = registry.get(card_id) else { continue };
        if card.card_type == CardType::Program && card.installs_on_ice {
            for host in &host_ice {
                candidates.push(PlayerAction::InstallProgramOnIce {
                    card_id: card_id.clone(),
                    host: *host,
                });
            }
        }
    }
    candidates
}


/// One `BreakSubroutineWithClick` per pending subroutine, gated on the
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
        .filter(|s| s.status == SubroutineStatus::Pending && s.definition.only_breakable_by.is_none())
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
        candidates.extend(paid_ability_candidates(&installed.card, installed.install_id, registry));
    }
    // The score area too — Proprionegation's counter ability is used from
    // there, and a scored agenda needs no rez check.
    for scored in &state.corp.scored_agendas {
        candidates.extend(paid_ability_candidates(&scored.card, scored.install_id, registry));
    }
    for rig_card in &state.runner.rig {
        candidates.extend(paid_ability_candidates(&rig_card.card, rig_card.install_id, registry));
    }
    // Identities are always active — Topan's click ability.
    if let Some(identity) = &state.corp.identity {
        candidates.extend(paid_ability_candidates(identity, InstallId::CORP_IDENTITY, registry));
    }
    if let Some(identity) = &state.runner.identity {
        candidates.extend(paid_ability_candidates(identity, InstallId::RUNNER_IDENTITY, registry));
    }
    candidates
}

/// Takes both the card (to look its abilities up) and the install (to name
/// the target), so two copies of one card offer two distinct actions rather
/// than collapsing onto whichever was installed first.
fn paid_ability_candidates(card_id: &CardId, target: InstallId, registry: &CardRegistry) -> Vec<PlayerAction> {
    let Some(card) = registry.get(card_id) else { return Vec::new() };
    card.abilities
        .iter()
        .enumerate()
        .filter(|(_, ability)| ability.trigger == Trigger::Paid)
        .map(|(ability_index, _)| PlayerAction::ActivateAbility { target, ability_index })
        .collect()
}

/// `AdvanceCard`/`ScoreAgenda` (Corp installed cards) and `TrashResource`
/// (Runner rig), keyed off each card's registry definition.
fn advance_score_trash_candidates(state: &GameState, registry: &CardRegistry) -> Vec<PlayerAction> {
    let mut candidates = Vec::new();
    for installed in &state.corp.installed {
        let Some(card) = registry.get(&installed.card) else { continue };
        if card.advancement_requirement.is_some() {
            candidates.push(PlayerAction::AdvanceCard { target: installed.install_id });
        }
        // Luminal Transubstantiation's lockout — mirrors the same guard in
        // `engine::score_agenda` so the mask never offers an action the
        // engine would reject.
        if card.card_type == CardType::Agenda && !state.corp.cannot_score_agendas_this_turn {
            candidates.push(PlayerAction::ScoreAgenda { target: installed.install_id });
        }
    }
    for rig_card in &state.runner.rig {
        if registry.get(&rig_card.card).is_some_and(|c| c.card_type == CardType::Resource) {
            candidates.push(PlayerAction::TrashResource { target: rig_card.install_id });
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
            PlayerAction::PayAccessTrigger { card_id: card_id.clone() },
            PlayerAction::DeclineAccessTrigger { card_id: card_id.clone() },
        ],
        AccessPhase::PendingChoice { card_id, trash_cost, mandatory_steal, steal_cost } => {
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
            // Affordability is not checked here: `resolve_trash` reads the
            // Runner's live credits, and the `apply_action` probe below
            // drops the candidate if they cannot pay. The state used to
            // carry a `can_trash` hint frozen at presentation, and gating on
            // it hid the trash from a Runner who gained the credits in the
            // window between being shown the card and deciding.
            if !steal_and_trash_blocked && trash_cost.is_some() {
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
    use crate::rules::test_support::install_of;
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
            ..Default::default()
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

    /// The probe is what filters an unaffordable install: the first ICE on
    /// a server is free even to a Corp on zero credits, and the second
    /// costs 1[c], which that Corp cannot pay (Rules Audit T3).
    #[test]
    fn install_card_candidates_are_filtered_by_the_ice_tax_not_the_printed_cost() {
        let mut registry = CardRegistry::new();
        let mut ice = blank_card("ice_wall", CardType::Ice(IceType::Barrier));
        ice.cost = 4;
        registry.insert(ice);

        let mut poor = corp_state(3, 0);
        poor.corp.hq = vec![CardId("ice_wall".to_string())];
        let poor_legal = legal_actions(&poor, &registry);
        assert!(
            poor_legal.contains(&PlayerAction::InstallCard {
                card_id: CardId("ice_wall".to_string()),
                zone: ServerId::Hq,
                slot: InstallSlot::Ice
            }),
            "the first ICE on a server is free, whatever it costs to rez"
        );

        poor.corp.installed = vec![crate::rules::InstalledCard {
            card: CardId("ice_wall".to_string()),
            install_id: InstallId(1),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let poor_legal = legal_actions(&poor, &registry);
        assert!(
            !poor_legal.contains(&PlayerAction::InstallCard {
                card_id: CardId("ice_wall".to_string()),
                zone: ServerId::Hq,
                slot: InstallSlot::Ice
            }),
            "a second ICE on HQ costs 1, which a broke Corp cannot pay"
        );
        assert!(
            poor_legal.contains(&PlayerAction::InstallCard {
                card_id: CardId("ice_wall".to_string()),
                zone: ServerId::RnD,
                slot: InstallSlot::Ice
            }),
            "but the first ICE on R&D is still free"
        );
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
            install_id: InstallId(1065),
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
        assert!(runner_legal.contains(&PlayerAction::DrawCardClick { side: Side::Runner }));
        assert!(runner_legal.contains(&PlayerAction::EndTurn));

        let corp_legal = legal_actions(&corp_state(3, 5), &registry);
        for action in &corp_legal {
            assert!(
                !matches!(action, PlayerAction::DrawCardClick { .. } | PlayerAction::InitiateRun { .. }),
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
            initiating_install: None,
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
            initiating_install: None,
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
            install_id: InstallId(1066),
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
                install_id: crate::rules::InstallId::PLACEHOLDER,
                card_id: CardId("wall_of_static".to_string()),
                current_strength: 3,
                ice_type: IceType::Barrier,
                subroutines: vec![EncounteredSubroutine {
                    id: 0,
                    definition: SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun, only_breakable_by: None },
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
        // The breaker's pump is offered; no free break exists. Wall of
        // Static is not a bioroid either, so the click-break is not offered
        // — the Runner's only way through is the breaker (Rules Audit T1).
        assert!(legal.contains(&PlayerAction::ActivateAbility { target: install_of(&state, "corroder"), ability_index: 0 }));
        assert!(!legal.iter().any(|a| matches!(a, PlayerAction::BreakSubroutineWithClick { .. })));
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
            access_state: Some(AccessState { pending_install: None, resolved_installs: Vec::new(),
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
        // Nothing else, not even a basic click: an active run suspends every
        // basic action (`engine::apply_action`'s `ActionBlockedByActiveRun`
        // guard). No `PaidAbilityWindow` is open here either — `SelectNextCard`
        // isn't a checkpoint, per `paid_ability::open_window_if_at_checkpoint`
        // — so this pins the guard rather than a window incidentally covering
        // for it.
        assert_same_actions(
            &legal,
            &[
                PlayerAction::SelectCardToAccess { card_id: CardId("a".to_string()) },
                PlayerAction::SelectCardToAccess { card_id: CardId("b".to_string()) },
            ],
        );
    }

    #[test]
    fn mandatory_steal_offers_only_steal() {
        let mut state = runner_state(3, 5);
        state.active_run = Some(RunState {
            phase: RunPhase::AccessingCard,
            access_state: Some(AccessState { pending_install: None, resolved_installs: Vec::new(),
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("agenda".to_string()),
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
            &[PlayerAction::StealAgenda { card_id: CardId("agenda".to_string()) }],
        );
    }

    /// Whether the trash is offered follows the Runner's *live* credits.
    /// The access state used to carry a `can_trash` hint computed when the
    /// card was presented, and this list trusted it — so credits gained in
    /// the paid-ability window before deciding never unlocked the trash.
    #[test]
    fn trash_is_offered_from_live_credits_not_a_hint_frozen_at_presentation() {
        let mut state = runner_state(3, 5);
        state.active_run = Some(RunState {
            phase: RunPhase::AccessingCard,
            access_state: Some(AccessState { pending_install: None, resolved_installs: Vec::new(),
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("asset".to_string()),
                    trash_cost: Some(2),
                    mandatory_steal: false,
                    steal_cost: None,
                },
                ..Default::default()
            }),
            jack_out_permitted: true,
            ..Default::default()
        });
        let trash = PlayerAction::TrashAccessedCard { card_id: CardId("asset".to_string()) };

        state.runner.resources.credits = Credits(1);
        assert!(!legal_actions(&state, &CardRegistry::new()).contains(&trash), "1 credit cannot pay a trash cost of 2");

        // The Runner gains a credit while the card is still presented
        // (a paid ability in the pre-trash window).
        state.runner.resources.credits = Credits(2);
        assert!(legal_actions(&state, &CardRegistry::new()).contains(&trash), "and now it can");
    }

    #[test]
    fn trashable_non_agenda_offers_trash_and_pass_not_steal() {
        let mut state = runner_state(3, 5);
        state.active_run = Some(RunState {
            phase: RunPhase::AccessingCard,
            access_state: Some(AccessState { pending_install: None, resolved_installs: Vec::new(),
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("asset".to_string()),
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
            initiating_install: None,
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
            install_id: InstallId(1067),
            card: CardId("ice_wall".to_string()),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce {
                install_id: InstallId(1067),
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
        let rez = PlayerAction::RezIce { ice: install_of(&state, "ice_wall") };
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

        let activate = PlayerAction::ActivateAbility { target: install_of(&state, "corroder"), ability_index: 0 };
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

    /// An emptied remote's number is offered again; ids never ratchet past
    /// what `ActionSpace` can index just because servers came and went.
    #[test]
    fn fresh_remote_ids_are_recycled() {
        assert_eq!(fresh_remote_id(&[]), 0);
        assert_eq!(fresh_remote_id(&[0, 1, 2]), 3);
        assert_eq!(fresh_remote_id(&[0, 2]), 1, "remote 1 was emptied and comes back");
        assert_eq!(fresh_remote_id(&[3]), 0);
    }
}
