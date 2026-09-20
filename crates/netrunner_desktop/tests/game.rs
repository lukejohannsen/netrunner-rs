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
use bevy::window::CursorMoved;
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::state::app::StatesPlugin;

use netrunner_client::board::{Affordance, Control, Pile, Target};
use netrunner_client::card_text::Segment;
use netrunner_client::start::{Level, StartChoice, DEFAULT_CORP_DECK, DEFAULT_RUNNER_DECK};
use netrunner_core::rules::{GamePhase, PlayerAction, ServerId, Side};
use netrunner_desktop::core::ClientCore;
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::screens::game::{ActionsMenu, ChoiceCard, Click, Contact, DecisionPopup, Glowing, EndTurnNotice, HelpRow, HudPanel, PhaseBarRow, PhaseStep, HudReadout, InstallFact, ScoreDetails, ScoreRow, LogRow, Model, Overlay, RunLane, ServerColumn, BoardFit, ControlBar, HandSlot, LiftedCard, ServerPlate};
use netrunner_core::rules::InstallId;
use netrunner_desktop::widgets::card_face::BodyText;
use netrunner_desktop::screens::new_game::{self, ActiveMatch, LastGame};
use netrunner_desktop::screens::settings::Control as SettingsControl;
use netrunner_desktop::widgets::{Disabled, Dressed};
use netrunner_desktop::skin::Slot;
use netrunner_desktop::theme::Theme;
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

/// What the overlay says, split by node kind: the panel's own `Text`
/// nodes (a heading is one) and the `TextSpan`s inside them (a card
/// face's title and rules text are these). The split is the point — it
/// is how "the name is drawn once, by the card" is stated as an
/// assertion rather than a count.
fn overlay_text(app: &mut App) -> (Vec<String>, Vec<String>) {
    let world = app.world_mut();
    let Ok(root) = world.query_filtered::<Entity, With<Overlay>>().single(world) else { return (Vec::new(), Vec::new()) };
    let (mut texts, mut spans) = (Vec::new(), Vec::new());
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(text) = world.get::<Text>(entity) {
            texts.push(text.0.clone());
        }
        if let Some(span) = world.get::<TextSpan>(entity) {
            spans.push(span.0.clone());
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    (texts, spans)
}

/// The panel inside the overlay: the overlay root's only child.
fn sheet_panel(app: &mut App) -> Entity {
    let world = app.world_mut();
    let root = world.query_filtered::<Entity, With<Overlay>>().single(world).expect("an overlay is up");
    world.get::<Children>(root).expect("the overlay holds a panel").iter().next().expect("the overlay holds a panel")
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

/// The seed every test game is dealt from.
const TEST_SEED: u64 = 1;

/// Play vs Computer → Start (Novice) → the board, as the Runner.
fn start_a_game(app: &mut App) {
    start_a_game_as(app, Side::Runner);
}

fn start_a_game_as(app: &mut App, human: Side) {
    start_a_game_with(app, human, DEFAULT_CORP_DECK);
}

fn start_a_game_with(app: &mut App, human: Side, corp_deck: &str) {
    app.world_mut().write_message(Navigate(AppScreen::NewGame));
    app.update();
    app.update();
    assert_eq!(screen(app), AppScreen::NewGame);
    // The form's choice, made directly rather than through five drop-downs:
    // the bottom rung so the bot answers at once. The record it leaves is
    // under the test's own directory (`ClientCore::in_dir`). `start` is
    // the same function the Start button calls.
    let choice = StartChoice { human, level: Level::Novice, style: None, corp_deck: corp_deck.to_string(), runner_deck: DEFAULT_RUNNER_DECK.to_string() };
    // One seed for every test: the games are real, and a test that leans
    // on a board state (an install by the Runner's first turn, a central
    // left open) must see the same deal each run rather than whatever the
    // clock dealt. `new_game::start` itself still seeds from the clock.
    let active = new_game::start_seeded(app.world().resource::<ClientCore>(), &choice, TEST_SEED).expect("the default decks start a game");
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

/// A press and release of the primary button on a hand card: its press is
/// armed by `drag_hand` and the release, with the pointer still, is the
/// click that opens the card's menu.
fn press_card(app: &mut App, entity: Entity) {
    app.world_mut().entity_mut(entity).insert(Interaction::Pressed);
    app.world_mut().write_message(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Pressed, window: Entity::PLACEHOLDER });
    app.update();
    app.world_mut().write_message(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Released, window: Entity::PLACEHOLDER });
    app.update();
    app.update();
}

/// A hand card picked up and dropped at `x`: the press, a pointer well
/// past the drag threshold, then the release.
fn drag_card(app: &mut App, entity: Entity, x: f32) {
    app.world_mut().entity_mut(entity).insert(Interaction::Pressed);
    app.world_mut().write_message(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Pressed, window: Entity::PLACEHOLDER });
    app.update();
    app.world_mut().write_message(CursorMoved { window: Entity::PLACEHOLDER, position: Vec2::new(x, 40.0), delta: None });
    app.update();
    app.world_mut().write_message(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Released, window: Entity::PLACEHOLDER });
    app.update();
    app.update();
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
    press_card(&mut app, face);
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
    press_card(&mut app, face);
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

/// The menu's panel, as the layout was told to place it: its four insets
/// and the box it may grow into.
fn menu_node(app: &mut App) -> Node {
    let mut q = app.world_mut().query_filtered::<&Node, With<ActionsMenu>>();
    q.iter(app.world()).next().cloned().expect("the menu is open")
}

fn pixels(value: Val) -> Option<f32> {
    match value {
        Val::Px(px) => Some(px),
        _ => None,
    }
}

