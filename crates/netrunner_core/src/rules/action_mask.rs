//! A fixed-size, indexed encoding of `PlayerAction` for MCTS/RL integration
//! (a neural-net policy head, or any tensor/array-based search
//! implementation needs a bounded categorical action space, not a
//! variable-length `Vec<PlayerAction>`).
//!
//! `rules::legal_actions::legal_actions` already fully answers "what can
//! this player legally do right now" — this module adds *only* a bijection
//! between `PlayerAction` and a fixed `0..ActionSpace::SIZE` index range,
//! built on top of it. It never re-derives legality itself.
//!
//! `PlayerAction`'s dynamic fields (which card, which index, which bid
//! amount) don't have a natural finite size, so each is capped by a
//! generous constant below and indexed by *position* in its zone's already
//! `Vec`-ordered list (`corp.hq`, `runner.grip`, `corp.installed`,
//! `runner.rig`, a run's `selectable_cards`) rather than by card identity.
//! **These caps bound what's *representable* in this fixed space, not what
//! the rules allow** — an action whose dynamic field exceeds its cap (a
//! 13th card in hand, a trace bid over 30 credits) is legal per
//! `legal_actions` as always, but has no index here and is silently absent
//! from `get_action_mask`. Real games essentially never reach these caps;
//! widen the relevant constant if one ever does.

use crate::cards::CardRegistry;
use crate::dsl::CardId;
use crate::rules::action::PlayerAction;
use crate::rules::legal_actions::legal_actions;
use crate::rules::run::{AccessPhase, ServerId};
use crate::rules::state::{GamePhase, GameState, InstallId, InstallSlot, Side};

pub const MAX_HAND_SIZE: usize = 12;
/// Raised 20 → 32 after real games overflowed it. This module's doc
/// comment used to claim "real games essentially never reach these caps";
/// for this one that was simply wrong — ordinary System Gateway matchups
/// reach 23-24 installed Corp cards (ICE on three centrals plus several
/// remotes, each with contents), at which point `RezIce` on the outermost
/// cards had no index. A legal action with no index is invisible to
/// `get_action_mask`, and when it is the *only* legal action the mask is
/// empty and `netrunner_bots`' index adapter has nothing to fall back to.
///
/// 32 leaves real headroom rather than just clearing the observed figure:
/// remote servers are unbounded in principle, so this cap should sit well
/// above anything a game plausibly produces.
pub const MAX_INSTALLED_PER_SIDE: usize = 32;
pub const MAX_REMOTE_SERVERS: usize = 10;
pub const MAX_SUBROUTINES: usize = 8;
pub const MAX_ABILITIES_PER_CARD: usize = 4;
pub const MAX_ACCESS_SELECTION: usize = MAX_HAND_SIZE;
pub const MAX_TRACE_BID: u32 = 30;
/// The largest `Cost::AnyOf` any System Gateway card offers (Manegarm
/// Skunkworks: clicks or credits) — widen if a future card needs more
/// alternatives.
pub const MAX_COST_CHOICE_OPTIONS: usize = 2;
/// The largest `Effect::PresentChoice` option list any System Gateway card
/// offers — widen if a future card needs more.
pub const MAX_PENDING_CHOICE_OPTIONS: usize = 2;

/// `Hq`/`RnD`/`Archives` plus `MAX_REMOTE_SERVERS` numbered remotes.
const ZONE_COUNT: usize = 3 + MAX_REMOTE_SERVERS;

// Segment layout: a fixed, ordered sequence of constant-size blocks. Each
// `_START` is the previous segment's start plus its length, computed as a
// const expression so the table can't drift out of sync with `SIZE`.
const UNIT_START: usize = 0;
const UNIT_LEN: usize = 9;

const GAIN_CREDIT_START: usize = UNIT_START + UNIT_LEN;
const GAIN_CREDIT_LEN: usize = 2;

const PASS_PRIORITY_START: usize = GAIN_CREDIT_START + GAIN_CREDIT_LEN;
const PASS_PRIORITY_LEN: usize = 2;

const INSTALL_CARD_START: usize = PASS_PRIORITY_START + PASS_PRIORITY_LEN;
const INSTALL_CARD_LEN: usize = MAX_HAND_SIZE * ZONE_COUNT * 2;

const REZ_ICE_START: usize = INSTALL_CARD_START + INSTALL_CARD_LEN;
const REZ_ICE_LEN: usize = MAX_INSTALLED_PER_SIDE;

const INITIATE_RUN_START: usize = REZ_ICE_START + REZ_ICE_LEN;
const INITIATE_RUN_LEN: usize = ZONE_COUNT;

const PLAY_EVENT_START: usize = INITIATE_RUN_START + INITIATE_RUN_LEN;
const PLAY_EVENT_LEN: usize = MAX_HAND_SIZE;

const INSTALL_HARDWARE_START: usize = PLAY_EVENT_START + PLAY_EVENT_LEN;
const INSTALL_HARDWARE_LEN: usize = MAX_HAND_SIZE;

const INSTALL_PROGRAM_START: usize = INSTALL_HARDWARE_START + INSTALL_HARDWARE_LEN;
const INSTALL_PROGRAM_LEN: usize = MAX_HAND_SIZE;

const PLAY_OPERATION_START: usize = INSTALL_PROGRAM_START + INSTALL_PROGRAM_LEN;
const PLAY_OPERATION_LEN: usize = MAX_HAND_SIZE;

const BREAK_SUBROUTINE_START: usize = PLAY_OPERATION_START + PLAY_OPERATION_LEN;
const BREAK_SUBROUTINE_LEN: usize = MAX_SUBROUTINES;

const DISCARD_CARD_START: usize = BREAK_SUBROUTINE_START + BREAK_SUBROUTINE_LEN;
const DISCARD_CARD_LEN: usize = MAX_HAND_SIZE;

const ACTIVATE_ABILITY_CORP_START: usize = DISCARD_CARD_START + DISCARD_CARD_LEN;
const ACTIVATE_ABILITY_CORP_LEN: usize = MAX_INSTALLED_PER_SIDE * MAX_ABILITIES_PER_CARD;

const ACTIVATE_ABILITY_RUNNER_START: usize = ACTIVATE_ABILITY_CORP_START + ACTIVATE_ABILITY_CORP_LEN;
const ACTIVATE_ABILITY_RUNNER_LEN: usize = MAX_INSTALLED_PER_SIDE * MAX_ABILITIES_PER_CARD;

const ADVANCE_CARD_START: usize = ACTIVATE_ABILITY_RUNNER_START + ACTIVATE_ABILITY_RUNNER_LEN;
const ADVANCE_CARD_LEN: usize = MAX_INSTALLED_PER_SIDE;

const SCORE_AGENDA_START: usize = ADVANCE_CARD_START + ADVANCE_CARD_LEN;
const SCORE_AGENDA_LEN: usize = MAX_INSTALLED_PER_SIDE;

const TRASH_RESOURCE_START: usize = SCORE_AGENDA_START + SCORE_AGENDA_LEN;
const TRASH_RESOURCE_LEN: usize = MAX_INSTALLED_PER_SIDE;

