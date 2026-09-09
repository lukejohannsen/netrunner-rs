//! `ort`-backed `PolicyEvaluator`: runs a trained ONNX policy/value network
//! instead of `UniformPolicyEvaluator`'s no-network baseline. Feature-gated
//! (`onnx`) since it pulls in `ort`/`ndarray` and — via `ort`'s
//! `download-binaries` feature — a prebuilt ONNX Runtime shared library,
//! none of which `PuctAgent`'s search machinery or the baseline evaluator
//! need.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use ort::session::Session;
use ort::value::Tensor;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{current_actor, get_action_mask, ActionSpace, GameState, Side};

use crate::observation::{encode_observation, OBS_SIZE};
use crate::policy::PolicyEvaluator;

#[derive(Debug, thiserror::Error)]
pub enum OnnxPolicyError {
    #[error(transparent)]
    Ort(#[from] ort::Error),
    #[error("ONNX model \"policy\" output has {actual} element(s), expected ActionSpace::SIZE ({expected})")]
    UnexpectedPolicyShape { actual: usize, expected: usize },
    #[error("ONNX model \"value\" output has {actual} element(s), expected exactly 1")]
    UnexpectedValueShape { actual: usize },
}

/// The most rows one `Session::run` may carry, and how many runs may be
/// in flight at once.
///
/// **Both are measured, and the thing they are measured against is cache
/// residency, not parallelism.** A batch-of-one forward pass reads all
/// 1,151,471 parameters — 4.6 MB — end to end, and a real game does tree
/// work between its leaf evaluations, so the weights are evicted before
/// the next one. Aggregate throughput with 8 MB of unrelated traffic
/// interleaved between calls, which is the regime self-play actually runs
/// in:
///
/// ```text
///           batch 1   batch 4   batch 8   batch 16   batch 32
/// 1 runner    2,565     9,643    13,099     18,005     19,072
/// 2 runners   2,565     9,984    16,779     22,677     26,197
/// 4 runners   1,753     8,203    18,171     36,683     46,229  rows/s
/// ```
///
/// Batch 1 is 1,753 rows/s where the same call with the weights hot is
/// 15,631 — a 9x collapse — and adding threads does not help it at all
/// (17 threads at batch 1 measured 1,700). Batching is the only lever
/// that works, because it is the only one that amortizes a weight read
/// over more than one row.
///
/// The queue self-sizes: a runner takes everything waiting, so with N
/// game threads and few outstanding requests the batches are small, and
/// they grow exactly when contention would otherwise be worst. Rejected:
/// making runners *wait* to fill a batch, which trades latency for size
/// and needs a timeout tuned per machine; the natural dynamics already
/// equilibrate because a busy runner is what lets the queue build.
const MAX_BATCH: usize = 32;
const MAX_RUNNERS: usize = 4;

/// What one evaluation returns: the raw policy logits and the scalar
/// value, in `PolicyEvaluator::evaluate`'s order.
type Answer = (Vec<f32>, f32);

/// One caller's row, and the slot its answer comes back in.
struct PendingRow {
    obs: Vec<f32>,
    /// `None` until a runner fills it. `Some(Err(()))` when the runner
    /// panicked: inference failing must stay the loud crash it was before
    /// rows shared a batch, and a waiter that is never answered would turn
    /// it into a silent hang instead — the worst possible failure for an
    /// unattended run.
    answer: Mutex<Option<Result<Answer, ()>>>,
    ready: Condvar,
}

struct BatchQueue {
    waiting: Vec<Arc<PendingRow>>,
    /// Session indices not currently held by a runner. A thread becomes a
    /// runner only by taking one, which is what bounds concurrency.
    free_runners: Vec<usize>,
}

/// One model, shared process-wide: a pool of `MAX_RUNNERS` sessions and
/// the queue feeding them.
///
/// Sharing matters twice over. Self-play used to build a session per side
/// per game — ~36 live at once on this machine, 165 MB of duplicated
/// weights that guaranteed no copy ever stayed in cache, and 8.0 ms of
/// graph parsing per game. And a shared queue is the only place rows from
/// *different games* can meet, which is where the batching has to happen:
/// one game's search evaluates strictly one leaf at a time.
struct Batcher {
    queue: Mutex<BatchQueue>,
    sessions: Vec<Mutex<Session>>,
    /// Rows per `Session::run`, `MAX_BATCH` for an ordinary exported
    /// checkpoint and `1` for a model whose outputs do not grow with the
    /// input's first dimension — see `probe_max_batch`.
    max_batch: usize,
}

impl Batcher {
    fn new(model_path: &str) -> Result<Self, OnnxPolicyError> {
        let mut sessions = Vec::with_capacity(MAX_RUNNERS);
        for _ in 0..MAX_RUNNERS {
            let mut builder = Session::builder()?;
            // One thread per session: the parallelism that matters here is
            // across games, and ORT's default pool is sized to the whole
            // machine and spin-waits. Measured worth 1.2% on wall time and
            // 9% of resident memory, with byte-identical output.
            builder = builder.with_intra_threads(1).map_err(ort::Error::from)?;
            builder = builder.with_inter_threads(1).map_err(ort::Error::from)?;
            sessions.push(Mutex::new(builder.commit_from_file(model_path)?));
        }
        let free_runners = (0..MAX_RUNNERS).collect();
        let max_batch = probe_max_batch(&sessions[0]);
        Ok(Batcher { queue: Mutex::new(BatchQueue { waiting: Vec::new(), free_runners }), sessions, max_batch })
    }

