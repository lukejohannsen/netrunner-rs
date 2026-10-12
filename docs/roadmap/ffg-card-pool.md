# Phase 1 §9, part two — the Fantasy Flight Games card pool

The plan for the rest of Eternal: the 57 sets of the Fantasy Flight Games
era that the embedded catalog does not hold. It follows
[nsg-card-pool.md](nsg-card-pool.md), whose recipe, gates and rules it
reuses, and starts when that plan's last tranche (the reprint packs and
the Core Set's remainder) closes.

Written 6 October 2026 at `3cec1a7`, from what the repository holds:
`crates/netrunner_core/data/formats.json` (each format's sets and card
ids, synced from NetrunnerDB) and `scripts/pool_status.py`. Stage 0 brought
the cards' text in (0a) and surveyed every card against the DSL (0c). The commands are under
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
  **0a is two PRs.** A trial sync found what the engine could not read,
  and that lands first, on its own:
  - **0a-i closed** (9 October 2026): the three Runner mini-factions,
    `Faction::{Adam, Apex, SunnyLebeau}` (appended, so no serialized
    faction moves), with their NetrunnerDB marks (`e91d`, `e91e`,
    `e91c`) and colours in both clients, and no plan of their own for
    the bots; and v3's strength of -1, which is a printed X (Darwin,
    Surveyor), read as no number, as a cost of "X" is. Every printed
    subtype in the FFG sets is already a `CardSubtype`. The trial also
    found a set the table above misses: **Terminal Directive Cards**
    (`terminal_directive_cards`, 57 printings), in neither format's set
    list but with 43 cards in both pools; it joins F5.
  - **0a-ii closed** (9 October 2026): the sync itself. The catalog
    embeds 73 sets, 2,423 printings of 2,017 cards (15 sets, 846
    printings of 797 cards before), and every new set has its gate over
    an `<SET>_UNIMPLEMENTED` list, 1,307 printings in all, which
    `pool_status.py` reports per pack: 797 of Eternal's 2,017 cards are
    built and 137 of Snapshot's 1,181. The embedded JSON grew from 0.8 MB
    to 2.1 MB and `netrunner_core`'s debug rlib from 116 MB to 120 MB; a
    rebuild of the crate took 21 s on both sides and a release build with
    its dependencies 45 s, so the build's time did not move. Eternal bans
    seven of the new cards (Aghora, Watch the World Burn and five more).
    What the new text needed from the clients came with it: a faction's
    mark inside rules text ("6 or more non-alliance [haas-bioroid]
    cards", the eight Mumbad alliances) and a trace of X (Searchlight,
    Surveyor and three more) are segments of their own
    (`card_text::Segment::{Faction, Superscript}`), a strength of X on
    ice or an icebreaker is drawn as X (`card_face::Slot::StrengthX`),
    and NAPD Multiplayer is the one embedded set with no mark, because
    NetrunnerDB's icon font draws none for its cycle.
- **0b — the observation vocabulary, once.** `CARD_VOCAB` is 1,024: the
  legacy 184 slots and the NSG packs' blocks end at slot 758, and the Core
  Set's block after them at 871 (NSG tranche 8 Stage 10a), so 152 are
  free, and the FFG sets bring about 1,220 cards. Grow it once, to 2,560,
  with a fixed block per FFG set after the NSG blocks (`RESERVED_BLOCKS`),
  every existing slot untouched. That is room for the whole of Eternal and
  about 540 cards to spare. `OBS_SIZE` grows by five planes of 1,536, and a
  trained network is retrained once. Rejected: letting the FFG cards share
  the overflow slot, which would make a thousand cards one card to the
  network; and growing per cycle, which is twelve reshapes and twelve
  retrains (the NSG plan's reason, restated).
  **0b closed** (11 October 2026): `CARD_VOCAB` 1,024 → 2,560 and
  `OBS_SIZE` 6,423 → 14,103, with 56 blocks after the Core Set's, slots
  871 to 2,162, in the tranche order. A block is a set's printed codes
  (by printing, as the NSG blocks are), so the 1,220-card estimate above
  was the wrong unit: the FFG sets print 1,577. Two sets get no block, the
  Revised Core Set (132) and System Core 2019 (147), because every card
  they print has an earlier printing that has one. Magnum Opus's block is
  only its two codes Magnum Opus Reprint did not reprint (23100, 23101),
  since its eight codes run 23001 to 23101. That leaves 396 slots to
  spare, and every card in the catalog, built or not, has a slot waiting
  (`every_catalog_card_has_a_slot`). Every slot below 871 is unmoved, and
  no built card sat after the blocks, so no slot moved at all. No trained
  network is committed, so there is nothing to retrain yet.
- **0c — the survey.** Read every new card against the DSL at 0a's commit
  and class it C, V or M as the NSG surveys did, a section per tranche in
  this file, and write each tranche's stage list.
  **0c closed** (11 October 2026): all 1,220 cards read against the DSL
  on `main` after 0b, **C 748 / V 353 / M 119**, in [The survey
  (Stage 0c)](#the-survey-stage-0c), with a section and a stage list per
  tranche under [The tranches](#the-tranches) and a row per card in
  [ffg-card-survey.md](ffg-card-survey.md). It replaces the list of
  expected machinery this file began with; of that list, Apex's facedown
  cards, Adam's directives, Rebirth, the Shards and Hivemind are all
  findings (M), and Jinteki Biotech's identity chosen before the first
  turn is vocabulary (V), on Méliès U's copies.
- **0d — the clients.** The card image cache is keyed by printing and
  needs nothing new. The set filter's cycle marks (`card_text::set_icon`)
  are checked against NetrunnerDB's icon font for every FFG cycle, and the
  deck builder and the card browser are measured over 2,000 cards: the
  NSG plan's Stage 0 promised a browser that stays fast over "a few
  thousand", and this is where that is checked.
  **0d closed** (11 October 2026), and with it Stage 0:
  - **The marks.** All 27 cycle glyphs `card_text::cycle_icon` names
    are in the committed `NetrunnerDB-Icons.ttf` (read off its `cmap`),
    and every embedded set but NAPD Multiplayer resolves to one, as
    `every_embedded_set_has_its_cycles_mark` already held in 0a-ii.
  - **The browser was not fast over 2,017 cards, and is now.** It
    spawned a face for every card the filters let through, and a
    keystroke in the search respawned them all. Timed in a real window
    (the release client under Xvfb, on the Mesa software renderer, four
    cores, a 1600×1000 window), a keystroke over the whole catalog took
    a frame of 0.7 to 1.2 seconds, and a frame with nothing happening
    took 124 ms against 62 ms for a grid narrowed to a handful. The
    grid is now windowed (`card_browser::Shown`): only the rows in view
    and four either side have thumbs, and the grid's padding stands for
    the rest, so the scrollbar still measures the whole pool. The same
    run afterwards: no keystroke frame over 115 ms, and an idle frame
    of 73 ms. A screenshot scrolled 20,000 px down shows the thumbs
    there. Rejected: a "show more" page, which makes the scrollbar lie
    about where a card is; and hiding thumbs off screen, which still
    spawns, lays out and shapes every one. A headless test has no
    window to keep thumbs to, so it still spawns every one, which keeps
    the navigation tests' counts of the whole catalog.
  - **The deck builder is not windowed yet, and does not need to be
    today.** Its pool is the playable cards of one side, 360 Runner
    cards at most now, drawn large, and an idle frame on that screen
    took 83 ms in the same setup. It grows to about a thousand a side
    as the FFG tranches are built, so the builder's pool gets the same
    windowing before Snapshot completes; it is owed, not done.

## The survey (Stage 0c)

Every card the sync brought in and Eternal holds, 1,220 of them, was read
against the DSL on `main` at 11 October 2026 (Downfall and FFG Stage 0b
closed) and put in one of the NSG surveys' three classes:

- **C**: composes from existing variants, at most a new filter or amount.
- **V**: needs vocabulary, a new `Trigger`, `EventFilter`, `Amount`,
  `CardFilter`, `Cost`, `ContinuousKind` or `PaysFor` word, and no new
  rules machinery.
- **M**: needs a mechanic the engine does not model.

A class is one reader's judgement, and the first stage of each tranche
re-reads it. Every card is in exactly one stage of its tranche, and every
rule cited is in `rules/comprehensive-rules.md`; both were checked by
script. The per-card tables, a row per card in stage order with what it
composes from or needs, are in [ffg-card-survey.md](ffg-card-survey.md).

| Tranche | Cards | C | V | M |
|---|---|---|---|---|
| F1 Revised Core Set | 38 | 32 | 6 | 0 |
| F2 Reign and Reverie | 58 | 41 | 10 | 7 |
| F3 Kitara | 120 | 80 | 33 | 7 |
| F4 Red Sand | 120 | 63 | 48 | 9 |
| F5 Flashpoint, and Terminal Directive Cards | 168 | 108 | 41 | 19 |
| F6 Mumbad, and Data and Destiny | 167 | 87 | 55 | 25 |
| F7 SanSan, and Order and Chaos | 170 | 104 | 45 | 21 |
| F8 Lunar, and Honor and Profit | 156 | 82 | 59 | 15 |
| F9 Creation and Control | 47 | 28 | 15 | 4 |
| F10 Spin | 97 | 70 | 20 | 7 |
| F11 Genesis | 76 | 52 | 21 | 3 |
| F12 The rest | 3 | 1 | 0 | 2 |
| **Total** | **1,220** | **748** | **353** | **119** |

**Six cards in ten compose**, where the NSG sets composed between one in
ten and one in three. The NSG pool built most of this era's mechanics: traces,
psi games, advanceable ice and assets, click-breakable bioroids, Trojans,
arrange, "as an agenda", ambush upgrades revealed in R&D, access
replacements, persistent cards, steal costs, `LaterThisTurn`, remembered
choices, `ForceEncounter` and the lockdown play area. The Revised Core Set
adds 38 cards and System Core 2019 none, because nearly everything they
print was printed first in another set.

### Mechanics shared across tranches

Each is built once, in the first tranche in this file's order that prints
it, and later tranches compose on it. The rule is read first, as in the
NSG plan, and **`Built in`** is filled by the stage that builds it.
"Nearest" is the nearest relative the NSG plan built.

| Mechanic | First needed | Also in | Rule | Nearest | Built in |
|---|---|---|---|---|---|
| **Currents**: an event or operation that stays active until another current is played, or an agenda is scored (the Runner's) or stolen (the Corp's) | F3: Corporate "Grant", Death and Taxes, Kill Switch | F4 Rolling Brownout, System Seizure; F5 nine cards and Another Day, Another Paycheck; F6 six, with News Hound and New Angeles Sol; F7 six; F8 nine | CR 3.5.1b, CR 3.7.1b, CR 8.6.6c | Lockdown (UR 7a, `CorpState::play_area`), which is the Corp's only and ends at a turn's start, not on an event | — |
| **Condition counters**: an operation or event that becomes a counter with text, hosted on a card | F2: Eavesdrop | F4 MCA Informant, Rover Algorithm, Transparency Initiative; F5 On the Lam, Wetwork Refit; F6 Political Graffiti; F7 Casting Call, Patch, Sub Boost; F9 Bioroid Efficiency Research; F11 Oversight AI | CR 1.9.5j, CR 1.13.6d, CR 3.5.1a | Hosting in general (VP 7b, 7c), Runner hosts only, and lockdown | — |
| **Corp cards that host cards** (a Corp install has no hosted list) | F6: Full Immersion RecStudio | F7 Glenn Station, Worlds Plaza; F9 Awakening Center (ice hosted and encountered there); every condition counter on a Corp card | CR 1.13.1a, CR 1.13.5 | Hosting in general (Runner side) | — |
| **Runner cards installed or turned facedown** in the rig | F2: Reboot | F3 Assimilator; F6 Apex: Invasive Predator, Harbinger, Hunting Grounds, Apocalypse | CR 8.1.4, CR 4.6.4d | PH 7's facedown *hosted* cards (Matryoshka) | — |
| **The Runner moved to another position or server mid-run** | F2: Formicary | F3 Code Replicator, Mti Mwekundu, Slipstream; F10 Copycat; F11 Bullfrog | CR 6.1.3b, CR 6.2.2b, CR 6.2.6, CR 6.2.7d | `ForceEncounter` (RWR 7d, UR 7b), `MoveRunToOutermost` | — |
| **A card gaining another card's abilities** | F4: Adjusted Matrix, Dedicated Processor, Loki | F5 Baba Yaga; F6 Media Blitz | CR 9.1.9, CR 9.8.3a | Losing abilities (PH 8) is the reverse; ZATO City Grid grants a triggered ability | — |
| **Identities and cards from outside the deck** | F2: DJ Fenris | F6 Adam's directives, Rebirth; F12 Cyber Bureau's setup | CR 1.5.3, CR 1.5.4 | Méliès U's identity copies (VP 8), Ayla's setup (reprints 11w) | — |
| **One card's access replaced, or chosen by the Corp** | F6: Dedicated Neural Net, Information Sifting | F7 Archives Interface, Immolation Script; F8 Kitsune; F10 Toshiyuki Sakai | CR 7.3.4b, CR 7.3.6 | `SetAccessReplacement`, which replaces a whole breach | — |
| **Encountering ice that is accessed, not installed** | F5: Chrysalis, Herald, Sapper | — | CR 9.1.8h | Ganked!'s encounter from an access (UR 7b) | — |
| **An additional cost to jack out or to break**, and a jack-out the run would not allow | F2: Daruma | F5 Armored Servers, Mind Game | CR 1.16.10, CR 6.1.5 | Additional costs imposed by another card (VP 3a …) | — |
| **Tagged, or holding a tag, without a tag counter** | F2: Acme Consulting | F4 MCA Informant; F7 Paparazzi | CR 1.9.4, CR 10.5.2 | none: being tagged is a count | — |
| **Memory only some programs may use** | F7: MemStrips | F10 Bishop, Knight, Rook, Pawn, Deep Red (Caïssa) | CR 1.20.4, CR 1.20.4b | Djinn's `HostsMemory` (reprints 11t), outside the limit rather than restricted | — |
| **Rearranging installed ice** (may compose as repeated swaps; the stage tests that first) | F9: Escher | F11 Sunset | CR 8.3.1 | `SwapInstalledIce`, Arrange (VP 6a) | — |

Single-card mechanics, each built by the stage that builds its card:
Patchwork's cost changed as it is paid (F2); Mass-Driver's subroutines that
do not resolve in a later encounter (F4); Skorpios's trash replaced by a
set-aside, and Encore's extra turn (F5); the Runner spending first in
Surveillance Sweep's traces, the deva swap of Aghora, Sadyojata and
Vamadeva, Harishchandra's grip revealed while tagged, Raman Rai's swap of a
drawn card, Street Magic's order of unbroken subroutines, and 24/7 News
Cycle resolving another agenda's "when scored" (F6); who chooses the cards
damage trashes (Chronos Protocol, Titanium Ribs), Gene Conditioning
Shoppe's "first time" met twice, Hivemind's shared counters, Turntable's
swap between score areas and An Offer You Can't Refuse's run offered on the
Corp's turn (F7); the Shards installed from the grip in place of a breach,
Mother Goddess copying every subtype and Push Your Luck's secret spend
(F8); Tyr's Hand preventing a break (F9); and Watch the World Burn's
ability for the rest of the game (F12). Each has its rule and nearest
relative in the per-card file.

**Vocabulary more than one tranche needs**, built once, in the first:
- **Trace words** (F1 first, then F3, F5, F6, F7, F8, F9, F11): an "if
  unsuccessful" branch on `Effect::Trace`, a trigger on a successful trace
  (`GameEvent::TraceSuccessful` is emitted and nothing hears it), the base
  as an `Amount` (a trace of X), and the margin of success as an amount.
- **A program's own memory cost as an amount or changed** (F3, F6, F7),
  and **unused memory as an amount** (F5, F8).
- **Jack-out words**: a prohibition (F1 Whirlpool), a moment of its own
  (F6, F7, F8) and a cost (above).
- **Gained subtypes beyond the three ice types, and gains with a duration**
  ("until derezzed", "for the next run"): F6 Rebranding Team and
  Panchatantra, F8 Rainbow and the Morph ice, F11 Chimera, Paintbrush and
  Sensei.
- **Deck-legality words**: the alliances' "0 influence if …" (F6, twelve
  cards), Custom Biotics' banned faction and The Professor's free first
  copy (F9).
- **Cost scopes that reach the other player's installs and plays** (F8
  Docklands Crackdown, Donut Taganes), and **a scope over every agenda**
  (F7 Traffic Jam, F11 Chakana and The Source).

### Calls the first stage of a tranche re-reads

- **Two calls disagree across tranches**, and the earlier tranche's stage
  settles them: memory only virus programs may use is V in F3 and M in F7
  (MemStrips), and swapping agendas between score areas is V in F6 and M
  in F7 (Turntable). The table above takes the M reading.
- **C calls that rest on a reading no built card has tested**: Mr. Li
  (F1; drawn cards sit at the end of the hand, as Daily Business Show
  reads it), Kongamato (F3; `BreakSubroutines` breaks the lowest unbroken
  first), Kasi String (F3; `first_each_turn` counting successful run
  endings only), Hired Help (F12; a trash cost taken from the Runner's own
  score area), Tori Hanzō (F8; the prevention window over damage the Corp
  deals itself), Brain Rewiring and Dean Lister (F5).
- **V calls that may be C once read**: Injection Attack, Successful Field
  Test, Charlatan (F5); Lean and Mean, Pushing the Envelope (F4); Rainbow
  (F8, two `GainSubtype`s).
- **Facedown Runner cards are needed first by F2's Reboot**, in this
  file's order; F3's survey suggests building them with Apex in F6, which
  would hold Reboot and Assimilator until then.

### For the person

- **Cyber Bureau: Keeping the Peace** (NAPD Multiplayer) is an identity
  for the multiplayer format, which the engine does not play. It is in
  Eternal's card list, so the survey classed it (M, a setup before the
  first turn); whether it is built or excluded is the person's call.
