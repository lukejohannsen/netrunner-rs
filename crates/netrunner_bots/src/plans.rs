//! The plans a deck stacks, and the style that names them (Phase 5 §25
//! Stage 6).
//!
//! **A style is a list of plans; a plan is a module of terms with the
//! board condition that switches it on.** The strategy guide's "Corp deck
//! styles" chapter names three ways a Corp makes its scoring windows —
//! glacier, fast advance, kill — and says most real decks mix two of
//! them: "glacier, then fast advance", "tag and punish", "traps behind
//! the wall". So a deck names a `Style`, an ordered list of up to
//! `Style::MAX` `Plan`s (`DeckFile::style`, written as a JSON list:
//! `["glacier", "fast-advance"]`), and the turn planner scores with every
//! plan on the list. What a plan is, in the evaluator, is the terms that
//! exist for it: the fort's protection and the rez it holds for
//! `Glacier`, the never-advance line for `FastAdvance` (a term every
//! Corp reads, because the guide's "Make scoring windows" chapter is
//! every Corp's), the lethal check and the tag's leverage for `Kill`,
//! the ambush terms and the bluffed asset for `Traps`. The stack
//! "glacier then fast advance" is one condition inside the fort terms —
//! they fall away once the Runner's rig beats the wall
//! (`Weights::fort_until_beaten`) — never a switch between weight sets
//! (§6–§8, §19).
//!
//! **This replaced `Personality`, outright.** A personality was "a bias,
//! not a plan": a handful of the shared `Weights` moved, one profile per
//! deck, and nothing that could say "first this, then that". The Corp
//! profiles' numbers are kept exactly, under the plans' names
//! (`Plan::weights`: `Rush` is `FastAdvance`, `Trap` is `Traps`), and the
//! planner starts from the first plan's profile (`Style::weights`) and
//! switches on every plan on the list (`Style::planned_weights`), which
//! is where the stack and every Stage 6 term live. Until Stage 8 the
//! one-ply `HeuristicAgent` — the fixed reference every stage was
//! measured against — scored with `Style::weights` alone, the first
//! plan's profile and nothing else, so that it never moved; Stage 8
//! deleted it once the planner had beaten it on both chairs, and the
//! planner is the bot at every rung. There is no `Balanced` plan: a
//! balanced bot is an empty style, `Weights::default()`.
//!
//! **The Runner's plans are the guide's three factions, and the seat's
//! own identity names one when the deck does not (Stage 7).** "The
//! Runner factions" chapter is three ways to play: Anarch tears the
//! Corp's board down (`Dismantle`), Criminal runs early and often and
//! takes the Corp's money (`Pressure`), Shaper builds the rig that makes
//! every run cheap and exact, above all on R&D (`Rig`). A Runner deck
//! names them as a Corp deck names its plans, and a deck that names none
//! is played in its identity's faction's plan (`Plan::for_faction`,
//! `Style::or_faction`) — the identity is public, so the reading costs
//! nothing and a saved deck with no style still plays its chapter. **A
//! Runner plan is its lever alone, and has no profile.** The old Runner
//! styles were profiles; two of them (`Aggressive`, `Builder`) kept their
//! numbers under `Pressure` and `Rig` for the reference through Stage 7,
//! which measured them as a cost to the planner — 0.03 and 0.07 of the
//! chair, dials tuned against a chooser with no economy term — and Stage
//! 8 deleted them with the reference. `Cautious` and `Wary` were one
//! deck's and two decks' dials with no chapter of the guide behind them,
//! and those decks now name their faction's plan.
//!
//! **Every Runner reads the Corp's identity too**: the last-click term
//! and the feared flatline (`LAST_CLICK_RUN_WEIGHT`,
//! `FEARED_FLATLINE_WEIGHT`) fear a Jinteki Corp's face-down cards as net
//! damage and an NBN Corp's as tags, which is the guide's "Stay alive"
//! and "don't run on your last click" written as a reading of the card
//! across the table.
//!
//! **Each plan is written for one chair.** A Corp plan seated as the
//! Runner touches only the shared terms, which is harmless and useless;
//! `Plan::side` says which chair a plan is for, `Style::new` refuses a
//! list that mixes chairs or repeats a plan, and `Style::for_deck` refuses
//! a plan written for the other chair.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use netrunner_core::card::Faction;
use netrunner_core::decks::DeckFile;
use netrunner_core::rules::Side;

