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
    ///
    /// Deliberately **not** named `PurgeTags`: in Netrunner "purge" is a
    /// term of art for the Corp's basic action that removes virus counters
    /// (`PlayerAction::PurgeVirusCounters`), and nothing else. Reusing the
    /// word for tags made this look like that action's cost, which it has
    /// never had anything to do with.
    ClearTags,
    /// The Runner accepts `u32` tags as payment — e.g. Funhouse's "end the
    /// run unless the Runner takes 1 tag." Only ever meaningful as
    /// `Effect::OfferPaidChoice`'s `cost` (there's no `AbilityDef` in this
    /// baseline that costs a tag to activate), but modeled as a `Cost`
    /// rather than folded into `OfferPaidChoice` itself so it composes with
    /// `Cost::AnyOf` the same way every other cost does.
    TakeTags(u32),
    /// The payer chooses which of these to pay — e.g. Manegarm Skunkworks's
    /// "spend [click][click] or pay 5 credits." Resolving *which* option is
    /// a player decision, not something `pay_cost` can pick on its own —
    /// see `Effect::OfferPaidChoice`/`PendingPaidChoice::cost_option_index`,
    /// the only place this is ever paid from. `pay_cost` itself rejects a
    /// raw `AnyOf` with `RulesError::CostRequiresChoice` if ever handed one
    /// directly (it never should be — the choice is resolved before
    /// `pay_cost` is called).
    AnyOf(Vec<Cost>),
    /// Spend `u32` of the acting card's own hosted generic counters (see
    /// `state::InstalledCard`/`InstalledRunnerCard::counters`) — e.g.
    /// Botulus's "hosted virus counter: break 1 subroutine on host ice."
    /// `pay_cost` errors `RulesError::InsufficientCounters` if fewer than
    /// this many are available; otherwise removes them via the same
    /// counter-mutation path `Effect::RemoveCounters` uses.
    RemoveCounters(u32),
    /// Removes the acting card from the game entirely — Spin Doctor's
    /// "Remove this asset from the game:" ability cost. Distinct from
    /// `TrashSelf`: a trashed card goes to Archives (where it stays
    /// accessible and countable), whereas a removed one goes to
    /// `CorpState::removed_from_game` and is gone for good.
    /// `RulesError::MissingActingCardContext` without an acting card, and
    /// `RulesError::CardNotInstalled` if it isn't a Corp install.
    RemoveSelfFromGame,
    /// The Corp reveals and trashes `u32` cards from HQ at random — Shred's
    /// "unless the Corp reveals and trashes X cards from HQ at random",
    /// built by the engine into the `OfferPaidChoice` it parks (X is the
    /// attacked server's root count, known only then). Affordable only
    /// with at least that many cards in HQ; the cards land faceup, being
    /// revealed. Drawn with the state's own PRNG, so a replay trashes the
    /// same cards.
    TrashRandomFromHq(u32),
    /// Every listed cost is paid, in order — Humanoid Resources' "[click]
    /// [click][click], [trash]: …", a click cost *and* a self-trash on one
    /// ability. `AnyOf`'s conjunctive twin: affordable only when every
    /// part is, paid part by part with no choice to resolve, so unlike
    /// `AnyOf` it goes straight through `pay_cost`. Not expressible
    /// otherwise: `AbilityDef::cost` is one `Cost`, and folding the trash
    /// into the effect would resolve it *after* the ability instead of as
    /// its price.
    AllOf(Vec<Cost>),
}
