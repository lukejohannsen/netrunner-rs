use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::ServerId;
use crate::rules::state::Side;

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
    /// unrezzed card. Corp-only (Runner grip/rig aren't modeled with card
    /// identity yet).
    InstallCard { card_id: CardId, zone: TargetZone },
    /// Flip an already-installed card face-up. Corp-only. No click cost (rez is
    /// not a click action) and no credit cost yet — rez cost is data-driven
    /// per-card via `dsl::Card`, and no `CardRegistry` is wired into the engine
    /// yet.
    RezIce { ice_id: CardId },
    /// Spend 1 click, start a run on `server`. Runner-only. The resulting
    /// `RunState::ice` is left empty — populating real ICE requires a
    /// `CardRegistry` lookup from `corp.installed` that doesn't exist yet.
    InitiateRun { server: ServerTarget },
    /// Voluntarily end the active run. Runner-only, no click cost. Delegates to
    /// `run::advance_run`'s `RunAction::JackOut`.
    JackOut,
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
}
