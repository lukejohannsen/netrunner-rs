use serde::{Deserialize, Serialize};

use crate::dsl::card::{CardDefinition, CardType};
use crate::rules::ServerId;

/// Which zone a `PendingDecision::ChooseCards`/`Effect::PromptChooseCards`
/// reads candidates from, or moves chosen cards into. "Own"/"Opponent" are
/// relative to the choosing side (`Effect::PromptChooseCards::side`) — e.g.
/// Above the Law's Corp-side chooser reads `OpponentInstalled` to mean the
/// Runner's rig.
///
/// `OpponentInstalled`/`OwnInstalled` select among installed cards by
/// `CardId` alone (first match), the same simplification `PlayerAction::
/// RezIce`/`TrashResource`/`ActivateAbility` already make — this engine's
/// model never disambiguates duplicate installs of the same card by
/// server/position outside `CardTarget::CorpInstalled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardZoneRef {
    OwnHq,
    OwnArchives,
    OwnRAndD,
    /// The bottom of the chooser's own deck, R&D or the stack — a
    /// destination only: Reeducation's "add any number of cards from HQ to
    /// the bottom of R&D", the first chosen uppermost of them, as a
    /// selection onto the top is arranged. `Effect::AddToDeck(Bottom)` moves
    /// the one card a resolution acts as; a selection's `then` resolves
    /// once, as the first card chosen, so it could not move the rest.
    BottomOfOwnDeck,
    OwnStack,
    OwnGrip,
    OwnHeap,
    /// The Runner's own cards in the set-aside zone (`RunnerState::
    /// set_aside`, CR 4.8) — The Wizard's Chest's "You may install 1 of
    /// those 2 cards", chosen from what it set aside. Faceup, so a
    /// selection over it shows nothing new.
    OwnSetAside,
    /// The Runner's own cards set aside facedown (`RunnerState::
    /// set_aside_facedown`, CR 4.8.6) — Ayla "Bios" Rahim's "Shuffle 2 of
    /// those cards into your stack" and "Add 1 card set aside with this
    /// identity to your grip". The Runner may look at them, so a selection
    /// over it shows the chooser nothing new; the Corp sees how many.
    OwnSetAsideFacedown,
    /// The Runner's own cards installed facedown (`RunnerState::
    /// installed_facedown`, CR 8.1.4) — where Reboot's "install up to 5
    /// cards from your heap facedown" puts the cards it chose, as a
    /// selection's `destination`. The Runner may look at them; the Corp sees
    /// how many.
    OwnInstalledFacedown,
    /// The pile of other identities the Runner brought (`RunnerState::
    /// other_identities`, CR 1.5.4a) — DJ Fenris's "host a g-mod identity".
    /// Outside the game (CR 4.1.3) and in no zone, but the one source an
    /// ability that brings a card in from there names. The Runner may look
    /// at it at any time, so choosing from it shows nothing new.
    OtherIdentities,
    /// The other player's cards in the set-aside zone (CR 4.8): the Corp's
    /// (`CorpState::set_aside`) named from the Runner's side — Deep Dive's
    /// "Access 1 of those cards" and "the Corp shuffles the set-aside cards
    /// into R&D" — and the Runner's (`RunnerState::set_aside`) from the
    /// Corp's — Steve Cambridge's "the Corp removes 1 of those cards from
    /// the game", out of the 2 the Runner chose and set aside for it.
    /// Faceup. The other side's, as every `Opponent…` zone is: it was the
    /// Corp's from either chair while Deep Dive was the only card.
    OpponentSetAside,
    /// The opposing side's installed cards (Corp's `installed` if the
    /// chooser is Runner, or the Runner's `rig` if the chooser is Corp).
    /// Eligibility filtering is done by the enclosing `Effect::
    /// PromptChooseCards::filter`/`PendingDecision::ChooseCards::filter`,
    /// not here — this variant carries no `filter` field of its own to
    /// avoid two redundant filters.
    OpponentInstalled,
    /// The opposing side's discard pile — a destination only (Archives if
    /// the chooser is Runner, the Heap if the chooser is Corp), e.g. where
    /// Above the Law/Ballista/Retribution send the Runner card they trash.
    OpponentDiscard,
    /// The chooser's own installed cards — e.g. Send a Message choosing one
    /// of its own controller's installed ICE to rez for free. Same "no
    /// embedded filter" note as `OpponentInstalled`.
    OwnInstalled,
    /// The cards hosted on the parking card itself
    /// (`rules::state::InstalledRunnerCard::hosted_cards` of the decision's
    /// `source_install`) — Madani's faceup hosted programs. As a
    /// *destination* it hosts the selection there; as a *source* it offers
    /// them. Only meaningful with a Runner rig card as the parking card.
    HostedOnSource,
    /// The single top card of the Runner's stack — MuslihaT's "look at the
    /// top card of your stack ... you may reveal it and add it to your
    /// grip". A zone rather than a `PromptChooseCards` field: the prompt's
    /// eligibility, view and confirm paths all key off the zone, and a
    /// one-card zone falls out of each of them for free, where a "top N"
    /// field would have had to be threaded through all three.
    TopOfOwnStack,
    /// The opposing side's hand — the Runner's grip when the chooser is
    /// the Corp (Touch-ups' "reveal the grip. Choose up to 2 revealed
    /// cards of that type"). The Corp side is unused: no Runner card
    /// selects out of HQ.
    OpponentHand,
    /// The opposing side's deck — the Runner's stack when the chooser is
    /// the Corp. A *destination* only, and the one Touch-ups shuffles the
    /// cards it took back into; `PromptChooseCards::shuffle_after` does
    /// the shuffling, as it does for R&D.
    OpponentDeck,
    /// Every card in the root of the attacked server — Because I Can's
    /// "force the Corp to shuffle all cards in the root of that server into
    /// R&D". A source for `Effect::ShuffleIntoDeck` only, which moves every
    /// card in it, the Corp's whoever resolves it; never a selection's,
    /// and nothing outside a run. `CardTarget::AttackedServerRoot` says
    /// the same cards for a trash; a deck is a zone, so here it is one.
    RootOfAttackedServer,
    /// The cards in and in the root of the attacked server — Counter
    /// Surveillance's "access that many cards in and/or in the root of the
    /// attacked server", a source for `Effect::Access` only: its candidates
    /// are a breach's of that server (HQ and R&D whole, with no random
    /// access limit, Archives turned faceup, every root), and the accesses
    /// end once the number given has been chosen (CR 7.1.10).
    AttackedServer,
    /// The opposing side's score area — the Runner's, when the chooser is
    /// the Corp (IP Enforcement's "install 1 agenda from the Runner's
    /// score area"). A source only: nothing puts a card *into* an
    /// opponent's score area.
    OpponentScoreArea,
    /// The chooser's own score area — where a forfeited agenda comes from
    /// (`Cost::Forfeit`: Biawak's and Plutus's "forfeit 1 agenda"). A source
    /// only, like `OpponentScoreArea`.
    OwnScoreArea,
    /// Out of the game, on the opposing side — the Runner's cards removed
    /// from it (`RunnerState::removed_from_game`) when the chooser is the
    /// Corp: Ansel 2.0's "remove 1 card in the heap from the game" — and
    /// the Corp's when the chooser is the Runner. A destination only.
    OpponentRemovedFromGame,
    /// Out of the game, on the chooser's own side — Bloo Moose's "remove 1
    /// card in the heap from the game". A destination only. Composition
    /// didn't work: `Effect::RemoveFromGame(ThisCard)` removes the acting
    /// card from where it is installed or was trashed to, not from wherever
    /// a selection found it.
    OwnRemovedFromGame,
    /// The operations in the play area that stay there after resolving
    /// (`CorpState::play_area`) — the zone is both players' (CR 4.1.1b), so
    /// neither "own" nor "opponent": every lockdown's "Play only if there
    /// is no active **lockdown**" (CR 3.5.1c), counted by
    /// `EffectRequirement::ZoneHasAtLeast` with a subtype filter. A source
    /// for a count only. A run's event is in the play area too and is not
    /// listed: no card counts one there.
    PlayArea,
}

