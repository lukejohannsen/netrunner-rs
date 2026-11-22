//! A current in play, read as what it adds to the prices both players
//! pay for as long as it lasts (Phase 5 §83).
//!
//! **Why.** A current is played for what it does on later turns — the
//! Runner's first run each turn costs 1[credit] more, each resource costs
//! 2[credit] more to install — and the line a seat plans ends with its
//! own turn, so a current read as its price and a click spent for
//! nothing: over the whole pool's pass on two seeds (13 November 2026)
//! random seats played the twenty-three currents on the blind list 5 to
//! 63 times each and the planner none of them, on either chair. Nothing
//! read the Runner's play area at all (`RunnerState::in_play`), and the
//! Corp's was read for a lockdown's run triggers and its ice strength
//! only (§58, §79).
//!
//! **A price is asked of the table with the current and without it**, as
//! `corp::lockdown_break_tax` asks a break's: the engine's own questions
//! (`continuous::run_costs`, `install_cost_of`, `play_cost_of`,
//! `advancement_requirement`, `rez_costs`), so a current is never matched
//! by name and one that prints a price this module has no question for
//! reads as nothing rather than as a guess. Both tables are asked as of a
//! turn that has not begun: Service Outage's "the first run each turn"
//! read off the turn under way would be worth nothing from the Runner's
//! first run to the end of its turn, and a run would read as buying the
//! current off.
//!
//! **Both chairs read one toll.** An evaluation is one side's, not a
//! difference of two, so a current is worth to its owner what it takes
//! from the other chair and costs the other chair what it takes — and
//! playing a current over the opponent's is worth the toll that leaves
//! with it, which no term has to say.
//!
//! This stage reads the prices. What a current prints as a trigger
//! (Manhunt's trace, Rolling Brownout's credit), a blanked identity and
//! the rest are later stages of the same entry; Interdiction's "cannot
//! rez non-ice cards during the Runner's turn" is a prohibition, not a
//! price, and is read with those.

use super::Weights;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardDefinition, CardId, CardSubtype, CardType, ContinuousKind, Cost, RecurringCredits};
use netrunner_core::rules::continuous;
use netrunner_core::rules::turn_log::TurnLog;
use netrunner_core::rules::{CorpState, GameState, InstallSlot, RunnerState, ServerId, Side};

/// The turns of its opponent's a current is in play for: until that
/// opponent steals or scores an agenda, or the game ends. One number for
/// both chairs, because two measurements of planner seats disagree on
/// which chair's lasts longer and agree on about four. Over the whole
/// pool's pass (2,508 games a seed, seeds 2 and 3, 13 November 2026) a
/// game is 14.1 Runner turns and 3.0 steals, 3.5 Runner turns to a steal
/// or the end, and 14.5 Corp turns and 1.8 scores, 5.2 Corp turns to a
/// score or the end. Over the first 50 Standard matchups on seed 2,
/// counted from every turn of every game to the agenda that would end a
/// current played on it, 4.9 Runner turns for a Corp current and 3.9
/// Corp turns for a Runner one. A rival current ends one sooner, which
/// neither could hold — no planner played one — so this is the long side
/// of the truth.
const CURRENT_LIFE: u32 = 4;

/// The cards a player draws a turn while a current lasts, for what it
/// adds to the price of cards not yet held: the Corp's mandatory draw,
/// and a Runner who draws less than a card a turn has nothing left to
/// install. A floor, so a current is not played for cards its opponent
/// may never see.
const DRAWS_A_TURN: f64 = 1.0;

/// What the currents in play take from each player over the turns they
/// are expected to last, in the units the players pay in. A positive
/// number is taken from that player; a negative one is given.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Toll {
    pub runner_credits: f64,
    pub runner_clicks: f64,
    pub corp_credits: f64,
    pub corp_clicks: f64,
    /// Cards out of HQ (Hacktivist Meeting's "trash 1 card from HQ at
    /// random" to rez).
    pub corp_cards: f64,
    /// The part of `runner_credits` that is the installs in the grip. The
    /// Runner's own chair leaves it out: `runner::held_price` already
    /// prices a held install on the table as it is, current and all, so
    /// read here too it was charged twice (found in review of §83).
    pub runner_held_installs: f64,
    /// The part of `corp_credits` and of `corp_clicks` that is installed
    /// agendas' added advancements. The Corp's own chair leaves it out
    /// for the same reason: its terms ask
    /// `continuous::advancement_requirement` of the table as it is.
    pub corp_advances: f64,
}

