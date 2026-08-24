use serde::{Deserialize, Serialize};

/// When (or how) an ability fires. Automatic variants name a rules-flow
/// moment; `Paid` marks an ability that never fires on its own and must be
/// explicitly activated by a player paying its `AbilityDef::cost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    OnPlay,
    OnRunStart,
    /// Renamed from `OnIceEncountered` — folded into a single "OnX"
    /// vocabulary with the other automatic variants.
    OnEncounter,
    /// Renamed from `StartOfTurn`, to disambiguate from the unrelated
    /// `rules::state::GamePhase::StartOfTurn(Side)` variant, which names a
    /// turn sub-phase, not a card-ability trigger condition.
    OnTurnStart,
    /// Fires once, the moment this card is presented as the Runner's
    /// current access choice (when `GameEvent::CardAccessed` fires for it)
    /// — Ambush-style "when accessed" abilities (Snare!). Always fires
    /// unconditionally and cannot itself be paid off. A card that needs a
    /// payable "avoid this" reaction (Fetal AI) models that separately via
    /// `dsl::ability::InteractiveOnAccess`/`Card::interactive_on_access`,
    /// resolved *before* `OnAccessed` via `rules::run::state::AccessPhase::
    /// PendingInteractiveTrigger`.
    OnAccessed,
    /// Fires when this card is trashed via `PlayerAction::
    /// TrashAccessedCard` specifically — not other trash paths (a
    /// subroutine's `Effect::TrashCard`, a normal Corp trash action, etc.).
    /// Shock!-style "when trashed by the Runner accessing it" abilities.
    OnTrashedFromAccess,
    /// Fires when a run completes successfully — distinct from
    /// `OnRunStart`, which fires at initiation, not resolution.
    OnSuccessfulRun,
    /// Not a moment in time — marks an ability that only resolves when a
    /// player explicitly activates it and pays `AbilityDef::cost`. Should
    /// only ever appear as an `AbilityDef::trigger`, never inside a
    /// `TriggeredEffect` (which has no `Cost` field to pay).
    Paid,
}
