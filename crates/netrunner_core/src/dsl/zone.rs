use serde::{Deserialize, Serialize};

use crate::dsl::card::{CardDefinition, CardType};

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
    OwnStack,
    OwnGrip,
    OwnHeap,
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
    /// non-Trojan Program, Hardware or Resource that is affordable, fits
    /// the memory budget and respects the console limit: the target set of
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
    /// `InstallableRunnerCard` priced `u32` cheaper — the offer half of
    /// `Effect::InstallRunnerCardFromGripWithDiscount`.
    InstallableRunnerCardWithDiscount(u32),
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
    /// can advance".
    Advanceable,
    /// An installed Corp card in the root of, or a piece of ice
    /// protecting, the server the current run is against — LEO
    /// Construction: Labor Solutions' "in the root of or protecting the
    /// attacked server". Instance-level; matches nothing outside a run,
    /// which is what makes the identity's ability legal only during one
    /// without a separate "during a run" requirement.
    InAttackedServer,
    /// An operation in the zone being selected from — HQ, or Archives for
    /// Plutus — that the Corp could play right now: its cost affordable
    /// and its `play_requirement` met. The offer half of
    /// `Effect::PlayOperation` (Humanoid Resources' "You may play 1
    /// operation from HQ"). Instance-level: affordability is state. The
    /// effect re-checks, so the offer and the resolution cannot disagree.
    PlayableOperation,
    /// One of the top `u32` cards of the zone being selected from —
    /// Poétrï Luxury Brands' "look at the top 3 cards of R&D". Instance-
    /// level, and a *filter* rather than a `CardZoneRef` of its own so it
    /// composes with the type filter through `All` and leaves the
    /// selection's zone (and therefore the position a `then` install
    /// re-finds the card by) the whole of R&D. The top of R&D and of the
    /// stack is the *end* of the `Vec` — see `zone_card_ids`.
    TopOfZone(u32),
    /// Ice of exactly this subtype — Mycoweb's "a rezzed **sentry**" and
    /// "another rezzed **code gate**". `CardType(Ice(_))`'s payload is
    /// explicitly a don't-care (see this module's `card_matches_filter`
    /// doc), so it cannot express this, and `HasSubtype` reads
    /// `CardSubtype`, which is the printed-tag vocabulary rather than the
    /// ice-type one.
    IceOfType(crate::dsl::IceType),
}

/// Whether `card` is eligible under `filter`. `CardType(CardType::Ice(_))`
/// matches ICE of any subtype — the specific `IceType` payload on the
/// filter's own `CardType::Ice` value is a don't-care placeholder, not a
/// subtype restriction (author it as e.g. `CardType::Ice(IceType::Barrier)`;
/// any subtype works identically).
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
        CardFilter::Rezzed => true,
        CardFilter::Unrezzed => true,
        CardFilter::AccessedDuringLastRun => true,
        CardFilter::All(filters) => filters.iter().all(|filter| card_matches_filter(card, filter)),
        CardFilter::AnyOf(filters) => filters.iter().any(|filter| card_matches_filter(card, filter)),
        CardFilter::HasSubtype(subtype) => card.subtypes.contains(subtype),
        CardFilter::Advanceable => card.advancement_requirement.is_some(),
        // Instance-level: where the card sits is state.
        CardFilter::InAttackedServer => true,
        // The definition-level half; affordability and the play
        // requirement are instance-level.
        CardFilter::PlayableOperation => card.card_type == CardType::Operation,
        // Purely instance-level: where the card sits in its zone.
        CardFilter::TopOfZone(_) => true,
        CardFilter::IceOfType(ice_type) => matches!(&card.card_type, CardType::Ice(t) if t == ice_type),
        CardFilter::InstallableRunnerCardWithDiscount(_) => card_matches_filter(card, &CardFilter::InstallableRunnerCard),
        // Instance-level, not definition-level — see the variant's doc
        // comment. `eligible_cards` applies the real check.
        CardFilter::NotInstalledThisTurn => true,
    }
}