- **The mini-factions' cards before their identities.** F2, F3 and F5
  print Adam, Apex and Sunny Lebeau cards before F6 brings the
  identities, so until then each goes into a Sweep deck as an
  out-of-faction card, paid for with influence. The default taken here:
  build them in their own tranche that way, rather than holding them to
  F6.

## The tranches

Each tranche's section is the survey's: its decks, its stages (composes first, then vocabulary grouped by word, then machinery grouped by mechanic, about ten cards a stage) and its riskiest cards. A stage that closes adds its one line under the section, as the NSG plan does.

### F1. Revised Core Set — 38 cards (C 32 / V 6 / M 0)

**Decks:** two new identities, Chaos Theory: Wünderkind (Shaper) and Haas-Bioroid: Stronger Together. One Sweep deck on each, as the NSG plan did. The other 36 cards are Core-era reprints and go into the Sweep decks the Core identities already head, each as a swap.

**Stages** (from the survey):
1. **Runner programs, composed**: Force of Nature, Morning Star, Peacock, Faerie, Darwin, Hemorrhage, Dinosaurus, Dyson Mem Chip, Chaos Theory: Wünderkind.
2. **Runner resources, events and hardware, composed**: All-nighter, Doppelgänger, Fall Guy, HQ Interface, Mr. Li, Notoriety, Singularity, Underworld Contact.
3. **Corp ice and the bioroid identity, composed**: Caduceus, Flare, Himitsu-Bako, Hive, Yagura, Haas-Bioroid: Stronger Together.
4. **Corp agendas, assets, upgrades and operations, composed**: Ash 2X3ZB9CY, Braintrust, Dedicated Response Team, Elizabeth Mills, Green Level Clearance, GRNDL Refinery, Project Ares, Strongbox, TGTBT.
5. **The trace's two outcomes become things a card can hear**: an "if unsuccessful" branch on `Effect::Trace`, and a `Trigger` on `GameEvent::TraceSuccessful` (the event exists, but no trigger hears it). Bernice Mai, Spinal Modem.
6. **Words that last for a run, and meat damage made larger**: a lingering "cannot access more than N cards", a `Prohibition` on jacking out, and `IncreaseAboutToResolve` widened from a parked draw to parked damage. Hudson 1.0, Whirlpool, The Cleaners.
7. **Recurring credits counted from the card**: `recurring_credits` as an `Amount` (it is `Option<u32>` today), and a `PaysFor` word for "during runs on HQ". Pheromones.

**Stage 1 closed** (11 October 2026): the nine Runner cards, with no new `Effect` (18 of 111 single-use, none unused, over 806 card files), and two words widened for Dinosaurus. `Scope::Hosted`, which only a piece of ice used (Magnet), now reaches the programs installed onto a rig card in the continuous scan, so "Hosted icebreaker has +2 strength" is a `Strength` there. `Amount::HostedCards` now counts those programs beside the cards hosted without installing, so "a single" is the `MayHost`'s `while`. Its `HostsMemory` room is 4, the most [mu] an icebreaker prints. Darwin's printed X is its 0 plus `Strength` per hosted virus counter, as the catalog gate already read a v3 strength of −1. Revised Core Set 9 of 38 built (`CORE2_UNIMPLEMENTED` 38 → 29), and each card left the list of its first set too. Decks: a new Sweep deck, Head Start, on Chaos Theory with two Dinosaurus and three Dyson Mem Chip; Pay As You Go takes Hemorrhage and Darwin, Rez Tax Force of Nature and Morning Star, and Hit List Peacock and Faerie, two of each, for cards other decks keep.

