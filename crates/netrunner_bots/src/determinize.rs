//! Samples one concrete, `ClientView`-consistent `GameState` — the "I" in
//! Information Set MCTS: `MctsAgent` runs its unchanged Phase-1 search
//! machinery against a sampled state instead of the real (unavailable) one,
//! `HeuristicAgent` does the same for its one-ply lookahead, and `PuctAgent`
//! searches one sample and redraws it at every breach.
//!
//! **Hidden slots are filled from the decklist when one is known.** An
//! identity is public (`ClientView::corp.identity`), and the published
//! decklists are embedded (`netrunner_core::decks`), so when a seat's
//! identity matches one, the pool for its hidden zones is that list as a
//! multiset minus every copy already visible somewhere in the view: the
//! forty-odd cards the opponent actually brought, with the right number of
//! each, rather than one of everything ever printed. When two published
//! lists share an identity, the one that explains the most visible cards
//! wins the sample, then the one whose size matches the cards on the
//! table; with nothing seen yet they tie and the first by id is taken.
//! The viewer's *own* hidden deck is the same computation, and for it the
//! answer is exact up to order.
//!
//! What this bought, measured (ROADMAP Phase 3 §1, September 2026): a
//! sample that is a permutation of the real deck, and the removal of a
//! confound — the registry pool was a quarter agendas, and every
//! expectation-based valuation of a breach had been blamed on it. Not
//! strength: the PUCT Runner against the fixed heuristic Corp moved
//! 0.516 → 0.500 (192 games, inside the band) and heuristic-vs-heuristic
//! 0.417 → 0.417 on the same seating; and the breach chance node, re-run
//! over the honest pool, still lost to a single committed sample (0.432 /
//! 0.453 / 0.448 at 2 / 4 / 8 outcomes against 0.500). Whatever keeps the
//! search from valuing a breach in expectation, it is not the pool.
//!
//! **Without a matching decklist** — a homebrew deck, or a test fixture
//! with no identity — the pool falls back to the full `CardRegistry` for
//! the right side (type-constrained: an unrezzed ICE slot only ever draws
//! an ICE-typed card, a root slot only a card that can sit in a root),
//! minus every card id already visible. The fallback also takes over when
//! a decklist runs out before the hidden slots do, which is the sample's
//! way of saying the guess was wrong. Neither path models anything the
//! view does not say: no draw order, no "they kept two ICE in hand".

use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

use rand::seq::SliceRandom;
use rand::Rng;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::rules::{
    ArchivedCard,
    AccessPhase, AccessState, AgendaPoints, Clicks, CorpState, Credits, GameState, InstallSlot, InstalledCard,
    InstalledRunnerCard, MaskedZone, MemoryUnits, PlayerResources, PublicAccessPhase,
    RunIce, RunState, RunnerState, Side,
};
use netrunner_core::view::ClientView;

/// A shuffled draw pool that cycles once exhausted (draws-with-replacement
/// across repeated full passes) — matches "shuffle then keep drawing" for
/// however many hidden slots need filling, however many that is. This is
/// the **registry fallback**; a known decklist (`Pools::corp_deck`,
/// `Pools::runner_deck`) is drawn first and is consumed, not cycled.
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
}

/// Which hidden slot a draw fills. The view says what *kind* of install
/// sits in a slot even when it hides which card, so an ICE slot only ever
/// takes an ICE-typed card and a root slot only a card that can be
/// installed in a root — an Agenda, an Asset or an Upgrade. Upgrades were
/// left out until the decklist pool landed, which biased every unrezzed
/// remote card toward being an agenda; against a real list the bias was
/// the difference between "a Manegarm Skunkworks" and "an agenda".
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    CorpAny,
    CorpIce,
    CorpRoot,
    RunnerAny,
}

impl Slot {
    fn side(self) -> Side {
        match self {
            Slot::CorpAny | Slot::CorpIce | Slot::CorpRoot => Side::Corp,
            Slot::RunnerAny => Side::Runner,
        }
    }

    fn admits(self, card: &CardDefinition) -> bool {
        match self {
            Slot::CorpAny | Slot::RunnerAny => true,
            Slot::CorpIce => matches!(card.card_type, CardType::Ice(_)),
            Slot::CorpRoot => matches!(card.card_type, CardType::Agenda | CardType::Asset | CardType::Upgrade),
        }
    }
}

/// The two tiers of draw pool for one determinization: a side's remaining
/// decklist when its identity names a published one, and the cycling
/// registry pools behind it.
struct Pools<'a> {
    registry: &'a CardRegistry,
    /// Each side's decklist minus the copies the view already shows,
    /// shuffled. Empty when no published list matches the identity, and
    /// emptied as slots consume it.
    corp_deck: Vec<CardId>,
    runner_deck: Vec<CardId>,
    corp_any: Pool,
    corp_ice: Pool,
    corp_root: Pool,
    runner_any: Pool,
}

