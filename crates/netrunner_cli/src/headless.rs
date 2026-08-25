//! Non-rendered self-play mode: runs `--games` Kate-vs-HB games through a
//! `netrunner_server::MatchSession` with both sides `PlayerSlot::Bot`
//! (`--corp`/`--runner` picking each side's `netrunner_bots::BotAgent`,
//! both `Random` by default — matching this mode's original behavior),
//! asserting every game reaches `GamePhase::GameOver`.

use netrunner_core::rules::{GamePhase, GameState, Side};
use netrunner_server::{MatchSession, PlayerSlot};

use crate::bots;
use crate::config::{BotKind, Config};
use crate::decks;

/// A `Human` agent can't drive a headless game forward on its own, so it
/// falls back to `Random` here — see `config::Config::corp`'s doc comment.
fn headless_kind(kind: BotKind) -> BotKind {
    if matches!(kind, BotKind::Human) {
        BotKind::Random
    } else {
        kind
    }
}

pub async fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = decks::kate_vs_hb_decks();
    let base_seed = config.seed.unwrap_or_else(rand::random);

    for game_index in 0..config.games {
        let seed = base_seed.wrapping_add(u64::from(game_index));
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

        let corp_agent = bots::make_agent(headless_kind(config.corp), Side::Corp, seed).expect("headless_kind never resolves to BotKind::Human");
        let runner_agent = bots::make_agent(headless_kind(config.runner), Side::Runner, seed.wrapping_add(1))
            .expect("headless_kind never resolves to BotKind::Human");

        let session = MatchSession::new(state, registry.clone(), PlayerSlot::Bot(corp_agent), PlayerSlot::Bot(runner_agent));
        let final_state = session.run().await;
        if !matches!(final_state.phase, GamePhase::GameOver(_)) {
            return Err(format!("game {game_index} did not reach GameOver").into());
        }
    }

    println!("{} games completed without panics", config.games);
    Ok(())
}