    /// Submits one row and blocks until its answer is filled in — either
    /// by this thread acting as a runner, or by whichever thread is.
    fn evaluate(&self, obs: Vec<f32>) -> Answer {
        let row = Arc::new(PendingRow { obs, answer: Mutex::new(None), ready: Condvar::new() });

        // Enqueueing and claiming a runner slot happen under one lock, so
        // a row can never be left queued with every runner having just
        // decided the queue was empty.
        let slot = {
            let mut queue = self.queue.lock().expect("batch queue mutex should never be poisoned");
            queue.waiting.push(Arc::clone(&row));
            queue.free_runners.pop()
        };

        if let Some(slot) = slot {
            self.run_until_drained(slot);
        }

        let mut answer = row.answer.lock().expect("row mutex should never be poisoned");
        while answer.is_none() {
            answer = row.ready.wait(answer).expect("row mutex should never be poisoned");
        }
        match answer.take().expect("just waited for Some") {
            Ok(answer) => answer,
            Err(()) => panic!("ONNX inference failed on an already-loaded model"),
        }
    }

    /// Runs batches on `slot` until nothing is waiting, then releases it.
    fn run_until_drained(&self, slot: usize) {
        loop {
            let batch: Vec<Arc<PendingRow>> = {
                let mut queue = self.queue.lock().expect("batch queue mutex should never be poisoned");
                if queue.waiting.is_empty() {
                    // Released under the same lock that guards `waiting`,
                    // so a row enqueued after this check always finds a
                    // free slot to claim.
                    queue.free_runners.push(slot);
                    return;
                }
                let take = queue.waiting.len().min(self.max_batch);
                queue.waiting.drain(..take).collect()
            };
            // `catch_unwind` so a failed batch cannot strand either the
            // runner slot or the rows waiting on it. The panic is still
            // raised on this thread afterwards — the behaviour before
            // batching, when a caller ran its own inference — but every
            // other row in the batch is answered first.
            let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.forward_batch(slot, &batch)));
            match attempt {
                Ok(answers) => {
                    for (row, answer) in batch.iter().zip(answers) {
                        *row.answer.lock().expect("row mutex should never be poisoned") = Some(Ok(answer));
                        row.ready.notify_all();
                    }
                }
                Err(payload) => {
                    for row in &batch {
                        *row.answer.lock().expect("row mutex should never be poisoned") = Some(Err(()));
                        row.ready.notify_all();
                    }
                    self.queue.lock().expect("batch queue mutex should never be poisoned").free_runners.push(slot);
                    std::panic::resume_unwind(payload);
                }
            }
        }
    }

    /// One `Session::run` over `batch.len()` rows, split back into one
    /// answer per row. A row's outputs do not depend on what else shares
    /// its batch — verified bitwise against one-row calls — so batching
    /// is invisible to every caller.
    fn forward_batch(&self, slot: usize, batch: &[Arc<PendingRow>]) -> Vec<Answer> {
        let rows = batch.len();
        let mut flat = Vec::with_capacity(rows * OBS_SIZE);
        for row in batch {
            flat.extend_from_slice(&row.obs);
        }
        let input = ndarray::Array2::from_shape_vec((rows, OBS_SIZE), flat)
            .expect("encode_observation always returns OBS_SIZE features");
        let input_tensor = Tensor::from_array(input).expect("a [n, OBS_SIZE] f32 array is always a valid tensor");

        let mut session = self.sessions[slot].lock().expect("ONNX session mutex should never be poisoned");
        let outputs =
            session.run(ort::inputs!["obs" => input_tensor]).expect("ONNX inference failed on an already-loaded model");

        let (_, policy_logits) =
            outputs["policy"].try_extract_tensor::<f32>().expect("model's \"policy\" output must be an f32 tensor");
        assert_eq!(
            policy_logits.len(),
            rows * ActionSpace::SIZE,
            "model's \"policy\" output must have ActionSpace::SIZE elements per row"
        );
        let (_, value_slice) =
            outputs["value"].try_extract_tensor::<f32>().expect("model's \"value\" output must be an f32 tensor");
        assert_eq!(value_slice.len(), rows, "model's \"value\" output must have exactly one element per row");

        (0..rows)
            .map(|i| (policy_logits[i * ActionSpace::SIZE..(i + 1) * ActionSpace::SIZE].to_vec(), value_slice[i]))
            .collect()
    }
}

