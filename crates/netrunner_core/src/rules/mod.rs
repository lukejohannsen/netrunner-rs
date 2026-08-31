mod ability;
mod action;
mod action_mask;
mod damage;
pub(crate) mod deck;
mod dispatcher;
mod engine;
mod error;
mod event;
mod legal_actions;
mod masking;
mod paid_ability;
mod pending_choice;
mod run;
mod setup;
mod state;
mod trace;
mod turn;
mod win;

pub use ability::{evaluate_effect, pay_cost, process_card_triggers, resolve_unbroken_subroutines, ResolutionContext};
// Only reached from `cards::tests` today (`ability.rs`'s own internal use
// doesn't need this re-export) — gated to avoid an unused-import warning on
// a non-test build.
#[cfg(test)]
pub(crate) use ability::computed_runner_strength;
pub use action::{PlayerAction, ServerTarget, TargetZone};
pub use action_mask::{get_action_mask, ActionSpace};
pub use damage::apply_damage;
pub use deck::{validate_deck, Deck};
pub use dispatcher::dispatch_event;
pub use engine::apply_action;
pub use error::RulesError;
pub use event::GameEvent;
pub use legal_actions::{current_actor, legal_actions, legal_actions_for};
pub use masking::{
    mask_state_for_player, MaskedZone, PublicAccessPhase, PublicAccessState, PublicArchivedCard, PublicCorpState,
    PublicGameState,
    PublicInstalledCard, PublicInstalledRunnerCard, PublicRunIce, PublicRunIceIdentity, PublicRunState,
    PublicRunnerState,
};
pub use run::{
    access_server, advance_run, resolve_pass, resolve_select_card, resolve_steal, resolve_trash,
    AccessPhase, AccessState, EncounteredSubroutine, RunAction, RunIce, RunPhase, RunState,
    ServerId, SubroutineStatus,
};
pub use state::{
    ArchivedCard,
    AgendaPoints, Clicks, CorpState, Credits, GamePhase, GameState, InstallSlot, InstalledCard, InstalledRunnerCard,
    MemoryUnits, PaidAbilityWindow, PendingChoiceResume, PendingDecision, PendingPaidChoice, PendingPaidChoiceResume,
    PendingPrevention, PendingPreventionKind, PlayerResources, PreventionKind, PreventionResume, RunnerState, Side,
    TraceResume, TraceState, WindowCheckpoint,
};
pub use turn::end_turn;
