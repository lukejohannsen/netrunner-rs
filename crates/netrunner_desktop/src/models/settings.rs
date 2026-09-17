//! The settings screen, as state: which row is which, what each control
//! does to the shared `Settings`, and what a commit of the name means.

use netrunner_client::settings::{format_name, Settings, Skin, Table, FORMATS};

/// The rows, in the order the screen shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Player,
    Format,
    AnimationSpeed,
    SfxVolume,
    MusicVolume,
    DownloadImages,
    PlayHelper,
    PlayHistory,
    PhaseBar,
    Table,
    Skin,
}

impl Row {
    pub const ALL: [Row; 11] = [Row::Player, Row::Format, Row::Table, Row::Skin, Row::AnimationSpeed, Row::SfxVolume, Row::MusicVolume, Row::DownloadImages, Row::PlayHelper, Row::PlayHistory, Row::PhaseBar];

    /// The rows the board's gear menu shows: what changes how a game is
    /// played and looks, and nothing that would want a text field. A
    /// future board property (a layout, a card-back choice) goes here
    /// as well as in `ALL`.
    pub const GAME: [Row; 8] = [Row::Table, Row::Skin, Row::PhaseBar, Row::PlayHelper, Row::PlayHistory, Row::AnimationSpeed, Row::SfxVolume, Row::MusicVolume];

    pub fn label(self) -> &'static str {
        match self {
            Row::Player => "Player name",
            Row::Format => "Format",
            Row::AnimationSpeed => "Animation speed",
            Row::SfxVolume => "Sound effects",
            Row::MusicVolume => "Music",
            Row::DownloadImages => "Download card images",
            Row::PlayHelper => "Play helper",
            Row::PlayHistory => "Play history",
            Row::PhaseBar => "Phase bar",
            Row::Table => "Table",
            Row::Skin => "Board art",
        }
    }

    /// Whether the row is adjusted with a pair of `<` `>` buttons (as
    /// opposed to edited or toggled).
    pub fn is_stepped(self) -> bool {
        matches!(self, Row::Format | Row::AnimationSpeed | Row::SfxVolume | Row::MusicVolume | Row::Table | Row::Skin)
    }
}

/// What a control on the screen asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// `<` or `>` on a stepped row.
    Step(Row, i32),
    Toggle(Row),
    /// The name field committed (`Some`) or was cancelled (`None`).
    NameEdited(Option<String>),
}

/// The name a player may set: at most this many characters, trimmed,
/// empty meaning "use the login name" — the terminal client's limits.
pub const MAX_NAME_LEN: usize = 32;

const SPEEDS: [f32; 5] = [0.0, 0.5, 1.0, 2.0, 4.0];
const VOLUME_STEP: f32 = 0.1;

/// Every value the board-art row steps through: follow the table, the
/// board's own outlines, then each installed skin.
///
/// `Auto` leads because it is the default and the one that needs no
/// decision — a field that came with matching chrome brings it along.
/// `Drawn` is second so refusing a skin is one step from taking one,
/// which is what somebody comparing them wants.
pub fn skin_cycle(installed: &[String]) -> Vec<Skin> {
    let mut cycle = vec![Skin::Auto, Skin::Drawn];
    cycle.extend(installed.iter().cloned().map(Skin::Named));
    cycle
}

/// Every value the table row steps through, in order: the painted
/// ground, then each installed table, then random.
///
/// The painted ground is first because it is the default and the tier
/// that always works, and random is last so a person stepping rightward
/// walks past the actual tables before being offered a shuffle of them.
/// Random is offered only when there is more than one table to choose
/// between — with one installed it would be that table under a name that
/// says otherwise, and with none it would be the painted ground.
pub fn table_cycle(installed: &[String]) -> Vec<Table> {
    let mut cycle = vec![Table::Painted];
    cycle.extend(installed.iter().cloned().map(Table::Named));
    if installed.len() > 1 {
        cycle.push(Table::Random);
    }
    cycle
}

