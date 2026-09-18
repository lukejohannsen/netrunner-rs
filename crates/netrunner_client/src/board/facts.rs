//! What a person needs to know about an installed card, in words.
//!
//! **The state of a card is game information, and a tile has no room
//! for it.** A card on the table shows whether it is face up, how many
//! advancement tokens sit on it, what it hosts; a tile shows a title.
//! So an install's state is written twice, from one place: as the few
//! words a tile can carry ([`tile_label`]) and as the lines its sheet
//! lists ([`install_facts`]) — rezzed or not and what a rez would cost,
//! where it sits and in what order the Runner meets it, its strength
//! now and as printed, each subroutine and whether it fired, the
//! tokens and counters, the trash cost, the agenda's points and how far
//! it is advanced, what it hosts and what it is hosted on. Both read
//! the masked view only, so a face-down card the viewer cannot name is
//! "face down" with its tokens, never its title.
//!
//! **Why here and not in the desktop crate:** the words are the same
//! for any client, and a rule about which facts a viewer may be told
//! belongs beside the other board words, tested over real views.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, CardType, CounterKind};
use netrunner_core::rules::{InstallId, InstallSlot, PublicInstalledCard, PublicInstalledRunnerCard, RunPhase, ServerId, SubroutineStatus};
use netrunner_core::view::ClientView;

use crate::actions::card_title;
use crate::board::action_map::server_name;

/// Where an install id resolves in the viewer's view.
enum Found<'a> {
    Corp { card: &'a PublicInstalledCard, server: ServerId, ice_count: usize, index: usize },
    Rig(&'a PublicInstalledRunnerCard),
}

fn find(view: &ClientView, id: InstallId) -> Option<Found<'_>> {
    for server in &view.corp.servers {
        if let Some((index, card)) = server.ice.iter().enumerate().find(|(_, c)| c.install_id == id) {
            return Some(Found::Corp { card, server: server.server, ice_count: server.ice.len(), index });
        }
        if let Some(card) = server.root.iter().find(|c| c.install_id == id) {
            return Some(Found::Corp { card, server: server.server, ice_count: server.ice.len(), index: 0 });
        }
    }
    view.runner.rig.iter().find(|c| c.install_id == id).map(Found::Rig)
}

/// The strength and subroutine statuses of an ice the run is at, when
/// the viewer may see them.
fn encounter(view: &ClientView, id: InstallId) -> Option<(i32, Vec<SubroutineStatus>, bool)> {
    let run = view.active_run.as_ref()?;
    let (i, piece) = run.ice.iter().enumerate().find(|(_, p)| p.install_id == id)?;
    let identity = piece.identity.as_ref()?;
    let at = i == run.position && matches!(run.phase, RunPhase::ApproachIce | RunPhase::EncounterIce);
    Some((identity.current_strength, identity.subroutines.iter().map(|s| s.status).collect(), at))
}

fn counter_word(kind: Option<CounterKind>, n: u32) -> String {
    let name = match kind {
        Some(CounterKind::Virus) => "virus counter",
        Some(CounterKind::Power) => "power counter",
        Some(CounterKind::Credit) => "credit",
        None => "counter",
    };
    format!("{n} {name}{}", if n == 1 { "" } else { "s" })
}

/// A token or a counter on a tile, apart from its words, so a client
/// with a picture for the kind can draw the picture beside the number
/// and one without can print [`Token::words`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// The number as the tile shows it: `2/3` for an agenda whose
    /// requirement the viewer knows, else the count.
    pub amount: String,
}

/// What a [`Token`] counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Advancement,
    /// Counters on the card, of the card's kind when the viewer may know
    /// the card (`CardDefinition::counter_kind`).
    Counter(Option<CounterKind>),
}

impl Token {
    /// The token as words, as [`tile_label`] has always printed it.
    pub fn words(&self) -> String {
        match self.kind {
            TokenKind::Advancement => format!("{} adv", self.amount),
            TokenKind::Counter(_) => format!("{} ctr", self.amount),
        }
    }
}

/// The words a tile carries: the title (or `ICE` / `Card` when the
/// viewer may not name it), then whether it is rezzed — or face down,
/// for a card that cannot be — its strength, its tokens and counters,
/// joined by ` · `. An agenda is never "unrezzed": it is advanced,
/// `2/3 adv` when the viewer knows the requirement.
pub fn tile_label(view: &ClientView, id: InstallId, registry: &CardRegistry) -> String {
    let mut parts = vec![tile_title(view, id, registry)];
    parts.extend(tile_tokens(view, id, registry).iter().map(Token::words));
    parts.join(" · ")
}

