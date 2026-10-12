//! Whom a queue pairs, and which chair each takes (Phase 4 §9): a pure
//! model with no clock, no lock and no store — seeks in, a partner and a
//! seating out — so the balance rule is tested on made-up histories
//! rather than through a daemon.
//!
//! **Three rules, in order** (decided with the person, 11 October 2026):
//!
//! 1. **The hard constraint.** A seek that brought one deck plays that
//!    deck's side, so two Corp decks never meet.
//! 2. **The preferences.** A seek that brought both decks may prefer a
//!    side (`Wants::Either`'s `prefer`) — a soft filter, never a refusal,
//!    because a preference that could refuse is a queue that can deadlock.
//!    A newcomer is paired with the waiter whose seating meets the most
//!    preferences, and seated the way that meets them.
//! 3. **The swap.** When the preferences leave the chairs open and the
//!    two keys have met before, they swap the chairs they last took
//!    against each other: "if the same people end up playing each other
//!    swapping seats at the table should most likely happen to balance."
//!    Only then a coin.
//!
//! **A preference waits only so long.** A waiter passed over for better
//! fits could wait for ever in a busy queue whose stream of newcomers all
//! prefer its side, so a waiter that has waited `PATIENCE` is ranked as a
//! perfect fit: the first newcomer who can sit opposite takes it.
//! Otherwise the queue is first come, first served among equal fits.
//!
//! **Whom, not only where.** Choosing the partner by fit, rather than
//! taking the first compatible waiter and only seating by preference, is
//! what makes a preference worth stating: two "prefer Corp" waiters and a
//! "prefer Runner" newcomer should be the pair that is happy, whichever
//! of the two arrived first.

use netrunner_core::rules::Side;

/// How long a waiter's preference counts in choosing its partner, in
/// seconds. A minute: long enough that two people who want opposite
/// chairs meet in a queue with any traffic, short enough that nobody
/// notices the wait.
pub const PATIENCE: u64 = 60;

/// Which chair a seek can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wants {
    /// One deck, a Corp one: the Corp's chair or nothing.
    Corp,
    /// One deck, a Runner one.
    Runner,
    /// A deck for each side, and the side preferred, if any.
    Either { prefer: Option<Side> },
}

impl Wants {
    /// The side this seek must play, if its deck fixes one.
    pub fn fixed(self) -> Option<Side> {
        match self {
            Wants::Corp => Some(Side::Corp),
            Wants::Runner => Some(Side::Runner),
            Wants::Either { .. } => None,
        }
    }

    fn prefers(self) -> Option<Side> {
        match self {
            Wants::Either { prefer } => prefer,
            Wants::Corp | Wants::Runner => None,
        }
    }
}

/// A seek as the queue weighs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seek {
    pub wants: Wants,
    /// The key it proved, as the store names it (`key:<base32>`): what
    /// the swap remembers. A seat that proved no key has no history,
    /// since a name is not an identity.
    pub key: Option<String>,
    /// When it joined the queue, in seconds on any clock the caller keeps
    /// to one queue.
    pub since: u64,
}

/// Which of two seeks takes the Corp's chair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corp {
    First,
    Second,
}

/// Whether two seeks can be one game: not if both must play one side.
pub fn compatible(a: Wants, b: Wants) -> bool {
    !matches!((a.fixed(), b.fixed()), (Some(x), Some(y)) if x == y)
}

/// How many of the two seeks' preferences seating `corp` in the Corp's
/// chair would meet.
fn met(a: Wants, b: Wants, corp: Corp) -> usize {
    let (a_side, b_side) = match corp {
        Corp::First => (Side::Corp, Side::Runner),
        Corp::Second => (Side::Runner, Side::Corp),
    };
    usize::from(a.prefers() == Some(a_side)) + usize::from(b.prefers() == Some(b_side))
}

/// How many stated preferences the best seating of the two leaves unmet:
/// 0 is a pair that is happy whichever way the swap goes.
fn unmet(a: Wants, b: Wants) -> usize {
    let stated = usize::from(a.prefers().is_some()) + usize::from(b.prefers().is_some());
    let best = [Corp::First, Corp::Second].into_iter().filter(|&corp| allowed(a, b, corp)).map(|corp| met(a, b, corp)).max().unwrap_or(0);
    stated - best
}