fn definition<'a>(registry: &'a CardRegistry, id: &CardId) -> Option<&'a CardDefinition> {
    registry.get(id).filter(|def| def.subtypes.contains(&CardSubtype::Current))
}

/// Whether `def` prints a price one of this module's questions asks, or
/// credits of its own — so a board whose current prints neither is read
/// without a copy of the state.
fn prices(def: &CardDefinition) -> bool {
    def.recurring_credits.is_some()
        || def.continuous.iter().any(|effect| {
            matches!(
                effect.kind,
                ContinuousKind::RunCost(_) | ContinuousKind::InstallCost(_) | ContinuousKind::PlayCost(_) | ContinuousKind::AdvancementRequirement(_) | ContinuousKind::AdditionalRezCost(_)
            )
        })
}

/// `state` as a turn that has not begun, for a price that is "the first
/// each turn".
fn as_a_new_turn(state: &GameState) -> GameState {
    GameState { this_turn: TurnLog::default(), ..state.clone() }
}

/// `state` with no current in either play area; a lockdown stays.
fn without_currents(state: &GameState, registry: &CardRegistry) -> GameState {
    let keep = |played: &&netrunner_core::rules::PlayedOperation| definition(registry, &played.card).is_none();
    GameState {
        corp: CorpState { play_area: state.corp.play_area.iter().filter(keep).cloned().collect(), ..state.corp.clone() },
        runner: RunnerState { in_play: state.runner.in_play.iter().filter(keep).cloned().collect(), ..state.runner.clone() },
        ..state.clone()
    }
}

/// What a run on `server` costs beside its ice: credits and clicks. A
/// cost in anything else is not one this reading prices.
fn run_price(state: &GameState, registry: &CardRegistry, server: ServerId) -> (u32, u32) {
    fn add(cost: &Cost, sum: &mut (u32, u32)) {
        match cost {
            Cost::Credits(n) => sum.0 += n,
            Cost::Clicks(n) => sum.1 += n,
            Cost::AllOf(parts) => parts.iter().for_each(|part| add(part, sum)),
            _ => {}
        }
    }
    let mut sum = (0, 0);
    continuous::run_costs(state, registry, server).iter().for_each(|cost| add(cost, &mut sum));
    sum
}

/// What `card` costs its owner to install or play on `state`'s table, or
/// `None` for a card that is neither installed for its printed cost nor
/// played (Corp installs are rezzed, not paid for).
fn price(state: &GameState, registry: &CardRegistry, card: &CardDefinition) -> Option<u32> {
    match card.card_type {
        CardType::Event | CardType::Operation => Some(continuous::play_cost_of(state, registry, card)),
        CardType::Program | CardType::Hardware | CardType::Resource => Some(continuous::install_cost_of(state, registry, card)),
        _ => None,
    }
}

/// What the currents add to the price of each of `cards`, one by one.
fn added<'a>(with: &'a GameState, bare: &'a GameState, registry: &'a CardRegistry, cards: &'a [CardId]) -> impl Iterator<Item = f64> + 'a {
    cards.iter().filter_map(move |id| {
        let def = registry.get(id)?;
        Some(f64::from(price(with, registry, def)?) - f64::from(price(bare, registry, def)?))
    })
}

