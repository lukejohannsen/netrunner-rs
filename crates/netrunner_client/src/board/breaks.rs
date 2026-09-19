//! Getting through a piece of ICE with one press: every way the Runner's
//! cards can break *all* of the encountered ICE's subroutines, each with
//! what it costs, and the one-step-at-a-time driver that carries a chosen
//! route out (ROADMAP Phase 7 §5.2, borrowed from jinteki.net).
//!
//! **A route is found by playing it, not by pricing it.** The obvious
//! planner reads each breaker's JSON — pump cost over pump amount, break
//! cost over break count — the way `netrunner_bots::eval::break_cost`
//! prices a break for the evaluator. It would be wrong here in ways a
//! person sees on a button: the view's `current_strength` leaves out
//! Echelon's and Rising Tide's `strength_modifier` and GAMEDRAGON's bonus,
//! so it plans pumps nobody needs; it cannot know that Mayfly's break is a
//! `Sequence`, that Chromatophores gives the ICE a subtype, or that a
//! subroutine is fracter-only. So each route is searched on
//! `netrunner_bots::determinize`'s sample of the view with the engine's
//! own `apply_action`: whatever the engine would refuse, the route never
//! contains, and whatever it charges is the price shown. Nothing hidden
//! can move that price — the encounter, the rig, the credit pools and the
//! counters are all public and copied exactly — and the sample is seeded
//! from a constant, so the same view gives the same routes. The one
//! discount the sample cannot see is Sang Kancil's (`RunEventActive` reads
//! who started the run, which the view does not carry), so its price can
//! read high; the driver re-plans on every view, so it only ever pays the
//! engine's real price.
//!
//! **One route per card, not the cheapest plan.** A route uses one card's
//! abilities only, and the list offers each card that can do the whole
//! job. Which is best is not a number: Mayfly breaks for the same credits
//! as Corroder and is trashed when the run ends, and Botulus spends a virus
//! counter that could have been the next ICE's. The person chooses; the
//! list is sorted by price so the cheap one is first. Mixing two cards on
//! one ICE is left to the card menus it always was.
//!
//! **Every step is still one legal action, submitted on its own view.**
//! Each activation hands priority to the Corp, so a route cannot be sent
//! as a batch: `AutoBreak::next` is asked once per view on which the
//! Runner holds priority, re-plans from that view, and offers the next
//! step only if the engine lists it and the price has not risen. Anything
//! else — the Corp rezzed or raised the ICE, a credit went somewhere, the
//! encounter ended — stops the driver and hands the person their controls
//! back. It never passes priority at the end: the subroutines are broken,
//! and whether to use another card before moving on is the person's call.

use netrunner_bots::determinize;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, Effect, Trigger};
use netrunner_core::rules::{apply_action, GameState, InstallId, PlayerAction, RunPhase, Side, SubroutineStatus};
use netrunner_core::view::ClientView;
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::actions::card_title;

/// More steps than any route in the pool takes (Buzzsaw at 3 credits a
/// pump reaches Wall of Static in two and breaks in one; Unity's
/// one-at-a-time pump against strength 5 ICE is the longest at six), with
/// headroom for boosted ICE. The search is depth-first, so this is what
/// stops a card whose pump never gets there.
const MAX_ROUTE_STEPS: usize = 12;

/// One card's way through the encountered ICE: the actions in order and
/// what they cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The card whose abilities the route uses — a rig card, or a Corp
    /// install whose ability the Runner may use (N-Pot).
    pub target: InstallId,
    pub card: CardId,
    /// The ICE being broken.
    pub ice: InstallId,
    /// Every step, the first of which is in the view's `legal_actions`.
    /// Later steps are what the engine would accept once the Corp passes
    /// in between; the driver re-checks each one on its own view.
    pub steps: Vec<PlayerAction>,
    /// Credits from anywhere a run's costs are paid from — Bad Publicity
    /// credits, the run's own credits, the credit pool.
    pub credits: u32,
    /// Hosted counters the route spends (Botulus, Hantu's pump).
    pub counters: u32,
}

