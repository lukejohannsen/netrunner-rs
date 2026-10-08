#!/usr/bin/env python3
"""The cards random seats use that the planner never does -- the bots' blind spots, measured.

`netrunner_cli diag precepts --report` ends with every card a seat *used*
over a pass (played, installed, rezzed, activated, advanced or scored by
its owner) and the cards of the decks played that no seat used
(`reach.unused_in_pass`). That list is "no seat used it" for one seating,
and it cannot see the blindness Phase 5 §28 closed: an installed
Matryoshka counted as used while the planner never hosted a copy on it.
The actionable list is the *difference between seatings*: a card random
seats reach often is a card the engine offers in ordinary play, so one
the planner never touches is a judgment of the evaluator, not a card the
decks never draw.

Two modes, over reports taken on the same `--games/--seed/--format`:

* **precepts** (default): two `diag precepts --report` JSONs, random seats
  first, planner seats second. Prints, per side, every card the random
  seats used at least `--min` times that the planner used at most
  `--ratio` times as often, with both counts -- and, for completeness,
  the cards only the planner uses, which is where a term has made a card
  the random seat is merely indifferent to.
* **`--coverage`**: two `--headless --report` coverage JSONs. The same
  diff over `installed + played + rezzed + activated + prompts_offered`,
  then a second table over `prompts_offered` alone: reach counts a card
  as used when it is installed, prompts count what its *text* was asked
  to do, which is the instrument for a card whose use is an ability that
  opens a prompt (Matryoshka's host, Madani's).

A card's side is read off the deck files under
`crates/netrunner_core/data/decks/` (a deck's `side`, its identity
included), since neither report says. A card in no deck is listed
under "either".

Counts, not ratios, are what the reports hold; the ratio here is derived
once, planner over random, so a card used 40 times by random and 0 by
the planner reads 0.00 and one used 40 / 38 reads 0.95.

The matchup pass deals Sample decks only; a card that only a Sweep deck
holds (most of a new set) is measured on two `diag precepts --sweep-decks`
reports, the sweeps' schedule (Phase 5 §57).

Usage:
    scripts/blind_cards.py random.json planner.json [--min 5] [--ratio 0.1]
    scripts/blind_cards.py --coverage random.json planner.json [--min 5]
"""
import argparse
import json
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DECKS = REPO / "crates" / "netrunner_core" / "data" / "decks"

USE_KEYS = ("installed", "played", "rezzed", "activated", "prompts_offered")


def sides():
    """card id -> 'Corp' | 'Runner', off every deck file."""
    by_card = {}
    for path in DECKS.glob("*.json"):
        deck = json.loads(path.read_text())
        for card in [deck["identity"], *(entry["card"] for entry in deck["cards"])]:
            by_card[card] = deck["side"]
    return by_card


def precepts_used(report):
    return {card: int(n) for card, n in report["reach"]["used"].items()}


def coverage_used(report, keys):
    return {card: sum(int(counts.get(key, 0)) for key in keys) for card, counts in report["cards"].items()}


def diff(random_used, planner_used, minimum, ratio):
    """(blind, planner_only): cards random reaches at least `minimum`
    times that the planner reaches at most `ratio` as often, and cards
    the planner reaches that random never does."""
    blind = []
    for card, n in random_used.items():
        if n < minimum:
            continue
        m = planner_used.get(card, 0)
        if m <= n * ratio:
            blind.append((card, n, m))
    planner_only = [(card, m) for card, m in planner_used.items() if m >= minimum and random_used.get(card, 0) == 0]
    return blind, planner_only


def print_table(title, rows, side_of):
    print(f"\n{title}")
    if not rows:
        print("  (none)")
        return
    for side in ("Corp", "Runner", "either"):
        group = [row for row in rows if side_of.get(row[0], "either") == side]
        if not group:
            continue
        print(f"  {side}:")
        for row in sorted(group, key=lambda r: (-r[1], r[0])):
            if len(row) == 3:
                card, n, m = row
                print(f"    {card:<40} random {n:>5}  planner {m:>5}  ratio {m / n:.2f}")
            else:
                card, m = row
                print(f"    {card:<40} planner {m:>5}")


def describe(report):
    if "reach" in report:
        return f"{report['games']} games, {report['format']}, seed {report['seed']}, {report['corp']} Corp vs {report['runner']} Runner"
    return f"{report.get('games', '?')} games"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("random", type=Path, help="report with random seats")
    parser.add_argument("planner", type=Path, help="report with planner seats, same games")
    parser.add_argument("--coverage", action="store_true", help="the two files are --headless --report coverage JSONs")
    parser.add_argument("--min", type=int, default=5, help="random uses a card must reach to be listed (default 5)")
    parser.add_argument("--ratio", type=float, default=0.0, help="planner-over-random use at or under which a card is blind (default 0: never)")
    args = parser.parse_args()

    random_report = json.loads(args.random.read_text())
    planner_report = json.loads(args.planner.read_text())
    side_of = sides()
    print(f"random:  {args.random} -- {describe(random_report)}")
    print(f"planner: {args.planner} -- {describe(planner_report)}")

    if args.coverage:
        blind, planner_only = diff(coverage_used(random_report, USE_KEYS), coverage_used(planner_report, USE_KEYS), args.min, args.ratio)
        print_table(f"Blind to the planner (random uses >= {args.min}, planner <= {args.ratio:.2f} of that; {' + '.join(USE_KEYS)}):", blind, side_of)
        prompts, _ = diff(coverage_used(random_report, ("prompts_offered",)), coverage_used(planner_report, ("prompts_offered",)), args.min, args.ratio)
        print_table(f"Prompts the planner never opens (prompts_offered, random >= {args.min}):", prompts, side_of)
    else:
        blind, planner_only = diff(precepts_used(random_report), precepts_used(planner_report), args.min, args.ratio)
        print_table(f"Blind to the planner (random uses >= {args.min}, planner <= {args.ratio:.2f} of that):", blind, side_of)
    print_table("Only the planner uses:", planner_only, side_of)
    print(f"\n{len(blind)} blind")


if __name__ == "__main__":
    main()