**Stage 2 closed** (11 October 2026): the eight Runner resources, events and hardware, with no new `Effect` (18 of 111 single-use, none unused, over 814 card files). Notoriety's "Add Notoriety to your score area as an agenda worth 1 agenda point" is the existing `AddToScoreAreaAsAgenda`, which until now only moved an operation or a card in Archives: a Runner event resolving it is in no zone, so it goes straight to the Runner's score area, and `engine::play_event` files it nowhere once it has landed there rather than trashing it as every other event (CR 10.1.3). Singularity is Light the Fire!'s root trash on a run event, Doppelgänger a `PresentChoice` of a server prompt after a successful run, Fall Guy a `Prevent` of a resource's trash beside its own [trash] for 2[credit], Mr. Li a draw then a selection of the top two of the grip, and Underworld Contact an `AmountAtLeast` on `Link`. Revised Core Set 17 of 38 built (`CORE2_UNIMPLEMENTED` 29 → 21). Decks: Head Start takes All-nighter, Notoriety and Underworld Contact, Hit List Fall Guy and HQ Interface, Cold Reading Doppelgänger and Mr. Li, and Rez Tax Singularity, for cards other decks keep.

**Stage 3 closed** (11 October 2026): the five pieces of ice and the identity, with no new `Effect` (17 of 111 single-use, none unused, over 820 card files). Hive's "This ice loses 1 of its printed “[subroutine] End the run.” subroutines for each agenda point in your score area" is the `ContinuousKind::Subroutines` Echo and Envelopment already gain by, with a negative count: the ice loses that many of its printed copies, the last printed first (CR 9.8.2b), never below none. **Not modelled:** a score that falls between two encounters with the same Hive in one run gives the second nothing back. Caduceus and Flare are `Trace` subroutines (Flare's meat damage `Unpreventable`), Himitsu-Bako a paid `AddToHand`, Yagura Blueberry!™ Diesel's look-and-bottom on R&D, and Haas-Bioroid: Stronger Together a `Strength` over `Ice(HasSubtype(Bioroid))`. Revised Core Set 23 of 38 built (`CORE2_UNIMPLEMENTED` 21 → 15). Decks: a new Sweep deck, Stronger Together, on Retirement Package's frame with eight bioroids, Hive, Caduceus, Yagura and Himitsu-Bako; Paid Content takes two Flare for its two Klevetnik.

**Stage 4 closed** (11 October 2026): the nine Corp agendas, assets, upgrades and operations, composed entirely from words the pool already had — no engine change and no new `Effect` (17 of 111 single-use, none unused, over 829 card files). Ash 2X3ZB9CY is Adrian Seis's `Prohibit(AccessOthers)` behind a `Trace`; Braintrust is Project Atlas's counting by `Every` two past 3 beside a `RezCost` over every piece of ice; Dedicated Response Team an `OnRunEnded` heard while the Runner `IsTagged`; Elizabeth Mills an `OnRez` `RemoveBadPublicity` and a `[click]`, `[trash]` selection of a location resource; Green Level Clearance two effects; GRNDL Refinery an advanceable asset whose `HostedAdvancementTokens` is read off its last known state after its own `[trash]`; Project Ares a selection of the Runner's own installs `count`ed past 4 with an `EffectIf` on `CardsSelected`; Strongbox Daniela Jorge Inácio's persistent `StealCost`, a [click]; and TGTBT Nightmare Archive's `RevealedWhileAccessed` with an `OnAccessed` tag. Revised Core Set 32 of 38 built (`CORE2_UNIMPLEMENTED` 15 → 6, the three mechanic stages left). Decks: Stronger Together takes Green Level Clearance, Ash, Strongbox and three Project Ares; Tag You're It Dedicated Response Team; Hostile Bid Elizabeth Mills and GRNDL Refinery; A Thousand Cuts Braintrust; and Broadcast Hour TGTBT — each for cards other decks keep.

**Stage 5 closed** (11 October 2026): a trace's outcomes as moments, and the two cards that need them (17 of 111 `Effect` variants single-use, none unused, over 831 card files; `Trigger` 58 → 59). **Bernice Mai** prints "If unsuccessful", so `Effect::Trace` gains `on_failure`, resolved in `trace::submit_runner_bid` when the Runner's link strength meets the trace strength (CR 10.8.4) and carried through the Flip Switch window whole, since that window now parks the trace effect itself rather than rebuilding it. **Spinal Modem** hears "whenever there is a successful trace during a run": `Trigger::OnTraceSuccessful`, `GameEvent::TraceSuccessful` dispatched through `dispatcher::emit` at CR 10.8.6e, about nothing and heard by both players; "during a run" is `DuringRun`, which ends at access — no built card starts a trace during an access. Its other lines are The Toolbox's words: 2[recurring-credit] paying for icebreakers, +1[mu], and the console limit off its subtype. Revised Core Set 34 of 38 built (`CORE2_UNIMPLEMENTED` 6 → 4). Decks: Broadcast Hour takes two Bernice Mai for its two Sync Rerouting, and Rez Tax two Spinal Modem for The Toolbox and a Cyberfeeder — each kept by other decks. Those decks led the debug build's `every_sample_deck_matchup_finishes` into a planner probe that replays a parked payment through a sequence, about 2 MiB of unoptimised stack — a spawned thread's whole allowance — so `netrunner_session::in_parallel` now gives each sweep thread 16 MiB; release needs a fraction of it.

**Stage 6a closed** (12 October 2026): two prohibitions for the rest of a run, and the two ice that print them (17 of 111 `Effect` variants single-use, none unused, over 833 card files; `Prohibition` 17 → 19). **Hudson 1.0**'s subroutines say "The Runner cannot access more than 1 card during this run": `Prohibition::AccessMoreThanOneCard`, made by `Effect::Prohibit` until the run ends and asked where the candidates are pruned, which empties them once `RunState::cards_accessed_count` reaches 1 (CR 7.4.2b) — the same place Flagship's limit is asked, so HQ Interface's second card is never offered; its "Lose [click]" break is Eli 1.0's. **Whirlpool**'s "The Runner cannot jack out for the remainder of this run" is `Prohibition::JackOut`, asked by the jack-out itself (`RulesError::JackOutProhibited`) and by `Cost::JackOut`, so neither the action nor a card that pays by jacking out is offered; then it trashes itself, as Data Mine does. Revised Core Set 36 of 38 built (`CORE2_UNIMPLEMENTED` 4 → 2: The Cleaners and Pheromones). Decks: Stronger Together takes two Hudson 1.0 for its two Viktor 1.0, and A Thousand Cuts two Whirlpool for its two Attini — each kept by other decks.

