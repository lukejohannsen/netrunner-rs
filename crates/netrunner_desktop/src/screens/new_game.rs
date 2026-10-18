//! The new-game form: chair, rung, style and the two decks, over
//! `netrunner_client::start::StartMenu` — the state machine the
//! terminal's form runs, so the two clients offer the same lists, suggest
//! the same rung and default to the same decks.
//!
//! Each choice is drawn the way it is best chosen, rather than five
//! drop-downs in a row (the first cut, which read as a settings file):
//! the side is two large cards in the sides' own colours, the rung and
//! the style are rows of pills with the chosen one filled, and the decks —
//! lists too long for pills — are drop-downs, each with the deck's
//! identity and style under it.
//!
//! Start puts the match together (`MatchHandle::start_local`, which
//! fails here rather than on the board: a deck that will not validate or
//! a record file that will not load is a notice under the form) and
//! leaves it in [`ActiveMatch`] for the game screen. When a game ends or
//! is left, the form reopens on the game just played with the rung moved
//! to the new suggestion — after a game, Start is "play again".

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;

use netrunner_client::decks::decks_for_match;
use netrunner_client::play::{LocalMatchSpec, MatchHandle, RecordFile};
use netrunner_core::tutorial::Lesson;
use netrunner_client::card_face::Face;
use netrunner_client::start::{DeckRow, Level, OpponentChoice, Pane, StartChoice, StartMenu, DEFAULT_CORP_DECK, DEFAULT_RUNNER_DECK};

use crate::card_images::CardImages;
use crate::screens::deck_picker::{DeckPicked, OpenDeckPicker, Purpose};
use crate::widgets::card_face::{spawn_face, FaceSize};

use crate::audio::{ButtonSound, Sfx};
use crate::core::TokioRuntime;
use netrunner_core::rules::Side;

use crate::core::ClientCore;
use crate::nav::{screen_root, Detour, Navigate};
use crate::screens::AppScreen;
use crate::skin::{Drawn, Slot};
use crate::theme::{size, Theme};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct NewGamePlugin;

impl Plugin for NewGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::NewGame), spawn).add_systems(Update, (controls, dev_opens_picker, refresh).chain().run_if(in_state(AppScreen::NewGame)));
    }
}

/// The match a Start put together, for the game screen: the handle, and
/// the choice the form reopens on afterwards (`None` for a dev game and a
/// lesson).
#[derive(Resource)]
pub struct ActiveMatch {
    pub handle: MatchHandle,
    pub choice: Option<StartChoice>,
    /// The lesson being played, when this is one (`screens::learn`): the
    /// board reads its words, and leaving goes back to Learn to Play.
    pub lesson: Option<Lesson>,
    /// The starter game being played, when this is one: Play again deals
    /// it again, and leaving goes back to Learn to Play.
    pub starter: Option<Starter>,
    /// A game at a host (`screens::online`): leaving goes back to Play
    /// Online, and the server, when this person hosts, rides here for as
    /// long as the match does.
    pub online: Option<crate::screens::online::OnlineMatch>,
}

/// A starter game, as Learn to Play names it: the person's chair, and
/// whether it is the booster-staged pair at 7 points rather than the
/// starter lists at 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Starter {
    pub side: Side,
    pub boosted: bool,
}

/// The last game played, left by the game screen; the form resumes from
/// it.
#[derive(Resource, Debug, Clone)]
pub struct LastGame(pub StartChoice);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Start,
    Back,
    /// No model is set up: opens the AI opponents screen, whose Back
    /// comes back here (`nav::Detour`).
    SetUpModel,
}

/// A deck slot's Change… button: which of the two deck panes it opens
/// the deck picker for (`screens::deck_picker`, Phase 7 §16). The slot
/// replaced a drop-down of names.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneSlot(pub Pane);

/// One choice drawn as a button of its own — a side card, a rung or a
/// style pill: its pane and its row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneChoice {
    pub pane: Pane,
    pub index: usize,
}

/// The form's sections, respawned on every change: a choice restyles
/// its row and the chair changes every other list.
#[derive(Component)]
struct Form;
#[derive(Component)]
struct NoticeLine;

