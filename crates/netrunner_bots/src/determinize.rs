//! Samples one concrete, `ClientView`-consistent `GameState` — the "I" in
//! Information Set MCTS: `MctsAgent` runs its unchanged Phase-1 search
//! machinery against a sampled state instead of the real (unavailable) one,
//! and `HeuristicAgent` does the same for its one-ply lookahead.
//!
//! There's no decklist/multiplicity concept anywhere in this engine, so
//! hidden slots are filled by drawing from the full `CardRegistry` pool for
//! the right side (optionally type-constrained — an unrezzed ICE slot only
//! ever draws an ICE-typed card, a root slot only an Agenda/Asset), minus
//! every card id already visible somewhere in the view. This is a known,
//! documented baseline simplification, not an attempt at a "real" opponent
//! hand model.

use std::collections::HashSet;

use rand::seq::SliceRandom;
use rand::Rng;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, CardType};
use netrunner_core::rules::{
    ArchivedCard,
    AccessPhase, AccessState, AgendaPoints, Clicks, CorpState, Credits, GameState, InstallSlot, InstalledCard,
    InstalledRunnerCard, MaskedZone, MemoryUnits, PlayerResources, PublicAccessPhase,
    RunIce, RunState, RunnerState, Side,
};
use netrunner_core::view::ClientView;

/// A shuffled draw pool that cycles once exhausted (draws-with-replacement
/// across repeated full passes) — matches "shuffle then keep drawing" for
/// however many hidden slots need filling, however many that is.
struct Pool {
    cards: Vec<CardId>,
    cursor: usize,
}

impl Pool {
    fn new(mut cards: Vec<CardId>, rng: &mut impl Rng) -> Self {
        cards.shuffle(rng);
        Pool { cards, cursor: 0 }
    }

    /// Falls back to a synthetic placeholder id only in the pathological
    /// case of an empty pool (no registered cards of the needed
    /// type/side at all) — keeps the caller total instead of panicking.
    fn draw(&mut self) -> CardId {
        if self.cards.is_empty() {
            return CardId("__determinize_unknown".to_string());
        }
        let card = self.cards[self.cursor % self.cards.len()].clone();
        self.cursor += 1;
        card
    }

    fn draw_n(&mut self, n: usize) -> Vec<CardId> {
        (0..n).map(|_| self.draw()).collect()
    }
}

struct Pools {
    corp_any: Pool,
    corp_ice: Pool,
    corp_root: Pool,
    runner_any: Pool,
}

fn visible_card_ids(view: &ClientView) -> HashSet<CardId> {
    let mut ids = HashSet::new();
    if let Some(cards) = &view.corp.hq_cards {
        ids.extend(cards.iter().cloned());
    }
    // Only Archives cards whose identity this viewer can actually see —
    // a facedown card is hidden from the Runner (`PublicArchivedCard::card`
    // is `None`), so it must be sampled from the pool like any other
    // unknown, not treated as already-visible.
    ids.extend(view.corp.archives.iter().filter_map(|a| a.card.clone()));
    ids.extend(view.corp.scored_agendas.iter().cloned());
    for server in &view.corp.servers {
        for card in server.ice.iter().chain(server.root.iter()) {
            if let Some(id) = &card.card {
                ids.insert(id.clone());
            }
        }
    }

    if let Some(cards) = &view.runner.grip_cards {
        ids.extend(cards.iter().cloned());
    }
    ids.extend(view.runner.heap.iter().cloned());
    ids.extend(view.runner.scored_agendas.iter().cloned());
    for rig_card in &view.runner.rig {
        ids.insert(rig_card.card.clone());
    }

    if let Some(run) = &view.active_run {
        for ice in &run.ice {
            if let Some(identity) = &ice.identity {
                ids.insert(identity.card.clone());
            }
        }
        if let Some(access) = &run.access_state {
            if let MaskedZone::Visible(cards) = &access.unaccessed_cards {
                ids.extend(cards.iter().cloned());
            }
            if let MaskedZone::Visible(cards) = &access.resolved_cards {
                ids.extend(cards.iter().cloned());
            }
            match &access.phase {
                PublicAccessPhase::SelectNextCard { selectable_cards: MaskedZone::Visible(cards) } => {
                    ids.extend(cards.iter().cloned());
                }
                PublicAccessPhase::PendingInteractiveTrigger { card: Some(id), .. }
                | PublicAccessPhase::PendingChoice { card: Some(id), .. } => {
                    ids.insert(id.clone());
                }
                _ => {}
            }
        }
    }

    ids
}

