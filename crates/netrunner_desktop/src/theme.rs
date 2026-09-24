//! One place for how the client looks: its font, its palette, its sizes.
//!
//! The palette is Netrunner's: the Corp is blue and the Runner red, as
//! Null Signal Games prints their card backs, and each faction keeps the
//! colour its cards carry. Everything else is a dark ground those colours
//! read against.

use bevy::prelude::*;

use netrunner_client::card_text::{self, Symbol};
use netrunner_core::card::Faction;
use netrunner_core::rules::Side;

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Theme>();
    }
}

/// The font under `assets/fonts`. Noto Sans, under the SIL Open Font
/// License (`assets/fonts/LICENSE-OFL.txt`); Bevy's built-in default font
/// is the fallback while it loads or if the file is missing.
pub const FONT_PATH: &str = "fonts/NotoSans-Regular.ttf";

/// The face the card's printed icons are drawn in: Noto Sans Symbols 2,
/// same licence. Noto Sans has no arrows, geometric shapes or dingbats,
/// so the click, the subroutine, the unique diamond and the influence
/// pips need a second font; `netrunner_client::card_text::Symbol::glyph`
/// names the code points, each checked against this file's character
/// map.
pub const SYMBOL_FONT_PATH: &str = "fonts/NotoSansSymbols2-Regular.ttf";

#[derive(Resource, Clone)]
pub struct Theme {
    /// `None` until boot loads it, and in a headless test.
    pub font: Option<Handle<Font>>,
    /// `None` until boot loads it, and in a headless test; a face then
    /// draws `Symbol::fallback` instead of `Symbol::glyph`.
    pub symbol_font: Option<Handle<Font>>,
    /// NetrunnerDB's icon font, once `icon_font` has fetched and loaded
    /// it: the printed symbols as the card prints them, the factions'
    /// marks and the sets'. `None` until then, and for a player who has
    /// not opted into downloads — the two fonts above then do.
    pub icon_font: Option<Handle<Font>>,
    pub background: Color,
    pub panel: Color,
    pub panel_border: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub button: Color,
    pub button_hover: Color,
    pub button_press: Color,
    pub corp: Color,
    pub runner: Color,
    pub danger: Color,
    /// The ring on a card the engine will accept an ordinary move on
    /// (`board::Affordance::Usable`). Purple because the two colours
    /// already spoken for on a card are the sides' own — Corp blue and
    /// Runner red — and their mixture belongs to neither side, so it
    /// reads the same on both chairs.
    ///
    /// **A light lavender, not a saturated violet, so that it can still
    /// be seen by a colour-blind player** (Phase 7 §8 item 18). A glow
    /// sits against the card's own border, which is its faction's
    /// colour, and between them the seven factions use up the whole hue
    /// wheel. So what keeps the halo from reading as a thicker border
    /// is lightness. The saturated violet this replaced was 0.02 OKLab
    /// from a Criminal border under protanopia, and 0.10 from Haas-Bioroid
    /// even with normal vision. `tests::the_glows_survive_colour_blindness`
    /// holds the bound.
    pub glow_usable: Color,
    /// The ring on a card whose moment will pass
    /// (`board::Affordance::Conditional`). Yellow as a warning, and kept
    /// clear of `danger`: danger is a number that has gone wrong, this is
    /// an opportunity about to be lost.
    ///
    /// **A pale yellow, for the same reason as `glow_usable`:** the
    /// saturated amber it replaced was 0.03 OKLab from NBN's yellow
    /// border for every viewer, so a playable NBN card had no visible
    /// glow at all.
    pub glow_conditional: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font: None,
            symbol_font: None,
            icon_font: None,
            background: Color::srgb(0.06, 0.07, 0.09),
            panel: Color::srgb(0.10, 0.11, 0.14),
            panel_border: Color::srgb(0.22, 0.24, 0.30),
            text: Color::srgb(0.90, 0.91, 0.93),
            text_dim: Color::srgb(0.58, 0.60, 0.66),
            accent: Color::srgb(0.20, 0.80, 0.70),
            button: Color::srgb(0.15, 0.17, 0.22),
            button_hover: Color::srgb(0.22, 0.25, 0.32),
            button_press: Color::srgb(0.12, 0.40, 0.36),
            corp: Color::srgb(0.16, 0.42, 0.85),
            runner: Color::srgb(0.80, 0.16, 0.20),
            danger: Color::srgb(0.90, 0.35, 0.30),
            glow_usable: Color::srgb(0.76, 0.55, 1.0),
            glow_conditional: Color::srgb(1.0, 0.99, 0.70),
        }
    }
}

