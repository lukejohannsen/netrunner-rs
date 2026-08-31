//! A fixed-size `f32` feature encoding of a `ClientView` — the shared
//! observation encoder for both `netrunner_gym` (its Gymnasium `Dict`
//! observation space's `"obs"` half; `"action_mask"` is
//! `netrunner_core::rules::get_action_mask` directly) and
//! `netrunner_bots::onnx_policy::OnnxPolicyEvaluator` (its trained model's
//! `"obs"` input tensor). Living here rather than in `netrunner_gym` keeps
//! both consumers on the exact same encoder — `netrunner_gym` already
//! depends on `netrunner_bots`, and the reverse would be a dependency
//! cycle.
//!
//! Two parts: [`SCALAR_COUNT`] normalized scalars (resources, zone counts,
//! run position) and [`PLANE_COUNT`] card-identity planes of [`CARD_VOCAB`]
//! slots each.
//!
//! The scalar half was once the whole encoding. That made every hand of the
//! same size identical to the network — it could learn tempo, but never
//! which card to play — so a policy trained on it could not improve past
//! generic click usage no matter how much self-play it saw. The planes fix
//! that by counting each card id in each visible zone.
//!
//! Everything is read from a `ClientView`, never a `GameState`, so
//! fog-of-war is enforced upstream: `hq_cards`/`grip_cards` are `None` for
//! the non-owner, an unrezzed Corp card masks to `card: None`, and a
//! facedown archived card hides its identity from the Runner. This module
//! encodes only what it is handed, so it cannot leak.

use std::collections::HashMap;
use std::sync::OnceLock;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{ActionSpace, GamePhase, GameState, ServerId, Side};
use netrunner_core::view::{build_client_view, ClientView};

// Normalization caps. Independently defined rather than imported from
// `netrunner_core::rules::action_mask`'s own `MAX_*` constants — that
// module is private, and only `ActionSpace`/`get_action_mask` are
// re-exported from `rules` — so these are this crate's own reasonable
// caps for scaling raw counts into a roughly-unit range, not a claim that
// they bound what's legal.
const MAX_HAND_SIZE: f32 = 12.0;
const MAX_INSTALLED_PER_SIDE: f32 = 20.0;
const MAX_REMOTE_SERVERS: f32 = 10.0;
const MAX_DECK_SIZE: f32 = 45.0;
const MAX_CREDITS: f32 = 30.0;
const MAX_CLICKS: f32 = 5.0;
const MAX_AGENDA_POINTS: f32 = 10.0;
const MAX_BAD_PUBLICITY: f32 = 10.0;
const MAX_TAGS: f32 = 10.0;
const MAX_BRAIN_DAMAGE: f32 = 10.0;
const MAX_MEMORY_UNITS: f32 = 10.0;
const MAX_LINK_STRENGTH: f32 = 5.0;
const MAX_RUN_ICE: f32 = 10.0;

const PHASE_COUNT: usize = 5;

/// Normalizer for a per-card count in one zone. Deck-building caps a card
/// at `MAX_COPIES_PER_CARD` (3), which also bounds how many can be in hand
/// or installed; a discard pile can exceed it, so counts are clamped rather
/// than assumed in range.
const MAX_COPIES_IN_ZONE: f32 = 3.0;

/// side(1) + phase one-hot(5) + self/opp credits+clicks+agenda_points(6) +
/// corp bad publicity(1) + runner tags/brain damage/memory/link(4) +
/// zone counts: hq/rd/archives/grip/stack/heap/rig/installed-ice/
/// installed-root/remote-servers(10) + active-run flag + normalized run
/// position(2) + normalized legal action count(1).
pub const SCALAR_COUNT: usize = 1 + PHASE_COUNT + 6 + 1 + 4 + 10 + 2 + 1;

