//! The words on a card-selection prompt: which card each
//! `ToggleCardSelection` is, where it sits, what is chosen so far, and
//! which buttons say the same thing twice.
//!
//! **A position is not a card.** The engine selects by position into the
//! prompt's zone — two copies of one card are two positions, and an
//! opponent's unrezzed ICE can be chosen without being named — and both
//! clients used to print that position: `Toggle selection of card 3`. The
//! engine now publishes the prompt's candidates to the chooser
//! (`ClientView::selection`, masked there), and this module turns them
//! into what a person reads: `Select Hedge Fund`, `Select Ice Wall
//! (protecting HQ)` when there is another Ice Wall somewhere else,
//! `Confirm Hedge Fund`, and — when only one card may be chosen and it
//! is — `Select a different card`.
//!
//! **Identical copies in one place are one button** (a decision taken with
//! the person, 15 September 2026). A grip holding two Sure Gambles offers
//! `Select Sure Gamble` once, and once it is taken, `Select another Sure
//! Gamble`. The alternative — a button per copy, numbered — keeps every
//! legal index on its own button and makes a person wonder what the
//! difference between copy 1 and copy 2 is; there is none. So
//! [`Selection::hidden`] names the positions a button already stands for,
//! and both clients leave them out of the list they draw. Every hidden
//! position is equivalent to a shown one — same card, same place, same
//! side of the selection — and the play helper still lists the whole
//! legal list.

use std::collections::BTreeSet;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, CardZoneRef};
use netrunner_core::rules::{InstallId, PendingDecision, PlayerAction, Side};
use netrunner_core::view::ClientView;

use crate::actions::card_title;
use crate::board::action_map::server_name;

/// One card the prompt may select, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub position: usize,
    /// The card, when the chooser may name it.
    pub card: Option<CardId>,
    /// The title, or what a concealed install is ("unrezzed ice").
    pub name: String,
    /// Where it is: "HQ", "protecting R&D, outermost of 2", "the heap".
    pub place: String,
    /// Whether `name` is a description rather than a title — such a
    /// candidate always says where it is, since that is all that tells two
    /// of them apart.
    pub concealed: bool,
    pub selected: bool,
}

/// A parked `ChooseCards` as its chooser reads it. `None` for anyone else,
/// whose view carries no candidates to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub candidates: Vec<Candidate>,
    pub min: u32,
    pub max: u32,
}

impl Selection {
    pub fn of(view: &ClientView, registry: &CardRegistry) -> Option<Selection> {
        let Some(PendingDecision::ChooseCards { side, source, min, max, selected, source_card, .. }) = &view.pending_decision else {
            return None;
        };
        if view.selection.is_empty() || !view.viewer.is(*side) {
            return None;
        }
        let candidates = view
            .selection
            .iter()
            .map(|candidate| {
                let (name, place, concealed) = match candidate.install {
                    Some(install) => install_words(view, install, candidate.card.as_ref(), registry),
                    None => {
                        let name = candidate.card.as_ref().map_or_else(|| "a card".to_string(), |card| card_title(card, registry));
                        (name, zone_place(view, *side, source, candidate.position, source_card.as_ref(), registry), candidate.card.is_none())
                    }
                };
                Candidate { position: candidate.position, card: candidate.card.clone(), name, place, concealed, selected: selected.contains(&candidate.position) }
            })
            .collect();
        Some(Selection { candidates, min: *min, max: *max })
    }

    pub fn candidate(&self, position: usize) -> Option<&Candidate> {
        self.candidates.iter().find(|c| c.position == position)
    }

    /// The chosen cards, in position order.
    pub fn chosen(&self) -> Vec<&Candidate> {
        self.candidates.iter().filter(|c| c.selected).collect()
    }

    /// Whether `candidate` needs its place to be told apart: it is
    /// concealed, or another candidate of the same name sits elsewhere.
    fn needs_place(&self, candidate: &Candidate) -> bool {
        candidate.concealed || self.candidates.iter().any(|other| other.name == candidate.name && other.place != candidate.place)
    }

