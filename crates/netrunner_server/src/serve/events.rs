//! Events: rolling gauntlets with lightweight incentives (Phase 4 §11).
//!
//! An event is the other kind of competition beside a tournament (§7
//! stage 6). A tournament is structured and synchronised — a fixed
//! start, Swiss in lockstep, decks locked — and its prize is its own
//! standings. An event is **asynchronous and rolling**: anyone enters at
//! any time, "Find event match" pairs them at once against anyone else
//! looking in the event's pool, and the prize is a **track of wins**
//! whose milestones grant a title or a badge the profile wears
//! everywhere on this server (`netrunner_store::Store::grant`, which
//! nothing else in production writes).
//!
//! **The pool is a lobby** (`protocol::event_lobby_id`): a seek there is
//! paired by §9's rules — the partner by fit, the chairs by preference,
//! the swap for two keys who have met — through `enqueue_or_pair`
//! unchanged, because its scoping by lobby id is already what a pool
//! needs. The lobby is `QueueMode::Event`: never rated, never a bot, and
//! only an entrant seeks there.
//!
//! **Wins count whole, never in a row, and a loss costs nothing.** A
//! gauntlet could mean three in a row with a loss sending the player
//! back, but an award once granted is never taken back, so a reset would
//! only make the track lie about the milestones it shows as earned;
//! losses are counted and shown. A cleared entrant keeps playing — the
//! pool of a small server would otherwise empty exactly when the last
//! climbers need opponents — and is granted nothing more, since no
//! milestone lies past the last win.
//!
//! **Held by an operator** (`ServeOptions::operators`), never by any key
//! as a tournament is: a key that could define "Champion · 1 win" would
//! make every award on the server worthless. The one piece of §12
//! brought forward.
//!
//! **One document per event, as a tournament is** (`Store::put_event`):
//! the roster is small, nothing queries across events, and the document
//! is what a daemon restarted mid-event must still hold. The awards it
//! grants are rows of the awards table (`source` = `event:<code>`),
//! which the profile already reads and `--rebuild` keeps.

use std::collections::BTreeMap;

use netrunner_identity::PublicKey;

use crate::protocol::{AwardSpec, EventEntry, EventInfo, EventSpec};

/// How long a closed event keeps its page, so the last tracks read: a
/// day.
pub(super) const CLOSED_FOR: u64 = 24 * 60 * 60;

/// An event the daemon holds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Event {
    pub(super) spec: EventSpec,
    /// The operator's key that held it.
    pub(super) held_by: PublicKey,
    pub(super) created_at: u64,
    /// Closed by the operator at this time; gone `CLOSED_FOR` later.
    pub(super) closed_at: Option<u64>,
    /// Every key's track, by rating id (`PublicKey::rating_id`).
    pub(super) entrants: BTreeMap<String, Entrant>,
}

/// One key's track.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Entrant {
    /// The name the key attached with when it entered. A label.
    pub(super) name: String,
    pub(super) wins: u32,
    pub(super) losses: u32,
    pub(super) entered_at: u64,
    pub(super) cleared_at: Option<u64>,
}

impl Event {
    pub(super) fn new(spec: EventSpec, held_by: PublicKey, now: u64) -> Event {
        Event { spec, held_by, created_at: now, closed_at: None, entrants: BTreeMap::new() }
    }

    pub(super) fn is_open(&self) -> bool {
        self.closed_at.is_none()
    }

    /// Closed long enough ago that its page is no longer kept.
    pub(super) fn is_gone(&self, now: u64) -> bool {
        self.closed_at.is_some_and(|at| now >= at.saturating_add(CLOSED_FOR))
    }

    pub(super) fn close(&mut self, now: u64) -> Result<(), String> {
        if self.closed_at.is_some() {
            return Err("this event is already closed".into());
        }
        self.closed_at = Some(now);
        Ok(())
    }

    pub(super) fn entry(&self, key: &str) -> Option<&Entrant> {
        self.entrants.get(key)
    }

