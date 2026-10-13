//! The words both clients put on an event (Phase 4 §11): how one is
//! named in a list, how its rule and its track read, and what a
//! milestone grants — so the terminal's rows and the desktop's page say
//! the same thing and neither spells it twice.
//!
//! **No I/O and no rules.** Everything here reads an `EventInfo` the
//! server sent; the server decides who may enter, what a win reaches and
//! what is granted. A client only shows it.

use netrunner_core::decks::FactionRule;
use netrunner_core::rules::Side;
use netrunner_protocol::{AwardKind, EventInfo, Milestone};

use crate::settings::format_label;

/// An event as a list names it: its name, format, rule, track and how
/// many are in it — "The Red Planet Gauntlet · Startup · 3 wins to clear
/// · 12 entered, 2 looking" — and this key's own track when it has one.
/// The code is not here: it is the page's heading.
pub fn line(info: &EventInfo) -> String {
    let mut parts = vec![info.name.clone(), format_label(info.format).to_string()];
    if let Some(rule) = &info.rule {
        parts.push(rule_line(rule));
    }
    parts.push(wins_line(info.wins_to_clear));
    parts.push(count_line(info));
    if let Some(mine) = &info.mine {
        parts.push(format!("you: {}", track_line(mine.wins, mine.losses)));
    }
    if !info.open {
        parts.push("closed".to_string());
    }
    parts.join(" · ")
}

/// "3 wins to clear".
pub fn wins_line(wins: u32) -> String {
    match wins {
        1 => "1 win to clear".to_string(),
        n => format!("{n} wins to clear"),
    }
}

/// "12 entered, 2 looking", or "nobody yet".
pub fn count_line(info: &EventInfo) -> String {
    match (info.entered, info.looking) {
        (0, _) => "nobody yet".to_string(),
        (n, 0) => format!("{n} entered"),
        (n, looking) => format!("{n} entered, {looking} looking"),
    }
}

/// "2 wins, 1 loss".
pub fn track_line(wins: u32, losses: u32) -> String {
    let plural = |n: u32, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
    format!("{}, {}", plural(wins, "win", "wins"), plural(losses, "loss", "losses"))
}

/// The rule in words: "Weyland Consortium or Jinteki Corps, any Runner".
pub fn rule_line(rule: &FactionRule) -> String {
    let side = |side: Side| -> String {
        let (word, any) = match side {
            Side::Corp => ("Corps", "any Corp"),
            Side::Runner => ("Runners", "any Runner"),
        };
        match rule.allowed(side) {
            None => any.to_string(),
            Some(factions) => {
                let names: Vec<&str> = factions.iter().map(|faction| faction.name()).collect();
                match names.as_slice() {
                    [one] => format!("{one} {word}"),
                    [first @ .., last] => format!("{} or {last} {word}", first.join(", ")),
                    [] => any.to_string(),
                }
            }
        }
    };
    format!("{}, {}", side(Side::Corp), side(Side::Runner))
}

/// What a milestone grants: `title "Net-Slinger"` or `badge red-planet`.
pub fn award_label(milestone: &Milestone) -> String {
    match milestone.award.kind {
        AwardKind::Title => format!("title \"{}\"", milestone.award.name),
        AwardKind::Badge => format!("badge {}", milestone.award.name),
    }
}

/// Where this key stands on the track, in a phrase for the page: not
/// entered, climbing, or cleared.
pub fn standing_line(info: &EventInfo) -> String {
    match &info.mine {
        None if !info.open => "This event is closed.".to_string(),
        None => "You have not entered.".to_string(),
        Some(mine) => match mine.cleared_at {
            Some(_) => format!("Cleared · {}", track_line(mine.wins, mine.losses)),
            None => {
                let left = info.wins_to_clear.saturating_sub(mine.wins);
                format!("{} · {}", track_line(mine.wins, mine.losses), match left {
                    1 => "one more win clears it".to_string(),
                    n => format!("{n} more wins clear it"),
                })
            }
        },
    }
}

/// A milestone's line on the page, lit or not by this key's track:
/// "Win 1 · title \"Net-Slinger\" · earned" / "· one more win".
pub fn milestone_line(info: &EventInfo, milestone: &Milestone) -> String {
    let wins = info.mine.as_ref().map_or(0, |mine| mine.wins);
    let state = if wins >= milestone.wins {
        "earned".to_string()
    } else {
        match milestone.wins - wins {
            1 => "one more win".to_string(),
            n => format!("{n} more wins"),
        }
    };
    format!("Win {} · {} · {state}", milestone.wins, award_label(milestone))
}

/// Whether this key's track has reached `milestone`.
pub fn reached(info: &EventInfo, milestone: &Milestone) -> bool {
    info.mine.as_ref().is_some_and(|mine| mine.wins >= milestone.wins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::card::Faction;
    use netrunner_core::format::NsgFormat;
    use netrunner_protocol::{AwardSpec, EventEntry};

    fn gauntlet(mine: Option<EventEntry>) -> EventInfo {
        EventInfo {
            id: "ABC123".into(),
            name: "The Red Planet Gauntlet".into(),
            format: NsgFormat::Startup,
            rule: Some(FactionRule { corp: vec![Faction::WeylandConsortium, Faction::Jinteki], runner: Vec::new() }),
            wins_to_clear: 3,
            milestones: vec![Milestone { wins: 1, award: AwardSpec { kind: AwardKind::Title, name: "Net-Slinger".into() } }, Milestone { wins: 3, award: AwardSpec { kind: AwardKind::Badge, name: "red-planet".into() } }],
            open: true,
            created_at: 1,
            entered: 12,
            looking: 2,
            mine,
        }
    }

    #[test]
    fn the_words_on_an_event() {
        let fresh = gauntlet(None);
        assert_eq!(line(&fresh), "The Red Planet Gauntlet · Startup · Weyland Consortium or Jinteki Corps, any Runner · 3 wins to clear · 12 entered, 2 looking");
        assert_eq!(standing_line(&fresh), "You have not entered.");
        let climbing = gauntlet(Some(EventEntry { wins: 2, losses: 1, entered_at: 5, cleared_at: None }));
        assert!(line(&climbing).ends_with("· you: 2 wins, 1 loss"));
        assert_eq!(standing_line(&climbing), "2 wins, 1 loss · one more win clears it");
        assert_eq!(milestone_line(&climbing, &climbing.milestones[0]), "Win 1 · title \"Net-Slinger\" · earned");
        assert_eq!(milestone_line(&climbing, &climbing.milestones[1]), "Win 3 · badge red-planet · one more win");
        let cleared = gauntlet(Some(EventEntry { wins: 3, losses: 0, entered_at: 5, cleared_at: Some(9) }));
        assert_eq!(standing_line(&cleared), "Cleared · 3 wins, 0 losses");
        assert!(reached(&cleared, &cleared.milestones[1]));
        let mut closed = gauntlet(None);
        closed.open = false;
        closed.entered = 0;
        assert!(line(&closed).ends_with("· nobody yet · closed"));
        assert_eq!(standing_line(&closed), "This event is closed.");
    }
}
