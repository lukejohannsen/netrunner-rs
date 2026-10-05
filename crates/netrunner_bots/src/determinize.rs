//! Samples one concrete, `ClientView`-consistent `GameState` — the "I" in
//! Information Set MCTS: `MctsAgent` runs its unchanged Phase-1 search
//! machinery against a sampled state instead of the real (unavailable) one,
//! `PlanningAgent` does the same for its lines and its one-ply lookahead, and `PuctAgent`
//! searches one sample and redraws it at every breach.
//!
//! **Hidden slots are filled from what the seat knows** (`Knowledge`,
//! Phase 5 §25 Stage 2). The seat's own hidden deck is exact up to order:
//! its `own_deck` — or, for a seat built without one, the published list
//! its identity names (`remaining_decklist`) — as a multiset minus every
//! copy already visible somewhere in the view. The **opponent's** hidden
//! cards are drawn from a prior over the format's pool (`Prior`): every
//! card of that side the format admits, each copy weighted by whether it
//! is in the opponent identity's faction, by the influence the deck has
//! left for the ones that are not, and by whether the seat has seen the
//! card — a seen card's other copies are `SEEN_COPY_WEIGHT` times as
//! likely as an unseen in-faction card, and a copy the view shows is one
//! fewer to draw. The opponent used to be guessed by matching its identity
//! to an embedded published list, which was right for every sample deck
//! and wrong for every deck a person builds; the record of what that
//! match bought and did not buy (ROADMAP Phase 3 §1: a sample that is a
//! permutation of the real deck moved no chair outside its seed band) is
//! why replacing it with a prior is not a strength decision either.
//!
//! **A slot only ever takes a card that fits it:** an unrezzed ICE slot
//! draws an ICE-typed card, a root slot a card that can sit in a root.
//! The prior is also what fills the slots left when a known deck runs out
//! — a rollout that has drawn more than the real deck holds — and, for a
//! registry with no card of the needed kind at all, a synthetic
//! placeholder keeps the sample buildable. Neither path models anything
//! the view does not say: no draw order, no "they kept two ICE in hand".

use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use netrunner_core::card::Faction;
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::format::DEFAULT_INFLUENCE_LIMIT;
use netrunner_core::rules::continuous;
use netrunner_core::rules::{
    ArchivedCard,
    AccessPhase, AccessState, AgendaPoints, Clicks, CorpState, Credits, EncounteredSubroutine, GameState, InstallSlot,
    InstalledCard, InstalledRunnerCard, MaskedZone, MemoryUnits, PlayerResources, PublicAccessPhase,
    PendingDecision, PsiBid, RunIce, RunState, RunnerState, ServerId, Side, SubroutineStatus,
};
use netrunner_core::rules::Viewer;
use netrunner_core::rules::Deck;
use netrunner_core::view::ClientView;

use crate::knowledge::Knowledge;

/// How much likelier one more copy of a card the seat has seen is than
/// an unseen card of the opponent's own faction.
///
/// A deck that holds a card holds two or three copies of it far more
/// often than one, so a sighting says the rest are probably there; an
/// unseen in-faction card is one of forty-odd choices for thirty-odd
/// slots. Three is the ratio of those two odds over the Startup pool —
/// about 0.8 for a second copy against about 0.27 per copy for an unseen
/// in-faction card — rounded, and it stands in for a measurement this
/// stage records rather than claims (Phase 5 §25 Stage 2, the
/// guess-quality line of `diag precepts`).
pub const SEEN_COPY_WEIGHT: f64 = 3.0;

/// The copies of a card a deck may hold when it prints no limit of its
/// own — `rules::deck::MAX_COPIES_PER_CARD`'s rule, read here for the
/// prior.
const PLAYSET: u32 = 3;

/// One card the prior may draw: the copies not yet accounted for and the
/// weight each of them draws at.
struct Candidate {
    card: CardId,
    /// Copies still hidden — the playset minus the copies the view shows.
    copies: u32,
    /// What `copies` is refilled to when every admissible candidate has
    /// been drawn dry (a pathological pool, see `Prior::draw`).
    playset: u32,
    weight: f64,
    is_ice: bool,
    fits_a_root: bool,
}

impl Candidate {
    fn admits(&self, slot: Slot) -> bool {
        match slot {
            Slot::CorpAny | Slot::RunnerAny => true,
            Slot::CorpIce => self.is_ice,
            Slot::CorpRoot => self.fits_a_root,
        }
    }
}

/// One side's prior over its hidden cards — see the module docs and
/// `Prior::over`. Drawn by weight and without replacement, so a sample
/// never holds more copies of a card than a deck may, which a cycling
/// registry pool did not promise.
struct Prior {
    entries: Vec<Candidate>,
}

impl Prior {
    /// The prior for `side`'s hidden cards, given the identity the view
    /// shows for that side and what the seat knows.
    ///
    /// The candidates are every playable non-identity card of `side` the
    /// format admits (`FormatRules::in_pool`, minus its ban list; `Casual`
    /// is every card), **sorted by `CardId` before anything is drawn.**
    /// `CardRegistry::iter` is `HashMap::values()`, whose order differs
    /// from one process to the next, and a seeded draw only reproduces
    /// its output over an identical input — so without the sort the same
    /// seed sampled different hidden cards on every run, and every
    /// determinizing bot was nondeterministic run to run (96 games at seed
    /// 1 once differed from *themselves* in 262 report keys).
    ///
    /// Each candidate's copies are its playset (a card's own `deck_limit`,
    /// else three, and no more than the identity allows — one under Nova
    /// Initiumia or Ampère; one for a ◆ card, which a deck runs one or two
    /// of and the table holds one of) less the copies the view shows. Its
    /// weight is `SEEN_COPY_WEIGHT` when the seat has seen the card at
    /// all, 1 for an unseen card of the identity's faction or a neutral
    /// one, and for an unseen out-of-faction card the share of the
    /// deck's remaining influence it could claim: the influence left
    /// after every seen out-of-faction copy has spent its cost, over the
    /// card's cost and the number of out-of-faction candidates. With
    /// fifteen influence and sixty-odd out-of-faction cards at two
    /// influence each, that is about an eighth of an in-faction card's
    /// weight — the ratio a real deck's six or seven imports bear to its
    /// thirty-odd faction cards. An identity with no budget (a *Learn to
    /// Play* starter) or none the view shows weighs every card alike.
    fn over(side: Side, identity: Option<&CardId>, visible: &BTreeMap<&CardId, usize>, knowledge: &Knowledge, registry: &CardRegistry) -> Self {
        let rules = knowledge.format.rules();
        let mut candidates: Vec<&CardDefinition> = registry
            .iter()
            .filter(|card| card.side == side && !matches!(card.card_type, CardType::Identity))
            // A card no NetrunnerDB pool lists (a test fixture, homebrew) is
            // in no format's pool; `Casual` alone holds it.
            .filter(|card| rules.in_pool(&card.id) && !rules.banned.contains(&card.id))
            .collect();
        candidates.sort_by(|a, b| a.id.cmp(&b.id));

        let identity = identity.and_then(|id| registry.get(id));
        let faction = identity.and_then(|id| id.faction);
        let budget = match identity {
            Some(id) if !id.unlimited_influence => Some(id.influence_limit.unwrap_or(DEFAULT_INFLUENCE_LIMIT)),
            _ => None,
        };
        let in_faction = |card: &CardDefinition| match (card.faction, faction) {
            (Some(Faction::NeutralCorp | Faction::NeutralRunner), _) | (None, _) | (_, None) => true,
            (Some(own), Some(theirs)) => own == theirs,
        };
        let seen = |card: &CardDefinition| knowledge.seen(&card.id).max(visible.get(&card.id).copied().unwrap_or(0));
        let cost = |card: &CardDefinition| card.influence_cost.unwrap_or(0).max(1);
        let spent: u32 = candidates.iter().filter(|card| !in_faction(card)).map(|card| seen(card) as u32 * cost(card)).sum();
        let out_of_faction = candidates.iter().filter(|card| !in_faction(card)).count().max(1) as f64;
        let remaining = budget.map(|limit| f64::from(limit.saturating_sub(spent).max(1)));

        let entries = candidates
            .into_iter()
            .filter_map(|card| {
                let playset = if card.unique { 1 } else { card.copy_limit_under(identity, PLAYSET) };
                let shown = visible.get(&card.id).copied().unwrap_or(0) as u32;
                let copies = playset.saturating_sub(shown);
                if copies == 0 {
                    return None;
                }
                let weight = if seen(card) > 0 {
                    SEEN_COPY_WEIGHT
                } else if in_faction(card) {
                    1.0
                } else {
                    remaining.map_or(1.0, |left| left / (f64::from(cost(card)) * out_of_faction))
                };
                Some(Candidate {
                    card: card.id.clone(),
                    copies,
                    playset,
                    weight,
                    is_ice: Slot::CorpIce.admits(card),
                    fits_a_root: Slot::CorpRoot.admits(card),
                })
            })
            .collect();
        Prior { entries }
    }

