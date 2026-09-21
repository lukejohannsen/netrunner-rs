//! Trashing like cards as part of installing one: CR 8.5.6, step 8.5.16c.
//!
//! "As part of the installation process, a player installing an agenda,
//! asset, upgrade, ice, or program has the opportunity to trash like
//! cards." Some of those trashes are the rules' and some are the player's:
//!
//! - **The rules':** an agenda or asset already in a remote's root when
//!   another goes in, a region already in a root when another goes in
//!   (CR 8.5.6a, 3.6.5d), and as many programs as it takes to fit a new one
//!   under the memory limit (CR 3.9.3b, 8.5.6c).
//! - **The player's:** any other card in that root, any ice protecting that
//!   server (CR 8.5.6b, and the install cost of new ice does not count what
//!   was trashed, because the trash comes first), any other program.
//!
//! **Each of the rules' trashes was a refusal.** A second region was
//! `RegionLimitExceeded`, a program over the limit was `InsufficientMemory`,
//! and a second console was `ConsoleLimitExceeded` (that one is the
//! checkpoint's, CR 3.8.5b and 10.3.1d: `checkpoint::enforce_consoles`). A
//! Runner with a full rig could not install their breaker, and the rules say
//! they may, trashing what they choose to make room. The player's trashes
//! were never offered at all.
//!
//! **A trash the rules force but leave no choice in is made unasked.** One
//! agenda or asset is in a root at most, one region likewise, and a memory
//! shortfall with one program to trash has one answer. **Where there is a
//! choice, it is asked one card at a time** (`payment::Ask::Install`), by the
//! same replay a payment is parked by (`engine::apply_action`): the question
//! unwinds the action, and the answer applies it again with the pick
//! recorded. The trashes are all picked before any is made, then made in
//! the order picked, so every question's positions are the table as the
//! action found it.
//!
//! **The player's trashes are a flag on the action** (`trash_first` on
//! `PlayerAction::InstallCard`, `InstallProgram`, `InstallProgramOnIce`),
//! never a question every install asks. It was decided that way on 21
//! September 2026: asked on every ice install onto an iced server and every
//! program install beside another program, the prompt came many times a
//! game with "none" nearly always the answer. So the install that trashes
//! first is its own entry, the engine refuses it unless it has a card to
//! trash beyond what is forced (`RulesError::NothingToTrashFirst`), and it
//! trashes at least one — so the two entries are never the same move.
//!
//! **An install by a card's text makes only the rules' trashes.** It has no
//! entry of its own to carry the flag, and no card in the pool installs
//! over something on purpose. That is a known difference from CR 8.5.6,
//! recorded under Rules Conformance B.
//!
//! **Nothing trashed here is prevented.** A player choosing among their own
//! installed cards is not a card's text trashing one (the Prevention Rule),
//! and CR 3.8.5b says so outright for the console. Each lands where CR 8.5.7
//! puts it: a Corp card in Archives as it was on the table, faceup if it was
//! rezzed; a Runner card in the heap with what it hosted.

use crate::cards::CardRegistry;
use crate::dsl::{CardDefinition, CardId, CardSubtype, CardType, CardZoneRef};
use crate::rules::action::PlayerAction;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::memory;
use crate::rules::payment::{Ask, InstallCandidate, InstallQuestion};
use crate::rules::pending_choice;
use crate::rules::run::ServerId;
use crate::rules::state::{ArchivedCard, GameState, InstallId, InstallSlot, Side};

/// The answer that says "no more": `PlayerAction::ConfirmCardSelection`,
/// offered only when nothing more is owed (`InstallQuestion::may_stop`).
/// Past every position a zone can hold, so no card's answer is ever it.
pub(crate) const STOP: u32 = u32::MAX;

/// Step 8.5.16c for a Corp card going into `server`'s `slot`: the cards
/// `trash_first` picks, then the ones the rules make a trash. Called by
/// `engine::place_corp_card` before the install cost is paid, which is what
/// keeps trashed ice out of the count (CR 8.5.6b).
pub(crate) fn before_corp_install(
    state: &mut GameState,
    registry: &CardRegistry,
    card: &CardId,
    card_def: &CardDefinition,
    server: ServerId,
    slot: InstallSlot,
    trash_first: bool,
) -> Result<Vec<GameEvent>, RulesError> {
    let forced = forced_corp_trashes(state, registry, card_def, server, slot);
    let optional: Vec<InstallId> = like_corp_cards(state, server, slot).into_iter().filter(|install| !forced.contains(install)).collect();
    let mut picked: Vec<InstallId> = Vec::new();
    if trash_first {
        if optional.is_empty() {
            return Err(RulesError::NothingToTrashFirst { card: card.clone() });
        }
        loop {
            let left: Vec<InstallId> = optional.iter().copied().filter(|install| !picked.contains(install)).collect();
            // At least one, then as many more as the Corp likes.
            let may_stop = !picked.is_empty();
            match left.as_slice() {
                [] => break,
                [only] if !may_stop => picked.push(*only),
                _ => match ask(state, Side::Corp, card, &left, may_stop, 0)? {
                    Some(install) => picked.push(install),
                    None => break,
                },
            }
        }
    }
    picked.extend(forced);
    Ok(trash(state, registry, Side::Corp, &picked))
}

