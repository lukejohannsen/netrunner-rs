//! Integration tests for the baseline Core Set card suite, exercising
//! `register_playable_cards` through the full `engine::apply_action` pipeline
//! rather than calling `ability::evaluate_effect` directly — these are
//! whole-card behavior tests, not DSL unit tests.

use crate::cards::{register_playable_cards, CardRegistry};
use crate::dsl::CardId;
use crate::rules::{
    ArchivedCard,
    apply_action, AgendaPoints, Clicks, CorpState, Credits, GamePhase, GameState, InstallId, InstallSlot, MemoryUnits,
    PlayerAction, PlayerResources, RulesError, RunnerState, ServerId, Side,
};
use crate::rules::test_support::{fixture_install_id, install_of, position_of};

fn registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    register_playable_cards(&mut registry);
    registry
}

/// See `rules::turn::tests::close_all_windows`'s doc comment — same helper,
/// duplicated here since that one lives in a private `mod tests`.
fn close_all_windows(mut state: GameState, registry: &CardRegistry) -> (GameState, Vec<crate::rules::GameEvent>) {
    let mut events = Vec::new();
    while let Some(window) = &state.paid_ability_window {
        let side = window.active_priority;
        let (next, ev) =
            apply_action(&state, registry, PlayerAction::PassPriority { side }).expect("pass priority should succeed");
        state = next;
        events.extend(ev);
    }
    (state, events)
}

/// A bare `GameState`: no identity, empty zones, `Action(Corp)` — the same
/// starting point `GameState::new` provides. Every test below overrides
/// exactly the fields its scenario needs.
fn base_state() -> GameState {
    GameState {
        corp: CorpState {
            resources: PlayerResources { credits: Credits(10), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            ..Default::default()
        },
        runner: RunnerState {
            resources: PlayerResources { credits: Credits(10), clicks: Clicks(4), agenda_points: AgendaPoints(0) },
            memory_units: MemoryUnits(10),
            ..Default::default()
        },
        phase: GamePhase::Action(Side::Corp),
        ..Default::default()
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
    // Started at 10, installing an asset is free, gained 1 from the identity bonus.
    assert_eq!(state.corp.resources.credits, Credits(11));
    assert!(state.corp.first_install_used_this_turn);
    assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 1 })));

    let (state, _) = apply_action(
        &state,
        &registry,
        PlayerAction::InstallCard { card_id: CardId("pad_campaign".to_string()), zone: ServerId::Remote(1), slot: InstallSlot::Root },
    )
    .expect("second install should succeed");
    // The second PAD Campaign is free too, and there is no further bonus this turn.
    assert_eq!(state.corp.resources.credits, Credits(11));
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

/// Passes priority while a window is open and nothing else is parked —
/// the "drive to whatever wants resolving next" loop the optional-siphon
/// tests need, where a fixed pass count would either under- or overshoot.
fn pass_until_settled(mut state: GameState, registry: &CardRegistry) -> (GameState, Vec<crate::rules::GameEvent>) {
    let mut events = Vec::new();
    while state.pending_decision.is_none()
        && state.pending_paid_choice.is_none()
        && state.active_trace.is_none()
        && state.paid_ability_window.is_some()
    {
        let side = state.paid_ability_window.as_ref().unwrap().active_priority;
        let (next, e) = apply_action(&state, registry, PlayerAction::PassPriority { side }).expect("pass priority");
        state = next;
        events.extend(e);
    }
    (state, events)
}

/// Runs Account Siphon up to the moment its "you may … instead of
/// breaching" choice parks.
fn siphon_to_choice(corp_credits: u32) -> (crate::rules::GameState, CardRegistry) {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.corp.resources.credits = Credits(corp_credits);
    state.runner.resources.clicks = Clicks(4);
    state.runner.resources.credits = Credits(5);
    state.runner.grip = vec![CardId("account_siphon".to_string())];
    state.corp.hq = vec![CardId("hq_card_0".to_string()), CardId("hq_card_1".to_string())];

    let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("account_siphon".to_string()) })
        .expect("play account siphon, initiating the HQ run");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("reach the server, no ice");
    let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit to the access step");
    let (state, _) = pass_until_settled(state, &registry);
    assert!(
        matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, ref options, .. }) if options.len() == 2),
        "the printed 'you may' is a real choice: siphon, or breach normally — got {:?}",
        state.pending_decision
    );
    (state, registry)
}

/// "…you may force the Corp to lose up to 5 credits, then you gain 2
/// credits **for each credit lost** and take 2 tags." Against a 3-credit
/// Corp the Runner gains 6, not a flat 10 — the gain scales with what was
/// actually lost (`Amount::CreditsLostThisResolution`).
#[test]
fn account_siphon_gains_two_per_credit_the_corp_actually_lost() {
    let (state, registry) = siphon_to_choice(3);
    let (state, events) =
        apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("take the siphon");

    assert_eq!(state.corp.resources.credits, Credits(0), "the Corp had 3 to lose");
    assert_eq!(state.runner.resources.credits, Credits(11), "5 + 2 per credit lost (6), not a flat 10");
    assert_eq!(state.runner.tags, 2);
    assert!(state.active_run.is_none(), "the run is over without a breach");
    assert_eq!(state.corp.hq.len(), 2, "HQ untouched");
    assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardAccessed { .. })));
}

#[test]
fn account_siphon_replaces_hq_access_and_accesses_zero_cards() {
    let (state, registry) = siphon_to_choice(10);
    let (state, events) =
        apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("take the siphon");

    assert_eq!(state.corp.resources.credits, Credits(5), "Corp should lose 5 credits");
    assert_eq!(state.runner.resources.credits, Credits(15), "5 + 2 per credit lost (10)");
    assert_eq!(state.runner.tags, 2);
    assert_eq!(state.corp.hq.len(), 2, "HQ itself is untouched — access was replaced, not resolved");
    assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardAccessed { .. })));
}

/// The other half of the printed "may": declining the siphon consumes the
/// replacement and the Runner breaches HQ normally — right when the Corp
/// is too poor for the siphon to beat an access.
#[test]
fn account_siphon_may_be_declined_in_favor_of_a_normal_breach() {
    let (state, registry) = siphon_to_choice(0);
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("decline the siphon");
    assert!(state.active_run.is_some(), "the run stands; the breach is still owed");
    let (state, events) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("breach normally");
    let mut all = events;
    let (state, more) = pass_until_settled(state, &registry);
    all.extend(more);
    assert!(
        all.iter().any(|e| matches!(e, crate::rules::GameEvent::CardAccessed { .. })),
        "declining leads to a real HQ access: {all:?}"
    );
    assert_eq!(state.runner.tags, 0, "no siphon, no tags");
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
    // Started at 10; installing is free. Unrezzed installs stay silent at
    // start of turn, so this must be rezzed for the trigger to fire below.
    assert_eq!(state.corp.resources.credits, Credits(10));
    let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "pad_campaign") })
        .expect("rez pad campaign");
    // Rez pays the card's printed cost: 10 -> 8.
    assert_eq!(state.corp.resources.credits, Credits(8));

    // Corp's own turn ending doesn't refire their own start-of-turn — only
    // the Runner's turn ending (advancing back to the Corp) does.
    let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
    let (state, _) = close_all_windows(state, &registry);
    let (state, mut events) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
    let (state, close_events) = close_all_windows(state, &registry);
    events.extend(close_events);

    assert_eq!(state.corp.resources.credits, Credits(9), "PAD Campaign should gain 1 credit at the start of the Corp's next turn");
    assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 1 })));
}

/// Runs the Runner into a Snare! installed in a remote, stopping at the
/// access-time decision Snare! parks. `corp_credits` sets what the Corp has
/// to spend on it.
fn run_into_snare(corp_credits: u32) -> (crate::rules::GameState, CardRegistry) {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.runner.resources.clicks = Clicks(4);
    state.corp.resources.credits = Credits(corp_credits);
    state.runner.grip = (0..5).map(|i| CardId(format!("grip_card_{i}"))).collect();
    state.corp.installed = vec![crate::rules::InstalledCard {
        install_id: InstallId(1001),
        card: CardId("snare".to_string()),
        server: ServerId::Remote(0),
        ..Default::default()
    }];

    let (state, _) =
        apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("initiate run");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to success");
    let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("open access window");
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes access window");
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes access window, resolving access");
    // A further window opens at the `PendingInteractiveTrigger` checkpoint
    // itself; close it so the parked decision is what's actually pending.
    let (state, _) = close_all_windows(state, &registry);
    (state, registry)
}

/// Snare! is *the* card driving `AccessInteraction::CorpPaysToApply`: its
/// printed text is "you may pay 4 [credit]. If you do, give the Runner 1 tag
/// and do 3 net damage." Nothing fires until the Corp actually pays.
#[test]
fn snare_asks_the_corp_to_pay_and_deals_three_net_damage_and_a_tag_when_it_does() {
    let (state, registry) = run_into_snare(5);

    assert_eq!(
        crate::rules::current_actor(&state),
        Some(Side::Corp),
        "Snare! asks the Corp to decide, even though the run makes it the Runner's action phase"
    );
    assert_eq!(state.runner.grip.len(), 5, "nothing resolves until the Corp pays");

    let (state, events) =
        apply_action(&state, &registry, PlayerAction::PayAccessTrigger { card_id: CardId("snare".to_string()) })
            .expect("corp pays for snare");

    assert_eq!(state.corp.resources.credits, Credits(1), "5 - 4 to fire Snare!");
    assert_eq!(state.runner.grip.len(), 2, "3 of the 5 grip cards should have been discarded to net damage");
    assert_eq!(state.runner.tags, 1);
    assert!(events.iter().any(|e| matches!(
        e,
        crate::rules::GameEvent::DamageTaken { damage_type: crate::dsl::DamageType::Net, amount: 3 }
    )));
    assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::TagsGiven { side: Side::Runner, amount: 1 })));
}

#[test]
fn snare_does_nothing_when_the_corp_declines_to_pay() {
    let (state, registry) = run_into_snare(5);

    let (state, _) =
        apply_action(&state, &registry, PlayerAction::DeclineAccessTrigger { card_id: CardId("snare".to_string()) })
            .expect("corp declines to pay for snare");

    assert_eq!(state.corp.resources.credits, Credits(5), "declining costs nothing");
    assert_eq!(state.runner.grip.len(), 5, "no net damage");
    assert_eq!(state.runner.tags, 0, "no tag");
}

/// The affordability hint and the resolution guard must agree: a Corp that
/// cannot pay is not offered the paying branch at all.
#[test]
fn a_corp_that_cannot_afford_snare_is_not_offered_the_option() {
    let (state, registry) = run_into_snare(3);

    let legal = crate::rules::legal_actions_for(&state, &registry, Side::Corp);
    assert!(
        !legal.iter().any(|a| matches!(a, PlayerAction::PayAccessTrigger { .. })),
        "paying should not be offered at 3 credits against a cost of 4: {legal:?}"
    );
    assert!(
        legal.iter().any(|a| matches!(a, PlayerAction::DeclineAccessTrigger { .. })),
        "declining must stay available, or the Corp has no legal action at all: {legal:?}"
    );

    // The per-seat slice every real client gets must route the decision to
    // exactly one side. Snare! is the only card that makes the *Corp* that
    // side during the Runner's action phase, and neither sweep contains it
    // — the view-based one runs System Gateway sample decks only — so this
    // stands in for coverage they cannot give this path.
    //
    // The Runner has nothing to do at all, not merely "isn't offered the
    // decision" — an active run suspends every basic action
    // (`engine::apply_action`'s `ActionBlockedByActiveRun` guard), so the
    // whole seat is idle while the Corp owes this one.
    let runner_legal = crate::rules::legal_actions_for(&state, &registry, Side::Runner);
    assert!(
        runner_legal.is_empty(),
        "Snare!'s decision belongs to the Corp, and the Runner cannot act around it: {runner_legal:?}"
    );
}

/// "When the Runner accesses this asset anywhere **except in Archives**" —
/// a Snare! the Runner meets in Archives is inert, and must not even park a
/// decision, or the pause itself would announce a trap that cannot fire.
#[test]
fn snare_offers_the_corp_nothing_when_accessed_from_archives() {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.runner.resources.clicks = Clicks(4);
    state.corp.resources.credits = Credits(10);
    state.runner.grip = (0..5).map(|i| CardId(format!("grip_card_{i}"))).collect();
    state.corp.archives = vec![crate::rules::ArchivedCard {
        card: CardId("snare".to_string()),
        facedown: false,
    }];

    let (state, _) =
        apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Archives }).expect("initiate run");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to success");
    let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("open access window");
    let (state, _) = close_all_windows(state, &registry);

    assert_eq!(
        crate::rules::current_actor(&state),
        Some(Side::Runner),
        "no Corp decision should be parked for a Snare! accessed in Archives"
    );
    assert_eq!(state.runner.grip.len(), 5, "an Archives Snare! deals no damage");
    assert_eq!(state.runner.tags, 0);
    assert_eq!(state.corp.resources.credits, Credits(10), "the Corp is never given the option to pay");
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
        install_id: InstallId(1002),
        card: CardId("wall_of_static".to_string()),
        slot: InstallSlot::Ice,
        rezzed: true,
        ..Default::default()
    }];

    let (state, _) = apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("cleaver".to_string()) })
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

    // Pump for 2c (well within Wall of Static's printed strength once
    // stacked with the break below, though not strictly required here since
    // Cleaver's base strength already matches it), then break its single
    // Barrier subroutine (well within Cleaver's up-to-2 cap) for 1c. Each
    // activation flips window priority to the Corp, who has nothing to
    // activate and passes back.
    let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "cleaver"), ability_index: 1 })
        .expect("pump strength");
    assert_eq!(state.runner.rig[0].effective_strength(), 4);
    let (state, _) =
        apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes back to the runner");

    let (state, events) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "cleaver"), ability_index: 0 })
        .expect("break subroutines");

    assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { index: 0, .. })));
    assert_eq!(
        state.active_run.as_ref().unwrap().ice[0].subroutines[0].status,
        crate::rules::SubroutineStatus::Broken
    );
    assert_eq!(state.runner.resources.credits, Credits(4), "7 - 2 (pump) - 1 (break)");
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

/// Integration tests for the hand-authored System Gateway cards
/// (`data/corp`/`data/runner`). They come from the same compile-time-embedded
/// pool as every other playable card, so — unlike when these cards were
/// only reachable through the `fs-loader` filesystem path — they run in the
/// default build.
/// The engine dispatches `Trigger::OnTransactionPlayed` off `subtypes`,
/// not catalog keywords — and three printed Transactions (Hedge Fund,
/// Hansei Review, Predictive Planogram) lacked the field, so Weyland
/// Consortium: Building a Better World never paid on them.
#[test]
fn building_a_better_world_gains_a_credit_when_hedge_fund_is_played() {
    let registry = registry();
    let mut state = base_state();
    state.corp.identity = Some(CardId("weyland_consortium_building_a_better_world".to_string()));
    state.corp.resources.credits = Credits(5);
    state.corp.hq = vec![CardId("hedge_fund".to_string())];

    let (state, events) =
        apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hedge_fund".to_string()) })
            .expect("play hedge fund");

    assert_eq!(state.corp.resources.credits, Credits(10), "5 - 5 (cost) + 9 (effect) + 1 (identity)");
    assert!(events.iter().any(|e| matches!(e,
        crate::rules::GameEvent::TriggerFired { card, .. } if card.0 == "weyland_consortium_building_a_better_world")));
}

/// "You can advance this ice. It gets +1 strength for each hosted
/// advancement counter." Neither half was modelled: no
/// `advancement_requirement` marker (so `AdvanceCard` refused it) and no
/// per-counter strength (`StrengthModifier::PerHostedAdvancement` is a
/// rate, which the threshold-shaped Pharos variant could not express).
#[test]
fn ice_wall_is_advanceable_and_gains_one_strength_per_advancement() {
    let registry = registry();
    let mut state = base_state();
    state.corp.resources.clicks = Clicks(3);
    state.corp.resources.credits = Credits(5);
    state.corp.installed = vec![crate::rules::InstalledCard {
        install_id: crate::rules::test_support::fixture_install_id("ice_wall"),
        card: CardId("ice_wall".to_string()),
        server: ServerId::Hq,
        slot: InstallSlot::Ice,
        rezzed: true,
        ..Default::default()
    }];

    let (mut state, _) = apply_action(
        &state,
        &registry,
        PlayerAction::AdvanceCard { target: crate::rules::test_support::install_of(&state, "ice_wall") },
    )
    .expect("ice wall is advanceable");
    assert_eq!(state.corp.installed[0].advancement_tokens, 1);

    state.corp.installed[0].advancement_tokens = 3;
    state.phase = GamePhase::Action(Side::Runner);
    state.runner.resources.clicks = Clicks(4);
    let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run HQ");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach ice wall");
    assert_eq!(state.active_run.as_ref().unwrap().ice[0].current_strength, 4, "1 printed + 3 advancement counters");
}

/// Gordian Blade's pump is "+1 strength **for the remainder of this
/// run**" — it must hold from one encounter to the next within a run
/// (unlike every Encounter-duration pump) and be gone once the run ends.
#[test]
fn gordian_blades_pump_lasts_the_whole_run_and_no_longer() {
    let registry = registry();
    let mut state = base_state();
    state.phase = GamePhase::Action(Side::Runner);
    state.runner.resources.clicks = Clicks(4);
    state.runner.resources.credits = Credits(10);
    state.runner.rig = vec![crate::rules::InstalledRunnerCard {
        install_id: crate::rules::test_support::fixture_install_id("gordian_blade"),
        card: CardId("gordian_blade".to_string()),
        base_strength: 2,
        ..Default::default()
    }];
    let enigma = |id: u32| crate::rules::InstalledCard {
        install_id: crate::rules::InstallId(id),
        card: CardId("enigma".to_string()),
        server: ServerId::Hq,
        slot: InstallSlot::Ice,
        rezzed: true,
        ..Default::default()
    };
    state.corp.installed = vec![enigma(7001), enigma(7002)];

    let gordian = crate::rules::test_support::install_of(&state, "gordian_blade");
    let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run HQ");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach the first enigma");
    let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach");
    let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes, encounter 1");

    // Pump once during the first encounter…
    let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: gordian, ability_index: 0 })
        .expect("pump gordian for the run");
    assert_eq!(state.runner.rig[0].effective_strength(), 3);
    // …break both subroutines and move on.
    let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes back");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: gordian, ability_index: 1 })
        .expect("break enigma's first subroutine");
    let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes back");
    let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: gordian, ability_index: 1 })
        .expect("break enigma's second subroutine");
    // Close only this encounter's window (two consecutive passes), so the
    // run stands on the second enigma when we look.
    let side = state.paid_ability_window.as_ref().expect("encounter window open").active_priority;
    let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side }).expect("first pass");
    let side = state.paid_ability_window.as_ref().expect("still open").active_priority;
    let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side }).expect("second pass, encounter 1 resolves");

    let run = state.active_run.as_ref().expect("the run survived the first enigma");
    assert_eq!(run.position, 1, "standing on the second enigma");
    assert_eq!(
        state.runner.rig[0].effective_strength(),
        3,
        "the run-duration pump holds into the second encounter"
    );
    assert_eq!(state.runner.rig[0].run_strength_buff, 1);

    // Let the second enigma bounce the run: the buff dies with the run.
    let (state, _) = pass_until_settled(state, &registry);
    assert!(state.active_run.is_none(), "enigma's end-the-run fired");
    assert_eq!(state.runner.rig[0].run_strength_buff, 0, "the pump ended with the run");
    assert_eq!(state.runner.rig[0].effective_strength(), 2);
}

mod system_gateway {
    use super::*;

    fn sg_registry() -> CardRegistry {
        registry()
    }

    /// A program that declares a memory cost must actually be *offered*.
    ///
    /// This is the check nothing performed, and its absence hid a total
    /// exclusion: both candidate generators built `InstallProgram` with
    /// `memory_cost: 0` while `engine::install_program` treated the
    /// registry's declared value as authoritative and rejected a mismatch.
    /// `legal_actions` keeps only candidates `apply_action` accepts, so
    /// every program was filtered out before reaching any caller — and all
    /// 14 playable programs declare a cost, so no Runner could install one
    /// at all through the legal-action path. Every per-card test reached
    /// past it by calling `apply_action` directly with the right value.
    ///
    /// The round trip is asserted alongside because the index path failed
    /// the same way for a different reason: `ActionSpace::action_at` takes
    /// no `CardRegistry` and so could only ever synthesise `0`.
    #[test]
    fn a_program_with_a_memory_cost_is_offered_and_round_trips() {
        let registry = sg_registry();
        assert_eq!(
            registry.get(&CardId("corroder".to_string())).expect("corroder is registered").memory_cost,
            Some(1),
            "the premise: Corroder declares a memory cost"
        );

        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("corroder".to_string())];

        let install = crate::rules::legal_actions(&state, &registry)
            .into_iter()
            .find(|a| matches!(a, PlayerAction::InstallProgram { card_id, .. } if card_id.0 == "corroder"))
            .expect("installing Corroder must be a legal action");

        let index = crate::rules::ActionSpace::index_of(&state, &install).expect("it has an index");
        assert_eq!(
            crate::rules::ActionSpace::action_at(&state, index),
            Some(install.clone()),
            "and the index decodes back to the same action"
        );

