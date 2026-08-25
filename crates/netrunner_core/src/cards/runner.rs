//! Baseline Core Set Runner cards: Events and icebreaker Programs.

use crate::cards::common::base_card;
use crate::cards::CardRegistry;
use crate::dsl::{
    AbilityDef, BoostDuration, Card, CardType, Cost, Effect, IceType, SubroutineBreakCount, Trigger, TriggeredEffect,
};
use crate::rules::{ServerId, Side};

/// Sure Gamble — "Gain 9 credits." Same stat line as the existing
/// `data/runner/sure_gamble.json` DSL round-trip fixture.
fn sure_gamble() -> Card {
    let mut card = base_card("sure_gamble", "Sure Gamble", Side::Runner, CardType::Event, 5);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnPlay,
        effects: vec![Effect::GainCredits(Side::Runner, 9)],
        requirement: None,
    }];
    card
}

/// Diesel — "Draw 3 cards."
fn diesel() -> Card {
    let mut card = base_card("diesel", "Diesel", Side::Runner, CardType::Event, 0);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnPlay,
        effects: vec![Effect::DrawCards(Side::Runner, 3)],
        requirement: None,
    }];
    card
}

/// The Maker's Eye — "Run R&D. Access 2 additional cards during this run."
/// `Effect::InitiateRun` starts the run as part of playing the event (no
/// separate click); `Effect::AddAdditionalAccess` then applies to it —
/// evaluated in order, both from this one `OnPlay` trigger.
fn the_makers_eye() -> Card {
    let mut card = base_card("the_makers_eye", "The Maker's Eye", Side::Runner, CardType::Event, 2);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnPlay,
        effects: vec![
            Effect::InitiateRun(ServerId::RnD),
            Effect::AddAdditionalAccess { server: ServerId::RnD, count: 2 },
        ],
        requirement: None,
    }];
    card
}

/// Account Siphon — "Run HQ. If successful, instead of accessing cards,
/// the Corp loses 5 credits, you gain 10 credits, and you take 2 tags."
/// `Effect::SetAccessReplacement` takes a single `Effect`, so the three
/// replacement effects are bundled via `Effect::Sequence`.
fn account_siphon() -> Card {
    let mut card = base_card("account_siphon", "Account Siphon", Side::Runner, CardType::Event, 2);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnPlay,
        effects: vec![
            Effect::InitiateRun(ServerId::Hq),
            Effect::SetAccessReplacement {
                server: ServerId::Hq,
                effect: Box::new(Effect::Sequence(vec![
                    Effect::LoseCredits(Side::Corp, 5),
                    Effect::GainCredits(Side::Runner, 10),
                    Effect::GiveTags(2),
                ])),
            },
        ],
        requirement: None,
    }];
    card
}

fn pump_and_break(id: &str, title: &str, restrict_to: IceType) -> Card {
    let mut card = base_card(id, title, Side::Runner, CardType::Program, 2);
    card.strength = Some(2);
    card.abilities = vec![
        AbilityDef {
            trigger: Trigger::Paid,
            cost: Some(Cost::Credits(1)),
            requirement: None,
            effect: Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
        },
        AbilityDef {
            trigger: Trigger::Paid,
            cost: Some(Cost::Credits(1)),
            requirement: None,
            effect: Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: Some(restrict_to) },
        },
    ];
    card
}

/// Corroder — Fracter: pump 1 for 1c, break 1 Barrier subroutine for 1c.
/// Same stat line as the existing `data/runner/corroder.json` DSL
/// round-trip fixture.
fn corroder() -> Card {
    pump_and_break("corroder", "Corroder", IceType::Barrier)
}

/// Gordian Blade — Decoder: pump 1 for 1c, break 1 Code Gate subroutine for
/// 1c.
fn gordian_blade() -> Card {
    pump_and_break("gordian_blade", "Gordian Blade", IceType::CodeGate)
}

pub fn register_runner_cards(registry: &mut CardRegistry) {
    for card in [sure_gamble(), diesel(), the_makers_eye(), account_siphon(), corroder(), gordian_blade()] {
        registry.insert(card);
    }
}
