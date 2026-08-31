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
use crate::dsl::{CardDefinition, CardType, IceType};
use crate::rules::Side;

const SYSTEM_GATEWAY_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/cards/system_gateway.json"));
const ELEVATION_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/cards/elevation.json"));
/// The original 2012 Core Set. Embedded not because any of it is a current
/// competitive pool — `format.rs` deliberately keeps `"core"` out of Startup
/// and Snapshot — but because the baseline hand-authored cards this repo
/// started from are Core Set printings, and without their catalog entries
/// they carry no faction, influence cost, deck limit or set code, which is
/// everything deckbuilding legality is computed from.
const CORE_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/cards/core.json"));

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
/// the subtype only appears as the first segment of the keyword list (e.g.
/// `"Sentry - Bioroid - Destroyer"`). Every ICE in the currently-embedded
/// sets carries exactly one of these three as its first keyword, once
/// `CATALOG_UNMODELABLE` has removed the one printing that carries none.
fn infer_ice_type(keywords: &[String]) -> Option<IceType> {
    match keywords.first().map(String::as_str) {
        Some("Barrier") => Some(IceType::Barrier),
        Some("Code Gate") => Some(IceType::CodeGate),
        Some("Sentry") => Some(IceType::Sentry),
        _ => None,
    }
}

fn parse_card_type(type_code: &str, keywords: &[String]) -> Result<CardType, CardConversionError> {
    match type_code {
        "identity" => Ok(CardType::Identity),
        "agenda" => Ok(CardType::Agenda),
        "asset" => Ok(CardType::Asset),
        "ice" => infer_ice_type(keywords)
            .map(CardType::Ice)
            .ok_or_else(|| CardConversionError::UnrecognizedIceKeywords(keywords.join(" - "))),
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
        subtypes: Vec::new(),
        play_requirement: None,
        recurring_credits: None,
        first_install_discount: None,
        memory_cost: non_negative("memory_cost", dto.memory_cost)?,
        counter_kind: None,

        numeric_id: Some(numeric_id),
        faction: Some(parse_faction(&dto.faction_code)?),
        type_line: Some(build_type_line(&card_type, &keywords)),
        keywords,
        set_code: Some(dto.pack_code),
        influence_cost: non_negative("faction_cost", dto.faction_cost)?,
        deck_limit: non_negative("deck_limit", dto.deck_limit)?,
        artist: dto.illustrator,
        image_url: None,
        memory_bonus: None,
        max_hand_size_bonus: None,
        install_cost_discount_if: None,
        installs_on_ice: false, click_breakable: false, strength_modifier: None, persistent_after_trash: false,
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
    let mut registry = CardRegistry::new();
    for json in [SYSTEM_GATEWAY_JSON, ELEVATION_JSON, CORE_JSON] {
        let dtos: Vec<NetrunnerDbCardDto> = serde_json::from_str(json)?;
        let modelable = dtos.into_iter().filter(|dto| !is_unmodelable(&dto.code)).collect();
        registry.merge(convert_dtos(modelable)?);
    }
    Ok(registry)
}

/// Catalog entries this crate's card schema cannot represent, keyed by
/// NetrunnerDB code with a stated reason. Filtered out *before*
/// `convert_dtos` rather than tolerated by `convert_dtos_lenient`, so an
/// unexpected conversion failure still aborts loudly — the same
/// explicit-exception-set discipline `SG_UNIMPLEMENTED` applies to card
/// coverage, not a place to silence a real gap.
const CATALOG_UNMODELABLE: &[(&str, &str)] = &[(
    "01076",
    "Data Mine — ICE whose keywords are \"Trap - AP\", with no Barrier/Code Gate/Sentry \
     subtype at all. `CardType::Ice` carries a mandatory `IceType`; widening it to an \
     `Option` would ripple through every `restrict_to` match and every ICE card file to \
     accommodate a catalog-only card nothing implements.",
)];

fn is_unmodelable(code: &str) -> bool {
    CATALOG_UNMODELABLE.iter().any(|(excluded, _)| *excluded == code)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            deck_limit: Some(3),
        }
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
    fn rejects_ice_with_unrecognized_keywords() {
        let mut dto = base_dto();
        dto.keywords = Some("Mythic".to_string());
        assert_eq!(
            convert_one(dto),
            Err(CardConversionError::UnrecognizedIceKeywords("Mythic".to_string()))
        );
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
            deck_limit: None,
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
        assert!(!registry.is_empty());
        assert!(registry.len() >= 159, "expected at least the 77 + 82 known System Gateway/Elevation cards");
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
