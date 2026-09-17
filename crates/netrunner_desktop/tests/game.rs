//! A game, without a window: the form starts a match against the bottom
//! rung on a thread, the board draws the first decision, a press on a
//! decision button submits it, a card or a zone opens its actions as a
//! menu above it and never acts, a secondary click opens its sheet to read,
//! the control bar greys what is not legal, the gear opens the options,
//! and Escape asks before leaving.
//!
//! Driven under `MinimalPlugins` like `tests/navigation.rs`; the match
//! thread is real, so the test pumps `update` until the message it
//! waits for has arrived, with a bound so a broken thread fails rather
//! than hangs.

use std::time::{Duration, Instant};

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseButtonInput;
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use netrunner_client::board::{Control, Pile, Target};
use netrunner_client::start::{Level, StartChoice, DEFAULT_CORP_DECK, DEFAULT_RUNNER_DECK};
use netrunner_core::rules::{GamePhase, PlayerAction, ServerId, Side};
use netrunner_desktop::core::ClientCore;
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::screens::game::{ActionsMenu, Click, DecisionPopup, HudPanel, HudReadout, InstallFact, ScoreDetails, ScoreRow, LogRow, Model, Overlay, RunLane, ServerColumn};
use netrunner_core::rules::InstallId;
use netrunner_desktop::widgets::card_face::BodyText;
use netrunner_desktop::screens::new_game::{self, ActiveMatch, LastGame};
use netrunner_desktop::screens::settings::Control as SettingsControl;
use netrunner_desktop::widgets::Disabled;
use netrunner_desktop::{AppScreen, NetrunnerDesktopPlugins};

fn headless_client() -> (App, std::path::PathBuf) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("netrunner_desktop_game_{}_{n}", std::process::id()));
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin));
    let mut core = ClientCore::in_dir(dir.clone());
    // No pause between a run's beats: the tests wait on the model with a
    // five-second bound, and a paced run is a person's to watch.
    core.settings.desktop.animation_speed = 0.0;
    app.insert_resource(core);
    app.add_plugins(NetrunnerDesktopPlugins);
    app.update();
    app.update();
    (app, dir)
}

fn press(app: &mut App, key_code: KeyCode, logical_key: Key) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput { key_code, logical_key: logical_key.clone(), state, text: None, repeat: false, window: Entity::PLACEHOLDER });
    }
}

/// A mouse button down and up, a frame each: `ButtonInput` reports the
/// press for the one frame after the message.
fn click(app: &mut App, button: MouseButton) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(MouseButtonInput { button, state, window: Entity::PLACEHOLDER });
        app.update();
    }
    app.update();
}

fn menus(app: &mut App) -> usize {
    app.world_mut().query::<&ActionsMenu>().iter(app.world()).count()
}

fn screen(app: &App) -> AppScreen {
    *app.world().resource::<State<AppScreen>>().get()
}

/// Pumps frames until `done`, or fails after five seconds.
fn wait_for(app: &mut App, what: &str, mut done: impl FnMut(&mut App) -> bool) {
    let start = Instant::now();
    while !done(app) {
        assert!(start.elapsed() < Duration::from_secs(5), "waited five seconds for {what}");
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    }
}

fn button_labelled(app: &mut App, label: &str) -> Option<Entity> {
    let mut buttons = app.world_mut().query::<(Entity, &Click, &Children)>();
    let mut texts = app.world_mut().query::<&Text>();
    buttons.iter(app.world()).find(|(_, _, children)| children.iter().any(|child| texts.get(app.world(), child).is_ok_and(|text| text.0 == label))).map(|(e, _, _)| e)
}

fn click_entry_count(app: &mut App) -> usize {
    app.world_mut().query::<&Click>().iter(app.world()).filter(|c| matches!(c, Click::Entry(_))).count()
}

fn press_entity(app: &mut App, entity: Entity) {
    app.world_mut().entity_mut(entity).insert(Interaction::Pressed);
    app.update();
    app.update();
}

fn overlays(app: &mut App) -> usize {
    app.world_mut().query::<&Overlay>().iter(app.world()).count()
}

fn texts(app: &mut App) -> Vec<String> {
    app.world_mut().query::<&Text>().iter(app.world()).map(|t| t.0.clone()).collect()
}

fn entity_with(app: &mut App, wanted: &Click) -> Option<Entity> {
    let mut q = app.world_mut().query::<(Entity, &Click)>();
    q.iter(app.world()).find(|(_, c)| *c == wanted).map(|(e, _)| e)
}

/// The control bar's button for `control`, and whether it is greyed.
fn control_button(app: &mut App, control: Control) -> (Entity, bool) {
    let entity = entity_with(app, &Click::Control(control)).expect("every control is on the bar");
    let disabled = app.world().entity(entity).contains::<Disabled>();
    (entity, disabled)
}

