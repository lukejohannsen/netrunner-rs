//! The client, without a window: the plugin set `main` runs, under
//! `MinimalPlugins`, driven by messages and key presses.
//!
//! What this pins is the shape every screen shares — boot lands on the
//! menu, a `Navigate` enters a screen under a root that `DespawnOnExit`
//! takes down, Escape leads back — so a screen that breaks the pattern
//! fails here before it fails on a desk. The `ClientCore` is pointed at
//! a temp directory so the test never reads the developer's own files.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use netrunner_desktop::core::ClientCore;
use netrunner_desktop::nav::Navigate;
use netrunner_desktop::{AppScreen, NetrunnerDesktopPlugins};

fn headless_client() -> (App, std::path::PathBuf) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("netrunner_desktop_test_{}_{n}", std::process::id()));
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin, InputPlugin));
    app.insert_resource(ClientCore::in_dir(dir.clone()));
    app.add_plugins(NetrunnerDesktopPlugins);
    (app, dir)
}

/// A key tap the way winit delivers one: a press and a release, as
/// messages the input plugin turns into `ButtonInput` state in
/// `PreUpdate`, so `just_pressed` is true for exactly the next `Update`
/// and the key is not still held when the next tap comes.
fn press(app: &mut App, key_code: KeyCode, logical_key: Key) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: logical_key.clone(),
            state,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    }
}

fn screen(app: &App) -> AppScreen {
    *app.world().resource::<State<AppScreen>>().get()
}

fn roots(app: &mut App, screen: AppScreen) -> usize {
    app.world_mut().query::<&DespawnOnExit<AppScreen>>().iter(app.world()).filter(|root| root.0 == screen).count()
}

/// The browser's thumbs in grid order — their parent's child order,
/// because a query iterates by table and an outlined thumb sits in a
/// different table from the rest.
fn thumbs_in_order(app: &mut App) -> Vec<(Entity, netrunner_core::card::CardId)> {
    use netrunner_desktop::screens::card_browser::FaceButton;
    let mut thumbs: Vec<(Entity, netrunner_core::card::CardId, Entity)> =
        app.world_mut().query::<(Entity, &FaceButton, &ChildOf)>().iter(app.world()).map(|(e, f, p)| (e, f.0, p.parent())).collect();
    let order = |app: &App, entity: Entity, parent: Entity| app.world().get::<Children>(parent).map_or(usize::MAX, |c| c.iter().position(|child| child == entity).unwrap_or(usize::MAX));
    thumbs.sort_by_key(|(e, _, p)| order(app, *e, *p));
    thumbs.into_iter().map(|(e, code, _)| (e, code)).collect()
}

#[test]
fn boot_lands_on_the_main_menu_with_its_root_spawned() {
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
    assert_eq!(roots(&mut app, AppScreen::MainMenu), 1);
}

#[test]
fn navigating_swaps_the_screen_root_and_escape_leads_back() {
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::Profile));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Profile);
    assert_eq!(roots(&mut app, AppScreen::Profile), 1, "the profile spawned under its root");
    assert_eq!(roots(&mut app, AppScreen::MainMenu), 0, "the menu's root was despawned on exit");

    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
    assert_eq!(roots(&mut app, AppScreen::Profile), 0);
    assert_eq!(roots(&mut app, AppScreen::MainMenu), 1);
}

/// Every screen a menu entry leads to exists, so no entry is a dead end.
#[test]
fn every_screen_can_be_entered_and_left() {
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    for target in [
        AppScreen::Profile,
        AppScreen::Settings,
        AppScreen::Decks,
        AppScreen::DeckEditor,
        AppScreen::CardBrowser,
        AppScreen::NewGame,
        AppScreen::Learn,
        AppScreen::Online,
        AppScreen::Game,
        AppScreen::Replay,
    ] {
        app.world_mut().write_message(Navigate(target));
        app.update();
        app.update();
        assert_eq!(screen(&app), target);
        assert_eq!(roots(&mut app, target), 1, "{target:?} spawned under one root");
        app.world_mut().write_message(Navigate(AppScreen::MainMenu));
        app.update();
        app.update();
        assert_eq!(roots(&mut app, target), 0, "{target:?} was taken down");
    }
}

