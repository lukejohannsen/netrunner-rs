//! Which sound a moment makes, as a plain enum the audio plugin plays
//! (`crate::audio`), and which moments of a match make one.
//!
//! **A sound is read off the board's `Transition`s**, the same record the
//! highlights are drawn from (`netrunner_client::board::diff`), never off
//! what a system sees on screen — the rule the animations follow
//! (AGENTS.md §5). So the opponent's draw is heard as a card taken even
//! though its face is hidden, and a card a card's text moves sounds like
//! one a click moved: the sound is about where a card went, not who sent
//! it.
//!
//! The words are the person's (1 October 2026): a card played to the
//! table or the discard is *placed*, a card taken from anywhere into a
//! hand is a *shove or slide*, any credits taken are *chips*, a toggle
//! button *toggles*, a selection that changes the window *switches*, the
//! way back is *Back*, a card added to a deck is its own click, and a
//! game opens with a shuffle and then a fan.

use netrunner_client::board::{Transition, Zone};

/// A sound the client makes. Each is a set of recordings, one drawn at
/// random per play (`crate::audio`), so a run of draws is not one sample
/// played five times.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    /// A game starts: the shuffle, then a fan once the shuffle is done.
    Opening,
    /// A card played to the table — a server, the rig, the score area —
    /// or to a discard pile.
    Place,
    /// A card taken into a hand, from anywhere.
    Take,
    /// Credits taken, by either side.
    Chips,
    /// A toggle button: a setting turned on or off, a choice of pills.
    Toggle,
    /// A selection that changes the window: one screen to another.
    Switch,
    /// The way back: a Back button, Escape, or a screen left for the one
    /// it was opened from.
    Back,
    /// A card added to a deck in the deck builder.
    DeckAdd,
}

/// How many of one kind of sound a single action makes, at most. A draw
/// of five is five cards, but five slides a beat apart is a drum roll;
/// three says "several" and stops.
pub const MOST_OF_A_KIND: usize = 3;

/// The sounds `transitions` make, in the order they happened, at most
/// [`MOST_OF_A_KIND`] of each.
///
/// A card whose destination is somewhere nobody draws (removed from the
/// game, shuffled into a deck, a zone the viewer cannot see) makes none:
/// the person named the table, the discard and the hand, and a sound
/// with nothing moving on screen is a sound nobody can place. Credits
/// spent make none either — the person asked for the taking of credits.
pub fn cues(transitions: &[Transition]) -> Vec<Sfx> {
    let mut out = Vec::new();
    for transition in transitions {
        let sound = match transition {
            Transition::CardMoved { to: Zone::Hand(_), .. } => Some(Sfx::Take),
            Transition::CardMoved { to: Zone::Server(..) | Zone::Rig | Zone::Scored(_) | Zone::Discard(_), .. } => Some(Sfx::Place),
            Transition::Credits { from, to, .. } if to > from => Some(Sfx::Chips),
            _ => None,
        };
        if let Some(sound) = sound
            && out.iter().filter(|s| **s == sound).count() < MOST_OF_A_KIND
        {
            out.push(sound);
        }
    }
    out
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
    fn a_draw_of_five_is_three_slides_and_somewhere_undrawn_is_silent() {
        let draws: Vec<Transition> = (0..5).map(|_| moved(Zone::Deck(Side::Runner), Zone::Hand(Side::Runner))).collect();
        assert_eq!(cues(&draws), vec![Sfx::Take; MOST_OF_A_KIND]);
        assert!(cues(&[moved(Zone::Discard(Side::Runner), Zone::RemovedFromGame), moved(Zone::Hand(Side::Corp), Zone::Deck(Side::Corp))]).is_empty());
    }
}