fn build_pools(view: &ClientView, registry: &CardRegistry, rng: &mut impl Rng) -> Pools {
    let visible = visible_card_ids(view);
    let corp_cards: Vec<&netrunner_core::dsl::CardDefinition> = registry.iter().filter(|c| c.side == Side::Corp && !visible.contains(&c.id)).collect();
    let runner_cards: Vec<CardId> =
        registry.iter().filter(|c| c.side == Side::Runner && !visible.contains(&c.id)).map(|c| c.id.clone()).collect();

    let corp_any: Vec<CardId> = corp_cards.iter().map(|c| c.id.clone()).collect();
    let corp_ice: Vec<CardId> = corp_cards.iter().filter(|c| matches!(c.card_type, CardType::Ice(_))).map(|c| c.id.clone()).collect();
    let corp_root: Vec<CardId> = corp_cards
        .iter()
        .filter(|c| matches!(c.card_type, CardType::Agenda | CardType::Asset))
        .map(|c| c.id.clone())
        .collect();

    Pools {
        // Falling back to the unfiltered `corp_any` pool when no
        // type-matching card is registered at all keeps a determinized
        // sample buildable even against a tiny/synthetic test registry,
        // rather than only ever emitting the placeholder id.
        corp_ice: Pool::new(if corp_ice.is_empty() { corp_any.clone() } else { corp_ice }, rng),
        corp_root: Pool::new(if corp_root.is_empty() { corp_any.clone() } else { corp_root }, rng),
        corp_any: Pool::new(corp_any, rng),
        runner_any: Pool::new(runner_cards, rng),
    }
}

/// Samples one server's installs, each paired with the position it holds
/// in the **real** `corp.installed` — see `determinize`'s reassembly, which
/// is what that position is for.
fn determinize_installed(
    server_view: &netrunner_core::view::ServerView,
    pools: &mut Pools,
) -> Vec<(usize, InstalledCard)> {
    let ice = server_view.ice.iter().map(|card| (card.position, InstalledCard {
        card: card.card.clone().unwrap_or_else(|| pools.corp_ice.draw()),
        // Carried straight off the view, never reallocated. This is what
        // keeps the sample's actions the *same* actions as the caller's:
        // an `InstallId` is public, so the real state and every
        // determinized hypothetical must agree on it even where they
        // disagree completely about which card it names. Reallocating here
        // would recreate exactly the disjoint-action-set bug this handle
        // was introduced to remove.
        install_id: card.install_id,
        server: card.server,
        slot: InstallSlot::Ice,
        rezzed: card.rezzed,
        advancement_tokens: card.advancement_tokens,
        // `None` is genuine ignorance, not a drop: the view masks an
        // unrezzed Corp card's counters because they would leak its
        // identity, so 0 is the honest sample. When they *are* visible,
        // carry them — zeroing a rezzed card's counters made its
        // counter-costed abilities illegal in the sample.
        counters: card.counters.unwrap_or(0),
        // `ClientView` doesn't carry install timing; a rollout re-derives it
        // from its own play-out, same approximation as `counters` above.
        installed_this_turn: false,
    }));
    let root = server_view.root.iter().map(|card| (card.position, InstalledCard {
        card: card.card.clone().unwrap_or_else(|| pools.corp_root.draw()),
        // Carried, never reallocated — see the `ice` arm above.
        install_id: card.install_id,
        server: card.server,
        slot: InstallSlot::Root,
        rezzed: card.rezzed,
        advancement_tokens: card.advancement_tokens,
        // `None` is genuine ignorance, not a drop: the view masks an
        // unrezzed Corp card's counters because they would leak its
        // identity, so 0 is the honest sample. When they *are* visible,
        // carry them — zeroing a rezzed card's counters made its
        // counter-costed abilities illegal in the sample.
        counters: card.counters.unwrap_or(0),
        // `ClientView` doesn't carry install timing; a rollout re-derives it
        // from its own play-out, same approximation as `counters` above.
        installed_this_turn: false,
    }));
    ice.chain(root).collect()
}

