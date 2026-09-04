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

**A run is pinned to one binary, built once and copied into the checkpoint
directory.** This script used to invoke `cargo run` for every stage, which
rebuilds the engine from whatever the working tree holds at that moment. The
second 2,400-game run was overtaken by exactly that: three *Elevation* stages
landed in the tree beside it, iterations 8 and 9 recompiled mid-run (the deck
pool went from 12 matchups to 36 and the card-identity planes reindexed), and
iteration 10 finally compiled a half-finished edit and died with six
iterations to go. A `cargo` invocation also blocks on another session's target
lock. Nothing about a training run should depend on what someone is editing
while it runs (ROADMAP Phase 2 §5).
"""
import argparse
import glob
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time

class StageFailed(Exception):
    """A stage exited non-zero, or printed no summary line.

    Raised rather than exiting where it happened so the loop can record
    *which* iteration and stage stopped it and print the command that
    resumes there. The run that died at iteration 10's arena left
    `iterations.log` with nine lines and nothing at all saying why there
    was no tenth."""

    def __init__(self, stage, returncode):
        super().__init__(f"{stage} failed with return code {returncode}")
        self.stage = stage
        self.returncode = returncode


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
        raise StageFailed(description, res.returncode)
    return res


def last_json_line(stdout: str, what: str):
    lines = [line for line in stdout.splitlines() if line.startswith("{")]
    if not lines:
        print(stdout)
        print(f"FAILED: {what} printed no JSON summary line")
        raise StageFailed(f"{what} summary", 1)
    return json.loads(lines[-1])


def pin_binary(ckpt_dir):
    """Build `netrunner_selfplay` once and copy it into the checkpoint
    directory, returning the copy's path — the only engine this run will
    use.

    The copy is the point. `cargo build` alone still leaves every later
    stage reading `target/release/`, which another session's build
    replaces; and `cargo run` would recompile from the working tree at each
    stage, which is what overtook the second volume run. See the module
    docstring."""
    run_cmd(
        ["cargo", "build", "--release", "-p", "netrunner_selfplay", "--features", "onnx"],
        "Building Rust self-play binary (release)",
    )
    pinned_dir = os.path.join(ckpt_dir, "bin")
    os.makedirs(pinned_dir, exist_ok=True)
    pinned = os.path.abspath(os.path.join(pinned_dir, "netrunner_selfplay"))
    shutil.copyfile(os.path.join("target", "release", "netrunner_selfplay"), pinned)
    shutil.copymode(os.path.join("target", "release", "netrunner_selfplay"), pinned)
    return pinned


def run_identity(binary):
    """What produced this run's data: the commit, whether the tree was
    dirty when the binary was built, and the binary's own hash.

    Recorded on every `iterations.log` line because the commit alone is not
    enough — the second volume run's fatal edit was uncommitted — and the
    binary hash alone does not say where to look."""
    def git(*argv):
        try:
            return subprocess.run(["git", *argv], capture_output=True, text=True, check=True).stdout.strip()
        except (subprocess.CalledProcessError, FileNotFoundError):
            return ""

    with open(binary, "rb") as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    return {"commit": git("rev-parse", "HEAD") or "unknown",
            "dirty": bool(git("status", "--porcelain")),
            "binary_sha256": digest}


def arena(binary, candidate, incumbent, games, simulations, description):
    """The evaluator step: the candidate against the incumbent (or the
    uniform search when there is none yet), both chairs. Returns the
    summary dict `netrunner_selfplay --arena-candidate` prints."""
    cmd = [
        binary,
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

    binary = pin_binary(args.ckpt_dir)
    identity = run_identity(binary)
    print(f"\nPinned engine: {binary}")
    print(f"  commit {identity['commit']}{' (working tree dirty)' if identity['dirty'] else ''}, "
          f"binary sha256 {identity['binary_sha256'][:16]}\n", flush=True)

    for iter_idx in range(args.start_iter, args.iterations + 1):
        try:
            record = {"iter": iter_idx, "games": args.games_per_iter, "simulations": args.simulations,
                      "incumbent": os.path.exists(latest_onnx), "engine": identity}
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
                    binary,
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
                binary, iter_onnx, incumbent, args.arena_games, args.simulations,
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
        except StageFailed as failure:
            # A run that stops mid-iteration says so in its own log, and
            # says where to pick it up. Everything before this iteration is
            # on disk and is not replayed: the self-play stage skips an
            # iteration directory that already holds its games.
            with open(iterations_log, "a", encoding="utf-8") as log:
                log.write(json.dumps({"iter": iter_idx, "failed": failure.stage,
                                      "returncode": failure.returncode, "engine": identity}) + "\n")
            argv = list(sys.argv)
            if "--start-iter" in argv:  # the resumed run's own flag, replaced rather than repeated
                at = argv.index("--start-iter")
                del argv[at:at + 2]
            print(f"\nSTOPPED at iteration {iter_idx}: {failure}")
            print(f"Resume with: {sys.executable} {' '.join(argv)} --start-iter {iter_idx}", flush=True)
            sys.exit(failure.returncode or 1)

    print("\n==================================================")
    print("  AlphaZero Training Pipeline Complete!")
    print("==================================================")

if __name__ == "__main__":
    main()
