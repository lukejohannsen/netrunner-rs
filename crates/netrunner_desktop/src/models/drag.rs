//! Picking a card up: the press, the travel, and what the release means.
//!
//! **A press on a hand card is not yet a click.** Every other target acts
//! on the press — a tile, a header, a pile opens its menu at once — but a
//! hand card can also be dragged into a new place in the hand, and a drag
//! begins with a press that has not been released yet. So a hand card's
//! press is *armed* here, and the release decides: still, and it is the
//! click that opens the card's menu ([`Release::Click`]); moved, and it is
//! a drag ([`Release::Moved`]). Opening the menu on the press and closing
//! it again when the pointer moved was the alternative, and it flashed a
//! panel over the board on every drag.
//!
//! **Travel, not time, starts a drag** ([`THRESHOLD`]): a person who holds
//! still is clicking however long they hold, and a person who has moved
//! six pixels is dragging however fast they did it. A timer would have made
//! a slow click into a drag and a fast drag into a click.
//!
//! No Bevy: the screen hands over the pointer's position in logical
//! pixels and reads back what the release meant.

/// How far the pointer travels before a press becomes a drag, in logical
/// pixels. Small enough that a deliberate pull is a drag at once, large
/// enough that a hand resting on a mouse does not drag by accident.
pub const THRESHOLD: f32 = 6.0;

/// A press being held on something draggable.
#[derive(Debug, Clone, PartialEq)]
pub struct Drag<T> {
    /// What was pressed: the caller's own handle on it (a hand slot).
    pub what: T,
    /// Where the press landed, and where the pointer is now.
    pub from: (f32, f32),
    pub now: (f32, f32),
    /// Whether the pointer has travelled far enough to be a drag. Once
    /// set it stays set: a drag that comes back to where it started is
    /// still a drag, and releasing there means "put it back", not "click".
    pub dragging: bool,
}

/// What a release meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Release {
    /// The pointer never travelled: the press was a click.
    Click,
    /// The pointer travelled: the press was a drag, released here.
    Moved,
}

impl<T> Drag<T> {
    /// Arms a press at `at`.
    pub fn press(what: T, at: (f32, f32)) -> Self {
        Drag { what, from: at, now: at, dragging: false }
    }

    /// The pointer moved to `at`. Returns whether this is the move that
    /// started the drag, so the screen can redraw once rather than every
    /// frame the pointer moves.
    pub fn moved(&mut self, at: (f32, f32)) -> bool {
        self.now = at;
        if self.dragging {
            return false;
        }
        let (dx, dy) = (at.0 - self.from.0, at.1 - self.from.1);
        self.dragging = dx.hypot(dy) >= THRESHOLD;
        self.dragging
    }

    /// What the release means, and where the pointer was.
    pub fn release(&self) -> (Release, (f32, f32)) {
        (if self.dragging { Release::Moved } else { Release::Click }, self.now)
    }
}

/// Where a card dropped at `x` belongs in a row of `slots` — each a card's
/// centre and half-width, left to right — as an index to insert *before*,
/// counting the dragged card's own slot out. A drop past the last card is
/// the end of the row.
///
/// The centres come from the laid-out faces, so an overlapped hand (a row
/// too wide for the board steps its cards, `layout::step`) reads the same
/// as a spread one.
pub fn insert_at(slots: &[f32], x: f32) -> usize {
    slots.iter().position(|centre| x < *centre).unwrap_or(slots.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_press_is_a_click_and_a_travelled_one_is_a_drag() {
        let mut drag = Drag::press(3usize, (100.0, 100.0));
        assert_eq!(drag.release(), (Release::Click, (100.0, 100.0)));
        assert!(!drag.moved((103.0, 101.0)), "a wobble is not a drag");
        assert_eq!(drag.release().0, Release::Click);
        assert!(drag.moved((100.0, 110.0)), "ten pixels is");
        assert_eq!(drag.what, 3);
        assert_eq!(drag.release(), (Release::Moved, (100.0, 110.0)));
        assert!(!drag.moved((120.0, 120.0)), "the drag had already begun");
        // Back where it started is still a drag: the release means "put
        // it back", not "open the menu".
        drag.moved((100.0, 100.0));
        assert_eq!(drag.release(), (Release::Moved, (100.0, 100.0)));
    }

    #[test]
    fn a_drop_lands_before_the_first_card_it_is_left_of() {
        let slots = [100.0, 200.0, 300.0];
        assert_eq!(insert_at(&slots, 0.0), 0);
        assert_eq!(insert_at(&slots, 99.0), 0);
        assert_eq!(insert_at(&slots, 150.0), 1);
        assert_eq!(insert_at(&slots, 250.0), 2);
        assert_eq!(insert_at(&slots, 400.0), 3, "past the last card is the end of the row");
        assert_eq!(insert_at(&[], 400.0), 0);
    }
}
