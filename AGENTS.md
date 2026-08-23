# AGENTS.md — AI Engineering Guidelines for Cyberpunk Netrunner Engine

This repository contains an asynchronous, turn-based Netrunner card game built in Rust using a modular, decoupled architecture.

## Architecture Guidelines

1. **Decoupled Engine Rule (`netrunner_core`)**:
   - `netrunner_core` MUST be a pure, deterministic Rust library.
   - It MUST NOT depend on `bevy`, `tokio`, or any I/O framework.
   - All state mutations are deterministic transitions: `(GameState, PlayerAction) -> Result<(GameState, Vec<Event>), RulesError>`.
   - Never hardcode card rules in Rust functions. Cards are data-driven JSON objects parsed into AST primitives defined in `netrunner_core::dsl`.

2. **Server Architecture (`netrunner_server`)**:
   - The server is an authoritative host process running `netrunner_core`.
   - It validates incoming client `PlayerAction` intents, updates the global state, and broadcasts state deltas (`GameStateEvent`) back to connected clients.
   - Fog of War / Hidden State MUST be enforced at the server layer (e.g., hidden Runner hand cards sent as `None` / masked state to the Corp player).

3. **Client Architecture (`netrunner_client`)**:
   - Built using **Bevy Engine**. The client is a **dumb renderer terminal**.
   - The UI listens for state deltas from the server or local state machine and presents them visually.
   - User inputs (drag card, click credit, run server) generate a `PlayerAction` event; they NEVER mutate game rules state directly.

4. **Testing & AI Gym (`netrunner_gym`)**:
   - Evaluates card balances and runs headless self-play bot rollouts via Monte Carlo Tree Search (MCTS) or heuristic AI agents over `netrunner_core`.

---

## Code Style & Conventions

- **State Immutability**: Prefer returning fresh updated states or using controlled mutation wrappers in `netrunner_core`.
- **Serde Serialization**: All `PlayerAction` and `GameStateEvent` enums must derive `Serialize` and `Deserialize`.
- **Error Handling**: Use explicit `Result<T, GameError>` return types over `panic!` or `unwrap()` in engine code.
- **Bevy Plugins**: Split client rendering code cleanly into modular Bevy plugins (`CardRenderPlugin`, `RunPhaseUIPlugin`, `NetworkClientPlugin`).

---

## Core Cargo Commands

### Build & Run
- `cargo build --workspace`: Build the entire monorepo.
- `cargo run -p netrunner_client`: Run the Bevy desktop game client.
- `cargo run -p netrunner_server`: Run the standalone headless server.
- `cargo run -p netrunner_gym`: Execute the AI Deck Testing harness.

### Testing & Quality
- `cargo test --workspace`: Run unit and integration tests across all crates.
- `cargo test -p netrunner_core`: Test engine rule logic and DSL card parsing.
- `cargo clippy --workspace`: Run the linter across the workspace.

---

## Claude Code Context Strategy

- When editing game engine rules or mechanics, operate strictly inside `crates/netrunner_core/`.
- When building UI components, visual card layouts, or drag-and-drop systems, restrict context to `crates/netrunner_client/`.
- Do not add visual rendering dependencies to `netrunner_core` or `netrunner_server`.
