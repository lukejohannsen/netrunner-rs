#!/usr/bin/env python3
"""Say whether two refs play the same games: four coverage reports, pinned binaries.

"Byte-identical" is this workspace's claim that a change moved no rule, and
until this script it was made by hand: build something, run `--headless
--report`, md5 the JSON. Three things went wrong with that often enough to
be written into AGENTS.md as warnings, and each is enforced here instead:

* **The binary was not pinned.** A before/after taken with `cargo run` on a
  tree being edited compared two versions of the *new* code (Phase 3 records
  the instance). Here a ref is checked out into a worktree of its own under
  `target/pinned/src`, built there, and the binary copied to
  `target/pinned/bin/netrunner_cli-<sha>` -- so what runs is what the sha
  says, and a sha already built is never built again. One worktree and one
  target directory are reused for every ref, so a second ref recompiles only
  the workspace crates that differ, not the dependency tree.
* **`--games` stopped short of the pool.** `--all-matchups` plays
  `matchups[index % len]`, so a run below the cross product leaves whole
  decks unplayed and their cards read as zero on both sides of the diff --
  identical, and measuring nothing. The game count is read off the ref's own
  sample decks (Corp x Runner), one full pass.
* **One seating was taken for all of them.** The view path (`Seat::Agent`)
  and the index path (`--index-path`, the `ActionSpace` round trip) reach
  different code, and random and heuristic bots reach different rules. All
  four are run: random and heuristic, each by view and by index.

A pair that differs is not just "differs": every number that moved is
listed by its path in the report, a section at a time, and `triggers_fired`
-- one count per card per trigger, read off the engine's own `TriggerFired`
-- is never cut short, because that is the column a dispatch change moves
first.

`--expect-renames Old=New,...` is for a change that renames `Trigger`
variants on purpose. `triggers_fired` keys are `card/Trigger`, so such a
change cannot be byte-identical; it passes when renaming the base report's
keys (summing where two collapse into one) makes the reports equal, and
fails on anything else.

`--head-worktree` measures the checkout as it stands, uncommitted changes
included. The binary is still copied out and named for the diff it was
built from, so an edit made while the games run cannot reach them.

Exit status is 0 when every pair is identical (or explained by the renames)
and 1 otherwise, so it can gate a merge by hand the way the deep sweeps do.
It is not a CI job for the same reason they are not: two release builds and
eight full passes of the pool.

Usage:
    scripts/coverage_identical.py main                 # main against HEAD
    scripts/coverage_identical.py main feat/x
    scripts/coverage_identical.py main --head-worktree
    scripts/coverage_identical.py main main             # self-test: must be identical
"""
import argparse
import concurrent.futures
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PINNED = REPO / "target" / "pinned"
SOURCE = PINNED / "src"
BUILD = PINNED / "build"
BINARIES = PINNED / "bin"

# (name, bot kind for both chairs, extra flags). Both chairs take the same
# kind: a mixed seating measures the bot as much as the engine (AGENTS.md).
REPORTS = [
    ("random-view", "random", []),
    ("random-index", "random", ["--index-path"]),
    ("heuristic-view", "heuristic", []),
    ("heuristic-index", "heuristic", ["--index-path"]),
]


def git(*args, cwd=REPO):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


def matchup_count(root):
    """Corp x Runner over the sample decks: `decks::matchups().len()`, read
    off the ref's own deck files so a pool that grew is played whole."""
    sides = {"Corp": 0, "Runner": 0}
    for path in (root / "crates" / "netrunner_core" / "data" / "decks").glob("*.json"):
        deck = json.loads(path.read_text())
        if deck.get("category") == "Sample":
            sides[deck["side"]] += 1
    return sides["Corp"] * sides["Runner"]


def build(root, target_dir, binary):
    if binary.exists():
        return
    BINARIES.mkdir(parents=True, exist_ok=True)
    print(f"building {binary.name} ...", flush=True)
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "-p", "netrunner_cli", "--target-dir", str(target_dir)],
        cwd=root,
        check=True,
    )
    shutil.copy2(target_dir / "release" / "netrunner_cli", binary)


def pin_ref(ref):
    """Build `ref` in the shared worktree; return (label, binary, games)."""
    sha = git("rev-parse", "--verify", f"{ref}^{{commit}}")
    if not SOURCE.exists():
        PINNED.mkdir(parents=True, exist_ok=True)
        git("worktree", "add", "--detach", str(SOURCE), sha)
    else:
        git("checkout", "--detach", "--force", sha, cwd=SOURCE)
    binary = BINARIES / f"netrunner_cli-{sha[:12]}"
    build(SOURCE, BUILD, binary)
    return f"{ref}@{sha[:12]}", binary, matchup_count(SOURCE)


