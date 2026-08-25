//! Integration tests for the baseline Core Set card suite, exercising
//! `register_baseline_set` through the full `engine::apply_action` pipeline
//! rather than calling `ability::evaluate_effect` directly — these are
//! whole-card behavior tests, not DSL unit tests.

use crate::cards::{register_baseline_set, CardRegistry};
use crate::dsl::CardId;
use crate::rules::{
    apply_action, AgendaPoints, Clicks, CorpState, Credits, GamePhase, GameState, InstallSlot, MemoryUnits,
    PlayerAction, PlayerResources, RulesError, RunnerState, ServerId, Side,
};

fn registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    register_baseline_set(&mut registry);
    registry
}

/// A bare `GameState`: no identity, empty zones, `Action(Corp)` — the same
/// starting point `GameState::new` provides. Every test below overrides
/// exactly the fields its scenario needs.
fn base_state() -> GameState {
    GameState {
        corp: CorpState {
            identity: None,
            bad_publicity: 0,
            first_install_used_this_turn: false,
            recurring_credits: 0,
            recurring_credits_max: 0,
            scored_agendas: Vec::new(),
            resources: PlayerResources { credits: Credits(10), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            hq: Vec::new(),
            r_and_d: Vec::new(),
            archives: Vec::new(),
            installed: Vec::new(),
        },
        runner: RunnerState {
            identity: None,
            scored_agendas: Vec::new(),
            resources: PlayerResources { credits: Credits(10), clicks: Clicks(4), agenda_points: AgendaPoints(0) },
            memory_units: MemoryUnits(10),
            brain_damage: 0,
            tags: 0,
            grip: Vec::new(),
            stack: Vec::new(),
            rig: Vec::new(),
            heap: Vec::new(),
            link_strength: 0,
            first_hq_run_used_this_turn: false,
            first_install_discount_used_this_turn: false,
        },
        phase: GamePhase::Action(Side::Corp),
        active_run: None,
        paid_ability_window: None,
        active_trace: None,
        seed: 0,
        rng_step: 0,
    }
}

/// Runs `InitiateRun { server }` through to full completion (`Success` ->
/// `CompleteRun` -> both sides pass priority -> access resolves), assuming
/// no ICE is installed on `server` — the common shape every test below that
/// needs a whole run (not just its setup) reuses. Returns every event
/// emitted along the way, in order.
fn run_to_completion(
    state: GameState,
    registry: &CardRegistry,
    server: ServerId,
) -> (GameState, Vec<crate::rules::GameEvent>) {
    let mut events = Vec::new();
    let (state, e) = apply_action(&state, registry, PlayerAction::InitiateRun { server }).expect("initiate run");
    events.extend(e);
    let (state, e) = apply_action(&state, registry, PlayerAction::ContinueRun).expect("continue run");
    events.extend(e);
    let (state, e) = apply_action(&state, registry, PlayerAction::CompleteRun).expect("complete run");
    events.extend(e);
    let (state, e) =
        apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner pass");
    events.extend(e);
    let (state, e) =
        apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp pass");
    events.extend(e);
    (state, events)
}

#[test]
fn gabriel_santiago_gains_two_credits_on_first_successful_hq_run_but_not_the_second() {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.runner.identity = Some(CardId("gabriel_santiago".to_string()));
    state.runner.resources.clicks = Clicks(4);
    state.runner.resources.credits = Credits(0);
    // HQ empty: a successful run against it completes immediately with
    // nothing to access, so each full run only costs the one click spent
    // by `InitiateRun` — no access decisions to resolve in between.

    let (state, _) = run_to_completion(state, &registry, ServerId::Hq);
    assert_eq!(state.runner.resources.credits, Credits(2), "first successful HQ run this turn should gain 2 credits");
    assert!(state.runner.first_hq_run_used_this_turn);

    let (state, _) = run_to_completion(state, &registry, ServerId::Hq);
    assert_eq!(
        state.runner.resources.credits,
        Credits(2),
        "a second successful HQ run the same turn should not gain another 2 credits"
    );
}

#[test]
fn haas_bioroid_engineering_the_future_gains_one_credit_on_first_install_but_not_the_second() {
    let registry = registry();
    let mut state = base_state();
    state.corp.identity = Some(CardId("haas_bioroid_engineering_the_future".to_string()));
    state.corp.resources.credits = Credits(10);
    state.corp.resources.clicks = Clicks(3);
    // Two copies of the same filler card in HQ — its own trigger (PAD
    // Campaign's OnTurnStart) never fires during an install, so it's inert
    // scenery for this test besides its printed cost.
    state.corp.hq = vec![CardId("pad_campaign".to_string()), CardId("pad_campaign".to_string())];

    let (state, events) = apply_action(
        &state,
        &registry,
        PlayerAction::InstallCard { card_id: CardId("pad_campaign".to_string()), zone: ServerId::Remote(0), slot: InstallSlot::Root },
    )
    .expect("first install should succeed");
    // Started at 10, paid 2 for PAD Campaign, gained 1 from the identity bonus.
    assert_eq!(state.corp.resources.credits, Credits(9));
    assert!(state.corp.first_install_used_this_turn);
    assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 1 })));

    let (state, _) = apply_action(
        &state,
        &registry,
        PlayerAction::InstallCard { card_id: CardId("pad_campaign".to_string()), zone: ServerId::Remote(1), slot: InstallSlot::Root },
    )
    .expect("second install should succeed");
    // Paid another 2 for the second PAD Campaign, no further bonus this turn.
    assert_eq!(state.corp.resources.credits, Credits(7));
}

