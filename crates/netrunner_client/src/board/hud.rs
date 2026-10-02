//! The numbers a player watches, for a HUD: credits, clicks, agenda
//! points, hand size and the side's own threats, in one fixed order.
//!
//! **Every readout is always there, at zero too.** A HUD is read by
//! where a number sits, not by its label: a Tags readout that appeared
//! with the first tag would move every number after it at the moment the
//! person most needs to find them. So each side has a fixed set — the
//! three both sides share, in the same slots for both, then the side's
//! own — and a threat that is live is *marked* ([`Readout::alarm`])
//! rather than added.
//!
//! The person asked for this after their first desktop game (Phase 7 §4
//! item 6): the same numbers were in the strip as sentences, and
//! "Credits 5 · Clicks 3 · Agenda points 2/7" is a line to read, not a
//! place to glance. Everything here is a public counter on the masked
//! `ClientView`, so a HUD built from it shows nothing the mask withheld.
//!
//! **The Agendas readout is a door.** A press opens the side's score area
//! ([`score_area`]) as a list the person reads down and opens one row of:
//! the title and points on the row, the card, its facts and its printed
//! text under it. It replaced the strip's "Stolen: …" line, which grew
//! with the game in a strip whose height is fixed.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{InstallId, Side};
use netrunner_core::view::ClientView;

use super::Pile;

/// One number on the HUD and the word under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readout {
    /// The short word that fits under a large number: "Credits", "Tags".
    pub label: &'static str,
    /// The number as drawn: `5`, or `2/7` for agenda points.
    pub value: String,
    /// A threat that is live — a tag, core damage, bad publicity — or an
    /// agenda-point total one agenda from winning: drawn so it is seen.
    pub alarm: bool,
    /// The zone a click on the readout opens, if it is one: the Agendas
    /// readout opens the side's score area.
    pub opens: Option<Pile>,
}

/// How many readouts a HUD row holds before it wraps: all of either
/// side's, so a strip is one row of numbers no taller than a hand's peek,
/// and both sides' shared readouts sit in the same columns.
pub const PER_ROW: usize = 6;

/// A side's readouts, in their fixed order: Credits, Clicks, Agendas, then
/// the side's own — Bad publicity for the Corp; the Grip, Tags and Core
/// damage for the Runner. Always the same length for a side.
///
/// **The Corp has no hand readout.** Its HQ is a server column whose
/// header carries the count ("HQ · 4"), so an "HQ" readout said the same
/// number twice, as the R&D and Archives line `details` dropped did. The
/// Runner's grip has no column, so its count stays.
/// The agenda points `side` needs to win, as the engine asks it
/// (`continuous::points_to_win`): the match's threshold less what a card
/// takes off it (Issuaq Adaptics: Sustaining Diversity's power counters).
/// Every "to win" a client prints reads this, never the match rule.
pub fn points_to_win(view: &ClientView, side: Side) -> i32 {
    match side {
        Side::Corp => view.corp.points_to_win,
        Side::Runner => view.runner.points_to_win,
    }
}

pub fn readouts(view: &ClientView, side: Side) -> Vec<Readout> {
    let quiet = |label, value: String| Readout { label, value, alarm: false, opens: None };
    let threat = |label, n: u32| Readout { label, value: n.to_string(), alarm: n > 0, opens: None };
    // Two points short is one agenda from the win in every sample deck
    // but a three-pointer's; close enough to be worth the colour.
    // Named for what a click opens, not for the number: the points *are*
    // the agendas, and "Points" did not say there was a pile behind it.
    // Signed: Word on the Street can put a score below 0.
    let points = |side, points: i32| {
        let to_win = points_to_win(view, side);
        Readout { label: "Agendas", value: format!("{points}/{to_win}"), alarm: points + 2 >= to_win && points > 0, opens: Some(Pile::Agendas(side)) }
    };
    match side {
        Side::Corp => {
            let corp = &view.corp;
            vec![
                quiet("Credits", corp.credits.to_string()),
                quiet("Clicks", corp.clicks.to_string()),
                points(Side::Corp, corp.agenda_points),
                threat("Bad pub.", corp.bad_publicity),
            ]
        }
        Side::Runner => {
            let runner = &view.runner;
            // A run's own credits — the bad publicity fund (CR 6.9.1b, kept
            // apart from the credit pool) and a run event's (Overclock) —
            // beside the pool as "5 +2": gone when the run ends, so
            // neither the pool nor the sum is the number the person needs.
            let for_the_run = view.active_run.as_ref().map_or(0, |run| run.bad_publicity_credits + run.bonus_run_credits);
            let credits = match for_the_run {
                0 => runner.credits.to_string(),
                n => format!("{} +{n}", runner.credits),
            };
            vec![
                quiet("Credits", credits),
                quiet("Clicks", runner.clicks.to_string()),
                points(Side::Runner, runner.agenda_points),
                quiet("Grip", runner.grip_count.to_string()),
                threat("Tags", runner.tags),
                threat("Core damage", u32::try_from(runner.brain_damage).unwrap_or(u32::MAX)),
            ]
        }
    }
}

/// The smaller line under the readouts: the counts a player looks up
/// rather than watches, and only those the table does not already show.
/// The Corp has none — R&D and Archives are server columns whose headers
/// carry their counts ("R&D · 31"), and the line repeating them was
/// dropped as redundant. The Runner's stack and heap are not here either:
/// they are the pile buttons beside it, which carry their counts.
pub fn details(view: &ClientView, side: Side) -> Option<String> {
    match side {
        Side::Corp => None,
        Side::Runner => Some(format!("MU {} · Link {}", view.runner.memory_units, view.runner.link_strength)),
    }
}