    /// The name a person reads for `candidate`, with its place when that
    /// is what tells it apart.
    pub fn display(&self, candidate: &Candidate) -> String {
        if self.needs_place(candidate) { format!("{} ({})", candidate.name, candidate.place) } else { candidate.name.clone() }
    }

    /// The label of the toggle at `position`.
    pub fn toggle_label(&self, position: usize) -> Option<String> {
        let candidate = self.candidate(position)?;
        if candidate.selected {
            // With room for one card, taking it back is how a person picks
            // another; with room for several it is one card out of many.
            return Some(if self.max == 1 { "Select a different card".to_string() } else { format!("Deselect {}", self.display(candidate)) });
        }
        let another = self.candidates.iter().any(|other| other.selected && other.name == candidate.name && other.place == candidate.place);
        Some(format!("Select {}{}", if another { "another " } else { "" }, self.display(candidate)))
    }

    /// The confirm's label: what is being confirmed, by name.
    pub fn confirm_label(&self) -> String {
        match self.chosen_words() {
            Some(words) => format!("Confirm {words}"),
            None => "Choose none".to_string(),
        }
    }

    /// The line under the prompt: what is chosen so far.
    pub fn summary(&self) -> String {
        match self.chosen_words() {
            Some(words) => format!("Selected: {words}"),
            None => "Nothing selected yet".to_string(),
        }
    }

    /// The chosen cards as a phrase — "Hedge Fund and Sure Gamble (×2)" —
    /// or `None` when nothing is chosen.
    fn chosen_words(&self) -> Option<String> {
        let mut groups: Vec<(String, usize)> = Vec::new();
        for candidate in self.chosen() {
            let words = self.display(candidate);
            match groups.iter_mut().find(|(w, _)| *w == words) {
                Some((_, n)) => *n += 1,
                None => groups.push((words, 1)),
            }
        }
        let parts: Vec<String> = groups.into_iter().map(|(words, n)| if n == 1 { words } else { format!("{words} (×{n})") }).collect();
        match parts.as_slice() {
            [] => None,
            [one] => Some(one.clone()),
            [init @ .., last] => Some(format!("{} and {last}", init.join(", "))),
        }
    }

    /// The positions a shown button already stands for: in each group of
    /// candidates with the same name, the same place and the same side of
    /// the selection, every position but the first. See the module doc.
    pub fn hidden(&self) -> BTreeSet<usize> {
        let mut seen: Vec<(&str, &str, bool)> = Vec::new();
        let mut hidden = BTreeSet::new();
        for candidate in &self.candidates {
            let key = (candidate.name.as_str(), candidate.place.as_str(), candidate.selected);
            if seen.contains(&key) {
                hidden.insert(candidate.position);
            } else {
                seen.push(key);
            }
        }
        hidden
    }

    /// How many candidates the button at `position` stands for: itself
    /// and every copy [`Selection::hidden`] folds into it. What a client
    /// draws as "×2" on the one face it shows for two Sure Gambles.
    pub fn copies(&self, position: usize) -> usize {
        let Some(candidate) = self.candidate(position) else { return 0 };
        self.candidates.iter().filter(|c| c.name == candidate.name && c.place == candidate.place && c.selected == candidate.selected).count()
    }

    /// Where `action` sits in the prompt's list: the cards still to choose
    /// from, then the confirm, then the way back from what is chosen — so
    /// a full single choice reads "Confirm Hedge Fund", "Select a different
    /// card", in the order the person asked for, rather than the engine's
    /// position order with the confirm last.
    pub fn rank(&self, action: &PlayerAction) -> u8 {
        match action {
            PlayerAction::ToggleCardSelection { position } if self.candidate(*position).is_some_and(|c| c.selected) => 2,
            PlayerAction::ConfirmCardSelection => 1,
            _ => 0,
        }
    }

