//! Learn to Play: both lesson tracks, each lesson a button that plays it
//! on the board (Phase 7 §6).
//!
//! **A lesson is played on the game screen, not a screen of its own.** A
//! press starts the lesson's match thread (`MatchHandle::start_lesson`)
//! and puts it where a game's would be, with the lesson beside it
//! (`ActiveMatch::lesson`), and the board draws it with every system it
//! draws a game with — the rules in AGENTS.md §5 kept in one place, as
//! the replay board keeps them. What the lesson changes is the list the
//! board offers (`models::lesson`) and the coaching in the right column.
//!
//! The lessons are Phase 1.75's, in the order Null Signal Games' own
//! guide introduces the concepts, and the tracks are the terminal's
//! `learn track corp|runner`: the Corp's seven, then the Runner's seven.
//! A lesson's seed is the terminal's default, 0, so a lesson plays the
//! same way every time — a lesson that dealt differently would teach a
//! different thing.

use std::sync::Arc;

use bevy::prelude::*;
use netrunner_client::play::MatchHandle;
use netrunner_core::rules::Side;
use netrunner_core::tutorial::{self, Lesson};

use crate::core::ClientCore;
use crate::nav::{screen_root, Navigate};
use crate::screens::new_game::ActiveMatch;
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct LearnPlugin;

impl Plugin for LearnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Learn), spawn).add_systems(Update, pick.run_if(in_state(AppScreen::Learn)));
    }
}

/// The seed every lesson is played on, as `netrunner_cli learn` plays it.
pub const LESSON_SEED: u64 = 0;

/// A lesson's button, by its id.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct LessonButton(pub String);

#[derive(Component)]
struct BackButton;

/// Why the last lesson pressed would not start; empty until one fails.
#[derive(Component)]
struct StartError;

/// Starts `lesson` for the board: the match thread and the lesson beside
/// it. Fails, as a sentence, only on a lesson that will not set up — an
/// authoring bug the lesson gates exist to catch first.
pub fn start(core: &ClientCore, lesson: Lesson) -> Result<ActiveMatch, String> {
    let handle = MatchHandle::start_lesson(Arc::clone(&core.registry), lesson.clone(), LESSON_SEED)?;
    Ok(ActiveMatch { handle, choice: None, lesson: Some(lesson) })
}

/// The lesson after `id` in its side's track, if there is one.
pub fn next_after(id: &str) -> Option<Lesson> {
    let lesson = tutorial::by_id(id)?;
    let track = tutorial::track(lesson.side);
    let at = track.iter().position(|each| each.id == id)?;
    track.into_iter().nth(at + 1)
}

fn spawn(mut commands: Commands, theme: Res<Theme>) {
    let mut root = commands.spawn(screen_root(AppScreen::Learn, &theme));
    root.with_children(|root| {
        root.spawn(widgets::title(&theme, AppScreen::Learn.title()));
        root.spawn((
            widgets::dim(&theme, "Short guided games, one idea each. The coach in the right column says what to do; everything else the rules allow is one press away."),
            TextLayout::new(Justify::Center, LineBreak::WordBoundary),
        ));
        root.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(24), align_items: AlignItems::FlexStart, max_width: percent(100), ..default() })
            .with_children(|row| {
                for side in [Side::Corp, Side::Runner] {
                    track_panel(row, &theme, side);
                }
            });
        root.spawn((StartError, widgets::notice(&theme, "", ())));
        root.spawn(widgets::styled_button(&theme, ButtonKind::Quiet, "Back", Val::Auto, BackButton));
    });
}

/// A side's track: its name in the side's colour, what it teaches, and
/// its lessons in order.
fn track_panel(parent: &mut ChildSpawnerCommands, theme: &Theme, side: Side) {
    parent.spawn(widgets::roomy_panel(theme, px(520))).with_children(|panel| {
        panel.spawn((Text::new(format!("{side:?} track")), theme.font(size::HEADING), TextColor(theme.side(side))));
        panel.spawn((widgets::dim(theme, track_blurb(side)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        for (index, lesson) in tutorial::track(side).into_iter().enumerate() {
            // A row of a list, as the Replays list's are: the panel's
            // width, so a lesson's title is one line.
            let mut button = panel.spawn(widgets::styled_button(theme, ButtonKind::Secondary, format!("{}. {}", index + 1, lesson.title), percent(100), LessonButton(lesson.id.clone())));
            button.entry::<Node>().and_modify(|mut node| node.max_width = Val::Auto);
        }
    });
}

fn track_blurb(side: Side) -> &'static str {
    match side {
        Side::Corp => "Clicks and credits, installing and rezzing, ICE, advancing and scoring — and what a Runner does to an agenda left open.",
        Side::Runner => "Credits and memory, a first run, breaking ICE, stealing from R&D, and getting out when a run goes wrong.",
    }
}

fn pick(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    lessons: Query<&LessonButton>,
    back: Query<(), With<BackButton>>,
    mut error: Query<&mut Text, With<StartError>>,
    core: Res<ClientCore>,
    mut navigate: MessageWriter<Navigate>,
) {
    for Pressed(entity) in pressed.read() {
        if back.contains(*entity) {
            navigate.write(Navigate(AppScreen::MainMenu));
            return;
        }
        let Ok(LessonButton(id)) = lessons.get(*entity) else { continue };
        let started = tutorial::by_id(id).ok_or_else(|| format!("no lesson {id:?}")).and_then(|lesson| start(&core, lesson));
        match started {
            Ok(active) => {
                commands.insert_resource(active);
                navigate.write(Navigate(AppScreen::Game));
                return;
            }
            Err(reason) => {
                for mut text in &mut error {
                    text.0 = format!("The lesson could not start: {reason}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_goes_on_to_its_next_lesson_and_stops_at_its_end() {
        for side in [Side::Corp, Side::Runner] {
            let track = tutorial::track(side);
            for pair in track.windows(2) {
                assert_eq!(next_after(&pair[0].id).map(|lesson| lesson.id), Some(pair[1].id.clone()));
            }
            assert_eq!(next_after(&track.last().unwrap().id), None, "a track does not run on into the other side's");
        }
    }
}
