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
///   "credit": { "splash": { "artist": "…", "website": "https://…", "title": "…" } } }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    /// Per key, 0 (the picture as drawn) to 1 (the flat ground). A key
    /// with no entry takes [`SHARED_KEY`]'s, then [`DEFAULT_DIM`].
    pub dim: BTreeMap<String, f32>,
    /// Per key, who made that key's own picture.
    ///
    /// **For a picture in the player's own folder, never a committed
    /// one.** A committed picture is credited in `assets/CREDITS.md`,
    /// where a test holds it to a licence; a picture a player drops into
    /// `<data dir>` is theirs to have, and may be one nobody could commit
    /// (the art that asked for this was "all rights reserved"). The
    /// About screen still names its artist, because a person looking at
    /// the art should be able to find out whose it is. Only the key's
    /// own picture is credited, never the shared one it falls back to —
    /// `menu`'s credit sits under `menu`.
    pub credit: BTreeMap<String, Credit>,
}

/// Who made a picture and where to find them, as the player wrote it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Credit {
    pub artist: String,
    /// The artist's site, or the picture's page on it.
    pub website: String,
    /// The work's name, if the player gave one.
    pub title: String,
}

impl Manifest {
    /// Parsed, or the empty manifest on any error.
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// How much `key`'s picture is dimmed, clamped to 0..=1.
    pub fn dim(&self, key: &str) -> f32 {
        self.dim.get(key).or_else(|| self.dim.get(SHARED_KEY)).copied().unwrap_or(DEFAULT_DIM).clamp(0.0, 1.0)
    }

    /// The credits for the keys `has_picture` says have a picture of
    /// their own, grouped by artist and website in key order, each with
    /// the keys and titles it covers. An entry naming no artist is
    /// dropped: a credit is a name, and a blank one credits nobody.
    pub fn credits(&self, has_picture: impl Fn(&str) -> bool) -> Vec<CreditGroup> {
        let mut groups: Vec<CreditGroup> = Vec::new();
        for (key, credit) in &self.credit {
            if credit.artist.trim().is_empty() || !has_picture(key) {
                continue;
            }
            let group = match groups.iter_mut().position(|g| g.artist == credit.artist && g.website == credit.website) {
                Some(index) => &mut groups[index],
                None => {
                    groups.push(CreditGroup { artist: credit.artist.clone(), website: credit.website.clone(), keys: Vec::new(), titles: Vec::new() });
                    groups.last_mut().expect("just pushed")
                }
            };
            group.keys.push(key.clone());
            if !credit.title.is_empty() && !group.titles.contains(&credit.title) {
                group.titles.push(credit.title.clone());
            }
        }
        groups
    }
}

/// One artist's pictures on the player's screens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditGroup {
    pub artist: String,
    pub website: String,
    /// The backdrop keys their pictures fill.
    pub keys: Vec<String>,
    pub titles: Vec<String>,
}

/// The files a key's *own* picture may be, without the shared fallback.
pub fn own_files(key: &str) -> Vec<String> {
    EXTENSIONS.iter().map(|extension| format!("{DIR}/{key}.{extension}")).collect()
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
    fn a_credit_is_shown_only_for_a_picture_that_is_there_and_one_artist_is_one_credit() {
        let manifest = Manifest::parse(
            r#"{ "credit": {
                "splash": { "artist": "A. Painter", "website": "https://a.example", "title": "City (smog)" },
                "main-menu": { "artist": "A. Painter", "website": "https://a.example", "title": "City (neon)" },
                "cards": { "artist": "B. Drawer", "website": "https://b.example" },
                "about": { "artist": " ", "website": "https://blank.example" } } }"#,
        );
        let credits = manifest.credits(|key| key != "cards");
        assert_eq!(credits.len(), 1, "no file for cards, and a blank artist credits nobody: {credits:?}");
        assert_eq!(credits[0].artist, "A. Painter");
        assert_eq!(credits[0].keys, vec!["main-menu", "splash"]);
        assert_eq!(credits[0].titles, vec!["City (neon)", "City (smog)"]);
        assert!(Manifest::parse(r#"{ "dim": { "menu": 0.2 } }"#).credits(|_| true).is_empty(), "a manifest from before credits still parses");
        assert_eq!(own_files("splash"), vec!["backdrops/splash.jpg", "backdrops/splash.png"]);
    }
}
