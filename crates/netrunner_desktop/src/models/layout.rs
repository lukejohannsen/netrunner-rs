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
//!
//! **The board is the table seen from the person's chair.** Null Signal
//! Games' setup puts the Corp's deck (R&D) at the left of the Corp's
//! area with Archives to its left and the identity (HQ) to its right,
//! remotes beyond, ice in a column out toward the Runner and a server's
//! root between its ice and the Corp. The Corp sees that from their
//! chair — Archives, R&D, HQ, remotes, left to right, ice climbing away
//! from them — and the Runner sees the ice coming down toward them,
//! outermost nearest. [`column_top_down`] and [`ice_top_down`] are that
//! rule, so the screen never decides an order itself. **Left to right is
//! one order for both chairs** (`netrunner_client::board::table_servers`):
//! the Runner once saw the row mirrored, remotes first, and every remote
//! the Corp made moved the three centrals a column over; the person asked
//! for them to stay put, so a Runner reads the Corp's own order too. The Runner's installed cards have no position in the
//! rules ("does not matter" — the learn-to-play guide), so the rig
//! keeps its three groups in the one order for both chairs.
//!
//! **A server column is a plate and a stack of tiles.** The plate — the
//! server's name and count, and the box its picture goes in — sits on
//! the Corp's edge of the table, and every card in the root and every
//! piece of ice is the same small block — a title and a number — in the
//! column out toward the Runner, never a card face: a root drawn as a
//! face was the one thing on the board at card size that was not in a
//! hand or a rig, and it cost the whole board its card width. A tile
//! opens the card's sheet when read, so nothing is lost but the picture.
//!
//! **The middle of the table is the ICE field, and it is the only thing
//! that grows.** Between the plates and the run lane the columns get
//! whatever height the fixed rows leave ([`field_height`]); a tile is as
//! tall as its share of it allows and overlaps past a floor
//! ([`tile_stack`]), so the Corp's ICE and the Runner's installs never
//! move a card: the rig's row is reserved empty or not. From the Corp's
//! chair the plates are at the bottom, next to the Corp, and the ICE
//! climbs; from the Runner's they are at the top and it comes down.
//!
//! **The far side is smaller.** The opponent's strip, hand and area are
//! drawn at [`OPPONENT_SCALE`] of the person's own width — the Runner
//! sees the Corp's servers smaller and their own rig at full size, the
//! Corp the reverse — and both hands show a [`PEEK`] of each card, the
//! person's own lifting out whole when hovered.
//!
//! **The run lane is a row of its own, always there.** Between the Corp's
//! servers and the Runner's rig — adjacent to the outermost ice from
//! either chair — [`RUN_LANE`] is reserved whether or not a run is on,
//! so the cards keep their size when one begins; a lane that appeared
//! with the run would have re-sized every card at the moment the person
//! most wants to watch the board.

use netrunner_core::rules::Side;

/// What a server column is made of, named so a chair can order it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    /// The header: the deck, the identity or the remote's name, which
    /// sits nearest the Corp.
    Header,
    /// The cards installed in the root, between the header and the ice.
    Root,
    /// The ice, in a column out toward the Runner.
    Ice,
}

/// A server column top to bottom as the chair sees it: from the Corp's
/// chair the ice climbs away toward the Runner at the top of the
/// screen, so it comes first; from the Runner's the header is at the
/// top and the ice comes down to them.
pub fn column_top_down(chair: Side) -> [Piece; 3] {
    match chair {
        Side::Corp => [Piece::Ice, Piece::Root, Piece::Header],
        Side::Runner => [Piece::Header, Piece::Root, Piece::Ice],
    }
}

/// A server's ice top to bottom as the chair sees it. `ice` is the
/// engine's order, outermost first (the piece approached first); the
/// outermost sits nearest the Runner, which is the top of the Corp's
/// screen and the bottom of the Runner's.
pub fn ice_top_down<T>(ice: &[T], chair: Side) -> Vec<&T> {
    match chair {
        Side::Corp => ice.iter().collect(),
        Side::Runner => ice.iter().rev().collect(),
    }
}

