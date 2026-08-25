use serde::{Deserialize, Serialize};

/// Raw wire shape of a card object from NetrunnerDB's
/// `/api/2.0/public/cards` endpoint. Deliberately permissive: fields this
/// catalog doesn't need (illustrator, flavor text, images, rotation status,
/// etc.) are simply not declared here and are ignored by `serde_json`'s
/// default (non-`deny_unknown_fields`) struct deserialization. Numeric
/// fields are typed `i32` (not `u32`) even though none are ever negative in
/// practice — non-negativity is validated during `TryFrom<NetrunnerDbCardDto>
/// for CardDefinition`, not by a serde-level parse failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetrunnerDbCardDto {
    pub code: String,
    pub title: String,
    pub type_code: String,
    pub side_code: String,
    pub faction_code: String,
    pub pack_code: String,

    #[serde(default)]
    pub text: Option<String>,
    /// A `" - "`-delimited string of subtypes/traits, e.g.
    /// `"Sentry - Bioroid - Destroyer"` or `"Virus - Trojan"`. Split into
    /// `CardDefinition::keywords` during conversion.
    #[serde(default)]
    pub keywords: Option<String>,
    #[serde(default)]
    pub cost: Option<i32>,
    #[serde(default)]
    pub strength: Option<i32>,
    #[serde(default)]
    pub advancement_cost: Option<i32>,
    #[serde(default)]
    pub agenda_points: Option<i32>,
    #[serde(default)]
    pub trash_cost: Option<i32>,
    /// NetrunnerDB's name for a card's influence cost.
    #[serde(default)]
    pub faction_cost: Option<i32>,
    #[serde(default)]
    pub memory_cost: Option<i32>,
    #[serde(default)]
    pub minimum_deck_size: Option<i32>,
    #[serde(default)]
    pub base_link: Option<i32>,
    #[serde(default)]
    pub uniqueness: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_ice_with_strength_and_keywords() {
        let json = r#"{
            "code": "30038",
            "title": "Ansel 1.0",
            "type_code": "ice",
            "side_code": "corp",
            "faction_code": "haas-bioroid",
            "pack_code": "sg",
            "cost": 6,
            "strength": 4,
            "keywords": "Sentry - Bioroid - Destroyer",
            "text": "Lose click: Break 1 subroutine on this ice.",
            "uniqueness": false
        }"#;

        let dto: NetrunnerDbCardDto = serde_json::from_str(json).expect("valid card JSON");
        assert_eq!(dto.code, "30038");
        assert_eq!(dto.title, "Ansel 1.0");
        assert_eq!(dto.type_code, "ice");
        assert_eq!(dto.strength, Some(4));
        assert_eq!(dto.keywords.as_deref(), Some("Sentry - Bioroid - Destroyer"));
        assert_eq!(dto.faction_cost, None);
    }

    #[test]
    fn parses_an_agenda_with_advancement_and_points() {
        let json = r#"{
            "code": "30036",
            "title": "Luminal Transubstantiation",
            "type_code": "agenda",
            "side_code": "corp",
            "faction_code": "haas-bioroid",
            "pack_code": "sg",
            "advancement_cost": 3,
            "agenda_points": 2
        }"#;

        let dto: NetrunnerDbCardDto = serde_json::from_str(json).expect("valid card JSON");
        assert_eq!(dto.advancement_cost, Some(3));
        assert_eq!(dto.agenda_points, Some(2));
        assert_eq!(dto.cost, None);
    }

    #[test]
    fn parses_an_identity_with_min_deck_size_and_link() {
        let json = r#"{
            "code": "30001",
            "title": "René “Loup” Arcemont: Party Animal",
            "type_code": "identity",
            "side_code": "runner",
            "faction_code": "anarch",
            "pack_code": "sg",
            "minimum_deck_size": 40,
            "base_link": 0,
            "keywords": "G-mod"
        }"#;

        let dto: NetrunnerDbCardDto = serde_json::from_str(json).expect("valid card JSON");
        assert_eq!(dto.minimum_deck_size, Some(40));
        assert_eq!(dto.base_link, Some(0));
    }

    #[test]
    fn ignores_unrecognized_fields() {
        let json = r#"{
            "code": "30002",
            "title": "Wildcat Strike",
            "type_code": "event",
            "side_code": "runner",
            "faction_code": "anarch",
            "pack_code": "sg",
            "cost": 2,
            "illustrator": "David Lei",
            "flavor": "flavor text",
            "images": { "large": "https://example.com/x.png" }
        }"#;

        let dto: NetrunnerDbCardDto = serde_json::from_str(json).expect("valid card JSON");
        assert_eq!(dto.cost, Some(2));
    }
}
