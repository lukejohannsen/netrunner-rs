#!/usr/bin/env python3
"""Trains the policy/value network on a corpus of `netrunner_selfplay` games.

The corpus is read in the sparse `[index, value]` form `netrunner_selfplay`
writes (see `crates/netrunner_selfplay/src/schema.rs` for why), held sparse
in memory, and densified one batch at a time. The dense form held every zero
of 990 + 1,646 floats per decision as a tensor, which is what capped an
iteration at 96 games (ROADMAP Phase 2 §5); sparse, a decision is ~40 pairs.
"""
import argparse
import glob
import json
import math
import os
import random
import sys
import time

import numpy as np
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from action_space_segments import SEGMENTS, segment_of_slot
import torch.nn as nn
import torch.nn.functional as F


class AlphaNetrunnerNet(nn.Module):
    def __init__(self, obs_dim=30, action_dim=724, hidden_dim=256):
        super().__init__()
        self.trunk = nn.Sequential(
            nn.Linear(obs_dim, hidden_dim),
            nn.LayerNorm(hidden_dim),
            nn.ReLU(),
            nn.Linear(hidden_dim, hidden_dim),
            nn.LayerNorm(hidden_dim),
            nn.ReLU(),
        )
        self.policy_head = nn.Sequential(
            nn.Linear(hidden_dim, hidden_dim),
            nn.ReLU(),
            nn.Linear(hidden_dim, action_dim)
        )
        self.value_head = nn.Sequential(
            nn.Linear(hidden_dim, 64),
            nn.ReLU(),
            nn.Linear(64, 1),
            nn.Tanh()
        )

    def forward(self, obs):
        feat = self.trunk(obs)
        logits = self.policy_head(feat)
        value = self.value_head(feat)
        return logits, value


def corpus_files(data_dir: str, window):
    """The trajectory files to train on.

    `run_iteration_loop.py` writes one subdirectory per iteration, so
    `window` (the last N subdirectories in sorted order) is AlphaZero's
    replay window: recent play only, rather than every game since the start.
    Without a window, every `.jsonl` under `data_dir` is used.
    """
    if window is None:
        return sorted(glob.glob(os.path.join(data_dir, "**", "*.jsonl"), recursive=True))
    subdirs = sorted(d for d in glob.glob(os.path.join(data_dir, "*")) if os.path.isdir(d))
    files = []
    for subdir in subdirs[-window:]:
        files.extend(sorted(glob.glob(os.path.join(subdir, "**", "*.jsonl"), recursive=True)))
    return files


class SparseRows:
    """A CSR-shaped store of sparse rows, densified a batch at a time."""

    def __init__(self, width: int):
        self.width = width
        self.indptr = [0]
        self.indices = []
        self.values = []

    def append(self, pairs):
        for index, value in pairs:
            self.indices.append(index)
            self.values.append(value)
        self.indptr.append(len(self.indices))

    def freeze(self):
        self.indptr = np.asarray(self.indptr, dtype=np.int64)
        self.indices = np.asarray(self.indices, dtype=np.int64)
        self.values = np.asarray(self.values, dtype=np.float32)

    def dense(self, rows: np.ndarray) -> np.ndarray:
        starts = self.indptr[rows]
        lengths = self.indptr[rows + 1] - starts
        total = int(lengths.sum())
        out = np.zeros((len(rows), self.width), dtype=np.float32)
        if total == 0:
            return out
        # Positions of every nonzero of the batch inside `indices`, vectorised:
        # each row's run starts at `starts[i]` and the runs are laid end to end.
        row_of = np.repeat(np.arange(len(rows)), lengths)
        offsets = np.arange(total) - np.repeat(np.cumsum(lengths) - lengths, lengths)
        positions = np.repeat(starts, lengths) + offsets
        out[row_of, self.indices[positions]] = self.values[positions]
        return out

    def entropy(self, rows: np.ndarray) -> float:
        """Mean Shannon entropy (nats) of the rows as distributions — the
        floor a cross-entropy against them cannot go below."""
        total = 0.0
        for row in rows:
            p = self.values[self.indptr[row]:self.indptr[row + 1]]
            p = p[p > 0]
            total += float(-(p * np.log(p)).sum())
        return total / max(1, len(rows))


#: The value-head targets `--value-target` selects between. See
#: `NetrunnerCorpus.set_value_target` for what each one is and the
#: measurement that motivates it.
VALUE_TARGET_KINDS = ("outcome", "mixed", "discounted", "discounted_unforeseeable")


