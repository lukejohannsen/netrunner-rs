#!/usr/bin/env python3
"""What the sample-deck pool's Corp win share does as play sharpens.

ROADMAP Phase 2 §5 item 31 left one thing standing: seating a policy head
moves the pool about ten points toward the Corp, for both players equally
and with no per-seat defect in the head (item 32). That is a claim about
*the pool under sharper play*, so it is measurable where game balance
already is -- `netrunner_cli bench`, whose self-pairing plays one bot in
both chairs over the 192 sample matchups and is therefore the same "chair
null" `netrunner_selfplay --arena-null` measures, at a tenth the cost.

This reads `bench --report` JSON and prints three things:

* **the curve** -- every self-paired cell in the given reports, pooled by
  participant id across reports, with the outcome split. The split is
  load-bearing: at random-vs-random *every* Corp win is a flatline, so
  0.477 there is not a balance reading at all.
* **the per-deck decomposition** -- whether a move is the whole pool or
  two broken decks.
* **one chair at a time** (`--against`) -- each participant's Corp-chair
  and Runner-chair share against one fixed opponent, which is what
  separates "the pool favours the Corp" from "this bot converts its
  budget on one chair only".

Cells from different reports are independent samples of the same
(matchup, seed) space and pool cleanly, but only cells at the same
*pairing offset* play the same games: `bench` seeds game n of the whole
run on `seed + n`, so a pairing's schedule depends on how many pairings
precede it. Compare a column taken from one report, not across reports.

Usage:
    scripts/corp_share_curve.py curve  reports/*.json
    scripts/corp_share_curve.py chairs --against heuristic reports/cross512.json
"""
import argparse
import json
import math
from collections import OrderedDict


def load(paths):
    for path in paths:
        with open(path) as handle:
            yield path, json.load(handle)


def share(games):
    """Corp win share over decided games, with its sampling sd.

    Stalls are excluded rather than counted as half: `netrunner_rating`
    has no "nobody won" outcome either, and a stall is a harness fact.
    """
    corp = sum(1 for g in games if g["winner"] == "Corp")
    runner = sum(1 for g in games if g["winner"] == "Runner")
    n = corp + runner
    if not n:
        return float("nan"), float("nan"), 0
    p = corp / n
    return p, math.sqrt(p * (1 - p) / n), n


def z_of_difference(a, b):
    """Both terms carry sampling error, so the sd is of the difference."""
    return (a[0] - b[0]) / math.sqrt(a[1] ** 2 + b[1] ** 2)


def self_paired(reports):
    """Every self-paired cell, pooled by participant id across reports."""
    legs = OrderedDict()
    for _path, report in reports:
        for game in report["games"]:
            if game["corp"] == game["runner"]:
                legs.setdefault(game["corp"], []).append(game)
    return legs


def curve(reports, baseline):
    legs = self_paired(reports)
    print(f"{'participant':<14} {'n':>5} {'corp share':>16} {'corp flatline':>14} "
          f"{'corp agenda':>12} {'runner agenda':>14} {'deckout':>8} {'steps':>6}")
    for name, games in legs.items():
        p, sd, n = share(games)
        total = len(games)

        def frac(winner, reason):
            return sum(1 for g in games if g["winner"] == winner and g["reason"] == reason) / total

        print(f"{name:<14} {n:>5} {p:>9.4f} ±{sd:.4f} {frac('Corp', 'Flatline'):>14.3f} "
              f"{frac('Corp', 'AgendaThreshold'):>12.3f} {frac('Runner', 'AgendaThreshold'):>14.3f} "
              f"{frac('Runner', 'Deckout'):>8.3f} {sum(g['steps'] for g in games) / total:>6.0f}")

    if baseline in legs:
        base = share(legs[baseline])
        print(f"\nagainst {baseline}:")
        for name, games in legs.items():
            if name == baseline:
                continue
            cell = share(games)
            print(f"  {name:<14} {cell[0] - base[0]:>+8.4f}  ({z_of_difference(cell, base):+.2f} sigma)")
    return legs


def by_deck(legs, first, last):
    """Per-deck share at two tiers -- is a move the pool or two decks?"""
    if first not in legs or last not in legs:
        return
    for which, index in (("corp deck", 0), ("runner deck", 1)):
        decks = sorted({g["matchup"].split("_vs_")[index] for g in legs[first]})
        print(f"\n{which:<28}{first:>14}{last:>14}{'delta':>10}")
        moved = 0
        for deck in decks:
            def cell(name):
                return share([g for g in legs[name] if g["matchup"].split("_vs_")[index] == deck])[0]

            a, b = cell(first), cell(last)
            moved += b > a
            print(f"{deck:<28}{a:>14.3f}{b:>14.3f}{b - a:>+10.3f}")
        print(f"{'toward the Corp':<28}{moved:>14}/{len(decks)}")


