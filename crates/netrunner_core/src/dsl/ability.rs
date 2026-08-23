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
    pub effect: Effect,
}

/// A subroutine's printed text and the `Effect` it resolves into, when the
/// Corp lets it fire (or the Runner fails to break it).
///
/// Deliberately NOT wired into `rules::run::state::RunIce` yet — `RunIce`
/// stays a bare `{ subroutines_pending: u32 }` counter, and
/// `rules::run::engine::step_subroutine` keeps treating subroutine
/// resolution as pure decrement-the-counter bookkeeping with no effect
/// payload consulted. This is inert data only for now.
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
                effect: Effect::DealDamage(DamageType::Net, 1),
            }
        );
        assert_eq!(
            bundle.abilities[1],
            AbilityDef {
                trigger: Trigger::Paid,
                cost: Some(Cost::TrashSelf),
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
