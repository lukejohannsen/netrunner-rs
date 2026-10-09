//! The hand-authored, gameplay-complete card sets, embedded at compile time.
//!
//! `build.rs` concatenates `data/corp/*.json` and `data/runner/*.json` (one
//! file per card, for readable diffs) into one array per side under
//! `OUT_DIR`; `include_str!` then bakes those into the binary. That keeps
//! this crate I/O-free at runtime while still making every playable card
//! reachable from the *default* build — no feature flag, no filesystem.
//!
//! This is the single source of truth for playable cards. The NetrunnerDB
//! catalog in `data/catalog` (see `cards::catalog`) is a separate,
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
/// the card's id — NetrunnerDB's v3 slug, which a card file's `id` is
/// (`every_card_file_id_is_a_netrunnerdb_card`).
///
/// These fields — faction, keywords, influence, deck limit, uniqueness,
/// link and the printed text — are NetrunnerDB's to state, so card files
/// don't restate them: doing so invited silent drift (a corrected influence
/// cost upstream) between two copies of the same fact. Card files own the
/// id and everything the rules engine actually runs on. What belongs to a
/// printing — its set, illustrator, flavour and picture — is not copied at
/// all: it is `cards::catalog`'s, asked for the printing a client shows.
///
/// A card the catalog doesn't carry simply keeps whatever it declared —
/// homebrew and test fixtures are not required to exist upstream.
fn fill_catalog_metadata(cards: &mut [CardDefinition]) {
    let catalog = crate::cards::catalog::cards();

    for card in cards {
        let Some(entry) = catalog.get(&card.id) else { continue };

        card.faction = entry.faction;
        card.type_line.clone_from(&entry.type_line);
        card.keywords.clone_from(&entry.keywords);
        card.subtypes.clone_from(&entry.subtypes);
        card.influence_cost = entry.influence_cost;
        card.deck_limit = entry.deck_limit;
        card.influence_limit = entry.influence_limit;
        card.printed_text.clone_from(&entry.printed_text);
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

    /// `Effect::with_chosen_number` ends in `other => other`, so an effect
    /// that holds an `Amount` and is missing from it would keep the
    /// placeholder and resolve as 0. Held to the pool instead of to the
    /// match: every `then` a card file writes must come out of the
    /// substitution naming no placeholder at all.
    #[test]
    fn a_chosen_number_reaches_every_amount_a_card_writes() {
        let mut asked = 0;
        for card in embedded_playable_cards() {
            let roots = card
                .abilities
                .iter()
                .map(|ability| &ability.effect)
                .chain(card.triggers.iter().flat_map(|triggered| &triggered.effects))
                .chain(card.subroutines.iter().map(|subroutine| &subroutine.effect))
                .chain(card.interactive_on_access.iter().flat_map(|interactive| &interactive.effects))
                .chain(card.before_starting_hand.iter());
            for root in roots {
                root.for_each_effect(&mut |effect| {
                    // Daily Business Show's number is the draw it grew.
                    let then = match effect {
                        crate::dsl::Effect::ChooseNumber { then, .. } | crate::dsl::Effect::IncreaseAboutToResolve { then: Some(then), .. } => then,
                        _ => return,
                    };
                    asked += 1;
                    let written = serde_json::to_string(&then.as_ref().clone().with_chosen_number(7)).expect("an effect serializes");
                    assert!(!written.contains("ChosenNumber"), "{:?} — the number never reaches part of its `then`: {written}", card.id);
                    assert!(written.contains("{\"Fixed\":7}"), "{:?} asks for a number and never reads it: {written}", card.id);
                });
            }
        }
        assert!(asked >= 4, "{asked} cards ask for a number — Bigger Picture, Lie Low, Account Siphon and Daily Business Show do");
    }

    /// Embedded cards are the playable pool; anything else is a bug in a
    /// card file, since `rules::deck::validate_deck` rejects unplayable cards.
    #[test]
    fn every_embedded_card_is_playable() {
        for card in embedded_playable_cards() {
            assert!(card.is_playable, "{:?} is embedded but not marked playable", card.id);
        }
    }

    /// Every card file is a NetrunnerDB card, by its v3 id, because that id
    /// is the *only* join to its faction, influence cost, deck limit and
    /// printed text — everything deckbuilding legality is computed from. A
    /// card file whose id the catalog does not know is playable but not
    /// deckbuildable: it silently counts as neutral, 0 influence, so
    /// `deck::validator` would wave through a deck it should reject.
    ///
    /// Nineteen baseline Core Set cards were in exactly that state until
    /// their metadata was backfilled, and sixteen ids were not v3's until
    /// NSG pool Stage 0c renamed them. This is what stops the next one.
    ///
    /// **And every card file names the printing it was built from, which is
    /// a printing of that card**: the observation vocabulary and
    /// `SameAction::FromHand` read the number, and a code of another card
    /// would put the card in that card's slot.
    #[test]
    fn every_card_file_id_is_a_netrunnerdb_card() {
        let catalog = crate::cards::catalog::cards();
        let mut wrong: Vec<String> = Vec::new();
        for card in embedded_playable_cards() {
            if catalog.get(&card.id).is_none() {
                wrong.push(format!("{} ({}): no NetrunnerDB card has this id", card.title, card.id.0));
            }
            match card.built_from.and_then(crate::cards::catalog::printing) {
                Some(printing) if printing.card == card.id => {}
                Some(printing) => wrong.push(format!("{}: built_from {} prints {}", card.id.0, printing.id, printing.card.0)),
                None => wrong.push(format!("{}: built_from names no embedded printing ({:?})", card.id.0, card.built_from)),
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// A misspelled key used to deserialize silently, leaving the intended
    /// field at its default — e.g. a typo'd `strength` would leave an ice at
    /// 0 with no test failure. `deny_unknown_fields` makes that
    /// a hard error; this proves the guard is actually wired up.
    #[test]
    fn a_misspelled_field_is_rejected_rather_than_silently_defaulted() {
        let typo = r#"{"id":"x","title":"X","side":"Corp","card_type":"Operation","cost":1,
                       "triggers":[],"strenght":null}"#;

        let err = serde_json::from_str::<CardDefinition>(typo).expect_err("a misspelled key must not parse");

        assert!(err.to_string().contains("strenght"), "error should name the offending key: {err}");
    }
}


/// System Gateway cards with no DSL implementation yet, as (card id, title). Every entry
/// needs a stated reason — this list is the deliberate exception set for
/// `every_system_gateway_card_is_implemented_or_explicitly_excluded`, not a
/// place to silence it. Empty since the two starter identities landed for
/// *Learn to Play* (ROADMAP Phase 1.75 §2): 77 of 77.
#[cfg(test)]
const SG_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Elevation* cards with no DSL implementation yet — the same gate as
/// `SG_UNIMPLEMENTED`, for the set being implemented deck by deck (ROADMAP
/// Phase 1 §8). Each entry names the stage and the published decklist(s)
/// that first need the card, and **the list only ever shrinks**: a card
/// file cannot land without deleting its entry (the count assertion fails
/// the other way), and the set cannot be called complete while an entry
/// remains. Started at 73 of 82 when Stage 1 (Flow and Ebb, Sabbatical)
/// landed its nine; 64 after Stage 2 (Enthusiasm, Tickets, please); 56 after Stage 3 (Bowel Movements, Dashing Mad); 48 after Stage 4 (Prick Thyself, Shootin' 'n' Lootin', Professional Opportunities); 40 after Stage 5 (Brick Stack, the first Corp deck); 31 after Stage 6 (Brutal Efficiency, Agency); 21 after Stage 7 (Fashion Lab, Pork Chops); 12 after Stage 8 (Quick Returns, Glyph of Warding); 8 after Stage 9 (Hidden Funds, Peculiarity); **empty after Stage 10 (Fine Print, Gimbatul, Not so subtle), which completes the set**.
#[cfg(test)]
const ELEV_UNIMPLEMENTED: &[(&str, &str)] = &[
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
        let catalog = crate::cards::catalog::cards();

        for card in embedded_playable_cards() {
            let Some(entry) = catalog.get(&card.id) else { continue };

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
                    card.id.0
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
        let mut flagged: Vec<&str> = cards
            .iter()
            .filter(|c| c.unique && crate::cards::catalog::printed_in(&c.id, "system_gateway"))
            .map(|c| c.id.0.as_str())
            .collect();
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
    ///
    /// **A printing is built when a playable card has its card's id.** A
    /// reprint is a printing of the same card, so a set that reprints a
    /// built card — System Update 2021's Corroder — is built there too, with
    /// no title fold: the v2 catalog keyed a card file by one printing's
    /// code and had to join the others back by title.
    fn assert_set_accounted_for(
        set_id: &str,
        set_name: &str,
        printed: usize,
        exceptions: &[(&str, &str)],
    ) {
        let playable: std::collections::HashSet<crate::dsl::CardId> =
            embedded_playable_cards().into_iter().filter(|card| card.is_playable).map(|card| card.id).collect();
        let excluded: std::collections::HashSet<&str> = exceptions.iter().map(|(id, _)| *id).collect();

        let mut unaccounted: Vec<String> = Vec::new();
        let mut built: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut total = 0;
        for printing in crate::cards::catalog::printings().filter(|printing| printing.set == set_id) {
            total += 1;
            if playable.contains(&printing.card) {
                built.insert(printing.card.0.as_str());
            } else if !excluded.contains(printing.card.0.as_str()) {
                unaccounted.push(format!("{} ({})", printing.card.0, printing.id));
            }
        }

        assert!(unaccounted.is_empty(), "{set_name} cards with neither an implementation nor an exception entry: {unaccounted:#?}");
        assert_eq!(total, printed, "{set_name} should have {printed} printed cards");
        let stale: Vec<String> = exceptions
            .iter()
            .filter(|(id, _)| built.contains(id))
            .map(|(id, title)| format!("{title} ({id})"))
            .collect();
        assert!(stale.is_empty(), "{set_name} exception entries whose card is now implemented: {stale:#?}");
        assert_eq!(
            total - exceptions.len(),
            built.len(),
            "{set_name}: the built count should be the printed set minus the documented exceptions \
             (an exception naming a card outside the set breaks this too)"
        );
    }

    /// A card's subtypes are the catalog's (`fill_catalog_metadata`), so a
    /// card file with a catalog entry authors none: one it wrote would be
    /// overwritten, which is a restatement that could only disagree.
    /// Holds the 49 files that authored them before VP Stage 1 to it.
    #[test]
    fn a_card_file_leaves_its_subtypes_to_the_catalog() {
        let catalog = crate::cards::catalog::cards();
        let mut authored: Vec<String> = parse_side(CORP_CARDS_JSON, "Corp")
            .into_iter()
            .chain(parse_side(RUNNER_CARDS_JSON, "Runner"))
            .filter(|card| !card.subtypes.is_empty())
            .filter(|card| catalog.get(&card.id).is_some())
            .map(|card| card.id.0)
            .collect();
        authored.sort();
        assert!(authored.is_empty(), "card files authoring subtypes the catalog owns: {authored:?}");
        let cleaver = embedded_playable_cards().into_iter().find(|card| card.id.0 == "cleaver").expect("Cleaver");
        assert_eq!(cleaver.subtypes, vec![crate::dsl::CardSubtype::Icebreaker, crate::dsl::CardSubtype::Fracter], "read off \"Icebreaker - Fracter\"");
    }

    #[test]
    fn every_system_gateway_card_is_implemented_or_explicitly_excluded() {
        assert_set_accounted_for("system_gateway", "System Gateway", 77, SG_UNIMPLEMENTED);
    }

    /// The same gate for *Elevation*, whose exception list shrinks one
    /// stage at a time — see `ELEV_UNIMPLEMENTED`.
    #[test]
    fn every_elevation_card_is_implemented_or_explicitly_excluded() {
        assert_set_accounted_for("elevation", "Elevation", 82, ELEV_UNIMPLEMENTED);
    }

    /// "Startup is complete" as a gate rather than a sentence (Phase 5 §25
    /// Stage 0, 29 September 2026): every card in a `COMPLETE_FORMATS` pool
    /// is a playable card. The per-set gates above say a *set* is built;
    /// this one says a *format* is, which is what a player choosing Startup
    /// and a bot told the format (Stage 2) are promised — and it fails the
    /// day a rotation adds a set to the pool before its cards land. A pool
    /// is a list of cards, so a card the catalog does not carry fails here
    /// too: it was a list of printings, some of them in sets this crate does
    /// not embed, and the gate named those codes as built under another.
    ///
    /// The other half holds the list honest: a format not on it must be
    /// short of a card, so completing Standard is a one-line change here
    /// and never a claim nobody checked.
    #[test]
    fn every_card_in_a_complete_formats_pool_is_built_and_playable() {
        use crate::cards::unimplemented::COMPLETE_FORMATS;
        use crate::format::NsgFormat;

        let playable: std::collections::HashSet<crate::dsl::CardId> =
            embedded_playable_cards().into_iter().filter(|card| card.is_playable).map(|card| card.id).collect();
        let unbuilt = |format: NsgFormat| -> Vec<String> {
            let pool = format.rules().cards.as_ref().unwrap_or_else(|| panic!("{format:?} has a pool"));
            let mut unbuilt: Vec<String> = pool.iter().filter(|card| !playable.contains(*card)).map(|card| card.0.clone()).collect();
            unbuilt.sort();
            unbuilt
        };

        for format in COMPLETE_FORMATS {
            let unbuilt = unbuilt(*format);
            assert!(unbuilt.is_empty(), "{format:?} is listed complete but these pool cards are not built: {unbuilt:#?}");
        }
        assert!(unbuilt(NsgFormat::Startup).is_empty());
        for format in NsgFormat::ALL {
            if COMPLETE_FORMATS.contains(&format) || format.rules().cards.is_none() {
                continue;
            }
            assert!(!unbuilt(format).is_empty(), "{format:?} is now complete: add it to COMPLETE_FORMATS");
        }
    }

    /// The NSG card-pool plan's packs (docs/roadmap/nsg-card-pool.md), each
    /// gated from the day it was embedded, its list in `cards::unimplemented`
    /// shrinking stage by stage as §8's did for Elevation.
    #[test]
    fn every_nsg_pack_card_is_implemented_or_explicitly_excluded() {
        use crate::cards::unimplemented::*;
        for (set_id, set_name, printed, exceptions) in [
            ("vantage_point", "Vantage Point", 66, VP_UNIMPLEMENTED),
            ("rebellion_without_rehearsal", "Rebellion Without Rehearsal", 65, RWR_UNIMPLEMENTED),
            ("the_automata_initiative", "The Automata Initiative", 65, TAI_UNIMPLEMENTED),
            ("parhelion", "Parhelion", 63, PH_UNIMPLEMENTED),
            ("midnight_sun_booster_pack", "Midnight Sun Booster Pack", 7, MSBP_UNIMPLEMENTED),
            ("midnight_sun", "Midnight Sun", 65, MS_UNIMPLEMENTED),
            ("uprising_booster_pack", "Uprising Booster Pack", 7, URBP_UNIMPLEMENTED),
            ("uprising", "Uprising", 65, UR_UNIMPLEMENTED),
            ("downfall", "Downfall", 65, DF_UNIMPLEMENTED),
            ("system_update_2021", "System Update 2021", 82, SU21_UNIMPLEMENTED),
            ("salvaged_memories", "Salvaged Memories", 18, SM_UNIMPLEMENTED),
            ("magnum_opus_reprint", "Magnum Opus Reprint", 6, MOR_UNIMPLEMENTED),
            ("core_set", "Core Set", 113, CORE_UNIMPLEMENTED),
            ("revised_core_set", "Revised Core Set", 132, CORE2_UNIMPLEMENTED),
            ("reign_and_reverie", "Reign and Reverie", 58, RAR_UNIMPLEMENTED),
            ("sovereign_sight", "Sovereign Sight", 20, SS_UNIMPLEMENTED),
            ("down_the_white_nile", "Down the White Nile", 20, DTWN_UNIMPLEMENTED),
            ("council_of_the_crest", "Council of the Crest", 20, COTC_UNIMPLEMENTED),
            ("the_devil_and_the_dragon", "The Devil and the Dragon", 20, TDATD_UNIMPLEMENTED),
            ("whispers_in_nalubaale", "Whispers in Nalubaale", 20, WIN_UNIMPLEMENTED),
            ("kampala_ascendent", "Kampala Ascendent", 20, KA_UNIMPLEMENTED),
            ("daedalus_complex", "Daedalus Complex", 20, DC_UNIMPLEMENTED),
            ("station_one", "Station One", 20, SO_UNIMPLEMENTED),
            ("earths_scion", "Earth's Scion", 20, EAS_UNIMPLEMENTED),
            ("blood_and_water", "Blood and Water", 20, BAW_UNIMPLEMENTED),
            ("free_mars", "Free Mars", 20, FM_UNIMPLEMENTED),
            ("crimson_dust", "Crimson Dust", 20, CD_UNIMPLEMENTED),
            ("23_seconds", "23 Seconds", 20, S23S_UNIMPLEMENTED),
            ("blood_money", "Blood Money", 20, BM_UNIMPLEMENTED),
            ("escalation", "Escalation", 20, ES_UNIMPLEMENTED),
            ("intervention", "Intervention", 20, IN_UNIMPLEMENTED),
            ("martial_law", "Martial Law", 20, ML_UNIMPLEMENTED),
            ("quorum", "Quorum", 20, QU_UNIMPLEMENTED),
            ("terminal_directive_cards", "Terminal Directive Cards", 57, TD_UNIMPLEMENTED),
            ("kala_ghoda", "Kala Ghoda", 19, KG_UNIMPLEMENTED),
            ("business_first", "Business First", 19, BF_UNIMPLEMENTED),
            ("democracy_and_dogma", "Democracy and Dogma", 19, DAG_UNIMPLEMENTED),
            ("salsette_island", "Salsette Island", 19, SI_UNIMPLEMENTED),
            ("the_liberated_mind", "The Liberated Mind", 19, TLM_UNIMPLEMENTED),
            ("fear_the_masses", "Fear the Masses", 19, FTM_UNIMPLEMENTED),
            ("data_and_destiny", "Data and Destiny", 55, DAD_UNIMPLEMENTED),
            ("the_valley", "The Valley", 20, VAL_UNIMPLEMENTED),
            ("breaker_bay", "Breaker Bay", 20, BB_UNIMPLEMENTED),
            ("chrome_city", "Chrome City", 20, CC_UNIMPLEMENTED),
            ("the_underway", "The Underway", 20, UW_UNIMPLEMENTED),
            ("old_hollywood", "Old Hollywood", 20, OH_UNIMPLEMENTED),
            ("the_universe_of_tomorrow", "The Universe of Tomorrow", 20, UOT_UNIMPLEMENTED),
            ("order_and_chaos", "Order and Chaos", 55, OAC_UNIMPLEMENTED),
            ("upstalk", "Upstalk", 20, UP_UNIMPLEMENTED),
            ("the_spaces_between", "The Spaces Between", 20, TSB_UNIMPLEMENTED),
            ("first_contact", "First Contact", 20, FC_UNIMPLEMENTED),
            ("up_and_over", "Up and Over", 20, UAO_UNIMPLEMENTED),
            ("all_that_remains", "All That Remains", 20, ATR_UNIMPLEMENTED),
            ("the_source", "The Source", 20, TS_UNIMPLEMENTED),
            ("honor_and_profit", "Honor and Profit", 55, HAP_UNIMPLEMENTED),
            ("creation_and_control", "Creation and Control", 55, CAC_UNIMPLEMENTED),
            ("opening_moves", "Opening Moves", 20, OM_UNIMPLEMENTED),
            ("second_thoughts", "Second Thoughts", 20, ST_UNIMPLEMENTED),
            ("mala_tempora", "Mala Tempora", 20, MT_UNIMPLEMENTED),
            ("true_colors", "True Colors", 20, TC_UNIMPLEMENTED),
            ("fear_and_loathing", "Fear and Loathing", 20, FAL_UNIMPLEMENTED),
            ("double_time", "Double Time", 20, DT_UNIMPLEMENTED),
            ("what_lies_ahead", "What Lies Ahead", 20, WLA_UNIMPLEMENTED),
            ("trace_amount", "Trace Amount", 20, TA_UNIMPLEMENTED),
            ("cyber_exodus", "Cyber Exodus", 20, CE_UNIMPLEMENTED),
            ("a_study_in_static", "A Study in Static", 20, ASIS_UNIMPLEMENTED),
            ("humanitys_shadow", "Humanity's Shadow", 20, HS_UNIMPLEMENTED),
            ("future_proof", "Future Proof", 20, FP_UNIMPLEMENTED),
            ("napd_multiplayer", "NAPD Multiplayer", 1, NAPD_UNIMPLEMENTED),
            ("magnum_opus", "Magnum Opus", 8, MO_UNIMPLEMENTED),
            ("system_core_2019", "System Core 2019", 147, SC19_UNIMPLEMENTED),
        ] {
            assert_set_accounted_for(set_id, set_name, printed, exceptions);
        }
    }

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
                Effect::PsiGame { on_match, on_differ } => {
                    walk(on_match, out);
                    walk(on_differ, out);
                }
                Effect::EffectIf { effect, otherwise, .. } => {
                    walk(effect, out);
                    if let Some(otherwise) = otherwise {
                        walk(otherwise, out);
                    }
                }
                Effect::Trace { on_success: effect, .. }
                | Effect::SetAccessReplacement { effect, .. }
                | Effect::SetRunEndedEffect(effect)
                | Effect::Unpreventable(effect)
                | Effect::ChooseNumber { then: effect, .. } => walk(effect, out),
                Effect::GainSubroutine { subroutine, .. } => walk(&subroutine.effect, out),
                Effect::PromptChooseCards { then: Some(then), .. } | Effect::Access { then: Some(then), .. } => walk(then, out),
                Effect::PromptInstallCorpCard { then, if_rezzed, .. } => {
                    then.iter().chain(if_rezzed.iter()).for_each(|e| walk(e, out))
                }
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
            card.before_starting_hand.iter().for_each(|e| walk(e, &mut effects));
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
                    Effect::ChooseNumber { text, .. } => {
                        checked += 1;
                        failures.extend(quote("chosen number", text));
                    }
                    // A gained subroutine is read on the ice it is gained
                    // by, so its words are the card's quote of it.
                    Effect::GainSubroutine { subroutine, .. } => {
                        checked += 1;
                        failures.extend(quote("gained subroutine", &subroutine.text));
                    }
                    _ => {}
                }
            }
        }
        assert!(failures.is_empty(), "{} clause(s) are not quotes of their card:\n  {}", failures.len(), failures.join("\n  "));
        assert!(checked > 100, "{checked} clauses checked");
    }

    /// Card files no longer restate what the catalog owns; the join is what
    /// puts it back. If the join regressed, every card would silently lose
    /// its faction and influence, and deckbuilding legality checks would go
    /// quiet.
    #[test]
    fn catalog_metadata_is_filled_in_from_the_join() {
        let tithe = embedded_playable_cards()
            .into_iter()
            .find(|card| card.id.0 == "tithe")
            .expect("tithe should be embedded");

        assert_eq!(tithe.faction, Some(crate::card::Faction::NeutralCorp));
        assert_eq!(tithe.influence_cost, Some(0));
        assert_eq!(tithe.deck_limit, Some(3));
        // What a printing says stays the printing's.
        let printing = crate::cards::catalog::latest_printing(&tithe.id).expect("Tithe is printed");
        assert_eq!((printing.set.as_str(), printing.illustrators.as_deref()), ("system_gateway", Some("Scott Uminga")));
        assert!(tithe.keywords.iter().any(|k| k == "Sentry"), "keywords should come from the catalog");
        // The printed text rides the same join: a client shows it, the
        // engine never reads it.
        let text = tithe.printed_text.as_deref().expect("the catalog carries Tithe's text");
        assert_eq!(text, "[subroutine] Do 1 net damage.\n[subroutine] Gain 1[credit].");
    }
}
