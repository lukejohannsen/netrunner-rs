//! The two predicates a lesson step is written in: which of the player's
//! legal actions the step *allows*, and which observed event *advances* it.
//!
//! Both are authored in lesson JSON, so both are data, not code — the same
//! house rule cards follow, and for the same reason: a lesson author edits
//! JSON and never touches the engine.
//!
//! Neither predicate re-derives legality. An `ActionPredicate` is only ever
//! applied to `view.legal_actions` (see `LessonProgress::allowed`), so it can
//! narrow what a client presents but can never make an illegal action
//! legal — ROADMAP Phase 1.75 §6's rule, enforced by the type: `matches`
//! takes an action and says yes or no, and nothing here can construct one.

use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::{GameEvent, InstallId, PlayerAction, ServerId, Side};
use crate::view::ClientView;

/// Which of the learner's legal actions a step lets through.
///
/// Actions are matched by variant name (`"PlayOperation"`) rather than by a
/// full `PlayerAction` literal, because most of a lesson's steps care about
/// *what kind* of thing the learner does — "install a card", "run a server"
/// — and a literal would also have to name a `Side` or an `InstallId` the
/// author cannot know in advance. `Card` and `Server` narrow a kind to one
/// card or one server when the lesson does care.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActionPredicate {
    /// Every legal action — a step that only watches for an event.
    Any,
    /// Any action of this variant, e.g. `"GainCreditClick"`. Validated
    /// against `PlayerAction::VARIANT_NAMES` by `Lesson::validate`.
    Kind(String),
    /// An action naming this card, optionally restricted to one variant.
    /// A card is named directly (`card_id`, `ice_id`) or through an
    /// `InstallId` (`RezIce`, `AdvanceCard`, `ScoreAgenda`, …) resolved
    /// against the learner's own view — their own installs are always
    /// identifiable there, so a lesson never hardcodes an install handle.
    Card {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        card: CardId,
    },
    /// An action naming this server (`InitiateRun`, `InstallCard`'s zone,
    /// `ChooseServerForPendingDecision`), optionally restricted to one
    /// variant.
    Server {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        server: ServerId,
    },
    /// Any of these.
    AnyOf(Vec<ActionPredicate>),
}

impl ActionPredicate {
    /// Whether `action` — one of `view.legal_actions` — passes.
    pub fn matches(&self, action: &PlayerAction, view: &ClientView) -> bool {
        match self {
            ActionPredicate::Any => true,
            ActionPredicate::Kind(kind) => action.variant_name() == *kind,
            ActionPredicate::Card { kind, card } => {
                kind.as_ref().is_none_or(|kind| action.variant_name() == *kind)
                    && action_card(action, view).as_ref() == Some(card)
            }
            ActionPredicate::Server { kind, server } => {
                kind.as_ref().is_none_or(|kind| action.variant_name() == *kind)
                    && action_server(action) == Some(*server)
            }
            ActionPredicate::AnyOf(options) => options.iter().any(|option| option.matches(action, view)),
        }
    }

    /// Every variant name this predicate mentions, for validation.
    pub fn kinds(&self) -> Vec<&str> {
        match self {
            ActionPredicate::Any => Vec::new(),
            ActionPredicate::Kind(kind) => vec![kind.as_str()],
            ActionPredicate::Card { kind, .. } | ActionPredicate::Server { kind, .. } => {
                kind.iter().map(String::as_str).collect()
            }
            ActionPredicate::AnyOf(options) => options.iter().flat_map(ActionPredicate::kinds).collect(),
        }
    }
}

/// The card an action names, if any: directly, or through an install
/// handle resolved against `view`. `None` for an action that names no card
/// and for a handle the viewer cannot identify (an opponent's unrezzed
/// install) — such an action never matches a `Card` predicate.
pub fn action_card(action: &PlayerAction, view: &ClientView) -> Option<CardId> {
    match action {
        PlayerAction::InstallCard { card_id, .. }
        | PlayerAction::PlayEvent { card_id }
        | PlayerAction::PlayOperation { card_id }
        | PlayerAction::InstallHardware { card_id }
        | PlayerAction::InstallProgram { card_id }
        | PlayerAction::InstallResource { card_id }
        | PlayerAction::InstallProgramOnIce { card_id, .. }
        | PlayerAction::DiscardCard { card_id }
        | PlayerAction::SelectCardToAccess { card_id }
        | PlayerAction::StealAgenda { card_id }
        | PlayerAction::TrashAccessedCard { card_id }
        | PlayerAction::PassAccessedCard { card_id }
        | PlayerAction::PayAccessTrigger { card_id }
        | PlayerAction::DeclineAccessTrigger { card_id } => Some(card_id.clone()),
        PlayerAction::BreakSubroutineWithClick { ice_id, .. } => Some(ice_id.clone()),
        PlayerAction::RezIce { ice } => resolve_install(view, *ice),
        PlayerAction::ActivateAbility { target, .. }
        | PlayerAction::AdvanceCard { target }
        | PlayerAction::ScoreAgenda { target }
        | PlayerAction::TrashResource { target } => resolve_install(view, *target),
        _ => None,
    }
}