use crate::eval::Weights;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Plan {
    /// The taxing wall: build ICE on the centrals and one deep remote,
    /// then score behind it over several turns because the Runner cannot
    /// afford the run. The profile: ICE in front of an agenda is worth
    /// six times balanced and rewards a third piece, rezzed ICE and
    /// credits are worth more, advancement less, and a face-down install
    /// *less* — so the hand is not put on the table before there is
    /// money to turn it face up.
    Glacier,
    /// No window at all: score agendas in the turn they are installed,
    /// or install one turn and finish the next without advancing in
    /// between. The profile is the old rush Corp's: an agenda on the
    /// table is worth more than the ICE in front of it, advancement is
    /// worth more, protection less and capped at one, HQ may run
    /// thinner, and credits are for spending.
    FastAdvance,
    /// Threat as leverage: tag the Runner, then punish the tags; build
    /// net damage until a trap finishes them. No profile of its own — the
    /// balanced weights, plus the planner's lethal check
    /// (`LETHAL_THREAT_WEIGHT`) and the tag's leverage
    /// (`TAG_LEVERAGE_WEIGHT`), which read the punishment the Corp holds.
    Kill,
    /// Bluff with what is installed: the ambush terms (a face-down lure
    /// trap is worth icing and advancing; a hand trap is worth holding)
    /// and, for the planner, an asset in the scoring remote now and then
    /// (`BLUFF_WEIGHT`), so the Runner pays the full price to find out.
    Traps,
    /// Anarch: "tear it down". The Corp's board is something to
    /// dismantle — trash what you access, damage the Corp's hand and
    /// deck. No profile of its own: the balanced weights, plus the
    /// planner's `DISMANTLE_WEIGHT`, a rezzed asset or upgrade on the
    /// table priced as the trash it is waiting for.
    Dismantle,
    /// Criminal: "take their money". Run early and often, punish an
    /// exposed HQ. The profile is the old aggressive Runner's — runs are
    /// worth double, unbroken subroutines and tags cost less, and the
    /// opponent's credits are worth denying — and the planner adds
    /// `HQ_PRESSURE_WEIGHT`, an HQ access worth more than another
    /// server's.
    Pressure,
    /// Shaper: "build the perfect rig". Breakers that scale, and once the
    /// rig is complete every run is cheap and exact, above all on R&D.
    /// The profile is the old builder Runner's — a breaker for a new
    /// subtype and free memory are worth more, a rig card that breaks
    /// nothing is worth one credit — and the planner adds
    /// `RD_ACCESS_WEIGHT`, an R&D access beyond the first worth more, on
    /// the run that makes it and on the card that promises it.
    Rig,
}

impl Plan {
    pub const ALL: [Plan; 7] = [
        Plan::Glacier,
        Plan::FastAdvance,
        Plan::Kill,
        Plan::Traps,
        Plan::Dismantle,
        Plan::Pressure,
        Plan::Rig,
    ];

    /// The chair this plan is written for.
    pub fn side(self) -> Side {
        match self {
            Plan::Glacier | Plan::FastAdvance | Plan::Kill | Plan::Traps => Side::Corp,
            Plan::Dismantle | Plan::Pressure | Plan::Rig => Side::Runner,
        }
    }

    /// The plans written for `side`, in `ALL`'s order — what a picker
    /// offers a deck of that side.
    pub fn for_side(side: Side) -> impl Iterator<Item = Plan> {
        Plan::ALL.into_iter().filter(move |plan| plan.side() == side)
    }

    /// The plan a faction's chapter of the guide teaches — the Runner
    /// factions' three — and `None` for a faction the guide gives no plan
    /// of its own (the Corp's four are ways to make a window, not a
    /// faction's, and a neutral identity has no chapter).
    pub fn for_faction(faction: Faction) -> Option<Plan> {
        match faction {
            Faction::Anarch => Some(Plan::Dismantle),
            Faction::Criminal => Some(Plan::Pressure),
            Faction::Shaper => Some(Plan::Rig),
            Faction::HaasBioroid
            | Faction::Jinteki
            | Faction::Nbn
            | Faction::WeylandConsortium
            | Faction::NeutralCorp
            | Faction::NeutralRunner => None,
        }
    }

