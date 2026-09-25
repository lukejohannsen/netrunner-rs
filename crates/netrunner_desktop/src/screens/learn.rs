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
//!
//! **Each track ends in its starter games** (§6b): The Syndicate against
//! The Catalyst on the starter lists at 6 points, and the booster-staged
//! pair at the standard 7, both unguided. The opponent is the rung the
//! person's record suggests, as Play vs Computer seats it, where the
//! terminal's `learn game` seats the un-handicapped one ply — the top of
//! the ladder, and a wall for someone whose first real game this is. It
//! is recorded like any game against a bot.
//!
//! **The strategy guide is a button under the tracks** (`screens::guide`):
//! the rules are the lessons', and what to do with them is the guide's.
//!
//! **A finished lesson is ticked** (`Settings::lessons_done`, which the
//! terminal writes too), and the first unfinished lesson is the one
//! primary button, Corp track first.

use std::sync::Arc;

use bevy::prelude::*;
use netrunner_client::play::{LocalMatchSpec, MatchHandle, RecordFile};
use netrunner_client::record::LocalRecord;
use netrunner_core::rules::Side;
use netrunner_core::tutorial::{self, Lesson};

use crate::core::ClientCore;
use crate::nav::{screen_root, Navigate};
use crate::screens::new_game::{ActiveMatch, Starter};
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

/// A starter game's button.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterButton(pub Starter);

#[derive(Component)]
struct BackButton;

/// Opens the strategy guide (`screens::guide`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideButton;

/// Why the last lesson pressed would not start; empty until one fails.
#[derive(Component)]
struct StartError;

/// Starts `lesson` for the board: the match thread and the lesson beside
/// it. Fails, as a sentence, only on a lesson that will not set up — an
/// authoring bug the lesson gates exist to catch first.
pub fn start(core: &ClientCore, lesson: Lesson) -> Result<ActiveMatch, String> {
    let handle = MatchHandle::start_lesson(Arc::clone(&core.registry), lesson.clone(), LESSON_SEED)?;
    Ok(ActiveMatch { handle, choice: None, lesson: Some(lesson), starter: None })
}

/// Starts a starter game on a clock seed: the rules its decks' category
/// sets, the rung the record suggests for the person's chair and the
/// style the bot's deck plays, recorded where every local game is.
pub fn start_starter(core: &ClientCore, starter: Starter) -> Result<ActiveMatch, String> {
    let (corp, runner) = netrunner_client::learn::starter_decks(starter.boosted)?;
    let player = core.player_name();
    // A record that cannot be read suggests the middle rung, as the
    // new-game form's does, rather than keep a person from playing.
    let level = core.record_path.as_deref().map(LocalRecord::load).and_then(Result::ok).map_or(netrunner_client::start::Level::Operator, |log| log.suggest(&player, starter.side));
    let spec = LocalMatchSpec {
        registry: Arc::clone(&core.registry),
        rules: corp.category.match_rules(),
        corp,
        runner,
        human: starter.side,
        level,
        style: None,
        seed: crate::screens::new_game::seed_from_clock(),
        record: core.record_path.clone().map(|path| RecordFile { path, player }),
    };
    let handle = MatchHandle::start_local(spec)?;
    Ok(ActiveMatch { handle, choice: None, lesson: None, starter: Some(starter) })
}

/// The lesson after `id` in its side's track, if there is one.
pub fn next_after(id: &str) -> Option<Lesson> {
    netrunner_client::learn::next_after(id)
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>) {
    let done = &core.settings.lessons_done;
    // One primary in view: the first lesson not yet finished, Corp first.
    let suggested = [Side::Corp, Side::Runner].into_iter().find_map(|side| netrunner_client::learn::first_unfinished(side, done)).map(|lesson| lesson.id);
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
                    track_panel(row, &theme, side, done, suggested.as_deref());
                }
            });
        // The reading beside the lessons: what to do with the rules once
        // they are learned. A secondary pill, because a lesson is the
        // move this screen expects.
        root.spawn(widgets::styled_button(&theme, ButtonKind::Secondary, "Strategy guide", Val::Auto, GuideButton));
        root.spawn((StartError, widgets::notice(&theme, "", ())));
        root.spawn(widgets::styled_button(&theme, ButtonKind::Quiet, "Back", Val::Auto, BackButton));
    });
}