impl Route {
    /// "Break Wall of Static with Corroder · 2 credits".
    pub fn label(&self, view: &ClientView, registry: &CardRegistry) -> String {
        let ice = encountered(view)
            .and_then(|(_, card)| card)
            .map(|card| card_title(&card, registry))
            .unwrap_or_else(|| "the ICE".to_string());
        format!("Break {ice} with {} · {}", card_title(&self.card, registry), self.price())
    }

    /// "2 credits", "1 counter", "3 credits and 1 counter", "free".
    pub fn price(&self) -> String {
        let credits = plural(self.credits, "credit");
        let counters = plural(self.counters, "counter");
        match (self.credits, self.counters) {
            (0, 0) => "free".to_string(),
            (_, 0) => credits,
            (0, _) => counters,
            _ => format!("{credits} and {counters}"),
        }
    }
}

fn plural(n: u32, noun: &str) -> String {
    if n == 1 { format!("1 {noun}") } else { format!("{n} {noun}s") }
}

/// Every card that can break all of the encountered ICE's pending
/// subroutines from this view, cheapest first. Empty unless the viewer is
/// the Runner, mid-encounter, holding priority with a subroutine to break.
pub fn routes(view: &ClientView, registry: &CardRegistry) -> Vec<Route> {
    let Some((ice, _)) = encountered(view) else { return Vec::new() };
    if !has_pending(view) {
        return Vec::new();
    }
    let mut targets: Vec<InstallId> = Vec::new();
    for action in &view.legal_actions {
        if let PlayerAction::ActivateAbility { target, .. } = action
            && !targets.contains(target)
        {
            targets.push(*target);
        }
    }
    let sample = sample(view, registry);
    let mut found: Vec<Route> = targets
        .into_iter()
        .filter_map(|target| route_for(&sample, registry, target, ice))
        .filter(|route| route.steps.first().is_some_and(|step| view.legal_actions.contains(step)))
        .collect();
    found.sort_by_key(|route| (route.credits, route.counters, route.steps.len()));
    found
}

/// The encountered ICE's install and, when the viewer may name it, its
/// card — `None` outside an encounter.
fn encountered(view: &ClientView) -> Option<(InstallId, Option<CardId>)> {
    let run = view.active_run.as_ref()?;
    if run.phase != RunPhase::EncounterIce {
        return None;
    }
    let ice = run.ice.get(run.position)?;
    Some((ice.install_id, ice.identity.as_ref().map(|identity| identity.card.clone())))
}

fn has_pending(view: &ClientView) -> bool {
    view.active_run
        .as_ref()
        .and_then(|run| run.ice.get(run.position))
        .and_then(|ice| ice.identity.as_ref())
        .is_some_and(|identity| identity.subroutines.iter().any(|sub| sub.status == SubroutineStatus::Pending))
}

fn sample(view: &ClientView, registry: &CardRegistry) -> GameState {
    determinize(view, registry, &mut StdRng::seed_from_u64(0))
}

/// The cheapest sequence of `target`'s abilities that leaves `ice` with no
/// pending subroutine, found on `state` by depth-first search bounded by
/// the best route so far.
fn route_for(state: &GameState, registry: &CardRegistry, target: InstallId, ice: InstallId) -> Option<Route> {
    let card = card_at(state, target)?;
    let definition = registry.get(&card)?;
    let (pumps, breaks): (Vec<usize>, Vec<usize>) = definition
        .abilities
        .iter()
        .enumerate()
        .filter(|(_, ability)| ability.trigger == Trigger::Paid)
        .filter_map(|(index, ability)| kind(&ability.effect).map(|kind| (index, kind)))
        .fold((Vec::new(), Vec::new()), |(mut pumps, mut breaks), (index, kind)| {
            match kind {
                Kind::Pump => pumps.push(index),
                Kind::Break => breaks.push(index),
            }
            (pumps, breaks)
        });
    if breaks.is_empty() {
        return None;
    }
    let mut search = Search { registry, target, ice, pumps, breaks, start: purse(state), best: None };
    search.walk(state, &mut Vec::new());
    let (steps, credits, counters) = search.best?;
    Some(Route { target, card, ice, steps, credits, counters })
}

