//! The board: the human's masked view of the match, drawn; a control bar
//! of the basic actions, greyed when the engine does not list them; the
//! cards and the zones as the way into everything else — a click opens a
//! sheet of what may be done there, and never acts by itself; the
//! prompt's decisions under the prompt; and, when the person turns them
//! on, the flat panel of every legal action and the match log.
//!
//! **Where the match is.** On its own thread, behind the [`ActiveMatch`]
//! the new-game form left: `poll` drains its messages once a frame into
//! `models::game::Game` intents, and a chosen entry goes back through
//! `MatchHandle::submit`, unfiltered. The screen never holds a
//! `GameState` and never re-derives legality; it draws the view and
//! hands back what the person chose from the engine's own list.
//!
//! **Layout, and the rule it obeys: the board never scrolls.** The
//! window is fullscreen and the board fits it — a static card size put
//! the person's own hand below the fold twice, and a scroll container
//! is not a fix for a card table. `models::layout::face_width` computes
//! the card width the window has room for from the window's size, the
//! chair and the number of servers — never from what is installed, so
//! nothing the opponent does moves a card — `fit` recomputes it whenever
//! one of those changes, and a row that is still too wide overlaps its
//! cards like a held hand (`layout::step`) rather than wrapping or
//! scrolling. The board runs the window's full height beside one right
//! column: nothing above the opponent's hand and nothing under the
//! person's. Top to bottom on the board: the opponent's strip and the
//! bottom of their hand as backs, hung from the window's top edge; their
//! area and the person's with the run lane between; the control bar (`board::Control::for_side`, one button each,
//! always in the same place) directly above the person's hand, then the
//! person's strip and the top of their hand on the window's bottom edge.
//! The right column is the status line with Quit and the gear, the phase
//! panel (`board::phase`, hidden with L), the Runner's identity while a
//! run is on, the rail and the log. The opponent's side is
//! drawn at `layout::OPPONENT_SCALE` of the person's own. The Corp's
//! servers are the area that grows: each a column with its plate on the
//! Corp's edge of the table and its ICE as tiles out toward the Runner —
//! bars rather than rotated cards, since a rotated `UiTransform` is laid
//! out as its unrotated box and would overlap its neighbours — sharing
//! the ICE field's height (`layout::tile_stack`). The Runner's rig is one
//! row of three groups in a row reserved whether or not anything is in
//! it, with the stack and the heap as buttons in the
//! Runner's strip. The rail is the prompt (`board::Prompt`, the card's
//! own words), the decisions the prompt is asking
//! (`ActionMap::decisions`), the flat panel if the play helper is on,
//! and the log if the play history is. Everything under the board root
//! is respawned when the view moves, which is at most once per applied
//! action, and when the fit changes; the bar, the rail and the overlay
//! are respawned on their own, so a sheet opening does not redraw the
//! board.
//!
//! **Sheets are for reading.** A card's sheet is the Large face — the
//! picture, or the text layout while no picture is cached — and, for an
//! installed card, its state beside it; a zone's sheet is its contents
//! where the viewer may see them — Archives as every card for the Corp
//! and as the face-up ones with backs for the rest for the Runner, the
//! heap, the Corp's own HQ, a remote's ICE and root — each face a button
//! that reads the card over the sheet. R&D and the stack are backs and a
//! count: a deck's order is never shown, even to its owner.
//!
//! **A click is a menu above the card; a secondary click is its sheet.**
//! The primary button on a card or a zone opens its actions as a small
//! panel sitting just above the node that was clicked, centred on it, so
//! the card stays in view under its own menu (`models::game::Menu`,
//! anchored by the node's laid-out box, not the pointer — a menu at the
//! pointer landed somewhere different on every click). The right button,
//! or the primary with Ctrl or Cmd held (a Mac's one-button click),
//! opens the sheet to read. `Interaction` reports only the primary
//! button, so `board_click` reads the right button from
//! `ButtonInput<MouseButton>` and takes the target from the node the
//! focus system marks hovered — every card, ICE bar and header is a
//! `Button`, which blocks, so the topmost is the one under the pointer.
//! A primary click that lands on nothing of the menu's closes it, and
//! so does the board moving; neither opens through an overlay, whose
//! ground passes hovers to the board beneath.
//!
//! **A hand card is dragged into place** (`models::drag`). A press on a
//! hand card is armed rather than acted on: released where it started it
//! is the click that opens the card's menu, and released after six pixels
//! of travel it is a drag, which drops the card between the two cards the
//! pointer is over (`models::game::HandOrder`, the person's own order for
//! a zone the rules give no order). Every other target still acts on the
//! press. `drag_hand` reads the pointer from `CursorMoved` rather than the
//! window, so the headless tests can drive it.
//!
//! **Keys are the buttons pressed another way** (`models::shortcuts`):
//! `shortcuts` reads the keyboard messages by the character a key types,
//! hands the model what it can answer, and acts itself only on the two it
//! cannot — the hovered card's menu and reading, and the play helper
//! setting. `?` or F1 lists them over the board.
//!
//! **Highlights come from `board::diff`.** After a redraw, every install
//! and hand card a `Transition` names is outlined for that redraw, and
//! nothing is ever inferred from the previous frame's nodes. §4 turns
//! the same transitions into movement and sound.
//!
//! **A run is shown a beat at a time.** The match's messages go through
//! `models::pace::Pacer` rather than straight into the model: in a run
//! each event that moves the run is released on its own beat, the
//! model's `RunTrail` observes it and the run lane between the two
//! areas redraws (`relane`, without the board), and the message itself
//! — the board — lands last. The column under run keeps its border in
//! the Runner's colour; a line from the lane to it was drawn and
//! dropped the same day, on the person's word that it was ugly.

use bevy::input::keyboard::{Key as BevyKey, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;

use netrunner_client::access::Access;
use netrunner_client::board::action_map::server_name;
use netrunner_client::board::{facts, hud, Affordance, Control, IceState, Outcome as RunOutcome, Pile, Prompt, Stage, Target, Token, TokenKind, Transition, Zone};
use netrunner_client::card_face::Face;
use netrunner_core::dsl::{CardId, CardType};
use netrunner_core::rules::{GamePhase, InstallId, InstallSlot, PendingDecision, PlayerAction, RunPhase, ServerId, Side, SubroutineStatus};
use netrunner_core::view::{ClientView, ServerView};

use crate::card_images::{CardImages, WantsImage};
use crate::core::{ClientCore, Notices};
use crate::models::game::{Anchor, Game, Intent, MatchMessageRef, Outcome};
use crate::board_art::{self, BoardArt};
use crate::models::layout::{self, Counts, Depth};
use crate::models::pace::{Beat, Pacer};
use crate::models::settings::{self as settings_model, Row};
use crate::models::shortcuts::{self, Shortcut};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::screens::new_game::{ActiveMatch, LastGame};
use crate::screens::replay::{ActiveReplay, OpenReplay, ReplayClick};
use crate::screens::settings::{self as settings_screen, Control as SettingsControl};
use crate::screens::AppScreen;
use crate::skin::{self, Drawn, Slot};
use crate::table;
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_back, spawn_face, FaceSize};
use crate::widgets::{self, Pressed};

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // `CursorMoved` is `WindowPlugin`'s, and the headless tests run
        // without one; registering it here is a no-op when the window
        // plugin already has, and lets a test drive a drag by message.
        app.add_message::<CursorMoved>()
            .init_resource::<Pending>()
            .init_resource::<table::LastTable>()
            .init_resource::<Pointer>()
            .add_systems(OnEnter(AppScreen::Game), spawn)
            .add_systems(OnExit(AppScreen::Game), leave)
            .add_systems(Update, (poll, autoplay, escape.in_set(Captures), board_click, drag_hand, shortcuts, controls, fit, board_pictures, side_panels, relane, redraw, lift_hovered, table_guide).chain().run_if(in_state(AppScreen::Game)))
            // Its own registration rather than a link in that chain: it
            // has no ordering requirement against any of them, and adding
            // a system to an existing `.chain()` reorders everything after
            // it (§4n moved `button_feedback` that way and broke twelve
            // board tests).
            .add_systems(Update, shadows.run_if(in_state(AppScreen::Game)))
            // Likewise its own line: it reads what the chain wrote and
            // orders against none of it, and a menu placed a frame late
            // is a menu that was on the window the whole time.
            .add_systems(Update, place_menu.run_if(in_state(AppScreen::Game)));
    }
}

/// What a button on the board means.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum Click {
    Target(Target),
    /// An entry of the action map.
    Entry(usize),
    /// A route through the encountered ICE (`Game::breaks`).
    Break(usize),
    /// Take the last move back (`Game::back_label`).
    TakeBack,
    /// A control-bar button.
    Control(Control),
    /// A face in a zone sheet: read the card over the sheet.
    Inspect(CardId),
    /// A row of a list sheet (the score area): open or close its details.
    Expand(usize),
    /// The gear.
    Options,
    /// Save the match so far as a file that replays (`save_report`).
    SaveReport,
    /// Open the report just saved on the replay board.
    WatchReport,
    CloseOverlay,
    ConfirmQuit,
    CancelQuit,
    /// Escape's button: the quit prompt.
    Quit,
    PlayAgain,
    Menu,
    Back,
}

/// The board, respawned when the view moves.
#[derive(Component)]
pub struct Board;

/// The run lane between the two areas, refilled on a beat without the
/// board.
#[derive(Component)]
pub struct RunLane;
/// A server's column, for a test that reads what a column holds.
#[derive(Component)]
pub struct ServerColumn(pub ServerId);

/// A server's plate, on the Corp's edge of its column: the box its name
/// and its picture sit in.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerPlate(pub ServerId);

/// The picture on a server's plate.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatePicture(pub ServerId);

/// The picture behind a tile's words, and the key it was drawn from.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct TileArt(pub &'static str);

/// A token's badge — on a tile or under a rig card — and the key of the
/// glyph it shows.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct TokenBadge(pub &'static str);
/// Where the pointer was last seen, in logical window pixels: read off
/// `CursorMoved`, because the window's own `cursor_position` needs a
/// window and the headless tests have none.
#[derive(Resource, Default)]
pub struct Pointer(pub (f32, f32));

/// A place a dragged card may be dropped: the node's own box is the
/// target, so the drop is hit-tested against what is laid out rather than
/// against whatever the focus system last marked hovered.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct DropPlace(pub Target);

/// A card's place in the person's own hand, for a drag to read.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandSlot(pub usize);

/// The phase panel in the right column, and one step chip on it, for a
/// test to read. Hidden rather than despawned when the setting is off.
#[derive(Component)]
pub struct PhaseBarRow;

/// The right column's panel that shows the Runner's identity while a run
/// is on; empty, and taking no room, when none is.
#[derive(Component)]
pub struct RunIdentity;

/// The Runner's name in the run panel, for a test to read.
#[derive(Component)]
pub struct RunIdentityName;

/// The cropped scan in the run panel, when one is cached, and the copy
/// it was drawn from: a smaller copy stands in until the panel's own
/// width has decoded, and the panel is redrawn when it has.
#[derive(Component)]
pub struct RunIdentityArt(Handle<Image>);

#[derive(Component)]
pub struct PhaseStep(pub netrunner_client::board::phase::State);

/// A row of the list of keys, for a test to count.
#[derive(Component)]
pub struct HelpRow;

/// One line of an install's state on its sheet, for a test to read.
#[derive(Component)]
pub struct InstallFact;
/// A side's HUD, for a test that reads where its numbers are.
#[derive(Component)]
pub struct HudPanel(pub Side);
/// One number on a HUD, as `hud::readouts` gave it, for a test to read.
#[derive(Component, Debug, Clone)]
pub struct HudReadout {
    pub label: &'static str,
    pub value: String,
}
/// A row of the score-area sheet, by position, for a test to press.
#[derive(Component)]
pub struct ScoreRow(pub usize);
/// The open row's details, by position, for a test to find.
#[derive(Component)]
pub struct ScoreDetails(pub usize);

/// The pacer the match's messages go through.
#[derive(Resource)]
pub struct Pace(pub Pacer);
/// The prompt, its decisions and the flat panel, respawned when the view
/// moves or a submit closes it.
#[derive(Component)]
pub struct Rail;
/// The control bar, respawned with the rail: it is a function of the
/// action map.
#[derive(Component)]
pub struct ControlBar;
/// The action panel's scroll column, inside the rail.
#[derive(Component)]
struct ActionList;
#[derive(Component)]
struct LogList;
#[derive(Component)]
struct LogScroll;
/// The log and its scrollbar; shown or hidden by the play history
/// preference, never despawned.
#[derive(Component)]
pub struct LogRow;
/// The full-window overlay, when one is up.
#[derive(Component)]
pub struct Overlay;
/// The decision the game is waiting on — the mulligan, an access, a
/// trace bid, a choice a card asks — as a pop-up in the middle of the
/// screen, where the eyes are, rather than buttons on the rail. It sits
/// over the board and does not block it: a card can still be read, and
/// a sheet opens above it. Respawned with the rail.
#[derive(Component)]
pub struct DecisionPopup;
/// A card the decision pop-up draws — a selection's candidate, the card an
/// install is placing, the card asking or the card accessed — which a
/// secondary click reads in the card sheet, as it would on the board.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ChoiceCard(pub CardId);
/// A click's menu of a target's actions, above its card.
/// Respawned with the rail, like the pop-up; over it and under the
/// overlays.
#[derive(Component)]
pub struct ActionsMenu;
/// The menu's panel and each of its buttons: a primary click that
/// presses none of these closes the menu.
#[derive(Component)]
struct MenuPart;
#[derive(Component)]
pub struct StatusLine;

#[derive(Resource)]
pub struct Model(pub Game);

#[derive(Resource, Default)]
pub(crate) struct Dirty {
    board: bool,
    rail: bool,
    log: bool,
    overlay: bool,
    /// The run lane alone: a beat of the trail, with the board still.
    lane: bool,
    /// The right column's status line, phase panel and run panel, which
    /// follow the board and the lane and also a setting.
    side: bool,
}

impl Dirty {
    pub(crate) fn all(&mut self) {
        self.board = true;
        self.rail = true;
        self.log = true;
        self.overlay = true;
        self.lane = true;
        self.side = true;
    }
}

/// Intents raised by a key, applied by `controls` with the pressed ones
/// so every outcome is handled in one place.
#[derive(Resource, Default)]
pub(crate) struct Pending(Vec<Intent>);

/// The card width the board is drawn at, and the window it was computed
/// for. `fit` keeps it current; a change redraws the board.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct BoardFit {
    /// The person's own face width; the opponent's side is drawn at
    /// `layout::OPPONENT_SCALE` of it.
    pub face: f32,
    pub window: Vec2,
    /// The chair the width was computed for.
    pub chair: Side,
    /// The ICE field's height at this width: what the fixed rows leave.
    pub field: f32,
}

impl Default for BoardFit {
    fn default() -> Self {
        // The headless tests' window: no `Window` exists there.
        Self { face: 0.0, window: Vec2::new(1280.0, 800.0), chair: Side::Runner, field: 0.0 }
    }
}

impl BoardFit {
    /// The face width a side's cards are drawn at from this chair.
    fn area_face(&self, side: Side) -> f32 {
        layout::area_face(side, self.chair, self.face)
    }

    /// A side's cards, at the size their side of the table is drawn.
    fn size_of(&self, side: Side) -> FaceSize {
        FaceSize::Board(self.area_face(side).round() as u16)
    }

    /// A side's identity in its strip.
    fn identity_size(&self, side: Side) -> FaceSize {
        FaceSize::Board((self.area_face(side) * layout::IDENTITY_SCALE).round() as u16)
    }

    fn board_width(&self) -> f32 {
        layout::board_width(self.window.x)
    }
}

/// What the view puts on the board, for the fit.
fn counts(game: &Game) -> Counts {
    let human_is_runner = game.side == Side::Runner;
    // The three centrals are always drawn.
    let remotes = game.view.as_ref().map_or(0, |view| view.corp.servers.iter().filter(|s| matches!(s.server, ServerId::Remote(_))).count());
    Counts { servers: 3 + remotes, human_is_runner }
}

/// Loads the board's pictures (`board_art`) for the skin in use and the
/// Corp's faction, and again when either changes, redrawing the board so
/// a new skin's buildings replace the old ones without leaving the
/// screen. The faction is the Corp identity's, which is public to both
/// chairs (`CorpClientView::identity`), so it is known from the first
/// view and fixed for the match. Loading decodes files, so it happens on
/// a change and never per frame. Nothing without `Assets<Image>`, which
/// is the headless tests: a plate there is its label alone.
#[allow(clippy::too_many_arguments)]
fn board_pictures(
    mut commands: Commands,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    model: Option<Res<Model>>,
    skin: Res<crate::skin::Skin>,
    art: Option<Res<BoardArt>>,
    images: Option<ResMut<Assets<Image>>>,
    mut dirty: ResMut<Dirty>,
) {
    let Some(mut images) = images else { return };
    let faction = model.as_ref().and_then(|model| model.0.view.as_ref()).and_then(|view| view.corp.identity.as_ref()).and_then(|id| core.registry.get(id)).and_then(|card| card.faction);
    let style = board_art::Style { skin: skin.folder.clone(), faction, basic: core.settings.desktop.basic_graphics };
    if art.is_some_and(|art| art.loaded_for == style) {
        return;
    }
    commands.insert_resource(BoardArt::load(style, &theme, &mut images));
    dirty.board = true;
}

