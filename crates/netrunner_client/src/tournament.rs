//! The words both clients put on a tournament (Phase 4 §7 stage 6b): how
//! a tournament is named in a list, what its state is called, and which
//! of its entrants is this player — so the terminal's rows and the
//! desktop's buttons say the same thing and neither spells it twice.
//!
//! **No I/O and no rules.** Everything here reads a `TournamentInfo` the
//! server sent; the server decides who may register, whose key an entry
//! is under and what a state means. A client only shows it.

use netrunner_identity::PublicKey;
use netrunner_protocol::swiss::{Outcome, Role, Round, Standing, Table};
use netrunner_protocol::{DrawOffer, Entrant, TournamentInfo, TournamentState};

use crate::settings::format_label;

/// A tournament's state in a phrase, for its row and its page.
/// Exhaustive, so a later stage (the cut) adds its words here.
pub fn state_label(state: TournamentState) -> String {
    match state {
        TournamentState::Registering => "taking registrations".to_string(),
        TournamentState::Playing { round } => format!("round {round}"),
        TournamentState::Finished => "finished".to_string(),
    }
}

/// An entrant's name by their key — the label they attached with — or the
/// key's fingerprint for a key the tournament does not list.
pub fn name_of(info: &TournamentInfo, key: &PublicKey) -> String {
    info.entrants.iter().find(|entrant| entrant.key == *key).map_or_else(|| key.fingerprint(), |entrant| entrant.name.clone())
}

/// One table of a round as a line names it: who sits where, how it
/// ended — "Table 1 · ann (Corp) vs bo (Runner) · ann won" — and, while
/// it has not, who has offered a draw there.
pub fn table_line(info: &TournamentInfo, index: usize, table: &Table<PublicKey>) -> String {
    let (corp, runner) = (name_of(info, &table.corp), name_of(info, &table.runner));
    let result = match table.result {
        None => {
            let offers: Vec<String> = draw_offers_at(info, index).map(|key| name_of(info, key)).collect();
            if offers.is_empty() { "not played yet".to_string() } else { format!("not played yet · {} offers a draw", offers.join(" and ")) }
        }
        Some(Outcome::CorpWon) => format!("{corp} won"),
        Some(Outcome::RunnerWon) => format!("{runner} won"),
        Some(Outcome::Tie) => "a tie".to_string(),
    };
    format!("Table {} · {corp} (Corp) vs {runner} (Runner) · {result}", index + 1)
}

/// Who has offered an intentional draw at table `index` of the current
/// round and not been answered (Organized Play Policies 2.5.8).
pub fn draw_offers_at(info: &TournamentInfo, index: usize) -> impl Iterator<Item = &PublicKey> {
    info.draw_offers.iter().filter(move |offer| offer.table == index).map(|offer: &DrawOffer| &offer.by)
}

/// Whether this key has a draw offer standing at its table this round.
pub fn has_offered_draw(info: &TournamentInfo, key: Option<&PublicKey>) -> bool {
    key.is_some_and(|key| my_table(info, Some(key)).is_some_and(|(index, _, _)| draw_offers_at(info, index).any(|by| by == key)))
}

/// Whether this key may offer a draw: a table this round with no result,
/// and no offer of theirs standing there. The server refuses one while
/// the game is under way; the page shows the button and lets it say so.
pub fn may_offer_draw(info: &TournamentInfo, key: Option<&PublicKey>) -> bool {
    my_table(info, key).is_some_and(|(_, _, table)| table.result.is_none()) && !has_offered_draw(info, key)
}

/// The bye's line, when the round has one.
pub fn bye_line(info: &TournamentInfo, round: &Round<PublicKey>) -> Option<String> {
    round.bye.as_ref().map(|key| format!("{} sits this round out with a bye", name_of(info, key)))
}

/// One row of the standings: rank, name, points, the record as
/// wins–ties–losses with byes noted, the two tiebreakers to two places,
/// and "dropped" for a player who left — "1. ann · 6 pts · 2–0–0 · SoS
/// 1.50 · ESoS 1.25".
pub fn standing_row(info: &TournamentInfo, rank: usize, standing: &Standing<PublicKey>) -> String {
    let byes = match standing.byes {
        0 => String::new(),
        1 => " (a bye)".to_string(),
        n => format!(" ({n} byes)"),
    };
    let dropped = if info.dropped.contains(&standing.key) { " · dropped" } else { "" };
    format!(
        "{rank}. {} · {} pts · {}–{}–{}{byes} · SoS {:.2} · ESoS {:.2}{dropped}",
        name_of(info, &standing.key),
        standing.points,
        standing.wins + standing.byes,
        standing.ties,
        standing.losses,
        standing.sos,
        standing.esos
    )
}