    /// A key enters: its track starts at nothing. Refused once closed,
    /// and for a key already in — quitting and entering starts over.
    pub(super) fn enter(&mut self, key: &str, name: &str, now: u64) -> Result<(), String> {
        if !self.is_open() {
            return Err("this event is closed".into());
        }
        if self.entrants.contains_key(key) {
            return Err("you have entered this event already".into());
        }
        self.entrants.insert(key.to_string(), Entrant { name: name.to_string(), wins: 0, losses: 0, entered_at: now, cleared_at: None });
        Ok(())
    }

    /// A key withdraws: the track goes. What it earned stays in the
    /// awards table, which is not this document's.
    pub(super) fn quit(&mut self, key: &str) -> Result<(), String> {
        self.entrants.remove(key).map(drop).ok_or_else(|| "you have not entered this event".to_string())
    }

    /// A game in the pool ended with a winner: the winner's track moves
    /// one win, the loser's one loss, and the awards the win reaches are
    /// returned to be granted. A seat that is no longer an entrant
    /// (quit mid-game) moves nothing. Past the last win nothing is
    /// reached, since every milestone is within `wins_to_clear`
    /// (`EventSpec::problem`).
    pub(super) fn game_ended(&mut self, winner: &str, loser: &str, now: u64) -> Vec<AwardSpec> {
        if let Some(entrant) = self.entrants.get_mut(loser) {
            entrant.losses += 1;
        }
        let Some(entrant) = self.entrants.get_mut(winner) else { return Vec::new() };
        entrant.wins += 1;
        if entrant.wins == self.spec.wins_to_clear && entrant.cleared_at.is_none() {
            entrant.cleared_at = Some(now);
        }
        let wins = entrant.wins;
        self.spec.milestones.iter().filter(|milestone| milestone.wins == wins).map(|milestone| milestone.award.clone()).collect()
    }

    /// The event as `key` sees it: `looking` is how many seeks wait in
    /// its pool right now, read off the registry by the caller.
    pub(super) fn info_for(&self, id: &str, key: Option<&str>, looking: usize) -> EventInfo {
        EventInfo {
            id: id.to_string(),
            name: self.spec.name.clone(),
            format: self.spec.format,
            rule: self.spec.rule.clone(),
            wins_to_clear: self.spec.wins_to_clear,
            milestones: self.spec.milestones.clone(),
            open: self.is_open(),
            created_at: self.created_at,
            entered: self.entrants.len(),
            looking,
            mine: key.and_then(|key| self.entrants.get(key)).map(|entrant| EventEntry { wins: entrant.wins, losses: entrant.losses, entered_at: entrant.entered_at, cleared_at: entrant.cleared_at }),
        }
    }
}

/// Every event, by code.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Events(pub(super) BTreeMap<String, Event>);

impl Events {
    /// Open events first, each group oldest first; a gone event is left
    /// out (and swept by the caller).
    pub(super) fn list(&self, key: Option<&str>, now: u64, looking: impl Fn(&str) -> usize) -> Vec<EventInfo> {
        let mut all: Vec<(&String, &Event)> = self.0.iter().filter(|(_, event)| !event.is_gone(now)).collect();
        all.sort_by_key(|(id, event)| (!event.is_open(), event.created_at, (*id).clone()));
        all.into_iter().map(|(id, event)| event.info_for(id, key, looking(id))).collect()
    }