fn determinize_zone(cards: &Option<Vec<CardId>>, count: usize, pool: &mut Pool) -> Vec<CardId> {
    match cards {
        Some(cards) => cards.clone(),
        None => pool.draw_n(count),
    }
}

fn determinize_access_cards(zone: &MaskedZone, pool: &mut Pool) -> Vec<CardId> {
    match zone {
        MaskedZone::Visible(cards) => cards.clone(),
        MaskedZone::Hidden { count } => pool.draw_n(*count as usize),
    }
}

fn determinize_access_phase(phase: &PublicAccessPhase, pool: &mut Pool) -> AccessPhase {
    match phase {
        PublicAccessPhase::SelectNextCard { selectable_cards } => {
            AccessPhase::SelectNextCard { selectable_cards: determinize_access_cards(selectable_cards, pool) }
        }
        // `decider` copies straight through rather than being re-derived
        // from the sampled card: it is public information, and a search tree
        // that disagreed with reality about whose decision is pending would
        // evaluate the position for the wrong player entirely.
        PublicAccessPhase::PendingInteractiveTrigger { card, cost, decider, can_pay } => AccessPhase::PendingInteractiveTrigger {
            card_id: card.clone().unwrap_or_else(|| pool.draw()),
            cost: cost.clone(),
            decider: *decider,
            can_pay: *can_pay,
        },
        PublicAccessPhase::PendingChoice { card, trash_cost, mandatory_steal, steal_cost } => AccessPhase::PendingChoice {
            card_id: card.clone().unwrap_or_else(|| pool.draw()),
            trash_cost: *trash_cost,
            mandatory_steal: *mandatory_steal,
            steal_cost: steal_cost.clone(),
        },
    }
}

/// `installed` is the already-sampled `corp.installed`: a masked `RunIce`
/// takes whatever card was drawn for the same `InstallId` there rather than
/// drawing again. The engine now keeps `run.ice` in step with
/// `corp.installed` (`run::reconcile_ice`), so a sample in which the two
/// disagree about one install is a state the real game cannot be in.
fn determinize_run(
    run: &netrunner_core::rules::PublicRunState,
    registry: &CardRegistry,
    pools: &mut Pools,
    installed: &[InstalledCard],
) -> RunState {
    let ice = run
        .ice
        .iter()
        .map(|ice| match &ice.identity {
            Some(identity) => RunIce {
                install_id: ice.install_id,
                card_id: identity.card.clone(),
                current_strength: identity.current_strength,
                ice_type: identity.ice_type,
                subroutines: identity.subroutines.clone(),
                rezzed: ice.rezzed,
            },
            None => {
                let card_id = installed
                    .iter()
                    .find(|c| c.install_id == ice.install_id)
                    .map(|c| c.card.clone())
                    .unwrap_or_else(|| pools.corp_ice.draw());
                let strength = registry.get(&card_id).and_then(|c| c.strength).unwrap_or(0);
                RunIce {
                    install_id: ice.install_id,
                    card_id,
                    current_strength: strength,
                    ice_type: netrunner_core::dsl::IceType::Barrier,
                    subroutines: Vec::new(),
                    rezzed: ice.rezzed,
                }
            }
        })
        .collect();

    let access_state = run.access_state.as_ref().map(|access| AccessState {
        server: access.server,
        // Internal bookkeeping for the window in which a card's own
        // `OnAccessed` trigger runs; never surfaced in `ClientView`, and a
        // rollout re-derives it from its own play-out.
        currently_accessing: None,
        unaccessed_cards: determinize_access_cards(&access.unaccessed_cards, &mut pools.corp_any),
        resolved_cards: determinize_access_cards(&access.resolved_cards, &mut pools.corp_any),
        phase: determinize_access_phase(&access.phase, &mut pools.corp_any),
    });

    RunState {
        server: run.server,
        phase: run.phase,
        ice,
        position: run.position,
        access_state,
        jack_out_permitted: run.jack_out_permitted,
        // Public and carried by the view — see `PublicRunState`. Zeroing
        // these made the sample poorer than the information the searcher
        // actually has: an action the Runner can really pay for out of
        // Bad Publicity looked unaffordable, and a barred steal/trash
        // looked permitted, in both cases deleting candidate actions.
        bad_publicity_credits: run.bad_publicity_credits,
        bonus_run_credits: run.bonus_run_credits,
        runner_cannot_steal_or_trash: run.runner_cannot_steal_or_trash,
        additional_rd_access: 0,
        additional_hq_access: 0,
        access_replacement: None, cards_accessed_count: 0, ice_rez_cost_modifier: 0,
        // Approximated, like `cards_accessed_count`/`bad_publicity_credits`
        // above: `ClientView` doesn't carry either, and both only matter at
        // the moment the run ends (`Trigger::OnRunEnded`), which a
        // determinized mid-run rollout re-derives from its own play-out
        // rather than from the sampled starting point.
        agendas_stolen_this_run: 0,
        persistent_trashed_upgrades: Vec::new(),
        on_success_effect: None,
    }
}

