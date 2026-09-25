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
//! rules ("does not matter" — the learn-to-play guide), but the table
//! gives them one: programs, hardware and resources each in their own
//! row, programs the row nearest the ICE they break
//! (`netrunner_client::board::rig::rows_top_down`),
//! which is how a real table and jinteki.net both lay a rig out.
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
//! that grows.** Between the plates and the rig the columns get
//! whatever height the fixed rows leave ([`field_height`]); a tile is as
//! tall as its share of it allows and overlaps past a floor
//! ([`tile_stack`]), so the Corp's ICE and the Runner's installs never
//! move a card: the rig's rows are reserved empty or not. From the Corp's
//! chair the plates are at the bottom, next to the Corp, and the ICE
//! climbs; from the Runner's they are at the top and it comes down.
//!
//! **The far side is smaller.** The opponent's avatar bar, hand and area are
//! drawn at [`OPPONENT_SCALE`] of the person's own width — the Runner
//! sees the Corp's servers smaller and their own rig at full size, the
//! Corp the reverse — and both hands show a [`PEEK`] of each card, the
//! person's own lifting out whole when hovered.
//!
//! **There is no run lane.** A row of chips between the servers and the
//! rig was reserved for a run until 24 September 2026, and removed at
//! the person's request: the phase panel and the encounter panel say
//! what it said, and its height went to the ICE field and the faces.

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
/// opponent did anything. Now the rig's rows are reserved whether or not
/// anything is in them, and the ICE grows into the flexible field between
/// the server plates and the rig ([`tile_stack`]), so the face
/// width is a function of the window, the chair and the servers alone.
/// A new remote can still narrow the cards, because server columns cannot
/// overlap. (The phase bar was a count too, while it was a row of the
/// board; it is a panel in the right column now and costs the cards
/// nothing.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Corp servers, centrals included: the widest row of columns.
    pub servers: usize,
    /// Which chair the person is in: it decides which side is drawn at
    /// the person's own size and which at [`OPPONENT_SCALE`].
    pub human_is_runner: bool,
}

impl Counts {
    /// The chair the person is in.
    pub fn chair(&self) -> Side {
        if self.human_is_runner { Side::Runner } else { Side::Corp }
    }
}

/// The right column beside the board: the status line, Quit and the gear,
/// the run's Runner, the prompt, the log and the phase panel.
pub const RAIL_WIDTH: f32 = 380.0;
/// The screen root's padding on its left and right, and the right
/// column's at its top and bottom. **The board has none above or below:**
/// the opponent's hand hangs from the window's top edge and the person's
/// own sits on its bottom edge, the way a table runs off the edge of a
/// photograph of it.
pub const PADDING: f32 = 12.0;
/// Between the board and the rail.
pub const BODY_GAP: f32 = 10.0;
/// The right column's header row (the status line, Quit, the gear), and
/// the control bar's height. The header is the right column's, not the
/// board's: a top bar across the window cost every card its height, and
/// the words in it were the same kind as the rail's beside it. The
/// control bar is a row of the board — directly above the person's hand
/// — so it is counted in [`fixed_height`].
pub const TOP_BAR: f32 = 44.0;
pub const CONTROL_BAR: f32 = 44.0;
/// The Continue button's width, whatever step it names: room for the
/// longest ("Continue to Encounter Wall of Static") on one line at the
/// body size, so the bar never reflows between two presses of it.
pub const CONTINUE_WIDTH: f32 = 360.0;
/// Between the root's rows, and between the board's rows.
pub const ROW_GAP: f32 = 8.0;
/// A section label ("Servers") and a chip line under
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
/// The opponent's side of the table — their area, their strip and their
/// hand — is drawn at this fraction of the person's own card width, so
/// the table has a near side and a far side. **A constant factor keeps
/// [`fixed_height`] monotone in the face width**, which is what licenses
/// [`face_width`]'s binary search; the per-row ramp §4m rejected was not
/// (see [`Depth`]).
pub const OPPONENT_SCALE: f32 = 0.75;
/// How much of a card in a hand or a rig row shows: its top third for
/// the person's own hand (a hovered one rises out of the row whole) and for every
/// rig card, the bottom third of a back for the opponent's hand. A hand
/// is a fan held at the table's edge, and the two hands were a full card
/// each — the largest thing on the board and the least looked at. The
/// rig's three rows are three peeks for the same reason: three whole
/// rows did not fit a laptop's board ([`rig_row_height`]).
pub const PEEK: f32 = 1.0 / 3.0;
/// A side's avatar — its identity's art cropped to a disc — in the
/// person's own row, and the bar of numbers either side of it: the plate
/// is [`BAR`] tall and the row as tall as the disc, the plate centred on
/// it. **Fixed pixels, never a fraction of the face width**, which moves
/// when a remote is made: the person asked for the bar and the avatar to
/// stay where they are for the whole match (Phase 7 §4bi). The far side's
/// is [`OPPONENT_SCALE`] of it, a constant too.
pub const AVATAR: f32 = 72.0;
pub const BAR: f32 = 48.0;
/// How far each wing of the bar runs in under the disc, so the ring sits
/// on the plate rather than beside it.
pub const BAR_TUCK: f32 = 14.0;
/// What a wing's outer end keeps clear before its text: the plate's
/// chamfer and the traces that climb out of the channel to their vias,
/// the last of which sits at 58 of the picture's 240 logical pixels.
pub const BAR_OUTER: f32 = 66.0;
/// Where the art sits on an identity's scan, for the avatar's crop:
/// `[centre x, centre y, side]`, the side a fraction of the card's width.
/// Measured off Null Signal Games' identity frame (Zahya Sadeghi, The
/// Catalyst, Precision Design): the picture runs from under the name
/// banner to the text box at 0.63 of the height, and a face sits a
/// little above its middle. Like [`IDENTITY_ART`], a fraction of the
/// scan, because the copy drawn is whichever width was decoded.
pub const AVATAR_ART: [f32; 3] = [0.5, 0.36, 0.62];
/// How much of a Runner identity's scan the run panel shows, from the
/// top, as a fraction of the card's height: the name banner and the
/// picture, stopping where the text box begins. Measured off Null Signal
/// Games' identity frame (the text box's top edge sits at 665 of 1050 on
/// Zahya Sadeghi and The Catalyst alike), and the one place that knows a
/// scan's geometry. A fraction of the laid-out card rather than a pixel
/// `rect`, because the scan is drawn from whichever resampled copy fits
/// (Phase 7 §4w) and their pixel sizes differ.
pub const IDENTITY_ART: f32 = 0.63;
/// How much of an ICE's scan the encounter panel shows: the art alone,
/// as `[left, top, right, bottom]` fractions of the card. An ICE is
/// printed upright with its text box on top and its art underneath, so
/// the name banner and the picture are not one band as on an identity,
/// and the panel prints the name in words instead. Measured off Null
/// Signal Games' ice frame — the art runs from about 0.51 of the height
/// to the bottom edge's 0.95, between the type strip on the left (0.12
/// of the width) and the subroutine track on the right (0.9) — on Brân
/// 1.0, Bumi 1.0, Biawak, Palisade, Mycoweb, Funhouse and Pharos, and it
/// falls inside the art of the older frame too (Ice Wall's text box ends
/// by 0.37). Drawn unrotated: a rotated `UiTransform` is laid out as
/// its unrotated box.
pub const ICE_ART: [f32; 4] = [0.12, 0.54, 0.88, 0.94];
/// The shortest the encounter panel's art is drawn; below it the panel
/// shows the words alone, because a picture that small is a smudge and
/// the words are what the encounter is decided on.
pub const ENCOUNTER_ART_MIN: f32 = 72.0;
/// The height a line of the encounter panel's small text takes, and how
/// many characters fit on one at the panel's width — an estimate, used
/// only to decide how tall the art may be, so it errs short.
const ENCOUNTER_LINE: f32 = 21.0;
const ENCOUNTER_CHARS_PER_LINE: usize = 40;
/// What the right column keeps under the encounter panel for the rail:
/// the prompt's title and one button (Take it back), so the art never
/// pushes the one thing the person may press off the window.
const RAIL_RESERVE: f32 = 120.0;
/// The panel's padding, border and gaps, the column's gaps around it,
/// and the name's taller line.
const ENCOUNTER_CHROME: f32 = 2.0 * 10.0 + 2.0 * 2.0 + 3.0 * 8.0 + 8.0;