/// Which side of a side's identity is up, for an identity that has more
/// than one: Dewi Subrotoputri and Nebula Talent Management flip, and
/// Méliès U: Only the Brightest has three reverse sides, one set in secret
/// at the end of each of the Corp's discard phases (CR 1.5.2b). Only the
/// faceup side is active (CR 3.1.1a), so which one is up is what the
/// identity *does* right now — and the view carried it from the day the
/// first flip identity played, with neither client ever drawing it.
///
/// Read off the masked view: which copy of Méliès U is in play is
/// `CorpClientView::identity_copy`, the Corp's own to see and the Runner's
/// once it has been turned over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentitySide {
    pub flipped: bool,
    /// Which copy is in play, where the identity has several and the
    /// viewer may know: 1 to 3 for Méliès U.
    pub copy: Option<u8>,
    /// The identity picks its reverse side in secret (Méliès U) and, as
    /// far as the viewer can tell, has picked one: the Corp knows none is
    /// set before its first discard phase ends, and the Runner, who never
    /// sees the number, is first asked anything after it has.
    pub secret: bool,
}

impl IdentitySide {
    /// The side up, in a word or two: what fits on the avatar.
    pub fn chip(&self) -> String {
        match (self.flipped, self.copy) {
            (true, Some(copy)) => format!("Side {copy}"),
            (true, None) => "Flipped".to_string(),
            (false, _) => "Front".to_string(),
        }
    }

    /// The side up, as a sheet or the terminal's board says it.
    pub fn line(&self) -> String {
        match (self.flipped, self.copy, self.secret) {
            (true, Some(copy), _) => format!("Side {copy} is up"),
            (true, None, _) => "Its flip side is up".to_string(),
            (false, Some(copy), true) => format!("Its front is up · side {copy} is set, in secret"),
            (false, None, true) => "Its front is up · the side under it is set in secret".to_string(),
            (false, _, _) => "Its front is up".to_string(),
        }
    }
}

/// [`IdentitySide`] for `side`'s identity, or `None` for an identity with
/// one side — told by its text, which is the only place that says it can
/// flip (`Effect::FlipIdentity`).
pub fn identity_side(view: &ClientView, side: Side, registry: &CardRegistry) -> Option<IdentitySide> {
    let identity = match side {
        Side::Corp => view.corp.identity.as_ref(),
        Side::Runner => view.runner.identity.as_ref(),
    }?;
    let card = registry.get(identity)?;
    let (mut flips, mut secret) = (false, false);
    let mut look = |effect: &netrunner_core::dsl::Effect| {
        flips |= matches!(effect, netrunner_core::dsl::Effect::FlipIdentity);
        secret |= matches!(effect, netrunner_core::dsl::Effect::SetIdentityCopy(_));
    };
    for trigger in &card.triggers {
        for effect in &trigger.effects {
            effect.for_each_effect(&mut look);
        }
    }
    for ability in &card.abilities {
        ability.effect.for_each_effect(&mut look);
    }
    if !flips {
        return None;
    }
    let (flipped, copy) = match side {
        // 0 is "not set yet": nothing is in play but the front.
        Side::Corp => (view.corp.identity_flipped, view.corp.identity_copy.filter(|copy| *copy > 0)),
        Side::Runner => (view.runner.identity_flipped, None),
    };
    let secret = secret && !(side == Side::Corp && view.corp.identity_copy == Some(0));
    Some(IdentitySide { flipped, copy: copy.filter(|_| secret), secret })
}

/// What an identity holds, apart from its printed text: the side up, for
/// one that flips ([`IdentitySide::line`]); its recurring credits left
/// (NBN: Making News, "2 of 2 recurring credits"); otherwise its counters
/// (AU Co.'s power counters, which its own ability spends). Both clients
/// list these — the desktop on the identity's sheet, the terminal on its
/// identity line — and they were drawn nowhere before the client ledger
/// named them. Empty for an identity with nothing on it.
pub fn identity_facts(view: &ClientView, side: Side, registry: &CardRegistry) -> Vec<String> {
    let mut facts: Vec<String> = identity_side(view, side, registry).map(|face| face.line()).into_iter().collect();
    if side == Side::Corp {
        let corp = &view.corp;
        let kind = corp.identity.as_ref().and_then(|id| registry.get(id)).and_then(|card| card.counter_kind);
        // A recurring credit is the identity's counter too (CR 1.10.5):
        // said once, as what it is.
        if corp.recurring_credits_max > 0 {
            facts.push(format!("{} of {} recurring credit{} left", corp.recurring_credits, corp.recurring_credits_max, if corp.recurring_credits_max == 1 { "" } else { "s" }));
        } else if corp.identity_counters > 0 {
            facts.push(super::facts::counter_word(kind, corp.identity_counters));
        }
    }
    facts
}

/// The words on the avatar's chip: the side up for a flip identity, else
/// what it holds in a few words ("2 credits", "3 power counters"); `None`
/// when there is nothing to say.
pub fn identity_chip(view: &ClientView, side: Side, registry: &CardRegistry) -> Option<String> {
    if let Some(face) = identity_side(view, side, registry) {
        return Some(face.chip());
    }
    let corp = &view.corp;
    if side != Side::Corp {
        return None;
    }
    if corp.recurring_credits_max > 0 {
        return Some(format!("{} of {}", corp.recurring_credits, corp.recurring_credits_max));
    }
    let kind = corp.identity.as_ref().and_then(|id| registry.get(id)).and_then(|card| card.counter_kind);
    (corp.identity_counters > 0).then(|| super::facts::counter_word(kind, corp.identity_counters))
}

/// The cards `side` has removed from the game, in the order they left:
/// Petty Cash played out of Archives, a forfeited agenda and Spin Doctor
/// on the Corp's side; an event that says "remove this event from the
/// game" on the Runner's (the Runner's pile arrived with Vantage Point).
/// Public, and a zone neither client had drawn.
pub fn removed_from_game(view: &ClientView, side: Side) -> &[CardId] {
    match side {
        Side::Corp => &view.corp.removed_from_game,
        Side::Runner => &view.runner.removed_from_game,
    }
}