impl Pools<'_> {
    /// The first remaining decklist card the slot admits, else a registry
    /// draw. Taking the first admissible card of an already-shuffled list
    /// is a uniform draw from the admissible ones, and removing it is what
    /// makes the sample a permutation of the deck rather than a bag of
    /// guesses: a card drawn into R&D is not also behind an unrezzed ICE.
    fn draw(&mut self, slot: Slot) -> CardId {
        let registry = self.registry;
        let deck = match slot.side() {
            Side::Corp => &mut self.corp_deck,
            Side::Runner => &mut self.runner_deck,
        };
        if let Some(index) = deck.iter().position(|id| registry.get(id).is_some_and(|card| slot.admits(card))) {
            return deck.remove(index);
        }
        match slot {
            Slot::CorpAny => self.corp_any.draw(),
            Slot::CorpIce => self.corp_ice.draw(),
            Slot::CorpRoot => self.corp_root.draw(),
            Slot::RunnerAny => self.runner_any.draw(),
        }
    }

    fn draw_n(&mut self, slot: Slot, n: usize) -> Vec<CardId> {
        (0..n).map(|_| self.draw(slot)).collect()
    }
}

/// Every card the view shows the identity of, **one entry per copy** —
/// a multiset, because the decklist pool subtracts copies, not ids. Each
/// physical location is walked once: a rezzed ICE appears in its server's
/// `ice`, so the run's copy of it is not counted again, and an accessed
/// card the viewer can see is counted from the access state only when its
/// zone (HQ, R&D) is hidden to them.
fn visible_cards(view: &ClientView) -> Vec<CardId> {
    let mut ids = Vec::new();
    if let Some(cards) = &view.corp.hq_cards {
        ids.extend(cards.iter().cloned());
    }
    // Only Archives cards whose identity this viewer can actually see —
    // a facedown card is hidden from the Runner (`PublicArchivedCard::card`
    // is `None`), so it must be sampled from the pool like any other
    // unknown, not treated as already-visible.
    ids.extend(view.corp.archives.iter().filter_map(|a| a.card.clone()));
    ids.extend(view.corp.scored_agendas.iter().map(|scored| scored.card.clone()));
    ids.extend(view.corp.removed_from_game.iter().cloned());
    for server in &view.corp.servers {
        for card in server.ice.iter().chain(server.root.iter()) {
            if let Some(id) = &card.card {
                ids.push(id.clone());
            }
        }
    }

    if let Some(cards) = &view.runner.grip_cards {
        ids.extend(cards.iter().cloned());
    }
    ids.extend(view.runner.heap.iter().cloned());
    ids.extend(view.runner.scored_agendas.iter().cloned());
    for rig_card in &view.runner.rig {
        ids.push(rig_card.card.clone());
        ids.extend(rig_card.hosted_cards.iter().cloned());
    }

    // An access shows the Runner cards out of a zone the view otherwise
    // hides from them (HQ, R&D); the Corp already counted them from its
    // own hand. Archives accesses are faceup cards counted above.
    if let Some(access) = view.active_run.as_ref().and_then(|run| run.access_state.as_ref())
        && view.corp.hq_cards.is_none()
        && !matches!(access.server, netrunner_core::rules::ServerId::Archives)
    {
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
                ids.push(id.clone());
            }
            _ => {}
        }
    }

    ids
}

/// The embedded decklists, parsed once: `determinize` runs on every
/// decision and `resample_hidden` on every breach outcome, and
/// `decks::embedded_decks` parses JSON each call.
fn embedded_decks() -> &'static [DeckFile] {
    static DECKS: OnceLock<Vec<DeckFile>> = OnceLock::new();
    DECKS.get_or_init(netrunner_core::decks::embedded_decks)
}

/// How many cards of `side` the view can account for, hidden or not — the
/// deck's size, if every card of it is still in the game somewhere. A
/// preference between two lists sharing an identity, never a filter: a
/// card in a play area the view does not model is one off, and one off
/// must not throw away the right list.
fn cards_in_game(view: &ClientView, side: Side) -> usize {
    match side {
        Side::Corp => {
            view.corp.hq_count
                + view.corp.rd_count
                + view.corp.archives.len()
                + view.corp.servers.iter().map(|server| server.ice.len() + server.root.len()).sum::<usize>()
                + view.corp.scored_agendas.len()
                + view.runner.scored_agendas.len()
                + view.corp.removed_from_game.len()
        }
        Side::Runner => {
            view.runner.grip_count
                + view.runner.stack_count
                + view.runner.heap.len()
                + view.runner.rig.iter().map(|card| 1 + card.hosted_cards.len()).sum::<usize>()
        }
    }
}

