//! The board in words, for a model: a system prompt built once per game
//! and a short message per decision, and the number read back.
//!
//! **Terse on purpose.** Every decision is a request, and input tokens
//! are the bulk of what a game costs, so the message is lines of facts
//! rather than prose: readouts, one line per server, the rig, the hand
//! by title, the run, the question, then the numbered legal actions.
//! Card text is in the system prompt (the bot's own deck, once per game,
//! which a provider that caches a repeated prefix serves at its cache
//! rate) and in the message only for the other side's cards in play,
//! which the deck's glossary cannot hold.
//!
//! **The rules are this project's words**, a summary a new player would
//! be told, never the Comprehensive Rules' text, which is Null Signal
//! Games' (`rules/NOTICE.md`).

use std::collections::BTreeSet;
use std::fmt;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, CardType};
use netrunner_core::rules::{Deck, GamePhase, InstallSlot, PublicInstalledCard, PublicInstalledRunnerCard, RunPhase, Side};
use netrunner_core::view::ClientView;

use crate::actions::{card_title, explain_action};
use crate::board::action_map::server_name;
use crate::board::{self, hud, rig, ActionEntry, ActionMap, Prompt};
use crate::card_text;

/// The heading over the model's own last few moves.
pub const RECENT_HEADING: &str = "Your recent moves";

/// The rules in this project's words, once per game. Kept short: a model
/// knows the game's shape, and what it needs is the vocabulary the
/// message uses.
const RULES: &str = "\
Netrunner is a two-player card game. The Corp installs cards into servers (HQ, R&D, Archives are the central servers; remotes are made by installing) \
and protects them with ice; it scores agendas by advancing them and spending the advancement. The Runner installs programs, hardware and resources \
into their rig, and makes runs on servers: each piece of ice protecting the server is approached outermost first, the Corp may rez it (pay to turn \
it face up), the Runner may break its subroutines with an icebreaker whose strength matches or exceeds the ice's, and every unbroken subroutine \
fires. A successful run accesses the server's cards: an agenda is stolen, a card may be trashed for its trash cost. Each turn a player has clicks \
(Corp 3, Runner 4): a click draws a card, gains a credit, installs a card, plays an event or operation, advances a card (Corp), makes a run \
(Runner), or removes a tag (Runner). Credits pay for everything. The first player to reach the match's agenda points wins; the Corp also wins by \
flatlining the Runner (damage beyond their hand), and the Runner wins if the Corp must draw from an empty R&D. Tags let the Corp trash the \
Runner's resources and play punishing operations. At the end of a turn, discard down to the maximum hand size (5 unless changed). A paid-ability \
window is a moment either player may use an ability or pass; passing is usually right unless a rez or an ability pays off now.";

/// The answer format, stable so the prefix caches.
fn answer_format(explain: bool) -> &'static str {
    if explain {
        "Answer with one JSON object and nothing else: {\"action\": <number from the Legal actions list>, \"why\": \"<at most fifteen words>\"}."
    } else {
        "Answer with one JSON object and nothing else: {\"action\": <number from the Legal actions list>}."
    }
}

/// The system prompt for one game: the rules, the chair, how to answer,
/// and the deck's cards with their printed text — one line per distinct
/// card, in title order, so the bytes never change within a game.
pub fn system_prompt(side: Side, own_deck: &Deck, registry: &CardRegistry, explain: bool) -> String {
    let chair = match side {
        Side::Corp => "You play the Corp. You win by scoring agendas or flatlining the Runner; you lose when the Runner steals enough agenda points or R&D runs out.",
        Side::Runner => "You play the Runner. You win by stealing agendas from the Corp's servers; you lose if flatlined or when the Corp scores enough agenda points.",
    };
    let mut text = format!("{RULES}\n\n{chair}\n\n{}\n\nEvery decision you are shown is yours; choose the action that best advances your win. Think like an experienced player: build economy early, do not waste clicks, and weigh what the other side can afford.\n\nYour deck's cards:\n", answer_format(explain));
    let mut ids: Vec<&CardId> = std::iter::once(&own_deck.identity).chain(own_deck.cards.iter().map(|(id, _)| id)).collect();
    ids.sort_by_key(|id| registry.get(id).map_or_else(|| id.0.clone(), |card| card.title.clone()));
    ids.dedup();
    for id in ids {
        text.push_str(&glossary_line(id, registry));
        text.push('\n');
    }
    text
}