/// What is in effect for a while, one line each with the card that made
/// it: "Aircheck: the Runner cannot spend or lose credits from their
/// credit pool, for the rest of this run". Words, not the printed
/// symbols, so the terminal reads it as the desktop does. Read off the view's lingering
/// effects, which hold only while their duration runs.
///
/// **A strength is left out**, because the number it changes is already
/// on the card it is about (a breaker's chip, the encountered ice's
/// panel) and a boost bought three times would be three lines. The rest
/// had no place at all: Vantage Point's credit-pool lock and its changes
/// to next turn's clicks, a score lock, a rez-cost tax on each piece of
/// ice, Shred's hold on the run's end — each a rule in force that the
/// person could see only by remembering the card. First, when a run is
/// announced as going elsewhere (Maintenance Access), where it will go:
/// the run carries the destination and not the card, so that line is the
/// run's ("This run: …").
/// A prohibition in force, as the In effect list says it: the same words
/// whether a card made it for a duration or a card's standing effect has it
/// on right now.
fn cannot_words(what: netrunner_core::dsl::Prohibition) -> &'static str {
    use netrunner_core::dsl::Prohibition;
    match what {
        Prohibition::StealOrTrash => "the Runner cannot steal or trash cards",
        Prohibition::ScoreAgendas => "the Corp cannot score agendas",
        Prohibition::SpendOrLoseCreditPool => "the Runner cannot spend or lose credits from their credit pool",
        Prohibition::SpendCredits => "the Runner cannot spend credits",
        Prohibition::EndTheRun => "subroutines cannot end the run",
        Prohibition::RunOnRemote => "the Runner cannot run on a remote server",
        Prohibition::AccessOthers => "the Runner cannot access any other card",
        Prohibition::Access => "the Runner cannot access that card",
        Prohibition::BreakSubroutines => "the Runner's abilities cannot break subroutines",
        Prohibition::DiscardStep => "the Corp skips their discard step",
    }
}

