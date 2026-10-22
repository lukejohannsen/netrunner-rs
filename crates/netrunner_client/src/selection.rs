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
//!
//! **A prompt says what choosing does** (Phase 7 §4bo, reported 11 October
//! 2026). Cultivate is three selections in a row — trash one of the top
//! five, add one to HQ, arrange the rest — and every one was headed
//! "Cultivate: choose 1 card". The parked decision already says what the
//! selection is for: its `source`, its `destination`, its `filter`, its
//! bounds and whether the deck is shuffled afterwards. [`purpose`] turns
//! those into the verb on the prompt ("Trash 1 of the top 5 cards of
//! R&D", "Add 1 of the top 4 cards of R&D to HQ", "Arrange the top 3
//! cards of R&D"), so no card file carries a second copy of words its
//! fields determine; a `then` the fields cannot say is read as prose
//! under the verb.
//!
//! **An arrangement is listed top first, and each pick is numbered.** The
//! engine arranges by pick order — the first card chosen ends on top, and
//! a card left unpicked stays beneath in its order (`pending_choice`) —
//! and the clients listed the deck deepest first, numbered nothing and
//! offered "Choose none", so the person who reported it believed the
//! cards could not be reordered. Now a selection from the top of a deck
//! runs top first, each candidate's place is its depth, a picked card
//! carries its number (1 is the top), and the confirm with nothing picked
//! says what it does: "Leave them as they are".

use std::collections::BTreeSet;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardFilter, CardId, CardType, CardZoneRef, DeckEnd, Effect};
use netrunner_core::rules::{InstallId, PendingDecision, PlayerAction, Side};
use netrunner_core::view::ClientView;

use crate::actions::card_title;
use crate::board::action_map::server_name;

/// What choosing does, in the words on the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purpose {
    /// The verb and its object: "Trash 1 of the top 5 cards of R&D".
    pub verb: String,
    /// The line under it when the mechanics need saying — how an
    /// arrangement's order is made, or what follows a choice the fields
    /// cannot name — else empty.
    pub hint: String,
    /// Cards taken off the top of a deck and put back in the order chosen:
    /// listed top first, each pick numbered, and nothing chosen is "leave
    /// them as they are".
    pub arrangement: bool,
}

