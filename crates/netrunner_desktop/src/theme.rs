//! One place for how the client looks: its font, its palette, its sizes.
//!
//! The palette is Netrunner's: the Corp is blue and the Runner red, as
//! Null Signal Games prints their card backs, and each faction keeps the
//! colour its cards carry. Everything else is a dark ground those colours
//! read against.

use bevy::prelude::*;

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
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font: None,
            symbol_font: None,
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
}
