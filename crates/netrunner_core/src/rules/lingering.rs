//! Effects with a duration — "+2 strength for the remainder of this run",
//! "-1 strength for the remainder of this encounter".
//!
//! A continuous effect (`dsl::continuous`) is declared on a card and holds
//! for as long as the card is active, so it is scanned and never stored. A
//! *lingering* effect is created once, by something that resolved, and
//! outlives it: the card that made it can leave play and the effect stays.
//! It has to be on `GameState` — it survives a parked decision, which is the
//! State Hygiene Rule's test — and it is **resolved when it is created**:
//! a flat number on an install handle, with no `Amount` and no `Box`, so a
//! search clone copies a few words and an empty list allocates nothing.
//!
//! There were three of these lists already, spelled as fields:
//! `encounter_strength_buff`, `run_strength_buff` and `turn_strength_buff`
//! on every rig card, each with a `reset_*` that five call sites had to
//! remember (Rules Audit T8 was one that forgot: pumps carried into the
//! next run). And one that was not a list at all: `Effect::ModifyStrength`
//! wrote Leech's -1 into `RunIce::current_strength` (a field that is gone:
//! an ice's strength is `continuous::ice_strength`, asked of the table),
//! where nothing could take it back out, so "for the remainder of this
//! encounter" lasted the run.
//!
//! **Whether an entry still holds is a question about the state, asked at
//! every read** ([`LingeringEffect::holds`]): the run it was made in is
//! over, the encounter it was made in is not the one happening. So nothing
//! depends on *when* the list is swept — `checkpoint::expire_durations`
//! sweeps it at every checkpoint and `run::engine::end_run` when a run is
//! taken down, and both are garbage collection. The one thing a derived
//! answer cannot tell apart is two runs, which is why `end_run` sweeps.

use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::RunPhase;
use crate::rules::state::{GameState, InstallId, InstalledRunnerCard};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LingeringEffect {
    pub what: Lingering,
    /// The installed card it is about — a rig card or a piece of ice;
    /// install handles are one sequence, so the handle says which. A
    /// handle and nothing wider, until a lingering effect is about a
    /// server or a player (the prohibitions, Rules Audit backlog item 2's
    /// last stage).
    pub on: InstallId,
    pub until: Until,
    /// The card whose text made it, for whoever shows it. Public to both
    /// players: every source is a card both watched resolve.
    pub source: CardId,
}

/// What changes. Only what a card in the pool does for a duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lingering {
    Strength(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Until {
    /// The end of the encounter with this ice.
    EndOfEncounter(InstallId),
    EndOfRun,
    /// The end of this turn, by `GameState::turn`.
    EndOfTurn(u32),
}

impl LingeringEffect {
    /// Whether the duration is still running, read off the state.
    pub fn holds(&self, state: &GameState) -> bool {
        match self.until {
            Until::EndOfEncounter(ice) => state.active_run.as_ref().is_some_and(|run| {
                run.phase == RunPhase::EncounterIce && run.ice.get(run.position).is_some_and(|encountered| encountered.install_id == ice)
            }),
            Until::EndOfRun => state.active_run.is_some(),
            Until::EndOfTurn(turn) => state.turn == turn,
        }
    }
}

/// What the lingering effects that still hold add to the strength of `on`.
pub fn strength(state: &GameState, on: InstallId) -> i32 {
    state
        .lingering
        .iter()
        .filter(|effect| effect.on == on && effect.holds(state))
        .map(|effect| match effect.what {
            Lingering::Strength(delta) => delta,
        })
        .sum()
}

/// A rig card's strength before the table is asked: what it prints (as
/// installed) and the pumps still running on it.
pub fn rig_strength(state: &GameState, card: &InstalledRunnerCard) -> i32 {
    card.base_strength + strength(state, card.install_id)
}

/// Drops what no longer holds.
pub(crate) fn sweep(state: &mut GameState) {
    if state.lingering.is_empty() {
        return;
    }
    let mut lingering = std::mem::take(&mut state.lingering);
    lingering.retain(|effect| effect.holds(state));
    state.lingering = lingering;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::run::{RunIce, RunState};

    fn effect(on: u32, amount: i32, until: Until) -> LingeringEffect {
        LingeringEffect { what: Lingering::Strength(amount), on: InstallId(on), until, source: CardId("source".to_string()) }
    }

    fn ice(install: u32) -> RunIce {
        RunIce {
            card_id: CardId("ice".to_string()),
            install_id: InstallId(install),
            ice_type: crate::dsl::IceType::Barrier,
            subroutines: Vec::new(),
            rezzed: true,
        }
    }

    fn encountering(ice_list: Vec<RunIce>, position: usize) -> GameState {
        GameState {
            active_run: Some(RunState { phase: RunPhase::EncounterIce, ice: ice_list, position, ..Default::default() }),
            ..Default::default()
        }
    }

    /// Each duration is a question about the state, so there is no moment
    /// at which one has to be remembered: the encounter pump is gone the
    /// instant the encounter is, whatever ended it.
    #[test]
    fn an_effect_holds_for_as_long_as_the_state_says_its_duration_is_running() {
        let mut state = encountering(vec![ice(10), ice(11)], 0);
        state.lingering = vec![
            effect(1, 1, Until::EndOfEncounter(InstallId(10))),
            effect(1, 2, Until::EndOfRun),
            effect(1, 4, Until::EndOfTurn(state.turn)),
            effect(2, 8, Until::EndOfRun),
        ];
        assert_eq!(strength(&state, InstallId(1)), 7, "all three, and not the other card's");

        state.active_run.as_mut().unwrap().phase = RunPhase::ApproachIce;
        assert_eq!(strength(&state, InstallId(1)), 6, "the encounter is over");

        let run = state.active_run.as_mut().unwrap();
        (run.phase, run.position) = (RunPhase::EncounterIce, 1);
        assert_eq!(strength(&state, InstallId(1)), 6, "and an encounter with the next ice is not that encounter");

        state.active_run = None;
        assert_eq!(strength(&state, InstallId(1)), 4);
        state.turn += 1;
        assert_eq!(strength(&state, InstallId(1)), 0);
    }

    #[test]
    fn a_sweep_drops_exactly_what_no_longer_holds() {
        let mut state = encountering(vec![ice(10)], 0);
        state.lingering = vec![effect(1, 1, Until::EndOfEncounter(InstallId(10))), effect(1, 2, Until::EndOfTurn(state.turn))];
        sweep(&mut state);
        assert_eq!(state.lingering.len(), 2);

        state.active_run = None;
        sweep(&mut state);
        assert_eq!(state.lingering, vec![effect(1, 2, Until::EndOfTurn(state.turn))]);
    }

    /// A state recorded before the list existed, or with nothing on it,
    /// reads and writes as it always did.
    #[test]
    fn an_empty_list_is_not_serialized() {
        let json = serde_json::to_string(&GameState::default()).unwrap();
        assert!(!json.contains("lingering"));
        assert!(serde_json::from_str::<GameState>(&json).unwrap().lingering.is_empty());
    }
}