/// The server an action names, if any.
pub fn action_server(action: &PlayerAction) -> Option<ServerId> {
    match action {
        PlayerAction::InstallCard { zone, .. } => Some(*zone),
        PlayerAction::InitiateRun { server } | PlayerAction::ChooseServerForPendingDecision { server } => Some(*server),
        _ => None,
    }
}

/// The card sitting at `install` as this viewer sees it — the same walk
/// `netrunner_cli`'s `describe_action` does to label an install handle.
pub fn resolve_install(view: &ClientView, install: InstallId) -> Option<CardId> {
    for server in &view.corp.servers {
        for card in server.ice.iter().chain(server.root.iter()) {
            if card.install_id == install {
                return card.card.clone();
            }
        }
    }
    view.runner.rig.iter().find(|card| card.install_id == install).map(|card| card.card.clone())
}

/// Which observed `GameEvent` moves a step on.
///
/// Matched by variant name for the same reason as `ActionPredicate`; the
/// three narrowing forms cover the payloads lessons distinguish on (which
/// card, which server, whose turn). Event kinds are not validated against
/// a name table — see `GameEvent::variant_name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EventPredicate {
    /// Any event at all — the step advances on the learner's next action.
    Any,
    /// Any event of this variant, e.g. `"OperationPlayed"`.
    Kind(String),
    /// An event of this variant naming this card. An event that carries no
    /// card (see `event_card`) never matches.
    Card { kind: String, card: CardId },
    /// An event of this variant naming this server.
    Server { kind: String, server: ServerId },
    /// An event of this variant attributed to this side.
    Side { kind: String, side: Side },
    /// Any of these.
    AnyOf(Vec<EventPredicate>),
}

impl EventPredicate {
    pub fn matches(&self, event: &GameEvent) -> bool {
        match self {
            EventPredicate::Any => true,
            EventPredicate::Kind(kind) => event.variant_name() == *kind,
            EventPredicate::Card { kind, card } => event.variant_name() == *kind && event_card(event) == Some(card),
            EventPredicate::Server { kind, server } => event.variant_name() == *kind && event_server(event) == Some(*server),
            EventPredicate::Side { kind, side } => event.variant_name() == *kind && event_side(event) == Some(*side),
            EventPredicate::AnyOf(options) => options.iter().any(|option| option.matches(event)),
        }
    }
}

/// The card an event is about, for the variants that carry one. Events
/// about several cards (`CardsSelected`, `VirusCountersPurged`) or none
/// return `None` and never match a `Card` predicate; a lesson that needs
/// one of those matches on `Kind` instead.
pub fn event_card(event: &GameEvent) -> Option<&CardId> {
    match event {
        GameEvent::IceEncountered { card_id, .. }
        | GameEvent::SubroutineBroken { card_id, .. }
        | GameEvent::SubroutineFired { card_id, .. }
        | GameEvent::IceStrengthModified { card_id, .. }
        | GameEvent::AbilityActivated { card_id, .. }
        | GameEvent::StrengthBoosted { card_id, .. } => Some(card_id),
        GameEvent::CardInstalled { card, .. }
        | GameEvent::IceRezzed { card, .. }
        | GameEvent::CardDerezzed { card }
        | GameEvent::EventPlayed { card, .. }
        | GameEvent::OperationPlayed { card, .. }
        | GameEvent::HardwareInstalled { card, .. }
        | GameEvent::ProgramInstalled { card, .. }
        | GameEvent::ResourceInstalled { card, .. }
        | GameEvent::CardAccessed { card, .. }
        | GameEvent::CardDiscarded { card, .. }
        | GameEvent::AgendaStolen { card, .. }
        | GameEvent::CardTrashed { card, .. }
        | GameEvent::CardRemovedFromGame { card, .. }
        | GameEvent::CardAdvanced { card, .. }
        | GameEvent::CardTrashedFromAccess { card, .. }
        | GameEvent::AccessPassed { card }
        | GameEvent::TriggerOrderChosen { card, .. }
        | GameEvent::TriggerFired { card, .. }
        | GameEvent::AgendaScored { card, .. }
        | GameEvent::CountersAdded { card, .. }
        | GameEvent::CountersRemoved { card, .. } => Some(card),
        _ => None,
    }
}

