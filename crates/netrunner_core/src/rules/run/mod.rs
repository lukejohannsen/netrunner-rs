mod access;
mod action;
mod engine;
mod state;

pub use access::{
    access_server, resolve_decline_access_trigger, resolve_pass, resolve_pay_access_trigger, resolve_select_card,
    resolve_steal, resolve_trash, trash_currently_accessed_card_without_cost,
};
pub(crate) use access::{at_mid_access_window, breach, breaching, host_currently_accessed_card};
pub use action::RunAction;
pub use engine::{advance_run, start_run};
pub(crate) use engine::start_breach;
pub(crate) use engine::{
    break_subroutine, bypass_encountered_ice, check_run_may_begin, encounter_ends, end_run, move_run_to_outermost, reconcile_ice, renumber_subroutines,
    swap_approached_ice_with_card, transition_subroutine,
};
pub use state::{
    AccessCandidate, AccessPhase, AccessState, BrokenBy, EncounterTally, EncounteredSubroutine, RunIce, RunPhase, RunState, ServerId,
    SubroutineStatus,
};

/// The event that started the active run, while the run lasts (CR 8.6.5)
/// — the card `RunState::initiated_by` names when it is an event, and
/// nothing for a run a program or the basic action started. The engine
/// puts a played event in the heap as its play resolves, so this is where
/// the rules' play area is read from: `InstallId::RUN_EVENT` names it for
/// its paid abilities, and `EffectRequirement::RunEventActive` asks whether
/// it is a run event.
pub(crate) fn run_event<'a>(state: &'a crate::rules::state::GameState, registry: &crate::cards::CardRegistry) -> Option<&'a crate::dsl::CardId> {
    state
        .active_run
        .as_ref()
        .and_then(|run| run.initiated_by.as_ref())
        .filter(|card| registry.get(card).is_some_and(|def| def.card_type == crate::dsl::CardType::Event))
}
