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
//!
//! **A steal and a tag are moments too** (Phase 5 §36). Jinteki: Personal
//! Evolution's net damage and Thule Subsea's core damage on a steal,
//! Poétrï Luxury Brands' install from HQ when one is stolen, and NBN:
//! Reality Plus's 2[credit] for the turn's first tag happen inside a run,
//! after the leaf that prices it. A "do 1 core damage unless they spend [click] and 2[credit]"
//! is its payer's cheaper side, and the side they can afford; a selection
//! whose `then` installs a Corp card is an install when its zone holds a
//! card the selection would offer (`eligible_positions`, the engine's own
//! question). Tāo Salonga's swap is read as nothing: what two pieces of
//! ICE are worth in each other's places is a reading of where each one
//! stands against the rig, which no term makes off a run.

use super::*;
use netrunner_core::dsl::{Cost, DamageType, EffectRequirement, EventFilter, Subject, TriggeredEffect};
use netrunner_core::rules::turn_log::{Class, ServerClass};
use netrunner_core::rules::{check_requirement, eligible_positions, ResolutionContext, ServerId};

/// What the identities' text pays at a moment, to each side.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Pays {
    pub runner_credits: i32,
    pub runner_cards: i32,
    /// Clicks the Runner spends (Thule Subsea's "unless they spend [click]
    /// and 2[credit]").
    pub runner_clicks: i32,
    pub corp_credits: i32,
    pub corp_cards: i32,
    /// Cards the Corp's text installs (Poétrï's "you may install 1
    /// non-agenda card from HQ", Synapse Global's "install 1 card from HQ,
    /// ignoring all costs").
    pub corp_installs: i32,
    /// Cards the breach accesses beyond the ones the run already makes
    /// (Mercury Chrome's "access 1 additional card").
    pub accesses: u32,
    /// Damage and tags the Runner takes (BANGUN's "do 2 meat damage and
    /// give the Runner 1 tag").
    pub damage: u32,
    /// Of `damage`, the points that are core damage: each discards a card
    /// as the rest do and lowers the hand size for good besides.
    pub core_damage: u32,
    pub tags: u32,
}

impl Pays {
    fn add(self, other: Pays) -> Pays {
        Pays {
            runner_credits: self.runner_credits + other.runner_credits,
            runner_cards: self.runner_cards + other.runner_cards,
            runner_clicks: self.runner_clicks + other.runner_clicks,
            corp_credits: self.corp_credits + other.corp_credits,
            corp_cards: self.corp_cards + other.corp_cards,
            corp_installs: self.corp_installs + other.corp_installs,
            accesses: self.accesses + other.accesses,
            damage: self.damage + other.damage,
            core_damage: self.core_damage + other.core_damage,
            tags: self.tags + other.tags,
        }
    }

    /// How a chooser compares two options of a choice, in credits: a
    /// credit, a card, a click, an access, an install and a point of
    /// damage about one each, for the side they help, as `Tally::worth`
    /// compares a play's — and a tag and a point of core damage what the
    /// evaluator prices them at over a credit, so a payer who can spend a
    /// click and 2[credit] to keep their hand size does (Thule Subsea).
    fn worth(self, side: Side) -> f64 {
        let tag = TAG_WEIGHT / OWN_CREDIT_WEIGHT;
        let core = CORE_DAMAGE_WEIGHT / OWN_CREDIT_WEIGHT;
        let runner = f64::from(self.runner_credits + self.runner_cards + self.runner_clicks) + f64::from(self.accesses)
            - f64::from(self.damage)
            - f64::from(self.tags) * tag
            - f64::from(self.core_damage) * core;
        let corp = f64::from(self.corp_credits + self.corp_cards + self.corp_installs);
        match side {
            Side::Runner => runner - corp,
            Side::Corp => corp - runner,
        }
    }

