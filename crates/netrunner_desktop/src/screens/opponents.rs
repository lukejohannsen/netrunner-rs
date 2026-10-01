//! The AI opponents screen, off Settings: the model opponents a person
//! plays on their own key (Phase 7 §11, `netrunner_client::llm`).
//!
//! A list of profiles on the left and the open profile's form on the
//! right: a preset to fill it from, the protocol, the name, the server
//! URL, the model, the key (shown as set or not set, typed into a masked
//! field, never read back), the timeout, which decisions the model is
//! asked about, whether it explains itself, a per-game token budget, and
//! a **Test** that sends one trivial request and shows the model's word
//! or the error. Every change saves at once, the settings to
//! `settings.toml` and the key to `secrets.toml` — two files, because the
//! settings file is the one a person pastes into a bug report.
//!
//! The form is `models::opponents` (pure, tested); this file draws it and
//! turns presses into its intents, as the settings screen does. The probe
//! runs on `core::TokioRuntime` and answers through a one-shot channel
//! polled each frame (`Probes`), the `netrunnerdb::Decklists` pattern,
//! and a headless test scripts the answer.

use bevy::prelude::*;
use tokio::sync::oneshot;

use netrunner_client::llm::{self, ApiKey, Asks, LlmProfile, Preset, Protocol};

use crate::core::{ClientCore, Notices, TokioRuntime};
use crate::models::opponents::{self as model, Field, Intent, Opponents};
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::text_field::{TextField, TextFieldEvent};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct OpponentsPlugin;

impl Plugin for OpponentsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Probes>()
            .add_systems(OnEnter(AppScreen::Opponents), spawn)
            .add_systems(Update, (controls, open_field, field_edits, probe_answers, refresh).chain().run_if(in_state(AppScreen::Opponents)));
    }
}

/// The controls, on their buttons.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum Control {
    Intent(Intent),
    Back,
}

/// The form's state, for the screen's lifetime.
#[derive(Resource)]
pub struct Model(pub Opponents);

/// The two columns, respawned on every change.
#[derive(Component)]
struct ProfileList;
#[derive(Component)]
struct Editor;
#[derive(Component)]
struct NoticeLine;

#[derive(Resource, Default)]
struct Dirty(bool);

#[derive(Resource, Default)]
struct EditRequested(Option<Field>);

#[derive(Component, Clone, Copy)]
struct Editing(Field);

/// A probe's answer, in a line for the person.
pub type Answer = Result<String, String>;

enum Pending {
    Channel(oneshot::Receiver<Answer>),
    Ready(Answer),
}

/// The one probe out at a time, and how it is answered: the network, or
/// a scripted answer for a test.
#[derive(Resource, Default)]
pub struct Probes {
    scripted: Option<Answer>,
    pending: Option<Pending>,
}

impl Probes {
    /// Answers every probe with `answer` without sending anything.
    pub fn scripted(answer: Answer) -> Self {
        Probes { scripted: Some(answer), pending: None }
    }

