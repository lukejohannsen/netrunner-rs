//! `netrunner_cli diag rez-rate`: when the Runner approaches an ICE the
//! Corp could rez, how often does it?
//!
//! ROADMAP Phase 2 §5 item 38 shipped `unbreakable_unrezzed_ice` at weight
//! **0.0**. The term does what item 36 said no term did — it is the
//! evaluator's one window onto the hidden state — and in games it buys
//! nothing (+0.016 / +0.010 on PUCT's Runner chair, under the 0.026–0.047
//! band). The entry's own explanation for the failure is a *probability*:
//! the term treats an unrezzed ICE as certain to be rezzed the moment the
//! Corp can afford it, and a real Corp declines, so the Runner is made
//! systematically too cautious on the chair that is already the weak one.
//!
//! That explanation is testable without playing a single extra kind of
//! game, and this is the test. Every approach to an unrezzed ICE is a rez
//! decision the Corp actually made; counting the ones it took gives the
//! discount the term is missing. If the rate is near 1.0 the explanation
//! is wrong and the Runner chair is not leaf-bound at all (item 37 already
//! makes that likely — `mcts`'s samples differ in HQ and R&D, never in
//! what is behind the ICE). If it is far from 1.0, the term has a
//! measured coefficient to be scaled by.
//!
//! **The denominator is the term's own predicate, not a copy of it.**
//! `netrunner_bots::is_unrezzed_threat` is the exact filter
//! `unbreakable_unrezzed_ice` counts with, called here against the
//! *authoritative* state rather than a determinized one — the true card,
//! which is what a rate has to be measured over. A second denominator,
//! the engine's own `RezIce` legality, is reported beside it because the
//! two differ: the term reads printed cost against credits and ignores
//! `ice_rez_cost_modifier`, a deliberate over-estimate (see its doc
//! comment), and the gap between the two columns is exactly how much that
//! over-estimate costs.
//!
//! **A window, not an event.** The Corp may act several times while the
//! Runner approaches — rez an asset, then the ICE — so one approach to one
//! ICE is one observation, opened when the approach is first seen and
//! closed when the position moves, the run ends, or the ICE is rezzed.
//! Reading `GameState` directly is allowed here for the reason in
//! `diag`'s module doc: a diagnostic is not a client.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::Serialize;

use netrunner_bots::is_unrezzed_threat;
use netrunner_core::dsl::Effect;
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks as core_decks;
use netrunner_core::rules::{GameEvent, GameState, PlayerAction, RunPhase, ServerId, Side, legal_actions_for};
use netrunner_session::{Seat, Session, SessionStep};

use crate::bots;
use crate::config::{BotSpec, Config};
use crate::decks;

pub struct RezRateArgs {
    pub corp: BotSpec,
    pub runner: BotSpec,
    pub games: u32,
    pub seed: u64,
    pub simulations: usize,
    pub determinizations: Option<usize>,
    pub threads: Option<usize>,
    pub report: Option<PathBuf>,
}

