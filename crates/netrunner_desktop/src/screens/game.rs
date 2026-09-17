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
//! the card width the window has room for from the window's size and
//! what is on the board (the tallest server's ICE, the number of
//! servers), `fit` recomputes it whenever either changes, and a row that
//! is still too wide overlaps its cards like a held hand
//! (`layout::step`) rather than wrapping or scrolling. Top to bottom:
//! the top bar (the status line, Quit, the gear); the board beside the
//! rail — the opponent's strip and hand as backs in one row, their
//! area, the person's area, the person's strip and hand in one row; and
//! the control bar (`board::Control::for_side`, one button each, always
//! in the same place) centred along the bottom, next to the hand, so
//! acting never means crossing the opponent's side. The Corp's servers
//! are columns — ICE as bars above the root, the way ICE lies on a
//! table, since a rotated `UiTransform` is laid out as its unrotated
//! box and would overlap its neighbours — and the Runner's rig is one
//! row of three groups, with the stack and the heap as buttons in the
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

use netrunner_client::board::action_map::server_name;
use netrunner_client::board::{facts, hud, Control, IceState, Outcome as RunOutcome, Pile, Stage, Target, Transition, Zone};
use netrunner_client::card_face::Face;
use netrunner_core::dsl::{CardId, CardType};
use netrunner_core::rules::{GamePhase, InstallId, InstallSlot, PendingDecision, RunPhase, ServerId, Side, SubroutineStatus};
use netrunner_core::view::{ClientView, ServerView};

use crate::card_images::CardImages;
use crate::core::{ClientCore, Notices};
use crate::models::game::{Anchor, Game, Intent, MatchMessageRef, Outcome};
use crate::models::layout::{self, Counts};
use crate::models::pace::{Beat, Pacer};
use crate::models::settings::{self as settings_model, Row};
use crate::models::shortcuts::{self, Shortcut};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::screens::new_game::{ActiveMatch, LastGame};
use crate::screens::settings::{self as settings_screen, Control as SettingsControl};
use crate::screens::AppScreen;
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
            .init_resource::<Pointer>()
            .add_systems(OnEnter(AppScreen::Game), spawn)
            .add_systems(OnExit(AppScreen::Game), leave)
            .add_systems(Update, (poll, autoplay, escape.in_set(Captures), board_click, drag_hand, shortcuts, controls, fit, relane, redraw).chain().run_if(in_state(AppScreen::Game)));
    }
}

/// What a button on the board means.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum Click {
    Target(Target),
    /// An entry of the action map.
    Entry(usize),
    /// A control-bar button.
    Control(Control),
    /// A face in a zone sheet: read the card over the sheet.
    Inspect(CardId),
    /// A row of a list sheet (the score area): open or close its details.
    Expand(usize),
    /// The gear.
    Options,
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
/// Where the pointer was last seen, in logical window pixels: read off
/// `CursorMoved`, because the window's own `cursor_position` needs a
/// window and the headless tests have none.
#[derive(Resource, Default)]
pub struct Pointer(pub (f32, f32));

/// A card's place in the person's own hand, for a drag to read.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandSlot(pub usize);

/// The phase bar's row, and one step chip on it, for a test to read.
#[derive(Component)]
pub struct PhaseBarRow;

#[derive(Component)]
pub struct PhaseStep(pub netrunner_client::board::phase::State);

/// The rail's line asking for a second Enter, for a test to find.
#[derive(Component)]
pub struct EndTurnNotice;

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
struct StatusLine;

#[derive(Resource)]
pub struct Model(pub Game);

#[derive(Resource, Default)]
struct Dirty {
    board: bool,
    rail: bool,
    log: bool,
    overlay: bool,
    /// The run lane alone: a beat of the trail, with the board still.
    lane: bool,
}

impl Dirty {
    fn all(&mut self) {
        self.board = true;
        self.rail = true;
        self.log = true;
        self.overlay = true;
        self.lane = true;
    }
}

/// Intents raised by a key, applied by `controls` with the pressed ones
/// so every outcome is handled in one place.
#[derive(Resource, Default)]
struct Pending(Vec<Intent>);

/// The card width the board is drawn at, and the window it was computed
/// for. `fit` keeps it current; a change redraws the board.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct BoardFit {
    pub face: f32,
    pub window: Vec2,
}

impl Default for BoardFit {
    fn default() -> Self {
        // The headless tests' window: no `Window` exists there.
        Self { face: 0.0, window: Vec2::new(1280.0, 800.0) }
    }
}

impl BoardFit {
    fn size(&self) -> FaceSize {
        FaceSize::Board(self.face.round() as u16)
    }

    fn identity_size(&self) -> FaceSize {
        FaceSize::Board((self.face * layout::IDENTITY_SCALE).round() as u16)
    }

    fn board_width(&self) -> f32 {
        layout::board_width(self.window.x)
    }
}

/// What the view puts on the board, for the fit.
fn counts(game: &Game, phase_bar: bool) -> Counts {
    let human_is_runner = game.side == Side::Runner;
    let Some(view) = &game.view else { return Counts { human_is_runner, phase_bar, ..Counts::default() } };
    let pieces = view.corp.servers.iter().map(|s| s.ice.len() + s.root.len()).max().unwrap_or(0);
    // The three centrals are always drawn.
    let remotes = view.corp.servers.iter().filter(|s| matches!(s.server, ServerId::Remote(_))).count();
    Counts { pieces, servers: 3 + remotes, human_is_runner, rig: !view.runner.rig.is_empty(), phase_bar }
}

