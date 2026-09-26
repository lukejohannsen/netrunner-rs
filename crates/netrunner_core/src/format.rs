//! Null Signal Games (NSG) competitive format definitions, and Casual.
//!
//! Distinct from `card::pack::PackInfo` (raw pack metadata) and
//! `cards::CardRegistry` (the full card pool) — this module answers "given a
//! format, which cards are legal," a question neither of those types
//! answers on its own.
//!
//! **The tables are NetrunnerDB's, embedded** (Phase 1 §9 Stage 0b, 26
//! September 2026). `data/formats.json` is written by
//! `scripts/catalog_sync.py` from NetrunnerDB's v3 API: each format's active
//! card pool and its active restriction list. `netrunner_core` does no I/O,
//! so the file is embedded the way the card catalog is, and a new ban list
//! is a re-sync and a diff. They used to be a hand-written seed that banned
//! nothing and scoped Startup to the two packs this crate then embedded,
//! pinned by a test that failed the day a real list arrived, so that adding
//! one was deliberate. This is that day.
//!
//! **A pool is a set of printing codes, not packs.** A card file names one
//! printing (`numeric_id`), and a format admits a card by any of its
//! printings: Corroder's file names its Core Set code, which no current
//! pool prints, while the card is reprinted elsewhere. So the file lists
//! every printing of every card in the pool, and a ban every printing of
//! the banned card.
//!
//! **A ban is the banning format's alone** (the person's decision, 26
//! September 2026). A card banned in Standard and not in Eternal makes a
//! deck that holds it illegal in Standard — refused at a Standard table and
//! in a Standard lobby — and legal in Eternal, where it plays. A ban never
//! reaches past its own list. A deck is legal in exactly the formats whose
//! lists it satisfies (`DeckFile::legal_formats`), and `Casual`, every card
//! with no list, is open to every card.
//!
//! **Casual lifts card legality and nothing else** (the person, the same
//! day): the pool and the lists. The deckbuilding rules still hold there as
//! everywhere — three copies of a card by name (its own `deck_limit` where
//! it prints one), the identity's influence and minimum deck size, the
//! agenda-point range — and the validator's construction tests run under
//! Casual for that reason (`too_many_copies_is_rejected`,
//! `influence_exceeded_is_rejected`).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::card::CardId;

/// A format a deck is judged in: Null Signal Games' four, and Casual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NsgFormat {
    /// The rotating, small-card-pool entry format.
    Startup,
    /// The larger, periodically-rotating competitive format.
    Standard,
    /// Every card ever released, with a points list.
    Eternal,
    /// A fixed historical card pool, frozen at a point in time, with a ban
    /// list and a restricted list.
    Snapshot,
    /// Not Null Signal Games': every card the catalog knows and no list.
    /// What a deck is legal in when every list refuses it, which is why a
    /// ban never refuses a deck outright. Card legality only: the
    /// deckbuilding rules (copies, influence, deck size, agenda points)
    /// hold here as in every format.
    Casual,
}

impl NsgFormat {
    /// Every format, in the order a list of them is shown.
    pub const ALL: [NsgFormat; 5] =
        [NsgFormat::Startup, NsgFormat::Standard, NsgFormat::Eternal, NsgFormat::Snapshot, NsgFormat::Casual];
}

/// The legality rules for one `NsgFormat`: which cards are in the pool,
/// which are banned, and what the deck may spend on restricted cards.
///
/// **The restriction model is a points budget, which is the real rule's
/// shape.** Give every listed card a cost and the deck an allowance:
/// Eternal's points list is exactly that (a budget of 7), and Snapshot's
/// restricted list — at most one restricted card — is the special case where
/// each costs 1 and the allowance is 1.
///
/// Cost is counted **once per distinct card**, not per copy, because the
/// list restricts which cards a deck may build around rather than how many
/// copies it runs; the copy count is already `CardDefinition::deck_limit`'s
/// job.
#[derive(Debug, Clone)]
pub struct FormatRules {
    /// The printing codes of every card in the pool. `None` is every card.
    pub pool: Option<HashSet<CardId>>,
    /// The pool's packs, NetrunnerDB's pack codes — for a builder's "which
    /// sets" and nothing legal is decided by it. Empty for every card.
    pub packs: Vec<String>,
    /// The ban list's name, when the format has one ("Startup Balance
    /// Update 26.03").
    pub list: Option<String>,
    pub banned: HashSet<CardId>,
    /// What each listed card costs against `restriction_budget`. A card
    /// that is not listed costs nothing.
    pub restriction_points: HashMap<CardId, u32>,
    /// What a deck may spend in total. `u32::MAX` is "unrestricted", which
    /// is what `Default` gives, so a format that lists nothing is not
    /// accidentally capped at zero.
    pub restriction_budget: u32,
}

impl Default for FormatRules {
    /// Every card legal and nothing restricted — Casual's rules.
    /// Hand-written rather than derived because `restriction_budget` must
    /// default to "unlimited" and `u32`'s own default is 0, which would make
    /// every deck illegal the moment a card was listed.
    fn default() -> Self {
        FormatRules {
            pool: None,
            packs: Vec::new(),
            list: None,
            banned: HashSet::new(),
            restriction_points: HashMap::new(),
            restriction_budget: u32::MAX,
        }
    }
}

impl FormatRules {
    /// Whether the pool holds a printing: any printing of a card in it.
    pub fn in_pool(&self, code: CardId) -> bool {
        self.pool.as_ref().is_none_or(|pool| pool.contains(&code))
    }
}

