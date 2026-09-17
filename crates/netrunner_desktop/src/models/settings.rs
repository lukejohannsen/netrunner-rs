//! The settings screen, as state: which row is which, what each control
//! does to the shared `Settings`, and what a commit of the name means.

use netrunner_client::settings::{format_name, Settings, FORMATS};

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
}

impl Row {
    pub const ALL: [Row; 9] = [Row::Player, Row::Format, Row::AnimationSpeed, Row::SfxVolume, Row::MusicVolume, Row::DownloadImages, Row::PlayHelper, Row::PlayHistory, Row::PhaseBar];

    /// The rows the board's gear menu shows: what changes how a game is
    /// played and looks, and nothing that would want a text field. A
    /// future board property (a layout, a card-back choice) goes here
    /// as well as in `ALL`.
    pub const GAME: [Row; 6] = [Row::PhaseBar, Row::PlayHelper, Row::PlayHistory, Row::AnimationSpeed, Row::SfxVolume, Row::MusicVolume];

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
        }
    }

    /// Whether the row is adjusted with a pair of `<` `>` buttons (as
    /// opposed to edited or toggled).
    pub fn is_stepped(self) -> bool {
        matches!(self, Row::Format | Row::AnimationSpeed | Row::SfxVolume | Row::MusicVolume)
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

/// Applies `intent` to `settings`. Returns whether anything changed, so
/// the screen saves only when there is something to save.
pub fn apply(settings: &mut Settings, intent: Intent) -> bool {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::format::NsgFormat;

    #[test]
    fn the_format_cycles_both_ways_and_wraps() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::Step(Row::Format, 1)));
        assert_eq!(settings.format, Some(NsgFormat::Standard));
        assert!(apply(&mut settings, Intent::Step(Row::Format, -2)));
        assert_eq!(settings.format, Some(NsgFormat::Snapshot), "wraps backwards");
        assert_eq!(value(&settings, Row::Format, "luke"), "snapshot");
    }

    #[test]
    fn the_animation_speed_walks_its_ladder_and_stops_at_the_ends() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::Step(Row::AnimationSpeed, -1)));
        assert_eq!(settings.desktop.animation_speed, 0.5);
        assert!(apply(&mut settings, Intent::Step(Row::AnimationSpeed, -1)));
        assert_eq!(value(&settings, Row::AnimationSpeed, "luke"), "instant");
        assert!(!apply(&mut settings, Intent::Step(Row::AnimationSpeed, -1)), "already at the bottom");
        for _ in 0..10 {
            apply(&mut settings, Intent::Step(Row::AnimationSpeed, 1));
        }
        assert_eq!(settings.desktop.animation_speed, 4.0);
    }

    #[test]
    fn volumes_step_by_tenths_and_clamp() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::Step(Row::SfxVolume, 1)));
        assert_eq!(value(&settings, Row::SfxVolume, "luke"), "90%");
        assert!(apply(&mut settings, Intent::Step(Row::SfxVolume, 1)));
        assert!(!apply(&mut settings, Intent::Step(Row::SfxVolume, 1)), "clamped at 100%");
        for _ in 0..12 {
            apply(&mut settings, Intent::Step(Row::MusicVolume, -1));
        }
        assert_eq!(settings.desktop.music_volume, 0.0);
    }

    #[test]
    fn the_name_is_trimmed_capped_and_empty_means_the_login_name() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::NameEdited(Some("  case ".to_string()))));
        assert_eq!(settings.player.as_deref(), Some("case"));
        assert!(!apply(&mut settings, Intent::NameEdited(Some("case".to_string()))), "unchanged");
        assert!(!apply(&mut settings, Intent::NameEdited(None)), "cancelled");
        assert_eq!(settings.player.as_deref(), Some("case"));
        assert!(apply(&mut settings, Intent::NameEdited(Some("x".repeat(40)))));
        assert_eq!(settings.player.as_deref().map(str::len), Some(MAX_NAME_LEN));
        assert!(apply(&mut settings, Intent::NameEdited(Some("   ".to_string()))));
        assert_eq!(settings.player, None);
        assert_eq!(value(&settings, Row::Player, "luke"), "luke (login name)");
    }

    #[test]
    fn download_images_toggles() {
        let mut settings = Settings::default();
        assert!(apply(&mut settings, Intent::Toggle(Row::DownloadImages)));
        assert!(settings.desktop.download_images);
        assert!(!apply(&mut settings, Intent::Toggle(Row::Format)), "not a toggle");
    }

    /// The board's two aids are off until turned on, and the gear menu's
    /// rows are settings rows, so a value has one spelling in both places.
    #[test]
    fn the_board_aids_toggle_and_the_game_rows_are_settings_rows() {
        let mut settings = Settings::default();
        assert_eq!(value(&settings, Row::PlayHelper, "luke"), "off");
        assert_eq!(value(&settings, Row::PlayHistory, "luke"), "off");
        assert!(apply(&mut settings, Intent::Toggle(Row::PlayHelper)));
        assert!(settings.desktop.play_helper && !settings.desktop.play_history);
        assert!(apply(&mut settings, Intent::Toggle(Row::PlayHistory)));
        assert_eq!(value(&settings, Row::PlayHistory, "luke"), "on");
        for row in Row::GAME {
            assert!(Row::ALL.contains(&row), "{row:?} is on the settings screen too");
            assert_ne!(row, Row::Player, "a text field has no place in an overlay");
        }
    }
}
