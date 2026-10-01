//! The NetrunnerDB catalog, embedded: every card of every embedded set, its
//! printings, and the sets (NSG card-pool plan, Stage 0d).
//!
//! **A card is not a printing.** NetrunnerDB's v3 API splits them, and so
//! does this module: a card (`dsl::CardId`, the v3 slug, `"hedge_fund"`) is
//! the rules object — its title, text, numbers, faction and influence — and
//! a printing (`card::PrintingId`, the code, `30075`) is that card as one
//! set prints it, with its place in the set, its illustrator, its flavour and
//! its picture. The catalog used to be NetrunnerDB's v2 card arrays, one
//! entry per printing, so a card file named one printing (`numeric_id`) and
//! that one number decided its metadata, its set, its picture and its
//! legality; a reprint was joined back to its card by title, and a format's
//! pool listed every printing of every card in it. Now a card is in a pool,
//! banned or built by its id, and a printing decides only what a printing
//! is: the picture, the set a person sees it under, the art they choose.
//!
//! **Printings are a side table, not a field of `CardDefinition`.** A
//! registry is cloned into every bot sample, and a list of printings on each
//! definition would ride along for nothing; a client that wants them asks
//! here (`printings_of`, `latest_printing`).
//!
//! The data is `data/catalog`, written by `scripts/catalog_sync.py` from the
//! v3 API and embedded by `build.rs`, so this does no I/O. A failure to parse
//! is an authoring bug in a committed file, not a runtime condition, so it
//! panics, the way `cards::embedded` does.
//!
//! Catalog-only definitions (`cards()`) are `is_playable: false` and carry no
//! DSL; the playable cards in `data/{corp,runner}` take their printed
//! metadata from here by id (`cards::embedded::fill_catalog_metadata`).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::card::{CardConversionError, Faction, PrintingId};
use crate::cards::CardRegistry;
use crate::dsl::{CardDefinition, CardId, CardSubtype, CardType, IceType};
use crate::rules::Side;

/// Every embedded set's file under `data/catalog/cards`, concatenated by
/// `build.rs` into one array.
const CATALOG_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/catalog.json"));
const SETS_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/catalog/sets.json"));

/// A card set by its v3 id (`"system_gateway"`). A string rather than a
/// type of its own because nothing but a lookup ever reads one.
pub type SetId = String;

/// One embedded card set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSet {
    pub id: SetId,
    pub name: String,
    /// NetrunnerDB's v2 code for the set (`"sg"`), kept because the
    /// observation vocabulary's reserved blocks were laid out by it.
    pub legacy_code: String,
    /// The set's cycle by v3 id (`"system_gateway"`, `"liberation"`). The
    /// committed icon font names each set's mark by its cycle, with `-`
    /// where v3 writes `_`.
    pub cycle: String,
    /// `YYYY-MM-DD`, so the string orders as the date does.
    pub date_release: String,
    pub position: u32,
    pub size: u32,
    pub first_printing: PrintingId,
}

/// One card as one set prints it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Printing {
    pub id: PrintingId,
    pub card: CardId,
    pub set: SetId,
    /// The card's number in its set.
    pub position: u32,
    /// How many copies the set holds.
    pub quantity: u32,
    pub illustrators: Option<String>,
    /// The flavour text this printing carries, its other faces' and the
    /// card's design credit included (`catalog_sync.py` folds them), with
    /// NetrunnerDB's markup removed.
    pub flavor: Option<String>,
    /// Whether NetrunnerDB holds a high-resolution scan of it, so an image
    /// download need not ask for one that does not exist.
    pub has_xlarge: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetFile {
    /// The set the file is, by v3 id: a printing's set is the file it is
    /// written in.
    id: String,
    cards: Vec<CardDto>,
    printings: Vec<PrintingDto>,
}

