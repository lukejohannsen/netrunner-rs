//! A deck from NetrunnerDB: the fetch, asked off the main thread.
//!
//! **A published decklist is imported by its link** (Phase 7 §10 Stage 6,
//! 1 October 2026): Import from NetrunnerDB… on the Decks screen takes a
//! decklist's link or uuid, `netrunner_card_sync::fetch_decklist` asks
//! the v3 API for it, and the answer goes through the same save-and-open
//! path a file does (`models::decks::Intent::Fetched`,
//! `deck_builder::from_published`). The ids are v3's on both sides, so a
//! card is matched by id and what the catalog does not know is named by
//! id. This is the download half of §9; searching NetrunnerDB's lists
//! stays owed there.
//!
//! **Bevy runs on the main thread, so the request does not.** It runs on
//! `core::TokioRuntime`, the runtime the image download already uses, and
//! answers through a one-shot channel the screen polls each frame
//! (`fetch_answers`), the way `files` answers for the dialog: a request
//! awaited on the main thread would freeze the window for as long as
//! NetrunnerDB took.
//!
//! **A headless test scripts the answer.** [`Decklists::scripted`]
//! answers every fetch with one result, at once, so the navigation test
//! drives the real pop-up and the real import with no network under CI.

use bevy::prelude::*;
use tokio::sync::oneshot;

use netrunner_card_sync::{Decklist, DecklistRef};

use crate::core::TokioRuntime;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Decklists>();
}

/// A fetch's answer: the list, or why there is none, in words for the
/// person.
pub type Answer = Result<Decklist, String>;

enum Pending {
    /// The request is out.
    Channel(oneshot::Receiver<Answer>),
    /// A scripted answer, handed over on the next poll.
    Ready(Answer),
}

/// The one fetch a screen may have out, and how it is answered.
#[derive(Resource, Default)]
pub struct Decklists {
    /// `Some(answer)`: every fetch answers with it at once, for a test.
    /// `None`: the network.
    scripted: Option<Answer>,
    pending: Option<Pending>,
}

impl Decklists {
    /// Answers every fetch with `answer` without asking anything.
    pub fn scripted(answer: Answer) -> Self {
        Decklists { scripted: Some(answer), pending: None }
    }

    pub fn is_fetching(&self) -> bool {
        self.pending.is_some()
    }

    /// Asks NetrunnerDB for the decklist `reference` names, or says why
    /// it cannot: one fetch is out already, or there is no runtime.
    pub fn fetch(&mut self, runtime: Option<&TokioRuntime>, reference: DecklistRef) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("A decklist is already being fetched".to_string());
        }
        if let Some(answer) = &self.scripted {
            self.pending = Some(Pending::Ready(answer.clone()));
            return Ok(());
        }
        let Some(runtime) = runtime else { return Err("There is no runtime to fetch on".to_string()) };
        let (tx, answer) = oneshot::channel();
        runtime.0.spawn(async move {
            let answer = netrunner_card_sync::fetch_decklist(&reference).await.map_err(|error| error.to_string());
            let _ = tx.send(answer);
        });
        self.pending = Some(Pending::Channel(answer));
        Ok(())
    }

    /// The answer, once it has come; nothing while the request is out or
    /// none was made. A request whose task died is a failure in words.
    pub fn poll(&mut self) -> Option<Answer> {
        match self.pending.take()? {
            Pending::Ready(answer) => Some(answer),
            Pending::Channel(mut answer) => match answer.try_recv() {
                Ok(answer) => Some(answer),
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.pending = Some(Pending::Channel(answer));
                    None
                }
                Err(oneshot::error::TryRecvError::Closed) => Some(Err("The fetch stopped without an answer".to_string())),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scripted_fetch_answers_at_once_and_one_at_a_time() {
        let reference = DecklistRef::parse("99ba7131-6cf1-474e-b73f-8b1aefc93d56").unwrap();
        let mut lists = Decklists::scripted(Err("no network in a test".to_string()));
        assert_eq!(lists.poll(), None, "nothing asked, nothing answered");
        lists.fetch(None, reference.clone()).unwrap();
        assert!(lists.is_fetching());
        assert!(lists.fetch(None, reference.clone()).is_err(), "one fetch at a time");
        assert_eq!(lists.poll(), Some(Err("no network in a test".to_string())));
        assert_eq!(lists.poll(), None, "an answer is given once");
        assert!(Decklists::default().fetch(None, reference).is_err(), "no runtime, no request");
    }
}
