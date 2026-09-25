//! Remote play, which is `netrunner_client::remote`: the one driver
//! around the connection state machine (`netrunner_client::connection`),
//! shared with the desktop. What is left here is the terminal's own:
//! printing a server's matches for `netrunner_cli matches`.
//!
//! This module used to be the connection — its own WebSocket bridge, and
//! a `Reconnector` the render loop called once a tick, blocking the
//! thread for up to three seconds under `block_in_place`. Bevy cannot
//! block its main thread, so the desktop could not have used it; the
//! machine and its driver replace both (Phase 4 §6 item 2).

pub use netrunner_client::connection::Goal;
pub use netrunner_client::remote::{connect, connect_message, list_matches, spawn, ConnectEvent, Connecting, Joined};

/// `netrunner_cli matches`: what the daemon is hosting, one line each.
pub async fn print_matches(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (matches, waiting, cap) = list_matches(url).await?;
    match cap {
        Some(cap) => println!("{} of {cap} matches running, {waiting} waiting in the lobby", matches.len()),
        None => println!("{} matches running, {waiting} waiting in the lobby", matches.len()),
    }
    for summary in matches {
        // The decklists are named because the daemon rotates the sample
        // pool: two running matches are no longer the same game with
        // different people in it. A host built before the ids were on the
        // wire sends empty strings, so they are omitted rather than shown
        // as blanks.
        let decks = match (summary.corp_deck.as_str(), summary.runner_deck.as_str()) {
            ("", "") => String::new(),
            (corp, runner) => format!(" [{corp} vs {runner}]"),
        };
        println!(
            "{}  {} (Corp) vs {} (Runner){decks}, started {}s ago",
            summary.match_id, summary.corp, summary.runner, summary.started_secs_ago
        );
    }
    Ok(())
}