/// The encounter panel's art, `(width, height)`, scaled whole from its
/// `natural` size to what the right column has left, or `None` when that
/// is under [`ENCOUNTER_ART_MIN`].
///
/// **The picture gives first**, as the pop-up's cards do: the column
/// never scrolls and never runs off the window, so its words — `lines`,
/// the panel's small text lines, each as its characters — and the rail's
/// prompt under it take their room, and the art takes what is left. At
/// 2560×1600 that is all of it; at 1366×768 with the phase panel on it is
/// a thumbnail or nothing. `above` is what the column's fixed panels
/// already hold (the header over the panel and the phase panel at the
/// column's foot, measured), and `window` the
/// window's logical height. Scaled whole, never squeezed: `ImageNode`
/// stretches, so a shorter box at the same width would distort the art.
pub fn encounter_art<'a>(natural: (f32, f32), window: f32, above: f32, lines: impl IntoIterator<Item = &'a str>) -> Option<(f32, f32)> {
    let words: f32 = lines.into_iter().map(|line| line.chars().count().div_ceil(ENCOUNTER_CHARS_PER_LINE).max(1) as f32 * ENCOUNTER_LINE + 6.0).sum();
    let room = window - 2.0 * PADDING - above - ENCOUNTER_CHROME - words - RAIL_RESERVE;
    let (width, height) = natural;
    let drawn = height.min(room.floor());
    (drawn >= ENCOUNTER_ART_MIN).then(|| ((width * drawn / height).round(), drawn))
}
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
/// The rig's three rows, each the top [`PEEK`] of its cards over their
/// chip line, with [`RIG_ROW_GAP`] between them.
pub const RIG_ROWS: usize = 3;

pub const RIG_ROW_GAP: f32 = 4.0;
/// The column of row labels at the rig's left ("Programs", "Hardware",
/// "Resources"), on the row rather than over it, so a label costs the
/// rig width, which it has, rather than height, which it does not.
pub const RIG_LABEL_WIDTH: f32 = 84.0;

/// The width the board column has: the window less the padding, the
/// rail and the gap.
pub fn board_width(window_width: f32) -> f32 {
    (window_width - 2.0 * PADDING - RAIL_WIDTH - BODY_GAP).max(200.0)
}