/// Slots per card-identity plane.
///
/// Deliberately fixed and larger than the current pool (94 playable cards)
/// rather than sized to it: the ONNX model's input shape is baked in at
/// export, so a vocabulary that grew with the card set would invalidate
/// every previously trained model the moment a card was added. 192 leaves
/// room for *Elevation*'s 82 cards without a reshape. The final slot is an
/// overflow bucket for anything unmapped (homebrew, test fixtures, or a
/// pool that outgrows the vocabulary).
pub const CARD_VOCAB: usize = 192;

const OVERFLOW_SLOT: usize = CARD_VOCAB - 1;

/// Card-identity planes, in encoding order: own hand, own installed,
/// opponent's visible installed, own discard, opponent's visible discard.
///
/// Scored agendas get no plane — `agenda_points` already carries the part
/// that drives win/loss, and a sixth plane costs 192 floats in every
/// recorded training step for comparatively little signal.
pub const PLANE_COUNT: usize = 5;

pub const OBS_SIZE: usize = SCALAR_COUNT + PLANE_COUNT * CARD_VOCAB;

/// Maps a card id to its plane slot.
///
/// Built once from `cards::register_playable_cards` — the canonical
/// playable pool — rather than from whatever registry a caller passes, so
/// the mapping is identical for every consumer and stable across processes.
/// Ordered by `(numeric_id, id)`: NetrunnerDB numbers a set contiguously,
/// so a future set's cards sort *after* today's and append to the tail
/// instead of shifting existing slots out from under a trained model.
/// Cards with no `numeric_id` (baseline and homebrew) sort last, by id.
fn vocabulary() -> &'static HashMap<CardId, usize> {
    static VOCABULARY: OnceLock<HashMap<CardId, usize>> = OnceLock::new();
    VOCABULARY.get_or_init(|| {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);

        let mut cards: Vec<(u32, String)> = registry
            .iter()
            .map(|card| (card.numeric_id.map_or(u32::MAX, |numeric| numeric.0), card.id.0.clone()))
            .collect();
        cards.sort();

        cards
            .into_iter()
            .take(OVERFLOW_SLOT)
            .enumerate()
            .map(|(index, (_numeric_id, id))| (CardId(id), index))
            .collect()
    })
}

fn slot_of(card: &CardId) -> usize {
    vocabulary().get(card).copied().unwrap_or(OVERFLOW_SLOT)
}

fn norm(value: f32, max: f32) -> f32 {
    if max <= 0.0 {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    }
}

fn phase_one_hot(phase: GamePhase) -> [f32; PHASE_COUNT] {
    let mut one_hot = [0.0; PHASE_COUNT];
    let index = match phase {
        GamePhase::Mulligan(_) => 0,
        GamePhase::StartOfTurn(_) => 1,
        GamePhase::Action(_) => 2,
        GamePhase::Discard { .. } => 3,
        GamePhase::GameOver(_) => 4,
    };
    one_hot[index] = 1.0;
    one_hot
}

/// Encodes `state` from `side`'s perspective into a fixed `OBS_SIZE`-length
/// vector, via `build_client_view` (so hidden zones are already correctly
/// Fog-of-War-collapsed to counts before anything here sees them).
pub fn encode_observation(state: &GameState, registry: &CardRegistry, side: Side) -> Vec<f32> {
    let view = build_client_view(state, registry, side);
    encode_view(&view)
}

/// Alias for `encode_observation`, named to match the `to_observation_vector`
/// shape a caller might expect. A free function, not a `GameState` method —
/// encoding needs a `CardRegistry` and `Side`, and keeping RL-specific
/// encoding out of `netrunner_core::GameState` itself is the whole point of
/// this module living here rather than in `netrunner_core` (see this
/// module's doc comment).
pub fn to_observation_vector(state: &GameState, registry: &CardRegistry, side: Side) -> Vec<f32> {
    encode_observation(state, registry, side)
}

