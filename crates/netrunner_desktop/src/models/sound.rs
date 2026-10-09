//! Which sound a moment makes, as a plain enum the audio plugin plays
//! (`crate::audio`), and which moments of a match make one.
//!
//! **A sound is read off the record, never off the screen** — the rule
//! the animations follow (AGENTS.md §5). Where a thing went and what a
//! number became are the board's `Transition`s
//! (`netrunner_client::board::diff`), so the opponent's draw is heard as
//! a card arriving even though its face is hidden. What *happened* — a
//! run begun, ice met, a card rezzed, an agenda taken — is the entry's
//! own events ([`event_cues`]), because a run's events reach the model a
//! beat at a time, ahead of the view the transitions are computed from
//! (`models::pace`), and a sound cued off the transitions would trail
//! the picture it belongs to.
//!
//! **The theme is the setting's, not the table's** (the person's choice,
//! 4 October 2026, replacing the recorded paper cards and poker chips of
//! 1 October): a card arriving is data, an install locks in, credits
//! tick, a panel slides, and a run jacks in and out. The moments are
//! still the ones the person named first — a card to the table or a
//! discard, a card into a hand, credits taken, a toggle, a change of
//! screen, the way back, a card added to a deck — with the run, the rez,
//! damage, a tag, an agenda, the turn and the end of the match beside
//! them.

use netrunner_client::board::trail;
use netrunner_client::board::{Transition, Zone};
use netrunner_core::rules::{GameEvent, Side};

/// A sound the client makes. Each is a set of recordings, one drawn at
/// random per play (`crate::audio`), so a run of draws is not one sample
/// played five times.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    /// A game starts: the connection comes up.
    Opening,
    /// A card played to the table — a server, the rig — or to a discard
    /// pile.
    Place,
    /// A card taken into a hand, from anywhere.
    Take,
    /// Credits taken, by either side.
    Chips,
    /// A button with no sound of its own was pressed.
    Click,
    /// A toggle button: a setting turned on or off, a choice of pills.
    Toggle,
    /// A selection that changes the window: one screen to another.
    Switch,
    /// The way back: a Back button, Escape, or a screen left for the one
    /// it was opened from.
    Back,
    /// A card added to a deck in the deck builder.
    DeckAdd,
    /// Something opened over the screen: a sheet, a menu, the options,
    /// the timing chart, a drop-down's list, the phase panel.
    Open,
    /// The same, closing.
    Close,
    /// A run begins.
    JackIn,
    /// The Runner jacks out.
    JackOut,
    /// The run approaches a piece of ice.
    Approach,
    /// The run encounters it.
    Encounter,
    /// The run is declared successful.
    RunSuccess,
    /// The run is ended by a card.
    RunEnded,
    /// A card is turned faceup.
    Rez,
    /// The Runner suffers damage, of any kind: net, meat or core.
    Damage,
    /// The Runner takes a tag.
    Tag,
    /// A card is advanced, or has advancement counters placed on it.
    Advance,
    /// The Corp scores an agenda.
    Score,
    /// The Runner steals one.
    Steal,
    /// A turn begins.
    TurnStart,
    /// The match ends, and the person won it.
    Win,
    /// The match ends, and they did not.
    Loss,
}

impl Sfx {
    pub const ALL: [Sfx; 26] = [
        Sfx::Opening,
        Sfx::Place,
        Sfx::Take,
        Sfx::Chips,
        Sfx::Click,
        Sfx::Toggle,
        Sfx::Switch,
        Sfx::Back,
        Sfx::DeckAdd,
        Sfx::Open,
        Sfx::Close,
        Sfx::JackIn,
        Sfx::JackOut,
        Sfx::Approach,
        Sfx::Encounter,
        Sfx::RunSuccess,
        Sfx::RunEnded,
        Sfx::Rez,
        Sfx::Damage,
        Sfx::Tag,
        Sfx::Advance,
        Sfx::Score,
        Sfx::Steal,
        Sfx::TurnStart,
        Sfx::Win,
        Sfx::Loss,
    ];
}

/// How many of one kind of sound a single action makes, at most. A draw
/// of five is five cards, but five blips a beat apart is a drum roll;
/// three says "several" and stops.
pub const MOST_OF_A_KIND: usize = 3;

fn capped(sounds: impl Iterator<Item = Sfx>) -> Vec<Sfx> {
    let mut out = Vec::new();
    for sound in sounds {
        if out.iter().filter(|s| **s == sound).count() < MOST_OF_A_KIND {
            out.push(sound);
        }
    }
    out
}