def pin_worktree():
    """Build the checkout as it stands, named for the diff it carries."""
    sha = git("rev-parse", "HEAD")
    dirty = subprocess.run(["git", "diff", "HEAD"], cwd=REPO, check=True, capture_output=True).stdout
    untracked = git("ls-files", "--others", "--exclude-standard")
    digest = hashlib.md5(dirty + untracked.encode()).hexdigest()[:8]
    binary = BINARIES / f"netrunner_cli-{sha[:12]}-worktree-{digest}"
    build(REPO, REPO / "target", binary)
    return f"worktree@{sha[:12]}+{digest}", binary, matchup_count(REPO)


def play(binary, kind, extra, games, seed, out):
    out.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [str(binary), "--headless", "--all-matchups", "--games", str(games), "--seed", str(seed),
         "--corp", kind, "--runner", kind, "--report", str(out), *extra],
        cwd=out.parent,
        check=True,
        stdout=subprocess.DEVNULL,
    )
    return out


def parse_renames(spec):
    renames = {}
    for pair in filter(None, (spec or "").split(",")):
        old, _, new = pair.partition("=")
        if not old or not new:
            sys.exit(f"--expect-renames wants Old=New pairs, got {pair!r}")
        renames[old] = new
    return renames


def renamed(report, renames):
    """`report` with its `triggers_fired` keys renamed, counts summed where
    two triggers collapse into one."""
    fired = {}
    for key, count in report.get("triggers_fired", {}).items():
        card, _, trigger = key.rpartition("/")
        key = f"{card}/{renames.get(trigger, trigger)}"
        fired[key] = fired.get(key, 0) + count
    return {**report, "triggers_fired": fired}


def flattened(value, prefix=""):
    if isinstance(value, dict):
        for key, inner in value.items():
            yield from flattened(inner, f"{prefix}/{key}" if prefix else key)
    else:
        yield prefix, value


# `triggers_fired` is never cut short: it is the column a dispatch change
# moves first, and the one a reader is here for.
LINES_PER_SECTION = 12


def describe_difference(base, head):
    lines = []
    for section in sorted(base.keys() | head.keys()):
        if base.get(section) == head.get(section):
            continue
        before, after = dict(flattened(base.get(section, {}), section)), dict(flattened(head.get(section, {}), section))
        moved = [key for key in sorted(before.keys() | after.keys()) if before.get(key) != after.get(key)]
        shown = moved if section == "triggers_fired" else moved[:LINES_PER_SECTION]
        lines += [f"    {key}: {before.get(key, 0)} -> {after.get(key, 0)}" for key in shown]
        if len(shown) < len(moved):
            lines.append(f"    {section}: ... and {len(moved) - len(shown)} more")
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("base", help="the ref to measure against, usually main")
    parser.add_argument("head", nargs="?", default="HEAD", help="the ref under test (default HEAD)")
    parser.add_argument("--head-worktree", action="store_true", help="measure the checkout, uncommitted changes included")
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--games", type=int, help="override one full pass of the pool (never go below it for per-card claims)")
    parser.add_argument("--expect-renames", metavar="OLD=NEW,...", help="Trigger variants renamed on purpose")
    args = parser.parse_args()
    renames = parse_renames(args.expect_renames)

    # Sequential: both refs build in the one shared worktree.
    base_label, base_binary, base_games = pin_ref(args.base)
    head_label, head_binary, head_games = pin_worktree() if args.head_worktree else pin_ref(args.head)
    games = args.games or max(base_games, head_games)
    if base_games != head_games:
        print(f"note: the sample pool changed ({base_games} -> {head_games} matchups); the reports cannot be identical")

    out = REPO / "target" / "coverage" / "identical" / f"{base_binary.name}--{head_binary.name}"
    print(f"{base_label}  vs  {head_label}: {games} games a report, seed {args.seed}\nreports under {out.relative_to(REPO)}")

    with concurrent.futures.ThreadPoolExecutor(max_workers=2 * len(REPORTS)) as pool:
        runs = {
            (name, side): pool.submit(play, binary, kind, extra, games, args.seed, out / f"{name}.{side}.json")
            for name, kind, extra in REPORTS
            for side, binary in (("base", base_binary), ("head", head_binary))
        }
        paths = {key: run.result() for key, run in runs.items()}

    failed = False
    for name, _, _ in REPORTS:
        base_bytes, head_bytes = (paths[(name, side)].read_bytes() for side in ("base", "head"))
        md5 = hashlib.md5(base_bytes).hexdigest()
        if base_bytes == head_bytes:
            print(f"  {name:<16} identical  {md5}")
            continue
        base, head = json.loads(base_bytes), json.loads(head_bytes)
        if renames and renamed(base, renames) == head:
            print(f"  {name:<16} identical but for the expected renames")
            continue
        failed = True
        print(f"  {name:<16} DIFFERS    {md5} -> {hashlib.md5(head_bytes).hexdigest()}")
        print("\n".join(describe_difference(renamed(base, renames), head)))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
