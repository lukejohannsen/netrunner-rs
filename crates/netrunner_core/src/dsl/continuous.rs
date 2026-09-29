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
use crate::dsl::card::{CardSubtype, IceType};
use crate::dsl::effect::Amount;
use crate::dsl::zone::CardFilter;
use crate::rules::Side;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuousEffect {
    pub kind: ContinuousKind,
    pub applies_to: Scope,
    /// Whether the effect is on, asked as the printing card each time the
    /// layer is asked — Carmen's "if you made a successful run this turn".
    /// Read and never spent: `validate` refuses a `OncePerTurn` here,
    /// because nothing that reads a standing effect *uses* the card.
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
/// with jinteki.net names and no card here needs yet are "cannot be
/// broken" and advancement requirements that change while installed.
/// A "cannot" about a player with a duration is an `Effect::Prohibit`, on
/// the lingering list; one that stands for as long as its card is active
/// is `Cannot`, and `continuous::cannot` reads both. The standing ones
/// about a run are about runs on a server (`RunsOnThisServer`).
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
    /// The Runner's link — The Toolbox's "+2[link]". What an identity prints
    /// is its `CardDefinition::base_link`; this is what an installed card
    /// adds while it is installed. Link was the other standing number still
    /// stored: `RunnerState::link_strength`, seeded once from the identity
    /// at setup, so there was nowhere for a console's link to be said at
    /// all. `continuous::link` is asked by the trace.
    Link(Number),
    /// Credits to install a card; negative is a discount.
    InstallCost(Number),
    /// Credits to rez a card.
    RezCost(Number),
    /// Credits for the Runner to trash a card they access.
    TrashCost(Number),
    /// Credits to play an event or operation; negative is a discount —
    /// Tailgate's "lowered by 1[credit] for each piece of ice protecting
    /// HQ". `continuous::play_cost_of` is the one question, asked by the
    /// play and by the offer.
    PlayCost(Number),
    /// Clicks to play an event or operation — Synchrocyclotron's "the
    /// first double operation you play each turn costs [click] less to
    /// play". A kind of its own rather than a unit on `PlayCost`: every
    /// other kind is one number in one unit, and a price in two would be
    /// the first. Taken off the Double's additional click
    /// (`CardDefinition::additional_play_cost`), which is the only click
    /// a card in the pool lowers; the action's own click is never lowered.
    PlayClicks(Number),
    /// Credits the Runner pays, as an additional cost, to steal an agenda —
    /// Magistrate Revontulet's "as an additional cost to steal an agenda,
    /// the Runner must pay 3[c]". On top of the agenda's printed
    /// `steal_cost` (Méliès City Luxury Line), with which it is one price
    /// (CR 1.16.10: additional costs are paid together with it; the Runner
    /// may decline to steal rather than pay, CR 1.17.3d).
    StealCost(Number),
    /// A cost the Corp pays, as an additional cost, to score an agenda —
    /// Word on the Street's "As an additional cost to score an agenda the
    /// Corp installed this turn, they must add this resource to their score
    /// area…", about the agendas `Scope::Scoring` admits. A `Cost` rather
    /// than `StealCost`'s number, because what is paid here is a card. Paid
    /// with the score, and then a checkpoint, before the agenda moves
    /// (CR 1.16.10b–c); the Corp may decline by not scoring (CR 1.17.3b).
    ScoreCost(crate::dsl::Cost),
    /// A cost the Corp pays, as an additional cost, to trash a resource with
    /// the basic action (CR 5.2.6g) — Sebastião Souza Pessoa's "As an
    /// additional cost to trash a **connection** resource with the basic
    /// action, the Corp must trash 1 card from HQ" (`Scope::Trashing`), and
    /// Manuel Lattes de Moura's "to trash **this** resource" (`Scope::This`,
    /// under a threat `while`). `ScoreCost`'s shape: a `Cost`, because what
    /// is paid is a card. Paid with the action's own [click] and 2[credit],
    /// all at once (CR 1.16.10b), and the Corp declines by not taking the
    /// action (CR 1.16.10a). Composition didn't work: nothing priced the
    /// basic action, whose cost was written in the handler.
    BasicTrashCost(crate::dsl::Cost),
    /// Agenda points an agenda is worth where it is — Let Them Dream's
    /// "while this agenda is in the Runner's score area, it is worth 1 less
    /// agenda point". Asked, never stored: the win check totals each score
    /// area's cards (`win::agenda_value_in`), and the stored tally beside it
    /// adds the same number when the card lands.
    AgendaPoints(Number),
    /// The ice gains a subtype it does not print — Chromatophores.
    GainSubtype(IceType),
    /// A strength boost that would last the encounter lasts the run
    /// instead — GAMEDRAGON™ Pro. A fact about the card, with no payload,
    /// which is the kind the numbers-only proposal had no room for.
    BoostsLastTheRun,
    /// The card is revealed while the Runner is accessing it — "While the
    /// Runner is accessing this asset in R&D, they must reveal it" (CR
    /// 1.21.7), the `while` saying where (`AccessingIn(RnD)`). Esca,
    /// Snare! and Byte! print it. A kind rather than a word on the access
    /// because it is a static ability of the card's, read wherever the card
    /// is, like every `Scope::This` effect.
    RevealedWhileAccessed,
    /// A card may be installed onto this one — Hackerspace's "You can
    /// install unique companion resources and unique connection resources
    /// onto this resource". A permission, and so a standing effect of the
    /// host's, about the cards its `Scope::InstallingOntoThis` filter
    /// admits; `continuous::may_install_onto` is the one question, put by
    /// the action list and by the install. No payload: which cards is the
    /// scope's to say.
    MayHost,
    /// A run it applies to cannot be declared successful — Flagship's
    /// "Runs against this server cannot be declared successful. (This effect
    /// does not cause runs to become unsuccessful.)" The Success Phase is
    /// still reached and the server still breached (CR 6.9.5, 6.8.4a); only
    /// the declaration, and everything that hears it, is withheld
    /// (`engine::complete_run`). About `RunsOnThisServer`.
    CannotBeDeclaredSuccessful,
    /// During a run it applies to, the Runner cannot access more than this
    /// many cards other than the card that says so — Flagship's "the Runner
    /// cannot access more than 1 card other than this upgrade". Once they
    /// have, every other candidate stops being one (CR 7.4.2b): it does
    /// nothing until a card is accessed, and never changes the random
    /// access limit. The card itself stays a candidate. Asked by the
    /// breach as it offers each candidate (`run::access::prune_candidates`).
    AccessOthersAtMost(u32),
    /// During each encounter with this ice, the Runner cannot break more
    /// than `at_most` of its printed subroutines, except using an icebreaker
    /// printed with `except_using` — Hammer's "cannot break more than 1 of
    /// its printed subroutines except using **killers**". A gained
    /// subroutine is not printed and is never limited; a break that is
    /// excepted is not counted. Asked by both break effects, which break
    /// no more than is left (`continuous::breaks_left`) and count what they
    /// broke in `RunState::this_encounter`. About `This`, on ice. Afshar
    /// and Akhet print it with no exception and a `while`.
    BreakLimit {
        at_most: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        except_using: Option<CardSubtype>,
    },
    /// The Corp cannot trash more than this many installed Runner cards
    /// with this ice during each encounter — Sorocaban Blade. Once it has,
    /// a selection this ice's text makes to trash an installed Runner card
    /// offers nothing (`continuous::may_trash_with`), and the trashes it
    /// has made are counted in `RunState::this_encounter`, where it was
    /// carried out: a trash the Runner prevented is not one. About `This`,
    /// on ice.
    TrashLimit(u32),
    /// A player cannot do something for as long as this is on — Attini's
    /// "Threat 3 → The Runner cannot spend credits while subroutines on
    /// this ice are resolving". About `Player`, the one the prohibition
    /// binds (`Prohibition::binds`), and asked by `continuous::cannot`
    /// beside the lingering list. Composition didn't work: every "cannot"
    /// before this one had a duration and was made by something that
    /// resolved (`Effect::Prohibit`), and this one is made by nothing — it
    /// is on while its `while` holds.
    Cannot(crate::dsl::Prohibition),
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
    /// A card its controller is installing *onto this card*, matching the
    /// filter — Hackerspace's "each resource installed this way costs
    /// 1[credit] less to install", and what it may host (`MayHost`). Not
    /// `Installing`: that reaches every install, and the host is what a
    /// price asked onto nothing cannot know (`continuous::Target::
    /// InstallingOnto`).
    InstallingOntoThis(CardFilter),
    /// Each piece of ice.
    Ice,
    /// Each card in the root of the server this one is installed in,
    /// matching the filter.
    RootOfThisServer(CardFilter),
    /// Each piece of ice protecting the server this one is installed in,
    /// matching the filter — Isaac Liberdade's "Each **advanced** piece of
    /// ice protecting this server gets +2 strength". The filter is read off
    /// the definition and off the copy (`pending_choice::copy_matches`), so
    /// an instance word (`Advanced`) is asked of the ice itself.
    /// Composition didn't work: `Ice` reaches every piece, and `while` is
    /// asked of the card that prints it, not of the ice.
    IceProtectingThisServer(CardFilter),
    /// Each card in the Runner's rig, matching the filter — Stegodon MK
    /// IV's "each installed **icebreaker** gets –2 strength". Composition
    /// didn't work: every other scope that reaches a rig card is the card's
    /// own (`This`) or its host (`Host`), and `Ice` is the Corp's table.
    Rig(CardFilter),
    /// An event or operation its controller is playing, matching the filter
    /// — Synchrocyclotron's "double operation". The play's half of
    /// `Installing`: both price a card from the hand, and `first_each_turn`
    /// counts plays here as it counts installs there.
    Playing(CardFilter),
    /// An agenda the Runner is stealing, matching the filter — Magistrate
    /// Revontulet's "an agenda".
    Stealing(CardFilter),
    /// An agenda the Corp is scoring, matching the filter — Word on the
    /// Street's "an agenda the Corp installed this turn". Asked of the
    /// install being scored (`continuous::Target::Scoring`), so an
    /// instance word (`InstalledThisTurn`) is read off that copy.
    Scoring(CardFilter),
    /// A resource the Corp is trashing with the basic action, matching the
    /// filter — Sebastião Souza Pessoa's "a **connection** resource". The
    /// trash action's half of `Scoring`: asked of the install being trashed
    /// (`continuous::Target::Trashing`), so a card's own text about itself
    /// is `This` and what an identity says about the rig is this.
    Trashing(CardFilter),
    /// Each run against the server this card is installed in — Flagship's
    /// "Runs against this server" and "During each run against this
    /// server". A run is not a card, so this is asked about
    /// `continuous::Target::Run`, and only by the run's own steps.
    RunsOnThisServer,
    /// This agenda, while it is in `side`'s score area — Let Them Dream.
    /// The card's own text, like `This`, and read only there: a question
    /// about a scored agenda says whose score area it is in.
    ScoreArea(Side),
    /// A player, named by side — Attini's "**The Runner** cannot spend
    /// credits". Not a card, so it is reached only by a question about a
    /// player that walks both sides' cards (`continuous::Target::Bound`):
    /// the Corp's ice binds the Runner. Only a `Cannot` is about one.
    Player(Side),
}

impl Scope {
    /// Whether the effect is the card's own text about itself, read from
    /// the card wherever it is rather than from the active cards.
    pub fn is_own_text(&self) -> bool {
        matches!(self, Scope::This | Scope::ScoreArea(_))
    }
}
