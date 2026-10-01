//! `netrunner_cli cards ...` — the card-image cache, measured against the
//! embedded catalog's printings, and filled on request through
//! `netrunner_card_sync`. Purely additive: does not touch the TUI or
//! headless game-play path.
//!
//! `cards sync` and `cards list-sets` fetched NetrunnerDB's v2 card and pack
//! lists into a cache nothing read; they went with the v2 catalog (NSG pool
//! Stage 0d). The embedded catalog changes through `scripts/catalog_sync.py`.

use netrunner_card_sync::CardImageStore;
use netrunner_core::card::PrintingId;
use netrunner_core::cards::catalog;

use crate::config::CardsAction;

pub async fn run(action: CardsAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        CardsAction::Images { set, download } => images(&set, download).await?,
    }
    Ok(())
}

/// The embedded catalog's printings in `sets` (all when empty), measured
/// against the image cache. The low-resolution list is the one to read:
/// a code on it is a card no client can draw sharply on a large screen,
/// and the first place to look for a better source.
async fn images(sets: &[String], download: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut printings: Vec<(PrintingId, String, String)> = catalog::printings()
        .filter(|printing| sets.is_empty() || sets.contains(&printing.set))
        .map(|printing| {
            let title = catalog::cards().get(&printing.card).map_or_else(|| printing.card.0.clone(), |card| card.title.clone());
            (printing.id, printing.set.clone(), title)
        })
        .collect();
    printings.sort();
    let codes: Vec<PrintingId> = printings.iter().map(|(code, ..)| *code).collect();

    let store = CardImageStore::new()?;
    if download {
        let report = store.download(codes.clone(), 4, None).await;
        println!("Downloaded {} ({} were already cached, {} failed).", report.fetched, report.already_cached, report.failed.len());
        for (code, reason) in &report.failed {
            println!("  failed {code}: {reason}");
        }
    }

    let low_res = store.low_res();
    let cached = store.cached_count(&codes);
    let low: Vec<_> = printings.iter().filter(|(code, ..)| low_res.contains(code)).collect();
    println!("{cached} of {} printings cached in {}; {} only at 300 px.", codes.len(), store.dir().display(), low.len());
    for (code, set, title) in low {
        println!("  {code}  {set:<20} {title}");
    }
    Ok(())
}
