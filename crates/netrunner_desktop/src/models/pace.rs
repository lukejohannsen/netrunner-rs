//! The pace a run is shown at.
//!
//! **A bot plays a run in an instant.** The match thread sends one
//! `MatchMessage` per applied action, and a Runner bot's run — continue,
//! continue, access, done — arrives as a burst the board redraws through
//! in a frame or two, so the person watching from the Corp's chair sees
//! a run begin and end and nothing between. The screen's `poll` used to
//! drain every message into the model at once; it now pushes them here
//! and takes back *beats*, released one at a time with [`BEAT`] between
//! them while a run is on. Outside a run nothing waits: a turn of
//! credits and draws is not a thing to watch.
//!
//! **One applied action can be several beats.** `ContinueRun` can pass
//! an ice, approach the next and encounter it in one entry, so an
//! `Applied` in a run is split at every event that moves the run
//! ([`trail::is_step`]): each group of events up to and including a step
//! is a beat of its own, for the trail to observe, and the message
//! itself — the view the board redraws from — is the last. The board
//! therefore moves only when the whole action has been shown, never
//! ahead of the trail.
//!
//! **The person's own click answers at once.** The first beat of an
//! action the person submitted is not held, so pressing Continue moves
//! the trail immediately; the beats after it, and every message of the
//! bot's, wait. A `Rejected` never waits: it is the engine answering a
//! click. The wait is [`BEAT`] over `DesktopPrefs::animation_speed`,
//! whose documented 0 is "instant" — how the headless tests and a
//! person who wants none of this turn it off.

use std::collections::VecDeque;
use std::time::Duration;

use netrunner_client::board::trail;
use netrunner_client::play::MatchMessage;
use netrunner_core::rules::{GameEvent, Side};

/// The authored pause between two beats of a run, at animation speed 1.
pub const BEAT: Duration = Duration::from_millis(700);

/// What the screen takes back: a group of run events for the trail, or
/// a message for the model.
#[derive(Debug)]
pub enum Beat {
    Steps(Vec<GameEvent>),
    Apply(MatchMessage),
}

struct Queued {
    beat: Beat,
    /// The pause before this beat may be released, measured from the
    /// release before it.
    wait: Duration,
}

pub struct Pacer {
    human: Side,
    queue: VecDeque<Queued>,
    /// Whether a run was on after the last message pushed, so a message
    /// that ends one is still paced.
    in_run: bool,
    /// When the last beat was released, on the caller's clock.
    last_release: Option<Duration>,
    /// The pause between beats: `BEAT` scaled by the animation speed.
    beat: Duration,
    /// A dev hook: stop releasing at the first encounter — or the
    /// server's approach, on a server with no ice — so a screenshot
    /// catches a run in flight.
    pub hold_at_encounter: bool,
    /// Whether the hold has taken effect.
    pub held: bool,
}

impl Pacer {
    pub fn new(human: Side, animation_speed: f32) -> Self {
        Pacer { human, queue: VecDeque::new(), in_run: false, last_release: None, beat: beat_for(animation_speed), hold_at_encounter: false, held: false }
    }