impl CardZoneRef {
    /// Whether a selection prompt over this zone shows its chooser cards
    /// they had not seen: their own deck, or the other side's hand. A
    /// take-back from such a prompt is not free (`GameEvent::
    /// may_teach_the_actor`). Installed cards and discard piles are
    /// masked for the chooser like everything else, so choosing among
    /// them shows nothing new. Exhaustive, for the same reason.
    pub fn shows_the_chooser_hidden_cards(&self) -> bool {
        match self {
            CardZoneRef::OwnRAndD | CardZoneRef::BottomOfOwnDeck | CardZoneRef::OwnStack | CardZoneRef::TopOfOwnStack | CardZoneRef::OpponentHand | CardZoneRef::OpponentDeck => true,
            CardZoneRef::OwnHq
            | CardZoneRef::OwnArchives
            | CardZoneRef::OwnGrip
            | CardZoneRef::OwnHeap
            | CardZoneRef::OwnSetAside
            | CardZoneRef::OwnSetAsideFacedown
            | CardZoneRef::OwnInstalledFacedown
            | CardZoneRef::OtherIdentities
            | CardZoneRef::OpponentSetAside
            | CardZoneRef::OpponentInstalled
            | CardZoneRef::OpponentDiscard
            | CardZoneRef::OwnInstalled
            | CardZoneRef::HostedOnSource
            | CardZoneRef::OpponentScoreArea
            | CardZoneRef::OwnScoreArea
            | CardZoneRef::OpponentRemovedFromGame
            | CardZoneRef::OwnRemovedFromGame
            | CardZoneRef::RootOfAttackedServer
            | CardZoneRef::AttackedServer
            | CardZoneRef::PlayArea => false,
        }
    }

    /// Where a card chosen out of this zone was, if it is trashed
    /// (`GameEvent::CardTrashed::from`). A discard pile, a score area and
    /// the removed-from-game zone are sources a trash never takes from;
    /// they are `Elsewhere` rather than a guess.
    pub fn trashed_from(&self) -> crate::dsl::TrashedFrom {
        use crate::dsl::TrashedFrom;
        match self {
            CardZoneRef::OwnHq | CardZoneRef::OwnGrip | CardZoneRef::OpponentHand => TrashedFrom::Hand,
            CardZoneRef::OwnRAndD | CardZoneRef::BottomOfOwnDeck | CardZoneRef::OwnStack | CardZoneRef::TopOfOwnStack | CardZoneRef::OpponentDeck => TrashedFrom::Deck,
            CardZoneRef::OpponentInstalled | CardZoneRef::OwnInstalled => TrashedFrom::Installed,
            CardZoneRef::OwnArchives
            | CardZoneRef::OwnHeap
            | CardZoneRef::OwnSetAside
            | CardZoneRef::OwnSetAsideFacedown
            | CardZoneRef::OwnInstalledFacedown
            | CardZoneRef::OtherIdentities
            | CardZoneRef::OpponentSetAside
            | CardZoneRef::OpponentDiscard
            | CardZoneRef::HostedOnSource
            | CardZoneRef::OpponentScoreArea
            | CardZoneRef::OwnScoreArea
            | CardZoneRef::OpponentRemovedFromGame
            | CardZoneRef::OwnRemovedFromGame
            | CardZoneRef::RootOfAttackedServer
            | CardZoneRef::AttackedServer => TrashedFrom::Elsewhere,
            CardZoneRef::PlayArea => TrashedFrom::PlayArea,
        }
    }
}

