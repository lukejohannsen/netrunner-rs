use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardType, Cost, HostedCreditUse};
use crate::rules::ability;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::state::{AccessPhase, AccessState, RunPhase, ServerId};
use crate::rules::state::{ArchivedCard, GameState, InstallId, InstallSlot, Side};
use crate::rules::win::check_win_conditions;

/// Root (non-ICE) installs on `server` — ICE is excluded via
/// `InstalledCard::slot`, which the installing action declares explicitly
/// (see `InstallSlot`'s doc comment for why this doesn't need a full
/// `CardRegistry`). A successful run accesses these alongside whatever else
/// that server's arm below yields, since Upgrades can be installed on
/// central servers (Hq/RnD) as well as Remote ones.
fn root_installs_on(state: &GameState, server: ServerId) -> Vec<CardId> {
    state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.server == server && installed.slot == InstallSlot::Root)
        .map(|installed| installed.card.clone())
        .collect()
}

/// Determine which `CardId`s become accessible when a run against `server`
/// concludes successfully.
fn compute_accessed_cards(state: &mut GameState, server: ServerId) -> Vec<CardId> {
    match server {
        // Real rules access one *randomly* chosen HQ card, plus one more
        // per `RunState::additional_hq_access` (`Effect::AddAdditionalAccess`).
        // `next_u64` is `GameState`'s deterministic pseudo-random source (no
        // external RNG, per AGENTS.md's purity requirement) — each roll is
        // reduced modulo the shrinking pool's length to pick a distinct
        // index, mirroring `damage::apply_damage`'s "pick N distinct random
        // elements" idiom.
        ServerId::Hq => {
            let additional = state.active_run.as_ref().map_or(0, |run| run.additional_hq_access);
            let take = (1 + additional as usize).min(state.corp.hq.len());
            let mut pool = state.corp.hq.clone();
            let mut accessed = Vec::with_capacity(take);
            for _ in 0..take {
                let roll = state.next_u64();
                let index = (roll as usize) % pool.len();
                accessed.push(pool.remove(index));
            }
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        // Real rules access one card too, but R&D isn't randomized — it's
        // drawn from a fixed deck order, plus one more per
        // `RunState::additional_rd_access`. `.rev()` walks from the end (top
        // of deck, per `RunnerState::stack`'s "top of deck is the end of the
        // Vec" convention — see `engine.rs::draw_card_click`'s `stack.pop()`)
        // backward, so `.take(n)` yields the top `n` cards in top-to-bottom
        // order.
        ServerId::RnD => {
            let additional = state.active_run.as_ref().map_or(0, |run| run.additional_rd_access);
            let take = (1 + additional as usize).min(state.corp.r_and_d.len());
            let mut accessed: Vec<CardId> = state.corp.r_and_d.iter().rev().take(take).cloned().collect();
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        // A successful run accesses everything in Archives, facedown
        // cards included — accessing them is exactly how the Runner sees
        // them — plus, as on every other server, any upgrade in its root.
        // The root was missing here alone, while `install_card_candidates`
        // offered upgrades onto Archives: an upgrade installed there could
        // never be accessed or trashed (ROADMAP Rules Audit T12).
        ServerId::Archives => {
            // Breaching Archives turns every facedown card there faceup —
            // the Runner has now seen them, and they stay public afterwards
            // (Null Signal Games rules: cards in Archives are faceup once
            // accessed). Nothing flipped them before, so a card the Runner
            // had just been shown was masked from them again the moment the
            // run ended, and *Jinteki: Restoring Humanity* kept paying for
            // "facedown" cards the Runner had read (ROADMAP Rules Audit,
            // Tier 2). Done here, at breach, rather than per card: the
            // whole zone is accessed at once.
            for archived in state.corp.archives.iter_mut() {
                archived.facedown = false;
            }
            let mut accessed: Vec<CardId> = state.corp.archives.iter().map(|a| a.card.clone()).collect();
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        ServerId::Remote(_) => root_installs_on(state, server),
    }
}

/// Builds the `AccessPhase::PendingChoice` for `card_id`, from its
/// `CardRegistry` definition (or the "unrecognized card" defaults if it
/// isn't registered — nothing stealable or trashable, so the only legal
/// resolution is `PlayerAction::PassAccessedCard`).
/// The installed instance `card_id` resolves to on `server`, if it is a
/// root install there: the first copy not already resolved this breach.
/// `None` for a card accessed out of a hidden zone. See
/// `AccessState::pending_install`.
fn resolve_install(state: &GameState, server: ServerId, card_id: &CardId) -> Option<InstallId> {
    let resolved = state
        .active_run
        .as_ref()
        .and_then(|run| run.access_state.as_ref())
        .map(|access| access.resolved_installs.clone())
        .unwrap_or_default();
    state
        .corp
        .installed
        .iter()
        .find(|c| &c.card == card_id && c.server == server && c.slot == InstallSlot::Root && !resolved.contains(&c.install_id))
        .map(|c| c.install_id)
}

fn compute_pending_choice(state: &GameState, card_id: &CardId, server: ServerId, registry: &CardRegistry) -> AccessPhase {
    let card_def = registry.get(card_id);
    let is_agenda = card_def.is_some_and(|c| c.agenda_points.is_some());
    let steal_cost = card_def.and_then(|c| c.steal_cost.clone());
    let mandatory_steal = is_agenda && steal_cost.is_none();
    let trash_cost = card_def.and_then(|c| c.trash_cost).map(|printed| {
        if card_def.is_some_and(|c| c.card_type == CardType::Asset) {
            printed + root_asset_trash_cost_bonus(state, server, registry)
        } else {
            printed
        }
    });

    AccessPhase::PendingChoice { card_id: card_id.clone(), trash_cost, mandatory_steal, steal_cost }
}

/// What the rezzed root upgrades of `server` add to the trash cost of an
/// asset accessed there (`CardDefinition::root_asset_trash_cost_bonus`,
/// Mahkota Langit Grid), plus the same from any such upgrade the Runner
/// trashed earlier in this run — the "Persistent" half, read off
/// `RunState::persistent_trashed_upgrades`, which only ever records
/// upgrades trashed during a run against their own server. An asset in
/// R&D or HQ is never accessed, so a central's root is never consulted in
/// practice.
fn root_asset_trash_cost_bonus(state: &GameState, server: ServerId, registry: &CardRegistry) -> u32 {
    let installed: u32 = state
        .corp
        .installed
        .iter()
        .filter(|c| c.rezzed && c.server == server && c.slot == InstallSlot::Root)
        .filter_map(|c| registry.get(&c.card))
        .map(|def| def.root_asset_trash_cost_bonus)
        .sum();
    let persistent: u32 = state
        .active_run
        .as_ref()
        .filter(|run| run.server == server)
        .map(|run| run.persistent_trashed_upgrades.iter().filter_map(|card| registry.get(card)).map(|def| def.root_asset_trash_cost_bonus).sum())
        .unwrap_or(0);
    installed + persistent
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
    // self-trashing trap via `Effect::TrashCard(CardTarget::ThisCard)`) —
    // presenting a `PendingChoice` for a card that's already gone would let
    // the Runner "trash"/"steal" it a second time (`move_to_archives`
    // doesn't verify the card is still where it thinks, so this would
    // duplicate it into Archives). Treat a self-trash as this card's
    // resolution instead, same as an explicit `TrashAccessedCard`.
    if was_trashed(&events, card_id) {
        events.extend(advance_or_finish(state, registry, server, card_id.clone())?);
        return Ok(events);
    }

    let phase = compute_pending_choice(state, card_id, server, registry);
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
/// "a card is now being accessed" call site (`access_server`,
/// `resolve_select_card`, `advance_or_finish`) should use.
fn present_card_for_access(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    card_id: &CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    // Pin the instance first: every later step of this card's resolution
    // (its `OnAccessed` trigger, a trash, a steal) reads it from here.
    let install = resolve_install(state, server, card_id);
    let rezzed = install.and_then(|install| state.find_corp_install(install)).is_some_and(|c| c.rezzed);
    if let Some(access) = state.active_run.as_mut().and_then(|run| run.access_state.as_mut()) {
        access.pending_install = install;
        access.pending_install_rezzed = rezzed;
    }
    let mut events = vec![GameEvent::CardAccessed { card: card_id.clone(), server, install }];

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
        let can_pay = match &interactive.cost {
            Cost::Credits(amount) => state.resources(decider).credits.0 >= *amount,
            // Other cost kinds aren't precomputed elsewhere either
            // (`resolve_steal`'s `steal_cost` handling is the same) —
            // `resolve_pay_access_trigger`'s `ability::pay_cost` call
            // re-validates affordability for every `Cost` variant regardless.
            _ => true,
        };
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
            chooser: Side::Runner,
            options: vec![
                crate::dsl::Effect::Sequence(vec![effect, crate::dsl::Effect::EndTheRun]),
                crate::dsl::Effect::Sequence(Vec::new()),
            ],
            source_card: None,
            source_install: None,
            resume: crate::rules::state::PendingChoiceResume::None,
        });
        return Ok(Some(vec![GameEvent::PendingChoicePresented { chooser: Side::Runner, option_count: 2 }]));
    }

    let mut events = ability::evaluate_effect(state, &effect, &mut ability::ResolutionContext::for_card(None), registry)?;
    super::engine::end_run(state);
    events.push(GameEvent::AccessReplaced { server });
    Ok(Some(events))
}

pub fn access_server(
    state: &mut GameState,
    server: ServerId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    if let Some(events) = try_replace_access(state, server, registry)? {
        return Ok(events);
    }

    let accessed = compute_accessed_cards(state, server);
    if accessed.is_empty() {
        super::engine::end_run(state);
        return Ok(Vec::new());
    }

    let run = state
        .active_run
        .as_mut()
        .expect("engine::complete_run confirmed active_run is Some before calling access_server");
    run.phase = RunPhase::AccessingCard;
    run.cards_accessed_count = accessed.len() as u32;

    if accessed.len() == 1 {
        let card_id = accessed.into_iter().next().unwrap();
        // Placeholder phase — `present_card_for_access` below overwrites it
        // immediately (with either `PendingInteractiveTrigger` or, via
        // `enter_pending_choice`, the real `PendingChoice`). `AccessState`
        // must exist first since both paths borrow `run.access_state.as_mut()`.
        run.access_state = Some(AccessState { currently_accessing: None, pending_install: None, pending_install_rezzed: false, resolved_installs: Vec::new(),
            server,
            unaccessed_cards: Vec::new(),
            resolved_cards: Vec::new(),
            phase: AccessPhase::SelectNextCard { selectable_cards: Vec::new() },
        });

        present_card_for_access(state, registry, server, &card_id)
    } else {
        run.access_state = Some(AccessState { currently_accessing: None, pending_install: None, pending_install_rezzed: false, resolved_installs: Vec::new(),
            server,
            unaccessed_cards: accessed.clone(),
            resolved_cards: Vec::new(),
            phase: AccessPhase::SelectNextCard { selectable_cards: accessed },
        });
        Ok(Vec::new())
    }
}

/// The `AccessState` fields `resolve_steal`/`resolve_trash`/`resolve_pass`
/// need, pulled out by value so the borrow of `state.active_run` doesn't
/// outlive the check — each caller goes on to mutate `state` afterward.
struct PendingAccess {
    server: ServerId,
    mandatory_steal: bool,
    steal_cost: Option<Cost>,
    trash_cost: Option<u32>,
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
    let AccessPhase::PendingChoice { card_id: pending, mandatory_steal, steal_cost, trash_cost, .. } =
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
        install: access.pending_install,
    })
}