#[derive(Resource)]
struct Model(StartMenu);

#[derive(Resource, Default)]
struct Dirty {
    form: bool,
    notice: Option<String>,
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, images: Res<CardImages>, last: Option<Res<LastGame>>) {
    commands.init_resource::<Dirty>();
    let (mut menu, notice) = match open_menu(&core) {
        Ok(menu) => (menu, String::new()),
        Err(error) => (StartMenu::with_decks([Vec::new(), Vec::new()], [Level::Operator; 2], defaults(&core)), format!("Decks could not be listed: {error}")),
    };
    if let Some(last) = last {
        menu.resume_from(&last.0);
    }
    let form = commands.spawn((Form, Node { flex_direction: FlexDirection::Column, row_gap: px(22), ..default() })).id();
    commands.entity(form).with_children(|parent| spawn_form(parent, &theme, &core, &images, &menu));
    let buttons = commands
        .spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::FlexEnd, column_gap: px(12), margin: UiRect::top(px(8)), ..default() })
        .with_children(|parent| {
            parent.spawn(widgets::styled_button(&theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
            parent.spawn(widgets::styled_button(&theme, ButtonKind::Primary, "Start", px(200), Control::Start));
        })
        .id();
    let mut panel = commands.spawn(widgets::roomy_panel(&theme, px(1000)));
    let panel = panel.add_child(form).add_child(buttons).id();
    let column = commands
        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(16), margin: UiRect::vertical(Val::Auto), max_width: percent(100), ..default() })
        .with_children(|parent| {
            parent.spawn(widgets::title(&theme, AppScreen::NewGame.title()));
            parent.spawn(widgets::dim(&theme, "A casual game against a rung of the ladder, in the style your deck chooses. A move can always be taken back."));
        })
        .add_child(panel)
        .with_children(|parent| {
            parent.spawn(widgets::notice(&theme, notice, NoticeLine));
        })
        .id();
    commands.spawn(screen_root(AppScreen::NewGame, &theme)).add_child(column);
    commands.insert_resource(Model(menu));
}

/// Where the deck cursors start: the active deck a side (the one last
/// chosen anywhere, Phase 7 §16), else the built-in pair a flag-less
/// game plays.
fn defaults(core: &ClientCore) -> [String; 2] {
    [core.settings.active_deck_or_default(Side::Corp), core.settings.active_deck_or_default(Side::Runner)]
}

/// The form from the client's files: the saved decks beside the built-in
/// ones, and the record's suggestion for each chair.
fn open_menu(core: &ClientCore) -> Result<StartMenu, String> {
    // No data directory means no saved decks; the built-in ones are still
    // listed, so any path that does not exist will do.
    let decks_dir = core.decks_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("netrunner-no-decks"));
    let format = core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT);
    StartMenu::open(&decks_dir, core.record_path.as_deref(), &core.player_name(), &core.registry, format, defaults(core), core.settings.opponent_names())
}

/// What each side does, in a line, on its card.
fn side_blurb(side: Side) -> &'static str {
    match side {
        Side::Corp => "Build servers, protect your agendas and score seven points.",
        Side::Runner => "Break in, steal agendas and trash what the Corp builds.",
    }
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// A labelled section of the form: an overline over its content.
fn section(parent: &mut ChildSpawnerCommands, theme: &Theme, name: impl Into<String>, content: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(10), ..default() }).with_children(|section| {
        section.spawn(widgets::overline(theme, name));
        content(section);
    });
}

/// A row of pills, one per choice, the chosen one filled; it wraps
/// rather than overflowing.
fn pills(parent: &mut ChildSpawnerCommands, theme: &Theme, pane: Pane, labels: Vec<String>, cursor: usize) {
    parent.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() }).with_children(|row| {
        for (index, label) in labels.into_iter().enumerate() {
            let kind = if index == cursor { ButtonKind::Primary } else { ButtonKind::Secondary };
            row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, (PaneChoice { pane, index }, ButtonSound(Sfx::Toggle))));
        }
    });
}

