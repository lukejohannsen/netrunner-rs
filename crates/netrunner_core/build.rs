//! Concatenates the one-file-per-card DSL definitions in `data/corp` and
//! `data/runner`, the one-file-per-deck decklists in `data/decks`, and the
//! one-file-per-lesson tutorials in `data/lessons/{corp,runner}`, into a
//! single JSON array each, written to `OUT_DIR` for `include_str!` to embed
//! at compile time.
//!
//! Cards and decks are authored one-per-file so diffs stay readable and two
//! people adding entries don't collide; the concatenation is purely a build
//! artifact, never checked in. This runs at *build* time, so the embedded
//! result costs nothing at runtime — `netrunner_core` stays as I/O-free as
//! AGENTS.md's "MUST NOT depend on any I/O framework" rule intends, exactly
//! like the `include_str!`-embedded NetrunnerDB catalog dumps in `data/cards`.

use std::path::Path;

/// Concatenates every `*.json` directly under `data/<dir>` into one JSON
/// array at `OUT_DIR/<out_name>`, sorted by path so the emitted array is
/// deterministic across filesystems — otherwise the build output churns on
/// directory-order differences.
fn embed_dir(manifest_dir: &str, out_dir: &str, dir_name: &str, out_name: &str) {
    let dir = Path::new(manifest_dir).join("data").join(dir_name);
    // Re-run whenever an entry is added, removed, or edited.
    println!("cargo:rerun-if-changed=data/{dir_name}");

    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|entry| entry.unwrap_or_else(|e| panic!("reading an entry of {}: {e}", dir.display())).path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();

    let entries: Vec<String> = paths
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display())))
        .collect();

    let combined = format!("[{}]", entries.join(","));
    let out_path = Path::new(out_dir).join(out_name);
    std::fs::write(&out_path, combined).unwrap_or_else(|e| panic!("writing {}: {e}", out_path.display()));
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set for a build script");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is always set for a build script");

    for side in ["corp", "runner"] {
        embed_dir(&manifest_dir, &out_dir, side, &format!("{side}_cards.json"));
    }
    embed_dir(&manifest_dir, &out_dir, "decks", "decks.json");
    // Lessons are one directory per side because the track order is the
    // sorted file order: `01_...json`, `02_...json` — numbering across two
    // sides in one directory would interleave them.
    for side in ["corp", "runner"] {
        embed_dir(&manifest_dir, &out_dir, &format!("lessons/{side}"), &format!("{side}_lessons.json"));
    }
}
