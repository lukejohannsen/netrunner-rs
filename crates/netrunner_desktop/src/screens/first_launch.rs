//! The first launch: one question after the splash, then the menu.
//!
//! What it asks and what an answer does is `models::first_launch`; this
//! draws the step and carries the answer out — the settings saved, and
//! on a yes the same download the Cards screen starts
//! (`downloads::Downloads`), which is not tied to a screen and so goes
//! on behind the menu once the person continues.
//!
//! Escape is "Not now": `nav::back_from` gives this screen no way back,
//! because leaving has to record the answer.
//!
//! `NETRUNNER_WELCOME=declined` (or `downloading`) opens it on that step
//! for a screenshot, changing no setting and starting no download.

use bevy::prelude::*;

use crate::core::{ClientCore, Notices, TokioRuntime};
use crate::downloads::{every_printing, Downloads};
use crate::models::first_launch::{Intent, Offer, Outcome, Step, LATER};
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::{self, ButtonKind, Pressed};

pub struct FirstLaunchPlugin;

impl Plugin for FirstLaunchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::FirstLaunch), spawn).add_systems(Update, (act, draw, progress).chain().run_if(in_state(AppScreen::FirstLaunch)));
    }
}

/// The step on screen.
#[derive(Resource, Default)]
struct Asked(Offer);

/// A button, by the intent it is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer(pub Intent);

/// The panel, whose children are the step.
#[derive(Component)]
struct Body;

/// The line that follows the download.
#[derive(Component)]
struct Progress;

fn spawn(mut commands: Commands, theme: Res<Theme>) {
    let step = match std::env::var("NETRUNNER_WELCOME").unwrap_or_default().trim().to_ascii_lowercase().as_str() {
        "declined" => Step::Declined,
        "downloading" => Step::Downloading,
        _ => Step::Asking,
    };
    commands.insert_resource(Asked(Offer { step }));
    commands.spawn((screen_root(AppScreen::FirstLaunch, &theme), children![(
        Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, margin: UiRect::vertical(Val::Auto), max_width: percent(100), ..default() },
        children![(widgets::roomy_panel(&theme, px(680)), Body)],
    )]));
}

fn act(
    mut pressed: MessageReader<Pressed>,
    answers: Query<&Answer>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut asked: ResMut<Asked>,
    mut core: ResMut<ClientCore>,
    mut notices: ResMut<Notices>,
    mut downloads: ResMut<Downloads>,
    runtime: Option<Res<TokioRuntime>>,
    mut navigate: MessageWriter<Navigate>,
) {
    let mut intents: Vec<Intent> = pressed.read().filter_map(|Pressed(entity)| answers.get(*entity).ok().map(|answer| answer.0)).collect();
    if keys.is_some_and(|keys| keys.just_pressed(KeyCode::Escape)) {
        intents.push(Intent::Escape);
    }
    for intent in intents {
        let outcome = asked.bypass_change_detection().0.apply(intent, &mut core.settings.desktop);
        match outcome {
            Outcome::Nothing => continue,
            Outcome::Leave => {
                navigate.write(Navigate(AppScreen::MainMenu));
                continue;
            }
            Outcome::Answered | Outcome::StartDownload => {}
        }
        asked.set_changed();
        if let Err(error) = core.save_settings() {
            notices.push(format!("Settings not saved: {error}"));
        }
        if outcome == Outcome::StartDownload {
            match &runtime {
                Some(runtime) => {
                    downloads.start(runtime, core.images.clone(), every_printing());
                }
                None => notices.push("No network runtime, so no download"),
            }
        }
    }
}

fn draw(mut commands: Commands, theme: Res<Theme>, asked: Res<Asked>, core: Res<ClientCore>, body: Query<Entity, With<Body>>) {
    if !asked.is_changed() {
        return;
    }
    let Ok(body) = body.single() else { return };
    let buttons = Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::FlexEnd, column_gap: px(10), margin: UiRect::top(px(8)), ..default() };
    commands.entity(body).despawn_children();
    commands.entity(body).with_children(|panel| match asked.0.step {
        Step::Asking => {
            let count = every_printing().len();
            panel.spawn(widgets::heading(&theme, "Download the card images?"));
            panel.spawn(widgets::label(
                &theme,
                format!("Netrunner can show every card as it is printed. The pictures come from NetrunnerDB: {count} images, about 100 MB, fetched once and kept on this computer. Without them a card is drawn as its text."),
            ));
            panel.spawn(widgets::dim(&theme, format!("They are saved in {}", core.images.dir().display())));
            panel.spawn(buttons).with_children(|row| {
                row.spawn(widgets::styled_button(&theme, ButtonKind::Quiet, "Not now", Val::Auto, Answer(Intent::Decline)));
                row.spawn(widgets::styled_button(&theme, ButtonKind::Primary, "Download", Val::Auto, Answer(Intent::Accept)));
            });
        }
        Step::Downloading => {
            panel.spawn(widgets::heading(&theme, "Downloading the card images"));
            panel.spawn((widgets::label(&theme, ""), Progress));
            panel.spawn(widgets::dim(&theme, "You need not wait: the download carries on while you use the menus, and the Cards screen shows how far it has got."));
            panel.spawn(buttons).with_children(|row| {
                row.spawn(widgets::styled_button(&theme, ButtonKind::Primary, "Continue", Val::Auto, Answer(Intent::Continue)));
            });
        }
        Step::Declined => {
            panel.spawn(widgets::heading(&theme, "No card images, then"));
            panel.spawn(widgets::label(&theme, LATER));
            panel.spawn(buttons).with_children(|row| {
                row.spawn(widgets::styled_button(&theme, ButtonKind::Primary, "Continue", Val::Auto, Answer(Intent::Continue)));
            });
        }
    });
}

fn progress(downloads: Res<Downloads>, mut lines: Query<&mut Text, With<Progress>>) {
    for mut text in &mut lines {
        let line = downloads.status_line();
        if text.0 != line {
            text.0 = line;
        }
    }
}