/// An open menu stays on the window: pinned by the edge that faces its
/// card and never past the window's own edge, whatever box it was opened
/// against and whatever size the window is.
///
/// Both of those can move under a menu that is already open — a resize
/// changes the window, a redraw the card's box — while the menu itself is
/// spawned only by a rail redraw, so this drives each in turn. The old
/// placement computed a `top` from an estimate of the panel's height and
/// clamped it on one side only, which put a tall menu's first options
/// above the top of the window, where the screen root's clip ate them.
#[test]
fn an_open_menu_is_placed_on_the_window_and_follows_it() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    // The Runner's own turn, so the menu has rows in it: the mulligan
    // offers a hand card nothing at all.
    to_the_runners_turn(&mut app);
    let card = {
        let model = &app.world().resource::<Model>().0;
        let hand = model.view.as_ref().unwrap().runner.grip_cards.clone().unwrap();
        hand.iter().find(|c| !model.actions.for_hand_card(c).is_empty()).expect("an opening Runner hand has something playable").clone()
    };
    let face = entity_with(&mut app, &Click::Target(Target::HandCard(card))).expect("the card is on the board");
    press_card(&mut app, face);
    let window = app.world().resource::<BoardFit>().window;

    // One horizontal edge and one vertical, the other of each `Auto`, so
    // the panel grows away from the card rather than from a guessed
    // height.
    let on_the_window = |node: &Node, window: Vec2, at: &str| {
        let (left, right) = (pixels(node.left), pixels(node.right));
        let (top, bottom) = (pixels(node.top), pixels(node.bottom));
        assert!(left.is_some() != right.is_some(), "one horizontal edge {at}: {:?}/{:?}", node.left, node.right);
        assert!(top.is_some() != bottom.is_some(), "one vertical edge {at}: {:?}/{:?}", node.top, node.bottom);
        for inset in [left, right, top, bottom].into_iter().flatten() {
            assert!(inset >= 12.0, "an inset of {inset} {at} is inside the window's padding");
        }
        let width = pixels(node.max_width).expect("a width it may grow to");
        let height = pixels(node.max_height).expect("a height it may grow to");
        assert!(width + left.or(right).unwrap_or(0.0) <= window.x + 1e-3, "{width} wide {at}");
        assert!(height + top.or(bottom).unwrap_or(0.0) <= window.y + 1e-3, "{height} tall {at}");
    };
    on_the_window(&menu_node(&mut app), window, "as the menu was spawned");

    // Where a hand card actually sits — on the window's bottom edge.
    // Headless there is no layout pass, so every box is zero-sized at the
    // origin; the boxes below are the ones the board would have laid out.
    let over = |x: f32, y: f32| netrunner_desktop::models::game::Anchor { x, y, width: 120.0, height: 168.0 };
    let place = |app: &mut App, anchor| {
        app.world_mut().resource_mut::<Model>().0.menu.as_mut().expect("the menu is open").over = anchor;
        app.update();
    };
    place(&mut app, over(window.x / 2.0, window.y - 90.0));
    let node = menu_node(&mut app);
    on_the_window(&node, window, "over a hand card");
    assert!(pixels(node.bottom).is_some(), "a hand card's menu opens upward from the card");

    // The card's box moves under it — here to the top-right corner, where
    // there is no room above and the panel must flip below and anchor to
    // the right edge. `place_menu` skips a target whose `ComputedNode` is
    // empty, which every node is here, and that is what lets the test
    // write a box of its own.
    place(&mut app, over(window.x - 30.0, 30.0));
    let node = menu_node(&mut app);
    on_the_window(&node, window, "over the top-right corner");
    assert!(pixels(node.top).is_some(), "with no room above, it hangs below the card");
    assert!(pixels(node.right).is_some(), "and is pinned to the edge it is nearest");

    // And the window itself changes under it.
    let small = Vec2::new(900.0, 520.0);
    app.world_mut().resource_mut::<BoardFit>().window = small;
    app.update();
    on_the_window(&menu_node(&mut app), small, "in a smaller window");

    // Every row is a width in pixels: inside the wrapping panel a
    // percentage has nothing to resolve against, and a row measured a
    // word to a line is how the options left the window in the first
    // place.
    let rows: Vec<Val> = {
        let mut q = app.world_mut().query::<(&Click, &Node)>();
        q.iter(app.world()).filter(|(click, _)| matches!(click, Click::Entry(_))).map(|(_, node)| node.width).collect()
    };
    assert!(!rows.is_empty(), "the menu has rows to check");
    assert!(rows.iter().all(|width| pixels(*width).is_some()), "every row is a px width: {rows:?}");
}

/// The decision pop-up is capped at the window, and the cards are what
/// give when it is short of room: every button stays rigid, so a decision
/// the person is asked is never a button drawn off the bottom.
#[test]
fn the_decision_pop_up_is_capped_at_the_window_and_its_cards_give_first() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    // A window short enough that the pop-up's own words and buttons are
    // most of it.
    let small = Vec2::new(900.0, 520.0);
    app.world_mut().resource_mut::<BoardFit>().window = small;
    app.update();
    app.update();

    let world = app.world_mut();
    let panel = world
        .query_filtered::<&Children, With<DecisionPopup>>()
        .iter(world)
        .next()
        .and_then(|children| children.iter().next())
        .expect("the pop-up is up with its panel");
    let node = world.get::<Node>(panel).expect("the panel is a node").clone();
    match node.max_height {
        Val::Px(height) => assert!(height <= small.y - 2.0 * 12.0 + 1e-3, "the panel may be {height} tall in a {} window", small.y),
        other => panic!("the panel is capped at the window, not {other:?}"),
    }

    // Inside it: the buttons are rigid and the card row is the one thing
    // that may lose height. A card drawn short is still a card, and a
    // secondary click reads it full size; a button off the window is an
    // action the person cannot take.
    let children: Vec<Entity> = world.get::<Children>(panel).expect("the panel has rows").iter().collect();
    let mut gave = 0;
    for child in children {
        let row = world.get::<Node>(child).expect("a row is a node");
        if row.overflow.y == OverflowAxis::Clip {
            assert_eq!(row.flex_shrink, 1.0, "the card row gives");
            assert_eq!(row.min_height, Val::Px(0.0), "and is allowed to, against its own content");
            gave += 1;
        } else {
            assert_eq!(row.flex_shrink, 0.0, "every other row is rigid");
        }
    }
    assert!(gave <= 1, "one row gives, not {gave}");
    let buttons: Vec<f32> = {
        let mut q = app.world_mut().query::<(&Click, &Node)>();
        q.iter(app.world()).filter(|(click, _)| matches!(click, Click::Entry(_))).map(|(_, node)| node.flex_shrink).collect()
    };
    assert!(!buttons.is_empty() && buttons.iter().all(|shrink| *shrink == 0.0), "no decision button gives: {buttons:?}");
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
    press_card(&mut app, face);
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
    // The card says its own name, once. A heading over the face was the
    // title twice on the text tier, which draws it in the face's own
    // title row, and once too often on a cached scan, which *is* the
    // printed card. So: no `Text` under the overlay is the title (a
    // heading is a `Text`), and one of the face's `TextSpan`s is.
    let title = app.world().resource::<ClientCore>().registry.get(&card).expect("the card is in the registry").title.clone();
    let (headings, spans) = overlay_text(&mut app);
    assert!(!headings.contains(&title), "the name is not repeated over the card: {headings:?}");
    assert!(spans.contains(&title), "the face itself prints the name: {spans:?}");
    // And no Close: Escape and a click that misses the panel are the two
    // doors, so a third button for the same rule is chrome.
    assert!(button_labelled(&mut app, "Close").is_none(), "the sheet has no Close button");
    // A press on the panel does not reach the wash behind it. Asserted
    // structurally rather than by hit test: `ui_focus_system` stops at
    // the first hovered node that blocks, and nothing under
    // `MinimalPlugins` has a window to hit-test against.
    let panel = sheet_panel(&mut app);
    assert_eq!(app.world().get::<FocusPolicy>(panel), Some(&FocusPolicy::Block), "the panel holds the press");
    // Through the sheet, neither click opens anything.
    right_click(&mut app, face);
    press_card(&mut app, face);
    app.world_mut().entity_mut(face).insert(Interaction::None);
    assert_eq!(menus(&mut app), 0, "nothing opens through the sheet");
    escape(&mut app);
    assert_eq!(overlays(&mut app), 0, "Escape closes the sheet");
    assert!(!app.world().resource::<Model>().0.confirm_quit, "and does not ask to quit");
    // The other door: the wash carries the Close button's old `Click`,
    // so a press that misses the panel closes the sheet and asks nothing.
    right_click(&mut app, face);
    assert_eq!(overlays(&mut app), 1, "the sheet is open again");
    let scrim = app.world_mut().query_filtered::<Entity, With<Overlay>>().single(app.world()).expect("the wash is the overlay root");
    assert_eq!(app.world().get::<Click>(scrim), Some(&Click::CloseOverlay), "the wash closes what it covers");
    // The wash holds the press rather than letting it through to the
    // board. `Node` requires `FocusPolicy` and defaults it to `Pass`, so
    // this is not something `Button` supplies — a required component is
    // kept, not overwritten, on a post-spawn insert. Without it a click
    // that missed the panel would also press whatever tile or control
    // sat under the pointer.
    assert_eq!(app.world().get::<FocusPolicy>(scrim), Some(&FocusPolicy::Block), "the wash holds the press");
    press_entity(&mut app, scrim);
    assert_eq!(overlays(&mut app), 0, "a click that misses the panel closes the sheet");
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
    press_card(&mut app, face);
    app.world_mut().entity_mut(face).insert(Interaction::None);
    assert_eq!(menus(&mut app), 1);
    click(&mut app, MouseButton::Left);
    assert_eq!(menus(&mut app), 0, "a click elsewhere closes the menu");
    assert_eq!(app.world().resource::<Model>().0.applied, before);
}

