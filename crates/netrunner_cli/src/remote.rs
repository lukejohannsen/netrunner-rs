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

pub use netrunner_client::remote::{connect, list_matches, seat, seek, spawn, watch, ConnectEvent, Connecting, Joined};

/// `netrunner_cli matches`: what the daemon is hosting, one line each.
/// `netrunner_cli profile`: the key, then the profile the server at
/// `url` keeps under it. Makes the key if there is none, as joining would.
pub async fn print_profile(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let credentials = netrunner_client::identity::Credentials::load()?;
    for line in netrunner_client::identity::key_lines(&credentials) {
        println!("{line}");
    }
    let profile = netrunner_client::remote::profile(url, &credentials).await?;
    println!("At {url}");
    for line in netrunner_client::identity::profile_lines(profile.as_ref()) {
        println!("  {line}");
    }
    Ok(())
}

/// `netrunner_cli ladder`: the server's ladder in `format`, one line per
/// listed key — best first, the name, the rating per chair and the title
/// worn. Asked on an attached connection with no key, since a ladder is
/// anyone's to read.
pub async fn print_ladder(url: &str, format: netrunner_core::format::NsgFormat) -> Result<(), Box<dyn std::error::Error>> {
    let rows = netrunner_client::remote::ladder(url, format).await?;
    println!("The {format:?} ladder at {url}");
    if rows.is_empty() {
        println!("  Nobody has {} rated games there yet", netrunner_server::protocol::LADDER_MIN_GAMES);
    }
    for (index, row) in rows.iter().enumerate() {
        println!("  {}", netrunner_client::identity::ladder_line(index + 1, row));
    }
    Ok(())
}

pub async fn print_matches(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (matches, waiting, cap) = list_matches(url).await?;
    match cap {
        Some(cap) => println!("{} of {cap} matches running, {waiting} waiting in the lobby", matches.len()),
        None => println!("{} matches running, {waiting} waiting in the lobby", matches.len()),
    }
    for summary in matches {
        // The lobby, and no decks: a deck is its player's secret, so a
        // server names none (`MatchSummary`).
        let lobby = format!(" [{}]", netrunner_client::settings::format_name(summary.format));
        println!(
            "{}  {} (Corp) vs {} (Runner){lobby}, started {}s ago",
            summary.match_id, summary.corp, summary.runner, summary.started_secs_ago
        );
    }
    Ok(())
}
