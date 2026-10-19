//! The words an operator's tools put on a server (Phase 4 §12): its
//! status and a key's awards, for the terminal's `admin` commands and the
//! desktop's admin page after it, so the two say the same thing.

use netrunner_protocol::{Award, ServerStatus};

/// How long a server has run, to the largest unit that says it: "3 days",
/// "5 hours", "12 minutes", "40 seconds".
pub fn uptime(secs: u64) -> String {
    let (count, unit) = match secs {
        0..60 => (secs, "second"),
        60..3_600 => (secs / 60, "minute"),
        3_600..86_400 => (secs / 3_600, "hour"),
        _ => (secs / 86_400, "day"),
    };
    format!("{count} {unit}{}", if count == 1 { "" } else { "s" })
}

/// A server's status as lines: how long it has run and whether it keeps
/// anything, then what is on it now, then what it holds.
pub fn status_lines(status: &ServerStatus) -> Vec<String> {
    let keeps = if status.keeps { "keeps its players, ratings and results" } else { "keeps nothing between runs: no rating, tournament or event" };
    vec![
        format!("Up {}; {keeps}", uptime(status.up_secs)),
        format!("{} {} running, {} waiting for an opponent, {} connected", status.matches, plural(status.matches, "game", "games"), status.waiting, status.attached),
        format!("{} {} under way, {} {} open", status.tournaments, plural(status.tournaments, "tournament", "tournaments"), status.events, plural(status.events, "event", "events")),
        format!("{} {}", status.operators, plural(status.operators, "operator", "operators")),
    ]
}

/// A key's titles and badges as lines, each with what granted it: an
/// event's code, or an operator.
pub fn award_lines(titles: &[Award], badges: &[Award]) -> Vec<String> {
    if titles.is_empty() && badges.is_empty() {
        return vec!["no title and no badge".to_string()];
    }
    let line = |kind: &str, award: &Award| format!("{kind} {:?} ({})", award.name, source(&award.source));
    titles.iter().map(|award| line("Title", award)).chain(badges.iter().map(|award| line("Badge", award))).collect()
}

/// What an award's `source` says granted it: `event:<code>` an event,
/// `tournament:<code>` a tournament's prize, a `key:…` the operator with
/// that key.
pub fn source(source: &str) -> String {
    if let Some(code) = source.strip_prefix("tournament:") {
        return format!("tournament {code}");
    }
    match netrunner_protocol::event_code(source) {
        Some(code) => format!("event {code}"),
        None if source.starts_with("key:") => "granted by an operator".to_string(),
        None => source.to_string(),
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_reads_as_lines() {
        let status = ServerStatus { engine: "netrunner_server 0.1.0".into(), up_secs: 7_300, keeps: true, matches: 1, waiting: 2, attached: 5, tournaments: 0, events: 1, operators: 1 };
        assert_eq!(
            status_lines(&status),
            [
                "Up 2 hours; keeps its players, ratings and results",
                "1 game running, 2 waiting for an opponent, 5 connected",
                "0 tournaments under way, 1 event open",
                "1 operator",
            ]
        );
        assert_eq!(uptime(59), "59 seconds");
        assert_eq!(uptime(60), "1 minute");
        assert_eq!(uptime(86_400 * 3 + 5), "3 days");
    }

    #[test]
    fn an_award_names_what_granted_it() {
        let award = |name: &str, source: &str| Award { name: name.into(), earned_at: 1, source: source.into() };
        assert_eq!(
            award_lines(&[award("Net-Slinger", "event:K7M2QX")], &[award("founder", "key:abc")]),
            ["Title \"Net-Slinger\" (event K7M2QX)", "Badge \"founder\" (granted by an operator)"]
        );
        assert_eq!(award_lines(&[], &[]), ["no title and no badge"]);
        assert_eq!(source("tournament:K7M2QX"), "tournament K7M2QX");
    }
}
