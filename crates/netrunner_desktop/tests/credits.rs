//! Every committed asset is a separately licensed work, and
//! `assets/CREDITS.md` is its register: who owns it, where they publish,
//! under what licence, and what was changed. This keeps the register and
//! the directory in step — an asset added without a row, or a row left
//! behind by an asset removed, fails here — and holds each row to the
//! policy the register states: a project-made asset is GPL-3.0-or-later
//! and the contributors', a third-party one names its owner and carries
//! the owner's licence text beside it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use netrunner_desktop::credits::{Asset, ALL_RIGHTS_RESERVED};

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// The register's own table, as the About screen reads it.
fn rows() -> Vec<Asset> {
    netrunner_desktop::credits::bundled()
}

/// Every file under `assets/` that is an asset rather than about one: the
/// guides (`*.md`), the licence texts (`LICENSE-*`) and the manifests
/// (`*.json`, a backdrop's dimming or a table's settings) are the
/// register's own furniture — settings, not a work anybody is credited
/// for.
fn committed(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).expect("readable") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            committed(&path, root, out);
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name.ends_with(".md") || name.ends_with(".json") || name.starts_with("LICENSE-") || name.starts_with('.') {
            continue;
        }
        out.insert(path.strip_prefix(root).expect("under assets").to_string_lossy().replace('\\', "/"));
    }
}

#[test]
fn every_committed_asset_has_a_row_and_every_row_an_asset() {
    let root = assets();
    let mut files = BTreeSet::new();
    committed(&root, &root, &mut files);
    // A row with the wrong number of cells would be dropped by the reader
    // and read as a missing row; say which it is instead.
    let written = netrunner_desktop::credits::REGISTER.lines().filter(|l| l.starts_with("| `")).count();
    assert_eq!(rows().len(), written, "a register row does not have its eight columns");
    let listed: BTreeSet<String> = rows().into_iter().map(|r| r.file).collect();
    let unlisted: Vec<_> = files.difference(&listed).collect();
    let gone: Vec<_> = listed.difference(&files).collect();
    assert!(unlisted.is_empty(), "committed without a row in assets/CREDITS.md — find its owner and licence first: {unlisted:?}");
    assert!(gone.is_empty(), "rows in assets/CREDITS.md for files that are gone — delete the rows: {gone:?}");
}

#[test]
fn every_row_names_its_owner_its_licence_and_what_was_changed() {
    for row in rows() {
        let file = &row.file;
        assert!(!row.owner.is_empty(), "{file}: no owner");
        assert!(row.website == "—" || row.website.starts_with("https://") || row.website.starts_with("http://"), "{file}: the website is a link, or — when the owner has none");
        assert!(!row.licence.is_empty(), "{file}: no licence");
        assert!(!row.changes.is_empty(), "{file}: say `none` or what was changed");
        match row.origin.as_str() {
            // Made here, by hand, by code or with AI assistance: the
            // project's, under the project's licence.
            "project" => {
                assert_eq!(row.licence, "GPL-3.0-or-later", "{file}: a project-made asset is GPL-3.0-or-later");
                assert_eq!(row.owner, "netrunner-rs contributors", "{file}: a project-made asset is the contributors'");
            }
            // Someone else's: it keeps their licence, and the text of it
            // ships beside the file — unless there is no licence, when it
            // is shipped on credit alone and removed if the owner asks
            // (the register's "Art under no licence").
            "third-party" => {
                assert!(row.owner != "netrunner-rs contributors", "{file}: a third-party asset names its real owner");
                if row.licence == ALL_RIGHTS_RESERVED {
                    assert_eq!(row.licence_text, "—", "{file}: there is no licence text for a work under no licence");
                    assert!(row.website != "—", "{file}: a work shipped on credit alone names where its owner publishes");
                } else {
                    assert!(row.licence_text != "—" && assets().join(&row.licence_text).is_file(), "{file}: the owner's licence text {} is committed", row.licence_text);
                }
            }
            other => panic!("{file}: origin is `project` or `third-party`, not {other:?}"),
        }
    }
}

/// Everything a player sees and the project does not own is credited
/// with its owner and a website, whether or not it is committed.
#[test]
fn what_is_fetched_and_what_the_client_is_built_with_are_credited_too() {
    let others: Vec<_> = netrunner_desktop::credits::fetched().into_iter().chain(netrunner_desktop::credits::software()).collect();
    assert!(others.len() >= 4, "{} rows", others.len());
    for credit in others {
        assert!(!credit.owner.is_empty() && !credit.terms.is_empty(), "{}: an owner and terms", credit.what);
        assert!(credit.website.starts_with("https://"), "{}: the owner's website", credit.what);
    }
}