class NetrunnerCorpus:
    """Every recorded decision of a corpus, remembering which game each came from.

    `game_of_sample[i]` is the index of the game sample `i` belongs to. The
    train/validation split is made over games, never over steps: neighbouring
    positions of one game are near-duplicates, so a split over steps puts a
    game on both sides of it and the validation loss falls by memorising
    games rather than judging positions — the loss that read 0.006 while the
    network lost to the uniform search on both sides (ROADMAP Phase 2 §5).

    Three things are refused rather than tolerated, because each has
    silently poisoned a corpus before: a file whose recorded widths differ
    from the rest (August's 1,357-wide targets against a 1,646-wide action
    space), two games with the same seed (a loop that replayed the same 96
    games every iteration once self-play became reproducible), and a window
    that mixes `pool_fingerprint`s — two games recorded by different
    engines. The third is the second 2,400-game run: its loop shelled out to
    `cargo run` per stage, so three *Elevation* stages landing in the working
    tree recompiled self-play mid-run, the deck pool went 12 matchups to 36,
    the card planes reindexed, and every width stayed put, so nothing here
    objected (ROADMAP Phase 2 §5).

    Stalled games are *dropped* rather than refused, and that is the fourth
    lesson from the same run: a game that hit `MAX_STEPS` is not a draw, it
    is ~10,000 cycling near-duplicate decisions carrying a zero value
    target, and at iteration 8 nineteen such games of 2,400 held 24% of
    every recorded decision — enough to move `baseline_mse` from 1.00 to
    0.77 and make a value head that had not improved look as though it had.
    """

    def __init__(self, data_dir: str, window=None, limit_games=None):
        files = corpus_files(data_dir, window)
        self.observation_size = None
        self.action_space_size = None
        self.observations = None
        self.policies = None
        outcomes = []
        active_sides = []
        search_values = []
        search_values_absolute = []
        positions = []
        steps_to_end = []
        end_reasons = []
        self.game_of_sample = []
        self.missing_search_value = 0
        self.missing_search_value_absolute = 0
        self.outcomes = {1.0: 0, -1.0: 0, 0.0: 0}
        self.dropped_stalls = 0
        self.dropped_stall_steps = 0
        seeds = {}
        fingerprints = {}
        game_index = 0
        for filepath in files:
            if limit_games is not None and game_index >= limit_games:
                break
            with open(filepath, "r", encoding="utf-8") as f:
                for line in f:
                    if not line.strip():
                        continue
                    if limit_games is not None and game_index >= limit_games:
                        break
                    game = json.loads(line)
                    widths = (game["observation_size"], game["action_space_size"])
                    if self.observations is None:
                        self.observation_size, self.action_space_size = widths
                        self.observations = SparseRows(self.observation_size)
                        self.policies = SparseRows(self.action_space_size)
                    elif widths != (self.observation_size, self.action_space_size):
                        raise ValueError(
                            f"{filepath} was recorded with widths {widths}, the corpus so far with "
                            f"{(self.observation_size, self.action_space_size)}; a corpus must not mix layouts"
                        )
                    # An absent fingerprint is the archived corpora's
                    # own value: they were all recorded before the field
                    # existed, so they read as one engine, which is what
                    # they are.
                    fingerprint = game.get("pool_fingerprint", "")
                    if fingerprint not in fingerprints:
                        if fingerprints:
                            other, other_file = next(iter(fingerprints.items()))
                            raise ValueError(
                                f"{filepath} was recorded by engine {fingerprint or '(pre-fingerprint)'}, "
                                f"{other_file} by engine {other or '(pre-fingerprint)'}; a corpus must not mix "
                                "engines — see GameTrajectory::pool_fingerprint"
                            )
                        fingerprints[fingerprint] = filepath
                    seed = game["seed"]
                    if seed in seeds:
                        raise ValueError(f"{filepath} replays seed {seed} already recorded by {seeds[seed]}; "
                                         "self-play iterations need distinct --seed-offset values")
                    seeds[seed] = filepath
                    # A stall is not a game. Dropped after the seed is
                    # registered — a replayed iteration is still a replayed
                    # iteration — and before its steps are read, so the
                    # window's size counts decisions a search resolved.
                    end_reason = str(game.get("end_reason", ""))
                    if end_reason.startswith("stall_"):
                        self.dropped_stalls += 1
                        self.dropped_stall_steps += len(game["steps"])
                        continue
                    game_index += 1
                    outcome_corp = float(game["outcome_corp"])
                    self.outcomes[outcome_corp] = self.outcomes.get(outcome_corp, 0) + 1
                    steps = game["steps"]
                    for position, step in enumerate(steps):
                        self.observations.append(step["observation"])
                        self.policies.append(step["policy_target"])
                        outcomes.append(outcome_corp if step["active_side"] == 0 else -outcome_corp)
                        active_sides.append(int(step["active_side"]))
                        # Recorded from the acting side already, like the
                        # outcome above once it is signed.
                        if "search_value" in step:
                            search_values.append(float(step["search_value"]))
                        else:
                            search_values.append(0.0)
                            self.missing_search_value += 1
                        # Absent from every corpus recorded before September
                        # 2026. Tested for presence, not for zero: an
                        # evaluator scoring a position at dead level reports
                        # exactly 0.0, which is a reading rather than a gap
                        # (66 steps of 499,225 in the first HEAD corpus), and
                        # counting those as missing refused the whole corpus.
                        if "search_value_absolute" in step:
                            search_values_absolute.append(float(step["search_value_absolute"]))
                        else:
                            search_values_absolute.append(0.0)
                            self.missing_search_value_absolute += 1
                        positions.append(position / max(1, len(steps) - 1))
                        steps_to_end.append(len(steps) - 1 - position)
                        end_reasons.append(end_reason)
                        self.game_of_sample.append(game_index)
        if self.observations is None:
            raise ValueError(f"No trajectory steps found in '{data_dir}'")
        self.observations.freeze()
        self.policies.freeze()
        self.outcomes_by_sample = np.asarray(outcomes, dtype=np.float32)
        self.search_values = np.asarray(search_values, dtype=np.float32)
        self.positions = np.asarray(positions, dtype=np.float32)
        self.active_sides = np.asarray(active_sides, dtype=np.int8)
        self.search_values_absolute = np.asarray(search_values_absolute, dtype=np.float32)
        self.steps_to_end = np.asarray(steps_to_end, dtype=np.float32)
        # Decided by something no board feature sees: the ~40% of games that
        # end in a flatline or a deck-out rather than on agenda points
        # (ROADMAP Phase 2 §5).
        self.unforeseeable = np.isin(np.asarray(end_reasons), ("flatline", "deckout"))
        self.game_of_sample = np.asarray(self.game_of_sample, dtype=np.int64)
        self.game_count = game_index
        self.values = self.outcomes_by_sample

    def set_value_target(self, kind: str, mix: float, discount: float):
        """Build the value head's target. `self.values` is what the head is
        trained on; `outcomes_by_sample` stays the signed final outcome and is
        what every diagnostic measures against, so targets stay comparable to
        each other and to all four earlier runs.

        Rebinds `self.values` rather than writing into it — it starts as an
        *alias* of `outcomes_by_sample`, and an in-place write would corrupt
        the diagnostics along with it.

        The kinds, and the measurement behind each (ROADMAP Phase 2 §5):

        `outcome` — the signed final outcome, ±1 on every step of the game.
        The original target, and the control. Its problem is on the record:
        `sign_accuracy_by_decile` reads 54% in the opening rising to 66% at
        the end, so the opening half of every game carries a confident label
        that is nearly a coin flip.

        `mixed` — `(1 - mix)` of that plus `mix` of the search's **absolute**
        root read. What `--value-target-mix` was always meant to be. It is
        keyed to `search_values_absolute`, not `search_values`: a static
        evaluator's leaves became root-relative in September 2026, so
        `search_value` from a uniform-search corpus is a per-decision delta
        centred near zero and blending it at 0.5 would halve the target
        rather than smooth it.

        `discounted` — the outcome decayed by distance to the end,
        `outcome * discount ** steps_to_end`. The head is asked how *decided*
        a position is and for whom, which is a function of the board, instead
        of being asked to call a game 200 decisions early. A near-zero value
        at an early leaf also hands that subtree back to the priors, which
        the ablation legs measured as neutral (0.672 as Corp against a 0.693
        baseline) where the value head measured as −0.292.

        `discounted_unforeseeable` — `discounted`, but the decay applies only
        to games that ended in a flatline or a deck-out. About 40% of
        PUCT-vs-PUCT games end that way and neither a board feature nor the
        agenda-point margin sees one coming, while an agenda-point game is
        genuinely foreshadowed by the score and the board. This is the
        narrower claim: discount what is unforeseeable, keep the label where
        it was earned.
        """
        if kind not in VALUE_TARGET_KINDS:
            raise ValueError(f"unknown value target {kind!r}; expected one of {', '.join(VALUE_TARGET_KINDS)}")
        if kind == "mixed" and mix > 0.0 and self.missing_search_value_absolute:
            raise ValueError(
                f"{self.missing_search_value_absolute} steps carry no search_value_absolute; this corpus was "
                "recorded before the field existed (or by a search that never ran), so --value-target mixed "
                "would train half its target on zeros. Re-record the corpus or use --value-target outcome."
            )

        decay = np.power(discount, self.steps_to_end, dtype=np.float32)
        if kind == "outcome":
            values = self.outcomes_by_sample
        elif kind == "mixed":
            values = (1.0 - mix) * self.outcomes_by_sample + mix * self.search_values_absolute
        elif kind == "discounted":
            values = self.outcomes_by_sample * decay
        elif kind == "discounted_unforeseeable":
            values = self.outcomes_by_sample * np.where(self.unforeseeable, decay, 1.0)
        self.values = np.asarray(values, dtype=np.float32)

    def __len__(self):
        return len(self.outcomes_by_sample)

    def split_by_game(self, val_fraction: float, seed: int = 0):
        """Indices of the training and validation samples, with every game
        wholly on one side. At least one game goes to validation whenever
        there are two or more games."""
        games = list(range(1, self.game_count + 1))
        random.Random(seed).shuffle(games)
        val_count = min(len(games) - 1, max(1, round(len(games) * val_fraction))) if len(games) > 1 else 0
        val_games = np.zeros(self.game_count + 1, dtype=bool)
        val_games[games[:val_count]] = True
        is_val = val_games[self.game_of_sample]
        return np.flatnonzero(~is_val), np.flatnonzero(is_val)

    def batch(self, rows: np.ndarray, device):
        obs = torch.from_numpy(self.observations.dense(rows)).to(device)
        pi = torch.from_numpy(self.policies.dense(rows)).to(device)
        val = torch.from_numpy(self.values[rows]).to(device).unsqueeze(1)
        return obs, pi, val


