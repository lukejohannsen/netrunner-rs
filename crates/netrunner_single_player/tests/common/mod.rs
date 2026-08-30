//! Builds a legal 45-card Kate "Mac" McCaffrey (Runner) vs. Haas-Bioroid:
//! Engineering the Future (Corp) matchup, for this crate's integration
//! tests. Deliberately a near-identical copy of
//! `netrunner_server::fixtures`/`netrunner_cli::decks` rather than a shared
//! dependency: neither is importable from here (one's a binary crate, the
//! other would defeat the point of this crate having no non-core/bots
//! deps), and the fixture is small enough that inventing a shared crate
//! just to deduplicate it isn't worth the indirection — same rationale as
//! `netrunner_server::fixtures`'s own doc comment.

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::rules::{Deck, Side};

const CORP_IDENTITY: &str = "haas_bioroid_engineering_the_future";
const RUNNER_IDENTITY: &str = "kate_mccaffrey";

const BASELINE_CORP_CARDS: [&str; 7] =
    ["hedge_fund", "scorched_earth", "hostile_takeover", "pad_campaign", "snare", "enigma", "wall_of_static"];
const BASELINE_RUNNER_CARDS: [&str; 6] =
    ["sure_gamble", "diesel", "the_makers_eye", "account_siphon", "corroder", "gordian_blade"];

const FILLER_AGENDA_COUNT: u32 = 6;
const FILLER_ASSET_COUNT: u32 = 2;
const FILLER_EVENT_COUNT: u32 = 9;

fn blank_card(id: String, side: Side, card_type: CardType) -> CardDefinition {
    CardDefinition {
        title: id.clone(),
        id: CardId(id),
        side,
        card_type,
        is_playable: true,
        ..Default::default()
    }
}

fn filler_agenda_id(index: u32) -> String {
    format!("filler_agenda_{index}")
}

fn filler_asset_id(index: u32) -> String {
    format!("filler_asset_{index}")
}

fn filler_event_id(index: u32) -> String {
    format!("filler_event_{index}")
}

#[allow(dead_code)]
pub fn kate_vs_hb_registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    cards::register_playable_cards(&mut registry);

    for index in 0..FILLER_AGENDA_COUNT {
        let mut agenda = blank_card(filler_agenda_id(index), Side::Corp, CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(1);
        registry.insert(agenda);
    }
    for index in 0..FILLER_ASSET_COUNT {
        registry.insert(blank_card(filler_asset_id(index), Side::Corp, CardType::Asset));
    }
    for index in 0..FILLER_EVENT_COUNT {
        registry.insert(blank_card(filler_event_id(index), Side::Runner, CardType::Event));
    }

    registry
}

/// A System Gateway matchup built for *mechanic coverage*: every card in
/// both decks is a real hand-authored System Gateway card with genuine DSL
/// rules — no synthetic filler at all, unlike the Kate-vs-HB fixture above.
///
/// The card mix is chosen so a bot-driven sweep actually reaches the
/// machinery the System Gateway work added, rather than passing vacuously
/// over a deck of blanks. Between the two decks it covers: hosting/Trojans
/// (Botulus, Tranquilizer), bioroid click-to-break (Ansel 1.0, Brân 1.0),
/// the pending-decision primitives (Sprint, Hansei Review, Longevity Serum,
/// Ballista, Retribution, Above the Law, Wildcat Strike, Mutual Favor),
/// pay-or-suffer choices (Funhouse, Manegarm Skunkworks, Anoetic Void,
/// Public Trail), facedown Archives (Corp discards feeding Spin Doctor and
/// Longevity Serum), dynamic amounts (Neurospike, Clearinghouse, Urtica
/// Cipher, Conduit, Echelon, Unity), conditional strength (Palisade,
/// Pharos), advanceable non-agendas (Clearinghouse, Urtica Cipher, Pharos,
/// Seamless Launch), persistent-after-trash (AMAZE Amusements),
/// remove-from-game (Spin Doctor), the scoring lockout (Luminal
/// Transubstantiation), hosted-credit pools (Nico Campaign, Regolith Mining
/// License, Red Team, Telework Contract, Smartware Distributor,
/// Pennyshaver), consoles and MU (Carnivore, Pantograph, Pennyshaver,
/// T400 Memory Diamond, DZMZ Optimizer), and the click-loss events
/// (Creative Commission, VRcation).
///
/// Deck legality here is `rules::deck::validate_deck`'s gameplay gate —
/// size, copy limits, side, agenda-point range. It deliberately does *not*
/// check influence, so this mixes factions freely to maximise coverage; it
/// is a test fixture, not a tournament-legal list.
const SG_CORP_IDENTITY: &str = "nbn_reality_plus";
const SG_RUNNER_IDENTITY: &str = "zahya_sadeghi";