/// Whether this model's outputs actually grow with the batch dimension.
///
/// Not every ONNX file does. `onnx_fixture`'s hand-built model answers
/// from `Constant` nodes of fixed `[1, N]` shape whatever it is fed, and
/// so would any checkpoint exported with a static batch axis. Batching
/// such a model silently hands every row the first row's answer — or, as
/// it did here first, trips the length assertion inside a runner and takes
/// the queue down with it. One two-row probe at construction is cheap and
/// turns "wrong answers or a crash" into "batching off for this model".
fn probe_max_batch(session: &Mutex<Session>) -> usize {
    let input = ndarray::Array2::<f32>::zeros((2, OBS_SIZE));
    let Ok(tensor) = Tensor::from_array(input) else { return 1 };
    let mut session = session.lock().expect("ONNX session mutex should never be poisoned");
    let Ok(outputs) = session.run(ort::inputs!["obs" => tensor]) else { return 1 };
    let policy_rows = outputs["policy"].try_extract_tensor::<f32>().is_ok_and(|(_, p)| p.len() == 2 * ActionSpace::SIZE);
    let value_rows = outputs["value"].try_extract_tensor::<f32>().is_ok_and(|(_, v)| v.len() == 2);
    if policy_rows && value_rows {
        MAX_BATCH
    } else {
        1
    }
}

/// One `Batcher` per model path, for the lifetime of the process.
fn batcher_for(model_path: &str) -> Result<Arc<Batcher>, OnnxPolicyError> {
    static BATCHERS: OnceLock<Mutex<HashMap<String, Arc<Batcher>>>> = OnceLock::new();
    let map = BATCHERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().expect("batcher registry mutex should never be poisoned");
    if let Some(existing) = map.get(model_path) {
        return Ok(Arc::clone(existing));
    }
    let batcher = Arc::new(Batcher::new(model_path)?);
    map.insert(model_path.to_string(), Arc::clone(&batcher));
    Ok(batcher)
}