/// A card as `catalog_sync.py` writes it: v3's attributes, by v3's names.
/// `cost` and `advancement_requirement` are strings because a card can
/// print X.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CardDto {
    id: String,
    title: String,
    side_id: String,
    faction_id: String,
    card_type_id: String,
    display_subtypes: Option<String>,
    text: Option<String>,
    cost: Option<String>,
    strength: Option<i64>,
    advancement_requirement: Option<String>,
    agenda_points: Option<i64>,
    trash_cost: Option<i64>,
    memory_cost: Option<i64>,
    base_link: Option<i64>,
    influence_cost: Option<i64>,
    influence_limit: Option<i64>,
    minimum_deck_size: Option<i64>,
    deck_limit: Option<i64>,
    is_unique: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrintingDto {
    id: String,
    card_id: String,
    position: u32,
    quantity: u32,
    illustrators: Option<String>,
    flavor: Option<String>,
    has_xlarge: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetDto {
    id: String,
    name: String,
    legacy_code: String,
    card_cycle_id: String,
    date_release: String,
    position: u32,
    size: u32,
    first_printing_id: String,
}

fn printing_id(text: &str) -> Result<PrintingId, CardConversionError> {
    text.parse().map(PrintingId).map_err(|_| CardConversionError::InvalidPrintingId(text.to_string()))
}

fn number(field: &'static str, value: Option<i64>) -> Result<Option<u32>, CardConversionError> {
    value
        .map(|v| u32::try_from(v).map_err(|_| CardConversionError::NotANumber { field, value: v.to_string() }))
        .transpose()
}

/// A number v3 writes as a string: `"5"`, or `"X"`, which is no fixed
/// number and reads as none — the v2 catalog's `null` for the same card.
fn printed_number(field: &'static str, value: Option<&str>) -> Result<Option<u32>, CardConversionError> {
    match value {
        None | Some("X") => Ok(None),
        Some(text) => text.parse().map(Some).map_err(|_| CardConversionError::NotANumber { field, value: text.to_string() }),
    }
}

fn parse_faction(faction_id: &str) -> Result<Faction, CardConversionError> {
    match faction_id {
        "anarch" => Ok(Faction::Anarch),
        "criminal" => Ok(Faction::Criminal),
        "shaper" => Ok(Faction::Shaper),
        "haas_bioroid" => Ok(Faction::HaasBioroid),
        "jinteki" => Ok(Faction::Jinteki),
        "nbn" => Ok(Faction::Nbn),
        "weyland_consortium" => Ok(Faction::WeylandConsortium),
        "neutral_corp" => Ok(Faction::NeutralCorp),
        "neutral_runner" => Ok(Faction::NeutralRunner),
        other => Err(CardConversionError::UnknownFaction(other.to_string())),
    }
}

fn parse_side(side_id: &str) -> Result<Side, CardConversionError> {
    match side_id {
        "corp" => Ok(Side::Corp),
        "runner" => Ok(Side::Runner),
        other => Err(CardConversionError::UnknownSide(other.to_string())),
    }
}

fn parse_keywords(keywords: Option<&str>) -> Vec<String> {
    keywords
        .unwrap_or_default()
        .split(" - ")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

/// NetrunnerDB's `ice` type carries no `IceType` payload of its own — the
/// type is a subtype (`"Sentry - Bioroid - Destroyer"`). The first of
/// Barrier, Code Gate and Sentry the subtypes name, and `Other` for ice
/// that names none (Vicsek's `"Trap - AP - Observer"`, a Mythic). Hafrún,
/// the one printing that names two (`"Barrier - Code Gate"`), reads as its
/// first until Parhelion builds it; a card that prints two types is that
/// tranche's question.
fn infer_ice_type(keywords: &[String]) -> IceType {
    keywords
        .iter()
        .find_map(|keyword| match keyword.as_str() {
            "Barrier" => Some(IceType::Barrier),
            "Code Gate" => Some(IceType::CodeGate),
            "Sentry" => Some(IceType::Sentry),
            _ => None,
        })
        .unwrap_or(IceType::Other)
}

/// v3 types an identity by side (`corp_identity`, `runner_identity`); the
/// side is a field of its own here.
fn parse_card_type(card_type_id: &str, keywords: &[String]) -> Result<CardType, CardConversionError> {
    match card_type_id {
        "corp_identity" | "runner_identity" => Ok(CardType::Identity),
        "agenda" => Ok(CardType::Agenda),
        "asset" => Ok(CardType::Asset),
        "ice" => Ok(CardType::Ice(infer_ice_type(keywords))),
        "operation" => Ok(CardType::Operation),
        "upgrade" => Ok(CardType::Upgrade),
        "event" => Ok(CardType::Event),
        "hardware" => Ok(CardType::Hardware),
        "resource" => Ok(CardType::Resource),
        "program" => Ok(CardType::Program),
        other => Err(CardConversionError::UnknownCardType(other.to_string())),
    }
}

fn type_display(card_type: &CardType) -> &'static str {
    match card_type {
        CardType::Agenda => "Agenda",
        CardType::Asset => "Asset",
        CardType::Operation => "Operation",
        CardType::Ice(_) => "Ice",
        CardType::Hardware => "Hardware",
        CardType::Resource => "Resource",
        CardType::Program => "Program",
        CardType::Event => "Event",
        CardType::Identity => "Identity",
        CardType::Upgrade => "Upgrade",
    }
}

/// Synthesizes a human-readable type line, e.g. `"Program: Icebreaker -
/// Killer"` — NetrunnerDB's API carries no combined field to copy this from.
fn build_type_line(card_type: &CardType, keywords: &[String]) -> String {
    if keywords.is_empty() {
        type_display(card_type).to_string()
    } else {
        format!("{}: {}", type_display(card_type), keywords.join(" - "))
    }
}

/// NetrunnerDB's `text` with its HTML tags removed and everything else
/// kept: the line breaks, and the `[subroutine]`, `[click]`, `[credit]`
/// symbols this codebase quotes verbatim. Preferred over the API's own
/// `stripped_text`, which flattens `[subroutine]` to the word "Subroutine"
/// and joins every line into one — fine for search, wrong for reading.
///
/// A list item (`<li>`, the "Resolve 1 of the following:" cards) becomes
/// its own line with a bullet. Dropping the tag alone ran the options
/// together — "choice:Gain 6[credit].Draw 4 cards." — which is what a
/// card face showed until the first one was drawn. `<strong>` and `<em>`
/// are still dropped: the emphasis is typographic.
fn strip_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut tag = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                if tag == "li" {
                    out.push_str("\n• ");
                }
            }
            _ if in_tag => tag.push(ch),
            _ => out.push(ch),
        }
    }
    out.trim().to_string()
}