#[test]
fn the_makers_eye_accesses_three_total_cards_from_rd() {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.runner.resources.clicks = Clicks(4);
    state.runner.resources.credits = Credits(5);
    state.runner.grip = vec![CardId("the_makers_eye".to_string())];
    state.corp.r_and_d = (0..5).map(|i| CardId(format!("rd_card_{i}"))).collect();

    let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("the_makers_eye".to_string()) })
        .expect("playing The Maker's Eye should initiate a run on R&D");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to success");
    let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("open access window");
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner pass");
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp pass");

    let access = state.active_run.as_ref().expect("run still parked awaiting access resolution").access_state.as_ref().expect("access state present");
    assert_eq!(access.unaccessed_cards.len(), 3, "1 base access + 2 additional from The Maker's Eye");
}

#[test]
fn account_siphon_replaces_hq_access_and_accesses_zero_cards() {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.corp.resources.credits = Credits(10);
    state.runner.resources.clicks = Clicks(4);
    state.runner.resources.credits = Credits(5);
    state.runner.grip = vec![CardId("account_siphon".to_string())];
    state.corp.hq = vec![CardId("hq_card_0".to_string()), CardId("hq_card_1".to_string())];

    let (state, events) = run_to_completion_after_playing(state, &registry, "account_siphon");

    assert_eq!(state.corp.resources.credits, Credits(5), "Corp should lose 5 credits");
    // Runner: started at 5, paid 2 to play Account Siphon, gained 10 from the siphon.
    assert_eq!(state.runner.resources.credits, Credits(13));
    assert_eq!(state.runner.tags, 2);
    assert_eq!(state.corp.hq.len(), 2, "HQ itself is untouched — access was replaced, not resolved");
    assert!(
        !events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardAccessed { .. })),
        "no card should have been accessed from HQ"
    );
}

/// Plays `card_id` (an Event already in the Runner's grip whose `OnPlay`
/// effects initiate a run on `server`), then drives that run to completion —
/// shared by `account_siphon`-shaped tests, where the run itself is started
/// by playing the card rather than a bare `InitiateRun` action.
fn run_to_completion_after_playing(
    state: GameState,
    registry: &CardRegistry,
    card_id: &str,
) -> (GameState, Vec<crate::rules::GameEvent>) {
    let mut events = Vec::new();
    let (state, e) =
        apply_action(&state, registry, PlayerAction::PlayEvent { card_id: CardId(card_id.to_string()) }).expect("play event");
    events.extend(e);
    let (state, e) = apply_action(&state, registry, PlayerAction::ContinueRun).expect("continue run");
    events.extend(e);
    let (state, e) = apply_action(&state, registry, PlayerAction::CompleteRun).expect("complete run");
    events.extend(e);
    let (state, e) =
        apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner pass");
    events.extend(e);
    let (state, e) =
        apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp pass");
    events.extend(e);
    (state, events)
}

#[test]
fn pad_campaign_gains_one_credit_at_the_start_of_the_corps_next_turn() {
    let registry = registry();
    let mut state = base_state();
    state.corp.hq = vec![CardId("pad_campaign".to_string())];
    // A non-empty R&D — an empty one would deck the Corp out the moment
    // their next turn's mandatory draw is attempted, which is exactly the
    // turn boundary this test needs to cross to see PAD Campaign fire.
    state.corp.r_and_d = vec![CardId("filler_card".to_string())];
    state.corp.resources.credits = Credits(10);
    state.corp.resources.clicks = Clicks(3);

    let (state, _) = apply_action(
        &state,
        &registry,
        PlayerAction::InstallCard {
            card_id: CardId("pad_campaign".to_string()),
            zone: ServerId::Remote(0),
            slot: InstallSlot::Root,
        },
    )
    .expect("install pad campaign");
    // Started at 10, paid 2 to install — unrezzed installs stay silent at
    // start of turn, so this must be rezzed for the trigger to fire below.
    assert_eq!(state.corp.resources.credits, Credits(8));
    let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice_id: CardId("pad_campaign".to_string()) })
        .expect("rez pad campaign");

    // Corp's own turn ending doesn't refire their own start-of-turn — only
    // the Runner's turn ending (advancing back to the Corp) does.
    let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
    let (state, events) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");

    assert_eq!(state.corp.resources.credits, Credits(9), "PAD Campaign should gain 1 credit at the start of the Corp's next turn");
    assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 1 })));
}

