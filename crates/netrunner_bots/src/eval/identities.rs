//! The two identities, read at the moments a term prices ahead of time
//! (Phase 5 §34). An identity is never installed, trashed or played, so
//! what its text pays *now* lands in the state the planner's line
//! produces — Topan's install, Weyland Consortium: Built to Last's 2[credit]
//! on an advance, Synapse Global's click — once the sample carries it
//! (§33). What it pays at a moment the line does not reach is what this
//! module reads: a run is priced as a leaf at its initiation, so what the
//! identities print about the run's success, its breach, its accesses and
//! its end had no reader, on either chair, and every identity read as a
//! blank card at the one moment most of them are about.
//!
//! **The reading is the engine's, at a moment that has not happened.** A
//! trigger is read off the identity's DSL — never its name — as the
//! listener scan would hear it: its `when` against the server, "the first
//! time each turn" against the turn's log, its intervening "if" through
//! the engine's own `check_requirement`. The one difference is the moment
//! itself: the leaf stands at the run's initiation, so a condition about
//! the run that has not finished is answered for the run being priced
//! (`At`) — "the last run was on HQ or R&D" is this run's server, "a card
//! was accessed" is the breach's count, "breaching HQ" is the server the
//! run will breach, and "accessing an installed card" is the card the
//! access would find. Anything else is the table as it stands, which is
//! what the engine will read too (Dewi Subrotoputri's full memory, a "once
//! per turn" spent).
//!
//! **What is counted is what a click is weighed against**: credits and
//! cards to either side, accesses added to the breach, damage and tags to
//! the Runner. A choice is its chooser's best option; a selection's
//! `then` (Pravdivost Consulting's counter, Ryō "Phoenix" Ōno's discard)
//! and a flip are not read — the cheaper direction.

use super::*;
use netrunner_core::dsl::{EffectRequirement, EventFilter, Subject, TriggeredEffect};
use netrunner_core::rules::turn_log::{Class, ServerClass};
use netrunner_core::rules::{check_requirement, ResolutionContext, ServerId};

/// What the identities' text pays at a moment, to each side.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Pays {
    pub runner_credits: i32,
    pub runner_cards: i32,
    pub corp_credits: i32,
    pub corp_cards: i32,
    /// Cards the breach accesses beyond the ones the run already makes
    /// (Mercury Chrome's "access 1 additional card").
    pub accesses: u32,
    /// Damage and tags the Runner takes (BANGUN's "do 2 meat damage and
    /// give the Runner 1 tag").
    pub damage: u32,
    pub tags: u32,
}

impl Pays {
    fn add(self, other: Pays) -> Pays {
        Pays {
            runner_credits: self.runner_credits + other.runner_credits,
            runner_cards: self.runner_cards + other.runner_cards,
            corp_credits: self.corp_credits + other.corp_credits,
            corp_cards: self.corp_cards + other.corp_cards,
            accesses: self.accesses + other.accesses,
            damage: self.damage + other.damage,
            tags: self.tags + other.tags,
        }
    }

    /// How a chooser compares two options of a choice: a credit, a card,
    /// an access, a point of damage and a tag about a click's worth each,
    /// for the side they help, as `Tally::worth` compares a play's.
    fn worth(self, side: Side) -> i32 {
        let runner = self.runner_credits + self.runner_cards + self.accesses as i32 - self.damage as i32 - self.tags as i32;
        let corp = self.corp_credits + self.corp_cards;
        match side {
            Side::Runner => runner - corp,
            Side::Corp => corp - runner,
        }
    }
}

/// The moment a reading is asked about: the server the run breaches, the
/// cards its breach accesses, and the card being accessed if the moment
/// is one access.
struct At {
    server: ServerId,
    accesses: u32,
    accessing: Option<Accessing>,
}

#[derive(Clone, Copy)]
struct Accessing {
    agenda: bool,
    installed: bool,
    rezzed: bool,
}

/// What both identities print about `run` succeeding, as the leaf prices
/// it: the breach first (what it adds to the accesses), then the success
/// and the run's end, read with the breach's whole count — Zahya
/// Sadeghi's "gain 1[credit] for each time you accessed a card during
/// that run" is paid on accesses Mercury Chrome or Docklands Pass added.
/// The breach is of the server the run will approach (`redirect_on_
/// approach`), as `access_prospect` reads it.
pub(super) fn run_success(state: &GameState, registry: &CardRegistry, run: &RunState) -> Pays {
    let server = run.redirect_on_approach.unwrap_or(run.server);
    let promised = rider_accesses(run, server) + rig_breach_accesses(state, registry, server);
    let breach = heard(state, registry, &[Trigger::OnBreach], &At { server, accesses: breach_accesses(state, run, server, promised), accessing: None });
    let accesses = breach_accesses(state, run, server, promised + breach.accesses);
    let after = heard(state, registry, &[Trigger::OnSuccessfulRun, Trigger::OnRunEnded], &At { server, accesses, accessing: None });
    // The breach's own added accesses are the reading's; what its triggers
    // pay besides is counted with the rest.
    breach.add(after)
}