fn convert_card(dto: CardDto) -> Result<CardDefinition, CardConversionError> {
    let keywords = parse_keywords(dto.display_subtypes.as_deref());
    let card_type = parse_card_type(&dto.card_type_id, &keywords)?;
    Ok(CardDefinition {
        id: CardId(dto.id),
        title: dto.title,
        side: parse_side(&dto.side_id)?,
        cost: printed_number("cost", dto.cost.as_deref())?.unwrap_or(0),
        trash_cost: number("trash_cost", dto.trash_cost)?,
        advancement_requirement: printed_number("advancement_requirement", dto.advancement_requirement.as_deref())?,
        agenda_points: number("agenda_points", dto.agenda_points)?,
        min_deck_size: number("minimum_deck_size", dto.minimum_deck_size)?,
        strength: number("strength", dto.strength)?.map(|v| v as i32),
        // Every subtype is one the rules list
        // (`every_catalog_keyword_is_a_subtype_the_rules_list`).
        subtypes: keywords.iter().filter_map(|keyword| CardSubtype::from_printed(keyword)).collect(),
        unique: dto.is_unique,
        base_link: number("base_link", dto.base_link)?,
        memory_cost: number("memory_cost", dto.memory_cost)?,
        faction: Some(parse_faction(&dto.faction_id)?),
        type_line: Some(build_type_line(&card_type, &keywords)),
        card_type,
        keywords,
        influence_cost: number("influence_cost", dto.influence_cost)?,
        influence_limit: number("influence_limit", dto.influence_limit)?,
        deck_limit: number("deck_limit", dto.deck_limit)?,
        printed_text: dto.text.as_deref().map(strip_markup),
        is_playable: false,
        ..CardDefinition::default()
    })
}

