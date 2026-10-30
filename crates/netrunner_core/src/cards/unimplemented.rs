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

// The Fantasy Flight Games sets (docs/roadmap/ffg-card-pool.md), embedded by
// its Stage 0a with every card they add listed here, in printing order, so
// each list only shrinks as the tranches build them. A card two of these
// sets print is listed in both. Written from the committed catalog the way
// `scripts/catalog_sync.py --unimplemented <set id>` writes one set.

/// *Revised Core Set* (`revised_core_set`): tranche F1 of the FFG plan.
pub(crate) const CORE2_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Reign and Reverie* (`reign_and_reverie`): tranche F2 of the FFG plan.
pub(crate) const RAR_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Sovereign Sight* (`sovereign_sight`): tranche F3 of the FFG plan.
pub(crate) const SS_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Down the White Nile* (`down_the_white_nile`): tranche F3 of the FFG plan.
pub(crate) const DTWN_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Council of the Crest* (`council_of_the_crest`): tranche F3 of the FFG plan.
pub(crate) const COTC_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *The Devil and the Dragon* (`the_devil_and_the_dragon`): tranche F3 of the FFG plan.
pub(crate) const TDATD_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Whispers in Nalubaale* (`whispers_in_nalubaale`): tranche F3 of the FFG plan.
pub(crate) const WIN_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Kampala Ascendent* (`kampala_ascendent`): tranche F3 of the FFG plan.
pub(crate) const KA_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Daedalus Complex* (`daedalus_complex`): tranche F4 of the FFG plan.
pub(crate) const DC_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Station One* (`station_one`): tranche F4 of the FFG plan.
pub(crate) const SO_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Earth's Scion* (`earths_scion`): tranche F4 of the FFG plan.
pub(crate) const EAS_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Blood and Water* (`blood_and_water`): tranche F4 of the FFG plan.
pub(crate) const BAW_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Free Mars* (`free_mars`): tranche F4 of the FFG plan.
pub(crate) const FM_UNIMPLEMENTED: &[(&str, &str)] = &[];

/// *Crimson Dust* (`crimson_dust`): tranche F4 of the FFG plan.
pub(crate) const CD_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *23 Seconds* (`23_seconds`): tranche F5 of the FFG plan.
pub(crate) const S23S_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("chrysalis", "Chrysalis"),
];

/// *Blood Money* (`blood_money`): tranche F5 of the FFG plan.
pub(crate) const BM_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Escalation* (`escalation`): tranche F5 of the FFG plan.
pub(crate) const ES_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("alexa_belsky", "Alexa Belsky"),
];

/// *Intervention* (`intervention`): tranche F5 of the FFG plan.
pub(crate) const IN_UNIMPLEMENTED: &[(&str, &str)] = &[
];

/// *Martial Law* (`martial_law`): tranche F5 of the FFG plan.
pub(crate) const ML_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("on_the_lam", "On the Lam"),
    ("baba_yaga", "Baba Yaga"),
    ("mind_game", "Mind Game"),
    ("sapper", "Sapper"),
];

/// *Quorum* (`quorum`): tranche F5 of the FFG plan.
pub(crate) const QU_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("encore", "Encore"),
    ("net_quarantine", "Net Quarantine"),
    ("herald", "Herald"),
];

/// *Terminal Directive Cards* (`terminal_directive_cards`): tranche F5 of the FFG
/// plan. Not in Eternal's or Snapshot's set lists, but most of its cards are
/// in both pools.
pub(crate) const TD_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("seidr_laboratories_destiny_defined", "Seidr Laboratories: Destiny Defined"),
    ("skorpios_defense_systems_persuasive_power", "Skorpios Defense Systems: Persuasive Power"),
];

/// *Kala Ghoda* (`kala_ghoda`): tranche F6 of the FFG plan.
pub(crate) const KG_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("ramujan_reliant_550_bmi", "Ramujan-reliant 550 BMI"),
    ("street_magic", "Street Magic"),
    ("mongoose", "Mongoose"),
    ("maya", "Maya"),
    ("panchatantra", "Panchatantra"),
];

