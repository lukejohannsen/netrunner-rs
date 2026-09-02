#!/usr/bin/env python3
"""AlphaZero-shaped self-play / train / arena loop over `netrunner_selfplay`.

Each iteration plays `--games-per-iter` games with the incumbent network in
the search (the uniform search until one is promoted), trains a fresh
network on the replay window, and promotes it only if it beats the
incumbent in the arena. Every game of a run has a distinct seed
(`--seed-offset`), the iteration is resumable (an iteration directory that
already holds its games is not replayed), and one JSON line per iteration
goes to `<ckpt-dir>/iterations.log` with the timings and both summaries, so
an unattended run can be read back afterwards.
"""
import argparse
import glob
import json
import os
import shutil
import subprocess
import sys
import time

def run_cmd(cmd, description, capture=False):
    print(f"\n==================================================")
    print(f"  {description}")
    print(f"==================================================")
    print(f"Running: {' '.join(cmd)}\n", flush=True)
    res = subprocess.run(cmd, capture_output=capture, text=capture)
    if res.returncode != 0:
        if capture:
            print(res.stdout)
            print(res.stderr)
        print(f"FAILED: {description} failed with return code {res.returncode}")
        sys.exit(res.returncode)
    return res


def last_json_line(stdout: str, what: str):
    lines = [line for line in stdout.splitlines() if line.startswith("{")]
    if not lines:
        print(stdout)
        print(f"FAILED: {what} printed no JSON summary line")
        sys.exit(1)
    return json.loads(lines[-1])


def arena(candidate, incumbent, games, simulations, description):
    """The evaluator step: the candidate against the incumbent (or the
    uniform search when there is none yet), both chairs. Returns the
    summary dict `netrunner_selfplay --arena-candidate` prints."""
    cmd = [
        "cargo", "run", "--release", "-p", "netrunner_selfplay", "--features", "onnx", "--",
        "--arena-candidate", candidate, "-n", str(games), "-s", str(simulations),
    ]
    if incumbent is not None:
        cmd.extend(["--arena-incumbent", incumbent])
    res = run_cmd(cmd, description, capture=True)
    return last_json_line(res.stdout, "arena")