/// The cards of `side` still unseen, if its identity names a published
/// decklist: that list as a multiset with one copy struck for every
/// visible copy of the card. Empty when nothing matches. Order is the
/// decklist's — the caller shuffles.
///
/// Several published lists share an identity (*Precision Design* has two
/// sample decks; a starter and its boosted form share theirs). The one
/// taken explains the most visible copies; a tie goes to the list whose
/// size is nearest the cards on the table, then to the first by id. That
/// is a maximum-likelihood pick over a two-element prior, not a posterior
/// over lists, and it is enough: the two lists sharing an identity
/// diverge early, on the first unique card seen.
fn remaining_decklist(side: Side, identity: Option<&CardId>, visible: &[CardId], view: &ClientView, registry: &CardRegistry) -> Vec<CardId> {
    let Some(identity) = identity else { return Vec::new() };
    let candidates = embedded_decks().iter().filter(|deck| deck.side == side && &deck.identity == identity);

    // Only this side's visible cards can count against its list; a Runner
    // card in the heap says nothing about which Corp list is in play.
    let mut seen: BTreeMap<&CardId, usize> = BTreeMap::new();
    for card in visible.iter().filter(|id| registry.get(id).is_some_and(|card| card.side == side)) {
        *seen.entry(card).or_insert(0) += 1;
    }
    let in_game = cards_in_game(view, side);

    let scored = candidates.map(|deck| {
        let explained: usize =
            deck.cards.iter().map(|entry| seen.get(&entry.card).map_or(0, |count| (*count).min(entry.count as usize))).sum();
        let size_gap = (deck.size() as usize).abs_diff(in_game);
        (explained, size_gap, deck)
    });
    let Some((_, _, deck)) = scored.min_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.id.cmp(&b.2.id))) else {
        return Vec::new();
    };

    let mut remaining = Vec::with_capacity(deck.size() as usize);
    for entry in &deck.cards {
        // A card the registry does not know cannot be played by the
        // sample either — a homebrew registry that lacks a published card
        // gets one fewer copy rather than an unplayable id.
        if registry.get(&entry.card).is_none() {
            continue;
        }
        let struck = seen.get(&entry.card).copied().unwrap_or(0).min(entry.count as usize);
        remaining.extend(std::iter::repeat_n(entry.card.clone(), entry.count as usize - struck));
    }
    remaining
}