/// Recomputes the face width from the window and the view, and marks the
/// board for a redraw when it moved. Runs every frame and is cheap: a
/// handful of comparisons.
pub(crate) fn fit(windows: Query<&Window, With<PrimaryWindow>>, model: Option<Res<Model>>, fit: Option<ResMut<BoardFit>>, mut dirty: ResMut<Dirty>) {
    let (Some(model), Some(mut fit)) = (model, fit) else { return };
    let window = windows.single().map_or(fit.window, |w| Vec2::new(w.width(), w.height()));
    let counts = counts(&model.0);
    let face = layout::face_width((window.x, window.y), counts);
    if (face - fit.face).abs() > 0.5 || window != fit.window || model.0.side != fit.chair {
        fit.face = face;
        fit.window = window;
        fit.chair = model.0.side;
        fit.field = layout::field_height(window.y, face, counts);
        dirty.board = true;
        // The pop-up is sized from the window too — capped at it, with
        // the cards drawn in what its words leave — so a resize has to
        // rebuild the rail's floating panels and not only the board.
        // (`place_menu` still earns its place: it re-anchors the menu
        // every frame the card's own box moves under it, which no resize
        // is involved in.)
        dirty.rail = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    mut commands: Commands,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    active: Option<Res<ActiveMatch>>,
    replay: Option<Res<ActiveReplay>>,
    mut images: Option<ResMut<Assets<Image>>>,
    dev: Option<Res<crate::dev::Dev>>,
    mut last_table: ResMut<table::LastTable>,
) {
    commands.init_resource::<Dirty>();
    // A match being played, or a recorded one being stepped through
    // (`screens::replay`): the same board either way.
    let source = match (&active, &replay) {
        (Some(active), _) => Some((Game::new(core.registry.clone(), active.handle.side()), active.handle.side())),
        (None, Some(replay)) => Some((crate::screens::replay::board_for(&core, &replay.0), replay.0.side())),
        (None, None) => None,
    };
    let Some((game, side)) = source else {
        commands.spawn((screen_root(AppScreen::Game, theme.background), children![
            widgets::heading(&theme, AppScreen::Game.title()),
            widgets::dim(&theme, "No game in progress. Start one from Play vs Computer."),
            widgets::button(&theme, "Back", Val::Auto, Click::Back),
        ]));
        return;
    };
    let replaying = game.replay.is_some();
    let mut pacer = Pacer::new(side, core.settings.desktop.animation_speed);
    pacer.hold_at_encounter = dev.is_some_and(|dev| dev.hold_run);
    commands.insert_resource(Pace(pacer));

    // No scroll area, ever: the board's rows are sized by `BoardFit` to
    // fit, and `Overflow::clip` is the backstop, not the design.
    let board = commands
        .spawn((
            Board,
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                min_height: px(0),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(layout::ROW_GAP),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    let rail = commands
        .spawn((Rail, Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }))
        .id();
    let log_scroll = commands
        .spawn((
            LogScroll,
            bevy::ui_widgets::ScrollArea,
            BackgroundColor(theme.panel),
            Node { width: percent(100), height: px(200), flex_shrink: 0.0, flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), padding: UiRect::all(px(6)), ..default() },
        ))
        .with_children(|parent| {
            parent.spawn((LogList, Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }));
        })
        .id();
    let log_row = commands
        .spawn((LogRow, Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Row, column_gap: px(4), height: px(200), ..default() }))
        .add_child(log_scroll)
        .with_children(|parent| {
            parent.spawn(widgets::scrollbar(&theme, log_scroll));
        })
        .id();
    // The right column's head: where the match is, Quit and the gear.
    // It was a bar across the top of the window, which cost every card
    // on the board its height to say what the column beside it says.
    let header = commands
        .spawn((Node { width: percent(100), min_height: px(layout::TOP_BAR), flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8), ..default() },))
        .with_children(|parent| {
            parent.spawn((StatusLine, widgets::dim(&theme, "Setting up…"), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
            // A replay has nothing to lose, so its way out says where it
            // goes rather than asking.
            parent.spawn(widgets::button(&theme, if replaying { "Close" } else { "Quit" }, Val::Auto, Click::Quit));
            // The gear in the corner, where a person looks for options.
            parent.spawn(widgets::gear_button(&theme, images.as_deref_mut(), Click::Options));
        })
        .id();
    let phase_panel = commands
        .spawn((PhaseBarRow, Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, ..default() }))
        .id();
    let run_panel = commands
        .spawn((RunIdentity, Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, ..default() }))
        .id();
    // The one column on the right: the header, the phase, the Runner on a
    // run, the prompt and the log. It carries the vertical padding the
    // root no longer does, so its buttons keep off the window's edges.
    let rail_column = commands
        .spawn((Node {
            width: px(layout::RAIL_WIDTH),
            height: percent(100),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            padding: UiRect::vertical(px(layout::PADDING)),
            ..default()
        },))
        .add_child(header)
        .add_child(phase_panel)
        .add_child(run_panel)
        .add_child(rail)
        .add_child(log_row)
        .id();
    let body = commands
        .spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(layout::BODY_GAP), ..default() },))
        .add_child(board)
        .add_child(rail_column)
        .id();
    // The field, chosen once for the match rather than per redraw, and
    // spawned before everything else so every later sibling paints over
    // it. Only where `Assets<Image>` exists: the headless tests build the
    // client without an asset plugin, and the board there has no field,
    // which costs them nothing since nothing they assert is a picture.
    let backdrops: Vec<Entity> = match images.as_deref_mut() {
        Some(images) => {
            let installed = table::available();
            let last = last_table.0.take();
            let folder = table::resolve(&core.settings.desktop.table, &installed, table_nonce(), last.as_deref(), core.settings.desktop.basic_graphics);
            last_table.0 = folder.clone().or(last);
            // A named table whose files have gone falls back to the
            // painted ground, which is the tier that always works.
            let field = folder.as_deref().and_then(table::base).unwrap_or_else(|| table::paint(&theme));
            let mut spawned = vec![commands.spawn(table::backdrop(images.add(field))).id()];
            if let Some(overlay) = folder.as_deref().and_then(table::overlay) {
                spawned.push(commands.spawn(table::backdrop(images.add(overlay))).id());
            }
            spawned
        }
        None => Vec::new(),
    };
    let mut root = commands.spawn(screen_root(AppScreen::Game, theme.background));
    root.entry::<Node>().and_modify(|mut node| {
        node.align_items = AlignItems::Stretch;
        // Sides only: the hands sit on the window's top and bottom edges.
        node.padding = UiRect::horizontal(px(layout::PADDING));
        node.row_gap = px(0);
        node.overflow = Overflow::clip();
    });
    for backdrop in backdrops {
        root.add_child(backdrop);
    }
    root.add_child(body);
    commands.insert_resource(BoardFit::default());
    commands.insert_resource(Model(game));
    let mut dirty = Dirty::default();
    dirty.all();
    commands.insert_resource(dirty);
}

/// Marks the bands the table guide draws, so the next frame's are not
/// drawn on top of this frame's.
#[derive(Component)]
struct TableGuide;

/// `NETRUNNER_TABLE_GUIDE=1`: a band over each row of the board, with a
/// centre line, so a field can be painted to where the cards actually
/// fall.
///
/// The person authoring a table has to put its horizon and its vanishing
/// point somewhere, and "somewhere" is only useful if it agrees with the
/// layout. The bands are read from the **laid-out boxes** rather than
/// recomputed from `models::layout`, so what they show is where the
/// cards are, including every overlap and clamp the fit applied.
///
/// Rebuilt every frame while it is on, because `redraw` despawns the
/// board's children wholesale and a guide that survived that would be
/// describing the previous layout. It is a dev hook; the cost is a dev's.
fn table_guide(
    mut commands: Commands,
    theme: Res<Theme>,
    dev: Option<Res<crate::dev::Dev>>,
    board: Query<(Entity, &Children, &ComputedNode, &UiGlobalTransform), With<Board>>,
    boxes: Query<(&ComputedNode, &UiGlobalTransform)>,
    drawn: Query<Entity, With<TableGuide>>,
) {
    if !dev.is_some_and(|dev| dev.table_guide) {
        return;
    }
    // The bands hang off one container rather than off the board
    // directly, and the loop below skips it. Both halves matter: the
    // first cut parented each band to the board, so the next frame read
    // the board's children — bands included — and drew a band for every
    // band. It grew by a row a frame (6, 12, 18 …) and looked like a
    // despawn that was not working.
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    let Ok((board_entity, children, board_node, board_at)) = board.single() else { return };
    let board_box = anchor_of(board_node, board_at);
    let container = commands
        .spawn((
            TableGuide,
            GlobalZIndex(5),
            Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() },
        ))
        .id();
    commands.entity(board_entity).add_child(container);
    for (index, child) in children.iter().filter(|child| !drawn.contains(*child)).enumerate() {
        let Ok((node, at)) = boxes.get(child) else { continue };
        let row = anchor_of(node, at);
        // The child's box in the board's own coordinates, since the band
        // sits in a container that fills the board.
        let left = row.x - row.width / 2.0 - (board_box.x - board_box.width / 2.0);
        let top = row.y - row.height / 2.0 - (board_box.y - board_box.height / 2.0);
        let tint = if index % 2 == 0 { theme.accent } else { theme.danger };
        // Outlined, never filled: the whole point is to see the field
        // under the rows, and a wash over it would hide what is being
        // painted to.
        commands.entity(container).with_children(|guide| {
            guide.spawn((
                BackgroundColor(Color::NONE),
                BorderColor::all(tint),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(left),
                    top: px(top),
                    width: px(row.width),
                    height: px(row.height),
                    border: UiRect::all(px(1)),
                    padding: UiRect::all(px(2)),
                    ..default()
                },
                // `widgets::dim` already carries a `TextColor`; a second
                // one in the same bundle is the duplicate-component panic.
                children![(
                    Text::new(format!("row {index} · y {:.0}…{:.0} · h {:.0}", top, top + row.height, row.height)),
                    theme.font(size::SMALL),
                    TextColor(tint),
                )],
            ));
        });
    }
}

/// What decides a random table. The wall clock, because the choice is
/// cosmetic and per match: seeding it from the game's own seed would tie
/// the field to the deal, so replaying a seed to look at a bug would
/// change the picture with it.
fn table_nonce() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|since| since.as_nanos() as u64).unwrap_or(0)
}

/// Saves the match so far where the client keeps its reports, and says
/// where in words a person can act on: the file, and the command that
/// opens it at the moment it was saved. A failure is a sentence too,
/// never a panic — a report that cannot be written must not end the game.
fn save_report(core: &ClientCore, handle: &netrunner_client::play::MatchHandle) -> (String, Option<std::path::PathBuf>) {
    let Some(dir) = &core.reports_dir else {
        return (format!("No bug report saved: there is no data directory; set {}", netrunner_client::bug_report::REPORTS_DIR_ENV), None);
    };
    let (header, history) = handle.record();
    match netrunner_client::bug_report::save(dir, &header, &history) {
        Ok(path) => (format!("Bug report saved to {} — it is under Replays, or open it with `netrunner_cli replay {}`", path.display(), path.display()), Some(path)),
        Err(error) => (format!("No bug report saved: {error}"), None),
    }
}

/// Remembers a report just saved on the board, for its "Watch it".
fn saved(model: &mut Game, notices: &mut Notices, (line, path): (String, Option<std::path::PathBuf>)) {
    notices.push(line.clone());
    model.saved_report = Some(line);
    model.saved_report_path = path;
}

/// Leaving the screen ends the match: dropping the handle quits it, and
/// the form gets the choice to reopen on.
fn leave(world: &mut World) {
    world.remove_resource::<Model>();
    world.remove_resource::<Pace>();
    world.remove_resource::<ActiveReplay>();
    if let Some(active) = world.remove_resource::<ActiveMatch>()
        && let Some(choice) = active.choice
    {
        world.insert_resource(LastGame(choice));
    }
}

/// Drains the match's messages into the pacer, and the beats due now
/// into the model: a run's events move the trail and the lane, a
/// message moves the board.
///
/// **A stall saves a bug report by itself**, the moment it arrives: it is
/// the report that matters most, the person may not think to press
/// anything, and the match thread has already stopped, so the record is
/// the whole game.
#[allow(clippy::too_many_arguments)]
fn poll(
    active: Option<ResMut<ActiveMatch>>,
    model: Option<ResMut<Model>>,
    pace: Option<ResMut<Pace>>,
    time: Res<Time>,
    mut dirty: ResMut<Dirty>,
    dev: Option<ResMut<crate::dev::Dev>>,
    core: Res<ClientCore>,
    mut notices: ResMut<Notices>,
) {
    // A replay has no match: its steps are pushed into the pacer by
    // `screens::replay`, and the beats are released here the same way.
    let (Some(mut model), Some(mut pace)) = (model, pace) else { return };
    let mut active = active;
    while let Some(message) = active.as_mut().and_then(|active| active.handle.poll()) {
        if let Some(active) = &active
            && matches!(message, netrunner_client::play::MatchMessage::Stalled { .. })
        {
            saved(&mut model.0, &mut notices, save_report(&core, &active.handle));
        }
        pace.0.push(message);
    }
    for beat in pace.0.tick(time.elapsed()) {
        match beat {
            Beat::Steps(events) => {
                if model.0.apply(Intent::RunStep(events)) == Outcome::Redraw {
                    dirty.lane = true;
                }
            }
            Beat::Apply(message) => {
                // The one message outcome that acts: a lone pass taken
                // for the person. `submit` fails only once the match has
                // ended, and its `Ended` is already on the way.
                if let Outcome::Submit(action) = model.0.apply(Intent::Message(MatchMessageRef(message)))
                    && let Some(active) = &active
                {
                    let _ = active.handle.submit(action);
                }
                dirty.all();
            }
        }
    }
    // Held at an encounter for a screenshot: the autoplay is done, so
    // the shot is taken with the run in flight.
    if pace.0.held && let Some(mut dev) = dev {
        dev.autoplayed = dev.autoplayed.max(dev.autoplay);
    }
}

/// The dev hook's hand: while `NETRUNNER_AUTOPLAY` has decisions left,
/// takes one from the panel each time the board is awaiting — the
/// `applied`th entry, so the choice wanders through the list and the
/// game develops rather than clicking for credits forever. Off, and
/// ignored, for a person.
fn autoplay(
    dev: Option<ResMut<crate::dev::Dev>>,
    model: Option<Res<Model>>,
    mut pending: ResMut<Pending>,
    nodes: Query<(&Click, &ComputedNode, &UiGlobalTransform)>,
    client: Res<ClientCore>,
) {
    let (Some(mut dev), Some(model)) = (dev, model) else { return };
    // A match that has stopped has no decisions left to take, so the
    // autoplay is done: the screenshot is taken of how it stopped and the
    // client exits. It used to wait for a count a finished game could
    // never reach, and a stall left the window open and hung for good —
    // what the person watching a Mutual Favor livelock saw (§4ag).
    if dev.autoplayed < dev.autoplay && (model.0.stalled.is_some() || model.0.over.is_some()) {
        match &model.0.stalled {
            Some(reason) => warn!("dev: the match stopped after {} of {} autoplayed decisions: {reason}", dev.autoplayed, dev.autoplay),
            None => info!("dev: the match ended after {} of {} autoplayed decisions", dev.autoplayed, dev.autoplay),
        }
        dev.autoplayed = dev.autoplay;
        return;
    }
    if dev.options && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        // The gear, pressed once the board has settled.
        dev.options = false;
        pending.0.push(Intent::ToggleOptions);
        return;
    }
    // The first decision at which a card in hand has somewhere to go: a
    // card held anywhere else lights nothing, and waiting for the
    // person's own action phase can wait for ever — the autoplay may have
    // stopped at a prompt inside the opponent's turn.
    if dev.drag && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        let held = model.0.hand.cards().iter().position(|card| !model.0.actions.destinations_for_hand_card(card).is_empty());
        match held {
            Some(slot) => {
                dev.drag = false;
                pending.0.push(Intent::DragPress { slot, at: (0.0, 0.0) });
                pending.0.push(Intent::DragMove { at: (400.0, 400.0) });
            }
            // Nothing in hand has anywhere to go at this decision (a hand
            // of operations, or a prompt mid-turn): take one more action
            // and look again, rather than holding the shot for ever.
            None => dev.autoplay += 1,
        }
    }
    if dev.drag && model.0.finished() {
        dev.drag = false;
    }
    if dev.keys && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        dev.keys = false;
        pending.0.push(Intent::Shortcut(Shortcut::Help));
    }
    if let Some(pick) = dev.menu.filter(|_| model.0.awaiting && dev.autoplayed >= dev.autoplay) {
        dev.menu = None;
        // Every box the board laid out, with what a click on it would
        // mean and how many entries that is — which is all three picks
        // need, so the hook learns nothing new about the view.
        let boxes = || {
            nodes.iter().filter_map(|(click, node, transform)| match click {
                Click::Target(target) => Some((target.clone(), model.0.entries_for(target).len(), anchor_of(node, transform))),
                _ => None,
            })
        };
        let picked = match pick {
            crate::dev::MenuPick::Hand => {
                let hand = model.0.view.as_ref().and_then(|view| match model.0.side {
                    Side::Corp => view.corp.hq_cards.clone(),
                    Side::Runner => view.runner.grip_cards.clone(),
                });
                // The first card with an action, else the first card: a
                // menu with nothing in it is a look worth taking too.
                let hand = hand.unwrap_or_default();
                hand.iter().find(|card| !model.0.actions.for_hand_card(card).is_empty()).or(hand.first()).map(|card| {
                    let target = Target::HandCard(card.clone());
                    // The card's own box, as the click would have read it.
                    let over = boxes().find(|(candidate, _, _)| *candidate == target).map_or_else(Anchor::default, |(_, _, over)| over);
                    (target, over)
                })
            }
            crate::dev::MenuPick::Most => boxes().max_by_key(|(_, entries, _)| *entries).filter(|(_, entries, _)| *entries > 0).map(|(target, _, over)| (target, over)),
            crate::dev::MenuPick::Top => boxes()
                .filter(|(_, entries, _)| *entries > 0)
                .min_by(|(_, _, a), (_, _, b)| a.y.total_cmp(&b.y))
                .map(|(target, _, over)| (target, over)),
        };
        if let Some((target, over)) = picked {
            pending.0.push(Intent::Click { target, over });
        }
    }
    if dev.sheet && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        dev.sheet = false;
        // The first Corp install on the board, ice before root, in the
        // engine's order: the tile's own sheet.
        let first = model.0.view.as_ref().and_then(|view| view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).map(|c| c.install_id).next());
        if let Some(id) = first {
            pending.0.push(Intent::Inspect(Target::Install(id)));
        }
    }
    if model.0.awaiting
        && dev.autoplayed >= dev.autoplay
        && let Some(side) = dev.agendas.take()
    {
        pending.0.push(Intent::Inspect(Target::Pile(Pile::Agendas(side))));
        pending.0.push(Intent::Expand(0));
    }
    if dev.autoplayed >= dev.autoplay || !model.0.awaiting || model.0.actions.is_empty() {
        return;
    }
    // Held at a card-selection prompt, a card's install, an access, or
    // an encounter with a way through, for a screenshot: the autoplay is done, and the pop-up is what is
    // shot. An access is not a `PendingDecision`, so it is asked of
    // `access::Access` rather than matched here.
    let held = model.0.view.as_ref().is_some_and(|view| match view.pending_decision {
        Some(PendingDecision::ChooseCards { .. }) => dev.hold_selection,
        Some(PendingDecision::ChooseServer { install: Some(_), .. }) => dev.hold_install,
        _ => dev.hold_access && Access::of(view, &client.registry).is_some(),
    }) || (dev.hold_break && !model.0.breaks.is_empty())
        || (dev.hold_ice && model.0.encounter().is_some());
    if held {
        dev.autoplayed = dev.autoplay;
        return;
    }
    dev.autoplayed += 1;
    let entries = &model.0.actions.entries;
    // At a card selection the wandering index toggled one card on and off
    // until the session's stall guard ended the game (Phase 5 §19 found
    // it, on Mutual Favor): a toggle leaves the same list behind, so
    // `applied` walked the same two entries forever. So a selection is
    // finished — confirmed once it can be, otherwise a card not yet picked
    // is added — and is still one of the listed entries.
    let selecting = model.0.view.as_ref().and_then(|view| match &view.pending_decision {
        Some(PendingDecision::ChooseCards { selected, .. }) => Some(selected.clone()),
        _ => None,
    });
    let index = selecting
        .and_then(|selected| {
            let confirm = entries.iter().position(|entry| entry.action == PlayerAction::ConfirmCardSelection);
            confirm.or_else(|| {
                entries.iter().position(|entry| matches!(&entry.action, PlayerAction::ToggleCardSelection { position } if !selected.contains(position)))
            })
        })
        .unwrap_or(model.0.applied % entries.len());
    pending.0.push(Intent::Choose(index));
}

/// Escape is the board's: it closes what is open, and otherwise asks to
/// quit, so the navigation rule never leaves the game without asking.
fn escape(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<InputCaptured>, mut pending: ResMut<Pending>, model: Option<Res<Model>>) {
    if model.is_none() || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    captured.0 = true;
    pending.0.push(Intent::Back);
}

/// The box a node was laid out in, in logical window pixels: the
/// global transform's translation is its centre and the computed size
/// its extent, both physical until scaled back.
fn anchor_of(node: &ComputedNode, transform: &UiGlobalTransform) -> Anchor {
    let scale = node.inverse_scale_factor();
    let centre = transform.translation * scale;
    let size = node.size() * scale;
    Anchor { x: centre.x, y: centre.y, width: size.x, height: size.y }
}

/// Whether the key that turns the primary button into the secondary is
/// held: Ctrl, or Cmd — a Mac with one button reaches for either, and
/// neither means anything else on a board click.
fn secondary_modifier(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight])
}

