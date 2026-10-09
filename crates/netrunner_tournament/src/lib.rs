//! Swiss rounds for a Netrunner tournament, as Null Signal Games' Organized
//! Play Policies run a *single-sided* Swiss (v1.6.2, section numbers
//! theirs): one game a round, the software choosing and balancing the
//! sides (1.1.5.2); a win 3, a tie 1, a loss 0 (1.1.4); a bye a full win,
//! to a random player in round 1 and afterwards to the lowest-scoring
//! player who has not had one (1.1.3); standings broken by Strength of
//! Schedule, then Extended Strength of Schedule, then randomly (1.1.6.2
//! to 1.1.6.4).
//!
//! **Pure and engine-free.** An entrant is whatever the caller names one
//! by (`K`: the server's public keys, a test's letters), a result is one
//! of three words, and nothing here reads a clock or a socket. Every
//! function is a fold over the rounds so far, which is what lets a client
//! recompute the standings the server shows from the rounds it publishes,
//! and a disputed standing be a recomputation rather than an argument
//! (Phase 4 §7 stage 6's design).
//!
//! **The seeding is the randomness.** Every "random" the policies ask for
//! — round 1's order, round 1's bye, the last tiebreak — is read off the
//! order of `seeding`, a shuffle the caller made once when registration
//! closed and published with the rounds. Done once rather than per call
//! so the pairing is a deterministic function of what everyone can see.
//!
//! **Tiebreakers as Cobra computes them** (the software most events use):
//! an entrant's Strength of Schedule is the mean over their opponents of
//! that opponent's points per round played, byes counting as rounds the
//! opponent played and never as opponents; Extended Strength of Schedule
//! is the mean of their opponents' Strength of Schedule.
//!
//! **A drop is a player, not an erasure.** An entrant who leaves
//! mid-event keeps their row in the standings with every result they
//! have — their opponents' Strength of Schedule still counts the games
//! played against them, as Cobra's does — and is paired no more: `pair`
//! is told who has dropped and leaves them out of the tables and the bye.
//! Their unplayed table of the round they left is the caller's to settle
//! (a forfeit, the policies' answer to a player who is not there).

use std::collections::HashSet;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

/// Points for a game won (1.1.4), and for a bye (1.1.3).
pub const WIN: u32 = 3;
/// Points for a game tied.
pub const TIE: u32 = 1;
/// Points for a game lost.
pub const LOSS: u32 = 0;

/// The chair a player sits in at a table. The crate's own type rather than
/// the engine's `Side`, so this crate stays engine-free; consumers convert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Corp,
    Runner,
}

/// How a table's game ended. A tie is a stall nobody won, or a draw the
/// players agreed and the organizer recorded (2.5.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    CorpWon,
    RunnerWon,
    Tie,
}

impl Outcome {
    /// The points each chair takes from this result, Corp then Runner.
    pub fn points(self) -> (u32, u32) {
        match self {
            Outcome::CorpWon => (WIN, LOSS),
            Outcome::RunnerWon => (LOSS, WIN),
            Outcome::Tie => (TIE, TIE),
        }
    }
}

/// One table of a round: who sits where, and how it ended once it has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table<K> {
    pub corp: K,
    pub runner: K,
    pub result: Option<Outcome>,
}

impl<K: PartialEq> Table<K> {
    /// The chair `key` sits in here, if they sit here at all.
    pub fn role_of(&self, key: &K) -> Option<Role> {
        if self.corp == *key {
            Some(Role::Corp)
        } else if self.runner == *key {
            Some(Role::Runner)
        } else {
            None
        }
    }

    pub fn opponent_of(&self, key: &K) -> Option<&K> {
        match self.role_of(key)? {
            Role::Corp => Some(&self.runner),
            Role::Runner => Some(&self.corp),
        }
    }

    /// The points `key` took here, once the table has a result.
    pub fn points_for(&self, key: &K) -> Option<u32> {
        let (corp, runner) = self.result?.points();
        match self.role_of(key)? {
            Role::Corp => Some(corp),
            Role::Runner => Some(runner),
        }
    }
}

/// One round: its tables, and the entrant who sat out with a bye when the
/// count was odd.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Round<K> {
    pub tables: Vec<Table<K>>,
    pub bye: Option<K>,
}