def export_onnx(model: nn.Module, export_path: str, obs_dim: int):
    model.eval()
    dummy_input = torch.randn(1, obs_dim, dtype=torch.float32)
    torch.onnx.export(
        model,
        dummy_input,
        export_path,
        export_params=True,
        opset_version=17,
        do_constant_folding=True,
        input_names=["obs"],
        output_names=["policy", "value"],
        dynamic_axes={
            "obs": {0: "batch_size"},
            "policy": {0: "batch_size"},
            "value": {0: "batch_size"}
        },
        dynamo=False
    )


def losses(model, obs, target_pi, target_val, masked_policy=False, sample_weight=None):
    """Policy cross-entropy and value MSE.

    `masked_policy` renormalizes the policy softmax over the target's own
    support instead of all `ActionSpace::SIZE` slots. **The default is the
    unmasked form the three volume runs were trained with**, kept so a
    rerun reproduces them; the masked form exists because the trained
    quantity is otherwise not the one the search consumes. Inference
    (`masked_softmax`) always renormalizes over the legal set, so an
    unmasked objective spends capacity on a normalizer that is thrown
    away — measured at 41.2% of the softmax mass landing on illegal slots
    (ROADMAP Phase 2 §5).

    The support stands in for the legal set: a recorded `policy_target` is
    the root visit distribution and PUCT expands every legal root edge, so
    with 128 simulations over ~7 legal actions the two coincide.

    **Masked and unmasked policy losses are not comparable to each other.**
    Both are bounded below by the target's entropy, but only the masked one
    can approach it, so a masked run reports a smaller number for the same
    quality. Compare a variant with itself across epochs, never across
    variants.

    `sample_weight` reweights whole samples in the mean — see
    `segment_balance_weights`.
    """
    logits, val_pred = model(obs)
    if masked_policy:
        support = target_pi > 0
        # A row with no target at all (never produced by self-play, but a
        # corpus is not a promise) would make every logit -inf and the
        # log-softmax NaN. Leave those rows unmasked; their all-zero target
        # contributes nothing to the sum either way.
        support = support | ~support.any(dim=1, keepdim=True)
        logits = logits.masked_fill(~support, float("-inf"))
        log_probs = F.log_softmax(logits, dim=1)
        # A masked slot is exactly -inf and its target is exactly 0, and
        # `0 * -inf` is NaN -- which poisons the *reported* loss without
        # touching the gradient (the derivative wrt log_probs is the
        # target, i.e. 0 there). That asymmetry is nastier than a crash:
        # the first run of this trained correctly for ten epochs while
        # every printed policy loss read `nan`, so `best_val_loss` never
        # improved, no checkpoint was written, and the export died on a
        # missing file. Drop the masked terms before multiplying.
        log_probs = torch.where(support, log_probs, torch.zeros_like(log_probs))
    else:
        log_probs = F.log_softmax(logits, dim=1)
    per_sample = -torch.sum(target_pi * log_probs, dim=1)
    if sample_weight is None:
        policy_loss = per_sample.mean()
        value_loss = F.mse_loss(val_pred, target_val)
    else:
        # Weights are normalized to mean 1.0 upstream, so a weighted loss
        # stays on the same scale as an unweighted one.
        policy_loss = (per_sample * sample_weight).mean()
        value_loss = (F.mse_loss(val_pred, target_val, reduction="none").squeeze(-1) * sample_weight).mean()
    return policy_loss, value_loss


