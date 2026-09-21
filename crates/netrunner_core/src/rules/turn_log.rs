//! What has happened this turn: one count per kind of moment, kept in one
//! place.
//!
//! A card that asks about the turn — "if you made a successful run this
//! turn", "play only if the Runner made a successful run during their last
//! turn", "if you played an operation this turn" — used to be answered by a
//! field of its own on `GameState`, written by whichever handler produced
//! the fact and reset by a line of its own in `turn::enter_start_of_turn`.
//! Seven fields, each with its own `EffectRequirement`, and each reset on
//! whichever side's turn its first card happened to care about: the
//! Runner's successful-run flag stood through the whole of the Corp's next
//! turn, where the view and the bots' evaluator both read it as true.
//!
//! Here the turn is counted where it is heard. `dispatcher::dispatch_event`
//! is the one door every event a card can hear goes through (a debug build
//! holds the engine to that — `dispatcher::audit`), and
//! `listeners::moments` already says what each event is an occurrence of,
//! so `record` bumps one cell per moment and nothing else in the engine
//! writes a count. `rotate` is the one reset: every turn start, both sides.
//!
//! **Constant size, no heap.** jinteki keeps the turn's events and filters
//! them (`first-event?`, `no-event?`); a `Vec` of events here would be
//! cloned with the `GameState` on every action and thousands of times per
//! search. The log is a flat array of `u8`, `Copy`, a few hundred bytes
//! beside the hundred-odd `CardId` strings a clone already allocates.
//!
//! **The key is the moment, not a list of named facts.** A `TurnFact::
//! SuccessfulRunOnHq` would be `Trigger::OnSuccessfulRunOnHq` coming back
//! one enum over — the variant `TriggeredEffect::when` deleted — and every
//! new kind of occurrence would be a Rust edit for a card with no new
//! mechanic in it. A row is a `Trigger`; a column is a `Class`, the coarse
//! thing the moment was about.
//!
//! **A class holds only what both players saw.** The Corp installs
//! facedown and the Runner is not told what an advanced card is, so a
//! count keyed by the type of *that* card would tell the Runner "an agenda
//! was installed this turn". `concealed` names the moments whose card one
//! player did not see, exhaustively, and those are counted as
//! `Kind::Unseen` — so the log needs no masking and rides in a view
//! whole.

use serde::{Deserialize, Serialize};

use crate::cards::CardRegistry;
use crate::dsl::{CardType, Trigger};
use crate::rules::event::GameEvent;
use crate::rules::listeners::{self, About, Moment};
use crate::rules::run::ServerId;
use crate::rules::state::{GameState, Side};

const TRIGGERS: usize = Trigger::ALL.len();
/// The widest of the three column sets: a card's `Kind`.
const CLASSES: usize = Kind::COUNT;

/// What a counted moment was about, as coarsely as a card in the pool
/// distinguishes — and no finer than both players saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Card(Kind),
    Server(ServerClass),
    /// A moment about nothing: whose it was.
    Of(Option<Side>),
}

/// A card's type, or `Unseen` where a player was not shown the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Unseen,
    Agenda,
    Asset,
    Operation,
    Ice,
    Hardware,
    Resource,
    Program,
    Event,
    Identity,
    Upgrade,
}

impl Kind {
    const COUNT: usize = 11;

    fn of(card_type: &CardType) -> Kind {
        match card_type {
            CardType::Agenda => Kind::Agenda,
            CardType::Asset => Kind::Asset,
            CardType::Operation => Kind::Operation,
            CardType::Ice(_) => Kind::Ice,
            CardType::Hardware => Kind::Hardware,
            CardType::Resource => Kind::Resource,
            CardType::Program => Kind::Program,
            CardType::Event => Kind::Event,
            CardType::Identity => Kind::Identity,
            CardType::Upgrade => Kind::Upgrade,
        }
    }
}

/// Which server, with the remotes as one: "a remote server" is as fine as
/// a card prints, and a remote's number is not a fixed-size key.
/// `RunnerState::servers_run_this_turn` is the list for a card that needs
/// the server itself (Red Team).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerClass {
    Archives,
    RnD,
    Hq,
    Remote,
}

impl Class {
    fn column(self) -> usize {
        match self {
            Class::Card(kind) => kind as usize,
            Class::Server(server) => server as usize,
            Class::Of(None) => 0,
            Class::Of(Some(Side::Corp)) => 1,
            Class::Of(Some(Side::Runner)) => 2,
        }
    }
}

