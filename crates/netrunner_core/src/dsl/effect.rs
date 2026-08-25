use serde::{Deserialize, Serialize};

use crate::dsl::card::{CardId, IceType};
use crate::rules::{ServerId, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageType {
    Net,
    Meat,
    Brain,
}

/// Which ordered deck zone a `TrashCard(CardTarget::TopOfStack)` effect
/// mills from — the only two zones in `GameState` that have a meaningful
/// "top."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackZone {
    RAndD,
    Stack,
}

/// What an `Effect::TrashCard` targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardTarget {
    /// The card this ability/subroutine/trigger is itself printed on. Must
    /// be resolved to a concrete target by the dispatch layer before
    /// reaching `evaluate_effect` — that function has no "which card is
    /// resolving" context on its own.
    ThisCard,
    /// A Corp card installed on a server, identified the same way
    /// `state::InstalledCard` already identifies one (`CardId` +
    /// `ServerId`).
    CorpInstalled { card: CardId, server: ServerId },
    /// A Runner card in the Rig — no server/slot component, since
    /// `RunnerState::rig` is a flat `Vec<CardId>` with no per-card
    /// location metadata.
    RunnerRig(CardId),
    /// The top card of an ordered deck zone, without needing to name it —
    /// covers "mill" effects (trash without revealing).
    TopOfStack { side: Side, zone: StackZone },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    /// `Side` is explicit — even though most cards only ever grant
    /// credits to their own controller (and `Card::side` already implies
    /// that), an explicit target lets a card affect the opponent instead.
    GainCredits(Side, u32),
    /// Renamed from `InflictDamage`. `usize` (not `u32`) matches
    /// `damage::apply_damage`'s existing signature exactly. No `Side`
    /// param: damage in this engine's model always targets the Runner,
    /// same as `apply_damage` itself.
    DealDamage(DamageType, usize),
    /// Never side-ambiguous — always targets whatever ICE the current run
    /// is encountering. Unlike `RunAction::BreakSubroutine`'s index (chosen
    /// by the player breaking a subroutine manually), this `usize` is the
    /// index of the specific subroutine this effect itself is wired to
    /// break — a target, not a count.
    BreakSubroutine(usize),
    ModifyStrength(i32),
    /// `Side`-explicit for the same reason as `GainCredits`.
    DrawCards(Side, u32),
    /// Ends whatever run is in `GameState::active_run`. No payload — there
    /// is exactly one active run at a time.
    EndTheRun,
    /// Deliberately no `Side` param, unlike `GainCredits`/`DrawCards` —
    /// tags exist solely on `RunnerState` in this data model, so
    /// `Side::Corp` would never be a legal target.
    GiveTags(u32),
    /// Deliberately no `Side` param, same rationale as `GiveTags`.
    RemoveTags(u32),
    /// Deliberately no `Side` param — Bad Publicity exists solely on
    /// `CorpState` in this data model, same rationale as `GiveTags`.
    GiveBadPublicity(u32),
    /// Deliberately no `Side` param, same rationale as `GiveBadPublicity`.
    RemoveBadPublicity(u32),
    TrashCard(CardTarget),
    /// Boosts a Runner rig card's own strength — unlike `ModifyStrength`,
    /// which always targets whatever ICE is currently being encountered,
    /// this always targets whichever rig card activated the ability (see
    /// `evaluate_effect`'s `acting_card` parameter). `Encounter`-duration
    /// boosts are cleared when the encounter ends
    /// (`RunnerState::reset_encounter_strength_buffs`); `Turn`-duration
    /// boosts are cleared at the end of the Runner's turn
    /// (`RunnerState::reset_turn_strength_buffs`).
    BoostStrength { amount: u32, duration: BoostDuration },
    /// Breaks pending subroutines on the ICE currently being encountered,
    /// gated on the acting rig card's `effective_strength()` meeting the
    /// ICE's `current_strength` (`RulesError::BreakerStrengthTooLow`
    /// otherwise). `restrict_to`, if set, further gates this on the ICE's
    /// subtype matching (`RulesError::InvalidBreakerSubtype` otherwise) —
    /// e.g. Corroder's `Some(IceType::Barrier)`. `None` is a universal
    /// breaker: no subtype restriction.
    BreakSubroutines {
        count: SubroutineBreakCount,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        restrict_to: Option<IceType>,
    },
    /// Establishes a trace of strength `base` (plus whatever the Corp
    /// commits on top once bidding begins). Does not resolve `on_success`
    /// synchronously — unlike every other variant, this effect alone cannot
    /// complete within one `evaluate_effect` call, since it spans two future
    /// `PlayerAction`s (the Corp's bid, then the Runner's). `evaluate_effect`
    /// instead parks the pending state in `GameState::active_trace` and
    /// returns immediately; `rules::trace::submit_runner_bid` is what
    /// eventually evaluates `on_success`, if the trace succeeds. `Box`ed
    /// since this is the first `Effect` variant that nests another `Effect`.
    Trace { base: u32, on_success: Box<Effect> },
    /// Grants `count` additional cards accessed from `server` on top of the
    /// normal single-card access, for the remainder of the current run —
    /// e.g. a Runner program's "access 1 additional card from HQ" ability.
    /// Requires an active run (`RulesError::NoActiveRun` otherwise);
    /// silently no-ops for `ServerId::Archives`/`ServerId::Remote(_)`,
    /// which already access every card/every root install respectively and
    /// have no "additional count" field to increment — see
    /// `RunState::additional_hq_access`/`additional_rd_access`, which only
    /// exist for the two central servers whose access is naturally capped
    /// at one card.
    AddAdditionalAccess { server: ServerId, count: u32 },
    /// Replaces this run's normal access of `server` with `effect` instead
    /// — e.g. Account Siphon's "gain 8 credits instead of accessing HQ".
    /// Consumed (and the run concluded) the moment `run::access_server` is
    /// next called against `server`; see `run::access::try_replace_access`.
    /// Requires an active run (`RulesError::NoActiveRun` otherwise).
    /// `Box`ed for the same reason as `Trace::on_success` — the first two
    /// other variants that nest another `Effect`.
    SetAccessReplacement { server: ServerId, effect: Box<Effect> },
}

