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
import time

import numpy as np
import torch
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


class NetrunnerCorpus:
    """Every recorded decision of a corpus, remembering which game each came from.

    `game_of_sample[i]` is the index of the game sample `i` belongs to. The
    train/validation split is made over games, never over steps: neighbouring
    positions of one game are near-duplicates, so a split over steps puts a
    game on both sides of it and the validation loss falls by memorising
    games rather than judging positions — the loss that read 0.006 while the
    network lost to the uniform search on both sides (ROADMAP Phase 2 §5).

    Two things are refused rather than tolerated, because each has silently
    poisoned a corpus before: a file whose recorded widths differ from the
    rest (August's 1,357-wide targets against a 1,646-wide action space),
    and two games with the same seed (a loop that replayed the same 96 games
    every iteration once self-play became reproducible).
    """

    def __init__(self, data_dir: str, window=None, limit_games=None):
        files = corpus_files(data_dir, window)
        self.observation_size = None
        self.action_space_size = None
        self.observations = None
        self.policies = None
        outcomes = []
        search_values = []
        positions = []
        self.game_of_sample = []
        self.missing_search_value = 0
        self.outcomes = {1.0: 0, -1.0: 0, 0.0: 0}
        seeds = {}
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
                    seed = game["seed"]
                    if seed in seeds:
                        raise ValueError(f"{filepath} replays seed {seed} already recorded by {seeds[seed]}; "
                                         "self-play iterations need distinct --seed-offset values")
                    seeds[seed] = filepath
                    game_index += 1
                    outcome_corp = float(game["outcome_corp"])
                    self.outcomes[outcome_corp] = self.outcomes.get(outcome_corp, 0) + 1
                    steps = game["steps"]
                    for position, step in enumerate(steps):
                        self.observations.append(step["observation"])
                        self.policies.append(step["policy_target"])
                        outcomes.append(outcome_corp if step["active_side"] == 0 else -outcome_corp)
                        # Recorded from the acting side already, like the
                        # outcome above once it is signed.
                        if "search_value" in step:
                            search_values.append(float(step["search_value"]))
                        else:
                            search_values.append(0.0)
                            self.missing_search_value += 1
                        positions.append(position / max(1, len(steps) - 1))
                        self.game_of_sample.append(game_index)
        if self.observations is None:
            raise ValueError(f"No trajectory steps found in '{data_dir}'")
        self.observations.freeze()
        self.policies.freeze()
        self.outcomes_by_sample = np.asarray(outcomes, dtype=np.float32)
        self.search_values = np.asarray(search_values, dtype=np.float32)
        self.positions = np.asarray(positions, dtype=np.float32)
        self.game_of_sample = np.asarray(self.game_of_sample, dtype=np.int64)
        self.game_count = game_index
        self.values = self.outcomes_by_sample

    def set_value_target_mix(self, mix: float):
        """The value head's target: `(1 - mix)` of the game's final outcome plus
        `mix` of the search's own root value at that decision, both from the
        acting side.

        The outcome alone taught the head nothing: a 64-simulation search's
        game is decided largely by opening noise, every position of a game
        carries the same label, and the head memorised games — held-out MSE
        never beat predicting zero at 96, 288, 960 or 2,400 games while the
        training loss sat at 0.01–0.05 (ROADMAP Phase 2 §5). The root value
        is what the search believed about *this* position, and it varies
        within a game. A corpus recorded before `search_value` existed can
        only be trained with `mix == 0`, and is refused otherwise rather
        than silently trained on zeros."""
        if mix > 0.0 and self.missing_search_value:
            raise ValueError(f"{self.missing_search_value} steps carry no search_value; this corpus predates it, "
                             "so --value-target-mix must be 0")
        self.values = ((1.0 - mix) * self.outcomes_by_sample + mix * self.search_values).astype(np.float32)

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


def losses(model, obs, target_pi, target_val):
    logits, val_pred = model(obs)
    log_probs = F.log_softmax(logits, dim=1)
    policy_loss = -torch.sum(target_pi * log_probs, dim=1).mean()
    value_loss = F.mse_loss(val_pred, target_val)
    return policy_loss, value_loss