/// The sounds `transitions` make, in the order they happened, at most
/// [`MOST_OF_A_KIND`] of each: where a card went and what a number
/// became.
///
/// A card whose destination is somewhere nobody draws (removed from the
/// game, shuffled into a deck, a zone the viewer cannot see) makes none:
/// the person named the table, the discard and the hand, and a sound
/// with nothing moving on screen is a sound nobody can place. Credits
/// spent make none either — the person asked for the taking of credits.
/// **A card into a score area makes none here**, because the score or
/// the steal that put it there is heard instead ([`event_cues`]), and so
/// is everything else a `Transition` restates from an event: a run's
/// start and end, a rez, damage, the turn.
pub fn cues(transitions: &[Transition]) -> Vec<Sfx> {
    capped(transitions.iter().filter_map(|transition| match transition {
        Transition::CardMoved { to: Zone::Hand(_), .. } => Some(Sfx::Take),
        Transition::CardMoved { to: Zone::Server(..) | Zone::Rig | Zone::Discard(_), .. } => Some(Sfx::Place),
        Transition::Credits { from, to, .. } if to > from => Some(Sfx::Chips),
        Transition::Tags { from, to } if to > from => Some(Sfx::Tag),
        // Off the transition, which every way of adding a counter makes
        // (an advance, counters a card's text places), rather than off
        // one of the three events behind it.
        Transition::Advancement { from, to, .. } if to > from => Some(Sfx::Advance),
        _ => None,
    }))
}

/// The sounds `events` make, in order, at most [`MOST_OF_A_KIND`] of
/// each: what happened, as the engine recorded it.
///
/// Called with a run's events as each beat releases them and with
/// whatever an applied action's record holds after its last beat
/// ([`after_the_beats`]), so every event is heard once, at the moment
/// the board shows it. Passing an ice and reaching the server are
/// silent: the trail moves, and the next thing to happen has a sound.
pub fn event_cues(events: &[GameEvent]) -> Vec<Sfx> {
    capped(events.iter().filter_map(|event| match event {
        GameEvent::RunInitiated { .. } => Some(Sfx::JackIn),
        GameEvent::IceApproached { .. } => Some(Sfx::Approach),
        GameEvent::IceEncountered { .. } => Some(Sfx::Encounter),
        GameEvent::RunSucceeded { .. } => Some(Sfx::RunSuccess),
        GameEvent::RunJackedOut { .. } => Some(Sfx::JackOut),
        GameEvent::RunEndedByEffect { .. } => Some(Sfx::RunEnded),
        GameEvent::IceRezzed { .. } => Some(Sfx::Rez),
        GameEvent::DamageTaken { .. } => Some(Sfx::Damage),
        GameEvent::AgendaScored { .. } => Some(Sfx::Score),
        GameEvent::AgendaStolen { .. } => Some(Sfx::Steal),
        GameEvent::TurnStarted { .. } => Some(Sfx::TurnStart),
        _ => None,
    }))
}

/// The events of an applied action that no beat carried: everything
/// after its last step (`trail::is_step`), which is how
/// `models::pace::Pacer` splits a record — each group up to and
/// including a step is a beat, and the rest rides with the message.
pub fn after_the_beats(events: &[GameEvent]) -> &[GameEvent] {
    let carried = events.iter().rposition(trail::is_step).map_or(0, |last| last + 1);
    &events[carried..]
}

/// The end of the match, from `chair`. A tie — time called with the
/// points even — is neither sound: the table closes.
pub fn ending(winner: Option<Side>, chair: Side) -> Sfx {
    match winner {
        Some(winner) if winner == chair => Sfx::Win,
        Some(_) => Sfx::Loss,
        None => Sfx::Close,
    }
}