/// What the board has to make room for.
///
/// **Nothing here grows with the game.** The ICE on the tallest server
/// and whether the rig is empty were once counts too, so the Corp's
/// third ICE or the Runner's first install shrank every card on the
/// board and redrew it — the middle of the table moved whenever the
/// opponent did anything. Now the rig's row is reserved whether or not
/// anything is in it, and the ICE grows into the flexible field between
/// the server plates and the run lane ([`tile_stack`]), so the face
/// width is a function of the window, the chair, the servers and the
/// phase bar alone. A new remote can still narrow the cards, because
/// server columns cannot overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Corp servers, centrals included: the widest row of columns.
    pub servers: usize,
    /// Which chair the person is in: it decides which side is drawn at
    /// the person's own size and which at [`OPPONENT_SCALE`], and which
    /// strip carries the pile buttons and is taller.
    pub human_is_runner: bool,
    /// Whether the phase bar is on, which costs the board its row.
    pub phase_bar: bool,
}

impl Counts {
    /// The chair the person is in.
    pub fn chair(&self) -> Side {
        if self.human_is_runner { Side::Runner } else { Side::Corp }
    }
}

/// The rail beside the board.
pub const RAIL_WIDTH: f32 = 380.0;
/// The screen root's padding, on each side.
pub const PADDING: f32 = 12.0;
/// Between the board and the rail.
pub const BODY_GAP: f32 = 10.0;
/// The top bar's height, and the control bar's. The control bar is a row
/// of the board now — directly above the person's hand — so it is
/// counted in [`fixed_height`], not taken off [`board_height`].
pub const TOP_BAR: f32 = 44.0;
pub const CONTROL_BAR: f32 = 44.0;
/// The phase bar's height when it is on: a row of step chips with the
/// window's line under it. It is a row of the board like any other — the
/// cards shrink by its height rather than the board scrolling — so
/// turning it off gives the cards the row back.
pub const PHASE_BAR: f32 = 54.0;
/// Between the root's rows, and between the board's rows.
pub const ROW_GAP: f32 = 8.0;
/// A section label ("Servers", "Your hand · 5") and a chip line under
/// a card ("2 adv"), at the small text size with its leading.
pub const LABEL: f32 = 22.0;
pub const CHIPS: f32 = 20.0;
/// The shortest a tile — an ice, or a card in a root — is drawn while it
/// still has the ICE field to itself: one line of small text in a
/// border. Past this the tiles overlap rather than shrink ([`tile_stack`]).
pub const TILE_MIN: f32 = 24.0;
/// The tallest a tile grows, as a fraction of its server's face width:
/// a server with one ICE draws it as a slab, not a card, so a column
/// never reads as a face.
pub const TILE_MAX_SCALE: f32 = 0.3;
/// The gap between two tiles in a column.
pub const TILE_GAP: f32 = 4.0;
/// The least height the ICE field is ever given: a few tiles at their
/// floor before any overlap. It is what the face width gives up to the
/// field; everything above it is the field's anyway.
pub const ICE_FIELD_MIN: f32 = 96.0;
/// The run lane between the two areas: a row of chips and a line of
/// words beneath, with the gaps around it.
pub const RUN_LANE: f32 = 64.0;
/// The opponent's side of the table — their area, their strip and their
/// hand — is drawn at this fraction of the person's own card width, so
/// the table has a near side and a far side. **A constant factor keeps
/// [`fixed_height`] monotone in the face width**, which is what licenses
/// [`face_width`]'s binary search; the per-row ramp §4m rejected was not
/// (see [`Depth`]).
pub const OPPONENT_SCALE: f32 = 0.75;
/// How much of a card in a hand shows: its top third for the person's
/// own (a hovered one lifts out whole), the bottom third of a back for
/// the opponent's. A hand is a fan held at the table's edge, and the
/// two hands were a full card each — the largest thing on the board and
/// the least looked at.
pub const PEEK: f32 = 1.0 / 3.0;
/// The identity in a strip is drawn at this fraction of its side's face
/// width.
pub const IDENTITY_SCALE: f32 = 0.4;
/// The gap between cards in a row.
pub const CARD_GAP: f32 = 6.0;
/// A server column's padding and border beyond its card, each side.
pub const SERVER_CHROME: f32 = 12.0;
/// A server column's vertical padding, borders and the gap under its
/// plate.
pub const SERVER_CHROME_V: f32 = 14.0;
/// A server's plate — where its picture goes — is this fraction of its
/// width tall: 16:9, so art has a fixed shape to be drawn to whatever
/// the card width is. The box is reserved for every server whether or
/// not anybody has drawn one, so art never changes the layout.
pub const PLATE_ASPECT: f32 = 9.0 / 16.0;
/// The widest and narrowest a board face is drawn. Below the floor the
/// text is unreadable and a picture is a smudge; above the cap a board
/// with little on it need not fill a large monitor with card.
pub const MIN_FACE: f32 = 72.0;
pub const MAX_FACE: f32 = 220.0;
/// The strip beside a hand: the identity plus its lines of numbers.
pub const STRIP_TEXT: f32 = 480.0;
/// A strip's text column: the name line, the HUD as one row of large
/// numbers over their words, and — the Runner's — the details line with
/// the pile buttons beside it. Nothing in a strip grows with the game.
/// Over-estimates are the ICE field's; under-estimates clip the hand,
/// which is the thing that must not happen.
pub const STRIP_CORP: f32 = 84.0;
pub const STRIP_RUNNER: f32 = 120.0;
/// The rig's group label.
pub const GROUP_LABEL: f32 = 22.0;

