//! Where a payment comes from: the one door every credit cost goes through.
//!
//! A player's credits are not all in one place. Beside the credit pool
//! there are the credits bad publicity lends the Runner for a run, the
//! credits a run event brought with it (Overclock), an identity's recurring
//! credits (NBN: Making News) and the credits hosted on a card that says
//! what they may be spent on (Azimat, Open Market, Mahkota Langit Grid).
//! `sources` is the one scan that finds them: the site that pays says what
//! the payment is *for* (`Purpose`), and every place whose credits may be
//! spent on that is a source, the credit pool last.
//!
//! **Whether a cost can be paid and paying it are the same scan.** They
//! were not. The run's pools and the identity's lived in `ability::
//! pay_cost`'s waterfall; a hosted pool was drained by whichever handler
//! knew the purpose, ahead of that waterfall, with its own affordability
//! sum written beside it — and every such sum forgot something the payment
//! then took:
//!
//! - **A trash cost could not be paid with bad publicity or Overclock's
//!   credits.** `run::access::resolve_trash` summed the credit pool and
//!   Azimat, refused, and never reached the `pay_cost` that would have
//!   taken both. Legal actions are probed, so the trash was never offered.
//! - **A paid ability could not be paid for with Overclock's credits**
//!   where the question was asked ahead of the payment —
//!   `ability::cost_is_affordable` counted bad publicity and forgot the
//!   run's own credits, so no window opened for an ability only they could
//!   pay for.
//! - **A connection installed by a card's text could not be paid for from
//!   Open Market**, which only the click install drained; and an install or
//!   an access trigger priced ahead of time counted the credit pool alone.
//!
//! **What a pool may be spent on is a word the card prints
//! (`dsl::PaysFor`), matched against the purpose in one place (`covers`).**
//! No handler names a pool. A purpose exists only where a card in the pool
//! makes it matter: a play cost, an advance and a steal cost are `Other`
//! until a card prints "use these credits to play…".
//!
//! **How a payment is split between pools is the payer's to say, when it
//! matters** (`plan`).
//! Every pool has a class — what its credits may be spent on, and how long
//! they last (`Breadth`, `Life`) — and a pool whose credits are never worth
//! more than another's is spent before it without a question: The Toolbox
//! before Cyberfeeder on a break, every pool before the credit pool. What is
//! left to ask is two pools neither of which is the other's lesser, and a
//! payment too small to take both: Azimat's credits pay only trash costs
//! and last the turn, a run's pay anything and are gone when it ends. The
//! payer says how many of the credits come from the first such class
//! (`Question`, a `PlayerAction::ChooseNumber`), to the credit. The
//! order used to be fixed — hosted credits, bad publicity, the run's, the
//! identity's, the credit pool — which drained Azimat on a run whose own
//! credits were about to vanish.
//!
//! **The question is parked by replay** (`state::PendingPayment`, `engine::
//! apply_action`): the payment unwinds the action with
//! `RulesError::PaymentChoiceNeeded`, the untouched state is returned with
//! the question on it, and the answer applies the action again with the
//! answer recorded in `GameState::payment_answers`. A payment is owed from
//! deep inside some thirty handlers; none of them had to learn to stop
//! halfway. Only the payer sees what the payment is for — the action has not
//! happened — in their view (`masking::PublicPendingPayment`) and in the log
//! (`masking::mask_logged_action_for_player`) alike.
//!
//! Rules Audit backlog item 5 (`docs/roadmap/rules-audit.md`).

use crate::cards::CardRegistry;
use crate::dsl::{card_matches_filter, CardDefinition, CardType, PaysFor};
use crate::rules::ability::{self, ResolutionContext};
use crate::rules::active::{self, Place};
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::state::{Credits, GameState, InstallId, InstallSlot, Side};

/// What a payment is for — said by the site that pays, because only it
/// knows. Only the purposes a card in the pool has a word for; everything
/// else is `Other`, which no restricted pool covers.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Purpose<'a> {
    /// A cost no card's credits are reserved for: a play cost, an ability,
    /// an advance, a steal cost, the basic actions.
    Other,
    /// Installing this card, by a click or by a card's text.
    Install(&'a CardDefinition),
    /// Rezzing this install.
    Rez(InstallId),
    /// The trash cost of the card the Runner is accessing.
    TrashCost,
    /// A trace attempt: either player's bid.
    Trace,
    /// The cost of a paid ability printed on this card. **Three sites state
    /// it and must all state it:** `engine::activate_ability`, which pays,
    /// and the two that ask ahead of time whether anybody could —
    /// `paid_ability::has_usable_paid_ability` (whether a window opens) and
    /// `prevention::could_prevent`. One of them left at `Other` is the
    /// disagreement stage 1 of this item was about: an ability only a
    /// console's credits could pay for, payable and never offered.
    Ability(&'a CardDefinition),
    /// The basic action that removes a tag.
    RemoveTag,
}

/// A place credits can be taken from. Serialisable because a parked payment
/// names the pool it is asking about (`PendingPayment::question`), and
/// public because a client words the question by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Pool {
    /// Credits hosted on an active installed card whose `pays_for` covers
    /// the purpose.
    Hosted(InstallId),
    /// `RunState::bad_publicity_credits` — the Runner's, for this run, on
    /// anything.
    BadPublicity,
    /// `RunState::bonus_run_credits` — what the card that began this run
    /// brought with it (Overclock), on anything during it.
    Run,
    /// Credits hosted on the Corp identity (`CorpState::identity_counters`)
    /// when its `pays_for` covers the purpose — NBN: Making News's, on
    /// trace attempts. A pool of its own only because an identity has no
    /// install handle for `Hosted` to name. The Runner's identity hosts
    /// nothing in this engine, and `CardDefinition::validate` refuses a
    /// Runner identity that prints recurring credits rather than let one
    /// silently do nothing.
    Identity,
    /// The credit pool itself. Always a source and always last.
    Wallet,
}