/// `(card id, copies)`. Corp: 40 cards, 20 agenda points — the top of the
/// `[18, 20]` range `agenda_point_range` requires below 45 cards. The four
/// "Limit 1 per deck" agendas take a single copy each, matching their
/// printed limit even though this gate only enforces the flat
/// `MAX_COPIES_PER_CARD`.
const SG_CORP_DECK: [(&str, u32); 34] = [
    // Agendas — 10 cards, 20 points.
    ("offworld_office", 3),
    ("orbital_superiority", 3),
    ("luminal_transubstantiation", 1),
    ("tomorrows_headline", 1),
    ("above_the_law", 1),
    ("longevity_serum", 1),
    // Ice — 12.
    ("ansel_1_0", 2),
    ("bran_1_0", 1),
    ("ballista", 1),
    ("diviner", 1),
    ("funhouse", 2),
    ("palisade", 1),
    ("pharos", 1),
    ("ping", 1),
    ("tithe", 1),
    ("whitespace", 1),
    // Upgrades — 4.
    ("amaze_amusements", 1),
    ("anoetic_void", 1),
    ("malapert_data_vault", 1),
    ("manegarm_skunkworks", 1),
    // Assets — 5.
    ("clearinghouse", 1),
    ("nico_campaign", 1),
    ("regolith_mining_license", 1),
    ("spin_doctor", 1),
    ("urtica_cipher", 1),
    // Operations — 9.
    ("government_subsidy", 1),
    ("hansei_review", 1),
    ("hedge_fund", 1),
    ("neurospike", 1),
    ("predictive_planogram", 1),
    ("public_trail", 1),
    ("retribution", 1),
    ("seamless_launch", 1),
    ("sprint", 1),
];

/// Runner: 40 cards. No agenda-point constraint applies to Runner decks.
const SG_RUNNER_DECK: [(&str, u32); 31] = [
    // Icebreakers — 10.
    ("buzzsaw", 2),
    ("cleaver", 2),
    ("carmen", 1),
    ("marjanah", 2),
    ("echelon", 1),
    ("unity", 1),
    ("mayfly", 1),
    // Trojans (hosted on ice) — 3.
    ("botulus", 2),
    ("tranquilizer", 1),
    // Other programs — 4.
    ("leech", 2),
    ("fermenter", 1),
    ("conduit", 1),
    // Hardware — 7.
    ("carnivore", 1),
    ("docklands_pass", 2),
    ("dzmz_optimizer", 1),
    ("pantograph", 1),
    ("pennyshaver", 1),
    ("t400_memory_diamond", 1),
    // Resources — 5.
    ("cookbook", 1),
    ("red_team", 1),
    ("smartware_distributor", 1),
    ("telework_contract", 1),
    ("verbal_plasticity", 1),
    // Events — 11.
    ("creative_commission", 2),
    ("jailbreak", 2),
    ("mutual_favor", 1),
    ("overclock", 1),
    ("sure_gamble", 2),
    ("tread_lightly", 1),
    ("vrcation", 1),
    ("wildcat_strike", 1),
];

/// A representative slice of each deck, for the delivery-proof test's
/// "these are real, playable, rules-carrying System Gateway cards" check.
/// `#[allow(dead_code)]` because each test binary compiles this module
/// separately, and `single_player_test.rs` uses only the Kate-vs-HB half.
#[allow(dead_code)]
pub const SG_CORP_CARDS: [&str; 4] = ["tithe", "government_subsidy", "palisade", "whitespace"];
#[allow(dead_code)]
pub const SG_RUNNER_CARDS: [&str; 4] = ["buzzsaw", "cleaver", "docklands_pass", "carmen"];

/// The registry a consumer builds, reached through the ordinary entry point
/// (`cards::register_playable_cards`) — no `fs-loader`, no filesystem, and
/// no synthetic filler cards: every card both System Gateway decks
/// reference is a real one.
#[allow(dead_code)]
pub fn sg_registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    cards::register_playable_cards(&mut registry);
    registry
}

#[allow(dead_code)]
pub fn sg_decks() -> (Deck, Deck) {
    let to_cards = |deck: &[(&str, u32)]| -> Vec<(CardId, u32)> {
        deck.iter().map(|(id, copies)| (CardId(id.to_string()), *copies)).collect()
    };

    let corp_deck = Deck { identity: CardId(SG_CORP_IDENTITY.to_string()), cards: to_cards(&SG_CORP_DECK) };
    let runner_deck = Deck { identity: CardId(SG_RUNNER_IDENTITY.to_string()), cards: to_cards(&SG_RUNNER_DECK) };
    (corp_deck, runner_deck)
}

#[allow(dead_code)]
pub fn kate_vs_hb_decks() -> (Deck, Deck) {
    let mut corp_cards: Vec<(CardId, u32)> =
        BASELINE_CORP_CARDS.into_iter().map(|id| (CardId(id.to_string()), 3)).collect();
    corp_cards.extend((0..FILLER_AGENDA_COUNT).map(|index| (CardId(filler_agenda_id(index)), 3)));
    corp_cards.extend((0..FILLER_ASSET_COUNT).map(|index| (CardId(filler_asset_id(index)), 3)));

    let mut runner_cards: Vec<(CardId, u32)> =
        BASELINE_RUNNER_CARDS.into_iter().map(|id| (CardId(id.to_string()), 3)).collect();
    runner_cards.extend((0..FILLER_EVENT_COUNT).map(|index| (CardId(filler_event_id(index)), 3)));

    let corp_deck = Deck { identity: CardId(CORP_IDENTITY.to_string()), cards: corp_cards };
    let runner_deck = Deck { identity: CardId(RUNNER_IDENTITY.to_string()), cards: runner_cards };
    (corp_deck, runner_deck)
}
