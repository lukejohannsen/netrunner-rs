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
//! has ICE in front of it, otherwise early — and never early past
//! `EARLY_STAGE_ENDS` (Phase 5 §46), so stripping a board does not make a
//! game young again. The guide's third signal —
//! each side's economy on the table — is deliberately not folded in yet:
//! it would move the baseline's stage columns without a term needing it,
//! and it goes in with the first term that reads it, measured. Points are
//! read against what each side needs to win (`continuous::points_to_win`):
//! the match's threshold, so a starter game to six points is late at four,
//! less what a side's cards spare it — Issuaq Adaptics' hosted counters
//! (Phase 5 §37), which read against the match rule alone left an Issuaq
//! Corp one counter from winning in the middle of the game.

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::continuous::points_to_win;
use netrunner_core::rules::{GameState, InstallSlot, ServerId, Side};

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

/// How many more of its own turns a seat should expect from a state in
/// each stage — the horizon an installed economy card pays over (Phase 5
/// §25 Stage 5, the first term to read the stage). Read off the
/// baseline's own-turn counts a game (`diag precepts`, Casual, the
/// one-ply reference both chairs): early 7.3–7.4, middle 4.6–5.1, late
/// 3.6–4.5 — so from an early state about half of early and all of the
/// rest remain, from a middle state half of middle and late, from a
/// late state half of late. The planner's games are shorter (5.1 / 3.3 /
/// 3.2), and the numbers sit between the two. An expectation, not a
/// promise: the discount on a future credit (`FUTURE_CREDIT_WEIGHT`)
/// carries the rest of the uncertainty.
pub fn horizon(stage: Stage) -> u32 {
    match stage {
        Stage::Early => 9,
        Stage::Middle => 5,
        Stage::Late => 2,
    }
}

/// The stage `state` is in — see the module docs. Reads ICE positions,
/// public points and the public cards that change a side's target, so a
/// sample and the real state agree on it.
pub fn stage(state: &GameState, registry: &CardRegistry) -> Stage {
    let close = |side: Side| state.resources(side).agenda_points.0 >= points_to_win(state, registry, side) - 2;
    if close(Side::Corp) || close(Side::Runner) {
        return Stage::Late;
    }
    let iced = |server: ServerId| state.corp.installed.iter().any(|card| card.slot == InstallSlot::Ice && card.server == server);
    let remote_iced =
        state.corp.installed.iter().any(|card| card.slot == InstallSlot::Ice && matches!(card.server, ServerId::Remote(_)));
    if (iced(ServerId::Hq) && iced(ServerId::RnD) && remote_iced) || state.turn >= EARLY_STAGE_ENDS {
        Stage::Middle
    } else {
        Stage::Early
    }
}

/// The game turn from which a game is no longer early, whatever its board
/// says (Phase 5 §46): a game does not get younger. The board reading is
/// one the Corp moves itself, and at LEO Construction's offer to end a run
/// on HQ by trashing its only piece of ICE, Ansel 1.0, an HQ left bare
/// read the game as early again — 9 turns of horizon where it had 5 — and
/// the Corp's declared income came to +4.0 more than passing, which
/// decided the trade (+1.23) at the run's initiation. The Corp had no
/// ICE in hand and no credits to replace it for three turns.
///
/// Measured, not chosen: over a planner pass of the pool (192 games,
/// seed 1), games left the early stage by game turn 11 at the median, 13
/// at three in four and **19 at nine in ten**, and not one started a
/// later Corp turn back in it. So this is a floor almost no game's own
/// board reaches first; it moves only the reading of a board stripped
/// late, and of the one game in ten that is still building.
pub const EARLY_STAGE_ENDS: u32 = 19;

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
        let registry = CardRegistry::default();
        let stage = |state: &GameState| stage(state, &registry);
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

    /// A game does not get younger (§46): a board with HQ stripped bare is
    /// early on the turns most games are still building, and middle from
    /// `EARLY_STAGE_ENDS` on — so trashing a central's last piece of ICE
    /// late in a game does not lengthen the horizon every income is
    /// counted over.
    #[test]
    fn a_stripped_board_late_in_a_game_is_not_early() {
        let registry = CardRegistry::default();
        let mut state = GameState::new(0);
        state.corp.installed = vec![ice(ServerId::RnD), ice(ServerId::Remote(0))];
        state.turn = EARLY_STAGE_ENDS - 1;
        assert_eq!(stage(&state, &registry), Stage::Early, "HQ bare while games are still building");
        state.turn = EARLY_STAGE_ENDS;
        assert_eq!(stage(&state, &registry), Stage::Middle, "HQ bare on a turn nine games in ten are past building");
        state.corp.installed.push(ice(ServerId::Hq));
        assert_eq!(stage(&state, &registry), Stage::Middle, "and the board's own reading is unchanged");
        assert!(horizon(stage(&state, &registry)) < horizon(Stage::Early));
    }

    /// The horizon shortens with the stage and is never nothing: a card
    /// installed on the last turn still pays that turn.
    #[test]
    fn the_horizon_shortens_with_the_stage() {
        assert!(horizon(Stage::Early) > horizon(Stage::Middle) && horizon(Stage::Middle) > horizon(Stage::Late));
        assert!(horizon(Stage::Late) >= 1);
    }
}