/// The secondary click — the right button, or the primary with Ctrl or
/// Cmd held — on a hovered card or zone opens its sheet, to read. The
/// focus system sets `Interaction` for the primary button only and
/// never for the right, so the button is read from the input resource
/// and the target is whichever `Click::Target` node it left hovered (or
/// pressed, with the modifier). The plain primary press is `controls`'s,
/// which opens the menu. With a menu open, a primary click that presses
/// no part of it and no other card closes it; a press on a card is that
/// card's menu instead (or, on the same card, the menu closing), and
/// the menu's own button reaches `controls` as a `Pressed` and submits.
fn board_click(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    targets: Query<(&Interaction, &Click)>,
    choice_cards: Query<(&Interaction, &ChoiceCard)>,
    menu_parts: Query<&Interaction, With<MenuPart>>,
    model: Option<Res<Model>>,
    mut pending: ResMut<Pending>,
) {
    let Some(model) = model else { return };
    let modifier = secondary_modifier(&keys);
    let secondary = mouse.just_pressed(MouseButton::Right) || (modifier && mouse.just_pressed(MouseButton::Left));
    // A card in the decision pop-up is read like one on the board: in the
    // card sheet, over the pop-up, which a click away closes again.
    if secondary && let Some((_, card)) = choice_cards.iter().find(|(interaction, _)| matches!(interaction, Interaction::Hovered | Interaction::Pressed)) {
        pending.0.push(Intent::InspectCard(Some(card.0.clone())));
        return;
    }
    let hovered = targets.iter().find_map(|(interaction, click)| match (interaction, click) {
        (Interaction::Hovered | Interaction::Pressed, Click::Target(target)) => Some(target.clone()),
        _ => None,
    });
    let menu_open = model.0.menu.is_some();
    if secondary {
        match hovered {
            Some(target) => pending.0.push(Intent::Inspect(target)),
            None if menu_open => pending.0.push(Intent::CloseMenu),
            None => {}
        }
        return;
    }
    if menu_open && mouse.just_pressed(MouseButton::Left) && hovered.is_none() && !menu_parts.iter().any(|i| *i == Interaction::Pressed) {
        pending.0.push(Intent::CloseMenu);
    }
}

/// A hand card's press, travel and release (`models::drag`). The pointer
/// comes from `CursorMoved` rather than the window, so a test with no
/// window can drive a drag; the press itself is `Interaction::Pressed` on
/// a face carrying a `HandSlot`, as the click is. A release with no travel
/// is handed back as the card's click, so the menu still opens from the
/// same press — `controls` never sees a hand card at all.
/// Whether a point is inside a laid-out box.
fn within(anchor: Anchor, (x, y): (f32, f32)) -> bool {
    (anchor.x - x).abs() <= anchor.width / 2.0 && (anchor.y - y).abs() <= anchor.height / 2.0
}

#[allow(clippy::too_many_arguments)]
fn drag_hand(
    mut moved: MessageReader<CursorMoved>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    slots: Query<(&HandSlot, &Interaction, &ComputedNode, &UiGlobalTransform)>,
    places: Query<(&DropPlace, &ComputedNode, &UiGlobalTransform)>,
    model: Option<Res<Model>>,
    mut pointer: ResMut<Pointer>,
    mut pending: ResMut<Pending>,
) {
    let Some(model) = model else {
        moved.clear();
        return;
    };
    for message in moved.read() {
        pointer.0 = (message.position.x, message.position.y);
    }
    let held = model.0.dragging.is_some();
    // Ctrl or Cmd with the primary button is the secondary click, which
    // reads the card; it never picks one up.
    if !held && mouse.just_pressed(MouseButton::Left) && !secondary_modifier(&keys) && !model.0.covered() {
        if let Some((slot, _, _, _)) = slots.iter().find(|(_, interaction, _, _)| matches!(interaction, Interaction::Pressed | Interaction::Hovered)) {
            pending.0.push(Intent::DragPress { slot: slot.0, at: pointer.0 });
        }
        return;
    }
    if held && mouse.pressed(MouseButton::Left) {
        pending.0.push(Intent::DragMove { at: pointer.0 });
    }
    if held && mouse.just_released(MouseButton::Left) {
        // The row as it is laid out: each face's centre, in the person's
        // own order, which is the order the slots were drawn in.
        let mut row: Vec<(usize, f32, Anchor)> = slots
            .iter()
            .map(|(slot, _, node, transform)| {
                let anchor = anchor_of(node, transform);
                (slot.0, anchor.x, anchor)
            })
            .collect();
        row.sort_by_key(|(slot, _, _)| *slot);
        let dragged = model.0.dragged_slot().or_else(|| model.0.dragging.as_ref().map(|drag| drag.what));
        let over = dragged.and_then(|slot| row.iter().find(|(at, _, _)| *at == slot)).map(|(_, _, anchor)| *anchor).unwrap_or_default();
        // A place the board lit for this card, under the pointer: the
        // innermost wins, so an ice tile takes the drop rather than the
        // column it sits in. Hit-tested against the laid-out boxes, since
        // the focus system does not follow a pointer with a button down.
        let lit = model.0.drop_places();
        let dropped_on = places
            .iter()
            .filter(|(place, _, _)| lit.contains(&place.0))
            .filter(|(_, node, transform)| within(anchor_of(node, transform), pointer.0))
            .min_by(|a, b| {
                let area = |node: &ComputedNode, transform: &UiGlobalTransform| {
                    let anchor = anchor_of(node, transform);
                    anchor.width * anchor.height
                };
                area(a.1, a.2).total_cmp(&area(b.1, b.2))
            })
            .map(|(place, node, transform)| (place.0.clone(), anchor_of(node, transform)));
        match dropped_on {
            Some((target, anchor)) => pending.0.push(Intent::DragDrop { target, over: anchor }),
            None => pending.0.push(Intent::DragRelease { over, slots: row.iter().map(|(_, x, _)| *x).collect() }),
        }
    }
}

/// The board's keys (`models::shortcuts`), read off the keyboard messages by
/// what they type, so a letter is found on any layout; a key held with
/// Ctrl, Cmd or Alt is left to the system. The model answers most of them.
/// Two need the screen: the pointer's keys, which act on whichever card or
/// zone is hovered as its click would, and the play helper, which is a
/// setting saved to the file as the options menu saves it.
#[allow(clippy::too_many_arguments)]
fn shortcuts(
    mut keyboard: MessageReader<KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    targets: Query<(Entity, &Interaction, &Click)>,
    boxes: Query<(&ComputedNode, &UiGlobalTransform)>,
    model: Option<Res<Model>>,
    mut pending: ResMut<Pending>,
    mut core: ResMut<ClientCore>,
    mut notices: ResMut<Notices>,
    mut dirty: ResMut<Dirty>,
) {
    let Some(model) = model else {
        keyboard.clear();
        return;
    };
    if held.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight, KeyCode::AltLeft, KeyCode::AltRight]) {
        keyboard.clear();
        return;
    }
    let shift = held.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for input in keyboard.read() {
        if input.state != ButtonState::Pressed || input.repeat {
            continue;
        }
        let key = match &input.logical_key {
            BevyKey::Space => shortcuts::Key::Space,
            BevyKey::Enter => shortcuts::Key::Enter,
            BevyKey::Tab => shortcuts::Key::Tab,
            BevyKey::F1 => shortcuts::Key::F1,
            BevyKey::Character(text) => match text.chars().next() {
                Some(c) => shortcuts::Key::Char(c.to_ascii_lowercase()),
                None => continue,
            },
            _ => continue,
        };
        let Some(shortcut) = shortcuts::shortcut(key, shift, model.0.side) else { continue };
        match shortcut {
            Shortcut::ReadHovered | Shortcut::MenuHovered => {
                let hovered = targets.iter().find_map(|(entity, interaction, click)| match (interaction, click) {
                    (Interaction::Hovered | Interaction::Pressed, Click::Target(target)) => Some((entity, target.clone())),
                    _ => None,
                });
                let Some((entity, target)) = hovered else { continue };
                if shortcut == Shortcut::ReadHovered {
                    pending.0.push(Intent::Inspect(target));
                } else {
                    let over = boxes.get(entity).map_or_else(|_| Anchor::default(), |(node, transform)| anchor_of(node, transform));
                    pending.0.push(Intent::Click { target, over });
                }
            }
            Shortcut::PlayHelper | Shortcut::PhaseBar => {
                if model.0.covered() {
                    continue;
                }
                let row = if shortcut == Shortcut::PhaseBar { Row::PhaseBar } else { Row::PlayHelper };
                settings_model::apply(&mut core.settings, settings_model::Intent::Toggle(row), &table::available(), &skin::available());
                if let Err(error) = core.save_settings() {
                    notices.push(format!("Settings not saved: {error}"));
                }
                // The phase panel is the right column's and costs the
                // cards nothing; the helper is the rail's.
                dirty.rail = true;
                dirty.side = true;
            }
            _ => pending.0.push(Intent::Shortcut(shortcut)),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn controls(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    keys: Res<ButtonInput<KeyCode>>,
    faces: Query<(Entity, &Interaction, &Click), (Changed<Interaction>, Without<widgets::Themed>)>,
    boxes: Query<(&ComputedNode, &UiGlobalTransform)>,
    slots: Query<&HandSlot>,
    mut pending: ResMut<Pending>,
    marks: Query<&Click>,
    settings_marks: Query<&SettingsControl>,
    mut core: ResMut<ClientCore>,
    mut model: Option<ResMut<Model>>,
    active: Option<Res<ActiveMatch>>,
    mut dirty: ResMut<Dirty>,
    mut notices: ResMut<Notices>,
    mut navigate: MessageWriter<Navigate>,
    mut pace: Option<ResMut<Pace>>,
) {
    let mut intents: Vec<Intent> = std::mem::take(&mut pending.0);
    let mut leave_to: Option<AppScreen> = None;
    // With Ctrl or Cmd held the primary button is the secondary click
    // (`board_click` opened the sheet), so the press it also registers
    // on the card opens no menu.
    let modifier = secondary_modifier(&keys);
    // A press on a card or a zone is its menu, over the node's own box.
    let click_on = |entity: Entity, target: &Target| {
        let over = boxes.get(entity).map_or_else(|_| Anchor::default(), |(node, transform)| anchor_of(node, transform));
        Intent::Click { target: target.clone(), over }
    };
    // A face is a `Button` without the theme's recolouring, so the
    // shared feedback system never reports it; its press is read here.
    // The first hand-driven game found every card click doing nothing.
    for (entity, interaction, click) in &faces {
        if *interaction != Interaction::Pressed || modifier {
            continue;
        }
        match click {
            // A card of the person's own hand is `drag_hand`'s: its press
            // is armed there and comes back as this same click if the
            // pointer never moved.
            Click::Target(_) if slots.contains(entity) => {}
            Click::Target(target) => intents.push(click_on(entity, target)),
            // A card in the decision pop-up is its own button.
            Click::Entry(index) => intents.push(Intent::Choose(*index)),
            _ => {}
        }
    }
    for Pressed(entity) in pressed.read() {
        // A row of the options menu: the settings model applies it and
        // the file is saved, as on the settings screen; the rail and the
        // log follow the new value on the next redraw.
        if let Ok(SettingsControl::Intent(intent)) = settings_marks.get(*entity) {
            if settings_model::apply(&mut core.settings, intent.clone(), &table::available(), &skin::available()) {
                if let Err(error) = core.save_settings() {
                    notices.push(format!("Settings not saved: {error}"));
                }
                if let Some(pace) = pace.as_mut() {
                    pace.0.set_speed(core.settings.desktop.animation_speed);
                }
                dirty.rail = true;
                dirty.log = true;
                dirty.overlay = true;
                dirty.side = true;
            }
            continue;
        }
        match marks.get(*entity) {
            Ok(Click::Target(_)) if modifier => {}
            Ok(Click::Target(target)) => intents.push(click_on(*entity, target)),
            Ok(Click::Entry(index)) => intents.push(Intent::Choose(*index)),
            Ok(Click::Break(index)) => intents.push(Intent::Break(*index)),
            Ok(Click::TakeBack) => intents.push(Intent::TakeBack),
            Ok(Click::Control(control)) => intents.push(Intent::Control(*control)),
            Ok(Click::Inspect(card)) => intents.push(Intent::InspectCard(Some(card.clone()))),
            Ok(Click::Expand(row)) => intents.push(Intent::Expand(*row)),
            Ok(Click::Options) => intents.push(Intent::ToggleOptions),
            Ok(Click::SaveReport) => {
                if let (Some(active), Some(model)) = (active.as_ref(), model.as_mut()) {
                    saved(&mut model.0, &mut notices, save_report(&core, &active.handle));
                    dirty.overlay = true;
                }
            }
            Ok(Click::WatchReport) => {
                if let Some(path) = model.as_ref().and_then(|model| model.0.saved_report_path.clone()) {
                    commands.insert_resource(OpenReplay(path, None));
                    leave_to = Some(AppScreen::Replay);
                }
            }
            Ok(Click::CloseOverlay) => intents.push(Intent::Back),
            Ok(Click::ConfirmQuit) => intents.push(Intent::ConfirmQuit),
            Ok(Click::CancelQuit) => intents.push(Intent::CancelQuit),
            Ok(Click::Quit) => intents.push(Intent::RequestQuit),
            Ok(Click::PlayAgain) => leave_to = Some(AppScreen::NewGame),
            Ok(Click::Menu | Click::Back) => leave_to = Some(AppScreen::MainMenu),
            Err(_) => {}
        }
    }
    if let Some(screen) = leave_to {
        navigate.write(Navigate(screen));
        return;
    }
    let Some(mut model) = model else { return };
    for intent in intents {
        match model.0.apply(intent) {
            Outcome::Nothing => {}
            Outcome::Redraw => {
                dirty.rail = true;
                dirty.overlay = true;
            }
            // Never from a replay, which awaits nothing; and a match that
            // has gone has nobody to hand it to.
            Outcome::Submit(action) => {
                if let Some(Err(error)) = active.as_ref().map(|active| active.handle.submit(action)) {
                    notices.push(error);
                }
                dirty.rail = true;
                dirty.overlay = true;
            }
            Outcome::Rewind => {
                if let Some(Err(error)) = active.as_ref().map(|active| active.handle.rewind()) {
                    notices.push(error);
                }
                dirty.rail = true;
                dirty.overlay = true;
            }
            // A replay goes back to the list it was picked from.
            Outcome::Quit => {
                navigate.write(Navigate(if model.0.replay.is_some() { AppScreen::Replay } else { AppScreen::MainMenu }));
                return;
            }
        }
    }
}

fn redraw(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    model: Option<ResMut<Model>>,
    board: Query<Entity, With<Board>>,
    rail: Query<Entity, With<Rail>>,
    bar: Query<Entity, With<ControlBar>>,
    log: Query<Entity, With<LogList>>,
    mut log_row: Query<&mut Node, With<LogRow>>,
    mut log_scroll: Query<&mut ScrollPosition, With<LogScroll>>,
    // One query for the three floating layers: a system takes sixteen
    // parameters at most, and this one is at the limit.
    floating: Query<(Entity, Has<Overlay>, Has<DecisionPopup>, Has<ActionsMenu>), Or<(With<Overlay>, With<DecisionPopup>, With<ActionsMenu>)>>,
    roots: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    // The card pictures and the board's own, as one parameter: the system
    // is at Bevy's sixteen.
    (images, art): (Res<CardImages>, Option<Res<BoardArt>>),
    fit: Option<Res<BoardFit>>,
) {
    let (Some(mut model), Some(fit)) = (model, fit) else { return };
    if !(dirty.board || dirty.rail || dirty.log || dirty.overlay) {
        return;
    }
    let Dirty { board: reboard, rail: rerail, log: relog, overlay: reoverlay, lane: _, side: _ } = std::mem::take(&mut *dirty);
    let game = &mut model.0;
    if reboard {
        let transitions = game.take_transitions();
        if let Ok(board) = board.single() {
            let art = art.as_deref();
            commands.entity(board).despawn_children().with_children(|parent| spawn_board(parent, &theme, &core, &images, art, game, &transitions, &fit));
        }
    }
    let prefs = &core.settings.desktop;
    if rerail {
        if let Ok(rail) = rail.single() {
            commands.entity(rail).despawn_children().with_children(|parent| spawn_rail(parent, &theme, game, prefs.play_helper));
        }
        // The bar is a row of the board, so a redrawn board brought a
        // fresh one with it; only a rail-only redraw refills the old one.
        if !reboard && let Ok(bar) = bar.single() {
            commands.entity(bar).despawn_children().with_children(|parent| spawn_control_bar(parent, &theme, game));
        }
        for (entity, _, _, _) in floating.iter().filter(|(_, _, popup, menu)| *popup || *menu) {
            commands.entity(entity).despawn();
        }
        let decisions = if game.awaiting && !game.finished() { game.actions.decisions() } else { Vec::new() };
        if let Some(root) = roots.iter().next() {
            if !decisions.is_empty() {
                commands.entity(root).with_children(|parent| spawn_decision_popup(parent, &theme, &core, &images, game, &decisions, fit.window));
            }
            if let Some(menu) = &game.menu {
                commands.entity(root).with_children(|parent| spawn_actions_menu(parent, &theme, game, menu, fit.window));
            }
        }
    }
    if relog {
        for mut node in &mut log_row {
            node.display = if prefs.play_history { Display::Flex } else { Display::None };
        }
        if prefs.play_history && let Ok(log) = log.single() {
            commands.entity(log).despawn_children().with_children(|parent| {
                for line in game.log.iter().rev().take(80).rev() {
                    parent.spawn((widgets::dim(&theme, line.trim().to_string()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
            });
            // The newest line is the one to read; the layout clamps this to
            // the real range.
            for mut position in &mut log_scroll {
                position.y = 1.0e6;
            }
        }
    }
    if reoverlay {
        for (overlay, _, _, _) in floating.iter().filter(|(_, overlay, _, _)| *overlay) {
            commands.entity(overlay).despawn();
        }
        if let Some(root) = roots.iter().next()
            && overlay_needed(game)
        {
            commands.entity(root).with_children(|parent| spawn_overlay(parent, &theme, &core, &images, game));
        }
    }
}

fn status_line(game: &Game) -> String {
    let Some(view) = &game.view else { return "Setting up…".to_string() };
    let phase = match view.phase {
        GamePhase::Mulligan(side) => format!("{side:?} decides on the opening hand"),
        GamePhase::StartOfTurn(side) => format!("{side:?}'s turn begins"),
        GamePhase::Action(side) => format!("{side:?}'s turn"),
        GamePhase::Discard { side, .. } => format!("{side:?} discards"),
        GamePhase::GameOver(side) => format!("{side:?} wins"),
    };
    match game.replay {
        Some(_) => format!("Turn {} · {phase} · from the {:?}'s chair", view.turn, game.side),
        None => format!("Turn {} · {phase} · you are the {:?}", view.turn, game.side),
    }
}

fn overlay_needed(game: &Game) -> bool {
    game.covered()
}

// ---- the board ----

#[allow(clippy::too_many_arguments)]
fn spawn_board(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, art: Option<&BoardArt>, game: &Game, transitions: &[Transition], fit: &BoardFit) {
    let drag = game.dragged_slot();
    // The control bar, directly above the person's hand: what they may do
    // sits between what they hold and the table, and acting never means
    // crossing the opponent's side. A row of the board, so it is redrawn
    // with it; a rail-only redraw refills it in place.
    let control_bar = |parent: &mut ChildSpawnerCommands, game: &Game| {
        parent
            .spawn((
                ControlBar,
                Node {
                    width: percent(100),
                    height: px(layout::CONTROL_BAR),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    ..default()
                },
            ))
            .with_children(|bar| spawn_control_bar(bar, theme, game));
    };
    let Some(view) = &game.view else {
        parent.spawn((widgets::dim(theme, "Waiting for the match to start…"), Node { flex_grow: 1.0, ..default() }));
        control_bar(parent, game);
        return;
    };
    let human = game.side;
    let opponent = human.other();
    let lit = Lit::of(transitions);
    // Top to bottom from either chair: the opponent's strip with their
    // hand as backs, the far area, the near area with the run lane on
    // whichever side of the ICE field faces the Runner, the control bar,
    // the person's strip with their hand. The Corp's servers are always
    // the area that grows (`layout::field_height`), with their plates on
    // the Corp's edge; the rig's row is reserved at its size whether or
    // not anything is in it, so nothing installed moves the middle.
    let mut opponent_row = strip_row();
    opponent_row.align_items = AlignItems::FlexStart;
    parent.spawn(opponent_row).with_children(|row| {
        spawn_strip(row, theme, core, images, art, game, view, opponent, fit);
        spawn_opponent_hand(row, theme, images, view, opponent, fit);
    });
    let lane = |parent: &mut ChildSpawnerCommands| {
        // The run lane, between the servers and the rig from either
        // chair, reserved whether or not a run is on (`layout::RUN_LANE`).
        parent.spawn((RunLane, run_lane_node())).with_children(|lane| fill_run_lane(lane, theme, core, game));
    };
    spawn_area(parent, theme, core, images, art, game, view, opponent, &lit, fit);
    lane(parent);
    spawn_area(parent, theme, core, images, art, game, view, human, &lit, fit);
    control_bar(parent, game);
    // The person's strip is the board's last row and sits on the window's
    // bottom edge: the hand's peek touches it, as the opponent's backs
    // touch the top, and whatever height the rows leave goes to the ICE
    // field between them rather than under the hand.
    let mut own_row = strip_row();
    own_row.align_items = AlignItems::FlexEnd;
    parent.spawn(own_row).with_children(|row| {
        spawn_strip(row, theme, core, images, art, game, view, human, fit);
        spawn_hand(row, theme, core, images, game, view, human, &lit, fit, drag);
    });
}

/// What the last transitions touched, so the redraw can outline it.
#[derive(Default)]
struct Lit {
    installs: Vec<InstallId>,
    hand: Vec<CardId>,
}

impl Lit {
    fn of(transitions: &[Transition]) -> Self {
        let mut lit = Lit::default();
        for transition in transitions {
            match transition {
                Transition::CardMoved { card, install, to, .. } => {
                    if let Some(id) = install {
                        lit.installs.push(*id);
                    }
                    if let (Some(card), Zone::Hand(_)) = (card, to) {
                        lit.hand.push(card.clone());
                    }
                }
                Transition::Revealed { install, .. } | Transition::Advancement { install, .. } => lit.installs.push(*install),
                _ => {}
            }
        }
        lit
    }
}

fn outline(theme: &Theme) -> Outline {
    Outline { width: px(3), offset: px(1), color: theme.accent }
}

/// The glow that says the engine will accept something on this card, in
/// the mood `netrunner_client::board::affordance` gave it: purple for a
/// move at the person's own pace, yellow for a moment that will pass.
///
/// **A `BoxShadow` rather than an `Outline`, and not as a matter of
/// taste**: `Outline` is spoken for three times over on this board
/// already — the transition highlight, the dragged card's lift and the
/// ice being encountered — and a node has exactly one. A shadow is a
/// second component, so a card can glow *and* be outlined in the same
/// frame, which is the common case: the card just drawn is usually also
/// a card that can be played. Neither costs any layout, so `face_width`'s
/// budget and the no-scroll rule are untouched (AGENTS.md §5).
///
/// `BoxShadow` is a `Vec<ShadowStyle>`, so the contact shadows the board
/// is owed next (ROADMAP Phase 7, the third list's item 1) can be a
/// second entry in the same component rather than a second component
/// contending for the same slot.
fn glow(commands: &mut Commands, entity: Entity, _theme: &Theme, mood: Option<Affordance>) {
    let Some(mood) = mood else { return };
    commands.entity(entity).insert(Glowing(mood));
}

/// Marks a glowing entity with its mood, so a test can ask the board
/// what it lit without reading a colour off a shadow.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glowing(pub Affordance);

/// Marks something that sits *on the table* — a card, a tile, a server
/// column — with the row it sits in, so it casts a contact shadow.
///
/// The board's chrome does not carry one: a control-bar button, a pop-up
/// or the phase bar is not an object on the field, and giving everything
/// a shadow is how a board ends up looking like a web page from 2013.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contact(pub Depth);

/// Composes the one `BoxShadow` a node may have from the two things that
/// want one: the contact shadow that sits it on the table, and the glow
/// that says the engine will accept something on it.
///
/// **They cannot be two components** — a node has exactly one `BoxShadow`
/// — and they must not be two `insert`s either, because the second would
/// silently replace the first. `BoxShadow` is a `Vec<ShadowStyle>`, so
/// this is the one place that builds the vector, and anything wanting a
/// third shadow later adds it here rather than contending for the slot.
///
/// Registered like `widgets::dress` and for the same reasons: on
/// `Added<..>` so a freshly spawned board is shadowed the frame after it
/// appears, and over everything when the [`Skin`] changes, because a skin
/// whose own pictures carry baked shadows turns the drawn ones off
/// (`Skin::wants_shadows`) and the settings row must take effect without
/// leaving the screen.
fn shadows(
    mut commands: Commands,
    theme: Res<Theme>,
    skin: Res<crate::skin::Skin>,
    added: Query<(Entity, Option<&Contact>, Option<&Glowing>), Or<(Added<Contact>, Added<Glowing>)>>,
    all: Query<(Entity, Option<&Contact>, Option<&Glowing>), Or<(With<Contact>, With<Glowing>)>>,
) {
    let wants = skin.wants_shadows();
    let compose = |commands: &mut Commands, entity: Entity, contact: Option<&Contact>, glowing: Option<&Glowing>| {
        let mut styles = Vec::new();
        // The glow goes first, which is the one that draws on top: a
        // halo the contact shadow has washed grey is not a signal.
        if let Some(Glowing(mood)) = glowing {
            let colour = match mood {
                Affordance::Usable => theme.glow_usable,
                Affordance::Conditional => theme.glow_conditional,
            };
            styles.push(ShadowStyle { color: colour, x_offset: px(0), y_offset: px(0), spread_radius: px(1), blur_radius: px(7) });
        }
        if let (Some(Contact(depth)), true) = (contact, wants) {
            let (y, blur, alpha) = depth.shadow();
            styles.push(ShadowStyle { color: Color::BLACK.with_alpha(alpha), x_offset: px(0), y_offset: px(y), spread_radius: px(0), blur_radius: px(blur) });
        }
        let mut entity = commands.entity(entity);
        if styles.is_empty() {
            entity.remove::<BoxShadow>();
        } else {
            entity.insert(BoxShadow(styles));
        }
    };
    if skin.is_changed() {
        for (entity, contact, glowing) in &all {
            compose(&mut commands, entity, contact, glowing);
        }
        return;
    }
    for (entity, contact, glowing) in &added {
        compose(&mut commands, entity, contact, glowing);
    }
}

fn section_label(parent: &mut ChildSpawnerCommands, theme: &Theme, text: impl Into<String>) {
    parent.spawn((widgets::dim(theme, text), Node { height: px(layout::LABEL), flex_shrink: 0.0, ..default() }));
}

/// A row that wraps: for a sheet's contents, where scrolling is the
/// overlay's and the board's rule does not apply.
fn wrap_row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::FlexStart, row_gap: px(6), column_gap: px(6), ..default() }
}

/// A row of cards on the board that never wraps: too many overlap instead.
fn card_row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, align_items: AlignItems::FlexStart, column_gap: px(layout::CARD_GAP), ..default() }
}

/// A strip beside a hand: the identity, the numbers, then the cards.
fn strip_row() -> Node {
    Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), min_height: px(0), ..default() }
}

