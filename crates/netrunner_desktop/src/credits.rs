//! The credits register (`assets/CREDITS.md`), read once for everyone who
//! needs it: the About screen shows it, and `tests/credits.rs` holds the
//! directory and the policy to it.
//!
//! **One file, read by the program, rather than a list in Rust beside a
//! list in Markdown.** The register is where a person adding an asset
//! writes its owner down, so the screen that credits the owner reads the
//! same rows; a second list would be the one that fell behind. It is
//! compiled in, so the About screen needs no file at run time and a
//! release binary credits what it was built with.

/// The register as committed.
pub const REGISTER: &str = include_str!("../assets/CREDITS.md");

/// A committed asset: a row of the register's own table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub file: String,
    pub what: String,
    pub owner: String,
    pub website: String,
    pub licence: String,
    pub licence_text: String,
    /// `project` or `third-party`.
    pub origin: String,
    pub changes: String,
}

/// Something the player sees that is not committed: fetched on their
/// opt-in, or the software the client is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credit {
    pub what: String,
    pub owner: String,
    pub website: String,
    pub terms: String,
    pub from: String,
}

/// The rows of the table under the `## <heading>` whose title starts
/// with `heading`, split into trimmed cells, backticks dropped. The
/// header row and its rule are skipped.
fn table(heading: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut inside = false;
    let mut header_seen = false;
    for line in REGISTER.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            inside = title.starts_with(heading);
            header_seen = false;
            continue;
        }
        if !inside || !line.starts_with('|') {
            continue;
        }
        if !header_seen {
            header_seen = true;
            continue;
        }
        if line.starts_with("|---") {
            continue;
        }
        rows.push(line.trim().trim_matches('|').split(" | ").map(|c| c.trim().replace('`', "")).collect());
    }
    rows
}

/// Every committed asset, in the register's order.
pub fn bundled() -> Vec<Asset> {
    table("The register")
        .into_iter()
        .filter(|cells| cells.len() == 8)
        .map(|c| Asset {
            file: c[0].clone(),
            what: c[1].clone(),
            owner: c[2].clone(),
            website: c[3].clone(),
            licence: c[4].clone(),
            licence_text: c[5].clone(),
            origin: c[6].clone(),
            changes: c[7].clone(),
        })
        .collect()
}

fn credits(heading: &str) -> Vec<Credit> {
    table(heading)
        .into_iter()
        .filter(|cells| cells.len() == 5)
        .map(|c| Credit { what: c[0].clone(), owner: c[1].clone(), website: c[2].clone(), terms: c[3].clone(), from: c[4].clone() })
        .collect()
}

/// What the player's client fetches on their opt-in and never ships.
pub fn fetched() -> Vec<Credit> {
    credits("Fetched")
}

/// The software the client is built with.
pub fn software() -> Vec<Credit> {
    credits("Software")
}

/// The register's closing disclaimer: its last paragraph.
pub fn disclaimer() -> String {
    REGISTER.trim_end().rsplit("\n\n").next().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_register_reads_as_its_three_tables_and_a_disclaimer() {
        let assets = bundled();
        assert!(assets.iter().any(|a| a.file == "fonts/NotoSans-Regular.ttf" && a.owner.contains("Noto")), "{assets:?}");
        assert!(assets.iter().all(|a| !a.file.is_empty() && !a.file.contains('`')));
        let fetched = fetched();
        assert!(fetched.iter().any(|c| c.from.contains("netrunnerdb")), "{fetched:?}");
        assert!(software().iter().any(|c| c.what.starts_with("Bevy")));
        assert!(disclaimer().starts_with("This client is a free, open-source fan implementation"), "{}", disclaimer());
    }
}
