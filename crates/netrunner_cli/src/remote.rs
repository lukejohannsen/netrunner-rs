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

/// `netrunner_cli admin event …` (Phase 4 §11): hold, list or close an
/// event at `--server`, on this player's key, over an attached
/// connection that is closed once the answer is printed.
pub async fn admin_event(config: &crate::config::Config, action: crate::config::EventAction) -> Result<(), Box<dyn std::error::Error>> {
    use crate::config::EventAction;
    use netrunner_client::remote::AttachedEvent;
    use netrunner_server::protocol::EventSpec;
    let credentials = netrunner_client::identity::Credentials::load()?;
    let mut attached = attach(&config.server, &crate::record::player_name(config), credentials).await?;
    match action {
        EventAction::List => attached.list_events(),
        EventAction::Hold { name, format, wins, corp_factions, runner_factions, milestones } => {
            let rule = netrunner_core::decks::FactionRule { corp: corp_factions.into_iter().map(Into::into).collect(), runner: runner_factions.into_iter().map(Into::into).collect() };
            let spec = EventSpec { name, format: format.into(), rule: Some(rule).filter(|rule| !rule.is_open()), wins_to_clear: wins, milestones };
            if let Some(problem) = spec.problem() {
                return Err(problem.into());
            }
            attached.hold_event(spec);
        }
        EventAction::Close { code } => attached.close_event(code),
    }
    loop {
        match attached.next().await {
            Some(AttachedEvent::Events { events, operator }) => {
                println!("Events at {}{}", config.server, if operator { " (your key holds events here)" } else { "" });
                if events.is_empty() {
                    println!("  none");
                }
                for info in &events {
                    println!("  {} · {}", info.id, netrunner_client::event::line(info));
                }
                return Ok(());
            }
            Some(AttachedEvent::Event(info)) => {
                println!("{} · {}", info.id, netrunner_client::event::line(&info));
                for milestone in &info.milestones {
                    println!("  {}", netrunner_client::event::milestone_line(&info, milestone));
                }
                return Ok(());
            }
            Some(AttachedEvent::EventRefused(reason)) => return Err(reason.into()),
            Some(AttachedEvent::Link(netrunner_client::connection::Link::Down(error))) => return Err(error.into()),
            Some(_) => {}
            None => return Err("the connection closed before the server answered".into()),
        }
    }
}

/// `netrunner_cli admin status|award|tournament` (Phase 4 §12): one
/// request at `--server` on this player's key, over an attached
/// connection closed once the answer is printed.
pub async fn admin(config: &crate::config::Config, action: crate::config::AdminAction) -> Result<(), Box<dyn std::error::Error>> {
    use crate::config::{AdminAction, AwardAction, TournamentAdminAction};
    use netrunner_client::remote::AttachedEvent;
    use netrunner_server::protocol::{AdminAnswer, AdminRequest, AwardSpec};
    let credentials = netrunner_client::identity::Credentials::load()?;
    let attached = attach(&config.server, &crate::record::player_name(config), credentials).await?;
    match action {
        AdminAction::Event { .. } => unreachable!("`admin event` is `admin_event`'s"),
        AdminAction::Status => attached.admin(AdminRequest::Status),
        AdminAction::Award { action: AwardAction::Grant { key, kind, name } } => attached.admin(AdminRequest::Grant { key, award: AwardSpec { kind: kind.into(), name } }),
        AdminAction::Award { action: AwardAction::Revoke { key, kind, name } } => attached.admin(AdminRequest::Revoke { key, award: AwardSpec { kind: kind.into(), name } }),
        AdminAction::Tournament { action: TournamentAdminAction::Cancel { code } } => attached.cancel_tournament(code),
        AdminAction::Tournament { action: TournamentAdminAction::Prizes { code, prizes } } => {
            if let Some(problem) = netrunner_server::protocol::Prize::problem(&prizes) {
                return Err(problem.into());
            }
            attached.admin(AdminRequest::SetPrizes { tournament: code, prizes })
        }
    }
    let mut attached = attached;
    loop {
        match attached.next().await {
            Some(AttachedEvent::Admin(AdminAnswer::Status(status))) => {
                println!("{} at {}", status.engine, config.server);
                for line in netrunner_client::admin::status_lines(&status) {
                    println!("  {line}");
                }
                return Ok(());
            }
            Some(AttachedEvent::Admin(AdminAnswer::Awards { key, titles, badges })) => {
                println!("{} now holds", key);
                for line in netrunner_client::admin::award_lines(&titles, &badges) {
                    println!("  {line}");
                }
                return Ok(());
            }
            Some(AttachedEvent::Tournament(info) | AttachedEvent::Admin(AdminAnswer::Tournament(info))) => {
                println!("{}", netrunner_client::tournament::line(&info));
                for line in netrunner_client::tournament::prize_lines(&info) {
                    println!("  {line}");
                }
                return Ok(());
            }
            Some(AttachedEvent::AdminRefused(reason) | AttachedEvent::TournamentRefused(reason)) => return Err(reason.into()),
            Some(AttachedEvent::Link(netrunner_client::connection::Link::Down(error))) => return Err(error.into()),
            Some(_) => {}
            None => return Err("the connection closed before the server answered".into()),
        }
    }
}

/// Enters event `code` at `url` on `credentials`' key, over a connection
/// of its own, and says where the key stands in it; entered already is
/// no error, since the point is to be in. For `--event` in remote mode.
pub async fn enter_event(url: &str, player_name: &str, credentials: netrunner_client::identity::Credentials, code: &str) -> Result<netrunner_server::protocol::EventInfo, Box<dyn std::error::Error>> {
    use netrunner_client::remote::AttachedEvent;
    let mut attached = attach(url, player_name, credentials).await?;
    attached.enter_event(code.trim().to_uppercase());
    loop {
        match attached.next().await {
            Some(AttachedEvent::Event(info)) => return Ok(info),
            Some(AttachedEvent::EventRefused(reason)) if reason.contains("entered this event already") => {
                attached.list_events();
            }
            Some(AttachedEvent::Events { events, .. }) => {
                return events.into_iter().find(|info| info.id.eq_ignore_ascii_case(code.trim())).ok_or_else(|| "that event has gone".into());
            }
            Some(AttachedEvent::EventRefused(reason)) => return Err(reason.into()),
            Some(AttachedEvent::Link(netrunner_client::connection::Link::Down(error))) => return Err(error.into()),
            Some(_) => {}
            None => return Err("the connection closed before the server answered".into()),
        }
    }
}

/// An attached connection at `url` on this key, awaited to its first
/// `Attached`, for the one-shot commands above.
async fn attach(url: &str, player_name: &str, credentials: netrunner_client::identity::Credentials) -> Result<netrunner_client::remote::Attached, Box<dyn std::error::Error>> {
    use netrunner_client::connection::{Goal, Who};
    use netrunner_client::connection::Link;
    use netrunner_client::remote::AttachedEvent;
    let mut attached = spawn(url.to_string(), Goal::Attach(Who { player_name: player_name.to_string(), credentials: Some(Box::new(credentials)) }));
    loop {
        match attached.next().await {
            Some(AttachedEvent::Attached(_)) => return Ok(attached),
            Some(AttachedEvent::Link(Link::Down(error))) => return Err(error.into()),
            Some(_) => {}
            None => return Err("the connection closed before it attached".into()),
        }
    }
}