/// The words for what a `ChooseCards` does, read off its fields from the
/// chooser's chair. `then` is what resolves after the choice, which names
/// the one thing the destination cannot: a deck's bottom.
#[allow(clippy::too_many_arguments)]
pub fn purpose(
    chooser: Side,
    source: &CardZoneRef,
    destination: Option<&CardZoneRef>,
    filter: &CardFilter,
    min: u32,
    max: u32,
    shuffle_after: bool,
    then: Option<&Effect>,
    source_card: Option<&CardId>,
    registry: &CardRegistry,
) -> Purpose {
    let zone = |zone: &CardZoneRef| zone_name(chooser, zone, source_card, registry);
    let top_n = top_of_deck(filter);
    let arrangement = top_n.is_some() && is_deck(source) && destination == Some(source) && !shuffle_after;
    let count = count_words(min, max);
    // What is chosen: "1 of the top 5 cards of R&D", "the top 3 cards of
    // R&D" when every one is taken, "1 installed resource", "2 cards from
    // the grip".
    let what = match top_n {
        Some(n) => {
            // One card off the top is "the top card" whether it must be
            // taken or may be; every card taken is the whole top.
            if n == 1 {
                format!("the top card of {}", zone(source))
            } else if min == max && min == n {
                format!("the top {n} cards of {}", zone(source))
            } else {
                format!("{count} of the top {n} cards of {}", zone(source))
            }
        }
        None => match type_word(filter) {
            Some(word) => {
                let installed = matches!(source, CardZoneRef::OwnInstalled | CardZoneRef::OpponentInstalled);
                let noun = format!("{}{}", if installed { "installed " } else { "" }, if max == 1 { word.to_string() } else { format!("{word}s") });
                if installed { format!("{count} {noun}") } else { format!("{count} {noun} from {}", zone(source)) }
            }
            None => format!("{count} {} from {}", if max == 1 { "card" } else { "cards" }, zone(source)),
        },
    };
    let mut hint = String::new();
    let verb = if arrangement {
        hint = "The first you choose goes on top; the cards you leave stay beneath, in their order.".to_string();
        format!("Arrange {}", what.trim_start_matches(&format!("{count} of ")))
    } else {
        match destination {
            None => {
                if let Some(then) = then {
                    hint = format!("Then: {}.", crate::prose::describe_effect(then, registry));
                }
                format!("Choose {what}")
            }
            Some(CardZoneRef::OwnArchives | CardZoneRef::OwnHeap | CardZoneRef::OpponentDiscard) => format!("Trash {what}"),
            Some(hand @ (CardZoneRef::OwnHq | CardZoneRef::OwnGrip | CardZoneRef::OpponentHand)) => format!("Add {what} to {}", zone(hand)),
            Some(CardZoneRef::BottomOfOwnDeck) => format!("Add {what} to the bottom of {}", zone(&CardZoneRef::BottomOfOwnDeck)),
            Some(deck @ (CardZoneRef::OwnRAndD | CardZoneRef::OwnStack | CardZoneRef::TopOfOwnStack | CardZoneRef::OpponentDeck)) => {
                if shuffle_after {
                    format!("Shuffle {what} into {}", zone(deck))
                } else if ends_on_the_bottom(then) {
                    format!("Add {what} to the bottom of {}", zone(deck))
                } else {
                    format!("Add {what} to the top of {}", zone(deck))
                }
            }
            Some(CardZoneRef::OwnScoreArea) => format!("Add {what} to your score area"),
            Some(CardZoneRef::OpponentScoreArea) => format!("Add {what} to {}'s score area", crate::prose::who(chooser.other())),
            Some(CardZoneRef::OpponentRemovedFromGame | CardZoneRef::OwnRemovedFromGame) => format!("Remove {what} from the game"),
            Some(CardZoneRef::HostedOnSource) => format!("Host {what} on {}", source_card.map_or_else(|| "this card".to_string(), |card| card_title(card, registry))),
            Some(CardZoneRef::PlayArea) => format!("Put {what} in the play area"),
            Some(CardZoneRef::OwnSetAside | CardZoneRef::OwnSetAsideFacedown | CardZoneRef::OpponentSetAside) => format!("Set {what} aside"),
            Some(CardZoneRef::OwnInstalled | CardZoneRef::OpponentInstalled) => format!("Install {what}"),
            Some(CardZoneRef::OwnInstalledFacedown) => format!("Install {what} facedown"),
            // A source only, moved whole and never chosen.
            Some(CardZoneRef::RootOfAttackedServer) => format!("Choose {what}"),
            // Nothing is ever put back into the pile.
            Some(CardZoneRef::OtherIdentities) => format!("Choose {what}"),
        }
    };
    // "You may trash the top card of R&D": a choice of none or one with
    // somewhere to go is the card's "may".
    let verb = if min == 0 && max == 1 && destination.is_some() && !arrangement { format!("You may {}", lower_first(&verb)) } else { verb };
    Purpose { verb, hint, arrangement }
}

/// "1", "up to 2", "1 to 3".
fn count_words(min: u32, max: u32) -> String {
    if min == max {
        format!("{max}")
    } else if min == 0 {
        format!("up to {max}")
    } else {
        format!("{min} to {max}")
    }
}

/// How many cards off the top of the zone a filter admits, when that is
/// what it says. `TopOf` reaches a parked decision only resolved to
/// `TopOfZone` (`with_resolution`); unresolved it is the top of the deck
/// all the same.
fn top_of_deck(filter: &CardFilter) -> Option<u32> {
    match filter {
        CardFilter::TopOfZone(n) => Some(*n),
        CardFilter::All(filters) => filters.iter().find_map(top_of_deck),
        _ => None,
    }
}

