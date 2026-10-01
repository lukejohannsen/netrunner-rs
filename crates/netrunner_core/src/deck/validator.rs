//! Validates a `Decklist` against a `CardRegistry` for a given `NsgFormat` —
//! deckbuilding-time legality (structure, influence, format pool, banlist),
//! not gameplay-executability (see `deck` module doc comment for how this
//! differs from `rules::deck::validate_deck`).

use thiserror::Error;

use crate::card::Faction;
use crate::cards::CardRegistry;
use crate::deck::Decklist;
use crate::dsl::{CardDefinition, CardId, CardType, DeckRule};
use crate::format::{FormatRules, NsgFormat, DEFAULT_INFLUENCE_LIMIT};
use crate::rules::Side;

/// Flat copy-limit applied to every non-restricted card in a deck whose own
/// `deck_limit` isn't set. Mirrors `rules::deck::MAX_COPIES_PER_CARD` (same
/// value) — kept as its own constant rather than importing that one, since
/// the two validators deliberately don't depend on each other (see the
/// `deck` module doc comment).
pub const MAX_COPIES_PER_CARD: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeckValidationError {
    #[error("identity {0:?} not found in the card registry")]
    IdentityNotFound(CardId),

    #[error("card {0:?} is not an Identity")]
    NotAnIdentity(CardId),

    #[error("card {0:?} not found in the card registry")]
    CardNotFound(CardId),

    /// Fires when a non-identity card's `side` doesn't match the identity's
    /// `side` — named to match this validator's requested error surface
    /// ("faction mismatch" is loose NRDB-community shorthand for this
    /// check; the actual field compared is `side`, not `Faction`, since
    /// real Netrunner never hard-bans an off-faction card by faction alone
    /// — only influence does that).
    #[error("card {card:?} is {actual:?}-side, which does not match the identity's {expected:?} side")]
    FactionMismatch { card: CardId, expected: Side, actual: Side },

    #[error("deck has {size} cards, below the identity's minimum of {minimum}")]
    DeckSizeTooSmall { size: u32, minimum: u32 },

    #[error("card {card:?} has {count} copies, exceeding the limit of {max}")]
    TooManyCopies { card: CardId, count: u32, max: u32 },

    #[error("total out-of-faction influence spent ({spent}) exceeds the identity's budget of {limit}")]
    InfluenceExceeded { spent: u32, limit: u32 },

    /// Fires whenever a Corp deck's total agenda points fall outside the
    /// size-derived legal range, above or below — named for the
    /// requirement's most common real-world trigger (a deck light on
    /// agendas), not exclusively the below-range case.
    #[error("agenda points ({points}) fall outside the legal range [{min}, {max}] for a {size}-card deck")]
    InsufficientAgendaPoints { points: u32, min: u32, max: u32, size: u32 },

    #[error("runner decks may not include agendas (card {0:?})")]
    RunnerDeckContainsAgenda(CardId),

    /// CR 1.4.4: "Decks cannot contain identity cards."
    #[error("identity {0:?} cannot be one of a deck's cards")]
    IdentityInDeck(CardId),

    /// A Corp deck may include only its own faction's agendas and neutral
    /// ones. A separate rule rather than an influence charge because
    /// agendas print no influence: priced through the influence check, an
    /// out-of-faction agenda cost 0 and the deck validated.
    #[error("agenda {card:?} is {faction:?}; a {identity_faction:?} deck may include only its own faction's agendas and neutral ones")]
    OutOfFactionAgenda { card: CardId, faction: Faction, identity_faction: Faction },

    /// Ampère's "up to 2 different agenda cards from each Corp faction"
    /// (`DeckRule::AgendasFromEachFaction`), broken by a third title.
    #[error("the deck holds {count} different {faction:?} agendas; its identity allows {max} from each Corp faction")]
    TooManyAgendasFromFaction { faction: Faction, count: u32, max: u32 },

    #[error("card {card:?} is banned in {format:?}")]
    BannedCardIncluded { card: CardId, format: NsgFormat },

    #[error("card {card:?} is not in {format:?}'s card pool")]
    NotInPool { card: CardId, format: NsgFormat },

    #[error("restricted cards cost {spent} points, over {format:?}'s budget of {budget}")]
    RestrictionBudgetExceeded { spent: u32, budget: u32, format: NsgFormat },
}