/// Applies `intent` to `settings`. Returns whether anything changed, so
/// the screen saves only when there is something to save.
///
/// `tables` and `skins` are the installed folders, which only the caller
/// can know — they are the two rows whose choices come off the disk
/// rather than being fixed in this file.
pub fn apply(settings: &mut Settings, intent: Intent, tables: &[String], skins: &[String]) -> bool {
    match intent {
        Intent::Step(Row::Format, delta) => {
            let current = settings.format.unwrap_or(FORMATS[0]);
            let index = FORMATS.iter().position(|f| *f == current).unwrap_or(0) as i32;
            let next = FORMATS[(index + delta).rem_euclid(FORMATS.len() as i32) as usize];
            settings.format = Some(next);
            next != current
        }
        Intent::Step(Row::AnimationSpeed, delta) => {
            let prefs = &mut settings.desktop;
            let index = SPEEDS.iter().position(|s| (*s - prefs.animation_speed).abs() < 1e-3).unwrap_or(2) as i32;
            let next = SPEEDS[(index + delta).clamp(0, SPEEDS.len() as i32 - 1) as usize];
            let changed = next != prefs.animation_speed;
            prefs.animation_speed = next;
            changed
        }
        Intent::Step(Row::Table, delta) => {
            let cycle = table_cycle(tables);
            // A table that has been uninstalled is not in the cycle, so
            // a step from it starts over rather than going nowhere.
            let index = cycle.iter().position(|table| *table == settings.desktop.table).unwrap_or(0) as i32;
            let next = cycle[(index + delta).rem_euclid(cycle.len() as i32) as usize].clone();
            let changed = next != settings.desktop.table;
            settings.desktop.table = next;
            changed
        }
        Intent::Step(Row::Skin, delta) => {
            let cycle = skin_cycle(skins);
            let index = cycle.iter().position(|skin| *skin == settings.desktop.skin).unwrap_or(0) as i32;
            let next = cycle[(index + delta).rem_euclid(cycle.len() as i32) as usize].clone();
            let changed = next != settings.desktop.skin;
            settings.desktop.skin = next;
            changed
        }
        Intent::Step(Row::SfxVolume, delta) => step_volume(&mut settings.desktop.sfx_volume, delta),
        Intent::Step(Row::MusicVolume, delta) => step_volume(&mut settings.desktop.music_volume, delta),
        Intent::Toggle(Row::DownloadImages) => {
            settings.desktop.download_images = !settings.desktop.download_images;
            true
        }
        Intent::Toggle(Row::PlayHelper) => {
            settings.desktop.play_helper = !settings.desktop.play_helper;
            true
        }
        Intent::Toggle(Row::PhaseBar) => {
            settings.desktop.phase_bar = !settings.desktop.phase_bar;
            true
        }
        Intent::Toggle(Row::PlayHistory) => {
            settings.desktop.play_history = !settings.desktop.play_history;
            true
        }
        Intent::NameEdited(Some(name)) => {
            let name: String = name.trim().chars().take(MAX_NAME_LEN).collect();
            let next = (!name.is_empty()).then_some(name);
            let changed = next != settings.player;
            settings.player = next;
            changed
        }
        Intent::NameEdited(None) | Intent::Step(..) | Intent::Toggle(..) => false,
    }
}

fn step_volume(volume: &mut f32, delta: i32) -> bool {
    let next = ((*volume + delta as f32 * VOLUME_STEP) * 10.0).round() / 10.0;
    let next = next.clamp(0.0, 1.0);
    let changed = (next - *volume).abs() > 1e-6;
    *volume = next;
    changed
}

fn on_off(flag: bool) -> String {
    if flag { "on" } else { "off" }.to_string()
}