/// Pulls the cards of a row together so `n` of them fit `available`:
/// every card after the first is moved left by the difference between
/// the natural advance and `layout::step`'s. Later cards draw over
/// earlier ones, as a held hand shows the right edge of each.
fn overlap(parent: &mut ChildSpawnerCommands, entities: &[Entity], width: f32, available: f32) {
    let step = layout::step(entities.len(), width, layout::CARD_GAP, available);
    let pull = step - (width + layout::CARD_GAP);
    if pull >= 0.0 {
        return;
    }
    for entity in entities.iter().skip(1) {
        parent.commands().entity(*entity).entry::<Node>().and_modify(move |mut node| node.margin.left = px(pull));
    }
}

/// A side's identity and numbers, at the strip's smaller size.
#[allow(clippy::too_many_arguments)]
fn spawn_strip(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, art: Option<&BoardArt>, game: &Game, view: &ClientView, side: Side, fit: &BoardFit) {
    let identity = match side {
        Side::Corp => view.corp.identity.clone(),
        Side::Runner => view.runner.identity.clone(),
    };
    let who = if side == game.side { "You" } else { "Opponent" };
    let colour = theme.side(side);
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                flex_shrink: 0.0,
                width: px(fit.identity_size(side).width() + layout::STRIP_TEXT),
                align_items: AlignItems::Center,
                column_gap: px(10),
                padding: UiRect::axes(px(6), px(2)),
                border: UiRect::left(px(4)),
                ..default()
            },
            BorderColor::all(colour),
        ))
        .with_children(|row| {
            if let Some(id) = &identity
                && let Some(card) = core.registry.get(id)
            {
                let image = card.numeric_id.and_then(|code| images.face(code, fit.identity_size(side)));
                let entity = spawn_face(row, theme, &Face::of(card), fit.identity_size(side), image, (Button, Click::Target(Target::Identity(side))));
                row.commands().entity(entity).insert(Contact(Depth::strip(side, game.side)));
                // An identity's own ability has nowhere else to live: it
                // is not an install and not in hand.
                glow(&mut row.commands(), entity, theme, game.affordance_for(&Target::Identity(side)));
            }
            row.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_width: px(0), row_gap: px(2), ..default() },)).with_children(|column| {
                let title = identity.as_ref().and_then(|id| core.registry.get(id)).map_or_else(|| format!("{side:?}"), |c| c.title.clone());
                column.spawn((Text::new(format!("{who} · {title}")), theme.font(size::SMALL), TextColor(colour), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                spawn_hud(column, theme, art, view, side);
                // The details line and the Runner's piles share one row, so
                // the strip is no taller than a hand's peek. The piles are
                // zones a click opens — the stack for its draw, the heap
                // for what is in it — as the Corp's centrals are through
                // their server plates.
                let details = hud::details(view, side);
                if details.is_some() || side == Side::Runner {
                    column.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), ..default() }).with_children(|piles| {
                        if let Some(details) = details {
                            piles.spawn(widgets::dim(theme, details));
                        }
                        if side == Side::Runner {
                            for (pile, label) in [(Pile::Stack, format!("Stack · {}", view.runner.stack_count)), (Pile::Heap, format!("Heap · {}", view.runner.heap.len()))] {
                                let entity = compact_button(piles, theme, label, Click::Target(Target::Pile(pile)));
                                glow(&mut piles.commands(), entity, theme, game.affordance_for(&Target::Pile(pile)));
                            }
                        }
                    });
                }
            });
        });
}

/// A button in the small size: seven server headers have to fit across
/// the board, and the shared button's body-size text does not.
fn compact_button(parent: &mut ChildSpawnerCommands, theme: &Theme, text: String, click: Click) -> Entity {
    parent
        .spawn((
            Button,
            widgets::Themed,
            click,
            Node {
                flex_shrink: 0.0,
                padding: UiRect::axes(px(10), px(6)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            BackgroundColor(theme.button),
            BorderColor::all(theme.panel_border),
            widgets::Dressed::button(theme, Slot::CompactButton, Drawn::new(theme.button, theme.panel_border)),
            children![(Text::new(text), theme.font(size::SMALL), TextColor(theme.text))],
        ))
        .id()
}

/// The HUD: a side's readouts as large numbers over short words, in a
/// grid of `hud::PER_ROW` columns so every number keeps its place from
/// one view to the next and from one side to the other. The numbers were
/// sentences in the strip before (Phase 7 §4 item 6) — dim and small
/// first, nobody saw them; body size next, they were a line to read. A
/// live threat is drawn in the danger colour rather than added, which is
/// `hud`'s rule. A readout that opens a zone — Agendas, the score area —
/// is a button, drawn with the buttons' fill so it reads as one.
fn spawn_hud(parent: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, view: &ClientView, side: Side) {
    let readouts = hud::readouts(view, side);
    parent
        .spawn((
            HudPanel(side),
            Node {
                display: Display::Grid,
                grid_template_columns: RepeatedGridTrack::flex(hud::PER_ROW as u16, 1.0),
                column_gap: px(6),
                row_gap: px(2),
                width: percent(100),
                ..default()
            },
        ))
        .with_children(|grid| {
            for readout in readouts {
                let colour = if readout.alarm { theme.danger } else { theme.text };
                let marker = HudReadout { label: readout.label, value: readout.value.clone() };
                let node = Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexStart, ..default() };
                // A readout that opens something is a button; the rest are
                // bare numbers. Both are slots, so a skin can put a plate
                // behind every readout and a brighter one behind the door.
                let mut cell = match readout.opens {
                    Some(pile) => grid.spawn((
                        marker,
                        Button,
                        widgets::Themed,
                        Click::Target(Target::Pile(pile)),
                        Node { padding: UiRect::axes(px(6), px(0)), margin: UiRect::left(px(-6)), border_radius: BorderRadius::all(px(6)), ..node },
                        BackgroundColor(theme.button),
                        widgets::Dressed::button(theme, Slot::HudCellOpens, Drawn::new(theme.button, Color::NONE)),
                    )),
                    None => grid.spawn((
                        marker,
                        node,
                        widgets::Dressed::still(
                            if readout.alarm { Slot::HudCellAlarm } else { Slot::HudCell },
                            Drawn::new(Color::NONE, Color::NONE),
                        ),
                    )),
                };
                cell.with_children(|cell| {
                        // The number, after Null Signal Games' own glyph for
                        // what it counts when the board has one; the word
                        // stays under it either way.
                        cell.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4), ..default() }).with_children(|line| {
                            if let Some(picture) = board_art::hud_key(readout.label).and_then(|key| art?.get(key)) {
                                line.spawn(board_art::glyph(picture, 18.0));
                            }
                            line.spawn((Text::new(readout.value), theme.font(size::HEADING), TextColor(colour)));
                        });
                        // One line: the HUD is a single row, and a word that
                        // wrapped ("Bad / pub.") made the strip a line taller.
                        cell.spawn((Text::new(readout.label), theme.font(size::SMALL), TextColor(if readout.alarm { theme.danger } else { theme.text_dim }), TextLayout::new(Justify::Left, LineBreak::NoWrap)));
                    });
            }
        });
}

/// The space a side's strip row leaves for its cards.
fn beside_strip(fit: &BoardFit, side: Side) -> f32 {
    fit.board_width() - (fit.identity_size(side).width() + layout::STRIP_TEXT) - 12.0
}

/// The window a hand of `n` cards is seen through: `layout::PEEK` of a
/// card's height, clipping the rest. The cards stay whole and laid out at
/// their full size inside it — only what shows is cut — so a drag, a
/// click and the hover lift all measure the card, not the sliver.
///
/// Its width is the row's, overlap included, and it has to be said: a
/// clipping node contributes nothing to its parent's size, so left to
/// the layout the window was as wide as the "Your hand" label over it.
fn peek_window(size: FaceSize, n: usize, available: f32) -> Node {
    let width = if n == 0 { 0.0 } else { layout::step(n, size.width(), layout::CARD_GAP, available) * (n - 1) as f32 + size.width() };
    Node { width: px(width), height: px((layout::PEEK * size.height()).round()), flex_shrink: 0.0, flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() }
}

/// The opponent's hand as backs at their side's size, the bottom
/// `layout::PEEK` of each hanging into the table from its far edge, and
/// overlapped when there are many — a count is in the strip, and a row
/// of backs is what a table shows.
fn spawn_opponent_hand(parent: &mut ChildSpawnerCommands, theme: &Theme, images: &CardImages, view: &ClientView, side: Side, fit: &BoardFit) {
    let count = match side {
        Side::Corp => view.corp.hq_count,
        Side::Runner => view.runner.grip_count,
    };
    let size = fit.size_of(side);
    let available = beside_strip(fit, side);
    // Hung from the top: the row is pulled up by the part that does not
    // show, so the window sees the backs' lower edge.
    let mut row_node = card_row();
    row_node.margin.top = px(-((1.0 - layout::PEEK) * size.height()).round());
    parent.spawn(peek_window(size, count, available)).with_children(|window| {
        window.spawn(row_node).with_children(|row| {
            let backs: Vec<Entity> = (0..count).map(|_| spawn_back(row, theme, images.back(side), side, size, Contact(Depth::Far))).collect();
            overlap(row, &backs, size.width(), available);
        });
    });
}

/// A side's board: the Corp's servers, or the Runner's rig.
#[allow(clippy::too_many_arguments)]
fn spawn_area(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, art: Option<&BoardArt>, game: &Game, view: &ClientView, side: Side, lit: &Lit, fit: &BoardFit) {
    let depth = Depth::area(side, game.side);
    match side {
        Side::Corp => spawn_servers(parent, theme, core, art, game, view, lit, fit, depth),
        Side::Runner => spawn_rig(parent, theme, core, images, art, game, view, lit, fit, depth),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_servers(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, art: Option<&BoardArt>, game: &Game, view: &ClientView, lit: &Lit, fit: &BoardFit, depth: Depth) {
    // Archives, R&D, HQ, then the remotes, from either chair, every
    // central a column even with nothing on it (`board::table_servers`).
    let mut servers = netrunner_client::board::table_servers(view);
    // A card held over the board lights the places it may go, and a
    // remote the Corp has not made yet gets a column of its own for the
    // length of the drag: the engine offers it, and there was nothing to
    // drop on.
    let places = game.drop_places();
    for place in &places {
        if let Target::Server(server @ ServerId::Remote(_)) = place
            && !servers.iter().any(|s| s.server == *server)
        {
            servers.push(ServerView { server: *server, ice: Vec::new(), root: Vec::new() });
        }
    }
    let run = view.active_run.as_ref();
    let encountered = run.filter(|r| matches!(r.phase, RunPhase::ApproachIce | RunPhase::EncounterIce)).and_then(|r| r.ice.get(r.position)).map(|i| i.install_id);
    let size = fit.size_of(Side::Corp);
    // Every column's tiles share one height, sized by the tallest: a
    // row of columns whose tiles differed by column read as a bar chart.
    let pieces = servers.iter().map(|s| s.ice.len() + s.root.len()).max().unwrap_or(0);
    let stack = layout::tile_stack(fit.field, pieces, size.width());
    // The area is the one row that grows: its columns span the ICE field
    // and end in their plates on the Corp's edge of the table.
    parent.spawn((Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0), ..default() },)).with_children(|area| {
        section_label(area, theme, "Servers");
        let mut row_node = card_row();
        row_node.flex_grow = 1.0;
        row_node.min_height = px(0);
        row_node.align_items = AlignItems::Stretch;
        area.spawn(row_node).with_children(|row| {
            for server in &servers {
                let under_run = game.run_on(server.server);
                let welcomes = places.contains(&Target::Server(server.server));
                let (border, slot) = if welcomes {
                    (theme.accent, Slot::ServerColumnWelcomes)
                } else if under_run {
                    (theme.runner, Slot::ServerColumnUnderRun)
                } else {
                    (theme.panel_border, Slot::ServerColumn)
                };
                row.spawn((
                    ServerColumn(server.server),
                    DropPlace(Target::Server(server.server)),
                    Node {
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0.0,
                        align_items: AlignItems::Center,
                        row_gap: px(4),
                        padding: UiRect::all(px(4)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(6)),
                        min_width: px(size.width() + 2.0 * layout::SERVER_CHROME - 2.0),
                        ..default()
                    },
                    // Mostly translucent: the column spans the whole ICE
                    // field now, and an opaque one walled the table off.
                    BackgroundColor(theme.panel.with_alpha(0.45)),
                    BorderColor::all(border),
                    widgets::Dressed::still(slot, Drawn::new(theme.panel.with_alpha(0.45), border)),
                    // The column is what sits on the table; its tiles sit
                    // on the column, one row nearer the light.
                    Contact(depth),
                ))
                .with_children(|column| {
                    // Plate, root and ice in the chair's order
                    // (`layout::column_top_down`): the plate nearest the
                    // Corp, the ice out toward the Runner, outermost
                    // nearest. The root and the ice share the stack, which
                    // takes the field's height and is pinned to the plate.
                    let mut tiles = Vec::new();
                    let pieces = layout::column_top_down(game.side);
                    let stack_node = Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0,
                        min_height: px(0),
                        align_items: AlignItems::Center,
                        justify_content: if game.side == Side::Corp { JustifyContent::FlexEnd } else { JustifyContent::FlexStart },
                        overflow: Overflow::clip(),
                        ..default()
                    };
                    let mut spawn_stack = |column: &mut ChildSpawnerCommands| {
                        column.spawn(stack_node.clone()).with_children(|stack_parent| {
                            for piece in pieces {
                                match piece {
                                    layout::Piece::Header => {}
                                    layout::Piece::Ice => tiles.extend(spawn_server_ice(stack_parent, theme, core, art, game, view, server, game.side, encountered, lit, size, stack.0, depth)),
                                    layout::Piece::Root => tiles.extend(spawn_server_root(stack_parent, theme, core, art, game, view, server, lit, size, stack.0, depth)),
                                }
                            }
                            // Pulled together by the stack's advance: its
                            // natural gap while they fit, overlapping past it.
                            let gap = stack.1 - stack.0;
                            for tile in tiles.iter().skip(1) {
                                stack_parent.commands().entity(*tile).entry::<Node>().and_modify(move |mut node| node.margin.top = px(gap));
                            }
                        });
                    };
                    if game.side == Side::Corp {
                        spawn_stack(column);
                        spawn_server_plate(column, theme, art, view, server.server, size, welcomes, under_run, game.affordance_for(&Target::Server(server.server)));
                    } else {
                        spawn_server_plate(column, theme, art, view, server.server, size, welcomes, under_run, game.affordance_for(&Target::Server(server.server)));
                        spawn_stack(column);
                    }
                });
            }
        });
    });
}