/// *Business First* (`business_first`): tranche F6 of the FFG plan.
pub(crate) const BF_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("emp_device", "EMP Device"),
    ("diwan", "Diwan"),
    ("corporate_scandal", "Corporate Scandal"),
    ("lakshmi_smartfabrics", "Lakshmi Smartfabrics"),
];

/// *Democracy and Dogma* (`democracy_and_dogma`): tranche F6 of the FFG plan.
pub(crate) const DAG_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("political_graffiti", "Political Graffiti"),
    ("councilman", "Councilman"),
];

/// *Salsette Island* (`salsette_island`): tranche F6 of the FFG plan.
pub(crate) const SI_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("bazaar", "Bazaar"),
    ("personality_profiles", "Personality Profiles"),
    ("jeeves_model_bioroids", "Jeeves Model Bioroids"),
    ("raman_rai", "Raman Rai"),
];

/// *The Liberated Mind* (`the_liberated_mind`): tranche F6 of the FFG plan.
pub(crate) const TLM_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("information_sifting", "Information Sifting"),
    ("rebirth", "Rebirth"),
    ("dedicated_neural_net", "Dedicated Neural Net"),
];

/// *Fear the Masses* (`fear_the_masses`): tranche F6 of the FFG plan.
pub(crate) const FTM_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("rigged_results", "Rigged Results"),
    ("harishchandra_ent_where_youre_the_star", "Harishchandra Ent.: Where You're the Star"),
    ("full_immersion_recstudio", "Full Immersion RecStudio"),
];

/// *Data and Destiny* (`data_and_destiny`): tranche F6 of the FFG plan.
pub(crate) const DAD_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("sync_everything_everywhere", "SYNC: Everything, Everywhere"),
    ("spark_agency_worldswide_reach", "Spark Agency: Worldswide Reach"),
    ("rebranding_team", "Rebranding Team"),
    ("archangel", "Archangel"),
    ("tl_dr", "TL;DR"),
    ("24_7_news_cycle", "24/7 News Cycle"),
    ("ad_blitz", "Ad Blitz"),
    ("media_blitz", "Media Blitz"),
    ("surveillance_sweep", "Surveillance Sweep"),
    ("apex_invasive_predator", "Apex: Invasive Predator"),
    ("apocalypse", "Apocalypse"),
    ("prey", "Prey"),
    ("endless_hunger", "Endless Hunger"),
    ("harbinger", "Harbinger"),
    ("hunting_grounds", "Hunting Grounds"),
    ("adam_compulsive_hacker", "Adam: Compulsive Hacker"),
    ("independent_thinking", "Independent Thinking"),
    ("always_be_running", "Always Be Running"),
    ("neutralize_all_threats", "Neutralize All Threats"),
    ("security_chip", "Security Chip"),
];

/// *The Valley* (`the_valley`): tranche F7 of the FFG plan.
pub(crate) const VAL_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("adjusted_chronotype", "Adjusted Chronotype"),
    ("gene_conditioning_shoppe", "Gene Conditioning Shoppe"),
    ("jinteki_biotech_life_imagined", "Jinteki Biotech: Life Imagined"),
];

/// *Breaker Bay* (`breaker_bay`): tranche F7 of the FFG plan.
pub(crate) const BB_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("hacktivist_meeting", "Hacktivist Meeting"),
    ("off_campus_apartment", "Off-Campus Apartment"),
    ("dorm_computer", "Dorm Computer"),
    ("comet", "Comet"),
    ("london_library", "London Library"),
    ("research_grant", "Research Grant"),
    ("recruiting_trip", "Recruiting Trip"),
    ("blacklist", "Blacklist"),
];

/// *Chrome City* (`chrome_city`): tranche F7 of the FFG plan.
pub(crate) const CC_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("immolation_script", "Immolation Script"),
    ("chrome_parlor", "Chrome Parlor"),
    ("titanium_ribs", "Titanium Ribs"),
];

/// *The Underway* (`the_underway`): tranche F7 of the FFG plan.
pub(crate) const UW_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("street_peddler", "Street Peddler"),
    ("muertos_gang_member", "Muertos Gang Member"),
    ("allele_repression", "Allele Repression"),
    ("marcus_batty", "Marcus Batty"),
];