def segment_balance_weights(corpus, strength: float):
    """Per-sample weights that lift steps whose decision lives in a rare
    `ActionSpace` segment.

    **Why this exists.** The policy head under-weights `SCORE AGENDA` by
    3× — the search spends 0.659 of its visits there when scoring is legal
    and the model offers 0.204 — and scoring is legal on only 430 of
    24,548 Corp steps. A uniform objective sees those steps 57 times less
    often than ordinary ones and fits them accordingly, while the arena
    weights them enormously: they are the moves that end the game. The
    same shape holds for `rez ice` (0.49) and `activate ability` (0.31).

    A sample is attributed to the segment its *target's* argmax falls in —
    what the search actually wanted here — and weighted by
    `(mean_count / count) ** strength`, normalized to mean 1.0. `strength`
    0.0 is uniform and 1.0 is full inverse frequency; the square root
    (0.5) is the usual compromise, since full inverse frequency on a
    segment seen 430 times hands single samples enormous gradients.

    **This deliberately biases the optimum.** Cross-entropy against a soft
    target is a proper scoring rule and its optimum is the target
    distribution; reweighting moves that optimum toward the rare segments.
    That is the point — it trades calibration on common steps for fit on
    decisive ones — but it is a distortion, not a correction, and it is
    why this is off by default.
    """
    slot_segment = segment_of_slot()
    argmax_slot = np.empty(len(corpus.policies.indptr) - 1, dtype=np.int64)
    for row in range(len(argmax_slot)):
        lo, hi = corpus.policies.indptr[row], corpus.policies.indptr[row + 1]
        if hi <= lo:
            argmax_slot[row] = 0
            continue
        values = corpus.policies.values[lo:hi]
        argmax_slot[row] = corpus.policies.indices[lo + int(np.argmax(values))]
    segment = slot_segment[argmax_slot]

    counts = np.bincount(segment, minlength=len(SEGMENTS)).astype(np.float64)
    seen = counts[counts > 0]
    weights = np.where(counts > 0, (seen.mean() / np.maximum(counts, 1.0)) ** strength, 1.0)
    per_sample = weights[segment]
    return (per_sample / per_sample.mean()).astype(np.float32), segment, counts