/// A `PolicyEvaluator` backed by a trained ONNX model: input `"obs"` shape
/// `[1, OBS_SIZE]` (from `crate::observation::encode_observation`), outputs
/// `"policy"` shape `[1, ActionSpace::SIZE]` (raw logits — see
/// `masked_softmax`'s doc comment for why these aren't assumed
/// pre-softmaxed) and `"value"` shape `[1, 1]`.
pub struct OnnxPolicyEvaluator {
    // Shared per model path rather than owned per evaluator. Every
    // consumer builds one of these per side per game, so owning a session
    // meant ~36 of them live during self-play; they now share a pool of
    // `MAX_RUNNERS` sessions behind a queue that batches rows across
    // games. See `Batcher`.
    batcher: Arc<Batcher>,
    side: Side,
}

impl OnnxPolicyEvaluator {
    /// Loads the ONNX model at `model_path` and builds a session for it.
    /// `side` is fixed at construction (matching `UniformPolicyEvaluator::
    /// new(side)`) since `PolicyEvaluator::evaluate` itself receives no
    /// `side` parameter. It is the perspective the returned **value** is
    /// reported from, which `puct::simulate` requires to be fixed. It is
    /// not the perspective the network is asked from — that is per-node and
    /// comes from `evaluation_perspective`.
    ///
    /// **One thread per session, and it is not the speed-up it looks
    /// like.** Every consumer here builds one session per side per game
    /// and runs the games across a rayon pool, so ORT's default intra-op
    /// pool — sized to the whole machine, and spin-waiting — was giving a
    /// 20-core box 541 threads for 20 cores at load average 53. That
    /// looked like the reason self-play costs 17x more with a network
    /// seated than without (0.38 s/game against 6.25 at 128 simulations).
    /// **It is not.** Measured on an idle box, 40 games at 128
    /// simulations: 238.5 s with the default pools, 235.5 s with these —
    /// 1.2%, inside noise, with the corpora byte-identical game for game.
    ///
    /// Kept anyway, for the smaller reasons rather than the headline one:
    /// 9% less resident memory (411 MB → 375 MB) and, more usefully, a
    /// session whose cost does not change shape on a machine with a
    /// different core count. The default *would* be pathological on a
    /// 96-core host; here it merely wasn't.
    ///
    /// The real cost is `batch = 1`. A decision is ~128 forward passes,
    /// a game ~205 decisions, and each pass re-reads all 1,151,471
    /// parameters — ~121 GB of weight traffic per game for 2.3 MFLOPs of
    /// arithmetic per pass, which is memory-bandwidth bound and cannot be
    /// threaded away. Batching the leaves (the four determinizations at a
    /// root are independent trees) is the fix and changes search results,
    /// so it needs the full strength bar (ROADMAP Phase 2 §5 item 25).
    pub fn new(model_path: &str, side: Side) -> Result<Self, OnnxPolicyError> {
        Ok(Self { batcher: batcher_for(model_path)?, side })
    }

    /// One forward pass over `obs`, returning the raw policy logits and the
    /// scalar value, both from whatever perspective `obs` was encoded from.
    fn forward(&self, obs: Vec<f32>) -> (Vec<f32>, f32) {
        self.batcher.evaluate(obs)
    }
}