/// [`tile_label`] without its tokens: the title, the rez state and the
/// strength.
pub fn tile_title(view: &ClientView, id: InstallId, registry: &CardRegistry) -> String {
    let Some(Found::Corp { card, .. }) = find(view, id) else {
        return match find(view, id) {
            Some(Found::Rig(rig)) => card_title(&rig.card, registry),
            _ => "Card".to_string(),
        };
    };
    let def = card.card.as_ref().and_then(|c| registry.get(c));
    let mut parts: Vec<String> = Vec::new();
    match (card.slot, def) {
        (InstallSlot::Ice, Some(def)) => {
            parts.push(def.title.clone());
            if card.rezzed {
                parts.push("rezzed".to_string());
                let strength = encounter(view, id).map_or(def.strength, |(now, _, _)| Some(now));
                if let Some(s) = strength {
                    parts.push(format!("str {s}"));
                }
            } else {
                parts.push("unrezzed".to_string());
            }
        }
        (InstallSlot::Ice, None) => {
            parts.push("ICE".to_string());
            parts.push(if card.rezzed { "rezzed".to_string() } else { "unrezzed".to_string() });
        }
        (InstallSlot::Root, Some(def)) => {
            parts.push(def.title.clone());
            if def.card_type != CardType::Agenda {
                parts.push(if card.rezzed { "rezzed".to_string() } else { "unrezzed".to_string() });
            }
        }
        (InstallSlot::Root, None) => {
            parts.push("Card".to_string());
            parts.push("face down".to_string());
        }
    }
    parts.join(" · ")
}

/// A Corp tile's advancement tokens and counters, in [`tile_label`]'s
/// order: advancement first (with the requirement when the viewer knows
/// it, and on an agenda the viewer knows even at zero), then counters,
/// of the card's kind when the viewer may know the card. Empty for a rig
/// card, which has its own chip line.
pub fn tile_tokens(view: &ClientView, id: InstallId, registry: &CardRegistry) -> Vec<Token> {
    let Some(Found::Corp { card, .. }) = find(view, id) else { return Vec::new() };
    let def = card.card.as_ref().and_then(|c| registry.get(c));
    let mut tokens = Vec::new();
    if card.advancement_tokens > 0 || def.is_some_and(|d| d.card_type == CardType::Agenda) {
        match def.and_then(|d| d.advancement_requirement) {
            Some(need) => tokens.push(Token { kind: TokenKind::Advancement, amount: format!("{}/{need}", card.advancement_tokens) }),
            None if card.advancement_tokens > 0 => tokens.push(Token { kind: TokenKind::Advancement, amount: card.advancement_tokens.to_string() }),
            None => {}
        }
    }
    if let Some(n) = card.counters.filter(|n| *n > 0) {
        tokens.push(Token { kind: TokenKind::Counter(def.and_then(|d| d.counter_kind)), amount: n.to_string() });
    }
    tokens
}