/// The type word a filter narrows to, when it narrows by type alone.
fn type_word(filter: &CardFilter) -> Option<&'static str> {
    match filter {
        CardFilter::CardType(card_type) => Some(match card_type {
            CardType::Agenda => "agenda",
            CardType::Asset => "asset",
            CardType::Operation => "operation",
            CardType::Ice(_) => "piece of ice",
            CardType::Hardware => "piece of hardware",
            CardType::Resource => "resource",
            CardType::Program => "program",
            CardType::Event => "event",
            CardType::Identity => "identity",
            CardType::Upgrade => "upgrade",
        }),
        CardFilter::Icebreaker => Some("icebreaker"),
        CardFilter::UnrezzedIce => Some("unrezzed piece of ice"),
        CardFilter::All(filters) => filters.iter().find_map(type_word),
        _ => None,
    }
}

fn is_deck(zone: &CardZoneRef) -> bool {
    matches!(zone, CardZoneRef::OwnRAndD | CardZoneRef::BottomOfOwnDeck | CardZoneRef::OwnStack | CardZoneRef::TopOfOwnStack | CardZoneRef::OpponentDeck)
}

/// Whether what follows the choice puts the cards on the bottom of the
/// deck — Lethe's second prompt says `OwnRAndD` and then `AddToDeck
/// (Bottom)`, which the destination alone would word as the top.
fn ends_on_the_bottom(then: Option<&Effect>) -> bool {
    match then {
        Some(Effect::AddToDeck(DeckEnd::Bottom)) => true,
        Some(Effect::Sequence(effects)) => effects.first().is_some_and(|first| ends_on_the_bottom(Some(first))),
        _ => false,
    }
}

fn lower_first(words: &str) -> String {
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// "1st", "2nd", "3rd", "4th" …
fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The chooser's name for a zone: a Runner over `OpponentDeck` reads
/// "R&D", not "the opponent's deck".
fn zone_name(chooser: Side, zone: &CardZoneRef, source_card: Option<&CardId>, registry: &CardRegistry) -> String {
    let opponent = chooser.other();
    let hand = |side: Side| if side == Side::Corp { "HQ" } else { "the grip" }.to_string();
    let deck = |side: Side| if side == Side::Corp { "R&D" } else { "the stack" }.to_string();
    let discard = |side: Side| if side == Side::Corp { "Archives" } else { "the heap" }.to_string();
    match zone {
        CardZoneRef::OwnHq => "HQ".to_string(),
        CardZoneRef::OwnArchives => "Archives".to_string(),
        CardZoneRef::OwnRAndD => "R&D".to_string(),
        CardZoneRef::BottomOfOwnDeck => deck(chooser),
        CardZoneRef::OwnStack => "the stack".to_string(),
        CardZoneRef::TopOfOwnStack => "the top of the stack".to_string(),
        CardZoneRef::OwnGrip => "the grip".to_string(),
        CardZoneRef::OwnHeap => "the heap".to_string(),
        CardZoneRef::OwnSetAside => "the cards set aside".to_string(),
        CardZoneRef::OwnSetAsideFacedown => "the cards set aside facedown".to_string(),
        CardZoneRef::OwnInstalledFacedown => "your facedown installed cards".to_string(),
        CardZoneRef::OtherIdentities => "your other identities".to_string(),
        CardZoneRef::OpponentSetAside => format!("{}'s cards set aside", crate::prose::who(opponent)),
        CardZoneRef::HostedOnSource => match source_card {
            Some(card) => format!("hosted on {}", card_title(card, registry)),
            None => "hosted here".to_string(),
        },
        CardZoneRef::OpponentHand => hand(opponent),
        CardZoneRef::OpponentDeck => deck(opponent),
        CardZoneRef::OpponentDiscard => discard(opponent),
        CardZoneRef::OpponentScoreArea => format!("{}'s score area", crate::prose::who(opponent)),
        CardZoneRef::OwnScoreArea => "your score area".to_string(),
        CardZoneRef::OpponentRemovedFromGame | CardZoneRef::OwnRemovedFromGame => "out of the game".to_string(),
        CardZoneRef::PlayArea => "the play area".to_string(),
        CardZoneRef::OwnInstalled | CardZoneRef::OpponentInstalled => "installed".to_string(),
        CardZoneRef::RootOfAttackedServer => "the root of the attacked server".to_string(),
    }
}

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
    /// Which pick this is, from 1, when it is selected: the order the
    /// engine keeps, and the deck's order for an arrangement (1 is the
    /// top).
    pub pick: Option<usize>,
}