    /// What these pays are worth to the Runner at `w`'s rates, its damage
    /// aside — the caller counts that toward the flatline it can be, with
    /// a trap's. A card and a click at the click's rate, the Corp's
    /// credits, cards and installs at the rate the Runner reads the
    /// Corp's credits, a tag and a point of core damage at their weights.
    pub fn to_runner(self, w: &Weights) -> f64 {
        f64::from(self.runner_credits) * w.own_credit_weight + f64::from(self.runner_cards + self.runner_clicks) * w.click_weight
            - f64::from(self.corp_credits + self.corp_cards + self.corp_installs) * w.opponent_credit_weight
            - f64::from(self.tags) * w.tag_weight
            - f64::from(self.core_damage) * w.core_damage_weight
    }
}

/// The moment a reading is asked about: the server the run breaches, the
/// cards its breach accesses, and the card being accessed if the moment
/// is one access.
struct At {
    server: ServerId,
    accesses: u32,
    accessing: Option<Accessing>,
    /// What the Runner will have to pay a cost the moment offers it — the
    /// run's credits after its breaks, at a leaf.
    runner_credits: u32,
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
    let credits = state.runner.resources.credits.0;
    let breach = heard(state, registry, &[Trigger::OnBreach], &At { server, accesses: breach_accesses(state, run, server, promised), accessing: None, runner_credits: credits });
    let accesses = breach_accesses(state, run, server, promised + breach.accesses);
    let after = heard(state, registry, &[Trigger::OnSuccessfulRun, Trigger::OnRunEnded], &At { server, accesses, accessing: None, runner_credits: credits });
    // The breach's own added accesses are the reading's; what its triggers
    // pay besides is counted with the rest.
    breach.add(after)
}

/// What both identities print about the Runner accessing `installed` in
/// `server`'s root: BANGUN's "whenever the Runner accesses a faceup
/// installed agenda, do 2 meat damage and give the Runner 1 tag".
pub(super) fn on_access(state: &GameState, registry: &CardRegistry, server: ServerId, def: &CardDefinition, installed: &InstalledCard) -> Pays {
    let accessing = Accessing { agenda: def.card_type == CardType::Agenda, installed: true, rezzed: installed.rezzed };
    heard(state, registry, &[Trigger::OnAccessed], &At { server, accesses: 1, accessing: Some(accessing), runner_credits: state.runner.resources.credits.0 })
}

/// What both identities print about the Runner trashing a card it is
/// accessing in `server`: René "Loup" Arcemont's "the first time each turn
/// you trash a card you are accessing, gain 1[credit] and draw 1 card".
pub(super) fn on_trash_while_accessing(state: &GameState, registry: &CardRegistry, server: ServerId) -> Pays {
    heard(state, registry, &[Trigger::OnTrashedFromAccess], &At { server, accesses: 1, accessing: None, runner_credits: state.runner.resources.credits.0 })
}

/// What both identities print about the Runner stealing an agenda in
/// `server` with `credits` to pay what the steal asks: Jinteki: Personal
/// Evolution's "whenever an agenda is scored or stolen, do 1 net damage",
/// Thule Subsea's "do 1 core damage unless they spend [click] and
/// 2[credit]" (the Runner's cheaper side, if it can pay at all), and
/// Poétrï's "you may install 1 non-agenda card from HQ".
pub(super) fn on_steal(state: &GameState, registry: &CardRegistry, server: ServerId, credits: u32) -> Pays {
    heard(state, registry, &[Trigger::OnAgendaStolen], &At { server, accesses: 1, accessing: None, runner_credits: credits })
}