/// One approach to one unrezzed ICE: the rez decision the Corp was handed
/// and what it did with it.
#[derive(Debug, Clone, Serialize)]
pub struct Approach {
    pub game: u32,
    pub seed: u64,
    pub step: u32,
    /// Which server the run is on, as `Hq`/`RnD`/`Archives`/`Remote`.
    pub server: String,
    /// Position from the outside in: 0 is the outermost ICE.
    pub position: usize,
    /// ICE still ahead of the Runner, this one included.
    pub ice_remaining: usize,
    pub card: String,
    /// Printed rez cost.
    pub cost: u32,
    pub corp_credits: u32,
    /// The engine offered `RezIce` for this ICE at some point in the
    /// window — true affordability, modifiers and discounts included.
    pub legal: bool,
    /// `is_unrezzed_threat` held when the window opened: what
    /// `unbreakable_unrezzed_ice` would have counted.
    pub threat: bool,
    /// The Corp rezzed it before the approach ended.
    pub rezzed: bool,
    /// Any subroutine on the ICE ends the run. `unbreakable_unrezzed_ice`
    /// does not ask: an ICE no rig card can break might only tag, or do
    /// damage, or drain credits, and the Runner walks through it.
    pub ends_the_run: bool,
    /// The run got no further than this ICE — it ended here rather than
    /// reaching the next position or the server. A jack-out counts: the
    /// term's claim is "this stops you", and a Runner who leaves rather
    /// than face it was stopped by it just as surely.
    pub run_stopped_here: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Rate {
    pub opportunities: usize,
    pub rezzed: usize,
    pub rate: f64,
    /// Binomial sd of `rate`, so a small cell is not read as a number.
    pub sd: f64,
}

impl Rate {
    fn of(approaches: impl Iterator<Item = bool>) -> Rate {
        let mut rate = Rate::default();
        for rezzed in approaches {
            rate.opportunities += 1;
            rate.rezzed += usize::from(rezzed);
        }
        if rate.opportunities > 0 {
            let n = rate.opportunities as f64;
            rate.rate = rate.rezzed as f64 / n;
            rate.sd = (rate.rate * (1.0 - rate.rate) / n).sqrt();
        }
        rate
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RezRateReport {
    pub corp: String,
    pub runner: String,
    pub games: u32,
    pub seed: u64,
    pub simulations: usize,
    pub determinizations: Option<usize>,
    /// Approach steps the watcher observed, rezzed ICE included, against
    /// the `GameEvent::IceApproached` the same games emitted. They must
    /// agree: a disagreement means the step loop cannot see every
    /// approach and every rate below is over an unknown subset.
    pub approach_steps: usize,
    pub approach_events: usize,
    /// Approaches to an *unrezzed* ICE — a rez decision the Corp was
    /// actually handed, and the widest honest denominator.
    pub approaches: usize,
    /// Of those, the ones the engine would have let the Corp rez.
    pub affordable: Rate,
    /// Of those, the ones `unbreakable_unrezzed_ice` counts — the term's
    /// own predicate, and the number it should be discounted by.
    pub threat: Rate,
    /// The other half of the term's claim: of the approaches it counts,
    /// how often the run actually stopped there. A rez the Runner then
    /// walks through — an unbreakable ICE whose subroutines do not end
    /// the run — is a threat the term priced and the game did not.
    pub threat_stopped: Rate,
    /// `threat_stopped` over the ones that were actually rezzed, which is
    /// the conditional the term is really asserting.
    pub rezzed_stopped: Rate,
    /// `rezzed_stopped`, split by whether the ICE has a subroutine that
    /// ends the run. The term counts both alike; if these two columns
    /// differ, that is the distinction it is missing.
    pub rezzed_stopped_etr: Rate,
    pub rezzed_stopped_no_etr: Rate,
    /// `threat`, split by the server being run. A Corp defends a remote it
    /// is scoring in differently from a central, and the term makes no
    /// such distinction.
    pub threat_by_server: BTreeMap<String, Rate>,
    /// `threat`, split by printed rez cost, cheapest first. The term is
    /// flat in cost; if the rate is not, that is the shape a discount
    /// would want.
    pub threat_by_cost: BTreeMap<String, Rate>,
}

pub fn run(args: &RezRateArgs, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::sample_deck_registry();
    let matchups = core_decks::matchups();
    let pool = match args.threads {
        Some(threads) => rayon::ThreadPoolBuilder::new().num_threads(threads).build()?,
        None => rayon::ThreadPoolBuilder::new().build()?,
    };

    let corp = describe(args.corp);
    let runner = describe(args.runner);
    println!("playing {} games, {corp} Corp vs {runner} Runner, seed {}...", args.games, args.seed);

    let games: Vec<Game> = pool.install(|| {
        (0..args.games)
            .into_par_iter()
            .map(|game| play(game, args, &registry, &matchups, config))
            .collect::<Result<Vec<_>, String>>()
    })?;
    let seen: usize = games.iter().map(|game| game.seen).sum();
    let announced: usize = games.iter().map(|game| game.announced).sum();
    let mut approaches: Vec<Approach> = games.into_iter().flat_map(|game| game.approaches).collect();
    approaches.sort_by_key(|a| (a.game, a.step));

    let threats: Vec<&Approach> = approaches.iter().filter(|a| a.threat).collect();
    let report = RezRateReport {
        corp,
        runner,
        games: args.games,
        seed: args.seed,
        simulations: args.simulations,
        determinizations: args.determinizations,
        approach_steps: seen,
        approach_events: announced,
        approaches: approaches.len(),
        affordable: Rate::of(approaches.iter().filter(|a| a.legal).map(|a| a.rezzed)),
        threat: Rate::of(threats.iter().map(|a| a.rezzed)),
        threat_stopped: Rate::of(threats.iter().map(|a| a.run_stopped_here)),
        rezzed_stopped: Rate::of(threats.iter().filter(|a| a.rezzed).map(|a| a.run_stopped_here)),
        rezzed_stopped_etr: Rate::of(threats.iter().filter(|a| a.rezzed && a.ends_the_run).map(|a| a.run_stopped_here)),
        rezzed_stopped_no_etr: Rate::of(threats.iter().filter(|a| a.rezzed && !a.ends_the_run).map(|a| a.run_stopped_here)),
        threat_by_server: group(&threats, |a| a.server.clone()),
        threat_by_cost: group(&threats, |a| cost_bucket(a.cost)),
    };

    print_report(&report);
    if let Some(path) = &args.report {
        fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("\nreport written to {}", path.display());
    }
    Ok(())
}

fn group(approaches: &[&Approach], key: impl Fn(&Approach) -> String) -> BTreeMap<String, Rate> {
    let mut buckets: BTreeMap<String, Vec<bool>> = BTreeMap::new();
    for approach in approaches {
        buckets.entry(key(approach)).or_default().push(approach.rezzed);
    }
    buckets.into_iter().map(|(name, rezzed)| (name, Rate::of(rezzed.into_iter()))).collect()
}

/// Buckets rather than raw costs, because a per-cost table over 178 cards
/// is mostly cells of one. Named so they sort in cost order as strings.
fn cost_bucket(cost: u32) -> String {
    match cost {
        0..=2 => "0: 0-2".to_string(),
        3..=4 => "1: 3-4".to_string(),
        5..=7 => "2: 5-7".to_string(),
        _ => "3: 8+".to_string(),
    }
}

fn play(
    game: u32,
    args: &RezRateArgs,
    registry: &CardRegistry,
    matchups: &[(core_decks::DeckFile, core_decks::DeckFile)],
    config: &Config,
) -> Result<Game, String> {
    let seed = args.seed.wrapping_add(u64::from(game));
    let (corp_deck, runner_deck) = &matchups[game as usize % matchups.len()];
    let (state, _events) =
        GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), registry, seed).map_err(|e| format!("{e:?}"))?;
    let setup = |spec: BotSpec| bots::AgentSetup {
        simulations: args.simulations,
        determinizations: args.determinizations,
        shared_sample: false,
        personality: spec.personality,
    };
    let corp = bots::make_agent_with_model(args.corp.kind, Side::Corp, seed, setup(args.corp), &config.model)?
        .ok_or("the Corp seat must be a bot that can take one")?;
    let runner =
        bots::make_agent_with_model(args.runner.kind, Side::Runner, seed.wrapping_add(1), setup(args.runner), &config.model)?
            .ok_or("the Runner seat must be a bot that can take one")?;
    let mut session = Session::new(state, registry.clone(), Seat::Agent(corp), Seat::Agent(runner));