/// Keeps the hand and waits for the Runner's own action phase, passing
/// priority through the bar whenever the Corp's turn asks for it.
fn to_the_runners_turn(app: &mut App) {
    wait_for(app, "the first decision", |app| click_entry_count(app) > 0);
    let keep = button_labelled(app, "Keep hand").expect("Keep hand is a decision");
    press_entity(app, keep);
    until_the_runners_turn(app);
}

/// Waits for the Runner's own action phase, passing priority whenever
/// asked to during the Corp's turn.
fn until_the_runners_turn(app: &mut App) {
    wait_for(app, "the Runner's turn", |app| {
        let model = &app.world().resource::<Model>().0;
        if !model.awaiting {
            return false;
        }
        let own_turn = model.view.as_ref().is_some_and(|v| matches!(v.phase, GamePhase::Action(Side::Runner)) && v.paid_ability_window.is_none());
        if own_turn {
            return true;
        }
        let (pass, disabled) = control_button(app, Control::PassPriority);
        assert!(!disabled, "asked during the Corp's turn, the Runner can pass priority");
        press_entity(app, pass);
        false
    });
}

/// Play vs Computer → Start (Novice, unrated) → the board, as the Runner.
fn start_a_game(app: &mut App) {
    start_a_game_as(app, Side::Runner);
}

fn start_a_game_as(app: &mut App, human: Side) {
    app.world_mut().write_message(Navigate(AppScreen::NewGame));
    app.update();
    app.update();
    assert_eq!(screen(app), AppScreen::NewGame);
    // The form's choice, made directly rather than through five drop-downs:
    // the bottom rung so the bot answers at once, unrated so no file is
    // written. `start` is the same function the Start button calls.
    let choice = StartChoice { human, level: Level::Novice, style: None, corp_deck: DEFAULT_CORP_DECK.to_string(), runner_deck: DEFAULT_RUNNER_DECK.to_string(), rated: false };
    let active = new_game::start(app.world().resource::<ClientCore>(), &choice).expect("the default decks start a game");
    app.world_mut().insert_resource(active);
    app.world_mut().write_message(Navigate(AppScreen::Game));
    app.update();
    app.update();
    assert_eq!(screen(app), AppScreen::Game);
}

#[test]
fn the_form_starts_a_game_the_board_offers_its_actions_and_a_press_submits_one() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    // The Corp mulligans first (a bot, instant), then the Runner's own
    // decision arrives and the panel fills.
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    let entries = click_entry_count(&mut app);
    let model = app.world().resource::<Model>();
    assert!(model.0.awaiting);
    // The helper is off, so the buttons are the prompt's decisions —
    // keep and mulligan — in the centred pop-up, not a flat panel.
    assert_eq!(entries, model.0.actions.decisions().len(), "one button per decision");
    assert_eq!(entries, 2);
    assert!(model.0.prompt.as_ref().is_some_and(|p| p.title.contains("mulligan")), "{:?}", model.0.prompt);
    let before = model.0.applied;
    assert_eq!(app.world_mut().query::<&DecisionPopup>().iter(app.world()).count(), 1, "the decision is a pop-up");
    assert!(texts(&mut app).iter().any(|t| t.contains("Keep this hand")), "headed by the prompt's words");
    let keep = button_labelled(&mut app, "Keep hand").expect("Keep hand is on the panel");
    app.world_mut().entity_mut(keep).insert(Interaction::Pressed);
    app.update();
    assert!(!app.world().resource::<Model>().0.awaiting, "the press closed the panel");
    wait_for(&mut app, "the kept hand to be applied", |app| app.world().resource::<Model>().0.applied > before);
    let log = &app.world().resource::<Model>().0.log;
    assert!(log.iter().any(|line| line.contains("Keep hand")), "{log:?}");
    // The board drew a hand of the Runner's own cards.
    let hand_faces = app.world_mut().query::<&Click>().iter(app.world()).filter(|c| matches!(c, Click::Target(netrunner_client::board::Target::HandCard(_)))).count();
    assert!(hand_faces >= 5, "{hand_faces} hand cards drawn");
    // And the next decision comes round.
    wait_for(&mut app, "the Runner's turn", |app| app.world().resource::<Model>().0.awaiting);
}

/// A mouse button with Ctrl held, down and up, then Ctrl released.
fn ctrl_click(app: &mut App, button: MouseButton) {
    app.world_mut().write_message(KeyboardInput { key_code: KeyCode::ControlLeft, logical_key: Key::Control, state: ButtonState::Pressed, text: None, repeat: false, window: Entity::PLACEHOLDER });
    click(app, button);
    app.world_mut().write_message(KeyboardInput { key_code: KeyCode::ControlLeft, logical_key: Key::Control, state: ButtonState::Released, text: None, repeat: false, window: Entity::PLACEHOLDER });
    app.update();
}

