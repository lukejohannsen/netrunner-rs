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
    ("gene_conditioning_shoppe", "Gene Conditioning Shoppe"),
    ("jinteki_biotech_life_imagined", "Jinteki Biotech: Life Imagined"),
];

/// *Breaker Bay* (`breaker_bay`): tranche F7 of the FFG plan.
pub(crate) const BB_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("off_campus_apartment", "Off-Campus Apartment"),
    ("london_library", "London Library"),
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
    ("allele_repression", "Allele Repression"),
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
    ("mark_yale", "Mark Yale"),
    ("satellite_grid", "Satellite Grid"),
    ("the_twins", "The Twins"),
    ("showing_off", "Showing Off"),
    ("wanton_destruction", "Wanton Destruction"),
    ("hivemind", "Hivemind"),
    ("progenitor", "Progenitor"),
    ("archives_interface", "Archives Interface"),
    ("qianju_pt", "Qianju PT"),
];

/// *Upstalk* (`upstalk`): tranche F8 of the FFG plan.
pub(crate) const UP_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("domestic_sleepers", "Domestic Sleepers"),
    ("mutate", "Mutate"),
    ("the_root", "The Root"),
    ("taurus", "Taurus"),
    ("mother_goddess", "Mother Goddess"),
    ("galahad", "Galahad"),
    ("bad_times", "Bad Times"),
    ("cyber_threat", "Cyber Threat"),
    ("power_tap", "Power Tap"),
    ("nasir_meidan_cyber_explorer", "Nasir Meidan: Cyber Explorer"),
    ("social_engineering", "Social Engineering"),
    ("eden_shard", "Eden Shard"),
];

/// *The Spaces Between* (`the_spaces_between`): tranche F8 of the FFG plan.
pub(crate) const TSB_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("the_foundry_refining_the_process", "The Foundry: Refining the Process"),
    ("enhanced_login_protocol", "Enhanced Login Protocol"),
    ("heinlein_grid", "Heinlein Grid"),
    ("cerebral_static", "Cerebral Static"),
    ("targeted_marketing", "Targeted Marketing"),
    ("sealed_vault", "Sealed Vault"),
    ("eden_fragment", "Eden Fragment"),
    ("will_o_the_wisp", "Will-o'-the-Wisp"),
    ("scrubbed", "Scrubbed"),
    ("unscheduled_maintenance", "Unscheduled Maintenance"),
    ("net_celebrity", "Net Celebrity"),
];

/// *First Contact* (`first_contact`): tranche F8 of the FFG plan.
pub(crate) const FC_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("kitsune", "Kitsune"),
    ("the_news_now_hour", "The News Now Hour"),
    ("wendigo", "Wendigo"),
    ("chronos_project", "Chronos Project"),
    ("shattered_remains", "Shattered Remains"),
    ("lancelot", "Lancelot"),
    ("blackat", "BlacKat"),
    ("the_supplier", "The Supplier"),
    ("order_of_sol", "Order of Sol"),
    ("hades_shard", "Hades Shard"),
];

/// *Up and Over* (`up_and_over`): tranche F8 of the FFG plan.
pub(crate) const UAO_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("architect", "Architect"),
    ("labyrinthine_servers", "Labyrinthine Servers"),
    ("mamba", "Mamba"),
    ("blue_sun_powering_the_future", "Blue Sun: Powering the Future"),
    ("changeling", "Changeling"),
    ("docklands_crackdown", "Docklands Crackdown"),
    ("inject", "Inject"),
    ("autoscripter", "Autoscripter"),
    ("trade_in", "Trade-In"),
    ("angel_arena", "Angel Arena"),
];

/// *All That Remains* (`all_that_remains`): tranche F8 of the FFG plan.
pub(crate) const ATR_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("bifrost_array", "Bifrost Array"),
    ("sagittarius", "Sagittarius"),
    ("gemini", "Gemini"),
    ("lycan", "Lycan"),
    ("snatch_and_grab", "Snatch and Grab"),
    ("merlin", "Merlin"),
    ("shell_corporation", "Shell Corporation"),
    ("ekomind", "Ekomind"),
    ("utopia_shard", "Utopia Shard"),
];