    /// The word a deck file, a flag and a report use.
    pub fn name(self) -> &'static str {
        match self {
            Plan::Glacier => "glacier",
            Plan::FastAdvance => "fast-advance",
            Plan::Kill => "kill",
            Plan::Traps => "traps",
            Plan::Dismantle => "dismantle",
            Plan::Pressure => "pressure",
            Plan::Rig => "rig",
        }
    }

    /// The plan's profile — the dials a seat starts from when the plan
    /// leads a style. Every number here is a ratio against the balanced
    /// constant it replaces; the reasoning for the balanced value is on
    /// that constant in `eval`. The Corp's are the `Personality` profiles
    /// as they were measured, unmoved; the Runner's plans have none
    /// (module docs).
    pub fn weights(self) -> Weights {
        let base = Weights::default();
        match self {
            Plan::FastAdvance => Weights {
                // The agenda goes on the table first — an agenda install
                // is +1.0 over an ICE install, where balanced sees no
                // difference — and a token is worth more than the ICE
                // that could sit in front of it: advancing at 2.5 − 0.3 =
                // +2.2 beats an ICE install at 1.0 + 0.2 = +1.2, where the
                // balanced Corp has them at +1.1 and +1.5.
                installed_agenda_weight: 1.0,
                advancement_weight: 2.5,
                agenda_protection_weight: 0.2,
                agenda_protection_cap: 1,
                hq_floor: 2,
                own_credit_weight: 0.3,
                rezzed_ice_weight: 1.2,
                ..base
            },
            Plan::Glacier => Weights {
                // ICE in front of an agenda is worth six times balanced
                // and a third piece still pays; an install there at 0.8 +
                // 3.0 = +3.8 beats advancing at 1.2 − 0.5 = +0.7.
                //
                // **Audited against the balanced Runner (ROADMAP Phase 5
                // §9), and two of these numbers moved.** Shipped, the
                // profile won 0.168 of 2,304 games (six seeds × 384) —
                // ahead of balanced's 0.148 in the same games, behind
                // Trap's 0.212. Each knob put back to balanced one at a
                // time said what it was carrying:
                //
                // - `unrezzed_install_weight` **1.2 → 0.8** is +0.071
                //   (z 7.2), and it is a switch, not a dial: 1.0 reads
                //   0.185, 0.9 reads 0.240, 0.7 0.238, and 0.4 never wins
                //   at all (nothing unprotected is installed, so no agenda
                //   is either). The step is where a face-down install out
                //   of a thin HQ, 1.2 − `hq_shortfall_weight` 0.5 = +0.7,
                //   stops beating this profile's credit click at 0.5. At
                //   1.2 the Corp put its hand face down on turns 2 and 3
                //   (2.29 installs on turn 2, 2.32 cards face down and 3.6
                //   credits by turn 3) and could not rez it; at 0.9 it
                //   clicks for credits instead (0.16 → 0.86 on turn 2),
                //   installs the same 13.8 cards a game later, when it can
                //   pay for them, and scores 0.9 → 1.2 agendas. That is
                //   Phase 5 §5's "credit-starved early", written into a
                //   profile. Raising `hq_shortfall_weight` to 0.8 instead
                //   recovers two thirds of it (0.216), so the floor is
                //   most of the decision and not all of it. 0.8 sits in
                //   the middle of the flat 0.7–0.9 region.
                // - `agenda_protection_weight` **1.0 → 3.0** is +0.043 on
                //   top (z 5.8) and was already the profile's engine —
                //   back at balanced's 0.5 it loses 0.026. It climbs to
                //   0.282 at 3.0–4.0 and turns over past 6.0; the cap is
                //   inert (2: 0.288, 4: 0.281).
                // - `rezzed_ice_weight` 1.8 is **not** a defect, whatever
                //   Phase 5 §3 suspected from the rez rate by cost: back
                //   at 1.4 it costs 0.010 (z 3.1), and 2.4 is flat.
                //   `own_credit_weight` 0.5 and `advancement_weight` 1.2
                //   each measure within noise of balanced and stay.
                //
                // Together: **0.168 → 0.282** (z 10.9), ahead of Trap by
                // 0.070 (z 6.1) in the same games, and +0.10 to +0.12
                // against every Runner profile (aggressive 0.200 →
                // 0.322, cautious 0.180 → 0.287, builder 0.134 → 0.231,
                // wary 0.173 → 0.286). It survives search: under
                // `puct@512` as Corp, 0.284 → 0.357 on one seed (z 2.5).
                //
                // **Then it was taught where the fort goes (Phase 5 §19),
                // from a report from play**: it "makes ICE for days
                // horizontally across as many servers as it can without
                // ever once putting down an agenda". Nothing in the
                // evaluator knew which server a piece of ICE was on, so
                // `diag fort` found ICE spread one piece deep over three
                // remotes a game, under one piece on each central, and a
                // quarter of its agendas installed behind nothing. Three
                // terms say the order a person plays a fort in — HQ and
                // R&D two deep and Archives one (`central_ice_weight`), one
                // remote two deep before the agenda exists (`fort_weight`),
                // and an agenda short of that fort priced below a credit
                // click (`exposed_agenda_weight`). See `eval::fort_value`.
                // Against the balanced Runner, six seeds × 384, paired:
                // **0.247 → 0.462** (z 16.6). **The fort terms are no
                // longer this profile's** (Phase 5 §23): the style matrix
                // found them general Corp strength rather than a style,
                // and they moved to the balanced constants every Corp
                // plan inherits (`eval::CENTRAL_ICE_WEIGHT`). What stays
                // here is how hard this plan protects an agenda once it is
                // in.
                agenda_protection_weight: 3.0,
                agenda_protection_cap: 3,
                rezzed_ice_weight: 1.8,
                unrezzed_install_weight: 0.8,
                advancement_weight: 1.2,
                own_credit_weight: 0.5,
                ..base
            },
            Plan::Traps => Weights {
                // **Three terms, all of them card-inspecting, and nothing
                // else.** The first cut of this profile was six generic
                // knobs — hoard credits, install more, hold a bigger hand,
                // protect agendas *less* — around a wish that the Runner's
                // hand be thin, and it lost to balanced 73 to 89 over five
                // seeds of 192 games. Every one of those knobs was
                // costing it: with them removed and only the ambush terms
                // left it went to 443, and turning the ambush terms up to
                // these values to 460 — ahead of balanced's 439 on all
                // five seeds. The lesson is recorded in ROADMAP Phase 3
                // §1: a profile earns its name from a lever that reads the
                // cards, not from twisting the shared dials.
                //
                // `ambush_advancement_weight` is the engine. Alone it is
                // worth +14 over balanced; `ambush_weight` alone is worth
                // −2, and only +7 more on top of it — a face-down ambush
                // is a threat *because it is loaded*, and valuing the
                // face-down card without valuing what fills it buys
                // nothing. The cap at 7 rather than the balanced 3
                // because this Corp is buying damage with its clicks and
                // means to reach a lethal number; past 10 it turns over
                // (457) as the clicks stop being worth it.
                //
                // **No grip term of its own**, and that is a
                // measurement, not an oversight: `opponent_grip_weight`
                // was the term this archetype was built around, and
                // setting it to 0.5 changed nothing — 443 against 444 at
                // the old settings, 460 against 460 at these, with the
                // same `DamageTaken` and the same trigger counts to the
                // unit. The Corp does not choose to deal this damage; the
                // Runner walks into it. See `eval`.
                ambush_weight: 3.0,
                ambush_advancement_weight: 2.8,
                ambush_advancement_cap: 7,
                ..base
            },
            // The kill plan is the planner's terms and no profile: a kill
            // Corp starts from the balanced dials, which is what it was
            // before the plan existed.
            Plan::Kill => base,
            // No Runner plan has a profile (module docs): `Pressure` and
            // `Rig` carried `Aggressive`'s and `Builder`'s numbers for
            // the reference until Stage 8, and the planner never scored
            // with them — measured, they cost it 0.03 and 0.07 of the
            // chair (Stage 7). Their records are in the archive's Phase 5
            // §7, §16 and §17.
            Plan::Dismantle | Plan::Pressure | Plan::Rig => base,
        }
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Plan {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Plan::ALL
            .into_iter()
            .find(|plan| plan.name().eq_ignore_ascii_case(s))
            .ok_or_else(|| format!("unknown plan {s:?}; one of {}", Plan::ALL.map(Plan::name).join(", ")))
    }
}