/// Escape on the main menu goes nowhere: quitting is an entry, never a
/// stray key.
#[test]
fn escape_on_the_main_menu_stays() {
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
}

/// The settings screen's name field: Edit opens it, typing fills it,
/// Escape cancels it *without* leaving the screen (the field captures
/// input), and a second Escape then leaves.
#[test]
fn a_text_field_captures_escape_until_it_closes() {
    use netrunner_desktop::widgets::text_field::TextField;
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::Settings));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Settings);
    // Press the Edit button the way a pointer would: find the themed
    // button whose label reads "Edit" and mark it pressed.
    let mut buttons = app.world_mut().query::<(Entity, &netrunner_desktop::widgets::Themed, &Children)>();
    let mut texts = app.world_mut().query::<&Text>();
    let edit = buttons
        .iter(app.world())
        .find(|(_, _, children)| children.iter().any(|child| texts.get(app.world(), child).is_ok_and(|text| text.0 == "Edit")))
        .map(|(entity, _, _)| entity)
        .expect("the settings screen has an Edit button");
    app.world_mut().entity_mut(edit).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).count(), 1, "Edit opened the name field");
    press(&mut app, KeyCode::KeyC, Key::Character("c".into()));
    app.update();
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).next().unwrap().text, "c");
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Settings, "Escape cancelled the edit, not the screen");
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).count(), 0);
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
}

/// The card browser: every printing is a face, a face opens in the
/// inspector, the arrows move the open card, typing narrows the grid,
/// and Escape clears, closes, then leaves — three presses, because the
/// search field captures the first two.
#[test]
fn the_card_browser_lists_faces_filters_as_typed_and_escapes_in_three() {
    use netrunner_desktop::screens::card_browser::FaceButton;
    use netrunner_desktop::widgets::text_field::TextField;
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::CardBrowser));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::CardBrowser);
    let faces = |app: &mut App| app.world_mut().query::<&FaceButton>().iter(app.world()).count();
    let all = faces(&mut app);
    assert!(all > 200, "{all} faces: every printing in the catalog");
    let texts = |app: &mut App| app.world_mut().query::<&Text>().iter(app.world()).map(|t| t.0.clone()).collect::<Vec<_>>();
    // The first card's numbers line, whatever kind of card it is: the
    // line comes from `card_face::Face`, so an agenda leads with its
    // advancement requirement and an identity with its deck size rather
    // than the `Cost 0` the browser used to print on both.
    let first = thumbs_in_order(&mut app)[0].1;
    let numbers = {
        let core = app.world().resource::<ClientCore>();
        let card = core.registry.get_by_numeric_id(first).expect("the thumb's card is in the registry");
        netrunner_client::card_face::Face::of(card).numbers_line()
    };
    assert!(texts(&mut app).contains(&numbers), "the first card is open in the inspector from the start: {numbers}");

    // Press a thumb the way a pointer would: the second, so the
    // inspector visibly changes.
    let (second, code) = thumbs_in_order(&mut app)[1];
    app.world_mut().entity_mut(second).insert(Interaction::Pressed);
    app.update();
    app.update();
    let spans = |app: &mut App| app.world_mut().query::<&TextSpan>().iter(app.world()).map(|t| t.0.clone()).collect::<Vec<_>>();
    assert!(spans(&mut app).iter().any(|t| t.ends_with(&format!("#{:05}", code.0))), "the inspector shows the pressed card's code");
    assert!(texts(&mut app).iter().any(|t| t.starts_with("Legal in ")), "and which formats allow it");
    let outlined = |app: &mut App| app.world_mut().query_filtered::<&FaceButton, With<Outline>>().iter(app.world()).map(|f| f.0).collect::<Vec<_>>();
    assert_eq!(outlined(&mut app), vec![code], "the pressed thumb is the outlined one");

    // The arrows move the open card: Right to the third, Left back, and
    // the outline follows without the grid being respawned.
    let third = thumbs_in_order(&mut app)[2].1;
    press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
    app.update();
    app.update();
    assert_eq!(outlined(&mut app), vec![third], "Right moved the selection one on");
    assert!(spans(&mut app).iter().any(|t| t.ends_with(&format!("#{:05}", third.0))), "and the inspector followed");
    assert_eq!(second, thumbs_in_order(&mut app)[1].0, "the thumbs were not respawned");
    press(&mut app, KeyCode::ArrowLeft, Key::ArrowLeft);
    app.update();
    app.update();
    assert_eq!(outlined(&mut app), vec![code]);

    // Type into the search field, which is open from the start.
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).count(), 1);
    for c in ["t", "i", "t", "h", "e"] {
        press(&mut app, KeyCode::KeyT, Key::Character(c.into()));
        app.update();
    }
    app.update();
    let narrowed = faces(&mut app);
    assert!(narrowed < all && narrowed >= 1, "{narrowed} of {all} faces match \"tithe\"");

    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::CardBrowser, "the first Escape clears the search");
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).next().unwrap().text, "");
    assert_eq!(faces(&mut app), all, "the grid is whole again");
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::CardBrowser, "the second closes the field");
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).count(), 0);
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu, "the third leaves");
    assert_eq!(roots(&mut app, AppScreen::CardBrowser), 0);
}

