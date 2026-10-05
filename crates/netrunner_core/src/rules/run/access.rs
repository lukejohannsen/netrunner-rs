use crate::cards::CardRegistry;
use crate::dsl::{CardId, Cost, Prohibition};
use crate::rules::ability;
use crate::rules::continuous;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::payment::{self, Purpose};
use crate::rules::event::GameEvent;
use crate::rules::run::state::{AccessCandidate, AccessPhase, AccessState, OutsideBreach, RunPhase, RunState, ServerId};
use crate::rules::state::{ArchivedCard, GameState, InstallId, InstallSlot, ScoredAgenda, Side};
use crate::rules::uninstall;

/// Root (non-ICE) installs on `server` — ICE is excluded via
/// `InstalledCard::slot`, which the installing action declares explicitly
/// (see `InstallSlot`'s doc comment for why this doesn't need a full
/// `CardRegistry`). Every breach's candidates include these (CR 7.4.1a),
/// since Upgrades can be installed on central servers as well as remote
/// ones.
fn root_installs_on(state: &GameState, server: ServerId) -> Vec<AccessCandidate> {
    state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.server == server && installed.slot == InstallSlot::Root)
        .map(|installed| AccessCandidate::Root(installed.install_id))
        .collect()
}

/// The candidates a breach of `server` begins with (CR 7.4.1), and the
/// cards of HQ or R&D it will access, in order (`AccessState::from_zone`).
fn begin_breach(state: &mut GameState, server: ServerId) -> (Vec<AccessCandidate>, Vec<CardId>) {
    match server {
        // The random access limit is 1 plus one per
        // `RunState::additional_hq_access` (CR 7.3.5a–b), and each access
        // takes "a random candidate from among the ones in the Corp's hand"
        // (CR 7.3.4a) that has not already been chosen (CR 7.4.3). Drawn
        // here, in the order they will be accessed, with `GameState::next_u64`
        // — the deterministic source inside the state, per AGENTS.md — each
        // roll reduced modulo the shrinking pool, `damage::apply_damage`'s
        // idiom for N distinct random cards.
        ServerId::Hq => {
            let additional = state.active_run.as_ref().map_or(0, |run| run.additional_hq_access);
            let take = (1 + additional as usize).min(state.corp.hq.len());
            let mut pool = state.corp.hq.clone();
            let mut from_zone = Vec::with_capacity(take);
            for _ in 0..take {
                let roll = state.next_u64();
                let index = (roll as usize) % pool.len();
                from_zone.push(pool.remove(index));
            }
            (root_installs_on(state, server), from_zone)
        }
        // "1 candidate from the Corp's deck at a time in turn, working down
        // from the top of the deck" (CR 7.4.7). The top of the deck is the
        // end of the `Vec` (`engine.rs::draw_card_click`'s `pop()`), so
        // `.rev().take(n)` is the top `n` cards in the order they come.
        ServerId::RnD => {
            let additional = state.active_run.as_ref().map_or(0, |run| run.additional_rd_access);
            let take = (1 + additional as usize).min(state.corp.r_and_d.len());
            let from_zone = state.corp.r_and_d.iter().rev().take(take).cloned().collect();
            (root_installs_on(state, server), from_zone)
        }
        // "When the Runner breaches Archives, all the facedown cards in the
        // Corp's discard pile are turned faceup before the Runner accesses
        // any cards" (CR 7.3.2), and they stay faceup: the Runner has seen
        // them. Nothing flipped them before, so a card the Runner had just
        // been shown was masked from them again the moment the run ended
        // (ROADMAP Rules Audit, Tier 2). Every card in the pile is a
        // candidate (CR 7.4.1d), offered by name in name order (see
        // `AccessState::candidates`), and so is the root, which was missing
        // here alone while upgrades could be installed onto Archives (Rules
        // Audit T12).
        ServerId::Archives => {
            for archived in state.corp.archives.iter_mut() {
                archived.facedown = false;
            }
            let mut pile: Vec<CardId> = state.corp.archives.iter().map(|a| a.card.clone()).collect();
            pile.sort();
            let mut candidates: Vec<AccessCandidate> = pile.into_iter().map(AccessCandidate::Archived).collect();
            candidates.extend(root_installs_on(state, server));
            (candidates, Vec::new())
        }
        ServerId::Remote(_) => (root_installs_on(state, server), Vec::new()),
    }
}

/// The card of HQ or R&D (`AccessState::from_zone` names which) that the
/// zone still holds, counting copies: one that has left since the breach
/// began is no longer a candidate (CR 7.4.5).
fn zone_holds(state: &GameState, server: ServerId, card: &CardId, earlier_copies: usize) -> bool {
    let zone = match server {
        ServerId::Hq => &state.corp.hq,
        ServerId::RnD => &state.corp.r_and_d,
        ServerId::Archives | ServerId::Remote(_) => return false,
    };
    zone.iter().filter(|c| *c == card).count() > earlier_copies
}

/// Drops every candidate that has left the breached server since the breach
/// began (CR 7.4.5): an install no longer in its root, a card no longer in
/// Archives, a card of HQ or R&D no longer there. And every candidate the
/// Runner is now prohibited from accessing (CR 7.4.2): once they have
/// accessed as many cards other than a Flagship as it allows, only the
/// Flagship itself is left (7.4.2b) — a limit that did nothing before the
/// first access, and never touched the random access limit. It is asked
/// afresh at each offer, so a limit that stops applying lets the cards
/// back (7.4.2a); the one in the pool persists when its card is trashed.
fn prune_candidates(state: &mut GameState, registry: &CardRegistry, server: ServerId) {
    let limits = continuous::access_limits(state, registry, server);
    let only_these = crate::rules::lingering::installs_prohibited(state, Prohibition::AccessOthers);
    let not_these = crate::rules::lingering::installs_prohibited(state, Prohibition::Access);
    // Accesses outside a breach name roots in other servers (Pinhole
    // Threading): any root still holds its candidate there.
    let outside_breach = state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).is_some_and(|access| access.outside_breach.is_some());
    let still_in_root: Vec<InstallId> = state
        .corp
        .installed
        .iter()
        .filter(|c| (outside_breach || c.server == server) && c.slot == InstallSlot::Root)
        .map(|c| c.install_id)
        .collect();
    let pile: Vec<CardId> = state.corp.archives.iter().map(|a| a.card.clone()).collect();
    let set_aside = state.corp.set_aside.clone();
    let Some(access) = state.active_run.as_ref().and_then(|run| run.access_state.as_ref()) else { return };
    let mut from_zone = Vec::with_capacity(access.from_zone.len());
    for card in &access.from_zone {
        let earlier = from_zone.iter().filter(|c| *c == card).count();
        if zone_holds(state, server, card, earlier) {
            from_zone.push(card.clone());
        }
    }
    let access = state.active_run.as_mut().and_then(|run| run.access_state.as_mut()).expect("checked above");
    access.from_zone = from_zone;
    access.candidates.retain(|candidate| match candidate {
        AccessCandidate::Root(install) => still_in_root.contains(install),
        AccessCandidate::Archived(card) => pile.contains(card),
        AccessCandidate::SetAside(card) => set_aside.contains(card),
        AccessCandidate::Zone => false,
    });
    for (count, card, install) in limits {
        let others = access.resolved_cards.iter().filter(|accessed| **accessed != card).count();
        if others >= count as usize {
            access.from_zone.clear();
            access.candidates.retain(|candidate| matches!(candidate, AccessCandidate::Root(root) if Some(*root) == install));
        }
    }
    // Adrian Seis's psi game, for the rest of the run: nothing but the
    // upgrade (bids differ), or everything but it (bids match).
    for only in &only_these {
        access.from_zone.clear();
        access.candidates.retain(|candidate| matches!(candidate, AccessCandidate::Root(root) if root == only));
    }
    access.candidates.retain(|candidate| !matches!(candidate, AccessCandidate::Root(root) if not_these.contains(root)));
}

/// What the Runner may choose among now: the zone, while it has a card
/// left to give, then the specific candidates.
fn selectable(access: &AccessState) -> Vec<AccessCandidate> {
    // "The procedure ends once the designated number of cards have been
    // chosen for access" (CR 7.1.10).
    if access.outside_breach.as_ref().is_some_and(|outside| outside.left == 0) {
        return Vec::new();
    }
    let zone = (!access.from_zone.is_empty()).then_some(AccessCandidate::Zone);
    zone.into_iter().chain(access.candidates.iter().cloned()).collect()
}

/// Takes `candidate` out of the breach's candidates (CR 7.4.3: "Once the
/// Runner chooses a candidate for access, it ceases to be a candidate"),
/// and returns the card it is and, for a card in the root, which install.
fn take_candidate(state: &mut GameState, candidate: &AccessCandidate) -> Option<(CardId, Option<InstallId>)> {
    let card = match candidate {
        AccessCandidate::Root(install) => state.find_corp_install(*install).map(|c| c.card.clone()),
        AccessCandidate::Archived(card) | AccessCandidate::SetAside(card) => Some(card.clone()),
        AccessCandidate::Zone => None,
    };
    let access = state.active_run.as_mut()?.access_state.as_mut()?;
    if let Some(outside) = access.outside_breach.as_mut() {
        outside.left = outside.left.saturating_sub(1);
    }
    match candidate {
        AccessCandidate::Zone => {
            if access.from_zone.is_empty() {
                return None;
            }
            Some((access.from_zone.remove(0), None))
        }
        AccessCandidate::Root(install) => {
            let position = access.candidates.iter().position(|c| c == candidate)?;
            access.candidates.remove(position);
            Some((card?, Some(*install)))
        }
        AccessCandidate::Archived(_) | AccessCandidate::SetAside(_) => {
            let position = access.candidates.iter().position(|c| c == candidate)?;
            access.candidates.remove(position);
            Some((card?, None))
        }
    }
}

/// Presents the next candidate, or asks which: with none left the breach
/// is over (`RunCompleted`), with one there is nothing to choose and it is
/// accessed, and with more the Runner picks (`AccessPhase::SelectNextCard`).
fn offer_next(state: &mut GameState, registry: &CardRegistry, server: ServerId) -> Result<Vec<GameEvent>, RulesError> {
    prune_candidates(state, registry, server);
    let access = state
        .active_run
        .as_mut()
        .and_then(|run| run.access_state.as_mut())
        .expect("offer_next called mid-access");
    let options = selectable(access);
    match options.len() {
        0 => {
            // What the card that asked for accesses outside a breach
            // resolves once they are over (Deep Dive's click for another).
            let after = access.outside_breach.as_mut().and_then(|outside| outside.then.take().map(|then| (then, outside.card.clone(), outside.install)));
            // A breach with no run ends as nothing a card hears: there was
            // no run to complete. A run that breached another server is
            // still a run on the one it attacked (CR 7.3.1).
            let mut events = Vec::new();
            if let Some(ended) = super::engine::end_run(state).filter(|run| !run.breach_only) {
                let completed_event = GameEvent::RunCompleted { server: ended.server };
                events.push(completed_event.clone());
                events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &completed_event)?);
            }
            if let Some((then, card, install)) = after {
                events.extend(resolve_after_accesses(state, registry, *then, card, install)?);
            }
            Ok(events)
        }
        1 => {
            let (card_id, install) = take_candidate(state, &options[0]).expect("a pruned candidate is still there");
            present_card_for_access(state, registry, server, &card_id, install)
        }
        _ => {
            access.phase = AccessPhase::SelectNextCard { selectable_cards: options };
            Ok(Vec::new())
        }
    }
}

/// Whether the card being accessed is in the Corp's discard pile — out of
/// Archives itself, not an upgrade in its root, which is installed and so
/// has a `pending_install`. Such a card cannot be trashed, by
/// the basic trash ability or any other (CR 7.1.5b: "The Runner cannot
/// trash or pay the trash cost of a card in the Corp's discard pile,
/// either with the basic trash ability or with other mid-access
/// abilities"). One predicate, so the offer and the effect cannot
/// disagree.
pub(crate) fn accessing_in_the_discard_pile(state: &GameState) -> bool {
    state
        .active_run
        .as_ref()
        .and_then(|run| run.access_state.as_ref())
        .is_some_and(|access| access.server == ServerId::Archives && access.pending_install.is_none())
}

/// `Effect::Access`: the Runner accesses `count` of the cards `from`
/// holds that `filter` admits, choosing each, not as a breach (CR 7.1.9,
/// 7.1.10) — so no `BreachBegun`, no random access limit, and nothing a
/// breach of a server would add. An installed card is a candidate in a
/// root (CR 7.4.1a's kind of candidate; a piece of ice protecting a server
/// is not in one); a set-aside card by its name, faceup.
///
/// Inside a run it is the run's breach replaced (`replacing_breach`), and
/// the run goes on to these accesses and ends when they do; anywhere else
/// in a run it is refused, as `Breach` is. Outside a run it stands in a
/// `RunState` flagged `breach_only`, on R&D, with nothing declared
/// successful: the access machinery is the run's (`run::engine::
/// start_breach`'s reasoning).
#[allow(clippy::too_many_arguments)]
pub(crate) fn access_cards(
    state: &mut GameState,
    registry: &CardRegistry,
    from: &crate::dsl::CardZoneRef,
    filter: &crate::dsl::CardFilter,
    count: u32,
    then: Option<Box<crate::dsl::Effect>>,
    card: Option<CardId>,
    install: Option<InstallId>,
    replacing_breach: bool,
) -> Result<Vec<GameEvent>, RulesError> {
    use crate::dsl::CardZoneRef;
    // The cards a selection over `from` would offer: the filter read off
    // each definition and each copy (`InAttackedServer` is the copy's).
    let admitted = crate::rules::pending_choice::eligible_positions(state, registry, Side::Runner, from, filter, None, None);
    let candidates: Vec<AccessCandidate> = match from {
        CardZoneRef::OpponentInstalled => admitted
            .into_iter()
            .filter_map(|position| state.corp.installed.get(position))
            .filter(|installed| installed.slot == InstallSlot::Root)
            .map(|installed| AccessCandidate::Root(installed.install_id))
            .collect(),
        CardZoneRef::OpponentSetAside => {
            let mut cards: Vec<CardId> = admitted.into_iter().filter_map(|position| state.corp.set_aside.get(position).cloned()).collect();
            // Faceup, and one choice per name: two copies are one card to
            // choose (`AccessState::candidates`' rule for Archives).
            cards.sort();
            cards.into_iter().map(AccessCandidate::SetAside).collect()
        }
        _ => return Err(RulesError::UnresolvedCardTarget),
    };
    match state.active_run.as_ref() {
        Some(run) if !replacing_breach || run.access_state.is_some() => return Err(RulesError::RunAlreadyInProgress),
        Some(_) => {}
        None => state.active_run = Some(RunState { server: ServerId::RnD, phase: RunPhase::Success, breach_only: true, ..RunState::default() }),
    }
    let run = state.active_run.as_mut().expect("stood up above");
    let server = run.server;
    run.phase = RunPhase::AccessingCard;
    run.access_state = Some(AccessState {
        server,
        candidates,
        outside_breach: Some(OutsideBreach { left: count, then, card, install }),
        ..AccessState::default()
    });
    offer_next(state, registry, server)
}

/// Resolves what a card asked for once its accesses outside a breach are
/// over, as that card — or queues it behind whatever the accesses left
/// parked, to resolve when that is answered (`DeferredTrigger::
/// continuation`, how a `Sequence` waits).
fn resolve_after_accesses(
    state: &mut GameState,
    registry: &CardRegistry,
    then: crate::dsl::Effect,
    card: Option<CardId>,
    install: Option<InstallId>,
) -> Result<Vec<GameEvent>, RulesError> {
    if state.resolution_halted() {
        if let Some(card) = card
            && !state.is_over()
        {
            state.deferred_triggers.push(crate::rules::state::DeferredTrigger {
                announce: None,
                card,
                trigger: crate::dsl::Trigger::OnPlay,
                target: None,
                install,
                target_install: None,
                event: None,
                continuation: Some(then),
                heard: Default::default(),
                not_the_first_this_turn: false,
                fired: 0,
            });
        }
        return Ok(Vec::new());
    }
    let mut ctx = ability::ResolutionContext::for_parked(install, card.as_ref());
    ability::evaluate_effect(state, &then, &mut ctx, registry)
}

