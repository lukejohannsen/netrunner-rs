//! The Runner's memory budget, derived from the board rather than spent —
//! and enforced when the board drops below what is installed.
//!
//! Real Netrunner treats memory as a continuously-evaluated property of what
//! is installed: a program reserves its memory cost for exactly as long as it
//! is in the rig, and frees it the instant it leaves. This module is that
//! rule, and `RunnerState::memory_units` is a cached *report* of it —
//! refreshed at a single choke point in `engine::apply_action`, never
//! mutated piecemeal.
//!
//! **It used to be a spent resource, and that was a leak waiting to happen.**
//! `MemoryUnits::spend` was called only by the two install handlers, and none
//! of the five paths a rig card can leave play by refunded anything. Nothing
//! noticed because the bug that made this module necessary — `legal_actions`
//! offering every program a `memory_cost` of `0` that the handler then
//! rejected — meant no program was ever installed through a legal action, so
//! no memory was ever spent in the first place.
//!
//! Deriving rather than refunding is the same choice `turn::
//! cards_over_hand_limit` makes for the discard count, and for the same
//! reason: five refund sites are five chances for a sixth to be added without
//! one. There is nothing to keep in sync here.
//!
//! **The budget can go negative, and the rules say what happens then.** An
//! earlier version of this doc claimed a negative budget was "not reachable
//! through `apply_action`" because `install_program` refuses an install over
//! budget. That guards one direction only: the *other* side of the ledger
//! moves when a console leaves play — *Retribution* and *Ansel 1.0* both
//! trash a Runner's installed hardware through `pending_choice::
//! remove_installed_card` — and every program stayed installed with the
//! report saturated at `0` (ROADMAP Rules Audit, Tier 2). Netrunner's rule
//! is a checkpoint condition: while the Runner has more memory in use than
//! available, they must trash installed programs of their choice until they
//! are within the limit. [`enforce_limit`] is that checkpoint.

use crate::cards::CardRegistry;
use crate::dsl::{CardFilter, CardType, CardZoneRef, Effect};
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::state::{GameState, MemoryUnits, Side};

/// Every Runner identity's base rig capacity in this engine. Real Netrunner
/// varies this per-identity; no identity-level override mechanism exists yet.
pub const RUNNER_BASE_MEMORY_UNITS: u32 = 4;

/// The Runner's memory ledger, signed: the base capacity plus every
/// installed card's `memory_bonus`, minus every installed card's
/// `memory_cost`. Negative exactly when more memory is in use than exists —
/// the condition [`enforce_limit`] acts on, and the number [`available_memory`]
/// clamps away.
///
/// Summed over the whole rig rather than filtered by `CardType`: today only
/// Hardware grants a bonus and only a Program charges a cost, but summing
/// both over everything installed is total by construction and needs no
/// update when that stops being true.
pub fn memory_balance(state: &GameState, registry: &CardRegistry) -> i32 {
    let (granted, spent) = state.runner.rig.iter().filter_map(|installed| registry.get(&installed.card)).fold(
        (0i32, 0i32),
        |(granted, spent), card| {
            (granted + card.memory_bonus.unwrap_or(0) as i32, spent + card.memory_cost.unwrap_or(0) as i32)
        },
    );
    RUNNER_BASE_MEMORY_UNITS as i32 + granted - spent
}

/// Memory units the Runner has free — [`memory_balance`] clamped at zero,
/// because "how much is free" has no negative answer. Whether the rig is
/// *over* is [`memory_balance`]'s question, not this one's.
pub fn available_memory(state: &GameState, registry: &CardRegistry) -> u32 {
    memory_balance(state, registry).max(0) as u32
}

/// Writes [`available_memory`] into `state.runner.memory_units`.
///
/// The **single** place that field is assigned outside `GameState::setup`.
/// Called from `engine::apply_action` after every handler, alongside
/// `dispatcher::drain_deferred_triggers` and for the same reason: one call
/// is simpler and harder to miss than threading it through every site that
/// installs or trashes a rig card.
pub(crate) fn refresh(state: &mut GameState, registry: &CardRegistry) {
    state.runner.memory_units = MemoryUnits(available_memory(state, registry));
}

/// The memory-limit checkpoint: if the Runner has more memory in use than
/// available, parks a `PendingDecision::ChooseCards` making them trash one
/// installed program of their choice, and reports it as
/// `GameEvent::MemoryLimitExceeded`.
///
/// **One program per action, deliberately.** Programs cost different
/// amounts (*Mayfly* is 2 MU), so "how many must go" has no single answer
/// until the Runner has chosen which; rather than compute a count, this
/// parks a 1-of-N choice and lets `engine::apply_action` — which runs this
/// after every handler, `ConfirmCardSelection` included — re-check and
/// re-park until the balance is non-negative. The Runner keeps the choice
/// the rules give them at every step.
///
/// Goes through `Effect::PromptChooseCards` rather than building the
/// decision by hand so it inherits that effect's fewer-than-`min` guard:
/// if nothing eligible is installed (a rig over budget with no programs,
/// which no card today can produce), nothing parks and nothing deadlocks.
/// Skipped while any resolution is already blocked — the parked thing
/// resolves first, and the next action re-checks — and once the game is
/// over.
pub(crate) fn enforce_limit(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    if state.resolution_halted() {
        return Ok(Vec::new());
    }
    let balance = memory_balance(state, registry);
    if balance >= 0 {
        return Ok(Vec::new());
    }
    let over_by = balance.unsigned_abs();

    let prompt = Effect::PromptChooseCards {
        side: Side::Runner,
        source: CardZoneRef::OwnInstalled,
        filter: CardFilter::CardType(CardType::Program),
        min: 1,
        max: 1,
        reveal: false,
        shuffle_after: false,
        destination: Some(CardZoneRef::OwnHeap),
        then: None,
    };
    let parked = ability::evaluate_effect(state, &prompt, &mut ability::ResolutionContext::default(), registry)?;
    if parked.is_empty() {
        // The guard declined to park (no eligible program). Reporting an
        // exceeded limit every action forever would be noise about a state
        // nothing can fix; stay quiet.
        return Ok(Vec::new());
    }
    let mut events = vec![GameEvent::MemoryLimitExceeded { over_by }];
    events.extend(parked);
    Ok(events)
}