/// The filter drop-downs: pressing a head opens its list, choosing an
/// entry narrows the grid and closes the list, and Escape closes an
/// open list before the search field gets it.
#[test]
fn the_card_browser_filters_through_drop_downs_that_escape_closes_first() {
    use netrunner_desktop::screens::card_browser::{FaceButton, Filter};
    use netrunner_desktop::widgets::dropdown::{Dropdown, Head, Item};
    use netrunner_desktop::widgets::text_field::TextField;
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::CardBrowser));
    app.update();
    app.update();
    let faces = |app: &mut App| app.world_mut().query::<&FaceButton>().iter(app.world()).count();
    let all = faces(&mut app);
    let items = |app: &mut App| app.world_mut().query::<&Item>().iter(app.world()).count();
    assert_eq!(items(&mut app), 0, "every list starts closed");
    assert_eq!(app.world_mut().query::<&Filter>().iter(app.world()).count(), 5, "one drop-down per filter");

    let side = app.world_mut().query::<(Entity, &Filter)>().iter(app.world()).find(|(_, f)| **f == Filter::Side).map(|(e, _)| e).unwrap();
    let head = app.world_mut().query::<(Entity, &Head)>().iter(app.world()).find(|(_, h)| h.0 == side).map(|(e, _)| e).unwrap();
    app.world_mut().entity_mut(head).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(items(&mut app), 3, "Both, Corp, Runner");
    assert!(app.world_mut().query::<&Dropdown>().iter(app.world()).any(|d| d.open));

    // Type with the list open ("dam", which both sides print), then
    // Escape: the list closes and the search keeps its text.
    for c in ["d", "a", "m"] {
        press(&mut app, KeyCode::KeyD, Key::Character(c.into()));
        app.update();
    }
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(items(&mut app), 0, "Escape closed the list");
    assert_eq!(screen(&app), AppScreen::CardBrowser);
    assert_eq!(app.world_mut().query::<&TextField>().iter(app.world()).next().unwrap().text, "dam", "the field did not also clear");
    let narrowed = faces(&mut app);
    assert!(narrowed < all);

    // Open it again and choose Runner: the grid narrows to the Runner's
    // cards that match, and the list closes.
    let head = app.world_mut().query::<(Entity, &Head)>().iter(app.world()).find(|(_, h)| h.0 == side).map(|(e, _)| e).unwrap();
    app.world_mut().entity_mut(head).insert(Interaction::Pressed);
    app.update();
    app.update();
    let runner = app.world_mut().query::<(Entity, &Item)>().iter(app.world()).find(|(_, i)| i.0 == side && i.1 == 2).map(|(e, _)| e).unwrap();
    app.world_mut().entity_mut(runner).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(items(&mut app), 0, "choosing closed the list");
    let runner_faces = faces(&mut app);
    assert!(runner_faces < narrowed && runner_faces > 0, "{runner_faces} Runner faces of {narrowed}");
    let side = app.world_mut().query::<(Entity, &Filter)>().iter(app.world()).find(|(_, f)| **f == Filter::Side).map(|(e, _)| e).unwrap();
    assert_eq!(app.world_mut().query::<&Dropdown>().get(app.world(), side).unwrap().selected, 2, "the respawned drop-down shows Runner");
}