/// Whether the card a `trigger`'s moment is about was hidden from one of
/// the players when it happened, so its type may not be counted.
///
/// Exhaustive, because the answer is a masking decision: a new `Trigger`
/// about a card does not compile until someone says who saw the card.
fn concealed(trigger: Trigger, of: Option<Side>) -> bool {
    match trigger {
        // The Corp installs facedown; the Runner's installs are faceup.
        Trigger::OnInstall => of == Some(Side::Corp),
        // `mask_event_for_player` strikes the advanced card's identity, and
        // the Corp is not shown what is accessed out of R&D.
        Trigger::OnAdvance | Trigger::OnAccessed => true,
        // Played, rezzed, encountered, scored, stolen, forfeited, trashed
        // faceup by the Runner, or a faceup card's ability: on the table.
        Trigger::OnPlay
        | Trigger::OnOperationPlayed
        | Trigger::OnCardInstalled
        | Trigger::OnTrashedFromAccess
        | Trigger::OnAgendaScored
        | Trigger::OnAgendaStolen
        | Trigger::OnForfeit
        | Trigger::OnRez
        | Trigger::OnEncounter
        | Trigger::OnAbilityGainedCredits => false,
        // Not about a card.
        Trigger::OnRunStart
        | Trigger::OnIceApproached
        | Trigger::OnApproachServer
        | Trigger::OnSuccessfulRun
        | Trigger::OnRunEnded
        | Trigger::OnTurnStart
        | Trigger::OnActionPhaseEnd
        | Trigger::OnDiscardPhaseEnd
        | Trigger::OnBasicDrawAction
        | Trigger::OnTagsGiven
        | Trigger::OnTagRemoved
        | Trigger::OnDamageDealt
        | Trigger::OnCardsTrashedFromHq
        | Trigger::OnDamageAboutToResolve
        | Trigger::OnTrashAboutToResolve
        | Trigger::Paid => false,
    }
}

fn class_of(registry: &CardRegistry, moment: &Moment) -> Class {
    match &moment.about {
        About::Nothing => Class::Of(moment.of),
        About::Server(ServerId::Archives) => Class::Server(ServerClass::Archives),
        About::Server(ServerId::RnD) => Class::Server(ServerClass::RnD),
        About::Server(ServerId::Hq) => Class::Server(ServerClass::Hq),
        About::Server(ServerId::Remote(_)) => Class::Server(ServerClass::Remote),
        About::Card { .. } if concealed(moment.trigger, moment.of) => Class::Card(Kind::Unseen),
        About::Card { card, .. } => Class::Card(registry.get(card).map_or(Kind::Unseen, |definition| Kind::of(&definition.card_type))),
    }
}

/// One turn's counts. See the module doc.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Sparse", into = "Sparse")]
pub struct TurnLog {
    counts: [[u8; CLASSES]; TRIGGERS],
    /// Actions the active side has *finished* this turn — every basic click
    /// action and run, not scoring (which is not an action) and not a click
    /// spent as a paid-ability cost. Petty Cash's "play only if you have
    /// not finished an action yet this turn". Finishing an action is not an
    /// event any card hears, so `engine::apply_action` records it; and it
    /// is not derivable from clicks, because Petty Cash itself refunds the
    /// click it cost when played from Archives, so "clicks still at the
    /// turn's starting value" would let a second copy follow the first.
    actions_finished: u8,
    /// The printed agenda points on agendas scored this turn — a sum, where
    /// every cell above is a count (Neurospike).
    agenda_points_scored: u8,
}

impl Default for TurnLog {
    fn default() -> Self {
        TurnLog { counts: [[0; CLASSES]; TRIGGERS], actions_finished: 0, agenda_points_scored: 0 }
    }
}

impl std::fmt::Debug for TurnLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The cells that are not zero: a derived `Debug` prints three
        // hundred of them into every failed `assert_eq!` on a state.
        std::fmt::Debug::fmt(&Sparse::from(*self), f)
    }
}

impl TurnLog {
    /// How many times `trigger`'s moment has happened, whatever it was
    /// about.
    pub fn times(&self, trigger: Trigger) -> u32 {
        self.counts[trigger.index()].iter().map(|count| u32::from(*count)).sum()
    }

    /// How many of those were about `class`.
    pub fn times_about(&self, trigger: Trigger, class: Class) -> u32 {
        u32::from(self.counts[trigger.index()][class.column()])
    }

