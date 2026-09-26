//! Converts NetrunnerDB wire-format card data (`card::NetrunnerDbCardDto`)
//! into the unified `dsl::CardDefinition`/`cards::CardRegistry` model.
//! Produces catalog-only entries (`is_playable: false`, no DSL trigger/
//! ability data) cross-referenced by `numeric_id` — distinct from the
//! hand-authored, `is_playable: true` baseline set in `cards::{corp,runner,
//! identities}`. The embedded default sets (System Gateway, Elevation) and
//! `netrunner_card_sync`'s live/cached NetrunnerDB data both flow through
//! this one conversion path.

use crate::card::{CardConversionError, CardId, Faction, NetrunnerDbCardDto};
use crate::cards::CardRegistry;
use crate::dsl::{CardDefinition, CardSubtype, CardType, IceType};
use crate::rules::Side;

/// Every embedded pack's NetrunnerDB card data, one array per pack
/// (`data/cards/<pack>.json`, concatenated by `build.rs`). The Core Set is
/// among them not because it is a current competitive pool — `format.rs`
/// keeps `"core"` out of Startup — but because the baseline cards this repo
/// started from are Core Set printings, and without their catalog entries
/// they carry no faction, influence cost, deck limit or set code, which is
/// everything deckbuilding legality is computed from.
const CATALOG_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/catalog.json"));

#[derive(Debug, thiserror::Error)]
pub enum EmbeddedSetsError {
    #[error("failed to parse embedded card catalog JSON: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("failed to convert card at index {index}: {source}")]
    Conversion { index: usize, source: CardConversionError },
}

fn non_negative(field: &'static str, value: Option<i32>) -> Result<Option<u32>, CardConversionError> {
    match value {
        None => Ok(None),
        Some(v) if v >= 0 => Ok(Some(v as u32)),
        Some(v) => Err(CardConversionError::NegativeValue { field, value: v }),
    }
}

fn parse_faction(faction_code: &str) -> Result<Faction, CardConversionError> {
    match faction_code {
        "anarch" => Ok(Faction::Anarch),
        "criminal" => Ok(Faction::Criminal),
        "shaper" => Ok(Faction::Shaper),
        "haas-bioroid" => Ok(Faction::HaasBioroid),
        "jinteki" => Ok(Faction::Jinteki),
        "nbn" => Ok(Faction::Nbn),
        "weyland-consortium" => Ok(Faction::WeylandConsortium),
        "neutral-corp" => Ok(Faction::NeutralCorp),
        "neutral-runner" => Ok(Faction::NeutralRunner),
        other => Err(CardConversionError::UnknownFaction(other.to_string())),
    }
}

fn parse_side(side_code: &str) -> Result<Side, CardConversionError> {
    match side_code {
        "corp" => Ok(Side::Corp),
        "runner" => Ok(Side::Runner),
        other => Err(CardConversionError::UnknownSide(other.to_string())),
    }
}