/// The height the board column has: the whole window. The top bar went
/// to the right column, the phase bar with it, and the root pads only
/// its sides, so the hands sit on the window's two edges.
pub fn board_height(window_height: f32) -> f32 {
    window_height
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

/// A side's avatar disc and the height of its bar's row, from `chair`:
/// [`AVATAR`] for the person's own and [`OPPONENT_SCALE`] of it for the
/// far side's. Constants of the chair, so nothing in the game moves them.
pub fn avatar_size(side: Side, chair: Side) -> f32 {
    if side == chair { AVATAR } else { (AVATAR * OPPONENT_SCALE).round() }
}

/// The square of an identity's scan the avatar shows, `[x0, y0, x1,
/// y1]` in the scan's pixels, for a scan `size` wide and tall: centred on
/// [`AVATAR_ART`]'s point and pushed back inside the scan if it would
/// run off an edge.
pub fn avatar_crop(size: (f32, f32)) -> [f32; 4] {
    let (width, height) = size;
    let [cx, cy, side] = AVATAR_ART;
    let side = (side * width).min(width).min(height);
    let x0 = (cx * width - side / 2.0).clamp(0.0, width - side);
    let y0 = (cy * height - side / 2.0).clamp(0.0, height - side);
    [x0, y0, x0 + side, y0 + side]
}

/// One wing of a bar `board` wide around a disc `disc` wide: half of
/// what the disc leaves, plus the part that runs in under it.
pub fn bar_wing(board: f32, disc: f32) -> f32 {
    ((board - disc) / 2.0 + BAR_TUCK).max(0.0)
}

/// The narrowest wing that keeps each readout's word beside its number.
/// Narrower, a readout with a glyph is the glyph and the number alone —
/// the glyph says what it counts — and one without keeps its word. At
/// 1366 × 768 a wing is 454 wide, and the Runner's credits, clicks,
/// agendas and piles did not fit it with their words; at 1920 × 1080 it
/// is 731.
pub const BAR_WORDS_MIN: f32 = 640.0;

/// A side's bar plate's height from `chair`, scaled as its disc is.
pub fn bar_height(side: Side, chair: Side) -> f32 {
    if side == chair { BAR } else { (BAR * OPPONENT_SCALE).round() }
}

/// A side's edge of the table: its avatar row, and under it (the
/// person's) or over it (the opponent's) the [`PEEK`] of its hand.
pub fn strip_height(side: Side, chair: Side, side_face: f32) -> f32 {
    avatar_size(side, chair) + (PEEK * 1.4 * side_face).round()
}

/// One rig row: the top [`PEEK`] of a card at its side's face width, and
/// the chip line under it. Measured before choosing (5 servers, the
/// Runner's chair): three rows of whole cards at half the width took the
/// face at 1366×768 from 119 px to 85 and left a rig card 42 px wide;
/// three peeked rows at the full width cost the face nothing — 220 at
/// 1920×1080 and 119 at 1366×768, as the one row did — because a peek
/// of three is the height of one card. The title and cost are the top
/// of a card; the strength and the counters are the chip line; the rest
/// is the sheet's.
pub fn rig_row_height(rig_face: f32) -> f32 {
    (PEEK * 1.4 * rig_face).round() + CHIPS
}

/// The rig: its three rows and the gaps between them — every row
/// reserved whether or not anything is installed in it, so the first
/// program, the first piece of hardware or the first resource moves no
/// card.
pub fn rig_height(rig_face: f32) -> f32 {
    RIG_ROWS as f32 * rig_row_height(rig_face) + (RIG_ROWS - 1) as f32 * RIG_ROW_GAP
}

/// Everything on the board but the ICE field, at face width `face`: the
/// two strip rows, the servers' label and plates, the rig,
/// the control bar and the gaps between them. Each term
/// is a non-decreasing function of `face`, so the sum is monotone, which
/// is what lets [`face_width`] search it.
pub fn fixed_height(face: f32, counts: Counts) -> f32 {
    let chair = counts.chair();
    let opponent = chair.other();
    let server_face = area_face(Side::Corp, chair, face);
    let rig_face = area_face(Side::Runner, chair, face);
    let strips = strip_height(opponent, chair, area_face(opponent, chair, face)) + strip_height(chair, chair, face);
    let servers = LABEL + plate_height(server_face) + SERVER_CHROME_V;
    // Five rows — two strips, the servers, the rig, the control bar —
    // and four gaps between them.
    strips + servers + rig_height(rig_face) + CONTROL_BAR + 4.0 * ROW_GAP
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

/// The width of a card in a zone's sheet — Archives, the Heap, the
/// Corp's own HQ, a remote's contents. It was the browser grid's `Thumb`
/// (140), and a card in Archives could not be read at that size (Phase 7
/// §8 item 20). 220 is the widest that still puts four to a row in the
/// zone sheet's 960-wide panel (four faces and three gaps are 898 of the
/// 912 the panel's padding and scroll bar leave), and its body text is
/// 12.8 px against the board's reference 10.5.
pub const PILE_FACE: f32 = 220.0;
/// What a zone's sheet keeps above its cards and around its panel: the
/// window's margin, the panel's padding and border, the heading, the
/// caption and their gaps.
pub const PILE_CHROME: f32 = 130.0;
/// The gap between a zone sheet's cards, both ways (`wrap_row`).
pub const PILE_GAP: f32 = 6.0;
/// The most rows a zone sheet shows before its box scrolls: four across
/// by three down, asked for by the person so a large pile on a tall
/// window is a sheet with a scroll bar rather than the whole screen.
pub const PILE_ROWS: f32 = 3.0;

/// The tallest a zone sheet's scrolling box may be in a window
/// `window_height` tall: as many whole rows of [`PILE_FACE`] cards as fit
/// under the sheet's chrome, up to [`PILE_ROWS`], so a row is never cut
/// at the box's edge while there is room for it and a pile of forty is
/// never the whole screen. A window too short for one row gets what
/// is left, and the wheel reaches the rest. The sheet is an overlay, so
/// this box scrolling is not the board scrolling; before item 20 the cap
/// was a fixed 460, which held three rows of `Thumb` faces and would
/// have held one and a half of these.
pub fn pile_height(window_height: f32) -> f32 {
    let available = (window_height - PILE_CHROME).max(0.0);
    let row = PILE_FACE * 1.4 + PILE_GAP;
    let rows = ((available + PILE_GAP) / row).floor().min(PILE_ROWS);
    if rows >= 1.0 { rows * row - PILE_GAP } else { available }
}

/// The overlays' panel widths. A card alone is its large face and the
/// panel's padding; an install's state sits beside the face; a zone's
/// contents are the widest, four [`PILE_FACE`] cards across.
pub const SHEET_CARD: f32 = CHOICE_FACE_MAX + 2.0 * 17.0;
pub const SHEET_INSTALL: f32 = 800.0;
pub const SHEET_ZONE: f32 = 960.0;

/// A card's width in the deck builder's grids — the editor's pool, a
/// built-in deck's spread and both identity pickers. The grids were
/// `FaceSize::Thumb` (140), and at that width neither a card's text nor an
/// identity's ability could be read on a 2560-wide window: the person
/// could not tell what they were adding. 260 puts a title and a type
/// line in reach at a glance and leaves the text to the secondary click's
/// reader (`widgets::reader`); it
/// is a fixed width rather than one fitted to the window because these
/// are menu screens, whose grids scroll, and a wrapping row of fixed
/// cells is what the card browser already is.
pub const DECK_FACE: f32 = 260.0;
/// The widest a deck builder grid's card grows to fill its row.
pub const DECK_FACE_MAX: f32 = 340.0;
/// The gap between a deck builder grid's cards, both ways.
pub const DECK_GAP: f32 = 8.0;

/// The width of each card in a deck builder grid whose rows are `width`
/// wide: as many [`DECK_FACE`] cards as fit, widened to fill the row up
/// to [`DECK_FACE_MAX`]. A fixed width left most of a card's width
/// unused at the row's end on a wide window (380 px at 2560), which is
/// room the person's reading wanted.
pub fn deck_face(width: f32) -> f32 {
    let across = ((width + DECK_GAP) / (DECK_FACE + DECK_GAP)).floor().max(1.0);
    ((width + DECK_GAP) / across - DECK_GAP).floor().clamp(120.0, DECK_FACE_MAX)
}

/// Where an overlay's panel sits (Phase 7 §8 item 21).
///
/// **A surface opened only to read goes to the right; a question stays
/// in the middle.** Asked for with play between two people in mind: one
/// chair reads a card while the other may still be playing, and a panel
/// over the middle of the board, under a wash that dims the rest, hid
/// the field the reader was watching. The right column is where the
/// reader's eye already goes: the status, the prompt and the run panel
/// are there. The decision pop-up is not an overlay and is not moved: a
/// choice pauses the game on both sides for the moment it takes, so the
/// middle is the point. Nor are the forms and the three panels that ask
/// a question (the end of the match, a stall, the quit prompt). The
/// split is `Game::dismissed_by_a_click_away`, the one that already
/// said which surfaces a click that misses closes, so there is still one
/// list of reading surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// In the middle of the window, under the full wash.
    Centre,
    /// Against the window's right edge, level with the right column's
    /// edge, under a light wash so the board stays readable behind it.
    Side,
}

impl Placement {
    /// A reading surface goes to the side; everything else is centred.
    pub fn of(reading: bool) -> Self {
        if reading { Placement::Side } else { Placement::Centre }
    }

    /// The panel's left edge in a window `window_width` wide, for a
    /// panel `panel_width` wide. A panel wider than the window keeps its
    /// left edge on the window's; the panel's own width caps at the
    /// window, so this only decides where the room goes.
    pub fn left(self, window_width: f32, panel_width: f32) -> f32 {
        match self {
            Placement::Centre => ((window_width - panel_width) / 2.0).max(0.0),
            Placement::Side => (window_width - PADDING - panel_width).max(0.0),
        }
    }

    /// How opaque the wash behind the panel is. The centred panels keep
    /// the 0.75 they always had: a form or a question is the whole of the
    /// moment. A reading surface's wash is light, because the board
    /// behind it is what the reader is watching; it is still there, and
    /// still a press that closes the sheet, so the board under it takes
    /// no click while the card is open.
    pub fn wash_alpha(self) -> f32 {
        match self {
            Placement::Centre => 0.75,
            Placement::Side => 0.3,
        }
    }
}

/// The widest a card in the decision pop-up is drawn: the card sheet's.
pub const CHOICE_FACE_MAX: f32 = 380.0;
/// What a card in the pop-up keeps under it: its button, room for a
/// label that wraps once, and the gap.
pub const CHOICE_CAPTION: f32 = 64.0;
pub const CHOICE_GAP: f32 = 10.0;

/// The width to draw `n` cards at in the decision pop-up, and how many to
/// a row, given the `available` box the pop-up's words and its other
/// buttons leave (width, height). **The cards give, never the window:**
/// like the board, the pop-up is never a scroll container — a choice
/// between cards the person has to scroll to see is the complaint this
/// answers with the words taken away. So every row count from one to `n`
/// is tried and the one that draws the cards widest wins; each card is
/// capped at [`CHOICE_FACE_MAX`] and a row is as tall as a card plus its
/// [`CHOICE_CAPTION`]. There is no floor: at a width too narrow to read,
/// the card sheet a secondary click opens is the reading, and a card cut
/// off the window would be no card at all.
pub fn choice_faces(available: (f32, f32), n: usize) -> (f32, usize) {
    if n == 0 {
        return (CHOICE_FACE_MAX, 0);
    }
    let (width, height) = available;
    let mut best = (0.0_f32, n);
    for rows in 1..=n {
        let per_row = n.div_ceil(rows);
        let by_width = (width - (per_row as f32 - 1.0) * CHOICE_GAP) / per_row as f32;
        let row_height = (height - (rows as f32 - 1.0) * CHOICE_GAP) / rows as f32;
        let by_height = (row_height - CHOICE_CAPTION) / 1.4;
        let face = by_width.min(by_height).min(CHOICE_FACE_MAX).floor();
        if face > best.0 {
            best = (face, per_row);
        }
    }
    (best.0.max(1.0), best.1)
}

/// What the start-of-game box keeps under its row of identities: one
/// line saying whose each is.
pub const OPENING_CAPTION: f32 = 24.0;

/// The widest the start-of-game box draws a card: [`PILE_FACE`], the
/// width a card is read at on a pile's sheet.
pub const OPENING_FACE_MAX: f32 = PILE_FACE;

/// The width to draw the start-of-game box's cards at, and how many of the
/// `hand` to a row: [`choice_faces`]'s search, with a row of two
/// identities over the hand at the same width. The hand's cards carry no
/// button — the keep and the mulligan are under them — so only the
/// identities' row has a caption. Every hand card and both identities are
/// one size, because the box is a look at the table, not a choice between
/// its cards.
///
/// **Capped at [`OPENING_FACE_MAX`], not [`CHOICE_FACE_MAX`].** Grown to
/// the window, the identities and a five-card hand filled it edge to edge and the keep and the
/// mulligan were pills two thousand pixels wide under them; the person
/// called it huge. A pile's cards are read at the same width.
pub fn opening_faces(available: (f32, f32), hand: usize) -> (f32, usize) {
    let (width, height) = available;
    let n = hand.max(1);
    let mut best = (0.0_f32, n);
    for rows in 1..=n {
        let per_row = n.div_ceil(rows);
        let across = per_row.max(2) as f32;
        let by_width = (width - (across - 1.0) * CHOICE_GAP) / across;
        let by_height = (height - OPENING_CAPTION - rows as f32 * CHOICE_GAP) / (1.4 * (rows as f32 + 1.0));
        let face = by_width.min(by_height).min(OPENING_FACE_MAX).floor();
        if face > best.0 {
            best = (face, per_row);
        }
    }
    (best.0.max(1.0), best.1)
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
    /// The person's own area, this side of the ICE.
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

/// The box on the window a menu sits against — the clicked node's, in
/// logical pixels: `x, y` its centre, `width, height` its size. A menu at
/// the pointer landed somewhere different on every click; the card's own
/// box is one place.
///
/// It lives here, with the fitting math, rather than beside the view
/// model: [`menu_box`] is the only thing that reads it, and this module
/// depends on nothing of the game so its geometry can be tested with no
/// view at all.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Anchor {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Anchor {
    /// The box's own edges, which is what a menu is placed against.
    fn top(self) -> f32 {
        self.y - self.height / 2.0
    }

    fn bottom(self) -> f32 {
        self.y + self.height / 2.0
    }
}

/// One column of a click's menu, and the gap between the panel and the
/// box it opens against.
pub const MENU_WIDTH: f32 = 280.0;
pub const MENU_GAP: f32 = 6.0;
/// The menu panel's own padding — smaller than [`crate::widgets::panel`]'s,
/// because a menu is a list of buttons and not a page.
pub const MENU_PADDING: f32 = 12.0;
/// The width every row in the menu takes, in pixels: the column less the
/// panel's padding and its one-pixel border. **Pixels and not a
/// percentage**, because inside a wrapping container a percentage has
/// nothing to resolve against while the container is being measured —
/// the decision pop-up's card rows learned that the expensive way.
pub const MENU_ENTRY: f32 = MENU_WIDTH - 2.0 * MENU_PADDING - 2.0;
/// The least room a side needs before a menu is opened into it: the
/// heading and about two rows. Below this the other side is tried, and
/// then the target itself is covered.
pub const MENU_MIN_ROOM: f32 = 120.0;

/// Where a menu opened against `over` sits: an inset from one horizontal
/// edge of the window and one vertical edge, and the box it may grow
/// into.
///
/// Exactly one of `left`/`right` and one of `top`/`bottom` is `Some`, and
/// the other is left `Val::Auto` — **that is the whole design**. The
/// panel is pinned by the edge that faces the card and grows *away* from
/// it, so its height is never needed. What this replaced computed a
/// `top` from an estimate of that height (a heading, and a 40px row per
/// entry); a menu whose labels wrapped was half as tall again, and the
/// difference went off the bottom of the window, where the screen root's
/// `Overflow::clip` ate it. The fallback branch also clamped on one side
/// only, so a menu taller than the window was given a negative `top`.
///
/// [`MenuBox::max_height`] is the room on the side chosen, which is what
/// makes the entries wrap into a second column instead of overflowing,
/// and `max_width` is the room from the anchored edge to the far one, so
/// those columns grow into the window and never past it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuBox {
    pub left: Option<f32>,
    pub right: Option<f32>,
    pub top: Option<f32>,
    pub bottom: Option<f32>,
    pub max_width: f32,
    pub max_height: f32,
}

