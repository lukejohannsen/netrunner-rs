//! The hand-authored, gameplay-complete card sets, embedded at compile time.
//!
//! `build.rs` concatenates `data/corp/*.json` and `data/runner/*.json` (one
//! file per card, for readable diffs) into one array per side under
//! `OUT_DIR`; `include_str!` then bakes those into the binary. That keeps
//! this crate I/O-free at runtime while still making every playable card
//! reachable from the *default* build — no feature flag, no filesystem.
//!
//! This is the single source of truth for playable cards. The NetrunnerDB
//! dumps in `data/cards` (see `cards::netrunnerdb`) are a separate,
//! catalog-only pool: metadata for every printed card, `is_playable: false`,
//! no DSL rules. `cards::loader` (feature `fs-loader`) is a third, optional
//! path for *external* card directories, not for these sets.

use crate::cards::CardRegistry;
use crate::dsl::CardDefinition;

const CORP_CARDS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/corp_cards.json"));
const RUNNER_CARDS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/runner_cards.json"));

/// Parses one side's embedded array. A failure here is an authoring bug in a
/// checked-in card file that got past the test suite, not a runtime
/// condition a caller could recover from — so it panics rather than
/// returning a `Result` that every consumer would have to `unwrap` anyway.
fn parse_side(json: &str, side: &str) -> Vec<CardDefinition> {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("embedded {side} card data failed to parse: {e}"))
}

/// Fills each card's printed metadata from the NetrunnerDB catalog, joined on
/// `numeric_id`.
///
/// These fields — faction, keywords, influence, deck limit, artist, set —
/// are NetrunnerDB's to state, so card files don't restate them: doing so
/// invited silent drift (a corrected influence cost upstream, a mistyped
/// artist) between two copies of the same fact. Card files own the join key
/// and everything the rules engine actually runs on.
///
/// A card with no `numeric_id`, or one the catalog doesn't carry, simply
/// keeps whatever it declared — homebrew and test fixtures are not required
/// to exist upstream.
fn fill_catalog_metadata(cards: &mut [CardDefinition]) {
    let catalog =
        crate::cards::load_embedded_netrunnerdb_sets().expect("embedded NetrunnerDB catalog should parse");

    for card in cards {
        let Some(numeric_id) = card.numeric_id else { continue };
        let Some(entry) = catalog.get_by_numeric_id(numeric_id) else { continue };

        card.faction = entry.faction;
        card.type_line.clone_from(&entry.type_line);
        card.keywords.clone_from(&entry.keywords);
        card.set_code.clone_from(&entry.set_code);
        card.influence_cost = entry.influence_cost;
        card.deck_limit = entry.deck_limit;
        card.artist.clone_from(&entry.artist);
        card.image_url.clone_from(&entry.image_url);
    }
}

/// Every hand-authored playable card, as a flat list, with printed metadata
/// filled in from the catalog.
pub fn embedded_playable_cards() -> Vec<CardDefinition> {
    let mut cards = parse_side(CORP_CARDS_JSON, "Corp");
    cards.extend(parse_side(RUNNER_CARDS_JSON, "Runner"));
    fill_catalog_metadata(&mut cards);
    cards
}

/// Registers every hand-authored playable card into `registry`.
pub fn register_embedded_cards(registry: &mut CardRegistry) {
    for card in embedded_playable_cards() {
        registry.insert(card);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every embedded card must parse and pass the same semantic validation
    /// the filesystem loader applies — so a malformed card file fails
    /// `cargo test` here rather than panicking in a consumer's binary.
    #[test]
    fn every_embedded_card_parses_and_validates() {
        for card in embedded_playable_cards() {
            card.validate().unwrap_or_else(|e| panic!("{:?} failed validation: {e}", card.id));
        }
    }

    /// Embedded cards are the playable pool; anything else is a bug in a
    /// card file, since `rules::deck::validate_deck` rejects unplayable cards.
    #[test]
    fn every_embedded_card_is_playable() {
        for card in embedded_playable_cards() {
            assert!(card.is_playable, "{:?} is embedded but not marked playable", card.id);
        }
    }

    /// A misspelled key used to deserialize silently, leaving the intended
    /// field at its default — e.g. a typo'd `strength_modifier` would cost a
    /// card its bonus with no test failure. `deny_unknown_fields` makes that
    /// a hard error; this proves the guard is actually wired up.
    #[test]
    fn a_misspelled_field_is_rejected_rather_than_silently_defaulted() {
        let typo = r#"{"id":"x","title":"X","side":"Corp","card_type":"Operation","cost":1,
                       "triggers":[],"strenght_modifier":null}"#;

        let err = serde_json::from_str::<CardDefinition>(typo).expect_err("a misspelled key must not parse");

        assert!(err.to_string().contains("strenght_modifier"), "error should name the offending key: {err}");
    }
}