/// *Old Hollywood* (`old_hollywood`): tranche F7 of the FFG plan.
pub(crate) const OH_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("film_critic", "Film Critic"),
    ("an_offer_you_cant_refuse", "An Offer You Can’t Refuse"),
    ("casting_call", "Casting Call"),
];

/// *The Universe of Tomorrow* (`the_universe_of_tomorrow`): tranche F7 of the FFG plan.
pub(crate) const UOT_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("surfer", "Surfer"),
    ("bookmark", "Bookmark"),
    ("chronos_protocol_selective_mind_mapping", "Chronos Protocol: Selective Mind-mapping"),
    ("worlds_plaza", "Worlds Plaza"),
];

/// *Order and Chaos* (`order_and_chaos`): tranche F7 of the FFG plan.
pub(crate) const OAC_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("gagarin_deep_space_expanding_the_horizon", "Gagarin Deep Space: Expanding the Horizon"),
    ("glenn_station", "Glenn Station"),
    ("constellation_protocol", "Constellation Protocol"),
    ("mark_yale", "Mark Yale"),
    ("housekeeping", "Housekeeping"),
    ("satellite_grid", "Satellite Grid"),
    ("the_twins", "The Twins"),
    ("dedicated_technician_team", "Dedicated Technician Team"),
    ("ive_had_worse", "I've Had Worse"),
    ("itinerant_protesters", "Itinerant Protesters"),
    ("showing_off", "Showing Off"),
    ("wanton_destruction", "Wanton Destruction"),
    ("hivemind", "Hivemind"),
    ("progenitor", "Progenitor"),
    ("archives_interface", "Archives Interface"),
    ("virus_breeding_ground", "Virus Breeding Ground"),
    ("qianju_pt", "Qianju PT"),
];

/// *Upstalk* (`upstalk`): tranche F8 of the FFG plan.
pub(crate) const UP_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("domestic_sleepers", "Domestic Sleepers"),
    ("mutate", "Mutate"),
    ("primary_transmission_dish", "Primary Transmission Dish"),
    ("midway_station_grid", "Midway Station Grid"),
    ("the_root", "The Root"),
    ("taurus", "Taurus"),
    ("mother_goddess", "Mother Goddess"),
    ("galahad", "Galahad"),
    ("bad_times", "Bad Times"),
    ("cyber_threat", "Cyber Threat"),
    ("lamprey", "Lamprey"),
    ("paper_tripping", "Paper Tripping"),
    ("power_tap", "Power Tap"),
    ("nasir_meidan_cyber_explorer", "Nasir Meidan: Cyber Explorer"),
    ("social_engineering", "Social Engineering"),
    ("leprechaun", "Leprechaun"),
    ("eden_shard", "Eden Shard"),
];

/// *The Spaces Between* (`the_spaces_between`): tranche F8 of the FFG plan.
pub(crate) const TSB_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("the_foundry_refining_the_process", "The Foundry: Refining the Process"),
    ("enhanced_login_protocol", "Enhanced Login Protocol"),
    ("heinlein_grid", "Heinlein Grid"),
    ("encrypted_portals", "Encrypted Portals"),
    ("cerebral_static", "Cerebral Static"),
    ("targeted_marketing", "Targeted Marketing"),
    ("information_overload", "Information Overload"),
    ("paywall_implementation", "Paywall Implementation"),
    ("sealed_vault", "Sealed Vault"),
    ("eden_fragment", "Eden Fragment"),
    ("lag_time", "Lag Time"),
    ("will_o_the_wisp", "Will-o'-the-Wisp"),
    ("d4v1d", "D4v1d"),
    ("scrubbed", "Scrubbed"),
    ("three_steps_ahead", "Three Steps Ahead"),
    ("unscheduled_maintenance", "Unscheduled Maintenance"),
    ("net_celebrity", "Net Celebrity"),
    ("llds_energy_regulator", "LLDS Energy Regulator"),
    ("ghost_runner", "Ghost Runner"),
];

