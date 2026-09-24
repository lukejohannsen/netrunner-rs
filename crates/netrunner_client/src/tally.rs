//! What each side did over a match, counted off the log, for the table
//! at the end of the game (Phase 7 §8 item 16, from jinteki's end-of-game
//! stats): turns, clicks spent, credits gained and spent, cards drawn and
//! installed, agenda points, and each side's own — the Corp's rezzes, the
//! Runner's runs, accesses, damage and tags.
//!
//! **Counted from a chair's masked log, never from `GameState`.** A client
//! has only its `PublicHistoryEntry`s, and every number here is one both
//! players watched happen — the Corp's facedown install is still an install
//! the Runner saw made, its card masked and its count not — so the table is
//! the same from either chair, and a test holds it to that.
//!
//! **That is why there is no "Cards accessed" row.** The mask drops an
//! access out of HQ, R&D or a remote from the Corp's log whole, because
//! the card is the whole of the event (`masking::mask_event_for_player`),
//! so the Corp's chair counted 16 accesses where the Runner's counted 26
//! in the same match. A row that disagreed between the two chairs would
//! be the one number on the table a person could not trust; counting
//! accesses for the Corp is a masking rule to write first, not a row to
//! add here.
//!
//! **An event, not an action, is what is counted.** "Clicks spent" is every
//! `ClickSpent`, whichever action or card spent it; "Credits gained" is
//! every `CreditsGained`, the basic action's and a card's alike. Counting
//! actions would have missed every credit a card's text gave.
//!
//! **"Credits spent" is `CreditsSpent`: what a payment took that did not
//! come off a card.** A run's bad-publicity and event credits are inside it
//! (the payer spent them), a card's hosted credits are not (the engine
//! reports those as the counters that left the card). The test below pins
//! down exactly that against each side's final credit pool, so the table's
//! credits cannot drift from the engine's without a failure naming it.
//!
//! **Kept as a fold, not a stored list.** A client adds each entry as it
//! arrives ([`Tally::add`]); a take-back, which drops entries, recounts
//! from what is left ([`Tally::of`]), because a count cannot be un-added
//! from a total without knowing which entry it came from.

use netrunner_core::rules::{GameEvent, Side};
use netrunner_session::PublicHistoryEntry;

/// One side's counts. Every field counts for both sides, and the rows a
/// side has no use for ([`Row::corp`] / [`Row::runner`] `None`) are left
/// out of the table rather than shown as a zero that looks like a result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub turns: u32,
    pub clicks_spent: u32,
    pub credits_gained: u32,
    pub credits_spent: u32,
    pub cards_drawn: u32,
    pub cards_installed: u32,
    /// Scored for the Corp, stolen for the Runner.
    pub agenda_points: u32,
    pub cards_rezzed: u32,
    pub runs: u32,
    pub successful_runs: u32,
    pub damage_suffered: u32,
    pub tags_taken: u32,
    /// What the side did with its first hand, once it has decided (CR
    /// 1.6.6a: the Corp first, then the Runner). Public — the log says
    /// which — and read by the start-of-game box as well as the table.
    pub first_hand: Option<FirstHand>,
}

/// A side's answer to its opening hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstHand {
    Kept,
    Mulligan,
}

/// Both sides' counts over the entries added so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub corp: Counts,
    pub runner: Counts,
}

/// One line of the table: what is counted and each side's number, `None`
/// for a side the line is not about (the Corp makes no runs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub label: &'static str,
    pub corp: Option<u32>,
    pub runner: Option<u32>,
}