/// A secondary click on `entity`: `Interaction` never reports the right
/// button, so the node is marked hovered and the button is read from the
/// input resource, as the pointer would leave it.
fn right_click(app: &mut App, entity: Entity) {
    app.world_mut().entity_mut(entity).insert(Interaction::Hovered);
    click(app, MouseButton::Right);
    app.world_mut().entity_mut(entity).insert(Interaction::None);
    app.update();
}

fn escape(app: &mut App) {
    press(app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
}

/// A hand card opens its menu whether or not it has an action, and a
/// press on the menu's button is what submits — never the card click.
/// The first game played by hand had every card click doing nothing
/// (a face is a `Button` the shared feedback system does not report),
/// and the second had a click to examine a card install it.
#[test]
fn pressing_a_hand_card_opens_its_menu_and_the_menu_submits() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    // At the mulligan no hand card has an action: the menu says so.
    let face = {
        let mut q = app.world_mut().query::<(Entity, &Click)>();
        q.iter(app.world()).find(|(_, c)| matches!(c, Click::Target(Target::HandCard(_)))).map(|(e, _)| e).expect("a hand card")
    };
    press_entity(&mut app, face);
    let model = app.world().resource::<Model>();
    assert!(model.0.menu.as_ref().is_some_and(|m| m.entries.is_empty()), "the card's menu is open with nothing to do");
    assert!(model.0.sheet.is_none(), "and nothing to read opened");
    assert_eq!(menus(&mut app), 1);
    assert_eq!(overlays(&mut app), 0);
    assert!(texts(&mut app).iter().any(|t| t.contains("Nothing to do")));
    escape(&mut app);
    assert_eq!(menus(&mut app), 0, "Escape closes it");
    assert_eq!(screen(&app), AppScreen::Game);
    app.world_mut().entity_mut(face).insert(Interaction::None);
    // On the Runner's turn a card with an action still only opens.
    to_the_runners_turn(&mut app);
    assert_eq!(app.world_mut().query::<&DecisionPopup>().iter(app.world()).count(), 0, "an ordinary action phase asks nothing");
    let (card, entries) = {
        let model = &app.world().resource::<Model>().0;
        let hand = model.view.as_ref().unwrap().runner.grip_cards.clone().unwrap();
        hand.iter().map(|c| (c.clone(), model.actions.for_hand_card(c))).find(|(_, e)| !e.is_empty()).expect("an opening Runner hand has something playable")
    };
    let face = entity_with(&mut app, &Click::Target(Target::HandCard(card.clone()))).expect("the card is on the board");
    let before = app.world().resource::<Model>().0.applied;
    press_entity(&mut app, face);
    let model = &app.world().resource::<Model>().0;
    assert!(model.awaiting && model.applied == before, "the click sent nothing");
    let menu = model.menu.clone().expect("the menu is open");
    assert_eq!(menu.target, Target::HandCard(card.clone()));
    assert_eq!(menu.entries, entries);
    assert_eq!(click_entry_count(&mut app), entries.len(), "one button per entry, and the helper is off");
    let button = entity_with(&mut app, &Click::Entry(entries[0])).expect("the menu lists the card's action");
    press_entity(&mut app, button);
    assert_eq!(menus(&mut app), 0, "the menu's button submitted and closed the menu");
    wait_for(&mut app, "the action to be applied", |app| app.world().resource::<Model>().0.applied > before);
}

