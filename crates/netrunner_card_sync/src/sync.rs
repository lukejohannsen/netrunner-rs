use std::path::{Path, PathBuf};

use serde::Deserialize;

use netrunner_core::card::{NetrunnerDbCardDto, NetrunnerDbPackDto, PackInfo};
use netrunner_core::catalog::{convert_dtos_lenient, CardCatalog};

use crate::cache_path::resolve_cache_file;
use crate::error::SyncError;

const NETRUNNERDB_CARDS_URL: &str = "https://netrunnerdb.com/api/2.0/public/cards";
const NETRUNNERDB_PACKS_URL: &str = "https://netrunnerdb.com/api/2.0/public/packs";

/// NetrunnerDB wraps every public API list response in a
/// `{"success": bool, "data": [...]}` envelope rather than returning a bare
/// array. The embedded fixture files store the unwrapped `data` array
/// directly (see `netrunner_core::catalog`), so this envelope is only
/// needed when talking to the live API.
#[derive(Debug, Deserialize)]
struct NetrunnerDbEnvelope<T> {
    data: Vec<T>,
}

/// Which sets a sync operation should apply to. `Sets` holds NetrunnerDB
/// pack codes, e.g. `["sg", "elev"]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncScope {
    All,
    Sets(Vec<String>),
}

fn filter_by_scope(dtos: Vec<NetrunnerDbCardDto>, scope: &SyncScope) -> Vec<NetrunnerDbCardDto> {
    match scope {
        SyncScope::All => dtos,
        SyncScope::Sets(codes) => dtos.into_iter().filter(|dto| codes.iter().any(|c| c == &dto.pack_code)).collect(),
    }
}

/// Fetches and caches NetrunnerDB card data. All HTTP and filesystem I/O for
/// the card catalog lives here, never in `netrunner_core` (which stays pure
/// per AGENTS.md's I/O-free rule for that crate).
pub struct NetrunnerDbSync {
    http_client: reqwest::Client,
    cache_file: PathBuf,
}

impl NetrunnerDbSync {
    /// Resolves the OS cache path via `resolve_cache_file()` and builds a
    /// default `reqwest::Client`.
    pub fn new() -> Result<Self, SyncError> {
        Ok(Self { http_client: reqwest::Client::new(), cache_file: resolve_cache_file()? })
    }

    /// Same as `new()` but with an explicit cache file path — the seam
    /// tests use to point at a temp directory instead of the real OS cache
    /// dir.
    pub fn with_cache_file(cache_file: PathBuf) -> Self {
        Self { http_client: reqwest::Client::new(), cache_file }
    }

    /// Builds the in-memory catalog WITHOUT any network call: embedded
    /// defaults as the base layer, with the disk cache (if present) merged
    /// on top. A missing cache file is not an error.
    pub fn load_catalog(&self) -> Result<CardCatalog, SyncError> {
        let mut catalog = CardCatalog::load_default_core_sets()?;
        catalog.merge(self.read_cached_catalog()?.into_definitions());
        Ok(catalog)
    }

    /// Fetches NetrunnerDB's full card list, filters to `scope`, merges the
    /// filtered cards into the *persisted* cache (so syncing one set does
    /// not evict previously-cached other sets), writes the updated cache
    /// atomically, then returns `load_catalog()`'s result.
    pub async fn sync_from_netrunnerdb(&self, scope: SyncScope) -> Result<CardCatalog, SyncError> {
        let bytes = self.http_client.get(NETRUNNERDB_CARDS_URL).send().await?.bytes().await?;
        self.ingest_fetched_payload(&bytes, &scope).await
    }

    /// Fetches `/api/2.0/public/packs` and returns the available sets —
    /// always a live call, no caching.
    pub async fn list_available_sets(&self) -> Result<Vec<PackInfo>, SyncError> {
        let envelope: NetrunnerDbEnvelope<NetrunnerDbPackDto> =
            self.http_client.get(NETRUNNERDB_PACKS_URL).send().await?.json().await?;
        Ok(envelope.data.into_iter().map(PackInfo::from).collect())
    }

    /// Everything after the HTTP fetch: parse the raw JSON body, filter by
    /// scope, merge onto the persisted cache, write it back, and recompose
    /// the full in-memory catalog. Exercised directly by tests with a
    /// fixture byte slice — no network access needed to test this half of
    /// the pipeline.
    async fn ingest_fetched_payload(&self, body: &[u8], scope: &SyncScope) -> Result<CardCatalog, SyncError> {
        let envelope: NetrunnerDbEnvelope<NetrunnerDbCardDto> = serde_json::from_slice(body)?;
        let scoped = filter_by_scope(envelope.data, scope);
        // Best-effort: NetrunnerDB's live card list naturally exceeds this
        // catalog's currently-modeled scope (e.g. mini-factions like Apex,
        // Adam, Sunny-Lebeau aren't in the closed `Faction` enum). Skip and
        // report those rather than aborting the whole sync over them.
        let (new_defs, skipped) = convert_dtos_lenient(scoped);
        if !skipped.is_empty() {
            eprintln!("warning: skipped {} card(s) this catalog doesn't model:", skipped.len());
            for (index, error) in &skipped {
                eprintln!("  [{index}] {error}");
            }
        }

        let mut persisted = self.read_cached_catalog()?;
        persisted.merge(new_defs);
        self.write_cache_atomically(&serde_json::to_vec(&persisted)?).await?;

        self.load_catalog()
    }

