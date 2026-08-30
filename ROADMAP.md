# Netrunner Workspace Roadmap

Canonical single source of truth for engine mechanics, client-server infrastructure, single-player card data, and AI bot development across `netrunner_core`, `netrunner_card_sync`, `netrunner_server`, `netrunner_cli`, `netrunner_bots`, and `netrunner_gym`.

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
- [x] **Unified Card Model (`netrunner_core`) — DONE:** Collapsed the two previously-parallel card systems (`dsl::Card`/`CardRegistry`, the only thing that ever powered gameplay, and the separate NetrunnerDB-metadata-only `card::CardDefinition`/`catalog::CardCatalog`/`deck::Decklist`) into one: `dsl::Card` renamed to `CardDefinition` and extended with `numeric_id`/`faction`/`type_line`/`keywords`/`set_code`/`influence_cost`/`deck_limit`/`artist`/`image_url`/`is_playable`, all optional/defaulted except `is_playable: bool` (`false` for catalog-only entries with no DSL data, `true` for every hand-authored card). `CardRegistry` gained a `by_numeric_id` secondary index (`get_by_numeric_id`) plus `get_by_title`/`merge`, so NetrunnerDB-sourced data and hand-authored gameplay data now live in one pool, cross-referenceable by either id scheme. `catalog::CardCatalog` and the old `card::CardDefinition`/`card::CardType` are deleted; NetrunnerDB DTO→struct conversion moved to `cards::netrunnerdb` (also now infers `IceType` from keywords and captures `illustrator`/`deck_limit`, previously silently dropped). `deck::validator::validate_deck` and `netrunner_card_sync` (`NetrunnerDbSync::load_registry`/`sync_from_netrunnerdb`) now operate on `CardRegistry` directly instead of the old `CardCatalog`. `rules::deck::validate_deck` rejects any deck referencing an `is_playable: false` card (`RulesError::UnplayableCard`), so `GameState::setup` can never be handed a catalog-only card.
- [x] **Bundled Gateway & Elevation Core Sets (`netrunner_core`)**: Embed *System Gateway* and *Elevation* JSON fixtures directly into `netrunner_core` (`include_str!`) for zero-dependency offline availability while maintaining strict no-I/O crate boundaries.
- [x] **Dedicated Sync & Cache Crate (`netrunner_card_sync`)**: Establish a dedicated workspace crate (`netrunner_card_sync`) handling async network I/O, NetrunnerDB API v2 synchronization, and cross-platform disk caching without polluting `netrunner_core`. Also wired into `netrunner_cli cards {list-sets,sync}`.
- [x] **Cross-Platform Cache Directory Resolution**: Utilize standard cross-platform path resolution via `dirs::cache_dir()` in `netrunner_card_sync` to target OS-specific cache locations (`~/.cache/netrunner`, `~/Library/Caches/netrunner`, `%LOCALAPPDATA%\netrunner`) across Windows, macOS, and Linux.
- [ ] **Null Signal Games Format Support**: Enforce official [Null Signal Games Supported Formats](https://nullsignal.games/players/supported-formats/) (Startup, Standard, Eternal, Snapshot), including rotation tracking, card banlists, points restrictions, and legality checks prior to match start.

### 2. System Gateway Card Set (`netrunner_core`) — DONE
All 75 playable *System Gateway* cards are implemented, tested, and `is_playable: true`. They are authored JSON-only in `crates/netrunner_core/data/{corp,runner}/*.json` and reach every consumer through `cards::register_playable_cards` in the default build — see "JSON Card Loading" below. Coverage is enforced mechanically by `every_system_gateway_card_is_implemented_or_explicitly_excluded`, which requires each of the 77 printed cards to have either an implementation or an `SG_UNIMPLEMENTED` entry stating why. **That test is the gate for calling any future set complete** — *Elevation* is already embedded as catalog-only data and would need the same treatment.
- [x] **DSL primitives added, in dependency order:** foundational fixes (`CardDefinition::validate()`'s agenda-field bug, Upgrade installability, generalized `EffectRequirement::OncePerTurn`); new triggers (`OnRez`, `OnApproachServer`, `OnRunEnded`, `OnBasicDrawAction`, `OnAdvance`, `OnDiscardPhaseEnd`); the decision primitives (`Effect::OfferPaidChoice`/`PresentChoice`/`PromptChooseCards`/`PromptChooseServer` parking a `PendingPaidChoice`/`PendingDecision`, resolved by dedicated actions); `dsl::zone::{CardZoneRef,CardFilter}` for zone search and target selection; hosted-credit pools; MU and max-hand-size bonuses, console-singleton enforcement, conditional cost discounts; **hosting** for Trojan programs; **bioroid click-to-break** (`PlayerAction::BreakSubroutineWithClick`, `CardDefinition.click_breakable`); dynamic amounts (`Amount`) and conditional strength (`StrengthModifier` + `computed_strength`); persistent-after-trash upgrades; facedown-card tracking in Archives (`CorpState.archives: Vec<ArchivedCard>`, with a new masking rule — orientation and count are public, a facedown card's identity is hidden from the Runner); and remove-from-game (`Cost::RemoveSelfFromGame`, `CorpState.removed_from_game`). `ActionSpace::SIZE` grew 724 → 1024 as new player-facing decisions were added.
- [x] **Two identities are permanently out of scope**: *The Catalyst: Convention Breaker* and *The Syndicate: Profit over Principle* have `stripped_text: "Starter game only."` — no rules text exists to implement, so they stay `is_playable: false` indefinitely (they are the only two `SG_UNIMPLEMENTED` entries).
- [x] **Three reprints share ids with pre-existing baseline cards**: *Sure Gamble*, *Hedge Fund*, and *Cleaver* are System Gateway reprints of cards the baseline set already had — handled by keeping the single existing definition rather than authoring duplicates (`cards::sg_reprint_dedup_tests`). *Cleaver*'s pre-existing definition had its paid-ability costs transposed relative to the printed card; fixed as a data-correctness bugfix.
- [x] **Bot-driven sweep**: `no_panics_or_deadlocks_across_many_seeds_system_gateway` (`netrunner_single_player`) plays two all-real-card System Gateway decks, chosen for mechanic coverage rather than realism, across many seeds and both agent seatings. It is the only test that drives the whole mechanic surface through real agents instead of scripted `apply_action` calls, and it earned its keep immediately — see "Engine bugs found by the sweep" below.
- [x] **Engine bugs found by the sweep** (all three were reachable in ordinary play, none by any per-card test):
  - `current_actor` never accounted for `pending_paid_choice`/`pending_decision`, so a decision parked *while a paid-ability window was open* named the window's priority holder instead of the decision's chooser — leaving a player with no legal action at all. Its precedence now mirrors `apply_action`'s blocking guards exactly.
  - `Effect::PromptChooseServer` parked a choice resolvable only by `run::start_run`, without checking no run was already active — an unresolvable decision that blocked every action. It now fails the precondition at park time, so `legal_actions`' dry-run probe filters out the activating ability instead.
  - `ToggleCardSelection` enforced eligibility but not `max`, so a selection could grow past the bound `ConfirmCardSelection` requires, escapable only by toggling back down.

### 3. Deckbuilding & Single-Player Customization
- [ ] **Deck Import & Validation:** Parse client-provided decklists (NetrunnerDB IDs, JSON, or text formats). Enforce faction influence limits, deck size minimums, agenda point ratios, and identity constraints. `deck::validator::validate_deck` already enforces all of this against `CardRegistry` (influence, faction, set/pack legality, per-card `deck_limit`, agenda points) — what's still missing is an end-to-end "user pastes/uploads a decklist and starts a local match" flow; no code anywhere converts a validated `Decklist` into a `rules::Deck` `GameState::setup` can consume.
- [x] **Rules Engine & Complex Mechanics Expansion:** Generic counters (`Effect::AddCounters`/`RemoveCounters`, `InstalledCard`/`InstalledRunnerCard::counters`) and **hosting** (`InstalledRunnerCard.hosted_on_ice`, `PlayerAction::InstallProgramOnIce`, cascade-trash-on-unhost) are both done, the latter built specifically against System Gateway's Trojan programs (*Botulus*, *Tranquilizer*) once real cards needed it, per this section's own stated build-on-demand philosophy — see "System Gateway Card Set" below. Multi-stage trace attempts and paid-ability windows during run phases were already in place; remaining edge cases are tracked per-card as they're discovered, not as an open-ended item.

### 4. Core Engine Windows & Integrity (`netrunner_core`)
- [x] **Asynchronous Start-of-Turn Windows:** `enter_start_of_turn`/`end_turn` now pause at a `WindowCheckpoint::StartOfTurn`/`EndOfTurn` paid-ability window (closed via `PlayerAction::PassPriority`) instead of transitioning inline.
- [x] **Expanded Priority Checkpoints:** Priority windows now exist at start/end of turn as well as every run checkpoint. Post-action windows (after an ordinary click action) remain unimplemented.
- [ ] **Dynamic Discard Re-checks:** Dynamically evaluate hand size limits during `GamePhase::Discard` to account for mid-discard draws or hand-size modifications.
- [x] **Generalized Prevent/Replace Effects:** `Effect::DealDamage`/`TrashCard` can be prevented via a `PendingPrevention`/`WindowCheckpoint::Prevention` window (opened only when a matching `Paid` `PreventDamage`/`PreventTrash` ability is in play). The generic "one side may pay X or else Y happens" primitive predicted here now exists as `Effect::OfferPaidChoice`/`GameState.pending_paid_choice` (plus its sibling `Effect::PresentChoice`/`PendingDecision::ChooseEffect` for "choose 1 of N effects" and `PendingDecision::ChooseCards`/`ChooseServer` for zone-search/target-selection) — built once System Gateway's *Funhouse*/*Manegarm Skunkworks*/*Anoetic Void* actually needed it, confirming the earlier analysis that it wants a direct single-side-choice shape, not `PendingPrevention`'s two-side priority-passing window. See "System Gateway Card Set" below.
- [x] **JSON Card Loading — DONE:** Cards are authored one-per-file under `crates/netrunner_core/data/{corp,runner}/`, concatenated by `build.rs` at compile time and embedded via `include_str!` (`cards::embedded`), so `cards::register_playable_cards` serves every playable card to every consumer with no feature flag and no runtime I/O. The hardcoded Rust builders and the JSON-vs-Rust parity test are deleted — JSON is now the single source of truth. Printed metadata is joined from the embedded NetrunnerDB catalog on `numeric_id` rather than restated per card, guarded by a printed-value parity test (which caught a real bug on first run: *Malapert Data Vault* was rezzing for 0 instead of 1). `cards::load_registry_from_dirs`/`fs-loader` is re-scoped to external card directories (homebrew, custom sets, no-recompile iteration).

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