/// A deck list is browsed with the keys as well as the pointer: the
/// arrows move a highlight from the chosen deck, Enter chooses it and
/// closes the list, and the form's own deck changes with it. The deck
/// lists sit at the foot of the form, where a list that ran off the
/// window left the pointer nothing to reach; the keys reach every deck
/// whatever the window.
#[test]
fn a_deck_list_is_browsed_and_chosen_with_the_keys() {
    use netrunner_client::start::Pane;
    use netrunner_desktop::screens::new_game::PaneDropdown;
    use netrunner_desktop::widgets::dropdown::{Dropdown, Head, Item};
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::NewGame));
    app.update();
    app.update();
    let own = |app: &mut App| app.world_mut().query::<(Entity, &PaneDropdown)>().iter(app.world()).find(|(_, p)| p.0 == Pane::OwnDeck).map(|(e, _)| e).unwrap();
    let root = own(&mut app);
    let (before, decks) = {
        let dropdown = app.world_mut().query::<&Dropdown>().get(app.world(), root).unwrap();
        (dropdown.selected, dropdown.choices.len())
    };
    assert!(decks > 3, "{decks} decks");
    let head = app.world_mut().query::<(Entity, &Head)>().iter(app.world()).find(|(_, h)| h.0 == root).map(|(e, _)| e).unwrap();
    app.world_mut().entity_mut(head).insert(Interaction::Pressed);
    app.update();
    app.update();
    assert_eq!(app.world_mut().query::<&Item>().iter(app.world()).count(), decks, "every deck is in the list");

    // Home, then down twice: the third deck, whatever was chosen before.
    press(&mut app, KeyCode::Home, Key::Home);
    app.update();
    press(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
    press(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
    app.update();
    assert_eq!(app.world_mut().query::<&Dropdown>().get(app.world(), root).unwrap().highlight, 2);
    press(&mut app, KeyCode::Enter, Key::Enter);
    app.update();
    app.update();
    assert_eq!(app.world_mut().query::<&Item>().iter(app.world()).count(), 0, "Enter closed the list");
    assert_eq!(screen(&app), AppScreen::NewGame, "and started nothing");
    // The form respawns on a change, so the drop-down is found again.
    let root = own(&mut app);
    let after = app.world_mut().query::<&Dropdown>().get(app.world(), root).unwrap().selected;
    assert_eq!(after, 2, "the third deck is chosen (it was {before})");
}

/// About credits every third party the register names, bundled or
/// fetched, and Escape leads back to the menu like any other screen.
#[test]
fn about_credits_every_owner_in_the_register() {
    use netrunner_desktop::screens::about::CreditLine;
    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::About));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::About);
    let lines: Vec<String> = app.world_mut().query::<&CreditLine>().iter(app.world()).map(|l| l.0.clone()).collect();
    let credits = netrunner_desktop::credits::bundled();
    let others = netrunner_desktop::credits::fetched().into_iter().chain(netrunner_desktop::credits::software());
    for owner in credits.iter().map(|a| a.owner.clone()).chain(others.map(|c| c.owner)) {
        assert!(lines.iter().any(|l| l.contains(&owner)), "About does not credit {owner}: {lines:?}");
    }
    for website in credits.iter().map(|a| a.website.clone()) {
        assert!(lines.iter().any(|l| l.contains(&website)), "About does not link {website}");
    }
    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu);
}