def run_epoch(model, corpus, rows, batch_size, device, optimizer=None, value_loss_weight=1.0,
              masked_policy=False, sample_weights=None):
    """One pass over `rows`; trains when `optimizer` is given, else evaluates.
    Returns (policy loss, value loss) averaged per sample, each unweighted —
    `value_loss_weight` scales the value term in the gradient only, so the
    reported losses stay comparable across weights."""
    total_p, total_v = 0.0, 0.0
    order = np.random.permutation(rows) if optimizer is not None else rows
    for start in range(0, len(order), batch_size):
        batch_rows = order[start:start + batch_size]
        obs, target_pi, target_val = corpus.batch(batch_rows, device)
        weight = None
        if sample_weights is not None:
            weight = torch.from_numpy(sample_weights[batch_rows]).to(device)
        if optimizer is not None:
            optimizer.zero_grad()
            policy_loss, value_loss = losses(model, obs, target_pi, target_val, masked_policy, weight)
            (policy_loss + value_loss_weight * value_loss).backward()
            optimizer.step()
        else:
            with torch.no_grad():
                policy_loss, value_loss = losses(model, obs, target_pi, target_val, masked_policy, weight)
        total_p += policy_loss.item() * len(batch_rows)
        total_v += value_loss.item() * len(batch_rows)
    return total_p / max(1, len(order)), total_v / max(1, len(order))