/// The ids the system prompt's glossary holds, so the message prints
/// text for none of them.
pub fn glossary_ids(own_deck: &Deck) -> BTreeSet<CardId> {
    std::iter::once(own_deck.identity.clone()).chain(own_deck.cards.iter().map(|(id, _)| id.clone())).collect()
}

/// "Hedge Fund (operation, 5 credits): Gain 9 credits."
fn glossary_line(id: &CardId, registry: &CardRegistry) -> String {
    let Some(card) = registry.get(id) else { return format!("{}: (unknown card)", id.0) };
    let mut facts: Vec<String> = vec![type_word(&card.card_type).to_string()];
    match card.card_type {
        CardType::Agenda => {
            if let Some(req) = crate::card_face::advancement_slot(card) {
                facts.push(format!("advance {}", req.value()));
            }
            if let Some(points) = card.agenda_points {
                facts.push(format!("{points} points"));
            }
        }
        CardType::Identity => {}
        CardType::Ice(_) | CardType::Asset | CardType::Upgrade => {
            facts.push(format!("rez {}", card.cost));
            if let Some(strength) = card.strength {
                facts.push(format!("strength {strength}"));
            }
            if let Some(trash) = card.trash_cost {
                facts.push(format!("trash cost {trash}"));
            }
        }
        _ => {
            facts.push(format!("cost {}", card.cost));
            if let Some(strength) = card.strength {
                facts.push(format!("strength {strength}"));
            }
        }
    }
    let text = printed(id, registry);
    format!("{} ({}): {}", card.title, facts.join(", "), if text.is_empty() { "(no text)".to_string() } else { text })
}

fn type_word(card_type: &CardType) -> &'static str {
    match card_type {
        CardType::Agenda => "agenda",
        CardType::Asset => "asset",
        CardType::Operation => "operation",
        CardType::Ice(_) => "ice",
        CardType::Hardware => "hardware",
        CardType::Resource => "resource",
        CardType::Program => "program",
        CardType::Event => "event",
        CardType::Identity => "identity",
        CardType::Upgrade => "upgrade",
    }
}

