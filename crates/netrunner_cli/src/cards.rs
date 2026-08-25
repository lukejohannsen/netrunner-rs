//! `netrunner_cli cards ...` — lists NetrunnerDB sets or fetches/caches
//! card data via `netrunner_card_sync`. Purely additive: does not touch the
//! existing TUI/headless game-play path.

use netrunner_card_sync::{NetrunnerDbSync, SyncScope};

use crate::config::CardsAction;

pub async fn run(action: CardsAction) -> Result<(), Box<dyn std::error::Error>> {
    let sync = NetrunnerDbSync::new()?;

    match action {
        CardsAction::ListSets => {
            let mut sets = sync.list_available_sets().await?;
            sets.sort_by(|a, b| a.code.cmp(&b.code));
            for pack in sets {
                println!("{:<8} {}", pack.code, pack.name);
            }
        }
        CardsAction::Sync { all, set } => {
            let scope = match (all, set.is_empty()) {
                (true, _) => SyncScope::All,
                (false, false) => SyncScope::Sets(set),
                (false, true) => return Err("specify --all or at least one --set <code>".into()),
            };
            let catalog = sync.sync_from_netrunnerdb(scope).await?;
            println!("Synced. Catalog now has {} card(s).", catalog.len());
        }
    }

    Ok(())
}
