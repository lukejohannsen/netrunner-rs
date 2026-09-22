//! `netrunner_cli diag trap`: how do the bots play a trap, and how do
//! they play *against* one?
//!
//! A report from play: the Corp turns its traps face up (a rezzed
//! Urtica Cipher is a card the Runner simply stops running), puts them in
//! a bare remote with no ICE and no tokens, and installs Snare! in a
//! remote where it lures nothing; and the Runner keeps running an Urtica
//! it has already hit. None of that is a win rate. This counts it.
//!
//! **Two kinds of trap, read off the card** (`eval::punishes_access_with_damage`):
//! a *lure* trap grows with its advancement tokens
//! (`eval::damage_grows_with_advancement` — Urtica Cipher) and is meant to
//! be installed, iced and advanced like an agenda; a *hand* trap cannot be
//! advanced (`advancement_requirement: None` — Snare!, Byte!) and is meant
//! to wait in HQ and R&D for a Runner who is looking for agendas.
//!
//! **"Known" is what the Runner has seen**: a trap install it has
//! accessed, or one turned face up. The engine does not remember an
//! access, so the diagnostic does, by `InstallId` — a run on a remote
//! whose root is nothing but known traps is a run into a trap the Runner
//! had every reason to see coming.
//!
//! **Damage is attributed to the trap accessed last in the run.** A
//! `DamageTaken` the Corp is responsible for, between a trap's access and
//! the next card accessed or the end of the run, is the trap's — which is
//! how Snare!'s paid damage, landing on a later action than the access
//! that parked it, is still counted.
//!
//! Reading `GameState` directly is allowed here for the reason in `diag`'s
//! module doc: a diagnostic is not a client.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::Serialize;

use netrunner_bots::eval::{damage_grows_with_advancement, punishes_access_with_damage};
use netrunner_bots::Personality;
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks as core_decks;
use netrunner_core::dsl::{CardDefinition, CardId};
use netrunner_core::rules::{GameEvent, GamePhase, GameState, InstallId, InstallSlot, ServerId, Side};
use netrunner_session::{Seat, Session, SessionStep};

use crate::bots;
use crate::config::{BotSpec, Config};
use netrunner_client::decks;

pub struct TrapArgs {
    pub corp: BotSpec,
    pub runner: BotSpec,
    pub games: u32,
    pub seed: u64,
    pub simulations: usize,
    pub determinizations: Option<usize>,
    pub threads: Option<usize>,
    /// Play only this matchup, `CORP_DECK/RUNNER_DECK` by deck id. Without
    /// it, every sample matchup whose Corp deck carries a trap.
    pub matchup: Option<String>,
    /// Seat each chair in its deck's own style (`Personality::for_deck`),
    /// as play does, instead of the style the bot spec names.
    pub deck_styles: bool,
    pub report: Option<PathBuf>,
}

/// Which kind of trap a card is, or `None` for a card that is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrapKind {
    Lure,
    Hand,
}

pub fn trap_kind(def: &CardDefinition) -> Option<TrapKind> {
    if !punishes_access_with_damage(def) {
        return None;
    }
    // Growing with its tokens is what makes an advanced trap a threat the
    // Runner has to weigh; a trap that cannot take a token looks, in a
    // remote, like nothing the Corp would ever protect.
    Some(if def.advancement_requirement.is_some() && damage_grows_with_advancement(def) {
        TrapKind::Lure
    } else {
        TrapKind::Hand
    })
}

/// One trap install, from the step it arrived to the end of the game.
#[derive(Debug, Clone, Serialize)]
pub struct TrapInstall {
    pub card: CardId,
    pub kind: TrapKind,
    pub server: ServerId,
    pub ice_at_install: usize,
    /// ICE on its server when the Runner first accessed it; `None` if it
    /// never was.
    pub ice_at_first_access: Option<usize>,
    /// Turned face up before the Runner ever accessed it — the giveaway.
    pub rezzed_before_access: bool,
    pub peak_tokens: u32,
    pub accesses: usize,
    pub damage: usize,
    pub trashed_on_access: bool,
}