/// The splash is skipped by a key, and moves on by itself once it has
/// been up long enough — the fonts count as loaded with no asset server,
/// which is the headless case. Boot itself never comes here without a
/// window, so it is entered the way a screen is.
#[test]
fn the_splash_moves_on_by_itself_or_on_a_key() {
    use bevy::time::TimeUpdateStrategy;
    use netrunner_desktop::screens::splash::SPLASH_MIN;

    let (mut app, _dir) = headless_client();
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu, "no window, no splash");

    app.world_mut().write_message(Navigate(AppScreen::Splash));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Splash);
    assert_eq!(roots(&mut app, AppScreen::Splash), 1);
    press(&mut app, KeyCode::Space, Key::Space);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::MainMenu, "any key skips it");
    assert_eq!(roots(&mut app, AppScreen::Splash), 0);

    app.insert_resource(TimeUpdateStrategy::ManualDuration(SPLASH_MIN / 4));
    app.world_mut().write_message(Navigate(AppScreen::Splash));
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Splash, "it holds for a moment");
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(screen(&app), AppScreen::MainMenu, "and then goes on without a key");
}

/// Presses an entity the way a pointer would, and lets the press land.
fn tap(app: &mut App, entity: Entity) {
    app.world_mut().entity_mut(entity).insert(Interaction::Pressed);
    app.update();
    // The press may have left the screen, taking the button with it.
    if let Ok(mut button) = app.world_mut().get_entity_mut(entity) {
        button.insert(Interaction::None);
    }
    app.update();
    app.update();
}

fn find<C: Component + Clone>(app: &mut App, wanted: impl Fn(&C) -> bool) -> Option<Entity> {
    app.world_mut().query::<(Entity, &C)>().iter(app.world()).find(|(_, c)| wanted(c)).map(|(e, _)| e)
}

fn open_decks(app: &mut App) {
    app.update();
    app.update();
    app.world_mut().write_message(Navigate(AppScreen::Decks));
    app.update();
    app.update();
    assert_eq!(screen(app), AppScreen::Decks);
}

/// The whole of building from a published list: Copy to edit on a
/// built-in tile saves a copy and opens it, a press on a pool card adds
/// a copy, and the file on disk has it at once — no Save to forget.
#[test]
fn a_built_in_deck_copies_into_an_editor_that_saves_every_add() {
    use netrunner_desktop::screens::decks::{Model, TileAction, TileButton};
    use netrunner_desktop::screens::deck_editor::{Model as EditorModel, PoolCard};
    let (mut app, dir) = headless_client();
    open_decks(&mut app);
    let row = app.world().resource::<Model>().0.rows.iter().position(|row| row.deck.id == "stolen_goods").unwrap();
    let copy = find::<TileButton>(&mut app, |button| button.row == row && button.action == TileAction::Copy).expect("a Copy to edit button");
    tap(&mut app, copy);
    assert_eq!(screen(&app), AppScreen::DeckEditor);
    let (id, before, read_only) = {
        let editor = &app.world().resource::<EditorModel>().0;
        (editor.deck().id.clone(), editor.deck().size(), editor.read_only)
    };
    assert!(!read_only, "the copy is the person's own");
    assert!(dir.join("decks").join(format!("{id}.json")).exists(), "the copy was saved before it opened");

    let limit = |app: &App, card: &netrunner_core::dsl::CardId| app.world().resource::<EditorModel>().0.draft.copies(card);
    let card = app.world_mut().query::<&PoolCard>().iter(app.world()).map(|c| c.0.clone()).find(|c| limit(&app, c) == 0).expect("a pool card the deck lacks");
    let face = find::<PoolCard>(&mut app, |c| c.0 == card).unwrap();
    tap(&mut app, face);
    let saved = netrunner_client::deck_store::read_file(&dir.join("decks").join(format!("{id}.json"))).unwrap();
    assert_eq!(saved.size(), before + 1, "the add was written as it was made");

    press(&mut app, KeyCode::Escape, Key::Escape);
    app.update();
    app.update();
    assert_eq!(screen(&app), AppScreen::Decks, "Escape leads back to the shelf");
    assert!(app.world().resource::<Model>().0.rows.iter().any(|row| row.saved && row.deck.id == id));
}