/// System Gateway cards with no DSL implementation yet, keyed by NetrunnerDB
/// code (titles carry typographic apostrophes, codes don't). Every entry
/// needs a stated reason — this list is the deliberate exception set for
/// `every_system_gateway_card_is_implemented_or_explicitly_excluded`, not a
/// place to silence it.
#[cfg(test)]
const SG_UNIMPLEMENTED: &[(u32, &str)] = &[
    (30076, "The Catalyst: Convention Breaker — \"Starter game only.\", no rules text to implement; permanent"),
    (30077, "The Syndicate: Profit over Principle — \"Starter game only.\", no rules text to implement; permanent"),
];

#[cfg(test)]
mod catalog_join_tests {
    use super::*;

    /// Printed values live in the card file (they drive the rules engine, and
    /// homebrew cards have no catalog entry) while NetrunnerDB remains the
    /// authority on what was actually printed. This catches drift between the
    /// two in either direction — a mistyped cost here, a corrected value
    /// upstream.
    ///
    /// Only compares where the catalog states a value: a marker like
    /// `advancement_requirement: 0` on an advanceable non-agenda (Clearinghouse,
    /// Urtica Cipher, Pharos) has no upstream counterpart, and identities have
    /// no printed cost.
    #[test]
    fn printed_values_agree_with_the_netrunnerdb_catalog() {
        let catalog = crate::cards::load_embedded_netrunnerdb_sets().expect("catalog should parse");

        for card in embedded_playable_cards() {
            let Some(numeric_id) = card.numeric_id else { continue };
            let Some(entry) = catalog.get_by_numeric_id(numeric_id) else { continue };

            let checks: [(&str, Option<i64>, Option<i64>); 6] = [
                ("cost", Some(i64::from(entry.cost)), Some(i64::from(card.cost))),
                ("strength", entry.strength.map(i64::from), card.strength.map(i64::from)),
                ("agenda_points", entry.agenda_points.map(i64::from), card.agenda_points.map(i64::from)),
                (
                    "advancement_requirement",
                    entry.advancement_requirement.map(i64::from),
                    card.advancement_requirement.map(i64::from),
                ),
                ("trash_cost", entry.trash_cost.map(i64::from), card.trash_cost.map(i64::from)),
                ("memory_cost", entry.memory_cost.map(i64::from), card.memory_cost.map(i64::from)),
            ];

            for (field, upstream, ours) in checks {
                let Some(upstream) = upstream else { continue };
                assert_eq!(
                    ours,
                    Some(upstream),
                    "{} ({}): {field} is {ours:?} but NetrunnerDB prints {upstream}",
                    card.title,
                    numeric_id.0
                );
            }
        }
    }

    /// The gate for calling a set done: every printed System Gateway card is
    /// either implemented or listed in `SG_UNIMPLEMENTED` with a reason.
    ///
    /// Seven milestones of card work tracked coverage by reading the catalog
    /// by eye, which quietly missed seven cards. This does it mechanically.
    #[test]
    fn every_system_gateway_card_is_implemented_or_explicitly_excluded() {
        let catalog = crate::cards::load_embedded_netrunnerdb_sets().expect("catalog should parse");
        let implemented: std::collections::HashSet<_> =
            embedded_playable_cards().iter().filter_map(|card| card.numeric_id).collect();
        let excluded: std::collections::HashSet<u32> = SG_UNIMPLEMENTED.iter().map(|(code, _)| *code).collect();

        let mut unaccounted: Vec<String> = Vec::new();
        let mut sg_total = 0;
        for entry in catalog.iter() {
            if entry.set_code.as_deref() != Some("sg") {
                continue;
            }
            sg_total += 1;
            let Some(numeric_id) = entry.numeric_id else { continue };
            if !implemented.contains(&numeric_id) && !excluded.contains(&numeric_id.0) {
                unaccounted.push(format!("{} ({})", entry.title, numeric_id.0));
            }
        }

        assert!(
            unaccounted.is_empty(),
            "System Gateway cards with neither an implementation nor an SG_UNIMPLEMENTED entry: {unaccounted:#?}"
        );
        assert_eq!(sg_total, 77, "System Gateway should have 77 printed cards");
        assert_eq!(
            sg_total - SG_UNIMPLEMENTED.len(),
            implemented.iter().filter(|id| (30001..=30077).contains(&id.0)).count(),
            "implemented-card count should be the printed set minus the documented exceptions"
        );
    }

    /// Card files no longer restate what the catalog owns; the join is what
    /// puts it back. If the join regressed, every card would silently lose its
    /// faction/influence/artist and deckbuilding legality checks would go quiet.
    #[test]
    fn catalog_metadata_is_filled_in_from_the_join() {
        let tithe = embedded_playable_cards()
            .into_iter()
            .find(|card| card.id.0 == "tithe")
            .expect("tithe should be embedded");

        assert_eq!(tithe.set_code.as_deref(), Some("sg"));
        assert_eq!(tithe.faction, Some(crate::card::Faction::NeutralCorp));
        assert_eq!(tithe.influence_cost, Some(0));
        assert_eq!(tithe.deck_limit, Some(3));
        assert_eq!(tithe.artist.as_deref(), Some("Scott Uminga"));
        assert!(tithe.keywords.iter().any(|k| k == "Sentry"), "keywords should come from the catalog");
    }
}