/// The breach of a run declared successful (CR 6.9.5b), which ends the
/// run when it presents nothing — an empty server, or a replaced access.
/// `RunCompleted` is then *dispatched*, not merely pushed: a run on an
/// empty Archives is the most ordinary run there is, and Mayfly's "when
/// this run ends, trash this program" never fired on one while this only
/// recorded the event. Skipped when access already concluded with its own
/// `RunCompleted` (a flatline mid-access). Otherwise the run stands in
/// `RunPhase::AccessingCard`, and the Runner's decisions finish it.
pub(crate) fn breach(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    let (server, breach_only) = (run.server, run.breach_only);
    let mut events = access_server(state, server, registry)?;
    if !breach_only && state.active_run.is_none() && !events.iter().any(|e| matches!(e, GameEvent::RunCompleted { .. })) {
        dispatcher::emit(state, registry, &mut events, GameEvent::RunCompleted { server })?;
    }
    Ok(events)
}

/// Whether a breach is in progress (CR 7.3): from the first candidate to
/// the last access, the run stands in `RunPhase::AccessingCard`. No paid
/// ability window opens in it (CR 7.2, 7.5), so neither player uses a paid
/// ability there, and the Corp rezzes nothing — only an interrupt, when
/// the players are asked about a prevention, and the Runner's mid-access
/// ability at each access (`at_mid_access_window`).
pub(crate) fn breaching(state: &GameState) -> bool {
    state.active_run.as_ref().is_some_and(|run| run.phase == RunPhase::AccessingCard)
}

/// Whether the Runner stands at step 7.2.2 of an access: the mid-access
/// window (CR 9.2.10), where they "may use a single mid-access ability,
/// such as the basic trash ability". It *is* the decision about the card,
/// `AccessPhase::PendingChoice`, whose other answers are the basic trash
/// ability (`TrashAccessedCard`) and moving on (`StealAgenda`,
/// `PassAccessedCard`): each of them, and every mid-access ability in the
/// pool, finishes the access, so one ability per access (CR 9.2.10c) needs
/// no count of its own. A decision parked by something else is not the
/// window: it has priority over it until it is answered.
pub(crate) fn at_mid_access_window(state: &GameState) -> bool {
    !state.is_resolution_blocked()
        && state
            .active_run
            .as_ref()
            .and_then(|run| run.access_state.as_ref())
            .is_some_and(|access| matches!(access.phase, AccessPhase::PendingChoice { .. }))
}

fn compute_pending_choice(state: &GameState, card_id: &CardId, registry: &CardRegistry) -> AccessPhase {
    let card_def = registry.get(card_id);
    let is_agenda = card_def.is_some_and(|c| c.agenda_points.is_some());
    let steal_cost = card_def.and_then(|card| continuous::steal_price(state, registry, card));
    let mandatory_steal = is_agenda && steal_cost.is_none();
    // What the table adds (`ContinuousKind::TrashCost` — Mahkota Langit
    // Grid, rezzed or trashed earlier in this run), never below 0. Asked
    // about the *install* being accessed: a card accessed out of HQ or R&D
    // is in no server's root, and the field this replaced taxed it anyway
    // whenever the grid was in that central's root.
    let install = state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).and_then(|access| access.pending_install);
    let table = card_def.map_or(0, |card| continuous::trash_cost_delta(state, registry, card, install));
    // A card in the discard pile has no trash cost to pay, so the basic
    // trash ability is never offered on it (`accessing_in_the_discard_pile`).
    let trash_cost = card_def
        .and_then(|c| c.trash_cost)
        .filter(|_| !accessing_in_the_discard_pile(state))
        .map(|printed| (printed as i32 + table).max(0) as u32);
    // What the card says its trash costs beside the credits (Daniela Jorge
    // Inácio), only where there is a trash to pay for.
    let trash_also = trash_cost.and(card_def).and_then(|card| continuous::additional_trash_cost(state, registry, card));

    AccessPhase::PendingChoice { card_id: card_id.clone(), trash_cost, mandatory_steal, steal_cost, trash_also }
}

/// Sets `access.phase` to the `PendingChoice` computed from `card_id`'s
/// registry def, then fires its (unconditional) `Trigger::OnAccessed`
/// triggers. Does not itself emit `GameEvent::CardAccessed` — callers are
/// responsible for that, since it differs depending on whether this is a
/// card's first presentation (a fresh access) or the continuation of one
/// already announced via `AccessPhase::PendingInteractiveTrigger` (in which
/// case `CardAccessed` was already emitted once and must not repeat).
fn enter_pending_choice(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    card_id: &CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    // Mark the card as being accessed *before* its `OnAccessed` reaction
    // runs, so a trap that trashes itself out of that trigger is still
    // recognized as seen by the Runner (and lands faceup in Archives). See
    // `AccessState::currently_accessing`.
    let install = state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).and_then(|a| a.pending_install);
    if let Some(access) = state.active_run.as_mut().and_then(|run| run.access_state.as_mut()) {
        access.currently_accessing = Some(card_id.clone());
    }

    // Dispatched from the `CardAccessed { card, server, install }` shape
    // directly — this function doesn't itself emit `CardAccessed` (see the
    // doc comment above), only `dispatch_event`'s `OnAccessed` reaction to
    // it.
    let mut events = dispatcher::dispatch_event(
        state,
        registry,
        &GameEvent::CardAccessed { card: card_id.clone(), server, install },
    )?;
    if let Some(finish) = finish_if_game_over(state, server) {
        events.extend(finish);
        return Ok(events);
    }

    // The trigger just fired may have trashed `card_id` itself (e.g. a
    // self-trashing trap via `Effect::TrashCard(CardTarget::ThisCard)`) or
    // moved it elsewhere (`move_currently_accessed_card`) — presenting a
    // `PendingChoice` for a card that's already gone would let the Runner
    // "trash"/"steal" it a second time (`move_to_archives` doesn't verify
    // the card is still where it thinks, so this would duplicate it into
    // Archives). Treat it as this card's resolution instead, same as an
    // explicit `TrashAccessedCard` (CR 7.1.7).
    if left_its_place(&events, card_id) {
        events.extend(advance_or_finish(state, registry, server, card_id.clone())?);
        return Ok(events);
    }

    let phase = compute_pending_choice(state, card_id, registry);
    let run = state.active_run.as_mut().expect("enter_pending_choice called mid-access");
    let access = run.access_state.as_mut().expect("enter_pending_choice called mid-access");
    access.phase = phase;
    Ok(events)
}

/// True if `events` already trashed `card_id` — i.e. a `GameEvent::
/// CardTrashed` naming it, from a self-referencing `Effect::TrashCard(
/// CardTarget::ThisCard)`/`Cost::TrashSelf` fired while resolving this
/// card's own trigger/avoidance effects.
fn was_trashed(events: &[GameEvent], card_id: &CardId) -> bool {
    events.iter().any(|e| matches!(e, GameEvent::CardTrashed { card, .. } if card == card_id))
}

/// `was_trashed`, or moved anywhere else by a card's text while it was
/// being accessed (`move_currently_accessed_card`): out of the game, into
/// the Runner's score area, onto a rig card. Each ends the access (CR
/// 7.1.7).
fn left_its_place(events: &[GameEvent], card_id: &CardId) -> bool {
    was_trashed(events, card_id)
        || events.iter().any(|e| match e {
            GameEvent::CardRemovedFromGame { side: Side::Corp, card } | GameEvent::CardHosted { card, .. } => card == card_id,
            GameEvent::AddedToScoreAreaAsAgenda { side: Side::Runner, card, .. } => card == card_id,
            _ => false,
        })
}

/// Like `enter_pending_choice`, but for callers (`resolve_pay_access_trigger`/
/// `resolve_decline_access_trigger`) that may have already trashed `card_id`
/// themselves (via the paid avoidance cost or the declined effects) before
/// ever reaching `enter_pending_choice` — `enter_pending_choice`'s own
/// self-trash check only sees events from *its* `Trigger::OnAccessed` call,
/// not `prior_events`.
fn enter_pending_choice_unless_self_trashed(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    card_id: &CardId,
    prior_events: &[GameEvent],
) -> Result<Vec<GameEvent>, RulesError> {
    if was_trashed(prior_events, card_id) {
        return advance_or_finish(state, registry, server, card_id.clone());
    }
    enter_pending_choice(state, registry, server, card_id)
}

/// Presents `card_id` for access: emits `GameEvent::CardAccessed`, then
/// either parks at `AccessPhase::PendingInteractiveTrigger` (if the card's
/// registry def has an `InteractiveOnAccess` trigger — e.g. Fetal AI) or
/// goes straight to `enter_pending_choice`. The single entry point every
/// "a card is now being accessed" call site (`offer_next`,
/// `resolve_select_card`) should use.
fn present_card_for_access(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    card_id: &CardId,
    install: Option<InstallId>,
) -> Result<Vec<GameEvent>, RulesError> {
    // Outside a breach a root card is accessed in its own server (Pinhole
    // Threading's "in the root of another server"), and the access says so.
    let outside_breach = state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).is_some_and(|access| access.outside_breach.is_some());
    let server = match install.and_then(|install| state.find_corp_install(install)) {
        Some(installed) if outside_breach => installed.server,
        _ => server,
    };
    if outside_breach && let Some(access) = state.active_run.as_mut().and_then(|run| run.access_state.as_mut()) {
        access.server = server;
    }
    // Pin the instance first: every later step of this card's resolution
    // (its `OnAccessed` trigger, a trash, a steal) reads it from here.
    let rezzed = install.and_then(|install| state.find_corp_install(install)).is_some_and(|c| c.rezzed);
    // The Runner has now seen this card's face, and keeps it: CR 7.3.1a
    // for the rest of the breach, and `InstalledCard::seen_by_runner` for
    // what they remember after it.
    if let Some(card) = install.and_then(|install| state.corp.installed.iter_mut().find(|c| c.install_id == install)) {
        card.seen_by_runner = true;
    }
    // "An ability that counts in this way only includes accesses that are
    // actually performed" (CR 7.3.6): counted here, as each card is
    // accessed, rather than as the candidates the breach began with, some
    // of which may leave before they are reached (CR 7.4.5).
    if let Some(run) = state.active_run.as_mut() {
        run.cards_accessed_count += 1;
    }
    if let Some(access) = state.active_run.as_mut().and_then(|run| run.access_state.as_mut()) {
        access.pending_install = install;
        access.pending_install_rezzed = rezzed;
        access.left = false;
    }
    let mut events = vec![GameEvent::CardAccessed { card: card_id.clone(), server, install }];
    // "They must reveal it" (CR 1.21.7): the Corp is shown which of its
    // cards this is, as the view shows it for the rest of the access.
    if crate::rules::continuous::revealed_while_accessed(state, registry, card_id) {
        events.push(GameEvent::CardRevealed { side: Side::Corp, card: card_id.clone() });
    }

    // An unmet `requirement` means the trigger does not apply to *this*
    // access at all (Snare! accessed in Archives), so nothing is parked and
    // the card falls through to its ordinary `PendingChoice` below. Checked
    // here, at presentation, rather than at resolution: parking a decision
    // that then resolves to nothing would show the Runner a pause that
    // reveals a trap fired, and cost the deciding side an action for
    // nothing.
    let applies = registry.get(card_id).and_then(|c| c.interactive_on_access.as_ref()).is_some_and(|interactive| {
        interactive.requirement.as_ref().is_none_or(|requirement| {
            let side = interactive.interaction.payer();
            ability::check_requirement(state, requirement, side, &ability::ResolutionContext::for_card(Some(card_id)), registry)
                .is_ok()
        })
    });

    if let Some(interactive) = applies.then(|| registry.get(card_id).and_then(|c| c.interactive_on_access.as_ref())).flatten()
    {
        let decider = interactive.interaction.payer();
        // The one affordability question, which the payment reads too: a
        // catch-all `true` here offered a cost that takes cards the payer
        // does not have (`Cost::Trash`), and the pay then refused it.
        let can_pay = ability::cost_is_affordable(
            state,
            registry,
            decider,
            &interactive.cost,
            Purpose::Other,
            &ability::ResolutionContext::for_card(Some(card_id)),
        );
        let cost = interactive.cost.clone();
        let run = state.active_run.as_mut().expect("present_card_for_access called mid-access");
        let access = run.access_state.as_mut().expect("present_card_for_access called mid-access");
        access.phase = AccessPhase::PendingInteractiveTrigger { card_id: card_id.clone(), cost, decider, can_pay };
        return Ok(events);
    }

    events.extend(enter_pending_choice(state, registry, server, card_id)?);
    Ok(events)
}

/// Determine which cards a successful run against `server` accesses and, if
/// any, park the run in `RunPhase::AccessingCard`. A single accessed card
/// goes straight to an `AccessState` describing its `PendingChoice` —
/// `PlayerAction::StealAgenda`/`TrashAccessedCard`/`PassAccessedCard`
/// (`resolve_steal`/`resolve_trash`/`resolve_pass` below) resolve it. Two or
/// more accessed cards instead park at `AccessPhase::SelectNextCard`, so the
/// Runner picks resolution order via `PlayerAction::SelectCardToAccess`
/// (`resolve_select_card` below) before any `PendingChoice` is presented. If
/// nothing is accessed (empty zone), clears `active_run` immediately instead
/// — there's nothing to present a choice about, so the run is simply over.
///
/// Takes `&mut GameState` because HQ access needs `GameState::next_u64` to
/// pick a pseudo-random index, and every outcome mutates `active_run`.
///
/// Fallible only because presenting the first accessed card can fire its
/// `Trigger::OnAccessed` effects (`ability::process_card_triggers`), which
/// can themselves error; an empty zone or a card with no matching trigger
/// still always succeeds.
/// If `server` has a pending `Effect::SetAccessReplacement` parked in
/// `state.active_run`'s `access_replacement`, consumes it (clearing the
/// field so it can't be reused on a later access) and evaluates its
/// `Effect` in place of normal access, then concludes the run — mirroring
/// `access_server`'s empty-zone shortcut (`active_run = None`, no
/// `AccessPhase` presented). Returns `Some` with the accumulated events
/// (the replacement effect's own events, then `GameEvent::AccessReplaced`)
/// if a replacement fired; `None` (nothing consumed — proceed with normal
/// access) if there's no active run, no pending replacement, or the
/// pending replacement targets a different server.
///
/// Reads/compares under a short-lived `&` borrow first (extracting only a
/// `bool`), then `Option::take()`s the field under a fresh short-lived
/// `&mut` borrow (extracting and clearing it in one step) — both borrows
/// end before `ability::evaluate_effect` needs its own `&mut GameState`, so
/// there's no simultaneous-borrow conflict.
fn try_replace_access(
    state: &mut GameState,
    server: ServerId,
    registry: &CardRegistry,
) -> Result<Option<Vec<GameEvent>>, RulesError> {
    let matches = state
        .active_run
        .as_ref()
        .and_then(|run| run.access_replacement.as_ref())
        .is_some_and(|(replaced_server, _, _)| *replaced_server == server);
    if !matches {
        return Ok(None);
    }

    let (_, effect, optional) = state
        .active_run
        .as_mut()
        .expect("just confirmed active_run is Some above")
        .access_replacement
        .take()
        .expect("just confirmed access_replacement is Some above");
    let card = state.active_run.as_mut().and_then(|run| run.access_replacement_card.take());
    let install = state.active_run.as_mut().and_then(|run| run.access_replacement_install.take());

    if optional {
        // "You **may** … instead of breaching" (Account Siphon): the
        // choice belongs to the run's owner. Option 0 resolves the
        // replacement and ends the run — `EndTheRun` is the same "run
        // over, no breach" conclusion the mandatory path performs, just
        // recorded as `RunEndedByEffect`. Option 1 does nothing: the
        // replacement is already consumed (taken above), the run still
        // stands at `RunPhase::Success`, and the owner's next
        // `CompleteRun` breaches normally.
        state.pending_decision = Some(crate::rules::state::PendingDecision::ChooseEffect {
            option_texts: Vec::new(),
            chooser: Side::Runner,
            options: vec![
                crate::dsl::Effect::Sequence(vec![effect, crate::dsl::Effect::EndTheRun]),
                crate::dsl::Effect::Sequence(Vec::new()),
            ],
            source_card: card.clone(),
            prompting_card: card,
            source_install: install,
            resume: crate::rules::state::PendingChoiceResume::None,
        });
        return Ok(Some(vec![GameEvent::PendingChoicePresented { chooser: Side::Runner, option_count: 2 }]));
    }

    let mut ctx = ability::ResolutionContext::for_card(card.as_ref());
    ctx.acting_install = install;
    ctx.replacing_breach = true;
    let mut events = ability::evaluate_effect(state, &effect, &mut ctx, registry)?;
    // "Instead of breaching Archives, breach R&D": the run goes on, to a
    // breach of the server the replacement named (`RunState::breached`).
    // "Instead of breaching …, access 1 card in the root of another
    // server" (Pinhole Threading): the run goes on to those accesses, and
    // ends when they do (`offer_next`).
    if !state.active_run.as_ref().is_some_and(|run| run.breached.is_some() || run.access_state.is_some()) {
        super::engine::end_run(state);
    }
    events.push(GameEvent::AccessReplaced { server });
    Ok(Some(events))
}