enum Kind {
    Pump,
    Break,
}

/// Whether an ability breaks (anywhere in its effect — Mayfly's break is
/// the first of a `Sequence`) or pumps. A break that also pumps is a
/// break: the search tries breaks first.
fn kind(effect: &Effect) -> Option<Kind> {
    let (mut pumps, mut breaks) = (false, false);
    effect.for_each_effect(&mut |effect| match effect {
        Effect::BreakSubroutines { .. } | Effect::BreakSubroutinesUnconditionally { .. } => breaks = true,
        Effect::BoostStrength { .. } | Effect::BoostStrengthAmount { .. } => pumps = true,
        _ => {}
    });
    if breaks {
        Some(Kind::Break)
    } else if pumps {
        Some(Kind::Pump)
    } else {
        None
    }
}

fn card_at(state: &GameState, target: InstallId) -> Option<CardId> {
    state
        .runner
        .rig
        .iter()
        .find(|card| card.install_id == target)
        .map(|card| card.card.clone())
        .or_else(|| state.corp.installed.iter().find(|card| card.install_id == target).map(|card| card.card.clone()))
}

/// What a route can spend: credits from every pool a run's costs draw on,
/// and the counters on the Runner's cards.
fn purse(state: &GameState) -> (u32, u32) {
    let run = state.active_run.as_ref();
    let credits = state.runner.resources.credits.0
        + run.map_or(0, |run| run.bad_publicity_credits + run.bonus_run_credits);
    let counters = state.runner.rig.iter().map(|card| card.counters).sum();
    (credits, counters)
}

/// Whether `state` is still in the encounter with `ice`, and if so
/// whether anything is left to break.
fn pending_on(state: &GameState, ice: InstallId) -> Option<bool> {
    let run = state.active_run.as_ref()?;
    if run.phase != RunPhase::EncounterIce {
        return None;
    }
    let encountered = run.ice.get(run.position).filter(|encountered| encountered.install_id == ice)?;
    Some(encountered.subroutines.iter().any(|sub| sub.status == SubroutineStatus::Pending))
}

struct Search<'a> {
    registry: &'a CardRegistry,
    target: InstallId,
    ice: InstallId,
    pumps: Vec<usize>,
    breaks: Vec<usize>,
    start: (u32, u32),
    best: Option<(Vec<PlayerAction>, u32, u32)>,
}