/// The plans a deck stacks, in the order it names them: up to `MAX`, one
/// chair, no plan twice. Empty is balanced play. Spelled
/// `glacier+fast-advance` on a command line and in a report, and as a
/// JSON list in a deck file and a match record.
///
/// A fixed array rather than a `Vec` so that a style is `Copy`, as the
/// bot specs and rung specs that carry one are: a style is three words,
/// not a collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style([Option<Plan>; Style::MAX]);

impl Style {
    /// The most plans a style names. The guide's mixes are pairs; three
    /// leaves room for "traps behind the wall, then speed".
    pub const MAX: usize = 3;

    /// No plan at all: `Weights::default()`, the balanced bot.
    pub const BALANCED: Style = Style([None; Style::MAX]);

    /// One plan alone.
    pub fn of(plan: Plan) -> Style {
        let mut plans = [None; Style::MAX];
        plans[0] = Some(plan);
        Style(plans)
    }

    /// `plans` as a style, refused when they exceed `MAX`, mix chairs or
    /// repeat a plan.
    pub fn new(plans: &[Plan]) -> Result<Style, String> {
        if plans.len() > Style::MAX {
            return Err(format!("a style names at most {} plans, not {}", Style::MAX, plans.len()));
        }
        let mut style = Style::BALANCED;
        for (n, plan) in plans.iter().enumerate() {
            if plans[..n].contains(plan) {
                return Err(format!("a style names {plan} twice"));
            }
            if let Some(first) = plans.first().filter(|first| first.side() != plan.side()) {
                return Err(format!("{first} is a {:?} plan and {plan} a {:?} plan; a style is one chair's", first.side(), plan.side()));
            }
            style.0[n] = Some(*plan);
        }
        Ok(style)
    }

