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
#[serde(deny_unknown_fields)]
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
    /// A conditional discount off this specific ability's `cost` —
    /// `(condition, amount)` — applied every time `condition` holds, e.g.
    /// Marjanah's "if you made a successful run this turn, this ability
    /// costs 1 credit less to use." `None` for the common case. See
    /// `CardDefinition::install_cost_discount_if`'s doc comment for the
    /// matching per-install-cost sibling; `engine::activate_ability`'s cost
    /// computation reads this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_discount_if: Option<(EffectRequirement, u32)>,
}

/// A precondition gating an `AbilityDef`'s activation, a `CardDefinition::
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
    /// Soft-gate only: true exactly once per turn per `tag`, backed by the
    /// acting side's `once_per_turn_used` set (`CorpState`/`RunnerState`) —
    /// the generalized replacement for `FirstInstallThisTurn`/
    /// `FirstSuccessfulHqRunThisTurn`'s one-bool-per-effect pattern, used for
    /// every *new* once-per-turn gate so a fresh bespoke bool field isn't
    /// needed each time. Which side's set is consulted is determined by
    /// context (the card's own `side`, resolved via `acting_card` — see
    /// `rules::ability::check_requirement`), not carried on this variant
    /// itself. The two existing bespoke variants are left as-is.
    OncePerTurn(String),
    /// The Runner's credit total is at most `0` — e.g. Whitespace's second
    /// subroutine ("if the Runner has 6 credits or less, end the run").
    RunnerCreditsAtMost(u32),
    /// The Runner has at least this many clicks left — e.g. Creative
    /// Commission/VRcation's "if you have any [click] remaining, lose
    /// [click]" (`RunnerClicksAtLeast(1)`). Checked *after* the event's own
    /// play cost has been paid, so playing one as the turn's last click
    /// correctly finds zero remaining and skips the loss rather than
    /// underflowing.
    RunnerClicksAtLeast(u32),
    /// Generic negation of another requirement.
    Not(Box<EffectRequirement>),
    /// Generic conjunction of two requirements — e.g. Zahya Sadeghi's
    /// "once per turn" gate combined with "only when the run targeted HQ
    /// or R&D."
    And(Box<EffectRequirement>, Box<EffectRequirement>),
    /// A run is currently active against the server the checking card
    /// (`acting_card`/the triggering card) is itself installed in, and
    /// that run hasn't passed the ICE-encounter stage yet (`RunPhase::
    /// ApproachIce`/`EncounterIce`) — e.g. Ping's "when you rez this ice
    /// during a run against this server."
    RezzedDuringRunAgainstThisServer,
    /// The Runner made a successful run during their immediately preceding
    /// turn (`RunnerState::made_successful_run_last_turn`) — e.g. Public
    /// Trail's play requirement.
    RunnerMadeSuccessfulRunLastTurn,
    /// The most recent `Effect::DealDamage` discarded at least one card
    /// whose registry `cost` is odd (`GameState::last_discarded_cards`,
    /// overwritten by every `damage::apply_damage` call) — e.g. Diviner's
    /// subroutine ("if you trash a card this way with a printed play or
    /// install cost that is an odd number, end the run").
    LastDamageTrashedOddCostCard,
    /// The most recently concluded run (`GameState::last_completed_run`)
    /// targeted HQ or R&D — e.g. Zahya Sadeghi's "when a run on HQ or R&D
    /// ends."
    LastRunWasOnHqOrRnD,
    /// The Runner stole at least one agenda during the most recently
    /// concluded run (`state::CompletedRun::agendas_stolen`) — e.g. AMAZE
    /// Amusements' "whenever a run on this server ends, if the Runner stole
    /// any agendas during that run". Pairs with `Trigger::OnRunEnded`,
    /// which is the only point at which that snapshot is populated.
    StoleAgendaDuringLastRun,
    /// At least one card in the Corp's Archives is facedown
    /// (`state::ArchivedCard::facedown`) — e.g. Jinteki: Restoring
    /// Humanity's "if there is a facedown card in Archives".
    ArchivesHasFacedownCard,
    /// The Runner has made at least one successful run this turn
    /// (`RunnerState::made_successful_run_this_turn`) — e.g. Mutual Favor's
    /// "if you made a successful run this turn, you may install [the found
    /// program]." Introduced ahead of its originally planned milestone
    /// (formalized further alongside Carmen/Marjanah's cost discounts) since
    /// the backing state (`made_successful_run_this_turn`) already existed
    /// and Mutual Favor needed the read now — no behavior change expected
    /// when those cards are added later.
    MadeSuccessfulRunThisTurn,
    /// `acting_card`'s current generic counter total (wherever it's
    /// currently installed/rigged) is at most `amount` — e.g. a
    /// hosted-credit-pool resource/asset detecting "this pool is now empty"
    /// right after spending it down, to gate an auto-trash `EffectIf`
    /// (Red Team, Telework Contract, Regolith Mining License, Nico
    /// Campaign). `RulesError::RequirementNotMet` (not a "card not found"
    /// error) if `acting_card` isn't currently installed/rigged at all —
    /// treated the same as "0 counters," since a card that's already gone
    /// trivially has no counters left to be above the threshold.
    ThisCardCountersAtMost(u32),
    /// A run is currently active and the Runner is mid-resolution of a
    /// specific accessed card (`run::AccessPhase::PendingChoice`) — e.g.
    /// Carnivore's "Access, once per turn → trash 2 cards from your grip:
    /// trash the card you are accessing," gating an `AbilityDef` so it's
    /// only activatable at that exact decision point. Hard-gates (errors,
    /// doesn't silently skip) via `AbilityDef::requirement`'s usual
    /// treatment.
    CurrentlyAccessingACard,
    /// `acting_card`'s current generic counter total is at least `amount`
    /// — the inverse comparison to `ThisCardCountersAtMost` — e.g.
    /// Tranquilizer's "if there are 3 or more hosted virus counters, derez
    /// host ice." Same "not installed/rigged at all" treatment as
    /// `ThisCardCountersAtMost` (0 counters, so never satisfied for
    /// `amount >= 1`).
    ThisCardCountersAtLeast(u32),
    /// `acting_card` is a Trojan Program (`dsl::CardDefinition::
    /// installs_on_ice`, `state::InstalledRunnerCard::hosted_on_ice`)
    /// currently hosted on the ICE the active run is encountering right
    /// now (`RunPhase::EncounterIce`, `RunState::ice[position]` matches
    /// the host) — e.g. Botulus's hosted-counter break ability, which only
    /// makes sense to activate while its host is actually being
    /// encountered. Hard-gates via `AbilityDef::requirement`, same
    /// treatment as `CurrentlyAccessingACard`.
    EncounteringHostIce,
    /// The advancement just placed by the `Trigger::OnAdvance` event
    /// currently being dispatched was the first one this card has ever
    /// received (`GameState::last_advancement_was_first`, a transient
    /// snapshot `engine::advance_card` overwrites on every advance,
    /// mirroring `last_discarded_cards`'s "state field, not a threaded
    /// event payload" shape) — e.g. Weyland Consortium: Built to Last's
    /// "whenever you advance a card, gain 2 credits if it had no
    /// advancement counters."
    WasFirstAdvancementThisCard,
}

/// An optional "may pay `cost` to prevent `effects`" access-time trigger —
/// e.g. Fetal AI's "pay 2 [credit] to avoid 2 net damage." Lives on
/// `dsl::card::CardDefinition::interactive_on_access`, `None` for the common case of
/// no such trigger. Resolved before the card's normal (unconditional)
/// `Trigger::OnAccessed` effects, via `rules::run::state::AccessPhase::
/// PendingInteractiveTrigger` and `PlayerAction::PayToAvoidAccessTrigger`/
/// `DeclineAccessTrigger` (`rules::run::access::resolve_pay_to_avoid`/
/// `resolve_decline_to_avoid`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
/// `CardDefinition::subroutines` at run start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
                cost_discount_if: None,
            }
        );
        assert_eq!(
            bundle.abilities[1],
            AbilityDef {
                trigger: Trigger::Paid,
                cost: Some(Cost::TrashSelf),
                requirement: None,
                effect: Effect::GiveTags(1),
                cost_discount_if: None,
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