/// A built-in deck's View opens it read-only: no pool to add from.
#[test]
fn a_built_in_deck_opens_read_only() {
    use netrunner_desktop::screens::decks::{Model, TileAction, TileButton};
    use netrunner_desktop::screens::deck_editor::{Model as EditorModel, PoolCard};
    let (mut app, _dir) = headless_client();
    open_decks(&mut app);
    let row = app.world().resource::<Model>().0.rows.iter().position(|row| row.deck.id == "stolen_goods").unwrap();
    let view = find::<TileButton>(&mut app, |button| button.row == row && button.action == TileAction::Open).unwrap();
    tap(&mut app, view);
    assert_eq!(screen(&app), AppScreen::DeckEditor);
    assert!(app.world().resource::<EditorModel>().0.read_only);
    let editor = &app.world().resource::<EditorModel>().0;
    let rows = netrunner_client::deck_builder::entries(editor.deck(), netrunner_client::deck_builder::CardBook::new(&netrunner_client::decks::sample_deck_registry(), &[])).len();
    let spread = app.world_mut().query::<&PoolCard>().iter(app.world()).count();
    assert_eq!(spread, rows, "the deck's own cards, no pool to add from");
}

/// New deck asks the side, then the identity, and opens the empty deck.
#[test]
fn a_new_deck_is_a_side_then_an_identity_and_opens_empty() {
    use netrunner_desktop::screens::decks::{Control, PopupButton};
    use netrunner_desktop::screens::deck_editor::Model as EditorModel;
    let (mut app, dir) = headless_client();
    open_decks(&mut app);
    let new = find::<Control>(&mut app, |c| *c == Control::New).unwrap();
    tap(&mut app, new);
    let runner = find::<PopupButton>(&mut app, |b| *b == PopupButton::Side(netrunner_core::rules::Side::Runner)).expect("the side question");
    tap(&mut app, runner);
    let identity = find::<PopupButton>(&mut app, |b| matches!(b, PopupButton::Identity(_))).expect("the identities");
    tap(&mut app, identity);
    assert_eq!(screen(&app), AppScreen::DeckEditor);
    let editor = &app.world().resource::<EditorModel>().0;
    assert!(editor.deck().cards.is_empty());
    assert_eq!(editor.deck().side, netrunner_core::rules::Side::Runner);
    assert!(!editor.status.standing.is_legal(), "an empty deck is saved all the same");
    assert!(dir.join("decks").join(format!("{}.json", editor.deck().id)).exists());
}

/// Copy to edit inside a read-only deck rebuilds the editor on the copy,
/// with a pool where the notes were.
#[test]
fn copy_to_edit_in_the_viewer_opens_the_copy_with_a_pool() {
    use netrunner_desktop::screens::decks::{Model, TileAction, TileButton};
    use netrunner_desktop::screens::deck_editor::{Control, Model as EditorModel, PoolCard};
    let (mut app, _dir) = headless_client();
    open_decks(&mut app);
    let row = app.world().resource::<Model>().0.rows.iter().position(|row| row.deck.id == "stolen_goods").unwrap();
    let view = find::<TileButton>(&mut app, |button| button.row == row && button.action == TileAction::Open).unwrap();
    tap(&mut app, view);
    let copy = find::<Control>(&mut app, |c| *c == Control::Copy).expect("Copy to edit");
    tap(&mut app, copy);
    assert_eq!(screen(&app), AppScreen::DeckEditor);
    let editor = &app.world().resource::<EditorModel>().0;
    assert!(!editor.read_only);
    assert_ne!(editor.deck().id, "stolen_goods");
    assert_eq!(roots(&mut app, AppScreen::DeckEditor), 1, "the old screen was taken down");
    assert!(app.world_mut().query::<&PoolCard>().iter(app.world()).count() > 20, "the pool is there");
}