/// What both identities print about the Runner accessing `installed` in
/// `server`'s root: BANGUN's "whenever the Runner accesses a faceup
/// installed agenda, do 2 meat damage and give the Runner 1 tag".
pub(super) fn on_access(state: &GameState, registry: &CardRegistry, server: ServerId, def: &CardDefinition, installed: &InstalledCard) -> Pays {
    let accessing = Accessing { agenda: def.card_type == CardType::Agenda, installed: true, rezzed: installed.rezzed };
    heard(state, registry, &[Trigger::OnAccessed], &At { server, accesses: 1, accessing: Some(accessing) })
}

/// What both identities print about the Runner trashing a card it is
/// accessing in `server`: René "Loup" Arcemont's "the first time each turn
/// you trash a card you are accessing, gain 1[credit] and draw 1 card".
pub(super) fn on_trash_while_accessing(state: &GameState, registry: &CardRegistry, server: ServerId) -> Pays {
    heard(state, registry, &[Trigger::OnTrashedFromAccess], &At { server, accesses: 1, accessing: None })
}

/// The cards a breach of `server` accesses: one from HQ or R&D and every
/// card the run and its cards add (`promised`), never more than the
/// server holds; every card in Archives; every card in a remote's root.
fn breach_accesses(state: &GameState, run: &RunState, server: ServerId, promised: u32) -> u32 {
    use netrunner_core::rules::InstallSlot;
    let held = |count: usize| count as u32;
    match server {
        ServerId::Hq => (1 + run.additional_hq_access + promised).min(held(state.corp.hq.len())),
        ServerId::RnD => (1 + run.additional_rd_access + promised).min(held(state.corp.r_and_d.len())),
        ServerId::Archives => held(state.corp.archives.len()),
        ServerId::Remote(_) => held(state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Root).count()),
    }
}

/// Every trigger of either identity on one of `triggers` that would be
/// heard at `at`, summed.
fn heard(state: &GameState, registry: &CardRegistry, triggers: &[Trigger], at: &At) -> Pays {
    let mut pays = Pays::default();
    for id in [state.corp.identity.as_ref(), state.runner.identity.as_ref()].into_iter().flatten() {
        let Some(def) = registry.get(id) else { continue };
        let ctx = ResolutionContext::for_card(Some(id));
        for trigger in def.triggers.iter().filter(|trigger| triggers.contains(&trigger.trigger) && trigger.subject != Some(Subject::This)) {
            if !admits(trigger, at.server) || (trigger.first_each_turn && spent(state, trigger)) {
                continue;
            }
            if let Some(requirement) = &trigger.requirement
                && !met(state, registry, def.side, &ctx, requirement, at)
            {
                continue;
            }
            for effect in &trigger.effects {
                pays = pays.add(paid(state, registry, def.side, &ctx, effect, at));
            }
        }
    }
    pays
}

/// Whether a trigger's `when` admits the moment's server. A condition on
/// anything else is not one this reading can answer ahead of the moment,
/// and is taken as not met.
fn admits(trigger: &TriggeredEffect, server: ServerId) -> bool {
    match &trigger.when {
        None => true,
        Some(EventFilter::Server(servers)) => servers.contains(&server),
        Some(_) => false,
    }
}

/// Whether the turn has already held what a "first time each turn"
/// entry listens for: its moment about any of the servers its `when`
/// names, or about anything when it names none — the count the engine
/// judges the entry by (`rig_breach_accesses` reads Docklands Pass's the
/// same way).
fn spent(state: &GameState, trigger: &TriggeredEffect) -> bool {
    match &trigger.when {
        Some(EventFilter::Server(servers)) => servers.iter().any(|server| {
            let class = match server {
                ServerId::Hq => ServerClass::Hq,
                ServerId::RnD => ServerClass::RnD,
                ServerId::Archives => ServerClass::Archives,
                ServerId::Remote(_) => ServerClass::Remote,
            };
            state.this_turn.times_about(trigger.trigger, Class::Server(class)) > 0
        }),
        _ => state.this_turn.times(trigger.trigger) > 0,
    }
}

