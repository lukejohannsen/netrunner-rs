//! The daemon's store, and the one writer that keeps it (Phase 4 §10).
//!
//! **Every write is queued, and one task carries them out in order.** The
//! registry is a `std` mutex that nothing awaits under, and a store write
//! is an await; so a change is decided under the lock — the book folded,
//! the tournament changed — and the rows that record it are handed to
//! the writer, which lands them in the order they were handed over. Two
//! games ending at once used to be two whole-file rewrites racing on one
//! path; now they are two row writes in the order the book took them, so
//! the store never holds an older standing over a newer one. A write
//! that fails is logged with what it was, as a failed file write was;
//! the book in memory is already right, and `rebuild` puts the store
//! right from the log.
//!
//! **The book stays in memory.** It is the rating system's own type and
//! the fold has to be serialized under the lock anyway; the store is its
//! index — loaded into it at bind, written row by row after each game,
//! and read directly for what the book never answered (a ladder page, a
//! profile).

use std::future::Future;
use std::pin::Pin;

use tokio::sync::mpsc;

use netrunner_core::format::NsgFormat;
use netrunner_rating::{Format, RatingBook, RoleRecord, Standing, Track};
use netrunner_store::{RatingRow, Store};

type Job = Pin<Box<dyn Future<Output = netrunner_store::Result<()>> + Send>>;

/// The store and the queue in front of it.
#[derive(Clone)]
pub(crate) struct Writer {
    store: Store,
    tx: mpsc::UnboundedSender<(&'static str, Job)>,
}

impl Writer {
    /// Starts the writer task on the current runtime.
    pub(crate) fn start(store: Store) -> Writer {
        let (tx, mut rx) = mpsc::unbounded_channel::<(&'static str, Job)>();
        tokio::spawn(async move {
            while let Some((what, job)) = rx.recv().await {
                if let Err(error) = job.await {
                    tracing::warn!(what, %error, "a store write failed");
                }
            }
        });
        Writer { store, tx }
    }

    /// Queues `job`, which is run with a handle on the store after every
    /// write queued before it.
    pub(crate) fn write<F, Fut>(&self, what: &'static str, job: F)
    where
        F: FnOnce(Store) -> Fut + Send + 'static,
        Fut: Future<Output = netrunner_store::Result<()>> + Send + 'static,
    {
        let store = self.store.clone();
        let _ = self.tx.send((what, Box::pin(job(store))));
    }
}

/// The ladder a game in `format` counts toward.
pub(crate) fn ladder(format: NsgFormat) -> Track {
    Track::Ladder(match format {
        NsgFormat::Startup => Format::Startup,
        NsgFormat::Standard => Format::Standard,
        NsgFormat::Eternal => Format::Eternal,
        NsgFormat::Snapshot => Format::Snapshot,
        NsgFormat::Unrestricted => Format::Unrestricted,
    })
}

/// A standing as the store's rows: one per role.
pub(crate) fn rows_of(track: Track, key: &str, standing: &Standing) -> Vec<RatingRow> {
    let Track::Ladder(format) = track else { return Vec::new() };
    let row = |role: &str, record: &RoleRecord| RatingRow {
        key: key.to_string(),
        format: format.name().to_string(),
        role: role.to_string(),
        rating: record.rating.rating,
        deviation: record.rating.deviation,
        volatility: record.rating.volatility,
        wins: i64::from(record.wins),
        draws: i64::from(record.draws),
        losses: i64::from(record.losses),
    };
    vec![row("corp", &standing.corp), row("runner", &standing.runner)]
}

/// The book the store's rows make. A row in a format or a role this
/// build does not know is skipped, as a retired track in a book file
/// was.
pub(crate) fn book_from(rows: &[RatingRow]) -> RatingBook {
    let mut book = RatingBook::default();
    let mut standings: std::collections::BTreeMap<(Track, String), Standing> = std::collections::BTreeMap::new();
    for row in rows {
        let Some(format) = Format::from_name(&row.format) else { continue };
        let standing = standings.entry((Track::Ladder(format), row.key.clone())).or_default();
        let record = match row.role.as_str() {
            "corp" => &mut standing.corp,
            "runner" => &mut standing.runner,
            _ => continue,
        };
        record.rating = netrunner_rating::Rating { rating: row.rating, deviation: row.deviation, volatility: row.volatility };
        record.wins = row.wins.try_into().unwrap_or(0);
        record.draws = row.draws.try_into().unwrap_or(0);
        record.losses = row.losses.try_into().unwrap_or(0);
    }
    for ((track, key), standing) in standings {
        book.insert(track, &key, standing);
    }
    book
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_rating::Outcome;

    #[test]
    fn a_book_goes_to_rows_and_back_whole() {
        let mut book = RatingBook::default();
        book.record(ladder(NsgFormat::Standard), "key:ann", "key:bo", Outcome::CorpWin);
        book.record(ladder(NsgFormat::Startup), "key:bo", "key:ann", Outcome::RunnerWin);
        let rows: Vec<RatingRow> = book.entries().flat_map(|(track, key, standing)| rows_of(track, key, standing)).collect();
        assert_eq!(rows.len(), 8, "two keys, two formats, two roles");
        assert_eq!(book_from(&rows), book);
    }
}
