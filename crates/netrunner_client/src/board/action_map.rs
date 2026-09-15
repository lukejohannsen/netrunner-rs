//! Every legal action, labelled and tied to the thing on the board a
//! person would click to mean it.
//!
//! **The flat list is the contract; the board is a convenience.** A
//! client must be able to offer every element of `view.legal_actions`
//! from one panel, because a card the layout could not place is still
//! playable (AGENTS.md §5). So the map is the legal list in order, each
//! entry with its label (`actions::describe_action`, the words the
//! terminal uses) and the targets it belongs to — a hand card, an
//! installed card, a server, a selection position, or nothing but the
//! panel. Clicking a target is a *second* route to the same entry; the
//! `for_*` lookups are indices into the one list, never a second list.
//!
//! Nothing here decides legality: an action is in the map because the
//! engine put it in `legal_actions`, and `MatchHandle::submit` sends it
//! back unfiltered.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{
    GamePhase, InstallId, PendingDecision, PendingPreventionKind, PlayerAction, PublicAccessPhase, RunPhase, ServerId, Side,
};
use netrunner_core::view::ClientView;

use crate::actions::{card_title, describe_action, explain_action};
use crate::prose;

/// The thing on the board an entry belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A card in the viewer's own hand.
    HandCard(CardId),
    /// An installed card, by the handle both sides can see.
    Install(InstallId),
    /// A side's identity card: its abilities are activated by the
    /// engine's identity handle, which is not on the board.
    Identity(Side),
    /// A server: a run on it, an install into it, a choice of it.
    Server(ServerId),
    /// A position in the zone a `ChooseCards` prompt selects from.
    Position(usize),
}

/// One legal action, as the panel lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionEntry {
    pub action: PlayerAction,
    /// `describe_action`'s label.
    pub label: String,
    /// The targets a click could mean this by; empty for an action that
    /// belongs to nothing on the board (end turn, keep hand, pass).
    pub targets: Vec<Target>,
}

/// `view.legal_actions`, labelled and indexed by target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActionMap {
    pub entries: Vec<ActionEntry>,
}

impl ActionMap {
    pub fn build(view: &ClientView, registry: &CardRegistry) -> Self {
        let entries = view
            .legal_actions
            .iter()
            .map(|action| ActionEntry { action: action.clone(), label: describe_action(action, registry, Some(view)), targets: targets_of(action) })
            .collect();
        Self { entries }
    }

    /// One sentence on what entry `index` does, for a tooltip or a
    /// coaching line.
    pub fn explain(&self, index: usize, registry: &CardRegistry, view: &ClientView) -> Option<String> {
        self.entries.get(index).map(|entry| explain_action(&entry.action, registry, Some(view)))
    }

    fn with_target(&self, wanted: &Target) -> Vec<usize> {
        self.entries.iter().enumerate().filter(|(_, entry)| entry.targets.contains(wanted)).map(|(i, _)| i).collect()
    }

    pub fn for_hand_card(&self, card: &CardId) -> Vec<usize> {
        self.with_target(&Target::HandCard(card.clone()))
    }

    pub fn for_install(&self, id: InstallId) -> Vec<usize> {
        self.with_target(&Target::Install(id))
    }

    pub fn for_server(&self, server: ServerId) -> Vec<usize> {
        self.with_target(&Target::Server(server))
    }

    pub fn for_identity(&self, side: Side) -> Vec<usize> {
        self.with_target(&Target::Identity(side))
    }

    pub fn for_position(&self, position: usize) -> Vec<usize> {
        self.with_target(&Target::Position(position))
    }

