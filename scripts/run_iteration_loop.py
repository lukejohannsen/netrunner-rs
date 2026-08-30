#!/usr/bin/env python3
import argparse
import os
import shutil
import subprocess
import sys

def run_cmd(cmd, description):
    print(f"\n==================================================")
    print(f"  {description}")
    print(f"==================================================")
    print(f"Running: {' '.join(cmd)}\n")
    res = subprocess.run(cmd)
    if res.returncode != 0:
        print(f"FAILED: {description} failed with return code {res.returncode}")
        sys.exit(res.returncode)

def main():
    parser = argparse.ArgumentParser(description="AlphaZero Continuous Self-Play & Training Loop")
    parser.add_argument("--iterations", "-i", type=int, default=100, help="Number of self-play/train iterations")
    parser.add_argument("--games-per-iter", "-g", type=int, default=100, help="Games per iteration")
    parser.add_argument("--simulations", "-s", type=int, default=200, help="MCTS simulations per step")
    parser.add_argument("--epochs", "-e", type=int, default=10, help="PyTorch training epochs per iter")
    parser.add_argument("--data-dir", type=str, default="./data/selfplay", help="Trajectory output directory")
    parser.add_argument("--ckpt-dir", type=str, default="./checkpoints", help="Checkpoints directory")
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

        # 3. Promote trained ONNX checkpoint to active latest model
        iter_onnx = os.path.join(args.ckpt_dir, "netrunner_policy.onnx")
        if os.path.exists(iter_onnx):
            shutil.copyfile(iter_onnx, latest_onnx)
            print(f"\n[+] Updated '{latest_onnx}' with newly trained weights.")

    print("\n==================================================")
    print("  AlphaZero Training Pipeline Complete!")
    print("==================================================")

if __name__ == "__main__":
    main()
