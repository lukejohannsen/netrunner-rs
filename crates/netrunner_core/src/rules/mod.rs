mod ability;
mod action;
pub(crate) mod active;
mod action_mask;
mod checkpoint;
pub mod continuous;
mod damage;
pub(crate) mod deck;
mod dispatcher;
mod engine;
mod error;
mod event;
mod install_trash;
mod legal_actions;
pub mod lingering;
mod listeners;
mod masking;
pub mod memory;
mod paid_ability;
pub(crate) mod payment;
pub(crate) mod pending_choice;
mod run;
mod setup;
mod state;
#[cfg(test)]
pub(crate) mod test_support;
mod trace;
mod turn;
pub(crate) mod prevention;
pub mod turn_log;
mod uninstall;
mod win;

// `check_requirement` is public for the bots' evaluator, which reads an
// identity's intervening "if" as the engine would at a moment it prices
// ahead of time (`netrunner_bots::eval::identities`) — one definition of
// what a requirement means, never a copy of it in the bot.
pub use ability::{amount_on_table, check_requirement, evaluate_effect, process_card_triggers, resolve_unbroken_subroutines, ResolutionContext};
// `eligible_positions` likewise: whether an identity's "you may install 1
// card from HQ" has a card to install is the selection's own question
// (Phase 5 §36), asked of the zone as the engine will ask it.
pub use pending_choice::eligible_positions;
pub use action::{PlayerAction, ServerTarget, TargetZone};
pub use action_mask::{get_action_mask, ActionSpace};
pub use damage::apply_damage;
pub use win::{agenda_value_in, score, scored_value};
pub use deck::{validate_deck, Deck};
pub use dispatcher::dispatch_event;
pub use engine::apply_action;
pub use error::RulesError;
pub use event::{GameEvent, TrashedInstall};
pub use payment::{Ask as PaymentAsk, CardQuestion as PaymentCardQuestion, InstallCandidate, InstallQuestion, Pool, Question as PaymentQuestion};
pub use legal_actions::{
    apply_sampled_legal_action, current_actor, legal_actions, legal_actions_for, legal_transitions, legal_transitions_for,
};
pub use masking::{
    mask_action_for_player, mask_logged_action_for_player, mask_event_for_player, mask_state_for_player, ConcealedAction, MaskedZone, PublicAction, PublicAccessPhase, PublicAccessState, PublicArchivedCard, PublicCorpState,
    PublicGameState,
    PublicPendingPayment, PublicInstalledCard, PublicInstalledRunnerCard, PublicRunIce, PublicRunIceIdentity, PublicRunState,
    PublicRunnerState, Viewer,
};
pub use run::{
    access_server, advance_run, resolve_pass, resolve_select_card, resolve_steal, resolve_trash,
    AccessCandidate, AccessPhase, AccessState, BrokenBy, BrokenWith, EncounteredSubroutine, GainedForTheRun, RunAction, RunIce, RunPhase, RunState,
    ServerId, SubroutineStatus,
};
pub use setup::DeckOrder;
pub use state::{MatchRules, DEFAULT_WINNING_AGENDA_POINTS, 
    ArchivedCard, DeferredTrigger,
    AgendaPoints, Clicks, CorpState, Credits, GamePhase, GameState, InstallId, InstallSlot, InstalledCard, InstalledRunnerCard,
    MemoryUnits, OncePerTurnKey, PaidAbilityWindow, PendingChoiceResume, PendingDecision, PsiBid, PendingPaidChoice, PendingPaidChoiceResume, PendingPayment,
    PendingPrevention, PlayerResources, PreventionResume, RevealedCard, RunnerState, ScoredAgenda, Side,
    TraceResume, TraceState, WindowCheckpoint, WouldHappen,
};
pub use turn::end_turn;
