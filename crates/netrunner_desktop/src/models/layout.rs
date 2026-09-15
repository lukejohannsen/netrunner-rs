//! How big a card on the board is, so that the board never scrolls.
//!
//! **The board fits the window or the cards give; nothing scrolls.** A
//! static face width put the person's own hand below the fold twice
//! (Phase 7 §3 item 7, and again after §4a) and a scroll container was
//! the wrong answer both times: it is a card table, not a document. So
//! the face width is *computed* from the window and the counts on the
//! board — how many ICE the tallest server has, how many servers there
//! are — and every row is laid out to that width. A row that would
//! still be too wide (a hand of ten, a rig of twelve) overlaps its
//! cards ([`step`]) before anything shrinks further, the way a hand is
//! held.
//!
//! The chrome constants are the sizes of what is not a card: the top
//! bar, the control bar, the labels and gaps, at the sizes the screen
//! draws them. They are here rather than read off laid-out nodes so the
//! width can be computed before the first frame, and tested.

/// What the board has to make room for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// The most ICE on any one server: the tallest server column.
    pub ice: usize,
    /// Corp servers, centrals included: the widest row of columns.
    pub servers: usize,
    /// Which strip is the person's: the Runner's carries the pile
    /// buttons and is taller.
    pub human_is_runner: bool,
    /// Whether any server has a card in its root: an empty row of
    /// headers is a short row, and the cards can be larger.
    pub roots: bool,
    /// Whether the rig has anything in it, for the same reason.
    pub rig: bool,
}

/// The rail beside the board.
pub const RAIL_WIDTH: f32 = 380.0;
/// The screen root's padding, on each side.
pub const PADDING: f32 = 12.0;
/// Between the board and the rail.
pub const BODY_GAP: f32 = 10.0;
/// The top bar's height, and the control bar's.
pub const TOP_BAR: f32 = 44.0;
pub const CONTROL_BAR: f32 = 44.0;
/// Between the root's rows, and between the board's rows.
pub const ROW_GAP: f32 = 8.0;
/// A section label ("Servers", "Your hand · 5") and a chip line under
/// a card ("2 adv"), at the small text size with its leading.
pub const LABEL: f32 = 22.0;
pub const CHIPS: f32 = 20.0;
/// An ICE bar's height, with the gap under it.
pub const ICE_BAR: f32 = 30.0;
/// The identity in a strip is drawn at this fraction of the face width.
pub const IDENTITY_SCALE: f32 = 0.6;
/// The gap between cards in a row.
pub const CARD_GAP: f32 = 6.0;
/// A server column's padding and border beyond its card, each side.
pub const SERVER_CHROME: f32 = 12.0;
/// The widest and narrowest a board face is drawn. Below the floor the
/// text is unreadable and a picture is a smudge; above the cap a board
/// with little on it need not fill a large monitor with card.
pub const MIN_FACE: f32 = 72.0;
pub const MAX_FACE: f32 = 220.0;
/// The strip beside a hand: the identity plus its lines of numbers.
pub const STRIP_TEXT: f32 = 330.0;
/// A strip's height when its text is taller than its identity: the
/// name line and four lines of numbers at the body size, each of which
/// may wrap once at `STRIP_TEXT`; the Runner's has the pile buttons
/// under them. Over-estimates absorb into the gaps between rows;
/// under-estimates clip the hand, which is the thing that must not
/// happen.
pub const STRIP_CORP: f32 = 175.0;
pub const STRIP_RUNNER: f32 = 210.0;
/// A server column's header button, and the rig's group label.
pub const SERVER_HEADER: f32 = 36.0;
pub const GROUP_LABEL: f32 = 22.0;

/// The width the board column has: the window less the padding, the
/// rail and the gap.
pub fn board_width(window_width: f32) -> f32 {
    (window_width - 2.0 * PADDING - RAIL_WIDTH - BODY_GAP).max(200.0)
}

/// The height the board column has: the window less the padding, the
/// top bar, the control bar and the gaps between them.
pub fn board_height(window_height: f32) -> f32 {
    window_height - 2.0 * PADDING - TOP_BAR - CONTROL_BAR - 2.0 * ROW_GAP
}

/// The height of the board's four rows at face width `face`: the
/// opponent's strip (its identity at `IDENTITY_SCALE`, or its text,
/// whichever is taller), the Corp's servers (label, header, the ICE
/// bars of the tallest, a card, a chip line), the Runner's rig (label,
/// group label, a card, a chip line) and the person's strip beside
/// their hand (label and a card, or the strip's text). Monotone in
/// `face`, which is what lets [`face_width`] search it.
pub fn rows_height(face: f32, counts: Counts) -> f32 {
    let (opponent_strip, own_strip) = if counts.human_is_runner { (STRIP_CORP, STRIP_RUNNER) } else { (STRIP_RUNNER, STRIP_CORP) };
    let card = 1.4 * face;
    let top = (1.4 * IDENTITY_SCALE * face).max(opponent_strip);
    let servers = LABEL + SERVER_HEADER + counts.ice as f32 * ICE_BAR + if counts.roots { card + CHIPS } else { 0.0 };
    let rig = LABEL + if counts.rig { GROUP_LABEL + card + CHIPS } else { 0.0 };
    let bottom = (LABEL + card).max(own_strip);
    top + servers + rig + bottom + 3.0 * ROW_GAP
}