/// Which cards within a `CardZoneRef` are eligible to be selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardFilter {
    Any,
    NonAgenda,
    CardType(CardType),
    CardTypeOneOf(Vec<CardType>),
    /// A Program with a printed `strength` — the closest signal this schema
    /// has to "is an icebreaker" without a dedicated keyword field (no card
    /// before Mutual Favor has needed to filter on it). Documented heuristic,
    /// not a real keyword lookup: every icebreaker in this registry has a
    /// `strength`, and no non-icebreaker Program does, so this holds for the
    /// full card pool as of this variant's introduction — revisit if that
    /// ever stops being true.
    Icebreaker,
    /// An installed card the Corp did *not* install this turn — Seamless
    /// Launch's targeting restriction. Unlike every other variant this is a
    /// property of the *installed instance*
    /// (`rules::state::InstalledCard::installed_this_turn`), not of the card
    /// definition, so `card_matches_filter` (which only sees a
    /// `CardDefinition`) can't decide it and passes everything; the real
    /// check lives in `rules::pending_choice::eligible_cards`, the single
    /// funnel every caller — legality, mask candidates,
    /// `ToggleCardSelection` validation, and `PromptChooseCards`'s
    /// availability check — already goes through.
    NotInstalledThisTurn,
    /// A card the Runner could install from the grip right now — a
    /// non-Trojan Program, Hardware or Resource that is affordable, and a
    /// program that fits once programs are trashed to make room (CR
    /// 3.9.3b; a second console is no bar, CR 3.8.5b): the target set of
    /// Pantograph's "you may install 1 card from your grip". The type half
    /// lives in `card_matches_filter`; affordability and the budgets are
    /// state-dependent, answered by `rules::engine::
    /// can_install_runner_card_from_grip` through `eligible_positions`, so
    /// a selection never offers an install its resolution would refuse. A
    /// Trojan is excluded — its host is a choice no parked effect models
    /// yet; the Runner installs one with a click instead.
    InstallableRunnerCard,
    /// An installed piece of ice that is **not already rezzed** — the only
    /// legal target of "rez 1 installed piece of ice" (*Send a Message*).
    ///
    /// Like [`CardFilter::NotInstalledThisTurn`] this is a property of the
    /// installed instance rather than the definition, so the real check
    /// lives in `rules::pending_choice::eligible_cards`.
    ///
    /// Authoring this as a plain `CardType(Ice(_))` deadlocked the game:
    /// every installed ice counted as eligible, so `PromptChooseCards`'
    /// park-time "are there at least `min` targets?" guard passed even when
    /// all of them were already rezzed — and then every possible selection
    /// made `ConfirmCardSelection` fail with `AlreadyRezzed`, leaving a
    /// parked decision that nothing could resolve while it blocked every
    /// other action. Excluding rezzed ice here restores that guard: with no
    /// unrezzed ice the effect correctly no-ops instead of parking.
    UnrezzedIce,
    /// A card in the Runner's heap that was discarded during the Runner's
    /// most recent discard phase (`rules::state::RunnerState::
    /// discarded_this_discard_phase`) — Magdalene Keino-Chemutai's "from
    /// among those cards". Instance-level, decided in `eligible_cards`;
    /// matches by card, so with two copies of one card in the heap either
    /// copy is offered — they are the same card in the same zone.
    DiscardedThisDiscardPhase,
    /// Every listed filter must match — the composition primitive that
    /// keeps the variants above orthogonal: Magdalene needs "a Program or
    /// Hardware" *and* "discarded this phase" *and* "installable right
    /// now", and a fused variant for each such conjunction would grow this
    /// enum per card rather than per property. Both halves recurse.
    All(Vec<CardFilter>),
    /// Any card but the parking card itself — "Knickknack" O'Brian's
    /// "1 of your *other* installed cards". Instance-level: it compares
    /// installs, so a second copy of the same card is still eligible.
    NotSourceCard,
    /// Any card but the one the parking card is hosted on — Hush's "Host
    /// this program on **another** installed piece of ice". Instance-level,
    /// compared by install as `NotSourceCard` is. Composition didn't work:
    /// the parking card is a Trojan in the rig, so `NotSourceCard` admits
    /// every piece of ice, its host among them.
    NotThisCardsHost,
    /// Only the card the parking card is hosted on — Dedicated Processor's
    /// "Host **icebreaker** gains '2[credit]: +4 strength'": the pump is
    /// the hardware's ability and boosts the selected card, so the one card
    /// it may select is its host (a rig card, `hosted_on_rig_card`, or a
    /// piece of ice, `hosted_on_ice`). Instance-level, as `NotThisCardsHost`
    /// is. Composition didn't work: `BoostStrength` boosts the acting card,
    /// which in an ability is the hardware, and no filter named the host.
    ThisCardsHost,
    /// Only a card installed on the parking card — London Library's "Trash
    /// all programs hosted on London Library" and "Add a program on London
    /// Library to your grip" (`InstalledRunnerCard::hosted_on_rig_card`).
    /// `ThisCardsHost` turned round; instance-level, compared by install.
    /// Composition didn't work: `HostedOnSource` is the cards a host holds
    /// uninstalled, and these are in the rig.
    HostedOnThisCard,
    /// Another copy of the parking card — Restore's "Remove all other
    /// copies of **that card** in Archives from the game", asked as the
    /// card Restore installed (`PromptInstallCorpCard::if_installed`), which
    /// has left Archives by then, so every copy still there is another;
    /// and Research Grant's "another copy of Research Grant that is
    /// installed", asked as the agenda being scored. Instance-level: the
    /// parking card is an install or a scored agenda, read off the state.
    /// Composition didn't work: `AmongCards` is written in the card file,
    /// and `ChosenThisWay` names installs, which a card in Archives is not.
    CopyOfThisCard,
    /// No piece of ice whose subroutine is already resolving through this
    /// one's — Ravana 1.0's "Resolve 1 subroutine on **another** rezzed
    /// bioroid ice", asked by a Ravana whose own subroutine another
    /// Ravana resolved (`EncounterTally::relayed`). Null Signal Games'
    /// ruling on Ravana: with only Ravanas rezzed, "a mandatory loop is
    /// created … the Corp chooses a number X and the loop continues X
    /// iterations and then stops", and "the subroutine does not have any
    /// effect"; going back to an ice already in the chain is that loop, and
    /// whatever it could reach the first pass already offered, so the chain
    /// is left out and a chain with nowhere new to go ends. Instance-level,
    /// compared by install. Composition didn't work: `NotSourceCard` knows
    /// only the parking card, and two Ravanas each chose the other forever.
    NotRelaying,
    /// An installed card the side named may trash — not one that says it
    /// cannot be (`ContinuousKind::CannotBeTrashedBy`, Jarogniew Mercs).
    /// Never written in a card file: every selection that trashes an
    /// installed card is narrowed by it as it parks (`Effect::
    /// PromptChooseCards`), so no card has to remember to say it.
    /// Instance-level, by install. Composition didn't work: the standing
    /// effect is the trashed card's, and no filter read one.
    TrashableBy(crate::rules::Side),
    /// `InstallableRunnerCard` priced `u32` cheaper — the offer half of
    /// `Effect::InstallRunnerCardFromGripWithDiscount`.
    InstallableRunnerCardWithDiscount(crate::dsl::Discount),
    /// A card the inner filter does not admit — Beta Build's "1
    /// **non-virus** program". Definition-level only: an instance-level
    /// word under it (`Rezzed`, `NotSourceCard`) is read by its
    /// definition half, which admits every card, so its negation admits
    /// none. Composition didn't work: every other word is a positive one.
    Not(Box<CardFilter>),
    /// An installed Corp card that is rezzed — Charm Offensive's "1 rezzed
    /// copy". Instance-level, like `UnrezzedIce`, and any card type.
    Rezzed,
    /// The twin of `Rezzed`: an installed Corp card that is *not* faceup —
    /// PT Untaian: Life's Building Blocks' "an unrezzed card you can
    /// advance", which is an agenda or an asset as often as it is ice.
    /// `UnrezzedIce` cannot serve: its definition half insists on ice.
    Unrezzed,
    /// A copy of a card the Runner accessed during the run that just
    /// ended (`CompletedRun::accessed_cards`) — Charm Offensive. By card,
    /// as the printed text says "a copy of a card you accessed".
    AccessedDuringLastRun,
    /// A card printed with this subtype — MuslihaT's "a run event".
    /// Definition-level.
    HasSubtype(crate::dsl::CardSubtype),
    /// A card printed with at least this many subtypes — Sadyojata's "a
    /// piece of ice with 3 or more subtypes". Definition-level: the printed
    /// line, ice type included (a Code Gate - Tracer has two), so a subtype
    /// an ice gains for a run (Panchatantra) is not counted. Composition
    /// didn't work: `HasSubtype` asks for one subtype by name.
    SubtypesAtLeast(u32),
    /// A card printed with exactly this many subroutines — Vamadeva's "a
    /// piece of ice with exactly 1 subroutine". Definition-level, the
    /// printed ones (`CardDefinition::subroutines`). Composition didn't
    /// work: no filter read a card's subroutines.
    SubroutinesExactly(u32),
    /// A unique (◆) card — Hackerspace's "unique (◆) companion resources".
    /// Definition-level. Not a subtype: uniqueness is printed before the
    /// name (CR 2.2.1), and `CardDefinition::unique` is where it is read.
    Unique,
    /// A card with a [trash] ability — The Back's "choose up to 2 cards in
    /// your heap with [trash] abilities": a paid ability whose cost prints
    /// [trash] (`Cost::prints_trash`). Definition-level. Composition didn't
    /// work: no filter read a card's abilities.
    HasTrashAbility,
    /// A card with a printed trash cost — Aeneas Informant's "access a
    /// card **with a trash cost**". Definition-level, read off
    /// `CardDefinition::trash_cost`. Composition didn't work: no filter
    /// read the trash cost, and `TrashableBy` asks whether the Runner may
    /// trash it now, which a card without a cost can be by a card's text.
    HasTrashCost,
    /// The positive twin of `NotInstalledThisTurn`: Word on the Street's
    /// "an agenda the Corp installed this turn". A separate word rather
    /// than `Not(NotInstalledThisTurn)`, because `Not` is definition-level
    /// and would admit nothing. Instance-level, read off an install
    /// (`InstalledCard::installed_this_turn`) and off a scored agenda
    /// (`ScoredAgenda::installed_on_scoring_turn`, when it was scored this
    /// turn).
    InstalledThisTurn,
    /// A Corp install rezzed this turn — Rubicon Switch's "1 piece of ice
    /// … that was rezzed this turn". Instance-level, read off the copy's
    /// own turn (`InstalledCard::this_turn`, which counts its rez), so a
    /// second copy rezzed earlier is not it. Composition didn't work:
    /// `RezzedDuringLastRun` is one run's, and a rez outside a run counts.
    RezzedThisTurn,
    /// An agenda in a score area that was not advanced on the turn it was
    /// scored — Issuaq Adaptics: Sustaining Diversity's "an agenda that you
    /// did not install or **advance** this turn", in an `OnAgendaScored`
    /// `when` beside `NotInstalledThisTurn`. Instance-level, read off
    /// `ScoredAgenda::advanced_on_scoring_turn`; a counter a card placed is
    /// not advancing (CR 1.18.2). The negative word, as `NotInstalledThisTurn`
    /// is, because `Not` is definition-level and would admit nothing.
    NotAdvancedThisTurn,
    /// An agenda in a score area that was **scored** this turn — Myōshu's
    /// "you scored an agenda this turn". Instance-level, read off
    /// `ScoredAgenda::scored_on_turn`. A card added "as an agenda" was not
    /// scored (CR 1.17.3f) and never matches.
    ScoredThisTurn,
    /// At least one listed filter must match — `All`'s disjunctive twin,
    /// for MuslihaT's "an icebreaker *or* a run event". Both halves
    /// recurse, as `All` does.
    AnyOf(Vec<CardFilter>),
    /// An installed Corp card that can be advanced — anything whose
    /// definition carries an `advancement_requirement` (agendas, and the
    /// "you can advance this" assets and ice such as Clearinghouse and
    /// Syailendra). Definition-level; the same test `legal_actions` uses
    /// to offer `AdvanceCard`. Syailendra's and Key Performance
    /// Indicators' "place 1 advancement counter on an installed card you
    /// can advance". The copy's half is `engine::may_be_advanced`'s, so an
    /// unrezzed Tyrant, which can be advanced only while rezzed, is not one.
    Advanceable,
    /// An installed Corp card in the root of, or a piece of ice
    /// protecting, the server the current run is against — LEO
    /// Construction: Labor Solutions' "in the root of or protecting the
    /// attacked server". Instance-level; matches nothing outside a run,
    /// which is what makes the identity's ability legal only during one
    /// without a separate "during a run" requirement.
    InAttackedServer,
    /// An installed Corp card in the root of, or protecting, the server the
    /// most recently ended run was against (`GameState::
    /// last_completed_run`) — Kompromat's "protecting the attacked server",
    /// asked when that run ends. `InAttackedServer` matches nothing then,
    /// since the run has left `active_run`; widening it to fall back on
    /// the last run would have made LEO Construction's ability legal after
    /// a run instead of during one. Instance-level.
    InLastRunServer,
    /// An installed Corp card in the root of the server the acting card is
    /// in — Hype Machine's "a card you can advance in the root of this
    /// server". **A placeholder, written over when the effect resolves**
    /// (`CardFilter::with_this_server`, the convention `Amount::
    /// ChosenNumber` follows): Hype Machine's "[trash]:" is paid before the
    /// selection is offered, so by the time a card is chosen nothing on the
    /// table says which server "this" was — the payer's `last_known` does,
    /// and it does not survive the park. Matches nothing unresolved.
    InRootOfThisServer,
    /// A piece of ice, of any type — Underdome Irregulars' "if a piece of
    /// ice was rezzed this turn". `CardType(Ice(_))` names a type, and the
    /// turn log counts ice as ice.
    Ice,
    /// An installed Corp card in the root of `server`: what
    /// `InRootOfThisServer` becomes when it resolves. Instance-level.
    InRootOf(ServerId),
    /// An operation in the zone being selected from — HQ, or Archives for
    /// Plutus — that the Corp could play right now: its cost affordable
    /// and its `play_requirement` met. The offer half of
    /// `Effect::PlayOperation` (Humanoid Resources' "You may play 1
    /// operation from HQ"). Instance-level: affordability is state. The
    /// effect re-checks, so the offer and the resolution cannot disagree.
    PlayableOperation,
    /// An operation or event in the zone being selected from that its
    /// owner could play right now paying its play cost and **ignoring any
    /// additional costs** — the offer half of `Effect::PlayOperation`'s
    /// `ignoring_additional_costs`: Accelerated Diagnostics' operations
    /// among the top 3 of R&D and Planned Assault's run event in the
    /// stack. Composition didn't work: `PlayableOperation` asks the
    /// additional cost too, so a double the Corp has no click left for
    /// would never be offered, and nothing asked it of an event.
    /// Instance-level, as that one is.
    PlayableIgnoringAdditionalCosts,
    /// One of the top `u32` cards of the zone being selected from —
    /// Poétrï Luxury Brands' "look at the top 3 cards of R&D". Instance-
    /// level, and a *filter* rather than a `CardZoneRef` of its own so it
    /// composes with the type filter through `All` and leaves the
    /// selection's zone (and therefore the position a `then` install
    /// re-finds the card by) the whole of R&D. The top of R&D and of the
    /// stack is the *end* of the `Vec` — see `zone_card_ids`.
    TopOfZone(u32),
    /// One of the bottom `u32` cards of the zone being selected from —
    /// Shannon Claire's "Draw 1 card from the bottom of R&D". The bottom
    /// is the *start* of the `Vec`. Composition didn't work: `TopOfZone`
    /// counts from the other end, and `NthFromTop` would need the deck's
    /// size as an amount read as the selection is offered.
    BottomOfZone(u32),
    /// A card with a copy in the Runner's heap — Student Loans' "if there is
    /// a copy of that event in the heap", about an event being played
    /// (`Scope::OpponentPlaying`, which reads it off the heap). Composition
    /// didn't work: no filter read the heap, and `CopyOfThisCard` is a copy
    /// of the card printing the effect.
    CopyInHeap,
    /// The one card `n` from the top of the zone, the top being 1 — Top
    /// Hat's "choose 1 of the top 5 cards in R&D and access it", the
    /// Runner choosing a position (`Effect::ChooseNumber`, written in as
    /// `Fixed` by `with_chosen_number`) among cards neither player sees.
    /// Instance-level; an amount not yet a number matches nothing.
    /// Composition didn't work: `TopOfZone` is the top `n` together, and a
    /// `Not` of a place is read off the definition, where every place
    /// matches.
    NthFromTop(Box<crate::dsl::Amount>),
    /// `TopOfZone` of a number read as the selection is offered — The
    /// Class Act's "look at the top X cards of your stack", X the cards
    /// about to be drawn plus 1 (`Amount::AboutToResolve`). Written over
    /// as `TopOfZone` by `with_resolution`; unresolved, it matches nothing.
    /// A word beside `TopOfZone` rather than an `Amount` in it, so the
    /// cards that print a number keep writing the number.
    TopOf(Box<crate::dsl::Amount>),
    /// A card revealed in its owner's hand by the ability still resolving
    /// (`GameState::revealed`) — Burner's "Add 2 of the revealed cards to
    /// the top and/or bottom of R&D". Instance-level, like `TopOfZone`:
    /// which card was revealed is state, not the card's definition. By card
    /// rather than position, so two copies in HQ with one revealed are both
    /// eligible; they are the same card, and a selection of one takes it
    /// off the list. Composition didn't work: nothing else names cards an
    /// earlier step picked at random.
    Revealed,
    /// Ice of exactly this subtype — Mycoweb's "a rezzed **sentry**" and
    /// "another rezzed **code gate**". `CardType(Ice(_))`'s payload is
    /// explicitly a don't-care (see this module's `card_matches_filter`
    /// doc), so it cannot express this, and `HasSubtype` reads
    /// `CardSubtype`, which is the printed-tag vocabulary rather than the
    /// ice-type one.
    IceOfType(crate::dsl::IceType),
    /// An agenda whose printed point value is no greater than the Runner's
    /// tag count — IP Enforcement pays for one by removing that many tags,
    /// so an agenda it could not pay for is never offered. Instance-level
    /// only because it reads the tag count, which is state; the point
    /// value is the card's own.
    AgendaPointsAtMostRunnerTags,
    /// An agenda worth no more than this many printed points — Kingmaking's
    /// "1 agenda worth 1 or less agenda points from HQ". Composition didn't
    /// work: `AgendaPointsAtMostRunnerTags` compares against a tag count,
    /// and no other filter reads a number.
    AgendaPointsAtMost(u32),
    /// A facedown card in Archives — Cohort Guidance Program's "Turn 1
    /// facedown card in Archives faceup". Instance-level: which way up a
    /// card lies is state, and matches nothing outside Archives.
    Facedown,
    /// A printed cost of at most this many credits — Kimberlite Field's
    /// "an installed Runner card with a printed install cost equal to or
    /// less than the printed rez cost of the Corp card you trashed". The
    /// amount is read as the selection is offered (`with_resolution`); a
    /// card with no printed cost (an agenda, an identity) has none to
    /// compare and is not admitted. Composition didn't work: no filter read
    /// a number from the resolution.
    PrintedCostAtMost(Box<crate::dsl::Amount>),
    /// A printed cost of exactly this many credits — Ob Superheavy
    /// Logistics' "a printed rez cost exactly 1[credit] less than the
    /// trashed card's printed rez cost". `PrintedCostAtMost`'s word for
    /// "equal to": read as the selection is offered, and admitting no
    /// agenda or identity. Composition didn't work: "at most N" and "not at
    /// most N − 1" is the same card twice over an amount read twice, and
    /// `Not` of a resolved filter is not resolved.
    PrintedCostExactly(Box<crate::dsl::Amount>),
    /// An agenda worth exactly this many printed points — Lady Liberty's
    /// "an agenda from HQ … worth agenda points equal to the exact number
    /// of hosted power counters". Read as the selection is offered, as
    /// `PrintedCostExactly` is. Composition didn't work:
    /// `AgendaPointsAtMost` holds a fixed number, and "at most N and not at
    /// most N − 1" reads the amount twice under a `Not`, which is not
    /// resolved.
    AgendaPointsExactly(Box<crate::dsl::Amount>),
    /// An agenda whose printed advancement requirement is exactly this —
    /// RNG Key's "if it has … an advancement requirement equal to the named
    /// number". `AgendaPointsExactly`'s twin for the requirement, read the
    /// same way. Composition didn't work: no filter read an agenda's
    /// advancement requirement.
    AdvancementRequirementExactly(Box<crate::dsl::Amount>),
    /// A printed trash cost of exactly this many credits — Political
    /// Operative's "trash 1 rezzed card with trash cost equal to X", X its
    /// `Cost::CreditsX`. Read as the selection is offered, as
    /// `PrintedCostExactly` is; a card with no trash cost is not admitted.
    /// The printed number, because every change to a trash cost in the
    /// pool is to the cost of a card being accessed (`ContinuousKind::
    /// TrashCost`), which this card is not. Composition didn't work:
    /// `HasTrashCost` reads no number.
    TrashCostExactly(Box<crate::dsl::Amount>),
    /// The same card type as the card a nested cost just trashed — World
    /// Tree's "trash 1 of your other installed cards to search your stack
    /// for 1 card **of the same type**", written in as the selection is
    /// offered (`with_resolution`). Matches nothing anywhere else.
    SameTypeAsPaidCard,
    /// No other card chosen in the same selection has this one's name —
    /// Asmund Pudlat's "search your stack for up to 2 virus or weapon cards
    /// **with different names**". Instance-level: it reads the selection
    /// in progress, so a second copy of a chosen card cannot be toggled on
    /// and a chosen card can still be toggled off. Composition didn't
    /// work: `max` counts cards, and nothing else read what was already
    /// chosen.
    DifferentNames,
    /// A faceup card in Archives — Armed Asset Protection's "if any of
    /// those cards are agendas", of the faceup cards it counts. `Facedown`'s
    /// other half. Composition didn't work: `Not` is read off a definition,
    /// where which way up a card lies is not, so `Not(Facedown)` matches
    /// nothing. Among installed Corp cards, one that is rezzed or was
    /// installed faceup — Dedication Ceremony's and Mumbad Construction
    /// Co.'s "a faceup card", which is an agenda as often as anything:
    /// `Rezzed` leaves agendas out, since an agenda is never rezzed.
    Faceup,
    /// A card hosting at least one counter of this kind — Business As
    /// Usual's "Remove all virus counters from 1 installed card", so a card
    /// with nothing to remove is never offered. The kind is the card's own
    /// (`CardDefinition::counter_kind`, as the purge reads it); how many it
    /// holds is instance-level. Virus counters are the exception: any
    /// Runner card may hold them beside its own kind (Contaminate's,
    /// `InstalledRunnerCard::virus_counters`), so for them the copy alone
    /// decides (`is_about_the_copy_alone`) and `Not` says "with no hosted
    /// virus counters".
    HostsCounters(crate::dsl::CounterKind),
    /// A card with "N[recurring-credit]" holding fewer than N — Mirror's
    /// "you may replace 1 spent recurring credit", which may go only where
    /// one was spent (CR 1.10.5). The definition says the card prints
    /// recurring credits; how many it holds is instance-level, counted as
    /// the refill counts it (`payment::spent_recurring`). Composition
    /// didn't work: `HostsCounters` asks whether a card holds any, and a
    /// card's recurring credits may sit beside counters of another kind
    /// (Pheromones').
    SpentRecurringCredit,
    /// A card in the server the acting card is in, its root or its ice —
    /// Isaac Liberdade's "a piece of ice protecting that server" (with
    /// `Ice`). A placeholder, as `InRootOfThisServer` is: written over as
    /// `InServer` where the acting card's server is known (`with_this_server`)
    /// and matching nothing where it is not.
    InThisServer,
    /// An installed Corp card in, or protecting, the server the acting card
    /// chose this turn (`Effect::ChooseServer`, `Lingering::ChosenServer`)
    /// — Tsakhia "Bankhar" Gantulga's "a piece of ice protecting **the
    /// chosen server**". A placeholder, as `InThisServer` is: written over
    /// as `InServer` where the card's choice is known
    /// (`with_chosen_server`) and matching nothing where it is not, so a
    /// turn with no choice made hears nothing. Composition didn't work: the
    /// server is chosen as the game goes, and `InServer` is written in the
    /// card file.
    InChosenServer,
    /// A card of the type the acting card chose (`Effect::Remember`,
    /// `Lingering::ChosenCardType`) — Engram Flush's "you may trash 1
    /// revealed card of **the chosen type**". A placeholder, as
    /// `InChosenServer` is: written over as `CardType` where the choice is
    /// known (`with_chosen_card_type`) and matching nothing where it is
    /// not, so a subroutine of an encounter in which no type was chosen
    /// trashes nothing. Composition didn't work: the type is chosen as the
    /// game goes, and `CardType` is written in the card file.
    OfChosenCardType,
    /// A card of the faction printed on the identity of the player whose
    /// card it is — District 99's "add a card that **matches the faction of
    /// your identity** from your heap to your grip", and Enforcing
    /// Loyalty's "trash an installed card that **does not match the faction
    /// of the Runner's identity**" (`Not`), the Corp choosing among the
    /// Runner's cards. The identity is the zone's owner's, never simply the
    /// chooser's, so one word says both. Instance-level: which faction is
    /// the identity in play, so `card_matches_filter` lets every definition
    /// through and `pending_choice`'s instance check compares the two. A
    /// neutral card matches only a neutral identity. Composition didn't
    /// work: no filter read a faction, and a faction written in the card
    /// file would be wrong in every deck that splashes the card.
    MatchesOwnersIdentityFaction,
    /// A card in this server, its root or its ice. `InRootOf` is the root
    /// alone; a card "protecting" a server is `All([Ice, InServer(..)])`.
    InServer(ServerId),
    /// An installed card hosting at least one advancement counter — Isaac
    /// Liberdade's "each **advanced** piece of ice" and Hearts and Minds'
    /// "move 1 advancement counter **from** an installed card", which is
    /// only offered where there is one. Instance-level.
    Advanced,
    /// An installed card hosting none — Isaac Liberdade's "a piece of ice
    /// … that has no advancement counters". Its own word because `Not` is
    /// decided off the definition alone. Instance-level.
    Unadvanced,
    /// One of these cards — what `TrashedThisWay` is written over as, by
    /// card, so every copy of a card trashed is one of them (they are the
    /// same card in the same zone, as `Revealed`'s are). Definition-level:
    /// a card's id is its definition's.
    AmongCards(Vec<crate::dsl::CardId>),
    /// **The chosen cards**: the installs the selection whose `then` this
    /// is chose — Jua's "Choose 2 installed Runner cards, if able. The
    /// Runner must add 1 of **the chosen cards** to the top of the stack",
    /// the Corp choosing and the Runner then choosing among them. A
    /// placeholder, written over as `AmongInstalls` when the selection
    /// resolves (`Effect::with_chosen_installs`), as `TrashedThisWay` is;
    /// unresolved it matches nothing. Composition didn't work: a
    /// selection's `then` acts as its first card alone, and the second
    /// chooser needs both.
    ChosenThisWay,
    /// One of these installs — what `ChosenThisWay` is written over as.
    /// Instance-level: a handle, not a card, so another copy of a chosen
    /// card is not one of them.
    AmongInstalls(Vec<crate::rules::InstallId>),
    /// A piece of ice in the same position as the one `MovedTo` names,
    /// positions counted from the innermost (CR 6.2.3) — Slipstream's
    /// "1 piece of ice protecting a central server in the same position
    /// as the passed ice". `MovedTo::PassedIce` is written over as
    /// `MovedTo::Ice` when the choice the selection waits behind parks
    /// (`Effect::with_passed_ice`); left unwritten, it admits nothing.
    SamePositionAs(crate::dsl::MovedTo),
    /// **Those cards**: the ones the `Effect::Mill` whose `then` this is
    /// trashed — The Price's "You may install 1 of those cards". A
    /// placeholder, written over when the mill resolves
    /// (`CardFilter::with_those_trashed`, the convention
    /// `InRootOfThisServer` follows); unresolved it matches nothing.
    /// Composition didn't work: `TopOfZone` on the heap found the event
    /// being played on top of it, since the event lands there in the
    /// middle of its own resolution, and a discard pile has no order to
    /// read (CR 4.4.2).
    TrashedThisWay,
    /// **That card**: the one the moment a trigger heard is about —
    /// Divested Trust's "add **the stolen agenda** to HQ", chosen out of the
    /// Runner's score area by a selection only it passes. A placeholder,
    /// written over as the card when the trigger fires
    /// (`CardFilter::with_that_card`, from `listeners::card_about`), the
    /// convention `TrashedThisWay` follows; unresolved it matches nothing.
    /// Composition didn't work: `acts_on_subject` moves every effect of the
    /// trigger onto the stolen agenda, and Divested Trust's forfeit is its
    /// own; and the paid choice parks, so the triggering event is gone by
    /// the time the agenda is chosen.
    ThatCard,
    /// A card with **the chosen name** — Complete Image's "If you trash a
    /// card with the chosen name this way" and Whistleblower's "an agenda
    /// with the chosen name". A placeholder inside `Effect::ChooseCardName::
    /// then`, written over as `AmongCards([name])` when the name is chosen
    /// (`with_chosen_name`), as `ThatCard` is; unresolved it matches
    /// nothing.
    ChosenName,
    /// The piece of ice the Runner encountered last during the last run
    /// (`CompletedRun::last_encountered`) — Always Have a Backup Plan's
    /// "whenever you encounter the last piece of ice you encountered during
    /// the first run", asked during the second, when the first is still the
    /// last run. Instance-level: a handle, not a card, so a second copy of
    /// the same ice is not it.
    LastEncounteredLastRun,
    /// A piece of ice the Runner passed during the last run
    /// (`CompletedRun::passed`) — En Passant's "Trash 1 unrezzed piece of
    /// ice you passed during your last run". Instance-level, a handle, as
    /// `LastEncounteredLastRun` is: a second copy of the same ice that the
    /// run never reached is not it, and neither is ice installed since.
    PassedDuringLastRun,
    /// A Corp install rezzed during the Runner's last run — Leave No
    /// Trace's "derez all ice that was rezzed during this run", read as
    /// the run ends (`CompletedRun::rezzed`). Instance-level, a handle, as
    /// `PassedDuringLastRun` is.
    RezzedDuringLastRun,
    /// A Runner install whose ability was used during the last run —
    /// Cold Read's "trash 1 installed program you used during that run",
    /// asked as the run ends (`CompletedRun::used`). Instance-level, a
    /// handle, as `RezzedDuringLastRun` is: a second copy of the program
    /// the run never used is not it.
    UsedDuringLastRun,
    /// A piece of ice the Runner fully broke during the run in progress
    /// (`RunState::fully_broken_ice`, CR 6.5.7a) — The Gauntlet's "each
    /// piece of ice protecting HQ that you fully broke during that run",
    /// asked as HQ is breached. Instance-level, a handle, as
    /// `RezzedDuringLastRun` is: a second copy of the same ice the run did
    /// not break is not it. Composition didn't work: no word read a full
    /// break but the moment it happened (`Trigger::OnIceFullyBroken`), and
    /// the breach asks after the run has passed every piece.
    FullyBrokenDuringThisRun,
    /// The piece of ice the Runner is encountering right now — Ice
    /// Carver's "While you are encountering a piece of ice, **it** gets −1
    /// strength", as the filter of a `Scope::Ice`. Instance-level: a
    /// handle, read off the run (`pending_choice::copy_matches`), so a
    /// second copy of the same ice elsewhere is not it. Composition didn't
    /// work: a `while` of `DuringEncounter` is asked of the card that
    /// prints it, so every piece of ice would have had −1 during an
    /// encounter.
    BeingEncountered,
    /// The piece of ice the Runner is approaching right now — AgInfusion:
    /// New Miracles for a New World's "Trash **the unrezzed piece of ice
    /// the Runner is approaching**:", the filter of a `Cost::Trash`, so the
    /// cost is affordable exactly while there is one. `BeingEncountered`'s
    /// twin for the approach (CR 6.9.2), read off the run the same way.
    /// Composition didn't work: `InAttackedServer` is every piece of ice
    /// on the server, and the run's position is nowhere else.
    BeingApproached,
    /// A card that may be swapped into the place of the install the
    /// selection acts as (`run::swappable_into`, CR 8.8.2) — Project
    /// Yagi-Uda's "swap 1 card from HQ with 1 card in the root of or
    /// protecting the attacked server", whose card from HQ is chosen
    /// inside the `then` of the choice of the installed one: ice for ice,
    /// and for a root only what that root may hold. Instance-level: the
    /// answer is the table's. Composition didn't work: a type filter says
    /// "ice", never "what fits where that card is".
    SwappableIntoThis,
}

