//! Shared fixture helpers for this crate's tests.
//!
//! Actions name *installs* and *zone positions*, not cards (see
//! `state::InstallId` and `PlayerAction::ToggleCardSelection`). That is
//! right for the engine and wrong for a test, which reads far better saying
//! "rez *Palisade*" than "rez install 4". These bridge the two, so fixtures
//! keep naming cards.

use std::collections::HashSet;

use crate::dsl::CardId;
use crate::rules::pending_choice;
use crate::rules::state::{GameState, InstallId, InstallSlot, InstalledCard, PendingDecision};

/// Puts on the board the ICE a hand-built `active_run` names.
///
/// `run::reconcile_ice` treats `corp.installed` as the truth about which
/// ICE protects the attacked server, so a fixture that sets `active_run`
/// with a `RunIce` list and no matching installs describes a run on ICE
/// that has since been trashed — and the run moves off it at its next
/// step, which is correct and not what such a test meant. Call this after
/// building the run: every `RunIce` without an install gets one (same
/// server, `rezzed` copied), and entries that share an `InstallId` — the
/// placeholder, typically — are given distinct fixture ids first so the
/// two lists can be matched entry for entry.
pub(crate) fn install_the_runs_ice(state: &mut GameState) {
    let GameState { active_run, corp, .. } = state;
    let Some(run) = active_run.as_mut() else { return };
    let mut seen = HashSet::new();
    for (index, ice) in run.ice.iter_mut().enumerate() {
        if !seen.insert(ice.install_id) {
            ice.install_id = InstallId(900_000 + index as u32);
            seen.insert(ice.install_id);
        }
        if corp.installed.iter().any(|c| c.install_id == ice.install_id) {
            continue;
        }
        corp.installed.push(InstalledCard {
            install_id: ice.install_id,
            card: ice.card_id.clone(),
            server: run.server,
            slot: InstallSlot::Ice,
            rezzed: ice.rezzed,
            ..Default::default()
        });
    }
}

/// A stable, distinct `InstallId` derived from a card's id — for the
/// fixture *constructors* (`corp_ice`, `installed_with_counters`, …) that
/// build a fresh install per call and so cannot carry a literal id.
///
/// Every fixture in this crate installs at most one copy of any given card,
/// which is exactly the condition under which keying on the name is
/// unambiguous — and [`install_of`], which resolves the other direction,
/// carries the same caveat. A fixture that installs two copies must assign
/// both ids itself.
///
/// The range sits far above anything `GameState::allocate_install_id`
/// reaches in a test, so a fixture install can never collide with one the
/// engine allocates later in the same test.
pub(crate) fn fixture_install_id(card: &str) -> InstallId {
    // FNV-1a. Any stable hash would do; this one avoids a dependency.
    let mut hash: u32 = 2_166_136_261;
    for byte in card.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    InstallId(1_000_000 + hash % 1_000_000)
}

/// The `InstallId` of the install of `card`, on either side.
///
/// Takes the **first** match — exactly what the production lookup did
/// before `InstallId`, and unambiguous for every fixture that installs at
/// most one copy of a card. A test that deliberately installs two must name
/// the ids itself; distinguishing them is the whole point of `InstallId`,
/// and `advancing_one_of_two_copies_advances_only_that_copy` does it.
pub(crate) fn install_of(state: &GameState, card: &str) -> InstallId {
    let id = CardId(card.to_string());
    let found = state
        .corp
        .installed
        .iter()
        .find(|c| c.card == id)
        .map(|c| c.install_id)
        .or_else(|| state.runner.rig.iter().find(|c| c.card == id).map(|c| c.install_id))
        .unwrap_or_else(|| panic!("{card} is not installed on either side"));

    // A fixture that hand-builds several installs with `..Default::default()`
    // gives them all `InstallId::PLACEHOLDER`, and every action then
    // resolves to whichever was listed first — silently acting on the wrong
    // card, which is precisely the aliasing `InstallId` exists to end.
    // Fail loudly instead: such a fixture must set its own ids.
    if found == InstallId::PLACEHOLDER {
        let placeholders = state.corp.installed.iter().filter(|c| c.install_id == InstallId::PLACEHOLDER).count()
            + state.runner.rig.iter().filter(|c| c.install_id == InstallId::PLACEHOLDER).count();
        assert!(
            placeholders <= 1,
            "{placeholders} installs share InstallId::PLACEHOLDER, so {card} cannot be addressed \
             unambiguously — give this fixture's installs explicit ids"
        );
    }
    found
}