/// The `AccessState` fields `resolve_select_card` needs, pulled out by value
/// for the same borrow-scoping reason as `PendingAccess`.
struct PendingSelection {
    server: ServerId,
    selectable_cards: Vec<CardId>,
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
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_selectable(state)?;
    if !pending.selectable_cards.contains(card_id) {
        return Err(RulesError::InvalidAccessSelection { card: card_id.clone() });
    }

    let run = state.active_run.as_mut().expect("resolve_select_card called mid-access");
    let access = run.access_state.as_mut().expect("resolve_select_card called mid-access");
    if let Some(pos) = access.unaccessed_cards.iter().position(|c| c == card_id) {
        access.unaccessed_cards.remove(pos);
    }

    present_card_for_access(state, registry, pending.server, card_id)
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
        super::engine::end_run(state);
        Some(vec![GameEvent::RunCompleted { server }])
    } else {
        None
    }
}

/// Shared tail of `resolve_steal`/`resolve_trash`/`resolve_pass`: if a steal
/// just won the game, finalize immediately without presenting further
/// accessed cards; otherwise record `resolved_card` as resolved and either
/// auto-present the last remaining card's `PendingChoice`, offer a choice
/// among 2+ remaining cards, or finalize if none remain.
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
    if let Some(install) = access.pending_install.take() {
        access.resolved_installs.push(install);
    }

    match access.unaccessed_cards.len() {
        0 => {
            super::engine::end_run(state);
            let completed_event = GameEvent::RunCompleted { server };
            let mut events = vec![completed_event.clone()];
            events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &completed_event)?);
            Ok(events)
        }
        1 => {
            let next_card = access.unaccessed_cards.remove(0);
            present_card_for_access(state, registry, server, &next_card)
        }
        _ => {
            access.phase = AccessPhase::SelectNextCard { selectable_cards: access.unaccessed_cards.clone() };
            Ok(Vec::new())
        }
    }
}

