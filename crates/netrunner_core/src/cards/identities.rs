//! Baseline Core Set identities. Every identity-specific mechanic here is
//! expressed through the general engine primitives added alongside this
//! module (`Trigger::OnInstall`/`OnSuccessfulRunOnHq`/`OnAgendaScored`/
//! `OnAgendaStolen`/`OnTransactionPlayed`/`OnVirusInstalled`,
//! `EffectRequirement::FirstInstallThisTurn`/`FirstSuccessfulHqRunThisTurn`,
//! `Card::recurring_credits`/`first_install_discount`) — nothing about an
//! identity's rules text is hardcoded into the engine itself; the engine
//! only knows the generic *categories* of reactive trigger/cost-modifier a
//! card can carry.

use crate::cards::common::base_card;
use crate::dsl::{Card, CardType, Effect, EffectRequirement, Trigger, TriggeredEffect};
use crate::rules::Side;

const MIN_DECK_SIZE: u32 = 45;

fn identity(id: &str, title: &str, side: Side) -> Card {
    let mut card = base_card(id, title, side, CardType::Identity, 0);
    card.min_deck_size = Some(MIN_DECK_SIZE);
    card
}

/// Haas-Bioroid: Engineering the Future — "The first time you install a
/// card each turn, gain 1 credit." Gated by `FirstInstallThisTurn` so a
/// second install the same turn is silently a no-op bonus (not an error —
/// see `TriggeredEffect::requirement`'s soft-gate doc comment).
fn haas_bioroid_engineering_the_future() -> Card {
    let mut card = identity("haas_bioroid_engineering_the_future", "Haas-Bioroid: Engineering the Future", Side::Corp);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnInstall,
        effects: vec![Effect::GainCredits(Side::Corp, 1)],
        requirement: Some(EffectRequirement::FirstInstallThisTurn),
    }];
    card
}

/// Jinteki: Personal Evolution — "Whenever an agenda is scored or stolen,
/// deal 1 net damage." Reacts to both `Trigger::OnAgendaScored` (dispatched
/// by `engine::score_agenda` against the Corp identity, alongside the
/// scored agenda's own triggers) and `Trigger::OnAgendaStolen` (dispatched
/// by `run::resolve_steal`).
fn jinteki_personal_evolution() -> Card {
    let mut card = identity("jinteki_personal_evolution", "Jinteki: Personal Evolution", Side::Corp);
    card.triggers = vec![
        TriggeredEffect {
            trigger: Trigger::OnAgendaScored,
            effects: vec![Effect::DealDamage(crate::dsl::DamageType::Net, 1)],
            requirement: None,
        },
        TriggeredEffect {
            trigger: Trigger::OnAgendaStolen,
            effects: vec![Effect::DealDamage(crate::dsl::DamageType::Net, 1)],
            requirement: None,
        },
    ];
    card
}

/// Weyland Consortium: Building a Better World — "Gain 1 credit whenever
/// you play a transaction." Reacts to `Trigger::OnTransactionPlayed`,
/// dispatched by `engine::play_operation` whenever the played Operation's
/// `subtypes` includes `CardSubtype::Transaction`.
fn weyland_consortium_building_a_better_world() -> Card {
    let mut card =
        identity("weyland_consortium_building_a_better_world", "Weyland Consortium: Building a Better World", Side::Corp);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnTransactionPlayed,
        effects: vec![Effect::GainCredits(Side::Corp, 1)],
        requirement: None,
    }];
    card
}

/// NBN: Making News — "You have 2 recurring credits. Use these credits
/// during a trace." Not a `Trigger`/`Effect` at all — `recurring_credits`
/// is read once at `GameState::setup` to seed `CorpState::
/// recurring_credits_max`, refilled every Corp turn by `turn::
/// enter_start_of_turn`, and drawn from by `ability::pay_cost` during an
/// active trace (`Cost::Credits`'s recurring-pool branch).
fn nbn_making_news() -> Card {
    let mut card = identity("nbn_making_news", "NBN: Making News", Side::Corp);
    card.recurring_credits = Some(2);
    card
}

/// Gabriel Santiago: Consummate Professional — "The first time you make a
/// successful run on HQ each turn, gain 2 credits." Gated by
/// `FirstSuccessfulHqRunThisTurn`; dispatched by `engine::continue_run`
/// whenever `RunSucceeded { server: ServerId::Hq }` is among the events a
/// `ContinueRun` action produced.
fn gabriel_santiago() -> Card {
    let mut card = identity("gabriel_santiago", "Gabriel Santiago: Consummate Professional", Side::Runner);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnSuccessfulRunOnHq,
        effects: vec![Effect::GainCredits(Side::Runner, 2)],
        requirement: Some(EffectRequirement::FirstSuccessfulHqRunThisTurn),
    }];
    card
}

/// Kate "Mac" McCaffrey: Digital Tinker — "The first time you install a
/// program or piece of hardware each turn, lower the install cost by 1."
/// Not a `Trigger`/`Effect` — `first_install_discount` is read directly by
/// `engine::install_hardware`/`install_program` (a cost modifier, not a
/// reactive effect — see `Card::first_install_discount`'s doc comment).
fn kate_mccaffrey() -> Card {
    let mut card = identity("kate_mccaffrey", "Kate \"Mac\" McCaffrey: Digital Tinker", Side::Runner);
    card.first_install_discount = Some(1);
    card
}

/// Noise: Hacker Extraordinaire — "Whenever you install a virus program,
/// the Corp trashes the top card of R&D." Reacts to `Trigger::
/// OnVirusInstalled`, dispatched by `engine::install_program` whenever the
/// installed Program's `subtypes` includes `CardSubtype::Virus`.
fn noise() -> Card {
    let mut card = identity("noise", "Noise: Hacker Extraordinaire", Side::Runner);
    card.triggers = vec![TriggeredEffect {
        trigger: Trigger::OnVirusInstalled,
        effects: vec![Effect::TrashCard(crate::dsl::CardTarget::TopOfStack {
            side: Side::Corp,
            zone: crate::dsl::StackZone::RAndD,
        })],
        requirement: None,
    }];
    card
}

pub fn register_identities(registry: &mut crate::cards::CardRegistry) {
    for card in [
        haas_bioroid_engineering_the_future(),
        jinteki_personal_evolution(),
        weyland_consortium_building_a_better_world(),
        nbn_making_news(),
        gabriel_santiago(),
        kate_mccaffrey(),
        noise(),
    ] {
        registry.insert(card);
    }
}