/// A secondary click — the right button, or Ctrl with the primary — on a
/// card opens its sheet to read: the card, and no action on it. It closes
/// a menu that was open, Escape closes it, nothing opens through it, and a
/// primary click on nothing of a menu's closes the menu.
#[test]
fn a_secondary_click_opens_the_card_to_read_and_offers_nothing() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let card = {
        let model = &app.world().resource::<Model>().0;
        let hand = model.view.as_ref().unwrap().runner.grip_cards.clone().unwrap();
        hand.into_iter().find(|c| !model.actions.for_hand_card(c).is_empty()).expect("an opening Runner hand has something playable")
    };
    let face = entity_with(&mut app, &Click::Target(Target::HandCard(card.clone()))).expect("the card is on the board");
    let before = app.world().resource::<Model>().0.applied;
    // Open the menu, then read the card: the sheet replaces the menu.
    press_entity(&mut app, face);
    app.world_mut().entity_mut(face).insert(Interaction::None);
    assert_eq!(menus(&mut app), 1);
    right_click(&mut app, face);
    let model = &app.world().resource::<Model>().0;
    assert!(model.awaiting && model.applied == before, "the click sent nothing");
    assert_eq!(model.sheet.as_ref().map(|s| s.target.clone()), Some(Target::HandCard(card.clone())));
    assert!(model.menu.is_none(), "reading closed the menu");
    assert_eq!(overlays(&mut app), 1);
    assert_eq!(menus(&mut app), 0);
    assert_eq!(click_entry_count(&mut app), 0, "the sheet offers no action");
    assert!(!texts(&mut app).iter().any(|t| t == "Actions"));
    // Through the sheet, neither click opens anything.
    right_click(&mut app, face);
    press_entity(&mut app, face);
    app.world_mut().entity_mut(face).insert(Interaction::None);
    assert_eq!(menus(&mut app), 0, "nothing opens through the sheet");
    escape(&mut app);
    assert_eq!(overlays(&mut app), 0, "Escape closes the sheet");
    assert!(!app.world().resource::<Model>().0.confirm_quit, "and does not ask to quit");
    // Ctrl with the primary button is the same click: the card
    // registers the press too, and opens no menu for it.
    app.world_mut().entity_mut(face).insert(Interaction::Pressed);
    ctrl_click(&mut app, MouseButton::Left);
    app.world_mut().entity_mut(face).insert(Interaction::None);
    app.update();
    assert_eq!(overlays(&mut app), 1, "Ctrl and the primary button open the sheet");
    assert_eq!(menus(&mut app), 0, "and no menu");
    escape(&mut app);
    // A primary click that presses no part of a menu closes it.
    press_entity(&mut app, face);
    app.world_mut().entity_mut(face).insert(Interaction::None);
    assert_eq!(menus(&mut app), 1);
    click(&mut app, MouseButton::Left);
    assert_eq!(menus(&mut app), 0, "a click elsewhere closes the menu");
    assert_eq!(app.world().resource::<Model>().0.applied, before);
}

/// The bar has every control of the side, greyed until the engine lists
/// it: End turn at the mulligan is drawn and dead, on the Runner's turn
/// it is live and a press ends the turn.
#[test]
fn the_control_bar_greys_what_is_not_legal_and_submits_what_is() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    for control in Control::for_side(Side::Runner) {
        let (_, disabled) = control_button(&mut app, *control);
        assert!(disabled, "{control:?} is greyed at the mulligan");
    }
    assert!(entity_with(&mut app, &Click::Control(Control::PurgeViruses)).is_none(), "the Corp's control is not on the Runner's bar");
    to_the_runners_turn(&mut app);
    let (end_turn, disabled) = control_button(&mut app, Control::EndTurn);
    assert!(!disabled, "End turn is live on the Runner's turn");
    let (_, jack_out) = control_button(&mut app, Control::JackOut);
    assert!(jack_out, "Jack out is greyed outside a run");
    let before = app.world().resource::<Model>().0.applied;
    press_entity(&mut app, end_turn);
    // Not `!awaiting`: at the bottom rung the Corp's next decision, and
    // the Runner's priority window in it, can arrive within the frame.
    wait_for(&mut app, "the turn to end", |app| app.world().resource::<Model>().0.applied > before);
    // The phase is still the Runner's action phase for a moment — an
    // end-of-turn paid-ability window opens before it moves — so the
    // log, not the phase, says the turn ended.
    let log = &app.world().resource::<Model>().0.log;
    assert!(log.iter().any(|line| line.contains("Runner: End turn")), "{log:?}");
}

/// Every `Click` on the screen in tree order — parents before children,
/// siblings in the order they were spawned — which is the order a row
/// draws left to right and a column top to bottom.
fn clicks_in_tree_order(app: &mut App) -> Vec<Click> {
    let world = app.world_mut();
    let roots: Vec<Entity> = world.query_filtered::<Entity, (With<Node>, Without<ChildOf>)>().iter(world).collect();
    let mut stack: Vec<Entity> = roots.into_iter().rev().collect();
    let mut clicks = Vec::new();
    while let Some(entity) = stack.pop() {
        if let Some(click) = world.get::<Click>(entity) {
            clicks.push(click.clone());
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter().rev());
        }
    }
    clicks
}

