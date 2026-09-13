#!/usr/bin/env python3
"""Read a `bench --report` over the difficulty ladder and say whether it is one.

A ladder is not a set of bots, it is an *ordered* set, and the order is the
only thing a player relies on: "level 4 is harder than level 3" has to be
true on the chair they are about to sit in, or raising the level teaches
them nothing. Strength is per chair here for the reason
`netrunner_bots::difficulty` gives -- the two chairs convert search budget
differently, so a rung that climbs cleanly as the Corp can be flat as the
Runner.

This prints three things from one `bench --report`:

* **the per-role Glicko ladder**, which is what a player's own rating on
  `Track::HumanVsBot` is comparable against;
* **each rung against one fixed reference rung** on the other chair --
  the "neutral opponent" measurement every chair figure in the roadmap
  uses, and the honest way to say how hard a rung is;
* **a monotonicity verdict** per chair: every rung must beat the one below
  it against that fixed reference, by more than the binomial noise of the
  sample. A violation names the pair, because the fix is a spec change,
  not a rounding of the table.

Usage:
    scripts/ladder_report.py target/diag/ladder-s1.json [--reference level:apprentice]
"""
import argparse
import json
import math

ORDER = ["level:novice", "level:apprentice", "level:operator", "level:veteran", "level:elite"]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("report")
    parser.add_argument("--reference", default="level:apprentice", help="the fixed opponent each rung is measured against")
    args = parser.parse_args()
    report = json.load(open(args.report))

    print(f"{'rung':<18} {'Glicko corp':>14}  {'Glicko runner':>14}")
    for row in sorted(report["ladder"], key=lambda r: ORDER.index(r["participant"]) if r["participant"] in ORDER else 99):
        corp, runner = row["standing"]["corp"]["rating"], row["standing"]["runner"]["rating"]
        print(
            f"{row['participant']:<18} {corp['rating']:>8.0f} ± {2 * corp['deviation']:<3.0f}"
            f" {runner['rating']:>8.0f} ± {2 * runner['deviation']:<3.0f}"
        )

    pairings = {(p["corp"], p["runner"]): p for p in report["pairings"]}
    for chair in ("corp", "runner"):
        print(f"\n{chair} chair, each rung against a fixed {args.reference}:")
        rungs, scores = [], []
        for rung in ORDER:
            key = (rung, args.reference) if chair == "corp" else (args.reference, rung)
            pairing = pairings.get(key)
            if pairing is None:
                continue
            games = pairing["corp_wins"] + pairing["runner_wins"]
            wins = pairing["corp_wins"] if chair == "corp" else pairing["runner_wins"]
            rungs.append(rung)
            scores.append((wins / games, games))
            mark = "  <- the reference itself" if rung == args.reference else ""
            print(f"  {rung:<18} {wins:>3}/{games:<4} {wins / games:>6.3f}{mark}")

        # A ladder has to *climb*, and the first version of this check
        # only caught a rung that fell significantly — which passed a Corp
        # chair measured at 0.562 / 0.500 / 0.458, three rungs a player
        # cannot tell apart. So each step is now classified, and only a
        # rise counts:
        #
        #   rise   - higher by more than the sd of the difference
        #   flat   - inside the noise: the two rungs are the same opponent
        #   INVERT - lower by more than the sd
        #
        # The bar is the sd of the difference of two independent
        # binomials, because both cells are measured rather than assumed.
        # A flat step is a failure, not a pass: it is a level selector
        # that does nothing.
        steps = []
        for (lower, (low, n_low)), (upper, (high, n_high)) in zip(zip(rungs, scores), zip(rungs[1:], scores[1:])):
            sd = math.sqrt(low * (1 - low) / n_low + high * (1 - high) / n_high)
            delta = high - low
            verdict = "rise" if delta > sd else ("INVERT" if delta < -sd else "flat")
            steps.append((verdict, f"{lower} -> {upper}: {low:.3f} -> {high:.3f} ({delta:+.3f}, sd {sd:.3f})"))
        climbs = all(verdict == "rise" for verdict, _ in steps)
        print(f"  climbs: {'yes' if climbs else 'NO'}")
        for verdict, line in steps:
            print(f"    {verdict:<6} {line}")


if __name__ == "__main__":
    main()
