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

**Stage 3b closed** (12 October 2026): Meridian, and ice that leaves the table during its own encounter into the Runner's score area (16 of 111 `Effect` variants single-use, none unused, over 857 card files). Its subroutine is the Runner's `PresentChoice` between `AddToScoreAreaAsAgenda { points: -1 }` and the Corp's 4[credit] and `EndTheRun`, as Nightmare Archive's choice is on access. **One branch, no new word:** `AddToScoreAreaAsAgenda` took a Corp card into the Runner's score area only out of an access, and now takes an installed one too, through the one door off the table (`uninstall::corp_install`) with the Trojans it hosts trashed as a trash would; the run finds its ice gone at its next step (`run::reconcile_ice`), which ends the encounter and passes the position (CR 8.5.10), so the run goes on to the server. **Otoroshi is held to Stage 3c:** "the Runner accesses that card unless they pay 3[credit]" is an access during an encounter, and `Effect::Access` inside a run stands only as the run's breach is replaced; the run would have to hold its encounter while the access is made and come back to it. Deck: Stronger Together takes two Meridian for its Ansel 1.0 and Tyr.

**Stage 3c closed** (12 October 2026): Otoroshi, and an access made in the middle of an encounter (16 of 111 `Effect` variants single-use, none unused, over 858 card files). Its subroutine is the Corp's `PromptChooseCards` over its installs in a remote's root (not ice, not in a central server), then a `ChooseNumber` of 1 to 3 counters, then the Runner's `OfferPaidChoice` of 3[credit] against `Access { from: OpponentInstalled, filter: Not(NotSourceCard) }`, the chosen card being the acting install inside the selection's `then`. **No new word; the access learned to come back.** `Effect::Access` inside a run stood only as the run's breach replaced; during an encounter it now stands too, and its `OutsideBreach` says so (`during_encounter`), so when the accesses are over `access::offer_next` puts the run back in `EncounterIce` rather than ending it, and resumes the ice's later subroutines when a subroutine asked for them (`resume_subroutines`, set by `pending_choice::mark_parked_resume_subroutines` beside every parked state's). `paid_ability::resolve_encounter_ice` does not carry the run on while such an access is under way (`run::accessing_during_an_encounter`). The rule is the forced encounter's shape read the other way (CR 6.5.9a: return to the effect that caused it), and CR 6.1.4b names an access initiated during an encounter as the encounter's. Deck: A Thousand Cuts takes two Otoroshi for its Anemone.

**Stage 4a closed** (12 October 2026): five Corp operations, composed, with no change to the engine (16 of 111 `Effect` variants single-use, none unused, over 863 card files). Too Big to Fail is a `Not(AmountAtLeast(Credits, 10))` play requirement over 7[credit] and a bad publicity; Under the Bus reads the Runner's last turn (`TimesLastTurn(OnAccessed)`) and trashes a connection as Elizabeth Mills trashes a location; Game Changer repeats a click for each agenda in the Runner's score area and is `removed_after_play`, as Hypoxia is; Building Blocks reveals a barrier from HQ into `PromptInstallCorpCard { ignore_costs, rez }`, as Eminent Domain's search does; Attitude Adjustment's "up to 2 agendas in HQ and/or Archives" is a selection from HQ whose `then` asks Archives for the rest (`up_to: RemainingAfterSelection(2)`), each half paying 2[credit] per card (`CardsSelected`) and both shuffling into R&D. Two NetrunnerDB rows print trash costs on operations (Game Changer 2, Too Big to Fail 5), which the card files carry so the catalog gate agrees. **Stage 4b holds Divert Power and Hangeki:** Divert Power's discount is 3 for each card derezzed this way, and `RezInstalled`'s discount is a printed number; Hangeki goes to the Runner's score area or out of the game from Archives after it has resolved, and both effects move only an installed or accessed Corp card today. Decks: Hostile Bid takes Building Blocks and Too Big to Fail for its Tocsin and Under the Bus for a Colossus (Ground Control, the other Weyland sweep deck, is the Earth Station list a planner test reads, and moved its line); Broadcast Hour takes two Attitude Adjustment for its B-1001; Endless Loop takes two Game Changer for its Big Deal.

**Stage 16 closed, and with it F2** (13 October 2026): DJ Fenris, an identity from outside the game (21 of 116 `Effect` variants single-use, none unused, over 893 card files; one `Effect` added). **Reign and Reverie is complete (58 of 58).** "Host a g-mod identity that does not match the faction of your identity on DJ Fenris when he is installed … DJ Fenris gains the text of hosted identity." **The pile is brought at setup** (CR 1.5.4a): a deck holding a card that refers to other identities (`CardDefinition::refers_to_other_identities`, a declaration beside `playable_from_archives`, since CR 1.5.4 names its two cards) gets `RunnerState::other_identities`, every playable Runner identity but its own, sorted, so a position means the same card in every game; every other game's pile is empty and costs nothing. DJ Fenris's install trigger chooses from it (`CardZoneRef::OtherIdentities`, filtered to g-mods of another faction) and `Effect::HostOtherIdentity` moves the choice into `RunnerState::hosted_identities`, a list beside the rig, as condition counters are beside the table: a hosted identity is not installed, so no scan of the rig counts it. **Gaining the text** is two readings of that list. `rules::active` lists the identity as active at its host's install while the host is in the rig, so its triggers are heard and its standing effects apply as DJ Fenris's (Chaos Theory's +1[mu], Liza's draw and tag); and `GameState::rig_abilities_card` is the one lookup for a rig card's paid abilities, so a hosted Quetzal's break is the same `ActivateAbility` on DJ Fenris's install, through the same `ActionSpace` slot — `validate` holds a card that refers to other identities to printing no ability of its own, which is what lets the index stand. "Remove hosted identity from the game if DJ Fenris is uninstalled" is a standing condition in `rules::checkpoint`, however he left; the card's text wins over CR 1.5.4b's return to the pile. The view carries both lists whole (the pile says nothing the registry does not), the bots' samples copy them and count a removed identity as no card of the deck, the card-conservation sweep does the same, and both clients draw the host's line under the board and name the gained ability on DJ Fenris's menu. **Known limits:** the bots' breaker pricing and the board's routes through ice read a rig card's own abilities, so a hosted Quetzal is used from the menu but not planned with; and Rebirth, CR 1.5.4's other card, is F6's (The Liberated Mind), and will read the same pile. Deck: Dead Reckoning takes DJ Fenris for its one Spec Work.

**Stage 15 closed** (13 October 2026): Reboot, and the Runner's facedown installs (20 of 115 `Effect` variants single-use, none unused, over 892 card files; no `Effect` added). "Run Archives. If successful, instead of breaching Archives, install up to 5 cards from your heap facedown" is a run event whose access replacement is a selection from the heap with a destination of its own, `CardZoneRef::OwnInstalledFacedown`. **A facedown Runner card is a blank card** (CR 8.1.4, 8.1.4a): no text, no type, no cost, so it is not in the rig — `RunnerState::installed_facedown` is a list beside it, and no scan of the rig, memory, link or the console limit changed. Its owner may look at it (CR 1.21.2a) and the Corp may not, so it is masked as Ayla's set-aside cards are (`MaskedZone`; the view carries a count for everyone and the cards for the Runner), the bots' samples copy it the same way, and both clients draw it as a line under the board (`hud::in_effect`: "installed facedown: …"). Reboot is removed from the game after it resolves. **Known limits**, each for the card that needs it: a facedown install raises no install event (nothing in the pool hears one), no Corp effect can target or trash a facedown Runner card yet, and nothing turns one faceup — the Apex identities' own facedown installs and their "turn faceup" are deferred to Apex. Deck: Pay As You Go takes two Reboot for its two Infiltration (10 of its 15 influence now spent; 12 in all).

**Stage 14 closed** (13 October 2026): Eavesdrop, and condition counters (20 of 115 `Effect` variants single-use, none unused, over 891 card files; one `Effect` added). "Install Eavesdrop on a piece of ice as a hosted condition counter" is an operation that becomes a counter with text (CR 1.13.6d, CR 3.5.1a). **A condition is filed in the play area while it resolves**, by its catalog subtype, as a lockdown is filed by its declaration, so its `OnPlay` resolves as the copy with that handle; the selection of a piece of ice is followed by `Effect::HostThisAsCondition`, whose two placeholders become that handle and the ice chosen, and which moves the card out of the play area into `CorpState::conditions` (CR 8.6.6b: not trashed). **A condition counter is not an install**: it is a list beside the table, so no scan of the table changed. `rules::active` lists each in its host's server while the host is there (CR 1.9.5j), `EventFilter::Host` admits a condition on the install the moment is about, a deferred trigger pinned to its handle is still present, and `ability::cascade_trash_hosted_programs`, the one place a piece of ice takes what it hosts with it, trashes it faceup into Archives. The view carries the list whole (it is faceup), the bots' samples copy it, and both clients draw it as a line under the board (`hud::in_effect`: "hosted on Ice Wall: Eavesdrop"). **One deviation:** the play needs a piece of ice installed (`play_requirement`), where the rules would let it resolve with nothing to host it on and be trashed; refused, it costs nothing a player would want. Deck: Broadcast Hour takes two Eavesdrop for its two Ganked!.

**Stage 13 closed** (13 October 2026): Patchwork, an interrupt on paying (19 of 114 `Effect` variants single-use, none unused, over 890 card files; no `Effect` added). "When you would play or install a card, you may trash 1 card from your grip. If you do, instead play or install that card paying 2[credit] less" is a once-per-turn interrupt (CR 9.9.1a, CR 9.9.3d) on a payment, so it is a declaration, `ContinuousKind::Interrupt { cost, less }`, scoped as a discount is (`Installing`, `Playing`) and asked inside the payment by the replay every parked payment uses (`payment::interrupt`, `Ask::Interrupt`, answered Use or Do not use by `ResolvePendingChoice`). It is asked only where its cost could be paid and its use this turn is unspent, a use is the card's `OncePerTurnKey`, and an offer counts it ahead of time (`payment::interrupt_could_lower`), so a card 2[credit] short is offered when Patchwork can pay the rest. **A payment that takes a card from a hand now names positions in the hand the seat is shown** (`engine::left_the_hand`): the question was asked with the played card already gone, and shown on the state that still holds it, so Patchwork's trash offered the card after the one it named. Moshing never met it, because its three Sure Gambles were one answer. Decks: Free Spirit takes two Patchwork for its two The Tungsten Tailor.

**Stage 12 closed** (13 October 2026): Daruma, a root swapped as the Runner approaches (19 of 114 `Effect` variants single-use, none unused, over 889 card files; no `Effect` added). "You may trash this upgrade. If you do" is not a cost (CR 1.16.11): a `PresentChoice` whose first option trashes it and goes on. The two places the other card may come from are a second `PresentChoice`, each a selection whose `then` is the selection of the card in the root of this server (`InAttackedServer` and not ice: the run is on this server, and Daruma has left it). From another root the swap is `SwapInstalledIce`, from HQ it is Project Yagi-Uda's `SwapApproachedIceWithCard` over `SwappableIntoThis`, which keeps an operation out. **"If you swap cards this way, the Runner may jack out" follows the swap in one sentence**, so a selection's `then` that is a `Sequence` starting with `SwapInstalledIce` now has that first swap substituted as it would be alone (`pending_choice`); a `Sequence` after the selection would have offered the jack out with nothing swapped. Decks: A Thousand Cuts takes two Daruma for its two Hyoubu Precog Manifold.

**Stage 11 closed** (13 October 2026): Fast Break, installs that share one remote server (19 of 114 `Effect` variants single-use, none unused, over 888 card files; no `Effect` added). "X is equal to the number of agendas in the Runner's score area" is `Amount::AgendasInScoreArea(Runner)`, every entry, a card added there as an agenda included (CR 10.1.3). "Draw up to X" is the `ChooseNumber` Gatekeeper already drew with. **"Install up to X cards in the root of and/or protecting a single remote server" is a chain of installs, each the `then` of the one before**, and `PromptInstallCorpCard::only_server` narrows every install after the first to the server the first chose: written as a placeholder and filled when the first lands (`pending_choice::substitute_install_server`, which goes through selections and installs where `substitute_chosen_server` stops at a selection, because an access or a run rider inside a selection was never about an install's server). The first is `remote_only`, so a new remote is offered. The chain is seven deep, each link behind `AmountAtLeast(X, k)`: the Runner has won by 7 agenda points before their score area holds an eighth agenda unless some are worth nothing (Word on the Street), and a declined pick ends the chain. Decks: Go Big takes two Fast Break for its two NAPD Cordon.

**Stage 10 closed** (13 October 2026): Formicary, ice that moves in to meet the Runner (19 of 114 `Effect` variants single-use, none unused, over 887 card files). **Its trigger is heard facedown** (CR 9.1.8c, `TriggeredEffect::while_unrezzed`, Zaibatsu Loyalty's word): "Whenever the Runner approaches a server, you may rez this ice" is `OnApproachServer` about any server, a `PresentChoice` of `RezInstalled` and nothing, and "If you do" is `ActingCardMatches(Rezzed)`, which the copy now answers (`pending_choice::copy_matches` read `Unrezzed` and let `Rezzed` pass). **The move and the encounter are one effect,** `Effect::MoveThisIceInnermostAndEncounter` (`run::move_innermost_and_encounter`): from the approach to the server the ice leaves wherever it was for the innermost position of the attacked server, keeping its install, and an encounter with it begins. Not a forced encounter: Null Signal Games' ruling is that the Runner passes it and approaches the server a second time, where a second Formicary may do the same, so the encounter begins through the one door both now share (`begin_encounter`, whose `forced` says whether its end is a pass). Decks: Family Business takes two Formicary for its two Shadow.

**Stage 9 closed** (13 October 2026): Acme Consulting, and tags the Runner is considered to have (18 of 113 `Effect` variants single-use, none unused, over 886 card files). **A considered tag is a standing number, asked like link** (CR 1.9.4): `ContinuousKind::Tags`, said of the Runner by a Corp card (`Scope::Player(Runner)`), summed by `continuous::tags` with the tags the Runner was given. It is what `EffectRequirement::IsTagged`, `Amount::RunnerTags` and the Corp's basic trash of a resource read; a removal, a cost paid in tags and the Runner's own action to remove one still read `RunnerState::tags`, because a considered tag "cannot be moved or removed … nor can it be spent to pay a cost". Acme's "during encounters with the outermost piece of ice protecting any server" is the effect's `while`, a new requirement about the table, `EffectRequirement::EncounteringOutermostIce` (the encountered ice is the first ice of the run's server). Decks: a new Sweep deck, The Truth You Need, is Broadcast Hour's list unchanged under Acme, whose Hydra, Capacitor and Game Over are what the considered tag feeds. **The deep sweep found a livelock on Ravana 1.0, older than the stage** (seed 254, Assembly Line against Burn Rate): with two Ravanas the only rezzed bioroids, each chose the other forever. Null Signal Games' ruling is that this is a mandatory loop the Corp runs as often as it likes and that "the subroutine does not have any effect", so a Ravana another Ravana resolved leaves the chain out of its choice (`CardFilter::NotRelaying`, over `EncounterTally::relayed`, the cards the chain has gone through this encounter): going back down the chain is the loop, and anything it could reach the first pass already offered.

**Stage 8 closed** (12 October 2026): Gatekeeper, Drudge Work, Lady Liberty and Saraswati Mnemonics, four Corp cards (18 of 113 `Effect` variants single-use, none unused, over 885 card files). **An agenda worth a number is a filter word** (`CardFilter::AgendaPointsExactly`): Lady Liberty's "an agenda from HQ with agenda points equal to the hosted power counters" is that filter over `HostedCounters`, its selection's `destination` the score area; and **a card's own points are an amount** (`Amount::PrintedAgendaPoints`), which Drudge Work's "gain credits equal to the agenda points on the revealed agenda" reads off the card its selection acts as. Gatekeeper's "+6 strength the turn it is rezzed" is a continuous `Strength` while its own `OnRez` has been counted this turn on this copy, and its first subroutine is a `ChooseNumber` of draws then a selection back into R&D and one into Archives sharing a budget. **"Until your next turn begins" is a duration** (`EffectDuration::UntilYourNextTurnBegins`, `Until::BeforeTurn`): `NextTurnOf` holds until something takes it off, which only allotted clicks ever do, and `ThroughYourNextTurn` holds *through* that turn, where Saraswati's install is free the moment it begins. Saraswati's ability is Peer Review's install into a remote's root with Mitosis's rider of a counter and two prohibitions bound to the install. Decks: Go Big takes two Gatekeeper for its two Magnet, Family Business two Drudge Work for its two Wall to Wall, and a new Sweep deck, Endless Exploration, puts Saraswati on A Thousand Cuts' frame with Lady Liberty and a third Project Junebug for its two Zaibatsu Loyalty; every card given up is kept elsewhere.

**Stage 7 closed** (12 October 2026): Mind's Eye and Divide and Conquer, two breaches with a root closed (18 of 113 `Effect` variants single-use, none unused, over 881 card files). **"You cannot access cards in the root of HQ or R&D" is a prohibition** (`Prohibition::AccessRootOfHqOrRnD`), asked where a breach drops what can no longer be accessed (`run::access::prune_candidates`), so a breach of either central leaves its root's candidates out and Archives' root is untouched. **A breach with no run is prohibited through by a new duration, `EffectDuration::NextBreach`** (`Until::ThroughNextBreach`, counted off the turn log's `OnBreach`): Mind's Eye's "[click], 3 hosted power counters: Breach R&D. You cannot access cards in the root of R&D during this breach" makes the prohibition just before `Breach` begins a breach that is no run, where `Run` cannot be made and `Turn` would close the root to a second run on R&D that turn. **A run can owe breaches after its own** (`Effect::BreachAfter`, `RunState::breaches_after`): Divide and Conquer's "after breaching Archives, breach HQ, then breach R&D" is each a breach of its own, announced (`BreachBegun`) and accessed where the last would have ended the run (`run::access::breach_next`), with the roots closed for the run. Decks: Burn Rate takes two Divide and Conquer for its two Finality, Dead Reckoning Mind's Eye for one of its two Spec Work, each card given up kept in other decks.

**Stage 6 closed** (12 October 2026): District 99, Mâché and Thunder Art Gallery, three Runner cards on three small words (17 of 112 `Effect` variants single-use, none unused, over 879 card files). **A tag avoided is a moment** (CR 9.9.4, 9.9.5): `prevention::prevent` announces `GameEvent::TagsAvoided` when what it prevents is tags, and `Trigger::OnTagsAvoided` hears it, about nothing and heard by everyone, as `OnTagRemoved` is; Thunder Art Gallery's "the first time each turn you avoid or remove a tag" is two entries of one printed ability, each `first_each_turn`, installing from the grip a credit cheaper (`InstallRunnerCardFromGripWithDiscount`). **A card's trash cost is read off the moment it was trashed** (`Amount::TriggeringCardTrashCost`): what the Runner paid, or, trashed for nothing, the printed cost with what the table adds (`continuous::trash_cost_delta`) — Mâché's "power counters equal to that card's trash cost", a `Repeat` of one counter. **A trigger's choice is filled as it parks:** a `PresentChoice` a trigger parks loses the moment it heard, so the numbers its options read off that moment are written in as `Fixed` (`Effect::filled`, the walk `with_chosen_number` now is one use of), which is what Mâché's "may" needed — without it the choice, once answered, placed no counters. District 99's "a card of your identity's faction" is `CardFilter::MatchesOwnIdentitysFaction`, judged per copy against the chooser's identity; its counter comes the first time each turn a program or a piece of hardware is trashed, whoever's. Decks: Pay as You Go takes two District 99 for its two Liberated Account, Safety Net Mâché for its Harmony AR Therapy, and Cold Reading Thunder Art Gallery for its The Back, each card given up kept in other decks.

**Stage 5 closed** (12 October 2026): the nine Corp agendas, assets and upgrades and the two Corp identities, composed, with no change to the engine (17 of 112 `Effect` variants single-use, none unused, over 876 card files). API-S Keeper Isobel is a turn-start "may" over a selection of an `Advanced` install whose `then` removes a token and gains 3[credit], asked only while one is advanced; Arella Salvatore is Malapert Data Vault's `AgendaCameFromThisCardsServer` over Kakurenbo's `PromptInstallCorpCard { ignore_costs, if_installed }`; Broad Daylight a "may" bad publicity and a `Repeat` of one agenda counter per `BadPublicity`, spent by Next Big Thing's `[click]` and counter cost behind `OncePerTurn`; Fly on the Wall Breaking News' tag; Giordano Memorial Field Cayambe Grid's `OfferPaidChoice` on its own `OnSuccessfulRun`, priced `Times(InZone(OpponentScoreArea, Agenda), 2)` and asked only once there is an agenda to pay for; Hyperloop Extension Fujii Asset Retrieval's two triggers; Jumon an `OnDiscardPhaseEnd` selection of a root install outside the three centrals; Neurostasis Aggressive Secretary's paid access selection, shuffled into the stack (`OpponentDeck`, `shuffle_after`); SIU a turn-start `OfferPaidChoice` with `TrashSelf` as the cost, over Sea Source's trace. Sportsmetal: Go Big or Go Home hears every score and steal (`subject: Any`) with a choice of 2[credit] or two cards; The Outfit: Family Owned and Operated is an `OnBadPublicityTaken` that gains 3[credit]. Decks: two new Sweep decks, Go Big (Sportsmetal, on Retirement Package's frame, two Hyperloop Extension for its two Élivágar Bifurcation and two Giordano Memorial Field for its two Experiential Data) and Family Business (The Outfit, on Hostile Bid's, two Broad Daylight for its two Azef Protocol), each Eternal-legal only; Honor Roll takes two Jumon, two Neurostasis and Isobel, and Broadcast Hour two Fly on the Wall, Arella Salvatore and SIU, each for cards other decks keep.

**Stage 4b closed** (12 October 2026): Divert Power and Hangeki, which complete Reign and Reverie's Corp operations (17 of 112 `Effect` variants single-use, none unused, over 865 card files). Divert Power is a `ChooseNumber` of rezzed cards to derez, then the same number of selections, then a rez whose discount is `Times(ChosenNumber, 3)`: `RezInstalled`'s discount became an `Amount` (a bare number in a card file is now `{"Fixed": N}`, three files) and `with_chosen_number` reaches it. Hangeki asks the Runner to access the chosen card or add Hangeki to their score area as an agenda worth -1; either branch runs inside a selection's `then`, where the acting card is the chosen install, so **`Effect::AsThisCard`** (single-use, its reason on the variant) resolves its inner effect as the card whose text it is, and `remove_this_card_from_game` and `AddToScoreAreaAsAgenda` take a resolving operation back out of Archives (`take_resolving_operation`) — `AsAgenda::to_the_runner` says whose score area. **The Runner now accesses on the Corp's turn:** an access outside a breach while the phase is the Corp's action phase is the Runner's to carry out (`run::accessing_outside_the_runners_turn`, read by `current_actor` and by the access actions' gate `engine::require_runner_accessing`), and the turn stays the Corp's when it ends. Decks: Endless Loop takes two Divert Power for its Next Activation Command; A Thousand Cuts two Hangeki for its Snare and Urtica Cipher — each a card other decks keep.

**Stage 4 closed** (11 October 2026): the nine Corp agendas, assets, upgrades and operations, composed entirely from words the pool already had — no engine change and no new `Effect` (17 of 111 single-use, none unused, over 829 card files). Ash 2X3ZB9CY is Adrian Seis's `Prohibit(AccessOthers)` behind a `Trace`; Braintrust is Project Atlas's counting by `Every` two past 3 beside a `RezCost` over every piece of ice; Dedicated Response Team an `OnRunEnded` heard while the Runner `IsTagged`; Elizabeth Mills an `OnRez` `RemoveBadPublicity` and a `[click]`, `[trash]` selection of a location resource; Green Level Clearance two effects; GRNDL Refinery an advanceable asset whose `HostedAdvancementTokens` is read off its last known state after its own `[trash]`; Project Ares a selection of the Runner's own installs `count`ed past 4 with an `EffectIf` on `CardsSelected`; Strongbox Daniela Jorge Inácio's persistent `StealCost`, a [click]; and TGTBT Nightmare Archive's `RevealedWhileAccessed` with an `OnAccessed` tag. Revised Core Set 32 of 38 built (`CORE2_UNIMPLEMENTED` 15 → 6, the three mechanic stages left). Decks: Stronger Together takes Green Level Clearance, Ash, Strongbox and three Project Ares; Tag You're It Dedicated Response Team; Hostile Bid Elizabeth Mills and GRNDL Refinery; A Thousand Cuts Braintrust; and Broadcast Hour TGTBT — each for cards other decks keep.

**Stage 5 closed** (11 October 2026): a trace's outcomes as moments, and the two cards that need them (17 of 111 `Effect` variants single-use, none unused, over 831 card files; `Trigger` 58 → 59). **Bernice Mai** prints "If unsuccessful", so `Effect::Trace` gains `on_failure`, resolved in `trace::submit_runner_bid` when the Runner's link strength meets the trace strength (CR 10.8.4) and carried through the Flip Switch window whole, since that window now parks the trace effect itself rather than rebuilding it. **Spinal Modem** hears "whenever there is a successful trace during a run": `Trigger::OnTraceSuccessful`, `GameEvent::TraceSuccessful` dispatched through `dispatcher::emit` at CR 10.8.6e, about nothing and heard by both players; "during a run" is `DuringRun`, which ends at access — no built card starts a trace during an access. Its other lines are The Toolbox's words: 2[recurring-credit] paying for icebreakers, +1[mu], and the console limit off its subtype. Revised Core Set 34 of 38 built (`CORE2_UNIMPLEMENTED` 6 → 4). Decks: Broadcast Hour takes two Bernice Mai for its two Sync Rerouting, and Rez Tax two Spinal Modem for The Toolbox and a Cyberfeeder — each kept by other decks. Those decks led the debug build's `every_sample_deck_matchup_finishes` into a planner probe that replays a parked payment through a sequence, about 2 MiB of unoptimised stack — a spawned thread's whole allowance — so `netrunner_session::in_parallel` now gives each sweep thread 16 MiB; release needs a fraction of it.

**Stage 6a closed** (12 October 2026): two prohibitions for the rest of a run, and the two ice that print them (17 of 111 `Effect` variants single-use, none unused, over 833 card files; `Prohibition` 17 → 19). **Hudson 1.0**'s subroutines say "The Runner cannot access more than 1 card during this run": `Prohibition::AccessMoreThanOneCard`, made by `Effect::Prohibit` until the run ends and asked where the candidates are pruned, which empties them once `RunState::cards_accessed_count` reaches 1 (CR 7.4.2b) — the same place Flagship's limit is asked, so HQ Interface's second card is never offered; its "Lose [click]" break is Eli 1.0's. **Whirlpool**'s "The Runner cannot jack out for the remainder of this run" is `Prohibition::JackOut`, asked by the jack-out itself (`RulesError::JackOutProhibited`) and by `Cost::JackOut`, so neither the action nor a card that pays by jacking out is offered; then it trashes itself, as Data Mine does. Revised Core Set 36 of 38 built (`CORE2_UNIMPLEMENTED` 4 → 2: The Cleaners and Pheromones). Decks: Stronger Together takes two Hudson 1.0 for its two Viktor 1.0, and A Thousand Cuts two Whirlpool for its two Attini — each kept by other decks.

**Stage 6b closed** (12 October 2026): damage increased as it is done, and The Cleaners (17 of 111 `Effect` variants single-use, none unused, over 834 card files; `ContinuousKind` gains `DamageDone`). "[interrupt] → Whenever you would do meat damage, increase that damage by 1" is a standing effect on the scored agenda, not a trigger: `ContinuousKind::DamageDone { kind, by }` about its `Controller`, asked once by `prevention::would` as the damage becomes imminent, before anybody is asked to prevent it. That is where the interrupt's number would land (CR 9.9.7): a prevention of some is the same subtraction either side of it (values may go below 0 while imminent, CR 9.9.7a), and one of all leaves nothing to increase either way (CR 9.9.7b). Bio-Modeled Network's "all but 1" (CR 9.9.7c) is the one ordering it could not say, and that card is not built. Unpreventable damage goes through the same door, so Flare's 2 meat damage becomes 3 (CR 9.9.7e). "You" is the side of the card doing it, so PAN-Weave's meat damage to its own Runner is not increased. A trigger was the plan and was dropped: it needed the side responsible on the parked damage, and a moment for unpreventable damage, which is never parked, for a number nobody chooses. Revised Core Set 37 of 38 built (`CORE2_UNIMPLEMENTED` 2 → 1: Pheromones). Decks: Tag, You're It takes one The Cleaners for one of its two Kimberlite Field, 21 points in 45 cards.

**Stage 7 closed, and with it F1** (12 October 2026): credits beside another kind of counter, and Pheromones (17 of 111 `Effect` variants single-use, none unused, over 835 card files). **The Revised Core Set is complete (38 of 38).** Pheromones holds virus counters and "X[recurring-credit]… X is the number of virus counters on Pheromones", and an installed card had one counter number, which every payment spent as credits; a purge would have taken the credits with the virus counters. So a rig card whose counters are another kind and which prints recurring credits (`CardDefinition::hosts_credits_beside_counters`) keeps its credits in `InstalledRunnerCard::credits`, public in the view and drawn on both clients' rig, placed, refilled and spent by `rules::payment` alone (`hosted_credits`, `change_hosted`); every other card's credits are still its `counters`, so the 31 cards that host credits did not change. Only the rig has the field, as only a Runner card in the pool prints it, and `validate` refuses it on a Corp card. "X" is `RecurringCredits::Counted(HostedCounters)` beside the printed `Fixed(N)`, written as a bare number as before, and counted each time the credits are placed or refilled (CR 1.10.5a): a virus counter placed by a successful run adds a credit at the next turn's refill. "Use these credits during runs on HQ" is `PaysFor::DuringRunsOn(server)`, as broad as the credit pool during such a run (CR 1.10.4c), Cezve's central-server word for one server. Deck: Hit List takes two Pheromones for its two Laser Pointer — another deck keeps Laser Pointer.

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

**Stage 1 closed** (12 October 2026): ten Runner programs, hardware and events, composed, with no change to the engine (16 of 111 `Effect` variants single-use, none unused, over 845 card files): Bankroll, Cradle, Tycoon, Ika, Kyuban, Guinea Pig, Hot Pursuit, Office Supplies, Insight, Paragon. Bankroll's "[trash]: Take all credits from Bankroll" is `GainCreditsPerCounter` at one a counter behind a `TrashSelf` cost, which reads the counters the cost removed (`ResolutionContext::last_known`), as Fermenter does; Cradle is `ContinuousKind::Strength` by `CardsInHand(Runner)` at -1; Ika is an icebreaker installed in the rig that moves itself onto any piece of ice by Hush's hosting (`HostRigCardOnInstall`) and breaks only during an encounter with its host; Kyuban is Pichação's `OnIcePassed` with `when: Host`; Office Supplies lowers its own play cost by `Link`, as Tailgate lowers its own by ice; Insight's Corp arranges the top 4 of R&D with Cultivate's selection back onto the deck, then `TopOfDeck` reveals them; Paragon is a first-each-turn `OnSuccessfulRun` offering Blueberry!™ Diesel's look and bottom. Guinea Pig's "trash your grip" is a "that many" selection of `CardsInHand(Runner)` into the heap, which asks the Runner to confirm the whole grip, because a selection with as many cards as it takes still parks. Decks: Hit List takes two Tycoon for its two Aurora; Cold Reading two Bankroll for its two Data Dealer and two Hot Pursuit for its two Inside Job; Express Delivery Paragon for one of its two Virtuoso and Office Supplies, a Sunny Lebeau event at 3 influence, for its Sure Gamble; Side Quest two Cradle for its two The Tungsten Tailor and Guinea Pig for its Chain Reaction; Spare Parts two Kyuban for its two GAMEDRAGON™ Pro, two Ika for its Echelon and a Telework Contract, and Insight for its Overclock. Picket Line was the first choice for the Criminal cards and was left alone: it is pinned Standard, and Reign and Reverie is not.

**Stage 2a closed** (12 October 2026): the three Runner identities and three of the four resources and hardware, composed (16 of 111 `Effect` variants single-use, none unused, over 851 card files): Akiko Nisei: Head Case, Liza Talking Thunder: Prominent Legislator, Nathaniel "Gnat" Hall: One-of-a-Kind, Algernon, Hijacked Router, Miss Bones. Akiko is an `OnBreach` of R&D with Adrian Seis's `PsiGame`, whose match is one `AddAdditionalAccess`; Liza a first-each-turn `OnSuccessfulRun` with `when: ServerKind(Central)`; Gnat an `OnTurnStart` whose intervening if is `CardsInHand(Runner)` under 3; Algernon a turn-start `OfferPaidChoice` whose "if you do" leaves a `LaterThisTurn` at the turn's end that trashes it unless `TimesThisTurn(OnSuccessfulRun)`; Hijacked Router hears the Corp's `OnServerCreated` with `when: Whose(Corp)` — the trigger is phrased about its controller, so the Runner's card names whose it means — and offers its own trash on a successful Archives run as Maglectric Rapid offers its. **One word:** Miss Bones's "Use these credits to trash installed cards" is `PaysFor::TrashCostsOfInstalledCards`, matched in `payment::covers` against the access's `pending_install`, because `TrashCosts`' filter is read off a definition and whether the card is installed is where the access found it; her twelve credits and "when there are no credits left, trash her" are Penumbral Toolkit's `AddCounters` and `trash_when_empty`. **Psych Mike is held to Stage 2b:** "the first time each turn a successful run on R&D ends" is a first time narrowed by success, which the turn log's `ServerClass` does not record, and as an intervening if (`LastRunSuccessful`) an unsuccessful R&D run earlier in the turn would have spent the first time — the Turn History Rule's case. Kasi String (F3) asks the same of a remote. Decks: three new Sweep decks, Head Case (Akiko, on Safety Net's frame, two Insight for its two Khusyuk), Legislator (Liza, Hit List's fifty cards, two Miss Bones for its two Networking) and One of a Kind (Gnat, on Free Spirit's frame, two Hijacked Router for its two Chastushka and Algernon, an Adam program at 5 influence, for its Chain Reaction), each Eternal-legal only.

**Stage 2b closed** (12 October 2026): Psych Mike, and the turn log counts a run's end by whether the run was successful (16 of 111 `Effect` variants single-use, none unused, over 852 card files). "The first time each turn a successful run on R&D ends" is `OnRunEnded` with `when: All([Server([RnD]), Successful])` and `first_each_turn`, and "you may gain 1[credit] for each time you accessed a card in R&D during that run" is a `PresentChoice` over `GainCreditsAmount(RnDCardsAccessedLastRun)`: a new `Amount`, because the run's access count (Zahya Sadeghi's) holds an upgrade accessed in the root of R&D too, which is not in R&D (CR 4.6.7b), so the run counts its accesses out of the deck beside it (`RunState::rnd_cards_accessed_count`). **One word and one column set:** `EventFilter::Successful`, valid only on `OnRunEnded`, reads `listeners::Moment::successful`, which `moments` takes off the ended run (`last_completed_run.successful`, CR 6.9.5a) since the event names only the server; the turn log counts the moment as `Class::RunEnded { server, successful }`, eight columns where `Class::Server` was four, and a server filter on a run's end (Info Bounty's mark among them) admits both of its server's columns, so nothing that counted every run's end counts differently. Written as an intervening if (`LastRunSuccessful`), a run on R&D the Runner jacked out of earlier in the turn would have spent the first time, which the test plays. Kasi String (F3) can now say "the first time each turn you make a successful run on a remote server and do not steal an agenda" with the same column set. Deck: Head Case takes two Psych Mike for its two Environmental Testing.

**Stage 3 closed** (12 October 2026): four pieces of ice, composed, with no change to the engine (16 of 111 `Effect` variants single-use, none unused, over 856 card files): Blockchain, Hydra, Peeping Tom, Thimblerig. Blockchain's "gains … before its other subroutines for every 2 faceup transaction operations in Archives" is Envelopment's `ContinuousKind::Subroutines` with `before`, counted by `Every { InZone { OwnArchives, Faceup ∧ Operation ∧ Transaction }, every: 2 }`; Hydra's three "if the Runner is tagged; otherwise, give the Runner 1 tag" are `EffectIf { IsTagged, otherwise: GiveTags }`, each read as it fires, so an untagged Runner is tagged by the first and meets the other two tagged; Peeping Tom is Engram Flush's choice of a card type, one option per type, each a `RevealHand` and Thunderbolt Armaments' `GainSubroutine` for the run with its count `InZone { OpponentHand, CardType }`, read as it is gained, since the grip is revealed then; Thimblerig is ezaM's swap as a "may" (`min: 0`) on `OnTurnStart` and on its own `OnIcePassed`. **Meridian and Otoroshi are held to Stage 3b**, because neither is composition after all: Meridian's "unless the Runner adds this ice to their score area as an agenda worth -1 agenda point" moves the ice being encountered off the table, and `AddToScoreAreaAsAgenda` takes a Corp card only out of an access; Otoroshi's "the Runner accesses that card" is an access during an encounter, and `Effect::Access` stands inside a run only as the run's breach is replaced. Decks: Hostile Bid takes two Blockchain for its Whitespace and Palisade, Broadcast Hour two Hydra for two of its F2P, Pay to Win two Peeping Tom for its two Unsmiling Tsarevna, A Thousand Cuts two Thimblerig for its two Vampyronassa.

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

**Stage 1 closed** (13 October 2026): ten pieces of ice (21 of 116 `Effect` variants single-use, none unused, over 903 card files; no `Effect` added): Aimor, Envelope, Endless EULA, Mlinzi, Najja 1.0, Kamali 1.0, Sandman, Nightdancer, Anansi, Sadaka. Mlinzi's and Kamali 1.0's "unless the Runner trashes …" are printed costs (`Cost::Trash`, from the top of the stack and from the rig), so a Runner with too few cards is never offered the trash. Sadaka's "or shuffle R&D" is `ShuffleIntoDeck([OwnRAndD])`, R&D named alone, rather than a new effect. **One engine fix, found by Sadaka:** a subroutine whose first effect parked a choice had the rest of its sentence queued behind the ice's *next* subroutine, because `paid_ability::resolve_encounter_ice` resumed the subroutines before `apply_action` drained the queue — so Sadaka's "You may draw 1 card" came after its second subroutine had trashed it. The rest of the parked subroutine now fires first (`dispatcher::drain_continuations_of`, the encountered ice's continuations), and a subroutine is one sentence, finished before the next begins. Decks: A Thousand Cuts takes the five Jinteki ice, Broadcast Hour the two NBN, Retirement Package the three Haas-Bioroid; every card they give up is in another deck.

**Stage 2 closed** (13 October 2026): counting ice and ambush upgrades, composed (21 of 116 `Effect` variants single-use, none unused, over 911 card files): Mganga, Masvingo, Oduduwa, NEXT Diamond, NEXT Sapphire, Forced Connection, Intake, Tempus. Mganga is Konjin's `PsiGame` with a `TrashCard` after it, which Stage 1's ordering fix is what lets resolve after the bids; Masvingo is NEXT Silver's `Subroutines` counted over `HostedAdvancementTokens`, with an `OnRez` that places its first counter; NEXT Diamond is Ivik's `RezCost` over the other rezzed NEXT ice; NEXT Sapphire's three X's are `CorpInstalls` of rezzed NEXT ice, a `ChooseNumber` for the draw and `up_to` for the two moves. The three ambushes are TGTBT's `RevealedWhileAccessed` with an `OnAccessed` trace whose intervening if is Snare!'s `Not(AccessingIn(Archives))`. **One fill:** Oduduwa's "place X advancement tokens on another piece of ice" is a `PromptChooseCards` inside its encounter trigger's `PresentChoice`, where the acting card of the `then` is the ice chosen, so `HostedAdvancementTokens` joins the amounts a trigger's choice writes in as it parks (`Effect::filled`) — read off Oduduwa, the card whose trigger it is. Ghost Branch and Idiosyncresis, the two cards whose options already read it, read their own card at the same moment, so nothing they do changes. **One mask:** an ambush accessed in HQ or R&D traces from where it sits, and a spectator saw neither the access nor the card, so `TraceInitiated` strikes its card for them there, as `AbilityTookCredits` already did for Esca (the 256-seed log-conceal sweep, seed 167, Tempus out of HQ). The client lists the three ambushes among the cards a rez gains nothing (`board::rez`). Decks: Go Big takes the HB three, Family Business the Weyland three, Permafrost Mganga and Pay to Win Intake.

**Stage 3 closed** (13 October 2026): nine advanceable cards and "as an agenda", composed (21 of 116 `Effect` variants single-use, none unused, over 920 card files): Bio Vault, Calibration Testing, NGO Front, Gene Splicer, False Flag, Echo Chamber, City Works Project, Reconstruction Contract, Urban Renewal. "Can be advanced" is `advancement_requirement: 0`; a printed "N hosted advancement tokens" spent is `Cost::RemoveAdvancementCounters` beside `TrashSelf` (Charlotte Caçador's), and one only held — "[click], 3 hosted advancement tokens: add Gene Splicer to your score area" — an `AmountAtLeast` requirement, since the card leaves with its counters. Remote server only is `install_only_in`; Bio Vault's "only during a run" is `DuringRun`; Urban Renewal is Nico Campaign's countdown; Reconstruction Contract hears `OnDamageSuffered` with `when: All([Whose(Runner), Damage(Meat)])`, Hijacked Router's way of hearing the other side's moment, and moves its last-known counters by a `ChooseNumber` over them. **One word made general:** `AsAgenda::to_the_runner` now decides the score area for an installed Corp card as it did for an operation, so Echo Chamber, Gene Splicer and False Flag go to the Corp's own and Meridian says `to_the_runner` — the card's "the Runner adds this ice to their score area" — where an installed Corp card had always gone to the Runner's. Overseer Matrix was held to Stage 3b. Decks: Tag, You're It takes City Works Project (for Transport Monopoly and a Hostile Takeover, 21 points), two Urban Renewal and two Reconstruction Contract; Hostile Bid two False Flag and two NGO Front; Open Book two Gene Splicer and two Bio Vault; Broadcast Hour two Echo Chamber and two Calibration Testing.

**Stage 3b closed** (13 October 2026): Overseer Matrix, and a persisting card heard once (21 of 116 `Effect` variants single-use, none unused, over 921 card files). "Persistent → Whenever the Runner trashes a card from this server or its root, you may pay 1[credit] to give the Runner 1 tag" is Yakov Erikovich Avdakov's two triggers — `OnCardTrashed` narrowed by `Whose(Runner)`, and `OnTrashedFromAccess` — with an `OfferPaidChoice`, and `persistent_after_trash`: the listener scan already adds a persisting card for every moment of the run (Tucana's), so the card needed no new word. **One fix:** the Runner trashing Overseer Matrix itself on access made it heard twice — once as the moment's subject, which hears its own trash wherever it is, and once as a persisting card — and the Corp was asked to order two copies of one ability. A persisting card that is the moment's subject now listens as the subject only, once per copy trashed. Deck: Ground Control takes two for its ZATO City Grids; Supply Chain keeps ZATO.

**Stage 4a closed** (13 October 2026): five Corp agendas and assets, composed (21 of 116 `Effect` variants single-use, none unused, over 926 card files): Armed Intimidation, Ikawah Project, Viral Weaponization, Personalized Portal, Rashida Jaheem. Armed Intimidation is Data Raven's `PresentChoice` with the Runner choosing; Ikawah Project's "[click] and 2[credit]" is a `steal_cost` of `AllOf`, Méliès City Luxury Line's word; Viral Weaponization is Algernon's `LaterThisTurn` at the end of the discard phase, counting the grip when the turn ends rather than when the agenda is scored; Personalized Portal pays `Every` 2 of `CardsInHand(Runner)` after its draw; Rashida Jaheem is Clearinghouse's `OfferPaidChoice` with `TrashSelf`. **Held to Stage 4b:** TechnoCo (a standing cost on the *other* player's installs — `Scope::Installing` reaches only its controller's), Better Citizen Program (the first icebreaker installed each turn, a column the turn log does not have), Remote Enforcement (a rez ignoring its cost from a card's install), Bacterial Programming, SSL Endorsement and Malia Z0L0K4. Decks: Hostile Bid takes two Armed Intimidation for two Sacrifice Zone Expansion; A Thousand Cuts two Viral Weaponization for two Blood in the Water and Rashida Jaheem for Neural EMP; Stronger Together two Ikawah Project for three Midnight-3 Arcology, the same six points; Pay to Win two Personalized Portal for two B-1001.

**Stage 4b closed** (13 October 2026): TechnoCo and Better Citizen Program, one scope and two turn-log columns (21 of 116 `Effect` variants single-use, none unused, over 928 card files). **`Scope::OpponentInstalling`:** TechnoCo's "the install cost of each program, piece of hardware, and virtual resource is increased by 1" is a Corp card pricing the Runner's installs, and `Scope::Installing` is its controller's own by the words every card using it prints ("you install"); a player on `CardFilter` would have been a second way to say it. Its "whenever the Runner installs …, you may gain 1[credit]" is `OnCardInstalled` narrowed by `Whose(Runner)`. **`Kind::Icebreaker` and `Kind::VirusIcebreaker`:** Better Citizen Program's "the first time the Runner plays a run event or installs an icebreaker program each turn" is two `first_each_turn` entries sharing one count, and the turn log counted a program as a program or a virus program; an icebreaker now has a column, and a virus icebreaker (Darwin, Crypsis) one of its own, as a double run event has, so "a virus program" and "an icebreaker" each still read it. **A test thread's stack:** the two columns grew the turn log, and so every `GameState`, by 720 bytes, and three debug suites that play a planner seat on libtest's 2 MiB thread overflowed it (a client tally test, the single-player history test, the terminal client's); `.cargo/config.toml` now gives every thread that does not ask the 16 MiB a match thread already asks for (`RUST_MIN_STACK`, `play::MATCH_STACK`'s size), rather than a `stack_size` per test. **Held to Stage 4c:** Remote Enforcement, Bacterial Programming, SSL Endorsement and Malia Z0L0K4. Deck: Broadcast Hour takes two Better Citizen Program for two Freedom of Information and TechnoCo for an F2P.

**Stage 4c closed** (13 October 2026): Remote Enforcement and SSL Endorsement, composed (21 of 116 `Effect` variants single-use, none unused, over 930 card files). Remote Enforcement is Ob Superheavy Logistics' search of R&D with `shuffle_after`, its install `remote_only`, and "rez it, ignoring its rez cost" an `if_installed` rider of `RezInstalled` on the placeholder — the card that landed — without `pay_cost`, since `rez` would have taken the install's cost off the same discount. SSL Endorsement's credits are its hosted counters, placed by `OnAgendaScored` and by `stolen_with_agenda_counters`; "active even while this agenda is in the Runner's score area" is its turn-start trigger twice, once `from_runner_score_area`, because a scored agenda's triggers are heard only from the area they name. **Held to Stage 4d:** Bacterial Programming and Malia Z0L0K4. Decks: Retirement Package takes two Remote Enforcement for two Accelerated Beta Test; Go Big two SSL Endorsement for two Project Vacheron.

**Stage 5a closed** (13 October 2026): ten Corp operations, composed (21 of 116 `Effect` variants single-use, none unused, over 940 card files): Distract the Masses, Genotyping, Economic Warfare, High-Profile Target, Market Forces, Riot Suppression, Self-Growth Program, Standard Procedure, Threat Assessment and Wake Up Call. "Play only if the Runner trashed a Corp card during their last turn" is Hangeki's condition over the turn log; "remove … from the game" is `removed_after_play`; Market Forces gains what was lost with `CreditsLostThisResolution`, so a Runner short of the full 3[credit] a tag pays only what they had; Self-Growth Program is two one-card selections, so a rig of one card still loses it; Threat Assessment and Wake Up Call are Hangeki's shape, the Corp's selection followed by a `PresentChoice` the Runner makes about the selected card, which is the acting install there. Standard Procedure's four options are the Runner's card types, each "Choose a card type" with its type as the note. Wake Up Call's trash is the Corp's `TrashCard` on the Runner's card, which is the Runner's choice in print; nothing in the pool tells the two apart yet. Decks: Tag You're It takes two High-Profile Target for a Public Trail and a Ping; Truth You Need two Market Forces and two Self-Growth Program for two Game Over and two Attitude Adjustment; Broadcast Hour two Standard Procedure and two Threat Assessment for two Scapenet and two Gold Farmer; Stronger Together two Riot Suppression for two Big Deal; A Thousand Cuts two Genotyping for two Cell Portal; Hostile Bid two Economic Warfare for two Punitive Counterstrike; Family Business two Wake Up Call for a Building Blocks and an Under the Bus; Go Big two Distract the Masses for two Big Deal.

**Stage 6a closed** (13 October 2026): six Runner cards, composed (21 of 116 `Effect` variants single-use, none unused, over 946 card files): Credit Kiting, Diversion of Funds, By Any Means, Falsified Credentials, Glut Cipher and Wari. Credit Kiting's "a successful run on a central server this turn" is `TimesThisTurnWhen` narrowed by `ServerKind(Central)`, and its install Bahia Bands' selection of a grip card at a discount; Diversion of Funds is Account Siphon without the tags, its "spend [click]" an `additional_play_cost`; By Any Means' "Play only as your first [click]" is Petty Cash's `NoActionTakenThisTurn` (CR 1.11.4), and its trash a `LaterThisTurn` on every access, gated by Snare!'s `Not(AccessingIn(Archives))`; Glut Cipher's "exactly 5 … if able" is a selection of five, which does nothing with fewer, and its random trashes a `Repeat`; Falsified Credentials and Wari name their type as `PresentChoice` options, expose the selected card and read it with `ActingCardMatches`. "A remote server" in a selection is the three `Not(InServer(..))` in an `All`: a `Not` over an `AnyOf` of where a card sits reads its definition, which passes every place, so it matched nothing. **One fix:** `TrashCurrentlyAccessedCard` now also trashes the card heard as it is accessed, before its access decision exists, and the breach moves on as it does for a card moved there (`move_currently_accessed_card`'s shape); it had refused anything but a card at its decision. **Held to Stage 6b:** 419: Amoral Scammer with its Sweep deck, Because I Can (a run moving the root's cards into R&D), Contaminate (virus counters on a card that keeps another kind) and Embezzle (a trash of each revealed card, which `RevealAtRandom::each` refuses because it could park). Decks: Cold Reading takes two Credit Kiting for two WAKE Implant v2A-JRJ; Express Delivery two Diversion of Funds for two Pinhole Threading; Legislator two Falsified Credentials and two Wari for two WAKE Implant v2A-JRJ and two Poison Vial; Rez Tax two By Any Means for two Finality; Pay As You Go two Glut Cipher for two Katorga Breakout.

**Stage 6b closed** (13 October 2026): 419: Amoral Scammer with its Sweep deck, and Embezzle (21 of 116 `Effect` variants single-use, none unused, over 948 card files). 419 hears the Corp's installs as Lago Paranoá Shelter does (`OnInstall`, narrowed by `Whose(Corp)`), `first_each_turn`, and acts on the card installed: the Runner's "you may" is a `PresentChoice` whose first option is the Corp's `OfferPaidChoice` of 1[credit], declined into an `Expose`. Embezzle is a run replacement naming its type as options, and "trash each revealed card that has the named type, then gain 4[credit] for each" is `RevealAtRandom::each` holding an `EffectIf` on the card, its trash and a gain. **One validation widened:** a reveal's `each` could hold only moves into a deck, since anything that parks drops the cards revealed after it; an `EffectIf`, a trash of the revealed card out of a hand (never an install, so no prevention window) and a gain never park, and are admitted. Deck: Advance Fee, 419's, on Cold Reading's frame (Criminal for Criminal at fifteen influence); Legislator takes two Embezzle for two Inside Job. **Held to Stage 6c:** Because I Can (a run that moves a root's cards into R&D) and Contaminate (virus counters on a card that keeps another kind).
**Stage 7a closed** (13 October 2026): seven breakers and break-trigger cards, composed (21 of 116 `Effect` variants single-use, none unused, over 955 card files): Amina, Cyberdelia, Engolo, Laamb, Kongamato, Takobi and Logic Bomb. "Whenever you fully break a piece of ice" is `OnIceFullyBroken`, narrowed to "using this program" by `when: ByThis` (Amina) and made "the first time each turn" by `first_each_turn` (Amina, Cyberdelia); Engolo's and Laamb's "once per turn … you may pay 2[credit]" is an `OncePerTurn` `OfferPaidChoice` on `OnEncounter` that gives the encountered ice its subtype (`GainIceSubtype`) for the encounter; Kongamato and Logic Bomb pay their own trash as a `Cost`, Logic Bomb's "lose all remaining [click]" a `Repeat` over `ClicksRemaining`; Takobi's counters, removed two at a time, boost a chosen non-AI icebreaker for the encounter. No engine change. **Held:** Trypano, whose "trash the host ice" at five virus counters is a standing condition the checkpoint does not yet read. Decks (Sweep only): Head Start takes Kongamato, Cyberdelia and two Takobi for Telework Contract, Rabbit Hole and two Self-modifying Code; Level Pegging two Laamb and two Engolo for two World Tree and two Mantle; Safety Net two Logic Bomb for two Decoy; Cold Reading two Amina for two Jeitinho.
**Stage 8 closed** (14 October 2026): seven virus and counter cards, composed (21 of 116 `Effect` variants single-use, none unused, over 962 card files): Consume, Crypt, Friday Chip, eXer, Nyashia, Upya and Zer0. "Whenever you trash a Corp card" is Solidarity Badge's pair of triggers, a trash from access and a trash by a card's text, without its first-each-turn (Consume, Friday Chip); Consume's "gain 2[credit] for each hosted virus counter, then remove all" is Fermenter's `GainCreditsPerCounter` followed by `RemoveCounters(HostedCounters)`; Crypt's "3 hosted virus counters" is a `Cost::RemoveCounters` beside its click and its trash, and its search Self-modifying Code's install from the stack; Friday Chip's "move 1 hosted virus counter to a virus program" is a paid choice of one of its counters whose effect places one on the selected program, offered only while a virus program is installed; eXer is Devadatta Drone's breach without the counter and Heliamphora's purge; Nyashia is Devadatta Drone with three counters; Upya and Zer0 are once-per-turn abilities whose costs are counters and net damage. No engine change. Decks (Sweep only): Pay As You Go takes two Crypt and two eXer for two Friend of a Friend and two Scrubber; Burn Rate two Friday Chip and Zer0 for two The Twinning and Buffer Drive; Free Spirit two Consume for two Valentina Ferreira Carvalho; Simulant two Nyashia and two Upya for two Joy Ride and two Environmental Testing.
**Stage 9a closed** (14 October 2026): five Runner economy cards, composed (21 of 116 `Effect` variants single-use, none unused, over 967 card files): Lewi Guilherme, Rogue Trading, Reclaim, PAD Tap and Kasi String. Lewi Guilherme's "the Corp's maximum hand size is reduced by 1" is Dr. Vientiane Keeling's `HandSize` about `Player(Corp)`, and its "either lose 1[credit] or trash" offers the loss only to a Runner with a credit to lose; Rogue Trading is Liberated Account with a tag; Reclaim's "trash a card from your grip" is a `Cost::Trash` beside its click and its own trash, and its install Self-modifying Code's from the heap; PAD Tap hears the Corp's `OnAbilityGainedCredits` through `when: Whose(Corp)` with `first_each_turn`, and its "only the Corp can use this ability" is Rotary's `used_by`; Kasi String's "if you breached the server but stole no agendas" is `BreachedLastRunsServer` without `StoleAgendaDuringLastRun` on Psych Mike's successful-run end, and its "4 or more hosted power counters" is asked after the trigger that is the only thing placing them. No engine change. **Held:** Jackpot!, whose "whenever an agenda is added to your score area" is a steal or a card added as an agenda, and the second is an occurrence of nothing yet (`listeners::moments` gives `AddedToScoreAreaAsAgenda` none). Decks (Sweep only): Advance Fee takes Lewi Guilherme, two Rogue Trading, two PAD Tap and Kasi String for Easy Mark, Special Order, Prognostic Q-Loop, Mu Safecracker, Swift and a Jeitinho; Dead Reckoning two Reclaim for two World Tree.
**Stage 10a closed** (14 October 2026): the Corp's trace words, four cards (21 of 116 `Effect` variants single-use, none unused, over 971 card files): Kuwinda K4H1U3, Surveyor, Trojan Horse and Amani Senai. **`Effect::Trace::base` is an `Amount`**, resolved once as the trace is initiated and kept as that number while anybody is asked to lower it (Flip Switch); a card file may still write the printed number, which every trace before Kitara does (`trace_base`). Kuwinda's "trace[X], where X is equal to the number of hosted power counters" is `HostedCounters`, Surveyor's "twice the number of ice protecting this server" `Times` over `IceProtectingThisServer` (its strength too, as a `Strength` continuous effect), and Amani Senai's "the advancement requirement of the scored or stolen agenda" the one new amount, `TriggeringAgendaAdvancementRequirement`, printed, because the agenda has left the table that changed it, and written into the choice it parks as Mâché's trash cost is. **The margin is a placeholder like `ChosenNumber`:** Trojan Horse's "an install cost of X or less, where X is equal to the amount by which your trace strength exceeded the Runner's link strength" is `PrintedCostAtMost(TraceMargin)` in the success, written over with the number as the trace succeeds (`rules::trace::submit_runner_bid`), so it rides into the selection; `validate` refuses it anywhere but a trace's `on_success`. "If unsuccessful" was already `on_failure` (Stage 5). **Held to Stage 10b:** Black Hat, White Hat and No One Home, the traces a Runner card makes the Corp start. Decks (Sweep only): Tag You're It takes two Surveyor for two Pharos; Hostile Bid two Trojan Horse for two Shipment from Kaguya; Go Big Kuwinda K4H1U3 for a BASS-CH1R180G4; Broadcast Hour Amani Senai for a Klevetnik.
**Stage 10b closed** (14 October 2026): two traces a Runner card makes the Corp start, composed (21 of 116 `Effect` variants single-use, none unused, over 973 card files): Black Hat and White Hat. "The Corp must trace[4]" is a trace on the event's `OnPlay`, bid by the Corp first as every trace is, with an empty success and the Runner's text as `on_failure`; nothing in the engine asked whose card started a trace. Black Hat's "for the remainder of the turn, access 2 additional cards whenever you breach HQ or R&D" is Climactic Showdown's `LaterThisTurn` on `OnBreach`, `every_time`; White Hat's play requirement is Credit Kiting's successful central run this turn, and its "reveal all cards in HQ … shuffle those cards into R&D" `RevealHand` and a selection of up to two revealed cards shuffled into R&D. No engine change. **Held to Stage 10c:** No One Home, whose trace starts inside the prevention window it interrupts. Decks (Sweep only): Rez Tax takes two Black Hat and White Hat for two Xanadu and a Marrow, fifteen influence of Sunny Lebeau's.

**Stage 10c closed** (14 October 2026): a trace an interrupt starts (21 of 116 `Effect` variants single-use, none unused, over 974 card files): No One Home. "Trash this resource to have the Corp trace[0]. If unsuccessful, prevent all tags or all net damage" is two paid abilities, one per word, each a trace whose `on_failure` is the `Prevent`, so the window it is used in waits for the bids: `prevention::after_interrupt` stands aside while a trace is active, and the Runner's bid calls it again once the trace is over (the bids were already let through while something is parked). An interrupt is now refused unless its word is about what is parked (`engine::activate_ability`); it was `Effect::Prevent`'s refusal alone, which a prevention waiting behind a trace never reaches. "All" is `Preventable::ALL`, a number more than anything parked, priced by the bots as three. "The first time each turn you would take tags or suffer net damage" is the ability's requirement over the turn log: no tags given or avoided this turn, and no net damage announced before this one. Decks (Sweep only): Advance Fee takes two No One Home for two Cat's Cradle.

**Stage 11a closed** (14 October 2026): what a purge removed, and a program's memory cost that grows (21 of 116 `Effect` variants single-use, none unused, over 977 card files): Acacia, Reverse Infection and Puffer. `GameEvent::VirusCountersPurged` carries `removed`, the counters the purge took off, and `Amount::VirusCountersPurged` reads the purge a trigger heard (Acacia's "gain 1[credit] for each virus counter removed", filled in as its "you may" parks) or else the one this resolution made (`ResolutionContext::virus_counters_purged`, Reverse Infection's "trash 1 card from the top of the stack for every 3 virus counters removed", under `Every`). Puffer's "costs +1[mu] for each hosted power counter" is `ContinuousKind::MemoryCost` on `This`, asked by `memory::memory_balance` through `continuous::memory_cost`, so a counter placed past the limit is the memory checkpoint's to trash for; its "place 1 power counter … or remove 1" is two abilities, the second paying `RemoveCounters` as its cost. **Held to Stage 11b:** Musaazi, Yusuf, Knobkierie and Freedom Khumalo, which spend virus counters from any installed card. Decks (Sweep only): A Thousand Cuts takes two Reverse Infection for Mavirus and Akitaro Watanabe, Rez Tax two Acacia for a Cyberfeeder and a Marrow, Hit List two Puffer for two Jeitinho.

**Stage 11b closed** (14 October 2026): virus counters spent from any card, and memory only for viruses (21 of 116 `Effect` variants single-use, none unused, over 980 card files): Musaazi, Yusuf and Knobkierie. "Any virus counter:" is `Cost::VirusCounters`, paid from whichever installed cards hold virus counters: which card each counter comes from is the Runner's, asked one counter at a time inside the payment by replay (`payment::Ask::Card`, `pending_choice::pick_virus_counters`), and not asked when every counter left is taken or one card holds them all; its affordability is the same scan (`pending_choice::virus_counters`). Knobkierie's "+3[mu]. Use the MU on Knobkierie only for virus programs" is `ContinuousKind::MemoryOnlyFor`: `memory::memory_balance_with` counts as much of it as the programs it is for spend, and an install is priced with the incoming program counted (`install_trash::short_after`, `program_could_fit` now taking the card), so a virus fits where another program of the same cost does not. Its first-successful-run counter is Captain Padma Isbister's optional selection. **Held to Stage 11c:** Freedom Khumalo, whose "Any X virus counters" is the access-cost of the card being accessed, with a Sweep deck of its own. Decks (Sweep only): Pay As You Go takes two Musaazi, two Yusuf and a Knobkierie for two Cyberfeeder, two The Tungsten Tailor and The Toolbox.

**Stage 11c closed** (14 October 2026): Freedom Khumalo: Crypto-Anarchist (21 of 116 `Effect` variants single-use, none unused, over 981 card files), Kitara's Anarch identity. "Access, once per turn → Any X virus counters: Trash the non-agenda card you are accessing. X must be equal to that card's rez or play cost" is Lampades's shape on an identity: `Cost::VirusCounters` now takes an `Amount` (`AccessedCardPrintedCost`, the printed rez or play cost), with a card file writing a fixed count as its number as a trace's base is written (`dsl::effect::number_or_amount`), and the requirement is `OncePerTurn` with `CurrentlyAccessingNonAgenda`. No engine change beyond that. Decks (Sweep only): **Pure Anarchy**, new, pinned to neither format list (Kitara is in neither pool): Pay As You Go's list under Freedom, with two Imp for its two Reboot, a second way to trash on access paid in virus counters. Its id sorts after Professional Opportunities on purpose: named Crypto-Anarchy, it moved every later Runner deck one seed on, and the 32-seed view sweep then never applied `ChooseCardName` (Whistleblower, Complete Image) in its 96 games; as Pure Anarchy the first 27 seeds pair as before.

**Stage 12a closed** (14 October 2026): currents (21 of 116 `Effect` variants single-use, none unused, over 983 card files): Corporate Grant and Death and Taxes, the first currents in the pool. "This card is not trashed until another current is played or an agenda is scored" (a Runner's) / "…stolen" (a Corp's) is CR 8.6.6c's lingering effect, read off `CardSubtype::Current` rather than declared beside it, in `rules::current`: a Corp current waits in `CorpState::play_area` beside the lockdowns, which the Corp's turn start now leaves it out of; a Runner current in the new `RunnerState::in_play`, since `play_area` is an event still resolving. Both are active there (`rules::active`) and carried whole by the view, drawn by `hud::in_effect` in both clients. The others are trashed as a current is filed, before its play is announced, and a side's currents after the other side's score or steal is announced, so a current hears the moment that ends it. Corporate Grant is an `OnInstall` of the Runner's with `first_each_turn`; Death and Taxes is Hostile Architecture's pair of trash moments (a trash by a card's text, the paid trash of an access) beside an `OnInstall` of the Runner's, each a "may". **Held:** Kill Switch, the third current, whose "must reveal" an agenda accessed in R&D is a rule of its own. Decks (Sweep only): Family Business takes two Death and Taxes for two Security Subcontract, and Hit List two Corporate Grant for two Penumbral Toolkit.

**Stage 12b closed** (14 October 2026): Gbahali and Plague (21 of 116 `Effect` variants single-use, none unused, over 985 card files). Gbahali's "[trash]: Break the last subroutine on the encountered piece of ice" is `SubroutineBreakCount::Last`, a place rather than a number: it breaks the ice's last subroutine if that one may be broken now and nothing otherwise, never the last one left; the bots' break pricing counts it only when the last is all that is pending. Plague is Hyoubu Precog Manifold's chosen server on a program: `ChooseServer` as it is installed and `when: ChosenServer` on its successful-run "may". **Held:** Hippo, whose "the first time each turn you fully break the **outermost** piece of ice" is a first time narrowed by something no turn-log class holds (Ryō "Phoenix" Ōno's deferral); Zamba, whose "whenever a Corp card is exposed" has no moment apart from a reveal (`CardRevealed` does not say an expose made it); Marathon, whose "you may not make another run on that server" has no prohibition naming a server. Decks (Sweep only): Safety Net takes two Gbahali for two The Artist, and Pure Anarchy two Plague for two Wyrm.

**Stage 12c closed** (14 October 2026): Warden Fatuma (21 of 116 `Effect` variants single-use, none unused, over 986 card files). "Each rezzed piece of bioroid ice gains “[subroutine] The Runner loses [click].” before its other subroutines" is `ContinuousKind::Subroutines` said by another card about ice (`Scope::Ice`), which `validate` refused until now: a standing grant, CR 9.8.3a (or 9.8.3e with no order said), read as each encounter begins with the ice's own grants and put outside them, inside the run's grants, which began to apply later; ice that cannot gain abilities (Hush on it) gains none. The encounter's window is (P) only (CR 6.9.3b), so the Corp never rezzes a Warden Fatuma during one; one rezzed or trashed there by a card's text changes the next encounter, not that one (**not modelled**). Decks (Sweep only): Stronger Together takes two Warden Fatuma for two Adonis Campaign.

**Stage 12d closed** (14 October 2026): Assimilator (22 of 117 `Effect` variants single-use, none unused, over 987 card files). "[click],[click]: Turn one of your facedown installed cards faceup. If that card is an event, trash it" is a selection over `CardZoneRef::OwnInstalledFacedown` whose `then` is the new `Effect::TurnFaceup` (`engine::turn_runner_card_faceup`): the card regains its characteristics and becomes active where it is (CR 8.1.4e), so it goes into the rig with its recurring credits (1.10.5b) and without an install — "install only if" is not asked and no "when installed" ability hears it. A card that may not stand in the rig is trashed (10.3.1e): the event, as the card says, and a Trojan, which is hosted on no ice. The memory limit, the console rule and uniqueness are the checkpoint's. **Not offered:** Matryoshka's facedown hosted copies, which are counted on their host (`InstalledRunnerCard::turned_facedown`) rather than listed. Decks (Sweep only): Pay As You Go, the Reboot deck, takes one Assimilator for one Chain Reaction (influence 15 of 15).

**Stage 12e closed** (14 October 2026): Kabonesa Wu: Netspace Thrillseeker (22 of 117 `Effect` variants single-use, none unused, over 988 card files; no `Effect` added). "[click]: Search your stack for a non-virus program and install it, lowering its install cost by 1[credit], then shuffle your stack. If that program is still installed when your turn ends, remove it from the game" is World Tree's stack search into `InstallRunnerCardFromZone`, which gains `if_installed`: a rider resolved as the card the install put in the rig, if it did. There it makes a `LaterThisTurn` on the turn's end as the program, so the delayed ability remembers that install and no other copy, and `ThisCardIsInstalled` asks after that install alone: a Corroder trashed and installed again is another object and stays. Deck (Sweep only): a new one, Netspace Thrill (`wu_netspace`), Level Pegging's list under Kabonesa Wu, named to sort after every other deck so the sweep schedule of the decks before it does not move.

**Stage 12f closed** (14 October 2026): Compile (22 of 117 `Effect` variants single-use, none unused, over 989 card files; no `Effect` added). "Make a run. The first time you encounter a piece of ice during this run, you may search your stack or heap for a program and install it, ignoring all costs. When the run ends, add that program to the bottom of your stack if it is still installed" is a run whose start leaves a `LaterThisTurn` on the run's first encounter (`this_run`, heard once), offering the stack, the heap or nothing; the install's `if_installed` rider (Stage 12e's) sets a run-ended effect as the program, `AddToDeck(Bottom)`, whose install is its "if". A Trojan is not offered, as Beta Build does not offer one: a text install of a Trojan has no host to choose. Deck (Sweep only): Netspace Thrill takes two Compile for two Rejig, which Level Pegging keeps.

**Stage 13a closed** (14 October 2026): Asa Group: Security Through Vigilance (22 of 117 `Effect` variants single-use, none unused, over 990 card files; no `Effect` added). "The first time each turn you install a card, you may install 1 non-agenda card from HQ in the root of or protecting the same server" is an `OnInstall` heard of any card, `first_each_turn`, whose selection's install is narrowed by `only_server` — Fast Break's placeholder, written over here by the trigger itself: a trigger heard on a Corp install writes the install's server into every `only_server` it carries (`Effect::names_only_server`, `pending_choice::substitute_install_server`), as Fast Break's first install writes its own into the next. The second install pays its costs, and being the turn's second it asks nothing. Deck (Sweep only): Vigilance, Asa Group on Stronger Together's list. The new deck moved the sweep schedule onto seed 165 (Family Business against Safety Net), where two unrezzed Formicary on one remote heard the approach and parked a trigger order naming both to the Runner and a spectator; a face-down card's name is now struck from its entry (`masking::mask_pending_decision`), as the event it heard already was.

**Stage 13b closed** (14 October 2026): Degree Mill (22 of 117 `Effect` variants single-use, none unused, over 991 card files; no `Effect` added). "As an additional cost to steal Degree Mill, the Runner must shuffle 2 installed Runner cards into the stack" is its `steal_cost`, a new `Cost::ShuffleInstalledIntoDeck { filter, count }`: picked one card at a time by replay as `AddInstalledToHand` picks (asked only when the Runner has more than two), each added to the stack, and the stack shuffled once after the last. It is a cost and not an effect after one, because a cost is paid before what it pays for (CR 1.16). Deck (Sweep only): The Truth You Need takes two Degree Mill for its two Bellona, the same three points each.

**Stage 13c closed** (15 October 2026): Emergent Creativity (22 of 117 `Effect` variants single-use, none unused, over 992 card files; no `Effect` added). "Trash any number of programs and/or pieces of hardware from your grip. Search your stack for 1 program or piece of hardware. Install it, paying X[credit] less. X is equal to the total install cost of the trashed cards" is a grip selection into the heap whose `then` is the stack search, whose own `then` installs at `Discount::Amount(CardsSelectedPrintedCost)`. The new `Amount` is the summed printed cost of what a selection chose, written into its `then` as `Fixed` when it resolves (`pending_choice`), through the selections nested in it, so the install made on a later action still knows it — `Effect::filled` and `CardFilter::filled` learnt the two discount shapes that carry an amount. A selection with nothing eligible never parks and skips its `then`, so a grip with no program or hardware goes straight to the search at full price (`EffectIf` over `ZoneHasAtLeast`); a grip with some and none chosen searches at 0 less. The extra [click] is its `additional_play_cost`. Deck (Sweep only): Head Start takes two for its two Overclock, 10 of Chaos Theory's 15 influence.

**Stage 13d closed** (15 October 2026): RNG Key (22 of 117 `Effect` variants single-use, none unused, over 993 card files; no `Effect` added). "The first time you make a successful run on HQ or R&D each turn, you may name a number. If you do, reveal the next card that you access this run. If it has a rez cost, play cost, or advancement requirement equal to the named number, either gain 3[credit] or draw 2 cards" is an `OnSuccessfulRun` on HQ or R&D, `first_each_turn`, offering a `ChooseNumber` whose `then` is a `LaterThisTurn` on the run's next `OnAccessed` (heard once, so the next card and only it). Its effect asks a new `EffectRequirement::CurrentlyAccessingMatches(CardFilter)` — the card being presented (`AccessState::currently_accessing`, set before the decision about it is parked) — over `PrintedCostExactly` or a new `CardFilter::AdvancementRequirementExactly`, so an agenda's 0 is never a rez cost. The number rides into the waiting ability because `Effect::filled` now walks a `LaterThisTurn`'s effect and `EffectRequirement::filled` a filter. The reveal is not modelled: the Runner sees the card by accessing it, and nothing in the pool reads a reveal of an accessed card. Deck (Sweep only): Head Start takes one for one of its three Sure Gamble.

**Stage 13e closed** (15 October 2026): Gebrselassie (22 of 117 `Effect` variants single-use, none unused, over 994 card files; no `Effect` added). "[click]: Host this hardware on an installed non-AI icebreaker. Abilities that increase host icebreaker's strength last for the remainder of the turn (instead of any shorter duration)" is a paid ability with GAMEDRAGON™ Pro's hosting selection (an icebreaker without the AI subtype), and a continuous effect on its host. GAMEDRAGON's `ContinuousKind::BoostsLastTheRun` became `BoostsLast(EffectDuration)` — the run or the turn, `validate` refusing any other — so the two cards are one word: `continuous::boosts_last` answers the longest duration a host's boosts are given, and a boost is lengthened only when that is longer than its own (a run-long pump under Gebrselassie lasts the turn; a turn-long one is untouched). Deck (Sweep only): Head Start takes it for one of its three Diesel.

**Stage 13f closed** (15 October 2026): Jua (22 of 117 `Effect` variants single-use, none unused, over 995 card files; no `Effect` added). "When the Runner encounters this ice, they cannot install cards for the remainder of the turn" is an `OnEncounter` making `Prohibit { what: Install, until: Turn }`, a new `Prohibition::Install` asked by `engine::install_requirement_met` — the question the one door into the rig and every gate offering an install already put — and by a selection that would install facedown (Reboot), which then asks nothing. "Choose 2 installed Runner cards, if able. The Runner must add 1 of the chosen cards to the top of the stack" is the Corp's selection of `count` 2 (every card when fewer qualify, CR 1.2.4) whose `then` is the Runner's selection of 1 onto the stack, among a new placeholder `CardFilter::ChosenThisWay`, written over when the first selection resolves as `CardFilter::AmongInstalls`, the handles it chose (`Effect::with_chosen_installs`, the walk `with_those_trashed` makes). Composition didn't work: a selection's `then` acts as its first card alone. Deck (Sweep only): The Truth You Need takes two for two of its three Gold Farmer.

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