/// The board is the table seen from the chair: the Corp reads their own
/// servers Archives, R&D, HQ, then the remotes, left to right; the
/// Runner sees them across the table, mirrored.
#[test]
fn the_servers_are_the_table_seen_from_the_chair() {
    let servers = |app: &mut App| -> Vec<ServerId> {
        clicks_in_tree_order(app).into_iter().filter_map(|c| if let Click::Target(Target::Server(s)) = c { Some(s) } else { None }).collect()
    };
    let (mut app, _dir) = headless_client();
    start_a_game_as(&mut app, Side::Corp);
    wait_for(&mut app, "the Corp's first decision", |app| click_entry_count(app) > 0);
    assert_eq!(servers(&mut app), [ServerId::Archives, ServerId::RnD, ServerId::Hq], "the Corp's chair");

    let (mut app, _dir) = headless_client();
    start_a_game_as(&mut app, Side::Runner);
    wait_for(&mut app, "the Runner's first decision", |app| click_entry_count(app) > 0);
    assert_eq!(servers(&mut app), [ServerId::Hq, ServerId::RnD, ServerId::Archives], "the Runner's chair, across the table");
}

/// Both sides have a HUD on the board, each holding its side's readouts
/// in `hud::readouts`' order with the view's numbers — the Runner's
/// Tags and Damage there at zero, so nothing moves when the first tag
/// lands.
#[test]
fn each_side_has_a_hud_with_every_readout_in_its_place() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    let view = app.world().resource::<Model>().0.view.clone().expect("a view has arrived");
    let world = app.world_mut();
    let panels: Vec<(Entity, Side)> = world.query::<(Entity, &HudPanel)>().iter(world).map(|(e, p)| (e, p.0)).collect();
    assert_eq!(panels.len(), 2, "one HUD a side");
    for (panel, side) in panels {
        let children: Vec<Entity> = world.entity(panel).get::<Children>().expect("a HUD has readouts").iter().collect();
        let drawn: Vec<(&str, String)> = children.iter().filter_map(|c| world.entity(*c).get::<HudReadout>()).map(|r| (r.label, r.value.clone())).collect();
        let expected: Vec<(&str, String)> = netrunner_client::board::hud::readouts(&view, side).into_iter().map(|r| (r.label, r.value)).collect();
        assert_eq!(drawn, expected, "{side:?}'s HUD");
    }
}

/// The Agendas readout is a button that opens the side's score area as
/// a list; a press on a row opens its details in place, a press on
/// another moves them, and a second press on the open row closes them.
#[test]
fn the_agendas_readout_opens_the_score_area_and_a_row_expands() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    // Put two agendas in the Corp's score area: no game this short
    // scores one, and the sheet reads the view it is opened over.
    let registry = netrunner_client::decks::sample_deck_registry();
    let mut agendas = registry.iter().filter(|c| c.agenda_points.is_some()).map(|c| c.id.clone());
    let (first, second) = (agendas.next().unwrap(), agendas.next().unwrap());
    {
        let mut model = app.world_mut().resource_mut::<Model>();
        let view = model.0.view.as_mut().unwrap();
        for (n, card) in [first, second].into_iter().enumerate() {
            view.corp.scored_agendas.push(netrunner_core::rules::ScoredAgenda { card, install_id: InstallId(9000 + n as u32), agenda_counters: 0 });
        }
    }
    let readout = entity_with(&mut app, &Click::Target(Target::Pile(Pile::Agendas(Side::Corp)))).expect("the Corp's Agendas readout is a button");
    assert_eq!(app.world().entity(readout).get::<HudReadout>().map(|r| r.label), Some("Agendas"));
    press_entity(&mut app, readout);
    assert_eq!(overlays(&mut app), 1, "the score area opened");
    assert!(texts(&mut app).iter().any(|t| t == "Agendas scored"), "headed by the pile's name");
    let rows = |app: &mut App| app.world_mut().query::<&ScoreRow>().iter(app.world()).count();
    let open = |app: &mut App| app.world_mut().query::<&ScoreDetails>().iter(app.world()).map(|d| d.0).collect::<Vec<_>>();
    assert_eq!(rows(&mut app), 2, "a row per agenda");
    assert!(open(&mut app).is_empty(), "every row starts closed");
    let row = |app: &mut App, n: usize| entity_with(app, &Click::Expand(n)).expect("each row is a button");
    let first_row = row(&mut app, 0);
    press_entity(&mut app, first_row);
    assert_eq!(open(&mut app), [0]);
    let second_row = row(&mut app, 1);
    press_entity(&mut app, second_row);
    assert_eq!(open(&mut app), [1], "one row open at a time");
    let second_row = row(&mut app, 1);
    press_entity(&mut app, second_row);
    assert!(open(&mut app).is_empty(), "the open row's second press closes it");
}

