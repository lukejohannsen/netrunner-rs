//! A game, without a window: the form starts a match against the bottom
//! rung on a thread, the board draws the first decision, a press on a
//! decision button submits it, a card or a zone opens its sheet and never
//! acts, the control bar greys what is not legal, the gear opens the
//! options, and Escape asks before leaving.
//!
//! Driven under `MinimalPlugins` like `tests/navigation.rs`; the match
//! thread is real, so the test pumps `update` until the message it
//! waits for has arrived, with a bound so a broken thread fails rather
//! than hangs.

use std::time::{Duration, Instant};

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use netrunner_client::board::{Control, Target};
use netrunner_client::start::{Level, StartChoice, DEFAULT_CORP_DECK, DEFAULT_RUNNER_DECK};
use netrunner_core::rules::{GamePhase, PlayerAction, ServerId, Side};
use netrunner_desktop::core::ClientCore;
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::screens::game::{Click, LogRow, Model, Overlay};
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
    app.insert_resource(ClientCore::in_dir(dir.clone()));
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

/// Play vs Computer → Start (Novice, unrated) → the board.
fn start_a_game(app: &mut App) {
    app.world_mut().write_message(Navigate(AppScreen::NewGame));
    app.update();
    app.update();
    assert_eq!(screen(app), AppScreen::NewGame);
    // The form's choice, made directly rather than through five drop-downs:
    // the bottom rung so the bot answers at once, unrated so no file is
    // written. `start` is the same function the Start button calls.
    let choice = StartChoice { human: Side::Runner, level: Level::Novice, style: None, corp_deck: DEFAULT_CORP_DECK.to_string(), runner_deck: DEFAULT_RUNNER_DECK.to_string(), rated: false };
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
    // The helper is off, so the rail has the prompt's decisions — keep
    // and mulligan — and not a flat panel (the same two, here).
    assert_eq!(entries, model.0.actions.decisions().len(), "one button per decision");
    assert_eq!(entries, 2);
    assert!(model.0.prompt.as_ref().is_some_and(|p| p.title.contains("mulligan")), "{:?}", model.0.prompt);
    let before = model.0.applied;
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

/// A hand card opens its sheet whether or not it has an action, and a
/// press on the sheet's button is what submits — never the card click.
/// The first game played by hand had every card click doing nothing
/// (a face is a `Button` the shared feedback system does not report),
/// and the second had a click to examine a card install it.
#[test]
fn pressing_a_hand_card_opens_its_sheet_and_the_sheet_submits() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    wait_for(&mut app, "the first decision", |app| click_entry_count(app) > 0);
    // At the mulligan no hand card has an action: the sheet is the card.
    let face = {
        let mut q = app.world_mut().query::<(Entity, &Click)>();
        q.iter(app.world()).find(|(_, c)| matches!(c, Click::Target(Target::HandCard(_)))).map(|(e, _)| e).expect("a hand card")
    };
    press_entity(&mut app, face);
    let model = app.world().resource::<Model>();
    assert!(model.0.sheet.as_ref().is_some_and(|s| s.entries.is_empty()), "the card is open with nothing to do");
    assert_eq!(overlays(&mut app), 1, "the sheet is up");
    assert!(texts(&mut app).iter().any(|t| t.contains("Nothing to do")));
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(overlays(&mut app), 0, "Escape closes it");
    assert_eq!(screen(&app), AppScreen::Game);
    // On the Runner's turn a card with an action still only opens.
    to_the_runners_turn(&mut app);
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
    assert_eq!(model.sheet.as_ref().map(|s| s.entries.clone()), Some(entries.clone()));
    let button = entity_with(&mut app, &Click::Entry(entries[0])).expect("the sheet lists the card's action");
    press_entity(&mut app, button);
    assert_eq!(overlays(&mut app), 0, "the sheet's button submitted and closed the sheet");
    wait_for(&mut app, "the action to be applied", |app| app.world().resource::<Model>().0.applied > before);
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

/// A zone click opens what may be done there and never does it: R&D
/// offers the run, Archives shows its pile, and the count does not move.
#[test]
fn a_zone_click_opens_its_sheet_and_never_acts() {
    let (mut app, _dir) = headless_client();
    start_a_game(&mut app);
    to_the_runners_turn(&mut app);
    let before = app.world().resource::<Model>().0.applied;
    let rnd = entity_with(&mut app, &Click::Target(Target::Server(ServerId::RnD))).expect("R&D's header");
    press_entity(&mut app, rnd);
    let model = &app.world().resource::<Model>().0;
    assert!(model.awaiting && model.applied == before, "nothing was sent");
    let sheet = model.sheet.clone().expect("the zone sheet is open");
    assert!(sheet.entries.iter().any(|i| matches!(model.actions.entries[*i].action, PlayerAction::InitiateRun { server: ServerId::RnD })), "R&D offers the run");
    assert!(texts(&mut app).iter().any(|t| t.contains("in an order nobody is shown")), "a deck's order is never shown");
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    let archives = entity_with(&mut app, &Click::Target(Target::Server(ServerId::Archives))).expect("Archives' header");
    press_entity(&mut app, archives);
    assert!(texts(&mut app).iter().any(|t| t == "Archives"), "the sheet is headed by the zone");
    assert_eq!(app.world().resource::<Model>().0.applied, before);
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    // The Runner's own piles are buttons in the strip; the stack's sheet
    // offers the draw.
    let stack = entity_with(&mut app, &Click::Target(Target::Pile(netrunner_client::board::Pile::Stack))).expect("the stack is a button");
    press_entity(&mut app, stack);
    let model = &app.world().resource::<Model>().0;
    assert!(model.sheet.as_ref().unwrap().entries.iter().any(|i| matches!(model.actions.entries[*i].action, PlayerAction::DrawCardClick { .. })));
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