/// A successfully validated deck's summary — the useful-to-a-caller
/// byproduct of validation, not re-derivable from `Decklist` alone without
/// re-walking the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    pub format: NsgFormat,
    pub deck_size: u32,
    pub identity_faction: Faction,
    pub influence_spent: u32,
    /// `Some` for a Corp deck (its validated agenda-point total), `None`
    /// for a Runner deck (the check doesn't apply).
    pub agenda_points: Option<u32>,
}

/// A deck's running totals against its identity's limits — what a deck
/// builder shows while the list is still illegal.
///
/// `validate_deck` answers "may this be played?" and stops at the first
/// rule broken, which is right for a gate and no help mid-build: a
/// 30-card deck is "below the minimum" and says nothing about its
/// influence. This never fails on a rule, only on a card it cannot find,
/// and it shares the validator's influence rule (`influence_per_copy`) and
/// agenda range (`agenda_point_range`), so the numbers a builder shows are
/// the numbers the gate will check. A client reads legality from
/// `validate_deck`, never from these — they are progress, not a verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckTally {
    pub size: u32,
    pub min_size: u32,
    pub influence_spent: u32,
    /// `None` for an identity with no influence budget (the starter
    /// identities' `unlimited_influence`).
    pub influence_limit: Option<u32>,
    /// Corp decks only.
    pub agenda: Option<AgendaTally>,
}

/// A Corp deck's agenda points and the legal range for its current size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgendaTally {
    pub points: u32,
    pub min: u32,
    pub max: u32,
}

/// See `DeckTally`.
pub fn tally_deck(deck: &Decklist, registry: &CardRegistry) -> Result<DeckTally, DeckValidationError> {
    let identity =
        registry.get(&deck.identity).ok_or_else(|| DeckValidationError::IdentityNotFound(deck.identity.clone()))?;
    if identity.card_type != CardType::Identity {
        return Err(DeckValidationError::NotAnIdentity(deck.identity.clone()));
    }
    let size: u32 = deck.cards.values().sum();
    let mut influence_spent = 0u32;
    let mut points = 0u32;
    for (card_id, &count) in &deck.cards {
        let card = registry.get(card_id).ok_or_else(|| DeckValidationError::CardNotFound(card_id.clone()))?;
        influence_spent += influence_per_copy(card, identity) * count;
        if card.card_type == CardType::Agenda {
            points += card.agenda_points.unwrap_or(0) * count;
        }
    }
    let agenda = (identity.side == Side::Corp).then(|| {
        let (min, max) = crate::rules::deck::agenda_point_range(size);
        AgendaTally { points, min, max }
    });
    Ok(DeckTally {
        size,
        min_size: identity.min_deck_size.unwrap_or(0),
        influence_spent,
        influence_limit: (!identity.unlimited_influence).then(|| identity.influence_limit.unwrap_or(DEFAULT_INFLUENCE_LIMIT)),
        agenda,
    })
}

/// What one copy of `card` costs `identity` in influence: its printed
/// cost when it is out of faction, nothing when it is in faction or
/// neutral. The one statement of that rule, for the gate, the tally and a
/// deck builder pricing a card before it is added.
pub fn influence_per_copy(card: &CardDefinition, identity: &CardDefinition) -> u32 {
    let identity_faction = identity.faction.unwrap_or(Faction::NeutralCorp);
    let card_faction = card.faction.unwrap_or(Faction::NeutralCorp);
    if card_faction != identity_faction && !is_neutral(card_faction) { card.influence_cost.unwrap_or(0) } else { 0 }
}

/// Validates `deck` against `registry` for `format`, checking in order:
/// identity exists, is an `Identity`, and is format-legal; total deck size
/// meets the identity's minimum; every card exists, matches the identity's
/// side, isn't an Agenda in a Runner deck, is format-legal, respects its
/// copy limit; total out-of-faction influence is within budget; and (Corp
/// decks only) total agenda points fall within the size-derived legal
/// range. Fails fast on the first violated rule, matching `rules::deck::
/// validate_deck`'s convention. Does NOT check `CardDefinition::
/// is_playable` — this is a deckbuilding-legality check for NetrunnerDB-
/// sourced decks that may legitimately reference cards this engine hasn't
/// implemented gameplay for yet; only `rules::deck::validate_deck` (the
/// gameplay-executability gate `GameState::setup` calls) enforces that.
pub fn validate_deck(
    deck: &Decklist,
    registry: &CardRegistry,
    format: NsgFormat,
) -> Result<ValidationReport, DeckValidationError> {
    validate_deck_with_rules(deck, registry, format, format.rules())
}