/// Type sizes, in logical pixels, named for their role so a screen never
/// invents a size.
pub mod size {
    pub const TITLE: f32 = 40.0;
    pub const HEADING: f32 = 26.0;
    pub const BODY: f32 = 18.0;
    pub const SMALL: f32 = 14.0;
}

impl Theme {
    pub fn side(&self, side: Side) -> Color {
        match side {
            Side::Corp => self.corp,
            Side::Runner => self.runner,
        }
    }

    /// The colour a faction's cards carry.
    pub fn faction(&self, faction: Option<Faction>) -> Color {
        match faction {
            Some(Faction::Anarch) => Color::srgb(0.95, 0.45, 0.15),
            Some(Faction::Criminal) => Color::srgb(0.20, 0.45, 0.90),
            Some(Faction::Shaper) => Color::srgb(0.45, 0.75, 0.25),
            Some(Faction::HaasBioroid) => Color::srgb(0.55, 0.30, 0.75),
            Some(Faction::Jinteki) => Color::srgb(0.85, 0.20, 0.30),
            Some(Faction::Nbn) => Color::srgb(0.95, 0.75, 0.15),
            Some(Faction::WeylandConsortium) => Color::srgb(0.25, 0.60, 0.50),
            Some(Faction::NeutralCorp | Faction::NeutralRunner) | None => self.text_dim,
        }
    }

    /// A `TextFont` in the theme's face at `size`.
    pub fn font(&self, size: f32) -> TextFont {
        let mut font = TextFont { font_size: FontSize::Px(size), ..default() };
        if let Some(handle) = &self.font {
            font.font = handle.clone().into();
        }
        font
    }

    /// A `TextFont` in the symbol face at `size`, or the text face when
    /// the symbol font is not loaded — in which case the caller draws
    /// fallbacks, not glyphs (`has_symbols`).
    pub fn symbol_font(&self, size: f32) -> TextFont {
        let mut font = TextFont { font_size: FontSize::Px(size), ..default() };
        if let Some(handle) = self.symbol_font.as_ref().or(self.font.as_ref()) {
            font.font = handle.clone().into();
        }
        font
    }

    /// Whether a face may draw `Symbol::glyph`.
    pub fn has_symbols(&self) -> bool {
        self.symbol_font.is_some()
    }

    /// A `TextFont` in NetrunnerDB's icon face at `size`, or the text
    /// face when it is not loaded — callers check `has_icons` and draw
    /// words instead.
    pub fn icon_font(&self, size: f32) -> TextFont {
        let mut font = TextFont { font_size: FontSize::Px(size), ..default() };
        if let Some(handle) = self.icon_font.as_ref().or(self.font.as_ref()) {
            font.font = handle.clone().into();
        }
        font
    }

    pub fn has_icons(&self) -> bool {
        self.icon_font.is_some()
    }

    /// What a printed symbol is drawn as, and in which face: the icon
    /// font's glyph when it is loaded, Noto Sans Symbols 2's when that
    /// is, the Latin-1 fallback otherwise. The three tiers of the asset
    /// rule, decided once.
    pub fn symbol(&self, symbol: Symbol, size: f32) -> (String, TextFont) {
        if self.has_icons() {
            (symbol.icon().to_string(), self.icon_font(size))
        } else if self.has_symbols() {
            (symbol.glyph().to_string(), self.symbol_font(size))
        } else {
            (symbol.fallback().to_string(), self.font(size))
        }
    }

    /// A faction's mark in the icon font, or nothing without it.
    pub fn faction_icon(&self, faction: Faction, size: f32) -> Option<(String, TextFont)> {
        self.has_icons().then(|| (card_text::faction_icon(faction).to_string(), self.icon_font(size)))
    }