    /// The codes of the events whose page is no longer kept.
    pub(super) fn gone(&self, now: u64) -> Vec<String> {
        self.0.iter().filter(|(_, event)| event.is_gone(now)).map(|(id, _)| id.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AwardKind, Milestone};
    use netrunner_core::format::NsgFormat;
    use netrunner_identity::Identity;

    fn gauntlet() -> Event {
        let spec = EventSpec {
            name: "The Red Planet Gauntlet".into(),
            format: NsgFormat::Startup,
            rule: None,
            wins_to_clear: 3,
            milestones: vec![
                Milestone { wins: 1, award: AwardSpec { kind: AwardKind::Title, name: "Net-Slinger".into() } },
                Milestone { wins: 3, award: AwardSpec { kind: AwardKind::Badge, name: "red-planet".into() } },
            ],
        };
        Event::new(spec, Identity::from_secret([9; 32]).public_key(), 100)
    }

    #[test]
    fn a_track_climbs_by_wins_whole_and_a_loss_costs_nothing() {
        let mut event = gauntlet();
        event.enter("key:ann", "ann", 101).unwrap();
        event.enter("key:bob", "bob", 102).unwrap();
        assert_eq!(event.enter("key:ann", "ann", 103), Err("you have entered this event already".into()));
        assert_eq!(event.game_ended("key:ann", "key:bob", 110).iter().map(|award| award.name.as_str()).collect::<Vec<_>>(), ["Net-Slinger"], "the first win reaches the first milestone");
        assert_eq!(event.game_ended("key:bob", "key:ann", 111).len(), 1, "bob's first win is the title too, granted to bob; the loss moves ann's track nowhere");
        assert!(event.game_ended("key:ann", "key:bob", 112).is_empty(), "no milestone at two");
        let cleared = event.game_ended("key:ann", "key:bob", 113);
        assert_eq!(cleared.iter().map(|award| award.name.as_str()).collect::<Vec<_>>(), ["red-planet"]);
        let ann = event.entry("key:ann").unwrap();
        assert_eq!((ann.wins, ann.losses, ann.cleared_at), (3, 1, Some(113)), "whole wins with a loss between, cleared at the third");
        assert!(event.game_ended("key:ann", "key:bob", 114).is_empty(), "a cleared entrant keeps playing and is granted nothing more");
        assert_eq!(event.entry("key:ann").unwrap().wins, 4);
        let info = event.info_for("ABC", Some("key:ann"), 2);
        assert_eq!((info.entered, info.looking, info.mine.as_ref().map(|mine| mine.wins)), (2, 2, Some(4)));
        assert!(event.info_for("ABC", None, 0).mine.is_none(), "no key, no track");
    }

    #[test]
    fn quitting_drops_the_track_and_a_closed_event_takes_nobody() {
        let mut event = gauntlet();
        event.enter("key:ann", "ann", 101).unwrap();
        event.game_ended("key:ann", "key:nobody", 110);
        event.quit("key:ann").unwrap();
        assert_eq!(event.quit("key:ann"), Err("you have not entered this event".into()));
        event.enter("key:ann", "ann", 120).unwrap();
        assert_eq!(event.entry("key:ann").unwrap().wins, 0, "entering again starts over");
        assert!(event.game_ended("key:ghost", "key:ann", 121).is_empty(), "a winner who is not an entrant moves nothing");
        assert_eq!(event.entry("key:ann").unwrap().losses, 1);
        event.close(200).unwrap();
        assert_eq!(event.close(201), Err("this event is already closed".into()));
        assert_eq!(event.enter("key:cy", "cy", 202), Err("this event is closed".into()));
        assert!(!event.is_gone(200 + CLOSED_FOR - 1));
        assert!(event.is_gone(200 + CLOSED_FOR));
    }

    #[test]
    fn the_listing_puts_open_events_first_and_leaves_gone_ones_out() {
        let mut events = Events::default();
        let mut closed = gauntlet();
        closed.close(150).unwrap();
        let mut gone = gauntlet();
        gone.close(10).unwrap();
        let mut newer = gauntlet();
        newer.created_at = 120;
        events.0.insert("OLD".into(), gauntlet());
        events.0.insert("CLOSED".into(), closed);
        events.0.insert("GONE".into(), gone);
        events.0.insert("NEW".into(), newer);
        let now = 10 + CLOSED_FOR;
        let listed: Vec<String> = events.list(None, now, |id| usize::from(id == "NEW")).into_iter().map(|info| format!("{}:{}", info.id, info.looking)).collect();
        assert_eq!(listed, ["OLD:0", "NEW:1", "CLOSED:0"]);
        assert_eq!(events.gone(now), ["GONE"]);
    }
}
