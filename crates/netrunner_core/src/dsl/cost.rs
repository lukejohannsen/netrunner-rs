use serde::{Deserialize, Serialize};

use super::effect::{Amount, DamageType};
use super::zone::{CardFilter, CardZoneRef};

/// What a player must pay to activate a `Paid`-triggered `AbilityDef`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cost {
    Credits(u32),
    /// Credits paid only from the cards `from` matches — "Spend credits
    /// only from **stealth** cards to use this ability" (Corsair, Lampades,
    /// Baker: `HasSubtype(Stealth)`), CR 1.10.4b read from the payer's
    /// side. The credit pool is not a card, so it cannot pay
    /// (`payment::sources`' `from`). `amount` is an `Amount` because
    /// Lampades's is "the printed rez or play cost of the card you are
    /// accessing" (`Amount::AccessedCardPrintedCost`). Not a restriction
    /// on the ability beside `Credits`: the printed sentence limits the
    /// credits, and a cost is where the credits are.
    CreditsFrom { amount: Amount, from: CardFilter },
    Clicks(u32),
    /// The payer loses `u32` clicks — Ansel 2.0's "**Lose [click][click]:**
    /// Break up to 2 subroutines on this ice." Not `Clicks`: that cost
    /// spends them, and a cost that begins with [click] makes the ability
    /// an action (CR 9.5.2a), which a Runner mid-encounter cannot take.
    /// This one begins with "lose", so it is used in the encounter's
    /// window. Payable only with that many clicks (CR 1.16.1).
    LoseClicks(u32),
    /// The Runner jacks out — Lionsmane's "Do 2 net damage unless the
    /// Runner jacks out", a nested cost (CR 1.16.11b: "[instructions]
    /// unless [player] [cost]"), so an `Effect::OfferPaidChoice`'s price.
    /// Not `Effect::EndTheRun`: a jack-out is the Runner's, ends the run as
    /// `GameEvent::RunJackedOut`, and is nothing Shred's "the first time
    /// … would end the run" can stop. Payable during a run.
    JackOut,
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
    /// Remove `u32` of the Runner's tags — Synapse Global: Faster than
    /// Thought's "[click], remove 1 tag: Gain 2[credit]." Payable only with
    /// that many tags to remove (Comprehensive Rules 1.16.1: a cost is paid
    /// in full or not at all), which is what the ability's `IsTagged`
    /// requirement stood in for while the removal was the effect's first
    /// step. Not `ClearTags`, which removes however many there are and is
    /// payable at none.
    RemoveTags(u32),
    /// The Runner accepts `u32` tags as payment — e.g. Funhouse's "end the
    /// run unless the Runner takes 1 tag." Only ever meaningful as
    /// `Effect::OfferPaidChoice`'s `cost` (there's no `AbilityDef` in this
    /// baseline that costs a tag to activate), but modeled as a `Cost`
    /// rather than folded into `OfferPaidChoice` itself so it composes with
    /// `Cost::AnyOf` the same way every other cost does.
    TakeTags(u32),
    /// The Runner suffers `u32` damage of a type as payment — Semak-samun's
    /// "End the run unless the Runner suffers 3 net damage", the damage
    /// twin of Funhouse's `TakeTags`. Paid through `damage::apply_damage`
    /// and never `prevention::would`: an optional interrupt cannot modify
    /// or cancel the paying of a cost (Comprehensive Rules 1.16.1a), so Net
    /// Shield is not asked, and nothing announces the damage as about to
    /// resolve. As a `PresentChoice` option it was an effect, and it was.
    ///
    /// Payable only with at least that many cards in the grip (1.16.1: a
    /// cost is paid in full). A Runner with fewer can only let the run
    /// end; the damage would flatline them, which is not paying it. A
    /// mandatory interrupt that would prevent the damage would also make
    /// the cost unpayable (1.16.1b); no pool card prints one.
    SufferDamage(DamageType, u32),
    /// The payer trashes `count` of their own cards in `from` that match
    /// `filter` — Carnivore's "Trash 2 cards from your grip:", LEO
    /// Construction's "Trash 1 rezzed bioroid card in the root of or
    /// protecting the attacked server:", Anoetic Void's "pay 2[credit] and
    /// trash 2 cards from HQ". Each was a `PromptChooseCards` in the
    /// effect with a `ZoneHasAtLeast` requirement standing in for
    /// affordability, which let the requirement and the payment drift
    /// apart, and resolved the rest of the ability as the card it had just
    /// trashed (LEO's "End the run." ran as the bioroid).
    ///
    /// Affordable when that many cards match; the payer is asked which, one
    /// card at a time, only when the answer changes what they are left with
    /// (`payment::Ask::Card`). A card trashed from HQ lands facedown unless
    /// `reveal`; a trashed install that was faceup stays faceup. Never
    /// prevented: the payer is choosing among their own cards (CR 1.16.1a).
    /// The Corp forfeits `u32` agendas from its score area — Biawak's "you
    /// can forfeit 1 agenda as you rez this ice", Plutus's "as an
    /// additional cost to rez this asset, forfeit 1 agenda". Affordable
    /// with that many scored; the Corp is asked which (`payment::Ask::Card`
    /// over `CardZoneRef::OwnScoreArea`) whenever more than one could go.
    /// It was `Effect::ForfeitAgendas`, which took the lowest-scoring
    /// agenda unasked on the argument that it always dominates — true of
    /// points, and not of an agenda whose counters are worth spending
    /// first, which is the Corp's to weigh. A forfeited agenda leaves the
    /// game, and "when you forfeit this" hears it (Greenmail), dispatched
    /// by the payer.
    Forfeit(u32),
    Trash {
        from: CardZoneRef,
        filter: CardFilter,
        count: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reveal: bool,
    },
    /// The Corp derezzes `count` of its own rezzed installs that match
    /// `filter` — Kompromat's "give the Corp 1 bad publicity unless they
    /// derez 1 piece of ice protecting the attacked server", a nested cost
    /// (CR 1.16.11b) the Corp pays or declines. `Trash`'s shape and its
    /// asking: affordable when that many match, and the Corp is asked which
    /// one card at a time, only when the answer changes what it is left
    /// with (`payment::Ask::Card` over `OwnInstalled`). A card that is not
    /// rezzed is never eligible, whatever the filter says, because a
    /// facedown card cannot be turned facedown (CR 8.1.3). Composition
    /// didn't work: `Effect::DerezCard` is an effect, and the Payment Rule
    /// holds a printed cost to being a `Cost` — as a `PresentChoice`
    /// option the derez could be chosen with no ice to derez.
    Derez {
        filter: CardFilter,
        count: u32,
    },
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
    /// Remove `u32` advancement counters hosted on the acting card —
    /// Sacrifice Zone Expansion's "you may remove 1 hosted advancement
    /// counter to do 1 meat damage". Not `RemoveCounters`: advancement
    /// counters are not the card's generic counters
    /// (`InstalledCard::advancement_tokens`). Payable only with that many
    /// (CR 1.16.1).
    RemoveAdvancementCounters(u32),
    /// The card this is printed on goes to the Corp's score area "as an
    /// agenda" (CR 10.1.3) — Word on the Street's "As an additional cost to
    /// score an agenda the Corp installed this turn, they must add this
    /// resource to their score area as an agenda worth −1 agenda points".
    /// Paid by the Corp, out of the Runner's rig: an additional cost is the
    /// payer's, wherever the card that imposes it is
    /// (`ContinuousKind::ScoreCost`). The effect of the same name is the
    /// same move made by a card's own text. Payable while the card is
    /// installed.
    AddToScoreAreaAsAgenda(crate::dsl::AsAgenda),
    /// Removes the acting card from the game entirely — Spin Doctor's
    /// "Remove this asset from the game:" ability cost. Distinct from
    /// `TrashSelf`: a trashed card goes to Archives (where it stays
    /// accessible and countable), whereas a removed one goes to
    /// `CorpState::removed_from_game` and is gone for good.
    /// `RulesError::MissingActingCardContext` without an acting card, and
    /// `RulesError::CardNotInstalled` if it isn't a Corp install.
    RemoveSelfFromGame,
    /// The card this is printed on, in its owner's hand, is revealed (CR
    /// 1.21.3) and trashed — Tocsin's "reveal and trash this ice from HQ:".
    /// The Runner has seen it, so it goes to Archives faceup (CR 4.4.6b).
    /// The price of an ability used from the hand (`AbilityDef::from_hand`),
    /// so affordable only while the card is there. Distinct from
    /// `TrashSelf`, which trashes a card out of HQ facedown.
    RevealAndTrashSelf,
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