/// A server's plate: its picture (`board_art`, a drawn building until
/// somebody draws one) with its name and count along the lower edge, on
/// the Corp's edge of the table, in a box `layout::plate_height` tall
/// that is reserved for every server — so a picture never changes the
/// layout. Under a run it shows the server's `.run` picture, which is
/// its plain one until one is drawn. A click is the server's, as the
/// header's was.
#[allow(clippy::too_many_arguments)]
fn spawn_server_plate(column: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, view: &ClientView, server: ServerId, size: FaceSize, welcomes: bool, under_run: bool, mood: Option<Affordance>) {
    let on_board = view.corp.servers.iter().any(|s| s.server == server) || !matches!(server, ServerId::Remote(_));
    let count = match server {
        ServerId::Hq => format!(" · {}", view.corp.hq_count),
        ServerId::RnD => format!(" · {}", view.corp.rd_count),
        ServerId::Archives => format!(" · {}", view.corp.archives.len()),
        ServerId::Remote(_) => String::new(),
    };
    // The column a drag conjured says what it is: a server that does not
    // exist yet, named for what dropping there would make.
    let label = if on_board { format!("{}{count}", server_name(server)) } else { format!("New {}", server_name(server).to_lowercase()) };
    // A plate is a button wearing the header's hat: its own slot, so a
    // skin can draw Archives unlike a Stack button.
    let slot = if welcomes { Slot::ServerHeaderWelcomes } else { Slot::ServerHeader };
    let (width, height) = (size.width() + 4.0, layout::plate_height(size.width()));
    let mut plate = column.spawn((
        ServerPlate(server),
        Button,
        widgets::Themed,
        Click::Target(Target::Server(server)),
        Node {
            width: px(width),
            height: px(height),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Stretch,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(theme.button),
        BorderColor::all(theme.panel_border),
        widgets::Dressed::button(theme, slot, Drawn::new(theme.button, theme.panel_border)),
    ));
    plate.with_children(|plate| {
        // The picture first, so the label draws over it; inside the
        // border, which is what the crop covers.
        if let Some(picture) = art.and_then(|art| art.get(board_art::server_key(server, under_run))) {
            // The drawn plates are painted in the Corp's colour already.
            plate.spawn((PlatePicture(server), board_art::backdrop(picture, Vec2::new(width - 2.0, height - 2.0), Color::WHITE)));
        }
        // The name on a band of the panel, so it reads over any picture.
        plate.spawn((
            Node { justify_content: JustifyContent::Center, padding: UiRect::axes(px(6), px(3)), ..default() },
            BackgroundColor(theme.panel.with_alpha(0.72)),
            children![(Text::new(label), theme.font(size::SMALL), TextColor(theme.text))],
        ));
    });
    if welcomes {
        plate.insert(outline(theme));
    }
    let entity = plate.id();
    // A server glows for the run or the install the engine offers on it,
    // which is the only affordance on the board with no card to carry it.
    glow(&mut column.commands(), entity, theme, mood);
}

/// A token beside a number: the kind's glyph when the board has one
/// (`board_art::token_key`, falling back to a drawn disc), and the words
/// alone when it has no pictures at all — the headless tests, which read
/// the same `2/3 adv` the terminal client prints.
fn spawn_badge(parent: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, token: &Token, glyph: f32, text_size: f32, colour: Color) {
    let key = board_art::token_key(token.kind);
    match art.and_then(|art| art.get(key)) {
        Some(picture) => {
            parent
                .spawn((TokenBadge(key), Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(2), flex_shrink: 0.0, ..default() }))
                .with_children(|badge| {
                    badge.spawn(board_art::glyph(picture, glyph));
                    badge.spawn((Text::new(token.amount.clone()), theme.font(text_size), TextColor(colour)));
                });
        }
        None => {
            parent.spawn((TokenBadge(key), Text::new(token.words()), theme.font(text_size), TextColor(colour)));
        }
    }
}

/// What a tile shows: its words, its tokens, its picture and the colour
/// of its state — the border's, and what a drawn picture is washed in.
struct TileLook {
    title: String,
    tokens: Vec<Token>,
    key: &'static str,
    colour: Color,
    text_colour: Color,
}

/// A tile in a server column — an ice or a root card: a picture of what
/// kind of card it is (`board_art`: a face-down card, a barrier, an
/// asset…) behind the title when it may be named, its tokens as badges,
/// and a border in the card's faction colour when it is rezzed. A click
/// opens the card's menu and a secondary click its sheet; the card's own
/// picture is read there, not here, so a column is tiles `height` tall
/// (`layout::tile_stack`) and never a face.
#[allow(clippy::too_many_arguments)]
fn spawn_tile(column: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, look: TileLook, install: InstallId, lit: bool, size: FaceSize, height: f32, slot: Slot, mood: Option<Affordance>, depth: Depth) -> Entity {
    let width = size.width() + 4.0;
    let mut tile = column.spawn((
        Button,
        widgets::Themed,
        Click::Target(Target::Install(install)),
        DropPlace(Target::Install(install)),
        Node {
            width: px(width),
            height: px(height),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            overflow: Overflow::clip(),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(theme.button),
        BorderColor::all(look.colour),
        // The border carries the state — a rezzed ice is its faction's
        // colour — so that colour is what a picture asking for `"state"`
        // is washed in.
        widgets::Dressed::button(theme, slot, Drawn::new(theme.button, look.colour)),
    ));
    tile.with_children(|tile| {
        if let Some(picture) = art.and_then(|art| art.get(look.key)) {
            tile.spawn((TileArt(look.key), board_art::backdrop(picture, Vec2::new(width - 2.0, height - 2.0), look.colour)));
        }
        // The words on a band, so they read over any picture; the badges
        // beside them at the tile's text size.
        let text_size = size::SMALL - 3.0;
        tile.spawn((
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), padding: UiRect::axes(px(6), px(1)), border_radius: BorderRadius::all(px(3)), ..default() },
            BackgroundColor(theme.panel.with_alpha(0.7)),
        ))
        .with_children(|band| {
            band.spawn((Text::new(look.title), theme.font(text_size), TextColor(look.text_colour)));
            for token in &look.tokens {
                spawn_badge(band, theme, art, token, (height - 8.0).clamp(12.0, 18.0), text_size, look.text_colour);
            }
        });
    });
    if lit {
        tile.insert(outline(theme));
    }
    let entity = tile.id();
    column.commands().entity(entity).insert(Contact(depth.nearer()));
    glow(&mut column.commands(), entity, theme, mood);
    entity
}

/// A server's ice as tiles, titled by `board::facts::tile_title` — the
/// title when it may be named, rezzed or unrezzed, its strength now —
/// with its tokens as badges (`facts::tile_tokens`) and the run's
/// marker on the piece being approached.
#[allow(clippy::too_many_arguments)]
fn spawn_server_ice(column: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, art: Option<&BoardArt>, game: &Game, view: &ClientView, server: &ServerView, chair: Side, encountered: Option<InstallId>, lit: &Lit, size: FaceSize, height: f32, depth: Depth) -> Vec<Entity> {
    let mut tiles = Vec::new();
    for ice in layout::ice_top_down(&server.ice, chair) {
        let def = ice.card.as_ref().and_then(|id| core.registry.get(id));
        let kind = def.and_then(|d| match &d.card_type {
            CardType::Ice(kind) => Some(*kind),
            _ => None,
        });
        let look = TileLook {
            title: facts::tile_title(view, ice.install_id, &core.registry),
            tokens: facts::tile_tokens(view, ice.install_id, &core.registry),
            key: board_art::ice_key(ice.rezzed, kind),
            colour: if ice.rezzed { theme.faction(def.and_then(|c| c.faction)) } else { theme.corp.with_alpha(0.5) },
            text_colour: if ice.rezzed { theme.text } else { theme.text_dim },
        };
        let is_lit = encountered == Some(ice.install_id) || lit.installs.contains(&ice.install_id);
        let slot = if ice.rezzed { Slot::TileRezzed } else { Slot::TileUnrezzed };
        tiles.push(spawn_tile(column, theme, art, look, ice.install_id, is_lit, size, height, slot, game.affordance_for(&Target::Install(ice.install_id)), depth));
    }
    tiles
}

/// The cards in a server's root as tiles, titled by
/// `board::facts::tile_title`: rezzed or unrezzed for an asset or an
/// upgrade, the title alone for an agenda the viewer knows, `face down`
/// for a card the viewer cannot name — with its advancement (public) and
/// counters as badges. An agenda's border is its faction's: it has no rez
/// to wait for.
#[allow(clippy::too_many_arguments)]
fn spawn_server_root(column: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, art: Option<&BoardArt>, game: &Game, view: &ClientView, server: &ServerView, lit: &Lit, size: FaceSize, height: f32, depth: Depth) -> Vec<Entity> {
    let mut tiles = Vec::new();
    for card in &server.root {
        let def = card.card.as_ref().and_then(|id| core.registry.get(id));
        let face_up = card.rezzed || def.is_some_and(|d| d.card_type == CardType::Agenda);
        let look = TileLook {
            title: facts::tile_title(view, card.install_id, &core.registry),
            tokens: facts::tile_tokens(view, card.install_id, &core.registry),
            key: board_art::root_key(face_up, def.map(|d| &d.card_type)),
            colour: if face_up { theme.faction(def.and_then(|c| c.faction)) } else { theme.corp.with_alpha(0.5) },
            text_colour: if face_up { theme.text } else { theme.text_dim },
        };
        let slot = if face_up { Slot::TileRezzed } else { Slot::TileUnrezzed };
        tiles.push(spawn_tile(column, theme, art, look, card.install_id, lit.installs.contains(&card.install_id), size, height, slot, game.affordance_for(&Target::Install(card.install_id)), depth));
    }
    tiles
}

/// The rig as three rows — programs, hardware, resources — in the order
/// the chair sees the table (`netrunner_client::board::rig::rows_top_down`: programs the row
/// nearest the ICE), each the top `layout::PEEK` of its cards over their
/// chip line, overlapped by `layout::step` when a row would not fit
/// across. Drawn at the Runner's side of the table's size — full from the
/// Runner's chair, `layout::OPPONENT_SCALE` from the Corp's — with every
/// row reserved at `layout::rig_row_height` whether or not anything is in
/// it, so the first install moves nothing. A row's label sits at its left
/// rather than over it: the rig has width to spare and no height.
#[allow(clippy::too_many_arguments)]
fn spawn_rig(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, art: Option<&BoardArt>, game: &Game, view: &ClientView, lit: &Lit, fit: &BoardFit, depth: Depth) {
    let size = fit.size_of(Side::Runner);
    let row_height = layout::rig_row_height(fit.area_face(Side::Runner));
    let available = fit.board_width() - layout::RIG_LABEL_WIDTH;
    parent.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, row_gap: px(layout::RIG_ROW_GAP), height: px(layout::rig_height(fit.area_face(Side::Runner))), overflow: Overflow::clip(), ..default() },)).with_children(|area| {
        for (wanted, cards) in netrunner_client::board::rig::rows(view, &core.registry, fit.chair) {
            let step = layout::step(cards.len(), size.width(), layout::CARD_GAP, available);
            let pull = (step - (size.width() + layout::CARD_GAP)).min(0.0);
            area.spawn((Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, height: px(row_height), ..default() },)).with_children(|row| {
                row.spawn((widgets::dim(theme, wanted.label()), Node { width: px(layout::RIG_LABEL_WIDTH), flex_shrink: 0.0, ..default() }));
                row.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() },)).with_children(|column| {
                    column.spawn(peek_window(size, cards.len(), available)).with_children(|window| {
                        window.spawn(card_row()).with_children(|cards_row| {
                            for (i, card) in cards.iter().enumerate() {
                                let Some(def) = core.registry.get(&card.card) else { continue };
                                let image = def.numeric_id.and_then(|code| images.face(code, size));
                                let entity = spawn_face(cards_row, theme, &Face::of(def), size, image, (Button, Click::Target(Target::Install(card.install_id))));
                                if i > 0 && pull < 0.0 {
                                    cards_row.commands().entity(entity).entry::<Node>().and_modify(move |mut node| node.margin.left = px(pull));
                                }
                                cards_row.commands().entity(entity).insert(Contact(depth));
                                glow(&mut cards_row.commands(), entity, theme, game.affordance_for(&Target::Install(card.install_id)));
                                if lit.installs.contains(&card.install_id) {
                                    cards_row.commands().entity(entity).insert(outline(theme));
                                }
                            }
                        });
                    });
                    // The chip line under the peek, one slot per card at the
                    // row's own step, so a card's numbers sit under it:
                    // strength and hosted cards as words, the counters as
                    // their kind's badge (a virus program's virus counters,
                    // a credit resource's credits).
                    column.spawn(Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, column_gap: px(layout::CARD_GAP), height: px(layout::CHIPS), ..default() }).with_children(|line| {
                        for (i, card) in cards.iter().enumerate() {
                            let Some(def) = core.registry.get(&card.card) else { continue };
                            let mut slot = line.spawn(Node { width: px(size.width()), flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::Center, column_gap: px(8), overflow: Overflow::clip(), ..default() });
                            if i > 0 && pull < 0.0 {
                                slot.entry::<Node>().and_modify(move |mut node| node.margin.left = px(pull));
                            }
                            let mut chips = Vec::new();
                            if def.card_type == CardType::Program && def.strength.is_some() {
                                chips.push(format!("str {}", card.current_strength));
                            }
                            if !card.hosted_cards.is_empty() {
                                chips.push(format!("{} hosted", card.hosted_cards.len()));
                            }
                            slot.with_children(|slot| {
                                if !chips.is_empty() {
                                    slot.spawn(widgets::dim(theme, chips.join(" · ")));
                                }
                                if card.counters > 0 {
                                    let token = Token { kind: TokenKind::Counter(def.counter_kind), amount: card.counters.to_string() };
                                    spawn_badge(slot, theme, art, &token, 16.0, size::SMALL, theme.text_dim);
                                }
                            });
                        }
                    });
                });
            });
        }
    });
}

/// The person's hand beside their strip, the top `layout::PEEK` of each
/// card showing and the rest below the table's edge, overlapped when it
/// is wide. A hovered card lifts out whole (`lift_hovered`).
#[allow(clippy::too_many_arguments)]
fn spawn_hand(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, side: Side, lit: &Lit, fit: &BoardFit, drag: Option<usize>) {
    // The person's own order for their own hand, the view's for the
    // opponent's (which is drawn as backs anyway).
    let own = side == game.side;
    let from_view = match side {
        Side::Corp => view.corp.hq_cards.as_deref(),
        Side::Runner => view.runner.grip_cards.as_deref(),
    }
    .unwrap_or(&[]);
    let hand = if own { game.hand.cards() } else { from_view };
    let size = fit.size_of(side);
    let available = beside_strip(fit, side);
    parent.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() },)).with_children(|column| {
        section_label(column, theme, format!("Your hand · {}", hand.len()));
        column.spawn(peek_window(size, hand.len(), available)).with_children(|window| {
            window.spawn(card_row()).with_children(|row| {
                // A hand is a multiset; the first copy of a lit card is the
                // one outlined, which is as much as a highlight can say.
                let mut lit_left = lit.hand.clone();
                let mut faces = Vec::new();
                for (slot, id) in hand.iter().enumerate() {
                    let Some(def) = core.registry.get(id) else { continue };
                    let image = def.numeric_id.and_then(|code| images.face(code, size));
                    let entity = spawn_face(row, theme, &Face::of(def), size, image, (Button, Click::Target(Target::HandCard(id.clone()))));
                    if own {
                        // Its place in the row, so a drag knows which card
                        // it picked up and where the others sit.
                        row.commands().entity(entity).insert(HandSlot(slot));
                    }
                    row.commands().entity(entity).insert(Contact(Depth::strip(side, game.side)));
                    if own {
                        glow(&mut row.commands(), entity, theme, game.affordance_for(&Target::HandCard(id.clone())));
                    }
                    if let Some(at) = lit_left.iter().position(|c| c == id) {
                        lit_left.swap_remove(at);
                        row.commands().entity(entity).insert(outline(theme));
                    }
                    // The card being dragged is lifted out of the row by its
                    // outline: the row itself never moves under the pointer,
                    // because a hand that re-flowed mid-drag moved the gap
                    // the person was aiming at.
                    if own && drag.is_some_and(|dragged| dragged == slot) {
                        row.commands().entity(entity).insert(outline(theme));
                    }
                    faces.push(entity);
                }
                overlap(row, &faces, size.width(), available);
            });
        });
    });
}

/// The full card a hovered hand card lifts out as, pointing back at the
/// face it lifted from.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiftedCard(pub Entity);

/// How far above the table's edge a lifted card's bottom sits.
const LIFT: f32 = 12.0;

/// A hand shows a third of each card (`layout::PEEK`); the card the
/// pointer rests on lifts out whole, drawn over the board above its
/// slot, so a hand can be read without a click.
///
/// **Paint, not play.** The lifted card is a second, non-interactive
/// copy in a floating layer: the face in the row never moves, so a
/// drag, a click and the hand's own order measure the same box they
/// always did, and nothing about it enters a `PlayerAction`, the log or
/// a `ClientView` — considering a card is not a game event (the third
/// list's item 3). It carries no `Button`, so the focus system passes
/// through it to the face beneath and the hover holds. Nothing lifts
/// while a card is being dragged, a menu is open or the board is
/// covered: each of those is already showing the person something.
#[allow(clippy::too_many_arguments)]
fn lift_hovered(
    mut commands: Commands,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    images: Res<CardImages>,
    model: Option<Res<Model>>,
    fit: Option<Res<BoardFit>>,
    slots: Query<(Entity, &HandSlot, &Interaction, &ComputedNode, &UiGlobalTransform)>,
    mut lifted: Query<(Entity, &LiftedCard, &mut Node)>,
    roots: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    dev: Option<Res<crate::dev::Dev>>,
) {
    let (Some(model), Some(fit)) = (model, fit) else { return };
    let game = &model.0;
    let quiet = game.dragging.is_none() && game.menu.is_none() && !game.covered();
    // `NETRUNNER_LIFT`: the first card is held lifted for a screenshot,
    // as if the pointer rested on it.
    let forced = dev.is_some_and(|dev| dev.lift && dev.autoplayed >= dev.autoplay);
    let hovered = if quiet { slots.iter().find(|(_, slot, interaction, _, _)| **interaction == Interaction::Hovered || (forced && slot.0 == 0)) } else { None };
    let size = fit.size_of(game.side);
    // The face's box is the whole card, laid out below the peek window;
    // the lifted copy is raised until its bottom clears that window.
    // Placed every frame rather than once: the face under a freshly drawn
    // board has no laid-out box until the frame after it is spawned.
    let place = |node: &ComputedNode, transform: &UiGlobalTransform| {
        let anchor = anchor_of(node, transform);
        let top = anchor.y - anchor.height / 2.0 - (1.0 - layout::PEEK) * size.height() - LIFT;
        let left = (anchor.x - size.width() / 2.0).clamp(layout::PADDING, (fit.window.x - size.width() - layout::PADDING).max(layout::PADDING));
        (left, top.max(0.0))
    };
    if let Some((entity, _, _, node, transform)) = hovered
        && let Some((_, _, mut lifted_node)) = lifted.iter_mut().find(|(_, over, _)| over.0 == entity)
    {
        let (left, top) = place(node, transform);
        if lifted_node.left != px(left) || lifted_node.top != px(top) {
            lifted_node.left = px(left);
            lifted_node.top = px(top);
        }
        return;
    }
    for (entity, _, _) in &lifted {
        commands.entity(entity).despawn();
    }
    let Some((entity, slot, _, node, transform)) = hovered else { return };
    let Some(def) = game.hand.cards().get(slot.0).and_then(|id| core.registry.get(id)) else { return };
    let Some(root) = roots.iter().next() else { return };
    let (left, top) = place(node, transform);
    let image = def.numeric_id.and_then(|code| images.face(code, size));
    commands.entity(root).with_children(|parent| {
        let face = spawn_face(parent, &theme, &Face::of(def), size, image, (LiftedCard(entity), Pickable::IGNORE, GlobalZIndex(12)));
        parent.commands().entity(face).entry::<Node>().and_modify(move |mut node| {
            node.position_type = PositionType::Absolute;
            node.left = px(left);
            node.top = px(top);
        });
        parent.commands().entity(face).insert(Contact(Depth::Near));
    });
}