/// A run is a trail in the lane between the two areas: the Runner runs
/// R&D from its sheet, and the lane shows one chip per ice in the run's
/// order, each a click on its install; the run ends and the lane keeps
/// the trail with its outcome; a root card is a tile, never a face.
#[test]
fn a_run_fills_the_lane_and_the_lane_keeps_the_trail() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let rnd = entity_with(&mut app, &Click::Target(Target::Server(ServerId::RnD))).expect("R&D's header");
    press_entity(&mut app, rnd);
    let run = {
        let model = &app.world().resource::<Model>().0;
        let menu = model.menu.clone().expect("the zone's menu is open");
        *menu.entries.iter().find(|i| matches!(model.actions.entries[**i].action, PlayerAction::InitiateRun { server: ServerId::RnD })).expect("R&D offers the run")
    };
    let button = entity_with(&mut app, &Click::Entry(run)).expect("the run's button");
    press_entity(&mut app, button);
    wait_for(&mut app, "the run to be on the board", |app| app.world().resource::<Model>().0.trail.is_some());
    // The lane after the redraw: the server chip, and the ice chips in
    // the trail's order, each a click on its install.
    app.update();
    app.update();
    let world = app.world_mut();
    assert_eq!(world.query::<&RunLane>().iter(world).count(), 1);
    let trail = world.resource::<Model>().0.trail.clone().unwrap();
    assert_eq!(trail.server, ServerId::RnD);
    let lane = world.query_filtered::<Entity, With<RunLane>>().single(world).unwrap();
    let mut chips: Vec<Click> = Vec::new();
    let mut stack = vec![lane];
    while let Some(entity) = stack.pop() {
        if let Some(click) = world.get::<Click>(entity) {
            chips.push(click.clone());
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter().rev());
        }
    }
    let expected: Vec<Click> = trail.ice.iter().map(|step| Click::Target(Target::Install(step.install))).collect();
    assert_eq!(chips, expected, "one chip per ice, in the order the Runner meets them");
    // No root card is a face: a server column holds tiles only.
    let columns: Vec<Entity> = world.query_filtered::<Entity, With<ServerColumn>>().iter(world).collect();
    assert!(columns.len() >= 3, "the three centrals, and any remote the Corp made");
    let mut stack = columns;
    while let Some(entity) = stack.pop() {
        assert!(world.get::<BodyText>(entity).is_none(), "a card face inside a server column");
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    // The run plays out (the Runner's bot is not seated; the person is
    // asked to continue), and the trail stays with its outcome.
    for _ in 0..24 {
        // Continue, complete, answer a decision, or pass priority in a
        // window — whichever the engine lists, as a person would.
        let mut pressed = false;
        for control in [Control::ContinueRun, Control::CompleteRun] {
            let (entity, disabled) = control_button(&mut app, control);
            if !disabled && !pressed {
                press_entity(&mut app, entity);
                pressed = true;
            }
        }
        if !pressed {
            let decision = app.world().resource::<Model>().0.actions.decisions().first().copied();
            if let Some(index) = decision {
                let button = entity_with(&mut app, &Click::Entry(index)).expect("the decision's button");
                press_entity(&mut app, button);
                pressed = true;
            }
        }
        if !pressed {
            let (entity, disabled) = control_button(&mut app, Control::PassPriority);
            if !disabled {
                press_entity(&mut app, entity);
            }
        }
        wait_for(&mut app, "the next decision", |app| {
            let model = &app.world().resource::<Model>().0;
            model.awaiting || model.finished()
        });
        if app.world().resource::<Model>().0.view.as_ref().is_some_and(|v| v.active_run.is_none()) {
            break;
        }
    }
    let model = &app.world().resource::<Model>().0;
    assert!(model.view.as_ref().is_some_and(|v| v.active_run.is_none()), "the run on R&D is over within two dozen presses");
    let trail = model.trail.clone().expect("the trail lingers after the run");
    assert!(trail.ended(), "{trail:?}");
}

