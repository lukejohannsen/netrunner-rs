//! Null Signal Games (NSG) competitive format definitions.
//!
//! Distinct from `card::pack::PackInfo` (raw pack metadata) and `catalog::
//! CardCatalog` (the full card pool) — this module answers "given a format,
//! which packs and cards are legal," a question neither of those types
//! answers on its own. Pure, static, in-memory data: `netrunner_core` has no
//! I/O, so there is no live-synced feed of NSG's actual current rotation or
//! banlist here. The tables below are a deliberately small, illustrative
//! seed (scoped to the packs this crate's own embedded catalog actually
//! ships, `catalog::CardCatalog::load_default_core_sets`'s "sg"/"elev") for
//! a maintainer to extend as new packs/banlist updates are published — not
//! a claim of being NSG's current authoritative rotation or banlist.

use std::collections::HashSet;

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
/// pool, which cards are outright banned, and which are capped to a single
/// copy (Null Signal Games' "Restricted List"). `restricted` is a
/// deliberate simplification of the real Restricted List rule, which caps
/// the *whole deck* to one restricted card total rather than one copy of
/// *each* restricted card — no `Decklist`/catalog field exists to source
/// the cross-card interaction from, and a single-copy-per-card cap is the
/// closer approximation of the two easy alternatives (the other being no
/// enforcement at all).
#[derive(Debug, Clone, Default)]
pub struct FormatRules {
    /// `None` means every pack is legal (Eternal). `Some(set)` restricts
    /// legality to exactly these `CardDefinition::pack_code` values.
    pub allowed_packs: Option<HashSet<&'static str>>,
    pub banned: HashSet<CardId>,
    pub restricted: HashSet<CardId>,
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