fn parse_keywords(keywords: Option<String>) -> Vec<String> {
    keywords
        .unwrap_or_default()
        .split(" - ")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

/// NetrunnerDB's `ice` `type_code` carries no `IceType` payload of its own —
/// the type is a keyword (e.g. `"Sentry - Bioroid - Destroyer"`). The first
/// of Barrier, Code Gate and Sentry the keywords name, and `Other` for ice
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

fn parse_card_type(type_code: &str, keywords: &[String]) -> Result<CardType, CardConversionError> {
    match type_code {
        "identity" => Ok(CardType::Identity),
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
/// are still dropped: the emphasis is typographic, and a client that
/// wants it can read the catalog's raw `text`.
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

fn convert_one(dto: NetrunnerDbCardDto) -> Result<CardDefinition, CardConversionError> {
    let numeric_id =
        dto.code.parse::<u32>().map(CardId).map_err(|_| CardConversionError::InvalidCardCode(dto.code.clone()))?;
    let keywords = parse_keywords(dto.keywords.clone());
    let card_type = parse_card_type(&dto.type_code, &keywords)?;
    let side = parse_side(&dto.side_code)?;

    Ok(CardDefinition {
        id: crate::dsl::CardId(format!("nrdb_{}", numeric_id.0)),
        title: dto.title,
        side,
        card_type: card_type.clone(),
        cost: non_negative("cost", dto.cost)?.unwrap_or(0),
        triggers: Vec::new(),
        abilities: Vec::new(),
        trash_cost: non_negative("trash_cost", dto.trash_cost)?,
        steal_cost: None,
        advancement_requirement: non_negative("advancement_cost", dto.advancement_cost)?,
        agenda_points: non_negative("agenda_points", dto.agenda_points)?,
        min_deck_size: non_negative("minimum_deck_size", dto.minimum_deck_size)?,
        strength: non_negative("strength", dto.strength)?.map(|v| v as i32),
        subroutines: Vec::new(),
        interactive_on_access: None,
        // Every keyword is a subtype the rules list
        // (`every_catalog_keyword_is_a_subtype_the_rules_list`).
        subtypes: keywords.iter().filter_map(|keyword| CardSubtype::from_printed(keyword)).collect(),
        unique: dto.uniqueness.unwrap_or(false),
        base_link: non_negative("base_link", dto.base_link)?,
        play_requirement: None,
        recurring_credits: None,
        memory_cost: non_negative("memory_cost", dto.memory_cost)?,
        counter_kind: None,

        numeric_id: Some(numeric_id),
        faction: Some(parse_faction(&dto.faction_code)?),
        type_line: Some(build_type_line(&card_type, &keywords)),
        keywords,
        set_code: Some(dto.pack_code),
        influence_cost: non_negative("faction_cost", dto.faction_cost)?,
        unlimited_influence: false,
        influence_limit: dto.influence_limit.and_then(|limit| u32::try_from(limit).ok()),
        deck_limit: non_negative("deck_limit", dto.deck_limit)?,
        artist: dto.illustrator,
        printed_text: dto.text.as_deref().map(strip_markup),
        flavor: dto.flavor.as_deref().map(strip_markup),
        image_url: None,
        additional_play_cost: None,
        removed_after_play: false,
        pays_for: Vec::new(),
        trash_when_empty: false,
        may_install_agendas_faceup: false,
        rez_alternatives: Vec::new(),
        continuous: Vec::new(),
        installs_on_ice: false, hosted_cards_playable_from_grip: false, dividends: None, playable_from_archives: false, click_breakable: false,  persistent_after_trash: false,
        is_playable: false,
    })
}

/// Converts each DTO via `convert_one`, tagging a failure with its position
/// in `dtos` for a useful error message. Any conversion failure aborts the
/// whole batch — used for the curated embedded fixtures, where a failure is
/// a real bug that should surface loudly.
pub fn convert_dtos(dtos: Vec<NetrunnerDbCardDto>) -> Result<Vec<CardDefinition>, EmbeddedSetsError> {
    dtos.into_iter()
        .enumerate()
        .map(|(index, dto)| convert_one(dto).map_err(|source| EmbeddedSetsError::Conversion { index, source }))
        .collect()
}

/// Same conversion as `convert_dtos`, but best-effort: a card this schema
/// doesn't model (e.g. a mini-faction like Apex/Adam/Sunny-Lebeau, absent
/// from the closed `Faction` enum) is skipped and reported rather than
/// aborting the whole batch. Intended for ingesting NetrunnerDB's full,
/// ever-growing live card list (`netrunner_card_sync`).
pub fn convert_dtos_lenient(dtos: Vec<NetrunnerDbCardDto>) -> (Vec<CardDefinition>, Vec<(usize, CardConversionError)>) {
    let mut definitions = Vec::new();
    let mut skipped = Vec::new();
    for (index, dto) in dtos.into_iter().enumerate() {
        match convert_one(dto) {
            Ok(definition) => definitions.push(definition),
            Err(source) => skipped.push((index, source)),
        }
    }
    (definitions, skipped)
}

/// Parses every embedded catalog JSON fixture (System Gateway, Elevation,
/// Core Set) and returns a `CardRegistry` of their catalog-only
/// (`is_playable: false`) entries. Stays I/O-free — `include_str!` is a
/// compile-time embed, not a runtime filesystem read.
pub fn load_embedded_netrunnerdb_sets() -> Result<CardRegistry, EmbeddedSetsError> {
    let packs: Vec<Vec<NetrunnerDbCardDto>> = serde_json::from_str(CATALOG_JSON)?;
    let mut registry = CardRegistry::new();
    registry.merge(convert_dtos(packs.into_iter().flatten().collect())?);
    Ok(registry)
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

    fn base_dto() -> NetrunnerDbCardDto {
        NetrunnerDbCardDto {
            code: "30038".to_string(),
            title: "Ansel 1.0".to_string(),
            type_code: "ice".to_string(),
            side_code: "corp".to_string(),
            faction_code: "haas-bioroid".to_string(),
            pack_code: "sg".to_string(),
            text: Some("Lose click: Break 1 subroutine on this ice.".to_string()),
            keywords: Some("Sentry - Bioroid - Destroyer".to_string()),
            cost: Some(6),
            strength: Some(4),
            advancement_cost: None,
            agenda_points: None,
            trash_cost: None,
            faction_cost: Some(3),
            memory_cost: None,
            minimum_deck_size: None,
            base_link: None,
            uniqueness: Some(false),
            illustrator: Some("Some Artist".to_string()),
            flavor: None,
            deck_limit: Some(3),
            influence_limit: None,
        }
    }

    /// A card's subtypes are its printed keywords, so every keyword in the
    /// embedded catalog must be one of Comprehensive Rules 2.16.7's words.
    /// One that is not is a printing newer than `rules/`, or a misspelling
    /// in `CardSubtype`: either way the card would silently lose a
    /// subtype a card reads.
    #[test]
    fn every_catalog_keyword_is_a_subtype_the_rules_list() {
        let catalog = load_embedded_netrunnerdb_sets().expect("catalog parses");
        let unknown: std::collections::BTreeSet<&str> = catalog
            .iter()
            .flat_map(|card| card.keywords.iter())
            .filter(|keyword| CardSubtype::from_printed(keyword).is_none())
            .map(String::as_str)
            .collect();
        assert!(unknown.is_empty(), "keywords CardSubtype does not list: {unknown:?}");
        let vicsek_era = catalog.get_by_numeric_id(CardId(36052)).expect("Paywall");
        assert_eq!(vicsek_era.subtypes, vec![CardSubtype::Barrier]);
        assert_eq!(CardSubtype::CodeGate.printed(), "Code Gate");
        assert_eq!(CardSubtype::from_printed("G-mod"), Some(CardSubtype::GMod));
    }

    #[test]
    fn converts_a_valid_ice() {
        let def = convert_one(base_dto()).expect("valid conversion");
        assert_eq!(def.numeric_id, Some(CardId(30038)));
        assert_eq!(def.card_type, CardType::Ice(IceType::Sentry));
        assert_eq!(def.faction, Some(Faction::HaasBioroid));
        assert_eq!(def.side, Side::Corp);
        assert_eq!(def.strength, Some(4));
        assert_eq!(def.influence_cost, Some(3));
        assert_eq!(def.keywords, vec!["Sentry", "Bioroid", "Destroyer"]);
        assert_eq!(def.type_line.as_deref(), Some("Ice: Sentry - Bioroid - Destroyer"));
        assert_eq!(def.artist.as_deref(), Some("Some Artist"));
        assert_eq!(def.deck_limit, Some(3));
        assert!(!def.is_playable);
        assert!(def.triggers.is_empty());
    }

    #[test]
    fn converts_each_card_type() {
        for (type_code, keywords, expected) in [
            ("identity", None, CardType::Identity),
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
            dto.type_code = type_code.to_string();
            dto.keywords = keywords.map(str::to_string);
            let def = convert_one(dto).expect("valid conversion");
            assert_eq!(def.card_type, expected);
        }
    }

    #[test]
    fn ice_that_prints_none_of_the_three_types_is_other() {
        for keywords in ["Trap - AP - Observer", "Mythic", "Mythic - Destroyer"] {
            let mut dto = base_dto();
            dto.keywords = Some(keywords.to_string());
            assert_eq!(convert_one(dto).expect("valid conversion").card_type, CardType::Ice(IceType::Other), "{keywords}");
        }
    }

    #[test]
    fn rejects_unknown_card_type() {
        let mut dto = base_dto();
        dto.type_code = "vehicle".to_string();
        assert_eq!(convert_one(dto), Err(CardConversionError::UnknownCardType("vehicle".to_string())));
    }

    #[test]
    fn rejects_unknown_faction() {
        let mut dto = base_dto();
        dto.faction_code = "brawlers".to_string();
        assert_eq!(convert_one(dto), Err(CardConversionError::UnknownFaction("brawlers".to_string())));
    }

    #[test]
    fn rejects_unknown_side() {
        let mut dto = base_dto();
        dto.side_code = "both".to_string();
        assert_eq!(convert_one(dto), Err(CardConversionError::UnknownSide("both".to_string())));
    }

    #[test]
    fn rejects_invalid_card_code() {
        let mut dto = base_dto();
        dto.code = "not-a-number".to_string();
        assert_eq!(convert_one(dto), Err(CardConversionError::InvalidCardCode("not-a-number".to_string())));
    }

    #[test]
    fn rejects_negative_values() {
        let mut dto = base_dto();
        dto.cost = Some(-1);
        assert_eq!(convert_one(dto), Err(CardConversionError::NegativeValue { field: "cost", value: -1 }));
    }

    #[test]
    fn convert_dtos_lenient_skips_unconvertible_cards_but_keeps_the_rest() {
        let good = NetrunnerDbCardDto {
            code: "1".to_string(),
            title: "Good Card".to_string(),
            type_code: "event".to_string(),
            side_code: "runner".to_string(),
            faction_code: "anarch".to_string(),
            pack_code: "test".to_string(),
            text: None,
            keywords: None,
            cost: None,
            strength: None,
            advancement_cost: None,
            agenda_points: None,
            trash_cost: None,
            faction_cost: None,
            memory_cost: None,
            minimum_deck_size: None,
            base_link: None,
            uniqueness: None,
            illustrator: None,
            flavor: None,
            deck_limit: None,
            influence_limit: None,
        };
        let mut unmodeled_faction = good.clone();
        unmodeled_faction.code = "2".to_string();
        unmodeled_faction.faction_code = "apex".to_string();

        let (defs, skipped) = convert_dtos_lenient(vec![good, unmodeled_faction]);

        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].title, "Good Card");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].0, 1);
    }

    #[test]
    fn load_embedded_netrunnerdb_sets_is_non_empty_and_matches_known_counts() {
        let registry = load_embedded_netrunnerdb_sets().expect("embedded sets should parse");
        // Fifteen packs, 846 printings. Exact rather than a floor: a pack
        // that failed to embed would have passed a floor for as long as the
        // others outnumbered it.
        assert_eq!(registry.len(), 846);
    }

    #[test]
    fn get_by_numeric_id_and_title_hit_and_miss() {
        let registry = load_embedded_netrunnerdb_sets().expect("embedded sets should parse");

        let by_title = registry.get_by_title("Wildcat Strike").expect("known card");
        assert_eq!(by_title.numeric_id, Some(CardId(30002)));
        assert_eq!(registry.get_by_numeric_id(CardId(30002)).unwrap().title, "Wildcat Strike");

        assert!(registry.get_by_numeric_id(CardId(999_999)).is_none());
        assert!(registry.get_by_title("Not A Real Card").is_none());
    }
}