/// The breach step of a run on `server` (CR 6.9.5b), or a breach with no
/// run: first whether a card replaces it ("instead of breaching", decided
/// as the breach would begin, CR 6.7.4c), then its beginning
/// (`GameEvent::BreachBegun`, of the server a replacement named or of
/// `server`), then the accesses.
///
/// **The accesses wait for what the beginning triggered.** "Whenever you
/// breach R&D, access 1 additional card" must be applied before the random
/// access limit is set (CR 7.3.5b), and Rotary's asks the Runner whether to
/// take a tag for it. So a beginning that leaves something parked or
/// queued returns there, with `RunState::breached` set, and the action that
/// answers it comes back through `engine::resume_run` to the accesses —
/// the way a "when successful" ability that parks holds up the breach.
pub fn access_server(
    state: &mut GameState,
    server: ServerId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();
    let server = match state.active_run.as_ref().and_then(|run| run.breached) {
        Some(breached) => breached,
        None => {
            let replaced = try_replace_access(state, server, registry)?;
            let replaced_by_a_breach = state.active_run.as_ref().and_then(|run| run.breached);
            if let Some(replaced) = replaced {
                events.extend(replaced);
                // Replaced by anything else, the breach is over, or waits on
                // whether it is replaced at all (Account Siphon's "may").
                if replaced_by_a_breach.is_none() {
                    return Ok(events);
                }
            }
            let breached = replaced_by_a_breach.unwrap_or(server);
            if let Some(run) = state.active_run.as_mut() {
                run.breached = Some(breached);
                dispatcher::emit(state, registry, &mut events, GameEvent::BreachBegun { server: breached })?;
                if state.resolution_halted() || !state.deferred_triggers.is_empty() || state.active_run.is_none() {
                    return Ok(events);
                }
            }
            breached
        }
    };

    // Counted before the breach turns them (CR 7.3.2): Nurse Hạnh hears
    // how many were facedown.
    let turned_faceup = if server == ServerId::Archives { state.corp.archives.iter().filter(|archived| archived.facedown).count() as u32 } else { 0 };
    let (candidates, from_zone) = begin_breach(state, server);
    if candidates.is_empty() && from_zone.is_empty() {
        super::engine::end_run(state);
        return Ok(events);
    }

    let run = state
        .active_run
        .as_mut()
        .expect("engine::complete_run confirmed active_run is Some before calling access_server");
    run.phase = RunPhase::AccessingCard;
    // Placeholder phase: `offer_next` overwrites it at once, with the
    // Runner's choice or the one card there is to present.
    run.access_state = Some(AccessState {
        server,
        candidates,
        from_zone,
        resolved_cards: Vec::new(),
        currently_accessing: None,
        pending_install: None,
        pending_install_rezzed: false,
        outside_breach: None,
        left: false,
        phase: AccessPhase::SelectNextCard { selectable_cards: Vec::new() },
    });
    events.extend(offer_next(state, registry, server)?);
    // Dispatched once the first access is offered rather than ahead of it:
    // a reaction that parks (two Nurse Hạnh to order) then waits beside
    // the Runner's choice of card instead of being overwritten by it, and
    // nothing a card in the pool does on hearing it touches the access.
    if turned_faceup > 0 {
        dispatcher::emit(state, registry, &mut events, GameEvent::ArchivesTurnedFaceup { count: turned_faceup })?;
    }
    Ok(events)
}

/// The `AccessState` fields `resolve_steal`/`resolve_trash`/`resolve_pass`
/// need, pulled out by value so the borrow of `state.active_run` doesn't
/// outlive the check — each caller goes on to mutate `state` afterward.
struct PendingAccess {
    server: ServerId,
    mandatory_steal: bool,
    steal_cost: Option<Cost>,
    trash_cost: Option<u32>,
    trash_also: Option<Cost>,
    /// `AccessState::pending_install` — the instance to take out of play.
    install: Option<InstallId>,
}

/// Confirms a run is parked in `RunPhase::AccessingCard` awaiting a choice
/// on exactly `card_id`, and returns that choice's context. Covers every
/// "wrong state to call this" case with a single error
/// (`RulesError::NotInAccessPhase`): no active run, the run isn't
/// `AccessingCard`, or `card_id` doesn't match what's actually pending —
/// mirroring how `RulesError::NotInEncounter` already covers several
/// "not in the right run sub-state" cases at once.
fn require_pending(state: &GameState, card_id: &CardId) -> Result<PendingAccess, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::PendingChoice { card_id: pending, mandatory_steal, steal_cost, trash_cost, trash_also } =
        &access.phase
    else {
        return Err(RulesError::NotInAccessPhase);
    };
    if pending != card_id {
        return Err(RulesError::NotInAccessPhase);
    }

    Ok(PendingAccess {
        server: access.server,
        mandatory_steal: *mandatory_steal,
        steal_cost: steal_cost.clone(),
        trash_cost: *trash_cost,
        trash_also: trash_also.clone(),
        install: access.pending_install,
    })
}

/// The `AccessState` fields `resolve_select_card` needs, pulled out by value
/// for the same borrow-scoping reason as `PendingAccess`.
struct PendingSelection {
    server: ServerId,
    selectable_cards: Vec<AccessCandidate>,
}

/// Confirms a run is parked in `RunPhase::AccessingCard` awaiting a
/// selection (`AccessPhase::SelectNextCard`), and returns that choice's
/// context. Covers every "wrong state to call this" case with a single
/// error (`RulesError::NotInAccessPhase`), mirroring `require_pending`.
fn require_selectable(state: &GameState) -> Result<PendingSelection, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::SelectNextCard { selectable_cards } = &access.phase else {
        return Err(RulesError::NotInAccessPhase);
    };

    Ok(PendingSelection { server: access.server, selectable_cards: selectable_cards.clone() })
}

/// Resolves `PlayerAction::SelectCardToAccess`. See its doc comment for the
/// error conditions.
pub fn resolve_select_card(
    state: &mut GameState,
    candidate: &AccessCandidate,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_selectable(state)?;
    if !pending.selectable_cards.contains(candidate) {
        return Err(RulesError::InvalidAccessSelection { candidate: candidate.clone() });
    }
    let (card_id, install) =
        take_candidate(state, candidate).ok_or_else(|| RulesError::InvalidAccessSelection { candidate: candidate.clone() })?;
    present_card_for_access(state, registry, pending.server, &card_id, install)
}

/// If `state.phase` became `GameOver` (e.g. a flatline mid-trigger, or an
/// agenda-point win), clears `active_run` and returns the terminal events;
/// otherwise `None`. Shared by every place in this file that fires
/// card-trigger effects capable of ending the game out from under an
/// in-progress access. `GameOver` itself is emitted by `win::end_game`,
/// which every way of ending the game goes through; this only closes the
/// run's own bookkeeping.
fn finish_if_game_over(state: &mut GameState, server: ServerId) -> Option<Vec<GameEvent>> {
    if state.is_over() {
        // `win::end_game` has already ended the run and emitted `GameOver`
        // — this used to push its own `GameOver` unless the caller's last
        // event was one, and `advance_or_finish` passed an empty slice, so
        // a steal whose identity reaction flatlined emitted it twice.
        let ended = super::engine::end_run(state);
        if ended.as_ref().is_some_and(|run| run.breach_only) {
            return Some(Vec::new());
        }
        Some(vec![GameEvent::RunCompleted { server: ended.map_or(server, |run| run.server) }])
    } else {
        None
    }
}

/// Shared tail of `resolve_steal`/`resolve_trash`/`resolve_pass`: if a steal
/// just won the game, finalize immediately without presenting further
/// accessed cards; otherwise record `resolved_card` as resolved and offer
/// the next candidate (`offer_next`).
fn advance_or_finish(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    resolved_card: CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    if let Some(events) = finish_if_game_over(state, server) {
        return Ok(events);
    }

    let run = state.active_run.as_mut().expect("advance_or_finish called mid-access");
    let access = run.access_state.as_mut().expect("advance_or_finish called mid-access");
    access.resolved_cards.push(resolved_card);
    access.pending_install = None;
    access.left = false;
    offer_next(state, registry, server)
}

/// Moves the breach on past the card at its access decision if that card
/// has left its place since the decision was presented (CR 7.1.7) — by a
/// card's text that resolved above the decision (`AccessState::left`; an
/// install, by having left the table). Asked by `engine::resume_run` at the
/// end of every action once nothing stands in the way, so it waits for
/// whatever the leaving began: Ganked!'s encounter, its choice of ice.
/// Nothing in any other access phase.
pub(crate) fn move_on_if_left(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    let Some(access) = state.active_run.as_ref().filter(|run| run.phase == RunPhase::AccessingCard).and_then(|run| run.access_state.as_ref()) else {
        return Ok(Vec::new());
    };
    let AccessPhase::PendingChoice { card_id, .. } = &access.phase else { return Ok(Vec::new()) };
    let install_gone = access.pending_install.is_some_and(|install| state.find_corp_install(install).is_none());
    if !(access.left || install_gone) {
        return Ok(Vec::new());
    }
    let (card_id, server) = (card_id.clone(), access.server);
    advance_or_finish(state, registry, server, card_id)
}

/// Resolves `PlayerAction::StealAgenda`. See its doc comment for the error
/// conditions.
pub fn resolve_steal(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    if continuous::cannot_about(state, registry, Prohibition::StealOrTrash, card_id) {
        return Err(RulesError::StealAndTrashPreventedThisRun);
    }
    let pending = require_pending(state, card_id)?;
    if !pending.mandatory_steal && pending.steal_cost.is_none() {
        return Err(RulesError::NotInAccessPhase);
    }

    let mut events = Vec::new();
    let mut cost_events = Vec::new();
    if let Some(cost) = &pending.steal_cost {
        // Asked of the scan the payment spends from: this was the credit
        // pool alone, which refused a steal that bad publicity's credits
        // or Methuselah's would have paid.
        if let Cost::Credits(requested) = cost {
            let available = payment::available(state, registry, Side::Runner, Purpose::Other);
            if available < *requested {
                return Err(RulesError::CannotAffordStealCost {
                    card: card_id.clone(),
                    available,
                    requested: *requested,
                });
            }
        }
        cost_events = ability::pay_cost(state, registry, Side::Runner, cost, Purpose::Other, Some(card_id))?;
        events.extend(cost_events.iter().cloned());
    }

    // The agenda leaves the Corp's zone — HQ, the top of R&D, the remote's
    // root, or Archives — and enters the Runner's score area. This removal
    // was missing: the card was pushed onto `scored_agendas` and *also*
    // stayed where it was, so the same top card of R&D was stolen again on
    // the next run and an installed agenda stayed scorable after being
    // stolen (ROADMAP Rules Audit T2). `pending.server` is what tells two
    // copies of one agenda in two remotes apart.
    let (removed_from, announced) = remove_from_corp_zone(state, registry, card_id, pending.server, pending.install)?;
    events.extend(announced);
    // A handle of its own: an ability of the agenda is used from the
    // Runner's score area by the Corp (Oracle Thinktank), and an ability
    // names its card by install.
    let install_id = state.allocate_install_id();
    // Project Vacheron's replacement, applied as it lands (CR 9.9.9c), so
    // what it is worth there is read with its counters from the start.
    let agenda_counters = match removed_from {
        RemovedFrom::Archives => 0,
        _ => registry.get(card_id).and_then(|card| card.stolen_with_agenda_counters).unwrap_or(0),
    };
    state.runner.scored_agendas.push(ScoredAgenda { scored_on_turn: state.turn, install_id, agenda_counters, ..ScoredAgenda::plain(card_id.clone()) });
    // Counted for `Trigger::OnRunEnded` consumers that gate on "if the
    // Runner stole any agendas during that run" (AMAZE Amusements), since
    // the `RunState` itself is gone by the time that trigger fires.
    if let Some(run) = state.active_run.as_mut() {
        run.agendas_stolen_this_run = run.agendas_stolen_this_run.saturating_add(1);
    }
    let landed = state.runner.scored_agendas.last().expect("pushed above");
    let agenda_points = crate::rules::win::scored_value(state, registry, landed, Side::Runner).max(0) as u32;
    state.runner.resources.agenda_points = state.runner.resources.agenda_points.gain(agenda_points as i32);
    if agenda_counters > 0 {
        events.push(GameEvent::CountersAdded { card: card_id.clone(), amount: agenda_counters });
    }
    let stolen_event = GameEvent::AgendaStolen { card: card_id.clone(), agenda_points };
    // Jinteki: Personal Evolution-style identity reaction to a steal —
    // unconditional dispatch, no per-turn gate.
    // …after the checkpoint, which `dispatch_event` runs first: a steal
    // that reaches the threshold has won, and the identity does not get to
    // flatline the winner.
    dispatcher::emit(state, registry, &mut events, stolen_event)?;
    // The cost's own moments after what it paid for (`ability::
    // dispatch_cost_events`): Shackleton Grid hears a steal paid for with
    // bad publicity's credits.
    events.extend(ability::dispatch_cost_events(state, registry, &cost_events)?);

    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone())?);
    Ok(events)
}

/// Where an accessed Corp card was taken from, so the caller can tell a
/// trashed upgrade (which keeps applying for the run if it says so) from a
/// trashed HQ or R&D card.
enum RemovedFrom {
    Hand,
    Deck,
    Archives,
    /// The Corp's set-aside zone (Deep Dive).
    SetAside,
    /// `rezzed`: whether it was, which is what makes a persistent ability
    /// persist (CR 9.12.5a).
    Installed { slot: InstallSlot, rezzed: bool },
    /// Nothing matched — the card is not in any Corp zone. Callers treat
    /// this as "nothing to remove" rather than an error; the access state
    /// that named the card is the authority on its existence.
    Nowhere,
}