/// The perspective **both** heads must be asked from: whoever decides at
/// `state`, falling back to `value_side` in the states nobody decides in
/// (`StartOfTurn`, `GameOver`), where there is no action to rank and the
/// value is about to be replaced by a terminal `±1.0` anyway.
///
/// **The network is only ever trained on the side to move.** Self-play
/// encodes its observation from the awaiting side and pairs it with that
/// same side's visit counts, and `train_alpha_netrunner.py` signs the value
/// target by `active_side` (`outcome_corp if active_side == 0 else
/// -outcome_corp`). So *every* training example is "this side is to move";
/// an observation encoded from a side that is not to move is a shape the
/// network has never seen.
///
/// That is why a fixed perspective is wrong for both heads and not just the
/// policy. `ActionSpace` is heavily side-partitioned — 91.7% of its 1,646
/// slots belong to exactly one side — so a fixed-perspective policy head
/// renormalized over precisely the slots training drove toward −∞. The value
/// head's error is quieter but the same kind: asked from a non-moving side,
/// it is out of distribution rather than merely rotated.
///
/// `puct::simulate` still requires every value in the tree to be from one
/// fixed side, and it still gets one — `evaluate` negates rather than
/// re-encodes. The outcome is zero-sum (`outcome_corp` is `±1.0`, and the
/// target is signed by side), so value-from-opponent is exactly
/// −value-from-self. That is the ordinary negamax identity, and it buys the
/// correct perspective for both heads at **one** forward pass per node
/// rather than two.
fn evaluation_perspective(state: &GameState, value_side: Side) -> Side {
    current_actor(state).unwrap_or(value_side)
}

/// The divisor for a **root-relative** network leaf, or `None` for the
/// absolute leaves a network has always reported.
///
/// `None` today, and left in with its numbers on it the way
/// `puct::BREACH_OUTCOMES` is. The static evaluator went root-relative in
/// September 2026 and a network's evaluator did not, so on that engine
/// every arena leg seating a *network's* value searched the regime the
/// static one had just left: `priors-only` 0.430 (its value comes from the
/// uniform evaluator) against `value-only` 0.135, about 12σ over 384 games.
/// Anchoring the network the same way is worth a consistent **+0.035** —
/// 0.177 / 0.164 / 0.174 at scales 0.05 / 0.1 / 0.2 against 0.135 — real,
/// flat in the scale, and only a seventh of the gap. It is off because
/// every one of those numbers is from `rejected_iter_016.onnx`, a
/// checkpoint fit where the Corp won 67% against this engine's 55.3%, so
/// the measurement cannot separate the regime from the staleness. Turn it
/// on when a checkpoint trained on this engine says so
/// (ROADMAP Phase 2 §5 item 21).
///
/// The scale is what the first attempt missed: subtracting two already
/// squashed `[-1, 1]` outputs leaves a within-decision gap of ~0.01–0.05
/// against an exploration term of ~0.13 — the flat-tail pathology, not the
/// cure — where the uniform evaluator's divisor *expands* a decision-sized
/// difference. Unscaled subtraction measured 0.122, inside noise of doing
/// nothing.
///
/// **Measured twice, and the second time is why it is still `None`.** The
/// +0.035 above is from `rejected_iter_016.onnx`, a checkpoint fit to a
/// 67%-Corp engine, so it could not separate the leaf regime from its own
/// staleness. On a checkpoint trained on *this* engine (ROADMAP Phase 2 §5
/// item 22; 384-game legs, null leg exactly 0.500 with zero draws) the
/// anchor is worth **+0.008** in `value-only` — 0.3σ, nothing — and
/// **−0.101 in `both`**, the configuration promotion is actually decided
/// in, which is about 3.9σ of harm. The regime is also not what ails a
/// network value in this search: on the same legs a head that beats its
/// chair null (0.762 against 0.911) still scores 0.141, where the stale
/// one scored 0.135.
const ANCHOR_SCALE: Option<f64> = None;

