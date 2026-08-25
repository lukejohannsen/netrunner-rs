//! Baseline Core Set Corp cards: Operations, Agendas, Assets, and ICE.

use crate::cards::common::base_card;
use crate::cards::CardRegistry;
use crate::dsl::{
    CardType, DamageType, Effect, EffectRequirement, IceType, SubroutineDef, Trigger, TriggeredEffect,
};
use crate::rules::Side;

/// Hedge Fund — "Gain 9 credits." Same stat line as the existing
/// `data/corp/hedge_fund.json` DSL round-trip fixture.
fn hedge_fund() -> crate::dsl::Card {
    let mut card = base_card("hedge_fund", "Hedge Fund", Side::Corp, CardType::Operation, 5);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnPlay,
        effects: vec![Effect::GainCredits(Side::Corp, 9)],
        requirement: None,
    }];
    card
}

/// Scorched Earth — "Requires a tagged Runner. Deal 4 meat damage." The tag
/// requirement is a hard play-legality gate (`Card::play_requirement`),
/// checked by `engine::play_operation` before the card's cost is even paid
/// — playing it against an untagged Runner returns
/// `RulesError::RunnerNotTagged` outright, rather than paying the cost and
/// then fizzling.
fn scorched_earth() -> crate::dsl::Card {
    let mut card = base_card("scorched_earth", "Scorched Earth", Side::Corp, CardType::Operation, 3);
    card.play_requirement = Some(EffectRequirement::IsTagged);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnPlay,
        effects: vec![Effect::DealDamage(DamageType::Meat, 4)],
        requirement: None,
    }];
    card
}

/// Hostile Takeover — Agenda 2/1: "When you score this agenda, gain 7
/// credits and take 1 bad publicity."
fn hostile_takeover() -> crate::dsl::Card {
    let mut card = base_card("hostile_takeover", "Hostile Takeover", Side::Corp, CardType::Agenda, 0);
    card.advancement_requirement = Some(2);
    card.agenda_points = Some(1);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnAgendaScored,
        effects: vec![Effect::GainCredits(Side::Corp, 7), Effect::GiveBadPublicity(1)],
        requirement: None,
    }];
    card
}

/// PAD Campaign — Asset: "At the start of your turn, gain 1 credit." Only
/// fires while rezzed — `turn::enter_start_of_turn` only dispatches
/// `Trigger::OnTurnStart` for rezzed Corp installs.
fn pad_campaign() -> crate::dsl::Card {
    let mut card = base_card("pad_campaign", "PAD Campaign", Side::Corp, CardType::Asset, 2);
    card.trash_cost = Some(4);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnTurnStart,
        effects: vec![Effect::GainCredits(Side::Corp, 1)],
        requirement: None,
    }];
    card
}

/// Snare! — Asset: "When the Runner accesses this card, deal 3 net damage
/// and give the Runner 1 tag." Simplified from the real card, which lets
/// the Corp optionally pay 4 credits to trigger this instead of it firing
/// automatically — no primitive for a Corp-paid, automatically-triggered
/// ability exists yet (`InteractiveOnAccess` only models the Runner paying
/// to *avoid* an effect, the opposite direction), and this isn't part of
/// the baseline set's test checklist, so it's implemented as unconditional
/// here.
fn snare() -> crate::dsl::Card {
    let mut card = base_card("snare", "Snare!", Side::Corp, CardType::Asset, 4);
    card.trash_cost = Some(3);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnAccessed,
        effects: vec![Effect::DealDamage(DamageType::Net, 3), Effect::GiveTags(1)],
        requirement: None,
    }];
    card
}

/// Enigma — Code Gate ICE: "Subroutine: The Runner loses 1 click.
/// Subroutine: End the run."
fn enigma() -> crate::dsl::Card {
    let mut card = base_card("enigma", "Enigma", Side::Corp, CardType::Ice(IceType::CodeGate), 3);
    card.strength = Some(2);
    card.subroutines = vec![
        SubroutineDef { text: "The Runner loses 1 click.".to_string(), effect: Effect::LoseClicks(1) },
        SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun },
    ];
    card
}

/// Wall of Static — Barrier ICE: "Subroutine: End the run."
fn wall_of_static() -> crate::dsl::Card {
    let mut card = base_card("wall_of_static", "Wall of Static", Side::Corp, CardType::Ice(IceType::Barrier), 3);
    card.strength = Some(3);
    card.subroutines = vec![SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun }];
    card
}

pub fn register_corp_cards(registry: &mut CardRegistry) {
    for card in [hedge_fund(), scorched_earth(), hostile_takeover(), pad_campaign(), snare(), enigma(), wall_of_static()]
    {
        registry.insert(card);
    }
}