/// This key's place in the round being played: the table's index, the
/// chair, and the opponent — or `None` for the bye and for a key not in
/// the tournament.
pub fn my_table<'a>(info: &'a TournamentInfo, key: Option<&PublicKey>) -> Option<(usize, Role, &'a Table<PublicKey>)> {
    let key = key?;
    let (index, table) = info.current_round()?.table_of(key)?;
    Some((index, table.role_of(key)?, table))
}

/// What the page says about the round for this key: their table and
/// chair, their bye, or that the round is being played without them.
pub fn round_line(info: &TournamentInfo, key: Option<&PublicKey>) -> Option<String> {
    let round = info.current_round()?;
    let number = match info.state {
        TournamentState::Playing { round } => round,
        _ => return None,
    };
    Some(match (my_table(info, key), key) {
        (Some((index, role, table)), Some(my_key)) => {
            let opponent = table.opponent_of(my_key).map_or_else(String::new, |opponent| name_of(info, opponent));
            match table.result {
                None => {
                    let mut line = format!("Round {number}: you play {role:?} against {opponent} at table {}.", index + 1);
                    if has_offered_draw(info, key) {
                        line.push_str(" You have offered a draw; the table is a tie if they offer one too.");
                    } else if draw_offers_at(info, index).next().is_some() {
                        line.push_str(&format!(" {opponent} offers a draw: offer one back to agree."));
                    }
                    line
                }
                Some(_) => format!("Round {number}: your game against {opponent} at table {} is over.", index + 1),
            }
        }
        (None, Some(my_key)) if round.bye.as_ref() == Some(my_key) => format!("Round {number}: you sit this one out with a bye, which counts as a win."),
        (None, Some(_)) if is_dropped(info, key) => format!("Round {number} is being played; you have dropped."),
        _ => format!("Round {number} is being played."),
    })
}

/// The round's clock at `now` (seconds since the Unix epoch, the
/// client's own): what is left, to the minute, or that time is called
/// and what that means (Organized Play Policies 1.1.5.3). `None` outside
/// a round. The server's clock and the client's may differ by a little,
/// which a minute swallows; the server is what refuses a seat.
pub fn clock_line(info: &TournamentInfo, now: u64) -> Option<String> {
    let TournamentState::Playing { round } = info.state else { return None };
    let clock = info.clock?;
    Some(match clock.remaining(now) {
        Some(left) if left >= 60 => match left.div_ceil(60) {
            1 => format!("Round {round}: 1 minute left."),
            minutes => format!("Round {round}: {minutes} minutes left."),
        },
        Some(_) => format!("Round {round}: under a minute left."),
        None => format!("Round {round}: time is called. A game under way finishes the turn in play and one more, then agenda points decide; a table not yet played is the organizer's to record."),
    })
}

/// A tournament as a list names it: its name, its format, how many have
/// entered and where it stands — "Friday Night · Startup · 4 entered ·
/// taking registrations". The code is not here: it is the page's heading,
/// because it is what a person reads out to a friend, not what they scan
/// a list by.
pub fn line(info: &TournamentInfo) -> String {
    let entered = match info.entrants.len() {
        1 => "1 entered".to_string(),
        n => format!("{n} entered"),
    };
    format!("{} · {} · {entered} · {}", info.name, format_label(info.format), state_label(info.state))
}

/// This key's entry in the tournament, if it has one.
pub fn entry_of<'a>(info: &'a TournamentInfo, key: Option<&PublicKey>) -> Option<&'a Entrant> {
    let key = key?;
    info.entrants.iter().find(|entrant| entrant.key == *key)
}

/// Whether this key holds the tournament — the one the policies give the
/// final say (Organized Play Policies 3.2.1).
pub fn is_organizer(info: &TournamentInfo, key: Option<&PublicKey>) -> bool {
    key.is_some_and(|key| info.organizer == *key)
}

/// Whether this key dropped mid-event: still an entrant, still in the
/// standings, paired no more.
pub fn is_dropped(info: &TournamentInfo, key: Option<&PublicKey>) -> bool {
    key.is_some_and(|key| info.dropped.contains(key))
}

/// Whether this key may drop: entered in a tournament in its rounds and
/// not dropped already. The server refuses a drop with a game under way;
/// a client shows the button and lets the server say so.
pub fn may_drop(info: &TournamentInfo, key: Option<&PublicKey>) -> bool {
    matches!(info.state, TournamentState::Playing { .. }) && entry_of(info, key).is_some() && !is_dropped(info, key)
}

/// One entrant as a list names them: the name they attached with and the
/// fingerprint of their key, since a name is a label anyone may wear and
/// the key is who they are (Phase 4 §5).
pub fn entrant_line(entrant: &Entrant) -> String {
    format!("{} · {}", entrant.name, entrant.key.fingerprint())
}