/// The value a row shows.
pub fn value(settings: &Settings, row: Row, login_name: &str) -> String {
    let prefs = &settings.desktop;
    match row {
        Row::Player => settings.player.clone().unwrap_or_else(|| format!("{login_name} (login name)")),
        Row::Format => format_name(settings.format.unwrap_or(FORMATS[0])).to_string(),
        Row::AnimationSpeed => {
            if prefs.animation_speed == 0.0 {
                "instant".to_string()
            } else {
                format!("{}×", prefs.animation_speed)
            }
        }
        Row::SfxVolume => format!("{:.0}%", prefs.sfx_volume * 100.0),
        Row::MusicVolume => format!("{:.0}%", prefs.music_volume * 100.0),
        Row::DownloadImages => on_off(prefs.download_images),
        Row::PlayHelper => on_off(prefs.play_helper),
        Row::PlayHistory => on_off(prefs.play_history),
        Row::PhaseBar => on_off(prefs.phase_bar),
        // The folder's own name. A table carrying a `table.json` with a
        // prettier one is relabelled by the screen that draws the row,
        // which is the only layer that may read the disk.
        Row::Skin => match &prefs.skin {
            Skin::Auto => "Follow the table".to_string(),
            Skin::Drawn => "Drawn".to_string(),
            Skin::Named(name) => name.clone(),
        },
        Row::Table => match &prefs.table {
            Table::Painted => "Painted ground".to_string(),
            Table::Random => "Random".to_string(),
            Table::Named(name) => name.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::format::NsgFormat;

    #[test]
    fn the_format_cycles_both_ways_and_wraps() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::Step(Row::Format, 1), &[], &[]));
        assert_eq!(settings.format, Some(NsgFormat::Standard));
        assert!(apply(&mut settings, Intent::Step(Row::Format, -2), &[], &[]));
        assert_eq!(settings.format, Some(NsgFormat::Snapshot), "wraps backwards");
        assert_eq!(value(&settings, Row::Format, "luke"), "snapshot");
    }

    #[test]
    fn the_animation_speed_walks_its_ladder_and_stops_at_the_ends() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::Step(Row::AnimationSpeed, -1), &[], &[]));
        assert_eq!(settings.desktop.animation_speed, 0.5);
        assert!(apply(&mut settings, Intent::Step(Row::AnimationSpeed, -1), &[], &[]));
        assert_eq!(value(&settings, Row::AnimationSpeed, "luke"), "instant");
        assert!(!apply(&mut settings, Intent::Step(Row::AnimationSpeed, -1), &[], &[]), "already at the bottom");
        for _ in 0..10 {
            apply(&mut settings, Intent::Step(Row::AnimationSpeed, 1), &[], &[]);
        }
        assert_eq!(settings.desktop.animation_speed, 4.0);
    }

    #[test]
    fn volumes_step_by_tenths_and_clamp() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::Step(Row::SfxVolume, 1), &[], &[]));
        assert_eq!(value(&settings, Row::SfxVolume, "luke"), "90%");
        assert!(apply(&mut settings, Intent::Step(Row::SfxVolume, 1), &[], &[]));
        assert!(!apply(&mut settings, Intent::Step(Row::SfxVolume, 1), &[], &[]), "clamped at 100%");
        for _ in 0..12 {
            apply(&mut settings, Intent::Step(Row::MusicVolume, -1), &[], &[]);
        }
        assert_eq!(settings.desktop.music_volume, 0.0);
    }

    #[test]
    fn the_name_is_trimmed_capped_and_empty_means_the_login_name() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::NameEdited(Some("  case ".to_string())), &[], &[]));
        assert_eq!(settings.player.as_deref(), Some("case"));
        assert!(!apply(&mut settings, Intent::NameEdited(Some("case".to_string())), &[], &[]), "unchanged");
        assert!(!apply(&mut settings, Intent::NameEdited(None), &[], &[]), "cancelled");
        assert_eq!(settings.player.as_deref(), Some("case"));
        assert!(apply(&mut settings, Intent::NameEdited(Some("x".repeat(40))), &[], &[]));
        assert_eq!(settings.player.as_deref().map(str::len), Some(MAX_NAME_LEN));
        assert!(apply(&mut settings, Intent::NameEdited(Some("   ".to_string())), &[], &[]));
        assert_eq!(settings.player, None);
        assert_eq!(value(&settings, Row::Player, "luke"), "luke (login name)");
    }

    #[test]
    fn download_images_toggles() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::Toggle(Row::DownloadImages), &[], &[]));
        assert!(settings.desktop.download_images);
        assert!(!apply(&mut settings, Intent::Toggle(Row::Format), &[], &[]), "not a toggle");
    }

    /// The table row cycles the painted ground, the installed tables and
    /// random — and offers random only when there is something to
    /// shuffle.
    #[test]
    fn the_table_cycles_the_ground_then_what_is_installed_then_random() {
        let none: [String; 0] = [];
        assert_eq!(table_cycle(&none), vec![Table::Painted], "nothing installed is not a choice");
        let one = ["neon-alley".to_string()];
        assert_eq!(table_cycle(&one), vec![Table::Painted, Table::Named("neon-alley".to_string())], "one table needs no random");
        let two = ["neon-alley".to_string(), "orbital".to_string()];
        assert_eq!(table_cycle(&two).len(), 4);
        assert_eq!(table_cycle(&two).last(), Some(&Table::Random), "random comes after the tables");

        let mut settings = Settings::default();
        assert_eq!(settings.desktop.table, Table::Painted);
        assert!(apply(&mut settings, Intent::Step(Row::Table, 1), &two, &[]));
        assert_eq!(settings.desktop.table, Table::Named("neon-alley".to_string()));
        assert_eq!(value(&settings, Row::Table, "luke"), "neon-alley");
        // It wraps both ways, like the format row.
        assert!(apply(&mut settings, Intent::Step(Row::Table, -1), &two, &[]));
        assert_eq!(value(&settings, Row::Table, "luke"), "Painted ground");
        assert!(apply(&mut settings, Intent::Step(Row::Table, -1), &two, &[]));
        assert_eq!(settings.desktop.table, Table::Random);

        // With nothing installed the row has one value and cannot move.
        let mut alone = Settings::default();
        assert!(!apply(&mut alone, Intent::Step(Row::Table, 1), &none, &[]), "nowhere to step");
        assert_eq!(alone.desktop.table, Table::Painted);

        // A table the player has since deleted is not in the cycle; a
        // step from it starts the cycle rather than doing nothing.
        let mut gone = Settings::default();
        gone.desktop.table = Table::Named("deleted".to_string());
        assert!(apply(&mut gone, Intent::Step(Row::Table, 1), &two, &[]));
        assert_eq!(gone.desktop.table, Table::Named("neon-alley".to_string()));
    }

    /// Board art leads with following the table, because that is the
    /// default and the one that needs no decision.
    #[test]
    fn the_board_art_row_leads_with_the_table_and_then_refuses_one() {
        let none: [String; 0] = [];
        assert_eq!(skin_cycle(&none), vec![Skin::Auto, Skin::Drawn], "both are choices with nothing installed");
        let two = ["neon-chrome".to_string(), "brass".to_string()];
        assert_eq!(skin_cycle(&two).len(), 4);

        let mut settings = Settings::default();
        assert_eq!(settings.desktop.skin, Skin::Auto);
        assert_eq!(value(&settings, Row::Skin, "luke"), "Follow the table");
        assert!(apply(&mut settings, Intent::Step(Row::Skin, 1), &[], &two));
        assert_eq!(value(&settings, Row::Skin, "luke"), "Drawn", "refusing one is a step from taking one");
        assert!(apply(&mut settings, Intent::Step(Row::Skin, 1), &[], &two));
        assert_eq!(settings.desktop.skin, Skin::Named("neon-chrome".to_string()));
        // It wraps, and `Auto` is what stepping back from the front reaches.
        assert!(apply(&mut settings, Intent::Step(Row::Skin, -2), &[], &two));
        assert_eq!(settings.desktop.skin, Skin::Auto);
        assert!(apply(&mut settings, Intent::Step(Row::Skin, -1), &[], &two));
        assert_eq!(settings.desktop.skin, Skin::Named("brass".to_string()), "wrapping backwards reaches the last");

        // With nothing installed the row still has two values to walk.
        let mut alone = Settings::default();
        assert!(apply(&mut alone, Intent::Step(Row::Skin, 1), &[], &none));
        assert_eq!(alone.desktop.skin, Skin::Drawn);
    }

    /// The board's two aids are off until turned on, and the gear menu's
    /// rows are settings rows, so a value has one spelling in both places.
    #[test]
    fn the_board_aids_toggle_and_the_game_rows_are_settings_rows() {
        let mut settings = Settings::default();
        assert_eq!(value(&settings, Row::PlayHelper, "luke"), "off");
        assert_eq!(value(&settings, Row::PlayHistory, "luke"), "off");
        assert!(apply(&mut settings, Intent::Toggle(Row::PlayHelper), &[], &[]));
        assert!(settings.desktop.play_helper && !settings.desktop.play_history);
        assert!(apply(&mut settings, Intent::Toggle(Row::PlayHistory), &[], &[]));
        assert_eq!(value(&settings, Row::PlayHistory, "luke"), "on");
        for row in Row::GAME {
            assert!(Row::ALL.contains(&row), "{row:?} is on the settings screen too");
            assert_ne!(row, Row::Player, "a text field has no place in an overlay");
        }
    }
}
