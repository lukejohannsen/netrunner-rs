#!/usr/bin/env python3
"""Where the card pool stands: each embedded set's printings built and not,
and the DSL Growth Rule's ratio.

The NSG card-pool plan (docs/roadmap/nsg-card-pool.md) quotes this script's
output in every stage entry, rather than a hand count:

* **Per set:** printed (the set's printings in `data/catalog/cards`), built
  (a card file has the printing's card id — a reprint is the same card, so
  it is built wherever its card is), and the length of the set's
  `<SET>_UNIMPLEMENTED` list in `cards/unimplemented.rs`, which the set gate
  holds to printed minus built. Sets are named by their v2 codes here, for
  a short column; the catalog and the lists name them by v3 id.
* **Per format (Phase 5 §25 Stage 0):** of the cards in each NetrunnerDB
  format's pool (`data/formats.json`, card ids), how many the embedded
  catalog knows and how many of those are built — the number the gate
  `every_card_in_a_complete_formats_pool_is_built_and_playable` holds a
  complete format to, so "Startup is complete" is read here rather than
  claimed.
* **The DSL ratio (AGENTS.md, DSL Growth Rule):** how many `Effect` variants
  exactly one card file uses, how many none do, over how many variants and
  card files. It had been counted by hand, once per set. A variant is
  "used" by a card file when its name appears as an effect in the file: a
  JSON key holding the variant's payload (`{"GainCredits": 2}`) or a bare
  string (`"EndTheRun"`). Names are read off `pub enum Effect` in
  `dsl/effect.rs`, so a new variant is counted without editing this.

Offline: it reads only the repository. Standard library only.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORE = ROOT / "crates" / "netrunner_core"
CATALOG_DIR = CORE / "data" / "catalog"
CARD_FILES = [CORE / "data" / side for side in ("corp", "runner")]
EFFECT_RS = CORE / "src" / "dsl" / "effect.rs"
UNIMPLEMENTED_RS = CORE / "src" / "cards" / "unimplemented.rs"
EMBEDDED_RS = CORE / "src" / "cards" / "embedded.rs"
FORMATS_JSON = CORE / "data" / "formats.json"
FORMATS = ["startup", "standard", "eternal", "snapshot"]

# The plan's order, then the sets that came before it, by v2 code.
PACKS = ["sg", "core", "elev", "vp", "rwr", "tai", "ph", "msbp", "ms", "urbp", "ur", "df", "su21", "sm", "mor"]
GATED = {"sg": "SG", "elev": "ELEV", "vp": "VP", "rwr": "RWR", "tai": "TAI", "ph": "PH", "msbp": "MSBP", "ms": "MS",
         "urbp": "URBP", "ur": "UR", "df": "DF", "su21": "SU21", "sm": "SM", "mor": "MOR"}


def catalog() -> tuple[dict[str, str], dict[str, list[dict]], set[str]]:
    """(v3 set id by v2 code, each set's printings, every catalog card id)."""
    sets = json.loads((CATALOG_DIR / "sets.json").read_text())
    by_code = {item["legacy_code"]: item["id"] for item in sets}
    printings: dict[str, list[dict]] = {}
    cards: set[str] = set()
    for item in sets:
        contents = json.loads((CATALOG_DIR / "cards" / f"{item['id']}.json").read_text())
        printings[item["id"]] = contents["printings"]
        cards.update(card["id"] for card in contents["cards"])
    return by_code, printings, cards


def card_files() -> list[dict]:
    return [json.loads(path.read_text()) for directory in CARD_FILES for path in sorted(directory.glob("*.json"))]


def effect_variants() -> list[str]:
    text = EFFECT_RS.read_text()
    body = text[text.index("pub enum Effect {"):]
    body = body[: body.index("\n}\n")]
    return re.findall(r"^    ([A-Z][A-Za-z0-9]*)\b", body, re.MULTILINE)


def effects_in(node, variants: set[str], found: set[str]) -> None:
    if isinstance(node, dict):
        for key, value in node.items():
            if key in variants:
                found.add(key)
            effects_in(value, variants, found)
    elif isinstance(node, list):
        for item in node:
            effects_in(item, variants, found)
    elif isinstance(node, str) and node in variants:
        found.add(node)


def format_rows(built: set[str], known: set[str]) -> list[dict]:
    """Each format's pool against the catalog and the card files."""
    formats = json.loads(FORMATS_JSON.read_text())
    rows = []
    for name in FORMATS:
        pool = set(formats[name]["cards"])
        rows.append({"format": name, "pool": len(pool), "cards": len(pool & known), "built": len(pool & built)})
    return rows


def list_lengths() -> dict[str, int]:
    lengths = {}
    for path in (UNIMPLEMENTED_RS, EMBEDDED_RS):
        text = path.read_text()
        for name, body in re.findall(r"const (\w+)_UNIMPLEMENTED: &\[\(&str, &str\)\] = &\[(.*?)\];", text, re.DOTALL):
            lengths[name] = len(re.findall(r'^\s*\("', body, re.MULTILINE))
    return lengths


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--json", action="store_true", help="print the report as JSON")
    args = parser.parse_args()

    cards = card_files()
    built_ids = {card["id"] for card in cards}
    by_code, printings, known = catalog()
    lengths = list_lengths()

    packs = []
    for pack in PACKS:
        printed = printings[by_code[pack]]
        built = sum(1 for printing in printed if printing["card_id"] in built_ids)
        gate = GATED.get(pack)
        packs.append({"pack": pack, "set": by_code[pack], "printed": len(printed), "built": built,
                      "unimplemented_list": lengths.get(gate) if gate else None})

    formats = format_rows(built_ids, known)
    variants = effect_variants()
    uses: Counter[str] = Counter()
    for card in cards:
        found: set[str] = set()
        effects_in(card, set(variants), found)
        uses.update(found)
    single = sorted(name for name in variants if uses[name] == 1)
    unused = sorted(name for name in variants if uses[name] == 0)
    ratio = {"card_files": len(cards), "effect_variants": len(variants), "single_use": len(single),
             "unused": len(unused), "single_use_names": single, "unused_names": unused}

    if args.json:
        print(json.dumps({"packs": packs, "formats": formats, "dsl": ratio}, indent=2))
        return 0

    print(f"{'pack':6} {'printed':>7} {'built':>6} {'list':>5}")
    for row in packs:
        listed = "-" if row["unimplemented_list"] is None else row["unimplemented_list"]
        print(f"{row['pack']:6} {row['printed']:>7} {row['built']:>6} {listed:>5}")
    print()
    for row in formats:
        outside = f", {row['pool'] - row['cards']} not in the catalog" if row["pool"] != row["cards"] else ""
        complete = " — complete" if row["built"] == row["pool"] else ""
        print(f"format {row['format']:9} {row['built']:>4}/{row['cards']:<4} cards built ({row['pool']} in the pool{outside}){complete}")
    print()
    print(f"DSL: {ratio['single_use']} of {ratio['effect_variants']} Effect variants single-use, "
          f"{ratio['unused']} unused, over {ratio['card_files']} card files")
    print(f"  unused: {', '.join(unused) or '-'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
