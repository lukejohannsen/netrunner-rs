use thiserror::Error;

use crate::dsl::CardId;
use crate::rules::run::RunPhase;
use crate::rules::state::{GamePhase, Side};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RulesError {
    #[error("{side:?} attempted to spend {requested} click(s) but only has {available}")]
    NotEnoughClicks {
        side: Side,
        available: u32,
        requested: u32,
    },

    #[error("action requires phase {expected:?} but game is in {actual:?}")]
    WrongPhase { expected: GamePhase, actual: GamePhase },

    #[error("action requires an Action phase but game is in {actual:?}")]
    NotInActionPhase { actual: GamePhase },

    #[error("action requires a Discard phase but game is in {actual:?}")]
    NotInDiscardPhase { actual: GamePhase },

    #[error("cannot continue past ICE while {pending} subroutine(s) are still pending")]
    SubroutinesStillPending { pending: u32 },

    #[error("not currently encountering ICE")]
    NotInEncounter,

    #[error("subroutine index {0} is out of range")]
    InvalidSubroutineIndex(usize),

    #[error("that subroutine has already been broken or resolved")]
    SubroutineAlreadyHandled,

    #[error("card {0:?} not found in the card registry")]
    CardNotFoundInRegistry(CardId),

    #[error("run action attempted after the run already reached {phase:?}")]
    RunAlreadyConcluded { phase: RunPhase },

    #[error("cannot complete a run that hasn't concluded yet (currently {phase:?})")]
    RunNotConcluded { phase: RunPhase },

    #[error("cannot jack out right now — no jack-out window is open (currently {phase:?})")]
    IllegalJackOutWindow { phase: RunPhase },

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

    #[error("cannot end turn while a run is active")]
    CannotEndTurnWhileRunActive,

    #[error("{side:?} attempted to spend {requested} credit(s) but only has {available}")]
    NotEnoughCredits { side: Side, available: u32, requested: u32 },

    #[error("{side:?} has no card {card:?} in the rig")]
    CardNotInRig { side: Side, card: CardId },

    #[error("{side:?}'s {zone:?} is empty, nothing to trash from the top")]
    EmptyZone { side: Side, zone: crate::dsl::StackZone },

    #[error("CardTarget::ThisCard/Cost::TrashSelf must be resolved to a concrete card before evaluation")]
    UnresolvedCardTarget,

    #[error("{side:?}'s card {card:?} is not in an active zone (installed+rezzed for Corp, in the Rig for Runner)")]
    CardNotActive { side: Side, card: CardId },

    #[error("ability index {0} is out of range")]
    InvalidAbilityIndex(usize),

    #[error("ability index {0} is not a Trigger::Paid ability and cannot be manually activated")]
    AbilityNotManuallyActivatable(usize),

    #[error("card {card:?} has no advancement_requirement and cannot be advanced")]
    CardNotAdvanceable { card: CardId },

    #[error("no run is currently awaiting an access choice for that card")]
    NotInAccessPhase,

    #[error("{card:?} is not one of the cards currently offered for access selection")]
    InvalidAccessSelection { card: CardId },

    #[error("Agenda {card:?} must be stolen and cannot be passed")]
    MandatoryStealViolation { card: CardId },

    #[error("attempted to pay {requested} credit(s) to steal {card:?} but only has {available}")]
    CannotAffordStealCost { card: CardId, available: u32, requested: u32 },

    #[error("attempted to pay {requested} credit(s) to trash {card:?} but only has {available}")]
    CannotAffordTrashCost { card: CardId, available: u32, requested: u32 },

    #[error("no paid ability window is currently open")]
    NotInPaidAbilityWindow,

    #[error("it is not {actual:?}'s priority right now — {expected:?} has priority")]
    NotYourPriority { expected: Side, actual: Side },

    #[error("cannot take that action while a paid ability window is open (priority: {priority:?})")]
    BlockedByPaidAbilityWindow { priority: Side },

    #[error("breaker {breaker:?} (strength {breaker_strength}) is too weak to break subroutines on {ice:?} (strength {ice_strength})")]
    BreakerStrengthTooLow {
        breaker: CardId,
        breaker_strength: i32,
        ice: CardId,
        ice_strength: i32,
    },
}
