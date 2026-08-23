use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::ServerId;
use crate::rules::state::{InstallSlot, Side};

/// Which Corp zone/server an installed card is placed into. Alias of
/// `ServerId` — see its doc comment.
pub type TargetZone = ServerId;

/// Which Corp server a run targets. Alias of `ServerId`.
pub type ServerTarget = ServerId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerAction {
    /// Spend 1 click, gain 1 credit. Symmetric: either side can do this.
    GainCreditClick { side: Side },
    /// Spend 1 click, draw 1 card from the Stack into the Grip. Runner-only for now.
    DrawCardClick,
    /// Spend 1 click, move `card_id` from HQ onto `zone` as a newly installed,
    /// unrezzed card occupying `slot`. Corp-only (Runner grip/rig aren't
    /// modeled with card identity yet). The caller declares `slot` explicitly
    /// (rather than the engine deriving it from the card's `dsl::CardType`,
    /// which it can't look up — no `CardRegistry` is wired in) so that
    /// `run::access_server` can correctly exclude ICE from what a run
    /// accesses on a remote server.
    InstallCard { card_id: CardId, zone: TargetZone, slot: InstallSlot },
    /// Flip an already-installed card face-up. Corp-only. No click cost (rez is
    /// not a click action) and no credit cost yet — rez cost is data-driven
    /// per-card via `dsl::Card`, and no `CardRegistry` is wired into the engine
    /// yet. Permitted either during the Corp's own `GamePhase::Action`, or —
    /// regardless of whose turn it is — while a run is active and in
    /// `RunPhase::ApproachIce` (the rez window), since `phase` never changes
    /// mid-run.
    RezIce { ice_id: CardId },
    /// Spend 1 click, start a run on `server`. Runner-only. The resulting
    /// `RunState::ice` is left empty — populating real ICE requires a
    /// `CardRegistry` lookup from `corp.installed` that doesn't exist yet.
    InitiateRun { server: ServerTarget },
    /// Advance the active run to its next phase (Initiation -> ApproachIce,
    /// ApproachIce -> EncounterIce, EncounterIce -> next ICE's ApproachIce or
    /// Success). Runner-only. No click cost — like `JackOut`/`BreakSubroutine`,
    /// this is a run-flow sub-action, not a basic click action. Delegates to
    /// `run::advance_run`'s `RunAction::Continue`; requires `active_run` to be
    /// `Some` (`RulesError::NoActiveRun` otherwise), and propagates
    /// `RulesError::SubroutinesStillPending` when subroutines remain on the
    /// ICE currently being encountered.
    ContinueRun,
    /// Voluntarily end the active run. Runner-only, no click cost. Delegates to
    /// `run::advance_run`'s `RunAction::JackOut`.
    JackOut,
    /// Close out a run that has already reached `RunPhase::Success`, clearing
    /// `active_run` so a new run can be initiated. Runner-only, no click cost —
    /// like `JackOut`/`ContinueRun`, this is a run-flow sub-action, not a basic
    /// click action. Deliberately does NOT delegate to `run::advance_run` (whose
    /// top-of-function guard exists specifically to reject action on an
    /// already-concluded run — the opposite of what's needed here); the engine
    /// manipulates `GameState.active_run` directly instead. Requires `active_run`
    /// to be `Some` (`RulesError::NoActiveRun` otherwise) with
    /// `phase == RunPhase::Success` (`RulesError::RunNotConcluded` otherwise).
    /// `JackOut` remains the way to end a run before `Success`; once `Success` is
    /// reached, `JackOut` continues to be rejected via `RunAlreadyConcluded`, and
    /// `CompleteRun` is the only path out.
    CompleteRun,
    /// Spend 1 click, move `card_id` out of the Grip and resolve it. Runner-only.
    /// No credit cost yet — like `RezIce`, cost is data-driven per-card and no
    /// `CardRegistry` is wired into the engine yet.
    PlayEvent { card_id: CardId },
    /// Spend 1 click, move `card_id` from the Grip into the Rig. Runner-only.
    /// No credit cost yet, for the same reason as `PlayEvent`.
    InstallHardware { card_id: CardId },
    /// Spend 1 click, move `card_id` from the Grip into the Rig, reserving
    /// `memory_cost` memory units. Runner-only. No credit cost yet, for the
    /// same reason as `PlayEvent`.
    InstallProgram { card_id: CardId, memory_cost: u8 },
    /// Break the next pending subroutine on the ICE currently being
    /// encountered. Runner-only; delegates to `run::advance_run`'s
    /// `RunAction::BreakSubroutine`. `ice_id` isn't cross-checked against
    /// `RunState::ice` — that list has no card identity yet (see its doc
    /// comment) — so it's accepted as caller-provided context only.
    /// `subroutine_index` must address one of the currently pending
    /// subroutines; since `RunIce` only tracks a pending *count*, not
    /// individually addressable subroutines, this is a bounds check rather
    /// than a break-this-exact-one operation.
    BreakSubroutine { ice_id: CardId, subroutine_index: usize },
    /// End the active side's turn, handing control to the other side and
    /// refilling their clicks to the fixed per-turn allotment (Corp 3 / Runner
    /// 4). Symmetric — no `side` field; the acting side is whichever side
    /// `GameState::phase` is currently `Action(_)` for. Errors with
    /// `RulesError::CannotEndTurnWhileRunActive` if a run is in progress.
    /// If the ending side's hand is over its max hand size, transitions to
    /// `GamePhase::Discard { side, required }` instead of handing control
    /// over immediately — see `turn::end_turn`'s doc comment. Otherwise,
    /// control passes to the Corp, `turn::end_turn` also performs their
    /// mandatory start-of-turn draw from R&D into HQ automatically.
    EndTurn,
    /// Discard `card_id` from hand to satisfy a pending mandatory discard.
    /// Symmetric — no `side` field; the acting side is whichever side
    /// `GameState::phase` is currently `Discard { side, .. }` for. Errors if
    /// the phase isn't `Discard` (`RulesError::NotInDiscardPhase`) or the
    /// card isn't in that side's hand (`RulesError::CardNotInHand`). Once the
    /// phase's `required` count reaches zero, transitions to the other
    /// side's `GamePhase::StartOfTurn` — see `turn::discard_card`'s doc
    /// comment.
    DiscardCard { card_id: CardId },
}