struct Catalog {
    cards: CardRegistry,
    /// Sorted by id.
    printings: Vec<Printing>,
    by_id: HashMap<PrintingId, usize>,
    /// Each card's printings, newest first.
    of_card: HashMap<CardId, Vec<usize>>,
    /// Newest first.
    sets: Vec<CardSet>,
}

fn load() -> Result<Catalog, String> {
    let sets: Vec<SetDto> = serde_json::from_str(SETS_JSON).map_err(|e| format!("sets.json: {e}"))?;
    let mut sets: Vec<CardSet> = sets
        .into_iter()
        .map(|set| {
            Ok(CardSet {
                first_printing: printing_id(&set.first_printing_id)?,
                id: set.id,
                name: set.name,
                legacy_code: set.legacy_code,
                cycle: set.card_cycle_id,
                date_release: set.date_release,
                position: set.position,
                size: set.size,
            })
        })
        .collect::<Result<_, CardConversionError>>()
        .map_err(|e| e.to_string())?;
    // A tie in release date (System Gateway and System Update 2021 share
    // one) is broken by the first printing's code, the later set numbered
    // above the earlier.
    sets.sort_by(|a, b| (&b.date_release, b.first_printing).cmp(&(&a.date_release, a.first_printing)));
    let rank: HashMap<&str, usize> = sets.iter().enumerate().map(|(index, set)| (set.id.as_str(), index)).collect();

    let files: Vec<SetFile> = serde_json::from_str(CATALOG_JSON).map_err(|e| format!("catalog: {e}"))?;
    let mut filed: Vec<&str> = files.iter().map(|file| file.id.as_str()).collect();
    let mut listed: Vec<&str> = sets.iter().map(|set| set.id.as_str()).collect();
    filed.sort_unstable();
    listed.sort_unstable();
    if filed != listed {
        return Err(format!("the set files {filed:?} are not the sets in sets.json {listed:?}"));
    }

    let mut cards = CardRegistry::new();
    let mut printings = Vec::new();
    for file in files {
        let set = file.id;
        for dto in file.cards {
            let id = dto.id.clone();
            let card = convert_card(dto).map_err(|e| format!("{id}: {e}"))?;
            if cards.get(&card.id).is_some() {
                return Err(format!("{id} is written in more than one set's file"));
            }
            cards.insert(card);
        }
        for dto in file.printings {
            printings.push(Printing {
                id: printing_id(&dto.id).map_err(|e| e.to_string())?,
                card: CardId(dto.card_id),
                set: set.clone(),
                position: dto.position,
                quantity: dto.quantity,
                illustrators: dto.illustrators,
                flavor: dto.flavor.as_deref().map(strip_markup),
                has_xlarge: dto.has_xlarge,
            });
        }
    }
    printings.sort_by_key(|printing| printing.id);
    let by_id = printings.iter().enumerate().map(|(index, printing)| (printing.id, index)).collect();
    let mut of_card: HashMap<CardId, Vec<usize>> = HashMap::new();
    for (index, printing) in printings.iter().enumerate() {
        if cards.get(&printing.card).is_none() {
            return Err(format!("printing {} is of {}, which no set's file writes", printing.id, printing.card.0));
        }
        of_card.entry(printing.card.clone()).or_default().push(index);
    }
    for indices in of_card.values_mut() {
        indices.sort_by_key(|&index| (rank[printings[index].set.as_str()], std::cmp::Reverse(printings[index].id)));
    }
    Ok(Catalog { cards, printings, by_id, of_card, sets })
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| load().unwrap_or_else(|e| panic!("the embedded NetrunnerDB catalog: {e}")))
}