def run_epoch(model, corpus, rows, batch_size, device, optimizer=None, value_loss_weight=1.0):
    """One pass over `rows`; trains when `optimizer` is given, else evaluates.
    Returns (policy loss, value loss) averaged per sample, each unweighted —
    `value_loss_weight` scales the value term in the gradient only, so the
    reported losses stay comparable across weights."""
    total_p, total_v = 0.0, 0.0
    order = np.random.permutation(rows) if optimizer is not None else rows
    for start in range(0, len(order), batch_size):
        batch_rows = order[start:start + batch_size]
        obs, target_pi, target_val = corpus.batch(batch_rows, device)
        if optimizer is not None:
            optimizer.zero_grad()
            policy_loss, value_loss = losses(model, obs, target_pi, target_val)
            (policy_loss + value_loss_weight * value_loss).backward()
            optimizer.step()
        else:
            with torch.no_grad():
                policy_loss, value_loss = losses(model, obs, target_pi, target_val)
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
    diag = {
        "mse_vs_outcome": float(np.mean((pred - outcome) ** 2)) if len(rows) else 0.0,
        "baseline_mse": float(np.mean(outcome ** 2)) if len(rows) else 0.0,
        "sign_accuracy": float(np.mean(np.sign(pred[decided]) == np.sign(outcome[decided]))) if decided.any() else 0.0,
    }
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
    corpus.set_value_target_mix(args.value_target_mix)
    train_idx, val_idx = corpus.split_by_game(val_fraction=0.1, seed=args.seed)
    outcomes = corpus.outcomes
    print(f"Loaded {corpus.game_count} games, {len(corpus)} decision steps in {time.time() - started:.1f}s "
          f"(corp wins {outcomes.get(1.0, 0)}, runner wins {outcomes.get(-1.0, 0)}, stalls {outcomes.get(0.0, 0)}).")
    print(f"Split by game: {len(train_idx)} training steps, {len(val_idx)} validation steps.")
    # A cross-entropy against a soft target bottoms out at the target's own
    # entropy, so a validation policy loss only means something relative to
    # this. Printed once so "4.5" can be read as "1.9 nats above the floor".
    policy_floor = corpus.policies.entropy(val_idx)
    print(f"Validation policy-target entropy (loss floor): {policy_floor:.4f} nats")
    print(f"Value target: {1.0 - args.value_target_mix:.2f} x outcome + {args.value_target_mix:.2f} x search root value; "
          f"value loss weight {args.value_loss_weight:g}")

    model = AlphaNetrunnerNet(obs_dim=corpus.observation_size, action_dim=corpus.action_space_size).to(device)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)

    best_val_loss = float("inf")
    best_epoch = 0
    best_diag = None
    history = []
    os.makedirs(args.output_dir, exist_ok=True)
    best_pt_path = os.path.join(args.output_dir, "best_model.pt")

    for epoch in range(1, args.epochs + 1):
        epoch_started = time.time()
        model.train()
        train_p, train_v = run_epoch(model, corpus, train_idx, args.batch_size, device, optimizer,
                                     value_loss_weight=args.value_loss_weight)
        model.eval()
        val_p, val_v = run_epoch(model, corpus, val_idx, args.batch_size, device)
        diag = value_diagnostics(model, corpus, val_idx, args.batch_size, device)
        # Selected on the same weighted sum the gradient used, so the
        # checkpoint that ships is the one the training objective preferred.
        val_loss = val_p + args.value_loss_weight * val_v
        history.append({"epoch": epoch, "train_policy": train_p, "train_value": train_v,
                        "val_policy": val_p, "val_value": val_v, "val_value_diagnostics": diag})
        print(f"Epoch {epoch:02d}/{args.epochs:02d} | "
              f"Train (Policy: {train_p:.4f}, Value: {train_v:.4f}) | "
              f"Val (Policy: {val_p:.4f}, +{val_p - policy_floor:.4f} over floor, Value: {val_v:.4f}) | "
              f"Value vs outcome: MSE {diag['mse_vs_outcome']:.3f} (predict-zero {diag['baseline_mse']:.3f}), "
              f"sign {diag['sign_accuracy']:.1%} | {time.time() - epoch_started:.0f}s")

        if val_loss < best_val_loss:
            best_val_loss = val_loss
            best_epoch = epoch
            best_diag = diag
            torch.save(model.state_dict(), best_pt_path)

    print("Training complete. Exporting best checkpoint to ONNX...")
    onnx_path = os.path.join(args.output_dir, "netrunner_policy.onnx")
    export_model = AlphaNetrunnerNet(obs_dim=corpus.observation_size, action_dim=corpus.action_space_size).to("cpu")
    export_model.load_state_dict(torch.load(best_pt_path, map_location="cpu"))
    export_onnx(export_model, onnx_path, corpus.observation_size)
    print(f"Model successfully exported to ONNX format at '{onnx_path}'")
    # One JSON line for the loop to keep, the way the arena reports.
    summary = {
        "games": corpus.game_count, "steps": len(corpus), "train_steps": int(len(train_idx)),
        "val_steps": int(len(val_idx)), "policy_floor": policy_floor, "best_epoch": best_epoch,
        "best_val_loss": best_val_loss, "value_target_mix": args.value_target_mix,
        "value_loss_weight": args.value_loss_weight, "best_value_diagnostics": best_diag,
        "epochs": history, "seconds": time.time() - started,
    }
    print(json.dumps(summary))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--data-dir", "-d", type=str, required=True)
    parser.add_argument("--output-dir", "-o", type=str, default="./checkpoints")
    parser.add_argument("--epochs", "-e", type=int, default=10)
    parser.add_argument("--batch-size", "-b", type=int, default=64)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--window", type=int, default=None,
                        help="Train on the last N iteration subdirectories only (the replay window); default all")
    parser.add_argument("--limit-games", type=int, default=None,
                        help="Use only the first N games, in file order — for measuring how loss scales with data")
    parser.add_argument("--seed", type=int, default=0, help="Seed for the game split and batch order")
    parser.add_argument("--value-target-mix", type=float, default=0.5,
                        help="Value target = (1 - mix) x final outcome + mix x search root value (see set_value_target_mix)")
    parser.add_argument("--value-loss-weight", type=float, default=0.25,
                        help="Weight of the value loss against the policy loss in the gradient and in checkpoint selection")
    args = parser.parse_args()
    train(args)