    /// A set's mark in the icon font, or nothing without it or for a
    /// set the font has no mark for.
    pub fn set_icon(&self, set_code: &str, size: f32) -> Option<(String, TextFont)> {
        if !self.has_icons() {
            return None;
        }
        card_text::set_icon(set_code).map(|icon| (icon.to_string(), self.icon_font(size)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Machado, Oliveira and Fernandes (2009), full severity, applied
    /// in linear RGB. These three are the standard simulations, so a
    /// palette checked against them is checked against the published
    /// model and not against a matrix made up here.
    const DICHROMACIES: [(&str, [[f32; 3]; 3]); 4] = [
        ("normal vision", [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        ("protanopia", [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]]),
        ("deuteranopia", [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]]),
        ("tritanopia", [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]]),
    ];

    /// The least OKLab distance two colours that must be told apart may
    /// have under any of the simulations. OKLab is perceptually even,
    /// so one bound serves every pair. At 0.10 a person sees two
    /// different colours without having to look for the difference.
    /// The glows' worst case is 0.12, and the colours they replaced
    /// were at 0.02.
    const APART: f32 = 0.10;

    /// A colour as a viewer with `matrix` sees it, in OKLab.
    fn seen(colour: Color, matrix: &[[f32; 3]; 3]) -> [f32; 3] {
        let linear = colour.to_linear();
        let rgb = [linear.red, linear.green, linear.blue];
        let simulated = LinearRgba::rgb(
            (0..3).map(|j| matrix[0][j] * rgb[j]).sum::<f32>().clamp(0.0, 1.0),
            (0..3).map(|j| matrix[1][j] * rgb[j]).sum::<f32>().clamp(0.0, 1.0),
            (0..3).map(|j| matrix[2][j] * rgb[j]).sum::<f32>().clamp(0.0, 1.0),
        );
        let lab = Oklaba::from(simulated);
        [lab.lightness, lab.a, lab.b]
    }

    /// The pair's distance for the viewer it is hardest for, and who that is.
    fn closest(a: Color, b: Color) -> (f32, &'static str) {
        DICHROMACIES
            .iter()
            .map(|(who, matrix)| {
                let (x, y) = (seen(a, matrix), seen(b, matrix));
                ((0..3).map(|i| (x[i] - y[i]).powi(2)).sum::<f32>().sqrt(), *who)
            })
            .min_by(|p, q| p.0.total_cmp(&q.0))
            .expect("four viewers")
    }

    /// **Each mood's glow must be visible against every border it can
    /// sit beside, and against the other mood, for every viewer.** A
    /// glow is drawn right against a card's border: a faction's colour
    /// on a face-up card, the side's colour on a face-down one. A glow
    /// that matches the border just looks like a thicker border, which
    /// tells the person nothing. `danger` is on the list because a
    /// warning and a mistake must not look alike.
    #[test]
    fn the_glows_survive_colour_blindness() {
        let theme = Theme::default();
        let factions = [
            Faction::Anarch,
            Faction::Criminal,
            Faction::Shaper,
            Faction::HaasBioroid,
            Faction::Jinteki,
            Faction::Nbn,
            Faction::WeylandConsortium,
        ];
        let mut beside: Vec<(String, Color)> = factions.iter().map(|faction| (format!("{faction:?}"), theme.faction(Some(*faction)))).collect();
        beside.extend([("the Corp".to_string(), theme.corp), ("the Runner".to_string(), theme.runner), ("danger".to_string(), theme.danger)]);
        let mut failures = Vec::new();
        for (mood, glow) in [("usable", theme.glow_usable), ("conditional", theme.glow_conditional)] {
            for (name, colour) in &beside {
                let (distance, who) = closest(glow, *colour);
                if distance < APART {
                    failures.push(format!("the {mood} glow beside {name}: {distance:.3} under {who}"));
                }
            }
        }
        let (distance, who) = closest(theme.glow_usable, theme.glow_conditional);
        if distance < APART {
            failures.push(format!("the two moods: {distance:.3} under {who}"));
        }
        assert!(failures.is_empty(), "closer than {APART} OKLab:\n{}", failures.join("\n"));
    }

    /// A subroutine's dot on the run lane is filled in `accent` when it
    /// was broken and in `danger` when it fired. The dots are the same
    /// shape, so colour is the only thing that tells them apart. (A
    /// pending dot is hollow, so it differs in shape, and the encounter
    /// panel writes `[x]` and `[!]`.)
    #[test]
    fn a_broken_subroutine_and_a_fired_one_are_told_apart() {
        let theme = Theme::default();
        let (distance, who) = closest(theme.accent, theme.danger);
        assert!(distance >= APART, "broken and fired dots are {distance:.3} OKLab apart under {who}");
    }
}