/// The side's card: its name in its own colour over what it does,
/// ringed in the accent when chosen and dimmed when not.
fn side_card(parent: &mut ChildSpawnerCommands, theme: &Theme, side: Side, index: usize, chosen: bool) {
    let colour = theme.side(side);
    let (fill, rim, hover) = if chosen {
        (colour.with_alpha(0.28), theme.accent, colour.with_alpha(0.34))
    } else {
        (theme.secondary.with_alpha(0.06), theme.glass_border, colour.with_alpha(0.16))
    };
    let drawn = Drawn::new(fill, rim);
    parent
        .spawn((
            Button,
            widgets::Themed,
            PaneChoice { pane: Pane::Chair, index },
            ButtonSound(Sfx::Toggle),
            Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                min_height: px(118),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(18),
                padding: UiRect::horizontal(px(24)),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(16)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(rim),
            widgets::Dressed { slot: Slot::Button, drawn, hover: Some(Drawn::new(hover, theme.border_hover)), pressed: Some(Drawn::new(colour.with_alpha(0.40), theme.accent)) },
        ))
        .with_children(|card| {
            // The side's own colour as a short bar beside its name, inset
            // from the edge: a strip down the edge itself ran over the
            // rounded corners, because `Overflow::clip` clips to the
            // rectangle and not to the radius.
            card.spawn((
                Node { width: px(6), height: px(64), flex_shrink: 0.0, border_radius: BorderRadius::all(px(3)), ..default() },
                BackgroundColor(if chosen { colour } else { colour.with_alpha(0.55) }),
            ));
            card.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: px(6), padding: UiRect::vertical(px(16)), ..default() })
                .with_children(|words| {
                    let name_colour = if chosen { theme.text } else { theme.text_dim };
                    words.spawn((Text::new(format!("{side:?}")), theme.font(size::HEADING), TextColor(name_colour)));
                    words.spawn((Text::new(side_blurb(side)), theme.font(size::SMALL), TextColor(theme.text_dim)));
                });
        });
}

/// A deck slot: the chosen deck as a person knows it — its identity's
/// face, its name, the identity, the style a bot plays it in, and why it
/// cannot start a game here when it cannot — with Change…, which opens
/// the deck picker (`screens::deck_picker`). A saved deck that cannot
/// start a game is still shown here if it was chosen, marked, so a
/// person finds the deck they built and reads why; Start refuses it with
/// that reason.
fn deck_slot(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, pane: Pane, title: String, decks: &[DeckRow], cursor: usize) {
    parent.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), row_gap: px(10), ..default() }).with_children(|column| {
        column.spawn(widgets::overline(theme, title));
        let Some(deck) = decks.get(cursor) else {
            column.spawn(widgets::dim(theme, "No deck for this side: build one under Decks."));
            return;
        };
        let identity = core.registry.get(&deck.deck.identity);
        let faction = theme.faction(deck.faction);
        column
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(14),
                    padding: UiRect::all(px(10)),
                    border: UiRect { left: px(4), right: px(1), top: px(1), bottom: px(1) },
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
                BackgroundColor(theme.glass),
                BorderColor { left: faction, right: theme.glass_border, top: theme.glass_border, bottom: theme.glass_border },
            ))
            .with_children(|tile| {
                match identity {
                    Some(card) => {
                        let drawn = Face::of(card, &core.settings.art);
                        let image = images.of(&drawn, FaceSize::Board(SLOT_FACE));
                        spawn_face(tile, theme, &drawn, FaceSize::Board(SLOT_FACE), image, crate::widgets::reader::Readable(card.id.clone()));
                    }
                    None => {
                        tile.spawn((Node { width: px(SLOT_FACE as f32), height: px(SLOT_FACE as f32 * 1.4), flex_shrink: 0.0, ..default() }, BackgroundColor(theme.panel)));
                    }
                }
                tile.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(3), ..default() }).with_children(|words| {
                    let saved = if deck.saved { "  ·  yours" } else { "" };
                    words.spawn((Text::new(format!("{}{saved}", deck.deck.name)), theme.font(size::BODY), TextColor(theme.text)));
                    words.spawn(widgets::dim(theme, format!("{}  ·  plays {}", deck.identity, deck.style())));
                    if let Some(problem) = deck.problem() {
                        words.spawn((Text::new(format!("Can't start a game: {problem}")), theme.font(size::SMALL), TextColor(theme.danger), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    }
                });
                tile.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Change…", PaneSlot(pane)));
            });
    });
}