/// A card's printed text on one line, its symbols as words.
fn printed(id: &CardId, registry: &CardRegistry) -> String {
    registry
        .get(id)
        .and_then(|card| card.printed_text.as_deref())
        .map(|text| card_text::render(&card_text::segments(text), false).split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// The legal actions the model is offered, labelled: without the
/// deselect toggles every bot drops (`netrunner_bots::agent::progressive`),
/// a selection prompt's duplicates folded and ordered as the terminal
/// lists them (`selection::shown`), each with the board's own label.
pub fn menu(view: &ClientView, registry: &CardRegistry) -> Vec<ActionEntry> {
    let map = ActionMap::build(view, registry);
    let actions = netrunner_bots::agent::progressive(&view.legal_actions, view.pending_decision.as_ref());
    let actions = crate::selection::shown(&actions, Some(view), registry);
    let entries: Vec<ActionEntry> = actions.iter().filter_map(|action| map.entries.iter().find(|entry| entry.action == *action).cloned()).collect();
    if entries.is_empty() { map.entries } else { entries }
}

/// A decision the planner takes under `Asks::Clicks`: a paid-ability
/// window with nothing parked in it, or a parked payment split. A
/// Runner's window mid-encounter is not routine — the breaks are the
/// decision — and nothing a card asks (a choice, a paid choice, a
/// prevention, a trace) ever is.
pub fn is_routine(view: &ClientView) -> bool {
    if view.pending_decision.is_some() || view.pending_paid_choice.is_some() || view.pending_prevention.is_some() || view.active_trace.is_some() {
        return false;
    }
    if view.pending_payment.as_ref().is_some_and(|payment| payment.own.is_some()) {
        return true;
    }
    if view.paid_ability_window.is_some() {
        let encountering = view.active_run.as_ref().is_some_and(|run| run.phase == RunPhase::EncounterIce);
        return !(encountering && view.viewer.side() == Some(Side::Runner));
    }
    false
}

/// The message for one decision.
pub fn situation(view: &ClientView, registry: &CardRegistry, menu: &[ActionEntry], recent: &[(String, String)], glossary: &BTreeSet<CardId>) -> String {
    let side = view.viewer.side().unwrap_or(Side::Corp);
    let mut out = String::new();
    let line = |out: &mut String, text: String| {
        out.push_str(&text);
        out.push('\n');
    };
    line(&mut out, format!("You are the {}. Turn {}, {}.", side_word(side), view.turn, phase_words(view)));
    for who in [Side::Corp, Side::Runner] {
        let mut parts: Vec<String> = hud::readouts(view, who).iter().map(|r| format!("{} {}", r.label, r.value)).collect();
        if who == Side::Corp {
            let facedown = view.corp.archives.iter().filter(|card| card.facedown).count();
            parts.push(format!("HQ {}", view.corp.hq_count));
            parts.push(format!("R&D {}", view.corp.rd_count));
            parts.push(format!("Archives {}{}", view.corp.archives.len(), if facedown > 0 { format!(" ({facedown} face down)") } else { String::new() }));
        } else {
            parts.push(format!("Stack {}", view.runner.stack_count));
            parts.push(format!("Heap {}", view.runner.heap.len()));
            if let Some(details) = hud::details(view, who) {
                parts.push(details);
            }
        }
        line(&mut out, format!("{}: {}", side_word(who), parts.join(" · ")));
    }
    let identity = |id: &Option<CardId>| id.as_ref().map_or_else(|| "unknown".to_string(), |id| card_title(id, registry));
    line(&mut out, format!("Corp identity: {}. Runner identity: {}.", identity(&view.corp.identity), identity(&view.runner.identity)));
    for who in [Side::Corp, Side::Runner] {
        let scored = hud::score_area(view, who, registry);
        if !scored.is_empty() {
            line(&mut out, format!("{}'s score area: {}", side_word(who), scored.iter().map(|card| card.line()).collect::<Vec<_>>().join("; ")));
        }
    }
    let in_effect = hud::in_effect(view, registry);
    if !in_effect.is_empty() {
        line(&mut out, format!("In effect: {}", in_effect.join("; ")));
    }
    // The servers, ice outermost first, as the table reads them.
    line(&mut out, "Servers (ice outermost first):".to_string());
    for server in board::table_servers(view) {
        let ice: Vec<String> = server.ice.iter().map(|card| corp_card_words(view, card, registry)).collect();
        let root: Vec<String> = server.root.iter().map(|card| corp_card_words(view, card, registry)).collect();
        line(
            &mut out,
            format!(
                "- {}: ice [{}]; root [{}]",
                server_name(server.server),
                if ice.is_empty() { "none".to_string() } else { ice.join(", ") },
                if root.is_empty() { "empty".to_string() } else { root.join(", ") }
            ),
        );
    }
    let faceup: Vec<String> = view.corp.archives.iter().filter_map(|card| card.card.as_ref()).map(|id| card_title(id, registry)).collect();
    if !faceup.is_empty() {
        line(&mut out, format!("Archives, face up: {}", faceup.join(", ")));
    }
    // The rig.
    let rows = rig::rows(view, registry, side);
    let rig_words: Vec<String> = rows
        .iter()
        .filter(|(_, cards)| !cards.is_empty())
        .map(|(row, cards)| format!("{}: {}", row.label(), cards.iter().map(|card| rig_card_words(view, card, registry)).collect::<Vec<_>>().join(", ")))
        .collect();
    line(&mut out, format!("Runner's rig: {}", if rig_words.is_empty() { "empty".to_string() } else { rig_words.join("; ") }));
    if !view.runner.heap.is_empty() {
        line(&mut out, format!("Heap: {}", view.runner.heap.iter().map(|id| card_title(id, registry)).collect::<Vec<_>>().join(", ")));
    }
    // The hand.
    let hand = match side {
        Side::Corp => view.corp.hq_cards.as_ref(),
        Side::Runner => view.runner.grip_cards.as_ref(),
    };
    if let Some(hand) = hand {
        let titles: Vec<String> = hand.iter().map(|id| card_title(id, registry)).collect();
        line(&mut out, format!("Your hand ({}): {}", hand.len(), if titles.is_empty() { "empty".to_string() } else { titles.join(", ") }));
    }
    // The run.
    if let Some(run) = &view.active_run {
        let at = if run.ice.is_empty() {
            "no ice".to_string()
        } else if run.position >= run.ice.len() {
            format!("past all {} ice", run.ice.len())
        } else {
            format!("at ice {} of {} (outermost first)", run.position + 1, run.ice.len())
        };
        line(&mut out, format!("Run on {}: {}, {}.", server_name(run.server), run_phase_words(run.phase), at));
        if let Some(encounter) = board::encounter_subroutines(view) {
            let subs: Vec<String> = encounter.subroutines.iter().enumerate().map(|(i, sub)| format!("{}. {} ({})", i + 1, sub.text, sub.word())).collect();
            line(&mut out, format!("Encountering {} — {}: {}", encounter.card.as_ref().map_or_else(|| "unrezzed ice".to_string(), |id| card_title(id, registry)), encounter.strength_line(registry), subs.join("; ")));
            let routes = board::routes(view, registry);
            if !routes.is_empty() {
                line(&mut out, format!("Ways through: {}", routes.iter().map(|route| route.label(view, registry)).collect::<Vec<_>>().join("; ")));
            }
        }
        if let Some(access) = &run.access_state {
            line(&mut out, format!("Accessing {}: {} candidate(s) on the table, {} from the zone.", server_name(access.server), access.candidates.len(), access.from_zone));
        }
    }
    // The other side's cards in play, whose text the glossary cannot hold.
    let mut others: Vec<CardId> = Vec::new();
    let mut note = |id: &CardId| {
        if !glossary.contains(id) && !others.contains(id) {
            others.push(id.clone());
        }
    };
    match side {
        Side::Corp => {
            if let Some(id) = &view.runner.identity {
                note(id);
            }
            for card in &view.runner.rig {
                note(&card.card);
            }
        }
        Side::Runner => {
            if let Some(id) = &view.corp.identity {
                note(id);
            }
            for server in &view.corp.servers {
                for card in server.ice.iter().chain(server.root.iter()) {
                    if let Some(id) = &card.card {
                        note(id);
                    }
                }
            }
        }
    }
    if !others.is_empty() {
        line(&mut out, "Opponent's cards in play:".to_string());
        for id in others {
            line(&mut out, format!("- {}", glossary_line(&id, registry)));
        }
    }
    // The question.
    if let Some(prompt) = Prompt::of(view, registry) {
        let detail = if prompt.detail.is_empty() { String::new() } else { format!(" {}", prompt.detail) };
        line(&mut out, format!("The question: {}.{}", prompt.title, detail));
    } else if let Some(asks) = crate::prose::decision_prompt(view, registry) {
        line(&mut out, format!("The question: {asks}"));
    }
    if !recent.is_empty() {
        line(&mut out, format!("{RECENT_HEADING}:"));
        for (label, why) in recent {
            line(&mut out, if why.is_empty() { format!("- {label}") } else { format!("- {label} — {why}") });
        }
    }
    line(&mut out, "Legal actions:".to_string());
    let ambiguous = view.pending_decision.is_some() || view.pending_paid_choice.is_some() || view.pending_prevention.is_some() || view.active_run.as_ref().is_some_and(|run| run.phase == RunPhase::EncounterIce);
    for (i, entry) in menu.iter().enumerate() {
        if ambiguous {
            let explained = explain_action(&entry.action, registry, Some(view));
            if explained != entry.label && !explained.is_empty() {
                line(&mut out, format!("{}. {} — {}", i + 1, entry.label, explained));
                continue;
            }
        }
        line(&mut out, format!("{}. {}", i + 1, entry.label));
    }
    out.push_str("Which number?");
    out
}

fn side_word(side: Side) -> &'static str {
    match side {
        Side::Corp => "Corp",
        Side::Runner => "Runner",
    }
}

fn phase_words(view: &ClientView) -> String {
    let whose = |side: Side| format!("{}'s", side_word(side));
    match &view.phase {
        GamePhase::Mulligan(side) => format!("{} mulligan decision", whose(*side)),
        GamePhase::StartOfTurn(side) => format!("start of {} turn", whose(*side)),
        GamePhase::Action(side) => format!("{} action phase", whose(*side)),
        GamePhase::Discard { side, .. } => format!("{} discard phase", whose(*side)),
        GamePhase::GameOver(_) => "game over".to_string(),
    }
    .replace("Corp's's", "Corp's")
    + if view.paid_ability_window.is_some() { " (paid-ability window)" } else { "" }
}

fn run_phase_words(phase: RunPhase) -> &'static str {
    match phase {
        RunPhase::Initiation => "initiating",
        RunPhase::ApproachIce => "approaching ice",
        RunPhase::EncounterIce => "encountering ice",
        RunPhase::Movement => "between ice",
        RunPhase::AccessingCard => "accessing a card",
        RunPhase::Success => "successful",
        RunPhase::Ended => "ending",
    }
}

