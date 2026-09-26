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
* **The formats are its other output**, `crates/netrunner_core/data/formats.json`:
  for each format this engine offers, NetrunnerDB's active card pool (its
  packs, and every printing code of every card in it) and its active
  restriction list as printing codes -- every printing of a banned or
  pointed card. Codes rather than packs, because a card file names one
  printing and a format admits a card by any of them. Read off the v3 API (`formats`, `card_pools`,
  `card_cycles`, `card_sets`, `restrictions`, `printings`). `--check`
  compares it too. Casual is not NetrunnerDB's and is not in the file: it is
  every pack and no list, which `format.rs` says itself.
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
V3 = "https://api.netrunnerdb.com/api/v3/public"
ROOT = Path(__file__).resolve().parent.parent
CARDS_DIR = ROOT / "crates" / "netrunner_core" / "data" / "cards"
FORMATS_FILE = ROOT / "crates" / "netrunner_core" / "data" / "formats.json"

# The NetrunnerDB formats this engine offers, by v3 id.
FORMATS = ["startup", "standard", "eternal", "snapshot"]
CARD_FILES = [ROOT / "crates" / "netrunner_core" / "data" / side for side in ("corp", "runner")]

# Every pack the engine embeds, in the order the NSG card-pool plan builds
# them (docs/roadmap/nsg-card-pool.md), after the three that came first.
# The order is documentation only: the file name is the pack code, and
# `build.rs` reads the directory sorted.
PACKS = [
    "sg", "core", "elev",
    "vp", "rwr", "tai", "ph", "msbp", "ms", "urbp", "ur", "df", "su21", "sm", "mor",
]


def get(url: str) -> dict:
    # NetrunnerDB answers urllib's default agent with 403 Forbidden.
    request = urllib.request.Request(url, headers={"User-Agent": "netrunner-rs catalog_sync"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)


def fetch_cards() -> list[dict]:
    return get(CARDS_URL)["data"]


def get_all(url: str) -> list[dict]:
    """Every page of a v3 collection."""
    items: list[dict] = []
    while url:
        page = get(url)
        items.extend(page["data"])
        url = page.get("links", {}).get("next")
    return items


def fetch_formats() -> dict:
    """What `formats.json` holds: per format, the pool as v2 pack codes and
    the restriction list as printing codes, each list sorted so a re-sync
    of unchanged data is an empty diff."""
    sets = get_all(f"{V3}/card_sets?page%5Bsize%5D=1000")
    pack_of_set = {item["id"]: item["attributes"]["legacy_code"] for item in sets}
    sets_of_cycle: dict[str, list[str]] = {}
    for item in sets:
        sets_of_cycle.setdefault(item["attributes"]["card_cycle_id"], []).append(item["id"])
    printings: dict[str, list[str]] = {}
    set_of_printing: dict[str, str] = {}
    for item in get_all(f"{V3}/printings?page%5Bsize%5D=1000&fields%5Bprintings%5D=card_id,card_set_id"):
        printings.setdefault(item["attributes"]["card_id"], []).append(item["id"])
        set_of_printing[item["id"]] = item["attributes"]["card_set_id"]

    def codes(card_ids: list[str]) -> list[str]:
        missing = [card for card in card_ids if card not in printings]
        if missing:
            raise SystemExit(f"a restriction names cards with no printing: {missing}")
        return sorted(code for card in card_ids for code in printings[card])

    out = {}
    for format_id in FORMATS:
        fmt = get(f"{V3}/formats/{format_id}")["data"]["attributes"]
        pool = get(f"{V3}/card_pools/{fmt['active_card_pool_id']}")["data"]["attributes"]
        set_ids = list(pool.get("card_set_ids") or [])
        for cycle in pool.get("card_cycle_ids") or []:
            set_ids.extend(sets_of_cycle[cycle])
        # A card is in the pool when any of its printings is in one of the
        # pool's sets; the pool is then every printing of those cards, so a
        # card file built under an older printing (Corroder, Core Set) is
        # judged by the card and not by the printing it happened to name.
        in_pool = set(set_ids)
        pool_cards = [card for card, codes_ in printings.items() if any(set_of_printing[c] in in_pool for c in codes_)]
        entry = {
            "name": fmt["name"],
            "card_pool": fmt["active_card_pool_id"],
            "packs": sorted({pack_of_set[set_id] for set_id in set_ids}),
            "pool": codes(pool_cards),
            "restriction": None,
            "banned": [],
            "restricted": [],
            "points": {},
            "point_limit": None,
        }
        if fmt["active_restriction_id"]:
            restriction = get(f"{V3}/restrictions/{fmt['active_restriction_id']}")["data"]["attributes"]
            verdicts = restriction["verdicts"]
            entry["restriction"] = restriction["name"]
            entry["banned"] = codes(verdicts.get("banned") or [])
            entry["restricted"] = codes(verdicts.get("restricted") or [])
            entry["points"] = {code: points for card, points in (verdicts.get("points") or {}).items() for code in codes([card])}
            entry["point_limit"] = restriction.get("point_limit")
            unmodelled = [key for key in ("universal_faction_cost", "global_penalty") if verdicts.get(key)]
            if unmodelled:
                raise SystemExit(f"{format_id}: {unmodelled} are restriction kinds format.rs does not model")
        out[format_id] = entry
    return out


def render_formats(formats: dict) -> str:
    return json.dumps(formats, indent=2, ensure_ascii=False, sort_keys=True) + "\n"


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

    formats_text = render_formats(fetch_formats())
    current = FORMATS_FILE.read_text() if FORMATS_FILE.exists() else None
    if current != formats_text:
        differing.append("formats")
        if not args.check:
            FORMATS_FILE.write_text(formats_text)
            print(f"formats: wrote {len(FORMATS)} formats")

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
