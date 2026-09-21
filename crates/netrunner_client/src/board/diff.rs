//! What changed between one view and the next, as the things a board
//! would animate: a card that moved, a card that turned face up, a number
//! that changed, a run that started or ended.
//!
//! **Computed, never inferred.** A client asks for the transitions between
//! two consecutive views and the masked entry of the action that took one
//! to the other, and draws them; it never works out from what is on
//! screen that "a card seems to have gone". The masked events say *what*
//! happened (`CardDrawn`, `CardInstalled`, `IceRezzed`, `AgendaScored`),
//! the two views say *where* (which install, which server) and *how much*
//! (a credit count before and after), and this module joins the two.
//!
//! **Nothing here can leak.** The entry is already masked for the viewer
//! (`Session::last_entry_for`), so any card an event names is one the
//! viewer may see; a card the mask struck moves as `card: None` — a
//! face-down install arriving at a server, a card the opponent drew —
//! and the test plays real games and checks that every card a transition
//! names is on the viewer's own board before or after.

use netrunner_core::dsl::{CardId, DamageType};
use netrunner_core::rules::{GameEvent, InstallId, InstallSlot, RunPhase, ServerId, Side};
use netrunner_core::view::ClientView;
use netrunner_session::PublicHistoryEntry;

/// A place a card can be, as the board lays places out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Hand(Side),
    Deck(Side),
    /// Archives or the heap.
    Discard(Side),
    /// A Corp server, the ICE column or the root.
    Server(ServerId, InstallSlot),
    Rig,
    /// The Corp's score area, or the Runner's stolen agendas.
    Scored(Side),
    RemovedFromGame,
    /// Somewhere the viewer cannot see into.
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// A card went from one place to another. `card` is `None` when the
    /// viewer may not know which; `install` names a card on the board at
    /// either end.
    CardMoved { card: Option<CardId>, install: Option<InstallId>, from: Zone, to: Zone },
    /// An installed card was rezzed and is now nameable.
    Revealed { install: InstallId, card: CardId },
    /// Advancement tokens on an install changed.
    Advancement { install: InstallId, from: u32, to: u32 },
    Credits { side: Side, from: u32, to: u32 },
    Clicks { side: Side, from: u32, to: u32 },
    Tags { from: u32, to: u32 },
    BadPublicity { from: u32, to: u32 },
    RunStarted { server: ServerId },
    /// The run moved: to another piece of ice, or into another phase.
    RunMoved { server: ServerId, position: usize, phase: RunPhase },
    RunEnded { server: ServerId, successful: bool },
    AgendaScored { card: CardId, points: u32 },
    AgendaStolen { card: CardId, points: u32 },
    Damage { kind: DamageType, amount: usize },
    TurnStarted { side: Side, turn: u32 },
    GameOver { winner: Side },
}

