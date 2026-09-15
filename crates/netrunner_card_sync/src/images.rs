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
//!
//! **The icon font is cached the same way.** NetrunnerDB draws the
//! factions, the sets and the printed symbols (`[credit]`, `[click]`,
//! `[subroutine]`…) with one small TrueType font its site serves at
//! [`ICON_FONT_URL`]. Its repository is MIT-licensed, but the marks in
//! it are Null Signal Games' and its predecessor's, so the file is
//! treated as the card scans are: fetched into the cache on the
//! player's say-so, never shipped. A client without it draws the
//! symbols from its own fonts and names the factions in words.
//!
//! **So are the official card backs.** Null Signal Games does not
//! publish its backs — its print-and-play files omit them and its
//! visual-assets page keeps them out of the public pack — and
//! jinteki.net, the community's online client, draws them by NSG's
//! arrangement from [`CARD_BACK_CORP_URL`] and [`CARD_BACK_RUNNER_URL`].
//! Those are fetched into the cache on the same opt-in, for the player's
//! own screen, exactly as the scans are, and a client without them draws
//! its own back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Semaphore;

use netrunner_core::card::CardId;
use netrunner_core::rules::Side;

use crate::cache_path::resolve_images_dir;
use crate::error::SyncError;
use crate::sync::{http_client, temp_file_path, NetrunnerDbEnvelope, NETRUNNERDB_CARDS_URL};

/// Where NetrunnerDB served card images when this crate was written.
/// `{code}` is the five-digit printing code (`30001`).
pub const DEFAULT_IMAGE_URL_TEMPLATE: &str = "https://card-images.netrunnerdb.com/v2/large/{code}.jpg";

const MANIFEST_FILE: &str = "manifest.json";

/// Where NetrunnerDB serves the icon font its pages use (the `netrunner`
/// face in its `netrunnerfont.css`). Code points e900–e935: the eight
/// printed symbols, the factions and the sets — see
/// `netrunner_client::card_text` for which is which.
pub const ICON_FONT_URL: &str = "https://netrunnerdb.com/fonts/netrunner.ttf";

/// The icon font's name in the cache, beside the images.
const ICON_FONT_FILE: &str = "netrunnerdb-icons.ttf";

/// Where jinteki.net serves the official Corp card back (255 × 356,
/// a 16-bit PNG). The same files sit in its repository under
/// `resources/public/img/`, but the site is what it deploys.
pub const CARD_BACK_CORP_URL: &str = "https://jinteki.net/img/nsg-corp.png";
/// Where jinteki.net serves the official Runner card back.
pub const CARD_BACK_RUNNER_URL: &str = "https://jinteki.net/img/nsg-runner.png";

/// A back's name in the cache, beside the images: the same name the
/// desktop's drop-in tier uses under `assets/cards/`, so one word means
/// one file wherever it sits.
fn card_back_file(side: Side) -> &'static str {
    match side {
        Side::Corp => "back-corp.png",
        Side::Runner => "back-runner.png",
    }
}