/// The toll of the currents in play, or `None` when none in play prints a
/// price. `horizon` is the turns the stage expects (`stage::horizon`); a
/// current lasts no longer than the game.
pub(super) fn toll(state: &GameState, registry: &CardRegistry, horizon: u32) -> Option<Toll> {
    let corp = state.corp.play_area.iter().filter_map(|played| definition(registry, &played.card)).map(|def| (Side::Corp, def));
    let runner = state.runner.in_play.iter().filter_map(|played| definition(registry, &played.card)).map(|def| (Side::Runner, def));
    let currents: Vec<(Side, &CardDefinition)> = corp.chain(runner).filter(|(_, def)| prices(def)).collect();
    if currents.is_empty() {
        return None;
    }
    let life = f64::from(horizon.min(CURRENT_LIFE));
    let with = as_a_new_turn(state);
    let bare = without_currents(&with, registry);
    let mut toll = Toll::default();

    // A run a turn, on the central that costs the Runner least to run:
    // the run is the Runner's to aim (`identities::lockdowns_on_the_
    // next_run`).
    let (credits, clicks) = [ServerId::Hq, ServerId::RnD]
        .into_iter()
        .map(|server| {
            let (with, bare) = (run_price(&with, registry, server), run_price(&bare, registry, server));
            (f64::from(with.0) - f64::from(bare.0), f64::from(with.1) - f64::from(bare.1))
        })
        .min_by(|a, b| (a.0 + a.1).total_cmp(&(b.0 + b.1)))
        .unwrap_or_default();
    toll.runner_credits += credits * life;
    toll.runner_clicks += clicks * life;

    // Its own credits, a turn (Net Celebrity's 1[recurring-credit]).
    for (side, def) in &currents {
        let credits = f64::from(def.recurring_credits.and_then(RecurringCredits::printed).unwrap_or(0)) * life;
        match side {
            Side::Corp => toll.corp_credits -= credits,
            Side::Runner => toll.runner_credits -= credits,
        }
    }

    // The cards each player holds, and the cards it will draw while the
    // current lasts at what its deck averages.
    let mean = |cards: &[CardId]| if cards.is_empty() { 0.0 } else { added(&with, &bare, registry, cards).sum::<f64>() / cards.len() as f64 };
    let installs: Vec<CardId> = state.runner.grip.iter().filter(|id| registry.get(id).is_some_and(|def| !matches!(def.card_type, CardType::Event))).cloned().collect();
    toll.runner_held_installs = added(&with, &bare, registry, &installs).sum();
    toll.runner_credits += added(&with, &bare, registry, &state.runner.grip).sum::<f64>() + mean(&state.runner.stack) * DRAWS_A_TURN * life;
    toll.corp_credits += added(&with, &bare, registry, &state.corp.hq).sum::<f64>() + mean(&state.corp.r_and_d) * DRAWS_A_TURN * life;

    for installed in &state.corp.installed {
        // An advancement is a click and a credit (Traffic Jam).
        if let (Some(with), Some(bare)) = (continuous::advancement_requirement(&with, registry, installed.install_id), continuous::advancement_requirement(&bare, registry, installed.install_id)) {
            let more = f64::from(with - bare);
            toll.corp_credits += more;
            toll.corp_clicks += more;
            toll.corp_advances += more;
        }
        // A card out of HQ to rez what is not ice (Hacktivist Meeting).
        // An agenda is never rezzed, so it never pays.
        let agenda = registry.get(&installed.card).is_some_and(|def| def.card_type == CardType::Agenda);
        if !installed.rezzed && installed.slot != InstallSlot::Ice && !agenda {
            let cards = |state: &GameState| -> f64 {
                continuous::rez_costs(state, registry, installed.install_id)
                    .iter()
                    .map(|(cost, ..)| match cost {
                        Cost::TrashRandomFromHq { count, .. } => f64::from(*count),
                        _ => 0.0,
                    })
                    .sum()
            };
            toll.corp_cards += cards(&with) - cards(&bare);
        }
    }
    Some(toll)
}