impl<K: PartialEq> Round<K> {
    /// Whether every table has a result, so the next round can be paired.
    pub fn complete(&self) -> bool {
        self.tables.iter().all(|table| table.result.is_some())
    }

    /// The table `key` sits at this round, by its index, if they are not
    /// the bye.
    pub fn table_of(&self, key: &K) -> Option<(usize, &Table<K>)> {
        self.tables.iter().enumerate().find(|(_, table)| table.role_of(key).is_some())
    }
}

/// One entrant's place in the standings, and the record behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Standing<K> {
    pub key: K,
    pub points: u32,
    /// Strength of Schedule (1.1.6.2), the first tiebreaker.
    pub sos: f64,
    /// Extended Strength of Schedule (1.1.6.3), the second.
    pub esos: f64,
    pub wins: u32,
    pub ties: u32,
    pub losses: u32,
    pub byes: u32,
    pub corp_games: u32,
    pub runner_games: u32,
}

impl<K> Standing<K> {
    /// Rounds this entrant has a score from: tables with a result, and
    /// byes.
    pub fn rounds_played(&self) -> u32 {
        self.wins + self.ties + self.losses + self.byes
    }
}

/// The standings after `rounds`, best first: points, then Strength of
/// Schedule, then Extended Strength of Schedule, then the seeding's order
/// — the "random" the policies ask for last (1.1.6.4), fixed once when the
/// seeding was made. Every entrant of the seeding has a row, with zeros
/// before any round.
pub fn standings<K: Clone + Eq + Hash>(seeding: &[K], rounds: &[Round<K>]) -> Vec<Standing<K>> {
    let mut rows: Vec<Standing<K>> = seeding.iter().map(|key| record_of(key, rounds)).collect();
    // Strength of Schedule needs every opponent's points per round first,
    // so it is a second pass; Extended Strength of Schedule a third.
    let per_round = |row: &Standing<K>| if row.rounds_played() == 0 { 0.0 } else { f64::from(row.points) / f64::from(row.rounds_played()) };
    let find = |rows: &[Standing<K>], key: &K| rows.iter().position(|row| row.key == *key);
    let opponents: Vec<Vec<usize>> = seeding.iter().map(|key| opponents_of(key, rounds).iter().filter_map(|opponent| find(&rows, opponent)).collect()).collect();
    let sos: Vec<f64> = opponents.iter().map(|faced| mean(faced.iter().map(|&index| per_round(&rows[index])))).collect();
    let esos: Vec<f64> = opponents.iter().map(|faced| mean(faced.iter().map(|&index| sos[index]))).collect();
    for (index, row) in rows.iter_mut().enumerate() {
        row.sos = sos[index];
        row.esos = esos[index];
    }
    let seed_index: Vec<usize> = (0..rows.len()).collect();
    let mut order: Vec<(usize, Standing<K>)> = seed_index.into_iter().zip(rows).collect();
    order.sort_by(|(a_seed, a), (b_seed, b)| b.points.cmp(&a.points).then(b.sos.total_cmp(&a.sos)).then(b.esos.total_cmp(&a.esos)).then(a_seed.cmp(b_seed)));
    order.into_iter().map(|(_, row)| row).collect()
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0u32), |(sum, count), value| (sum + value, count + 1));
    if count == 0 { 0.0 } else { sum / f64::from(count) }
}

/// Everyone `key` has sat across from, once per table, in round order.
/// A bye is not an opponent.
fn opponents_of<'a, K: PartialEq>(key: &K, rounds: &'a [Round<K>]) -> Vec<&'a K> {
    rounds.iter().filter_map(|round| round.table_of(key)).filter(|(_, table)| table.result.is_some()).filter_map(|(_, table)| table.opponent_of(key)).collect()
}

fn record_of<K: Clone + PartialEq>(key: &K, rounds: &[Round<K>]) -> Standing<K> {
    let mut row = Standing { key: key.clone(), points: 0, sos: 0.0, esos: 0.0, wins: 0, ties: 0, losses: 0, byes: 0, corp_games: 0, runner_games: 0 };
    for round in rounds {
        if round.bye.as_ref() == Some(key) {
            row.byes += 1;
            row.points += WIN;
            continue;
        }
        let Some((_, table)) = round.table_of(key) else { continue };
        match table.role_of(key) {
            Some(Role::Corp) => row.corp_games += 1,
            Some(Role::Runner) => row.runner_games += 1,
            None => {}
        }
        match table.points_for(key) {
            Some(WIN) => row.wins += 1,
            Some(TIE) => row.ties += 1,
            Some(_) => row.losses += 1,
            None => {}
        }
        row.points += table.points_for(key).unwrap_or(0);
    }
    row
}

