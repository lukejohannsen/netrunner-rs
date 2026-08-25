# Netrunner Workspace Roadmap

Canonical single source of truth for engine mechanics, client-server infrastructure, single-player card data, and AI bot development across `netrunner_core`, `netrunner_server`, `netrunner_cli`, `netrunner_bots`, and `netrunner_gym`.

---

## 🚀 Shipped & Verified Milestones

### Core Engine & Rules Primitives (`netrunner_core`)
- [x] **Phase & Priority State Machines:** Integrated `GamePhase` state machine (`StartOfTurn`, `Action`, `Discard`, `GameOver`) and priority-based `PaidAbilityWindow` system (ICE approach/encounter, pre-access, and per-card access decisions).
- [x] **ICE Stack & Jack-out Windows:** Dynamic `RunIce` resolution from installed Corp cards and four Netrunner-compliant jack-out legality windows.
- [x] **Access Phase Plumbing:** Multi-card access selection (`SelectNextCard`), post-access decisions (`StealAgenda`, `TrashAccessedCard`, `PassAccessedCard`), automatic on-access triggers (`OnAccessed`, `OnTrashedFromAccess`), and interactive triggers (`PayToAvoidAccessTrigger`/`DeclineAccessTrigger`, e.g., *Fetal AI*).
- [x] **Deck-Out & Victory Resolution:** Start-of-turn deck-out checks, agenda point victory detection via `CardRegistry`, and public Heap/Archives tracking.
- [x] **Icebreaker & Economy Primitives:** Rig state with `Encounter`/`Turn` strength buff tracking, subtype-gated subroutine breaking (`restrict_to: Option<IceType>`), and `OnPlay` event/operation economy boosters (*Sure Gamble*, *Hedge Fund*).
- [x] **Self-Reference Card Triggers:** `CardTarget::ThisCard`/`Cost::TrashSelf` dynamic resolution for paid abilities and self-trashing access traps.

### Network, Masking & Client Architecture (`netrunner_server` & `netrunner_cli`)
- [x] **Client-Server Architecture & Masked State (`ClientView`)**
  - Perspective masking and fog-of-war for Corp/Runner hidden information.
  - State leak prevention ensuring game integrity for human and bot players.
- [x] **Transport-Agnostic Channel Layer**
  - Decoupled `ClientMessage` and `ServerMessage` definitions for uniform `MatchSession` execution.
- [x] **Real WebSocket Transport (`tokio-tungstenite`)**
  - Standalone daemon mode (`netrunner_server --serve`) with TCP upgrade and WebSocket streaming.
  - Remote client mode (`netrunner_cli --mode remote`) with `Connect` handshakes, two-human lobby pairing, and embedded bot hosting.
  - Verified live TUI socket driving and clean client/server channel bridges.

---

## 📇 Phase 1: Single-Player Engine Completeness & Card Data (Immediate Focus)