impl MenuBox {
    /// The rectangle the panel may occupy — `(left, top, right, bottom)`
    /// in window pixels — which is what a test asserts against and what
    /// a reader of the numbers wants. The panel may of course be smaller
    /// than this; it can never be larger.
    pub fn bounds(&self, window: (f32, f32)) -> (f32, f32, f32, f32) {
        let left = self.left.unwrap_or_else(|| window.0 - self.right.unwrap_or(0.0) - self.max_width);
        let top = self.top.unwrap_or_else(|| window.1 - self.bottom.unwrap_or(0.0) - self.max_height);
        (left, top, left + self.max_width, top + self.max_height)
    }
}

/// Where a menu of `rows` rows opened against `over` goes.
///
/// **`rows` picks the side, and nothing else.** Above is preferred, as it
/// always has been — the common target is a card in the person's own
/// hand, along the bottom edge, and its menu belongs over it with the
/// card still in view beneath — so an estimate of the height decides
/// only *which* side is tried first. An estimate that is wrong now costs
/// a column, never an option: the anchored edge and [`MenuBox::max_height`]
/// are what keep the panel on the window, and neither is a guess.
///
/// The order is: above if the estimate fits there, else below if it fits
/// there, else whichever side has more room — and if neither side has
/// [`MENU_MIN_ROOM`] (a target as tall as the window: a server column)
/// the menu covers the target rather than leaving the
/// window, because a menu over the card is readable and a menu off the
/// screen is not.
pub fn menu_box(window: (f32, f32), over: Anchor, rows: usize) -> MenuBox {
    let (width, height) = (window.0.max(1.0), window.1.max(1.0));
    let inset = PADDING.min(width / 4.0).min(height / 4.0);
    // The same terms the placement used to be computed from, kept only to
    // choose a side: the panel's padding, the heading, the row gap, and a
    // single-line row per entry with the gaps between them.
    let rows = rows.max(1) as f32;
    let wanted = 2.0 * MENU_PADDING + LABEL + ROW_GAP + rows * 40.0 + (rows - 1.0) * ROW_GAP;
    let above = (over.top() - MENU_GAP - inset).max(0.0);
    let below = (height - over.bottom() - MENU_GAP - inset).max(0.0);

    let (top, bottom, max_height) = if wanted <= above || (above >= below && above >= MENU_MIN_ROOM) {
        // Pinned a gap above the card's top edge, growing upward.
        (None, Some((height - over.top() + MENU_GAP).max(inset)), above)
    } else if wanted <= below || below >= MENU_MIN_ROOM {
        // Pinned a gap below the card's bottom edge, growing downward.
        (Some((over.bottom() + MENU_GAP).max(inset)), None, below)
    } else {
        // Neither side has room: over the target, on the window.
        (Some(inset), None, height - 2.0 * inset)
    };

    // Sideways the menu is centred on the target as it always was, but
    // anchored by the edge that target is nearer, so extra columns grow
    // into the free half of the window instead of off the near edge.
    let (left, right, max_width) = if over.x <= width / 2.0 {
        let left = (over.x - MENU_WIDTH / 2.0).clamp(inset, (width - MENU_WIDTH - inset).max(inset));
        (Some(left), None, (width - left - inset).max(MENU_WIDTH))
    } else {
        let right = (width - over.x - MENU_WIDTH / 2.0).clamp(inset, (width - MENU_WIDTH - inset).max(inset));
        (None, Some(right), (width - right - inset).max(MENU_WIDTH))
    };

    // `min` before `max`, never `clamp`: on a window too small to hold
    // even one row the bounds cross, and `clamp` panics when they do.
    MenuBox { left, right, top, bottom, max_width, max_height: max_height.min(height - 2.0 * inset).max(1.0) }
}

