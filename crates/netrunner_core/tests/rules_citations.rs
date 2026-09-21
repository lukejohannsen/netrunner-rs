//! Every rule this workspace cites is a rule in the committed copy of the
//! Comprehensive Rules.
//!
//! The engine's comments and the roadmap cite Null Signal Games' rules by
//! number — `CR <number>` or `Comprehensive Rules <number>` — and until this
//! gate each one was read off the live page on the day it was written. The
//! page is revised a few times a year and renumbers as it goes, so a
//! citation could quietly come to mean another rule, or none: the first run
//! of this test found one pointing at a rule that does not exist (Priority
//! Construction's example is 1.12.2a's, not the cited 1.12.3a's).
//!
//! This checks only that the number exists in `rules/manifest.json`, which
//! `scripts/rules_sync.py` writes. Whether a cited rule still *says* what
//! the code assumes is the other half, and it needs the live page:
//! `rules_sync.py --check` names every citation whose rule changed text or
//! number, and `.github/workflows/rules-watch.yml` runs it weekly. No
//! network here — this runs on every `cargo test`.
//!
//! It scans the same files the script does: `crates/**/*.rs`, `docs/**/*.md`
//! and the Markdown at the repository root.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repository root")
}

fn manifest_numbers(root: &Path) -> BTreeSet<String> {
    let text = fs::read_to_string(root.join("rules/manifest.json"))
        .expect("rules/manifest.json — regenerate it with scripts/rules_sync.py");
    let manifest: serde_json::Value = serde_json::from_str(&text).expect("manifest is JSON");
    manifest["entries"]
        .as_array()
        .expect("manifest has entries")
        .iter()
        .map(|e| e["number"].as_str().expect("entry has a number").to_owned())
        .collect()
}

fn files_under(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            files_under(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// The numbers cited on one line, the same pattern as the script's
/// `CITATION`: a whole-word `CR` or `Comprehensive Rules`, whitespace, then
/// dotted digits with at most one trailing lowercase letter.
fn citations(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    for marker in ["CR", "Comprehensive Rules"] {
        let mut from = 0;
        while let Some(at) = line[from..].find(marker) {
            let start = from + at;
            from = start + marker.len();
            if line[..start].chars().next_back().is_some_and(|c| c.is_ascii_alphabetic()) {
                continue;
            }
            let rest = &line[from..];
            let trimmed = rest.trim_start();
            if trimmed.len() == rest.len() {
                continue;
            }
            let bytes = trimmed.as_bytes();
            let mut end = 0;
            loop {
                let digits = bytes[end..].iter().take_while(|b| b.is_ascii_digit()).count();
                if digits == 0 {
                    break;
                }
                end += digits;
                if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                } else {
                    break;
                }
            }
            if end == 0 {
                continue;
            }
            if bytes.get(end).is_some_and(u8::is_ascii_lowercase) {
                end += 1;
            }
            found.push(trimmed[..end].to_owned());
        }
    }
    found
}

#[test]
fn every_cited_rule_is_in_the_committed_comprehensive_rules() {
    let root = repo_root();
    let numbers = manifest_numbers(&root);

    let mut files = Vec::new();
    files_under(&root.join("crates"), "rs", &mut files);
    files_under(&root.join("docs"), "md", &mut files);
    for entry in fs::read_dir(&root).expect("repository root").flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            files.push(path);
        }
    }

    let mut cited = 0;
    let mut missing = Vec::new();
    for path in &files {
        let Ok(text) = fs::read_to_string(path) else { continue };
        for (n, line) in text.lines().enumerate() {
            for number in citations(line) {
                cited += 1;
                if !numbers.contains(&number) {
                    let shown = path.strip_prefix(&root).unwrap_or(path).display();
                    missing.push(format!("{shown}:{}: CR {number}", n + 1));
                }
            }
        }
    }

    assert!(cited > 0, "no citations found — the scan is looking in the wrong place");
    assert!(
        missing.is_empty(),
        "these citations name no rule in rules/comprehensive-rules.md \
         (grep it for the rule meant and cite its current number):\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn the_citation_pattern_reads_what_the_script_reads() {
    assert_eq!(citations("a cost is not prevented (CR 1.16.1a)."), ["1.16.1a"]);
    assert_eq!(citations("CR 10.3. And Comprehensive Rules 1.10.5b"), ["10.3", "1.10.5b"]);
    assert_eq!(citations("the CR 6.9.4 movement phase"), ["6.9.4"]);
    assert!(citations("SCR 1.2, CRC 3, CR: none").is_empty());
}