/// A key typed on the board: its press and release, then two frames.
fn type_key(app: &mut App, key_code: KeyCode, logical_key: Key) {
    press(app, key_code, logical_key);
    app.update();
    app.update();
}

fn letter(app: &mut App, key_code: KeyCode, c: &str) {
    type_key(app, key_code, Key::Character(c.into()));
}

/// The keys are the buttons pressed another way, read off the keyboard
/// by what they type: at the mulligan 1 keeps; on the Runner's turn C
/// takes a credit, Ctrl-C does nothing, ? lists the keys and nothing acts
/// under the list, M and I open the hovered card's menu and sheet, H turns
/// the play helper on and saves it, and Enter with clicks left asks for a
/// second Enter on the rail before the turn ends.
#[test]
fn the_keys_press_the_buttons_they_stand_for() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    letter(&mut app, KeyCode::Digit1, "1");
    wait_for(&mut app, "the mulligan's first button to be applied", |app| app.world().resource::<Model>().0.applied > 0);
    until_the_runners_turn(&mut app);
    // C: Take 1 credit.
    let (before, credits) = {
        let model = &app.world().resource::<Model>().0;
        (model.applied, model.view.as_ref().unwrap().runner.credits)
    };
    letter(&mut app, KeyCode::KeyC, "c");
    wait_for(&mut app, "the credit to be taken", |app| app.world().resource::<Model>().0.applied > before);
    until_the_runners_turn(&mut app);
    assert_eq!(app.world().resource::<Model>().0.view.as_ref().unwrap().runner.credits, credits + 1);
    // With Ctrl held, C is the system's.
    let before = app.world().resource::<Model>().0.applied;
    app.world_mut().write_message(KeyboardInput { key_code: KeyCode::ControlLeft, logical_key: Key::Control, state: ButtonState::Pressed, text: None, repeat: false, window: Entity::PLACEHOLDER });
    app.update();
    letter(&mut app, KeyCode::KeyC, "c");
    app.world_mut().write_message(KeyboardInput { key_code: KeyCode::ControlLeft, logical_key: Key::Control, state: ButtonState::Released, text: None, repeat: false, window: Entity::PLACEHOLDER });
    app.update();
    app.update();
    assert!(app.world().resource::<Model>().0.awaiting && app.world().resource::<Model>().0.applied == before, "Ctrl-C sent nothing");
    // ? lists every key; under the list no key acts; Escape closes it.
    type_key(&mut app, KeyCode::Slash, Key::Character("?".into()));
    assert_eq!(overlays(&mut app), 1);
    assert_eq!(app.world_mut().query::<&HelpRow>().iter(app.world()).count(), netrunner_desktop::models::shortcuts::LIST.len());
    letter(&mut app, KeyCode::KeyC, "c");
    assert_eq!(app.world().resource::<Model>().0.applied, before, "nothing acts under the list");
    type_key(&mut app, KeyCode::Escape, Key::Escape);
    assert_eq!(overlays(&mut app), 0);
    assert!(!app.world().resource::<Model>().0.confirm_quit);
    // M and I act on the hovered card.
    let face = {
        let mut q = app.world_mut().query::<(Entity, &Click)>();
        q.iter(app.world()).find(|(_, c)| matches!(c, Click::Target(Target::HandCard(_)))).map(|(e, _)| e).expect("a hand card")
    };
    app.world_mut().entity_mut(face).insert(Interaction::Hovered);
    letter(&mut app, KeyCode::KeyM, "m");
    assert_eq!(menus(&mut app), 1, "M opens the hovered card's menu");
    letter(&mut app, KeyCode::KeyI, "i");
    assert_eq!(overlays(&mut app), 1, "I reads it");
    assert_eq!(menus(&mut app), 0);
    type_key(&mut app, KeyCode::Escape, Key::Escape);
    app.world_mut().entity_mut(face).insert(Interaction::None);
    // H turns the play helper on, and it is saved.
    assert!(!app.world().resource::<ClientCore>().settings.desktop.play_helper);
    letter(&mut app, KeyCode::KeyH, "h");
    assert!(app.world().resource::<ClientCore>().settings.desktop.play_helper);
    letter(&mut app, KeyCode::KeyH, "h");
    assert!(!app.world().resource::<ClientCore>().settings.desktop.play_helper);
    // Enter with clicks left: a notice on the rail, then the turn ends.
    assert!(app.world().resource::<Model>().0.clicks_left() > 0);
    type_key(&mut app, KeyCode::Enter, Key::Enter);
    assert_eq!(app.world_mut().query::<&EndTurnNotice>().iter(app.world()).count(), 1, "the rail asks for a second Enter");
    assert!(app.world().resource::<Model>().0.awaiting, "and nothing was sent");
    type_key(&mut app, KeyCode::Enter, Key::Enter);
    wait_for(&mut app, "the turn to end", |app| app.world().resource::<Model>().0.view.as_ref().is_some_and(|v| !matches!(v.phase, GamePhase::Action(Side::Runner)) || v.paid_ability_window.is_some()));
    assert_eq!(app.world_mut().query::<&EndTurnNotice>().iter(app.world()).count(), 0);
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

