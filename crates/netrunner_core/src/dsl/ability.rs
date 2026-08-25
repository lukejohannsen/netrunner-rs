use serde::{Deserialize, Serialize};

use crate::dsl::cost::Cost;
use crate::dsl::effect::Effect;
use crate::dsl::trigger::Trigger;

/// A single costed/manually-activated ability: when/how it fires
/// (`trigger`), what must be paid to make it fire (`cost` — `None` for
/// automatic triggers), and what it does (`effect`, singular). Distinct
/// from `dsl::card::TriggeredEffect`, which models the common no-cost,
/// possibly-multi-effect case ("when played, do these N things for
/// free") — `AbilityDef` is scoped to what `TriggeredEffect` structurally
/// can't express: an optional `Cost`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityDef {
    pub trigger: Trigger,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    /// A precondition gating whether this ability may even be activated,
    /// checked (via `rules::ability::check_requirement`) before `cost` is
    /// paid. `None` for the common case of no precondition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<EffectRequirement>,
    pub effect: Effect,
}

/// A precondition gating an `AbilityDef`'s activation, a `Card::
/// play_requirement`'s play legality, or (as a soft/silent gate — see
/// `dsl::card::TriggeredEffect::requirement`) a `TriggeredEffect`'s firing.
/// Kept minimal — extend as new tag/state-conditional card text is needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectRequirement {
    /// The Runner must have at least one tag (`RunnerState::is_tagged()`).
    IsTagged,
    /// Soft-gate only (`dsl::card::TriggeredEffect::requirement`): true
    /// exactly once per Corp turn, the first time the Corp installs a card
    /// (`CorpState::first_install_used_this_turn` not yet consumed this
    /// turn) — e.g. Haas-Bioroid: Engineering the Future.
    FirstInstallThisTurn,
    /// Soft-gate only: true exactly once per Runner turn, the first time a
    /// run on HQ succeeds (`RunnerState::first_hq_run_used_this_turn` not
    /// yet consumed this turn) — e.g. Gabriel Santiago.
    FirstSuccessfulHqRunThisTurn,
}

/// An optional "may pay `cost` to prevent `effects`" access-time trigger —
/// e.g. Fetal AI's "pay 2 [credit] to avoid 2 net damage." Lives on
/// `dsl::card::Card::interactive_on_access`, `None` for the common case of
/// no such trigger. Resolved before the card's normal (unconditional)
/// `Trigger::OnAccessed` effects, via `rules::run::state::AccessPhase::
/// PendingInteractiveTrigger` and `PlayerAction::PayToAvoidAccessTrigger`/
/// `DeclineAccessTrigger` (`rules::run::access::resolve_pay_to_avoid`/
/// `resolve_decline_to_avoid`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractiveOnAccess {
    pub cost: Cost,
    pub effects: Vec<Effect>,
}

/// A subroutine's printed text and the `Effect` it resolves into, when the
/// Corp lets it fire (or the Runner fails to break it).
///
/// Wired into `rules::run::state::RunIce` via `EncounteredSubroutine`,
/// individually addressable and status-tracked
/// (`SubroutineStatus::{Pending, Broken, Resolved}`);
/// `rules::run::engine::step_subroutine`/`transition_subroutine` consult
/// and fire the real `Effect` payload on resolution. `engine::initiate_run`
/// (via `build_run_ice`) populates real `SubroutineDef`s from
/// `Card::subroutines` at run start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubroutineDef {
    pub text: String,
    pub effect: Effect,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardId, CardTarget, DamageType};

    /// Test-local wrapper, not a production type — exists only to exercise
    /// `AbilityDef`/`SubroutineDef` (and, through them, `Cost`, `Effect`,
    /// `CardTarget`) together in one JSON document.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Bundle {
        abilities: Vec<AbilityDef>,
        subroutines: Vec<SubroutineDef>,
    }

    const BUNDLE_JSON: &str = r#"
    {
        "abilities": [
            {
                "trigger": "Paid",
                "cost": { "Credits": 3 },
                "effect": { "DealDamage": ["Net", 1] }
            },
            {
                "trigger": "Paid",
                "cost": "TrashSelf",
                "effect": { "GiveTags": 1 }
            }
        ],
        "subroutines": [
            {
                "text": "Trash a program.",
                "effect": { "TrashCard": { "RunnerRig": "gordian_blade" } }
            }
        ]
    }
    "#;

    #[test]
    fn ability_and_subroutine_ast_round_trips_through_json() {
        let bundle: Bundle = serde_json::from_str(BUNDLE_JSON).expect("valid ability/subroutine JSON");

        assert_eq!(
            bundle.abilities[0],
            AbilityDef {
                trigger: Trigger::Paid,
                cost: Some(Cost::Credits(3)),
                requirement: None,
                effect: Effect::DealDamage(DamageType::Net, 1),
            }
        );
        assert_eq!(
            bundle.abilities[1],
            AbilityDef {
                trigger: Trigger::Paid,
                cost: Some(Cost::TrashSelf),
                requirement: None,
                effect: Effect::GiveTags(1),
            }
        );
        assert_eq!(
            bundle.subroutines[0],
            SubroutineDef {
                text: "Trash a program.".to_string(),
                effect: Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))),
            }
        );

        // Round-trip: re-serialize then re-parse — locks in the wire format
        // both directions, not just one-way parsing of a hand-written fixture.
        let re_serialized = serde_json::to_string(&bundle).expect("bundle should serialize");
        let round_tripped: Bundle =
            serde_json::from_str(&re_serialized).expect("re-serialized JSON should parse");
        assert_eq!(bundle, round_tripped);
    }
}