/// Every card of every embedded set, one catalog-only definition each
/// (`is_playable: false`, no DSL), by v3 id.
pub fn cards() -> &'static CardRegistry {
    &catalog().cards
}

/// Every embedded printing, by code.
pub fn printings() -> impl Iterator<Item = &'static Printing> {
    catalog().printings.iter()
}

pub fn printing(id: PrintingId) -> Option<&'static Printing> {
    let catalog = catalog();
    catalog.by_id.get(&id).map(|&index| &catalog.printings[index])
}

/// A card's embedded printings, newest set first — v3's own order for a
/// card's `printing_ids`. Empty for a card the catalog does not know
/// (homebrew, a test fixture).
pub fn printings_of(card: &CardId) -> impl Iterator<Item = &'static Printing> {
    let catalog = catalog();
    catalog.of_card.get(card).into_iter().flatten().map(|&index| &catalog.printings[index])
}

/// The newest embedded printing of a card: the one a client shows when the
/// person has chosen none.
pub fn latest_printing(card: &CardId) -> Option<&'static Printing> {
    printings_of(card).next()
}

/// Whether any printing of `card` is in `set`.
pub fn printed_in(card: &CardId, set: &str) -> bool {
    printings_of(card).any(|printing| printing.set == set)
}

pub fn set(id: &str) -> Option<&'static CardSet> {
    catalog().sets.iter().find(|set| set.id == id)
}