pub fn in_effect(view: &ClientView, registry: &CardRegistry) -> Vec<String> {
    use netrunner_core::dsl::{EndRunPrevention, Prohibition};
    use netrunner_core::rules::lingering::{Lingering, On, Until};
    let title = |id: &CardId| registry.get(id).map_or_else(|| id.0.replace('_', " "), |card| card.title.clone());
    let who = |on: &On, fallback: Side| match on {
        On::Player(side) => *side,
        _ => fallback,
    };
    // Maintenance Access: the run it started is announced as going
    // elsewhere once it reaches Archives (`redirect_on_approach`).
    let redirect = view.active_run.as_ref().and_then(|run| run.redirect_on_approach.map(|to| (run.server, to))).map(|(from, to)| {
        format!("This run: when the Runner would approach {}, the attacked server becomes {} instead", super::action_map::server_name(from), super::action_map::server_name(to))
    });
    // Spree: counters on the event in the play area, which has no place
    // on the board of its own (`event_counters`).
    let event_counters = view.active_run.as_ref().filter(|run| run.event_counters > 0).map(|run| {
        let event = run.initiated_by.as_ref().map_or_else(|| "the run's event".to_string(), title);
        format!("This run: {event} has {} power counter{}", run.event_counters, if run.event_counters == 1 { "" } else { "s" })
    });
    // Thunderbolt Armaments: a subroutine a piece of ice has for the rest
    // of the run (`gained_for_the_run`), which its card does not print.
    let gained_for_the_run: Vec<String> = view.active_run.iter().flat_map(|run| &run.gained_for_the_run).map(|gained| {
        let ice = super::facts::card_of(view, gained.ice).map_or_else(|| "a piece of ice".to_string(), |card| title(&card));
        let order = if gained.after { "after" } else { "before" };
        format!("This run: {ice} has \u{201c}{}\u{201d} {order} its other subroutines", gained.subroutine.text.trim_end_matches('.'))
    }).collect();
    // Stegodon MK IV: its −2 is already in every icebreaker's strength,
    // and this is why (`ice_derezzed`).
    let derezzed = view.active_run.as_ref().filter(|run| run.ice_derezzed).map(|_| "This run: a piece of ice has been derezzed".to_string());
    // Bahia Bands: the run's credits beside the pool ("5 +4") pay for one
    // thing only (`run_credits_pay_for`), which the sum cannot say.
    let run_credits_for = view.active_run.as_ref().filter(|run| run.bonus_run_credits > 0).and_then(|run| {
        let word = run.run_credits_pay_for.as_ref()?;
        let event = run.initiated_by.as_ref().map_or_else(|| "the run's event".to_string(), title);
        Some(format!("This run: the {} [credit] on {event} may be spent only {}", run.bonus_run_credits, crate::prose::describe_pays_for(word)))
    });
    // Attini: a prohibition a card's standing effect has in force right
    // now (`standing_cannot`) — why an offer to pay has no Accept.
    let standing = view.standing_cannot.iter().map(|standing| format!("{}: {}", title(&standing.source), cannot_words(standing.what)));
    redirect.into_iter().chain(event_counters).chain(gained_for_the_run).chain(derezzed).chain(run_credits_for).chain(standing).chain(view.lingering
        .iter()
        .filter_map(|effect| {
            let what = match (&effect.what, &effect.on) {
                (Lingering::Strength(_), _) => return None,
                // One line per piece of ice for every type it gained, below.
                (Lingering::GainSubtype(_), _) => return None,
                (Lingering::RezCost(n), _) => {
                    let credits = n.unsigned_abs();
                    format!("each piece of ice costs {credits} credit{} {} to rez", if credits == 1 { "" } else { "s" }, if *n >= 0 { "more" } else { "less" })
                }
                (Lingering::Cannot(what), On::CopiesOf(card)) => match what {
                    Prohibition::StealOrTrash => format!("the Runner cannot steal or trash copies of {}", title(card)),
                    Prohibition::ScoreAgendas => format!("the Corp cannot score copies of {}", title(card)),
                    what => cannot_words(*what).to_string(),
                },
                (Lingering::Cannot(Prohibition::ScoreAgendas), On::Install(_)) => "the Corp cannot score the card it installed".to_string(),
                // Banner: about the ice being encountered.
                (Lingering::Cannot(Prohibition::EndTheRun), On::Install(ice)) => {
                    let ice = super::facts::card_of(view, *ice).map_or_else(|| "the ice".to_string(), |card| title(&card));
                    format!("subroutines on {ice} cannot end the run")
                }
                // Adrian Seis: about the upgrade that played the psi game.
                (Lingering::Cannot(Prohibition::AccessOthers), On::Install(install)) => {
                    let card = super::facts::card_of(view, *install).map_or_else(|| "the upgrade".to_string(), |card| title(&card));
                    format!("the Runner cannot access cards other than {card}")
                }
                (Lingering::Cannot(Prohibition::Access), On::Install(install)) => {
                    let card = super::facts::card_of(view, *install).map_or_else(|| "the upgrade".to_string(), |card| title(&card));
                    format!("the Runner cannot access {card}")
                }
                // Hafrún: about the Runner card it chose.
                (Lingering::Cannot(Prohibition::BreakSubroutines), On::Install(install)) => {
                    let card = super::facts::card_of(view, *install).map_or_else(|| "that card".to_string(), |card| title(&card));
                    format!("{card}'s abilities cannot break subroutines")
                }
                (Lingering::Cannot(what), _) => cannot_words(*what).to_string(),
                // Klevetnik: the resource it chose.
                (Lingering::LosesAbilities, on) => {
                    let card = match on {
                        On::Install(install) => super::facts::card_of(view, *install).map_or_else(|| "that card".to_string(), |card| title(&card)),
                        _ => "that card".to_string(),
                    };
                    format!("{card} has lost all its abilities")
                }
                // Anvil, Unsmiling Tsarevna: about the ice itself.
                (Lingering::BreakLimit(at_most), on) => {
                    let ice = match on {
                        On::Install(install) => super::facts::card_of(view, *install).map_or_else(|| "the ice".to_string(), |card| title(&card)),
                        _ => "the ice".to_string(),
                    };
                    match at_most {
                        0 => format!("the Runner cannot break {ice}'s printed subroutines"),
                        n => format!("the Runner cannot break more than {n} of {ice}'s printed subroutines each encounter"),
                    }
                }
                // Tsakhia: the server it chose, and the encounter it spent it on.
                (Lingering::ChosenServer(server), _) => format!("the chosen server is {}", crate::board::action_map::server_name(*server)),
                (Lingering::SubroutinesReplaced, on) => {
                    let ice = match on {
                        On::Install(install) => super::facts::card_of(view, *install).map_or_else(|| "the ice".to_string(), |card| title(&card)),
                        _ => "the ice".to_string(),
                    };
                    let instead = registry.get(&effect.source).and_then(|card| card.subroutines.first()).map_or_else(String::new, |sub| sub.text.trim_end_matches('.').to_string());
                    format!("each subroutine on {ice} resolves as \u{201c}{instead}\u{201d}")
                }
                (Lingering::PreventRunEnding(EndRunPrevention::UnlessCorpTrashesRootCountFromHq), _) => {
                    "the first time the Corp would end the run, it ends only if the Corp trashes a card from HQ for each card in the server's root".to_string()
                }
                (Lingering::Mark(server), _) => format!("the Runner's mark is {}", crate::board::action_map::server_name(*server)),
                (Lingering::AllottedClicks(n), on) => {
                    let side = who(on, Side::Runner);
                    let clicks = n.unsigned_abs();
                    format!("the {side:?} has {clicks} {} allotted click{} next turn", if *n >= 0 { "more" } else { "fewer" }, if clicks == 1 { "" } else { "s" })
                }
            };
            let until = match effect.until {
                Until::EndOfEncounter(_) => ", for this encounter",
                Until::EndOfRun => ", for the rest of this run",
                Until::EndOfTurn(turn) if turn <= view.turn => ", for the rest of this turn",
                // Klevetnik's "until your next turn ends": the turn of the
                // player whose card it is.
                Until::EndOfTurn(_) => match registry.get(&effect.source).map(|card| card.side) {
                    Some(Side::Runner) => ", until the end of the Runner's next turn",
                    _ => ", until the end of the Corp's next turn",
                },
                Until::NextTurnOf(_) => "",
                Until::WhileRezzed(_) => ", while it remains rezzed",
            };
            Some(format!("{}: {what}{until}", title(&effect.source)))
        }))
        // Lycian Multi-Munition: the subtypes it was rezzed as, one line for
        // the piece of ice rather than one per subtype.
        .chain({
            let mut gained: Vec<(InstallId, CardId)> = Vec::new();
            for effect in &view.lingering {
                if let (Lingering::GainSubtype(_), On::Install(install)) = (&effect.what, &effect.on)
                    && !gained.iter().any(|(seen, _)| seen == install)
                {
                    gained.push((*install, effect.source.clone()));
                }
            }
            gained.into_iter().map(|(install, source)| {
                format!("{}: it is a {} while it remains rezzed", title(&source), super::facts::ice_type_words(&super::facts::gained_ice_types(view, install)))
            })
        })
        // A delayed ability (Lightning Laboratory's derez): what it will do,
        // in the words the card inspector uses, since it is a sentence of
        // the card's that has not resolved yet.
        .chain(view.delayed.iter().map(|delayed| {
            format!("{}: when this turn ends, {}", title(&delayed.card), crate::prose::describe_effect(&delayed.effect, registry))
        }))
        // A card revealed in a hand and not moved yet (Burner's three out
        // of HQ while the Runner chooses): both players were shown it.
        .chain(view.revealed.iter().map(|revealed| {
            format!("revealed in {}: {}", if revealed.side == Side::Corp { "HQ" } else { "the grip" }, title(&revealed.card))
        }))
        // The set-aside zone (The Wizard's Chest's cards while the Runner
        // chooses one): faceup, so both players read it.
        .chain((!view.runner.set_aside.is_empty()).then(|| {
            let names: Vec<String> = view.runner.set_aside.iter().map(&title).collect();
            format!("set aside: {}", names.join(", "))
        }))
        .collect()
}

/// One agenda in a side's score area, as its list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredCard {
    pub card: CardId,
    pub title: String,
    /// Signed: a card added "as an agenda" can be worth −1.
    pub points: i32,
    /// Added "as an agenda" rather than scored or stolen (CR 10.1.3), and
    /// whether it may be forfeited — Myōshu, Word on the Street.
    pub as_agenda: Option<netrunner_core::dsl::AsAgenda>,
    /// The handle the agenda kept from its install, when the side scored
    /// it: a scored agenda's ability is activated by it. A stolen agenda
    /// has none.
    pub install: Option<InstallId>,
    /// Agenda counters on it (Dividends, spent by its own abilities).
    pub counters: u32,
}

impl ScoredCard {
    /// The row's one line: the title and what it is worth.
    pub fn line(&self) -> String {
        format!("{} · {} point{}", self.title, self.points, if self.points == 1 { "" } else { "s" })
    }

