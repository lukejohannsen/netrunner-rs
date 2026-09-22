//! `netrunner_cli diag fort`: *where* does the Corp put its ICE, and what
//! is in front of an agenda when it goes down?
//!
//! A report from play (Phase 5 §19): the `glacier` Corp "makes ICE for
//! days horizontally across as many servers as it can without ever once
//! putting down an agenda". What the person wanted instead is the fort
//! the profile is named for: HQ and R&D one to two deep, Archives at
//! least one, then one remote one to two deep, and *then* an agenda in it.
//! None of that is a win rate, and `diag tempo` counts installs by turn,
//! not by server, so neither could say whether the complaint was true or
//! whether a change answered it. This does.
//!
//! **Read off the state, not the actions.** After every applied step the
//! Corp's installs are compared with the ones already seen, by
//! `InstallId`. A card that is new this step is new wherever it came from
//! — an install action, a card's text, an install that trashed first — so
//! no action shape has to be recognised, and the ICE in front of an
//! agenda is counted as it stands the moment the agenda arrived.
//!
//! Reading `GameState` directly is allowed here for the reason in `diag`'s
//! module doc: a diagnostic is not a client.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::Serialize;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks as core_decks;
use netrunner_core::dsl::CardType;
use netrunner_core::rules::{GamePhase, GameState, InstallId, InstallSlot, ServerId, Side};
use netrunner_session::{Seat, Session, SessionStep};

use crate::bots;
use crate::config::{BotSpec, Config};
use netrunner_client::decks;

pub struct FortArgs {
    pub corp: BotSpec,
    pub runner: BotSpec,
    pub games: u32,
    pub seed: u64,
    pub simulations: usize,
    pub determinizations: Option<usize>,
    pub threads: Option<usize>,
    /// Play only this matchup, `CORP_DECK/RUNNER_DECK` by deck id — the
    /// games a person reported, rather than the pool they sit in.
    pub matchup: Option<String>,
    pub report: Option<PathBuf>,
}

/// What one game showed.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GameFort {
    pub game: u32,
    pub matchup: String,
    pub corp_won: bool,
    /// ICE on the agenda's own server at the moment each agenda was
    /// installed, in install order.
    pub ice_in_front_of_agendas: Vec<usize>,
    /// ICE on HQ, R&D and Archives when the first agenda was installed;
    /// `None` when none ever was.
    pub centrals_at_first_agenda: Option<[usize; 3]>,
    /// Remotes that ever held a card.
    pub remotes_opened: usize,
    /// Remotes that ever had a piece of ICE on them — the "horizontally
    /// across as many servers as it can" of the report, which
    /// `remotes_opened` cannot say because an asset needs a remote of its
    /// own whether or not anything protects it.
    pub iced_remotes: usize,
    /// Remote ICE installed over the game, wherever it ended up.
    pub remote_ice: usize,
    /// Central ICE installed over the game: HQ, R&D, Archives.
    pub central_ice: [usize; 3],
    pub agendas_scored: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FortReport {
    pub corp: String,
    pub runner: String,
    pub games: u32,
    pub seed: u64,
    pub matchup: Option<String>,
    pub corp_win_share: f64,
    pub agendas_installed_per_game: f64,
    pub agendas_scored_per_game: f64,
    /// Share of agenda installs with at least one piece of ICE in front,
    /// and with at least two.
    pub behind_one: f64,
    pub behind_two: f64,
    /// ICE in front of an agenda at install: count of installs by depth.
    pub depth_at_install: BTreeMap<usize, usize>,
    /// Of the games that installed an agenda, the share with all three
    /// centrals iced at the first one, and with none.
    pub all_centrals_at_first_agenda: f64,
    pub no_centrals_at_first_agenda: f64,
    pub remotes_opened_per_game: f64,
    pub iced_remotes_per_game: f64,
    pub remote_ice_per_game: f64,
    /// HQ, R&D, Archives.
    pub central_ice_per_game: [f64; 3],
    pub per_game: Vec<GameFort>,
}