const SELECT_CARD_TO_ACCESS_START: usize = TRASH_RESOURCE_START + TRASH_RESOURCE_LEN;
const SELECT_CARD_TO_ACCESS_LEN: usize = MAX_ACCESS_SELECTION;

const STEAL_AGENDA_START: usize = SELECT_CARD_TO_ACCESS_START + SELECT_CARD_TO_ACCESS_LEN;
const TRASH_ACCESSED_CARD_START: usize = STEAL_AGENDA_START + 1;
const PASS_ACCESSED_CARD_START: usize = TRASH_ACCESSED_CARD_START + 1;
const PAY_ACCESS_TRIGGER_START: usize = PASS_ACCESSED_CARD_START + 1;
const DECLINE_TRIGGER_START: usize = PAY_ACCESS_TRIGGER_START + 1;

const CORP_TRACE_BID_START: usize = DECLINE_TRIGGER_START + 1;
const CORP_TRACE_BID_LEN: usize = MAX_TRACE_BID as usize + 1;

const RUNNER_TRACE_BID_START: usize = CORP_TRACE_BID_START + CORP_TRACE_BID_LEN;
const RUNNER_TRACE_BID_LEN: usize = MAX_TRACE_BID as usize + 1;

// `AcceptPendingPaidChoice`'s `cost_option_index`: local 0 means `None`
// (the pending cost isn't `Cost::AnyOf`), local `1 + i` means
// `Some(i)`.
const ACCEPT_PENDING_PAID_CHOICE_START: usize = RUNNER_TRACE_BID_START + RUNNER_TRACE_BID_LEN;
const ACCEPT_PENDING_PAID_CHOICE_LEN: usize = 1 + MAX_COST_CHOICE_OPTIONS;

const RESOLVE_PENDING_CHOICE_START: usize = ACCEPT_PENDING_PAID_CHOICE_START + ACCEPT_PENDING_PAID_CHOICE_LEN;
const RESOLVE_PENDING_CHOICE_LEN: usize = MAX_PENDING_CHOICE_OPTIONS;

/// `ToggleCardSelection` is encoded by position within the pending
/// `PendingDecision::ChooseCards`'s *raw* zone contents (see
/// `pending_choice::zone_card_ids`) — the largest such zone any System
/// Gateway card selects from is an installed-card list
/// (`MAX_INSTALLED_PER_SIDE`), which dominates the hand-sized zones
/// (HQ/Archives/R&D/Stack/Grip/Heap, all `<= MAX_HAND_SIZE`).
///
/// The action now *carries* that position, so encode and decode are the
/// identity here and neither needs the `GameState` any more. This segment's
/// meaning is unchanged — the payload caught up with the encoding, not the
/// other way round.
const TOGGLE_CARD_SELECTION_START: usize = RESOLVE_PENDING_CHOICE_START + RESOLVE_PENDING_CHOICE_LEN;
const TOGGLE_CARD_SELECTION_LEN: usize = MAX_INSTALLED_PER_SIDE;

const CONFIRM_CARD_SELECTION_START: usize = TOGGLE_CARD_SELECTION_START + TOGGLE_CARD_SELECTION_LEN;

const CHOOSE_SERVER_START: usize = CONFIRM_CARD_SELECTION_START + 1;
const CHOOSE_SERVER_LEN: usize = ZONE_COUNT;

const INSTALL_RESOURCE_START: usize = CHOOSE_SERVER_START + CHOOSE_SERVER_LEN;
const INSTALL_RESOURCE_LEN: usize = MAX_HAND_SIZE;

/// `InstallProgramOnIce` — every Trojan Program hand slot crossed with
/// every installed-ICE slot (`MAX_HAND_SIZE * MAX_INSTALLED_PER_SIDE`).
/// The single largest segment in the whole encoding (M6/hosting) —
/// deliberately verified against the plan's estimate rather than assumed.
const INSTALL_PROGRAM_ON_ICE_START: usize = INSTALL_RESOURCE_START + INSTALL_RESOURCE_LEN;
const INSTALL_PROGRAM_ON_ICE_LEN: usize = MAX_HAND_SIZE * MAX_INSTALLED_PER_SIDE;

const BREAK_SUBROUTINE_WITH_CLICK_START: usize = INSTALL_PROGRAM_ON_ICE_START + INSTALL_PROGRAM_ON_ICE_LEN;
const BREAK_SUBROUTINE_WITH_CLICK_LEN: usize = MAX_SUBROUTINES;

/// `PurgeVirusCounters` is payload-free and would sit naturally in the
/// `UNIT` segment with the other nine — but inserting it there would shift
/// every subsequent index by one, silently changing what ~1,000 slots mean
/// to an already-exported policy network. Appended here instead, so every
/// existing index keeps its meaning and only the head *width* changes.
/// Same principle as `observation::CARD_VOCAB`'s numeric_id ordering:
/// append, never shift.
const PURGE_VIRUS_COUNTERS_START: usize = BREAK_SUBROUTINE_WITH_CLICK_START + BREAK_SUBROUTINE_WITH_CLICK_LEN;
const PURGE_VIRUS_COUNTERS_LEN: usize = 1;

/// `ChooseTriggerToResolve` is encoded by position within the parked
/// `PendingDecision::ChooseTriggerOrder`'s own `pending` list, which is
/// bounded by how many of one side's cards can react at once — at most
/// every installed card plus their identity. `MAX_INSTALLED_PER_SIDE`
/// covers it with room to spare, matching how `ToggleCardSelection` bounds
/// its own zone-position encoding.
///
/// Appended, like `PurgeVirusCounters` before it — see that segment's doc
/// comment for why nothing is ever inserted mid-space.
const CHOOSE_TRIGGER_START: usize = PURGE_VIRUS_COUNTERS_START + PURGE_VIRUS_COUNTERS_LEN;
const CHOOSE_TRIGGER_LEN: usize = MAX_INSTALLED_PER_SIDE;

/// A fixed, categorical index space over `PlayerAction` — see the module
/// doc comment. A zero-sized marker type; every operation is an associated
/// function/const, since the encoding itself carries no per-instance state.
pub struct ActionSpace;

impl ActionSpace {
    pub const SIZE: usize = CHOOSE_TRIGGER_START + CHOOSE_TRIGGER_LEN;