/// A parked `ChooseCards` as its chooser reads it, or an install asking
/// which like card it trashes next. `None` for anyone else, whose view
/// carries no candidates to name.
///
/// The candidates run in the order a person reads them: top first for
/// cards off the top of a deck, else the engine's position order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub candidates: Vec<Candidate>,
    pub min: u32,
    pub max: u32,
    /// What choosing a card does.
    pub verb: Verb,
    /// What the whole selection does, in the words on the prompt.
    pub purpose: Purpose,
}

/// What a toggle does to the card it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// Adds it to a card's selection, or takes it back out.
    Select,
    /// Trashes it as part of an install (CR 8.5.6), at once: an install
    /// asks one card at a time and keeps each answer, so there is nothing
    /// to take back and the confirm is "no more".
    Trash,
}

impl Selection {
    pub fn of(view: &ClientView, registry: &CardRegistry) -> Option<Selection> {
        // An install's question is answered ahead of anything parked
        // beneath it, and only its installer's view carries it.
        if let Some(netrunner_core::rules::PendingPayment { question: netrunner_core::rules::PaymentAsk::Install(_), .. }) =
            view.pending_payment.as_ref().and_then(|payment| payment.own.as_ref())
        {
            let candidates = view
                .selection
                .iter()
                .map(|candidate| {
                    let (name, place, concealed) = match candidate.install {
                        Some(install) => install_words(view, install, candidate.card.as_ref(), registry),
                        None => (candidate.card.as_ref().map_or_else(|| "a card".to_string(), |card| card_title(card, registry)), String::new(), false),
                    };
                    Candidate { position: candidate.position, card: candidate.card.clone(), name, place, concealed, selected: false, pick: None }
                })
                .collect();
            let purpose = Purpose { verb: "Trash a card to make room".to_string(), hint: String::new(), arrangement: false };
            return Some(Selection { candidates, min: 0, max: 1, verb: Verb::Trash, purpose });
        }
        let Some(PendingDecision::ChooseCards { side, source, filter, min, max, shuffle_after, destination, then, selected, source_card, .. }) = &view.pending_decision
        else {
            return None;
        };
        if view.selection.is_empty() || !view.viewer.is(*side) {
            return None;
        }
        let purpose = purpose(*side, source, destination.as_ref(), filter, *min, *max, *shuffle_after, then.as_deref(), source_card.as_ref(), registry);
        // Cards off the top of a deck are told apart by their depth, and
        // read top first — the order the person looked at them in, and
        // the order an arrangement is about. The top of a deck is its
        // highest position, so the depth is read off the candidates
        // themselves, never off a count the view may not carry.
        let from_the_top = top_of_deck(filter).is_some() && is_deck(source);
        let top = view.selection.iter().map(|candidate| candidate.position).max().unwrap_or(0);
        let mut candidates: Vec<Candidate> = view
            .selection
            .iter()
            .map(|candidate| {
                let (name, place, concealed) = match candidate.install {
                    Some(install) => install_words(view, install, candidate.card.as_ref(), registry),
                    None => {
                        let name = candidate.card.as_ref().map_or_else(|| "a card".to_string(), |card| card_title(card, registry));
                        let place = if from_the_top {
                            depth_words(top - candidate.position, &zone_name(*side, source, source_card.as_ref(), registry))
                        } else {
                            zone_place(view, *side, source, candidate.position, source_card.as_ref(), registry)
                        };
                        (name, place, candidate.card.is_none())
                    }
                };
                let pick = selected.iter().position(|p| *p == candidate.position).map(|i| i + 1);
                Candidate { position: candidate.position, card: candidate.card.clone(), name, place, concealed, selected: pick.is_some(), pick }
            })
            .collect();
        if from_the_top {
            candidates.sort_by_key(|c| std::cmp::Reverse(c.position));
        }
        Some(Selection { candidates, min: *min, max: *max, verb: Verb::Select, purpose })
    }

