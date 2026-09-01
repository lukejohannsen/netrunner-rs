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
        // Instance-level, not definition-level — see the variant's doc
        // comment. `eligible_cards` applies the real check.
        CardFilter::NotInstalledThisTurn => true,
    }
}
