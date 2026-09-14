//! Card images, fetched from NetrunnerDB on request and kept on disk.
//!
//! **A client shows a card's printed face when it has one and its text
//! when it does not**, and the difference is only whether this store has
//! the file. Nothing here is required for play: the embedded catalog and
//! the card text are the game, and an image is a nicety the player opts
//! into (`DesktopPrefs::download_images`), because it is a network call
//! made on their behalf.
//!
//! **Never committed, never under the repo.** NetrunnerDB serves Null
//! Signal Games' art under its own terms, not this project's GPL, so the
//! files live in the player's cache directory and only ever get there
//! through this store. That is also why `CardDefinition::image_url` stays
//! `None`: the join is by NetrunnerDB code, and the store, not the card,
//! knows where the picture is.
//!
//! **The URL template is read off the API, with a fallback.** NetrunnerDB's
//! cards envelope carries `imageUrlTemplate`; the value seen when this was
//! written is [`DEFAULT_IMAGE_URL_TEMPLATE`]. The template last seen is
//! kept in `manifest.json` beside the images, so a client that has never
//! synced still knows where to look and a host move is picked up by the
//! next refresh rather than by a release.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Semaphore;

use netrunner_core::card::CardId;

use crate::cache_path::resolve_images_dir;
use crate::error::SyncError;
use crate::sync::{http_client, temp_file_path, NetrunnerDbEnvelope, NETRUNNERDB_CARDS_URL};

/// Where NetrunnerDB served card images when this crate was written.
/// `{code}` is the five-digit printing code (`30001`).
pub const DEFAULT_IMAGE_URL_TEMPLATE: &str = "https://card-images.netrunnerdb.com/v2/large/{code}.jpg";

const MANIFEST_FILE: &str = "manifest.json";

/// Whether a card's image is on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageStatus {
    /// Never fetched, or fetched and later deleted.
    Missing,
    Cached(PathBuf),
    /// Fetched this run and refused (a 404 for a card NetrunnerDB has no
    /// scan of, say). Remembered in memory only, so a later run tries
    /// again — a fix on their side should not need one on ours.
    Failed(String),
}

/// One line of a download's progress, sent after every card settles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadProgress {
    pub done: usize,
    pub total: usize,
    pub failed: usize,
    pub last: CardId,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DownloadReport {
    pub fetched: usize,
    pub already_cached: usize,
    /// Each failure with the reason, so a screen can show which cards
    /// have no picture and why.
    pub failed: Vec<(CardId, String)>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    image_url_template: String,
}

/// The on-disk image cache. Cheap to clone-by-`Arc` into a download task;
/// every method takes `&self`.
#[derive(Debug)]
pub struct CardImageStore {
    dir: PathBuf,
    template: Mutex<String>,
    http: reqwest::Client,
    failed: Mutex<HashMap<CardId, String>>,
}

impl CardImageStore {
    /// The OS cache directory's `images/`, reading the template last seen
    /// if a manifest is there. Creates nothing.
    pub fn new() -> Result<Self, SyncError> {
        Ok(Self::with_dir(resolve_images_dir()?))
    }

    /// An explicit directory — the seam tests use, as
    /// `NetrunnerDbSync::with_cache_file` is.
    pub fn with_dir(dir: PathBuf) -> Self {
        let template = read_manifest(&dir).unwrap_or_else(|| DEFAULT_IMAGE_URL_TEMPLATE.to_string());
        Self { dir, template: Mutex::new(template), http: http_client(), failed: Mutex::new(HashMap::new()) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The template in use: the manifest's, else the default.
    pub fn template(&self) -> String {
        self.template.lock().expect("template lock").clone()
    }

    /// `<dir>/<code>.jpg`, whether or not it exists.
    pub fn path_for(&self, code: CardId) -> PathBuf {
        self.dir.join(format!("{}.jpg", code.0))
    }

    /// The URL the template gives for `code`.
    pub fn url_for(&self, code: CardId) -> String {
        self.template().replace("{code}", &code.0.to_string())
    }

    pub fn status(&self, code: CardId) -> ImageStatus {
        let path = self.path_for(code);
        if path.is_file() {
            return ImageStatus::Cached(path);
        }
        match self.failed.lock().expect("failed lock").get(&code) {
            Some(reason) => ImageStatus::Failed(reason.clone()),
            None => ImageStatus::Missing,
        }
    }

    /// How many of `codes` are on disk — the "cached / total" a settings
    /// screen shows.
    pub fn cached_count(&self, codes: &[CardId]) -> usize {
        codes.iter().filter(|code| self.path_for(**code).is_file()).count()
    }

    /// Asks NetrunnerDB for its current image URL template and keeps it.
    /// One cards request, the same one a catalog sync makes; the card data
    /// in the response is not used here.
    pub async fn refresh_template(&self) -> Result<String, SyncError> {
        let envelope: NetrunnerDbEnvelope<serde_json::Value> = self.http.get(NETRUNNERDB_CARDS_URL).send().await?.json().await?;
        let Some(template) = envelope.image_url_template.filter(|t| t.contains("{code}")) else {
            return Ok(self.template());
        };
        self.set_template(template.clone()).await?;
        Ok(template)
    }

    async fn set_template(&self, template: String) -> Result<(), SyncError> {
        *self.template.lock().expect("template lock") = template.clone();
        let manifest = serde_json::to_vec_pretty(&Manifest { image_url_template: template })?;
        write_atomically(&self.dir.join(MANIFEST_FILE), &manifest).await
    }

    /// Fetches every image in `codes` that is not already on disk, at
    /// most `concurrency` at a time, reporting after each. A failure is
    /// recorded and the rest continue: one missing scan should not stop
    /// the other two hundred.
    pub async fn download(
        &self,
        codes: Vec<CardId>,
        concurrency: usize,
        progress: Option<UnboundedSender<DownloadProgress>>,
    ) -> DownloadReport {
        let total = codes.len();
        let limit = Arc::new(Semaphore::new(concurrency.max(1)));
        let mut report = DownloadReport::default();
        let mut tasks = tokio::task::JoinSet::new();
        for code in codes {
            if self.path_for(code).is_file() {
                report.already_cached += 1;
                continue;
            }
            let permit = limit.clone();
            let url = self.url_for(code);
            let path = self.path_for(code);
            let http = self.http.clone();
            tasks.spawn(async move {
                let _permit = permit.acquire_owned().await.expect("the semaphore outlives the task");
                (code, fetch_one(&http, &url, &path, code).await)
            });
        }
        let mut done = report.already_cached;
        while let Some(joined) = tasks.join_next().await {
            let Ok((code, result)) = joined else { continue };
            done += 1;
            match result {
                Ok(()) => report.fetched += 1,
                Err(error) => {
                    let reason = error.to_string();
                    self.failed.lock().expect("failed lock").insert(code, reason.clone());
                    report.failed.push((code, reason));
                }
            }
            if let Some(progress) = &progress {
                let _ = progress.send(DownloadProgress { done, total, failed: report.failed.len(), last: code });
            }
        }
        report.failed.sort_by_key(|(code, _)| *code);
        report
    }
}

async fn fetch_one(http: &reqwest::Client, url: &str, path: &Path, code: CardId) -> Result<(), SyncError> {
    let response = http.get(url).send().await?;
    if !response.status().is_success() {
        return Err(SyncError::ImageDownload { code: code.0, status: response.status().as_u16() });
    }
    let bytes = response.bytes().await?;
    write_atomically(path, &bytes).await
}

/// Temp file beside the target and a rename, as the catalog cache does,
/// so a download cut off halfway never leaves a truncated image that
/// would then be trusted as cached.
async fn write_atomically(target: &Path, contents: &[u8]) -> Result<(), SyncError> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|source| SyncError::CreateCacheDir { path: parent.to_path_buf(), source })?;
    }
    let temp = temp_file_path(target);
    tokio::fs::write(&temp, contents).await.map_err(|source| SyncError::ImageWrite { path: temp.clone(), source })?;
    tokio::fs::rename(&temp, target).await.map_err(|source| SyncError::AtomicRename { path: target.to_path_buf(), source })
}