    pub fn candidate(&self, position: usize) -> Option<&Candidate> {
        self.candidates.iter().find(|c| c.position == position)
    }

    /// Where `position` sits in the order the candidates are read, which
    /// is what both clients draw them in.
    pub fn order(&self, position: usize) -> usize {
        self.candidates.iter().position(|c| c.position == position).unwrap_or(usize::MAX)
    }

    /// The chosen cards, in the order they were picked — the deck's order
    /// for an arrangement.
    pub fn chosen(&self) -> Vec<&Candidate> {
        let mut chosen: Vec<&Candidate> = self.candidates.iter().filter(|c| c.selected).collect();
        chosen.sort_by_key(|c| c.pick);
        chosen
    }

    /// The candidates not chosen, in the order they are read.
    fn left(&self) -> Vec<&Candidate> {
        self.candidates.iter().filter(|c| !c.selected).collect()
    }

    /// Whether `candidate` needs its place to be told apart: it is
    /// concealed, another candidate of the same name sits elsewhere, or
    /// the selection is an arrangement, where the place *is* the order.
    fn needs_place(&self, candidate: &Candidate) -> bool {
        candidate.concealed || self.purpose.arrangement || self.candidates.iter().any(|other| other.name == candidate.name && other.place != candidate.place)
    }

    /// The name a person reads for `candidate`, with its place when that
    /// is what tells it apart.
    pub fn display(&self, candidate: &Candidate) -> String {
        if self.needs_place(candidate) { format!("{} ({})", candidate.name, candidate.place) } else { candidate.name.clone() }
    }

    /// The label of the toggle at `position`.
    pub fn toggle_label(&self, position: usize) -> Option<String> {
        let candidate = self.candidate(position)?;
        if self.verb == Verb::Trash {
            return Some(format!("Trash {}", self.display(candidate)));
        }
        if self.purpose.arrangement {
            // Each pick is a place in the new order: the next one, from
            // the top down; a card picked is taken back out of the order.
            return Some(match candidate.pick {
                Some(_) => format!("Take {} back out", self.display(candidate)),
                None => format!("Put {} {}", self.display(candidate), self.next_place()),
            });
        }
        if candidate.selected {
            // With room for one card, taking it back is how a person picks
            // another; with room for several it is one card out of many.
            return Some(if self.max == 1 { "Select a different card".to_string() } else { format!("Deselect {}", self.display(candidate)) });
        }
        let another = self.candidates.iter().any(|other| other.selected && other.name == candidate.name && other.place == candidate.place);
        Some(format!("Select {}{}", if another { "another " } else { "" }, self.display(candidate)))
    }

    /// The place the next pick of an arrangement takes: "1st (on top)",
    /// "2nd", "3rd (last)".
    fn next_place(&self) -> String {
        let n = self.chosen().len() + 1;
        let last = n as u32 == self.max;
        match (n, last) {
            (1, true) => "1st (on top, and the only one moved)".to_string(),
            (1, false) => "1st (on top)".to_string(),
            (_, true) => format!("{} (last)", ordinal(n)),
            (_, false) => ordinal(n),
        }
    }

    /// The confirm's label: what is being confirmed, by name.
    pub fn confirm_label(&self) -> String {
        if self.verb == Verb::Trash {
            return "Trash no more".to_string();
        }
        if self.purpose.arrangement {
            return if self.chosen().is_empty() { "Leave them as they are".to_string() } else { "Confirm this order".to_string() };
        }
        match self.chosen_words() {
            Some(words) => format!("Confirm {words}"),
            None => "Choose none".to_string(),
        }
    }

