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
//! **An encounter is the one state a sheet is too slow for**, because
//! it is the state a person is deciding in. So it is written a third
//! way: [`encounter_subroutines`] gives the ice being encountered with
//! every subroutine marked broken, fired or pending, for a client to
//! put where the person is already looking. All three readings share
//! one vocabulary ([`subroutine_word`]).
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

/// One subroutine of the ice a run is at: the clause the card prints and
/// what has become of it this encounter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subroutine {
    /// `SubroutineDef::text` — the printed clause, which the Linked
    /// Clause Rule requires a subroutine to carry, read off the view's
    /// own copy rather than the registry's. The run's copy is the one
    /// the engine is resolving, so a subroutine a card *added* to the
    /// ice is listed and one it removed is not.
    pub text: String,
    pub status: SubroutineStatus,
}

impl Subroutine {
    /// This subroutine's [`subroutine_word`].
    pub fn word(&self) -> &'static str {
        subroutine_word(self.status)
    }
}

/// The word for what has become of a subroutine. The one vocabulary:
/// the board's marks and [`install_facts`]'s sheet lines both say it, so
/// a person reads "broken" in the same sense wherever they look.
pub fn subroutine_word(status: SubroutineStatus) -> &'static str {
    match status {
        SubroutineStatus::Broken => "broken",
        SubroutineStatus::Resolved => "fired",
        SubroutineStatus::Pending => "pending",
    }
}

/// A strength as a person reads it: the number now, and the printed one
/// beside it only when the table or a lingering effect has moved it,
/// because that is the reason a break costs what it costs. The one
/// wording: the sheets of an ice and a breaker and the encounter panel
/// all say it.
pub fn strength_words(now: i32, printed: Option<i32>) -> String {
    match printed {
        Some(printed) if printed != now => format!("Strength {now} now, {printed} printed"),
        _ => format!("Strength {now}"),
    }
}

/// The ice a run is encountering and the state of its subroutines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encounter {
    pub install: InstallId,
    /// The server the run is on, for a client's heading.
    pub server: ServerId,
    /// The ice, when the viewer may name it. A client with no name says
    /// "Ice"; it never reaches around this for one.
    pub card: Option<CardId>,
    /// Its strength as the encounter has it, not as printed.
    pub strength: i32,
    /// In the order the subroutines resolve.
    pub subroutines: Vec<Subroutine>,
}

impl Encounter {
    /// The strength line ([`strength_words`]) against the printed number
    /// the registry holds for the ice, so "Strength 7 now, 4 printed"
    /// says why a break costs more than the card suggests.
    pub fn strength_line(&self, registry: &CardRegistry) -> String {
        let printed = self.card.as_ref().and_then(|id| registry.get(id)).and_then(|def| def.strength);
        strength_words(self.strength, printed)
    }
}