def value_diagnostics(model, corpus, rows, batch_size, device):
    """How the value head does against the *outcome*, whatever it was trained
    on — so runs with different `--value-target-mix` stay comparable.

    `mse_vs_outcome` beside `baseline_mse` (predicting zero everywhere, which
    is the mean squared outcome, i.e. the decided-game fraction) is the test
    the head kept failing; `sign_accuracy` over decided games is the one it
    partially passed, and `sign_accuracy_by_decile` (position in the game,
    ten buckets) shows where — 54% in the opening rising to 66% at the end
    on the September 2026 corpus. Computed by hand then; recorded here so it
    is measured every time."""
    model.eval()
    preds = []
    with torch.no_grad():
        for start in range(0, len(rows), batch_size):
            batch_rows = rows[start:start + batch_size]
            obs, _target_pi, _target_val = corpus.batch(batch_rows, device)
            _logits, value = model(obs)
            preds.append(value.squeeze(1).cpu().numpy())
    pred = np.concatenate(preds) if preds else np.zeros(0, dtype=np.float32)
    outcome = corpus.outcomes_by_sample[rows]
    decided = outcome != 0.0
    sides = corpus.active_sides[rows]
    # The honest null. Predicting zero is the null for a *symmetric* game,
    # and this one is not: the Corp wins about two games in three, so a
    # predictor that knows nothing but which chair is to move already
    # scores 1 - (2p-1)^2 per chair. Measured on the fourth volume run's
    # own logs, that null is 0.882 and the head averaged 0.926 — it beat
    # the chair in two iterations of sixteen. `baseline_mse` (predict-zero,
    # 1.000 when every game is decided) is kept beside it because every
    # earlier run is recorded against it, but it is the easy null and
    # ROADMAP Phase 2 §5 item 20's reading of it was too kind.
    chair_pred = np.zeros_like(outcome)
    for side in (0, 1):
        pick = sides == side
        if pick.any():
            chair_pred[pick] = np.mean(outcome[pick])
    diag = {
        "mse_vs_outcome": float(np.mean((pred - outcome) ** 2)) if len(rows) else 0.0,
        "baseline_mse": float(np.mean(outcome ** 2)) if len(rows) else 0.0,
        "chair_baseline_mse": float(np.mean((chair_pred - outcome) ** 2)) if len(rows) else 0.0,
        "sign_accuracy": float(np.mean(np.sign(pred[decided]) == np.sign(outcome[decided]))) if decided.any() else 0.0,
    }
    # Where the head's mass actually sits, by chair: a head that has learned
    # only the base rate reports about +/-(2p-1) here and little spread.
    for side, name in ((0, "corp"), (1, "runner")):
        pick = sides == side
        diag[f"mean_pred_{name}"] = float(np.mean(pred[pick])) if pick.any() else None
        diag[f"rms_pred_{name}"] = float(np.sqrt(np.mean(pred[pick] ** 2))) if pick.any() else None
    # Split by how the game ended. If the head calls agenda-point games and
    # misses flatlines and deck-outs, the deficit is localised for free —
    # and a target built around end reason has earned its place.
    for name, pick_reason in (("unforeseeable", corpus.unforeseeable[rows]), ("foreseeable", ~corpus.unforeseeable[rows])):
        pick = decided & pick_reason
        diag[f"sign_accuracy_{name}"] = float(np.mean(np.sign(pred[pick]) == np.sign(outcome[pick]))) if pick.any() else None
        diag[f"share_{name}"] = float(np.mean(pick_reason)) if len(rows) else 0.0
    deciles = np.minimum((corpus.positions[rows] * 10).astype(int), 9)
    by_decile = []
    for d in range(10):
        pick = decided & (deciles == d)
        by_decile.append(float(np.mean(np.sign(pred[pick]) == np.sign(outcome[pick]))) if pick.any() else None)
    diag["sign_accuracy_by_decile"] = by_decile
    return diag