/// The identity's face in a deck slot.
const SLOT_FACE: u16 = 64;

fn spawn_form(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, menu: &StartMenu) {
    let human = menu.human();
    let bot = menu.bot();
    section(parent, theme, "Your side", |section| {
        section.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(16), ..default() }).with_children(|row| {
            for (index, side) in [Side::Corp, Side::Runner].into_iter().enumerate() {
                side_card(row, theme, side, index, side == human);
            }
        });
    });
    let suggested = menu.suggested();
    let level = menu.level();
    // The model opponents the settings hold: a row of pills after the
    // chair, the built-in bot first. With none, the built-in bot alone
    // and the way to set one up — this form is where a person looks for
    // an AI opponent, and a section drawn only once one existed hid the
    // whole feature behind Settings.
    let rows = menu.opponent_rows();
    section(parent, theme, format!("Opponent · the {bot:?}"), |section| {
        if rows.len() > 1 {
            pills(section, theme, Pane::Opponent, rows, menu.cursor(Pane::Opponent));
        } else {
            section.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn(widgets::styled_button(theme, ButtonKind::Primary, rows[0].clone(), Val::Auto, (PaneChoice { pane: Pane::Opponent, index: 0 }, ButtonSound(Sfx::Toggle))));
                row.spawn(widgets::styled_button(theme, ButtonKind::Secondary, "Set up an AI model…", Val::Auto, Control::SetUpModel));
            });
            section.spawn((
                widgets::dim(theme, "A language model can play the other chair on your own API key (Anthropic, OpenAI, Gemini) or locally through Ollama."),
                TextLayout::new(Justify::Left, LineBreak::WordBoundary),
            ));
        }
    });
    if let OpponentChoice::Model(name) = menu.opponent() {
        section(parent, theme, "Opponent level", |section| {
            section.spawn((
                widgets::dim(theme, format!("{name} plays every decision it is asked about; the built-in {} planner plays the rest, and any decision the model fails.", capitalised(suggested.name()))),
                TextLayout::new(Justify::Left, LineBreak::WordBoundary),
            ));
        });
    }
    if menu.opponent() == OpponentChoice::BuiltIn {
    section(parent, theme, format!("Opponent level · the {bot:?}"), |section| {
        let labels = Level::ALL
            .iter()
            .map(|l| {
                let mark = if *l == suggested { "  ·  suggested" } else { "" };
                format!("{}  {}{mark}", l.rung(), capitalised(l.name()))
            })
            .collect();
        pills(section, theme, Pane::Level, labels, menu.cursor(Pane::Level));
        section.spawn(widgets::dim(theme, format!("{}: {}.", capitalised(level.name()), level.spec(bot).describe())));
    });
    section(parent, theme, "Opponent style", |section| {
        let labels = menu
            .styles()
            .into_iter()
            .map(|style| match style {
                None => "Deck's own".to_string(),
                Some(style) => capitalised(&style.to_string()),
            })
            .collect();
        pills(section, theme, Pane::Style, labels, menu.cursor(Pane::Style));
    });
    }
    parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(24), ..default() }).with_children(|row| {
        deck_slot(row, theme, core, images, Pane::OwnDeck, format!("Your deck · {human:?}"), menu.own_decks(), menu.cursor(Pane::OwnDeck));
        deck_slot(row, theme, core, images, Pane::OpponentDeck, format!("Opponent's deck · {bot:?}"), menu.opponent_decks(), menu.cursor(Pane::OpponentDeck));
    });
}

/// A seed off the clock: the terminal uses `rand::random`, and the
/// desktop has no reason to carry a random crate for one number a game.
pub fn seed_from_clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64 | 1)
}

/// Puts the match together from a choice and the client's files. Every
/// failure is a `String` for the notice line.
pub fn start(core: &ClientCore, choice: &StartChoice) -> Result<ActiveMatch, String> {
    start_seeded(core, choice, seed_from_clock())
}