/// What both identities print about the Runner taking tags at a moment in
/// `server`, when `tags` is any: NBN: Reality Plus's "the first time each
/// turn the Runner takes a tag, gain 2[credit] or draw 2 cards".
pub(super) fn on_tags(state: &GameState, registry: &CardRegistry, server: ServerId, tags: u32) -> Pays {
    if tags == 0 {
        return Pays::default();
    }
    heard(state, registry, &[Trigger::OnTagsGiven], &At { server, accesses: 0, accessing: None, runner_credits: state.runner.resources.credits.0 })
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
        Effect::DealDamage(DamageType::Brain, n) => Pays { damage: *n as u32, core_damage: *n as u32, ..Pays::default() },
        Effect::DealDamage(_, n) => Pays { damage: *n as u32, ..Pays::default() },
        Effect::GiveTags(Amount::Fixed(n)) => Pays { tags: *n, ..Pays::default() },
        Effect::Sequence(effects) => effects.iter().fold(Pays::default(), |sum, effect| sum.add(paid(state, registry, side, ctx, effect, at))),
        Effect::EffectIf { condition, effect } if met(state, registry, side, ctx, condition, at) => paid(state, registry, side, ctx, effect, at),
        Effect::PresentChoice { chooser, options, .. } => options
            .iter()
            .map(|option| paid(state, registry, side, ctx, option, at))
            .max_by(|a, b| a.worth(*chooser).total_cmp(&b.worth(*chooser)))
            .unwrap_or_default(),
        // "Unless they spend": the payer's cheaper side, of the ones it
        // can take — a cost it cannot pay leaves it the other.
        Effect::OfferPaidChoice { side: payer, cost, if_paid, if_declined, .. } => {
            let declined = paid(state, registry, side, ctx, if_declined, at);
            match price(state, *payer, cost, at) {
                Some(price) => {
                    let accepted = price.add(paid(state, registry, side, ctx, if_paid, at));
                    if accepted.worth(*payer) >= declined.worth(*payer) { accepted } else { declined }
                }
                None => declined,
            }
        }
        // "You may install 1 card from HQ": an install, when the zone
        // holds a card the selection would offer. Which card, and where
        // it goes, is the Corp's to choose when the moment comes.
        Effect::PromptChooseCards { side: Side::Corp, source, filter, then: Some(then), .. }
            if installs_a_corp_card(then)
                && !eligible_positions(state, registry, Side::Corp, source, filter, None, None).is_empty() =>
        {
            Pays { corp_installs: 1, ..Pays::default() }
        }
        _ => Pays::default(),
    }
}

/// What paying `cost` takes from `payer`, when it can pay it at `at`: a
/// cost of credits and clicks only. `None` for anything else, or one it
/// cannot afford.
fn price(state: &GameState, payer: Side, cost: &Cost, at: &At) -> Option<Pays> {
    fn needs(cost: &Cost) -> Option<(u32, u32)> {
        match cost {
            Cost::Credits(n) => Some((*n, 0)),
            Cost::Clicks(n) => Some((0, *n)),
            Cost::AllOf(parts) => parts.iter().try_fold((0, 0), |(c, k), part| needs(part).map(|(pc, pk)| (c + pc, k + pk))),
            _ => None,
        }
    }
    let (credits, clicks) = needs(cost)?;
    match payer {
        Side::Runner => (credits <= at.runner_credits && clicks <= state.runner.resources.clicks.0)
            .then_some(Pays { runner_credits: -(credits as i32), runner_clicks: -(clicks as i32), ..Pays::default() }),
        Side::Corp => (clicks == 0 && credits <= state.corp.resources.credits.0).then_some(Pays { corp_credits: -(credits as i32), ..Pays::default() }),
    }
}

