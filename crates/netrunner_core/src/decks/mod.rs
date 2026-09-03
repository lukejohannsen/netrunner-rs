//! The authored deck format, and the decklists embedded at compile time.
//!
//! [`DeckFile`] is the shape a deck is *written* in — an identity, a card
//! list keyed by registry slug, and the metadata a person needs to tell one
//! deck from another (name, category, description, how-to-play prose). It is
//! deliberately one type for both the decks compiled into this crate and the
//! decks a player saves to disk: one parser, one validator, one lister, and
//! a published deck is simply one that ships in the binary.
//!
//! **This crate performs no I/O.** [`DeckFile::from_json`]/[`DeckFile::to_json`]
//! convert to and from text; opening files and resolving directories belongs
//! to a consumer (`netrunner_cli`'s deck store), per AGENTS.md's decoupled
//! engine rule.
//!
//! Three shapes, and why each exists:
//!
//! | Type | Keyed by | Answers |
//! |---|---|---|
//! | `decks::DeckFile` | slug | how a deck is authored and stored |
//! | `rules::Deck` | slug | what `GameState::setup` takes |
//! | `deck::Decklist` | NetrunnerDB code | what deckbuilding legality is defined over |
//!
//! [`DeckFile::to_deck`] and [`DeckFile::to_decklist`] are the conversions
//! between them, and [`DeckFile::validate`] runs both validators in one
//! call — see `crate::deck`'s module doc for why those stay separate.
//!
//! The embedded decks are Null Signal Games' seven System Gateway-only
//! decklists, published at
//! <https://nullsignal.games/players/getting-started-sample-decklists/> —
//! the seven System Gateway-only lists plus every System Gateway +
//! *Elevation* list whose cards are implemented (ROADMAP Phase 1 §8 lands
//! those one or two decks at a time; `cards::embedded::ELEV_UNIMPLEMENTED`
//! names what is still missing). They exist so every consumer needing a
//! real, playable pair of decks (self-play training, the gym environment,
//! the single-player CLI) draws from one source of truth instead of
//! hand-rolling a fixture. Authored one-file-per-deck under `data/decks/`,
//! concatenated by `build.rs` and baked in via `include_str!`, mirroring
//! `cards::embedded` exactly.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::rules::MatchRules;
use crate::card::CardId as NumericCardId;
use crate::cards::CardRegistry;
use crate::deck::{DeckValidationError, Decklist, ValidationReport};
use crate::dsl::CardId;
use crate::format::NsgFormat;
use crate::rules::{Deck, RulesError, Side};

const DECKS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/decks.json"));

/// One entry in a decklist: a card and how many copies of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckEntry {
    pub card: CardId,
    pub count: u32,
}

/// What kind of deck a `DeckFile` is.
///
/// Load-bearing rather than descriptive: `matchups()` filters on it, so a
/// deck's category decides whether the policy network ever trains on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeckCategory {
    /// One of Null Signal Games' published sample decklists. **The only
    /// category `matchups()` yields**, and therefore the only one self-play
    /// and the gym environment ever see.
    Sample,
    /// One of Null Signal Games' *Learn to Play* starter decks, played to
    /// 6 agenda points (`match_rules`).
    Starter,
    /// A starter deck plus its booster pack — a full-size deck played
    /// under Standard rules.
    Boosted,
    /// A deck someone built themselves.
    ///
    /// The default, deliberately: a deck file that forgets to state its
    /// category is excluded from training rather than silently added to it.
    /// Failing that way round costs nothing, while the reverse would quietly
    /// corrupt a training run.
    #[default]
    Custom,
}

impl DeckCategory {
    /// The rules a deck of this category is played under. This is where a
    /// deck record carries its variant's win threshold rather than a
    /// validator guessing: the starter game wins at 6, everything else at
    /// Standard's 7.
    pub fn match_rules(self) -> MatchRules {
        match self {
            DeckCategory::Starter => MatchRules { winning_agenda_points: 6 },
            DeckCategory::Sample | DeckCategory::Boosted | DeckCategory::Custom => MatchRules::default(),
        }
    }
}