    /// The log's words for a toggle, read off the view *after* it: a
    /// position now selected was just selected.
    pub fn logged_toggle(&self, position: usize) -> Option<String> {
        let candidate = self.candidate(position)?;
        Some(format!("{} {}", if candidate.selected { "Selected" } else { "Deselected" }, self.display(candidate)))
    }
}

/// `actions` as the terminal lists them: without the toggles
/// [`Selection::hidden`] folds into another button, in
/// [`Selection::rank`]'s order. With no selection prompt the list is
/// returned whole and as it came.
pub fn shown(actions: &[PlayerAction], view: Option<&ClientView>, registry: &CardRegistry) -> Vec<PlayerAction> {
    let Some(selection) = view.and_then(|view| Selection::of(view, registry)) else { return actions.to_vec() };
    let hidden = selection.hidden();
    let mut shown: Vec<PlayerAction> = actions
        .iter()
        .filter(|action| !matches!(action, PlayerAction::ToggleCardSelection { position } if hidden.contains(position)))
        .cloned()
        .collect();
    shown.sort_by_key(|action| selection.rank(action));
    shown
}

/// A Corp install or a rig card, in words: its name (or what it is, when
/// the chooser may not name it), where it is, and whether it is concealed.
fn install_words(view: &ClientView, install: InstallId, card: Option<&CardId>, registry: &CardRegistry) -> (String, String, bool) {
    for server in &view.corp.servers {
        let name = server_name(server.server);
        if let Some(index) = server.ice.iter().position(|c| c.install_id == install) {
            let count = server.ice.len();
            let order = match (index, count) {
                (_, 1) => String::new(),
                (0, _) => format!(", outermost of {count}"),
                (i, n) if i + 1 == n => format!(", innermost of {n}"),
                (i, n) => format!(", {} of {n} from the outside", i + 1),
            };
            let words = card.map_or_else(|| "unrezzed ice".to_string(), |card| card_title(card, registry));
            return (words, format!("protecting {name}{order}"), card.is_none());
        }
        if server.root.iter().any(|c| c.install_id == install) {
            let words = card.map_or_else(|| "face-down card".to_string(), |card| card_title(card, registry));
            return (words, format!("in {name}"), card.is_none());
        }
    }
    let rig = view.runner.rig.iter().find(|c| c.install_id == install);
    let place = match rig.and_then(|r| r.hosted_on_ice.or(r.hosted_on_program)) {
        Some(host) => format!("hosted on {}", crate::actions::install_label(&host, registry, Some(view))),
        None => "in the rig".to_string(),
    };
    let name = card.or(rig.map(|r| &r.card)).map_or_else(|| "a card".to_string(), |card| card_title(card, registry));
    (name, place, false)
}