    pub fn actions_finished(&self) -> u32 {
        u32::from(self.actions_finished)
    }

    pub fn agenda_points_scored(&self) -> u32 {
        u32::from(self.agenda_points_scored)
    }

    /// The log a `ClientView` lets a bot rebuild: the two facts the view
    /// states, and nothing else. **What the successful run was on is not
    /// one of them**, so the cell is an arbitrary one of its row — a sample
    /// answers "did a run succeed this turn" and must not be asked where.
    /// The view carrying the log itself is what replaces this.
    pub fn from_what_a_view_shows(actions_finished: u32, made_successful_run: bool) -> TurnLog {
        let mut log = TurnLog { actions_finished: u8::try_from(actions_finished).unwrap_or(u8::MAX), ..TurnLog::default() };
        if made_successful_run {
            log.bump(Trigger::OnSuccessfulRun, Class::Server(ServerClass::Hq));
        }
        log
    }

    fn bump(&mut self, trigger: Trigger, class: Class) {
        let cell = &mut self.counts[trigger.index()][class.column()];
        *cell = cell.saturating_add(1);
    }
}

/// The turn before this one, as far as any card asks: how many times each
/// moment happened, not what each was about. No card in the pool prints a
/// "last turn" narrower than "made a successful run", and a second whole
/// table would be carried by every clone for a question nobody puts.
///
/// **Whose turn "last turn" was is the rotation's:** it is the turn that
/// ended most recently, either side's, so on the Corp's turn it is the
/// Runner's — which is when an operation printed "during their last turn"
/// (Public Trail, Measured Response) can be played at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "Vec<(Trigger, u8)>", into = "Vec<(Trigger, u8)>")]
pub struct LastTurn {
    times: [u8; TRIGGERS],
}

impl LastTurn {
    pub fn times(&self, trigger: Trigger) -> u32 {
        u32::from(self.times[trigger.index()])
    }
}

impl From<Vec<(Trigger, u8)>> for LastTurn {
    fn from(rows: Vec<(Trigger, u8)>) -> Self {
        let mut last = LastTurn::default();
        for (trigger, times) in rows {
            last.times[trigger.index()] = times;
        }
        last
    }
}

impl From<LastTurn> for Vec<(Trigger, u8)> {
    fn from(last: LastTurn) -> Self {
        Trigger::ALL.iter().map(|trigger| (*trigger, last.times[trigger.index()])).filter(|(_, times)| *times > 0).collect()
    }
}

/// `TurnLog` on the wire and in a failed assertion: the cells that are not
/// zero. A `StateUpdate` would otherwise carry three hundred zeros, and
/// serde derives arrays only to 32.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sparse {
    /// `(trigger, column, count)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    cells: Vec<(Trigger, u8, u8)>,
    #[serde(default, skip_serializing_if = "is_zero")]
    actions_finished: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    agenda_points_scored: u8,
}

fn is_zero(count: &u8) -> bool {
    *count == 0
}

impl From<TurnLog> for Sparse {
    fn from(log: TurnLog) -> Self {
        let mut cells = Vec::new();
        for trigger in Trigger::ALL {
            for (column, count) in log.counts[trigger.index()].iter().enumerate() {
                if *count > 0 {
                    cells.push((trigger, column as u8, *count));
                }
            }
        }
        Sparse { cells, actions_finished: log.actions_finished, agenda_points_scored: log.agenda_points_scored }
    }
}

impl From<Sparse> for TurnLog {
    fn from(sparse: Sparse) -> Self {
        let mut log = TurnLog { actions_finished: sparse.actions_finished, agenda_points_scored: sparse.agenda_points_scored, ..TurnLog::default() };
        for (trigger, column, count) in sparse.cells {
            // A column off the end is a log written by some other build;
            // dropping the cell beats a panic in a deserializer.
            if let Some(cell) = log.counts[trigger.index()].get_mut(usize::from(column)) {
                *cell = count;
            }
        }
        log
    }
}

/// Counts `event`'s moments. Called by `dispatcher::dispatch_event` before
/// anything reacts, so a card asking about the turn while it reacts to an
/// occurrence finds that occurrence already counted.
pub(crate) fn record(state: &mut GameState, registry: &CardRegistry, event: &GameEvent) {
    for moment in listeners::moments(state, event) {
        let class = class_of(registry, &moment);
        state.this_turn.bump(moment.trigger, class);
    }
    // The one sum. The points are on the event, so this is still the one
    // door: an agenda scored by a card's text is counted like any other.
    if let GameEvent::AgendaScored { agenda_points, .. } = event {
        let points = u8::try_from(*agenda_points).unwrap_or(u8::MAX);
        state.this_turn.agenda_points_scored = state.this_turn.agenda_points_scored.saturating_add(points);
    }
}