    /// The flat index `action` occupies given `state` — `None` if `action`
    /// can't be placed (a dynamic field exceeds its cap, or a
    /// state-inferred field like `DiscardCard`'s side/`BreakSubroutine`'s
    /// ice can't be resolved from `state` at all). Not itself a legality
    /// check — an out-of-range or currently-nonsensical `action` may still
    /// return `Some` (see `action_at`'s doc comment on the single-slot
    /// access segments); use `legal_actions`/`get_action_mask` for
    /// legality.
    pub fn index_of(state: &GameState, action: &PlayerAction) -> Option<usize> {
        match action {
            PlayerAction::DrawCardClick => Some(UNIT_START),
            PlayerAction::ContinueRun => Some(UNIT_START + 1),
            PlayerAction::JackOut => Some(UNIT_START + 2),
            PlayerAction::CompleteRun => Some(UNIT_START + 3),
            PlayerAction::EndTurn => Some(UNIT_START + 4),
            PlayerAction::KeepHand => Some(UNIT_START + 5),
            PlayerAction::TakeMulligan => Some(UNIT_START + 6),
            PlayerAction::RemoveTag => Some(UNIT_START + 7),
            PlayerAction::DeclinePendingPaidChoice => Some(UNIT_START + 8),

            PlayerAction::PurgeVirusCounters => Some(PURGE_VIRUS_COUNTERS_START),

            PlayerAction::ChooseTriggerToResolve { card_id } => {
                let Some(crate::rules::state::PendingDecision::ChooseTriggerOrder { pending, .. }) =
                    state.pending_decision.as_ref()
                else {
                    return None;
                };
                let slot = pending.iter().position(|due| &due.card == card_id)?;
                (slot < CHOOSE_TRIGGER_LEN).then_some(CHOOSE_TRIGGER_START + slot)
            }

            PlayerAction::GainCreditClick { side } => Some(GAIN_CREDIT_START + side_index(*side)),
            PlayerAction::PassPriority { side } => Some(PASS_PRIORITY_START + side_index(*side)),

            PlayerAction::InstallCard { card_id, zone, slot } => {
                let hand_slot = bounded_position(&state.corp.hq, card_id, MAX_HAND_SIZE)?;
                let zone_idx = encode_zone(*zone)?;
                let slot_idx = encode_install_slot(*slot);
                Some(INSTALL_CARD_START + (hand_slot * ZONE_COUNT + zone_idx) * 2 + slot_idx)
            }

            PlayerAction::RezIce { ice } => {
                let slot = bounded_position_installed(&state.corp.installed, *ice, MAX_INSTALLED_PER_SIDE)?;
                Some(REZ_ICE_START + slot)
            }

            PlayerAction::InitiateRun { server } => Some(INITIATE_RUN_START + encode_zone(*server)?),

            PlayerAction::PlayEvent { card_id } => {
                Some(PLAY_EVENT_START + bounded_position(&state.runner.grip, card_id, MAX_HAND_SIZE)?)
            }
            PlayerAction::InstallHardware { card_id } => {
                Some(INSTALL_HARDWARE_START + bounded_position(&state.runner.grip, card_id, MAX_HAND_SIZE)?)
            }
            PlayerAction::InstallProgram { card_id, .. } => {
                Some(INSTALL_PROGRAM_START + bounded_position(&state.runner.grip, card_id, MAX_HAND_SIZE)?)
            }
            PlayerAction::InstallResource { card_id } => {
                Some(INSTALL_RESOURCE_START + bounded_position(&state.runner.grip, card_id, MAX_HAND_SIZE)?)
            }
            PlayerAction::InstallProgramOnIce { card_id, host, .. } => {
                let hand_slot = bounded_position(&state.runner.grip, card_id, MAX_HAND_SIZE)?;
                let ice_slot = bounded_position_installed(&state.corp.installed, *host, MAX_INSTALLED_PER_SIDE)?;
                Some(INSTALL_PROGRAM_ON_ICE_START + hand_slot * MAX_INSTALLED_PER_SIDE + ice_slot)
            }
            PlayerAction::PlayOperation { card_id } => {
                Some(PLAY_OPERATION_START + bounded_position(&state.corp.hq, card_id, MAX_HAND_SIZE)?)
            }

            PlayerAction::BreakSubroutine { subroutine_index, .. } => {
                (*subroutine_index < MAX_SUBROUTINES).then_some(BREAK_SUBROUTINE_START + subroutine_index)
            }

            PlayerAction::BreakSubroutineWithClick { subroutine_index, .. } => (*subroutine_index
                < MAX_SUBROUTINES)
                .then_some(BREAK_SUBROUTINE_WITH_CLICK_START + subroutine_index),

            PlayerAction::DiscardCard { card_id } => {
                let GamePhase::Discard { side, .. } = state.phase else { return None };
                let hand = hand_for(state, side);
                Some(DISCARD_CARD_START + bounded_position(hand, card_id, MAX_HAND_SIZE)?)
            }

            PlayerAction::ActivateAbility { target, ability_index } => {
                if *ability_index >= MAX_ABILITIES_PER_CARD {
                    return None;
                }
                if let Some(slot) = bounded_position_installed(&state.corp.installed, *target, MAX_INSTALLED_PER_SIDE) {
                    Some(ACTIVATE_ABILITY_CORP_START + slot * MAX_ABILITIES_PER_CARD + ability_index)
                } else {
                    let slot = bounded_position_rig(&state.runner.rig, *target, MAX_INSTALLED_PER_SIDE)?;
                    Some(ACTIVATE_ABILITY_RUNNER_START + slot * MAX_ABILITIES_PER_CARD + ability_index)
                }
            }

            PlayerAction::AdvanceCard { target } => {
                Some(ADVANCE_CARD_START + bounded_position_installed(&state.corp.installed, *target, MAX_INSTALLED_PER_SIDE)?)
            }
            PlayerAction::ScoreAgenda { target } => {
                Some(SCORE_AGENDA_START + bounded_position_installed(&state.corp.installed, *target, MAX_INSTALLED_PER_SIDE)?)
            }
            PlayerAction::TrashResource { target } => {
                Some(TRASH_RESOURCE_START + bounded_position_rig(&state.runner.rig, *target, MAX_INSTALLED_PER_SIDE)?)
            }

            PlayerAction::SelectCardToAccess { card_id } => {
                let AccessPhase::SelectNextCard { selectable_cards } = access_phase(state)? else { return None };
                Some(SELECT_CARD_TO_ACCESS_START + bounded_position(selectable_cards, card_id, MAX_ACCESS_SELECTION)?)
            }

            // Single-slot segments: the pending card is entirely
            // state-determined (at most one `AccessPhase::PendingChoice`/
            // `PendingInteractiveTrigger` card at a time), so `index_of`
            // doesn't need to (and structurally can't distinguish) which
            // `card_id` these carry — every value maps to the same lone
            // slot. `action_at` fills in the *actual* pending card_id from
            // `state`, so this only round-trips for a `card_id` that
            // genuinely matches it (true of every member of
            // `legal_actions`, which is what the roundtrip tests exercise).
            PlayerAction::StealAgenda { .. } => Some(STEAL_AGENDA_START),
            PlayerAction::TrashAccessedCard { .. } => Some(TRASH_ACCESSED_CARD_START),
            PlayerAction::PassAccessedCard { .. } => Some(PASS_ACCESSED_CARD_START),
            PlayerAction::PayAccessTrigger { .. } => Some(PAY_ACCESS_TRIGGER_START),
            PlayerAction::DeclineAccessTrigger { .. } => Some(DECLINE_TRIGGER_START),

            PlayerAction::SubmitCorpTraceBid { amount } => {
                (*amount <= MAX_TRACE_BID).then_some(CORP_TRACE_BID_START + *amount as usize)
            }
            PlayerAction::SubmitRunnerTraceBid { amount } => {
                (*amount <= MAX_TRACE_BID).then_some(RUNNER_TRACE_BID_START + *amount as usize)
            }

            PlayerAction::AcceptPendingPaidChoice { cost_option_index } => match cost_option_index {
                None => Some(ACCEPT_PENDING_PAID_CHOICE_START),
                Some(i) => (*i < MAX_COST_CHOICE_OPTIONS).then_some(ACCEPT_PENDING_PAID_CHOICE_START + 1 + i),
            },

            PlayerAction::ResolvePendingChoice { option_index } => {
                (*option_index < MAX_PENDING_CHOICE_OPTIONS).then_some(RESOLVE_PENDING_CHOICE_START + option_index)
            }

            // The payload *is* the encoding now — no zone lookup, because
            // `ToggleCardSelection` already carries the position this
            // segment was always built from.
            PlayerAction::ToggleCardSelection { position } => {
                (*position < TOGGLE_CARD_SELECTION_LEN).then_some(TOGGLE_CARD_SELECTION_START + position)
            }

            PlayerAction::ConfirmCardSelection => Some(CONFIRM_CARD_SELECTION_START),

            PlayerAction::ChooseServerForPendingDecision { server } => {
                Some(CHOOSE_SERVER_START + encode_zone(*server)?)
            }
        }
    }