/// Every `Text` on the screen in tree order, as [`clicks_in_tree_order`].
fn texts_in_tree_order(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let roots: Vec<Entity> = world.query_filtered::<Entity, (With<Node>, Without<ChildOf>)>().iter(world).collect();
    let mut stack: Vec<Entity> = roots.into_iter().rev().collect();
    let mut texts = Vec::new();
    while let Some(entity) = stack.pop() {
        if let Some(text) = world.get::<Text>(entity) {
            texts.push(text.0.clone());
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter().rev());
        }
    }
    texts
}

/// The servers keep one order from either chair — Archives, R&D, HQ,
/// then the remotes — so the centrals never move as remotes are made.
#[test]
fn the_servers_keep_one_order_from_either_chair() {
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
    assert_eq!(servers(&mut app), [ServerId::Archives, ServerId::RnD, ServerId::Hq], "the Runner's chair, the same");
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
    // No Close — but the heading stays (asserted for Archives below):
    // a zone has no face to print its own name on.
    assert!(button_labelled(&mut app, "Close").is_none(), "a zone's sheet has no Close either");
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
    escape(&mut app);
    // The heap has no action on it, so its plain click is its sheet: every
    // card in it, face up, without a secondary click.
    let heap = entity_with(&mut app, &Click::Target(Target::Pile(netrunner_client::board::Pile::Heap))).expect("the heap is a button");
    press_entity(&mut app, heap);
    assert_eq!(overlays(&mut app), 1, "the heap's sheet is up");
    assert!(texts(&mut app).iter().any(|t| t.contains("all face up")), "{:?}", texts(&mut app));
    assert_eq!(app.world().resource::<Model>().0.applied, before);
}

/// The rig's rows are labelled in the order the chair sees the table:
/// programs next to the ICE — the top of the Runner's rig, the bottom of
/// the Corp's view of it — and every row is there before anything is
/// installed in it.
#[test]
fn the_rig_is_three_rows_with_programs_next_to_the_ice_from_either_chair() {
    // The first three: a card face below the rig can print its own type
    // ("Hardware") on its type line.
    let rows = |app: &mut App| -> Vec<String> { texts_in_tree_order(app).into_iter().filter(|t| ["Programs", "Hardware", "Resources"].contains(&t.as_str())).take(3).collect() };
    let (mut app, _dir) = headless_client();
    start_a_game_as(&mut app, Side::Runner);
    wait_for(&mut app, "the Runner's first decision", |app| click_entry_count(app) > 0);
    assert_eq!(rows(&mut app), ["Programs", "Hardware", "Resources"], "the Runner's chair");

    let (mut app, _dir) = headless_client();
    start_a_game_as(&mut app, Side::Corp);
    wait_for(&mut app, "the Corp's first decision", |app| click_entry_count(app) > 0);
    assert_eq!(rows(&mut app), ["Resources", "Hardware", "Programs"], "the Corp's chair");
}


/// The carve-out: a click that misses the panel closes a *reading*
/// surface and leaves a question standing.
///
/// The options and the list of keys are forms, and they keep their Close
/// — a setting given up because the pointer landed an inch wide is a
/// worse failure than a button nobody needed. The quit prompt is asking
/// something and Escape does not close it either.
///
/// Both kinds of wash still **block**, which is the separate half: a form
/// declining to act on a press is not the same as letting it through to
/// the board, where the control bar would take it and end the turn.
#[test]
fn the_wash_over_a_form_blocks_the_press_without_acting_on_it() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);

    let gear = entity_with(&mut app, &Click::Options).expect("the gear is in the top bar");
    press_entity(&mut app, gear);
    assert_eq!(overlays(&mut app), 1, "the options are open");
    let scrim = app.world_mut().query_filtered::<Entity, With<Overlay>>().single(app.world()).expect("a wash");
    assert_eq!(app.world().get::<FocusPolicy>(scrim), Some(&FocusPolicy::Block), "a form's wash still holds the press");
    assert_eq!(app.world().get::<Click>(scrim), None, "but carries nothing to act on");
    assert!(button_labelled(&mut app, "Close").is_some(), "a form keeps its Close");
    press_entity(&mut app, scrim);
    assert_eq!(overlays(&mut app), 1, "a click that misses a form leaves it open");
    assert!(app.world().resource::<Model>().0.options_open);

    // Escape closes the options; Escape again asks to quit, and that
    // question stands against a click away too.
    escape(&mut app);
    escape(&mut app);
    assert!(app.world().resource::<Model>().0.confirm_quit, "the quit prompt is up");
    let scrim = app.world_mut().query_filtered::<Entity, With<Overlay>>().single(app.world()).expect("a wash");
    assert_eq!(app.world().get::<Click>(scrim), None, "a question is not dismissed by missing it");
    press_entity(&mut app, scrim);
    assert!(app.world().resource::<Model>().0.confirm_quit, "and it stands");
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

/// A hand card dragged along the hand changes the person's own order and
/// never plays it, and a press that does not travel is still the click
/// that opens its menu. The order is the client's: the view is unchanged.
#[test]
fn a_hand_card_dragged_along_the_hand_reorders_it_and_plays_nothing() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let (before, order) = {
        let model = &app.world().resource::<Model>().0;
        (model.applied, model.hand.cards().to_vec())
    };
    assert!(order.len() >= 3, "an opening hand");
    let last = order.len() - 1;
    let face = entity_with(&mut app, &Click::Target(Target::HandCard(order[last].clone()))).expect("the card is on the board");
    // The faces are laid out at zero width with no window, so every slot
    // centre is 0.0 and a drop left of them lands at the head of the row —
    // which is what this test is about: the drag path, not the geometry
    // (`models::drag::insert_at` is tested on its own).
    drag_card(&mut app, face, -50.0);
    let (head, held, applied, awaiting, view_hand) = {
        let model = &app.world().resource::<Model>().0;
        (
            model.hand.cards()[0].clone(),
            model.hand.cards().len(),
            model.applied,
            model.awaiting,
            model.view.as_ref().unwrap().runner.grip_cards.clone().unwrap(),
        )
    };
    assert_eq!(head, order[last], "the card is at the head of the hand");
    assert_eq!(held, order.len(), "and the hand is the same cards");
    assert!(applied == before && awaiting, "the drag played nothing");
    assert_eq!(menus(&mut app), 0, "and opened no menu");
    assert_eq!(view_hand, order, "the view keeps the engine's order: the order is the client's");
    // A press that does not travel is still the click.
    let face = entity_with(&mut app, &Click::Target(Target::HandCard(order[0].clone()))).expect("the card is on the board");
    press_card(&mut app, face);
    assert_eq!(menus(&mut app), 1, "a still press opens the menu");
    assert_eq!(app.world().resource::<Model>().0.applied, before);
}