/// The transitions from `before` to `after`, `entry` being the masked
/// record of the action that took one to the other.
pub fn transitions(before: &ClientView, after: &ClientView, entry: &PublicHistoryEntry) -> Vec<Transition> {
    let mut out = Vec::new();
    let viewer = after.viewer.side();

    for event in &entry.events {
        match event {
            GameEvent::CardDrawn { side } => {
                // The viewer's own draw is the card that appeared in their
                // hand; the opponent's is a card they cannot see.
                let card = if Some(*side) == viewer { arrived_in_hand(before, after, *side) } else { None };
                out.push(Transition::CardMoved { card, install: None, from: Zone::Deck(*side), to: Zone::Hand(*side) });
            }
            GameEvent::CardInstalled { install, card, server, .. } => {
                let slot = corp_install(after, *install).map_or(InstallSlot::Root, |c| c.slot);
                let card = card.clone().or_else(|| corp_install(after, *install).and_then(|c| c.card.clone()));
                let from = origin(before, card.as_ref(), Side::Corp, viewer);
                out.push(Transition::CardMoved { card, install: Some(*install), from, to: Zone::Server(*server, slot) });
            }
            GameEvent::HardwareInstalled { card, .. } | GameEvent::ProgramInstalled { card, .. } | GameEvent::ResourceInstalled { card, .. } => {
                let install = new_rig_install(before, after, card);
                let from = origin(before, Some(card), Side::Runner, viewer);
                out.push(Transition::CardMoved { card: Some(card.clone()), install, from, to: Zone::Rig });
            }
            GameEvent::EventPlayed { card, .. } => {
                out.push(Transition::CardMoved { card: Some(card.clone()), install: None, from: Zone::Hand(Side::Runner), to: Zone::Discard(Side::Runner) });
            }
            GameEvent::OperationPlayed { card, from_archives, .. } => {
                let from = if *from_archives { Zone::Discard(Side::Corp) } else { Zone::Hand(Side::Corp) };
                out.push(Transition::CardMoved { card: Some(card.clone()), install: None, from, to: Zone::Discard(Side::Corp) });
            }
            GameEvent::CardDiscarded { side, card } => {
                out.push(Transition::CardMoved { card: Some(card.clone()), install: None, from: Zone::Hand(*side), to: Zone::Discard(*side) });
            }
            GameEvent::CardsTrashedFromHq { count } => {
                for _ in 0..*count {
                    out.push(Transition::CardMoved { card: None, install: None, from: Zone::Hand(Side::Corp), to: Zone::Discard(Side::Corp) });
                }
            }
            GameEvent::CardTrashed { side, card } => {
                let (from, install) = locate(before, card);
                out.push(Transition::CardMoved { card: Some(card.clone()), install, from, to: Zone::Discard(*side) });
            }
            GameEvent::CardTrashedFromAccess { card, .. } => {
                let (from, install) = locate(before, card);
                out.push(Transition::CardMoved { card: Some(card.clone()), install, from, to: Zone::Discard(Side::Corp) });
            }
            GameEvent::CardRemovedFromGame { card, .. } => {
                let (from, install) = locate(before, card);
                out.push(Transition::CardMoved { card: Some(card.clone()), install, from, to: Zone::RemovedFromGame });
            }
            GameEvent::CardAddedToBottomOfStack { card } => {
                let (from, install) = locate(before, card);
                out.push(Transition::CardMoved { card: Some(card.clone()), install, from, to: Zone::Deck(Side::Runner) });
            }
            GameEvent::CardMoved { install, card, from, to } => {
                let slot = corp_install(after, *install).map_or(InstallSlot::Ice, |c| c.slot);
                out.push(Transition::CardMoved { card: card.clone(), install: Some(*install), from: Zone::Server(*from, slot), to: Zone::Server(*to, slot) });
            }
            GameEvent::AgendaScored { card, agenda_points, server } => {
                let install = corp_install_by_card(before, card, *server);
                out.push(Transition::CardMoved { card: Some(card.clone()), install, from: Zone::Server(*server, InstallSlot::Root), to: Zone::Scored(Side::Corp) });
                out.push(Transition::AgendaScored { card: card.clone(), points: *agenda_points });
            }
            GameEvent::AgendaStolen { card, agenda_points } => {
                let (from, install) = locate(before, card);
                out.push(Transition::CardMoved { card: Some(card.clone()), install, from, to: Zone::Scored(Side::Runner) });
                out.push(Transition::AgendaStolen { card: card.clone(), points: *agenda_points });
            }
            GameEvent::AgendaForfeited { card } => {
                let side = if before.runner.scored_agendas.contains(card) { Side::Runner } else { Side::Corp };
                out.push(Transition::CardMoved { card: Some(card.clone()), install: None, from: Zone::Scored(side), to: Zone::Discard(Side::Corp) });
            }
            GameEvent::IceRezzed { install, card, .. } => out.push(Transition::Revealed { install: *install, card: card.clone() }),
            // Both advance events animate identically: the counter moves on
            // the board the same way whichever rule put it there. They are
            // two events for what may *trigger* on them (CR 1.18.2), and
            // this match ends in a catch-all, so the second one has to be
            // named here or a placed counter would appear with no beat.
            GameEvent::CardAdvanced { install, advancement_tokens, .. }
            | GameEvent::AdvancementCountersPlaced { install, advancement_tokens, .. } => {
                let from = corp_install(before, *install).map_or(0, |c| c.advancement_tokens);
                out.push(Transition::Advancement { install: *install, from, to: *advancement_tokens });
            }
            GameEvent::RunInitiated { server } => out.push(Transition::RunStarted { server: *server }),
            GameEvent::DamageTaken { damage_type, amount, .. } => out.push(Transition::Damage { kind: *damage_type, amount: *amount }),
            GameEvent::TurnStarted { side, .. } => out.push(Transition::TurnStarted { side: *side, turn: after.turn }),
            GameEvent::GameOver { winner } => out.push(Transition::GameOver { winner: *winner }),
            _ => {}
        }
    }

    // The run's position and phase, off the two views: a run that is on
    // in both and has moved, or one that was on and is not.
    match (&before.active_run, &after.active_run) {
        (Some(was), Some(is)) if was.server == is.server && (was.position != is.position || was.phase != is.phase) => {
            out.push(Transition::RunMoved { server: is.server, position: is.position, phase: is.phase });
        }
        (Some(was), None) => {
            let successful = matches!(was.phase, RunPhase::Success | RunPhase::AccessingCard)
                || entry.events.iter().any(|e| matches!(e, GameEvent::RunSucceeded { .. }));
            out.push(Transition::RunEnded { server: was.server, successful });
        }
        _ => {}
    }

    // The numbers, off the two views.
    for side in [Side::Corp, Side::Runner] {
        let (credits_before, credits_after, clicks_before, clicks_after) = match side {
            Side::Corp => (before.corp.credits, after.corp.credits, before.corp.clicks, after.corp.clicks),
            Side::Runner => (before.runner.credits, after.runner.credits, before.runner.clicks, after.runner.clicks),
        };
        if credits_before != credits_after {
            out.push(Transition::Credits { side, from: credits_before, to: credits_after });
        }
        if clicks_before != clicks_after {
            out.push(Transition::Clicks { side, from: clicks_before, to: clicks_after });
        }
    }
    if before.runner.tags != after.runner.tags {
        out.push(Transition::Tags { from: before.runner.tags, to: after.runner.tags });
    }
    if before.corp.bad_publicity != after.corp.bad_publicity {
        out.push(Transition::BadPublicity { from: before.corp.bad_publicity, to: after.corp.bad_publicity });
    }
    out
}

