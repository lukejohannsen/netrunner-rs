//! Currents — "This card is not trashed until another **current** is
//! played or an agenda is scored" (a Runner's event) / "…stolen" (a
//! Corp's operation). CR 8.6.6c: instead of the trash a played card takes
//! as it resolves, a lingering effect is created whose duration expires as
//! the indicated effect occurs, and the card stays in the play area,
//! active there (CR 1.8.3a).
//!
//! **Read off the subtype, never declared beside it.** Every current prints
//! the same sentence, the side deciding which agenda moment ends it, so the
//! subtype is the declaration (`CardSubtype::Current`), as `Condition` is
//! for a Corp operation that becomes a hosted counter.
//!
//! **Where a current waits:** a Corp current in `CorpState::play_area`
//! beside the lockdowns, which the Corp's turn start leaves alone
//! (`turn::begin_turn` trashes a lockdown only); a Runner current in
//! `RunnerState::in_play`, because `RunnerState::play_area` is an event
//! still resolving, which reaches the heap when its own trash comes up.
//!
//! **When the others go:** as the new current is filed, before its play is
//! announced, so a current never hears a current replace it; and after an
//! agenda's score or steal is announced, so a current hears the moment
//! that ends it — Kill Switch's "whenever an agenda is … scored" is heard
//! by the Kill Switch a steal is about to trash.

use crate::cards::CardRegistry;
use crate::dsl::{CardDefinition, CardId, CardSubtype, TrashedFrom};
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::state::{ArchivedCard, GameState, PlayedOperation, Side};

pub(crate) fn is_current(definition: &CardDefinition) -> bool {
    definition.subtypes.contains(&CardSubtype::Current)
}

fn is_current_card(registry: &CardRegistry, card: &CardId) -> bool {
    registry.get(card).is_some_and(is_current)
}

/// Files `card`, a current `side` just played, in its play area, after
/// trashing every other current in play — "until another current is
/// played", whoever played it.
pub(crate) fn file(state: &mut GameState, registry: &CardRegistry, events: &mut Vec<GameEvent>, side: Side, card: CardId) -> Result<(), RulesError> {
    trash_currents(state, registry, events, Side::Corp)?;
    trash_currents(state, registry, events, Side::Runner)?;
    let handle = state.allocate_install_id();
    let played = PlayedOperation { card, handle };
    match side {
        Side::Corp => state.corp.play_area.push(played),
        Side::Runner => state.runner.in_play.push(played),
    }
    Ok(())
}

/// An agenda was scored (`Side::Corp`) or stolen (`Side::Runner`): the
/// other side's currents expire.
pub(crate) fn agenda_moved(state: &mut GameState, registry: &CardRegistry, events: &mut Vec<GameEvent>, to: Side) -> Result<(), RulesError> {
    trash_currents(state, registry, events, to.other())
}

fn trash_currents(state: &mut GameState, registry: &CardRegistry, events: &mut Vec<GameEvent>, side: Side) -> Result<(), RulesError> {
    let gone: Vec<CardId> = match side {
        Side::Corp => {
            let (gone, stay): (Vec<_>, Vec<_>) = std::mem::take(&mut state.corp.play_area).into_iter().partition(|played| is_current_card(registry, &played.card));
            state.corp.play_area = stay;
            gone.into_iter().map(|played| played.card).collect()
        }
        Side::Runner => std::mem::take(&mut state.runner.in_play).into_iter().map(|played| played.card).collect(),
    };
    for card in gone {
        match side {
            Side::Corp => crate::rules::turn_log::file_in_archives(state, ArchivedCard::faceup(card.clone())),
            Side::Runner => state.runner.heap.push(card.clone()),
        }
        let trashed = GameEvent::CardTrashed { side, card, from: TrashedFrom::PlayArea, by: None, install: None };
        dispatcher::emit(state, registry, events, trashed)?;
    }
    Ok(())
}
