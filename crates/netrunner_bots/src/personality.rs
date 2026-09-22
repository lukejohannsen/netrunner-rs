//! Bot personalities: a named `Weights` profile that biases the shared
//! evaluator, and through it every agent that scores a position with it —
//! `HeuristicAgent`'s one-ply choice, `MctsAgent`'s rollouts and leaves,
//! and the uniform PUCT evaluator's value head.
//!
//! **A personality is a bias, not a plan.** Each profile moves a handful
//! of `Weights` terms away from the balanced defaults so that, choosing
//! between the same legal actions, the bot prefers what its archetype
//! would: a rush Corp values an advancement token over the ICE in front
//! of it, a glacier Corp the reverse. Nothing here adds a rule, a card
//! preference by name, or a script; the evaluator is still one function
//! and the profile is data. That keeps a personality cheap to add and
//! impossible to make illegal — it can only rank the same candidates
//! differently. The price is that a profile can only express what the
//! evaluator already has a term for; `Weights::installed_agenda_weight`
//! was added (at zero by default, so balanced play is byte-identical to
//! before it existed) because the rush archetype had no lever at all
//! without it. `opponent_grip_weight` was added the same way for Trap and
//! is **gone, with nothing in its place**: it measured inert for the very
//! archetype it was built around, and rebuilding it in a better shape
//! measured inert too — see the note above `ACTIVE_RUN_AGAINST_WEIGHT` in
//! `eval` for why no weight can work here (ROADMAP Phase 3 §1).
//!
//! **Each archetype is written for one chair.** A Corp profile seated as
//! the Runner touches only the shared terms (credits), which is harmless
//! and useless; `Personality::side` says which chair a profile is for,
//! and the CLI's help text repeats it. `Balanced` is the default
//! everywhere and is exactly `Weights::default()`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use netrunner_core::decks::DeckFile;
use netrunner_core::rules::Side;

use crate::eval::Weights;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Personality {
    /// `Weights::default()`: the evaluator as tuned in Phase 2 §5.
    #[default]
    Balanced,
    /// A fast-advance Corp: score early, protect late. Advancement is
    /// worth more, protection less and capped at one ICE, HQ may run
    /// thinner, and credits are for spending.
    Rush,
    /// A glacier Corp: build the fort, then score behind it. ICE in front
    /// of an agenda is worth six times balanced and rewards a third piece,
    /// rezzed ICE and credits are worth more, advancement less, and a
    /// face-down install *less* — so the hand is not put on the table
    /// before there is money to turn it face up.
    Glacier,
    /// A trap Corp: wants the Runner's grip thin and cards on the table
    /// that might be anything. Three ambush terms and nothing else — see
    /// the profile — plus more credits kept (an ambush costs to fire),
    /// readier installs and a fuller HQ.
    Trap,
    /// An aggressive Runner: runs are worth double, unbroken subroutines
    /// and tags cost less, the grip may run thinner, and the opponent's
    /// credits are worth denying.
    Aggressive,
    /// A cautious Runner: a full rig before a run, a fuller grip, more
    /// saving for the breaker in hand, and a subroutine or a tag costs
    /// more.
    Cautious,
    /// A builder Runner: the rig that breaks ICE, and nothing else. A
    /// breaker for a new subtype and free memory are worth more, and a rig
    /// card that breaks nothing is worth less — one credit — so the
    /// credits go to breakers rather than onto the table.
    Builder,
    /// A wary Runner: treats face-down ICE as real. The one term in the
    /// evaluator that reads an unrezzed piece (`unrezzed_threat_weight`,
    /// zero for balanced) is switched on, so a run into ICE the rig
    /// cannot break is priced as the stop it probably is, and the Runner
    /// goes where the ICE is known.
    Wary,
}

impl Personality {
    pub const ALL: [Personality; 8] = [
        Personality::Balanced,
        Personality::Rush,
        Personality::Glacier,
        Personality::Trap,
        Personality::Aggressive,
        Personality::Cautious,
        Personality::Builder,
        Personality::Wary,
    ];

