//! The view-based no-deadlock sweep: many seeds of real agent play across
//! every sample matchup, asserting the match always reaches `GameOver`.
//!
//! **Why this exists alongside
//! `no_panics_or_deadlocks_across_many_seeds_system_gateway`.** That sweep
//! drives *index-based* agents through `SinglePlayerSession`, so every
//! decision goes through `ActionSpace::index_of`/`action_at` over the
//! side-agnostic `get_action_mask`. This one drives `Seat::Agent`, so each
//! side chooses only from `legal_actions_for` — the per-seat `ClientView`
//! slice a real client actually receives. The two reach different code, and
//! neither substitutes for the other: the index path's round trip hid both
//! an engine panic and a deadlock that were trivially reachable here, on
//! ordinary sample decks at low seeds.
//!
//! The invariant this pins is the one every deadlock found in this repo has
//! violated: **if `current_actor` names a side, that side must have at
//! least one legal action.** `Session` reports a violation directly as
//! `StallReason::NoLegalActions { side }` rather than conflating it with
//! budget exhaustion the way each hand-rolled `else { break }` used to.

use netrunner_bots::{BotAgent, HeuristicAgent, RandomAgent};
use netrunner_core::cards::{register_playable_cards, CardRegistry};
use netrunner_core::decks;
use netrunner_core::rules::{GameState, MaskedZone, PublicAccessPhase, Side};
use netrunner_session::{Seat, Session, SessionStep};

/// How many seeds the sweep walks, default 32 — sized for the inner loop,
/// matching `system_gateway_delivery::sweep_seed_count`. Raise it for a deep
/// run before merging engine-level work:
///
/// ```text
/// NETRUNNER_SWEEP_SEEDS=256 cargo test -p netrunner_session --release
/// ```
fn sweep_seed_count() -> u64 {
    std::env::var("NETRUNNER_SWEEP_SEEDS").ok().and_then(|value| value.parse().ok()).unwrap_or(32)
}

#[test]
fn view_based_agents_never_reach_a_state_with_no_legal_action() {
    let matchups = decks::matchups();
    assert!(!matchups.is_empty(), "the embedded sample decks should yield at least one matchup");

    for seed in 0..sweep_seed_count() {
        // Rotating by seed rather than nesting a second loop keeps the
        // default run's cost in line with the existing sweep while still
        // covering all twelve pairings as the seed count rises.
        let (corp_deck, runner_deck) = &matchups[seed as usize % matchups.len()];
        let matchup = format!("{} vs {}", corp_deck.id, runner_deck.id);

        for runner_is_random in [false, true] {
            let mut registry = CardRegistry::new();
            register_playable_cards(&mut registry);
            let (state, _events) =
                GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed)
                    .expect("sample decks are legal by construction");

            // Both seatings, since which side holds the random agent
            // changes which decisions get explored.
            let (corp, runner): (Box<dyn netrunner_bots::BotAgent>, Box<dyn netrunner_bots::BotAgent>) =
                if runner_is_random {
                    (Box::new(HeuristicAgent::new(Side::Corp, seed)), Box::new(RandomAgent::new(seed)))
                } else {
                    (Box::new(RandomAgent::new(seed)), Box::new(HeuristicAgent::new(Side::Runner, seed)))
                };

            let mut session = Session::new(state, registry, Seat::Agent(corp), Seat::Agent(runner));
            match session.run() {
                SessionStep::Ended { .. } => {}
                SessionStep::Stalled(reason) => panic!(
                    "seed {seed} ({matchup}, runner_is_random={runner_is_random}) stalled: {reason:?} \
                     after {} actions",
                    session.steps()
                ),
                other => panic!(
                    "seed {seed} ({matchup}, runner_is_random={runner_is_random}): \
                     an all-Agent session should never yield {other:?}"
                ),
            }
            assert!(!session.history().is_empty(), "seed {seed} ({matchup}): recorded no actions");
        }
    }
}