/// An authored decklist — one of the published samples embedded at compile
/// time, or one a player saved to disk.
///
/// Cards are referenced by registry `id`, never by title — several System
/// Gateway titles carry non-ASCII characters that are easy to mistype
/// (*Tomorrowʼs Headline* uses U+02BC MODIFIER LETTER APOSTROPHE, matching
/// NetrunnerDB's own data, and *Karunā*/*Brân 1.0*/*Tāo Salonga* carry
/// diacritics), and an id typo is caught by the registry lookup while a
/// title typo would silently miss.
///
/// This is the *authoring* shape. `to_deck` converts it to the runtime
/// `rules::Deck` that `GameState::setup` takes, and `validate` checks it
/// against both of the engine's validators — see the module doc on
/// `crate::deck` for how those two differ and why they stay separate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckFile {
    pub id: String,
    pub name: String,
    pub side: Side,
    #[serde(default)]
    pub category: DeckCategory,
    /// A one-line summary, shown when listing decks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Markdown prose on how the deck wants to be played, rendered by
    /// `netrunner_cli deck show`. Markdown rather than plain text because it
    /// is written for a human to read and edit by hand, and headings and
    /// lists survive being displayed raw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how_to_play: Option<String>,
    pub identity: CardId,
    pub cards: Vec<DeckEntry>,
}

impl DeckFile {
    /// Parses one deck file's JSON text. Pure — reading the file is the
    /// caller's job, since `netrunner_core` performs no I/O.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// This deck as pretty-printed JSON, in the same shape `from_json`
    /// accepts, for a caller that edits a deck and writes it back.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// The `rules::Deck` this decklist describes, ready for
    /// `GameState::setup`. Does not validate — call [`DeckFile::validate`]
    /// for that.
    pub fn to_deck(&self) -> Deck {
        Deck {
            identity: self.identity.clone(),
            cards: self.cards.iter().map(|entry| (entry.card.clone(), entry.count)).collect(),
        }
    }

    /// Total non-identity cards.
    pub fn size(&self) -> u32 {
        self.cards.iter().map(|entry| entry.count).sum()
    }

    /// The numeric-keyed `deck::Decklist` this deck describes, which is what
    /// the deckbuilding validator speaks.
    ///
    /// The two shapes exist because they answer different questions and are
    /// keyed differently: a `DeckFile` names cards by the engine-native slug
    /// it plays from, while deckbuilding legality is defined over
    /// NetrunnerDB's printed metadata, keyed by card code. `numeric_id` is
    /// the join between them.
    ///
    /// Total for the embedded pool — `every_playable_card_carries_a_numeric_id`
    /// keeps it so — but still fallible, because a homebrew card loaded
    /// through the `fs-loader` feature need not carry a code.
    pub fn to_decklist(&self, registry: &CardRegistry) -> Result<Decklist, DeckError> {
        fn code(registry: &CardRegistry, card: &CardId) -> Result<NumericCardId, DeckError> {
            let definition = registry.get(card).ok_or_else(|| DeckError::UnknownCard(card.clone()))?;
            definition.numeric_id.ok_or_else(|| DeckError::NoPrintedMetadata(card.clone()))
        }

        let mut cards = HashMap::new();
        for entry in &self.cards {
            // Summed rather than inserted: a deck file may legitimately list
            // the same card twice, and the copy limit is about the total.
            *cards.entry(code(registry, &entry.card)?).or_insert(0) += entry.count;
        }
        Ok(Decklist { identity: code(registry, &self.identity)?, cards })
    }