def fixed_opponent(report, against):
    """`against`, resolved to the id it actually rates under in `report`.

    `--label` suffixes *every* participant in a run (`heuristic#t4`), and
    it has to, because that is how two settings of the same bot stay
    apart when `curve` pools self-paired cells across reports. But the
    fixed opponent is named by kind, once, for a whole ladder of
    differently-labelled runs -- so match it before the suffix too rather
    than making the caller retype the label per file.
    """
    if against in report["bots"]:
        return against
    matches = [name for name in report["bots"] if name.split("#")[0] == against]
    return matches[0] if len(matches) == 1 else against


def chairs(reports, against):
    """Each participant's two chairs against one fixed opponent."""
    print(f"{'report':<22} {'participant':<12} {'as Corp':>20} {'as Runner':>20}")
    for path, report in reports:
        # Resolved per report, never rebinding `against` itself: each file
        # in a ladder carries its own label.
        fixed = fixed_opponent(report, against)
        for name in report["bots"]:
            if name == fixed:
                continue
            corp = share([g for g in report["games"] if g["corp"] == name and g["runner"] == fixed])
            runner = share([g for g in report["games"] if g["corp"] == fixed and g["runner"] == name])
            if not corp[2] or not runner[2]:
                continue
            print(f"{path.split('/')[-1]:<22} {name:<12} {corp[0]:>10.3f} ±{corp[1]:.3f}"
                  f"{1 - runner[0]:>13.3f} ±{runner[1]:.3f}")
        control = share([g for g in report["games"] if g["corp"] == fixed and g["runner"] == fixed])
        if control[2]:
            print(f"{path.split('/')[-1]:<22} {'(control) ' + fixed:<12} {control[0]:>10.3f} ±{control[1]:.3f}")


def paired(reports, against, chair):
    """Two same-shape reports' chair cells, compared game by game.

    Cells at the same pairing offset in same-shape runs play the *same*
    (matchup, seed) games -- the offset rule in this module's docstring,
    used rather than merely respected. That makes the comparison paired,
    and a paired comparison is much the sharper one here: the games
    differ enormously in how winnable a chair is, and that variance is
    common to both cells and cancels. Only the games the two cells
    disagree on carry information, which is McNemar's test.

    `chair` is which chair the *varying* participant sits in; the fixed
    opponent takes the other.
    """
    def cell(report):
        """The report's one non-fixed participant, and its chair cell
        keyed by the game identity two reports share."""
        fixed = fixed_opponent(report, against)
        name = next(b for b in report["bots"] if b != fixed)
        wanted = (name, fixed) if chair == "corp" else (fixed, name)
        return name, {
            (g["matchup"], g["seed"]): g for g in report["games"] if (g["corp"], g["runner"]) == wanted
        }

    name_a, games_a = cell(reports[0][1])
    name_b, games_b = cell(reports[1][1])
    shared = sorted(set(games_a) & set(games_b))
    if not shared:
        print("no shared games: the two reports are not the same shape")
        return

    def won(game):
        """Did the varying participant win this game?"""
        return game["winner"] == ("Corp" if chair == "corp" else "Runner")

    a_only = sum(1 for k in shared if won(games_a[k]) and not won(games_b[k]))
    b_only = sum(1 for k in shared if won(games_b[k]) and not won(games_a[k]))
    both = sum(1 for k in shared if won(games_a[k]) and won(games_b[k]))
    neither = len(shared) - a_only - b_only - both
    rate_a = (a_only + both) / len(shared)
    rate_b = (b_only + both) / len(shared)
    discordant = a_only + b_only
    # McNemar without the normal approximation's continuity fudge: the
    # discordant pairs are Binomial(n, 1/2) under the null, and n here is
    # in the hundreds, so the z is honest.
    z = (a_only - b_only) / math.sqrt(discordant) if discordant else float("nan")
    unpaired_sd = math.sqrt(rate_a * (1 - rate_a) / len(shared) + rate_b * (1 - rate_b) / len(shared))
    print(f"paired on {len(shared)} shared games, {chair} chair against {against}")
    print(f"  {name_a:<16} {rate_a:.4f}")
    print(f"  {name_b:<16} {rate_b:.4f}")
    print(f"  delta            {rate_a - rate_b:+.4f}")
    print(f"  discordant       {a_only} / {b_only}  (agreed: {both} both won, {neither} both lost)")
    print(f"  paired z         {z:+.2f}   (unpaired would be {(rate_a - rate_b) / unpaired_sd:+.2f})")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("mode", choices=["curve", "chairs", "paired"])
    parser.add_argument("reports", nargs="+")
    parser.add_argument("--baseline", default="puct@128", help="participant the curve differences against")
    parser.add_argument("--decks", nargs=2, metavar=("FIRST", "LAST"), help="two participants to decompose by deck")
    parser.add_argument("--against", default="heuristic", help="the fixed opponent on the other chair")
    parser.add_argument("--chair", default="runner", choices=["corp", "runner"],
                        help="paired mode: which chair the varying participant sits in")
    args = parser.parse_args()

    reports = list(load(args.reports))
    if args.mode == "curve":
        legs = curve(reports, args.baseline)
        if args.decks:
            by_deck(legs, *args.decks)
    elif args.mode == "paired":
        paired(reports, args.against, args.chair)
    else:
        chairs(reports, args.against)


if __name__ == "__main__":
    main()