    let mut watcher = Watcher::default();
    let mut announced = 0usize;
    loop {
        watcher.observe(game, seed, session.steps(), session.state(), registry);
        match session.step() {
            SessionStep::Applied { .. } => {
                announced += session
                    .last_entry()
                    .map_or(0, |entry| entry.events.iter().filter(|e| matches!(e, GameEvent::IceApproached { .. })).count());
                continue;
            }
            _ => break,
        }
    }
    watcher.observe(game, seed, session.steps(), session.state(), registry);
    let (approaches, seen) = watcher.finish();
    Ok(Game { approaches, seen, announced })
}

/// One game's approaches, with the two counts that say whether the state
/// machine above saw all of them.
struct Game {
    approaches: Vec<Approach>,
    /// Approach steps the watcher saw at a step boundary.
    seen: usize,
    /// `GameEvent::IceApproached` in the same game. The engine can walk
    /// several run steps inside one applied action, so an approach that
    /// nobody had a decision in could in principle never be a state this
    /// loop observes. If these two agree, none was missed.
    announced: usize,
}

/// Turns a sequence of authoritative states into one record per approach.
///
/// A state-machine rather than an event tally, because neither half of the
/// question is in the event stream: `GameEvent::IceApproached` carries no
/// affordability, and a declined rez emits nothing at all. The run counter
/// exists so that approaching the same position of the same server twice
/// in one game is two observations.
#[derive(Default)]
struct Watcher {
    /// The approach being watched. Held open across the Corp's whole rez
    /// window: it may act several times there, and only the last state of
    /// the ICE says what it decided.
    open: Option<Approach>,
    closed: Vec<Approach>,
    /// Runs so far this game, so the same position of the same server on
    /// a later run is a second decision.
    run: u32,
    in_run: bool,
    /// The `(run, position)` currently in its approach step.
    watching: Option<(u32, usize)>,
    /// Every approach seen, rezzed ICE included — the number checked
    /// against `GameEvent::IceApproached`.
    approached: usize,
    /// Indices into `closed` for the current run whose fate is not yet
    /// known: the run has neither passed them nor ended.
    unresolved: Vec<usize>,
}

impl Watcher {
    fn observe(&mut self, game: u32, seed: u64, step: u32, state: &GameState, registry: &CardRegistry) {
        match &state.active_run {
            Some(_) if !self.in_run => {
                self.in_run = true;
                self.run += 1;
                // A run that ends and another that starts inside one
                // applied action is never observed as "no run", so the
                // previous run's approaches are resolved here too.
                self.end_of_run();
            }
            None if self.in_run => {
                self.in_run = false;
                self.end_of_run();
            }
            _ => {}
        }

        let Some(run) = &state.active_run else {
            self.watching = None;
            self.close();
            return;
        };

        // The approach step is the rez window. Every other step of the
        // run still has to be observed, because passing an ICE is how an
        // approach gets resolved as "the run got through".
        let approaching = run.phase == RunPhase::ApproachIce && run.position < run.ice.len();
        let key = approaching.then_some((self.run, run.position));
        let opened = key.is_some() && key != self.watching;
        if key != self.watching {
            self.watching = key;
            self.close();
        }
        // Anything the run has moved past is not where it stopped. The
        // server step — `position` past the last ICE — goes through the
        // same comparison, which is the whole reason this runs on every
        // step and not only inside an approach: the innermost ICE is
        // otherwise never resolved and every run reads as stopped there.
        self.unresolved.retain(|index| self.closed[*index].position >= run.position);
        if !approaching {
            return;
        }

        let ice = &run.ice[run.position];
        let legal = || {
            legal_actions_for(state, registry, Side::Corp)
                .iter()
                .any(|action| matches!(action, PlayerAction::RezIce { ice: install } if *install == ice.install_id))
        };
        if opened {
            self.approached += 1;
            // An ICE already rezzed when its approach opens was decided
            // in an earlier run; counting it would put a guaranteed
            // "rezzed" in a denominator nobody chose here.
            if !ice.rezzed {
                self.open = Some(Approach {
                    game,
                    seed,
                    step,
                    server: server_name(run.server),
                    position: run.position,
                    ice_remaining: run.ice.len() - run.position,
                    card: ice.card_id.0.clone(),
                    cost: registry.get(&ice.card_id).map_or(0, |definition| definition.cost),
                    corp_credits: state.corp.resources.credits.0,
                    legal: legal(),
                    threat: is_unrezzed_threat(state, ice, registry),
                    ends_the_run: ice.subroutines.iter().any(|sub| ends_the_run(&sub.definition.effect)),
                    rezzed: false,
                    run_stopped_here: false,
                });
            }
            return;
        }

        let Some(open) = &mut self.open else { return };
        if ice.rezzed {
            open.rezzed = true;
            return self.close();
        }
        // Credits move within a window — a rezzed asset, a trashed
        // resource — so legality is whether the Corp could rez at any
        // point in it, not only at the first observation.
        open.legal |= legal();
    }

