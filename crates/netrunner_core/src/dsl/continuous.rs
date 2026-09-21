//! What a card does for as long as it is active — "while X, Y" — as data.
//!
//! Every standing effect used to be its own field, added for the card that
//! needed it: `StrengthModifier`'s six variants, nine one-off
//! `CardDefinition` fields (`memory_bonus`, `ice_rez_cost_modifier`,
//! `host_ice_gains_subtypes`, …), each with its own scan of the board in
//! whichever handler priced the thing. A card that "costs less" or "gets
//! +1" was a Rust edit with no new mechanic in it (ROADMAP Rules Audit,
//! jinteki comparison, backlog item 2).
//!
//! A `ContinuousEffect` says three things, and the printed sentence says the
//! same three: **what** changes (`kind`, with the payload that kind needs —
//! a number, a subtype, nothing at all), **which cards it is about**
//! (`applies_to`, read relative to the card that prints it) and **whether it
//! is on** (`while`). The first proposal was `{ kind, value: Amount, while }`
//! and was too narrow twice over: it could not say which cards, and it
//! assumed every value is a number (docs/jinteki-comparison.md §6.2).
//!
//! Nothing here is stored. `rules::continuous` scans the active cards at
//! each question, the way `rules::memory` always derived the budget — there
//! is deliberately no registry of "effects in play" to keep in sync with the
//! table. An effect with a *duration* ("for the remainder of this run") is a
//! different thing, resolved when it is created, and lives on `GameState`.

use serde::{Deserialize, Serialize};

use crate::dsl::ability::EffectRequirement;
use crate::dsl::card::IceType;
use crate::dsl::effect::Amount;
use crate::dsl::zone::CardFilter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuousEffect {
    pub kind: ContinuousKind,
    pub applies_to: Scope,
    /// Whether the effect is on, asked as the printing card each time the
    /// layer is asked — Carmen's "if you made a successful run this turn".
    /// A `OncePerTurn` here is spent by the one caller that *uses* the
    /// number rather than reads it (`continuous::pay_install_cost_of`).
    #[serde(default, rename = "while", skip_serializing_if = "Option::is_none")]
    pub condition: Option<EffectRequirement>,
    /// "The **first** program you install each turn": the effect reaches
    /// only the turn's first install its `Scope::Installing` filter
    /// matches. Read off `rules::turn_log` — none yet this turn, since an
    /// install is priced before it happens — so nothing is spent and a
    /// card that arrives after the turn's first program has missed it.
    /// `validate` refuses it on any other scope. See
    /// `TriggeredEffect::first_each_turn`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_each_turn: bool,
    /// The printed sentence this implements, quoted from the card — see
    /// `AbilityDef::text`. Optional like `TriggeredEffect::text`: nobody is
    /// asked to choose a continuous effect, so no prompt depends on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// `per × of`: signed, because a discount and a penalty are the same kind,
/// over an unsigned count, because `Amount` is one. `{ "per": -1 }` is a flat
/// −1; `{ "per": 1, "of": "InstalledIcebreakerCount" }` is Echelon.
///
/// Not a signed `Amount`: every other reader of `Amount` deals damage, draws
/// cards or gains credits, and none of those has a negative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Number {
    pub per: i32,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub of: Amount,
}

fn one() -> Amount {
    Amount::Fixed(1)
}

fn is_one(amount: &Amount) -> bool {
    *amount == Amount::Fixed(1)
}

/// What changes. Closed, with a payload per kind, and **only the kinds a
/// card in the pool prints** (the DSL Growth Rule): the ones the comparison
/// with jinteki.net names and no card here needs yet are additional
/// subroutines, "cannot be broken", agenda points and advancement
/// requirements that change while installed, additional costs to steal,
/// what a card can host, the number of cards accessed, and a standing
/// prohibition (every "cannot" in the pool has a duration, so it is an
/// `Effect::Prohibit` and `continuous::cannot` reads the lingering list).
/// Each is a variant here when a card prints it — never a field on
/// `CardDefinition`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContinuousKind {
    /// Strength of an icebreaker (`continuous::breaker_strength`) or of a
    /// piece of ice (`continuous::ice_strength`) — Ice Wall's "+1 strength
    /// for each hosted advancement counter", Palisade's "+2 while protecting
    /// a remote server". The ice half was `StrengthModifier`, baked into
    /// the run's copy of the ice when the run began.
    Strength(Number),
    /// Memory units available to the player — a console's "+1[mu]".
    Memory(Number),
    /// Maximum hand size of the player — "you get +1 maximum hand size".
    /// It was the one standing number still *stored*: folded into the state
    /// at install, at a score and at setup, and never taken back out, so a
    /// trashed T400 Memory Diamond and a forfeited Superconducting Hub kept
    /// paying.
    HandSize(Number),
    /// Credits to install a card; negative is a discount.
    InstallCost(Number),
    /// Credits to rez a card.
    RezCost(Number),
    /// Credits for the Runner to trash a card they access.
    TrashCost(Number),
    /// The ice gains a subtype it does not print — Chromatophores.
    GainSubtype(IceType),
    /// A strength boost that would last the encounter lasts the run
    /// instead — GAMEDRAGON™ Pro. A fact about the card, with no payload,
    /// which is the kind the numbers-only proposal had no room for.
    BoostsLastTheRun,
}

/// Which cards an effect is about, read from the card that prints it.
///
/// Not a `CardFilter`, which is what the roadmap first wrote: a filter
/// variant is legal in every `PromptChooseCards` and every
/// `EventFilter::Card`, and "the card I am hosted on" or "the root of my
/// server" means nothing there — `NotSourceCard` and `InAttackedServer`
/// already strain that enum. A `Scope` carries the relation to the source
/// and wraps a `CardFilter` where the sentence goes on to say what kind of
/// card ("each *asset* in the root of this server").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    /// This card — wherever it is, since a card's own text about itself
    /// applies from the grip too ("this program costs 2[c] less to install").
    This,
    /// The card this one is hosted on, program or ice.
    Host,
    /// The player who controls this card.
    Controller,
    /// A card its controller is installing, matching the filter.
    Installing(CardFilter),
    /// Each piece of ice.
    Ice,
    /// Each card in the root of the server this one is installed in,
    /// matching the filter.
    RootOfThisServer(CardFilter),
}
