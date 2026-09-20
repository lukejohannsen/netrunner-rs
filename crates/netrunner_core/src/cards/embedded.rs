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
/// These fields — faction, keywords, influence, deck limit, artist, set,
/// the printed text and flavour —
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
        card.influence_limit = entry.influence_limit;
        card.artist.clone_from(&entry.artist);
        card.printed_text.clone_from(&entry.printed_text);
        card.flavor.clone_from(&entry.flavor);
        card.image_url.clone_from(&entry.image_url);
        card.unique = entry.unique;
        card.base_link = entry.base_link;
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

    /// Every playable card must carry its catalog join key, because that key
    /// is the *only* source of faction, influence cost, deck limit and set
    /// code — everything deckbuilding legality is computed from. A card
    /// without one is playable but not deckbuildable: it silently counts as
    /// neutral, 0 influence, no set, so `deck::validator` would wave through
    /// a deck it should reject.
    ///
    /// Nineteen baseline Core Set cards were in exactly that state until
    /// their metadata was backfilled. This is what stops the next
    /// hand-authored card from reintroducing the hole.
    #[test]
    fn every_playable_card_carries_a_numeric_id() {
        let missing: Vec<String> = embedded_playable_cards()
            .into_iter()
            .filter(|card| card.numeric_id.is_none())
            .map(|card| format!("{} ({})", card.title, card.id.0))
            .collect();

        assert!(
            missing.is_empty(),
            "these playable cards have no NetrunnerDB join key, so they carry no printed \
             metadata and cannot be deckbuilt against: {missing:#?}"
        );
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
/// place to silence it. Empty since the two starter identities landed for
/// *Learn to Play* (ROADMAP Phase 1.75 §2): 77 of 77.
#[cfg(test)]
const SG_UNIMPLEMENTED: &[(u32, &str)] = &[];

/// *Elevation* cards with no DSL implementation yet — the same gate as
/// `SG_UNIMPLEMENTED`, for the set being implemented deck by deck (ROADMAP
/// Phase 1 §8). Each entry names the stage and the published decklist(s)
/// that first need the card, and **the list only ever shrinks**: a card
/// file cannot land without deleting its entry (the count assertion fails
/// the other way), and the set cannot be called complete while an entry
/// remains. Started at 73 of 82 when Stage 1 (Flow and Ebb, Sabbatical)
/// landed its nine; 64 after Stage 2 (Enthusiasm, Tickets, please); 56 after Stage 3 (Bowel Movements, Dashing Mad); 48 after Stage 4 (Prick Thyself, Shootin' 'n' Lootin', Professional Opportunities); 40 after Stage 5 (Brick Stack, the first Corp deck); 31 after Stage 6 (Brutal Efficiency, Agency); 21 after Stage 7 (Fashion Lab, Pork Chops); 12 after Stage 8 (Quick Returns, Glyph of Warding); 8 after Stage 9 (Hidden Funds, Peculiarity); **empty after Stage 10 (Fine Print, Gimbatul, Not so subtle), which completes the set**.
#[cfg(test)]
const ELEV_UNIMPLEMENTED: &[(u32, &str)] = &[
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

            let checks: [(&str, Option<i64>, Option<i64>); 7] = [
                ("cost", Some(i64::from(entry.cost)), Some(i64::from(card.cost))),
                ("base_link", entry.base_link.map(i64::from), card.base_link.map(i64::from)),
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
    /// Uniqueness is joined from the catalog, never authored: the eleven ◆
    /// System Gateway cards must come out flagged and nothing else may.
    #[test]
    fn system_gateway_unique_cards_are_flagged_from_the_catalog() {
        let expected = [
            "carnivore",
            "cookbook",
            "docklands_pass",
            "pennyshaver",
            "pantograph",
            "verbal_plasticity",
            "manegarm_skunkworks",
            "anoetic_void",
            "spin_doctor",
            "amaze_amusements",
            "malapert_data_vault",
        ];
        let cards = embedded_playable_cards();
        let mut flagged: Vec<&str> =
            cards.iter().filter(|c| c.unique && c.set_code.as_deref() == Some("sg")).map(|c| c.id.0.as_str()).collect();
        flagged.sort_unstable();
        let mut expected: Vec<&str> = expected.to_vec();
        expected.sort_unstable();
        assert_eq!(flagged, expected);
    }

    /// The completeness gate for one set: every printed card is either
    /// implemented or carries a stated exception, and the implemented
    /// count is exactly the printed count minus the exceptions — so an
    /// exception left behind after its card landed fails too. One helper
    /// for every set, because "the gate for calling any future set
    /// complete" (ROADMAP Phase 1 §7) has to be the same gate.
    fn assert_set_accounted_for(
        set_code: &str,
        set_name: &str,
        printed: usize,
        codes: std::ops::RangeInclusive<u32>,
        exceptions: &[(u32, &str)],
    ) {
        let catalog = crate::cards::load_embedded_netrunnerdb_sets().expect("catalog should parse");
        let implemented: std::collections::HashSet<_> =
            embedded_playable_cards().iter().filter_map(|card| card.numeric_id).collect();
        let excluded: std::collections::HashSet<u32> = exceptions.iter().map(|(code, _)| *code).collect();

        let mut unaccounted: Vec<String> = Vec::new();
        let mut total = 0;
        for entry in catalog.iter() {
            if entry.set_code.as_deref() != Some(set_code) {
                continue;
            }
            total += 1;
            let Some(numeric_id) = entry.numeric_id else { continue };
            if !implemented.contains(&numeric_id) && !excluded.contains(&numeric_id.0) {
                unaccounted.push(format!("{} ({})", entry.title, numeric_id.0));
            }
        }

        assert!(unaccounted.is_empty(), "{set_name} cards with neither an implementation nor an exception entry: {unaccounted:#?}");
        assert_eq!(total, printed, "{set_name} should have {printed} printed cards");
        let stale: Vec<&str> = exceptions
            .iter()
            .filter(|(code, _)| implemented.contains(&crate::card::CardId(*code)))
            .map(|(_, reason)| *reason)
            .collect();
        assert!(stale.is_empty(), "{set_name} exception entries whose card is now implemented: {stale:#?}");
        assert_eq!(
            total - exceptions.len(),
            implemented.iter().filter(|id| codes.contains(&id.0)).count(),
            "{set_name}: implemented-card count should be the printed set minus the documented exceptions"
        );
    }

    #[test]
    fn every_system_gateway_card_is_implemented_or_explicitly_excluded() {
        assert_set_accounted_for("sg", "System Gateway", 77, 30001..=30077, SG_UNIMPLEMENTED);
    }

    /// The same gate for *Elevation*, whose exception list shrinks one
    /// stage at a time — see `ELEV_UNIMPLEMENTED`.
    #[test]
    fn every_elevation_card_is_implemented_or_explicitly_excluded() {
        assert_set_accounted_for("elev", "Elevation", 82, 35001..=35082, ELEV_UNIMPLEMENTED);
    }

    /// Card files no longer restate what the catalog owns; the join is what
    /// puts it back. If the join regressed, every card would silently lose its
    /// faction/influence/artist and deckbuilding legality checks would go quiet.
    /// Cards whose choice texts are not quotes of the printed text, each
    /// with the reason: the words the choice needs are not on the card.
    const CLAUSE_QUOTE_EXEMPT: &[(&str, &str)] = &[
        // "choose a card type" — the four types are the choice and the
        // card never prints their names.
        ("touch_ups", "the options are card-type names the card does not print"),
    ];

    /// Every linked clause is a quote of the card. The point of linking a
    /// DSL node to the printed text it implements is that a person can
    /// check the one against the other; a clause that is not on the card
    /// is a second rendering, which is what the link exists to avoid —
    /// and a clause that *was* on the card until an erratum or a catalog
    /// update changed the wording is the drift this catches. Compared
    /// with case, whitespace and punctuation dropped, so `[credit]` and
    /// "1[credit]." match however the quote was typed.
    ///
    /// **Every sample-deck card that presents a choice, offers a paid
    /// choice or has a paid ability must carry the clause.** Triggers may
    /// (`TriggeredEffect::text`) and are not gated. A clause may end in
    /// ` — <note>` to tell two options apart that one printed phrase
    /// covers ("install 1 card from HQ or Archives — from Archives"); only
    /// the part before the dash has to be the quote.
    #[test]
    fn printed_clauses_are_quoted_from_the_card() {
        use crate::dsl::Effect;

        fn normalise(text: &str) -> String {
            text.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
        }
        fn walk<'a>(effect: &'a Effect, out: &mut Vec<&'a Effect>) {
            out.push(effect);
            match effect {
                Effect::Sequence(steps) => steps.iter().for_each(|e| walk(e, out)),
                Effect::PresentChoice { options, .. } | Effect::ResolveSomeOf { options, .. } => {
                    options.iter().for_each(|e| walk(e, out))
                }
                Effect::OfferPaidChoice { if_paid, if_declined, .. } => {
                    walk(if_paid, out);
                    walk(if_declined, out);
                }
                Effect::EffectIf { effect, .. }
                | Effect::Trace { on_success: effect, .. }
                | Effect::SetAccessReplacement { effect, .. }
                | Effect::SetRunEndedEffect(effect) => walk(effect, out),
                Effect::PromptChooseCards { then: Some(then), .. } => walk(then, out),
                Effect::PromptChooseServer { on_success, on_start, .. } => {
                    on_success.iter().for_each(|e| walk(e, out));
                    on_start.iter().for_each(|e| walk(e, out));
                }
                _ => {}
            }
        }

        let mut registry = crate::cards::CardRegistry::new();
        crate::cards::register_playable_cards(&mut registry);
        let mut pool: Vec<crate::dsl::CardId> = Vec::new();
        for deck in crate::decks::embedded_decks() {
            pool.push(deck.identity.clone());
            pool.extend(deck.cards.iter().map(|entry| entry.card.clone()));
        }
        pool.sort();
        pool.dedup();

        let mut failures: Vec<String> = Vec::new();
        let mut checked = 0;
        for id in &pool {
            let card = registry.get(id).unwrap_or_else(|| panic!("{} is in a sample deck", id.0));
            let printed = normalise(card.printed_text.as_deref().unwrap_or_default());
            let exempt = CLAUSE_QUOTE_EXEMPT.iter().any(|(exempt, _)| *exempt == id.0);
            // `Some(failure)` when the clause is not a quote of the card.
            let quote = |what: &str, clause: &str| -> Option<String> {
                let quoted = clause.rsplit_once(" — ").map_or(clause, |(quote, _note)| quote);
                (!exempt && !printed.contains(&normalise(quoted)))
                    .then(|| format!("{} — {what}: {clause:?} is not on the card", card.title))
            };
            for (index, ability) in card.abilities.iter().enumerate() {
                match &ability.text {
                    Some(text) => {
                        checked += 1;
                        failures.extend(quote(&format!("ability {index}"), text));
                    }
                    None => failures.push(format!("{} — ability {index} has no printed clause", card.title)),
                }
            }
            for trigger in &card.triggers {
                if let Some(text) = &trigger.text {
                    checked += 1;
                    failures.extend(quote("trigger", text));
                }
            }
            // Nobody is asked to choose a continuous effect, so no prompt
            // needs its clause — but it is the only part of a card an
            // erratum can change without a trigger or an ability failing
            // here, so the gate asks for it anyway.
            for (index, effect) in card.continuous.iter().enumerate() {
                match &effect.text {
                    Some(text) => {
                        checked += 1;
                        failures.extend(quote(&format!("continuous effect {index}"), text));
                    }
                    None => failures.push(format!("{} — continuous effect {index} has no printed clause", card.title)),
                }
            }
            let mut effects = Vec::new();
            card.triggers.iter().flat_map(|t| t.effects.iter()).for_each(|e| walk(e, &mut effects));
            card.abilities.iter().for_each(|a| walk(&a.effect, &mut effects));
            card.subroutines.iter().for_each(|s| walk(&s.effect, &mut effects));
            for effect in effects {
                match effect {
                    Effect::PresentChoice { options, texts, .. } | Effect::ResolveSomeOf { options, texts, .. } => {
                        if texts.len() != options.len() {
                            failures.push(format!("{} — a choice has {} options and {} clauses", card.title, options.len(), texts.len()));
                            continue;
                        }
                        for (option, text) in options.iter().zip(texts) {
                            if text.is_empty() {
                                if !matches!(option, Effect::Sequence(steps) if steps.is_empty()) {
                                    failures.push(format!("{} — an empty clause on an option that does something", card.title));
                                }
                                continue;
                            }
                            checked += 1;
                            failures.extend(quote("choice option", text));
                        }
                    }
                    Effect::OfferPaidChoice { text, .. } => match text {
                        Some(text) => {
                            checked += 1;
                            failures.extend(quote("paid choice", text));
                        }
                        None => failures.push(format!("{} — a paid choice has no printed clause", card.title)),
                    },
                    _ => {}
                }
            }
        }
        assert!(failures.is_empty(), "{} clause(s) are not quotes of their card:\n  {}", failures.len(), failures.join("\n  "));
        assert!(checked > 100, "{checked} clauses checked");
    }

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
        // The printed text rides the same join: a client shows it, the
        // engine never reads it.
        let text = tithe.printed_text.as_deref().expect("the catalog carries Tithe's text");
        assert_eq!(text, "[subroutine] Do 1 net damage.\n[subroutine] Gain 1[credit].");
    }
}