    fn send(&mut self, runtime: Option<&TokioRuntime>, profile: LlmProfile, key: Option<ApiKey>) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("A test is already out".to_string());
        }
        if let Some(answer) = &self.scripted {
            self.pending = Some(Pending::Ready(answer.clone()));
            return Ok(());
        }
        let Some(runtime) = runtime else { return Err("There is no runtime to reach the model on".to_string()) };
        let request = llm::probe(&profile, key.as_ref()).map_err(|error| error.to_string())?;
        let (tx, answer) = oneshot::channel();
        let timeout = profile.timeout();
        runtime.0.spawn(async move {
            let response = llm::transport::send_once(&request, timeout).await;
            let _ = tx.send(llm::probe_answer(&profile, response));
        });
        self.pending = Some(Pending::Channel(answer));
        Ok(())
    }

    fn poll(&mut self) -> Option<Answer> {
        match self.pending.take()? {
            Pending::Ready(answer) => Some(answer),
            Pending::Channel(mut answer) => match answer.try_recv() {
                Ok(answer) => Some(answer),
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.pending = Some(Pending::Channel(answer));
                    None
                }
                Err(oneshot::error::TryRecvError::Closed) => Some(Err("The test stopped without an answer".to_string())),
            },
        }
    }
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, mut notices: ResMut<Notices>) {
    commands.init_resource::<Dirty>();
    commands.init_resource::<EditRequested>();
    let secrets = match core.load_secrets() {
        Ok(secrets) => secrets,
        Err(error) => {
            notices.push(format!("Keys not read: {error}"));
            Default::default()
        }
    };
    let form = Opponents::new(core.settings.opponents.clone(), secrets);
    let list = commands.spawn((ProfileList, widgets::roomy_panel(&theme, px(320)))).id();
    let editor = commands.spawn((Editor, widgets::roomy_panel(&theme, px(640)))).id();
    let columns = commands
        .spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, align_items: AlignItems::FlexStart, column_gap: px(16), row_gap: px(16), ..default() })
        .add_children(&[list, editor])
        .id();
    let saved_where = match (&core.settings_path, core.secrets_path()) {
        (Some(settings), Some(secrets)) => format!("Profiles are saved to {}; keys to {}, readable by you alone", settings.display(), secrets.display()),
        _ => "Not saved: the OS has no data directory, so these apply to this session only".to_string(),
    };
    commands
        .spawn((screen_root(AppScreen::Opponents, &theme), children![
            widgets::heading(&theme, AppScreen::Opponents.title()),
            (widgets::dim(&theme, "A language model in the bot's chair, on your own key: Anthropic, OpenAI, Gemini, Ollama, or any server that speaks the OpenAI shape. Pick it on the Play vs Computer form."), TextLayout::new(Justify::Center, LineBreak::WordBoundary)),
            widgets::dim(&theme, saved_where),
            widgets::button(&theme, "Back", Val::Auto, Control::Back),
            (widgets::notice(&theme, "", NoticeLine), TextLayout::new(Justify::Center, LineBreak::WordBoundary)),
        ]))
        .add_child(columns);
    commands.entity(list).with_children(|parent| spawn_list(parent, &theme, &form));
    commands.entity(editor).with_children(|parent| spawn_editor(parent, &theme, &form));
    commands.insert_resource(Model(form));
}

/// The profiles, each a pill, the open one filled; then one Add per preset.
fn spawn_list(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &Opponents) {
    parent.spawn(widgets::label(theme, "Opponents"));
    if form.profiles.is_empty() {
        parent.spawn((widgets::dim(theme, "None yet. Add one from a preset."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    for (index, row) in form.rows().into_iter().enumerate() {
        let kind = if form.selected == Some(index) { ButtonKind::Primary } else { ButtonKind::Secondary };
        parent.spawn(widgets::styled_button(theme, kind, row, percent(100), Control::Intent(Intent::Select(index))));
    }
    parent.spawn(widgets::overline(theme, "Add"));
    parent.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(8), row_gap: px(8), ..default() }).with_children(|row| {
        for preset in Preset::ALL {
            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, preset.label(), Control::Intent(Intent::Add(preset))));
        }
    });
}

/// A row of pills, the chosen one filled.
fn pills(parent: &mut ChildSpawnerCommands, theme: &Theme, choices: Vec<(String, bool, Intent)>) {
    parent.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(8), row_gap: px(8), ..default() }).with_children(|row| {
        for (label, chosen, intent) in choices {
            let kind = if chosen { ButtonKind::Primary } else { ButtonKind::Secondary };
            row.spawn(widgets::small_button(theme, kind, label, Control::Intent(intent)));
        }
    });
}

/// One labelled row: the label, the value, and the controls after it.
fn row(parent: &mut ChildSpawnerCommands, theme: &Theme, label: &str, value: String, controls: impl FnOnce(&mut ChildSpawnerCommands)) {
    let mut node = parent.spawn((widgets::row(12.0), children![
        (widgets::label(theme, label.to_string()), Node { width: px(150), flex_shrink: 0.0, ..default() }),
        (widgets::label(theme, value), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }, TextLayout::new(Justify::Left, LineBreak::WordBoundary)),
    ]));
    node.entry::<Node>().and_modify(|mut node| node.width = percent(100));
    node.with_children(controls);
}