/// Builds the per-side draw pools: each side's remaining decklist when
/// known (`remaining_decklist`), and the registry fallback behind it.
///
/// The registry candidates are **sorted by `CardId` before `Pool::new`
/// shuffles them.** `CardRegistry::iter` is `HashMap::values()`, whose
/// order differs from one process to the next, and a seeded shuffle only
/// reproduces its output over an identical input — so without the sort
/// the same seed sampled different hidden cards on every run, and every
/// determinizing bot (heuristic, MCTS, PUCT) was nondeterministic run to
/// run. That made heuristic-vs-heuristic useless as a before/after
/// measurement (96 games at seed 1 differed from *themselves* in 262
/// report keys) and a seeded self-play or arena run unreproducible.
/// Sorting here rather than making `CardRegistry::iter` ordered keeps the
/// fix where the requirement lives; the registry's doc still promises no
/// order, and nothing else relies on one. A decklist is already ordered.
///
/// Identities are excluded from the registry pool: an identity is never
/// in a deck, so it can never be in a hidden zone, and a sample that put
/// *The Syndicate* in HQ or on top of the stack was imagining a card the
/// real game cannot hold there. The registry carries every side's
/// identities alongside its playable cards, so the pool has to say so
/// itself.
fn build_pools<'a>(view: &ClientView, registry: &'a CardRegistry, rng: &mut impl Rng) -> Pools<'a> {
    let visible_copies = visible_cards(view);
    let mut corp_deck = remaining_decklist(Side::Corp, view.corp.identity.as_ref(), &visible_copies, view, registry);
    let mut runner_deck = remaining_decklist(Side::Runner, view.runner.identity.as_ref(), &visible_copies, view, registry);
    corp_deck.shuffle(rng);
    runner_deck.shuffle(rng);

    let visible: HashSet<CardId> = visible_copies.into_iter().collect();
    let could_be_hidden = |c: &&CardDefinition| !matches!(c.card_type, CardType::Identity) && !visible.contains(&c.id);
    let mut corp_cards: Vec<&CardDefinition> = registry.iter().filter(|c| c.side == Side::Corp).filter(could_be_hidden).collect();
    corp_cards.sort_by(|a, b| a.id.cmp(&b.id));
    let mut runner_cards: Vec<CardId> =
        registry.iter().filter(|c| c.side == Side::Runner).filter(could_be_hidden).map(|c| c.id.clone()).collect();
    runner_cards.sort();

    let corp_any: Vec<CardId> = corp_cards.iter().map(|c| c.id.clone()).collect();
    let corp_ice: Vec<CardId> = corp_cards.iter().filter(|c| Slot::CorpIce.admits(c)).map(|c| c.id.clone()).collect();
    let corp_root: Vec<CardId> = corp_cards.iter().filter(|c| Slot::CorpRoot.admits(c)).map(|c| c.id.clone()).collect();

    Pools {
        registry,
        corp_deck,
        runner_deck,
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
    pools: &mut Pools<'_>,
) -> Vec<(usize, InstalledCard)> {
    // Drawn before the root cards, in server order: both slots consume
    // the same decklist, so the two maps cannot be lazily interleaved.
    let ice: Vec<(usize, InstalledCard)> = server_view.ice.iter().map(|card| (card.position, InstalledCard {
        card: card.card.clone().unwrap_or_else(|| pools.draw(Slot::CorpIce)),
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
    })).collect();
    let root = server_view.root.iter().map(|card| (card.position, InstalledCard {
        card: card.card.clone().unwrap_or_else(|| pools.draw(Slot::CorpRoot)),
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
    ice.into_iter().chain(root).collect()
}

fn determinize_zone(cards: &Option<Vec<CardId>>, count: usize, pools: &mut Pools<'_>, slot: Slot) -> Vec<CardId> {
    match cards {
        Some(cards) => cards.clone(),
        None => pools.draw_n(slot, count),
    }
}

fn determinize_access_cards(zone: &MaskedZone, pools: &mut Pools<'_>) -> Vec<CardId> {
    match zone {
        MaskedZone::Visible(cards) => cards.clone(),
        MaskedZone::Hidden { count } => pools.draw_n(Slot::CorpAny, *count as usize),
    }
}

fn determinize_access_phase(phase: &PublicAccessPhase, pools: &mut Pools<'_>) -> AccessPhase {
    match phase {
        PublicAccessPhase::SelectNextCard { selectable_cards } => {
            AccessPhase::SelectNextCard { selectable_cards: determinize_access_cards(selectable_cards, pools) }
        }
        // `decider` copies straight through rather than being re-derived
        // from the sampled card: it is public information, and a search tree
        // that disagreed with reality about whose decision is pending would
        // evaluate the position for the wrong player entirely.
        PublicAccessPhase::PendingInteractiveTrigger { card, cost, decider, can_pay } => AccessPhase::PendingInteractiveTrigger {
            card_id: card.clone().unwrap_or_else(|| pools.draw(Slot::CorpAny)),
            cost: cost.clone(),
            decider: *decider,
            can_pay: *can_pay,
        },
        PublicAccessPhase::PendingChoice { card, trash_cost, mandatory_steal, steal_cost } => AccessPhase::PendingChoice {
            card_id: card.clone().unwrap_or_else(|| pools.draw(Slot::CorpAny)),
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
    pools: &mut Pools<'_>,
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
                    .unwrap_or_else(|| pools.draw(Slot::CorpIce));
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
        // Public, and load-bearing for a rollout that trashes the pending
        // card: the sample must take the same instance the real game will.
        pending_install: access.pending_install,
        // Whether that install was rezzed when presented — the view carries
        // the install's rez state, so the sample can say so honestly.
        pending_install_rezzed: access
            .pending_install
            .and_then(|install| installed.iter().find(|c| c.install_id == install))
            .is_some_and(|card| card.rezzed),
        // Not in the view; a rollout that re-picks an already-resolved copy
        // takes the other one, which is harmless.
        resolved_installs: Vec::new(),
        unaccessed_cards: determinize_access_cards(&access.unaccessed_cards, pools),
        resolved_cards: determinize_access_cards(&access.resolved_cards, pools),
        phase: determinize_access_phase(&access.phase, pools),
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
        redirect_on_approach: run.redirect_on_approach,
        // Not in the view: a run's end rider, Shred's armed prevention and
        // whether a subroutine resolved are known to the seat that set
        // them, not carried. The determinized run neither fires a Charm
        // Offensive rider nor prevents its own end — a search-quality
        // limit, recorded in ROADMAP Phase 1 §8 Stage 3.
        on_end_effect: None,
        on_end_card: None,
        on_end_install: None,
        end_run_prevention: None,
        subroutine_resolved: false,
        // Not in the view either: which card started the run (Sang
        // Kancil's cheaper boost) and whether the encountered ice was
        // bypassed. A rollout starting mid-encounter sees no bypass, which
        // only ever makes the searcher pay for subroutines the real game
        // will not fire.
        initiated_by: None,
        ice_bypassed: false,
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
        // Not in the view either: a rollout only ever *starts* runs of its own,
        // so the rider's source is whatever it seeds itself.
        on_success_card: None,
        on_success_install: None,
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
        // Deliberately not `view.corp.identity`, though the view carries
        // it. With the identity set, every identity trigger fires in the
        // rollouts — which is the truth, and measured worse for the search
        // (PUCT Runner against the fixed heuristic Corp, 192 games): over
        // the registry pool both identities cost 0.516 → 0.406; over the
        // decklist pool the Corp's cost 0.500 → 0.495, the Runner's own
        // 0.484, both 0.469 — inside the band, negative every time, while
        // the one-ply heuristic Runner *gained* 0.417 → 0.448. A sample
        // that plays *Precision Design* as a blank card is the lesser
        // error until the search's loss is understood (ROADMAP Phase 3 §1).
        identity: None,
        bad_publicity: view.corp.bad_publicity,
        first_install_used_this_turn: false,
        // Public (visible tokens on the granting card) and carried by the
        // view; zeroing them narrowed the Corp's affordable actions and
        // trace-bid range in the sample.
        recurring_credits: view.corp.recurring_credits,
        recurring_credits_max: view.corp.recurring_credits_max,
        agenda_points_scored_this_turn: 0, max_hand_size_bonus: 0, cannot_score_agendas_this_turn: false, once_per_turn_used: std::collections::HashSet::new(),
        // Not carried by `ClientView`, and a rollout re-derives it from its
        // own play-out — the same approximation as `installed_this_turn`.
        extra_clicks_next_turn: 0,
        // Public on the identity, and carried straight off the view.
        identity_counters: view.corp.identity_counters,
        // Not carried by `ClientView`; a rollout re-derives it from its own
        // play-out, as with `installed_this_turn`.
        played_operation_this_turn: false,
        // Public, and carried: a flipped Corp identity has different text.
        identity_flipped: view.corp.identity_flipped,
        removed_from_game: view.corp.removed_from_game.clone(),
        scored_agendas: view.corp.scored_agendas.clone(),
        // The engine seeds this from the decklist; the registry-wide list
        // is equivalent, since it is only ever consulted for cards that
        // are actually in Archives.
        playable_from_archives: registry.iter().filter(|c| c.playable_from_archives).map(|c| c.id.clone()).collect(),
        resources: PlayerResources {
            credits: Credits(view.corp.credits),
            clicks: Clicks(view.corp.clicks),
            agenda_points: AgendaPoints(view.corp.agenda_points),
        },
        hq: determinize_zone(&view.corp.hq_cards, view.corp.hq_count, &mut pools, Slot::CorpAny),
        r_and_d: pools.draw_n(Slot::CorpAny, view.corp.rd_count),
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
                None => ArchivedCard { card: pools.draw(Slot::CorpAny), facedown: true },
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
            run_strength_buff: 0,
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
            hosted_on_program: card.hosted_on_program,
            hosted_cards: card.hosted_cards.clone(),
            hosted_cards_playable: card.hosted_cards_playable,
        })
        .collect();

    let runner = RunnerState {
        // Not carried — see `corp.identity` above.
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
        grip: determinize_zone(&view.runner.grip_cards, view.runner.grip_count, &mut pools, Slot::RunnerAny),
        stack: pools.draw_n(Slot::RunnerAny, view.runner.stack_count),
        rig,
        heap: view.runner.heap.clone(),
        link_strength: view.runner.link_strength,
        first_hq_run_used_this_turn: false,
        first_install_discount_used_this_turn: false, once_per_turn_used: std::collections::HashSet::new(),
        // Public and carried, like `servers_run_this_turn` beside it: the
        // evaluator's run term reads it, so a sample that forgot this
        // turn's success priced the next one as the first.
        made_successful_run_this_turn: view.runner.made_successful_run_this_turn,
        made_successful_run_last_turn: false, max_hand_size_bonus: 0, servers_run_this_turn: view.runner.servers_run_this_turn.clone(),
        discarded_this_discard_phase: view.runner.discarded_this_discard_phase.clone(),
        identity_flipped: view.runner.identity_flipped,
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
        actions_taken_this_turn: view.actions_taken_this_turn,
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
        // Public, and the win threshold the search plays toward.
        rules: view.rules,
    }
}

/// Re-draws, in place, every card of `state` that `view` hides from its
/// viewer — R&D and the Stack always, HQ and the Grip when they are the
/// opponent's, the Archives cards the viewer saw facedown, and the
/// identity of every installed card the view masks — keeping each zone's
/// length, every install's `InstallId`, and everything public exactly as
/// it is. Cards the sample has moved since it was drawn (a card the
/// rollout installed, an Archives entry it added) are not in the view and
/// are left alone.
///
/// The second half of determinization. `determinize` fixes one story
/// about the hidden cards for a whole search, which is the right thing
/// for an unrezzed ICE — the sample commits to it being *something* and
/// the search plays against that — but the wrong thing at a breach: the
/// card the Runner is about to access is, to the real Runner, a draw from
/// everything it might be, and a tree that resolves it to the one card the
/// sample happened to put there values the access as that card. `PuctAgent`
/// calls this on each outcome child of a `CompleteRun` edge so the edge's
/// value averages over the draw (`PuctConfig::breach_outcomes`).
pub fn resample_hidden(state: &mut GameState, view: &ClientView, registry: &CardRegistry, rng: &mut impl Rng) {
    let mut pools = build_pools(view, registry, rng);

    state.corp.r_and_d = pools.draw_n(Slot::CorpAny, state.corp.r_and_d.len());
    if view.corp.hq_cards.is_none() {
        state.corp.hq = pools.draw_n(Slot::CorpAny, state.corp.hq.len());
    }
    for (archived, seen) in state.corp.archives.iter_mut().zip(&view.corp.archives) {
        if seen.card.is_none() {
            archived.card = pools.draw(Slot::CorpAny);
        }
    }
    let masked: HashSet<_> = view
        .corp
        .servers
        .iter()
        .flat_map(|server| server.ice.iter().chain(server.root.iter()))
        .filter(|card| card.card.is_none())
        .map(|card| card.install_id)
        .collect();
    for installed in state.corp.installed.iter_mut().filter(|card| masked.contains(&card.install_id)) {
        installed.card = match installed.slot {
            InstallSlot::Ice => pools.draw(Slot::CorpIce),
            InstallSlot::Root => pools.draw(Slot::CorpRoot),
        };
    }

    state.runner.stack = pools.draw_n(Slot::RunnerAny, state.runner.stack.len());
    if view.runner.grip_cards.is_none() {
        state.runner.grip = pools.draw_n(Slot::RunnerAny, state.runner.grip.len());
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
                extra_clicks_next_turn: 0,
                identity_counters: 0,
                played_operation_this_turn: false,
                identity_flipped: false,
                bad_publicity: 0,
                first_install_used_this_turn: false,
                recurring_credits: 0,
                recurring_credits_max: 0, agenda_points_scored_this_turn: 0, max_hand_size_bonus: 0, cannot_score_agendas_this_turn: false, removed_from_game: Vec::new(), once_per_turn_used: std::collections::HashSet::new(),
                scored_agendas: Vec::new(),
                playable_from_archives: Vec::new(),
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
                first_install_discount_used_this_turn: false, once_per_turn_used: std::collections::HashSet::new(), made_successful_run_this_turn: false, made_successful_run_last_turn: false, max_hand_size_bonus: 0, servers_run_this_turn: Vec::new(), discarded_this_discard_phase: Vec::new(), identity_flipped: false,
            },
            phase: GamePhase::Action(Side::Runner),
            seed: 1,
            ..Default::default()
        }
    }


    fn multiset(cards: impl IntoIterator<Item = CardId>) -> std::collections::BTreeMap<CardId, usize> {
        let mut counts = std::collections::BTreeMap::new();
        for card in cards {
            *counts.entry(card).or_insert(0) += 1;
        }
        counts
    }

    fn decklist(id: &str) -> std::collections::BTreeMap<CardId, usize> {
        let deck = netrunner_core::decks::by_id(id).unwrap();
        multiset(deck.cards.iter().flat_map(|entry| std::iter::repeat_n(entry.card.clone(), entry.count as usize)))
    }

    /// Every Corp card the sample holds anywhere — hidden zones and public
    /// ones alike — so that it can be compared with the decklist as a
    /// whole.
    fn every_corp_card(state: &CoreGameState) -> std::collections::BTreeMap<CardId, usize> {
        multiset(
            state
                .corp
                .hq
                .iter()
                .chain(&state.corp.r_and_d)
                .chain(state.corp.archives.iter().map(|a| &a.card))
                .chain(state.corp.installed.iter().map(|c| &c.card))
                .chain(state.corp.scored_agendas.iter().map(|s| &s.card))
                .chain(&state.runner.scored_agendas)
                .chain(&state.corp.removed_from_game)
                .cloned(),
        )
    }

    fn playable_registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        registry
    }

    fn sample_game(corp: &str, runner: &str) -> (CoreGameState, CardRegistry) {
        let registry = playable_registry();
        let corp = netrunner_core::decks::by_id(corp).unwrap().to_deck();
        let runner = netrunner_core::decks::by_id(runner).unwrap().to_deck();
        let (state, _) = CoreGameState::setup(&corp, &runner, &registry, 5).unwrap();
        (state, registry)
    }

    /// The whole point of the decklist pool: a sample's hidden Corp cards
    /// plus the Corp cards the view shows are the Corp's decklist, exactly
    /// — one copy of each card for every copy the list has, whatever has
    /// been installed, discarded or stolen — and the same for the Runner's
    /// own stack, which the Runner cannot see either.
    #[test]
    fn a_known_decklist_partitions_exactly_into_the_sample() {
        let (mut state, registry) = sample_game("agency", "stolen_goods");
        let mut rng = StdRng::seed_from_u64(11);

        let view = build_client_view(&state, &registry, Side::Runner);
        let sample = determinize(&view, &registry, &mut rng);
        assert_eq!(every_corp_card(&sample), decklist("agency"), "at setup, HQ and R&D together are the list");
        assert_eq!(
            multiset(sample.runner.grip.iter().chain(&sample.runner.stack).cloned()),
            decklist("stolen_goods"),
            "the Runner's own stack is the list minus the grip"
        );

        // Mid-game: a rezzed install, a faceup discard and a stolen agenda
        // are all visible to the Runner, and each strikes one copy.
        let installed = state.corp.hq.remove(0);
        state.corp.installed.push(InstalledCard {
            card: installed,
            install_id: netrunner_core::rules::InstallId(7),
            server: netrunner_core::rules::ServerId::Remote(0),
            slot: CoreInstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        });
        let discarded = state.corp.r_and_d.remove(0);
        state.corp.archives.push(netrunner_core::rules::ArchivedCard { card: discarded, facedown: false });
        let stolen = state.corp.r_and_d.remove(0);
        state.runner.scored_agendas.push(stolen);

        let view = build_client_view(&state, &registry, Side::Runner);
        let sample = determinize(&view, &registry, &mut rng);
        assert_eq!(every_corp_card(&sample), decklist("agency"));
        assert_eq!(every_corp_card(&state), decklist("agency"), "and so does the real state, which is the premise");
    }

    /// The identities reach the sample's *pool* (the decklist is chosen
    /// by them) but not the sample's *state*: carrying them measured
    /// worse for the search — see the comment on `determinize`'s
    /// `identity: None`. This pins the decision so that re-enabling it is
    /// a deliberate, measured change rather than a drive-by.
    #[test]
    fn identities_choose_the_pool_but_are_not_carried_into_the_sample() {
        let (state, registry) = sample_game("agency", "stolen_goods");
        assert!(state.corp.identity.is_some() && state.runner.identity.is_some(), "the premise");
        for viewer in [Side::Corp, Side::Runner] {
            let view = build_client_view(&state, &registry, viewer);
            assert_eq!(view.corp.identity, state.corp.identity, "the view carries it");
            let sample = determinize(&view, &registry, &mut StdRng::seed_from_u64(1));
            assert_eq!(every_corp_card(&sample), decklist("agency"), "and the pool was chosen by it");
            assert_eq!(sample.corp.identity, None);
            assert_eq!(sample.runner.identity, None);
        }
    }

    /// Two published lists share *Precision Design*. With nothing visible
    /// they tie and the first by id is taken; the first card seen that
    /// only one of them plays decides it.
    #[test]
    fn two_lists_sharing_an_identity_are_told_apart_by_what_is_visible() {
        let (mut state, registry) = sample_game("discretion_advised", "stolen_goods");
        let mut rng = StdRng::seed_from_u64(2);

        let view = build_client_view(&state, &registry, Side::Runner);
        let sample = determinize(&view, &registry, &mut rng);
        assert_eq!(every_corp_card(&sample), decklist("brutal_efficiency"), "nothing seen: the tie goes to the first by id");

        // Nico Campaign is in Discretion Advised and not in Brutal
        // Efficiency. Faceup in Archives, it is visible to the Runner.
        let nico = CardId("nico_campaign".to_string());
        let position = state.corp.r_and_d.iter().position(|c| c == &nico).expect("the list plays it");
        let card = state.corp.r_and_d.remove(position);
        state.corp.archives.push(netrunner_core::rules::ArchivedCard { card, facedown: false });

        let view = build_client_view(&state, &registry, Side::Runner);
        let sample = determinize(&view, &registry, &mut rng);
        assert_eq!(every_corp_card(&sample), decklist("discretion_advised"));
    }

    /// A list that runs out before the hidden slots do — the opponent is
    /// not playing the list the identity suggests, or a rollout has drawn
    /// more than the real deck holds — hands the remaining slots to the
    /// registry rather than a placeholder or a panic.
    #[test]
    fn hidden_slots_beyond_the_decklist_fall_back_to_the_registry() {
        let (mut state, registry) = sample_game("agency", "stolen_goods");
        state.corp.r_and_d.extend(std::iter::repeat_n(CardId("hedge_fund".to_string()), 10));
        let view = build_client_view(&state, &registry, Side::Runner);
        let sample = determinize(&view, &registry, &mut StdRng::seed_from_u64(4));
        assert_eq!(sample.corp.r_and_d.len(), state.corp.r_and_d.len());
        assert!(sample.corp.r_and_d.iter().all(|card| registry.get(card).is_some_and(|c| c.side == Side::Corp)));
    }

    /// A root slot can hold an Upgrade as well as an Agenda or an Asset;
    /// leaving Upgrades out made every unrezzed remote card an agenda or an
    /// asset, which against a real list overstated the agenda density.
    #[test]
    fn a_root_slot_admits_upgrades() {
        assert!(Slot::CorpRoot.admits(&blank_card("grid", Side::Corp, CardType::Upgrade)));
        assert!(Slot::CorpRoot.admits(&blank_card("agenda", Side::Corp, CardType::Agenda)));
        assert!(!Slot::CorpRoot.admits(&blank_card("ice", Side::Corp, CardType::Ice(IceType::Barrier))));
        assert!(!Slot::CorpIce.admits(&blank_card("grid", Side::Corp, CardType::Upgrade)));
    }

    /// `resample_hidden` redraws only what the viewer cannot see and
    /// keeps every count, every public card and every `InstallId`.
    #[test]
    fn resample_hidden_redraws_hidden_zones_only_and_keeps_their_shape() {
        let registry = registry();
        let state = state_with_hidden_zones();
        let view = build_client_view(&state, &registry, Side::Runner);
        let mut rng = StdRng::seed_from_u64(3);
        let mut sample = determinize(&view, &registry, &mut rng);
        let before = sample.clone();

        resample_hidden(&mut sample, &view, &registry, &mut StdRng::seed_from_u64(11));

        assert_eq!(sample.corp.hq.len(), before.corp.hq.len());
        assert_eq!(sample.corp.r_and_d.len(), before.corp.r_and_d.len());
        assert_eq!(sample.runner.stack.len(), before.runner.stack.len());
        assert_eq!(sample.runner.grip, before.runner.grip, "the Runner's own grip is visible and kept");
        assert_eq!(sample.runner.rig, before.runner.rig);
        assert_eq!(
            sample.corp.installed.iter().map(|c| c.install_id).collect::<Vec<_>>(),
            before.corp.installed.iter().map(|c| c.install_id).collect::<Vec<_>>(),
            "install ids are public and never reallocated"
        );
        assert!(sample.corp.installed.iter().all(|c| registry.get(&c.card).is_some_and(|d| matches!(d.card_type, CardType::Ice(_)))));
        assert!(
            sample.corp.hq != before.corp.hq || sample.corp.r_and_d != before.corp.r_and_d || sample.runner.stack != before.runner.stack,
            "a different draw tells a different story about the hidden cards"
        );
        let mut again = before.clone();
        resample_hidden(&mut again, &view, &registry, &mut StdRng::seed_from_u64(11));
        assert_eq!(again, sample, "the redraw is a pure function of its seed");
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

    /// The pools are drawn from a `HashMap`, so two registries holding the
    /// same cards can enumerate them in different orders. The sample must
    /// not depend on that order: a seeded shuffle over an unordered input
    /// is exactly the nondeterminism that made every determinizing bot
    /// differ from itself run to run.
    #[test]
    fn the_same_seed_samples_the_same_hidden_cards_whatever_the_registry_order() {
        let forward = registry();
        let mut cards: Vec<CardDefinition> = forward.iter().cloned().collect();
        cards.sort_by(|a, b| a.id.cmp(&b.id));
        let ascending = CardRegistry::from_cards(cards.clone());
        cards.reverse();
        let descending = CardRegistry::from_cards(cards);

        let state = state_with_hidden_zones();
        let view = build_client_view(&state, &forward, Side::Runner);
        let sample_a = determinize(&view, &ascending, &mut StdRng::seed_from_u64(7));
        let sample_b = determinize(&view, &descending, &mut StdRng::seed_from_u64(7));

        assert_eq!(sample_a.corp.hq, sample_b.corp.hq);
        assert_eq!(sample_a.corp.r_and_d, sample_b.corp.r_and_d);
        assert_eq!(sample_a.runner.stack, sample_b.runner.stack);
        assert_eq!(sample_a.corp.installed, sample_b.corp.installed);
    }

    /// An identity is never in a deck, so it can never be sampled into a
    /// hidden zone — whatever seed is used and however many draws it takes
    /// to cycle the whole pool.
    #[test]
    fn identities_are_never_sampled_into_a_hidden_zone() {
        let mut registry = registry();
        registry.insert(blank_card("corp_identity", Side::Corp, CardType::Identity));
        registry.insert(blank_card("runner_identity", Side::Runner, CardType::Identity));
        let state = state_with_hidden_zones();
        let view = build_client_view(&state, &registry, Side::Runner);

        for seed in 0..64 {
            let sample = determinize(&view, &registry, &mut StdRng::seed_from_u64(seed));
            let hidden = sample
                .corp
                .hq
                .iter()
                .chain(&sample.corp.r_and_d)
                .chain(&sample.runner.stack)
                .chain(sample.corp.installed.iter().map(|c| &c.card));
            for card in hidden {
                assert!(
                    !matches!(registry.get(card).unwrap().card_type, CardType::Identity),
                    "seed {seed} sampled identity {card:?} into a hidden zone"
                );
            }
        }
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
            prompting_card: None,
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