pub fn run(args: &FortArgs, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::sample_deck_registry();
    let matchups = select(core_decks::matchups(), args.matchup.as_deref())?;
    let pool = match args.threads {
        Some(threads) => rayon::ThreadPoolBuilder::new().num_threads(threads).build()?,
        None => rayon::ThreadPoolBuilder::new().build()?,
    };
    let corp = describe(args.corp);
    let runner = describe(args.runner);
    println!("playing {} games, {corp} Corp vs {runner} Runner, seed {}...", args.games, args.seed);

    let mut games: Vec<GameFort> = pool.install(|| {
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

fn select(
    all: Vec<(core_decks::DeckFile, core_decks::DeckFile)>,
    matchup: Option<&str>,
) -> Result<Vec<(core_decks::DeckFile, core_decks::DeckFile)>, String> {
    let Some(matchup) = matchup else { return Ok(all) };
    let (corp, runner) = matchup.split_once('/').ok_or("--matchup is CORP_DECK/RUNNER_DECK")?;
    let chosen: Vec<_> = all.into_iter().filter(|(c, r)| c.id == corp && r.id == runner).collect();
    if chosen.is_empty() {
        return Err(format!("no sample matchup {corp} vs {runner}"));
    }
    Ok(chosen)
}

fn play(
    game: u32,
    args: &FortArgs,
    registry: &CardRegistry,
    matchups: &[(core_decks::DeckFile, core_decks::DeckFile)],
    config: &Config,
) -> Result<GameFort, String> {
    let seed = args.seed.wrapping_add(u64::from(game));
    let (corp_deck, runner_deck) = &matchups[game as usize % matchups.len()];
    let (state, _events) =
        GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), registry, seed).map_err(|e| format!("{e:?}"))?;
    let setup = |spec: BotSpec| bots::AgentSetup {
        simulations: args.simulations,
        determinizations: args.determinizations,
        personality: spec.personality,
        ..bots::AgentSetup::new(args.simulations)
    };
    let corp = bots::make_seat_agent(args.corp.level, args.corp.kind, Side::Corp, seed, setup(args.corp), &config.model)?
        .ok_or("the Corp seat must be a bot that can take one")?;
    let runner = bots::make_seat_agent(
        args.runner.level,
        args.runner.kind,
        Side::Runner,
        seed.wrapping_add(1),
        setup(args.runner),
        &config.model,
    )?
    .ok_or("the Runner seat must be a bot that can take one")?;
    let mut session = Session::new(state, registry.clone(), Seat::Agent(corp), Seat::Agent(runner));

    let mut fort = GameFort { game, matchup: format!("{}_vs_{}", corp_deck.id, runner_deck.id), ..GameFort::default() };
    let mut seen: BTreeSet<InstallId> = BTreeSet::new();
    let mut remotes = Remotes::default();
    watch(session.state(), registry, &mut seen, &mut remotes, &mut fort);
    while let SessionStep::Applied { .. } = session.step() {
        watch(session.state(), registry, &mut seen, &mut remotes, &mut fort);
    }
    let end = session.state();
    fort.corp_won = matches!(end.phase, GamePhase::GameOver(Side::Corp));
    fort.agendas_scored = end.corp.scored_agendas.len();
    fort.remotes_opened = remotes.opened.len();
    fort.iced_remotes = remotes.iced.len();
    Ok(fort)
}

/// Remote numbers seen: any card, and ICE.
#[derive(Default)]
struct Remotes {
    opened: BTreeSet<u32>,
    iced: BTreeSet<u32>,
}

/// Count every Corp install that was not there before this step.
fn watch(
    state: &GameState,
    registry: &CardRegistry,
    seen: &mut BTreeSet<InstallId>,
    remotes: &mut Remotes,
    fort: &mut GameFort,
) {
    for card in &state.corp.installed {
        if !seen.insert(card.install_id) {
            continue;
        }
        if let ServerId::Remote(n) = card.server {
            remotes.opened.insert(n);
            if card.slot == InstallSlot::Ice {
                remotes.iced.insert(n);
            }
        }
        let Some(def) = registry.get(&card.card) else { continue };
        match (&def.card_type, card.slot) {
            (CardType::Ice(_), InstallSlot::Ice) => match central_slot(card.server) {
                Some(slot) => fort.central_ice[slot] += 1,
                None => fort.remote_ice += 1,
            },
            (CardType::Agenda, _) => {
                fort.ice_in_front_of_agendas.push(ice_on(state, card.server));
                if fort.centrals_at_first_agenda.is_none() {
                    fort.centrals_at_first_agenda =
                        Some([ice_on(state, ServerId::Hq), ice_on(state, ServerId::RnD), ice_on(state, ServerId::Archives)]);
                }
            }
            _ => {}
        }
    }
}

fn ice_on(state: &GameState, server: ServerId) -> usize {
    state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Ice).count()
}

fn central_slot(server: ServerId) -> Option<usize> {
    match server {
        ServerId::Hq => Some(0),
        ServerId::RnD => Some(1),
        ServerId::Archives => Some(2),
        ServerId::Remote(_) => None,
    }
}