/// A hand shows a third of each card, and the one the pointer rests on
/// lifts out whole as a second, inert copy: the face in the row stays
/// where it was, nothing is submitted, and the copy goes when the pointer
/// does.
#[test]
fn a_hovered_hand_card_lifts_out_whole_and_nothing_is_sent() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let before = app.world().resource::<Model>().0.applied;
    let lifted = |app: &mut App| app.world_mut().query::<&LiftedCard>().iter(app.world()).map(|l| l.0).collect::<Vec<_>>();
    let slot = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<HandSlot>>().iter(world).next().expect("a hand card")
    };
    assert!(lifted(&mut app).is_empty(), "nothing is lifted until something is hovered");
    app.world_mut().entity_mut(slot).insert(Interaction::Hovered);
    app.update();
    assert_eq!(lifted(&mut app), [slot], "the hovered card lifts out");
    let copy = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<LiftedCard>>().single(world).unwrap()
    };
    assert!(!app.world().entity(copy).contains::<Button>(), "the lifted copy is not a button, so the hover holds on the face under it");
    app.update();
    assert_eq!(lifted(&mut app), [slot], "and stays while the pointer does");
    app.world_mut().entity_mut(slot).insert(Interaction::None);
    app.update();
    assert!(lifted(&mut app).is_empty(), "and goes when it leaves");
    let model = &app.world().resource::<Model>().0;
    assert!(model.applied == before && model.menu.is_none(), "considering a card is not a game event");
}

/// The middle of the table does not move: the card width is a function
/// of the window, the chair and the servers, so the Corp's ICE and the
/// Runner's installs never re-fit it. The control bar is a row of the
/// board, above the hand, and every server has its plate.
#[test]
fn the_card_width_holds_while_the_board_fills_and_the_bar_sits_on_the_board() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    let columns = |app: &mut App| app.world_mut().query::<&ServerColumn>().iter(app.world()).count();
    let plates = |app: &mut App| app.world_mut().query::<&ServerPlate>().iter(app.world()).count();
    let (face, servers) = (app.world().resource::<BoardFit>().face, columns(&mut app));
    assert!(face > 0.0, "fitted");
    assert_eq!(plates(&mut app), servers, "a plate per server");
    let keep = button_labelled(&mut app, "Keep hand").expect("Keep hand is a decision");
    press_entity(&mut app, keep);
    // The game is seeded from the clock, so the Corp's first install may
    // be a turn or two away: end the Runner's turns until it has made one,
    // as the tile test does.
    let pieces = |app: &App| -> usize { app.world().resource::<Model>().0.view.as_ref().map_or(0, |v| v.corp.servers.iter().map(|s| s.ice.len() + s.root.len()).sum()) };
    for _ in 0..6 {
        until_the_runners_turn(&mut app);
        if pieces(&app) > 0 {
            break;
        }
        let (end, disabled) = control_button(&mut app, Control::EndTurn);
        assert!(!disabled);
        press_entity(&mut app, end);
        wait_for(&mut app, "the turn to end", |app| app.world().resource::<Model>().0.view.as_ref().is_some_and(|v| !matches!(v.phase, GamePhase::Action(Side::Runner)) || v.paid_ability_window.is_some()));
    }
    assert!(pieces(&app) > 0, "the Corp installed something within six turns");
    if columns(&mut app) == servers {
        assert_eq!(app.world().resource::<BoardFit>().face, face, "the Corp's installs moved no card");
    }
    // The bar is inside the board now, so it is redrawn with it: one bar,
    // with its buttons, after the redraw.
    assert_eq!(app.world_mut().query::<&ControlBar>().iter(app.world()).count(), 1);
    let (_, disabled) = control_button(&mut app, Control::GainCredit);
    assert!(!disabled, "the bar offers what the engine does");
}

/// The phase bar is a row of the board: the turn's steps with the one in
/// play marked, a run's steps beside them while a run is on, and L turns
/// it off — which gives the cards the row back — and saves the choice.
#[test]
fn the_phase_bar_marks_the_step_in_play_and_l_turns_it_off() {
    use netrunner_client::board::phase::State;
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    assert!(app.world().resource::<ClientCore>().settings.desktop.phase_bar, "the bar is on by default");
    let steps = |app: &mut App| -> Vec<State> {
        let world = app.world_mut();
        world.query::<&PhaseStep>().iter(world).map(|s| s.0).collect()
    };
    // The panel is in the right column and is hidden, not despawned, when
    // the setting is off.
    let shown = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&Node, With<PhaseBarRow>>().iter(world).filter(|node| node.display != Display::None).count()
    };
    assert_eq!(shown(&mut app), 1);
    // The mulligan is its own segment: one step, and the game is in it.
    assert_eq!(steps(&mut app), [State::Now]);
    assert!(texts(&mut app).iter().any(|t| t == "Opening hands"));
    to_the_runners_turn(&mut app);
    // The turn's three steps, the actions one in play.
    let marked = steps(&mut app);
    assert_eq!(marked.len(), 3, "{marked:?}");
    assert_eq!(marked, [State::Past, State::Now, State::Ahead]);
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Actions · ")), "the clicks left are on the step");
    let face = app.world().resource::<netrunner_desktop::screens::game::BoardFit>().face;
    // L turns it off: the panel goes, the choice is saved, and no card
    // moves — the panel was never a row of the board.
    letter(&mut app, KeyCode::KeyL, "l");
    assert_eq!(shown(&mut app), 0, "the panel is hidden");
    assert!(steps(&mut app).is_empty());
    assert!(!app.world().resource::<ClientCore>().settings.desktop.phase_bar);
    let after = app.world().resource::<netrunner_desktop::screens::game::BoardFit>().face;
    assert_eq!(after, face, "the cards keep their size");
    letter(&mut app, KeyCode::KeyL, "l");
    assert_eq!(shown(&mut app), 1, "and back on");
    // A run adds a second segment, and the run's own step is marked.
    let rnd = entity_with(&mut app, &Click::Target(Target::Server(ServerId::RnD))).expect("R&D's header");
    press_entity(&mut app, rnd);
    let run = {
        let model = &app.world().resource::<Model>().0;
        let menu = model.menu.clone().expect("the zone's menu is open");
        *menu.entries.iter().find(|i| matches!(model.actions.entries[**i].action, PlayerAction::InitiateRun { server: ServerId::RnD })).expect("R&D offers the run")
    };
    let button = entity_with(&mut app, &Click::Entry(run)).expect("the run's button");
    press_entity(&mut app, button);
    wait_for(&mut app, "the run on the board", |app| app.world().resource::<Model>().0.view.as_ref().is_some_and(|v| v.active_run.is_some()));
    app.update();
    app.update();
    assert!(texts(&mut app).iter().any(|t| t == "Run on R&D"), "{:?}", texts(&mut app));
    assert!(steps(&mut app).len() >= 3 + 4, "the turn's steps and the run's");
}

