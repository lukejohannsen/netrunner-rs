#!/usr/bin/env python3
"""Write the Standard tournament lists the `Sample` pool holds.

The NSG card-pool plan (`docs/roadmap/nsg-card-pool.md`, decision of 26
September 2026) owes this step once Standard is complete: current Standard
tournament lists from NetrunnerDB join the `Sample` pool, and
`decks::tests::PUBLISHED` pins them. **Nothing here is chosen by hand, and
no list is edited.** The script reads who played what off Always Be Running
and the lists themselves off NetrunnerDB, and writes each one as a deck file
in `crates/netrunner_core/data/decks/`.

Which lists, by one rule (6 October 2026):

* **The meta is the 2026 World Championship's top cut.** Every identity a
  player in its top sixteen brought is one the pool should hold.
* **Each identity's list is the best-placed one published** from the
  championships played under the current card pool and balance update: the
  World Championship, then the EMEA, Americas and APAC Online Continentals,
  largest event first, and within an event by top-cut rank. A list is
  "published" when the player linked it from Always Be Running to
  NetrunnerDB; most top-cut players did not, which is why one event is not
  enough.
* An identity no player published a list for in those events is left out
  rather than filled from elsewhere (Mercury: Chrome Libertador, 6 October
  2026).

Each deck's description credits the list's author on NetrunnerDB, its
link, and the player who took it to its placing (a player may bring
someone else's list). The style (the bot's plans) is left empty: how a list
wants to be played is a reading this project writes, and a published list
arrives without one.

* **With no flags it writes** one deck file per list, `tournament_<identity
  card id>.json` (one list per identity, so the id is stable when a better
  placed list replaces it), and prints what it chose.
* **`--check` writes nothing.** It names every deck file whose committed
  text differs from what the sources say now, and exits 1 on a difference.

Standard library only, like `catalog_sync.py`.
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DECKS = ROOT / "crates" / "netrunner_core" / "data" / "decks"

ABR = "https://alwaysberunning.net/api"
V2 = "https://netrunnerdb.com/api/2.0/public"
V3 = "https://api.netrunnerdb.com/api/v3/public"

# The championships, largest first: (Always Be Running id, name, date).
WORLDS = (5464, "the 2026 World Championship", "2 October 2026")
EVENTS = [
    WORLDS,
    (5762, "the 2026 EMEA Online Continental", "5 September 2026"),
    (5761, "the 2026 Americas Online Continental", "15 August 2026"),
    (5760, "the 2026 APAC Online Continental", "29 August 2026"),
]


def get(url: str):
    request = urllib.request.Request(url, headers={"User-Agent": "netrunner-rs tournament_decks.py"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)


def ordinal(n: int) -> str:
    suffix = "th" if 10 <= n % 100 <= 20 else {1: "st", 2: "nd", 3: "rd"}.get(n % 10, "th")
    return f"{n}{suffix}"


def top_cut(event: int) -> list[dict]:
    entries = [entry for entry in get(f"{ABR}/entries?id={event}") if entry.get("rank_top")]
    return sorted(entries, key=lambda entry: entry["rank_top"])


# The deck builder's longest name (`netrunner_client::deck_builder::MAX_NAME`).
MAX_NAME = 48


def deck_name(title: str) -> str:
    """The published title, or -- when it is longer than a deck name may be
    -- the title without its trailing bracketed placing ("High Speed Rail
    3.0 (15-3, 1st/9th&17th@EMEA IRL/Online)"). The whole title stays in
    the description."""
    title = title.strip()
    while len(title) > MAX_NAME and title[-1] in ")]":
        opening = "(" if title[-1] == ")" else "["
        title = title[: title.rfind(opening)].strip()
    return title[:MAX_NAME].strip()


def v3_decklist(url: str) -> dict:
    """The v3 decklist a NetrunnerDB link names. An old link carries the
    v2 numeric id, which v3 does not answer to; v2 gives its uuid."""
    key = url.rstrip("/").split("/decklist/")[1].split("/")[0]
    if key.isdigit():
        key = get(f"{V2}/decklist/{key}")["data"][0]["uuid"]
    data = get(f"{V3}/decklists/{key}")["data"]
    return {"uuid": data["id"], **data["attributes"]}


def choose() -> list[dict]:
    meta = {"corp": set(), "runner": set()}
    for entry in top_cut(WORLDS[0]):
        for side in meta:
            meta[side].add(entry[f"{side}_deck_identity_id"])

    chosen: dict[tuple[str, str], dict] = {}
    for event, name, date in EVENTS:
        for entry in top_cut(event):
            for side in meta:
                identity, url = entry[f"{side}_deck_identity_id"], entry[f"{side}_deck_url"]
                if identity in meta[side] and url and (side, identity) not in chosen:
                    player = entry["user_name"] or entry["user_import_name"]
                    chosen[(side, identity)] = {"event": name, "date": date, "rank": entry["rank_top"], "player": player, "url": url}
    missing = sorted(
        entry[f"{side}_deck_identity_title"]
        for entry in top_cut(WORLDS[0])
        for side in meta
        if (side, entry[f"{side}_deck_identity_id"]) not in chosen
    )
    for title in dict.fromkeys(missing):
        print(f"no published list: {title}", file=sys.stderr)
    return list(chosen.values())


def deck_file(pick: dict) -> tuple[str, dict]:
    deck = v3_decklist(pick["url"])
    link = f"https://netrunnerdb.com/en/decklist/{deck['uuid']}"
    identity = deck["identity_card_id"]
    cards = [{"card": card, "count": count} for card, count in sorted(deck["card_slots"].items()) if card != identity]
    deck_id = f"tournament_{identity}"
    return deck_id, {
        "id": deck_id,
        "name": deck_name(deck["name"]),
        "side": deck["side_id"].capitalize(),
        "category": "Sample",
        "description": (
            f"{deck['name'].strip()}, published on NetrunnerDB by {deck['user_id']} ({link}). "
            f"{pick['player']} played it to {ordinal(pick['rank'])} in the top cut of {pick['event']} ({pick['date']})."
        ),
        "identity": identity,
        "cards": cards,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="write nothing; exit 1 if a deck file differs")
    args = parser.parse_args()

    differs = []
    for pick in choose():
        deck_id, deck = deck_file(pick)
        path = DECKS / f"{deck_id}.json"
        text = json.dumps(deck, indent=2, ensure_ascii=False) + "\n"
        print(f"{deck['side']:6} {deck['identity']:50} {path.name}")
        if args.check:
            if not path.exists() or path.read_text() != text:
                differs.append(path.name)
        else:
            path.write_text(text)
    for name in differs:
        print(f"differs: {name}", file=sys.stderr)
    return 1 if differs else 0


if __name__ == "__main__":
    sys.exit(main())
