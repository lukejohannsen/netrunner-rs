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
//! Seven blocks, in order: [`SCALAR_COUNT`] normalized scalars (resources,
//! zone counts, turn position), a per-install block for the Corp's board,
//! one for the Runner's rig, a breaker-coverage summary, the run in
//! progress, the decision that is parked, and [`PLANE_COUNT`] card-identity
//! planes of [`CARD_VOCAB`] slots each.
//!
//! **Why the board blocks exist (ROADMAP Phase 2 §5, item 17).** The scalar
//! half was once the whole encoding, which made every hand of the same size
//! identical to the network; the planes fixed that by counting card ids per
//! zone. But planes and scalars together still saw only *how many* installs
//! and *which* cards — not a single advancement token, which install was
//! rezzed, what server a run was against, the encountered ICE's strength or
//! how many subroutines were still pending, or what decision was parked.
//! `eval::evaluate_state_with` reads all of that, and on a fixed arena with
//! zero draws the value head alone scored the Corp at 0.469 against a 0.724
//! chair baseline while the Runner chair was neutral: the network could not
//! learn to close a scoring window it could not see. This layout gives it
//! what the evaluator reads.
//!
//! **The install blocks are indexed by `PublicInstalledCard::position` and
//! rig order** — the same indices the `ActionSpace`'s `AdvanceCard`,
//! `ScoreAgenda`, `RezIce`, `TrashResource` and `ActivateAbility` segments
//! use — so "advance slot 3" in the policy target and "slot 3 holds an
//! agenda at two of three tokens" in the observation are the same index in
//! both tensors. A per-server grouping was the alternative, and it would
//! have left the policy head to learn the mapping from server to slot on
//! its own.
//!
//! Everything is read from a `ClientView`, never a `GameState`, so
//! fog-of-war is enforced upstream: `hq_cards`/`grip_cards` are `None` for
//! the non-owner, an unrezzed Corp card masks to `card: None`, a facedown
//! archived card hides its identity from the Runner, and an unrezzed ICE on
//! a run has no `identity`. Printed values (`CardDefinition`) are looked up
//! only through a card id the view shows, so this module encodes only what
//! it is handed and cannot leak.

use std::collections::HashMap;
use std::sync::OnceLock;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardDefinition, CardId, CardType, Cost, IceType};
use netrunner_core::rules::{
    ActionSpace, GamePhase, GameState, InstallSlot, MaskedZone, PendingDecision, PendingPreventionKind, PublicAccessPhase,
    PublicAccessState, PublicInstalledCard, PublicInstalledRunnerCard, PublicRunIce, PublicRunState, RunPhase, ServerId,
    Side, SubroutineStatus,
};
use netrunner_core::view::{build_client_view, ClientView};

use crate::eval::covers;

// Normalization caps. Independently defined rather than imported from
// `netrunner_core::rules::action_mask`'s own `MAX_*` constants — that
// module is private, and only `ActionSpace`/`get_action_mask` are
// re-exported from `rules` — so these are this crate's own reasonable
// caps for scaling raw counts into a roughly-unit range, not a claim that
// they bound what's legal. The two exceptions are the slot counts below,
// which *must* match the action mask's segment widths.
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
const MAX_TURNS: f32 = 50.0;
const MAX_ACTIONS_PER_TURN: f32 = 5.0;
const MAX_RECURRING_CREDITS: f32 = 5.0;
const MAX_IDENTITY_COUNTERS: f32 = 5.0;
const MAX_AGENDA_COUNTERS: f32 = 5.0;
const MAX_SCORED_AGENDAS: f32 = 5.0;
const MAX_REMOVED_FROM_GAME: f32 = 10.0;
const MAX_ADVANCEMENT_TOKENS: f32 = 5.0;
const MAX_CARD_COUNTERS: f32 = 5.0;
const MAX_PRINTED_AGENDA_POINTS: f32 = 3.0;
const MAX_STRENGTH: f32 = 8.0;
const MAX_PRINTED_COST: f32 = 10.0;
const MAX_PROTECTING_ICE: f32 = 3.0;
const MAX_MEMORY_COST: f32 = 4.0;
const MAX_HOSTED_CARDS: f32 = 3.0;
const MAX_SUBROUTINES: f32 = 4.0;
const MAX_ACCESSED_CARDS: f32 = 8.0;
const MAX_SELECTION: f32 = 8.0;
const MAX_CHOICE_OPTIONS: f32 = 4.0;
const MAX_TRACE_STRENGTH: f32 = 10.0;
const MAX_PENDING_DAMAGE: f32 = 5.0;
const MAX_WINDOW_PASSES: f32 = 2.0;
const MAX_BAD_PUBLICITY_CREDITS: f32 = 5.0;
const MAX_BONUS_RUN_CREDITS: f32 = 10.0;

const PHASE_COUNT: usize = 5;
const RUN_PHASE_COUNT: usize = 6;
/// HQ, R&D, Archives, remote — the remote's *index* is a separate scalar.
const SERVER_KIND_COUNT: usize = 4;
/// Barrier, Code Gate, Sentry — `eval::covers`'s order.
const ICE_TYPE_COUNT: usize = 3;

/// Normalizer for a per-card count in one zone. Deck-building caps a card
/// at `MAX_COPIES_PER_CARD` (3), which also bounds how many can be in hand
/// or installed; a discard pile can exceed it, so counts are clamped rather
/// than assumed in range.
const MAX_COPIES_IN_ZONE: f32 = 3.0;

/// side(1) + phase one-hot(5) + self/opp credits+clicks+agenda_points(6) +
/// corp bad publicity(1) + runner tags/brain damage/memory/link(4) +
/// zone counts: hq/rd/archives/grip/stack/heap/rig/installed-ice/
/// installed-root/remote-servers(10) + active-run flag + normalized run
/// position(2) + normalized legal action count(1) + active player is
/// self(1) + turn(1) + actions this turn(1) + corp recurring credits and
/// pool(2) + corp identity counters(1) + both identities flipped(2) +
/// scored agenda counters(1) + runner scored agendas(1) + corp removed
/// from game(1) + servers run this turn HQ/R&D/Archives/any remote(4).
pub const SCALAR_COUNT: usize = 1 + PHASE_COUNT + 6 + 1 + 4 + 10 + 2 + 1 + 1 + 1 + 1 + 2 + 1 + 2 + 1 + 1 + 1 + 4;

