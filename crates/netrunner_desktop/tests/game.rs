//! A game, without a window: the form starts a match against the bottom
//! rung on a thread, the board draws the first decision, a press on an
//! action button submits it, and Escape asks before leaving.
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

use netrunner_client::start::{Level, StartChoice, DEFAULT_CORP_DECK, DEFAULT_RUNNER_DECK};
use netrunner_core::rules::Side;
use netrunner_desktop::core::ClientCore;
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::screens::game::{Click, Model, Overlay};
use netrunner_desktop::screens::new_game::{self, ActiveMatch, LastGame};
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
    assert_eq!(entries, model.0.actions.entries.len(), "one button per legal action");
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