    /// The `PlayerAction` occupying `index` given `state` — `None` if
    /// `index` is out of range, or `state` has nothing at the resolved slot
    /// (e.g. hand slot 7 against a 5-card hand). This is what makes
    /// `get_action_mask` correct without separate bounds-checking: an
    /// index either decodes to a real, currently-meaningful action or it
    /// doesn't.
    pub fn action_at(state: &GameState, index: usize) -> Option<PlayerAction> {
        if let Some(local) = in_segment(index, UNIT_START, UNIT_LEN) {
            return Some(match local {
                0 => PlayerAction::DrawCardClick,
                1 => PlayerAction::ContinueRun,
                2 => PlayerAction::JackOut,
                3 => PlayerAction::CompleteRun,
                4 => PlayerAction::EndTurn,
                5 => PlayerAction::KeepHand,
                6 => PlayerAction::TakeMulligan,
                7 => PlayerAction::RemoveTag,
                _ => PlayerAction::DeclinePendingPaidChoice,
            });
        }
        if let Some(local) = in_segment(index, GAIN_CREDIT_START, GAIN_CREDIT_LEN) {
            return Some(PlayerAction::GainCreditClick { side: side_from_index(local)? });
        }
        if let Some(local) = in_segment(index, PASS_PRIORITY_START, PASS_PRIORITY_LEN) {
            return Some(PlayerAction::PassPriority { side: side_from_index(local)? });
        }
        if let Some(local) = in_segment(index, INSTALL_CARD_START, INSTALL_CARD_LEN) {
            let hand_slot = local / (ZONE_COUNT * 2);
            let rem = local % (ZONE_COUNT * 2);
            let zone = decode_zone(rem / 2)?;
            let slot = decode_install_slot(rem % 2);
            let card_id = state.corp.hq.get(hand_slot)?.clone();
            return Some(PlayerAction::InstallCard { card_id, zone, slot });
        }
        if let Some(local) = in_segment(index, REZ_ICE_START, REZ_ICE_LEN) {
            let ice = state.corp.installed.get(local)?.install_id;
            return Some(PlayerAction::RezIce { ice });
        }
        if let Some(local) = in_segment(index, INITIATE_RUN_START, INITIATE_RUN_LEN) {
            return Some(PlayerAction::InitiateRun { server: decode_zone(local)? });
        }
        if let Some(local) = in_segment(index, PLAY_EVENT_START, PLAY_EVENT_LEN) {
            let card_id = state.runner.grip.get(local)?.clone();
            return Some(PlayerAction::PlayEvent { card_id });
        }
        if let Some(local) = in_segment(index, INSTALL_HARDWARE_START, INSTALL_HARDWARE_LEN) {
            let card_id = state.runner.grip.get(local)?.clone();
            return Some(PlayerAction::InstallHardware { card_id });
        }
        if let Some(local) = in_segment(index, INSTALL_PROGRAM_START, INSTALL_PROGRAM_LEN) {
            let card_id = state.runner.grip.get(local)?.clone();
            return Some(PlayerAction::InstallProgram { card_id });
        }
        if let Some(local) = in_segment(index, PLAY_OPERATION_START, PLAY_OPERATION_LEN) {
            let card_id = state.corp.hq.get(local)?.clone();
            return Some(PlayerAction::PlayOperation { card_id });
        }
        if let Some(local) = in_segment(index, BREAK_SUBROUTINE_START, BREAK_SUBROUTINE_LEN) {
            let ice_id = current_ice_id(state)?;
            return Some(PlayerAction::BreakSubroutine { ice_id, subroutine_index: local });
        }
        if let Some(local) = in_segment(index, DISCARD_CARD_START, DISCARD_CARD_LEN) {
            let GamePhase::Discard { side, .. } = state.phase else { return None };
            let card_id = hand_for(state, side).get(local)?.clone();
            return Some(PlayerAction::DiscardCard { card_id });
        }
        if let Some(local) = in_segment(index, ACTIVATE_ABILITY_CORP_START, ACTIVATE_ABILITY_CORP_LEN) {
            let slot = local / MAX_ABILITIES_PER_CARD;
            let ability_index = local % MAX_ABILITIES_PER_CARD;
            let target = state.corp.installed.get(slot)?.install_id;
            return Some(PlayerAction::ActivateAbility { target, ability_index });
        }
        if let Some(local) = in_segment(index, ACTIVATE_ABILITY_RUNNER_START, ACTIVATE_ABILITY_RUNNER_LEN) {
            let slot = local / MAX_ABILITIES_PER_CARD;
            let ability_index = local % MAX_ABILITIES_PER_CARD;
            let target = state.runner.rig.get(slot)?.install_id;
            return Some(PlayerAction::ActivateAbility { target, ability_index });
        }
        if let Some(local) = in_segment(index, ADVANCE_CARD_START, ADVANCE_CARD_LEN) {
            let target = state.corp.installed.get(local)?.install_id;
            return Some(PlayerAction::AdvanceCard { target });
        }
        if let Some(local) = in_segment(index, SCORE_AGENDA_START, SCORE_AGENDA_LEN) {
            let target = state.corp.installed.get(local)?.install_id;
            return Some(PlayerAction::ScoreAgenda { target });
        }
        if let Some(local) = in_segment(index, TRASH_RESOURCE_START, TRASH_RESOURCE_LEN) {
            let target = state.runner.rig.get(local)?.install_id;
            return Some(PlayerAction::TrashResource { target });
        }
        if let Some(local) = in_segment(index, SELECT_CARD_TO_ACCESS_START, SELECT_CARD_TO_ACCESS_LEN) {
            let AccessPhase::SelectNextCard { selectable_cards } = access_phase(state)? else { return None };
            let card_id = selectable_cards.get(local)?.clone();
            return Some(PlayerAction::SelectCardToAccess { card_id });
        }
        if index == STEAL_AGENDA_START {
            return Some(PlayerAction::StealAgenda { card_id: pending_choice_card(state)? });
        }
        if index == TRASH_ACCESSED_CARD_START {
            return Some(PlayerAction::TrashAccessedCard { card_id: pending_choice_card(state)? });
        }
        if index == PASS_ACCESSED_CARD_START {
            return Some(PlayerAction::PassAccessedCard { card_id: pending_choice_card(state)? });
        }
        if index == PAY_ACCESS_TRIGGER_START {
            return Some(PlayerAction::PayAccessTrigger { card_id: pending_interactive_card(state)? });
        }
        if index == DECLINE_TRIGGER_START {
            return Some(PlayerAction::DeclineAccessTrigger { card_id: pending_interactive_card(state)? });
        }
        if let Some(local) = in_segment(index, CORP_TRACE_BID_START, CORP_TRACE_BID_LEN) {
            return Some(PlayerAction::SubmitCorpTraceBid { amount: local as u32 });
        }
        if let Some(local) = in_segment(index, RUNNER_TRACE_BID_START, RUNNER_TRACE_BID_LEN) {
            return Some(PlayerAction::SubmitRunnerTraceBid { amount: local as u32 });
        }
        if let Some(local) = in_segment(index, ACCEPT_PENDING_PAID_CHOICE_START, ACCEPT_PENDING_PAID_CHOICE_LEN) {
            let cost_option_index = if local == 0 { None } else { Some(local - 1) };
            return Some(PlayerAction::AcceptPendingPaidChoice { cost_option_index });
        }
        if let Some(local) = in_segment(index, RESOLVE_PENDING_CHOICE_START, RESOLVE_PENDING_CHOICE_LEN) {
            return Some(PlayerAction::ResolvePendingChoice { option_index: local });
        }
        if let Some(local) = in_segment(index, TOGGLE_CARD_SELECTION_START, TOGGLE_CARD_SELECTION_LEN) {
            return Some(PlayerAction::ToggleCardSelection { position: local });
        }
        if index == CONFIRM_CARD_SELECTION_START {
            return Some(PlayerAction::ConfirmCardSelection);
        }
        if let Some(local) = in_segment(index, CHOOSE_SERVER_START, CHOOSE_SERVER_LEN) {
            return Some(PlayerAction::ChooseServerForPendingDecision { server: decode_zone(local)? });
        }
        if let Some(local) = in_segment(index, INSTALL_RESOURCE_START, INSTALL_RESOURCE_LEN) {
            let card_id = state.runner.grip.get(local)?.clone();
            return Some(PlayerAction::InstallResource { card_id });
        }
        // Must stay the LAST check: `INSTALL_PROGRAM_ON_ICE_START` is
        // defined after every other segment in the const chain above, and
        // `in_segment` eagerly computes `index - start` (via `then_some`,
        // not a lazily-evaluated `then`) even when the range test fails —
        // checking this segment any earlier would underflow-panic for
        // every smaller index that reaches this line before matching an
        // earlier segment.
        if let Some(local) = in_segment(index, INSTALL_PROGRAM_ON_ICE_START, INSTALL_PROGRAM_ON_ICE_LEN) {
            let hand_slot = local / MAX_INSTALLED_PER_SIDE;
            let ice_slot = local % MAX_INSTALLED_PER_SIDE;
            let card_id = state.runner.grip.get(hand_slot)?.clone();
            let host = state.corp.installed.get(ice_slot)?.install_id;
            return Some(PlayerAction::InstallProgramOnIce { card_id, host });
        }
        // Same out-of-order-panic hazard as `INSTALL_PROGRAM_ON_ICE` above —
        // `BREAK_SUBROUTINE_WITH_CLICK` is defined last in the const chain,
        // so its check must stay last here too.
        if let Some(local) = in_segment(index, BREAK_SUBROUTINE_WITH_CLICK_START, BREAK_SUBROUTINE_WITH_CLICK_LEN) {
            let ice_id = current_ice_id(state)?;
            return Some(PlayerAction::BreakSubroutineWithClick { ice_id, subroutine_index: local });
        }
        if in_segment(index, PURGE_VIRUS_COUNTERS_START, PURGE_VIRUS_COUNTERS_LEN).is_some() {
            return Some(PlayerAction::PurgeVirusCounters);
        }
        if let Some(local) = in_segment(index, CHOOSE_TRIGGER_START, CHOOSE_TRIGGER_LEN) {
            let crate::rules::state::PendingDecision::ChooseTriggerOrder { pending, .. } =
                state.pending_decision.as_ref()?
            else {
                return None;
            };
            let card_id = pending.get(local)?.card.clone();
            return Some(PlayerAction::ChooseTriggerToResolve { card_id });
        }
        None
    }
}