impl Search<'_> {
    /// Spent so far on `state`: credits, and counters net of any a step
    /// added (Mayfly adds one to itself), never below zero.
    fn spent(&self, state: &GameState) -> (u32, u32) {
        let now = purse(state);
        (self.start.0.saturating_sub(now.0), self.start.1.saturating_sub(now.1))
    }

    fn better(&self, cost: (u32, u32), steps: usize) -> bool {
        self.best
            .as_ref()
            .is_none_or(|(best_steps, credits, counters)| (cost.0, cost.1, steps) < (*credits, *counters, best_steps.len()))
    }

    fn walk(&mut self, state: &GameState, steps: &mut Vec<PlayerAction>) {
        match pending_on(state, self.ice) {
            // Every subroutine is broken (or none is left pending).
            Some(false) => {
                let cost = self.spent(state);
                if self.better(cost, steps.len()) {
                    self.best = Some((steps.clone(), cost.0, cost.1));
                }
                return;
            }
            // The encounter ended some other way; not a route.
            None => return,
            Some(true) => {}
        }
        if steps.len() >= MAX_ROUTE_STEPS || !self.better(self.spent(state), steps.len() + 1) {
            return;
        }
        // A break the engine accepts is always taken before a pump is
        // tried: once the card is strong enough, pumping again only costs.
        let mut broke = false;
        for index in self.breaks.clone() {
            if let Some(next) = self.step(state, index) {
                broke = true;
                steps.push(self.action(index));
                self.walk(&next, steps);
                steps.pop();
            }
        }
        if broke {
            return;
        }
        for index in self.pumps.clone() {
            if let Some(next) = self.step(state, index) {
                steps.push(self.action(index));
                self.walk(&next, steps);
                steps.pop();
            }
        }
    }

    fn action(&self, ability_index: usize) -> PlayerAction {
        PlayerAction::ActivateAbility { target: self.target, ability_index }
    }

    /// The state after the Runner uses the ability and — if the encounter
    /// is still going — the Corp passes back, which is what the driver
    /// waits for between steps. `None` if the engine refuses either.
    fn step(&self, state: &GameState, ability_index: usize) -> Option<GameState> {
        let (next, _) = apply_action(state, self.registry, self.action(ability_index)).ok()?;
        if pending_on(&next, self.ice) != Some(true) {
            return Some(next);
        }
        match next.paid_ability_window.as_ref().map(|window| window.active_priority) {
            Some(Side::Corp) => apply_action(&next, self.registry, PlayerAction::PassPriority { side: Side::Corp })
                .ok()
                .map(|(passed, _)| passed),
            _ => Some(next),
        }
    }
}

/// What the driver asks the client to do on the view it was handed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// Submit this action; it is in the view's `legal_actions`.
    Submit(PlayerAction),
    /// Not the Runner's priority on this view; ask again on the next one.
    Wait,
    /// Every subroutine is broken. The person moves on.
    Done,
    /// The route no longer holds, in the person's words. The person has
    /// their controls back.
    Stopped(String),
}

/// A route being carried out, one view at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoBreak {
    target: InstallId,
    card: CardId,
    ice: InstallId,
    /// The credits and counters the person agreed to spend, and what they
    /// held when they agreed.
    budget: (u32, u32),
    start: (u32, u32),
}

impl AutoBreak {
    pub fn new(route: &Route, view: &ClientView) -> Self {
        Self {
            target: route.target,
            card: route.card.clone(),
            ice: route.ice,
            budget: (route.credits, route.counters),
            start: view_purse(view),
        }
    }

    /// The next step on `view`, re-planned from it.
    pub fn next(&self, view: &ClientView, registry: &CardRegistry) -> Next {
        match encountered(view) {
            Some((ice, _)) if ice == self.ice => {}
            _ => return Next::Done,
        }
        if !has_pending(view) {
            return Next::Done;
        }
        let runner_priority = view
            .paid_ability_window
            .as_ref()
            .is_some_and(|window| window.active_priority == Side::Runner);
        if !runner_priority {
            return Next::Wait;
        }
        let name = card_title(&self.card, registry);
        let Some(route) = route_for(&sample(view, registry), registry, self.target, self.ice) else {
            return Next::Stopped(format!("{name} can no longer break this ICE."));
        };
        let now = view_purse(view);
        let spent = (self.start.0.saturating_sub(now.0), self.start.1.saturating_sub(now.1));
        if spent.0 + route.credits > self.budget.0 || spent.1 + route.counters > self.budget.1 {
            return Next::Stopped(format!("Breaking with {name} now costs more than it did."));
        }
        match route.steps.first() {
            Some(step) if view.legal_actions.contains(step) => Next::Submit(step.clone()),
            _ => Next::Stopped(format!("{name}'s next step is not available.")),
        }
    }
}

