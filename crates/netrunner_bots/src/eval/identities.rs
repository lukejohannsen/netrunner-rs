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
//!
//! **A hosted counter is worth what spending it buys** (Phase 5 §37).
//! AU Co.'s "remove 2 hosted power counters to look at the top 3 cards of
//! R&D", Epiphany Analytica's "[click], hosted power counter: … you may
//! install 1 of those cards" and the scored agendas whose counters search
//! or install (Off the Books, Project Ingatan) or advance (Sericulture
//! Expansion) are read off the use that spends them — the same reading a
//! trigger's pays are, at the evaluator's own rates — and a held counter is
//! worth half of it (`COUNTER_USE_SHARE`). Issuaq Adaptics' counters are
//! not spent: they are points, which the evaluator reads through the
//! engine's `continuous::points_to_win`, and what a score places on the
//! identity is read here (`counters_on_score`). Haas-Bioroid: Precision
//! Design's "+1 maximum hand size" needed no reader: the discard it spares
//! is the engine's, asked of `continuous::hand_size` at the turn's end, a
//! step of the planned line since §35. Kate "Mac" McCaffrey's discount is
//! the engine's price of a held card (`runner::held_price`).

use super::*;
use netrunner_core::dsl::{CardId, ContinuousKind, Cost, DamageType, EffectRequirement, EventFilter, Subject, TriggeredEffect};
use netrunner_core::rules::turn_log::{Class, ServerClass};
use netrunner_core::rules::{check_requirement, eligible_positions, InstallId, ResolutionContext, ServerId};

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
    /// Advancement counters the Corp's text places on a card of its choice
    /// (Sericulture Expansion's "place 2 advancement counters on 1
    /// installed card").
    pub corp_advancements: u32,
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
            corp_advancements: self.corp_advancements + other.corp_advancements,
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
        let corp = f64::from(self.corp_credits + self.corp_cards + self.corp_installs) + f64::from(self.corp_advancements);
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