/// What the currents in play are worth to `side`: what they take from the
/// other chair at the rate `side` reads that chair's credits, a click of
/// either chair's at a click, less what they take from `side` — its own
/// credits at the rate of a credit it is yet to have
/// (`future_credit_weight`), a card of the Corp's own at a click, and of
/// the Corp's to the Runner at the Corp's credits (`Pays::to_runner`).
/// Zero at the default weights, as every reading over the horizon is.
pub(super) fn value(state: &GameState, registry: &CardRegistry, side: Side, w: &Weights, horizon: u32) -> f64 {
    if w.future_credit_weight == 0.0 || (state.corp.play_area.is_empty() && state.runner.in_play.is_empty()) {
        return 0.0;
    }
    let Some(toll) = toll(state, registry, horizon) else { return 0.0 };
    match side {
        Side::Corp => {
            toll.runner_credits * w.opponent_credit_weight + toll.runner_clicks * w.click_weight
                - (toll.corp_credits - toll.corp_advances) * w.future_credit_weight
                - (toll.corp_clicks - toll.corp_advances + toll.corp_cards) * w.click_weight
        }
        Side::Runner => {
            (toll.corp_credits + toll.corp_cards) * w.opponent_credit_weight + toll.corp_clicks * w.click_weight
                - (toll.runner_credits - toll.runner_held_installs) * w.future_credit_weight
                - toll.runner_clicks * w.click_weight
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::test_support::*;
    use netrunner_core::rules::{InstallId, InstalledCard, PlayedOperation};

    fn id(name: &str) -> CardId {
        CardId(name.to_string())
    }

    fn corp_current(state: &mut GameState, name: &str) {
        state.corp.play_area = vec![PlayedOperation { card: id(name), handle: InstallId(900), counters: 0, text_of: None }];
    }

    fn runner_current(state: &mut GameState, name: &str) {
        state.runner.in_play = vec![PlayedOperation { card: id(name), handle: InstallId(900), counters: 0, text_of: None }];
    }

    /// No current, or one that prints no price, is read as nothing — and
    /// a lockdown is not a current.
    #[test]
    fn a_board_with_no_priced_current_has_no_toll() {
        let pool = pool();
        let mut state = GameState::new(0);
        assert_eq!(toll(&state, &pool, 9), None);
        corp_current(&mut state, "manhunt");
        assert_eq!(toll(&state, &pool, 9), None, "a trigger is a later stage's");
        corp_current(&mut state, "next_activation_command");
        assert_eq!(toll(&state, &pool, 9), None, "a lockdown");
        assert_eq!(value(&state, &pool, Side::Corp, &guide(), 9), 0.0);
    }

    /// Service Outage: "the Runner must pay 1[credit] as an additional
    /// cost to make the first run each turn" — a credit a turn for the
    /// turns it lasts, whether or not the Runner has run this turn.
    #[test]
    fn a_run_cost_is_a_credit_a_turn_and_does_not_flicker_with_the_turn() {
        let pool = pool();
        let mut state = GameState::new(0);
        corp_current(&mut state, "service_outage");
        let early = toll(&state, &pool, 9).expect("priced");
        assert_eq!(early, Toll { runner_credits: f64::from(CURRENT_LIFE), ..Toll::default() });
        assert_eq!(toll(&state, &pool, 2).expect("priced").runner_credits, 2.0, "no longer than the game");
        let w = guide();
        let worth = value(&state, &pool, Side::Corp, &w, 9);
        assert!((worth - f64::from(CURRENT_LIFE) * w.opponent_credit_weight).abs() < 1e-9);
        assert!((value(&state, &pool, Side::Runner, &w, 9) + f64::from(CURRENT_LIFE) * w.future_credit_weight).abs() < 1e-9);
        assert_eq!(value(&state, &pool, Side::Corp, &Weights::default(), 9), 0.0, "zero at the default weights");
        // The Runner has made its first run this turn: the reading stands.
        let mut ran = state.clone();
        netrunner_core::rules::dispatch_event(&mut ran, &pool, &netrunner_core::rules::GameEvent::RunInitiated { server: ServerId::Hq }).expect("dispatched");
        assert!(ran.this_turn.times(netrunner_core::dsl::Trigger::OnRunStart) > 0, "the turn holds a run");
        assert_eq!(toll(&ran, &pool, 9), Some(early));
    }

    /// Enhanced Login Protocol: "the first time each turn the Runner
    /// takes the basic action to make a run, they must spend [click] as
    /// an additional cost" — a click a turn.
    #[test]
    fn a_run_click_is_a_click_a_turn() {
        let pool = pool();
        let mut state = GameState::new(0);
        corp_current(&mut state, "enhanced_login_protocol");
        assert_eq!(toll(&state, &pool, 9), Some(Toll { runner_clicks: f64::from(CURRENT_LIFE), ..Toll::default() }));
    }

    /// Scarcity of Resources: "the install cost of each resource is
    /// increased by 2[credit]" — on the resources the Runner holds, and
    /// on the ones it will draw at its stack's share of them.
    #[test]
    fn an_install_cost_is_read_on_the_grip_and_the_draws_to_come() {
        let pool = pool();
        let mut state = GameState::new(0);
        corp_current(&mut state, "scarcity_of_resources");
        state.runner.grip = vec![id("daily_casts"), id("sure_gamble")];
        state.runner.stack = vec![id("daily_casts"), id("corroder"), id("corroder"), id("corroder")];
        let toll = toll(&state, &pool, 9).expect("priced");
        assert!((toll.runner_credits - (2.0 + 0.5 * f64::from(CURRENT_LIFE))).abs() < 1e-9, "{toll:?}");
        assert_eq!(toll.runner_held_installs, 2.0);
        assert_eq!(toll.corp_credits, 0.0);
    }

    /// The Runner's own chair reads a held install's price on the table
    /// as it is (`runner::held_price`), so the current's value to the
    /// Runner charges the draws to come and not the grip a second time;
    /// the Corp, which has no such term for the Runner's grip, reads all
    /// of it.
    #[test]
    fn a_held_install_is_not_charged_twice_on_its_own_chair() {
        let pool = pool();
        let mut state = GameState::new(0);
        corp_current(&mut state, "scarcity_of_resources");
        state.runner.grip = vec![id("daily_casts")];
        let w = Weights::default().at_the_guides_rate();
        assert_eq!(value(&state, &pool, Side::Runner, &w, 9), 0.0, "nothing to draw, and the grip is the held term's");
        assert!((value(&state, &pool, Side::Corp, &w, 9) - 2.0 * w.opponent_credit_weight).abs() < 1e-9);
    }

    /// Rolling Brownout: "the play cost of each operation and event is
    /// increased by 1[credit]" — both players'.
    #[test]
    fn a_play_cost_is_read_on_both_hands() {
        let pool = pool();
        let mut state = GameState::new(0);
        corp_current(&mut state, "rolling_brownout");
        state.runner.grip = vec![id("sure_gamble"), id("corroder")];
        state.corp.hq = vec![id("hedge_fund"), id("hedge_fund"), id("ice_wall")];
        let toll = toll(&state, &pool, 9).expect("priced");
        assert_eq!((toll.runner_credits, toll.corp_credits), (1.0, 2.0));
    }

    /// Net Celebrity: "1[recurring-credit]. Use this credit during runs"
    /// — a credit a turn to the Runner, which the Corp reads as taken
    /// from it at nothing and the Runner as its own.
    #[test]
    fn a_runner_currents_own_credits_are_a_credit_a_turn() {
        let pool = pool();
        let mut state = GameState::new(0);
        runner_current(&mut state, "net_celebrity");
        assert_eq!(toll(&state, &pool, 9), Some(Toll { runner_credits: -f64::from(CURRENT_LIFE), ..Toll::default() }));
        let w = guide();
        assert!(value(&state, &pool, Side::Runner, &w, 9) > 0.0);
        assert!(value(&state, &pool, Side::Corp, &w, 9) < 0.0, "and playing a current over it is worth what leaves with it");
    }

    /// Hacktivist Meeting: "as an additional cost to rez a non-ice card,
    /// the Corp must trash 1 card from HQ at random" — a card for each
    /// install it has yet to rez that is not ice.
    #[test]
    fn a_rez_cost_in_cards_is_read_on_what_is_yet_to_be_rezzed() {
        let pool = pool();
        let mut state = GameState::new(0);
        runner_current(&mut state, "hacktivist_meeting");
        let install = |n: u32, card: &str, slot, rezzed| InstalledCard { card: id(card), install_id: InstallId(n), server: ServerId::Remote(0), slot, rezzed, ..Default::default() };
        state.corp.installed = vec![
            install(1, "pad_campaign", InstallSlot::Root, false),
            install(2, "pad_campaign", InstallSlot::Root, true),
            install(3, "ice_wall", InstallSlot::Ice, false),
            install(4, "hostile_takeover", InstallSlot::Root, false),
        ];
        assert_eq!(toll(&state, &pool, 9), Some(Toll { corp_cards: 1.0, ..Toll::default() }));
    }

    /// Traffic Jam: "the advancement requirement of each agenda is
    /// increased by 1 for each copy of that agenda in the Corp's score
    /// area" — a click and a credit for each advancement it adds to an
    /// installed agenda.
    #[test]
    fn an_advancement_requirement_is_a_click_and_a_credit() {
        use netrunner_core::rules::ScoredAgenda;
        let pool = pool();
        let mut state = GameState::new(0);
        runner_current(&mut state, "traffic_jam");
        state.corp.installed = vec![InstalledCard { card: id("hostile_takeover"), install_id: InstallId(1), server: ServerId::Remote(0), slot: InstallSlot::Root, ..Default::default() }];
        assert_eq!(toll(&state, &pool, 9), Some(Toll::default()), "no copy scored, nothing added");
        state.corp.scored_agendas = vec![ScoredAgenda { card: id("hostile_takeover"), install_id: InstallId(7), agenda_counters: 0, scored_on_turn: 0, installed_on_scoring_turn: false, advanced_on_scoring_turn: false, as_agenda: None }];
        assert_eq!(toll(&state, &pool, 9), Some(Toll { corp_credits: 1.0, corp_clicks: 1.0, corp_advances: 1.0, ..Toll::default() }));
    }
}
