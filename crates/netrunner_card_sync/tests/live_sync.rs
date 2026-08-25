//! Real-network integration test, excluded from the default `cargo test`
//! run (see `#[ignore]` below). Run explicitly with:
//! `cargo test -p netrunner_card_sync -- --ignored`

use netrunner_card_sync::{NetrunnerDbSync, SyncScope};

#[tokio::test]
#[ignore]
async fn syncs_system_gateway_from_the_real_netrunnerdb_api() {
    let dir = std::env::temp_dir().join(format!("netrunner_card_sync_live_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let sync = NetrunnerDbSync::with_cache_file(dir.join("cards.json"));

    let catalog = sync
        .sync_from_netrunnerdb(SyncScope::Sets(vec!["sg".to_string()]))
        .await
        .expect("live sync against netrunnerdb.com should succeed");

    assert!(catalog.get_by_title("Wildcat Strike").is_some());

    let sets = sync.list_available_sets().await.expect("live pack listing should succeed");
    assert!(sets.iter().any(|pack| pack.code == "sg"));

    std::fs::remove_dir_all(&dir).ok();
}