/// Installs per side the board blocks can hold. **Must equal
/// `action_mask::MAX_INSTALLED_PER_SIDE`** — it is the width of the
/// `AdvanceCard`/`ScoreAgenda`/`RezIce`/`TrashResource` segments, and the
/// whole point of a per-slot block is that slot *n* here is slot *n*
/// there. A card past this position is dropped, exactly as the action
/// space cannot name it. Pinned by `install_slots_match_the_action_space`.
pub const MAX_INSTALL_SLOTS: usize = 32;

/// Per Corp install: present(1), rezzed(1), in the ICE slot(1), server
/// kind one-hot(4), remote index(1), advancement tokens(1), counters(1),
/// ICE protecting its server(1); then, only when the view shows the
/// card, its printed values: type one-hot agenda/asset/upgrade/ICE(4),
/// advancement requirement(1), agenda points(1), ICE subtype one-hot(3),
/// strength(1), rez/install cost(1), trash cost(1).
///
/// Tokens, rez state and slot are public on the physical card and stay
/// visible to the Runner on an unrezzed install; the printed block is what
/// `eval::corp_install_value` reads and is zero when the identity is masked.
pub const CORP_SLOT_LEN: usize = 1 + 1 + 1 + SERVER_KIND_COUNT + 1 + 1 + 1 + 1 + 4 + 1 + 1 + ICE_TYPE_COUNT + 1 + 1 + 1;
pub const CORP_BLOCK_LEN: usize = MAX_INSTALL_SLOTS * CORP_SLOT_LEN;

/// Per rig card: present(1), current strength(1), counters(1), hosted
/// on ICE(1), type one-hot program/hardware/resource(3), memory cost(1),
/// breaks Barrier/Code Gate/Sentry(3), hosted cards(1). The rig is
/// never masked, so the printed values are always readable.
pub const RIG_SLOT_LEN: usize = 1 + 1 + 1 + 1 + 3 + 1 + ICE_TYPE_COUNT + 1;
pub const RIG_BLOCK_LEN: usize = MAX_INSTALL_SLOTS * RIG_SLOT_LEN;

/// Per ICE subtype: whether any rig card breaks it, and the strongest such
/// card's current strength — `eval::breaker_coverage` and the input to its
/// `strength_shortfall`.
pub const COVERAGE_BLOCK_LEN: usize = ICE_TYPE_COUNT * 2;

/// ICE on one run the run block can hold. Netrunner servers rarely carry
/// more than four; eight leaves room without a fifth of the encoding going
/// to empty run slots.
pub const MAX_RUN_ICE_SLOTS: usize = 8;

/// Per run ICE: present(1), already passed(1), rezzed(1); then, only when
/// the view shows the ICE's identity (its owner, or rezzed): strength(1),
/// subtype one-hot(3), pending/broken/resolved subroutines(3). What
/// `eval::run_is_breakable` and `pending_subroutines` read.
pub const RUN_ICE_LEN: usize = 1 + 1 + 1 + 1 + ICE_TYPE_COUNT + 3;

/// active(1), target server kind one-hot(4), remote index(1), run
/// phase one-hot(6), position(1), ICE count(1), jack out permitted(1),
/// bad publicity credits(1), bonus run credits(1), cannot steal or
/// trash(1), redirect on approach(1); the ICE slots; then access:
/// accessing(1), unaccessed(1), resolved(1), access phase one-hot(3),
/// trash cost(1), mandatory steal(1), steal cost present(1), can pay(1),
/// interactive trigger cost(1).
pub const RUN_BLOCK_LEN: usize =
    1 + SERVER_KIND_COUNT + 1 + RUN_PHASE_COUNT + 1 + 1 + 1 + 1 + 1 + 1 + 1 + MAX_RUN_ICE_SLOTS * RUN_ICE_LEN + 1 + 1 + 1 + 3 + 1 + 1 + 1 + 1 + 1;

/// What is parked: flags for ChooseEffect/ChooseCards/ChooseServer/
/// ChooseTriggerOrder/paid choice/prevention/trace/paid-ability window(8)
/// — flags rather than a one-hot because a window and a prevention can be
/// open together; owed by self/opponent(2, by `rules::current_actor`'s
/// precedence); selection progress: selected over max, min, max(3);
/// effect options(1); paid-choice credit cost(1); trace base strength,
/// Corp bid, Corp bid placed(3); prevention amount and prevented(2);
/// window passes(1). The evaluator charges `unresolved_decision_weight`
/// for exactly this state and prices `ChooseCards` by its `min`.
pub const DECISION_BLOCK_LEN: usize = 8 + 2 + 3 + 1 + 1 + 3 + 2 + 1;

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

// Block offsets, in encoding order. Tests address features through these
// so a block growing cannot silently shift the assertions off the feature
// they meant.
const CORP_BLOCK_START: usize = SCALAR_COUNT;
const RIG_BLOCK_START: usize = CORP_BLOCK_START + CORP_BLOCK_LEN;
const COVERAGE_BLOCK_START: usize = RIG_BLOCK_START + RIG_BLOCK_LEN;
const RUN_BLOCK_START: usize = COVERAGE_BLOCK_START + COVERAGE_BLOCK_LEN;
const DECISION_BLOCK_START: usize = RUN_BLOCK_START + RUN_BLOCK_LEN;
const PLANES_START: usize = DECISION_BLOCK_START + DECISION_BLOCK_LEN;

pub const OBS_SIZE: usize = PLANES_START + PLANE_COUNT * CARD_VOCAB;

/// Vocabulary-slot rank for a card's set. **Deliberately not release
/// order** — it is the order sets were added to *this vocabulary*, which is
/// the thing that has to stay stable.
///
/// Sorting purely by `numeric_id` was the original rule, and it delivers
/// "a new set appends" only while every new set is numbered *above* the
/// current pool. That stopped being true the moment the Core Set's printed
/// metadata was backfilled: those cards were already in the vocabulary (at
/// the tail, as entries with no `numeric_id`), and their `01xxx` codes sort
/// below System Gateway's `30xxx` — which would have pushed all 75 System
/// Gateway cards 19 slots along, corrupting every exported policy for a
/// change that added no new card at all. Ranking the Core Set after the
/// sets already ordered keeps every System Gateway slot exactly where it
/// was.
///
/// A genuinely new set takes the next rank and appends, exactly as before.
///
/// **`elev` used to sit at rank 1, ahead of `core`, and that was the bug
/// this rule exists to prevent.** The rank was reserved for *Elevation*
/// before any of its cards existed, which reads as "append" only while the
/// Core Set is not already in the vocabulary — it was. Every Elevation card
/// that landed therefore *inserted* ahead of the 19 Core Set cards and
/// pushed them along (slots 75..=93 → 101..=119 by the third stage), in the
/// middle of the second 2,400-game training run, whose self-play was
/// recompiled mid-run by a loop that shelled out to `cargo run`
/// (ROADMAP Phase 2 §5). Ranks are assigned in the order sets *entered this
/// vocabulary* — System Gateway, then the backfilled Core Set, then
/// *Elevation* — which is the only ordering under which a later set cannot
/// disturb an earlier one's slots.
fn set_rank(set_code: Option<&str>) -> u32 {
    match set_code {
        Some("sg") => 0,
        Some("core") => 1,
        Some("elev") => 2,
        _ => 3,
    }
}

