//! The words on "where does this card go?" — a card's text installing a
//! card (Scatter Field, Ansel 1.0, Peer Review, Mercia B4LL4RD), where the
//! Corp picks the server after the card.
//!
//! **The choice was there and could not be read.** The engine offers
//! every existing remote *and* a new one (`corp_install_destinations`),
//! and installing an agenda or asset where one already sits trashes the
//! one there — Null Signal Games' one-per-remote rule. A person was shown
//! `Choose Remote(0)` and `Choose Remote(1)`: nothing said Remote 1 did not
//! exist yet, nothing said Remote 0 would lose the asset making them
//! money, and the desktop listed neither under the prompt — a new remote
//! has no column to click, so the one choice that cost nothing was
//! reachable only from the play helper. A person concluded they had to
//! overwrite their best card (15 September 2026).
//!
//! So every destination says what it is and what it costs: `Install in a
//! new remote server`, `Install in Remote 0 — trashes Nico Campaign`,
//! `Install protecting HQ — costs 2 credits`. Read off the masked view
//! alone: the Corp sees its own cards, and the parked install names the
//! card by its position in the zone it comes from.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, CardType, CardZoneRef};
use netrunner_core::rules::{PendingDecision, ServerId};
use netrunner_core::view::{ClientView, ServerView};

use crate::actions::card_title;
use crate::board::action_map::server_name;

/// A parked install-from-a-card-effect, as the Corp choosing reads it.
/// `None` for anyone else, and for a server choice that starts a run.
pub struct Placement<'a> {
    view: &'a ClientView,
    registry: &'a CardRegistry,
    /// Whether the install pays its costs — for ice, the tax — and how
    /// much less (Mercia B4LL4RD), off the parked install.
    pay_cost: bool,
    discount: u32,
    /// The card going in, when the view shows it: HQ and Archives do, R&D
    /// (Poétrï's top three) does not — the view never shows a deck.
    card: Option<CardId>,
    allowed: Option<&'a [ServerId]>,
}

