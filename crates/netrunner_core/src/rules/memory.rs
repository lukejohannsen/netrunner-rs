//! The Runner's memory budget, derived from the board rather than spent.
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

use crate::cards::CardRegistry;
use crate::rules::state::{GameState, MemoryUnits};

/// Every Runner identity's base rig capacity in this engine. Real Netrunner
/// varies this per-identity; no identity-level override mechanism exists yet.
pub const RUNNER_BASE_MEMORY_UNITS: u32 = 4;

/// Memory units the Runner has free: the base capacity, plus every installed
/// card's `memory_bonus`, minus every installed card's `memory_cost`.
///
/// Saturating at zero rather than wrapping. A negative budget is not
/// reachable through `apply_action` — `install_program` refuses an install
/// that would exceed the budget — but a hand-built fixture or a future effect
/// that installs while ignoring costs could produce one, and reporting `0` is
/// the honest answer to "how much is free".
///
/// Summed over the whole rig rather than filtered by `CardType`: today only
/// Hardware grants a bonus and only a Program charges a cost, but summing
/// both over everything installed is total by construction and needs no
/// update when that stops being true.
pub fn available_memory(state: &GameState, registry: &CardRegistry) -> u32 {
    let (granted, spent) = state.runner.rig.iter().filter_map(|installed| registry.get(&installed.card)).fold(
        (0, 0),
        |(granted, spent), card| {
            (granted + card.memory_bonus.unwrap_or(0), spent + card.memory_cost.unwrap_or(0))
        },
    );
    (RUNNER_BASE_MEMORY_UNITS + granted).saturating_sub(spent)
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