/// A boolean mask over `0..ActionSpace::SIZE`: `true` at exactly the
/// indices whose `ActionSpace::action_at` yields a member of
/// `legal_actions(state, registry)`. All legality logic is `legal_actions`'
/// — this only decides which fixed index each legal action occupies.
pub fn get_action_mask(state: &GameState, registry: &CardRegistry) -> Vec<bool> {
    let legal = legal_actions(state, registry);
    (0..ActionSpace::SIZE)
        .map(|index| ActionSpace::action_at(state, index).is_some_and(|action| legal.contains(&action)))
        .collect()
}

fn in_segment(index: usize, start: usize, len: usize) -> Option<usize> {
    (index >= start && index < start + len).then_some(index - start)
}

fn side_index(side: Side) -> usize {
    match side {
        Side::Corp => 0,
        Side::Runner => 1,
    }
}

fn side_from_index(index: usize) -> Option<Side> {
    match index {
        0 => Some(Side::Corp),
        1 => Some(Side::Runner),
        _ => None,
    }
}

fn hand_for(state: &GameState, side: Side) -> &[CardId] {
    match side {
        Side::Corp => &state.corp.hq,
        Side::Runner => &state.runner.grip,
    }
}

fn encode_zone(server: ServerId) -> Option<usize> {
    match server {
        ServerId::Hq => Some(0),
        ServerId::RnD => Some(1),
        ServerId::Archives => Some(2),
        ServerId::Remote(n) => {
            let n = n as usize;
            (n < MAX_REMOTE_SERVERS).then_some(3 + n)
        }
    }
}

fn decode_zone(index: usize) -> Option<ServerId> {
    match index {
        0 => Some(ServerId::Hq),
        1 => Some(ServerId::RnD),
        2 => Some(ServerId::Archives),
        n if n < ZONE_COUNT => Some(ServerId::Remote((n - 3) as u32)),
        _ => None,
    }
}

fn encode_install_slot(slot: InstallSlot) -> usize {
    match slot {
        InstallSlot::Ice => 0,
        InstallSlot::Root => 1,
    }
}

fn decode_install_slot(index: usize) -> InstallSlot {
    if index == 0 { InstallSlot::Ice } else { InstallSlot::Root }
}