**Riskiest:**
- **The Cleaners**: `IncreaseAboutToResolve` handles only a parked draw (Daily Business Show). Making parked damage bigger means changing the prevention window's `WouldHappen::Damage`. CR 9.9.7e's example (Flare with The Cleaners) is the case to test, and its "cannot be prevented" damage must still be made bigger.
- **Hive**: it loses printed subroutines, one per agenda point in the Corp's score area. MS 8a's `ContinuousKind::Subroutines` (the ice's own, counted, down to none) has to accept a negative count that changes during a run.
- **Darwin**: its strength is X, and the catalog records it as −1. `printed_values_agree_with_the_netrunnerdb_catalog` has to accept it the way Blood in the Water's printed X was accepted (MS 7a). The turn-start "you may pay 1" is an offered paid choice.
- **Hudson 1.0**: the bioroid "lose [click]: break" is Eli 1.0's. A "no more than 1 card" limit on the run is not `ContinuousKind::AccessOthersAtMost`, which is standing and about the card that prints it. A run-long version has to go through the access-limit code (`Amount::AccessLimit`, CR 7.4.2).
- **Mr. Li**: "1 of those cards" relies on Daily Business Show's reading that drawn cards sit at the end of the hand (`CardFilter::TopOf` over the hand). That reading has to hold for the Runner's grip, and the grip has to be unordered for the player and ordered for the engine.
- **Dinosaurus**: the breaker has to install onto it (CR 1.13.6a) as Djinn's programs do, with its memory outside the limit and +2 strength through `Scope::Hosted`. The non-AI filter and the console limit are already words.

### F2. Reign and Reverie — 58 cards (C 41 / V 10 / M 7)

**Decks:** seven new identities: Acme Consulting: The Truth You Need, Saraswati Mnemonics: Endless Exploration, Sportsmetal: Go Big or Go Home, The Outfit: Family Owned and Operated (Corp); Akiko Nisei: Head Case, Liza Talking Thunder: Prominent Legislator, Nathaniel "Gnat" Hall: One-of-a-Kind (Runner). One Sweep deck on each. Three cards belong to mini-factions with no identity in the pool: Algernon (Adam, 5 influence), Office Supplies (Sunny Lebeau, 3) and Reboot (Apex, 5). Each one has to go into a Sweep deck as an out-of-faction card, paid for with influence.

**Stages** (from the survey):
1. **Runner programs and events, composed**: Bankroll, Cradle, Tycoon, Ika, Kyuban, Guinea Pig, Hot Pursuit, Office Supplies, Insight, Paragon.
2. **Runner identities and resources, composed**: Akiko Nisei: Head Case, Liza Talking Thunder: Prominent Legislator, Nathaniel "Gnat" Hall: One-of-a-Kind, Algernon, Hijacked Router, Miss Bones, Psych Mike.
3. **Corp ice, composed**: Blockchain, Hydra, Meridian, Peeping Tom, Otoroshi, Thimblerig.
4. **Corp operations, composed**: Attitude Adjustment, Building Blocks, Divert Power, Game Changer, Hangeki, Too Big to Fail, Under the Bus.
5. **Corp agendas, assets, upgrades and identities, composed**: API-S Keeper Isobel, Arella Salvatore, Broad Daylight, Fly on the Wall, Giordano Memorial Field, Hyperloop Extension, Jumon, Neurostasis, SIU, Sportsmetal: Go Big or Go Home, The Outfit: Family Owned and Operated.
6. **Runner words: a faction filter, a trash cost read, and a tag avoided**: District 99, Mâché, Thunder Art Gallery.
7. **A breach that leaves out the root**: Mind's Eye, Divide and Conquer.
8. **Corp words: rezzed this turn, a card's agenda points, an exact agenda points filter, a remembered install, and an install bound to one remote**: Gatekeeper, Drudge Work, Lady Liberty, Saraswati Mnemonics: Endless Exploration, Fast Break.
9. **Condition counters**: Eavesdrop.
10. **A tag the Runner is considered to have**: Acme Consulting: The Truth You Need.
11. **Ice that moves into the run and is encountered**: Formicary.
12. **A root swap that hands the Runner a jack-out**: Daruma.
13. **An interrupt on paying to play or install**: Patchwork.
14. **Runner facedown installs**: Reboot.
15. **An identity hosted from outside the game**: DJ Fenris.

**Riskiest:**
- **Formicary**: it rezzes on any server's approach and moves from another server to the innermost position of the attacked one. The Runner then moves back onto it and encounters it (CR 6.2.2b, 6.2.8). The run engine's positions have to treat this as a move, not a swap, after every piece of ice has been passed.
- **Acme Consulting**: a tag that is not on the Runner but counts during one kind of encounter (CR 1.9.4). Every read of `RunnerTags` and `IsTagged` would have to ask for it, the way link is asked rather than stored. The view and the bots' samples have to see it too.
- **Patchwork**: an interrupt on a play or install that turns a grip card into a 2 [credit] discount. It sits inside the payment that `payment::plan` replays, so it has to be one more question asked by the replay, not a window.
- **Reboot**: facedown Runner installs (CR 8.1.4a–f) are blank cards. Masking, memory, uniqueness and every filter would have to read nothing off them, and the heap installs need to be selected while hidden from the Corp.
- **DJ Fenris**: it needs a pool of identities outside the deck (CR 1.5.4) and a resource that gains an identity's text. Neither exists, and the deck format has nowhere to list them.
- **Divide and Conquer**: three breaches in a row, two of them after the run's own and without their roots. Every "when you breach" in the pool is heard at the run's success (Known limits: Docklands Pass, Manuel Lattes de Moura), so the extra breaches would go unheard by those cards.

### F3. Kitara — 120 cards (C 80 / V 33 / M 7)

Sets: Sovereign Sight, Down the White Nile, Council of the Crest, The Devil
and the Dragon, Whispers in Nalubaale, Kampala Ascendent. Read against the
DSL as it stands on 11 October 2026 (Downfall closed, FFG Stage 0b
closed). Most of the cycle composes because the NSG pool has already built
its era's mechanics: advanceable assets and ice (`advancement_requirement:
0`), click-breakable bioroids (`click_breakable`), psi, arrange, "as an
agenda", ambush upgrades revealed in R&D (`RevealedWhileAccessed`), access
replacements, Trojans, persistent, steal costs, `LaterThisTurn`, a
remembered card type, and `installs_faceup`.

**Decks:** seven new identities, a Sweep deck each, built in the stage
that builds the identity: 419: Amoral Scammer (Criminal, Stage 6),
Freedom Khumalo (Anarch, Stage 11), Kabonesa Wu (Shaper, Stage 12), Asa
Group, Azmari EdTech and SSO Industries (Haas-Bioroid, NBN, Weyland,
Stage 14), Mti Mwekundu (Jinteki, Stage 16). The mini-faction cards
(Black Hat and White Hat, Sunny Lebeau's; Emergent Creativity and Logic
Bomb, Adam's; Assimilator and Consume, Apex's) have no identity here and
go in a Sweep deck by influence. Their identities arrive with F6.

**Stages** (from the survey):
1. **Ice that composes**: Aimor, Envelope, Endless EULA, Mlinzi, Najja
   1.0, Kamali 1.0, Sandman, Nightdancer, Anansi, Sadaka.
2. **Counting ice and ambush upgrades**: Mganga, Masvingo, Oduduwa, NEXT
   Diamond, NEXT Sapphire, Forced Connection, Intake, Tempus.
3. **Advanceable cards and "as an agenda"**: Bio Vault, Calibration
   Testing, NGO Front, Gene Splicer, False Flag, Echo Chamber, City Works
   Project, Reconstruction Contract, Urban Renewal, Overseer Matrix.
4. **Corp agendas and assets**: Armed Intimidation, Bacterial
   Programming, Better Citizen Program, Ikawah Project, Remote
   Enforcement, SSL Endorsement, Viral Weaponization, Personalized Portal,
   Rashida Jaheem, TechnoCo, Malia Z0L0K4.
5. **Corp operations**: Distract the Masses, Genotyping, Economic
   Warfare, High-Profile Target, Market Forces, Riot Suppression,
   Self-Growth Program, Standard Procedure, Threat Assessment, Wake Up
   Call.
6. **Runner events, and 419's deck**: 419: Amoral Scammer, Because I Can,
   By Any Means, Contaminate, Credit Kiting, Diversion of Funds, Embezzle,
   Falsified Credentials, Glut Cipher, Wari.
7. **Breakers and break triggers**: Amina, Cyberdelia, Engolo, Laamb,
   Takobi, Kongamato, Logic Bomb, Trypano.
8. **Virus and counter programs**: Consume, Crypt, Friday Chip, eXer,
   Nyashia, Upya, Zer0.
9. **Runner economy**: Lewi Guilherme, PAD Tap, Rogue Trading, Reclaim,
   Jackpot!, Kasi String.
10. **Trace words**: a base that is an `Amount`, an "if unsuccessful"
    branch, the margin as an amount, and a trace a Runner card forces:
    Amani Senai, Kuwinda K4H1U3, Surveyor, Trojan Horse, Black Hat, White
    Hat, No One Home.
11. **Virus counters from any card, purged counters, memory**: Musaazi,
    Yusuf, Freedom Khumalo: Crypto-Anarchist, Puffer, Knobkierie, Acacia,
    Reverse Infection.
12. **A card or choice remembered past its resolution**: Compile,
    Kabonesa Wu: Netspace Thrillseeker, Marathon, Flame-out, Plague, RNG
    Key.
13. **Breaker and Runner words**: Gbahali, Hippo, Gebrselassie, Zamba,
    Emergent Creativity.
14. **Corp server, install and draw words, and three Corp decks**: Jinja
    City Grid, Asa Group: Security Through Vigilance, Mwanza City Grid,
    SSO Industries: Fueling Innovation, Warden Fatuma, Azmari EdTech:
    Shaping the Future, Jua, Degree Mill.
15. **Currents** (CR 3.5.1b, 3.7.1b, 8.6.6c): Corporate "Grant", Death and
    Taxes, Kill Switch.
16. **Moving the Runner to a position other than the outermost**, and Mti
    Mwekundu's deck: Code Replicator, Mti Mwekundu: Life Improved,
    Slipstream.
17. **Facedown installed Runner cards** (CR 8.1.4): Assimilator. Nothing
    in Kitara installs a Runner card facedown, so this can wait for Apex
    (F6) and be built with it.

**Riskiest:**
- Mti Mwekundu: ice installed innermost as the Runner approaches the
  server, and the Runner moved back out to approach it (CR 6.2.2b,
  6.2.6b). The run's position and ice list change during a server
  approach.
- Code Replicator: the Runner approaches the ice they just passed without
  moving (CR 6.1.3b, the only card that does this).
- Currents: they stay in play until another current is played or an
  agenda is scored or stolen. The Corp's play area exists (lockdown), but
  the Runner has none, and the trash condition is something new.
- No One Home: a trace started inside the prevention window, which
  `rules::prevention` nests, with its prevention depending on the trace
  failing.
- Black Hat and White Hat: a trace the Corp makes because a Runner card
  says so. Every trace so far starts from a Corp card.
- Assimilator: facedown rig cards have no characteristics (CR 8.1.4a),
  which touches masking, the view and every scan of the rig.

### F4. Red Sand — 120 cards (C 63 / V 48 / M 9)

**Decks:** four new identities — AgInfusion: New Miracles for a New World
(Jinteki), Jemison Astronautics: Sacrifice. Audacity. Success. (Weyland),
Alice Merchant: Clan Agitator (Anarch) and Los: Data Hijacker (Criminal).
One Sweep deck per identity: Alice Merchant's and Los's at Stage 5 (both
compose), Jemison's at Stage 9 (the forfeit amounts), AgInfusion's at
Stage 10 (moving the run to another server).

**Stages** (from the survey):
1. **Composes: Corp ice**: Battlement, Authenticator, Kakugo, Seidr
   Adaptive Barrier, Self-Adapting Code Wall, Conundrum, Data Loop, Free
   Lunch, Owl, Watchtower.
2. **Composes: bioroids, NEXT, break and rez triggers, traces**: NEXT Opal,
   NEXT Wave 2, Nerine 2.0, Synth DNA Modification, Henry Phillips, Fractal
   Threat Matrix, Traffic Analyzer, Warroid Tracker, Threat Level Alpha, CFC
   Excavation Contract.
3. **Composes: Corp operations and advancement**: Audacity, Mass
   Commercialization, Priority Construction, Replanting, Shipment from
   Tennin, Oberth Protocol, Load Testing, MCA Austerity Policy, O₂ Shortage,
   Meteor Mining, Biased Reporting.
4. **Composes: Corp agendas, assets and upgrades**: AR-Enhanced Security,
   Escalate Vitriol, Obokata Protocol, Water Monopoly, Ben Musashi, Bamboo
   Dome, Breached Dome, Clyde Van Rite, Defense Construct, Whampoa
   Reclamation.
5. **Composes: Runner identities and events**, with Alice Merchant's and
   Los's Sweep decks: Alice Merchant: Clan Agitator, Los: Data Hijacker,
   Build Script, Spot the Prey, Exploit, Mining Accident, Mad Dash, Möbius,
   Mars for Martians, Reshape.
6. **Composes: Runner programs**: Berserker, Flashbang, God of War, Maven,
   Customized Secretary, Keros Mcintyre.
7. **Composes: Runner resources and interrupts**: Bloo Moose, Dadiana
   Chacon, Bug Out Bag, Clan Vengeance, Caldera, Dummy Box.
8. **Moments of access, draw and basic action** (a trigger for an access
   that ended with the card neither stolen nor trashed, for a breach
   finished, for the mandatory draw done, for the Corp's basic draw and the
   basic click-for-credit; access counts for a chosen number and "1
   fewer"): Aeneas Informant, Maw, Aumakua, Rip Deal, Counter Surveillance,
   SYNC BRE, Severnius Stim Implant, Corporate Defector, Open Forum, CPC
   Generator.
9. **The forfeited agenda as an amount**, with Jemison's Sweep deck (its
   points and advancement requirement read after `Cost::Forfeit` paid,
   `OnForfeit` heard off the agenda, "repeat this process"): Jemison
   Astronautics: Sacrifice. Audacity. Success., Sacrifice, Quarantine
   System, Success, Red Planet Couriers, Tithonium, Standoff, Reeducation.
10. **Ice and run positions**, with AgInfusion's Sweep deck (the run moved
    to a chosen server, ice moved or swapped between servers, an install
    directly inward, a cost by position, a pass replaced): AgInfusion: New
    Miracles for a New World, Sand Storm, Mirāju, Bloom, Mandatory Seed
    Replacement, Metamorph, Network Exchange, Inversificator, Persephone.
11. **Run-scoped strength, filters and prohibitions** (every icebreaker
    for the run, ice protecting this server, ice protecting the attacked
    server, ice rezzed this run or this turn, cards installed by this card,
    a bar on installing, a click lost to break): Lean and Mean, Pushing the
    Envelope, Helheim Servers, Na'Not'K, Daredevil, Leave No Trace, Rubicon
    Switch, Diana's Hunt, Signal Jamming, Zed 2.0.
12. **Prevention, timing windows, recurring X, costs**: Bio-Modeled
    Network, Jarogniew Mercs, Net Analytics, Bioroid Work Crew, Salvaged
    Vanadis Armory, Respirocytes, Khondi Plaza, Māui, Rosetta 2.0, Restore,
    The Archivist.
13. **Granted abilities and subroutines that change** (M): Adjusted Matrix,
    Dedicated Processor, Loki, Mass-Driver.
14. **Condition counters and currents** (M): MCA Informant, Rover
    Algorithm, Transparency Initiative, Rolling Brownout, System Seizure.

**Riskiest:**
- Loki: for the run it takes a chosen ice's subtypes and its subroutines,
  ahead of its own (CR 9.8.3a). Nothing copies another card's subroutine
  list yet, and the copied subroutines have to be broken and resolved as
  Loki's.
- Transparency Initiative: an operation that becomes a condition counter
  (CR 1.13.6d) on an installed agenda it turns faceup. The agenda then
  gains public and an advance trigger. This is the first condition counter
  and the first agenda turned faceup in a server.
- MCA Informant: a Corp condition counter hosted on a Runner resource. It
  adds a tag the Runner does not hold, gives the Corp a click ability on
  the Runner's card, and ends the action phase (Terminal).
- System Seizure: a Runner current (CR 3.7.1b) whose interrupt fires on a
  strength increase *about to happen*. Nothing announces that moment yet.
- Mandatory Seed Replacement and Metamorph: ice rearranged across servers,
  as repeated swaps (CR 8.3.2), and two non-ice installs swapped between
  servers (CR 8.8.3), which must respect the one-region limit (CR 3.6.5e).
- Mass-Driver: the first 3 subroutines of the next encounter do not
  resolve (CR 6.9.3c). That is a lingering effect on an encounter not yet
  begun.

### F5. Flashpoint, and Terminal Directive Cards — 168 cards (C 108 / V 41 / M 19)

**Decks:** nine new identities, one Sweep deck each, as the NSG plan did. Runner: Ele "Smoke" Scovak: Cynosure of the Net (Shaper; the stealth breakers Blackstone, Fawkes, Houdini, Dai V and Net Mercur want her), Khan: Savvy Skiptracer (Criminal), Null: Whistleblower and Omar Keung: Conspiracy Theorist (Anarch). Corp: Jinteki: Potential Unleashed, NBN: Controlling the Message, Seidr Laboratories: Destiny Defined, Skorpios Defense Systems: Persuasive Power and Weyland Consortium: Builder of Nations. Three cards are mini-faction cards whose identities are not in the pool — Another Day, Another Paycheck (Sunny Lebeau), Find the Truth (Adam), Reaver (Apex) — and are splashed into a Sweep deck on influence. The currents need a deck on each side that plays two of them, so a current is seen trashed by another.

**Stages** (from the survey):
1. **Runner economy and draw, composed**: Aaron Marrón, Algo Trading, Beth Kilrain-Chang, Blockade Runner, Laguna Velasco District, LLDS Memory Diamond, Ubax, Process Automation, Peace in Our Time, Temüjin Contract.
2. **Runner tags, damage and the heap, composed**: Biometric Spoofing, Misdirection, First Responders, Officer Frank, Reaver, Tapwrm, Obelus, Equivocation, Frantic Coding, Levy Advanced Research Lab.
3. **Runner breakers and runs, composed**: Black Orchestra, MKUltra, Paperclip, Demara, Lustig, Nfr, Sūnya, Dai V, Mammon, Dhegdheer.
4. **Runner identities and the rest, composed**: Ele "Smoke" Scovak: Cynosure of the Net, Khan: Savvy Skiptracer, Null: Whistleblower, Omar Keung: Conspiracy Theorist, Data Breach, Dean Lister, Deuces Wild, GPI Net Tap, Careful Planning, SYN Attack.
5. **Corp ice, composed**: Aiki, Bloodletter, Bulwark, Chiyashi, Data Ward, DNA Tracker, IP Block, Tapestry, Tribunal, Weir, Veritas.
6. **Corp ice, composed (bioroids and the rest)**: Eli 2.0, Fairchild, Fairchild 1.0, Fairchild 2.0, Fairchild 3.0, Sherlock 2.0, Executive Functioning, Holmegaard, Macrophage, Hailstorm, Mausolus, Thoth.
7. **Corp operations, composed**: Ark Lockdown, Best Defense, BOOM!, Financial Collapse, Friends in High Places, Hard-Hitting News, Hatchet Job, Hunter Seeker, IPO, Liquidation, Observe and Destroy.
8. **Corp operations and agendas, composed**: Preemptive Action, Psychokinesis, Special Report, Stock Buy-Back, Ultraviolet Clearance, Violet Level Clearance, Brain Rewiring, Crisis Management, Elective Upgrade, Graft, Illicit Sales.
9. **Corp agendas, assets and upgrades, composed**: Paper Trail, Project Kusanagi, Show of Force, Sensor Net Activation, Anson Rose, C.I. Fund, Estelle Moon, Illegal Arms Factory, Long-Term Investment, Mr. Stone, NASX.
10. **Corp assets, upgrades and identities, composed**: Honeyfarm, Prisec, Sandburg, Watchdog, Bryan Stinson, Black Level Clearance, Drone Screen, Georgia Emelyov, Manta Grid, Mason Bellamy, Jinteki: Potential Unleashed, NBN: Controlling the Message.
11. **Unused memory as an amount**: Adept, Savant, Deep Data Mining.
12. **Stealth payments and self-bouncing breakers**: Blackstone, Fawkes, Houdini, Net Mercur, Golden, Peregrine, Saker.
13. **Traces and bids heard**: Citadel Sanctuary, Hellion Beta Test, Net Quarantine, Fumiko Yamamori, Hyoubu Research Facility, Enforcing Loyalty.
14. **Run moments: all the ice passed, broken, clicks**: K. P. Lynn, Polyhistor, The Gauntlet, Chief Slee, Seidr Laboratories: Destiny Defined, Weyland Consortium: Builder of Nations, Şifr, Spear Phishing, Charlatan, Injection Attack.
15. **Accesses, payments and cards in odd places**: Top Hat, Credit Crash, Recon Drone, Tracker, Cold Read, Maxwell James, Mirror, Find the Truth, The Shadow Net, Brute-Force-Hack.
16. **Corp words: costs, swaps and installs**: Hernando Cortez, Hasty Relocation, Nihongai Grid, Alexa Belsky, Successful Field Test.
17. **Currents**: Enforced Curfew, Scarcity of Resources, Service Outage, Door to Door, Interdiction, Rumor Mill, System Outage, Government Investigations, Another Day, Another Paycheck.
18. **Condition counters and encountering accessed ice**: On the Lam, Wetwork Refit, Chrysalis, Herald, Sapper.
19. **Jack-out costs, a gained paid ability, a replaced trash and an extra turn**: Armored Servers, Mind Game, Baba Yaga, Skorpios Defense Systems: Persuasive Power, Encore.

**Riskiest:**
- The nine currents (Enforced Curfew, Rumor Mill, System Outage and the rest): a card that stays in the play area on either side until another current is played or an agenda is scored or stolen (CR 3.5.1b, CR 3.7.1b, CR 8.6.6c). Lockdown's `CorpState::play_area` is the base, but the Runner has no play area that persists, and the trash is a checkpoint condition.
- Encore: an additional turn. The turn log's `rotate`, `last_turn`, the Session's alternation and every "during their last turn" read assume the sides alternate; no rule in the committed CR names an extra turn.
- Chrysalis, Herald, Sapper: an encounter with ice that is accessed in HQ or R&D and not installed (CR 9.1.8h). The encounter state keys on an install handle; Ganked!'s suspended run (UR 7b) is the nearest thing.
- Skorpios Defense Systems: an interrupt on every Runner trash from any zone — damage, a mill, a cost, an install trashed by a card — set aside as a batch. Every trash site would have to park; the Prevention Rule's "a cost is not prevented" does not cover a replacement.
- Baba Yaga: a program that gains the paid abilities of programs it hosts (CR 9.1.9). Ability lookup is by the card that prints it; a breaker's interface, strength pumps and stealth costs would be borrowed by another card with its own strength.
- Armored Servers and Mind Game: additional costs on jacking out and on breaking a subroutine, both free or paid-ability timing points the payment layer has never priced.

### F6. Mumbad, and Data and Destiny — 167 cards (C 87 / V 55 / M 25)

Surveyed 11 October 2026 against the DSL as it stands after the reprint
packs' tranche (Core Set gate closed). Every card is an FFG design; none is
in Standard.

**Decks:** ten new identities. Runner: Jesminder Sareen and Nero Severn
(Stage 4), Sunny Lebeau (Stage 4, no text: a link-2 deck for the GS cloud
breakers and Security Chip), Apex (Stage 17, needs facedown installs) and
Adam (Stage 19, needs his directives — Always Be Running, Neutralize All
Threats and Safety First are built in Stages 12, 15 and 13 first). Corp:
Spark Agency (Stage 9), Pālanā Foods (Stage 13), SYNC: Everything,
Everywhere (Stage 15; the flip is `FlipIdentity`), New Angeles Sol (Stage
16, with the currents) and Harishchandra Ent. (Stage 21). One Sweep deck per
new identity, as the NSG plan did. The three mini-factions are already in
`Faction` (`Adam`, `Apex`, `SunnyLebeau`).

**What the survey found already there:** psi (`PsiGame`, Konjin), traces
(`Trace`, Scapenet), Starlit Knight's counted `GainSubroutine` (Brainstorm),
Nightmare Archive's "into the Runner's score area as an agenda" (News Team,
Victoria Jenkins), Oracle Thinktank's ability used from the Runner's score
area (15 Minutes), Security Testing's chosen-server replacement (Patron),
Sipa's pass-and-swap (Mumbad City Grid), Mitosis's per-install prohibition
(Councilman, Dedication Ceremony, PAD Factory), Wage Workers' same-action
count (Jeeves Model Bioroids), Ganked!'s encounter from an access
(Archangel), Klevetnik's lost abilities (Dr. Lovegood), arrange (Spy Camera,
Making an Entrance) and AirbladeX's encounter-ability prevention (Hunting
Grounds' first half). That is why the composed share is over half.

**The alliance cards** (twelve, Stage 10 plus Mumbad Virtual Tour and Raman
Rai) print a deck-building waiver — "costs 0 influence if …" — that is a
deck-legality word (CR 1.4.5) and nothing in play; it is built once, in
Stage 10, and each card whose play text composes is counted V for it alone.

**Stages** (from the survey):

1. **Runner economy and connections, composed** (10): Akshara Sareen, Bhagat, Councilman, Dr. Lovegood, Emptied Mind, Populist Rally, Sports Hopper, Technical Writer, Temple of the Liberated Mind, Windfall.
2. **Runner rig, composed** (10): Ankusa, Brahman, Brain Chip, Heartbeat, Multithreader, NetChip, Security Chip, Globalsec Security Clearance, Bazaar, Chatterjee University.
3. **Runner runs and events, composed** (10): CBI Raid, Fear the Masses, High-Stakes Job, Exclusive Party, Making an Entrance, Out of the Ashes, Patron, The Turning Wheel, Spy Camera, Independent Thinking.
4. **Runner identities and the rest, composed** (7): Artist Colony, Jesminder Sareen: Girl Behind the Curtain, Liberated Chela, Nero Severn: Information Broker, Political Operative, Wasteland, Sunny Lebeau: Security Specialist.
5. **Corp ice, composed** (11): Assassin, Bailiff, Brainstorm, Chetana, Cobra, Red Tape, Resistor, Special Offer, Turnpike, Upayoga, Vanilla.
6. **Corp agendas, composed** (10): 15 Minutes, Advanced Concept Hopper, Corporate Sales Team, Global Food Initiative, Improved Protein Source, Merger, New Construction, Puppet Master, Remote Data Farm, Voting Machine Initiative.
7. **Corp turn-start assets, composed** (10): Bio-Ethics Association, Clone Suffrage Movement, Commercial Bankers Group, Sensie Actors Union, Launch Campaign, Reality Threedee, Pālanā Agroplex, Kala Ghoda Real TV, Mumbad Construction Co., Advanced Assembly Lines.
8. **Corp operations, composed** (10): Ad Blitz, Dedication Ceremony, Election Day, Lateral Growth, Localized Product Line, Subcontract, The All-Seeing I, Voter Intimidation, Mumbad City Hall, Lily Lockwell.
9. **Corp upgrades, ambushes and Spark Agency, composed** (9): Archangel, Disposable HQ, Keegan Lane, Mumbad City Grid, Surat City Grid, News Team, Victoria Jenkins, Zealous Judge, Spark Agency: Worldswide Reach.
10. **Alliance influence, a deck-legality word** (10): Consulting Visit, Executive Search Firm, Heritage Committee, Ibrahim Salem, Jeeves Model Bioroids, Museum of History, PAD Factory, Product Recall, Salem's Hospitality, Mumba Temple.
11. **Trace words** (5): Aryabhata Tech, Improved Tracers, Rutherford Grid, Waiver, Security Nexus.
12. **Prohibitions and imposed costs** (10): Vikram 1.0, Jak Sinclair, Navi Mumbai City Grid, EMP Device, The Price of Freedom, The Black File, Always Be Running, Lakshmi Smartfabrics, Interrupt 0, Diwan.
13. **Draw, search, hand-size and prevention moments** (10): Pālanā Foods: Sustainable Growth, Political Dealings, Personality Profiles, Shannon Claire, Harvester, Safety First, Reflection, Tech Trader, Guru Davinder, Maya.
14. **Breaker and ice words** (10): GS Sherman M3, GS Shrike M2, GS Striker M1, Mongoose, Endless Hunger, Panchatantra, TL;DR, Rebranding Team, Indian Union Stock Exchange, Run Amok.
15. **Access, run and basic-action words** (10): Neutralize All Threats, Mumbad Virtual Tour, Salsette Slums, Quantum Predictive Model, Exchange of Information, Prey, The Noble Path, Ramujan-reliant 550 BMI, Rigged Results, SYNC: Everything, Everywhere.
16. **Currents (machinery)** (8): "Clones are not People", Corporate Scandal, Employee Strike, "Freedom Through Equality", Surveillance Sweep, News Hound, New Angeles Sol: Your News, Media Blitz.
17. **Facedown Runner cards (machinery)** (4): Apex: Invasive Predator, Harbinger, Hunting Grounds, Apocalypse.
18. **Devas (machinery)** (3): Aghora, Sadyojata, Vamadeva.
19. **Identities from outside the deck (machinery)** (2): Adam: Compulsive Hacker, Rebirth.
20. **Who chooses an access, and the order of subroutines (machinery)** (3): Dedicated Neural Net, Information Sifting, Street Magic.
21. **Single-card machinery** (5): 24/7 News Cycle, Political Graffiti, Full Immersion RecStudio, Harishchandra Ent.: Where You're the Star, Raman Rai.

**Riskiest:**
- **Currents** (Stage 16, eight cards): a played card that stays active in
  the play area until another current is played or an agenda is scored or
  stolen (CR 3.5.1b, 3.7.1b, 8.6.6c), and News Hound reads whether one is.
  Lockdown's play area (UR 7a) is the near relative, but a current's end is
  an event, not a turn. Media Blitz on top of it gains another card's text.
- **Apex and facedown Runner cards** (Stage 17): an installed Runner card
  that is inactive and has no type or name (CR 8.1.4, 1.8.4b) touches every
  scan of the rig, the masking of the Runner's installs in the Corp's view,
  and both clients' rig rows.
- **Adam and Rebirth** (Stage 19): cards that come from outside the deck
  (CR 1.5.3, 1.5.4) — the deck file, the deck builder, deck legality and
  the online deck all have to carry them.
- **Harishchandra Ent.** (Stage 21): a standing exception to the grip's
  secrecy (CR 4.3.2), which is a change to `rules::masking` itself, read on
  every view while the Runner is tagged.
- **Dedicated Neural Net and Information Sifting** (Stage 20): the Corp
  choosing, or splitting, what the Runner accesses from HQ (CR 7.3.4b,
  7.1.9), which the breach's candidate model has never let the Corp touch.
- **Street Magic** (Stage 20): the Runner ordering a piece of ice's
  unbroken subroutines (CR 9.8.7d), where the run resolves them in printed
  order.

### F7. SanSan, and Order and Chaos — 170 cards (C 104 / V 45 / M 21)

**Decks:** thirteen new identities, a Sweep deck each — Runner: Armand "Geist" Walker, Edward Kim, Hayley Kaplan, Laramy Fisk, MaxX, Valencia Estevez; Corp: Argus Security, Chronos Protocol, Cybernetics Division, Gagarin Deep Space, Haarpsichord Studios, Jinteki Biotech, Titan Transnational. Jinteki Biotech's deck brings its other two copies, as Méliès U's does (CR 1.5.2a). Seven of the identities compose and land with their stage (4 and 10); the rest wait for their word or mechanic.

**Read against** the DSL on `main` (11 October 2026), after Downfall and the reprint tranche. Much of what these older cards print is already there: advanceable ice and its rez discount (Ice Wall's counters with `RezCost`), traces, psi games, `RevealedWhileAccessed`, `AddToScoreAreaAsAgenda`, `LaterThisTurn` lasting a run (Forked, Knifed, Spooned, Bandwidth, Checkpoint), F2P's Runner-only break on ice, Archer's forfeit to rez, the region limit, expose and Runner-side hosting. What it lacks clusters in **currents** (six cards) and **Corp-side hosting** (condition counters and two hosts: `InstalledCard` has no hosted list).

**Stages** (from the survey):
1. **Runner events, composes**: Amped Up, Day Job, Fisk Investment Seminar, Power to the People, Uninstall, Drive By, Forked, Knifed, Spooned, Wanton Destruction.
2. **Runner resources, composes**: Beach Party, Drug Dealer, Enhanced Vision, Fan Site, Human First, Investigative Journalism, Symmetrical Visage, Synthetic Blood, Stim Dealer, Wireless Net Pavilion.
3. **Runner hosts, piles and the rest of the rig, composes**: Bookmark, Street Peddler, London Library, Off-Campus Apartment, Rolodex, Sacrificial Clone, Tyson Observatory, Spoilers, Chop Bot 3000.
4. **Runner hardware and four identities, composes**: Brain Cage, Skulljack, Net-Ready Eyes, Forger, Edward Kim: Humanity's Hammer, Laramy Fisk: Savvy Investor, MaxX: Maximum Punk Rock, Valencia Estevez: The Angel of Cayambe.
5. **Runner programs, composes**: Analog Dreamers, DaVinci, Faust, Gravedigger, Hyperdriver, Study Guide, Trope, Virus Breeding Ground.
6. **Corp advanceable and big ice, composes**: Asteroid Belt, Nebula, Wormhole, Orion, Fire Wall, Spiderweb, Meru Mati, Gutenberg, Crick, Quicksand.
7. **Corp ice, composes**: Bandwidth, Checkpoint, Clairvoyant Monitor, Enforcer 1.0, Lab Dog, Little Engine, Negotiator, NEXT Gold, Pachinko, Tour Guide, Turing.
8. **Corp agendas, composes**: Award Bait, Explode-a-palooza, Firmware Updates, Genetic Resequencing, Government Takeover, High-Risk Investment, Hollywood Renovation, Research Grant, Self-Destruct Chips, The Future is Now, Underway Renovation, Vanity Project.
9. **Corp assets and operations, composes**, in two: **9a**, the advancement tokens — Allele Repression, Back Channels, Capital Investors, Constellation Protocol, Contract Killer, Early Premiere, Exposé, Test Ground; **9b**, the rest — Cybernetics Court, Public Support, Ronald Five, Team Sponsorship, Tech Startup, Recruiting Trip, Traffic Accident.
10. **Corp upgrades and three identities, composes**: Breaker Bay Grid, Expo Grid, Oaktown Grid, Cyberdex Virus Suite, Product Placement, Space Camp, Marcus Batty, Ryon Knight, Argus Security: Protection Guaranteed, Cybernetics Division: Humanity Upgraded, Titan Transnational: Investing In Your Future.
11. **Trigger words**: Adjusted Chronotype, Ancestral Imager, Armand "Geist" Walker: Tech Lord, It's a Trap!, I've Had Worse, Mark Yale, Chrome Parlor, Muertos Gang Member.
12. **Filter, scope and payment words**: Hayley Kaplan: Universal Scholar, Paige Piper, The Twins, Student Loans, Franchise City, The Board, Dedicated Technician Team.
13. **Amount words**: Brain-Taping Warehouse, Cortex Lock, Data Folding, Game Day, Vigil, Searchlight, Satellite Grid.
14. **Prohibition words**: DDoS, Eater, Haarpsichord Studios: Entertainment Unleashed, Lockdown, Genetics Pavilion, Blacklist, Underway Grid, Old Hollywood Grid.
15. **Prevention and duration words**: Valley Grid, Dorm Computer, Qianju PT, Progenitor.
16. **Cost, play and setup words**: Crowbar, Shiv, Spike, Gagarin Deep Space: Expanding the Horizon, Comet, Jinteki Biotech: Life Imagined.
17. **Breach and run words**: Showing Off, Gang Sign, Builder, Surfer, Film Critic.
18. **Currents**: Defective Brainchips, Hacktivist Meeting, Housekeeping, Itinerant Protesters, Predictive Algorithm, Traffic Jam.
19. **Condition counters and Corp hosts**: Casting Call, Patch, Sub Boost, Glenn Station, Worlds Plaza.
20. **Replacements of an access and of damage**: Archives Interface, Immolation Script, Chronos Protocol: Selective Mind-mapping, Titanium Ribs.
21. **One-card mechanics**: Gene Conditioning Shoppe, Hivemind, MemStrips, Paparazzi, Turntable, An Offer You Can’t Refuse.

**Riskiest:**
- The six currents: a played card that stays in the play area across turns and is trashed at a checkpoint when another current is played or an agenda is scored or stolen (CR 3.5.1b, 3.7.1b, 8.6.6c). Nothing in the pool stays there longer than its own resolution (Déjà Vu's wait is the nearest).
- Casting Call, Patch, Sub Boost: a card becomes a condition counter hosted on a Corp card (CR 10.1.4, 1.9.5j), and Corp installs host nothing today; Glenn Station and Worlds Plaza need the same hosting, one of them of installed, rezzed assets.
- Hivemind: counters on one card read and spent as if on every other virus program, so every `HostedCounters` and counter cost on a virus has to ask it.
- MemStrips: restricted [mu] turns the memory check into a mapping of each program's cost to a source (CR 1.20.4b), where `memory::memory_balance` is a sum.
- Gene Conditioning Shoppe: a "first time each turn" met a second time, against the turn log's `first_each_turn` verdict, for the four genetics in this tranche (Adjusted Chronotype, Enhanced Vision, Symmetrical Visage, Synthetic Blood).
- An Offer You Can't Refuse: a run the Runner may make during the Corp's turn, with no jack-out, and a fallback that scores the operation.

### F8. Lunar, and Honor and Profit — 156 cards (C 82 / V 59 / M 15)

**Decks:** ten new identities. Corp: Blue Sun: Powering the Future (Weyland), The Foundry: Refining the Process (Haas-Bioroid), and the four Jinteki divisions of Honor and Profit, Harmony Medtech: Biomedical Pioneer, Industrial Genomics: Growing Solutions, Nisei Division: The Next Generation and Tennin Institute: The Secrets Within. Runner: Iain Stirling: Retired Spook, Leela Patel: Trained Pragmatist, Silhouette: Stealth Operative (Criminal) and Nasir Meidan: Cyber Explorer (Shaper). One Sweep deck per identity, built in the stage that builds the identity: Iain Stirling, Leela Patel and Silhouette at Stage 3; Blue Sun, Harmony Medtech and Tennin Institute at Stage 6; Industrial Genomics at Stage 8a; Nisei Division at Stage 14; The Foundry and Nasir Meidan at Stage 16. Nisei Division's deck should carry the set's psi cards (Mamba, Cerebral Cast, Psychic Field, The Future Perfect), so its trigger is reached in the sweeps.

**Stages** (from the survey):
1. **Runner programs, composed**: Cerberus "Cuj.0" H3, Cerberus "Rex" H2, D4v1d, Refractor, Switchblade, Ghost Runner, Collective Consciousness, Lamprey, Fester, Incubator, Leprechaun.
2. **Runner hardware and resources, composed**: Box-E, Astrolabe, Logos, Origami, Q-Coherence Chip, Cybsoft MacroDrive, Public Terminal, Rachel Beckman, Tri-maf Contact, Zona Sul Shipping.
3. **Runner rig and identities, composed**: Unregistered S&W '35, The Supplier, LLDS Energy Regulator, Oracle May, Duggar's, Leela Patel: Trained Pragmatist, Iain Stirling: Retired Spook, Silhouette: Stealth Operative.
4. **Runner events, composed**: Calling in Favors, Early Bird, Express Delivery, Paper Tripping, Three Steps Ahead, Mass Install, Trade-In, Inject, Code Siphon, Feint, Cyber Threat.
5. **Corp ice, composed**: Ashigaru, Errand Boy, Information Overload, IQ, Komainu, Mamba, Markus 1.0, Pup, Troll, Universal Connectivity Fee.
6. **Corp agendas and identities, composed**: Blue Sun: Powering the Future, Harmony Medtech: Biomedical Pioneer, Tennin Institute: The Secrets Within, Chronos Project, Domestic Sleepers, Encrypted Portals, Superior Cyberwalls, Hades Fragment, Helium-3 Deposit, Philotic Entanglement, Utopia Fragment.
7. **Corp operations, composed**: Cerebral Cast, Diversified Portfolio, Fast Track, Medical Research Fundraiser, Peak Efficiency, Reuse, Shoot the Moon, Snatch and Grab.
8. **Corp assets and upgrades, composed**, in two halves:
   - 8a: Chairman Hiro, Industrial Genomics: Growing Solutions, Mental Health Clinic, Primary Transmission Dish, Sealed Vault, Shell Corporation, Eliza's Toybox.
   - 8b: IT Department, NeoTokyo Grid, Shattered Remains, Tenma Line, Tori Hanzō.
9. **Breaker words** (a filter for ice protecting a remote, a break by subtype, "all but 1", an unused-MU amount, credits paid from a stealth card): Alias, Breach, Passport, Gingerbread, Grappling Hook, Overmind, Sage, BlacKat.
10. **Jack-out words** (a trigger on jacking out, a cost to jack out, a jack-out prohibition and its prevention): Au Revoir, Port Anson Grid, Labyrinthine Servers, Inazuma, Susanoo-no-Mikoto.
11. **Trace words** (the trace strength reached, a base that is an amount, a trace initiated): Gemini, Sagittarius, Taurus, Virgo, Self-destruct, Power Tap.
12. **Ice type and protection words** (more than one printed type, a type lost, cannot be bypassed, cannot be trashed): Changeling, Lycan, Wendigo, Rainbow, Guard, Architect.
13. **Subroutines and abilities borrowed, and a breach without the root**: Galahad, Lancelot, Merlin, Bifrost Array, Shiro.
14. **Psi and expose words** (a bid revealed, an expose of this card, a steal barred for one access): Psychic Field, Nisei Division: The Next Generation, The Future Perfect.
15. **Limits, scopes and payment words** (a limit set to an amount, a lingering memory change, a scope over the other player's installs and plays, a break surcharge, an install cost ignored, `PaysFor` advancing): Ekomind, Theophilius Bagbiter, Bad Times, Medical Breakthrough, Docklands Crackdown, Donut Taganes, Midway Station Grid, Eden Fragment, The Root, Order of Sol.
16. **Trigger and filter words** (an install from the grip, the Corp's drawn card, the Corp losing credits, a click spent in a run, ice rezzed during the approach, a breaker that broke this run, another copy of the triggering card, a remembered ice's rez, a draw from the bottom): Autoscripter, Bug, Ixodidae, Heinlein Grid, Nasir Meidan: Cyber Explorer, Will-o'-the-Wisp, The Foundry: Refining the Process, Social Engineering, Window.
17. **Play, install and score words** (a per-card bar on score and rez, an install in a trashed ice's position, an advancement-requirement filter, a negative worth, a Runner event played from the stack, an install cost of X, a rez surcharge on one ice): Mushin No Shin, Mutate, Plan B, Shi.Kyū, Planned Assault, Angel Arena, Bribery.
18. **Currents** (CR 3.5.1b, 3.7.1b, 8.6.6c), in two halves:
   - 18a, the Corp's: Lag Time, Paywall Implementation, Manhunt, Cerebral Static, Enhanced Login Protocol, Targeted Marketing, and The News Now Hour (composes, but bars only what this stage builds).
   - 18b, the Runner's, which needs a Runner play area: Net Celebrity, Scrubbed, Unscheduled Maintenance.
19. **Shards**, installed in place of a breach from the grip: Eden Shard, Hades Shard, Utopia Shard.
20. **One-card mechanics**: Kitsune, Mother Goddess, Push Your Luck.

**Riskiest:**
- Currents (nine cards): the play area exists for the Corp only (lockdown, UR 7a). A current stays until another current is played or an agenda is stolen (the Corp's) or scored (the Runner's), and it is trashed at the next checkpoint (CR 8.6.6c). The Runner needs a play area and both clients need to draw one.
- The Shards: a trigger heard while the card is in the grip, which replaces the breach with an install. No trigger is heard from the hand today (`from_discard` and `from_runner_score_area` are the only out-of-play listeners), and the replacement has to sit beside `SetAccessReplacement` and other cards' replacements of the same breach.
- Mother Goddess: it gains every subtype of every other rezzed ice, bioroid and tracer as well as the three ice types. `GainSubtype` holds only an `IceType`, and CR 9.12.1d's dependency order becomes real once it meets Hush or another subtype reader.
- Morph ice (Changeling, Lycan, Wendigo) and Rainbow: the engine's "is this a barrier?" is one printed `IceType` plus gained types. Losing a printed type, or printing three, touches the matcher every breaker and filter asks (CR 2.16.5, 9.12.1b).
- Kitsune: the Corp fixes the first card accessed in a breach of HQ, where CR 7.3.4a makes an HQ access random. Only Dedicated Neural Net (CR 7.3.4b) comes close.
- Tori Hanzō: classed C as a Corp interrupt on its own damage through the prevention window. The window has only ever been answered about damage the other side deals, so the first stage that takes it should check that before building.

### F9. Creation and Control — 47 cards (C 28 / V 15 / M 4)

**Decks:** three new Corp identities (Cerebral Imaging: Infinite Frontiers, Custom Biotics: Engineered for Success, NEXT Design: Guarding the Net, all Haas-Bioroid) and two new Runner identities (Exile: Streethawk, The Professor: Keeper of Knowledge, both Shaper). Five Sweep decks, one per new identity; Custom Biotics' deck carries no Jinteki card and The Professor's leans on one out-of-faction copy of each program, so each deck exercises its own deck rule.

**Stages** (from the survey):
1. **Composes: Corp ice and bioroid click breaks** — Bastion, Datapike, Heimdall 2.0, Ichi 2.0, Viktor 2.0, Minelayer, Project Wotan, Sentinel Defense Program, Gila Hands Arcology.
2. **Composes: Corp assets and an identity** — Alix T4LB07, Director Haas, Thomas Haas, Levy University, Server Diagnostics, Cerebral Imaging: Infinite Frontiers.
3. **Composes: Runner rig** — Borrowed Satellite, Cloak, Dagger, Inti, Ice Analyzer, Sahasrara, Clone Chip, Feedback Filter, Monolith.
4. **Composes: Runner events** — Exploratory Romp, Freelance Coding Contract, Levy AR Lab Access, Scavenge.
5. **Identity and deck words** — Custom Biotics: Engineered for Success, The Professor: Keeper of Knowledge, NEXT Design: Guarding the Net, Exile: Streethawk.
6. **Advancement words** — Chakana, The Source, Efficiency Committee, Haas Arcology AI.
7. **Run, break and play words** — Cyber-Cypher, Zed 1.0, Successful Demonstration, Same Old Thing, Omni-drive.
8. **Installs by text into a named place** — Director Haas' Pet Project, Howler.
9. **Machinery: condition counters** — Bioroid Efficiency Research.
10. **Machinery: ice hosted on an upgrade** — Awakening Center.
11. **Machinery: rearranging installed ice** — Escher.
12. **Machinery: preventing a break** — Tyr's Hand.

**Riskiest:**
- Tyr's Hand: a break is not an event anything can interrupt today; `Preventable` would need a break, and the break contest (paid breaks of several subroutines at once) has to park mid-ability.
- Awakening Center: ice that is hosted rather than protecting a server, encountered after the server's ice was passed, so the run's position model meets a piece of ice with no position (CR 6.2.2, CR 6.2.7a).
- Bioroid Efficiency Research: an operation that stays in play as a hosted counter with text, which no zone or `InstalledCard` holds today; F11's Oversight AI is the same machine.
- Escher: a rearrangement of every server's ice at once while the run that caused it is still resolving, through `run::reconcile_ice`.
- The Professor: an influence rule both validators (`decks::DeckFile::validate` and the builder's `deck_builder`) must apply identically, by copy (CR 1.4.5a).
- NEXT Design: a pre-game install step after hands and mulligans (CR 1.6.7a), which every setup path (session, server, sweeps) must run.

### F10. Spin — 97 cards (C 70 / V 20 / M 7)

**Decks:** one new identity, GRNDL: Power Unleashed (Weyland Consortium Corp), and one Sweep deck on it; it is also the natural home for the pack's Liability cards (Fenris, Grim, Swarm, Geothermal Fracking, Profiteering), so bad publicity taken and removed is reached in play. The Caïssa programs (Bishop, Knight, Rook, Pawn, Deep Red) want a Runner Sweep deck of their own on an existing Anarch identity, since no new Runner identity heads them.

**Stages** (from the survey):
1. **Composes: Corp ice** — Quandary, Paper Wall, Tsurugi, Wotan, RSVP, Marker, Swarm.
2. **Composes: bad publicity taken** — Fenris, Grim, Muckraker, Shinobi, Ireress, Geothermal Fracking, Profiteering, Clone Retirement, Vulcan Coverup, GRNDL: Power Unleashed.
3. **Composes: Corp operations and bad publicity removed** — Veterans Program, Witness Tampering, Restoring Face, Rex Campaign, Restructure, Cyberdex Trial, Corporate Shuffle, Blue Level Clearance, Shipment from SanSan, Interns.
4. **Composes: Corp assets, agendas and operations** — Character Assassination, City Surveillance, Isabel McGuire, Jackson Howard, Market Research, NAPD Contract, Panic Button, Power Shutdown, Reclamation Order, Shock!, Caprice Nisei, Snoop.
5. **Composes: Runner breakers and rig** — Garrote, Leviathan, Torch, Sharpshooter, Dyson Fractal Generator, Lockpick, Silencer, CyberSolutions Mem Chip, LLDS Processor, Scheherazade, Savoir-faire.
6. **Composes: Runner resources** — Activist Support, Grifter, Hard at Work, Motivation, Starlight Crusade Funding, Tallie Perrault, Woman in the Red Dress, False Echo, Record Reconstructor.
7. **Composes: Runner events and run programs** — Blackmail, Eureka!, Executive Wiretaps, Expert Schedule Analyzer, Frame Job, Hostage, Keyhole, Lawyer Up, Power Nap, Quest Completed, Recon.
8. **Trace words** — Hellion Alpha Test, Invasion of Privacy, Broadcast Square.
9. **Ice position words** — Alpha, Omega, Curtain Wall.
10. **Corp vocabulary** — Off the Grid, Gyri Labyrinth, Running Interference, Unorthodox Predictions, Accelerated Diagnostics, Sundew.
11. **Runner vocabulary** — Blackguard, Capstone, Gorman Drip v1, John Masanori, Leverage, Paintbrush, Queen's Gambit, Raymond Flint.
12. **Machinery: Caïssa programs** — Bishop, Knight, Rook, Pawn, Deep Red.
13. **Machinery: the run moves to another server** — Copycat.
14. **Machinery: swapping a card under access** — Toshiyuki Sakai.

**Riskiest:**
- The Caïssa five (Bishop, Knight, Rook, Pawn, Deep Red): ice-to-ice re-hosting with positional rules, and Deep Red's restricted memory, which CR 1.20.4b checks as a mapping rather than a sum.
- Copycat: the run changes server and position after a pass, which every run consumer keyed on the attacked server has to follow.
- Toshiyuki Sakai: a swap in the middle of an access, with the Runner allowed to decline the new card.
- Sundew: "spends 1 or more [click]" is not an event today, and its second sentence is a delayed ability scoped to one action.
- Running Interference: a per-ice surcharge read off each ice's own rez cost, against the Continuous Effect Rule's flat lingering number.
- Leverage: a damage prevention standing for a duration that nobody uses, the shape only Shred has had.

### F11. Genesis — 76 cards (C 52 / V 21 / M 3)

**Decks:** five new identities: Jinteki: Replicating Perfection, NBN: The World is Yours* and Weyland Consortium: Because We Built It (Corp); Andromeda: Dispossessed Ristie and Whizzard: Master Gamer (Runner). Five Sweep decks, one per new identity; Weyland's carries the advanceable ice (Tyrant, Woodcutter, Salvage) its credit pays for, and NBN's the tracers, so the trace words are reached in play.

**Stages** (from the survey):
1. **Composes: Corp ice** — Burke Bugs, Hourglass, Janus 1.0, Sherlock 1.0, Snowflake, Uroboros, Viper, Dracō, Midori.
2. **Composes: Corp operations, agendas and assets** — Amazon Industrial Zone, Big Brother, Commercialization, Corporate War, Edge of World, Eve Campaign, Executive Retreat, Fetal AI, Foxfire, Freelancer.
3. **Composes: more Corp cards and two identities** — Government Contracts, Jinteki: Replicating Perfection, Mandatory Upgrades, Marked Accounts, NBN: The World is Yours*, Private Contracts, Restructured Datapool, Rework, Ruhr Valley.
4. **Composes: Runner programs and hardware** — Crescentus, Deep Thought, Deus X, Disrupter, Nerve Agent, Snitch, Snowball, Muresh Bodysuit, Plascrete Carapace, R&D Interface, Whizzard: Master Gamer.
5. **Composes: Runner resources** — Compromised Employee, Cortez Chip, Data Leak Reversal, Inside Man, Joshua B., Kati Jones, New Angeles City Hall, Public Sympathy, The Helpful AI.
6. **Composes: Runner events** — Kraken, Quality Time, Satellite Uplink, Vamp.
7. **Trace words** — TMI, Data Hound, Midseason Replacements, Power Grid Overload, ChiLo City Grid, Net Police.
8. **Advancing ice and the credits that pay for it** — Tyrant, Woodcutter, Salvage, Weyland Consortium: Because We Built It, Simone Diego.
9. **Corp vocabulary** — Chimera, Dedicated Server, Encryption Protocol, Sensei.
10. **Runner vocabulary** — Andromeda: Dispossessed Ristie, Creeper, ZU.13 Key Master, Replicator, Surge, Personal Workshop.
11. **Machinery: condition counters** — Oversight AI.
12. **Machinery: rearranging installed ice** — Sunset.
13. **Machinery: the run moves to another server** — Bullfrog.

**Riskiest:**
- Bullfrog: CR 6.2.7d moves the run itself to another server mid-encounter; `MoveThisIceToOutermost` only ever moves ice within the attacked server.
- Oversight AI: a condition counter, the operation staying in play hosted on ice (shared with F9's Bioroid Efficiency Research).
- Personal Workshop: hosted cards that are not installed carrying counters, paid down and installed for free, through the install path's own checks (memory, console).
- Net Police: the first recurring credits that are a number read off the game, refilled by `payment::refill`, which takes a fixed count today.
- Creeper and ZU.13 Key Master: a memory cost that changes "even if it is not installed", asked by the install price and by the memory limit at every checkpoint.
- Sunset: rearranging ice that may be under a run, through `run::reconcile_ice`.

### F12. The rest (NAPD Multiplayer, Magnum Opus, System Core 2019) — 3 cards (C 1 / V 0 / M 2)

**Decks:** one new identity, Cyber Bureau: Keeping the Peace (NAPD Multiplayer, a neutral Police Department identity printed for multiplayer play). It gets its own Sweep deck only once its setup is built. Hired Help and Watch the World Burn are neutral and limited to 1 per deck (`deck_limit`), so each goes into an existing Sweep deck as a swap.

**Stages** (from the survey):
1. **An additional cost to run, composed**: Hired Help.
2. **An ability a resolved card creates that lasts for the rest of the game**: Watch the World Burn.
3. **A setup before the first turn, with a pooled rez discount, then a flip**: Cyber Bureau: Keeping the Peace.

**Riskiest:**
- **Cyber Bureau: Keeping the Peace**: it changes the starting hand to 10 and installs up to 5 cards ignoring costs before the first turn (CR 1.6.6, CR 1.6.7a). It then rezzes any number of them with one shared discount of 20, which no `Discount` word expresses, and flips. It is a multiplayer card, so whether it belongs in any format the engine gates is a question for the person before anything is built.
- **Watch the World Burn**: "until the game ends, whenever you access a copy of that card" is a delayed triggered ability with no end, keyed to a remembered name. The "triggered ability created by a card that resolved" row is only built in part (RWR 5d), and `EffectDuration` has no "the game" duration. The first non-agenda access also has to be removed from the game as an interrupt to the access, not after it.
- **Hired Help**: the cost is a trash of an agenda from the Runner's own score area, which is not a forfeit (CR 4.9.3 makes a forfeit a removal). It assumes `Cost::Trash` accepts `OwnScoreArea` as its zone, which no card has tested. The cost is a `ContinuousKind::RunCost` (Earth Station: SEA Headquarters) whose `while` is a successful HQ run this turn not having happened.

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
