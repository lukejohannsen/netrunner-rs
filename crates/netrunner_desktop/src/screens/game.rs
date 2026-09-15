//! The board: the human's masked view of the match, drawn; every legal
//! action on a panel; a click on a card or a server as a second route to
//! the same action; the match log; and the overlays — a popup of what a
//! click could mean, a card's text, the quit prompt, the end of the game.
//!
//! **Where the match is.** On its own thread, behind the [`ActiveMatch`]
//! the new-game form left: `poll` drains its messages once a frame into
//! `models::game::Game` intents, and a chosen entry goes back through
//! `MatchHandle::submit`, unfiltered. The screen never holds a
//! `GameState` and never re-derives legality; it draws the view and
//! hands back what the person chose from the engine's own list.
//!
//! **Layout.** The opponent's strip and area across the top, the
//! person's area, hand and strip across the bottom; the Corp's servers
//! are columns — ICE as bars above the root, the way ICE lies on a
//! table, since a rotated `UiTransform` is laid out as its unrotated box
//! and would overlap its neighbours — and the Runner's rig is three
//! labelled rows. The right rail is the prompt (`board::Prompt`, the
//! card's own words), the action panel, and the log. Everything under
//! the board root is respawned when the view moves, which is at most
//! once per applied action; the overlay and the rail are respawned on
//! their own, so a popup opening does not redraw the board.
//!
//! **Highlights come from `board::diff`.** After a redraw, every install
//! and hand card a `Transition` names is outlined for that redraw, and
//! nothing is ever inferred from the previous frame's nodes. §4 turns
//! the same transitions into movement and sound.

use bevy::prelude::*;

use netrunner_client::board::action_map::server_name;
use netrunner_client::board::{Target, Transition, Zone};
use netrunner_client::card_face::Face;
use netrunner_core::dsl::{CardId, CardType};
use netrunner_core::rules::{GamePhase, InstallId, InstallSlot, RunPhase, ServerId, Side};
use netrunner_core::view::{ClientView, ServerView};

use crate::card_images::CardImages;
use crate::core::{ClientCore, Notices};
use crate::models::game::{Game, Intent, MatchMessageRef, Outcome};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::screens::new_game::{ActiveMatch, LastGame};
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
            .add_systems(Update, (poll, autoplay, escape.in_set(Captures), controls, redraw).chain().run_if(in_state(AppScreen::Game)));
    }
}