    fn close(&mut self) {
        if let Some(open) = self.open.take() {
            self.closed.push(open);
            self.unresolved.push(self.closed.len() - 1);
        }
    }

    /// The run is over; every approach it never got past is where it
    /// stopped.
    fn end_of_run(&mut self) {
        self.close();
        for index in self.unresolved.drain(..) {
            self.closed[index].run_stopped_here = true;
        }
    }

    fn finish(mut self) -> (Vec<Approach>, usize) {
        self.end_of_run();
        (self.closed, self.approached)
    }
}

/// Whether resolving `effect` can end the run, at any depth.
///
/// A scan of the serialized AST rather than a walk over `Effect`'s 74
/// variants: `EndTheRun` nests inside `Sequence`, `EffectIf`,
/// `PresentChoice` and `OfferPaidChoice`, and a hand-written walk in the
/// CLI would silently stop finding it the day a new wrapper variant is
/// added — the failure mode this measurement can least afford, since it
/// would move the number rather than break the build. `Effect` is
/// `Serialize` (it crosses a process boundary in every `GameEvent`), and
/// `EndTheRun` is a unit variant, so the name appears in the JSON only
/// where the effect itself does.
fn ends_the_run(effect: &Effect) -> bool {
    serde_json::to_string(effect).is_ok_and(|json| json.contains("EndTheRun"))
}

fn server_name(server: ServerId) -> String {
    match server {
        ServerId::Hq => "Hq".to_string(),
        ServerId::RnD => "RnD".to_string(),
        ServerId::Archives => "Archives".to_string(),
        ServerId::Remote(_) => "Remote".to_string(),
    }
}

fn describe(spec: BotSpec) -> String {
    format!("{:?}:{:?}", spec.kind, spec.personality).to_lowercase()
}

fn print_report(report: &RezRateReport) {
    println!(
        "\n{} approach steps over {} games ({} IceApproached events{})",
        report.approach_steps,
        report.games,
        report.approach_events,
        if report.approach_steps == report.approach_events { "; every approach seen" } else { " — MISSED SOME" }
    );
    println!("{} of them were to an unrezzed ICE", report.approaches);
    println!("  {:>34}  {:>7}  {:>7}  {:>7}  {:>7}", "denominator", "n", "rezzed", "rate", "sd");
    print_rate("engine-legal to rez", &report.affordable);
    print_rate("counted by unbreakable_unrezzed_ice", &report.threat);
    println!("  {:>34}  {:>7}  {:>7}  {:>7}  {:>7}", "", "n", "stopped", "rate", "sd");
    print_rate("counted, and the run stopped there", &report.threat_stopped);
    print_rate("rezzed, and the run stopped there", &report.rezzed_stopped);
    print_rate("  of those, with an ETR subroutine", &report.rezzed_stopped_etr);
    print_rate("  of those, with none", &report.rezzed_stopped_no_etr);
    for (name, rows) in [("server", &report.threat_by_server), ("printed cost", &report.threat_by_cost)] {
        println!("\n  counted, by {name}:");
        for (key, rate) in rows {
            print_rate(key, rate);
        }
    }
}

fn print_rate(label: &str, rate: &Rate) {
    println!(
        "  {:>34}  {:>7}  {:>7}  {:>7.3}  {:>7.3}",
        label, rate.opportunities, rate.rezzed, rate.rate, rate.sd
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType, IceType};
    use netrunner_core::rules::{InstallId, RunIce, RunState};

    fn registry() -> CardRegistry {
        CardRegistry::from_cards(vec![CardDefinition {
            id: CardId("palisade".to_string()),
            cost: 3,
            strength: Some(4),
            card_type: CardType::Ice(IceType::Barrier),
            ..CardDefinition::default()
        }])
    }

    fn approaching(position: usize, rezzed: bool, phase: RunPhase) -> GameState {
        let ice = |rezzed| RunIce {
            install_id: InstallId::PLACEHOLDER,
            card_id: CardId("palisade".to_string()),
            current_strength: 4,
            ice_type: IceType::Barrier,
            subroutines: Vec::new(),
            rezzed,
        };
        let mut state = GameState::new(0);
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase,
            position,
            // Two ICE, so a run can move from one approach to the next
            // without the run ending in between.
            ice: vec![ice(rezzed && position == 0), ice(rezzed && position == 1)],
            ..RunState::default()
        });
        state
    }

    /// The Corp may act several times in one rez window — rez an asset,
    /// take a credit, then decide — and that is one decision, not three.
    #[test]
    fn one_approach_is_one_observation_however_often_it_is_observed() {
        let registry = registry();
        let mut watcher = Watcher::default();
        for step in 0..5 {
            watcher.observe(0, 0, step, &approaching(0, false, RunPhase::ApproachIce), &registry);
        }
        let (approaches, seen) = watcher.finish();
        assert_eq!((approaches.len(), seen), (1, 1));
        assert!(!approaches[0].rezzed, "the window closed with the ICE still unrezzed");
    }

    /// The ICE turning rezzed while the window is open is the Corp taking
    /// the decision; the same ICE already rezzed when the approach opens
    /// is not a decision made here and must not become a denominator.
    #[test]
    fn a_rez_inside_the_window_is_the_decision_and_one_before_it_is_not() {
        let registry = registry();
        let mut watcher = Watcher::default();
        watcher.observe(0, 0, 0, &approaching(0, false, RunPhase::ApproachIce), &registry);
        watcher.observe(0, 0, 1, &approaching(0, true, RunPhase::ApproachIce), &registry);
        let (taken, _) = watcher.finish();
        assert_eq!(taken.len(), 1);
        assert!(taken[0].rezzed);

        let mut watcher = Watcher::default();
        watcher.observe(0, 0, 0, &approaching(0, true, RunPhase::ApproachIce), &registry);
        let (approaches, seen) = watcher.finish();
        assert!(approaches.is_empty(), "an ICE rezzed before the approach was never a decision");
        assert_eq!(seen, 1, "it is still an approach, and the event stream will have counted it");
    }

    /// Position and run identity both have to close a window: the same
    /// server approached twice in a game is two decisions, and so is the
    /// second ICE of one run.
    #[test]
    fn each_position_and_each_run_is_its_own_observation() {
        let registry = registry();
        let mut watcher = Watcher::default();
        watcher.observe(0, 0, 0, &approaching(0, false, RunPhase::ApproachIce), &registry);
        watcher.observe(0, 0, 1, &approaching(1, false, RunPhase::ApproachIce), &registry);
        assert_eq!(watcher.finish().0.len(), 2, "two ICE on one run");

        let mut watcher = Watcher::default();
        watcher.observe(0, 0, 0, &approaching(0, false, RunPhase::ApproachIce), &registry);
        watcher.observe(0, 0, 1, &GameState::new(0), &registry);
        watcher.observe(0, 0, 2, &approaching(0, false, RunPhase::ApproachIce), &registry);
        let (approaches, _) = watcher.finish();
        assert_eq!(approaches.len(), 2, "the same position on a later run is a second decision");
    }

    /// Only the approach step is a rez decision. An encounter is what
    /// happens after one was already taken, and counting it would double
    /// every rez the Corp made.
    #[test]
    fn only_the_approach_phase_opens_a_window() {
        let registry = registry();
        let mut watcher = Watcher::default();
        watcher.observe(0, 0, 0, &approaching(0, false, RunPhase::EncounterIce), &registry);
        watcher.observe(0, 0, 1, &approaching(0, false, RunPhase::AccessingCard), &registry);
        let (approaches, seen) = watcher.finish();
        assert!(approaches.is_empty());
        assert_eq!(seen, 0, "nor does either count as an approach step");
    }

    /// The other half of the term's claim. A run that never gets past the
    /// ICE stopped there; one that reaches the next position did not, and
    /// an unbreakable ICE whose subroutines do not end the run is exactly
    /// the case the term prices and the game does not.
    #[test]
    fn an_approach_the_run_never_got_past_is_where_it_stopped() {
        let registry = registry();
        let mut watcher = Watcher::default();
        watcher.observe(0, 0, 0, &approaching(0, false, RunPhase::ApproachIce), &registry);
        watcher.observe(0, 0, 1, &GameState::new(0), &registry);
        let (approaches, _) = watcher.finish();
        assert!(approaches[0].run_stopped_here, "the run ended while still at this ICE");

        let mut watcher = Watcher::default();
        watcher.observe(0, 0, 0, &approaching(0, false, RunPhase::ApproachIce), &registry);
        watcher.observe(0, 0, 1, &approaching(1, false, RunPhase::ApproachIce), &registry);
        watcher.observe(0, 0, 2, &GameState::new(0), &registry);
        let (approaches, _) = watcher.finish();
        assert!(!approaches[0].run_stopped_here, "the run reached the next ICE, so it got past this one");
        assert!(approaches[1].run_stopped_here);
    }

    /// The bug this test exists for: resolving "the run got past it" only
    /// inside an approach step leaves the *innermost* ICE unresolved
    /// forever, because nothing is ever approached after it — and every
    /// run then reads as having been stopped by its last ICE. Measured
    /// before the fix, a Corp that declined half its rezzes still showed
    /// 0.98 of its counted approaches stopping the run.
    #[test]
    fn passing_the_innermost_ice_resolves_it_even_though_no_approach_follows() {
        let registry = registry();
        let mut watcher = Watcher::default();
        watcher.observe(0, 0, 0, &approaching(1, false, RunPhase::ApproachIce), &registry);
        watcher.observe(0, 0, 1, &approaching(2, false, RunPhase::Success), &registry);
        watcher.observe(0, 0, 2, &GameState::new(0), &registry);
        let (approaches, _) = watcher.finish();
        assert_eq!(approaches.len(), 1);
        assert!(!approaches[0].run_stopped_here, "the run reached the server");
    }

    /// The distinction the term does not draw, and the reason the stop
    /// rate is what it is. Nested because real card text nests: Palisade
    /// ends the run outright, a bioroid offers a choice that ends it, and
    /// a tagging ICE no rig card can break stops nothing at all.
    #[test]
    fn ending_the_run_is_found_at_any_depth_of_a_subroutine() {
        assert!(ends_the_run(&Effect::EndTheRun));
        assert!(ends_the_run(&Effect::Sequence(vec![Effect::GiveTags(1), Effect::EndTheRun])));
        assert!(!ends_the_run(&Effect::GiveTags(1)));
        assert!(!ends_the_run(&Effect::Sequence(vec![Effect::GiveTags(1)])));
    }

    #[test]
    fn a_rate_is_the_share_rezzed_with_its_binomial_sd() {
        let rate = Rate::of([true, true, false, true].into_iter());
        assert_eq!((rate.opportunities, rate.rezzed), (4, 3));
        assert_eq!(rate.rate, 0.75);
        assert!((rate.sd - 0.2165).abs() < 1e-4);
        assert_eq!(Rate::of([].into_iter()).rate, 0.0, "an empty cell reads zero, not NaN");
    }
}