/// The ice the run is encountering, with each subroutine marked broken,
/// fired or still pending — the state of an encounter, so it can be read
/// off the board instead of out of the log.
///
/// **`RunPhase::EncounterIce` only, not the approach.** Nothing has
/// happened to a subroutine at the approach, so there is nothing to mark
/// and the list would be a second copy of the card's text; the
/// encounter is also exactly the window `breaks::routes` offers a route
/// in, so in a client the marks and the buttons that change them appear
/// and disappear together. `None` outside one, and for an ice whose face
/// the viewer may not see — an unrezzed ice reveals nothing, subroutines
/// included (`masking::mask_run_ice`).
pub fn encounter_subroutines(view: &ClientView) -> Option<Encounter> {
    let run = view.active_run.as_ref()?;
    if run.phase != RunPhase::EncounterIce {
        return None;
    }
    let ice = run.ice.get(run.position)?;
    let identity = ice.identity.as_ref()?;
    Some(Encounter {
        install: ice.install_id,
        server: run.server,
        card: Some(identity.card.clone()),
        strength: identity.current_strength,
        subroutines: identity
            .subroutines
            .iter()
            .map(|sub| Subroutine { text: sub.definition.text.clone(), status: sub.status })
            .collect(),
    })
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
                        (Some((now, _, _)), printed) => lines.push(strength_words(now, printed)),
                        (None, Some(printed)) => lines.push(strength_words(printed, Some(printed))),
                        (None, None) => {}
                    }
                    let statuses = encounter(view, id).map(|(_, subs, at)| (subs, at));
                    for (i, sub) in def.subroutines.iter().enumerate() {
                        let status = match statuses.as_ref().and_then(|(subs, at)| at.then(|| subs.get(i)).flatten()) {
                            Some(status) => format!(" — {}", subroutine_word(*status)),
                            None => String::new(),
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
            let hosted: Vec<String> = super::rig::hosted_on(view, id).into_iter().map(|r| card_title(&r.card, registry)).collect();
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
                lines.push(strength_words(rig.current_strength, Some(printed)));
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

    /// A mid-encounter state: `ice` rezzed on HQ and being encountered,
    /// with `statuses` already on its subroutines.
    fn encountering(registry: &CardRegistry, ice: &str, statuses: &[SubroutineStatus]) -> GameState {
        use netrunner_core::dsl::CardType;
        use netrunner_core::rules::{EncounteredSubroutine, InstalledCard, RunIce, RunState, ServerId};

        let mut state = GameState::new(1);
        let definition = registry.get(&CardId(ice.to_string())).expect("ICE in the pool");
        let CardType::Ice(ice_type) = definition.card_type else { panic!("{ice} is not ICE") };
        let install_id = InstallId(1);
        state.corp.installed.push(InstalledCard {
            install_id,
            card: definition.id.clone(),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        });
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                card_id: definition.id.clone(),
                install_id,
                ice_type,
                subroutines: definition
                    .subroutines
                    .iter()
                    .enumerate()
                    .map(|(id, sub)| EncounteredSubroutine { id, definition: sub.clone(), status: statuses.get(id).copied().unwrap_or(SubroutineStatus::Pending) })
                    .collect(),
                rezzed: true,
            }],
            position: 0,
            ..Default::default()
        });
        state
    }

    /// The marks are the three states, in the subroutines' own order,
    /// against the clauses the card prints.
    #[test]
    fn the_encountered_ice_carries_a_mark_on_every_subroutine() {
        use netrunner_core::view::build_client_view;

        let registry: CardRegistry = crate::decks::sample_deck_registry();
        // Three subroutines, so all three marks fit on one piece of ICE.
        let statuses = [SubroutineStatus::Broken, SubroutineStatus::Resolved, SubroutineStatus::Pending];
        let state = encountering(&registry, "bran_1_0", &statuses);
        let printed = &registry.get(&CardId("bran_1_0".into())).unwrap().subroutines;
        assert_eq!(printed.len(), 3, "the fixture wants an ice with three subroutines");

        for side in [Side::Corp, Side::Runner] {
            let view = build_client_view(&state, &registry, side);
            let met = encounter_subroutines(&view).expect("the run is encountering a rezzed ice");
            assert_eq!(met.install, InstallId(1));
            assert_eq!(met.card, Some(CardId("bran_1_0".into())), "a rezzed ice is named to both chairs");
            assert_eq!(met.subroutines.iter().map(|sub| sub.text.clone()).collect::<Vec<_>>(), printed.iter().map(|sub| sub.text.clone()).collect::<Vec<_>>());
            assert_eq!(met.subroutines.iter().map(Subroutine::word).collect::<Vec<_>>(), ["broken", "fired", "pending"], "{side:?}");
        }
    }

    /// The strength line is the number now, with the printed one beside
    /// it only once something has moved it: Ice Wall advanced twice is
    /// "Strength 3 now, 1 printed", and unadvanced is "Strength 1". The
    /// heading's server is the run's.
    #[test]
    fn the_encounter_says_its_strength_and_the_printed_one_when_they_differ() {
        use netrunner_core::view::build_client_view;

        let registry: CardRegistry = crate::decks::sample_deck_registry();
        let mut state = encountering(&registry, "ice_wall", &[]);
        let printed = registry.get(&CardId("ice_wall".into())).unwrap().strength.expect("Ice Wall prints a strength");
        for side in [Side::Corp, Side::Runner] {
            let met = encounter_subroutines(&build_client_view(&state, &registry, side)).expect("encountering");
            assert_eq!(met.server, ServerId::Hq);
            assert_eq!(met.strength_line(&registry), format!("Strength {printed}"), "{side:?}");
        }
        state.corp.installed[0].advancement_tokens = 2;
        for side in [Side::Corp, Side::Runner] {
            let met = encounter_subroutines(&build_client_view(&state, &registry, side)).expect("encountering");
            assert_eq!(met.strength, printed + 2, "Ice Wall gains 1 strength per advancement token");
            assert_eq!(met.strength_line(&registry), format!("Strength {} now, {printed} printed", printed + 2), "{side:?}");
        }
    }

    /// The ice's face is the gate, not the run: a derezzed ice mid-
    /// encounter tells the Runner nothing, while the Corp still reads
    /// its own card. Outside an encounter there is nothing to mark —
    /// the approach included, which is where the routes stop too.
    #[test]
    fn an_ice_the_viewer_cannot_see_is_not_marked_and_nor_is_an_approach() {
        use netrunner_core::view::build_client_view;

        let registry: CardRegistry = crate::decks::sample_deck_registry();
        let mut state = encountering(&registry, "bran_1_0", &[]);
        assert!(encounter_subroutines(&build_client_view(&state, &registry, Side::Runner)).is_some());

        for phase in [RunPhase::ApproachIce, RunPhase::AccessingCard, RunPhase::Success] {
            state.active_run.as_mut().unwrap().phase = phase;
            assert_eq!(encounter_subroutines(&build_client_view(&state, &registry, Side::Runner)), None, "{phase:?}");
        }
        state.active_run.as_mut().unwrap().phase = RunPhase::EncounterIce;

        state.active_run.as_mut().unwrap().ice[0].rezzed = false;
        state.corp.installed[0].rezzed = false;
        assert_eq!(encounter_subroutines(&build_client_view(&state, &registry, Side::Runner)), None, "a derezzed ice reveals nothing");
        assert!(encounter_subroutines(&build_client_view(&state, &registry, Side::Corp)).is_some(), "the Corp reads its own card");

        state.active_run = None;
        assert_eq!(encounter_subroutines(&build_client_view(&state, &registry, Side::Runner)), None, "no run, no marks");
    }

    /// Real games: whenever a view is mid-encounter, the marks are the
    /// same words the sheet lists for that ice — one vocabulary, so a
    /// person reads "broken" in one sense wherever they look.
    #[test]
    fn the_marks_and_the_sheet_say_the_same_words() {
        let mut marked = 0;
        for seed in 0..6u64 {
            let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
            let registry: CardRegistry = crate::decks::sample_deck_registry();
            let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
            let mut corp = HeuristicAgent::new(Side::Corp, seed);
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
                            let Some(met) = encounter_subroutines(&view) else { continue };
                            marked += 1;
                            let facts = install_facts(&view, met.install, &registry).expect("the encountered ice is on the board");
                            let lines: Vec<String> = facts.iter().filter(|line| line.starts_with("» ")).cloned().collect();
                            let marks: Vec<String> = met.subroutines.iter().map(|sub| format!("» {} — {}", sub.text, sub.word())).collect();
                            assert_eq!(lines, marks, "{viewer:?}: the board and the sheet disagree");
                            assert!(facts.iter().any(|line| line.starts_with(&format!("Strength {}", met.strength))), "{facts:?}");
                        }
                    }
                    SessionStep::Applied { .. } => {}
                    SessionStep::Ended { .. } => break,
                    SessionStep::Stalled(reason) => panic!("seed {seed}: {reason:?}"),
                }
            }
        }
        assert!(marked > 0, "an encounter was marked");
    }
}
