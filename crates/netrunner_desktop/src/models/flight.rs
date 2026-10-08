//! A card seen moving: where a `Transition` says it went, as a flight
//! from one box on the board to another.
//!
//! **What moved is read off the record, never off the screen** (AGENTS.md
//! §5). `board::diff` says *which* card went from *which* zone to *which*;
//! this module only decides which drawn box stands for each end
//! ([`places`]) and how far along a flight is at a given moment
//! ([`progress`]). The boxes themselves are read once off the laid-out
//! board — the one before the redraw for where a card came from, the one
//! after for where it landed — as a [`Survey`], so a flight starts where
//! the card *was drawn* and ends where it *is drawn*, and nothing is
//! inferred from "a card seems to have gone".
//!
//! **A place is a stand-in for a zone the board does not draw as cards.**
//! The Corp's deck is its R&D plate, its discard the Archives plate, the
//! opponent's hand their avatar bar (it is never drawn, AGENTS.md §5), a
//! score area its Agendas readout, and a card the viewer may not name is
//! drawn as the back of the side it belongs to. Each zone names its
//! places in order of preference, so a Runner whose Stack button is on
//! the far side of the bar still has the bar to fall back on, and a zone
//! with no box at all (removed from the game, somewhere hidden) is a move
//! nobody can see and is not flown.
//!
//! **The destination's own card is hidden while its flight is in the
//! air** ([`hides`]): a drawn card arriving in the hand would otherwise be
//! on the table twice, once landed and once still flying. A pile is never
//! hidden — the card flies *into* it, and the pile stays.
//!
//! **The pace is the person's.** `DesktopPrefs::animation_speed` scales
//! [`FLIGHT`] as it scales the pacer's beat, and its documented 0 is
//! "instant" — no flight is made at all, which is how the headless tests
//! and a person who wants none of this turn it off. Several cards moved
//! by one action leave [`STAGGER`] apart, so a draw of five is five cards
//! in a fan rather than one thick one.

use std::time::Duration;

use netrunner_client::board::{Pile, Transition, Zone};
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{InstallId, ServerId, Side};

use crate::models::layout::Anchor;

/// A box on the board a flight can start from or land on, named by what
/// the board draws there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// An installed card: a server tile or a rig face.
    Install(InstallId),
    /// A card in the person's own hand.
    HandCard(CardId),
    /// A central server's nameplate: R&D for the Corp's deck, Archives
    /// for its discard, HQ for its hand seen from the Runner's chair.
    Plate(ServerId),
    /// A server column, for an install whose tile the window does not
    /// show (a server past its strips).
    Column(ServerId),
    /// The Runner's Stack or Heap button, or a side's Agendas readout.
    Pile(Pile),
    /// A side's avatar bar: the opponent's hand, and the last resort.
    Bar(Side),
    /// The rig area, for a Runner install whose face is not found.
    Rig,
}

/// The authored length of one flight, at animation speed 1.
pub const FLIGHT: Duration = Duration::from_millis(480);
/// How far apart the cards of one action set off.
pub const STAGGER: Duration = Duration::from_millis(110);

/// The flight's length at `animation_speed`; `None` at 0, which is
/// "instant": nothing flies.
pub fn flight_for(animation_speed: f32) -> Option<Duration> {
    (animation_speed > 0.0).then(|| FLIGHT.div_f32(animation_speed))
}

/// The side whose card a move out of, or into, `zone` is: what back it is
/// drawn with when the viewer may not name it.
pub fn owner(zone: &Zone) -> Side {
    match zone {
        Zone::Hand(side) | Zone::Deck(side) | Zone::Discard(side) | Zone::Scored(side) => *side,
        Zone::Server(..) => Side::Corp,
        Zone::Rig => Side::Runner,
        Zone::RemovedFromGame | Zone::Hidden => Side::Corp,
    }
}

