#!/usr/bin/env python3
"""Where the card pool stands: each embedded pack's printings built and not,
and the DSL Growth Rule's ratio.

The NSG card-pool plan (docs/roadmap/nsg-card-pool.md) quotes this script's
output in every stage entry, rather than a hand count:

* **Per pack:** printed (the pack's catalog file), built (a playable card
  carries the printing's code, or its title under `cards::title_key`'s
  fold, which is how a reprint is built), and the length of the pack's
  `<SET>_UNIMPLEMENTED` list in `cards/unimplemented.rs`, which the set gate
  holds to printed minus built.
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
CARDS_DIR = CORE / "data" / "cards"
CARD_FILES = [CORE / "data" / side for side in ("corp", "runner")]
EFFECT_RS = CORE / "src" / "dsl" / "effect.rs"
UNIMPLEMENTED_RS = CORE / "src" / "cards" / "unimplemented.rs"
EMBEDDED_RS = CORE / "src" / "cards" / "embedded.rs"

# The plan's order, then the packs that came before it.
PACKS = ["sg", "core", "elev", "vp", "rwr", "tai", "ph", "msbp", "ms", "urbp", "ur", "df", "su21", "sm", "mor"]
GATED = {"sg": "SG", "elev": "ELEV", "vp": "VP", "rwr": "RWR", "tai": "TAI", "ph": "PH", "msbp": "MSBP", "ms": "MS",
         "urbp": "URBP", "ur": "UR", "df": "DF", "su21": "SU21", "sm": "SM", "mor": "MOR"}


def title_key(title: str) -> str:
    """`cards::title_key`, the same fold as `catalog_sync.py`'s."""
    for fancy, plain in (("’", "'"), ("ʼ", "'"), ("‘", "'"), ("“", '"'), ("”", '"')):
        title = title.replace(fancy, plain)
    return title.lower()


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


def list_lengths() -> dict[str, int]:
    lengths = {}
    for path in (UNIMPLEMENTED_RS, EMBEDDED_RS):
        text = path.read_text()
        for name, body in re.findall(r"const (\w+)_UNIMPLEMENTED: &\[\(u32, &str\)\] = &\[(.*?)\];", text, re.DOTALL):
            lengths[name] = len(re.findall(r"^\s*\(\d+,", body, re.MULTILINE))
    return lengths


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--json", action="store_true", help="print the report as JSON")
    args = parser.parse_args()

    cards = card_files()
    codes = {card["numeric_id"] for card in cards if card.get("numeric_id") is not None}
    titles = {(card["side"].lower(), title_key(card["title"])) for card in cards}
    lengths = list_lengths()

    packs = []
    for pack in PACKS:
        printed = json.loads((CARDS_DIR / f"{pack}.json").read_text())
        built = sum(
            1 for card in printed
            if int(card["code"]) in codes or (card["side_code"], title_key(card["title"])) in titles
        )
        gate = GATED.get(pack)
        packs.append({"pack": pack, "printed": len(printed), "built": built,
                      "unimplemented_list": lengths.get(gate) if gate else None})

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
        print(json.dumps({"packs": packs, "dsl": ratio}, indent=2))
        return 0

    print(f"{'pack':6} {'printed':>7} {'built':>6} {'list':>5}")
    for row in packs:
        listed = "-" if row["unimplemented_list"] is None else row["unimplemented_list"]
        print(f"{row['pack']:6} {row['printed']:>7} {row['built']:>6} {listed:>5}")
    print()
    print(f"DSL: {ratio['single_use']} of {ratio['effect_variants']} Effect variants single-use, "
          f"{ratio['unused']} unused, over {ratio['card_files']} card files")
    print(f"  unused: {', '.join(unused) or '-'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