/// Resolves `PlayerAction::StealAgenda`. See its doc comment for the error
/// conditions.
pub fn resolve_steal(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    if state.active_run.as_ref().is_some_and(|r| r.runner_cannot_steal_or_trash) {
        return Err(RulesError::StealAndTrashPreventedThisRun);
    }
    let pending = require_pending(state, card_id)?;
    if !pending.mandatory_steal && pending.steal_cost.is_none() {
        return Err(RulesError::NotInAccessPhase);
    }

    let mut events = Vec::new();
    if let Some(cost) = &pending.steal_cost {
        if let Cost::Credits(requested) = cost {
            let available = state.runner.resources.credits.0;
            if available < *requested {
                return Err(RulesError::CannotAffordStealCost {
                    card: card_id.clone(),
                    available,
                    requested: *requested,
                });
            }
        }
        events.extend(ability::pay_cost(state, Side::Runner, cost, Some(card_id))?);
    }

    // The agenda leaves the Corp's zone — HQ, the top of R&D, the remote's
    // root, or Archives — and enters the Runner's score area. This removal
    // was missing: the card was pushed onto `scored_agendas` and *also*
    // stayed where it was, so the same top card of R&D was stolen again on
    // the next run and an installed agenda stayed scorable after being
    // stolen (ROADMAP Rules Audit T2). `pending.server` is what tells two
    // copies of one agenda in two remotes apart.
    remove_from_corp_zone(state, card_id, pending.server, pending.install);
    state.runner.scored_agendas.push(card_id.clone());
    // Counted for `Trigger::OnRunEnded` consumers that gate on "if the
    // Runner stole any agendas during that run" (AMAZE Amusements), since
    // the `RunState` itself is gone by the time that trigger fires.
    if let Some(run) = state.active_run.as_mut() {
        run.agendas_stolen_this_run = run.agendas_stolen_this_run.saturating_add(1);
    }
    let agenda_points = registry.get(card_id).and_then(|c| c.agenda_points).unwrap_or(0);
    state.runner.resources.agenda_points = state.runner.resources.agenda_points.gain(agenda_points);
    let stolen_event = GameEvent::AgendaStolen { card: card_id.clone(), agenda_points };
    events.push(stolen_event.clone());
    // Jinteki: Personal Evolution-style identity reaction to a steal —
    // unconditional dispatch, no per-turn gate.
    events.extend(dispatcher::dispatch_event(state, registry, &stolen_event)?);

    events.extend(check_win_conditions(state, registry));
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
    Installed { slot: InstallSlot },
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
/// top (the end of the `Vec` — see `compute_accessed_cards`), since the
/// accessed copy is the top one and a duplicate may sit deeper. Archives
/// is included because an agenda *stolen* out of Archives leaves it, even
/// though a card *trashed* while being accessed there stays put.
fn remove_from_corp_zone(
    state: &mut GameState,
    card_id: &CardId,
    server: ServerId,
    install: Option<InstallId>,
) -> RemovedFrom {
    // The access pinned the exact instance: take that one and nothing
    // else. Two copies of one upgrade in a root used to both resolve to
    // the lower-indexed install (ROADMAP Rules Audit follow-ups).
    if let Some(install) = install
        && let Some(pos) = state.corp.installed.iter().position(|c| c.install_id == install)
    {
        let slot = state.corp.installed[pos].slot;
        state.corp.installed.remove(pos);
        return RemovedFrom::Installed { slot };
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
        let slot = state.corp.installed[pos].slot;
        state.corp.installed.remove(pos);
        return RemovedFrom::Installed { slot };
    }
    match server {
        ServerId::Hq => {
            if let Some(pos) = state.corp.hq.iter().position(|c| c == card_id) {
                state.corp.hq.remove(pos);
                return RemovedFrom::Hand;
            }
        }
        ServerId::RnD => {
            if let Some(pos) = state.corp.r_and_d.iter().rposition(|c| c == card_id) {
                state.corp.r_and_d.remove(pos);
                return RemovedFrom::Deck;
            }
        }
        ServerId::Archives => {
            if let Some(pos) = state.corp.archives.iter().position(|c| &c.card == card_id) {
                state.corp.archives.remove(pos);
                return RemovedFrom::Archives;
            }
        }
        ServerId::Remote(_) => {}
    }
    // Not where the server said — take it from wherever it actually is.
    if let Some(pos) = state.corp.hq.iter().position(|c| c == card_id) {
        state.corp.hq.remove(pos);
        RemovedFrom::Hand
    } else if let Some(pos) = state.corp.r_and_d.iter().rposition(|c| c == card_id) {
        state.corp.r_and_d.remove(pos);
        RemovedFrom::Deck
    } else {
        RemovedFrom::Nowhere
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
) {
    if server == ServerId::Archives && install.is_none() {
        return;
    }
    if let RemovedFrom::Installed { slot: InstallSlot::Root } = remove_from_corp_zone(state, card_id, server, install) {
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
    state.corp.archives.push(ArchivedCard::faceup(card_id.clone()));
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

    move_to_archives(state, registry, &card_id, server, install);
    let trashed_event = GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: 0 };
    let mut events = vec![trashed_event.clone()];
    events.extend(dispatcher::dispatch_event(state, registry, &trashed_event)?);
    events.extend(advance_or_finish(state, registry, server, card_id)?);
    Ok(events)
}

/// Resolves `PlayerAction::TrashAccessedCard`. See its doc comment for the
/// error conditions.
pub fn resolve_trash(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    if state.active_run.as_ref().is_some_and(|r| r.runner_cannot_steal_or_trash) {
        return Err(RulesError::StealAndTrashPreventedThisRun);
    }
    let pending = require_pending(state, card_id)?;
    let cost = pending.trash_cost.ok_or(RulesError::NotInAccessPhase)?;

    // Hosted credits a card lets the Runner spend on trash costs (Azimat's
    // `hosted_credits_usable_for: TrashCosts`) count towards affordability
    // and are drained first, rig order, before the wallet — the same
    // "pool before wallet" precedent `ability::pay_cost` applies to bad
    // publicity and bonus run credits. Drained here rather than inside
    // `pay_cost` because that waterfall is purpose-blind and these pools
    // are not; this is the one site that knows the purpose is a trash
    // cost. See `CardDefinition::hosted_credits_usable_for`.
    let pays_trash_costs = |def: &crate::dsl::CardDefinition| def.hosted_credits_usable_for == Some(HostedCreditUse::TrashCosts);
    let hosted: u32 = state
        .runner
        .rig
        .iter()
        .filter(|card| registry.get(&card.card).is_some_and(pays_trash_costs))
        .map(|card| card.counters)
        .sum();
    let available = state.runner.resources.credits.0.saturating_add(hosted);
    if available < cost {
        return Err(RulesError::CannotAffordTrashCost { card: card_id.clone(), available, requested: cost });
    }

    let (mut events, from_pools) = ability::drain_hosted_credit_pools(state, registry, cost, pays_trash_costs)?;
    events.extend(ability::pay_cost(state, Side::Runner, &Cost::Credits(cost - from_pools), Some(card_id))?);
    move_to_archives(state, registry, card_id, pending.server, pending.install);
    let trashed_event = GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: cost };
    events.push(trashed_event.clone());
    events.extend(dispatcher::dispatch_event(state, registry, &trashed_event)?);

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
    let steal_blocked = state.active_run.as_ref().is_some_and(|r| r.runner_cannot_steal_or_trash);
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
    if paid {
        if let Cost::Credits(requested) = &pending.cost {
            let available = state.resources(pending.decider).credits.0;
            if available < *requested {
                return Err(RulesError::CannotAffordAccessTriggerCost {
                    card: card_id.clone(),
                    available,
                    requested: *requested,
                });
            }
        }
        events.extend(ability::pay_cost(state, pending.decider, &pending.cost, Some(card_id))?);
    }

    if paid != interaction.effects_resolve_on_decline() {
        let effects = registry
            .get(card_id)
            .and_then(|c| c.interactive_on_access.as_ref())
            .map(|interactive| interactive.effects.clone())
            .unwrap_or_default();

        for effect in &effects {
            events.extend(ability::evaluate_effect(
                state,
                effect,
                &mut ability::ResolutionContext::for_card(Some(card_id)),
                registry,
            )?);
        }
        // Only the effect-resolving branch can flatline the Runner, so the
        // game-over check belongs here rather than around the whole body.
        if let Some(finish) = finish_if_game_over(state, pending.server) {
            events.extend(finish);
            return Ok(events);
        }
    }

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
            triggers: vec![TriggeredEffect { trigger: Trigger::OnAccessed, effects, requirement: None }],
            trash_cost: None,
            ..trashable_card(id, 0)
        }
    }

    /// A trashable `CardDefinition` (see `trashable_card`) with an
    /// `OnTrashedFromAccess` trigger firing `effects` — Shock!-style.
    fn trashable_card_with_on_trashed_from_access(id: &str, trash_cost: u32, effects: Vec<Effect>) -> CardDefinition {
        CardDefinition {
            triggers: vec![TriggeredEffect { trigger: Trigger::OnTrashedFromAccess, effects, requirement: None }],
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
            corp: crate::rules::state::CorpState { identity: None, extra_clicks_next_turn: 0, identity_counters: 0, played_operation_this_turn: false, identity_flipped: false, bad_publicity: 0, first_install_used_this_turn: false, recurring_credits: 0, recurring_credits_max: 0, agenda_points_scored_this_turn: 0, max_hand_size_bonus: 0, cannot_score_agendas_this_turn: false, removed_from_game: Vec::new(), once_per_turn_used: std::collections::HashSet::new(),
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
            access_server(&mut state, ServerId::Hq, &registry()).unwrap(),
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

        let events_a = access_server(&mut state_a, ServerId::Hq, &registry()).unwrap();
        let events_b = access_server(&mut state_b, ServerId::Hq, &registry()).unwrap();

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
                match access_server(&mut state, ServerId::Hq, &registry()).unwrap().into_iter().next() {
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
            access_server(&mut state, ServerId::RnD, &registry()).unwrap(),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::RnD,
                install: None,
            }]
        );
    }

    #[test]
    fn accessing_rnd_with_additional_access_yields_top_two_cards_in_order() {
        let mut state = game_state(
            Vec::new(),
            vec![CardId("bottom".to_string()), CardId("middle".to_string()), CardId("top".to_string())],
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(RunState { additional_rd_access: 1, ..run_in_success(ServerId::RnD) });

        access_server(&mut state, ServerId::RnD, &registry()).unwrap();

        let access_state = state.active_run.unwrap().access_state.unwrap();
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![CardId("top".to_string()), CardId("middle".to_string())]
            }
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

        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let access_state = state.active_run.unwrap().access_state.unwrap();
        let selectable = match access_state.phase {
            AccessPhase::SelectNextCard { selectable_cards } => selectable_cards,
            other => panic!("expected SelectNextCard, got {other:?}"),
        };
        assert_eq!(selectable.len(), 2);
        assert_eq!(selectable.iter().collect::<HashSet<_>>().len(), 2, "cards must be distinct");
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

        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let access_state = state.active_run.unwrap().access_state.unwrap();
        let selectable = match access_state.phase {
            AccessPhase::SelectNextCard { selectable_cards } => selectable_cards,
            other => panic!("expected SelectNextCard, got {other:?}"),
        };
        assert_eq!(selectable.len(), 3);
        assert_eq!(selectable.iter().collect::<HashSet<_>>().len(), 3, "cards must be distinct");
    }

    #[test]
    fn accessing_hq_with_more_additional_access_than_available_caps_at_hq_size() {
        let hq = vec![CardId("card_0".to_string()), CardId("card_1".to_string())];
        let mut state = game_state(hq, Vec::new(), Vec::new(), Vec::new(), 42);
        state.active_run = Some(RunState { additional_hq_access: 4, ..run_in_success(ServerId::Hq) });

        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let access_state = state.active_run.unwrap().access_state.unwrap();
        let selectable = match access_state.phase {
            AccessPhase::SelectNextCard { selectable_cards } => selectable_cards,
            other => panic!("expected SelectNextCard, got {other:?}"),
        };
        assert_eq!(selectable.len(), 2);
        assert_eq!(selectable.iter().collect::<HashSet<_>>().len(), 2, "cards must be distinct");
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

        access_server(&mut state, ServerId::RnD, &registry()).unwrap();

        let access_state = state.active_run.unwrap().access_state.unwrap();
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![CardId("top".to_string()), CardId("bottom".to_string())]
            }
        );
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

        let events = access_server(&mut state, ServerId::Hq, &registry()).unwrap();

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

        let events = access_server(&mut state, ServerId::Hq, &registry()).unwrap();

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
        assert_eq!(access_server(&mut state, ServerId::Hq, &registry()).unwrap(), Vec::new());
        let access_state = state.active_run.unwrap().access_state.unwrap();
        assert_eq!(
            access_state.unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("ash_2_0".to_string())]
        );
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![
                    CardId("hedge_fund".to_string()),
                    CardId("ash_2_0".to_string())
                ]
            }
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
        assert_eq!(access_server(&mut state, ServerId::RnD, &registry()).unwrap(), Vec::new());
        assert_eq!(
            state.active_run.unwrap().access_state.unwrap().unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("crisium_grid".to_string())]
        );
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
        assert_eq!(access_server(&mut state, ServerId::Archives, &registry()).unwrap(), Vec::new());
        assert_eq!(
            state.active_run.unwrap().access_state.unwrap().unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())]
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

        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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

        access_server(&mut state, ServerId::Archives, &registry()).unwrap();

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
            access_server(&mut state, ServerId::Remote(0), &registry()).unwrap(),
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
        assert_eq!(access_server(&mut state, ServerId::Remote(0), &registry()).unwrap(), Vec::new());
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn accessing_an_empty_zone_yields_no_events() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        assert_eq!(access_server(&mut state, ServerId::Hq, &registry()).unwrap(), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::RnD, &registry()).unwrap(), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::Archives, &registry()).unwrap(), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::Remote(0), &registry()).unwrap(), Vec::new());
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("priority_requisition".to_string());
        assert_eq!(
            resolve_pass(&mut state, &card_id, &registry),
            Err(RulesError::MandatoryStealViolation { card: card_id.clone() })
        );

        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");
        assert_eq!(state.runner.scored_agendas, vec![card_id.clone()]);
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
        access_server(&mut state, ServerId::RnD, &registry).unwrap();
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert_eq!(state.corp.r_and_d, vec![agenda.clone(), other.clone()], "the top copy left R&D");
        assert_eq!(state.runner.scored_agendas, vec![agenda.clone()]);

        // HQ.
        let mut state = game_state(vec![agenda.clone()], Vec::new(), Vec::new(), Vec::new(), 0);
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry).unwrap();
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert!(state.corp.hq.is_empty(), "the stolen agenda left HQ");

        // Archives: a stolen agenda leaves Archives too (a *trashed* card
        // accessed there stays — that is `move_to_archives`' early return).
        let mut state = game_state(Vec::new(), Vec::new(), vec![agenda.clone()], Vec::new(), 0);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();
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
        access_server(&mut state, ServerId::Remote(1), &registry).unwrap();
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
        access_server(&mut state, ServerId::RnD, &registry).unwrap();
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert!(state.corp.r_and_d.is_empty(), "the R&D copy was stolen");
        assert_eq!(state.corp.installed.len(), 2, "both installs untouched: {:?}", state.corp.installed);
        assert_eq!(state.corp.installed[0].advancement_tokens, 3);

        // Trash out of HQ: the hand copy goes to Archives, the rezzed remote
        // copy keeps its counters.
        let mut state = game_state(vec![asset.clone()], Vec::new(), Vec::new(), installed(), 0);
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry).unwrap();
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
        access_server(&mut state, ServerId::Hq, &registry).unwrap();

        let events = resolve_steal(&mut state, &agenda, &registry).unwrap();

        assert_eq!(state.runner.scored_agendas, vec![agenda], "the steal itself stands");
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let access = state.active_run.as_ref().unwrap().access_state.as_ref().expect("two cards to access");
        let AccessPhase::SelectNextCard { selectable_cards } = &access.phase else {
            panic!("two accessed cards should present a selection, got {:?}", access.phase);
        };
        assert_eq!(selectable_cards, &vec![archived, upgrade]);
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
        state.runner.scored_agendas = vec![CardId("already_scored".to_string())];
        state.runner.resources.agenda_points = AgendaPoints(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
        state.runner.scored_agendas = vec![CardId("already_scored".to_string())];
        state.runner.resources.agenda_points = AgendaPoints(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("priority_requisition".to_string());
        resolve_select_card(&mut state, &card_id, &registry).expect("selecting should succeed");
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("napd_contract".to_string());
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        assert_eq!(state.runner.resources.credits, Credits(0));
        assert_eq!(state.runner.scored_agendas, vec![card_id.clone()]);
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
        access_server(&mut state, ServerId::Remote(0), &registry).unwrap();

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
                GameEvent::CardTrashedFromAccess { card: card_id, cost_paid: 2 },
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
        access_server(&mut state, ServerId::Remote(0), &registry).unwrap();

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
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

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
        let first = access_server(&mut state, ServerId::Archives, &registry()).unwrap();
        assert_eq!(first, Vec::new());
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![
                    CardId("hedge_fund".to_string()),
                    CardId("ice_wall".to_string())
                ]
            }
        );

        // Pick the second card first — order is the Runner's choice, not
        // the fixed access-determination order.
        let selected = resolve_select_card(&mut state, &CardId("ice_wall".to_string()), &registry())
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
        access_server(&mut state, ServerId::Archives, &registry()).unwrap();

        let events = resolve_select_card(&mut state, &CardId("ice_wall".to_string()), &registry())
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
        assert_eq!(access_state.unaccessed_cards, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(
            access_state.phase,
            AccessPhase::PendingChoice {
                card_id: CardId("ice_wall".to_string()),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
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
        access_server(&mut state, ServerId::Archives, &registry()).unwrap();

        resolve_select_card(&mut state, &CardId("card_3".to_string()), &registry())
            .expect("selecting card_3 should succeed");
        resolve_pass(&mut state, &CardId("card_3".to_string()), &registry())
            .expect("passing card_3 should succeed");

        // Two cards remain, so the Runner is offered another choice rather
        // than auto-advancing.
        let access_state = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![CardId("card_1".to_string()), CardId("card_2".to_string())]
            }
        );
        assert_eq!(access_state.resolved_cards, vec![CardId("card_3".to_string())]);

        resolve_select_card(&mut state, &CardId("card_1".to_string()), &registry())
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
        access_server(&mut state, ServerId::Archives, &registry()).unwrap();
        resolve_select_card(&mut state, &CardId("hedge_fund".to_string()), &registry())
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
        access_server(&mut state, ServerId::Archives, &registry()).unwrap();

        let wrong_id = CardId("wrong_card".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &wrong_id, &registry()),
            Err(RulesError::InvalidAccessSelection { card: wrong_id })
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
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn selecting_with_no_active_run_errors() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &card_id, &registry()),
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
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

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
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

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
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

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

        let events = access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
        assert_eq!(events[2], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2 });
        assert_eq!(events.len(), 5);
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
        access_server(&mut state, ServerId::Remote(0), &registry).unwrap();

        let card_id = CardId("shock".to_string());
        let events = resolve_trash(&mut state, &card_id, &registry).expect("trash should succeed");

        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(events[0], GameEvent::CreditsSpent { side: Side::Runner, amount: 2 });
        assert_eq!(
            events[1],
            GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: 2 }
        );
        assert_eq!(
            events[2],
            GameEvent::TriggerFired { card: card_id.clone(), trigger: crate::dsl::Trigger::OnTrashedFromAccess }
        );
        assert_eq!(events[3], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 1 });
        assert!(matches!(events[4], GameEvent::CardDiscarded { side: Side::Runner, .. }));
        assert_eq!(events[5], GameEvent::RunCompleted { server: ServerId::Remote(0) });
        assert_eq!(events.len(), 6);
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let events = resolve_select_card(&mut state, &CardId("snare".to_string()), &registry)
            .expect("selecting should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives, install: None },
                GameEvent::TriggerFired { card: CardId("snare".to_string()), trigger: crate::dsl::Trigger::OnAccessed },
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();
        resolve_select_card(&mut state, &CardId("hedge_fund".to_string()), &registry)
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

        let events = access_server(&mut state, ServerId::Archives, &registry).unwrap();

        assert_eq!(
            events,
            vec![GameEvent::CardAccessed { card: CardId("fetal_ai".to_string()), server: ServerId::Archives, install: None }]
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
                steal_cost: None,
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
                steal_cost: None,
            }
        );
        assert!(matches!(events[0], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2 }));
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("fetal_ai".to_string());
        let events =
            resolve_decline_access_trigger(&mut state, &card_id, &registry).expect("declining should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::RunnerFlatlined,
                GameEvent::GameOver { winner: Side::Corp },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn ordinary_on_accessed_cards_are_unaffected_by_the_interactive_trigger_refactor() {
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "snare",
            vec![Effect::GiveTags(1)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("snare".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));

        let events = access_server(&mut state, ServerId::Archives, &registry).unwrap();

        assert_eq!(state.runner.tags, 1, "OnAccessed still fires unconditionally for non-interactive cards");
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: CardId("snare".to_string()),
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            }
        );
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives, install: None },
                GameEvent::TriggerFired { card: CardId("snare".to_string()), trigger: crate::dsl::Trigger::OnAccessed },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
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
            triggers: vec![TriggeredEffect { trigger: Trigger::OnAccessed, effects: on_accessed_effects, requirement: None }],
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
            vec![Effect::GiveTags(1)],
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
            ]
        );
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

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
                steal_cost: None,
            }
        );
        assert_eq!(
            resolve_pass(&mut state, &card_id, &registry),
            Err(RulesError::MandatoryStealViolation { card: card_id.clone() })
        );

        let events = resolve_steal(&mut state, &card_id, &registry).expect("stealing should succeed");
        assert_eq!(state.runner.scored_agendas, vec![card_id.clone()]);
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
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        // Pick the plain card first, then pass it — auto-advancing to the
        // second (and last) card, which carries the interactive trigger.
        resolve_select_card(&mut state, &CardId("hedge_fund".to_string()), &registry)
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
            vec![Effect::GiveTags(1), Effect::TrashCard(CardTarget::ThisCard)],
        )]);
        let mut state = game_state(
            vec![CardId("shock_ish".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));

        let events = access_server(&mut state, ServerId::Hq, &registry).unwrap();

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
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
                GameEvent::CardTrashed { side: Side::Corp, card: CardId("shock_ish".to_string()) },
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
        access_server(&mut state, ServerId::Remote(0), &registry).unwrap();

        // Both offered; picking "skunkworks" pins the first unresolved instance.
        resolve_select_card(&mut state, &upgrade, &registry).unwrap();
        let access = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(access.pending_install, Some(InstallId(1)));

        resolve_trash(&mut state, &upgrade, &registry).unwrap();
        assert_eq!(state.corp.installed.len(), 1, "one copy left");
        assert_eq!(state.corp.installed[0].install_id, InstallId(2), "the pinned instance is the one that left");
        let access = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(access.resolved_installs, vec![InstallId(1)]);
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
        access_server(&mut state, ServerId::Remote(0), &registry).unwrap();

        let events = resolve_select_card(&mut state, &trap, &registry).unwrap();
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
        let events = access_server(&mut state, ServerId::Hq, &registry).unwrap();
        assert!(events.iter().any(|e| matches!(e, GameEvent::CardAccessed { install: None, .. })), "{events:?}");
        assert_eq!(state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().pending_install, None);
        resolve_steal(&mut state, &agenda, &registry).unwrap();
        assert!(state.corp.hq.is_empty());
    }
}