/// `validate_deck` against rules supplied by the caller rather than
/// `NsgFormat::rules()`.
///
/// The seam exists because the shipped tables are a deliberately small
/// seed (see `format`'s module comment) — every format currently bans
/// nothing and restricts nothing, so the ban and restriction paths would
/// otherwise be unreachable and untestable until real data landed. `format`
/// is still passed because it is what the error variants name, and a rule
/// set is meaningless without saying whose it is. Named for the
/// `evaluate_state_with` idiom the bots crate already uses for exactly this
/// "same function, caller-supplied configuration" shape.
pub fn validate_deck_with_rules(
    deck: &Decklist,
    registry: &CardRegistry,
    format: NsgFormat,
    rules: &FormatRules,
) -> Result<ValidationReport, DeckValidationError> {

    let identity =
        registry.get(&deck.identity).ok_or_else(|| DeckValidationError::IdentityNotFound(deck.identity.clone()))?;
    if identity.card_type != CardType::Identity {
        return Err(DeckValidationError::NotAnIdentity(deck.identity.clone()));
    }
    check_format_legality(&deck.identity, format, rules)?;

    // A missing `min_deck_size` degrades to "no minimum enforced" rather
    // than a new error variant — every real Identity card carries this
    // field, so it's a defensive fallback for malformed registry data, not
    // a legality rule this validator is meant to express.
    let min_deck_size = identity.min_deck_size.unwrap_or(0);
    let deck_size: u32 = deck.cards.values().sum();
    if deck_size < min_deck_size {
        return Err(DeckValidationError::DeckSizeTooSmall { size: deck_size, minimum: min_deck_size });
    }

    let identity_faction = identity.faction.unwrap_or(Faction::NeutralCorp);
    let agendas_from_each_faction = identity.deck_rules.iter().find_map(|rule| match rule {
        DeckRule::AgendasFromEachFaction(n) => Some(*n),
        _ => None,
    });

    let mut influence_spent = 0u32;
    let mut agenda_points = 0u32;
    let mut restriction_spent = 0u32;
    // Different titles, by faction, of the agendas the identity's
    // `AgendasFromEachFaction` admits from outside its own.
    let mut foreign_agendas: Vec<Faction> = Vec::new();

    for (card_id, &count) in &deck.cards {
        let card = registry.get(card_id).ok_or_else(|| DeckValidationError::CardNotFound(card_id.clone()))?;

        if card.side != identity.side {
            return Err(DeckValidationError::FactionMismatch {
                card: card_id.clone(),
                expected: identity.side,
                actual: card.side,
            });
        }
        if identity.side == Side::Runner && card.card_type == CardType::Agenda {
            return Err(DeckValidationError::RunnerDeckContainsAgenda(card_id.clone()));
        }
        if card.card_type == CardType::Identity {
            return Err(DeckValidationError::IdentityInDeck(card_id.clone()));
        }
        if card.card_type == CardType::Agenda {
            let faction = card.faction.unwrap_or(Faction::NeutralCorp);
            if faction != identity_faction && !is_neutral(faction) {
                if agendas_from_each_faction.is_none() {
                    return Err(DeckValidationError::OutOfFactionAgenda { card: card_id.clone(), faction, identity_faction });
                }
                // One entry per title: `deck.cards` is keyed by card.
                foreign_agendas.push(faction);
            }
        }

        check_format_legality(card_id, format, rules)?;

        // The copy limit is the card's own; the restriction list is a
        // budget spent below, not a cap on copies. Capping a restricted
        // card at one copy was the old approximation of a rule that
        // constrains the deck rather than the card — see `FormatRules`.
        let max_copies = card.copy_limit_under(Some(identity), MAX_COPIES_PER_CARD);
        if count > max_copies {
            return Err(DeckValidationError::TooManyCopies { card: card_id.clone(), count, max: max_copies });
        }
        // Once per distinct card, whatever the count.
        restriction_spent = restriction_spent.saturating_add(rules.restriction_points.get(card_id).copied().unwrap_or(0));

        influence_spent += influence_per_copy(card, identity) * count;
        if card.card_type == CardType::Agenda {
            agenda_points += card.agenda_points.unwrap_or(0) * count;
        }
    }

    if let Some(max) = agendas_from_each_faction {
        for faction in &foreign_agendas {
            let count = foreign_agendas.iter().filter(|other| *other == faction).count() as u32;
            if count > max {
                return Err(DeckValidationError::TooManyAgendasFromFaction { faction: *faction, count, max });
            }
        }
    }

    // The starter identities, Nova Initiumia and Ampère have no influence
    // budget (`unlimited_influence`, the catalog's `influence_limit: null`);
    // every other identity gets its
    // printed budget, or the flat default when it prints none.
    let limit = identity.influence_limit.unwrap_or(DEFAULT_INFLUENCE_LIMIT);
    if !identity.unlimited_influence && influence_spent > limit {
        return Err(DeckValidationError::InfluenceExceeded { spent: influence_spent, limit });
    }

    // Checked here rather than per card, because the budget is a property
    // of the whole deck — which is the thing the old per-card copy cap
    // could not express.
    if restriction_spent > rules.restriction_budget {
        return Err(DeckValidationError::RestrictionBudgetExceeded {
            spent: restriction_spent,
            budget: rules.restriction_budget,
            format,
        });
    }

    let agenda_points_report = if identity.side == Side::Corp {
        let (min, max) = crate::rules::deck::agenda_point_range(deck_size);
        if agenda_points < min || agenda_points > max {
            return Err(DeckValidationError::InsufficientAgendaPoints { points: agenda_points, min, max, size: deck_size });
        }
        Some(agenda_points)
    } else {
        None
    };

    Ok(ValidationReport {
        format,
        deck_size,
        identity_faction,
        influence_spent,
        agenda_points: agenda_points_report,
    })
}