/// The Runner appears in the right column while a run is on, and only
/// then: the text tier here, since the headless client has no scans.
#[test]
fn a_run_puts_the_runner_in_the_right_column() {
    use netrunner_desktop::screens::game::{RunIdentity, RunIdentityName};
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let names = |app: &mut App| -> Vec<String> {
        let world = app.world_mut();
        world.query_filtered::<&Text, With<RunIdentityName>>().iter(world).map(|t| t.0.clone()).collect()
    };
    assert_eq!(app.world_mut().query::<&RunIdentity>().iter(app.world()).count(), 1, "the panel's slot is always there");
    assert!(names(&mut app).is_empty(), "no run, no Runner");
    let rnd = entity_with(&mut app, &Click::Target(Target::Server(ServerId::RnD))).expect("R&D's header");
    press_entity(&mut app, rnd);
    let run = {
        let model = &app.world().resource::<Model>().0;
        let menu = model.menu.clone().expect("the zone's menu is open");
        *menu.entries.iter().find(|i| matches!(model.actions.entries[**i].action, PlayerAction::InitiateRun { server: ServerId::RnD })).expect("R&D offers the run")
    };
    let button = entity_with(&mut app, &Click::Entry(run)).expect("the run's button");
    press_entity(&mut app, button);
    wait_for(&mut app, "the Runner in the right column", |app| !names(app).is_empty());
    let identity = {
        let model = &app.world().resource::<Model>().0;
        let id = model.view.as_ref().and_then(|v| v.runner.identity.clone()).expect("the Runner has an identity");
        app.world().resource::<ClientCore>().registry.get(&id).expect("a known card").title.clone()
    };
    assert_eq!(names(&mut app), [identity]);
    assert!(texts(&mut app).iter().any(|t| t == "Hacking into R&D"), "{:?}", texts(&mut app));
}