    fn read_cached_catalog(&self) -> Result<CardCatalog, SyncError> {
        match std::fs::read(&self.cache_file) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(CardCatalog::new()),
            Err(source) => Err(SyncError::ReadCacheFile { path: self.cache_file.clone(), source }),
        }
    }

    /// Writes `contents` to a temp file in the same directory as the cache
    /// file (so the final rename is same-filesystem and therefore atomic),
    /// then renames it into place. Creates the cache directory first if
    /// absent.
    async fn write_cache_atomically(&self, contents: &[u8]) -> Result<(), SyncError> {
        let target = &self.cache_file;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| SyncError::CreateCacheDir { path: parent.to_path_buf(), source })?;
        }

        let temp_path = temp_file_path(target);
        tokio::fs::write(&temp_path, contents)
            .await
            .map_err(|source| SyncError::WriteCacheFile { path: temp_path.clone(), source })?;
        tokio::fs::rename(&temp_path, target)
            .await
            .map_err(|source| SyncError::AtomicRename { path: target.clone(), source })
    }
}

fn temp_file_path(target: &Path) -> PathBuf {
    let file_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("cards.json");
    let temp_name = format!("{file_name}.tmp.{}", std::process::id());
    target.with_file_name(temp_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::card::CardType;

    fn sample_payload() -> Vec<u8> {
        br#"{
            "success": true,
            "data": [
                {
                    "code": "30001",
                    "title": "Card From SG",
                    "type_code": "event",
                    "side_code": "runner",
                    "faction_code": "anarch",
                    "pack_code": "sg",
                    "cost": 1
                },
                {
                    "code": "40001",
                    "title": "Card From Elevation",
                    "type_code": "event",
                    "side_code": "runner",
                    "faction_code": "anarch",
                    "pack_code": "elev",
                    "cost": 2
                }
            ]
        }"#
        .to_vec()
    }

    fn sync_with_temp_cache() -> (NetrunnerDbSync, tempfile_dir::TempDir) {
        let dir = tempfile_dir::TempDir::new();
        let cache_file = dir.path().join("cards.json");
        (NetrunnerDbSync::with_cache_file(cache_file), dir)
    }

    /// Minimal std-only stand-in for a `tempfile` crate temp directory:
    /// creates a uniquely-named directory under `std::env::temp_dir()` and
    /// removes it on drop.
    mod tempfile_dir {
        use std::path::{Path, PathBuf};

        pub struct TempDir(PathBuf);

        impl TempDir {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "netrunner_card_sync_test_{}_{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ));
                std::fs::create_dir_all(&path).expect("create temp dir");
                Self(path)
            }

            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[tokio::test]
    async fn ingest_all_scope_merges_every_card() {
        let (sync, _dir) = sync_with_temp_cache();
        let catalog = sync.ingest_fetched_payload(&sample_payload(), &SyncScope::All).await.expect("ingest succeeds");

        assert!(catalog.filter_by_type(CardType::Event).count() >= 2);
        assert!(catalog.get_by_title("Card From SG").is_some());
        assert!(catalog.get_by_title("Card From Elevation").is_some());
    }

    #[tokio::test]
    async fn ingest_scoped_to_one_set_excludes_the_other() {
        let (sync, _dir) = sync_with_temp_cache();
        let scope = SyncScope::Sets(vec!["sg".to_string()]);
        let catalog = sync.ingest_fetched_payload(&sample_payload(), &scope).await.expect("ingest succeeds");

        assert!(catalog.get_by_title("Card From SG").is_some());
        assert!(catalog.get_by_title("Card From Elevation").is_none());
    }

    #[tokio::test]
    async fn syncing_one_set_does_not_evict_a_previously_cached_other_set() {
        let (sync, _dir) = sync_with_temp_cache();

        sync.ingest_fetched_payload(&sample_payload(), &SyncScope::Sets(vec!["sg".to_string()]))
            .await
            .expect("first ingest succeeds");

        let catalog = sync
            .ingest_fetched_payload(&sample_payload(), &SyncScope::Sets(vec!["elev".to_string()]))
            .await
            .expect("second ingest succeeds");

        assert!(catalog.get_by_title("Card From SG").is_some(), "earlier-cached set should survive");
        assert!(catalog.get_by_title("Card From Elevation").is_some());
    }
}