/// Removes one copy of `card_id` from the Corp zone the Runner accessed it
/// in, preferring the zone `server` names.
///
/// Zone lookups are by `CardId`, and two copies of one card are
/// interchangeable everywhere except the table: for an install the copy in
/// `server`'s root is the accessed one, so that is removed first, before
/// falling back to any root install of that card. R&D is searched from the
/// top (the end of the `Vec` — see `begin_breach`), since the
/// accessed copy is the top one and a duplicate may sit deeper. Archives
/// is included because an agenda *stolen* out of Archives leaves it, even
/// though a card *trashed* while being accessed there stays put.
///
/// An install leaves through `rules::uninstall`, and what that announced
/// comes back beside where the card was.
fn remove_from_corp_zone(
    state: &mut GameState,
    registry: &CardRegistry,
    card_id: &CardId,
    server: ServerId,
    install: Option<InstallId>,
) -> Result<(RemovedFrom, Vec<GameEvent>), RulesError> {
    let from_the_table = |state: &mut GameState, install: InstallId| -> Result<(RemovedFrom, Vec<GameEvent>), RulesError> {
        Ok(match uninstall::corp_install(state, registry, install)? {
            Some((removed, announced)) => (RemovedFrom::Installed { slot: removed.slot, rezzed: removed.rezzed }, announced),
            None => (RemovedFrom::Nowhere, Vec::new()),
        })
    };
    // A card set aside faceup and accessed there (Deep Dive) leaves the
    // set-aside zone; its access is not a breach, so `server` names no zone.
    let outside_breach = state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).is_some_and(|access| access.outside_breach.is_some());
    if install.is_none()
        && outside_breach
        && let Some(position) = state.corp.set_aside.iter().position(|c| c == card_id)
    {
        state.corp.set_aside.remove(position);
        return Ok((RemovedFrom::SetAside, Vec::new()));
    }
    // The access pinned the exact instance: take that one and nothing
    // else. Two copies of one upgrade in a root used to both resolve to
    // the lower-indexed install (ROADMAP Rules Audit follow-ups).
    if let Some(install) = install
        && state.find_corp_install(install).is_some()
    {
        return from_the_table(state, install);
    }
    let installed_at = |installed: &crate::rules::state::InstalledCard| {
        &installed.card == card_id && installed.slot == InstallSlot::Root
    };
    // Without a pinned instance (a history recorded before
    // `pending_install` existed, or a card sampled into a root by
    // `determinize`): a card accessed on a remote is one of that remote's
    // root installs; prefer the copy on the run's server and fall back to
    // any root copy only there. A card accessed in HQ, R&D or
    // Archives is a card *in that zone*: only an upgrade installed in that
    // central's root may be matched among the installs. The fallback used
    // to apply to every server, so stealing Offworld Office off the top of
    // R&D removed the Corp's installed, advanced copy from its remote and
    // left the R&D copy in the deck (ROADMAP Rules Audit §4).
    let on_server = |c: &crate::rules::state::InstalledCard| installed_at(c) && c.server == server;
    let position = match server {
        ServerId::Remote(_) => {
            state.corp.installed.iter().position(on_server).or_else(|| state.corp.installed.iter().position(installed_at))
        }
        ServerId::Hq | ServerId::RnD | ServerId::Archives => state.corp.installed.iter().position(on_server),
    };
    if let Some(pos) = position {
        let install = state.corp.installed[pos].install_id;
        return from_the_table(state, install);
    }
    match server {
        ServerId::Hq => {
            if let Some(pos) = state.corp.hq.iter().position(|c| c == card_id) {
                state.corp.hq.remove(pos);
                return Ok((RemovedFrom::Hand, Vec::new()));
            }
        }
        ServerId::RnD => {
            if let Some(pos) = state.corp.r_and_d.iter().rposition(|c| c == card_id) {
                state.corp.r_and_d.remove(pos);
                return Ok((RemovedFrom::Deck, Vec::new()));
            }
        }
        ServerId::Archives => {
            if let Some(pos) = state.corp.archives.iter().position(|c| &c.card == card_id) {
                state.corp.archives.remove(pos);
                return Ok((RemovedFrom::Archives, Vec::new()));
            }
        }
        ServerId::Remote(_) => {}
    }
    // Not where the server said — take it from wherever it actually is.
    if let Some(pos) = state.corp.hq.iter().position(|c| c == card_id) {
        state.corp.hq.remove(pos);
        Ok((RemovedFrom::Hand, Vec::new()))
    } else if let Some(pos) = state.corp.r_and_d.iter().rposition(|c| c == card_id) {
        state.corp.r_and_d.remove(pos);
        Ok((RemovedFrom::Deck, Vec::new()))
    } else {
        Ok((RemovedFrom::Nowhere, Vec::new()))
    }
}

/// Removes `card_id` from wherever it currently lives (HQ, R&D, or a
/// Root-slot Corp install) and pushes it onto Archives — unless it was
/// already being accessed *from* Archives, in which case it's already
/// there and this is a no-op.
///
/// "From Archives" means a card in the pile, not an upgrade installed in
/// Archives' root: that one is pinned by `install` and has to leave the
/// table like any other root install. It used to be skipped with the pile
/// — the trash cost was paid and `CardTrashedFromAccess` emitted while the
/// upgrade stayed installed, unrezzed, which the spectator fog gate caught
/// at *Elevation* Stage 1 (the log named a card the view still concealed).
fn move_to_archives(
    state: &mut GameState,
    registry: &CardRegistry,
    card_id: &CardId,
    server: ServerId,
    install: Option<InstallId>,
) -> Result<Vec<GameEvent>, RulesError> {
    if server == ServerId::Archives && install.is_none() {
        return Ok(Vec::new());
    }
    let (from, announced) = remove_from_corp_zone(state, registry, card_id, server, install)?;
    // Only a rezzed card's persistent abilities persist (CR 9.12.5a): an
    // unrezzed one had none active to keep.
    if let RemovedFrom::Installed { slot: InstallSlot::Root, rezzed: true } = from {
        // "(If the Runner trashes this card while accessing it, this ability
        // still applies for the remainder of this run.)" — record it so
        // `Trigger::OnRunEnded` can still reach it from the registry once
        // it's no longer installed. Scoped to this run's own `RunState`, so
        // it cannot leak into a later run. Only the access-trash path
        // records this; an effect-driven trash of a persistent upgrade
        // mid-run does not, since no card in this set needs that and the
        // `Cost::TrashSelf` path has no registry to check the flag against.
        if registry.get(card_id).is_some_and(|card| card.persistent_after_trash)
            && let Some(run) = state.active_run.as_mut()
        {
            run.persistent_trashed_upgrades.push(card_id.clone());
        }
    }
    // The Runner just accessed this card, so it lands faceup.
    crate::rules::turn_log::file_in_archives(state, ArchivedCard::faceup(card_id.clone()));
    Ok(announced)
}

/// Resolves `Effect::TrashCurrentlyAccessedCard` — trashes whatever card is
/// currently pending in `AccessPhase::PendingChoice`, skipping its
/// `trash_cost` entirely (unlike `resolve_trash`, which charges it). e.g.
/// Carnivore's "trash 2 cards from your grip: trash the card you are
/// accessing." `RulesError::NotInAccessPhase` if the Runner isn't currently
/// mid-resolution of a specific accessed card.
pub fn trash_currently_accessed_card_without_cost(
    state: &mut GameState,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::PendingChoice { card_id, .. } = &access.phase else {
        return Err(RulesError::NotInAccessPhase);
    };
    let card_id = card_id.clone();
    let server = access.server;
    let install = access.pending_install;
    if accessing_in_the_discard_pile(state) {
        return Err(RulesError::CannotTrashFromArchives { card: card_id });
    }

    let mut events = move_to_archives(state, registry, &card_id, server, install)?;
    let trashed_event = GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: 0, install };
    events.push(trashed_event.clone());
    events.extend(dispatcher::dispatch_event(state, registry, &trashed_event)?);
    events.extend(advance_or_finish(state, registry, server, card_id)?);
    Ok(events)
}

/// Where a card's text puts the card the Runner is accessing — see
/// [`move_currently_accessed_card`].
#[derive(Debug, Clone, Copy)]
pub(crate) enum AccessedTo {
    /// Faceup on the rig card `host`, not installed (CR 1.13.2a) —
    /// `HostedCardOrigin::AccessedCard`: Cupellation's mid-access ability
    /// and Heliamphora's.
    Hosted(InstallId),
    /// Out of the game — Nightmare Archive's "remove this asset from the
    /// game", `Effect::RemoveFromGame(ThisCard)` resolving as the card
    /// being accessed.
    RemovedFromGame,
    /// The Runner's score area as an agenda (CR 10.1.3) — Nightmare
    /// Archive's "they may add it to their score area as an agenda worth
    /// -1 agenda point", `Effect::AddToScoreAreaAsAgenda` resolving as the
    /// card being accessed. Only the Runner accesses, so "their" is the
    /// Runner's.
    RunnerScoreArea(crate::dsl::AsAgenda),
}

/// Whether `card` is the card the Runner is accessing right now: at its
/// access decision, or presented and still resolving its own "when
/// accessed" (`AccessState::currently_accessing`). What a card's text says
/// it does with itself then is what [`move_currently_accessed_card`] does.
pub(crate) fn is_being_accessed(state: &GameState, card: &CardId) -> bool {
    breaching(state)
        && state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).is_some_and(|access| match &access.phase {
            AccessPhase::PendingChoice { card_id, .. } => card_id == card,
            _ => access.currently_accessing.as_ref() == Some(card),
        })
}

/// Moves the card being accessed to `to` and ends its access (CR 7.1.7:
/// "If a card moves to another location while it is being accessed, the
/// access ends immediately"), as a trash does. It leaves its zone the way a
/// trashed card does (`remove_from_corp_zone`, an install through
/// `uninstall::corp_install`), a card in Archives included.
///
/// At the access decision the breach moves on from here
/// (`advance_or_finish`). Out of the card's own "when accessed", before
/// the decision exists, `enter_pending_choice` sees the card has left
/// (`left_its_place`) and moves on itself. A card that is no longer where
/// it was accessed is not moved again. `RulesError::NotInAccessPhase` with
/// nothing being accessed.
///
/// One function for the three places a card's text sends it: Cupellation
/// hosted it, and Nightmare Archive was the second and third, which is
/// when the host's version became this.
pub(crate) fn move_currently_accessed_card(state: &mut GameState, registry: &CardRegistry, to: AccessedTo) -> Result<Vec<GameEvent>, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let (card_id, at_decision) = match &access.phase {
        AccessPhase::PendingChoice { card_id, .. } => (card_id.clone(), true),
        _ => (access.currently_accessing.clone().ok_or(RulesError::NotInAccessPhase)?, false),
    };
    let (server, install) = (access.server, access.pending_install);
    let host_card = match to {
        AccessedTo::Hosted(host) => Some(state.find_rig_install(host).map(|installed| installed.card.clone()).ok_or(RulesError::InstallNotFound(host))?),
        _ => None,
    };
    let (from, mut events) = remove_from_corp_zone(state, registry, &card_id, server, install)?;
    if matches!(from, RemovedFrom::Nowhere) {
        return Ok(events);
    }
    match to {
        AccessedTo::Hosted(host) => {
            if let Some(installed) = state.runner.rig.iter_mut().find(|installed| installed.install_id == host) {
                installed.hosted_cards.push(card_id.clone());
            }
            events.push(GameEvent::CardHosted { card: card_id.clone(), host: host_card });
        }
        AccessedTo::RemovedFromGame => {
            state.corp.removed_from_game.push(card_id.clone());
            events.push(GameEvent::CardRemovedFromGame { side: Side::Corp, card: card_id.clone() });
        }
        AccessedTo::RunnerScoreArea(as_agenda) => {
            events.push(ability::add_to_score_area_as_agenda(state, Side::Runner, card_id.clone(), as_agenda));
        }
    }
    if at_decision {
        events.extend(advance_or_finish(state, registry, server, card_id)?);
    }
    Ok(events)
}

/// Hosts the card being accessed faceup on the rig card `host` and ends
/// its access — [`move_currently_accessed_card`] to `AccessedTo::Hosted`.
pub(crate) fn host_currently_accessed_card(state: &mut GameState, registry: &CardRegistry, host: InstallId) -> Result<Vec<GameEvent>, RulesError> {
    move_currently_accessed_card(state, registry, AccessedTo::Hosted(host))
}

/// Resolves `PlayerAction::TrashAccessedCard`. See its doc comment for the
/// error conditions.
pub fn resolve_trash(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    if continuous::cannot_about(state, registry, Prohibition::StealOrTrash, card_id) {
        return Err(RulesError::StealAndTrashPreventedThisRun);
    }
    let pending = require_pending(state, card_id)?;
    let cost = pending.trash_cost.ok_or(RulesError::NotInAccessPhase)?;
    let also = pending.trash_also.clone();
    if let Some(also) = &also {
        let ctx = ability::ResolutionContext::for_card(Some(card_id));
        if !ability::cost_is_affordable(state, registry, Side::Runner, also, Purpose::TrashCost, &ctx) {
            return Err(RulesError::CannotAffordTrashCost { card: card_id.clone(), available: 0, requested: cost });
        }
    }

    // Asked of the scan the payment spends from (`rules::payment`), so a
    // pool that may pay a trash cost — Azimat's hosted credits, and the
    // run's own: bad publicity, Overclock — counts here too. This was a
    // sum of the credit pool and Azimat, which refused a trash the payment
    // below would have taken the run's credits for.
    let available = payment::available(state, registry, Side::Runner, Purpose::TrashCost);
    if available < cost {
        return Err(RulesError::CannotAffordTrashCost { card: card_id.clone(), available, requested: cost });
    }

    // Paid together (CR 1.16.10b): the credits, then what the card adds.
    let mut cost_events = ability::pay_cost(state, registry, Side::Runner, &Cost::Credits(cost), Purpose::TrashCost, Some(card_id))?;
    if let Some(also) = &also {
        cost_events.extend(ability::pay_cost(state, registry, Side::Runner, also, Purpose::TrashCost, Some(card_id))?);
    }
    let mut events = cost_events.clone();
    events.extend(move_to_archives(state, registry, card_id, pending.server, pending.install)?);
    let trashed_event = GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: cost, install: pending.install };
    dispatcher::emit(state, registry, &mut events, trashed_event)?;
    events.extend(ability::dispatch_cost_events(state, registry, &cost_events)?);

    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone())?);
    Ok(events)
}

/// Resolves `PlayerAction::PassAccessedCard`. See its doc comment for the
/// error conditions.
pub fn resolve_pass(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending(state, card_id)?;
    let steal_blocked = continuous::cannot_about(state, registry, Prohibition::StealOrTrash, card_id);
    if pending.mandatory_steal && !steal_blocked {
        return Err(RulesError::MandatoryStealViolation { card: card_id.clone() });
    }

    let mut events = vec![GameEvent::AccessPassed { card: card_id.clone() }];
    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone())?);
    Ok(events)
}

/// The `AccessState` fields `resolve_access_trigger` needs, pulled out by
/// value for the same borrow-scoping reason as `PendingAccess`.
struct PendingInteractive {
    server: ServerId,
    cost: Cost,
    decider: Side,
}

/// Confirms a run is parked in `RunPhase::AccessingCard` awaiting an
/// interactive-trigger decision (`AccessPhase::PendingInteractiveTrigger`)
/// on exactly `card_id`, and returns that decision's context. Covers every
/// "wrong state to call this" case with a single error
/// (`RulesError::NotInAccessPhase`), mirroring `require_pending`.
fn require_pending_interactive(state: &GameState, card_id: &CardId) -> Result<PendingInteractive, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::PendingInteractiveTrigger { card_id: pending, cost, decider, .. } = &access.phase else {
        return Err(RulesError::NotInAccessPhase);
    };
    if pending != card_id {
        return Err(RulesError::NotInAccessPhase);
    }

    Ok(PendingInteractive { server: access.server, cost: cost.clone(), decider: *decider })
}

/// The one resolution path behind both `PlayerAction::PayAccessTrigger` and
/// `PlayerAction::DeclineAccessTrigger`.
///
/// Paying and declining are mirror images — the card's `AccessInteraction`
/// says which branch resolves `effects`, and the other branch resolves
/// nothing — so this is one function taking `paid` rather than two that
/// would have to be kept in step. Getting them out of step is exactly the
/// class of bug the `current_actor`/`apply_action` precedence invariant
/// exists to prevent, one layer up.
fn resolve_access_trigger(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
    paid: bool,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending_interactive(state, card_id)?;

    let interaction = registry
        .get(card_id)
        .and_then(|c| c.interactive_on_access.as_ref())
        .map(|interactive| interactive.interaction)
        .unwrap_or_default();

    let mut events = Vec::new();
    let mut cost_events = Vec::new();
    if paid {
        if let Cost::Credits(requested) = &pending.cost {
            let available = payment::available(state, registry, pending.decider, Purpose::Other);
            if available < *requested {
                return Err(RulesError::CannotAffordAccessTriggerCost {
                    card: card_id.clone(),
                    available,
                    requested: *requested,
                });
            }
        }
        cost_events = ability::pay_cost(state, registry, pending.decider, &pending.cost, Purpose::Other, Some(card_id))?;
        events.extend(cost_events.iter().cloned());
    }

    if paid != interaction.effects_resolve_on_decline() {
        let effects = registry
            .get(card_id)
            .and_then(|c| c.interactive_on_access.as_ref())
            .map(|interactive| interactive.effects.clone())
            .unwrap_or_default();

        events.extend(ability::evaluate_sequence(state, &effects, &mut ability::ResolutionContext::for_card(Some(card_id)), registry)?);
        // Only the effect-resolving branch can flatline the Runner, so the
        // game-over check belongs here rather than around the whole body.
        if let Some(finish) = finish_if_game_over(state, pending.server) {
            events.extend(finish);
            return Ok(events);
        }
    }

    events.extend(ability::dispatch_cost_events(state, registry, &cost_events)?);
    let choice_events = enter_pending_choice_unless_self_trashed(state, registry, pending.server, card_id, &events)?;
    events.extend(choice_events);
    Ok(events)
}

/// Resolves `PlayerAction::PayAccessTrigger`. See its doc comment for the
/// error conditions.
pub fn resolve_pay_access_trigger(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    resolve_access_trigger(state, card_id, registry, true)
}