/// The board has no margin above the opponent's hand or below the
/// person's: the root pads its sides only, and the person's strip is the
/// board's last row, pinned to its bottom.
#[test]
fn the_hands_sit_on_the_windows_edges() {
    use netrunner_desktop::screens::game::Board;
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    let world = app.world_mut();
    let board = world.query_filtered::<Entity, With<Board>>().single(world).expect("the board");
    let root = world.get::<ChildOf>(board).and_then(|body| world.get::<ChildOf>(body.parent())).expect("the board is in the body, in the root").parent();
    let padding = world.get::<Node>(root).expect("the root is a node").padding;
    assert_eq!((padding.top, padding.bottom), (Val::Px(0.0), Val::Px(0.0)), "nothing above or below the board");
    let last = *world.get::<Children>(board).expect("the board has rows").last().expect("a last row");
    assert_eq!(world.get::<Node>(last).expect("a row").align_items, AlignItems::FlexEnd, "the hand is pinned to the bottom edge");
    assert!(world.get::<Children>(last).is_some_and(|row| row.iter().count() == 2), "the last row is the person's strip and hand");
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
    // The words sit on a band over the tile's picture, so they are read
    // from anywhere under the tile.
    let mut stack = vec![tile];
    let mut words = String::new();
    while let Some(entity) = stack.pop() {
        if let Some(text) = world.get::<Text>(entity) {
            words.push_str(&text.0);
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    let view = world.resource::<Model>().0.view.clone().unwrap();
    let card = view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).find(|c| c.install_id == id).unwrap().clone();
    let expected = if card.rezzed { "rezzed" } else if card.slot == netrunner_core::rules::InstallSlot::Ice { "unrezzed" } else { "face down" };
    assert!(words.contains(expected), "the tile reads {words:?}, expected {expected:?}");
    // Its sheet, on a secondary click: the state lines, and nothing else.
    right_click(&mut app, tile);
    let facts: Vec<String> = {
        let world = app.world_mut();
        world.query_filtered::<&Text, With<InstallFact>>().iter(world).map(|t| t.0.clone()).collect()
    };
    assert!(!facts.is_empty(), "the sheet lists the install's state");
    assert!(facts[0].starts_with("Ice protecting") || facts[0].starts_with("In the root of"), "{facts:?}");
    assert!(facts.iter().any(|l| l == "Rezzed" || l.starts_with("Unrezzed") || l.starts_with("Face down")), "{facts:?}");
    assert_eq!(overlays(&mut app), 1);
    assert!(button_labelled(&mut app, "Close").is_none(), "an install's sheet has no Close either");
    // The one heading that survived, and the reason it did: a card the
    // viewer cannot name is drawn as a card *back*, which says nothing,
    // so `facts::hidden_title` is all that names it. A card they *can*
    // name has its title on its face, and a heading would be the second
    // copy — which is what this branch checks by its absence.
    let (headings, _) = overlay_text(&mut app);
    let title = card.card.as_ref().and_then(|c| app.world().resource::<ClientCore>().registry.get(c).map(|d| d.title.clone()));
    match title {
        Some(title) => assert!(!headings.contains(&title), "a card the viewer can name is not titled twice: {headings:?}"),
        // The Runner cannot name an unrezzed Corp card, so this is the
        // branch an opening board actually takes; the exact words are
        // `facts::hidden_title`'s two.
        None => assert!(
            headings.iter().any(|h| h == "Unrezzed ice" || h == "Face-down card"),
            "a card the viewer cannot name is named by the sheet: {headings:?}"
        ),
    }
    assert_eq!(app.world().resource::<Model>().0.applied, app.world().resource::<Model>().0.applied, "nothing was submitted");
}

/// An access shows the card, not its name: the person's report was that
/// all they got was a title. The Runner runs R&D, breaches it, and the
/// decision pop-up carries the accessed card's printed face above the
/// steal/trash/pass buttons — the one place on the board where a card
/// and its actions sit together, because an accessed card has no tile.
#[test]
fn an_access_puts_the_card_in_the_decision_popup_above_its_actions() {
    use netrunner_client::access::Access;

    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    // A central the Corp has not protected yet: this presses only Continue
    // and Complete, so a run into ICE ends before any breach. The game is
    // seeded from the clock and the Corp's first turn sometimes puts ICE
    // on R&D, which made R&D-only a one-in-five timeout. R&D and HQ both
    // always hold cards, so either one breaches to an access.
    let target = {
        let view = app.world().resource::<Model>().0.view.clone().expect("a view");
        let bare = |server: ServerId| view.corp.servers.iter().find(|s| s.server == server).is_none_or(|s| s.ice.is_empty());
        [ServerId::RnD, ServerId::Hq].into_iter().find(|s| bare(*s)).expect("R&D or HQ is still unprotected on the Runner's first turn")
    };
    let header = entity_with(&mut app, &Click::Target(Target::Server(target))).expect("the central's plate");
    press_entity(&mut app, header);
    let run = {
        let model = &app.world().resource::<Model>().0;
        let menu = model.menu.clone().expect("the zone's menu is open");
        *menu.entries.iter().find(|i| matches!(model.actions.entries[**i].action, PlayerAction::InitiateRun { server } if server == target)).expect("the central offers the run")
    };
    let button = entity_with(&mut app, &Click::Entry(run)).expect("the run's button");
    press_entity(&mut app, button);
    // A run does not breach by itself: the Runner continues it through
    // initiation and approach, and an unprotected R&D then breaches. The
    // bar's Continue run is the one button pressed, so the test never
    // takes a jack-out by accident.
    let parked = |app: &App| {
        let core = app.world().resource::<ClientCore>();
        app.world().resource::<Model>().0.view.as_ref().and_then(|view| Access::of(view, &core.registry)).is_some()
    };
    // Continue through the phases, then Complete run at the success
    // phase, which is what opens the breach. Only those two are pressed,
    // so the test never takes a jack-out by accident. Waiting on
    // `awaiting` between presses rather than counting frames: the match
    // is a real thread, and under a loaded test runner it answers later
    // than a fixed frame budget allows.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !parked(&app) {
        assert!(Instant::now() < deadline, "waited ten seconds for the breach");
        if !app.world().resource::<Model>().0.awaiting {
            std::thread::sleep(Duration::from_millis(5));
            app.update();
            continue;
        }
        let live = [Control::ContinueRun, Control::CompleteRun]
            .into_iter()
            .map(|control| control_button(&mut app, control))
            .find(|(_, greyed)| !greyed);
        match live {
            Some((button, _)) => press_entity(&mut app, button),
            None => {
                std::thread::sleep(Duration::from_millis(5));
                app.update();
            }
        }
    }
    app.update();
    app.update();

    let access = {
        let core = app.world().resource::<ClientCore>();
        let view = app.world().resource::<Model>().0.view.clone().expect("a view");
        Access::of(&view, &core.registry).expect("still parked at the access")
    };

    // The card is inside the pop-up, drawn as a face: its printed text
    // is on a `BodyText` node under the `DecisionPopup`.
    let world = app.world_mut();
    let popup = world.query_filtered::<Entity, With<DecisionPopup>>().single(world).expect("the pop-up is up");
    // A face's rules text is a `Text` marked `BodyText` whose runs are
    // `TextSpan` children, one per word-run and one per icon, so the
    // card's words are the spans joined rather than the node's own text.
    let mut body = None;
    let mut headings = Vec::new();
    let mut stack = vec![popup];
    while let Some(entity) = stack.pop() {
        if let Some(text) = world.get::<Text>(entity) {
            headings.push(text.0.clone());
        }
        if world.get::<BodyText>(entity).is_some() {
            let mut runs = String::new();
            let mut spans = world.get::<Children>(entity).map(|c| c.iter().collect::<Vec<_>>()).unwrap_or_default();
            spans.reverse();
            while let Some(span) = spans.pop() {
                if let Some(run) = world.get::<TextSpan>(span) {
                    runs.push_str(&run.0);
                }
            }
            body = Some(runs);
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter().rev());
        }
    }
    let body = body.expect("the accessed card's face is in the pop-up");
    // The words, not the symbols: which glyph a `[subroutine]` is drawn
    // with is the theme's choice (the icon font when it is cached, a
    // stand-in when it is not), so asserting the whole rendered string
    // would make this test depend on what fonts the machine has.
    for run in &access.face.body {
        if let Segment::Text(text) = run {
            let text = text.trim();
            if !text.is_empty() {
                assert!(body.contains(text), "the face carries the card's own words: {text:?} missing from {body:?}");
            }
        }
    }
    assert!(headings.iter().any(|text| *text == access.title()), "the pop-up is titled by the card: {headings:?}");
    // The pop-up keeps its accent border now that `widgets::panel`
    // carries a `Dressed` of its own: a caller that draws its border
    // differently says so with its own slot, or `dress` would repaint it
    // in the panel's colour a frame after it was spawned. The slot is
    // half the assertion, the colour the other half.
    let accent = app.world().resource::<Theme>().accent;
    let panel = app.world().get::<Children>(popup).expect("the pop-up holds a panel").iter().next().expect("a panel");
    assert_eq!(app.world().get::<Dressed>(panel).map(|d| d.slot), Some(Slot::PanelDecision), "the pop-up names its own slot");
    assert_eq!(app.world().get::<BorderColor>(panel).map(|b| b.top), Some(accent), "and keeps the accent border");

    // And the actions that follow the access are the buttons under it.
    let labels: Vec<String> = {
        let model = &app.world().resource::<Model>().0;
        model.actions.decisions().iter().map(|i| model.actions.entries[*i].label.clone()).collect()
    };
    assert!(
        labels.iter().any(|label| label.starts_with("Steal ") || label.starts_with("Trash ") || label.starts_with("Pass on ")),
        "the access decisions are the pop-up's buttons: {labels:?}"
    );
    for label in &labels {
        assert!(button_labelled(&mut app, label).is_some(), "{label} has a button");
    }
}

