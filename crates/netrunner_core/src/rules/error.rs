use thiserror::Error;

use crate::dsl::CardId;
use crate::rules::run::RunPhase;
use crate::rules::state::Side;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RulesError {
    #[error("{side:?} attempted to spend {requested} click(s) but only has {available}")]
    NotEnoughClicks {
        side: Side,
        available: u32,
        requested: u32,
    },

    #[error("action requires it to be {side:?}'s turn")]
    NotYourTurn { side: Side },

    #[error("cannot continue past ICE while {pending} subroutine(s) are still pending")]
    SubroutinesStillPending { pending: u32 },

    #[error("no subroutines are pending to resolve or break right now")]
    NoSubroutinesPending,

    #[error("run action attempted after the run already reached {phase:?}")]
    RunAlreadyConcluded { phase: RunPhase },

    #[error("cannot complete a run that hasn't concluded yet (currently {phase:?})")]
    RunNotConcluded { phase: RunPhase },

    #[error("{side:?} has no card {card:?} in hand")]
    CardNotInHand { side: Side, card: CardId },

    #[error("no installed card {card:?} found")]
    CardNotInstalled { card: CardId },

    #[error("card {card:?} is already rezzed")]
    AlreadyRezzed { card: CardId },

    #[error("a run is already in progress")]
    RunAlreadyInProgress,

    #[error("no active run to act on")]
    NoActiveRun,

    #[error("attempted to spend {requested} memory unit(s) but only has {available}")]
    InsufficientMemory { available: u32, requested: u32 },

    #[error("subroutine index {index} is out of range: only {pending} subroutine(s) pending")]
    InvalidSubroutineIndex { index: usize, pending: u32 },

    #[error("cannot end turn while a run is active")]
    CannotEndTurnWhileRunActive,
}