impl PolicyEvaluator for OnnxPolicyEvaluator {
    fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32) {
        let mask = get_action_mask(state, registry);
        let deciding = evaluation_perspective(state, self.side);
        let (policy_logits, value) = self.forward(encode_observation(state, registry, deciding));

        // The one place the fixed-perspective convention is restored: the
        // tree wants `self.side`'s value at every node, and the network was
        // asked for the mover's.
        let value = if deciding == self.side { value } else { -value };

        (masked_softmax(&policy_logits, &mask), value)
    }

    /// The network's own root value as the anchor, leaves reported relative
    /// to it and rescaled — the symmetry of what the static evaluator got.
    /// Inert while `ANCHOR_SCALE` is `None`; see it for the measurement.
    fn anchor(&self, state: &GameState, registry: &CardRegistry) -> f32 {
        if ANCHOR_SCALE.is_some() { self.evaluate(state, registry).1 } else { 0.0 }
    }

    fn evaluate_from(&self, state: &GameState, registry: &CardRegistry, anchor: f32) -> (Vec<f32>, f32) {
        let (priors, value) = self.evaluate(state, registry);
        let value = match ANCHOR_SCALE {
            Some(scale) => (((value - anchor) as f64) / scale).tanh() as f32,
            None => value,
        };
        (priors, value)
    }
}