/// Every embedded set, newest first by release date.
pub fn sets() -> &'static [CardSet] {
    &catalog().sets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_stripped_and_the_symbols_and_line_breaks_are_kept() {
        assert_eq!(strip_markup("Whenever you install a <strong>virus</strong> program"), "Whenever you install a virus program");
        assert_eq!(strip_markup("[subroutine] Do 1 net damage.\n[subroutine] Gain 1[credit]."), "[subroutine] Do 1 net damage.\n[subroutine] Gain 1[credit].");
        assert_eq!(strip_markup("a < b"), "a", "a stray angle bracket swallows to the end, which no card text has");
    }

    /// Wildcat Strike: each option on its own bulleted line, and no blank
    /// line where the `<ul>` was.
    #[test]
    fn list_items_become_bulleted_lines() {
        assert_eq!(
            strip_markup("Resolve 1 of the following of the Corpʼs choice:<ul><li>Gain 6[credit].</li><li>Draw 4 cards.</li></ul>"),
            "Resolve 1 of the following of the Corpʼs choice:\n• Gain 6[credit].\n• Draw 4 cards."
        );
    }

    fn base_dto() -> CardDto {
        CardDto {
            id: "ansel_1_0".to_string(),
            title: "Ansel 1.0".to_string(),
            side_id: "corp".to_string(),
            faction_id: "haas_bioroid".to_string(),
            card_type_id: "ice".to_string(),
            display_subtypes: Some("Sentry - Bioroid - Destroyer".to_string()),
            text: Some("Lose click: Break 1 subroutine on this ice.".to_string()),
            cost: Some("6".to_string()),
            strength: Some(4),
            advancement_requirement: None,
            agenda_points: None,
            trash_cost: None,
            memory_cost: None,
            base_link: None,
            influence_cost: Some(3),
            influence_limit: None,
            minimum_deck_size: None,
            deck_limit: Some(3),
            is_unique: false,
        }
    }

    /// A card's subtypes are its printed keywords, so every keyword in the
    /// embedded catalog must be one of Comprehensive Rules 2.16.7's words.
    /// One that is not is a printing newer than `rules/`, or a misspelling
    /// in `CardSubtype`: either way the card would silently lose a
    /// subtype a card reads.
    #[test]
    fn every_catalog_keyword_is_a_subtype_the_rules_list() {
        let unknown: std::collections::BTreeSet<&str> = cards()
            .iter()
            .flat_map(|card| card.keywords.iter())
            .filter(|keyword| CardSubtype::from_printed(keyword).is_none())
            .map(String::as_str)
            .collect();
        assert!(unknown.is_empty(), "keywords CardSubtype does not list: {unknown:?}");
        let paywall = cards().get(&CardId("paywall".to_string())).expect("Paywall");
        assert_eq!(paywall.subtypes, vec![CardSubtype::Barrier]);
        assert_eq!(CardSubtype::CodeGate.printed(), "Code Gate");
        assert_eq!(CardSubtype::from_printed("G-mod"), Some(CardSubtype::GMod));
    }

    #[test]
    fn converts_a_valid_ice() {
        let def = convert_card(base_dto()).expect("valid conversion");
        assert_eq!(def.id, CardId("ansel_1_0".to_string()));
        assert_eq!(def.card_type, CardType::Ice(IceType::Sentry));
        assert_eq!(def.faction, Some(Faction::HaasBioroid));
        assert_eq!(def.side, Side::Corp);
        assert_eq!(def.cost, 6);
        assert_eq!(def.strength, Some(4));
        assert_eq!(def.influence_cost, Some(3));
        assert_eq!(def.keywords, vec!["Sentry", "Bioroid", "Destroyer"]);
        assert_eq!(def.type_line.as_deref(), Some("Ice: Sentry - Bioroid - Destroyer"));
        assert_eq!(def.deck_limit, Some(3));
        assert_eq!(def.built_from, None, "a catalog card is built from nothing");
        assert!(!def.is_playable);
        assert!(def.triggers.is_empty());
    }

    #[test]
    fn converts_each_card_type() {
        for (type_id, keywords, expected) in [
            ("corp_identity", None, CardType::Identity),
            ("runner_identity", None, CardType::Identity),
            ("agenda", None, CardType::Agenda),
            ("asset", None, CardType::Asset),
            ("ice", Some("Barrier"), CardType::Ice(IceType::Barrier)),
            ("ice", Some("Code Gate"), CardType::Ice(IceType::CodeGate)),
            ("ice", Some("Sentry"), CardType::Ice(IceType::Sentry)),
            ("operation", None, CardType::Operation),
            ("upgrade", None, CardType::Upgrade),
            ("event", None, CardType::Event),
            ("hardware", None, CardType::Hardware),
            ("resource", None, CardType::Resource),
            ("program", None, CardType::Program),
        ] {
            let mut dto = base_dto();
            dto.card_type_id = type_id.to_string();
            dto.display_subtypes = keywords.map(str::to_string);
            assert_eq!(convert_card(dto).expect("valid conversion").card_type, expected);
        }
    }

    #[test]
    fn ice_that_prints_none_of_the_three_types_is_other() {
        for keywords in ["Trap - AP - Observer", "Mythic", "Mythic - Destroyer"] {
            let mut dto = base_dto();
            dto.display_subtypes = Some(keywords.to_string());
            assert_eq!(convert_card(dto).expect("valid conversion").card_type, CardType::Ice(IceType::Other), "{keywords}");
        }
    }

    /// An X is no fixed number: Psychographics costs X and Blood in the
    /// Water advances for X, and each reads as none, as v2's `null` did.
    #[test]
    fn a_printed_x_is_no_number() {
        assert_eq!(cards().get(&CardId("psychographics".to_string())).expect("Psychographics").cost, 0);
        assert_eq!(cards().get(&CardId("blood_in_the_water".to_string())).expect("Blood in the Water").advancement_requirement, None);
        let mut dto = base_dto();
        dto.cost = Some("many".to_string());
        assert_eq!(convert_card(dto), Err(CardConversionError::NotANumber { field: "cost", value: "many".to_string() }));
    }

    #[test]
    fn rejects_what_the_engine_does_not_model() {
        let mut dto = base_dto();
        dto.card_type_id = "vehicle".to_string();
        assert_eq!(convert_card(dto), Err(CardConversionError::UnknownCardType("vehicle".to_string())));
        let mut dto = base_dto();
        dto.faction_id = "apex".to_string();
        assert_eq!(convert_card(dto), Err(CardConversionError::UnknownFaction("apex".to_string())));
        let mut dto = base_dto();
        dto.side_id = "both".to_string();
        assert_eq!(convert_card(dto), Err(CardConversionError::UnknownSide("both".to_string())));
        let mut dto = base_dto();
        dto.strength = Some(-1);
        assert_eq!(convert_card(dto), Err(CardConversionError::NotANumber { field: "strength", value: "-1".to_string() }));
        assert_eq!(printing_id("not-a-code"), Err(CardConversionError::InvalidPrintingId("not-a-code".to_string())));
    }

    /// Fifteen sets, 846 printings of 797 cards. Exact rather than a floor:
    /// a set that failed to embed would have passed a floor for as long as
    /// the others outnumbered it.
    #[test]
    fn the_embedded_catalog_holds_the_known_counts() {
        assert_eq!(sets().len(), 15);
        assert_eq!(printings().count(), 846);
        assert_eq!(cards().len(), 797);
        assert!(cards().iter().all(|card| printings_of(&card.id).next().is_some()), "every card has a printing");
    }

    /// A reprint is a printing of the same card: Hedge Fund is one card,
    /// printed in System Gateway (30075) and the Core Set (01110), newest
    /// first.
    #[test]
    fn a_card_is_found_by_its_id_and_its_printings_newest_first() {
        let hedge_fund = CardId("hedge_fund".to_string());
        assert_eq!(cards().get(&hedge_fund).expect("Hedge Fund").title, "Hedge Fund");
        let codes: Vec<u32> = printings_of(&hedge_fund).map(|printing| printing.id.0).collect();
        assert_eq!(codes, [30075, 1110]);
        assert_eq!(latest_printing(&hedge_fund).map(|printing| printing.set.as_str()), Some("system_gateway"));
        assert!(printed_in(&hedge_fund, "core_set"));
        let wildcat = printing(PrintingId(30002)).expect("Wildcat Strike's printing");
        assert_eq!(wildcat.card, CardId("wildcat_strike".to_string()));
        assert_eq!((wildcat.set.as_str(), wildcat.position, wildcat.quantity), ("system_gateway", 2, 3));
        assert!(printing(PrintingId(999_999)).is_none());
        assert!(cards().get(&CardId("not_a_real_card".to_string())).is_none());
        assert_eq!(printings_of(&CardId("not_a_real_card".to_string())).count(), 0);
    }

    /// Newest first by release date, System Update 2021 ahead of System
    /// Gateway on the day they share.
    #[test]
    fn sets_are_newest_first() {
        let order: Vec<&str> = sets().iter().map(|set| set.legacy_code.as_str()).collect();
        assert_eq!(order, ["vp", "elev", "rwr", "tai", "ph", "ms", "msbp", "su21", "sg", "sm", "ur", "urbp", "mor", "df", "core"]);
        let sg = set("system_gateway").expect("System Gateway");
        assert_eq!((sg.name.as_str(), sg.cycle.as_str(), sg.first_printing), ("System Gateway", "system_gateway", PrintingId(30001)));
        assert!(set("sg").is_none(), "a set is found by its v3 id");
    }

    /// A card with more than one face carries every face's text, as v2
    /// spelled it, because a card file's clauses quote the flip side too.
    #[test]
    fn a_flip_identity_carries_both_faces() {
        let dewi = cards().get(&CardId("dewi_subrotoputri_pedagogical_dhalang".to_string())).expect("Dewi");
        let text = dewi.printed_text.as_deref().expect("text");
        assert!(text.contains("\nFlip side:\nWhenever you make a successful run, if you have at least 1 unused [mu]"), "{text}");
        let flavor = printing(PrintingId(35023)).and_then(|printing| printing.flavor.as_deref());
        assert_eq!(flavor, Some("Who else will teach the stories of good and evil?\nWe can’t leave the corps’ dirty deeds in the shadows."));
    }
}