/// *First Contact* (`first_contact`): tranche F8 of the FFG plan.
pub(crate) const FC_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("iq", "IQ"),
    ("elizas_toybox", "Eliza's Toybox"),
    ("kitsune", "Kitsune"),
    ("port_anson_grid", "Port Anson Grid"),
    ("the_news_now_hour", "The News Now Hour"),
    ("manhunt", "Manhunt"),
    ("wendigo", "Wendigo"),
    ("chronos_project", "Chronos Project"),
    ("shattered_remains", "Shattered Remains"),
    ("lancelot", "Lancelot"),
    ("blackat", "BlacKat"),
    ("duggars", "Duggar's"),
    ("box_e", "Box-E"),
    ("the_supplier", "The Supplier"),
    ("refractor", "Refractor"),
    ("order_of_sol", "Order of Sol"),
    ("hades_shard", "Hades Shard"),
    ("rachel_beckman", "Rachel Beckman"),
];

/// *Up and Over* (`up_and_over`): tranche F8 of the FFG plan.
pub(crate) const UAO_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("architect", "Architect"),
    ("peak_efficiency", "Peak Efficiency"),
    ("labyrinthine_servers", "Labyrinthine Servers"),
    ("ashigaru", "Ashigaru"),
    ("mamba", "Mamba"),
    ("universal_connectivity_fee", "Universal Connectivity Fee"),
    ("blue_sun_powering_the_future", "Blue Sun: Powering the Future"),
    ("changeling", "Changeling"),
    ("reuse", "Reuse"),
    ("hades_fragment", "Hades Fragment"),
    ("docklands_crackdown", "Docklands Crackdown"),
    ("inject", "Inject"),
    ("origami", "Origami"),
    ("fester", "Fester"),
    ("autoscripter", "Autoscripter"),
    ("switchblade", "Switchblade"),
    ("trade_in", "Trade-In"),
    ("astrolabe", "Astrolabe"),
    ("angel_arena", "Angel Arena"),
];

/// *All That Remains* (`all_that_remains`): tranche F8 of the FFG plan.
pub(crate) const ATR_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("bifrost_array", "Bifrost Array"),
    ("sagittarius", "Sagittarius"),
    ("gemini", "Gemini"),
    ("superior_cyberwalls", "Superior Cyberwalls"),
    ("lycan", "Lycan"),
    ("snatch_and_grab", "Snatch and Grab"),
    ("merlin", "Merlin"),
    ("shell_corporation", "Shell Corporation"),
    ("ekomind", "Ekomind"),
    ("cerberus_cuj_0_h3", "Cerberus \"Cuj.0\" H3"),
    ("leela_patel_trained_pragmatist", "Leela Patel: Trained Pragmatist"),
    ("cerberus_rex_h2", "Cerberus \"Rex\" H2"),
    ("zona_sul_shipping", "Zona Sul Shipping"),
    ("cybsoft_macrodrive", "Cybsoft MacroDrive"),
    ("utopia_shard", "Utopia Shard"),
];

/// *The Source* (`the_source`): tranche F8 of the FFG plan.
pub(crate) const TS_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("helium_3_deposit", "Helium-3 Deposit"),
    ("errand_boy", "Errand Boy"),
    ("it_department", "IT Department"),
    ("markus_1_0", "Markus 1.0"),
    ("industrial_genomics_growing_solutions", "Industrial Genomics: Growing Solutions"),
    ("shoot_the_moon", "Shoot the Moon"),
    ("troll", "Troll"),
    ("virgo", "Virgo"),
    ("utopia_fragment", "Utopia Fragment"),
    ("self_destruct", "Self-destruct"),
    ("incubator", "Incubator"),
    ("ixodidae", "Ixodidae"),
    ("code_siphon", "Code Siphon"),
    ("collective_consciousness", "Collective Consciousness"),
    ("sage", "Sage"),
    ("bribery", "Bribery"),
    ("au_revoir", "Au Revoir"),
];