fn corp_install(view: &ClientView, id: InstallId) -> Option<&netrunner_core::rules::PublicInstalledCard> {
    view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).find(|c| c.install_id == id)
}

fn corp_install_by_card(view: &ClientView, card: &CardId, server: ServerId) -> Option<InstallId> {
    view.corp
        .servers
        .iter()
        .filter(|s| s.server == server)
        .flat_map(|s| s.root.iter())
        .find(|c| c.card.as_ref() == Some(card))
        .map(|c| c.install_id)
}

/// Where an install came from. A card installs from hand as a rule, but
/// a card's text can install from the heap, Archives or the deck; the
/// answer is where `before` showed it. The owner's hidden hand is the
/// default for a viewer who could not see it there, and the deck for an
/// owner who could see their own hand and did not.
fn origin(before: &ClientView, card: Option<&CardId>, owner: Side, viewer: Option<Side>) -> Zone {
    match card.map(|card| locate(before, card).0) {
        Some(Zone::Hidden) | None => {
            if viewer == Some(owner) { Zone::Deck(owner) } else { Zone::Hand(owner) }
        }
        Some(zone) => zone,
    }
}

/// The rig card `card` that is in `after` and was not in `before` — the
/// install just made. Two copies of one program tell apart by handle.
fn new_rig_install(before: &ClientView, after: &ClientView, card: &CardId) -> Option<InstallId> {
    after.runner.rig.iter().find(|c| c.card == *card && !before.runner.rig.iter().any(|b| b.install_id == c.install_id)).map(|c| c.install_id)
}