/// Where a drop-down's open list goes, against the head it hangs from:
/// which side, and how large it may be. The list is sized to its
/// choices up to these bounds and scrolls past them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListBox {
    /// The list grows upward from the head's top edge rather than down
    /// from its bottom edge.
    pub opens_up: bool,
    pub max_height: f32,
    pub max_width: f32,
}

/// The height one row of a drop-down's list is estimated at, the list's
/// own padding included once — used only to choose a side, as
/// [`menu_box`] uses its estimate.
pub const LIST_ROW: f32 = 40.0;

/// Where the list of a drop-down with `rows` choices, hanging from
/// `head`, goes on a `window`.
///
/// **Below is preferred**, as a drop-down's name says, but only when the
/// whole list fits there. Otherwise it goes above if it fits there, and
/// otherwise to whichever side has more room, where it scrolls. The
/// height is always that side's room and never an estimate. So a
/// list can never run off the window: the deck lists sit at the foot of
/// the new-game form, and a list that always opened downward put every
/// deck past the first few below the window's edge, where nothing could
/// reach them.
pub fn list_box(window: (f32, f32), head: Anchor, rows: usize) -> ListBox {
    let (width, height) = (window.0.max(1.0), window.1.max(1.0));
    let inset = PADDING.min(width / 4.0).min(height / 4.0);
    let wanted = rows.max(1) as f32 * LIST_ROW;
    let below = (height - head.bottom() - MENU_GAP - inset).max(0.0);
    let above = (head.top() - MENU_GAP - inset).max(0.0);
    let opens_up = !(wanted <= below || (wanted > above && below >= above));
    let room = if opens_up { above } else { below };
    let left = head.x - head.width / 2.0;
    // `min` before `max`, never `clamp`: on a tiny window the bounds cross.
    ListBox { opens_up, max_height: room.min(height - 2.0 * inset).max(1.0), max_width: (width - left - inset).max(head.width).max(1.0) }
}

/// The advance one character is counted at, as a fraction of the font
/// size, when guessing how many lines a string will take.
///
/// **Deliberately wide** — Noto Sans runs about 0.5 em for lowercase and
/// 0.68 for capitals, and this sits at the top of that range — because
/// the two errors are not equal: a line counted short is a button pushed
/// off the window, and a line counted long is a card drawn a few pixels
/// narrower.
pub const WIDE_ADVANCE: f32 = 0.62;

/// How many lines `text` takes when it wraps in `width` at `size`.
///
/// An estimate and not a measurement: the layout has not run when the
/// decision pop-up is built, and word-boundary wrapping can leave most
/// of a line empty before a long word. So this is the pop-up's first
/// guess at its own chrome, never its guarantee — the panel is capped at
/// the window and its cards give first, which is what actually keeps
/// every button on screen.
pub fn wrapped_lines(text: &str, width: f32, size: f32) -> usize {
    if text.is_empty() {
        return 0;
    }
    let per_line = (width / (size * WIDE_ADVANCE)).floor().max(1.0);
    text.lines().map(|line| (line.chars().count() as f32 / per_line).ceil().max(1.0) as usize).sum()
}

#[cfg(test)]
mod tests {

