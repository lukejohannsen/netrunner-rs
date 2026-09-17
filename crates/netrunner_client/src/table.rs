//! What comes with a table besides its picture: the name to show it
//! under, and the two colour facts a board needs so its panels and its
//! text read *against* the field rather than fighting it.
//!
//! The picture is the client's business and the manifest is not, which
//! is why this sits here rather than in `netrunner_desktop`: it is data,
//! it is hand-written by whoever made the table, and it is worth testing
//! without a window. The graphical client resolves the folder, decodes
//! the images and paints the fallback; all of that needs Bevy and none
//! of it needs this.
//!
//! A manifest is optional. A folder holding nothing but `base.jpg` is a
//! table — it is named after its folder and keeps the theme's own accent
//! — because the first thing someone does is drop in a picture, and
//! being made to write JSON first would be a reason not to bother.

use serde::{Deserialize, Serialize};

/// The file inside a table's folder, if it has one.
pub const MANIFEST_FILE: &str = "table.json";
/// The field itself. JPEG because a painted ground is photographic and
/// has no alpha: at 2560×1440 it is about a megabyte where a PNG is six.
pub const BASE_FILE: &str = "base.jpg";
/// Optional, and PNG precisely because it *does* have alpha: a vignette,
/// a frame, a wash of colour laid over the field and under the cards.
pub const OVERLAY_FILE: &str = "overlay.png";

/// Whether a table is dark enough to read light text on, or light enough
/// to need dark. Everything in this client is built for a dark ground,
/// so that is the default and the untested path is the other one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ink {
    #[default]
    Dark,
    Light,
}

/// A table's `table.json`, as written by hand.
///
/// Every field is optional, and [`Manifest::for_folder`] is what a
/// folder without one gets, so a malformed or missing manifest costs the
/// table its colours and never its picture.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    /// What the settings row calls it. The folder name when unset.
    pub name: Option<String>,
    /// A colour drawn from the field, as `#rrggbb`, for the board to
    /// pick out its live edges with. The theme's own accent when unset.
    pub accent: Option<String>,
    /// How dark the field is, which decides which way the text goes.
    pub ink: Ink,
    /// The skin this field was made to be seen with, if any. Read only
    /// when the player's skin setting is `Auto`, so naming one here is a
    /// suggestion the table makes and never a choice it takes away.
    pub skin: Option<String>,
}

impl Manifest {
    /// What a folder with no manifest — or an unreadable one — is worth:
    /// its own name, and nothing that would override the theme.
    pub fn for_folder(folder: &str) -> Self {
        Self { name: Some(folder.to_string()), ..Self::default() }
    }

    /// Parsed, falling back to [`Manifest::for_folder`] on any error.
    /// A table whose JSON has a typo in it still draws; a person editing
    /// one by hand finds out by the name not changing, which is a better
    /// failure than a client that will not start.
    pub fn parse(folder: &str, json: &str) -> Self {
        match serde_json::from_str::<Manifest>(json) {
            Ok(manifest) => Self { name: manifest.name.or_else(|| Some(folder.to_string())), ..manifest },
            Err(_) => Self::for_folder(folder),
        }
    }

    /// The name to show, which is always something.
    pub fn label<'a>(&'a self, folder: &'a str) -> &'a str {
        self.name.as_deref().unwrap_or(folder)
    }

    /// The accent as `(r, g, b)`, if it parsed. `#rgb` and `#rrggbb`,
    /// with or without the hash, because a person writing the file by
    /// hand will use whichever they know.
    pub fn accent_rgb(&self) -> Option<(u8, u8, u8)> {
        parse_hex(self.accent.as_deref()?)
    }
}

/// `#1ec8b4`, `1ec8b4`, `#1cb` — or `None`. Public because a skin's
/// tints are written the same way, by the same hand, in a sibling file.
pub fn parse_hex(text: &str) -> Option<(u8, u8, u8)> {
    let hex = text.trim().trim_start_matches('#');
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        6 => Some((pair(0)?, pair(2)?, pair(4)?)),
        // A three-digit code doubles each digit: `#1cb` is `#11ccbb`.
        3 => {
            let digit = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
            Some((digit(0)?, digit(1)?, digit(2)?))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_without_a_manifest_is_named_after_itself() {
        let manifest = Manifest::for_folder("neon-alley");
        assert_eq!(manifest.label("neon-alley"), "neon-alley");
        assert_eq!(manifest.ink, Ink::Dark);
        assert_eq!(manifest.accent_rgb(), None);
    }

    #[test]
    fn a_manifest_is_read_and_a_broken_one_costs_only_its_colours() {
        let manifest = Manifest::parse("neon-alley", r##"{"name":"Neon Alley","accent":"#1ec8b4","ink":"light"}"##);
        assert_eq!(manifest.label("neon-alley"), "Neon Alley");
        assert_eq!(manifest.accent_rgb(), Some((0x1e, 0xc8, 0xb4)));
        assert_eq!(manifest.ink, Ink::Light);

        // A typo anywhere in the file, and the table still draws.
        let broken = Manifest::parse("neon-alley", "{ this is not json");
        assert_eq!(broken.label("neon-alley"), "neon-alley");
        assert_eq!(broken, Manifest::for_folder("neon-alley"));

        // A manifest that sets only one thing keeps the folder's name.
        let partial = Manifest::parse("orbital", r#"{"ink":"light"}"#);
        assert_eq!(partial.label("orbital"), "orbital");
        assert_eq!(partial.ink, Ink::Light);
    }

    #[test]
    fn an_accent_is_read_in_the_spellings_a_person_would_write() {
        for text in ["#1ec8b4", "1ec8b4", "  #1EC8B4  "] {
            assert_eq!(parse_hex(text), Some((0x1e, 0xc8, 0xb4)), "{text}");
        }
        assert_eq!(parse_hex("#1cb"), Some((0x11, 0xcc, 0xbb)), "three digits double");
        for text in ["", "#", "#12345", "#gggggg", "teal"] {
            assert_eq!(parse_hex(text), None, "{text}");
        }
    }
}