    /// Checks this deck against **both** of the engine's validators and
    /// returns the deckbuilding report on success.
    ///
    /// Gameplay executability is checked first: "this references a card the
    /// engine cannot play" is a more fundamental complaint than "this is two
    /// influence over", and reporting it second would bury it.
    ///
    /// The two validators are deliberately not merged — see `crate::deck`'s
    /// module doc. This is the seam that runs them together, so a caller
    /// gets one answer instead of having to know which to invoke.
    pub fn validate(&self, registry: &CardRegistry, format: NsgFormat) -> Result<ValidationReport, DeckError> {
        crate::rules::deck::validate_deck(&self.to_deck(), self.side, registry)?;
        Ok(crate::deck::validate_deck(&self.to_decklist(registry)?, registry, format)?)
    }
}

/// Why a `DeckFile` could not be converted or validated.
///
/// Wraps both validators' error types rather than flattening them: each
/// already carries a precise, human-readable message, and restating those
/// cases here would be a second copy to keep in step.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeckError {
    #[error("deck references {0:?}, which is not a card this engine implements")]
    UnknownCard(CardId),

    #[error(
        "card {0:?} carries no NetrunnerDB code, so it has no faction, influence cost or set, \
         and its deckbuilding legality cannot be checked"
    )]
    NoPrintedMetadata(CardId),

    #[error("{0}")]
    Unplayable(#[from] RulesError),

    #[error("{0}")]
    Illegal(#[from] DeckValidationError),
}

/// Parses the embedded decklists.
///
/// A failure here is an authoring bug in a checked-in deck file that got
/// past the test suite, not a runtime condition a caller could recover
/// from — so it panics rather than returning a `Result` every consumer
/// would have to `unwrap` anyway, matching `cards::embedded`'s reasoning.
fn parse() -> Vec<DeckFile> {
    serde_json::from_str(DECKS_JSON).unwrap_or_else(|e| panic!("embedded deck data failed to parse: {e}"))
}

/// Every deck compiled into the binary, ordered by file name (so, by deck
/// id). These are immutable; a deck a player saves lives on disk and is the
/// CLI's business, not this crate's.
pub fn embedded_decks() -> Vec<DeckFile> {
    parse()
}

/// The embedded deck with this id, if one exists.
pub fn by_id(id: &str) -> Option<DeckFile> {
    parse().into_iter().find(|deck| deck.id == id)
}

/// The embedded decks for one side.
pub fn for_side(side: Side) -> Vec<DeckFile> {
    parse().into_iter().filter(|deck| deck.side == side).collect()
}

/// Every legal `(corp, runner)` pairing of *sample* decks, ordered
/// deterministically.
///
/// Every `Sample` Corp deck against every `Sample` Runner deck — the full
/// cross product (4 × 3 = 12 at the System Gateway pool, growing as
/// *Elevation* decks land), which is the pool self-play samples from so
/// training sees the whole card set rather than one fixed matchup's slice
/// of it.
///
/// **Filtered to `DeckCategory::Sample`, and that filter is load-bearing.**
/// Every consumer of this function feeds a training or verification harness
/// — `netrunner_selfplay`, `netrunner_gym`, and both agent-driven sweeps —
/// so anything yielded here is something the policy network learns from. A
/// tutorial or player-built deck reaching them would quietly train the
/// network on decks it will never face, which is why `DeckCategory` defaults
/// to `Custom` rather than `Sample`.
pub fn matchups() -> Vec<(DeckFile, DeckFile)> {
    let sample = |side| -> Vec<DeckFile> {
        for_side(side).into_iter().filter(|deck| deck.category == DeckCategory::Sample).collect()
    };
    let corps = sample(Side::Corp);
    let runners = sample(Side::Runner);
    corps.iter().flat_map(|corp| runners.iter().map(move |runner| (corp.clone(), runner.clone()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{register_playable_cards, CardRegistry};

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        registry
    }

    /// Runs **both** validators, via `DeckFile::validate` — these are real
    /// published decklists, so they must be legal to build as well as
    /// playable. The deckbuilding half (influence, format pool, per-card
    /// deck limits) had no caller at all before this, so this is the first
    /// thing that exercises it against real data.
    #[test]
    fn every_sample_deck_is_legal() {
        let registry = registry();
        for deck in embedded_decks() {
            deck.validate(&registry, NsgFormat::Startup)
                .unwrap_or_else(|e| panic!("sample deck {:?} ({}) is not legal: {e}", deck.id, deck.name));
        }
    }

    /// Every published sample decklist this crate embeds, pinned to what
    /// Null Signal Games printed: id, side, card count, influence spent
    /// and (for a Corp deck) agenda points. One table, extended a stage at
    /// a time as *Elevation* lands (ROADMAP Phase 1 §8), so a card-data
    /// edit that changes an agenda's points, an identity's minimum deck
    /// size or a card's influence fails here against the real list rather
    /// than silently breaking it. The seven System Gateway-only decks
    /// spend 14-15 of 15 influence; the Elevation-era lists are pinned to
    /// the influence they actually spend.
    const PUBLISHED: &[(&str, Side, u32, u32, u32)] = &[
        // id, side, cards, influence, agenda points
        ("advanced_yomi", Side::Corp, 44, 15, 18),
        ("agency", Side::Corp, 49, 15, 20),
        ("bowel_movements", Side::Runner, 40, 15, 0),
        ("brick_stack", Side::Corp, 44, 15, 18),
        ("brutal_efficiency", Side::Corp, 44, 15, 18),
        ("dashing_mad", Side::Runner, 45, 17, 0),
        ("discretion_advised", Side::Corp, 44, 15, 18),
        ("enthusiasm", Side::Runner, 45, 15, 0),
        ("fashion_lab", Side::Corp, 49, 15, 20),
        ("flow_and_ebb", Side::Runner, 40, 15, 0),
        ("hyper_velocity", Side::Corp, 44, 15, 18),
        ("party_hard", Side::Runner, 40, 14, 0),
        ("planning_ahead", Side::Runner, 40, 15, 0),
        ("pork_chops", Side::Corp, 49, 15, 20),
        ("prick_thyself", Side::Runner, 45, 15, 0),
        ("professional_opportunities", Side::Runner, 45, 15, 0),
        ("quick_and_dirty", Side::Corp, 44, 15, 18),
        ("sabbatical", Side::Runner, 45, 15, 0),
        ("shootin_n_lootin", Side::Runner, 45, 15, 0),
        ("stolen_goods", Side::Runner, 40, 14, 0),
        ("tickets_please", Side::Runner, 40, 15, 0),
    ];

    /// See `PUBLISHED`. Also the guard that the pool is exactly the
    /// published lists: a `Sample` deck this table does not name fails.
    #[test]
    fn sample_decks_match_the_published_lists() {
        let registry = registry();
        let mut decks: Vec<DeckFile> =
            embedded_decks().into_iter().filter(|deck| deck.category == DeckCategory::Sample).collect();
        decks.sort_by(|a, b| a.id.cmp(&b.id));
        let ids: Vec<&str> = decks.iter().map(|deck| deck.id.as_str()).collect();
        let expected: Vec<&str> = PUBLISHED.iter().map(|(id, ..)| *id).collect();
        assert_eq!(ids, expected, "the Sample pool must be exactly the published lists this table pins");

        for (deck, (id, side, cards, influence, agenda_points)) in decks.iter().zip(PUBLISHED) {
            assert_eq!(deck.side, *side, "{id}");
            assert_eq!(deck.size(), *cards, "{id} card count");
            let report = deck.validate(&registry, NsgFormat::Startup).expect("published decks are legal");
            assert_eq!(report.influence_spent, *influence, "{id} influence spent");
            let points: u32 = deck
                .cards
                .iter()
                .map(|entry| registry.get(&entry.card).expect("deck card is registered").agenda_points.unwrap_or(0) * entry.count)
                .sum();
            assert_eq!(points, *agenda_points, "{id} agenda points");
        }
    }

    /// The *Learn to Play* lists, pinned the same way: sizes, categories and
    /// the starter Corp deck's 14 agenda points (18 once boosted).
    #[test]
    fn starter_decks_match_the_published_lists() {
        let registry = registry();
        let mut decks: Vec<(String, Side, DeckCategory, u32)> = embedded_decks()
            .into_iter()
            .filter(|deck| matches!(deck.category, DeckCategory::Starter | DeckCategory::Boosted))
            .map(|deck| (deck.id.clone(), deck.side, deck.category, deck.size()))
            .collect();
        decks.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            decks,
            vec![
                ("the_catalyst_boosted".to_string(), Side::Runner, DeckCategory::Boosted, 40),
                ("the_catalyst_starter".to_string(), Side::Runner, DeckCategory::Starter, 30),
                ("the_syndicate_boosted".to_string(), Side::Corp, DeckCategory::Boosted, 44),
                ("the_syndicate_starter".to_string(), Side::Corp, DeckCategory::Starter, 34),
            ]
        );
        let points = |id: &str| -> u32 {
            by_id(id)
                .expect("embedded")
                .cards
                .iter()
                .map(|entry| registry.get(&entry.card).expect("registered").agenda_points.unwrap_or(0) * entry.count)
                .sum()
        };
        assert_eq!(points("the_syndicate_starter"), 14);
        assert_eq!(points("the_syndicate_boosted"), 18);
        assert_eq!(DeckCategory::Starter.match_rules().winning_agenda_points, 6);
        assert_eq!(DeckCategory::Boosted.match_rules().winning_agenda_points, 7);
    }

    #[test]
    fn matchups_cover_every_corp_runner_pairing() {
        let sample = |side| for_side(side).into_iter().filter(|deck| deck.category == DeckCategory::Sample).count();
        let (corps, runners) = (sample(Side::Corp), sample(Side::Runner));
        let matchups = matchups();
        assert_eq!(matchups.len(), corps * runners, "{corps} Corp decks x {runners} Runner decks");
        for (corp, runner) in &matchups {
            assert_eq!(corp.side, Side::Corp);
            assert_eq!(runner.side, Side::Runner);
        }
    }

    #[test]
    fn by_id_finds_a_deck_and_rejects_an_unknown_one() {
        assert_eq!(by_id("party_hard").map(|deck| deck.name), Some("Party Hard".to_string()));
        assert!(by_id("no_such_deck").is_none());
    }

    /// Every embedded deck must state its category: `matchups()` filters
    /// on `Sample` and silently yields nothing otherwise, and a starter
    /// deck left `Custom` would be a tutorial deck the tutorial cannot find.
    #[test]
    fn every_embedded_deck_is_labelled_and_none_is_custom() {
        for deck in embedded_decks() {
            assert_ne!(deck.category, DeckCategory::Custom, "{} must state its category", deck.id);
        }
    }

    /// The filter that keeps player-built decks out of training. Asserted
    /// directly rather than only through `matchups()`'s count, so the reason
    /// a deck was excluded is unambiguous when this fails.
    #[test]
    fn matchups_exclude_decks_that_are_not_samples() {
        let mut custom = by_id("party_hard").expect("party_hard is embedded");
        custom.category = DeckCategory::Custom;

        let sample = |decks: &[DeckFile]| -> usize {
            decks.iter().filter(|deck| deck.category == DeckCategory::Sample).count()
        };
        assert_eq!(sample(&[custom.clone()]), 0, "a Custom deck must not count as a sample");
        assert_eq!(custom.category, DeckCategory::Custom);
    }

    /// A deck file that omits `category` is `Custom`, not `Sample` — the
    /// direction that keeps a forgotten label out of training rather than
    /// silently into it.
    #[test]
    fn an_unlabelled_deck_file_is_custom() {
        let json = r#"{
            "id": "homebrew", "name": "Homebrew", "side": "Corp",
            "identity": "haas_bioroid_precision_design", "cards": []
        }"#;
        let deck = DeckFile::from_json(json).expect("valid deck JSON");

        assert_eq!(deck.category, DeckCategory::Custom);
        assert_eq!(deck.description, None);
        assert_eq!(deck.how_to_play, None);
    }

    #[test]
    fn a_deck_file_round_trips_with_its_prose_intact() {
        let mut deck = by_id("party_hard").expect("party_hard is embedded");
        deck.description = Some("Aggressive Anarch tempo.".to_string());
        deck.how_to_play = Some("## Opening\n\n- Mulligan for Cleaver.".to_string());

        let round_tripped = DeckFile::from_json(&deck.to_json().expect("serializes")).expect("deserializes");
        assert_eq!(round_tripped, deck);
    }

    /// `deny_unknown_fields` is what makes a mistyped key a loud failure
    /// rather than a silently-defaulted field — the same guard card JSON has.
    #[test]
    fn a_misspelled_deck_key_is_rejected() {
        let json = r#"{
            "id": "typo", "name": "Typo", "side": "Corp", "catagory": "Sample",
            "identity": "haas_bioroid_precision_design", "cards": []
        }"#;
        let err = DeckFile::from_json(json).expect_err("a misspelled key must not parse");
        assert!(err.to_string().contains("catagory"), "error should name the offending key: {err}");
    }

    #[test]
    fn to_decklist_maps_slugs_to_netrunnerdb_codes() {
        let registry = registry();
        let deck = by_id("party_hard").expect("party_hard is embedded");

        let decklist = deck.to_decklist(&registry).expect("every sample card carries a code");

        // René "Loup" Arcemont is 30001; the deck runs 3 Sure Gamble (30029).
        assert_eq!(decklist.identity, NumericCardId(30001));
        assert_eq!(decklist.cards.values().sum::<u32>(), deck.size());
        assert_eq!(decklist.cards.get(&NumericCardId(30029)), Some(&3));
    }

    #[test]
    fn to_decklist_rejects_a_card_the_engine_does_not_implement() {
        let registry = registry();
        let mut deck = by_id("party_hard").expect("party_hard is embedded");
        deck.cards.push(DeckEntry { card: CardId("not_a_real_card".to_string()), count: 1 });

        assert_eq!(
            deck.to_decklist(&registry),
            Err(DeckError::UnknownCard(CardId("not_a_real_card".to_string())))
        );
    }

    /// The two validators answer different questions, and `validate` must
    /// surface whichever one actually failed.
    #[test]
    fn validate_reports_an_unplayable_deck_before_an_illegal_one() {
        let registry = registry();
        let mut deck = by_id("party_hard").expect("party_hard is embedded");
        deck.cards.push(DeckEntry { card: CardId("sure_gamble".to_string()), count: 9 });

        // Over the copy limit: a gameplay-executability failure, so it is
        // `Unplayable`, not `Illegal`.
        assert!(
            matches!(deck.validate(&registry, NsgFormat::Startup), Err(DeckError::Unplayable(_))),
            "exceeding the copy limit is a rules failure"
        );
    }

    /// Core Set cards carry full printed metadata now, so they validate —
    /// but they are not in Startup's pool, and must fail it for the right
    /// reason rather than passing unnoticed.
    #[test]
    fn a_core_set_card_is_outside_startups_pool() {
        let registry = registry();
        let mut deck = by_id("advanced_yomi").expect("advanced_yomi is embedded");

        // Swapped in for a non-agenda card rather than appended: growing the
        // deck to 45 would move the legal agenda-point range and fail the
        // *gameplay* validator first, testing the wrong thing.
        let swap = deck
            .cards
            .iter_mut()
            .find(|entry| {
                registry.get(&entry.card).is_some_and(|card| card.agenda_points.is_none()) && entry.count == 1
            })
            .expect("every Corp sample deck has a single-copy non-agenda card");
        swap.card = CardId("ice_wall".to_string());

        match deck.validate(&registry, NsgFormat::Startup) {
            Err(DeckError::Illegal(DeckValidationError::PackNotLegal { set_code, .. })) => {
                assert_eq!(set_code, "core", "Ice Wall is a Core Set printing, and Startup is sg+elev");
            }
            other => panic!("expected Ice Wall to be outside Startup's pool, got {other:?}"),
        }
    }
}