/// Resolves `PlayerAction::DeclineAccessTrigger`. See its doc comment for
/// the error conditions.
pub fn resolve_decline_access_trigger(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    resolve_access_trigger(state, card_id, registry, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `access_server`'s events after the breach's beginning, which it
    /// checks is the first of them when there is a run to breach in (and
    /// nothing replaced it): the accesses are what these tests are about.
    fn accesses(state: &mut GameState, server: ServerId, registry: &CardRegistry) -> Vec<GameEvent> {
        let mut events = access_server(state, server, registry).unwrap();
        if let Some(GameEvent::BreachBegun { server: breached }) = events.first() {
            assert_eq!(*breached, server);
            events.remove(0);
        }
        events
    }
    use crate::rules::state::GamePhase;
    use crate::dsl::{AccessInteraction, CardDefinition, CardTarget, CardType, DamageType, Effect, InteractiveOnAccess, Trigger, TriggeredEffect};
    use crate::rules::run::state::RunState;
    use crate::rules::state::{
        AgendaPoints, Clicks, Credits, InstalledCard, MemoryUnits, PlayerResources, RunnerState,
        Side,
    };
    use std::collections::HashSet;

    /// An empty registry, for every test that doesn't exercise agenda
    /// scoring and so doesn't need real card definitions.
    fn registry() -> CardRegistry {
        CardRegistry::new()
    }

    /// A minimal Agenda `CardDefinition` worth `points` — everything besides id and
    /// `agenda_points` is irrelevant to these tests.
    fn agenda_card(id: &str, points: u32) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Agenda,
            advancement_requirement: Some(points),
            agenda_points: Some(points),
            is_playable: true,
            ..Default::default()
        }
    }

    /// A NAPD-Contract-style Agenda: worth `points`, but costs `steal_cost`
    /// credits to steal instead of being a mandatory free steal.
    fn costed_agenda_card(id: &str, points: u32, steal_cost: u32) -> CardDefinition {
        CardDefinition { steal_cost: Some(Cost::Credits(steal_cost)), ..agenda_card(id, points) }
    }

    /// A minimal non-Agenda Asset `CardDefinition` with the given `trash_cost` —
    /// everything besides id and `trash_cost` is irrelevant to these tests.
    fn trashable_card(id: &str, trash_cost: u32) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Asset,
            trash_cost: Some(trash_cost),
            is_playable: true,
            ..Default::default()
        }
    }

    /// A minimal non-Agenda, non-trashable Asset `CardDefinition` with an
    /// `OnAccessed` trigger firing `effects` — Snare!/Fetal AI-style traps.
    fn card_with_on_accessed(id: &str, effects: Vec<Effect>) -> CardDefinition {
        CardDefinition {
            triggers: vec![TriggeredEffect { subject: Some(crate::dsl::Subject::This), when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_heap: false, from_runner_score_area: false, text: None, trigger: Trigger::OnAccessed, effects, requirement: None }],
            trash_cost: None,
            ..trashable_card(id, 0)
        }
    }

    /// A trashable `CardDefinition` (see `trashable_card`) with an
    /// `OnTrashedFromAccess` trigger firing `effects` — Shock!-style.
    fn trashable_card_with_on_trashed_from_access(id: &str, trash_cost: u32, effects: Vec<Effect>) -> CardDefinition {
        CardDefinition {
            triggers: vec![TriggeredEffect { subject: Some(crate::dsl::Subject::This), when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_heap: false, from_runner_score_area: false, text: None, trigger: Trigger::OnTrashedFromAccess, effects, requirement: None }],
            ..trashable_card(id, trash_cost)
        }
    }

    /// A minimal non-Agenda, non-trashable Asset `CardDefinition` with an
    /// `InteractiveOnAccess` trigger — Fetal AI-style "pay `cost` to avoid
    /// `effects`."
    fn card_with_interactive_on_access(id: &str, cost: Cost, effects: Vec<Effect>) -> CardDefinition {
        CardDefinition {
            interactive_on_access: Some(InteractiveOnAccess { cost, effects, interaction: AccessInteraction::default(), requirement: None }),
            trash_cost: None,
            ..trashable_card(id, 0)
        }
    }

    /// A run against `server` already in `RunPhase::Success`, ready for
    /// `access_server` to park in `AccessingCard`.
    /// The cards of HQ or R&D the breach has reached or will reach, in
    /// order: the one being accessed, then `AccessState::from_zone`.
    fn reached_from_zone(state: &GameState) -> Vec<CardId> {
        let access = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        let current = match &access.phase {
            AccessPhase::PendingChoice { card_id, .. } | AccessPhase::PendingInteractiveTrigger { card_id, .. } => Some(card_id.clone()),
            AccessPhase::SelectNextCard { .. } => None,
        };
        current.into_iter().chain(access.from_zone.iter().cloned()).collect()
    }

    fn archived(id: &str) -> AccessCandidate {
        AccessCandidate::Archived(CardId(id.to_string()))
    }

    fn run_in_success(server: ServerId) -> RunState {
        RunState {
            server,
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
        }
    }

    fn game_state(
        hq: Vec<CardId>,
        r_and_d: Vec<CardId>,
        archives: Vec<CardId>,
        installed: Vec<InstalledCard>,
        seed: u64,
    ) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState { identity: None, identity_counters: 0, identity_flipped: false, identity_copy: 0, bad_publicity: 0, removed_from_game: Vec::new(), set_aside: Vec::new(), play_area: Vec::new(), once_per_turn_used: Default::default(),
                scored_agendas: Vec::new(),
                playable_from_archives: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                hq,
                r_and_d,
                // Plain fixture cards nobody has seen yet — facedown, the
                // ordinary state for anything discarded into Archives.
                archives: archives.into_iter().map(ArchivedCard::facedown).collect(),
                installed,
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: crate::rules::state::GamePhase::Action(Side::Corp),
            seed,
            ..Default::default()
        }
    }

    #[test]
    fn accessing_hq_with_one_card_yields_that_card() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        assert_eq!(
            accesses(&mut state, ServerId::Hq, &registry()),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::Hq,
                install: None,
            }]
        );
        // The RNG step still advances even with only one possible index.
        assert_eq!(state.rng_step, 1);
        assert_eq!(state.active_run.unwrap().phase, RunPhase::AccessingCard);
    }

    #[test]
    fn accessing_hq_is_deterministic_for_a_given_seed() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];
        let mut state_a = game_state(hq.clone(), Vec::new(), Vec::new(), Vec::new(), 42);
        state_a.active_run = Some(run_in_success(ServerId::Hq));
        let mut state_b = game_state(hq, Vec::new(), Vec::new(), Vec::new(), 42);
        state_b.active_run = Some(run_in_success(ServerId::Hq));

        let events_a = accesses(&mut state_a, ServerId::Hq, &registry());
        let events_b = accesses(&mut state_b, ServerId::Hq, &registry());

        assert_eq!(events_a, events_b);
        assert_eq!(events_a.len(), 1);
    }

    #[test]
    fn accessing_hq_yields_varied_indices_across_different_seeds() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];

        let accessed_cards: HashSet<CardId> = (0..20u64)
            .map(|seed| {
                let mut state = game_state(hq.clone(), Vec::new(), Vec::new(), Vec::new(), seed);
                state.active_run = Some(run_in_success(ServerId::Hq));
                match accesses(&mut state, ServerId::Hq, &registry()).into_iter().next() {
                    Some(GameEvent::CardAccessed { card, .. }) => card,
                    other => panic!("expected a CardAccessed event, got {other:?}"),
                }
            })
            .collect();

        assert!(
            accessed_cards.len() > 1,
            "expected varied indices across seeds, got only {accessed_cards:?}"
        );
    }

    #[test]
    fn accessing_rnd_yields_the_last_card() {
        let mut state = game_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::RnD));
        assert_eq!(
            accesses(&mut state, ServerId::RnD, &registry()),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::RnD,
                install: None,
            }]
        );
    }

    #[test]
    fn accessing_rnd_with_additional_access_presents_the_top_two_cards_one_at_a_time() {
        let mut state = game_state(
            Vec::new(),
            vec![CardId("bottom".to_string()), CardId("middle".to_string()), CardId("top".to_string())],
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(RunState { additional_rd_access: 1, ..run_in_success(ServerId::RnD) });

        // "1 candidate from the Corp's deck at a time in turn, working down
        // from the top" (CR 7.4.7): the top card is accessed with nothing to
        // choose, and the next one waits unseen until it is passed.
        let events = accesses(&mut state, ServerId::RnD, &registry());
        assert_eq!(events, vec![GameEvent::CardAccessed { card: CardId("top".to_string()), server: ServerId::RnD, install: None }]);
        let access_state = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(access_state.from_zone, vec![CardId("middle".to_string())]);

        let events = resolve_pass(&mut state, &CardId("top".to_string()), &registry()).unwrap();
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: CardId("top".to_string()) },
                GameEvent::CardAccessed { card: CardId("middle".to_string()), server: ServerId::RnD, install: None },
            ]
        );
    }

    #[test]
    fn accessing_hq_with_additional_access_yields_two_distinct_cards() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];
        let mut state = game_state(hq, Vec::new(), Vec::new(), Vec::new(), 42);
        state.active_run = Some(RunState { additional_hq_access: 1, ..run_in_success(ServerId::Hq) });

        accesses(&mut state, ServerId::Hq, &registry());

        let reached = reached_from_zone(&state);
        assert_eq!(reached.len(), 2);
        assert_eq!(reached.iter().collect::<HashSet<_>>().len(), 2, "cards must be distinct");
    }

    #[test]
    fn add_additional_access_stacks_to_three_total_cards() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];
        let mut state = game_state(hq, Vec::new(), Vec::new(), Vec::new(), 42);
        state.active_run = Some(RunState { additional_hq_access: 2, ..run_in_success(ServerId::Hq) });

        accesses(&mut state, ServerId::Hq, &registry());

        let reached = reached_from_zone(&state);
        assert_eq!(reached.len(), 3);
        assert_eq!(reached.iter().collect::<HashSet<_>>().len(), 3, "cards must be distinct");
    }

    #[test]
    fn accessing_hq_with_more_additional_access_than_available_caps_at_hq_size() {
        let hq = vec![CardId("card_0".to_string()), CardId("card_1".to_string())];
        let mut state = game_state(hq, Vec::new(), Vec::new(), Vec::new(), 42);
        state.active_run = Some(RunState { additional_hq_access: 4, ..run_in_success(ServerId::Hq) });

        accesses(&mut state, ServerId::Hq, &registry());

        let reached = reached_from_zone(&state);
        assert_eq!(reached.len(), 2);
        assert_eq!(reached.iter().collect::<HashSet<_>>().len(), 2, "cards must be distinct");
    }

    #[test]
    fn accessing_rnd_with_more_additional_access_than_available_caps_at_rnd_size() {
        let mut state = game_state(
            Vec::new(),
            vec![CardId("bottom".to_string()), CardId("top".to_string())],
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(RunState { additional_rd_access: 4, ..run_in_success(ServerId::RnD) });

        accesses(&mut state, ServerId::RnD, &registry());

        assert_eq!(reached_from_zone(&state), vec![CardId("top".to_string()), CardId("bottom".to_string())]);
    }

    #[test]
    fn access_replacement_skips_normal_access_and_fires_its_effect() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(RunState { agendas_stolen_this_run: 0, persistent_trashed_upgrades: Vec::new(),
            access_replacement: Some((ServerId::Hq, Effect::GainCredits(Side::Runner, 8), false)),
            ..run_in_success(ServerId::Hq)
        });

        let events = accesses(&mut state, ServerId::Hq, &registry());

        assert_eq!(
            events,
            vec![
                GameEvent::CreditsGained { side: Side::Runner, amount: 8 },
                GameEvent::AccessReplaced { server: ServerId::Hq },
            ]
        );
        assert_eq!(state.runner.resources.credits, Credits(8));
        assert!(state.active_run.is_none());
    }

    #[test]
    fn access_replacement_for_a_different_server_does_not_fire() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(RunState { agendas_stolen_this_run: 0, persistent_trashed_upgrades: Vec::new(),
            access_replacement: Some((ServerId::RnD, Effect::GainCredits(Side::Runner, 8), false)),
            ..run_in_success(ServerId::Hq)
        });

        let events = accesses(&mut state, ServerId::Hq, &registry());

        // Normal HQ access proceeded — the replacement (parked for RnD)
        // never fired.
        assert_eq!(
            events,
            vec![GameEvent::CardAccessed { card: CardId("hedge_fund".to_string()), server: ServerId::Hq, install: None }]
        );
        assert_eq!(state.runner.resources.credits, Credits(0));
        assert!(state.active_run.is_some());
    }

    #[test]
    fn accessing_hq_yields_hq_card_and_root_installed_upgrades() {
        let installed = vec![
            InstalledCard {
                card: CardId("ice_wall".to_string()),
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            },
            InstalledCard {
                card: CardId("ash_2_0".to_string()),
                ..Default::default()
            },
        ];
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            installed,
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        // Two cards are accessed (the HQ card and the Root-installed
        // Upgrade), so nothing is presented until the Runner picks which to
        // resolve first (see
        // `multi_card_sequence_advances_through_each_card_in_order`).
        assert_eq!(accesses(&mut state, ServerId::Hq, &registry()), Vec::new());
        // The upgrade is offered as the install it is, and HQ as "a random
        // card from HQ" (CR 7.3.4a): neither choice names a card the Runner
        // has not accessed.
        let ash = state.corp.installed[1].install_id;
        let access_state = state.active_run.unwrap().access_state.unwrap();
        assert_eq!(access_state.candidates, vec![AccessCandidate::Root(ash)]);
        assert_eq!(access_state.from_zone, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard { selectable_cards: vec![AccessCandidate::Zone, AccessCandidate::Root(ash)] }
        );
    }

    #[test]
    fn accessing_rnd_yields_rnd_card_and_root_installed_upgrades() {
        let installed = vec![
            InstalledCard {
                card: CardId("wraparound".to_string()),
                server: ServerId::RnD,
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            },
            InstalledCard {
                card: CardId("crisium_grid".to_string()),
                server: ServerId::RnD,
                ..Default::default()
            },
        ];
        let mut state = game_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            installed,
            0,
        );
        state.active_run = Some(run_in_success(ServerId::RnD));
        assert_eq!(accesses(&mut state, ServerId::RnD, &registry()), Vec::new());
        let grid = state.corp.installed[1].install_id;
        let access_state = state.active_run.unwrap().access_state.unwrap();
        assert_eq!(access_state.from_zone, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(access_state.candidates, vec![AccessCandidate::Root(grid)]);
    }

    #[test]
    fn accessing_archives_yields_every_card_in_it() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        assert_eq!(accesses(&mut state, ServerId::Archives, &registry()), vec![GameEvent::ArchivesTurnedFaceup { count: 2 }], "the breach turned both over (CR 7.3.2)");
        assert_eq!(state.active_run.unwrap().access_state.unwrap().candidates, vec![archived("hedge_fund"), archived("ice_wall")]);
    }

    /// Rules Conformance A1. The breach used to pick every HQ card and name
    /// it in the Runner's choice, and name every unrezzed card in the root
    /// too: "Access Priority Requisition, or the upgrade (Ash 2X3ZB9CY)?"
    /// A candidate is now what the Runner can point at — "a random card
    /// from HQ" (CR 7.3.4a) and an install — and neither the choice, the
    /// access state nor the Runner's view of it names a card not yet
    /// accessed.
    #[test]
    fn an_hq_breach_offers_the_hand_and_the_root_without_naming_either() {
        let installed = vec![InstalledCard {
            card: CardId("secret_upgrade".to_string()),
            install_id: InstallId(7),
            server: ServerId::Hq,
            slot: InstallSlot::Root,
            rezzed: false,
            ..Default::default()
        }];
        let mut state = game_state(vec![CardId("secret_agenda".to_string())], Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Hq));
        assert_eq!(accesses(&mut state, ServerId::Hq, &registry()), Vec::new(), "nothing is accessed before the Runner chooses");

        let access = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(
            access.phase,
            AccessPhase::SelectNextCard { selectable_cards: vec![AccessCandidate::Zone, AccessCandidate::Root(InstallId(7))] }
        );
        let runner_view = crate::rules::mask_state_for_player(&state, &registry(), Side::Runner);
        let seen = serde_json::to_string(&runner_view.active_run).unwrap();
        assert!(!seen.contains("secret_agenda") && !seen.contains("secret_upgrade"), "{seen}");

        // Choosing the hand accesses the random card, and only then is it named.
        let events = resolve_select_card(&mut state, &AccessCandidate::Zone, &registry()).unwrap();
        assert_eq!(events, vec![GameEvent::CardAccessed { card: CardId("secret_agenda".to_string()), server: ServerId::Hq, install: None }]);
    }

    /// Accessing an installed card marks it seen, whether it is left
    /// installed or not: the Runner keeps what they saw
    /// (`InstalledCard::seen_by_runner`). A card left unaccessed does not.
    #[test]
    fn an_accessed_install_is_seen_by_the_runner_and_one_not_reached_is_not() {
        let installed = vec![
            InstalledCard { card: CardId("first".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), ..Default::default() },
            InstalledCard { card: CardId("second".to_string()), install_id: InstallId(2), server: ServerId::Remote(0), ..Default::default() },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        accesses(&mut state, ServerId::Remote(0), &registry());
        resolve_select_card(&mut state, &AccessCandidate::Root(InstallId(1)), &registry()).unwrap();
        let seen = |state: &GameState, id| state.find_corp_install(InstallId(id)).unwrap().seen_by_runner;
        assert!(seen(&state, 1));
        assert!(!seen(&state, 2));
        resolve_pass(&mut state, &CardId("first".to_string()), &registry()).unwrap();
        assert!(seen(&state, 1), "and still seen once the access is over");
    }

    /// CR 7.4.5: "If a candidate leaves the breached server, it ceases to
    /// be a candidate." A root install trashed while another card was
    /// being accessed is not offered afterwards — it used to be accessed
    /// anyway, as whatever the zone fallback found under its name.
    #[test]
    fn a_root_card_that_leaves_mid_breach_is_no_longer_a_candidate() {
        let installed = vec![
            InstalledCard { card: CardId("first".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), rezzed: true, ..Default::default() },
            InstalledCard { card: CardId("second".to_string()), install_id: InstallId(2), server: ServerId::Remote(0), rezzed: true, ..Default::default() },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        accesses(&mut state, ServerId::Remote(0), &registry());
        resolve_select_card(&mut state, &AccessCandidate::Root(InstallId(1)), &registry()).unwrap();

        state.corp.installed.retain(|c| c.install_id != InstallId(2));
        let events = resolve_pass(&mut state, &CardId("first".to_string()), &registry()).unwrap();
        assert_eq!(
            events,
            vec![GameEvent::AccessPassed { card: CardId("first".to_string()) }, GameEvent::RunCompleted { server: ServerId::Remote(0) }]
        );
        assert_eq!(state.active_run, None);
    }

    /// Rules Conformance A2: "Discard piles are not ordered" (CR 4.4.2).
    /// The breach offers Archives' cards by name in name order, never in
    /// the order they arrived, which matched each card the breach turned
    /// faceup to the moment it went in facedown.
    #[test]
    fn archives_candidates_come_in_name_order_not_arrival_order() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("zeta".to_string()), CardId("alpha".to_string()), CardId("mu".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry());
        assert_eq!(
            state.active_run.unwrap().access_state.unwrap().phase,
            AccessPhase::SelectNextCard { selectable_cards: vec![archived("alpha"), archived("mu"), archived("zeta")] }
        );
    }

    /// Breaching Archives turns its facedown cards faceup — the Runner has
    /// seen them, and keeps seeing them after the run. Nothing flipped
    /// them before: the Runner's view named the card mid-access and masked
    /// it again the moment the run ended.
    #[test]
    fn accessing_archives_turns_its_facedown_cards_faceup_for_good() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        state.corp.archives = vec![
            crate::rules::ArchivedCard::facedown(CardId("hedge_fund".to_string())),
            crate::rules::ArchivedCard::faceup(CardId("ice_wall".to_string())),
        ];
        state.active_run = Some(run_in_success(ServerId::Archives));
        let registry = registry();

        accesses(&mut state, ServerId::Archives, &registry);

        assert!(state.corp.archives.iter().all(|a| !a.facedown), "{:?}", state.corp.archives);
        let view = crate::view::build_client_view(&state, &registry, Side::Runner);
        assert_eq!(
            view.corp.archives.iter().map(|a| a.card.clone()).collect::<Vec<_>>(),
            vec![Some(CardId("hedge_fund".to_string())), Some(CardId("ice_wall".to_string()))],
            "the Runner's view now names every card in Archives"
        );
    }

    /// A replaced access (`Effect::SetAccessReplacement`) never breaches, so
    /// nothing is revealed.
    #[test]
    fn a_replaced_archives_access_leaves_facedown_cards_facedown() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        state.corp.archives = vec![crate::rules::ArchivedCard::facedown(CardId("hedge_fund".to_string()))];
        let mut run = run_in_success(ServerId::Archives);
        run.access_replacement = Some((ServerId::Archives, Effect::GainCredits(Side::Runner, 1), false));
        state.active_run = Some(run);

        accesses(&mut state, ServerId::Archives, &registry());

        assert!(state.corp.archives[0].facedown);
    }

    #[test]
    fn accessing_remote_skips_installed_ice_and_yields_only_root_installs() {
        let installed = vec![
            InstalledCard {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            },
            InstalledCard {
                card: CardId("pad_campaign".to_string()),
                install_id: InstallId(5),
                server: ServerId::Remote(0),
                ..Default::default()
            },
            InstalledCard {
                card: CardId("enigma".to_string()),
                server: ServerId::Remote(1),
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        assert_eq!(
            accesses(&mut state, ServerId::Remote(0), &registry()),
            vec![GameEvent::CardAccessed {
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0),
                install: Some(InstallId(5)),
            }],
            "a root card is accessed as its instance"
        );
    }

    #[test]
    fn accessing_remote_with_only_ice_yields_no_events() {
        let installed = vec![InstalledCard {
            card: CardId("ice_wall".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        assert_eq!(accesses(&mut state, ServerId::Remote(0), &registry()), Vec::new());
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn accessing_an_empty_zone_yields_no_events() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        assert_eq!(accesses(&mut state, ServerId::Hq, &registry()), Vec::new());
        assert_eq!(accesses(&mut state, ServerId::RnD, &registry()), Vec::new());
        assert_eq!(accesses(&mut state, ServerId::Archives, &registry()), Vec::new());
        assert_eq!(accesses(&mut state, ServerId::Remote(0), &registry()), Vec::new());
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn free_agenda_access_is_a_mandatory_steal() {
        let registry = CardRegistry::from_cards(vec![agenda_card("priority_requisition", 3)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("priority_requisition".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("priority_requisition".to_string());
        assert_eq!(
            resolve_pass(&mut state, &card_id, &registry),
            Err(RulesError::MandatoryStealViolation { card: card_id.clone() })
        );

        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");
        assert_eq!(state.runner.scored_agendas.iter().map(|scored| scored.card.clone()).collect::<Vec<_>>(), vec![card_id.clone()]);
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(3));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    /// ROADMAP Rules Audit T2: stealing used to push the agenda onto the
    /// Runner's score area and leave it where it was, so the same top card
    /// of R&D was stolen every run and an installed agenda stayed scorable
    /// after being stolen. One test per zone an agenda can be accessed in.
    #[test]
    fn a_stolen_agenda_leaves_the_corp_zone_it_was_accessed_from() {
        let registry = CardRegistry::from_cards(vec![agenda_card("offworld_office", 2)]);
        let agenda = CardId("offworld_office".to_string());
        let other = CardId("unregistered_filler".to_string());

        // R&D: the *top* copy (end of the Vec) goes; a duplicate deeper stays.
        let mut state = game_state(Vec::new(), vec![agenda.clone(), other.clone(), agenda.clone()], Vec::new(), Vec::new(), 0);
        state.active_run = Some(run_in_success(ServerId::RnD));
        accesses(&mut state, ServerId::RnD, &registry);
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert_eq!(state.corp.r_and_d, vec![agenda.clone(), other.clone()], "the top copy left R&D");
        assert_eq!(state.runner.scored_agendas.iter().map(|scored| scored.card.clone()).collect::<Vec<_>>(), vec![agenda.clone()]);

        // HQ.
        let mut state = game_state(vec![agenda.clone()], Vec::new(), Vec::new(), Vec::new(), 0);
        state.active_run = Some(run_in_success(ServerId::Hq));
        accesses(&mut state, ServerId::Hq, &registry);
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert!(state.corp.hq.is_empty(), "the stolen agenda left HQ");

        // Archives: a stolen agenda leaves Archives too (a *trashed* card
        // accessed there stays — that is `move_to_archives`' early return).
        let mut state = game_state(Vec::new(), Vec::new(), vec![agenda.clone()], Vec::new(), 0);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert!(state.corp.archives.is_empty(), "the stolen agenda left Archives");

        // Two copies installed in two remotes: the run's server says which.
        // Distinct install ids, because the access now pins the instance
        // and would otherwise be told "the first card with this id".
        let installed = vec![
            InstalledCard {
                card: agenda.clone(),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                advancement_tokens: 1,
                ..Default::default()
            },
            InstalledCard {
                card: agenda.clone(),
                install_id: InstallId(2),
                server: ServerId::Remote(1),
                advancement_tokens: 2,
                ..Default::default()
            },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Remote(1)));
        accesses(&mut state, ServerId::Remote(1), &registry);
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert_eq!(state.corp.installed.len(), 1);
        assert_eq!(state.corp.installed[0].server, ServerId::Remote(0), "the copy in the run's remote is the one taken");
        assert_eq!(state.corp.installed[0].advancement_tokens, 1);
    }

    /// The server-preference above fell back to "any root copy" for *every*
    /// server, so an agenda accessed in R&D or HQ was removed from the
    /// remote it was installed in — the R&D copy stayed in the deck and the
    /// Corp's advanced copy went to the Runner's score area. Offworld
    /// Office ships ×3 in two sample decks, so this happened in ordinary
    /// play, and card conservation held, so the sweep could not see it.
    #[test]
    fn a_card_accessed_in_a_central_never_removes_an_installed_copy() {
        let registry = CardRegistry::from_cards(vec![agenda_card("offworld_office", 2), trashable_card("nico_campaign", 3)]);
        let agenda = CardId("offworld_office".to_string());
        let asset = CardId("nico_campaign".to_string());
        let installed = || {
            vec![
                InstalledCard {
                    card: agenda.clone(),
                    server: ServerId::Remote(0),
                    advancement_tokens: 3,
                    install_id: crate::rules::InstallId(1),
                    ..Default::default()
                },
                InstalledCard {
                    card: asset.clone(),
                    server: ServerId::Remote(1),
                    rezzed: true,
                    counters: 9,
                    install_id: crate::rules::InstallId(2),
                    ..Default::default()
                },
            ]
        };

        // Steal off the top of R&D: the deck copy goes, the remote copy stays.
        let mut state = game_state(Vec::new(), vec![agenda.clone()], Vec::new(), installed(), 0);
        state.active_run = Some(run_in_success(ServerId::RnD));
        accesses(&mut state, ServerId::RnD, &registry);
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert!(state.corp.r_and_d.is_empty(), "the R&D copy was stolen");
        assert_eq!(state.corp.installed.len(), 2, "both installs untouched: {:?}", state.corp.installed);
        assert_eq!(state.corp.installed[0].advancement_tokens, 3);

        // Trash out of HQ: the hand copy goes to Archives, the rezzed remote
        // copy keeps its counters.
        let mut state = game_state(vec![asset.clone()], Vec::new(), Vec::new(), installed(), 0);
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Hq));
        accesses(&mut state, ServerId::Hq, &registry);
        resolve_trash(&mut state, &asset, &registry).unwrap();
        assert!(state.corp.hq.is_empty(), "the HQ copy was trashed");
        assert_eq!(state.corp.archives.len(), 1);
        assert_eq!(state.corp.installed.len(), 2, "{:?}", state.corp.installed);
        assert_eq!(state.corp.installed[1].counters, 9);
    }

    /// Jinteki: Personal Evolution's shape — the steal succeeds, the
    /// identity's reaction flatlines the Runner. `GameOver` used to be
    /// emitted twice on this path (once by the flatline, once by
    /// `advance_or_finish`'s `finish_if_game_over`, which could not see the
    /// first); `win::end_game` is now the only emitter.
    #[test]
    fn stealing_an_agenda_whose_identity_reaction_flatlines_emits_game_over_once() {
        let identity = CardDefinition {
            id: CardId("jinteki_pe_ish".to_string()),
            title: "PE".to_string(),
            side: Side::Corp,
            card_type: CardType::Identity,
            triggers: vec![crate::dsl::TriggeredEffect {
                subject: None, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_heap: false, from_runner_score_area: false,
                text: None,
                trigger: crate::dsl::Trigger::OnAgendaStolen,
                effects: vec![Effect::DealDamage(crate::dsl::DamageType::Net, 1)],
                requirement: None,
            }],
            ..Default::default()
        };
        let registry = CardRegistry::from_cards(vec![agenda_card("offworld_office", 2), identity]);
        let agenda = CardId("offworld_office".to_string());
        let mut state = game_state(vec![agenda.clone()], Vec::new(), Vec::new(), Vec::new(), 0);
        state.corp.identity = Some(CardId("jinteki_pe_ish".to_string()));
        assert!(state.runner.grip.is_empty());
        state.active_run = Some(run_in_success(ServerId::Hq));
        accesses(&mut state, ServerId::Hq, &registry);

        let events = resolve_steal(&mut state, &agenda, &registry).unwrap();

        assert_eq!(state.runner.scored_agendas.iter().map(|scored| scored.card.clone()).collect::<Vec<_>>(), vec![agenda], "the steal itself stands");
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(events.iter().filter(|e| matches!(e, GameEvent::GameOver { .. })).count(), 1, "{events:?}");
        assert!(state.active_run.is_none());
        assert!(events.contains(&GameEvent::RunCompleted { server: ServerId::Hq }));
    }

    /// ROADMAP Rules Audit T12: Archives was the one server whose root
    /// installs were not accessed, while `install_card_candidates` offered
    /// upgrades onto it — so an upgrade there could never be trashed.
    #[test]
    fn an_upgrade_in_archives_root_is_accessed_alongside_the_archived_cards() {
        let registry = CardRegistry::from_cards(vec![trashable_card("manegarm_skunkworks", 2)]);
        let upgrade = CardId("manegarm_skunkworks".to_string());
        let archived = CardId("hedge_fund".to_string());
        let installed = vec![InstalledCard {
            card: upgrade.clone(),
            server: ServerId::Archives,
            slot: InstallSlot::Root,
            rezzed: true,
            ..Default::default()
        }];
        let mut state = game_state(Vec::new(), Vec::new(), vec![archived.clone()], installed, 0);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let access = state.active_run.as_ref().unwrap().access_state.as_ref().expect("two cards to access");
        let AccessPhase::SelectNextCard { selectable_cards } = &access.phase else {
            panic!("two accessed cards should present a selection, got {:?}", access.phase);
        };
        let root = state.corp.installed[0].install_id;
        assert_eq!(selectable_cards, &vec![AccessCandidate::Archived(archived), AccessCandidate::Root(root)]);
        let _ = upgrade;
    }

    /// CR 7.1.5b: "The Runner cannot trash or pay the trash cost of a card
    /// in the Corp's discard pile, either with the basic trash ability or
    /// with other mid-access abilities." The trash was offered, and paid
    /// for, with nothing moving; Carnivore's free trash took it too.
    #[test]
    fn a_card_in_the_discard_pile_cannot_be_trashed_on_access() {
        let registry = CardRegistry::from_cards(vec![trashable_card("pad_campaign", 2)]);
        let archived = CardId("pad_campaign".to_string());
        let mut state = game_state(Vec::new(), Vec::new(), vec![archived.clone()], Vec::new(), 0);
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let access = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert!(
            matches!(&access.phase, AccessPhase::PendingChoice { trash_cost: None, .. }),
            "no trash cost is offered on a card in Archives, got {:?}",
            access.phase
        );
        assert_eq!(resolve_trash(&mut state, &archived, &registry), Err(RulesError::NotInAccessPhase));
        assert_eq!(
            trash_currently_accessed_card_without_cost(&mut state, &registry),
            Err(RulesError::CannotTrashFromArchives { card: archived })
        );
        assert_eq!(state.runner.resources.credits, Credits(5));
    }

    /// The rule is about the discard pile, not the server: an upgrade in
    /// Archives' root is installed, and trashed like any other.
    #[test]
    fn an_upgrade_in_archives_root_can_still_be_trashed() {
        let registry = CardRegistry::from_cards(vec![trashable_card("manegarm_skunkworks", 2)]);
        let upgrade = CardId("manegarm_skunkworks".to_string());
        let installed = vec![InstalledCard {
            card: upgrade.clone(),
            server: ServerId::Archives,
            slot: InstallSlot::Root,
            rezzed: true,
            ..Default::default()
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        resolve_trash(&mut state, &upgrade, &registry).expect("an installed upgrade is trashable");
        assert!(state.corp.installed.is_empty());
        assert_eq!(state.runner.resources.credits, Credits(3));
    }

    #[test]
    fn stealing_an_agenda_that_reaches_seven_points_ends_the_game_with_a_runner_win() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", 3),
            agenda_card("already_scored", 4),
        ]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("priority_requisition".to_string())],
            Vec::new(),
            0,
        );
        // Simulate having already stolen 4 points' worth of Agendas earlier
        // in the game.
        state.runner.scored_agendas = vec![crate::rules::ScoredAgenda::plain(CardId("already_scored".to_string()))];
        state.runner.resources.agenda_points = AgendaPoints(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("priority_requisition".to_string());
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(7));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::GameOver { winner: Side::Runner },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn winning_mid_sequence_never_presents_the_next_accessed_card() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", 3),
            agenda_card("hostile_takeover", 1),
            agenda_card("already_scored", 4),
        ]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![
                CardId("priority_requisition".to_string()),
                CardId("hostile_takeover".to_string()),
            ],
            Vec::new(),
            0,
        );
        state.runner.scored_agendas = vec![crate::rules::ScoredAgenda::plain(CardId("already_scored".to_string()))];
        state.runner.resources.agenda_points = AgendaPoints(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("priority_requisition".to_string());
        resolve_select_card(&mut state, &AccessCandidate::Archived(card_id.clone()), &registry).expect("selecting should succeed");
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        // Capped at the winning threshold, not 8 — the second agenda
        // (worth 1 more point) was never reached, and never presented.
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(7));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::GameOver { winner: Side::Runner },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn costed_agenda_can_be_stolen_by_paying_its_steal_cost() {
        let registry = CardRegistry::from_cards(vec![costed_agenda_card("napd_contract", 2, 4)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("napd_contract".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("napd_contract".to_string());
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        assert_eq!(state.runner.resources.credits, Credits(0));
        assert_eq!(state.runner.scored_agendas.iter().map(|scored| scored.card.clone()).collect::<Vec<_>>(), vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 4 },
                GameEvent::AgendaStolen { card: card_id, agenda_points: 2 },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn costed_agenda_can_be_declined_when_unaffordable() {
        let registry = CardRegistry::from_cards(vec![costed_agenda_card("napd_contract", 2, 4)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("napd_contract".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(2);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("napd_contract".to_string());
        assert_eq!(
            resolve_steal(&mut state, &card_id, &registry),
            Err(RulesError::CannotAffordStealCost { card: card_id.clone(), available: 2, requested: 4 })
        );
        // Declining is legal — this Agenda isn't a mandatory steal.
        let events = resolve_pass(&mut state, &card_id, &registry).expect("passing should succeed");

        assert!(state.runner.scored_agendas.is_empty());
        assert_eq!(state.runner.resources.credits, Credits(2));
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn trashing_an_installed_asset_pays_its_trash_cost_and_moves_it_to_archives() {
        let registry = CardRegistry::from_cards(vec![trashable_card("pad_campaign", 2)]);
        let installed = vec![InstalledCard {
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(3);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        accesses(&mut state, ServerId::Remote(0), &registry);

        let card_id = CardId("pad_campaign".to_string());
        let events = resolve_trash(&mut state, &card_id, &registry).expect("trash should succeed");

        assert_eq!(state.runner.resources.credits, Credits(1));
        assert!(state.corp.installed.is_empty());
        // Accessed and trashed by the Runner, so it lands faceup.
        assert_eq!(state.corp.archives, vec![ArchivedCard::faceup(card_id.clone())]);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 2 },
                GameEvent::CardTrashedFromAccess { card: card_id, cost_paid: 2, install: Some(crate::rules::state::InstallId(0)) },
                GameEvent::RunCompleted { server: ServerId::Remote(0) },
            ]
        );
    }

    #[test]
    fn trashing_with_insufficient_credits_errors() {
        let registry = CardRegistry::from_cards(vec![trashable_card("pad_campaign", 2)]);
        let installed = vec![InstalledCard {
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(1);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        accesses(&mut state, ServerId::Remote(0), &registry);

        let card_id = CardId("pad_campaign".to_string());
        assert_eq!(
            resolve_trash(&mut state, &card_id, &registry),
            Err(RulesError::CannotAffordTrashCost { card: card_id, available: 1, requested: 2 })
        );
        assert_eq!(state.corp.installed.len(), 1);
    }

    #[test]
    fn passing_a_non_agenda_non_trashable_card_completes_the_run() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        accesses(&mut state, ServerId::Hq, &registry());

        let card_id = CardId("hedge_fund".to_string());
        let events = resolve_pass(&mut state, &card_id, &registry()).expect("passing should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn multi_card_sequence_advances_through_each_card_in_order() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        let first = accesses(&mut state, ServerId::Archives, &registry());
        assert_eq!(first, vec![GameEvent::ArchivesTurnedFaceup { count: 2 }]);
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![archived("hedge_fund"), archived("ice_wall")]
            }
        );

        // Pick the second card first — order is the Runner's choice, not
        // the fixed access-determination order.
        let selected = resolve_select_card(&mut state, &archived("ice_wall"), &registry())
            .expect("selecting the second card should succeed");
        assert_eq!(
            selected,
            vec![GameEvent::CardAccessed {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Archives,
                install: None,
            }]
        );

        let second = resolve_pass(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("passing the selected card should succeed");
        // Only one card remains, so it auto-presents rather than offering
        // another `SelectNextCard` choice.
        assert_eq!(
            second,
            vec![
                GameEvent::AccessPassed { card: CardId("ice_wall".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Archives,
                    install: None,
                },
            ]
        );
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::AccessingCard);

        let last = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry())
            .expect("passing the last card should succeed");
        assert_eq!(
            last,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn multi_card_selection_lets_runner_pick_the_second_card_first() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry());

        let events = resolve_select_card(&mut state, &archived("ice_wall"), &registry())
            .expect("selecting the second card should succeed");
        assert_eq!(
            events,
            vec![GameEvent::CardAccessed {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Archives,
                install: None,
            }]
        );
        let access_state = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(access_state.candidates, vec![archived("hedge_fund")]);
        assert_eq!(
            access_state.phase,
            AccessPhase::PendingChoice {
                card_id: CardId("ice_wall".to_string()),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None, trash_also: None,
            }
        );

        let resolved = resolve_pass(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("passing the selected card should succeed");
        assert_eq!(
            resolved,
            vec![
                GameEvent::AccessPassed { card: CardId("ice_wall".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Archives,
                    install: None,
                },
            ]
        );
    }

    #[test]
    fn three_card_access_supports_out_of_order_resolution() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![
                CardId("card_1".to_string()),
                CardId("card_2".to_string()),
                CardId("card_3".to_string()),
            ],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry());

        resolve_select_card(&mut state, &archived("card_3"), &registry())
            .expect("selecting card_3 should succeed");
        resolve_pass(&mut state, &CardId("card_3".to_string()), &registry())
            .expect("passing card_3 should succeed");

        // Two cards remain, so the Runner is offered another choice rather
        // than auto-advancing.
        let access_state = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![archived("card_1"), archived("card_2")]
            }
        );
        assert_eq!(access_state.resolved_cards, vec![CardId("card_3".to_string())]);

        resolve_select_card(&mut state, &archived("card_1"), &registry())
            .expect("selecting card_1 should succeed");
        let resolved = resolve_pass(&mut state, &CardId("card_1".to_string()), &registry())
            .expect("passing card_1 should succeed");

        // Only card_2 remains, so it auto-presents.
        assert_eq!(
            resolved,
            vec![
                GameEvent::AccessPassed { card: CardId("card_1".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("card_2".to_string()),
                    server: ServerId::Archives,
                    install: None,
                },
            ]
        );
    }

    #[test]
    fn selecting_the_final_remaining_card_bypasses_select_next_card() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry());
        resolve_select_card(&mut state, &archived("hedge_fund"), &registry())
            .expect("selecting should succeed");

        let events = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry())
            .expect("passing should succeed");

        // With exactly one card left in `unaccessed_cards`, it goes
        // straight to `PendingChoice` instead of another `SelectNextCard`.
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("ice_wall".to_string()),
                    server: ServerId::Archives,
                    install: None,
                },
            ]
        );
        assert!(matches!(
            state.active_run.unwrap().access_state.unwrap().phase,
            AccessPhase::PendingChoice { .. }
        ));
    }

    #[test]
    fn selecting_a_card_not_in_selectable_cards_errors() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry());

        let wrong = archived("wrong_card");
        assert_eq!(
            resolve_select_card(&mut state, &wrong, &registry()),
            Err(RulesError::InvalidAccessSelection { candidate: wrong })
        );
    }

    #[test]
    fn selecting_while_already_at_pending_choice_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        accesses(&mut state, ServerId::Hq, &registry());

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &AccessCandidate::Archived(card_id.clone()), &registry()),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn selecting_with_no_active_run_errors() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &AccessCandidate::Archived(card_id.clone()), &registry()),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn resolving_with_a_card_id_that_does_not_match_the_pending_card_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        accesses(&mut state, ServerId::Hq, &registry());

        let wrong_id = CardId("wrong_card".to_string());
        assert_eq!(resolve_pass(&mut state, &wrong_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn resolving_with_no_active_run_errors() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_pass(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn stealing_a_non_agenda_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        accesses(&mut state, ServerId::Hq, &registry());

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_steal(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn trashing_a_non_trashable_card_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        accesses(&mut state, ServerId::Hq, &registry());

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_trash(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn accessing_a_trap_card_deals_damage_via_on_accessed_trigger() {
        let registry =
            CardRegistry::from_cards(vec![card_with_on_accessed("snare", vec![Effect::DealDamage(DamageType::Net, 2)])]);
        let mut state =
            game_state(Vec::new(), Vec::new(), vec![CardId("snare".to_string())], Vec::new(), 0);
        state.runner.grip = vec![
            CardId("card_a".to_string()),
            CardId("card_b".to_string()),
            CardId("card_c".to_string()),
        ];
        state.active_run = Some(run_in_success(ServerId::Archives));

        let events = accesses(&mut state, ServerId::Archives, &registry);

        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(state.runner.heap.len(), 2);
        assert_eq!(
            events[0],
            GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives, install: None }
        );
        assert_eq!(
            events[1],
            GameEvent::TriggerFired { card: CardId("snare".to_string()), trigger: crate::dsl::Trigger::OnAccessed }
        );
        assert_eq!(events[2], GameEvent::AboutToResolve { what: crate::rules::WouldHappen::Damage { kind: DamageType::Net, amount: 2 } });
        assert_eq!(events[3], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2, responsible: Some(Side::Corp) });
        assert_eq!(events.last(), Some(&GameEvent::ArchivesTurnedFaceup { count: 1 }));
        // The two cards' trashes and their batch out of the grip.
        assert_eq!(events.len(), 8);
    }

    #[test]
    fn trashing_a_card_fires_on_trashed_from_access_trigger() {
        let registry = CardRegistry::from_cards(vec![trashable_card_with_on_trashed_from_access(
            "shock",
            2,
            vec![Effect::DealDamage(DamageType::Net, 1)],
        )]);
        let installed = vec![InstalledCard {
            card: CardId("shock".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(3);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        accesses(&mut state, ServerId::Remote(0), &registry);

        let card_id = CardId("shock".to_string());
        let events = resolve_trash(&mut state, &card_id, &registry).expect("trash should succeed");

        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(events[0], GameEvent::CreditsSpent { side: Side::Runner, amount: 2 });
        assert_eq!(
            events[1],
            GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: 2, install: Some(crate::rules::state::InstallId(0)) }
        );
        assert_eq!(
            events[2],
            GameEvent::TriggerFired { card: card_id.clone(), trigger: crate::dsl::Trigger::OnTrashedFromAccess }
        );
        assert_eq!(events[3], GameEvent::AboutToResolve { what: crate::rules::WouldHappen::Damage { kind: DamageType::Net, amount: 1 } });
        assert_eq!(events[4], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 1, responsible: Some(Side::Corp) });
        assert!(matches!(events[5], GameEvent::CardTrashed { side: Side::Runner, from: crate::dsl::TrashedFrom::Hand, by: Some(Side::Corp), .. }));
        assert!(matches!(&events[6], GameEvent::CardsTrashedFromGripOrStack { cards, by: Some(Side::Corp) } if cards.len() == 1), "the batch");
        assert_eq!(events[7], GameEvent::RunCompleted { server: ServerId::Remote(0) });
        assert_eq!(events.len(), 8);
    }

    #[test]
    fn on_trashed_from_access_trigger_does_not_fire_on_steal_or_pass() {
        // A trashable, non-Agenda card with an `OnTrashedFromAccess`
        // trigger — passing it (rather than trashing it) must not fire it.
        let registry = CardRegistry::from_cards(vec![trashable_card_with_on_trashed_from_access(
            "shock",
            2,
            vec![Effect::DealDamage(DamageType::Net, 5)],
        )]);
        let mut state =
            game_state(Vec::new(), Vec::new(), vec![CardId("shock".to_string())], Vec::new(), 0);
        state.runner.grip = vec![CardId("card_a".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("shock".to_string());
        let events = resolve_pass(&mut state, &card_id, &registry).expect("passing should succeed");

        // Had the trigger fired, 5 net damage against a 1-card grip would
        // have flatlined the Runner (and ended the game).
        assert_eq!(state.runner.grip.len(), 1);
        assert_ne!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn on_accessed_flatline_clears_active_run_and_halts_further_access() {
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "snare",
            vec![Effect::DealDamage(DamageType::Net, 5)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("snare".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            0,
        );
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let events = resolve_select_card(&mut state, &archived("snare"), &registry)
            .expect("selecting should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives, install: None },
                GameEvent::TriggerFired { card: CardId("snare".to_string()), trigger: crate::dsl::Trigger::OnAccessed },
                GameEvent::AboutToResolve { what: crate::rules::WouldHappen::Damage { kind: DamageType::Net, amount: 5 } },
                GameEvent::RunnerFlatlined,
                GameEvent::GameOver { winner: Side::Corp },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );

        // The second (never-presented) card's own trigger effects never ran
        // — no leftover access state to resolve against, so any further
        // access action now fails as "no active run" rather than
        // "not in access phase" for a card that's still nominally pending.
        assert_eq!(
            resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn on_accessed_flatline_via_advance_or_finish_auto_bypass() {
        // Two cards; the first has no trigger and is passed normally, which
        // auto-bypasses straight to the second (only one remains) via
        // `advance_or_finish`'s `1 =>` arm — the one hook point distinct
        // from `access_server`/`resolve_select_card`.
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "snare",
            vec![Effect::DealDamage(DamageType::Net, 5)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("snare".to_string())],
            Vec::new(),
            0,
        );
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);
        resolve_select_card(&mut state, &archived("hedge_fund"), &registry)
            .expect("selecting the first card should succeed");

        let events = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry)
            .expect("passing the first card should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives, install: None },
                GameEvent::TriggerFired { card: CardId("snare".to_string()), trigger: crate::dsl::Trigger::OnAccessed },
                GameEvent::AboutToResolve { what: crate::rules::WouldHappen::Damage { kind: DamageType::Net, amount: 5 } },
                GameEvent::RunnerFlatlined,
                GameEvent::GameOver { winner: Side::Corp },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn accessing_a_card_with_interactive_on_access_pauses_at_pending_interactive_trigger() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));

        let events = accesses(&mut state, ServerId::Archives, &registry);

        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("fetal_ai".to_string()), server: ServerId::Archives, install: None },
                GameEvent::ArchivesTurnedFaceup { count: 1 },
            ]
        );
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingInteractiveTrigger {
                card_id: CardId("fetal_ai".to_string()),
                cost: Cost::Credits(4),
                decider: Side::Runner,
                can_pay: true,
            }
        );
        // No damage taken yet — the effect hasn't resolved.
        assert_eq!(state.runner.grip.len(), 2);
    }

    #[test]
    fn pay_to_avoid_deducts_cost_skips_effects_and_proceeds_to_pending_choice() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("fetal_ai".to_string());
        let events = resolve_pay_access_trigger(&mut state, &card_id, &registry).expect("paying should succeed");

        assert_eq!(state.runner.resources.credits, Credits(1));
        // No damage — the effect was avoided.
        assert_eq!(state.runner.grip.len(), 2);
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: card_id.clone(),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None, trash_also: None,
            }
        );
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Runner, amount: 4 }]);

        // The normal choice is still reachable afterward.
        let pass_events = resolve_pass(&mut state, &card_id, &registry).expect("pass should succeed");
        assert_eq!(
            pass_events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn decline_to_avoid_applies_effects_and_proceeds_to_pending_choice() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("fetal_ai".to_string());
        let events =
            resolve_decline_access_trigger(&mut state, &card_id, &registry).expect("declining should succeed");

        // Credits untouched, but the 2 net damage landed.
        assert_eq!(state.runner.resources.credits, Credits(5));
        assert_eq!(state.runner.grip.len(), 0);
        assert_eq!(state.runner.heap.len(), 2);
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: card_id.clone(),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None, trash_also: None,
            }
        );
        assert_eq!(events[0], GameEvent::AboutToResolve { what: crate::rules::WouldHappen::Damage { kind: DamageType::Net, amount: 2 } });
        assert!(matches!(events[1], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2, responsible: Some(Side::Corp) }));
    }

    #[test]
    fn pay_to_avoid_with_insufficient_credits_errors_and_leaves_state_untouched() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(2);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("fetal_ai".to_string());
        assert_eq!(
            resolve_pay_access_trigger(&mut state, &card_id, &registry),
            Err(RulesError::CannotAffordAccessTriggerCost { card: card_id.clone(), available: 2, requested: 4 })
        );

        // Untouched: still credits 2, still pending the same interactive trigger.
        assert_eq!(state.runner.resources.credits, Credits(2));
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingInteractiveTrigger { card_id, cost: Cost::Credits(4), decider: Side::Runner, can_pay: false }
        );
    }

    #[test]
    fn resolving_interactive_trigger_actions_against_the_wrong_state_errors_not_in_access_phase() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("fetal_ai".to_string());

        assert_eq!(
            resolve_pay_access_trigger(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );
        assert_eq!(
            resolve_decline_access_trigger(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );

        // Also errors when a *different* card is actually pending.
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let wrong_card = CardId("not_pending".to_string());
        assert_eq!(
            resolve_pay_access_trigger(&mut state, &wrong_card, &registry),
            Err(RulesError::NotInAccessPhase)
        );
        assert_eq!(
            resolve_decline_access_trigger(&mut state, &wrong_card, &registry),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn decline_to_avoid_flatlining_ends_the_game_and_skips_pending_choice() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 5)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(0);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("fetal_ai".to_string());
        let events =
            resolve_decline_access_trigger(&mut state, &card_id, &registry).expect("declining should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::AboutToResolve { what: crate::rules::WouldHappen::Damage { kind: DamageType::Net, amount: 5 } },
                GameEvent::RunnerFlatlined,
                GameEvent::GameOver { winner: Side::Corp },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    /// **The Corp cannot be asked to pay for a card their own view will
    /// not name.** Snare! and Byte! ask the Corp, and both refuse to fire
    /// on Archives, so the question only ever lands where the base
    /// masking rule hides the card from them. Out of R&D there is nowhere
    /// else in the Corp's view the card appears — HQ they can read, their
    /// own installs they can read, R&D is a count — so before the mask
    /// learned about the decider, `legal_actions_for` handed the Corp
    /// `PayAccessTrigger { card_id }` naming a card `ClientView` blanked.
    ///
    /// This is the deterministic version of the sweep's
    /// `assert_actions_name_only_what_the_view_shows`: the sweep reaches
    /// it only when a Runner breaches R&D onto the one Byte! in three of
    /// sixteen Corp decks, which 32 seeds do not manage.
    #[test]
    fn the_corp_is_named_the_rnd_card_it_is_asked_to_pay_for() {
        use crate::rules::{legal_actions_for, PlayerAction, PublicAccessPhase};
        use crate::view::build_client_view;

        let registry = CardRegistry::from_cards(vec![CardDefinition {
            interactive_on_access: Some(InteractiveOnAccess {
                cost: Cost::Credits(4),
                effects: vec![Effect::GiveTags(crate::dsl::Amount::Fixed(1))],
                interaction: AccessInteraction::CorpPaysToApply,
                requirement: None,
            }),
            trash_cost: None,
            ..trashable_card("byte", 0)
        }]);
        let card = CardId("byte".to_string());
        // The card is in R&D and nowhere else: not in HQ, not installed,
        // not in Archives.
        let mut state = game_state(Vec::new(), vec![card.clone()], Vec::new(), Vec::new(), 0);
        state.corp.resources.credits = Credits(8);
        state.phase = crate::rules::state::GamePhase::Action(Side::Runner);
        state.active_run = Some(run_in_success(ServerId::RnD));

        accesses(&mut state, ServerId::RnD, &registry);
        let phase = &state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase;
        assert!(
            matches!(phase, AccessPhase::PendingInteractiveTrigger { decider: Side::Corp, .. }),
            "the Corp should be the one parked on the decision: {phase:?}"
        );

        // The Corp's own actions name the card...
        let actions = legal_actions_for(&state, &registry, Side::Corp);
        assert!(
            actions.iter().any(|a| matches!(a, PlayerAction::PayAccessTrigger { card_id } if *card_id == card)),
            "the Corp is offered the payment: {actions:?}"
        );
        // ...so their view has to name it too, or the panel rendering the
        // decision cannot say what is being paid for.
        let view = build_client_view(&state, &registry, Side::Corp);
        let masked = &view.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase;
        assert!(
            matches!(masked, PublicAccessPhase::PendingInteractiveTrigger { card: Some(id), .. } if *id == card),
            "the Corp's view must name the card their own actions name: {masked:?}"
        );

        // The Runner, who is accessing it, sees it as they always did; and
        // a spectator is asked nothing and told nothing.
        let runner = build_client_view(&state, &registry, Side::Runner);
        let runner_phase = &runner.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase;
        assert!(matches!(runner_phase, PublicAccessPhase::PendingInteractiveTrigger { card: Some(id), .. } if *id == card));
        let spectator = build_client_view(&state, &registry, crate::rules::Viewer::Spectator);
        let spectator_phase = &spectator.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase;
        assert!(
            matches!(spectator_phase, PublicAccessPhase::PendingInteractiveTrigger { card: None, .. }),
            "a spectator decides nothing, so learns nothing: {spectator_phase:?}"
        );
    }

    #[test]
    fn ordinary_on_accessed_cards_are_unaffected_by_the_interactive_trigger_refactor() {
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "snare",
            vec![Effect::GiveTags(crate::dsl::Amount::Fixed(1))],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("snare".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));

        let events = accesses(&mut state, ServerId::Archives, &registry);

        assert_eq!(state.runner.tags, 1, "OnAccessed still fires unconditionally for non-interactive cards");
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: CardId("snare".to_string()),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None, trash_also: None,
            }
        );
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives, install: None },
                GameEvent::TriggerFired { card: CardId("snare".to_string()), trigger: crate::dsl::Trigger::OnAccessed },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1, had: 0 },
                GameEvent::ArchivesTurnedFaceup { count: 1 },
            ]
        );
    }

    /// A minimal non-Agenda, non-trashable Asset `CardDefinition` with both an
    /// `InteractiveOnAccess` trigger and a normal `OnAccessed` trigger —
    /// proves the two compose (the normal trigger still fires once the
    /// interactive decision resolves).
    fn card_with_interactive_and_on_accessed(
        id: &str,
        cost: Cost,
        avoided_effects: Vec<Effect>,
        on_accessed_effects: Vec<Effect>,
    ) -> CardDefinition {
        CardDefinition {
            interactive_on_access: Some(InteractiveOnAccess { cost, effects: avoided_effects, interaction: AccessInteraction::default(), requirement: None }),
            triggers: vec![TriggeredEffect { subject: Some(crate::dsl::Subject::This), when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_heap: false, from_runner_score_area: false, text: None, trigger: Trigger::OnAccessed, effects: on_accessed_effects, requirement: None }],
            trash_cost: None,
            ..trashable_card(id, 0)
        }
    }

    #[test]
    fn interactive_on_access_composes_with_a_normal_on_accessed_trigger() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_and_on_accessed(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
            vec![Effect::GiveTags(crate::dsl::Amount::Fixed(1))],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("fetal_ai".to_string());
        let events = resolve_pay_access_trigger(&mut state, &card_id, &registry).expect("paying should succeed");

        // The avoided damage never landed, but the normal OnAccessed trigger
        // still fired once the interactive decision resolved.
        assert_eq!(state.runner.resources.credits, Credits(1));
        assert_eq!(state.runner.tags, 1);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 4 },
                GameEvent::TriggerFired { card: CardId("fetal_ai".to_string()), trigger: crate::dsl::Trigger::OnAccessed },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1, had: 0 },
            ]
        );
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None, trash_also: None,
            }
        );
    }

    /// An Agenda (see `agenda_card`) with an `InteractiveOnAccess` trigger —
    /// Fetal AI's actual card shape (a damage trap that's also an Agenda).
    fn agenda_with_interactive_on_access(id: &str, points: u32, cost: Cost, effects: Vec<Effect>) -> CardDefinition {
        CardDefinition {
            interactive_on_access: Some(InteractiveOnAccess { cost, effects, interaction: AccessInteraction::default(), requirement: None }),
            ..agenda_card(id, points)
        }
    }

    #[test]
    fn interactive_on_access_composes_with_mandatory_steal_on_an_agenda() {
        let registry = CardRegistry::from_cards(vec![agenda_with_interactive_on_access(
            "fetal_ai",
            2,
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(0);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        let card_id = CardId("fetal_ai".to_string());
        // Can't afford to pay — decline, taking the damage.
        resolve_decline_access_trigger(&mut state, &card_id, &registry).expect("declining should succeed");
        assert_eq!(state.runner.grip.len(), 0);
        assert_eq!(state.runner.heap.len(), 2);

        // The normal Agenda choice is still reachable afterward, and is a
        // mandatory steal (no steal_cost).
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: card_id.clone(),
                trash_cost: None,
                mandatory_steal: true,
                steal_cost: None, trash_also: None,
            }
        );
        assert_eq!(
            resolve_pass(&mut state, &card_id, &registry),
            Err(RulesError::MandatoryStealViolation { card: card_id.clone() })
        );

        let events = resolve_steal(&mut state, &card_id, &registry).expect("stealing should succeed");
        assert_eq!(state.runner.scored_agendas.iter().map(|scored| scored.card.clone()).collect::<Vec<_>>(), vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 2 },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn interactive_on_access_on_the_second_of_a_multi_card_access() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Archives));
        accesses(&mut state, ServerId::Archives, &registry);

        // Pick the plain card first, then pass it — auto-advancing to the
        // second (and last) card, which carries the interactive trigger.
        resolve_select_card(&mut state, &archived("hedge_fund"), &registry)
            .expect("selecting should succeed");
        let events = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry)
            .expect("passing should succeed");

        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::CardAccessed { card: CardId("fetal_ai".to_string()), server: ServerId::Archives, install: None },
            ]
        );
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingInteractiveTrigger {
                card_id: CardId("fetal_ai".to_string()),
                cost: Cost::Credits(4),
                decider: Side::Runner,
                can_pay: true,
            }
        );
    }

    #[test]
    fn self_trashing_trap_trashes_itself_exactly_once_without_breaking_the_access_loop() {
        // HQ (not Archives) — the card must actually move zones (hq ->
        // archives) for the self-trash to be observable at all.
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "shock_ish",
            vec![Effect::GiveTags(crate::dsl::Amount::Fixed(1)), Effect::TrashCard(CardTarget::ThisCard)],
        )]);
        let mut state = game_state(
            vec![CardId("shock_ish".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));

        let events = accesses(&mut state, ServerId::Hq, &registry);

        assert_eq!(state.runner.tags, 1);
        assert!(state.corp.hq.is_empty());
        // Exactly one copy — not duplicated by a stale PendingChoice being
        // acted on afterward.
        assert_eq!(state.corp.archives, vec![ArchivedCard::faceup(CardId("shock_ish".to_string()))]);
        assert_eq!(state.active_run, None, "the run should complete, not hang on a phantom PendingChoice");
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("shock_ish".to_string()), server: ServerId::Hq, install: None },
                GameEvent::TriggerFired { card: CardId("shock_ish".to_string()), trigger: crate::dsl::Trigger::OnAccessed },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1, had: 0 },
                GameEvent::CardTrashed { side: Side::Corp, card: CardId("shock_ish".to_string()), from: crate::dsl::TrashedFrom::Hand, by: Some(Side::Corp), install: None },
                GameEvent::RunCompleted { server: ServerId::Hq },
            ]
        );
    }

    /// Two copies of one upgrade in a root are two instances. Each pick
    /// pins one; trashing takes exactly that one, and the next pick pins
    /// the other. Before `pending_install`, both trashes removed the
    /// lower-indexed copy and the second one stayed installed forever.
    #[test]
    fn two_copies_of_one_upgrade_in_a_root_are_trashed_as_two_instances() {
        let upgrade = CardId("skunkworks".to_string());
        let registry = CardRegistry::from_cards(vec![trashable_card("skunkworks", 0)]);
        let installed = vec![
            InstalledCard { card: upgrade.clone(), install_id: InstallId(1), server: ServerId::Remote(0), ..Default::default() },
            InstalledCard { card: upgrade.clone(), install_id: InstallId(2), server: ServerId::Remote(0), ..Default::default() },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        accesses(&mut state, ServerId::Remote(0), &registry);

        // Both offered, each as its own install.
        resolve_select_card(&mut state, &AccessCandidate::Root(InstallId(1)), &registry).unwrap();
        let access = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(access.pending_install, Some(InstallId(1)));

        resolve_trash(&mut state, &upgrade, &registry).unwrap();
        assert_eq!(state.corp.installed.len(), 1, "one copy left");
        assert_eq!(state.corp.installed[0].install_id, InstallId(2), "the pinned instance is the one that left");
        let access = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(access.pending_install, Some(InstallId(2)), "the last card was auto-presented as the other instance");

        resolve_trash(&mut state, &upgrade, &registry).unwrap();
        assert!(state.corp.installed.is_empty(), "both instances trashed, none duplicated");
        assert_eq!(state.corp.archives.len(), 2);
    }

    /// `OnAccessed` fires against the instance being accessed: an
    /// `AddCounters` trap installed twice puts one counter on *each* copy,
    /// where the by-`CardId` lookup put both on the first.
    #[test]
    fn on_accessed_fires_against_the_instance_being_accessed() {
        let trap = CardId("counter_trap".to_string());
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed("counter_trap", vec![Effect::AddCounters(1)])]);
        let installed = vec![
            InstalledCard { card: trap.clone(), install_id: InstallId(1), server: ServerId::Remote(0), rezzed: true, ..Default::default() },
            InstalledCard { card: trap.clone(), install_id: InstallId(2), server: ServerId::Remote(0), rezzed: true, ..Default::default() },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        accesses(&mut state, ServerId::Remote(0), &registry);

        let events = resolve_select_card(&mut state, &AccessCandidate::Root(InstallId(1)), &registry).unwrap();
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::CardAccessed { install: Some(InstallId(1)), .. })),
            "{events:?}"
        );
        resolve_pass(&mut state, &trap, &registry).unwrap();
        let counters: Vec<(InstallId, u32)> = state.corp.installed.iter().map(|c| (c.install_id, c.counters)).collect();
        assert_eq!(counters, vec![(InstallId(1), 1), (InstallId(2), 1)], "one counter on each instance");
    }

    /// A card accessed out of HQ is a zone card: no instance to pin, and
    /// the by-`CardId` removal path is the one that runs.
    #[test]
    fn an_accessed_zone_card_has_no_install() {
        let agenda = CardId("agenda".to_string());
        let registry = CardRegistry::from_cards(vec![agenda_card("agenda", 2)]);
        let mut state = game_state(vec![agenda.clone()], Vec::new(), Vec::new(), Vec::new(), 0);
        state.active_run = Some(run_in_success(ServerId::Hq));
        let events = accesses(&mut state, ServerId::Hq, &registry);
        assert!(events.iter().any(|e| matches!(e, GameEvent::CardAccessed { install: None, .. })), "{events:?}");
        assert_eq!(state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().pending_install, None);
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert!(state.corp.hq.is_empty());
    }
}
