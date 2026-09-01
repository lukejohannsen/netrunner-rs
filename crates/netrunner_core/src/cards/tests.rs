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
    // Runner: started at 5, paid 0 to play Account Siphon (it is printed at
    // cost 0), gained 10 from the siphon.
    assert_eq!(state.runner.resources.credits, Credits(15));
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
        assert_eq!(state.runner.rig[0].hosted_on_ice, Some(CardId("palisade".to_string())));
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
        rig[0].hosted_on_ice = Some(CardId("palisade".to_string()));
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
        rig[0].hosted_on_ice = Some(CardId("palisade".to_string()));
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
        assert_eq!(state.corp.scored_agendas, vec![CardId("offworld_office".to_string())]);
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

    /// Both subroutines fire in the same window-close batch —
    /// `resolve_unbroken_subroutines` only pauses between subroutines for a
    /// `Trace`/`PendingPrevention`, not `PermitJackOut`, so all 4 net damage
    /// (2 + 2) resolves before control returns to either side. What
    /// `PermitJackOut` actually buys the Runner, given this engine's
    /// per-encounter (not per-subroutine) window granularity, is the ability
    /// to `JackOut` instead of continuing the run once the encounter
    /// resolves — checked here via `jack_out_permitted` and the emitted
    /// event, not via an interrupt between the two subroutines.
    #[test]
    fn karuna_first_subroutine_permits_jack_out_and_both_subroutines_deal_net_damage() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = (0..5).map(|i| CardId(format!("grip_card_{i}"))).collect();
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1005),
            card: CardId("karuna".to_string()),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("approach karuna");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes approach window");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).expect("corp passes approach window, entering encounter");

        assert!(
            !state.active_run.as_ref().unwrap().jack_out_permitted,
            "jack-out should be closed on committing to the encounter"
        );

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).expect("runner passes encounter window");
        let (state, events) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("corp passes encounter window, firing both subroutines");

        assert_eq!(state.runner.grip.len(), 1, "4 of the 5 grip cards should have been discarded to the two subroutines' net damage");
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::JackOutPermitted { .. })));
        assert!(
            state.active_run.as_ref().unwrap().jack_out_permitted,
            "the first subroutine should have re-opened the jack-out window"
        );
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

    #[test]
    fn predictive_planogram_resolves_both_options_when_the_runner_is_tagged() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(10);
        state.corp.hq = vec![CardId("predictive_planogram".to_string())];
        state.corp.r_and_d = (0..5).map(|i| CardId(format!("rd_card_{i}"))).collect();
        state.runner.tags = 1;

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("predictive_planogram".to_string()) })
                .expect("play predictive planogram");

        assert!(state.pending_decision.is_none(), "tagged case resolves both immediately, no choice needed");
        assert_eq!(state.corp.resources.credits, Credits(13), "10 - 0 (cost) + 3");
        assert_eq!(state.corp.hq.len(), 3);
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Corp, amount: 3 })));
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

    #[test]
    fn mayfly_trashes_itself_when_the_run_it_was_installed_during_ends() {
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.rig = vec![crate::rules::InstalledRunnerCard {
            card: CardId("mayfly".to_string()),
            base_strength: 1,
            ..Default::default()
        }];
        state.corp.archives = vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Archives }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("reach success, no ice");
        let (state, _) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("open pre-access window");
        let (state, _) = close_all_windows(state, &registry);
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PassAccessedCard { card_id: CardId("hedge_fund".to_string()) })
                .expect("pass on hedge fund, concluding the run");

        assert!(state.runner.rig.is_empty(), "mayfly should have trashed itself when the run ended");
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

        assert!(state.corp.scored_agendas.contains(&CardId("longevity_serum".to_string())));
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

    #[test]
    fn hansei_review_gains_ten_credits_then_trashes_one_hq_card() {
        let registry = sg_registry();
        let mut state = base_state();
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("hansei_review".to_string()), CardId("hedge_fund".to_string())];

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: CardId("hansei_review".to_string()) })
                .expect("play hansei review");

        assert_eq!(state.corp.resources.credits, Credits(10), "5 - 5 (cost) + 10 (effect)");
        assert_eq!(state.corp.hq, vec![CardId("hedge_fund".to_string())]);

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") })
                .expect("toggle hedge_fund");
        let (state, events) =
            apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm trash");

        assert!(state.corp.hq.is_empty());
        assert!(state.corp.archives_contains(&CardId("hedge_fund".to_string())));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CardsSelected { .. })));
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

        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ScoreAgenda { target: install_of(&state, "above_the_law") })
                .expect("score above the law");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to trash a resource");
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

        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("corp chooses to pay and trash");
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
        // the last one.
        for _ in 0..3 {
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

    #[test]
    fn red_team_pays_out_on_any_successful_run_not_only_ones_it_initiated() {
        // Documents a deliberate simplification: real Red Team only pays
        // out for runs *it itself* initiates ("[click]: Run a central
        // server you have not run this turn..."). This engine's
        // `Trigger::OnSuccessfulRun` dispatch has no per-run
        // initiator-attribution mechanism (building one would mean adding
        // a new field to every one of ~30 `RunState` construction sites
        // across the crate for a single card), so Red Team's payout fires
        // for *any* successful run while it's installed — including a
        // plain `PlayerAction::InitiateRun` the Runner took via the basic
        // action, not through Red Team's own paid ability. Also
        // unenforced: the real restriction to central servers only, and
        // "a server not already run this turn."
        let registry = sg_registry();
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![rig_card_with_counters("red_team", 12)];

        let (state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::RnD }).expect("initiate run");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ContinueRun).expect("resolves immediately, empty rd");
        let (state, events) = apply_action(&state, &registry, PlayerAction::CompleteRun).expect("commit: the run is now successful");

        assert_eq!(state.runner.rig[0].counters, 9);
        assert_eq!(state.runner.resources.credits, Credits(3));
        assert!(events.iter().any(|e| matches!(e, crate::rules::GameEvent::CreditsGained { side: Side::Runner, amount: 3 })));
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
        assert_eq!(hosted.runner.rig[0].hosted_on_ice, Some(CardId("wall_of_static".to_string())));

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
        assert_eq!(state.runner.rig[0].hosted_on_ice, Some(CardId("wall_of_static".to_string())));
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
        state.corp.hq = vec![CardId("hedge_fund".to_string())];

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

        // Subroutine 2: may install 1 card from HQ or Archives — choose HQ.
        let (state, _) = apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 0 })
            .expect("choose to install from HQ");
        let (state, _) =
            apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: position_of(&state, "hedge_fund") })
                .expect("toggle hedge_fund");
        let (state, _) = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection)
            .expect("confirm install, ignoring cost, resuming subroutine resolution");
        assert!(!state.corp.hq.contains(&CardId("hedge_fund".to_string())));
        assert!(state.corp.installed.iter().any(|c| c.card == CardId("hedge_fund".to_string()) && c.server == ServerId::Hq && !c.rezzed));
        assert_eq!(state.corp.resources.credits, Credits(10), "installed ignoring hedge_fund's printed cost — unchanged from the starting balance");

        // Subroutine 3: prevent steal/trash for the remainder of this run.
        assert!(state.active_run.as_ref().unwrap().runner_cannot_steal_or_trash);
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
        assert_eq!(next.corp.scored_agendas, vec![CardId("offworld_office".to_string())]);
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
            vec!["botulus", "conduit", "fermenter", "leech", "tranquilizer"],
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
}