/// What one game showed.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GameTrap {
    pub game: u32,
    pub matchup: String,
    pub corp_won: bool,
    pub flatline: bool,
    /// The flatline came while a trap's damage was resolving.
    pub flatline_by_trap: bool,
    pub installs: Vec<TrapInstall>,
    /// Hand traps accessed where they were never installed: HQ, R&D,
    /// Archives.
    pub hand_accesses: [usize; 3],
    /// Of those, the ones where the Corp held the credits to spring it:
    /// a hand trap is a paid interaction, and one met by a Corp that
    /// cannot pay is a card the Runner simply walks past.
    pub hand_affordable: usize,
    pub hand_damage: usize,
    pub remote_runs: usize,
    pub empty_remote_runs: usize,
    /// Runs on a remote whose root was nothing but traps the Runner had
    /// already seen.
    pub known_trap_remote_runs: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct KindSummary {
    pub installs_per_game: f64,
    pub rezzed_before_access: f64,
    /// Share of installs with at least one piece of ICE in front when it
    /// went down.
    pub iced_at_install: f64,
    /// Share of installs the Runner ever accessed, and the mean ICE in
    /// front at that first access.
    pub accessed: f64,
    pub mean_ice_at_first_access: f64,
    pub mean_peak_tokens: f64,
    /// Accesses past the first, per install: running into a trap already
    /// sprung.
    pub repeat_accesses_per_install: f64,
    pub trashed_on_access: f64,
    pub damage_per_game: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrapReport {
    pub corp: String,
    pub runner: String,
    pub games: u32,
    pub seed: u64,
    pub matchup: Option<String>,
    pub corp_win_share: f64,
    pub flatline_share: f64,
    pub flatline_by_trap_share: f64,
    pub lure: KindSummary,
    pub hand: KindSummary,
    /// Hand traps accessed out of HQ, R&D and Archives, per game.
    pub hand_accesses_per_game: [f64; 3],
    pub hand_affordable_per_game: f64,
    pub hand_damage_per_game: f64,
    pub remote_runs_per_game: f64,
    pub empty_remote_runs_per_game: f64,
    pub known_trap_remote_runs_per_game: f64,
    pub per_game: Vec<GameTrap>,
}

pub fn run(args: &TrapArgs, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::sample_deck_registry();
    let matchups = select(core_decks::matchups(), args.matchup.as_deref(), &registry)?;
    let pool = match args.threads {
        Some(threads) => rayon::ThreadPoolBuilder::new().num_threads(threads).build()?,
        None => rayon::ThreadPoolBuilder::new().build()?,
    };
    let styled = |spec| if args.deck_styles { format!("{}, deck styles", describe(spec)) } else { describe(spec) };
    let corp = styled(args.corp);
    let runner = styled(args.runner);
    println!(
        "playing {} games over {} matchups, {corp} Corp vs {runner} Runner, seed {}...",
        args.games,
        matchups.len(),
        args.seed
    );

    let mut games: Vec<GameTrap> = pool.install(|| {
        (0..args.games)
            .into_par_iter()
            .map(|game| play(game, args, &registry, &matchups, config))
            .collect::<Result<Vec<_>, String>>()
    })?;
    games.sort_by_key(|game| game.game);

    let report = summarise(corp, runner, args, games);
    print_report(&report);
    if let Some(path) = &args.report {
        fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("\nreport written to {}", path.display());
    }
    Ok(())
}

type Matchup = (core_decks::DeckFile, core_decks::DeckFile);

fn select(all: Vec<Matchup>, matchup: Option<&str>, registry: &CardRegistry) -> Result<Vec<Matchup>, String> {
    let chosen: Vec<Matchup> = match matchup {
        Some(matchup) => {
            let (corp, runner) = matchup.split_once('/').ok_or("--matchup is CORP_DECK/RUNNER_DECK")?;
            all.into_iter().filter(|(c, r)| c.id == corp && r.id == runner).collect()
        }
        None => all
            .into_iter()
            .filter(|(corp, _)| {
                corp.cards.iter().any(|entry| registry.get(&entry.card).is_some_and(|def| trap_kind(def).is_some()))
            })
            .collect(),
    };
    if chosen.is_empty() {
        return Err(match matchup {
            Some(matchup) => format!("no sample matchup {matchup}"),
            None => "no sample Corp deck carries a trap".to_string(),
        });
    }
    Ok(chosen)
}

fn play(
    game: u32,
    args: &TrapArgs,
    registry: &CardRegistry,
    matchups: &[Matchup],
    config: &Config,
) -> Result<GameTrap, String> {
    let seed = args.seed.wrapping_add(u64::from(game));
    let (corp_deck, runner_deck) = &matchups[game as usize % matchups.len()];
    let (state, _events) =
        GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), registry, seed).map_err(|e| format!("{e:?}"))?;
    let style = |spec: BotSpec, deck: &core_decks::DeckFile| -> Result<Personality, String> {
        if args.deck_styles { Personality::for_deck(deck) } else { Ok(spec.personality) }
    };
    let setup = |personality: Personality| bots::AgentSetup {
        simulations: args.simulations,
        determinizations: args.determinizations,
        personality,
        ..bots::AgentSetup::new(args.simulations)
    };
    let corp_setup = setup(style(args.corp, corp_deck)?);
    let runner_setup = setup(style(args.runner, runner_deck)?);
    let corp = bots::make_seat_agent(args.corp.level, args.corp.kind, Side::Corp, seed, corp_setup, &config.model)?
        .ok_or("the Corp seat must be a bot that can take one")?;
    let runner = bots::make_seat_agent(
        args.runner.level,
        args.runner.kind,
        Side::Runner,
        seed.wrapping_add(1),
        runner_setup,
        &config.model,
    )?
    .ok_or("the Runner seat must be a bot that can take one")?;
    let mut session = Session::new(state, registry.clone(), Seat::Agent(corp), Seat::Agent(runner));

    let mut tracker = Tracker::default();
    tracker.game.game = game;
    tracker.game.matchup = format!("{}_vs_{}", corp_deck.id, runner_deck.id);
    tracker.board(session.state(), registry);
    while let SessionStep::Applied { .. } = session.step() {
        let events = session.last_entry().map(|entry| entry.events.clone()).unwrap_or_default();
        tracker.step(session.state(), registry, &events);
    }
    let end = session.state();
    let mut result = tracker.finish();
    result.corp_won = matches!(end.phase, GamePhase::GameOver(Side::Corp));
    Ok(result)
}

