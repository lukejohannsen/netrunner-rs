use serde::{Deserialize, Serialize};

/// What a player must pay to activate a `Paid`-triggered `AbilityDef`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cost {
    Credits(u32),
    Clicks(u32),
    /// Trash the card this ability is printed on, as part of paying to
    /// activate it — distinct from `Effect::TrashCard(CardTarget::
    /// ThisCard)`, which is an effect (something the ability does),
    /// versus this, which is a cost (something paid before the ability
    /// resolves).
    TrashSelf,
    /// Remove all of the Runner's tags. A legal no-op at 0 tags, the same
    /// as a `Credits(0)`/`Clicks(0)` cost would be.
    PurgeTags,
}
