#!/usr/bin/env python3
"""Keep the embedded NetrunnerDB catalog in step with NetrunnerDB's v3 API.

The catalog is NetrunnerDB's own split of a card from its printings (NSG
card-pool plan, Stage 0d):

* `crates/netrunner_core/data/catalog/sets.json` -- every embedded card set:
  its v3 id (`system_gateway`), name, v2 `legacy_code` (`sg`), cycle (the
  glyph name of its mark in NetrunnerDB's icon font), `date_release`,
  `position`, `size` and `first_printing_id`.
* `crates/netrunner_core/data/catalog/cards/<set id>.json` -- one file per
  set, `{"id": <set id>, "cards": [...], "printings": [...]}`. A **printing** is one card as
  one set prints it: its id is the printing code (`30075`), with the card it
  prints, its place in the set, the copies a set holds, the illustrators,
  the flavour and whether a high-resolution scan exists. A **card** is the
  rules object, by v3 slug (`hedge_fund`), with its title, text and numbers;
  it is written once, in the file of its earliest embedded printing, so a
  reprint is a printing and never a second card.

`build.rs` embeds the directory and `cards::catalog` reads it. A set joins
the catalog by adding its v3 id to `SETS` and running this script -- never
by hand.

Two things are folded rather than copied, both so a reader sees what the
card says: a card with more than one face (a flip identity, Méliès U) has
its other faces' text appended the way NetrunnerDB's v2 API spelled it
("Flip side:", "Side 2:"), which is what the Linked Clause gate quotes from;
and a printing's flavour carries its other faces' flavour and the card's
design credit ("Designed by ..."). Every other field is v3's, as v3 spells
it -- `cost` is a string, because a card can print X.

* **With no flags it writes** every set in `SETS` and `formats.json`, and
  removes a set file it no longer owns.
* **`--check` writes nothing.** It names every file whose committed text
  differs from the live data -- an erratum, a new printing, a corrected
  number -- and exits 1 on any difference. A changed card's printed text is
  then the Linked Clause gate's business (`printed_clauses_are_quoted_from_
  the_card`), and its numbers `printed_values_agree_with_the_netrunnerdb_
  catalog`'s.
* **The formats are its other output**, `crates/netrunner_core/data/formats.json`:
  for each format this engine offers, NetrunnerDB's active card pool (its
  sets, and every card in it, by card id) and its active restriction list
  (by card id). A card is in a pool when any of its printings is in one of
  the pool's sets, which v3 states per card. Casual is not NetrunnerDB's and
  is not in the file: it is every card and no list, which `format.rs` says
  itself.
* **`--unimplemented <set id>`** prints the `<SET>_UNIMPLEMENTED` entries for
  a set's cards that no card file builds yet, for pasting into
  `cards/unimplemented.rs` when a set is added.

No fetch date is written anywhere: a sync of unchanged data is an empty diff.
Standard library only, like `rules_sync.py`.
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from pathlib import Path

V3 = "https://api.netrunnerdb.com/api/v3/public"
ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "crates" / "netrunner_core" / "data"
CATALOG_DIR = DATA / "catalog"
SETS_FILE = CATALOG_DIR / "sets.json"
CARDS_DIR = CATALOG_DIR / "cards"
FORMATS_FILE = DATA / "formats.json"

# The NetrunnerDB formats this engine offers, by v3 id.
FORMATS = ["startup", "standard", "eternal", "snapshot"]
CARD_FILES = [DATA / side for side in ("corp", "runner")]

# Every set the engine embeds, by v3 id, in the order the NSG card-pool plan
# builds them (docs/roadmap/nsg-card-pool.md), after the three that came
# first. The order is documentation only: `cards::catalog` orders sets by
# release.
SETS = [
    "system_gateway", "core_set", "elevation",
    "vantage_point", "rebellion_without_rehearsal", "the_automata_initiative", "parhelion",
    "midnight_sun_booster_pack", "midnight_sun", "uprising_booster_pack", "uprising", "downfall",
    "system_update_2021", "salvaged_memories", "magnum_opus_reprint",
]

# What a card carries, by v3 attribute name. `text` is folded (above).
CARD_FIELDS = [
    "title", "side_id", "faction_id", "card_type_id", "display_subtypes", "text",
    "cost", "strength", "advancement_requirement", "agenda_points", "trash_cost", "memory_cost",
    "base_link", "influence_cost", "influence_limit", "minimum_deck_size", "deck_limit", "is_unique",
]
SET_FIELDS = ["name", "legacy_code", "card_cycle_id", "date_release", "position", "size", "first_printing_id"]


def get(url: str) -> dict:
    # NetrunnerDB answers urllib's default agent with 403 Forbidden.
    request = urllib.request.Request(url, headers={"User-Agent": "netrunner-rs catalog_sync"})
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)


def get_all(url: str) -> list[dict]:
    """Every page of a v3 collection, as `{id, **attributes}`."""
    items: list[dict] = []
    while url:
        page = get(url)
        items.extend({"id": item["id"], **item["attributes"]} for item in page["data"])
        url = page.get("links", {}).get("next")
    return items


def collection(name: str) -> list[dict]:
    return get_all(f"{V3}/{name}?page%5Bsize%5D=1000")


def folded_text(card: dict) -> str | None:
    """The card's text with its other faces' appended, as v2 spelled it."""
    text = card["text"]
    faces = card.get("faces") or []
    if card["layout_id"] == "flip":
        for face in faces:
            text = f"{text}\nFlip side:\n{face['text']}"
    elif card["layout_id"] == "facade":
        for index, face in enumerate(faces):
            text = f"{text}\nSide {index + 1}: {face['text']}"
    elif faces:
        raise SystemExit(f"{card['id']}: a {card['layout_id']!r} layout with faces, which this script does not fold")
    return text


def folded_flavor(printing: dict, card: dict) -> str | None:
    lines = [printing.get("flavor")] + [face.get("flavor") for face in printing.get("faces") or []]
    if card.get("attribution"):
        lines.append(f"<strong>{card['attribution']}</strong>")
    lines = [line for line in lines if line]
    return "\n".join(lines) if lines else None


def release_key(set_: dict) -> tuple[str, str]:
    return (set_["date_release"], set_["first_printing_id"])


def catalog_files(sets: list[dict], cards: list[dict], printings: list[dict]) -> dict[Path, str]:
    """Every file under `data/catalog`, by path, rendered."""
    by_id = {set_["id"]: set_ for set_ in sets}
    missing = [set_id for set_id in SETS if set_id not in by_id]
    if missing:
        raise SystemExit(f"NetrunnerDB lists no sets {missing}")
    embedded = [by_id[set_id] for set_id in SETS]
    card_of = {card["id"]: card for card in cards}

    files: dict[str, dict] = {set_id: {"cards": [], "printings": []} for set_id in SETS}
    earliest: dict[str, tuple] = {}
    for printing in printings:
        set_id = printing["card_set_id"]
        if set_id not in files:
            continue
        card = card_of[printing["card_id"]]
        images = (printing.get("images") or {}).get("nrdb_classic") or {}
        files[set_id]["printings"].append({
            "id": printing["id"],
            "card_id": printing["card_id"],
            "position": printing["position"],
            "quantity": printing["quantity"],
            "illustrators": printing.get("display_illustrators"),
            "flavor": folded_flavor(printing, card),
            "has_xlarge": "xlarge" in images,
        })
        key = (*release_key(by_id[set_id]), printing["id"])
        if printing["card_id"] not in earliest or key < earliest[printing["card_id"]][0]:
            earliest[printing["card_id"]] = (key, set_id)
    for card_id, (_key, set_id) in earliest.items():
        card = card_of[card_id]
        entry = {"id": card_id, **{field: card.get(field) for field in CARD_FIELDS}}
        entry["text"] = folded_text(card)
        files[set_id]["cards"].append(entry)

    out = {SETS_FILE: render([{"id": set_["id"], **{field: set_[field] for field in SET_FIELDS}}
                              for set_ in sorted(embedded, key=lambda set_: set_["id"])])}
    for set_id, contents in files.items():
        if not contents["printings"]:
            raise SystemExit(f"{set_id}: NetrunnerDB lists no printings for this set")
        out[CARDS_DIR / f"{set_id}.json"] = render({
            "id": set_id,
            "cards": sorted(contents["cards"], key=lambda card: card["id"]),
            "printings": sorted(contents["printings"], key=lambda printing: printing["id"]),
        })
    return out


def fetch_formats(sets: list[dict], cards: list[dict]) -> dict:
    """What `formats.json` holds: per format, the pool as v3 set ids and
    card ids, and the restriction list as card ids, each list sorted so a
    re-sync of unchanged data is an empty diff."""
    sets_of_cycle: dict[str, list[str]] = {}
    for set_ in sets:
        sets_of_cycle.setdefault(set_["card_cycle_id"], []).append(set_["id"])
    known = {card["id"] for card in cards}

    def ids(card_ids: list[str]) -> list[str]:
        unknown = [card for card in card_ids if card not in known]
        if unknown:
            raise SystemExit(f"a restriction names cards NetrunnerDB does not list: {unknown}")
        return sorted(card_ids)

    out = {}
    for format_id in FORMATS:
        fmt = get(f"{V3}/formats/{format_id}")["data"]["attributes"]
        pool_id = fmt["active_card_pool_id"]
        pool = get(f"{V3}/card_pools/{pool_id}")["data"]["attributes"]
        set_ids = list(pool.get("card_set_ids") or [])
        for cycle in pool.get("card_cycle_ids") or []:
            set_ids.extend(sets_of_cycle[cycle])
        entry = {
            "name": fmt["name"],
            "card_pool": pool_id,
            "sets": sorted(set(set_ids)),
            # v3 names each card's pools; a card is in one when any of its
            # printings is in one of the pool's sets.
            "cards": sorted(card["id"] for card in cards if pool_id in (card.get("card_pool_ids") or [])),
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
            entry["banned"] = ids(verdicts.get("banned") or [])
            entry["restricted"] = ids(verdicts.get("restricted") or [])
            points = verdicts.get("points") or {}
            entry["points"] = {card: points[card] for card in ids(list(points))}
            entry["point_limit"] = restriction.get("point_limit")
            unmodelled = [key for key in ("universal_faction_cost", "global_penalty") if verdicts.get(key)]
            if unmodelled:
                raise SystemExit(f"{format_id}: {unmodelled} are restriction kinds format.rs does not model")
        out[format_id] = entry
    return out


def render(value) -> str:
    return json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n"


def built_card_ids() -> set[str]:
    return {json.loads(path.read_text())["id"] for directory in CARD_FILES for path in directory.glob("*.json")}


def unimplemented(cards: list[dict], printings: list[dict], set_id: str) -> list[str]:
    built = built_card_ids()
    titles = {card["id"]: card["title"] for card in cards}
    lines = []
    for printing in sorted((p for p in printings if p["card_set_id"] == set_id), key=lambda p: p["id"]):
        if printing["card_id"] in built:
            continue
        title = titles[printing["card_id"]].replace("\\", "\\\\").replace('"', '\\"')
        lines.append(f'    ("{printing["card_id"]}", "{title}"),')
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--check", action="store_true", help="compare with the live data; write nothing")
    parser.add_argument("--unimplemented", metavar="SET", help="print the unbuilt cards of one set, by v3 set id")
    args = parser.parse_args()

    sets, cards, printings = collection("card_sets"), collection("cards"), collection("printings")

    if args.unimplemented:
        print("\n".join(unimplemented(cards, printings, args.unimplemented)))
        return 0

    wanted = catalog_files(sets, cards, printings)
    wanted[FORMATS_FILE] = render(fetch_formats(sets, cards))

    differing = []
    for path, text in wanted.items():
        current = path.read_text() if path.exists() else None
        if current == text:
            continue
        differing.append(path.relative_to(ROOT))
        if not args.check:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
            print(f"wrote {path.relative_to(ROOT)}")

    unowned = sorted(path for path in CARDS_DIR.glob("*.json") if path not in wanted)
    for path in unowned:
        differing.append(path.relative_to(ROOT))
        if not args.check:
            path.unlink()
            print(f"removed {path.relative_to(ROOT)}: not a set this script embeds")

    if args.check:
        for path in differing:
            print(f"{path}: the committed catalog differs from NetrunnerDB")
        return 1 if differing else 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