/// The card that is in `side`'s hand after and was not before, when the
/// viewer can see that hand. A hand is a multiset: a second copy of a
/// card already held is still a draw.
fn arrived_in_hand(before: &ClientView, after: &ClientView, side: Side) -> Option<CardId> {
    let (was, is) = match side {
        Side::Corp => (before.corp.hq_cards.as_ref()?, after.corp.hq_cards.as_ref()?),
        Side::Runner => (before.runner.grip_cards.as_ref()?, after.runner.grip_cards.as_ref()?),
    };
    let mut remaining: Vec<&CardId> = was.iter().collect();
    for card in is {
        match remaining.iter().position(|c| *c == card) {
            Some(i) => {
                remaining.swap_remove(i);
            }
            None => return Some(card.clone()),
        }
    }
    None
}

/// Where `card` was in `view`, if the viewer could see it there, with
/// its handle when it was installed. `Hidden` for a card the view did
/// not show — a face-down install the Corp just trashed, a card the
/// opponent held.
fn locate(view: &ClientView, card: &CardId) -> (Zone, Option<InstallId>) {
    if let Some(rig) = view.runner.rig.iter().find(|c| c.card == *card) {
        return (Zone::Rig, Some(rig.install_id));
    }
    for server in &view.corp.servers {
        if let Some(c) = server.ice.iter().chain(server.root.iter()).find(|c| c.card.as_ref() == Some(card)) {
            return (Zone::Server(server.server, c.slot), Some(c.install_id));
        }
    }
    if view.corp.hq_cards.as_ref().is_some_and(|hand| hand.contains(card)) {
        return (Zone::Hand(Side::Corp), None);
    }
    if view.runner.grip_cards.as_ref().is_some_and(|hand| hand.contains(card)) {
        return (Zone::Hand(Side::Runner), None);
    }
    if view.corp.scored_agendas.iter().any(|a| a.card == *card) {
        return (Zone::Scored(Side::Corp), None);
    }
    if view.runner.scored_agendas.contains(card) {
        return (Zone::Scored(Side::Runner), None);
    }
    if view.corp.archives.iter().any(|a| a.card.as_ref() == Some(card)) {
        return (Zone::Discard(Side::Corp), None);
    }
    if view.runner.heap.contains(card) {
        return (Zone::Discard(Side::Runner), None);
    }
    (Zone::Hidden, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::{BotAgent, HeuristicAgent, RandomAgent};
    use netrunner_core::cards::CardRegistry;
    use netrunner_core::rules::{GameState, Viewer};
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};
    use std::collections::HashSet;

    /// Every card the viewer's view names, anywhere.
    fn nameable(view: &ClientView) -> HashSet<CardId> {
        view.corp
            .servers
            .iter()
            .flat_map(|s| s.ice.iter().chain(s.root.iter()))
            .filter_map(|c| c.card.clone())
            .chain(view.corp.hq_cards.clone().unwrap_or_default())
            .chain(view.corp.archives.iter().filter_map(|a| a.card.clone()))
            .chain(view.corp.scored_agendas.iter().map(|a| a.card.clone()))
            .chain(view.corp.removed_from_game.iter().cloned())
            .chain(view.runner.grip_cards.clone().unwrap_or_default())
            .chain(view.runner.rig.iter().map(|c| c.card.clone()))
            .chain(view.runner.rig.iter().flat_map(|c| c.hosted_cards.iter().cloned()))
            .chain(view.runner.heap.iter().cloned())
            .chain(view.runner.scored_agendas.iter().cloned())
            .chain(view.corp.identity.iter().cloned())
            .chain(view.runner.identity.iter().cloned())
            .collect()
    }

    fn holds(view: &ClientView, zone: Zone, card: &CardId) -> bool {
        match zone {
            Zone::Hand(Side::Corp) => view.corp.hq_cards.as_ref().is_some_and(|h| h.contains(card)),
            Zone::Hand(Side::Runner) => view.runner.grip_cards.as_ref().is_some_and(|h| h.contains(card)),
            Zone::Discard(Side::Corp) => view.corp.archives.iter().any(|a| a.card.as_ref() == Some(card)),
            Zone::Discard(Side::Runner) => view.runner.heap.contains(card),
            Zone::Rig => view.runner.rig.iter().any(|c| c.card == *card),
            Zone::Scored(Side::Corp) => view.corp.scored_agendas.iter().any(|a| a.card == *card),
            Zone::Scored(Side::Runner) => view.runner.scored_agendas.contains(card),
            Zone::Server(server, slot) => view.corp.servers.iter().filter(|s| s.server == server).flat_map(|s| s.ice.iter().chain(s.root.iter())).any(|c| c.slot == slot && c.card.as_ref() == Some(card)),
            Zone::RemovedFromGame | Zone::Deck(_) | Zone::Hidden => true,
        }
    }

    /// Real games, both viewers, two seatings: every transition agrees
    /// with the view it claims to describe, and never names a card the
    /// viewer's own views do not show. Then the counts that prove the
    /// kinds were exercised.
    #[test]
    fn transitions_agree_with_the_views_and_name_only_what_the_viewer_sees() {
        let mut moved = 0;
        let mut revealed = 0;
        let mut own_draws_named = 0;
        let mut runs = 0;
        let mut scored = 0;
        for seed in 0..6u64 {
            let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
            let registry: CardRegistry = crate::decks::sample_deck_registry();
            let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
            let mut corp: Box<dyn BotAgent> = if seed % 2 == 0 { Box::new(HeuristicAgent::new(Side::Corp, seed)) } else { Box::new(RandomAgent::new(seed)) };
            let mut runner = RandomAgent::new(seed + 7);
            let viewers = [Viewer::Player(Side::Corp), Viewer::Player(Side::Runner)];
            let mut before: Vec<ClientView> = viewers.iter().map(|v| session.view_for(*v)).collect();
            loop {
                match session.step() {
                    SessionStep::Awaiting { side, view } => {
                        let action = match side {
                            Side::Corp => corp.select_action(&view, &registry),
                            Side::Runner => runner.select_action(&view, &registry),
                        };
                        session.submit(action).unwrap();
                        for (i, viewer) in viewers.iter().enumerate() {
                            let after = session.view_for(*viewer);
                            let entry = session.last_entry_for(*viewer).unwrap();
                            let ts = transitions(&before[i], &after, &entry);
                            let seen: HashSet<CardId> = nameable(&before[i]).union(&nameable(&after)).cloned().collect();
                            for t in &ts {
                                match t {
                                    Transition::CardMoved { card, install, from, to } => {
                                        moved += 1;
                                        if let Some(card) = card {
                                            assert!(seen.contains(card), "seed {seed} {viewer:?}: {t:?} names a card the viewer never saw");
                                            assert!(holds(&after, *to, card), "seed {seed} {viewer:?}: {t:?} but the card is not there after");
                                            // A departure is checked where the viewer could see
                                            // the place: their own hand, and the open board.
                                            let visible_from = match from {
                                                Zone::Hand(side) => Some(*side) == viewer.side(),
                                                // A face-down root card scored or trashed was never
                                                // shown; the server itself was.
                                                Zone::Server(server, slot) => {
                                                    assert!(
                                                        before[i].corp.servers.iter().any(|s| s.server == *server && s.ice.iter().chain(s.root.iter()).any(|c| c.slot == *slot)),
                                                        "seed {seed} {viewer:?}: {t:?} but that server slot was empty before"
                                                    );
                                                    false
                                                }
                                                Zone::Rig | Zone::Scored(_) => true,
                                                _ => false,
                                            };
                                            if visible_from {
                                                assert!(holds(&before[i], *from, card), "seed {seed} {viewer:?}: {t:?} but the card was not there before");
                                            }
                                            if *from == Zone::Deck(Side::Corp) || *from == Zone::Deck(Side::Runner) {
                                                own_draws_named += 1;
                                            }
                                        }
                                        if let (Some(id), Zone::Server(server, slot)) = (install, to) {
                                            let there = after.corp.servers.iter().filter(|s| s.server == *server).flat_map(|s| s.ice.iter().chain(s.root.iter())).any(|c| c.install_id == *id && c.slot == *slot);
                                            assert!(there, "seed {seed} {viewer:?}: {t:?} but no such install after");
                                        }
                                        if let (Some(id), Zone::Rig) = (install, to) {
                                            assert!(after.runner.rig.iter().any(|c| c.install_id == *id), "seed {seed}: {t:?}");
                                        }
                                    }
                                    Transition::Revealed { install, card } => {
                                        revealed += 1;
                                        let c = corp_install(&after, *install).expect("a rezzed install is on the board");
                                        assert!(c.rezzed && c.card.as_ref() == Some(card), "seed {seed} {viewer:?}: {t:?}");
                                    }
                                    Transition::Advancement { install, to, .. } => {
                                        assert_eq!(corp_install(&after, *install).map(|c| c.advancement_tokens), Some(*to), "seed {seed}: {t:?}");
                                    }
                                    Transition::Credits { side, to, .. } => {
                                        let now = if *side == Side::Corp { after.corp.credits } else { after.runner.credits };
                                        assert_eq!(now, *to);
                                    }
                                    Transition::Clicks { side, to, .. } => {
                                        let now = if *side == Side::Corp { after.corp.clicks } else { after.runner.clicks };
                                        assert_eq!(now, *to);
                                    }
                                    Transition::Tags { to, .. } => assert_eq!(after.runner.tags, *to),
                                    Transition::BadPublicity { to, .. } => assert_eq!(after.corp.bad_publicity, *to),
                                    Transition::RunStarted { server } => {
                                        runs += 1;
                                        assert_eq!(after.active_run.as_ref().map(|r| r.server), Some(*server));
                                    }
                                    Transition::RunMoved { position, phase, .. } => {
                                        let run = after.active_run.as_ref().expect("a moved run is on");
                                        assert_eq!((run.position, run.phase), (*position, *phase));
                                    }
                                    Transition::RunEnded { .. } => assert!(after.active_run.is_none()),
                                    Transition::AgendaScored { card, .. } => {
                                        scored += 1;
                                        assert!(after.corp.scored_agendas.iter().any(|a| a.card == *card));
                                    }
                                    Transition::AgendaStolen { card, .. } => assert!(after.runner.scored_agendas.contains(card)),
                                    Transition::TurnStarted { turn, .. } => assert_eq!(*turn, after.turn),
                                    Transition::Damage { .. } | Transition::GameOver { .. } => {}
                                }
                            }
                            before[i] = after;
                        }
                    }
                    SessionStep::Applied { .. } => {}
                    SessionStep::Ended { .. } => break,
                    SessionStep::Stalled(reason) => panic!("seed {seed}: {reason:?}"),
                }
            }
        }
        assert!(moved > 200, "{moved} card movements over six games");
        assert!(revealed > 0, "a rez was seen");
        assert!(own_draws_named > 50, "{own_draws_named}: a viewer's own draws name the card");
        assert!(runs > 20, "{runs} runs");
        assert!(scored > 0, "the heuristic Corp scores");
    }

    /// A draw is a multiset difference: a second copy of a card already
    /// in hand is the card drawn.
    #[test]
    fn a_second_copy_drawn_is_named() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let session = Session::new(state, registry, Seat::External, Seat::External);
        let mut before = session.view_for(Side::Corp);
        let mut after = before.clone();
        let hand = before.corp.hq_cards.clone().unwrap();
        let first = hand[0].clone();
        before.corp.hq_cards = Some(vec![first.clone(), hand[1].clone()]);
        after.corp.hq_cards = Some(vec![first.clone(), hand[1].clone(), first.clone()]);
        assert_eq!(arrived_in_hand(&before, &after, Side::Corp), Some(first));
        assert_eq!(arrived_in_hand(&after, &before, Side::Corp), None, "a hand that shrank drew nothing");
    }
}
