//! The card-image download, as a resource a screen can start and watch.
//!
//! `CardImageStore::download` is async and runs on the tokio runtime;
//! its progress arrives on a channel and its report on another, and one
//! system a frame drains both. The download is not tied to the screen
//! that started it: a player who starts it from the browser and goes to
//! the profile sees the cached count there move, and comes back to a
//! grid that has filled in. Starting it is the screen's decision, so the
//! opt-in (`Settings::desktop.download_images`) is checked there.

use std::sync::Arc;

use bevy::prelude::*;
use tokio::sync::{mpsc, oneshot};

use netrunner_card_sync::{CardImageStore, DownloadProgress, DownloadReport};
use netrunner_core::card::CardId;

use crate::card_images::CardImages;
use crate::core::{Notices, TokioRuntime};

pub struct DownloadsPlugin;

impl Plugin for DownloadsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Downloads>().add_systems(Update, poll_downloads);
    }
}

/// How many fetches are in flight at once. NetrunnerDB's image host is a
/// CDN; four is polite and fills a grid in seconds.
const CONCURRENCY: usize = 4;

#[derive(Resource, Default)]
pub struct Downloads {
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Running {
        progress: mpsc::UnboundedReceiver<DownloadProgress>,
        report: oneshot::Receiver<DownloadReport>,
        last: Option<DownloadProgress>,
        total: usize,
    },
    Finished(DownloadReport),
}

impl Downloads {
    /// Starts fetching `codes`, refreshing the URL template first so a
    /// host move NetrunnerDB has announced is picked up. `false` if one
    /// is already running.
    pub fn start(&mut self, runtime: &TokioRuntime, store: Arc<CardImageStore>, codes: Vec<CardId>) -> bool {
        if self.is_running() {
            return false;
        }
        let total = codes.len();
        let (progress_tx, progress) = mpsc::unbounded_channel();
        let (report_tx, report) = oneshot::channel();
        runtime.0.spawn(async move {
            // A template that will not refresh is not a reason to stop:
            // the manifest's or the default still points at the CDN, and
            // a wrong one shows up as failures in the report.
            let _ = store.refresh_template().await;
            let report = store.download(codes, CONCURRENCY, Some(progress_tx)).await;
            let _ = report_tx.send(report);
        });
        self.state = State::Running { progress, report, last: None, total };
        true
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, State::Running { .. })
    }

    /// `(done, total)` while running.
    pub fn progress(&self) -> Option<(usize, usize)> {
        match &self.state {
            State::Running { last, total, .. } => Some((last.as_ref().map_or(0, |p| p.done), *total)),
            _ => None,
        }
    }

    /// One line for a status row: what is happening, or what happened.
    pub fn status_line(&self) -> String {
        match &self.state {
            State::Idle => String::new(),
            State::Running { last: None, total, .. } => format!("Starting: {total} images"),
            State::Running { last: Some(p), .. } => match p.failed {
                0 => format!("Downloading {} of {}", p.done, p.total),
                failed => format!("Downloading {} of {} ({failed} failed)", p.done, p.total),
            },
            State::Finished(report) => match report.failed.len() {
                0 => format!("Downloaded {} ({} were already cached)", report.fetched, report.already_cached),
                failed => format!("Downloaded {}, {failed} failed ({} were already cached)", report.fetched, report.already_cached),
            },
        }
    }
}

fn poll_downloads(mut downloads: ResMut<Downloads>, mut notices: ResMut<Notices>, mut images: ResMut<CardImages>) {
    let State::Running { progress, report, last, .. } = &mut downloads.state else { return };
    let mut moved = false;
    while let Ok(update) = progress.try_recv() {
        *last = Some(update);
        moved = true;
    }
    match report.try_recv() {
        Ok(report) => {
            notices.push(downloads.status_line_for(&report));
            downloads.state = State::Finished(report);
            images.recheck = true;
        }
        Err(oneshot::error::TryRecvError::Empty) => {
            if moved {
                images.recheck = true;
            }
        }
        Err(oneshot::error::TryRecvError::Closed) => {
            notices.push("The image download stopped without a report");
            downloads.state = State::Idle;
        }
    }
}

impl Downloads {
    fn status_line_for(&self, report: &DownloadReport) -> String {
        Downloads { state: State::Finished(report.clone()) }.status_line()
    }
}
