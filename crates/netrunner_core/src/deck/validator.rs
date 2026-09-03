//! Validates a `Decklist` against a `CardRegistry` for a given `NsgFormat` —
//! deckbuilding-time legality (structure, influence, format pool, banlist),
//! not gameplay-executability (see `deck` module doc comment for how this
//! differs from `rules::deck::validate_deck`).

use thiserror::Error;

use crate::card::{CardId, Faction};
use crate::cards::CardRegistry;
use crate::deck::Decklist;
use crate::dsl::{CardDefinition, CardType};
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

    #[error("card {card:?} is banned in {format:?}")]
    BannedCardIncluded { card: CardId, format: NsgFormat },

    #[error("card {card:?}'s set {set_code:?} is not legal in {format:?}")]
    PackNotLegal { card: CardId, set_code: String, format: NsgFormat },
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
    let rules = format.rules();

    let identity =
        registry.get_by_numeric_id(deck.identity).ok_or(DeckValidationError::IdentityNotFound(deck.identity))?;
    if identity.card_type != CardType::Identity {
        return Err(DeckValidationError::NotAnIdentity(deck.identity));
    }
    check_format_legality(deck.identity, identity, format, &rules)?;

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

    let mut influence_spent = 0u32;
    let mut agenda_points = 0u32;

    for (&card_id, &count) in &deck.cards {
        let card = registry.get_by_numeric_id(card_id).ok_or(DeckValidationError::CardNotFound(card_id))?;

        if card.side != identity.side {
            return Err(DeckValidationError::FactionMismatch {
                card: card_id,
                expected: identity.side,
                actual: card.side,
            });
        }
        if identity.side == Side::Runner && card.card_type == CardType::Agenda {
            return Err(DeckValidationError::RunnerDeckContainsAgenda(card_id));
        }

        check_format_legality(card_id, card, format, &rules)?;

        let max_copies =
            if rules.restricted.contains(&card_id) { 1 } else { card.deck_limit.unwrap_or(MAX_COPIES_PER_CARD) };
        if count > max_copies {
            return Err(DeckValidationError::TooManyCopies { card: card_id, count, max: max_copies });
        }

        let card_faction = card.faction.unwrap_or(Faction::NeutralCorp);
        if card_faction != identity_faction && !is_neutral(card_faction) {
            influence_spent += card.influence_cost.unwrap_or(0) * count;
        }
        if card.card_type == CardType::Agenda {
            agenda_points += card.agenda_points.unwrap_or(0) * count;
        }
    }

    // The starter identities have no influence budget (`unlimited_influence`,
    // the catalog's `influence_limit: null`); every other identity gets its
    // printed budget, or the flat default when it prints none.
    let limit = identity.influence_limit.unwrap_or(DEFAULT_INFLUENCE_LIMIT);
    if !identity.unlimited_influence && influence_spent > limit {
        return Err(DeckValidationError::InfluenceExceeded { spent: influence_spent, limit });
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

fn check_format_legality(
    card_id: CardId,
    card: &CardDefinition,
    format: NsgFormat,
    rules: &FormatRules,
) -> Result<(), DeckValidationError> {
    if rules.banned.contains(&card_id) {
        return Err(DeckValidationError::BannedCardIncluded { card: card_id, format });
    }
    let set_code = card.set_code.as_deref().unwrap_or("");
    if let Some(allowed) = &rules.allowed_packs
        && !allowed.contains(set_code)
    {
        return Err(DeckValidationError::PackNotLegal { card: card_id, set_code: set_code.to_string(), format });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn identity(id: u32, side: Side, faction: Faction, min_deck_size: u32, set_code: &str) -> CardDefinition {
        CardDefinition {
            numeric_id: Some(CardId(id)),
            faction: Some(faction),
            set_code: Some(set_code.to_string()),
            min_deck_size: Some(min_deck_size),
            ..crate::cards::common::base_card(&format!("identity_{id}"), &format!("identity_{id}"), side, CardType::Identity, 0)
        }
    }

    fn card(
        id: u32,
        side: Side,
        faction: Faction,
        card_type: CardType,
        influence_cost: Option<u32>,
        set_code: &str,
    ) -> CardDefinition {
        CardDefinition {
            numeric_id: Some(CardId(id)),
            faction: Some(faction),
            set_code: Some(set_code.to_string()),
            influence_cost,
            ..crate::cards::common::base_card(&format!("card_{id}"), &format!("card_{id}"), side, card_type, 1)
        }
    }

    fn agenda(id: u32, points: u32, set_code: &str) -> CardDefinition {
        let mut c = card(id, Side::Corp, Faction::NeutralCorp, CardType::Agenda, None, set_code);
        c.agenda_points = Some(points);
        c
    }

    /// Registers `total` non-agenda, in-faction Corp filler cards (0
    /// influence), split across as many distinct ids as needed to respect
    /// `MAX_COPIES_PER_CARD`, and returns matching `Decklist.cards` entries.
    fn corp_filler(registry: &mut CardRegistry, faction: Faction, start_id: u32, total: u32, set_code: &str) -> HashMap<CardId, u32> {
        filler(registry, Side::Corp, faction, CardType::Asset, start_id, total, set_code)
    }

    fn runner_filler(registry: &mut CardRegistry, faction: Faction, start_id: u32, total: u32, set_code: &str) -> HashMap<CardId, u32> {
        filler(registry, Side::Runner, faction, CardType::Event, start_id, total, set_code)
    }

    fn filler(
        registry: &mut CardRegistry,
        side: Side,
        faction: Faction,
        card_type: CardType,
        start_id: u32,
        total: u32,
        set_code: &str,
    ) -> HashMap<CardId, u32> {
        let mut cards = HashMap::new();
        let mut remaining = total;
        let mut id = start_id;
        while remaining > 0 {
            let copies = remaining.min(MAX_COPIES_PER_CARD);
            registry.insert(card(id, side, faction, card_type.clone(), None, set_code));
            cards.insert(CardId(id), copies);
            remaining -= copies;
            id += 1;
        }
        cards
    }

    /// A minimal, legal 45-card Corp deck: 4 distinct 5-point Agendas (20
    /// points, within [20,22]) plus 41 in-faction filler cards.
    fn valid_corp_registry_and_deck() -> (CardRegistry, Decklist) {
        let mut registry = CardRegistry::new();
        registry.insert(identity(1, Side::Corp, Faction::WeylandConsortium, 45, "sg"));

        let mut cards = HashMap::new();
        for i in 0..4 {
            registry.insert(agenda(100 + i, 5, "sg"));
            cards.insert(CardId(100 + i), 1);
        }
        cards.extend(corp_filler(&mut registry, Faction::WeylandConsortium, 200, 41, "sg"));

        (registry, Decklist { identity: CardId(1), cards })
    }

    fn valid_runner_registry_and_deck() -> (CardRegistry, Decklist) {
        let mut registry = CardRegistry::new();
        registry.insert(identity(2, Side::Runner, Faction::Criminal, 45, "sg"));
        let cards = runner_filler(&mut registry, Faction::Criminal, 300, 45, "sg");

        (registry, Decklist { identity: CardId(2), cards })
    }

    #[test]
    fn valid_corp_deck_passes_for_startup_and_standard() {
        let (registry, deck) = valid_corp_registry_and_deck();
        for format in [NsgFormat::Startup, NsgFormat::Standard] {
            let report = validate_deck(&deck, &registry, format).expect("well-formed deck should validate");
            assert_eq!(report.deck_size, 45);
            assert_eq!(report.agenda_points, Some(20));
            assert_eq!(report.influence_spent, 0);
        }
    }

    #[test]
    fn valid_runner_deck_passes_for_startup_and_standard() {
        let (registry, deck) = valid_runner_registry_and_deck();
        for format in [NsgFormat::Startup, NsgFormat::Standard] {
            let report = validate_deck(&deck, &registry, format).expect("well-formed deck should validate");
            assert_eq!(report.deck_size, 45);
            assert_eq!(report.agenda_points, None);
        }
    }

    #[test]
    fn deck_size_too_small_is_rejected() {
        let mut registry = CardRegistry::new();
        registry.insert(identity(1, Side::Corp, Faction::WeylandConsortium, 45, "sg"));
        let cards = corp_filler(&mut registry, Faction::WeylandConsortium, 200, 10, "sg");
        let deck = Decklist { identity: CardId(1), cards };

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Standard),
            Err(DeckValidationError::DeckSizeTooSmall { size: 10, minimum: 45 })
        );
    }

    #[test]
    fn insufficient_agenda_points_is_rejected() {
        let mut registry = CardRegistry::new();
        registry.insert(identity(1, Side::Corp, Faction::WeylandConsortium, 45, "sg"));
        let mut cards = HashMap::new();
        registry.insert(agenda(100, 5, "sg"));
        registry.insert(agenda(101, 5, "sg"));
        cards.insert(CardId(100), 1);
        cards.insert(CardId(101), 1); // 10 points total, below [20, 22]
        cards.extend(corp_filler(&mut registry, Faction::WeylandConsortium, 200, 43, "sg"));
        let deck = Decklist { identity: CardId(1), cards };

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Standard),
            Err(DeckValidationError::InsufficientAgendaPoints { points: 10, min: 20, max: 22, size: 45 })
        );
    }

    #[test]
    fn influence_exceeded_is_rejected() {
        let mut registry = CardRegistry::new();
        registry.insert(identity(2, Side::Runner, Faction::Criminal, 45, "sg"));
        let mut cards = runner_filler(&mut registry, Faction::Criminal, 300, 39, "sg");
        // 3 copies of a 4-influence off-faction (Anarch) card (12) plus 2
        // copies of a second 4-influence off-faction (Shaper) card (8) — 20
        // total, over the 15 budget on its own.
        registry.insert(card(400, Side::Runner, Faction::Anarch, CardType::Program, Some(4), "sg"));
        registry.insert(card(401, Side::Runner, Faction::Shaper, CardType::Program, Some(4), "sg"));
        cards.insert(CardId(400), 3); // 12 influence
        cards.insert(CardId(401), 2); // 8 influence -> 20 total, over the 15 budget
        cards.insert(CardId(402), 1);
        registry.insert(card(402, Side::Runner, Faction::Criminal, CardType::Program, None, "sg"));
        let deck = Decklist { identity: CardId(2), cards };

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Standard),
            Err(DeckValidationError::InfluenceExceeded { spent: 20, limit: 15 })
        );
    }

    #[test]
    fn banned_card_included_is_rejected() {
        let mut registry = CardRegistry::new();
        registry.insert(card(500, Side::Runner, Faction::Criminal, CardType::Program, None, "sg"));

        // A format whose rules ban card 500 — built directly rather than via
        // `NsgFormat::rules()`, since no real format's hardcoded banlist
        // includes a synthetic test id.
        let rules = FormatRules { banned: std::collections::HashSet::from([CardId(500)]), ..Default::default() };
        let card_500 = registry.get_by_numeric_id(CardId(500)).unwrap();
        assert_eq!(
            check_format_legality(CardId(500), card_500, NsgFormat::Standard, &rules),
            Err(DeckValidationError::BannedCardIncluded { card: CardId(500), format: NsgFormat::Standard })
        );
    }

    #[test]
    fn faction_mismatch_is_rejected_when_a_card_side_differs_from_the_identity() {
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        registry.insert(card(600, Side::Runner, Faction::Criminal, CardType::Program, None, "sg"));
        deck.cards.insert(CardId(600), 1);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Standard),
            Err(DeckValidationError::FactionMismatch { card: CardId(600), expected: Side::Corp, actual: Side::Runner })
        );
    }

    #[test]
    fn pack_not_legal_in_startup_but_legal_in_standard() {
        let mut registry = CardRegistry::new();
        registry.insert(identity(2, Side::Runner, Faction::Criminal, 45, "sg"));
        let mut cards = runner_filler(&mut registry, Faction::Criminal, 300, 44, "sg");
        registry.insert(card(700, Side::Runner, Faction::Criminal, CardType::Program, None, "future-pack"));
        cards.insert(CardId(700), 1);
        let deck = Decklist { identity: CardId(2), cards };

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Startup),
            Err(DeckValidationError::PackNotLegal {
                card: CardId(700),
                set_code: "future-pack".to_string(),
                format: NsgFormat::Startup,
            })
        );
        assert!(validate_deck(&deck, &registry, NsgFormat::Standard).is_ok());
    }

    #[test]
    fn too_many_copies_is_rejected() {
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        registry.insert(card(999, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None, "sg"));
        deck.cards.insert(CardId(999), 4);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Standard),
            Err(DeckValidationError::TooManyCopies { card: CardId(999), count: 4, max: 3 })
        );
    }

    #[test]
    fn a_card_specific_deck_limit_overrides_the_flat_copy_limit() {
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        let mut restricted_card = card(998, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None, "sg");
        restricted_card.deck_limit = Some(1);
        registry.insert(restricted_card);
        deck.cards.insert(CardId(998), 2);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Standard),
            Err(DeckValidationError::TooManyCopies { card: CardId(998), count: 2, max: 1 })
        );
    }

    #[test]
    fn identity_not_found_and_not_an_identity_are_rejected() {
        let registry = CardRegistry::new();
        let deck = Decklist { identity: CardId(9999), cards: HashMap::new() };
        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Standard),
            Err(DeckValidationError::IdentityNotFound(CardId(9999)))
        );

        let mut registry = CardRegistry::new();
        registry.insert(card(1, Side::Corp, Faction::WeylandConsortium, CardType::Asset, None, "sg"));
        let deck = Decklist { identity: CardId(1), cards: HashMap::new() };
        assert_eq!(validate_deck(&deck, &registry, NsgFormat::Standard), Err(DeckValidationError::NotAnIdentity(CardId(1))));
    }

    #[test]
    fn runner_deck_containing_an_agenda_is_rejected() {
        let (mut registry, mut deck) = valid_runner_registry_and_deck();
        // A mislabeled card: `CardType::Agenda` but tagged Runner-side, to
        // isolate this check from the more general side-mismatch check
        // (real Agendas are always Corp-side, which would trip
        // `FactionMismatch` first).
        let mut mislabeled = agenda(800, 3, "sg");
        mislabeled.side = Side::Runner;
        registry.insert(mislabeled);
        deck.cards.insert(CardId(800), 1);

        assert_eq!(
            validate_deck(&deck, &registry, NsgFormat::Standard),
            Err(DeckValidationError::RunnerDeckContainsAgenda(CardId(800)))
        );
    }
}
