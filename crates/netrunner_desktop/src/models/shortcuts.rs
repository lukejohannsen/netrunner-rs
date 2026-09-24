//! The board's keys: which key means what, and the list a person reads.
//!
//! **A key is a button already on the screen, pressed another way.** Every
//! shortcut resolves to something the board offers by pointer — a control
//! on the bar, a button on the decision pop-up or the open menu, the score
//! area's readout, a card's two clicks — and goes through the same intent,
//! so the engine's `legal_actions` still decides whether a key does
//! anything, and a key that means nothing right now does nothing. A key
//! never reaches through an overlay, for the reason a click does not.
//!
//! **A key acts at once, as its button does.** C and D spend a click the
//! moment they are pressed. A confirmation (or a Shift) was weighed and
//! declined by the person: the bar's buttons ask nothing either, and a
//! key that asks is slower than the pointer it replaces.
//!
//! **Space is Continue, the button that names its step** — pass priority,
//! go on with the run, or breach, whichever the engine offers (never more
//! than one). It never jacks out and never ends the turn, which have keys
//! of their own: the key a person leans on must not be the one that gives
//! something up. Breaching gives nothing up, so it lost its own key (A)
//! when the three buttons became one.
//!
//! **Enter is End turn, and asks nothing.** It used to ask for a second
//! press while clicks were left, because the engine listed `EndTurn` then
//! and one stray key gave the clicks up. The engine no longer does: a
//! player with clicks must take an action (CR 5.6.2b), so End turn is
//! greyed until they are spent, and Enter does nothing until then.
//!
//! **Letters are read by what they type, not where they sit.** The screen
//! hands over the logical key, so C is the key marked C on an AZERTY or a
//! Dvorak board too, and a key held with Ctrl, Cmd or Alt is never a
//! shortcut, so the system's own chords (Cmd-Q, Ctrl-C) pass through.
//!
//! No Bevy here: the screen turns its keyboard message into a [`Key`], and
//! the model turns a [`Shortcut`] into an outcome.

use netrunner_client::board::Control;
use netrunner_core::rules::Side;

/// A key the board may read, as the screen found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A key that types a character, lowercased.
    Char(char),
    Space,
    Enter,
    Tab,
    F1,
}

/// What a key means on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// A control on the bar.
    Control(Control),
    /// The `n`th button, from zero, of the open menu, else of the decision
    /// pop-up, else — mid-encounter, where there is no pop-up — of the
    /// ICE's routes on the rail.
    Decision(usize),
    /// A side's score area, as its Agendas readout opens it.
    ScoreArea(Side),
    /// Read the card or zone under the pointer (the secondary click).
    ReadHovered,
    /// Open the menu of the card or zone under the pointer (the click).
    MenuHovered,
    /// Turn the play helper's flat panel on or off.
    PlayHelper,
    /// Turn the phase bar on or off.
    PhaseBar,
    /// Take the last move back (`Game::back`): the pop-up's and the rail's
    /// button, which is there only when there is a move to take.
    TakeBack,
    /// The Corp's "pass for the rest of this run", or stop: the rail's
    /// button, there only during a run (`Game::run_pass_label`).
    PassTheRun,
    /// Open or close the list of keys.
    Help,
    /// Open or close the timing of the turn and the run
    /// (`netrunner_client::board::timing`), as a press on the phase panel
    /// does.
    Timing,
}

/// The shortcut `key` means for a person in `side`'s chair, with Shift
/// held or not. `None` for every key that is not one.
pub fn shortcut(key: Key, shift: bool, side: Side) -> Option<Shortcut> {
    Some(match key {
        Key::Space => Shortcut::Control(Control::Continue),
        Key::Enter => Shortcut::Control(Control::EndTurn),
        Key::Tab if shift => Shortcut::ScoreArea(match side {
            Side::Corp => Side::Runner,
            Side::Runner => Side::Corp,
        }),
        Key::Tab => Shortcut::ScoreArea(side),
        Key::F1 | Key::Char('?') => Shortcut::Help,
        Key::Char('c') => Shortcut::Control(Control::GainCredit),
        Key::Char('d') => Shortcut::Control(Control::Draw),
        Key::Char('r') => Shortcut::Control(Control::RemoveTag),
        Key::Char('p') => Shortcut::Control(Control::PurgeViruses),
        Key::Char('j') => Shortcut::Control(Control::JackOut),
        Key::Char('i') => Shortcut::ReadHovered,
        Key::Char('m') => Shortcut::MenuHovered,
        Key::Char('h') => Shortcut::PlayHelper,
        Key::Char('l') => Shortcut::PhaseBar,
        Key::Char('t') => Shortcut::Timing,
        Key::Char('u') => Shortcut::TakeBack,
        Key::Char('w') => Shortcut::PassTheRun,
        Key::Char(digit @ '1'..='9') => Shortcut::Decision(digit as usize - '1' as usize),
        Key::Char(_) => return None,
    })
}

/// The list the help overlay shows, in the order a person reaches for
/// them: the keys that act, then the keys that read, then the rest.
pub const LIST: &[(&str, &str)] = &[
    ("Space", "Continue — the button says to what"),
    ("Enter", "End turn (once your clicks are spent)"),
    ("C", "Take 1 credit"),
    ("D", "Draw a card"),
    ("R", "Remove a tag (Runner)"),
    ("P", "Purge viruses (Corp)"),
    ("J", "Jack out"),
    ("U", "Take the last move back"),
    ("W", "Pass for the rest of the run, or stop (Corp)"),
    ("1 – 9", "The open menu's buttons, else the pop-up's, else a way through the ICE"),
    ("M", "Actions of the card under the pointer"),
    ("I", "Read the card under the pointer"),
    ("Tab", "Your score area (Shift: your opponent's)"),
    ("L", "Phase bar on or off"),
    ("T", "Timing of the turn and the run (or click the phase bar)"),
    ("H", "Play helper on or off"),
    ("? or F1", "This list"),
    ("Esc", "Close what is open, else leave the game"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_letter_is_a_shortcut_and_no_other_letter_is() {
        for c in 'a'..='z' {
            let listed = LIST.iter().any(|(key, _)| key.len() == 1 && key.eq_ignore_ascii_case(&c.to_string()));
            assert_eq!(shortcut(Key::Char(c), false, Side::Runner).is_some(), listed, "{c}");
        }
    }

    #[test]
    fn space_never_gives_anything_up_and_the_digits_count_from_one() {
        assert_eq!(shortcut(Key::Space, false, Side::Runner), Some(Shortcut::Control(Control::Continue)));
        assert_eq!(shortcut(Key::Enter, false, Side::Corp), Some(Shortcut::Control(Control::EndTurn)));
        assert_eq!(shortcut(Key::Char('1'), false, Side::Corp), Some(Shortcut::Decision(0)));
        assert_eq!(shortcut(Key::Char('9'), false, Side::Corp), Some(Shortcut::Decision(8)));
        assert_eq!(shortcut(Key::Char('0'), false, Side::Corp), None);
        assert_eq!(shortcut(Key::Tab, false, Side::Corp), Some(Shortcut::ScoreArea(Side::Corp)));
        assert_eq!(shortcut(Key::Tab, true, Side::Corp), Some(Shortcut::ScoreArea(Side::Runner)));
        assert_eq!(shortcut(Key::Char('?'), true, Side::Corp), Some(Shortcut::Help));
    }
}