def train(args):
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"Using device: {device}")
    np.random.seed(args.seed)
    torch.manual_seed(args.seed)

    started = time.time()
    corpus = NetrunnerCorpus(args.data_dir, window=args.window, limit_games=args.limit_games)
    corpus.set_value_target(args.value_target, args.value_target_mix, args.value_discount)
    train_idx, val_idx = corpus.split_by_game(val_fraction=0.1, seed=args.seed)
    outcomes = corpus.outcomes
    print(f"Loaded {corpus.game_count} games, {len(corpus)} decision steps in {time.time() - started:.1f}s "
          f"(corp wins {outcomes.get(1.0, 0)}, runner wins {outcomes.get(-1.0, 0)}, stalls {outcomes.get(0.0, 0)}).")
    if corpus.dropped_stalls:
        share = corpus.dropped_stall_steps / max(1, corpus.dropped_stall_steps + len(corpus))
        print(f"Dropped {corpus.dropped_stalls} stalled games holding {corpus.dropped_stall_steps} decisions "
              f"({share:.1%} of everything recorded) — see NetrunnerCorpus's docstring.")
    print(f"Split by game: {len(train_idx)} training steps, {len(val_idx)} validation steps.")
    # A cross-entropy against a soft target bottoms out at the target's own
    # entropy, so a validation policy loss only means something relative to
    # this. Printed once so "4.5" can be read as "1.9 nats above the floor".
    policy_floor = corpus.policies.entropy(val_idx)
    print(f"Validation policy-target entropy (loss floor): {policy_floor:.4f} nats")
    described = {
        "outcome": "final outcome",
        "mixed": f"{1.0 - args.value_target_mix:.2f} x outcome + {args.value_target_mix:.2f} x absolute search root value",
        "discounted": f"outcome x {args.value_discount:g} ** steps_to_end",
        "discounted_unforeseeable": f"outcome, x {args.value_discount:g} ** steps_to_end on flatline/deckout games only",
    }[args.value_target]
    print(f"Value target ({args.value_target}): {described}; value loss weight {args.value_loss_weight:g} "
          f"| mean |target| {float(np.mean(np.abs(corpus.values))):.3f}, "
          f"unforeseeable steps {float(np.mean(corpus.unforeseeable)):.1%}")

    sample_weights = None
    if args.segment_balance > 0.0:
        sample_weights, segment, counts = segment_balance_weights(corpus, args.segment_balance)
        loudest = np.argsort(-counts)[:3]
        rarest = [i for i in np.argsort(counts) if counts[i] > 0][:3]
        print(f"Segment balance {args.segment_balance:g}: weights {sample_weights.min():.2f}-{sample_weights.max():.2f}; "
              f"most common {[SEGMENTS[i][0] for i in loudest]}, rarest {[SEGMENTS[i][0] for i in rarest]}")
    print(f"Policy objective: {'masked to the target support' if args.masked_policy else 'unmasked over all slots'}")

    model = AlphaNetrunnerNet(obs_dim=corpus.observation_size, action_dim=corpus.action_space_size).to(device)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)

    best_val_loss = float("inf")
    best_epoch = 0
    best_diag = None
    history = []
    os.makedirs(args.output_dir, exist_ok=True)
    best_pt_path = os.path.join(args.output_dir, "best_model.pt")
    epochs_since_best = 0

    for epoch in range(1, args.epochs + 1):
        epoch_started = time.time()
        model.train()
        train_p, train_v = run_epoch(model, corpus, train_idx, args.batch_size, device, optimizer,
                                     value_loss_weight=args.value_loss_weight,
                                     masked_policy=args.masked_policy, sample_weights=sample_weights)
        model.eval()
        # Validation is unweighted on purpose: the selection criterion has
        # to stay the honest distribution even when the gradient does not.
        val_p, val_v = run_epoch(model, corpus, val_idx, args.batch_size, device,
                                 masked_policy=args.masked_policy)
        diag = value_diagnostics(model, corpus, val_idx, args.batch_size, device)
        # Selected on the same weighted sum the gradient used, so the
        # checkpoint that ships is the one the training objective preferred.
        #
        # `--select-on value` exists because that sum is dominated by the
        # policy term at the default weight of 0.25: a change that improves
        # the value head need not change which epoch ships, so an experiment
        # about the value target measured through this criterion can read as
        # no change when the head did move (ROADMAP Phase 2 §5).
        val_loss = {
            "blended": val_p + args.value_loss_weight * val_v,
            "value": val_v,
            "policy": val_p,
        }[args.select_on]
        history.append({"epoch": epoch, "train_policy": train_p, "train_value": train_v,
                        "val_policy": val_p, "val_value": val_v, "val_value_diagnostics": diag})
        print(f"Epoch {epoch:02d}/{args.epochs:02d} | "
              f"Train (Policy: {train_p:.4f}, Value: {train_v:.4f}) | "
              f"Val (Policy: {val_p:.4f}, +{val_p - policy_floor:.4f} over floor, Value: {val_v:.4f}) | "
              f"Value vs outcome: MSE {diag['mse_vs_outcome']:.3f} (chair {diag['chair_baseline_mse']:.3f}, "
              f"predict-zero {diag['baseline_mse']:.3f}), "
              f"sign {diag['sign_accuracy']:.1%} | {time.time() - epoch_started:.0f}s")

        if val_loss < best_val_loss - 1e-9:
            epochs_since_best = 0
        else:
            epochs_since_best += 1
        if val_loss < best_val_loss:
            best_val_loss = val_loss
            best_epoch = epoch
            best_diag = diag
            torch.save(model.state_dict(), best_pt_path)
        if args.early_stop_patience and epochs_since_best >= args.early_stop_patience:
            print(f"Early stop: {epochs_since_best} epochs since the best (epoch {best_epoch}).")
            break

    print("Training complete. Exporting best checkpoint to ONNX...")
    onnx_path = os.path.join(args.output_dir, "netrunner_policy.onnx")
    export_model = AlphaNetrunnerNet(obs_dim=corpus.observation_size, action_dim=corpus.action_space_size).to("cpu")
    export_model.load_state_dict(torch.load(best_pt_path, map_location="cpu"))
    export_onnx(export_model, onnx_path, corpus.observation_size)
    print(f"Model successfully exported to ONNX format at '{onnx_path}'")
    # One JSON line for the loop to keep, the way the arena reports.
    summary = {
        "games": corpus.game_count, "steps": len(corpus), "train_steps": int(len(train_idx)),
        "dropped_stalls": corpus.dropped_stalls, "dropped_stall_steps": corpus.dropped_stall_steps,
        "val_steps": int(len(val_idx)), "policy_floor": policy_floor, "best_epoch": best_epoch,
        "best_val_loss": best_val_loss, "value_target": args.value_target,
        "value_target_mix": args.value_target_mix, "value_discount": args.value_discount,
        "select_on": args.select_on, "early_stop_patience": args.early_stop_patience,
        "mean_abs_value_target": float(np.mean(np.abs(corpus.values))),
        "value_loss_weight": args.value_loss_weight, "best_value_diagnostics": best_diag,
        "masked_policy": args.masked_policy, "segment_balance": args.segment_balance,
        "epochs": history, "seconds": time.time() - started,
    }
    print(json.dumps(summary))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--data-dir", "-d", type=str, required=True)
    parser.add_argument("--output-dir", "-o", type=str, default="./data/checkpoints")
    parser.add_argument("--epochs", "-e", type=int, default=10)
    parser.add_argument("--batch-size", "-b", type=int, default=64)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--window", type=int, default=None,
                        help="Train on the last N iteration subdirectories only (the replay window); default all")
    parser.add_argument("--limit-games", type=int, default=None,
                        help="Use only the first N games, in file order — for measuring how loss scales with data")
    parser.add_argument("--seed", type=int, default=0, help="Seed for the game split and batch order")
    parser.add_argument("--value-target", choices=VALUE_TARGET_KINDS, default="mixed",
                        help="Which value-head target to train (see NetrunnerCorpus.set_value_target)")
    parser.add_argument("--value-target-mix", type=float, default=0.5,
                        help="For --value-target mixed: (1 - mix) x final outcome + mix x absolute search root value")
    parser.add_argument("--value-discount", type=float, default=0.99,
                        help="For the discounted targets: outcome x discount ** steps_to_end")
    parser.add_argument("--select-on", choices=("blended", "value", "policy"), default="blended",
                        help="Which validation loss picks the exported epoch. 'blended' is "
                             "val_policy + value_loss_weight * val_value, the historical criterion, and is "
                             "policy-dominated at the default weight; 'value' is what an experiment about "
                             "the value target should select on.")
    parser.add_argument("--early-stop-patience", type=int, default=0,
                        help="Stop after this many epochs with no improvement in the selection loss "
                             "(0 = off, the historical behaviour: every epoch always runs)")
    parser.add_argument("--masked-policy", action="store_true",
                        help="Renormalize the policy softmax over the target's support instead of all "
                             "ActionSpace slots, matching what masked_softmax does at inference. Off by "
                             "default: the recorded runs were trained unmasked and a rerun must reproduce them.")
    parser.add_argument("--segment-balance", type=float, default=0.0,
                        help="Reweight samples by the inverse frequency of their target's ActionSpace segment, "
                             "raised to this power (0 = off, 0.5 = square root, 1 = full). Lifts rare decisive "
                             "decisions such as SCORE AGENDA. Biases the optimum -- see segment_balance_weights.")
    parser.add_argument("--value-loss-weight", type=float, default=0.25,
                        help="Weight of the value loss against the policy loss in the gradient and in checkpoint selection")
    args = parser.parse_args()
    train(args)
