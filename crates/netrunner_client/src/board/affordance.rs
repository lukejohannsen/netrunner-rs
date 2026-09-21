//! Which cards can act right now, and in which of two moods.
//!
//! A click on the board opens a card's menu rather than submitting
//! anything (AGENTS.md §5), which is the rule that stopped a click meant
//! to examine a card from installing it — but it left the person with no
//! way to tell, without clicking, which cards have a menu worth opening.
//! The board knew the answer all along: a target with entries in the
//! [`ActionMap`](super::ActionMap) is a target the engine will accept an
//! action on. This module is that answer, named and given a mood, so a
//! screen can light it.
//!
//! **Two moods, because the two kinds of moment are different.**
//!
//! - [`Affordance::Usable`] — a move made at the person's own pace, on
//!   their own turn. Play, install, run, advance, score, take a credit.
//!   Nothing is waiting on it; it will still be there after a think.
//! - [`Affordance::Conditional`] — a moment that will pass: a run is
//!   under way, a paid-ability window is open, or a prompt is waiting on
//!   an answer. The pump not taken during the encounter, the ICE not
//!   rezzed on the approach and the trigger not paid on access are all
//!   gone the instant the window closes, which is why this one is drawn
//!   as a warning rather than an invitation.
//!
//! **Classification is per action, not per moment**, with exactly two
//! exceptions. The rejected alternative was to read the mood off the
//! clock alone — "a run or a window is open, so everything listed is
//! conditional" — which is true of today's engine and would have been
//! half the code. It was rejected because it colours by *when* rather
//! than by *what*: a `PlayerAction` added later would inherit the mood of
//! whatever moment it was first listed in, with no test failing and
//! nobody deciding. The match below is exhaustive for that reason — a new
//! variant does not compile until someone says which mood it is.
//!
//! The two exceptions are the actions that are genuinely both, and they
//! take the moment as their tie-break: `RezIce` is an asset flipped up at
//! leisure on the Corp's own turn and an ICE rezzed on the Runner's
//! approach, and `ActivateAbility` is a Corp asset's click ability and a
//! breaker's pump. Same action, two moments, and the person needs them
//! told apart precisely because one of them expires.
//!
//! Nothing here decides legality, and nothing here reads a rule: an
//! action is classified because the engine already put it in
//! `legal_actions`. A target with no entry gets no mood and no glow,
//! which is also what the whole opposing side gets while it is not their
//! priority — the client gates on that before it asks (the desktop's
//! `Game::awaiting`).

use netrunner_core::rules::PlayerAction;
use netrunner_core::view::ClientView;

/// The mood a target's legal actions give it, as a screen draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Affordance {
    /// A move at the person's own pace. Drawn in the standard colour.
    Usable,
    /// A moment that will pass — a run, an open window, a waiting
    /// prompt. Drawn as a warning, and it wins over `Usable` when a
    /// target offers both (see [`Affordance::stronger`]).
    Conditional,
}

impl Affordance {
    /// The mood a target with both takes. `Conditional` wins: a card
    /// that can do something expiring *and* something that will keep is
    /// worth the warning, since only one of the two can be missed.
    ///
    /// The ordering on the enum is this rule, which is why the derive is
    /// there rather than being an accident of declaration order.
    pub fn stronger(self, other: Affordance) -> Affordance {
        self.max(other)
    }
}

/// Whether the view is inside a moment that will pass: a run, an open
/// paid-ability window, a trace, or a parked decision waiting on an
/// answer.
///
/// Read once when the [`ActionMap`](super::ActionMap) is built rather
/// than per card, so every target of one view is judged against the same
/// moment — and so the map, which already holds everything a board needs,
/// does not have to be handed the view a second time.
pub(super) fn in_a_passing_moment(view: &ClientView) -> bool {
    view.active_run.is_some()
        || view.paid_ability_window.is_some()
        || view.active_trace.is_some()
        || view.pending_prevention.is_some()
        || view.pending_paid_choice.is_some()
        || view.pending_decision.is_some()
        // A payment waiting on which credits go first is a prompt parked on
        // its payer like any other.
        || view.pending_payment.is_some()
}

