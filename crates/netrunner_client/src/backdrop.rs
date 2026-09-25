//! The picture behind a screen that is not the board — the splash, the
//! main menu and every screen off it — as data: which files a screen's
//! backdrop may come from, in order, and how much its manifest asks for
//! the picture to be dimmed under the screen's text.
//!
//! **Every screen has a slot before anybody has drawn for it.** The
//! graphical client names each screen's key from its own screen enum, by
//! an exhaustive match, so a new screen does not compile until it has
//! one; this module is the part of the rule that is worth testing
//! without a window. The pictures are the client's business, as a
//! table's are (`crate::table`).
//!
//! **One shared picture dresses every screen nobody drew for.** A key's
//! own file wins, then [`SHARED_KEY`]'s, then the client's drawn tier —
//! the flat ground the screens always had. So a single `menu.jpg` is a
//! finished set of menus, and a screen's own picture is an improvement
//! on it rather than a prerequisite.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Where backdrops live, under either asset tier.
pub const DIR: &str = "backdrops";

/// The key whose picture dresses every screen without its own.
pub const SHARED_KEY: &str = "menu";

/// The optional manifest beside the pictures.
pub const MANIFEST_FILE: &str = "backdrops.json";

/// The extensions a backdrop is looked for under, in order. JPEG first,
/// because a full-window picture is photographic and has no alpha — at
/// 2560 × 1600 it is about a megabyte where a PNG is six — and PNG for
/// the picture that does want alpha, over the screen's flat ground.
pub const EXTENSIONS: [&str; 2] = ["jpg", "png"];

/// How much a picture is dimmed under the screen's text when the
/// manifest says nothing: enough that the theme's light text reads over
/// a bright picture, little enough that the picture is still the point.
pub const DEFAULT_DIM: f32 = 0.45;

/// Every file `key`'s backdrop may come from, first found wins: its own
/// picture in each extension, then the shared one's. The shared key's
/// own list does not repeat itself.
pub fn candidates(key: &str) -> Vec<String> {
    let mut keys = vec![key];
    if key != SHARED_KEY {
        keys.push(SHARED_KEY);
    }
    keys.into_iter().flat_map(|key| EXTENSIONS.iter().map(move |extension| format!("{DIR}/{key}.{extension}"))).collect()
}

/// `backdrops/backdrops.json`, as written by hand.
///
/// Optional at every level, as a table's manifest is: no file, a file
/// that will not parse, and a key it does not mention all mean
/// [`DEFAULT_DIM`], so a typo costs a picture its dimming and never the
/// picture.
///
/// ```json
/// { "dim": { "menu": 0.5, "cards": 0.7, "splash": 0.0 },
///   "same_as": { "deck-editor": "decks" } }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    /// Per key, 0 (the picture as drawn) to 1 (the flat ground). A key
    /// with no entry takes [`SHARED_KEY`]'s, then [`DEFAULT_DIM`].
    pub dim: BTreeMap<String, f32>,
    /// Per key, another key whose picture this screen shows. **A screen
    /// that wants a particular screen's picture says so here rather than
    /// holding a copy of the file**, and rather than the picture becoming
    /// [`SHARED_KEY`]'s, which dresses every screen nobody drew for: the
    /// deck editor stands on the deck list's picture while the other
    /// screens keep the shared one. One step only — the key named
    /// here is looked up as itself, its own entry unread — so no chain
    /// can loop. The dim stays the screen's own.
    pub same_as: BTreeMap<String, String>,
}

impl Manifest {
    /// Parsed, or the empty manifest on any error.
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// The key whose files `key`'s picture is looked for under: its
    /// [`Manifest::same_as`] entry, or itself.
    pub fn picture_key<'a>(&'a self, key: &'a str) -> &'a str {
        self.same_as.get(key).map_or(key, String::as_str)
    }

    /// How much `key`'s picture is dimmed, clamped to 0..=1.
    pub fn dim(&self, key: &str) -> f32 {
        self.dim.get(key).or_else(|| self.dim.get(SHARED_KEY)).copied().unwrap_or(DEFAULT_DIM).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_looks_for_its_own_picture_then_the_shared_one() {
        assert_eq!(candidates("cards"), vec!["backdrops/cards.jpg", "backdrops/cards.png", "backdrops/menu.jpg", "backdrops/menu.png"]);
        assert_eq!(candidates(SHARED_KEY), vec!["backdrops/menu.jpg", "backdrops/menu.png"], "the shared key does not repeat itself");
    }

    #[test]
    fn the_dim_falls_back_key_then_shared_then_default_and_survives_a_typo() {
        let manifest = Manifest::parse(r#"{ "dim": { "menu": 0.2, "cards": 0.7, "splash": 3.0 } }"#);
        assert_eq!(manifest.dim("cards"), 0.7);
        assert_eq!(manifest.dim("settings"), 0.2, "a screen it does not name takes the shared key's");
        assert_eq!(manifest.dim("splash"), 1.0, "clamped");
        assert_eq!(Manifest::parse("{ not json").dim("cards"), DEFAULT_DIM);
        assert_eq!(Manifest::default().dim("cards"), DEFAULT_DIM);
    }

    #[test]
    fn a_screen_may_show_another_screens_picture_one_step_only() {
        let manifest = Manifest::parse(r#"{ "same_as": { "about": "main-menu", "main-menu": "splash" }, "dim": { "main-menu": 0.3 } }"#);
        assert_eq!(manifest.picture_key("about"), "main-menu", "not followed on to splash");
        assert_eq!(manifest.picture_key("cards"), "cards");
        assert_eq!(manifest.dim("about"), DEFAULT_DIM, "the dim is the screen's own, not the picture's");
    }
}