fn bounded_position(zone: &[CardId], card_id: &CardId, cap: usize) -> Option<usize> {
    let position = zone.iter().position(|id| id == card_id)?;
    (position < cap).then_some(position)
}

/// Where `id` sits in the Corp's install list, if it is still installed and
/// within the encodable range.
///
/// Replaces a `bounded_position_installed` that searched by `CardId` and
/// took the **first** match, so with three *Tithe* installed every
/// `ScoreAgenda`/`AdvanceCard`/`RezIce` on the second or third encoded to
/// the first one's slot — an action on those copies was simply unreachable
/// through the mask. Searching by `InstallId` makes each copy its own slot.
/// No segment moved: this is the same position arithmetic, now correct.
fn bounded_position_installed(
    installed: &[crate::rules::state::InstalledCard],
    id: InstallId,
    cap: usize,
) -> Option<usize> {
    let position = installed.iter().position(|c| c.install_id == id)?;
    (position < cap).then_some(position)
}

/// The rig-side mirror of `bounded_position_installed`, and the same fix:
/// two copies of one Resource are now two distinct `TrashResource` slots.
fn bounded_position_rig(
    rig: &[crate::rules::state::InstalledRunnerCard],
    id: InstallId,
    cap: usize,
) -> Option<usize> {
    let position = rig.iter().position(|c| c.install_id == id)?;
    (position < cap).then_some(position)
}

fn current_ice_id(state: &GameState) -> Option<CardId> {
    let run = state.active_run.as_ref()?;
    run.ice.get(run.position).map(|ice| ice.card_id.clone())
}

fn access_phase(state: &GameState) -> Option<&AccessPhase> {
    Some(&state.active_run.as_ref()?.access_state.as_ref()?.phase)
}

fn pending_choice_card(state: &GameState) -> Option<CardId> {
    match access_phase(state)? {
        AccessPhase::PendingChoice { card_id, .. } => Some(card_id.clone()),
        _ => None,
    }
}