/// *Honor and Profit* (`honor_and_profit`): tranche F8 of the FFG plan.
pub(crate) const HAP_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("harmony_medtech_biomedical_pioneer", "Harmony Medtech: Biomedical Pioneer"),
    ("nisei_division_the_next_generation", "Nisei Division: The Next Generation"),
    ("tennin_institute_the_secrets_within", "Tennin Institute: The Secrets Within"),
    ("medical_breakthrough", "Medical Breakthrough"),
    ("philotic_entanglement", "Philotic Entanglement"),
    ("the_future_perfect", "The Future Perfect"),
    ("chairman_hiro", "Chairman Hiro"),
    ("mental_health_clinic", "Mental Health Clinic"),
    ("psychic_field", "Psychic Field"),
    ("shi_kyu", "Shi.Kyū"),
    ("tenma_line", "Tenma Line"),
    ("cerebral_cast", "Cerebral Cast"),
    ("medical_research_fundraiser", "Medical Research Fundraiser"),
    ("mushin_no_shin", "Mushin No Shin"),
    ("inazuma", "Inazuma"),
    ("komainu", "Komainu"),
    ("pup", "Pup"),
    ("shiro", "Shiro"),
    ("susanoo_no_mikoto", "Susanoo-no-Mikoto"),
    ("neotokyo_grid", "NeoTokyo Grid"),
    ("tori_hanzo", "Tori Hanzō"),
    ("plan_b", "Plan B"),
    ("guard", "Guard"),
    ("rainbow", "Rainbow"),
    ("diversified_portfolio", "Diversified Portfolio"),
    ("fast_track", "Fast Track"),
    ("iain_stirling_retired_spook", "Iain Stirling: Retired Spook"),
    ("silhouette_stealth_operative", "Silhouette: Stealth Operative"),
    ("calling_in_favors", "Calling in Favors"),
    ("early_bird", "Early Bird"),
    ("express_delivery", "Express Delivery"),
    ("feint", "Feint"),
    ("planned_assault", "Planned Assault"),
    ("logos", "Logos"),
    ("public_terminal", "Public Terminal"),
    ("unregistered_s_w_35", "Unregistered S&W '35"),
    ("window", "Window"),
    ("alias", "Alias"),
    ("breach", "Breach"),
    ("bug", "Bug"),
    ("gingerbread", "Gingerbread"),
    ("grappling_hook", "Grappling Hook"),
    ("passport", "Passport"),
    ("push_your_luck", "Push Your Luck"),
    ("theophilius_bagbiter", "Theophilius Bagbiter"),
    ("tri_maf_contact", "Tri-maf Contact"),
    ("mass_install", "Mass Install"),
    ("q_coherence_chip", "Q-Coherence Chip"),
    ("overmind", "Overmind"),
    ("oracle_may", "Oracle May"),
    ("donut_taganes", "Donut Taganes"),
];

/// *Creation and Control* (`creation_and_control`): tranche F9 of the FFG plan.
pub(crate) const CAC_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("cerebral_imaging_infinite_frontiers", "Cerebral Imaging: Infinite Frontiers"),
    ("custom_biotics_engineered_for_success", "Custom Biotics: Engineered for Success"),
    ("next_design_guarding_the_net", "NEXT Design: Guarding the Net"),
    ("director_haas_pet_project", "Director Haas' Pet Project"),
    ("efficiency_committee", "Efficiency Committee"),
    ("project_wotan", "Project Wotan"),
    ("sentinel_defense_program", "Sentinel Defense Program"),
    ("alix_t4lb07", "Alix T4LB07"),
    ("director_haas", "Director Haas"),
    ("haas_arcology_ai", "Haas Arcology AI"),
    ("thomas_haas", "Thomas Haas"),
    ("bioroid_efficiency_research", "Bioroid Efficiency Research"),
    ("successful_demonstration", "Successful Demonstration"),
    ("heimdall_2_0", "Heimdall 2.0"),
    ("howler", "Howler"),
    ("ichi_2_0", "Ichi 2.0"),
    ("minelayer", "Minelayer"),
    ("viktor_2_0", "Viktor 2.0"),
    ("zed_1_0", "Zed 1.0"),
    ("awakening_center", "Awakening Center"),
    ("tyrs_hand", "Tyr's Hand"),
    ("gila_hands_arcology", "Gila Hands Arcology"),
    ("levy_university", "Levy University"),
    ("server_diagnostics", "Server Diagnostics"),
    ("bastion", "Bastion"),
    ("datapike", "Datapike"),
    ("the_professor_keeper_of_knowledge", "The Professor: Keeper of Knowledge"),
    ("exile_streethawk", "Exile: Streethawk"),
    ("escher", "Escher"),
    ("exploratory_romp", "Exploratory Romp"),
    ("freelance_coding_contract", "Freelance Coding Contract"),
    ("scavenge", "Scavenge"),
    ("levy_ar_lab_access", "Levy AR Lab Access"),
    ("monolith", "Monolith"),
    ("feedback_filter", "Feedback Filter"),
    ("clone_chip", "Clone Chip"),
    ("omni_drive", "Omni-drive"),
    ("cloak", "Cloak"),
    ("dagger", "Dagger"),
    ("chakana", "Chakana"),
    ("cyber_cypher", "Cyber-Cypher"),
    ("sahasrara", "Sahasrara"),
    ("inti", "Inti"),
    ("borrowed_satellite", "Borrowed Satellite"),
    ("ice_analyzer", "Ice Analyzer"),
    ("same_old_thing", "Same Old Thing"),
    ("the_source", "The Source"),
];