fn spawn_editor(parent: &mut ChildSpawnerCommands, theme: &Theme, form: &Opponents) {
    let Some(profile) = form.selected() else {
        parent.spawn((widgets::dim(theme, "Add an opponent, or choose one from the list."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        return;
    };
    let preset = Preset::ALL.into_iter().find(|preset| {
        let filled = preset.profile(&profile.name);
        filled.protocol == profile.protocol && filled.url == profile.url
    });
    parent.spawn(widgets::label(theme, format!("{} · {}", profile.name, profile.protocol.label())));
    parent.spawn(widgets::overline(theme, "Fill from a preset"));
    pills(parent, theme, Preset::ALL.into_iter().map(|p| (p.label().to_string(), preset == Some(p), Intent::Preset(p))).collect());
    parent.spawn(widgets::overline(theme, "Protocol"));
    pills(parent, theme, Protocol::ALL.into_iter().map(|p| (p.label().to_string(), profile.protocol == p, Intent::Protocol(p))).collect());
    for field in [Field::Name, Field::Url, Field::Model] {
        row(parent, theme, field.label(), form.value(field), |controls| {
            controls.spawn(widgets::button(theme, "Edit", Val::Auto, Control::Intent(Intent::Edit(field))));
        });
    }
    if let Some(hint) = preset.and_then(Preset::cheap_hint) {
        parent.spawn((widgets::dim(theme, hint), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    let key_value = form.value(Field::Key);
    let has_key = form.has_key(&profile.name);
    let wants_key = preset.is_none_or(Preset::needs_key);
    row(parent, theme, Field::Key.label(), key_value, |controls| {
        controls.spawn(widgets::button(theme, if has_key { "Replace" } else { "Edit" }, Val::Auto, Control::Intent(Intent::Edit(Field::Key))));
        if has_key {
            controls.spawn(widgets::button(theme, "Clear", Val::Auto, Control::Intent(Intent::ClearKey)));
        }
    });
    if !wants_key && !has_key {
        parent.spawn((widgets::dim(theme, "A local server takes no key; one behind a proxy may."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    row(parent, theme, "Timeout", form.timeout_label(), |controls| {
        controls.spawn(widgets::round_button(theme, "<", Control::Intent(Intent::StepTimeout(-1))));
        controls.spawn(widgets::round_button(theme, ">", Control::Intent(Intent::StepTimeout(1))));
    });
    parent.spawn(widgets::overline(theme, "Asked about"));
    pills(parent, theme, Asks::ALL.into_iter().map(|a| (a.label().to_string(), profile.asks == a, Intent::Asks(a))).collect());
    parent.spawn((widgets::dim(theme, profile.asks.note()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    row(parent, theme, "Explains its moves", if profile.explain { "on".to_string() } else { "off".to_string() }, |controls| {
        controls.spawn(widgets::button(theme, "Toggle", Val::Auto, Control::Intent(Intent::ToggleExplain)));
    });
    row(parent, theme, "Budget per game", form.budget_label(), |controls| {
        controls.spawn(widgets::round_button(theme, "<", Control::Intent(Intent::StepBudget(-1))));
        controls.spawn(widgets::round_button(theme, ">", Control::Intent(Intent::StepBudget(1))));
    });
    parent.spawn(widgets::row(12.0)).with_children(|buttons| {
        if form.testing {
            buttons.spawn(widgets::disabled_button(theme, "Testing…", Val::Auto, ()));
        } else {
            buttons.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Test", Val::Auto, Control::Intent(Intent::Test)));
        }
        buttons.spawn(widgets::button(theme, "Remove", Val::Auto, Control::Intent(Intent::Remove)));
    });
    match &form.last_test {
        Some(Ok(line)) => {
            parent.spawn((widgets::dim(theme, line.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
        Some(Err(error)) => {
            parent.spawn((widgets::notice(theme, format!("The test failed: {error}"), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
        None => {}
    }
}

fn controls(
    mut pressed: MessageReader<Pressed>,
    marks: Query<&Control>,
    mut form: ResMut<Model>,
    mut core: ResMut<ClientCore>,
    mut probes: ResMut<Probes>,
    runtime: Option<Res<TokioRuntime>>,
    mut dirty: ResMut<Dirty>,
    mut edit: ResMut<EditRequested>,
    mut notices: ResMut<Notices>,
    mut navigate: MessageWriter<Navigate>,
) {
    for Pressed(entity) in pressed.read() {
        match marks.get(*entity) {
            Ok(Control::Back) => {
                navigate.write(Navigate(AppScreen::Settings));
            }
            Ok(Control::Intent(Intent::Edit(field))) => {
                if form.0.apply(Intent::Edit(*field)).redraw {
                    edit.0 = Some(*field);
                }
            }
            Ok(Control::Intent(intent)) => {
                let outcome = form.0.apply(intent.clone());
                settle(&outcome, &mut form.0, &mut core, &mut probes, runtime.as_deref(), &mut notices);
                dirty.0 |= outcome.redraw;
            }
            Err(_) => {}
        }
    }
}

/// Saves what an outcome says to save and sends the probe it asks for.
fn settle(outcome: &model::Outcome, form: &mut Opponents, core: &mut ClientCore, probes: &mut Probes, runtime: Option<&TokioRuntime>, notices: &mut Notices) {
    if outcome.save_settings {
        core.settings.opponents = form.profiles.clone();
        if let Err(error) = core.save_settings() {
            notices.push(format!("Opponents not saved: {error}"));
        }
    }
    if outcome.save_secrets
        && let Err(error) = core.save_secrets(&form.secrets)
    {
        notices.push(format!("Key not saved: {error}"));
    }
    if let Some((profile, key)) = &outcome.probe
        && let Err(error) = probes.send(runtime, profile.clone(), key.clone())
    {
        form.apply(Intent::Tested(Err(error)));
    }
}

/// Spawns the field for the value Edit was pressed on, under the editor.
fn open_field(mut commands: Commands, mut edit: ResMut<EditRequested>, fields: Query<Entity, With<TextField>>, editor: Query<Entity, With<Editor>>, theme: Res<Theme>, form: Res<Model>) {
    let Some(field) = edit.0.take() else { return };
    if !fields.is_empty() {
        return;
    }
    let Ok(editor) = editor.single() else { return };
    let current = form.0.editable(field);
    let (text_field, hint) = match field {
        Field::Key => (TextField::masked(current.clone(), field.max_len()), "  Type or paste the key. It is kept in secrets.toml, readable by you alone, and never shown again. Enter saves, Escape cancels"),
        Field::Url => (TextField::new(current.clone(), field.max_len()), "  The server's base URL, http:// or https://, without the path. Enter saves, Escape cancels"),
        Field::Model => (TextField::new(current.clone(), field.max_len()), "  The model's id as the provider names it. Enter saves, Escape cancels"),
        Field::Name => (TextField::new(current.clone(), field.max_len()), "  Enter saves, Escape cancels"),
    };
    let shown = text_field.shown();
    commands.entity(editor).with_children(|parent| {
        parent.spawn((
            text_field,
            Editing(field),
            widgets::field_node(Val::Auto),
            BackgroundColor(theme.glass_strong),
            BorderColor::all(theme.accent),
            children![widgets::label(&theme, format!("{shown}|")), (widgets::dim(&theme, hint), TextLayout::new(Justify::Left, LineBreak::WordBoundary))],
        ));
    });
}

fn field_edits(
    mut commands: Commands,
    fields: Query<(Entity, &TextFieldEvent, &Editing)>,
    mut form: ResMut<Model>,
    mut core: ResMut<ClientCore>,
    mut probes: ResMut<Probes>,
    runtime: Option<Res<TokioRuntime>>,
    mut dirty: ResMut<Dirty>,
    mut notices: ResMut<Notices>,
) {
    for (entity, event, Editing(field)) in &fields {
        let intent = match event {
            TextFieldEvent::Committed(text) => Intent::Committed(*field, text.clone()),
            TextFieldEvent::Cancelled => Intent::Cancelled,
        };
        let outcome = form.0.apply(intent);
        settle(&outcome, &mut form.0, &mut core, &mut probes, runtime.as_deref(), &mut notices);
        commands.entity(entity).despawn();
        dirty.0 = true;
    }
}

fn probe_answers(mut probes: ResMut<Probes>, mut form: ResMut<Model>, mut dirty: ResMut<Dirty>) {
    let Some(answer) = probes.poll() else { return };
    form.0.apply(Intent::Tested(answer));
    dirty.0 = true;
}

fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    list: Query<Entity, With<ProfileList>>,
    editor: Query<Entity, With<Editor>>,
    mut notice: Query<&mut Text, With<NoticeLine>>,
    theme: Res<Theme>,
    form: Res<Model>,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    if let Ok(list) = list.single() {
        commands.entity(list).despawn_children();
        commands.entity(list).with_children(|parent| spawn_list(parent, &theme, &form.0));
    }
    if let Ok(editor) = editor.single() {
        commands.entity(editor).despawn_children();
        commands.entity(editor).with_children(|parent| spawn_editor(parent, &theme, &form.0));
    }
    if let Ok(mut text) = notice.single_mut() {
        text.0 = form.0.notice.clone().unwrap_or_default();
    }
}