/// The boxes that stand for `zone`, best first. `card` and `install` are
/// the transition's; `human` is the chair, whose own hand is drawn as
/// cards where the opponent's is a bar.
pub fn places(zone: &Zone, card: Option<&CardId>, install: Option<InstallId>, human: Side) -> Vec<Place> {
    let mut out = Vec::new();
    match zone {
        Zone::Hand(side) if *side == human => {
            if let Some(card) = card {
                out.push(Place::HandCard(card.clone()));
            }
            out.push(Place::Bar(*side));
        }
        Zone::Hand(Side::Corp) => out.extend([Place::Plate(ServerId::Hq), Place::Bar(Side::Corp)]),
        Zone::Hand(Side::Runner) => out.push(Place::Bar(Side::Runner)),
        Zone::Deck(Side::Corp) => out.extend([Place::Plate(ServerId::RnD), Place::Bar(Side::Corp)]),
        Zone::Deck(Side::Runner) => out.extend([Place::Pile(Pile::Stack), Place::Bar(Side::Runner)]),
        Zone::Discard(Side::Corp) => out.extend([Place::Plate(ServerId::Archives), Place::Bar(Side::Corp)]),
        Zone::Discard(Side::Runner) => out.extend([Place::Pile(Pile::Heap), Place::Bar(Side::Runner)]),
        Zone::Server(server, _) => {
            if let Some(install) = install {
                out.push(Place::Install(install));
            }
            out.extend([Place::Column(*server), Place::Plate(*server), Place::Bar(Side::Corp)]);
        }
        Zone::Rig => {
            if let Some(install) = install {
                out.push(Place::Install(install));
            }
            out.extend([Place::Rig, Place::Bar(Side::Runner)]);
        }
        Zone::Scored(side) => out.extend([Place::Pile(Pile::Agendas(*side)), Place::Bar(*side)]),
        Zone::RemovedFromGame | Zone::Hidden => {}
    }
    out
}

/// Whether the card drawn at `place` is the flying card itself, and so
/// is hidden until the flight lands. A pile, a plate, a bar or an area
/// is what the card flies into, not the card.
pub fn hides(place: &Place) -> bool {
    matches!(place, Place::Install(_) | Place::HandCard(_))
}

/// Where every place was drawn, read off a laid-out board: one entry per
/// box, in the order the board was walked, so two copies of a card in
/// the hand are two entries and the `nth` of them can be asked for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Survey {
    pub entries: Vec<(Place, Anchor)>,
}

impl Survey {
    /// The index of the `nth` box drawn for `place`, if there are that
    /// many.
    pub fn find(&self, place: &Place, nth: usize) -> Option<usize> {
        self.entries.iter().enumerate().filter(|(_, (p, _))| p == place).nth(nth).map(|(index, _)| index)
    }

    pub fn anchor(&self, index: usize) -> Anchor {
        self.entries[index].1
    }
}

/// One end of a flight, resolved: the place it stood for and which copy,
/// so the board after a redraw can be asked for the same box again.
#[derive(Debug, Clone, PartialEq)]
pub struct End {
    pub place: Place,
    pub nth: usize,
    pub anchor: Anchor,
}

/// A card's flight, planned: what to draw and between which boxes.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The card, when the viewer may name it.
    pub card: Option<CardId>,
    /// Whose back to draw when they may not.
    pub back: Side,
    pub from: End,
    pub to: End,
    /// This flight's place in its action's departures: the `order`th
    /// sets off `order` staggers after the first.
    pub order: usize,
}

/// Resolves the best of `candidates` against `survey`, taking the copy
/// `taken` has not yet claimed for that place and claiming it.
fn resolve(candidates: &[Place], survey: &Survey, taken: &mut Vec<(Place, usize)>) -> Option<End> {
    for place in candidates {
        let nth = taken.iter().filter(|(p, _)| p == place).count();
        if let Some(index) = survey.find(place, nth) {
            taken.push((place.clone(), nth));
            return Some(End { place: place.clone(), nth, anchor: survey.anchor(index) });
        }
        // Every copy of a place that has several is claimed: a pile or a
        // bar is one box and may be shared by every flight into it.
        if nth > 0 && !hides(place) && let Some(index) = survey.find(place, 0) {
            return Some(End { place: place.clone(), nth: 0, anchor: survey.anchor(index) });
        }
    }
    None
}

/// The flights `transitions` call for: one per `CardMoved` whose two ends
/// are both drawn, from where its card was on the board `before` the
/// redraw to where it is on the board `after`. Other transitions move no
/// card and plan nothing.
pub fn plan(transitions: &[Transition], before: &Survey, after: &Survey, human: Side) -> Vec<Plan> {
    let mut plans = Vec::new();
    let mut departed: Vec<(Place, usize)> = Vec::new();
    let mut arrived: Vec<(Place, usize)> = Vec::new();
    for transition in transitions {
        let Transition::CardMoved { card, install, from, to } = transition else { continue };
        let Some(from_end) = resolve(&places(from, card.as_ref(), *install, human), before, &mut departed) else { continue };
        let Some(to_end) = resolve(&places(to, card.as_ref(), *install, human), after, &mut arrived) else { continue };
        // A move that lands in the box it left — a card the viewer cannot
        // place at either end, both stood for by the same bar — shows
        // nothing and is not flown. The box, not its position: headless
        // every box is laid out at the origin.
        if (&from_end.place, from_end.nth) == (&to_end.place, to_end.nth) {
            continue;
        }
        let order = plans.len();
        plans.push(Plan { card: card.clone(), back: owner(if card.is_some() { to } else { from }), from: from_end, to: to_end, order });
    }
    plans
}