    /// What its expanded row lists above the printed text: the points,
    /// the advancement it needed and who took it, and any counters.
    pub fn facts(&self, side: Side, registry: &CardRegistry) -> Vec<String> {
        let mut lines = Vec::new();
        let def = registry.get(&self.card);
        let how = match (side, self.as_agenda) {
            // Jeitinho: "as an assassination agenda", the subtype it has.
            (_, Some(netrunner_core::dsl::AsAgenda { subtype: Some(subtype), .. })) => format!("Added as {}", crate::prose::a_subtype_agenda(&subtype)),
            (_, Some(_)) => "Added as an agenda".to_string(),
            (Side::Corp, None) => "Scored by the Corp".to_string(),
            (Side::Runner, None) => "Stolen by the Runner".to_string(),
        };
        match def.and_then(|d| d.advancement_requirement).filter(|_| self.as_agenda.is_none()) {
            Some(need) => lines.push(format!("{how} · {} point{} · advancement requirement {need}", self.points, if self.points == 1 { "" } else { "s" })),
            None => lines.push(format!("{how} · {} point{}", self.points, if self.points == 1 { "" } else { "s" })),
        }
        // What the table makes of it, when that is not what it prints:
        // Let Them Dream in the Runner's score area.
        if let Some(printed) = def.and_then(|d| d.agenda_points).filter(|printed| self.as_agenda.is_none() && *printed as i32 != self.points) {
            lines.push(format!("Prints {printed} point{}; its text changes that here", if printed == 1 { "" } else { "s" }));
        }
        if self.as_agenda.is_some_and(|as_agenda| as_agenda.cannot_forfeit) {
            lines.push("Cannot be forfeited".to_string());
        }
        if self.counters > 0 {
            lines.push(format!("{} agenda counter{}", self.counters, if self.counters == 1 { "" } else { "s" }));
        }
        lines
    }
}