/// A zone click opens what may be done there and never does it; a
/// secondary click reads what is in it: R&D offers the run and shows no
/// order, Archives shows its pile, the stack offers the draw, and the
/// count does not move.
#[test]
fn a_zone_click_opens_its_menu_and_never_acts_and_its_sheet_shows_the_contents() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let before = app.world().resource::<Model>().0.applied;
    let rnd = entity_with(&mut app, &Click::Target(Target::Server(ServerId::RnD))).expect("R&D's header");
    press_entity(&mut app, rnd);
    app.world_mut().entity_mut(rnd).insert(Interaction::None);
    let model = &app.world().resource::<Model>().0;
    assert!(model.awaiting && model.applied == before, "nothing was sent");
    let menu = model.menu.clone().expect("the zone's menu is open");
    assert!(menu.entries.iter().any(|i| matches!(model.actions.entries[*i].action, PlayerAction::InitiateRun { server: ServerId::RnD })), "R&D offers the run");
    assert!(texts(&mut app).iter().any(|t| t == "R&D"), "headed by the zone");
    escape(&mut app);
    right_click(&mut app, rnd);
    assert_eq!(overlays(&mut app), 1, "the zone's sheet is up");
    assert!(texts(&mut app).iter().any(|t| t.contains("in an order nobody is shown")), "a deck's order is never shown");
    assert_eq!(click_entry_count(&mut app), 0, "and it offers no action");
    escape(&mut app);
    let archives = entity_with(&mut app, &Click::Target(Target::Server(ServerId::Archives))).expect("Archives' header");
    right_click(&mut app, archives);
    assert!(texts(&mut app).iter().any(|t| t == "Archives"), "the sheet is headed by the zone");
    assert_eq!(app.world().resource::<Model>().0.applied, before);
    escape(&mut app);
    // The Runner's own piles are buttons in the strip; the stack's menu
    // offers the draw.
    let stack = entity_with(&mut app, &Click::Target(Target::Pile(netrunner_client::board::Pile::Stack))).expect("the stack is a button");
    press_entity(&mut app, stack);
    let model = &app.world().resource::<Model>().0;
    assert!(model.menu.as_ref().unwrap().entries.iter().any(|i| matches!(model.actions.entries[*i].action, PlayerAction::DrawCardClick { .. })));
    assert_eq!(model.applied, before);
}

/// The gear opens the options; the play helper toggle is saved and puts
/// the flat panel on the rail, the play history toggle shows the log,
/// and Escape closes the options before it asks to quit.
#[test]
fn the_gear_opens_the_options_and_the_toggles_are_saved_and_drawn() {
    let (mut app, dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let legal = app.world().resource::<Model>().0.actions.entries.len();
    assert!(click_entry_count(&mut app) < legal, "with the helper off the rail is not the whole list");
    let log_display = |app: &mut App| app.world_mut().query::<(&Node, &LogRow)>().iter(app.world()).map(|(n, _)| n.display).next().expect("the log row exists");
    assert_eq!(log_display(&mut app), Display::None, "the history is off by default");
    let gear = entity_with(&mut app, &Click::Options).expect("the gear is in the top bar");
    press_entity(&mut app, gear);
    assert_eq!(overlays(&mut app), 1);
    assert!(texts(&mut app).iter().any(|t| t == "Game options"));
    let toggle_for = |app: &mut App, row: netrunner_desktop::models::settings::Row| {
        let mut q = app.world_mut().query::<(Entity, &SettingsControl)>();
        q.iter(app.world())
            .find(|(_, c)| **c == SettingsControl::Intent(netrunner_desktop::models::settings::Intent::Toggle(row)))
            .map(|(e, _)| e)
            .expect("the row has a toggle")
    };
    let helper = toggle_for(&mut app, netrunner_desktop::models::settings::Row::PlayHelper);
    press_entity(&mut app, helper);
    assert!(app.world().resource::<ClientCore>().settings.desktop.play_helper);
    let saved = std::fs::read_to_string(dir.join("settings.json")).expect("the settings were saved");
    assert!(saved.contains("\"play_helper\": true"), "{saved}");
    assert_eq!(overlays(&mut app), 1, "the options stay open");
    let history = toggle_for(&mut app, netrunner_desktop::models::settings::Row::PlayHistory);
    press_entity(&mut app, history);
    assert_eq!(log_display(&mut app), Display::Flex, "the history is shown");
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(overlays(&mut app), 0, "Escape closes the options, not the game");
    assert!(!app.world().resource::<Model>().0.confirm_quit);
    assert_eq!(click_entry_count(&mut app), legal, "with the helper on every legal action is on the rail");
    assert!(texts(&mut app).iter().any(|t| t.contains("Keep hand")), "the log has the mulligan line");
}

#[test]
fn escape_asks_before_leaving_and_leaving_ends_the_match() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Game, "Escape does not leave outright");
    assert_eq!(app.world_mut().query::<&Overlay>().iter(app.world()).count(), 1, "the quit prompt is up");
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(app.world_mut().query::<&Overlay>().iter(app.world()).count(), 0, "a second Escape withdraws it");
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    // Two buttons read "Quit" now — the top bar's and the prompt's — so
    // the prompt's is found by what it does.
    let quit = {
        let mut q = app.world_mut().query::<(Entity, &Click)>();
        q.iter(app.world()).find(|(_, c)| **c == Click::ConfirmQuit).map(|(e, _)| e).expect("the prompt's Quit button")
    };
    app.world_mut().entity_mut(quit).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
    assert!(!app.world().contains_resource::<ActiveMatch>(), "the match went with the screen");
    assert!(!app.world().contains_resource::<Model>());
    assert!(app.world().get_resource::<LastGame>().is_some_and(|last| last.0.human == Side::Runner), "the form will reopen on this game");
}

