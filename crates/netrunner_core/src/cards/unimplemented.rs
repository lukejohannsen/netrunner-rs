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
pub(crate) const SU21_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("quetzal_free_spirit", "Quetzal: Free Spirit"),
    ("reina_roja_freedom_fighter", "Reina Roja: Freedom Fighter"),
    ("en_passant", "En Passant"),
    ("clot", "Clot"),
    ("imp", "Imp"),
    ("ice_carver", "Ice Carver"),
    ("ken_express_tenma_disappeared_clone", "Ken “Express” Tenma: Disappeared Clone"),
    ("steve_cambridge_master_grifter", "Steve Cambridge: Master Grifter"),
    ("forged_activation_orders", "Forged Activation Orders"),
    ("inside_job", "Inside Job"),
    ("networking", "Networking"),
    ("femme_fatale", "Femme Fatale"),
    ("sneakdoor_beta", "Sneakdoor Beta"),
    ("security_testing", "Security Testing"),
    ("ayla_bios_rahim_simulant_specialist", "Ayla “Bios” Rahim: Simulant Specialist"),
    ("rielle_kit_peddler_transhuman", "Rielle “Kit” Peddler: Transhuman"),
    ("test_run", "Test Run"),
    ("atman", "Atman"),
    ("chameleon", "Chameleon"),
    ("egret", "Egret"),
    ("paricia", "Paricia"),
    ("haas_bioroid_architects_of_tomorrow", "Haas-Bioroid: Architects of Tomorrow"),
    ("project_vitruvius", "Project Vitruvius"),
    ("marilyn_campaign", "Marilyn Campaign"),
    ("magnet", "Magnet"),
    ("ravana_1_0", "Ravana 1.0"),
    ("archived_memories", "Archived Memories"),
    ("biotic_labor", "Biotic Labor"),
    ("corporate_troubleshooter", "Corporate Troubleshooter"),
    ("nisei_mk_ii", "Nisei MK II"),
    ("lotus_field", "Lotus Field"),
    ("swordsman", "Swordsman"),
    ("trick_of_light", "Trick of Light"),
    ("near_earth_hub_broadcast_center", "Near-Earth Hub: Broadcast Center"),
    ("project_beale", "Project Beale"),
    ("daily_business_show", "Daily Business Show"),
    ("tollbooth", "Tollbooth"),
    ("psychographics", "Psychographics"),
    ("sansan_city_grid", "SanSan City Grid"),
    ("oaktown_renovation", "Oaktown Renovation"),
    ("project_atlas", "Project Atlas"),
    ("hortum", "Hortum"),
    ("punitive_counterstrike", "Punitive Counterstrike"),
    ("crisium_grid", "Crisium Grid"),
    ("subliminal_messaging", "Subliminal Messaging"),
];

/// *Salvaged Memories* (`salvaged_memories`): tranche 8 of the NSG plan.
pub(crate) const SM_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("medium", "Medium"),
    ("parasite", "Parasite"),
    ("e3_feedback_implants", "e3 Feedback Implants"),
    ("cerberus_lady_h1", "Cerberus \"Lady\" H1"),
    ("next_bronze", "NEXT Bronze"),
    ("next_silver", "NEXT Silver"),
    ("hostile_infrastructure", "Hostile Infrastructure"),
    ("turtlebacks", "Turtlebacks"),
    ("sansan_city_grid", "SanSan City Grid"),
    ("executive_boot_camp", "Executive Boot Camp"),
    ("excalibur", "Excalibur"),
    ("subliminal_messaging", "Subliminal Messaging"),
];

/// *Magnum Opus Reprint* (`magnum_opus_reprint`): tranche 8 of the NSG plan.
pub(crate) const MOR_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("crowdfunding", "Crowdfunding"),
    ("slot_machine", "Slot Machine"),
    ("timely_public_release", "Timely Public Release"),
];
