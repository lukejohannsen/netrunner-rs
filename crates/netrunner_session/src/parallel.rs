//! Many independent games at once — how the sweeps play theirs.
//!
//! A sweep is hundreds of games that share nothing: each builds its own
//! registry and state and seeds its own agents, so the order they are
//! played in cannot change any of them. They were played one after
//! another on one thread, which on a 20-thread machine left the sweep at
//! a twentieth of it while the step that dominates a stage's wall clock
//! waited on it.
//!
//! **What stays the same is what a failure says.** A game that panics
//! stops the others from starting, and once every running game has
//! finished, the lowest-numbered failing job's own panic is re-raised —
//! so a failure still names its seed and seating, and names the same one
//! whatever order the threads happened to finish in. Results come back in
//! the jobs' order, so a caller that folds them (a `Coverage` merged game
//! by game) folds them as the sequential loop did.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// The threads a sweep plays on: `NETRUNNER_SWEEP_THREADS` when set (1
/// plays the jobs in order on one thread), otherwise every thread the
/// machine offers.
pub fn sweep_threads() -> usize {
    std::env::var("NETRUNNER_SWEEP_THREADS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|threads: &usize| *threads > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |threads| threads.get()))
}

/// Runs `play` on every job across `sweep_threads()` threads and returns
/// the results in the jobs' order. A job that panics stops the rest from
/// starting, and the lowest-numbered failing job's panic is re-raised once
/// every job already running has finished.
pub fn in_parallel<J: Sync, T: Send>(jobs: &[J], play: impl Fn(&J) -> T + Sync) -> Vec<T> {
    let workers = sweep_threads().min(jobs.len()).max(1);
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let slots: Vec<Mutex<Option<std::thread::Result<T>>>> = jobs.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else { break };
                    let outcome = panic::catch_unwind(AssertUnwindSafe(|| play(job)));
                    if outcome.is_err() {
                        stop.store(true, Ordering::Relaxed);
                    }
                    *slots[index].lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(outcome);
                }
            });
        }
    });
    // Jobs are taken in order, so every job below a failing one was taken
    // and has finished: the first failure in order is the lowest there is.
    let mut results = Vec::with_capacity(jobs.len());
    for slot in slots {
        match slot.into_inner().unwrap_or_else(|poisoned| poisoned.into_inner()) {
            Some(Ok(result)) => results.push(result),
            Some(Err(payload)) => panic::resume_unwind(payload),
            None => {}
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_come_back_in_the_jobs_order() {
        let jobs: Vec<u64> = (0..200).collect();
        assert_eq!(in_parallel(&jobs, |job| job * 2), jobs.iter().map(|job| job * 2).collect::<Vec<_>>());
    }

    #[test]
    fn the_lowest_failing_job_is_the_one_reported() {
        let jobs: Vec<u64> = (0..64).collect();
        let failure = panic::catch_unwind(|| in_parallel(&jobs, |job| if *job == 7 || *job == 40 { panic!("job {job}") } else { *job }));
        let payload = failure.expect_err("a job failed");
        let message = payload.downcast_ref::<String>().cloned().unwrap_or_default();
        assert_eq!(message, "job 7");
    }
}