/// Whether `card` is eligible under `filter`. `CardType(CardType::Ice(_))`
/// matches ICE of any subtype — the specific `IceType` payload on the
/// filter's own `CardType::Ice` value is a don't-care placeholder, not a
/// subtype restriction (author it as e.g. `CardType::Ice(IceType::Barrier)`;
/// any subtype works identically).
impl CardFilter {
    /// This filter with `InRootOfThisServer` written over as the server the
    /// acting card is in, where one is known; a placeholder left unresolved
    /// matches nothing.
    /// The words read off the resolution, written in as the selection is
    /// offered, as `with_this_server` writes in the server: an amount
    /// becomes the number it is now (`PrintedCostAtMost`), and "the same
    /// type" becomes the type of the card a nested cost trashed
    /// (`SameTypeAsPaidCard`). Unresolved, each matches nothing.
    pub fn with_resolution(self, amount: &dyn Fn(&crate::dsl::Amount) -> u32, paid: Option<&CardType>) -> CardFilter {
        match self {
            CardFilter::PrintedCostAtMost(at_most) => CardFilter::PrintedCostAtMost(Box::new(crate::dsl::Amount::Fixed(amount(&at_most)))),
            CardFilter::PrintedCostExactly(exactly) => CardFilter::PrintedCostExactly(Box::new(crate::dsl::Amount::Fixed(amount(&exactly)))),
            CardFilter::AgendaPointsExactly(exactly) => CardFilter::AgendaPointsExactly(Box::new(crate::dsl::Amount::Fixed(amount(&exactly)))),
            CardFilter::AdvancementRequirementExactly(exactly) => CardFilter::AdvancementRequirementExactly(Box::new(crate::dsl::Amount::Fixed(amount(&exactly)))),
            CardFilter::TrashCostExactly(exactly) => CardFilter::TrashCostExactly(Box::new(crate::dsl::Amount::Fixed(amount(&exactly)))),
            CardFilter::TopOf(count) => CardFilter::TopOfZone(amount(&count)),
            CardFilter::SameTypeAsPaidCard => paid.map_or(CardFilter::SameTypeAsPaidCard, |card_type| CardFilter::CardType(card_type.clone())),
            // A discount read off the resolution (Rejig's "paying X[credit]
            // less", X the printed cost of the card its cost returned) is
            // the number it is now: the selection is answered on a later
            // action, which has no payment to read it from.
            CardFilter::InstallableRunnerCardWithDiscount(crate::dsl::Discount::Amount(of)) => {
                CardFilter::InstallableRunnerCardWithDiscount(crate::dsl::Discount::Credits(amount(&of)))
            }
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_resolution(amount, paid)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_resolution(amount, paid)).collect()),
            other => other,
        }
    }

    /// This filter with `InChosenServer` written over as the server the
    /// card chose (`Effect::ChooseServer`), where it chose one; left
    /// unresolved, it matches nothing.
    pub fn with_chosen_server(self, server: Option<ServerId>) -> CardFilter {
        match self {
            CardFilter::InChosenServer => server.map_or(CardFilter::InChosenServer, CardFilter::InServer),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_chosen_server(server)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_chosen_server(server)).collect()),
            other => other,
        }
    }

    /// This filter with `ChosenName` written over as the card named
    /// (`Effect::ChooseCardName`).
    pub fn with_chosen_name(self, card: &crate::dsl::CardId) -> CardFilter {
        match self {
            CardFilter::ChosenName => CardFilter::AmongCards(vec![card.clone()]),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_chosen_name(card)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_chosen_name(card)).collect()),
            CardFilter::Not(filter) => CardFilter::Not(Box::new(filter.with_chosen_name(card))),
            other => other,
        }
    }

    /// This filter with a chosen number written into the amounts it reads
    /// (`Amount::with_chosen_number`): Khusyuk's "your installed cards with
    /// that printed install cost".
    pub fn with_chosen_number(self, number: u32) -> CardFilter {
        self.filled(&|amount| matches!(amount, crate::dsl::Amount::ChosenNumber).then_some(number))
    }

    /// See `Amount::filled`.
    pub fn filled(self, fill: &dyn Fn(&crate::dsl::Amount) -> Option<u32>) -> CardFilter {
        let amount = |amount: Box<crate::dsl::Amount>| Box::new(amount.filled(fill));
        match self {
            CardFilter::PrintedCostAtMost(at_most) => CardFilter::PrintedCostAtMost(amount(at_most)),
            CardFilter::PrintedCostExactly(exactly) => CardFilter::PrintedCostExactly(amount(exactly)),
            CardFilter::AgendaPointsExactly(exactly) => CardFilter::AgendaPointsExactly(amount(exactly)),
            CardFilter::AdvancementRequirementExactly(exactly) => CardFilter::AdvancementRequirementExactly(amount(exactly)),
            CardFilter::TrashCostExactly(exactly) => CardFilter::TrashCostExactly(amount(exactly)),
            CardFilter::TopOf(count) => CardFilter::TopOf(amount(count)),
            CardFilter::NthFromTop(n) => CardFilter::NthFromTop(amount(n)),
            CardFilter::InstallableRunnerCardWithDiscount(crate::dsl::Discount::Amount(of)) => {
                CardFilter::InstallableRunnerCardWithDiscount(crate::dsl::Discount::Amount(amount(of)))
            }
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.filled(fill)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.filled(fill)).collect()),
            CardFilter::Not(filter) => CardFilter::Not(Box::new(filter.filled(fill))),
            other => other,
        }
    }

    /// Whether `with_chosen_card_type` has anything to write in, so a
    /// listener that does not ask copies no filter.
    pub fn names_chosen_card_type(&self) -> bool {
        match self {
            CardFilter::OfChosenCardType => true,
            CardFilter::All(filters) | CardFilter::AnyOf(filters) => filters.iter().any(CardFilter::names_chosen_card_type),
            _ => false,
        }
    }

    /// This filter with `OfChosenCardType` written over as the type the
    /// card chose (`Effect::Remember`), where it chose one; left
    /// unresolved, it matches nothing.
    pub fn with_chosen_card_type(self, chosen: Option<&CardType>) -> CardFilter {
        match self {
            CardFilter::OfChosenCardType => chosen.map_or(CardFilter::OfChosenCardType, |card_type| CardFilter::CardType(card_type.clone())),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_chosen_card_type(chosen)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_chosen_card_type(chosen)).collect()),
            other => other,
        }
    }

    pub fn with_this_server(self, server: Option<ServerId>) -> CardFilter {
        match self {
            CardFilter::InRootOfThisServer => server.map_or(CardFilter::InRootOfThisServer, CardFilter::InRootOf),
            CardFilter::InThisServer => server.map_or(CardFilter::InThisServer, CardFilter::InServer),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_this_server(server)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_this_server(server)).collect()),
            other => other,
        }
    }
}