    /// One weighted draw among the candidates `slot` admits that still
    /// have a copy, which is then spent. When every admissible candidate
    /// is dry — a tiny test registry, or a rollout that has drawn more
    /// hidden cards than the whole pool holds — their copies are refilled
    /// to the playset and the draw goes on, which is the old cycling
    /// pool's "shuffle and keep drawing"; `None` only when the pool holds
    /// no card of the needed kind at all.
    fn draw(&mut self, slot: Slot, rng: &mut impl Rng) -> Option<CardId> {
        let total = |entries: &[Candidate]| entries.iter().filter(|c| c.copies > 0 && c.admits(slot)).map(|c| c.weight).sum::<f64>();
        let mut sum = total(&self.entries);
        if sum <= 0.0 {
            for candidate in self.entries.iter_mut().filter(|c| c.admits(slot)) {
                candidate.copies = candidate.playset;
            }
            sum = total(&self.entries);
            if sum <= 0.0 {
                return None;
            }
        }
        let mut pick = rng.random::<f64>() * sum;
        let mut chosen = None;
        for (index, candidate) in self.entries.iter().enumerate().filter(|(_, c)| c.copies > 0 && c.admits(slot)) {
            chosen = Some(index);
            if pick < candidate.weight {
                break;
            }
            pick -= candidate.weight;
        }
        let candidate = &mut self.entries[chosen?];
        candidate.copies -= 1;
        Some(candidate.card.clone())
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

/// The two tiers of draw pool for one determinization: the seat's own
/// remaining deck, and each side's prior behind it.
struct Pools<'a> {
    registry: &'a CardRegistry,
    /// The viewer's own decklist minus the copies the view already shows,
    /// shuffled; the other side's is empty, as is a seat's with no deck
    /// known. Emptied as slots consume it.
    corp_deck: Vec<CardId>,
    runner_deck: Vec<CardId>,
    corp: Prior,
    runner: Prior,
    /// The prior's own stream, seeded off the caller's: a weighted draw
    /// needs randomness at every slot, and threading the caller's `rng`
    /// through every `draw` site bought nothing the seed does not.
    rng: StdRng,
}

impl Pools<'_> {
    /// The first remaining decklist card the slot admits, else a draw from
    /// the prior. Taking the first admissible card of an already-shuffled
    /// list is a uniform draw from the admissible ones, and removing it is
    /// what makes the sample a permutation of the deck rather than a bag
    /// of guesses: a card drawn into R&D is not also behind an unrezzed
    /// ICE.
    fn draw(&mut self, slot: Slot) -> CardId {
        let registry = self.registry;
        let rng = &mut self.rng;
        let (deck, prior) = match slot.side() {
            Side::Corp => (&mut self.corp_deck, &mut self.corp),
            Side::Runner => (&mut self.runner_deck, &mut self.runner),
        };
        if let Some(index) = deck.iter().position(|id| registry.get(id).is_some_and(|card| slot.admits(card))) {
            return deck.remove(index);
        }
        // The placeholder keeps the caller total against a registry with
        // no card of the needed kind at all, instead of panicking.
        prior.draw(slot, rng).unwrap_or_else(|| CardId("__determinize_unknown".to_string()))
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
pub(crate) fn visible_cards(view: &ClientView) -> Vec<CardId> {
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
    ids.extend(view.corp.set_aside.iter().cloned());
    ids.extend(view.corp.play_area.iter().map(|played| played.card.clone()));
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
    ids.extend(view.runner.removed_from_game.iter().cloned());
    ids.extend(view.runner.set_aside.iter().cloned());
    ids.extend(view.runner.scored_agendas.iter().map(|scored| scored.card.clone()));
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
        if let MaskedZone::Visible(cards) = &access.resolved_cards {
            ids.extend(cards.iter().cloned());
        }
        match &access.phase {
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
fn cards_in_game(view: &ClientView, side: Side, registry: &CardRegistry) -> usize {
    // The Corp's score area can hold a Runner card added "as an agenda"
    // (Word on the Street), which is the Runner's to count.
    let owner = |card: &CardId| registry.get(card).map_or(Side::Corp, |definition| definition.side);
    let in_corp_score_area = view.corp.scored_agendas.iter().filter(|scored| owner(&scored.card) == side).count();
    match side {
        Side::Corp => {
            view.corp.hq_count
                + view.corp.rd_count
                + view.corp.archives.len()
                + view.corp.servers.iter().map(|server| server.ice.len() + server.root.len()).sum::<usize>()
                + in_corp_score_area
                + view.runner.scored_agendas.len()
                + view.corp.removed_from_game.len()
                + view.corp.set_aside.len()
                + view.corp.play_area.len()
        }
        Side::Runner => {
            view.runner.grip_count
                + view.runner.stack_count
                + view.runner.heap.len()
                + view.runner.removed_from_game.len()
                + view.runner.rig.iter().map(|card| 1 + card.hosted_cards.len() + card.hosted_unseen).sum::<usize>()
                + in_corp_score_area
        }
    }
}

/// The cards of `side` still unseen, if its identity names a published
/// decklist: that list as a multiset with one copy struck for every
/// visible copy of the card. Empty when nothing matches. Order is the
/// decklist's — the caller shuffles.
///
/// **Asked for the viewer's own side only** (Stage 2): a seat built
/// without `Knowledge::own_deck` is one of this workspace's own drivers
/// seating a sample deck, so the match is knowledge there; for the
/// opponent it was a guess, and the opponent is drawn from the prior.
///
/// Several published lists share an identity (*Precision Design* has two
/// sample decks; a starter and its boosted form share theirs). The one
/// taken explains the most visible copies; a tie goes to the list whose
/// size is nearest the cards on the table, then to the first by id. That
/// is a maximum-likelihood pick over a two-element prior, not a posterior
/// over lists, and it is enough: the two lists sharing an identity
/// diverge early, on the first unique card seen.
fn remaining_decklist(side: Side, identity: Option<&CardId>, seen: &BTreeMap<&CardId, usize>, view: &ClientView, registry: &CardRegistry) -> Vec<CardId> {
    let Some(identity) = identity else { return Vec::new() };
    let candidates = embedded_decks().iter().filter(|deck| deck.side == side && &deck.identity == identity);
    let in_game = cards_in_game(view, side, registry);

    let scored = candidates.map(|deck| {
        let explained: usize =
            deck.cards.iter().map(|entry| seen.get(&entry.card).map_or(0, |count| (*count).min(entry.count as usize))).sum();
        let size_gap = (deck.size() as usize).abs_diff(in_game);
        (explained, size_gap, deck)
    });
    let Some((_, _, deck)) = scored.min_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.id.cmp(&b.2.id))) else {
        return Vec::new();
    };
    remaining_of(&deck.to_deck(), seen, registry)
}

/// `deck` as a multiset with one copy struck for every visible copy of
/// each card — the seat's own remaining stack, up to order.
fn remaining_of(deck: &Deck, seen: &BTreeMap<&CardId, usize>, registry: &CardRegistry) -> Vec<CardId> {
    // Summed first: a deck file may list the same card twice, and the
    // visible copies are struck from the total, not from each entry.
    let mut counts: BTreeMap<&CardId, usize> = BTreeMap::new();
    for (card, count) in &deck.cards {
        // A card the registry does not know cannot be played by the
        // sample either — a homebrew registry that lacks a published card
        // gets one fewer copy rather than an unplayable id.
        if registry.get(card).is_some() {
            *counts.entry(card).or_insert(0) += *count as usize;
        }
    }
    let mut remaining = Vec::new();
    for (card, count) in counts {
        let struck = seen.get(card).copied().unwrap_or(0).min(count);
        remaining.extend(std::iter::repeat_n(card.clone(), count - struck));
    }
    remaining
}

/// Builds the draw pools: the viewer's own remaining deck (`Knowledge::
/// own_deck`, else the published list its identity names), and each
/// side's prior over the format's pool behind it.
///
/// Only the viewer's own side gets a list. `Knowledge::own_deck` is used
/// when its identity is the one the view shows for that side — a deck
/// threaded to the wrong seat is a driver's bug, which the debug
/// assertion names — and a spectator's sample draws both sides from the
/// prior.
///
/// Identities are never candidates (`Prior::over`): an identity is never
/// in a deck, so it can never be in a hidden zone, and a sample that put
/// *The Syndicate* in HQ or on top of the stack was imagining a card the
/// real game cannot hold there.
fn build_pools<'a>(view: &ClientView, registry: &'a CardRegistry, knowledge: &Knowledge, rng: &mut impl Rng) -> Pools<'a> {
    let visible_copies = visible_cards(view);
    // Only a side's own visible cards count against its list or its
    // prior; card ids are unique across sides, so one multiset serves.
    let mut visible: BTreeMap<&CardId, usize> = BTreeMap::new();
    for card in &visible_copies {
        *visible.entry(card).or_insert(0) += 1;
    }

    let own_deck = |side: Side, identity: Option<&CardId>| -> Vec<CardId> {
        if view.viewer != Viewer::Player(side) {
            return Vec::new();
        }
        match &knowledge.own_deck {
            Some(deck) => {
                debug_assert_eq!(Some(&deck.identity), identity, "the seat's own deck was built for another identity");
                if Some(&deck.identity) == identity { remaining_of(deck, &visible, registry) } else { Vec::new() }
            }
            None => remaining_decklist(side, identity, &visible, view, registry),
        }
    };
    let mut corp_deck = own_deck(Side::Corp, view.corp.identity.as_ref());
    let mut runner_deck = own_deck(Side::Runner, view.runner.identity.as_ref());
    corp_deck.shuffle(rng);
    runner_deck.shuffle(rng);

