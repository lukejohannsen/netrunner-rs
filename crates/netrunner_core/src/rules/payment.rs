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
//! **The order is fixed, for now:** hosted credits in table order, bad
//! publicity, the run's credits, the identity's, then the credit pool —
//! the order the old sites added up to, kept so this module moves no game
//! by itself. Which pool a payment is taken from *first* is the payer's to
//! say when it matters, and is the next stage of Rules Audit backlog item 5
//! (`docs/roadmap/rules-audit.md`).

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
}

/// A place credits can be taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pool {
    /// Credits hosted on an active installed card whose `pays_for` covers
    /// the purpose.
    Hosted(InstallId),
    /// `RunState::bad_publicity_credits` — the Runner's, for this run, on
    /// anything.
    BadPublicity,
    /// `RunState::bonus_run_credits` — what the card that began this run
    /// brought with it (Overclock), on anything during it.
    Run,
    /// `CorpState::recurring_credits` — the Corp identity's, on trace
    /// attempts (NBN: Making News).
    Identity,
    /// The credit pool itself. Always a source and always last.
    Wallet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Source {
    pub pool: Pool,
    pub credits: u32,
}

/// Whether credits a card says may be spent on `word` may be spent on
/// `purpose`. `host` is the install the credits are on — a Corp pool's word
/// can be about where that card sits ("this server").
fn covers(word: &PaysFor, purpose: Purpose<'_>, host: InstallId, state: &GameState, registry: &CardRegistry) -> bool {
    match (word, purpose) {
        (PaysFor::TrashCosts, Purpose::TrashCost) => true,
        (PaysFor::Installing(filter), Purpose::Install(card)) => card_matches_filter(card, filter),
        (PaysFor::RezzingInThisServer, Purpose::Rez(rezzing)) => {
            let installed = |id: InstallId| state.corp.installed.iter().find(|c| c.install_id == id);
            let (Some(host), Some(rezzing)) = (installed(host), installed(rezzing)) else { return false };
            // Assets in the root and ice protecting the server, as printed:
            // an upgrade being rezzed pays from the credit pool alone.
            let named = registry.get(&rezzing.card).is_some_and(|def| matches!(def.card_type, CardType::Ice(_) | CardType::Asset));
            named && host.slot == InstallSlot::Root && host.server == rezzing.server
        }
        (PaysFor::TrashCosts | PaysFor::Installing(_) | PaysFor::RezzingInThisServer, _) => false,
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
        definition.pays_for.iter().any(|word| covers(word, purpose, install, state, registry)).then_some(install)
    })
    .collect();
    for install in hosts {
        push(Pool::Hosted(install), hosted_credits(state, side, install));
    }
    if let (Side::Runner, Some(run)) = (side, state.active_run.as_ref()) {
        push(Pool::BadPublicity, run.bad_publicity_credits);
        push(Pool::Run, run.bonus_run_credits);
    }
    if let (Side::Corp, Purpose::Trace) = (side, purpose) {
        push(Pool::Identity, state.corp.recurring_credits);
    }
    sources.push(Source { pool: Pool::Wallet, credits: state.resources(side).credits.0 });
    sources
}

/// How many credits `side` could put towards `purpose` — the one
/// affordability question, asked of the same scan `pay` spends from.
pub(crate) fn available(state: &GameState, registry: &CardRegistry, side: Side, purpose: Purpose<'_>) -> u32 {
    sources(state, registry, side, purpose).iter().fold(0u32, |total, source| total.saturating_add(source.credits))
}

/// Pays `amount` credits for `purpose`, from `sources` in order.
/// `RulesError::NotEnoughCredits` — before anything is spent — if they do
/// not hold it.
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
    let mut events = Vec::new();
    let mut remaining = amount;
    let mut from_hosted = 0;
    for source in sources {
        let spend = source.credits.min(remaining);
        remaining -= spend;
        match source.pool {
            // Reported even at 0: `CreditsSpent` is how the record says a
            // cost was paid at all. Its amount is what did not come off a
            // card — the run's and the identity's credits are reported
            // beside it, a hosted credit by the counter that left its host.
            Pool::Wallet => {
                state.resources_mut(side).credits = Credits(source.credits - spend);
                events.push(GameEvent::CreditsSpent { side, amount: amount - from_hosted });
            }
            _ if spend == 0 => {}
            Pool::Hosted(install) => {
                from_hosted += spend;
                events.extend(spend_hosted(state, registry, side, install, spend, spend == source.credits)?);
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
                state.corp.recurring_credits -= spend;
                events.push(GameEvent::RecurringCreditsSpent { amount: spend });
            }
        }
    }
    Ok(events)
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
}