fn read_manifest(dir: &Path) -> Option<String> {
    let bytes = std::fs::read(dir.join(MANIFEST_FILE)).ok()?;
    let manifest: Manifest = serde_json::from_slice(&bytes).ok()?;
    manifest.image_url_template.contains("{code}").then_some(manifest.image_url_template)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!("netrunner_images_{tag}_{}_{n}", std::process::id()))
    }

    #[test]
    fn a_store_with_no_manifest_uses_the_default_template() {
        let store = CardImageStore::with_dir(temp_dir("default"));
        assert_eq!(store.template(), DEFAULT_IMAGE_URL_TEMPLATE);
        assert_eq!(store.url_for(CardId(30001)), "https://card-images.netrunnerdb.com/v2/large/30001.jpg");
        assert!(store.path_for(CardId(30001)).ends_with("30001.jpg"));
        assert_eq!(store.status(CardId(30001)), ImageStatus::Missing);
    }

    #[tokio::test]
    async fn the_template_persists_in_the_manifest_and_a_bad_one_is_ignored() {
        let dir = temp_dir("manifest");
        let store = CardImageStore::with_dir(dir.clone());
        store.set_template("https://example.test/{code}.png".to_string()).await.unwrap();
        assert_eq!(CardImageStore::with_dir(dir.clone()).url_for(CardId(7)), "https://example.test/7.png");
        std::fs::write(dir.join(MANIFEST_FILE), r#"{"image_url_template":"no placeholder"}"#).unwrap();
        assert_eq!(CardImageStore::with_dir(dir.clone()).template(), DEFAULT_IMAGE_URL_TEMPLATE);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_on_disk_is_cached_and_counts() {
        let dir = temp_dir("cached");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("30001.jpg"), b"not really a jpeg").unwrap();
        let store = CardImageStore::with_dir(dir.clone());
        assert_eq!(store.status(CardId(30001)), ImageStatus::Cached(dir.join("30001.jpg")));
        assert_eq!(store.cached_count(&[CardId(30001), CardId(30002)]), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The envelope parses with and without the template key, since the
    /// packs response and every fixture lack it.
    #[test]
    fn the_envelope_carries_the_template_when_present() {
        let with: NetrunnerDbEnvelope<serde_json::Value> =
            serde_json::from_str(r#"{"success":true,"data":[],"imageUrlTemplate":"https://x/{code}.jpg"}"#).unwrap();
        assert_eq!(with.image_url_template.as_deref(), Some("https://x/{code}.jpg"));
        let without: NetrunnerDbEnvelope<serde_json::Value> = serde_json::from_str(r#"{"success":true,"data":[]}"#).unwrap();
        assert_eq!(without.image_url_template, None);
    }

    /// An already-cached card is skipped without a request, so a download
    /// over an all-cached set touches the network for nothing.
    #[tokio::test]
    async fn already_cached_cards_are_skipped() {
        let dir = temp_dir("skip");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("30001.jpg"), b"jpeg").unwrap();
        let store = CardImageStore::with_dir(dir.clone());
        let report = store.download(vec![CardId(30001)], 4, None).await;
        assert_eq!(report, DownloadReport { fetched: 0, already_cached: 1, failed: vec![] });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
