//! What a skin says about itself: which picture dresses which part of
//! the board, how it stretches, and what colour to wash it in.
//!
//! Here rather than in `netrunner_desktop` for the reason
//! [`crate::table`]'s manifest is here: it is data written by hand, it is
//! worth reading without a window, and the pixels are somebody else's
//! problem. The graphical client owns the slot list, decodes the files
//! and decides what an undressed slot looks like.
//!
//! **Every part of this is optional, at two levels, and that is the
//! point.** A slot nobody has drawn keeps the board's own outlined look;
//! a *state* nobody has drawn — a pressed button, a rezzed tile — falls
//! back to the slot's base picture, tinted with the colour that state
//! already uses. So a skin can be one PNG, and the first thing somebody
//! makes shows up on the board without the other thirty.
//!
//! A skin never changes the size of anything. The board's boxes are
//! `models::layout`'s and the widgets' own; a picture fits the box it is
//! given. That is what keeps the no-scroll guarantee out of an artist's
//! hands.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The file inside a skin's folder.
pub const MANIFEST_FILE: &str = "skin.json";

/// How a slot's picture is fitted to its box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Nine-slice: the corners keep their size, the edges and middle
    /// stretch. What a bubble or a frame wants, and the default whenever
    /// `insets` are given.
    #[default]
    Sliced,
    /// Stretched corner to corner, aspect ignored. For a picture with
    /// nothing at its edges worth preserving.
    Stretch,
    /// Drawn at its own aspect inside the box. What an icon wants.
    Fit,
}

/// The four inset distances that cut a nine-slice, in the picture's own
/// pixels.
///
/// Written either as one number for all four sides or as
/// `[left, top, right, bottom]`, because a person writing this by hand
/// will reach for whichever is shorter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Insets {
    All(f32),
    Sides([f32; 4]),
}

impl Insets {
    /// `(left, top, right, bottom)`.
    pub fn sides(self) -> (f32, f32, f32, f32) {
        match self {
            Insets::All(n) => (n, n, n, n),
            Insets::Sides([l, t, r, b]) => (l, t, r, b),
        }
    }
}

/// What a skin says about one slot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlotArt {
    /// The picture, relative to the skin's folder. Without one the entry
    /// is still useful: a `tint` alone re-washes the base slot's
    /// picture, which is how a hover state is written when the art is
    /// the same shape in a different colour.
    pub file: Option<String>,
    /// The nine-slice cut. Present implies [`Mode::Sliced`].
    pub insets: Option<Insets>,
    /// `#rrggbb`, multiplied into the picture.
    pub tint: Option<String>,
    /// Overrides what `insets` would imply.
    pub mode: Option<Mode>,
}

impl SlotArt {
    /// How this entry is fitted: what it says, else nine-slice when it
    /// gave insets, else stretched.
    pub fn mode(&self) -> Mode {
        self.mode.unwrap_or(if self.insets.is_some() { Mode::Sliced } else { Mode::Stretch })
    }

    /// The tint as `(r, g, b)`, if it parsed.
    pub fn tint_rgb(&self) -> Option<(u8, u8, u8)> {
        crate::table::parse_hex(self.tint.as_deref()?)
    }
}

/// A skin's `skin.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    /// What the settings row calls it. The folder name when unset.
    pub name: Option<String>,
    /// Slot key to picture. A key is a slot (`"tile"`) or a slot and a
    /// state (`"tile.rezzed"`); the client owns the vocabulary and
    /// ignores a key it does not know, so a skin written for a later
    /// version still loads.
    pub slots: BTreeMap<String, SlotArt>,
    /// Whether the board should still draw its own contact shadows.
    /// A skin whose pictures carry their own sets this false rather than
    /// having two.
    pub shadows: Option<bool>,
}

impl Manifest {
    /// What a folder with no manifest — or an unreadable one — is worth:
    /// its own name, and no art at all, which is the undressed board.
    pub fn for_folder(folder: &str) -> Self {
        Self { name: Some(folder.to_string()), ..Self::default() }
    }

    /// Parsed, falling back to [`Manifest::for_folder`] on any error, so
    /// a typo costs a skin its pictures rather than stopping the client.
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

    /// The entry for `key`, if the skin has one.
    pub fn slot(&self, key: &str) -> Option<&SlotArt> {
        self.slots.get(key)
    }

    /// Whether the board draws its own shadows over this skin.
    pub fn wants_shadows(&self) -> bool {
        self.shadows.unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_without_a_manifest_dresses_nothing() {
        let manifest = Manifest::for_folder("neon-chrome");
        assert_eq!(manifest.label("neon-chrome"), "neon-chrome");
        assert!(manifest.slots.is_empty());
        assert!(manifest.wants_shadows(), "a skin says so only by saying so");
        assert_eq!(manifest.slot("tile"), None);
    }

    #[test]
    fn a_manifest_is_read_and_a_broken_one_costs_only_its_pictures() {
        let json = r##"{
            "name": "Neon Chrome",
            "shadows": false,
            "slots": {
                "tile": { "file": "tile.png", "insets": 6 },
                "tile.rezzed": { "file": "tile-lit.png", "insets": [4, 6, 4, 6] },
                "button.hover": { "tint": "#8ffff0" },
                "server.mark.hq": { "file": "hq.png", "mode": "fit" },
                "not.a.slot": { "file": "later.png" }
            }
        }"##;
        let manifest = Manifest::parse("neon-chrome", json);
        assert_eq!(manifest.label("neon-chrome"), "Neon Chrome");
        assert!(!manifest.wants_shadows(), "its pictures carry their own");

        let tile = manifest.slot("tile").unwrap();
        assert_eq!(tile.file.as_deref(), Some("tile.png"));
        assert_eq!(tile.insets.unwrap().sides(), (6.0, 6.0, 6.0, 6.0), "one number is all four sides");
        assert_eq!(tile.mode(), Mode::Sliced, "insets imply a nine-slice");

        assert_eq!(manifest.slot("tile.rezzed").unwrap().insets.unwrap().sides(), (4.0, 6.0, 4.0, 6.0));

        // A state written as a tint alone: no file, so the base slot's
        // picture is re-washed rather than replaced.
        let hover = manifest.slot("button.hover").unwrap();
        assert_eq!(hover.file, None);
        assert_eq!(hover.tint_rgb(), Some((0x8f, 0xff, 0xf0)));
        assert_eq!(hover.mode(), Mode::Stretch, "no insets, nothing to preserve");

        assert_eq!(manifest.slot("server.mark.hq").unwrap().mode(), Mode::Fit, "an icon keeps its aspect");

        // A key from a later version is kept rather than refused, so a
        // skin does not have to be written against one client.
        assert!(manifest.slot("not.a.slot").is_some());

        let broken = Manifest::parse("neon-chrome", "{ nope");
        assert_eq!(broken, Manifest::for_folder("neon-chrome"));
        assert!(broken.slots.is_empty(), "a typo undresses the board, it does not stop it");
    }

    #[test]
    fn a_manifest_that_sets_only_one_thing_keeps_the_folders_name() {
        let manifest = Manifest::parse("orbital", r#"{"slots":{"tile":{"file":"t.png"}}}"#);
        assert_eq!(manifest.label("orbital"), "orbital");
        assert_eq!(manifest.slots.len(), 1);
    }
}