    /// The plans, in order.
    pub fn plans(&self) -> impl Iterator<Item = Plan> + '_ {
        self.0.iter().flatten().copied()
    }

    pub fn is_balanced(&self) -> bool {
        self.0[0].is_none()
    }

    /// The plan that leads: whose profile the seat starts from.
    pub fn first(&self) -> Option<Plan> {
        self.0[0]
    }

    pub fn has(&self, plan: Plan) -> bool {
        self.0.contains(&Some(plan))
    }

    /// The chair the style is written for; `None` when balanced.
    pub fn side(&self) -> Option<Side> {
        self.first().map(Plan::side)
    }

    /// The style a deck asks to be played in — `DeckFile::style` parsed
    /// against this vocabulary, balanced when the deck names none.
    ///
    /// `Err` for a name that is not a plan, for a list `new` refuses, and
    /// for a plan written for the other chair: a Runner plan under a Corp
    /// deck "touches only the shared terms, which is harmless and
    /// useless" (module docs), so it would seat a balanced bot under a
    /// misleading name. Every embedded deck is checked by
    /// `every_embedded_deck_style_is_a_stack_of_plans_for_its_side`; this
    /// is the runtime check for a deck someone saved.
    pub fn for_deck(deck: &DeckFile) -> Result<Style, String> {
        let plans = deck
            .style
            .iter()
            .map(|name| name.parse::<Plan>())
            .collect::<Result<Vec<Plan>, String>>()
            .map_err(|error| format!("deck {:?}: {error}", deck.id))?;
        let style = Style::new(&plans).map_err(|error| format!("deck {:?}: {error}", deck.id))?;
        match style.side() {
            Some(side) if side != deck.side => Err(format!(
                "deck {:?} is a {:?} deck but its style {style} is a {side:?} plan",
                deck.id, deck.side
            )),
            _ => Ok(style),
        }
    }

    /// This style, or the plan `faction`'s chapter teaches when this
    /// style names none and a plan exists for the faction: what the
    /// planner plays a seat on `side` in, read off the seat's own
    /// identity (Stage 7). A deck that names a style is played in it
    /// whatever its faction — the list overrides the identity, as the
    /// plan said it would — and the Corp's factions default to nothing,
    /// because the guide's Corp plans are ways to make a window and not a
    /// faction's.
    pub fn or_faction(self, side: Side, faction: Option<Faction>) -> Style {
        if !self.is_balanced() {
            return self;
        }
        faction.and_then(Plan::for_faction).filter(|plan| plan.side() == side).map_or(self, Style::of)
    }

    /// The first plan's profile, or the default: the dials a seat starts
    /// from before the guide's rate and the plans' terms
    /// (`planned_weights`), and what the searches (`MctsAgent`, the
    /// uniform PUCT evaluator) score with whole. One plan, because a
    /// profile is a set of the shared dials and two cannot both be
    /// started from; the second plan on a list is its terms alone.
    pub fn weights(&self) -> Weights {
        self.first().map_or_else(Weights::default, Plan::weights)
    }

    /// The planner's weights for a seat on `side`: the first plan's
    /// profile (`weights`; the balanced dials for every Runner plan), at
    /// the guide's rate (Stage 5), with every seat's plan terms on and
    /// every plan on the list switched on (Stages 6 and 7,
    /// `Weights::with_plans`). The side is the seat's, not the style's: a
    /// balanced Corp planner still plays the guide's "Playing the Corp"
    /// chapter, and a balanced Runner planner the Runner's.
    pub fn planned_weights(&self, side: Side) -> Weights {
        self.weights().at_the_guides_rate().with_plans(side, self)
    }
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_balanced() {
            return f.write_str("balanced");
        }
        for (n, plan) in self.plans().enumerate() {
            if n > 0 {
                f.write_str("+")?;
            }
            f.write_str(plan.name())?;
        }
        Ok(())
    }
}