    Pools {
        registry,
        corp_deck,
        runner_deck,
        corp: Prior::over(Side::Corp, view.corp.identity.as_ref(), &visible, knowledge, registry),
        runner: Prior::over(Side::Runner, view.runner.identity.as_ref(), &visible, knowledge, registry),
        rng: StdRng::seed_from_u64(rng.random()),
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
        seen_by_runner: card.seen_by_runner,
        // Nor what happened to the copy this turn (`turn_log::CopyTurn`).
        this_turn: Default::default(),
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
        seen_by_runner: card.seen_by_runner,
        // Nor what happened to the copy this turn (`turn_log::CopyTurn`).
        this_turn: Default::default(),
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

/// The cards of HQ or R&D the sample's breach will still access
/// (`AccessState::from_zone`), drawn from the sample's own zone the way the
/// engine draws them: at random from HQ, from the top of R&D. Neither
/// player knows them — a Corp that sees its hand still does not know which
/// card of it the Runner will reach — so no view carries more than their
/// number, which `from_zone`'s length already holds.
fn draw_from_zone(run: &mut RunState, corp: &CorpState, rng: &mut impl Rng) {
    use rand::seq::IndexedRandom;
    let Some(access) = run.access_state.as_mut() else { return };
    let count = access.from_zone.len();
    access.from_zone = match access.server {
        ServerId::Hq => corp.hq.choose_multiple(rng, count).cloned().collect(),
        ServerId::RnD => corp.r_and_d.iter().rev().take(count).cloned().collect(),
        ServerId::Archives | ServerId::Remote(_) => Vec::new(),
    };
}

/// The name a masked access carries until the sample's zones exist to take
/// one from (`name_the_accessed_card`), as `AccessState::from_zone`'s are.
fn unnamed() -> CardId {
    CardId(String::new())
}

fn determinize_access_phase(phase: &PublicAccessPhase) -> AccessPhase {
    match phase {
        PublicAccessPhase::SelectNextCard { selectable_cards } => AccessPhase::SelectNextCard { selectable_cards: selectable_cards.clone() },
        // `decider` copies straight through rather than being re-derived
        // from the sampled card: it is public information, and a search tree
        // that disagreed with reality about whose decision is pending would
        // evaluate the position for the wrong player entirely.
        PublicAccessPhase::PendingInteractiveTrigger { card, cost, decider, can_pay } => AccessPhase::PendingInteractiveTrigger {
            card_id: card.clone().unwrap_or_else(unnamed),
            cost: cost.clone(),
            decider: *decider,
            can_pay: *can_pay,
        },
        PublicAccessPhase::PendingChoice { card, trash_cost, mandatory_steal, steal_cost, trash_also } => AccessPhase::PendingChoice {
            card_id: card.clone().unwrap_or_else(unnamed),
            trash_cost: *trash_cost,
            mandatory_steal: *mandatory_steal,
            steal_cost: steal_cost.clone(),
            trash_also: trash_also.clone(),
        },
    }
}

/// Whether `card`, out of HQ or R&D, could be the card the access `phase`
/// is of, by what the view says about a card it does not name: the
/// decision is the engine's reading of the card (`run::access`'s
/// `compute_pending_choice`), and its public half says whether the card is
/// an agenda — a steal, free or priced, is offered on nothing else — and
/// what trashing it costs, which for a card in no root is what it prints.
/// An interactive trigger is the card's own, at the card's own cost.
fn fits_the_access(phase: &AccessPhase, card: &CardDefinition) -> bool {
    match phase {
        AccessPhase::PendingChoice { mandatory_steal, steal_cost, trash_cost, .. } => {
            card.agenda_points.is_some() == (*mandatory_steal || steal_cost.is_some()) && card.trash_cost == *trash_cost
        }
        AccessPhase::PendingInteractiveTrigger { cost, .. } => card.interactive_on_access.as_ref().is_some_and(|interactive| interactive.cost == *cost),
        AccessPhase::SelectNextCard { .. } => false,
    }
}

/// Names the card of an access the view masks — the Corp's own view of a
/// card accessed in HQ, R&D or a remote, which it is not shown until the
/// card lands somewhere public — as **a card the sample has where the
/// access is**: the install the access pinned, else a card of the sample's
/// HQ or R&D the public half of the decision is true of
/// (`fits_the_access`), which is where the real card still is.
///
/// It was a draw from the pool with the view's decision copied beside it.
/// For the Corp the pool is its own deck, all of it already dealt into the
/// sample's R&D, so the draw fell through to the prior and named any Corp
/// card in the format: with the Corp parked on Send a Message's rez while
/// the breach stood at a second agenda, the sample's Runner "must steal" a
/// Boto, or could trash a Hedge Fund for 2[credit]. The rollout stole the
/// ice, and a score area holding it overflowed the stack
/// (`continuous::for_each_applying`, which has the other half of the fix).
///
/// A sample whose zone holds no such card — a seat with no deck of its
/// own, drawing from the prior — takes any Corp card the decision is true
/// of, and only a registry with none at all keeps a pool draw: the
/// decision and the card it is about never disagree where a card exists
/// that they could agree on.
fn name_the_accessed_card(run: &mut RunState, corp: &CorpState, registry: &CardRegistry, pools: &mut Pools<'_>, rng: &mut impl Rng) {
    use rand::seq::IndexedRandom;
    let Some(access) = run.access_state.as_mut() else { return };
    let named = match &access.phase {
        AccessPhase::PendingChoice { card_id, .. } | AccessPhase::PendingInteractiveTrigger { card_id, .. } => *card_id != unnamed(),
        AccessPhase::SelectNextCard { .. } => true,
    };
    if named {
        return;
    }
    let pinned = access.pending_install.and_then(|install| corp.installed.iter().find(|card| card.install_id == install)).map(|card| card.card.clone());
    let zone: &[CardId] = match access.server {
        ServerId::Hq => &corp.hq,
        ServerId::RnD => &corp.r_and_d,
        ServerId::Archives | ServerId::Remote(_) => &[],
    };
    let fits = |card: &&CardId| registry.get(card).is_some_and(|definition| fits_the_access(&access.phase, definition));
    let card = pinned.or_else(|| zone.iter().filter(fits).collect::<Vec<_>>().choose(rng).map(|card| (*card).clone())).unwrap_or_else(|| {
        // Sorted: a registry iterates in hash order, and a sample is a
        // function of its seed.
        let mut anywhere: Vec<&CardId> =
            registry.iter().filter(|definition| definition.side == Side::Corp && fits_the_access(&access.phase, definition)).map(|definition| &definition.id).collect();
        anywhere.sort();
        anywhere.choose(rng).map(|card| (*card).clone()).unwrap_or_else(|| pools.draw(Slot::CorpAny))
    });
    match &mut access.phase {
        AccessPhase::PendingChoice { card_id, .. } | AccessPhase::PendingInteractiveTrigger { card_id, .. } => *card_id = card,
        AccessPhase::SelectNextCard { .. } => {}
    }
}

/// `installed` is the already-sampled `corp.installed`: a masked `RunIce`
/// takes whatever card was drawn for the same `InstallId` there rather than
/// drawing again. The engine now keeps `run.ice` in step with
/// `corp.installed` (`run::reconcile_ice`), so a sample in which the two
/// disagree about one install is a state the real game cannot be in.
///
/// A shown `current_strength` is not carried at all: an ice's strength is a
/// question put to the sample (`continuous::ice_strength`), and the sample
/// has the installs, their counters and the lingering list the view's
/// answer was made from — `debug_assert_strengths_agree` checks it has.
fn determinize_run(
    run: &netrunner_core::rules::PublicRunState,
    registry: &CardRegistry,
    pools: &mut Pools<'_>,
    installed: &[InstalledCard],
) -> RunState {
    // One piece of ice of the run, its identity the view's or drawn from
    // the pool: the run's own list, and each list a forced encounter
    // away from the Runner's position is holding (`RunState::suspended`).
    let mut sample = |ice: &netrunner_core::rules::PublicRunIce| -> RunIce { match &ice.identity {
            Some(identity) => RunIce {
                install_id: ice.install_id,
                card_id: identity.card.clone(),
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
                let definition = registry.get(&card_id);
                // Subtype and subroutines follow the card that was drawn,
                // the way `run::engine::build_run_ice` seeds them — and it
                // seeds them **whatever the rez state**, so a sample that
                // left them blank was not a state the real game can be in.
                //
                // This used to hardcode `Barrier` with no subroutines, and
                // nothing downstream repaired it: `run::engine::reconcile_ice`
                // keeps an existing entry whenever its `card_id` still
                // matches the install, and here it matches by construction.
                // So a rollout that rezzed this ICE rezzed a toothless
                // Barrier and walked through it, on every sample, for every
                // search. See ROADMAP Phase 2 §5 item 37.
                let ice_type = match definition.map(|c| &c.card_type) {
                    Some(netrunner_core::dsl::CardType::Ice(ice_type)) => *ice_type,
                    // The pool is type-constrained to ICE, so this is
                    // unreachable for a registry that knows the card; the
                    // placeholder is for a card it does not.
                    _ => netrunner_core::dsl::IceType::Barrier,
                };
                let subroutines = definition
                    .map(|c| {
                        c.subroutines
                            .iter()
                            .enumerate()
                            .map(|(id, definition)| EncounteredSubroutine {
                                id,
                                definition: definition.clone(),
                                status: SubroutineStatus::Pending,
                                gained: false,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                RunIce {
                    install_id: ice.install_id,
                    card_id,
                    ice_type,
                    subroutines,
                    rezzed: ice.rezzed,
                }
            }
        } };
    let ice: Vec<RunIce> = run.ice.iter().map(&mut sample).collect();
    let suspended: Vec<netrunner_core::rules::SuspendedEncounter> = run
        .suspended
        .iter()
        .map(|suspended| netrunner_core::rules::SuspendedEncounter {
            phase: suspended.phase,
            ice: suspended.ice.iter().map(&mut sample).collect(),
            position: suspended.position,
            jack_out_permitted: suspended.jack_out_permitted,
            forced_encounter: suspended.forced_encounter,
            // Not in the view, as the run's own is not.
            ice_bypassed: false,
            fully_broken: suspended.fully_broken,
            this_encounter: suspended.this_encounter.clone(),
        })
        .collect();

    let access_state = run.access_state.as_ref().map(|access| AccessState {
        server: access.server,
        // Public: a card's text asked for these accesses in the open.
        outside_breach: access.outside_breach.clone(),
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
        candidates: access.candidates.clone(),
        // Public: the card at its decision left while it was accessed.
        left: access.left,
        // Only the number is in the view, and it is the number the sample
        // needs: which cards they are is drawn from the sample's own HQ or
        // R&D once those exist (`draw_from_zone`), since the cards the
        // breach will reach are cards of that zone.
        from_zone: vec![CardId(String::new()); access.from_zone as usize],
        resolved_cards: determinize_access_cards(&access.resolved_cards, pools),
        // A card the view masks is named once the sample's zones exist
        // (`name_the_accessed_card`), for the reason `from_zone` is.
        phase: determinize_access_phase(&access.phase),
    });

    RunState {
        server: run.server,
        phase: run.phase,
        ice,
        position: run.position,
        access_state,
        jack_out_permitted: run.jack_out_permitted,
        declared_successful: run.declared_successful,
        breach_only: run.breach_only,
        forced_encounter: run.forced_encounter,
        // Public, and where a forced encounter away from the Runner's
        // position returns to (Konjin, Ganked!).
        suspended,
        event_counters: run.event_counters,
        gained_for_the_run: run.gained_for_the_run.clone(),
        // Public and carried by the view — see `PublicRunState`. Zeroing
        // these made the sample poorer than the information the searcher
        // actually has: an action the Runner can really pay for out of
        // Bad Publicity looked unaffordable, deleting candidate actions.
        // (A barred steal/trash is on `lingering`, carried below.)
        bad_publicity_credits: run.bad_publicity_credits,
        bonus_run_credits: run.bonus_run_credits,
        run_credits_pay_for: run.run_credits_pay_for.clone(),
        redirect_on_approach: run.redirect_on_approach,
        // Not in the view: a run's end rider and whether a subroutine
        // resolved are known to the seat that set them, not carried. The
        // determinized run does not fire a Charm Offensive rider — a
        // search-quality limit, recorded in ROADMAP Phase 1 §8 Stage 3.
        // (Shred's armed prevention was a third; it is on `lingering`,
        // carried below, since it stopped being a field of the run.)
        on_end: Vec::new(),
        subroutine_resolved: false,
        // Public since RWR Stage 3b: the run's event has abilities of its
        // own for the run (Eye for an Eye), and Sang Kancil's boost asks
        // whether it is a run event.
        initiated_by: run.initiated_by.clone(),
        begun_as_the_turn_began: run.begun_as_the_turn_began,
        // Not in the view: whether the encountered ice was bypassed. A
        // rollout starting mid-encounter sees no bypass, which only ever
        // makes the searcher pay for subroutines the real game will not
        // fire.
        ice_bypassed: false,
        // Public, and what a pass "after fully breaking it" reads (Sipa).
        fully_broken: run.fully_broken,
        // Public, and what Stegodon MK IV's -2 reads.
        ice_derezzed: run.ice_derezzed,
        // Public, and what Mercury: Chrome Libertador's "if you did not
        // break any subroutines" reads.
        subroutine_broken: run.subroutine_broken,
        // Public, and what makes the run's end unsuccessful or not.
        reached_success_phase: run.reached_success_phase,
        // Public, and which server a breach that has begun goes on to.
        breached: run.breached,
        // Public, and what S-Dobrado's "the second time" reads.
        encounters: run.encounters,
        // Public, and what Into the Depths' "for each time you passed ice"
        // reads.
        ice_passed: run.ice_passed,
        // Not in the view: which action the run is part of. Only its end
        // announces it (`GameEvent::ActionFinished`), and the only card
        // that hears one is the Corp's, about the Corp's own actions.
        finishes: None,
        this_encounter: run.this_encounter.clone(),
        once_per_run_used: run.once_per_run_used.iter().cloned().collect(),
        additional_rd_access: 0,
        additional_hq_access: 0,
        access_replacement: None,
        access_replacement_card: None,
        access_replacement_install: None,
        cards_accessed_count: 0,
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

pub fn determinize(view: &ClientView, registry: &CardRegistry, knowledge: &Knowledge, rng: &mut impl Rng) -> GameState {
    let mut pools = build_pools(view, registry, knowledge, rng);

    // Reassembled in the **real** install order, not the view's
    // server-grouped one. `ServerView` groups installs by server, so
    // chaining the groups produces a different `corp.installed` ordering
    // than the state the view was built from — and both
    // `pending_choice::zone_card_ids` and `ActionSpace`'s installed-card
    // segments index by exactly that ordering. A sample that disagreed
    // about it made the caller's own `ToggleCardSelection { position }`
    // decode to a different card, which the one-ply chooser then found
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
        // Carried: public in every view, and the card whose text the
        // sample plays. Without it (Phase 3 §1 to Phase 5 §33) no
        // identity's ability was a transition of any sample, so the
        // planner — which takes the root from the view's list but every
        // step after it, and every judgment of a line, from the sample's —
        // never used one: Topan's install, Synapse Global's tag and LEO
        // Construction's end-the-run were the blind list's identities,
        // and every evaluator term that reads the Corp's identity across
        // the table (`eval::read::corp_faction`) read `None` at every
        // leaf. The carry was measured once under MCTS and found inside
        // the seed-spread band (Phase 3 §1: −0.016 at 128 simulations,
        // −0.002 at 512), so it was left off as a default with nothing
        // measured for it; Phase 5 §33 is what was measured for it.
        identity: view.corp.identity.clone(),
        bad_publicity: view.corp.bad_publicity,
        // Public (visible tokens on the granting card) and carried by the
        // view; zeroing them narrowed the Corp's affordable actions and
        // trace-bid range in the sample.
        // Public, and carried: which once-per-turn abilities are spent. A
        // Runner's view leaves out a use by a facedown Corp install, which
        // the sample then believes unspent — the one approximation left.
        once_per_turn_used: view.corp.once_per_turn_used.iter().cloned().collect(),
        // Public on the identity, and carried straight off the view.
        identity_counters: view.corp.identity_counters,
        // Public, and carried: a flipped Corp identity has different text.
        identity_flipped: view.corp.identity_flipped,
        // Carried where the view has it. A copy the Runner has not seen
        // is 0, which no back side reads: the view shows the copy as soon
        // as the identity flips (`masking`), so guessing one while the
        // front is up would spend the sample's randomness on nothing.
        identity_copy: view.corp.identity_copy.unwrap_or(0),
        removed_from_game: view.corp.removed_from_game.clone(),
        set_aside: view.corp.set_aside.clone(),
        // Public, and carried: a lockdown in play is active, so a sample
        // without it would steal under NAPD Cordon for nothing and break
        // NEXT Activation Command's ice at its printed strength.
        play_area: view.corp.play_area.clone(),
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
            // The printed number, the way `engine::seed_rig_card` seeds
            // it — **not** `card.current_strength`. That is the strength
            // the engine uses, with the boosts still running and what the
            // table adds (Echelon, Rising Tide) already in it; the sample
            // carries the first as `lingering` and derives the second from
            // its own board, so folding the shown number in here counted
            // both twice, and made a pump for this encounter permanent in
            // every rollout.
            base_strength: registry.get(&card.card).and_then(|definition| definition.strength).unwrap_or(0),
            // Both public and both carried by the view. `counters` was
            // simply being dropped, which made every counter-costed
            // ability (Botulus, Leech, Pennyshaver) illegal in the sample;
            // `hosted_on_ice` likewise, which made
            // `EffectRequirement::EncounteringHostIce` fail
            // unconditionally and so put every Trojan ability out of the
            // search's reach entirely.
            counters: card.counters,
            hosted_on_ice: card.hosted_on_ice,
            hosted_on_rig_card: card.hosted_on_rig_card,
            // Cards hosted facedown are only counted for this viewer, and
            // drawn like the grip they came from (Read-Write Share).
            hosted_cards: card.hosted_cards.iter().cloned().chain(pools.draw_n(Slot::RunnerAny, card.hosted_unseen)).collect(),
            // Public: Matryoshka's copies spent on a break this turn.
            turned_facedown: card.turned_facedown,
            hosted_cards_playable: card.hosted_cards_playable,
            // Not in the view, as the Corp install's is not.
            this_turn: Default::default(),
        })
        .collect();

    let runner = RunnerState {
        // Carried — see `corp.identity` above.
        identity: view.runner.identity.clone(),
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
        removed_from_game: view.runner.removed_from_game.clone(),
        set_aside: view.runner.set_aside.clone(),
        once_per_turn_used: view.runner.once_per_turn_used.iter().cloned().collect(),
        servers_run_this_turn: view.runner.servers_run_this_turn.clone(),
        discarded_this_discard_phase: view.runner.discarded_this_discard_phase.clone(),
        identity_flipped: view.runner.identity_flipped,
    };

    let mut active_run = view.active_run.as_ref().map(|run| determinize_run(run, registry, &mut pools, &corp.installed));
    if let Some(run) = active_run.as_mut() {
        draw_from_zone(run, &corp, rng);
        name_the_accessed_card(run, &corp, registry, &mut pools, rng);
    }

    // A rollout installs cards of its own under the ids the real game
    // will give them: the counter is public and the view carries it
    // (`ClientView::next_install_id`). It used to start just past the
    // highest id the view showed — a lower bound, short by every install
    // trashed since — which was enough for a search that never named an
    // install it had made, and not for a planner that names the card it
    // installed at its next step. Never below the highest id sampled,
    // so a view built by hand around installs it never counted still
    // installs without a collision.
    let next_install_id = corp
        .installed
        .iter()
        .map(|c| c.install_id.0)
        .chain(runner.rig.iter().map(|c| c.install_id.0))
        .max()
        .map_or(view.next_install_id, |highest| view.next_install_id.max(highest + 1));

    let state = GameState {
        corp,
        runner,
        phase: view.phase,
        // Public information, so it comes straight off the view rather than
        // being resampled — a determinized state that disagreed with the
        // real one about the turn number would mis-evaluate any "on turn N"
        // effect the search looks ahead through.
        turn: view.turn,
        // Public and carried whole (`rules::turn_log`): the sample agrees
        // with the real state about what has happened this turn and last.
        // It was rebuilt from two facts — the actions finished and whether
        // a run had succeeded — so inside a sample no operation had been
        // played, no agenda scored (Neurospike dealt 0) and the last turn
        // was empty (Public Trail and Measured Response unplayable).
        this_turn: view.this_turn,
        last_turn: view.last_turn,
        active_run,
        paid_ability_window: view.paid_ability_window.clone(),
        active_trace: view.active_trace.clone(),
        pending_prevention: view.pending_prevention.clone(),
        pending_paid_choice: view.pending_paid_choice.clone(),
        pending_decision: sample_decision(view, rng),
        // A parked payment is the payer's own unplayed action, so only the
        // payer's view holds it (`masking::PublicPendingPayment::own`) — and
        // the payer is the only seat asked to act while one is parked, so
        // theirs is the only view a decision is ever sampled from then. A
        // sample taken from the other chair has no action to park and gets
        // none: it disagrees with the table about whose move it is, for as
        // long as the question stands, and nothing is decided from it.
        pending_payment: view.pending_payment.as_ref().and_then(|payment| payment.own.clone()),
        // Filled only for the length of a replay inside `apply_action`.
        payment_answers: Vec::new(),
        // No masked representation to reconstruct these from (see their doc
        // comments on `GameState`), so a determinized hypothetical starts
        // them blank, same as a fresh `GameState::new`. Two former siblings
        // here — `last_discarded_cards` and `last_advancement_was_first` —
        // are gone entirely: they were transient resolution state and now
        // live on `ability::ResolutionContext`, which a determinized state
        // has no business carrying at all.
        last_completed_run: None,
        // Public and carried: every entry is a flat number on an install
        // both players see. `LingeringEffect::holds` reads the sample's own
        // run and turn, which are the view's.
        lingering: view.lingering.clone(),
        delayed: view.delayed.clone(),
        // Public: both players were shown each card.
        revealed: view.revealed.clone(),
        deferred_triggers: Vec::new(),
        seed: rng.random(),
        rng_step: 0,
        next_install_id,
        // Public, and the win threshold the search plays toward.
        rules: view.rules,
    };
    let mut state = state;
    seat_revealed(&mut state);
    seat_selection(&mut state, view);
    debug_assert_strengths_agree(&state, view, registry);
    state
}

/// The view's parked decision, with a bid the viewer was not shown (the
/// Corp's in a psi game, `PsiBid::Concealed`) guessed from the bids it
/// could have been — uniformly, since nothing in the view says which.
fn sample_decision(view: &ClientView, rng: &mut impl Rng) -> Option<PendingDecision> {
    let mut decision = view.pending_decision.clone()?;
    if let PendingDecision::PsiGame { corp_bid: corp_bid @ PsiBid::Concealed, corp_max, .. } = &mut decision {
        *corp_bid = PsiBid::Bid(rng.random_range(0..=*corp_max));
    }
    Some(decision)
}

/// Puts each card a parked selection shows its chooser out of a zone
/// they cannot otherwise see (`CardZoneRef::shows_the_chooser_hidden_cards`:
/// their own R&D or stack, the other side's hand or deck) at the position
/// the view names it by (`ClientView::selection`). The zone itself is
/// drawn from the pool, so before this the sample's cards at those
/// positions were guesses: AU Co.'s "look at the top 3 cards of R&D. Trash
/// 1 of those cards" was decided on three cards the Corp was not looking
/// at, and trashed an agenda 8 times in 14 where it could have kept it
/// (Phase 5 §40). A copy of the card already elsewhere in the zone is
/// swapped into place, so the sample keeps its count of each card; one
/// that is not is written over the guess, as `seat_revealed` does.
fn seat_selection(state: &mut GameState, view: &ClientView) {
    use netrunner_core::dsl::CardZoneRef;
    let Some(PendingDecision::ChooseCards { side: chooser, source, .. }) = &view.pending_decision else { return };
    if !source.shows_the_chooser_hidden_cards() || view.selection.is_empty() {
        return;
    }
    let opponent = match chooser {
        Side::Corp => Side::Runner,
        Side::Runner => Side::Corp,
    };
    let zone = match (source, opponent) {
        (CardZoneRef::OwnRAndD, _) | (CardZoneRef::OpponentDeck, Side::Corp) => &mut state.corp.r_and_d,
        (CardZoneRef::OwnStack | CardZoneRef::TopOfOwnStack, _) | (CardZoneRef::OpponentDeck, Side::Runner) => &mut state.runner.stack,
        (CardZoneRef::OpponentHand, Side::Corp) => &mut state.corp.hq,
        (CardZoneRef::OpponentHand, Side::Runner) => &mut state.runner.grip,
        _ => return,
    };
    // `TopOfOwnStack`'s one position is the stack's top, its last card
    // (`pending_choice::zone_card_ids`); every other zone is indexed as
    // it is stored.
    let index = |position: usize, len: usize| match source {
        CardZoneRef::TopOfOwnStack => len.checked_sub(1 + position),
        _ => (position < len).then_some(position),
    };
    let shown: Vec<(usize, &CardId)> =
        view.selection.iter().filter_map(|candidate| Some((index(candidate.position, zone.len())?, candidate.card.as_ref()?))).collect();
    let pinned: HashSet<usize> = shown.iter().map(|(at, _)| *at).collect();
    for (at, card) in shown {
        if zone[at] == *card {
            continue;
        }
        match (0..zone.len()).find(|&other| !pinned.contains(&other) && zone[other] == *card) {
            Some(other) => zone.swap(at, other),
            None => zone[at] = card.clone(),
        }
    }
}

/// Puts each card revealed in a hand the viewer cannot see
/// (`GameState::revealed`) into the sampled hand, over a card that is not
/// one of them. Revealed is known: a sample that drew Burner's three cards
/// out of HQ anew had nothing for the Runner to choose, and every
/// `CardFilter::Revealed` selection in it failed.
fn seat_revealed(state: &mut GameState) {
    for side in [Side::Corp, Side::Runner] {
        let wanted: Vec<CardId> = state.revealed.iter().filter(|revealed| revealed.side == side).map(|revealed| revealed.card.clone()).collect();
        let hand = match side {
            Side::Corp => &mut state.corp.hq,
            Side::Runner => &mut state.runner.grip,
        };
        // Which positions already hold a revealed card, a copy each.
        let mut kept = vec![false; hand.len()];
        let mut missing = Vec::new();
        for card in wanted {
            match (0..hand.len()).find(|&at| !kept[at] && hand[at] == card) {
                Some(at) => kept[at] = true,
                None => missing.push(card),
            }
        }
        for card in missing {
            if let Some(at) = (0..hand.len()).find(|&at| !kept[at]) {
                hand[at] = card;
                kept[at] = true;
            }
        }
    }
}

/// A sample's strengths are the view's. Both are public and both are the
/// engine's own question (`continuous::breaker_strength`,
/// `continuous::ice_strength`) put to a different state, so a disagreement
/// is a part of the answer this module failed to carry — the way a shown
/// strength taken as the printed one would now count Echelon's bonus
/// twice. Checked in a debug build, which is every test and both sweeps;
/// only for cards the registry knows, because a view built against another
/// registry is somebody else's mistake.
fn debug_assert_strengths_agree(state: &GameState, view: &ClientView, registry: &CardRegistry) {
    if !cfg!(debug_assertions) {
        return;
    }
    for (card, shown) in state.runner.rig.iter().zip(&view.runner.rig) {
        if registry.get(&card.card).is_some() {
            debug_assert_eq!(
                continuous::breaker_strength(state, registry, card),
                shown.current_strength,
                "the sample's {} disagrees with the view about its strength",
                card.card.0
            );
        }
    }
    let (Some(run), Some(shown)) = (&state.active_run, &view.active_run) else { return };
    for (ice, shown) in run.ice.iter().zip(&shown.ice) {
        if let Some(identity) = &shown.identity {
            debug_assert_eq!(
                continuous::ice_strength(state, registry, ice),
                identity.current_strength,
                "the sample's {} disagrees with the view about its strength",
                ice.card_id.0
            );
        }
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
pub fn resample_hidden(state: &mut GameState, view: &ClientView, registry: &CardRegistry, knowledge: &Knowledge, rng: &mut impl Rng) {
    let mut pools = build_pools(view, registry, knowledge, rng);

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
    for (installed, seen) in state.runner.rig.iter_mut().zip(&view.runner.rig) {
        if seen.hosted_unseen > 0 {
            installed.hosted_cards = pools.draw_n(Slot::RunnerAny, installed.hosted_cards.len());
        }
    }
    seat_revealed(state);
    // A breach already under way reaches cards of the zones just re-drawn.
    if let Some(run) = state.active_run.as_mut() {
        draw_from_zone(run, &state.corp, rng);
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
    use netrunner_core::dsl::{CardDefinition, CardType, IceType, Prohibition};
    use netrunner_core::rules::{
        AgendaPoints as AP, Clicks as C, CorpState as CS, Credits as Cr, GamePhase, GameState as CoreGameState,
        InstallSlot as CoreInstallSlot, InstalledCard, InstalledRunnerCard, MemoryUnits as MU, PlayerResources as PR,
        RunnerState as RS,
    };
    use netrunner_core::format::NsgFormat;
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
                identity_counters: 0,
                identity_copy: 0,
                identity_flipped: false,
                bad_publicity: 0,
                removed_from_game: Vec::new(), set_aside: Vec::new(), play_area: Vec::new(), once_per_turn_used: Default::default(),
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
                removed_from_game: Vec::new(),
                set_aside: Vec::new(),
                heap: Vec::new(),
                once_per_turn_used: Default::default(), servers_run_this_turn: Vec::new(), discarded_this_discard_phase: Vec::new(), identity_flipped: false,
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
                .chain(state.runner.scored_agendas.iter().map(|s| &s.card))
                .chain(&state.corp.removed_from_game)
                .chain(&state.corp.set_aside)
                .chain(state.corp.play_area.iter().map(|played| &played.card))
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

    /// The seat's own deck is exact: a sample's hidden cards of the
    /// viewer's side plus the cards the view shows are its decklist, one
    /// copy for every copy the list has, whatever has been installed,
    /// discarded or stolen — and the opponent's are not, because they
    /// are drawn from the prior.
    #[test]
    fn the_seats_own_deck_partitions_exactly_into_the_sample_and_the_opponents_does_not() {
        let (mut state, registry) = sample_game("agency", "stolen_goods");
        let corp_knows = Knowledge::new(NsgFormat::Casual, Some(netrunner_core::decks::by_id("agency").unwrap().to_deck()));
        let runner_knows = Knowledge::new(NsgFormat::Casual, Some(netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck()));
        let mut rng = StdRng::seed_from_u64(11);

        let view = build_client_view(&state, &registry, Side::Corp);
        let sample = determinize(&view, &registry, &corp_knows, &mut rng);
        assert_eq!(every_corp_card(&sample), decklist("agency"), "at setup, HQ and R&D together are the Corp's own list");
        assert_ne!(
            multiset(sample.runner.grip.iter().chain(&sample.runner.stack).cloned()),
            decklist("stolen_goods"),
            "the Runner's cards are the Corp's guess, not the Runner's list"
        );

        let view = build_client_view(&state, &registry, Side::Runner);
        let sample = determinize(&view, &registry, &runner_knows, &mut rng);
        assert_eq!(
            multiset(sample.runner.grip.iter().chain(&sample.runner.stack).cloned()),
            decklist("stolen_goods"),
            "the Runner's own stack is the list minus the grip"
        );
        assert_ne!(every_corp_card(&sample), decklist("agency"), "the Corp's cards are the Runner's guess");

        // Mid-game: a rezzed install, a faceup discard and a stolen agenda
        // are all visible to the Corp, and each strikes one copy of its
        // own list.
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
        state.runner.scored_agendas.push(netrunner_core::rules::ScoredAgenda::plain(stolen));

        let view = build_client_view(&state, &registry, Side::Corp);
        let sample = determinize(&view, &registry, &corp_knows, &mut rng);
        assert_eq!(every_corp_card(&sample), decklist("agency"));
        assert_eq!(every_corp_card(&state), decklist("agency"), "and so does the real state, which is the premise");
    }

    /// The Corp's view of a breach of its own HQ or R&D does not name the
    /// card being accessed, and says what the decision about it is: a steal
    /// the Runner must make, a trash at a price. The sample names it as a
    /// card it has there that the decision is true of. It drew one from the
    /// pool instead — for the Corp, past the end of its own deck, so from
    /// every Corp card in the format — and kept the decision: with the
    /// Corp parked on Send a Message's rez while the breach stood at the
    /// next agenda, the rollout's Runner stole a Boto, and the score of a
    /// score area with an ice in it never came back (`bench --bots
    /// planner,mcts --pairing mcts/planner --games 192 --seed 2` aborted
    /// with a stack overflow on `5903ab6`).
    #[test]
    fn a_masked_access_is_named_as_a_card_of_the_samples_own_zone_that_the_decision_is_true_of() {
        use netrunner_core::rules::{AccessPhase, AccessState, InstallId, PlayerAction, RunPhase, RunState, ServerId};
        let (mut state, registry) = sample_game("quick_and_dirty", "tickets_please");
        let knows = Knowledge::new(NsgFormat::Casual, Some(netrunner_core::decks::by_id("quick_and_dirty").unwrap().to_deck()));
        let mut deck: Vec<CardId> = state.corp.hq.drain(..).chain(state.corp.r_and_d.drain(..)).collect();
        let mut take = |id: &str| deck.remove(deck.iter().position(|card| card.0 == id).unwrap());
        // One agenda in a hand of cards that are not, and the one card of
        // the list with a trash cost on top of R&D.
        state.corp.hq = vec![take("hedge_fund"), take("palisade"), take("send_a_message"), take("tithe")];
        let vault = take("malapert_data_vault");
        let installed = take("offworld_office");
        state.corp.r_and_d = deck;
        state.corp.r_and_d.push(vault.clone());
        state.corp.installed.push(InstalledCard {
            card: installed.clone(),
            install_id: InstallId(40),
            server: ServerId::Remote(0),
            slot: CoreInstallSlot::Root,
            ..Default::default()
        });
        state.phase = GamePhase::Action(Side::Runner);
        let accessing = |server: ServerId, card: &CardId, pending_install: Option<InstallId>| {
            let definition = registry.get(card).unwrap();
            RunState {
                server,
                phase: RunPhase::AccessingCard,
                access_state: Some(AccessState {
                    server,
                    outside_breach: None,
                    left: false,
                    candidates: Vec::new(),
                    from_zone: Vec::new(),
                    resolved_cards: Vec::new(),
                    currently_accessing: None,
                    pending_install,
                    pending_install_rezzed: false,
                    phase: AccessPhase::PendingChoice {
                        card_id: card.clone(),
                        trash_cost: definition.trash_cost,
                        mandatory_steal: definition.agenda_points.is_some(),
                        steal_cost: None,
                        trash_also: None,
                    },
                }),
                ..Default::default()
            }
        };
        let named = |sample: &CoreGameState| sample.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase.card().cloned().unwrap();
        let agenda = CardId("send_a_message".to_string());

        for seed in 0..32 {
            let mut rng = StdRng::seed_from_u64(seed);

            // HQ: the one agenda in it, whatever the seed.
            state.active_run = Some(accessing(ServerId::Hq, &agenda, None));
            let view = build_client_view(&state, &registry, Side::Corp);
            let masked = &view.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase;
            assert!(matches!(masked, PublicAccessPhase::PendingChoice { card: None, mandatory_steal: true, .. }), "the premise: {masked:?}");
            let sample = determinize(&view, &registry, &knows, &mut rng);
            assert_eq!(named(&sample), agenda, "seed {seed}: a card the Runner must steal is an agenda of this HQ");
            // And the steal the rollout then makes is of an agenda, scored
            // as one, with Send a Message's rez parked on the Corp — the
            // position the search was standing in.
            let (stolen, _) = netrunner_core::rules::apply_action(&sample, &registry, PlayerAction::StealAgenda { card_id: agenda.clone() }).unwrap();
            assert_eq!(netrunner_core::rules::score(&stolen, &registry, Side::Runner), 3, "seed {seed}");
            assert!(!stolen.corp.hq.contains(&agenda), "seed {seed}: and it left the hand it was in");

            // R&D, whose order the Corp does not know: a card of the
            // sample's R&D that costs what the view says to trash.
            state.active_run = Some(accessing(ServerId::RnD, &vault, None));
            let sample = determinize(&build_client_view(&state, &registry, Side::Corp), &registry, &knows, &mut rng);
            assert_eq!(named(&sample), vault, "seed {seed}: the one card of the list with a trash cost of 4");
            assert!(sample.corp.r_and_d.contains(&vault), "seed {seed}");
            assert_eq!(every_corp_card(&sample), decklist("quick_and_dirty"), "seed {seed}: and naming it drew nothing past the deck");

            // A root: the install the access pinned, which the sample has.
            state.active_run = Some(accessing(ServerId::Remote(0), &installed, Some(InstallId(40))));
            let sample = determinize(&build_client_view(&state, &registry, Side::Corp), &registry, &knows, &mut rng);
            assert_eq!(named(&sample), installed, "seed {seed}");
        }

        // A seat with no deck of its own draws its R&D from the prior, and
        // may hold no such card there: the name is still one the decision
        // is true of.
        state.active_run = Some(accessing(ServerId::RnD, &vault, None));
        state.corp.identity = None;
        let view = build_client_view(&state, &registry, Side::Corp);
        for seed in 0..32 {
            let sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(seed));
            let definition = registry.get(&named(&sample)).unwrap();
            assert_eq!((definition.agenda_points, definition.trash_cost), (None, Some(4)), "seed {seed}: {}", definition.id.0);
        }
    }

    /// A seat built without a deck still finds its own by its identity
    /// — the published-list match, kept for the one side it is knowledge
    /// about (the person's decision, 29 September 2026) — and the two
    /// sample decks sharing *Precision Design* are told apart by the hand
    /// the Corp can see.
    #[test]
    fn a_seat_without_a_deck_finds_its_own_by_identity_and_never_the_opponents() {
        let (state, registry) = sample_game("agency", "stolen_goods");
        let view = build_client_view(&state, &registry, Side::Runner);
        let sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(1));
        assert_eq!(
            multiset(sample.runner.grip.iter().chain(&sample.runner.stack).cloned()),
            decklist("stolen_goods"),
            "the Runner's own list, by its identity"
        );
        assert_ne!(every_corp_card(&sample), decklist("agency"), "the Corp's list is not guessed from its identity");

        let (state, registry) = sample_game("discretion_advised", "stolen_goods");
        let view = build_client_view(&state, &registry, Side::Corp);
        let sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(2));
        assert_eq!(every_corp_card(&sample), decklist("discretion_advised"), "the hand the Corp sees picks its own list");
    }

    /// The identities shape the prior (faction, influence) and are
    /// carried into the sample's state, both of them, from either chair:
    /// they are public, and an identity's ability is a transition only
    /// of a state that has the identity (Phase 5 §33 — the planner never
    /// used one while samples left them off).
    #[test]
    fn identities_shape_the_prior_and_are_carried_into_the_sample() {
        let (state, registry) = sample_game("agency", "stolen_goods");
        assert!(state.corp.identity.is_some() && state.runner.identity.is_some(), "the premise");
        for viewer in [Side::Corp, Side::Runner] {
            let view = build_client_view(&state, &registry, viewer);
            assert_eq!(view.corp.identity, state.corp.identity, "the view carries it");
            let sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(1));
            assert_eq!(sample.corp.identity, state.corp.identity, "{viewer:?}");
            assert_eq!(sample.runner.identity, state.runner.identity, "{viewer:?}");
        }
    }

    /// Whether every Corp card the sample holds is in `format`'s pool.
    fn corp_cards_in_pool(sample: &CoreGameState, registry: &CardRegistry, format: NsgFormat) -> bool {
        let rules = format.rules();
        every_corp_card(sample).keys().all(|card| registry.get(card).is_some() && rules.in_pool(card) && !rules.banned.contains(card))
    }

    /// Under Startup the opponent's hidden cards are Startup cards, every
    /// one; under Casual the same seat imagines cards from every set.
    #[test]
    fn the_opponents_hidden_cards_come_from_the_formats_pool() {
        let (state, registry) = sample_game("agency", "stolen_goods");
        let view = build_client_view(&state, &registry, Side::Runner);
        let startup = Knowledge::new(NsgFormat::Startup, None);
        for seed in 0..8 {
            let sample = determinize(&view, &registry, &startup, &mut StdRng::seed_from_u64(seed));
            assert!(corp_cards_in_pool(&sample, &registry, NsgFormat::Startup), "seed {seed}: a card outside Startup");
            assert_eq!(sample.corp.r_and_d.len(), state.corp.r_and_d.len());
        }
        let outside_startup = (0..8).any(|seed| {
            let sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(seed));
            !corp_cards_in_pool(&sample, &registry, NsgFormat::Startup)
        });
        assert!(outside_startup, "Casual is every card, and the registry holds cards from every set");
    }

    /// A seat never imagines its own side's cards in the other side's
    /// zones: from the Runner's chair every hidden Corp card is a Corp
    /// card and none is a card of the Runner's deck, and the reverse from
    /// the Corp's — the prior is built per side, and a Corp slot draws
    /// from the Corp's alone. The seat's own hidden cards are its deck
    /// exactly, only the order unknown (the person's requirement, 29
    /// September 2026).
    #[test]
    fn a_seat_never_guesses_its_own_sides_cards_into_the_opponents_zones() {
        let (state, registry) = sample_game("agency", "stolen_goods");
        let corp_list = decklist("agency");
        let runner_list = decklist("stolen_goods");
        let knows = |id: &str| Knowledge::new(NsgFormat::Startup, Some(netrunner_core::decks::by_id(id).unwrap().to_deck()));
        for seed in 0..8 {
            let view = build_client_view(&state, &registry, Side::Runner);
            let sample = determinize(&view, &registry, &knows("stolen_goods"), &mut StdRng::seed_from_u64(seed));
            for card in every_corp_card(&sample).keys() {
                assert_eq!(registry.get(card).map(|def| def.side), Some(Side::Corp), "seed {seed}: {} in a Corp zone", card.0);
                assert!(!runner_list.contains_key(card), "seed {seed}: the Runner's own {} guessed into the Corp's zones", card.0);
            }
            assert_eq!(multiset(sample.runner.grip.iter().chain(&sample.runner.stack).cloned()), runner_list, "seed {seed}: its own deck, exactly");

            let view = build_client_view(&state, &registry, Side::Corp);
            let sample = determinize(&view, &registry, &knows("agency"), &mut StdRng::seed_from_u64(seed));
            for card in sample.runner.grip.iter().chain(&sample.runner.stack) {
                assert_eq!(registry.get(card).map(|def| def.side), Some(Side::Runner), "seed {seed}: {} in a Runner zone", card.0);
                assert!(!corp_list.contains_key(card), "seed {seed}: the Corp's own {} guessed into the Runner's zones", card.0);
            }
            assert_eq!(every_corp_card(&sample), corp_list, "seed {seed}: its own deck, exactly");
        }
    }

    /// Ten operations of one faction, fifteen hidden slots. A card the
    /// seat has seen — faceup in Archives once, then shuffled back — is
    /// drawn into the hidden zones more often than one it has not.
    #[test]
    fn a_seen_card_makes_its_other_copies_likelier() {
        let mut registry = CardRegistry::new();
        for i in 0..10 {
            registry.insert(blank_card(&format!("op_{i}"), Side::Corp, CardType::Operation));
        }
        let mut state = CoreGameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.hq = vec![CardId("op_0".to_string()); 5];
        state.corp.r_and_d = vec![CardId("op_0".to_string()); 10];
        let hidden = |sample: &CoreGameState, card: &str| {
            sample.corp.hq.iter().chain(&sample.corp.r_and_d).filter(|c| c.0 == card).count()
        };

        let mut seen = Knowledge::default();
        state.corp.archives = vec![netrunner_core::rules::ArchivedCard::faceup(CardId("op_3".to_string()))];
        seen.observe(&build_client_view(&state, &registry, Side::Runner));
        state.corp.archives.clear();
        assert_eq!(seen.seen(&CardId("op_3".to_string())), 1);

        let view = build_client_view(&state, &registry, Side::Runner);
        let (mut with, mut without) = (0, 0);
        for seed in 0..64 {
            with += hidden(&determinize(&view, &registry, &seen, &mut StdRng::seed_from_u64(seed)), "op_3");
            without += hidden(&determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(seed)), "op_3");
        }
        assert!(with > without, "seen {with} against unseen {without}");
        // Fifteen slots over thirty copies: about a copy and a half of
        // each unseen card, never more than its playset.
        assert!(without > 32 && without < 160, "{without}");
    }

    /// With a Jinteki identity, an unseen NBN card is drawn far less
    /// often than an unseen Jinteki one; no card is drawn past its
    /// playset; and a ◆ card the table shows is not drawn at all.
    #[test]
    fn faction_and_influence_weigh_the_prior_and_a_playset_bounds_it() {
        use netrunner_core::card::Faction;
        let mut registry = CardRegistry::new();
        let mut identity = blank_card("jinteki_id", Side::Corp, CardType::Identity);
        identity.faction = Some(Faction::Jinteki);
        identity.influence_limit = Some(15);
        registry.insert(identity);
        // Thirty cards a side, so fifteen hidden slots come nowhere near
        // drawing the faction dry — the shape of a real pool.
        for i in 0..30 {
            let mut own = blank_card(&format!("jin_{i}"), Side::Corp, CardType::Operation);
            own.faction = Some(Faction::Jinteki);
            registry.insert(own);
            let mut import = blank_card(&format!("nbn_{i}"), Side::Corp, CardType::Operation);
            import.faction = Some(Faction::Nbn);
            import.influence_cost = Some(3);
            registry.insert(import);
        }
        let mut console = blank_card("console", Side::Corp, CardType::Asset);
        console.unique = true;
        registry.insert(console);

        let mut state = CoreGameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.identity = Some(CardId("jinteki_id".to_string()));
        state.corp.hq = vec![CardId("jin_0".to_string()); 5];
        state.corp.r_and_d = vec![CardId("jin_0".to_string()); 10];
        state.corp.installed = vec![InstalledCard {
            card: CardId("console".to_string()),
            server: netrunner_core::rules::ServerId::Remote(0),
            slot: CoreInstallSlot::Root,
            rezzed: true,
            ..Default::default()
        }];
        let view = build_client_view(&state, &registry, Side::Runner);

        let (mut jinteki, mut nbn) = (0, 0);
        for seed in 0..32 {
            let sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(seed));
            let hidden = multiset(sample.corp.hq.iter().chain(&sample.corp.r_and_d).cloned());
            for (card, count) in &hidden {
                assert!(*count <= 3, "seed {seed}: {} copies of {}", count, card.0);
                assert_ne!(card.0, "console", "seed {seed}: the ◆ card on the table is the only one");
                if card.0.starts_with("jin_") { jinteki += count } else { nbn += count }
            }
        }
        assert!(jinteki > 4 * nbn, "Jinteki {jinteki} against NBN {nbn}");
        assert!(nbn > 0, "an import is unlikely, not impossible");
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
        let sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(4));
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
        let mut sample = determinize(&view, &registry, &Knowledge::default(), &mut rng);
        let before = sample.clone();

        resample_hidden(&mut sample, &view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(11));

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
        resample_hidden(&mut again, &view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(11));
        assert_eq!(again, sample, "the redraw is a pure function of its seed");
    }

    #[test]
    fn own_hand_is_reproduced_exactly() {
        let state = state_with_hidden_zones();
        let registry = registry();
        let view = build_client_view(&state, &registry, Side::Runner);
        let mut rng = StdRng::seed_from_u64(1);

        let sample = determinize(&view, &registry, &Knowledge::default(), &mut rng);
        assert_eq!(sample.runner.grip, vec![CardId("sure_gamble".to_string())]);
    }

    /// A card revealed in HQ is known to the Runner, so every sample holds
    /// it there, however HQ is otherwise drawn — and a redraw keeps it too.
    #[test]
    fn a_card_revealed_in_hq_is_in_every_sample_of_it() {
        let mut state = state_with_hidden_zones();
        let revealed = state.corp.hq[0].clone();
        state.revealed = vec![netrunner_core::rules::RevealedCard { side: Side::Corp, card: revealed.clone() }];
        let registry = registry();
        let view = build_client_view(&state, &registry, Side::Runner);
        for seed in 0..16 {
            let mut sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(seed));
            assert!(sample.corp.hq.contains(&revealed), "seed {seed}");
            assert_eq!(sample.corp.hq.len(), view.corp.hq_count);
            resample_hidden(&mut sample, &view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(seed + 100));
            assert!(sample.corp.hq.contains(&revealed), "seed {seed}, redrawn");
        }
    }