    /// The line under the prompt: what is chosen so far — for an
    /// arrangement, the order so far, top first, and what stays beneath.
    pub fn summary(&self) -> String {
        if self.purpose.arrangement {
            let chosen = self.chosen();
            if chosen.is_empty() {
                return "Nothing moved yet: they stay in the order shown".to_string();
            }
            let order: Vec<String> = chosen.iter().map(|c| c.name.clone()).collect();
            let left: Vec<String> = self.left().iter().map(|c| c.name.clone()).collect();
            return match left.as_slice() {
                [] => format!("Top first: {}", order.join(", ")),
                [one] => format!("Top first: {} — {one} stays beneath", order.join(", ")),
                many => format!("Top first: {} — {} stay beneath, in that order", order.join(", "), many.join(", ")),
            };
        }
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
    ///
    /// Within each of those, the candidates' own order ([`Selection::order`]):
    /// top first for cards off the top of a deck.
    pub fn rank(&self, action: &PlayerAction) -> (u8, usize) {
        match action {
            PlayerAction::ToggleCardSelection { position } if self.candidate(*position).is_some_and(|c| c.selected) => (2, self.order(*position)),
            PlayerAction::ConfirmCardSelection => (1, 0),
            PlayerAction::ToggleCardSelection { position } => (0, self.order(*position)),
            _ => (0, usize::MAX),
        }
    }

    /// The log's words for a toggle, read off the view *after* it: a
    /// position now selected was just selected.
    pub fn logged_toggle(&self, position: usize) -> Option<String> {
        let candidate = self.candidate(position)?;
        if self.purpose.arrangement {
            return Some(match candidate.pick {
                Some(pick) => format!("Put {} {}", self.display(candidate), if pick == 1 { "1st, on top".to_string() } else { ordinal(pick) }),
                None => format!("Took {} back out", self.display(candidate)),
            });
        }
        Some(format!("{} {}", if candidate.selected { "Selected" } else { "Deselected" }, self.display(candidate)))
    }
}

/// Where a card off the top of a deck sits: "top of R&D", "2nd from the
/// top of R&D".
fn depth_words(depth: usize, deck: &str) -> String {
    if depth == 0 {
        format!("top of {deck}")
    } else {
        format!("{} from the top of {deck}", ordinal(depth + 1))
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
    let place = match rig.and_then(|r| r.hosted_on_ice.or(r.hosted_on_rig_card)) {
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
        CardZoneRef::BottomOfOwnDeck => deck(chooser),
        CardZoneRef::OwnStack => "the stack".to_string(),
        CardZoneRef::TopOfOwnStack => "the top of the stack".to_string(),
        CardZoneRef::OwnGrip => "the grip".to_string(),
        CardZoneRef::OwnHeap => "the heap".to_string(),
        CardZoneRef::OwnSetAside => "the cards set aside".to_string(),
        CardZoneRef::OwnSetAsideFacedown => "the cards set aside facedown".to_string(),
        CardZoneRef::OwnInstalledFacedown => "your facedown installed cards".to_string(),
        CardZoneRef::OtherIdentities => "your other identities".to_string(),
        CardZoneRef::OpponentSetAside => format!("the {opponent:?}'s cards set aside"),
        CardZoneRef::HostedOnSource => match source_card {
            Some(card) => format!("hosted on {}", card_title(card, registry)),
            None => "hosted here".to_string(),
        },
        CardZoneRef::OpponentHand => hand(opponent),
        CardZoneRef::OpponentDeck => deck(opponent),
        CardZoneRef::OpponentDiscard => discard(opponent),
        CardZoneRef::OpponentScoreArea => format!("the {opponent:?}'s score area"),
        CardZoneRef::OwnScoreArea => "your score area".to_string(),
        CardZoneRef::OpponentRemovedFromGame | CardZoneRef::OwnRemovedFromGame => "out of the game".to_string(),
        CardZoneRef::PlayArea => "the play area".to_string(),
        // Installed zones carry an install and never reach here; a
        // position with none is still somewhere on the table.
        CardZoneRef::OwnInstalled | CardZoneRef::OpponentInstalled => "installed".to_string(),
        CardZoneRef::RootOfAttackedServer => "the root of the attacked server".to_string(),
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
            card("cultivate", "Cultivate", Side::Corp, CardType::Operation),
            card("enigma", "Enigma", Side::Corp, CardType::Ice(IceType::CodeGate)),
        ])
    }