#[test]
fn the_board_without_a_match_says_so_and_goes_back() {
    let (mut app, _dir) = headless_client();
    app.world_mut().write_message(Navigate(AppScreen::Game));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Game);
    assert!(!app.world().contains_resource::<Model>());
    let back = button_labelled(&mut app, "Back").expect("a way back");
    app.world_mut().entity_mut(back).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
}

/// The form: five drop-downs over the shared state machine, and Start
/// through the button.
#[test]
fn the_form_has_a_drop_down_per_pane_and_start_opens_the_board() {
    use netrunner_desktop::screens::new_game::PaneDropdown;
    let (mut app, _dir) = headless_client();
    app.world_mut().write_message(Navigate(AppScreen::NewGame));
    app.update();
    app.update();
    assert_eq!(app.world_mut().query::<&PaneDropdown>().iter(app.world()).count(), 5);
    let start = {
        let mut buttons = app.world_mut().query::<(Entity, &Children)>();
        let mut texts = app.world_mut().query::<&Text>();
        buttons.iter(app.world()).find(|(_, children)| children.iter().any(|child| texts.get(app.world(), child).is_ok_and(|t| t.0 == "Start"))).map(|(e, _)| e).expect("a Start button")
    };
    app.world_mut().entity_mut(start).insert(Interaction::Pressed);
    wait_for(&mut app, "Start to open the board", |app| {
        let notice: Vec<String> = app.world_mut().query::<&Text>().iter(app.world()).map(|t| t.0.clone()).filter(|t| t.contains("could not")).collect();
        assert!(notice.is_empty(), "{notice:?}");
        screen(app) == AppScreen::Game
    });
    assert!(app.world().contains_resource::<ActiveMatch>());
    // Operator, the default suggestion, answers within the bound too.
    wait_for(&mut app, "the first decision at the suggested rung", |app| click_entry_count(app) > 0);
}

/// A tile says whether its card is rezzed — or face down, from the
/// chair that cannot name it — and its sheet lists the card's state:
/// where it sits, rezzed or not, its tokens. Played from the Runner's
/// chair until the Corp has installed something.
#[test]
fn a_tile_says_its_rez_state_and_its_sheet_lists_the_facts() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let mut installs: Vec<InstallId> = Vec::new();
    for _ in 0..6 {
        until_the_runners_turn(&mut app);
        installs = app.world().resource::<Model>().0.view.as_ref().map_or(Vec::new(), |v| v.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).map(|c| c.install_id).collect());
        if !installs.is_empty() {
            break;
        }
        let (end, disabled) = control_button(&mut app, Control::EndTurn);
        assert!(!disabled);
        press_entity(&mut app, end);
        wait_for(&mut app, "the turn to end", |app| app.world().resource::<Model>().0.view.as_ref().is_some_and(|v| !matches!(v.phase, GamePhase::Action(Side::Runner)) || v.paid_ability_window.is_some()));
    }
    let id = *installs.first().expect("the Corp installed something within six turns");
    app.update();
    // The tile's words: a face-down card, or unrezzed ice, since the
    // Runner cannot name an unrezzed Corp card.
    let tile = entity_with(&mut app, &Click::Target(Target::Install(id))).expect("the install has a tile");
    let world = app.world_mut();
    let children = world.get::<Children>(tile).expect("a tile has a text").iter().collect::<Vec<_>>();
    let words: String = children.iter().filter_map(|c| world.get::<Text>(*c).map(|t| t.0.clone())).collect();
    let view = world.resource::<Model>().0.view.clone().unwrap();
    let card = view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).find(|c| c.install_id == id).unwrap().clone();
    let expected = if card.rezzed { "rezzed" } else if card.slot == netrunner_core::rules::InstallSlot::Ice { "unrezzed" } else { "face down" };
    assert!(words.contains(expected), "the tile reads {words:?}, expected {expected:?}");
    // Its sheet, on a secondary click: the state lines, then Close.
    right_click(&mut app, tile);
    let facts: Vec<String> = {
        let world = app.world_mut();
        world.query_filtered::<&Text, With<InstallFact>>().iter(world).map(|t| t.0.clone()).collect()
    };
    assert!(!facts.is_empty(), "the sheet lists the install's state");
    assert!(facts[0].starts_with("Ice protecting") || facts[0].starts_with("In the root of"), "{facts:?}");
    assert!(facts.iter().any(|l| l == "Rezzed" || l.starts_with("Unrezzed") || l.starts_with("Face down")), "{facts:?}");
    assert_eq!(overlays(&mut app), 1);
    assert_eq!(app.world().resource::<Model>().0.applied, app.world().resource::<Model>().0.applied, "nothing was submitted");
}