/// "Ice Wall (rezzed, strength 1)", "unrezzed ice", "face-down card
/// (advanced 2)", "Hedge Fund".
fn corp_card_words(view: &ClientView, card: &PublicInstalledCard, registry: &CardRegistry) -> String {
    let mut facts: Vec<String> = Vec::new();
    let name = match &card.card {
        Some(id) => {
            let title = card_title(id, registry);
            if card.slot == InstallSlot::Ice || registry.get(id).is_some_and(|def| !matches!(def.card_type, CardType::Agenda)) {
                facts.push(if card.rezzed { "rezzed".to_string() } else { "unrezzed".to_string() });
            }
            if card.rezzed
                && let Some(strength) = registry.get(id).and_then(|def| def.strength)
            {
                facts.push(format!("strength {strength}"));
            }
            title
        }
        None => {
            if card.slot == InstallSlot::Ice { "unrezzed ice".to_string() } else { "face-down card".to_string() }
        }
    };
    let _ = view;
    if card.advancement_tokens > 0 {
        facts.push(format!("advanced {}", card.advancement_tokens));
    }
    if let Some(n) = card.counters.filter(|n| *n > 0) {
        facts.push(format!("{n} counters"));
    }
    if facts.is_empty() { name } else { format!("{name} ({})", facts.join(", ")) }
}