/// The face width the board has room for, in pixels: the largest for
/// which [`rows_height`] fits [`board_height`], capped by the servers,
/// which are columns that cannot overlap, within [`MIN_FACE`]..[`MAX_FACE`].
pub fn face_width(window: (f32, f32), counts: Counts) -> f32 {
    let (width, height) = window;
    let room = board_height(height);
    // Binary search over whole pixels: the rows grow with the face and
    // the answer is the last width that still fits.
    let (mut low, mut high) = (MIN_FACE as u32, MAX_FACE as u32);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if rows_height(mid as f32, counts) <= room {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let by_height = low as f32;
    let servers = counts.servers.max(1) as f32;
    let by_width = (board_width(width) - (servers - 1.0) * CARD_GAP) / servers - 2.0 * SERVER_CHROME;
    by_height.min(by_width).clamp(MIN_FACE, MAX_FACE).floor()
}

/// The horizontal advance from one card to the next in a row of `n`
/// cards `width` wide that must fit in `available`: the natural
/// `width + gap` when the row fits, and less — the cards overlapping,
/// like a held hand — when it does not. Never less than a fifth of a
/// card, so every card keeps an edge to click.
pub fn step(n: usize, width: f32, gap: f32, available: f32) -> f32 {
    let natural = width + gap;
    if n < 2 || (n as f32) * width + (n as f32 - 1.0) * gap <= available {
        return natural;
    }
    ((available - width) / (n as f32 - 1.0)).max(width * 0.2).min(natural)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_face_shrinks_with_the_window_and_with_ice_and_stays_in_range() {
        let none = Counts { ice: 0, servers: 4, human_is_runner: true, roots: true, rig: true };
        assert!(face_width((1280.0, 800.0), Counts { roots: false, rig: false, ..none }) > face_width((1280.0, 800.0), none), "an empty board has room for larger cards");
        let large = face_width((1920.0, 1080.0), none);
        let small = face_width((1280.0, 800.0), none);
        assert!(large > small, "{large} > {small}");
        assert!(face_width((1920.0, 1080.0), Counts { ice: 4, ..none }) < large, "four ICE bars cost card height");
        assert_eq!(face_width((800.0, 400.0), Counts { ice: 6, ..none }), MIN_FACE, "never below the floor");
        assert_eq!(face_width((4000.0, 3000.0), none), MAX_FACE, "never above the cap");
        // Nine servers across a laptop width cap it below what the
        // height would allow.
        assert!(face_width((1280.0, 1080.0), Counts { servers: 9, ..none }) < face_width((1280.0, 1080.0), none));
    }

    /// Four rows at the computed width fit the board's height, and one
    /// pixel more would not: the invariant the whole module exists for.
    #[test]
    fn four_rows_at_the_computed_width_fit_the_window_and_no_wider_would() {
        for (window, ice, runner) in [((1280.0, 800.0), 0, true), ((1280.0, 800.0), 3, false), ((1920.0, 1080.0), 5, true), ((1366.0, 768.0), 2, true), ((2560.0, 1440.0), 0, false), ((2000.0, 1250.0), 2, true)] {
            let counts = Counts { ice, servers: 5, human_is_runner: runner, roots: true, rig: ice % 2 == 0 };
            let w = face_width(window, counts);
            let room = board_height(window.1);
            assert!(rows_height(w, counts) <= room || w == MIN_FACE, "{window:?} with {ice} ICE: {} of {room}", rows_height(w, counts));
            if w < MAX_FACE && w > MIN_FACE {
                let wider = (w + 1.0).min(MAX_FACE);
                let servers_cap = (board_width(window.0) - 4.0 * CARD_GAP) / 5.0 - 2.0 * SERVER_CHROME;
                assert!(rows_height(wider, counts) > room || wider > servers_cap, "{window:?}: {w} could have been {wider}");
            }
        }
    }

    #[test]
    fn a_row_that_fits_keeps_its_gap_and_one_that_does_not_overlaps() {
        assert_eq!(step(5, 100.0, 6.0, 1000.0), 106.0);
        assert_eq!(step(1, 100.0, 6.0, 50.0), 106.0, "one card never overlaps itself");
        let s = step(10, 100.0, 6.0, 500.0);
        assert!(s < 106.0 && 9.0 * s + 100.0 <= 500.0 + 1e-3, "{s}");
        assert_eq!(step(50, 100.0, 6.0, 300.0), 20.0, "every card keeps an edge");
    }
}
