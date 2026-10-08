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
//! **A search is the same request shape with a list for an answer**
//! (Phase 7 §9, 8 October 2026): Search NetrunnerDB… names a card, and
//! `netrunner_card_sync::fetch_decklists` asks for the newest lists that
//! play it; a row picked from the answer goes through `Intent::Fetched`
//! as a fetched list does, since a row carries its cards.
//!
//! **A headless test scripts the answer.** [`Decklists::scripted`]
//! answers every fetch with one result, at once, and
//! [`Decklists::scripted_search`] every search, so the navigation test
//! drives the real pop-up and the real import with no network under CI.

use bevy::prelude::*;
use tokio::sync::oneshot;

use netrunner_card_sync::{Decklist, DecklistRef, Search};

use crate::core::TokioRuntime;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Decklists>();
}

/// A fetch's answer: the list, or why there is none, in words for the
/// person.
pub type Answer = Result<Decklist, String>;
/// A search's answer: the lists found, newest first, or why none.
pub type SearchAnswer = Result<Vec<Decklist>, String>;

enum Pending<A> {
    /// The request is out.
    Channel(oneshot::Receiver<A>),
    /// A scripted answer, handed over on the next poll.
    Ready(A),
}

/// The one fetch and the one search a screen may have out, and how
/// each is answered.
#[derive(Resource, Default)]
pub struct Decklists {
    /// `Some(answer)`: every fetch answers with it at once, for a test.
    /// `None`: the network.
    scripted: Option<Answer>,
    pending: Option<Pending<Answer>>,
    scripted_search: Option<SearchAnswer>,
    searching: Option<Pending<SearchAnswer>>,
}

impl Decklists {
    /// Answers every fetch with `answer` without asking anything.
    pub fn scripted(answer: Answer) -> Self {
        Decklists { scripted: Some(answer), ..Default::default() }
    }

    /// Answers every search with `answer` without asking anything.
    pub fn scripted_search(answer: SearchAnswer) -> Self {
        Decklists { scripted_search: Some(answer), ..Default::default() }
    }

    pub fn is_searching(&self) -> bool {
        self.searching.is_some()
    }

    /// Asks NetrunnerDB for the lists `search` names, or says why it
    /// cannot: one search is out already, or there is no runtime.
    pub fn search(&mut self, runtime: Option<&TokioRuntime>, search: Search) -> Result<(), String> {
        if self.searching.is_some() {
            return Err("A search is already out".to_string());
        }
        if let Some(answer) = &self.scripted_search {
            self.searching = Some(Pending::Ready(answer.clone()));
            return Ok(());
        }
        let Some(runtime) = runtime else { return Err("There is no runtime to search on".to_string()) };
        let (tx, answer) = oneshot::channel();
        runtime.0.spawn(async move {
            let answer = netrunner_card_sync::fetch_decklists(&search).await.map_err(|error| error.to_string());
            let _ = tx.send(answer);
        });
        self.searching = Some(Pending::Channel(answer));
        Ok(())
    }

    /// The search's answer, once it has come, as `poll` gives a fetch's.
    pub fn poll_search(&mut self) -> Option<SearchAnswer> {
        let (answer, still) = take(self.searching.take()?, "The search stopped without an answer");
        self.searching = still;
        answer
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
        let (answer, still) = take(self.pending.take()?, "The fetch stopped without an answer");
        self.pending = still;
        answer
    }
}

/// What a pending request has to give: its answer, or itself back while
/// the request is still out.
fn take<T>(pending: Pending<Result<T, String>>, died: &str) -> (Option<Result<T, String>>, Option<Pending<Result<T, String>>>) {
    match pending {
        Pending::Ready(answer) => (Some(answer), None),
        Pending::Channel(mut answer) => match answer.try_recv() {
            Ok(answer) => (Some(answer), None),
            Err(oneshot::error::TryRecvError::Empty) => (None, Some(Pending::Channel(answer))),
            Err(oneshot::error::TryRecvError::Closed) => (Some(Err(died.to_string())), None),
        },
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
        let mut lists = Decklists::scripted_search(Ok(Vec::new()));
        assert_eq!(lists.poll_search(), None);
        lists.search(None, Search::Card("sure_gamble".into())).unwrap();
        assert!(lists.is_searching());
        assert!(lists.search(None, Search::Card("sure_gamble".into())).is_err(), "one search at a time");
        assert_eq!(lists.poll_search(), Some(Ok(Vec::new())));
        assert_eq!(lists.poll_search(), None);
    }
}