impl CardFilter {
    /// Whether this word is about the copy alone — where it sits, its rez
    /// state, when it came — so the definition says nothing about it and
    /// `card_matches_filter` passes it. Under a `Not` such a word is the
    /// copy's to decide too (`rules::pending_choice`'s instance half):
    /// Stegodon MK IV's "a piece of ice **not protecting the attacked
    /// server**". Negated at the definition, it matched nothing.
    pub fn is_about_the_copy_alone(&self) -> bool {
        matches!(
            self,
            CardFilter::DiscardedThisDiscardPhase
                | CardFilter::MatchesOwnersIdentityFaction
                | CardFilter::DifferentNames
                | CardFilter::NotSourceCard
                | CardFilter::NotThisCardsHost
                | CardFilter::ThisCardsHost
                | CardFilter::HostedOnThisCard
                | CardFilter::CopyOfThisCard
                | CardFilter::NotRelaying
                | CardFilter::TrashableBy(_)
                | CardFilter::InChosenServer
                | CardFilter::Rezzed
                | CardFilter::Unrezzed
                | CardFilter::Facedown
                | CardFilter::Faceup
                | CardFilter::AccessedDuringLastRun
                | CardFilter::PassedDuringLastRun
                | CardFilter::RezzedDuringLastRun
                | CardFilter::UsedDuringLastRun
                | CardFilter::FullyBrokenDuringThisRun
                | CardFilter::RezzedThisTurn
                | CardFilter::InAttackedServer
                | CardFilter::InLastRunServer
                | CardFilter::InRootOfThisServer
                | CardFilter::InRootOf(_)
                | CardFilter::InThisServer
                | CardFilter::InServer(_)
                | CardFilter::Advanced
                | CardFilter::Unadvanced
                | CardFilter::TopOfZone(_)
                | CardFilter::BottomOfZone(_)
                | CardFilter::CopyInHeap
                | CardFilter::TopOf(_)
                | CardFilter::NthFromTop(_)
                | CardFilter::HostsCounters(crate::dsl::CounterKind::Virus)
                | CardFilter::Revealed
                | CardFilter::NotInstalledThisTurn
                | CardFilter::NotAdvancedThisTurn
                | CardFilter::InstalledThisTurn
                | CardFilter::ScoredThisTurn
                | CardFilter::AmongInstalls(_)
                | CardFilter::SamePositionAs(_)
        )
    }

