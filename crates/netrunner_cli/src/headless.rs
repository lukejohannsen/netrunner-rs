//! Non-rendered self-play mode: runs `--games` Kate-vs-HB games, each
//! stepping through `legal_actions` picking uniformly at random, asserting
//! every game reaches `GamePhase::GameOver` within a finite tick budget
//! without panicking or stalling.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use netrunner_core::rules::{apply_action, legal_actions, GamePhase, GameState};

use crate::config::Config;
use crate::decks;

/// Generous relative to a real game: random self-play wastes many
/// clicks/passes a skilled player wouldn't, so this needs enough headroom
/// to still terminate legitimately rather than tripping the stall assert.
const MAX_TICKS: u32 = 10_000;

pub fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = decks::kate_vs_hb_decks();
    let base_seed = config.seed.unwrap_or_else(rand::random);

    for game_index in 0..config.games {
        let seed = base_seed.wrapping_add(u64::from(game_index));
        let (mut state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;
        let mut rng = StdRng::seed_from_u64(seed);

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

            let action = legal[rng.random_range(0..legal.len())].clone();
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