    /// The entries no click on the board reaches — only the panel does.
    pub fn globals(&self) -> Vec<usize> {
        self.entries.iter().enumerate().filter(|(_, entry)| entry.targets.is_empty()).map(|(i, _)| i).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Where a click could mean `action`. Two targets where the action names
/// two things (an install names the card and the server; a trojan the
/// card and its host), so either click offers it.
fn targets_of(action: &PlayerAction) -> Vec<Target> {
    match action {
        PlayerAction::InstallCard { card_id, zone, .. } => vec![Target::HandCard(card_id.clone()), Target::Server(*zone)],
        PlayerAction::InstallProgramOnIce { card_id, host } => vec![Target::HandCard(card_id.clone()), Target::Install(*host)],
        PlayerAction::PlayEvent { card_id }
        | PlayerAction::PlayOperation { card_id }
        | PlayerAction::InstallHardware { card_id }
        | PlayerAction::InstallProgram { card_id }
        | PlayerAction::InstallResource { card_id }
        | PlayerAction::DiscardCard { card_id } => vec![Target::HandCard(card_id.clone())],
        PlayerAction::ActivateAbility { target, .. } if *target == InstallId::CORP_IDENTITY => vec![Target::Identity(Side::Corp)],
        PlayerAction::ActivateAbility { target, .. } if *target == InstallId::RUNNER_IDENTITY => vec![Target::Identity(Side::Runner)],
        PlayerAction::RezIce { ice: target }
        | PlayerAction::ActivateAbility { target, .. }
        | PlayerAction::AdvanceCard { target }
        | PlayerAction::ScoreAgenda { target }
        | PlayerAction::TrashResource { target } => vec![Target::Install(*target)],
        PlayerAction::InitiateRun { server } | PlayerAction::ChooseServerForPendingDecision { server } => vec![Target::Server(*server)],
        PlayerAction::ToggleCardSelection { position } => vec![Target::Position(*position)],
        // The run and access actions belong to the run panel, which is
        // always drawn while a run is on; the accessed card is named on
        // it, not on the board.
        PlayerAction::GainCreditClick { .. }
        | PlayerAction::DrawCardClick { .. }
        | PlayerAction::ContinueRun
        | PlayerAction::JackOut
        | PlayerAction::CompleteRun
        | PlayerAction::BreakSubroutineWithClick { .. }
        | PlayerAction::EndTurn
        | PlayerAction::KeepHand
        | PlayerAction::TakeMulligan
        | PlayerAction::RemoveTag
        | PlayerAction::PurgeVirusCounters
        | PlayerAction::ChooseTriggerToResolve { .. }
        | PlayerAction::SelectCardToAccess { .. }
        | PlayerAction::StealAgenda { .. }
        | PlayerAction::TrashAccessedCard { .. }
        | PlayerAction::PassAccessedCard { .. }
        | PlayerAction::PayAccessTrigger { .. }
        | PlayerAction::DeclineAccessTrigger { .. }
        | PlayerAction::PassPriority { .. }
        | PlayerAction::SubmitCorpTraceBid { .. }
        | PlayerAction::SubmitRunnerTraceBid { .. }
        | PlayerAction::AcceptPendingPaidChoice { .. }
        | PlayerAction::DeclinePendingPaidChoice
        | PlayerAction::ResolvePendingChoice { .. }
        | PlayerAction::ConfirmCardSelection => Vec::new(),
    }
}

/// What the game is asking the viewer right now, in one line, with the
/// card's own words where a card is asking (the Linked Clause Rule: the
/// printed clause, never a rendering of the DSL, when the card has one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub title: String,
    /// The clause or the detail under the title; empty when the title is
    /// the whole prompt.
    pub detail: String,
}

impl Prompt {
    /// The prompt for `view`, or `None` when it is an ordinary action
    /// phase and the panel needs no heading.
    pub fn of(view: &ClientView, registry: &CardRegistry) -> Option<Prompt> {
        let title = |card: &Option<CardId>| card.as_ref().map_or_else(|| "A card".to_string(), |id| card_title(id, registry));
        let asked_by = |prompting: &Option<CardId>, source: &Option<CardId>| title(if prompting.is_some() { prompting } else { source });
        if let Some(decision) = &view.pending_decision {
            return Some(match decision {
                PendingDecision::ChooseEffect { source_card, prompting_card, .. } => {
                    Prompt { title: format!("{}: choose", asked_by(prompting_card, source_card)), detail: String::new() }
                }
                PendingDecision::ChooseCards { min, max, selected, source_card, prompting_card, .. } => {
                    let range = if min == max { format!("{min}") } else { format!("{min} to {max}") };
                    Prompt {
                        title: format!("{}: choose {range} card{}", asked_by(prompting_card, source_card), if *max == 1 { "" } else { "s" }),
                        detail: format!("{} selected", selected.len()),
                    }
                }
                PendingDecision::ChooseTriggerOrder { .. } => Prompt { title: "Choose which triggers first".to_string(), detail: String::new() },
                PendingDecision::ChooseServer { source_card, prompting_card, .. } => {
                    Prompt { title: format!("{}: choose a server", asked_by(prompting_card, source_card)), detail: String::new() }
                }
            });
        }
        if let Some(paid) = &view.pending_paid_choice {
            let detail = match &paid.text {
                Some(text) => text.clone(),
                None => format!("Pay {} to {}", prose::describe_cost(&paid.cost), prose::describe_effect(&paid.if_paid, registry)),
            };
            return Some(Prompt { title: format!("{}: pay?", asked_by(&paid.prompting_card, &paid.source_card)), detail });
        }
        if let Some(prevention) = &view.pending_prevention {
            let title = match &prevention.kind {
                PendingPreventionKind::Damage { damage_type, amount, prevented } => {
                    format!("Prevent {} {} damage? ({prevented} prevented)", amount, format!("{damage_type:?}").to_lowercase())
                }
                PendingPreventionKind::Trash { .. } => "Prevent the trash?".to_string(),
            };
            return Some(Prompt { title, detail: format!("{} offers to", title_of(prevention.source_card.as_ref(), registry)) });
        }
        if let Some(trace) = &view.active_trace {
            let detail = match trace.corp_bid {
                Some(bid) => format!("The Corp bid {bid}; beat it with link and credits"),
                None => "The Corp bids first".to_string(),
            };
            return Some(Prompt { title: format!("Trace, base strength {}", trace.base_strength), detail });
        }
        if let Some(run) = &view.active_run {
            if let Some(access) = &run.access_state {
                match &access.phase {
                    PublicAccessPhase::PendingChoice { card, trash_cost, mandatory_steal, .. } => {
                        let mut detail = String::new();
                        if let Some(cost) = trash_cost {
                            detail = format!("Trash cost {cost}");
                        }
                        if *mandatory_steal {
                            detail = "An agenda: it must be stolen".to_string();
                        }
                        return Some(Prompt { title: format!("Accessing {}", title(card)), detail });
                    }
                    PublicAccessPhase::PendingInteractiveTrigger { card, cost, decider, .. } => {
                        return Some(Prompt {
                            title: format!("{} asks {decider:?} to pay {}", title(card), prose::describe_cost(cost)),
                            detail: String::new(),
                        });
                    }
                    PublicAccessPhase::SelectNextCard { .. } => {
                        return Some(Prompt { title: format!("Breaching {}: choose a card to access", server_name(run.server)), detail: String::new() });
                    }
                }
            }
            let phase = match run.phase {
                RunPhase::Initiation => "starting",
                RunPhase::ApproachIce => "approaching ice",
                RunPhase::EncounterIce => "encountering ice",
                RunPhase::AccessingCard => "accessing",
                RunPhase::Success => "successful",
                RunPhase::Ended => "over",
            };
            return Some(Prompt { title: format!("Run on {}: {phase}", server_name(run.server)), detail: String::new() });
        }
        if let Some(window) = &view.paid_ability_window {
            return Some(Prompt { title: format!("Paid ability window: {:?} has priority", window.active_priority), detail: String::new() });
        }
        match view.phase {
            GamePhase::Mulligan(side) if Some(side) == view.viewer.side() => Some(Prompt { title: "Keep this hand, or mulligan once".to_string(), detail: String::new() }),
            GamePhase::Discard { side, .. } if Some(side) == view.viewer.side() => Some(Prompt { title: "Discard down to your hand size".to_string(), detail: String::new() }),
            _ => None,
        }
    }
}

fn title_of(card: Option<&CardId>, registry: &CardRegistry) -> String {
    card.map_or_else(|| "A card".to_string(), |id| card_title(id, registry))
}

/// A server as a person names it. Remotes keep the engine's numbering
/// — `Remote 0` is the one every action label and log line calls
/// `Remote(0)` — because a header that says 1 over a panel that says
/// `Run Remote(0)` is two names for one server.
pub fn server_name(server: ServerId) -> String {
    match server {
        ServerId::Hq => "HQ".to_string(),
        ServerId::RnD => "R&D".to_string(),
        ServerId::Archives => "Archives".to_string(),
        ServerId::Remote(n) => format!("Remote {n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::GameState;
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};

    /// The whole map, over real games: every legal action is exactly one
    /// entry, in the engine's order, and every target names something
    /// the viewer's own view shows — a hand card in the visible hand, an
    /// install on the board, a server that exists or a central.
    #[test]
    fn every_legal_action_is_one_entry_and_every_target_is_on_the_board() {
        let mut targeted = 0;
        let mut prompts = 0;
        for seed in 0..4u64 {
            let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
            let registry = crate::decks::sample_deck_registry();
            let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
            let mut agents = [RandomAgent::new(seed), RandomAgent::new(seed + 100)];
            loop {
                match session.step() {
                    SessionStep::Awaiting { side, view } => {
                        let map = ActionMap::build(&view, &registry);
                        assert_eq!(map.entries.len(), view.legal_actions.len());
                        for (entry, action) in map.entries.iter().zip(&view.legal_actions) {
                            assert_eq!(&entry.action, action, "in the engine's order");
                            assert!(!entry.label.is_empty());
                            for target in &entry.targets {
                                targeted += 1;
                                match target {
                                    Target::HandCard(card) => {
                                        let hand = match side {
                                            Side::Corp => view.corp.hq_cards.as_ref(),
                                            Side::Runner => view.runner.grip_cards.as_ref(),
                                        };
                                        assert!(hand.is_some_and(|hand| hand.contains(card)), "seed {seed}: {action:?} targets a card not in the visible hand");
                                    }
                                    Target::Install(id) => {
                                        let on_board = view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).any(|c| c.install_id == *id)
                                            || view.runner.rig.iter().any(|c| c.install_id == *id);
                                        assert!(on_board, "seed {seed}: {action:?} targets an install the view does not show");
                                    }
                                    Target::Server(server) => {
                                        // An install, or a card that installs, may name the
                                        // remote it would create.
                                        let exists = !matches!(server, ServerId::Remote(_))
                                            || view.corp.servers.iter().any(|s| s.server == *server)
                                            || matches!(action, PlayerAction::InstallCard { .. } | PlayerAction::ChooseServerForPendingDecision { .. });
                                        assert!(exists, "seed {seed}: {action:?} targets a server that does not exist");
                                    }
                                    Target::Position(_) => assert!(matches!(view.pending_decision, Some(PendingDecision::ChooseCards { .. }))),
                                    Target::Identity(owner) => {
                                        let shown = match owner {
                                            Side::Corp => view.corp.identity.is_some(),
                                            Side::Runner => view.runner.identity.is_some(),
                                        };
                                        assert!(shown, "seed {seed}: {action:?} targets an identity the view does not carry");
                                    }
                                }
                            }
                        }
                        assert_eq!(map.for_hand_card(&CardId("no_such_card".to_string())), Vec::<usize>::new());
                        if Prompt::of(&view, &registry).is_some() {
                            prompts += 1;
                        }
                        let index = match side {
                            Side::Corp => 0,
                            Side::Runner => 1,
                        };
                        use netrunner_bots::BotAgent;
                        let action = agents[index].select_action(&view, &registry);
                        session.submit(action).unwrap();
                    }
                    SessionStep::Applied { .. } => {}
                    SessionStep::Ended { .. } => break,
                    SessionStep::Stalled(reason) => panic!("seed {seed}: {reason:?}"),
                }
            }
        }
        assert!(targeted > 100, "{targeted} targeted entries: the board route is reachable");
        assert!(prompts > 10, "{prompts} prompts: the mulligan alone is two a game");
    }

    /// A click on a hand card and a click on a server reach the same
    /// install entry, and the panel's index is the one list's.
    #[test]
    fn an_install_is_reached_from_its_card_and_from_its_server() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
        // Keep both hands, then take the first legal action until the Corp
        // is offered an install (an opening hand can be five operations).
        let view = loop {
            match session.step() {
                SessionStep::Awaiting { view, .. } if matches!(view.phase, GamePhase::Mulligan(_)) => session.submit(PlayerAction::KeepHand).unwrap(),
                SessionStep::Awaiting { side: Side::Corp, view } if view.legal_actions.iter().any(|a| matches!(a, PlayerAction::InstallCard { .. })) => break view,
                SessionStep::Awaiting { view, .. } => session.submit(view.legal_actions[0].clone()).unwrap(),
                SessionStep::Applied { .. } => {}
                other => panic!("{other:?}"),
            }
        };
        let map = ActionMap::build(&view, &registry);
        let install = map.entries.iter().position(|e| matches!(e.action, PlayerAction::InstallCard { .. })).expect("a Corp can install something");
        let PlayerAction::InstallCard { card_id, zone, .. } = &map.entries[install].action else { unreachable!() };
        assert!(map.for_hand_card(card_id).contains(&install));
        assert!(map.for_server(*zone).contains(&install));
        assert!(map.globals().iter().any(|i| matches!(map.entries[*i].action, PlayerAction::EndTurn)), "End turn is panel-only");
        assert!(map.explain(install, &registry, &view).unwrap().contains("install"));
        assert_eq!(Prompt::of(&view, &registry), None, "an ordinary action phase has no heading");
    }
}