/// An action was finished — see `TurnLog::actions_finished`.
pub(crate) fn record_action_finished(state: &mut GameState) {
    state.this_turn.actions_finished = state.this_turn.actions_finished.saturating_add(1);
}

/// A turn begins: this turn becomes last turn. The one reset, for both
/// sides at every turn start — five fields each had a line of their own in
/// `turn::enter_start_of_turn`, under whichever side's branch their first
/// card cared about.
pub(crate) fn rotate(state: &mut GameState) {
    let mut last = LastTurn::default();
    for trigger in Trigger::ALL {
        last.times[trigger.index()] = u8::try_from(state.this_turn.times(trigger)).unwrap_or(u8::MAX);
    }
    state.last_turn = last;
    state.this_turn = TurnLog::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardDefinition, CardId};
    use crate::rules::state::InstallId;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::default();
        registry.insert(CardDefinition { id: CardId("an_agenda".into()), side: Side::Corp, card_type: CardType::Agenda, ..CardDefinition::default() });
        registry.insert(CardDefinition { id: CardId("a_program".into()), side: Side::Runner, card_type: CardType::Program, ..CardDefinition::default() });
        registry
    }

    #[test]
    fn a_moment_is_counted_by_what_it_was_about_and_a_turn_start_makes_it_last_turn() {
        let registry = registry();
        let mut state = GameState::default();
        record(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Hq });
        record(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Remote(3) });
        record(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Remote(4) });
        assert_eq!(state.this_turn.times(Trigger::OnSuccessfulRun), 3);
        assert_eq!(state.this_turn.times_about(Trigger::OnSuccessfulRun, Class::Server(ServerClass::Hq)), 1);
        assert_eq!(state.this_turn.times_about(Trigger::OnSuccessfulRun, Class::Server(ServerClass::Remote)), 2);
        assert_eq!(state.this_turn.times(Trigger::OnRunStart), 0);
        // An event no card can hear is an occurrence of nothing.
        record(&mut state, &registry, &GameEvent::CardDrawn { side: Side::Runner });
        assert_eq!(Sparse::from(state.this_turn).cells.len(), 2);

        record_action_finished(&mut state);
        record(&mut state, &registry, &GameEvent::AgendaScored { card: CardId("an_agenda".into()), agenda_points: 3, server: ServerId::Remote(0) });
        assert_eq!((state.this_turn.actions_finished(), state.this_turn.agenda_points_scored()), (1, 3));
        rotate(&mut state);
        assert_eq!(state.this_turn, TurnLog::default());
        assert_eq!(state.last_turn.times(Trigger::OnSuccessfulRun), 3);
        rotate(&mut state);
        assert_eq!(state.last_turn, LastTurn::default());
    }

    /// The fog rule of the module doc: the Runner's program is counted as a
    /// program, the Corp's facedown agenda as a card nobody saw.
    #[test]
    fn a_corp_install_is_counted_without_its_type() {
        let registry = registry();
        let mut state = GameState::default();
        let corp = GameEvent::CardInstalled {
            side: Side::Corp,
            install: InstallId(1),
            card: Some(CardId("an_agenda".into())),
            server: ServerId::Remote(0),
        };
        record(&mut state, &registry, &corp);
        assert_eq!(state.this_turn.times_about(Trigger::OnInstall, Class::Card(Kind::Unseen)), 1);
        assert_eq!(state.this_turn.times_about(Trigger::OnInstall, Class::Card(Kind::Agenda)), 0);
    }

    #[test]
    fn a_log_survives_serde_and_an_empty_one_is_written_as_nothing() {
        let registry = registry();
        let mut state = GameState::default();
        assert_eq!(serde_json::to_string(&state.this_turn).unwrap(), "{}");
        record(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Archives });
        record_action_finished(&mut state);
        let read: TurnLog = serde_json::from_str(&serde_json::to_string(&state.this_turn).unwrap()).unwrap();
        assert_eq!(read, state.this_turn);
        rotate(&mut state);
        let read: LastTurn = serde_json::from_str(&serde_json::to_string(&state.last_turn).unwrap()).unwrap();
        assert_eq!(read, state.last_turn);
    }
}
