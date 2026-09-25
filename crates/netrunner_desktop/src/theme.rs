//! One place for how the client looks: its font, its palette, its sizes.
//!
//! The palette is Netrunner's: the Corp is blue and the Runner red, as
//! Null Signal Games prints their card backs, and each faction keeps the
//! colour its cards carry. Everything else — the glass, the buttons,
//! the backdrop, the board's own chrome — is the palette of the *Android*
//! setting's art (Fantasy Flight's *Shadow of the Beanstalk* cover, which
//! the person held up as the look): a steel-teal sky falling into a
//! violet night, neon purple, and an icy white-blue for the lines that
//! catch the eye. It is a dark ground the sides' and factions' colours
//! read against, tinted rather than grey (the plain blue of §4aw was the
//! first cut).

use bevy::prelude::*;

use netrunner_client::card_text::{self, Symbol};
use netrunner_core::card::Faction;
use netrunner_core::rules::Side;

pub struct ThemePlugin;

/// `top` painted over an opaque `ground`: an opaque colour.
pub fn over(top: Color, ground: Color) -> Color {
    let (t, g) = (top.to_srgba(), ground.to_srgba());
    let a = t.alpha;
    Color::srgb(t.red * a + g.red * (1.0 - a), t.green * a + g.green * (1.0 - a), t.blue * a + g.blue * (1.0 - a))
}

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
    /// A deck that can start a game: the deck builder's "legal" badge.
    /// A mint that is neither side's colour nor a glow's, and always
    /// beside the word, so no reader depends on the hue.
    pub legal: Color,
    /// A deck the engine runs but the format refuses: amber, between
    /// `legal` and `danger`, and likewise always beside its words.
    pub caution: Color,
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
    /// What a menu, a form, a pop-up or a sheet is drawn on: an
    /// indigo-violet glass the screen behind shows through. Translucent on purpose —
    /// a menu over a backdrop, or a pop-up over the board, should read as
    /// a layer on the place, not a hole cut in it. `panel` above stays
    /// opaque grey for the board's own chrome (the log, the plates),
    /// which has nothing behind it to show.
    pub glass: Color,
    /// The same glass, thicker: a decision pop-up or a card's menu sits
    /// over cards, whose own text would otherwise read through.
    pub glass_strong: Color,
    /// A glass panel's edge: a faint light rim rather than a dark line.
    pub glass_border: Color,
    /// The one button a screen most expects to be pressed (Start game,
    /// Continue): filled in neon violet, with white text on it. Not the
    /// accent, which is a line colour and too light to carry white text.
    pub primary: Color,
    pub primary_hover: Color,
    pub primary_press: Color,
    /// The text on a primary button.
    pub on_primary: Color,
    /// Every other button: a light tint over the glass it sits on, so it
    /// takes that glass's colour rather than painting over it.
    pub secondary: Color,
    pub secondary_hover: Color,
    pub secondary_press: Color,
    /// A button's rim when hovered: brighter than `glass_border`, so the
    /// edge answers the pointer as well as the fill.
    pub border_hover: Color,
    /// The drawn backdrop behind every menu screen, top to bottom, and the
    /// colours of the two soft blooms over it (`nav::screen_root`): a
    /// blue haze high on the left, a violet one low on the right, so the
    /// screen shades from the sky into the street's neon.
    pub backdrop_top: Color,
    pub backdrop_bottom: Color,
    pub backdrop_bloom: Color,
    pub backdrop_bloom_violet: Color,
    /// The wash under a pop-up or a sheet: the backdrop's deep violet, so
    /// what it dims is tinted into the glass rather than greyed out.
    pub wash: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font: None,
            symbol_font: None,
            icon_font: None,
            background: Color::srgb(0.05, 0.05, 0.10),
            panel: Color::srgb(0.09, 0.08, 0.16),
            panel_border: Color::srgb(0.25, 0.23, 0.38),
            text: Color::srgb(0.94, 0.95, 0.99),
            text_dim: Color::srgb(0.63, 0.63, 0.76),
            accent: Color::srgb(0.60, 0.88, 1.0),
            button: Color::srgb(0.14, 0.12, 0.24),
            button_hover: Color::srgb(0.21, 0.18, 0.34),
            button_press: Color::srgb(0.30, 0.22, 0.52),
            corp: Color::srgb(0.16, 0.42, 0.85),
            runner: Color::srgb(0.80, 0.16, 0.20),
            danger: Color::srgb(0.90, 0.35, 0.30),
            legal: Color::srgb(0.45, 0.88, 0.66),
            caution: Color::srgb(0.98, 0.76, 0.36),
            glow_usable: Color::srgb(0.76, 0.55, 1.0),
            glow_conditional: Color::srgb(1.0, 0.99, 0.70),
            glass: Color::srgba(0.12, 0.10, 0.30, 0.70),
            glass_strong: Color::srgba(0.09, 0.07, 0.22, 0.92),
            glass_border: Color::srgba(0.82, 0.80, 1.0, 0.26),
            primary: Color::srgb(0.56, 0.42, 1.0),
            primary_hover: Color::srgb(0.66, 0.54, 1.0),
            primary_press: Color::srgb(0.45, 0.32, 0.88),
            on_primary: Color::srgb(0.98, 0.98, 1.0),
            secondary: Color::srgba(0.72, 0.68, 1.0, 0.12),
            secondary_hover: Color::srgba(0.72, 0.68, 1.0, 0.24),
            secondary_press: Color::srgba(0.72, 0.68, 1.0, 0.36),
            border_hover: Color::srgba(0.80, 0.84, 1.0, 0.60),
            backdrop_top: Color::srgb(0.06, 0.15, 0.22),
            backdrop_bottom: Color::srgb(0.06, 0.03, 0.14),
            backdrop_bloom: Color::srgba(0.35, 0.60, 1.0, 0.16),
            backdrop_bloom_violet: Color::srgba(0.62, 0.35, 1.0, 0.18),
            wash: Color::srgb(0.04, 0.03, 0.11),
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
    /// A section's name inside a panel: small capitals-in-spirit, spaced
    /// out, dim — a label for a group, not a heading to read.
    pub const OVERLINE: f32 = 13.0;
}