    /// The chair this profile is written for; `None` for `Balanced`.
    pub fn side(self) -> Option<Side> {
        match self {
            Personality::Balanced => None,
            Personality::Rush | Personality::Glacier | Personality::Trap => Some(Side::Corp),
            Personality::Aggressive | Personality::Cautious | Personality::Builder | Personality::Wary => Some(Side::Runner),
        }
    }

    /// The style a deck asks to be played in — `DeckFile::style` parsed
    /// against this vocabulary, `Balanced` when the deck names none.
    ///
    /// `Err` for a name that is not a profile, and for a profile written
    /// for the other chair: a Runner archetype under a Corp deck "touches
    /// only the shared terms, which is harmless and useless" (module
    /// docs), so it would seat a balanced bot under a misleading name.
    /// Every embedded deck is checked by
    /// `every_embedded_deck_style_is_a_personality_for_its_side`; this is
    /// the runtime check for a deck someone saved.
    pub fn for_deck(deck: &DeckFile) -> Result<Personality, String> {
        let Some(style) = deck.style.as_deref() else { return Ok(Personality::Balanced) };
        let personality: Personality = style.parse().map_err(|error| format!("deck {:?}: {error}", deck.id))?;
        match personality.side() {
            Some(side) if side != deck.side => Err(format!(
                "deck {:?} is a {:?} deck but its style {style:?} is a {side:?} archetype",
                deck.id, deck.side
            )),
            _ => Ok(personality),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Personality::Balanced => "balanced",
            Personality::Rush => "rush",
            Personality::Glacier => "glacier",
            Personality::Trap => "trap",
            Personality::Aggressive => "aggressive",
            Personality::Cautious => "cautious",
            Personality::Builder => "builder",
            Personality::Wary => "wary",
        }
    }

