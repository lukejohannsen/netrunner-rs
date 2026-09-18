//! `netrunner_cli cards ...` — lists NetrunnerDB sets or fetches/caches
//! card data via `netrunner_card_sync`. Purely additive: does not touch the
//! existing TUI/headless game-play path.

use netrunner_card_sync::{CardImageStore, NetrunnerDbSync, SyncScope};
use netrunner_core::card::CardId;
use netrunner_core::cards::load_embedded_netrunnerdb_sets;

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
            let registry = sync.sync_from_netrunnerdb(scope).await?;
            println!("Synced. Catalog now has {} card(s).", registry.len());
        }
        CardsAction::Images { set, download } => images(&set, download).await?,
    }

    Ok(())
}

/// The embedded catalog's printings in `sets` (all when empty), measured
/// against the image cache. The low-resolution list is the one to read:
/// a code on it is a card no client can draw sharply on a large screen,
/// and the first place to look for a better source.
async fn images(sets: &[String], download: bool) -> Result<(), Box<dyn std::error::Error>> {
    let catalog = load_embedded_netrunnerdb_sets()?;
    let mut printings: Vec<(CardId, String, String)> = catalog
        .iter()
        .filter_map(|card| {
            let set_code = card.set_code.clone().unwrap_or_default();
            let wanted = sets.is_empty() || sets.contains(&set_code);
            card.numeric_id.filter(|_| wanted).map(|code| (code, set_code, card.title.clone()))
        })
        .collect();
    printings.sort();
    let codes: Vec<CardId> = printings.iter().map(|(code, ..)| *code).collect();

    let store = CardImageStore::new()?;
    if download {
        let _ = store.refresh_template().await;
        let report = store.download(codes.clone(), 4, None).await;
        println!("Downloaded {} ({} were already cached, {} failed).", report.fetched, report.already_cached, report.failed.len());
        for (code, reason) in &report.failed {
            println!("  failed {:05}: {reason}", code.0);
        }
    }

    let low_res = store.low_res();
    let cached = store.cached_count(&codes);
    let low: Vec<_> = printings.iter().filter(|(code, ..)| low_res.contains(code)).collect();
    println!("{cached} of {} printings cached in {}; {} only at 300 px.", codes.len(), store.dir().display(), low.len());
    for (code, set_code, title) in low {
        println!("  {:05}  {:<5} {title}", code.0, set_code);
    }
    Ok(())
}