impl Cost {
    /// Whether this cost, as printed, begins with a [click] symbol — what
    /// makes a paid ability an action (CR 9.5.2a). `AllOf` is printed in
    /// order ("[click], [trash]:"), so its first part decides; `AnyOf` is
    /// only ever an `OfferPaidChoice`'s price, never an ability's trigger
    /// cost.
    pub fn begins_with_click(&self) -> bool {
        match self {
            Cost::Clicks(n) => *n > 0,
            Cost::AllOf(costs) => costs.first().is_some_and(Cost::begins_with_click),
            _ => false,
        }
    }

    /// Whether paying this takes the ability's own source off the table —
    /// "[trash]:", or a removal from the game. Its hosted cards are then
    /// set aside for an effect that acts on them (CR 9.5.5).
    pub fn uninstalls_its_source(&self) -> bool {
        match self {
            Cost::TrashSelf | Cost::RemoveSelfFromGame => true,
            Cost::AnyOf(costs) | Cost::AllOf(costs) => costs.iter().any(Cost::uninstalls_its_source),
            _ => false,
        }
    }

    /// Whether paying this could ask the payer which cards — the structural
    /// half of `payment::could_ask`, which copies an action only where a
    /// question is possible.
    pub fn may_ask(&self) -> bool {
        match self {
            Cost::Trash { .. } | Cost::Forfeit(_) | Cost::Derez { .. } => true,
            Cost::AnyOf(costs) | Cost::AllOf(costs) => costs.iter().any(Cost::may_ask),
            _ => false,
        }
    }
}