    /// The profile's evaluator weights. Every number here is a ratio
    /// against the balanced constant it replaces; the reasoning for the
    /// balanced value is on that constant in `eval`.
    pub fn weights(self) -> Weights {
        let base = Weights::default();
        match self {
            Personality::Balanced => base,
            Personality::Rush => Weights {
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
            Personality::Glacier => Weights {
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
                // At one ply: a first piece on HQ is 0.8 + 2.0, the
                // fort's first piece 0.8 + 1.5, and an agenda in a new
                // remote 0.8 − 2 × 5.0, where the credit click is 0.5.
                //
                // Against the balanced Runner, six seeds × 384, paired:
                // **0.247 → 0.462** (z 16.6), with every agenda installed
                // behind two pieces (30% before), all three centrals iced
                // at the first agenda in every game (14%), remotes given
                // ICE 3.16 → 2.18 a game and agendas scored 1.25 → 1.68.
                // Against each other Runner profile, two seeds × 384:
                // +0.223 to +0.237, z ≥ 10.0 each. A plateau, not a
                // peak: 2.5 / 2.0 / 4.0 plays the same games. The
                // exposure term is the one that carries it. At 0.0 the
                // other two cost 0.039, because a fort nobody waits for
                // is only ICE. At 1.5 and 3.0 (0.370, 0.410 on two seeds
                // against 0.499 here) an agenda still goes down behind
                // one piece in about half its installs. From about 4.0 it
                // waits for the second piece every time.
                agenda_protection_weight: 3.0,
                agenda_protection_cap: 3,
                rezzed_ice_weight: 1.8,
                unrezzed_install_weight: 0.8,
                advancement_weight: 1.2,
                own_credit_weight: 0.5,
                central_ice_weight: 2.0,
                central_ice_cap: 2,
                fort_weight: 1.5,
                fort_cap: 2,
                exposed_agenda_weight: 5.0,
                ..base
            },
            Personality::Trap => Weights {
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
                // Runner walks into it. The term is gone entirely, and
                // so is the reshaped version tried in its place: a one-ply
                // evaluator ranks actions, and no Corp action in the pool
                // makes the Runner's grip smaller. See `eval`.
                ambush_weight: 3.0,
                ambush_advancement_weight: 2.8,
                ambush_advancement_cap: 7,
                ..base
            },
            Personality::Aggressive => Weights {
                // A run at 1.2 beats a credit at every click, and the
                // opponent's credits are worth denying.
                //
                // **Audited against the balanced Corp (ROADMAP Phase 5
                // §16), and the grip and the subroutine went back to
                // balanced.** Shipped with a thinner grip (floor 2 at
                // 0.4 a card) and an unbroken subroutine at 0.7, the
                // profile lost +0.040 of Corp win share to balanced over
                // the same 2,304 games (z 4.4), and was flatlined 140
                // times where balanced was 36: the pressure it bought was
                // paid for in damage it could not absorb. Knob by knob,
                // back at balanced: `grip_floor` 2 → 3 is −0.020 (z 3.8),
                // `grip_shortfall_weight` 0.4 → 0.7 −0.010 (z 3.1), both
                // −0.034 (z 5.6); the rest alone move nothing against the
                // balanced Corp. Against the other Corp profiles the
                // subroutine at 0.7 also costs (−0.011 against `rush`,
                // z 3.1; −0.010 against `glacier`, z 2.3). Repaired, the
                // Corp's share falls 0.188 → 0.155 (z 5.3), flatlines
                // 140 → 58, and it falls against every Corp profile
                // (`rush` 0.155 → 0.118, `glacier` 0.322 → 0.299, `trap`
                // 0.260 → 0.218) while the profile still runs 20.7 times
                // a game to balanced's 17.9.
                //
                // **The run weight is the archetype, and it is its
                // remaining cost**, as `Rush`'s knobs are `Rush`'s: back
                // at 0.6 it is flat against the balanced Corp and −0.041
                // against `rush` (z 4.4), and 0.9 trades `glacier` for
                // `trap` without closing either. A Runner that does not
                // run more is not this profile, so it stays.
                active_run_weight: 1.2,
                savings_shortfall_weight: 0.15,
                tag_weight: 2.5,
                opponent_credit_weight: 0.4,
                ..base
            },
            Personality::Cautious => Weights {
                // A run is worth exactly a credit; a breaker for a new
                // subtype and the credits to afford one are worth more,
                // and every subroutine or tag costs half again.
                active_run_weight: 0.4,
                pending_subroutine_weight: 1.5,
                grip_floor: 4,
                grip_shortfall_weight: 0.9,
                savings_shortfall_weight: 0.5,
                breaker_coverage_weight: 4.0,
                tag_weight: 6.0,
                ..base
            },
            Personality::Builder => Weights {
                // Distinct from `Cautious`, which is about safety (grip
                // floor, tags, savings): this is about the rig.
                //
                // **Presence is *below* balanced, and that is the profile.**
                // It was 1.6 — "a rig card beats a credit click by four to
                // one" — and that made this the worst Runner profile in
                // the pool: the balanced Corp won 0.305 of 2,304 games
                // against it (six seeds × 384) where it wins 0.148 against
                // balanced. A flat bonus on *any* rig card outbids the
                // click that saves for a breaker (a 1[c] resource at 1.6 −
                // 0.4 = +1.2 against a credit's 0.4 + 0.3 shortfall =
                // +0.7), so the Runner spent its credits on the table,
                // sat on 3.5 of them by turn 3 and could not pay for the
                // breakers the coverage term was asking for: one breaker
                // subtype per 1.6 rig cards by turn 5. At 0.4 — exactly
                // `own_credit_weight`, so a card that breaks nothing is
                // worth the credit it costs and no more — the rig is
                // breakers (coverage 1.12 on a rig of 1.14 at turn 5),
                // and the Corp's win share falls to **0.121** over the
                // same games, below balanced, and by more than half
                // against every Corp profile (rush 0.253 → 0.066, glacier
                // 0.301 → 0.134, trap 0.344 → 0.188; ROADMAP Phase 5 §7).
                // The response is flat from 0.0 to 0.6 and climbs from
                // there (0.212 at 0.8 and 0.245 at 1.0, two seeds each),
                // so the claim is "no more than a credit", not the digit.
                //
                // `memory_weight` prices *free* memory (`memory_units` is
                // what is left, not what is used), so 0.8 values a
                // console's headroom and taxes each program's MU by the
                // same coin. It measured neutral at 0.25 and 0.5 once
                // presence was fixed (0.119, 0.120 against 0.123), and
                // `breaker_coverage_weight` at balanced's 3.0 read 0.107
                // — lower on four seeds of four but inside the seed-spread
                // band — so both stay as they were rather than ride along
                // with the one knob that carried 0.18.
                //
                // **The grip floor stays at 3**: the first cut dropped it
                // to 2 to empty a hand of programs onto the table, and the
                // Runner was flatlined 18 → 26 and 13 → 21 on two of three
                // seeds for it, losing ten games of 96 on every seed.
                // Installing from a thin grip is how a builder dies.
                board_presence_weight: 0.4,
                memory_weight: 0.8,
                breaker_coverage_weight: 4.0,
                ..base
            },
            Personality::Wary => Weights {
                // One term, and it reads the cards: `unrezzed_threat_
                // weight` counts unrezzed ICE ahead that no rig card
                // could break *and* that would end the run (ROADMAP Phase
                // 2 §5 items 38 and 40 built it for the search, where it
                // bought nothing and ships at 0.0). At 1.5 such a piece
                // costs more than an open run earns (0.6), so the Runner
                // does not walk into it; a rezzed piece it can break is
                // unchanged. `Cautious` is about the rig and the grip;
                // this is about the ICE.
                unrezzed_threat_weight: 1.5,
                ..base
            },
        }
    }
}

impl fmt::Display for Personality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Personality {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Personality::ALL
            .into_iter()
            .find(|personality| personality.name().eq_ignore_ascii_case(s))
            .ok_or_else(|| format!("unknown personality {s:?}; one of {}", Personality::ALL.map(Personality::name).join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_is_the_default_weights_and_the_default_personality() {
        assert_eq!(Personality::default(), Personality::Balanced);
        assert_eq!(Personality::Balanced.weights(), Weights::default());
    }

    #[test]
    fn every_profile_round_trips_through_its_name() {
        for personality in Personality::ALL {
            assert_eq!(personality.to_string().parse::<Personality>().unwrap(), personality);
        }
        assert_eq!("RUSH".parse::<Personality>().unwrap(), Personality::Rush);
        assert!("berserk".parse::<Personality>().is_err());
    }

    /// The direction of each profile's bias, against the balanced value.
    #[test]
    fn profiles_move_the_terms_their_doc_comments_name_in_the_direction_they_say() {
        let base = Weights::default();
        let rush = Personality::Rush.weights();
        assert!(rush.advancement_weight > base.advancement_weight && rush.agenda_protection_weight < base.agenda_protection_weight);
        assert!(rush.installed_agenda_weight > 0.0 && base.installed_agenda_weight == 0.0);
        assert!(rush.agenda_protection_cap < base.agenda_protection_cap);
        let glacier = Personality::Glacier.weights();
        assert!(glacier.agenda_protection_weight > base.agenda_protection_weight && glacier.advancement_weight < base.advancement_weight);
        assert!(glacier.agenda_protection_cap > base.agenda_protection_cap);
        // A face-down install out of a thin HQ must not beat this profile's
        // own credit click, or it buries its hand before it can rez it
        // (ROADMAP Phase 5 §9: the switch is worth 0.071).
        assert!(glacier.unrezzed_install_weight - glacier.hq_shortfall_weight < glacier.own_credit_weight);
        let trap = Personality::Trap.weights();
        assert!(trap.ambush_weight > 0.0 && base.ambush_weight == 0.0, "the lever the profile was missing");
        assert!(trap.ambush_advancement_weight > base.ambush_advancement_weight);
        assert!(trap.ambush_advancement_cap > base.ambush_advancement_cap);
        // The profile is *only* its ambush terms — the retune measured
        // every other knob it used to carry as a cost, and the grip term
        // it was named for as inert. If a later change reintroduces one,
        // it should have to justify it against balanced.
        assert_eq!(
            Weights { ambush_weight: base.ambush_weight, ambush_advancement_weight: base.ambush_advancement_weight, ambush_advancement_cap: base.ambush_advancement_cap, ..trap },
            base,
            "Trap deviates from balanced in its ambush terms and nothing else"
        );
        let aggressive = Personality::Aggressive.weights();
        assert!(aggressive.active_run_weight > base.active_run_weight);
        // A thinner grip got this profile flatlined four times as often as
        // balanced (ROADMAP Phase 5 §16: repaired, +0.040 → +0.007).
        assert!(aggressive.grip_floor >= base.grip_floor && aggressive.grip_shortfall_weight >= base.grip_shortfall_weight);
        assert!(aggressive.pending_subroutine_weight >= base.pending_subroutine_weight);
        let cautious = Personality::Cautious.weights();
        assert!(cautious.active_run_weight < base.active_run_weight && cautious.pending_subroutine_weight > base.pending_subroutine_weight);
        assert!(cautious.breaker_coverage_weight > base.breaker_coverage_weight);
        let builder = Personality::Builder.weights();
        // Presence *below* balanced and coverage above it: a flat bonus on
        // any rig card outbid saving for the breakers, and made this the
        // worst Runner profile in the pool (ROADMAP Phase 5 §7).
        assert!(builder.board_presence_weight < base.board_presence_weight, "a builder pays for coverage, not for cards");
        assert!(builder.board_presence_weight <= builder.own_credit_weight, "a rig card that breaks nothing is worth no more than a credit");
        assert!(builder.breaker_coverage_weight > base.breaker_coverage_weight && builder.memory_weight > base.memory_weight);
        assert_eq!(builder.grip_floor, base.grip_floor, "a builder that installs from a thin grip gets flatlined for it");
        let wary = Personality::Wary.weights();
        assert!(wary.unrezzed_threat_weight > base.unrezzed_threat_weight && base.unrezzed_threat_weight == 0.0);
        assert_eq!(Weights { unrezzed_threat_weight: base.unrezzed_threat_weight, ..wary }, base, "Wary is its one term and nothing else");
    }

    /// The gate on `DeckFile::style`: the vocabulary lives here, the data
    /// lives in `netrunner_core`, and this is the one place both are in
    /// scope. A typo in a deck file fails the build rather than seating a
    /// balanced bot under a rush deck's name.
    #[test]
    fn every_embedded_deck_style_is_a_personality_for_its_side() {
        let mut styled = 0;
        for deck in netrunner_core::decks::embedded_decks() {
            let personality = Personality::for_deck(&deck).unwrap_or_else(|error| panic!("{error}"));
            if deck.style.is_some() {
                styled += 1;
                assert_ne!(personality, Personality::Balanced, "{}: a deck that names a style should not name balanced", deck.id);
            }
        }
        assert!(styled >= 12, "the sample decks carry styles; only {styled} do");
    }

    #[test]
    fn a_deck_style_is_parsed_and_checked_against_the_chair() {
        let mut deck = netrunner_core::decks::by_id("stolen_goods").expect("embedded");
        deck.style = None;
        assert_eq!(Personality::for_deck(&deck).unwrap(), Personality::Balanced);
        deck.style = Some("Aggressive".to_string());
        assert_eq!(Personality::for_deck(&deck).unwrap(), Personality::Aggressive);
        deck.style = Some("rush".to_string());
        let error = Personality::for_deck(&deck).unwrap_err();
        assert!(error.contains("Corp archetype"), "{error}");
        deck.style = Some("berserk".to_string());
        assert!(Personality::for_deck(&deck).unwrap_err().contains("unknown personality"));
    }

    #[test]
    fn corp_profiles_are_for_the_corp_and_runner_profiles_for_the_runner() {
        assert_eq!(Personality::Rush.side(), Some(Side::Corp));
        assert_eq!(Personality::Glacier.side(), Some(Side::Corp));
        assert_eq!(Personality::Trap.side(), Some(Side::Corp));
        assert_eq!(Personality::Aggressive.side(), Some(Side::Runner));
        assert_eq!(Personality::Cautious.side(), Some(Side::Runner));
        assert_eq!(Personality::Balanced.side(), None);
        for runner in [Personality::Builder, Personality::Wary] {
            assert_eq!(runner.side(), Some(Side::Runner));
        }
    }
}