    /// The words for what a selection does, one per shape in the pool:
    /// Cultivate's three, a shuffle back (Sprint), a trash out of the grip
    /// (Carnivore), an installed resource (Above the Law), a deck's bottom
    /// read off what follows (Lethe), the top of the stack (Working
    /// Prototype), a Runner arranging R&D (Indexing), a search of R&D that
    /// goes back onto it and is no arrangement (Embedded Reporting), and
    /// the "may" of a choice of none or one.
    #[test]
    fn a_selection_says_what_it_does() {
        let registry = registry();
        let words = |chooser: Side, source: CardZoneRef, destination: Option<CardZoneRef>, filter: CardFilter, min: u32, max: u32, shuffle: bool, then: Option<Effect>| {
            purpose(chooser, &source, destination.as_ref(), &filter, min, max, shuffle, then.as_ref(), None, &registry)
        };
        let top = |n: u32| CardFilter::TopOfZone(n);
        use CardZoneRef::*;
        let trash = words(Side::Corp, OwnRAndD, Some(OwnArchives), top(5), 1, 1, false, None);
        assert_eq!(trash.verb, "Trash 1 of the top 5 cards of R&D");
        assert!(!trash.arrangement && trash.hint.is_empty());
        assert_eq!(words(Side::Corp, OwnRAndD, Some(OwnHq), top(4), 1, 1, false, None).verb, "Add 1 of the top 4 cards of R&D to HQ");
        let arrange = words(Side::Corp, OwnRAndD, Some(OwnRAndD), top(3), 0, 3, false, None);
        assert_eq!(arrange.verb, "Arrange the top 3 cards of R&D");
        assert!(arrange.arrangement);
        assert_eq!(arrange.hint, "The first you choose goes on top; the cards you leave stay beneath, in their order.");
        assert_eq!(words(Side::Corp, OwnHq, Some(OwnRAndD), CardFilter::Any, 2, 2, true, None).verb, "Shuffle 2 cards from HQ into R&D");
        assert_eq!(words(Side::Runner, OwnGrip, Some(OwnHeap), CardFilter::Any, 2, 2, false, None).verb, "Trash 2 cards from the grip");
        assert_eq!(words(Side::Corp, OpponentInstalled, Some(OpponentDiscard), CardFilter::CardType(CardType::Resource), 1, 1, false, None).verb, "Trash 1 installed resource");
        assert_eq!(words(Side::Runner, OwnHeap, Some(OwnStack), CardFilter::Any, 1, 1, false, Some(Effect::AddToDeck(DeckEnd::Bottom))).verb, "Add 1 card from the heap to the bottom of the stack");
        assert_eq!(words(Side::Runner, OwnInstalled, Some(OwnStack), CardFilter::CardType(CardType::Resource), 1, 1, false, None).verb, "Add 1 installed resource to the top of the stack");
        let indexing = words(Side::Runner, OpponentDeck, Some(OpponentDeck), top(5), 0, 5, false, None);
        assert_eq!(indexing.verb, "Arrange the top 5 cards of R&D", "the Runner reads the Corp's deck as R&D");
        assert!(indexing.arrangement);
        let search = words(Side::Corp, OwnRAndD, Some(OwnRAndD), CardFilter::CardType(CardType::Operation), 1, 1, false, None);
        assert_eq!(search.verb, "Add 1 operation from R&D to the top of R&D");
        assert!(!search.arrangement, "a search put back on top is not an arrangement");
        assert_eq!(words(Side::Corp, OwnRAndD, Some(OwnArchives), top(1), 0, 1, false, None).verb, "You may trash the top card of R&D");
        assert_eq!(words(Side::Corp, OwnRAndD, Some(OwnHq), top(3), 3, 3, false, None).verb, "Add the top 3 cards of R&D to HQ", "every one taken is the whole top");
        assert_eq!(words(Side::Corp, OwnHq, None, CardFilter::Any, 1, 1, false, Some(Effect::EndTheRun)).hint, "Then: end the run.", "what follows a choice with nowhere to go is read as prose");
    }

