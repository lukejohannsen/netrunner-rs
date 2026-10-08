//! A person's notes on a recorded match, kept beside the record (Phase 7
//! §8 item 5: "replays with notes and bookmarks").
//!
//! **A note is about a position**, the one the viewer is standing at
//! — "here is where I should have run" — so the book is a map from the
//! record's cursor (how many actions have been applied) to a line of
//! text, and a position with a note is also a bookmark: the replay bar
//! steps from note to note. **The file sits beside the record and never
//! inside it:** the record is the bug report, written once and replayed
//! bit for bit, and a note is the person's and changes; a second file of
//! its own, `<record>.notes.json`, leaves the record untouched and lets a
//! report be sent without the notes or with them. The desktop writes the
//! book; the terminal's `replay` reads it, so a note written in one
//! client is read in the other.
//!
//! **A note is a line.** The desktop's one text field is one line of up
//! to [`MAX_LEN`] characters (`widgets::text_field`, which says that a
//! second line is the point to adopt Bevy's own editor), and a note that
//! wants a paragraph is a sign the position wants two notes.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The most characters a note holds.
pub const MAX_LEN: usize = 200;

/// The notes on one record, by the position each is about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notes {
    #[serde(default)]
    pub at: BTreeMap<usize, String>,
}

impl Notes {
    /// The note at `position`, if one is written.
    pub fn get(&self, position: usize) -> Option<&str> {
        self.at.get(&position).map(String::as_str)
    }

    /// Writes `text` as the note at `position`; blank removes it.
    pub fn set(&mut self, position: usize, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            self.at.remove(&position);
        } else {
            self.at.insert(position, text.chars().take(MAX_LEN).collect());
        }
    }

    /// Every position with a note, in order: the bookmarks.
    pub fn positions(&self) -> Vec<usize> {
        self.at.keys().copied().collect()
    }

    /// The first noted position after `position`.
    pub fn next_after(&self, position: usize) -> Option<usize> {
        self.at.range(position + 1..).next().map(|(p, _)| *p)
    }

    /// The last noted position before `position`.
    pub fn previous_before(&self, position: usize) -> Option<usize> {
        self.at.range(..position).next_back().map(|(p, _)| *p)
    }
}

/// Where a record's notes live: beside it, `<stem>.notes.json`.
pub fn path_for(record: &Path) -> PathBuf {
    let stem = record.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    record.with_file_name(format!("{stem}.notes.json"))
}

/// The notes beside `record`: none when the file is absent, and none —
/// rather than an error — when it cannot be read, since a replay is
/// still a replay without its notes.
pub fn load(record: &Path) -> Notes {
    std::fs::read_to_string(path_for(record)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Writes the notes beside `record`; a book with no notes left removes
/// the file, so an untouched record has nothing beside it.
pub fn save(record: &Path, notes: &Notes) -> io::Result<()> {
    let path = path_for(record);
    if notes.at.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    let text = serde_json::to_string_pretty(notes).map_err(io::Error::other)?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_set_trimmed_capped_and_removed_blank_and_the_bookmarks_step_either_way() {
        let mut notes = Notes::default();
        notes.set(3, "  run here  ");
        notes.set(7, &"x".repeat(MAX_LEN + 50));
        notes.set(5, "");
        assert_eq!(notes.get(3), Some("run here"));
        assert_eq!(notes.get(7).map(str::len), Some(MAX_LEN));
        assert_eq!(notes.get(5), None);
        assert_eq!(notes.positions(), vec![3, 7]);
        assert_eq!(notes.next_after(0), Some(3));
        assert_eq!(notes.next_after(3), Some(7));
        assert_eq!(notes.next_after(7), None);
        assert_eq!(notes.previous_before(7), Some(3));
        assert_eq!(notes.previous_before(3), None);
        notes.set(3, "   ");
        assert_eq!(notes.positions(), vec![7], "blank removes");
    }

    #[test]
    fn the_book_lives_beside_the_record_and_goes_when_it_is_empty() {
        let dir = std::env::temp_dir().join(format!("netrunner_notes_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("2026-09-22T20-15-03-seed42.jsonl");
        assert_eq!(path_for(&record), dir.join("2026-09-22T20-15-03-seed42.notes.json"));
        assert_eq!(load(&record), Notes::default(), "no file is no notes");
        let mut notes = Notes::default();
        notes.set(4, "the Corp iced HQ first");
        save(&record, &notes).unwrap();
        assert_eq!(load(&record), notes);
        notes.set(4, "");
        save(&record, &notes).unwrap();
        assert!(!path_for(&record).exists(), "an empty book removes its file");
        save(&record, &notes).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