/// How long an `Effect::BoostStrength` buff lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoostDuration {
    /// Cleared when the current ICE encounter ends.
    Encounter,
    /// Cleared at the end of the Runner's turn.
    Turn,
}

/// How many pending subroutines an `Effect::BreakSubroutines` breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubroutineBreakCount {
    /// Breaks up to this many pending subroutines, lowest-id first —
    /// breaks fewer (not an error) if fewer are pending, mirroring
    /// `Effect::DrawCards`'s "stop silently on empty" precedent.
    Fixed(u32),
    /// Breaks every currently-pending subroutine.
    All,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_strength_and_break_subroutines_round_trip_through_json() {
        let boost = Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter };
        let boost_json = serde_json::to_string(&boost).unwrap();
        assert_eq!(boost_json, r#"{"BoostStrength":{"amount":1,"duration":"Encounter"}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&boost_json).unwrap(), boost);

        let turn_boost = Effect::BoostStrength { amount: 2, duration: BoostDuration::Turn };
        let turn_boost_json = serde_json::to_string(&turn_boost).unwrap();
        assert_eq!(turn_boost_json, r#"{"BoostStrength":{"amount":2,"duration":"Turn"}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&turn_boost_json).unwrap(), turn_boost);

        let fixed = Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None };
        let fixed_json = serde_json::to_string(&fixed).unwrap();
        assert_eq!(fixed_json, r#"{"BreakSubroutines":{"count":{"Fixed":1}}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&fixed_json).unwrap(), fixed);

        let all = Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None };
        let all_json = serde_json::to_string(&all).unwrap();
        assert_eq!(all_json, r#"{"BreakSubroutines":{"count":"All"}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&all_json).unwrap(), all);
    }

    #[test]
    fn break_subroutines_restrict_to_round_trips_through_json() {
        let restricted = Effect::BreakSubroutines {
            count: SubroutineBreakCount::Fixed(1),
            restrict_to: Some(crate::dsl::card::IceType::Barrier),
        };
        let restricted_json = serde_json::to_string(&restricted).unwrap();
        assert_eq!(
            restricted_json,
            r#"{"BreakSubroutines":{"count":{"Fixed":1},"restrict_to":"Barrier"}}"#
        );
        assert_eq!(serde_json::from_str::<Effect>(&restricted_json).unwrap(), restricted);

        // Absent restrict_to key still parses fine (backward-compatible with
        // older JSON that predates this field).
        let no_restrict_json = r#"{"BreakSubroutines":{"count":{"Fixed":1}}}"#;
        assert_eq!(
            serde_json::from_str::<Effect>(no_restrict_json).unwrap(),
            Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }
        );
    }

    #[test]
    fn trace_round_trips_through_json() {
        let trace = Effect::Trace { base: 3, on_success: Box::new(Effect::GiveTags(1)) };
        let trace_json = serde_json::to_string(&trace).unwrap();
        assert_eq!(trace_json, r#"{"Trace":{"base":3,"on_success":{"GiveTags":1}}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&trace_json).unwrap(), trace);
    }

    #[test]
    fn add_additional_access_round_trips_through_json() {
        let effect = Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 };
        let json = serde_json::to_string(&effect).unwrap();
        assert_eq!(json, r#"{"AddAdditionalAccess":{"server":"Hq","count":1}}"#);
        assert_eq!(serde_json::from_str::<Effect>(&json).unwrap(), effect);
    }

    #[test]
    fn set_access_replacement_round_trips_through_json() {
        let effect = Effect::SetAccessReplacement {
            server: ServerId::Hq,
            effect: Box::new(Effect::GainCredits(Side::Runner, 8)),
        };
        let json = serde_json::to_string(&effect).unwrap();
        assert_eq!(
            json,
            r#"{"SetAccessReplacement":{"server":"Hq","effect":{"GainCredits":["Runner",8]}}}"#
        );
        assert_eq!(serde_json::from_str::<Effect>(&json).unwrap(), effect);
    }
}