def main():
    parser = argparse.ArgumentParser(description="AlphaZero Continuous Self-Play & Training Loop")
    parser.add_argument("--iterations", "-i", type=int, default=100, help="Number of self-play/train iterations")
    parser.add_argument("--start-iter", type=int, default=1,
                        help="First iteration to run (resume a run whose earlier iterations are on disk)")
    parser.add_argument("--games-per-iter", "-g", type=int, default=100, help="Games per iteration")
    parser.add_argument("--simulations", "-s", type=int, default=200, help="MCTS simulations per step")
    parser.add_argument("--epochs", "-e", type=int, default=10, help="PyTorch training epochs per iter")
    parser.add_argument("--window", type=int, default=None,
                        help="Train on the last N iterations only (the replay window); default every iteration")
    parser.add_argument("--data-dir", type=str, default="./data/selfplay", help="Trajectory output directory")
    parser.add_argument("--ckpt-dir", type=str, default="./checkpoints", help="Checkpoints directory")
    parser.add_argument("--arena-games", type=int, default=48,
                        help="Head-to-head games a candidate plays against the incumbent before promotion")
    parser.add_argument("--promote-threshold", type=float, default=0.55,
                        help="Candidate score (wins + draws/2, over arena games) needed to be promoted")
    parser.add_argument("--value-target-mix", type=float, default=0.5,
                        help="Passed to the trainer: share of the value target taken from the search's root value")
    parser.add_argument("--value-loss-weight", type=float, default=0.25,
                        help="Passed to the trainer: weight of the value loss against the policy loss")
    parser.add_argument("--skip-arena", action="store_true",
                        help="Promote every checkpoint unconditionally (the pre-gating behaviour)")
    args = parser.parse_args()

    os.makedirs(args.data_dir, exist_ok=True)
    os.makedirs(args.ckpt_dir, exist_ok=True)

    latest_onnx = os.path.join(args.ckpt_dir, "latest_policy.onnx")
    iterations_log = os.path.join(args.ckpt_dir, "iterations.log")

    # Ensure cargo release binaries are built before starting loop
    run_cmd(
        ["cargo", "build", "--release", "-p", "netrunner_selfplay", "--features", "onnx"],
        "Building Rust self-play binary (release)"
    )

    for iter_idx in range(args.start_iter, args.iterations + 1):
        record = {"iter": iter_idx, "games": args.games_per_iter, "simulations": args.simulations,
                  "incumbent": os.path.exists(latest_onnx)}
        iter_data_dir = os.path.join(args.data_dir, f"iter_{iter_idx:03d}")
        os.makedirs(iter_data_dir, exist_ok=True)

        # 1. Self-play with the incumbent network in the search, if there is
        # one. Seeds are `(iter − 1) × games` onward: self-play is
        # bit-reproducible, so without the offset every un-promoted
        # iteration would replay the previous one's games exactly.
        started = time.time()
        if len(glob.glob(os.path.join(iter_data_dir, "game_*.jsonl"))) >= args.games_per_iter:
            print(f"\nIteration {iter_idx}: '{iter_data_dir}' already holds its games, self-play skipped.")
        else:
            selfplay_cmd = [
                "cargo", "run", "--release", "-p", "netrunner_selfplay", "--features", "onnx", "--",
                "-n", str(args.games_per_iter),
                "-s", str(args.simulations),
                "-o", iter_data_dir,
                "--seed-offset", str((iter_idx - 1) * args.games_per_iter),
            ]
            if os.path.exists(latest_onnx):
                selfplay_cmd.extend(["-m", latest_onnx])
            run_cmd(
                selfplay_cmd,
                f"Iteration {iter_idx}/{args.iterations}: MCTS Self-Play ({args.games_per_iter} games)"
            )
        record["selfplay_seconds"] = time.time() - started

        # 2. Train a fresh network on the replay window.
        started = time.time()
        train_cmd = [
            sys.executable, "scripts/train_alpha_netrunner.py",
            "-d", args.data_dir,
            "-o", args.ckpt_dir,
            "-e", str(args.epochs),
        ]
        if args.window is not None:
            train_cmd.extend(["--window", str(args.window)])
        train_cmd.extend(["--value-target-mix", str(args.value_target_mix),
                          "--value-loss-weight", str(args.value_loss_weight)])
        res = run_cmd(
            train_cmd,
            f"Iteration {iter_idx}/{args.iterations}: Training Neural Network",
            capture=True,
        )
        print(res.stdout)
        record["train"] = last_json_line(res.stdout, "training")
        record["train_seconds"] = time.time() - started

        # 3. Gate, then promote. A checkpoint that cannot beat the model
        # that generated its data is not an improvement, whatever its loss
        # says; promoting one unconditionally is how a single Runner-biased
        # network turned six iterations of self-play into "the Runner
        # wins" (ROADMAP Phase 2 §5).
        iter_onnx = os.path.join(args.ckpt_dir, "netrunner_policy.onnx")
        if not os.path.exists(iter_onnx):
            continue
        if args.skip_arena:
            shutil.copyfile(iter_onnx, latest_onnx)
            print(f"\n[+] Updated '{latest_onnx}' with newly trained weights (arena skipped).")
            record["promoted"] = True
            with open(iterations_log, "a", encoding="utf-8") as log:
                log.write(json.dumps(record) + "\n")
            continue

        started = time.time()
        incumbent = latest_onnx if os.path.exists(latest_onnx) else None
        summary = arena(
            iter_onnx, incumbent, args.arena_games, args.simulations,
            f"Iteration {iter_idx}/{args.iterations}: Arena, candidate vs "
            f"{'incumbent' if incumbent else 'uniform search'} ({args.arena_games} games)",
        )
        record["arena"] = summary
        record["arena_seconds"] = time.time() - started
        promoted = summary["candidate_score"] >= args.promote_threshold
        record["promoted"] = promoted
        verdict = (
            f"iter={iter_idx} games={summary['games']} wins={summary['candidate_wins']} "
            f"losses={summary['incumbent_wins']} draws={summary['draws']} "
            f"score={summary['candidate_score']:.3f} threshold={args.promote_threshold} promoted={promoted}"
        )
        with open(os.path.join(args.ckpt_dir, "promotions.log"), "a", encoding="utf-8") as log:
            log.write(verdict + "\n")
        with open(iterations_log, "a", encoding="utf-8") as log:
            log.write(json.dumps(record) + "\n")
        if promoted:
            shutil.copyfile(iter_onnx, latest_onnx)
            print(f"\n[+] PROMOTED: {verdict}", flush=True)
        else:
            rejected = os.path.join(args.ckpt_dir, f"rejected_iter_{iter_idx:03d}.onnx")
            shutil.copyfile(iter_onnx, rejected)
            print(f"\n[-] REJECTED (kept '{rejected}', incumbent stays): {verdict}", flush=True)

    print("\n==================================================")
    print("  AlphaZero Training Pipeline Complete!")
    print("==================================================")

if __name__ == "__main__":
    main()