/// Whether a selection's `then` installs the card chosen.
fn installs_a_corp_card(then: &Effect) -> bool {
    let mut found = false;
    then.for_each_effect(&mut |effect| {
        if matches!(effect, Effect::PromptInstallCorpCard { .. }) {
            found = true;
        }
    });
    found
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

    /// A steal under the Corp's identity (§36): Jinteki: Personal
    /// Evolution's net damage, and Thule Subsea's core damage — or the
    /// click and 2[credit] that keep it off, when the Runner has them.
    #[test]
    fn personal_evolution_and_thule_subsea_charge_a_steal() {
        use netrunner_core::rules::Clicks;
        let pool = pool();
        let mut state = table();
        state.corp.identity = id("jinteki_personal_evolution");
        let net = on_steal(&state, &pool, ServerId::Hq, 5);
        assert_eq!((net.damage, net.core_damage), (1, 0), "\"do 1 net damage\"");
        state.corp.identity = id("thule_subsea_safety_below");
        state.runner.resources.clicks = Clicks(1);
        let paid = on_steal(&state, &pool, ServerId::Hq, 5);
        assert_eq!((paid.runner_clicks, paid.runner_credits, paid.damage), (-1, -2, 0), "a Runner who can pay keeps its hand size");
        let short = on_steal(&state, &pool, ServerId::Hq, 1);
        assert_eq!((short.damage, short.core_damage), (1, 1), "one who cannot takes the core damage");
        state.runner.resources.clicks = Clicks(0);
        assert_eq!(on_steal(&state, &pool, ServerId::Hq, 5).core_damage, 1, "nor one with no click to spend");
        state.corp.identity = None;
        assert_eq!(on_steal(&state, &pool, ServerId::Hq, 5), Pays::default());
    }

    /// Poétrï's "you may install 1 non-agenda card from HQ" is an install
    /// when HQ holds a card the selection would offer, and nothing when it
    /// does not.
    #[test]
    fn poetri_installs_on_a_steal_when_hq_holds_a_card_to_install() {
        let pool = pool();
        let mut state = table();
        state.corp.identity = id("poetri_luxury_brands_all_the_rage");
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        assert_eq!(on_steal(&state, &pool, ServerId::Hq, 5).corp_installs, 0, "an operation is not installed");
        state.corp.hq.push(CardId("pad_campaign".to_string()));
        assert_eq!(on_steal(&state, &pool, ServerId::Hq, 5).corp_installs, 1);
    }

    /// NBN: Reality Plus's "the first time each turn the Runner takes a
    /// tag, gain 2[credit] or draw 2 cards" — on the turn's first tag only.
    #[test]
    fn reality_plus_is_paid_for_the_turns_first_tag() {
        let pool = pool();
        let mut state = table();
        state.corp.identity = id("nbn_reality_plus");
        let first = on_tags(&state, &pool, ServerId::Remote(0), 1);
        assert_eq!(first.corp_credits + first.corp_cards, 2);
        assert_eq!(on_tags(&state, &pool, ServerId::Remote(0), 0), Pays::default(), "no tag, no moment");
        netrunner_core::rules::dispatch_event(&mut state, &pool, &GameEvent::TagsGiven { side: Side::Runner, amount: 1, had: 0 }).expect("a tag is given");
        assert_eq!(on_tags(&state, &pool, ServerId::Remote(0), 1), Pays::default(), "the turn's first tag has been taken");
    }

    /// The breach prices what a steal costs under the Corp's identity: with
    /// a grip Personal Evolution's net damage would empty past, the chance
    /// of an agenda in R&D is the chance of the flatline.
    #[test]
    fn a_run_on_rnd_under_personal_evolution_with_an_empty_grip_risks_the_flatline() {
        let pool = pool();
        let w = crate::plans::Style::BALANCED.planned_weights(Side::Runner);
        let mut state = table();
        state.runner.grip.clear();
        let prospect = |state: &GameState| access_prospect(state, &run_on(ServerId::RnD), &pool, &w, 5, 9);
        let blank = prospect(&state);
        state.corp.identity = id("jinteki_personal_evolution");
        let empty = prospect(&state);
        assert!(empty < blank - w.lethal_trap_weight * 0.1, "an agenda in R&D is the flatline: {empty} against {blank}");
        state.runner.grip = corp_cards("grip", 3);
        let held = prospect(&state);
        assert!(held < blank && held > blank - 1.0, "with cards to lose, the damage at its chance: {held} against {blank}");
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