/// *The Source* (`the_source`): tranche F8 of the FFG plan.
pub(crate) const TS_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("helium_3_deposit", "Helium-3 Deposit"),
    ("it_department", "IT Department"),
    ("industrial_genomics_growing_solutions", "Industrial Genomics: Growing Solutions"),
    ("virgo", "Virgo"),
    ("utopia_fragment", "Utopia Fragment"),
    ("self_destruct", "Self-destruct"),
    ("incubator", "Incubator"),
    ("ixodidae", "Ixodidae"),
    ("code_siphon", "Code Siphon"),
    ("bribery", "Bribery"),
];

/// *Honor and Profit* (`honor_and_profit`): tranche F8 of the FFG plan.
pub(crate) const HAP_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("harmony_medtech_biomedical_pioneer", "Harmony Medtech: Biomedical Pioneer"),
    ("medical_breakthrough", "Medical Breakthrough"),
    ("the_future_perfect", "The Future Perfect"),
    ("psychic_field", "Psychic Field"),
    ("shi_kyu", "Shi.Kyū"),
    ("mushin_no_shin", "Mushin No Shin"),
    ("inazuma", "Inazuma"),
    ("shiro", "Shiro"),
    ("susanoo_no_mikoto", "Susanoo-no-Mikoto"),
    ("neotokyo_grid", "NeoTokyo Grid"),
    ("tori_hanzo", "Tori Hanzō"),
    ("plan_b", "Plan B"),
    ("diversified_portfolio", "Diversified Portfolio"),
    ("feint", "Feint"),
    ("planned_assault", "Planned Assault"),
    ("unregistered_s_w_35", "Unregistered S&W '35"),
    ("window", "Window"),
    ("bug", "Bug"),
    ("grappling_hook", "Grappling Hook"),
    ("push_your_luck", "Push Your Luck"),
    ("overmind", "Overmind"),
    ("oracle_may", "Oracle May"),
];

/// *Creation and Control* (`creation_and_control`): tranche F9 of the FFG plan.
pub(crate) const CAC_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("custom_biotics_engineered_for_success", "Custom Biotics: Engineered for Success"),
    ("next_design_guarding_the_net", "NEXT Design: Guarding the Net"),
    ("director_haas_pet_project", "Director Haas' Pet Project"),
    ("project_wotan", "Project Wotan"),
    ("haas_arcology_ai", "Haas Arcology AI"),
    ("bioroid_efficiency_research", "Bioroid Efficiency Research"),
    ("howler", "Howler"),
    ("minelayer", "Minelayer"),
    ("awakening_center", "Awakening Center"),
    ("tyrs_hand", "Tyr's Hand"),
    ("the_professor_keeper_of_knowledge", "The Professor: Keeper of Knowledge"),
    ("exile_streethawk", "Exile: Streethawk"),
    ("escher", "Escher"),
    ("scavenge", "Scavenge"),
    ("monolith", "Monolith"),
    ("omni_drive", "Omni-drive"),
    ("cyber_cypher", "Cyber-Cypher"),
];

/// *Opening Moves* (`opening_moves`): tranche F10 of the FFG plan.
pub(crate) const OM_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("frame_job", "Frame Job"),
    ("pawn", "Pawn"),
    ("rook", "Rook"),
    ("false_echo", "False Echo"),
    ("john_masanori", "John Masanori"),
    ("swarm", "Swarm"),
];

/// *Second Thoughts* (`second_thoughts`): tranche F10 of the FFG plan.
pub(crate) const ST_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("bishop", "Bishop"),
    ("scheherazade", "Scheherazade"),
    ("recon", "Recon"),
    ("copycat", "Copycat"),
    ("eureka", "Eureka!"),
    ("record_reconstructor", "Record Reconstructor"),
    ("off_the_grid", "Off the Grid"),
];