/// What a payment asks its payer: how many of its credits come from
/// `pool`'s class, from `min` to `max`. The rest comes from the other pools
/// that could have gone first, which `min` already makes sure can cover it.
/// Answered by `PlayerAction::ChooseNumber` (Rules Audit backlog item 6).
///
/// It was "which pool first?", answered by `ResolvePendingChoice`, with the
/// chosen class emptied as far as the payment went — a simplification item
/// 5 named, because a split needs a number for an answer and no decision
/// took one. A number says everything which-first could (all of it, or none
/// of it) and what it could not: one credit off Azimat and one off the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Question {
    /// The class asked about, named by its first pool in table order.
    pub pool: Pool,
    pub min: u32,
    pub max: u32,
}

/// What a parked payment asks its payer. Credits are asked about by
/// number (`Pools`, answered by `PlayerAction::ChooseNumber`); a cost that
/// takes cards is asked about **one card at a time** (`Card`, answered by
/// `PlayerAction::ToggleCardSelection` naming the card's position) —
/// Carnivore's "trash 2 cards from your grip", LEO Construction's "trash 1
/// rezzed bioroid", Anoetic Void's two cards from HQ.
///
/// One card at a time, with no confirm and no deselect, because each
/// answer is recorded the moment it is given and the action replayed with
/// it, exactly as a number is: a pick is progress, so a bot has nothing to
/// wander between, and `engine::completes` searches picks the way it
/// searches numbers. Toggle-then-confirm was the shape of a card *effect*
/// (`PendingDecision::ChooseCards`), which parks a selection that can be
/// edited; a replayed payment holds no state but its answers. A person
/// who wants a pick back takes the move back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Ask {
    Pools(Question),
    Card(CardQuestion),
    /// Which of a card's printed ways to pay for its own rez
    /// (`CardDefinition::rez_alternatives`) — Biawak's forfeit or its full
    /// price, Plutus's forfeit or its three cards — by index into that
    /// list, among the `offered` ones the Corp could complete. Answered by
    /// `PlayerAction::ResolvePendingChoice`. It was a `PresentChoice` of
    /// "pay, then rez" sequences, which is what kept the forfeit an effect.
    /// `card` is the card being rezzed, for the payer's prompt to read its
    /// ways off.
    Alternative { card: crate::dsl::CardId, offered: Vec<u32> },
}

impl Ask {
    /// Every answer the question admits, for `engine::completes` to try and
    /// `legal_actions` to offer.
    pub fn answers(&self) -> Vec<u32> {
        match self {
            Ask::Pools(question) => (question.min..=question.max).collect(),
            Ask::Card(question) => question.eligible.clone(),
            Ask::Alternative { offered, .. } => offered.clone(),
        }
    }

    /// The action that gives `answer` — one of `answers()`.
    pub fn action_for(&self, answer: u32) -> crate::rules::PlayerAction {
        use crate::rules::PlayerAction;
        match self {
            Ask::Pools(_) => PlayerAction::ChooseNumber { amount: answer },
            Ask::Card(_) => PlayerAction::ToggleCardSelection { position: answer as usize },
            Ask::Alternative { .. } => PlayerAction::ResolvePendingChoice { option_index: answer as usize },
        }
    }
}

/// Which card a cost takes next: one of `eligible`, positions in `zone` as
/// the payer sees it before anything is paid (`pending_choice::
/// zone_card_ids`), with the cards already picked left out. `remaining`
/// counts this pick.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CardQuestion {
    pub zone: crate::dsl::CardZoneRef,
    pub eligible: Vec<u32>,
    pub remaining: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Source {
    pub pool: Pool,
    pub credits: u32,
}

/// How long a pool's unspent credits last — the half of a pool's class that
/// says how soon "use it or lose it" bites. Ordered soonest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Life {
    /// Gone when the run ends: bad publicity's credits and a run event's.
    Run,
    /// Recurring credits: whatever is unspent is simply topped back up when
    /// its owner's next turn begins, so an unspent one was worth nothing.
    Turn,
    /// Kept until spent: Open Market's load, and the credit pool itself.
    Kept,
}

/// What a pool may be spent on — the other half of its class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Breadth {
    /// The words its card prints (`CardDefinition::pays_for`).
    Words(Vec<PaysFor>),
    /// Anything: the run's pools and the credit pool.
    Anything,
}