pub fn card_back_url(side: Side) -> &'static str {
    match side {
        Side::Corp => CARD_BACK_CORP_URL,
        Side::Runner => CARD_BACK_RUNNER_URL,
    }
}

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
    ///
    /// The code is written as NetrunnerDB prints it: five digits,
    /// zero-padded. `CardId` is a `u32`, so the Core Set's `"01001"` is
    /// `1001` in memory, and the first cut of this store asked the CDN
    /// for `1001.jpg` — a different card's picture, or nothing. System
    /// Gateway and later sets have five significant digits, so the
    /// padding changes nothing for them.
    pub fn path_for(&self, code: CardId) -> PathBuf {
        self.dir.join(format!("{}.jpg", padded(code)))
    }

    /// The URL the template gives for `code`, padded as `path_for` pads.
    pub fn url_for(&self, code: CardId) -> String {
        self.template().replace("{code}", &padded(code))
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

    /// `<dir>/netrunnerdb-icons.ttf`, whether or not it exists.
    pub fn icon_font_path(&self) -> PathBuf {
        self.dir.join(ICON_FONT_FILE)
    }

    /// The icon font, if it has been fetched.
    pub fn icon_font(&self) -> Option<PathBuf> {
        let path = self.icon_font_path();
        path.is_file().then_some(path)
    }

    /// Fetches the icon font unless it is already on disk. Forty
    /// kilobytes, once; a CDN error page in its place would be loaded
    /// as a font and fail silently, which is why the bytes are checked
    /// for a TrueType header before they are kept.
    pub async fn download_icon_font(&self) -> Result<PathBuf, SyncError> {
        let path = self.icon_font_path();
        if path.is_file() {
            return Ok(path);
        }
        let response = self.http.get(ICON_FONT_URL).send().await?;
        if !response.status().is_success() {
            return Err(SyncError::IconFontDownload { status: response.status().as_u16() });
        }
        let bytes = response.bytes().await?;
        if !is_truetype(&bytes) {
            return Err(SyncError::IconFontInvalid);
        }
        write_atomically(&path, &bytes).await?;
        Ok(path)
    }

    /// `<dir>/back-corp.png` or `<dir>/back-runner.png`, whether or not
    /// it exists.
    pub fn card_back_path(&self, side: Side) -> PathBuf {
        self.dir.join(card_back_file(side))
    }

    /// The official back for `side`, if it has been fetched.
    pub fn card_back(&self, side: Side) -> Option<PathBuf> {
        let path = self.card_back_path(side);
        path.is_file().then_some(path)
    }

    /// Fetches the official back for `side` unless it is already on
    /// disk. Under half a megabyte, once a side; the bytes are checked
    /// for a PNG signature before they are kept, for the reason the
    /// icon font's are — an error page in a `.png` would decode as
    /// nothing and the drawn back would stay, with no word why.
    pub async fn download_card_back(&self, side: Side) -> Result<PathBuf, SyncError> {
        let path = self.card_back_path(side);
        if path.is_file() {
            return Ok(path);
        }
        let response = self.http.get(card_back_url(side)).send().await?;
        if !response.status().is_success() {
            return Err(SyncError::CardBackDownload { side, status: response.status().as_u16() });
        }
        let bytes = response.bytes().await?;
        if !is_png(&bytes) {
            return Err(SyncError::CardBackInvalid { side });
        }
        write_atomically(&path, &bytes).await?;
        Ok(path)
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

/// NetrunnerDB's five-digit spelling of a code.
fn padded(code: CardId) -> String {
    format!("{:05}", code.0)
}

/// Whether `bytes` start as a TrueType or OpenType file does: the
/// version tag `00 01 00 00`, `true`, or `OTTO` for CFF outlines.
fn is_truetype(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some([0, 1, 0, 0] | b"true" | b"OTTO"))
}

/// The eight-byte PNG signature.
fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
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
        assert_eq!(CardImageStore::with_dir(dir.clone()).url_for(CardId(7)), "https://example.test/00007.png");
        std::fs::write(dir.join(MANIFEST_FILE), r#"{"image_url_template":"no placeholder"}"#).unwrap();
        assert_eq!(CardImageStore::with_dir(dir.clone()).template(), DEFAULT_IMAGE_URL_TEMPLATE);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_icon_font_lives_beside_the_images_and_is_absent_until_fetched() {
        let dir = temp_dir("font");
        let store = CardImageStore::with_dir(dir.clone());
        assert_eq!(store.icon_font_path(), dir.join("netrunnerdb-icons.ttf"));
        assert_eq!(store.icon_font(), None);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(store.icon_font_path(), [0, 1, 0, 0, 0, 11]).unwrap();
        assert_eq!(store.icon_font(), Some(dir.join("netrunnerdb-icons.ttf")));
        assert!(is_truetype(&[0, 1, 0, 0, 0, 11]) && is_truetype(b"OTTO....") && is_truetype(b"true...."));
        assert!(!is_truetype(b"<!DOCTYPE html>") && !is_truetype(b"\0\x01"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_card_backs_live_beside_the_images_and_are_absent_until_fetched() {
        let dir = temp_dir("backs");
        let store = CardImageStore::with_dir(dir.clone());
        assert_eq!(store.card_back_path(Side::Corp), dir.join("back-corp.png"));
        assert_eq!(store.card_back_path(Side::Runner), dir.join("back-runner.png"));
        assert_eq!(store.card_back(Side::Corp), None);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(store.card_back_path(Side::Corp), b"\x89PNG\r\n\x1a\n....").unwrap();
        assert_eq!(store.card_back(Side::Corp), Some(dir.join("back-corp.png")));
        assert_eq!(store.card_back(Side::Runner), None);
        assert!(card_back_url(Side::Corp).ends_with("nsg-corp.png") && card_back_url(Side::Runner).ends_with("nsg-runner.png"));
        assert!(is_png(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"));
        assert!(!is_png(b"<!DOCTYPE html>") && !is_png(b"\x89PN"));
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

    /// A Core Set code keeps its leading zero on disk and in the URL:
    /// `CardId(1001)` is NetrunnerDB's `01001`.
    #[test]
    fn core_set_codes_are_five_digits_on_disk_and_in_the_url() {
        let store = CardImageStore::with_dir(temp_dir("pad"));
        assert_eq!(store.path_for(CardId(1001)).file_name().unwrap(), "01001.jpg");
        assert_eq!(store.url_for(CardId(1001)), "https://card-images.netrunnerdb.com/v2/large/01001.jpg");
        assert_eq!(store.path_for(CardId(30001)).file_name().unwrap(), "30001.jpg");
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