/// Neutral-faction cards always cost 0 influence, regardless of the
/// identity's own faction — the only faction category exempt from the
/// off-faction influence charge.
fn is_neutral(faction: Faction) -> bool {
    matches!(faction, Faction::NeutralCorp | Faction::NeutralRunner)
}

fn check_format_legality(card_id: &CardId, format: NsgFormat, rules: &FormatRules) -> Result<(), DeckValidationError> {
    if rules.banned.contains(card_id) {
        return Err(DeckValidationError::BannedCardIncluded { card: card_id.clone(), format });
    }
    if !rules.in_pool(card_id) {
        return Err(DeckValidationError::NotInPool { card: card_id.clone(), format });
    }
    Ok(())
}

// The fixtures below number their cards 1, 100, 901… — ids no real
// pool holds — so a test of deck construction judges them in Casual, and a
// test of a pool supplies its own through `validate_deck_with_rules`.
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A fixture card's id: its number, as no real card is named.
    fn c(id: u32) -> CardId {
        CardId(format!("card_{id}"))
    }

    fn identity(id: u32, side: Side, faction: Faction, min_deck_size: u32) -> CardDefinition {
        CardDefinition {
            faction: Some(faction),
            min_deck_size: Some(min_deck_size),
            ..crate::cards::common::base_card(&c(id).0, &c(id).0, side, CardType::Identity, 0)
        }
    }

    fn card(
        id: u32,
        side: Side,
        faction: Faction,
        card_type: CardType,
        influence_cost: Option<u32>,
    ) -> CardDefinition {
        CardDefinition {
            faction: Some(faction),
            influence_cost,
            ..crate::cards::common::base_card(&c(id).0, &c(id).0, side, card_type, 1)
        }
    }

    fn agenda(id: u32, points: u32) -> CardDefinition {
        let mut agenda = card(id, Side::Corp, Faction::NeutralCorp, CardType::Agenda, None);
        agenda.agenda_points = Some(points);
        agenda
    }

    /// Registers `total` non-agenda, in-faction Corp filler cards (0
    /// influence), split across as many distinct ids as needed to respect
    /// `MAX_COPIES_PER_CARD`, and returns matching `Decklist.cards` entries.
    fn corp_filler(registry: &mut CardRegistry, faction: Faction, start_id: u32, total: u32) -> HashMap<CardId, u32> {
        filler(registry, Side::Corp, faction, CardType::Asset, start_id, total)
    }

    fn runner_filler(registry: &mut CardRegistry, faction: Faction, start_id: u32, total: u32) -> HashMap<CardId, u32> {
        filler(registry, Side::Runner, faction, CardType::Event, start_id, total)
    }

    fn filler(
        registry: &mut CardRegistry,
        side: Side,
        faction: Faction,
        card_type: CardType,
        start_id: u32,
        total: u32,
    ) -> HashMap<CardId, u32> {
        let mut cards = HashMap::new();
        let mut remaining = total;
        let mut id = start_id;
        while remaining > 0 {
            let copies = remaining.min(MAX_COPIES_PER_CARD);
            registry.insert(card(id, side, faction, card_type.clone(), None));
            cards.insert(c(id), copies);
            remaining -= copies;
            id += 1;
        }
        cards
    }

    /// A minimal, legal 45-card Corp deck: 4 distinct 5-point Agendas (20
    /// points, within [20,22]) plus 41 in-faction filler cards.
    fn valid_corp_registry_and_deck() -> (CardRegistry, Decklist) {
        let mut registry = CardRegistry::new();
        registry.insert(identity(1, Side::Corp, Faction::WeylandConsortium, 45));

        let mut cards = HashMap::new();
        for i in 0..4 {
            registry.insert(agenda(100 + i, 5));
            cards.insert(c(100 + i), 1);
        }
        cards.extend(corp_filler(&mut registry, Faction::WeylandConsortium, 200, 41));

        (registry, Decklist { identity: c(1), cards })
    }

    fn valid_runner_registry_and_deck() -> (CardRegistry, Decklist) {
        let mut registry = CardRegistry::new();
        registry.insert(identity(2, Side::Runner, Faction::Criminal, 45));
        let cards = runner_filler(&mut registry, Faction::Criminal, 300, 45);

        (registry, Decklist { identity: c(2), cards })
    }

    #[test]
    fn a_valid_corp_deck_passes() {
        let (registry, deck) = valid_corp_registry_and_deck();
        let report = validate_deck(&deck, &registry, NsgFormat::Casual).expect("well-formed deck should validate");
        assert_eq!(report.deck_size, 45);
        assert_eq!(report.agenda_points, Some(20));
        assert_eq!(report.influence_spent, 0);
    }

    #[test]
    fn a_valid_runner_deck_passes() {
        let (registry, deck) = valid_runner_registry_and_deck();
        let report = validate_deck(&deck, &registry, NsgFormat::Casual).expect("well-formed deck should validate");
        assert_eq!(report.deck_size, 45);
        assert_eq!(report.agenda_points, None);
    }

    #[test]
    fn deck_size_too_small_is_rejected() {
        let mut registry = CardRegistry::new();
        registry.insert(identity(1, Side::Corp, Faction::WeylandConsortium, 45));
        let cards = corp_filler(&mut registry, Faction::WeylandConsortium, 200, 10);
        let deck = Decklist { identity: c(1), cards };

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::DeckSizeTooSmall { size: 10, minimum: 45 })
        );
    }

    #[test]
    fn insufficient_agenda_points_is_rejected() {
        let mut registry = CardRegistry::new();
        registry.insert(identity(1, Side::Corp, Faction::WeylandConsortium, 45));
        let mut cards = HashMap::new();
        registry.insert(agenda(100, 5));
        registry.insert(agenda(101, 5));
        cards.insert(c(100), 1);
        cards.insert(c(101), 1); // 10 points total, below [20, 22]
        cards.extend(corp_filler(&mut registry, Faction::WeylandConsortium, 200, 43));
        let deck = Decklist { identity: c(1), cards };

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::InsufficientAgendaPoints { points: 10, min: 20, max: 21, size: 45 })
        );
    }

    #[test]
    fn influence_exceeded_is_rejected() {
        let mut registry = CardRegistry::new();
        registry.insert(identity(2, Side::Runner, Faction::Criminal, 45));
        let mut cards = runner_filler(&mut registry, Faction::Criminal, 300, 39);
        // 3 copies of a 4-influence off-faction (Anarch) card (12) plus 2
        // copies of a second 4-influence off-faction (Shaper) card (8) — 20
        // total, over the 15 budget on its own.
        registry.insert(card(400, Side::Runner, Faction::Anarch, CardType::Program, Some(4)));
        registry.insert(card(401, Side::Runner, Faction::Shaper, CardType::Program, Some(4)));
        cards.insert(c(400), 3); // 12 influence
        cards.insert(c(401), 2); // 8 influence -> 20 total, over the 15 budget
        cards.insert(c(402), 1);
        registry.insert(card(402, Side::Runner, Faction::Criminal, CardType::Program, None));
        let deck = Decklist { identity: c(2), cards };

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::InfluenceExceeded { spent: 20, limit: 15 })
        );
    }

    #[test]
    fn banned_card_included_is_rejected() {
        let mut registry = CardRegistry::new();
        registry.insert(card(500, Side::Runner, Faction::Criminal, CardType::Program, None));

        // A format whose rules ban card 500 — built directly rather than via
        // `NsgFormat::rules()`, since no real format's hardcoded banlist
        // includes a synthetic test id.
        let rules = FormatRules { banned: std::collections::HashSet::from([c(500)]), ..Default::default() };
        assert_eq!(
            check_format_legality(&c(500), NsgFormat::Standard, &rules),
            Err(DeckValidationError::BannedCardIncluded { card: c(500), format: NsgFormat::Standard })
        );
    }

    #[test]
    fn faction_mismatch_is_rejected_when_a_card_side_differs_from_the_identity() {
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        registry.insert(card(600, Side::Runner, Faction::Criminal, CardType::Program, None));
        deck.cards.insert(c(600), 1);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::FactionMismatch { card: c(600), expected: Side::Corp, actual: Side::Runner })
        );
    }

    /// A card outside a format's pool is refused in it and nowhere else.
    /// The fixture's codes are in no real pool, so the pool is the test's
    /// own, through the seam the budget tests use.
    #[test]
    fn a_card_outside_the_pool_is_refused_in_that_format_and_casual_admits_it() {
        let mut registry = CardRegistry::new();
        registry.insert(identity(2, Side::Runner, Faction::Criminal, 45));
        let mut cards = runner_filler(&mut registry, Faction::Criminal, 300, 44);
        registry.insert(card(700, Side::Runner, Faction::Criminal, CardType::Program, None));
        cards.insert(c(700), 1);
        let deck = Decklist { identity: c(2), cards };
        let pool = FormatRules {
            cards: Some(registry.iter().map(|card| card.id.clone()).filter(|id| *id != c(700)).collect()),
            ..FormatRules::default()
        };

        assert_eq!(
            validate_deck_with_rules(&deck, &registry, NsgFormat::Startup, &pool),
            Err(DeckValidationError::NotInPool { card: c(700), format: NsgFormat::Startup })
        );
        assert!(validate_deck(&deck, &registry, NsgFormat::Casual).is_ok());
    }

    /// The restriction budget, which no shipped format uses yet because
    /// the tables are a seed — so it is exercised through
    /// `validate_deck_with_rules`, which exists for this.
    ///
    /// The rule it replaces could not be expressed at all: a per-card copy
    /// cap says "at most one copy of *this* card", where the real list
    /// says "at most this much restricted card in the deck, whichever you
    /// pick". Three copies of one listed card is legal here and one copy
    /// each of two listed cards is not, which is precisely the distinction
    /// the old approximation got backwards.
    #[test]
    fn the_restriction_budget_constrains_the_deck_not_the_card() {
        use std::collections::HashMap;
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        registry.insert(card(901, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None));
        registry.insert(card(902, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None));
        let listed = |ids: &[u32]| FormatRules {
            restriction_points: ids.iter().map(|id| (c(*id), 1)).collect::<HashMap<_, _>>(),
            restriction_budget: 1,
            ..FormatRules::default()
        };

        // Three copies of one listed card: one point, inside the budget.
        deck.cards.insert(c(901), 3);
        assert!(
            validate_deck_with_rules(&deck, &registry, NsgFormat::Standard, &listed(&[901, 902])).is_ok(),
            "cost is per card, not per copy"
        );

        // One copy each of two listed cards: two points, over it.
        deck.cards.insert(c(902), 1);
        assert_eq!(
            validate_deck_with_rules(&deck, &registry, NsgFormat::Standard, &listed(&[901, 902])),
            Err(DeckValidationError::RestrictionBudgetExceeded {
                spent: 2,
                budget: 1,
                format: NsgFormat::Standard,
            })
        );

        // The same deck is legal where neither card is listed, and where
        // the budget covers both.
        assert!(validate_deck_with_rules(&deck, &registry, NsgFormat::Standard, &FormatRules::default()).is_ok());
        let generous = FormatRules { restriction_budget: 2, ..listed(&[901, 902]) };
        assert!(validate_deck_with_rules(&deck, &registry, NsgFormat::Standard, &generous).is_ok());
    }

    /// A listed card is not otherwise capped: the budget is the only thing
    /// the list does, and copies remain `CardDefinition::deck_limit`'s job.
    #[test]
    fn a_restricted_card_keeps_its_own_copy_limit() {
        use std::collections::HashMap;
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        let mut limited = card(903, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None);
        limited.deck_limit = Some(1);
        registry.insert(limited);
        deck.cards.insert(c(903), 2);
        let rules = FormatRules {
            restriction_points: HashMap::from([(c(903), 1)]),
            restriction_budget: 5,
            ..FormatRules::default()
        };
        assert_eq!(
            validate_deck_with_rules(&deck, &registry, NsgFormat::Standard, &rules),
            Err(DeckValidationError::TooManyCopies { card: c(903), count: 2, max: 1 }),
            "the card's own limit still applies, and is not the list's doing"
        );
    }

    #[test]
    fn too_many_copies_is_rejected() {
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        registry.insert(card(999, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None));
        deck.cards.insert(c(999), 4);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::TooManyCopies { card: c(999), count: 4, max: 3 })
        );
    }

    #[test]
    fn a_card_specific_deck_limit_overrides_the_flat_copy_limit() {
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        let mut restricted_card = card(998, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None);
        restricted_card.deck_limit = Some(1);
        registry.insert(restricted_card);
        deck.cards.insert(c(998), 2);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::TooManyCopies { card: c(998), count: 2, max: 1 })
        );
    }

    #[test]
    fn identity_not_found_and_not_an_identity_are_rejected() {
        let registry = CardRegistry::new();
        let deck = Decklist { identity: c(9999), cards: HashMap::new() };
        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::IdentityNotFound(c(9999)))
        );

        let mut registry = CardRegistry::new();
        registry.insert(card(1, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None));
        let deck = Decklist { identity: c(1), cards: HashMap::new() };
        assert_eq!(validate_deck(&deck, &registry, NsgFormat::Casual), Err(DeckValidationError::NotAnIdentity(c(1))));
    }

    #[test]
    fn an_agenda_from_another_faction_is_rejected_and_a_neutral_one_is_not() {
        // The fixture's four agendas are neutral, and it validates.
        let (registry, deck) = valid_corp_registry_and_deck();
        validate_deck(&deck, &registry, NsgFormat::Casual).expect("neutral agendas are legal in any Corp deck");

        // Swap one for a 5-point Jinteki agenda in the Weyland deck: same
        // points, same size, no influence printed — only the faction rule
        // can refuse it.
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        let mut foreign = card(900, Side::Corp, Faction::Jinteki, CardType::Agenda, None);
        foreign.agenda_points = Some(5);
        registry.insert(foreign);
        deck.cards.remove(&c(100));
        deck.cards.insert(c(900), 1);
        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::OutOfFactionAgenda {
                card: c(900),
                faction: Faction::Jinteki,
                identity_faction: Faction::WeylandConsortium
            })
        );

        // And in faction it is fine.
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        let mut own = card(901, Side::Corp, Faction::WeylandConsortium, CardType::Agenda, None);
        own.agenda_points = Some(5);
        registry.insert(own);
        deck.cards.remove(&c(100));
        deck.cards.insert(c(901), 1);
        validate_deck(&deck, &registry, NsgFormat::Casual).expect("an in-faction agenda is legal");
    }

    #[test]
    fn runner_deck_containing_an_agenda_is_rejected() {
        let (mut registry, mut deck) = valid_runner_registry_and_deck();
        // A mislabeled card: `CardType::Agenda` but tagged Runner-side, to
        // isolate this check from the more general side-mismatch check
        // (real Agendas are always Corp-side, which would trip
        // `FactionMismatch` first).
        let mut mislabeled = agenda(800, 3);
        mislabeled.side = Side::Runner;
        registry.insert(mislabeled);
        deck.cards.insert(c(800), 1);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::RunnerDeckContainsAgenda(c(800)))
        );
    }

    /// Nova Initiumia's "Your deck cannot include more than 1 copy of any
    /// card": a second copy is refused under the identity and nowhere
    /// else, and a card that prints a lower limit keeps it.
    #[test]
    fn an_identity_that_prints_one_copy_of_each_card_refuses_a_second() {
        let (mut registry, mut deck) = valid_runner_registry_and_deck();
        let mut nova = identity(3, Side::Runner, Faction::NeutralRunner, 45);
        nova.deck_rules = vec![DeckRule::CopiesOfEachCard(1)];
        registry.insert(nova);
        deck.identity = c(3);
        // The fixture's filler holds three copies of each card.
        assert!(matches!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::TooManyCopies { count: 3, max: 1, .. })
        ));
        deck.cards = runner_filler_singletons(&mut registry, 1000, 45);
        validate_deck(&deck, &registry, NsgFormat::Casual).expect("forty-five different cards");
        assert_eq!(registry.get(&c(1000)).unwrap().copy_limit_under(registry.get(&c(3)), MAX_COPIES_PER_CARD), 1);
        assert_eq!(registry.get(&c(1000)).unwrap().copy_limit_under(registry.get(&c(2)), MAX_COPIES_PER_CARD), 3);
    }

    fn runner_filler_singletons(registry: &mut CardRegistry, start_id: u32, total: u32) -> HashMap<CardId, u32> {
        (start_id..start_id + total)
            .map(|id| {
                registry.insert(card(id, Side::Runner, Faction::Criminal, CardType::Event, None));
                (c(id), 1)
            })
            .collect()
    }

    /// Ampère's "Your deck may include up to 2 different agenda cards from
    /// each Corp faction": two Jinteki titles are legal under it, a third
    /// is not, and without the rule the first is refused.
    #[test]
    fn an_identity_may_admit_two_different_agendas_from_each_corp_faction() {
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        let mut ampere = identity(4, Side::Corp, Faction::NeutralCorp, 45);
        ampere.deck_rules = vec![DeckRule::AgendasFromEachFaction(2)];
        registry.insert(ampere);
        for (id, faction) in [(910, Faction::Jinteki), (911, Faction::Jinteki), (912, Faction::Jinteki), (913, Faction::Nbn)] {
            let mut foreign = card(id, Side::Corp, faction, CardType::Agenda, None);
            foreign.agenda_points = Some(5);
            registry.insert(foreign);
        }
        // Swap the fixture's neutral 5-pointers for foreign ones, keeping
        // the points and the size.
        let swap = |deck: &mut Decklist, out: u32, into: u32| {
            deck.cards.remove(&c(out));
            deck.cards.insert(c(into), 1);
        };
        swap(&mut deck, 100, 910);
        swap(&mut deck, 101, 911);
        swap(&mut deck, 102, 913);
        assert!(matches!(validate_deck(&deck, &registry, NsgFormat::Casual), Err(DeckValidationError::OutOfFactionAgenda { .. })));
        deck.identity = c(4);
        validate_deck(&deck, &registry, NsgFormat::Casual).expect("two Jinteki titles and one NBN");
        swap(&mut deck, 103, 912);
        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::TooManyAgendasFromFaction { faction: Faction::Jinteki, count: 3, max: 2 })
        );
    }

    /// CR 1.4.4: "Decks cannot contain identity cards."
    #[test]
    fn a_deck_holding_an_identity_is_rejected() {
        let (mut registry, mut deck) = valid_runner_registry_and_deck();
        registry.insert(identity(801, Side::Runner, Faction::Criminal, 45));
        deck.cards.insert(c(801), 1);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Casual),
            Err(DeckValidationError::IdentityInDeck(c(801)))
        );
    }
}