fn encode_view(view: &ClientView) -> Vec<f32> {
    let (self_credits, self_clicks, self_agenda_points, opp_credits, opp_clicks, opp_agenda_points) = match view.side {
        Side::Corp => (
            view.corp.credits,
            view.corp.clicks,
            view.corp.agenda_points,
            view.runner.credits,
            view.runner.clicks,
            view.runner.agenda_points,
        ),
        Side::Runner => (
            view.runner.credits,
            view.runner.clicks,
            view.runner.agenda_points,
            view.corp.credits,
            view.corp.clicks,
            view.corp.agenda_points,
        ),
    };

    let installed_ice_count: usize = view.corp.servers.iter().map(|server| server.ice.len()).sum();
    let installed_root_count: usize = view.corp.servers.iter().map(|server| server.root.len()).sum();
    let remote_server_count = view.corp.servers.iter().filter(|server| matches!(server.server, ServerId::Remote(_))).count();

    let mut features = Vec::with_capacity(OBS_SIZE);
    features.push(if view.side == Side::Corp { 1.0 } else { 0.0 });
    features.extend(phase_one_hot(view.phase));
    features.push(norm(self_credits as f32, MAX_CREDITS));
    features.push(norm(self_clicks as f32, MAX_CLICKS));
    features.push(norm(self_agenda_points as f32, MAX_AGENDA_POINTS));
    features.push(norm(opp_credits as f32, MAX_CREDITS));
    features.push(norm(opp_clicks as f32, MAX_CLICKS));
    features.push(norm(opp_agenda_points as f32, MAX_AGENDA_POINTS));
    features.push(norm(view.corp.bad_publicity as f32, MAX_BAD_PUBLICITY));
    features.push(norm(view.runner.tags as f32, MAX_TAGS));
    features.push(norm(view.runner.brain_damage as f32, MAX_BRAIN_DAMAGE));
    features.push(norm(view.runner.memory_units as f32, MAX_MEMORY_UNITS));
    features.push(norm(view.runner.link_strength as f32, MAX_LINK_STRENGTH));
    features.push(norm(view.corp.hq_count as f32, MAX_HAND_SIZE));
    features.push(norm(view.corp.rd_count as f32, MAX_DECK_SIZE));
    features.push(norm(view.corp.archives.len() as f32, MAX_DECK_SIZE));
    features.push(norm(view.runner.grip_count as f32, MAX_HAND_SIZE));
    features.push(norm(view.runner.stack_count as f32, MAX_DECK_SIZE));
    features.push(norm(view.runner.heap.len() as f32, MAX_DECK_SIZE));
    features.push(norm(view.runner.rig.len() as f32, MAX_INSTALLED_PER_SIDE));
    features.push(norm(installed_ice_count as f32, MAX_INSTALLED_PER_SIDE));
    features.push(norm(installed_root_count as f32, MAX_INSTALLED_PER_SIDE));
    features.push(norm(remote_server_count as f32, MAX_REMOTE_SERVERS));
    match &view.active_run {
        Some(run) => {
            features.push(1.0);
            features.push(norm(run.position as f32, MAX_RUN_ICE));
        }
        None => {
            features.push(0.0);
            features.push(0.0);
        }
    }
    features.push(norm(view.legal_actions.len() as f32, ActionSpace::SIZE as f32));
    debug_assert_eq!(features.len(), SCALAR_COUNT);

    features.extend(encode_card_planes(view));

    debug_assert_eq!(features.len(), OBS_SIZE);
    features
}

/// Which plane a zone's cards belong in, given who is looking.
const OWN_HAND: usize = 0;
const OWN_INSTALLED: usize = 1;
const OPP_INSTALLED: usize = 2;
const OWN_DISCARD: usize = 3;
const OPP_DISCARD: usize = 4;

