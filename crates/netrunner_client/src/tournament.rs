//! The words both clients put on a tournament (Phase 4 §7 stage 6b): how
//! a tournament is named in a list, what its state is called, and which
//! of its entrants is this player — so the terminal's rows and the
//! desktop's buttons say the same thing and neither spells it twice.
//!
//! **No I/O and no rules.** Everything here reads a `TournamentInfo` the
//! server sent; the server decides who may register, whose key an entry
//! is under and what a state means. A client only shows it.

use netrunner_identity::PublicKey;
use netrunner_protocol::{Entrant, TournamentInfo, TournamentState};

use crate::settings::format_label;

/// A tournament's state in a phrase, for its row and its page.
/// Exhaustive, so the rounds' stages add their words here.
pub fn state_label(state: TournamentState) -> &'static str {
    match state {
        TournamentState::Registering => "taking registrations",
    }
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
        TournamentInfo { id: "K7M2QX".into(), name: "Friday Night".into(), format: NsgFormat::Startup, organizer: Identity::from_secret([1; 32]).public_key(), state: TournamentState::Registering, entrants }
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
}