/// Where a card in a zone is, named from the chooser's chair.
fn zone_place(view: &ClientView, chooser: Side, zone: &CardZoneRef, position: usize, source_card: Option<&CardId>, registry: &CardRegistry) -> String {
    let opponent = chooser.other();
    let archives = || match view.corp.archives.get(position) {
        Some(card) if card.facedown => "Archives, face down".to_string(),
        _ => "Archives".to_string(),
    };
    let hand = |side: Side| if side == Side::Corp { "HQ" } else { "the grip" }.to_string();
    let deck = |side: Side| if side == Side::Corp { "R&D" } else { "the stack" }.to_string();
    let discard = |side: Side| if side == Side::Corp { archives() } else { "the heap".to_string() };
    match zone {
        CardZoneRef::OwnHq => "HQ".to_string(),
        CardZoneRef::OwnArchives => archives(),
        CardZoneRef::OwnRAndD => "R&D".to_string(),
        CardZoneRef::OwnStack => "the stack".to_string(),
        CardZoneRef::TopOfOwnStack => "the top of the stack".to_string(),
        CardZoneRef::OwnGrip => "the grip".to_string(),
        CardZoneRef::OwnHeap => "the heap".to_string(),
        CardZoneRef::HostedOnSource => match source_card {
            Some(card) => format!("hosted on {}", card_title(card, registry)),
            None => "hosted here".to_string(),
        },
        CardZoneRef::OpponentHand => hand(opponent),
        CardZoneRef::OpponentDeck => deck(opponent),
        CardZoneRef::OpponentDiscard => discard(opponent),
        CardZoneRef::OpponentScoreArea => format!("the {opponent:?}'s score area"),
        CardZoneRef::OwnScoreArea => "your score area".to_string(),
        // Installed zones carry an install and never reach here; a
        // position with none is still somewhere on the table.
        CardZoneRef::OwnInstalled | CardZoneRef::OpponentInstalled => "installed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardFilter, CardType, IceType};
    use netrunner_core::rules::{GamePhase, GameState, InstallSlot, InstalledCard, PendingChoiceResume, ServerId};
    use netrunner_core::view::build_client_view;

    fn card(id: &str, title: &str, side: Side, card_type: CardType) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), title: title.to_string(), side, card_type, is_playable: true, ..Default::default() }
    }

    fn registry() -> CardRegistry {
        CardRegistry::from_cards(vec![
            card("sure_gamble", "Sure Gamble", Side::Runner, CardType::Event),
            card("daily_casts", "Daily Casts", Side::Runner, CardType::Resource),
            card("ice_wall", "Ice Wall", Side::Corp, CardType::Ice(IceType::Barrier)),
            card("hedge_fund", "Hedge Fund", Side::Corp, CardType::Operation),
        ])
    }

    fn choose(side: Side, source: CardZoneRef, max: u32, selected: Vec<usize>) -> PendingDecision {
        PendingDecision::ChooseCards {
            side,
            source,
            filter: CardFilter::Any,
            min: if max == 1 { 1 } else { 0 },
            max,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected,
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        }
    }

    fn id(s: &str) -> CardId {
        CardId(s.to_string())
    }

    /// Carnivore against a grip of two Sure Gambles and a Daily Casts: the
    /// two copies are one button, and once one is taken that button offers
    /// the other and the chosen one is one "Deselect" — the preview the
    /// person picked.
    #[test]
    fn identical_copies_in_one_place_are_one_button() {
        let registry = registry();
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.grip = vec![id("sure_gamble"), id("sure_gamble"), id("daily_casts")];
        state.pending_decision = Some(choose(Side::Runner, CardZoneRef::OwnGrip, 2, Vec::new()));
        let view = build_client_view(&state, &registry, Side::Runner);
        let selection = Selection::of(&view, &registry).expect("the Runner is choosing");
        assert_eq!(selection.hidden(), BTreeSet::from([1]), "the second Sure Gamble is the first one's button");
        assert_eq!(selection.toggle_label(0).as_deref(), Some("Select Sure Gamble"));
        assert_eq!(selection.toggle_label(2).as_deref(), Some("Select Daily Casts"));
        assert_eq!((selection.copies(0), selection.copies(2)), (2, 1), "one face for two Sure Gambles, marked as two");
        assert_eq!(selection.summary(), "Nothing selected yet");
        let shown = shown(&view.legal_actions, Some(&view), &registry);
        assert!(!shown.contains(&PlayerAction::ToggleCardSelection { position: 1 }));
        assert!(shown.contains(&PlayerAction::ToggleCardSelection { position: 0 }));

        state.pending_decision = Some(choose(Side::Runner, CardZoneRef::OwnGrip, 2, vec![0]));
        let view = build_client_view(&state, &registry, Side::Runner);
        let selection = Selection::of(&view, &registry).unwrap();
        assert!(selection.hidden().is_empty(), "one copy on each side of the selection");
        assert_eq!(selection.copies(0), 1);
        assert_eq!(selection.toggle_label(1).as_deref(), Some("Select another Sure Gamble"));
        assert_eq!(selection.toggle_label(0).as_deref(), Some("Deselect Sure Gamble"));
        assert_eq!(selection.summary(), "Selected: Sure Gamble");
        assert_eq!(selection.confirm_label(), "Confirm Sure Gamble");

        state.pending_decision = Some(choose(Side::Runner, CardZoneRef::OwnGrip, 2, vec![0, 1]));
        let view = build_client_view(&state, &registry, Side::Runner);
        let selection = Selection::of(&view, &registry).unwrap();
        assert_eq!(selection.hidden(), BTreeSet::from([1]), "and both chosen are one Deselect");
        assert_eq!(selection.confirm_label(), "Confirm Sure Gamble (×2)");
    }

    /// With room for one card, the chosen card's button is how a person
    /// changes their mind; the confirm names what they chose.
    #[test]
    fn a_full_single_selection_offers_confirm_and_a_different_card() {
        let registry = registry();
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.hq = vec![id("hedge_fund"), id("ice_wall")];
        state.pending_decision = Some(choose(Side::Corp, CardZoneRef::OwnHq, 1, vec![0]));
        let view = build_client_view(&state, &registry, Side::Corp);
        let selection = Selection::of(&view, &registry).unwrap();
        assert_eq!(selection.toggle_label(0).as_deref(), Some("Select a different card"));
        assert_eq!(selection.confirm_label(), "Confirm Hedge Fund");
        assert_eq!(
            shown(&view.legal_actions, Some(&view), &registry),
            vec![PlayerAction::ConfirmCardSelection, PlayerAction::ToggleCardSelection { position: 0 }],
            "the confirm, then the way back"
        );
        assert_eq!(selection.summary(), "Selected: Hedge Fund");
        assert_eq!(selection.logged_toggle(0).as_deref(), Some("Selected Hedge Fund"));
        assert_eq!(selection.logged_toggle(1).as_deref(), Some("Deselected Ice Wall"));
    }

    /// Two Ice Walls on two servers are told apart by where they are, a
    /// lone title is not given a place it does not need, and an unrezzed
    /// piece of the Corp's ICE is named by where it is and nothing else.
    #[test]
    fn a_place_is_shown_when_it_is_what_tells_two_cards_apart() {
        let registry = registry();
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        let ice = |install: u32, server: ServerId, rezzed: bool| InstalledCard {
            install_id: InstallId(install),
            card: id("ice_wall"),
            server,
            slot: InstallSlot::Ice,
            rezzed,
            ..Default::default()
        };
        state.corp.installed = vec![
            ice(10, ServerId::Hq, true),
            ice(11, ServerId::Remote(0), true),
            ice(12, ServerId::Remote(0), false),
            InstalledCard { install_id: InstallId(13), card: id("hedge_fund"), server: ServerId::Remote(0), slot: InstallSlot::Root, rezzed: true, ..Default::default() },
        ];
        state.pending_decision = Some(choose(Side::Runner, CardZoneRef::OpponentInstalled, 1, Vec::new()));
        let view = build_client_view(&state, &registry, Side::Runner);
        let selection = Selection::of(&view, &registry).unwrap();
        assert_eq!(selection.toggle_label(0).as_deref(), Some("Select Ice Wall (protecting HQ)"));
        assert_eq!(selection.toggle_label(1).as_deref(), Some("Select Ice Wall (protecting Remote 0, outermost of 2)"));
        assert_eq!(selection.toggle_label(2).as_deref(), Some("Select unrezzed ice (protecting Remote 0, innermost of 2)"));
        assert_eq!(selection.toggle_label(3).as_deref(), Some("Select Hedge Fund"));
        assert!(selection.hidden().is_empty());
        let words = format!("{selection:?}");
        assert_eq!(words.matches("ice_wall").count(), 2, "the unrezzed Ice Wall is never named: {words}");
    }

    /// The opponent, and a spectator, have nothing to label: no selection.
    #[test]
    fn only_the_chooser_has_a_selection() {
        let registry = registry();
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.hq = vec![id("hedge_fund")];
        state.pending_decision = Some(choose(Side::Corp, CardZoneRef::OwnHq, 1, Vec::new()));
        assert!(Selection::of(&build_client_view(&state, &registry, Side::Corp), &registry).is_some());
        assert!(Selection::of(&build_client_view(&state, &registry, Side::Runner), &registry).is_none());
    }
}