/// How far along a flight launched at `launched` is at `now`, with its
/// `order` staggers still to wait out: `None` before it has set off,
/// `Some(1.0)` once landed. Eased, so a card leaves and arrives gently.
pub fn progress(launched: Duration, order: usize, flight: Duration, speed: f32, now: Duration) -> Option<f32> {
    let start = launched + STAGGER.div_f32(speed.max(f32::EPSILON)).mul_f32(order as f32);
    if now < start {
        return None;
    }
    if flight.is_zero() {
        return Some(1.0);
    }
    let t = ((now - start).as_secs_f32() / flight.as_secs_f32()).min(1.0);
    Some(t * t * (3.0 - 2.0 * t))
}

/// The box a flight at `t` is drawn in: the two ends' centres and sizes
/// interpolated.
pub fn between(from: Anchor, to: Anchor, t: f32) -> Anchor {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    Anchor { x: lerp(from.x, to.x), y: lerp(from.y, to.y), width: lerp(from.width, to.width), height: lerp(from.height, to.height) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::InstallSlot;

    fn at(x: f32, y: f32) -> Anchor {
        Anchor { x, y, width: 100.0, height: 140.0 }
    }

    fn card(name: &str) -> CardId {
        CardId(name.to_string())
    }

    /// A survey as the Corp's board draws it: plates, a bar, two copies
    /// of a card in hand, and an installed card.
    fn corp_board() -> Survey {
        Survey {
            entries: vec![
                (Place::Bar(Side::Runner), at(500.0, 20.0)),
                (Place::Plate(ServerId::Archives), at(100.0, 400.0)),
                (Place::Plate(ServerId::RnD), at(220.0, 400.0)),
                (Place::Plate(ServerId::Hq), at(340.0, 400.0)),
                (Place::Column(ServerId::Remote(0)), at(460.0, 350.0)),
                (Place::Install(InstallId(7)), at(460.0, 300.0)),
                (Place::Bar(Side::Corp), at(500.0, 700.0)),
                (Place::HandCard(card("hedge_fund")), at(300.0, 760.0)),
                (Place::HandCard(card("hedge_fund")), at(400.0, 760.0)),
            ],
        }
    }

    #[test]
    fn a_zone_names_its_boxes_best_first_and_the_opponents_hand_is_their_bar() {
        let hf = card("hedge_fund");
        assert_eq!(places(&Zone::Hand(Side::Corp), Some(&hf), None, Side::Corp), vec![Place::HandCard(hf.clone()), Place::Bar(Side::Corp)]);
        assert_eq!(places(&Zone::Hand(Side::Corp), Some(&hf), None, Side::Runner), vec![Place::Plate(ServerId::Hq), Place::Bar(Side::Corp)], "the Corp's hand from the Runner's chair is HQ");
        assert_eq!(places(&Zone::Hand(Side::Runner), None, None, Side::Corp), vec![Place::Bar(Side::Runner)]);
        assert_eq!(places(&Zone::Deck(Side::Runner), None, None, Side::Corp), vec![Place::Pile(Pile::Stack), Place::Bar(Side::Runner)]);
        assert_eq!(places(&Zone::Server(ServerId::Remote(0), InstallSlot::Root), None, Some(InstallId(7)), Side::Corp)[0], Place::Install(InstallId(7)));
        assert!(places(&Zone::RemovedFromGame, Some(&hf), None, Side::Corp).is_empty(), "a zone with no box is not flown to");
        assert!(hides(&Place::Install(InstallId(1))) && hides(&Place::HandCard(hf)));
        assert!(!hides(&Place::Plate(ServerId::Hq)) && !hides(&Place::Pile(Pile::Heap)) && !hides(&Place::Bar(Side::Corp)));
    }

    #[test]
    fn a_draw_flies_from_the_deck_to_the_card_in_hand_and_two_copies_land_on_two_cards() {
        let before = Survey { entries: corp_board().entries.into_iter().filter(|(p, _)| !matches!(p, Place::HandCard(_))).collect() };
        let after = corp_board();
        let hf = card("hedge_fund");
        let drawn = Transition::CardMoved { card: Some(hf.clone()), install: None, from: Zone::Deck(Side::Corp), to: Zone::Hand(Side::Corp) };
        let plans = plan(&[drawn.clone(), drawn], &before, &after, Side::Corp);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].from.place, Place::Plate(ServerId::RnD));
        assert_eq!(plans[0].to, End { place: Place::HandCard(hf.clone()), nth: 0, anchor: at(300.0, 760.0) });
        assert_eq!(plans[1].to, End { place: Place::HandCard(hf), nth: 1, anchor: at(400.0, 760.0) }, "the second copy lands on the second card");
        assert_eq!((plans[0].order, plans[1].order), (0, 1));
        assert_eq!(plans[0].card.as_ref().map(|c| c.0.as_str()), Some("hedge_fund"));
    }

    #[test]
    fn the_opponents_draw_is_a_back_from_their_deck_to_their_bar_and_a_hidden_move_is_not_flown() {
        let board = corp_board();
        let drawn = Transition::CardMoved { card: None, install: None, from: Zone::Deck(Side::Runner), to: Zone::Hand(Side::Runner) };
        let plans = plan(&[drawn], &board, &board, Side::Corp);
        assert_eq!(plans.len(), 0, "the Runner's Stack button is not on this board, so the deck and the hand are both the bar: nothing to see");
        let mut with_stack = board.clone();
        with_stack.entries.push((Place::Pile(Pile::Stack), at(600.0, 20.0)));
        let drawn = Transition::CardMoved { card: None, install: None, from: Zone::Deck(Side::Runner), to: Zone::Hand(Side::Runner) };
        let plans = plan(&[drawn], &with_stack, &with_stack, Side::Corp);
        assert_eq!(plans.len(), 1);
        assert_eq!((plans[0].card.clone(), plans[0].back), (None, Side::Runner));
        assert_eq!(plans[0].from.place, Place::Pile(Pile::Stack));
        assert_eq!(plans[0].to.place, Place::Bar(Side::Runner));
        let gone = Transition::CardMoved { card: Some(card("hedge_fund")), install: None, from: Zone::Hand(Side::Corp), to: Zone::RemovedFromGame };
        assert!(plan(&[gone], &board, &board, Side::Corp).is_empty());
    }

    #[test]
    fn an_install_flies_from_the_hand_to_its_tile_and_a_trash_from_the_tile_to_archives() {
        let before = corp_board();
        let after = corp_board();
        let hf = card("hedge_fund");
        let installed = Transition::CardMoved { card: Some(hf.clone()), install: Some(InstallId(7)), from: Zone::Hand(Side::Corp), to: Zone::Server(ServerId::Remote(0), InstallSlot::Root) };
        let plans = plan(&[installed], &before, &after, Side::Corp);
        assert_eq!(plans[0].from.place, Place::HandCard(hf.clone()));
        assert_eq!(plans[0].to.place, Place::Install(InstallId(7)));
        let trashed = Transition::CardMoved { card: Some(hf), install: Some(InstallId(7)), from: Zone::Server(ServerId::Remote(0), InstallSlot::Root), to: Zone::Discard(Side::Corp) };
        let plans = plan(&[trashed], &before, &after, Side::Corp);
        assert_eq!(plans[0].from.place, Place::Install(InstallId(7)));
        assert_eq!(plans[0].to.place, Place::Plate(ServerId::Archives));
        let credits = Transition::Credits { side: Side::Corp, from: 5, to: 6 };
        assert!(plan(&[credits], &before, &after, Side::Corp).is_empty(), "a number changing moves no card");
    }

    #[test]
    fn a_flight_waits_its_stagger_eases_and_lands_and_speed_zero_is_no_flight() {
        let flight = flight_for(1.0).unwrap();
        assert_eq!(flight, FLIGHT);
        assert_eq!(flight_for(2.0).unwrap(), FLIGHT / 2);
        assert_eq!(flight_for(0.0), None);
        let launched = Duration::from_secs(10);
        assert_eq!(progress(launched, 0, flight, 1.0, launched), Some(0.0));
        assert_eq!(progress(launched, 1, flight, 1.0, launched), None, "the second card has not set off");
        assert_eq!(progress(launched, 1, flight, 1.0, launched + STAGGER), Some(0.0));
        let half = progress(launched, 0, flight, 1.0, launched + flight / 2).unwrap();
        assert!((half - 0.5).abs() < 1e-5, "eased, but symmetric about the middle: {half}");
        let early = progress(launched, 0, flight, 1.0, launched + flight / 10).unwrap();
        assert!(early < 0.1, "it leaves gently: {early}");
        assert_eq!(progress(launched, 0, flight, 1.0, launched + flight * 3), Some(1.0));
        let box_ = between(at(0.0, 0.0), Anchor { x: 100.0, y: 200.0, width: 50.0, height: 70.0 }, 0.5);
        assert_eq!(box_, Anchor { x: 50.0, y: 100.0, width: 75.0, height: 105.0 });
    }
}