/// Whether the hard constraint lets `corp` take the Corp's chair.
fn allowed(a: Wants, b: Wants, corp: Corp) -> bool {
    let (a_side, b_side) = match corp {
        Corp::First => (Side::Corp, Side::Runner),
        Corp::Second => (Side::Runner, Side::Corp),
    };
    a.fixed().is_none_or(|side| side == a_side) && b.fixed().is_none_or(|side| side == b_side)
}

/// The waiter `newcomer` is paired with, as an index into `waiting` (in
/// arrival order), or `None` when nobody waiting can sit opposite it:
/// the best fit, a waiter out of patience counting as a perfect one, and
/// the earliest among equals.
pub fn partner(waiting: &[Seek], newcomer: &Seek, now: u64) -> Option<usize> {
    waiting
        .iter()
        .enumerate()
        .filter(|(_, waiter)| compatible(waiter.wants, newcomer.wants))
        .min_by_key(|(index, waiter)| {
            let patient = now.saturating_sub(waiter.since) < PATIENCE;
            (if patient { unmet(waiter.wants, newcomer.wants) } else { 0 }, *index)
        })
        .map(|(index, _)| index)
}

/// Which of `a` and `b` takes the Corp's chair: the one the hard
/// constraint allows; else the seating that meets more preferences;
/// else the swap of the chairs they last took against each other
/// (`last_corp`: which of the two keys was the Corp, `true` for `a`);
/// else `coin`.
pub fn seat(a: &Seek, b: &Seek, last_corp: Option<bool>, coin: bool) -> Corp {
    let can = |corp| allowed(a.wants, b.wants, corp);
    match (can(Corp::First), can(Corp::Second)) {
        (true, false) => return Corp::First,
        (false, true) => return Corp::Second,
        _ => {}
    }
    match met(a.wants, b.wants, Corp::First).cmp(&met(a.wants, b.wants, Corp::Second)) {
        std::cmp::Ordering::Greater => Corp::First,
        std::cmp::Ordering::Less => Corp::Second,
        std::cmp::Ordering::Equal => match last_corp {
            Some(true) => Corp::Second,
            Some(false) => Corp::First,
            None if coin => Corp::First,
            None => Corp::Second,
        },
    }
}