/// Recomputes the face width from the window and the view, and marks the
/// board for a redraw when it moved. Runs every frame and is cheap: a
/// handful of comparisons.
fn fit(windows: Query<&Window, With<PrimaryWindow>>, model: Option<Res<Model>>, fit: Option<ResMut<BoardFit>>, core: Res<ClientCore>, mut dirty: ResMut<Dirty>) {
    let (Some(model), Some(mut fit)) = (model, fit) else { return };
    let window = windows.single().map_or(fit.window, |w| Vec2::new(w.width(), w.height()));
    let face = layout::face_width((window.x, window.y), counts(&model.0, core.settings.desktop.phase_bar));
    if (face - fit.face).abs() > 0.5 || window != fit.window {
        fit.face = face;
        fit.window = window;
        dirty.board = true;
    }
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, active: Option<Res<ActiveMatch>>, mut images: Option<ResMut<Assets<Image>>>, dev: Option<Res<crate::dev::Dev>>) {
    commands.init_resource::<Dirty>();
    let Some(active) = active else {
        commands.spawn((screen_root(AppScreen::Game, theme.background), children![
            widgets::heading(&theme, AppScreen::Game.title()),
            widgets::dim(&theme, "No game in progress. Start one from Play vs Computer."),
            widgets::button(&theme, "Back", Val::Auto, Click::Back),
        ]));
        return;
    };
    let game = Game::new(core.registry.clone(), active.handle.side());
    let mut pacer = Pacer::new(active.handle.side(), core.settings.desktop.animation_speed);
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
                justify_content: JustifyContent::SpaceBetween,
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
    let rail_column = commands
        .spawn((Node { width: px(layout::RAIL_WIDTH), height: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },))
        .add_child(rail)
        .add_child(log_row)
        .id();
    let body = commands
        .spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(layout::BODY_GAP), ..default() },))
        .add_child(board)
        .add_child(rail_column)
        .id();
    let top = commands
        .spawn((Node { width: percent(100), height: px(layout::TOP_BAR), flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(16), ..default() },))
        .with_children(|parent| {
            parent.spawn(widgets::heading(&theme, AppScreen::Game.title()));
            parent.spawn((StatusLine, widgets::dim(&theme, "Setting up…")));
            let mut quit = parent.spawn(widgets::button(&theme, "Quit", Val::Auto, Click::Quit));
            quit.entry::<Node>().and_modify(|mut node| node.margin = UiRect::left(Val::Auto));
            // The gear in the corner, where a person looks for options.
            parent.spawn(widgets::gear_button(&theme, images.as_deref_mut(), Click::Options));
        })
        .id();
    // The control bar along the bottom, centred: next to the person's
    // hand, so acting never means crossing the opponent's side.
    let bar = commands
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
        .id();
    let mut root = commands.spawn(screen_root(AppScreen::Game, theme.background));
    root.entry::<Node>().and_modify(|mut node| {
        node.align_items = AlignItems::Stretch;
        node.padding = UiRect::all(px(layout::PADDING));
        node.row_gap = px(layout::ROW_GAP);
        node.overflow = Overflow::clip();
    });
    root.add_child(top).add_child(body).add_child(bar);
    commands.insert_resource(BoardFit::default());
    commands.insert_resource(Model(game));
    let mut dirty = Dirty::default();
    dirty.all();
    commands.insert_resource(dirty);
}

/// Leaving the screen ends the match: dropping the handle quits it, and
/// the form gets the choice to reopen on.
fn leave(world: &mut World) {
    world.remove_resource::<Model>();
    world.remove_resource::<Pace>();
    if let Some(active) = world.remove_resource::<ActiveMatch>()
        && let Some(choice) = active.choice
    {
        world.insert_resource(LastGame(choice));
    }
}

/// Drains the match's messages into the pacer, and the beats due now
/// into the model: a run's events move the trail and the lane, a
/// message moves the board.
fn poll(active: Option<ResMut<ActiveMatch>>, model: Option<ResMut<Model>>, pace: Option<ResMut<Pace>>, time: Res<Time>, mut dirty: ResMut<Dirty>, dev: Option<ResMut<crate::dev::Dev>>) {
    let (Some(mut active), Some(mut model), Some(mut pace)) = (active, model, pace) else { return };
    while let Some(message) = active.handle.poll() {
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
                if let Outcome::Submit(action) = model.0.apply(Intent::Message(MatchMessageRef(message))) {
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
fn autoplay(dev: Option<ResMut<crate::dev::Dev>>, model: Option<Res<Model>>, mut pending: ResMut<Pending>, nodes: Query<(&Click, &ComputedNode, &UiGlobalTransform)>) {
    let (Some(mut dev), Some(model)) = (dev, model) else { return };
    if dev.options && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        // The gear, pressed once the board has settled.
        dev.options = false;
        pending.0.push(Intent::ToggleOptions);
        return;
    }
    if dev.keys && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        dev.keys = false;
        pending.0.push(Intent::Shortcut(Shortcut::Help));
    }
    if dev.menu && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        dev.menu = false;
        let hand = model.0.view.as_ref().and_then(|view| match model.0.side {
            Side::Corp => view.corp.hq_cards.clone(),
            Side::Runner => view.runner.grip_cards.clone(),
        });
        // The first card with an action, else the first card: a menu
        // with nothing in it is a look worth taking too.
        let hand = hand.unwrap_or_default();
        if let Some(card) = hand.iter().find(|card| !model.0.actions.for_hand_card(card).is_empty()).or(hand.first()) {
            let target = Target::HandCard(card.clone());
            // The card's own box, as the click would have read it.
            let over = nodes.iter().find(|(click, _, _)| **click == Click::Target(target.clone())).map_or_else(Anchor::default, |(_, node, transform)| anchor_of(node, transform));
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
    // Held at a card-selection prompt, or a card's install, for a
    // screenshot: the autoplay is done, and the pop-up is what is shot.
    let held = model.0.view.as_ref().is_some_and(|view| match view.pending_decision {
        Some(PendingDecision::ChooseCards { .. }) => dev.hold_selection,
        Some(PendingDecision::ChooseServer { install: Some(_), .. }) => dev.hold_install,
        _ => false,
    });
    if held {
        dev.autoplayed = dev.autoplay;
        return;
    }
    dev.autoplayed += 1;
    let index = model.0.applied % model.0.actions.entries.len();
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
    menu_parts: Query<&Interaction, With<MenuPart>>,
    model: Option<Res<Model>>,
    mut pending: ResMut<Pending>,
) {
    let Some(model) = model else { return };
    let modifier = secondary_modifier(&keys);
    let secondary = mouse.just_pressed(MouseButton::Right) || (modifier && mouse.just_pressed(MouseButton::Left));
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
fn drag_hand(
    mut moved: MessageReader<CursorMoved>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    slots: Query<(&HandSlot, &Interaction, &ComputedNode, &UiGlobalTransform)>,
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
        pending.0.push(Intent::DragRelease { over, slots: row.iter().map(|(_, x, _)| *x).collect() });
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
                settings_model::apply(&mut core.settings, settings_model::Intent::Toggle(row));
                if let Err(error) = core.save_settings() {
                    notices.push(format!("Settings not saved: {error}"));
                }
                // The phase bar is a row of the board, so the cards are
                // re-fitted around it; the helper is the rail's.
                dirty.rail = true;
                dirty.board = shortcut == Shortcut::PhaseBar;
            }
            _ => pending.0.push(Intent::Shortcut(shortcut)),
        }
    }
}

