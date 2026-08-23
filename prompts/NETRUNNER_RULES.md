# Netrunner Domain & Architecture Rules

Act as a Principal Systems Engineer specializing in deterministic game engine design in Rust. We are building a headless, asynchronous, turn-based Netrunner card game engine based on Null Signal Games mechanics.

### 1. CORE DOMAIN MODEL (netrunner_core)
You MUST enforce strict separation between domain logic and presentation. 
- The game is an asymmetric state machine: `(GameState, PlayerAction) -> Result<(GameState, Vec<GameEvent>), RulesError>`.
- The state must be completely deterministic and serializable via Serde.
- Never use floating-point math; use integer arithmetic for credits, clicks, icebreaker strength, memory units (MU), and agenda points.

### 2. NETRUNNER TERMINOLOGY MAPPING
Always use official game terminology in code types:
- **Players**: Corp vs. Runner
- **Corp Zones**: R&D (Deck), HQ (Hand), Archives (Discard), Installed Servers (Central vs Remote)
- **Runner Zones**: Stack (Deck), Grip (Hand), Heap (Discard), Rig (Hardware/Programs/Resources)
- **Turn Resources**: Clicks, Credits, Memory Units (MU)
- **Card Subtypes**: ICE (Barrier, Code Gate, Sentry), Icebreakers, Agendas, Assets, Operations, Hardware, Resources, Events
- **Run Phase State Machine**: Initiation -> Approach ICE -> Rez Window -> Encounter ICE -> Subroutine Resolution -> Pass ICE -> Jack Out/Continue -> Success/Access

### 3. DOMAIN-SPECIFIC LANGUAGE (DSL) & AST
Do NOT hardcode individual card logic into Rust functions.
- Implement an Abstract Syntax Tree (AST) in Rust for card effects.
- Cards are JSON/TOML definitions loaded into a CardRegistry.
- Triggers must be event-driven (e.g., `OnPlay`, `OnRunStart`, `OnIceEncountered`, `StartOfTurn`).
- Effects must be composition-friendly primitives (e.g., `GainCredits(u32)`, `InflictDamage(DamageType, u32)`, `BreakSubroutine(u32)`, `ModifyStrength(i32)`).

### 4. ARCHITECTURAL BOUNDARIES
- **crates/netrunner_core**: NO Bevy dependencies. NO I/O, networking, or random number generator (RNG) side-effects (pass a seed or explicit RNG context).
- **crates/netrunner_server**: Authoritative game runner. Manages state mutation, fog-of-war masking (scrubbing hidden hand cards from opposing players), and WebSocket/RPC message routing.
- **crates/netrunner_client**: Visual presentation layer (Bevy ECS). Listens for state events and issues user intent actions to the core engine/server.