/// *Opening Moves* (`opening_moves`): tranche F10 of the FFG plan.
pub(crate) const OM_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("frame_job", "Frame Job"),
    ("pawn", "Pawn"),
    ("rook", "Rook"),
    ("hostage", "Hostage"),
    ("gorman_drip_v1", "Gorman Drip v1"),
    ("lockpick", "Lockpick"),
    ("false_echo", "False Echo"),
    ("motivation", "Motivation"),
    ("john_masanori", "John Masanori"),
    ("character_assassination", "Character Assassination"),
    ("jackson_howard", "Jackson Howard"),
    ("invasion_of_privacy", "Invasion of Privacy"),
    ("geothermal_fracking", "Geothermal Fracking"),
    ("swarm", "Swarm"),
    ("cyberdex_trial", "Cyberdex Trial"),
    ("grim", "Grim"),
];

/// *Second Thoughts* (`second_thoughts`): tranche F10 of the FFG plan.
pub(crate) const ST_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("bishop", "Bishop"),
    ("scheherazade", "Scheherazade"),
    ("hard_at_work", "Hard at Work"),
    ("recon", "Recon"),
    ("copycat", "Copycat"),
    ("leviathan", "Leviathan"),
    ("eureka", "Eureka!"),
    ("record_reconstructor", "Record Reconstructor"),
    ("wotan", "Wotan"),
    ("hellion_alpha_test", "Hellion Alpha Test"),
    ("clone_retirement", "Clone Retirement"),
    ("shipment_from_sansan", "Shipment from SanSan"),
    ("muckraker", "Muckraker"),
    ("off_the_grid", "Off the Grid"),
    ("profiteering", "Profiteering"),
    ("restructure", "Restructure"),
];

/// *Mala Tempora* (`mala_tempora`): tranche F10 of the FFG plan.
pub(crate) const MT_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("deep_red", "Deep Red"),
    ("knight", "Knight"),
    ("running_interference", "Running Interference"),
    ("expert_schedule_analyzer", "Expert Schedule Analyzer"),
    ("grifter", "Grifter"),
    ("torch", "Torch"),
    ("woman_in_the_red_dress", "Woman in the Red Dress"),
    ("raymond_flint", "Raymond Flint"),
    ("isabel_mcguire", "Isabel McGuire"),
    ("accelerated_diagnostics", "Accelerated Diagnostics"),
    ("unorthodox_predictions", "Unorthodox Predictions"),
    ("sundew", "Sundew"),
    ("city_surveillance", "City Surveillance"),
    ("snoop", "Snoop"),
    ("ireress", "Ireress"),
    ("power_shutdown", "Power Shutdown"),
    ("paper_wall", "Paper Wall"),
    ("interns", "Interns"),
];

/// *True Colors* (`true_colors`): tranche F10 of the FFG plan.
pub(crate) const TC_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("keyhole", "Keyhole"),
    ("activist_support", "Activist Support"),
    ("lawyer_up", "Lawyer Up"),
    ("leverage", "Leverage"),
    ("garrote", "Garrote"),
    ("llds_processor", "LLDS Processor"),
    ("sharpshooter", "Sharpshooter"),
    ("capstone", "Capstone"),
    ("starlight_crusade_funding", "Starlight Crusade Funding"),
    ("rex_campaign", "Rex Campaign"),
    ("fenris", "Fenris"),
    ("panic_button", "Panic Button"),
    ("shock", "Shock!"),
    ("tsurugi", "Tsurugi"),
    ("rsvp", "RSVP"),
    ("curtain_wall", "Curtain Wall"),
    ("veterans_program", "Veterans Program"),
];

