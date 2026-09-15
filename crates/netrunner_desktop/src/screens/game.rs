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
//! **Sheets.** A card's sheet is the Large face, its text and its legal
//! actions as buttons; a zone's sheet is its actions and its contents
//! where the viewer may see them — Archives as every card for the Corp
//! and as the face-up ones with backs for the rest for the Runner, the
//! heap, the Corp's own HQ, a remote's ICE and root — each face a button
//! that reads the card over the sheet. R&D and the stack are backs and a
//! count: a deck's order is never shown, even to its owner.
//!
//! **Highlights come from `board::diff`.** After a redraw, every install
//! and hand card a `Transition` names is outlined for that redraw, and
//! nothing is ever inferred from the previous frame's nodes. §4 turns
//! the same transitions into movement and sound.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use netrunner_client::board::action_map::server_name;
use netrunner_client::board::{Control, Pile, Target, Transition, Zone};
use netrunner_client::card_face::Face;
use netrunner_core::dsl::{CardId, CardType};
use netrunner_core::rules::{GamePhase, InstallId, InstallSlot, RunPhase, ServerId, Side};
use netrunner_core::view::{ClientView, ServerView};

use crate::card_images::CardImages;
use crate::core::{ClientCore, Notices};
use crate::models::game::{Game, Intent, MatchMessageRef, Outcome};
use crate::models::layout::{self, Counts};
use crate::models::settings::{self as settings_model, Row};
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
        app.init_resource::<Pending>()
            .add_systems(OnEnter(AppScreen::Game), spawn)
            .add_systems(OnExit(AppScreen::Game), leave)
            .add_systems(Update, (poll, autoplay, escape.in_set(Captures), controls, fit, redraw).chain().run_if(in_state(AppScreen::Game)));
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
}