/// Maps a card id to its plane slot.
///
/// Built once from `cards::register_playable_cards` — the canonical
/// playable pool — rather than from whatever registry a caller passes, so
/// the mapping is identical for every consumer and stable across processes.
/// Ordered by `(set_rank, numeric_id, id)`: `set_rank` keeps whole sets
/// from interleaving, and NetrunnerDB numbers a set contiguously, so within
/// one set the printed order is stable. Cards with no `numeric_id`
/// (homebrew, test fixtures) sort last, by id.
fn vocabulary() -> &'static HashMap<CardId, usize> {
    static VOCABULARY: OnceLock<HashMap<CardId, usize>> = OnceLock::new();
    VOCABULARY.get_or_init(|| {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);

        let mut cards: Vec<(u32, u32, String)> = registry
            .iter()
            .map(|card| {
                (
                    set_rank(card.set_code.as_deref()),
                    card.numeric_id.map_or(u32::MAX, |numeric| numeric.0),
                    card.id.0.clone(),
                )
            })
            .collect();
        cards.sort();

        cards
            .into_iter()
            .take(OVERFLOW_SLOT)
            .enumerate()
            .map(|(index, (_rank, _numeric_id, id))| (CardId(id), index))
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

fn flag(value: bool) -> f32 {
    if value {
        1.0
    } else {
        0.0
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

fn run_phase_one_hot(phase: RunPhase) -> [f32; RUN_PHASE_COUNT] {
    let mut one_hot = [0.0; RUN_PHASE_COUNT];
    let index = match phase {
        RunPhase::Initiation => 0,
        RunPhase::ApproachIce => 1,
        RunPhase::EncounterIce => 2,
        RunPhase::AccessingCard => 3,
        RunPhase::Success => 4,
        RunPhase::Ended => 5,
    };
    one_hot[index] = 1.0;
    one_hot
}

/// A server as kind one-hot (HQ, R&D, Archives, remote) plus the remote's
/// normalized index — two features rather than one-hot over every remote,
/// so the encoding does not grow with `MAX_REMOTE_SERVERS`.
fn server_features(server: ServerId) -> ([f32; SERVER_KIND_COUNT], f32) {
    let mut one_hot = [0.0; SERVER_KIND_COUNT];
    let remote_index = match server {
        ServerId::Hq => {
            one_hot[0] = 1.0;
            0.0
        }
        ServerId::RnD => {
            one_hot[1] = 1.0;
            0.0
        }
        ServerId::Archives => {
            one_hot[2] = 1.0;
            0.0
        }
        ServerId::Remote(index) => {
            one_hot[3] = 1.0;
            norm(index as f32, MAX_REMOTE_SERVERS)
        }
    };
    (one_hot, remote_index)
}

fn ice_type_one_hot(ice_type: IceType) -> [f32; ICE_TYPE_COUNT] {
    let mut one_hot = [0.0; ICE_TYPE_COUNT];
    let index = match ice_type {
        IceType::Barrier => 0,
        IceType::CodeGate => 1,
        IceType::Sentry => 2,
    };
    one_hot[index] = 1.0;
    one_hot
}

fn zone_len(zone: &MaskedZone) -> usize {
    match zone {
        MaskedZone::Visible(cards) => cards.len(),
        MaskedZone::Hidden { count } => *count as usize,
    }
}

fn credit_cost(cost: &Cost) -> f32 {
    match cost {
        Cost::Credits(credits) => norm(*credits as f32, MAX_PRINTED_COST),
        _ => 0.0,
    }
}

/// Encodes `state` from `side`'s perspective into a fixed `OBS_SIZE`-length
/// vector, via `build_client_view` (so hidden zones are already correctly
/// Fog-of-War-collapsed to counts before anything here sees them).
pub fn encode_observation(state: &GameState, registry: &CardRegistry, side: Side) -> Vec<f32> {
    let view = build_client_view(state, registry, side);
    encode_view(&view, registry)
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

/// The registry is consulted only through card ids the view carries — see
/// the module doc comment — so a `ClientView` for either seat encodes
/// without a leak.
fn encode_view(view: &ClientView, registry: &CardRegistry) -> Vec<f32> {
    let mut features = Vec::with_capacity(OBS_SIZE);
    encode_scalars(view, &mut features);
    debug_assert_eq!(features.len(), CORP_BLOCK_START);
    encode_corp_board(view, registry, &mut features);
    debug_assert_eq!(features.len(), RIG_BLOCK_START);
    encode_rig(view, registry, &mut features);
    debug_assert_eq!(features.len(), COVERAGE_BLOCK_START);
    encode_coverage(view, registry, &mut features);
    debug_assert_eq!(features.len(), RUN_BLOCK_START);
    encode_run(view, &mut features);
    debug_assert_eq!(features.len(), DECISION_BLOCK_START);
    encode_decision(view, &mut features);
    debug_assert_eq!(features.len(), PLANES_START);
    features.extend(encode_card_planes(view));
    debug_assert_eq!(features.len(), OBS_SIZE);
    features
}

fn encode_scalars(view: &ClientView, features: &mut Vec<f32>) {
    // Observations are per seat: the encoding is "self" against "opponent",
    // which a spectator's view has no way to orient.
    let side = view.viewer.side().expect("observations are encoded from a player's view, never a spectator's");
    let (self_credits, self_clicks, self_agenda_points, opp_credits, opp_clicks, opp_agenda_points) = match side {
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

    features.push(flag(side == Side::Corp));
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

    features.push(flag(view.active_player == side));
    features.push(norm(view.turn as f32, MAX_TURNS));
    features.push(norm(view.actions_taken_this_turn as f32, MAX_ACTIONS_PER_TURN));
    features.push(norm(view.corp.recurring_credits as f32, MAX_RECURRING_CREDITS));
    features.push(norm(view.corp.recurring_credits_max as f32, MAX_RECURRING_CREDITS));
    features.push(norm(view.corp.identity_counters as f32, MAX_IDENTITY_COUNTERS));
    features.push(flag(view.corp.identity_flipped));
    features.push(flag(view.runner.identity_flipped));
    let agenda_counters: u32 = view.corp.scored_agendas.iter().map(|scored| scored.agenda_counters).sum();
    features.push(norm(agenda_counters as f32, MAX_AGENDA_COUNTERS));
    features.push(norm(view.runner.scored_agendas.len() as f32, MAX_SCORED_AGENDAS));
    features.push(norm(view.corp.removed_from_game.len() as f32, MAX_REMOVED_FROM_GAME));
    let ran = |wanted: fn(&ServerId) -> bool| flag(view.runner.servers_run_this_turn.iter().any(wanted));
    features.push(ran(|server| *server == ServerId::Hq));
    features.push(ran(|server| *server == ServerId::RnD));
    features.push(ran(|server| *server == ServerId::Archives));
    features.push(ran(|server| matches!(server, ServerId::Remote(_))));
    debug_assert_eq!(features.len(), SCALAR_COUNT);
}

/// One slot per `PublicInstalledCard::position`, so the block lines up with
/// the `ActionSpace`'s installed-card segments (see the module doc).
fn encode_corp_board(view: &ClientView, registry: &CardRegistry, features: &mut Vec<f32>) {
    let mut by_position: Vec<Option<(&PublicInstalledCard, usize)>> = vec![None; MAX_INSTALL_SLOTS];
    for server in &view.corp.servers {
        let protecting_ice = server.ice.len();
        for installed in server.ice.iter().chain(server.root.iter()) {
            if let Some(slot) = by_position.get_mut(installed.position) {
                *slot = Some((installed, protecting_ice));
            }
        }
    }
    for slot in by_position {
        let start = features.len();
        match slot {
            None => features.extend(std::iter::repeat_n(0.0, CORP_SLOT_LEN)),
            Some((installed, protecting_ice)) => {
                let (server_kind, remote_index) = server_features(installed.server);
                features.push(1.0);
                features.push(flag(installed.rezzed));
                features.push(flag(installed.slot == InstallSlot::Ice));
                features.extend(server_kind);
                features.push(remote_index);
                features.push(norm(installed.advancement_tokens as f32, MAX_ADVANCEMENT_TOKENS));
                features.push(norm(installed.counters.unwrap_or(0) as f32, MAX_CARD_COUNTERS));
                features.push(norm(protecting_ice as f32, MAX_PROTECTING_ICE));
                // `card` is `None` exactly when the mask withheld the
                // identity, and every printed value below follows it.
                let def = installed.card.as_ref().and_then(|card| registry.get(card));
                encode_corp_printed(def, features);
            }
        }
        debug_assert_eq!(features.len() - start, CORP_SLOT_LEN);
    }
}

/// The printed half of a Corp slot — zero for a card the viewer cannot
/// identify, or one the registry does not know.
fn encode_corp_printed(def: Option<&CardDefinition>, features: &mut Vec<f32>) {
    let Some(def) = def else {
        features.extend(std::iter::repeat_n(0.0, 4 + 1 + 1 + ICE_TYPE_COUNT + 1 + 1 + 1));
        return;
    };
    features.push(flag(def.card_type == CardType::Agenda));
    features.push(flag(def.card_type == CardType::Asset));
    features.push(flag(def.card_type == CardType::Upgrade));
    features.push(flag(matches!(def.card_type, CardType::Ice(_))));
    features.push(norm(def.advancement_requirement.unwrap_or(0) as f32, MAX_ADVANCEMENT_TOKENS));
    features.push(norm(def.agenda_points.unwrap_or(0) as f32, MAX_PRINTED_AGENDA_POINTS));
    features.extend(match def.card_type {
        CardType::Ice(ice_type) => ice_type_one_hot(ice_type),
        _ => [0.0; ICE_TYPE_COUNT],
    });
    features.push(norm(def.strength.unwrap_or(0) as f32, MAX_STRENGTH));
    features.push(norm(def.cost as f32, MAX_PRINTED_COST));
    features.push(norm(def.trash_cost.unwrap_or(0) as f32, MAX_PRINTED_COST));
}

/// One slot per rig position — the order `TrashResource` and the Runner's
/// `ActivateAbility` segment index by. Nothing in the rig is masked.
fn encode_rig(view: &ClientView, registry: &CardRegistry, features: &mut Vec<f32>) {
    for slot in 0..MAX_INSTALL_SLOTS {
        let start = features.len();
        match view.runner.rig.get(slot) {
            None => features.extend(std::iter::repeat_n(0.0, RIG_SLOT_LEN)),
            Some(installed) => encode_rig_card(installed, registry.get(&installed.card), features),
        }
        debug_assert_eq!(features.len() - start, RIG_SLOT_LEN);
    }
}

fn encode_rig_card(installed: &PublicInstalledRunnerCard, def: Option<&CardDefinition>, features: &mut Vec<f32>) {
    features.push(1.0);
    features.push(norm(installed.current_strength as f32, MAX_STRENGTH));
    features.push(norm(installed.counters as f32, MAX_CARD_COUNTERS));
    features.push(flag(installed.hosted_on_ice.is_some()));
    features.push(flag(def.is_some_and(|d| d.card_type == CardType::Program)));
    features.push(flag(def.is_some_and(|d| d.card_type == CardType::Hardware)));
    features.push(flag(def.is_some_and(|d| d.card_type == CardType::Resource)));
    features.push(norm(def.and_then(|d| d.memory_cost).unwrap_or(0) as f32, MAX_MEMORY_COST));
    features.extend(def.map_or([false; ICE_TYPE_COUNT], covers).map(flag));
    features.push(norm(installed.hosted_cards.len() as f32, MAX_HOSTED_CARDS));
}

/// Per ICE subtype, whether the rig breaks it and how strong its best
/// breaker for that subtype currently is.
fn encode_coverage(view: &ClientView, registry: &CardRegistry, features: &mut Vec<f32>) {
    let mut covered = [false; ICE_TYPE_COUNT];
    let mut strongest = [i32::MIN; ICE_TYPE_COUNT];
    for installed in &view.runner.rig {
        let Some(def) = registry.get(&installed.card) else { continue };
        for (subtype, breaks) in covers(def).into_iter().enumerate() {
            if breaks {
                covered[subtype] = true;
                strongest[subtype] = strongest[subtype].max(installed.current_strength);
            }
        }
    }
    for subtype in 0..ICE_TYPE_COUNT {
        features.push(flag(covered[subtype]));
        features.push(if covered[subtype] { norm(strongest[subtype] as f32, MAX_STRENGTH) } else { 0.0 });
    }
}

fn encode_run(view: &ClientView, features: &mut Vec<f32>) {
    let Some(run) = &view.active_run else {
        features.extend(std::iter::repeat_n(0.0, RUN_BLOCK_LEN));
        return;
    };
    let (server_kind, remote_index) = server_features(run.server);
    features.push(1.0);
    features.extend(server_kind);
    features.push(remote_index);
    features.extend(run_phase_one_hot(run.phase));
    features.push(norm(run.position as f32, MAX_RUN_ICE_SLOTS as f32));
    features.push(norm(run.ice.len() as f32, MAX_RUN_ICE_SLOTS as f32));
    features.push(flag(run.jack_out_permitted));
    features.push(norm(run.bad_publicity_credits as f32, MAX_BAD_PUBLICITY_CREDITS));
    features.push(norm(run.bonus_run_credits as f32, MAX_BONUS_RUN_CREDITS));
    features.push(flag(run.runner_cannot_steal_or_trash));
    features.push(flag(run.redirect_on_approach.is_some()));
    for slot in 0..MAX_RUN_ICE_SLOTS {
        let start = features.len();
        match run.ice.get(slot) {
            None => features.extend(std::iter::repeat_n(0.0, RUN_ICE_LEN)),
            Some(ice) => encode_run_ice(ice, slot < run.position, features),
        }
        debug_assert_eq!(features.len() - start, RUN_ICE_LEN);
    }
    encode_access(run, features);
}

fn encode_run_ice(ice: &PublicRunIce, passed: bool, features: &mut Vec<f32>) {
    features.push(1.0);
    features.push(flag(passed));
    features.push(flag(ice.rezzed));
    // `identity` is `None` for an unrezzed ICE seen by the Runner — its
    // strength, subtype and subroutines are all on the hidden face.
    let Some(identity) = &ice.identity else {
        features.extend(std::iter::repeat_n(0.0, 1 + ICE_TYPE_COUNT + 3));
        return;
    };
    features.push(norm(identity.current_strength as f32, MAX_STRENGTH));
    features.extend(ice_type_one_hot(identity.ice_type));
    for status in [SubroutineStatus::Pending, SubroutineStatus::Broken, SubroutineStatus::Resolved] {
        let count = identity.subroutines.iter().filter(|subroutine| subroutine.status == status).count();
        features.push(norm(count as f32, MAX_SUBROUTINES));
    }
}

fn encode_access(run: &PublicRunState, features: &mut Vec<f32>) {
    let Some(access) = &run.access_state else {
        features.extend(std::iter::repeat_n(0.0, 1 + 1 + 1 + 3 + 1 + 1 + 1 + 1 + 1));
        return;
    };
    let PublicAccessState { unaccessed_cards, resolved_cards, phase, .. } = access;
    features.push(1.0);
    features.push(norm(zone_len(unaccessed_cards) as f32, MAX_ACCESSED_CARDS));
    features.push(norm(zone_len(resolved_cards) as f32, MAX_ACCESSED_CARDS));
    match phase {
        PublicAccessPhase::SelectNextCard { .. } => {
            features.extend([1.0, 0.0, 0.0]);
            features.extend([0.0; 5]);
        }
        PublicAccessPhase::PendingInteractiveTrigger { cost, can_pay, .. } => {
            features.extend([0.0, 1.0, 0.0]);
            features.extend([0.0, 0.0, 0.0, flag(*can_pay), credit_cost(cost)]);
        }
        PublicAccessPhase::PendingChoice { trash_cost, mandatory_steal, steal_cost, .. } => {
            features.extend([0.0, 0.0, 1.0]);
            features.push(norm(trash_cost.unwrap_or(0) as f32, MAX_PRINTED_COST));
            features.push(flag(*mandatory_steal));
            features.push(flag(steal_cost.is_some()));
            features.extend([0.0, 0.0]);
        }
    }
}

/// Which parked state the view is waiting on, and whose move that makes
/// it. "Owed by" follows `rules::current_actor`'s precedence exactly —
/// trace, paid choice, decision, window, access trigger — because that is
/// the order `engine::apply_action`'s blocking guards apply, and the side
/// it names is the one whose legal actions are non-empty.
fn encode_decision(view: &ClientView, features: &mut Vec<f32>) {
    let side = view.viewer.side().expect("observations are encoded from a player's view, never a spectator's");
    let decision = view.pending_decision.as_ref();
    features.push(flag(matches!(decision, Some(PendingDecision::ChooseEffect { .. }))));
    features.push(flag(matches!(decision, Some(PendingDecision::ChooseCards { .. }))));
    features.push(flag(matches!(decision, Some(PendingDecision::ChooseServer { .. }))));
    features.push(flag(matches!(decision, Some(PendingDecision::ChooseTriggerOrder { .. }))));
    features.push(flag(view.pending_paid_choice.is_some()));
    features.push(flag(view.pending_prevention.is_some()));
    features.push(flag(view.active_trace.is_some()));
    features.push(flag(view.paid_ability_window.is_some()));

    let chooser = decision.map(|decision| match decision {
        PendingDecision::ChooseEffect { chooser, .. }
        | PendingDecision::ChooseTriggerOrder { chooser, .. }
        | PendingDecision::ChooseServer { chooser, .. } => *chooser,
        PendingDecision::ChooseCards { side, .. } => *side,
    });
    let access_decider = view.active_run.as_ref().and_then(|run| match &run.access_state.as_ref()?.phase {
        PublicAccessPhase::PendingInteractiveTrigger { decider, .. } => Some(*decider),
        _ => None,
    });
    let owed_by = view
        .active_trace
        .as_ref()
        .map(|trace| if trace.corp_bid.is_none() { Side::Corp } else { Side::Runner })
        .or(view.pending_paid_choice.as_ref().map(|choice| choice.side))
        .or(chooser)
        .or(view.paid_ability_window.as_ref().map(|window| window.active_priority))
        .or(access_decider);
    features.push(flag(owed_by == Some(side)));
    features.push(flag(owed_by == Some(side.other())));

    match decision {
        Some(PendingDecision::ChooseCards { selected, min, max, .. }) => {
            features.push(norm(selected.len() as f32, (*max).max(1) as f32));
            features.push(norm(*min as f32, MAX_SELECTION));
            features.push(norm(*max as f32, MAX_SELECTION));
        }
        _ => features.extend([0.0; 3]),
    }
    features.push(match decision {
        Some(PendingDecision::ChooseEffect { options, .. }) => norm(options.len() as f32, MAX_CHOICE_OPTIONS),
        _ => 0.0,
    });
    features.push(view.pending_paid_choice.as_ref().map_or(0.0, |choice| credit_cost(&choice.cost)));
    match &view.active_trace {
        Some(trace) => {
            features.push(norm(trace.base_strength as f32, MAX_TRACE_STRENGTH));
            features.push(norm(trace.corp_bid.unwrap_or(0) as f32, MAX_TRACE_STRENGTH));
            features.push(flag(trace.corp_bid.is_some()));
        }
        None => features.extend([0.0; 3]),
    }
    match view.pending_prevention.as_ref().map(|prevention| &prevention.kind) {
        Some(PendingPreventionKind::Damage { amount, prevented, .. }) => {
            features.push(norm(*amount as f32, MAX_PENDING_DAMAGE));
            features.push(norm(*prevented as f32, MAX_PENDING_DAMAGE));
        }
        Some(PendingPreventionKind::Trash { prevented, .. }) => {
            features.push(0.0);
            features.push(flag(*prevented));
        }
        None => features.extend([0.0; 2]),
    }
    features.push(view.paid_ability_window.as_ref().map_or(0.0, |window| norm(window.consecutive_passes as f32, MAX_WINDOW_PASSES)));
    debug_assert_eq!(features.len(), PLANES_START);
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
    let side = view.viewer.side().expect("observations are encoded from a player's view, never a spectator's");
    let mut planes = vec![0.0f32; PLANE_COUNT * CARD_VOCAB];
    let mut count = |plane: usize, card: &CardId| {
        planes[plane * CARD_VOCAB + slot_of(card)] += 1.0;
    };

    // Hand: `Some` only for its owner, so at most one of these fires.
    let own_hand = match side {
        Side::Corp => view.corp.hq_cards.as_ref(),
        Side::Runner => view.runner.grip_cards.as_ref(),
    };
    for card in own_hand.into_iter().flatten() {
        count(OWN_HAND, card);
    }

    // Corp installed: `card` is `None` for an unrezzed card seen by the
    // Runner, so only rezzed ice/upgrades reach the Runner's planes.
    let corp_installed_plane = if side == Side::Corp { OWN_INSTALLED } else { OPP_INSTALLED };
    for server in &view.corp.servers {
        for installed in server.ice.iter().chain(server.root.iter()) {
            if let Some(card) = &installed.card {
                count(corp_installed_plane, card);
            }
        }
    }

    // The rig is public to both sides.
    let rig_plane = if side == Side::Runner { OWN_INSTALLED } else { OPP_INSTALLED };
    for installed in &view.runner.rig {
        count(rig_plane, &installed.card);
    }

    // Archives: a facedown card's identity is `None` for the Runner.
    let archives_plane = if side == Side::Corp { OWN_DISCARD } else { OPP_DISCARD };
    for archived in &view.corp.archives {
        if let Some(card) = &archived.card {
            count(archives_plane, card);
        }
    }

    let heap_plane = if side == Side::Runner { OWN_DISCARD } else { OPP_DISCARD };
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
    use netrunner_core::dsl::{CardFilter, CardZoneRef, Effect, SubroutineDef};
    use netrunner_core::rules::{
        EncounteredSubroutine, GameState, InstallId, InstalledCard, PendingChoiceResume, RunIce, RunState,
    };

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

    /// The install blocks are only useful if slot *n* here is slot *n* in
    /// the `ActionSpace`'s installed-card segments. The action mask's cap
    /// is private, so this pins it through the one thing that is public:
    /// `index_of` resolves `RezIce`'s install handle to the card's
    /// position in `corp.installed`, bounded by that cap. On a board of
    /// `MAX_INSTALL_SLOTS + 1` installs, the last one inside the cap must
    /// land exactly `MAX_INSTALL_SLOTS - 1` slots after the first, and the
    /// one past it must have no slot at all — the card the board block
    /// drops.
    #[test]
    fn install_slots_match_the_action_space() {
        use netrunner_core::rules::PlayerAction;
        let mut state = GameState::new(0);
        state.corp.installed = (0..=MAX_INSTALL_SLOTS as u32)
            .map(|id| installed("wall", id, ServerId::Hq, InstallSlot::Ice))
            .collect();
        let index = |id: u32| ActionSpace::index_of(&state, &PlayerAction::RezIce { ice: InstallId(id) });
        let (Some(first), Some(last)) = (index(0), index(MAX_INSTALL_SLOTS as u32 - 1)) else {
            panic!("positions inside the cap have a slot")
        };
        assert_eq!(last - first, MAX_INSTALL_SLOTS - 1);
        assert_eq!(index(MAX_INSTALL_SLOTS as u32), None, "the action space cannot name a position the board block drops");
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

        // The Core Set sorts *after* System Gateway despite its far lower
        // card numbers — the whole point of `set_rank`. Backfilling its
        // printed metadata reordered those 19 baseline cards among
        // themselves (printed order now rather than alphabetical) and moved
        // no System Gateway card at all, which is the property deciding
        // whether an exported policy survives a card-data change.
        //
        // **Pinned by slot, not just by id.** Asserting the tail's *ids*
        // was what let *Elevation* reindex the Core Set unnoticed: its
        // cards ranked ahead of `core`, so all 19 moved along together
        // while `enigma` and `wall_of_static` stayed the last two of them
        // and this test stayed green. 77..=95 is where the Core Set sits
        // with System Gateway's 77 cards below it — the slots the second
        // volume run's first seven iterations were recorded against.
        assert_eq!(slot_of(&CardId("enigma".to_string())), 94);
        assert_eq!(slot_of(&CardId("wall_of_static".to_string())), 95);

        assert!(vocabulary.len() <= OVERFLOW_SLOT, "vocabulary must leave the overflow slot free");
    }

    /// The rule `set_rank` exists to enforce, stated over the whole
    /// vocabulary rather than at its ends: a set that joins later takes
    /// higher slots than every set already in it, so adding a set appends
    /// and no recorded observation reindexes underneath a model.
    ///
    /// This is the assertion that would have failed the moment *Elevation*
    /// Stage 1 landed, instead of a training run finding out nine
    /// iterations later (ROADMAP Phase 2 §5).
    #[test]
    fn a_later_set_takes_higher_slots_than_every_earlier_one() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);

        let mut highest = std::collections::HashMap::new();
        let mut lowest = std::collections::HashMap::new();
        for card in registry.iter() {
            let slot = slot_of(&card.id);
            let set = card.set_code.clone().unwrap_or_else(|| "none".to_string());
            highest.entry(set.clone()).and_modify(|top| *top = slot.max(*top)).or_insert(slot);
            lowest.entry(set).and_modify(|bottom| *bottom = slot.min(*bottom)).or_insert(slot);
        }

        // In vocabulary order: System Gateway, the backfilled Core Set,
        // then Elevation. A new set adds a pair here and nothing else.
        for (earlier, later) in [("sg", "core"), ("core", "elev")] {
            let (Some(top), Some(bottom)) = (highest.get(earlier), lowest.get(later)) else {
                panic!("both {earlier} and {later} should be in the playable pool");
            };
            assert!(top < bottom, "{later} must append after {earlier}: {earlier} ends at {top}, {later} starts at {bottom}");
        }
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
        let corp_hand_plane = &corp_obs[PLANES_START..PLANES_START + CARD_VOCAB];
        assert!(corp_hand_plane.iter().any(|value| *value > 0.0), "Corp should see its own opening hand");

        // ...but nothing of it reaches the Runner's view of Corp installs
        // or the Runner's own hand plane.
        let runner_obs = encode_observation(&state, &registry, Side::Runner);
        let opp_installed =
            &runner_obs[PLANES_START + OPP_INSTALLED * CARD_VOCAB..PLANES_START + (OPP_INSTALLED + 1) * CARD_VOCAB];
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

    // ---- the board, run and decision blocks ----

    fn agenda(id: &str, required: u32) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            side: Side::Corp,
            card_type: CardType::Agenda,
            advancement_requirement: Some(required),
            agenda_points: Some(2),
            ..CardDefinition::default()
        }
    }

    fn barrier(id: &str, strength: i32) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::Barrier),
            strength: Some(strength),
            cost: 3,
            ..CardDefinition::default()
        }
    }

    fn installed(card: &str, install_id: u32, server: ServerId, slot: InstallSlot) -> InstalledCard {
        InstalledCard { card: CardId(card.to_string()), install_id: InstallId(install_id), server, slot, ..Default::default() }
    }

    fn corp_slot(obs: &[f32], position: usize) -> &[f32] {
        let start = CORP_BLOCK_START + position * CORP_SLOT_LEN;
        &obs[start..start + CORP_SLOT_LEN]
    }

    // Feature offsets inside one Corp slot, named so the assertions read.
    const SLOT_PRESENT: usize = 0;
    const SLOT_REZZED: usize = 1;
    const SLOT_IS_ICE: usize = 2;
    const SLOT_ADVANCEMENT: usize = 1 + 1 + 1 + SERVER_KIND_COUNT + 1;
    const SLOT_PROTECTING_ICE: usize = SLOT_ADVANCEMENT + 2;
    const SLOT_PRINTED: usize = SLOT_PROTECTING_ICE + 1;
    const SLOT_IS_AGENDA: usize = SLOT_PRINTED;
    const SLOT_REQUIREMENT: usize = SLOT_PRINTED + 4;

    /// Advancing one card changes the observation only inside that card's
    /// own slot — the `ActionSpace` alignment the block exists for.
    #[test]
    fn advancing_an_install_changes_exactly_its_own_slot() {
        let registry = CardRegistry::from_cards(vec![agenda("agenda", 3), barrier("wall", 4)]);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.installed = vec![
            installed("wall", 1, ServerId::Remote(0), InstallSlot::Ice),
            installed("agenda", 2, ServerId::Remote(0), InstallSlot::Root),
        ];
        let before = encode_observation(&state, &registry, Side::Corp);
        state.corp.installed[1].advancement_tokens = 2;
        let after = encode_observation(&state, &registry, Side::Corp);

        let changed: Vec<usize> = (0..OBS_SIZE).filter(|i| before[*i] != after[*i]).collect();
        let slot_1 = CORP_BLOCK_START + CORP_SLOT_LEN..CORP_BLOCK_START + 2 * CORP_SLOT_LEN;
        assert!(!changed.is_empty(), "two tokens must be visible");
        assert!(
            changed.iter().all(|i| slot_1.contains(i)),
            "only the advanced card's slot may move; changed {changed:?}, slot 1 is {slot_1:?}"
        );
        let slot = corp_slot(&after, 1);
        assert_eq!(slot[SLOT_ADVANCEMENT], 2.0 / MAX_ADVANCEMENT_TOKENS);
        assert_eq!(slot[SLOT_IS_AGENDA], 1.0);
        assert_eq!(slot[SLOT_REQUIREMENT], 3.0 / MAX_ADVANCEMENT_TOKENS);
        assert_eq!(slot[SLOT_PROTECTING_ICE], 1.0 / MAX_PROTECTING_ICE, "one ICE protects the agenda's server");
        assert_eq!(corp_slot(&after, 0)[SLOT_IS_ICE], 1.0);
        assert_eq!(corp_slot(&after, 2)[SLOT_PRESENT], 0.0, "nothing sits at position 2");
    }

    /// What the Runner sees of a face-down install is what is on the
    /// table — that it exists, where, whether it is rezzed, how many
    /// tokens it carries — and none of what is printed on the hidden face.
    #[test]
    fn an_unrezzed_install_shows_the_runner_its_tokens_but_not_its_printed_face() {
        let registry = CardRegistry::from_cards(vec![agenda("agenda", 3)]);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.installed = vec![installed("agenda", 1, ServerId::Remote(0), InstallSlot::Root)];
        state.corp.installed[0].advancement_tokens = 2;

        let runner = corp_slot(&encode_observation(&state, &registry, Side::Runner), 0).to_vec();
        assert_eq!(runner[SLOT_PRESENT], 1.0);
        assert_eq!(runner[SLOT_REZZED], 0.0);
        assert_eq!(runner[SLOT_ADVANCEMENT], 2.0 / MAX_ADVANCEMENT_TOKENS);
        assert!(runner[SLOT_PRINTED..].iter().all(|v| *v == 0.0), "the printed half is hidden: {runner:?}");

        let corp = corp_slot(&encode_observation(&state, &registry, Side::Corp), 0).to_vec();
        assert_eq!(corp[SLOT_IS_AGENDA], 1.0, "the owner reads its own card");

        state.corp.installed[0].rezzed = true;
        let rezzed = corp_slot(&encode_observation(&state, &registry, Side::Runner), 0).to_vec();
        assert_eq!(rezzed[SLOT_REZZED], 1.0);
        assert_eq!(rezzed[SLOT_IS_AGENDA], 1.0, "rezzing turns the face up for the Runner too");
    }

    fn run_ice(strength: i32, subroutines: usize, rezzed: bool) -> RunIce {
        RunIce {
            install_id: InstallId(1),
            card_id: CardId("wall".to_string()),
            current_strength: strength,
            ice_type: IceType::Barrier,
            subroutines: (0..subroutines)
                .map(|id| EncounteredSubroutine {
                    id,
                    definition: SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
                    status: SubroutineStatus::Pending,
                })
                .collect(),
            rezzed,
        }
    }

    // Offsets inside the run block.
    const RUN_ACTIVE: usize = 0;
    const RUN_SERVER_REMOTE: usize = 1 + 3;
    const RUN_REMOTE_INDEX: usize = 1 + SERVER_KIND_COUNT;
    const RUN_PHASE_ENCOUNTER: usize = RUN_REMOTE_INDEX + 1 + 2;
    const RUN_ICE_START: usize = 1 + SERVER_KIND_COUNT + 1 + RUN_PHASE_COUNT + 1 + 1 + 1 + 1 + 1 + 1 + 1;
    const ICE_REZZED: usize = 2;
    const ICE_STRENGTH: usize = 3;
    const ICE_PENDING: usize = 3 + 1 + ICE_TYPE_COUNT;

    /// The run block names the target and, for a rezzed encounter, the
    /// ICE's strength and how many subroutines are still pending — the
    /// inputs to `eval`'s run terms.
    #[test]
    fn a_run_encodes_its_target_and_the_encountered_ice() {
        let registry = CardRegistry::from_cards(vec![barrier("wall", 4)]);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.installed = vec![installed("wall", 1, ServerId::Remote(2), InstallSlot::Ice)];
        state.corp.installed[0].rezzed = true;
        state.active_run = Some(RunState {
            server: ServerId::Remote(2),
            phase: RunPhase::EncounterIce,
            ice: vec![run_ice(4, 2, true)],
            position: 0,
            ..Default::default()
        });

        for side in [Side::Runner, Side::Corp] {
            let obs = encode_observation(&state, &registry, side);
            let run = &obs[RUN_BLOCK_START..RUN_BLOCK_START + RUN_BLOCK_LEN];
            assert_eq!(run[RUN_ACTIVE], 1.0);
            assert_eq!(run[RUN_SERVER_REMOTE], 1.0);
            assert_eq!(run[RUN_REMOTE_INDEX], 2.0 / MAX_REMOTE_SERVERS);
            assert_eq!(run[RUN_PHASE_ENCOUNTER], 1.0);
            let ice = &run[RUN_ICE_START..RUN_ICE_START + RUN_ICE_LEN];
            assert_eq!(ice[ICE_REZZED], 1.0);
            assert_eq!(ice[ICE_STRENGTH], 4.0 / MAX_STRENGTH, "{side:?} sees a rezzed ICE's strength");
            assert_eq!(ice[ICE_PENDING], 2.0 / MAX_SUBROUTINES);
        }

        // Face-down, the Runner sees only that ICE is there; the Corp
        // still reads its own card.
        state.corp.installed[0].rezzed = false;
        state.active_run.as_mut().expect("run").ice[0].rezzed = false;
        let runner = encode_observation(&state, &registry, Side::Runner);
        let ice = &runner[RUN_BLOCK_START + RUN_ICE_START..RUN_BLOCK_START + RUN_ICE_START + RUN_ICE_LEN];
        assert_eq!(ice[0], 1.0);
        assert_eq!(ice[ICE_REZZED], 0.0);
        assert!(ice[ICE_STRENGTH..].iter().all(|v| *v == 0.0), "unrezzed ICE shows the Runner no face: {ice:?}");
        let corp = encode_observation(&state, &registry, Side::Corp);
        let ice = &corp[RUN_BLOCK_START + RUN_ICE_START..RUN_BLOCK_START + RUN_ICE_START + RUN_ICE_LEN];
        assert_eq!(ice[ICE_STRENGTH], 4.0 / MAX_STRENGTH);
    }

    const DECISION_CHOOSE_CARDS: usize = 1;
    const DECISION_OWED_BY_SELF: usize = 8;
    const DECISION_OWED_BY_OPPONENT: usize = 9;
    const DECISION_SELECTED: usize = 10;

    /// A parked selection is named, with whose it is and how far along it
    /// is — the state `eval` charges `unresolved_decision_weight` for, and
    /// the one every livelock this repo has seen sat inside.
    #[test]
    fn a_parked_selection_encodes_its_kind_owner_and_progress() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnInstalled,
            filter: CardFilter::Advanceable,
            min: 1,
            max: 2,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected: vec![0],
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });

        let corp = encode_observation(&state, &registry, Side::Corp);
        let decision = &corp[DECISION_BLOCK_START..DECISION_BLOCK_START + DECISION_BLOCK_LEN];
        assert_eq!(decision[DECISION_CHOOSE_CARDS], 1.0);
        assert_eq!(decision[DECISION_OWED_BY_SELF], 1.0);
        assert_eq!(decision[DECISION_OWED_BY_OPPONENT], 0.0);
        assert_eq!(decision[DECISION_SELECTED], 0.5, "one of two selected");

        let runner = encode_observation(&state, &registry, Side::Runner);
        let decision = &runner[DECISION_BLOCK_START..DECISION_BLOCK_START + DECISION_BLOCK_LEN];
        assert_eq!(decision[DECISION_OWED_BY_SELF], 0.0);
        assert_eq!(decision[DECISION_OWED_BY_OPPONENT], 1.0);

        state.pending_decision = None;
        let idle = encode_observation(&state, &registry, Side::Corp);
        assert!(idle[DECISION_BLOCK_START..DECISION_BLOCK_START + DECISION_BLOCK_LEN].iter().all(|v| *v == 0.0));
    }
}