    /// The avatar bar never moves with the game: its disc and its plate
    /// are the same at every face width and every number of servers, and
    /// the far side's is the near side's at the opponent's scale.
    #[test]
    fn the_avatar_bar_is_a_constant_of_the_chair() {
        use netrunner_core::rules::Side;
        for chair in [Side::Corp, Side::Runner] {
            let mine = (avatar_size(chair, chair), bar_height(chair, chair));
            let theirs = (avatar_size(chair.other(), chair), bar_height(chair.other(), chair));
            assert_eq!(mine, (AVATAR, BAR));
            assert!(theirs.0 < mine.0 && theirs.1 < mine.1, "the far side's is smaller");
            assert!(mine.1 < mine.0, "the disc stands proud of the plate");
            // The edge's height moves with the hand's peek, never the bar.
            let narrow = strip_height(chair, chair, MIN_FACE);
            let wide = strip_height(chair, chair, MAX_FACE);
            assert_eq!(wide - narrow, (PEEK * 1.4 * MAX_FACE).round() - (PEEK * 1.4 * MIN_FACE).round());
        }
    }

    /// An avatar's crop is a square inside the scan, whatever its size.
    #[test]
    fn an_avatar_is_a_square_of_the_scan_it_is_cut_from() {
        for size in [(300.0, 419.0), (750.0, 1047.0), (180.0, 251.0), (400.0, 300.0)] {
            let [x0, y0, x1, y1] = avatar_crop(size);
            assert!((x1 - x0 - (y1 - y0)).abs() < 1e-3, "square: {size:?}");
            assert!(x0 >= 0.0 && y0 >= 0.0 && x1 <= size.0 + 1e-3 && y1 <= size.1 + 1e-3, "inside: {size:?}");
        }
        // On a card, the art above the text box: centred, and above 0.63.
        let [x0, _, x1, y1] = avatar_crop((300.0, 419.0));
        assert!((x0 + x1 - 300.0).abs() < 1e-3);
        assert!(y1 / 419.0 < IDENTITY_ART);
    }

    /// A deck builder row is filled by whole cards no narrower than
    /// `DECK_FACE`, and a row too narrow for one still gets a card.
    #[test]
    fn a_deck_grid_row_is_filled_by_whole_cards() {
        for width in [600.0, 1286.0, 1540.0, 1966.0, 2200.0] {
            let face = deck_face(width);
            let across = ((width + DECK_GAP) / (face + DECK_GAP)).floor();
            assert!((DECK_FACE..=DECK_FACE_MAX).contains(&face), "{width}: {face}");
            assert!(across * face + (across - 1.0) * DECK_GAP <= width, "{width}: {face}");
            assert!(width - (across * (face + DECK_GAP) - DECK_GAP) < across + DECK_GAP, "{width}: a row ends within a few pixels of its edge");
        }
        assert!(deck_face(200.0) >= 120.0);
    }

    use super::*;

    /// Two cards drawn by Top-Down Solutions, and a hand of nine, each fit
    /// the window they are offered in: widest at one row while that is
    /// the wider, wrapping only when the cards would otherwise shrink.
    #[test]
    fn a_pile_sheet_holds_whole_rows_and_fits_the_window() {
        let row = PILE_FACE * 1.4;
        for height in [520.0, 768.0, 800.0, 1080.0, 1600.0, 2400.0] {
            let box_height = pile_height(height);
            assert!(box_height + PILE_CHROME <= height, "{height}: the sheet fits the window");
            let rows = (box_height + PILE_GAP) / (row + PILE_GAP);
            assert!((rows - rows.round()).abs() < 1e-3, "{height}: whole rows, not {rows}");
            assert!(rows <= PILE_ROWS, "{height}: never more than {PILE_ROWS} rows before the scroll bar");
            assert!(rows == PILE_ROWS || box_height + PILE_GAP + row + PILE_GAP + PILE_CHROME > height, "{height}: no room left for another row");
        }
        assert_eq!(pile_height(2400.0), PILE_ROWS * row + (PILE_ROWS - 1.0) * PILE_GAP, "a tall window stops at three rows");
        assert_eq!(pile_height(800.0), 2.0 * row + PILE_GAP, "a laptop shows two rows");
        assert_eq!(pile_height(300.0), 170.0, "too short for a row: what is left, and it scrolls");
    }

    #[test]
    fn the_opening_box_reads_its_cards_without_filling_the_window() {
        // A 2000 × 1250 window, a five-card hand: the cap, and one row.
        assert_eq!(opening_faces((1900.0, 900.0), 5), (OPENING_FACE_MAX, 5));
        // A small window still takes height off the cards.
        let (face, _) = opening_faces((900.0, 500.0), 5);
        assert!(face < OPENING_FACE_MAX && 2.0 * face * 1.4 + OPENING_CAPTION < 500.0, "{face}");
    }

    #[test]
    fn choice_faces_fit_the_pop_up_and_wrap_only_to_grow() {
        let (face, per_row) = choice_faces((1900.0, 900.0), 2);
        assert_eq!((face, per_row), (CHOICE_FACE_MAX, 2), "two cards at full size side by side");
        let (face, per_row) = choice_faces((1900.0, 900.0), 9);
        assert!(per_row < 9, "nine in one row would be {} wide", (1900.0 - 8.0 * CHOICE_GAP) / 9.0);
        let rows = 9usize.div_ceil(per_row) as f32;
        assert!(per_row as f32 * face + (per_row as f32 - 1.0) * CHOICE_GAP <= 1900.0);
        assert!(rows * (face * 1.4 + CHOICE_CAPTION) + (rows - 1.0) * CHOICE_GAP <= 900.0);
        let (small, _) = choice_faces((700.0, 500.0), 9);
        assert!(small < face && small >= 1.0, "a small window shrinks the cards, never the count");
    }

    /// Item 21's check, at the smallest window the board supports: the
    /// card sheet at the side covers the right column and only a sliver
    /// of the board's right edge, where centred it sat over the middle
    /// of the field. The wider sheets reach further in, but every one of
    /// them leaves the board's left side clear, and none leaves the
    /// window.
    #[test]
    fn a_reading_surface_sits_over_the_right_column_not_the_field() {
        for width in [1280.0, 1366.0, 1920.0] {
            let board_right = PADDING + board_width(width);
            let rail_left = width - PADDING - RAIL_WIDTH;
            let card = Placement::Side.left(width, SHEET_CARD);
            assert!(card <= rail_left, "the card sheet covers the whole right column at {width}");
            assert!(board_right - card <= 40.0, "and at most a sliver of the board at {width}: {}", board_right - card);
            assert_eq!(card + SHEET_CARD, rail_left + RAIL_WIDTH, "its right edge is the column's");
            let centred = Placement::Centre.left(width, SHEET_CARD);
            assert!(centred + SHEET_CARD < board_right, "centred, the same card sat over the field at {width}");
            for sheet in [SHEET_CARD, SHEET_INSTALL, SHEET_ZONE] {
                let left = Placement::Side.left(width, sheet);
                assert!(left > PADDING, "the board's left edge stays clear at {width} for a {sheet} sheet");
                assert!(left + sheet <= width, "and the sheet stays in the window");
            }
        }
        assert!(Placement::Side.wash_alpha() < Placement::Centre.wash_alpha());
        assert_eq!(Placement::of(true), Placement::Side);
        assert_eq!(Placement::of(false), Placement::Centre);
    }

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