/// [`start`] with the runtime a model opponent is reached on. The Start
/// button's path: a model chosen with no runtime is a notice, not a game.
pub fn start_on(core: &ClientCore, choice: &StartChoice, runtime: Option<&TokioRuntime>) -> Result<ActiveMatch, String> {
    start_with(core, choice, seed_from_clock(), core.record_path.clone(), runtime.map(TokioRuntime::handle))
}

/// [`start`] on a given seed: the same deal and the same bot every time,
/// which is what a test needs — a board state the test depends on (an
/// unprotected central, a Corp install by turn one) otherwise holds on
/// some clock seeds and not others, and the test flakes.
pub fn start_seeded(core: &ClientCore, choice: &StartChoice, seed: u64) -> Result<ActiveMatch, String> {
    start_with(core, choice, seed, core.record_path.clone(), None)
}

/// `record` is where the game is logged. Every game a person starts from
/// the form is — there is no unrecorded kind to ask for, since nothing
/// rides on a game against a bot — and `None` is the dev hook's, whose
/// autoplayed games are nobody's record.
fn start_with(core: &ClientCore, choice: &StartChoice, seed: u64, record: Option<std::path::PathBuf>, runtime: Option<tokio::runtime::Handle>) -> Result<ActiveMatch, String> {
    let decks_dir = core.decks_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("netrunner-no-decks"));
    let format = core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT);
    let (corp, runner) = decks_for_match(&decks_dir, &choice.corp_deck, &choice.runner_deck, &core.registry, format)?;
    let record = record.map(|path| RecordFile { path, player: core.player_name() });
    // A model is named on the form and resolved here: the profile from
    // the settings, its key from the secrets file, the planner at the
    // chair's suggested rung behind it (`StartChoice::level`).
    let opponent = match &choice.opponent {
        OpponentChoice::BuiltIn => netrunner_client::play::Opponent::Ladder(choice.level),
        OpponentChoice::Model(name) => {
            let profile = core.settings.opponent(name).cloned().ok_or_else(|| format!("no AI opponent named {name:?} is set up"))?;
            let key = core.load_secrets()?.get(name).cloned();
            let handle = runtime.ok_or_else(|| "there is no runtime to reach the model on".to_string())?;
            netrunner_client::play::Opponent::model_opponent(profile, key, netrunner_client::play::ModelLink::Http(handle), choice.level)
        }
    };
    let spec = LocalMatchSpec {
        registry: Arc::clone(&core.registry),
        corp,
        runner,
        human: choice.human,
        opponent,
        style: choice.style,
        seed,
        rules: Default::default(),
        format,
        record,
    };
    let handle = MatchHandle::start_local(spec)?;
    Ok(ActiveMatch { handle, choice: Some(choice.clone()), lesson: None, starter: None, online: None })
}

/// An unrecorded game on the default decks against the middle rung, the
/// person in `side`'s chair — a test's game.
pub fn start_default(core: &ClientCore, side: Side) -> Result<ActiveMatch, String> {
    start_dev(core, side, None, None)
}

/// `start_default` with either deck replaced — the dev hook's game, so a
/// screen a card reaches (Scatter Field's install) can be shot on a deck
/// that holds the card.
pub fn start_dev(core: &ClientCore, side: Side, corp_deck: Option<&str>, runner_deck: Option<&str>) -> Result<ActiveMatch, String> {
    let choice = StartChoice {
        human: side,
        opponent: netrunner_client::start::OpponentChoice::BuiltIn,
        level: Level::Operator,
        style: None,
        corp_deck: corp_deck.unwrap_or(DEFAULT_CORP_DECK).to_string(),
        runner_deck: runner_deck.unwrap_or(DEFAULT_RUNNER_DECK).to_string(),
    };
    start_with(core, &choice, seed_from_clock(), None, None).map(|active| ActiveMatch { choice: None, ..active })
}