    /// Cultivate's last prompt: the top three of R&D run top first and say
    /// where each sits, nothing chosen is "leave them as they are", each
    /// pick is numbered from the top, and two copies never fold into one
    /// button, because their order is the question.
    #[test]
    fn an_arrangement_is_listed_top_first_numbered_and_may_be_left_alone() {
        let registry = registry();
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Corp);
        // The top of R&D is the end of the Vec: Enigma is on top, then Ice
        // Wall, then a Hedge Fund; the first Hedge Fund is 4th and out of
        // the top three.
        state.corp.r_and_d = vec![id("hedge_fund"), id("hedge_fund"), id("ice_wall"), id("enigma")];
        let parked = |selected: Vec<usize>| PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnRAndD,
            filter: CardFilter::TopOfZone(3),
            min: 0,
            max: 3,
            reveal: false,
            shuffle_after: false,
            destination: Some(CardZoneRef::OwnRAndD),
            then: None,
            selected,
            source_card: Some(id("cultivate")),
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        };
        state.pending_decision = Some(parked(Vec::new()));
        let view = build_client_view(&state, &registry, Side::Corp);
        let selection = Selection::of(&view, &registry).expect("the Corp is arranging");
        assert!(selection.purpose.arrangement);
        let order: Vec<(usize, &str)> = selection.candidates.iter().map(|c| (c.position, c.name.as_str())).collect();
        assert_eq!(order, vec![(3, "Enigma"), (2, "Ice Wall"), (1, "Hedge Fund")], "top first");
        assert_eq!(selection.candidate(3).unwrap().place, "top of R&D");
        assert_eq!(selection.candidate(1).unwrap().place, "3rd from the top of R&D");
        assert_eq!(selection.toggle_label(2).as_deref(), Some("Put Ice Wall (2nd from the top of R&D) 1st (on top)"));
        assert_eq!(selection.confirm_label(), "Leave them as they are");
        assert_eq!(selection.summary(), "Nothing moved yet: they stay in the order shown");
        assert_eq!(
            shown(&view.legal_actions, Some(&view), &registry),
            vec![
                PlayerAction::ToggleCardSelection { position: 3 },
                PlayerAction::ToggleCardSelection { position: 2 },
                PlayerAction::ToggleCardSelection { position: 1 },
                PlayerAction::ConfirmCardSelection
            ],
            "the terminal lists them top first too"
        );

        // Ice Wall picked first, then the Hedge Fund: Ice Wall goes on top,
        // Hedge Fund 2nd, and Enigma, unpicked, stays beneath.
        state.pending_decision = Some(parked(vec![2, 1]));
        let view = build_client_view(&state, &registry, Side::Corp);
        let selection = Selection::of(&view, &registry).unwrap();
        assert_eq!(selection.candidate(2).unwrap().pick, Some(1));
        assert_eq!(selection.candidate(1).unwrap().pick, Some(2));
        assert_eq!(selection.candidate(3).unwrap().pick, None);
        assert_eq!(selection.chosen().iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Ice Wall", "Hedge Fund"], "in pick order, not position order");
        assert_eq!(selection.summary(), "Top first: Ice Wall, Hedge Fund — Enigma stays beneath");
        assert_eq!(selection.confirm_label(), "Confirm this order");
        assert_eq!(selection.toggle_label(3).as_deref(), Some("Put Enigma (top of R&D) 3rd (last)"));
        assert_eq!(selection.toggle_label(1).as_deref(), Some("Take Hedge Fund (3rd from the top of R&D) back out"));
        assert_eq!(selection.logged_toggle(2).as_deref(), Some("Put Ice Wall (2nd from the top of R&D) 1st, on top"));

        // Two Hedge Funds in the top three are two buttons, each its place.
        state.corp.r_and_d = vec![id("enigma"), id("hedge_fund"), id("hedge_fund"), id("ice_wall")];
        state.pending_decision = Some(parked(Vec::new()));
        let view = build_client_view(&state, &registry, Side::Corp);
        let selection = Selection::of(&view, &registry).unwrap();
        assert!(selection.hidden().is_empty(), "an arrangement folds nothing: {:?}", selection.hidden());
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