/// Which of `a` and `b` was the Corp the last time their two keys met,
/// `true` for `a`, read off `last`: the Corp's key of a pair's last game,
/// asked by the two keys in either order. `None` when either seek proved
/// no key, when both are one key, or when the two have never met.
pub fn last_corp(a: &Seek, b: &Seek, last: impl Fn(&str, &str) -> Option<String>) -> Option<bool> {
    let (a_key, b_key) = (a.key.as_deref()?, b.key.as_deref()?);
    if a_key == b_key {
        return None;
    }
    let corp = last(a_key, b_key)?;
    Some(corp == a_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANY: Wants = Wants::Either { prefer: None };
    const PREFER_CORP: Wants = Wants::Either { prefer: Some(Side::Corp) };
    const PREFER_RUNNER: Wants = Wants::Either { prefer: Some(Side::Runner) };

    fn seek(wants: Wants, key: Option<&str>, since: u64) -> Seek {
        Seek { wants, key: key.map(str::to_string), since }
    }

    #[test]
    fn two_decks_for_one_side_never_meet() {
        let waiting = [seek(Wants::Corp, None, 0)];
        assert_eq!(partner(&waiting, &seek(Wants::Corp, None, 1), 1), None);
        assert_eq!(partner(&waiting, &seek(Wants::Runner, None, 1), 1), Some(0));
        assert_eq!(partner(&waiting, &seek(PREFER_CORP, None, 1), 1), Some(0), "a preference is soft: the Corp deck's waiter is taken");
        assert_eq!(seat(&waiting[0], &seek(PREFER_CORP, None, 1), None, false), Corp::First, "and the hard side wins the chair");
    }

    #[test]
    fn the_newcomer_meets_the_waiter_whose_preference_fits() {
        // Two waiters who want the Corp's chair, and one who wants the
        // Runner's — the last to arrive. A Corp-preferring newcomer is
        // paired with the Runner-preferring waiter, not the first in line.
        let waiting = [seek(PREFER_CORP, None, 0), seek(PREFER_CORP, None, 1), seek(PREFER_RUNNER, None, 2)];
        let newcomer = seek(PREFER_CORP, None, 3);
        assert_eq!(partner(&waiting, &newcomer, 3), Some(2));
        assert_eq!(seat(&waiting[2], &newcomer, None, true), Corp::Second, "the newcomer takes the Corp");
        // Among equal fits it is first come, first served.
        assert_eq!(partner(&waiting, &seek(ANY, None, 3), 3), Some(0));
        assert_eq!(partner(&waiting[..2], &seek(PREFER_CORP, None, 3), 3), Some(0));
    }

    #[test]
    fn a_waiter_out_of_patience_is_taken_first() {
        let waiting = [seek(PREFER_CORP, None, 0), seek(PREFER_RUNNER, None, 30)];
        let newcomer = seek(PREFER_CORP, None, 40);
        assert_eq!(partner(&waiting, &newcomer, 40), Some(1), "inside a minute the fit decides");
        assert_eq!(partner(&waiting, &newcomer, PATIENCE), Some(0), "after it the first in line is next");
    }

    #[test]
    fn preferences_seat_the_pair_before_the_swap() {
        let (corp_first, runner_first) = (seek(PREFER_CORP, Some("key:a"), 0), seek(PREFER_RUNNER, Some("key:b"), 0));
        assert_eq!(seat(&corp_first, &runner_first, Some(true), false), Corp::First, "both preferences met outranks the swap");
        let any = seek(ANY, Some("key:b"), 0);
        assert_eq!(seat(&any, &corp_first, Some(false), true), Corp::Second, "one preference met outranks it too");
    }

    #[test]
    fn two_keys_who_met_before_swap_chairs() {
        let (ann, bo) = (seek(ANY, Some("key:ann"), 0), seek(ANY, Some("key:bo"), 0));
        // Ann was the Corp last time: she is the Runner now, whichever
        // order the two are named in and whatever the coin says.
        let last = |a: &str, b: &str| (a == "key:ann" || b == "key:ann").then(|| "key:ann".to_string());
        assert_eq!(last_corp(&ann, &bo, last), Some(true));
        assert_eq!(last_corp(&bo, &ann, last), Some(false));
        for coin in [false, true] {
            assert_eq!(seat(&ann, &bo, last_corp(&ann, &bo, last), coin), Corp::Second);
            assert_eq!(seat(&bo, &ann, last_corp(&bo, &ann, last), coin), Corp::First);
        }
        // Two equal "prefer Corp" seeks leave the chairs open, so the swap
        // decides between them as well.
        let (ann, bo) = (seek(PREFER_CORP, Some("key:ann"), 0), seek(PREFER_CORP, Some("key:bo"), 0));
        assert_eq!(seat(&ann, &bo, last_corp(&ann, &bo, last), true), Corp::Second);
    }

    #[test]
    fn no_history_without_two_keys() {
        let never = |_: &str, _: &str| -> Option<String> { panic!("not asked") };
        let anonymous = seek(ANY, None, 0);
        assert_eq!(last_corp(&anonymous, &seek(ANY, Some("key:bo"), 0), never), None, "a name is not an identity");
        assert_eq!(last_corp(&seek(ANY, Some("key:bo"), 0), &seek(ANY, Some("key:bo"), 0), never), None, "one key in both chairs");
        assert_eq!(last_corp(&seek(ANY, Some("key:ann"), 0), &seek(ANY, Some("key:bo"), 0), |_, _| None), None, "never met");
        assert_eq!(seat(&anonymous, &anonymous, None, true), Corp::First, "then the coin");
        assert_eq!(seat(&anonymous, &anonymous, None, false), Corp::Second);
    }

    #[test]
    fn a_rematch_series_alternates_the_chairs() {
        // The person's case: the same two keys meeting again and again in
        // a friend queue of two, both happy with either chair. Whatever
        // the coin, they take turns at the Corp.
        let mut last: Option<String> = None;
        let mut corps = Vec::new();
        for game in 0..6u64 {
            let (ann, bo) = (seek(ANY, Some("key:ann"), game), seek(ANY, Some("key:bo"), game));
            let corp = match seat(&ann, &bo, last_corp(&ann, &bo, |_, _| last.clone()), game % 3 == 0) {
                Corp::First => "key:ann",
                Corp::Second => "key:bo",
            };
            corps.push(corp);
            last = Some(corp.to_string());
        }
        assert!(corps.windows(2).all(|pair| pair[0] != pair[1]), "{corps:?}");
    }
}