/// The Corp cards the rules trash under this install (CR 8.5.6a): the
/// agenda or asset already in a remote's root under a new agenda or asset,
/// the region already in a root under a new region. At most one of each,
/// so none of it is a choice.
fn forced_corp_trashes(state: &GameState, registry: &CardRegistry, card_def: &CardDefinition, server: ServerId, slot: InstallSlot) -> Vec<InstallId> {
    if slot != InstallSlot::Root {
        return Vec::new();
    }
    let region = card_def.subtypes.contains(&CardSubtype::Region);
    let agenda_or_asset = matches!(card_def.card_type, CardType::Agenda | CardType::Asset);
    state
        .corp
        .installed
        .iter()
        .filter(|c| c.server == server && c.slot == InstallSlot::Root)
        .filter(|c| {
            registry.get(&c.card).is_some_and(|d| {
                (agenda_or_asset && matches!(d.card_type, CardType::Agenda | CardType::Asset)) || (region && d.subtypes.contains(&CardSubtype::Region))
            })
        })
        .map(|c| c.install_id)
        .collect()
}

/// The cards CR 8.5.6 calls like: every other card in the root for a root
/// install (8.5.6a), every piece of ice protecting the server for ice
/// (8.5.6b).
fn like_corp_cards(state: &GameState, server: ServerId, slot: InstallSlot) -> Vec<InstallId> {
    state.corp.installed.iter().filter(|c| c.server == server && c.slot == slot).map(|c| c.install_id).collect()
}

/// Step 8.5.16c for a program: the programs the memory limit makes the
/// Runner trash (CR 3.9.3b), and with `trash_first` any others they choose
/// (8.5.6c). Refused with `InsufficientMemory` only when trashing every
/// installed program would still not make room. Called before the install
/// cost is paid, with the program out of the grip and not yet in the rig —
/// 8.5.16a's "not yet installed", so it is never one of the choices and its
/// own memory grant, if it prints one, does not count.
pub(crate) fn before_program_install(
    state: &mut GameState,
    registry: &CardRegistry,
    card: &CardId,
    memory_cost: u32,
    trash_first: bool,
) -> Result<Vec<GameEvent>, RulesError> {
    let programs: Vec<InstallId> = installed_programs(state, registry);
    if short_after(state, registry, memory_cost, &[]) == 0 && !trash_first {
        return Ok(Vec::new());
    }
    if short_after(state, registry, memory_cost, &programs) > 0 {
        return Err(RulesError::InsufficientMemory { available: memory::available_memory(state, registry), requested: memory_cost });
    }
    // Optional only when the rules force nothing: when they do, the plain
    // install already asks which, and the two entries would be one move.
    if trash_first && (programs.is_empty() || short_after(state, registry, memory_cost, &[]) > 0) {
        return Err(RulesError::NothingToTrashFirst { card: card.clone() });
    }
    let mut picked: Vec<InstallId> = Vec::new();
    loop {
        let short = short_after(state, registry, memory_cost, &picked);
        let owed = short > 0 || (trash_first && picked.is_empty());
        if !owed && !trash_first {
            break;
        }
        let left: Vec<InstallId> = programs.iter().copied().filter(|p| !picked.contains(p)).collect();
        match left.as_slice() {
            [] => break,
            [only] if owed => picked.push(*only),
            _ => match ask(state, Side::Runner, card, &left, !owed, short)? {
                Some(install) => picked.push(install),
                None => break,
            },
        }
    }
    Ok(trash(state, registry, Side::Runner, &picked))
}

/// Whether a program of `memory_cost` could be installed at all: it fits
/// once every installed program is gone. The gate an install by a card's
/// text is offered by (`engine::can_install_runner_card_from_zone`), so a
/// selection never names a program its install would refuse.
pub(crate) fn program_could_fit(state: &GameState, registry: &CardRegistry, memory_cost: u32) -> bool {
    short_after(state, registry, memory_cost, &[]) == 0 || short_after(state, registry, memory_cost, &installed_programs(state, registry)) == 0
}

fn installed_programs(state: &GameState, registry: &CardRegistry) -> Vec<InstallId> {
    state
        .runner
        .rig
        .iter()
        .filter(|c| registry.get(&c.card).is_some_and(|d| d.card_type == CardType::Program))
        .map(|c| c.install_id)
        .collect()
}

