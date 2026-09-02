#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import subprocess
import sys

def run_cmd(cmd, description, capture=False):
    print(f"\n==================================================")
    print(f"  {description}")
    print(f"==================================================")
    print(f"Running: {' '.join(cmd)}\n", flush=True)
    res = subprocess.run(cmd, capture_output=capture, text=capture)
    if res.returncode != 0:
        if capture:
            print(res.stderr)
        print(f"FAILED: {description} failed with return code {res.returncode}")
        sys.exit(res.returncode)
    return res


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
    lines = [line for line in res.stdout.splitlines() if line.startswith("{")]
    if not lines:
        print(res.stdout)
        print("FAILED: arena printed no JSON summary line")
        sys.exit(1)
    return json.loads(lines[-1])

def main():
    parser = argparse.ArgumentParser(description="AlphaZero Continuous Self-Play & Training Loop")
    parser.add_argument("--iterations", "-i", type=int, default=100, help="Number of self-play/train iterations")
    parser.add_argument("--games-per-iter", "-g", type=int, default=100, help="Games per iteration")
    parser.add_argument("--simulations", "-s", type=int, default=200, help="MCTS simulations per step")
    parser.add_argument("--epochs", "-e", type=int, default=10, help="PyTorch training epochs per iter")
    parser.add_argument("--data-dir", type=str, default="./data/selfplay", help="Trajectory output directory")
    parser.add_argument("--ckpt-dir", type=str, default="./checkpoints", help="Checkpoints directory")
    parser.add_argument("--arena-games", type=int, default=48,
                        help="Head-to-head games a candidate plays against the incumbent before promotion")
    parser.add_argument("--promote-threshold", type=float, default=0.55,
                        help="Candidate score (wins + draws/2, over arena games) needed to be promoted")
    parser.add_argument("--skip-arena", action="store_true",
                        help="Promote every checkpoint unconditionally (the pre-gating behaviour)")
    args = parser.parse_args()

    os.makedirs(args.data_dir, exist_ok=True)
    os.makedirs(args.ckpt_dir, exist_ok=True)

    latest_onnx = os.path.join(args.ckpt_dir, "latest_policy.onnx")

    # Ensure cargo release binaries are built before starting loop
    run_cmd(
        ["cargo", "build", "--release", "-p", "netrunner_selfplay", "--features", "onnx"],
        "Building Rust self-play binary (release)"
    )

    for iter_idx in range(1, args.iterations + 1):
        iter_data_dir = os.path.join(args.data_dir, f"iter_{iter_idx:03d}")
        os.makedirs(iter_data_dir, exist_ok=True)

        # 1. Run MCTS Self-Play (using active ONNX model if available)
        selfplay_cmd = [
            "cargo", "run", "--release", "-p", "netrunner_selfplay", "--features", "onnx", "--",
            "-n", str(args.games_per_iter),
            "-s", str(args.simulations),
            "-o", iter_data_dir,
        ]
        if os.path.exists(latest_onnx):
            selfplay_cmd.extend(["-m", latest_onnx])
            
        run_cmd(
            selfplay_cmd,
            f"Iteration {iter_idx}/{args.iterations}: MCTS Self-Play ({args.games_per_iter} games)"
        )

        # 2. Train Policy/Value Network on cumulative trajectories
        train_cmd = [
            sys.executable, "scripts/train_alpha_netrunner.py",
            "-d", args.data_dir,
            "-o", args.ckpt_dir,
            "-e", str(args.epochs),
        ]
        run_cmd(
            train_cmd,
            f"Iteration {iter_idx}/{args.iterations}: Training Neural Network"
        )

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
            continue

        incumbent = latest_onnx if os.path.exists(latest_onnx) else None
        summary = arena(
            iter_onnx, incumbent, args.arena_games, args.simulations,
            f"Iteration {iter_idx}/{args.iterations}: Arena, candidate vs "
            f"{'incumbent' if incumbent else 'uniform search'} ({args.arena_games} games)",
        )
        promoted = summary["candidate_score"] >= args.promote_threshold
        verdict = (
            f"iter={iter_idx} games={summary['games']} wins={summary['candidate_wins']} "
            f"losses={summary['incumbent_wins']} draws={summary['draws']} "
            f"score={summary['candidate_score']:.3f} threshold={args.promote_threshold} promoted={promoted}"
        )
        with open(os.path.join(args.ckpt_dir, "promotions.log"), "a", encoding="utf-8") as log:
            log.write(verdict + "\n")
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
