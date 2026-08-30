//! Concatenates the one-file-per-card DSL definitions in `data/corp` and
//! `data/runner` into a single JSON array per side, written to `OUT_DIR` for
//! `include_str!` to embed at compile time.
//!
//! Cards are authored one-per-file so diffs stay readable and two people
//! adding cards don't collide; the concatenation is purely a build artifact,
//! never checked in. This runs at *build* time, so the embedded result costs
//! nothing at runtime — `netrunner_core` stays as I/O-free as AGENTS.md's
//! "MUST NOT depend on any I/O framework" rule intends, exactly like the
//! `include_str!`-embedded NetrunnerDB catalog dumps in `data/cards`.

use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set for a build script");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is always set for a build script");

    for side in ["corp", "runner"] {
        let dir = Path::new(&manifest_dir).join("data").join(side);
        // Re-run whenever a card is added, removed, or edited.
        println!("cargo:rerun-if-changed=data/{side}");

        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
            .map(|entry| entry.unwrap_or_else(|e| panic!("reading an entry of {}: {e}", dir.display())).path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect();
        // Sort so the emitted array is deterministic across filesystems —
        // otherwise the build output churns on directory-order differences.
        paths.sort();

        let cards: Vec<String> = paths
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display())))
            .collect();

        let combined = format!("[{}]", cards.join(","));
        let out_path = Path::new(&out_dir).join(format!("{side}_cards.json"));
        std::fs::write(&out_path, combined).unwrap_or_else(|e| panic!("writing {}: {e}", out_path.display()));
    }
}