/// The influence budget every identity grants by default. Real Netrunner
/// varies this per identity only in rare, special-cased cases NetrunnerDB's
/// API doesn't expose as structured data (`CardDefinition` has no field to
/// source an override from — the same reasoning `rules::deck::
/// MAX_COPIES_PER_CARD`'s doc comment already gives for that constant being
/// flat rather than per-card).
pub const DEFAULT_INFLUENCE_LIMIT: u32 = 15;

/// One format as `data/formats.json` holds it.
#[derive(Deserialize)]
struct FormatData {
    packs: Vec<String>,
    pool: Vec<String>,
    restriction: Option<String>,
    banned: Vec<String>,
    restricted: Vec<String>,
    points: HashMap<String, u32>,
    point_limit: Option<u32>,
}

const FORMATS_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/formats.json"));

fn code(text: &str) -> CardId {
    CardId(text.parse().unwrap_or_else(|_| panic!("formats.json holds printing codes, not {text:?}")))
}

fn from_data(data: &FormatData) -> FormatRules {
    // A restricted list is a budget of one at a point each; a points list
    // is its own budget. NetrunnerDB's lists carry one or the other.
    let (restriction_points, restriction_budget) = if !data.restricted.is_empty() {
        (data.restricted.iter().map(|c| (code(c), 1)).collect(), 1)
    } else {
        (data.points.iter().map(|(c, points)| (code(c), *points)).collect(), data.point_limit.unwrap_or(u32::MAX))
    };
    FormatRules {
        pool: Some(data.pool.iter().map(|c| code(c)).collect()),
        packs: data.packs.clone(),
        list: data.restriction.clone(),
        banned: data.banned.iter().map(|c| code(c)).collect(),
        restriction_points,
        restriction_budget,
    }
}

impl NsgFormat {
    /// NetrunnerDB's id for the format, the key in `data/formats.json`.
    fn netrunnerdb_id(self) -> Option<&'static str> {
        match self {
            NsgFormat::Startup => Some("startup"),
            NsgFormat::Standard => Some("standard"),
            NsgFormat::Eternal => Some("eternal"),
            NsgFormat::Snapshot => Some("snapshot"),
            NsgFormat::Casual => None,
        }
    }

    /// The legality rules for this format, parsed once from the embedded
    /// tables and shared: a builder asks for them per card, and a pool is
    /// thousands of codes.
    pub fn rules(self) -> &'static FormatRules {
        static RULES: OnceLock<HashMap<NsgFormat, FormatRules>> = OnceLock::new();
        let rules = RULES.get_or_init(|| {
            let data: HashMap<String, FormatData> =
                serde_json::from_str(FORMATS_JSON).expect("the embedded formats.json parses");
            NsgFormat::ALL
                .into_iter()
                .map(|format| {
                    let rules = format.netrunnerdb_id().map_or_else(FormatRules::default, |id| {
                        from_data(data.get(id).unwrap_or_else(|| panic!("formats.json has no {id}")))
                    });
                    (format, rules)
                })
                .collect()
        });
        &rules[&self]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pools, read off NetrunnerDB on 26 September 2026: Startup is
    /// System Gateway, Elevation and Vantage Point; Standard adds Ashes,
    /// Borealis and Liberation; Casual is everything. A re-sync that moves
    /// a pool fails here, so a rotation is read before it is shipped.
    #[test]
    fn each_format_holds_the_pool_netrunnerdb_publishes() {
        assert_eq!(NsgFormat::Startup.rules().packs, ["elev", "sg", "vp"]);
        assert_eq!(
            NsgFormat::Standard.rules().packs,
            ["df", "elev", "ms", "msbp", "ph", "rwr", "sg", "tai", "ur", "urbp", "vp"]
        );
        assert!(NsgFormat::Eternal.rules().packs.iter().any(|pack| pack == "core"));
        assert!(NsgFormat::Casual.rules().pool.is_none());
        // Hedge Fund's System Gateway printing is in Startup; the Core
        // Set's Ice Wall is not, and is Eternal.
        assert!(NsgFormat::Startup.rules().in_pool(CardId(30075)));
        assert!(!NsgFormat::Startup.rules().in_pool(CardId(1103)));
        assert!(NsgFormat::Eternal.rules().in_pool(CardId(1103)));
    }

    /// A card is in a pool by any of its printings: Hedge Fund's Core Set
    /// printing is in Standard because its System Gateway one is.
    #[test]
    fn a_pool_admits_a_card_by_any_printing() {
        assert!(NsgFormat::Standard.rules().in_pool(CardId(30075)));
        assert!(NsgFormat::Standard.rules().in_pool(CardId(1110)), "the Core Set's Hedge Fund");
    }

    /// The lists: Startup's balance update bans Cleaver (System Gateway's
    /// 30006) and Standard's ban list does too; Eternal is a points budget
    /// of 7; Snapshot's restricted list is a budget of one; Casual lists
    /// nothing.
    #[test]
    fn each_format_holds_the_list_netrunnerdb_publishes() {
        let startup = NsgFormat::Startup.rules();
        assert_eq!(startup.list.as_deref(), Some("Startup Balance Update 26.03"));
        assert!(startup.banned.contains(&CardId(30006)));
        assert!(NsgFormat::Standard.rules().banned.contains(&CardId(30006)));
        assert_eq!(NsgFormat::Eternal.rules().restriction_budget, 7);
        assert!(!NsgFormat::Eternal.rules().restriction_points.is_empty());
        assert_eq!(NsgFormat::Snapshot.rules().restriction_budget, 1);
        assert!(NsgFormat::Snapshot.rules().restriction_points.values().all(|points| *points == 1));
        let casual = NsgFormat::Casual.rules();
        assert!(casual.banned.is_empty() && casual.restriction_points.is_empty());
        assert_eq!(casual.restriction_budget, u32::MAX);
    }
}
