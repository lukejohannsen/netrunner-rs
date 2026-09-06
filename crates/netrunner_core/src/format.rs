//! Null Signal Games (NSG) competitive format definitions.
//!
//! Distinct from `card::pack::PackInfo` (raw pack metadata) and
//! `cards::CardRegistry` (the full card pool) — this module answers "given a
//! format, which packs and cards are legal," a question neither of those
//! types answers on its own. Pure, static, in-memory data: `netrunner_core`
//! has no I/O, so there is no live-synced feed of NSG's actual current
//! rotation or banlist here. The tables below are a deliberately small,
//! illustrative seed (scoped to the packs this crate's own embedded sets
//! actually ship, `cards::netrunnerdb::load_embedded_netrunnerdb_sets`'s
//! "sg"/"elev") for
//! a maintainer to extend as new packs/banlist updates are published — not
//! a claim of being NSG's current authoritative rotation or banlist.
//!
//! **The mechanisms are complete; the data is not, and the distinction is
//! deliberate.** A ban, a pack pool and a restriction budget are all
//! enforced by `deck::validator`, and each is exercised by a test that
//! supplies its own rules through `validate_deck_with_rules` — because
//! every shipped format currently bans nothing, restricts nothing, and
//! (for Startup and Snapshot) allows exactly the two packs this crate
//! embeds, so those paths are unreachable from the shipped tables alone.
//! Filling in a real banlist or rotation is a data change against working
//! machinery, and `no_shipped_format_restricts_anything_yet` fails when
//! someone makes it, so it is a deliberate act rather than a silent one.
//! Nothing here should be invented: an entry that is not sourced from a
//! published NSG list is worse than an empty table, because an empty table
//! is visibly a seed and a wrong one is not.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::card::CardId;

/// A Null Signal Games-supported competitive format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NsgFormat {
    /// The rotating, small-card-pool entry format.
    Startup,
    /// The larger, periodically-rotating competitive format.
    Standard,
    /// Every pack ever released is legal — only the banlist restricts it.
    Eternal,
    /// A fixed historical card-pool snapshot, frozen at a point in time.
    Snapshot,
}

/// The legality rules for one `NsgFormat`: which packs are in the legal
/// pool, which cards are outright banned, and what the deck may spend on
/// restricted cards.
///
/// **The restriction model is a points budget, which is the real rule's
/// shape.** It used to be a `HashSet` of cards each capped to one copy,
/// and that was documented as a deliberate approximation: the actual rule
/// constrains the *deck as a whole* rather than each card, so a per-card
/// copy limit could not express "one restricted card, whichever you pick"
/// at all. A budget does: give every listed card a cost and the deck an
/// allowance, and the classic "at most one restricted card" list is the
/// special case where each costs 1 and the allowance is 1.
///
/// Cost is counted **once per distinct card**, not per copy, because the
/// list restricts which cards a deck may build around rather than how many
/// copies it runs; the copy count is already `CardDefinition::deck_limit`'s
/// job.
#[derive(Debug, Clone)]
pub struct FormatRules {
    /// `None` means every pack is legal (Eternal). `Some(set)` restricts
    /// legality to exactly these `CardDefinition::set_code` values.
    pub allowed_packs: Option<HashSet<&'static str>>,
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
    /// Everything legal and nothing restricted — the shape `Standard` and
    /// `Eternal` take. Hand-written rather than derived because
    /// `restriction_budget` must default to "unlimited" and `u32`'s own
    /// default is 0, which would make every deck illegal the moment a card
    /// was listed.
    fn default() -> Self {
        FormatRules {
            allowed_packs: None,
            banned: HashSet::new(),
            restriction_points: HashMap::new(),
            restriction_budget: u32::MAX,
        }
    }
}

/// The influence budget every identity grants by default. Real Netrunner
/// varies this per identity only in rare, special-cased cases NetrunnerDB's
/// API doesn't expose as structured data (`CardDefinition` has no field to
/// source an override from — the same reasoning `rules::deck::
/// MAX_COPIES_PER_CARD`'s doc comment already gives for that constant being
/// flat rather than per-card).
pub const DEFAULT_INFLUENCE_LIMIT: u32 = 15;

impl NsgFormat {
    /// The legality rules for this format. Startup and Snapshot are scoped
    /// to this crate's embedded catalog packs (`"sg"`, `"elev"`); Standard
    /// is left unrestricted by pack pending a real rotation feed (see the
    /// module doc comment) rather than falsely narrowing it to match
    /// Startup; Eternal is unrestricted by design.
    pub fn rules(self) -> FormatRules {
        match self {
            NsgFormat::Startup => {
                FormatRules { allowed_packs: Some(HashSet::from(["sg", "elev"])), ..Default::default() }
            }
            NsgFormat::Standard => FormatRules::default(),
            NsgFormat::Eternal => FormatRules::default(),
            NsgFormat::Snapshot => {
                FormatRules { allowed_packs: Some(HashSet::from(["sg", "elev"])), ..Default::default() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eternal_and_standard_allow_every_pack() {
        assert_eq!(NsgFormat::Eternal.rules().allowed_packs, None);
        assert_eq!(NsgFormat::Standard.rules().allowed_packs, None);
    }

    #[test]
    fn startup_and_snapshot_restrict_to_the_embedded_packs() {
        for format in [NsgFormat::Startup, NsgFormat::Snapshot] {
            let allowed = format.rules().allowed_packs.expect("restricted pack pool");
            assert!(allowed.contains("sg"));
            assert!(allowed.contains("elev"));
            assert!(!allowed.contains("some-future-pack"));
        }
    }
}
