//! Learn to Play on the board, without a window (Phase 7 §6): the tracks
//! screen starts a lesson, the board opens on its words, the coach in the
//! right column narrows what is offered, the lesson plays to its closing
//! words, "Next lesson" puts the next one on a fresh board, and leaving
//! goes back to the tracks.
//!
//! Driven under `MinimalPlugins` like `tests/game.rs`, with the lesson's
//! match thread real. The learner's moves are each step's own solution,
//! chosen off the board's action map — the same list every button on the
//! board is built from — so this is the session crate's completability
//! gate walked through the screen.

use std::time::{Duration, Instant};

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use netrunner_core::tutorial;
use netrunner_desktop::core::ClientCore;
use netrunner_desktop::models::game::{Intent, Outcome};
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::screens::game::{Click, LessonCoach, Model, Overlay};
use netrunner_desktop::screens::learn::LessonButton;
use netrunner_desktop::screens::new_game::ActiveMatch;
use netrunner_desktop::{AppScreen, NetrunnerDesktopPlugins};

fn headless_client() -> App {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("netrunner_desktop_learn_{}_{n}", std::process::id()));
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin));
    let mut core = ClientCore::in_dir(dir);
    core.settings.desktop.animation_speed = 0.0;
    app.insert_resource(core);
    app.add_plugins(NetrunnerDesktopPlugins);
    app.update();
    app.update();
    app
}

fn screen(app: &App) -> AppScreen {
    *app.world().resource::<State<AppScreen>>().get()
}

fn wait_for(app: &mut App, what: &str, mut done: impl FnMut(&mut App) -> bool) {
    let start = Instant::now();
    while !done(app) {
        assert!(start.elapsed() < Duration::from_secs(20), "waited for {what}");
        std::thread::sleep(Duration::from_millis(2));
        app.update();
    }
}

fn press(app: &mut App, entity: Entity) {
    app.world_mut().entity_mut(entity).insert(Interaction::Pressed);
    app.update();
    app.update();
}

fn escape(app: &mut App) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput { key_code: KeyCode::Escape, logical_key: Key::Escape, state, text: None, repeat: false, window: Entity::PLACEHOLDER });
    }
    app.update();
    app.update();
}

fn clicked(app: &mut App, wanted: &Click) -> Option<Entity> {
    let mut query = app.world_mut().query::<(Entity, &Click)>();
    query.iter(app.world()).find(|(_, click)| *click == wanted).map(|(entity, _)| entity)
}

fn overlay_texts(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let Ok(root) = world.query_filtered::<Entity, With<Overlay>>().single(world) else { return Vec::new() };
    let mut texts = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(text) = world.get::<Text>(entity) {
            texts.push(text.0.clone());
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    texts
}

fn lesson_id(app: &App) -> String {
    app.world().resource::<ActiveMatch>().lesson.as_ref().expect("a lesson is on the board").id.clone()
}

/// Plays the lesson on the board to its closing words: at every decision,
/// the step's first solution the board offers.
fn play_to_the_outro(app: &mut App) {
    let id = lesson_id(app);
    let steps = tutorial::by_id(&id).unwrap().steps;
    let mut decisions = 0;
    loop {
        wait_for(app, "a decision or the outro", |app| {
            let game = &app.world().resource::<Model>().0;
            game.awaiting || game.finished()
        });
        let game = &app.world().resource::<Model>().0;
        if game.finished() {
            assert!(game.over.is_none(), "{id}: the match ended before the lesson did");
            return;
        }
        decisions += 1;
        assert!(decisions < 300, "{id}: no end in sight");
        let coaching = game.lesson.as_ref().unwrap().coaching.clone().expect("a lesson's decision is coached");
        let offered: Vec<_> = game.actions.entries.iter().map(|entry| entry.action.clone()).collect();
        assert!(offered.iter().all(|action| game.view.as_ref().unwrap().legal_actions.contains(action)), "{id}: the board offers only legal actions");
        if !coaching.allowed.is_empty() {
            assert_eq!(offered, coaching.allowed, "{id} step {}: the board offers the step's actions", coaching.step);
        }
        let solution = &steps[coaching.step - 1].solution;
        let index = offered.iter().position(|action| solution.contains(action)).unwrap_or_else(|| panic!("{id} step {}: no solution on the board", coaching.step));
        let outcome = app.world_mut().resource_mut::<Model>().0.apply(Intent::Choose(index));
        let Outcome::Submit(action) = outcome else { panic!("{id}: a choice did not submit: {outcome:?}") };
        app.world().resource::<ActiveMatch>().handle.submit(action).unwrap();
        app.update();
    }
}

#[test]
fn a_lesson_is_played_on_the_board_to_its_end_and_goes_on_to_the_next() {
    let mut app = headless_client();
    app.world_mut().write_message(Navigate(AppScreen::Learn));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Learn);

    // Every lesson of both tracks is a button, in track order.
    let buttons: Vec<String> = app.world_mut().query::<&LessonButton>().iter(app.world()).map(|button| button.0.clone()).collect();
    let tracks: Vec<String> = tutorial::track(netrunner_core::rules::Side::Corp).into_iter().chain(tutorial::track(netrunner_core::rules::Side::Runner)).map(|lesson| lesson.id).collect();
    assert_eq!(buttons.len(), tracks.len());
    assert!(tracks.iter().all(|id| buttons.contains(id)));

    let first = &tracks[0];
    let button = app.world_mut().query::<(Entity, &LessonButton)>().iter(app.world()).find(|(_, button)| &button.0 == first).unwrap().0;
    press(&mut app, button);
    wait_for(&mut app, "the board", |app| screen(app) == AppScreen::Game);
    assert_eq!(&lesson_id(&app), first);

    // The board opens on the lesson's words, and Begin puts them away.
    let title = tutorial::by_id(first).unwrap().title;
    assert!(overlay_texts(&mut app).contains(&title), "{:?}", overlay_texts(&mut app));
    let begin = clicked(&mut app, &Click::BeginLesson).expect("the intro has Begin");
    press(&mut app, begin);
    assert!(!app.world().resource::<Model>().0.intro_open());

    // The coach is at the head of the rail once a decision is asked.
    wait_for(&mut app, "the first decision", |app| app.world().resource::<Model>().0.awaiting);
    app.update();
    assert_eq!(app.world_mut().query::<&LessonCoach>().iter(app.world()).count(), 1);

    play_to_the_outro(&mut app);
    app.update();
    assert!(overlay_texts(&mut app).iter().any(|text| text.ends_with("— complete")), "{:?}", overlay_texts(&mut app));
    let next = clicked(&mut app, &Click::NextLesson).expect("the first lesson of a track has a next");
    press(&mut app, next);
    wait_for(&mut app, "the next lesson", |app| app.world().get_resource::<ActiveMatch>().and_then(|active| active.lesson.as_ref()).is_some_and(|lesson| lesson.id == tracks[1]) && app.world().get_resource::<Model>().is_some());
    assert_eq!(screen(&app), AppScreen::Game);
    assert!(app.world().resource::<Model>().0.intro_open(), "opening on its own words");

    // Escape on the words leaves for the tracks: nothing has been played.
    escape(&mut app);
    wait_for(&mut app, "the tracks", |app| screen(app) == AppScreen::Learn);
    assert!(app.world().get_resource::<ActiveMatch>().is_none(), "the lesson's match is gone");
}