/// What the page says over its buttons about this player's standing in
/// the tournament: entered with which decks (by the hashes the server
/// published — the lists are private through Swiss), organizing it, or
/// neither.
pub fn standing_line(info: &TournamentInfo, key: Option<&PublicKey>) -> String {
    let mut parts = Vec::new();
    if is_organizer(info, key) {
        parts.push("You hold this tournament.".to_string());
    }
    match (key, entry_of(info, key)) {
        (None, _) => parts.push("This client has no key, so it cannot enter.".to_string()),
        (Some(_), Some(_)) if is_dropped(info, key) => parts.push("You dropped from this tournament; your results stand.".to_string()),
        (Some(_), Some(_)) => parts.push("You are entered, with the two decks you committed to.".to_string()),
        (Some(_), None) if info.state == TournamentState::Registering => parts.push("Not entered yet.".to_string()),
        (Some(_), None) => parts.push("Not entered.".to_string()),
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::format::NsgFormat;
    use netrunner_identity::Identity;

    fn info(entrants: Vec<Entrant>) -> TournamentInfo {
        TournamentInfo { id: "K7M2QX".into(), name: "Friday Night".into(), format: NsgFormat::Startup, organizer: Identity::from_secret([1; 32]).public_key(), state: TournamentState::Registering, entrants, seeding: Vec::new(), rounds: Vec::new(), dropped: Vec::new(), draw_offers: Vec::new(), clock: None }
    }

    fn entrant(byte: u8, name: &str) -> Entrant {
        let identity = Identity::from_secret([byte; 32]);
        Entrant { name: name.into(), key: identity.public_key(), corp_hash: "c".into(), runner_hash: "r".into(), commitment: identity.sign(b"test", "{}".into()) }
    }

    #[test]
    fn a_tournament_is_named_by_what_a_person_scans_a_list_for() {
        assert_eq!(line(&info(vec![])), "Friday Night · Startup · 0 entered · taking registrations");
        assert_eq!(line(&info(vec![entrant(2, "bo")])), "Friday Night · Startup · 1 entered · taking registrations");
    }

    /// The organizer is told so; an entrant is told so; a client with no
    /// key is told why it cannot enter.
    #[test]
    fn the_standing_line_says_who_this_key_is_to_the_tournament() {
        let (ann, bo, cy) = (Identity::from_secret([1; 32]).public_key(), Identity::from_secret([2; 32]).public_key(), Identity::from_secret([3; 32]).public_key());
        let info = info(vec![entrant(2, "bo")]);
        assert_eq!(standing_line(&info, Some(&ann)), "You hold this tournament. Not entered yet.");
        assert_eq!(standing_line(&info, Some(&bo)), "You are entered, with the two decks you committed to.");
        assert_eq!(standing_line(&info, Some(&cy)), "Not entered yet.");
        assert_eq!(standing_line(&info, None), "This client has no key, so it cannot enter.");
        assert!(entry_of(&info, Some(&bo)).is_some() && entry_of(&info, Some(&cy)).is_none() && entry_of(&info, None).is_none());
        assert!(is_organizer(&info, Some(&ann)) && !is_organizer(&info, Some(&bo)));
        assert!(entrant_line(&entrant(2, "bo")).starts_with("bo · "));
    }

    /// The round's words: each table by its names and result, the bye,
    /// a standing's row, and what this key is told about its own table.
    #[test]
    fn a_round_is_told_by_names_and_this_key_by_its_table() {
        let (ann, bo, cy) = (Identity::from_secret([2; 32]).public_key(), Identity::from_secret([3; 32]).public_key(), Identity::from_secret([4; 32]).public_key());
        let mut info = info(vec![entrant(2, "ann"), entrant(3, "bo"), entrant(4, "cy")]);
        info.seeding = vec![ann, bo, cy];
        info.rounds = vec![Round { tables: vec![Table { corp: bo, runner: ann, result: None }], bye: Some(cy) }];
        info.state = TournamentState::Playing { round: 1 };
        assert_eq!(state_label(info.state), "round 1");
        assert_eq!(line(&info), "Friday Night · Startup · 3 entered · round 1");
        assert_eq!(table_line(&info, 0, &info.rounds[0].tables[0]), "Table 1 · bo (Corp) vs ann (Runner) · not played yet");
        assert_eq!(bye_line(&info, &info.rounds[0]).as_deref(), Some("cy sits this round out with a bye"));
        assert_eq!(round_line(&info, Some(&ann)).as_deref(), Some("Round 1: you play Runner against bo at table 1."));
        assert_eq!(round_line(&info, Some(&cy)).as_deref(), Some("Round 1: you sit this one out with a bye, which counts as a win."));
        assert_eq!(round_line(&info, None).as_deref(), Some("Round 1 is being played."));
        assert_eq!(my_table(&info, Some(&bo)).map(|(index, role, _)| (index, role)), Some((0, Role::Corp)));
        // bo offers a draw: the table and ann's line say so, bo's own
        // line says it stands, and only ann may offer one now.
        info.draw_offers = vec![DrawOffer { table: 0, by: bo }];
        assert_eq!(table_line(&info, 0, &info.rounds[0].tables[0]), "Table 1 · bo (Corp) vs ann (Runner) · not played yet · bo offers a draw");
        assert_eq!(round_line(&info, Some(&ann)).as_deref(), Some("Round 1: you play Runner against bo at table 1. bo offers a draw: offer one back to agree."));
        assert_eq!(round_line(&info, Some(&bo)).as_deref(), Some("Round 1: you play Corp against ann at table 1. You have offered a draw; the table is a tie if they offer one too."));
        assert!(may_offer_draw(&info, Some(&ann)) && !may_offer_draw(&info, Some(&bo)) && !may_offer_draw(&info, Some(&cy)) && !may_offer_draw(&info, None));
        assert!(has_offered_draw(&info, Some(&bo)) && !has_offered_draw(&info, Some(&ann)));
        info.draw_offers.clear();
        info.rounds[0].tables[0].result = Some(Outcome::RunnerWon);
        assert!(!may_offer_draw(&info, Some(&ann)), "a played table takes no offer");
        assert_eq!(table_line(&info, 0, &info.rounds[0].tables[0]), "Table 1 · bo (Corp) vs ann (Runner) · ann won");
        assert_eq!(round_line(&info, Some(&bo)).as_deref(), Some("Round 1: your game against ann at table 1 is over."));
        let rows = info.standings();
        // ann's one opponent, bo, has 0 points; bo's one opponent is ann at
        // 3 per round, so bo's Strength of Schedule is 3 and ann's
        // Extended one is too.
        assert_eq!(standing_row(&info, 1, &rows[0]), "1. ann · 3 pts · 1–0–0 · SoS 0.00 · ESoS 3.00");
        assert_eq!(standing_row(&info, 2, &rows[1]), "2. cy · 3 pts · 1–0–0 (a bye) · SoS 0.00 · ESoS 0.00");
        // cy drops: still a row, paired no more, told so.
        info.dropped = vec![cy];
        assert!(is_dropped(&info, Some(&cy)) && !is_dropped(&info, Some(&ann)) && !may_drop(&info, Some(&cy)) && may_drop(&info, Some(&ann)));
        assert_eq!(standing_line(&info, Some(&cy)), "You dropped from this tournament; your results stand.");
        assert_eq!(standing_row(&info, 2, &info.standings()[1]), "2. cy · 3 pts · 1–0–0 (a bye) · SoS 0.00 · ESoS 0.00 · dropped");
        info.rounds.push(Round { tables: vec![Table { corp: ann, runner: bo, result: None }], bye: None });
        info.state = TournamentState::Playing { round: 2 };
        assert_eq!(round_line(&info, Some(&cy)).as_deref(), Some("Round 2 is being played; you have dropped."));
        info.rounds.pop();
        info.dropped.clear();
        info.state = TournamentState::Playing { round: 1 };
        // The clock, read by the client's own time.
        assert_eq!(clock_line(&info, 0), None, "no clock, no line");
        info.clock = Some(netrunner_protocol::RoundClock { began_at: 1_000, seconds: 40 * 60 });
        assert_eq!(clock_line(&info, 1_000).as_deref(), Some("Round 1: 40 minutes left."));
        assert_eq!(clock_line(&info, 1_000 + 38 * 60 + 30).as_deref(), Some("Round 1: 2 minutes left."), "ninety seconds round up");
        assert_eq!(clock_line(&info, 1_000 + 39 * 60).as_deref(), Some("Round 1: 1 minute left."));
        assert_eq!(clock_line(&info, 1_000 + 40 * 60 - 30).as_deref(), Some("Round 1: under a minute left."));
        assert!(clock_line(&info, 1_000 + 40 * 60).is_some_and(|line| line.starts_with("Round 1: time is called.")));
        info.state = TournamentState::Finished;
        assert_eq!(clock_line(&info, 1_000), None, "no round, no line");
        assert!(!may_drop(&info, Some(&ann)), "nothing to drop from once it is over");
        assert_eq!(round_line(&info, Some(&ann)), None);
        assert_eq!(standing_line(&info, Some(&ann)), "You are entered, with the two decks you committed to.");
        assert_eq!(standing_line(&info, Some(&Identity::from_secret([9; 32]).public_key())), "Not entered.");
    }
}