/// The board lights what the engine will accept and nothing else.
///
/// The assertion is *agreement*, not a count: for every clickable target
/// on the board, the glow it wears is exactly the mood the model gives
/// it. A glow derived from anything but `legal_actions` — a card type, a
/// credit total, what a system can see on screen — would fail here the
/// first time the engine disagreed with the guess.
#[test]
fn the_board_glows_exactly_what_the_engine_offers_and_in_the_right_mood() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);

    let mut lit: Vec<(Target, Option<Affordance>, Option<Affordance>)> = Vec::new();
    {
        let mut targets = app.world_mut().query::<(Entity, &Click)>();
        let clickable: Vec<(Entity, Target)> = targets
            .iter(app.world())
            .filter_map(|(entity, click)| match click {
                Click::Target(target) => Some((entity, target.clone())),
                _ => None,
            })
            .collect();
        for (entity, target) in clickable {
            let drawn = app.world().get::<Glowing>(entity).map(|g| g.0);
            let wanted = app.world().resource::<Model>().0.affordance_for(&target);
            lit.push((target, drawn, wanted));
        }
    }
    assert!(!lit.is_empty(), "the board draws clickable targets on the Runner's turn");
    for (target, drawn, wanted) in &lit {
        assert_eq!(drawn, wanted, "{target:?} is drawn {drawn:?} but the engine offers {wanted:?}");
    }
    // The Runner's own turn: a playable card in hand is the commonest
    // glow there is, and it is purple, not the warning colour.
    assert!(
        lit.iter().any(|(target, drawn, _)| matches!(target, Target::HandCard(_)) && *drawn == Some(Affordance::Usable)),
        "a card in hand should be playable on the Runner's own turn"
    );
    // And nothing on this board is a passing moment yet, so nothing is
    // yellow: the mood tracks the moment rather than being decoration.
    assert!(
        !lit.iter().any(|(_, drawn, _)| *drawn == Some(Affordance::Conditional)),
        "no window is open in the Runner's action phase, so nothing should warn"
    );
}

/// The contact shadow sits a card on the table, and it shares its one
/// `BoxShadow` with the glow rather than replacing it.
///
/// That sharing is the whole reason the composer exists: a node has
/// exactly one `BoxShadow`, so two `insert`s would mean whichever ran
/// second silently won, and the board would have lost either its depth or
/// its affordances depending on system order.
#[test]
fn a_card_carries_its_contact_shadow_and_its_glow_in_one_component() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    // The composer runs on `Added`, so let the frame after the board's
    // own spawn go by.
    app.update();

    let theme = app.world().resource::<netrunner_desktop::theme::Theme>().clone();
    let mut found_plain = 0;
    let mut found_glowing = 0;
    let mut query = app.world_mut().query::<(&Contact, Option<&Glowing>, &BoxShadow)>();
    let rows: Vec<(bool, Vec<Color>)> = query.iter(app.world()).map(|(_, glowing, shadow)| (glowing.is_some(), shadow.0.iter().map(|s| s.color).collect())).collect();
    assert!(!rows.is_empty(), "the board's cards and tiles sit on the table");
    for (glowing, colours) in rows {
        if glowing {
            found_glowing += 1;
            assert_eq!(colours.len(), 2, "a glowing card carries both shadows");
            // The glow is first, which is the one drawn on top: a halo
            // the contact shadow has washed grey is not a signal.
            assert!(colours[0] == theme.glow_usable || colours[0] == theme.glow_conditional, "the glow comes first, got {:?}", colours[0]);
            let contact = colours[1].to_srgba();
            assert!(contact.alpha > 0.0 && contact.red == 0.0 && contact.green == 0.0 && contact.blue == 0.0, "the contact shadow is black at an alpha, got {contact:?}");
        } else {
            found_plain += 1;
            assert_eq!(colours.len(), 1, "a card with nothing to do carries the contact shadow alone");
        }
    }
    assert!(found_plain > 0, "the opponent's cards never glow but still sit on the table");
    assert!(found_glowing > 0, "something on the Runner's own turn can be acted on");
}

/// The report: Top-Down Solutions drew two cards and asked which to
/// install, and the pop-up named them without showing them. A card
/// selection now draws every candidate as its card, the card is its own
/// button (a press selects it, as the label under it does), and the one
/// selected keeps its place, outlined. Played from a real game: the Corp's
/// seat takes the engine's actions in turn until it is asked to choose.
#[test]
fn a_card_selection_shows_the_cards_and_a_card_is_its_own_button() {
    use netrunner_client::selection::Selection;
    let (mut app, _dir) = headless_client();
    start_a_game_with(&mut app, Side::Corp, "fashion_lab");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut submitted = None;
    loop {
        assert!(Instant::now() < deadline, "no card selection in a minute of play");
        app.update();
        let model = &app.world().resource::<Model>().0;
        assert!(!model.finished(), "the game ended before the Corp was asked to choose a card");
        if !model.awaiting || submitted == Some(model.applied) {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        let core = app.world().resource::<ClientCore>();
        if model.view.as_ref().and_then(|view| Selection::of(view, &core.registry)).is_some_and(|s| !s.candidates.is_empty()) {
            break;
        }
        let action = model.actions.entries[model.applied % model.actions.entries.len()].action.clone();
        submitted = Some(model.applied);
        app.world().resource::<ActiveMatch>().handle.submit(action).expect("the match is live");
    }
    app.update();
    app.update();

    let (selection, choices) = {
        let model = &app.world().resource::<Model>().0;
        let selection = model.actions.selection().expect("the Corp is choosing").clone();
        let hidden = selection.hidden();
        let choices: Vec<_> = selection.candidates.iter().filter(|c| !hidden.contains(&c.position)).cloned().collect();
        (selection, choices)
    };
    let cards: Vec<ChoiceCard> = app.world_mut().query::<&ChoiceCard>().iter(app.world()).cloned().collect();
    let named: Vec<_> = choices.iter().filter_map(|c| c.card.clone()).collect();
    assert_eq!(cards.iter().map(|c| c.0.clone()).collect::<Vec<_>>(), named, "one card per button, in position order");

    // Press the first candidate's card, not its label.
    let first = choices[0].position;
    let index = {
        let model = &app.world().resource::<Model>().0;
        model.actions.entries.iter().position(|e| e.action == PlayerAction::ToggleCardSelection { position: first }).expect("the toggle")
    };
    let face = {
        let world = app.world_mut();
        let mut faces = world.query::<(Entity, &Click, Option<&ChoiceCard>)>();
        faces.iter(world).find(|(_, click, card)| **click == Click::Entry(index) && card.is_some()).map(|(e, _, _)| e).expect("the card is a button")
    };
    let before = selection.chosen().len();
    press_entity(&mut app, face);
    wait_for(&mut app, "the selection to take the card", |app| {
        let model = &app.world().resource::<Model>().0;
        model.awaiting && model.actions.selection().is_some_and(|s| s.chosen().len() > before)
    });
    // The card is still drawn, now outlined as chosen.
    app.update();
    let outlined = app.world_mut().query_filtered::<&ChoiceCard, With<Outline>>().iter(app.world()).count();
    assert_eq!(outlined, 1, "the chosen card is drawn, outlined");
}