fn rig_card_words(view: &ClientView, card: &PublicInstalledRunnerCard, registry: &CardRegistry) -> String {
    let mut facts: Vec<String> = Vec::new();
    if registry.get(&card.card).is_some_and(|def| def.strength.is_some()) {
        facts.push(format!("strength {}", card.current_strength));
    }
    if card.counters > 0 {
        facts.push(format!("{} counters", card.counters));
    }
    if let Some(ice) = card.hosted_on_ice {
        facts.push(format!("hosted on {}", board::tile_title(view, ice, registry)));
    }
    let name = card_title(&card.card, registry);
    if facts.is_empty() { name } else { format!("{name} ({})", facts.join(", ")) }
}

/// What the model chose: the menu index (0-based) and its word on why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub index: usize,
    pub why: String,
}

/// Why a reply could not be read, in the sentence quoted back to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseProblem {
    NoNumber,
    OutOfRange { given: i64, n: usize },
}

impl fmt::Display for ParseProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseProblem::NoNumber => write!(f, "the reply named no action number"),
            ParseProblem::OutOfRange { given, n } => write!(f, "{given} is not one of the numbers 1 to {n}"),
        }
    }
}

/// Reads the number off a reply: a JSON object (fenced or not), or the
/// first number in the text. The list is numbered from 1; the index
/// comes back from 0.
pub fn parse_reply(text: &str, n: usize) -> Result<Chosen, ParseProblem> {
    let text = text.trim();
    let (number, why) = json_choice(text).unwrap_or_else(|| (first_number(text), String::new()));
    let given = number.ok_or(ParseProblem::NoNumber)?;
    if given < 1 || given > n as i64 {
        return Err(ParseProblem::OutOfRange { given, n });
    }
    Ok(Chosen { index: (given - 1) as usize, why })
}

