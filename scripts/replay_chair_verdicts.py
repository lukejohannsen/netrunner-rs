#!/usr/bin/env python3
"""Replay a recorded run's promotion verdicts against a measured chair null.

`run_iteration_loop.py` used to hold each chair to an absolute floor
(`--promote-chair-floor 0.45`, `--arena-screen-chair-floor 0.30`)
differenced against a chair parity of 0.500. A chair's parity is the pool's
Corp win rate at that configuration -- 0.6823/0.3177 for the fifth volume
run, 0.7135/0.2865 for the sixth -- so those floors asked the Runner chair
for about +0.13 over its own null and the screen floor sat *above* it
(ROADMAP Phase 2 §5 item 32). The floors are now deltas from a null leg.

This replays the `iterations.log` of a run recorded under the old rule and
prints, per iteration, what each rule decides and why. It changes nothing:
it is the evidence that re-centring the floors moves the verdicts it should
and leaves the rest alone.

**A screened-out iteration cannot be replayed into a promotion.** The
recorded summary is the screen's own 96 games; a candidate the new screen
lets through would then play a 384-game arena that was never played, so the
honest replay says "full arena (never run)" rather than inventing a result.

The `logged` column is what the run itself wrote, which is not always the
old rule: the fifth run predates the chair floor entirely (item 28 landed
after it), so its rows were rejected on the blend alone and `old` there is
what today's code *would* decide, not what happened.

    scripts/replay_chair_verdicts.py \\
        --log data/runs/chair_balanced_v6/iterations.log \\
        --null data/runs/chair_by_head_v8/null_v6pinned.json \\
        --screen-null data/runs/chair_by_head_v8/screen_nulls/screen_null_v6pinned.json
"""
import argparse
import json
import math


def load_null(path):
    """A null leg's chair scores, and how many games each was measured over.

    The game counts are not decoration: a chair sits against a *measured*
    baseline, so the sd of the comparison carries the null's own sampling
    error too. At 192 games a side that is sqrt(2) x the chair's own sd,
    and reporting the chair's sd alone overstates every sigma by 40%.
    """
    with open(path, encoding="utf-8") as handle:
        summary = json.load(handle)
    if abs(summary["candidate_score"] - 0.5) > 1e-9:
        raise SystemExit(f"{path} is not a null leg: it scores {summary['candidate_score']}")
    return {name: (summary[key]["score"], summary[key]["games"])
            for key, name in (("as_corp", "corp"), ("as_runner", "runner"))}


def chairs(summary):
    return {"corp": summary["as_corp"], "runner": summary["as_runner"]}


def old_collapse(summary, floor):
    """The old rule: one absolute floor, both chairs, parity assumed 0.5."""
    failed = [(row["score"], name) for name, row in chairs(summary).items()
              if row["games"] and row["score"] < floor]
    return min(failed)[1] if failed else None


def new_collapse(summary, null, margin):
    """The new rule: each chair against its own null, less `margin`."""
    failed = [(row["score"] - (null[name][0] - margin), name) for name, row in chairs(summary).items()
              if row["games"] and row["score"] < null[name][0] - margin]
    return min(failed)[1] if failed else None


def sigma(score, baseline, games):
    """How far a chair sits from its measured null, in sd of that
    difference -- both terms' sampling error, not just the chair's."""
    null_score, null_games = baseline
    if not games:
        return 0.0
    variance = max(null_score * (1 - null_score), 1e-9) * (1 / games + 1 / max(null_games, 1))
    return (score - null_score) / math.sqrt(variance)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--log", required=True, help="a run's iterations.log")
    parser.add_argument("--null", required=True, help="the full-arena null leg's JSON summary")
    parser.add_argument("--screen-null", help="the screen-shaped null leg's JSON summary (defaults to --null)")
    parser.add_argument("--threshold", type=float, default=0.55)
    parser.add_argument("--screen-threshold", type=float, default=0.45)
    parser.add_argument("--old-promote-floor", type=float, default=0.45)
    parser.add_argument("--old-screen-floor", type=float, default=0.30)
    parser.add_argument("--promote-margin", type=float, default=0.07)
    parser.add_argument("--screen-margin", type=float, default=0.20)
    args = parser.parse_args()

    full_null = load_null(args.null)
    screen_null = load_null(args.screen_null) if args.screen_null else full_null

    print(f"{args.log}")
    print(f"  full-arena null   corp {full_null['corp'][0]:.4f}  runner {full_null['runner'][0]:.4f}  "
          f"({full_null['corp'][1]} games a chair)")
    print(f"  screen null       corp {screen_null['corp'][0]:.4f}  runner {screen_null['runner'][0]:.4f}  "
          f"({screen_null['corp'][1]} games a chair)")
    print(f"  old floors        {args.old_promote_floor} promote, {args.old_screen_floor} screen (against 0.500)")
    print(f"  new floors        promote corp {full_null['corp'][0] - args.promote_margin:.4f} / "
          f"runner {full_null['runner'][0] - args.promote_margin:.4f}; "
          f"screen corp {screen_null['corp'][0] - args.screen_margin:.4f} / "
          f"runner {screen_null['runner'][0] - args.screen_margin:.4f}")
    print()
    header = (f"{'iter':>4} {'games':>5} {'blend':>6} {'corp':>6} {'runner':>6} {'r vs null':>9}  "
              f"{'logged':<22} {'old rule':<22} {'new rule':<26} changed")
    print(header)
    print("-" * len(header))

    changed = 0
    for line in open(args.log, encoding="utf-8"):
        record = json.loads(line)
        summary = record.get("arena")
        if not summary or not summary.get("as_corp", {}).get("games"):
            continue
        screened = bool(record.get("arena_screened_out"))
        null = screen_null if screened else full_null
        floor = args.old_screen_floor if screened else args.old_promote_floor
        cut = args.screen_threshold if screened else args.threshold
        margin = args.screen_margin if screened else args.promote_margin
        runner = summary["as_runner"]

        old = old_collapse(summary, floor)
        new = new_collapse(summary, null, margin)
        blend_ok = summary["candidate_score"] >= cut

        if screened:
            old_reason = f"screened out: {old}" if old else "screened out: blend"
            if new:
                new_reason = f"screened out: {new}"
            elif blend_ok:
                new_reason = "full arena (never run)"
            else:
                new_reason = "screened out: blend"
        else:
            old_reason = f"rejected: {old}" if old else ("promoted" if blend_ok else "rejected: blend")
            new_reason = f"rejected: {new}" if new else ("promoted" if blend_ok else "rejected: blend")

        # What the run actually wrote, which for a run predating the chair
        # floor is the blend alone.
        if "chair_floor_failure" not in record:
            logged = "(no chair floor)"
        elif record.get("promoted"):
            logged = "promoted"
        elif record["chair_floor_failure"]:
            logged = f"{'screened out' if screened else 'rejected'}: {record['chair_floor_failure']}"
        else:
            logged = f"{'screened out' if screened else 'rejected'}: blend"

        differs = old_reason != new_reason
        changed += differs
        print(f"{record['iter']:>4} {summary['games']:>5} {summary['candidate_score']:>6.3f} "
              f"{summary['as_corp']['score']:>6.3f} {runner['score']:>6.3f} "
              f"{sigma(runner['score'], null['runner'], runner['games']):>+8.1f}σ  "
              f"{logged:<22} {old_reason:<22} {new_reason:<26} {'yes' if differs else ''}")

    print(f"\n{changed} verdict reason(s) change.")


if __name__ == "__main__":
    main()