/// The trap accessed most recently in the run in progress, which is who
/// a Corp's damage belongs to until another card is accessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Springing {
    Installed(InstallId),
    Hand,
}

#[derive(Default)]
struct Tracker {
    game: GameTrap,
    seen: BTreeSet<InstallId>,
    traps: BTreeMap<InstallId, TrapInstall>,
    /// Installs the Runner has accessed or seen face up.
    known: BTreeSet<InstallId>,
    springing: Option<Springing>,
}

impl Tracker {
    /// New installs, face-up flips and token counts, off the board as it
    /// stands after a step.
    fn board(&mut self, state: &GameState, registry: &CardRegistry) {
        for card in &state.corp.installed {
            if self.seen.insert(card.install_id)
                && card.slot == InstallSlot::Root
                && let Some(kind) = registry.get(&card.card).and_then(trap_kind)
            {
                self.traps.insert(
                    card.install_id,
                    TrapInstall {
                        card: card.card.clone(),
                        kind,
                        server: card.server,
                        ice_at_install: ice_on(state, card.server),
                        ice_at_first_access: None,
                        rezzed_before_access: false,
                        peak_tokens: 0,
                        accesses: 0,
                        damage: 0,
                        trashed_on_access: false,
                    },
                );
            }
            if let Some(trap) = self.traps.get_mut(&card.install_id) {
                trap.peak_tokens = trap.peak_tokens.max(card.advancement_tokens);
                if card.rezzed {
                    if trap.accesses == 0 {
                        trap.rezzed_before_access = true;
                    }
                    self.known.insert(card.install_id);
                }
            }
        }
    }

    fn step(&mut self, state: &GameState, registry: &CardRegistry, events: &[GameEvent]) {
        for event in events {
            self.event(state, registry, event);
        }
        self.board(state, registry);
        if state.active_run.is_none() {
            self.springing = None;
        }
    }

