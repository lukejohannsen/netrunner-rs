#!/usr/bin/env python3
"""Compare two `netrunner_cli bench --report` JSONs game by game.

A strength leg on this workspace is two pinned binaries playing the *same*
games -- same seeds, same matchup schedule, same bots -- so the honest
comparison is paired, not two independent win rates. `bench` writes one
record per game with its seed, matchup and winner, and two reports taken
at the same `--games/--seed/--simulations` line those records up exactly,
which is what makes the pairing possible without any bookkeeping in the
engine.

Per pairing (corp bot vs runner bot) this prints:

* the Corp win rate before and after, and the delta;
* **McNemar's z over the discordant games** -- games one binary's Corp won
  and the other's lost. Games both binaries decide the same way carry no
  information about the change and inflate an unpaired test's denominator;
  after ROADMAP Phase 2 §5 item 37, three of six cells in a leg were
  *byte-identical*, where an unpaired comparison still reports a
  confidence interval.
* the discordant count itself, because `z` on four discordant games is
  not a number to quote.

A delta also has to beat Phase 3's seed-spread band (0.026-0.047 over 192
games) before it means anything: the pairing removes sampling noise from
deck and seed, not the trajectory drift any code change re-rolls.

`--by-kind` keys each seat on the bot's *kind* -- `mcts@512#t16` pairs with
`mcts@128` -- so two cells taken at different `--simulations` or `--label`
still line up on the same games. `bench` puts the budget in the
participant id, which is right for a rating book and wrong for asking
"what did 16 trees buy over 4 on these games". It is only honest when
both reports seat one bot family in the same place in the square.

Usage:
    scripts/paired_bench.py before.json after.json [--label NAME] [--by-kind]
"""
import argparse
import json
import math
import sys


def kind(participant):
    """`mcts@512:rush#t16` -> `mcts:rush`: the budget and the label go,
    the personality stays, because it changes what the bot is."""
    base, _, _ = participant.partition("#")
    name, _, rest = base.partition("@")
    personality = rest.partition(":")[2] if rest else ""
    return f"{name}:{personality}" if personality else name


def games(path, by_kind=False):
    """Games keyed by what identifies one trial: the pairing, the deck
    matchup and the seed. Keyed rather than zipped by index so a report
    taken with a different `--bots` order still lines up, and so a
    mismatch is caught rather than silently comparing different games."""
    report = json.load(open(path))
    keyed = {}
    for game in report["games"]:
        corp, runner = game["corp"], game["runner"]
        if by_kind:
            corp, runner = kind(corp), kind(runner)
        key = (corp, runner, game["matchup"], game["seed"])
        if key in keyed:
            sys.exit(f"{path}: two games share {key}; nothing can be paired on it")
        keyed[key] = game["winner"]
    return keyed


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("before")
    parser.add_argument("after")
    parser.add_argument("--label", default="")
    parser.add_argument("--by-kind", action="store_true", help="pair seats on the bot kind, ignoring @budget and #label")
    args = parser.parse_args()

    before, after = games(args.before, args.by_kind), games(args.after, args.by_kind)
    shared = before.keys() & after.keys()
    if not shared:
        sys.exit("the two reports share no games: check --games/--seed/--simulations match")
    if len(shared) != len(before) or len(shared) != len(after):
        print(f"note: {len(before)} vs {len(after)} games, {len(shared)} paired\n")

    pairings = sorted({(key[0], key[1]) for key in shared})
    width = max(len(f"{corp} vs {runner}") for corp, runner in pairings)
    if args.label:
        print(args.label)
    print(f"{'pairing (Corp win rate)':<{width}}  {'before':>7}  {'after':>7}  {'delta':>7}  {'z':>6}  {'disc':>4}")
    for corp, runner in pairings:
        keys = [key for key in shared if key[0] == corp and key[1] == runner]
        won_before = sum(before[key] == "Corp" for key in keys)
        won_after = sum(after[key] == "Corp" for key in keys)
        gained = sum(after[key] == "Corp" and before[key] != "Corp" for key in keys)
        lost = sum(before[key] == "Corp" and after[key] != "Corp" for key in keys)
        discordant = gained + lost
        # Under the null the discordant games split evenly, so the count
        # of one direction is Binomial(discordant, 1/2).
        z = (gained - lost) / math.sqrt(discordant) if discordant else None
        print(
            f"{corp + ' vs ' + runner:<{width}}  {won_before / len(keys):>7.3f}  {won_after / len(keys):>7.3f}"
            f"  {(won_after - won_before) / len(keys):>+7.3f}  {f'{z:+.2f}' if z is not None else '—':>6}  {discordant:>4}"
        )


if __name__ == "__main__":
    main()
