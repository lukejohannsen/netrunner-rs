use serde::{Deserialize, Serialize};

use crate::card::dto::NetrunnerDbCardDto;
use crate::card::error::CardConversionError;
use crate::card::id::CardId;
use crate::rules::Side;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardType {
    Identity,
    Agenda,
    Asset,
    Ice,
    Operation,
    Upgrade,
    Event,
    Hardware,
    Resource,
    Program,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Faction {
    Anarch,
    Criminal,
    Shaper,
    HaasBioroid,
    Jinteki,
    Nbn,
    WeylandConsortium,
    NeutralCorp,
    NeutralRunner,
}

/// Descriptive NetrunnerDB-backed card metadata — cost/faction/type/oracle
/// text, no parsed ability logic. Distinct from `dsl::Card`, which is the
/// rules engine's authoritative, playable card representation. See
/// `netrunner_core::card` module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardDefinition {
    pub id: CardId,
    pub title: String,
    pub card_type: CardType,
    pub faction: Faction,
    pub side: Side,
    pub pack_code: String,

    pub cost: Option<u32>,
    pub strength: Option<u32>,
    pub advancement_requirement: Option<u32>,
    pub agenda_points: Option<u32>,
    pub trash_cost: Option<u32>,
    pub influence_cost: Option<u32>,
    pub memory_cost: Option<u32>,
    pub min_deck_size: Option<u32>,
    pub base_link: Option<u32>,
    pub unique: bool,

    pub keywords: Vec<String>,
    pub text: Option<String>,
}

fn non_negative(field: &'static str, value: Option<i32>) -> Result<Option<u32>, CardConversionError> {
    match value {
        None => Ok(None),
        Some(v) if v >= 0 => Ok(Some(v as u32)),
        Some(v) => Err(CardConversionError::NegativeValue { field, value: v }),
    }
}

fn parse_card_type(type_code: &str) -> Result<CardType, CardConversionError> {
    match type_code {
        "identity" => Ok(CardType::Identity),
        "agenda" => Ok(CardType::Agenda),
        "asset" => Ok(CardType::Asset),
        "ice" => Ok(CardType::Ice),
        "operation" => Ok(CardType::Operation),
        "upgrade" => Ok(CardType::Upgrade),
        "event" => Ok(CardType::Event),
        "hardware" => Ok(CardType::Hardware),
        "resource" => Ok(CardType::Resource),
        "program" => Ok(CardType::Program),
        other => Err(CardConversionError::UnknownCardType(other.to_string())),
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

impl TryFrom<NetrunnerDbCardDto> for CardDefinition {
    type Error = CardConversionError;

    fn try_from(dto: NetrunnerDbCardDto) -> Result<Self, Self::Error> {
        let id = dto
            .code
            .parse::<u32>()
            .map(CardId)
            .map_err(|_| CardConversionError::InvalidCardCode(dto.code.clone()))?;

        Ok(CardDefinition {
            id,
            title: dto.title,
            card_type: parse_card_type(&dto.type_code)?,
            faction: parse_faction(&dto.faction_code)?,
            side: parse_side(&dto.side_code)?,
            pack_code: dto.pack_code,

            cost: non_negative("cost", dto.cost)?,
            strength: non_negative("strength", dto.strength)?,
            advancement_requirement: non_negative("advancement_cost", dto.advancement_cost)?,
            agenda_points: non_negative("agenda_points", dto.agenda_points)?,
            trash_cost: non_negative("trash_cost", dto.trash_cost)?,
            influence_cost: non_negative("faction_cost", dto.faction_cost)?,
            memory_cost: non_negative("memory_cost", dto.memory_cost)?,
            min_deck_size: non_negative("minimum_deck_size", dto.minimum_deck_size)?,
            base_link: non_negative("base_link", dto.base_link)?,
            unique: dto.uniqueness.unwrap_or(false),

            keywords: parse_keywords(dto.keywords),
            text: dto.text,
        })
    }
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
        }
    }

    #[test]
    fn converts_a_valid_ice() {
        let def = CardDefinition::try_from(base_dto()).expect("valid conversion");
        assert_eq!(def.id, CardId(30038));
        assert_eq!(def.card_type, CardType::Ice);
        assert_eq!(def.faction, Faction::HaasBioroid);
        assert_eq!(def.side, Side::Corp);
        assert_eq!(def.strength, Some(4));
        assert_eq!(def.influence_cost, Some(3));
        assert_eq!(def.keywords, vec!["Sentry", "Bioroid", "Destroyer"]);
        assert!(!def.unique);
    }

    #[test]
    fn converts_each_card_type() {
        for (type_code, expected) in [
            ("identity", CardType::Identity),
            ("agenda", CardType::Agenda),
            ("asset", CardType::Asset),
            ("ice", CardType::Ice),
            ("operation", CardType::Operation),
            ("upgrade", CardType::Upgrade),
            ("event", CardType::Event),
            ("hardware", CardType::Hardware),
            ("resource", CardType::Resource),
            ("program", CardType::Program),
        ] {
            let mut dto = base_dto();
            dto.type_code = type_code.to_string();
            let def = CardDefinition::try_from(dto).expect("valid conversion");
            assert_eq!(def.card_type, expected);
        }
    }

    #[test]
    fn rejects_unknown_card_type() {
        let mut dto = base_dto();
        dto.type_code = "vehicle".to_string();
        assert_eq!(
            CardDefinition::try_from(dto),
            Err(CardConversionError::UnknownCardType("vehicle".to_string()))
        );
    }

    #[test]
    fn rejects_unknown_faction() {
        let mut dto = base_dto();
        dto.faction_code = "brawlers".to_string();
        assert_eq!(
            CardDefinition::try_from(dto),
            Err(CardConversionError::UnknownFaction("brawlers".to_string()))
        );
    }

    #[test]
    fn rejects_unknown_side() {
        let mut dto = base_dto();
        dto.side_code = "both".to_string();
        assert_eq!(
            CardDefinition::try_from(dto),
            Err(CardConversionError::UnknownSide("both".to_string()))
        );
    }

    #[test]
    fn rejects_invalid_card_code() {
        let mut dto = base_dto();
        dto.code = "not-a-number".to_string();
        assert_eq!(
            CardDefinition::try_from(dto),
            Err(CardConversionError::InvalidCardCode("not-a-number".to_string()))
        );
    }

    #[test]
    fn rejects_negative_values() {
        let mut dto = base_dto();
        dto.cost = Some(-1);
        assert_eq!(
            CardDefinition::try_from(dto),
            Err(CardConversionError::NegativeValue { field: "cost", value: -1 })
        );
    }
}