/// The specific position that motivated this sweep, pinned by seed so a
/// regression names itself rather than hiding in a 32-seed run.
///
/// A flatlining subroutine on a multi-subroutine ICE used to leave the run
/// parked at `EncounterIce` with subroutines still pending while
/// `phase` was already `GameOver`. `resolve_encounter_ice` then tried to
/// advance past the ICE anyway, `continue_run` refused with
/// `SubroutinesStillPending`, and that error propagated out through
/// `close_window` — making the Corp's own `PassPriority` illegal while
/// `current_actor` still named them. See `paid_ability::resolve_encounter_ice`.
#[test]
fn the_flatline_during_an_encounter_window_position_plays_out() {
    let (corp_deck, runner_deck) = decks::matchups().into_iter().next().expect("at least one matchup");
    let mut registry = CardRegistry::new();
    register_playable_cards(&mut registry);
    let (state, _events) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 2)
        .expect("sample decks are legal by construction");

    let mut session = Session::new(
        state,
        registry,
        Seat::Agent(Box::new(RandomAgent::new(2))),
        Seat::Agent(Box::new(RandomAgent::new(3))),
    );
    match session.run() {
        SessionStep::Ended { .. } => {}
        other => panic!(
            "seed 2 of {} vs {} must play to a conclusion, got {other:?}",
            corp_deck.id,
            runner_deck.id,
        ),
    }
}

/// **No `ClientView` may name a card it also conceals.**
///
/// Fog of war is meant to be structural at the `ClientView` boundary
/// (AGENTS.md §2: never by asking the client to be polite), but nothing
/// asserted that the *actions* a view offers respect it. Two did not:
///
/// - `pending_choice::zone_card_ids` read the real `corp.installed`, so
///   *Tāo Salonga*'s selection over `OpponentInstalled` offered the Runner
///   `ToggleCardSelection` naming an unrezzed ICE their own view masked;
/// - `install_program_on_ice_candidates` paired every Trojan with every
///   installed ICE, "rezzed or not", carrying the host's real `CardId`.
///
/// Both actions are *legal* — real Netrunner lets the Runner host on and
/// swap ICE they cannot identify — so the fix was to name the install
/// rather than the card (`state::InstallId`), not to withdraw the action.
/// This is the gate on that: for every view either seat receives, no card
/// the view hides may appear in `legal_actions` or `pending_decision`.
///
/// Driven through `Seat::External` on both sides, so every action is chosen
/// from a real per-seat `ClientView` — the same slice a networked client
/// gets, and the only shape in which this property is even observable.
#[test]
fn no_client_view_ever_names_a_card_it_conceals() {
    let matchups = decks::matchups();

    for seed in 0..sweep_seed_count() {
        let (corp_deck, runner_deck) = &matchups[seed as usize % matchups.len()];
        let matchup = format!("{} vs {}", corp_deck.id, runner_deck.id);

        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        let (state, _events) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed)
            .expect("sample decks are legal by construction");

        // Both seats are `External` so every action is chosen from a real
        // `ClientView` this test can inspect — but they are *driven* by the
        // same agents the sweep above seats directly, so play is as strong
        // here as there. A uniform random picker would confound a genuine
        // stall with "random play is just slow", which it did: it hit the
        // step budget on ordinary positions.
        let mut corp = HeuristicAgent::new(Side::Corp, seed);
        let mut runner = RandomAgent::new(seed);
        let mut session = Session::new(state, registry, Seat::External, Seat::External);

        loop {
            match session.step() {
                SessionStep::Awaiting { side, view } => {
                    assert_no_concealed_card_is_named(&view, session.state(), seed, &matchup, side);

                    assert!(
                        !view.legal_actions.is_empty(),
                        "seed {seed} ({matchup}): {side:?} was asked to act with no legal action"
                    );
                    let action = match side {
                        Side::Corp => corp.select_action(&view, session.registry()),
                        Side::Runner => runner.select_action(&view, session.registry()),
                    };
                    session.submit(action).unwrap_or_else(|e| {
                        panic!("seed {seed} ({matchup}): {side:?} submitted a legal action, rejected: {e:?}")
                    });
                }
                // An action resolved with nothing further owed — keep pumping.
                SessionStep::Applied { .. } => {}
                SessionStep::Ended { .. } => break,
                SessionStep::Stalled(reason) => {
                    panic!("seed {seed} ({matchup}) stalled: {reason:?} after {} actions", session.steps())
                }
            }
        }
    }
}