/// How much memory a program of `memory_cost` would still be short with
/// `picked` gone, and what they hosted with them — worked out on a copy
/// rather than by subtracting costs, because a host program can change what
/// its hosted programs cost and a program can grant memory.
fn short_after(state: &GameState, registry: &CardRegistry, memory_cost: u32, picked: &[InstallId]) -> u32 {
    let balance = if picked.is_empty() { memory::memory_balance(state, registry) } else { balance_without(state, registry, picked) };
    (memory_cost as i32 - balance).max(0) as u32
}

fn balance_without(state: &GameState, registry: &CardRegistry, picked: &[InstallId]) -> i32 {
    let mut without = state.clone();
    for install in picked {
        pending_choice::remove_installed_card(&mut without, registry, Side::Runner, &CardZoneRef::OwnInstalled, *install);
    }
    memory::memory_balance(&without, registry)
}

/// Takes the next answer, or asks: `Some(install)` for a card, `None` for
/// "no more" (only when `may_stop`). The question names each candidate by
/// its position in `CardZoneRef::OwnInstalled`, the card there and its
/// install, so the prompt can show the cards without looking them up.
fn ask(state: &mut GameState, side: Side, card: &CardId, left: &[InstallId], may_stop: bool, memory_short: u32) -> Result<Option<InstallId>, RulesError> {
    let installs = pending_choice::zone_install_ids(state, side, &CardZoneRef::OwnInstalled).unwrap_or_default();
    let cards = pending_choice::zone_card_ids(state, side, &CardZoneRef::OwnInstalled, None);
    let eligible: Vec<InstallCandidate> = left
        .iter()
        .filter_map(|install| {
            let position = installs.iter().position(|i| i == install)?;
            Some(InstallCandidate { position: position as u32, card: cards.get(position)?.clone(), install: *install })
        })
        .collect();
    if state.payment_answers.is_empty() {
        let question = InstallQuestion { card: card.clone(), eligible, may_stop, memory_short };
        return Err(RulesError::PaymentChoiceNeeded { side, amount: memory_short, question: Ask::Install(question) });
    }
    let answer = state.payment_answers.remove(0);
    if answer == STOP && may_stop {
        return Ok(None);
    }
    eligible.iter().find(|c| c.position == answer).map(|c| Some(c.install)).ok_or(RulesError::CardNotEligibleForSelection(answer as usize))
}

/// Makes the trashes, in the order picked: CR 8.5.7's placement, and each
/// host's hosted cards with it.
fn trash(state: &mut GameState, registry: &CardRegistry, side: Side, picked: &[InstallId]) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for install in picked {
        let Some((card, was_public, cascade)) = pending_choice::remove_installed_card(state, registry, side, &CardZoneRef::OwnInstalled, *install) else {
            continue;
        };
        match side {
            Side::Corp => state.corp.archives.push(if was_public { ArchivedCard::faceup(card.clone()) } else { ArchivedCard::facedown(card.clone()) }),
            Side::Runner => state.runner.heap.push(card.clone()),
        }
        events.push(GameEvent::CardTrashed { side, card });
        events.extend(cascade);
    }
    events
}

/// Whether applying `action` could ask an install's question — the part of
/// `payment::could_ask` that is about installing, a necessary condition in
/// the same sense. An install that trashes first may always ask. Otherwise
/// only the memory limit asks, and only with two programs or more to choose
/// between and some program not in the rig that does not fit — the one a
/// click installs, or one a card's text finds wherever it looks.
pub(crate) fn could_ask(state: &GameState, registry: &CardRegistry, action: &PlayerAction) -> bool {
    match action {
        PlayerAction::InstallCard { trash_first: true, .. }
        | PlayerAction::InstallProgram { trash_first: true, .. }
        | PlayerAction::InstallProgramOnIce { trash_first: true, .. } => true,
        _ => memory_could_ask(state, registry),
    }
}

/// A card's text can install a program out of the grip, the heap, the
/// stack or a host's hosted cards, and the memory limit asks which programs
/// go when two or more could and one of those programs does not fit.
fn memory_could_ask(state: &GameState, registry: &CardRegistry) -> bool {
    if installed_programs(state, registry).len() < 2 {
        return false;
    }
    let free = memory::available_memory(state, registry);
    let hosted = state.runner.rig.iter().flat_map(|c| c.hosted_cards.iter());
    state
        .runner
        .grip
        .iter()
        .chain(state.runner.heap.iter())
        .chain(state.runner.stack.iter())
        .chain(hosted)
        .filter_map(|card| registry.get(card))
        .any(|d| d.card_type == CardType::Program && d.memory_cost.unwrap_or(0) > free)
}