/// The next round's pairing, from the standings after `rounds`.
///
/// - **The bye** goes to the lowest-standing entrant who has not had one
///   (1.1.3), when the count is odd; in round 1 the standings are the
///   seeding, so that is the seeding's last, which is the random player
///   the policy asks for. With everyone having had one, the lowest gets
///   a second.
/// - **Tables** pair neighbours in the standings, avoiding a rematch
///   where any pairing without one exists (a search over the standings,
///   nearest partner first); when none does, neighbours are paired as
///   they stand and a rematch is allowed.
/// - **Sides** go to the player who has played that side less; when the
///   two want the same side the one who has played it less takes it,
///   and when they are even too, the lower-standing player takes the
///   Corp — so round 1's sides follow the seeding, as its order does.
///
/// `dropped` are paired no more: out of the tables and out of the bye,
/// their rows still in the standings everyone else is ordered by.
///
/// A seeding of one entrant is a round of no tables and that entrant's
/// bye; of none, an empty round.
pub fn pair<K: Clone + Eq + Hash>(seeding: &[K], rounds: &[Round<K>], dropped: &[K]) -> Round<K> {
    let standings = standings(seeding, rounds);
    let mut order: Vec<&Standing<K>> = standings.iter().filter(|row| !dropped.contains(&row.key)).collect();
    let bye = if order.len() % 2 == 1 {
        let at = order.iter().rposition(|row| row.byes == 0).unwrap_or(order.len() - 1);
        Some(order.remove(at).key.clone())
    } else {
        None
    };
    let played: HashSet<(usize, usize)> = {
        let index = |key: &K| order.iter().position(|row| row.key == *key);
        rounds
            .iter()
            .flat_map(|round| round.tables.iter())
            .filter_map(|table| Some(unordered(index(&table.corp)?, index(&table.runner)?)))
            .collect()
    };
    let indices: Vec<usize> = (0..order.len()).collect();
    let mut pairs = Vec::with_capacity(order.len() / 2);
    if !pair_without_rematch(&indices, &played, &mut pairs) {
        pairs = indices.chunks(2).map(|pair| (pair[0], pair[1])).collect();
    }
    let tables = pairs
        .into_iter()
        .map(|(higher, lower)| {
            let (a, b) = (order[higher], order[lower]);
            let wants_corp = |row: &Standing<K>| row.corp_games < row.runner_games;
            let a_corp = match (wants_corp(a), wants_corp(b)) {
                (true, false) => true,
                (false, true) => false,
                _ => a.corp_games < b.corp_games,
            };
            let (corp, runner) = if a_corp { (a, b) } else { (b, a) };
            Table { corp: corp.key.clone(), runner: runner.key.clone(), result: None }
        })
        .collect();
    Round { tables, bye }
}

