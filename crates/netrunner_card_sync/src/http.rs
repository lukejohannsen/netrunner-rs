use std::path::{Path, PathBuf};

/// One HTTP client shape for everything this crate fetches. NetrunnerDB
/// asks API consumers to identify themselves, and a request with no
/// timeout can hang an image download forever on a dropped connection;
/// the first version of this crate had neither.
pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("netrunner-rs/", env!("CARGO_PKG_VERSION"), " (+https://github.com/lukejohannsen/netrunner-rs)"))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("a client with a user agent and a timeout builds")
}

/// The temp file a write goes to before it is renamed over `target`:
/// beside it, so the rename never crosses a filesystem, and named for this
/// process, so two clients writing the same cache never share one.
pub(crate) fn temp_file_path(target: &Path) -> PathBuf {
    let file_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("download");
    let temp_name = format!("{file_name}.tmp.{}", std::process::id());
    target.with_file_name(temp_name)
}
