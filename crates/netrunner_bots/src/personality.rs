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
//! evaluator already has a term for; `Weights::opponent_grip_weight` and
//! `Weights::installed_agenda_weight` were added (at zero by default, so
//! balanced play is byte-identical to before they existed) because the
//! trap and rush archetypes had no lever at all without them.
//!
//! **Each archetype is written for one chair.** A Corp profile seated as
//! the Runner touches only the shared terms (credits), which is harmless
//! and useless; `Personality::side` says which chair a profile is for,
//! and the CLI's help text repeats it. `Balanced` is the default
//! everywhere and is exactly `Weights::default()`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

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
    /// A glacier Corp: build the fort, then score behind it. Protection
    /// is worth double and rewards a third ICE, rezzed ICE and installs
    /// are worth more, advancement less, and credits more.
    Glacier,
    /// A trap Corp: wants the Runner's grip thin and cards on the table
    /// that might be anything. The only profile with a term of its own
    /// (`opponent_grip_weight`); also keeps more credits (an ambush
    /// costs to fire), installs more readily and holds a fuller HQ.
    Trap,
    /// An aggressive Runner: runs are worth double, unbroken subroutines
    /// and tags cost less, the grip may run thinner, and the opponent's
    /// credits are worth denying.
    Aggressive,
    /// A cautious Runner: a full rig before a run, a fuller grip, more
    /// saving for the breaker in hand, and a subroutine or a tag costs
    /// more.
    Cautious,
}

impl Personality {
    pub const ALL: [Personality; 6] = [
        Personality::Balanced,
        Personality::Rush,
        Personality::Glacier,
        Personality::Trap,
        Personality::Aggressive,
        Personality::Cautious,
    ];

    /// The chair this profile is written for; `None` for `Balanced`.
    pub fn side(self) -> Option<Side> {
        match self {
            Personality::Balanced => None,
            Personality::Rush | Personality::Glacier | Personality::Trap => Some(Side::Corp),
            Personality::Aggressive | Personality::Cautious => Some(Side::Runner),
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
                // ICE in front of an agenda is worth double and a third
                // piece still pays; an install at 1.2 + 1.0 = +2.2 beats
                // advancing at 1.2 − 0.5 = +0.7.
                agenda_protection_weight: 1.0,
                agenda_protection_cap: 3,
                rezzed_ice_weight: 1.8,
                unrezzed_install_weight: 1.2,
                advancement_weight: 1.2,
                own_credit_weight: 0.5,
                ..base
            },
            Personality::Trap => Weights {
                // Half a point per card in the Runner's grip: a net
                // damage is +0.5 and an emptied grip is +2.5, on the
                // scale where an agenda point is 20.
                opponent_grip_weight: 0.5,
                // **The lever the first cut of this profile was missing.**
                // It valued a thin grip with no way to recognise the
                // cards that would thin it, so it only ever paid for the
                // wish: ROADMAP Phase 3 §1 records it losing to balanced,
                // 37 Corp wins to 42. A face-down ambush is now worth
                // more than a plain install to *this* profile alone, and
                // its tokens are worth nearly an agenda's, since damage
                // is what this Corp is buying with them.
                ambush_weight: 1.0,
                ambush_advancement_weight: 1.4,
                ambush_advancement_cap: 4,
                own_credit_weight: 0.5,
                unrezzed_install_weight: 1.3,
                agenda_protection_weight: 0.3,
                hq_floor: 4,
                ..base
            },
            Personality::Aggressive => Weights {
                // A run at 1.2 beats a draw below the floor (0.4 × ...)
                // and a credit at every click; a subroutine left unbroken
                // costs 0.7, so a run through one is still worth it.
                active_run_weight: 1.2,
                pending_subroutine_weight: 0.7,
                grip_shortfall_weight: 0.4,
                grip_floor: 2,
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
        let trap = Personality::Trap.weights();
        assert!(trap.opponent_grip_weight > 0.0 && base.opponent_grip_weight == 0.0);
        assert!(trap.ambush_weight > 0.0 && base.ambush_weight == 0.0, "the lever the profile was missing");
        assert!(trap.ambush_advancement_weight > base.ambush_advancement_weight);
        assert!(trap.ambush_advancement_cap > base.ambush_advancement_cap);
        let aggressive = Personality::Aggressive.weights();
        assert!(aggressive.active_run_weight > base.active_run_weight && aggressive.pending_subroutine_weight < base.pending_subroutine_weight);
        let cautious = Personality::Cautious.weights();
        assert!(cautious.active_run_weight < base.active_run_weight && cautious.pending_subroutine_weight > base.pending_subroutine_weight);
        assert!(cautious.breaker_coverage_weight > base.breaker_coverage_weight);
    }

    #[test]
    fn corp_profiles_are_for_the_corp_and_runner_profiles_for_the_runner() {
        assert_eq!(Personality::Rush.side(), Some(Side::Corp));
        assert_eq!(Personality::Glacier.side(), Some(Side::Corp));
        assert_eq!(Personality::Trap.side(), Some(Side::Corp));
        assert_eq!(Personality::Aggressive.side(), Some(Side::Runner));
        assert_eq!(Personality::Cautious.side(), Some(Side::Runner));
        assert_eq!(Personality::Balanced.side(), None);
    }
}