/// The server an event is about, for the variants that carry one.
pub fn event_server(event: &GameEvent) -> Option<ServerId> {
    match event {
        GameEvent::IceApproached { server, .. }
        | GameEvent::IcePassed { server, .. }
        | GameEvent::ServerApproached { server }
        | GameEvent::RunSucceeded { server }
        | GameEvent::RunJackedOut { server }
        | GameEvent::RunCompleted { server }
        | GameEvent::CardInstalled { server, .. }
        | GameEvent::IceRezzed { server, .. }
        | GameEvent::RunInitiated { server }
        | GameEvent::CardAccessed { server, .. }
        | GameEvent::RunEndedByEffect { server }
        | GameEvent::AdditionalAccessGranted { server, .. }
        | GameEvent::AccessReplacementSet { server }
        | GameEvent::AccessReplaced { server }
        | GameEvent::AgendaScored { server, .. } => Some(*server),
        _ => None,
    }
}

/// The side an event is attributed to, for the variants that carry one.
pub fn event_side(event: &GameEvent) -> Option<Side> {
    match event {
        GameEvent::ClickSpent { side }
        | GameEvent::CreditsGained { side, .. }
        | GameEvent::CardDrawn { side }
        | GameEvent::CardInstalled { side, .. }
        | GameEvent::EventPlayed { side, .. }
        | GameEvent::OperationPlayed { side, .. }
        | GameEvent::HardwareInstalled { side, .. }
        | GameEvent::ProgramInstalled { side, .. }
        | GameEvent::ResourceInstalled { side, .. }
        | GameEvent::TurnEnded { side }
        | GameEvent::TurnStarted { side, .. }
        | GameEvent::DiscardPending { side, .. }
        | GameEvent::DiscardPhaseEnded { side }
        | GameEvent::CardDiscarded { side, .. }
        | GameEvent::CreditsSpent { side, .. }
        | GameEvent::TagsGiven { side, .. }
        | GameEvent::TagsCleared { side }
        | GameEvent::CardTrashed { side, .. }
        | GameEvent::CardRemovedFromGame { side, .. }
        | GameEvent::AbilityActivated { side, .. }
        | GameEvent::PaidAbilityWindowOpened { side }
        | GameEvent::PriorityPassed { side }
        | GameEvent::TagRemoved { side }
        | GameEvent::TagsRemoved { side, .. }
        | GameEvent::CardsSelected { side, .. }
        | GameEvent::PendingCardSelectionOffered { side, .. }
        | GameEvent::HandKept { side }
        | GameEvent::MulliganTaken { side }
        | GameEvent::CreditsLost { side, .. }
        | GameEvent::ClicksLost { side, .. }
        | GameEvent::ClicksGained { side, .. }
        | GameEvent::MaxHandSizeGained { side, .. }
        | GameEvent::BasicDrawActionTaken { side }
        | GameEvent::PendingPaidChoiceOffered { side }
        | GameEvent::PendingPaidChoiceAccepted { side }
        | GameEvent::PendingPaidChoiceDeclined { side } => Some(*side),
        GameEvent::GameOver { winner } => Some(*winner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{GamePhase, GameState, InstallSlot, InstalledCard};
    use crate::cards::CardRegistry;
    use crate::view::build_client_view;

    fn id(s: &str) -> CardId {
        CardId(s.to_string())
    }

    /// A Corp view with one of its own cards installed in a remote, so an
    /// install handle resolves to a card the viewer may name.
    fn corp_view_with_install() -> (ClientView, InstallId) {
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Corp);
        let install = state.allocate_install_id();
        state.corp.installed.push(InstalledCard {
            install_id: install,
            card: id("offworld_office"),
            server: ServerId::Remote(1),
            slot: InstallSlot::Root,
            ..InstalledCard::default()
        });
        (build_client_view(&state, &CardRegistry::new(), Side::Corp), install)
    }

    #[test]
    fn kind_matches_the_variant_name_only() {
        let (view, _) = corp_view_with_install();
        let predicate = ActionPredicate::Kind("GainCreditClick".to_string());
        assert!(predicate.matches(&PlayerAction::GainCreditClick { side: Side::Corp }, &view));
        assert!(predicate.matches(&PlayerAction::GainCreditClick { side: Side::Runner }, &view));
        assert!(!predicate.matches(&PlayerAction::DrawCardClick { side: Side::Corp }, &view));
    }

    #[test]
    fn card_matches_directly_and_through_an_install_handle() {
        let (view, install) = corp_view_with_install();
        let predicate = ActionPredicate::Card { kind: None, card: id("offworld_office") };
        assert!(predicate.matches(&PlayerAction::AdvanceCard { target: install }, &view));
        assert!(predicate.matches(&PlayerAction::ScoreAgenda { target: install }, &view));
        assert!(predicate.matches(
            &PlayerAction::InstallCard { card_id: id("offworld_office"), zone: ServerId::Remote(2), slot: InstallSlot::Root },
            &view
        ));
        assert!(!predicate.matches(&PlayerAction::AdvanceCard { target: InstallId(99) }, &view), "an unknown handle names no card");
        let narrowed = ActionPredicate::Card { kind: Some("ScoreAgenda".to_string()), card: id("offworld_office") };
        assert!(narrowed.matches(&PlayerAction::ScoreAgenda { target: install }, &view));
        assert!(!narrowed.matches(&PlayerAction::AdvanceCard { target: install }, &view));
    }

    #[test]
    fn server_and_any_of_match_as_expected() {
        let (view, _) = corp_view_with_install();
        let hq = ActionPredicate::Server { kind: Some("InitiateRun".to_string()), server: ServerId::Hq };
        assert!(hq.matches(&PlayerAction::InitiateRun { server: ServerId::Hq }, &view));
        assert!(!hq.matches(&PlayerAction::InitiateRun { server: ServerId::RnD }, &view));
        let either = ActionPredicate::AnyOf(vec![hq, ActionPredicate::Kind("EndTurn".to_string())]);
        assert!(either.matches(&PlayerAction::EndTurn, &view));
        assert!(!either.matches(&PlayerAction::JackOut, &view));
        assert_eq!(either.kinds(), vec!["InitiateRun", "EndTurn"]);
    }

    #[test]
    fn event_predicates_narrow_on_card_server_and_side() {
        let scored = GameEvent::AgendaScored { card: id("offworld_office"), agenda_points: 2, server: ServerId::Remote(1) };
        assert!(EventPredicate::Kind("AgendaScored".to_string()).matches(&scored));
        assert!(EventPredicate::Card { kind: "AgendaScored".to_string(), card: id("offworld_office") }.matches(&scored));
        assert!(!EventPredicate::Card { kind: "AgendaScored".to_string(), card: id("send_a_message") }.matches(&scored));
        assert!(EventPredicate::Server { kind: "AgendaScored".to_string(), server: ServerId::Remote(1) }.matches(&scored));
        let ended = GameEvent::TurnEnded { side: Side::Corp };
        assert!(EventPredicate::Side { kind: "TurnEnded".to_string(), side: Side::Corp }.matches(&ended));
        assert!(!EventPredicate::Side { kind: "TurnEnded".to_string(), side: Side::Runner }.matches(&ended));
        assert!(!EventPredicate::Card { kind: "TurnEnded".to_string(), card: id("x") }.matches(&ended), "an event with no card never matches a Card predicate");
        assert!(EventPredicate::Any.matches(&ended));
    }

    #[test]
    fn predicates_round_trip_through_json_in_the_authored_shape() {
        let json = r#"{"Card":{"kind":"PlayOperation","card":"hedge_fund"}}"#;
        let predicate: ActionPredicate = serde_json::from_str(json).unwrap();
        assert_eq!(predicate, ActionPredicate::Card { kind: Some("PlayOperation".to_string()), card: id("hedge_fund") });
        let any: ActionPredicate = serde_json::from_str(r#""Any""#).unwrap();
        assert_eq!(any, ActionPredicate::Any);
        let event: EventPredicate = serde_json::from_str(r#"{"Side":{"kind":"TurnEnded","side":"Corp"}}"#).unwrap();
        assert_eq!(event, EventPredicate::Side { kind: "TurnEnded".to_string(), side: Side::Corp });
        assert!(serde_json::from_str::<ActionPredicate>(r#"{"Card":{"kind":"X","card":"y","extra":1}}"#).is_err());
    }
}