/// The lines an install's sheet lists, for the viewer: everything the
/// board knows about the card that a person would read off the table.
/// `None` when the id is not on the board.
pub fn install_facts(view: &ClientView, id: InstallId, registry: &CardRegistry) -> Option<Vec<String>> {
    let mut lines = Vec::new();
    match find(view, id)? {
        Found::Corp { card, server, ice_count, index } => {
            let def = card.card.as_ref().and_then(|c| registry.get(c));
            let name = server_name(server);
            match card.slot {
                InstallSlot::Ice => {
                    let order = match (index, ice_count) {
                        (_, 1) => "the only ice".to_string(),
                        (0, _) => format!("outermost of {ice_count}, met first"),
                        (i, n) if i + 1 == n => format!("innermost of {n}, met last"),
                        (i, n) => format!("{} of {n} from the outside", i + 1),
                    };
                    lines.push(format!("Ice protecting {name} — {order}"));
                }
                InstallSlot::Root => lines.push(format!("In the root of {name}")),
            }
            let is_agenda = def.is_some_and(|d| d.card_type == CardType::Agenda);
            match (card.rezzed, def, is_agenda) {
                (_, _, true) => {}
                (true, _, _) => lines.push("Rezzed".to_string()),
                (false, Some(def), _) => lines.push(format!("Unrezzed — rez cost {}", def.cost)),
                (false, None, _) if card.slot == InstallSlot::Ice => lines.push("Unrezzed — the Corp may rez it as it is approached".to_string()),
                (false, None, _) => lines.push("Face down — unrezzed; an agenda, an asset or an upgrade".to_string()),
            }
            if let Some(def) = def {
                if let (Some(points), Some(need)) = (def.agenda_points, def.advancement_requirement) {
                    lines.push(format!("Agenda worth {points} point{} — advanced {} of {need}", if points == 1 { "" } else { "s" }, card.advancement_tokens));
                } else if card.advancement_tokens > 0 {
                    lines.push(format!("{} advancement token{}", card.advancement_tokens, if card.advancement_tokens == 1 { "" } else { "s" }));
                }
                if let Some(cost) = def.trash_cost
                    && def.card_type != CardType::Agenda
                {
                    lines.push(format!("Trash cost {cost}"));
                }
                if card.slot == InstallSlot::Ice {
                    match (encounter(view, id), def.strength) {
                        (Some((now, _, _)), Some(printed)) if now != printed => lines.push(format!("Strength {now} now, {printed} printed")),
                        (Some((now, _, _)), _) => lines.push(format!("Strength {now}")),
                        (None, Some(printed)) => lines.push(format!("Strength {printed}")),
                        (None, None) => {}
                    }
                    let statuses = encounter(view, id).map(|(_, subs, at)| (subs, at));
                    for (i, sub) in def.subroutines.iter().enumerate() {
                        let status = match statuses.as_ref().and_then(|(subs, at)| at.then(|| subs.get(i)).flatten()) {
                            Some(SubroutineStatus::Broken) => " — broken",
                            Some(SubroutineStatus::Resolved) => " — fired",
                            Some(SubroutineStatus::Pending) => " — pending",
                            None => "",
                        };
                        lines.push(format!("» {}{status}", sub.text));
                    }
                }
            } else if card.advancement_tokens > 0 {
                lines.push(format!("{} advancement token{}", card.advancement_tokens, if card.advancement_tokens == 1 { "" } else { "s" }));
            }
            if let Some(n) = card.counters.filter(|n| *n > 0) {
                lines.push(counter_word(def.and_then(|d| d.counter_kind), n));
            }
            let hosted: Vec<String> = view.runner.rig.iter().filter(|r| r.hosted_on_ice == Some(id)).map(|r| card_title(&r.card, registry)).collect();
            if !hosted.is_empty() {
                lines.push(format!("Hosts {}", hosted.join(", ")));
            }
        }
        Found::Rig(rig) => {
            let def = registry.get(&rig.card);
            lines.push("Installed in the rig".to_string());
            if let Some(def) = def
                && let Some(printed) = def.strength
            {
                if rig.current_strength != printed {
                    lines.push(format!("Strength {} now, {printed} printed", rig.current_strength));
                } else {
                    lines.push(format!("Strength {printed}"));
                }
            }
            if rig.counters > 0 {
                lines.push(counter_word(def.and_then(|d| d.counter_kind), rig.counters));
            }
            if let Some(ice) = rig.hosted_on_ice {
                lines.push(format!("Hosted on {}", crate::actions::install_label(&ice, registry, Some(view))));
            }
            if let Some(program) = rig.hosted_on_program {
                lines.push(format!("Hosted on {}", crate::actions::install_label(&program, registry, Some(view))));
            }
            if !rig.hosted_cards.is_empty() {
                lines.push(format!("Hosts {}", rig.hosted_cards.iter().map(|c| card_title(c, registry)).collect::<Vec<_>>().join(", ")));
            }
        }
    }
    Some(lines)
}

/// The sheet's heading for an install the viewer cannot name.
pub fn hidden_title(view: &ClientView, id: InstallId) -> String {
    match find(view, id) {
        Some(Found::Corp { card, .. }) if card.slot == InstallSlot::Ice => "Unrezzed ice".to_string(),
        _ => "Face-down card".to_string(),
    }
}

