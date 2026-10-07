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
//! names what is still missing), and — since Standard was complete (6
//! October 2026) — twelve Standard tournament lists published on
//! NetrunnerDB, written by `scripts/tournament_decks.py` and never edited
//! by hand. They exist so every consumer needing a
//! real, playable pair of decks (self-play training, the gym environment,
//! the single-player CLI) draws from one source of truth instead of
//! hand-rolling a fixture. Authored one-file-per-deck under `data/decks/`,
//! concatenated by `build.rs` and baked in via `include_str!`, mirroring
//! `cards::embedded` exactly.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::rules::MatchRules;
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
    /// A published decklist: one of Null Signal Games' sample decklists,
    /// or a Standard tournament list from NetrunnerDB
    /// (`scripts/tournament_decks.py`, ids `tournament_<identity>`). **The
    /// only category `matchups()` yields**, and therefore the only one
    /// self-play and the gym environment ever see.
    Sample,
    /// One of Null Signal Games' *Learn to Play* starter decks, played to
    /// 6 agenda points (`match_rules`).
    Starter,
    /// A starter deck plus its booster pack — a full-size deck played
    /// under Standard rules.
    Boosted,
    /// A deck built in this project to reach rules no published list
    /// prints — today, the prevention window, whose only cards are the Core
    /// Set's interrupts, which no Startup deck may hold. **Played by both
    /// agent-driven sweeps beside the samples
    /// (`netrunner_session::sweep_decks_for_seed`) and by nothing else:**
    /// `matchups()` does not yield it, so no network trains on it and no
    /// measurement taken over "one pass of the pool" moves when one is
    /// added. Legal in Eternal, not Startup (`format`); its identity is one
    /// no sample deck uses, so `netrunner_bots::determinize`'s guess at a
    /// sample deck's list never lands on it.
    Sweep,
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
            DeckCategory::Sample | DeckCategory::Boosted | DeckCategory::Sweep | DeckCategory::Custom => MatchRules::default(),
        }
    }

    /// The format an embedded deck of this category is legal in — what
    /// `every_sample_deck_is_legal` holds each one to. Casual for
    /// everything Null Signal Games published (see below); Eternal for a
    /// sweep deck, which exists to field cards Startup does not have.
    pub fn format(self) -> NsgFormat {
        match self {
            DeckCategory::Sweep => NsgFormat::Eternal,
            // The published lists were Startup lists when they were
            // published; NetrunnerDB's lists have since banned cards some
            // of them hold (Stage 0b). They are still decks to play, so the
            // format they are held to as a category is Casual, and which
            // formats each is legal in is pinned deck by deck
            // (`every_shipped_deck_is_legal_in_the_formats_pinned_for_it`).
            DeckCategory::Sample | DeckCategory::Starter | DeckCategory::Boosted | DeckCategory::Custom => NsgFormat::Casual,
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
    /// How a bot should play this list: the plans it stacks, in order,
    /// each the name of a `netrunner_bots::Plan` written for the deck's
    /// side (`["glacier", "fast-advance"]`, `["traps"]`, `["rig", "dismantle"]`).
    /// Empty is balanced play.
    ///
    /// Strings rather than the enum because this crate is the bottom of
    /// the dependency graph and the vocabulary lives in `netrunner_bots`,
    /// which is where it belongs — a style is what a bot's evaluator
    /// reads, not a property the rules know. `netrunner_bots` carries the
    /// test that every embedded deck's style parses and is for the right
    /// chair, so a typo here fails a build rather than seating a balanced
    /// bot under a glacier deck's name. A saved deck may omit it.
    ///
    /// A list because the strategy guide says most real decks mix two of
    /// its styles ("glacier, then fast advance"), and the turn planner
    /// plays the whole list; the one-ply reference plays the first. On the
    /// deck rather than the identity because two of these lists share an
    /// identity and play differently (*Discretion Advised* and *Brutal
    /// Efficiency* are both *Precision Design*), and because a player who
    /// builds a deck knows how it wants to be played better than a table
    /// keyed on the identity could.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style: Vec<String>,
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

    /// The style as one word for a label: the plans joined with `+`, or
    /// "balanced" for none — the spelling `netrunner_bots::Style` prints.
    pub fn style_label(&self) -> String {
        if self.style.is_empty() {
            "balanced".to_string()
        } else {
            self.style.join("+")
        }
    }

    /// Total non-identity cards.
    pub fn size(&self) -> u32 {
        self.cards.iter().map(|entry| entry.count).sum()
    }

    /// The `deck::Decklist` this deck describes, which is what the
    /// deckbuilding validator speaks: the same cards by the same ids, the
    /// counts summed per card.
    ///
    /// It was keyed by printing code, joined through each card file's
    /// `numeric_id`, until the catalog moved to NetrunnerDB v3 (NSG pool
    /// Stage 0d): legality is per card now, so the only thing left to fail
    /// on is a card the registry does not hold.
    pub fn to_decklist(&self, registry: &CardRegistry) -> Result<Decklist, DeckError> {
        let known = |card: &CardId| registry.get(card).map(|_| card.clone()).ok_or_else(|| DeckError::UnknownCard(card.clone()));

        let mut cards = HashMap::new();
        for entry in &self.cards {
            // Summed rather than inserted: a deck file may legitimately list
            // the same card twice, and the copy limit is about the total.
            *cards.entry(known(&entry.card)?).or_insert(0) += entry.count;
        }
        Ok(Decklist { identity: known(&self.identity)?, cards })
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
    ///
    /// **A *Learn to Play* identity is legal only with its own lists.** CR
    /// 1.4.1a: The Catalyst and The Syndicate "are intended for use only with
    /// the decks included in that pack. They are not legal for play under
    /// the full deck construction rules." Those identities print no
    /// influence budget (`CardDefinition::unlimited_influence`), so the
    /// deckbuilding validator could not refuse anything under them and a
    /// Catalyst deck of every faction validated in Standard. What says so
    /// is their printed "Starter game only." (`DeckRule::StarterGameOnly`),
    /// not the budget they lack: Nova Initiumia and Ampère print none
    /// either, and are legal in Standard. The check is
    /// against the lists themselves — the embedded starter and boosted decks
    /// — rather than the file's `category`, because a deck brought to a
    /// server names its own category.
    pub fn validate(&self, registry: &CardRegistry, format: NsgFormat) -> Result<ValidationReport, DeckError> {
        crate::rules::deck::validate_deck(&self.to_deck(), self.side, registry)?;
        let starter_identity = registry
            .get(&self.identity)
            .is_some_and(|identity| identity.deck_rules.contains(&crate::dsl::DeckRule::StarterGameOnly));
        if starter_identity && !self.is_a_learn_to_play_list() {
            return Err(DeckError::StarterIdentity(self.identity.clone()));
        }
        Ok(crate::deck::validate_deck(&self.to_decklist(registry)?, registry, format)?)
    }

    /// Every format this deck is legal in, in `NsgFormat::ALL`'s order:
    /// both validators, per format. A deck's legality is this set, never
    /// one flag (the person's decision, 26 September 2026) — a ban makes a
    /// deck illegal in the format that bans it and nowhere else, and
    /// `Casual` holds every deck the engine can run. Computed, never stored:
    /// a new list re-judges every deck on the next read.
    pub fn legal_formats(&self, registry: &CardRegistry) -> Vec<NsgFormat> {
        NsgFormat::ALL.into_iter().filter(|format| self.validate(registry, *format).is_ok()).collect()
    }

    /// Whether this deck is card for card one of the published *Learn to
    /// Play* lists (a starter deck or its boosted version).
    fn is_a_learn_to_play_list(&self) -> bool {
        fn contents(deck: &DeckFile) -> Vec<(CardId, u32)> {
            let mut counts: HashMap<CardId, u32> = HashMap::new();
            for entry in &deck.cards {
                *counts.entry(entry.card.clone()).or_insert(0) += entry.count;
            }
            let mut contents: Vec<_> = counts.into_iter().collect();
            contents.sort();
            contents
        }
        let mine = contents(self);
        embedded_decks().iter().any(|published| {
            matches!(published.category, DeckCategory::Starter | DeckCategory::Boosted)
                && published.identity == self.identity
                && contents(published) == mine
        })
    }

    /// Running totals against the identity's limits, for a deck still being
    /// built — see `deck::DeckTally`. Not a legality check: `validate` is.
    pub fn tally(&self, registry: &CardRegistry) -> Result<crate::deck::DeckTally, DeckError> {
        Ok(crate::deck::tally_deck(&self.to_decklist(registry)?, registry)?)
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

    /// CR 1.4.1a — see [`DeckFile::validate`].
    #[error("{0:?} is a Learn to Play identity, legal only with its own starter or boosted list")]
    StarterIdentity(CardId),

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
/// — `netrunner_selfplay`, `netrunner_gym`, `bench`, the headless
/// `--all-matchups` report — so anything yielded here is something the
/// policy network learns from or a recorded measurement was taken over.
/// (The two agent-driven sweeps rotate a pool of their own,
/// `netrunner_session::sweep_decks_for_seed`: these and the `Sweep` decks.) A
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

/// `matchups()`, keeping only the pairings both decks are legal in
/// `format` — the pass a measurement of *Startup* play is taken over
/// (Phase 5 §25 Stage 0), in `matchups()`'s own order so a `Casual` pass
/// is `matchups()` game for game and every number recorded before the
/// filter existed still reproduces.
///
/// Filtered a deck at a time rather than a pairing at a time: a deck's
/// legality is `validate` under every rule of the format, and 40 decks
/// asked once is cheaper than 391 pairs asked twice. NetrunnerDB's lists
/// have banned cards nine of the sample decks hold, and no tournament list
/// is Startup, so a Startup pass is 90 of the 391 pairings (it was 90 of
/// 192 before the tournament lists joined), and the gap between the two is why
/// `coverage_identical.py` refuses to compare passes of different sizes.
pub fn matchups_in(format: NsgFormat, registry: &CardRegistry) -> Vec<(DeckFile, DeckFile)> {
    let legal = |side| -> Vec<DeckFile> {
        for_side(side)
            .into_iter()
            .filter(|deck| deck.category == DeckCategory::Sample && deck.validate(registry, format).is_ok())
            .collect()
    };
    let corps = legal(Side::Corp);
    let runners = legal(Side::Runner);
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
    /// **Which formats each shipped deck is legal in**, pinned (Stage 0b, 26
    /// September 2026, against NetrunnerDB's lists of that day). Every deck
    /// is legal in Eternal and Casual. The published lists predate the
    /// current Startup balance update and Standard ban list, so a deck
    /// holding Cleaver, Mercia B4LL4RD, NBN: Reality Plus or Seamless
    /// Launch is not Startup-legal, and one holding Cleaver, Luminal
    /// Transubstantiation, NBN: Reality Plus or Touch-ups is not
    /// Standard-legal. None is Snapshot-legal: Snapshot is a pool of Fantasy
    /// Flight Games cycles. A new list that moves a deck fails here, so the
    /// change is read rather than shipped.
    #[test]
    fn every_shipped_deck_is_legal_in_the_formats_pinned_for_it() {
        use NsgFormat::{Casual, Eternal, Standard, Startup};
        let registry = registry();
        let not_startup = [Standard, Eternal, Casual];
        let neither = [Eternal, Casual];
        let not_standard = [Startup, Eternal, Casual];
        let expected: &[(&str, &[NsgFormat])] = &[
            ("agency", &neither),
            ("brutal_efficiency", &neither),
            ("discretion_advised", &neither),
            ("fashion_lab", &neither),
            ("fine_print", &neither),
            ("hyper_velocity", &neither),
            ("party_hard", &neither),
            ("planning_ahead", &neither),
            ("the_catalyst_boosted", &neither),
            ("the_catalyst_starter", &neither),
            ("a_thousand_cuts", &neither),
            // Vantage Point Stage 8: Méliès U's deck, pinned by the test.
            ("honor_roll", &neither),
            ("hostile_bid", &neither),
            // Phase 5 §25 Stage 6: the kill plan's deck, on Building a
            // Better World; Scorched Earth and Ice Wall keep it out of
            // Standard and Startup.
            ("tag_youre_it", &neither),
            // Rebellion Without Rehearsal Stage 5e: Nuvem SA's deck, on
            // Hostile Bid's frame. The Automata Initiative Stage 7 took out
            // its Ice Wall (for a Tree Line), its one card off the Standard
            // pool, so it is Standard too.
            ("land_grab", &not_startup),
            // The Automata Initiative Stage 5: Epiphany Analytica's deck,
            // on Paid Content's frame. Without Making News and Neurospike
            // it holds no Core Set card, so it is Standard too.
            ("grand_opening", &not_startup),
            // The Automata Initiative Stage 6: Mercury's deck, on Stolen
            // Goods' frame, which holds no Core Set card, so it is Standard.
            ("picket_line", &not_startup),
            // Rebellion Without Rehearsal Stage 8b: Thunderbolt Armaments'
            // deck, on Retirement Package's frame: without Engineering the
            // Future it holds no Core Set card, so it is Standard too.
            ("deterrence", &not_startup),
            // Parhelion Stage 1: Thule Subsea's deck, on Retirement
            // Package's frame, which without Engineering the Future holds
            // no Core Set card, so it is Standard too.
            ("undertow", &not_startup),
            // Parhelion Stage 5a: Nova Initiumia's and Ampère's singleton
            // decks, of Standard cards alone.
            ("mixtape", &not_startup),
            ("sampler", &not_startup),
            // The Automata Initiative Stage 8c: A Teia's and Arissana's
            // decks, on A Thousand Cuts' and Safety Net's frames, whose Core
            // Set cards keep them out of Standard.
            ("second_site", &neither),
            // Parhelion Stage 7: Issuaq Adaptics' deck, on A Thousand Cuts'
            // frame, whose Core Set cards keep it out of Standard.
            ("permafrost", &neither),
            // Midnight Sun Stage 1: Pravdivost Consulting's deck, on Paid
            // Content's frame, which without NBN: Making News holds no Core
            // Set card, so it is Standard too.
            ("spin_cycle", &not_startup),
            ("street_gallery", &neither),
            ("pay_as_you_go", &neither),
            ("safety_net", &neither),
            // Midnight Sun Stage 3: Esâ Afontov's and Captain Padma
            // Isbister's decks, on Pay As You Go's and Safety Net's frames,
            // whose Core Set cards keep them out of Standard.
            ("burn_rate", &neither),
            ("dead_reckoning", &neither),
            // Midnight Sun Stage 6a: Nyusha "Sable" Sintashta's deck, the
            // mark's; she is banned in Standard, and Midnight Sun is out of
            // Startup's pool.
            ("encore", &neither),
            // Midnight Sun Stage 8c: Ob Superheavy Logistics' deck, of
            // Weyland and neutral cards; pinned as the test reads it.
            ("supply_chain", &neither),
            // Uprising Stage 2: Hoshiko Shiro's deck, on Pay As You Go's
            // frame, whose Core Set cards keep it out of Standard (where
            // Hoshiko is banned besides).
            ("side_quest", &neither),
            // Uprising Stage 5: GameNET's deck, on Paid Content's frame,
            // whose Drago Ivanov and Gold Farmer keep it out of Standard.
            ("pay_to_win", &neither),
            // Uprising Stage 8: Earth Station's deck, on Hostile Bid's
            // frame, whose Core Set cards keep it out of Standard.
            ("ground_control", &neither),
            // Downfall Stage 4: Lat's deck, on Safety Net's frame, whose Core
            // Set cards keep it out of Standard.
            ("level_pegging", &neither),
            // Downfall Stage 7: Hyoubu Institute's deck, on Permafrost's
            // and Second Site's frames, whose Core Set cards keep it out of
            // Standard.
            ("open_book", &neither),
            // Downfall Stage 8: MirrorMorph's deck, on Retirement Package's
            // frame, whose Core Set cards keep it out of Standard.
            ("endless_loop", &neither),
            // Tranche 8 Stage 4: Ken "Express" Tenma's deck, on Encore's
            // frame, and Near-Earth Hub's, on Pay to Win's. Both identities
            // are System Update 2021's, which is in no current pool.
            ("express_delivery", &neither),
            ("broadcast_hour", &neither),
            // Tranche 8 Stage 5a: Quetzal's deck, on Pay As You Go's frame,
            // and Rielle "Kit" Peddler's, on Safety Net's.
            ("free_spirit", &neither),
            ("transhuman", &neither),
            // Downfall Stage 1: Az McCaffrey's deck, on Picket Line's
            // frame, which holds no Core Set card, so it is Standard too.
            ("moonlighting", &not_startup),
            // Vantage Point Stage 5d: Hiram's deck carries Core Set cards
            // (Net Shield among them), as Safety Net does.
            ("spare_parts", &neither),
            // Vantage Point Stage 1b's two Corp decks sit on Core Set
            // identities (Engineering the Future, Making News), which no
            // current pool holds. Its Runner deck, Borrowed Time, is every
            // format but Snapshot: Vic and Stolen Goods' frame are all
            // Startup cards. Stage 2's Ad Nihilum is too: Editorial
            // Division and every card with it is System Gateway, Elevation
            // or Vantage Point.
            // Rebellion Without Rehearsal Stage 2a: Sebastião's deck
            // carries Core Set cards (Cyberfeeder, Corroder), as Pay As You
            // Go does.
            ("grassroots", &neither),
            // Rebellion Without Rehearsal Stage 8c: Jeitinho's deck, on Gabriel
            // Santiago, a Core Set identity.
            ("hit_list", &neither),
            ("retirement_package", &neither),
            // Tranche 8 Stage 7d: Architects of Tomorrow's deck, on
            // Retirement Package's frame and its Core Set cards.
            ("assembly_line", &neither),
            ("paid_content", &neither),
            ("quick_returns", &not_startup),
            ("the_syndicate_boosted", &not_startup),
            ("the_syndicate_starter", &not_startup),
            // The Standard tournament lists (`scripts/tournament_decks.py`,
            // 6 October 2026) were played under the Standard Balance Update
            // 26.08, which unbans NBN: Reality Plus and Svyatogor Excavator.
            // NetrunnerDB still names Ban List 26.03 as Standard's active
            // list, and `formats.json` is NetrunnerDB's, so these two read
            // as not Standard until NetrunnerDB moves and the catalog is
            // synced. Every other tournament list is Standard, and none is
            // Startup.
            ("tournament_au_co_the_gold_standard_in_clones", &not_startup),
            ("tournament_haas_bioroid_precision_design", &not_startup),
            ("tournament_leo_construction_labor_solutions", &not_startup),
            ("tournament_magdalene_keino_chemutai_cryptarchitect", &not_startup),
            ("tournament_muslihat_multifarious_marketeer", &not_startup),
            ("tournament_nbn_reality_plus", &neither),
            ("tournament_nebula_talent_management_making_stars", &not_startup),
            ("tournament_nuvem_sa_law_of_the_land", &not_startup),
            ("tournament_ob_superheavy_logistics_extract_export_excel", &neither),
            ("tournament_rene_loup_arcemont_party_animal", &not_startup),
            ("tournament_sebastiao_souza_pessoa_activist_organizer", &not_startup),
            ("tournament_virtual_intelligence_p_i_you_can_call_me_vic", &not_startup),
            ("gimbatul", &not_standard),
            ("not_so_subtle", &not_standard),
            ("pork_chops", &not_standard),
        ];
        for deck in embedded_decks() {
            let legal = deck.legal_formats(&registry);
            let want: Vec<NsgFormat> = expected
                .iter()
                .find(|(id, _)| *id == deck.id)
                .map_or_else(|| vec![Startup, Standard, Eternal, Casual], |(_, formats)| formats.to_vec());
            assert_eq!(legal, want, "{} ({})", deck.id, deck.name);
        }
    }

    #[test]
    fn every_sample_deck_is_legal() {
        let registry = registry();
        for deck in embedded_decks() {
            deck.validate(&registry, deck.category.format())
                .unwrap_or_else(|e| panic!("sample deck {:?} ({}) is not legal: {e}", deck.id, deck.name));
        }
    }

    /// CR 1.4.1a: a *Learn to Play* identity is legal with its published
    /// lists and nothing else. Swapping one card out of the starter Runner
    /// deck is a deck of the same identity that no pack contains, and with
    /// no influence budget to break it validated in every format before.
    #[test]
    fn a_learn_to_play_identity_is_legal_only_with_its_own_lists() {
        let registry = registry();
        for id in ["the_catalyst_starter", "the_catalyst_boosted", "the_syndicate_starter", "the_syndicate_boosted"] {
            let deck = by_id(id).expect("embedded");
            deck.validate(&registry, NsgFormat::Casual).unwrap_or_else(|e| panic!("{id} is a published list: {e}"));
        }

        let mut altered = by_id("the_catalyst_starter").expect("embedded");
        // Swap the first entry for a card from a Runner sample deck that the
        // starter list lacks, keeping the count so the size stays at the
        // identity's minimum and only the list itself is wrong.
        let replacement = embedded_decks()
            .into_iter()
            .filter(|deck| deck.side == Side::Runner && deck.category == DeckCategory::Sample)
            .flat_map(|deck| deck.cards)
            .map(|entry| entry.card)
            .find(|card| {
                altered.cards.iter().all(|entry| entry.card != *card)
                    && registry.get(card).is_some_and(|definition| definition.deck_limit.unwrap_or(3) >= altered.cards[0].count)
            })
            .expect("a card the starter lacks");
        altered.cards[0].card = replacement;
        altered.category = DeckCategory::Starter;
        assert_eq!(
            altered.validate(&registry, NsgFormat::Standard).err(),
            Some(DeckError::StarterIdentity(altered.identity.clone())),
            "a starter identity under a list no pack contains, whatever its file says it is"
        );
    }

    /// The builder's running totals are the gate's numbers: on every legal
    /// embedded deck the tally agrees with the validator's report, and on a
    /// half-built one it still reports where the list stands.
    #[test]
    fn a_tally_agrees_with_the_validator_and_survives_an_illegal_deck() {
        let registry = registry();
        for deck in embedded_decks() {
            let report = deck.validate(&registry, deck.category.format()).unwrap();
            let tally = deck.tally(&registry).unwrap();
            assert_eq!(tally.size, report.deck_size, "{}", deck.id);
            assert_eq!(tally.influence_spent, report.influence_spent, "{}", deck.id);
            assert_eq!(tally.agenda.map(|agenda| agenda.points), report.agenda_points, "{}", deck.id);
            assert!(tally.size >= tally.min_size, "{}", deck.id);
        }
        let mut half = by_id("brick_stack").unwrap();
        half.cards.truncate(3);
        assert!(half.validate(&registry, NsgFormat::Startup).is_err(), "too small to be legal");
        let tally = half.tally(&registry).unwrap();
        assert_eq!(tally.size, half.size());
        assert_eq!(tally.min_size, 40);
        assert!(tally.agenda.is_some(), "a Corp deck reports its agenda range");
        assert_eq!(tally.influence_limit, Some(15));
    }

    /// Every published decklist this crate embeds, pinned to what Null
    /// Signal Games printed or the player published: id, side, card count, influence spent
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
        ("fine_print", Side::Corp, 44, 15, 18),
        ("flow_and_ebb", Side::Runner, 40, 15, 0),
        ("gimbatul", Side::Corp, 49, 15, 20),
        ("glyph_of_warding", Side::Corp, 49, 15, 20),
        ("hidden_funds", Side::Corp, 44, 15, 18),
        ("hyper_velocity", Side::Corp, 44, 15, 18),
        ("not_so_subtle", Side::Corp, 49, 15, 20),
        ("party_hard", Side::Runner, 40, 14, 0),
        ("peculiarity", Side::Corp, 49, 15, 20),
        ("planning_ahead", Side::Runner, 40, 15, 0),
        ("pork_chops", Side::Corp, 49, 15, 20),
        ("prick_thyself", Side::Runner, 45, 15, 0),
        ("professional_opportunities", Side::Runner, 45, 15, 0),
        ("quick_and_dirty", Side::Corp, 44, 15, 18),
        ("quick_returns", Side::Corp, 48, 15, 20),
        ("sabbatical", Side::Runner, 45, 15, 0),
        ("shootin_n_lootin", Side::Runner, 45, 15, 0),
        ("stolen_goods", Side::Runner, 40, 14, 0),
        ("tickets_please", Side::Runner, 40, 15, 0),
        // The Standard tournament lists, as NetrunnerDB publishes them
        // (`scripts/tournament_decks.py`): one per identity in the 2026
        // World Championship's top cut, the best-placed list published from
        // that event and the three 2026 Online Continentals.
        ("tournament_au_co_the_gold_standard_in_clones", Side::Corp, 49, 14, 20),
        ("tournament_haas_bioroid_precision_design", Side::Corp, 44, 13, 18),
        ("tournament_leo_construction_labor_solutions", Side::Corp, 49, 12, 20),
        ("tournament_magdalene_keino_chemutai_cryptarchitect", Side::Runner, 56, 15, 0),
        ("tournament_muslihat_multifarious_marketeer", Side::Runner, 45, 12, 0),
        ("tournament_nbn_reality_plus", Side::Corp, 44, 13, 18),
        ("tournament_nebula_talent_management_making_stars", Side::Corp, 49, 15, 20),
        ("tournament_nuvem_sa_law_of_the_land", Side::Corp, 54, 12, 22),
        ("tournament_ob_superheavy_logistics_extract_export_excel", Side::Corp, 49, 15, 20),
        ("tournament_rene_loup_arcemont_party_animal", Side::Runner, 51, 15, 0),
        ("tournament_sebastiao_souza_pessoa_activist_organizer", Side::Runner, 60, 14, 0),
        ("tournament_virtual_intelligence_p_i_you_can_call_me_vic", Side::Runner, 46, 14, 0),
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
            // Casual: what this pins is the list — its size, influence and
            // points — and a later ban list does not change the list.
            let report = deck.validate(&registry, NsgFormat::Casual).expect("published decks are legal");
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

    /// The format filter keeps `matchups()`'s order, so a Casual pass is the
    /// whole pool unchanged and a Startup pass is the 9 × 10 sample decks
    /// NetrunnerDB's Startup list admits (Stage 0b pinned which nine it
    /// refuses). Both counts are load-bearing: every recorded Casual number
    /// was taken over `matchups()`, and the Startup pass is what Phase 5
    /// §25 measures Startup play over.
    #[test]
    fn matchups_in_a_format_keep_the_pairings_both_decks_are_legal_in() {
        let registry = registry();
        assert_eq!(matchups_in(NsgFormat::Casual, &registry), matchups(), "Casual is the whole pool, in order");
        let startup = matchups_in(NsgFormat::Startup, &registry);
        assert_eq!(startup.len(), 90, "9 Startup-legal Corp decks x 10 Startup-legal Runner decks");
        for (corp, runner) in &startup {
            assert!(corp.validate(&registry, NsgFormat::Startup).is_ok(), "{}", corp.id);
            assert!(runner.validate(&registry, NsgFormat::Startup).is_ok(), "{}", runner.id);
        }
        let order: Vec<_> = matchups().into_iter().filter(|pair| startup.contains(pair)).collect();
        assert_eq!(startup, order, "the filter never reorders");
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
    fn to_decklist_sums_each_cards_copies_under_its_id() {
        let registry = registry();
        let deck = by_id("party_hard").expect("party_hard is embedded");

        let decklist = deck.to_decklist(&registry).expect("every sample card is in the registry");

        assert_eq!(decklist.identity, CardId("rene_loup_arcemont_party_animal".to_string()));
        assert_eq!(decklist.cards.values().sum::<u32>(), deck.size());
        for entry in &deck.cards {
            let copies: u32 = deck.cards.iter().filter(|other| other.card == entry.card).map(|other| other.count).sum();
            assert_eq!(decklist.cards.get(&entry.card), Some(&copies), "{}", entry.card.0);
        }
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
            Err(DeckError::Illegal(DeckValidationError::NotInPool { card, format })) => {
                assert_eq!((card.0.as_str(), format), ("ice_wall", NsgFormat::Startup), "Ice Wall is printed only in the Core Set");
            }
            other => panic!("expected Ice Wall to be outside Startup's pool, got {other:?}"),
        }
    }
}