pub fn determinize(view: &ClientView, registry: &CardRegistry, rng: &mut impl Rng) -> GameState {
    let mut pools = build_pools(view, registry, rng);

    // Reassembled in the **real** install order, not the view's
    // server-grouped one. `ServerView` groups installs by server, so
    // chaining the groups produces a different `corp.installed` ordering
    // than the state the view was built from — and both
    // `pending_choice::zone_card_ids` and `ActionSpace`'s installed-card
    // segments index by exactly that ordering. A sample that disagreed
    // about it made the caller's own `ToggleCardSelection { position }`
    // decode to a different card, which `HeuristicAgent` then found
    // illegal, scored nothing, and fell back out of — livelocking on
    // `legal_actions[0]` until the step budget ran out (sweep seed 40,
    // `discretion_advised vs planning_ahead`, on Tāo Salonga's swap).
    let mut placed: Vec<(usize, InstalledCard)> = Vec::new();
    for server in &view.corp.servers {
        placed.extend(determinize_installed(server, &mut pools));
    }
    placed.sort_by_key(|(position, _)| *position);
    let installed: Vec<InstalledCard> = placed.into_iter().map(|(_, card)| card).collect();

    let corp = CorpState {
        identity: None,
        bad_publicity: view.corp.bad_publicity,
        first_install_used_this_turn: false,
        // Public (visible tokens on the granting card) and carried by the
        // view; zeroing them narrowed the Corp's affordable actions and
        // trace-bid range in the sample.
        recurring_credits: view.corp.recurring_credits,
        recurring_credits_max: view.corp.recurring_credits_max,
        agenda_points_scored_this_turn: 0, max_hand_size_bonus: 0, cannot_score_agendas_this_turn: false, removed_from_game: Vec::new(), once_per_turn_used: std::collections::HashSet::new(),
        scored_agendas: view.corp.scored_agendas.clone(),
        resources: PlayerResources {
            credits: Credits(view.corp.credits),
            clicks: Clicks(view.corp.clicks),
            agenda_points: AgendaPoints(view.corp.agenda_points),
        },
        hq: determinize_zone(&view.corp.hq_cards, view.corp.hq_count, &mut pools.corp_any),
        r_and_d: pools.corp_any.draw_n(view.corp.rd_count),
        archives: view
            .corp
            .archives
            .iter()
            .map(|archived| match &archived.card {
                Some(card) => ArchivedCard { card: card.clone(), facedown: archived.facedown },
                // Facedown and hidden from this viewer: the count and
                // orientation are known, the identity is not, so draw a
                // plausible one from the same pool every other hidden zone
                // samples from.
                None => ArchivedCard { card: pools.corp_any.draw(), facedown: true },
            })
            .collect(),
        installed,
    };

    let rig = view
        .runner
        .rig
        .iter()
        .map(|card| InstalledRunnerCard {
            card: card.card.clone(),
            // Carried, never reallocated — see `determinize_installed`.
            install_id: card.install_id,
            base_strength: card.current_strength,
            encounter_strength_buff: 0,
            turn_strength_buff: 0,
            // Both public and both carried by the view. `counters` was
            // simply being dropped, which made every counter-costed
            // ability (Botulus, Leech, Pennyshaver) illegal in the sample;
            // `hosted_on_ice` likewise, which made
            // `EffectRequirement::EncounteringHostIce` fail
            // unconditionally and so put every Trojan ability out of the
            // search's reach entirely.
            counters: card.counters,
            hosted_on_ice: card.hosted_on_ice,
        })
        .collect();

    let runner = RunnerState {
        identity: None,
        scored_agendas: view.runner.scored_agendas.clone(),
        resources: PlayerResources {
            credits: Credits(view.runner.credits),
            clicks: Clicks(view.runner.clicks),
            agenda_points: AgendaPoints(view.runner.agenda_points),
        },
        memory_units: MemoryUnits(view.runner.memory_units),
        brain_damage: view.runner.brain_damage,
        tags: view.runner.tags,
        grip: determinize_zone(&view.runner.grip_cards, view.runner.grip_count, &mut pools.runner_any),
        stack: pools.runner_any.draw_n(view.runner.stack_count),
        rig,
        heap: view.runner.heap.clone(),
        link_strength: view.runner.link_strength,
        first_hq_run_used_this_turn: false,
        first_install_discount_used_this_turn: false, once_per_turn_used: std::collections::HashSet::new(), made_successful_run_this_turn: false, made_successful_run_last_turn: false, max_hand_size_bonus: 0,
    };

    let active_run = view.active_run.as_ref().map(|run| determinize_run(run, registry, &mut pools, &corp.installed));

    // A rollout installs cards of its own, and those ids must not collide
    // with one the view already carries. The real counter isn't in the
    // view — nothing needs it — so start just past the highest id sampled,
    // which is the honest lower bound for it.
    let next_install_id = corp
        .installed
        .iter()
        .map(|c| c.install_id.0)
        .chain(runner.rig.iter().map(|c| c.install_id.0))
        .max()
        .map_or(1, |highest| highest + 1);

    GameState {
        corp,
        runner,
        phase: view.phase,
        // Public information, so it comes straight off the view rather than
        // being resampled — a determinized state that disagreed with the
        // real one about the turn number would mis-evaluate any "on turn N"
        // effect the search looks ahead through.
        turn: view.turn,
        active_run,
        paid_ability_window: view.paid_ability_window.clone(),
        active_trace: view.active_trace.clone(),
        pending_prevention: view.pending_prevention.clone(),
        pending_paid_choice: view.pending_paid_choice.clone(),
        pending_decision: view.pending_decision.clone(),
        // No masked representation to reconstruct these from (see their doc
        // comments on `GameState`), so a determinized hypothetical starts
        // them blank, same as a fresh `GameState::new`. Two former siblings
        // here — `last_discarded_cards` and `last_advancement_was_first` —
        // are gone entirely: they were transient resolution state and now
        // live on `ability::ResolutionContext`, which a determinized state
        // has no business carrying at all.
        last_completed_run: None,
        deferred_triggers: Vec::new(),
        seed: rng.random(),
        rng_step: 0,
        next_install_id,
    }
}

