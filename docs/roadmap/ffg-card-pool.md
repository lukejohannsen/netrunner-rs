# Phase 1 §9, part two — the Fantasy Flight Games card pool

The plan for the rest of Eternal: the 57 sets of the Fantasy Flight Games
era that the embedded catalog does not hold. It follows
[nsg-card-pool.md](nsg-card-pool.md), whose recipe, gates and rules it
reuses, and starts when that plan's last tranche (the reprint packs and
the Core Set's remainder) closes.

Written 6 October 2026 at `3cec1a7`, from what the repository holds:
`crates/netrunner_core/data/formats.json` (each format's sets and card
ids, synced from NetrunnerDB) and `scripts/pool_status.py`. The cards'
text is not in the repository yet, so **there is no per-card survey
here**: Stage 0 brings the text in and writes one. The commands are under
[How the numbers were taken](#how-the-numbers-were-taken).

## Decisions (6 October 2026)

- **The FFG cycles come after the NSG pool** (the person's, Phase 1 §9,
  25 September 2026), and the Core Set's 68 unbuilt cards ride with the
  NSG reprint packs, so the embedded catalog is complete and gated before
  any FFG set is brought in (the person's, 6 October 2026).
- **The order, the milestones and Stage 0 below were proposed by Claude
  and taken by the person** ("do your suggestion", 6 October 2026). Each is
  a proposal the person can still change, and the first stage of each
  tranche re-reads its survey before building on it, as in the NSG plan.
- **Newest first, Snapshot before the rest of Eternal.** The reason is the
  one the NSG plan gave for Standard: the cheapest sets go first, and the
  last FFG cycles are the ones whose designs NSG's first sets continued,
  so the most of their vocabulary is already built. Snapshot (1,181 cards,
  42 sets) is a format a person can name, so it is the first milestone.
  The sets only Eternal holds (Genesis, Spin, NAPD Multiplayer, Magnum
  Opus, System Core 2019) close the plan.
- **The Revised Core Set goes first.** It is the era's staples, and most of
  what it prints is reprinted from the Core Set and the early cycles, so
  building it is cheap and puts the commonest cards on the table soonest.
- **The observation vocabulary grows once, in Stage 0**, as the NSG plan
  grew it, rather than once per cycle (see Stage 0b).
- **Decks, as in the NSG plan:** `DeckCategory::Sweep` decks built here for
  every stage. At each milestone (Snapshot complete, Eternal complete)
  current tournament lists for that format join the `Sample` pool and
  `decks::tests::PUBLISHED` pins them. **Standard's step is done** (6
  October 2026, `scripts/tournament_decks.py`; the NSG plan records it),
  and is the shape to repeat: one rule for which lists, a script that
  writes them, nothing edited by hand.
- **The printed card is NetrunnerDB's current text.** NSG's errata to FFG
  cards are in the v3 text the sync brings in, and the Linked Clause Rule's
  quote gate holds every clause to it. Every card is judged by the current
  Comprehensive Rules (`rules/`), not the FFG-era rules it was printed
  under.

## Where it stands

From `pool_status.py` and `formats.json` at `3cec1a7`:

| Format | Pool | In the catalog | Built | Not in the catalog |
|---|---|---|---|---|
| Startup | 225 | 225 | 225 | — |
| Standard | 613 | 613 | 613 | — |
| Snapshot | 1,181 | 137 | 23 | 1,044 |
| Eternal | 2,017 | 797 | 638 | 1,220 |

The 159 Eternal cards in the catalog and unbuilt are the NSG plan's last
tranche. Of the 1,220 not in the catalog, 1,044 are in Snapshot's sets and
176 in the five sets only Eternal holds. Eternal bans seven cards
(Accelerated Diagnostics, Aghora, Hired Help, Sadyojata, Salvaged Vanadis
Armory, Vamadeva, Watch the World Burn), none in the catalog; a banned card
is still built, last in its set, as the NSG plan does. Eternal's points
list (7 points a deck) is already read by deck legality.

## The sets

Eternal's 72 sets less the 15 the catalog embeds. The set ids are
`formats.json`'s. **The cycle each pack belongs to is read off the sync at
Stage 0**; the grouping below is from the packs' names and is checked there.

| # | Tranche | Sets | In Snapshot |
|---|---|---|---|
| F1 | Revised Core Set | `revised_core_set` | yes |
| F2 | Reign and Reverie | `reign_and_reverie` | yes |
| F3 | Kitara | `sovereign_sight`, `down_the_white_nile`, `council_of_the_crest`, `the_devil_and_the_dragon`, `whispers_in_nalubaale`, `kampala_ascendent` | yes |
| F4 | Red Sand | `daedalus_complex`, `station_one`, `earths_scion`, `blood_and_water`, `free_mars`, `crimson_dust` | yes |
| F5 | Flashpoint | `23_seconds`, `blood_money`, `escalation`, `intervention`, `martial_law`, `quorum` | yes |
| F6 | Mumbad, and Data and Destiny | `kala_ghoda`, `business_first`, `democracy_and_dogma`, `salsette_island`, `the_liberated_mind`, `fear_the_masses`; `data_and_destiny` | yes |
| F7 | SanSan, and Order and Chaos | `the_valley`, `breaker_bay`, `chrome_city`, `the_underway`, `old_hollywood`, `the_universe_of_tomorrow`; `order_and_chaos` | yes |
| F8 | Lunar, and Honor and Profit | `upstalk`, `the_spaces_between`, `first_contact`, `up_and_over`, `all_that_remains`, `the_source`; `honor_and_profit` | yes |
| F9 | Creation and Control | `creation_and_control` | yes — **Snapshot complete** |
| F10 | Spin | `opening_moves`, `second_thoughts`, `mala_tempora`, `true_colors`, `fear_and_loathing`, `double_time` | no |
| F11 | Genesis | `what_lies_ahead`, `trace_amount`, `cyber_exodus`, `a_study_in_static`, `humanitys_shadow`, `future_proof` | no |
| F12 | The rest | `napd_multiplayer`, `magnum_opus`, `system_core_2019` | no — **Eternal complete** |

A cycle tranche is about 120 cards, nearly twice an NSG set, and splits
into stages the same way: composes first, then vocabulary, then machinery.
System Core 2019 and the Revised Core Set reprint heavily; the sync counts
what each set adds that the catalog does not already hold, and a set that
adds nothing closes at Stage 0.

## Stage 0 — groundwork, before F1

No cards. Four parts, each its own PR.

- **0a — the catalog.** Add the 57 set ids to `SETS` in
  `scripts/catalog_sync.py` and run it, so `data/catalog/` holds every
  Eternal card and printing and `build.rs` embeds them. Measure the
  embedded catalog's size and the build's time before and after. Each set
  gets an `every_<set>_card_is_implemented_or_explicitly_excluded` gate over
  an `<SET>_UNIMPLEMENTED` list that only shrinks (one list per cycle is
  enough), and `pool_status.py` reports each pack.
  **The sync needs `api.netrunnerdb.com`, which this cloud environment's
  network policy refuses**: it is run on the person's machine, or after
  the host is added to the environment's allowed domains.
- **0b — the observation vocabulary, once.** `CARD_VOCAB` is 1,024: the
  legacy 184 slots and the NSG packs' blocks end at slot 758, so 265 are
  free, and the FFG sets bring about 1,220 cards. Grow it once, to 2,560,
  with a fixed block per FFG set after the NSG blocks (`RESERVED_BLOCKS`),
  every existing slot untouched. That is room for the whole of Eternal and
  about 540 cards to spare. `OBS_SIZE` grows by five planes of 1,536, and a
  trained network is retrained once. Rejected: letting the FFG cards share
  the overflow slot, which would make a thousand cards one card to the
  network; and growing per cycle, which is twelve reshapes and twelve
  retrains (the NSG plan's reason, restated).
- **0c — the survey.** Read every new card against the DSL at 0a's commit
  and class it C, V or M as the NSG surveys did, a section per tranche in
  this file, and write each tranche's stage list. Until then the mechanics
  below are expectations, not findings.
- **0d — the clients.** The card image cache is keyed by printing and
  needs nothing new. The set filter's cycle marks (`card_text::set_icon`)
  are checked against NetrunnerDB's icon font for every FFG cycle, and the
  deck builder and the card browser are measured over 2,000 cards: the
  NSG plan's Stage 0 promised a browser that stays fast over "a few
  thousand", and this is where that is checked.

## Expected new machinery

From what is widely known of these cycles, not from a survey (0c
replaces this list). The NSG pool already built most of the era's
mechanics: traces, psi games, bad publicity, Trojans, hosting, flip
identities, the mark, set aside, and from tranche 8 expose and abilities
from Archives and the heap. What looks new:
- Runner cards installed facedown (Apex), and Runner cards turned
  facedown (Apocalypse).
- Cards installed at setup (Adam's directives).
- An identity chosen after setup (Jinteki Biotech) or replaced mid-game
  (Rebirth). Méliès U's three reverse sides are the nearest built thing.
- Installing in place of a breach (the Shards).
- Virus counters one program shares with others (Hivemind).

Each gets a Comprehensive Rules re-read and a conformance row before it is
built, as in the NSG plan.

## The recipe

Every stage follows [the NSG plan's recipe](nsg-card-pool.md#the-recipe--every-stage-of-every-tranche)
unchanged:
- the rule read first;
- card files, then per-card tests;
- Sweep decks;
- both 256-seed sweeps;
- `coverage_identical.py`;
- `pool_status.py`'s ratio;
- the conformance row;
- the client side of anything the view gains.

Closed entries go to `docs/roadmap/archive/ffg-card-pool.md`, verbatim,
the address kept.

The DSL Growth Rule's ratio stands at 13 of 105 `Effect` variants
single-use, none unused, over 638 card files (Downfall Stage 8). The FFG
pool roughly triples the card count. If a tranche's single-use variants
approach its card count, stop and build a composition primitive first.

## How the numbers were taken

```bash
python3 scripts/pool_status.py            # formats: pool, in catalog, built
python3 - <<'EOF'                         # the sets and the format split
import json
f = json.load(open("crates/netrunner_core/data/formats.json"))
emb = {s["id"] for s in json.load(open("crates/netrunner_core/data/catalog/sets.json"))}
print([s for s in f["eternal"]["sets"] if s not in emb])        # the 57 sets
print(f["eternal"]["banned"], f["eternal"]["point_limit"])
EOF
```

The not-in-catalog split (1,044 Snapshot, 176 Eternal-only) is the
Eternal and Snapshot card-id lists in `formats.json` less every card id in
`data/catalog/cards/`.