/// The card an install shows the viewer, if any.
pub fn card_of(view: &ClientView, id: InstallId) -> Option<CardId> {
    match find(view, id)? {
        Found::Corp { card, .. } => card.card.clone(),
        Found::Rig(rig) => Some(rig.card.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::{BotAgent, HeuristicAgent, RandomAgent};
    use netrunner_core::rules::{GameState, Side, Viewer};
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};

    /// Real games as both viewers: every install on the board has a tile
    /// label that says whether it is rezzed (or face down, or how far an
    /// agenda is advanced) and a sheet that says where it is; a hidden
    /// card is never named; an ice met in a run says its strength now
    /// and its subroutines with their status.
    #[test]
    fn every_install_has_a_label_and_facts_the_viewer_may_see() {
        let mut labelled = 0;
        let mut hidden = 0;
        let mut encountered = 0;
        let mut agendas = 0;
        for seed in 0..4u64 {
            let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
            let registry: CardRegistry = crate::decks::sample_deck_registry();
            let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
            let mut corp: Box<dyn BotAgent> = if seed % 2 == 0 { Box::new(HeuristicAgent::new(Side::Corp, seed)) } else { Box::new(RandomAgent::new(seed)) };
            let mut runner = RandomAgent::new(seed + 7);
            loop {
                match session.step() {
                    SessionStep::Awaiting { side, view } => {
                        let action = match side {
                            Side::Corp => corp.select_action(&view, &registry),
                            Side::Runner => runner.select_action(&view, &registry),
                        };
                        session.submit(action).unwrap();
                        for viewer in [Viewer::Player(Side::Corp), Viewer::Player(Side::Runner)] {
                            let view = session.view_for(viewer);
                            let installs: Vec<(InstallId, InstallSlot, bool, Option<CardId>, u32)> = view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).map(|c| (c.install_id, c.slot, c.rezzed, c.card.clone(), c.advancement_tokens)).collect();
                            for (id, slot, rezzed, card, adv) in installs {
                                let label = tile_label(&view, id, &registry);
                                let facts = install_facts(&view, id, &registry).expect("an install on the board has facts");
                                labelled += 1;
                                let is_agenda = card.as_ref().and_then(|c| registry.get(c)).is_some_and(|d| d.card_type == CardType::Agenda);
                                match (card.as_ref(), is_agenda) {
                                    (None, _) => {
                                        hidden += 1;
                                        assert!(label.contains(if slot == InstallSlot::Ice { "unrezzed" } else { "face down" }), "{viewer:?}: {label}");
                                        assert!(!rezzed, "a card the viewer cannot name is not rezzed");
                                        if adv > 0 {
                                            assert!(label.contains(&format!("{adv} adv")), "{label}");
                                        }
                                    }
                                    (Some(c), true) => {
                                        agendas += 1;
                                        assert!(!label.contains("rezzed"), "an agenda is never rezzed: {label}");
                                        assert!(label.contains("adv") && label.starts_with(&card_title(c, &registry)), "{label}");
                                        assert!(facts.iter().any(|l| l.starts_with("Agenda worth")), "{facts:?}");
                                    }
                                    (Some(c), false) => {
                                        assert!(label.starts_with(&card_title(c, &registry)), "{label}");
                                        assert!(label.contains(if rezzed { "· rezzed" } else { "· unrezzed" }), "{label}");
                                        assert!(facts.iter().any(|l| l == "Rezzed" || l.starts_with("Unrezzed")), "{facts:?}");
                                    }
                                }
                                assert!(facts[0].starts_with("Ice protecting") || facts[0].starts_with("In the root of"), "{facts:?}");
                                if let Some((now, subs, true)) = encounter(&view, id) {
                                    encountered += 1;
                                    assert!(facts.iter().any(|l| l.starts_with(&format!("Strength {now}"))), "{facts:?}");
                                    assert_eq!(facts.iter().filter(|l| l.starts_with("» ")).count(), subs.len().max(card.as_ref().and_then(|c| registry.get(c)).map_or(0, |d| d.subroutines.len())), "{facts:?}");
                                }
                            }
                            for rig in &view.runner.rig {
                                let facts = install_facts(&view, rig.install_id, &registry).unwrap();
                                assert_eq!(facts[0], "Installed in the rig");
                                assert_eq!(tile_label(&view, rig.install_id, &registry), card_title(&rig.card, &registry));
                            }
                        }
                    }
                    SessionStep::Applied { .. } => {}
                    SessionStep::Ended { .. } => break,
                    SessionStep::Stalled(reason) => panic!("seed {seed}: {reason:?}"),
                }
            }
        }
        assert!(labelled > 500, "{labelled} installs labelled");
        assert!(hidden > 50, "{hidden} hidden installs seen from the other chair");
        assert!(encountered > 0, "an ice was met in a run");
        assert!(agendas > 0, "an agenda was installed");
    }
}