/// Softmax computed only over `mask`'s legal indices — illegal indices get
/// exactly `0.0` and never absorb any of the exp'd mass, and the legal
/// subset sums to `1.0`. This is deliberately *not* "softmax over
/// everything, then zero the illegal entries": that would leave the legal
/// subset summing to less than `1.0` whenever the model places nonzero
/// mass on actions it has no way to know are illegal — the model has no
/// notion of game rules, only `get_action_mask` does. Numerically stable
/// (shifts by the max legal logit before exponentiating). An all-illegal
/// mask (or empty input) returns an all-`0.0` vector rather than dividing
/// by zero.
fn masked_softmax(logits: &[f32], mask: &[bool]) -> Vec<f32> {
    debug_assert_eq!(logits.len(), mask.len());

    let max_legal =
        logits.iter().zip(mask).filter_map(|(&logit, &legal)| legal.then_some(logit)).fold(f32::NEG_INFINITY, f32::max);

    if !max_legal.is_finite() {
        return vec![0.0; logits.len()];
    }

    let exp: Vec<f32> =
        logits.iter().zip(mask).map(|(&logit, &legal)| if legal { (logit - max_legal).exp() } else { 0.0 }).collect();
    let sum: f32 = exp.iter().sum();
    if sum <= 0.0 {
        return vec![0.0; logits.len()];
    }
    exp.into_iter().map(|value| value / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- `masked_softmax`: pure-function tests, no `ort`/ONNX Runtime
    // involved at all — the primary, always-reliable coverage for this
    // module's masking logic. ---

    #[test]
    fn sums_to_one_over_the_legal_subset_and_is_zero_elsewhere() {
        let logits = vec![1.0, 5.0, -3.0, 0.5];
        let mask = vec![true, false, true, false];

        let priors = masked_softmax(&logits, &mask);
        assert_eq!(priors.len(), 4);
        assert_eq!(priors[1], 0.0);
        assert_eq!(priors[3], 0.0);
        assert!(priors[0] > 0.0 && priors[2] > 0.0);
        let sum: f32 = priors.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "sum was {sum}");
    }

    #[test]
    fn uniform_logits_yield_a_uniform_distribution_over_legal_actions() {
        let logits = vec![0.0; 6];
        let mask = vec![true, true, false, true, false, false];

        let priors = masked_softmax(&logits, &mask);
        let legal_count = mask.iter().filter(|&&legal| legal).count() as f32;
        for (index, &legal) in mask.iter().enumerate() {
            if legal {
                assert!((priors[index] - 1.0 / legal_count).abs() < 1e-6);
            } else {
                assert_eq!(priors[index], 0.0);
            }
        }
    }

    #[test]
    fn all_illegal_mask_returns_all_zero_without_panicking() {
        let logits = vec![1.0, 2.0, 3.0];
        let mask = vec![false, false, false];

        let priors = masked_softmax(&logits, &mask);
        assert_eq!(priors, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn is_stable_under_extreme_logits() {
        let logits = vec![1e30, -1e30, 500.0];
        let mask = vec![true, true, true];

        let priors = masked_softmax(&logits, &mask);
        assert!(priors.iter().all(|value| value.is_finite()));
        let sum: f32 = priors.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4, "sum was {sum}");
    }

    // --- `evaluation_perspective`: tested as the pure function it is. The
    // fixture model below emits constant logits and a constant value, so it
    // cannot tell one perspective from another — this is where the change
    // is actually pinned. ---

    use netrunner_core::rules::{GamePhase, PaidAbilityWindow, WindowCheckpoint};

    #[test]
    fn the_network_is_asked_from_the_side_that_decides_not_the_evaluators_own() {
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);

        assert_eq!(evaluation_perspective(&state, Side::Corp), Side::Corp);
        assert_eq!(
            evaluation_perspective(&state, Side::Runner),
            Side::Corp,
            "a Runner-sided evaluator at a Corp node must still ask the network as the Corp"
        );
    }

    #[test]
    fn a_paid_ability_window_moves_the_perspective_without_moving_the_phase() {
        // The case a phase-based rule would get wrong: `phase` names the
        // Runner throughout a run while the window hands priority to the
        // Corp to rez. `current_actor` is the only thing that sees it.
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
        });

        assert_eq!(evaluation_perspective(&state, Side::Runner), Side::Corp);
    }

    #[test]
    fn a_state_nobody_decides_in_falls_back_to_the_value_heads_side() {
        let mut state = GameState::new(0);
        state.phase = GamePhase::GameOver(Side::Runner);

        assert_eq!(evaluation_perspective(&state, Side::Corp), Side::Corp);
        assert_eq!(evaluation_perspective(&state, Side::Runner), Side::Runner);
    }

    // --- Constructor error path: real `ort`/ONNX Runtime involved, but no
    // model file needed. ---

    #[test]
    fn new_with_a_nonexistent_model_path_returns_err_not_a_panic() {
        let result = OnnxPolicyEvaluator::new("/nonexistent/path/to/model.onnx", Side::Corp);
        assert!(result.is_err());
    }

    // --- Full construct-and-`evaluate` round trip against a hand-built
    // fixture model (no Python/torch/onnx tooling is available in this
    // sandbox — see `crate::onnx_fixture` for how it's built; extracted
    // there, rather than kept local, so `tests/agent_adapter_test.rs` can
    // reuse it too without duplicating the protobuf encoding). ---

    use crate::onnx_fixture::write_fixture_model;

    #[test]
    fn the_value_is_negated_when_the_network_was_asked_as_the_opponent() {
        // The fixture reports a constant +0.25 from whatever perspective it
        // is handed. `GameState::new(0)` is `Mulligan(Corp)`, so the Corp
        // decides: a Corp-sided evaluator reports it as-is and a
        // Runner-sided one must report the negation, because the tree wants
        // every value from its own agent's side.
        let model_file = write_fixture_model();
        let registry = CardRegistry::new();
        let state = GameState::new(0);
        assert_eq!(current_actor(&state), Some(Side::Corp), "this test needs a state the Corp decides");

        let as_corp = OnnxPolicyEvaluator::new(model_file.path.to_str().unwrap(), Side::Corp).unwrap();
        let as_runner = OnnxPolicyEvaluator::new(model_file.path.to_str().unwrap(), Side::Runner).unwrap();

        let (_, corp_value) = as_corp.evaluate(&state, &registry);
        let (_, runner_value) = as_runner.evaluate(&state, &registry);

        assert!((corp_value - 0.25).abs() < 1e-5, "expected the fixture's +0.25, got {corp_value}");
        assert!((runner_value + 0.25).abs() < 1e-5, "expected the negation, got {runner_value}");
    }

    /// A model whose outputs do not grow with the batch dimension is
    /// detected and served one row at a time, rather than being handed
    /// four rows and returning one row's answer four times.
    ///
    /// `onnx_fixture`'s model answers from `Constant` nodes of fixed
    /// `[1, N]` shape, so it is exactly that case, and it is the reason
    /// `probe_max_batch` exists: the first version of this batcher asserted
    /// its way out of the runner instead, which stranded every row waiting
    /// behind it.
    ///
    /// The positive case — a batch of four coming back bitwise equal to
    /// four single-row calls — was verified against the real 1,151,471
    /// parameter checkpoint, on every one of 2 x 1,646 outputs. It cannot
    /// be checked here, because a fixture that returns a constant would
    /// pass whether or not batching worked.
    #[test]
    fn a_model_that_cannot_batch_is_detected_and_still_answered() {
        let model_file = write_fixture_model();
        let batcher = Batcher::new(model_file.path.to_str().unwrap()).expect("fixture model should load");
        assert_eq!(batcher.max_batch, 1, "the fixture's constant outputs do not scale with the batch");

        let (priors, value) = batcher.evaluate(vec![0.0; OBS_SIZE]);
        assert_eq!(priors.len(), ActionSpace::SIZE);
        assert!((value - 0.25).abs() < 1e-6, "expected the fixture's 0.25, got {value}");
    }

    /// Many threads through one batcher: every caller gets *its own* row's
    /// answer, not a neighbour's. The queue hands results back by
    /// `Arc<PendingRow>` rather than by position, and this is what would
    /// catch that going wrong under real contention.
    #[test]
    fn concurrent_callers_each_get_their_own_row_back() {
        let model_file = write_fixture_model();
        let batcher = Arc::new(Batcher::new(model_file.path.to_str().unwrap()).expect("fixture model should load"));

        // One distinctive row per thread, and its answer taken alone first.
        let rows: Vec<Vec<f32>> = (0..16)
            .map(|r| {
                let mut obs = vec![0.0f32; OBS_SIZE];
                obs[r * 7 % OBS_SIZE] = 1.0 + r as f32;
                obs
            })
            .collect();
        let expected: Vec<(Vec<f32>, f32)> = rows.iter().map(|obs| batcher.evaluate(obs.clone())).collect();

        std::thread::scope(|scope| {
            for (r, obs) in rows.iter().enumerate() {
                let batcher = Arc::clone(&batcher);
                let expected = &expected[r];
                scope.spawn(move || {
                    for _ in 0..8 {
                        let got = batcher.evaluate(obs.clone());
                        assert_eq!(got.1, expected.1, "thread {r} got another row's value");
                        assert_eq!(got.0, expected.0, "thread {r} got another row's policy");
                    }
                });
            }
        });

        // Every runner slot is back in the pool once the work is done.
        let queue = batcher.queue.lock().unwrap();
        assert!(queue.waiting.is_empty(), "queue should be drained");
        assert_eq!(queue.free_runners.len(), MAX_RUNNERS, "every runner slot should have been released");
    }

    #[test]
    fn constructs_and_evaluates_against_a_fixture_model() {
        let model_file = write_fixture_model();
        let evaluator = OnnxPolicyEvaluator::new(model_file.path.to_str().unwrap(), Side::Runner)
            .expect("hand-built fixture model should load successfully");

        let registry = CardRegistry::new();
        let state = GameState::new(0);
        let mask = get_action_mask(&state, &registry);

        let (priors, value) = evaluator.evaluate(&state, &registry);

        assert_eq!(priors.len(), ActionSpace::SIZE);
        // Negated: the evaluator is Runner-sided and the Corp decides at
        // `GameState::new(0)`, so the fixture's constant +0.25 is reported
        // to the tree as the Runner's −0.25. Pinned on its own above.
        assert!((value + 0.25).abs() < 1e-5, "expected the fixture's 0.25 negated, got {value}");

        let legal_count = mask.iter().filter(|&&legal| legal).count() as f32;
        for (index, &legal) in mask.iter().enumerate() {
            if legal {
                assert!((priors[index] - 1.0 / legal_count).abs() < 1e-4, "index {index}: expected uniform prior, got {}", priors[index]);
            } else {
                assert_eq!(priors[index], 0.0, "index {index} is illegal but got a nonzero prior");
            }
        }
    }
}