/// What a button on the board means.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum Click {
    Target(Target),
    /// An entry of the action map.
    Entry(usize),
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
/// The prompt and the action panel, respawned when the view moves or a
/// submit closes it.
#[derive(Component)]
pub struct Rail;
/// The action panel's scroll column, inside the rail.
#[derive(Component)]
struct ActionList;
#[derive(Component)]
struct LogList;
#[derive(Component)]
struct LogScroll;
/// The full-window overlay, when one is up.
#[derive(Component)]
pub struct Overlay;
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

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, active: Option<Res<ActiveMatch>>) {
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

    let board = commands
        .spawn((
            Board,
            bevy::ui_widgets::ScrollArea,
            Node { flex_grow: 1.0, min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, row_gap: px(6), overflow: Overflow::scroll_y(), ..default() },
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
        .spawn((Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Row, column_gap: px(4), height: px(200), ..default() },))
        .add_child(log_scroll)
        .with_children(|parent| {
            parent.spawn(widgets::scrollbar(&theme, log_scroll));
        })
        .id();
    let rail_column = commands
        .spawn((Node { width: px(380), height: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },))
        .add_child(rail)
        .add_child(log_row)
        .id();
    let body = commands
        .spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(10), ..default() },))
        .add_child(board)
        .add_child(rail_column)
        .id();
    let top = commands
        .spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(16), ..default() },))
        .with_children(|parent| {
            parent.spawn(widgets::heading(&theme, AppScreen::Game.title()));
            parent.spawn((StatusLine, widgets::dim(&theme, "Setting up…")));
            let mut quit = parent.spawn(widgets::button(&theme, "Quit", Val::Auto, Click::Quit));
            quit.entry::<Node>().and_modify(|mut node| node.margin = UiRect::left(Val::Auto));
        })
        .id();
    let mut root = commands.spawn(screen_root(AppScreen::Game, theme.background));
    root.entry::<Node>().and_modify(|mut node| {
        node.align_items = AlignItems::Stretch;
        node.padding = UiRect::all(px(12));
        node.row_gap = px(8);
    });
    root.add_child(top).add_child(body);
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
    mut pending: ResMut<Pending>,
    marks: Query<&Click>,
    model: Option<ResMut<Model>>,
    active: Option<Res<ActiveMatch>>,
    mut dirty: ResMut<Dirty>,
    mut notices: ResMut<Notices>,
    mut navigate: MessageWriter<Navigate>,
) {
    let mut intents: Vec<Intent> = std::mem::take(&mut pending.0);
    let mut leave_to: Option<AppScreen> = None;
    for Pressed(entity) in pressed.read() {
        match marks.get(*entity) {
            Ok(Click::Target(target)) => intents.push(Intent::Click(target.clone())),
            Ok(Click::Entry(index)) => intents.push(Intent::Choose(*index)),
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
    log: Query<Entity, With<LogList>>,
    mut log_scroll: Query<&mut ScrollPosition, With<LogScroll>>,
    overlays: Query<Entity, With<Overlay>>,
    roots: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    mut status: Query<&mut Text, With<StatusLine>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    images: Res<CardImages>,
) {
    let Some(mut model) = model else { return };
    if !(dirty.board || dirty.rail || dirty.log || dirty.overlay) {
        return;
    }
    let Dirty { board: reboard, rail: rerail, log: relog, overlay: reoverlay } = std::mem::take(&mut *dirty);
    let game = &mut model.0;
    if reboard {
        let transitions = game.take_transitions();
        if let Ok(board) = board.single() {
            commands.entity(board).despawn_children().with_children(|parent| spawn_board(parent, &theme, &core, &images, game, &transitions));
        }
        for mut text in &mut status {
            text.0 = status_line(game);
        }
    }
    if rerail && let Ok(rail) = rail.single() {
        commands.entity(rail).despawn_children().with_children(|parent| spawn_rail(parent, &theme, game));
    }
    if relog && let Ok(log) = log.single() {
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
    if reoverlay {
        for overlay in &overlays {
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
    game.finished() || game.confirm_quit || game.popup.is_some() || game.inspecting.is_some()
}

// ---- the board ----

fn spawn_board(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, transitions: &[Transition]) {
    let Some(view) = &game.view else {
        parent.spawn(widgets::dim(theme, "Waiting for the match to start…"));
        return;
    };
    let human = game.side;
    let opponent = human.other();
    let lit = Lit::of(transitions);
    // Each side's hand sits beside its strip rather than under it: two
    // strips of their own took a third of the height and put the
    // person's hand below the fold.
    parent.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(16), ..default() },)).with_children(|row| {
        spawn_strip(row, theme, core, images, game, view, opponent);
        spawn_opponent_hand(row, theme, images, view, opponent);
    });
    spawn_area(parent, theme, core, images, game, view, opponent, &lit);
    spawn_area(parent, theme, core, images, game, view, human, &lit);
    parent.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexEnd, column_gap: px(16), ..default() },)).with_children(|row| {
        spawn_hand(row, theme, core, images, view, human, &lit);
        spawn_strip(row, theme, core, images, game, view, human);
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

/// How much of the identity's face a strip shows.
const IDENTITY_CROP: f32 = 72.0;

fn outline(theme: &Theme) -> Outline {
    Outline { width: px(3), offset: px(1), color: theme.accent }
}

fn section_label(parent: &mut ChildSpawnerCommands, theme: &Theme, text: impl Into<String>) {
    parent.spawn(widgets::dim(theme, text));
}

fn wrap_row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::FlexStart, row_gap: px(6), column_gap: px(6), ..default() }
}

/// A side's identity and numbers.
fn spawn_strip(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, side: Side) {
    let identity = match side {
        Side::Corp => view.corp.identity.clone(),
        Side::Runner => view.runner.identity.clone(),
    };
    let who = if side == game.side { "You" } else { "Opponent" };
    let colour = theme.side(side);
    parent
        .spawn((Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, align_items: AlignItems::Center, column_gap: px(12), padding: UiRect::all(px(6)), border: UiRect::left(px(4)), ..default() }, BorderColor::all(colour)))
        .with_children(|row| {
            if let Some(id) = &identity
                && let Some(card) = core.registry.get(id)
            {
                // The top of the face — its title and art — in a clipped
                // slot: a full identity in each strip was a third of the
                // board's height, and the strip's text names it anyway.
                // The click still opens it or its ability.
                let image = card.numeric_id.and_then(|code| images.face(code));
                row.spawn((Node { height: px(IDENTITY_CROP), overflow: Overflow::clip(), flex_shrink: 0.0, ..default() },)).with_children(|slot| {
                    spawn_face(slot, theme, &Face::of(card), FaceSize::Board, image, (Button, Click::Target(Target::Identity(side))));
                });
            }
            row.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() },)).with_children(|column| {
                let title = identity.as_ref().and_then(|id| core.registry.get(id)).map_or_else(|| format!("{side:?}"), |c| c.title.clone());
                column.spawn((Text::new(format!("{who} · {title}")), theme.font(size::BODY), TextColor(colour)));
                for line in strip_lines(view, side) {
                    column.spawn(widgets::dim(theme, line));
                }
            });
        });
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