    /// `InAttackedServer` written over by `InServer(server)`, through `All`
    /// and `AnyOf` — "that server", after the run (`Effect::
    /// with_attacked_server`).
    pub fn with_attacked_server(self, server: ServerId) -> CardFilter {
        match self {
            CardFilter::InAttackedServer => CardFilter::InServer(server),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_attacked_server(server)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_attacked_server(server)).collect()),
            CardFilter::Not(filter) => CardFilter::Not(Box::new(filter.with_attacked_server(server))),
            other => other,
        }
    }

    /// Whether `ThatCard` is in this filter, through `All`, `AnyOf` and
    /// `Not`.
    pub fn names_that_card(&self) -> bool {
        match self {
            CardFilter::ThatCard => true,
            CardFilter::All(filters) | CardFilter::AnyOf(filters) => filters.iter().any(CardFilter::names_that_card),
            CardFilter::Not(filter) => filter.names_that_card(),
            _ => false,
        }
    }

    /// `ThatCard` written over as `card`, through `All`, `AnyOf` and `Not`.
    pub fn with_that_card(self, card: &crate::dsl::CardId) -> CardFilter {
        match self {
            CardFilter::ThatCard => CardFilter::AmongCards(vec![card.clone()]),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_that_card(card)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_that_card(card)).collect()),
            CardFilter::Not(filter) => CardFilter::Not(Box::new(filter.with_that_card(card))),
            other => other,
        }
    }

    /// This filter with the cards an `AmongCards` names left out, for a
    /// player who may not be told them (`masking`): "that card" written in
    /// from a moment about a card in HQ (Jinja City Grid's drawn ice).
    pub fn naming_no_cards(self) -> CardFilter {
        match self {
            CardFilter::AmongCards(_) => CardFilter::AmongCards(Vec::new()),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(CardFilter::naming_no_cards).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(CardFilter::naming_no_cards).collect()),
            CardFilter::Not(filter) => CardFilter::Not(Box::new(filter.naming_no_cards())),
            other => other,
        }
    }

    /// `ChosenThisWay` written over as the installs a selection chose,
    /// through `All`, `AnyOf` and `Not`.
    pub fn with_chosen_installs(self, installs: &[crate::rules::InstallId]) -> CardFilter {
        match self {
            CardFilter::ChosenThisWay => CardFilter::AmongInstalls(installs.to_vec()),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_chosen_installs(installs)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_chosen_installs(installs)).collect()),
            CardFilter::Not(filter) => CardFilter::Not(Box::new(filter.with_chosen_installs(installs))),
            other => other,
        }
    }

    /// `SamePositionAs(PassedIce)` written over as the pass the moment was,
    /// through `All`, `AnyOf` and `Not`.
    pub fn with_passed_ice(self, server: ServerId, position: u32) -> CardFilter {
        match self {
            CardFilter::SamePositionAs(crate::dsl::MovedTo::PassedIce) => CardFilter::SamePositionAs(crate::dsl::MovedTo::Ice { server, position }),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_passed_ice(server, position)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_passed_ice(server, position)).collect()),
            CardFilter::Not(filter) => CardFilter::Not(Box::new(filter.with_passed_ice(server, position))),
            other => other,
        }
    }

    /// `TrashedThisWay` written over as the cards a mill trashed, through
    /// `All`, `AnyOf` and `Not`.
    pub fn with_those_trashed(self, cards: &[crate::dsl::CardId]) -> CardFilter {
        match self {
            CardFilter::TrashedThisWay => CardFilter::AmongCards(cards.to_vec()),
            CardFilter::All(filters) => CardFilter::All(filters.into_iter().map(|filter| filter.with_those_trashed(cards)).collect()),
            CardFilter::AnyOf(filters) => CardFilter::AnyOf(filters.into_iter().map(|filter| filter.with_those_trashed(cards)).collect()),
            CardFilter::Not(filter) => CardFilter::Not(Box::new(filter.with_those_trashed(cards))),
            other => other,
        }
    }
}

