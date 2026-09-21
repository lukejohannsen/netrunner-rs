//! A run as a trail of steps, read one event at a time.
//!
//! **Why not `diff::transitions`?** A `Transition` is one line per
//! applied action, and one applied `ContinueRun` can pass a piece of
//! ice, approach the next and encounter it in a single
//! `PublicHistoryEntry` — `diff` collapses that to one `RunMoved` naming
//! only where the run ended up. A person following the run wants each
//! of those as its own beat, and the Corp wants to see which subroutine
//! fired on the ice that was just passed. So a trail is built from the
//! entry's own events in order ([`RunTrail::observe`]) and reconciled
//! with the view once the action has applied ([`RunTrail::sync`]), so it
//! can be shown a step at a time and still never disagree with the
//! engine about where the run is.
//!
//! **Everything here is masked already.** The events are the viewer's
//! copy of the log and the run state is `PublicRunState`, so an
//! unrezzed ice the Runner may not name is a step with `card: None`, and
//! a card accessed out of HQ that the Corp may not see is counted, not
//! named. The words for what the run did (`consequences`) are the
//! client's, short enough to sit in one line under the steps: a log
//! narrates, a trail summarises.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{GameEvent, InstallId, MaskedZone, PublicAccessPhase, PublicRunState, RunPhase, ServerId, Side, SubroutineStatus};

use crate::actions::card_title;
use crate::board::action_map::server_name;

/// One piece of ice on the run's path, in the order the Runner meets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceStep {
    pub install: InstallId,
    /// The card when the viewer may name it: rezzed, or the viewer's own.
    pub card: Option<CardId>,
    pub state: IceState,
    /// One status per subroutine the viewer may see; empty for an ice
    /// whose identity the mask withholds.
    pub subs: Vec<SubroutineStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IceState {
    Upcoming,
    Approaching,
    Encountering,
    Passed,
    Bypassed,
}