/// A side's track: its name in the side's colour, what it teaches, and
/// its lessons in order.
fn track_panel(parent: &mut ChildSpawnerCommands, theme: &Theme, side: Side, done: &std::collections::BTreeSet<String>, suggested: Option<&str>) {
    parent.spawn(widgets::roomy_panel(theme, px(520))).with_children(|panel| {
        let track = tutorial::track(side);
        let finished = track.iter().filter(|lesson| done.contains(&lesson.id)).count();
        panel.spawn((Text::new(format!("{side:?} track")), theme.font(size::HEADING), TextColor(theme.side(side))));
        panel.spawn((widgets::dim(theme, format!("{} {finished} of {} done.", track_blurb(side), track.len())), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        for (index, lesson) in track.into_iter().enumerate() {
            let kind = if suggested == Some(lesson.id.as_str()) { ButtonKind::Primary } else { ButtonKind::Secondary };
            let label = lesson_label(index, &lesson.title, done.contains(&lesson.id));
            // A row of a list, as the Replays list's are: the panel's
            // width, so a lesson's title is one line.
            let mut button = panel.spawn(widgets::styled_button(theme, kind, label, percent(100), LessonButton(lesson.id.clone())));
            button.entry::<Node>().and_modify(|mut node| node.max_width = Val::Auto);
        }
        panel.spawn((widgets::overline(theme, "Then play a whole game"), Node { margin: UiRect::top(px(8)), ..default() }));
        // Rows of the list like the lessons', not a pair side by side: two
        // pills in a row ran past the panel's edge and wrapped their words.
        for (boosted, label) in [(false, "Starter game · to 6 points"), (true, "Boosted starter game · to 7 points")] {
            let mut button = panel.spawn(widgets::styled_button(theme, ButtonKind::Secondary, label, percent(100), StarterButton(Starter { side, boosted })));
            button.entry::<Node>().and_modify(|mut node| node.max_width = Val::Auto);
        }
    });
}

/// A lesson's row: its place in the track and its title, and "done" once
/// finished. A word rather than a tick, because the Latin fonts the
/// client ships have no check mark and a missing glyph is a box.
pub fn lesson_label(index: usize, title: &str, done: bool) -> String {
    if done { format!("{}. {title} · done", index + 1) } else { format!("{}. {title}", index + 1) }
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
    starters: Query<&StarterButton>,
    back: Query<(), With<BackButton>>,
    guide: Query<(), With<GuideButton>>,
    mut error: Query<&mut Text, With<StartError>>,
    core: Res<ClientCore>,
    mut navigate: MessageWriter<Navigate>,
) {
    for Pressed(entity) in pressed.read() {
        if back.contains(*entity) {
            navigate.write(Navigate(AppScreen::MainMenu));
            return;
        }
        if guide.contains(*entity) {
            navigate.write(Navigate(AppScreen::Guide));
            return;
        }
        let started = if let Ok(StarterButton(starter)) = starters.get(*entity) {
            start_starter(&core, *starter)
        } else if let Ok(LessonButton(id)) = lessons.get(*entity) {
            tutorial::by_id(id).ok_or_else(|| format!("no lesson {id:?}")).and_then(|lesson| start(&core, lesson))
        } else {
            continue;
        };
        match started {
            Ok(active) => {
                commands.insert_resource(active);
                navigate.write(Navigate(AppScreen::Game));
                return;
            }
            Err(reason) => {
                for mut text in &mut error {
                    text.0 = format!("It could not start: {reason}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_lesson_says_so() {
        assert_eq!(lesson_label(0, "Clicks and credits", false), "1. Clicks and credits");
        assert_eq!(lesson_label(2, "Your first run", true), "3. Your first run · done");
    }
}