/// Asserts the invariant for one view.
///
/// A card counts as concealed when the view renders an install with
/// `card: None` **and** that card's identity is not legitimately visible
/// somewhere else in the same view — two copies of one ICE, one rezzed and
/// one not, leave the title public, so naming it leaks nothing. Access is
/// the other such route: an unrezzed asset the Runner is accessing is
/// theirs to see, which is what accessing *is*.
///
/// The check is over the `Debug` rendering rather than a match on all 40-odd
/// `PlayerAction` variants: a new variant carrying a `CardId` is then
/// covered the day it is added, with no chance of anyone forgetting to
/// extend a list here.
fn assert_no_concealed_card_is_named(
    view: &netrunner_core::view::ClientView,
    state: &GameState,
    seed: u64,
    matchup: &str,
    side: Side,
) {
    use std::collections::HashSet;

    let mut visible: HashSet<&str> = HashSet::new();
    for server in &view.corp.servers {
        for card in server.ice.iter().chain(server.root.iter()) {
            if let Some(id) = &card.card {
                visible.insert(id.0.as_str());
            }
        }
    }
    visible.extend(view.corp.archives.iter().filter_map(|a| a.card.as_ref()).map(|c| c.0.as_str()));
    visible.extend(view.corp.scored_agendas.iter().map(|c| c.0.as_str()));
    visible.extend(view.runner.scored_agendas.iter().map(|c| c.0.as_str()));
    visible.extend(view.runner.heap.iter().map(|c| c.0.as_str()));
    visible.extend(view.runner.rig.iter().map(|c| c.card.0.as_str()));
    if let Some(cards) = &view.corp.hq_cards {
        visible.extend(cards.iter().map(|c| c.0.as_str()));
    }
    if let Some(cards) = &view.runner.grip_cards {
        visible.extend(cards.iter().map(|c| c.0.as_str()));
    }
    if let Some(run) = &view.active_run {
        for ice in &run.ice {
            if let Some(identity) = &ice.identity {
                visible.insert(identity.card.0.as_str());
            }
        }
        // Access reveals. The masking layer already draws this line — see
        // `PublicAccessState`'s doc comment — and the invariant is about
        // actions naming what the view hides, not about the table alone.
        if let Some(access) = &run.access_state {
            for zone in [&access.unaccessed_cards, &access.resolved_cards] {
                if let MaskedZone::Visible(cards) = zone {
                    visible.extend(cards.iter().map(|c| c.0.as_str()));
                }
            }
            match &access.phase {
                PublicAccessPhase::SelectNextCard { selectable_cards: MaskedZone::Visible(cards) } => {
                    visible.extend(cards.iter().map(|c| c.0.as_str()));
                }
                PublicAccessPhase::PendingInteractiveTrigger { card: Some(id), .. }
                | PublicAccessPhase::PendingChoice { card: Some(id), .. } => {
                    visible.insert(id.0.as_str());
                }
                _ => {}
            }
        }
    }

    // Every install this view masks, resolved through the real state to the
    // card it is actually hiding.
    let masked: Vec<&str> = view
        .corp
        .servers
        .iter()
        .flat_map(|server| server.ice.iter().chain(server.root.iter()))
        .filter(|card| card.card.is_none())
        .filter_map(|card| state.find_corp_install(card.install_id))
        .map(|installed| installed.card.0.as_str())
        .filter(|id| !visible.contains(id))
        .collect();
    if masked.is_empty() {
        return;
    }

    let actions = format!("{:?}", view.legal_actions);
    let decision = format!("{:?}", view.pending_decision);
    for id in masked {
        let quoted = format!("\"{id}\"");
        assert!(
            !actions.contains(&quoted),
            "seed {seed} ({matchup}): {side:?}'s legal_actions name {id}, which their own view masks — {actions}"
        );
        assert!(
            !decision.contains(&quoted),
            "seed {seed} ({matchup}): {side:?}'s pending_decision names {id}, which their own view masks — {decision}"
        );
    }
}