impl Dirty {
    fn all(&mut self) {
        self.board = true;
        self.rail = true;
        self.log = true;
        self.overlay = true;
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
fn counts(game: &Game) -> Counts {
    let human_is_runner = game.side == Side::Runner;
    let Some(view) = &game.view else { return Counts { human_is_runner, ..Counts::default() } };
    let ice = view.corp.servers.iter().map(|s| s.ice.len()).max().unwrap_or(0);
    // The three centrals are always drawn.
    let remotes = view.corp.servers.iter().filter(|s| matches!(s.server, ServerId::Remote(_))).count();
    let roots = view.corp.servers.iter().any(|s| !s.root.is_empty());
    Counts { ice, servers: 3 + remotes, human_is_runner, roots, rig: !view.runner.rig.is_empty() }
}

/// Recomputes the face width from the window and the view, and marks the
/// board for a redraw when it moved. Runs every frame and is cheap: a
/// handful of comparisons.
fn fit(windows: Query<&Window, With<PrimaryWindow>>, model: Option<Res<Model>>, fit: Option<ResMut<BoardFit>>, mut dirty: ResMut<Dirty>) {
    let (Some(model), Some(mut fit)) = (model, fit) else { return };
    let window = windows.single().map_or(fit.window, |w| Vec2::new(w.width(), w.height()));
    let face = layout::face_width((window.x, window.y), counts(&model.0));
    if (face - fit.face).abs() > 0.5 || window != fit.window {
        fit.face = face;
        fit.window = window;
        dirty.board = true;
    }
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, active: Option<Res<ActiveMatch>>, mut images: Option<ResMut<Assets<Image>>>) {
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
    if let Some(active) = world.remove_resource::<ActiveMatch>()
        && let Some(choice) = active.choice
    {
        world.insert_resource(LastGame(choice));
    }
}

/// Drains the match's messages into the model.
fn poll(active: Option<ResMut<ActiveMatch>>, model: Option<ResMut<Model>>, mut dirty: ResMut<Dirty>) {
    let (Some(mut active), Some(mut model)) = (active, model) else { return };
    while let Some(message) = active.handle.poll() {
        model.0.apply(Intent::Message(MatchMessageRef(message)));
        dirty.all();
    }
}

/// The dev hook's hand: while `NETRUNNER_AUTOPLAY` has decisions left,
/// takes one from the panel each time the board is awaiting — the
/// `applied`th entry, so the choice wanders through the list and the
/// game develops rather than clicking for credits forever. Off, and
/// ignored, for a person.
fn autoplay(dev: Option<ResMut<crate::dev::Dev>>, model: Option<Res<Model>>, mut pending: ResMut<Pending>) {
    let (Some(mut dev), Some(model)) = (dev, model) else { return };
    if dev.options && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        // The gear, pressed once the board has settled.
        dev.options = false;
        pending.0.push(Intent::ToggleOptions);
        return;
    }
    if dev.autoplayed >= dev.autoplay || !model.0.awaiting || model.0.actions.is_empty() {
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

fn controls(
    mut pressed: MessageReader<Pressed>,
    faces: Query<(&Interaction, &Click), (Changed<Interaction>, Without<widgets::Themed>)>,
    mut pending: ResMut<Pending>,
    marks: Query<&Click>,
    settings_marks: Query<&SettingsControl>,
    mut core: ResMut<ClientCore>,
    model: Option<ResMut<Model>>,
    active: Option<Res<ActiveMatch>>,
    mut dirty: ResMut<Dirty>,
    mut notices: ResMut<Notices>,
    mut navigate: MessageWriter<Navigate>,
) {
    let mut intents: Vec<Intent> = std::mem::take(&mut pending.0);
    let mut leave_to: Option<AppScreen> = None;
    // A face is a `Button` without the theme's recolouring, so the
    // shared feedback system never reports it; its press is read here.
    // The first hand-driven game found every card click doing nothing.
    for (interaction, click) in &faces {
        if *interaction == Interaction::Pressed
            && let Click::Target(target) = click
        {
            intents.push(Intent::Click(target.clone()));
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
                dirty.rail = true;
                dirty.log = true;
                dirty.overlay = true;
            }
            continue;
        }
        match marks.get(*entity) {
            Ok(Click::Target(target)) => intents.push(Intent::Click(target.clone())),
            Ok(Click::Entry(index)) => intents.push(Intent::Choose(*index)),
            Ok(Click::Control(control)) => intents.push(Intent::Control(*control)),
            Ok(Click::Inspect(card)) => intents.push(Intent::Inspect(Some(card.clone()))),
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
    // One query for both floating layers: a system takes sixteen
    // parameters at most, and this one is at the limit.
    floating: Query<(Entity, Has<Overlay>, Has<DecisionPopup>), Or<(With<Overlay>, With<DecisionPopup>)>>,
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
    let Dirty { board: reboard, rail: rerail, log: relog, overlay: reoverlay } = std::mem::take(&mut *dirty);
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
        for (popup, _, _) in floating.iter().filter(|(_, _, popup)| *popup) {
            commands.entity(popup).despawn();
        }
        let decisions = if game.awaiting && !game.finished() { game.actions.decisions() } else { Vec::new() };
        if let Some(root) = roots.iter().next()
            && !decisions.is_empty()
        {
            commands.entity(root).with_children(|parent| spawn_decision_popup(parent, &theme, game, &decisions));
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
        for (overlay, _, _) in floating.iter().filter(|(_, overlay, _)| *overlay) {
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
    game.finished() || game.confirm_quit || game.options_open || game.sheet.is_some() || game.inspecting.is_some()
}

// ---- the board ----

fn spawn_board(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, transitions: &[Transition], fit: &BoardFit) {
    let Some(view) = &game.view else {
        parent.spawn(widgets::dim(theme, "Waiting for the match to start…"));
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
    spawn_area(parent, theme, core, images, game, view, human, &lit, fit);
    parent.spawn(strip_row()).with_children(|row| {
        spawn_strip(row, theme, core, images, game, view, human, fit);
        spawn_hand(row, theme, core, images, view, human, &lit, fit);
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
                // The numbers a player watches, in the body size and the
                // text colour: dim and small, nobody saw them.
                for line in strip_lines(view, side) {
                    column.spawn((widgets::label(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
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

fn strip_lines(view: &ClientView, side: Side) -> Vec<String> {
    let to_win = view.rules.winning_agenda_points;
    match side {
        Side::Corp => {
            let corp = &view.corp;
            let mut lines = vec![
                format!("Credits {} · Clicks {} · Agenda points {}/{to_win}", corp.credits, corp.clicks, corp.agenda_points),
                format!("HQ {} · R&D {} · Archives {} · Bad publicity {}", corp.hq_count, corp.rd_count, corp.archives.len(), corp.bad_publicity),
            ];
            if !corp.scored_agendas.is_empty() {
                lines.push(format!("Scored: {}", corp.scored_agendas.iter().map(|a| a.card.0.replace('_', " ")).collect::<Vec<_>>().join(", ")));
            }
            lines
        }
        Side::Runner => {
            let runner = &view.runner;
            let mut lines = vec![
                format!("Credits {} · Clicks {} · Agenda points {}/{to_win}", runner.credits, runner.clicks, runner.agenda_points),
                format!("Grip {} · Stack {} · Heap {} · Tags {} · MU {} · Link {}", runner.grip_count, runner.stack_count, runner.heap.len(), runner.tags, runner.memory_units, runner.link_strength),
            ];
            if runner.brain_damage > 0 {
                lines.push(format!("Core damage {}", runner.brain_damage));
            }
            if !runner.scored_agendas.is_empty() {
                lines.push(format!("Stolen: {}", runner.scored_agendas.iter().map(|c| c.0.replace('_', " ")).collect::<Vec<_>>().join(", ")));
            }
            lines
        }
    }
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
        Side::Corp => spawn_servers(parent, theme, core, images, game, view, lit, fit),
        Side::Runner => spawn_rig(parent, theme, core, images, view, lit, fit),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_servers(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, lit: &Lit, fit: &BoardFit) {
    // Every central is a column even when nothing is installed on it,
    // because a run on an empty central is a click on its header.
    let mut servers: Vec<ServerView> = view.corp.servers.clone();
    for central in [ServerId::Hq, ServerId::RnD, ServerId::Archives] {
        if !servers.iter().any(|s| s.server == central) {
            servers.push(ServerView { server: central, ice: Vec::new(), root: Vec::new() });
        }
    }
    // The table seen from the chair (`layout::servers_left_to_right`):
    // the Corp's Archives, R&D, HQ, remotes; the Runner's mirror.
    let order = layout::servers_left_to_right(servers.iter().map(|s| s.server), game.side);
    servers.sort_by_key(|s| order.iter().position(|id| *id == s.server));
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
                let border = if under_run { theme.accent } else { theme.panel_border };
                row.spawn((
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
                            layout::Piece::Ice => spawn_server_ice(column, theme, core, server, game.side, encountered, lit, size),
                            layout::Piece::Root => spawn_server_root(column, theme, core, images, server, lit, size),
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

/// A server's ice as bars: the title when it can be named, its strength
/// when rezzed, and the run's marker on the piece being approached.
#[allow(clippy::too_many_arguments)]
fn spawn_server_ice(column: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, server: &ServerView, chair: Side, encountered: Option<InstallId>, lit: &Lit, size: FaceSize) {
    for ice in layout::ice_top_down(&server.ice, chair) {
        let title = ice.card.as_ref().and_then(|id| core.registry.get(id));
        // The title when it may be named and the strength when
        // rezzed; an unrezzed bar is told by its dim border
        // and text, not a suffix that would wrap the bar.
        let label = match (ice.rezzed, title) {
            (true, Some(card)) => format!("{}  {}", card.title, card.strength.map_or(String::new(), |s| s.to_string())),
            (false, Some(card)) => card.title.clone(),
            (_, None) => "ICE".to_string(),
        };
        let colour = if ice.rezzed { theme.faction(title.and_then(|c| c.faction)) } else { theme.corp.with_alpha(0.5) };
        let text_colour = if ice.rezzed { theme.text } else { theme.text_dim };
        let mut bar = column.spawn((
            Button,
            widgets::Themed,
            Click::Target(Target::Install(ice.install_id)),
            Node {
                width: px(size.width() + 4.0),
                height: px(layout::ICE_BAR - 4.0),
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
        if encountered == Some(ice.install_id) || lit.installs.contains(&ice.install_id) {
            bar.insert(outline(theme));
        }
    }
}

/// The cards in a server's root, each with its chips beneath.
fn spawn_server_root(column: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, server: &ServerView, lit: &Lit, size: FaceSize) {
    for card in &server.root {
        let marker = (Button, Click::Target(Target::Install(card.install_id)));
        let entity = match card.card.as_ref().and_then(|id| core.registry.get(id)) {
            Some(def) => {
                let image = def.numeric_id.and_then(|code| images.face(code));
                spawn_face(column, theme, &Face::of(def), size, image, marker)
            }
            None => spawn_back(column, theme, images.back(Side::Corp), Side::Corp, size, marker),
        };
        if lit.installs.contains(&card.install_id) {
            column.commands().entity(entity).insert(outline(theme));
        }
        let mut chips = Vec::new();
        if card.advancement_tokens > 0 {
            chips.push(format!("{} adv", card.advancement_tokens));
        }
        if let Some(counters) = card.counters.filter(|n| *n > 0) {
            chips.push(format!("{counters} ctr"));
        }
        if !card.rezzed && card.card.is_some() {
            chips.push("unrezzed".to_string());
        }
        if !chips.is_empty() {
            column.spawn(widgets::dim(theme, chips.join(" · ")));
        }
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
fn spawn_hand(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, view: &ClientView, side: Side, lit: &Lit, fit: &BoardFit) {
    let hand = match side {
        Side::Corp => view.corp.hq_cards.as_deref(),
        Side::Runner => view.runner.grip_cards.as_deref(),
    }
    .unwrap_or(&[]);
    let size = fit.size();
    let available = beside_strip(fit);
    parent.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() },)).with_children(|column| {
        section_label(column, theme, format!("Your hand · {}", hand.len()));
        column.spawn(card_row()).with_children(|row| {
            // A hand is a multiset; the first copy of a lit card is the one
            // outlined, which is as much as a highlight can say.
            let mut lit_left = lit.hand.clone();
            let mut faces = Vec::new();
            for id in hand {
                let Some(def) = core.registry.get(id) else { continue };
                let image = def.numeric_id.and_then(|code| images.face(code));
                let entity = spawn_face(row, theme, &Face::of(def), size, image, (Button, Click::Target(Target::HandCard(id.clone()))));
                if let Some(at) = lit_left.iter().position(|c| c == id) {
                    lit_left.swap_remove(at);
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
            parent.spawn((widgets::dim(theme, "Your turn: click a card or a zone, or use the bar above."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
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
fn entry_button(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, index: usize) {
    let Some(entry) = game.actions.entries.get(index) else { return };
    let mut button = parent.spawn(widgets::button(theme, entry.label.clone(), percent(100), Click::Entry(index)));
    button.entry::<Node>().and_modify(|mut node| {
        node.justify_content = JustifyContent::FlexStart;
        node.padding = UiRect::axes(px(10), px(6));
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

// ---- the overlays ----

fn spawn_overlay(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game) {
    // The card read over a zone sheet is wider than a sheet's panel;
    // the sheets are wider than the prompts.
    let width = if game.finished() || game.confirm_quit || game.options_open {
        px(560)
    } else if game.inspecting.is_some() || game.sheet.as_ref().is_some_and(|s| game.card_of(&s.target).is_some()) {
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
                } else if let Some(id) = &game.inspecting {
                    // A card read out of a pile: the face and its text,
                    // nothing to do with it from here.
                    card_sheet(panel, theme, core, images, game, id, &[]);
                } else if let Some(sheet) = &game.sheet {
                    match game.card_of(&sheet.target) {
                        Some(id) => card_sheet(panel, theme, core, images, game, &id, &sheet.entries),
                        None => zone_sheet(panel, theme, core, images, game, &sheet.target, &sheet.entries),
                    }
                }
            });
        });
}

/// The card large, its text beside it, and its legal actions under the
/// text — the deliberate second click. With no entries it is the
/// inspector.
fn card_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, id: &CardId, entries: &[usize]) {
    let Some(def) = core.registry.get(id) else {
        panel.spawn(widgets::dim(theme, format!("{} is not in the registry", id.0)));
        panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
        return;
    };
    panel.spawn(widgets::heading(theme, def.title.clone()));
    panel.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(16), align_items: AlignItems::FlexStart, ..default() },)).with_children(|row| {
        let image = def.numeric_id.and_then(|code| images.face(code));
        spawn_face(row, theme, &Face::of(def), FaceSize::Large, image, ());
        row.spawn((Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },)).with_children(|column| {
            column.spawn((Text::new(Face::of(def).body_text(false)), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            if entries.is_empty() {
                column.spawn(widgets::dim(theme, if game.awaiting { "Nothing to do with this card right now." } else { "Not your decision right now." }));
            } else {
                column.spawn(widgets::label(theme, "Actions"));
                for index in entries {
                    entry_button(column, theme, game, *index);
                }
            }
        });
    });
    panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
}

/// A zone: its actions, then what is in it as far as the viewer may
/// see. Every visible card is a button that reads it over the sheet.
fn zone_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, target: &Target, entries: &[usize]) {
    let Some(view) = &game.view else { return };
    panel.spawn(widgets::heading(theme, target_title(game, target)));
    if entries.is_empty() {
        panel.spawn(widgets::dim(theme, if game.awaiting { "Nothing to do here right now." } else { "Not your decision right now." }));
    } else {
        // The install entries name the card and the server both; the row
        // wraps when a hand of installs is offered to one remote.
        panel.spawn(wrap_row()).with_children(|row| {
            for index in entries {
                if let Some(entry) = game.actions.entries.get(*index) {
                    row.spawn(widgets::button(theme, entry.label.clone(), Val::Auto, Click::Entry(*index)));
                }
            }
        });
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

fn target_title(game: &Game, target: &Target) -> String {
    let title = |id: &CardId| game.registry().get(id).map_or_else(|| id.0.clone(), |c| c.title.clone());
    match target {
        Target::HandCard(card) => title(card),
        Target::Install(id) => game.card_at(*id).map_or_else(|| "This card".to_string(), |card| title(&card)),
        Target::Server(server) => server_name(*server),
        Target::Identity(side) => format!("{side:?} identity"),
        Target::Position(position) => format!("Card {position}"),
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