fn controls(
    mut pressed: MessageReader<Pressed>,
    keys: Res<ButtonInput<KeyCode>>,
    faces: Query<(Entity, &Interaction, &Click), (Changed<Interaction>, Without<widgets::Themed>)>,
    boxes: Query<(&ComputedNode, &UiGlobalTransform)>,
    slots: Query<&HandSlot>,
    mut pending: ResMut<Pending>,
    marks: Query<&Click>,
    settings_marks: Query<&SettingsControl>,
    mut core: ResMut<ClientCore>,
    model: Option<ResMut<Model>>,
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
        if *interaction == Interaction::Pressed
            && !modifier
            && let Click::Target(target) = click
        {
            // A card of the person's own hand is `drag_hand`'s: its press
            // is armed there and comes back as this same click if the
            // pointer never moved.
            if slots.contains(entity) {
                continue;
            }
            intents.push(click_on(entity, target));
        }
    }
    for Pressed(entity) in pressed.read() {
        // A row of the options menu: the settings model applies it and
        // the file is saved, as on the settings screen; the rail and the
        // log follow the new value on the next redraw.
        if let Ok(SettingsControl::Intent(intent)) = settings_marks.get(*entity) {
            if settings_model::apply(&mut core.settings, intent.clone()) {
                if let Err(error) = core.save_settings() {
                    notices.push(format!("Settings not saved: {error}"));
                }
                if let Some(pace) = pace.as_mut() {
                    pace.0.set_speed(core.settings.desktop.animation_speed);
                }
                dirty.rail = true;
                dirty.log = true;
                dirty.overlay = true;
            }
            continue;
        }
        match marks.get(*entity) {
            Ok(Click::Target(_)) if modifier => {}
            Ok(Click::Target(target)) => intents.push(click_on(*entity, target)),
            Ok(Click::Entry(index)) => intents.push(Intent::Choose(*index)),
            Ok(Click::Control(control)) => intents.push(Intent::Control(*control)),
            Ok(Click::Inspect(card)) => intents.push(Intent::InspectCard(Some(card.clone()))),
            Ok(Click::Expand(row)) => intents.push(Intent::Expand(*row)),
            Ok(Click::Options) => intents.push(Intent::ToggleOptions),
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
    let (Some(mut model), Some(active)) = (model, active) else { return };
    for intent in intents {
        match model.0.apply(intent) {
            Outcome::Nothing => {}
            Outcome::Redraw => {
                dirty.rail = true;
                dirty.overlay = true;
            }
            Outcome::Submit(action) => {
                if let Err(error) = active.handle.submit(action) {
                    notices.push(error);
                }
                dirty.rail = true;
                dirty.overlay = true;
            }
            Outcome::Quit => {
                navigate.write(Navigate(AppScreen::MainMenu));
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
    mut status: Query<&mut Text, With<StatusLine>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    images: Res<CardImages>,
    fit: Option<Res<BoardFit>>,
) {
    let (Some(mut model), Some(fit)) = (model, fit) else { return };
    if !(dirty.board || dirty.rail || dirty.log || dirty.overlay) {
        return;
    }
    let Dirty { board: reboard, rail: rerail, log: relog, overlay: reoverlay, lane: _ } = std::mem::take(&mut *dirty);
    let game = &mut model.0;
    if reboard {
        let transitions = game.take_transitions();
        if let Ok(board) = board.single() {
            commands.entity(board).despawn_children().with_children(|parent| spawn_board(parent, &theme, &core, &images, game, &transitions, &fit));
        }
        for mut text in &mut status {
            text.0 = status_line(game);
        }
    }
    let prefs = &core.settings.desktop;
    if rerail {
        if let Ok(rail) = rail.single() {
            commands.entity(rail).despawn_children().with_children(|parent| spawn_rail(parent, &theme, game, prefs.play_helper));
        }
        if let Ok(bar) = bar.single() {
            commands.entity(bar).despawn_children().with_children(|parent| spawn_control_bar(parent, &theme, game));
        }
        for (entity, _, _, _) in floating.iter().filter(|(_, _, popup, menu)| *popup || *menu) {
            commands.entity(entity).despawn();
        }
        let decisions = if game.awaiting && !game.finished() { game.actions.decisions() } else { Vec::new() };
        if let Some(root) = roots.iter().next() {
            if !decisions.is_empty() {
                commands.entity(root).with_children(|parent| spawn_decision_popup(parent, &theme, game, &decisions));
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
    format!("Turn {} · {phase} · you are the {:?}", view.turn, game.side)
}

fn overlay_needed(game: &Game) -> bool {
    game.covered()
}

// ---- the board ----

fn spawn_board(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, transitions: &[Transition], fit: &BoardFit) {
    let drag = game.dragged_slot();
    // The phase bar is drawn last, below the hand and above the control
    // bar: where the game is belongs beside what the person may do about
    // it, and a row at the top would have sat among the opponent's cards.
    let with_phase_bar = |parent: &mut ChildSpawnerCommands, game: &Game| {
        if core.settings.desktop.phase_bar {
            parent.spawn((PhaseBarRow, phase_bar_node())).with_children(|row| fill_phase_bar(row, theme, game));
        }
    };
    let Some(view) = &game.view else {
        parent.spawn(widgets::dim(theme, "Waiting for the match to start…"));
        with_phase_bar(parent, game);
        return;
    };
    let human = game.side;
    let opponent = human.other();
    let lit = Lit::of(transitions);
    // Four rows, top to bottom, sized by `fit` so they never scroll: the
    // opponent's strip with their hand as backs, their area, the
    // person's area, the person's strip with their hand.
    parent.spawn(strip_row()).with_children(|row| {
        spawn_strip(row, theme, core, images, game, view, opponent, fit);
        spawn_opponent_hand(row, theme, images, view, opponent, fit);
    });
    spawn_area(parent, theme, core, images, game, view, opponent, &lit, fit);
    // The run lane, between the servers and the rig from either chair,
    // reserved whether or not a run is on (`layout::RUN_LANE`).
    parent.spawn((RunLane, run_lane_node())).with_children(|lane| fill_run_lane(lane, theme, core, game));
    spawn_area(parent, theme, core, images, game, view, human, &lit, fit);
    parent.spawn(strip_row()).with_children(|row| {
        spawn_strip(row, theme, core, images, game, view, human, fit);
        spawn_hand(row, theme, core, images, game, view, human, &lit, fit, drag);
    });
    with_phase_bar(parent, game);
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
fn spawn_strip(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, side: Side, fit: &BoardFit) {
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
                width: px(fit.identity_size().width() + layout::STRIP_TEXT),
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
                let image = card.numeric_id.and_then(|code| images.face(code));
                spawn_face(row, theme, &Face::of(card), fit.identity_size(), image, (Button, Click::Target(Target::Identity(side))));
            }
            row.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_width: px(0), row_gap: px(2), ..default() },)).with_children(|column| {
                let title = identity.as_ref().and_then(|id| core.registry.get(id)).map_or_else(|| format!("{side:?}"), |c| c.title.clone());
                column.spawn((Text::new(format!("{who} · {title}")), theme.font(size::SMALL), TextColor(colour), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                spawn_hud(column, theme, view, side);
                if let Some(line) = hud::details(view, side) {
                    column.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
                // The Runner's piles are zones a click opens — the stack
                // for its draw, the heap for what is in it — as the
                // Corp's centrals are through their server headers.
                if side == Side::Runner {
                    column.spawn(widgets::row(6.0)).with_children(|piles| {
                        compact_button(piles, theme, format!("Stack · {}", view.runner.stack_count), Click::Target(Target::Pile(Pile::Stack)));
                        compact_button(piles, theme, format!("Heap · {}", view.runner.heap.len()), Click::Target(Target::Pile(Pile::Heap)));
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
fn spawn_hud(parent: &mut ChildSpawnerCommands, theme: &Theme, view: &ClientView, side: Side) {
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
                let mut cell = match readout.opens {
                    Some(pile) => grid.spawn((
                        marker,
                        Button,
                        widgets::Themed,
                        Click::Target(Target::Pile(pile)),
                        Node { padding: UiRect::axes(px(6), px(0)), margin: UiRect::left(px(-6)), border_radius: BorderRadius::all(px(6)), ..node },
                        BackgroundColor(theme.button),
                    )),
                    None => grid.spawn((marker, node)),
                };
                cell.with_children(|cell| {
                        cell.spawn((Text::new(readout.value), theme.font(size::HEADING), TextColor(colour)));
                        cell.spawn((Text::new(readout.label), theme.font(size::SMALL), TextColor(if readout.alarm { theme.danger } else { theme.text_dim })));
                    });
            }
        });
}

/// The space a strip row leaves for its cards.
fn beside_strip(fit: &BoardFit) -> f32 {
    fit.board_width() - (fit.identity_size().width() + layout::STRIP_TEXT) - 12.0
}

/// The opponent's hand as backs at the strip's size, overlapped when
/// there are many — a count is in the strip, and a row of backs is
/// what a table shows.
fn spawn_opponent_hand(parent: &mut ChildSpawnerCommands, theme: &Theme, images: &CardImages, view: &ClientView, side: Side, fit: &BoardFit) {
    let count = match side {
        Side::Corp => view.corp.hq_count,
        Side::Runner => view.runner.grip_count,
    };
    let size = fit.identity_size();
    let available = beside_strip(fit);
    parent.spawn(card_row()).with_children(|row| {
        let backs: Vec<Entity> = (0..count).map(|_| spawn_back(row, theme, images.back(side), side, size, ())).collect();
        overlap(row, &backs, size.width(), available);
    });
}

/// A side's board: the Corp's servers, or the Runner's rig.
#[allow(clippy::too_many_arguments)]
fn spawn_area(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, side: Side, lit: &Lit, fit: &BoardFit) {
    match side {
        Side::Corp => spawn_servers(parent, theme, core, game, view, lit, fit),
        Side::Runner => spawn_rig(parent, theme, core, images, view, lit, fit),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_servers(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, game: &Game, view: &ClientView, lit: &Lit, fit: &BoardFit) {
    // Archives, R&D, HQ, then the remotes, from either chair, every
    // central a column even with nothing on it (`board::table_servers`).
    let servers = netrunner_client::board::table_servers(view);
    let run = view.active_run.as_ref();
    let encountered = run.filter(|r| matches!(r.phase, RunPhase::ApproachIce | RunPhase::EncounterIce)).and_then(|r| r.ice.get(r.position)).map(|i| i.install_id);
    let size = fit.size();
    // The columns line up along the Corp's edge of the table: their
    // headers at the top from the Runner's chair, at the bottom from
    // the Corp's, however tall their ice makes them.
    let mut row_node = card_row();
    row_node.align_items = if game.side == Side::Corp { AlignItems::FlexEnd } else { AlignItems::FlexStart };
    parent.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() },)).with_children(|area| {
        section_label(area, theme, "Servers");
        area.spawn(row_node).with_children(|row| {
            for server in &servers {
                let under_run = game.run_on(server.server);
                let border = if under_run { theme.runner } else { theme.panel_border };
                row.spawn((
                    ServerColumn(server.server),
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
                    BackgroundColor(theme.panel),
                    BorderColor::all(border),
                ))
                .with_children(|column| {
                    // Header, root and ice in the chair's order
                    // (`layout::column_top_down`): the header nearest the
                    // Corp, the ice out toward the Runner, outermost nearest.
                    for piece in layout::column_top_down(game.side) {
                        match piece {
                            layout::Piece::Header => spawn_server_header(column, theme, view, server.server),
                            layout::Piece::Ice => spawn_server_ice(column, theme, core, view, server, game.side, encountered, lit, size),
                            layout::Piece::Root => spawn_server_root(column, theme, core, view, server, lit, size),
                        }
                    }
                });
            }
        });
    });
}

fn spawn_server_header(column: &mut ChildSpawnerCommands, theme: &Theme, view: &ClientView, server: ServerId) {
    let count = match server {
        ServerId::Hq => format!(" · {}", view.corp.hq_count),
        ServerId::RnD => format!(" · {}", view.corp.rd_count),
        ServerId::Archives => format!(" · {}", view.corp.archives.len()),
        ServerId::Remote(_) => String::new(),
    };
    compact_button(column, theme, format!("{}{count}", server_name(server)), Click::Target(Target::Server(server)));
}

/// A tile in a server column — an ice or a root card — the same block
/// the header is: the title when it may be named, a number or two, and
/// a border in the card's faction colour when it is rezzed. A click
/// opens the card's sheet; the picture is read there, not here, so the
/// column costs `layout::TILE` per piece and never a face.
fn spawn_tile(column: &mut ChildSpawnerCommands, theme: &Theme, label: String, colour: Color, text_colour: Color, install: InstallId, lit: bool, size: FaceSize) -> Entity {
    let mut tile = column.spawn((
        Button,
        widgets::Themed,
        Click::Target(Target::Install(install)),
        Node {
            width: px(size.width() + 4.0),
            height: px(layout::TILE - 4.0),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            overflow: Overflow::clip(),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(theme.button),
        BorderColor::all(colour),
        children![(Text::new(label), theme.font(size::SMALL - 3.0), TextColor(text_colour))],
    ));
    if lit {
        tile.insert(outline(theme));
    }
    tile.id()
}

/// A server's ice as tiles, labelled by `board::facts::tile_label` —
/// the title when it may be named, rezzed or unrezzed, its strength
/// now, its tokens — with the run's marker on the piece being approached.
#[allow(clippy::too_many_arguments)]
fn spawn_server_ice(column: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, view: &ClientView, server: &ServerView, chair: Side, encountered: Option<InstallId>, lit: &Lit, size: FaceSize) {
    for ice in layout::ice_top_down(&server.ice, chair) {
        let def = ice.card.as_ref().and_then(|id| core.registry.get(id));
        let label = facts::tile_label(view, ice.install_id, &core.registry);
        let colour = if ice.rezzed { theme.faction(def.and_then(|c| c.faction)) } else { theme.corp.with_alpha(0.5) };
        let text_colour = if ice.rezzed { theme.text } else { theme.text_dim };
        let is_lit = encountered == Some(ice.install_id) || lit.installs.contains(&ice.install_id);
        spawn_tile(column, theme, label, colour, text_colour, ice.install_id, is_lit, size);
    }
}

/// The cards in a server's root as tiles, labelled by
/// `board::facts::tile_label`: rezzed or unrezzed for an asset or an
/// upgrade, `2/3 adv` for an agenda the viewer knows, `face down` with
/// its tokens for a card the viewer cannot name (advancement is public).
/// An agenda's border is its faction's: it has no rez to wait for.
fn spawn_server_root(column: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, view: &ClientView, server: &ServerView, lit: &Lit, size: FaceSize) {
    for card in &server.root {
        let def = card.card.as_ref().and_then(|id| core.registry.get(id));
        let label = facts::tile_label(view, card.install_id, &core.registry);
        let face_up = card.rezzed || def.is_some_and(|d| d.card_type == CardType::Agenda);
        let colour = if face_up { theme.faction(def.and_then(|c| c.faction)) } else { theme.corp.with_alpha(0.5) };
        let text_colour = if face_up { theme.text } else { theme.text_dim };
        spawn_tile(column, theme, label, colour, text_colour, card.install_id, lit.installs.contains(&card.install_id), size);
    }
}

/// The rig as one row of three groups; the cards of every group overlap
/// by the same amount when the whole rig would not fit across.
fn spawn_rig(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, view: &ClientView, lit: &Lit, fit: &BoardFit) {
    let group = |kind: &CardType| match kind {
        CardType::Program => 0,
        CardType::Hardware => 1,
        _ => 2,
    };
    let size = fit.size();
    let groups: Vec<Vec<_>> = (0..3).map(|wanted| view.runner.rig.iter().filter(|c| core.registry.get(&c.card).is_some_and(|def| group(&def.card_type) == wanted)).collect()).collect();
    let filled = groups.iter().filter(|g| !g.is_empty()).count();
    let total: usize = groups.iter().map(Vec::len).sum();
    // The step every group uses: the whole rig, less the gaps between
    // groups, as if it were one row.
    let available = fit.board_width() - (filled.saturating_sub(1) as f32) * 16.0;
    let step = layout::step(total, size.width(), layout::CARD_GAP, available);
    let pull = (step - (size.width() + layout::CARD_GAP)).min(0.0);
    parent.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() },)).with_children(|area| {
        section_label(area, theme, if view.runner.rig.is_empty() { "Rig · nothing installed" } else { "Rig" });
        area.spawn((Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, column_gap: px(16), align_items: AlignItems::FlexStart, ..default() },)).with_children(|row| {
            for (wanted, cards) in groups.iter().enumerate() {
                if cards.is_empty() {
                    continue;
                }
                row.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, row_gap: px(2), ..default() },)).with_children(|column| {
                    column.spawn(widgets::dim(theme, ["Programs", "Hardware", "Resources"][wanted]));
                    column.spawn(card_row()).with_children(|cards_row| {
                        for (i, card) in cards.iter().enumerate() {
                            let Some(def) = core.registry.get(&card.card) else { continue };
                            let image = def.numeric_id.and_then(|code| images.face(code));
                            let mut slot = cards_row.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, align_items: AlignItems::Center, row_gap: px(2), ..default() },));
                            if i > 0 && pull < 0.0 {
                                slot.entry::<Node>().and_modify(move |mut node| node.margin.left = px(pull));
                            }
                            slot.with_children(|slot| {
                                let entity = spawn_face(slot, theme, &Face::of(def), size, image, (Button, Click::Target(Target::Install(card.install_id))));
                                if lit.installs.contains(&card.install_id) {
                                    slot.commands().entity(entity).insert(outline(theme));
                                }
                                let mut chips = Vec::new();
                                if def.card_type == CardType::Program && def.strength.is_some() {
                                    chips.push(format!("str {}", card.current_strength));
                                }
                                if card.counters > 0 {
                                    chips.push(format!("{} ctr", card.counters));
                                }
                                if !card.hosted_cards.is_empty() {
                                    chips.push(format!("{} hosted", card.hosted_cards.len()));
                                }
                                if !chips.is_empty() {
                                    slot.spawn(widgets::dim(theme, chips.join(" · ")));
                                }
                            });
                        }
                    });
                });
            }
        });
    });
}