/// The width the board column has: the window less the padding, the
/// rail and the gap.
pub fn board_width(window_width: f32) -> f32 {
    (window_width - 2.0 * PADDING - RAIL_WIDTH - BODY_GAP).max(200.0)
}

/// The height the board column has: the window less the padding, the
/// top bar and the gap under it.
pub fn board_height(window_height: f32) -> f32 {
    window_height - 2.0 * PADDING - TOP_BAR - ROW_GAP
}

/// The face width a side's cards are drawn at from `chair`: the
/// person's own at `face`, the opponent's at [`OPPONENT_SCALE`] of it.
pub fn area_face(side: Side, chair: Side, face: f32) -> f32 {
    if side == chair { face } else { (face * OPPONENT_SCALE).floor() }
}

/// A server plate's height at its server's face width.
pub fn plate_height(server_face: f32) -> f32 {
    ((server_face + 4.0) * PLATE_ASPECT).round()
}

/// A strip row's height: the hand's peek (with its label), the identity,
/// or the strip's text, whichever is tallest.
pub fn strip_height(side: Side, side_face: f32) -> f32 {
    let text = match side {
        Side::Corp => STRIP_CORP,
        Side::Runner => STRIP_RUNNER,
    };
    let peek = LABEL + PEEK * 1.4 * side_face;
    let identity = 1.4 * IDENTITY_SCALE * side_face;
    peek.max(identity).max(text)
}

/// The rig's row: its label, a group label, a card and its chip line —
/// reserved whether or not anything is installed.
pub fn rig_height(rig_face: f32) -> f32 {
    LABEL + GROUP_LABEL + 1.4 * rig_face + CHIPS
}

/// Everything on the board but the ICE field, at face width `face`: the
/// two strip rows, the servers' label and plates, the run lane, the rig,
/// the control bar, the phase bar and the gaps between them. Each term
/// is a non-decreasing function of `face`, so the sum is monotone, which
/// is what lets [`face_width`] search it.
pub fn fixed_height(face: f32, counts: Counts) -> f32 {
    let chair = counts.chair();
    let opponent = chair.other();
    let server_face = area_face(Side::Corp, chair, face);
    let rig_face = area_face(Side::Runner, chair, face);
    let strips = strip_height(opponent, area_face(opponent, chair, face)) + strip_height(chair, face);
    let servers = LABEL + plate_height(server_face) + SERVER_CHROME_V;
    let phase = if counts.phase_bar { PHASE_BAR + ROW_GAP } else { 0.0 };
    // Six rows — two strips, the servers, the lane, the rig, the control
    // bar — and five gaps between them.
    strips + servers + RUN_LANE + rig_height(rig_face) + CONTROL_BAR + phase + 5.0 * ROW_GAP
}