/// *Fear and Loathing* (`fear_and_loathing`): tranche F10 of the FFG plan.
pub(crate) const FAL_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("quest_completed", "Quest Completed"),
    ("tallie_perrault", "Tallie Perrault"),
    ("executive_wiretaps", "Executive Wiretaps"),
    ("blackguard", "Blackguard"),
    ("cybersolutions_mem_chip", "CyberSolutions Mem Chip"),
    ("alpha", "Alpha"),
    ("omega", "Omega"),
    ("blackmail", "Blackmail"),
    ("blue_level_clearance", "Blue Level Clearance"),
    ("toshiyuki_sakai", "Toshiyuki Sakai"),
    ("restoring_face", "Restoring Face"),
    ("market_research", "Market Research"),
    ("grndl_power_unleashed", "GRNDL: Power Unleashed"),
    ("vulcan_coverup", "Vulcan Coverup"),
];

/// *Double Time* (`double_time`): tranche F10 of the FFG plan.
pub(crate) const DT_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("queens_gambit", "Queen's Gambit"),
    ("dyson_fractal_generator", "Dyson Fractal Generator"),
    ("silencer", "Silencer"),
    ("savoir_faire", "Savoir-faire"),
    ("power_nap", "Power Nap"),
    ("paintbrush", "Paintbrush"),
    ("gyri_labyrinth", "Gyri Labyrinth"),
    ("reclamation_order", "Reclamation Order"),
    ("broadcast_square", "Broadcast Square"),
    ("corporate_shuffle", "Corporate Shuffle"),
    ("caprice_nisei", "Caprice Nisei"),
    ("shinobi", "Shinobi"),
    ("marker", "Marker"),
    ("witness_tampering", "Witness Tampering"),
    ("napd_contract", "NAPD Contract"),
    ("quandary", "Quandary"),
];

/// *What Lies Ahead* (`what_lies_ahead`): tranche F11 of the FFG plan.
pub(crate) const WLA_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("whizzard_master_gamer", "Whizzard: Master Gamer"),
    ("cortez_chip", "Cortez Chip"),
    ("zu_13_key_master", "ZU.13 Key Master"),
    ("the_helpful_ai", "The Helpful AI"),
    ("plascrete_carapace", "Plascrete Carapace"),
    ("mandatory_upgrades", "Mandatory Upgrades"),
    ("janus_1_0", "Janus 1.0"),
    ("snowflake", "Snowflake"),
    ("restructured_datapool", "Restructured Datapool"),
    ("tmi", "TMI"),
    ("draco", "Dracō"),
];

/// *Trace Amount* (`trace_amount`): tranche F11 of the FFG plan.
pub(crate) const TA_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("vamp", "Vamp"),
    ("satellite_uplink", "Satellite Uplink"),
    ("compromised_employee", "Compromised Employee"),
    ("snowball", "Snowball"),
    ("encryption_protocol", "Encryption Protocol"),
    ("sherlock_1_0", "Sherlock 1.0"),
    ("jinteki_replicating_perfection", "Jinteki: Replicating Perfection"),
    ("fetal_ai", "Fetal AI"),
    ("sensei", "Sensei"),
    ("big_brother", "Big Brother"),
    ("chilo_city_grid", "ChiLo City Grid"),
    ("power_grid_overload", "Power Grid Overload"),
    ("amazon_industrial_zone", "Amazon Industrial Zone"),
    ("executive_retreat", "Executive Retreat"),
    ("freelancer", "Freelancer"),
];

/// *Cyber Exodus* (`cyber_exodus`): tranche F11 of the FFG plan.
pub(crate) const CE_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("nerve_agent", "Nerve Agent"),
    ("joshua_b", "Joshua B."),
    ("muresh_bodysuit", "Muresh Bodysuit"),
    ("snitch", "Snitch"),
    ("personal_workshop", "Personal Workshop"),
    ("public_sympathy", "Public Sympathy"),
    ("viper", "Viper"),
    ("edge_of_world", "Edge of World"),
    ("sunset", "Sunset"),
    ("marked_accounts", "Marked Accounts"),
    ("woodcutter", "Woodcutter"),
    ("commercialization", "Commercialization"),
    ("private_contracts", "Private Contracts"),
    ("chimera", "Chimera"),
];