/// The person's hand beside their strip, overlapped when it is wide.
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
    let size = fit.size();
    let available = beside_strip(fit);
    parent.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() },)).with_children(|column| {
        section_label(column, theme, format!("Your hand · {}", hand.len()));
        column.spawn(card_row()).with_children(|row| {
            // A hand is a multiset; the first copy of a lit card is the one
            // outlined, which is as much as a highlight can say.
            let mut lit_left = lit.hand.clone();
            let mut faces = Vec::new();
            for (slot, id) in hand.iter().enumerate() {
                let Some(def) = core.registry.get(id) else { continue };
                let image = def.numeric_id.and_then(|code| images.face(code));
                let entity = spawn_face(row, theme, &Face::of(def), size, image, (Button, Click::Target(Target::HandCard(id.clone()))));
                if own {
                    // Its place in the row, so a drag knows which card it
                    // picked up and where the others sit.
                    row.commands().entity(entity).insert(HandSlot(slot));
                }
                if let Some(at) = lit_left.iter().position(|c| c == id) {
                    lit_left.swap_remove(at);
                    row.commands().entity(entity).insert(outline(theme));
                }
                // The card being dragged is lifted out of the row by its
                // outline: the row itself never moves under the pointer,
                // because a hand that re-flowed mid-drag moved the gap the
                // person was aiming at.
                if own && drag.is_some_and(|dragged| dragged == slot) {
                    row.commands().entity(entity).insert(outline(theme));
                }
                faces.push(entity);
            }
            overlap(row, &faces, size.width(), available);
        });
    });
}