/// The height the ICE field has at face width `face`: what the fixed
/// rows leave of the board. Never less than [`ICE_FIELD_MIN`] unless the
/// face is already at its floor.
pub fn field_height(window_height: f32, face: f32, counts: Counts) -> f32 {
    (board_height(window_height) - fixed_height(face, counts)).max(0.0)
}

/// The face width the board has room for, in pixels: the largest for
/// which [`fixed_height`] and [`ICE_FIELD_MIN`] fit [`board_height`],
/// capped by the servers, which are columns that cannot overlap, within
/// [`MIN_FACE`]..[`MAX_FACE`].
pub fn face_width(window: (f32, f32), counts: Counts) -> f32 {
    let (width, height) = window;
    let room = board_height(height) - ICE_FIELD_MIN;
    // Binary search over whole pixels: the rows grow with the face and
    // the answer is the last width that still fits.
    let (mut low, mut high) = (MIN_FACE as u32, MAX_FACE as u32);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if fixed_height(mid as f32, counts) <= room {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let by_height = low as f32;
    let servers = counts.servers.max(1) as f32;
    let server_face = (board_width(width) - (servers - 1.0) * CARD_GAP) / servers - 2.0 * SERVER_CHROME;
    // The servers are the opponent's from the Runner's chair, drawn at the
    // smaller scale, so the cap on the person's own face is looser there.
    let by_width = if counts.human_is_runner { server_face / OPPONENT_SCALE } else { server_face };
    by_height.min(by_width).clamp(MIN_FACE, MAX_FACE).floor()
}

/// A server column's tiles in its share of the ICE field: `(height,
/// advance)`, where `advance` is the distance from one tile's top to
/// the next's. `pieces` tiles — the ICE and the root cards — share
/// `field` pixels: each as tall as its share allows between [`TILE_MIN`]
/// and [`TILE_MAX_SCALE`] of the server's face, and past the floor they
/// overlap by [`step`]'s rule turned on its side, so the ICE grows into
/// the middle of the table rather than shrinking every card on it.
pub fn tile_stack(field: f32, pieces: usize, server_face: f32) -> (f32, f32) {
    let max = (server_face * TILE_MAX_SCALE).max(TILE_MIN);
    if pieces == 0 {
        return (max, max + TILE_GAP);
    }
    let share = field / pieces as f32 - TILE_GAP;
    let height = share.clamp(TILE_MIN, max).floor();
    (height, step(pieces, height, TILE_GAP, field))
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

/// How far from the person's chair a row of the board sits.
///
/// **The depth the board has is two things: a scale and a shadow.** The
/// opponent's side is drawn at [`OPPONENT_SCALE`] of the person's own —
/// the far side of a table is smaller — and every row's contact shadow
/// says how near it is. §4m first put the perspective in the table's
/// paint and the third list's item 1 rejected a per-row *face-width*
/// ramp on more than taste: the natural form is not monotone in the face
/// width, and `face_width`'s binary search is licensed only by the rows'
/// height being monotone, so the ramp would have returned a silently
/// wrong width with every test still green. A *constant* factor has no
/// such problem — every row is still a non-decreasing function of one
/// width — which is why the scale is one number for the whole far side
/// and not a ramp. A shadow is paint and not layout: a `BoxShadow` is
/// drawn outside the node and measured by nothing, so it may vary by row
/// freely.
///
/// The four values are `spawn_board`'s four rows, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Depth {
    /// The opponent's strip and their hand, drawn as backs: the far edge.
    Far,
    /// The opponent's area — their servers, or their rig.
    Upper,
    /// The person's own area, this side of the run lane.
    Lower,
    /// The person's own strip and their hand: the near edge, where the
    /// table meets the chair.
    Near,
}

impl Depth {
    /// The row an area belongs to: the person's own is the near one.
    pub fn area(side: Side, chair: Side) -> Depth {
        if side == chair {
            Depth::Lower
        } else {
            Depth::Upper
        }
    }

    /// The row a strip and its hand belong to.
    pub fn strip(side: Side, chair: Side) -> Depth {
        if side == chair {
            Depth::Near
        } else {
            Depth::Far
        }
    }

    /// One step nearer the chair, for something stacked *on* something
    /// else: a tile sits on its server column, so it is raised off the
    /// table by the column's own thickness and its shadow says so. The
    /// near edge has nowhere further to go and stays put.
    pub fn nearer(self) -> Depth {
        match self {
            Depth::Far => Depth::Upper,
            Depth::Upper => Depth::Lower,
            Depth::Lower | Depth::Near => Depth::Near,
        }
    }

    /// The contact shadow this row casts: `(y offset, blur, alpha)`, in
    /// logical pixels and a black alpha.
    ///
    /// Nearer is bigger, softer and darker, which is what a shallow
    /// perspective does to a shadow — not a physical model, and it does
    /// not need to be: the point is that the four rows do not all sit at
    /// the same height off the same table. Spread stays 0 throughout, so
    /// a shadow never reads as a second border.
    pub fn shadow(self) -> (f32, f32, f32) {
        match self {
            Depth::Far => (1.0, 5.0, 0.50),
            Depth::Upper => (2.0, 8.0, 0.55),
            Depth::Lower => (3.0, 11.0, 0.60),
            Depth::Near => (5.0, 16.0, 0.65),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The depth ramp is the one that *may* vary across rows, because it
    /// is paint: every value grows toward the chair, and nothing here is
    /// read by `face_width` or by any other size.
    #[test]
    fn a_nearer_row_casts_a_bigger_softer_shadow() {
        let rows = [Depth::Far, Depth::Upper, Depth::Lower, Depth::Near];
        for pair in rows.windows(2) {
            let (near_y, near_blur, near_alpha) = pair[1].shadow();
            let (far_y, far_blur, far_alpha) = pair[0].shadow();
            assert!(near_y > far_y && near_blur > far_blur && near_alpha > far_alpha, "{:?} should sit nearer than {:?}", pair[1], pair[0]);
        }
    }

    /// Which row a thing belongs to is the chair's question, not the
    /// side's: the Corp sees their own servers in the near row and the
    /// Runner sees the same servers in the far one.
    #[test]
    fn the_rows_are_read_from_the_persons_own_chair() {
        assert_eq!(Depth::area(Side::Corp, Side::Corp), Depth::Lower);
        assert_eq!(Depth::area(Side::Corp, Side::Runner), Depth::Upper);
        assert_eq!(Depth::strip(Side::Runner, Side::Runner), Depth::Near);
        assert_eq!(Depth::strip(Side::Runner, Side::Corp), Depth::Far);
        // Stacked on something else is one row nearer, and the near edge
        // has nowhere left to go.
        assert_eq!(Depth::Upper.nearer(), Depth::Lower);
        assert_eq!(Depth::Near.nearer(), Depth::Near);
    }

    #[test]
    fn the_columns_are_the_table_seen_from_the_chair() {
        assert_eq!(column_top_down(Side::Corp), [Piece::Ice, Piece::Root, Piece::Header]);
        assert_eq!(column_top_down(Side::Runner), [Piece::Header, Piece::Root, Piece::Ice]);
        // Outermost first from the engine: nearest the Runner either way.
        let ice = ["outer", "middle", "inner"];
        assert_eq!(ice_top_down(&ice, Side::Corp), [&"outer", &"middle", &"inner"]);
        assert_eq!(ice_top_down(&ice, Side::Runner), [&"inner", &"middle", &"outer"]);
    }

    #[test]
    fn the_face_shrinks_with_the_window_and_stays_in_range() {
        let none = Counts { servers: 4, human_is_runner: true, phase_bar: true };
        let large = face_width((1920.0, 1080.0), none);
        let small = face_width((1280.0, 800.0), none);
        assert!(large > small, "{large} > {small}");
        assert_eq!(face_width((800.0, 400.0), none), MIN_FACE, "never below the floor");
        assert_eq!(face_width((4000.0, 3000.0), none), MAX_FACE, "never above the cap");
        // Nine servers across a laptop width cap it below what the
        // height would allow.
        assert!(face_width((1280.0, 1080.0), Counts { servers: 9, ..none }) < face_width((1280.0, 1080.0), none));
        // The phase bar is a row of the board's height, so the cards are
        // never larger with it on.
        assert!(face_width((1280.0, 900.0), Counts { phase_bar: false, ..none }) >= face_width((1280.0, 900.0), none));
        assert_eq!(fixed_height(100.0, none) - fixed_height(100.0, Counts { phase_bar: false, ..none }), PHASE_BAR + ROW_GAP, "the bar is a row of the board");
    }

    /// The rule the whole field exists for: the ICE and the rig are not
    /// counts, so nothing the opponent installs moves a card. The face
    /// width is a function of the window, the chair, the servers and the
    /// phase bar alone — `Counts` has nowhere to put an ICE count.
    #[test]
    fn the_far_side_is_smaller_by_one_constant() {
        for chair in [Side::Corp, Side::Runner] {
            assert_eq!(area_face(chair, chair, 200.0), 200.0);
            assert_eq!(area_face(chair.other(), chair, 200.0), 150.0);
        }
        // From the Runner's chair the rig is the near row and the plates
        // the far one; from the Corp's the reverse. Either way the near
        // side costs more of the height.
        let runner = Counts { servers: 4, human_is_runner: true, phase_bar: false };
        let corp = Counts { human_is_runner: false, ..runner };
        assert!(rig_height(area_face(Side::Runner, Side::Runner, 200.0)) > rig_height(area_face(Side::Runner, Side::Corp, 200.0)));
        assert!(fixed_height(200.0, runner) > fixed_height(200.0, corp), "a full-size rig is taller than full-size plates");
    }

    /// The rows at the computed width leave the ICE field its minimum, and
    /// one pixel more would not: the invariant the module exists for.
    #[test]
    fn the_rows_at_the_computed_width_fit_the_window_and_no_wider_would() {
        for (window, runner, phase_bar) in [((1280.0, 800.0), true, true), ((1280.0, 800.0), false, false), ((1920.0, 1080.0), true, false), ((1366.0, 768.0), true, true), ((2560.0, 1440.0), false, true), ((2000.0, 1250.0), true, false)] {
            let counts = Counts { servers: 5, human_is_runner: runner, phase_bar };
            let w = face_width(window, counts);
            assert!(field_height(window.1, w, counts) >= ICE_FIELD_MIN || w == MIN_FACE, "{window:?}: field {}", field_height(window.1, w, counts));
            if w < MAX_FACE && w > MIN_FACE {
                let wider = (w + 1.0).min(MAX_FACE);
                let server_face = (board_width(window.0) - 4.0 * CARD_GAP) / 5.0 - 2.0 * SERVER_CHROME;
                let cap = if runner { server_face / OPPONENT_SCALE } else { server_face };
                assert!(field_height(window.1, wider, counts) < ICE_FIELD_MIN || wider > cap, "{window:?}: {w} could have been {wider}");
            }
        }
    }

    /// A tile is as tall as its share of the field allows, within its
    /// bounds, and past the floor the tiles overlap rather than leave the
    /// field.
    #[test]
    fn tiles_share_the_field_and_overlap_past_the_floor() {
        let face = 160.0;
        let max = face * TILE_MAX_SCALE;
        assert_eq!(tile_stack(400.0, 1, face).0, max.floor(), "one ICE is a slab at most");
        assert_eq!(tile_stack(400.0, 0, face).0, max);
        let (height, advance) = tile_stack(200.0, 5, face);
        assert!((TILE_MIN..=max).contains(&height), "{height}");
        assert_eq!(advance, height + TILE_GAP, "five fit without overlapping");
        let (height, advance) = tile_stack(120.0, 10, face);
        assert_eq!(height, TILE_MIN);
        assert!(advance < height + TILE_GAP && 9.0 * advance + height <= 120.0 + 1e-3, "ten overlap to fit: {advance}");
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