/// Counts each visible card id into its plane slot.
///
/// Every zone read here is already masked by `build_client_view`; the
/// `Option`s below are precisely where the engine withheld an identity, and
/// skipping a `None` is what keeps a hidden card out of the encoding.
fn encode_card_planes(view: &ClientView) -> Vec<f32> {
    let mut planes = vec![0.0f32; PLANE_COUNT * CARD_VOCAB];
    let mut count = |plane: usize, card: &CardId| {
        planes[plane * CARD_VOCAB + slot_of(card)] += 1.0;
    };

    // Hand: `Some` only for its owner, so at most one of these fires.
    let own_hand = match view.side {
        Side::Corp => view.corp.hq_cards.as_ref(),
        Side::Runner => view.runner.grip_cards.as_ref(),
    };
    for card in own_hand.into_iter().flatten() {
        count(OWN_HAND, card);
    }

    // Corp installed: `card` is `None` for an unrezzed card seen by the
    // Runner, so only rezzed ice/upgrades reach the Runner's planes.
    let corp_installed_plane = if view.side == Side::Corp { OWN_INSTALLED } else { OPP_INSTALLED };
    for server in &view.corp.servers {
        for installed in server.ice.iter().chain(server.root.iter()) {
            if let Some(card) = &installed.card {
                count(corp_installed_plane, card);
            }
        }
    }

    // The rig is public to both sides.
    let rig_plane = if view.side == Side::Runner { OWN_INSTALLED } else { OPP_INSTALLED };
    for installed in &view.runner.rig {
        count(rig_plane, &installed.card);
    }

    // Archives: a facedown card's identity is `None` for the Runner.
    let archives_plane = if view.side == Side::Corp { OWN_DISCARD } else { OPP_DISCARD };
    for archived in &view.corp.archives {
        if let Some(card) = &archived.card {
            count(archives_plane, card);
        }
    }

    let heap_plane = if view.side == Side::Runner { OWN_DISCARD } else { OPP_DISCARD };
    for card in &view.runner.heap {
        count(heap_plane, card);
    }

    for value in &mut planes {
        *value = norm(*value, MAX_COPIES_IN_ZONE);
    }
    planes
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::cards::CardRegistry;
    use netrunner_core::rules::GameState;

    #[test]
    fn observation_length_matches_obs_size_for_both_sides() {
        let registry = CardRegistry::new();
        let state = GameState::new(0);

        assert_eq!(encode_observation(&state, &registry, Side::Corp).len(), OBS_SIZE);
        assert_eq!(encode_observation(&state, &registry, Side::Runner).len(), OBS_SIZE);
    }

    #[test]
    fn all_features_are_finite_and_within_a_sane_range() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.corp.resources.credits = netrunner_core::rules::Credits(999);
        state.runner.tags = 999;

        for side in [Side::Corp, Side::Runner] {
            for value in encode_observation(&state, &registry, side) {
                assert!(value.is_finite());
                assert!((-1.0..=1.0).contains(&value), "feature {value} out of expected range");
            }
        }
    }

    #[test]
    fn side_indicator_is_the_first_feature_and_differs_by_viewer() {
        let registry = CardRegistry::new();
        let state = GameState::new(0);

        let corp_obs = encode_observation(&state, &registry, Side::Corp);
        let runner_obs = encode_observation(&state, &registry, Side::Runner);
        assert_eq!(corp_obs[0], 1.0);
        assert_eq!(runner_obs[0], 0.0);
    }

    /// The vocabulary's *order* is baked into every trained model: slot 7
    /// meaning a different card than it did at export time silently
    /// corrupts the policy rather than failing loudly. Adding a card from a
    /// later set must append, never shift. This pins the head of the
    /// ordering so a reordering shows up as a test failure.
    #[test]
    fn vocabulary_order_is_stable() {
        let vocabulary = vocabulary();

        // System Gateway is numbered from 30001, so its cards occupy the
        // low slots in `numeric_id` order.
        let mut by_slot: Vec<(usize, String)> =
            vocabulary.iter().map(|(id, slot)| (*slot, id.0.clone())).collect();
        by_slot.sort();

        // System Gateway numbers from 30001, so its cards take the low
        // slots in printed order: 30001 René, 30002 Wildcat Strike, ...
        let head: Vec<&str> = by_slot.iter().take(6).map(|(_, id)| id.as_str()).collect();
        assert_eq!(
            head,
            vec!["rene_loup_arcemont", "wildcat_strike", "carnivore", "botulus", "buzzsaw", "cleaver"]
        );

        // Cards with no `numeric_id` (the baseline pool) sort last, by id,
        // so a new set slots in ahead of them without disturbing 0..=n.
        let tail: Vec<&str> = by_slot.iter().rev().take(2).map(|(_, id)| id.as_str()).collect();
        assert_eq!(tail, vec!["weyland_consortium_building_a_better_world", "wall_of_static"]);

        assert!(vocabulary.len() <= OVERFLOW_SLOT, "vocabulary must leave the overflow slot free");
    }

    /// Every playable card must have its own slot — two cards sharing one
    /// would make them indistinguishable to the network.
    #[test]
    fn every_playable_card_has_a_distinct_slot() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);

        let cards: Vec<_> = registry.iter().collect();
        let slots: std::collections::HashSet<usize> = cards.iter().map(|card| slot_of(&card.id)).collect();

        assert_eq!(slots.len(), cards.len(), "every playable card needs its own vocabulary slot");
        assert!(!slots.contains(&OVERFLOW_SLOT), "no playable card should land in the overflow bucket");
    }

    #[test]
    fn an_unmapped_card_lands_in_the_overflow_bucket() {
        assert_eq!(slot_of(&CardId("definitely_not_a_real_card".to_string())), OVERFLOW_SLOT);
    }

    /// The planes must never encode a card the viewer cannot see. This is
    /// the fog-of-war guarantee: an unrezzed Corp card masks to
    /// `card: None`, and skipping those `None`s is what keeps its identity
    /// out of the Runner's observation.
    #[test]
    fn an_unrezzed_corp_card_is_invisible_in_the_runners_planes() {
        use netrunner_core::decks;
        use netrunner_core::rules::validate_deck;

        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);

        let corp = decks::by_id("discretion_advised").expect("sample deck exists");
        let runner = decks::by_id("stolen_goods").expect("sample deck exists");
        let (corp_deck, runner_deck) = (corp.to_deck(), runner.to_deck());
        assert_eq!(validate_deck(&corp_deck, Side::Corp, &registry), Ok(()));

        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 7).expect("setup");

        // The Corp's own hand is in its own planes...
        let corp_obs = encode_observation(&state, &registry, Side::Corp);
        let corp_hand_plane = &corp_obs[SCALAR_COUNT..SCALAR_COUNT + CARD_VOCAB];
        assert!(corp_hand_plane.iter().any(|value| *value > 0.0), "Corp should see its own opening hand");

        // ...but nothing of it reaches the Runner's view of Corp installs
        // or the Runner's own hand plane.
        let runner_obs = encode_observation(&state, &registry, Side::Runner);
        let opp_installed = &runner_obs
            [SCALAR_COUNT + OPP_INSTALLED * CARD_VOCAB..SCALAR_COUNT + (OPP_INSTALLED + 1) * CARD_VOCAB];
        assert!(
            opp_installed.iter().all(|value| *value == 0.0),
            "nothing is installed yet, so the Runner must see no Corp card identities"
        );
    }

    /// Two different hands of the same size must produce different
    /// observations — the exact property the scalar-only encoding lacked,
    /// and the reason a policy trained on it could not learn card choice.
    #[test]
    fn different_hands_produce_different_observations() {
        use netrunner_core::decks;

        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);

        let runner = decks::by_id("stolen_goods").expect("sample deck exists");
        let corp = decks::by_id("discretion_advised").expect("sample deck exists");
        let (corp_deck, runner_deck) = (corp.to_deck(), runner.to_deck());

        let (a, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 1).expect("setup");
        let (b, _) = GameState::setup(&corp_deck, &runner_deck, &registry, 2).expect("setup");

        let obs_a = encode_observation(&a, &registry, Side::Runner);
        let obs_b = encode_observation(&b, &registry, Side::Runner);
        assert_ne!(obs_a, obs_b, "two different opening hands must not encode identically");
    }
}