fn unordered(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Pairs `remaining` (in standing order) with no two who have met,
/// nearest partner first, backing out of a choice that leaves the rest
/// unpairable. Small events make this instant; the rematch set is sparse
/// enough that the first choice nearly always stands.
fn pair_without_rematch(remaining: &[usize], played: &HashSet<(usize, usize)>, out: &mut Vec<(usize, usize)>) -> bool {
    let Some((&first, rest)) = remaining.split_first() else { return true };
    for (at, &partner) in rest.iter().enumerate() {
        if played.contains(&unordered(first, partner)) {
            continue;
        }
        out.push((first, partner));
        let mut left = rest.to_vec();
        left.remove(at);
        if pair_without_rematch(&left, played, out) {
            return true;
        }
        out.pop();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(corp: char, runner: char, result: Option<Outcome>) -> Table<char> {
        Table { corp, runner, result }
    }

    fn standing_of(rows: &[Standing<char>], key: char) -> &Standing<char> {
        rows.iter().find(|row| row.key == key).unwrap()
    }

    #[test]
    fn a_win_is_three_a_tie_one_a_bye_a_win() {
        let seeding = ['a', 'b', 'c'];
        let rounds = [Round { tables: vec![table('a', 'b', Some(Outcome::Tie))], bye: Some('c') }];
        let rows = standings(&seeding, &rounds);
        assert_eq!((standing_of(&rows, 'a').points, standing_of(&rows, 'b').points, standing_of(&rows, 'c').points), (TIE, TIE, WIN));
        assert_eq!(standing_of(&rows, 'c').byes, 1);
        assert_eq!(rows[0].key, 'c', "the bye leads");
        assert_eq!(Outcome::CorpWon.points(), (3, 0));
        assert_eq!(Outcome::RunnerWon.points(), (0, 3));
    }

    /// Strength of Schedule is the mean of the opponents' points per round;
    /// an unplayed table and a bye are nobody's opponent.
    #[test]
    fn strength_of_schedule_reads_the_opponents_and_breaks_a_tie() {
        let seeding = ['a', 'b', 'c', 'd'];
        let rounds = [
            Round { tables: vec![table('a', 'b', Some(Outcome::CorpWon)), table('c', 'd', Some(Outcome::CorpWon))], bye: None },
            // a and c met; b beat d. a's opponents: b (3/2) and c (3/2); c's: d (0/2) and a (6/2).
            Round { tables: vec![table('c', 'a', Some(Outcome::RunnerWon)), table('d', 'b', Some(Outcome::RunnerWon))], bye: None },
        ];
        let rows = standings(&seeding, &rounds);
        assert_eq!(rows.iter().map(|row| (row.key, row.points)).collect::<Vec<_>>(), vec![('a', 6), ('b', 3), ('c', 3), ('d', 0)]);
        let (a, b, c) = (standing_of(&rows, 'a'), standing_of(&rows, 'b'), standing_of(&rows, 'c'));
        assert_eq!(a.sos, 1.5);
        assert_eq!(b.sos, 1.5, "b faced a (3.0) and d (0.0)");
        assert_eq!(c.sos, 1.5, "c faced d (0.0) and a (3.0)");
        // b and c tie on points and SoS; Extended SoS decides: b's
        // opponents a and d have SoS 1.5 and 1.5; c's d and a the same —
        // so the seeding breaks it, b before c.
        assert_eq!((b.esos, c.esos), (1.5, 1.5));
        assert!(rows.iter().position(|row| row.key == 'b') < rows.iter().position(|row| row.key == 'c'));
        let open = [Round { tables: vec![table('a', 'b', None)], bye: None }];
        let rows = standings(&['a', 'b'], &open);
        assert!(rows.iter().all(|row| row.points == 0 && row.sos == 0.0 && row.rounds_played() == 0), "an unplayed table scores nothing yet");
        assert!(!open[0].complete());
    }

    /// Round 1 pairs the seeding's neighbours with the last as the bye;
    /// the later bye goes to the lowest who has not had one.
    #[test]
    fn the_bye_goes_to_the_lowest_without_one_and_round_one_follows_the_seeding() {
        let seeding = ['a', 'b', 'c', 'd', 'e'];
        let first = pair(&seeding, &[], &[]);
        assert_eq!(first.bye, Some('e'));
        assert_eq!(first.tables.len(), 2);
        assert_eq!(first.tables.iter().map(|t| (t.corp, t.runner)).collect::<Vec<_>>(), vec![('b', 'a'), ('d', 'c')], "even sides: the lower-standing takes the Corp");
        let mut played = first.clone();
        played.tables[0].result = Some(Outcome::RunnerWon);
        played.tables[1].result = Some(Outcome::CorpWon);
        // a 3, d 3, e 3 (bye), b 0, c 0: the lowest without a bye is c.
        let second = pair(&seeding, &[played], &[]);
        assert_eq!(second.bye, Some('c'));
        let rows = standings(&seeding, &[]);
        assert_eq!(rows.iter().map(|row| row.key).collect::<Vec<_>>(), seeding.to_vec(), "before any round the standings are the seeding");
        assert_eq!(pair(&['a'], &[], &[]), Round { tables: vec![], bye: Some('a') });
        assert_eq!(pair::<char>(&[], &[], &[]), Round { tables: vec![], bye: None });
    }

    /// Two who have met are not paired again while any other pairing is
    /// possible; sides go to the player who has played that side less.
    #[test]
    fn a_rematch_is_avoided_and_sides_are_balanced() {
        let seeding = ['a', 'b', 'c', 'd'];
        let first = Round { tables: vec![table('a', 'b', Some(Outcome::CorpWon)), table('c', 'd', Some(Outcome::CorpWon))], bye: None };
        // a 3, c 3, b 0, d 0: a and c meet (a was Corp, c was Corp — c,
        // lower-standing, takes the Corp... both played Corp once, so
        // neither "wants" it, and equal Corp games gives it to the lower).
        let second = pair(&seeding, std::slice::from_ref(&first), &[]);
        assert_eq!(second.tables.iter().map(|t| (t.corp, t.runner)).collect::<Vec<_>>(), vec![('c', 'a'), ('d', 'b')]);
        let mut second_played = second.clone();
        second_played.tables[0].result = Some(Outcome::RunnerWon);
        second_played.tables[1].result = Some(Outcome::RunnerWon);
        // a 6, b 3, c 3, d 0. a has met b and c: the search pairs a with d
        // and b with c (who have not met) rather than a with b again.
        let third = pair(&seeding, &[first, second_played], &[]);
        let pairs: Vec<(char, char)> = third.tables.iter().map(|t| unordered_chars(t.corp, t.runner)).collect();
        assert_eq!(pairs, vec![('a', 'd'), ('b', 'c')]);
        // a: Corp 1, Runner 1; d: Corp 1, Runner 1 — even, so the lower
        // (d) takes the Corp. b: Corp 0 Runner 2 wants Corp; c: Corp 2.
        assert_eq!(third.tables.iter().map(|t| (t.corp, t.runner)).collect::<Vec<_>>(), vec![('d', 'a'), ('b', 'c')]);
    }

    /// A drop keeps its row and its opponents' Strength of Schedule, and
    /// is neither paired nor given the bye — even when it is the lowest
    /// without one.
    #[test]
    fn a_drop_is_paired_no_more_and_keeps_its_row() {
        let seeding = ['a', 'b', 'c', 'd', 'e', 'f'];
        let first = Round { tables: vec![table('a', 'b', Some(Outcome::CorpWon)), table('c', 'd', Some(Outcome::CorpWon)), table('e', 'f', Some(Outcome::CorpWon))], bye: None };
        // Six became five: a bye, to the lowest remaining without one.
        // b and f are both on 0 with no bye; b stands above f (the
        // seeding breaks their tie), so f sits out, and d is at no table.
        let second = pair(&seeding, std::slice::from_ref(&first), &['d']);
        assert_eq!(second.bye, Some('f'));
        assert_eq!(second.tables.len(), 2);
        assert!(second.tables.iter().all(|t| t.corp != 'd' && t.runner != 'd'));
        let rows = standings(&seeding, &[first]);
        assert_eq!(rows.len(), 6, "the drop keeps its row");
        assert_eq!(standing_of(&rows, 'c').sos, 0.0, "c's one opponent, d, scored nothing — and still counts");
        // Two left of three: no bye at all.
        let two = pair(&['a', 'b', 'c'], &[], &['c']);
        assert_eq!((two.bye, two.tables.len()), (None, 1));
    }

    /// When every pairing is a rematch, neighbours are paired anyway.
    #[test]
    fn a_rematch_is_allowed_when_nothing_else_is_left() {
        let seeding = ['a', 'b'];
        let rounds = [Round { tables: vec![table('a', 'b', Some(Outcome::CorpWon))], bye: None }];
        let next = pair(&seeding, &rounds, &[]);
        assert_eq!(next.tables.len(), 1);
        assert_eq!(unordered_chars(next.tables[0].corp, next.tables[0].runner), ('a', 'b'));
        assert_eq!((next.tables[0].corp, next.tables[0].runner), ('b', 'a'), "a played Corp, so b takes it");
    }

    #[test]
    fn a_round_survives_the_wire() {
        let round = Round { tables: vec![table('a', 'b', Some(Outcome::Tie))], bye: Some('c') };
        let text = serde_json::to_string(&round).unwrap();
        assert_eq!(serde_json::from_str::<Round<char>>(&text).unwrap(), round);
        assert_eq!(round.table_of(&'b').map(|(index, table)| (index, table.role_of(&'b'), table.opponent_of(&'b').copied())), Some((0, Some(Role::Runner), Some('a'))));
        assert_eq!(round.table_of(&'c'), None);
    }

    fn unordered_chars(a: char, b: char) -> (char, char) {
        if a <= b { (a, b) } else { (b, a) }
    }
}