impl Breadth {
    /// Whether everything `self` may be spent on, `other` may be too.
    fn within(&self, other: &Breadth) -> bool {
        match (self, other) {
            (_, Breadth::Anything) => true,
            (Breadth::Anything, Breadth::Words(_)) => false,
            (Breadth::Words(mine), Breadth::Words(theirs)) => mine.iter().all(|word| theirs.contains(word)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Class {
    pub breadth: Breadth,
    pub life: Life,
}

impl Class {
    /// Whether a credit of `self` is never worth more than one of `other`:
    /// it can be spent on no more, and is lost no later. Spending `self`
    /// first then costs the payer nothing, so nobody is asked. Two pools
    /// that each say this of the other are one class.
    fn no_better_than(&self, other: &Class) -> bool {
        self.breadth.within(&other.breadth) && self.life <= other.life
    }
}

/// What `plan` settled on: how much from which pool, the credit pool last
/// and always present, and how many of the payer's answers it took to get
/// there — one action can ask more than once, and the next payment's
/// answers begin where this one's ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Planned {
    pub spend: Vec<(Pool, u32)>,
    pub answers_used: usize,
}

/// Which pools a payment of `amount` is taken from, and how much from each —
/// or, as the `Err`, what the payer has to say first.
///
/// **The payer is asked only when the answer changes what they are left
/// with.** A pool whose credits are never worth more than another's is
/// spent before it without a question (The Toolbox before Cyberfeeder on a
/// break; every pool before the credit pool), two pools of one class are
/// spent in table order, and when the payment is large enough to empty
/// every pool that could go first, how it is split between them is
/// nothing. What is left is two pools neither of which is the other's
/// lesser — Azimat's credits pay only trash costs but last the turn, the
/// run's pay anything and are gone when it ends — and a payment too small
/// to take both.
///
/// **Then the payer says how many come from the first of those classes**
/// (`Question`), and that class is done with: what it did not give stays
/// on its cards, and the loop goes on with the rest. The range is what the
/// payment allows — no more than the class holds or the payment needs, and
/// no fewer than the other classes that could go first would leave
/// uncovered — so every number in it is a payment that can be made, and
/// the last class is never asked about. `chosen` is what the payer has
/// already answered for this payment, in order; a number outside the range
/// is asked for again. A range that does not fit a `ChooseNumber`
/// (`MAX_CHOSEN_NUMBER`) is cut to it, and one that cannot be — the fewest
/// the class must give is more than a number can say — is not asked: the
/// classes go in table order, as two pools of one class do. No pool in the
/// game holds thirty credits another class cannot cover.
///
/// Pure: `entries` in, a plan out. `sources` and `class_of` read the state.
pub(crate) fn plan(entries: &[(Source, Class)], amount: u32, chosen: &[u32]) -> Result<Planned, Question> {
    let cap = crate::rules::action_mask::MAX_CHOSEN_NUMBER;
    let mut left: Vec<(Source, &Class)> = entries.iter().filter(|(source, _)| source.pool != Pool::Wallet).map(|(s, c)| (*s, c)).collect();
    let mut spent: Vec<(Pool, u32)> = Vec::new();
    let mut remaining = amount;
    let mut answers = chosen.iter();
    while remaining > 0 && !left.is_empty() {
        // The pools that could go first: nothing else is strictly their lesser.
        let strictly_less = |a: &Class, b: &Class| a.no_better_than(b) && !b.no_better_than(a);
        let first: Vec<usize> =
            (0..left.len()).filter(|&i| !left.iter().any(|(_, other)| strictly_less(other, left[i].1))).collect();
        // One class among them, or enough to empty them all: no question.
        let held: u32 = first.iter().map(|&i| left[i].0.credits).sum();
        let one_class = first.iter().all(|&i| left[i].1.no_better_than(left[first[0]].1) && left[first[0]].1.no_better_than(left[i].1));
        // The first class among them, and what the payment lets it give.
        let asked: Vec<usize> = first.iter().copied().filter(|&i| left[i].1 == left[first[0]].1).collect();
        let asked_holds: u32 = asked.iter().map(|&i| left[i].0.credits).sum();
        let fewest = remaining.saturating_sub(held - asked_holds);
        let (take, mut budget): (Vec<usize>, u32) = if one_class || remaining >= held || fewest > cap {
            (first, remaining)
        } else {
            let question = Question { pool: left[asked[0]].0.pool, min: fewest, max: asked_holds.min(remaining).min(cap) };
            match answers.next() {
                Some(&number) if (question.min..=question.max).contains(&number) => (asked, number),
                // No answer yet, or one the payment cannot be made with —
                // which is put again rather than bent into range.
                _ => return Err(question),
            }
        };
        for &i in &take {
            let spend = left[i].0.credits.min(budget);
            budget -= spend;
            remaining -= spend;
            if spend > 0 {
                spent.push((left[i].0.pool, spend));
            }
        }
        let mut index = 0;
        left.retain(|_| {
            let keep = !take.contains(&index);
            index += 1;
            keep
        });
    }
    spent.push((Pool::Wallet, remaining));
    Ok(Planned { spend: spent, answers_used: chosen.len() - answers.len() })
}

/// Whether credits a card says may be spent on `word` may be spent on
/// `purpose`. `host` is the install the credits are on — a Corp pool's word
/// can be about where that card sits ("this server") — and `None` for an
/// identity, which sits nowhere.
fn covers(word: &PaysFor, purpose: Purpose<'_>, host: Option<InstallId>, state: &GameState, registry: &CardRegistry) -> bool {
    match (word, purpose) {
        (PaysFor::TrashCosts, Purpose::TrashCost) => true,
        (PaysFor::TraceAttempts, Purpose::Trace) => true,
        (PaysFor::UsingIcebreakers, Purpose::Ability(card)) => card_matches_filter(card, &crate::dsl::CardFilter::Icebreaker),
        (PaysFor::RemovingTags, Purpose::RemoveTag) => true,
        (PaysFor::Installing(filter), Purpose::Install(card)) => card_matches_filter(card, filter),
        (PaysFor::RezzingInThisServer, Purpose::Rez(rezzing)) => {
            let installed = |id: InstallId| state.corp.installed.iter().find(|c| c.install_id == id);
            let (Some(host), Some(rezzing)) = (host.and_then(installed), installed(rezzing)) else { return false };
            // Assets in the root and ice protecting the server, as printed:
            // an upgrade being rezzed pays from the credit pool alone.
            let named = registry.get(&rezzing.card).is_some_and(|def| matches!(def.card_type, CardType::Ice(_) | CardType::Asset));
            named && host.slot == InstallSlot::Root && host.server == rezzing.server
        }
        (
            PaysFor::TrashCosts
            | PaysFor::Installing(_)
            | PaysFor::RezzingInThisServer
            | PaysFor::TraceAttempts
            | PaysFor::UsingIcebreakers
            | PaysFor::RemovingTags,
            _,
        ) => false,
    }
}

fn hosted_credits(state: &GameState, side: Side, install: InstallId) -> u32 {
    match side {
        Side::Corp => state.corp.installed.iter().find(|c| c.install_id == install).map_or(0, |c| c.counters),
        Side::Runner => state.runner.rig.iter().find(|c| c.install_id == install).map_or(0, |c| c.counters),
    }
}

/// Everywhere `side` could take credits from to pay for `purpose`, in the
/// order they are spent, each with what it holds. Empty pools are left out
/// but the credit pool, which is always the last entry.
pub(crate) fn sources(state: &GameState, registry: &CardRegistry, side: Side, purpose: Purpose<'_>) -> Vec<Source> {
    let mut sources = Vec::new();
    let mut push = |pool: Pool, credits: u32| {
        if credits > 0 {
            sources.push(Source { pool, credits });
        }
    };
    // Active cards only: an unrezzed upgrade's text is not live, and a
    // card's words about its hosted credits are text like any other.
    let hosts: Vec<InstallId> = match side {
        Side::Corp => active::corp(state, registry).filter(|card| card.place == Place::Installed).collect::<Vec<_>>(),
        Side::Runner => active::runner(state).filter(|card| card.place == Place::Installed).collect::<Vec<_>>(),
    }
    .into_iter()
    .filter_map(|card| {
        let install = card.install?;
        let definition = registry.get(card.card)?;
        definition.pays_for.iter().any(|word| covers(word, purpose, Some(install), state, registry)).then_some(install)
    })
    .collect();
    for install in hosts {
        push(Pool::Hosted(install), hosted_credits(state, side, install));
    }
    if let (Side::Runner, Some(run)) = (side, state.active_run.as_ref()) {
        push(Pool::BadPublicity, run.bad_publicity_credits);
        push(Pool::Run, run.bonus_run_credits);
    }
    let identity_pays = side == Side::Corp
        && state.corp.identity.as_ref().and_then(|identity| registry.get(identity)).is_some_and(|definition| {
            definition.pays_for.iter().any(|word| covers(word, purpose, None, state, registry))
        });
    if identity_pays {
        push(Pool::Identity, state.corp.identity_counters);
    }
    sources.push(Source { pool: Pool::Wallet, credits: state.resources(side).credits.0 });
    sources
}

/// Whether any payment made from `state` could have to ask — a cheap,
/// allocation-free *necessary* condition, for `engine::apply_action`, which
/// has to keep a copy of the action if it might park and would rather not
/// otherwise: the copy was measured at 2.4% of a whole heuristic pass, all
/// of stage 3's cost, because the legal-action probe applies every
/// candidate and most fail at the first guard.
///
/// **Sound because of what a source is.** `plan` asks only about two
/// non-wallet pools of different classes. Every non-wallet `Source` holds
/// credits (`sources` leaves empty ones out) and is one of: the run's two
/// pools, which are one class and so count once between them; the Corp
/// identity's hosted credits; a rezzed Corp install's counters; a rig
/// card's counters. Fewer than two of those on a side means fewer than two
/// classes for any purpose, and nothing to ask. It over-counts on purpose
/// — a counter need not be a credit, a pool need not cover the purpose —
/// since a wrong `true` costs one clone and a wrong `false` would lose the
/// question. Which side pays is not known until the action is applied, so
/// both are counted.
///
/// **A cost that takes cards** asks too (`Ask::Card`), and is paid by one
/// of three actions only: an activated ability (Carnivore, LEO
/// Construction), a parked paid choice accepted (Anoetic Void), and a rez
/// of a card that prints another way to pay for it (Biawak, Plutus: which
/// way is asked too, `Ask::Alternative`). The debug assertion in
/// `engine::apply_action` is what says so if a third appears.
pub(crate) fn could_ask(state: &GameState, registry: &CardRegistry, action: &crate::rules::PlayerAction) -> bool {
    pools_could_ask(state) || pays_a_cost_that_may_ask(state, registry, action)
}

fn pays_a_cost_that_may_ask(state: &GameState, registry: &CardRegistry, action: &crate::rules::PlayerAction) -> bool {
    use crate::rules::{InstallId, PlayerAction};
    match action {
        PlayerAction::ActivateAbility { target, ability_index } => {
            let card = match *target {
                InstallId::CORP_IDENTITY => state.corp.identity.as_ref(),
                InstallId::RUNNER_IDENTITY => state.runner.identity.as_ref(),
                target => state
                    .find_corp_install(target)
                    .map(|c| &c.card)
                    .or_else(|| state.corp.find_scored(target).map(|s| &s.card))
                    .or_else(|| state.find_rig_install(target).map(|c| &c.card)),
            };
            card.and_then(|card| registry.get(card))
                .and_then(|def| def.abilities.get(*ability_index))
                .and_then(|ability| ability.cost.as_ref())
                .is_some_and(crate::dsl::Cost::may_ask)
        }
        PlayerAction::AcceptPendingPaidChoice { .. } => state.pending_paid_choice.as_ref().is_some_and(|choice| choice.cost.may_ask()),
        // A card that prints another way to pay for its rez asks which,
        // and what the way it names takes.
        PlayerAction::RezIce { ice } => state
            .find_corp_install(*ice)
            .and_then(|installed| registry.get(&installed.card))
            .is_some_and(|def| !def.rez_alternatives.is_empty()),
        _ => false,
    }
}

fn pools_could_ask(state: &GameState) -> bool {
    let run_pool = state.active_run.as_ref().is_some_and(|run| run.bad_publicity_credits > 0 || run.bonus_run_credits > 0);
    let runner = usize::from(run_pool) + state.runner.rig.iter().filter(|card| card.counters > 0).count();
    let corp = usize::from(state.corp.identity_counters > 0) + state.corp.installed.iter().filter(|card| card.rezzed && card.counters > 0).count();
    runner >= 2 || corp >= 2
}

/// A pool's class, read off the state: what its credits may be spent on and
/// how long they last. A hosted pool's breadth is the words its card prints,
/// and it lasts the turn if the card prints recurring credits
/// (`CardDefinition::recurring_credits` — what is unspent is only topped
/// back up) and until spent otherwise (Open Market's load).
fn class_of(state: &GameState, registry: &CardRegistry, side: Side, pool: Pool) -> Class {
    let of_card = |card: Option<&crate::dsl::CardId>| {
        let definition = card.and_then(|card| registry.get(card));
        Class {
            breadth: Breadth::Words(definition.map(|d| d.pays_for.clone()).unwrap_or_default()),
            life: if definition.is_some_and(|d| d.recurring_credits.is_some()) { Life::Turn } else { Life::Kept },
        }
    };
    match pool {
        Pool::Hosted(install) => of_card(match side {
            Side::Corp => state.corp.installed.iter().find(|c| c.install_id == install).map(|c| &c.card),
            Side::Runner => state.runner.rig.iter().find(|c| c.install_id == install).map(|c| &c.card),
        }),
        Pool::Identity => of_card(state.corp.identity.as_ref()),
        Pool::BadPublicity | Pool::Run => Class { breadth: Breadth::Anything, life: Life::Run },
        Pool::Wallet => Class { breadth: Breadth::Anything, life: Life::Kept },
    }
}

/// How many credits `side` could put towards `purpose` — the one
/// affordability question, asked of the same scan `pay` spends from.
pub(crate) fn available(state: &GameState, registry: &CardRegistry, side: Side, purpose: Purpose<'_>) -> u32 {
    sources(state, registry, side, purpose).iter().fold(0u32, |total, source| total.saturating_add(source.credits))
}

/// Pays `amount` credits for `purpose`. `RulesError::NotEnoughCredits` —
/// before anything is spent — if `sources` do not hold it, and
/// `RulesError::PaymentChoiceNeeded`, also before anything is spent, if the
/// payer has to say how many come from which pool and has not (`plan`); `engine::
/// apply_action` turns that into a parked `PendingPayment`. The payer's
/// answers are taken from the front of `GameState::payment_answers`.
pub(crate) fn pay(
    state: &mut GameState,
    registry: &CardRegistry,
    side: Side,
    amount: u32,
    purpose: Purpose<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    let sources = sources(state, registry, side, purpose);
    let total = sources.iter().fold(0u32, |total, source| total.saturating_add(source.credits));
    if total < amount {
        return Err(RulesError::NotEnoughCredits { side, available: total, requested: amount });
    }
    let entries: Vec<(Source, Class)> = sources.iter().map(|source| (*source, class_of(state, registry, side, source.pool))).collect();
    let planned = plan(&entries, amount, &state.payment_answers).map_err(|question| RulesError::PaymentChoiceNeeded { side, amount, question: Ask::Pools(question) })?;
    state.payment_answers.drain(..planned.answers_used);

    let mut events = Vec::new();
    let mut from_hosted = 0;
    for (pool, spend) in planned.spend {
        match pool {
            // Reported even at 0: `CreditsSpent` is how the record says a
            // cost was paid at all. Its amount is what did not come off a
            // card — the run's credits are reported beside it, a hosted
            // credit by the counter that left its host.
            Pool::Wallet => {
                let held = state.resources(side).credits.0;
                state.resources_mut(side).credits = Credits(held - spend);
                events.push(GameEvent::CreditsSpent { side, amount: amount - from_hosted });
            }
            Pool::Hosted(install) => {
                from_hosted += spend;
                let emptied = hosted_credits(state, side, install) == spend;
                events.extend(spend_hosted(state, registry, side, install, spend, emptied)?);
            }
            Pool::BadPublicity => {
                state.active_run.as_mut().expect("a bad publicity source implies an active run").bad_publicity_credits -= spend;
                events.push(GameEvent::BadPublicityCreditsSpent { amount: spend });
            }
            Pool::Run => {
                state.active_run.as_mut().expect("a run-credit source implies an active run").bonus_run_credits -= spend;
                events.push(GameEvent::BonusRunCreditsSpent { amount: spend });
            }
            Pool::Identity => {
                from_hosted += spend;
                let identity = state.corp.identity.clone().ok_or(RulesError::MissingActingCardContext)?;
                events.extend(ability::modify_counters(state, &ResolutionContext::for_card(Some(&identity)), -i64::from(spend))?);
            }
        }
    }
    Ok(events)
}

/// Comprehensive Rules 1.10.5a, second half: "Before abilities meet their
/// trigger conditions for your turn beginning, if there are fewer than N
/// credits on this card, place credits on it until there are N credits on
/// it" — for every active card of `side` that prints recurring credits.
/// Called by `turn` ahead of the `TurnStarted` dispatch, which is what
/// "before" means here: a "when your turn begins" ability resolves against
/// pools already refilled, and the refill is nobody's to order.
///
/// Refills *up to* N and never down (1.10.5d, "do not accumulate" — and a
/// card holding more than N by some other text keeps them).
pub(crate) fn refill(state: &mut GameState, registry: &CardRegistry, side: Side) -> Result<Vec<GameEvent>, RulesError> {
    let recurring = |card: &crate::dsl::CardId| registry.get(card).and_then(|definition| definition.recurring_credits);
    let due: Vec<(Option<InstallId>, crate::dsl::CardId, u32)> = match side {
        Side::Corp => active::corp(state, registry).collect::<Vec<_>>(),
        Side::Runner => active::runner(state).collect::<Vec<_>>(),
    }
    .into_iter()
    // A scored agenda's counters are agenda counters, and the Runner's
    // identity hosts nothing: neither prints recurring credits in this
    // pool, and `validate` refuses the second.
    .filter(|card| card.place == Place::Installed || (card.place == Place::Identity && side == Side::Corp))
    .filter_map(|card| recurring(card.card).map(|credits| (card.install, card.card.clone(), credits)))
    .collect();
    let mut events = Vec::new();
    for (install, card, credits) in due {
        events.extend(raise_to(state, side, install, &card, credits)?);
    }
    Ok(events)
}

/// Comprehensive Rules 1.10.5a, first half, and 1.10.5b: "When this card
/// becomes active, place N credits on it" — called where a card does:
/// `engine::seed_rig_card`'s callers for the Runner, `engine::rez_install`
/// for the Corp (every Corp install is constructed facedown, and that is
/// the one place one is turned faceup), and `setup` for the identity.
/// `install` is `None` for an identity. Nothing for a card that prints no
/// recurring credits.
pub(crate) fn place_recurring(
    state: &mut GameState,
    registry: &CardRegistry,
    side: Side,
    install: Option<InstallId>,
    card: &crate::dsl::CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    match registry.get(card).and_then(|definition| definition.recurring_credits) {
        Some(credits) => raise_to(state, side, install, card, credits),
        None => Ok(Vec::new()),
    }
}

fn raise_to(
    state: &mut GameState,
    side: Side,
    install: Option<InstallId>,
    card: &crate::dsl::CardId,
    credits: u32,
) -> Result<Vec<GameEvent>, RulesError> {
    let held = match install {
        Some(install) => hosted_credits(state, side, install),
        None => state.corp.identity_counters,
    };
    if held >= credits {
        return Ok(Vec::new());
    }
    let ctx = match install {
        Some(install) => ResolutionContext::for_parked(Some(install), Some(card)),
        None => ResolutionContext::for_card(Some(card)),
    };
    ability::modify_counters(state, &ctx, i64::from(credits - held))
}

/// Takes `spend` hosted credits off `install` — through the path
/// `Cost::RemoveCounters` uses, so the record reads the same — and trashes
/// a card that says it is trashed when empty (`CardDefinition::
/// trash_when_empty`, Open Market) if that emptied it. No card text runs
/// for a payment, which is why the flag is the engine's to honour here.
fn spend_hosted(
    state: &mut GameState,
    registry: &CardRegistry,
    side: Side,
    install: InstallId,
    spend: u32,
    emptied: bool,
) -> Result<Vec<GameEvent>, RulesError> {
    let card = match side {
        Side::Corp => state.corp.installed.iter().find(|c| c.install_id == install).map(|c| c.card.clone()),
        Side::Runner => state.runner.rig.iter().find(|c| c.install_id == install).map(|c| c.card.clone()),
    }
    .ok_or(RulesError::InstallNotFound(install))?;
    let ctx = ResolutionContext::for_parked(Some(install), Some(&card));
    let mut events = ability::modify_counters(state, &ctx, -i64::from(spend))?;
    if emptied && registry.get(&card).is_some_and(|definition| definition.trash_when_empty) {
        events.extend(ability::trash_this_card(state, &ctx)?);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::register_playable_cards;
    use crate::dsl::{CardId, Cost};
    use crate::rules::engine;
    use crate::rules::run::RunState;
    use crate::rules::state::{Clicks, GamePhase, InstalledRunnerCard};
    use crate::rules::test_support::fixture_install_id;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        registry
    }

    fn runner_turn(credits: u32) -> GameState {
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.runner.resources.credits = Credits(credits);
        state.runner.resources.clicks = Clicks(4);
        state
    }

    /// `cost_is_affordable` is the question a paid-ability window and a
    /// prevention are opened on. It counted bad publicity and forgot the
    /// credits the run brought (Overclock).
    #[test]
    fn a_cost_the_runs_credits_cover_is_affordable_where_it_is_asked_ahead_of_the_payment() {
        let registry = registry();
        let mut state = runner_turn(0);
        state.active_run = Some(RunState { bonus_run_credits: 3, ..Default::default() });
        let ctx = ResolutionContext::default();
        let affordable = |cost: u32| ability::cost_is_affordable(&state, &registry, Side::Runner, &Cost::Credits(cost), Purpose::Other, &ctx);
        assert!(affordable(3));
        assert!(!affordable(4));
    }

    /// Only the click install drained the market, and the text install's
    /// own affordability check counted the credit pool alone.
    #[test]
    fn open_market_pays_for_a_job_a_cards_text_installs() {
        let registry = registry();
        let mut state = runner_turn(0);
        state.runner.rig = vec![InstalledRunnerCard {
            install_id: fixture_install_id("open_market"),
            card: CardId("open_market".to_string()),
            counters: 6,
            ..Default::default()
        }];
        let telework = CardId("telework_contract".to_string());
        state.runner.grip = vec![telework.clone()];
        assert!(
            engine::can_install_runner_card_from_grip(&state, &registry, &telework),
            "\"You can spend hosted credits to install connection and job resources\" says nothing of a click"
        );
        engine::install_runner_card_from_grip_paying_cost(&mut state, &registry, telework).expect("a 1[c] Job, paid from the market");
        assert_eq!(state.runner.rig[0].counters, 5);
        assert_eq!(state.runner.resources.credits, Credits(0));
    }

    #[test]
    fn the_credit_pool_is_always_the_last_source_and_a_restricted_pool_is_one_only_for_its_purpose() {
        let registry = registry();
        let mut state = runner_turn(4);
        let azimat = fixture_install_id("azimat");
        state.runner.rig = vec![InstalledRunnerCard { install_id: azimat, card: CardId("azimat".to_string()), counters: 2, ..Default::default() }];
        state.active_run = Some(RunState { bad_publicity_credits: 1, ..Default::default() });
        assert_eq!(
            sources(&state, &registry, Side::Runner, Purpose::TrashCost),
            vec![
                Source { pool: Pool::Hosted(azimat), credits: 2 },
                Source { pool: Pool::BadPublicity, credits: 1 },
                Source { pool: Pool::Wallet, credits: 4 },
            ]
        );
        assert_eq!(
            sources(&state, &registry, Side::Runner, Purpose::Other),
            vec![Source { pool: Pool::BadPublicity, credits: 1 }, Source { pool: Pool::Wallet, credits: 4 }],
            "Azimat's credits pay trash costs and nothing else"
        );
        assert_eq!(sources(&state, &registry, Side::Corp, Purpose::Other), vec![Source { pool: Pool::Wallet, credits: 0 }]);
    }

    // `plan` is pure, so its rules are pinned here without an engine.

    fn spend(planned: Result<Planned, Question>) -> Result<Vec<(Pool, u32)>, Question> {
        planned.map(|planned| planned.spend)
    }
    fn words(list: &[PaysFor]) -> Breadth {
        Breadth::Words(list.to_vec())
    }
    fn entry(pool: Pool, credits: u32, breadth: Breadth, life: Life) -> (Source, Class) {
        (Source { pool, credits }, Class { breadth, life })
    }
    fn wallet(credits: u32) -> (Source, Class) {
        entry(Pool::Wallet, credits, Breadth::Anything, Life::Kept)
    }
    const AZIMAT: Pool = Pool::Hosted(InstallId(1));
    const TOOLBOX: Pool = Pool::Hosted(InstallId(2));
    const FEEDER: Pool = Pool::Hosted(InstallId(3));
    fn azimat(credits: u32) -> (Source, Class) {
        entry(AZIMAT, credits, words(&[PaysFor::TrashCosts]), Life::Turn)
    }
    fn bad_publicity(credits: u32) -> (Source, Class) {
        entry(Pool::BadPublicity, credits, Breadth::Anything, Life::Run)
    }
    fn run_credits(credits: u32) -> (Source, Class) {
        entry(Pool::Run, credits, Breadth::Anything, Life::Run)
    }

    #[test]
    fn a_payment_with_only_the_credit_pool_is_never_a_question() {
        assert_eq!(spend(plan(&[wallet(5)], 3, &[])), Ok(vec![(Pool::Wallet, 3)]));
        assert_eq!(spend(plan(&[wallet(5)], 0, &[])), Ok(vec![(Pool::Wallet, 0)]));
    }

    #[test]
    fn a_pool_that_is_never_worth_more_is_spent_first_without_asking() {
        // Narrower and no longer-lived: before the broader pool, then the wallet.
        let narrow = entry(TOOLBOX, 2, words(&[PaysFor::TrashCosts]), Life::Turn);
        let broad = entry(FEEDER, 1, words(&[PaysFor::TrashCosts, PaysFor::TraceAttempts]), Life::Turn);
        assert_eq!(spend(plan(&[broad.clone(), narrow.clone(), wallet(9)], 1, &[])), Ok(vec![(TOOLBOX, 1), (Pool::Wallet, 0)]));
        assert_eq!(spend(plan(&[broad, narrow, wallet(9)], 4, &[])), Ok(vec![(TOOLBOX, 2), (FEEDER, 1), (Pool::Wallet, 1)]));
    }

    #[test]
    fn two_pools_of_one_class_are_spent_in_table_order() {
        assert_eq!(spend(plan(&[bad_publicity(2), run_credits(5), wallet(1)], 3, &[])), Ok(vec![(Pool::BadPublicity, 2), (Pool::Run, 1), (Pool::Wallet, 0)]));
    }

    fn ask(pool: Pool, min: u32, max: u32) -> Question {
        Question { pool, min, max }
    }

    #[test]
    fn two_pools_neither_the_others_lesser_are_a_question_when_the_payment_cannot_take_both() {
        // Azimat pays less but lasts longer; the run's credits pay anything and are gone sooner.
        let table = [azimat(2), run_credits(5), wallet(3)];
        assert_eq!(plan(&table, 2, &[]), Err(ask(AZIMAT, 0, 2)));
        assert_eq!(spend(plan(&table, 2, &[2])), Ok(vec![(AZIMAT, 2), (Pool::Wallet, 0)]));
        assert_eq!(spend(plan(&table, 2, &[0])), Ok(vec![(Pool::Run, 2), (Pool::Wallet, 0)]));
        // What which-first could not say, and the reason for the number.
        assert_eq!(spend(plan(&table, 2, &[1])), Ok(vec![(AZIMAT, 1), (Pool::Run, 1), (Pool::Wallet, 0)]));
    }

    #[test]
    fn the_range_is_what_the_payment_allows() {
        let table = [azimat(2), run_credits(5), wallet(3)];
        // 6 of the 7 they hold: the run can cover 5, so Azimat gives at least 1.
        assert_eq!(plan(&table, 6, &[]), Err(ask(AZIMAT, 1, 2)));
        assert_eq!(spend(plan(&table, 6, &[1])), Ok(vec![(AZIMAT, 1), (Pool::Run, 5), (Pool::Wallet, 0)]));
        // 1 credit: Azimat holds 2 and may give no more than is owed.
        assert_eq!(plan(&table, 1, &[]), Err(ask(AZIMAT, 0, 1)));
        // The credit pool is never part of the split: every other pool goes before it.
        assert_eq!(spend(plan(&table, 6, &[2])), Ok(vec![(AZIMAT, 2), (Pool::Run, 4), (Pool::Wallet, 0)]));
    }

    #[test]
    fn nobody_is_asked_when_the_payment_empties_every_pool_that_could_go_first() {
        let table = [azimat(2), run_credits(5), wallet(3)];
        assert_eq!(spend(plan(&table, 7, &[])), Ok(vec![(AZIMAT, 2), (Pool::Run, 5), (Pool::Wallet, 0)]));
        assert_eq!(spend(plan(&table, 9, &[])), Ok(vec![(AZIMAT, 2), (Pool::Run, 5), (Pool::Wallet, 2)]));
    }

    #[test]
    fn a_question_is_about_a_class_and_bad_publicity_and_the_runs_credits_are_one() {
        let table = [bad_publicity(1), azimat(2), run_credits(5), wallet(0)];
        assert_eq!(plan(&table, 3, &[]), Err(ask(Pool::BadPublicity, 1, 3)), "the class holds 6, named by its first pool; Azimat covers 2 at most");
        assert_eq!(spend(plan(&table, 3, &[2])), Ok(vec![(Pool::BadPublicity, 1), (Pool::Run, 1), (AZIMAT, 1), (Pool::Wallet, 0)]), "table order inside the class");
    }

    #[test]
    fn three_classes_are_two_questions_and_the_last_is_never_asked_about() {
        let icebreakers = entry(TOOLBOX, 2, words(&[PaysFor::UsingIcebreakers]), Life::Turn);
        let table = [azimat(2), icebreakers, run_credits(2), wallet(0)];
        assert_eq!(plan(&table, 3, &[]), Err(ask(AZIMAT, 0, 2)));
        assert_eq!(plan(&table, 3, &[1]), Err(ask(TOOLBOX, 0, 2)), "2 still owed, and the run could cover it all");
        assert_eq!(spend(plan(&table, 3, &[1, 1])), Ok(vec![(AZIMAT, 1), (TOOLBOX, 1), (Pool::Run, 1), (Pool::Wallet, 0)]));
        assert_eq!(spend(plan(&table, 3, &[0, 1])), Ok(vec![(TOOLBOX, 1), (Pool::Run, 2), (Pool::Wallet, 0)]));
        assert_eq!(plan(&table, 3, &[0]), Err(ask(TOOLBOX, 1, 2)), "with Azimat out of it the other two hold 4 of the 3");
    }

    #[test]
    fn a_number_outside_the_range_is_asked_for_again_and_the_plan_still_ends() {
        let table = [azimat(2), run_credits(5), wallet(3)];
        assert_eq!(plan(&table, 6, &[0]), Err(ask(AZIMAT, 1, 2)), "0 would leave a credit nobody can pay");
        assert_eq!(plan(&table, 6, &[3]), Err(ask(AZIMAT, 1, 2)), "more than the pool holds");
    }

    #[test]
    fn a_range_a_number_cannot_say_is_cut_to_one_it_can_or_not_asked() {
        let cap = crate::rules::action_mask::MAX_CHOSEN_NUMBER;
        let table = [azimat(cap + 10), run_credits(cap + 10), wallet(0)];
        assert_eq!(plan(&table, cap + 5, &[]), Err(ask(AZIMAT, 0, cap)), "cut to what `ChooseNumber` can carry");
        // Azimat must give at least cap + 5: no number says so, so table order.
        assert_eq!(
            spend(plan(&table, 2 * cap + 15, &[])),
            Ok(vec![(AZIMAT, cap + 10), (Pool::Run, cap + 5), (Pool::Wallet, 0)])
        );
    }

    #[test]
    fn a_plan_says_how_many_answers_it_took_so_the_next_payment_starts_after_them() {
        let table = [azimat(2), run_credits(5), wallet(3)];
        assert_eq!(plan(&table, 2, &[2, 0]).map(|p| p.answers_used), Ok(1), "one question, one answer taken; the second is the next payment's");
        assert_eq!(plan(&table, 7, &[2]).map(|p| p.answers_used), Ok(0), "no question, so the answer is left for whoever asks");
    }

    #[test]
    fn a_question_is_possible_only_with_two_places_that_could_hold_credits_on_one_side() {
        let rig_card = |counters| InstalledRunnerCard { install_id: fixture_install_id("azimat"), card: CardId("azimat".to_string()), counters, ..Default::default() };
        let mut state = runner_turn(9);
        assert!(!pools_could_ask(&state), "a credit pool and nothing else");
        state.runner.rig = vec![rig_card(2)];
        assert!(!pools_could_ask(&state), "one hosted pool: it goes before the credit pool, unasked");
        state.active_run = Some(RunState { bad_publicity_credits: 1, bonus_run_credits: 5, ..Default::default() });
        assert!(pools_could_ask(&state), "a hosted pool and the run's");
        state.runner.rig = vec![rig_card(0)];
        assert!(!pools_could_ask(&state), "the run's two pools are one class, and an empty host is no pool");
        state.corp.identity_counters = 2;
        assert!(!pools_could_ask(&state), "one place on each side is not two on either");
    }

    fn rig_card(id: &str, counters: u32) -> InstalledRunnerCard {
        InstalledRunnerCard { install_id: fixture_install_id(id), card: CardId(id.to_string()), counters, ..Default::default() }
    }

    #[test]
    fn credits_for_using_icebreakers_pay_for_an_icebreakers_ability_and_no_other_cards() {
        let registry = registry();
        let mut state = runner_turn(0);
        state.runner.rig = vec![rig_card("cyberfeeder", 1)];
        let feeder = Pool::Hosted(fixture_install_id("cyberfeeder"));
        let of = |id: &str| registry.get(&CardId(id.to_string())).expect("in the pool");
        let pools = |purpose| sources(&state, &registry, Side::Runner, purpose).into_iter().map(|source| source.pool).collect::<Vec<_>>();
        assert_eq!(pools(Purpose::Ability(of("corroder"))), vec![feeder, Pool::Wallet], "a fracter's pump or break");
        assert_eq!(pools(Purpose::Ability(of("madani"))), vec![Pool::Wallet], "a console's ability is not an icebreaker's");
        assert_eq!(pools(Purpose::Install(of("hantu"))), vec![feeder, Pool::Wallet], "a virus program");
        assert_eq!(pools(Purpose::Install(of("corroder"))), vec![Pool::Wallet], "a program that is no virus");
        assert_eq!(pools(Purpose::RemoveTag), vec![Pool::Wallet]);
    }

    /// `Purpose::Ability` is stated where an ability is paid for and at the
    /// two places that ask ahead of time whether anybody could pay. Here is
    /// one of those two: with an empty wallet, whether a window opens for
    /// Corroder turns on whether Cyberfeeder's credit counts.
    #[test]
    fn a_window_opens_for_an_icebreaker_ability_only_a_pool_could_pay_for() {
        let registry = registry();
        let mut state = runner_turn(0);
        state.runner.rig = vec![rig_card("corroder", 0)];
        state.active_run = Some(RunState { phase: crate::rules::run::RunPhase::EncounterIce, ..Default::default() });
        let usable = |state: &GameState| crate::rules::paid_ability::has_usable_paid_ability(state, &registry, Side::Runner);
        let broke = usable(&state);
        state.runner.rig.push(rig_card("cyberfeeder", 1));
        assert!(usable(&state), "Cyberfeeder's credit pays for the pump");
        assert!(!broke, "and without it nothing could: that the first assertion means something");
    }
}