    #[test]
    fn hidden_zone_sizes_match_the_view() {
        let state = state_with_hidden_zones();
        let registry = registry();
        let view = build_client_view(&state, &registry, Side::Runner);
        let mut rng = StdRng::seed_from_u64(2);

        let sample = determinize(&view, &registry, &Knowledge::default(), &mut rng);
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
        let sample_a = determinize(&view, &registry, &Knowledge::default(), &mut rng_a);
        let sample_b = determinize(&view, &registry, &Knowledge::default(), &mut rng_b);

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
        let sample_a = determinize(&view, &ascending, &Knowledge::default(), &mut StdRng::seed_from_u64(7));
        let sample_b = determinize(&view, &descending, &Knowledge::default(), &mut StdRng::seed_from_u64(7));

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
            let sample = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(seed));
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

        let sample = determinize(&view, &registry, &Knowledge::default(), &mut rng);
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
    /// The card at that position is the one the view shows there, too
    /// (`seat_selection`, Phase 5 §40). Before, the stack was resampled
    /// around the position, so the sample judged a toggle on a card the
    /// chooser was not looking at — and AU Co.'s search trashed agendas by
    /// it.
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
            let sampled = determinize(&view, &registry, &Knowledge::default(), &mut rng);
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
            assert_eq!(sampled.runner.stack[0], target, "seed {seed}: the card shown at the position is the one sampled there");
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
        // An identity's recurring credits are hosted on it (NBN: Making
        // News), so a sample carries them the way it carries any identity
        // counter, with nothing of its own to copy.
        state.corp.identity_counters = 2;
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
            ..Default::default()
        });
        // What is lingering is public too, and a sample that dropped it
        // would steal through Ansel's bar, score through Luminal's lock
        // and rez 3[c] short of a Tread Lightly run — the last two were
        // fields no view carried, so every sample did. Shred's armed
        // prevention was a third, a field of the run: every sample of a
        // Shred run ended it at the first "End the run".
        {
            use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
            let entry = |what, on, until| LingeringEffect { what, on, until, source: CardId("source".to_string()) };
            state.lingering = vec![
                entry(Lingering::Cannot(Prohibition::StealOrTrash), On::Player(Side::Runner), Until::EndOfRun),
                entry(Lingering::Cannot(Prohibition::ScoreAgendas), On::Player(Side::Corp), Until::EndOfTurn(state.turn)),
                entry(Lingering::RezCost(3), On::EachIce, Until::EndOfRun),
                entry(
                    Lingering::PreventRunEnding(netrunner_core::dsl::EndRunPrevention::UnlessCorpTrashesRootCountFromHq),
                    On::Player(Side::Corp),
                    Until::EndOfRun,
                ),
            ];
        }

        for side in [Side::Corp, Side::Runner] {
            let view = build_client_view(&state, &registry, side);
            let mut rng = StdRng::seed_from_u64(7);
            let sampled = determinize(&view, &registry, &Knowledge::default(), &mut rng);

            let run = sampled.active_run.as_ref().expect("the run survives");
            assert_eq!(run.bad_publicity_credits, 2, "{side:?}");
            assert_eq!(run.bonus_run_credits, 3, "{side:?}");
            assert!(continuous::cannot(&sampled, &registry, Prohibition::StealOrTrash), "{side:?}");
            assert!(continuous::cannot(&sampled, &registry, Prohibition::ScoreAgendas), "{side:?}");
            assert_eq!(netrunner_core::rules::lingering::ice_rez_cost(&sampled), 3, "{side:?}");
            assert!(
                sampled.lingering.iter().any(|e| matches!(e.what, netrunner_core::rules::lingering::Lingering::PreventRunEnding(_))),
                "{side:?}: Shred is still armed in the sample"
            );

            assert_eq!(sampled.runner.rig[0].counters, 3, "{side:?}");
            assert_eq!(
                sampled.runner.rig[0].hosted_on_ice,
                Some(netrunner_core::rules::InstallId::PLACEHOLDER),
                "{side:?}: a Trojan's host is public"
            );

            assert_eq!(sampled.corp.identity_counters, 2, "{side:?}: credits hosted on the identity");
            // Rezzed, so its counters are visible to both sides.
            assert_eq!(sampled.corp.installed[0].counters, 4, "{side:?}");
        }
    }

    /// What has happened this turn is public and a sample keeps it. It was
    /// rebuilt from two facts, so inside a sample the last turn was empty
    /// (Public Trail unplayable), no agenda had been scored (Neurospike
    /// dealt 0) and every once-per-turn ability was unspent.
    #[test]
    fn the_turn_survives_determinization_and_a_facedown_use_is_not_shown_to_the_runner() {
        use netrunner_core::dsl::Trigger;
        use netrunner_core::rules::{GameEvent, InstallId, OncePerTurnKey, ServerId};
        let registry = registry();
        let mut state = CoreGameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        // Two of the Runner's successful runs, then the turn they were in
        // ends.
        let succeeded = GameEvent::RunSucceeded { server: ServerId::Hq };
        for _ in 0..2 {
            netrunner_core::rules::dispatch_event(&mut state, &registry, &succeeded).expect("a run succeeded");
        }
        state.last_turn = std::mem::take(&mut state.this_turn);
        let scored = GameEvent::AgendaScored { card: CardId("corp_agenda_0".to_string()), agenda_points: 2, server: ServerId::Remote(0) };
        netrunner_core::rules::dispatch_event(&mut state, &registry, &scored).expect("scored");
        let install = |id: u32, rezzed| InstalledCard {
            card: CardId("corp_ice_0".to_string()),
            install_id: InstallId(id),
            server: ServerId::Remote(0),
            slot: CoreInstallSlot::Ice,
            rezzed,
            ..Default::default()
        };
        state.corp.installed = vec![install(1, true), install(2, false)];
        let used = |id: u32| OncePerTurnKey { card: Some(CardId("corp_ice_0".to_string())), install: Some(InstallId(id)) };
        state.corp.once_per_turn_used = [used(1), used(2)].into_iter().collect();
        let telework = OncePerTurnKey { card: Some(CardId("telework_contract".to_string())), install: Some(InstallId(9)) };
        state.runner.once_per_turn_used = [telework.clone()].into_iter().collect();

        for side in [Side::Corp, Side::Runner] {
            let view = build_client_view(&state, &registry, side);
            let sampled = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(7));
            assert_eq!(sampled.this_turn, state.this_turn, "{side:?}");
            assert_eq!(sampled.this_turn.agenda_points_scored(), 2, "{side:?}");
            assert_eq!(sampled.last_turn.times(Trigger::OnSuccessfulRun), 2, "{side:?}");
            assert!(sampled.runner.once_per_turn_used.contains(&telework), "{side:?}: the rig is faceup");
            assert!(sampled.corp.once_per_turn_used.contains(&used(1)), "{side:?}: a rezzed card's use is public");
            assert_eq!(sampled.corp.once_per_turn_used.contains(&used(2)), side == Side::Corp, "{side:?}: a facedown card's use would name it");
        }
    }

    /// A view shows the strength the engine uses, and a sample has to take
    /// that number apart again: the printed part from the registry, the
    /// boosts still running from the view's `lingering`, and what the table
    /// adds from its own rig. Taking the shown number as the printed one —
    /// what this did while the view showed the stored half alone — would
    /// count Echelon's bonus twice and make a pump for this encounter last
    /// every rollout; dropping `lingering` would lose a pump already paid
    /// for.
    #[test]
    fn a_shown_strength_is_taken_apart_into_printed_lingering_and_table() {
        use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
        use netrunner_core::rules::{InstallId, RunIce, RunPhase, RunState, ServerId};

        let registry = playable_registry();
        let (echelon, corroder, wall) = (InstallId(10), InstallId(11), InstallId(1));
        let mut state = CoreGameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.rig = vec![
            InstalledRunnerCard { card: CardId("echelon".to_string()), install_id: echelon, base_strength: 0, ..Default::default() },
            InstalledRunnerCard { card: CardId("corroder".to_string()), install_id: corroder, base_strength: 2, ..Default::default() },
        ];
        state.corp.installed = vec![InstalledCard {
            install_id: wall,
            card: CardId("wall_of_static".to_string()),
            server: ServerId::Hq,
            slot: CoreInstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                install_id: wall,
                card_id: CardId("wall_of_static".to_string()),
                ice_type: IceType::Barrier,
                subroutines: Vec::new(),
                rezzed: true,
            }],
            ..Default::default()
        });
        let pump = |on, delta| LingeringEffect {
            what: Lingering::Strength(delta),
            on: On::Install(on),
            until: Until::EndOfEncounter(wall),
            source: CardId("corroder".to_string()),
        };
        state.lingering = vec![pump(corroder, 2), pump(wall, -1)];

        for side in [Side::Corp, Side::Runner] {
            let view = build_client_view(&state, &registry, side);
            assert_eq!(view.runner.rig[0].current_strength, 2, "{side:?}: Echelon, +1 for each of two icebreakers");
            assert_eq!(view.runner.rig[1].current_strength, 4, "{side:?}: Corroder, 2 printed and +2 paid for");

            let mut sampled = determinize(&view, &registry, &Knowledge::default(), &mut StdRng::seed_from_u64(7));
            assert_eq!(sampled.lingering, state.lingering, "{side:?}");
            assert_eq!(sampled.runner.rig[0].base_strength, 0, "{side:?}: the printed number, not the shown one");
            assert_eq!(sampled.runner.rig[1].base_strength, 2, "{side:?}");
            for (card, shown) in sampled.runner.rig.iter().zip(&view.runner.rig) {
                assert_eq!(continuous::breaker_strength(&sampled, &registry, card), shown.current_strength, "{side:?}: {}", card.card.0);
            }
            let ice = &sampled.active_run.as_ref().unwrap().ice[0];
            assert_eq!(continuous::ice_strength(&sampled, &registry, ice), 2, "{side:?}: 3 printed, and the -1 is a lingering effect, not the ice");

            // And the pump ends with the encounter inside the sample too.
            sampled.active_run = None;
            assert_eq!(continuous::breaker_strength(&sampled, &registry, &sampled.runner.rig[1]), 2, "{side:?}");
        }
    }

    /// The counterpart: an *unrezzed* Corp card's counters are masked
    /// precisely so they cannot leak its identity, so the sample must not
    /// invent them. `0` here is honest ignorance, not a dropped field.
    /// The sample must be a state the real game could be in — the
    /// `determinize_run` doc comment says exactly that. An unrezzed ICE
    /// is masked from the Runner, so the sample draws its identity; the
    /// question is whether the rest of the `RunIce` follows that
    /// identity. `run::engine::build_run_ice` seeds `ice_type`,
    /// `current_strength` and `subroutines` from the card definition
    /// **whatever the rez state**, so a faithful sample must too.
    ///
    /// Every ICE the pool can draw is a Sentry with two subroutines, so
    /// the assertion cannot pass by drawing something that happens to
    /// match a placeholder — the fixture's default `Barrier` with an
    /// empty subroutine list is exactly what this is hunting.
    #[test]
    fn a_sampled_unrezzed_ice_carries_its_cards_subtype_and_subroutines() {
        let subroutine = || netrunner_core::dsl::SubroutineDef {
            text: "End the run.".to_string(),
            effect: netrunner_core::dsl::Effect::EndTheRun,
            only_breakable_by: None,
        };
        let mut registry = CardRegistry::new();
        for i in 0..5 {
            let mut ice = blank_card(&format!("corp_ice_{i}"), Side::Corp, CardType::Ice(IceType::Sentry));
            ice.subroutines = vec![subroutine(), subroutine()];
            registry.insert(ice);
            registry.insert(blank_card(&format!("runner_card_{i}"), Side::Runner, CardType::Event));
        }

        let mut state = CoreGameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.installed = vec![InstalledCard {
            card: CardId("corp_ice_0".to_string()),
            server: netrunner_core::rules::ServerId::RnD,
            slot: CoreInstallSlot::Ice,
            rezzed: false,
            ..Default::default()
        }];
        state.active_run = Some(netrunner_core::rules::RunState {
            server: netrunner_core::rules::ServerId::RnD,
            phase: netrunner_core::rules::RunPhase::ApproachIce,
            ice: vec![netrunner_core::rules::RunIce {
                card_id: CardId("corp_ice_0".to_string()),
                install_id: state.corp.installed[0].install_id,
                ice_type: IceType::Sentry,
                subroutines: vec![
                    netrunner_core::rules::EncounteredSubroutine {
                        id: 0,
                        definition: subroutine(),
                        status: netrunner_core::rules::SubroutineStatus::Pending,
                        gained: false,
                    },
                    netrunner_core::rules::EncounteredSubroutine {
                        id: 1,
                        definition: subroutine(),
                        status: netrunner_core::rules::SubroutineStatus::Pending,
                        gained: false,
                    },
                ],
                rezzed: false,
            }],
            ..Default::default()
        });

        let view = build_client_view(&state, &registry, Side::Runner);
        assert!(
            view.active_run.as_ref().expect("the run is visible").ice[0].identity.is_none(),
            "an unrezzed ICE must be masked from the Runner, or this test is measuring the wrong branch"
        );

        let mut rng = StdRng::seed_from_u64(7);
        let sampled = determinize(&view, &registry, &Knowledge::default(), &mut rng);
        let ice = &sampled.active_run.as_ref().expect("the run survives the sample").ice[0];

        assert_eq!(ice.ice_type, IceType::Sentry, "the sample's subtype must follow the card it drew, not a placeholder");
        assert_eq!(
            ice.subroutines.len(),
            2,
            "an unrezzed ICE the sample later rezzes must already carry its subroutines: `reconcile_ice` keeps an \
             entry whose `card_id` still matches, so an empty list is never repaired"
        );
    }

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
        let sampled = determinize(&view, &registry, &Knowledge::default(), &mut rng);
        assert_eq!(sampled.corp.installed[0].counters, 0);

        let corp_view = build_client_view(&state, &registry, Side::Corp);
        let mut rng = StdRng::seed_from_u64(7);
        let sampled = determinize(&corp_view, &registry, &Knowledge::default(), &mut rng);
        assert_eq!(sampled.corp.installed[0].counters, 5, "the owner sees its own counters");
    }
}