/// The position of `card` within the parked `ChooseCards` decision's source
/// zone — what `PlayerAction::ToggleCardSelection` names.
///
/// Same first-match caveat as [`install_of`]: a test selecting two copies of
/// one card must spell the positions out, as
/// `carnivore_can_select_two_copies_of_the_same_card` does.
///
/// A parked payment that asks for a card (`payment::Ask::Card`), or an
/// install asking which like card it trashes (`payment::Ask::Install`), is
/// answered by position too, and comes first, as `apply_action` answers it
/// first.
pub(crate) fn position_of(state: &GameState, card: &str) -> usize {
    let id = CardId(card.to_string());
    // An install's question names its candidates outright.
    if let Some(crate::rules::PendingPayment { question: crate::rules::payment::Ask::Install(question), .. }) = &state.pending_payment {
        return question
            .eligible
            .iter()
            .find(|candidate| candidate.card == id)
            .map(|candidate| candidate.position as usize)
            .unwrap_or_else(|| panic!("{card} is not one the install could trash"));
    }
    if let Some(crate::rules::PendingPayment { side, question: crate::rules::payment::Ask::Card(question), .. }) = &state.pending_payment {
        return pending_choice::zone_card_ids(state, *side, &question.zone, None)
            .iter()
            .position(|c| *c == id)
            .unwrap_or_else(|| panic!("{card} is not in the zone the payment asks about"));
    }
    let Some(PendingDecision::ChooseCards { side, source, source_install, .. }) = &state.pending_decision else {
        panic!("no ChooseCards decision is parked");
    };
    pending_choice::zone_card_ids(state, *side, source, *source_install)
        .iter()
        .position(|c| *c == id)
        .unwrap_or_else(|| panic!("{card} is not in the pending decision's source zone"))
}

/// The strength of the `index`th piece of ice in the active run, as the
/// break contest would read it — a run's ice stores no strength to assert
/// on, so a test asks the question the engine asks.
pub(crate) fn ice_strength_in_run(state: &GameState, registry: &crate::cards::CardRegistry, index: usize) -> i32 {
    let run = state.active_run.as_ref().expect("a run is active");
    crate::rules::continuous::ice_strength(state, registry, &run.ice[index])
}

/// A Barrier named `card_id` that prints `strength` and nothing else. A
/// run's ice stores no strength, so a fixture whose break contest turns on
/// one registers the card that says it.
pub(crate) fn ice_printing(card_id: &str, strength: i32) -> crate::dsl::CardDefinition {
    crate::dsl::CardDefinition {
        id: CardId(card_id.to_string()),
        title: card_id.to_string(),
        side: crate::rules::state::Side::Corp,
        card_type: crate::dsl::CardType::Ice(crate::dsl::IceType::Barrier),
        strength: Some(strength),
        ..crate::dsl::CardDefinition::default()
    }
}

/// A run succeeded earlier this turn, as far as any card asks — counted
/// the way the engine counts it, because there is no flag to set.
pub(crate) fn a_run_succeeded_this_turn(state: &mut GameState) {
    let event = crate::rules::GameEvent::RunSucceeded { server: crate::rules::ServerId::Hq };
    crate::rules::turn_log::record(state, &crate::cards::CardRegistry::default(), &event);
}