/// The mood of one legal action, given whether a passing moment is open.
///
/// Exhaustive on purpose: see the module comment.
pub(super) fn affordance_of(action: &PlayerAction, passing: bool) -> Affordance {
    use Affordance::{Conditional, Usable};
    match action {
        // The turn's own business, taken at the person's pace.
        PlayerAction::GainCreditClick { .. }
        | PlayerAction::DrawCardClick { .. }
        | PlayerAction::InstallCard { .. }
        | PlayerAction::InitiateRun { .. }
        | PlayerAction::PlayEvent { .. }
        | PlayerAction::PlayOperation { .. }
        | PlayerAction::InstallHardware { .. }
        | PlayerAction::InstallProgram { .. }
        | PlayerAction::InstallResource { .. }
        | PlayerAction::InstallProgramOnIce { .. }
        | PlayerAction::AdvanceCard { .. }
        | PlayerAction::ScoreAgenda { .. }
        | PlayerAction::RemoveTag
        | PlayerAction::PurgeVirusCounters
        | PlayerAction::TrashResource { .. }
        | PlayerAction::EndTurn => Usable,

        // Both, and the moment decides. An asset flipped up on the
        // Corp's own turn is leisure; the same action on the Runner's
        // approach is the last chance to pay for the wall.
        PlayerAction::RezIce { .. } | PlayerAction::ActivateAbility { .. } => {
            if passing {
                Conditional
            } else {
                Usable
            }
        }

        // The run itself: every step of it expires.
        PlayerAction::ContinueRun
        | PlayerAction::JackOut
        | PlayerAction::CompleteRun
        | PlayerAction::BreakSubroutineWithClick { .. }
        // What a breach offers, for as long as the card is being
        // accessed and no longer.
        | PlayerAction::SelectCardToAccess { .. }
        | PlayerAction::StealAgenda { .. }
        | PlayerAction::TrashAccessedCard { .. }
        | PlayerAction::PassAccessedCard { .. }
        | PlayerAction::PayAccessTrigger { .. }
        | PlayerAction::DeclineAccessTrigger { .. }
        // A prompt is parked on the person: the pop-up's own actions,
        // which the person asked be warned about alongside the rest.
        | PlayerAction::ChooseTriggerToResolve { .. }
        | PlayerAction::SubmitCorpTraceBid { .. }
        | PlayerAction::SubmitRunnerTraceBid { .. }
        | PlayerAction::AcceptPendingPaidChoice { .. }
        | PlayerAction::DeclinePendingPaidChoice
        | PlayerAction::ResolvePendingChoice { .. }
        | PlayerAction::ToggleCardSelection { .. }
        | PlayerAction::ConfirmCardSelection
        | PlayerAction::ChooseServerForPendingDecision { .. }
        | PlayerAction::KeepHand
        | PlayerAction::TakeMulligan
        // Discarding to hand size is the engine telling the person the
        // turn cannot end until they do, which is a warning and not an
        // invitation.
        | PlayerAction::DiscardCard { .. }
        // Priority handed back closes whatever window is open, so it is
        // the moment as much as anything in it.
        | PlayerAction::PassPriority { .. } => Conditional,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{ActionMap, Pile, Target};
    use netrunner_bots::{BotAgent, HeuristicAgent, RandomAgent};
    use netrunner_core::cards::CardRegistry;
    use netrunner_core::rules::{GameState, Side, Viewer};
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};

    /// Every target on the board a click could land on, from this
    /// viewer's side: their own hand, every install either side can see,
    /// every server, their piles and both identities.
    fn targets_on(view: &ClientView) -> Vec<Target> {
        let side = view.viewer.side();
        let mut targets: Vec<Target> = Vec::new();
        if let Some(hand) = match side {
            Some(Side::Corp) => view.corp.hq_cards.clone(),
            Some(Side::Runner) => view.runner.grip_cards.clone(),
            None => None,
        } {
            targets.extend(hand.into_iter().map(Target::HandCard));
        }
        targets.extend(view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).map(|c| Target::Install(c.install_id)));
        targets.extend(view.runner.rig.iter().map(|c| Target::Install(c.install_id)));
        targets.extend(view.corp.servers.iter().map(|s| Target::Server(s.server)));
        targets.extend([Target::Pile(Pile::Stack), Target::Pile(Pile::Heap)]);
        targets.extend([Target::Identity(Side::Corp), Target::Identity(Side::Runner)]);
        targets
    }

    /// Real games, both viewers: a target glows exactly when the engine
    /// offers something on it, the side without priority never glows at
    /// all, and both moods are reached in ordinary play.
    ///
    /// The counts at the end are the point of the seeds. A glow that
    /// never turns yellow is a glow that would have shipped as one
    /// colour without anyone noticing, and the run's moods are the ones
    /// the classification exists for.
    #[test]
    fn a_target_takes_a_mood_exactly_when_the_engine_offers_something_on_it() {
        let mut usable = 0;
        let mut conditional = 0;
        let mut conditional_installs = 0;
        let mut usable_hand_cards = 0;
        let mut idle_targets = 0;
        let mut off_priority = 0;
        for seed in 0..6u64 {
            let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
            let registry: CardRegistry = crate::decks::sample_deck_registry();
            let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
            let mut corp: Box<dyn BotAgent> = if seed % 2 == 0 { Box::new(HeuristicAgent::new(Side::Corp, seed)) } else { Box::new(RandomAgent::new(seed)) };
            let mut runner: Box<dyn BotAgent> = if seed % 2 == 0 { Box::new(RandomAgent::new(seed + 7)) } else { Box::new(HeuristicAgent::new(Side::Runner, seed)) };
            loop {
                match session.step() {
                    SessionStep::Awaiting { side, view } => {
                        for viewer in [Viewer::Player(Side::Corp), Viewer::Player(Side::Runner)] {
                            let seen = session.view_for(viewer);
                            let map = ActionMap::build(&seen, &registry);
                            for target in targets_on(&seen) {
                                let entries = map.for_target(&target);
                                let mood = map.affordance(&target);
                                assert_eq!(
                                    mood.is_some(),
                                    !entries.is_empty(),
                                    "seed {seed} {viewer:?}: {target:?} has {} entries and mood {mood:?}",
                                    entries.len()
                                );
                                match (viewer.side() == Some(side), mood) {
                                    // Not a bug and not a glow: the Corp's
                                    // standing rez is in its own
                                    // `legal_actions` while the Runner holds
                                    // priority, which is why the gate is
                                    // `awaiting` and not "the map is empty".
                                    (false, Some(_)) => off_priority += 1,
                                    (false, None) => idle_targets += 1,
                                    (true, Some(Affordance::Usable)) => {
                                        usable += 1;
                                        if matches!(target, Target::HandCard(_)) {
                                            usable_hand_cards += 1;
                                        }
                                    }
                                    (true, Some(Affordance::Conditional)) => {
                                        conditional += 1;
                                        if matches!(target, Target::Install(_)) {
                                            conditional_installs += 1;
                                        }
                                    }
                                    (true, None) => {}
                                }
                            }
                        }
                        let action = match side {
                            Side::Corp => corp.select_action(&view, &registry),
                            Side::Runner => runner.select_action(&view, &registry),
                        };
                        session.submit(action).unwrap();
                    }
                    SessionStep::Ended { .. } | SessionStep::Stalled(_) => break,
                    SessionStep::Applied { .. } => {}
                }
            }
        }
        assert!(usable > 500, "a turn's own moves should be the bulk of the glows, saw {usable}");
        assert!(usable_hand_cards > 100, "a playable hand card is the commonest glow of all, saw {usable_hand_cards}");
        assert!(conditional > 20, "the moments that pass should be reached in six games, saw {conditional}");
        assert!(conditional_installs > 0, "a rez or a pump inside a window is what the second colour is for, saw none");
        assert!(idle_targets > 1_000, "the side without priority should have been looked at throughout, saw {idle_targets}");
        // The reason the client gates on `awaiting`: a seat off priority
        // still has legal actions of its own, so "the map is empty" is
        // not the same question as "may this person act now".
        assert!(off_priority > 0, "a seat off priority was expected to keep at least one standing action across six games");
    }

    #[test]
    fn a_conditional_action_wins_over_a_usable_one() {
        assert_eq!(Affordance::Usable.stronger(Affordance::Conditional), Affordance::Conditional);
        assert_eq!(Affordance::Conditional.stronger(Affordance::Usable), Affordance::Conditional);
        assert_eq!(Affordance::Usable.stronger(Affordance::Usable), Affordance::Usable);
    }

    #[test]
    fn the_two_dual_actions_follow_the_moment() {
        let rez = PlayerAction::RezIce { ice: netrunner_core::rules::InstallId(1) };
        let ability = PlayerAction::ActivateAbility { target: netrunner_core::rules::InstallId(1), ability_index: 0 };
        for action in [rez, ability] {
            assert_eq!(affordance_of(&action, false), Affordance::Usable, "{action:?} at leisure");
            assert_eq!(affordance_of(&action, true), Affordance::Conditional, "{action:?} in a window");
        }
    }

    #[test]
    fn a_turn_move_keeps_its_mood_inside_a_window() {
        // Nothing but the two dual actions reads the moment: an install
        // listed during an open window is still an install.
        let install = PlayerAction::InstallProgram { card_id: netrunner_core::dsl::CardId("x".into()) };
        assert_eq!(affordance_of(&install, true), Affordance::Usable);
    }
}
