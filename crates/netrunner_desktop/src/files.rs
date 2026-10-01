//! Decks as files: the native dialog, asked off the main thread.
//!
//! **A deck is imported from a file and exported to one through the OS's
//! own dialog** (Phase 7 §10 Stage 5, 30 September 2026). Import and
//! export went through the clipboard until then — a decklist pasted from
//! NetrunnerDB, a list copied out to a friend — with a file dropped on
//! the window the one way a file came in. The person asked for files, so
//! a deck can be shared as one, and chose the native dialog over a file
//! browser drawn in the client: it is the dialog they already know, it
//! knows their folders, and it is one less screen to keep looking like
//! the last one. That is `rfd`, over the XDG desktop portal on Linux (no
//! GTK linked) and the system dialogs elsewhere. Export writes `.txt`,
//! the text shape NetrunnerDB's own download has and `deck_builder`
//! reads back; import takes that or this project's `.json` deck file.
//!
//! **Bevy runs on the main thread, so the dialog does not.** The dialog
//! blocks for as long as it is open, so it runs on one of
//! `core::TokioRuntime`'s blocking threads — not a worker, which a
//! download shares — and answers through a channel the screen polls each
//! frame, the way `downloads` does: a dialog opened on the main thread
//! would freeze the window behind it. Export writes the file on the same
//! thread, with the text it was given when the dialog opened — the deck
//! as it was when Export was pressed.
//!
//! **A headless test scripts the answer.** [`DeckFiles::scripted`]
//! answers every dialog with one path, at once, so the navigation tests
//! drive the real buttons through a real file in a scratch directory and
//! no dialog opens under CI.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use tokio::sync::oneshot;

use crate::core::TokioRuntime;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<DeckFiles>();
}

/// What is asked of the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// A decklist to import.
    Open,
    /// Where to write `text`, offered as `<name>.txt` in Downloads.
    Save { name: String, text: String },
}

/// The dialog's answer.
#[derive(Debug, PartialEq, Eq)]
pub enum Done {
    /// The file to import, or nothing: the dialog was cancelled.
    Opened(Option<PathBuf>),
    /// Where the text was written, or why it was not; nothing is a
    /// cancelled dialog.
    Saved(Option<Result<PathBuf, String>>),
}

enum Pending {
    /// The dialog is open; `saving` says which answer a closed channel
    /// stands for.
    Channel { answer: oneshot::Receiver<Done>, saving: bool },
    /// A scripted answer, handed over on the next poll.
    Ready(Done),
}

/// The one dialog a screen may have open, and how it is answered.
#[derive(Resource, Default)]
pub struct DeckFiles {
    /// `Some(path)`: every dialog answers with `path` (or nothing) at
    /// once, for a test. `None`: the native dialog.
    scripted: Option<Option<PathBuf>>,
    pending: Option<Pending>,
}

impl DeckFiles {
    /// Answers every dialog with `path` — `None` for a cancelled one —
    /// without opening any.
    pub fn scripted(path: Option<PathBuf>) -> Self {
        DeckFiles { scripted: Some(path), pending: None }
    }

    pub fn is_asking(&self) -> bool {
        self.pending.is_some()
    }

    /// Opens the dialog `ask` names, or says why it cannot: one is open
    /// already, or there is no runtime to open it on.
    pub fn ask(&mut self, runtime: Option<&TokioRuntime>, ask: Ask) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("A file dialog is already open".to_string());
        }
        if let Some(path) = &self.scripted {
            let done = match ask {
                Ask::Open => Done::Opened(path.clone()),
                Ask::Save { text, .. } => Done::Saved(path.as_ref().map(|path| write(path, &text))),
            };
            self.pending = Some(Pending::Ready(done));
            return Ok(());
        }
        let Some(runtime) = runtime else { return Err("There is no runtime to open a file dialog on".to_string()) };
        let saving = matches!(ask, Ask::Save { .. });
        let (tx, answer) = oneshot::channel();
        runtime.0.spawn_blocking(move || {
            let done = match ask {
                Ask::Open => Done::Opened(open_dialog().pick_file()),
                Ask::Save { name, text } => Done::Saved(save_dialog(&name).save_file().map(|path| write(&path, &text))),
            };
            let _ = tx.send(done);
        });
        self.pending = Some(Pending::Channel { answer, saving });
        Ok(())
    }

    /// The answer, once it has come; nothing while the dialog is open or
    /// none was asked. A dialog whose task died is a cancelled one.
    pub fn poll(&mut self) -> Option<Done> {
        match self.pending.take()? {
            Pending::Ready(done) => Some(done),
            Pending::Channel { mut answer, saving } => match answer.try_recv() {
                Ok(done) => Some(done),
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.pending = Some(Pending::Channel { answer, saving });
                    None
                }
                Err(oneshot::error::TryRecvError::Closed) => Some(if saving { Done::Saved(None) } else { Done::Opened(None) }),
            },
        }
    }
}

/// The decklist filter both dialogs offer: NetrunnerDB's text shape and
/// this project's deck file.
fn decklists(dialog: rfd::FileDialog) -> rfd::FileDialog {
    dialog.add_filter("Decklist", &["txt", "json"])
}

fn open_dialog() -> rfd::FileDialog {
    decklists(rfd::FileDialog::new().set_title("Import a decklist"))
}

/// The save dialog opens in Downloads, where a file to pass on goes,
/// offering the deck's name; a name with a path separator in it would
/// be a folder, so those become dashes.
fn save_dialog(name: &str) -> rfd::FileDialog {
    let mut dialog = decklists(rfd::FileDialog::new().set_title("Export the deck as a decklist")).set_file_name(file_name(name));
    if let Some(downloads) = dirs::download_dir() {
        dialog = dialog.set_directory(downloads);
    }
    dialog
}

/// `<name>.txt`, safe as one file's name.
pub fn file_name(name: &str) -> String {
    format!("{}.txt", name.trim().replace(['/', '\\'], "-"))
}

fn write(path: &Path, text: &str) -> Result<PathBuf, String> {
    std::fs::write(path, text).map(|()| path.to_path_buf()).map_err(|error| format!("{} could not be written: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted answer comes on the first poll and is then spent; a
    /// second dialog is refused while the first is unanswered.
    #[test]
    fn a_scripted_dialog_answers_at_once_and_one_at_a_time() {
        let dir = std::env::temp_dir().join(format!("netrunner_files_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        let mut files = DeckFiles::scripted(Some(path.clone()));
        assert_eq!(files.poll(), None, "nothing asked, nothing answered");
        files.ask(None, Ask::Save { name: "Mine".into(), text: "a list\n".into() }).unwrap();
        assert!(files.is_asking());
        assert!(files.ask(None, Ask::Open).is_err(), "one dialog at a time");
        assert_eq!(files.poll(), Some(Done::Saved(Some(Ok(path.clone())))));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a list\n");
        assert_eq!(files.poll(), None, "an answer is given once");
        files.ask(None, Ask::Open).unwrap();
        assert_eq!(files.poll(), Some(Done::Opened(Some(path))));
        let mut cancelled = DeckFiles::scripted(None);
        cancelled.ask(None, Ask::Open).unwrap();
        assert_eq!(cancelled.poll(), Some(Done::Opened(None)));
        assert!(DeckFiles::default().ask(None, Ask::Open).is_err(), "no runtime, no native dialog");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_name_is_the_decks_name_with_no_path_in_it() {
        assert_eq!(file_name("  Stolen Goods "), "Stolen Goods.txt");
        assert_eq!(file_name("a/b\\c"), "a-b-c.txt");
    }
}