#[allow(clippy::too_many_arguments)]
fn controls(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    mut picked: MessageReader<DeckPicked>,
    marks: Query<&Control>,
    slots: Query<&PaneSlot>,
    choices: Query<&PaneChoice>,
    mut menu: ResMut<Model>,
    mut dirty: ResMut<Dirty>,
    mut core: ResMut<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut navigate: MessageWriter<Navigate>,
    mut detour: MessageWriter<Detour>,
    mut open_picker: MessageWriter<OpenDeckPicker>,
) {
    // A deck chosen in the picker: the pane's cursor goes to it. The
    // picker lists the same rows the form holds, so the id is found.
    for DeckPicked { purpose, id } in picked.read() {
        let Purpose::NewGame(pane) = purpose else { continue };
        let rows = if *pane == Pane::OwnDeck { menu.0.own_decks() } else { menu.0.opponent_decks() };
        if let Some(index) = rows.iter().position(|row| row.deck.id == *id) {
            menu.0.set_cursor(*pane, index);
            dirty.form = true;
        }
    }
    for Pressed(entity) in pressed.read() {
        if let Ok(PaneChoice { pane, index }) = choices.get(*entity) {
            if menu.0.cursor(*pane) != *index {
                menu.0.set_cursor(*pane, *index);
                dirty.form = true;
            }
            continue;
        }
        if let Ok(PaneSlot(pane)) = slots.get(*entity) {
            let (side, rows) = if *pane == Pane::OwnDeck { (menu.0.human(), menu.0.own_decks()) } else { (menu.0.bot(), menu.0.opponent_decks()) };
            let chosen = rows.get(menu.0.cursor(*pane)).map(|row| row.deck.id.clone());
            let format = core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT);
            open_picker.write(OpenDeckPicker { side, format, purpose: Purpose::NewGame(*pane), chosen });
            continue;
        }
        match marks.get(*entity) {
            Ok(Control::Back) => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Ok(Control::SetUpModel) => {
                detour.write(Detour { to: AppScreen::Opponents, back: AppScreen::NewGame });
            }
            Ok(Control::Start) => {
                let Some(choice) = menu.0.choice() else {
                    dirty.notice = Some("No deck to play: the list is empty".to_string());
                    continue;
                };
                if let Some(problem) = menu.0.choice_problem() {
                    dirty.notice = Some(problem);
                    continue;
                }
                match start_on(&core, &choice, runtime.as_deref()) {
                    Ok(active) => {
                        // The decks played are the active decks now,
                        // whichever way they were chosen.
                        let changed = core.settings.set_active_deck(Side::Corp, &choice.corp_deck) | core.settings.set_active_deck(Side::Runner, &choice.runner_deck);
                        if changed {
                            let _ = core.save_settings();
                        }
                        commands.insert_resource(active);
                        navigate.write(Navigate(AppScreen::Game));
                    }
                    Err(error) => dirty.notice = Some(format!("The game could not start: {error}")),
                }
            }
            Err(_) => {}
        }
    }
}

/// `NETRUNNER_DECK_PICKER`: the own-deck slot pressed once the form is
/// up, for a screenshot of the picker.
fn dev_opens_picker(dev: Option<Res<crate::dev::Dev>>, mut opened: Local<bool>, menu: Res<Model>, core: Res<ClientCore>, mut open_picker: MessageWriter<OpenDeckPicker>) {
    if *opened || !dev.is_some_and(|dev| dev.deck_picker) {
        return;
    }
    *opened = true;
    let chosen = menu.0.own_decks().get(menu.0.cursor(Pane::OwnDeck)).map(|row| row.deck.id.clone());
    let format = core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT);
    open_picker.write(OpenDeckPicker { side: menu.0.human(), format, purpose: Purpose::NewGame(Pane::OwnDeck), chosen });
}

fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    form: Query<Entity, With<Form>>,
    mut notice: Query<&mut Text, With<NoticeLine>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    images: Res<CardImages>,
    menu: Res<Model>,
) {
    let Dirty { form: reform, notice: note } = std::mem::take(&mut *dirty);
    if reform && let Ok(form) = form.single() {
        commands.entity(form).despawn_children().with_children(|parent| spawn_form(parent, &theme, &core, &images, &menu.0));
    }
    if let Some(note) = note {
        for mut text in &mut notice {
            text.0 = note.clone();
        }
    }
}