// ---- the control bar and the rail ----

/// One button per control of the person's side, in the bar's fixed
/// order; enabled when the engine lists what it means and the person
/// may act, greyed otherwise. Greyed rather than absent so "End turn"
/// is always in the same place.
fn spawn_control_bar(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game) {
    // A replay's bar moves through the record instead: the same row, so
    // the board keeps its layout, and nothing on it is an action.
    if let Some(at) = &game.replay {
        for step in crate::models::replay::Step::BAR {
            if step.moves(at.cursor, at.len) {
                parent.spawn(widgets::button(theme, step.label(), Val::Auto, ReplayClick(step)));
            } else {
                parent.spawn(widgets::disabled_button(theme, step.label(), Val::Auto, ReplayClick(step)));
            }
        }
        return;
    }
    for control in Control::for_side(game.side) {
        let offered = game.awaiting && game.actions.for_control(*control).is_some();
        if offered {
            parent.spawn(widgets::button(theme, control.label(), Val::Auto, Click::Control(*control)));
        } else {
            parent.spawn(widgets::disabled_button(theme, control.label(), Val::Auto, Click::Control(*control)));
        }
    }
}

/// The prompt, the decisions it is asking, and — with the play helper on
/// — every legal action in the engine's order.
fn spawn_rail(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, helper: bool) {
    if let Some(at) = &game.replay {
        parent.spawn((widgets::label(theme, format!("Replay · step {} of {}", at.cursor, at.len)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        parent.spawn((widgets::dim(theme, at.title.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if let Some(prompt) = &game.prompt {
        parent.spawn((widgets::label(theme, prompt.title.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        if !prompt.detail.is_empty() {
            parent.spawn((widgets::dim(theme, prompt.detail.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
    }
    if let Some(rejection) = &game.rejection {
        parent.spawn((widgets::notice(theme, format!("Rejected: {rejection}"), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if let Some(reason) = &game.break_stopped {
        parent.spawn((widgets::notice(theme, reason.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if let Some(reason) = &game.stalled {
        parent.spawn((widgets::notice(theme, reason.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        return;
    }
    if game.over.is_some() {
        parent.spawn(widgets::dim(theme, "The match is over."));
        return;
    }
    // The state of the encounter, above the routes that change it: the
    // ice being met and every subroutine marked broken, fired or still
    // pending (`Game::encounter`). It is here and not only in the ice's
    // own sheet because an encounter is a thing a person decides *in*,
    // and a sheet is a click away and a click back.
    //
    // **Above the `awaiting` return, so the marks stay up while the Corp
    // thinks.** They are something to read, not something to press.
    //
    // **The marks are the terminal client's, character for character** —
    // `[x]`, `[!]`, `[ ]` — rather than a tick glyph: the bundled Noto
    // fallback is not guaranteed to carry one, and a person moving
    // between the two clients should not have to learn the marks twice.
    // The colour is what this client adds.
    //
    // **The tiles are deliberately left alone.** A pip per subroutine on
    // the encountered tile was the other candidate and was rejected: the
    // run lane already draws exactly that, a dot per subroutine in these
    // three colours, so a tile pip would be the same fact a third time
    // and still not say *which* subroutine. The words are what was
    // missing, and they need a column's width.
    if let Some(met) = game.encounter() {
        let name = met.card.as_ref().and_then(|id| game.registry().get(id)).map_or_else(|| "Ice".to_string(), |def| def.title.clone());
        parent.spawn((widgets::label(theme, format!("{name} · strength {}", met.strength)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        for sub in &met.subroutines {
            let (mark, colour) = match sub.status {
                SubroutineStatus::Broken => ("[x]", theme.text_dim),
                SubroutineStatus::Resolved => ("[!]", theme.danger),
                SubroutineStatus::Pending => ("[ ]", theme.text),
            };
            parent.spawn((
                Text::new(format!("{mark} {}", sub.text)),
                theme.font(size::SMALL),
                TextColor(colour),
                TextLayout::new(Justify::Left, LineBreak::WordBoundary),
            ));
        }
    }
    if game.replay.is_some() {
        parent.spawn((
            widgets::dim(theme, "Left and Right step, Page Up and Down ten at a time, Home and End go to either end, S is the other chair. A click reads a card."),
            TextLayout::new(Justify::Left, LineBreak::WordBoundary),
        ));
        return;
    }
    if !game.awaiting {
        parent.spawn(widgets::dim(theme, if game.view.is_some() { "Opponent is thinking…" } else { "Setting up…" }));
        return;
    }
    // The decisions are the pop-up's (`spawn_decision_popup`), not the
    // rail's; the rail keeps the prompt's words and, when on, the flat
    // panel. An encounter's routes are the exception: they sit under the
    // prompt, because an encounter has no pop-up and one on every piece
    // of ICE would cover the ICE it is about. The number keys press them
    // as they press the pop-up's.
    let view = game.view.as_ref();
    for (index, route) in game.breaks.iter().enumerate() {
        let label = view.map_or_else(|| route.price(), |view| route.label(view, game.registry()));
        let mut button = parent.spawn(widgets::button(theme, label, percent(100), Click::Break(index)));
        button.entry::<Node>().and_modify(|mut node| {
            node.justify_content = JustifyContent::FlexStart;
            node.padding = UiRect::axes(px(10), px(6));
        });
    }
    // Beside the routes and not on the control bar: the bar is the basic
    // actions, greyed when the engine does not list them, and this is
    // not an action.
    if let Some(label) = game.back_label() {
        parent.spawn(widgets::button(theme, label, percent(100), Click::TakeBack));
    }
    if !helper {
        if game.prompt.is_none() {
            parent.spawn((widgets::dim(theme, "Your turn: click a card or a zone for what it can do, right-click to read it, or use the bar. ? lists the keys."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
        return;
    }
    parent.spawn(widgets::label(theme, "Every action"));
    let list = parent
        .spawn((
            ActionList,
            bevy::ui_widgets::ScrollArea,
            Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(4), overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|list| {
            for index in 0..game.actions.entries.len() {
                entry_button(list, theme, game, index, percent(100));
            }
        })
        .id();
    parent.spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(list).with_children(|row| {
        row.spawn(widgets::scrollbar(theme, list));
    });
}

/// A button for entry `index`, its label left-aligned, at `width`.
///
/// **The width is the caller's** because two of the three callers are
/// inside a wrapping container, where a percentage has nothing to
/// resolve against while the container is measured: the label came out
/// measured a word to a line and every row kept that height, which ran
/// two rows of cards off the window. The rail's list is the one caller
/// still in ordinary flow, and it passes `percent(100)`.
fn entry_button(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, index: usize, width: Val) -> Option<Entity> {
    let entry = game.actions.entries.get(index)?;
    let mut button = parent.spawn(widgets::button(theme, entry.label.clone(), width, Click::Entry(index)));
    button.entry::<Node>().and_modify(|mut node| {
        node.justify_content = JustifyContent::FlexStart;
        node.padding = UiRect::axes(px(10), px(6));
    });
    Some(button.id())
}

/// The menu a click opened: the target's name and one button per
/// entry, or a line saying so when there is nothing,
/// in a small panel just above the box the target was laid out in and
/// centred on it — so it is in one place for a card however the card
/// was clicked, and the card stays in view beneath it. When the box is
/// too near the top for the menu to fit above, it sits just below
/// instead (a header along the top edge, from the Runner's chair). The
/// panel takes `Interaction` and blocks, so a click on its ground is a
/// click on the menu, not on the card beneath. Between the decision
/// pop-up and the overlays in depth: a sheet covers it, it covers the
/// pop-up.
///
/// **It is pinned by the edge that faces the card and grows away from
/// it** ([`layout::menu_box`]), so its height is never guessed and it
/// cannot leave the window. What this replaced computed a `top` from an
/// estimate — a 40px row per entry — and a menu whose labels wrapped was
/// half as tall again, so its last options went off the bottom of the
/// window, where the screen root's `Overflow::clip` ate them. Measuring
/// the panel once the layout had run was the other way and was rejected:
/// `ComputedNode` is written in `PostUpdate`, so a measured placement is
/// a frame late by construction — a flicker on the most-used click on
/// the board — and the headless tests have no layout pass to measure it
/// with.
///
/// **The panel is itself the wrapping container**, so when the room on
/// its side runs out the entries flow into a second column rather than
/// being clipped: every option stays on the window, which is what
/// opening a menu promised. A scrolling list was asked about and turned
/// down — a scroll bar makes an option reachable, not visible. An inner
/// entries container would not do, because *its* auto width is measured
/// under a max-content constraint, where taffy never wraps; the panel is
/// `position: Absolute` and so is measured under a definite space, where
/// it does. Hence also the `px` width on every child: a percentage has
/// nothing to resolve against there. With two columns the heading sits
/// atop the first rather than spanning both, which reads as a title and
/// costs nothing.
fn spawn_actions_menu(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, menu: &crate::models::game::Menu, window: Vec2) {
    let place = layout::menu_box((window.x, window.y), menu.over, menu.entries.len());
    let accent = theme.accent;
    let mut panel = parent.spawn((ActionsMenu, MenuPart, Interaction::None, FocusPolicy::Block, GlobalZIndex(15), widgets::panel(theme, Val::Auto)));
    // Its own slot, replacing the one `widgets::panel` supplied, so a
    // skin can paint the menu apart from the sheet. The `and_modify`
    // below stays: `dress` only runs on `Added<Dressed>`, a frame after
    // the commands flush, so leaving the accent to it would show the
    // panel's own border colour for that frame.
    panel.insert(widgets::Dressed::still(Slot::PanelMenu, Drawn::new(theme.panel, accent)));
    panel.entry::<Node>().and_modify(move |mut node| {
        node.position_type = PositionType::Absolute;
        // Wrapping is always on and only ever bites when the room runs
        // out, so the one-column menu every screenshot shows is the same
        // code path as the two-column one.
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(layout::ROW_GAP);
        node.align_content = AlignContent::FlexStart;
        node.padding = UiRect::all(px(layout::MENU_PADDING));
        place_node(&mut node, place);
    });
    panel.entry::<BorderColor>().and_modify(move |mut border| *border = BorderColor::all(accent));
    panel.with_children(|panel| {
        let row = px(layout::MENU_ENTRY);
        panel.spawn((widgets::label(theme, target_title(game, &menu.target)), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { width: row, ..default() }));
        if menu.entries.is_empty() {
            let line = if game.awaiting { "Nothing to do here right now." } else { "Not your decision right now." };
            panel.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { width: row, ..default() }));
        }
        for index in &menu.entries {
            if let Some(button) = entry_button(panel, theme, game, *index, row) {
                panel.commands().entity(button).insert(MenuPart);
            }
        }
    });
}

/// Writes a [`layout::MenuBox`] onto a node: one horizontal inset and one
/// vertical, the other of each left `Val::Auto` so the panel grows away
/// from the edge it was pinned to, and the room that side has as its
/// maximum.
fn place_node(node: &mut Node, place: layout::MenuBox) {
    node.left = place.left.map_or(Val::Auto, px);
    node.right = place.right.map_or(Val::Auto, px);
    node.top = place.top.map_or(Val::Auto, px);
    node.bottom = place.bottom.map_or(Val::Auto, px);
    node.max_width = px(place.max_width);
    node.max_height = px(place.max_height);
}

/// Keeps an open menu on the window and on its card.
///
/// It is placed from two things that can both move under it — the
/// window, which a resize changes, and the box the target was laid out
/// in, which any board redraw changes — while the menu itself is spawned
/// only by a *rail* redraw. Without this, a resize left the menu where
/// the old window had put it. Re-anchoring rather than respawning,
/// because a respawn would take the hover and the pressed state of the
/// button under the pointer with it.
///
/// A target whose `ComputedNode` is empty is skipped: it has not been
/// laid out yet — the frame after a redraw, and every frame in the
/// headless tests, which run without a `UiPlugin` — and its zero-size
/// box would drag the menu into the window's corner.
fn place_menu(fit: Option<Res<BoardFit>>, model: Option<Res<Model>>, targets: Query<(&Click, &ComputedNode, &UiGlobalTransform)>, mut panel: Query<&mut Node, With<ActionsMenu>>) {
    let (Some(fit), Some(model)) = (fit, model) else { return };
    let Ok(mut node) = panel.single_mut() else { return };
    let Some(menu) = &model.0.menu else { return };
    let over = targets
        .iter()
        .find(|(click, computed, _)| matches!(click, Click::Target(target) if *target == menu.target) && !computed.is_empty())
        .map_or(menu.over, |(_, computed, transform)| anchor_of(computed, transform));
    let place = layout::menu_box((fit.window.x, fit.window.y), over, menu.entries.len());
    let mut fresh = node.clone();
    place_node(&mut fresh, place);
    if fresh != *node {
        *node = fresh;
    }
}

/// The narrowest the decision pop-up's panel is drawn, and what its own
/// padding and border take off that on each side.
const POPUP_MIN_WIDTH: f32 = 520.0;
const POPUP_PADDING: f32 = 17.0;

/// The decision pop-up: the prompt's words as its heading and one
/// button per decision, centred over the board. The container is the
/// whole window so the panel can be centred in it, and ignores picking
/// so the board beneath stays clickable; only the panel and its
/// buttons are hit. Under the overlays (`GlobalZIndex(20)`), so a
/// sheet opened to read a card covers it.
///
/// **At an access the card itself is in the panel, above the heading**
/// (`netrunner_client::access`), which makes this the second thing on
/// the board to carry a card and actions together — the score area
/// being the first. §4g's rule that a reading surface offers nothing
/// holds everywhere it can: a card on the table has a tile, so its
/// actions belong on the menu that tile's click opens, and its sheet
/// stays read-only. An accessed card has no tile. It is in HQ or R&D,
/// face down, and this panel is the only place it exists, so the
/// alternative to putting its actions here is a person reading a name
/// and guessing. The face is `FaceSize::Large`, the same as a sheet's,
/// and fits the 520px panel with room to spare.
fn spawn_decision_popup(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, decisions: &[usize], window: Vec2) {
    let view = game.view.as_ref();
    // The access owns the panel's words when there is one: its title
    // names the card, and its facts are the live costs (a grid may have
    // raised the printed trash cost) rather than `Prompt`'s single line.
    let access = view.and_then(|view| Access::of(view, &core.registry));
    let (title, detail) = match (&access, &game.prompt) {
        (Some(access), _) => (access.title(), access.facts().join("\n")),
        (None, Some(prompt)) => (prompt.title.clone(), prompt.detail.clone()),
        (None, None) => ("Your decision".to_string(), String::new()),
    };
    // A card selection's candidates are drawn as their cards, each with
    // its own button under it; the rest (the confirm, "Choose none")
    // stay a list. The cards keep their positions' order, so the one just
    // selected does not jump to the end of the row.
    let selection = game.actions.selection();
    let position_of = |index: usize| match &game.actions.entries[index].action {
        PlayerAction::ToggleCardSelection { position } if selection.is_some_and(|s| s.candidate(*position).is_some()) => Some(*position),
        _ => None,
    };
    let mut choices: Vec<(usize, usize)> = decisions.iter().filter_map(|index| position_of(*index).map(|position| (position, *index))).collect();
    choices.sort();
    let buttons: Vec<usize> = decisions.iter().copied().filter(|index| position_of(*index).is_none()).collect();
    // Otherwise the one card the prompt is about: the accessed card, the
    // card going in, or the card whose text is asking.
    let single: Option<CardId> = match &access {
        Some(access) => Some(access.card.clone()),
        None if choices.is_empty() => view.and_then(|view| Prompt::card(view, &core.registry)),
        None => None,
    };
    // What the words and the list of buttons take, guessed before the
    // layout has run: the panel's padding, the heading and the lines
    // under it, a button per row with the gap between each, and the
    // caption under a lone card. The cards get the rest.
    //
    // **A guess, and no longer load-bearing.** It decides only how big
    // the cards are *drawn*; what keeps every button on the window is
    // the cap on the panel below and the card row being the only thing
    // that gives. It used to count one line per line of text, so a
    // heading or a button label that wrapped was a row of the panel
    // nobody had budgeted, and the buttons under it went off the bottom
    // — the same fault the actions menu had, in the one place a pop-up
    // can have it. The width assumed is the narrowest the panel can be,
    // so a panel that comes out wider has fewer lines than this, never
    // more.
    let inner = POPUP_MIN_WIDTH - 2.0 * POPUP_PADDING;
    let lines = |text: &str, size: f32| layout::wrapped_lines(text, inner, size) as f32;
    let label_rows: f32 = buttons.iter().filter_map(|index| game.actions.entries.get(*index)).map(|entry| lines(&entry.label, size::BODY) * 22.0 + 14.0 + layout::ROW_GAP).sum();
    let chrome = 2.0 * 16.0
        + lines(&title, size::HEADING) * 30.0
        + 8.0
        + if detail.is_empty() { 0.0 } else { lines(&detail, size::SMALL) * 22.0 + 8.0 }
        + game.rejection.as_ref().map_or(0.0, |_| 30.0)
        + label_rows
        + game.back_label().map_or(0.0, |_| 22.0 + 14.0 + layout::ROW_GAP)
        + if choices.is_empty() { layout::CHOICE_CAPTION } else { 0.0 };
    let available = (window.x - 2.0 * layout::PADDING - 2.0 * POPUP_PADDING, window.y - 2.0 * layout::PADDING - chrome);
    let count = if choices.is_empty() { usize::from(single.is_some()) } else { choices.len() };
    let (face, per_row) = layout::choice_faces(available, count);
    let size = if face >= FaceSize::Large.width() { FaceSize::Large } else { FaceSize::Board(face as u16) };
    let width = if choices.is_empty() { POPUP_MIN_WIDTH.max(size.width() + 2.0 * POPUP_PADDING) } else {
        let row = per_row as f32 * size.width() + (per_row as f32 - 1.0) * layout::CHOICE_GAP;
        POPUP_MIN_WIDTH.max(row + 2.0 * POPUP_PADDING)
    };
    parent
        .spawn((
            DecisionPopup,
            GlobalZIndex(10),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|screen| {
            let accent = theme.accent;
            let mut panel = screen.spawn(widgets::panel(theme, px(width)));
            // As the actions menu does: its own slot over the one the
            // panel supplied, and the immediate recolour kept, because
            // `dress` lands a frame later.
            panel.insert(widgets::Dressed::still(Slot::PanelDecision, Drawn::new(theme.panel, accent)));
            panel.entry::<BorderColor>().and_modify(move |mut border| *border = BorderColor::all(accent));
            // **Capped at the window, and the cards are what give.** The
            // panel is centred, so a cap is all it takes for it to be on
            // the window; the shrinking is then shared out by the rule
            // `choice_faces` already states in words — every text node
            // and every button is rigid (`widgets::button` already sets
            // `flex_shrink: 0.0`), and the card row alone may lose
            // height. A card that is drawn short is still a card, and a
            // secondary click reads it full size; a button off the
            // bottom is an action the person cannot take.
            panel.entry::<Node>().and_modify(move |mut node| node.max_height = px(window.y - 2.0 * layout::PADDING));
            panel.with_children(|panel| {
                // `min_height: px(0)` with the clip is what lets it: a
                // flex item's automatic minimum is its content, so
                // without both the row would refuse to give and push the
                // buttons out again. A clip is not a scroll container —
                // nothing on the board is one.
                let gives = Node { width: percent(100), flex_shrink: 1.0, min_height: px(0), overflow: Overflow::clip(), ..default() };
                let rigid = Node { flex_shrink: 0.0, ..default() };
                if let Some(card) = &single {
                    panel.spawn(Node { justify_content: JustifyContent::Center, ..gives.clone() }).with_children(|row| {
                        spawn_choice_card(row, theme, core, images, Some(card), game.side.other(), size, ());
                    });
                }
                panel.spawn((widgets::heading(theme, title), TextLayout::new(Justify::Left, LineBreak::WordBoundary), rigid.clone()));
                if !detail.is_empty() {
                    panel.spawn((widgets::dim(theme, detail), TextLayout::new(Justify::Left, LineBreak::WordBoundary), rigid.clone()));
                }
                if let Some(rejection) = &game.rejection {
                    panel.spawn((widgets::notice(theme, format!("Rejected: {rejection}"), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary), rigid.clone()));
                }
                if let Some(selection) = selection.filter(|_| !choices.is_empty()) {
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            justify_content: JustifyContent::Center,
                            column_gap: px(layout::CHOICE_GAP),
                            row_gap: px(layout::CHOICE_GAP),
                            ..gives.clone()
                        })
                        .with_children(|row| {
                            for (position, index) in &choices {
                                let Some(candidate) = selection.candidate(*position) else { continue };
                                row.spawn(Node { width: px(size.width()), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|cell| {
                                    // The card is the button, as its label
                                    // under it is: the pop-up's own choice,
                                    // so a press submits it.
                                    let face = spawn_choice_card(cell, theme, core, images, candidate.card.as_ref(), game.side.other(), size, (Button, Click::Entry(*index)));
                                    if candidate.selected {
                                        cell.commands().entity(face).insert(Outline::new(px(3), px(2), theme.accent));
                                    } else {
                                        glow(&mut cell.commands(), face, theme, game.actions.affordance_of_entry(*index));
                                    }
                                    let copies = selection.copies(*position);
                                    if copies > 1 {
                                        cell.spawn(widgets::dim(theme, format!("{copies} copies")));
                                    }
                                    // A button the card's width in pixels,
                                    // not the cell's 100% — the reason is on
                                    // `entry_button`'s `width`, which this
                                    // case is why it takes.
                                    entry_button(cell, theme, game, *index, px(size.width()));
                                });
                            }
                        });
                }
                for index in &buttons {
                    // The pop-up's own buttons glow like the cards do, and
                    // for the same reason: a decision parked on the person
                    // is the clearest case of a moment that will pass.
                    if let Some(entity) = entry_button(panel, theme, game, *index, percent(100)) {
                        glow(&mut panel.commands(), entity, theme, game.actions.affordance_of_entry(*index));
                    }
                }
                // The pop-up's wash blocks the rail, so the way back out
                // of a prompt has to be in the prompt. Last, unlit, and
                // never one of the numbered decisions: it answers nothing.
                if let Some(label) = game.back_label() {
                    panel.spawn(widgets::button(theme, label, percent(100), Click::TakeBack));
                }
            });
        });
}

/// A card the pop-up shows: its face at `size`, marked as a
/// [`ChoiceCard`] so a secondary click reads it in the card sheet, or the
/// `concealed` side's back for a card the view does not name.
#[allow(clippy::too_many_arguments)]
fn spawn_choice_card(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, card: Option<&CardId>, concealed: Side, size: FaceSize, marker: impl Bundle) -> Entity {
    match card.and_then(|id| core.registry.get(id).map(|def| (id, def))) {
        Some((id, def)) => {
            let exact = def.numeric_id.and_then(|code| images.face(code, size));
            // Its own width not decoded yet: the sharpest copy the board
            // already has, stretched, and still asking for its own width,
            // which `poll_decoded` puts in place when it lands.
            let stand_in = if exact.is_none() { def.numeric_id.and_then(|code| images.nearest_face(code).map(|handle| (code, handle))) } else { None };
            let image = exact.or_else(|| stand_in.as_ref().map(|(_, handle)| handle.clone()));
            let entity = spawn_face(parent, theme, &Face::of(def), size, image, marker);
            // A face with no `Button` of its own still needs to know it is
            // hovered, for the secondary click.
            parent.commands().entity(entity).insert((ChoiceCard(id.clone()), Interaction::None));
            if let Some((code, _)) = stand_in {
                parent.commands().entity(entity).insert(WantsImage { code, size });
            }
            entity
        }
        None => spawn_back(parent, theme, images.back(concealed), concealed, size, marker),
    }
}

// ---- the phase bar ----

/// The turn's steps, and a run's, from `board::phase`, as a panel at the
/// head of the right column: a column per segment — its title, then its
/// steps top to bottom — the one in play in the accent colour, the ones
/// behind it dim, and the window's line under the lot.
///
/// **A panel of the right column, not a row of the board.** It was a bar
/// under the person's hand, which kept the hand off the window's bottom
/// edge and cost every card its height; the column beside the board had
/// room it was not using. Turned off, it is hidden rather than despawned,
/// and the cards do not move either way.
fn fill_phase_bar(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game) {
    use netrunner_client::board::phase::{self, State};
    let panel = Node {
        width: percent(100),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(10)),
        row_gap: px(6),
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(8)),
        ..default()
    };
    parent
        .spawn((panel, BackgroundColor(theme.panel), BorderColor::all(theme.panel_border), widgets::Dressed::still(Slot::PanelPhase, Drawn::new(theme.panel, theme.panel_border))))
        .with_children(|panel| {
            let Some(view) = &game.view else {
                panel.spawn(widgets::dim(theme, "Setting up…"));
                return;
            };
            let bar = phase::bar(view);
            panel.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexStart, column_gap: px(10), ..default() },)).with_children(|row| {
                for segment in &bar.segments {
                    row.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexStart, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(4), ..default() },)).with_children(|column| {
                        column.spawn((Text::new(segment.title.clone()), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        for step in &segment.steps {
                            let (text, background, border, slot) = match step.state {
                                State::Past => (theme.text_dim, theme.panel, theme.panel_border, Slot::PhaseChipPast),
                                State::Now => (theme.background, theme.accent, theme.accent, Slot::PhaseChipNow),
                                State::Ahead => (theme.text_dim, theme.background, theme.panel_border, Slot::PhaseChipAhead),
                            };
                            column.spawn((
                                PhaseStep(step.state),
                                Node {
                                    padding: UiRect::axes(px(8), px(3)),
                                    border: UiRect::all(px(1)),
                                    border_radius: BorderRadius::all(px(10)),
                                    max_width: percent(100),
                                    ..default()
                                },
                                BackgroundColor(background),
                                BorderColor::all(border),
                                widgets::Dressed::still(slot, Drawn::new(background, border)),
                                children![(Text::new(step.label.clone()), theme.font(size::SMALL), TextColor(text), TextLayout::new(Justify::Left, LineBreak::WordBoundary))],
                            ));
                        }
                    });
                }
            });
            // A line only when a window is open: the panel is not a row of
            // the board, so its height changing moves no card.
            if let Some(note) = &bar.note {
                panel.spawn((Text::new(note.clone()), theme.font(size::SMALL), TextColor(theme.accent), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
        });
}

/// The Runner's identity while a run is on, in the right column: the
/// person asked for the Runner to appear there "hacking into" the server,
/// so a run reads at a glance from across the room, not only from the
/// lane's chips.
///
/// **Its picture is the top of the scan** — the name banner and the art,
/// cut where the text box begins (`layout::IDENTITY_ART`) — as an
/// `ImageNode::rect` over the decoded copy's own pixel size, read from
/// `Assets<Image>`, so whichever resampled copy is cached crops the same.
/// With no scan
/// cached it is the identity's name alone, large, in the Runner's colour.
/// It follows the paced trail, not the view, so it appears on the run's
/// first beat and goes when the trail ends, never ahead of the lane. It
/// is paint: no button, no action, nothing the engine offered.
fn fill_run_identity(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, assets: Option<&Assets<Image>>, game: &Game) {
    let (Some(trail), Some(view)) = (&game.trail, &game.view) else { return };
    if trail.ended() {
        return;
    }
    let card = view.runner.identity.as_ref().and_then(|id| core.registry.get(id));
    let name = card.map_or_else(|| "The Runner".to_string(), |card| card.title.clone());
    let colour = theme.side(Side::Runner);
    let panel = Node {
        width: percent(100),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(10)),
        row_gap: px(8),
        border: UiRect::all(px(2)),
        border_radius: BorderRadius::all(px(8)),
        ..default()
    };
    parent
        .spawn((panel, BackgroundColor(theme.panel), BorderColor::all(colour), widgets::Dressed::still(Slot::PanelRun, Drawn::new(theme.panel, colour))))
        .with_children(|panel| {
            // Each line says its width in pixels. The right column sizes
            // this panel before the percentage has anything to resolve
            // against, so a line left to it was measured one word to a
            // line — and the panel kept that height, a hundred pixels of
            // nothing under a one-line name.
            let line = || Node { width: px(RUN_ART_WIDTH), ..default() };
            panel.spawn((Text::new(format!("Hacking into {}", server_name(trail.server))), theme.font(size::SMALL), TextColor(colour), line()));
            // The top of the scan at the panel's inner width: the panel's
            // own copy once it has decoded, the sharpest other until then.
            // The rect is a fraction of whichever copy's own size.
            let width = RUN_ART_WIDTH;
            let scan = card.and_then(|card| card.numeric_id).and_then(|code| images.face(code, FaceSize::Board(width as u16)).or_else(|| images.nearest_face(code)));
            let size = scan.as_ref().and_then(|scan| assets?.get(scan)).map(|image| image.size_f32());
            match scan.zip(size) {
                Some((scan, size)) => {
                    let rect = Rect::new(0.0, 0.0, size.x, size.y * layout::IDENTITY_ART);
                    let height = (width * rect.height() / rect.width()).round();
                    panel.spawn((
                        RunIdentityArt(scan.clone()),
                        ImageNode { image_mode: NodeImageMode::Stretch, rect: Some(rect), ..ImageNode::new(scan) },
                        Node { width: px(width), height: px(height), flex_shrink: 0.0, border_radius: BorderRadius::all(px(6)), ..default() },
                    ));
                    // The name is on the banner; this one is for a test and
                    // a screen reader, so it is drawn small.
                    panel.spawn((RunIdentityName, Text::new(name), theme.font(size::SMALL), TextColor(theme.text_dim), line()));
                }
                None => {
                    panel.spawn((RunIdentityName, Text::new(name), theme.font(size::HEADING), TextColor(colour), TextLayout::new(Justify::Left, LineBreak::WordBoundary), line()));
                }
            }
        });
}

/// The run panel's picture width: the right column less the panel's
/// padding and border.
const RUN_ART_WIDTH: f32 = layout::RAIL_WIDTH - 2.0 * 12.0;

/// Refills the right column's three parts that follow the match rather
/// than the prompt: the status line, the phase panel and the run panel.
/// Runs before `relane` and `redraw`, and reads their flags rather than
/// taking them, so a new view and a beat of the trail both reach it;
/// `side` is its own, for a setting.
#[allow(clippy::too_many_arguments)]
fn side_panels(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    model: Option<Res<Model>>,
    mut status: Query<&mut Text, With<StatusLine>>,
    mut phase: Query<(Entity, &mut Node), (With<PhaseBarRow>, Without<RunIdentity>)>,
    run: Query<Entity, With<RunIdentity>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    mut images: ResMut<CardImages>,
    assets: Option<Res<Assets<Image>>>,
    drawn: Query<&RunIdentityArt>,
) {
    // While a run is on, the panel wants the Runner's scan at its own
    // width: ask for it once, and redraw when it lands, since the strip's
    // identity copy that stands in for it is a fifth as wide.
    let running = model.as_ref().and_then(|model| {
        let game = &model.0;
        game.trail.as_ref().filter(|trail| !trail.ended())?;
        let id = game.view.as_ref()?.runner.identity.as_ref()?;
        core.registry.get(id)?.numeric_id
    });
    if let Some(code) = running {
        let size = FaceSize::Board(RUN_ART_WIDTH as u16);
        match images.face(code, size) {
            Some(sharp) => {
                if drawn.iter().any(|art| art.0 != sharp) {
                    dirty.side = true;
                }
            }
            None => {
                if let netrunner_card_sync::ImageStatus::Cached(path) = core.images.status(code) {
                    images.request(code, size, path);
                }
            }
        }
    }
    if !(dirty.board || dirty.lane || dirty.side) {
        return;
    }
    dirty.side = false;
    let Some(model) = model else { return };
    let game = &model.0;
    for mut text in &mut status {
        text.0 = status_line(game);
    }
    let shown = core.settings.desktop.phase_bar;
    for (entity, mut node) in &mut phase {
        node.display = if shown { Display::Flex } else { Display::None };
        let mut panel = commands.entity(entity);
        panel.despawn_children();
        if shown {
            panel.with_children(|parent| fill_phase_bar(parent, &theme, game));
        }
    }
    for entity in &run {
        commands.entity(entity).despawn_children().with_children(|parent| fill_run_identity(parent, &theme, &core, &images, assets.as_deref(), game));
    }
}

// ---- the run lane ----

fn run_lane_node() -> Node {
    Node { width: percent(100), height: px(layout::RUN_LANE - layout::ROW_GAP), flex_shrink: 0.0, flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: px(4), overflow: Overflow::clip(), ..default() }
}

/// The trail as a row of chips, left to right in the order the Runner
/// meets them — the server, each ice, the server's approach, the access,
/// the outcome — and one line beneath of what the run did. Empty with no
/// trail; the lane keeps its height either way.
fn fill_run_lane(lane: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, game: &Game) {
    let Some(trail) = &game.trail else { return };
    let title = |card: &Option<CardId>| card.as_ref().and_then(|id| core.registry.get(id)).map(|def| def.title.clone());
    lane.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), flex_shrink: 0.0, ..default() }).with_children(|row| {
        let lit = |on: bool| if on { ChipStyle::Current } else { ChipStyle::Done };
        chip(row, theme, trail.heading(), ChipStyle::Origin, Pickable::IGNORE);
        for step in &trail.ice {
            arrow(row, theme);
            let name = title(&step.card).unwrap_or_else(|| "ICE".to_string());
            let (label, style) = match step.state {
                IceState::Upcoming => (name, ChipStyle::Upcoming),
                IceState::Approaching => (format!("{name} · approach"), ChipStyle::Current),
                IceState::Encountering => (format!("{name} · encounter"), ChipStyle::Current),
                IceState::Passed => (format!("{name} · passed"), ChipStyle::Done),
                IceState::Bypassed => (format!("{name} · bypassed"), ChipStyle::Done),
            };
            let entity = chip(row, theme, label, style, (Button, Click::Target(Target::Install(step.install))));
            if !step.subs.is_empty() {
                row.commands().entity(entity).with_children(|chip| {
                    chip.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(3), margin: UiRect::left(px(6)), ..default() }).with_children(|dots| {
                        for status in &step.subs {
                            let (fill, edge) = match status {
                                SubroutineStatus::Pending => (Color::NONE, theme.text_dim),
                                SubroutineStatus::Broken => (theme.accent, theme.accent),
                                SubroutineStatus::Resolved => (theme.danger, theme.danger),
                            };
                            dots.spawn((Node { width: px(8), height: px(8), border: UiRect::all(px(1)), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(fill), BorderColor::all(edge)));
                        }
                    });
                });
            }
        }
        arrow(row, theme);
        let at_server = matches!(trail.stage, Stage::AtServer | Stage::Accessing { .. });
        chip(row, theme, server_name(trail.server), if at_server { lit(!trail.ended()) } else { ChipStyle::Upcoming }, Pickable::IGNORE);
        if let Stage::Accessing { count, card } = &trail.stage {
            arrow(row, theme);
            // The count is what the mask allows: none for the Corp
            // watching a breach of HQ or R&D.
            let label = match (title(card), count) {
                (Some(name), _) => format!("Accessing {name}"),
                (None, 0) => "Accessing".to_string(),
                (None, n) => format!("Accessing · {n}"),
            };
            chip(row, theme, label, lit(!trail.ended()), Pickable::IGNORE);
        }
        if let Some(outcome) = trail.outcome {
            arrow(row, theme);
            let (label, style) = match outcome {
                RunOutcome::Successful => ("Successful", ChipStyle::Success),
                RunOutcome::JackedOut => ("Jacked out", ChipStyle::Done),
                RunOutcome::Ended => ("Run ends", ChipStyle::Ended),
            };
            chip(row, theme, label.to_string(), style, Pickable::IGNORE);
        }
    });
    if !trail.consequences.is_empty() {
        // The last few, newest last: one line, clipped by the lane.
        let recent: Vec<&str> = trail.consequences.iter().rev().take(4).rev().map(String::as_str).collect();
        lane.spawn((widgets::dim(theme, recent.join("  ·  ")), Node { flex_shrink: 0.0, overflow: Overflow::clip(), ..default() }));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChipStyle {
    /// The server chip: the Runner's colour, the colour of the column
    /// under run.
    Origin,
    Upcoming,
    /// Where the run is now.
    Current,
    /// Behind the run.
    Done,
    Success,
    Ended,
}

fn chip(row: &mut ChildSpawnerCommands, theme: &Theme, label: String, style: ChipStyle, marker: impl Bundle) -> Entity {
    let (border, background, text, slot) = match style {
        ChipStyle::Origin => (theme.runner, theme.runner.with_alpha(0.2), theme.text, Slot::RunChipOrigin),
        ChipStyle::Upcoming => (theme.panel_border, theme.panel, theme.text_dim, Slot::RunChipUpcoming),
        ChipStyle::Current => (theme.accent, theme.accent.with_alpha(0.25), theme.text, Slot::RunChipCurrent),
        ChipStyle::Done => (theme.panel_border, theme.button, theme.text_dim, Slot::RunChipDone),
        ChipStyle::Success => (theme.runner, theme.runner.with_alpha(0.25), theme.text, Slot::RunChipSuccess),
        ChipStyle::Ended => (theme.corp, theme.corp.with_alpha(0.25), theme.text, Slot::RunChipEnded),
    };
    row.spawn((
        marker,
        Node {
            flex_shrink: 0.0,
            height: px(28),
            padding: UiRect::axes(px(10), px(0)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(background),
        BorderColor::all(border),
        widgets::Dressed::still(slot, Drawn::new(background, border)),
        children![(Text::new(label), theme.font(size::SMALL - 2.0), TextColor(text))],
    ))
    .id()
}

fn arrow(row: &mut ChildSpawnerCommands, theme: &Theme) {
    row.spawn((Text::new("\u{203a}"), theme.font(size::SMALL), TextColor(theme.text_dim)));
}

/// A beat of the trail with the board still: refills the lane alone.
/// Runs before `redraw`, which covers the lane whenever it respawns
/// the board.
fn relane(mut commands: Commands, mut dirty: ResMut<Dirty>, model: Option<Res<Model>>, lane: Query<Entity, With<RunLane>>, theme: Res<Theme>, core: Res<ClientCore>) {
    if !dirty.lane || dirty.board {
        return;
    }
    dirty.lane = false;
    let Some(model) = model else { return };
    if let Ok(lane) = lane.single() {
        commands.entity(lane).despawn_children().with_children(|parent| fill_run_lane(parent, &theme, &core, &model.0));
    }
}

// ---- the overlays ----

fn spawn_overlay(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game) {
    // A card alone is its face and the panel's padding; an install's
    // state sits beside the face; a zone's contents are the widest.
    let card_alone = game.inspecting.is_some() || game.sheet.as_ref().is_some_and(|s| !matches!(s.target, Target::Install(_)) && game.card_of(&s.target).is_some());
    let width = if game.finished() || game.confirm_quit || game.options_open || game.help_open {
        px(560)
    } else if card_alone {
        px(FaceSize::Large.width() + 2.0 * 17.0)
    } else if game.sheet.as_ref().is_some_and(|s| matches!(s.target, Target::Install(_))) {
        px(800)
    } else {
        px(960)
    };
    // A reading surface closes on a click that misses it, so the wash is
    // a button carrying the same `Click::CloseOverlay` the Close button
    // used to — no new system and no new intent, and because it resolves
    // to `Intent::Back` it pops one level, taking an inspected card back
    // to the zone sheet under it exactly as Escape does. A form and the
    // three panels that are asking a question stay modal
    // (`Game::dismissed_by_a_click_away`), so the wash is inert there.
    //
    // **`FocusPolicy::Block` is in the spawn bundle, unconditionally, and
    // both halves of that matter.** `Node` *requires* `FocusPolicy`, whose
    // default is `Pass` — so a wash that did not say `Block` would let
    // `ui_focus_system` walk straight past it into the board behind, and
    // the board's tiles and control-bar buttons are `Themed` buttons that
    // would take the press. A click that missed the panel could close the
    // sheet and end the turn in the same frame. It is unconditional
    // because a *form*'s wash has to hold a press too: it declines to act
    // on one, which is not the same as letting it through. Inheriting the
    // `Block` from `Button` would not have worked either — a required
    // component is inserted with `Keep`, and the spawn above has already
    // given the entity `Node`'s `Pass`, so `Button`'s `Block` is silently
    // skipped on a post-spawn `insert`. Both facts are the opposite of
    // what they look like, which is why they are written down rather
    // than left to a requirement to express.
    //
    // The `Dressed` is not decoration either. `button_feedback` resets a
    // `Themed` node's background to `theme.button` on every interaction
    // change unless it carries one — the limitation recorded in place on
    // that system — so an undressed scrim would turn button-grey after
    // the first hover. `Dressed::still` hands back the same `Drawn` for
    // all three interactions, so the wash never moves under the pointer.
    let wash = theme.background.with_alpha(0.75);
    let mut overlay = parent.spawn((
        Overlay,
        FocusPolicy::Block,
        GlobalZIndex(20),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(wash),
    ));
    if game.dismissed_by_a_click_away() {
        overlay.insert((Button, Interaction::None, widgets::Themed, Click::CloseOverlay, widgets::Dressed::still(Slot::OverlayScrim, Drawn::new(wash, Color::NONE))));
    }
    overlay
        .with_children(|screen| {
            // And the panel blocks for the same reason, so a press on its
            // padding, its row gaps or the empty column beside an
            // install's face is not a press on the wash behind it. No
            // `Interaction` with it: `spawn_actions_menu` carries one
            // because `board_click` reads the menu's, and nothing reads
            // this panel's.
            let mut sheet = screen.spawn((FocusPolicy::Block, widgets::panel(theme, width)));
            // Its own slot over the one `widgets::panel` supplied, so a
            // skin can dress the game's surfaces without also repainting
            // the main menu and the settings, which `Slot::Panel` reaches
            // too. Inserted rather than bundled: two `Dressed` in one
            // bundle is a duplicate-component panic.
            sheet.insert(widgets::Dressed::still(Slot::PanelSheet, Drawn::new(theme.panel, theme.panel_border)));
            sheet.with_children(|panel| {
                if let Some(reason) = &game.stalled {
                    panel.spawn(widgets::heading(theme, "The match stopped"));
                    panel.spawn((widgets::dim(theme, reason.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    if let Some(line) = &game.saved_report {
                        panel.spawn((widgets::dim(theme, line.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    }
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        if game.saved_report_path.is_some() {
                            row.spawn(widgets::button(theme, "Watch it", Val::Auto, Click::WatchReport));
                        }
                        row.spawn(widgets::button(theme, "Menu", Val::Auto, Click::Menu));
                    });
                } else if let Some(over) = &game.over {
                    let won = over.winner == game.side;
                    panel.spawn((Text::new(if won { "You win" } else { "You lose" }), theme.font(size::HEADING), TextColor(if won { theme.accent } else { theme.danger })));
                    panel.spawn(widgets::dim(theme, format!("{:?} wins: {}", over.winner, end_reason(over.reason))));
                    if let Some(report) = &over.report {
                        for line in report.lines() {
                            panel.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                    }
                    if let Some(notice) = &over.notice {
                        panel.spawn(widgets::notice(theme, notice.clone(), ()));
                    }
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::button(theme, "Play again", Val::Auto, Click::PlayAgain));
                        row.spawn(widgets::button(theme, "Menu", Val::Auto, Click::Menu));
                    });
                } else if game.confirm_quit {
                    panel.spawn(widgets::heading(theme, "Leave the game?"));
                    let turn = game.view.as_ref().map_or(0, |v| v.turn);
                    let consequence = if turn >= netrunner_client::record::FORFEIT_FROM_TURN {
                        "From turn 3 on, a quit counts as a loss toward your suggested rung."
                    } else {
                        "Nothing is recorded this early in the game."
                    };
                    panel.spawn((widgets::dim(theme, consequence), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::button(theme, "Quit", Val::Auto, Click::ConfirmQuit));
                        row.spawn(widgets::button(theme, "Keep playing", Val::Auto, Click::CancelQuit));
                    });
                } else if game.options_open {
                    panel.spawn(widgets::heading(theme, "Game options"));
                    settings_screen::spawn_rows(panel, theme, core, &Row::GAME);
                    if let Some(line) = &game.saved_report {
                        panel.spawn((widgets::dim(theme, line.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    }
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        // A replay is already a saved game.
                        if game.replay.is_none() {
                            row.spawn(widgets::button(theme, "Save a bug report", Val::Auto, Click::SaveReport));
                        }
                        if game.saved_report_path.is_some() {
                            row.spawn(widgets::button(theme, "Watch it", Val::Auto, Click::WatchReport));
                        }
                        row.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
                    });
                } else if game.help_open {
                    help_sheet(panel, theme);
                } else if let Some(id) = &game.inspecting {
                    // A card read out of a pile: the face and its text,
                    // nothing to do with it from here.
                    card_sheet(panel, theme, core, images, id);
                } else if let Some(sheet) = &game.sheet {
                    match (&sheet.target, game.card_of(&sheet.target)) {
                        (Target::Install(id), card) => install_sheet(panel, theme, core, images, game, *id, card.as_ref()),
                        (_, Some(id)) => card_sheet(panel, theme, core, images, &id),
                        (_, None) => zone_sheet(panel, theme, core, images, game, &sheet.target),
                    }
                }
            });
        });
}

/// The list of keys, a row each: the key in a fixed column, what it does
/// beside it. From `shortcuts::LIST`, so the list and the keys cannot
/// disagree about which letters there are.
fn help_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme) {
    panel.spawn(widgets::heading(theme, "Keyboard shortcuts"));
    for (key, meaning) in shortcuts::LIST {
        panel.spawn((HelpRow, Node { flex_direction: FlexDirection::Row, column_gap: px(16), ..default() })).with_children(|row| {
            // A key never wraps, and its column is wide enough that it need
            // not: a wrapped measure left a blank line under "? or F1".
            row.spawn((Text::new(*key), theme.font(size::BODY), TextColor(theme.accent), TextLayout::new(Justify::Left, LineBreak::NoWrap), Node { width: px(120), flex_shrink: 0.0, ..default() }));
            row.spawn((Text::new(*meaning), theme.font(size::BODY), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { flex_grow: 1.0, min_width: px(0), ..default() }));
        });
    }
    panel.spawn(widgets::dim(theme, "A key does what its button does, only when the button would."));
    panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
}

/// The card large, and nothing else at all: the picture, or the text
/// layout (which carries the printed text) while no picture is cached.
/// What may be done with it is the menu's — the sheet once listed the
/// actions under the card's text, and the person asked for reading and
/// acting to be two clicks rather than one panel.
///
/// **No heading over it and no Close under it.** The card says its own
/// name in both tiers — a cached scan *is* the printed card, and the
/// text face draws the title in its own title row — so a heading was the
/// name twice on the fallback face and once too often on the other. The
/// button was a third door to a rule Escape already had; a click that
/// misses the panel is the second. Between them that is about eighty-six
/// logical pixels of chrome off a panel whose whole content is a
/// 380-wide face. The registry-miss arm keeps its line because there is
/// no face there to say anything.
fn card_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, id: &CardId) {
    let Some(def) = core.registry.get(id) else {
        panel.spawn(widgets::dim(theme, format!("{} is not in the registry", id.0)));
        return;
    };
    let image = def.numeric_id.and_then(|code| images.face(code, FaceSize::Large));
    spawn_face(panel, theme, &Face::of(def), FaceSize::Large, image, ());
}

/// An installed card: the face (or the back, for a card the viewer
/// cannot name) beside its state — `board::facts::install_facts`: where
/// it sits and in what order it is met, rezzed or not and the cost of a
/// rez, strength now and printed, each subroutine with its status in an
/// encounter, tokens, counters, trash cost, what it hosts. The state is
/// the one thing beside the card, because the printed face cannot show
/// it and a tile has no room; the printed text is on the face.
///
/// **The heading survives here for exactly one case**, which is why it
/// moved inside the match rather than going the way `card_sheet`'s did:
/// a card the viewer cannot name is drawn as a card *back*, and a back
/// says nothing, so `facts::hidden_title` is the only thing naming it.
/// With a card in hand the face prints its own title and a heading would
/// be the second copy.
fn install_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, id: InstallId, card: Option<&CardId>) {
    let Some(view) = &game.view else { return };
    let def = card.and_then(|c| core.registry.get(c));
    if def.is_none() {
        panel.spawn(widgets::heading(theme, facts::hidden_title(view, id)));
    }
    panel.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(16), align_items: AlignItems::FlexStart, ..default() },)).with_children(|row| {
        match def {
            Some(def) => {
                let image = def.numeric_id.and_then(|code| images.face(code, FaceSize::Large));
                spawn_face(row, theme, &Face::of(def), FaceSize::Large, image, ());
            }
            None => {
                spawn_back(row, theme, images.back(Side::Corp), Side::Corp, FaceSize::Large, ());
            }
        }
        row.spawn((Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },)).with_children(|column| {
            column.spawn(widgets::label(theme, "State"));
            for line in facts::install_facts(view, id, &core.registry).unwrap_or_default() {
                column.spawn((InstallFact, Text::new(line), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
        });
    });
}

/// A zone: what is in it as far as the viewer may see — its actions are
/// the menu's. Every visible card is a button that reads it over the
/// sheet.
/// The score area is the exception, a list rather than a spread of
/// faces (`score_area_sheet`).
fn zone_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, target: &Target) {
    let Some(view) = &game.view else { return };
    panel.spawn(widgets::heading(theme, target_title(game, target)));
    if let Target::Pile(Pile::Agendas(side)) = target {
        score_area_sheet(panel, theme, core, images, game, view, *side);
        return;
    }
    // What the zone holds, for the viewer.
    enum Shown {
        Card(CardId),
        Back(Side),
    }
    let (caption, shown): (String, Vec<Shown>) = match target {
        Target::Server(ServerId::Archives) => {
            let cards: Vec<Shown> = view.corp.archives.iter().map(|c| c.card.clone().map_or(Shown::Back(Side::Corp), Shown::Card)).collect();
            let facedown = view.corp.archives.iter().filter(|c| c.facedown).count();
            let caption = match (game.side, facedown) {
                (_, 0) => format!("{} cards", cards.len()),
                (Side::Corp, n) => format!("{} cards, {n} face down — the Runner has not seen those", cards.len()),
                (Side::Runner, n) => format!("{} cards, {n} face down", cards.len()),
            };
            (caption, cards)
        }
        Target::Server(ServerId::RnD) => (format!("{} cards, in an order nobody is shown", view.corp.rd_count), (0..view.corp.rd_count.min(5)).map(|_| Shown::Back(Side::Corp)).collect()),
        Target::Server(ServerId::Hq) => match &view.corp.hq_cards {
            Some(hand) => (format!("{} cards in hand", hand.len()), hand.iter().cloned().map(Shown::Card).collect()),
            None => (format!("{} cards, hidden", view.corp.hq_count), (0..view.corp.hq_count.min(8)).map(|_| Shown::Back(Side::Corp)).collect()),
        },
        Target::Server(server @ ServerId::Remote(_)) => {
            let cards: Vec<Shown> = view
                .corp
                .servers
                .iter()
                .filter(|s| s.server == *server)
                .flat_map(|s| s.ice.iter().chain(s.root.iter()))
                .map(|c| c.card.clone().map_or(Shown::Back(Side::Corp), Shown::Card))
                .collect();
            (if cards.is_empty() { "Nothing installed".to_string() } else { format!("{} cards, ICE first", cards.len()) }, cards)
        }
        Target::Pile(Pile::Stack) => (format!("{} cards, in an order nobody is shown", view.runner.stack_count), (0..view.runner.stack_count.min(5)).map(|_| Shown::Back(Side::Runner)).collect()),
        Target::Pile(Pile::Heap) => (format!("{} cards, all face up", view.runner.heap.len()), view.runner.heap.iter().cloned().map(Shown::Card).collect()),
        Target::Pile(Pile::Agendas(_)) => unreachable!("the score area returned above"),
        Target::HandCard(_) | Target::Install(_) | Target::Identity(_) | Target::Position(_) => (String::new(), Vec::new()),
    };
    panel.spawn(widgets::dim(theme, caption));
    if !shown.is_empty() {
        // A wrapping row inside a column that scrolls: a pile of forty
        // is five rows of faces, and the wheel reaches them all.
        let scroll = panel
            .spawn((
                bevy::ui_widgets::ScrollArea,
                Node { width: percent(100), max_height: px(460), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() },
            ))
            .with_children(|column| {
                column.spawn(wrap_row()).with_children(|row| {
                    for item in shown {
                        match item {
                            Shown::Card(id) => {
                                if let Some(def) = core.registry.get(&id) {
                                    let image = def.numeric_id.and_then(|code| images.face(code, FaceSize::Thumb));
                                    spawn_face(row, theme, &Face::of(def), FaceSize::Thumb, image, (Button, Click::Inspect(id.clone())));
                                }
                            }
                            Shown::Back(side) => {
                                spawn_back(row, theme, images.back(side), side, FaceSize::Thumb, ());
                            }
                        }
                    }
                });
            })
            .id();
        panel.spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(scroll).with_children(|row| {
            row.spawn(widgets::scrollbar(theme, scroll));
        });
    }
}

/// A side's score area as a list: a row per agenda — its face small,
/// its title and points — and a press on a row opens it in place, the
/// face large beside what it is worth, its counters, its printed text
/// and, for an agenda the viewer scored, the abilities it still has
/// (`Target::Install` by the handle it kept). A list rather than the
/// piles' spread of faces because the person asked to read down it and
/// open one, and a card read over the sheet hides the others.
#[allow(clippy::too_many_arguments)]
fn score_area_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, side: Side) {
    let agendas = hud::score_area(view, side, &core.registry);
    let (points, to_win) = (if side == Side::Corp { view.corp.agenda_points } else { view.runner.agenda_points }, view.rules.winning_agenda_points);
    let caption = match agendas.len() {
        0 => format!("None yet · {points} of {to_win} points"),
        1 => format!("1 agenda · {points} of {to_win} points"),
        n => format!("{n} agendas · {points} of {to_win} points"),
    };
    panel.spawn(widgets::dim(theme, caption));
    if agendas.is_empty() {
        return;
    }
    let scroll = panel
        .spawn((
            bevy::ui_widgets::ScrollArea,
            Node { width: percent(100), max_height: px(560), flex_direction: FlexDirection::Column, row_gap: px(6), overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|list| {
            for (row, agenda) in agendas.iter().enumerate() {
                let open = game.expanded == Some(row);
                let def = core.registry.get(&agenda.card);
                let code = def.and_then(|d| d.numeric_id);
                list.spawn((
                    ScoreRow(row),
                    Button,
                    widgets::Themed,
                    Click::Expand(row),
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), padding: UiRect::all(px(6)), border_radius: BorderRadius::all(px(6)), flex_shrink: 0.0, ..default() },
                    BackgroundColor(theme.button),
                ))
                .with_children(|button| {
                    // The disclosure triangles are in Noto Sans Symbols 2 (U+25B8,
                    // U+25BE), not Noto Sans, which drew both as a box — and
                    // has no U+2212 minus either. Without the symbol font, a
                    // plus and an en dash.
                    let (marker, font) = if theme.has_symbols() {
                        (if open { "▾" } else { "▸" }, theme.symbol_font(size::BODY))
                    } else {
                        (if open { "–" } else { "+" }, theme.font(size::BODY))
                    };
                    button.spawn((Text::new(marker), font, TextColor(theme.text_dim), Node { width: px(18), ..default() }));
                    if let Some(def) = def {
                        spawn_face(button, theme, &Face::of(def), FaceSize::Board(56), code.and_then(|code| images.face(code, FaceSize::Board(56))), ());
                    }
                    button.spawn(widgets::label(theme, agenda.line()));
                });
                if !open {
                    continue;
                }
                list.spawn((ScoreDetails(row), Node { flex_direction: FlexDirection::Row, column_gap: px(16), align_items: AlignItems::FlexStart, padding: UiRect::left(px(28)), flex_shrink: 0.0, ..default() })).with_children(|details| {
                    if let Some(def) = def {
                        spawn_face(details, theme, &Face::of(def), FaceSize::Large, code.and_then(|code| images.face(code, FaceSize::Large)), ());
                    }
                    details.spawn((Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },)).with_children(|column| {
                        for line in agenda.facts(side, &core.registry) {
                            column.spawn((Text::new(line), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                        if let Some(def) = def {
                            column.spawn((Text::new(Face::of(def).body_text(false)), theme.font(size::SMALL), TextColor(theme.text_dim), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                        let entries = agenda.install.map(|id| game.entries_for(&Target::Install(id))).unwrap_or_default();
                        if !entries.is_empty() {
                            column.spawn(widgets::label(theme, "Actions"));
                            for index in entries {
                                entry_button(column, theme, game, index, percent(100));
                            }
                        }
                    });
                });
            }
        })
        .id();
    panel.spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(scroll).with_children(|row| {
        row.spawn(widgets::scrollbar(theme, scroll));
    });
}

fn target_title(game: &Game, target: &Target) -> String {
    let title = |id: &CardId| game.registry().get(id).map_or_else(|| id.0.clone(), |c| c.title.clone());
    match target {
        Target::HandCard(card) => title(card),
        Target::Install(id) => game.card_at(*id).map_or_else(|| "This card".to_string(), |card| title(&card)),
        Target::Server(server) => server_name(*server),
        Target::Identity(side) => format!("{side:?} identity"),
        // The card the prompt means by that position, as the selection
        // names it; a position is never shown as a number.
        Target::Position(position) => game
            .view
            .as_ref()
            .and_then(|view| netrunner_client::selection::Selection::of(view, game.registry()))
            .and_then(|selection| selection.candidate(*position).map(|candidate| selection.display(candidate)))
            .unwrap_or_else(|| "A card".to_string()),
        Target::Pile(pile) => pile.name().to_string(),
    }
}

fn end_reason(reason: netrunner_client::play::GameEndReason) -> &'static str {
    use netrunner_client::play::GameEndReason;
    match reason {
        GameEndReason::AgendaThreshold => "enough agenda points",
        GameEndReason::Flatline => "the Runner was flatlined",
        GameEndReason::Deckout => "the Corp ran out of cards",
        GameEndReason::Surrender => "the other side surrendered",
        GameEndReason::Disconnected => "the other side disconnected",
        GameEndReason::TimedOut => "the other side ran out of time",
    }
}

/// The slot an install occupies, for a test that reads the board.
pub fn install_slot(view: &ClientView, id: InstallId) -> Option<InstallSlot> {
    view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).find(|c| c.install_id == id).map(|c| c.slot)
}