impl Tally {
    /// The tally of `entries`, in order.
    pub fn of<'a>(entries: impl IntoIterator<Item = &'a PublicHistoryEntry>) -> Tally {
        let mut tally = Tally::default();
        for entry in entries {
            tally.add(entry);
        }
        tally
    }

    /// Counts one entry's events.
    pub fn add(&mut self, entry: &PublicHistoryEntry) {
        for event in &entry.events {
            self.count(event);
        }
    }

    fn side(&mut self, side: Side) -> &mut Counts {
        match side {
            Side::Corp => &mut self.corp,
            Side::Runner => &mut self.runner,
        }
    }

    /// Exhaustive only over what is counted: an event no row reads is
    /// nothing to the table, and a new one is not a question the table
    /// has to answer before it compiles, as it is for `listeners::moments`.
    fn count(&mut self, event: &GameEvent) {
        match event {
            GameEvent::TurnStarted { side, .. } => self.side(*side).turns += 1,
            GameEvent::ClickSpent { side } => self.side(*side).clicks_spent += 1,
            GameEvent::CreditsGained { side, amount } => self.side(*side).credits_gained += amount,
            GameEvent::CreditsSpent { side, amount } => self.side(*side).credits_spent += amount,
            GameEvent::CardDrawn { side } => self.side(*side).cards_drawn += 1,
            // The Corp's installs are one event, faceup or not; the
            // Runner's are one per card type.
            GameEvent::CardInstalled { side, .. } => self.side(*side).cards_installed += 1,
            GameEvent::ProgramInstalled { side, .. } | GameEvent::HardwareInstalled { side, .. } | GameEvent::ResourceInstalled { side, .. } => {
                self.side(*side).cards_installed += 1
            }
            GameEvent::AgendaScored { agenda_points, .. } => self.corp.agenda_points += agenda_points,
            GameEvent::AgendaStolen { agenda_points, .. } => self.runner.agenda_points += agenda_points,
            // Every Corp card is turned faceup by the one rez, whatever
            // the event's name says (`engine::rez_install`).
            GameEvent::IceRezzed { .. } => self.corp.cards_rezzed += 1,
            GameEvent::RunInitiated { .. } => self.runner.runs += 1,
            GameEvent::RunSucceeded { .. } => self.runner.successful_runs += 1,
            GameEvent::DamageTaken { amount, .. } => self.runner.damage_suffered += *amount as u32,
            GameEvent::TagsGiven { amount, .. } => self.runner.tags_taken += amount,
            GameEvent::HandKept { side } => self.side(*side).first_hand = Some(FirstHand::Kept),
            GameEvent::MulliganTaken { side } => self.side(*side).first_hand = Some(FirstHand::Mulligan),
            _ => {}
        }
    }

    /// The table, in a fixed order: what both sides do, then the Corp's
    /// own, then the Runner's.
    pub fn rows(&self) -> Vec<Row> {
        let (c, r) = (&self.corp, &self.runner);
        let both = |label, corp: u32, runner: u32| Row { label, corp: Some(corp), runner: Some(runner) };
        let corp = |label, corp: u32| Row { label, corp: Some(corp), runner: None };
        let runner = |label, runner: u32| Row { label, corp: None, runner: Some(runner) };
        let mulligan = |counts: &Counts| u32::from(counts.first_hand == Some(FirstHand::Mulligan));
        vec![
            both("Mulligans", mulligan(c), mulligan(r)),
            both("Turns", c.turns, r.turns),
            both("Clicks spent", c.clicks_spent, r.clicks_spent),
            both("Credits gained", c.credits_gained, r.credits_gained),
            both("Credits spent", c.credits_spent, r.credits_spent),
            both("Cards drawn", c.cards_drawn, r.cards_drawn),
            both("Cards installed", c.cards_installed, r.cards_installed),
            both("Agenda points", c.agenda_points, r.agenda_points),
            corp("Cards rezzed", c.cards_rezzed),
            runner("Runs", r.runs),
            runner("Successful runs", r.successful_runs),
            runner("Damage suffered", r.damage_suffered),
            runner("Tags taken", r.tags_taken),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::{BotAgent, HeuristicAgent, RandomAgent};
    use netrunner_core::rules::GameState;
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};

    /// Plays a heuristic match on `seed` and returns each chair's log with
    /// the final board as each chair sees it.
    fn play(seed: u64) -> (Vec<PublicHistoryEntry>, Vec<PublicHistoryEntry>, netrunner_core::view::ClientView) {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
        let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
        // A random seat on one side in alternate seeds: the heuristic
        // Runner seldom runs into ice, and a random one does.
        let runner: Box<dyn BotAgent> = if seed.is_multiple_of(2) { Box::new(HeuristicAgent::new(Side::Runner, seed + 1)) } else { Box::new(RandomAgent::new(seed + 1)) };
        let mut agents: [Box<dyn BotAgent>; 2] = [Box::new(HeuristicAgent::new(Side::Corp, seed)), runner];
        let (mut corp, mut runner) = (Vec::new(), Vec::new());
        loop {
            match session.step() {
                SessionStep::Awaiting { side, view } => {
                    let agent = &mut agents[usize::from(side == Side::Runner)];
                    session.submit(agent.select_action(&view, &registry)).unwrap();
                    // Read at once: the mask reads the state the action left.
                    corp.push(session.last_entry_for(Side::Corp).unwrap());
                    runner.push(session.last_entry_for(Side::Runner).unwrap());
                }
                SessionStep::Applied { .. } => unreachable!("both seats are external"),
                SessionStep::Ended { .. } => break,
                SessionStep::Stalled(reason) => panic!("seed {seed}: {reason:?}"),
            }
        }
        let view = session.view_for(Side::Corp);
        (corp, runner, view)
    }

    /// Both chairs count the same match the same: every number on the
    /// table is one both players saw happen.
    #[test]
    fn the_table_is_the_same_from_either_chair() {
        for seed in 0..4 {
            let (corp, runner, _) = play(seed);
            assert_eq!(Tally::of(&corp), Tally::of(&runner), "seed {seed}");
        }
    }

    /// The credits on the table add up to the pool on the board: five to
    /// start, plus every gain, less every loss and every credit paid out of
    /// the pool — `CreditsSpent` less the run's own credits it includes.
    /// A payment or a gain that went unreported would fail this, naming
    /// the seed and the side.
    #[test]
    fn the_credits_add_up_to_the_pool_on_the_board() {
        for seed in 0..6 {
            let (entries, _, view) = play(seed);
            let tally = Tally::of(&entries);
            let (mut lost, mut run_credits) = ([0u32; 2], 0u32);
            for event in entries.iter().flat_map(|entry| &entry.events) {
                match event {
                    GameEvent::CreditsLost { side, amount } => lost[usize::from(*side == Side::Runner)] += amount,
                    GameEvent::BadPublicityCreditsSpent { amount } | GameEvent::BonusRunCreditsSpent { amount } => run_credits += amount,
                    _ => {}
                }
            }
            let pool = |start: u32, counts: &Counts, lost: u32, run: u32| i64::from(start) + i64::from(counts.credits_gained) - i64::from(counts.credits_spent - run) - i64::from(lost);
            assert_eq!(pool(5, &tally.corp, lost[0], 0), i64::from(view.corp.credits), "seed {seed}: the Corp's credits");
            assert_eq!(pool(5, &tally.runner, lost[1], run_credits), i64::from(view.runner.credits), "seed {seed}: the Runner's credits");
        }
    }

    #[test]
    fn a_match_fills_the_rows_that_matter() {
        let (entries, _, view) = play(1);
        let tally = Tally::of(&entries);
        assert!(tally.corp.turns > 0 && tally.runner.turns > 0);
        assert!(tally.corp.clicks_spent >= 3 * (tally.corp.turns - 1), "{tally:?}");
        assert!(tally.runner.cards_installed > 0, "the Runner's installs are one event per card type: {tally:?}");
        assert!(tally.corp.first_hand.is_some() && tally.runner.first_hand.is_some(), "both sides decided on a hand");
        assert_eq!(tally.corp.agenda_points, view.corp.agenda_points, "every point scored is on the table");
        assert_eq!(tally.runner.agenda_points, view.runner.agenda_points, "every point stolen is on the table");
        let rows = tally.rows();
        assert!(rows.iter().all(|row| row.corp.is_some() || row.runner.is_some()));
        assert_eq!(rows.iter().find(|row| row.label == "Runs").unwrap().corp, None, "the Corp makes no runs");
    }
}