/// An identity's intervening "if", answered at `at`: the conditions about
/// the run the moment ends and the card it accesses are answered for the
/// moment being priced, and every other is the engine's, on the table as
/// it stands.
fn met(state: &GameState, registry: &CardRegistry, side: Side, ctx: &ResolutionContext<'_>, requirement: &EffectRequirement, at: &At) -> bool {
    match requirement {
        EffectRequirement::And(a, b) => met(state, registry, side, ctx, a, at) && met(state, registry, side, ctx, b, at),
        EffectRequirement::Not(a) => !met(state, registry, side, ctx, a, at),
        EffectRequirement::LastRunWasOnHqOrRnD => matches!(at.server, ServerId::Hq | ServerId::RnD),
        EffectRequirement::AccessedAnyCardDuringLastRun => at.accesses > 0,
        EffectRequirement::Breaching(server) => *server == at.server,
        EffectRequirement::CurrentlyAccessingNonAgenda => at.accessing.is_some_and(|card| !card.agenda),
        EffectRequirement::CurrentlyAccessingInstalledCard { rezzed_only } => {
            at.accessing.is_some_and(|card| card.installed && (card.rezzed || !rezzed_only))
        }
        other => check_requirement(state, other, side, ctx, registry).is_ok(),
    }
}

/// What one effect of an identity's trigger pays at `at`.
fn paid(state: &GameState, registry: &CardRegistry, side: Side, ctx: &ResolutionContext<'_>, effect: &Effect, at: &At) -> Pays {
    let credits = |to: Side, n: i32| match to {
        Side::Runner => Pays { runner_credits: n, ..Pays::default() },
        Side::Corp => Pays { corp_credits: n, ..Pays::default() },
    };
    match effect {
        Effect::GainCredits(to, n) => credits(*to, *n as i32),
        Effect::LoseCredits(to, n) => credits(*to, -(*n as i32)),
        Effect::GainCreditsAmount(to, Amount::CardsAccessedLastRun) => credits(*to, at.accesses as i32),
        Effect::DrawCards(Side::Runner, n) => Pays { runner_cards: *n as i32, ..Pays::default() },
        Effect::DrawCards(Side::Corp, n) => Pays { corp_cards: *n as i32, ..Pays::default() },
        Effect::AddAdditionalAccess { server, count } if *server == at.server => Pays { accesses: *count, ..Pays::default() },
        Effect::DealDamage(_, n) => Pays { damage: *n as u32, ..Pays::default() },
        Effect::GiveTags(Amount::Fixed(n)) => Pays { tags: *n, ..Pays::default() },
        Effect::Sequence(effects) => effects.iter().fold(Pays::default(), |sum, effect| sum.add(paid(state, registry, side, ctx, effect, at))),
        Effect::EffectIf { condition, effect } if met(state, registry, side, ctx, condition, at) => paid(state, registry, side, ctx, effect, at),
        Effect::PresentChoice { chooser, options, .. } => options
            .iter()
            .map(|option| paid(state, registry, side, ctx, option, at))
            .max_by_key(|pays| pays.worth(*chooser))
            .unwrap_or_default(),
        _ => Pays::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::runner::access_prospect;
    use crate::eval::test_support::*;
    use netrunner_core::dsl::CardId;
    use netrunner_core::rules::{GameEvent, InstallId, InstallSlot};

    fn id(card: &str) -> Option<CardId> {
        Some(CardId(card.to_string()))
    }

    fn run_on(server: ServerId) -> RunState {
        RunState { server, ..Default::default() }
    }

    /// A table with a hand, a deck and a blank identity on each side.
    fn table() -> GameState {
        let mut state = GameState::new(0);
        state.corp.hq = corp_cards("hq", 4);
        state.corp.r_and_d = corp_cards("rd", 10);
        state.runner.grip = corp_cards("grip", 5);
        state
    }

    #[test]
    fn gabriel_santiagos_first_hq_run_of_the_turn_pays_two_and_no_other_run_does() {
        let pool = pool();
        let mut state = table();
        state.runner.identity = id("gabriel_santiago_consummate_professional");
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::Hq)).runner_credits, 2);
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::RnD)).runner_credits, 0, "the text names HQ");
        // "The first time each turn": a successful HQ run already this turn
        // spends it, as the engine's listener scan judges it.
        netrunner_core::rules::dispatch_event(&mut state, &pool, &GameEvent::RunSucceeded { server: ServerId::Hq }).expect("a run succeeds");
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::Hq)).runner_credits, 0, "the turn's first HQ run has been made");
    }

    #[test]
    fn zahya_sadeghi_is_paid_a_credit_for_each_card_the_breach_accesses_on_hq_or_rnd() {
        let pool = pool();
        let mut state = table();
        state.runner.identity = id("zahya_sadeghi_versatile_smuggler");
        let mut hq = run_on(ServerId::Hq);
        assert_eq!(run_success(&state, &pool, &hq).runner_credits, 1);
        hq.additional_hq_access = 1;
        assert_eq!(run_success(&state, &pool, &hq).runner_credits, 2, "\"for each time you accessed a card during that run\"");
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::RnD)).runner_credits, 1);
        // "When a run on HQ or R&D ends": the conditions about the run the
        // moment ends are answered for the run being priced.
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::Archives)).runner_credits, 0);
        state.corp.hq.clear();
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::Hq)).runner_credits, 0, "nothing to access, nothing accessed");
    }

    #[test]
    fn bangun_punishes_the_access_of_a_faceup_agenda_and_nothing_else() {
        let pool = pool();
        let mut state = table();
        state.corp.identity = id("bangun_when_disaster_strikes");
        let agenda = printed(&pool, "offworld_office");
        let faceup = InstalledCard { card: agenda.id.clone(), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, rezzed: true, ..Default::default() };
        let punished = on_access(&state, &pool, ServerId::Remote(0), &agenda, &faceup);
        assert_eq!((punished.damage, punished.tags), (2, 1), "\"do 2 meat damage and give the Runner 1 tag\"");
        let facedown = InstalledCard { rezzed: false, ..faceup.clone() };
        assert_eq!(on_access(&state, &pool, ServerId::Remote(0), &agenda, &facedown), Pays::default(), "a faceup agenda only");
        let asset = printed(&pool, "pad_campaign");
        let rezzed_asset = InstalledCard { card: asset.id.clone(), ..faceup.clone() };
        assert_eq!(on_access(&state, &pool, ServerId::Remote(0), &asset, &rezzed_asset), Pays::default(), "an agenda only");
    }

    /// The run on a faceup agenda is the steal it cannot miss, less what
    /// the Corp's identity does to the access — and with too few cards in
    /// the grip, the flatline it is.
    #[test]
    fn the_runner_prices_a_faceup_agenda_under_bangun_as_a_steal_and_a_punishment() {
        let pool = pool();
        let w = crate::plans::Style::BALANCED.planned_weights(Side::Runner);
        let mut state = table();
        state.runner.resources.credits = netrunner_core::rules::Credits(5);
        let agenda = printed(&pool, "offworld_office");
        state.corp.installed = vec![InstalledCard { card: agenda.id.clone(), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, rezzed: true, ..Default::default() }];
        let prospect = |state: &GameState| access_prospect(state, &run_on(ServerId::Remote(0)), &pool, &w, 5, 9);
        let blank = prospect(&state);
        assert!(blank >= f64::from(agenda.agenda_points.unwrap()) * w.agenda_point_weight, "a faceup agenda is a steal: {blank}");
        state.corp.identity = id("bangun_when_disaster_strikes");
        let punished = prospect(&state);
        assert!(punished < blank, "2 meat damage and a tag cost the steal something: {punished} against {blank}");
        assert!(punished > 0.0, "and the points are still worth taking: {punished}");
        state.runner.grip.truncate(1);
        assert!(prospect(&state) < 0.0, "two damage into a one-card grip is a flatline: {}", prospect(&state));
    }

    #[test]
    fn rene_arcemonts_first_trash_of_the_turn_pays_a_credit_and_a_card() {
        let pool = pool();
        let mut state = table();
        state.runner.identity = id("rene_loup_arcemont_party_animal");
        let pays = on_trash_while_accessing(&state, &pool, ServerId::Remote(0));
        assert_eq!((pays.runner_credits, pays.runner_cards), (1, 1));
        // And the run that reaches a trashable asset is worth more for it.
        let w = crate::plans::Style::BALANCED.planned_weights(Side::Runner);
        let asset = printed(&pool, "pad_campaign");
        state.corp.installed = vec![InstalledCard { card: asset.id.clone(), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, rezzed: true, ..Default::default() }];
        let prospect = |state: &GameState| access_prospect(state, &run_on(ServerId::Remote(0)), &pool, &w, 10, 9);
        let rene = prospect(&state);
        state.runner.identity = None;
        assert!(rene > prospect(&state), "{rene} against {}", prospect(&state));
    }

    /// The Corp reads the same run: what Gabriel Santiago's success would
    /// take is the Runner's, so ending that run is worth it to the Corp.
    #[test]
    fn the_corp_reads_what_the_runners_identity_takes_off_a_run() {
        let pool = pool();
        let w = every_corp();
        let mut state = table();
        state.active_run = Some(run_on(ServerId::Hq));
        let blank = evaluate_state_with(&state, Side::Corp, &pool, &w);
        state.runner.identity = id("gabriel_santiago_consummate_professional");
        let gabriel = evaluate_state_with(&state, Side::Corp, &pool, &w);
        assert!((blank - gabriel - 2.0 * w.opponent_credit_weight).abs() < 1e-9, "{blank} against {gabriel}");
    }
}