/// Where the run is along its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Initiated, nothing approached yet.
    Starting,
    /// At the `n`th piece of ice (outermost first).
    AtIce(usize),
    /// In the movement phase, between pieces of ice: `next` is the one the
    /// Runner will approach, or `ice.len()` when the server is next. At no
    /// ice, so a lane lights none.
    Moving { next: usize },
    /// Approaching the server, every ice behind.
    AtServer,
    /// Breaching: `count` cards accessed so far, `card` the one the
    /// viewer may name.
    Accessing { count: usize, card: Option<CardId> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Successful,
    JackedOut,
    /// Ended by a subroutine or an effect.
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTrail {
    pub server: ServerId,
    pub ice: Vec<IceStep>,
    pub stage: Stage,
    /// `RunSucceeded` was seen: the run is successful whether or not it
    /// has finished accessing.
    pub successful: bool,
    pub outcome: Option<Outcome>,
    /// What the run did, in order, in a few words each.
    pub consequences: Vec<String>,
}

impl RunTrail {
    /// A trail shaped from the view's run: the ice in approach order,
    /// named and with their subroutines where the viewer may see them,
    /// each step's state from the run's position and phase.
    pub fn begin(run: &PublicRunState) -> Self {
        let mut trail = RunTrail { server: run.server, ice: Vec::new(), stage: Stage::Starting, successful: false, outcome: None, consequences: Vec::new() };
        trail.sync(Some(run));
        trail
    }

    /// Whether the run is over, so the lane shows an outcome and the
    /// next run replaces the trail.
    pub fn ended(&self) -> bool {
        self.outcome.is_some()
    }

    /// One event of the viewer's log, in order. Events that are not the
    /// run's are ignored, so an entry can be fed whole.
    pub fn observe(&mut self, event: &GameEvent, registry: &CardRegistry) {
        let title = |card: &CardId| card_title(card, registry);
        match event {
            GameEvent::IceApproached { position, .. } => {
                let position = *position as usize;
                for (i, step) in self.ice.iter_mut().enumerate() {
                    if i < position && matches!(step.state, IceState::Upcoming | IceState::Approaching | IceState::Encountering) {
                        step.state = IceState::Passed;
                    }
                }
                if let Some(step) = self.ice.get_mut(position) {
                    step.state = IceState::Approaching;
                }
                self.stage = Stage::AtIce(position);
            }
            GameEvent::IceEncountered { card_id, subroutine_count, .. } => {
                if let Stage::AtIce(position) = self.stage
                    && let Some(step) = self.ice.get_mut(position)
                {
                    step.state = IceState::Encountering;
                    step.card.get_or_insert_with(|| card_id.clone());
                    if step.subs.len() < *subroutine_count {
                        step.subs.resize(*subroutine_count, SubroutineStatus::Pending);
                    }
                }
            }
            GameEvent::SubroutineBroken { card_id, index } => {
                self.mark_subroutine(*index, SubroutineStatus::Broken);
                self.consequences.push(format!("{} broken on {}", subroutine_words(card_id, *index, registry), title(card_id)));
            }
            GameEvent::SubroutineFired { card_id, index, .. } => {
                self.mark_subroutine(*index, SubroutineStatus::Resolved);
                self.consequences.push(format!("{} fired on {}", subroutine_words(card_id, *index, registry), title(card_id)));
            }
            GameEvent::IcePassed { position, .. } => {
                if let Some(step) = self.ice.get_mut(*position as usize)
                    && step.state != IceState::Bypassed
                {
                    step.state = IceState::Passed;
                }
                self.stage = Stage::Moving { next: *position as usize + 1 };
            }
            GameEvent::IceBypassed { position, .. } => {
                if let Some(step) = self.ice.get_mut(*position as usize) {
                    step.state = IceState::Bypassed;
                }
            }
            GameEvent::IceRezzed { card, install, .. } => {
                if let Some(step) = self.ice.iter_mut().find(|s| s.install == *install) {
                    step.card = Some(card.clone());
                }
                self.consequences.push(format!("{} rezzed", title(card)));
            }
            GameEvent::ServerApproached { .. } => {
                self.pass_everything();
                self.stage = Stage::AtServer;
            }
            GameEvent::RunSucceeded { .. } => {
                self.pass_everything();
                self.successful = true;
                if !matches!(self.stage, Stage::Accessing { .. }) {
                    self.stage = Stage::AtServer;
                }
            }
            GameEvent::CardAccessed { card, .. } => {
                let count = match &self.stage {
                    Stage::Accessing { count, .. } => count + 1,
                    _ => 1,
                };
                self.stage = Stage::Accessing { count, card: Some(card.clone()) };
                self.consequences.push(format!("accessed {}", title(card)));
            }
            GameEvent::AgendaStolen { card, agenda_points } => self.consequences.push(format!("stole {} ({agenda_points})", title(card))),
            GameEvent::CardTrashedFromAccess { card, .. } => self.consequences.push(format!("trashed {}", title(card))),
            GameEvent::DamageTaken { damage_type, amount } => self.consequences.push(format!("{amount} {} damage", format!("{damage_type:?}").to_lowercase())),
            GameEvent::TagsGiven { side, amount } => self.consequences.push(format!("{side:?} took {amount} tag{}", plural(*amount))),
            GameEvent::CreditsLost { side, amount } => self.consequences.push(format!("{side:?} lost {amount} credit{}", plural(*amount))),
            GameEvent::RunJackedOut { .. } => self.outcome = Some(Outcome::JackedOut),
            GameEvent::RunEndedByEffect { .. } => {
                self.outcome = Some(Outcome::Ended);
                self.consequences.push("the run ended".to_string());
            }
            GameEvent::RunCompleted { .. } if self.outcome.is_none() => {
                self.outcome = Some(if self.successful { Outcome::Successful } else { Outcome::Ended });
            }
            _ => {}
        }
    }

    /// Reconciles the trail with the view once the action has applied:
    /// the ice list and names, each subroutine's status, the position
    /// and phase — the engine's word over the events' — and the outcome
    /// when the run is gone.
    pub fn sync(&mut self, run: Option<&PublicRunState>) {
        let Some(run) = run else {
            if self.outcome.is_none() {
                self.outcome = Some(if self.successful { Outcome::Successful } else { Outcome::Ended });
            }
            return;
        };
        if run.server != self.server {
            // A redirected run keeps its trail; the server chip follows.
            self.server = run.server;
        }
        let mut ice = Vec::with_capacity(run.ice.len());
        for (i, piece) in run.ice.iter().enumerate() {
            let known = self.ice.iter().find(|s| s.install == piece.install_id);
            let mut step = known.cloned().unwrap_or(IceStep { install: piece.install_id, card: None, state: IceState::Upcoming, subs: Vec::new() });
            if let Some(identity) = &piece.identity {
                step.card = Some(identity.card.clone());
                step.subs = identity.subroutines.iter().map(|s| s.status).collect();
            }
            step.state = match run.phase {
                RunPhase::Initiation => IceState::Upcoming,
                RunPhase::ApproachIce | RunPhase::EncounterIce if i == run.position => {
                    if run.phase == RunPhase::ApproachIce {
                        IceState::Approaching
                    } else {
                        IceState::Encountering
                    }
                }
                _ if i < run.position || matches!(run.phase, RunPhase::Success | RunPhase::AccessingCard | RunPhase::Ended) => {
                    if step.state == IceState::Bypassed {
                        IceState::Bypassed
                    } else {
                        IceState::Passed
                    }
                }
                _ => IceState::Upcoming,
            };
            ice.push(step);
        }
        self.ice = ice;
        self.stage = match run.phase {
            RunPhase::Initiation => Stage::Starting,
            RunPhase::ApproachIce | RunPhase::EncounterIce => Stage::AtIce(run.position),
            RunPhase::Movement => Stage::Moving { next: run.position },
            RunPhase::Success | RunPhase::Ended => {
                if matches!(self.stage, Stage::Accessing { .. }) {
                    self.stage.clone()
                } else {
                    Stage::AtServer
                }
            }
            RunPhase::AccessingCard => {
                let accessed = run.access_state.as_ref().map_or(0, |a| match &a.resolved_cards {
                    MaskedZone::Visible(cards) => cards.len(),
                    MaskedZone::Hidden { count } => *count as usize,
                });
                let card = run.access_state.as_ref().and_then(|a| match &a.phase {
                    PublicAccessPhase::PendingChoice { card, .. } | PublicAccessPhase::PendingInteractiveTrigger { card, .. } => card.clone(),
                    PublicAccessPhase::SelectNextCard { .. } => None,
                });
                let count = match &self.stage {
                    Stage::Accessing { count, .. } => (*count).max(accessed),
                    _ => accessed,
                };
                Stage::Accessing { count, card }
            }
        };
        if matches!(run.phase, RunPhase::Success | RunPhase::AccessingCard) {
            self.successful = true;
        }
    }

    /// The step the run is at, for a lane to light.
    pub fn current(&self) -> Option<usize> {
        match self.stage {
            Stage::AtIce(i) => Some(i),
            _ => None,
        }
    }

    /// The words of the run's heading: "Run on HQ".
    pub fn heading(&self) -> String {
        format!("Run on {}", server_name(self.server))
    }

    fn mark_subroutine(&mut self, index: usize, status: SubroutineStatus) {
        if let Stage::AtIce(position) = self.stage
            && let Some(step) = self.ice.get_mut(position)
        {
            if step.subs.len() <= index {
                step.subs.resize(index + 1, SubroutineStatus::Pending);
            }
            step.subs[index] = status;
        }
    }

    fn pass_everything(&mut self) {
        for step in &mut self.ice {
            if step.state != IceState::Bypassed {
                step.state = IceState::Passed;
            }
        }
    }
}

/// Whether an event moves the run along its path — the events a client
/// gives a beat of their own. The rest (a subroutine, a rez, a steal)
/// are read with the step they belong to.
pub fn is_step(event: &GameEvent) -> bool {
    matches!(
        event,
        GameEvent::IceApproached { .. }
            | GameEvent::IceEncountered { .. }
            | GameEvent::IcePassed { .. }
            | GameEvent::IceBypassed { .. }
            | GameEvent::ServerApproached { .. }
            | GameEvent::RunSucceeded { .. }
            | GameEvent::CardAccessed { .. }
            | GameEvent::RunJackedOut { .. }
            | GameEvent::RunEndedByEffect { .. }
            | GameEvent::RunCompleted { .. }
    )
}

/// Whether an event belongs to a run at all: a step, or something a
/// step did.
pub fn concerns_run(event: &GameEvent) -> bool {
    is_step(event)
        || matches!(
            event,
            GameEvent::RunInitiated { .. }
                | GameEvent::SubroutineBroken { .. }
                | GameEvent::SubroutineFired { .. }
                | GameEvent::IceRezzed { .. }
                | GameEvent::AgendaStolen { .. }
                | GameEvent::CardTrashedFromAccess { .. }
                | GameEvent::AccessPassed { .. }
                | GameEvent::RunRedirected { .. }
        )
}

/// The printed clause of a subroutine when the card is known, else its
/// number: the Linked Clause Rule says a person reads the card's words.
fn subroutine_words(card: &CardId, index: usize, registry: &CardRegistry) -> String {
    registry
        .get(card)
        .and_then(|def| def.subroutines.get(index))
        .map(|sub| format!("\u{201c}{}\u{201d}", sub.text.trim_end_matches('.')))
        .unwrap_or_else(|| format!("subroutine {}", index + 1))
}

fn plural(n: u32) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The trail's side, for a lane's colour: a run is the Runner's.
pub const ACTOR: Side = Side::Runner;

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::{BotAgent, HeuristicAgent, RandomAgent};
    use netrunner_core::rules::{GameState, PublicRunIce, Viewer};
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};

    /// Real games, both viewers: a trail fed the entry's events and then
    /// synced never names an ice the view's run does not have, its
    /// stage never runs past the ice, every run that ends has an
    /// outcome, and the Corp sees a subroutine fire.
    #[test]
    fn a_trail_follows_real_runs_as_both_viewers() {
        let mut runs = 0;
        let mut fired_seen_by_corp = 0;
        let mut encounters = 0;
        let mut outcomes = [0usize; 3];
        for seed in 0..6u64 {
            let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
            let registry: CardRegistry = crate::decks::sample_deck_registry();
            let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
            let mut corp: Box<dyn BotAgent> = if seed % 2 == 0 { Box::new(HeuristicAgent::new(Side::Corp, seed)) } else { Box::new(RandomAgent::new(seed)) };
            let mut runner = RandomAgent::new(seed + 7);
            let viewers = [Viewer::Player(Side::Corp), Viewer::Player(Side::Runner)];
            let mut trails: Vec<Option<RunTrail>> = vec![None, None];
            loop {
                match session.step() {
                    SessionStep::Awaiting { side, view } => {
                        let action = match side {
                            Side::Corp => corp.select_action(&view, &registry),
                            Side::Runner => runner.select_action(&view, &registry),
                        };
                        session.submit(action).unwrap();
                        for (i, viewer) in viewers.iter().enumerate() {
                            let after = session.view_for(*viewer);
                            let entry = session.last_entry_for(*viewer).unwrap();
                            let started = entry.events.iter().any(|e| matches!(e, GameEvent::RunInitiated { .. }));
                            if started {
                                runs += 1;
                                trails[i] = None;
                            }
                            if let Some(trail) = &mut trails[i] {
                                for event in &entry.events {
                                    trail.observe(event, &registry);
                                    if matches!(event, GameEvent::IceEncountered { .. }) {
                                        encounters += 1;
                                        assert!(matches!(trail.stage, Stage::AtIce(p) if p < trail.ice.len()), "seed {seed} {viewer:?}: encounter at {:?} of {} ice", trail.stage, trail.ice.len());
                                    }
                                    if matches!(event, GameEvent::SubroutineFired { .. }) && *viewer == Viewer::Player(Side::Corp) {
                                        fired_seen_by_corp += 1;
                                        assert!(trail.ice.iter().any(|s| s.subs.contains(&SubroutineStatus::Resolved)), "seed {seed}: a fired subroutine shows as resolved");
                                    }
                                }
                            }
                            match (&mut trails[i], &after.active_run) {
                                (trail @ None, Some(run)) => *trail = Some(RunTrail::begin(run)),
                                (Some(trail), run) => trail.sync(run.as_ref()),
                                (None, None) => {}
                            }
                            if let Some(trail) = &trails[i] {
                                if let Some(run) = &after.active_run {
                                    assert_eq!(trail.ice.len(), run.ice.len(), "seed {seed} {viewer:?}");
                                    for (step, piece) in trail.ice.iter().zip(&run.ice) {
                                        assert_eq!(step.install, piece.install_id);
                                        assert_eq!(step.card.is_some(), piece.identity.is_some(), "seed {seed} {viewer:?}: a step names an ice exactly when the view does");
                                    }
                                    if let Stage::AtIce(p) = trail.stage {
                                        assert_eq!(p, run.position);
                                    }
                                    assert!(!trail.ended(), "seed {seed} {viewer:?}: a run on the board is not over");
                                } else {
                                    assert!(trail.ended(), "seed {seed} {viewer:?}: the run is gone but the trail has no outcome");
                                }
                            }
                            if let Some(trail) = &trails[i]
                                && trail.ended()
                            {
                                outcomes[match trail.outcome.unwrap() {
                                    Outcome::Successful => 0,
                                    Outcome::JackedOut => 1,
                                    Outcome::Ended => 2,
                                }] += 1;
                                trails[i] = None;
                            }
                        }
                    }
                    SessionStep::Applied { .. } => {}
                    SessionStep::Ended { .. } => break,
                    SessionStep::Stalled(reason) => panic!("seed {seed}: {reason:?}"),
                }
            }
        }
        assert!(runs > 20, "{runs} runs");
        assert!(encounters > 0, "an ice was encountered");
        assert!(fired_seen_by_corp > 0, "the Corp saw a subroutine fire");
        assert!(outcomes[0] > 0, "a run succeeded: {outcomes:?}");
        assert!(outcomes[2] > 0, "a run was ended: {outcomes:?}");
    }

    #[test]
    fn the_steps_of_one_continue_are_read_in_order() {
        let registry = crate::decks::sample_deck_registry();
        let run = PublicRunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![
                PublicRunIce { install_id: InstallId(3), rezzed: true, identity: None },
                PublicRunIce { install_id: InstallId(4), rezzed: false, identity: None },
            ],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
            bad_publicity_credits: 0,
            bonus_run_credits: 0,
            redirect_on_approach: None,
        };
        let mut trail = RunTrail::begin(&run);
        assert_eq!(trail.stage, Stage::AtIce(0));
        assert_eq!(trail.ice[0].state, IceState::Encountering);
        assert_eq!(trail.ice[1].state, IceState::Upcoming);
        let ice = CardId("palisade".to_string());
        for event in [
            GameEvent::SubroutineFired { card_id: ice.clone(), index: 0, effect: netrunner_core::dsl::Effect::EndTheRun },
            GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
            GameEvent::IceApproached { server: ServerId::Hq, position: 1 },
        ] {
            trail.observe(&event, &registry);
        }
        assert_eq!(trail.ice[0].state, IceState::Passed);
        assert_eq!(trail.ice[0].subs, vec![SubroutineStatus::Resolved], "the fired subroutine stays on the passed ice");
        assert_eq!(trail.ice[1].state, IceState::Approaching);
        assert_eq!(trail.stage, Stage::AtIce(1));
        assert_eq!(trail.consequences, vec!["\u{201c}End the run\u{201d} fired on Palisade"], "the printed clause, not a number");
        trail.observe(&GameEvent::ServerApproached { server: ServerId::Hq }, &registry);
        trail.observe(&GameEvent::RunJackedOut { server: ServerId::Hq }, &registry);
        assert_eq!(trail.stage, Stage::AtServer);
        assert_eq!(trail.outcome, Some(Outcome::JackedOut));
        assert!(is_step(&GameEvent::IcePassed { server: ServerId::Hq, position: 0 }));
        assert!(!is_step(&GameEvent::IceRezzed { card: ice, server: ServerId::Hq, install: InstallId(3) }));
    }
}