/// The shape of a menu, in logical pixels, so a panel on the settings
/// screen is the panel on the new-game screen.
pub mod shape {
    /// A glass panel's corner.
    pub const PANEL_RADIUS: f32 = 18.0;
    /// A drop-down's open list and a row inside it.
    pub const LIST_RADIUS: f32 = 14.0;
    pub const ROW_RADIUS: f32 = 10.0;
    /// The least height of a button: a comfortable target for a pointer.
    pub const BUTTON_HEIGHT: f32 = 44.0;
}

impl Theme {
    /// `fill` as it looks on the glass of a pop-up over the wash, with
    /// nothing behind it showing through.
    ///
    /// **For a glowing button.** A `BoxShadow` is painted under the whole
    /// node, not only outside it, so under a translucent pill (a
    /// `secondary` fill is 12% opaque) the glow floods the button and its
    /// white label sits on pale yellow — the Keep and the Mulligan were
    /// unreadable that way. Made solid, the pill hides the middle of its
    /// shadow and the glow is a ring again, as it is round a card.
    pub fn solid(&self, fill: Color) -> Color {
        over(fill, over(self.glass_strong, self.wash))
    }

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

    #[test]
    fn a_glowing_pill_is_solid() {
        let theme = Theme::default();
        for fill in [theme.secondary, theme.secondary_hover, theme.secondary_press, Color::NONE, theme.primary] {
            assert_eq!(theme.solid(fill).alpha(), 1.0);
        }
        assert_eq!(theme.solid(theme.primary), theme.primary);
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
    /// The accent rings the encountered ice and a card that just changed,
    /// and a Corp card's own border is `corp`: both blue, so the accent
    /// is kept much lighter than the Corp's blue or a ring on a Corp card
    /// would be no ring at all.
    #[test]
    fn the_accent_is_not_the_corps_blue() {
        let theme = Theme::default();
        let (distance, who) = closest(theme.accent, theme.corp);
        assert!(distance >= APART, "the accent and the Corp's blue are {distance:.3} OKLab apart under {who}");
    }

    /// The accent rings the encountered ice and a card that just changed;
    /// the usable glow rings a card that can act. Both sit on a card's
    /// edge, so a ring must never be mistaken for an offer.
    #[test]
    fn the_accent_is_not_the_usable_glow() {
        let theme = Theme::default();
        let (distance, who) = closest(theme.accent, theme.glow_usable);
        assert!(distance >= APART, "the accent and the usable glow are {distance:.3} OKLab apart under {who}");
    }

    #[test]
    fn a_broken_subroutine_and_a_fired_one_are_told_apart() {
        let theme = Theme::default();
        let (distance, who) = closest(theme.accent, theme.danger);
        assert!(distance >= APART, "broken and fired dots are {distance:.3} OKLab apart under {who}");
    }
}