/// What the Corp's lockdowns in play take from the Runner's next run
/// (Phase 5 §58): what they print about a run beginning and succeeding,
/// read for a run on HQ and for one on R&D — the servers a Runner's run
/// is most often on — and the cheaper of the two for the Corp, since the
/// run is the Runner's to aim. A run a turn, the rate every reading of a
/// run's trigger is taken at (`about_every_run`). SYNC Rerouting's "give
/// the Runner 1 tag unless they pay 4[credit]" is the Runner's cheaper
/// side, as any "unless" is (`paid`).
///
/// **Why.** A lockdown pays on the Runner's turn, past the end of the
/// turn the Corp's line plans, so it was priced as a click spent for
/// nothing and the planner never played SYNC Rerouting or Argus
/// Crackdown on a pass of the full pool, where random seats did.
pub(super) fn lockdowns_on_the_next_run(state: &GameState, registry: &CardRegistry) -> Pays {
    if state.corp.play_area.is_empty() {
        return Pays::default();
    }
    let credits = state.runner.resources.credits.0;
    [ServerId::Hq, ServerId::RnD]
        .into_iter()
        .map(|server| {
            let at = At { server, accesses: 1, accessing: None, runner_credits: credits };
            heard_from(state, registry, lockdowns(state), &[Trigger::OnRunStart, Trigger::OnSuccessfulRun], &at)
        })
        .min_by(|a, b| a.worth(Side::Corp).total_cmp(&b.worth(Side::Corp)))
        .unwrap_or_default()
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

/// Every trigger of either identity, and of a lockdown in the Corp's play
/// area, on one of `triggers` that would be heard at `at`, summed.
///
/// **A lockdown is read as an identity is** (Phase 5 §58): it is active
/// where it lies (CR 3.5.1c) and is never installed, rezzed or trashed in
/// the Runner's turn, so what it prints about the Runner's runs —
/// Argus Crackdown's "do 2 meat damage" on a successful run on a server
/// protected by ice — happens at a moment the line does not reach, which
/// is exactly what this module reads. Its "the chosen server" is the
/// choice the copy made (`lingering::chosen_server`), as the listener scan
/// asks it.
fn heard(state: &GameState, registry: &CardRegistry, triggers: &[Trigger], at: &At) -> Pays {
    let identities = [state.corp.identity.as_ref(), state.runner.identity.as_ref()].into_iter().flatten().map(|id| (id, None));
    heard_from(state, registry, identities.chain(lockdowns(state)), triggers, at)
}

/// The Corp's lockdowns in play, each with its copy's handle.
fn lockdowns(state: &GameState) -> impl Iterator<Item = (&CardId, Option<InstallId>)> {
    state.corp.play_area.iter().map(|played| (&played.card, Some(played.handle)))
}

/// `heard` over the cards `sources` names.
fn heard_from<'a>(state: &GameState, registry: &CardRegistry, sources: impl Iterator<Item = (&'a CardId, Option<InstallId>)>, triggers: &[Trigger], at: &At) -> Pays {
    let mut pays = Pays::default();
    for (id, handle) in sources {
        let Some(def) = registry.get(id) else { continue };
        let ctx = ResolutionContext::for_card(Some(id));
        for trigger in def.triggers.iter().filter(|trigger| triggers.contains(&trigger.trigger) && trigger.subject != Some(Subject::This)) {
            if !admits(state, trigger, at.server, id, handle) || (trigger.first_each_turn && spent(state, trigger)) {
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

/// Whether a trigger's `when` admits the moment's server: the servers it
/// names, a server protected by ice and the server the listening copy
/// chose, as the listener scan judges them (`listeners`). A condition on
/// anything else is not one this reading can answer ahead of the moment,
/// and is taken as not met.
fn admits(state: &GameState, trigger: &TriggeredEffect, server: ServerId, card: &CardId, handle: Option<InstallId>) -> bool {
    use netrunner_core::rules::InstallSlot;
    match &trigger.when {
        None => true,
        Some(EventFilter::Server(servers)) => servers.contains(&server),
        Some(EventFilter::ProtectedByIce) => state.corp.installed.iter().any(|card| card.server == server && card.slot == InstallSlot::Ice),
        Some(EventFilter::ChosenServer) => netrunner_core::rules::lingering::chosen_server(state, card, handle) == Some(server),
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
        // A selection of the Corp's, when its zone holds a card it would
        // offer: "you may install 1 card from HQ" is an install, "place 2
        // advancement counters on 1 installed card" those counters, and
        // cards it takes into HQ from R&D or Archives are cards, with
        // whatever its `then` does after (AU Co.'s "trash 1 of those cards
        // and add the rest to HQ"). Which cards, and where an install goes,
        // is the Corp's to choose when the moment comes.
        Effect::PromptChooseCards { side: Side::Corp, source, filter, max, destination, then, .. } => {
            let offered = eligible_positions(state, registry, Side::Corp, source, filter, None, None).len() as u32;
            let chosen = (*max).min(offered);
            if chosen == 0 {
                return Pays::default();
            }
            let taken = matches!(source, CardZoneRef::OwnRAndD | CardZoneRef::OwnArchives) && *destination == Some(CardZoneRef::OwnHq);
            let cards = Pays { corp_cards: if taken { chosen as i32 } else { 0 }, ..Pays::default() };
            match then.as_deref() {
                Some(then) if installs_a_corp_card(then) => Pays { corp_installs: 1, ..Pays::default() },
                Some(Effect::PlaceAdvancementCounters(Amount::Fixed(n))) => Pays { corp_advancements: *n, ..Pays::default() },
                Some(then) => cards.add(paid(state, registry, side, ctx, then, at)),
                None => cards,
            }
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

/// The share of what spending a counter buys that a held counter is worth.
/// A half, because a counter held is a use deferred at least to the next
/// moment that offers it, and because the use itself scores whole where it
/// is made: a counter worth all of its use was a tie between spending and
/// holding, and a counter priced above its use was never spent — Off the
/// Books' at `AGENDA_COUNTER_WEIGHT`, kept rather than spent on the search
/// it buys once the planner judged its offer (§35).
const COUNTER_USE_SHARE: f64 = 0.5;

/// What one counter hosted on the Corp card `def` is worth to the Corp
/// held, at `w`'s rates: `COUNTER_USE_SHARE` of the best use that spends
/// counters — a `Paid` ability or an offer its triggers make the Corp,
/// costing `RemoveCounters(n)` — read as a trigger's pays are, less the
/// clicks and credits the use costs besides, over the `n` it spends. A
/// card in HQ is worth what the evaluator prices it at (`zone_size_value`,
/// the shortfall below the floor), an install `unrezzed_install_weight`,
/// an advancement counter `advancement_weight`, a credit `own_credit_
/// weight`. `None` when no use is one this reading prices — a card whose
/// counters nothing spends (Issuaq Adaptics', NBN: Making News'
/// recurring credits) or spends on what no pays say (Proprionegation's
/// "the Runner moves to the outermost position of Archives"), which the
/// caller prices as it did before.
pub(super) fn counter_worth(state: &GameState, registry: &CardRegistry, def: &CardDefinition, w: &Weights) -> Option<f64> {
    if def.side != Side::Corp {
        return None;
    }
    let ctx = ResolutionContext::for_card(Some(&def.id));
    let at = At { server: ServerId::Hq, accesses: 0, accessing: None, runner_credits: state.runner.resources.credits.0 };
    let mut uses: Vec<(&Cost, &Effect)> =
        def.abilities.iter().filter(|ability| ability.trigger == Trigger::Paid).filter_map(|ability| Some((ability.cost.as_ref()?, &ability.effect))).collect();
    // An offer is one of a trigger's effects in every card that makes one.
    for effect in def.triggers.iter().flat_map(|trigger| &trigger.effects) {
        if let Effect::OfferPaidChoice { side: Side::Corp, cost, if_paid, .. } = effect {
            uses.push((cost, if_paid));
        }
    }
    let mut best: Option<f64> = None;
    for (cost, effect) in uses {
        let Some((counters, clicks, credits)) = spends_counters(cost) else { continue };
        if !priced(effect) {
            continue;
        }
        let pays = paid(state, registry, Side::Corp, &ctx, effect, &at);
        let bought = f64::from(pays.corp_credits) * w.own_credit_weight
            + f64::from(pays.corp_installs) * w.unrezzed_install_weight
            + f64::from(pays.corp_advancements) * w.advancement_weight
            + super::fundamentals::zone_size_value(state, Side::Corp, w, &CardZoneRef::OwnHq, i64::from(pays.corp_cards), 0)
            - f64::from(clicks) * w.click_weight
            - f64::from(credits) * w.own_credit_weight;
        let each = bought / f64::from(counters);
        best = Some(best.map_or(each, |best: f64| best.max(each)));
    }
    best.map(|each| each.max(0.0) * COUNTER_USE_SHARE)
}

/// `(counters, clicks, credits)` a cost spends, when it removes hosted
/// counters: `RemoveCounters(n)`, alone or beside clicks and credits.
fn spends_counters(cost: &Cost) -> Option<(u32, u32, u32)> {
    fn parts(cost: &Cost) -> Option<(u32, u32, u32)> {
        match cost {
            Cost::RemoveCounters(n) => Some((*n, 0, 0)),
            Cost::Clicks(n) => Some((0, *n, 0)),
            Cost::Credits(n) => Some((0, 0, *n)),
            Cost::AllOf(all) => all.iter().try_fold((0, 0, 0), |(a, b, c), part| parts(part).map(|(x, y, z)| (a + x, b + y, c + z))),
            _ => None,
        }
    }
    parts(cost).filter(|(counters, _, _)| *counters > 0)
}

/// Whether a use's effect says something `paid` prices for the Corp:
/// credits or cards for it, an install, advancement counters, or cards
/// taken into HQ. An effect that says none of them is not read as
/// worthless — it is not read.
fn priced(effect: &Effect) -> bool {
    let mut found = false;
    effect.for_each_effect(&mut |effect| {
        found |= matches!(
            effect,
            Effect::GainCredits(Side::Corp, _)
                | Effect::DrawCards(Side::Corp, _)
                | Effect::PromptInstallCorpCard { .. }
                | Effect::PlaceAdvancementCounters(_)
                | Effect::PromptChooseCards { destination: Some(CardZoneRef::OwnHq), .. }
        );
    });
    found
}

/// The counters the Corp's identity places on itself when `installed`, a
/// `def` agenda, is scored — now (`later` false), or on a later turn it is
/// neither installed nor advanced in (`later` true). Issuaq Adaptics'
/// "whenever you score an agenda that you did not install or advance this
/// turn, place 1 power counter on this identity", read off its `when`: the
/// words about what happened to the agenda this turn are answered off the
/// install (`installed_this_turn`, its own count of advances), and the
/// words about the card off the card. A trigger with an intervening "if"
/// or a "first time each turn" is not read.
pub(super) fn counters_on_score(state: &GameState, registry: &CardRegistry, installed: &InstalledCard, def: &CardDefinition, later: bool) -> u32 {
    let Some(identity) = state.corp.identity.as_ref().and_then(|id| registry.get(id)) else { return 0 };
    identity
        .triggers
        .iter()
        .filter(|trigger| trigger.trigger == Trigger::OnAgendaScored && trigger.subject != Some(Subject::This))
        .filter(|trigger| trigger.requirement.is_none() && !trigger.first_each_turn)
        .filter(|trigger| match &trigger.when {
            None => true,
            Some(EventFilter::Card(filter)) => scored_card_matches(state, filter, def, installed, later),
            Some(_) => false,
        })
        .flat_map(|trigger| &trigger.effects)
        .map(|effect| if let Effect::AddCounters(n) = effect { *n } else { 0 })
        .sum()
}

fn scored_card_matches(state: &GameState, filter: &CardFilter, def: &CardDefinition, installed: &InstalledCard, later: bool) -> bool {
    match filter {
        CardFilter::All(filters) => filters.iter().all(|filter| scored_card_matches(state, filter, def, installed, later)),
        CardFilter::AnyOf(filters) => filters.iter().any(|filter| scored_card_matches(state, filter, def, installed, later)),
        CardFilter::NotInstalledThisTurn => later || !installed.installed_this_turn,
        CardFilter::InstalledThisTurn => !later && installed.installed_this_turn,
        CardFilter::NotAdvancedThisTurn => later || installed.this_turn.count(state.turn, Trigger::OnAdvance) == 0,
        other => card_matches_filter(def, other),
    }
}

/// The agenda points one counter on the Corp's identity spares it — Issuaq
/// Adaptics' "for each hosted power counter, you need 1 less agenda point
/// to win the game", read off the identity's `AgendaPointsToWin` of its
/// hosted counters.
pub(super) fn points_per_identity_counter(state: &GameState, registry: &CardRegistry) -> i32 {
    let Some(identity) = state.corp.identity.as_ref().and_then(|id| registry.get(id)) else { return 0 };
    identity
        .continuous
        .iter()
        .filter_map(|effect| match &effect.kind {
            ContinuousKind::AgendaPointsToWin(number) if number.of == Amount::HostedCounters => Some(-number.per),
            _ => None,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::runner::access_prospect;
    use crate::eval::test_support::*;
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

    /// A lockdown in the play area is read as an identity is (§58): Argus
    /// Crackdown's damage on a successful run on a server protected by ice
    /// and on no other, and SYNC Rerouting's "unless they pay 4[credit]"
    /// as the Runner's cheaper side — the credits while it has them, the
    /// tag once it does not.
    #[test]
    fn a_lockdown_in_play_is_read_on_the_runs_it_is_about() {
        use netrunner_core::rules::PlayedOperation;
        let pool = pool();
        let mut state = table();
        state.corp.play_area = vec![PlayedOperation { card: CardId("argus_crackdown".to_string()), handle: InstallId(90) }];
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::Hq)).damage, 0, "HQ has no ice");
        state.corp.installed.push(InstalledCard { card: CardId("ice_wall".to_string()), install_id: InstallId(1), server: ServerId::Hq, slot: InstallSlot::Ice, ..Default::default() });
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::Hq)).damage, 2);
        assert_eq!(run_success(&state, &pool, &run_on(ServerId::RnD)).damage, 0, "R&D has no ice");
        assert_eq!(lockdowns_on_the_next_run(&state, &pool).damage, 0, "the Runner's next run goes where it costs least");

        state.corp.play_area = vec![PlayedOperation { card: CardId("sync_rerouting".to_string()), handle: InstallId(91) }];
        state.runner.resources.credits = netrunner_core::rules::Credits(6);
        assert_eq!(lockdowns_on_the_next_run(&state, &pool), Pays { runner_credits: -4, ..Pays::default() });
        state.runner.resources.credits = netrunner_core::rules::Credits(3);
        assert_eq!(lockdowns_on_the_next_run(&state, &pool), Pays { tags: 1, ..Pays::default() });
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

    /// A hosted counter is half of what spending it buys, at the
    /// evaluator's rates (§37): Off the Books' search installed free,
    /// Sericulture Expansion's two advancement counters, Epiphany
    /// Analytica's install less the click it costs, AU Co.'s two cards
    /// where HQ is short of them — and nothing read for a counter no use
    /// prices (Embedded Reporting's operation set on R&D) or none spends
    /// (Issuaq Adaptics', which are points).
    #[test]
    fn a_hosted_counter_is_worth_half_of_what_spending_it_buys() {
        let pool = pool();
        let w = every_corp();
        let mut state = table();
        state.corp.r_and_d = ["hedge_fund", "hedge_fund", "pad_campaign", "hedge_fund", "hedge_fund", "hedge_fund", "pad_campaign"].map(|card| CardId(card.to_string())).to_vec();
        let worth = |state: &GameState, card: &str| counter_worth(state, &pool, &printed(&pool, card), &w);
        assert_eq!(worth(&state, "off_the_books"), Some(0.5 * w.unrezzed_install_weight), "\"search R&D for 1 card … install that card\"");
        assert_eq!(worth(&state, "sericulture_expansion"), Some(0.0), "nothing installed to advance");
        state.corp.installed.push(InstalledCard { card: CardId("pad_campaign".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, ..Default::default() });
        assert_eq!(worth(&state, "sericulture_expansion"), Some(0.5 * 2.0 * w.advancement_weight));
        assert_eq!(worth(&state, "epiphany_analytica_nations_undivided"), Some(0.5 * (w.unrezzed_install_weight - w.click_weight)), "an asset in the top 3, for a click");
        assert_eq!(worth(&state, "embedded_reporting"), None, "an operation set on R&D is not priced");
        assert_eq!(worth(&state, "issuaq_adaptics_sustaining_diversity"), None, "counters nothing spends");
        assert_eq!(worth(&state, "nbn_making_news"), None, "recurring credits");
        // AU Co.: the two cards it takes are worth what HQ is short of them.
        state.corp.hq = corp_cards("hq", 1);
        let short = worth(&state, "au_co_the_gold_standard_in_clones").expect("a search");
        assert!((short - 0.5 * 2.0 * w.hq_shortfall_weight / 2.0).abs() < 1e-9, "{short}");
        state.corp.hq = corp_cards("hq", 5);
        assert_eq!(worth(&state, "au_co_the_gold_standard_in_clones"), Some(0.0), "a card above the floor is worth nothing here");
    }

    /// Issuaq Adaptics places a counter for an agenda "that you did not
    /// install or advance this turn": none for one advanced now, one for the
    /// same agenda scored on a later turn untouched.
    #[test]
    fn issuaq_counts_a_score_now_and_a_score_later() {
        let pool = pool();
        let mut state = table();
        state.corp.identity = id("issuaq_adaptics_sustaining_diversity");
        let agenda = printed(&pool, "offworld_office");
        let mut installed = InstalledCard { card: agenda.id.clone(), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, ..Default::default() };
        assert_eq!((counters_on_score(&state, &pool, &installed, &agenda, false), counters_on_score(&state, &pool, &installed, &agenda, true)), (1, 1));
        installed.installed_this_turn = true;
        assert_eq!((counters_on_score(&state, &pool, &installed, &agenda, false), counters_on_score(&state, &pool, &installed, &agenda, true)), (0, 1), "installed this turn");
        installed.installed_this_turn = false;
        state.corp.installed.push(installed.clone());
        let advance = GameEvent::CardAdvanced { install: installed.install_id, card: Some(agenda.id.clone()), advancement_tokens: 1 };
        netrunner_core::rules::dispatch_event(&mut state, &pool, &advance).expect("the agenda is advanced");
        let advanced = state.corp.installed[0].clone();
        assert_eq!((counters_on_score(&state, &pool, &advanced, &agenda, false), counters_on_score(&state, &pool, &advanced, &agenda, true)), (0, 1), "advanced this turn");
        assert_eq!(points_per_identity_counter(&state, &pool), 1);
        state.corp.identity = None;
        assert_eq!(counters_on_score(&state, &pool, &advanced, &agenda, true), 0);
        assert_eq!(points_per_identity_counter(&state, &pool), 0);
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