impl<'a> Placement<'a> {
    pub fn of(view: &'a ClientView, registry: &'a CardRegistry) -> Option<Placement<'a>> {
        let Some(PendingDecision::ChooseServer { chooser, install: Some(install), allowed_servers, .. }) = &view.pending_decision else {
            return None;
        };
        if !view.viewer.is(*chooser) {
            return None;
        }
        let card = match install.origin {
            CardZoneRef::OwnHq => view.corp.hq_cards.as_ref().and_then(|hand| hand.get(install.position).cloned()),
            CardZoneRef::OwnArchives => view.corp.archives.get(install.position).and_then(|a| a.card.clone()),
            _ => None,
        };
        Some(Placement { view, registry, pay_cost: install.pay_cost, discount: install.discount, card, allowed: allowed_servers.as_deref() })
    }

    /// The card's title, or "the card" when the view does not show it.
    pub fn card_name(&self) -> String {
        self.card.as_ref().map_or_else(|| "the card".to_string(), |card| card_title(card, self.registry))
    }

    fn card_type(&self) -> Option<&CardType> {
        self.card.as_ref().and_then(|card| self.registry.get(card)).map(|def| &def.card_type)
    }

    fn is_ice(&self) -> bool {
        matches!(self.card_type(), Some(CardType::Ice(_)))
    }

    /// Whether the card takes a remote's one agenda-or-asset place — or
    /// might, when the view does not say what it is.
    fn takes_the_remote(&self) -> Option<bool> {
        self.card_type().map(|t| matches!(t, CardType::Agenda | CardType::Asset))
    }

    fn server(&self, server: ServerId) -> Option<&'a ServerView> {
        self.view.corp.servers.iter().find(|s| s.server == server)
    }

    /// Whether `server` is a remote that does not exist yet — the one the
    /// install would create.
    fn is_new(&self, server: ServerId) -> bool {
        matches!(server, ServerId::Remote(_)) && self.server(server).is_none()
    }

    /// The agenda or asset in `server`'s root, which an agenda or asset
    /// installed there would trash.
    fn occupant(&self, server: ServerId) -> Option<String> {
        self.server(server)?
            .root
            .iter()
            .find(|c| c.card.as_ref().and_then(|id| self.registry.get(id)).is_some_and(|d| matches!(d.card_type, CardType::Agenda | CardType::Asset)))
            .map(|c| c.card.as_ref().map_or_else(|| "the card there".to_string(), |id| card_title(id, self.registry)))
    }

    /// The label of the choice of `server`.
    pub fn label(&self, server: ServerId) -> String {
        if self.is_ice() {
            if self.is_new(server) {
                return "Install protecting a new remote server".to_string();
            }
            let ice = self.server(server).map_or(0, |s| s.ice.len() as u32);
            let tax = if self.pay_cost { ice.saturating_sub(self.discount) } else { 0 };
            let cost = match tax {
                0 => String::new(),
                1 => " — costs 1 credit".to_string(),
                n => format!(" — costs {n} credits"),
            };
            return format!("Install protecting {}{cost}", server_name(server));
        }
        if self.is_new(server) {
            return "Install in a new remote server".to_string();
        }
        let place = match server {
            ServerId::Remote(_) => server_name(server),
            central => format!("the root of {}", server_name(central)),
        };
        match (self.occupant(server), self.takes_the_remote()) {
            (Some(there), Some(true)) => format!("Install in {place} — trashes {there}"),
            (Some(there), None) => format!("Install in {place} — holds {there}"),
            _ => format!("Install in {place}"),
        }
    }

    /// The prompt's heading after the asking card's name.
    pub fn question(&self) -> String {
        format!("where to install {}?", self.card_name())
    }

    /// The line under the prompt: what the choice costs, before a person
    /// makes it. The new remote is named only when it is on offer — past
    /// the engine's ten remotes it is not.
    pub fn detail(&self) -> String {
        let mut lines: Vec<&str> = Vec::new();
        let new_on_offer = self.allowed.is_some_and(|allowed| allowed.iter().any(|s| self.is_new(*s)));
        if new_on_offer {
            lines.push("A new remote server is an option.");
        }
        if !self.pay_cost {
            // Key Performance Indicators, Off the Books: "ignoring all
            // costs", so the ice tax a label would otherwise name is waived.
            lines.push("It is installed ignoring all costs.");
        } else if self.is_ice() {
            lines.push("Ice costs 1 credit for each piece already protecting the server.");
        }
        if !self.is_ice() && self.takes_the_remote() != Some(false) && self.allowed.is_some_and(|allowed| allowed.iter().any(|s| self.occupant(*s).is_some())) {
            lines.push("A remote holds one agenda or asset: installing one where one already is trashes it.");
        }
        lines.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, IceType};
    use netrunner_core::rules::{GamePhase, GameState, InstallId, InstallSlot, InstalledCard, PendingChoiceResume, Side};
    use netrunner_core::view::build_client_view;

    fn card(id: &str, title: &str, card_type: CardType) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), title: title.to_string(), side: Side::Corp, card_type, is_playable: true, ..Default::default() }
    }

    fn registry() -> CardRegistry {
        CardRegistry::from_cards(vec![
            card("nico_campaign", "Nico Campaign", CardType::Asset),
            card("pad_campaign", "PAD Campaign", CardType::Asset),
            card("ice_wall", "Ice Wall", CardType::Ice(IceType::Barrier)),
            card("manegarm", "Manegarm Skunkworks", CardType::Upgrade),
            card("scatter_field", "Scatter Field", CardType::Ice(IceType::CodeGate)),
        ])
    }

    fn id(s: &str) -> CardId {
        CardId(s.to_string())
    }

    /// The Corp with a rezzed Nico Campaign in Remote 0 and one Ice Wall on
    /// HQ, answering Scatter Field's "You may install 1 card from HQ" with
    /// `hand` — parked, confirmed and turned into the server choice by the
    /// engine itself, so the offer labelled is the one it really makes.
    fn parked(hand: &str) -> (CardRegistry, ClientView) {
        use netrunner_core::dsl::{CardFilter, Effect};
        use netrunner_core::rules::{apply_action, PlayerAction};
        let registry = registry();
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.resources.credits.0 = 5;
        state.corp.hq = vec![id(hand)];
        state.corp.installed = vec![
            InstalledCard { install_id: InstallId(1), card: id("nico_campaign"), server: ServerId::Remote(0), slot: InstallSlot::Root, rezzed: true, ..Default::default() },
            InstalledCard { install_id: InstallId(2), card: id("ice_wall"), server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: true, ..Default::default() },
        ];
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnHq,
            filter: CardFilter::Any,
            min: 0,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: Some(Box::new(Effect::PromptInstallCorpCard { origin_zone: CardZoneRef::OwnHq, ignore_costs: false, discount: 0, then: None, remote_only: false })),
            selected: Vec::new(),
            source_card: Some(id("scatter_field")),
            prompting_card: Some(id("scatter_field")),
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).unwrap();
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).unwrap();
        assert!(matches!(state.pending_decision, Some(PendingDecision::ChooseServer { install: Some(_), .. })), "{:?}", state.pending_decision);
        let view = build_client_view(&state, &registry, Side::Corp);
        (registry, view)
    }

    fn offered(view: &ClientView) -> Vec<ServerId> {
        view.legal_actions
            .iter()
            .filter_map(|a| match a {
                netrunner_core::rules::PlayerAction::ChooseServerForPendingDecision { server } => Some(*server),
                _ => None,
            })
            .collect()
    }

    /// The report: an asset from HQ, a remote already earning credits. The
    /// new remote says it is new, and the occupied one says what it costs.
    #[test]
    fn an_asset_is_offered_a_new_remote_and_told_what_an_install_over_trashes() {
        let (registry, view) = parked("pad_campaign");
        assert_eq!(offered(&view), vec![ServerId::Remote(0), ServerId::Remote(1)], "the engine offers the new remote");
        let placement = Placement::of(&view, &registry).expect("the Corp is choosing");
        assert_eq!(placement.label(ServerId::Remote(0)), "Install in Remote 0 — trashes Nico Campaign");
        assert_eq!(placement.label(ServerId::Remote(1)), "Install in a new remote server");
        assert_eq!(placement.question(), "where to install PAD Campaign?");
        assert_eq!(crate::prose::decision_prompt(&view, &registry).as_deref(), Some("Scatter Field asks — where to install PAD Campaign?"), "the terminal pane's title");
        assert_eq!(
            placement.detail(),
            "A new remote server is an option. A remote holds one agenda or asset: installing one where one already is trashes it."
        );
        assert!(Placement::of(&build_client_view_for_runner(&view), &registry).is_none());
    }

    fn build_client_view_for_runner(view: &ClientView) -> ClientView {
        let mut runner = view.clone();
        runner.viewer = Side::Runner.into();
        runner
    }

    /// Ice says what the install tax is where there is one, and an upgrade
    /// sits beside an asset without trashing it.
    #[test]
    fn ice_names_its_tax_and_an_upgrade_trashes_nothing() {
        let (registry, view) = parked("ice_wall");
        assert_eq!(offered(&view), vec![ServerId::Hq, ServerId::RnD, ServerId::Archives, ServerId::Remote(0), ServerId::Remote(1)]);
        let placement = Placement::of(&view, &registry).unwrap();
        assert_eq!(placement.label(ServerId::Hq), "Install protecting HQ — costs 1 credit");
        assert_eq!(placement.label(ServerId::Remote(0)), "Install protecting Remote 0");
        assert_eq!(placement.label(ServerId::Remote(1)), "Install protecting a new remote server");

        let (registry, view) = parked("manegarm");
        let placement = Placement::of(&view, &registry).unwrap();
        assert_eq!(placement.label(ServerId::Hq), "Install in the root of HQ");
        assert_eq!(placement.label(ServerId::Remote(0)), "Install in Remote 0");
        assert_eq!(placement.detail(), "A new remote server is an option.");
    }
}