    fn event(&mut self, state: &GameState, registry: &CardRegistry, event: &GameEvent) {
        match event {
            GameEvent::RunInitiated { server } => {
                self.springing = None;
                if matches!(server, ServerId::Remote(_)) {
                    self.game.remote_runs += 1;
                    let root: Vec<_> =
                        state.corp.installed.iter().filter(|c| c.server == *server && c.slot == InstallSlot::Root).collect();
                    if root.is_empty() {
                        self.game.empty_remote_runs += 1;
                    } else if root.iter().all(|c| self.traps.contains_key(&c.install_id) && self.known.contains(&c.install_id)) {
                        self.game.known_trap_remote_runs += 1;
                    }
                }
            }
            GameEvent::CardAccessed { card, server, install } => {
                self.springing = None;
                if let Some(id) = install
                    && let Some(trap) = self.traps.get_mut(id)
                {
                    if trap.accesses == 0 {
                        trap.ice_at_first_access = Some(ice_on(state, *server));
                    }
                    trap.accesses += 1;
                    self.known.insert(*id);
                    self.springing = Some(Springing::Installed(*id));
                } else if install.is_none() && registry.get(card).and_then(trap_kind) == Some(TrapKind::Hand) {
                    let slot = match server {
                        ServerId::Hq => Some(0),
                        ServerId::RnD => Some(1),
                        ServerId::Archives => Some(2),
                        ServerId::Remote(_) => None,
                    };
                    if let Some(slot) = slot {
                        self.game.hand_accesses[slot] += 1;
                        let price = registry
                            .get(card)
                            .and_then(|def| def.interactive_on_access.as_ref())
                            .map_or(0, |access| match access.cost { netrunner_core::dsl::Cost::Credits(n) => n, _ => 0 });
                        if state.corp.resources.credits.0 >= price {
                            self.game.hand_affordable += 1;
                        }
                    }
                    self.springing = Some(Springing::Hand);
                }
            }
            GameEvent::DamageTaken { amount, responsible: Some(Side::Corp), .. } => match self.springing {
                Some(Springing::Installed(id)) => {
                    if let Some(trap) = self.traps.get_mut(&id) {
                        trap.damage += amount;
                    }
                }
                Some(Springing::Hand) => self.game.hand_damage += amount,
                None => {}
            },
            GameEvent::CardTrashedFromAccess { card, .. } => {
                if let Some(Springing::Installed(id)) = self.springing
                    && let Some(trap) = self.traps.get_mut(&id)
                    && trap.card == *card
                {
                    trap.trashed_on_access = true;
                }
            }
            GameEvent::RunnerFlatlined => {
                self.game.flatline = true;
                self.game.flatline_by_trap = self.springing.is_some();
            }
            _ => {}
        }
    }

    fn finish(mut self) -> GameTrap {
        self.game.installs = self.traps.into_values().collect();
        self.game
    }
}

fn ice_on(state: &GameState, server: ServerId) -> usize {
    state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Ice).count()
}

fn summarise_kind(games: &[GameTrap], kind: TrapKind) -> KindSummary {
    let n = games.len().max(1) as f64;
    let installs: Vec<&TrapInstall> =
        games.iter().flat_map(|game| game.installs.iter()).filter(|install| install.kind == kind).collect();
    let count = installs.len().max(1) as f64;
    let share = |test: &dyn Fn(&TrapInstall) -> bool| installs.iter().filter(|i| test(i)).count() as f64 / count;
    let accessed: Vec<usize> = installs.iter().filter_map(|i| i.ice_at_first_access).collect();
    KindSummary {
        installs_per_game: installs.len() as f64 / n,
        rezzed_before_access: share(&|i| i.rezzed_before_access),
        iced_at_install: share(&|i| i.ice_at_install > 0),
        accessed: share(&|i| i.accesses > 0),
        mean_ice_at_first_access: accessed.iter().sum::<usize>() as f64 / accessed.len().max(1) as f64,
        mean_peak_tokens: installs.iter().map(|i| f64::from(i.peak_tokens)).sum::<f64>() / count,
        repeat_accesses_per_install: installs.iter().map(|i| i.accesses.saturating_sub(1)).sum::<usize>() as f64 / count,
        trashed_on_access: share(&|i| i.trashed_on_access),
        damage_per_game: installs.iter().map(|i| i.damage).sum::<usize>() as f64 / n,
    }
}

fn summarise(corp: String, runner: String, args: &TrapArgs, games: Vec<GameTrap>) -> TrapReport {
    let n = games.len().max(1) as f64;
    let mean = |get: &dyn Fn(&GameTrap) -> usize| games.iter().map(get).sum::<usize>() as f64 / n;
    let share = |test: &dyn Fn(&GameTrap) -> bool| games.iter().filter(|g| test(g)).count() as f64 / n;
    TrapReport {
        corp,
        runner,
        games: args.games,
        seed: args.seed,
        matchup: args.matchup.clone(),
        corp_win_share: share(&|g| g.corp_won),
        flatline_share: share(&|g| g.flatline),
        flatline_by_trap_share: share(&|g| g.flatline_by_trap),
        lure: summarise_kind(&games, TrapKind::Lure),
        hand: summarise_kind(&games, TrapKind::Hand),
        hand_accesses_per_game: [
            mean(&|g| g.hand_accesses[0]),
            mean(&|g| g.hand_accesses[1]),
            mean(&|g| g.hand_accesses[2]),
        ],
        hand_affordable_per_game: mean(&|g| g.hand_affordable),
        hand_damage_per_game: mean(&|g| g.hand_damage),
        remote_runs_per_game: mean(&|g| g.remote_runs),
        empty_remote_runs_per_game: mean(&|g| g.empty_remote_runs),
        known_trap_remote_runs_per_game: mean(&|g| g.known_trap_remote_runs),
        per_game: games,
    }
}

