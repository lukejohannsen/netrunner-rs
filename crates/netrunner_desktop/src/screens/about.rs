//! About: what this client is, whose work is in it, and under what terms
//! — every third party a player sees, not only the files in the
//! repository.
//!
//! Everything shown is read from the credits register
//! (`crate::credits`, `assets/CREDITS.md`), the same file that
//! `tests/credits.rs` holds the committed assets to. So an asset added to
//! the repository is credited here by the act of adding it. The bundled
//! assets are grouped by owner and licence, one line per owner, so
//! Null Signal Games' ten symbols read as one credit rather than ten.
//!
//! A reading screen and not the board, so it scrolls: the no-scroll rule
//! is the play area's (AGENTS.md §5).

use bevy::prelude::*;

use crate::credits::{self, Asset, Credit};
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::{self, Pressed};

pub struct AboutPlugin;

impl Plugin for AboutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::About), spawn).add_systems(Update, back.run_if(in_state(AppScreen::About)));
    }
}

/// The Back button.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Back;

/// A credit line on the screen, for a test to read what is credited.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct CreditLine(pub String);

/// One owner's committed assets under one licence: the owner, their
/// website, the licence, where its text is, and what of theirs is here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub owner: String,
    pub website: String,
    pub licence: String,
    pub licence_text: String,
    pub origin: String,
    pub items: Vec<String>,
    /// What was done to them, each change once: a player wants to know a
    /// symbol was recoloured, not which source file each one came from,
    /// which the licence text beside the files records.
    pub changes: Vec<String>,
}

/// A change without the source file it names (`NSG_TAG.svg converted…`
/// reads `converted…`).
fn change_summary(changes: &str) -> String {
    changes.split_whitespace().filter(|word| !(word.ends_with(".svg") || word.ends_with(".png") || word.ends_with(".ttf"))).collect::<Vec<_>>().join(" ")
}

/// The committed assets grouped by owner and licence, in the register's
/// order of first appearance.
pub fn groups(assets: &[Asset]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for asset in assets {
        let item = asset.what.clone();
        let change = (asset.changes != "none").then(|| change_summary(&asset.changes));
        match groups.iter_mut().find(|g| g.owner == asset.owner && g.licence == asset.licence) {
            Some(group) => {
                if !group.items.contains(&item) {
                    group.items.push(item);
                }
                if let Some(change) = change
                    && !group.changes.contains(&change)
                {
                    group.changes.push(change);
                }
            }
            None => groups.push(Group {
                owner: asset.owner.clone(),
                website: asset.website.clone(),
                licence: asset.licence.clone(),
                licence_text: asset.licence_text.clone(),
                origin: asset.origin.clone(),
                items: vec![item],
                changes: change.into_iter().collect(),
            }),
        }
    }
    groups
}

fn section(parent: &mut ChildSpawnerCommands, theme: &Theme, title: &str, note: &str) {
    parent.spawn((widgets::label(theme, title), Node { margin: UiRect::top(px(12)), ..default() }));
    if !note.is_empty() {
        parent.spawn((widgets::dim(theme, note), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
}

fn line(parent: &mut ChildSpawnerCommands, theme: &Theme, text: String, dim: bool) {
    let layout = TextLayout::new(Justify::Left, LineBreak::WordBoundary);
    if dim {
        parent.spawn((CreditLine(text.clone()), widgets::dim(theme, text), layout));
    } else {
        parent.spawn((CreditLine(text.clone()), widgets::label(theme, text), layout));
    }
}

fn spawn_credit(parent: &mut ChildSpawnerCommands, theme: &Theme, credit: &Credit) {
    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), margin: UiRect::top(px(6)), ..default() }).with_children(|entry| {
        line(entry, theme, format!("{} — {}", credit.what, credit.owner), false);
        line(entry, theme, format!("{} · {} · {}", credit.website, credit.terms, credit.from), true);
    });
}

fn spawn(mut commands: Commands, theme: Res<Theme>) {
    let bundled = groups(&credits::bundled());
    let scroll = commands
        .spawn((
            bevy::ui_widgets::ScrollArea,
            Node { flex_grow: 1.0, min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|page| {
            page.spawn((widgets::roomy_panel(&theme, px(900)),)).with_children(|panel| {
                line(panel, &theme, format!("Netrunner desktop client {}", env!("CARGO_PKG_VERSION")), false);
                line(panel, &theme, "The source code is licensed under the GNU General Public License, version 3 or later.".to_string(), true);
                line(panel, &theme, credits::disclaimer(), true);

                section(panel, &theme, "Bundled with the client", "Each is a separately licensed work, owned by its maker. The licence texts ship beside the files, in the client's assets folder.");
                for group in &bundled {
                    panel.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), margin: UiRect::top(px(6)), ..default() }).with_children(|entry| {
                        let whose = if group.origin == "project" { format!("{} (made for this project)", group.owner) } else { group.owner.clone() };
                        line(entry, &theme, whose, false);
                        line(entry, &theme, format!("{} · {} · licence text: {}", group.website, group.licence, group.licence_text), true);
                        line(entry, &theme, group.items.join(" · "), true);
                        if !group.changes.is_empty() {
                            line(entry, &theme, format!("Changed: {}", group.changes.join("; ")), true);
                        }
                    });
                }

                section(panel, &theme, "Fetched on your say-so, never shipped", "Downloaded into your own cache for your own screen when you turn card images on in Settings.");
                for credit in credits::fetched() {
                    spawn_credit(panel, &theme, &credit);
                }

                section(panel, &theme, "Built with", "");
                for credit in credits::software() {
                    spawn_credit(panel, &theme, &credit);
                }
            });
        })
        .id();
    let body = commands
        .spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, justify_content: JustifyContent::Center, column_gap: px(8), ..default() })
        .add_child(scroll)
        .with_children(|body| {
            body.spawn(widgets::scrollbar(&theme, scroll));
        })
        .id();
    let mut root = commands.spawn(screen_root(AppScreen::About, &theme));
    root.with_children(|root| {
        root.spawn(widgets::heading(&theme, AppScreen::About.title()));
    });
    root.add_child(body);
    root.with_children(|root| {
        root.spawn(widgets::button(&theme, "Back", Val::Auto, Back));
    });
}

fn back(mut pressed: MessageReader<Pressed>, marks: Query<&Back>, mut navigate: MessageWriter<Navigate>) {
    for Pressed(entity) in pressed.read() {
        if marks.get(*entity).is_ok() {
            navigate.write(Navigate(AppScreen::MainMenu));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_owners_assets_are_one_credit_and_each_owner_is_named() {
        let groups = groups(&credits::bundled());
        let nsg: Vec<_> = groups.iter().filter(|g| g.owner == "Null Signal Games").collect();
        assert_eq!(nsg.len(), 1, "NSG's symbols are one credit");
        assert!(nsg[0].items.len() > 1 && nsg[0].licence == "CC BY-ND 4.0");
        assert_eq!(groups.iter().filter(|g| g.owner.contains("Noto")).count(), 1, "both Noto fonts are one credit");
        // Each change once, and without the file it came from.
        assert!(nsg[0].changes.iter().all(|c| !c.contains(".svg")), "{:?}", nsg[0].changes);
        assert!(nsg[0].changes.len() <= 2, "{:?}", nsg[0].changes);
    }
}
