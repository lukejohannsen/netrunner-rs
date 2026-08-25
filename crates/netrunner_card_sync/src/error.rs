use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("could not determine an OS cache directory")]
    CacheDirUnavailable,

    #[error("failed to create cache directory {path:?}: {source}")]
    CreateCacheDir { path: PathBuf, source: std::io::Error },

    #[error("failed to read cached card file {path:?}: {source}")]
    ReadCacheFile { path: PathBuf, source: std::io::Error },

    #[error("failed to write cached card file {path:?}: {source}")]
    WriteCacheFile { path: PathBuf, source: std::io::Error },

    #[error("failed to rename temp cache file into place at {path:?}: {source}")]
    AtomicRename { path: PathBuf, source: std::io::Error },

    #[error("failed to parse cached/fetched card JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP request to NetrunnerDB failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("failed to load embedded default core sets: {0}")]
    Catalog(#[from] netrunner_core::catalog::CardCatalogError),
}
