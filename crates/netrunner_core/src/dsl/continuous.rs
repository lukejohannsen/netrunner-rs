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
//! assumed every value is a number (docs/archive/jinteki-comparison.md §6.2).
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
/// card in the pool prints** (the DSL Growth Rule): the one the comparison
/// with jinteki.net names and no card here needs yet is "cannot be
/// broken".
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
    /// Clicks the player gains as their turn begins, on top of the rules'
    /// allotment (CR 1.11.2, 5.7.1a) — Basilar Synthgland 2KVJ's "You get
    /// +1 allotted [click] for each of your turns" (`Scope::Controller`).
    /// Asked by `turn::enter_start_of_turn` as it assigns the allotment,
    /// beside the lingering `Lingering::AllottedClicks` an earlier turn made
    /// for this one. Composition didn't work: that one is made once by
    /// something that resolved and is spent on the turn it names; this one
    /// is made by nothing and holds every turn the card is installed.
    AllottedClicks(Number),
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
    /// What the Runner pays, as an additional cost, to steal an agenda —
    /// Magistrate Revontulet's "as an additional cost to steal an agenda,
    /// the Runner must pay 3[c]", Daniela Jorge Inácio's "…the Runner must
    /// add 2 cards from the grip at random to the bottom of the stack". On
    /// top of the agenda's printed `steal_cost` (Méliès City Luxury Line),
    /// with which it is one price (CR 1.16.10: additional costs are paid
    /// together with it; the Runner may decline to steal rather than pay,
    /// CR 1.17.3d). A `Cost`, as `ScoreCost` is: it was a number of
    /// credits until Daniela's was a card.
    StealCost(crate::dsl::Cost),
    /// What the Runner pays, beside the trash cost, to trash a card they
    /// access — Daniela Jorge Inácio's "As an additional cost to trash
    /// this upgrade, the Runner must add 2 cards from the grip at random to
    /// the bottom of the stack" (`Scope::This`). Offered with the trash and
    /// paid with it (CR 1.16.10b), and not payable means no trash (the
    /// access's `AccessPhase::PendingChoice::trash_also`). Not
    /// `TrashCost`, which is credits and changes the printed number.
    AdditionalTrashCost(crate::dsl::Cost),
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
    /// The agenda points the player needs to win, added to the match's
    /// threshold (`MatchRules::winning_agenda_points`) — Issuaq Adaptics:
    /// Sustaining Diversity's "For each hosted power counter, you need 1
    /// less agenda point to win the game" (`Scope::Controller`, a negative
    /// `per` of `HostedCounters`). Asked, never stored
    /// (`continuous::points_to_win`), by the win check at every checkpoint
    /// (CR 10.3.1c) and by the view. Composition didn't work: the
    /// threshold was a match rule, read off the state.
    AgendaPointsToWin(Number),
    /// The agenda's advancement requirement, added to what it prints —
    /// Ontological Dependence's "This agenda gets −1 advancement
    /// requirement for each core damage the Runner has taken this game".
    /// Asked, never stored (`continuous::advancement_requirement`), by the
    /// score, by its dividends (CR 10.13.2, before the agenda moves) and by
    /// the view; the number may fall to 0 or below (CR 1.1.3), and an
    /// agenda whose requirement is 0 or less may be scored with no counters
    /// on it (CR 1.17.3a). About `This`, which CR 9.1.8e makes active
    /// wherever the agenda is, or an upgrade's agendas in its root
    /// (`RootOfThisServer`, SanSan City Grid's "each agenda in the root of
    /// this server", for as long as the grid is rezzed). Composition didn't work: the
    /// requirement was the printed field, read at the score.
    AdvancementRequirement(Number),
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
    /// "You can rez this ice any time you could rez non-ice cards" —
    /// Rime's, `while` a run is against its server
    /// (`RunAgainstThisServer`): the windows marked (R) (CR 9.2.7c) and the
    /// Corp's own action phase, beside the approach of the ice itself,
    /// never instead of it. About `This`, on ice; the rez handler asks it
    /// (`continuous::rezzed_as_non_ice`), and with it the action list,
    /// whose probe applies the rez. A kind because it is a static ability
    /// of the card's, read face down as every `Scope::This` effect is.
    /// Composition didn't work: when a card may be rezzed was the rez
    /// handler's rule, read off the card's type alone.
    RezzedAsNonIce,
    /// A card may be installed onto this one — Hackerspace's "You can
    /// install unique companion resources and unique connection resources
    /// onto this resource". A permission, and so a standing effect of the
    /// host's, about the cards its `Scope::InstallingOntoThis` filter
    /// admits; `continuous::may_install_onto` is the one question, put by
    /// the action list and by the install. No payload: which cards is the
    /// scope's to say.
    MayHost,
    /// This card hosts up to this many [mu] of programs, and the memory
    /// costs of the programs it hosts do not count against the Runner's
    /// memory limit — Djinn's "Djinn can host up to 3[mu] of non-icebreaker
    /// programs. The memory costs of hosted programs do not count against
    /// your memory limit." About `This`; which programs is `MayHost`'s to
    /// say. Asked by `continuous::may_install_onto` (the room left) and by
    /// `memory::memory_balance` (the programs it leaves out). One kind for
    /// both sentences because the room is what the memory is free within.
    /// Composition didn't work: `Memory` adds to the limit whatever is
    /// hosted, and nothing limited what a host may take.
    HostsMemory(Number),
    /// A run it applies to cannot be declared successful — Flagship's
    /// "Runs against this server cannot be declared successful. (This effect
    /// does not cause runs to become unsuccessful.)" The Success Phase is
    /// still reached and the server still breached (CR 6.9.5, 6.8.4a); only
    /// the declaration, and everything that hears it, is withheld
    /// (`engine::complete_run`). About `RunsOnThisServer`.
    CannotBeDeclaredSuccessful,
    /// What the Runner pays, as an additional cost, to run a server — Earth
    /// Station: SEA Headquarters' "As an additional cost to run HQ, the
    /// Runner must pay 1[credit]" and its flip side's "…to run a remote
    /// server, the Runner must pay 6[credit]" (`Scope::Runs`, under a
    /// `while` saying which face is up). Paid as the server is announced
    /// (CR 6.3.2b), all at once (1.16.10b), by `run::start_run` — the one
    /// door every run comes through, the basic action's and a card's — and
    /// a run that cannot pay it is refused, so the basic action is not
    /// offered. A `Cost`, as `StealCost` is. Composition didn't work:
    /// nothing priced a run.
    RunCost(crate::dsl::Cost),
    /// The Corp may have at most this many remote servers — A Teia: IP
    /// Recovery's "Limit 2 remote servers" (`Scope::Controller`). Asked
    /// wherever a new remote would be made: every Corp install
    /// (`engine::place_corp_card`, which refuses one) and every offer of a
    /// fresh remote (`legal_actions::may_add_remote`). Composition didn't
    /// work: nothing limited the servers, and a new remote is not an
    /// install a `Cannot` could name without its count.
    RemoteServerLimit(u32),
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
    /// The Runner cannot break subroutines on this ice using a program of
    /// this subtype — Swordsman's "The Runner cannot break subroutines on
    /// this ice using **AI** programs", and Hortum's with a `while` of three
    /// hosted advancement counters. About `This`, on ice, and asked by both
    /// break effects of the breaking card (`ability::breakable_now`), which
    /// then find nothing to break, so the ability is not offered. Every
    /// subroutine, a gained one too. Composition didn't work: `BreakLimit`
    /// counts printed subroutines only and excepts a subtype rather than
    /// naming one, and `Prohibition::BreakSubroutinesOnIce` is about every
    /// Runner card and is made by a choice with a duration.
    CannotBeBrokenUsing(crate::dsl::CardSubtype),
    /// This ice's strength cannot be lowered — Lotus Field's "The strength
    /// of this ice cannot be lowered". About `This`, on ice. Asked by
    /// `continuous::installed_ice_strength`, the one question a strength
    /// is put to, which then counts only the terms that raise it: an
    /// effect that would lower it is not refused, it does nothing to this
    /// ice (CR 1.2.2, the "cannot" takes precedence), and every other
    /// piece of ice it reaches is lowered as before. Composition didn't
    /// work: a `Strength` of the opposite sign would be a raise, not a
    /// floor, and the strength still moves up.
    StrengthCannotBeLowered,
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
    /// The card loses all its abilities but its printed subroutines (CR
    /// 9.1.9a) — Hush's "Host ice … loses all abilities except its printed
    /// subroutines" (`Scope::Host`), and Magnet's "Each hosted program
    /// loses all abilities" (`Scope::Hosted`). A lost ability "is completely
    /// ignored": no trigger of the card's is heard (`listeners`), none of
    /// its standing effects applies (this scan), none of its paid abilities
    /// is used and no credits it hosts are spent; `rules::active::
    /// lost_abilities` is the one question all of them put. **A printed
    /// subroutine is never lost**: the one card in the pool that takes a
    /// piece of ice's abilities keeps them, and the other loss in the pool
    /// (Klevetnik's, `Lingering::LosesAbilities`) is of a resource, which
    /// prints none. Read by a direct scan of the rig, never through this
    /// one, so a card's loss never depends on what it has lost (CR
    /// 9.12.1e: a hosted object's effects do not depend on its host's).
    /// Composition didn't work: nothing took a card's abilities away.
    LosesAbilities,
    /// The ice has `count` copies of `subroutine` beside the ones it prints
    /// — Echo's "This ice gains “[subroutine] End the run.” for each hosted
    /// power counter" (after, CR 9.8.3d) and Envelopment's "…**before its
    /// other subroutines** for each hosted power counter" (9.8.3b). About
    /// `This`, on ice, and nothing else: a subroutine another card grants
    /// is 9.8.3a or 9.8.3e, which `Effect::GainSubroutine` already says.
    /// Written into the encountered ice's list as each encounter begins
    /// (`run::engine::add_own_subroutines`), between those two categories,
    /// so the five fall in CR 9.8.2's order. Composition didn't work:
    /// `GainSubroutine` is triggered, and a "when encountered" trigger is
    /// one AirbladeX could prevent and would sort among another card's
    /// grants; this is a static ability of the ice, and it is never
    /// prevented, only lost (Hush, which this scan already reads).
    Subroutines {
        subroutine: Box<crate::dsl::SubroutineDef>,
        count: Number,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        before: bool,
    },
    /// The card cannot gain abilities (CR 9.1.9) — Hush's "Host ice cannot
    /// gain abilities" (`Scope::Host`): no subroutine is added to it
    /// (`Effect::GainSubroutine`, a run's `gained_for_the_run`) and no
    /// ability another card grants it is heard (`TriggeredEffect::
    /// granted`, ZATO City Grid's). Apart from `LosesAbilities`, which
    /// keeps what is printed: the two are two sentences on the card, and
    /// a granted ability gained before Hush arrived is lost by the first.
    CannotGainAbilities,
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
    /// Each card hosted on this one — Magnet's "Each hosted program loses
    /// all abilities and cannot gain abilities". `Host` read from the other
    /// end: the ice says it of what it hosts, where Hush says it of its
    /// host. Composition didn't work: `Host` is read from the hosted card,
    /// and `Rig(_)` reaches every program, hosted here or not.
    Hosted,
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
    /// Each piece of ice matching the filter — Fransofia Ward's "each piece
    /// of ice" (`Any`), Cat's Cradle's "each piece of **code gate** ice".
    /// The filter is read off the definition and off the copy, as
    /// `IceProtectingThisServer`'s is; `IceOfType` is a definition word, so
    /// a code gate is one as printed (a subtype gained for a run is not
    /// asked of a rez cost — on the known-limits list). It was a bare `Ice`
    /// until Cat's Cradle: no scope said "each piece of ice of a kind"
    /// anywhere on the table, and `IceProtectingThisServer` is about one
    /// server.
    Ice(CardFilter),
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
    /// A card the Runner is accessing, matching the filter — Demolisher's
    /// "The trash cost of each Corp card is lowered by 1[credit]", in HQ
    /// and R&D as much as in a root. Asked about the accessed card itself
    /// (`continuous::Target::Card`), beside the install's own question that
    /// `RootOfThisServer` answers (`continuous::trash_cost_delta`).
    /// Composition didn't work: every other scope that reaches a Corp card
    /// reaches an install, and a card accessed out of HQ is none.
    Accessing(CardFilter),
    /// An agenda the Runner is stealing, matching the filter — Magistrate
    /// Revontulet's "an agenda".
    Stealing(CardFilter),
    /// An agenda the Runner is stealing from the server this card is
    /// installed in — Daniela Jorge Inácio's "to steal an agenda from this
    /// server or its root": accessed in a breach of that server, so one in
    /// HQ's hand or R&D's deck as well as in its root. `Stealing`'s filter
    /// is about the card and cannot say where it was accessed. A
    /// persistent upgrade trashed during the run keeps it (the run's
    /// server).
    StealingFromThisServer,
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
    /// Each run against a server of this kind — Earth Station: SEA
    /// Headquarters' "to run **HQ**" and "to run **a remote server**".
    /// Asked about `continuous::Target::Run`, as `RunsOnThisServer` is, but
    /// said by any active card, since the server is named rather than the
    /// card's own. Only `RunCost` is about it.
    Runs(crate::dsl::ServerKind),
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
    /// the Corp's ice binds the Runner. A `Cannot` is about one, and so is
    /// a `HandSize` about the other player (Dr. Vientiane Keeling's "The
    /// Runner gets -1 maximum hand size"), asked the same way by
    /// `continuous::hand_size`.
    Player(Side),
}

impl Scope {
    /// Whether the effect is the card's own text about itself, read from
    /// the card wherever it is rather than from the active cards.
    pub fn is_own_text(&self) -> bool {
        matches!(self, Scope::This | Scope::ScoreArea(_))
    }
}
