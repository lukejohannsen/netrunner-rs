//! What the whole client shares: the files it reads and writes, the card
//! registry, the image cache, and the runtime its network code runs on.
//!
//! **Built once, at boot, and never rebuilt.** Every path is resolved
//! the way the terminal client resolves it (`netrunner_client`), so a
//! deck saved here is the deck the terminal lists. A path that cannot be
//! resolved — no OS data directory — is a notice on the main menu, not a
//! crash: the terminal client's rule that a game which fails to start is
//! "a notice under the menu rather than a drop to the shell" holds here
//! for the whole client.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;

use netrunner_card_sync::CardImageStore;
use netrunner_client::deck_store;
use netrunner_client::ratings;
use netrunner_client::settings::{self, Settings};
use netrunner_core::cards::CardRegistry;

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Notices>();
    }
}

/// The client's shared state. `Arc`s where a background task will need a
/// copy; plain values where only systems read them.
#[derive(Resource)]
pub struct ClientCore {
    /// Every playable card, the registry every match and every screen
    /// resolves titles through.
    pub registry: Arc<CardRegistry>,
    pub settings: Settings,
    /// `None` when the OS has no data directory; edits then apply to this
    /// session only, and the settings screen says so.
    pub settings_path: Option<PathBuf>,
    pub decks_dir: Option<PathBuf>,
    pub ratings_path: Option<PathBuf>,
    pub images: Arc<CardImageStore>,
}

impl ClientCore {
    /// Resolves every path and loads the settings, collecting rather
    /// than returning the failures so the client still opens.
    pub fn load() -> (Self, Vec<String>) {
        let mut notices = Vec::new();
        let mut note = |result: Result<PathBuf, String>| match result {
            Ok(path) => Some(path),
            Err(error) => {
                notices.push(error);
                None
            }
        };
        let settings_path = note(settings::resolve_settings_file());
        let decks_dir = note(deck_store::resolve_decks_dir(None));
        let ratings_path = note(ratings::resolve_ratings_file(None));
        let settings = match settings_path.as_deref().map(Settings::load) {
            Some(Ok(settings)) => settings,
            Some(Err(error)) => {
                notices.push(format!("settings ignored: {error}"));
                Settings::default()
            }
            None => Settings::default(),
        };
        let images = match CardImageStore::new() {
            Ok(store) => store,
            Err(error) => {
                notices.push(format!("card images unavailable: {error}"));
                CardImageStore::with_dir(std::env::temp_dir().join("netrunner").join("images"))
            }
        };
        let core = Self {
            registry: Arc::new(netrunner_client::decks::sample_deck_registry()),
            settings,
            settings_path,
            decks_dir,
            ratings_path,
            images: Arc::new(images),
        };
        (core, notices)
    }

    /// A client with every file under `dir` — what a test builds so it
    /// never reads or writes the developer's own data directory.
    pub fn in_dir(dir: PathBuf) -> Self {
        Self {
            registry: Arc::new(netrunner_client::decks::sample_deck_registry()),
            settings: Settings::default(),
            settings_path: Some(dir.join("settings.json")),
            decks_dir: Some(dir.join("decks")),
            ratings_path: Some(dir.join("ratings.json")),
            images: Arc::new(CardImageStore::with_dir(dir.join("images"))),
        }
    }

    /// The name games are rated under: the settings' name, else the
    /// login name — the terminal client's rule, through the same function.
    pub fn player_name(&self) -> String {
        ratings::player_name(self.settings.player.as_deref())
    }

    /// Writes the settings, or says why it could not. Called after every
    /// edit, as the terminal client saves after every change: a
    /// preference is not something to lose to a crash.
    pub fn save_settings(&self) -> Result<(), String> {
        match &self.settings_path {
            Some(path) => self.settings.save(path),
            None => Err("no OS data directory is available, so settings apply to this session only".to_string()),
        }
    }
}

/// Things the player should be told that are not errors of any one
/// screen — a path that would not resolve, a file that would not save.
/// The main menu shows the newest.
#[derive(Resource, Default, Debug)]
pub struct Notices(pub Vec<String>);

impl Notices {
    pub fn push(&mut self, notice: impl Into<String>) {
        self.0.push(notice.into());
    }

    pub fn latest(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }
}

/// The tokio runtime the network code and the image downloader run on,
/// in its own threads. Bevy's task pools cannot run tokio's I/O; one
/// runtime in a resource is where the two meet, and a system only ever
/// `try_recv`s from it.
#[derive(Resource)]
pub struct TokioRuntime(pub tokio::runtime::Runtime);

impl TokioRuntime {
    pub fn new() -> Result<Self, String> {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("netrunner-io")
            .build()
            .map(Self)
            .map_err(|error| format!("could not start the network runtime: {error}"))
    }

    pub fn handle(&self) -> tokio::runtime::Handle {
        self.0.handle().clone()
    }
}
