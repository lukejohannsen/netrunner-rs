//! The unbuilt cards of every set the NSG card-pool plan builds
//! (docs/roadmap/nsg-card-pool.md), one list per set, for the set gates in
//! `embedded::catalog_join_tests` — the lists `SG_UNIMPLEMENTED` and
//! `ELEV_UNIMPLEMENTED` were for their sets.
//!
//! **Every entry's reason is the same, so it is said once per list:** the
//! card is not built yet, and the list's doc comment names the tranche that
//! builds it. An entry is the card's id and its title — the title so a
//! stale entry, left behind after its card landed, names itself when the
//! gate fails. **The lists only shrink.** A card file cannot land without
//! deleting its entries, in every set that prints it (a reprint is in two
//! lists), because the gate counts a printing as built when a playable card
//! has its card's id.
//!
//! Seeded by `scripts/catalog_sync.py --unimplemented <set>` when the packs
//! were embedded (Stage 0, 26 September 2026); keyed by printing code until
//! the catalog moved to NetrunnerDB v3 (Stage 0d).

/// The formats whose whole card pool is built, held by
/// `every_card_in_a_complete_formats_pool_is_built_and_playable`: every
/// card in the format's pool is a playable card. **The list only grows**, and it grows only there: a format that
/// is not on it must still be short of a card (the same gate says so), so
/// the day Standard's last pack lands, Standard is added here or the gate
/// names it. Startup was verified complete on 29 September 2026 (Phase 5
/// §25 Stage 0): System Gateway, Elevation and Vantage Point, 225 cards.
/// Standard was, with Downfall Stage 8 (6 October 2026): its last unbuilt
/// cards were Downfall's.
pub(crate) const COMPLETE_FORMATS: &[crate::format::NsgFormat] = &[crate::format::NsgFormat::Startup, crate::format::NsgFormat::Standard];

/// *Vantage Point* (`vantage_point`): tranche 1 of the NSG plan.
pub(crate) const VP_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Rebellion Without Rehearsal* (`rebellion_without_rehearsal`): tranche 2 of the NSG plan.
pub(crate) const RWR_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *The Automata Initiative* (`the_automata_initiative`): tranche 3 of the NSG plan.
pub(crate) const TAI_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Parhelion* (`parhelion`): tranche 4 of the NSG plan.
pub(crate) const PH_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Midnight Sun Booster Pack* (`midnight_sun_booster_pack`): tranche 5 of the NSG plan.
pub(crate) const MSBP_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Midnight Sun* (`midnight_sun`): tranche 5 of the NSG plan.
pub(crate) const MS_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Uprising Booster Pack* (`uprising_booster_pack`): tranche 6 of the NSG plan.
pub(crate) const URBP_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Uprising* (`uprising`): tranche 6 of the NSG plan.
pub(crate) const UR_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Downfall* (`downfall`): tranche 7 of the NSG plan.
pub(crate) const DF_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *System Update 2021* (`system_update_2021`): tranche 8 of the NSG plan.
pub(crate) const SU21_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Salvaged Memories* (`salvaged_memories`): tranche 8 of the NSG plan.
pub(crate) const SM_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Magnum Opus Reprint* (`magnum_opus_reprint`): tranche 8 of the NSG plan.
pub(crate) const MOR_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// The *Core Set* (`core_set`): its reprinted cards were built with their
/// reprints and the rest by tranche 8 Stage 11, so it was gated (Stage 11x)
/// only once every card was built; a catalog sync that changes its list
/// now fails here.
pub(crate) const CORE_UNIMPLEMENTED: &[(&str, &str)] = &[];