    /// Three peeked rows cost the board no more than the one row of whole
    /// cards they replaced, so splitting the rig shrank no card: at most
    /// one row gap and one chip line more than a card, at any width.
    #[test]
    fn three_peeked_rig_rows_are_about_one_card_tall() {
        for face in [MIN_FACE, 119.0, 165.0, MAX_FACE] {
            let one_row_of_whole_cards = 1.4 * face + CHIPS;
            let rig = rig_height(face);
            assert!(rig >= 1.4 * face, "{face}: {rig}");
            assert!(rig <= one_row_of_whole_cards + 2.0 * CHIPS + 2.0 * RIG_ROW_GAP + 2.0, "{face}: {rig}");
        }
        // And it never falls as the face grows, which `face_width`'s
        // search needs.
        for face in MIN_FACE as u32..MAX_FACE as u32 {
            assert!(rig_height(face as f32) <= rig_height(face as f32 + 1.0), "{face}");
        }
        assert!(rig_height(MIN_FACE) < rig_height(MAX_FACE));
    }

    #[test]
    fn the_face_shrinks_with_the_window_and_stays_in_range() {
        let none = Counts { servers: 4, human_is_runner: true };
        let large = face_width((1920.0, 1080.0), none);
        let small = face_width((1280.0, 800.0), none);
        assert!(large > small, "{large} > {small}");
        assert_eq!(face_width((800.0, 400.0), none), MIN_FACE, "never below the floor");
        assert_eq!(face_width((4000.0, 3000.0), none), MAX_FACE, "never above the cap");
        // Nine servers across a laptop width cap it below what the
        // height would allow.
        assert!(face_width((1280.0, 1080.0), Counts { servers: 9, ..none }) < face_width((1280.0, 1080.0), none));
        // The board is the window's whole height: nothing above the
        // opponent's hand and nothing under the person's.
        assert_eq!(board_height(900.0), 900.0);
    }

    /// The rule the whole field exists for: the ICE and the rig are not
    /// counts, so nothing the opponent installs moves a card. The face
    /// width is a function of the window, the chair and the servers
    /// alone — `Counts` has nowhere to put an ICE count.
    #[test]
    fn the_far_side_is_smaller_by_one_constant() {
        for chair in [Side::Corp, Side::Runner] {
            assert_eq!(area_face(chair, chair, 200.0), 200.0);
            assert_eq!(area_face(chair.other(), chair, 200.0), 150.0);
        }
        // From the Runner's chair the rig is the near row and the plates
        // the far one; from the Corp's the reverse. Either way the near
        // side costs more of the height.
        let runner = Counts { servers: 4, human_is_runner: true };
        let corp = Counts { human_is_runner: false, ..runner };
        assert!(rig_height(area_face(Side::Runner, Side::Runner, 200.0)) > rig_height(area_face(Side::Runner, Side::Corp, 200.0)));
        assert!(fixed_height(200.0, runner) > fixed_height(200.0, corp), "a full-size rig is taller than full-size plates");
    }