fn summarise(corp: String, runner: String, args: &FortArgs, games: Vec<GameFort>) -> FortReport {
    let n = games.len().max(1) as f64;
    let depths: Vec<usize> = games.iter().flat_map(|game| game.ice_in_front_of_agendas.iter().copied()).collect();
    let installs = depths.len().max(1) as f64;
    let mut depth_at_install = BTreeMap::new();
    for depth in &depths {
        *depth_at_install.entry(*depth).or_insert(0) += 1;
    }
    let firsts: Vec<[usize; 3]> = games.iter().filter_map(|game| game.centrals_at_first_agenda).collect();
    let with_first = firsts.len().max(1) as f64;
    let mean = |get: &dyn Fn(&GameFort) -> usize| games.iter().map(get).sum::<usize>() as f64 / n;
    FortReport {
        corp,
        runner,
        games: args.games,
        seed: args.seed,
        matchup: args.matchup.clone(),
        corp_win_share: games.iter().filter(|game| game.corp_won).count() as f64 / n,
        agendas_installed_per_game: depths.len() as f64 / n,
        agendas_scored_per_game: mean(&|game| game.agendas_scored),
        behind_one: depths.iter().filter(|depth| **depth >= 1).count() as f64 / installs,
        behind_two: depths.iter().filter(|depth| **depth >= 2).count() as f64 / installs,
        depth_at_install,
        all_centrals_at_first_agenda: firsts.iter().filter(|c| c.iter().all(|ice| *ice > 0)).count() as f64 / with_first,
        no_centrals_at_first_agenda: firsts.iter().filter(|c| c.iter().all(|ice| *ice == 0)).count() as f64 / with_first,
        remotes_opened_per_game: mean(&|game| game.remotes_opened),
        iced_remotes_per_game: mean(&|game| game.iced_remotes),
        remote_ice_per_game: mean(&|game| game.remote_ice),
        central_ice_per_game: [
            mean(&|game| game.central_ice[0]),
            mean(&|game| game.central_ice[1]),
            mean(&|game| game.central_ice[2]),
        ],
        per_game: games,
    }
}

fn describe(spec: BotSpec) -> String {
    match spec.level {
        Some(level) => format!("level:{level:?}:{:?}", spec.personality).to_lowercase(),
        None => format!("{:?}:{:?}", spec.kind, spec.personality).to_lowercase(),
    }
}

fn print_report(r: &FortReport) {
    println!("\nCorp ({}) against {}, {} games", r.corp, r.runner, r.games);
    println!("  Corp win share            {:.3}", r.corp_win_share);
    println!("  agendas installed / game  {:.2}   scored / game {:.2}", r.agendas_installed_per_game, r.agendas_scored_per_game);
    println!("  agenda installs behind ≥1 ICE {:.3}, ≥2 ICE {:.3}", r.behind_one, r.behind_two);
    let depths: Vec<String> = r.depth_at_install.iter().map(|(depth, count)| format!("{depth}: {count}")).collect();
    println!("  ICE in front at install   {}", depths.join(", "));
    println!(
        "  at the first agenda: all three centrals iced {:.3}, none {:.3}",
        r.all_centrals_at_first_agenda, r.no_centrals_at_first_agenda
    );
    println!(
        "  ICE installed / game      HQ {:.2}  R&D {:.2}  Archives {:.2}  remotes {:.2}",
        r.central_ice_per_game[0], r.central_ice_per_game[1], r.central_ice_per_game[2], r.remote_ice_per_game
    );
    println!("  remotes opened / game     {:.2}   with ICE {:.2}", r.remotes_opened_per_game, r.iced_remotes_per_game);
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardId, IceType};
    use netrunner_core::rules::InstalledCard;

    fn def(id: &str, card_type: CardType) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), card_type, ..CardDefinition::default() }
    }

    fn install(id: &str, n: u32, server: ServerId, slot: InstallSlot) -> InstalledCard {
        InstalledCard { card: CardId(id.to_string()), install_id: InstallId(n), server, slot, ..Default::default() }
    }

    /// An agenda is measured against the ICE standing when it arrived,
    /// and counted once however many steps it then sits through; ICE is
    /// counted by where it went.
    #[test]
    fn an_agenda_is_measured_against_the_ice_in_front_of_it_when_it_arrives() {
        let registry =
            CardRegistry::from_cards(vec![def("wall", CardType::Ice(IceType::Barrier)), def("plan", CardType::Agenda)]);
        let mut state = GameState::new(0);
        let (mut seen, mut remotes, mut fort) = (BTreeSet::new(), Remotes::default(), GameFort::default());
        state.corp.installed = vec![
            install("wall", 1, ServerId::Hq, InstallSlot::Ice),
            install("wall", 2, ServerId::Remote(0), InstallSlot::Ice),
            install("wall", 3, ServerId::Remote(0), InstallSlot::Ice),
        ];
        watch(&state, &registry, &mut seen, &mut remotes, &mut fort);
        state.corp.installed.push(install("plan", 4, ServerId::Remote(0), InstallSlot::Root));
        watch(&state, &registry, &mut seen, &mut remotes, &mut fort);
        state.corp.installed.push(install("wall", 5, ServerId::Remote(0), InstallSlot::Ice));
        watch(&state, &registry, &mut seen, &mut remotes, &mut fort);

        assert_eq!(fort.ice_in_front_of_agendas, vec![2]);
        assert_eq!(fort.centrals_at_first_agenda, Some([1, 0, 0]));
        assert_eq!(fort.central_ice, [1, 0, 0]);
        assert_eq!(fort.remote_ice, 3);
        assert_eq!(remotes.opened.len(), 1);
        assert_eq!(remotes.iced.len(), 1);
    }
}
