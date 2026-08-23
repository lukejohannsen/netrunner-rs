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
}