// `reseat_selectable_cards` used to sit here, and is deliberately gone.
//
// It repaired a parked `PendingDecision::ChooseCards` over a resampled
// hidden zone: `ToggleCardSelection` named its target by `CardId`, so
// resampling R&D or the stack could delete the very card the caller's
// legal action pointed at, leaving `PuctAgent::search` with no action that
// decoded against the sampled root at all.
//
// `ToggleCardSelection` now carries a *position*, and determinization
// preserves every zone's length (counts are public — see
// `determinize_zone`), so a position the caller may submit always
// addresses some card in the sample. There is nothing left to re-seat.
//
// What remains is narrower, and needs a `CardRegistry` this function never
// took: the card sampled *at* that position may not match the decision's
// `CardFilter`, so the sample can still judge the action illegal. That is a
// search-quality gap rather than a crash — `PuctAgent::search` seeds its
// root from `view.legal_actions` and values such a branch as a dead end.
// Measure it before building anything to close it.

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardType, IceType};
    use netrunner_core::rules::{
        AgendaPoints as AP, Clicks as C, CorpState as CS, Credits as Cr, GamePhase, GameState as CoreGameState,
        InstallSlot as CoreInstallSlot, InstalledCard, InstalledRunnerCard, MemoryUnits as MU, PlayerResources as PR,
        RunnerState as RS,
    };
    use netrunner_core::view::build_client_view;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn blank_card(id: &str, side: Side, card_type: CardType) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type,
            strength: Some(2),
            is_playable: true,
            ..Default::default()
        }
    }

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        for i in 0..5 {
            registry.insert(blank_card(&format!("corp_ice_{i}"), Side::Corp, CardType::Ice(IceType::Barrier)));
            registry.insert(blank_card(&format!("corp_asset_{i}"), Side::Corp, CardType::Asset));
            registry.insert(blank_card(&format!("runner_card_{i}"), Side::Runner, CardType::Event));
        }
        registry.insert(blank_card("hedge_fund", Side::Corp, CardType::Operation));
        registry.insert(blank_card("sure_gamble", Side::Runner, CardType::Event));
        registry
    }

    fn state_with_hidden_zones() -> CoreGameState {
        CoreGameState {
            corp: CS {
                identity: None,
                bad_publicity: 0,
                first_install_used_this_turn: false,
                recurring_credits: 0,
                recurring_credits_max: 0, agenda_points_scored_this_turn: 0, max_hand_size_bonus: 0, cannot_score_agendas_this_turn: false, removed_from_game: Vec::new(), once_per_turn_used: std::collections::HashSet::new(),
                scored_agendas: Vec::new(),
                resources: PR { credits: Cr(5), clicks: C(3), agenda_points: AP(0) },
                hq: vec![CardId("hedge_fund".to_string())],
                r_and_d: vec![CardId("corp_asset_0".to_string()), CardId("corp_asset_1".to_string())],
                archives: Vec::new(),
                installed: vec![InstalledCard {
                    card: CardId("corp_ice_0".to_string()),
                    slot: CoreInstallSlot::Ice,
                    ..Default::default()
                }],
            },
            runner: RS {
                identity: None,
                scored_agendas: Vec::new(),
                resources: PR { credits: Cr(5), clicks: C(4), agenda_points: AP(0) },
                memory_units: MU(4),
                brain_damage: 0,
                tags: 0,
                grip: vec![CardId("sure_gamble".to_string())],
                stack: vec![CardId("runner_card_0".to_string()), CardId("runner_card_1".to_string()), CardId("runner_card_2".to_string())],
                rig: vec![InstalledRunnerCard {
                    card: CardId("runner_card_3".to_string()),
                    base_strength: 2,
                    ..Default::default()
                }],
                heap: Vec::new(),
                link_strength: 0,
                first_hq_run_used_this_turn: false,
                first_install_discount_used_this_turn: false, once_per_turn_used: std::collections::HashSet::new(), made_successful_run_this_turn: false, made_successful_run_last_turn: false, max_hand_size_bonus: 0,
            },
            phase: GamePhase::Action(Side::Runner),
            seed: 1,
            ..Default::default()
        }
    }

    #[test]
    fn own_hand_is_reproduced_exactly() {
        let state = state_with_hidden_zones();
        let registry = registry();
        let view = build_client_view(&state, &registry, Side::Runner);
        let mut rng = StdRng::seed_from_u64(1);

        let sample = determinize(&view, &registry, &mut rng);
        assert_eq!(sample.runner.grip, vec![CardId("sure_gamble".to_string())]);
    }

    #[test]
    fn hidden_zone_sizes_match_the_view() {
        let state = state_with_hidden_zones();
        let registry = registry();
        let view = build_client_view(&state, &registry, Side::Runner);
        let mut rng = StdRng::seed_from_u64(2);

        let sample = determinize(&view, &registry, &mut rng);
        assert_eq!(sample.corp.hq.len(), view.corp.hq_count);
        assert_eq!(sample.corp.r_and_d.len(), view.corp.rd_count);
        assert_eq!(sample.corp.installed.len(), 1);
    }

    #[test]
    fn different_rng_states_sample_different_hidden_cards() {
        let state = state_with_hidden_zones();
        let registry = registry();
        let view = build_client_view(&state, &registry, Side::Runner);

        let mut rng_a = StdRng::seed_from_u64(10);
        let mut rng_b = StdRng::seed_from_u64(20);
        let sample_a = determinize(&view, &registry, &mut rng_a);
        let sample_b = determinize(&view, &registry, &mut rng_b);

        assert_ne!(sample_a.corp.hq, sample_b.corp.hq);
    }

    #[test]
    fn sampled_hidden_cards_come_from_the_correct_side_and_type_pool() {
        let state = state_with_hidden_zones();
        let registry = registry();
        let view = build_client_view(&state, &registry, Side::Runner);
        let mut rng = StdRng::seed_from_u64(3);

        let sample = determinize(&view, &registry, &mut rng);
        for card_id in &sample.corp.hq {
            assert_eq!(registry.get(card_id).unwrap().side, Side::Corp);
        }
        for card_id in &sample.runner.stack {
            assert_eq!(registry.get(card_id).unwrap().side, Side::Runner);
        }
        let ice = &sample.corp.installed[0];
        assert!(matches!(registry.get(&ice.card).unwrap().card_type, CardType::Ice(_)));
    }

    /// A parked card-selection's targets must stay *addressable* after
    /// determinization.
    ///
    /// The stack is resampled from a pool because its contents are hidden.
    /// When `ToggleCardSelection` named its target by `CardId`, resampling
    /// deleted the very card the caller's legal action pointed at, leaving
    /// a root state where none of those actions decoded — `PuctAgent::
    /// search` returned an empty action list and self-play panicked. That
    /// is what `reseat_selectable_cards` existed to patch up.
    ///
    /// The action now carries a position, and zone *counts* are public and
    /// preserved, so every offered position still addresses some card in
    /// the sample. This asserts that directly, which is why the repair
    /// function is gone rather than merely unused.
    ///
    /// Note what is deliberately *not* asserted: that the sampled card at
    /// that position matches the decision's `CardFilter`. It generally will
    /// not — the stack is resampled — so the sample may still judge the
    /// action illegal. That is a search-quality gap, not a crash, and it is
    /// the residual documented where `reseat_selectable_cards` used to be.
    #[test]
    fn a_parked_selections_targets_stay_addressable_after_determinization() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision, PlayerAction};

        let mut registry = netrunner_core::cards::CardRegistry::new();
        // A pool of decoys the sampler would otherwise fill the stack with.
        for index in 0..12 {
            registry.insert(blank_card(&format!("decoy_{index}"), Side::Runner, CardType::Event));
        }
        let target = CardId("target_breaker".to_string());
        registry.insert(blank_card(&target.0, Side::Runner, CardType::Program));

        let mut state = CoreGameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.stack = vec![target.clone(); 1];
        state.runner.stack.extend((0..9).map(|i| CardId(format!("decoy_{i}"))));
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Runner,
            source: CardZoneRef::OwnStack,
            filter: CardFilter::Icebreaker,
            min: 1,
            max: 1,
            reveal: true,
            shuffle_after: true,
            destination: Some(CardZoneRef::OwnGrip),
            then: None,
            selected: Vec::new(),
            source_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });

        let view = build_client_view(&state, &registry, Side::Runner);
        let offered: Vec<usize> = view
            .legal_actions
            .iter()
            .filter_map(|a| match a {
                PlayerAction::ToggleCardSelection { position } => Some(*position),
                _ => None,
            })
            .collect();
        assert_eq!(offered, vec![0], "the breaker sits at position 0 and is the only eligible target");
        // The breaker's identity is the Runner's own to see, but the
        // action still does not carry it.
        let _ = &target;

        // Many samples: every offered position must address a card in all
        // of them, not most.
        for seed in 0..25u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let sampled = determinize(&view, &registry, &mut rng);
            assert_eq!(
                sampled.runner.stack.len(),
                state.runner.stack.len(),
                "seed {seed}: zone size is public and must not change"
            );
            for position in &offered {
                assert!(
                    sampled.runner.stack.get(*position).is_some(),
                    "seed {seed}: offered position {position} addresses nothing in the sample"
                );
            }
        }
    }

    /// Public state the view carries must survive sampling.
    ///
    /// Each of these was previously hard-coded to zero/`None` here, which
    /// is not a neutral approximation: every one of them gates a `Cost` or
    /// an `EffectRequirement`, so zeroing them makes actions the searcher
    /// can really take illegal *in the sample*, and the search then never
    /// considers them. `hosted_on_ice` was the worst — `None` fails
    /// `EncounteringHostIce` unconditionally, putting every Trojan's
    /// ability permanently out of reach.
    #[test]
    fn public_run_and_card_state_survives_determinization() {
        let mut registry = registry();
        registry.insert(blank_card("botulus", Side::Runner, CardType::Program));

        let mut state = CoreGameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.recurring_credits = 2;
        state.corp.recurring_credits_max = 3;
        state.corp.installed = vec![InstalledCard {
            card: CardId("corp_ice_0".to_string()),
            server: netrunner_core::rules::ServerId::Remote(0),
            slot: CoreInstallSlot::Ice,
            rezzed: true,
            counters: 4,
            ..Default::default()
        }];
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("botulus".to_string()),
            base_strength: 2,
            counters: 3,
            hosted_on_ice: Some(netrunner_core::rules::InstallId::PLACEHOLDER),
            ..Default::default()
        }];
        state.active_run = Some(netrunner_core::rules::RunState {
            server: netrunner_core::rules::ServerId::Remote(0),
            phase: netrunner_core::rules::RunPhase::Initiation,
            bad_publicity_credits: 2,
            bonus_run_credits: 3,
            runner_cannot_steal_or_trash: true,
            ..Default::default()
        });

        for side in [Side::Corp, Side::Runner] {
            let view = build_client_view(&state, &registry, side);
            let mut rng = StdRng::seed_from_u64(7);
            let sampled = determinize(&view, &registry, &mut rng);

            let run = sampled.active_run.as_ref().expect("the run survives");
            assert_eq!(run.bad_publicity_credits, 2, "{side:?}");
            assert_eq!(run.bonus_run_credits, 3, "{side:?}");
            assert!(run.runner_cannot_steal_or_trash, "{side:?}");

            assert_eq!(sampled.runner.rig[0].counters, 3, "{side:?}");
            assert_eq!(
                sampled.runner.rig[0].hosted_on_ice,
                Some(netrunner_core::rules::InstallId::PLACEHOLDER),
                "{side:?}: a Trojan's host is public"
            );

            assert_eq!(sampled.corp.recurring_credits, 2, "{side:?}");
            assert_eq!(sampled.corp.recurring_credits_max, 3, "{side:?}");
            // Rezzed, so its counters are visible to both sides.
            assert_eq!(sampled.corp.installed[0].counters, 4, "{side:?}");
        }
    }

    /// The counterpart: an *unrezzed* Corp card's counters are masked
    /// precisely so they cannot leak its identity, so the sample must not
    /// invent them. `0` here is honest ignorance, not a dropped field.
    #[test]
    fn an_unrezzed_corp_cards_counters_stay_hidden_from_the_runner() {
        let registry = registry();
        let mut state = CoreGameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.installed = vec![InstalledCard {
            card: CardId("corp_asset_0".to_string()),
            server: netrunner_core::rules::ServerId::Remote(0),
            slot: CoreInstallSlot::Root,
            rezzed: false,
            counters: 5,
            ..Default::default()
        }];

        let view = build_client_view(&state, &registry, Side::Runner);
        let mut rng = StdRng::seed_from_u64(7);
        let sampled = determinize(&view, &registry, &mut rng);
        assert_eq!(sampled.corp.installed[0].counters, 0);

        let corp_view = build_client_view(&state, &registry, Side::Corp);
        let mut rng = StdRng::seed_from_u64(7);
        let sampled = determinize(&corp_view, &registry, &mut rng);
        assert_eq!(sampled.corp.installed[0].counters, 5, "the owner sees its own counters");
    }
}
