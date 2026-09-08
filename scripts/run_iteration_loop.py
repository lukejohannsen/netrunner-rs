#!/usr/bin/env python3
"""AlphaZero-shaped self-play / train / arena loop over `netrunner_selfplay`.

Each iteration plays `--games-per-iter` games with the incumbent network in
the search (the uniform search until one is promoted), trains a fresh
network on the replay window (masked policy objective unless
`--unmasked-policy`), and promotes it only if it beats the incumbent in a
384-game arena, both chairs. Every game of a run has a distinct seed
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


def arena(binary, candidate, incumbent, games, simulations, description, stride=1, uses="both"):
    """The evaluator step: the candidate against the incumbent (or the
    uniform search when there is none yet), both chairs. Returns the
    summary dict `netrunner_selfplay --arena-candidate` prints.

    `uses` is the ablation the *candidate* is seated with, and it tracks
    --model-uses rather than defaulting to the whole network: a run that
    generates its games with priors-only search must be gated on a
    priors-only candidate, or promotion would measure a configuration the
    run never plays. The incumbent is never ablated — it is the bar."""
    cmd = [
        binary,
        "--arena-candidate", candidate, "-n", str(games), "-s", str(simulations),
        "--arena-pair-stride", str(stride), "--candidate-uses", uses,
        # Both sides, not just the candidate. The incumbent this gates
        # against is deployed by self-play under the same ablation, so
        # seating it whole would let a candidate clear the gate on the
        # configuration gap (0.617 against 0.359) rather than on merit.
        "--incumbent-uses", uses,
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
    parser.add_argument("--ckpt-dir", type=str, default="./data/checkpoints", help="Checkpoints directory")
    # 384, not 48. The three volume runs gated on 48-game arenas, and their
    # verdicts swung 0.22-0.48 iteration to iteration with no trend: at 48
    # games one chair is 24 games, and the chair baseline alone is 0.72/0.28,
    # so the noise was the size of any effect a candidate could have. 384
    # is the count every re-measurement in ROADMAP Phase 2 §5 (items 13 and
    # 17) was made at, and it is about a tenth of an iteration's self-play.
    parser.add_argument("--arena-games", type=int, default=384,
                        help="Head-to-head games a candidate plays against the incumbent before promotion, "
                             "half in each chair")
    # Masked by default here, unmasked by default in the trainer: the
    # trainer's default reproduces the recorded runs byte for byte, while a
    # new run has no reason to train the objective that put the Corp's
    # prior mass on `pass priority` and `draw` (Phase 2 §5 items 14-15).
    parser.add_argument("--unmasked-policy", action="store_true",
                        help="Train the policy softmax over every ActionSpace slot rather than the target's "
                             "support (the pre-item-15 objective; masked is the default)")
    parser.add_argument("--promote-threshold", type=float, default=0.55,
                        help="Candidate score (wins + draws/2, over arena games) needed to be promoted")
    parser.add_argument("--value-target-mix", type=float, default=0.5,
                        help="Passed to the trainer: share of the value target taken from the search's root value")
    parser.add_argument("--value-loss-weight", type=float, default=0.25,
                        help="Passed to the trainer: weight of the value loss against the policy loss")
    parser.add_argument("--arena-screen-games", type=int, default=96,
                        help="Games in the cheap screen run before the full arena (0 disables the screen). "
                             "A cost filter only: promotion is still decided by the full arena.")
    parser.add_argument("--arena-screen-threshold", type=float, default=0.45,
                        help="Skip the full arena when the screen scores below this. Sits well under "
                             "--promote-threshold so a real candidate is not screened out.")
    parser.add_argument("--value-target", default="mixed",
                        choices=("outcome", "mixed", "discounted", "discounted_unforeseeable"),
                        help="Which value-head target the trainer builds (see train_alpha_netrunner.py)")
    parser.add_argument("--value-discount", type=float, default=0.99,
                        help="For the discounted value targets")
    parser.add_argument("--select-on", choices=("blended", "value", "policy"), default="blended",
                        help="Which validation loss picks the exported epoch")
    parser.add_argument("--early-stop-patience", type=int, default=0,
                        help="Trainer early stop; 0 keeps the historical every-epoch behaviour")
    parser.add_argument("--model-uses", choices=("both", "priors-only", "value-only"), default="both",
                        help="Which halves of the network self-play seats, and the ablation the arena then "
                             "gates the candidate with. 'priors-only' is the configuration ROADMAP Phase 2 "
                             "§5 item 22 measured at 0.617 against the uniform search, where the whole "
                             "network scored 0.359 and its value alone 0.141.")
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
                      "incumbent": os.path.exists(latest_onnx), "engine": identity,
                      # What generated this iteration's games and what the
                      # arena then gated: an iterations.log line has to say
                      # which configuration a number belongs to, since
                      # priors-only and whole-network runs are otherwise
                      # indistinguishable in it.
                      "model_uses": args.model_uses}
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
                    # --model-uses only after there is a network to seat
                    # halves of: self-play refuses the flag without -m
                    # rather than silently producing a uniform corpus under
                    # a label saying otherwise.
                    selfplay_cmd.extend(["-m", latest_onnx, "--model-uses", args.model_uses])
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
            train_cmd.extend(["--value-target", args.value_target,
                              "--value-discount", str(args.value_discount),
                              "--select-on", args.select_on,
                              "--early-stop-patience", str(args.early_stop_patience),
                              "--value-target-mix", str(args.value_target_mix),
                              "--value-loss-weight", str(args.value_loss_weight)])
            if not args.unmasked_policy:
                train_cmd.append("--masked-policy")
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
            against = f"{'incumbent' if incumbent else 'uniform search'}"

            # A cheap screen before the full verdict. The arena was 5.00 h of
            # the fourth run's 9.95 h, every hour of it a 384-game verdict on
            # an iteration that was never going to promote (Phase 2 §5 item
            # 20). The screen is a *cost filter only* — the number that gates
            # promotion is still the full arena's — and its threshold sits
            # well below the gate so a real candidate is not screened out: at
            # 96 games the score's sd is about 0.051, so a true 0.55 survives
            # a 0.45 cut about 97 times in 100 while a true 0.40 is stopped
            # about five times in six.
            #
            # The stride is not optional at this size. `decks::matchups()` is
            # corp-major, so 48 pairs at stride 1 are the first four Corp
            # decks — a narrower arena, not a smaller one, and the reason the
            # first three runs' 48-game verdicts swung 0.22–0.48.
            screen = None
            if args.arena_screen_games and args.arena_screen_games < args.arena_games:
                # The full arena walks every pairing once; the screen walks
                # the same span in `stride` steps, so it spreads over the
                # whole pool instead of its first corner.
                stride = max(1, args.arena_games // args.arena_screen_games)
                screen = arena(
                    binary, iter_onnx, incumbent, args.arena_screen_games, args.simulations,
                    f"Iteration {iter_idx}/{args.iterations}: Arena screen vs {against} "
                    f"({args.arena_screen_games} games, stride {stride})",
                    stride=stride, uses=args.model_uses,
                )
                record["arena_screen"] = screen

            if screen is not None and screen["candidate_score"] < args.arena_screen_threshold:
                summary = screen
                record["arena_screened_out"] = True
            else:
                summary = arena(
                    binary, iter_onnx, incumbent, args.arena_games, args.simulations,
                    f"Iteration {iter_idx}/{args.iterations}: Arena, candidate vs {against} "
                    f"({args.arena_games} games)",
                    uses=args.model_uses,
                )
                record["arena_screened_out"] = False
            record["arena"] = summary
            record["arena_seconds"] = time.time() - started
            # A screened-out candidate never played the full arena, so it
            # cannot clear the gate — the screen's own score stands in the
            # log as the reason, flagged by `arena_screened_out`.
            promoted = not record["arena_screened_out"] and summary["candidate_score"] >= args.promote_threshold
            record["promoted"] = promoted
            # Both chairs on the line, not only the blend. The blend hid the
            # whole story for three runs: a network broken as the Corp and
            # neutral-to-good as the Runner averaged to "a bit below the
            # search" (Phase 2 §5 item 13), and nobody reading
            # `promotions.log` could have told.
            chair = lambda c: f"{c['wins']}-{c['losses']}-{c['draws']} ({c['score']:.3f})"
            verdict = (
                f"iter={iter_idx} games={summary['games']} wins={summary['candidate_wins']} "
                f"losses={summary['incumbent_wins']} draws={summary['draws']} "
                f"score={summary['candidate_score']:.3f} "
                f"corp={chair(summary['as_corp'])} runner={chair(summary['as_runner'])} "
                f"threshold={args.promote_threshold} promoted={promoted}"
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
