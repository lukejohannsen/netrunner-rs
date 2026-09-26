#!/usr/bin/env python3
"""Keep the embedded NetrunnerDB catalog in step with NetrunnerDB.

`crates/netrunner_core/data/cards/<pack>.json` is NetrunnerDB's v2 card data
for one pack: the `data` array of `/api/2.0/public/cards`, filtered to the
pack, sorted by code, each card's keys sorted, two-space indent, UTF-8 kept.
`build.rs` concatenates the directory and `cards::netrunnerdb` converts it,
so a pack is added to the engine's catalog by adding its code to `PACKS`
and running this script -- never by hand. The first three files were
committed by hand before this script existed, and it reproduces them byte for
byte, which is the check that the format above is the one they used.

* **With no flags it writes** every pack in `PACKS`, and removes nothing it
  does not own: a file whose pack is not listed is left alone and named.
* **`--check` writes nothing.** It names every pack whose committed file
  differs from the live data -- an erratum, a new printing, a corrected
  number -- and exits 1 on any difference. A changed card's printed text is
  then the Linked Clause gate's business (`printed_clauses_are_quoted_from_
  the_card`), and its numbers `printed_values_agree_with_the_netrunnerdb_
  catalog`'s.
* **`--unimplemented <pack>`** prints the `<SET>_UNIMPLEMENTED` entries for
  a pack's cards that no card file builds yet, for pasting into
  `cards/unimplemented.rs` when a pack is added.

No fetch date is written anywhere: a sync of unchanged data is an empty diff.
Standard library only, like `rules_sync.py`.
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from pathlib import Path

CARDS_URL = "https://netrunnerdb.com/api/2.0/public/cards"
ROOT = Path(__file__).resolve().parent.parent
CARDS_DIR = ROOT / "crates" / "netrunner_core" / "data" / "cards"
CARD_FILES = [ROOT / "crates" / "netrunner_core" / "data" / side for side in ("corp", "runner")]

# Every pack the engine embeds, in the order the NSG card-pool plan builds
# them (docs/roadmap/nsg-card-pool.md), after the three that came first.
# The order is documentation only: the file name is the pack code, and
# `build.rs` reads the directory sorted.
PACKS = [
    "sg", "core", "elev",
    "vp", "rwr", "tai", "ph", "msbp", "ms", "urbp", "ur", "df", "su21", "sm", "mor",
]


def fetch_cards() -> list[dict]:
    # NetrunnerDB answers urllib's default agent with 403 Forbidden.
    request = urllib.request.Request(CARDS_URL, headers={"User-Agent": "netrunner-rs catalog_sync"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)["data"]


def render(cards: list[dict]) -> str:
    cards = sorted(cards, key=lambda card: card["code"])
    return json.dumps(cards, indent=2, ensure_ascii=False, sort_keys=True) + "\n"


def title_key(title: str) -> str:
    """The same fold as `cards::title_key`: a printing's title with its
    typographic apostrophes and quotes made plain, lowercased. NetrunnerDB
    spells a reprint's title with whatever the reprint's editor typed --
    The Maker's Eye is straight in the Core Set and curly in System Update
    2021 -- so a title match that did not fold them missed it."""
    for fancy, plain in (("’", "'"), ("ʼ", "'"), ("‘", "'"), ("“", '"'), ("”", '"')):
        title = title.replace(fancy, plain)
    return title.lower()


def built_cards() -> tuple[set[int], set[tuple[str, str]]]:
    codes: set[int] = set()
    titles: set[tuple[str, str]] = set()
    for directory in CARD_FILES:
        for path in directory.glob("*.json"):
            card = json.loads(path.read_text())
            if card.get("numeric_id") is not None:
                codes.add(card["numeric_id"])
            titles.add((card["side"].lower(), title_key(card["title"])))
    return codes, titles


def unimplemented(cards: list[dict], pack: str) -> list[str]:
    codes, titles = built_cards()
    lines = []
    for card in sorted((c for c in cards if c["pack_code"] == pack), key=lambda c: c["code"]):
        if int(card["code"]) in codes or (card["side_code"], title_key(card["title"])) in titles:
            continue
        title = card["title"].replace("\\", "\\\\").replace('"', '\\"')
        lines.append(f'    ({int(card["code"])}, "{title}"),')
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--check", action="store_true", help="compare with the live data; write nothing")
    parser.add_argument("--unimplemented", metavar="PACK", help="print the unbuilt cards of one pack")
    args = parser.parse_args()

    cards = fetch_cards()

    if args.unimplemented:
        print("\n".join(unimplemented(cards, args.unimplemented)))
        return 0

    differing = []
    for pack in PACKS:
        pack_cards = [card for card in cards if card["pack_code"] == pack]
        if not pack_cards:
            print(f"{pack}: NetrunnerDB lists no cards for this pack", file=sys.stderr)
            return 2
        path = CARDS_DIR / f"{pack}.json"
        text = render(pack_cards)
        current = path.read_text() if path.exists() else None
        if current == text:
            continue
        differing.append(pack)
        if not args.check:
            path.write_text(text)
            print(f"{pack}: wrote {len(pack_cards)} cards")

    unowned = sorted(path.stem for path in CARDS_DIR.glob("*.json") if path.stem not in PACKS)
    for stem in unowned:
        print(f"{stem}.json: not a pack this script owns; left alone", file=sys.stderr)

    if args.check:
        for pack in differing:
            print(f"{pack}: the committed catalog differs from NetrunnerDB")
        return 1 if differing else 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
