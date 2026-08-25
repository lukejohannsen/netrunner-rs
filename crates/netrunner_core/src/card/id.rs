use serde::{Deserialize, Serialize};

/// NetrunnerDB's numeric card `code` (e.g. JSON `"code": "01001"` parsed to
/// `1001`), keying the descriptive catalog layer (`CardDefinition`) —
/// distinct from `dsl::CardId`, which keys the authoritative rules-engine
/// card AST by a slug string (e.g. `"ice_wall"`). These two ID types are
/// intentionally not unified: this one is reference/deckbuilding metadata,
/// the other is what the engine actually plays. Callers importing both
/// should alias one, e.g. `use netrunner_core::card::CardId as
/// CatalogCardId;`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CardId(pub u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let id = CardId(1001);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "1001");
        assert_eq!(serde_json::from_str::<CardId>(&json).unwrap(), id);
    }

    #[test]
    fn orders_numerically() {
        assert!(CardId(1) < CardId(2));
        assert!(CardId(999) < CardId(1000));
    }
}