### 1. Card Data, Storage & NetrunnerDB Ingestion
- [ ] **Dynamic Card Storage & Schemas:** Standardize card representation, metadata, and rules text schemas inside `netrunner_core` to support dynamic updates, serialization, and offline local caching.
- [ ] **NetrunnerDB Sync Pipeline:** Build an ingestion client to fetch card definitions, printings, and set structures directly from NetrunnerDB JSON APIs without requiring hardcoded Rust fixture registries.
- [ ] **Null Signal Games Format Support:** Enforce official [Null Signal Games Supported Formats](https://nullsignal.games/players/supported-formats/) (Startup, Standard, Eternal, Snapshot), including rotation tracking, card banlists, points restrictions, and legality checks prior to match start.

### 2. Deckbuilding & Single-Player Customization
- [ ] **Deck Import & Validation:** Parse client-provided decklists (NetrunnerDB IDs, JSON, or text formats). Enforce faction influence limits, deck size minimums, agenda point ratios, and identity constraints.
- [ ] **Rules Engine & Complex Mechanics Expansion:** Iteratively expand `netrunner_core` to support multi-stage trace attempts, complex paid-ability windows during run phases, hosting mechanics, and intricate subroutine edge cases.

### 3. Core Engine Windows & Integrity (`netrunner_core`)
- [ ] **Asynchronous Start-of-Turn Windows:** Refactor `enter_start_of_turn` into a yielding state machine for start-of-turn paid ability windows and interactive triggers.
- [ ] **Expanded Priority Checkpoints:** Expand priority window checkpoints beyond runs to include start/end of turn and post-action windows.
- [ ] **Dynamic Discard Re-checks:** Dynamically evaluate hand size limits during `GamePhase::Discard` to account for mid-discard draws or hand-size modifications.

---

## 🧠 Phase 2: Bot Intelligence, Replay Infrastructure & Gym Harness

### 1. MCTS Determinization & Information Horizon
- [ ] **State Determinization in `netrunner_bots`:** Sample plausible hidden state distributions from a `ClientView` so `MctsAgent` can perform valid tree rollouts without unmasked state leaks or panics.

### 2. Action Replay Protocol & Match Logging
- [ ] **Structured Match Logging:** Create an append-only match log/replay format (JSON-Lines) emitted by `MatchSession`.
- [ ] **TUI Replay Viewer:** Add replay playback capabilities to `netrunner_cli` for post-match analysis and step-by-step review.

### 3. Dedicated Gym & Self-Play Harness (`netrunner_gym`)
- [ ] **`ClientView`-Driven Gym API:** Update `netrunner_gym`'s `reset()` / `step()` interface to consume masked `ClientView` states rather than omniscient `GameState`, enforcing fog-of-war during training.
- [ ] **Vectorized Feature Extraction:** Map game states, card types, and legal actions into dense tensor/numerical observations for machine learning models and Python bindings (via `pyo3`).
- [ ] **Headless Self-Play Benchmark Suite:** Build a multi-threaded headless runner in `netrunner_gym` to execute high-volume self-play matches between bot versions for Elo calibration and policy evaluation.

---

## 🏆 Phase 3: Bot Personalities, Elo Rating & Player Progression

### 1. Bot Personalities & Playstyle Archetypes
- [ ] **Configurable Bot Traits:** Define distinct bot archetypes (e.g., Fast-Rush Corp, Glacier/Late-Game Corp, Aggressive Runner, Trap/Net-Damage heavy).
- [ ] **Biased Evaluation Function:** Adjust heuristic values and tree-search node evaluations to reflect specific bot personality traits during play.

### 2. Rating & Elo Engine
- [ ] **Persistent Multi-Track Elo/Glicko-2 System:** Track separate ratings to reflect distinct competitive contexts:
  - **Human vs. Human Elo:** Official rank for competitive match play against real opponents.
  - **Human vs. Bot Elo:** Skill progression tracking for single-player practice sessions across difficulty tiers.
  - **Bot Benchmark Elo:** Internal performance tracking for comparing bot algorithms (e.g., MCTS vs. Heuristic) against one another.
- [ ] **Role-Specific Asymmetry:** Track independent ratings for **Corp** vs. **Runner** roles across all tracks (Human vs. Human, Human vs. Bot, and Bot vs. Bot) to reflect asymmetric skill mastery.

---

## 🌐 Phase 4: Network Resilience & Server Infrastructure

### 1. Reconnection & Session Recovery
- [ ] **Session Token Handshake:** Implement `session_token` reconnection logic, allowing dropped WebSocket clients to reconnect within $N$ seconds and re-sync state using a fresh `ClientView`.

### 2. Multi-Match Daemon & Matchmaking
- [ ] **Multi-Room Server Daemon:** Expand `netrunner_server` beyond single-slot lobby pairing into a multi-room server handling concurrent matches, spectator channels, and turn timers.