fn describe(spec: BotSpec) -> String {
    match spec.level {
        Some(level) => format!("level:{level:?}:{:?}", spec.personality).to_lowercase(),
        None => format!("{:?}:{:?}", spec.kind, spec.personality).to_lowercase(),
    }
}

fn print_kind(name: &str, k: &KindSummary) {
    println!("  {name}");
    println!("    installed / game          {:.2}", k.installs_per_game);
    println!("    rezzed before any access  {:.3}", k.rezzed_before_access);
    println!("    ICE in front at install   {:.3}", k.iced_at_install);
    println!("    ever accessed             {:.3}   mean ICE at first access {:.2}", k.accessed, k.mean_ice_at_first_access);
    println!("    mean peak tokens          {:.2}", k.mean_peak_tokens);
    println!("    repeat accesses / install {:.3}", k.repeat_accesses_per_install);
    println!("    trashed on access         {:.3}", k.trashed_on_access);
    println!("    damage dealt / game       {:.2}", k.damage_per_game);
}

fn print_report(r: &TrapReport) {
    println!("\nCorp ({}) against {}, {} games", r.corp, r.runner, r.games);
    println!(
        "  Corp win share {:.3}   flatlines {:.3}   of them while a trap resolved {:.3}",
        r.corp_win_share, r.flatline_share, r.flatline_by_trap_share
    );
    print_kind("lure traps (advanceable: Urtica Cipher)", &r.lure);
    print_kind("hand traps installed (Snare!, Byte!)", &r.hand);
    println!(
        "  hand traps accessed / game   HQ {:.2}  R&D {:.2}  Archives {:.2}   Corp could pay {:.2}   damage / game {:.2}",
        r.hand_accesses_per_game[0],
        r.hand_accesses_per_game[1],
        r.hand_accesses_per_game[2],
        r.hand_affordable_per_game,
        r.hand_damage_per_game
    );
    println!(
        "  Runner remote runs / game {:.2}   on an empty remote {:.2}   on a remote of known traps {:.2}",
        r.remote_runs_per_game, r.empty_remote_runs_per_game, r.known_trap_remote_runs_per_game
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardType, DamageType, Effect, Subject, Trigger, TriggeredEffect};
    use netrunner_core::rules::InstalledCard;

    fn urtica() -> CardDefinition {
        CardDefinition {
            id: CardId("urtica".to_string()),
            card_type: CardType::Asset,
            advancement_requirement: Some(0),
            triggers: vec![TriggeredEffect {
                subject: Some(Subject::This),
                when: None,
                acts_on_subject: false,
                first_each_turn: false,
                text: None,
                trigger: Trigger::OnAccessed,
                effects: vec![Effect::DealDamageAmount(DamageType::Net, netrunner_core::dsl::Amount::HostedAdvancementTokens)],
                requirement: None,
            }],
            ..CardDefinition::default()
        }
    }

    fn install(n: u32, server: ServerId) -> InstalledCard {
        InstalledCard {
            card: CardId("urtica".to_string()),
            install_id: InstallId(n),
            server,
            slot: InstallSlot::Root,
            ..Default::default()
        }
    }

    /// A trap is counted once from the step it arrives; an access makes it
    /// known, the damage after it is its own, and a second run at a remote
    /// of nothing but known traps is counted as the run into one.
    #[test]
    fn a_sprung_trap_is_known_and_a_run_back_into_it_is_counted() {
        let registry = CardRegistry::from_cards(vec![urtica()]);
        assert_eq!(trap_kind(&urtica()), Some(TrapKind::Lure));
        let mut state = GameState::new(0);
        state.corp.installed = vec![install(1, ServerId::Remote(0))];
        let mut tracker = Tracker::default();
        tracker.board(&state, &registry);
        assert_eq!(tracker.traps.len(), 1);

        let run = GameEvent::RunInitiated { server: ServerId::Remote(0) };
        let access =
            GameEvent::CardAccessed { card: CardId("urtica".to_string()), server: ServerId::Remote(0), install: Some(InstallId(1)) };
        let damage = GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2, responsible: Some(Side::Corp) };
        tracker.step(&state, &registry, &[run.clone(), access.clone(), damage]);
        assert_eq!(tracker.game.known_trap_remote_runs, 0, "the first run could not have known");
        tracker.step(&state, &registry, &[run, access]);
        assert_eq!(tracker.game.known_trap_remote_runs, 1);

        let game = tracker.finish();
        assert_eq!(game.remote_runs, 2);
        assert_eq!(game.installs[0].accesses, 2);
        assert_eq!(game.installs[0].damage, 2);
        assert!(!game.installs[0].rezzed_before_access);
    }
}