impl FromStr for Style {
    type Err = String;

    /// `glacier+fast-advance`; `balanced` or nothing is the empty style.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("balanced") {
            return Ok(Style::BALANCED);
        }
        let plans = s.split('+').map(|name| name.trim().parse::<Plan>()).collect::<Result<Vec<Plan>, String>>()?;
        Style::new(&plans)
    }
}

impl Serialize for Style {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.plans())
    }
}

impl<'de> Deserialize<'de> for Style {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let plans = Vec::<Plan>::deserialize(deserializer)?;
        Style::new(&plans).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_is_the_empty_style_and_the_default_weights() {
        assert_eq!(Style::default(), Style::BALANCED);
        assert!(Style::BALANCED.is_balanced());
        assert_eq!(Style::BALANCED.weights(), Weights::default());
        assert_eq!(Style::BALANCED.to_string(), "balanced");
        assert_eq!("balanced".parse::<Style>().unwrap(), Style::BALANCED);
        assert_eq!("".parse::<Style>().unwrap(), Style::BALANCED);
    }

    #[test]
    fn every_plan_round_trips_through_its_name_and_a_style_through_its_spelling() {
        for plan in Plan::ALL {
            assert_eq!(plan.to_string().parse::<Plan>().unwrap(), plan);
            assert_eq!(Style::of(plan).to_string().parse::<Style>().unwrap(), Style::of(plan));
        }
        assert_eq!("FAST-ADVANCE".parse::<Plan>().unwrap(), Plan::FastAdvance);
        assert!("rush".parse::<Plan>().is_err(), "the old name is gone");
        assert!("berserk".parse::<Plan>().is_err());
        let stacked = "glacier+fast-advance".parse::<Style>().unwrap();
        assert_eq!(stacked.plans().collect::<Vec<_>>(), vec![Plan::Glacier, Plan::FastAdvance]);
        assert_eq!(stacked.to_string(), "glacier+fast-advance");
        assert_eq!(stacked.first(), Some(Plan::Glacier));
        assert!(stacked.has(Plan::FastAdvance) && !stacked.has(Plan::Traps));
        assert_eq!(stacked.side(), Some(Side::Corp));
        let json = serde_json::to_string(&stacked).unwrap();
        assert_eq!(json, r#"["glacier","fast-advance"]"#);
        assert_eq!(serde_json::from_str::<Style>(&json).unwrap(), stacked);
        assert_eq!(serde_json::to_string(&Style::BALANCED).unwrap(), "[]");
    }

    #[test]
    fn a_style_is_one_chairs_plans_named_once_and_at_most_three() {
        assert!(Style::new(&[Plan::Glacier, Plan::Pressure]).unwrap_err().contains("one chair"));
        assert!(Style::new(&[Plan::Glacier, Plan::Glacier]).unwrap_err().contains("twice"));
        assert!(Style::new(&[Plan::Glacier, Plan::Traps, Plan::FastAdvance, Plan::Kill]).unwrap_err().contains("at most"));
        assert!(Style::new(&[Plan::Glacier, Plan::Traps, Plan::FastAdvance]).is_ok());
        assert!(serde_json::from_str::<Style>(r#"["glacier","glacier"]"#).is_err());
    }

    /// The direction of each plan's profile, against the balanced value.
    #[test]
    fn profiles_move_the_terms_their_doc_comments_name_in_the_direction_they_say() {
        let base = Weights::default();
        let fast = Plan::FastAdvance.weights();
        assert!(fast.advancement_weight > base.advancement_weight && fast.agenda_protection_weight < base.agenda_protection_weight);
        assert!(fast.installed_agenda_weight > 0.0 && base.installed_agenda_weight == 0.0);
        assert!(fast.agenda_protection_cap < base.agenda_protection_cap);
        let glacier = Plan::Glacier.weights();
        assert!(glacier.agenda_protection_weight > base.agenda_protection_weight && glacier.advancement_weight < base.advancement_weight);
        assert!(glacier.agenda_protection_cap > base.agenda_protection_cap);
        // A face-down install out of a thin HQ must not beat this profile's
        // own credit click, or it buries its hand before it can rez it
        // (ROADMAP Phase 5 §9: the switch is worth 0.071).
        assert!(glacier.unrezzed_install_weight - glacier.hq_shortfall_weight < glacier.own_credit_weight);
        let traps = Plan::Traps.weights();
        assert!(traps.ambush_weight > 0.0 && base.ambush_weight == 0.0, "the lever the profile was missing");
        assert!(traps.ambush_advancement_weight > base.ambush_advancement_weight);
        assert!(traps.ambush_advancement_cap > base.ambush_advancement_cap);
        // The profile is *only* its ambush terms — the retune measured
        // every other knob it used to carry as a cost, and the grip term
        // it was named for as inert. If a later change reintroduces one,
        // it should have to justify it against balanced.
        assert_eq!(
            Weights { ambush_weight: base.ambush_weight, ambush_advancement_weight: base.ambush_advancement_weight, ambush_advancement_cap: base.ambush_advancement_cap, ..traps },
            base,
            "Traps deviates from balanced in its ambush terms and nothing else"
        );
        assert_eq!(Plan::Kill.weights(), base, "the kill plan has no profile; its terms are the planner's");
        // No Runner plan has a profile: the old ones were the reference's
        // and went with it (Stage 8), and each plan is its lever.
        for plan in Plan::for_side(Side::Runner) {
            assert_eq!(plan.weights(), base, "{plan:?}");
        }
    }

    /// A style's dials are the first plan's and nothing else's, so the
    /// second plan on a list is its terms alone.
    #[test]
    fn a_style_starts_from_the_first_plans_profile_alone() {
        let stacked = Style::new(&[Plan::Glacier, Plan::FastAdvance]).unwrap();
        assert_eq!(stacked.weights(), Plan::Glacier.weights());
        assert_eq!(Style::new(&[Plan::Traps, Plan::Glacier]).unwrap().weights(), Plan::Traps.weights());
        assert_ne!(stacked.planned_weights(Side::Corp), stacked.weights().at_the_guides_rate(), "the planner reads the stack");
        let rig = Style::of(Plan::Rig).planned_weights(Side::Runner);
        assert_eq!(Weights { rd_access_weight: 0.0, ..rig }, Style::BALANCED.planned_weights(Side::Runner), "a Runner plan is its lever alone");
    }

    /// The gate on `DeckFile::style`: the vocabulary lives here, the data
    /// lives in `netrunner_core`, and this is the one place both are in
    /// scope. A typo in a deck file fails the build rather than seating a
    /// balanced bot under a glacier deck's name.
    #[test]
    fn every_embedded_deck_style_is_a_stack_of_plans_for_its_side() {
        let mut styled = 0;
        let mut stacked = 0;
        for deck in netrunner_core::decks::embedded_decks() {
            let style = Style::for_deck(&deck).unwrap_or_else(|error| panic!("{error}"));
            if !deck.style.is_empty() {
                styled += 1;
                assert!(!style.is_balanced(), "{}: a deck that names a style should not name balanced", deck.id);
            }
            if style.plans().count() > 1 {
                stacked += 1;
            }
        }
        assert!(styled >= 12, "the sample decks carry styles; only {styled} do");
        assert!(stacked >= 3, "the guide says most real decks mix two plans; only {stacked} do");
    }

    #[test]
    fn a_deck_style_is_parsed_and_checked_against_the_chair() {
        let mut deck = netrunner_core::decks::by_id("stolen_goods").expect("embedded");
        deck.style = Vec::new();
        assert_eq!(Style::for_deck(&deck).unwrap(), Style::BALANCED);
        deck.style = vec!["Pressure".to_string()];
        assert_eq!(Style::for_deck(&deck).unwrap(), Style::of(Plan::Pressure));
        deck.style = vec!["glacier".to_string()];
        let error = Style::for_deck(&deck).unwrap_err();
        assert!(error.contains("Corp plan"), "{error}");
        deck.style = vec!["berserk".to_string()];
        assert!(Style::for_deck(&deck).unwrap_err().contains("unknown plan"));
        deck.style = vec!["pressure".to_string(), "pressure".to_string()];
        assert!(Style::for_deck(&deck).unwrap_err().contains("twice"));
    }

    #[test]
    fn corp_plans_are_for_the_corp_and_runner_plans_for_the_runner() {
        for plan in [Plan::Glacier, Plan::FastAdvance, Plan::Kill, Plan::Traps] {
            assert_eq!(plan.side(), Side::Corp);
        }
        for plan in [Plan::Dismantle, Plan::Pressure, Plan::Rig] {
            assert_eq!(plan.side(), Side::Runner);
        }
        assert_eq!(Plan::for_side(Side::Corp).count() + Plan::for_side(Side::Runner).count(), Plan::ALL.len());
        assert!("aggressive".parse::<Plan>().is_err() && "wary".parse::<Plan>().is_err(), "the old Runner styles are gone");
    }

    /// The identity names the plan when the deck does not: a Criminal
    /// deck with no style is played under pressure, a Shaper's as a rig,
    /// an Anarch's dismantling; a styled deck keeps its list whatever its
    /// faction, and a Corp faction names nothing.
    #[test]
    fn a_runner_seat_plays_its_factions_plan_when_the_deck_names_none() {
        assert_eq!(Style::BALANCED.or_faction(Side::Runner, Some(Faction::Criminal)), Style::of(Plan::Pressure));
        assert_eq!(Style::BALANCED.or_faction(Side::Runner, Some(Faction::Shaper)), Style::of(Plan::Rig));
        assert_eq!(Style::BALANCED.or_faction(Side::Runner, Some(Faction::Anarch)), Style::of(Plan::Dismantle));
        assert_eq!(Style::BALANCED.or_faction(Side::Runner, Some(Faction::NeutralRunner)), Style::BALANCED);
        assert_eq!(Style::BALANCED.or_faction(Side::Runner, None), Style::BALANCED);
        for faction in [Faction::HaasBioroid, Faction::Jinteki, Faction::Nbn, Faction::WeylandConsortium, Faction::NeutralCorp] {
            assert_eq!(Style::BALANCED.or_faction(Side::Corp, Some(faction)), Style::BALANCED, "{faction:?}");
        }
        let rig = Style::of(Plan::Rig);
        assert_eq!(rig.or_faction(Side::Runner, Some(Faction::Criminal)), rig, "the deck's list overrides the identity");
        assert_eq!(Style::of(Plan::Glacier).or_faction(Side::Corp, Some(Faction::Anarch)), Style::of(Plan::Glacier), "a plan for the other chair is never seated");
        // Every Runner sample deck reaches a plan one way or the other,
        // and every faction the pool's identities print has its chapter.
        let pool = {
            let mut registry = netrunner_core::cards::CardRegistry::new();
            netrunner_core::cards::register_playable_cards(&mut registry);
            registry
        };
        for deck in netrunner_core::decks::embedded_decks().into_iter().filter(|deck| deck.side == Side::Runner) {
            let faction = pool.get(&deck.identity).and_then(|def| def.faction);
            let played = Style::for_deck(&deck).unwrap().or_faction(Side::Runner, faction);
            assert!(played.is_balanced() == (faction == Some(Faction::NeutralRunner) && deck.style.is_empty()), "{}: {played} from {faction:?}", deck.id);
            if let Some(plan) = faction.and_then(Plan::for_faction).filter(|_| deck.style.len() > 1) {
                assert!(played.has(plan), "{}: a stacked Runner deck stacks its faction's plan, {played}", deck.id);
            }
        }
    }
}