fn json_choice(text: &str) -> Option<(Option<i64>, String)> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&text[start..=end]).ok()?;
    let action = value.get("action")?;
    let number = action.as_i64().or_else(|| action.as_str().and_then(first_number));
    let why = value.get("why").and_then(|why| why.as_str()).unwrap_or("").trim().to_string();
    Some((number, why))
}

fn first_number(text: &str) -> Option<i64> {
    let digits: String = text.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::fixture::{view_after, view_where};

    #[test]
    fn parse_reply_reads_each_shape_and_names_what_it_could_not() {
        assert_eq!(parse_reply("3", 5).unwrap(), Chosen { index: 2, why: String::new() });
        assert_eq!(parse_reply("{\"action\": 2, \"why\": \"economy\"}", 5).unwrap(), Chosen { index: 1, why: "economy".to_string() });
        assert_eq!(parse_reply("```json\n{\"action\": \"4\"}\n```", 5).unwrap().index, 3);
        assert_eq!(parse_reply("I pick action 5 because it is best", 5).unwrap().index, 4);
        assert_eq!(parse_reply("none of these", 5), Err(ParseProblem::NoNumber));
        assert_eq!(parse_reply("{\"action\": 9}", 5), Err(ParseProblem::OutOfRange { given: 9, n: 5 }));
        assert_eq!(parse_reply("0", 5), Err(ParseProblem::OutOfRange { given: 0, n: 5 }));
        assert_eq!(ParseProblem::OutOfRange { given: 9, n: 5 }.to_string(), "9 is not one of the numbers 1 to 5");
    }

    #[test]
    fn the_system_prompt_is_the_same_bytes_all_game_and_lists_every_card_once() {
        let (registry, _, deck) = view_after(1, 10, Side::Corp);
        let a = system_prompt(Side::Corp, &deck, &registry, false);
        let b = system_prompt(Side::Corp, &deck, &registry, false);
        assert_eq!(a, b);
        assert!(a.contains("You play the Corp"));
        assert!(a.contains("{\"action\": <number"));
        assert!(!a.contains("\"why\""), "no why unless asked");
        assert!(system_prompt(Side::Corp, &deck, &registry, true).contains("\"why\""));
        for (id, _) in &deck.cards {
            let title = card_title(id, &registry);
            assert_eq!(a.matches(&format!("\n{title} (")).count(), 1, "{title} once in the glossary");
        }
        assert!(a.len() > 2000, "long enough to cache on every model: {}", a.len());
    }

    #[test]
    fn the_situation_names_every_legal_action_once_in_menu_order_and_no_own_card_text() {
        let (registry, view, deck) = view_after(2, 12, Side::Runner);
        let menu = menu(&view, &registry);
        assert!(!menu.is_empty());
        let glossary = glossary_ids(&deck);
        let text = situation(&view, &registry, &menu, &[("Draw a card".to_string(), String::new())], &glossary);
        for (i, entry) in menu.iter().enumerate() {
            assert!(text.contains(&format!("\n{}. {}", i + 1, entry.label)), "{}: {text}", entry.label);
        }
        assert!(text.contains("You are the Runner."));
        assert!(text.contains("Your hand ("));
        assert!(text.contains(RECENT_HEADING));
        assert!(text.ends_with("Which number?"));
        // The grip's cards are named, never their text: the glossary has it.
        for id in view.runner.grip_cards.as_ref().unwrap() {
            let printed = printed(id, &registry);
            if printed.len() > 20 {
                assert!(!text.contains(&printed), "{} text in the message: {text}", card_title(id, &registry));
            }
        }
        assert!(text.len() < 6000, "terse: {} chars\n{text}", text.len());
    }

    #[test]
    fn the_runner_never_sees_the_corps_hand_and_the_corp_never_sees_the_grip() {
        let (registry, view, deck) = view_after(3, 8, Side::Runner);
        assert!(view.corp.hq_cards.is_none());
        let text = situation(&view, &registry, &menu(&view, &registry), &[], &glossary_ids(&deck));
        assert!(text.contains("HQ ") && !text.contains("Corp's hand"));
        let (registry, view, deck) = view_after(3, 8, Side::Corp);
        assert!(view.runner.grip_cards.is_none());
        let text = situation(&view, &registry, &menu(&view, &registry), &[], &glossary_ids(&deck));
        assert!(text.contains("Your hand (") && text.contains("Grip "));
    }

    #[test]
    fn an_encounter_lists_the_subroutines_and_a_window_is_routine_but_the_runners_encounter_is_not() {
        let mut found = false;
        for seed in 1..40 {
            let Some((registry, view, deck)) = view_where(seed, Side::Runner, |view| board::encounter_subroutines(view).is_some_and(|e| e.card.is_some())) else { continue };
            let text = situation(&view, &registry, &menu(&view, &registry), &[], &glossary_ids(&deck));
            assert!(text.contains("Encountering "), "{text}");
            assert!(text.contains("Run on "), "{text}");
            if view.paid_ability_window.is_some() {
                assert!(!is_routine(&view), "the Runner's encounter window is a decision");
            }
            found = true;
            break;
        }
        assert!(found, "no encounter in forty seeds");
        let (_, view, _) = view_where(1, Side::Corp, |view| view.paid_ability_window.is_some() && view.active_run.is_none() && view.pending_decision.is_none()).expect("a plain window");
        assert!(is_routine(&view));
        let (_, view, _) = view_where(1, Side::Corp, |view| view.paid_ability_window.is_none() && matches!(view.phase, GamePhase::Action(Side::Corp)) && view.pending_decision.is_none()).expect("a click");
        assert!(!is_routine(&view));
        let (_, view, _) = view_where(1, Side::Runner, |view| view.pending_decision.is_some()).expect("a prompt");
        assert!(!is_routine(&view));
    }

    #[test]
    fn a_selection_prompt_is_folded_to_its_shown_actions() {
        let mut found = false;
        for seed in 1..60 {
            let Some((registry, view, _)) = view_where(seed, Side::Corp, |view| matches!(view.pending_decision, Some(netrunner_core::rules::PendingDecision::ChooseCards { .. })))
                .or_else(|| view_where(seed, Side::Runner, |view| matches!(view.pending_decision, Some(netrunner_core::rules::PendingDecision::ChooseCards { .. }))))
            else {
                continue;
            };
            let menu = menu(&view, &registry);
            let shown = crate::selection::shown(&netrunner_bots::agent::progressive(&view.legal_actions, view.pending_decision.as_ref()), Some(&view), &registry);
            assert_eq!(menu.iter().map(|e| e.action.clone()).collect::<Vec<_>>(), shown);
            assert!(menu.len() <= view.legal_actions.len());
            found = true;
            break;
        }
        assert!(found, "no card selection in sixty seeds");
    }
}