#[test]
fn snare_deals_three_net_damage_and_a_tag_when_accessed() {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.runner.resources.clicks = Clicks(4);
    state.runner.grip = (0..5).map(|i| CardId(format!("grip_card_{i}"))).collect();
    state.corp.installed = vec![crate::rules::InstalledCard {
        card: CardId("snare".to_string()),
        server: ServerId::Remote(0),
        slot: InstallSlot::Root,
        rezzed: false,
        advancement_tokens: 0,
    }];

    let (state, _) =
        apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("initiate run");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to success");
    let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("open access window");
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes access window");
    let (state, events) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes access window, resolving access");

    assert_eq!(state.runner.grip.len(), 2, "3 of the 5 grip cards should have been discarded to net damage");
    assert_eq!(state.runner.tags, 1);
    assert!(events.iter().any(|e| matches!(
        e,
        crate::rules::GameEvent::DamageTaken { damage_type: crate::dsl::DamageType::Net, amount: 3 }
    )));
    assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::TagsGiven { side: Side::Runner, amount: 1 })));
}

#[test]
fn cleaver_pumps_strength_and_breaks_up_to_two_barrier_subroutines() {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.runner.resources.clicks = Clicks(4);
    state.runner.resources.credits = Credits(10);
    state.runner.grip = vec![CardId("cleaver".to_string())];
    state.corp.installed = vec![crate::rules::InstalledCard {
        card: CardId("wall_of_static".to_string()),
        server: ServerId::Hq,
        slot: InstallSlot::Ice,
        rezzed: true,
        advancement_tokens: 0,
    }];

    let (state, _) = apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("cleaver".to_string()), memory_cost: 1 })
        .expect("install cleaver");
    assert_eq!(state.runner.resources.credits, Credits(7), "10 - 3 (Cleaver's install cost)");

    let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
    // `ApproachIce` opens a paid-ability window (Runner has priority first,
    // since it's their turn) — both sides must pass before the run commits
    // to `EncounterIce`, per `paid_ability`'s priority-passing rules.
    let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach wall of static");
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach window");
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes approach window, entering encounter");

    // Pump for 1c (well within Wall of Static's printed strength once
    // stacked with the break below, though not strictly required here since
    // Cleaver's base strength already matches it), then break its single
    // Barrier subroutine (well within Cleaver's up-to-2 cap) for 2c. Each
    // activation flips window priority to the Corp, who has nothing to
    // activate and passes back.
    let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { card_id: CardId("cleaver".to_string()), ability_index: 0 })
        .expect("pump strength");
    assert_eq!(state.runner.rig[0].effective_strength(), 4);
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes back to the runner");

    let (state, events) = apply_action(&state, &registry, PlayerAction::ActivateAbility { card_id: CardId("cleaver".to_string()), ability_index: 1 })
        .expect("break subroutines");

    assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { index: 0, .. })));
    assert_eq!(
        state.active_run.as_ref().unwrap().ice[0].subroutines[0].status,
        crate::rules::SubroutineStatus::Broken
    );
    assert_eq!(state.runner.resources.credits, Credits(4), "7 - 1 (pump) - 2 (break)");
}

#[test]
fn scorched_earth_requires_a_tagged_runner_and_deals_four_meat_damage() {
    let registry = registry();
    let mut state = base_state();
    state.corp.resources.clicks = Clicks(3);
    state.corp.resources.credits = Credits(10);
    state.corp.hq = vec![CardId("scorched_earth".to_string())];
    state.runner.grip = (0..5).map(|i| CardId(format!("grip_card_{i}"))).collect();

    let result = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("scorched_earth".to_string()) });
    assert_eq!(result, Err(RulesError::RunnerNotTagged));

    state.runner.tags = 1;
    let (state, events) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("scorched_earth".to_string()) })
        .expect("scorched earth should resolve against a tagged runner");
    assert_eq!(state.runner.grip.len(), 1, "4 of the 5 grip cards should have been discarded to meat damage");
    assert!(events.iter().any(|e| matches!(
        e,
        crate::rules::GameEvent::DamageTaken { damage_type: crate::dsl::DamageType::Meat, amount: 4 }
    )));
}