/// A side's score area, in the order the agendas arrived, each worth what
/// the engine counts it as now (`scored_worth`, which the view carries
/// beside the list): Let Them Dream stolen is worth 1 less than it prints,
/// and while the list read the printed value its rows summed to more than
/// the total over them.
pub fn score_area(view: &ClientView, side: Side, registry: &CardRegistry) -> Vec<ScoredCard> {
    let entry = |card: &CardId, worth: Option<i32>, install: Option<InstallId>, counters: u32, as_agenda: Option<netrunner_core::dsl::AsAgenda>| {
        let def = registry.get(card);
        ScoredCard {
            card: card.clone(),
            title: def.map_or_else(|| card.0.replace('_', " "), |d| d.title.clone()),
            // A card added as an agenda is worth what the addition said,
            // never what it prints (CR 10.1.3); the view's number says so
            // already, and the fallback, for a view built by hand, too.
            points: worth.unwrap_or_else(|| as_agenda.map_or_else(|| def.and_then(|d| d.agenda_points).unwrap_or(0) as i32, |as_agenda| as_agenda.points)),
            as_agenda,
            install,
            counters,
        }
    };
    match side {
        Side::Corp => view
            .corp
            .scored_agendas
            .iter()
            .enumerate()
            .map(|(i, a)| entry(&a.card, view.corp.scored_worth.get(i).copied(), Some(a.install_id), a.agenda_counters, a.as_agenda))
            .collect(),
        // A stolen agenda's handle is the Corp's way to its ability there
        // (Oracle Thinktank); a card added as an agenda has none to use.
        Side::Runner => view
            .runner
            .scored_agendas
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let install = (a.install_id != InstallId::PLACEHOLDER && a.as_agenda.is_none()).then_some(a.install_id);
                entry(&a.card, view.runner.scored_worth.get(i).copied(), install, 0, a.as_agenda)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::GameState;
    use netrunner_session::{sweep_decks_for_seed, Seat, Session};

    fn view() -> ClientView {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        Session::new(state, registry, Seat::External, Seat::External).view_for(Side::Corp)
    }

    #[test]
    fn the_shared_readouts_sit_in_the_same_slots_for_both_sides() {
        let view = view();
        let corp: Vec<_> = readouts(&view, Side::Corp).iter().map(|r| r.label).collect();
        let runner: Vec<_> = readouts(&view, Side::Runner).iter().map(|r| r.label).collect();
        assert_eq!(corp[..3], runner[..3]);
        assert_eq!(corp, ["Credits", "Clicks", "Agendas", "Bad pub."]);
        assert_eq!(runner, ["Credits", "Clicks", "Agendas", "Grip", "Tags", "Core damage"]);
        assert_eq!(readouts(&view, Side::Corp)[2].opens, Some(Pile::Agendas(Side::Corp)));
        assert_eq!(readouts(&view, Side::Runner)[2].opens, Some(Pile::Agendas(Side::Runner)));
        assert!(readouts(&view, Side::Runner).iter().filter(|r| r.opens.is_some()).count() == 1, "only the Agendas readout opens anything");
    }

    #[test]
    fn a_threat_is_always_shown_and_marked_only_when_live() {
        let mut view = view();
        let quiet = readouts(&view, Side::Runner);
        assert_eq!(quiet[4].value, "0");
        assert!(!quiet[4].alarm, "no tags is not an alarm");
        view.runner.tags = 2;
        view.runner.brain_damage = 1;
        let hot = readouts(&view, Side::Runner);
        assert_eq!(hot.len(), quiet.len(), "a live threat marks its slot rather than adding one");
        assert!(hot[4].alarm && hot[4].value == "2");
        assert!(hot[5].alarm && hot[5].value == "1");
    }

    #[test]
    fn points_read_against_the_total_and_warn_one_agenda_out() {
        let mut view = view();
        let to_win = view.corp.points_to_win;
        assert_eq!(readouts(&view, Side::Corp)[2].value, format!("0/{to_win}"));
        assert!(!readouts(&view, Side::Corp)[2].alarm);
        view.corp.agenda_points = to_win - 2;
        assert!(readouts(&view, Side::Corp)[2].alarm);
    }

    /// Issuaq Adaptics: Sustaining Diversity takes a point off what the Corp
    /// needs for each power counter it hosts, so the readout's total is the
    /// view's own number, not the match rule's, and only the Corp's moves.
    #[test]
    fn the_total_is_the_engines_number_for_each_side() {
        let mut view = view();
        view.corp.points_to_win = view.rules.winning_agenda_points as i32 - 2;
        assert_eq!(readouts(&view, Side::Corp)[2].value, format!("0/{}", view.rules.winning_agenda_points - 2));
        assert_eq!(readouts(&view, Side::Runner)[2].value, format!("0/{}", view.runner.points_to_win));
    }

    #[test]
    fn the_score_area_lists_each_agenda_with_its_printed_points() {
        let registry = crate::decks::sample_deck_registry();
        let mut view = view();
        assert!(score_area(&view, Side::Corp, &registry).is_empty());
        let agenda = registry.iter().find(|c| c.agenda_points.is_some() && c.advancement_requirement.is_some()).expect("the sample decks hold an agenda").clone();
        view.runner.scored_agendas = vec![netrunner_core::rules::ScoredAgenda::plain(agenda.id.clone()); 2];
        let stolen = score_area(&view, Side::Runner, &registry);
        assert_eq!(stolen.len(), 2, "two copies are two rows");
        assert_eq!(stolen[0].title, agenda.title);
        assert_eq!(stolen[0].points, agenda.agenda_points.unwrap() as i32);
        assert!(stolen[0].install.is_none(), "a copy placed with no handle offers no ability");
        let facts = stolen[0].facts(Side::Runner, &registry);
        assert!(facts[0].starts_with("Stolen by the Runner"), "{facts:?}");
        assert!(stolen[0].line().contains(&agenda.title));
    }

    /// Jeitinho in the Runner's score area is an assassination agenda worth
    /// 0, and its row says so.
    #[test]
    fn a_runner_card_added_as_an_agenda_says_the_subtype_it_was_added_as() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let as_agenda = netrunner_core::dsl::AsAgenda { points: 0, cannot_forfeit: false, subtype: Some(netrunner_core::dsl::CardSubtype::Assassination) };
        state.runner.scored_agendas = vec![netrunner_core::rules::ScoredAgenda { as_agenda: Some(as_agenda), ..netrunner_core::rules::ScoredAgenda::plain(CardId("jeitinho".into())) }];
        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Corp);
        let scored = score_area(&view, Side::Runner, &registry);
        assert_eq!((scored[0].title.as_str(), scored[0].points), ("Jeitinho", 0));
        assert_eq!(scored[0].facts(Side::Runner, &registry)[0], "Added as an assassination agenda · 0 points");
    }

    /// A stolen Let Them Dream is worth 1 less than it prints, and the
    /// row says what the total counts rather than the printed 2.
    #[test]
    fn the_score_area_lists_what_each_agenda_is_worth_now() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let dream = CardId("let_them_dream".into());
        state.runner.scored_agendas = vec![netrunner_core::rules::ScoredAgenda::plain(dream.clone())];
        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Runner);
        let stolen = score_area(&view, Side::Runner, &registry);
        assert_eq!(registry.get(&dream).and_then(|card| card.agenda_points), Some(2));
        assert_eq!(stolen[0].points, 1, "worth 1 less in the Runner's score area");
        let facts = stolen[0].facts(Side::Runner, &registry);
        assert!(facts[0].contains("1 point"), "{facts:?}");
        assert!(facts.iter().any(|line| line.starts_with("Prints 2 points")), "{facts:?}");
    }

    /// Méliès U's copy is the Corp's to see from the moment it is set and
    /// the Runner's once it is turned over; an identity with one side has
    /// no side to name, and Dewi's flip reads as its flip side.
    #[test]
    fn a_flip_identity_says_which_side_is_up_to_whoever_may_know() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let as_seen = |state: &GameState, viewer: Side, side: Side| {
            identity_side(&netrunner_core::view::build_client_view(state, &registry, viewer), side, &registry)
        };
        state.corp.identity = Some(CardId("melies_u_only_the_brightest".into()));
        state.runner.identity = Some(CardId("dewi_subrotoputri_pedagogical_dhalang".into()));
        let front = as_seen(&state, Side::Corp, Side::Corp).expect("Méliès U flips");
        assert_eq!((front.chip().as_str(), front.line().as_str()), ("Front", "Its front is up"), "nothing is set before the first discard phase ends");
        state.corp.identity_copy = 2;
        assert_eq!(as_seen(&state, Side::Corp, Side::Corp).unwrap().line(), "Its front is up · side 2 is set, in secret");
        assert_eq!(as_seen(&state, Side::Runner, Side::Corp).unwrap().line(), "Its front is up · the side under it is set in secret");
        state.corp.identity_flipped = true;
        assert_eq!(as_seen(&state, Side::Runner, Side::Corp).unwrap().chip(), "Side 2", "turned over, it is public");
        assert_eq!(as_seen(&state, Side::Corp, Side::Runner).unwrap().chip(), "Front");
        state.runner.identity_flipped = true;
        assert_eq!(as_seen(&state, Side::Corp, Side::Runner).unwrap().line(), "Its flip side is up");
        state.runner.identity = Some(CardId("hiram_0mission_svensson_shadow_of_the_past".into()));
        assert_eq!(as_seen(&state, Side::Corp, Side::Runner), None, "one side, nothing to say");
    }

    /// A lasting rule reads with the card that made it; a strength does
    /// not, because the number it changes is already on the card.
    #[test]
    fn what_is_in_effect_is_listed_by_the_card_that_made_it() {
        use netrunner_core::dsl::Prohibition;
        use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
        let registry = crate::decks::sample_deck_registry();
        let mut view = view();
        assert!(in_effect(&view, &registry).is_empty());
        let made = |what, on, until, source: &str| LingeringEffect { what, on, until, source: CardId(source.into()) };
        view.lingering = vec![
            made(Lingering::Cannot(Prohibition::SpendOrLoseCreditPool), On::Player(Side::Runner), Until::EndOfRun, "aircheck"),
            made(Lingering::AllottedClicks(-1), On::Player(Side::Runner), Until::NextTurnOf(Side::Runner), "caveat_emptor"),
            made(Lingering::Strength(2), On::Install(InstallId(3)), Until::EndOfEncounter(InstallId(4)), "aircheck"),
        ];
        let lines = in_effect(&view, &registry);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0], "Aircheck: the Runner cannot spend or lose credits from their credit pool, for the rest of this run");
        assert!(lines[1].ends_with(": the Runner has 1 fewer allotted click next turn"), "{lines:?}");
    }

    /// The Runner's mark is in effect for the turn and names its server, in
    /// both clients, which is what Tunnel Vision's break and Info Bounty's
    /// credits are about.
    #[test]
    fn the_runners_mark_is_listed_for_the_rest_of_the_turn() {
        use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
        let registry = crate::decks::sample_deck_registry();
        let mut view = view();
        view.lingering = vec![LingeringEffect {
            what: Lingering::Mark(netrunner_core::rules::ServerId::RnD),
            on: On::Player(Side::Runner),
            until: Until::EndOfTurn(view.turn),
            source: CardId("tunnel_vision".into()),
        }];
        assert_eq!(in_effect(&view, &registry), vec!["Tunnel Vision: the Runner's mark is R&D, for the rest of this turn".to_string()]);
    }

    /// Parhelion Stage 8's lasting rules, each with the card that made it:
    /// a loss that ends with a later turn says whose, and a replaced
    /// subroutine quotes what resolves instead.
    #[test]
    fn a_lost_ability_a_limit_on_breaking_and_a_replaced_subroutine_are_listed() {
        use netrunner_core::dsl::Prohibition;
        use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
        use netrunner_core::rules::ServerId;
        let registry = crate::decks::sample_deck_registry();
        let mut view = view();
        let made = |what, on, until, source: &str| LingeringEffect { what, on, until, source: CardId(source.into()) };
        let turn = view.turn;
        view.lingering = vec![
            made(Lingering::LosesAbilities, On::Install(InstallId(901)), Until::EndOfTurn(turn + 1), "klevetnik"),
            made(Lingering::BreakLimit(0), On::Install(InstallId(902)), Until::EndOfEncounter(InstallId(902)), "anvil"),
            made(Lingering::BreakLimit(1), On::Install(InstallId(903)), Until::EndOfRun, "unsmiling_tsarevna"),
            made(Lingering::Cannot(Prohibition::BreakSubroutines), On::Install(InstallId(904)), Until::EndOfRun, "hafrun"),
            made(Lingering::ChosenServer(ServerId::Hq), On::Player(Side::Runner), Until::EndOfTurn(turn), "tsakhia_bankhar_gantulga"),
            made(Lingering::SubroutinesReplaced, On::Install(InstallId(905)), Until::EndOfEncounter(InstallId(905)), "tsakhia_bankhar_gantulga"),
        ];
        let lines = in_effect(&view, &registry);
        assert_eq!(
            lines,
            [
                "Klevetnik: that card has lost all its abilities, until the end of the Corp's next turn",
                "Anvil: the Runner cannot break the ice's printed subroutines, for this encounter",
                "Unsmiling Tsarevna: the Runner cannot break more than 1 of the ice's printed subroutines each encounter, for the rest of this run",
                "Hafrún: that card's abilities cannot break subroutines, for the rest of this run",
                "Tsakhia \"Bankhar\" Gantulga: the chosen server is HQ, for the rest of this turn",
                "Tsakhia \"Bankhar\" Gantulga: each subroutine on the ice resolves as \u{201c}Do 1 net damage\u{201d}, for this encounter",
            ]
        );
    }

    /// Attini's standing "cannot", in force only while its subroutines
    /// resolve at threat 3, reads as the card that says it — the reason an
    /// offer to pay 2[credit] has no Accept.
    #[test]
    fn a_standing_prohibition_is_listed_by_the_card_that_says_it() {
        use netrunner_core::dsl::Prohibition;
        use netrunner_core::view::StandingProhibition;
        let registry = crate::decks::sample_deck_registry();
        let mut view = view();
        view.standing_cannot = vec![StandingProhibition { what: Prohibition::SpendCredits, source: CardId("attini".into()) }];
        assert_eq!(in_effect(&view, &registry), vec!["Attini: the Runner cannot spend credits".to_string()]);
    }

    /// Front Company's "the first run each turn cannot be made against a
    /// remote server" is in force until the turn's first run, and is listed
    /// by the card until then: the reason no remote is offered to run.
    #[test]
    fn front_company_keeps_the_first_run_off_the_remotes_and_says_so_until_it_is_made() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        state.corp.installed.push(netrunner_core::rules::InstalledCard {
            install_id: netrunner_core::rules::InstallId(900),
            card: CardId("front_company".into()),
            server: netrunner_core::rules::ServerId::Remote(0),
            slot: netrunner_core::rules::InstallSlot::Root,
            rezzed: true,
            ..Default::default()
        });
        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Runner);
        assert_eq!(in_effect(&view, &registry), vec!["Front Company: the Runner cannot run on a remote server".to_string()]);
    }

    /// Bahia Bands' 4[credit] sit beside the credit pool as "+4", which
    /// cannot say they pay only trash costs; the line does.
    #[test]
    fn bahia_bands_says_its_run_credits_pay_only_trash_costs() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        state.active_run = Some(netrunner_core::rules::RunState {
            server: netrunner_core::rules::ServerId::Hq,
            initiated_by: Some(CardId("bahia_bands".into())),
            bonus_run_credits: 4,
            run_credits_pay_for: Some(netrunner_core::dsl::PaysFor::TrashCosts),
            ..Default::default()
        });
        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Corp);
        assert_eq!(in_effect(&view, &registry), ["This run: the 4 [credit] on Bahia Bands may be spent only to pay trash costs"]);
    }

    /// Lycian Multi-Munition's choice is one line for the piece of ice,
    /// however many subtypes it chose.
    #[test]
    fn the_subtypes_a_piece_of_ice_was_rezzed_as_are_one_line() {
        use netrunner_core::dsl::IceType;
        use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
        let registry = crate::decks::sample_deck_registry();
        let mut view = view();
        let lycian = InstallId(7);
        let gained = |kind| LingeringEffect { what: Lingering::GainSubtype(kind), on: On::Install(lycian), until: Until::WhileRezzed(lycian), source: CardId("lycian_multi_munition".into()) };
        view.lingering = vec![gained(IceType::Sentry), gained(IceType::Barrier)];
        assert_eq!(in_effect(&view, &registry), ["Lycian Multi-Munition: it is a barrier and sentry while it remains rezzed"]);
    }

    #[test]
    fn a_card_revealed_in_a_hand_is_listed_until_it_moves() {
        let registry = crate::decks::sample_deck_registry();
        let mut view = view();
        view.revealed = vec![netrunner_core::rules::RevealedCard { side: Side::Corp, card: CardId("hedge_fund".into()) }];
        assert_eq!(in_effect(&view, &registry), ["revealed in HQ: Hedge Fund"]);
        view.revealed.clear();
        view.runner.set_aside = vec![CardId("sure_gamble".into()), CardId("corroder".into())];
        assert_eq!(in_effect(&view, &registry), ["set aside: Sure Gamble, Corroder"]);
    }

    /// During a run the Runner's credits read "pool +run": the bad
    /// publicity fund and a run event's credits, which the pool excludes.
    #[test]
    fn a_runs_own_credits_sit_beside_the_pool() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let credits = |state: &GameState| readouts(&netrunner_core::view::build_client_view(state, &registry, Side::Runner), Side::Runner)[0].value.clone();
        let pool = state.runner.resources.credits.0;
        assert_eq!(credits(&state), pool.to_string());
        state.active_run = Some(netrunner_core::rules::RunState { server: netrunner_core::rules::ServerId::Hq, bad_publicity_credits: 2, bonus_run_credits: 1, ..Default::default() });
        assert_eq!(credits(&state), format!("{pool} +3"));
    }

    /// What an identity holds is said: Making News' recurring credits
    /// once, as credits; AU Co.'s power counters; nothing for an identity
    /// with nothing on it.
    #[test]
    fn an_identitys_credits_and_counters_are_said() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let seen = |state: &GameState| {
            let view = netrunner_core::view::build_client_view(state, &registry, Side::Runner);
            (identity_facts(&view, Side::Corp, &registry), identity_chip(&view, Side::Corp, &registry))
        };
        state.corp.identity = Some(CardId("nbn_making_news".into()));
        state.corp.identity_counters = 1;
        assert_eq!(seen(&state), (vec!["1 of 2 recurring credits left".to_string()], Some("1 of 2".to_string())));
        state.corp.identity = Some(CardId("au_co_the_gold_standard_in_clones".into()));
        state.corp.identity_counters = 3;
        assert_eq!(seen(&state), (vec!["3 power counters".to_string()], Some("3 power counters".to_string())));
        state.corp.identity_counters = 0;
        assert_eq!(seen(&state), (Vec::new(), None));
    }

    /// Spree's counters are on an event in the play area, which the board
    /// has no place for: the run says them.
    #[test]
    fn a_run_says_the_counters_on_its_event() {
        use netrunner_core::rules::{RunState, ServerId};
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        state.active_run = Some(RunState { server: ServerId::Hq, initiated_by: Some(CardId("spree".into())), event_counters: 2, ..Default::default() });
        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Corp);
        assert_eq!(in_effect(&view, &registry), ["This run: Spree has 2 power counters"]);
    }

    /// Stegodon MK IV's −2 is in every icebreaker's strength; the run says
    /// why.
    #[test]
    fn a_run_says_a_piece_of_ice_was_derezzed_during_it() {
        use netrunner_core::rules::{RunState, ServerId};
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        state.active_run = Some(RunState { server: ServerId::Hq, ice_derezzed: true, ..Default::default() });
        for side in [Side::Corp, Side::Runner] {
            let view = netrunner_core::view::build_client_view(&state, &registry, side);
            assert_eq!(in_effect(&view, &registry), ["This run: a piece of ice has been derezzed"]);
        }
    }

    /// Thunderbolt Armaments' subroutine is the ice's for the rest of the
    /// run, which its card does not print: the run says it, and the ice's
    /// sheet lists it after the printed ones.
    #[test]
    fn a_subroutine_gained_for_the_run_is_said_and_listed_on_the_ice() {
        use netrunner_core::dsl::{Effect, SubroutineDef};
        use netrunner_core::rules::{GainedForTheRun, InstallSlot, InstalledCard, RunState, ServerId};
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let tithe = InstallId(900);
        state.corp.installed.push(InstalledCard { install_id: tithe, card: CardId("tithe".into()), server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: true, ..Default::default() });
        let subroutine = SubroutineDef { text: "End the run unless the Runner trashes 1 of their installed cards.".into(), effect: Effect::EndTheRun, only_breakable_by: None };
        state.active_run = Some(RunState { server: ServerId::Hq, gained_for_the_run: vec![GainedForTheRun { ice: tithe, subroutine, after: true }], ..Default::default() });
        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Runner);
        assert_eq!(in_effect(&view, &registry), ["This run: Tithe has \u{201c}End the run unless the Runner trashes 1 of their installed cards\u{201d} after its other subroutines"]);
        let sheet = super::super::facts::install_facts(&view, tithe, &registry).expect("on the board");
        let listed: Vec<&String> = sheet.iter().filter(|line| line.starts_with('»')).collect();
        assert_eq!(listed.len(), 3, "{sheet:?}");
        assert_eq!(listed[2], "» End the run unless the Runner trashes 1 of their installed cards. — for the rest of this run");
    }

    /// A Maintenance Access run says where it is going before it gets
    /// there.
    #[test]
    fn a_redirected_run_says_where_it_will_go() {
        use netrunner_core::rules::{RunState, ServerId};
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        state.active_run = Some(RunState { server: ServerId::Archives, redirect_on_approach: Some(ServerId::Hq), ..Default::default() });
        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Corp);
        assert_eq!(in_effect(&view, &registry), ["This run: when the Runner would approach Archives, the attacked server becomes HQ instead"]);
    }

    #[test]
    fn the_details_line_repeats_nothing_the_table_shows() {
        let view = view();
        assert_eq!(details(&view, Side::Corp), None, "R&D and Archives are server headers");
        assert!(details(&view, Side::Runner).is_some_and(|line| line.starts_with("MU ")));
    }
}
