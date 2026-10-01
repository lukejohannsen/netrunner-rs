use std::path::PathBuf;

use thiserror::Error;

use netrunner_core::card::PrintingId;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("could not determine an OS cache directory")]
    CacheDirUnavailable,

    #[error("failed to create cache directory {path:?}: {source}")]
    CreateCacheDir { path: PathBuf, source: std::io::Error },

    #[error("failed to rename temp cache file into place at {path:?}: {source}")]
    AtomicRename { path: PathBuf, source: std::io::Error },

    #[error("failed to read or write the image manifest: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP request to NetrunnerDB failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("NetrunnerDB answered {status} for the image of printing {code}")]
    ImageDownload { code: PrintingId, status: u16 },

    #[error("what NetrunnerDB served for the image of printing {code} is not a picture")]
    ImageInvalid { code: PrintingId },

    #[error("failed to write card image {path:?}: {source}")]
    ImageWrite { path: PathBuf, source: std::io::Error },

    #[error("NetrunnerDB has no published decklist {id}: it may be unpublished, or the link may be wrong")]
    DecklistNotFound { id: String },

    #[error("NetrunnerDB answered {status} for decklist {id}")]
    DecklistDownload { id: String, status: u16 },

    #[error("NetrunnerDB refused the decklist: {0}")]
    DecklistRefused(String),

    #[error("NetrunnerDB's answer is not a decklist: {0}")]
    DecklistShape(String),
}