/// The sound of going from `before` things open to `after`: one more is
/// an opening, one fewer a closing, and one swapped for another is
/// neither.
pub fn opened_or_closed(before: usize, after: usize) -> Option<Sfx> {
    match after.cmp(&before) {
        std::cmp::Ordering::Greater => Some(Sfx::Open),
        std::cmp::Ordering::Less => Some(Sfx::Close),
        std::cmp::Ordering::Equal => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{InstallSlot, ServerId, Side};

    fn moved(from: Zone, to: Zone) -> Transition {
        Transition::CardMoved { card: None, install: None, from, to }
    }

    #[test]
    fn a_card_to_the_table_or_a_discard_is_placed_and_one_into_a_hand_is_taken() {
        let transitions = [
            moved(Zone::Hand(Side::Corp), Zone::Server(ServerId::Hq, InstallSlot::Root)),
            moved(Zone::Hand(Side::Runner), Zone::Rig),
            moved(Zone::Hand(Side::Runner), Zone::Discard(Side::Runner)),
            moved(Zone::Discard(Side::Corp), Zone::Hand(Side::Corp)),
            moved(Zone::Rig, Zone::Hand(Side::Runner)),
        ];
        assert_eq!(cues(&transitions), vec![Sfx::Place, Sfx::Place, Sfx::Place, Sfx::Take, Sfx::Take]);
    }

    #[test]
    fn credits_taken_are_chips_and_credits_spent_are_silent() {
        let gained = Transition::Credits { side: Side::Corp, from: 5, to: 8 };
        let spent = Transition::Credits { side: Side::Runner, from: 5, to: 2 };
        assert_eq!(cues(&[gained, spent]), vec![Sfx::Chips]);
    }

    #[test]
    fn a_draw_of_five_is_three_arrivals_and_somewhere_undrawn_is_silent() {
        let draws: Vec<Transition> = (0..5).map(|_| moved(Zone::Deck(Side::Runner), Zone::Hand(Side::Runner))).collect();
        assert_eq!(cues(&draws), vec![Sfx::Take; MOST_OF_A_KIND]);
        assert!(cues(&[moved(Zone::Discard(Side::Runner), Zone::RemovedFromGame), moved(Zone::Hand(Side::Corp), Zone::Deck(Side::Corp))]).is_empty());
    }

    #[test]
    fn a_tag_taken_alarms_and_a_tag_removed_does_not() {
        assert_eq!(cues(&[Transition::Tags { from: 0, to: 2 }]), vec![Sfx::Tag]);
        assert!(cues(&[Transition::Tags { from: 2, to: 1 }]).is_empty());
    }

    #[test]
    fn a_counter_added_is_an_advance_however_it_got_there_and_one_removed_is_silent() {
        let install = netrunner_core::rules::InstallId(4);
        assert_eq!(cues(&[Transition::Advancement { install, from: 1, to: 2 }]), vec![Sfx::Advance]);
        assert!(cues(&[Transition::Advancement { install, from: 2, to: 0 }]).is_empty());
    }

    /// What an event says is not said again by the transition that
    /// restates it: an agenda's move into the score area, a run's start
    /// and end, the turn.
    #[test]
    fn what_an_event_is_heard_for_its_transition_is_silent_for() {
        let restated = [
            moved(Zone::Server(ServerId::Remote(0), InstallSlot::Root), Zone::Scored(Side::Corp)),
            Transition::RunStarted { server: ServerId::Hq },
            Transition::RunEnded { server: ServerId::Hq, successful: true },
            Transition::TurnStarted { side: Side::Corp, turn: 2 },
            Transition::GameOver { winner: Side::Corp },
        ];
        assert!(cues(&restated).is_empty());
    }

    #[test]
    fn a_run_is_heard_from_its_events_and_passing_ice_is_silent() {
        let hq = ServerId::Hq;
        let run = [
            GameEvent::RunInitiated { server: hq },
            GameEvent::ServerApproached { server: hq },
            GameEvent::RunSucceeded { server: hq },
        ];
        assert_eq!(event_cues(&run), vec![Sfx::JackIn, Sfx::RunSuccess]);
        assert_eq!(event_cues(&[GameEvent::RunJackedOut { server: hq }]), vec![Sfx::JackOut]);
        assert_eq!(event_cues(&[GameEvent::RunEndedByEffect { server: hq }]), vec![Sfx::RunEnded]);
        assert_eq!(event_cues(&[GameEvent::TurnStarted { side: Side::Runner, clicks: 4 }]), vec![Sfx::TurnStart]);
    }

    /// The beats carry everything up to an action's last step, so the
    /// message that follows them is heard only for what comes after: a
    /// run is jacked into once, whichever of the two carried it.
    #[test]
    fn an_event_is_heard_by_its_beat_or_by_its_message_never_both() {
        let hq = ServerId::Hq;
        let begun = GameEvent::RunInitiated { server: hq };
        let reached = GameEvent::ServerApproached { server: hq };
        let turn = GameEvent::TurnStarted { side: Side::Corp, clicks: 3 };
        // A step after it: the run's start rode in the beat.
        let with_a_step = [begun.clone(), reached.clone(), turn.clone()];
        assert_eq!(after_the_beats(&with_a_step), &[turn]);
        // No step at all: nothing was a beat, and the message is heard whole.
        let parked = [begun.clone()];
        assert_eq!(event_cues(after_the_beats(&parked)), vec![Sfx::JackIn]);
        assert!(after_the_beats(&[begun, reached]).is_empty());
    }

    #[test]
    fn the_end_is_heard_from_the_persons_chair() {
        assert_eq!(ending(Some(Side::Runner), Side::Runner), Sfx::Win);
        assert_eq!(ending(Some(Side::Corp), Side::Runner), Sfx::Loss);
        assert_eq!(ending(None, Side::Runner), Sfx::Close, "a tie is neither");
    }

    #[test]
    fn one_more_open_is_an_opening_and_one_swapped_is_silent() {
        assert_eq!(opened_or_closed(0, 1), Some(Sfx::Open));
        assert_eq!(opened_or_closed(2, 1), Some(Sfx::Close));
        assert_eq!(opened_or_closed(1, 1), None);
    }
}