        let (state, _) = apply_action(&state, &registry, install).expect("and it resolves");
        assert_eq!(state.runner.rig[0].card, CardId("corroder".to_string()));
    }

    /// Memory is freed when a program leaves play.
    ///
    /// This is the bug that fixing the install path would otherwise have
    /// woken up. Memory used to be *spent* — `MemoryUnits::spend` was called
    /// by the two install handlers and refunded by none of the five paths a
    /// rig card can leave play by — so a Runner whose programs were trashed
    /// lost that memory permanently. It went unnoticed because no program
    /// could be installed through a legal action at all, so nothing was ever
    /// spent. Memory is now derived from the rig (`rules::memory`), which
    /// makes freeing it automatic rather than something five sites must
    /// each remember to do.
    #[test]
    fn trashing_a_program_frees_its_memory() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("corroder".to_string())];

        // Read the derived budget, not the fixture's hand-set field —
        // `base_state` seeds a number that the refresh in `apply_action`
        // immediately corrects.
        let base = crate::rules::memory::available_memory(&state, &registry);
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgram { card_id: CardId("corroder".to_string()) },
        )
        .expect("install corroder");
        assert_eq!(state.runner.memory_units.0, base - 1, "Corroder reserves 1 MU while installed");

        let mut trashed = state.clone();
        crate::rules::evaluate_effect(
            &mut trashed,
            &crate::dsl::Effect::TrashCard(crate::dsl::CardTarget::RunnerRig(CardId("corroder".to_string()))),
            &mut crate::rules::ResolutionContext::default(),
            &registry,
        )
        .expect("trash corroder");
        assert!(trashed.runner.rig.is_empty());

        // The report is refreshed by `apply_action`, so drive one — any
        // action will do; this asserts the refresh point, not the effect.
        let (trashed, _) =
            apply_action(&trashed, &registry, PlayerAction::GainCreditClick { side: Side::Runner })
                .expect("any action refreshes the memory report");
        assert_eq!(trashed.runner.memory_units.0, base, "and gives it back on leaving play");
    }

    /// Printed link is joined from the catalog, never authored. *Kate
    /// "Mac" McCaffrey* is the only implemented identity with link; every
    /// System Gateway Runner identity prints 0.
    #[test]
    fn printed_link_comes_from_the_catalog() {
        let registry = sg_registry();
        let link = |id: &str| registry.get(&CardId(id.to_string())).expect(id).base_link;
        assert_eq!(link("kate_mccaffrey"), Some(1));
        for identity in ["rene_loup_arcemont", "zahya_sadeghi", "tao_salonga"] {
            assert_eq!(link(identity), Some(0), "{identity}");
        }
        assert_eq!(link("corroder"), None, "not an identity: no printed link at all");
    }

    /// A trojan installs *on* ICE. `legal_actions` never offered it as an
    /// ordinary `InstallProgram`, but the handler accepted one, and a
    /// hostless Tranquilizer failed its own third turn-start
    /// (`DerezCard(HostIce)` with no host). The handler refuses it now.
    #[test]
    fn a_trojan_cannot_be_installed_from_the_grip_as_an_ordinary_program() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("botulus".to_string()), CardId("tranquilizer".to_string())];
        state.corp.installed = vec![corp_ice("palisade", ServerId::Hq)];

        for trojan in ["botulus", "tranquilizer"] {
            assert_eq!(
                apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId(trojan.to_string()) }).err(),
                Some(RulesError::TrojanMustBeHostedOnIce(CardId(trojan.to_string())))
            );
        }
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgramOnIce { card_id: CardId("botulus".to_string()), host: install_of(&state, "palisade") },
        )
        .expect("hosting on ICE is the one way in");
        assert_eq!(state.runner.rig[0].hosted_on_ice, Some(install_of(&state, "palisade")));
    }

    /// A selection-trash of a piece of ICE takes the trojans hosted on it,
    /// as `Effect::TrashCard` always has — this was the one removal path
    /// that left them in the rig pointing at ICE that no longer existed.
    #[test]
    fn a_selection_trash_of_ice_cascades_to_the_trojans_hosted_on_it() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.installed = vec![corp_ice("palisade", ServerId::Hq), corp_ice("whitespace", ServerId::RnD)];
        let mut rig = rig_of(&["botulus", "corroder"]);
        rig[0].hosted_on_ice = Some(fixture_install_id("palisade"));
        state.runner.rig = rig;
        // A Runner-side "trash 1 installed Corp card" selection, hand-built:
        // no pool card offers exactly this, but Ansel 1.0/Retribution park
        // the mirror image against the Runner through the same code.
        state.pending_decision = Some(crate::rules::PendingDecision::ChooseCards {
            side: Side::Runner,
            source: crate::dsl::CardZoneRef::OpponentInstalled,
            filter: crate::dsl::CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: Some(crate::dsl::CardZoneRef::OpponentDiscard),
            then: None,
            selected: Vec::new(),
            source_card: None,
            source_install: None,
            resume: crate::rules::PendingChoiceResume::None,
        });

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "palisade") })
                .expect("pick palisade");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");

        assert!(events.contains(&crate::rules::GameEvent::CardTrashed { side: Side::Corp, card: CardId("palisade".to_string()) }));
        assert!(events.contains(&crate::rules::GameEvent::CardTrashed { side: Side::Runner, card: CardId("botulus".to_string()) }), "{events:?}");
        assert_eq!(state.corp.installed.len(), 1, "whitespace remains");
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "palisade" && !a.facedown), "a rezzed ICE lands faceup");
        let rig: Vec<&str> = state.runner.rig.iter().map(|c| c.card.0.as_str()).collect();
        assert_eq!(rig, vec!["corroder"], "botulus went with its host");
        assert!(state.runner.heap.contains(&CardId("botulus".to_string())));
    }

    /// Only a move into a discard pile is a trash. The same selection
    /// machinery moves cards into R&D (Spin Doctor, Sprint) and the grip
    /// (Mutual Favor); those say nothing.
    #[test]
    fn a_selection_records_a_trash_only_when_the_destination_is_a_discard_pile() {
        let registry = sg_registry();
        let decision = |destination: crate::dsl::CardZoneRef| crate::rules::PendingDecision::ChooseCards {
            side: Side::Corp,
            source: crate::dsl::CardZoneRef::OwnHq,
            filter: crate::dsl::CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: Some(destination),
            then: None,
            selected: Vec::new(),
            source_card: None,
            source_install: None,
            resume: crate::rules::PendingChoiceResume::None,
        };
        let run = |destination: crate::dsl::CardZoneRef| {
            let mut state = base_state();
            state.corp.hq = vec![CardId("hedge_fund".to_string())];
            state.pending_decision = Some(decision(destination));
            let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("toggle");
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm")
        };
        let trashed = crate::rules::GameEvent::CardTrashed { side: Side::Corp, card: CardId("hedge_fund".to_string()) };

        let (state, events) = run(crate::dsl::CardZoneRef::OwnArchives);
        assert!(events.contains(&trashed), "{events:?}");
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "hedge_fund" && a.facedown), "unseen, so facedown");

        let (state, events) = run(crate::dsl::CardZoneRef::OwnRAndD);
        assert!(!events.contains(&trashed), "{events:?}");
        assert_eq!(state.corp.r_and_d.last(), Some(&CardId("hedge_fund".to_string())));
    }

    /// A hand-built rig of `ids`, each with its own fixture `InstallId` so
    /// the selection machinery can address them individually.
    fn rig_of(ids: &[&str]) -> Vec<crate::rules::InstalledRunnerCard> {
        ids.iter()
            .map(|id| crate::rules::InstalledRunnerCard {
                install_id: fixture_install_id(id),
                card: CardId(id.to_string()),
                ..Default::default()
            })
            .collect()
    }

    fn memory_limit_exceeded(events: &[crate::rules::GameEvent]) -> Option<u32> {
        events.iter().find_map(|e| match e {
            crate::rules::GameEvent::MemoryLimitExceeded { over_by } => Some(*over_by),
            _ => None,
        })
    }

    /// The memory limit is a checkpoint, not just an install gate. Before
    /// `memory::enforce_limit`, *Retribution* trashing a console under a
    /// full rig left every program installed with the report clamped at
    /// `0` — the Runner kept 5 MU of programs on 4 MU indefinitely.
    #[test]
    fn trashing_a_console_with_a_full_rig_forces_the_runner_to_trash_one_program() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("retribution".to_string())];
        state.runner.tags = 1;
        state.runner.rig = rig_of(&["pennyshaver", "corroder", "cleaver", "buzzsaw", "carmen", "unity"]);
        assert_eq!(crate::rules::memory::memory_balance(&state, &registry), 0, "5 MU in use on 4 + 1");

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("retribution".to_string()) })
                .expect("play retribution against a tagged runner");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ToggleCardSelection { position: position_of(&state, "pennyshaver") },
        )
        .expect("the corp picks the console");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("and trashes it");

        assert!(
            events.contains(&crate::rules::GameEvent::CardTrashed { side: Side::Runner, card: CardId("pennyshaver".to_string()) }),
            "a selection-trash says so, naming the card's owner: {events:?}"
        );
        assert_eq!(memory_limit_exceeded(&events), Some(1), "{events:?}");
        assert_eq!(state.runner.memory_units, MemoryUnits(0), "the report still never reads negative");
        assert!(
            matches!(
                state.pending_decision,
                Some(crate::rules::PendingDecision::ChooseCards { side: Side::Runner, min: 1, max: 1, .. })
            ),
            "a 1-of-N program trash is parked for the Runner: {:?}",
            state.pending_decision
        );
        assert_eq!(state.phase, GamePhase::Action(Side::Corp), "still the Corp's turn");
        assert_eq!(crate::rules::current_actor(&state), Some(Side::Runner), "but the Runner is the one to act");

        // The Runner chooses which program goes — the rules give them that,
        // and the engine does not pick for them.
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "unity") })
                .expect("pick unity");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash unity");

        assert_eq!(memory_limit_exceeded(&events), None, "within the limit again");
        assert!(events.contains(&crate::rules::GameEvent::CardTrashed { side: Side::Runner, card: CardId("unity".to_string()) }));
        assert!(state.pending_decision.is_none());
        let rig: Vec<&str> = state.runner.rig.iter().map(|c| c.card.0.as_str()).collect();
        assert_eq!(rig, vec!["corroder", "cleaver", "buzzsaw", "carmen"]);
        assert!(state.runner.heap.contains(&CardId("unity".to_string())), "trashed, not removed from the game");
        assert_eq!(state.runner.memory_units, MemoryUnits(0));
        assert_eq!(crate::rules::current_actor(&state), Some(Side::Corp), "and the Corp's turn resumes");
    }

    /// Programs cost different amounts, so the checkpoint never computes
    /// "how many": it takes one, re-checks on the next action, and repeats.
    /// Trashing *Mayfly* (2 MU) first would have cleared a 2-MU deficit in
    /// one step; trashing a 1-MU program first leaves one more to go.
    #[test]
    fn an_over_budget_rig_forces_one_program_trash_per_action_until_within_limit() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        // 6 MU in use on 4. No single card produces this today; two consoles
        // leaving play in one resolution would.
        state.runner.rig = rig_of(&["mayfly", "corroder", "cleaver", "buzzsaw", "carmen"]);
        assert_eq!(crate::rules::memory::memory_balance(&state, &registry), -2);

        let (state, events) = apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Runner })
            .expect("any action reaches the checkpoint");
        assert_eq!(memory_limit_exceeded(&events), Some(2));

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "corroder") })
                .expect("pick corroder");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert_eq!(memory_limit_exceeded(&events), Some(1), "one 1-MU program was not enough; parked again at once");
        assert!(matches!(
            state.pending_decision,
            Some(crate::rules::PendingDecision::ChooseCards { side: Side::Runner, .. })
        ));

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "mayfly") })
                .expect("pick mayfly");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert_eq!(memory_limit_exceeded(&events), None);
        assert!(state.pending_decision.is_none());
        assert_eq!(crate::rules::memory::memory_balance(&state, &registry), 1);
        let rig: Vec<&str> = state.runner.rig.iter().map(|c| c.card.0.as_str()).collect();
        assert_eq!(rig, vec!["cleaver", "buzzsaw", "carmen"]);
    }

    /// Exactly full is not over: a rig at 4 of 4 MU parks nothing.
    #[test]
    fn a_rig_exactly_at_the_memory_limit_is_not_forced_to_trash() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = rig_of(&["corroder", "cleaver", "buzzsaw", "carmen"]);
        assert_eq!(crate::rules::memory::memory_balance(&state, &registry), 0);

        let (state, events) = apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Runner })
            .expect("gain a credit");
        assert_eq!(memory_limit_exceeded(&events), None);
        assert!(state.pending_decision.is_none());
        assert_eq!(state.runner.rig.len(), 4);
    }

    /// A trojan hosted on ICE is still an installed program in the rig and
    /// still counts against memory, so it is a legal pick for the forced
    /// trash — and leaves the rig by the same `InstallId`-keyed removal.
    #[test]
    fn a_hosted_trojan_can_be_the_program_trashed_to_the_memory_limit() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("palisade", ServerId::Hq)];
        let mut rig = rig_of(&["botulus", "corroder", "cleaver", "buzzsaw", "carmen"]);
        rig[0].hosted_on_ice = Some(fixture_install_id("palisade"));
        state.runner.rig = rig;
        assert_eq!(crate::rules::memory::memory_balance(&state, &registry), -1);

        let (state, events) = apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Runner })
            .expect("gain a credit");
        assert_eq!(memory_limit_exceeded(&events), Some(1));

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "botulus") })
                .expect("the hosted trojan is eligible");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert!(state.pending_decision.is_none());
        assert!(!state.runner.rig.iter().any(|c| c.card.0 == "botulus"));
        assert!(state.runner.heap.contains(&CardId("botulus".to_string())));
        assert_eq!(crate::rules::memory::memory_balance(&state, &registry), 0);
    }

    /// The `memory_bonus` half of the same property.
    ///
    /// `install_hardware` used to add a console's "+1[mu]" straight onto
    /// `memory_units` and never remove it, which its own comment recorded as
    /// a deliberate shortcut ("threading a `CardRegistry` through every
    /// trash path... would be a much larger refactor"). Deriving the budget
    /// made that free.
    #[test]
    fn a_trashed_console_takes_its_memory_bonus_with_it() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("pennyshaver".to_string())];

        let base = crate::rules::memory::available_memory(&state, &registry);
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallHardware { card_id: CardId("pennyshaver".to_string()) },
        )
        .expect("install pennyshaver");
        assert_eq!(state.runner.memory_units.0, base + 1, "the console grants +1 MU");

        let mut trashed = state.clone();
        crate::rules::evaluate_effect(
            &mut trashed,
            &crate::dsl::Effect::TrashCard(crate::dsl::CardTarget::RunnerRig(CardId("pennyshaver".to_string()))),
            &mut crate::rules::ResolutionContext::default(),
            &registry,
        )
        .expect("trash pennyshaver");
        let (trashed, _) =
            apply_action(&trashed, &registry, PlayerAction::GainCreditClick { side: Side::Runner })
                .expect("any action refreshes the memory report");
        assert_eq!(trashed.runner.memory_units.0, base, "and takes it away again");
    }

    /// The memory limit binds through `legal_actions`, not merely through a
    /// rejection.
    ///
    /// A bot never submits an action it was not offered, so a limit that
    /// only shows up as an `Err` from `apply_action` is invisible to one.
    /// The base budget is 4 and every System Gateway breaker costs 1, so a
    /// fifth is the one that must stop being offered.
    #[test]
    fn a_full_rig_stops_offering_further_program_installs() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.credits = Credits(50);
        let programs =
            ["corroder", "cleaver", "buzzsaw", "carmen", "unity"].map(|id| CardId(id.to_string()));
        state.runner.grip = programs.to_vec();

        for card_id in programs.iter().take(4) {
            state.runner.resources.clicks = Clicks(4);
            let install = PlayerAction::InstallProgram { card_id: card_id.clone() };
            assert!(
                crate::rules::legal_actions(&state, &registry).contains(&install),
                "{} must still be offered with memory free",
                card_id.0
            );
            state = apply_action(&state, &registry, install).expect("install").0;
        }

        assert_eq!(state.runner.memory_units, crate::rules::MemoryUnits(0), "4 MU, four 1-MU programs");
        state.runner.resources.clicks = Clicks(4);
        let fifth = PlayerAction::InstallProgram { card_id: programs[4].clone() };
        assert!(
            !crate::rules::legal_actions(&state, &registry).contains(&fifth),
            "a fifth program must not be offered with no memory left"
        );
        assert!(matches!(
            apply_action(&state, &registry, fifth),
            Err(crate::rules::RulesError::InsufficientMemory { available: 0, requested: 1 })
        ));
    }

    #[test]
    fn government_subsidy_gains_fifteen_credits() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.hq = vec![CardId("government_subsidy".to_string())];

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::PlayOperation { card_id: CardId("government_subsidy".to_string()) },
        )
        .expect("government subsidy should resolve");

        // 10 - 10 (cost) + 15 (effect) = 15.
        assert_eq!(state.corp.resources.credits, Credits(15));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 15 })));
    }

    #[test]
    fn offworld_office_scores_for_two_points_and_gains_seven_credits() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1003),
            card: CardId("offworld_office".to_string()),
            server: ServerId::Remote(0),
            advancement_tokens: 4,
            ..Default::default()
        }];

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "offworld_office") })
                .expect("offworld office should score");

        assert_eq!(state.corp.resources.credits, Credits(7));
        assert_eq!(state.corp.scored_agendas.iter().map(|s| s.card.clone()).collect::<Vec<_>>(), vec![CardId("offworld_office".to_string())]);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 7 })));
    }

    #[test]
    fn tithe_deals_one_net_damage_then_gains_one_credit() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.resources.credits = Credits(0);
        state.runner.grip = vec![CardId("grip_card_0".to_string())];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1004),
            card: CardId("tithe".to_string()),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach tithe");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach window");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes approach window, entering encounter");
        // Neither side breaks/activates anything — pass straight through the
        // encounter window so both subroutines auto-fire on window close.
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes encounter window");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes encounter window, firing subroutines");

        assert!(state.runner.grip.is_empty(), "the one grip card should have been discarded to net damage");
        assert_eq!(state.corp.resources.credits, Credits(1));
        assert!(events.iter().any(|e| matches!(
            e,
            crate::rules::GameEvent::DamageTaken { damage_type: crate::dsl::DamageType::Net, amount: 1 }
        )));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 1 })));
    }

    /// "Do 2 net damage. The Runner may jack out." is a decision *between*
    /// the two subroutines. It used to be `Effect::PermitJackOut`, a flag
    /// that `resolve_unbroken_subroutines` never paused for: both
    /// subroutines fired in one batch, all 4 damage landed, and the "may
    /// jack out" the Runner had paid two cards for was only ever offered
    /// after the encounter — when it no longer bought anything. The first
    /// subroutine now parks a Runner choice; taking it ends the run with
    /// the second subroutine unfired.
    #[test]
    fn karuna_lets_the_runner_jack_out_between_its_two_subroutines() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = (0..5).map(|i| CardId(format!("grip_card_{i}"))).collect();
        state.corp.installed = vec![corp_ice("karuna", ServerId::Hq)];

        let state = enter_encounter_with(state, &registry, ServerId::Hq);
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes encounter window");
        let (after_first, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes encounter window, firing the first subroutine");

        assert_eq!(after_first.runner.grip.len(), 3, "only the first subroutine's 2 net damage has resolved");
        assert!(
            matches!(after_first.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })),
            "the Runner is asked whether to jack out before the second subroutine fires"
        );

        // Jack out: the run ends, the second subroutine never fires.
        let (jacked_out, _) = apply_action(&after_first, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("runner jacks out");
        assert!(jacked_out.active_run.is_none(), "the run is over");
        assert_eq!(jacked_out.runner.grip.len(), 3, "no further damage");

        // Stay: the second subroutine fires and the run goes on.
        let (stayed, _) = apply_action(&after_first, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 })
            .expect("runner stays in the run");
        assert_eq!(stayed.runner.grip.len(), 1, "the second subroutine's 2 net damage resolved");
        assert!(stayed.active_run.is_some(), "and the run continues past Karunā");
    }

    #[test]
    fn buzzsaw_pumps_strength_and_breaks_up_to_two_code_gate_subroutines() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("buzzsaw".to_string())];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1006),
            card: CardId("enigma".to_string()),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("buzzsaw".to_string()) })
                .expect("install buzzsaw");
        assert_eq!(state.runner.resources.credits, Credits(6), "10 - 4 (Buzzsaw's install cost)");

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach enigma");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach window");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes approach window, entering encounter");

        // Buzzsaw's base strength (3) already matches Enigma's (2), so break
        // its up-to-2 code gate subroutines outright for 1 credit — no pump
        // needed, but exercise the pump ability too for coverage.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "buzzsaw"), ability_index: 1 })
            .expect("pump strength");
        assert_eq!(state.runner.rig[0].effective_strength(), 4);
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes back to the runner");

        let (state, events) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "buzzsaw"), ability_index: 0 })
            .expect("break subroutines");

        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { index: 0, .. })));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { index: 1, .. })));
        assert_eq!(state.runner.resources.credits, Credits(2), "6 - 3 (pump) - 1 (break)");
    }

    #[test]
    fn rene_loup_arcemont_gains_a_credit_and_draws_once_per_turn_on_trashing_an_accessed_card() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.identity = Some(CardId("rene_loup_arcemont".to_string()));
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(5);
        state.runner.stack = vec![CardId("stack_card".to_string())];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1007),
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to the approach step");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes access window");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes success window, presenting the pending choice");
        // Landing on `PendingChoice` opens a fresh paid-ability window of
        // its own (see `paid_ability::open_window_if_at_checkpoint`) — both
        // sides must pass it too before `TrashAccessedCard` is legal.
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes pending-choice window");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes pending-choice window");
        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::TrashAccessedCard { card_id: CardId("pad_campaign".to_string()) },
        )
        .expect("trash the accessed asset");

        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 1 })));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardDrawn { side: Side::Runner })));
        assert!(state.runner.stack.is_empty(), "the drawn card should have left the stack");
        assert!(state.runner.once_per_turn_used.iter().any(|k| k.tag == "rene_loup_arcemont"));
    }

    #[test]
    fn docklands_pass_grants_one_additional_hq_access_on_the_first_hq_run_this_turn_only() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("docklands_pass".to_string()),
            ..Default::default()
        }];
        state.corp.hq = vec![CardId("hq_card_0".to_string()), CardId("hq_card_1".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to the approach step");
        let (state, events) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");

        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::AdditionalAccessGranted { server: ServerId::Hq, count: 1 })));
        assert_eq!(state.active_run.as_ref().unwrap().additional_hq_access, 1);
    }

    fn ice_installed(id: &str, server: ServerId, rezzed: bool) -> crate::rules::InstalledCard {
        crate::rules::InstalledCard {
            install_id: fixture_install_id(id),
            card: CardId(id.to_string()),
            server,
            slot: InstallSlot::Ice,
            rezzed,
            ..Default::default()
        }
    }

    #[test]
    fn funhouse_encounter_ends_the_run_unless_the_runner_takes_a_tag() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![ice_installed("funhouse", ServerId::Hq, true)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach funhouse");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes approach window, entering encounter and firing OnEncounter");

        assert!(state.pending_paid_choice.is_some(), "OnEncounter should have parked a choice");
        assert!(state.active_run.is_some(), "not resolved yet");

        // Decline to take the tag: the run ends.
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("decline the tag");
        assert!(state.active_run.is_none(), "run should have ended");
        assert_eq!(state.runner.tags, 0);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunEndedByEffect { .. })));
    }

    #[test]
    fn ping_gives_a_tag_when_rezzed_during_a_run_against_its_own_server() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.resources.credits = Credits(10);
        state.corp.installed = vec![ice_installed("ping", ServerId::Hq, false)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach ping");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "ping") }).expect("rez ping mid-approach");

        assert_eq!(state.runner.tags, 1);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::TagsGiven { side: Side::Runner, amount: 1 })));
    }

    #[test]
    fn manegarm_skunkworks_ends_the_run_unless_the_runner_pays() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1010),
            card: CardId("manegarm_skunkworks".to_string()),
            rezzed: true,
            ..Default::default()
        }];

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        // No ICE at all: `ContinueRun`'s `Initiation` arm reaches `Success`
        // immediately, firing `OnApproachServer` synchronously.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("reach server approach");

        assert!(state.pending_paid_choice.is_some());
        let pending = state.pending_paid_choice.as_ref().unwrap();
        assert_eq!(pending.cost, crate::dsl::Cost::AnyOf(vec![crate::dsl::Cost::Clicks(2), crate::dsl::Cost::Credits(5)]));
        drop(events);

        // Pay the credits option (index 1).
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: Some(1) })
            .expect("pay 5 credits");
        assert_eq!(state.runner.resources.credits, Credits(5));
        assert!(state.active_run.is_some(), "run should continue after paying");
    }

    #[test]
    fn public_trail_gives_a_tag_when_declined() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.hq = vec![CardId("public_trail".to_string())];
        state.runner.made_successful_run_last_turn = true;

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("public_trail".to_string()) })
                .expect("play public trail");
        assert!(state.pending_paid_choice.is_some());

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("decline to pay");
        assert_eq!(state.runner.tags, 1);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::TagsGiven { side: Side::Runner, amount: 1 })));
    }

    #[test]
    fn public_trail_play_requirement_rejects_without_a_successful_run_last_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.hq = vec![CardId("public_trail".to_string())];

        let result = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("public_trail".to_string()) });
        assert_eq!(result, Err(RulesError::RequirementNotMet));
    }

    #[test]
    fn wildcat_strike_lets_the_corp_choose_the_runners_reward() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = vec![CardId("wildcat_strike".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("wildcat_strike".to_string()) })
            .expect("play wildcat strike");
        assert!(state.pending_decision.is_some());

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("corp picks credits");
        assert_eq!(state.runner.resources.credits, Credits(14), "10 - 2 (play cost) + 6");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 6 })));
    }

    #[test]
    fn predictive_planogram_offers_a_choice_when_the_runner_is_not_tagged() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.hq = vec![CardId("predictive_planogram".to_string())];
        state.corp.r_and_d = (0..5).map(|i| CardId(format!("rd_card_{i}"))).collect();

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("predictive_planogram".to_string()) })
                .expect("play predictive planogram");
        assert!(state.pending_decision.is_some());

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("corp picks draw");
        assert_eq!(state.corp.hq.len(), 3);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardDrawn { side: Side::Corp })));
    }

    /// "If the Runner is tagged, you may resolve both instead" — the Corp
    /// still chooses. It used to resolve both unconditionally when tagged,
    /// which forced a draw the Corp may not want (a thin R&D, a full HQ).
    /// Tagged, the choice has three options: either one, or both.
    #[test]
    fn predictive_planogram_may_resolve_both_options_when_the_runner_is_tagged() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.hq = vec![CardId("predictive_planogram".to_string())];
        state.corp.r_and_d = (0..5).map(|i| CardId(format!("rd_card_{i}"))).collect();
        state.runner.tags = 1;

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("predictive_planogram".to_string()) })
                .expect("play predictive planogram");
        match &state.pending_decision {
            Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, options, .. }) => {
                assert_eq!(options.len(), 3, "gain, draw, or both")
            }
            other => panic!("expected the Corp's three-way choice, got {other:?}"),
        }

        let (both, events) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 2 }).expect("corp takes both");
        assert_eq!(both.corp.resources.credits, Credits(13), "10 - 0 (cost) + 3");
        assert_eq!(both.corp.hq.len(), 3);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 3 })));

        let (credits_only, _) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("corp takes credits only");
        assert_eq!(credits_only.corp.resources.credits, Credits(13));
        assert!(credits_only.corp.hq.is_empty(), "declined the draw");
    }

    #[test]
    fn orbital_superiority_gives_a_tag_when_the_runner_is_not_tagged() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1011),
            card: CardId("orbital_superiority".to_string()),
            server: ServerId::Remote(0),
            advancement_tokens: 4,
            ..Default::default()
        }];

        let (state, events) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "orbital_superiority") })
            .expect("score orbital superiority");

        assert_eq!(state.runner.tags, 1);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::TagsGiven { side: Side::Runner, amount: 1 })));
    }

    #[test]
    fn orbital_superiority_deals_meat_damage_when_the_runner_is_tagged() {
        let registry = sg_registry();
        let mut state = base_state();
        state.runner.tags = 1;
        state.runner.grip = (0..3).map(|i| CardId(format!("grip_card_{i}"))).collect();
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1012),
            card: CardId("orbital_superiority".to_string()),
            server: ServerId::Remote(0),
            advancement_tokens: 4,
            ..Default::default()
        }];

        let (state, events) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "orbital_superiority") })
            .expect("score orbital superiority");

        assert!(state.runner.grip.is_empty(), "4 meat damage against a 3-card grip should flatline");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunnerFlatlined)));
    }

    #[test]
    fn nbn_reality_plus_offers_the_corp_a_choice_the_first_time_the_runner_is_tagged_each_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.resources.credits = Credits(10);
        state.corp.identity = Some(CardId("nbn_reality_plus".to_string()));
        state.corp.installed = vec![ice_installed("ping", ServerId::Hq, false)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach ping");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "ping") }).expect("rez ping, giving a tag");

        assert_eq!(state.runner.tags, 1);
        assert!(state.pending_decision.is_some(), "NBN: Reality Plus should have offered a choice");

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("corp picks credits");
        assert_eq!(state.corp.resources.credits, Credits(10), "10 - 2 (rez cost) + 2 (choice)");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 2 })));
    }

    /// A tag paid as a cost is still a taken tag. Funhouse's encounter —
    /// "end the run unless the Runner takes 1 tag" — pays `Cost::TakeTags`,
    /// whose `TagsGiven` event used to go undispatched, so the printed
    /// pairing (NBN: Reality Plus + Funhouse) never fired the identity.
    #[test]
    fn nbn_reality_plus_reacts_to_a_tag_taken_as_a_cost() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.identity = Some(CardId("nbn_reality_plus".to_string()));
        state.corp.installed = vec![corp_ice("funhouse", ServerId::Hq)];

        let state = enter_encounter_with(state, &registry, ServerId::Hq);
        assert!(state.pending_paid_choice.is_some(), "funhouse's encounter choice is parked");

        let corp_credits_before = state.corp.resources.credits;
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None })
            .expect("runner takes the tag to stay in the run");
        assert_eq!(state.runner.tags, 1);
        assert!(
            matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })),
            "NBN: Reality Plus reacts to the taken tag"
        );
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("corp takes credits");
        assert_eq!(state.corp.resources.credits, corp_credits_before.gain(2));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 2 })));
    }

    /// Zahya's once-per-turn is consumed the moment her trigger fires, and
    /// `OnRunEnded` fires for a bounced run too — so a 0-access run on a
    /// central used to burn the once-per-turn for a gain of zero, silently
    /// disenfranchising a later run the same turn. The 0-access run must
    /// leave it unconsumed.
    #[test]
    fn zahya_sadeghi_is_not_spent_by_a_run_that_accessed_nothing() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(0);
        state.runner.identity = Some(CardId("zahya_sadeghi".to_string()));
        state.corp.installed = vec![corp_ice("palisade", ServerId::RnD)];
        state.corp.hq = vec![CardId("hq_card_0".to_string())];

        // Run 1: R&D, bounced by Palisade's "end the run" — 0 accesses.
        let state = enter_encounter_with(state, &registry, ServerId::RnD);
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes encounter window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes, firing the end-the-run subroutine");
        assert!(state.active_run.is_none(), "bounced");
        assert_eq!(state.runner.resources.credits, Credits(0), "no gain from a 0-access run");
        assert!(state.runner.once_per_turn_used.is_empty(), "and the once-per-turn was not consumed");

        // Run 2, same turn: HQ, one access — the identity still pays out.
        let (state, mut events) = run_to_completion(state, &registry, ServerId::Hq);
        let (state, closing_events) = close_all_windows(state, &registry);
        events.extend(closing_events);
        let (state, more_events) =
            apply_action(&state, &registry, PlayerAction::PassAccessedCard { card_id: CardId("hq_card_0".to_string()) })
                .expect("pass on the accessed card, concluding the run");
        events.extend(more_events);
        assert_eq!(state.runner.resources.credits, Credits(1), "1 card accessed on the HQ run");
    }

    /// "…while it is installed": an Urtica Cipher accessed out of R&D is
    /// not installed and deals nothing. The trigger used to fire anyway —
    /// flat 2 net damage from a deck access, and with another copy on the
    /// table the first-match token read could even size the hit by *that*
    /// copy's advancement counters.
    #[test]
    fn urtica_cipher_deals_no_damage_when_accessed_from_rnd() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = (0..3).map(|i| CardId(format!("grip_card_{i}"))).collect();
        state.corp.r_and_d = vec![CardId("urtica_cipher".to_string())];
        // A second, installed and heavily advanced copy elsewhere must not
        // answer for the copy being accessed out of R&D.
        state.corp.installed = vec![installed_with_counters("urtica_cipher", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 3;

        let (state, events) = run_to_completion(state, &registry, ServerId::RnD);
        assert_eq!(state.runner.grip.len(), 3, "no net damage from the R&D access");
        assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::DamageTaken { .. })), "{events:?}");
    }

    #[test]
    fn zahya_sadeghi_gains_a_credit_per_card_accessed_when_a_run_on_hq_ends() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(0);
        state.runner.identity = Some(CardId("zahya_sadeghi".to_string()));
        state.corp.hq = vec![CardId("hq_card_0".to_string())];

        let (state, mut events) = run_to_completion(state, &registry, ServerId::Hq);
        let (state, closing_events) = close_all_windows(state, &registry);
        events.extend(closing_events);
        let (state, more_events) =
            apply_action(&state, &registry, PlayerAction::PassAccessedCard { card_id: CardId("hq_card_0".to_string()) })
                .expect("pass on the accessed card, concluding the run");
        events.extend(more_events);

        assert_eq!(state.runner.resources.credits, Credits(1), "1 card accessed on this HQ run");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 1 })));
    }

    #[test]
    fn verbal_plasticity_draws_an_extra_card_on_the_first_basic_draw_each_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("verbal_plasticity".to_string()),
            ..Default::default()
        }];
        state.runner.stack = (0..3).map(|i| CardId(format!("stack_card_{i}"))).collect();

        let (state, events) = apply_action(&state, &registry, PlayerAction::DrawCardClick { side: Side::Runner }).expect("draw a card");

        assert_eq!(state.runner.grip.len(), 2, "1 basic + 1 bonus from Verbal Plasticity");
        assert_eq!(events.iter().filter(|e| matches!(e, crate::rules::GameEvent::CardDrawn { .. })).count(), 2);
    }

    #[test]
    fn diviner_ends_the_run_when_it_nets_an_odd_cost_card() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        // Sure Gamble has a printed cost of 5 (odd).
        state.runner.grip = vec![CardId("sure_gamble".to_string())];
        state.corp.installed = vec![ice_installed("diviner", ServerId::Hq, true)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach diviner");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes approach window, entering encounter");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes encounter window");
        let (state, events) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes encounter window, firing the subroutine");

        assert!(state.runner.grip.is_empty(), "sure_gamble should have been discarded to net damage");
        assert!(state.active_run.is_none(), "an odd-cost card was trashed — the run should have ended");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunEndedByEffect { .. })));
    }

    #[test]
    fn whitespace_ends_the_run_when_the_runner_is_left_with_six_credits_or_fewer() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(8);
        state.corp.installed = vec![ice_installed("whitespace", ServerId::Hq, true)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach whitespace");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes approach window, entering encounter");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes encounter window");
        let (state, events) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes encounter window, firing both subroutines");

        assert_eq!(state.runner.resources.credits, Credits(5), "8 - 3");
        assert!(state.active_run.is_none(), "5 credits is at most 6 — the run should have ended");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunEndedByEffect { .. })));
    }

    /// "When this run ends, trash this program" is a rider on the *break*
    /// ability — Mayfly is only spent by a run in which it broke something.
    /// It used to trash itself at the end of every run, used or not, which
    /// made it a one-run program the moment the Runner ran anywhere. The
    /// break now leaves a hosted counter as the "used this run" marker and
    /// the run-end trigger reads it; the counter leaves with the card.
    #[test]
    fn mayfly_is_not_trashed_by_a_run_in_which_it_broke_nothing() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        let mut mayfly = rig_card_with_counters("mayfly", 0);
        mayfly.base_strength = 1;
        state.runner.rig = vec![mayfly];
        state.corp.archives = vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Archives }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("reach success, no ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("open pre-access window");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassAccessedCard { card_id: CardId("hedge_fund".to_string()) })
                .expect("pass on hedge fund, concluding the run");

        assert_eq!(state.runner.rig.len(), 1, "mayfly stays: it was not used this run");
        assert!(!state.runner.heap.contains(&CardId("mayfly".to_string())));
    }

    #[test]
    fn mayfly_trashes_itself_when_a_run_in_which_it_broke_a_subroutine_ends() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        let mut mayfly = rig_card_with_counters("mayfly", 0);
        mayfly.base_strength = 1;
        state.runner.rig = vec![mayfly];
        // Palisade on a central: strength 2, one "end the run" subroutine.
        state.corp.installed = vec![corp_ice("palisade", ServerId::Hq)];
        state.corp.hq = vec![CardId("hq_card_0".to_string())];

        let state = enter_encounter_with(state, &registry, ServerId::Hq);
        let mayfly = install_of(&state, "mayfly");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: mayfly, ability_index: 1 })
            .expect("pump mayfly to strength 2");
        // Using an ability hands the Corp a chance to respond; it passes.
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: mayfly, ability_index: 0 })
            .expect("break palisade's subroutine");
        assert_eq!(state.runner.rig[0].counters, 1, "the break left the used-this-run marker");

        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit to the access");
        let (state, _) = close_all_windows(state, &registry);
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PassAccessedCard { card_id: CardId("hq_card_0".to_string()) })
                .expect("pass on the accessed card, concluding the run");

        assert!(state.runner.rig.is_empty(), "mayfly should have trashed itself when the run it was used in ended");
        assert!(state.runner.heap.contains(&CardId("mayfly".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardTrashed { side: Side::Runner, .. })));
    }

    #[test]
    fn longevity_serum_lets_the_corp_trash_from_hq_then_shuffle_from_archives_into_rd() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("government_subsidy".to_string())];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1013),
            card: CardId("longevity_serum".to_string()),
            server: ServerId::Remote(0),
            advancement_tokens: 3,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "longevity_serum") })
                .expect("score longevity serum");

        // First ChooseCards: trash 1 of the 2 HQ cards.
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") })
                .expect("toggle hedge_fund");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm HQ trash");

        assert_eq!(state.corp.hq, vec![CardId("government_subsidy".to_string())]);
        assert!(state.corp.archives_contains(&CardId("hedge_fund".to_string())));

        // Second ChooseCards: "shuffle up to 3 from Archives into R&D" —
        // confirm with nothing selected (min 0) to prove it's optional.
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm archives shuffle");

        assert!(state.corp.scored_agendas.iter().any(|s| s.card == CardId("longevity_serum".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { .. })));
    }

    #[test]
    fn sprint_draws_three_then_shuffles_two_hq_cards_into_rd() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq =
            vec![CardId("hedge_fund".to_string()), CardId("government_subsidy".to_string()), CardId("sprint".to_string())];
        state.corp.r_and_d = (0..5).map(|i| CardId(format!("rd_card_{i}"))).collect();

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("sprint".to_string()) })
            .expect("play sprint");

        let rd_before_confirm = state.corp.r_and_d.len();
        let hq_before_confirm: Vec<CardId> = state.corp.hq.clone();
        assert_eq!(hq_before_confirm.len(), 5, "2 starting + 3 drawn, Sprint itself already moved to Archives");

        let pick: Vec<CardId> = hq_before_confirm.into_iter().take(2).collect();
        // HQ order is the selection's own indexing, so the first two cards
        // are positions 0 and 1.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 })
            .expect("toggle first");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 })
            .expect("toggle second");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm sprint shuffle");

        assert_eq!(state.corp.hq.len(), 3, "5 - 2 shuffled back");
        assert_eq!(state.corp.r_and_d.len(), rd_before_confirm + 2);
        assert!(!state.corp.hq.contains(&pick[0]));
        assert!(!state.corp.hq.contains(&pick[1]));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { .. })));
    }

    /// "If there are any cards in HQ, trash 1 of them." The Corp chooses
    /// which: the printed text says nothing about "at random", and under
    /// Null Signal Games' rules a player trashing cards from their own hand
    /// picks them unless the card says otherwise. A previous pass made this
    /// a seeded random trash on the reasoning that choosing "removed the
    /// drawback" — but the drawback is the card, not which card, and the
    /// engine is not the place to re-balance print.
    #[test]
    fn hansei_review_gains_ten_credits_then_lets_the_corp_choose_one_hq_card_to_trash() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq =
            vec![CardId("hansei_review".to_string()), CardId("hedge_fund".to_string()), CardId("offworld_office".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hansei_review".to_string()) })
                .expect("play hansei review");

        assert_eq!(state.corp.resources.credits, Credits(10), "5 - 5 (cost) + 10 (effect)");
        assert!(
            matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, .. })),
            "the Corp picks the card"
        );
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") })
                .expect("toggle hedge fund");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm the trash");

        assert_eq!(state.corp.hq, vec![CardId("offworld_office".to_string())], "the chosen card left HQ; the other stayed");
        assert!(state.corp.archives_contains(&CardId("hedge_fund".to_string())));
    }

    /// With nothing left in HQ after paying for it, the trash clause has
    /// nothing to do and parks no decision — the "if there are any cards in
    /// HQ" is `PromptChooseCards`'s own fewer-than-`min` no-op.
    #[test]
    fn hansei_review_with_an_otherwise_empty_hq_trashes_nothing_and_asks_nothing() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("hansei_review".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hansei_review".to_string()) })
                .expect("play hansei review");

        assert_eq!(state.corp.resources.credits, Credits(10));
        assert!(state.pending_decision.is_none());
        assert!(state.corp.hq.is_empty());
    }

    #[test]
    fn mutual_favor_finds_an_icebreaker_from_the_stack_and_adds_it_to_grip() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = vec![CardId("mutual_favor".to_string())];
        state.runner.stack = vec![CardId("sure_gamble".to_string()), CardId("corroder".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("mutual_favor".to_string()) })
                .expect("play mutual favor");

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "corroder") })
                .expect("toggle corroder (an icebreaker; sure_gamble is not offered)");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm search");

        assert!(state.runner.grip.contains(&CardId("corroder".to_string())));
        assert!(!state.runner.stack.contains(&CardId("corroder".to_string())));
        assert!(events
            .iter()
            .any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { revealed: true, .. })));
        assert!(
            state.pending_decision.is_none(),
            "without a successful run this turn there is no install offer — the card just goes to the grip"
        );
    }

    /// "If you made a successful run this turn, you may install that
    /// program." The install clause was previously unmodelled: the found
    /// icebreaker always went to the grip and the Runner paid a click to
    /// install it later. The search now offers the install — paying the
    /// program's cost — whenever a successful run happened this turn.
    #[test]
    fn mutual_favor_may_install_the_found_icebreaker_after_a_successful_run() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(5);
        state.runner.made_successful_run_this_turn = true;
        state.runner.grip = vec![CardId("mutual_favor".to_string())];
        state.runner.stack = vec![CardId("corroder".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("mutual_favor".to_string()) })
                .expect("play mutual favor");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "corroder") })
                .expect("toggle corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm search");

        assert!(state.runner.grip.contains(&CardId("corroder".to_string())), "the find lands in the grip first");
        assert!(
            matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })),
            "and the Runner is offered the install"
        );
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("install it");
        assert!(state.runner.rig.iter().any(|c| c.card == CardId("corroder".to_string())));
        assert!(!state.runner.grip.contains(&CardId("corroder".to_string())));
        assert_eq!(state.runner.resources.credits, Credits(3), "5 - 2: the install cost is paid, unlike a subroutine install");
    }

    /// Pantograph's second clause — "Then, you may install 1 card from
    /// your grip." — was previously unmodelled entirely. The offer excludes
    /// what an effect install cannot do: a Trojan (host choice), and
    /// anything the Runner cannot pay for.
    #[test]
    fn pantograph_may_install_a_card_from_grip_when_an_agenda_is_scored() {
        let registry = sg_registry();
        let mut state = base_state();
        state.runner.resources.credits = Credits(2);
        state.runner.rig = vec![rig_card_with_counters("pantograph", 0)];
        // corroder (2[c] program) is installable with the credit Pantograph
        // just paid out; botulus is a Trojan and must not be offered.
        state.runner.grip = vec![CardId("corroder".to_string()), CardId("botulus".to_string())];
        state.corp.installed = vec![installed_with_counters("tomorrows_headline", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 3;

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "tomorrows_headline") })
                .expect("score tomorrow's headline");
        assert_eq!(state.runner.resources.credits, Credits(3), "pantograph's 1[c] came first");
        assert!(
            matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })),
            "then the install offer"
        );

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("choose to install");
        let offered = crate::rules::legal_actions(&state, &registry);
        assert!(
            offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position } if *position == 0)),
            "corroder (position 0) is offered"
        );
        assert!(
            !offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position } if *position == 1)),
            "botulus, a Trojan, is not: {offered:?}"
        );
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("toggle corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm install");
        assert!(state.runner.rig.iter().any(|c| c.card == CardId("corroder".to_string())));
        assert_eq!(state.runner.resources.credits, Credits(1), "3 - 2: the install cost is paid");
    }

    /// The other half of the offer filter: with nothing affordable in the
    /// grip, choosing "install" simply finds no eligible card and nothing
    /// is parked — never a dead-end decision.
    #[test]
    fn pantographs_install_offer_finds_no_card_the_runner_cannot_afford() {
        let registry = sg_registry();
        let mut state = base_state();
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![rig_card_with_counters("pantograph", 0)];
        // corroder costs 2; after Pantograph's +1 the Runner has 1.
        state.runner.grip = vec![CardId("corroder".to_string())];
        state.corp.installed = vec![installed_with_counters("tomorrows_headline", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 3;

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "tomorrows_headline") })
                .expect("score tomorrow's headline");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("choose to install");
        assert!(state.pending_decision.is_none(), "no eligible card, so the selection never parks");
        assert_eq!(state.runner.grip, vec![CardId("corroder".to_string())]);
    }

    #[test]
    fn malapert_data_vault_may_search_rd_when_an_agenda_scores_from_its_server() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        state.corp.installed = vec![
            crate::rules::InstalledCard {
                install_id: InstallId(1014),
                card: CardId("malapert_data_vault".to_string()),
                server: ServerId::Remote(0),
                rezzed: true,
                ..Default::default()
            },
            crate::rules::InstalledCard {
                install_id: InstallId(1015),
                card: CardId("hostile_takeover".to_string()),
                server: ServerId::Remote(0),
                advancement_tokens: 2,
                ..Default::default()
            },
        ];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ScoreAgenda { target: install_of(&state, "hostile_takeover") },
        )
        .expect("score hostile takeover from the vault's own server");

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to search R&D");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") })
                .expect("toggle hedge_fund (non-agenda)");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm search");

        assert!(state.corp.hq.contains(&CardId("hedge_fund".to_string())));
        assert!(!state.corp.r_and_d.contains(&CardId("hedge_fund".to_string())));
        assert!(events
            .iter()
            .any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { revealed: true, .. })));
    }

    #[test]
    fn malapert_data_vault_does_not_react_to_an_agenda_scored_from_a_different_server() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.installed = vec![
            crate::rules::InstalledCard {
                install_id: InstallId(1016),
                card: CardId("malapert_data_vault".to_string()),
                server: ServerId::Remote(0),
                rezzed: true,
                ..Default::default()
            },
            crate::rules::InstalledCard {
                install_id: InstallId(1017),
                card: CardId("hostile_takeover".to_string()),
                server: ServerId::Remote(1),
                advancement_tokens: 2,
                ..Default::default()
            },
        ];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ScoreAgenda { target: install_of(&state, "hostile_takeover") },
        )
        .expect("score hostile takeover from a different remote");

        assert!(state.pending_decision.is_none(), "the vault's own server didn't score the agenda");
    }

    /// "You may trash 1 installed resource" — the Corp can decline. A
    /// previous pass removed the opt-out as if the trash were mandatory;
    /// the printed card says "may".
    #[test]
    fn above_the_law_may_trash_an_installed_runner_resource() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1018),
            card: CardId("above_the_law".to_string()),
            server: ServerId::Remote(0),
            advancement_tokens: 3,
            ..Default::default()
        }];
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("verbal_plasticity".to_string()),
            ..Default::default()
        }];

        let (scored, _) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "above_the_law") })
                .expect("score above the law");
        assert!(matches!(scored.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })));

        // Decline: the resource stays.
        let (declined, _) = apply_action(&scored, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("decline");
        assert_eq!(declined.runner.rig.len(), 1);
        assert!(declined.pending_decision.is_none());

        // Accept: pick the resource and trash it.
        let (state, _) = apply_action(&scored, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("accept");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { .. })));
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ToggleCardSelection { position: position_of(&state, "verbal_plasticity") },
        )
        .expect("toggle verbal_plasticity");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm trash");

        assert!(state.runner.rig.is_empty());
        assert!(state.runner.heap.contains(&CardId("verbal_plasticity".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { .. })));
    }

    #[test]
    fn ballista_subroutine_may_trash_an_installed_runner_program() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1019),
            card: CardId("ballista".to_string()),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach ballista");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes approach, entering encounter");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner })
            .expect("runner passes encounter window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes encounter window, firing the subroutine");

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("corp chooses to trash a program");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "corroder") })
                .expect("toggle corroder");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm trash");

        assert!(state.runner.rig.is_empty());
        assert!(state.runner.heap.contains(&CardId("corroder".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { .. })));
    }

    #[test]
    fn retribution_requires_a_tagged_runner_and_trashes_an_installed_program_or_hardware() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("retribution".to_string())];
        state.runner.tags = 1;
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("retribution".to_string()) })
                .expect("play retribution while the runner is tagged");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "corroder") })
                .expect("toggle corroder");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm trash");

        assert!(state.runner.rig.is_empty());
        assert!(state.runner.heap.contains(&CardId("corroder".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { .. })));
    }

    #[test]
    fn retribution_cannot_be_played_against_an_untagged_runner() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("retribution".to_string())];

        let result =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("retribution".to_string()) });
        assert!(result.is_err(), "the Runner isn't tagged, so retribution's play_requirement should reject it");
    }

    #[test]
    fn send_a_message_may_rez_an_installed_ice_for_free_when_scored() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.installed = vec![
            crate::rules::InstalledCard {
                install_id: InstallId(1020),
                card: CardId("send_a_message".to_string()),
                server: ServerId::Remote(0),
                advancement_tokens: 5,
                ..Default::default()
            },
            crate::rules::InstalledCard {
                install_id: InstallId(1021),
                card: CardId("ice_wall".to_string()),
                slot: InstallSlot::Ice,
                ..Default::default()
            },
        ];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ScoreAgenda { target: install_of(&state, "send_a_message") },
        )
        .expect("score send a message");

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to rez an installed ICE");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") })
                .expect("toggle ice_wall");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm rez");

        assert!(state.corp.installed.iter().any(|c| c.card == CardId("ice_wall".to_string()) && c.rezzed));
        assert_eq!(state.corp.resources.credits, Credits(0), "rez was free, ignoring ice_wall's printed cost");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::IceRezzed { .. })));
    }

    /// Scoring *Send a Message* with every installed ice already rezzed
    /// must not park a card-selection nothing can resolve.
    ///
    /// It used to: the filter accepted any installed ice, so
    /// `PromptChooseCards`' "are there at least `min` targets?" guard saw
    /// four and parked — but confirming any of them failed with
    /// `AlreadyRezzed`, so `ConfirmCardSelection` was never legal while the
    /// parked decision blocked every other action. A real game reached this
    /// and hung until the step budget ran out; found by the sample-deck
    /// matchup sweep in `netrunner_single_player`.
    #[test]
    fn send_a_message_no_ops_when_every_installed_ice_is_already_rezzed() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.installed = vec![
            crate::rules::InstalledCard {
                install_id: InstallId(1022),
                card: CardId("send_a_message".to_string()),
                server: ServerId::Remote(0),
                advancement_tokens: 5,
                ..Default::default()
            },
            crate::rules::InstalledCard {
                install_id: InstallId(1023),
                card: CardId("ice_wall".to_string()),
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            },
        ];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ScoreAgenda { target: install_of(&state, "send_a_message") },
        )
        .expect("score send a message");

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choosing the rez option is still legal");

        assert!(
            state.pending_decision.is_none(),
            "with no unrezzed ice the choice must no-op, not park an unresolvable decision"
        );
        assert!(
            !crate::rules::legal_actions(&state, &registry).is_empty(),
            "the Corp must still have somewhere to go"
        );
    }

    /// Already-rezzed ice must not even be offered as a rez target.
    #[test]
    fn send_a_message_only_offers_unrezzed_ice() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.installed = vec![
            crate::rules::InstalledCard {
                install_id: InstallId(1024),
                card: CardId("send_a_message".to_string()),
                server: ServerId::Remote(0),
                advancement_tokens: 5,
                ..Default::default()
            },
            crate::rules::InstalledCard {
                install_id: InstallId(1025),
                card: CardId("ice_wall".to_string()),
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            },
            crate::rules::InstalledCard {
                install_id: InstallId(1026),
                card: CardId("enigma".to_string()),
                slot: InstallSlot::Ice,
                ..Default::default()
            },
        ];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ScoreAgenda { target: install_of(&state, "send_a_message") },
        )
        .expect("score send a message");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to rez an installed ICE");

        let toggles: Vec<usize> = crate::rules::legal_actions(&state, &registry)
            .into_iter()
            .filter_map(|action| match action {
                PlayerAction::ToggleCardSelection { position } => Some(position),
                _ => None,
            })
            .collect();

        assert_eq!(
            toggles,
            vec![position_of(&state, "enigma")],
            "only the unrezzed ice is a legal target"
        );
    }

    #[test]
    fn send_a_message_also_reacts_when_the_agenda_is_stolen() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.resources.credits = Credits(0);
        state.corp.installed = vec![
            crate::rules::InstalledCard {
                install_id: InstallId(1027),
                card: CardId("send_a_message".to_string()),
                server: ServerId::Remote(0),
                ..Default::default()
            },
            crate::rules::InstalledCard {
                install_id: InstallId(1028),
                card: CardId("ice_wall".to_string()),
                slot: InstallSlot::Ice,
                ..Default::default()
            },
        ];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run the remote");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("reach success, no rezzed ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("open pre-access window");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::StealAgenda { card_id: CardId("send_a_message".to_string()) },
        )
        .expect("steal send a message");

        assert!(state.pending_decision.is_some(), "OnAgendaStolen should have parked the same PresentChoice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to rez");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") })
                .expect("toggle ice_wall");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm rez");

        assert!(state.corp.installed.iter().any(|c| c.card == CardId("ice_wall".to_string()) && c.rezzed));
    }

    #[test]
    fn tread_lightly_lets_the_runner_choose_a_server_and_raises_its_ice_rez_cost() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![CardId("tread_lightly".to_string())];
        state.corp.resources.credits = Credits(10);
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1029),
            card: CardId("ice_wall".to_string()),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("tread_lightly".to_string()) })
                .expect("play tread lightly");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq })
                .expect("choose HQ to run");

        assert!(state.active_run.is_some());
        assert_eq!(state.active_run.as_ref().unwrap().ice_rez_cost_modifier, 3);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunInitiated { server: ServerId::Hq })));

        let ice_wall_cost = registry.get(&CardId("ice_wall".to_string())).unwrap().cost;
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach ice_wall");
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "ice_wall") })
            .expect("rez ice_wall at the increased cost");

        assert_eq!(state.corp.resources.credits, Credits(10 - (ice_wall_cost + 3)));
    }

    #[test]
    fn anoetic_void_may_pay_two_and_trash_two_hq_cards_to_end_the_run() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("government_subsidy".to_string())];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1030),
            card: CardId("anoetic_void".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run the remote");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun)
            .expect("reach the approach-server step, firing OnApproachServer");

        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None })
            .expect("corp pays 2 to trash");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") },
        )
        .expect("toggle hedge_fund");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ToggleCardSelection { position: position_of(&state, "government_subsidy") },
        )
        .expect("toggle government_subsidy");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm trash, ending the run");

        assert_eq!(state.corp.resources.credits, Credits(3), "5 - 2");
        assert!(state.corp.hq.is_empty());
        assert_eq!(state.corp.archives.len(), 2);
        assert!(state.active_run.is_none(), "the run should have ended");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunEndedByEffect { .. })));
    }

    /// Anoetic Void's 2[c] is a *cost*, and its two-card trash is what the
    /// cost buys. It used to be `LoseCredits` (saturating, so a Corp on 0
    /// credits ended the run for free) followed by a card selection that
    /// silently no-oped with fewer than two cards in HQ — the Corp paid and
    /// the run went on.
    #[test]
    fn anoetic_void_is_a_real_cost_declinable_unaffordable_and_needs_two_cards_in_hq() {
        let registry = sg_registry();
        let base = |credits: u32, hq: Vec<&str>| {
            let mut state = base_state();
            state.phase = GamePhase::Action(Side::Runner);
            state.runner.resources.clicks = Clicks(4);
            state.corp.resources.credits = Credits(credits);
            state.corp.hq = hq.into_iter().map(|c| CardId(c.to_string())).collect();
            state.corp.installed = vec![crate::rules::InstalledCard {
                install_id: InstallId(1031),
                card: CardId("anoetic_void".to_string()),
                server: ServerId::Remote(0),
                rezzed: true,
                ..Default::default()
            }];
            let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
            apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach the server").0
        };

        // Declined: nothing is paid, the run continues to its pre-access window.
        let state = base(5, vec!["hedge_fund", "government_subsidy"]);
        assert!(state.pending_paid_choice.is_some());
        let (state, _) = apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("decline");
        assert_eq!(state.corp.resources.credits, Credits(5));
        assert_eq!(state.corp.hq.len(), 2);
        assert!(state.active_run.is_some(), "the run goes on");

        // Unaffordable: accepting is not even offered; the Corp can only decline.
        let state = base(1, vec!["hedge_fund", "government_subsidy"]);
        let legal = crate::rules::legal_actions_for(&state, &registry, Side::Corp);
        assert!(!legal.iter().any(|a| matches!(a, PlayerAction::AcceptPendingPaidChoice { .. })), "{legal:?}");
        assert!(legal.contains(&PlayerAction::DeclinePendingPaidChoice));

        // One card in HQ: no offer at all — there is nothing to buy.
        let state = base(5, vec!["hedge_fund"]);
        assert!(state.pending_paid_choice.is_none(), "{:?}", state.pending_paid_choice);
        assert_eq!(state.corp.resources.credits, Credits(5));
        assert!(state.active_run.is_some());
    }

    #[test]
    fn overclock_lets_the_runner_choose_a_server_and_grants_bonus_run_credits() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(1);
        state.runner.grip = vec![CardId("overclock".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("overclock".to_string()) })
                .expect("play overclock");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ChooseServerForPendingDecision { server: ServerId::Archives },
        )
        .expect("choose Archives to run");

        assert_eq!(state.active_run.as_ref().unwrap().bonus_run_credits, 5);
    }

    fn rig_card_with_counters(id: &str, counters: u32) -> crate::rules::InstalledRunnerCard {
        crate::rules::InstalledRunnerCard {
            install_id: fixture_install_id(id),
            card: CardId(id.to_string()),
            counters,
            ..Default::default()
        }
    }

    fn installed_with_counters(id: &str, server: ServerId, counters: u32) -> crate::rules::InstalledCard {
        crate::rules::InstalledCard {
            install_id: fixture_install_id(id),
            card: CardId(id.to_string()),
            server,
            rezzed: true,
            counters,
            ..Default::default()
        }
    }

    #[test]
    fn red_team_installs_with_twelve_counters_and_pays_out_three_credits_on_any_successful_run() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        // 5 activations total (install + 4 ability uses) need 5 clicks —
        // generously past the normal 4-per-turn cap, purely to isolate
        // Red Team's own drain-to-zero behavior in one turn without
        // needing real turn-advancement ceremony in between.
        state.runner.resources.clicks = Clicks(10);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("red_team".to_string())];

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::InstallResource { card_id: CardId("red_team".to_string()) })
                .expect("install red team");
        assert_eq!(state.runner.resources.credits, Credits(5), "10 - 5 (install cost)");
        assert_eq!(state.runner.rig[0].counters, 12);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CountersAdded { amount: 12, .. })));

        // Activate Red Team's own ability once — a partial spend (12 -> 9)
        // should leave the card in play uneventfully.
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "red_team"), ability_index: 0 },
        )
        .expect("activate red team");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq })
                .expect("choose hq");
        let (state, events2) =
            apply_action(&state, &registry, PlayerAction::ContinueRun).expect("resolves immediately, empty hq");
        let (state, events3) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");
        let mut events = events;
        events.extend(events2);
        events.extend(events3);

        assert_eq!(state.runner.rig[0].counters, 9);
        assert_eq!(state.runner.resources.credits, Credits(8), "5 + 3");
        assert!(state.runner.rig.iter().any(|c| c.card == CardId("red_team".to_string())), "still in play");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 3 })));

        // Close out the run so a fresh one can be initiated.
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner pass");
        let (mut state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp pass");

        // Three more successful runs (9 -> 6 -> 3 -> 0) should trash it on
        // the last one. This test compresses four turns into one (see the
        // click count above), so the per-turn record behind "a central
        // server you have not run this turn" is reset by hand between them
        // — `red_team_offers_only_centrals_not_yet_run_this_turn` is the
        // test of that rule.
        for _ in 0..3 {
            state.runner.servers_run_this_turn.clear();
            let (next, _) = apply_action(
                &state,
                &registry,
                PlayerAction::ActivateAbility { target: install_of(&state, "red_team"), ability_index: 0 },
            )
            .expect("activate red team");
            let (next, _) =
                apply_action(&next, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq })
                    .expect("choose hq");
            let (next, _) = apply_action(&next, &registry, PlayerAction::ContinueRun).expect("resolves immediately");
            let (next, _) = apply_action(&next, &registry, PlayerAction::CompleteRun).expect("complete run");
            let (next, _) = apply_action(&next, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner pass");
            let (next, _) = apply_action(&next, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp pass");
            state = next;
        }

        assert!(state.runner.rig.is_empty(), "should have trashed itself once drained to zero");
        assert!(state.runner.heap.contains(&CardId("red_team".to_string())));
        assert_eq!(state.runner.resources.credits, Credits(8 + 9), "8 + 3 credits x 3 more successful runs");
    }

    /// The payout is the rider on Red Team's *own* run ("If successful,
    /// take 3[c] from this resource"), not a reaction to every successful
    /// run. This used to be a `Trigger::OnSuccessfulRun`, so a plain basic
    /// action run paid out too — and on remotes, which the click cannot
    /// even target.
    #[test]
    fn red_team_does_not_pay_out_on_a_run_it_did_not_start() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![rig_card_with_counters("red_team", 12)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::RnD }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("resolves immediately, empty rd");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");

        assert_eq!(state.runner.rig[0].counters, 12, "a basic-action run is not Red Team's run");
        assert_eq!(state.runner.resources.credits, Credits(0));
    }

    /// "[click]: Run a **central** server" — the server choice offers only
    /// centrals. ("…you have not run this turn" is still unmodelled; see
    /// ROADMAP.)
    #[test]
    fn red_teams_click_offers_central_servers_only() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = vec![rig_card_with_counters("red_team", 12)];
        state.corp.installed = vec![installed_with_counters("nico_campaign", ServerId::Remote(0), 0)];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "red_team"), ability_index: 0 },
        )
        .expect("click red team");
        let offered: Vec<ServerId> = crate::rules::legal_actions_for(&state, &registry, Side::Runner)
            .into_iter()
            .filter_map(|a| match a {
                PlayerAction::ChooseServerForPendingDecision { server } => Some(server),
                _ => None,
            })
            .collect();
        assert!(offered.contains(&ServerId::Hq) && offered.contains(&ServerId::RnD) && offered.contains(&ServerId::Archives), "{offered:?}");
        assert!(!offered.iter().any(|s| matches!(s, ServerId::Remote(_))), "no remote: {offered:?}");
    }

    fn red_team_state() -> GameState {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = vec![rig_card_with_counters("red_team", 12)];
        state.corp.installed = vec![installed_with_counters("nico_campaign", ServerId::Remote(0), 0)];
        state
    }

    /// Runs the run out to completion and drains the closing window, so
    /// the next action of the turn is a plain action-phase one.
    fn finish_run(state: GameState, registry: &CardRegistry) -> GameState {
        let (state, _) = apply_action(&state, registry, PlayerAction::ContinueRun).expect("no ice: straight to the server");
        let (state, _) = apply_action(&state, registry, PlayerAction::CompleteRun).expect("complete run");
        let (state, _) = apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner pass");
        let (state, _) = apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp pass");
        state
    }

    fn offered_servers(state: &GameState, registry: &CardRegistry) -> Vec<ServerId> {
        crate::rules::legal_actions_for(state, registry, Side::Runner)
            .into_iter()
            .filter_map(|a| match a {
                PlayerAction::ChooseServerForPendingDecision { server } => Some(server),
                _ => None,
            })
            .collect()
    }

    /// "Run a central server **you have not run this turn**": a central run
    /// by click earlier in the turn is not offered, and a run Red Team
    /// itself started counts too.
    #[test]
    fn red_team_offers_only_centrals_not_yet_run_this_turn() {
        let registry = sg_registry();
        let state = red_team_state();
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("click run on hq");
        assert_eq!(state.runner.servers_run_this_turn, vec![ServerId::Hq]);
        let state = finish_run(state, &registry);

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "red_team"), ability_index: 0 },
        )
        .expect("click red team");
        let offered = offered_servers(&state, &registry);
        assert_eq!(offered, vec![ServerId::RnD, ServerId::Archives], "HQ was run this turn");
        assert!(
            apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).is_err(),
            "a directly submitted HQ is refused too"
        );

        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD })
            .expect("choose r&d");
        assert_eq!(state.runner.servers_run_this_turn, vec![ServerId::Hq, ServerId::RnD], "Red Team's own run is recorded");
    }

    /// Once every central has been run this turn the click is not offered
    /// at all — the effect refuses before parking, so no click is sunk into
    /// a decision nothing can resolve.
    #[test]
    fn red_teams_click_is_not_offered_once_every_central_was_run_this_turn() {
        let registry = sg_registry();
        let mut state = red_team_state();
        state.runner.resources.clicks = Clicks(6);
        for server in [ServerId::Hq, ServerId::RnD, ServerId::Archives] {
            let (next, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server }).expect("click run");
            state = finish_run(next, &registry);
        }
        let red_team = install_of(&state, "red_team");
        let legal = crate::rules::legal_actions_for(&state, &registry, Side::Runner);
        assert!(
            !legal.contains(&PlayerAction::ActivateAbility { target: red_team, ability_index: 0 }),
            "Red Team must not be offered: {legal:?}"
        );
        assert!(matches!(
            apply_action(&state, &registry, PlayerAction::ActivateAbility { target: red_team, ability_index: 0 }),
            Err(crate::rules::RulesError::NoServerLeftToRun)
        ));
        assert!(state.pending_decision.is_none(), "nothing parked");
    }

    /// The record is per turn: it survives the rest of the turn and clears
    /// when the Runner's next turn starts.
    #[test]
    fn servers_run_this_turn_resets_at_the_runners_next_turn() {
        let registry = sg_registry();
        let state = red_team_state();
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Archives }).expect("run");
        let state = finish_run(state, &registry);
        assert_eq!(state.runner.servers_run_this_turn, vec![ServerId::Archives]);

        // Drive through the end-of-turn window, the Corp's whole turn and
        // its window, passing and ending turns as they come up, until the
        // Runner's next turn has started (`turn` advanced twice). The Corp
        // needs something to draw, or its turn never starts.
        let runner_turn = state.turn;
        let mut state = state;
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 3];
        let mut seen_corp_turn = false;
        while state.turn < runner_turn + 2 {
            let actor = crate::rules::current_actor(&state).expect("someone always has a decision");
            if state.turn == runner_turn + 1 && !seen_corp_turn {
                seen_corp_turn = true;
                assert_eq!(state.runner.servers_run_this_turn, vec![ServerId::Archives], "still recorded on the Corp's turn");
            }
            let legal = crate::rules::legal_actions_for(&state, &registry, actor);
            let action = legal
                .iter()
                .find(|a| matches!(a, PlayerAction::PassPriority { .. }))
                .or_else(|| legal.iter().find(|a| matches!(a, PlayerAction::EndTurn)))
                .or_else(|| legal.first())
                .cloned()
                .expect("a legal action");
            state = apply_action(&state, &registry, action).expect("drive to the Runner's next turn").0;
        }
        assert!(seen_corp_turn);
        assert!(state.runner.servers_run_this_turn.is_empty(), "cleared when the Runner's turn began");
    }

    #[test]
    fn telework_contract_installs_with_nine_counters_and_pays_out_three_credits_once_per_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("telework_contract".to_string())];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallResource { card_id: CardId("telework_contract".to_string()) },
        )
        .expect("install telework contract");
        assert_eq!(state.runner.resources.credits, Credits(9), "10 - 1 (install cost)");
        assert_eq!(state.runner.rig[0].counters, 9);

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "telework_contract"), ability_index: 0 },
        )
        .expect("first activation this turn");
        assert_eq!(state.runner.rig[0].counters, 6, "partial spend leaves it in play");
        assert_eq!(state.runner.resources.credits, Credits(12), "9 + 3");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 3 })));

        let second_attempt = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "telework_contract"), ability_index: 0 },
        );
        assert!(second_attempt.is_err(), "once per turn should block a second activation the same turn");
    }

    #[test]
    fn telework_contract_trashes_itself_when_drained_to_zero() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![rig_card_with_counters("telework_contract", 3)];

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "telework_contract"), ability_index: 0 },
        )
        .expect("final activation drains to zero");

        assert!(state.runner.rig.is_empty(), "should have trashed itself");
        assert!(state.runner.heap.contains(&CardId("telework_contract".to_string())));
        assert_eq!(state.runner.resources.credits, Credits(3));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardTrashed { side: Side::Runner, .. })));
    }

    #[test]
    fn regolith_mining_license_installs_with_fifteen_counters_and_pays_out_three_credits_per_click() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.resources.clicks = Clicks(3);
        state.corp.hq = vec![CardId("regolith_mining_license".to_string())];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard {
                card_id: CardId("regolith_mining_license".to_string()),
                zone: ServerId::Remote(0),
                slot: InstallSlot::Root,
            },
        )
        .expect("install regolith mining license");
        assert_eq!(state.corp.resources.credits, Credits(10), "installing an asset is free");

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "regolith_mining_license") })
                .expect("rez regolith mining license");
        assert_eq!(state.corp.installed[0].counters, 15);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CountersAdded { amount: 15, .. })));

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "regolith_mining_license"), ability_index: 0 },
        )
        .expect("activate regolith mining license");
        assert_eq!(state.corp.installed[0].counters, 12, "partial spend leaves it in play");
        assert_eq!(state.corp.resources.credits, Credits(11), "10 - 2 (rez pays the printed cost) + 3");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 3 })));
    }

    #[test]
    fn regolith_mining_license_trashes_itself_when_drained_to_zero() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.resources.clicks = Clicks(3);
        state.corp.installed = vec![installed_with_counters("regolith_mining_license", ServerId::Remote(0), 3)];

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "regolith_mining_license"), ability_index: 0 },
        )
        .expect("final activation drains to zero");

        assert!(state.corp.installed.is_empty(), "should have trashed itself");
        assert!(state.corp.archives_contains(&CardId("regolith_mining_license".to_string())));
        assert_eq!(state.corp.resources.credits, Credits(3));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardTrashed { side: Side::Corp, .. })));
    }

    #[test]
    fn nico_campaign_installs_with_nine_counters_and_pays_out_three_credits_at_corps_turn_start() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.hq = vec![CardId("nico_campaign".to_string())];
        state.corp.r_and_d = vec![CardId("filler_card".to_string())];
        state.corp.resources.credits = Credits(10);
        state.corp.resources.clicks = Clicks(3);

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard {
                card_id: CardId("nico_campaign".to_string()),
                zone: ServerId::Remote(0),
                slot: InstallSlot::Root,
            },
        )
        .expect("install nico campaign");
        assert_eq!(state.corp.resources.credits, Credits(10), "installing is free");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "nico_campaign") })
                .expect("rez nico campaign");
        assert_eq!(state.corp.resources.credits, Credits(8), "10 - 2 (rez cost)");
        assert_eq!(state.corp.installed[0].counters, 9);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CountersAdded { amount: 9, .. })));

        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, mut events) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, close_events) = close_all_windows(state, &registry);
        events.extend(close_events);

        assert_eq!(state.corp.installed[0].counters, 6, "partial spend leaves it in play");
        assert_eq!(state.corp.resources.credits, Credits(11), "8 + 3");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 3 })));
    }

    #[test]
    fn nico_campaign_trashes_itself_and_draws_a_card_when_drained_to_zero() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.resources.clicks = Clicks(3);
        // Two cards in R&D: one for the mandatory start-of-turn draw, one
        // for Nico Campaign's own "trash it and draw 1 card" side effect.
        state.corp.r_and_d = vec![CardId("filler_card".to_string()), CardId("filler_card".to_string())];
        state.corp.installed = vec![installed_with_counters("nico_campaign", ServerId::Remote(0), 3)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, mut events) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, close_events) = close_all_windows(state, &registry);
        events.extend(close_events);

        assert!(state.corp.installed.is_empty(), "should have trashed itself");
        assert!(state.corp.archives_contains(&CardId("nico_campaign".to_string())));
        assert_eq!(state.corp.resources.credits, Credits(3));
        assert_eq!(
            state.corp.hq,
            vec![CardId("filler_card".to_string()), CardId("filler_card".to_string())],
            "the mandatory draw and Nico Campaign's own draw should both have landed in HQ"
        );
    }

    #[test]
    fn smartware_distributor_loads_credits_and_pays_out_one_per_turn_while_it_has_any() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("smartware_distributor".to_string())];
        // A non-empty R&D — an empty one would deck the Corp out the
        // moment their turn (reached by ending the Runner's turn below)
        // attempts its mandatory draw, ending the game before this test's
        // second `EndTurn` runs.
        state.corp.r_and_d = vec![CardId("filler_card".to_string())];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallResource { card_id: CardId("smartware_distributor".to_string()) },
        )
        .expect("install smartware distributor");
        assert_eq!(state.runner.resources.credits, Credits(10), "cost 0");

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "smartware_distributor"), ability_index: 0 },
        )
        .expect("load 3 credits onto it");
        assert_eq!(state.runner.rig[0].counters, 3);

        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, mut events) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, close_events) = close_all_windows(state, &registry);
        events.extend(close_events);

        assert_eq!(state.runner.rig[0].counters, 2, "1 taken at the runner's next turn start");
        assert_eq!(state.runner.resources.credits, Credits(11), "10 + 1");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 1 })));
    }

    #[test]
    fn smartware_distributor_does_not_trash_itself_when_its_counters_reach_zero() {
        // Unlike Red Team/Telework Contract/Regolith Mining
        // License/Nico Campaign, Smartware Distributor's real text has no
        // "when it is empty, trash it" clause — confirmed against
        // `system_gateway.json`'s card 30033 before implementing.
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![rig_card_with_counters("smartware_distributor", 1)];
        state.corp.r_and_d = vec![CardId("filler_card".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);

        assert_eq!(state.runner.rig[0].counters, 0);
        assert_eq!(state.runner.resources.credits, Credits(1));
        assert!(
            state.runner.rig.iter().any(|c| c.card == CardId("smartware_distributor".to_string())),
            "should remain installed at zero counters, unlike the auto-trashing hosted-credit cards"
        );
    }

    #[test]
    fn pennyshaver_gains_a_counter_per_successful_run_and_takes_all_hosted_credits() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("pennyshaver".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallHardware { card_id: CardId("pennyshaver".to_string()) })
                .expect("install pennyshaver");
        assert_eq!(state.runner.resources.credits, Credits(7), "10 - 3 (install cost)");

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ContinueRun).expect("resolves immediately, empty hq");
        let (state, events) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");
        assert_eq!(state.runner.rig[0].counters, 1, "gained 1 counter from the successful run");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CountersAdded { amount: 1, .. })));

        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner pass");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp pass");

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "pennyshaver"), ability_index: 0 },
        )
        .expect("place 1 more then take all hosted credits");
        assert_eq!(state.runner.rig[0].counters, 0, "took every hosted credit, leaving none behind");
        assert_eq!(state.runner.resources.credits, Credits(9), "7 + 2 (1 already hosted, 1 placed, both taken)");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 2 })));
    }

    #[test]
    fn installing_a_console_grants_its_memory_bonus() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.memory_units = crate::rules::MemoryUnits(4);
        state.runner.grip = vec![CardId("carnivore".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallHardware { card_id: CardId("carnivore".to_string()) })
                .expect("install carnivore");

        assert_eq!(state.runner.memory_units, crate::rules::MemoryUnits(5), "4 base + 1 from Carnivore's console MU bonus");
    }

    #[test]
    fn a_second_console_is_rejected_and_never_offered_by_the_mask() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(20);
        state.runner.grip = vec![CardId("carnivore".to_string()), CardId("pantograph".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallHardware { card_id: CardId("carnivore".to_string()) })
                .expect("first console installs fine");

        let result =
            apply_action(&state, &registry, PlayerAction::InstallHardware { card_id: CardId("pantograph".to_string()) });
        assert_eq!(result, Err(crate::rules::RulesError::ConsoleLimitExceeded));

        let mask = crate::rules::get_action_mask(&state, &registry);
        let legal = crate::rules::legal_actions(&state, &registry);
        assert!(
            !legal.contains(&PlayerAction::InstallHardware { card_id: CardId("pantograph".to_string()) }),
            "a second console must never appear in legal_actions"
        );
        // `ActionSpace`'s own roundtrip/mask-agreement tests (action_mask.rs)
        // cover the general index<->action<->mask consistency machinery;
        // this just confirms this specific illegal action is consistently
        // excluded from both views the mask is built from.
        assert_eq!(mask.len(), crate::rules::ActionSpace::SIZE);
    }

    #[test]
    fn carnivore_ability_is_only_offered_while_mid_access_of_a_specific_card() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.rig = vec![rig_card_with_counters("carnivore", 0)];
        state.runner.grip = vec![CardId("grip_card_a".to_string()), CardId("grip_card_b".to_string())];

        // No active run at all: the ability's `CurrentlyAccessingACard`
        // requirement must reject activation outright.
        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "carnivore"), ability_index: 0 },
        );
        assert_eq!(result, Err(crate::rules::RulesError::RequirementNotMet));
        let legal = crate::rules::legal_actions(&state, &registry);
        assert!(!legal.contains(&PlayerAction::ActivateAbility { target: install_of(&state, "carnivore"), ability_index: 0 }));
    }

    #[test]
    fn carnivore_trashes_the_accessed_card_for_free_by_trashing_two_grip_cards() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        // `PromptChooseCards`/`eligible_cards` only offers cards the
        // `CardRegistry` actually knows about (needed to check `filter`
        // against), so the 2 grip filler cards trashed as Carnivore's
        // "cost" must be real registered cards, not synthetic placeholder
        // ids — unlike plain zone-move tests elsewhere in this file that
        // never run their contents through a card-selection filter.
        state.runner.grip = vec![
            CardId("carnivore".to_string()),
            CardId("sure_gamble".to_string()),
            CardId("diesel".to_string()),
        ];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: fixture_install_id("pad_campaign"),
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallHardware { card_id: CardId("carnivore".to_string()) })
                .expect("install carnivore");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to the approach step");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes access window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes success window, presenting the pending choice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner })
            .expect("runner passes pending-choice window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes pending-choice window");

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "carnivore"), ability_index: 0 },
        )
        .expect("activating parks the grip-selection decision");

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ToggleCardSelection { position: position_of(&state, "sure_gamble") },
        )
        .expect("select first grip card");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ToggleCardSelection { position: position_of(&state, "diesel") },
        )
        .expect("select second grip card");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection)
            .expect("confirm trashes the 2 grip cards, then the accessed card for free");

        assert!(state.runner.grip.is_empty());
        assert_eq!(state.runner.heap.len(), 2, "the 2 trashed grip cards");
        assert!(
            state.corp.archives_contains(&CardId("pad_campaign".to_string())),
            "the accessed card should be trashed for free, skipping its trash_cost entirely"
        );
        assert!(!state.corp.installed.iter().any(|c| c.card == CardId("pad_campaign".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardTrashedFromAccess { cost_paid: 0, .. })));
        assert!(state.runner.once_per_turn_used.iter().any(|k| k.tag == "carnivore"));
    }

    #[test]
    fn pantograph_gains_a_credit_on_both_agenda_scored_and_agenda_stolen() {
        let registry = sg_registry();
        let mut state = base_state();
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![rig_card_with_counters("pantograph", 0)];
        state.corp.installed = vec![installed_with_counters("offworld_office", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 4;

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "offworld_office") })
                .expect("score offworld office");
        assert!(
            events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 1 })),
            "Pantograph should react to the Corp's own agenda score now that AgendaScored reaches the Runner side"
        );
        assert_eq!(state.runner.resources.credits, Credits(1));

        // Zero-regression check for the same widening: Gabriel Santiago's
        // unrelated `OnSuccessfulRunOnHq` identity trigger (a completely
        // different event/trigger pair) is unaffected — already covered by
        // that card's own passing baseline test elsewhere in this suite;
        // the widening here only touches `AgendaScored`/`AgendaStolen`'s own
        // audience computation.
    }

    #[test]
    fn dzmz_optimizer_discounts_the_first_program_installed_each_turn_but_not_hardware() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.rig = vec![rig_card_with_counters("dzmz_optimizer", 0)];
        state.runner.grip = vec![CardId("corroder".to_string()), CardId("gordian_blade".to_string())];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgram { card_id: CardId("corroder".to_string()) },
        )
        .expect("install corroder");
        assert_eq!(state.runner.resources.credits, Credits(9), "10 - 2 (cost) + 1 (DZMZ discount)");
        assert!(state.runner.first_install_discount_used_this_turn);

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgram { card_id: CardId("gordian_blade".to_string()) },
        )
        .expect("install gordian blade");
        assert_eq!(state.runner.resources.credits, Credits(5), "9 - 4 (cost), discount already used this turn");
    }

    #[test]
    fn t400_memory_diamond_grants_memory_and_max_hand_size() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.memory_units = crate::rules::MemoryUnits(4);
        state.runner.grip = vec![CardId("t400_memory_diamond".to_string())];
        state.runner.grip.extend((0..6).map(|i| CardId(format!("filler_{i}"))));

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallHardware { card_id: CardId("t400_memory_diamond".to_string()) },
        )
        .expect("install t400 memory diamond");

        assert_eq!(state.runner.memory_units, crate::rules::MemoryUnits(5));
        assert_eq!(state.runner.max_hand_size_bonus, 1);
        assert_eq!(state.runner.grip.len(), 6, "t400 itself left the grip on install, 6 filler cards remain");

        // End-to-end proof the bonus actually raises the enforced max hand
        // size (not just the bookkeeping field): 6 cards exceeds the base
        // limit of 5 but exactly fits the bonus-adjusted limit (5 + 1), so
        // ending the Runner's turn here must NOT enter `GamePhase::Discard`.
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn within the raised limit");
        assert!(!matches!(state.phase, GamePhase::Discard { .. }), "6 cards should fit under the +1 max hand size bonus");
    }

    #[test]
    fn carmen_install_cost_is_only_discounted_after_a_successful_run_this_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("carmen".to_string())];

        let (state_no_run, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgram { card_id: CardId("carmen".to_string()) },
        )
        .expect("install carmen without a successful run this turn");
        assert_eq!(state_no_run.runner.resources.credits, Credits(5), "10 - 5, no discount");

        state.runner.made_successful_run_this_turn = true;
        let (state_after_run, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgram { card_id: CardId("carmen".to_string()) },
        )
        .expect("install carmen after a successful run this turn");
        assert_eq!(state_after_run.runner.resources.credits, Credits(7), "10 - 5 + 2 (discount)");
    }

    #[test]
    fn marjanah_break_ability_is_only_discounted_after_a_successful_run_this_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        // Base strength set to match Wall of Static's printed strength
        // directly (bypassing `rig_card_with_counters`'s 0 default) so this
        // test isolates the ability's credit-cost discount, not the
        // separate breaker-strength-vs-ICE-strength gate.
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("marjanah".to_string()),
            base_strength: 3,
            ..Default::default()
        }];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1034),
            card: CardId("wall_of_static".to_string()),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach wall of static");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes approach window, entering encounter");

        let (state_no_run, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "marjanah"), ability_index: 0 },
        )
        .expect("break without a successful run this turn");
        assert_eq!(state_no_run.runner.resources.credits, Credits(8), "10 - 2, no discount");

        let mut state_with_run = state;
        state_with_run.runner.made_successful_run_this_turn = true;
        let (state_after_run, _) = apply_action(
            &state_with_run,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state_with_run, "marjanah"), ability_index: 0 },
        )
        .expect("break after a successful run this turn");
        assert_eq!(state_after_run.runner.resources.credits, Credits(9), "10 - 1 (discounted)");
    }

    #[test]
    fn superconducting_hub_grants_max_hand_size_and_offers_an_optional_draw() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.r_and_d = (0..2).map(|i| CardId(format!("rd_card_{i}"))).collect();
        state.corp.installed = vec![installed_with_counters("superconducting_hub", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 3;

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "superconducting_hub") })
                .expect("score superconducting hub");
        assert_eq!(state.corp.max_hand_size_bonus, 2);

        let (state, events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("corp chooses to draw 2");
        assert_eq!(state.corp.hq.len(), 2);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardDrawn { side: Side::Corp })));
    }

    #[test]
    fn haas_bioroid_precision_design_may_add_an_archives_card_to_hq_on_agenda_score() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.identity = Some(CardId("haas_bioroid_precision_design".to_string()));
        // Same registered-card requirement as Carnivore's test above —
        // `PromptChooseCards` filters candidates through the registry.
        state.corp.archives = vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))];
        state.corp.installed = vec![installed_with_counters("offworld_office", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 4;

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "offworld_office") })
                .expect("score offworld office");

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ResolvePendingChoice { option_index: 0 },
        )
        .expect("corp chooses to add an archives card to hq");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") },
        )
        .expect("select the archived card");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection)
            .expect("confirm moves it to hq");

        assert!(state.corp.archives.is_empty());
        assert!(state.corp.hq.contains(&CardId("hedge_fund".to_string())));

        assert_eq!(
            registry.get(&CardId("haas_bioroid_precision_design".to_string())).unwrap().max_hand_size_bonus,
            Some(1),
            "the identity's max-hand-size bonus is applied at GameState::setup, not exercised by this base_state()-driven test"
        );
    }

    fn corp_ice(id: &str, server: ServerId) -> crate::rules::InstalledCard {
        crate::rules::InstalledCard {
            install_id: fixture_install_id(id),
            card: CardId(id.to_string()),
            server,
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }
    }

    /// Hosting a Trojan on **unrezzed** ICE is legal, and must not tell the
    /// Runner what that ICE is.
    ///
    /// The second of the two identity leaks `InstallId` closed, and the one
    /// the roadmap did not list: `install_program_on_ice_candidates` pairs
    /// every Trojan with every installed piece of ICE — its own doc comment
    /// says "rezzed or not — real rules allow hosting on unrezzed ICE" —
    /// and it used to carry the host's real `CardId`. `action_owner`
    /// assigns the action to the Runner, so the identity of a card their
    /// own `ClientView` masks to `None` was written straight into a
    /// candidate for `legal_actions_for(Runner)`.
    ///
    /// **It was latent when written, and is live now.** `legal_actions`
    /// keeps only candidates `apply_action` accepts, and the candidate used
    /// to be built with `memory_cost: 0` while `install_program_on_ice`
    /// demanded the registry's declared value — `1` for both *Botulus* and
    /// *Tranquilizer* — so the leaking candidate was filtered out before
    /// reaching any view. That filter was its own bug and is now fixed, so
    /// this action finally reaches a `ClientView` and the last assertion
    /// here does real work rather than passing vacuously.
    ///
    /// Withdrawing the action was never an option — real Netrunner allows
    /// the host to be unrezzed — so the fix is that it names the install.
    #[test]
    fn hosting_a_trojan_on_unrezzed_ice_resolves_without_naming_the_ice() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("botulus".to_string())];
        state.corp.installed = vec![crate::rules::InstalledCard {
            card: CardId("wall_of_static".to_string()),
            install_id: crate::rules::InstallId(1),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
            ..Default::default()
        }];

        let view = crate::view::build_client_view(&state, &registry, Side::Runner);
        let hq = view.corp.servers.iter().find(|s| s.server == ServerId::Hq).expect("HQ has ice");
        assert_eq!(hq.ice[0].card, None, "the premise: the Runner cannot identify this ice");

        // The rules half: an unrezzed host is a legal host, and the action
        // resolves against the real card underneath without the Runner
        // having had to name it.
        let (hosted, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgramOnIce {
                card_id: CardId("botulus".to_string()),
                host: crate::rules::InstallId(1),
            },
        )
        .expect("botulus hosts on unrezzed ice");
        assert_eq!(hosted.runner.rig[0].hosted_on_ice, Some(install_of(&hosted, "wall_of_static")));

        // Offered, not merely resolvable — which only became true once the
        // memory-cost filter was fixed, and is what gives the assertion
        // below something to be true *of*.
        assert!(
            view.legal_actions.iter().any(|a| matches!(
                a,
                PlayerAction::InstallProgramOnIce { host, .. } if *host == crate::rules::InstallId(1)
            )),
            "hosting on unrezzed ice is a legal action: {:?}",
            view.legal_actions
        );

        // The masking half, and the one that regresses if anyone reverts
        // the payload to a `CardId`: nothing the Runner is offered may name
        // the ice. Stated over the whole action list rather than over
        // `InstallProgramOnIce` alone.
        assert!(
            !format!("{:?}", view.legal_actions).contains("wall_of_static"),
            "no legal action may name an ice this view masks: {:?}",
            view.legal_actions
        );
    }

    #[test]
    fn botulus_installs_on_ice_gains_counters_and_only_breaks_while_encountering_its_host() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("botulus".to_string())];
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Hq)];

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgramOnIce {
                card_id: CardId("botulus".to_string()),
                host: install_of(&state, "wall_of_static"),
            },
        )
        .expect("install botulus onto wall of static");
        assert_eq!(state.runner.rig[0].hosted_on_ice, Some(install_of(&state, "wall_of_static")));
        assert_eq!(state.runner.rig[0].counters, 1, "OnInstall places 1 virus counter");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CountersAdded { amount: 1, .. })));

        // Not currently encountering the host — the ability must not be legal.
        let mask = crate::rules::get_action_mask(&state, &registry);
        let index = crate::rules::ActionSpace::index_of(
            &state,
            &PlayerAction::ActivateAbility { target: install_of(&state, "botulus"), ability_index: 0 },
        )
        .expect("the action is always encodable regardless of legality");
        assert!(!mask[index], "Botulus's ability must not be legal outside an encounter with its own host");
        assert!(apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "botulus"), ability_index: 0 }
        )
        .is_err());

        // Actually encounter wall_of_static.
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach wall_of_static");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes, entering encounter");

        let mask = crate::rules::get_action_mask(&state, &registry);
        assert!(mask[index], "now legal, mid-encounter with its host");

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "botulus"), ability_index: 0 },
        )
        .expect("break a subroutine using the hosted counter");
        assert_eq!(state.runner.rig[0].counters, 0, "the hosted counter was spent as the ability's cost");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { index: 0, .. })));
    }

    /// Runs `EndTurn` through to completion (including whatever
    /// mandatory-draw/`OnTurnStart` reactions `enter_start_of_turn` fires
    /// for the new side) and closes the `StartOfTurn` window it opens —
    /// the common "advance one full turn boundary" shape every step below
    /// reuses.
    fn end_turn_and_settle(state: GameState, registry: &CardRegistry) -> (GameState, Vec<crate::rules::GameEvent>) {
        let (state, events) = apply_action(&state, registry, PlayerAction::EndTurn).expect("end turn");
        let (state, more_events) = close_all_windows(state, registry);
        let mut all = events;
        all.extend(more_events);
        (state, all)
    }

    #[test]
    fn tranquilizer_derezzes_its_host_once_it_accumulates_three_virus_counters() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("tranquilizer".to_string())];
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Hq)];
        state.corp.resources.clicks = Clicks(3);
        // Two Corp turn starts happen along the way (mandatory draw each
        // time) — an empty R&D would end the game in the Runner's favor
        // via deck-out before Tranquilizer ever reaches 3 counters.
        state.corp.r_and_d = vec![CardId("filler_0".to_string()), CardId("filler_1".to_string())];

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgramOnIce {
                card_id: CardId("tranquilizer".to_string()),
                host: install_of(&state, "wall_of_static"),
            },
        )
        .expect("install tranquilizer");
        assert_eq!(state.runner.rig[0].counters, 1, "OnInstall places the first counter");
        assert!(state.corp.installed[0].rezzed, "only 1 counter — not derezzed yet");
        assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardDerezzed { .. })));

        // Drive it to 3 counters via two more Runner `OnTurnStart`s — each
        // requires a full Runner-ends/Corp-ends round trip, since
        // Tranquilizer only reacts to its own controller's turn starting.
        let (state, _) = end_turn_and_settle(state, &registry); // Runner ends -> Corp's turn.
        let (state, _) = end_turn_and_settle(state, &registry); // Corp ends -> Runner's turn: counter -> 2.
        assert_eq!(state.runner.rig[0].counters, 2);
        assert!(state.corp.installed[0].rezzed, "still rezzed at 2 counters");

        let (state, _) = end_turn_and_settle(state, &registry); // Runner ends -> Corp's turn.
        let (state, events) = end_turn_and_settle(state, &registry); // Corp ends -> Runner's turn: counter -> 3, derez.

        assert_eq!(state.runner.rig[0].counters, 3);
        assert!(!state.corp.installed[0].rezzed, "3 counters should have derezzed the host ice");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardDerezzed { card } if card.0 == "wall_of_static")));
    }

    #[test]
    fn leech_gains_a_counter_only_from_a_central_server_run_and_spends_it_to_weaken_ice() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("leech".to_string()),
            ..Default::default()
        }];
        // Both remotes below start with no ICE — `run_to_completion` only
        // works against an ICE-free server (it always calls `CompleteRun`
        // straight after one `ContinueRun`); ICE is installed fresh, on a
        // third remote, only for the final mid-encounter step.

        // A run on a remote server should NOT add a counter.
        let (state, _) = run_to_completion(state, &registry, ServerId::Remote(0));
        assert_eq!(state.runner.rig[0].counters, 0, "remote server runs don't feed Leech");

        // A run on a central server (HQ, empty) should.
        let (state, _) = run_to_completion(state, &registry, ServerId::Hq);
        assert_eq!(state.runner.rig[0].counters, 1, "central server runs feed Leech");

        // Spend the hosted counter mid-encounter to weaken the ICE.
        let mut state = state;
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Remote(1))];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(1) })
            .expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach wall_of_static");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes, entering encounter");

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "leech"), ability_index: 0 },
        )
        .expect("spend the hosted counter to weaken the ice");
        assert_eq!(state.runner.rig[0].counters, 0);
        assert_eq!(state.active_run.as_ref().unwrap().ice[0].current_strength, 2, "3 - 1 (Leech)");
    }

    #[test]
    fn fermenter_pays_out_two_credits_per_hosted_counter_and_trashes_itself() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("fermenter".to_string()),
            counters: 3,
            ..Default::default()
        }];

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "fermenter"), ability_index: 0 },
        )
        .expect("cash out fermenter");

        assert_eq!(state.runner.resources.credits, Credits(11), "5 + 3 * 2");
        assert!(state.runner.rig.is_empty(), "fermenter trashes itself");
        assert!(state.runner.heap.contains(&CardId("fermenter".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 6 })));
    }

    #[test]
    fn cookbook_may_place_a_counter_on_a_newly_installed_virus_program_but_not_on_itself() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("cookbook".to_string()),
            ..Default::default()
        }];
        state.runner.grip = vec![CardId("leech".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("leech".to_string()) })
                .expect("install leech");
        // Leech's own OnInstall doesn't place a counter (only Cookbook's
        // optional reaction can, and only Leech's OnSuccessfulRun does) —
        // so before resolving Cookbook's choice, Leech has 0 counters.
        assert_eq!(state.runner.rig.iter().find(|c| c.card.0 == "leech").unwrap().counters, 0);

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("cookbook chooses to place a counter");

        assert_eq!(
            state.runner.rig.iter().find(|c| c.card.0 == "leech").unwrap().counters,
            1,
            "Cookbook's effect targets the installed virus program, not itself"
        );
        assert_eq!(
            state.runner.rig.iter().find(|c| c.card.0 == "cookbook").unwrap().counters,
            0,
            "Cookbook never places a counter on itself"
        );
    }

    /// Drives a run against `server` (assumed ICE-free save for a single
    /// `click_breakable` ICE at `ice_id` already installed by the caller)
    /// up to and including entering the encounter with it, returning the
    /// resulting state.
    fn enter_encounter_with(
        state: GameState,
        registry: &CardRegistry,
        server: ServerId,
    ) -> GameState {
        let (state, _) = apply_action(&state, registry, PlayerAction::InitiateRun { server }).expect("initiate run");
        let (state, _) = apply_action(&state, registry, PlayerAction::ContinueRun).expect("approach ice");
        let (state, _) =
            apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach");
        let (state, _) = apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes approach, entering encounter");
        state
    }

    #[test]
    fn break_subroutine_with_click_is_only_legal_on_click_breakable_ice_mid_encounter_and_spends_a_click() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("ansel_1_0", ServerId::Hq)];

        let action = PlayerAction::BreakSubroutineWithClick { ice_id: CardId("ansel_1_0".to_string()), subroutine_index: 0 };

        // Not yet in an encounter — never offered.
        let mask = crate::rules::get_action_mask(&state, &registry);
        let index = crate::rules::ActionSpace::index_of(&state, &action).expect("always encodable");
        assert!(!mask[index], "not legal before the run even starts");
        assert!(apply_action(&state, &registry, action.clone()).is_err());

        state = enter_encounter_with(state, &registry, ServerId::Hq);

        let mask = crate::rules::get_action_mask(&state, &registry);
        assert!(mask[index], "legal once encountering click-breakable ice");
        // Never offered to the Corp, even mid-encounter.
        let corp_legal = crate::rules::legal_actions_for(&state, &registry, Side::Corp);
        assert!(!corp_legal.contains(&action), "the Corp can never use this action");

        let (state, events) = apply_action(&state, &registry, action).expect("break via click");
        assert_eq!(state.runner.resources.clicks, Clicks(2), "4 - 1 (InitiateRun) - 1 (break via click)");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { index: 0, .. })));
        assert_eq!(
            state.active_run.as_ref().unwrap().ice[0].subroutines[0].status,
            crate::rules::SubroutineStatus::Broken
        );
    }

    #[test]
    fn break_subroutine_with_click_is_illegal_on_ordinary_ice() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Hq)];
        state = enter_encounter_with(state, &registry, ServerId::Hq);

        let action = PlayerAction::BreakSubroutineWithClick { ice_id: CardId("wall_of_static".to_string()), subroutine_index: 0 };
        let mask = crate::rules::get_action_mask(&state, &registry);
        let index = crate::rules::ActionSpace::index_of(&state, &action).expect("always encodable");
        assert!(!mask[index], "wall_of_static isn't click_breakable");
        assert!(apply_action(&state, &registry, action).is_err());
    }

    #[test]
    fn ansel_1_0_full_subroutine_chain_trashes_installs_and_prevents_steal_and_trash() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];
        state.corp.installed = vec![corp_ice("ansel_1_0", ServerId::Hq)];
        state.corp.hq = vec![CardId("nico_campaign".to_string())];

        state = enter_encounter_with(state, &registry, ServerId::Hq);

        // Subroutine 1: trash 1 installed Runner card — parks a
        // `ChooseCards` directly (no top-level choice, it's mandatory).
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes encounter window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes, firing subroutine 1 and parking the trash choice");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "corroder") })
                .expect("toggle corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection)
            .expect("confirm trash, resuming subroutine resolution");
        assert!(state.runner.rig.is_empty(), "corroder should have been trashed");
        assert!(state.runner.heap.contains(&CardId("corroder".to_string())));

        // Subroutine 2: may install 1 card from HQ or Archives — choose HQ,
        // pick the asset, and pick where it goes: the printed text fixes
        // neither the server nor waives the cost (an asset installs free
        // either way; the ICE tax case is pinned by
        // `ansels_install_pays_the_ice_install_tax`).
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to install from HQ");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "nico_campaign") })
                .expect("toggle nico_campaign");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection)
            .expect("confirm the pick, parking the destination choice");
        match &state.pending_decision {
            Some(crate::rules::PendingDecision::ChooseServer { chooser: Side::Corp, allowed_servers: Some(allowed), install: Some(_), .. }) => {
                assert!(
                    allowed.iter().all(|server| matches!(server, ServerId::Remote(_))),
                    "an asset is remote-only: {allowed:?}"
                );
            }
            other => panic!("expected the Corp's destination choice, got {other:?}"),
        }
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Remote(0) })
                .expect("install into a new remote, resuming subroutine resolution");
        assert!(!state.corp.hq.contains(&CardId("nico_campaign".to_string())));
        assert!(state.corp.installed.iter().any(|c| c.card == CardId("nico_campaign".to_string()) && c.server == ServerId::Remote(0) && !c.rezzed));
        assert_eq!(state.corp.resources.credits, Credits(10), "an asset installs for free — unchanged from the starting balance");

        // Subroutine 3: prevent steal/trash for the remainder of this run.
        assert!(state.active_run.as_ref().unwrap().runner_cannot_steal_or_trash);
    }

    /// Null Signal Games' install rule: new ICE goes in the **outermost**
    /// position. `corp.installed`'s per-server vec order is what the run
    /// approaches positionally, and every install used to append — which
    /// put each new piece *innermost* and silently reversed the approach
    /// order of every stacked server.
    #[test]
    fn installing_ice_lands_outermost_so_the_runner_approaches_the_newest_first() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(10);
        state.corp.installed = vec![corp_ice("tithe", ServerId::Hq)];
        state.corp.hq = vec![CardId("palisade".to_string())];

        let (mut state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard { card_id: CardId("palisade".to_string()), zone: ServerId::Hq, slot: InstallSlot::Ice },
        )
        .expect("install palisade onto HQ");
        assert_eq!(state.corp.resources.credits, Credits(9), "1[c]: one piece of ICE already protects HQ");
        assert_eq!(
            state.corp.installed.iter().map(|c| c.card.0.as_str()).collect::<Vec<_>>(),
            vec!["palisade", "tithe"],
            "the new piece sits in front — outermost"
        );

        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run HQ");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach the outermost ICE");
        assert_eq!(state.active_run.as_ref().unwrap().ice[0].card_id, CardId("palisade".to_string()), "newest first");
    }

    /// Ansel 1.0's install pays the normal cost — 1[c] per piece of ICE
    /// already protecting the chosen server. Only Brân prints "ignoring
    /// all costs".
    #[test]
    fn ansels_install_pays_the_ice_install_tax() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];
        state.corp.installed = vec![corp_ice("ansel_1_0", ServerId::Hq)];
        state.corp.hq = vec![CardId("palisade".to_string())];

        state = enter_encounter_with(state, &registry, ServerId::Hq);
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes encounter window");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes, firing subroutine 1");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "corroder") })
                .expect("toggle corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm trash");

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to install from HQ");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "palisade") })
                .expect("toggle palisade");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm the pick");
        let credits_before = state.corp.resources.credits;
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq })
                .expect("install onto HQ, behind the encounter");
        assert_eq!(state.corp.resources.credits.0, credits_before.0 - 1, "Ansel itself already protects HQ: the tax is 1[c]");
        assert_eq!(
            state.corp.installed.iter().filter(|c| c.slot == InstallSlot::Ice).map(|c| c.card.0.as_str()).collect::<Vec<_>>(),
            vec!["palisade", "ansel_1_0"],
            "the new ICE is outermost — behind a Runner already at Ansel, so it is never approached this run"
        );
        // The run is still standing on Ansel; its remaining subroutine
        // resolves and the encounter concludes normally.
        assert!(state.active_run.as_ref().unwrap().runner_cannot_steal_or_trash, "subroutine 3 resolved after the install");
    }

    #[test]
    fn bran_1_0_installs_ice_directly_inward_ignoring_cost_and_the_run_encounters_it() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        // Brân 1.0 is the sole ICE on the run's own server (a second,
        // subroutine-bearing ICE isn't usable here — every baseline ICE's
        // only subroutine ends the run, which would prevent ever reaching
        // Brân's own encounter at all) — but `diviner`, installed on an
        // unrelated remote *after* Brân in `installed`'s order, proves the
        // insert lands immediately after Brân specifically rather than
        // merely being appended to the end of the whole vec.
        state.corp.installed = vec![corp_ice("bran_1_0", ServerId::Hq), corp_ice("diviner", ServerId::Remote(0))];
        state.corp.hq = vec![CardId("ice_wall".to_string())];
        state.corp.resources.credits = Credits(0);

        state = enter_encounter_with(state, &registry, ServerId::Hq);

        // Break Brân's two "end the run" subroutines with clicks so the run
        // survives its own encounter — the point is what happens *after*
        // Brân, on the ICE it just installed.
        let bran = CardId("bran_1_0".to_string());
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::BreakSubroutineWithClick { ice_id: bran.clone(), subroutine_index: 1 },
        )
        .expect("click-break bran's first end the run");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::BreakSubroutineWithClick { ice_id: bran.clone(), subroutine_index: 2 },
        )
        .expect("click-break bran's second end the run");

        // Both sides pass (each break handed priority across, so the order
        // is whoever holds it), firing bran's remaining first subroutine
        // and parking the install choice.
        let (state, _) = close_all_windows(state, &registry);

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to install from HQ");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") })
                .expect("toggle ice_wall");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection)
            .expect("confirm install, ignoring cost");

        assert_eq!(state.corp.resources.credits, Credits(0), "installed ignoring ice_wall's printed cost");
        let positions: Vec<&str> = state.corp.installed.iter().map(|c| c.card.0.as_str()).collect();
        assert_eq!(
            positions,
            vec!["bran_1_0", "ice_wall", "diviner"],
            "ice_wall must land directly inward from bran_1_0 (immediately after it), not appended past diviner"
        );

        // The run's own ICE list was a snapshot from initiation, so the
        // freshly installed ice_wall was never in it: the run passed Brân
        // straight to the server, one ICE early. It is now approached.
        let run = state.active_run.as_ref().expect("the run continues past bran");
        assert_eq!(run.phase, crate::rules::RunPhase::ApproachIce, "{events:?}");
        assert_eq!(run.position, 1);
        assert_eq!(run.ice.len(), 2);
        assert_eq!(run.ice[1].install_id, install_of(&state, "ice_wall"));
        assert!(!run.ice[1].rezzed);
        assert!(events.contains(&crate::rules::GameEvent::IceApproached { server: ServerId::Hq, position: 1 }));

        // The Corp gets its rez window on the new ICE, exactly as on any
        // approach — `rez_ice`'s "is this the ICE being approached" gate
        // sees it.
        let mut state = state;
        state.corp.resources.credits = Credits(5);
        let ice_wall = install_of(&state, "ice_wall");
        assert!(
            crate::rules::legal_actions_for(&state, &registry, Side::Corp).contains(&PlayerAction::RezIce { ice: ice_wall }),
            "rezzing the approached ice_wall is legal for the Corp"
        );
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: ice_wall }).expect("rez ice_wall");
        let (state, events) = close_all_windows(state, &registry);
        assert!(
            events.iter().any(|e| matches!(e, crate::rules::GameEvent::IceEncountered { card_id, .. } if card_id.0 == "ice_wall")),
            "the run encounters the ICE brân installed: {events:?}"
        );
        // …and, both sides having passed through that encounter too,
        // ice_wall's own "end the run" is what ends it — the ICE Brân
        // installed did its job, where before the run walked straight past.
        assert!(
            events.contains(&crate::rules::GameEvent::RunEndedByEffect { server: ServerId::Hq }),
            "{events:?}"
        );
        assert!(state.active_run.is_none());
    }

    #[test]
    fn tao_salonga_swaps_two_installed_ice_and_the_swap_is_reflected_in_a_later_run() {
        let registry = sg_registry();
        let mut state = base_state();
        state.runner.identity = Some(CardId("tao_salonga".to_string()));
        state.corp.resources.credits = Credits(10);
        // 1 (install) + 2 (advance x2) + 1 (score) = 4 clicks.
        state.corp.resources.clicks = Clicks(4);
        state.corp.installed = vec![
            corp_ice("wall_of_static", ServerId::Hq),
            corp_ice("ice_wall", ServerId::Remote(0)),
        ];
        state.corp.hq = vec![CardId("hostile_takeover".to_string())];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard {
                card_id: CardId("hostile_takeover".to_string()),
                zone: ServerId::Remote(1),
                slot: InstallSlot::Root,
            },
        )
        .expect("install hostile takeover");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { target: install_of(&state, "hostile_takeover") },
        )
        .expect("advance once");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { target: install_of(&state, "hostile_takeover") },
        )
        .expect("advance twice — hostile_takeover needs 2 advancement tokens to score");
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ScoreAgenda { target: install_of(&state, "hostile_takeover") },
        )
        .expect("score hostile takeover, firing Tao Salonga's OnAgendaScored reaction");

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to swap");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "wall_of_static") })
                .expect("toggle wall_of_static");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") })
                .expect("toggle ice_wall");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm swap");

        let wall = state.corp.installed.iter().find(|c| c.card.0 == "wall_of_static").unwrap();
        let ice_wall = state.corp.installed.iter().find(|c| c.card.0 == "ice_wall").unwrap();
        assert_eq!(wall.server, ServerId::Remote(0), "wall_of_static took ice_wall's old position");
        assert_eq!(ice_wall.server, ServerId::Hq, "ice_wall took wall_of_static's old position");
    }

    /// Drives a run into Brân 1.0's encounter on `server`, fires its first
    /// subroutine, and picks "install from HQ" — parked at the card
    /// selection, one `ConfirmCardSelection` away from the install.
    fn bran_install_choice_parked(mut state: GameState, registry: &CardRegistry, server: ServerId) -> GameState {
        state = enter_encounter_with(state, registry, server);
        let (state, _) = close_all_windows(state, registry);
        let (state, _) = apply_action(&state, registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to install from HQ");
        let (state, _) =
            apply_action(&state, registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") })
                .expect("toggle ice_wall");
        state
    }

    /// "Directly inward from *this* ice" names the Brân being encountered —
    /// not the first Brân in install order. With the encountered copy
    /// listed second, a first-match-by-`CardId` lookup installed inward of
    /// the other copy, on the other server.
    #[test]
    fn bran_1_0_installs_inward_of_the_encountered_copy_not_the_first_copy() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        let other_bran = crate::rules::InstalledCard { install_id: InstallId(777), ..corp_ice("bran_1_0", ServerId::Remote(0)) };
        state.corp.installed = vec![other_bran, corp_ice("bran_1_0", ServerId::Hq)];
        state.corp.hq = vec![CardId("ice_wall".to_string())];

        let state = bran_install_choice_parked(state, &registry, ServerId::Hq);
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");

        let placed: Vec<(&str, ServerId)> = state.corp.installed.iter().map(|c| (c.card.0.as_str(), c.server)).collect();
        assert_eq!(
            placed,
            vec![("bran_1_0", ServerId::Remote(0)), ("bran_1_0", ServerId::Hq), ("ice_wall", ServerId::Hq)],
            "inward of the HQ brân — the one being encountered"
        );
    }

    /// If the Brân whose subroutine parked the install has left play by the
    /// time the Corp confirms, there is nothing to install "directly inward
    /// from", and nothing is installed — the chosen card stays in HQ. It
    /// used to be installed into Archives, the JSON placeholder server.
    #[test]
    fn bran_1_0s_install_does_not_happen_if_bran_is_gone_when_the_choice_resolves() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("bran_1_0", ServerId::Hq)];
        state.corp.hq = vec![CardId("ice_wall".to_string())];

        let mut state = bran_install_choice_parked(state, &registry, ServerId::Hq);
        // Brân leaves play while the Corp is choosing.
        state.corp.installed.retain(|c| c.card.0 != "bran_1_0");

        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");

        assert!(state.corp.installed.is_empty(), "nothing was installed anywhere: {:?}", state.corp.installed);
        assert_eq!(state.corp.hq, vec![CardId("ice_wall".to_string())], "the chosen card is still in HQ");
        assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardInstalled { .. })));
        // And with the encountered ICE gone the encounter is over: its
        // remaining "end the run" subroutines do not fire, and the run
        // stands at the server.
        assert!(!events.contains(&crate::rules::GameEvent::RunEndedByEffect { server: ServerId::Hq }));
        assert_eq!(state.active_run.as_ref().map(|r| r.phase), Some(crate::rules::RunPhase::Success), "{events:?}");
    }

    /// A swap that touches the attacked server used to be refused during a
    /// run (`CannotSwapIceDuringActiveRun`) because `run.ice` was a
    /// snapshot. It now follows `corp.installed`: the Runner approaches
    /// whatever ICE is actually protecting the server when they step.
    #[test]
    fn swap_installed_ice_during_a_run_is_reflected_in_the_runs_ice_list() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Hq), corp_ice("ice_wall", ServerId::Remote(0))];

        let (mut state, _) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        assert_eq!(state.active_run.as_ref().unwrap().ice[0].card_id.0, "wall_of_static");

        let swap = crate::dsl::Effect::SwapInstalledIce(install_of(&state, "wall_of_static"), install_of(&state, "ice_wall"));
        crate::rules::evaluate_effect(&mut state, &swap, &mut crate::rules::ResolutionContext::default(), &registry)
            .expect("swapping mid-run is legal");

        let (state, events) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let run = state.active_run.as_ref().unwrap();
        assert_eq!(run.ice.len(), 1);
        assert_eq!(run.ice[0].install_id, install_of(&state, "ice_wall"), "ice_wall now protects HQ, so it is approached");
        assert_eq!(run.position, 0);
        assert!(events.contains(&crate::rules::GameEvent::IceApproached { server: ServerId::Hq, position: 0 }));
    }

    /// ICE derezzed after the run began (Tranquilizer's shape) is passed
    /// like any unrezzed ICE — the run reads the install, not a snapshot.
    #[test]
    fn a_derezzed_ice_is_passed_without_an_encounter_even_if_rezzed_when_the_run_began() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("palisade", ServerId::Hq)];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (mut state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach palisade");
        assert!(state.active_run.as_ref().unwrap().ice[0].rezzed, "rezzed when the run began");
        assert!(state.paid_ability_window.is_some(), "the approach window is open");

        crate::rules::evaluate_effect(
            &mut state,
            &crate::dsl::Effect::DerezCard(crate::dsl::CardTarget::CorpInstalled {
                card: CardId("palisade".to_string()),
                server: ServerId::Hq,
            }),
            &mut crate::rules::ResolutionContext::default(),
            &registry,
        )
        .expect("derez");

        let (state, events) = close_all_windows(state, &registry);
        assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::IceEncountered { .. })), "{events:?}");
        assert!(events.contains(&crate::rules::GameEvent::IcePassed { server: ServerId::Hq, position: 0 }));
        assert_eq!(state.active_run.as_ref().unwrap().phase, crate::rules::RunPhase::Success);
    }

    #[test]
    fn weyland_built_to_last_gains_credits_only_on_the_first_advancement_of_a_card() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.identity = Some(CardId("weyland_consortium_built_to_last".to_string()));
        // Enough to pay for 2 advancements (1 credit each) with none to
        // spare, isolating the identity's bonus in the assertions below.
        state.corp.resources.credits = Credits(2);
        state.corp.resources.clicks = Clicks(3);
        state.corp.hq = vec![CardId("hostile_takeover".to_string())];

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard {
                card_id: CardId("hostile_takeover".to_string()),
                zone: ServerId::Remote(0),
                slot: InstallSlot::Root,
            },
        )
        .expect("install hostile takeover");

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { target: install_of(&state, "hostile_takeover") },
        )
        .expect("first advancement");
        assert_eq!(state.corp.resources.credits, Credits(3), "2 - 1 (advance cost) + 2 (Weyland's first-advancement bonus)");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 2 })));

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { target: install_of(&state, "hostile_takeover") },
        )
        .expect("second advancement");
        assert_eq!(state.corp.resources.credits, Credits(2), "no further credits on a card's second+ advancement");
    }

    #[test]
    fn neurospike_deals_net_damage_equal_to_agenda_points_scored_this_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.resources.clicks = Clicks(3);
        state.corp.hq = vec![CardId("neurospike".to_string())];
        state.corp.installed = vec![installed_with_counters("offworld_office", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 4;
        state.runner.grip = (0..5).map(|i| CardId(format!("grip_card_{i}"))).collect();

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "offworld_office") })
                .expect("score offworld office");
        assert_eq!(state.corp.agenda_points_scored_this_turn, 2);

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("neurospike".to_string()) })
                .expect("play neurospike");
        assert_eq!(state.runner.grip.len(), 3, "2 net damage discards 2 of the 5 grip cards");
        assert_eq!(
            events.iter().filter(|e| matches!(e, crate::rules::GameEvent::CardDiscarded { side: Side::Runner, .. })).count(),
            2
        );
    }

    #[test]
    fn conduit_may_place_a_counter_on_a_successful_rnd_run_and_spends_counters_for_bonus_access() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = vec![rig_card_with_counters("conduit", 0)];
        state.corp.r_and_d = (0..3).map(|i| CardId(format!("rd_card_{i}"))).collect();

        // `Trigger::OnSuccessfulRunOnRnD` fires (and parks the "place a
        // counter?" choice) the moment `RunSucceeded` fires, on
        // `CompleteRun`, ahead of the pre-access window — resolve it before
        // continuing, rather than using `run_to_completion`'s all-in-one
        // helper, which would otherwise hit `ActionBlockedByPendingDecision`.
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::RnD }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue run to the approach step");
        let (state, events) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");
        assert!(events
            .iter()
            .any(|e| matches!(e, crate::rules::GameEvent::PendingChoicePresented { chooser: Side::Runner, .. })));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("runner chooses to place a virus counter");
        assert_eq!(state.runner.rig[0].counters, 1);

        // Independently: with 2 hosted counters already banked, activating
        // Conduit's ability grants 2 additional R&D accesses on top of the
        // normal one.
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = vec![rig_card_with_counters("conduit", 2)];
        state.corp.r_and_d = (0..5).map(|i| CardId(format!("rd_card_{i}"))).collect();

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "conduit"), ability_index: 0 },
        )
        .expect("activate conduit's run ability");
        assert_eq!(state.active_run.as_ref().unwrap().additional_rd_access, 2);
    }

    #[test]
    fn echelon_and_unity_scale_with_the_number_of_installed_icebreakers() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        // 3 installed icebreakers (Echelon itself plus 2 others), so
        // Echelon's `PerInstalledIcebreaker(1)` should add +3 on top of its
        // 0 printed strength, and Unity's pump should add +3 as well.
        // `rig_card_with_counters` forces `base_strength: 0` regardless of
        // the card's real printed strength — fine for Echelon (whose real
        // printed strength genuinely is 0) but Unity prints 1, so it's
        // built explicitly here instead.
        state.runner.rig = vec![
            rig_card_with_counters("echelon", 0),
            crate::rules::InstalledRunnerCard {
                card: CardId("unity".to_string()),
                base_strength: 1,
                ..Default::default()
            },
            rig_card_with_counters("corroder", 0),
        ];
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Hq)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach wall of static");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes, entering encounter");

        // Echelon's live strength (base 0 + 3 icebreakers) should already be
        // 3 without spending anything.
        assert_eq!(
            crate::rules::computed_runner_strength(&state.runner.rig[0], &state, &registry),
            3,
            "Echelon: 0 base + 1 per installed icebreaker (3 installed, including itself)"
        );

        // Unity's pump ability (+X, X = installed icebreaker count = 3)
        // should bring it from base 1 to 4.
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "unity"), ability_index: 1 })
                .expect("pump unity");
        assert_eq!(state.runner.rig[1].effective_strength(), 4, "Unity: 1 base + 3 (installed icebreaker count)");
    }

    /// The win-reverting bug (ROADMAP Rules Audit §4). Clearinghouse's
    /// start-of-turn choice is parked *under* the start-of-turn window; when
    /// the choice flatlines the Runner, the window used to survive, so
    /// `current_actor` still named the Corp, `PassPriority` was legal, and
    /// two passes later `close_window` wrote `phase = Action(Corp)`: play
    /// continued with the Runner at zero cards and no flatline on record.
    #[test]
    fn clearinghouse_flatline_at_corp_turn_start_ends_the_game_and_leaves_no_legal_action() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.r_and_d = vec![CardId("filler_card".to_string())];
        state.corp.installed = vec![installed_with_counters("clearinghouse", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 3;
        // Two cards against three meat damage: a flatline.
        state.runner.grip = vec![CardId("grip_card_0".to_string()), CardId("grip_card_1".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes end-of-turn");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes, entering their start of turn");
        assert!(state.paid_ability_window.is_some(), "the start-of-turn window is open over the parked choice");

        let (state, events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("corp chooses to trash clearinghouse for damage");

        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp), "{events:?}");
        assert!(events.contains(&crate::rules::GameEvent::RunnerFlatlined));
        assert_eq!(events.iter().filter(|e| matches!(e, crate::rules::GameEvent::GameOver { .. })).count(), 1);
        assert!(state.paid_ability_window.is_none(), "the window died with the game");
        assert!(state.pending_decision.is_none());
        assert_eq!(crate::rules::current_actor(&state), None);
        assert!(crate::rules::legal_actions(&state, &registry).is_empty(), "nothing is legal in a finished game");
        for side in [Side::Corp, Side::Runner] {
            assert_eq!(
                apply_action(&state, &registry, PlayerAction::PassPriority { side }).err(),
                Some(RulesError::GameIsOver { winner: Side::Corp })
            );
        }
    }

    /// Carnivore's "trash 2 cards from your grip" is offered only when there
    /// are 2 to trash. Before its `ZoneHasAtLeast` gate, the ability was
    /// legal with one card in the grip: `PromptChooseCards` silently parked
    /// nothing, the accessed card stayed, and Carnivore's once-per-turn was
    /// spent on it.
    #[test]
    fn carnivore_is_not_offered_and_spends_nothing_with_fewer_than_two_grip_cards() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.rig = rig_of(&["carnivore"]);
        state.runner.grip = vec![CardId("sure_gamble".to_string())];
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.active_run = Some(crate::rules::RunState {
            server: ServerId::Hq,
            phase: crate::rules::RunPhase::AccessingCard,
            access_state: Some(crate::rules::AccessState { pending_install: None, resolved_installs: Vec::new(),
                server: ServerId::Hq,
                phase: crate::rules::AccessPhase::PendingChoice {
                    card_id: CardId("hedge_fund".to_string()),
                    trash_cost: None,
                    mandatory_steal: false,
                    steal_cost: None,
                },
                ..Default::default()
            }),
            ..Default::default()
        });
        let activate = PlayerAction::ActivateAbility { target: fixture_install_id("carnivore"), ability_index: 0 };

        assert!(!crate::rules::legal_actions_for(&state, &registry, Side::Runner).contains(&activate), "one card in grip");
        assert_eq!(apply_action(&state, &registry, activate.clone()).err(), Some(RulesError::RequirementNotMet));
        assert!(state.runner.once_per_turn_used.is_empty(), "a refused activation consumes nothing");

        state.runner.grip.push(CardId("overclock".to_string()));
        assert!(crate::rules::legal_actions_for(&state, &registry, Side::Runner).contains(&activate), "two cards in grip");
    }

    /// `hosted_on_ice` is an install: with two Palisades on one server, the
    /// Botulus on the first breaks only while the first is encountered, and
    /// the Botulus on the second only then. As a `CardId` the host matched
    /// both copies (or, resolved through the first Botulus, neither).
    #[test]
    fn two_botulus_on_two_copies_of_one_ice_each_break_only_on_their_own_host() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        let palisade = |install: u32| crate::rules::InstalledCard { install_id: InstallId(install), ..corp_ice("palisade", ServerId::Hq) };
        state.corp.installed = vec![palisade(7001), palisade(7002)];
        let botulus = |install: u32, host: u32| crate::rules::InstalledRunnerCard {
            install_id: InstallId(install),
            card: CardId("botulus".to_string()),
            counters: 1,
            hosted_on_ice: Some(InstallId(host)),
            ..Default::default()
        };
        state.runner.rig = vec![botulus(8001, 7001), botulus(8002, 7002)];

        // Encounter the outermost Palisade (install 7001).
        let state = enter_encounter_with(state, &registry, ServerId::Hq);
        let legal = crate::rules::legal_actions_for(&state, &registry, Side::Runner);
        let activate = |target: u32| PlayerAction::ActivateAbility { target: InstallId(target), ability_index: 0 };
        assert!(legal.contains(&activate(8001)), "the Botulus hosted on the encountered Palisade may break: {legal:?}");
        assert!(!legal.contains(&activate(8002)), "the Botulus on the *other* Palisade may not");

        let (state, _) = apply_action(&state, &registry, activate(8001)).expect("break with the right botulus");
        assert_eq!(state.runner.rig[0].counters, 0, "it spent its own counter");
        assert_eq!(state.runner.rig[1].counters, 1, "the other copy's counter is untouched");
    }

    /// Two copies of one counter card are two cards. Trigger dispatch used
    /// to plan by `CardId`, so both Fermenters' `OnTurnStart` resolved on
    /// copy #1 — copy #2 never accrued a counter in its life, and cashing
    /// it read copy #1's total and trashed copy #1.
    #[test]
    fn two_fermenters_accrue_and_cash_their_own_counters() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.runner.resources.credits = Credits(0);
        let fermenter = |install: u32, counters: u32| crate::rules::InstalledRunnerCard {
            install_id: InstallId(install),
            card: CardId("fermenter".to_string()),
            counters,
            ..Default::default()
        };
        state.runner.rig = vec![fermenter(3001, 2), fermenter(3002, 0)];

        // Corp ends its turn; the Runner's turn start fires both Fermenters,
        // and two same-side reactors hand the Runner the order.
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("pass");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("pass, runner turn starts");
        assert!(
            matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseTriggerOrder { .. })),
            "{:?}",
            state.pending_decision
        );
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ChooseTriggerToResolve { index: 0 }).expect("order the two");
        let (state, _) = close_all_windows(state, &registry);
        assert_eq!(state.runner.rig[0].counters, 3, "copy #1 gained one");
        assert_eq!(state.runner.rig[1].counters, 1, "copy #2 gained one of its own");

        // Cash copy #2: its own counter's worth, and copy #2 is what goes.
        let credits_before = state.runner.resources.credits;
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId(3002), ability_index: 0 })
            .expect("cash the second fermenter");
        assert_eq!(state.runner.resources.credits, credits_before.gain(2), "1 counter × 2 credits");
        assert_eq!(state.runner.rig.len(), 1);
        assert_eq!(state.runner.rig[0].install_id, InstallId(3001), "copy #1 is still installed");
        assert_eq!(state.runner.rig[0].counters, 3, "with its own counters intact");
    }

    /// `OnRez` resolves on the copy that was rezzed: `IceRezzed` names the
    /// install. Before, rezzing the second Nico Campaign loaded its nine
    /// counters onto the first — possibly still unrezzed, where masking
    /// hides counters, so nothing showed the discrepancy.
    #[test]
    fn two_nico_campaigns_each_rez_with_their_own_nine_counters() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(10);
        let nico = |install: u32, server: ServerId| crate::rules::InstalledCard {
            install_id: InstallId(install),
            card: CardId("nico_campaign".to_string()),
            server,
            slot: InstallSlot::Root,
            rezzed: false,
            ..Default::default()
        };
        state.corp.installed = vec![nico(4001, ServerId::Remote(0)), nico(4002, ServerId::Remote(1))];

        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: InstallId(4002) }).expect("rez the second");

        assert!(!state.corp.installed[0].rezzed);
        assert_eq!(state.corp.installed[0].counters, 0, "the unrezzed first copy is untouched");
        assert!(state.corp.installed[1].rezzed);
        assert_eq!(state.corp.installed[1].counters, 9, "the rezzed copy holds its own counters");
    }

    /// An ambush's damage is sized by *its own* advancement counters. The
    /// `OnAccessed` reaction used to read the first installed Urtica, so a
    /// decoy with no counters dealt the other copy's damage — a flatline
    /// the Runner did nothing to deserve.
    #[test]
    fn urtica_cipher_decoy_deals_only_its_own_advancement_damage() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = (0..4).map(|i| CardId(format!("grip_{i}"))).collect();
        let urtica = |install: u32, server: ServerId, advancement_tokens: u32| crate::rules::InstalledCard {
            install_id: InstallId(install),
            card: CardId("urtica_cipher".to_string()),
            server,
            slot: InstallSlot::Root,
            advancement_tokens,
            ..Default::default()
        };
        state.corp.installed = vec![urtica(5001, ServerId::Remote(0), 3), urtica(5002, ServerId::Remote(1), 0)];

        // Run the *decoy* in Remote(1).
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(1) }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach the server");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit");
        let (state, events) = close_all_windows(state, &registry);

        assert!(!events.contains(&crate::rules::GameEvent::RunnerFlatlined), "{events:?}");
        assert_eq!(state.runner.grip.len(), 2, "2 net damage — the decoy's own printed 2, plus its 0 counters");
        assert!(state.active_run.is_some(), "the run goes on to the access decision");
    }

    /// A choice parked by a trigger acts on the copy that triggered. With
    /// two Clearinghouses the Corp orders the two reactions; resolving the
    /// second copy's choice must size the damage by *its* counters and
    /// trash *it*.
    #[test]
    fn clearinghouse_choice_trashes_the_copy_that_triggered() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.r_and_d = vec![CardId("filler_card".to_string())];
        let clearinghouse = |install: u32, server: ServerId, advancement_tokens: u32| crate::rules::InstalledCard {
            install_id: InstallId(install),
            card: CardId("clearinghouse".to_string()),
            server,
            slot: InstallSlot::Root,
            rezzed: true,
            advancement_tokens,
            ..Default::default()
        };
        state.corp.installed = vec![clearinghouse(6001, ServerId::Remote(0), 0), clearinghouse(6002, ServerId::Remote(1), 3)];
        state.runner.grip = (0..5).map(|i| CardId(format!("grip_{i}"))).collect();

        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("pass");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("pass, corp turn starts");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseTriggerOrder { .. })));

        // Resolve the *second* copy's trigger first, then take its damage option.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseTriggerToResolve { index: 1 }).expect("second copy first");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { source_install: Some(InstallId(6002)), .. })));
        let (state, events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("trash for damage");

        assert!(events.contains(&crate::rules::GameEvent::DamageTaken { damage_type: crate::dsl::DamageType::Meat, amount: 3 }), "{events:?}");
        assert_eq!(state.runner.grip.len(), 2);
        assert_eq!(state.corp.installed.len(), 1, "one Clearinghouse trashed itself");
        assert_eq!(state.corp.installed[0].install_id, InstallId(6001), "the *other* copy is the one still installed");
    }

    #[test]
    fn clearinghouse_may_trash_itself_for_meat_damage_equal_to_its_advancement_tokens() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.r_and_d = vec![CardId("filler_card".to_string())];
        state.corp.installed = vec![installed_with_counters("clearinghouse", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 3;
        state.runner.grip = (0..5).map(|i| CardId(format!("grip_card_{i}"))).collect();

        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);
        // Ending the Runner's turn opens an `EndOfTurn(Runner)` window
        // first; `Trigger::OnTurnStart`'s `PresentChoice` (parking a
        // `pending_decision`) doesn't fire until that window closes and
        // `enter_start_of_turn(Corp)` actually runs — matching Nico
        // Campaign's test, which finds its own `OnTurnStart` payout only
        // after closing windows past the second `EndTurn`, not in that
        // call's own return. Stepping priority manually here (rather than
        // the blind `close_all_windows` loop) since a `pending_decision`
        // parked mid-close would otherwise block the loop's own
        // `PassPriority` calls.
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes end-of-turn");
        let (state, mut events) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes, entering their start of turn");
        assert!(events
            .iter()
            .any(|e| matches!(e, crate::rules::GameEvent::PendingChoicePresented { chooser: Side::Corp, .. })));

        let (state, resolve_events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("corp chooses to trash clearinghouse for damage");
        events.extend(resolve_events);

        assert!(state.corp.installed.is_empty(), "should have trashed itself");
        assert!(state.corp.archives_contains(&CardId("clearinghouse".to_string())));
        assert_eq!(state.runner.grip.len(), 2, "5 - 3 meat damage (3 hosted advancement tokens)");
        assert_eq!(
            events.iter().filter(|e| matches!(e, crate::rules::GameEvent::CardDiscarded { side: Side::Runner, .. })).count(),
            3
        );
    }

    #[test]
    fn urtica_cipher_deals_net_damage_scaled_by_its_advancement_tokens_on_access() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = (0..6).map(|i| CardId(format!("grip_card_{i}"))).collect();
        state.corp.installed = vec![installed_with_counters("urtica_cipher", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 2;

        let (state, events) = run_to_completion(state, &registry, ServerId::Remote(0));
        // 2 (printed) + 2 (advancement tokens) = 4 net damage.
        assert_eq!(state.runner.grip.len(), 2, "6 - 4 net damage");
        assert_eq!(
            events.iter().filter(|e| matches!(e, crate::rules::GameEvent::CardDiscarded { side: Side::Runner, .. })).count(),
            4
        );
    }

    #[test]
    fn palisade_gets_bonus_strength_only_while_protecting_a_remote_server() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("palisade", ServerId::Remote(0))];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run remote");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach palisade");
        assert_eq!(
            state.active_run.as_ref().unwrap().ice[0].current_strength,
            4,
            "2 printed + 2 while protecting a remote server"
        );

        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("palisade", ServerId::Hq)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run hq");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach palisade on hq");
        assert_eq!(
            state.active_run.as_ref().unwrap().ice[0].current_strength,
            2,
            "no bonus while protecting a central server"
        );
    }

    #[test]
    fn pharos_gets_bonus_strength_only_at_three_or_more_advancement_tokens() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("pharos", ServerId::Hq)];
        state.corp.installed[0].advancement_tokens = 2;

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run hq");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach pharos below threshold");
        assert_eq!(state.active_run.as_ref().unwrap().ice[0].current_strength, 5, "below the 3-token threshold: no bonus");

        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![corp_ice("pharos", ServerId::Hq)];
        state.corp.installed[0].advancement_tokens = 3;
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run hq");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach pharos at threshold");
        assert_eq!(state.active_run.as_ref().unwrap().ice[0].current_strength, 10, "5 printed + 5 at 3+ advancement tokens");
    }

    /// A rezzed Root-slot Corp install, the shape every AMAZE Amusements
    /// scenario below starts from.
    fn corp_root(id: &str, server: ServerId) -> crate::rules::InstalledCard {
        crate::rules::InstalledCard {
            install_id: fixture_install_id(id),
            card: CardId(id.to_string()),
            server,
            rezzed: true,
            ..Default::default()
        }
    }

    /// Applies `action`, then drains whatever paid-ability window it opened.
    /// Multi-card access parks a fresh window after *every* step (select,
    /// steal, trash, pass), so the AMAZE tests below would otherwise be
    /// half `close_all_windows` calls.
    fn act(state: GameState, registry: &CardRegistry, action: PlayerAction) -> GameState {
        let (state, _) = apply_action(&state, registry, action.clone()).unwrap_or_else(|e| panic!("{action:?}: {e:?}"));
        let (state, _) = close_all_windows(state, registry);
        state
    }

    /// Runs Remote(0) — holding AMAZE Amusements plus an agenda — up to the
    /// point where both are on the table awaiting access.
    fn amaze_run_to_access(registry: &CardRegistry) -> GameState {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.corp.installed =
            vec![corp_root("amaze_amusements", ServerId::Remote(0)), corp_root("offworld_office", ServerId::Remote(0))];

        let state = act(state, registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) });
        let state = act(state, registry, PlayerAction::ContinueRun);
        act(state, registry, PlayerAction::CompleteRun)
    }

    #[test]
    fn amaze_amusements_tags_the_runner_twice_when_a_run_on_its_server_ends_after_a_steal() {
        let registry = sg_registry();
        let state = amaze_run_to_access(&registry);

        let state = act(state, &registry, PlayerAction::SelectCardToAccess { card_id: CardId("offworld_office".to_string()) });
        let state = act(state, &registry, PlayerAction::StealAgenda { card_id: CardId("offworld_office".to_string()) });
        let state = act(state, &registry, PlayerAction::PassAccessedCard { card_id: CardId("amaze_amusements".to_string()) });

        assert!(state.active_run.is_none(), "the run should have ended");
        assert_eq!(state.runner.tags, 2, "AMAZE gives 2 tags when an agenda was stolen during the run");
    }

    #[test]
    fn amaze_amusements_still_tags_after_the_runner_trashes_it_mid_run() {
        let registry = sg_registry();
        let state = amaze_run_to_access(&registry);

        // Trash AMAZE *first* — its "Persistent" clause means the ability
        // must still apply for the remainder of this run.
        let state = act(state, &registry, PlayerAction::SelectCardToAccess { card_id: CardId("amaze_amusements".to_string()) });
        let state = act(state, &registry, PlayerAction::TrashAccessedCard { card_id: CardId("amaze_amusements".to_string()) });
        assert!(
            !state.corp.installed.iter().any(|c| c.card == CardId("amaze_amusements".to_string())),
            "amaze should be off the table"
        );

        let state = act(state, &registry, PlayerAction::StealAgenda { card_id: CardId("offworld_office".to_string()) });

        assert!(state.active_run.is_none(), "the run should have ended");
        assert_eq!(state.runner.tags, 2, "the trashed AMAZE still applies for the remainder of the run");
    }

    /// Remote(0) holding AMAZE Amusements alone — no agenda, so the run can
    /// end without a (mandatory) steal.
    fn amaze_alone_run_to_access(registry: &CardRegistry) -> GameState {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.corp.installed = vec![corp_root("amaze_amusements", ServerId::Remote(0))];

        let state = act(state, registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) });
        let state = act(state, registry, PlayerAction::ContinueRun);
        act(state, registry, PlayerAction::CompleteRun)
    }

    #[test]
    fn amaze_amusements_gives_no_tags_when_the_run_ends_without_a_steal() {
        let registry = sg_registry();
        let state = amaze_alone_run_to_access(&registry);

        let state = act(state, &registry, PlayerAction::PassAccessedCard { card_id: CardId("amaze_amusements".to_string()) });

        assert!(state.active_run.is_none(), "the run should have ended");
        assert_eq!(state.runner.tags, 0, "no agenda stolen means no tags");
    }

    #[test]
    fn amaze_amusements_persistence_does_not_leak_into_a_later_run() {
        let registry = sg_registry();
        let state = amaze_alone_run_to_access(&registry);

        // Run 1: trash AMAZE. Nothing stolen, so no tags — but the trash is
        // what records it in run 1's `persistent_trashed_upgrades`.
        let mut state =
            act(state, &registry, PlayerAction::TrashAccessedCard { card_id: CardId("amaze_amusements".to_string()) });
        assert_eq!(state.runner.tags, 0, "run 1 ended with no steal");
        assert!(state.active_run.is_none(), "run 1 should have ended");

        // Put an agenda in the same server, then run it again. AMAZE is
        // gone and was only ever recorded against run 1's `RunState`, so it
        // must not fire for run 2's steal.
        state.corp.installed = vec![corp_root("offworld_office", ServerId::Remote(0))];
        state.runner.resources.clicks = Clicks(4);

        let state = act(state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) });
        let state = act(state, &registry, PlayerAction::ContinueRun);
        let state = act(state, &registry, PlayerAction::CompleteRun);
        let state = act(state, &registry, PlayerAction::StealAgenda { card_id: CardId("offworld_office".to_string()) });

        assert_eq!(state.runner.tags, 0, "a trashed persistent upgrade must not survive into the next run");
    }

    /// The Corp's own end-of-turn discard is a card the Runner never saw, so
    /// it lands facedown; a rezzed install trashed off the table is one they
    /// did see, so it lands faceup. These two are the poles of the
    /// `ArchivedCard::facedown` rule everything else keys off.
    #[test]
    fn discarding_from_hq_lands_facedown_while_trashing_a_rezzed_install_lands_faceup() {
        let registry = sg_registry();

        let mut state = base_state();
        state.phase = GamePhase::Discard { side: Side::Corp, required: 1 };
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::DiscardCard { card_id: CardId("hedge_fund".to_string()) })
                .expect("discard from hq");
        let (state, _) = close_all_windows(state, &registry);
        assert_eq!(
            state.corp.archives,
            vec![ArchivedCard::facedown(CardId("hedge_fund".to_string()))],
            "a Corp discard from HQ was never seen by the Runner"
        );

        // A rezzed asset the Runner trashes on access: seen, so faceup.
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.corp.installed = vec![corp_root("regolith_mining_license", ServerId::Remote(0))];
        let state = act(state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) });
        let state = act(state, &registry, PlayerAction::ContinueRun);
        let state = act(state, &registry, PlayerAction::CompleteRun);
        let state = act(
            state,
            &registry,
            PlayerAction::TrashAccessedCard { card_id: CardId("regolith_mining_license".to_string()) },
        );
        assert_eq!(
            state.corp.archives,
            vec![ArchivedCard::faceup(CardId("regolith_mining_license".to_string()))],
            "the Runner accessed and trashed this one, so it is faceup"
        );
    }

    /// Jinteki: Restoring Humanity — "when your discard phase ends, if there
    /// is a facedown card in Archives, gain 1 credit." Drives the Corp to
    /// the end of its turn and checks the credit against each Archives
    /// shape.
    fn corp_turn_end_with_archives(registry: &CardRegistry, archives: Vec<ArchivedCard>) -> GameState {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.identity = Some(CardId("jinteki_restoring_humanity".to_string()));
        state.corp.resources.clicks = Clicks(1);
        state.corp.resources.credits = Credits(5);
        state.corp.archives = archives;
        // R&D needs a card: passing control to the Runner is fine, but the
        // *next* Corp turn's mandatory draw would deck them out otherwise.
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        act(state, registry, PlayerAction::EndTurn)
    }

    #[test]
    fn jinteki_restoring_humanity_gains_a_credit_when_archives_holds_a_facedown_card() {
        let registry = sg_registry();
        let state =
            corp_turn_end_with_archives(&registry, vec![ArchivedCard::facedown(CardId("hedge_fund".to_string()))]);
        assert_eq!(state.corp.resources.credits, Credits(6), "5 + 1 for the facedown card in Archives");
    }

    /// "Gain 1[c] **for each** facedown card in Archives" — the printed
    /// text scales; this used to pay a flat 1.
    #[test]
    fn jinteki_restoring_humanity_gains_one_credit_per_facedown_card() {
        let registry = sg_registry();
        let state = corp_turn_end_with_archives(
            &registry,
            vec![
                ArchivedCard::facedown(CardId("hedge_fund".to_string())),
                ArchivedCard::facedown(CardId("government_subsidy".to_string())),
                ArchivedCard::faceup(CardId("palisade".to_string())),
                ArchivedCard::facedown(CardId("offworld_office".to_string())),
            ],
        );
        assert_eq!(state.corp.resources.credits, Credits(8), "5 + 3 facedown cards; the faceup one does not count");
    }

    #[test]
    fn jinteki_restoring_humanity_gains_nothing_when_archives_is_all_faceup_or_empty() {
        let registry = sg_registry();

        let state =
            corp_turn_end_with_archives(&registry, vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))]);
        assert_eq!(state.corp.resources.credits, Credits(5), "a faceup card doesn't satisfy the requirement");

        let state = corp_turn_end_with_archives(&registry, Vec::new());
        assert_eq!(state.corp.resources.credits, Credits(5), "empty Archives doesn't satisfy the requirement");
    }

    #[test]
    fn jinteki_restoring_humanity_fires_after_an_actual_discard_too_not_only_a_skipped_phase() {
        // The tests above all skip the discard phase (hand within size).
        // Here the Corp is over hand size, so the phase really runs — and
        // the discard itself is what puts the facedown card in Archives.
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.identity = Some(CardId("jinteki_restoring_humanity".to_string()));
        state.corp.resources.clicks = Clicks(1);
        state.corp.resources.credits = Credits(5);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        state.corp.hq = (0..6).map(|i| CardId(format!("hedge_fund_{i}"))).collect();

        let state = act(state, &registry, PlayerAction::EndTurn);
        assert!(matches!(state.phase, GamePhase::Discard { side: Side::Corp, .. }), "should owe a discard");
        assert_eq!(state.corp.resources.credits, Credits(5), "nothing gained until the phase actually ends");

        let state =
            act(state, &registry, PlayerAction::DiscardCard { card_id: CardId("hedge_fund_0".to_string()) });
        assert_eq!(
            state.corp.resources.credits,
            Credits(6),
            "the discard put a facedown card in Archives, then the phase ended"
        );
    }

    #[test]
    fn creative_commission_gains_five_credits_and_costs_a_click_when_one_remains() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(2);
        state.runner.grip = vec![CardId("creative_commission".to_string())];

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("creative_commission".to_string()) })
                .expect("creative commission should resolve");

        // 2 - 1 (play cost) + 5 = 6.
        assert_eq!(state.runner.resources.credits, Credits(6));
        // 4 - 1 (playing the event) - 1 (the card's own click loss) = 2.
        assert_eq!(state.runner.resources.clicks, Clicks(2));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::ClicksLost { side: Side::Runner, amount: 1 })));
    }

    /// The click check runs *after* the event's own play cost, so spending
    /// the turn's last click finds zero remaining and skips the loss rather
    /// than underflowing.
    #[test]
    fn creative_commission_played_on_the_last_click_loses_no_further_click() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(1);
        state.runner.resources.credits = Credits(2);
        state.runner.grip = vec![CardId("creative_commission".to_string())];

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("creative_commission".to_string()) })
                .expect("creative commission should resolve");

        assert_eq!(state.runner.resources.credits, Credits(6));
        assert_eq!(state.runner.resources.clicks, Clicks(0), "no underflow, and no second click spent");
        assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::ClicksLost { .. })));
    }

    #[test]
    fn vrcation_draws_four_cards_and_costs_a_click_when_one_remains() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(2);
        state.runner.grip = vec![CardId("vrcation".to_string())];
        state.runner.stack = (0..6).map(|i| CardId(format!("stack_card_{i}"))).collect();

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("vrcation".to_string()) })
            .expect("vrcation should resolve");

        assert_eq!(state.runner.grip.len(), 4, "VRcation left the grip, then drew 4");
        assert_eq!(state.runner.resources.clicks, Clicks(2), "1 for the event, 1 for its own click loss");
    }

    /// Rules Audit T6: scoring is not an action. It costs no click and is
    /// legal on zero clicks — the Corp advances an agenda with the turn's
    /// last click and scores it before ending the turn, which a click cost
    /// made impossible.
    #[test]
    fn scoring_costs_no_click_and_is_legal_on_zero_clicks() {
        let registry = registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(0);
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1041),
            card: CardId("offworld_office".to_string()),
            server: ServerId::Remote(0),
            advancement_tokens: 4,
            ..Default::default()
        }];
        let target = install_of(&state, "offworld_office");

        assert!(
            crate::rules::legal_actions(&state, &registry).contains(&PlayerAction::ScoreAgenda { target }),
            "scoring is offered with no clicks left"
        );
        let (next, events) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target }).expect("scoring is free");
        assert_eq!(next.corp.resources.clicks, Clicks(0));
        assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::ClickSpent { .. })), "{events:?}");
        assert_eq!(next.corp.scored_agendas.iter().map(|s| s.card.clone()).collect::<Vec<_>>(), vec![CardId("offworld_office".to_string())]);
    }

    #[test]
    fn luminal_transubstantiation_gains_three_clicks_and_locks_out_further_scoring() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.installed = vec![
            crate::rules::InstalledCard {
                install_id: InstallId(1039),
                card: CardId("luminal_transubstantiation".to_string()),
                server: ServerId::Remote(0),
                advancement_tokens: 3,
                ..Default::default()
            },
            crate::rules::InstalledCard {
                install_id: InstallId(1040),
                card: CardId("offworld_office".to_string()),
                server: ServerId::Remote(1),
                advancement_tokens: 4,
                ..Default::default()
            },
        ];

        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ScoreAgenda { target: install_of(&state, "luminal_transubstantiation") },
        )
        .expect("luminal should score");

        // 3 + 3 (the agenda's own grant) = 6: scoring itself costs no
        // click (Rules Audit T6).
        assert_eq!(state.corp.resources.clicks, Clicks(6));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::ClicksGained { side: Side::Corp, amount: 3 })));
        assert!(state.corp.cannot_score_agendas_this_turn);

        // The second agenda is fully advanced but can no longer be scored,
        // and the mask agrees with the engine rather than offering it.
        let mask_offers_score = crate::rules::legal_actions(&state, &registry)
            .iter()
            .any(|action| matches!(action, PlayerAction::ScoreAgenda { .. }));
        assert!(!mask_offers_score, "no ScoreAgenda should be offered while the lockout holds");
        assert_eq!(
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "offworld_office") }),
            Err(crate::rules::RulesError::CannotScoreAgendasThisTurn)
        );
    }

    #[test]
    fn luminal_transubstantiations_scoring_lockout_lifts_next_corp_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.cannot_score_agendas_this_turn = true;
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];

        // Corp ends its turn, Runner ends theirs, and the Corp's next
        // start-of-turn clears the flag.
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, _) = close_all_windows(state, &registry);

        assert!(!state.corp.cannot_score_agendas_this_turn, "the lockout is turn-scoped");
    }

    #[test]
    fn tomorrows_headline_tags_the_runner_whether_scored_or_stolen() {
        let registry = sg_registry();

        // Scored by the Corp.
        let mut scored = base_state();
        scored.corp.resources.clicks = Clicks(3);
        scored.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1041),
            card: CardId("tomorrows_headline".to_string()),
            server: ServerId::Remote(0),
            advancement_tokens: 3,
            ..Default::default()
        }];
        let (scored, events) = apply_action(
            &scored,
            &registry,
            PlayerAction::ScoreAgenda { target: install_of(&scored, "tomorrows_headline") },
        )
        .expect("tomorrow's headline should score");
        assert_eq!(scored.runner.tags, 1);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::TagsGiven { side: Side::Runner, amount: 1 })));

        // Stolen by the Runner off a run on HQ.
        let mut stolen = base_state();
        stolen.phase = GamePhase::Action(Side::Runner);
        stolen.runner.resources.clicks = Clicks(4);
        stolen.corp.hq = vec![CardId("tomorrows_headline".to_string())];
        let (stolen, _) = run_to_completion(stolen, &registry, ServerId::Hq);
        let (stolen, _) = close_all_windows(stolen, &registry);
        let (stolen, events) =
            apply_action(&stolen, &registry, PlayerAction::StealAgenda { card_id: CardId("tomorrows_headline".to_string()) })
                .expect("runner steals tomorrow's headline");

        assert_eq!(stolen.runner.tags, 1, "the same trigger fires on a steal");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::TagsGiven { side: Side::Runner, amount: 1 })));
    }

    #[test]
    fn seamless_launch_places_two_advancement_counters_on_an_older_install() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("seamless_launch".to_string())];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1042),
            card: CardId("offworld_office".to_string()),
            server: ServerId::Remote(0),
            // Installed on an earlier turn — the eligible case.
            installed_this_turn: false,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("seamless_launch".to_string()) })
                .expect("play seamless launch");
        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ToggleCardSelection { position: position_of(&state, "offworld_office") },
        )
        .and_then(|(state, _)| apply_action(&state, &registry, PlayerAction::ConfirmCardSelection))
        .expect("confirm the advancement target");

        assert_eq!(state.corp.installed[0].advancement_tokens, 2);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardAdvanced { advancement_tokens: 2, .. })));
    }

    /// The `then` of a card selection acts on the *selected install*. It
    /// used to act on the first install matching the selected card's id, so
    /// with two Offworld Offices in two remotes Seamless Launch advanced the
    /// wrong one — while `ScoreAgenda`, which names an `InstallId`, scored
    /// from the right one, and the two disagreed.
    #[test]
    fn seamless_launch_advances_the_selected_offworld_office_not_the_first() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("seamless_launch".to_string())];
        let office = |install: u32, server: ServerId| crate::rules::InstalledCard {
            install_id: InstallId(install),
            card: CardId("offworld_office".to_string()),
            server,
            installed_this_turn: false,
            ..Default::default()
        };
        state.corp.installed = vec![office(1042, ServerId::Remote(0)), office(1043, ServerId::Remote(1))];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("seamless_launch".to_string()) })
                .expect("play seamless launch");
        // Position 1 in `corp.installed`: the second copy. (`position_of`
        // is first-match by card, so it is not usable here.)
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("pick the second office");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");

        assert_eq!(state.corp.installed[0].advancement_tokens, 0, "the first copy is untouched");
        assert_eq!(state.corp.installed[1].advancement_tokens, 2, "the selected copy was advanced");
    }

    /// "Once per turn" is per card. Three installed Telework Contracts are
    /// three cards; the per-turn key used to be the bare tag, so the copies
    /// shared one use — and the counters spent came off the first copy
    /// whichever one was clicked.
    #[test]
    fn two_telework_contracts_are_each_usable_once_per_turn_and_spend_their_own_counters() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        let contract = |install: u32| crate::rules::InstalledRunnerCard {
            install_id: InstallId(install),
            card: CardId("telework_contract".to_string()),
            counters: 9,
            ..Default::default()
        };
        state.runner.rig = vec![contract(2001), contract(2002)];
        let credits_before = state.runner.resources.credits;

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: InstallId(2002), ability_index: 0 },
        )
        .expect("click the second contract");
        assert_eq!(state.runner.rig[0].counters, 9, "the first copy keeps its counters");
        assert_eq!(state.runner.rig[1].counters, 6, "the clicked copy paid");
        assert_eq!(state.runner.resources.credits, credits_before.gain(3));

        assert!(
            matches!(
                apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId(2002), ability_index: 0 }),
                Err(RulesError::RequirementNotMet)
            ),
            "the same copy is spent for the turn"
        );
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: InstallId(2001), ability_index: 0 },
        )
        .expect("the other copy has its own once-per-turn");
        assert_eq!(state.runner.rig[0].counters, 6);
        assert_eq!(state.runner.resources.credits, credits_before.gain(6));
    }

    /// The "that you did not install this turn" restriction is enforced by
    /// filtering the selectable set, not by rejecting a confirm — so a card
    /// installed this turn is never even offered.
    #[test]
    fn seamless_launch_cannot_target_a_card_installed_this_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("seamless_launch".to_string())];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1043),
            card: CardId("offworld_office".to_string()),
            server: ServerId::Remote(0),
            installed_this_turn: true,
            ..Default::default()
        }];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("seamless_launch".to_string()) })
                .expect("play seamless launch");

        let offered: Vec<_> = crate::rules::legal_actions(&state, &registry)
            .into_iter()
            .filter(|action| matches!(action, PlayerAction::ToggleCardSelection { .. }))
            .collect();
        assert!(offered.is_empty(), "a card installed this turn is not a legal target: {offered:?}");
        assert_eq!(state.corp.installed[0].advancement_tokens, 0);
    }

    #[test]
    fn spin_doctor_draws_on_rez_then_removes_itself_from_the_game_to_recur_archives() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.r_and_d = (0..4).map(|i| CardId(format!("rd_card_{i}"))).collect();
        state.corp.archives = vec![
            crate::rules::ArchivedCard::faceup(CardId("hedge_fund".to_string())),
            crate::rules::ArchivedCard::facedown(CardId("government_subsidy".to_string())),
        ];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1044),
            card: CardId("spin_doctor".to_string()),
            server: ServerId::Remote(0),
            ..Default::default()
        }];

        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "spin_doctor") })
            .expect("rez spin doctor");
        assert_eq!(state.corp.hq.len(), 2, "rezzing drew 2");

        let (state, _) = close_all_windows(state, &registry);
        let rd_before = state.corp.r_and_d.len();
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "spin_doctor"), ability_index: 0 },
        )
        .expect("activate spin doctor");

        // The cost resolved immediately: it is gone from play, and — the
        // assertion that matters — it is *not* in Archives, so it can never
        // be recurred or counted as a facedown card there.
        assert!(state.corp.installed.is_empty());
        assert!(!state.corp.archives.iter().any(|a| a.card == CardId("spin_doctor".to_string())));
        assert_eq!(state.corp.removed_from_game, vec![CardId("spin_doctor".to_string())]);

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") })
                .expect("pick an archives card");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm the shuffle");

        assert_eq!(state.corp.r_and_d.len(), rd_before + 1);
        assert!(!state.corp.archives.iter().any(|a| a.card == CardId("hedge_fund".to_string())));
    }

    #[test]
    fn jailbreak_runs_a_chosen_central_and_on_success_draws_and_adds_an_access() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = vec![CardId("jailbreak".to_string())];
        state.runner.stack = (0..3).map(|i| CardId(format!("stack_card_{i}"))).collect();
        state.corp.hq = (0..4).map(|i| CardId(format!("hq_card_{i}"))).collect();

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("jailbreak".to_string()) })
            .expect("play jailbreak");

        // "Run HQ or R&D" — Archives and remotes are never offered.
        let offered: Vec<ServerId> = crate::rules::legal_actions(&state, &registry)
            .into_iter()
            .filter_map(|action| match action {
                PlayerAction::ChooseServerForPendingDecision { server } => Some(server),
                _ => None,
            })
            .collect();
        assert_eq!(offered, vec![ServerId::Hq, ServerId::RnD], "only the two named centrals");
        assert_eq!(
            apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Archives }),
            Err(crate::rules::RulesError::ServerNotAllowedForChoice { server: ServerId::Archives })
        );

        let grip_before = state.runner.grip.len();
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq })
                .expect("choose HQ");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to the approach step");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");

        // The on-success rider fired: 1 card drawn, and the extra access is
        // seeded before the breach is computed.
        assert_eq!(state.runner.grip.len(), grip_before + 1, "if successful, draw 1 card");
        assert_eq!(state.active_run.as_ref().unwrap().additional_hq_access, 1, "substituted onto the chosen server");
    }

    /// The additional access is placed on whichever server was actually
    /// chosen, not the placeholder the card JSON authored.
    #[test]
    fn jailbreaks_additional_access_follows_the_chosen_server() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = vec![CardId("jailbreak".to_string())];
        state.runner.stack = (0..3).map(|i| CardId(format!("stack_card_{i}"))).collect();
        state.corp.r_and_d = (0..4).map(|i| CardId(format!("rd_card_{i}"))).collect();

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("jailbreak".to_string()) })
            .expect("play jailbreak");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD })
                .expect("choose R&D");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("continue to the approach step");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");

        let run = state.active_run.as_ref().unwrap();
        assert_eq!(run.additional_rd_access, 1, "R&D, not the authored HQ placeholder");
        assert_eq!(run.additional_hq_access, 0);
    }

    /// Purge against the *real* card data, not synthetic fixtures. The
    /// engine tests in `rules::engine` build cards with `counter_kind`
    /// set by hand, so they would still pass if every shipped virus card
    /// were missing the field — which would leave purge silently doing
    /// nothing in an actual game. This pins the data end.
    ///
    /// Also asserts the full virus roster: if a future set adds a virus
    /// and forgets `counter_kind`, this fails rather than quietly leaving
    /// that card immune to purging.
    #[test]
    fn purge_clears_counters_on_every_real_system_gateway_virus() {
        use crate::dsl::CounterKind;

        let registry = sg_registry();

        let viruses: Vec<CardId> = registry
            .iter()
            .filter(|card| card.counter_kind == Some(CounterKind::Virus))
            .map(|card| card.id.clone())
            .collect();
        let mut sorted = viruses.iter().map(|id| id.0.as_str()).collect::<Vec<_>>();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            vec!["botulus", "conduit", "fermenter", "hantu", "leech", "tranquilizer"],
            "the System Gateway virus roster changed — confirm the new card carries counter_kind: Virus"
        );

        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.runner.rig = viruses
            .iter()
            .map(|id| crate::rules::InstalledRunnerCard { card: id.clone(), counters: 3, ..Default::default() })
            .collect();

        let (next, _events) = apply_action(&state, &registry, PlayerAction::PurgeVirusCounters).expect("corp purges");

        for rigged in &next.runner.rig {
            assert_eq!(rigged.counters, 0, "{} still holds virus counters after a purge", rigged.card.0);
        }
    }

    // ----- Elevation, Stage 1: Flow and Ebb / Sabbatical (ROADMAP Phase 1 §8) -----

    /// Two Corp turns' worth of mandatory draws, so a Runner-turn-start
    /// test can cycle turns without decking the Corp.
    fn corp_rd_filler(state: &mut GameState) {
        state.corp.r_and_d = (0..4).map(|i| CardId(format!("filler_{i}"))).collect();
        state.corp.resources.clicks = Clicks(3);
    }

    #[test]
    fn ritual_draws_one_card_per_click_remaining_after_it_is_played() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = vec![CardId("ritual".to_string())];
        state.runner.stack = (0..5).map(|i| CardId(format!("stack_{i}"))).collect();

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("ritual".to_string()) }).expect("play ritual");
        assert_eq!(state.runner.resources.clicks, Clicks(3), "the click that played it is gone first");
        assert_eq!(state.runner.grip.len(), 3, "one card per remaining click");
        assert_eq!(state.runner.stack.len(), 2);
    }

    #[test]
    fn side_hustle_loads_a_credit_on_install_and_on_every_run_and_cashes_out_at_six() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(2);
        state.runner.grip = vec![CardId("side_hustle".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallResource { card_id: CardId("side_hustle".to_string()) })
                .expect("install side hustle");
        assert_eq!(state.runner.rig[0].counters, 1, "when you install this resource, place 1 credit on it");
        assert_eq!(state.runner.resources.credits, Credits(0));

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        assert_eq!(state.runner.rig[0].counters, 2, "whenever a run begins, place 1 credit on it");

        // At the threshold: the sixth credit cashes the card out.
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![rig_card_with_counters("side_hustle", 5)];
        state.runner.stack = vec![CardId("stack_0".to_string())];
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        assert_eq!(state.runner.resources.credits, Credits(6), "take all credits from this resource");
        assert!(state.runner.rig.is_empty(), "trash it");
        assert_eq!(state.runner.heap, vec![CardId("side_hustle".to_string())]);
        assert_eq!(state.runner.grip, vec![CardId("stack_0".to_string())], "and draw 1 card");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 6 })));
    }

    #[test]
    fn principia_costs_one_less_to_install_for_each_other_installed_icebreaker() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("principia".to_string())];

        let (alone, _) =
            apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("principia".to_string()) })
                .expect("install principia into an empty rig");
        assert_eq!(alone.runner.resources.credits, Credits(6), "10 - 4: no other icebreaker, no discount");

        state.runner.rig = vec![rig_card_with_counters("cleaver", 0), rig_card_with_counters("unity", 0)];
        let (discounted, _) =
            apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("principia".to_string()) })
                .expect("install principia beside two icebreakers");
        assert_eq!(discounted.runner.resources.credits, Credits(8), "10 - (4 - 2): one less per other icebreaker");
        assert_eq!(discounted.runner.rig.len(), 3);
    }

    #[test]
    fn chromatophores_lets_a_fracter_break_the_sentry_it_is_hosted_on() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("cleaver".to_string()),
            base_strength: 3,
            ..Default::default()
        }];
        state.runner.grip = vec![CardId("chromatophores".to_string())];
        state.corp.installed = vec![corp_ice("tithe", ServerId::Hq)];

        // Without the trojan a fracter cannot touch a sentry.
        let bare = enter_encounter_with(state.clone(), &registry, ServerId::Hq);
        let refused = apply_action(
            &bare,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&bare, "cleaver"), ability_index: 0 },
        );
        assert!(matches!(refused, Err(RulesError::InvalidBreakerSubtype { .. })), "{refused:?}");

        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallProgramOnIce { card_id: CardId("chromatophores".to_string()), host: install_of(&state, "tithe") },
        )
        .expect("host chromatophores on tithe");
        let state = enter_encounter_with(state, &registry, ServerId::Hq);
        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: install_of(&state, "cleaver"), ability_index: 0 },
        )
        .expect("host ice gains barrier: the fracter breaks it");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { .. })));
        assert!(state.active_run.as_ref().unwrap().ice[0]
            .subroutines
            .iter()
            .all(|s| s.status == crate::rules::SubroutineStatus::Broken));
    }

    #[test]
    fn gamedragon_pro_hosts_on_an_icebreaker_for_a_strength_and_run_long_boosts_and_goes_with_it() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("cleaver".to_string()),
            install_id: InstallId(1),
            base_strength: 3,
            ..Default::default()
        }];
        state.next_install_id = 2;
        state.runner.grip = vec![CardId("gamedragon_pro".to_string())];
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Hq)];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallHardware { card_id: CardId("gamedragon_pro".to_string()) })
                .expect("install gamedragon");
        assert!(
            matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })),
            "when you install this hardware, you may host it"
        );
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("choose to host");
        let offered = crate::rules::legal_actions(&state, &registry);
        assert!(offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 0 })), "cleaver is offered");
        assert!(!offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 1 })), "the hardware itself is not");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("pick cleaver");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("host on it");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardHosted { .. })));
        assert_eq!(state.runner.rig[1].hosted_on_program, Some(InstallId(1)));
        assert_eq!(
            crate::rules::computed_runner_strength(&state.runner.rig[0], &state, &registry),
            4,
            "host icebreaker gets +1 strength"
        );

        // An encounter-long pump on the host now lasts the run.
        let state = enter_encounter_with(state, &registry, ServerId::Hq);
        let (state, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { target: InstallId(1), ability_index: 1 },
        )
        .expect("pump cleaver");
        assert!(events.iter().any(|e| matches!(
            e,
            crate::rules::GameEvent::StrengthBoosted { duration: crate::dsl::BoostDuration::Run, .. }
        )));
        assert_eq!(state.runner.rig[0].run_strength_buff, 1);
        assert_eq!(state.runner.rig[0].encounter_strength_buff, 0);

        // The host leaving the rig takes the hardware with it.
        let mut state = state;
        let removed = crate::rules::pending_choice::remove_installed_card(&mut state, Side::Runner, &crate::dsl::CardZoneRef::OwnInstalled, InstallId(1))
            .expect("cleaver was installed");
        assert_eq!(removed.0, CardId("cleaver".to_string()));
        assert!(removed.2.iter().any(|e| matches!(e, crate::rules::GameEvent::CardTrashed { side: Side::Runner, card } if card.0 == "gamedragon_pro")));
        assert!(state.runner.rig.is_empty(), "nothing left to host on");
    }

    #[test]
    fn azimat_refills_to_two_credits_each_turn_and_they_pay_trash_costs() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("azimat".to_string())];
        corp_rd_filler(&mut state);

        let (state, _) = apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("azimat".to_string()) })
            .expect("install azimat");
        assert_eq!(state.runner.rig[0].counters, 2, "when you install this program, refill to 2");

        let mut state = state;
        state.runner.rig[0].counters = 0;
        let (state, _) = end_turn_and_settle(state, &registry); // Runner ends -> Corp's turn.
        let (state, _) = end_turn_and_settle(state, &registry); // Corp ends -> Runner's turn: refill.
        assert_eq!(state.runner.rig[0].counters, 2, "before your turn begins, refill to 2");

        // The hosted credits pay a trash cost before the wallet does.
        let mut state = state;
        state.runner.resources.credits = Credits(3);
        state.corp.installed = vec![corp_root("pad_campaign", ServerId::Remote(0))];
        let state = act(state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) });
        let state = act(state, &registry, PlayerAction::ContinueRun);
        let state = act(state, &registry, PlayerAction::CompleteRun);
        assert!(
            crate::rules::legal_actions(&state, &registry)
                .contains(&PlayerAction::TrashAccessedCard { card_id: CardId("pad_campaign".to_string()) }),
            "3[c] in the wallet plus 2 hosted afford the 4[c] trash cost"
        );
        let state = act(state, &registry, PlayerAction::TrashAccessedCard { card_id: CardId("pad_campaign".to_string()) });
        assert_eq!(state.runner.rig[0].counters, 0, "hosted credits are spent first");
        assert_eq!(state.runner.resources.credits, Credits(1), "3 - (4 - 2)");
        assert_eq!(state.corp.archives, vec![ArchivedCard::faceup(CardId("pad_campaign".to_string()))]);
    }

    #[test]
    fn devadatta_drone_installs_with_two_power_counters_and_spends_one_for_an_extra_rnd_access() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("devadatta_drone".to_string())];
        state.corp.r_and_d = (0..3).map(|i| CardId(format!("rd_card_{i}"))).collect();

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("devadatta_drone".to_string()) })
                .expect("install devadatta drone");
        assert_eq!(state.runner.rig[0].counters, 2);

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::RnD }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, events) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("the run succeeds");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::PendingPaidChoiceOffered { side: Side::Runner })));
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None })
                .expect("remove 1 hosted power counter");
        assert_eq!(state.runner.rig[0].counters, 1);
        assert_eq!(state.active_run.as_ref().unwrap().additional_rd_access, 1, "access 1 additional card");
    }

    #[test]
    fn scrounge_costs_a_second_click_installs_a_program_from_the_heap_and_may_bury_another() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        state.runner.grip = vec![CardId("scrounge".to_string())];
        state.runner.heap = vec![CardId("cleaver".to_string()), CardId("corroder".to_string()), CardId("sure_gamble".to_string())];
        state.runner.stack = vec![CardId("stack_top".to_string())];

        // A Double: with a single click left it cannot be played at all.
        let mut single = state.clone();
        single.runner.resources.clicks = Clicks(1);
        let play = PlayerAction::PlayEvent { card_id: CardId("scrounge".to_string()) };
        assert!(matches!(apply_action(&single, &registry, play.clone()), Err(RulesError::NotEnoughClicks { .. })));
        assert!(!crate::rules::legal_actions(&single, &registry).contains(&play));

        let (state, _) = apply_action(&state, &registry, play).expect("play scrounge");
        assert_eq!(state.runner.resources.clicks, Clicks(2), "4 - 1 (play) - 1 (additional cost)");
        assert_eq!(state.runner.resources.credits, Credits(9));
        let offered = crate::rules::legal_actions(&state, &registry);
        assert!(offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 0 })), "cleaver");
        assert!(offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 1 })), "corroder");
        assert!(!offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 2 })), "sure gamble is an event");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("pick cleaver");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("install it from the heap");
        assert!(state.runner.rig.iter().any(|c| c.card.0 == "cleaver"));
        assert_eq!(state.runner.resources.credits, Credits(6), "9 - 3: paying its install cost");

        assert!(
            matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })),
            "you may add 1 program from your heap to the bottom of your stack"
        );
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("choose to");
        let offered = crate::rules::legal_actions(&state, &registry);
        assert!(offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 0 })), "corroder is the only program left");
        assert_eq!(offered.iter().filter(|a| matches!(a, PlayerAction::ToggleCardSelection { .. })).count(), 1);
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("pick corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("bury it");
        assert_eq!(state.runner.stack, vec![CardId("corroder".to_string()), CardId("stack_top".to_string())], "bottom, not top");
        assert_eq!(state.runner.heap, vec![CardId("sure_gamble".to_string()), CardId("scrounge".to_string())]);
    }

    #[test]
    fn magdalene_may_install_a_program_she_discarded_to_hand_size_and_the_turn_still_hands_over() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.identity = Some(CardId("magdalene_keino_chemutai".to_string()));
        state.runner.resources.clicks = Clicks(0);
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![
            CardId("corroder".to_string()),
            CardId("sure_gamble".to_string()),
            CardId("grip_2".to_string()),
            CardId("grip_3".to_string()),
            CardId("grip_4".to_string()),
            CardId("grip_5".to_string()),
        ];
        corp_rd_filler(&mut state);

        let state = act(state, &registry, PlayerAction::EndTurn);
        assert_eq!(state.phase, GamePhase::Discard { side: Side::Runner, required: 1 });
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::DiscardCard { card_id: CardId("corroder".to_string()) }).expect("discard");
        assert_eq!(state.runner.discarded_this_discard_phase, vec![CardId("corroder".to_string())]);
        assert!(
            matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })),
            "you may install 1 program or piece of hardware from among those cards"
        );

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("choose to");
        let offered = crate::rules::legal_actions(&state, &registry);
        assert_eq!(offered.iter().filter(|a| matches!(a, PlayerAction::ToggleCardSelection { .. })).count(), 1, "only the discard");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("pick corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("install it");
        assert!(state.runner.rig.iter().any(|c| c.card.0 == "corroder"));
        assert_eq!(state.runner.resources.credits, Credits(3), "paying its cost");

        // The decision resolved during the Corp's start-of-turn window; the
        // game goes on with the Corp to act and nobody stuck.
        let (state, _) = close_all_windows(state, &registry);
        assert_eq!(state.phase, GamePhase::Action(Side::Corp));
        assert!(!crate::rules::legal_actions(&state, &registry).is_empty());
    }

    /// An upgrade in the root of Archives is accessed with the pile and
    /// trashed like any other root install — it must leave the table, not
    /// merely be paid for. See `run::access::move_to_archives`.
    #[test]
    fn trashing_an_upgrade_installed_in_the_root_of_archives_removes_it_from_the_table() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(10);
        let mut vault = corp_root("malapert_data_vault", ServerId::Archives);
        vault.rezzed = false;
        state.corp.installed = vec![vault];

        let state = act(state, &registry, PlayerAction::InitiateRun { server: ServerId::Archives });
        let state = act(state, &registry, PlayerAction::ContinueRun);
        let state = act(state, &registry, PlayerAction::CompleteRun);
        let trash = PlayerAction::TrashAccessedCard { card_id: CardId("malapert_data_vault".to_string()) };
        assert!(crate::rules::legal_actions(&state, &registry).contains(&trash), "the root upgrade is accessed with the pile");
        let state = act(state, &registry, trash);

        assert!(state.corp.installed.is_empty(), "the trashed upgrade has left the table");
        assert_eq!(state.corp.archives, vec![ArchivedCard::faceup(CardId("malapert_data_vault".to_string()))]);
    }

    // ----- Elevation, Stage 2: Enthusiasm / Tickets, please -----

    fn runner_turn(credits: u32, clicks: u32) -> GameState {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(clicks);
        state.runner.resources.credits = Credits(credits);
        state
    }

    #[test]
    fn clean_getaway_runs_any_server_and_pays_six_on_success() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("clean_getaway".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("clean_getaway".to_string()) })
            .expect("play clean getaway");
        assert_eq!(state.runner.resources.credits, Credits(2), "5 - 3");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq })
            .expect("run any server");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, events) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 6 })));
        assert_eq!(state.runner.resources.credits, Credits(8));
    }

    #[test]
    fn lie_low_is_a_double_that_draws_four_or_removes_two_tags() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.tags = 3;
        state.runner.grip = vec![CardId("lie_low".to_string())];
        state.runner.stack = (0..5).map(|i| CardId(format!("s{i}"))).collect();

        let (played, _) =
            apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("lie_low".to_string()) }).expect("play lie low");
        assert_eq!(played.runner.resources.clicks, Clicks(2), "a Double");
        let (drew, _) = apply_action(&played, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("draw 4");
        assert_eq!(drew.runner.grip.len(), 4);
        let (untagged, _) = apply_action(&played, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("remove tags");
        assert_eq!(untagged.runner.tags, 1, "remove up to 2 tags");
    }

    #[test]
    fn maintenance_access_runs_archives_then_approaches_hq_without_its_ice() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("maintenance_access".to_string())];
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Hq)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("maintenance_access".to_string()) })
            .expect("play maintenance access");
        assert_eq!(state.runner.resources.clicks, Clicks(2), "a Double");
        assert_eq!(state.active_run.as_ref().unwrap().server, ServerId::Archives);

        let (state, events) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach archives");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunRedirected { from: ServerId::Archives, to: ServerId::Hq })), "{events:?}");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::ServerApproached { server: ServerId::Hq })));
        assert!(!events.iter().any(|e| matches!(e, crate::rules::GameEvent::IceEncountered { .. })), "HQ's ice is not encountered");
        let run = state.active_run.as_ref().unwrap();
        assert_eq!(run.server, ServerId::Hq);
        assert_eq!(run.position, run.ice.len(), "HQ's ice counts as passed");

        let (state, events) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success on hq");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunSucceeded { server: ServerId::Hq })));
        // The pre-access window, then the breach — of HQ, not Archives.
        let (_, events) = close_all_windows(state, &registry);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardAccessed { card, server: ServerId::Hq, .. } if card.0 == "hedge_fund")), "{events:?}");
    }

    #[test]
    fn rising_tide_gains_a_strength_for_each_fracter_in_the_heap() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard { card: CardId("rising_tide".to_string()), base_strength: 1, ..Default::default() }];
        state.runner.heap = vec![CardId("cleaver".to_string()), CardId("corroder".to_string()), CardId("sure_gamble".to_string()), CardId("unity".to_string())];
        assert_eq!(crate::rules::computed_runner_strength(&state.runner.rig[0], &state, &registry), 3, "1 + two fracters (Unity is a decoder)");
    }

    #[test]
    fn open_market_loads_six_credits_that_pay_for_jobs_and_connections_and_trashes_itself_when_empty() {
        let registry = sg_registry();
        let mut state = runner_turn(2, 4);
        state.runner.grip = vec![CardId("open_market".to_string())];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InstallResource { card_id: CardId("open_market".to_string()) })
            .expect("install open market");
        assert_eq!(state.runner.rig[0].counters, 6, "when you install this resource, load 6 credits onto it");
        assert_eq!(state.runner.resources.credits, Credits(0), "its own cost comes from the wallet");

        let mut state = runner_turn(1, 4);
        state.runner.rig = vec![rig_card_with_counters("open_market", 6)];
        state.runner.grip = vec![CardId("telework_contract".to_string()), CardId("telework_contract".to_string())];
        corp_rd_filler(&mut state);
        let (state, _) = apply_action(&state, &registry, PlayerAction::InstallResource { card_id: CardId("telework_contract".to_string()) })
            .expect("a 1[c] Job, paid from the market");
        assert_eq!(state.runner.rig[0].counters, 5, "hosted credits pay first");
        assert_eq!(state.runner.resources.credits, Credits(1), "the wallet is untouched");

        let (state, _) = end_turn_and_settle(state, &registry);
        let (state, _) = end_turn_and_settle(state, &registry);
        assert_eq!(state.runner.rig[0].counters, 4, "when your turn begins, take 1 credit");
        assert_eq!(state.runner.resources.credits, Credits(2));

        // Drained to zero by an install: trashed on the spot.
        let mut state = state;
        state.runner.rig[0].counters = 1;
        let (state, events) = apply_action(&state, &registry, PlayerAction::InstallResource { card_id: CardId("telework_contract".to_string()) })
            .expect("a second 1[c] Job, paid from the last hosted credit");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardTrashed { side: Side::Runner, card } if card.0 == "open_market")));
        assert!(!state.runner.rig.iter().any(|c| c.card.0 == "open_market"), "when it is empty, trash it");
        assert_eq!(state.runner.resources.credits, Credits(2), "nothing from the wallet");
    }

    #[test]
    fn knickknack_obrian_may_trash_another_installed_card_for_its_printed_cost_once_per_turn() {
        let registry = sg_registry();
        let mut state = runner_turn(0, 4);
        state.runner.rig = vec![
            rig_card_with_counters("knickknack_obrian", 0),
            crate::rules::InstalledRunnerCard { card: CardId("cleaver".to_string()), base_strength: 3, ..Default::default() },
        ];
        state.runner.stack = vec![CardId("s0".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("a run begins");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("choose to trash");
        let offered = crate::rules::legal_actions(&state, &registry);
        assert!(!offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 0 })), "not itself");
        assert!(offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 1 })), "cleaver");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("pick cleaver");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert_eq!(state.runner.resources.credits, Credits(3), "gain its printed install cost");
        assert_eq!(state.runner.heap, vec![CardId("cleaver".to_string())]);
        assert_eq!(state.runner.grip, vec![CardId("s0".to_string())], "and draw 1 card");

        // A second run the same turn: no offer.
        let (state, _) = apply_action(&state, &registry, PlayerAction::JackOut).unwrap_or_else(|_| (state.clone(), Vec::new()));
        let mut state = state;
        state.active_run = None;
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("second run");
        assert!(state.pending_decision.is_none(), "the first time each turn only");
    }

    #[test]
    fn illumination_installs_up_to_three_cards_from_the_grip_for_one_less_each() {
        let registry = sg_registry();
        let mut state = runner_turn(3, 4);
        state.runner.grip = vec![CardId("illumination".to_string()), CardId("corroder".to_string()), CardId("cleaver".to_string())];
        state.corp.r_and_d = (0..3).map(|i| CardId(format!("rd{i}"))).collect();

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("illumination".to_string()) }).expect("play");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD }).expect("run r&d");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })), "install?");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("yes");
        let offered = crate::rules::legal_actions(&state, &registry);
        assert_eq!(offered.iter().filter(|a| matches!(a, PlayerAction::ToggleCardSelection { .. })).count(), 2, "corroder at 1, cleaver at 2 — both affordable with the discount");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("cleaver");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("install for 2");
        assert_eq!(state.runner.resources.credits, Credits(1));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("again");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("install for 1");
        assert_eq!(state.runner.resources.credits, Credits(0));
        assert_eq!(state.runner.rig.len(), 2);
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { .. })), "a third offer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("decline");
        assert!(state.pending_decision.is_none());
        assert!(state.active_run.is_some(), "the run goes on to its accesses");
    }

    #[test]
    fn madani_hosts_programs_from_the_grip_and_installs_one_per_turn_and_they_leave_with_it() {
        let registry = sg_registry();
        let mut state = runner_turn(10, 4);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard { card: CardId("madani".to_string()), install_id: InstallId(1), ..Default::default() }];
        state.next_install_id = 2;
        state.runner.grip = vec![CardId("cleaver".to_string()), CardId("corroder".to_string()), CardId("sure_gamble".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId(1), ability_index: 0 }).expect("click: host");
        let offered = crate::rules::legal_actions(&state, &registry);
        assert!(!offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 2 })), "sure gamble is not a program");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("cleaver");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("host both");
        assert_eq!(state.runner.rig[0].hosted_cards, vec![CardId("cleaver".to_string()), CardId("corroder".to_string())]);
        assert_eq!(state.runner.grip, vec![CardId("sure_gamble".to_string())]);
        assert_eq!(state.runner.resources.clicks, Clicks(3));

        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId(1), ability_index: 1 }).expect("0[c]: install one");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("corroder");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("install it, paying");
        assert!(state.runner.rig.iter().any(|c| c.card.0 == "corroder"));
        assert_eq!(state.runner.rig[0].hosted_cards, vec![CardId("cleaver".to_string())]);
        assert_eq!(state.runner.resources.credits, Credits(8), "10 - 2: paying its install cost");
        assert!(
            apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId(1), ability_index: 1 }).is_err(),
            "once per turn"
        );

        // The console leaving takes its hosted cards to the heap.
        let mut state = state;
        let removed = crate::rules::pending_choice::remove_installed_card(&mut state, Side::Runner, &crate::dsl::CardZoneRef::OwnInstalled, InstallId(1))
            .expect("madani was installed");
        assert!(removed.2.iter().any(|e| matches!(e, crate::rules::GameEvent::CardTrashed { card, .. } if card.0 == "cleaver")));
        assert!(state.runner.heap.contains(&CardId("cleaver".to_string())));
    }

    #[test]
    fn dewi_subrotoputri_flips_on_a_successful_run_when_memory_is_full_and_back_when_it_is_not() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.identity = Some(CardId("dewi_subrotoputri".to_string()));
        // `memory_units` is derived (`memory::refresh`): four 1[mu] programs
        // against the base 4[mu] is "full".
        state.runner.rig = ["cleaver", "corroder", "unity", "echelon"].map(|id| rig_card_with_counters(id, 0)).to_vec();
        state.runner.stack = vec![CardId("s0".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success with full memory");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })), "you may flip");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("flip");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::IdentityFlipped { side: Side::Runner })));
        assert!(state.runner.identity_flipped);
        assert_eq!(state.runner.resources.credits, Credits(6), "and gain 1 credit");
        assert!(state.pending_decision.is_none(), "the back side did not also fire");

        // Flipped, with memory to spare: the back side offers to flip back.
        let mut state = runner_turn(5, 4);
        state.runner.identity = Some(CardId("dewi_subrotoputri".to_string()));
        state.runner.identity_flipped = true;
        state.runner.rig = ["cleaver", "corroder", "unity"].map(|id| rig_card_with_counters(id, 0)).to_vec();
        state.runner.stack = vec![CardId("s0".to_string())];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success with unused memory");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("flip back");
        assert!(!state.runner.identity_flipped);
        assert_eq!(state.runner.grip, vec![CardId("s0".to_string())], "and draw 1 card");

        // Full memory on the back side: no offer at all.
        let mut state = runner_turn(5, 4);
        state.runner.identity = Some(CardId("dewi_subrotoputri".to_string()));
        state.runner.identity_flipped = true;
        state.runner.rig = ["cleaver", "corroder", "unity", "echelon"].map(|id| rig_card_with_counters(id, 0)).to_vec();
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success");
        assert!(state.pending_decision.is_none());
    }

    /// Three simultaneous Runner triggers where the chosen one parks a paid
    /// choice: the remaining two must not be re-parked as an order decision
    /// beside it, or neither can be resolved (the Stage 2 deep-sweep
    /// deadlock, seed 126). See `pending_choice::resolve_choose_trigger_to_resolve`.
    #[test]
    fn a_chosen_trigger_that_parks_a_paid_choice_does_not_deadlock_the_remaining_triggers() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.rig = (0..3).map(|i| crate::rules::InstalledRunnerCard {
            card: CardId("devadatta_drone".to_string()),
            install_id: InstallId(10 + i),
            counters: 2,
            ..Default::default()
        }).collect();
        state.next_install_id = 20;
        state.corp.r_and_d = (0..6).map(|i| CardId(format!("rd{i}"))).collect();

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::RnD }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success: three drones react");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseTriggerOrder { .. })));

        let (mut state, _) = apply_action(&state, &registry, PlayerAction::ChooseTriggerToResolve { index: 0 }).expect("first drone");
        for step in 0..3 {
            let legal = crate::rules::legal_actions(&state, &registry);
            assert!(!legal.is_empty(), "step {step}: nothing to do with {:?} / {:?}", state.pending_decision, state.pending_paid_choice);
            assert!(state.pending_paid_choice.is_some(), "step {step}: a drone's paid choice is parked");
            let (next, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None })
                .expect("pay a counter");
            state = next;
        }
        assert_eq!(state.active_run.as_ref().unwrap().additional_rd_access, 3, "every drone paid");
        assert!(state.pending_decision.is_none() && state.pending_paid_choice.is_none());
    }

    // ----- Elevation, Stage 3: Bowel Movements / Dashing Mad -----

    /// Drives the game forward through the "nothing to decide" actions —
    /// passing priority, passing accessed cards, continuing and completing
    /// a run — until a decision is parked or the run has ended.
    fn advance_until_choice(mut state: GameState, registry: &CardRegistry) -> GameState {
        for _ in 0..60 {
            if state.pending_decision.is_some() || state.pending_paid_choice.is_some() {
                return state;
            }
            let legal = crate::rules::legal_actions(&state, registry);
            let Some(action) = legal.into_iter().find(|a| {
                matches!(
                    a,
                    PlayerAction::PassPriority { .. }
                        | PlayerAction::PassAccessedCard { .. }
                        | PlayerAction::ContinueRun
                        | PlayerAction::CompleteRun
                )
            }) else {
                return state;
            };
            let (next, _) = apply_action(&state, registry, action.clone()).unwrap_or_else(|e| panic!("{action:?}: {e:?}"));
            state = next;
        }
        state
    }

    #[test]
    fn rent_rioters_spends_three_clicks_and_itself_for_nine_credits() {
        let registry = sg_registry();
        let mut state = runner_turn(0, 4);
        state.runner.rig = vec![rig_card_with_counters("rent_rioters", 0)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "rent_rioters"), ability_index: 0 })
            .expect("click click click, trash");
        assert_eq!(state.runner.resources.clicks, Clicks(1));
        assert_eq!(state.runner.resources.credits, Credits(9));
        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.heap, vec![CardId("rent_rioters".to_string())]);
    }

    #[test]
    fn gourmand_trashes_itself_to_trash_a_non_agenda_it_is_accessing_and_draws() {
        let registry = sg_registry();
        let mut state = runner_turn(0, 4);
        state.runner.rig = vec![rig_card_with_counters("gourmand", 0)];
        state.runner.stack = vec![CardId("s0".to_string())];
        let mut asset = corp_root("pad_campaign", ServerId::Remote(0));
        asset.rezzed = false;
        state.corp.installed = vec![asset];

        let state = act(state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) });
        let state = act(state, &registry, PlayerAction::ContinueRun);
        let state = act(state, &registry, PlayerAction::CompleteRun);
        let gourmand = install_of(&state, "gourmand");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: gourmand, ability_index: 0 })
            .expect("access -> trash: trash the card you are accessing");
        assert!(state.corp.installed.is_empty(), "the asset is trashed without paying its trash cost");
        assert_eq!(state.runner.heap, vec![CardId("gourmand".to_string())]);
        assert_eq!(state.runner.grip, vec![CardId("s0".to_string())], "and draw 1 card");

        // Not an agenda: refused while accessing one.
        let mut state = runner_turn(0, 4);
        state.runner.rig = vec![rig_card_with_counters("gourmand", 0)];
        state.corp.installed = vec![corp_root("offworld_office", ServerId::Remote(0))];
        let state = act(state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) });
        let state = act(state, &registry, PlayerAction::ContinueRun);
        let state = act(state, &registry, PlayerAction::CompleteRun);
        let gourmand = install_of(&state, "gourmand");
        assert!(apply_action(&state, &registry, PlayerAction::ActivateAbility { target: gourmand, ability_index: 0 }).is_err());
    }

    #[test]
    fn hantu_installs_with_two_virus_counters_and_spends_them_for_strength() {
        let registry = sg_registry();
        let mut state = runner_turn(10, 4);
        state.runner.grip = vec![CardId("hantu".to_string())];
        state.corp.installed = vec![corp_ice("tithe", ServerId::Hq)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("hantu".to_string()) }).expect("install");
        assert_eq!(state.runner.rig[0].counters, 2);
        let state = enter_encounter_with(state, &registry, ServerId::Hq);
        let hantu = install_of(&state, "hantu");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: hantu, ability_index: 1 }).expect("hosted virus counter: +2 strength");
        assert_eq!(state.runner.rig[0].counters, 1);
        assert_eq!(state.runner.rig[0].effective_strength(), 4);
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("the corp passes back");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: hantu, ability_index: 0 }).expect("break a sentry subroutine");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { .. })));
        assert_eq!(state.runner.resources.credits, Credits(6), "10 - 3 (install) - 1 (break)");
    }

    #[test]
    fn charm_offensive_runs_archives_and_may_then_trash_a_rezzed_copy_of_an_accessed_card() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("charm_offensive".to_string())];
        state.corp.archives = vec![ArchivedCard::facedown(CardId("nico_campaign".to_string()))];
        state.corp.installed = vec![corp_root("nico_campaign", ServerId::Remote(0)), corp_root("pad_campaign", ServerId::Remote(1))];

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("charm_offensive".to_string()) }).expect("play");
        assert_eq!(state.active_run.as_ref().unwrap().server, ServerId::Archives);
        let state = advance_until_choice(state, &registry);
        assert!(state.active_run.is_none(), "the run ended");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })), "when that run ends, you may");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("choose to trash");
        let offered = crate::rules::legal_actions(&state, &registry);
        assert!(offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 0 })), "the rezzed nico campaign, a copy of what was accessed");
        assert!(!offered.iter().any(|a| matches!(a, PlayerAction::ToggleCardSelection { position: 1 })), "pad campaign was not accessed");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("pick");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert_eq!(state.corp.installed.len(), 1);
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "nico_campaign" && !a.facedown), "trashed from the table, faceup");
    }

    #[test]
    fn detente_may_host_a_random_hq_card_on_the_first_successful_hq_run_each_turn() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.rig = vec![rig_card_with_counters("detente", 0)];
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string()), CardId("pad_campaign".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success on hq");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })));
        let (state, events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("host one");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardHosted { host, .. } if host.0 == "detente")));
        assert_eq!(state.runner.rig[0].hosted_cards.len(), 1);
        assert_eq!(state.corp.hq.len(), 2);

        // A second HQ run this turn: no offer.
        let state = advance_until_choice(state, &registry);
        let mut state = state;
        state.active_run = None;
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run again");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success");
        assert!(state.pending_decision.is_none(), "the first time each turn only");
    }

    #[test]
    fn shred_makes_the_corps_first_end_the_run_cost_a_random_hq_card_per_root_card() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("shred".to_string())];
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("nico_campaign".to_string())];
        let mut vault = corp_root("pad_campaign", ServerId::Remote(0));
        vault.rezzed = false;
        state.corp.installed = vec![corp_ice("wall_of_static", ServerId::Remote(0)), vault];

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("shred".to_string()) }).expect("play");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Remote(0) }).expect("run");
        assert!(state.active_run.as_ref().unwrap().end_run_prevention.is_some(), "armed on start");
        // Approach, encounter, and let Wall of Static's End the run fire.
        let state = advance_until_choice(state, &registry);
        assert!(state.active_run.is_some(), "the run has not ended: the end was intercepted");
        let paid = state.pending_paid_choice.as_ref().expect("the corp's choice");
        assert_eq!(paid.side, Side::Corp);
        assert_eq!(paid.cost, crate::dsl::Cost::TrashRandomFromHq(1), "one card in the root");

        let (paid_state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("the corp pays");
        assert_eq!(paid_state.corp.hq.len(), 1, "one random HQ card trashed");
        assert!(paid_state.corp.archives.iter().all(|a| !a.facedown), "revealed, so faceup");
        assert!(paid_state.active_run.is_none(), "and the run ends");

        let (declined, _) = apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("the corp declines");
        assert!(declined.active_run.is_some(), "the run goes on");
        assert_eq!(declined.corp.hq.len(), 2);
    }

    #[test]
    fn cacophony_counts_the_first_trash_each_turn_and_sabotages_three_at_action_phase_end() {
        let registry = sg_registry();
        // Counting: the first trash from access this turn, once.
        let mut state = runner_turn(10, 4);
        state.runner.rig = vec![rig_card_with_counters("cacophony", 0)];
        state.corp.installed = vec![corp_root("pad_campaign", ServerId::Remote(0)), corp_root("pad_campaign", ServerId::Remote(1))];
        let state = act(state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) });
        let state = act(state, &registry, PlayerAction::ContinueRun);
        let state = act(state, &registry, PlayerAction::CompleteRun);
        let state = act(state, &registry, PlayerAction::TrashAccessedCard { card_id: CardId("pad_campaign".to_string()) });
        assert_eq!(state.runner.rig[0].counters, 1);
        let state = act(state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(1) });
        let state = act(state, &registry, PlayerAction::ContinueRun);
        let state = act(state, &registry, PlayerAction::CompleteRun);
        let state = act(state, &registry, PlayerAction::TrashAccessedCard { card_id: CardId("pad_campaign".to_string()) });
        assert_eq!(state.runner.rig[0].counters, 1, "the first time each turn");

        // Sabotage: the Corp chooses from HQ and the rest comes off R&D.
        let mut state = runner_turn(0, 0);
        state.runner.rig = vec![rig_card_with_counters("cacophony", 2)];
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("nico_campaign".to_string()), CardId("ice_wall".to_string())];
        state.corp.r_and_d = vec![CardId("r0".to_string()), CardId("r1".to_string())];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn: action phase ends");
        assert_eq!(state.pending_paid_choice.as_ref().map(|p| p.side), Some(Side::Runner), "you may remove 2 counters");
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("sabotage 3");
        assert_eq!(state.runner.rig[0].counters, 0);
        match &state.pending_decision {
            Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, min, max, .. }) => {
                assert_eq!((*min, *max), (1, 3), "R&D covers 2 of 3, so at least 1 from HQ; at most all 3");
            }
            other => panic!("expected the corp's HQ choice, got {other:?}"),
        }
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("hedge fund");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it and mill 2");
        assert_eq!(state.corp.hq.len(), 2);
        assert!(state.corp.r_and_d.is_empty(), "the other two came off the top of R&D");
        assert_eq!(state.corp.archives.len(), 3);
        assert!(state.corp.archives.iter().all(|a| a.facedown));
    }

    #[test]
    fn phoenix_gains_a_credit_and_makes_the_corp_trash_from_hq_after_a_subroutine_resolved() {
        let registry = sg_registry();
        let mut state = runner_turn(10, 4);
        state.runner.identity = Some(CardId("ryo_phoenix_ono".to_string()));
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())];
        state.corp.installed = vec![corp_ice("whitespace", ServerId::Remote(0))];

        // Without a resolved subroutine: nothing.
        let (quiet, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run hq, no ice");
        let (quiet, _) = apply_action(&quiet, &registry, PlayerAction::ContinueRun).expect("approach");
        let (quiet, _) = apply_action(&quiet, &registry, PlayerAction::CompleteRun).expect("success");
        assert!(quiet.pending_decision.is_none());

        let state = enter_encounter_with(state, &registry, ServerId::Remote(0));
        let state = advance_until_choice(state, &registry);
        assert_eq!(
            state.runner.resources.credits,
            Credits(8),
            "10 - 3 (whitespace fired) + 1 (phoenix); run {:?} pending {:?} paid {:?} legal {:?}",
            state.active_run.as_ref().map(|r| (r.phase, r.subroutine_resolved, r.position)),
            state.pending_decision,
            state.pending_paid_choice,
            crate::rules::legal_actions(&state, &registry)
        );
        assert_eq!(state.runner.resources.credits, Credits(8), "gain 1 credit");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, .. })), "the corp trashes 1 card from HQ");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("ice wall");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert_eq!(state.corp.hq, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(state.corp.archives, vec![ArchivedCard::facedown(CardId("ice_wall".to_string()))]);
    }

    /// Two rig cards react to a run starting; the first one resolved
    /// trashes the second. The second's deferred trigger must stand down,
    /// not fail the selection that trashed it (Stage 3 deep sweep, seed
    /// 182). See `dispatcher::still_applies`.
    #[test]
    fn a_deferred_trigger_on_a_card_that_left_play_stands_down() {
        let registry = sg_registry();
        let mut state = runner_turn(0, 4);
        state.runner.rig = vec![
            crate::rules::InstalledRunnerCard { card: CardId("knickknack_obrian".to_string()), install_id: InstallId(1), ..Default::default() },
            crate::rules::InstalledRunnerCard { card: CardId("side_hustle".to_string()), install_id: InstallId(2), ..Default::default() },
        ];
        state.next_install_id = 3;
        state.runner.stack = vec![CardId("s0".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run: both react");
        let Some(crate::rules::PendingDecision::ChooseTriggerOrder { pending, .. }) = &state.pending_decision else {
            panic!("expected a trigger order choice, got {:?}", state.pending_decision);
        };
        let knickknack = pending.iter().position(|t| t.card.0 == "knickknack_obrian").expect("knickknack reacts");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseTriggerToResolve { index: knickknack }).expect("knickknack first");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("choose to trash");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("side hustle");
        assert!(crate::rules::legal_actions(&state, &registry).contains(&PlayerAction::ConfirmCardSelection), "the trash must be confirmable");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash side hustle");
        assert_eq!(state.runner.heap, vec![CardId("side_hustle".to_string())]);
        assert_eq!(state.runner.resources.credits, Credits(2), "its printed cost");
        assert!(state.deferred_triggers.is_empty(), "side hustle's own run-start trigger stood down");
    }

    // ---- Elevation Stage 4: Prick Thyself, Shootin' 'n' Lootin', Professional Opportunities

    #[test]
    fn topan_installs_one_card_a_turn_for_a_click_two_cheaper_and_suffers_a_meat_damage() {
        let registry = sg_registry();
        let mut state = runner_turn(3, 4);
        state.runner.identity = Some(CardId("topan".to_string()));
        state.runner.grip = vec![CardId("corroder".to_string()), CardId("sure_gamble".to_string()), CardId("diesel".to_string())];

        let legal = crate::rules::legal_actions(&state, &registry);
        assert!(legal.contains(&PlayerAction::ActivateAbility { target: InstallId::RUNNER_IDENTITY, ability_index: 0 }), "the identity's click ability is offered");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId::RUNNER_IDENTITY, ability_index: 0 })
            .expect("click: install paying 2 less");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("corroder");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("install it");
        assert!(state.runner.rig.iter().any(|c| c.card.0 == "corroder"));
        assert_eq!(state.runner.resources.credits, Credits(3), "2 - 2: nothing spent");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::ProgramInstalled { credits_paid: 0, .. })));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::DamageTaken { damage_type: crate::dsl::DamageType::Meat, amount: 1 })));
        assert_eq!(state.runner.grip.len(), 1, "one of the other two cards was discarded to the damage");
        assert_eq!(state.runner.heap.len(), 1);
        assert!(
            apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId::RUNNER_IDENTITY, ability_index: 0 }).is_err(),
            "once per turn"
        );
    }

    #[test]
    fn bling_hosts_the_top_of_the_stack_on_a_free_install_and_the_hosted_card_plays_from_the_host() {
        let registry = sg_registry();
        let mut state = runner_turn(7, 4);
        state.runner.rig = vec![rig_card_with_counters("bling", 0)];
        state.runner.rig[0].hosted_cards_playable = true;
        state.runner.stack = vec![CardId("diesel".to_string()), CardId("sure_gamble".to_string())];
        state.runner.grip = vec![CardId("marjanah".to_string()), CardId("corroder".to_string())];

        // A paid install: no offer.
        let (state, _) = apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("corroder".to_string()) }).expect("install, paying 2");
        assert!(state.pending_decision.is_none(), "credits were spent");
        // A free one: host the top of the stack.
        let (state, _) = apply_action(&state, &registry, PlayerAction::InstallProgram { card_id: CardId("marjanah".to_string()) }).expect("install for 0");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })), "you may host");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("host it");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardHosted { card, host } if card.0 == "sure_gamble" && host.0 == "bling")));
        assert_eq!(state.runner.rig[0].hosted_cards, vec![CardId("sure_gamble".to_string())]);
        assert_eq!(state.runner.stack, vec![CardId("diesel".to_string())]);

        // The hosted card plays as if it were in the grip.
        let legal = crate::rules::legal_actions(&state, &registry);
        assert!(legal.contains(&PlayerAction::PlayEvent { card_id: CardId("sure_gamble".to_string()) }));
        let index = crate::rules::ActionSpace::index_of(&state, &PlayerAction::PlayEvent { card_id: CardId("sure_gamble".to_string()) })
            .expect("the action space numbers a hosted card's play");
        assert_eq!(crate::rules::ActionSpace::action_at(&state, index), Some(PlayerAction::PlayEvent { card_id: CardId("sure_gamble".to_string()) }));
        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("sure_gamble".to_string()) }).expect("play it off the host");
        assert_eq!(state.runner.resources.credits, Credits(9), "7 - 2 (corroder) - 5 + 9");
        assert!(state.runner.rig[0].hosted_cards.is_empty());
        assert_eq!(state.runner.heap, vec![CardId("sure_gamble".to_string())]);
    }

    #[test]
    fn bling_trashes_its_hosted_cards_when_the_runners_discard_phase_ends() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 0);
        corp_rd_filler(&mut state);
        state.runner.rig = vec![rig_card_with_counters("bling", 0)];
        state.runner.rig[0].hosted_cards = vec![CardId("diesel".to_string()), CardId("sure_gamble".to_string())];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn: the end-of-turn window first");
        let (state, events) = close_all_windows(state, &registry);
        assert!(state.runner.rig[0].hosted_cards.is_empty());
        assert_eq!(state.runner.heap, vec![CardId("diesel".to_string()), CardId("sure_gamble".to_string())]);
        assert_eq!(events.iter().filter(|e| matches!(e, crate::rules::GameEvent::CardTrashed { side: Side::Runner, .. })).count(), 2);
    }

    #[test]
    fn barry_baz_wong_may_install_a_resource_or_hardware_when_the_corp_rezzes_ice() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.identity = Some(CardId("barry_baz_wong".to_string()));
        state.runner.grip = vec![CardId("cleaver".to_string()), CardId("telework_contract".to_string())];
        state.corp.resources.credits = Credits(10);
        state.corp.installed = vec![ice_installed("ice_wall", ServerId::Hq, false)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "ice_wall") }).expect("rez");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })), "you may install");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("install one");
        let legal = crate::rules::legal_actions(&state, &registry);
        assert!(!legal.contains(&PlayerAction::ToggleCardSelection { position: 0 }), "cleaver is a program");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("telework contract");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("installed, paying");
        assert!(state.runner.rig.iter().any(|c| c.card.0 == "telework_contract"));
        assert_eq!(state.runner.resources.credits, Credits(4), "its printed cost");
        assert!(state.active_run.is_some(), "mid-run, and the run goes on");

        // An asset rezzed on the Corp's turn is not ice: nothing offered.
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(10);
        state.runner.identity = Some(CardId("barry_baz_wong".to_string()));
        state.runner.grip = vec![CardId("telework_contract".to_string())];
        state.runner.resources.credits = Credits(5);
        let mut asset = corp_root("pad_campaign", ServerId::Remote(0));
        asset.rezzed = false;
        state.corp.installed = vec![asset];
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "pad_campaign") }).expect("rez the asset");
        assert!(state.pending_decision.is_none());
    }

    #[test]
    fn muslihat_offers_to_reveal_and_take_an_icebreaker_or_run_event_from_the_top_of_the_stack() {
        let registry = sg_registry();
        let corp_turn_end = |top: &str| {
            let mut state = base_state();
            state.phase = GamePhase::Action(Side::Corp);
            state.corp.resources.clicks = Clicks(0);
            state.runner.identity = Some(CardId("muslihat".to_string()));
            state.runner.stack = vec![CardId("diesel".to_string()), CardId("jailbreak".to_string()), CardId(top.to_string())];
            state
        };
        // A run event on top: offered, revealed, and it comes off the top.
        let state = corp_turn_end("jailbreak");
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn: its end-of-turn window first");
        let state = advance_until_choice(state, &registry);
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Runner, .. })), "look at the top card");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("take it");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("add to grip");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { revealed: true, .. })));
        assert_eq!(state.runner.grip, vec![CardId("jailbreak".to_string())]);
        assert_eq!(state.runner.stack, vec![CardId("diesel".to_string()), CardId("jailbreak".to_string())], "the top copy left, not the lower one");

        // Neither an icebreaker nor a run event: nothing to decide.
        let state = corp_turn_end("sure_gamble");
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let state = advance_until_choice(state, &registry);
        assert_eq!(state.phase, GamePhase::Action(Side::Runner), "the runner's turn began");
        assert!(state.pending_decision.is_none());
        assert!(state.runner.grip.is_empty());
    }

    #[test]
    fn transfer_of_wealth_runs_hq_and_on_success_tags_the_runner_and_doubles_what_the_corp_loses() {
        let registry = sg_registry();
        let mut state = runner_turn(0, 4);
        state.runner.grip = vec![CardId("transfer_of_wealth".to_string())];
        state.corp.resources.credits = Credits(2);

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("transfer_of_wealth".to_string()) }).expect("play");
        let legal = crate::rules::legal_actions(&state, &registry);
        assert!(!legal.contains(&PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD }), "HQ only");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).expect("run hq");
        assert_eq!(state.active_run.as_ref().unwrap().initiated_by, Some(CardId("transfer_of_wealth".to_string())));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success");
        assert_eq!(state.runner.tags, 1);
        assert_eq!(state.corp.resources.credits, Credits(0), "loses 3 of its 2");
        assert_eq!(state.runner.resources.credits, Credits(4), "2 for each credit actually lost");
    }

    #[test]
    fn maglectric_rapid_trashes_itself_on_a_successful_hq_run_to_derez_a_rezzed_corp_card() {
        let registry = sg_registry();
        let mut state = runner_turn(0, 4);
        state.runner.rig = vec![rig_card_with_counters("maglectric_rapid", 0)];
        state.corp.installed = vec![corp_root("pad_campaign", ServerId::Remote(0)), corp_ice("ice_wall", ServerId::Remote(0))];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success on hq");
        let paid = state.pending_paid_choice.as_ref().expect("the offer");
        assert_eq!((paid.side, &paid.cost), (Side::Runner, &crate::dsl::Cost::TrashSelf));
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("trash it");
        assert_eq!(state.runner.heap, vec![CardId("maglectric_rapid".to_string())]);
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("the ice");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("derez it");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardDerezzed { card } if card.0 == "ice_wall")));
        assert!(!state.corp.installed[1].rezzed);
        assert!(state.corp.installed[0].rezzed, "the other one untouched");

        // Nothing rezzed: no offer, and the hardware stays.
        let mut state = runner_turn(0, 4);
        state.runner.rig = vec![rig_card_with_counters("maglectric_rapid", 0)];
        let mut asset = corp_root("pad_campaign", ServerId::Remote(0));
        asset.rezzed = false;
        state.corp.installed = vec![asset];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("success on hq");
        assert!(state.pending_paid_choice.is_none());
    }

    #[test]
    fn sang_kancil_boosts_two_credits_cheaper_while_a_run_event_is_active() {
        let registry = sg_registry();
        let encounter = |from_event: bool| {
            let mut state = runner_turn(10, 4);
            state.runner.rig = vec![rig_card_with_counters("sang_kancil", 0)];
            state.runner.grip = vec![CardId("jailbreak".to_string())];
            state.corp.installed = vec![corp_ice("whitespace", ServerId::Hq)];
            let state = if from_event {
                let (state, _) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: CardId("jailbreak".to_string()) }).expect("play");
                let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).expect("run hq");
                state
            } else {
                let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
                state
            };
            let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach whitespace");
            let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
            let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes: encounter");
            assert_eq!(state.active_run.as_ref().unwrap().phase, crate::rules::RunPhase::EncounterIce);
            state
        };
        let state = encounter(true);
        let before = state.runner.resources.credits;
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "sang_kancil"), ability_index: 1 }).expect("boost");
        assert_eq!(before.0 - state.runner.resources.credits.0, 1, "3 - 2 during a run event");
        assert_eq!(state.runner.rig[0].encounter_strength_buff, 2);

        let state = encounter(false);
        let before = state.runner.resources.credits;
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "sang_kancil"), ability_index: 1 }).expect("boost");
        assert_eq!(before.0 - state.runner.resources.credits.0, 3, "full price on a basic run");
    }

    #[test]
    fn fransofia_ward_taxes_every_ice_rez_and_bypasses_ice_when_the_corp_is_rich() {
        let registry = sg_registry();
        let run_into_ice_wall = |corp_credits: u32| {
            let mut state = runner_turn(0, 4);
            state.runner.rig = vec![rig_card_with_counters("fransofia_ward", 0)];
            state.corp.resources.credits = Credits(corp_credits);
            state.corp.installed = vec![ice_installed("ice_wall", ServerId::Hq, false)];
            let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
            let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
            let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "ice_wall") }).expect("rez");
            assert_eq!(state.corp.resources.credits, Credits(corp_credits - 2), "1 printed + 1 for the ward");
            advance_until_choice(state, &registry)
        };

        let state = run_into_ice_wall(20);
        let paid = state.pending_paid_choice.as_ref().expect("the bypass offer");
        assert_eq!((paid.side, &paid.cost), (Side::Runner, &crate::dsl::Cost::TrashSelf));
        let (state, events) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("bypass");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::IceBypassed { card_id, .. } if card_id.0 == "ice_wall")));
        assert_eq!(state.runner.heap, vec![CardId("fransofia_ward".to_string())]);
        assert!(state.active_run.as_ref().unwrap().ice_bypassed);
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes: the encounter ends");
        let run = state.active_run.as_ref().expect("the run survived the barrier");
        assert_eq!(run.phase, crate::rules::RunPhase::Success, "past the ice with its End the run never firing");
        assert!(!run.ice_bypassed, "cleared once the ice was passed");

        // Too poor a Corp: no offer, and the barrier ends the run.
        let state = run_into_ice_wall(16);
        assert!(state.pending_paid_choice.is_none(), "14 credits after the rez");
        assert!(state.active_run.is_none(), "End the run fired");
        assert!(state.runner.rig.iter().any(|c| c.card.0 == "fransofia_ward"));
    }

    // ---- Elevation Stage 5: Brick Stack -------------------------------

    #[test]
    fn bumi_1_0_may_trash_a_trojan_when_rezzed_mid_run_and_its_subroutines_trash_a_program_and_deal_core_damage() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("sure_gamble".to_string()), CardId("jailbreak".to_string())];
        state.runner.rig = vec![rig_card_with_counters("botulus", 0), rig_card_with_counters("cleaver", 0)];
        state.corp.installed = vec![ice_installed("bumi_1_0", ServerId::Hq, false)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "bumi_1_0") }).expect("rez during the run");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { .. })), "the trojan offer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "botulus") }).expect("pick the trojan");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert_eq!(state.runner.heap, vec![CardId("botulus".to_string())]);
        assert!(state.runner.rig.iter().all(|c| c.card.0 != "botulus"));

        // Both subroutines fire: the remaining program goes, then a core damage.
        let state = advance_until_choice(state, &registry);
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { .. })), "the first subroutine's program choice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "cleaver") }).expect("pick cleaver");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        let state = advance_until_choice(state, &registry);
        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.brain_damage, 1, "core damage from the second subroutine");
        assert_eq!(state.runner.grip.len(), 1, "one card discarded to the damage");

        // No trojan in the rig: nothing is offered and the encounter goes on.
        let mut state = runner_turn(5, 4);
        state.runner.rig = vec![rig_card_with_counters("cleaver", 0)];
        state.corp.installed = vec![ice_installed("bumi_1_0", ServerId::Hq, false)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "bumi_1_0") }).expect("rez");
        assert!(state.pending_decision.is_none());
        assert_eq!(state.runner.rig.len(), 1);
    }

    #[test]
    fn idiosyncresis_trashes_itself_at_turn_start_for_three_credits_and_two_runner_credits_per_advancement() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.r_and_d = vec![CardId("filler_card".to_string())];
        state.corp.installed = vec![installed_with_counters("idiosyncresis", ServerId::Remote(0), 0)];
        state.corp.installed[0].advancement_tokens = 2;
        state.runner.resources.credits = Credits(5);
        assert!(crate::rules::legal_actions(&state, &registry).contains(&PlayerAction::AdvanceCard { target: install_of(&state, "idiosyncresis") }), "you can advance this asset");

        // Same drive as Clearinghouse: two `EndTurn`s and the priority passes
        // that reach the Corp's next start of turn.
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes into their turn");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })));
        let corp_before = state.corp.resources.credits.0;
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("cash it in");
        assert_eq!(state.corp.resources.credits.0 - corp_before, 6, "3 per advancement counter");
        assert_eq!(state.runner.resources.credits, Credits(1), "5 - 2 per advancement counter");
        assert!(state.corp.installed.is_empty());
        assert!(state.corp.archives_contains(&CardId("idiosyncresis".to_string())));
    }

    #[test]
    fn off_the_books_keeps_dividends_in_the_score_area_and_spends_one_to_fetch_and_install_a_card_free() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(4);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())];
        state.corp.installed = vec![corp_root("off_the_books", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[0].advancement_tokens = 5;

        let (state, events) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "off_the_books") }).expect("score");
        let scored = &state.corp.scored_agendas[0];
        assert_eq!((scored.card.0.as_str(), scored.agenda_counters), ("off_the_books", 2), "Dividends 1: one counter per excess advancement");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CountersAdded { amount: 2, .. })));
        let (state, _) = close_all_windows(state, &registry);

        // The discard phase ends (after the end-of-turn window closes): the
        // scored agenda offers to spend a counter.
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn");
        let state = advance_until_choice(state, &registry);
        let paid = state.pending_paid_choice.as_ref().expect("the offer from the score area");
        assert_eq!((paid.side, &paid.cost), (Side::Corp, &crate::dsl::Cost::RemoveCounters(1)));
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("spend a counter");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 1);
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("search R&D");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("reveal it");
        assert!(state.corp.hq.contains(&CardId("ice_wall".to_string())), "added to HQ before the install offer");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("install it");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).expect("onto HQ");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "ice_wall" && c.server == ServerId::Hq));
        assert_eq!(state.corp.resources.credits, Credits(4), "ignoring all costs");
        assert!(!state.corp.hq.contains(&CardId("ice_wall".to_string())));
    }

    #[test]
    fn kessleroid_ends_the_run_twice_over() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.corp.installed = vec![ice_installed("kessleroid", ServerId::Hq, true)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let state = advance_until_choice(state, &registry);
        assert!(state.active_run.is_none(), "End the run fired");
        assert_eq!(registry.get(&CardId("kessleroid".to_string())).unwrap().subroutines.len(), 2);
    }

    #[test]
    fn syailendra_places_advancement_counters_when_encountered_with_three_and_from_its_subroutine() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("sure_gamble".to_string()), CardId("jailbreak".to_string())];
        state.corp.installed = vec![ice_installed("syailendra", ServerId::Hq, true), corp_root("off_the_books", ServerId::Remote(0))];
        state.corp.installed[0].advancement_tokens = 3;
        state.corp.installed[1].rezzed = false;

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let state = advance_until_choice(state, &registry);
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { .. })), "the encounter offer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "off_the_books") }).expect("pick the agenda");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("advance it");
        assert_eq!(state.corp.installed[1].advancement_tokens, 1);

        // Subroutines: the same offer again, then 2 credits, then a net damage.
        let state = advance_until_choice(state, &registry);
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { .. })), "the subroutine offer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "off_the_books") }).expect("pick the agenda again");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("advance it again");
        let state = advance_until_choice(state, &registry);
        assert_eq!(state.corp.installed[1].advancement_tokens, 2);
        assert_eq!(state.runner.resources.credits, Credits(3), "5 - 2");
        assert_eq!(state.runner.grip.len(), 1, "one net damage");

        // Two counters: the encounter offer is not made.
        let mut state = runner_turn(5, 4);
        state.corp.installed = vec![ice_installed("syailendra", ServerId::Hq, true)];
        state.corp.installed[0].advancement_tokens = 2;
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes: encounter");
        assert!(state.pending_decision.is_none());
        assert_eq!(state.active_run.as_ref().unwrap().phase, crate::rules::RunPhase::EncounterIce);
    }

    #[test]
    fn key_performance_indicators_resolves_two_of_four_options_in_the_order_chosen() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(3);
        state.corp.hq = vec![CardId("key_performance_indicators".to_string()), CardId("ice_wall".to_string())];

        let (state, events) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("key_performance_indicators".to_string()) }).expect("play");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::PendingChoicePresented { chooser: Side::Corp, option_count: 4 })));
        // Gain 2 credits first ...
        let (state, events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 3 }).expect("gain 2");
        assert_eq!(state.corp.resources.credits, Credits(4), "3 - 1 to play + 2");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::PendingChoicePresented { chooser: Side::Corp, option_count: 3 })), "the three that remain");
        // ... then install the ice for free.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("install ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick the ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD }).expect("protect R&D");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "ice_wall" && c.server == ServerId::RnD));
        assert_eq!(state.corp.resources.credits, Credits(4), "ignoring all costs");
        assert!(state.pending_decision.is_none(), "two resolved, nothing more offered");
        assert!(state.corp.archives_contains(&CardId("key_performance_indicators".to_string())));

        // A parking option first: the second pick is still owed once the
        // prompt resolves (the `Sequence` continuation).
        let mut state = base_state();
        state.corp.resources.credits = Credits(3);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        state.corp.hq = vec![CardId("key_performance_indicators".to_string()), CardId("ice_wall".to_string())];
        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("key_performance_indicators".to_string()) }).expect("play");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("draw, then shuffle one back");
        assert_eq!(state.corp.hq.len(), 2, "ice wall + the drawn hedge fund");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).expect("pick");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("shuffle it in");
        assert_eq!(state.corp.r_and_d, vec![CardId("hedge_fund".to_string())]);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::PendingChoicePresented { chooser: Side::Corp, option_count: 3 })), "the second pick, after the prompt");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 2 }).expect("gain 2 (index 2 of the remaining three)");
        assert_eq!(state.corp.resources.credits, Credits(4));
        assert!(state.pending_decision.is_none());
    }

    #[test]
    fn flyswatter_purges_virus_counters_when_rezzed_during_a_run_on_its_server() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.rig = vec![rig_card_with_counters("botulus", 3)];
        state.corp.installed = vec![ice_installed("flyswatter", ServerId::Hq, false)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, events) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "flyswatter") }).expect("rez");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::VirusCountersPurged { cards } if cards.len() == 1)));
        assert_eq!(state.runner.rig[0].counters, 0);
        // The engine only ever rezzes ice while it is approached, so the
        // "during a run against this server" condition cannot be false for
        // ice here; the requirement is authored for fidelity.
    }

    #[test]
    fn petty_cash_plays_only_as_the_first_action_and_refunds_its_click_when_played_from_archives() {
        let registry = sg_registry();
        let petty_cash = CardId("petty_cash".to_string());
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.hq = vec![petty_cash.clone()];
        state.corp.archives = vec![ArchivedCard::facedown(petty_cash.clone())];
        state.corp.playable_from_archives = vec![petty_cash.clone()];

        let play = PlayerAction::PlayOperation { card_id: petty_cash.clone() };
        assert!(crate::rules::legal_actions(&state, &registry).contains(&play));
        let (state, _) = apply_action(&state, &registry, play.clone()).expect("from HQ, as the first action");
        assert_eq!((state.corp.resources.credits, state.corp.resources.clicks), (Credits(12), Clicks(2)), "-3 +5, no click back from HQ");
        assert_eq!(state.actions_taken_this_turn, 1);
        assert!(state.corp.archives_contains(&petty_cash), "the played copy joins the one already there");
        assert_eq!(state.corp.archives.len(), 2);
        // A second one, from Archives, is no longer the first action.
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&play));
        assert!(matches!(apply_action(&state, &registry, play.clone()), Err(crate::rules::RulesError::RequirementNotMet)));

        // Fresh turn, nothing in HQ: the Archives copy is playable, refunds
        // the click and leaves the game.
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.archives = vec![ArchivedCard::facedown(petty_cash.clone())];
        state.corp.playable_from_archives = vec![petty_cash.clone()];
        let index = crate::rules::ActionSpace::index_of(&state, &play).expect("encodable");
        assert_eq!(crate::rules::ActionSpace::action_at(&state, index), Some(play.clone()), "round trip through the archives slot");
        assert!(crate::rules::get_action_mask(&state, &registry)[index]);
        let (state, events) = apply_action(&state, &registry, play).expect("from Archives");
        assert_eq!((state.corp.resources.credits, state.corp.resources.clicks), (Credits(12), Clicks(3)), "-3 +5, the click refunded");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::OperationPlayed { from_archives: true, .. })));
        assert!(state.corp.archives.is_empty());
        assert_eq!(state.corp.removed_from_game, vec![petty_cash]);
    }

    #[test]
    fn leo_construction_trashes_a_rezzed_bioroid_on_the_attacked_server_once_per_turn_to_end_the_run() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.corp.identity = Some(CardId("leo_construction_labor_solutions".to_string()));
        state.corp.installed = vec![ice_installed("bran_1_0", ServerId::Hq, true), ice_installed("ansel_1_0", ServerId::RnD, true)];
        let identity_ability = PlayerAction::ActivateAbility { target: InstallId::CORP_IDENTITY, ability_index: 0 };

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run HQ");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach Brân");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        assert!(crate::rules::legal_actions(&state, &registry).contains(&identity_ability), "a rezzed bioroid protects the attacked server");
        let (state, _) = apply_action(&state, &registry, identity_ability.clone()).expect("use the identity");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, .. })));
        // Ansel protects R&D, not the attacked server: only Brân is offered.
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ansel_1_0") }).is_err(), "not offered");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "bran_1_0") }).expect("pick Brân");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert!(state.active_run.is_none(), "the run ended");
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "bran_1_0" && !a.facedown), "a rezzed card lands faceup");
        assert_eq!(state.corp.installed.len(), 1);

        // Once per turn: the second run this turn gets no offer.
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::RnD }).expect("run R&D");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach Ansel");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&identity_ability), "already used this turn");

        // Outside a run nothing is "the attacked server", so the ability is
        // never offered on the Corp's own turn.
        let mut state = base_state();
        state.corp.identity = Some(CardId("leo_construction_labor_solutions".to_string()));
        state.corp.installed = vec![ice_installed("bran_1_0", ServerId::Hq, true)];
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&identity_ability));
    }

    #[test]
    fn project_ingatan_spends_a_dividend_to_install_a_card_from_archives_ignoring_all_costs() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(0);
        state.corp.archives = vec![ArchivedCard::facedown(CardId("ice_wall".to_string())), ArchivedCard::faceup(CardId("hedge_fund".to_string()))];
        state.corp.installed = vec![corp_root("project_ingatan", ServerId::Remote(0)), ice_installed("enigma", ServerId::Hq, false)];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[0].advancement_tokens = 4;

        let (state, _) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "project_ingatan") }).expect("score");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 1, "Dividends 1: one excess advancement");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn");
        let state = advance_until_choice(state, &registry);
        assert!(state.pending_paid_choice.is_some(), "the discard-phase offer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("spend the counter");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 0);
        // The operation in Archives is not installable and is not offered.
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).is_err(), "not offered");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick the ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).expect("onto HQ, behind Enigma");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "ice_wall" && c.server == ServerId::Hq));
        assert_eq!(state.corp.resources.credits, Credits(0), "the install tax is waived");
        assert_eq!(state.corp.archives.len(), 1, "only Hedge Fund remains");
    }

    #[test]
    fn humanoid_resources_spends_three_clicks_and_itself_to_gain_draw_install_twice_and_play_an_operation() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(1);
        state.corp.hq = vec![CardId("ice_wall".to_string()), CardId("hedge_fund".to_string())];
        state.corp.r_and_d = vec![CardId("enigma".to_string()), CardId("pad_campaign".to_string()), CardId("filler_card".to_string())];
        state.corp.installed = vec![corp_root("humanoid_resources", ServerId::Remote(0))];
        let ability = PlayerAction::ActivateAbility { target: install_of(&state, "humanoid_resources"), ability_index: 0 };
        // Two clicks cannot pay a three-click cost.
        let mut short = state.clone();
        short.corp.resources.clicks = Clicks(2);
        assert!(!crate::rules::legal_actions(&short, &registry).contains(&ability));

        let (state, _) = apply_action(&state, &registry, ability).expect("activate");
        assert_eq!(state.corp.resources.clicks, Clicks(0));
        assert!(state.corp.installed.is_empty(), "trashed as part of the cost");
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "humanoid_resources"));
        assert_eq!(state.corp.resources.credits, Credits(5), "1 + 4");
        assert_eq!(state.corp.hq.len(), 5, "two in hand plus three drawn");

        // First install offer.
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { .. })));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick the ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD }).expect("protect R&D");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "ice_wall" && c.server == ServerId::RnD));
        // Second install offer, from the continuation of an ability whose
        // source has already left play — declined.
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { .. })), "the second install offer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("install nothing");
        // Then the operation: Hedge Fund is affordable at 5 and is played.
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { .. })), "the operation offer");
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "enigma") }).is_err(), "ice is not an operation");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).expect("pick Hedge Fund");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("play it");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::OperationPlayed { card, .. } if card.0 == "hedge_fund")));
        assert_eq!(state.corp.resources.credits, Credits(9), "5 - 5 + 9, and no click spent");
        assert_eq!(state.corp.resources.clicks, Clicks(0));
        assert!(state.pending_decision.is_none());
    }

    #[test]
    fn otto_campaign_loads_six_pays_two_a_turn_and_refunds_two_clicks_when_it_empties() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.installed = vec![corp_root("otto_campaign", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "otto_campaign") }).expect("rez");
        assert_eq!(state.corp.installed[0].counters, 6);

        let mut state = base_state();
        state.corp.r_and_d = vec![CardId("filler_card".to_string())];
        state.corp.installed = vec![installed_with_counters("otto_campaign", ServerId::Remote(0), 2)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends turn");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends turn");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes into their turn");
        assert_eq!(state.corp.resources.credits, Credits(12), "the last two credits");
        assert!(state.corp.installed.is_empty(), "empty, so trashed");
        assert_eq!(state.corp.resources.clicks, Clicks(5), "three for the turn plus two");
    }

    #[test]
    fn scatter_field_is_strength_four_alone_and_zero_with_company_and_may_install_from_hq() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.corp.hq = vec![CardId("pad_campaign".to_string())];
        state.corp.installed = vec![ice_installed("scatter_field", ServerId::Hq, true)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        assert_eq!(state.active_run.as_ref().unwrap().ice[0].current_strength, 4, "alone on HQ");
        let state = advance_until_choice(state, &registry);
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, .. })), "the first subroutine's offer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "pad_campaign") }).expect("pick");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Remote(0) }).expect("a new remote");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "pad_campaign" && c.server == ServerId::Remote(0)));
        let state = advance_until_choice(state, &registry);
        assert!(state.active_run.is_none(), "the second subroutine ended the run");

        let mut state = runner_turn(5, 4);
        state.corp.installed = vec![ice_installed("scatter_field", ServerId::Hq, true), ice_installed("ice_wall", ServerId::Hq, true)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        assert_eq!(state.active_run.as_ref().unwrap().ice[0].current_strength, 0, "not the only ice");
    }

    #[test]
    fn nanomanagement_gains_two_clicks() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.hq = vec![CardId("nanomanagement".to_string())];
        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("nanomanagement".to_string()) }).expect("play");
        assert_eq!(state.corp.resources.clicks, Clicks(4), "3 - 1 + 2");
        assert_eq!(state.corp.resources.credits, Credits(6));
    }

    #[test]
    fn mercia_b4ll4rd_installs_ice_for_one_less_when_the_action_phase_ends_and_moves_to_that_server() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(1);
        state.corp.hq = vec![CardId("ice_wall".to_string()), CardId("pad_campaign".to_string())];
        state.corp.installed = vec![corp_root("mercia_b4ll4rd", ServerId::Remote(0)), ice_installed("enigma", ServerId::Hq, false)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end the action phase");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, .. })), "Mercia's offer");
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "pad_campaign") }).is_err(), "ice only");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick the ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, events) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).expect("behind Enigma");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "ice_wall" && c.server == ServerId::Hq));
        assert_eq!(state.corp.resources.credits, Credits(1), "the 1[c] tax, paid 1[c] less");
        let mercia = state.corp.installed.iter().find(|c| c.card.0 == "mercia_b4ll4rd").expect("still installed");
        assert_eq!((mercia.server, mercia.rezzed), (ServerId::Hq, true), "moved to HQ's root, still rezzed");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardMoved { from: ServerId::Remote(0), to: ServerId::Hq, .. })));

        // Declined: nothing is installed and Mercia stays put.
        let mut state = base_state();
        state.corp.hq = vec![CardId("ice_wall".to_string())];
        state.corp.installed = vec![corp_root("mercia_b4ll4rd", ServerId::Remote(0))];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end the action phase");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("decline");
        assert_eq!(state.corp.installed[0].server, ServerId::Remote(0));
        assert_eq!(state.corp.hq.len(), 1);
    }

    #[test]
    fn semak_samun_is_broken_only_by_a_fracter_and_ends_the_run_unless_the_runner_takes_three_net() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 4];
        state.runner.rig = vec![rig_card_with_counters("mayfly", 0)];
        state.runner.rig[0].base_strength = 3;
        state.corp.installed = vec![ice_installed("semak_samun", ServerId::Hq, true)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("encounter");
        // Mayfly matches the strength but an AI is no fracter.
        let err = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "mayfly"), ability_index: 0 }).expect_err("cannot break");
        assert!(matches!(err, RulesError::NoBreakableSubroutine { .. }), "{err:?}");
        // The subroutine fires and the Runner chooses: end the run.
        let state = advance_until_choice(state, &registry);
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Runner, .. })));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("end the run");
        assert!(state.active_run.is_none());
        assert_eq!(state.runner.grip.len(), 4);

        // Suffering the damage instead keeps the run alive.
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 4];
        state.corp.installed = vec![ice_installed("semak_samun", ServerId::Hq, true)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let state = advance_until_choice(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("take the damage");
        assert_eq!(state.runner.grip.len(), 1, "three net damage");
        assert!(state.active_run.is_some(), "the run goes on");

        // A fracter breaks it.
        let mut state = runner_turn(5, 4);
        state.runner.rig = vec![rig_card_with_counters("cleaver", 0)];
        state.runner.rig[0].base_strength = 3;
        state.corp.installed = vec![ice_installed("semak_samun", ServerId::Hq, true)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("encounter");
        let (_, events) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "cleaver"), ability_index: 0 }).expect("break");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { .. })));
    }

    #[test]
    fn mahkota_langit_grid_is_one_per_server_pays_for_rezzes_in_its_server_and_taxes_its_assets_trash_cost_persistently() {
        let registry = sg_registry();
        // Limit 1 region per server.
        let mut state = base_state();
        state.corp.hq = vec![CardId("mahkota_langit_grid".to_string())];
        state.corp.installed = vec![corp_root("mahkota_langit_grid", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        let second_here = PlayerAction::InstallCard { card_id: CardId("mahkota_langit_grid".to_string()), zone: ServerId::Remote(0), slot: InstallSlot::Root };
        let err = apply_action(&state, &registry, second_here.clone()).expect_err("one region per server");
        assert!(matches!(err, RulesError::RegionLimitExceeded { server: ServerId::Remote(0) }), "{err:?}");
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&second_here));
        apply_action(&state, &registry, PlayerAction::InstallCard { card_id: CardId("mahkota_langit_grid".to_string()), zone: ServerId::Remote(1), slot: InstallSlot::Root }).expect("another server is fine");

        // Rez: the load, then the hosted credits pay for an asset in the root
        // and for ice protecting the server, but not for an upgrade.
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.installed = vec![corp_root("mahkota_langit_grid", ServerId::Remote(0)), corp_root("pad_campaign", ServerId::Remote(0)), corp_root("manegarm_skunkworks", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[1].rezzed = false;
        state.corp.installed[2].rezzed = false;
        let err = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "mahkota_langit_grid") }).expect_err("2[c] with nothing");
        assert!(matches!(err, RulesError::NotEnoughCredits { .. }));
        state.corp.resources.credits = Credits(2);
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "mahkota_langit_grid") }).expect("rez the grid");
        assert_eq!((state.corp.resources.credits, state.corp.installed[0].counters), (Credits(0), 2), "loaded on rez");
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "pad_campaign") }).expect("the grid pays");
        assert_eq!((state.corp.resources.credits, state.corp.installed[0].counters), (Credits(0), 0));
        let err = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "manegarm_skunkworks") }).expect_err("an upgrade is not covered");
        assert!(matches!(err, RulesError::NotEnoughCredits { .. }));

        let mut state = runner_turn(5, 4);
        state.corp.resources.credits = Credits(0);
        state.corp.installed = vec![installed_with_counters("mahkota_langit_grid", ServerId::Remote(0), 2), ice_installed("ice_wall", ServerId::Remote(0), false)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "ice_wall") }).expect("the grid pays for its ice");
        assert_eq!(state.corp.installed[0].counters, 1);

        // Trash cost: +2 on assets in the root, and still +2 after the grid
        // itself was trashed earlier in the run.
        let mut state = runner_turn(4, 4);
        state.corp.installed = vec![installed_with_counters("mahkota_langit_grid", ServerId::Remote(0), 2), corp_root("pad_campaign", ServerId::Remote(0))];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
        let state = advance_until_choice(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::SelectCardToAccess { card_id: CardId("mahkota_langit_grid".to_string()) }).expect("the grid first");
        let (state, _) = pass_until_settled(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::TrashAccessedCard { card_id: CardId("mahkota_langit_grid".to_string()) }).expect("trash it for 2");
        assert_eq!(state.runner.resources.credits, Credits(2));
        // The last card in the root is accessed next, with or without a
        // selection step.
        let (state, _) = pass_until_settled(state, &registry);
        let select = PlayerAction::SelectCardToAccess { card_id: CardId("pad_campaign".to_string()) };
        let state = if crate::rules::legal_actions(&state, &registry).contains(&select) {
            pass_until_settled(apply_action(&state, &registry, select).expect("then the asset").0, &registry).0
        } else {
            state
        };
        let run = state.active_run.as_ref().expect("mid-access");
        let phase = &run.access_state.as_ref().expect("access").phase;
        assert!(matches!(phase, crate::rules::AccessPhase::PendingChoice { trash_cost: Some(6), .. }), "PAD Campaign's 4 + 2, persistent: {phase:?}");
        let err = apply_action(&state, &registry, PlayerAction::TrashAccessedCard { card_id: CardId("pad_campaign".to_string()) }).expect_err("2 credits cannot pay 6");
        assert!(matches!(err, RulesError::CannotAffordTrashCost { requested: 6, .. }), "{err:?}");
    }


    // ---- Elevation Stage 7: Fashion Lab, Pork Chops ----

    #[test]
    fn poetri_luxury_brands_installs_from_the_top_three_of_rd_on_a_score_and_from_hq_on_a_steal() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.identity = Some(CardId("poetri_luxury_brands_all_the_rage".to_string()));
        // The top of R&D is the *end* of the vec: Pad Campaign is the
        // fourth card down and out of the ability's reach.
        state.corp.r_and_d = vec![
            CardId("pad_campaign".to_string()),
            CardId("hedge_fund".to_string()),
            CardId("enigma".to_string()),
            CardId("ice_wall".to_string()),
        ];
        state.corp.installed = vec![corp_root("offworld_office", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[0].advancement_tokens = 4;

        let (state, _) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "offworld_office") }).expect("score");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, .. })));
        // Deeper than the top 3, and an operation among them: neither is offered.
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "pad_campaign") }).is_err(), "below the top 3");
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).is_err(), "an operation is not installable");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "enigma") }).expect("pick the ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).expect("protect HQ");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "enigma" && c.server == ServerId::Hq && !c.rezzed));
        assert_eq!(state.corp.r_and_d.len(), 3, "the card left R&D");

        // The stolen half: the Runner takes an agenda and the Corp installs from HQ.
        let mut state = runner_turn(5, 4);
        state.corp.identity = Some(CardId("poetri_luxury_brands_all_the_rage".to_string()));
        state.corp.hq = vec![CardId("ice_wall".to_string())];
        state.corp.installed = vec![corp_root("offworld_office", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
        let state = advance_until_choice(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::StealAgenda { card_id: CardId("offworld_office".to_string()) }).expect("steal");
        let state = advance_until_choice(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick from HQ");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD }).expect("protect R&D");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "ice_wall" && c.server == ServerId::RnD));
        assert!(state.corp.hq.is_empty());
    }

    #[test]
    fn aggressive_trendsetting_charges_the_runner_a_click_to_trash_an_install_or_banks_one_for_the_corp() {
        let registry = sg_registry();
        let trendsetting = || crate::rules::ScoredAgenda::plain(CardId("aggressive_trendsetting".to_string()));
        // Walks a run to the point where `card` is the card being accessed.
        // `advance_until_choice` cannot be used here: it *passes* an
        // accessed card, which is the one thing this test must not do.
        let trash = |state: &GameState, registry: &CardRegistry, server: ServerId, card: &str| {
            let want = PlayerAction::TrashAccessedCard { card_id: CardId(card.to_string()) };
            let (mut state, _) = apply_action(state, registry, PlayerAction::InitiateRun { server }).expect("run");
            for _ in 0..40 {
                let legal = crate::rules::legal_actions(&state, registry);
                if legal.contains(&want) {
                    break;
                }
                let Some(action) = legal.into_iter().find(|a| {
                    matches!(
                        a,
                        PlayerAction::PassPriority { .. }
                            | PlayerAction::ContinueRun
                            | PlayerAction::CompleteRun
                            | PlayerAction::SelectCardToAccess { .. }
                    )
                }) else {
                    break;
                };
                state = apply_action(&state, registry, action.clone()).unwrap_or_else(|e| panic!("{action:?}: {e:?}")).0;
            }
            apply_action(&state, registry, want).expect("trash it").0
        };

        let mut state = runner_turn(10, 4);
        state.corp.scored_agendas = vec![trendsetting()];
        // The Corp must survive its next mandatory draw for the banked
        // click to be observable.
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 3];
        state.corp.installed = vec![corp_root("pad_campaign", ServerId::Remote(0))];
        let state = trash(&state, &registry, ServerId::Remote(0), "pad_campaign");
        assert!(matches!(state.pending_paid_choice.as_ref().map(|p| p.side), Some(Side::Runner)), "the Runner is asked for a click");

        // Refusing hands the Corp a fourth click next turn.
        let (declined, _) = apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("refuse");
        assert_eq!(declined.corp.extra_clicks_next_turn, 1);
        let (declined, _) = pass_until_settled(declined, &registry);
        let (declined, _) = apply_action(&declined, &registry, PlayerAction::EndTurn).expect("end the Runner turn");
        let (declined, _) = pass_until_settled(declined, &registry);
        assert_eq!(declined.corp.resources.clicks, Clicks(4), "three plus the banked one");
        assert_eq!(declined.corp.extra_clicks_next_turn, 0, "spent, not kept");

        // Paying the click costs the Runner one and banks nothing.
        let (paid, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("pay");
        assert_eq!(paid.corp.extra_clicks_next_turn, 0);
        assert_eq!(paid.runner.resources.clicks, Clicks(2), "one for the run, one for this");

        // Only the first trash each turn, and only of an *installed* card.
        let mut state = runner_turn(10, 4);
        state.corp.scored_agendas = vec![trendsetting()];
        state.corp.installed = vec![corp_root("pad_campaign", ServerId::Remote(0)), corp_root("nico_campaign", ServerId::Remote(1))];
        let state = trash(&state, &registry, ServerId::Remote(0), "pad_campaign");
        let (state, _) = apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("refuse");
        let (state, _) = pass_until_settled(state, &registry);
        let state = trash(&state, &registry, ServerId::Remote(1), "nico_campaign");
        assert!(state.pending_paid_choice.is_none(), "once per Runner turn");

        // A card trashed out of HQ was never installed.
        let mut state = runner_turn(10, 4);
        state.corp.scored_agendas = vec![trendsetting()];
        state.corp.hq = vec![CardId("pad_campaign".to_string())];
        let state = trash(&state, &registry, ServerId::Hq, "pad_campaign");
        assert!(state.pending_paid_choice.is_none(), "not an installed card");
    }

    #[test]
    fn top_down_solutions_draws_two_and_installs_two_cards_from_hq() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.hq = vec![CardId("top_down_solutions".to_string())];
        state.corp.r_and_d = vec![CardId("pad_campaign".to_string()), CardId("ice_wall".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("top_down_solutions".to_string()) }).expect("play");
        assert_eq!(state.corp.hq.len(), 2, "drew both");
        assert_eq!(state.corp.resources.credits, Credits(8));
        // First install.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick the ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).expect("onto HQ");
        // Second install, offered one at a time.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "pad_campaign") }).expect("pick the asset");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Remote(0) }).expect("into a remote");
        assert_eq!(state.corp.installed.len(), 2);
        assert!(state.corp.hq.is_empty());
    }

    #[test]
    fn byte_tags_and_deals_three_net_when_the_corp_pays_four_and_nothing_when_it_declines() {
        let registry = sg_registry();
        let run_into_byte = |corp_credits: u32| {
            let mut state = runner_turn(5, 4);
            state.corp.resources.credits = Credits(corp_credits);
            state.runner.grip = vec![CardId("sure_gamble".to_string()); 4];
            state.corp.installed = vec![corp_root("byte", ServerId::Remote(0))];
            let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
            advance_until_choice(state, &registry)
        };

        let state = run_into_byte(4);
        let pay = PlayerAction::PayAccessTrigger { card_id: CardId("byte".to_string()) };
        assert!(crate::rules::legal_actions(&state, &registry).contains(&pay));
        let (paid, _) = apply_action(&state, &registry, pay).expect("pay 4");
        assert_eq!(paid.runner.tags, 1);
        assert_eq!(paid.runner.grip.len(), 1, "three net damage");
        assert_eq!(paid.corp.resources.credits, Credits(0));

        let (declined, _) = apply_action(&state, &registry, PlayerAction::DeclineAccessTrigger { card_id: CardId("byte".to_string()) }).expect("decline");
        assert_eq!(declined.runner.tags, 0);
        assert_eq!(declined.runner.grip.len(), 4);

        // A Corp that cannot pay is not offered the choice at all.
        let broke = run_into_byte(3);
        assert!(!crate::rules::legal_actions(&broke, &registry).iter().any(|a| matches!(a, PlayerAction::PayAccessTrigger { .. })));
    }

    #[test]
    fn mycoweb_installs_from_archives_rezzes_two_cheaper_and_resolves_another_ices_subroutine() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.corp.resources.credits = Credits(1);
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 3];
        state.corp.archives = vec![ArchivedCard::facedown(CardId("ice_wall".to_string())), ArchivedCard::faceup(CardId("hedge_fund".to_string()))];
        state.corp.installed = vec![
            ice_installed("mycoweb", ServerId::Hq, true),
            ice_installed("enigma", ServerId::RnD, false),
            ice_installed("bumi_1_0", ServerId::Archives, true),
        ];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");

        // Subroutine 1: install a piece of ice from Archives for free.
        let state = advance_until_choice(state, &registry);
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).is_err(), "not ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick the ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Archives }).expect("install it");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "ice_wall" && c.server == ServerId::Archives));
        assert_eq!(state.corp.resources.credits, Credits(1), "ignoring all costs");

        // Subroutine 2: rez Enigma (3[c]) for 1.
        let state = advance_until_choice(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "enigma") }).expect("pick Enigma");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("rez it");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "enigma" && c.rezzed));
        assert_eq!(state.corp.resources.credits, Credits(0), "3 less 2");

        // Subroutine 3: resolve a subroutine on a rezzed sentry.
        let state = advance_until_choice(state, &registry);
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "enigma") }).is_err(), "a code gate is no sentry");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "bumi_1_0") }).expect("pick Bumi");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })), "Bumi's two subroutines");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("core damage");
        assert_eq!(state.runner.grip.len(), 2, "Bumi's subroutine resolved");

        // Subroutine 4: another rezzed code gate — Enigma, which subroutine
        // 2 rezzed a moment ago. Mycoweb itself is not "another".
        let state = advance_until_choice(state, &registry);
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "mycoweb") }).is_err(), "not itself");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "enigma") }).expect("pick Enigma");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("end the run");
        assert!(state.active_run.is_none(), "Enigma's subroutine, resolved through Mycoweb");
    }

    #[test]
    fn touch_ups_costs_an_extra_click_advances_twice_and_shuffles_two_cards_of_one_type_away() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.hq = vec![CardId("touch_ups".to_string())];
        state.corp.installed = vec![corp_root("offworld_office", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        state.runner.grip = vec![
            CardId("sure_gamble".to_string()),
            CardId("cleaver".to_string()),
            CardId("jailbreak".to_string()),
        ];
        state.runner.stack = vec![CardId("sure_gamble".to_string())];

        // One click short is one click short: the Double cannot be played.
        let mut short = state.clone();
        short.corp.resources.clicks = Clicks(1);
        assert!(!crate::rules::legal_actions(&short, &registry).contains(&PlayerAction::PlayOperation { card_id: CardId("touch_ups".to_string()) }));

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("touch_ups".to_string()) }).expect("play");
        assert_eq!(state.corp.resources.clicks, Clicks(1), "the click to play plus the Double's own");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "offworld_office") }).expect("pick the agenda");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("advance it");
        assert_eq!(state.corp.installed[0].advancement_tokens, 2);
        // Naming a card type reveals the grip; two of that type go back to the stack.
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 3 }).expect("name Event");
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "cleaver") }).is_err(), "a program is not an event");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "sure_gamble") }).expect("one");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "jailbreak") }).expect("two");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        assert_eq!(state.runner.grip, vec![CardId("cleaver".to_string())]);
        assert_eq!(state.runner.stack.len(), 3);
    }

    #[test]
    fn bangun_flips_agendas_faceup_and_hurts_the_runner_for_looking() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.installed = vec![corp_root("offworld_office", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        let rez = PlayerAction::RezIce { ice: install_of(&state, "offworld_office") };

        // No other Corp may turn an agenda faceup.
        let err = apply_action(&state, &registry, rez.clone()).expect_err("agendas are installed facedown");
        assert!(matches!(err, RulesError::CardTypeMismatch { .. }), "{err:?}");
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&rez));

        state.corp.identity = Some(CardId("bangun_when_disaster_strikes".to_string()));
        let (state, _) = apply_action(&state, &registry, rez).expect("BANGUN may");
        assert!(state.corp.installed[0].rezzed);
        assert_eq!(state.corp.resources.credits, Credits(10), "an agenda costs nothing to flip");

        // Accessing it costs the Runner 2 meat and a tag — and it is still stealable.
        let mut state = runner_turn(5, 4);
        state.corp.identity = Some(CardId("bangun_when_disaster_strikes".to_string()));
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 4];
        state.corp.installed = vec![corp_root("offworld_office", ServerId::Remote(0))];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
        let state = advance_until_choice(state, &registry);
        assert_eq!(state.runner.grip.len(), 2, "two meat damage");
        assert_eq!(state.runner.tags, 1);
        let (state, _) = pass_until_settled(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::StealAgenda { card_id: CardId("offworld_office".to_string()) }).expect("steal it anyway");
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(2));

        // A facedown agenda is just an agenda.
        let mut state = runner_turn(5, 4);
        state.corp.identity = Some(CardId("bangun_when_disaster_strikes".to_string()));
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 4];
        state.corp.installed = vec![corp_root("offworld_office", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
        let state = advance_until_choice(state, &registry);
        assert_eq!((state.runner.grip.len(), state.runner.tags), (4, 0));
    }

    #[test]
    fn anthill_excavation_contract_pays_four_and_draws_for_two_turns_then_trashes_itself() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(3);
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 6];
        state.corp.installed = vec![corp_root("anthill_excavation_contract", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;

        let (state, _) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "anthill_excavation_contract") }).expect("rez");
        assert_eq!((state.corp.resources.credits, state.corp.installed[0].counters), (Credits(0), 8));

        let next_corp_turn = |state: &GameState, registry: &CardRegistry| {
            let (state, _) = pass_until_settled(state.clone(), registry);
            let (state, _) = apply_action(&state, registry, PlayerAction::EndTurn).expect("corp ends");
            let (state, _) = pass_until_settled(state, registry);
            let (state, _) = apply_action(&state, registry, PlayerAction::EndTurn).expect("runner ends");
            pass_until_settled(state, registry).0
        };

        let state = next_corp_turn(&state, &registry);
        assert_eq!(state.corp.resources.credits, Credits(4));
        assert_eq!(state.corp.installed[0].counters, 4);
        assert_eq!(state.corp.hq.len(), 2, "the mandatory draw plus the card this drew");

        let state = next_corp_turn(&state, &registry);
        assert_eq!(state.corp.resources.credits, Credits(8));
        assert!(state.corp.installed.is_empty(), "empty, so trashed");
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "anthill_excavation_contract"));
    }

    #[test]
    fn biawak_may_forfeit_the_cheapest_agenda_to_rez_ten_credits_cheaper() {
        let registry = sg_registry();
        let approach_biawak = |corp_credits: u32, agendas: Vec<&str>| {
            let mut state = runner_turn(5, 4);
            state.corp.resources.credits = Credits(corp_credits);
            state.corp.scored_agendas =
                agendas.iter().map(|id| crate::rules::ScoredAgenda::plain(CardId((*id).to_string()))).collect();
            state.corp.installed = vec![ice_installed("biawak", ServerId::Hq, false)];
            let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
            apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach").0
        };
        let rez = |state: &GameState| PlayerAction::RezIce { ice: install_of(state, "biawak") };

        // Rich enough to pay outright, so both ways are on the table.
        let state = approach_biawak(14, vec!["offworld_office", "aggressive_trendsetting"]);
        let (state, _) = apply_action(&state, &registry, rez(&state)).expect("rez, one way or the other");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })), "forfeit or not");

        // Forfeiting takes the 1-pointer, not the 2-pointer, and pays the rest.
        let (forfeited, events) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("forfeit");
        assert_eq!(forfeited.corp.resources.credits, Credits(10), "14 less the 4 left after the agenda paid 10");
        assert!(forfeited.corp.installed[0].rezzed);
        assert_eq!(forfeited.corp.scored_agendas.len(), 1);
        assert_eq!(forfeited.corp.scored_agendas[0].card.0, "offworld_office", "the cheaper agenda went");
        assert!(forfeited.corp.removed_from_game.contains(&CardId("aggressive_trendsetting".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::AgendaForfeited { .. })));

        // Or pay the whole 14 and keep both agendas.
        let (paid, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("pay in full");
        assert_eq!(paid.corp.resources.credits, Credits(0));
        assert!(paid.corp.installed[0].rezzed);
        assert_eq!(paid.corp.scored_agendas.len(), 2);

        // With only 4 credits the forfeit is the one way to rez, so the Corp
        // is not asked a question with a single answer.
        let state = approach_biawak(4, vec!["offworld_office"]);
        let (state, _) = apply_action(&state, &registry, rez(&state)).expect("the only way");
        assert!(state.pending_decision.is_none());
        assert!(state.corp.installed[0].rezzed);
        assert!(state.corp.scored_agendas.is_empty());

        // With nothing to forfeit and 4 credits, 14 is 14: the rez is
        // refused rather than quietly doing nothing, which is what kept a
        // random Corp from re-rezzing it forever.
        let broke = approach_biawak(4, Vec::new());
        let err = apply_action(&broke, &registry, rez(&broke)).expect_err("cannot pay");
        assert!(matches!(err, RulesError::NotEnoughCredits { requested: 14, .. }), "{err:?}");
        assert!(!crate::rules::legal_actions(&broke, &registry).contains(&rez(&broke)));

        // With nothing installed to trash, the first subroutine ends the run.
        let (state, _) = pass_until_settled(state, &registry);
        assert!(state.active_run.is_none(), "subroutine 1 ended the run");
    }

    #[test]
    fn measured_response_needs_threat_four_and_a_recent_run_then_taxes_eight_or_deals_four_meat() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.hq = vec![CardId("measured_response".to_string())];
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
        state.runner.made_successful_run_last_turn = true;
        let play = PlayerAction::PlayOperation { card_id: CardId("measured_response".to_string()) };

        // Threat 2: one 2-point agenda in the Runner's score area is not enough.
        let mut low = state.clone();
        low.runner.scored_agendas = vec![CardId("offworld_office".to_string())];
        assert!(!crate::rules::legal_actions(&low, &registry).contains(&play));

        // Threat 4, but no run last turn.
        state.runner.scored_agendas = vec![CardId("offworld_office".to_string()); 2];
        let mut quiet = state.clone();
        quiet.runner.made_successful_run_last_turn = false;
        assert!(!crate::rules::legal_actions(&quiet, &registry).contains(&play));

        let (state, _) = apply_action(&state, &registry, play).expect("play");
        assert!(matches!(state.pending_paid_choice.as_ref().map(|p| p.side), Some(Side::Runner)));
        let (paid, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("pay 8");
        assert_eq!(paid.runner.resources.credits, Credits(2));
        assert_eq!(paid.runner.grip.len(), 5, "no damage");
        let (hit, _) = apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("refuse");
        assert_eq!(hit.runner.grip.len(), 1, "four meat damage");
    }


    // ---- Elevation Stage 8: Quick Returns, Glyph of Warding ----

    #[test]
    fn au_co_counts_damage_and_hq_trashes_then_spends_two_counters_to_dig_three_deep() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.identity = Some(CardId("au_co_the_gold_standard_in_clones".to_string()));
        state.corp.hq = vec![CardId("hansei_review".to_string()), CardId("ice_wall".to_string())];
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 6];

        // Trashing from HQ is one counter for the batch, however many go.
        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hansei_review".to_string()) }).expect("play");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick one");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("trash it");
        assert_eq!(state.corp.identity_counters, 1, "one per batch out of HQ");

        // Damage is another, wherever it comes from.
        let mut damaged = runner_turn(5, 4);
        damaged.corp.identity = Some(CardId("au_co_the_gold_standard_in_clones".to_string()));
        damaged.runner.grip = vec![CardId("sure_gamble".to_string()); 4];
        damaged.corp.installed = vec![ice_installed("semak_samun", ServerId::Hq, true)];
        let (damaged, _) = apply_action(&damaged, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (damaged, _) = apply_action(&damaged, &registry, PlayerAction::ContinueRun).expect("approach");
        let damaged = advance_until_choice(damaged, &registry);
        let (damaged, _) = apply_action(&damaged, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("take 3 net");
        assert_eq!(damaged.corp.identity_counters, 1, "one for the damage");

        // Two counters buy a look at the top 3 of R&D at the turn's start.
        let mut state = base_state();
        state.corp.identity = Some(CardId("au_co_the_gold_standard_in_clones".to_string()));
        state.corp.identity_counters = 2;
        state.corp.r_and_d = vec![
            CardId("tithe".to_string()),
            CardId("pad_campaign".to_string()),
            CardId("enigma".to_string()),
            CardId("ice_wall".to_string()),
        ];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end the corp turn");
        let (state, _) = pass_until_settled(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends");
        let state = advance_until_choice(state, &registry);
        assert!(state.pending_paid_choice.is_some(), "AU Co. offers the dig");
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("spend two");
        assert_eq!(state.corp.identity_counters, 0);
        // The mandatory draw took Ice Wall; the top 3 are now Enigma, Pad
        // Campaign and Tithe.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "pad_campaign") }).expect("trash one");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm the trash");
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "pad_campaign" && a.facedown), "trashed unseen");
        // …and the other two go to HQ, both of them.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "enigma") }).expect("one");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "tithe") }).expect("two");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("into HQ");
        assert!(state.corp.r_and_d.is_empty(), "the top 3 are all gone");
        assert!(state.corp.hq.iter().any(|c| c.0 == "enigma") && state.corp.hq.iter().any(|c| c.0 == "tithe"));
    }

    #[test]
    fn sericulture_expansion_spends_a_dividend_at_the_end_of_the_turn_to_place_two_advancements() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 3];
        state.corp.installed = vec![
            corp_root("sericulture_expansion", ServerId::Remote(0)),
            corp_root("offworld_office", ServerId::Remote(1)),
        ];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[0].advancement_tokens = 4;
        state.corp.installed[1].rezzed = false;

        let (state, _) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "sericulture_expansion") }).expect("score");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 1, "Dividends 1");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn");
        let state = advance_until_choice(state, &registry);
        assert!(state.pending_paid_choice.is_some(), "the discard-phase offer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("spend the counter");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "offworld_office") }).expect("pick it");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("advance");
        let advanced = state.corp.installed.iter().find(|c| c.card.0 == "offworld_office").expect("still there");
        assert_eq!(advanced.advancement_tokens, 2);
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 0);
    }

    #[test]
    fn phat_gioan_baotixita_loads_at_end_of_turn_and_spends_counters_for_net_damage_once_a_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 3];
        state.corp.installed = vec![
            installed_with_counters("phat_gioan_baotixita", ServerId::Remote(0), 0),
            corp_root("offworld_office", ServerId::Remote(1)),
        ];
        state.corp.installed[1].rezzed = false;
        state.corp.installed[1].advancement_tokens = 4;
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 6];

        // Scoring with no counters is 1 net damage and nothing to spend.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "offworld_office") }).expect("score");
        let state = advance_until_choice(state, &registry);
        assert!(state.pending_paid_choice.is_some(), "asked whether to spend a counter");
        let (state, _) = apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("nothing to spend");
        assert_eq!(state.runner.grip.len(), 5, "1 net damage");

        // The discard phase loads one.
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn");
        let (state, _) = pass_until_settled(state, &registry);
        assert_eq!(state.corp.installed[0].counters, 1, "a counter when the discard phase ends");
    }

    #[test]
    fn empiricist_draws_and_buries_a_card_then_damages_and_tags() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 6];
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.corp.r_and_d = vec![CardId("ice_wall".to_string())];
        state.corp.installed = vec![ice_installed("empiricist", ServerId::Hq, true)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let state = advance_until_choice(state, &registry);
        assert_eq!(state.corp.hq.len(), 2, "drew Ice Wall");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).expect("pick one");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("onto R&D");
        assert_eq!(state.corp.r_and_d.last(), Some(&CardId("hedge_fund".to_string())), "on top of R&D");
        let (state, _) = pass_until_settled(state, &registry);
        assert_eq!(state.runner.tags, 1, "subroutine 2");
        assert_eq!(state.runner.grip.len(), 3, "1 net then 2 net");
    }

    #[test]
    fn peer_review_gains_seven_and_installs_into_a_remote_only() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.hq = vec![CardId("peer_review".to_string()), CardId("malapert_data_vault".to_string())];

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("peer_review".to_string()) }).expect("play");
        assert_eq!(state.corp.resources.credits, Credits(13), "10 - 4 + 7");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "malapert_data_vault") }).expect("pick the upgrade");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        // An upgrade could normally root on a central; "in the root of a
        // remote server" is what keeps R&D off the offer.
        let onto_rd = PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD };
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&onto_rd));
        assert!(apply_action(&state, &registry, onto_rd).is_err());
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Remote(0) }).expect("into a remote");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "malapert_data_vault" && c.server == ServerId::Remote(0)));
    }

    #[test]
    fn the_zwicky_group_draws_once_a_turn_when_an_agenda_or_operation_pays_out() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.identity = Some(CardId("the_zwicky_group_invisible_hands".to_string()));
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("hedge_fund".to_string())];
        state.corp.r_and_d = vec![CardId("ice_wall".to_string()); 4];

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hedge_fund".to_string()) }).expect("play");
        assert_eq!(state.corp.hq.len(), 2, "one Hedge Fund left, plus the card it drew");
        // Once per turn: the second operation pays out but draws nothing.
        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hedge_fund".to_string()) }).expect("play again");
        assert_eq!(state.corp.hq.len(), 1, "no second draw");

        // An asset's credits are not an agenda's or an operation's.
        let mut state = base_state();
        state.corp.identity = Some(CardId("the_zwicky_group_invisible_hands".to_string()));
        state.corp.r_and_d = vec![CardId("ice_wall".to_string()); 4];
        state.corp.installed = vec![installed_with_counters("regolith_mining_license", ServerId::Remote(0), 15)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: install_of(&state, "regolith_mining_license"), ability_index: 0 }).expect("mine");
        assert_eq!(state.corp.resources.credits, Credits(13));
        assert!(state.corp.hq.is_empty(), "an asset does not trigger it");
    }

    #[test]
    fn greenmail_pays_two_when_scored_and_four_more_when_forfeited() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.installed = vec![corp_root("greenmail", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[0].advancement_tokens = 2;

        let (state, _) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "greenmail") }).expect("score");
        assert_eq!(state.corp.resources.credits, Credits(2), "2 on score");

        // Forfeited through Biawak's rez: 4 more, and the agenda leaves the game.
        let mut state = runner_turn(5, 4);
        state.corp.resources.credits = Credits(4);
        state.corp.scored_agendas = vec![crate::rules::ScoredAgenda::plain(CardId("greenmail".to_string()))];
        state.corp.installed = vec![ice_installed("biawak", ServerId::Hq, false)];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let (state, events) = apply_action(&state, &registry, PlayerAction::RezIce { ice: install_of(&state, "biawak") }).expect("forfeit and rez");
        assert!(state.corp.installed[0].rezzed);
        assert!(state.corp.scored_agendas.is_empty());
        assert_eq!(state.corp.resources.credits, Credits(4), "4 paid for the rez, 4 back from Greenmail");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::AgendaForfeited { .. })));
    }

    #[test]
    fn plutus_is_rezzed_by_forfeiting_or_trashing_three_and_replays_a_transaction_from_archives() {
        let registry = sg_registry();
        let plutus = |state: &GameState| PlayerAction::RezIce { ice: install_of(state, "plutus") };
        let mut base = base_state();
        base.corp.installed = vec![corp_root("plutus", ServerId::Remote(0))];
        base.corp.installed[0].rezzed = false;

        // Neither cost is payable: the rez is refused outright, and never offered.
        let bare = base.clone();
        let err = apply_action(&bare, &registry, plutus(&bare)).expect_err("nothing to pay with");
        assert!(matches!(err, RulesError::NoAvailableRezAlternative { .. }), "{err:?}");
        assert!(!crate::rules::legal_actions(&bare, &registry).contains(&plutus(&bare)));

        // Three cards in HQ is one way, and the only one here.
        let mut hq = base.clone();
        hq.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string()), CardId("enigma".to_string())];
        let (hq, _) = apply_action(&hq, &registry, plutus(&hq)).expect("trash three");
        assert!(matches!(hq.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { min: 3, max: 3, .. })));
        let (hq, _) = apply_action(&hq, &registry, PlayerAction::ToggleCardSelection { position: 0 }).expect("one");
        let (hq, _) = apply_action(&hq, &registry, PlayerAction::ToggleCardSelection { position: 1 }).expect("two");
        let (hq, _) = apply_action(&hq, &registry, PlayerAction::ToggleCardSelection { position: 2 }).expect("three");
        let (hq, _) = apply_action(&hq, &registry, PlayerAction::ConfirmCardSelection).expect("pay");
        assert!(hq.corp.hq.is_empty());
        assert!(hq.corp.installed[0].rezzed, "the rez follows the payment");

        // With an agenda too, the Corp picks which cost to pay.
        let mut both = base.clone();
        both.corp.hq = vec![CardId("hedge_fund".to_string()); 3];
        both.corp.scored_agendas = vec![crate::rules::ScoredAgenda::plain(CardId("offworld_office".to_string()))];
        let (both, _) = apply_action(&both, &registry, plutus(&both)).expect("two ways to pay");
        assert!(matches!(both.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })));
        let (forfeited, _) = apply_action(&both, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("forfeit instead");
        assert!(forfeited.corp.scored_agendas.is_empty());
        assert_eq!(forfeited.corp.hq.len(), 3, "HQ untouched");
        assert!(forfeited.corp.installed[0].rezzed);

        // Its turn-start ability replays a transaction out of Archives and
        // removes it from the game.
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.r_and_d = vec![CardId("ice_wall".to_string()); 3];
        state.corp.installed = vec![corp_root("plutus", ServerId::Remote(0))];
        state.corp.archives = vec![
            ArchivedCard::faceup(CardId("hedge_fund".to_string())),
            ArchivedCard::faceup(CardId("seamless_launch".to_string())),
        ];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn");
        let (state, _) = pass_until_settled(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends");
        let state = advance_until_choice(state, &registry);
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, .. })), "offered a transaction");
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "seamless_launch") }).is_err(), "not a transaction");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).expect("pick it");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("play it");
        assert_eq!(state.corp.resources.credits, Credits(9), "5 - 5 + 9");
        assert!(state.corp.removed_from_game.contains(&CardId("hedge_fund".to_string())), "gone after it resolves");
        assert!(!state.corp.archives.iter().any(|a| a.card.0 == "hedge_fund"));
    }

    #[test]
    fn lamplighter_taxes_a_tag_or_three_credits_and_burns_out_when_its_server_gives_up_an_agenda() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.corp.installed = vec![
            ice_installed("lamplighter", ServerId::Remote(0), true),
            corp_root("offworld_office", ServerId::Remote(0)),
        ];
        state.corp.installed[1].rezzed = false;

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
        let state = advance_until_choice(state, &registry);
        assert!(matches!(state.pending_paid_choice.as_ref().map(|p| p.side), Some(Side::Runner)), "pay 3 or take the tag");

        // Paying keeps the Runner clean, and the second subroutine then
        // finds them untagged and lets the run continue.
        let (paid, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("pay 3");
        assert_eq!(paid.runner.resources.credits, Credits(2), "5 less the 3 it cost to dodge the tag");
        assert_eq!(paid.runner.tags, 0);
        let (paid, _) = pass_until_settled(paid, &registry);
        assert!(paid.active_run.is_some(), "an untagged Runner walks past subroutine 2");

        // Refusing takes the tag, and then the run ends.
        let (tagged, _) = apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).expect("refuse");
        assert_eq!(tagged.runner.tags, 1);
        let (tagged, _) = pass_until_settled(tagged, &registry);
        assert!(tagged.active_run.is_none(), "end the run if the Runner is tagged");

        // Stealing an agenda from the server it protects burns it out.
        let (state, _) = pass_until_settled(paid, &registry);
        let state = advance_until_choice(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::StealAgenda { card_id: CardId("offworld_office".to_string()) }).expect("steal");
        assert!(!state.corp.installed.iter().any(|c| c.card.0 == "lamplighter"), "trashed itself");
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "lamplighter"));
    }


    // ---- Elevation Stage 9: Hidden Funds, Peculiarity ----

    #[test]
    fn pt_untaian_pays_one_to_advance_an_unrezzed_card_when_hq_is_down_to_three() {
        let registry = sg_registry();
        let base = || {
            let mut state = base_state();
            state.corp.identity = Some(CardId("pt_untaian_lifes_building_blocks".to_string()));
            state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 4];
            state.corp.resources.credits = Credits(1);
            state.corp.installed = vec![
                corp_root("offworld_office", ServerId::Remote(0)),
                corp_root("pad_campaign", ServerId::Remote(1)),
                corp_root("nico_campaign", ServerId::Remote(2)),
            ];
            state.corp.installed[0].rezzed = false;
            state.corp.installed[1].rezzed = false;
            state
        };

        // Four cards in HQ closes the offer.
        let mut full = base();
        full.corp.hq = vec![CardId("hedge_fund".to_string()); 4];
        let (full, _) = apply_action(&full, &registry, PlayerAction::EndTurn).expect("end the corp turn");
        let full = advance_until_choice(full, &registry);
        assert!(full.pending_paid_choice.is_none(), "four in HQ is too many");

        // Three opens it.
        let mut state = base();
        state.corp.hq = vec![CardId("hedge_fund".to_string()); 3];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end the corp turn");
        let state = advance_until_choice(state, &registry);
        assert!(state.pending_paid_choice.is_some(), "three or fewer opens it");
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("pay 1");
        assert_eq!(state.corp.resources.credits, Credits(0));
        // A rezzed asset is not "an unrezzed card you can advance", and
        // neither is an unrezzed one with no advancement requirement.
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "nico_campaign") }).is_err(), "rezzed");
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "pad_campaign") }).is_err(), "not advanceable");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "offworld_office") }).expect("the agenda");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("advance it");
        let agenda = state.corp.installed.iter().find(|c| c.card.0 == "offworld_office").expect("still there");
        assert_eq!(agenda.advancement_tokens, 1);
    }

    #[test]
    fn proprionegation_scores_with_a_counter_that_sends_the_runner_back_to_archives() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.installed = vec![corp_root("proprionegation", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[0].advancement_tokens = 4;

        let (state, _) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "proprionegation") }).expect("score");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 1, "a counter on scoring");
        let ability = PlayerAction::ActivateAbility { target: state.corp.scored_agendas[0].install_id, ability_index: 0 };
        // Outside a run there is nothing to move, so it is never offered.
        let (state, _) = close_all_windows(state, &registry);
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&ability));

        // Mid-run it moves the Runner to the outermost position of Archives.
        let mut state = state;
        state.phase = crate::rules::GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.corp.installed = vec![
            ice_installed("ice_wall", ServerId::Hq, true),
            ice_installed("enigma", ServerId::Archives, false),
        ];
        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run HQ");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach Ice Wall");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes first");
        let (state, events) = apply_action(&state, &registry, ability).expect("spend the counter");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::RunRedirected { to: ServerId::Archives, .. })));
        let run = state.active_run.as_ref().expect("still running");
        assert_eq!((run.server, run.position, run.ice.len()), (ServerId::Archives, 0, 1), "outermost, nothing passed");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 0);
        // And only once: the counter is gone.
        let again = PlayerAction::ActivateAbility { target: state.corp.scored_agendas[0].install_id, ability_index: 0 };
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&again));
    }

    #[test]
    fn mitra_aman_trashes_itself_on_an_approach_to_pay_three_and_swap_the_ice_being_approached() {
        let registry = sg_registry();
        let mut state = runner_turn(5, 4);
        state.corp.resources.credits = Credits(0);
        state.corp.hq = vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())];
        state.corp.installed = vec![
            ice_installed("ice_wall", ServerId::Remote(0), false),
            corp_root("mitra_aman", ServerId::Remote(0)),
        ];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach the ice");
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseEffect { chooser: Side::Corp, .. })), "Mitra asks");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("trash it");
        assert_eq!(state.corp.resources.credits, Credits(3));
        assert!(state.corp.archives.iter().any(|a| a.card.0 == "mitra_aman"), "trashed itself");
        // Then the zone to swap from, and the card.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("from HQ");
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).is_err(), "not ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "enigma") }).expect("pick Enigma");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("swap");
        let ice = state.corp.installed.iter().find(|c| c.slot == InstallSlot::Ice).expect("still one piece of ice");
        assert_eq!((ice.card.0.as_str(), ice.rezzed), ("enigma", false), "Enigma took the position, unrezzed");
        assert!(state.corp.hq.contains(&CardId("ice_wall".to_string())), "Ice Wall went back to HQ");
        let run = state.active_run.as_ref().expect("still running");
        assert_eq!(run.ice[run.position].card_id.0, "enigma", "the approach follows the swap");
    }

    #[test]
    fn doomscroll_tags_damages_and_damages_again_only_at_two_tags() {
        let registry = sg_registry();
        let run_into_doomscroll = |tags: u32| {
            let mut state = runner_turn(5, 4);
            state.runner.tags = tags;
            state.runner.grip = vec![CardId("sure_gamble".to_string()); 6];
            state.corp.installed = vec![ice_installed("doomscroll", ServerId::Hq, true)];
            let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
            let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
            pass_until_settled(state, &registry).0
        };

        // From one tag: the first subroutine makes it two, so the third fires.
        let state = run_into_doomscroll(1);
        assert_eq!(state.runner.tags, 2);
        assert_eq!(state.runner.grip.len(), 3, "1 net then 2 more");

        // From none: one tag is not two, and the third subroutine does nothing.
        let state = run_into_doomscroll(0);
        assert_eq!(state.runner.tags, 1);
        assert_eq!(state.runner.grip.len(), 5, "1 net only");
    }


    // ---- Elevation Stage 10: Fine Print, Gimbatul, Not so subtle ----

    #[test]
    fn nebula_talent_management_flips_on_an_operation_refunds_a_click_and_flips_back_on_a_central_run() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.identity = Some(CardId("nebula_talent_management_making_stars".to_string()));
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 4];
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("hedge_fund".to_string())];

        // A turn with no operation played leaves it face up.
        let quiet = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn").0;
        assert!(!quiet.corp.identity_flipped, "no operation, no flip");

        // Playing one flips it at the end of the action phase, for a credit.
        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hedge_fund".to_string()) }).expect("play");
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn");
        assert!(state.corp.identity_flipped);
        assert_eq!(state.corp.resources.credits, Credits(15), "10 - 5 + 9 + 1");

        // Flipped, the first operation each turn refunds a click.
        let (state, _) = pass_until_settled(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends");
        let (state, _) = pass_until_settled(state, &registry);
        assert_eq!(state.corp.resources.clicks, Clicks(3));
        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hedge_fund".to_string()) }).expect("play again");
        assert_eq!(state.corp.resources.clicks, Clicks(3), "spent one, got one back");

        // A successful run on a central flips it back.
        let mut running = state.clone();
        running.phase = crate::rules::GamePhase::Action(Side::Runner);
        running.runner.resources.clicks = Clicks(4);
        let (running, _) = apply_action(&running, &registry, PlayerAction::InitiateRun { server: ServerId::RnD }).expect("run R&D");
        let running = advance_until_choice(running, &registry);
        assert!(!running.corp.identity_flipped, "flipped back on a successful central run");
    }

    #[test]
    fn synapse_global_installs_from_hq_the_first_time_a_tag_comes_off_each_turn() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.identity = Some(CardId("synapse_global_faster_than_thought".to_string()));
        state.corp.hq = vec![CardId("ice_wall".to_string())];
        state.runner.tags = 2;
        let ability = PlayerAction::ActivateAbility { target: InstallId::CORP_IDENTITY, ability_index: 0 };

        let (state, _) = apply_action(&state, &registry, ability.clone()).expect("click, remove a tag");
        assert_eq!(state.runner.tags, 1);
        // …and the removal offers a free install out of HQ, *before* the
        // ability's own credits: the prompt parks mid-`Sequence` and the
        // gain is the continuation.
        assert!(matches!(state.pending_decision, Some(crate::rules::PendingDecision::ChooseCards { side: Side::Corp, .. })));
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("pick it");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ChooseServerForPendingDecision { server: ServerId::Hq }).expect("protect HQ");
        assert!(state.corp.installed.iter().any(|c| c.card.0 == "ice_wall"));
        assert_eq!(state.corp.resources.credits, Credits(12), "the install was free, and the ability paid 2");

        // Once per turn: the second tag comes off with no install offered.
        let (state, _) = apply_action(&state, &registry, ability).expect("remove the other tag");
        assert_eq!(state.runner.tags, 0);
        assert!(state.pending_decision.is_none(), "once per turn");

        // With no tag at all the ability is not offered.
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&PlayerAction::ActivateAbility { target: InstallId::CORP_IDENTITY, ability_index: 0 }));
    }

    #[test]
    fn embedded_reporting_pays_two_dividends_per_excess_and_buys_back_an_operation() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.r_and_d = vec![CardId("ice_wall".to_string()), CardId("hedge_fund".to_string())];
        state.corp.installed = vec![corp_root("embedded_reporting", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[0].advancement_tokens = 5;

        let (state, _) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "embedded_reporting") }).expect("score");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 4, "Dividends 2, two excess");
        let (state, _) = close_all_windows(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("end turn");
        let state = advance_until_choice(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None }).expect("spend one");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 3);
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).is_err(), "not an operation");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") }).expect("find the operation");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        assert_eq!(state.corp.r_and_d.last(), Some(&CardId("hedge_fund".to_string())), "on top of R&D");
        assert_eq!(state.corp.r_and_d.len(), 2, "it moved rather than multiplied");
    }

    #[test]
    fn next_big_thing_scores_with_a_counter_that_draws_four_and_buries_hq() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 6];
        state.corp.hq = vec![CardId("ice_wall".to_string())];
        state.corp.installed = vec![corp_root("next_big_thing", ServerId::Remote(0))];
        state.corp.installed[0].rezzed = false;
        state.corp.installed[0].advancement_tokens = 5;

        let (state, _) = apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "next_big_thing") }).expect("score");
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 1);
        let (state, _) = close_all_windows(state, &registry);
        let ability = PlayerAction::ActivateAbility { target: state.corp.scored_agendas[0].install_id, ability_index: 0 };
        let (state, _) = apply_action(&state, &registry, ability).expect("a click and the counter");
        assert_eq!(state.corp.resources.clicks, Clicks(2));
        assert_eq!(state.corp.scored_agendas[0].agenda_counters, 0);
        assert_eq!(state.corp.hq.len(), 5, "drew four");
        // Any number of cards from HQ go back into R&D.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "ice_wall") }).expect("bury one");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("shuffle it in");
        assert_eq!(state.corp.hq.len(), 4);
        assert_eq!(state.corp.r_and_d.len(), 3, "two left plus the one shuffled back");
    }

    #[test]
    fn public_access_plaza_pays_a_credit_a_turn_and_tags_its_trasher_only_at_threat_two() {
        let registry = sg_registry();
        let trash_it = |threat_points: usize, rezzed: bool| {
            let mut state = runner_turn(10, 4);
            state.runner.scored_agendas = vec![CardId("offworld_office".to_string()); threat_points];
            state.corp.installed = vec![corp_root("public_access_plaza", ServerId::Remote(0))];
            state.corp.installed[0].rezzed = rezzed;
            let want = PlayerAction::TrashAccessedCard { card_id: CardId("public_access_plaza".to_string()) };
            let (mut state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("run");
            for _ in 0..30 {
                let legal = crate::rules::legal_actions(&state, &registry);
                if legal.contains(&want) {
                    break;
                }
                let Some(action) = legal.into_iter().find(|a| {
                    matches!(a, PlayerAction::PassPriority { .. } | PlayerAction::ContinueRun | PlayerAction::CompleteRun)
                }) else {
                    break;
                };
                state = apply_action(&state, &registry, action).expect("walk to the access").0;
            }
            apply_action(&state, &registry, want).expect("trash").0
        };

        // Threat 0: no tag. (Each Offworld Office is 2 points.)
        assert_eq!(trash_it(0, true).runner.tags, 0);
        // Threat 2: tagged.
        assert_eq!(trash_it(1, true).runner.tags, 1);
        // Threat 2 but the asset was never rezzed: no tag.
        assert_eq!(trash_it(1, false).runner.tags, 0, "only while it is rezzed");

        // And it pays a credit at the start of each Corp turn.
        let mut state = base_state();
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 3];
        state.corp.installed = vec![corp_root("public_access_plaza", ServerId::Remote(0))];
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("corp ends");
        let (state, _) = pass_until_settled(state, &registry);
        let (state, _) = apply_action(&state, &registry, PlayerAction::EndTurn).expect("runner ends");
        let (state, _) = pass_until_settled(state, &registry);
        assert_eq!(state.corp.resources.credits, Credits(11));
    }

    #[test]
    fn n_pot_lets_only_the_runner_pay_three_to_break_and_ends_the_run_harder_as_the_threat_rises() {
        let registry = sg_registry();
        let approach = |runner_points: usize| {
            let mut state = runner_turn(10, 4);
            state.runner.scored_agendas = vec![CardId("offworld_office".to_string()); runner_points];
            state.corp.installed = vec![ice_installed("n_pot", ServerId::Hq, true)];
            let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("run");
            let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach");
            let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes");
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("encounter").0
        };
        let break_one = PlayerAction::ActivateAbility { target: fixture_install_id("n_pot"), ability_index: 0 };

        // The ability belongs to the Runner even though the card is the Corp's.
        let state = approach(0);
        assert!(crate::rules::legal_actions_for(&state, &registry, Side::Runner).contains(&break_one));
        assert!(!crate::rules::legal_actions_for(&state, &registry, Side::Corp).contains(&break_one));
        let (broken, events) = apply_action(&state, &registry, break_one.clone()).expect("pay 3, break 1");
        assert_eq!(broken.runner.resources.credits, Credits(7));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::SubroutineBroken { index: 0, .. })));

        // Threat 0: only the first subroutine ends the run, and it was broken.
        let (state, _) = pass_until_settled(broken, &registry);
        assert!(state.active_run.is_some(), "the other two are threat-gated");

        // Threat 2 (one 2-point agenda stolen): the second one bites.
        let state = approach(1);
        let (state, _) = apply_action(&state, &registry, break_one).expect("break the first");
        let (state, _) = pass_until_settled(state, &registry);
        assert!(state.active_run.is_none(), "subroutine 2 ended the run");
    }

    #[test]
    fn bigger_picture_either_tags_again_or_drains_five_a_tag_into_the_corps_pocket() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(0);
        state.corp.hq = vec![CardId("bigger_picture".to_string()); 2];
        state.runner.resources.credits = Credits(12);
        state.runner.tags = 2;
        let play = PlayerAction::PlayOperation { card_id: CardId("bigger_picture".to_string()) };

        // Only against a tagged Runner.
        let mut untagged = state.clone();
        untagged.runner.tags = 0;
        assert!(!crate::rules::legal_actions(&untagged, &registry).contains(&play));

        // One more tag…
        let (state, _) = apply_action(&state, &registry, play).expect("play");
        let (tagged, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 }).expect("give a tag");
        assert_eq!(tagged.runner.tags, 3);

        // …or 5 credits a tag, straight across the table.
        let (drained, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 }).expect("drain");
        assert_eq!(drained.runner.resources.credits, Credits(2), "12 less 5 per tag");
        assert_eq!(drained.corp.resources.credits, Credits(10), "and the Corp takes what they lost");
        assert_eq!(drained.runner.tags, 0, "the tags come off");
    }

    #[test]
    fn ip_enforcement_buys_a_stolen_agenda_back_for_its_own_points_in_tags() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.hq = vec![CardId("ip_enforcement".to_string())];
        state.runner.scored_agendas = vec![
            CardId("offworld_office".to_string()),
            CardId("send_a_message".to_string()),
        ];
        state.runner.resources.agenda_points = AgendaPoints(5);
        state.runner.tags = 2;

        let (state, _) = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("ip_enforcement".to_string()) }).expect("play");
        // Send a Message is 3 points and two tags cannot pay for it.
        assert!(apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "send_a_message") }).is_err(), "3 points, 2 tags");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "offworld_office") }).expect("the 2-pointer");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("take it back");
        assert_eq!(state.runner.tags, 0, "two tags paid for two points");
        assert_eq!(state.runner.scored_agendas, vec![CardId("send_a_message".to_string())]);
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(3));
        let installed = state.corp.installed.iter().find(|c| c.card.0 == "offworld_office").expect("back on the table");
        assert!(matches!(installed.server, ServerId::Remote(_)));
        assert_eq!(installed.advancement_tokens, 0, "the Runner is no longer tagged, so no counter");
    }

}