/// `purse` read off the view — the same pools, all public.
fn view_purse(view: &ClientView) -> (u32, u32) {
    let run = view.active_run.as_ref();
    let credits = view.runner.credits + run.map_or(0, |run| run.bad_publicity_credits + run.bonus_run_credits);
    let counters = view.runner.rig.iter().map(|card| card.counters).sum();
    (credits, counters)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::CardType;
    use netrunner_core::rules::{
        EncounteredSubroutine, InstallSlot, InstalledCard, InstalledRunnerCard, PaidAbilityWindow, RunIce, RunState, ServerId,
        WindowCheckpoint, GamePhase,
    };
    use netrunner_core::view::build_client_view;

    use crate::decks::sample_deck_registry;

    /// A real mid-encounter state: `ice` rezzed on HQ and being encountered,
    /// `rig` installed, the Runner holding priority in the encounter's
    /// window with `credits` in the pool.
    fn encounter(registry: &CardRegistry, ice: &str, rig: &[&str], credits: u32) -> GameState {
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.credits.0 = credits;
        for (i, card) in rig.iter().enumerate() {
            let definition = registry.get(&CardId(card.to_string())).expect("a rig card in the pool");
            state.runner.rig.push(InstalledRunnerCard {
                card: definition.id.clone(),
                install_id: InstallId(100 + i as u32),
                base_strength: definition.strength.unwrap_or(0),
                ..Default::default()
            });
        }
        let definition = registry.get(&CardId(ice.to_string())).expect("ICE in the pool");
        let CardType::Ice(ice_type) = definition.card_type else { panic!("{ice} is not ICE") };
        let install_id = InstallId(1);
        state.corp.installed.push(InstalledCard {
            install_id,
            card: definition.id.clone(),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        });
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                card_id: definition.id.clone(),
                install_id,
                current_strength: definition.strength.unwrap_or(0),
                ice_type,
                subroutines: definition
                    .subroutines
                    .iter()
                    .enumerate()
                    .map(|(id, sub)| EncounteredSubroutine { id, definition: sub.clone(), status: SubroutineStatus::Pending })
                    .collect(),
                rezzed: true,
            }],
            position: 0,
            ..Default::default()
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
        });
        state
    }

    fn view(state: &GameState, registry: &CardRegistry) -> ClientView {
        build_client_view(state, registry, Side::Runner)
    }

    fn priced(routes: &[Route]) -> Vec<(String, u32, u32)> {
        routes.iter().map(|route| (route.card.0.clone(), route.credits, route.counters)).collect()
    }

    #[test]
    fn each_card_that_can_break_the_ice_is_a_route_cheapest_first() {
        let registry = sample_deck_registry();
        // Wall of Static: a strength-3 barrier with one subroutine. Corroder
        // pumps once and breaks (2); Mayfly breaks any type but pumps twice
        // (3); Gordian Blade breaks only code gates, so it is no route.
        let state = encounter(&registry, "wall_of_static", &["mayfly", "gordian_blade", "corroder"], 10);
        let view = view(&state, &registry);
        let found = routes(&view, &registry);
        assert_eq!(priced(&found), [("corroder".to_string(), 2, 0), ("mayfly".to_string(), 3, 0)]);
        assert_eq!(found[0].label(&view, &registry), "Break Wall of Static with Corroder · 2 credits");
        assert!(view.legal_actions.contains(&found[0].steps[0]), "the first step is one the engine lists");
    }

    #[test]
    fn a_card_that_cannot_afford_the_whole_ice_is_not_offered() {
        let registry = sample_deck_registry();
        let state = encounter(&registry, "wall_of_static", &["corroder"], 1);
        assert!(routes(&view(&state, &registry), &registry).is_empty(), "a pump alone breaks nothing");
    }

    #[test]
    fn nothing_is_offered_outside_an_encounter_or_off_priority() {
        let registry = sample_deck_registry();
        let mut state = encounter(&registry, "wall_of_static", &["corroder"], 10);
        state.paid_ability_window.as_mut().unwrap().active_priority = Side::Corp;
        assert!(routes(&view(&state, &registry), &registry).is_empty());
        state.paid_ability_window.as_mut().unwrap().active_priority = Side::Runner;
        state.active_run.as_mut().unwrap().phase = RunPhase::ApproachIce;
        assert!(routes(&view(&state, &registry), &registry).is_empty());
    }

    #[test]
    fn a_break_that_takes_every_subroutine_needs_no_second_break() {
        let registry = sample_deck_registry();
        // Enigma: a strength-2 code gate with two subroutines. Gordian
        // Blade is at strength and breaks one a credit.
        let state = encounter(&registry, "enigma", &["gordian_blade"], 5);
        let found = routes(&view(&state, &registry), &registry);
        assert_eq!(priced(&found), [("gordian_blade".to_string(), 2, 0)]);
        assert_eq!(found[0].steps.len(), 2);
    }

    /// Why a route is played rather than priced: the view says Echelon
    /// is strength 0, but it is +1 for each installed icebreaker, which the
    /// engine counts and the view's `current_strength` does not. Priced off
    /// the view, Karuna (a strength-3 sentry, two subroutines) would need
    /// two 3-credit pumps before the breaks.
    #[test]
    fn a_strength_the_view_leaves_out_is_still_priced_right() {
        let registry = sample_deck_registry();
        let state = encounter(&registry, "karuna", &["echelon", "corroder", "gordian_blade"], 20);
        let view = view(&state, &registry);
        assert_eq!(view.runner.rig[0].current_strength, 0, "the view leaves the modifier out");
        let found = routes(&view, &registry);
        assert_eq!(priced(&found), [("echelon".to_string(), 2, 0)], "three icebreakers: strength 3, no pump");
    }

    /// The driver end to end against the engine: one step a view, the
    /// Corp passing in between, until nothing is left — and it spends
    /// exactly what the button said, and never passes priority itself.
    #[test]
    fn the_driver_spends_what_the_route_priced_one_step_a_view() {
        let registry = sample_deck_registry();
        let mut state = encounter(&registry, "wall_of_static", &["corroder"], 10);
        let first = view(&state, &registry);
        let route = routes(&first, &registry).remove(0);
        let driver = AutoBreak::new(&route, &first);
        let mut submitted = 0;
        loop {
            let now = view(&state, &registry);
            match driver.next(&now, &registry) {
                Next::Submit(action) => {
                    assert!(!matches!(action, PlayerAction::PassPriority { .. }));
                    state = apply_action(&state, &registry, action).expect("the step is legal").0;
                    submitted += 1;
                    if state.paid_ability_window.as_ref().is_some_and(|w| w.active_priority == Side::Corp) {
                        // Off priority the driver waits — or, after the
                        // last break, is already done.
                        let waiting = driver.next(&view(&state, &registry), &registry);
                        assert!(matches!(waiting, Next::Wait | Next::Done), "{waiting:?}");
                        state = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).unwrap().0;
                    }
                }
                Next::Done => break,
                other => panic!("driver stopped: {other:?}"),
            }
            assert!(submitted <= route.steps.len(), "the driver took more steps than the route");
        }
        assert_eq!(submitted, route.steps.len());
        assert_eq!(state.runner.resources.credits.0, 10 - route.credits);
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::EncounterIce, "the person moves on, not the driver");
    }

    #[test]
    fn the_driver_stops_when_the_price_rises() {
        let registry = sample_deck_registry();
        let mut state = encounter(&registry, "wall_of_static", &["corroder"], 10);
        let first = view(&state, &registry);
        let driver = AutoBreak::new(&routes(&first, &registry)[0], &first);
        // The ICE grows between views: the route now needs a second pump.
        state.active_run.as_mut().unwrap().ice[0].current_strength += 1;
        assert!(matches!(driver.next(&view(&state, &registry), &registry), Next::Stopped(_)));
    }
}