    /// The rows at the computed width leave the ICE field its minimum, and
    /// one pixel more would not: the invariant the module exists for.
    #[test]
    fn the_rows_at_the_computed_width_fit_the_window_and_no_wider_would() {
        for (window, runner) in [((1280.0, 800.0), true), ((1280.0, 800.0), false), ((1920.0, 1080.0), true), ((1366.0, 768.0), true), ((2560.0, 1440.0), false), ((2000.0, 1250.0), true)] {
            let counts = Counts { servers: 5, human_is_runner: runner };
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

    /// The window the headless tests run in, and the one a menu is
    /// placed in below unless the test says otherwise.
    const WINDOW: (f32, f32) = (1280.0, 800.0);

    /// A card in the person's own hand: the menu is pinned a gap above
    /// the card's top edge and grows upward from there, which is where
    /// §4a put it and where it stays.
    #[test]
    fn a_menu_over_a_hand_card_opens_upward_from_the_card() {
        let card = Anchor { x: 600.0, y: 700.0, width: 120.0, height: 168.0 };
        let menu = menu_box(WINDOW, card, 3);
        assert!(menu.top.is_none(), "it is not placed by its top edge");
        let bottom = menu.bottom.expect("it is pinned by its bottom edge");
        assert_eq!(WINDOW.1 - bottom, card.top() - MENU_GAP, "its bottom edge is a gap above the card");
        assert_eq!(menu.max_height, card.top() - MENU_GAP - PADDING, "it may grow to the top of the window");
        let (left, top, right, down) = menu.bounds(WINDOW);
        assert!(left >= PADDING && top >= PADDING && right <= WINDOW.0 - PADDING && down <= WINDOW.1 - PADDING);
    }

    /// The opponent's hand hangs from the window's top edge, so its
    /// cards have no room above them: the menu flips below and is still
    /// on the window.
    #[test]
    fn a_menu_over_the_top_edge_flips_below_and_stays_on_the_window() {
        let card = Anchor { x: 600.0, y: 40.0, width: 120.0, height: 80.0 };
        let menu = menu_box(WINDOW, card, 4);
        assert!(menu.bottom.is_none(), "it is not placed by its bottom edge");
        assert_eq!(menu.top, Some(card.bottom() + MENU_GAP), "it hangs a gap under the card");
        let (_, top, _, down) = menu.bounds(WINDOW);
        assert!(top >= PADDING && down <= WINDOW.1 - PADDING, "top {top}, bottom {down}");
    }

    /// The bug this answers, in the shape it took: a menu of twelve
    /// entries under a card near the top of the window. The placement it
    /// replaced computed a `top` from the rows and subtracted it from
    /// the window with no floor, so `top` went negative and the screen
    /// root's clip ate the first options; the room a side has is now a
    /// cap, and the entries wrap into it.
    #[test]
    fn a_tall_menu_below_a_card_is_capped_rather_than_pushed_off() {
        let card = Anchor { x: 400.0, y: 120.0, width: 120.0, height: 168.0 };
        let menu = menu_box(WINDOW, card, 12);
        let (_, top, _, down) = menu.bounds(WINDOW);
        assert!(top >= PADDING, "the old math put this at {top}");
        assert!(down <= WINDOW.1 - PADDING);
        assert!(menu.max_height <= WINDOW.1 - 2.0 * PADDING);
    }

    /// A card at the right-hand edge: the panel is anchored by the edge
    /// it is near and grows inward, so a second column opens into the
    /// window rather than off it.
    #[test]
    fn a_menu_near_an_edge_is_anchored_by_that_edge_and_grows_inward() {
        let card = Anchor { x: WINDOW.0 - 40.0, y: 400.0, width: 120.0, height: 168.0 };
        let menu = menu_box(WINDOW, card, 6);
        assert!(menu.left.is_none(), "it is placed by the edge it is nearest");
        let right = menu.right.expect("pinned to the right");
        assert!(right >= PADDING, "{right}");
        assert!(menu.max_width >= MENU_WIDTH, "there is room for at least the one column");
        let (left, _, r, _) = menu.bounds(WINDOW);
        assert!(left >= PADDING && r <= WINDOW.0 - PADDING);

        let near = Anchor { x: 20.0, y: 400.0, width: 120.0, height: 168.0 };
        let menu = menu_box(WINDOW, near, 6);
        assert_eq!(menu.right, None, "and by the left edge on the other side");
        assert!(menu.left.is_some_and(|left| left >= PADDING));
    }

    /// A target as tall as the window — a server column —
    /// has room on neither side. The menu covers it rather than leaving
    /// the window: a menu over the card is readable, one off the screen
    /// is not.
    #[test]
    fn a_target_taller_than_the_window_gets_the_menu_over_it() {
        let column = Anchor { x: 300.0, y: 400.0, width: 160.0, height: 780.0 };
        let menu = menu_box(WINDOW, column, 5);
        assert_eq!(menu.top, Some(PADDING));
        assert_eq!(menu.max_height, WINDOW.1 - 2.0 * PADDING);
    }

    /// The property the old placement failed on its first day: whatever
    /// the window and whatever box the menu was opened against, the box
    /// it may occupy is inside that window, and no number it returns is
    /// negative or NaN.
    #[test]
    fn no_anchor_and_no_window_puts_a_menu_off_the_screen() {
        for window in [(1280.0, 800.0), (1920.0, 1080.0), (900.0, 520.0)] {
            for column in 0..16 {
                for row in 0..16 {
                    let over = Anchor {
                        x: window.0 * column as f32 / 15.0,
                        y: window.1 * row as f32 / 15.0,
                        // Every size from a zero-height header to a box
                        // that spans the window.
                        width: 160.0 * column as f32 / 15.0,
                        height: window.1 * row as f32 / 15.0,
                    };
                    for rows in [1usize, 3, 12, 40] {
                        let menu = menu_box(window, over, rows);
                        let (left, top, right, bottom) = menu.bounds(window);
                        let at = format!("{window:?} over {over:?} with {rows} rows");
                        assert!(menu.left.is_some() != menu.right.is_some(), "one horizontal edge, at {at}");
                        assert!(menu.top.is_some() != menu.bottom.is_some(), "one vertical edge, at {at}");
                        for number in [left, top, right, bottom, menu.max_width, menu.max_height] {
                            assert!(number.is_finite(), "{number} at {at}");
                        }
                        assert!(left >= 0.0 && top >= 0.0, "({left}, {top}) at {at}");
                        assert!(right <= window.0 + 1e-3, "right {right} at {at}");
                        assert!(bottom <= window.1 + 1e-3, "bottom {bottom} at {at}");
                    }
                }
            }
        }
    }

    /// The pop-up's guess at its own words. It may over-count — that
    /// costs a card a few pixels — but never under-count, which would
    /// cost a button its place on the window.
    #[test]
    fn wrapped_lines_never_under_counts_a_line() {
        assert_eq!(wrapped_lines("", 400.0, 18.0), 0, "nothing takes no room");
        assert_eq!(wrapped_lines("Steal", 400.0, 18.0), 1);
        assert_eq!(wrapped_lines("one\ntwo\nthree", 400.0, 18.0), 3, "a line each");
        let long = "Install protecting Server 1 and pay the advancement cost";
        let narrow = wrapped_lines(long, 120.0, 18.0);
        assert!(narrow > 1, "it wraps at 120px");
        assert!(narrow >= wrapped_lines(long, 400.0, 18.0), "wider is never more lines");
        // The count is at least what a generous 0.5 em advance would give.
        let widest = (long.chars().count() as f32 * 18.0 * 0.5 / 120.0).floor() as usize;
        assert!(narrow >= widest, "{narrow} lines for {widest} lines of glyphs");
    }

    /// The encounter panel's art takes what the words leave: all of it on
    /// a tall window, a whole-scaled thumbnail on a short one, nothing at
    /// all once that would be a smudge — and never more than its own size.
    #[test]
    fn the_encounter_art_gives_first_and_keeps_its_shape() {
        let natural = (356.0, 262.0);
        let lines = ["Encountering · HQ", "Brân 1.0", "Ice: Barrier - Bioroid", "Strength 6", "[ ] You may install 1 piece of ice from HQ or Archives directly inward from this ice, ignoring all costs.", "[ ] End the run.", "[ ] End the run."];
        assert_eq!(encounter_art(natural, 1600.0, 280.0, lines), Some(natural), "a tall window draws it whole");
        let (width, height) = encounter_art(natural, 768.0, 170.0, lines).expect("room for a thumbnail");
        assert!(height < natural.1 && height >= ENCOUNTER_ART_MIN, "{height}");
        assert!((width / height - natural.0 / natural.1).abs() < 0.02, "scaled whole, not squeezed: {width}×{height}");
        assert_eq!(encounter_art(natural, 768.0, 280.0, lines), None, "the phase panel on a laptop leaves no room worth drawing in");
        let more = lines.into_iter().chain(["[ ] End the run."; 4]);
        assert!(encounter_art(natural, 1600.0, 280.0, more).is_some_and(|(_, h)| h <= natural.1));
    }
    #[test]
    fn a_list_at_the_foot_of_the_window_opens_upward_and_stays_on_it() {
        let window = (1280.0, 800.0);
        let head = Anchor { x: 400.0, y: 700.0, width: 400.0, height: 44.0 };
        let list = list_box(window, head, 20);
        assert!(list.opens_up);
        assert!(head.top() - MENU_GAP - list.max_height >= 0.0, "{list:?} runs off the top");
    }

    #[test]
    fn a_list_near_the_top_opens_downward_and_stays_on_it() {
        let window = (1280.0, 800.0);
        let head = Anchor { x: 400.0, y: 60.0, width: 400.0, height: 44.0 };
        let list = list_box(window, head, 40);
        assert!(!list.opens_up);
        assert!(head.bottom() + MENU_GAP + list.max_height <= window.1, "{list:?} runs off the bottom");
    }

    #[test]
    fn a_short_list_opens_downward_whenever_it_fits() {
        let head = Anchor { x: 400.0, y: 600.0, width: 400.0, height: 44.0 };
        assert!(!list_box((1280.0, 800.0), head, 3).opens_up);
    }

    #[test]
    fn a_list_is_never_wider_than_the_room_to_the_right() {
        let head = Anchor { x: 1100.0, y: 300.0, width: 300.0, height: 44.0 };
        let list = list_box((1280.0, 800.0), head, 5);
        assert!(head.x - head.width / 2.0 + list.max_width <= 1280.0);
    }

    #[test]
    fn a_tiny_window_places_a_list_without_panicking() {
        for window in [(0.0, 0.0), (10.0, 10.0), (200.0, 60.0)] {
            let list = list_box(window, Anchor { x: 5.0, y: 5.0, width: 100.0, height: 44.0 }, 50);
            assert!(list.max_height >= 1.0 && list.max_width >= 1.0);
        }
    }

}
