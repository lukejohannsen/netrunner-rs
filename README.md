# netrunner-rs

A deterministic, data-driven engine for the **Netrunner** card game, written in Rust.

The rules engine is a pure state machine with no I/O, no async runtime, and no rendering
dependencies. Card behaviour is expressed as JSON parsed into a small DSL rather than hardcoded in
Rust, and everything else in the workspace — a terminal client, an authoritative server, bots, and
a reinforcement-learning environment — is a consumer of that one engine.

> **Disclaimer**
>
> This project is a free, open-source fan implementation of the Netrunner card game. It is not
> affiliated with, authorized by, or endorsed by Null Signal Games, Fantasy Flight Games, or
> Wizards of the Coast. All card art and text belong to their respective copyright holders.

---

## What it does

- **A pure, deterministic core.** Every state transition is
  `apply_action(state, registry, action) -> Result<(GameState, Vec<GameEvent>), RulesError>`.
  Handlers mutate a clone and return it on success, so a rejected action can never leave the game
  partially mutated — atomicity is structural, not something each handler has to remember.
- **Randomness lives inside the state.** `GameState` carries its own `seed` and `rng_step`, so
  `apply_action` stays a pure function of its two explicit inputs and replaying a recorded action
  history reproduces a bit-identical final state. No RNG is ever threaded in from outside.
- **Cards are data, not code.** Card files under `crates/netrunner_core/data/{corp,runner}/` are
  parsed into DSL primitives and embedded at compile time by `build.rs`. Adding a card normally
  means writing JSON, not Rust. All 75 playable *System Gateway* cards are implemented, with a
  test that fails if any printed card is neither implemented nor explicitly excluded.
- **Legality has exactly one definition.** `legal_actions` generates candidates and keeps only
  those `apply_action` actually accepts on a cloned state, so what a UI or bot is offered can
  never drift from what the engine permits.
- **Fog of war is enforced at the boundary.** A client receives a `ClientView` — a per-side masked
  projection carrying only that seat's legal actions — and never the real `GameState`. Hidden
  information is structural, not a matter of the client being polite about what it renders.
- **One match loop.** A single pull-shaped `Session` drives every mode: the terminal client pumps
  it synchronously, the server pumps it inside `tokio`, and the RL environment pumps it from
  Python. Sync versus async is a property of who pumps it, never a fork in rules flow.
- **Bots and training.** Random, heuristic, MCTS and PUCT agents all play from the same masked
  view a human gets, with an optional ONNX policy, a PyO3 gym environment over a fixed action
  space, and a self-play trajectory generator.

## Quick start

Requires a Rust toolchain supporting edition 2024 (1.85+).

```bash
# Play a local match in the terminal against a bot
cargo run -p netrunner_cli

# Run bot-vs-bot games with no UI
cargo run -p netrunner_cli -- --headless --games 20

# Network play: start the host, then connect a client from another terminal
cargo run -p netrunner_server -- --serve
cargo run -p netrunner_cli -- --mode remote

# Generate self-play training trajectories
cargo run -p netrunner_selfplay -- -n 10 -s 100 -o data/selfplay

# The gate for any change
cargo test --workspace
cargo clippy --workspace --all-targets
```

## Workspace layout

| Crate | Role |
|---|---|
| `netrunner_core` | Pure deterministic rules engine, card DSL, embedded card/deck data, masking. Everything else depends on this; it depends on nothing. |
| `netrunner_bots` | Automated players over a masked `ClientView`: `BotAgent`, random/heuristic/MCTS/PUCT agents, `determinize`, RL observation encoding, optional ONNX policy. |
| `netrunner_session` | The one match decision loop. `Session`, `Seat`, the single step budget, `MatchHistory`, and end-of-match classification. Every driver pumps this. |
| `netrunner_single_player` | Thin index-based adapter over `netrunner_session` for the RL / fixed-action-space path. |
| `netrunner_server` | Authoritative async host: `MatchSession`, the `ClientMessage`/`ServerMessage` protocol, WebSocket transport. |
| `netrunner_cli` | Reference client: ratatui TUI, headless runner, local and remote modes, card/deck subcommands. |
| `netrunner_gym` | PyO3 reinforcement-learning environment over the fixed action space. |
| `netrunner_selfplay` | High-volume self-play data generation for training. |
| `netrunner_card_sync` | Async NetrunnerDB API sync and cross-platform disk caching — the only crate doing network I/O for card data. |

## Documentation

- **[`ARCHITECTURE.md`](ARCHITECTURE.md)** — how the engine is shaped and why: the state and
  execution model, the blocking-guard precedence invariant, the privacy layers, and card data flow.
- **[`ROADMAP.md`](ROADMAP.md)** — the single source of truth for project status: what is done,
  what is open, what is next.
- **[`AGENTS.md`](AGENTS.md)** — contributing conventions: the decoupled-engine rule, the DSL
  growth rule, state hygiene, and the testing gates a change has to clear.

## Card data

Card metadata — titles, printed rules text, factions, influence, set information — comes from
**[NetrunnerDB](https://netrunnerdb.com/)**. The `netrunner_card_sync` crate pulls from its public
API v2, and the resulting catalogs are embedded under `crates/netrunner_core/data/cards/`. Card
files in this repository own only the rules-engine data plus the numeric id used to join against
that catalog; printed metadata is never restated per card.

NetrunnerDB is an independent community project and is not affiliated with this one.

## License

The source code in this repository is licensed under the
[GNU General Public License v3.0](LICENSE).

**The GPL covers this project's own source code only.** The card catalogs under
`crates/netrunner_core/data/cards/` are data dumps from [NetrunnerDB](https://netrunnerdb.com/)
and include card titles, rules text, flavor text and illustrator credits. **That content is not
covered by this license** and is not this project's to license — it remains the property of its
respective copyright holders, as stated in the disclaimer above.