fn pending_interactive_card(state: &GameState) -> Option<CardId> {
    match access_phase(state)? {
        AccessPhase::PendingInteractiveTrigger { card_id, .. } => Some(card_id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_support::{install_of, position_of};
    use crate::cards::CardRegistry;
    use crate::dsl::{AbilityDef, CardDefinition, CardType, Cost, Effect, IceType, SubroutineDef, Trigger, TriggeredEffect};
    use crate::rules::run::{AccessState, EncounteredSubroutine, RunIce, RunPhase, RunState, SubroutineStatus};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, InstalledCard, InstalledRunnerCard, MemoryUnits, PlayerResources,
        RunnerState,
    };

    fn base_state() -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources { credits: Credits(10), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources { credits: Credits(10), clicks: Clicks(4), agenda_points: AgendaPoints(0) },
                memory_units: MemoryUnits(10),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Corp),
            ..Default::default()
        }
    }

    fn hedge_fund() -> CardDefinition {
        CardDefinition {
            id: CardId("hedge_fund".to_string()),
            title: "Hedge Fund".to_string(),
            side: Side::Corp,
            card_type: CardType::Operation,
            cost: 5,
            triggers: vec![TriggeredEffect {
                trigger: Trigger::OnPlay,
                effects: vec![Effect::GainCredits(Side::Corp, 9)],
                requirement: None,
            }],
            is_playable: true,
            ..Default::default()
        }
    }

    fn corroder() -> CardDefinition {
        CardDefinition {
            id: CardId("corroder".to_string()),
            title: "Corroder".to_string(),
            side: Side::Runner,
            card_type: CardType::Program,
            cost: 2,
            abilities: vec![
                AbilityDef {
                    trigger: Trigger::Paid,
                    cost: Some(Cost::Credits(1)),
                    requirement: None,
                    effect: Effect::BoostStrength { amount: 1, duration: crate::dsl::BoostDuration::Encounter },
                    cost_discount_if: None,
                },
                AbilityDef {
                    trigger: Trigger::Paid,
                    cost: Some(Cost::Credits(1)),
                    requirement: None,
                    effect: Effect::BreakSubroutines {
                        count: crate::dsl::SubroutineBreakCount::Fixed(1),
                        restrict_to: Some(IceType::Barrier),
                    },
                    cost_discount_if: None,
                },
            ],
            strength: Some(2),
            is_playable: true,
            ..Default::default()
        }
    }

    fn assert_roundtrips(state: &GameState, registry: &CardRegistry) {
        for action in legal_actions(state, registry) {
            let index = ActionSpace::index_of(state, &action)
                .unwrap_or_else(|| panic!("no index for legal action {action:?}"));
            assert!(index < ActionSpace::SIZE, "index {index} out of range for {action:?}");
            let decoded = ActionSpace::action_at(state, index);
            assert_eq!(decoded, Some(action.clone()), "roundtrip mismatch for {action:?} at index {index}");
        }
    }

    fn assert_mask_matches_legal_actions(state: &GameState, registry: &CardRegistry) {
        let legal = legal_actions(state, registry);
        let mask = get_action_mask(state, registry);
        assert_eq!(mask.len(), ActionSpace::SIZE);
        for (index, &is_legal) in mask.iter().enumerate() {
            let decoded = ActionSpace::action_at(state, index);
            let expected = decoded.as_ref().is_some_and(|a| legal.contains(a));
            assert_eq!(is_legal, expected, "mask[{index}] disagrees with legal_actions (decoded: {decoded:?})");
        }
    }

    /// Three copies of one card must occupy three distinct slots.
    ///
    /// This is ROADMAP Phase 1 §3's "`ActionSpace` aliases duplicate card
    /// ids" bullet, as a regression test. `bounded_position_installed` used
    /// to search `corp.installed` by `CardId` and take the **first** match,
    /// so with three *Tithe* installed, `AdvanceCard`/`ScoreAgenda`/
    /// `RezIce` on the second or third all encoded to the first one's
    /// index. The action on those copies was not merely mis-indexed — it
    /// was unreachable through the mask entirely, and an index decoded back
    /// to an action naming the wrong card.
    #[test]
    fn three_copies_of_one_card_occupy_three_distinct_slots() {
        let mut registry = CardRegistry::new();
        registry.insert(CardDefinition {
            id: CardId("tithe".to_string()),
            title: "Tithe".to_string(),
            side: Side::Corp,
            card_type: CardType::Agenda,
            advancement_requirement: Some(3),
            agenda_points: Some(1),
            is_playable: true,
            ..Default::default()
        });

        let mut state = base_state();
        state.corp.installed = (0..3)
            .map(|i| InstalledCard {
                card: CardId("tithe".to_string()),
                install_id: InstallId(i + 1),
                server: ServerId::Remote(i),
                ..Default::default()
            })
            .collect();

        let indices: Vec<usize> = (0..3)
            .map(|i| {
                let action = PlayerAction::AdvanceCard { target: InstallId(i + 1) };
                ActionSpace::index_of(&state, &action).expect("every copy has an index")
            })
            .collect();

        assert_eq!(
            indices,
            vec![ADVANCE_CARD_START, ADVANCE_CARD_START + 1, ADVANCE_CARD_START + 2],
            "each copy gets its own slot, in install order"
        );

        // And decoding is the exact inverse, so an index never names a
        // different copy than the one it was encoded from.
        for (offset, index) in indices.iter().enumerate() {
            assert_eq!(
                ActionSpace::action_at(&state, *index),
                Some(PlayerAction::AdvanceCard { target: InstallId(offset as u32 + 1) })
            );
        }
    }

    /// The same aliasing, on the rig side.
    #[test]
    fn two_copies_of_one_resource_are_two_distinct_trash_slots() {
        let mut state = base_state();
        state.runner.rig = (0..2)
            .map(|i| InstalledRunnerCard {
                card: CardId("daily_casts".to_string()),
                install_id: InstallId(i + 1),
                ..Default::default()
            })
            .collect();

        let first = ActionSpace::index_of(&state, &PlayerAction::TrashResource { target: InstallId(1) });
        let second = ActionSpace::index_of(&state, &PlayerAction::TrashResource { target: InstallId(2) });
        assert_eq!(first, Some(TRASH_RESOURCE_START));
        assert_eq!(second, Some(TRASH_RESOURCE_START + 1));
    }

    /// An `InstallId` naming nothing on the table has no index, rather than
    /// silently encoding to some other card's slot.
    #[test]
    fn an_install_id_that_matches_nothing_has_no_index() {
        let state = base_state();
        assert_eq!(ActionSpace::index_of(&state, &PlayerAction::RezIce { ice: InstallId(9_999) }), None);
    }

    #[test]
    fn turn_one_corp_click_phase() {
        let mut registry = CardRegistry::new();
        registry.insert(hedge_fund());
        let mut state = base_state();
        state.corp.hq = vec![CardId("hedge_fund".to_string())];

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);

        let legal = legal_actions(&state, &registry);
        assert!(legal.contains(&PlayerAction::GainCreditClick { side: Side::Corp }));
        assert!(legal.contains(&PlayerAction::PlayOperation { card_id: CardId("hedge_fund".to_string()) }));
        assert!(!legal.contains(&PlayerAction::DrawCardClick), "DrawCardClick is Runner-only today");
    }

    /// The appended `PurgeVirusCounters` slot, checked explicitly rather
    /// than only via `assert_roundtrips`, since a payload-free action in
    /// its own one-wide segment is exactly where an off-by-one in the
    /// segment chain would show up.
    #[test]
    fn purge_virus_counters_roundtrips_in_its_own_segment() {
        let registry = CardRegistry::new();
        let state = base_state();

        let index = ActionSpace::index_of(&state, &PlayerAction::PurgeVirusCounters).expect("purge always encodes");
        assert!(index < ActionSpace::SIZE);
        assert_eq!(ActionSpace::action_at(&state, index), Some(PlayerAction::PurgeVirusCounters));

        assert!(
            legal_actions(&state, &registry).contains(&PlayerAction::PurgeVirusCounters),
            "the Corp has 3 clicks in their Action phase, so purge is legal even with no viruses in play"
        );
    }

    #[test]
    fn mid_run_encounter_window_icebreaker_and_subroutines() {
        let mut registry = CardRegistry::new();
        registry.insert(corroder());
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                card_id: CardId("ice_wall".to_string()),
                current_strength: 0,
                ice_type: IceType::Barrier,
                subroutines: vec![EncounteredSubroutine {
                    id: 0,
                    definition: SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun },
                    status: SubroutineStatus::Pending,
                }],
                rezzed: true,
            }],
            ..Default::default()
        });

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);

        let mask = get_action_mask(&state, &registry);
        let pump_index = ActionSpace::index_of(
            &state,
            &PlayerAction::ActivateAbility { target: install_of(&state, "corroder"), ability_index: 0 },
        )
        .unwrap();
        let break_index =
            ActionSpace::index_of(&state, &PlayerAction::BreakSubroutine { ice_id: CardId("ice_wall".to_string()), subroutine_index: 0 })
                .unwrap();
        assert!(mask[pump_index], "Corroder's pump ability should be legal mid-encounter");
        assert!(mask[break_index], "breaking the pending subroutine should be legal mid-encounter");
        assert!(!mask[UNIT_START + 4], "EndTurn should be illegal while a run is active");
    }

    #[test]
    fn end_of_turn_discard_prompt_only_offers_hand_cards() {
        let registry = CardRegistry::new();
        let mut state = base_state();
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };
        state.corp.hq = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.runner.grip = vec![CardId("not_offered".to_string())];

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);

        let mask = get_action_mask(&state, &registry);
        let discard_a = ActionSpace::index_of(&state, &PlayerAction::DiscardCard { card_id: CardId("card_a".to_string()) }).unwrap();
        let discard_b = ActionSpace::index_of(&state, &PlayerAction::DiscardCard { card_id: CardId("card_b".to_string()) }).unwrap();
        assert!(mask[discard_a]);
        assert!(mask[discard_b]);
        // Every other index must decode to something other than a
        // DiscardCard for a card not actually in the discarding hand.
        for (index, &is_legal) in mask.iter().enumerate().skip(DISCARD_CARD_START).take(DISCARD_CARD_LEN) {
            if index != discard_a && index != discard_b {
                assert!(!is_legal, "unexpected legal discard at index {index}");
            }
        }
    }

    #[test]
    fn end_game_state_has_no_legal_actions() {
        let registry = CardRegistry::new();
        let mut state = base_state();
        state.phase = GamePhase::GameOver(Side::Runner);

        assert!(state.is_over());
        assert!(legal_actions(&state, &registry).is_empty());
        let mask = get_action_mask(&state, &registry);
        assert_eq!(mask.len(), ActionSpace::SIZE);
        assert!(mask.iter().all(|&legal| !legal));
    }

    #[test]
    fn access_pending_choice_roundtrips() {
        let registry = CardRegistry::new();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState {
            server: ServerId::Remote(0),
            phase: RunPhase::AccessingCard,
            jack_out_permitted: true,
            access_state: Some(AccessState {
                server: ServerId::Remote(0),
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("agenda_x".to_string()),
                    can_trash: false,
                    trash_cost: None,
                    mandatory_steal: true,
                    steal_cost: None,
                },
                ..Default::default()
            }),
            ..Default::default()
        });

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);
    }

    #[test]
    fn trace_bid_roundtrips() {
        use crate::dsl::Effect as DslEffect;
        use crate::rules::state::TraceState;

        let registry = CardRegistry::new();
        let mut state = base_state();
        state.active_trace = Some(TraceState {
            initiating_card: None,
            base_strength: 2,
            corp_bid: None,
            effect_on_success: DslEffect::GiveTags(1),
            resume: crate::rules::state::TraceResume::None,
        });

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);
    }

    #[test]
    fn step_rejects_actions_outside_legal_actions() {
        let registry = CardRegistry::new();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce {
                card_id: CardId("ice_wall".to_string()),
                current_strength: 0,
                ice_type: IceType::Barrier,
                subroutines: Vec::new(),
                rezzed: false,
            }],
            ..Default::default()
        });

        let legal = legal_actions(&state, &registry);
        assert!(!legal.contains(&PlayerAction::EndTurn), "EndTurn should be illegal mid-run");
        assert!(state.step(&registry, PlayerAction::EndTurn).is_err());

        assert!(legal.contains(&PlayerAction::ContinueRun));
        assert!(state.step(&registry, PlayerAction::ContinueRun).is_ok());
    }

    #[test]
    fn pending_paid_choice_roundtrips_and_matches_mask() {
        let mut state = base_state();
        state.pending_paid_choice = Some(crate::rules::state::PendingPaidChoice {
            side: Side::Corp,
            cost: Cost::AnyOf(vec![Cost::Clicks(2), Cost::Credits(5)]),
            if_paid: Effect::Sequence(Vec::new()),
            if_declined: Effect::GiveTags(1),
            source_card: None,
            resume: crate::rules::state::PendingPaidChoiceResume::None,
        });
        let registry = CardRegistry::new();

        let legal = legal_actions(&state, &registry);
        assert!(legal.contains(&PlayerAction::DeclinePendingPaidChoice));
        assert!(legal.contains(&PlayerAction::AcceptPendingPaidChoice { cost_option_index: Some(0) }));
        assert!(legal.contains(&PlayerAction::AcceptPendingPaidChoice { cost_option_index: Some(1) }));
        assert!(!legal.contains(&PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }));

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);
    }

    #[test]
    fn pending_decision_roundtrips_and_matches_mask() {
        let mut state = base_state();
        state.pending_decision = Some(crate::rules::state::PendingDecision::ChooseEffect {
            chooser: Side::Corp,
            options: vec![Effect::GainCredits(Side::Corp, 2), Effect::DrawCards(Side::Corp, 2)],
            source_card: None,
            resume: crate::rules::state::PendingChoiceResume::None,
        });
        let registry = CardRegistry::new();

        let legal = legal_actions(&state, &registry);
        assert!(legal.contains(&PlayerAction::ResolvePendingChoice { option_index: 0 }));
        assert!(legal.contains(&PlayerAction::ResolvePendingChoice { option_index: 1 }));

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);
    }

    #[test]
    fn choose_cards_decision_roundtrips_and_matches_mask() {
        let mut state = base_state();
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.pending_decision = Some(crate::rules::state::PendingDecision::ChooseCards {
            side: Side::Corp,
            source: crate::dsl::CardZoneRef::OwnHq,
            filter: crate::dsl::CardFilter::Any,
            min: 0,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: Some(crate::dsl::CardZoneRef::OwnArchives),
            then: None,
            selected: Vec::new(),
            source_card: None,
            resume: crate::rules::state::PendingChoiceResume::None,
        });
        let mut registry = CardRegistry::new();
        registry.insert(hedge_fund());

        let legal = legal_actions(&state, &registry);
        assert!(legal.contains(&PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }));
        assert!(legal.contains(&PlayerAction::ConfirmCardSelection));

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);
    }

    #[test]
    fn choose_server_decision_roundtrips_and_matches_mask() {
        let mut state = base_state();
        // A `ChooseServer` is only ever parked where a run may actually
        // begin — the Runner's own action phase (`run::check_run_may_begin`).
        // Parking one anywhere else is a state the engine can no longer
        // produce, and every option would probe as illegal.
        state.phase = GamePhase::Action(Side::Runner);
        state.pending_decision = Some(crate::rules::state::PendingDecision::ChooseServer {
            chooser: Side::Runner,
            rez_cost_delta: 3,
            bonus_run_credits: 0,
            allowed_servers: None,
            on_success: None,
            source_card: None,
            resume: crate::rules::state::PendingChoiceResume::None,
        });
        let registry = CardRegistry::new();

        let legal = legal_actions(&state, &registry);
        assert!(legal.contains(&PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }));
        assert!(legal.contains(&PlayerAction::ChooseServerForPendingDecision { server: ServerId::Archives }));

        assert_roundtrips(&state, &registry);
        assert_mask_matches_legal_actions(&state, &registry);
    }

    #[test]
    fn action_space_size_is_stable() {
        // A regression guard: this constant is part of any trained model's
        // input/output shape — changing it is a breaking change that
        // should be a deliberate, visible diff, not a silent side effect
        // of touching an unrelated segment.
        // M2 added 6: `DeclinePendingPaidChoice` (+1, folded into UNIT),
        // `AcceptPendingPaidChoice` (+3: none-or-one-of-2 `AnyOf` options),
        // `ResolvePendingChoice` (+2: one-of-2 `PresentChoice` options).
        // M3 added 34: `ToggleCardSelection` (+20, `MAX_INSTALLED_PER_SIDE`
        // — Ballista/Retribution/Above the Law select among up to 20
        // installed cards, the largest zone any `PendingDecision::
        // ChooseCards` reads from), `ConfirmCardSelection` (+1),
        // `ChooseServerForPendingDecision` (+13, `ZONE_COUNT`).
        // +12 (MAX_HAND_SIZE) over M3's 764, for the new `InstallResource`
        // segment — the Runner previously had no action to install a
        // `CardType::Resource` card into the Rig at all.
        // +240 (MAX_HAND_SIZE * MAX_INSTALLED_PER_SIDE = 12 * 20) over M5's
        // 776, for the new `InstallProgramOnIce` segment (M6/hosting) — by
        // far the largest single addition in the whole plan.
        // +1 over 1024 for `PurgeVirusCounters`, the Corp's basic
        // purge action. Appended as its own trailing segment rather than
        // added to the payload-free `UNIT` block it belongs with, so that
        // every pre-existing index keeps its meaning for an already-trained
        // policy — see `PURGE_VIRUS_COUNTERS_START`'s doc comment.
        // +20 (MAX_INSTALLED_PER_SIDE) over 1025 for
        // `ChooseTriggerToResolve`, picking which of your own simultaneous
        // triggers resolves next. Appended for the same reason.
        //
        // 1045 → 1357 when `MAX_INSTALLED_PER_SIDE` went 20 → 32, because
        // real games overflowed 20 (see that constant's doc comment).
        // **Unlike every previous growth, this one SHIFTS indices rather
        // than appending** — the constant sizes 26 segments spread through
        // the space, so everything after the first of them moves. An
        // exported policy's outputs do not survive it; the model needs
        // retraining, not just a wider head. Prefer appending when there is
        // a choice; there wasn't one here.
        //
        // **Unchanged at 1357 by the `InstallId` conversion**, which is the
        // point: `RezIce`/`AdvanceCard`/`ScoreAgenda`/`TrashResource`/
        // `ActivateAbility`/`InstallProgramOnIce` were *already* encoded by
        // position into `corp.installed`/`runner.rig`, and
        // `ToggleCardSelection` by position into its zone. Only the
        // `PlayerAction` payloads changed, to carry the position the
        // encoding always used. No segment moved and no exported policy
        // needs retraining — though slots that were previously unreachable
        // (the second and third copy of a card, which the old
        // first-match-by-`CardId` lookup could never address) now are.
        assert_eq!(ActionSpace::SIZE, 1357);
    }
}