/// A run succeeded during the turn that has just ended.
pub(crate) fn a_run_succeeded_last_turn(state: &mut GameState) {
    a_run_succeeded_this_turn(state);
    crate::rules::turn_log::rotate(state);
}

/// Plays the run's movement phase out the way a person who does not jack
/// out would: `ContinueRun` past the jack-out decision, both players pass
/// the window that opens, and the Runner approaches what is next — the
/// next piece of ice (standing in its rez window) or the server
/// (`RunPhase::Success`). From `Initiation` on a server with no ice it is
/// the whole way to the server.
///
/// Stops early on anything parked (a trigger's choice, a paid choice, a
/// trace, a payment question), so a test can answer it; and it has the
/// shape of `apply_action`, so a call site that expected the one
/// `ContinueRun` this used to take keeps its `.expect(...)`.
pub(crate) fn through_movement(
    state: &GameState,
    registry: &crate::cards::CardRegistry,
) -> Result<(GameState, Vec<crate::rules::GameEvent>), crate::rules::RulesError> {
    use crate::rules::{apply_action, run::RunPhase, PlayerAction};
    let mut state = state.clone();
    let mut events = Vec::new();
    let mut continued = false;
    loop {
        let parked = state.pending_decision.is_some()
            || state.pending_paid_choice.is_some()
            || state.active_trace.is_some()
            || state.pending_payment.is_some();
        let Some(phase) = state.active_run.as_ref().map(|run| run.phase) else { break };
        let in_movement = matches!(phase, RunPhase::Initiation | RunPhase::Movement);
        if parked || (continued && !in_movement) {
            break;
        }
        let action = match &state.paid_ability_window {
            Some(window) => PlayerAction::PassPriority { side: window.active_priority },
            None if in_movement => PlayerAction::ContinueRun,
            None => break,
        };
        continued |= action == PlayerAction::ContinueRun;
        let (next, more) = apply_action(&state, registry, action)?;
        state = next;
        events.extend(more);
    }
    Ok((state, events))
}

/// A `ContinueRun`, or, from a run standing in its initiation's paid
/// ability window (CR 6.9.1e), both players' passes, which take the run to
/// the same place: the outermost ice's approach, standing in its rez
/// window, or the movement phase of a server with no ice. The Runner took
/// that step with `ContinueRun` before the initiation had its window, and
/// the scripted runs that did keep this shape.
pub(crate) fn continue_run(
    state: &GameState,
    registry: &crate::cards::CardRegistry,
) -> Result<(GameState, Vec<crate::rules::GameEvent>), crate::rules::RulesError> {
    use crate::rules::{apply_action, run::RunPhase, PlayerAction};
    let at_initiation = state.active_run.as_ref().is_some_and(|run| run.phase == RunPhase::Initiation);
    if !at_initiation || state.paid_ability_window.is_none() {
        return apply_action(state, registry, PlayerAction::ContinueRun);
    }
    let mut state = state.clone();
    let mut events = Vec::new();
    while let Some(window) = state.paid_ability_window.as_ref()
        && state.active_run.as_ref().is_some_and(|run| run.phase == RunPhase::Initiation)
    {
        let (next, more) = apply_action(&state, registry, PlayerAction::PassPriority { side: window.active_priority })?;
        state = next;
        events.extend(more);
    }
    Ok((state, events))
}

/// `state` with the active side's clicks spent, ready for `EndTurn`.
///
/// `EndTurn` is refused while clicks remain (CR 5.6.2b; `RulesError::
/// ClicksRemain`), and most tests that end a turn are about what the turn
/// boundary does, not about how the clicks went. This stands in for
/// spending them, without the credits or cards spending them would add.
pub(crate) fn clicks_spent(state: &GameState) -> GameState {
    let mut state = state.clone();
    if let crate::rules::GamePhase::Action(side) = state.phase {
        state.resources_mut(side).clicks = crate::rules::state::Clicks(0);
    }
    state
}
