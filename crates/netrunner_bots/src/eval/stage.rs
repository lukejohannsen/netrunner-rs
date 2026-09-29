//! Where the game is, read off the board — the strategy guide's three
//! stages, as one function both the precepts report and any term that
//! conditions on a stage share (Phase 5 §25 Stage 3).
//!
//! **A reading, never a dial.** Two attempts to stage this evaluator by
//! interpolating between weight profiles lost on both chairs (Phase 5
//! §6–§8, §19), and the record's lesson is that a stage is a *condition
//! inside a term* — the fort terms fall away once the wall is beaten, the
//! from-hand line rises — never a switch between weight sets. So this
//! module says which stage the board is in and nothing about what to
//! weigh there.
//!
//! **The definition is the one the baseline was taken on** (`diag
//! precepts`, Stage 1): late once either side is within two points of the
//! target, otherwise middle once both HQ and R&D are iced and some remote
//! has ICE in front of it, otherwise early. The guide's third signal —
//! each side's economy on the table — is deliberately not folded in yet:
//! it would move the baseline's stage columns without a term needing it,
//! and it goes in with the first term that reads it, measured. Points are
//! read against `MatchRules::winning_agenda_points`, so a starter game to
//! six points is late at four.

use netrunner_core::rules::{GameState, InstallSlot, ServerId};

/// The strategy guide's three stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Early,
    Middle,
    Late,
}

impl Stage {
    /// The word a report keys a stage's lines by.
    pub fn name(self) -> &'static str {
        match self {
            Stage::Early => "early",
            Stage::Middle => "middle",
            Stage::Late => "late",
        }
    }
}

/// The stage `state` is in — see the module docs. Reads ICE positions and
/// public points only, so a sample and the real state agree on it.
pub fn stage(state: &GameState) -> Stage {
    let target = state.rules.winning_agenda_points as i32;
    let best = state.corp.resources.agenda_points.0.max(state.runner.resources.agenda_points.0);
    if best >= target - 2 {
        return Stage::Late;
    }
    let iced = |server: ServerId| state.corp.installed.iter().any(|card| card.slot == InstallSlot::Ice && card.server == server);
    let remote_iced =
        state.corp.installed.iter().any(|card| card.slot == InstallSlot::Ice && matches!(card.server, ServerId::Remote(_)));
    if iced(ServerId::Hq) && iced(ServerId::RnD) && remote_iced {
        Stage::Middle
    } else {
        Stage::Early
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{AgendaPoints, InstalledCard};

    fn ice(server: ServerId) -> InstalledCard {
        InstalledCard { server, slot: InstallSlot::Ice, ..Default::default() }
    }

    /// The board and the clock, in the order the report reads them: an
    /// empty board is early, iced centrals alone are still early, a
    /// remote with ICE in front makes it middle, and either side within
    /// two points of the target makes it late whatever is built.
    #[test]
    fn the_stage_is_read_off_the_board_and_the_clock() {
        let mut state = GameState::new(0);
        assert_eq!(stage(&state), Stage::Early, "an empty board");
        state.corp.installed = vec![ice(ServerId::Hq), ice(ServerId::RnD)];
        assert_eq!(stage(&state), Stage::Early, "centrals iced, no remote");
        state.corp.installed.push(ice(ServerId::Remote(0)));
        assert_eq!(stage(&state), Stage::Middle, "a remote with ICE in front");
        state.runner.resources.agenda_points = AgendaPoints(2);
        assert_eq!(stage(&state), Stage::Middle, "two agendas from seven");
        state.corp.resources.agenda_points = AgendaPoints(5);
        assert_eq!(stage(&state), Stage::Late, "two from seven");
        let mut fresh = GameState::new(0);
        fresh.runner.resources.agenda_points = AgendaPoints(6);
        assert_eq!(stage(&fresh), Stage::Late, "the clock beats an empty board");
        assert!(Stage::Early < Stage::Middle && Stage::Middle < Stage::Late, "the order is the game's");
    }
}