/// *Mala Tempora* (`mala_tempora`): tranche F10 of the FFG plan.
pub(crate) const MT_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("deep_red", "Deep Red"),
    ("knight", "Knight"),
    ("running_interference", "Running Interference"),
    ("raymond_flint", "Raymond Flint"),
    ("accelerated_diagnostics", "Accelerated Diagnostics"),
    ("unorthodox_predictions", "Unorthodox Predictions"),
    ("sundew", "Sundew"),
    ("snoop", "Snoop"),
    ("power_shutdown", "Power Shutdown"),
];

/// *True Colors* (`true_colors`): tranche F10 of the FFG plan.
pub(crate) const TC_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("leverage", "Leverage"),
    ("capstone", "Capstone"),
    ("starlight_crusade_funding", "Starlight Crusade Funding"),
    ("panic_button", "Panic Button"),
    ("curtain_wall", "Curtain Wall"),
];

/// *Fear and Loathing* (`fear_and_loathing`): tranche F10 of the FFG plan.
pub(crate) const FAL_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("tallie_perrault", "Tallie Perrault"),
    ("blackguard", "Blackguard"),
    ("blackmail", "Blackmail"),
    ("toshiyuki_sakai", "Toshiyuki Sakai"),
];

/// *Double Time* (`double_time`): tranche F10 of the FFG plan.
pub(crate) const DT_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("queens_gambit", "Queen's Gambit"),
    ("paintbrush", "Paintbrush"),
    ("reclamation_order", "Reclamation Order"),
    ("broadcast_square", "Broadcast Square"),
    ("corporate_shuffle", "Corporate Shuffle"),
    ("marker", "Marker"),
    ("napd_contract", "NAPD Contract"),
];

/// *What Lies Ahead* (`what_lies_ahead`): tranche F11 of the FFG plan.
pub(crate) const WLA_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("cortez_chip", "Cortez Chip"),
];

/// *Trace Amount* (`trace_amount`): tranche F11 of the FFG plan.
pub(crate) const TA_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("encryption_protocol", "Encryption Protocol"),
    ("fetal_ai", "Fetal AI"),
    ("sensei", "Sensei"),
    ("amazon_industrial_zone", "Amazon Industrial Zone"),
    ("executive_retreat", "Executive Retreat"),
];

/// *Cyber Exodus* (`cyber_exodus`): tranche F11 of the FFG plan.
pub(crate) const CE_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("snitch", "Snitch"),
    ("personal_workshop", "Personal Workshop"),
    ("sunset", "Sunset"),
    ("woodcutter", "Woodcutter"),
    ("chimera", "Chimera"),
];

/// *A Study in Static* (`a_study_in_static`): tranche F11 of the FFG plan.
pub(crate) const ASIS_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("disrupter", "Disrupter"),
    ("crescentus", "Crescentus"),
    ("dedicated_server", "Dedicated Server"),
    ("bullfrog", "Bullfrog"),
    ("net_police", "Net Police"),
    ("weyland_consortium_because_we_built_it", "Weyland Consortium: Because We Built It"),
    ("tyrant", "Tyrant"),
    ("oversight_ai", "Oversight AI"),
];

/// *Humanity's Shadow* (`humanitys_shadow`): tranche F11 of the FFG plan.
pub(crate) const HS_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("surge", "Surge"),
    ("andromeda_dispossessed_ristie", "Andromeda: Dispossessed Ristie"),
    ("replicator", "Replicator"),
    ("kraken", "Kraken"),
    ("kati_jones", "Kati Jones"),
    ("data_hound", "Data Hound"),
    ("salvage", "Salvage"),
    ("simone_diego", "Simone Diego"),
];

/// *Future Proof* (`future_proof`): tranche F11 of the FFG plan.
pub(crate) const FP_UNIMPLEMENTED: &[(&str, &str)] = &[
    ("midori", "Midori"),
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
    ("john_masanori", "John Masanori"),
    ("kati_jones", "Kati Jones"),
    ("seidr_laboratories_destiny_defined", "Seidr Laboratories: Destiny Defined"),
    ("fetal_ai", "Fetal AI"),
    ("sundew", "Sundew"),
    ("spark_agency_worldswide_reach", "Spark Agency: Worldswide Reach"),
    ("blue_sun_powering_the_future", "Blue Sun: Powering the Future"),
    ("oversight_ai", "Oversight AI"),
];
