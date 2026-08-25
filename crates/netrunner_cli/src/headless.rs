//! Non-rendered self-play mode: runs `--games` Kate-vs-HB games, each
//! stepping through `legal_actions` via `--corp-agent`/`--runner-agent`
//! (`netrunner_bots::BotAgent`s, both `Random` by default — matching this
//! mode's original inline-random behavior), asserting every game reaches
//! `GamePhase::GameOver` within a finite tick budget without panicking or
//! stalling.

use netrunner_bots::BotAgent;
use netrunner_core::rules::{apply_action, legal_actions, GamePhase, GameState, Side};

use crate::bots;
use crate::config::{BotKind, Config};
use crate::decks;

/// Generous relative to a real game: random self-play wastes many
/// clicks/passes a skilled player wouldn't, so this needs enough headroom
/// to still terminate legitimately rather than tripping the stall assert.
const MAX_TICKS: u32 = 10_000;

/// A `Human` agent can't drive a headless game forward on its own, so it
/// falls back to `Random` here — see `config::Config::corp_agent`'s doc
/// comment.
fn headless_kind(kind: BotKind) -> BotKind {
    if matches!(kind, BotKind::Human) {
        BotKind::Random
    } else {
        kind
    }
}

pub fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = decks::kate_vs_hb_decks();
    let base_seed = config.seed.unwrap_or_else(rand::random);

    for game_index in 0..config.games {
        let seed = base_seed.wrapping_add(u64::from(game_index));
        let (mut state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

        let mut corp_agent = bots::make_agent(headless_kind(config.corp_agent), Side::Corp, seed)
            .expect("headless_kind never resolves to BotKind::Human");
        let mut runner_agent = bots::make_agent(headless_kind(config.runner_agent), Side::Runner, seed.wrapping_add(1))
            .expect("headless_kind never resolves to BotKind::Human");

        let mut terminated = false;
        for _tick in 0..MAX_TICKS {
            if matches!(state.phase, GamePhase::GameOver(_)) {
                terminated = true;
                break;
            }

            let legal = legal_actions(&state, &registry);
            if legal.is_empty() {
                return Err(format!("game {game_index} stalled: legal_actions is empty but the game isn't over").into());
            }

            let Some(side) = bots::current_actor(&state) else {
                return Err(format!("game {game_index}: legal_actions is non-empty but current_actor found no pending decision").into());
            };
            let agent: &mut dyn BotAgent = match side {
                Side::Corp => corp_agent.as_mut(),
                Side::Runner => runner_agent.as_mut(),
            };

            let action = agent.select_action(&state, &registry, &legal);
            let (next, _events) = apply_action(&state, &registry, action.clone())
                .map_err(|error| format!("game {game_index}: apply_action rejected its own legal_actions output: {action:?} -> {error:?}"))?;
            state = next;
        }

        if !terminated {
            return Err(format!("game {game_index} did not reach GameOver within {MAX_TICKS} ticks").into());
        }
    }

    println!("{} games completed without panics", config.games);
    Ok(())
}