// ---- the control bar and the rail ----

/// One button per control of the person's side, in the bar's fixed
/// order; enabled when the engine lists what it means and the person
/// may act, greyed otherwise. Greyed rather than absent so "End turn"
/// is always in the same place.
fn spawn_control_bar(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game) {
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
    if let Some(prompt) = &game.prompt {
        parent.spawn((widgets::label(theme, prompt.title.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        if !prompt.detail.is_empty() {
            parent.spawn((widgets::dim(theme, prompt.detail.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
    }
    if let Some(rejection) = &game.rejection {
        parent.spawn((widgets::notice(theme, format!("Rejected: {rejection}"), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if game.end_turn_armed {
        let clicks = match game.clicks_left() {
            1 => "1 click".to_string(),
            n => format!("{n} clicks"),
        };
        parent.spawn((EndTurnNotice, widgets::notice(theme, format!("{clicks} left — press Enter again to end the turn."), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if let Some(reason) = &game.stalled {
        parent.spawn((widgets::notice(theme, reason.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        return;
    }
    if game.over.is_some() {
        parent.spawn(widgets::dim(theme, "The match is over."));
        return;
    }
    if !game.awaiting {
        parent.spawn(widgets::dim(theme, if game.view.is_some() { "Opponent is thinking…" } else { "Setting up…" }));
        return;
    }
    // The decisions are the pop-up's (`spawn_decision_popup`), not the
    // rail's; the rail keeps the prompt's words and, when on, the flat
    // panel.
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
                entry_button(list, theme, game, index);
            }
        })
        .id();
    parent.spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(list).with_children(|row| {
        row.spawn(widgets::scrollbar(theme, list));
    });
}

/// A full-width button for entry `index`, its label left-aligned.
fn entry_button(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, index: usize) -> Option<Entity> {
    let entry = game.actions.entries.get(index)?;
    let mut button = parent.spawn(widgets::button(theme, entry.label.clone(), percent(100), Click::Entry(index)));
    button.entry::<Node>().and_modify(|mut node| {
        node.justify_content = JustifyContent::FlexStart;
        node.padding = UiRect::axes(px(10), px(6));
    });
    Some(button.id())
}

/// The width of a click's menu, and the gap between its
/// bottom edge and the top of the card it sits above.
const MENU_WIDTH: f32 = 280.0;
const MENU_GAP: f32 = 6.0;

/// The menu a click opened: the target's name and one button per
/// entry, or a line saying so when there is nothing,
/// in a small panel just above the box the target was laid out in and
/// centred on it — so it is in one place for a card however the card
/// was clicked, and the card stays in view beneath it. When the box is
/// too near the top for the menu to fit above, it sits just below
/// instead (a header along the top edge, from the Runner's chair).
/// Kept on the window sideways: pulled in when it would run off the
/// left or right edge. The height is an estimate (the layout has not
/// run when it is spawned, and a menu is a heading and a row per
/// entry). The panel
/// takes `Interaction` and blocks, so a click on its ground is a click
/// on the menu, not on the card beneath. Between the decision pop-up
/// and the overlays in depth: a sheet covers it, it covers the pop-up.
fn spawn_actions_menu(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, menu: &crate::models::game::Menu, window: Vec2) {
    // Padding, the heading, the panel's row gap, then a 40 px button (or
    // the one-line notice) per row with the gap between rows.
    let rows = menu.entries.len().max(1) as f32;
    let height = 2.0 * 12.0 + 22.0 + 8.0 + rows * 40.0 + (rows - 1.0) * 8.0;
    let left = (menu.over.x - MENU_WIDTH / 2.0).min(window.x - MENU_WIDTH - layout::PADDING).max(layout::PADDING);
    let above = menu.over.y - menu.over.height / 2.0 - MENU_GAP - height;
    let below = menu.over.y + menu.over.height / 2.0 + MENU_GAP;
    let top = if above >= layout::PADDING { above } else { below.min(window.y - height - layout::PADDING) };
    let accent = theme.accent;
    let mut panel = parent.spawn((ActionsMenu, MenuPart, Interaction::None, FocusPolicy::Block, GlobalZIndex(15), widgets::panel(theme, px(MENU_WIDTH))));
    panel.entry::<Node>().and_modify(move |mut node| {
        node.position_type = PositionType::Absolute;
        node.left = px(left);
        node.top = px(top);
        node.padding = UiRect::all(px(12));
    });
    panel.entry::<BorderColor>().and_modify(move |mut border| *border = BorderColor::all(accent));
    panel.with_children(|panel| {
        panel.spawn((widgets::label(theme, target_title(game, &menu.target)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        if menu.entries.is_empty() {
            let line = if game.awaiting { "Nothing to do here right now." } else { "Not your decision right now." };
            panel.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
        for index in &menu.entries {
            if let Some(button) = entry_button(panel, theme, game, *index) {
                panel.commands().entity(button).insert(MenuPart);
            }
        }
    });
}

/// The decision pop-up: the prompt's words as its heading and one
/// button per decision, centred over the board. The container is the
/// whole window so the panel can be centred in it, and ignores picking
/// so the board beneath stays clickable; only the panel and its
/// buttons are hit. Under the overlays (`GlobalZIndex(20)`), so a
/// sheet opened to read a card covers it.
fn spawn_decision_popup(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, decisions: &[usize]) {
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
            let mut panel = screen.spawn(widgets::panel(theme, px(520)));
            panel.entry::<BorderColor>().and_modify(move |mut border| *border = BorderColor::all(accent));
            panel.with_children(|panel| {
                let (title, detail) = match &game.prompt {
                    Some(prompt) => (prompt.title.clone(), prompt.detail.clone()),
                    None => ("Your decision".to_string(), String::new()),
                };
                panel.spawn((widgets::heading(theme, title), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                if !detail.is_empty() {
                    panel.spawn((widgets::dim(theme, detail), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
                if let Some(rejection) = &game.rejection {
                    panel.spawn((widgets::notice(theme, format!("Rejected: {rejection}"), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
                for index in decisions {
                    entry_button(panel, theme, game, *index);
                }
            });
        });
}

// ---- the phase bar ----

fn phase_bar_node() -> Node {
    Node {
        width: percent(100),
        height: px(layout::PHASE_BAR - layout::ROW_GAP),
        flex_shrink: 0.0,
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        row_gap: px(2),
        overflow: Overflow::clip(),
        ..default()
    }
}

/// The turn's steps, and a run's, from `board::phase`: a chip per step in
/// order, the one in play in the accent colour and outlined, the ones
/// behind it dim. A segment's title is a chip of its own at the head of
/// its row, so "Corp turn 12" and "Run on HQ" read as the headings they
/// are, and the window's line sits under the lot.
fn fill_phase_bar(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game) {
    use netrunner_client::board::phase::{self, State};
    let Some(view) = &game.view else {
        parent.spawn(widgets::dim(theme, "Setting up…"));
        return;
    };
    let bar = phase::bar(view);
    parent.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), overflow: Overflow::clip(), ..default() },)).with_children(|row| {
        for (n, segment) in bar.segments.iter().enumerate() {
            if n > 0 {
                row.spawn((Text::new("·"), theme.font(size::SMALL), TextColor(theme.text_dim)));
            }
            row.spawn((Text::new(segment.title.clone()), theme.font(size::SMALL), TextColor(theme.text)));
            for step in &segment.steps {
                let (text, background, border) = match step.state {
                    State::Past => (theme.text_dim, theme.panel, theme.panel),
                    State::Now => (theme.background, theme.accent, theme.accent),
                    State::Ahead => (theme.text_dim, theme.background, theme.panel_border),
                };
                row.spawn((
                    PhaseStep(step.state),
                    Node {
                        padding: UiRect::axes(px(8), px(3)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(10)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(background),
                    BorderColor::all(border),
                    children![(Text::new(step.label.clone()), theme.font(size::SMALL), TextColor(text))],
                ));
            }
        }
    });
    match &bar.note {
        Some(note) => {
            parent.spawn((Text::new(note.clone()), theme.font(size::SMALL), TextColor(theme.accent)));
        }
        // A line only when a window is open, and a blank one to hold the
        // row's height when it is not, so the bar never changes the board.
        None => {
            parent.spawn((Text::new(" "), theme.font(size::SMALL), TextColor(theme.text_dim)));
        }
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
    let (border, background, text) = match style {
        ChipStyle::Origin => (theme.runner, theme.runner.with_alpha(0.2), theme.text),
        ChipStyle::Upcoming => (theme.panel_border, theme.panel, theme.text_dim),
        ChipStyle::Current => (theme.accent, theme.accent.with_alpha(0.25), theme.text),
        ChipStyle::Done => (theme.panel_border, theme.button, theme.text_dim),
        ChipStyle::Success => (theme.runner, theme.runner.with_alpha(0.25), theme.text),
        ChipStyle::Ended => (theme.corp, theme.corp.with_alpha(0.25), theme.text),
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
    parent
        .spawn((
            Overlay,
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
            BackgroundColor(theme.background.with_alpha(0.75)),
        ))
        .with_children(|screen| {
            screen.spawn(widgets::panel(theme, width)).with_children(|panel| {
                if let Some(reason) = &game.stalled {
                    panel.spawn(widgets::heading(theme, "The match stopped"));
                    panel.spawn((widgets::dim(theme, reason.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    panel.spawn(widgets::button(theme, "Menu", Val::Auto, Click::Menu));
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
                    let consequence = if turn >= netrunner_client::ratings::FORFEIT_FROM_TURN {
                        "From turn 3 on, a quit counts as a loss on your ladder."
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
                    panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
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

/// The card large, and nothing else: the picture, or the text layout
/// (which carries the printed text) while no picture is cached. What
/// may be done with it is the menu's — the sheet once listed the actions
/// under the card's text, and the person asked for reading and acting to
/// be two clicks rather than one panel.
fn card_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, id: &CardId) {
    let Some(def) = core.registry.get(id) else {
        panel.spawn(widgets::dim(theme, format!("{} is not in the registry", id.0)));
        panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
        return;
    };
    panel.spawn(widgets::heading(theme, def.title.clone()));
    let image = def.numeric_id.and_then(|code| images.face(code));
    spawn_face(panel, theme, &Face::of(def), FaceSize::Large, image, ());
    panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
}

/// An installed card: the face (or the back, for a card the viewer
/// cannot name) beside its state — `board::facts::install_facts`: where
/// it sits and in what order it is met, rezzed or not and the cost of a
/// rez, strength now and printed, each subroutine with its status in an
/// encounter, tokens, counters, trash cost, what it hosts. The state is
/// the one thing beside the card, because the printed face cannot show
/// it and a tile has no room; the printed text is on the face.
fn install_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, id: InstallId, card: Option<&CardId>) {
    let Some(view) = &game.view else { return };
    let def = card.and_then(|c| core.registry.get(c));
    let heading = def.map_or_else(|| facts::hidden_title(view, id), |d| d.title.clone());
    panel.spawn(widgets::heading(theme, heading));
    panel.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(16), align_items: AlignItems::FlexStart, ..default() },)).with_children(|row| {
        match def {
            Some(def) => {
                let image = def.numeric_id.and_then(|code| images.face(code));
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
    panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
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
        panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
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
                                    let image = def.numeric_id.and_then(|code| images.face(code));
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
    panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
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
                let image = def.and_then(|d| d.numeric_id).and_then(|code| images.face(code));
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
                        spawn_face(button, theme, &Face::of(def), FaceSize::Board(56), image.clone(), ());
                    }
                    button.spawn(widgets::label(theme, agenda.line()));
                });
                if !open {
                    continue;
                }
                list.spawn((ScoreDetails(row), Node { flex_direction: FlexDirection::Row, column_gap: px(16), align_items: AlignItems::FlexStart, padding: UiRect::left(px(28)), flex_shrink: 0.0, ..default() })).with_children(|details| {
                    if let Some(def) = def {
                        spawn_face(details, theme, &Face::of(def), FaceSize::Large, image, ());
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
                                entry_button(column, theme, game, index);
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
