use std::fmt;

use serde::{Deserialize, Serialize};

/// A NetrunnerDB printing: one card as one set prints it, by the code
/// NetrunnerDB's v3 API uses as the printing's id (`"30075"` is System
/// Gateway's Hedge Fund, parsed to `30075`). Distinct from `dsl::CardId`,
/// the card itself by v3 slug (`"hedge_fund"`), which is what the engine
/// plays and what legality is judged by: a reprint is another printing of
/// the same card, never another card.
///
/// A printing is what has a picture, a set and a place in it, and an
/// illustrator — so the image cache, the set a card is shown under and the
/// art a person chooses all hang off this number (`cards::catalog`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PrintingId(pub u32);

impl fmt::Display for PrintingId {
    /// Five digits, as NetrunnerDB writes a code: the Core Set's `01001`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:05}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let id = PrintingId(1001);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "1001");
        assert_eq!(serde_json::from_str::<PrintingId>(&json).unwrap(), id);
    }

    #[test]
    fn orders_numerically_and_prints_as_netrunnerdb_writes_it() {
        assert!(PrintingId(999) < PrintingId(1000));
        assert_eq!(PrintingId(1001).to_string(), "01001");
        assert_eq!(PrintingId(30075).to_string(), "30075");
    }
}