/// *A Study in Static* (`a_study_in_static`): tranche F11 of the FFG plan.
pub(crate) const ASIS_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("disrupter", "Disrupter"),
    ("crescentus", "Crescentus"),
    ("deus_x", "Deus X"),
    ("inside_man", "Inside Man"),
    ("hourglass", "Hourglass"),
    ("dedicated_server", "Dedicated Server"),
    ("bullfrog", "Bullfrog"),
    ("uroboros", "Uroboros"),
    ("net_police", "Net Police"),
    ("weyland_consortium_because_we_built_it", "Weyland Consortium: Because We Built It"),
    ("government_contracts", "Government Contracts"),
    ("tyrant", "Tyrant"),
    ("oversight_ai", "Oversight AI"),
];

/// *Humanity's Shadow* (`humanitys_shadow`): tranche F11 of the FFG plan.
pub(crate) const HS_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("surge", "Surge"),
    ("andromeda_dispossessed_ristie", "Andromeda: Dispossessed Ristie"),
    ("quality_time", "Quality Time"),
    ("replicator", "Replicator"),
    ("creeper", "Creeper"),
    ("kraken", "Kraken"),
    ("kati_jones", "Kati Jones"),
    ("eve_campaign", "Eve Campaign"),
    ("rework", "Rework"),
    ("data_hound", "Data Hound"),
    ("salvage", "Salvage"),
    ("simone_diego", "Simone Diego"),
    ("foxfire", "Foxfire"),
];

/// *Future Proof* (`future_proof`): tranche F11 of the FFG plan.
pub(crate) const FP_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("data_leak_reversal", "Data Leak Reversal"),
    ("r_d_interface", "R&D Interface"),
    ("deep_thought", "Deep Thought"),
    ("new_angeles_city_hall", "New Angeles City Hall"),
    ("ruhr_valley", "Ruhr Valley"),
    ("midori", "Midori"),
    ("nbn_the_world_is_yours", "NBN: The World is Yours*"),
    ("midseason_replacements", "Midseason Replacements"),
    ("burke_bugs", "Burke Bugs"),
    ("corporate_war", "Corporate War"),
];

/// *NAPD Multiplayer* (`napd_multiplayer`): tranche F12 of the FFG plan.
pub(crate) const NAPD_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("cyber_bureau_keeping_the_peace", "Cyber Bureau: Keeping the Peace"),
];

/// *Magnum Opus* (`magnum_opus`): tranche F12 of the FFG plan.
pub(crate) const MO_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("watch_the_world_burn", "Watch the World Burn"),
    ("hired_help", "Hired Help"),
];

/// *System Core 2019* (`system_core_2019`): tranche F12 of the FFG plan.
pub(crate) const SC19_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("queens_gambit", "Queen's Gambit"),
    ("quest_completed", "Quest Completed"),
    ("lamprey", "Lamprey"),
    ("leela_patel_trained_pragmatist", "Leela Patel: Trained Pragmatist"),
    ("hostage", "Hostage"),
    ("r_d_interface", "R&D Interface"),
    ("deus_x", "Deus X"),
    ("ice_analyzer", "Ice Analyzer"),
    ("john_masanori", "John Masanori"),
    ("kati_jones", "Kati Jones"),
    ("seidr_laboratories_destiny_defined", "Seidr Laboratories: Destiny Defined"),
    ("blue_level_clearance", "Blue Level Clearance"),
    ("jinteki_replicating_perfection", "Jinteki: Replicating Perfection"),
    ("fetal_ai", "Fetal AI"),
    ("philotic_entanglement", "Philotic Entanglement"),
    ("sundew", "Sundew"),
    ("tsurugi", "Tsurugi"),
    ("spark_agency_worldswide_reach", "Spark Agency: Worldswide Reach"),
    ("marked_accounts", "Marked Accounts"),
    ("blue_sun_powering_the_future", "Blue Sun: Powering the Future"),
    ("oversight_ai", "Oversight AI"),
];
