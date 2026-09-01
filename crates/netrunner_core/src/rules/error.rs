use thiserror::Error;

use crate::dsl::{CardId, IceType};
use crate::rules::run::{RunPhase, ServerId};
use crate::rules::state::{GamePhase, InstallId, PreventionKind, Side};

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

    #[error("action requires a Mulligan phase but game is in {actual:?}")]
    NotInMulliganPhase { actual: GamePhase },

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

    /// An action named an install that is no longer on the table — trashed,
    /// scored, or stolen since the action was offered. Distinct from
    /// `CardNotInstalled`, which reports a *card* the engine's own effect
    /// resolution could not find: this one reports a stale `InstallId` from
    /// a `PlayerAction`, and carries no `CardId` because the whole point of
    /// an `InstallId` is that the actor may not know which card it names.
    #[error("no install {0:?} found on the table")]
    InstallNotFound(InstallId),

    #[error("card {0:?} does not declare installs_on_ice and cannot be installed via InstallProgramOnIce")]
    NotATrojanProgram(CardId),

    #[error("card {0:?} is not an installed piece of ICE and cannot host a Trojan Program")]
    HostIsNotIce(CardId),

    #[error("card {card:?} is already rezzed")]
    AlreadyRezzed { card: CardId },

    #[error("a run is already in progress")]
    RunAlreadyInProgress,

    #[error("a run cannot begin right now (phase {phase:?})")]
    RunNotPermittedNow { phase: GamePhase },

    #[error("no active run to act on")]
    NoActiveRun,

    #[error("attempted to spend {requested} memory unit(s) but only has {available}")]
    InsufficientMemory { available: u32, requested: u32 },

    #[error("the Runner already has an installed Console — limit 1 per player")]
    ConsoleLimitExceeded,

    #[error("cannot end turn while a run is active")]
    CannotEndTurnWhileRunActive,

    #[error("{side:?} attempted to spend {requested} credit(s) but only has {available}")]
    NotEnoughCredits { side: Side, available: u32, requested: u32 },

    #[error("{side:?} has no card {card:?} in the rig")]
    CardNotInRig { side: Side, card: CardId },

    #[error("{side:?}'s {zone:?} is empty, nothing to trash from the top")]
    EmptyZone { side: Side, zone: crate::dsl::StackZone },

    #[error("no acting rig card was identified for an effect that targets whichever card activated it")]
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

    #[error("breaker {breaker:?} cannot break subroutines on {ice:?}: requires {expected:?}")]
    InvalidBreakerSubtype {
        breaker: CardId,
        ice: CardId,
        expected: IceType,
    },

    #[error("card {card:?} is not an Operation and cannot be played via PlayOperation")]
    CardNotOperation { card: CardId },

    #[error("attempted to pay {requested} credit(s) to avoid {card:?}'s access trigger but only has {available}")]
    CannotAffordAccessTriggerCost { card: CardId, available: u32, requested: u32 },

    #[error("a self-reference (CardTarget::ThisCard or Cost::TrashSelf) requires an acting card, but none was available here")]
    MissingActingCardContext,

    #[error("a trace is already active — cannot start a second one before it resolves")]
    TraceAlreadyActive,

    #[error("no trace is currently awaiting the Corp's bid")]
    TraceNotAwaitingCorpBid,

    #[error("no trace is currently awaiting the Runner's bid")]
    TraceNotAwaitingRunnerBid,

    #[error("cannot take that action while a trace is active (awaiting {awaiting:?}'s bid)")]
    ActionBlockedByActiveTrace { awaiting: Side },

    /// A run is itself an action in progress, so no *basic* action may begin
    /// until it resolves. Distinct from `RunAlreadyInProgress` (which is
    /// `InitiateRun`'s "you already have one") and
    /// `CannotEndTurnWhileRunActive` (the turn-structure case): this is the
    /// general one, raised by `engine::apply_action`'s central guard for
    /// every `ActionKind::BasicClickAction`.
    #[error("cannot take a basic action while a run is in progress")]
    ActionBlockedByActiveRun,

    #[error("the Runner has no tags")]
    RunnerNotTagged,

    #[error("card {card:?} is not a Resource")]
    CardNotResource { card: CardId },

    #[error("card {card:?} is not an Identity and cannot anchor a deck")]
    CardNotIdentity { card: CardId },

    #[error("identity {card:?} has no min_deck_size configured in the registry")]
    IdentityMissingMinDeckSize { card: CardId },

    #[error("card {card:?} is not this deck's declared identity's side (expected {expected:?}, found {actual:?})")]
    IdentitySideMismatch { card: CardId, expected: Side, actual: Side },

    #[error("deck has {size} card(s) but its identity requires at least {minimum}")]
    DeckBelowMinimumSize { size: u32, minimum: u32 },

    #[error("card {card:?} has {count} copies, exceeding the {max}-copy limit")]
    TooManyCopies { card: CardId, count: u32, max: u32 },

    #[error("card {card:?} is {actual:?}-side but this deck is {expected:?}-side")]
    DeckCardWrongSide { card: CardId, expected: Side, actual: Side },

    #[error("Runner decks cannot contain Agenda {card:?}")]
    RunnerDeckContainsAgenda { card: CardId },

    #[error("deck has {points} agenda point(s), outside the required range {min}-{max}")]
    AgendaPointsOutOfRange { points: u32, min: u32, max: u32 },

    #[error("card {card:?} is not an Agenda and cannot be scored")]
    CardNotAgenda { card: CardId },

    #[error("card {card:?} has {current} advancement token(s) but needs {required} to score")]
    AdvancementRequirementNotMet { card: CardId, current: u32, required: u32 },

    #[error("the Corp cannot score any further agenda this turn")]
    CannotScoreAgendasThisTurn,

    #[error("server {server:?} is not among those this choice allows")]
    ServerNotAllowedForChoice { server: ServerId },

    #[error("a soft-gated requirement was not met")]
    RequirementNotMet,

    #[error("the break named ice {actual:?}, but the ICE currently being encountered is {expected:?}")]
    MismatchedIceId { expected: CardId, actual: CardId },

    #[error("ICE {card:?} can only be rezzed while the Runner is approaching it")]
    IceNotBeingApproached { card: CardId },

    #[error("no pending prevention window is currently open")]
    NoPendingPrevention,

    #[error("expected a {expected:?} prevention window but {actual:?} is currently pending")]
    PreventionKindMismatch { expected: PreventionKind, actual: PreventionKind },


    #[error("card {0:?} is not an active installed/rigged card and cannot hold counters")]
    CardNotEligibleForCounters(CardId),

    #[error("card {card:?} has only {available} counters, but {required} are required to pay this cost")]
    InsufficientCounters { card: CardId, required: u32, available: u32 },

    #[error("card {0:?} has no gameplay data (catalog-only) and cannot be included in a playable deck")]
    UnplayableCard(CardId),

    #[error("Cost::AnyOf cannot be paid directly — the payer's choice must be resolved first")]
    CostRequiresChoice,

    #[error("no pending paid choice is currently awaiting a decision")]
    NoPendingPaidChoice,

    #[error("cost option index {0} is out of range for this pending paid choice's Cost::AnyOf")]
    InvalidCostChoiceIndex(usize),

    #[error("no pending decision is currently awaiting a choice")]
    NoPendingDecision,

    #[error("choice option index {0} is out of range for this pending decision")]
    InvalidChoiceIndex(usize),

    #[error("cannot take that action while a paid choice is pending ({side:?} must Accept or Decline it)")]
    ActionBlockedByPendingPaidChoice { side: Side },

    #[error("cannot take that action while a decision is pending ({side:?} must resolve it)")]
    ActionBlockedByPendingDecision { side: Side },

    #[error("position {0} is not a legal candidate for the pending card selection")]
    CardNotEligibleForSelection(usize),

    #[error("the pending card selection already holds its maximum of {max} cards")]
    CardSelectionFull { max: u32 },

    #[error("selected {selected} cards, but this pending choice requires between {min} and {max}")]
    CardSelectionOutOfRange { selected: usize, min: u32, max: u32 },

    #[error("trigger choice {index} is out of range: {pending} triggers are pending")]
    TriggerChoiceOutOfRange { index: usize, pending: usize },

    #[error("cannot swap ice {0:?} while it is part of the currently active run")]
    CannotSwapIceDuringActiveRun(CardId),

    #[error("the runner cannot steal or trash cards for the remainder of this run")]
    StealAndTrashPreventedThisRun,

    #[error("piece of ice {0:?} cannot be broken by spending a click (not click-breakable)")]
    IceNotClickBreakable(CardId),
}