/// The opponent's hand as backs — a count is in the strip, and a row of
/// backs is what a table shows.
fn spawn_opponent_hand(parent: &mut ChildSpawnerCommands, theme: &Theme, images: &CardImages, view: &ClientView, side: Side) {
    let count = match side {
        Side::Corp => view.corp.hq_count,
        Side::Runner => view.runner.grip_count,
    };
    parent.spawn((Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), flex_grow: 1.0, min_width: px(0), ..default() },)).with_children(|row| {
        for _ in 0..count.min(12) {
            let mut back = row.spawn(Node::default());
            back.with_children(|slot| {
                spawn_back(slot, theme, images.back(side), side, FaceSize::Board, ());
            });
            back.entry::<Node>().and_modify(|mut node| {
                node.height = px(FaceSize::Board.height() * 0.45);
                node.overflow = Overflow::clip();
            });
        }
    });
}

/// A side's board: the Corp's servers, or the Runner's rig.
#[allow(clippy::too_many_arguments)]
fn spawn_area(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, side: Side, lit: &Lit) {
    match side {
        Side::Corp => spawn_servers(parent, theme, core, images, game, view, lit),
        Side::Runner => spawn_rig(parent, theme, core, images, view, lit),
    }
}

fn spawn_servers(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, lit: &Lit) {
    // Every central is a column even when nothing is installed on it,
    // because a run on an empty central is a click on its header.
    let mut servers: Vec<ServerView> = view.corp.servers.clone();
    for central in [ServerId::Hq, ServerId::RnD, ServerId::Archives] {
        if !servers.iter().any(|s| s.server == central) {
            servers.push(ServerView { server: central, ice: Vec::new(), root: Vec::new() });
        }
    }
    servers.sort_by_key(|s| match s.server {
        ServerId::Hq => (0, 0),
        ServerId::RnD => (1, 0),
        ServerId::Archives => (2, 0),
        ServerId::Remote(n) => (3, n),
    });
    let run = view.active_run.as_ref();
    let encountered = run.filter(|r| matches!(r.phase, RunPhase::ApproachIce | RunPhase::EncounterIce)).and_then(|r| r.ice.get(r.position)).map(|i| i.install_id);
    section_label(parent, theme, "Servers");
    parent.spawn(wrap_row()).with_children(|row| {
        for server in &servers {
            let under_run = game.run_on(server.server);
            let border = if under_run { theme.accent } else { theme.panel_border };
            row.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(4),
                    padding: UiRect::all(px(4)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    min_width: px(FaceSize::Board.width() + 10.0),
                    ..default()
                },
                BackgroundColor(theme.panel),
                BorderColor::all(border),
            ))
            .with_children(|column| {
                let count = match server.server {
                    ServerId::Hq => format!(" · {}", view.corp.hq_count),
                    ServerId::RnD => format!(" · {}", view.corp.rd_count),
                    ServerId::Archives => format!(" · {}", view.corp.archives.len()),
                    ServerId::Remote(_) => String::new(),
                };
                // A compact button: seven columns have to fit across the
                // board, and the shared button's body-size text does not.
                column.spawn((
                    Button,
                    widgets::Themed,
                    Click::Target(Target::Server(server.server)),
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
                    children![(Text::new(format!("{}{count}", server_name(server.server))), theme.font(size::SMALL), TextColor(theme.text))],
                ));
                // ICE, outermost at the top: a bar with the title when it
                // can be named, its strength, and the run's marker.
                for ice in server.ice.iter().rev() {
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
                            width: px(FaceSize::Board.width() + 4.0),
                            height: px(26),
                            flex_shrink: 0.0,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
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
                for card in &server.root {
                    let marker = (Button, Click::Target(Target::Install(card.install_id)));
                    let entity = match card.card.as_ref().and_then(|id| core.registry.get(id)) {
                        Some(def) => {
                            let image = def.numeric_id.and_then(|code| images.face(code));
                            spawn_face(column, theme, &Face::of(def), FaceSize::Board, image, marker)
                        }
                        None => spawn_back(column, theme, images.back(Side::Corp), Side::Corp, FaceSize::Board, marker),
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
            });
        }
    });
}

fn spawn_rig(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, view: &ClientView, lit: &Lit) {
    let group = |kind: &CardType| match kind {
        CardType::Program => 0,
        CardType::Hardware => 1,
        _ => 2,
    };
    section_label(parent, theme, if view.runner.rig.is_empty() { "Rig · nothing installed" } else { "Rig" });
    parent.spawn(wrap_row()).with_children(|row| {
        for wanted in 0..3 {
            let cards: Vec<_> = view.runner.rig.iter().filter(|c| core.registry.get(&c.card).is_some_and(|def| group(&def.card_type) == wanted)).collect();
            if cards.is_empty() {
                continue;
            }
            row.spawn((Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() },)).with_children(|column| {
                column.spawn(widgets::dim(theme, ["Programs", "Hardware", "Resources"][wanted]));
                column.spawn(wrap_row()).with_children(|cards_row| {
                    for card in cards {
                        let Some(def) = core.registry.get(&card.card) else { continue };
                        let image = def.numeric_id.and_then(|code| images.face(code));
                        cards_row.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(2), ..default() },)).with_children(|slot| {
                            let entity = spawn_face(slot, theme, &Face::of(def), FaceSize::Board, image, (Button, Click::Target(Target::Install(card.install_id))));
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
}

fn spawn_hand(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, view: &ClientView, side: Side, lit: &Lit) {
    let hand = match side {
        Side::Corp => view.corp.hq_cards.as_deref(),
        Side::Runner => view.runner.grip_cards.as_deref(),
    }
    .unwrap_or(&[]);
    parent.spawn((Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() },)).with_children(|column| {
    section_label(column, theme, format!("Your hand · {}", hand.len()));
    column.spawn(wrap_row()).with_children(|row| {
        // A hand is a multiset; the first copy of a lit card is the one
        // outlined, which is as much as a highlight can say.
        let mut lit_left = lit.hand.clone();
        for id in hand {
            let Some(def) = core.registry.get(id) else { continue };
            let image = def.numeric_id.and_then(|code| images.face(code));
            let entity = spawn_face(row, theme, &Face::of(def), FaceSize::Board, image, (Button, Click::Target(Target::HandCard(id.clone()))));
            if let Some(at) = lit_left.iter().position(|c| c == id) {
                lit_left.swap_remove(at);
                row.commands().entity(entity).insert(outline(theme));
            }
        }
    });
    });
}

// ---- the rail ----

fn spawn_rail(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game) {
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
    parent.spawn(widgets::label(theme, "Your actions"));
    let list = parent
        .spawn((
            ActionList,
            bevy::ui_widgets::ScrollArea,
            Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(4), overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|list| {
            for (index, entry) in game.actions.entries.iter().enumerate() {
                let mut button = list.spawn(widgets::button(theme, entry.label.clone(), percent(100), Click::Entry(index)));
                button.entry::<Node>().and_modify(|mut node| {
                    node.justify_content = JustifyContent::FlexStart;
                    node.padding = UiRect::axes(px(10), px(6));
                });
            }
        })
        .id();
    parent.spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(list).with_children(|row| {
        row.spawn(widgets::scrollbar(theme, list));
    });
}

// ---- the overlays ----

fn spawn_overlay(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game) {
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
            screen.spawn(widgets::panel(theme, px(520))).with_children(|panel| {
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
                } else if let Some(popup) = &game.popup {
                    panel.spawn(widgets::heading(theme, target_title(game, &popup.target)));
                    for index in &popup.entries {
                        if let Some(entry) = game.actions.entries.get(*index) {
                            let mut button = panel.spawn(widgets::button(theme, entry.label.clone(), percent(100), Click::Entry(*index)));
                            button.entry::<Node>().and_modify(|mut node| node.justify_content = JustifyContent::FlexStart);
                        }
                    }
                    panel.spawn(widgets::button(theme, "Cancel", Val::Auto, Click::CloseOverlay));
                } else if let Some(id) = &game.inspecting {
                    match core.registry.get(id) {
                        Some(def) => {
                            let image = def.numeric_id.and_then(|code| images.face(code));
                            panel.spawn((Node { justify_content: JustifyContent::Center, ..default() },)).with_children(|centre| {
                                spawn_face(centre, theme, &Face::of(def), FaceSize::Large, image, ());
                            });
                            panel.spawn((Text::new(Face::of(def).body_text(false)), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                        None => {
                            panel.spawn(widgets::dim(theme, format!("{} is not in the registry", id.0)));
                        }
                    }
                    panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
                }
            });
        });
}

fn target_title(game: &Game, target: &Target) -> String {
    let title = |id: &CardId| game.registry().get(id).map_or_else(|| id.0.clone(), |c| c.title.clone());
    match target {
        Target::HandCard(card) => title(card),
        Target::Install(id) => game.card_at(*id).map_or_else(|| "This card".to_string(), |card| title(&card)),
        Target::Server(server) => server_name(*server),
        Target::Identity(side) => format!("{side:?} identity"),
        Target::Position(position) => format!("Card {position}"),
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