pub fn card_matches_filter(card: &CardDefinition, filter: &CardFilter) -> bool {
    match filter {
        CardFilter::Any => true,
        CardFilter::NonAgenda => !matches!(card.card_type, CardType::Agenda),
        CardFilter::CardType(t) => match (&card.card_type, t) {
            (CardType::Ice(_), CardType::Ice(_)) => true,
            (a, b) => a == b,
        },
        CardFilter::CardTypeOneOf(types) => {
            types.iter().any(|t| card_matches_filter(card, &CardFilter::CardType(t.clone())))
        }
        CardFilter::Icebreaker => card.card_type == CardType::Program && card.strength.is_some(),
        // The definition-level half only: it must be an installable Runner
        // type at all (a Trojan is not — see the variant's doc comment).
        // Affordability/memory/console are state-dependent and applied by
        // `eligible_positions`.
        CardFilter::InstallableRunnerCard => match card.card_type {
            CardType::Program => !card.installs_on_ice,
            CardType::Hardware | CardType::Resource => true,
            _ => false,
        },
        // The definition-level half: it must be ice at all. The
        // "not rezzed" half is instance-level, applied by `eligible_cards`.
        CardFilter::UnrezzedIce => matches!(card.card_type, CardType::Ice(_)),
        // Purely instance-level; the definition says nothing about it.
        CardFilter::DiscardedThisDiscardPhase => true,
        CardFilter::NotSourceCard => true,
        CardFilter::NotThisCardsHost => true,
        CardFilter::ThisCardsHost => true,
        CardFilter::HostedOnThisCard => true,
        CardFilter::CopyOfThisCard => true,
        CardFilter::NotRelaying => true,
        CardFilter::TrashableBy(_) => true,
        CardFilter::InChosenServer => true,
        // Written over before it is read; unresolved, nothing was chosen.
        CardFilter::OfChosenCardType => false,
        // Instance-level: the identity in play is state.
        CardFilter::MatchesOwnersIdentityFaction => true,
        CardFilter::Rezzed => true,
        CardFilter::Unrezzed => true,
        CardFilter::AgendaPointsAtMostRunnerTags => card.agenda_points.is_some(),
        CardFilter::AgendaPointsAtMost(points) => card.card_type == CardType::Agenda && card.agenda_points.is_some_and(|printed| printed <= *points),
        CardFilter::Facedown => true,
        CardFilter::Faceup => true,
        // Any Runner card may hold virus counters beside its own kind
        // (Contaminate's), so for a virus counter the definition decides
        // only for a Corp card.
        CardFilter::HostsCounters(kind) => card.counter_kind == Some(*kind) || (*kind == crate::dsl::CounterKind::Virus && card.side == crate::rules::Side::Runner),
        CardFilter::SpentRecurringCredit => card.recurring_credits.is_some(),
        CardFilter::AccessedDuringLastRun => true,
        CardFilter::All(filters) => filters.iter().all(|filter| card_matches_filter(card, filter)),
        CardFilter::AnyOf(filters) => filters.iter().any(|filter| card_matches_filter(card, filter)),
        // A word about the copy alone is the copy's to negate.
        CardFilter::Not(filter) => filter.is_about_the_copy_alone() || !card_matches_filter(card, filter),
        CardFilter::HasSubtype(subtype) => card.subtypes.contains(subtype),
        CardFilter::SubtypesAtLeast(n) => card.subtypes.len() as u32 >= *n,
        CardFilter::SubroutinesExactly(n) => card.subroutines.len() as u32 == *n,
        CardFilter::Unique => card.unique,
        CardFilter::HasTrashAbility => card
            .abilities
            .iter()
            .any(|ability| ability.trigger == crate::dsl::Trigger::Paid && ability.cost.as_ref().is_some_and(crate::dsl::Cost::prints_trash)),
        CardFilter::HasTrashCost => card.trash_cost.is_some(),
        CardFilter::Advanceable => card.advancement_requirement.is_some(),
        // Instance-level: where the card sits is state.
        CardFilter::InAttackedServer => true,
        CardFilter::InLastRunServer => true,
        CardFilter::InRootOfThisServer | CardFilter::InRootOf(_) => true,
        CardFilter::InThisServer | CardFilter::InServer(_) | CardFilter::Advanced | CardFilter::Unadvanced => true,
        CardFilter::Ice => matches!(card.card_type, CardType::Ice(_)),
        // The definition-level half; affordability and the play
        // requirement are instance-level.
        CardFilter::PlayableOperation => card.card_type == CardType::Operation,
        CardFilter::PlayableIgnoringAdditionalCosts => matches!(card.card_type, CardType::Operation | CardType::Event),
        // Purely instance-level: where the card sits in its zone.
        CardFilter::TopOfZone(_) | CardFilter::BottomOfZone(_) | CardFilter::CopyInHeap | CardFilter::TopOf(_) | CardFilter::NthFromTop(_) | CardFilter::Revealed => true,
        CardFilter::IceOfType(ice_type) => card.is_ice_of_type(*ice_type),
        CardFilter::InstallableRunnerCardWithDiscount(_) => card_matches_filter(card, &CardFilter::InstallableRunnerCard),
        // Instance-level, not definition-level — see the variant's doc
        // comment. `eligible_cards` applies the real check.
        CardFilter::NotInstalledThisTurn => true,
        CardFilter::NotAdvancedThisTurn => true,
        CardFilter::InstalledThisTurn => true,
        CardFilter::RezzedThisTurn => true,
        CardFilter::ScoredThisTurn => true,
        CardFilter::AmongCards(cards) => cards.contains(&card.id),
        CardFilter::TrashedThisWay | CardFilter::ThatCard | CardFilter::ChosenName | CardFilter::ChosenThisWay => false,
        CardFilter::AmongInstalls(_) => true,
        CardFilter::SamePositionAs(_) => matches!(card.card_type, CardType::Ice(_)),
        CardFilter::LastEncounteredLastRun => true,
        CardFilter::PassedDuringLastRun => true,
        CardFilter::RezzedDuringLastRun => true,
        CardFilter::UsedDuringLastRun => true,
        CardFilter::FullyBrokenDuringThisRun => true,
        CardFilter::BeingEncountered => true,
        CardFilter::BeingApproached => true,
        CardFilter::SwappableIntoThis => true,
        CardFilter::PrintedCostAtMost(at_most) => match **at_most {
            crate::dsl::Amount::Fixed(n) => !matches!(card.card_type, CardType::Agenda | CardType::Identity) && card.cost <= n,
            _ => false,
        },
        CardFilter::PrintedCostExactly(exactly) => match **exactly {
            crate::dsl::Amount::Fixed(n) => !matches!(card.card_type, CardType::Agenda | CardType::Identity) && card.cost == n,
            _ => false,
        },
        CardFilter::AgendaPointsExactly(exactly) => match **exactly {
            crate::dsl::Amount::Fixed(n) => card.card_type == CardType::Agenda && card.agenda_points == Some(n),
            _ => false,
        },
        CardFilter::AdvancementRequirementExactly(exactly) => match **exactly {
            crate::dsl::Amount::Fixed(n) => card.card_type == CardType::Agenda && card.advancement_requirement == Some(n),
            _ => false,
        },
        CardFilter::TrashCostExactly(exactly) => match **exactly {
            crate::dsl::Amount::Fixed(n) => card.trash_cost == Some(n),
            _ => false,
        },
        CardFilter::SameTypeAsPaidCard => false,
        // Instance-level: the selection in progress.
        CardFilter::DifferentNames => true,
    }
}