    /// The animation speed changed (the options menu): the pause follows.
    pub fn set_speed(&mut self, animation_speed: f32) {
        self.beat = beat_for(animation_speed);
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Queues a message as the beats it is shown as.
    pub fn push(&mut self, message: MatchMessage) {
        let (run_after, events, own) = match &message {
            MatchMessage::Applied { entry, view } => (view.active_run.is_some(), entry.events.clone(), entry.side == self.human),
            MatchMessage::Awaiting { view } => (view.active_run.is_some(), Vec::new(), false),
            // A lesson's coaching waits in line behind the beats ahead of
            // it, so the panel changes with the decision it is about.
            // A clock is for the decision it follows, so it waits in line
            // with it.
            MatchMessage::Rejected { .. } | MatchMessage::Back { .. } | MatchMessage::Coach(_) | MatchMessage::Clock { .. } => {
                self.queue.push_back(Queued { beat: Beat::Apply(message), wait: Duration::ZERO });
                return;
            }
            // A move is only ever begun outside a run, so the board a
            // take-back restores has none.
            MatchMessage::Rewound { .. } => {
                self.in_run = false;
                self.queue.push_back(Queued { beat: Beat::Apply(message), wait: Duration::ZERO });
                return;
            }
            // A board shown as it stands — a remote seat's first, or its
            // first after a reconnect — has no steps to pace: it lands at
            // once, and what follows is paced from wherever it left the run.
            MatchMessage::Snapshot { view } => {
                self.in_run = view.active_run.is_some();
                self.queue.push_back(Queued { beat: Beat::Apply(message), wait: Duration::ZERO });
                return;
            }
            MatchMessage::Ended { .. } | MatchMessage::Stalled { .. } | MatchMessage::LessonComplete { .. } | MatchMessage::Rated { .. } => (false, Vec::new(), false),
        };
        let paced = self.in_run || run_after || events.iter().any(trail::concerns_run);
        self.in_run = run_after;
        if !paced {
            self.queue.push_back(Queued { beat: Beat::Apply(message), wait: Duration::ZERO });
            return;
        }
        // Split at every step: each group is a beat, the remainder rides
        // with the message.
        let mut groups: Vec<Vec<GameEvent>> = Vec::new();
        let mut current: Vec<GameEvent> = Vec::new();
        for event in events {
            let step = trail::is_step(&event);
            current.push(event);
            if step {
                groups.push(std::mem::take(&mut current));
            }
        }
        let mut first = true;
        for group in groups {
            let wait = if first && own { Duration::ZERO } else { self.beat };
            first = false;
            self.queue.push_back(Queued { beat: Beat::Steps(group), wait });
        }
        if !current.is_empty() {
            self.queue.push_back(Queued { beat: Beat::Steps(current), wait: Duration::ZERO });
        }
        let wait = if first && !own { self.beat } else { Duration::ZERO };
        self.queue.push_back(Queued { beat: Beat::Apply(message), wait });
    }

    /// The beats due at `now`, in order: every beat whose pause has
    /// elapsed since the release before it. With no pause, everything.
    pub fn tick(&mut self, now: Duration) -> Vec<Beat> {
        let mut out = Vec::new();
        while let Some(front) = self.queue.front() {
            if self.held {
                break;
            }
            let since = self.last_release.map_or(front.wait, |last| now.saturating_sub(last));
            if since < front.wait {
                break;
            }
            let Queued { beat, .. } = self.queue.pop_front().expect("front was Some");
            if self.hold_at_encounter && matches!(&beat, Beat::Steps(events) if events.iter().any(|e| matches!(e, GameEvent::IceEncountered { .. } | GameEvent::ServerApproached { .. }))) {
                self.held = true;
            }
            self.last_release = Some(now);
            out.push(beat);
        }
        out
    }
}

fn beat_for(animation_speed: f32) -> Duration {
    if animation_speed <= 0.0 {
        Duration::ZERO
    } else {
        BEAT.div_f32(animation_speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_client::play::{LocalMatchSpec, MatchHandle};
    use netrunner_client::start::Level;
    use std::sync::Arc;

    /// A real match against the bottom rung, the person as the Corp so
    /// every run is the bot's: pumped through a pacer with a fake clock,
    /// the beats keep the messages' order, a run's beats are spaced by
    /// the pause and nothing outside a run waits.
    #[test]
    fn a_run_is_released_a_beat_at_a_time_and_the_rest_at_once() {
        let registry = Arc::new(netrunner_client::decks::sample_deck_registry());
        let corp = netrunner_core::decks::by_id("discretion_advised").unwrap();
        let runner = netrunner_core::decks::by_id("stolen_goods").unwrap();
        let spec = LocalMatchSpec { registry, corp, runner, human: Side::Corp, level: Level::Novice, style: None, seed: 3, rules: Default::default(), record: None };
        let mut handle = MatchHandle::start_local(spec).unwrap();
        let mut pacer = Pacer::new(Side::Corp, 1.0);
        let mut now = Duration::ZERO;
        let mut applied_in_order = 0usize;
        let mut run_beats = 0usize;
        let mut instant_outside = 0usize;
        let mut last_release: Option<Duration> = None;
        let mut in_run = false;
        let mut submitted = 0;
        'game: loop {
            while let Some(message) = handle.poll() {
                pacer.push(message);
            }
            for beat in pacer.tick(now) {
                match beat {
                    Beat::Steps(events) => {
                        assert!(!events.is_empty());
                        if events.iter().any(trail::is_step) {
                            // Every step here is the bot's, so each waits
                            // the full pause after the release before it.
                            run_beats += 1;
                            if let Some(last) = last_release {
                                assert!(now - last >= BEAT, "a step beat came {:?} after the last", now - last);
                            }
                            last_release = Some(now);
                        }
                    }
                    Beat::Apply(MatchMessage::Applied { view, entry }) => {
                        applied_in_order += 1;
                        let run_now = view.active_run.is_some();
                        if !in_run && !run_now && !entry.events.iter().any(trail::concerns_run) {
                            instant_outside += 1;
                        } else {
                            last_release = Some(now);
                        }
                        in_run = run_now;
                    }
                    Beat::Apply(MatchMessage::Awaiting { view }) => {
                        // The person: the first legal action, as the panel would.
                        if let Some(action) = view.legal_actions.first() {
                            handle.submit(action.clone()).unwrap();
                            submitted += 1;
                        }
                    }
                    Beat::Apply(MatchMessage::Rejected { reason }) => panic!("{reason}"),
                    Beat::Apply(MatchMessage::Back { .. } | MatchMessage::Rewound { .. }) => {}
                    Beat::Apply(MatchMessage::Coach(_) | MatchMessage::LessonComplete { .. }) => unreachable!("a local match is not a lesson"),
                    Beat::Apply(MatchMessage::Snapshot { .. } | MatchMessage::Clock { .. } | MatchMessage::Rated { .. }) => unreachable!("a local match sends none of these"),
                    Beat::Apply(MatchMessage::Ended { .. } | MatchMessage::Stalled { .. }) => break 'game,
                }
            }
            now += Duration::from_millis(50);
            std::thread::sleep(Duration::from_millis(1));
            assert!(now < Duration::from_secs(600), "the paced game did not end");
        }
        assert!(applied_in_order > 50, "{applied_in_order} applied");
        assert!(submitted > 5, "{submitted} submitted");
        assert!(run_beats > 3, "{run_beats} run beats");
        assert!(instant_outside > 10, "{instant_outside} messages outside a run released at once");
    }

    #[test]
    fn speed_zero_releases_everything_in_one_tick_and_a_rejection_never_waits() {
        let mut pacer = Pacer::new(Side::Runner, 0.0);
        assert_eq!(beat_for(0.0), Duration::ZERO);
        assert_eq!(beat_for(2.0), Duration::from_millis(350));
        pacer.push(MatchMessage::Rejected { reason: "no".to_string() });
        pacer.push(MatchMessage::Stalled { reason: "gone".to_string() });
        let beats = pacer.tick(Duration::ZERO);
        assert_eq!(beats.len(), 2);
        assert!(matches!(beats[0], Beat::Apply(MatchMessage::Rejected { .. })));
        assert!(pacer.is_empty());
        let mut slow = Pacer::new(Side::Runner, 1.0);
        slow.in_run = true;
        slow.push(MatchMessage::Rejected { reason: "no".to_string() });
        assert_eq!(slow.tick(Duration::ZERO).len(), 1, "a rejection is the engine answering a click");
    }
}
