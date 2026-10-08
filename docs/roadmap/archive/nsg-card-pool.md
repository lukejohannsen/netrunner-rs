# The Null Signal Games card pool — the closed stages

Moved verbatim from [`docs/roadmap/nsg-card-pool.md`](../nsg-card-pool.md) on 29 September 2026, and added to by every stage since as it closes; the live file keeps one line per stage. Headings are the originals under their tranche, so "RWR Stage 3b" resolves here.

## Stage 0 — groundwork

### Stage 0a — the catalog, the gates, the vocabulary (26 September 2026)

`feat/nsg-pool-stage-0a`.

- **The catalog comes from NetrunnerDB by script** (`scripts/catalog_sync.py`,
  modelled on `rules_sync.py`). It writes `crates/netrunner_core/data/cards/<pack>.json`
  for fifteen packs, and `--check` compares them against the live API
  without writing. The script reproduced the three hand-committed files
  byte for byte: the v2 `data` array, sorted by code, keys sorted,
  two-space indent, UTF-8 kept. That is the check that the format is the
  one they used. The two files not already named by pack code were renamed
  (`system_gateway.json` → `sg.json`, `elevation.json` → `elev.json`).
  `build.rs` embeds the directory as one array per pack, so a pack joins
  the catalog by gaining a file, not a Rust constant. NetrunnerDB refuses
  urllib's default user agent with a 403, so the script sends its own, as
  `rules_sync.py` does.
- **846 printings, 839 in the catalog.** Six NSG ice print no Barrier, Code
  Gate or Sentry: Loot Box, Rime, Konjin, Excalibur, Lycian Multi-Munition
  and Vicsek. They join Data Mine in `CATALOG_UNMODELABLE` until the tranche
  that builds the first of them gives `CardType::Ice` a way to say it. That
  is a mechanic, not a catalog change. The count test is exact now; it was a
  floor that a missing pack could have passed. (Stage 2b gave the ice a
  type, `IceType::Other`, and the catalog holds all 846.)
- **Every NSG pack is gated** (`every_nsg_pack_card_is_implemented_or_explicitly_excluded`).
  Each pack has a list in `cards/unimplemented.rs`, seeded by
  `catalog_sync.py --unimplemented <pack>`: 562 entries across twelve
  lists. A reprint is in every pack that prints it, and leaves them all
  when it is built.
  **`assert_set_accounted_for` now counts a printing as built when a
  playable card carries its code or its title**, and counts the withheld
  ice as printed. The reprint packs needed both: System Update 2021's
  Corroder has a code of its own, and a card file carries one
  `numeric_id`. A mutation check (one Vantage Point entry deleted) fails
  the gate naming the card.
- **"The same card" is `cards::title_key`**: typographic apostrophes and
  quotes made plain, lowercased. NetrunnerDB spells a reprint's title as its
  editor typed it. The Maker's Eye is straight in the Core Set and curly in
  System Update 2021, so the planning count took it for unbuilt, and the
  deck builder's `resolve` and pool, which compared exact titles, would
  have listed it as a card nothing plays. Both now compare the key.
- **`CARD_VOCAB` 192 → 1024, and `OBS_SIZE` 2,263 → 6,423.** A card's slot
  is now three regions:
  - The first 184 slots are the pool as it stood, in `set_rank` order,
    closed and pinned (`LEGACY_SLOTS`).
  - Each NSG pack has a fixed block by printed code (`RESERVED_BLOCKS`,
    slots 184–757), so a slot is a function of the printing alone. **That
    is stronger than the plan asked:** a rank per set keeps sets apart, but
    within a set a card numbered below the ones already built inserted
    ahead of them, which is how *Elevation*'s stages moved slots mid-set.
    A reprint built under another code leaves its slot empty.
  - Everything else goes after the blocks.
  **The rule this sets for card files:** an NSG card's `numeric_id` is the
  NSG printing's code, or it lands outside its block. No slot of the 184
  moved: the pinned slots hold.
- **`scripts/pool_status.py`** prints, per pack, the printed count, the
  built count and the length of its list, plus the DSL ratio read off
  `pub enum Effect` and the card files. **At 0a: 26 of 70 `Effect`
  variants single-use, 3 unused (`MillRnDAmount`, `RemoveBadPublicity`,
  `Trace`), over 184 card files.** That is the last hand count's 26 and 3
  reproduced. The enum has 70 variants, where AGENTS.md's last hand count
  said 71.
- **Owed to a later stage:** Blood in the Water (Midnight Sun) prints its
  advancement requirement as X, which NetrunnerDB records as none. The card
  face draws no circle for it (`card_face`'s layout test names the card),
  until the Midnight Sun stage that builds a variable requirement gives
  `Slot` an X.

### Stage 0b — formats, and a deck's legality per format (26 September 2026)

`feat/nsg-pool-stage-0b`, stacked on 0a.

**The person's decision:**
- A card can be banned in one format and legal in others.
- A deck is legal or illegal *per format*, and the builder keeps track of
  which formats each deck is legal in.
- A deck holding a banned card is not refused. It is illegal in that
  format and can still be played casually. If that needs a Casual format,
  add one.

What landed:

- **The formats come from NetrunnerDB.** `catalog_sync.py` writes
  `data/formats.json` from the v3 API: the active card pool and restriction
  list of Startup, Standard, Eternal and Snapshot. `format.rs` parses it
  once and shares it (`NsgFormat::rules` returns `&'static`, because a
  builder asks per card and a pool is thousands of codes). The state of
  NetrunnerDB on 26 September 2026:
  - **Startup**: SG, Elevation and VP, with 5 bans (Startup Balance Update
    26.03).
  - **Standard**: eleven NSG packs, with 29 cards banned (32 printings).
  - **Eternal**: a 7-point budget.
  - **Snapshot**: an FFG pool with 17 bans and a restricted list, which is
    a budget of 1 at a point each.

  `no_shipped_format_restricts_anything_yet` is retired, and tests of what
  the lists hold replace it.
- **A pool is printing codes, not packs.** A card file names one printing,
  and a format admits a card by any of them. The file therefore lists every
  printing of every card in a pool, and every printing of a banned card.
  Corroder's file names its Core Set code, and Snapshot admits it through
  a Revised Core printing that is not embedded, so a pack check would have
  refused it. `FormatRules::allowed_packs` became `pool` and `in_pool`.
  `packs` stays for display.
- **`NsgFormat::Casual`**: every card, no list. It is the default table
  format in both clients and the CLI (`settings::DEFAULT_FORMAT`), where
  it was Startup. A deck any list refuses still plays against a bot, and
  naming a format holds a deck to its list.
- **A deck's legality is the set of formats it is legal in**
  (`DeckFile::legal_formats`, both validators, computed, never stored).
  The builder's `DeckStatus::legal_in` already showed it, and now lists
  Casual too. `every_shipped_deck_is_legal_in_the_formats_pinned_for_it`
  pins all 36 shipped decks:
  - Every deck is legal in Eternal and Casual.
  - None is legal in Snapshot.
  - 13 of the 32 published decks are not Startup-legal: Agency, Brutal Efficiency, Discretion
    Advised, Fashion Lab, Fine Print, Hyper Velocity, Party Hard, Planning
    Ahead, Quick Returns, both Catalyst decks, and both Syndicate decks.
  - 13 are not Standard-legal: every deck in the list above but Quick
    Returns and the two Syndicate decks, plus Gimbatul, Not so subtle and
    Pork Chops (Touch-ups).
  - The four Sweep decks are Eternal and Casual only.

  A category's own format (`DeckCategory::format`) is now Casual for
  everything published, and Eternal for Sweep decks.
- **The server deals a bot's deck from the part of the pool the lobby's
  format allows** (`legal_matchups`, worked out per format at bind). It
  used to refuse to start when any sample deck was illegal, which held
  while the tables allowed every deck. A daemon with a bot refuses only a
  format with no legal matchup, which is Snapshot. The default lobbies are
  therefore Startup, Standard, Eternal and Casual; Snapshot is named to
  host human-vs-human games.
- Tests that checked deck construction (size, influence, points) under
  Startup now check it under Casual. Tests that brought Fine Print or
  pinned Discretion Advised to a Startup lobby bring Startup-legal decks.

### Stage 0c — card ids are NetrunnerDB v3's (30 September 2026)

`fix/v3-card-ids`. This is the first step of moving the catalog to v3
(Stage 0d), at the person's decision: "everything v3".

**Why.** v3 names a card by a slug (`sure_gamble`), which is the id our
card files and saved decks already use. A v3 decklist's `card_slots` and a
v3 format's restriction list name cards the same way. 391 of the 407 card
files matched v3 exactly. The sixteen that didn't were fourteen
identities cut short at their colon, plus two cards whose titles had been
shortened (M.I.C., and Maglectric Rapid without its "(748 Mod)"):

| was | v3 |
|---|---|
| `mic` | `m_i_c` |
| `maglectric_rapid` | `maglectric_rapid_748_mod` |
| `the_syndicate` | `the_syndicate_profit_over_principle` |
| `the_catalyst` | `the_catalyst_convention_breaker` |
| `noise` | `noise_hacker_extraordinaire` |
| `gabriel_santiago` | `gabriel_santiago_consummate_professional` |
| `kate_mccaffrey` | `kate_mac_mccaffrey_digital_tinker` |
| `rene_loup_arcemont` | `rene_loup_arcemont_party_animal` |
| `tao_salonga` | `tao_salonga_telepresence_magician` |
| `zahya_sadeghi` | `zahya_sadeghi_versatile_smuggler` |
| `barry_baz_wong` | `barry_baz_wong_tri_maf_veteran` |
| `dewi_subrotoputri` | `dewi_subrotoputri_pedagogical_dhalang` |
| `magdalene_keino_chemutai` | `magdalene_keino_chemutai_cryptarchitect` |
| `muslihat` | `muslihat_multifarious_marketeer` |
| `ryo_phoenix_ono` | `ryo_phoenix_ono_out_of_the_ashes` |
| `topan` | `topan_ormas_leader` |

Each was resolved through its own printing code, read off v3's
`printing_ids`, never by guessing a slug from the title.

**What moved.** For each of the sixteen: the card file (renamed and its
`id` changed) and every quoted reference. That is 18 in deck files (16
identities, M.I.C. in Undertow, Maglectric Rapid in Professional
Opportunities) and 76 in Rust tests. No non-test code named any of them.
The archived Learn to Play entry, which names the starter identities' old
files, is left as written, since it is the record.

**No shim**, per the rule that nothing is kept for compatibility before a
release. Three things outside the repository that name an old id stop
resolving:
- a saved deck with one of these identities;
- a bug report or match record whose header holds the deck;
- a remembered Always/Never answer for one of these cards.

The table above is the map for re-pointing one by hand.

**Measured.**
- `cargo test --workspace`: 2,419 passed, 0 failed.
- Clippy silent.
- Both sweeps at 256 seeds: clean.
- `scripts/coverage_identical.py main --head-worktree --expect-card-renames …`:
  all four shapes (random and planner, by view and by index, 192 games
  each, seed 1) were **identical but for the expected renames**. The
  planner seatings were expected to re-roll, because `determinize` sorts
  its candidates by slug, but they didn't.
- `scripts/coverage_identical.py` gained `--expect-card-renames`. A
  card's row under `cards` and the card half of a `triggers_fired` key are
  renamed before comparing, the way `--expect-renames` already handled
  `Trigger` variants.

### Stage 0d — the catalog on NetrunnerDB v3 (30 September 2026)

`feat/v3-catalog`, the person's decision: "everything v3". Stage 0c gave
every card file its v3 card id; this stage moved the catalog under it.

**Why.** The embedded catalog was NetrunnerDB's v2 card arrays
(`data/cards/<pack>.json`), one entry per printing, and every card file
named one printing (`numeric_id`). That one number decided a card's
printed metadata, its set, its picture and its legality, and three things
existed only because of it: `formats.json` listed every printing code of
every card in a pool and of every banned card; a reprint was joined back
to its card by `cards::title_key`; and the deckbuilding validator
(`deck::Decklist`) was keyed by printing code through
`CardRegistry::get_by_numeric_id`. v3 splits a card (slug id, the rules
object) from its printings (the code: picture, set, place in it), which is
the split this engine needed.

**What is true now.**
- `scripts/catalog_sync.py` reads only v3. It writes
  `data/catalog/sets.json` (fifteen sets: id, name, v2 `legacy_code`,
  cycle, `date_release`, position, size, first printing) and
  `data/catalog/cards/<set id>.json`, each `{"cards", "printings"}`: a card
  once, in the file of its earliest embedded printing; a printing with its
  card, position, quantity, illustrators, flavour and whether a 750-pixel
  scan exists. 797 cards, 846 printings. `--check` is clean against the
  live API.
- **Two things are folded, both so a reader sees what the card says:** a
  card with more than one face (the flip identities, Méliès U) has its
  other faces' text appended as v2 spelled it ("Flip side:", "Side 2:"),
  because the Linked Clause gate quotes the flip side; and a printing's
  flavour carries its faces' flavour and the card's design credit. Checked
  against every v2 printing before the v2 files were deleted: text,
  flavour and illustrators reproduce exactly; every number and subtype
  matched; the only differences were v3's identity type ids
  (`corp_identity`) and identities' influence, `null` in v3 where v2 wrote 0.
- `cards::catalog` is the side table: `cards()` (one catalog-only
  definition per card, by v3 id), `printing(code)`, `printings_of(card)`
  (newest first), `latest_printing`, `printed_in(card, set)`, `set(id)`,
  `sets()` (newest first by `date_release`, System Update 2021 ahead of
  System Gateway on the day they share). Parsed once; it replaced
  `load_embedded_netrunnerdb_sets`, which re-parsed the whole catalog on
  every call, including every `embedded_playable_cards()`.
- `card::CardId(u32)` is `card::PrintingId`, which is what it was.
- `CardDefinition` lost `numeric_id`, `set_code`, `artist`, `flavor` and
  `image_url` (always `None`). Card files carry `built_from` in place of
  `numeric_id` — the printing the implementation was checked against —
  read by the observation vocabulary and `SameAction::FromHand`, nothing
  else. `fill_catalog_metadata` joins on the card id.
- **Legality is per card.** `formats.json` holds per format its v3 sets and
  its cards by id (v3's own `card_pool_ids`), and its ban and restriction
  lists by id. `FormatRules::{cards, sets}`, `in_pool(&CardId)`.
  `deck::Decklist` and the validator are keyed by card id;
  `DeckValidationError::PackNotLegal { set_code }` became `NotInPool`,
  and `DeckError::NoPrintedMetadata` went, since nothing is left to join.
- The gates re-keyed on card id: `printed_values_agree_with_the_netrunnerdb_catalog`,
  every set's `assert_set_accounted_for` (a printing is built when a
  playable card has its card's id — no title fold), the `*_UNIMPLEMENTED`
  lists as `(card id, title)`, the System Gateway uniqueness gate (cards
  with a printing in `system_gateway`), and the complete-formats gate,
  where `STARTUP_POOL_CODES_OUTSIDE_THE_CATALOG` went: a card-level pool
  holds no printing the catalog lacks. New:
  `every_card_file_id_is_a_netrunnerdb_card`, which also holds each
  `built_from` to a printing of that very card.
- `netrunner_card_sync`'s live registry sync (`NetrunnerDbSync`, the v2
  `/cards` and `/packs` calls, and the CLI's `cards sync` and `cards
  list-sets`) is deleted: it wrote a cache nothing read. The image store
  is keyed by `PrintingId`, its file names unchanged; it no longer reads
  the v2 envelope's `imageUrlTemplate` before every download, and it skips
  the `xlarge` request for the 219 embedded printings the catalog says
  have none (the Core Set, System Update 2021, Salvaged Memories, the
  Magnum Opus Reprint — the same 219 a local cache's manifest had learned
  by 404).
- `netrunner_client::art::printing_for` is the one question every picture
  asks: the newest embedded printing. **So the twelve card files built
  from a Core Set printing later reprinted show their newer scan** —
  eleven from System Update 2021 (Ice Wall, Corroder, Hostile Takeover…)
  and Scorched Earth from Salvaged Memories — and Phase 7 §10 Stage 3 lets
  a person choose.
- The Cards screen lists one entry per card (797, was 846), and its
  inspector lists the card's printings — set mark, set name, code,
  illustrator. The survey's finding that the Core Set's Hedge Fund read
  "Not implemented in the engine yet" beside the System Gateway one that
  was is gone with it. A set filter admits a card by any printing.
- The deck builder's pool lost its title fold, and `Draft::resolve`,
  which swapped a catalog-only `nrdb_<code>` id for the playable card's,
  went: a card's id is the one it plays under once built.
- `scripts/pool_status.py` reads the v3 catalog; its per-set numbers are
  the ones it printed on `main`.

**What moved, on purpose.**
- **Snapshot's pool gained seven embedded cards** (Abagnale, Ayla "Bios"
  Rahim, Colossus, Egret, Hortum, Marilyn Campaign, Steve Cambridge), from
  NetrunnerDB's own word: v3 lists them in the Snapshot pool through the
  Terminal Directive Cards set, which the old walk over the pool's sets
  missed. Eternal gained 43 cards the same way, none of them embedded.
  Startup and Standard are the same cards as before.
- The Cards screen's set list is in release-date order (it was by lowest
  printing code, which put Uprising ahead of its booster pack).
- **The person's data, no shim:** a saved deck holding a catalog-only
  `nrdb_<code>` id no longer finds the card.

**Measured.**
- The observation vocabulary, every slot dumped on `main` and on the
  branch: **identical** (407 cards).
- `scripts/coverage_identical.py main`: all four shapes (random and
  planner, by view and by index, 192 games each, seed 1) **byte-identical**
  to `main` — no renames to declare, since no card id moved. A catalog
  migration that changed a subtype, a link or a uniqueness flag would
  have moved the random seating first.
- Both sweeps at 256 seeds: clean.
- `cargo test --workspace`: 2,400 passed, 0 failed (2,419 on `main`; the
  difference is the tests of the deleted v2 DTOs, the numeric registry
  index, the printing-keyed decklist shape and the live sync). Clippy
  silent.
- `scripts/catalog_sync.py --check`: clean against the live v3 API.
- `scripts/pool_status.py`: every set's printed and built counts as on
  `main`; Snapshot's catalog-known cards 130 → 137 (the seven above).
- The Cards screen, screenshotted on a virtual compositor with the new
  `NETRUNNER_CARD=ice_wall` hook: 797 of 797 cards, and Ice Wall's
  inspector listing System Update 2021 #31077 (Zoe Cohen), the scan it is
  drawn as, then Core Set #01103 (Matt Zeilinger).

### 1. Vantage Point — 66 cards (C 13 / V 36 / M 17)

#### Stage 1a — every printed subtype, read from the catalog (26 September 2026)

`feat/vp-stage-1-subtypes`. No card yet.

- **`CardSubtype` is Comprehensive Rules 2.16.7's whole list.** That is
  106 words, each spelled as printed (`#[serde(rename)]` for Code Gate,
  G-mod, AP, NEXT, Caïssa, Off-site, Consumer-grade). Before this it had
  ten, each added when a card first read it.
- **A card's subtypes are its catalog keywords.** The NetrunnerDB
  conversion reads them into `subtypes`, and `fill_catalog_metadata` copies
  them onto the card file, as it does faction and influence. The 49 card
  files that authored `subtypes` no longer do, and
  `a_card_file_leaves_its_subtypes_to_the_catalog` refuses one that does.
  Every authored list was a subset of the printed keywords, so nothing was
  lost. `every_catalog_keyword_is_a_subtype_the_rules_list` holds all 83
  keywords the catalog prints to the rules' list.
- **Two cards change: Account Siphon and The Maker's Eye are run events**,
  and their files never said so. Sang Kancil's "run event" discount
  (`RunEventActive`) and MuslihaT's "an icebreaker or a run event" did not
  see them; now they do. It is a card-fidelity fix the subtype list found.
  No other card gained a subtype a rule reads.
- `CardSubtype::printed` is the word as the card prints it, so the prose
  says "fracter", not a Rust name.

#### Stage 1b — the first thirteen cards (26 September 2026)

`feat/vp-stage-1b-first-cards`: Virtual Intelligence P.I. ("Vic"), Sell
Out, Borrowed Goods, Rotary, Méliès City Luxury Line, Sleipnir, Retirement
Plan, Nihilo Agent, Grubber, Paywall, Scapegoat, Flywheel, Vulture Fund.
**No new `Effect`, `Trigger` or `EffectRequirement`.** Vantage Point 13 of
66; `VP_UNIMPLEMENTED` 66 → 53.

- **One engine change.** An ability a card names the *other* side as the
  user of now works in both directions. Before, only N-Pot's direction
  worked: the Runner using a Corp card's ability. Rotary's "[click],
  2[credit]: Trash this hardware. Only the Corp can use this ability" is a
  Runner card the Corp uses on its own turn, and `engine::activate_ability`
  read the user off the phase before it found the card. On the Corp's turn
  it looked only at the Corp's installs, so the Rotary was never found.
  The card is now found where the install is, and `used_by` names the user.
  The phase and priority checks after it still hold that user to their own
  turn. `legal_actions_for` offers it to the Corp and not the Runner, and a
  test pins both.
- **Méliès City Luxury Line is the first card to set `steal_cost`**, a
  click, and the steal path that was built for it and never exercised
  holds. The test is one steal with a click to spare and one without.
- **The Sweep decks** are three, none of them published:
  - **Retirement Package**: Engineering the Future with the HB and Weyland
    cards, on Discretion Advised's frame.
  - **Paid Content**: Making News with the NBN cards, on Hyper Velocity's
    frame.
  - **Borrowed Time**: Vic with the Criminal cards, on Stolen Goods' frame.

  The two Corp identities are Core Set cards no deck used. Borrowed Time
  is legal in every format but Snapshot; the other two in Eternal and
  Casual (pinned in `every_shipped_deck_is_legal_in_the_formats_pinned_for_it`).
- **What the new decks surfaced.** They shifted `sweep_decks_for_seed`'s
  rotation, and `board::diff`'s transitions test, which plays its first
  six seeds, reached a Petty Cash played from Archives for the first time.
  The diff had drawn it as a move back into Archives as well as the move to
  removed from the game, so it is fixed in a commit of its own. Three tests
  that had written Startup as "System Gateway and Elevation" now ask the
  format's pool instead.
- **Fidelity limits:**
  - Rotary's "whenever you breach HQ or R&D" is a successful run on
    either, as Docklands Pass's is.
  - Nihilo Agent's "load … when it is empty, trash it" is three counters
    and a self-trash after its own removal. Nothing else in the pool
    removes its counters.
  - Sleipnir's "from HQ or Archives" is a choice of zone, then a card.
- **DSL ratio (`pool_status.py`): 25 of 70 `Effect` variants single-use,
  2 unused** (`MillRnDAmount`, `Trace`), over 197 card files. It was 26
  and 3: `RemoveBadPublicity` left the unused list for Nihilo Agent and
  Scapegoat.
- **Measured.** Both sweeps at 256 seeds are green. `coverage_identical.py
  main`: the random seatings are **identical**, by view and by index; the
  heuristic ones moved across the pool (Send a Message scored 44 → 24,
  Public Access Plaza's turn starts 216 → 133, …). **That is
  `determinize`, not a rule:** its Corp sample draws hidden cards from
  every playable Corp card in the registry, and thirteen new ones changed
  what the heuristic imagines in HQ and R&D. Checked, not inferred: this
  branch against a copy without the ability-lookup change is identical in
  all four shapes, so the engine change moves nothing. **Every card a stage
  adds will do the same**, so a stage's heuristic reports are not diffed
  against the last stage's — the Elevation rule, for the same reason.

#### Stage 2 — bad publicity, tags and costs (26 September 2026)

`feat/vp-stage-2-bad-publicity`: Editorial Division: Ad Nihilum, Witch
Hunt, Take a Dive, Kompromat, Reanimation Protocol, Unleash, realloc(),
Flood the Market. **No new `Effect`.** Vantage Point 21 of 66;
`VP_UNIMPLEMENTED` 53 → 45. Vicsek is Stage 2b and Synchrocyclotron
Stage 3 (above).

- **What the cards needed, and where it went:**
  - **Taking bad publicity is a moment** (`Trigger::OnBadPublicityTaken`,
    Editorial Division). "Take" and "give" are one event, the Corp's
    (CR 1.14.2e), so Witch Hunt's "take 1" and Take a Dive's "give the
    Corp 1" are both heard; `Effect::GiveBadPublicity` now dispatches its
    event, and a zero is an occurrence of nothing.
  - **The Runner has a removed-from-game zone**
    (`RunnerState::removed_from_game`, in the view and the sweeps' card
    count), and an event that prints "Remove this event from the game" says
    so as a declaration (`CardDefinition::removed_after_play`) that
    `engine::play_event` reads when it files the card.
  - **A derez can be the price** (`Cost::Derez`, Kompromat's nested
    "unless they derez 1 piece of ice", CR 1.16.11b), asked one card at a
    time like `Cost::Trash`; the ice is "protecting the attacked server"
    after the run has ended (`CardFilter::InLastRunServer`), because
    `InAttackedServer` falling back on the last run would have made LEO
    Construction usable after a run. A run offer can be narrowed to the
    servers ice protects (`PromptChooseServer::only_protected_by_ice`).
  - **"Install and rez … paying a total of 10 less"**
    (`PromptInstallCorpCard::rez` and `if_rezzed`, Reanimation Protocol).
    The Corp divides a total between the two costs (CR 1.16.2f); the
    division taken is install first, which is never more credits. A rez it
    cannot afford leaves the ice installed and unrezzed (CR 1.16.4b). The
    offer of servers now prices a discount, which also let Mercia B4LL4RD's
    1[credit] reach a server it made affordable.
  - **"If you scored this agenda this turn"** is a fact about the copy
    (`ScoredAgenda::scored_on_turn`, `EffectRequirement::
    ThisAgendaScoredThisTurn`, Witch Hunt), not a turn-log count: a
    second Witch Hunt, or one scored last turn, is not this one.
  - **"If it is a non-liability ice"** asked as the card itself
    (`EffectRequirement::ActingCardMatches`).
  - **A count of servers** (`Amount::ProtectedRemotesWithRootCards`, Flood
    the Market), and `PlaceAdvancementCounters` takes an `Amount` in place
    of a number — seven card files rewritten rather than a
    `PlaceAdvancementCountersAmount` beside it.
  - `Amount::PrintedInstallCost` is `PrintedCost`: realloc() reads a rez
    cost, and the printed cost is one number either way.
- **Three fixes the cards reached.**
  - A rez of "the chosen card" inside a sequence resolves as that card
    (`RezInstalled`'s placeholder is the acting install): Unleash rezzes
    the ice and then resolves one of its subroutines.
  - Credits a card's text gains are that card's
    (`AbilityGainedCredits` names the prompting card, not the card a
    selection resolves as): realloc()'s are an operation's, which The
    Zwicky Group hears.
  - An additional play cost's events are dispatched after the play, as
    every payer's are: Unleash's "remove 1 tag" is a tag removed.
- **The decks.** **Ad Nihilum**, a new NBN Sweep deck on Editorial
  Division with every gray ops and liability card the identity searches
  for, the Weyland, Jinteki and HB operations on influence, and legal in
  every format but Snapshot. **Take a Dive** replaces Jailbreak in Pay as
  You Go (Noise), and **Kompromat** replaces Tread Lightly in Borrowed
  Time (Vic); both cards it replaced are in other decks.
- **Fidelity limits:**
  - realloc()'s "choose 2 rezzed pieces of ice" is two choices of one,
    each resolved before the next is offered. Nothing hidden changes in
    between, and each card is its own instance of the effect anyway
    (CR 9.12.2c names realloc()).
  - Editorial Division shuffles R&D when the search is declined.
  - The "total" discount is never asked: the Corp is given the division
    that costs least.
- **DSL ratio (`pool_status.py`): 23 of 70 `Effect` variants
  single-use, 2 unused** (`MillRnDAmount`, `Trace`), over 205 card files.
  It was 25 and 2: `ResolveSubroutineOfSelectedIce` (Unleash, beside
  Mycoweb) and `SetRunEndedEffect` (Kompromat, beside Charm Offensive) left
  the single-use list, and nothing joined it. The growth went into the vocabulary of when (`Trigger`), whether
  (two requirements), what a card reads (`Amount`, `CardFilter`) and what
  it costs (`Cost::Derez`).
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included, so every Ad Nihilum card and both Runner events were seen in
  play. `coverage_identical.py main` (192 games a shape, seed 1):
  - **Random seatings differ, and the whole play difference is Mercia
    B4LL4RD's offer.** Against a copy of this branch with only the
    discount pricing of the install offer reverted, the random reports
    differ from `main` in one line, `effects_seen: MoveThisCardToRoot`
    0 → 85 — the effect walk now reaching `PromptInstallCorpCard::then`,
    which Mercia's rider always resolved through and the report never
    credited. With the pricing back, a few games branch (Corp flatlines
    76 → 75, Mercia's action-phase triggers 85 → 87).
  - **The credit attribution moves nothing:** against a copy without it,
    all four reports are identical.
  - **Heuristic seatings move across the pool** (Corp wins 69 → 86 of
    192; The Zwicky Group's credit triggers 27 → 53, Mercia's 273 → 242).
    Neither engine change above is it: the heuristic reports are
    identical with each reverted. What is left is the new cards, which
    `determinize` samples into every hidden Corp zone (Stage 1b's check),
    not re-checked card by card here.

#### Stage 2b — ice with none of the three types (26 September 2026)

`feat/vp-stage-2b-ice-with-no-breaker-type`: Vicsek. **No new `Effect`.**
Vantage Point 22 of 66; `VP_UNIMPLEMENTED` 45 → 44.

- **Ice that prints none of Barrier, Code Gate or Sentry is
  `IceType::Other`**, a fourth variant, not `CardType::Ice(Option<..>)`.
  Every reader asks "is this ice a barrier?", and equality with a variant
  that is none of the three already answers no, so no matcher changed; an
  `Option` would have put `Some(..)` at about two hundred sites and in
  every ice card file. Only a breaker with no restriction breaks it (CR
  3.9.5h), and a card that gains a type reaches it through
  `ContinuousKind::GainSubtype` as it reaches any ice. `validate` refuses
  `Other` where it can only mean nothing: a breaker restricted to it, a
  card gaining it.
- **`CATALOG_UNMODELABLE` is gone.** The catalog reads an ice's type as the
  first of the three its keywords name, `Other` for none, so the seven
  printings it withheld (Data Mine, Loot Box, Rime, Konjin, Excalibur,
  Lycian Multi-Munition, Vicsek) are in the catalog: 846 of 846. The set
  gates and the observation's reserved blocks no longer add them back.
  Hafrún prints two types ("Barrier - Code Gate") and reads as its first
  until Parhelion builds it.
- **`GiveTags` takes an `Amount` in place**, as `PlaceAdvancementCounters`
  did in Stage 2: twenty card files rewritten to `{"Fixed": n}` rather
  than a `GiveTagsAmount` beside it. Vicsek's "X is equal to the number of
  tags the Runner has" is read twice, once for the damage and once for the
  tags, which agree because nothing between them changes the count: the
  damage's prevention window admits only interrupts, and no interrupt
  removes a tag. A primitive that fixes X once was not built for the one
  card in the catalog that uses a defined X twice.
- **The bots and clients.** The evaluator and the observation keep their
  three flags: `Other` ice encodes as no one-hot (so `OBS_SIZE` does not
  move), and the Corp's evaluator counts it broken only by a rig that
  covers all three types, which an AI does (a rig of three typed breakers
  reads as breaking it too, the one inexactness). The desktop draws a
  rezzed one on the untyped tile art in the untyped steel frame.
- **The deck.** Two Vicsek replace A Thousand Cuts' two Diviner (Jinteki
  Sweep deck, Personal Evolution); Diviner is still in three other decks.
- **DSL ratio (`pool_status.py`): 23 of 70 `Effect` variants
  single-use, 2 unused** (`MillRnDAmount`, `Trace`), over 206 card files —
  unchanged, one card later.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included, so Vicsek was seen in play. `coverage_identical.py main` (192
  games a shape, seed 1): **the random seatings are identical**, by view
  and by index. Vicsek is in a Sweep deck, which `--all-matchups` never
  plays, so that is the engine change measured alone: `IceType::Other`,
  `GiveTags(Amount)` and the seven printings joining the catalog move no
  game. **The heuristic seatings move** (Corp wins 86 → 78 of 192: agenda
  wins 72 → 68, flatlines 14 → 10). Checked, not inferred: this branch
  without Vicsek's card file and deck swap is identical to `main` in all
  four shapes, so the movement is the one new playable card `determinize`
  samples into hidden Corp zones.

#### Stage 3a — standing kinds: play, steal and score prices (26 September 2026)

`feat/vp-stage-3a-standing-kinds`: Tailgate, Reverb, Synchrocyclotron,
Magistrate Revontulet, Hype Machine, Let Them Dream. **No new `Effect`.**
Vantage Point 28 of 66; `VP_UNIMPLEMENTED` 44 → 38. Stage 3 was split: the
turn-log and hosted-counter cards (Chain Reaction, Underdome Irregulars,
The Red Room, Perfect Recall, Lotus Haze) are Stage 3b. Built on #226,
the search-shuffle fix Let Them Dream's R&D search needed.

- **Four `ContinuousKind`s, each a price asked in one place:**
  - `PlayCost` (Tailgate's "lowered by 1[credit] for each piece of ice
    protecting HQ"), asked by `continuous::play_cost_of` at the play and in
    the offer (`can_play_operation`), as `install_cost_of` is for installs.
  - `PlayClicks` (Synchrocyclotron's "costs [click] less"), taken off the
    Double's additional click (`additional_play_cost_of`), never the
    action's own: the only click a card in the pool lowers.
  - `StealCost` (Magistrate Revontulet's "as an additional cost to steal an
    agenda … 3[credit]"), joined to the agenda's printed `steal_cost` as one
    price (CR 1.16.10b), which the Runner may decline (1.17.3d).
  - `AgendaPoints` (Let Them Dream's "worth 1 less" in the Runner's score
    area). **Asked, never stored:** `win::agenda_value_in` is the one
    number, and the win check, the stored tally at a score or a steal, a
    forfeit and `Amount`s that count points all read it. The printed-only
    `agenda_value` is gone.
- **Three `Scope`s:** `Playing(filter)`, the play's half of `Installing`;
  `Stealing(filter)`, an agenda being stolen; `ScoreArea(side)`, the card's
  own text read only there (`Scope::is_own_text`).
- **"The first double operation you play each turn"** is a turn-log count:
  `Kind::DoubleOperation` and `DoubleEvent` are columns of their own, the
  one subtype the log counts apart (a double is played in the open), and
  `first_each_turn` on a `Playing` scope reads `Occurrences::plays`.
- **Two `Amount`s** (`IceProtecting(server)`, `OtherUnrezzedIce`) and a
  placeholder filter (`CardFilter::InRootOfThisServer`, written over as
  `InRootOf(server)` when Hype Machine's ability resolves, from the
  install or from `LastKnown::server` once its "[trash]:" has taken it).
- **`AddToBottomOfStack` is `AddToBottomOfDeck`**, the Corp's too (Let Them
  Dream's "the bottom of R&D"); a Corp card's move is hidden from the
  Runner like a Corp discard.
- **A sweep finding:** Sell Out's "trash 1 installed resource" asks which
  when there are two, and `payment::could_ask` listed no play among the
  actions that can ask; the debug assertion caught it once Borrowed Time
  reached two resources. A play's additional cost that takes cards is on
  the list now.
- **The decks.** Tailgate for Jailbreak (Borrowed Time); Reverb for
  Whitespace and two Synchrocyclotron for two NICO Campaign (Retirement
  Package, whose Retirement Plan is the double); Hype Machine for AMAZE
  Amusements and Magistrate Revontulet for Nihilo Agent (Paid Content); Let
  Them Dream for Offworld Office (Hostile Bid). Every card replaced is in
  another deck.
- **Fidelity limits:** Let Them Dream's search is "HQ, R&D or Archives"
  as a choice of zone first; a card that prints two types (Hafrún) is
  still Parhelion's question.
- **DSL ratio (`pool_status.py`): 22 of 70 `Effect` variants single-use,
  2 unused**, over 212 card files. It was 23: `AddToBottomOfDeck` has two
  cards now.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included. `coverage_identical.py` against the fix it is built on (192
  games a shape, seed 1): **the random seatings differ only by the
  rename** — `AddToBottomOfStack` 35 → `AddToBottomOfDeck` 35, the same
  seven cards moved — so no game in the pool moved, which is the pricing
  and scoring questions measured alone (every new card is in a Sweep deck,
  which `--all-matchups` never plays). The heuristic seatings move (Corp
  wins 75 → 78 of 192), the movement `determinize` makes of every new
  playable card; not re-checked card by card this stage.

#### Stage 3b — turn-log counts and hosted counters (26 September 2026)

`feat/vp-stage-3b-turn-counts-and-counters`: Chain Reaction, Underdome
Irregulars, The Red Room, Perfect Recall, Lotus Haze. **One new `Effect`**
(`PromptMoveThisCardToAnotherRoot`). Vantage Point 33 of 66;
`VP_UNIMPLEMENTED` 38 → 33. Stage 3 is complete.

- **"If you made a successful run on HQ, R&D, and Archives this turn",
  "if a piece of ice was rezzed this turn"** are
  `Amount::TimesThisTurnWhen { trigger, when }`: the turn-log count
  narrowed by the same `EventFilter` a trigger's `when` uses, read through
  `turn_log::Occurrences`. `TimesThisTurn` had named no filter because
  `Amount` was `Copy`; dropping `Copy` broke nothing in the workspace.
  `CardFilter::Ice` says "a piece of ice" of any type, which the log counts
  as one kind.
- **"Central server only"** is a declaration, `CardDefinition::
  install_only_in` (`ServerKind::Central`/`Remote`), refused by
  `place_corp_card` and left out of both install offers, and honoured by a
  move (CR 8.5.12). Four later cards print "Remote server only".
- **"During a run against another server"** is `And(DuringRun,
  Not(RunAgainstThisServer))`, a requirement asked of the card's own
  server at any step of the run.
- **"The Runner cannot steal or trash copies of that card"** is a lingering
  prohibition aimed at copies (`lingering::On::CopiesOf`, made by
  `Effect::Prohibit { copies_of_it }` as the revealed card), and the four
  access sites ask `continuous::cannot_about` of the card accessed, so a
  copy the Runner may not steal may be passed.
- **"Move 1 rezzed upgrade to the root of another server"** is the new
  `Effect`: every existing server choice starts a run or installs a card,
  so it parks `ChooseServer` in a third mode (`move_to_root`) and resolves
  as `MoveThisCardToRoot`. The servers offered are the ones that exist, its
  own excepted.
- **The decks.** Chain Reaction for Overclock (Pay as You Go); Underdome
  Irregulars for Overclock (Borrowed Time); The Red Room for PAD Campaign
  and Lotus Haze for Offworld Office (A Thousand Cuts); Perfect Recall for
  Manegarm Skunkworks (Retirement Package).
- **Fidelity limit:** Chain Reaction's "trash 2" is two choices of one,
  each resolved before the next. A move honours an install restriction
  (CR 8.5.12: it "applies at all times"), so Lotus Haze never offers The
  Red Room a remote.
- **DSL ratio (`pool_status.py`): 23 of 71 `Effect` variants
  single-use, 2 unused**, over 217 card files. It was 22 of 70: the new
  `PromptMoveThisCardToAnotherRoot` is single-use (Lotus Haze), its reason
  on the variant; everything else went into the vocabulary of what a card
  reads (`Amount`, `CardFilter`, `EffectRequirement`) and a declaration.
- **A gate entry, reasoned:** no agent played Chain Reaction in the 768
  games of the deep view sweep — it needs successful runs on all three
  centrals in one turn with a click to spare, which no agent plans and no
  random seat completed. It is on `CARDS_RARE_WITH_SWEEP_DECKS` with that
  reason and no batch that demands it; its per-card test is what reaches
  it, and it stays in Pay as You Go so an agent that does make that turn
  plays it under the sweep.
- **Measured.** Both sweeps at 256 seeds are green (the view sweep with the
  entry above; before it, its only failure was that gate line — no
  deadlock, no crash). `coverage_identical.py main`: **the random seatings
  are identical**, by view and by index — the turn-log amount, the install
  restriction, the copies prohibition and the move change no game in the
  pool. The heuristic seatings move (Corp wins 78 → 89 of 192), the movement `determinize`
  makes of every new playable card; not re-checked card by card.

#### Stage 4a — the run's moments about ice (26 September 2026)

`feat/vp-stage-4a-ice-moments`: Vertigo, Sipa, Lethe, ezaM, The Tungsten
Tailor. **No new `Effect`.** Vantage Point 38 of 66; `VP_UNIMPLEMENTED`
33 → 28. Stage 4 was split: Lionsmane, Event Horizon and Ansel 2.0 are
4b, and Shackleton Grid moved to Stage 5 (above).

- **Four moments a card can hear**, each a step the rules name:
  `Trigger::OnIcePassed`, `OnSubroutineBroken`, `OnIceFullyBroken` (CR
  6.5.7a, a new `GameEvent::IceFullyBroken`) and `OnIceBypassed` (6.5.8).
  The three events that already existed (`IcePassed`, `SubroutineBroken`,
  `IceBypassed`) were occurrences of nothing and are now dispatched: the
  run engine's own dispatch takes the pass and a click-break, a breaker's
  break and a bypass go through `dispatcher::emit`.
- **What was true of the ice is read off the event** (`dsl::IceFacts`,
  `EventFilter::Ice`): "the outermost piece of ice" (position 0,
  CR 4.6.9b), "after fully breaking it" (`IcePassed::after_fully_breaking`,
  CR 6.1.3f: only the encounter just ended) and "a piece of ice with 0 or
  less strength" (`SubroutineBroken::strength`, the strength it was broken
  at). On the event rather than the state because a trigger's `when` is
  asked again where it fires. `validate` admits only the facts a trigger
  states.
- **"The first time each turn" counts them.** Sipa's and The Tungsten
  Tailor's first times are narrower than a card's type, so the turn log
  counts a moment about ice by its facts (`turn_log::Class::Ice`, eight
  columns), not as `Kind::Ice`. This was the gap Ryō "Phoenix" Ōno is
  deferred on, filled for the words these two print; Ryō's "after a
  subroutine resolved during that run" is still not one of them.
- **Fully broken is run state** (`RunState::fully_broken`, in the view and
  the determinized sample): set by `run::break_subroutine`, now the one
  way any subroutine is broken, and cleared as an encounter begins. Not
  modelled: ice with no subroutines is fully broken as step 6.9.3b begins
  (6.5.7c).
- **A facedown ice is not "this" to itself for these moments.** An
  unrezzed Vertigo is passed without being encountered, and its "when the
  Runner passes this ice" is inactive (CR 9.1.7, none of 9.1.8's
  exceptions): the Listener Rule's "the subject always hears it" does not
  reach a moment about ice in a run.
- **`ModifyStrength` takes which ice and how long** (`each_ice`,
  `duration`) in place of a number, and "each piece of ice gets +1
  strength for the remainder of this run" is a `Lingering::Strength` on
  `On::EachIce`, which `lingering::ice_strength` adds and a rig card's
  strength does not. Leech's file rewritten. **A selection of one swaps
  it with the card the choice acts as**: ezaM's "this ice", and Sipa's
  "it" through `acts_on_subject`.
- **A sweep finding:** Lethe's "add 1 installed Runner card to the grip"
  failed the view sweep's concealment check (seed 57): the Corp's log
  named a card now in the grip. The card was faceup on the table when
  chosen, so the selection is marked revealed, which also gives the
  Runner's log the line.
- **The decks.** Sipa for DZMZ Optimizer (Safety Net); The Tungsten Tailor
  for Scrounge and a Wildcat Strike (Pay as You Go); Vertigo for Tithe
  (Retirement Package); two ezaM for two Tithe (A Thousand Cuts); two
  Lethe for two Tithe (Paid Content). Every card replaced is in another
  deck.
- **Fidelity limits:** Lethe's first subroutine asks top or bottom before
  the card. ezaM's "look at the top card" is a selection of it that may
  be left.
- **DSL ratio (`pool_status.py`): 21 of 71 `Effect` variants single-use,
  2 unused**, over 222 card files. It was 23: `SwapInstalledIce` (Tāo
  Salonga's, now ezaM's and Sipa's too) and `ModifyStrength` (Leech's,
  now ezaM's) left the single-use list.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included, so every new card was seen in play. `coverage_identical.py
  main` (192 games a shape, seed 1): **the random seatings differ in one
  line**, `events/IceFullyBroken` 0 → 42, by view and by index — the new
  event counted where it happens, and no game in the pool moved: the new
  moments, the facts, the turn-log columns, the dispatch of the three old
  events and the rewritten Leech change nothing a card in the pool did.
  The heuristic seatings move (Corp wins 89 → 82 of 192), the movement
  `determinize` makes of every new playable card; not re-checked card by
  card.

#### Stage 4b — two costs and a break on this ice (26 September 2026)

`feat/vp-stage-4b-lionsmane-event-horizon-ansel`: Lionsmane, Event
Horizon, Ansel 2.0. **No new `Effect`.** Vantage Point 41 of 66;
`VP_UNIMPLEMENTED` 28 → 25. Stage 4 is complete.

- **"Unless the Runner jacks out" is a cost** (`Cost::JackOut`,
  Lionsmane): CR 1.16.11b makes "[instructions] unless [player] [cost]" a
  nested cost, so it is an `OfferPaidChoice`'s price, not an effect. It
  ends the run as a jack-out (`GameEvent::RunJackedOut`, dispatched by the
  payer), which "when a run ends" hears and Shred's "would end the run"
  does not stop.
- **"Lose [click][click]:" is a cost that is not an action**
  (`Cost::LoseClicks`, Ansel 2.0): CR 9.5.2a makes an ability an action
  only when its cost begins with [click], so the Runner uses this one in
  the encounter's window. It spends nothing, so no `ClickSpent`.
- **"Break … subroutines on this ice"** asks that the acting card be the
  ice encountered (`EffectRequirement::EncounteringThisIce`). **N-Pot had
  the bug this closes:** its "3[credit]: Break 1 subroutine on this ice"
  broke a subroutine on whatever piece of ice the Runner was encountering
  while an N-Pot was rezzed anywhere. Checked: Ansel 2.0's test fails for
  N-Pot on the file without the requirement.
- **"Remove 1 card in the heap from the game"** is a selection's
  destination (`CardZoneRef::OpponentRemovedFromGame`, each side's
  `removed_from_game`), recorded as `CardRemovedFromGame`.
- Event Horizon's "[trash]: End the run. Use this ability only during a
  run against this server" composes: `Cost::TrashSelf` and Stage 3b's
  `RunAgainstThisServer`.
- **The decks.** Two Lionsmane for two Public Trail (A Thousand Cuts); two
  Event Horizon for two Tithe (Hostile Bid); two Ansel 2.0 for two Ansel
  1.0 (Retirement Package). Every card replaced is in another deck.
- **Fidelity limit:** Ansel 2.0's "break up to 2" breaks two when two are
  left; breaking fewer is never better for the Runner.
- **DSL ratio (`pool_status.py`): 21 of 71 `Effect` variants
  single-use, 2 unused**, over 225 card files — unchanged. The growth went
  into what a card costs (`Cost`, two words), whether it may (one
  requirement) and where a card goes (one zone).
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included. `coverage_identical.py origin/main` (192 games a shape, seed
  1): **the random seatings move, and all of it is the N-Pot fix.**
  `effects_seen/BreakSubroutinesUnconditionally` 16 → 10 by view and by
  index — six breaks a random Runner had made with an N-Pot on some other
  piece of ice — and the games after them branch (Runner agenda wins
  111 → 112, deck-outs 6 → 5). Checked, not inferred: this branch with
  only N-Pot's file reverted is **identical** to `main` in both random
  seatings, so the two costs, the requirement on Ansel 2.0 and the new
  zone move nothing else. The heuristic seatings move (Corp wins 82 → 88
  of 192), N-Pot and the `determinize` movement of three new cards
  together; not separated.

#### Stage 5a — credits during runs, a spend from outside the pool, a next turn's clicks (26 September 2026)

`feat/vp-stage-5a-methuselah-touchstone-shackleton-caveat`: Methuselah,
Touchstone, Shackleton Grid, Caveat Emptor. **No new `Effect`.** Vantage
Point 45 of 66; `VP_UNIMPLEMENTED` 25 → 21.

- **"You can spend hosted credits during runs"** is a `PaysFor` word
  about *when* (`DuringRuns`, Methuselah and Touchstone): CR 1.10.4c lets
  such credits pay for any purpose while the period runs. During a run
  the pool is classed as broad as the credit pool (`payment::class_of`),
  so Cyberfeeder's credit is spent before Methuselah's without a question.
- **A spend from outside the credit pool is a moment**
  (`GameEvent::CreditsSpentFromOutsidePool`, once per payment;
  `Trigger::OnCreditsSpentOutsidePool`, Shackleton Grid). It is about the
  server of the run it was made in, so "during a run against this server"
  is `Subject::This`. Only the Runner's spend during a run is a moment; any
  other spend is an occurrence of nothing. The payer dispatches it after
  what it paid for. Five sites that did not dispatch their cost's events
  now do: a text install, a steal cost, an access trash, a paid access
  interaction and the Runner's trace bid. The dispatch audit named the
  trace bid.
- **Two affordability sums gone:** the steal-cost and access-interaction
  checks counted the credit pool alone, which refused a steal that bad
  publicity's credits would have paid for. They ask `payment::available`.
- **`OnOperationPlayed` is `OnCardPlayed`** and hears events too.
  Touchstone could not listen to `OnPlay`, the play's own resolution step,
  which nobody orders. The Corp only plays operations, so Nebula Talent
  Management and Building a Better World hear what they did.
- **A change to a next turn's allotment is a lingering effect**
  (`Lingering::AllottedClicks` until `Until::NextTurnOf(side)`, taken as
  CR 1.11.2's first step assigns the clicks). `GainClicksNextTurn(Side,
  u32)` became `AllottedClicksNextTurn(Side, i32)`, which Aggressive
  Trendsetting and Caveat Emptor both use. It was
  `CorpState::extra_clicks_next_turn`, a field no view carried, so every
  bot sample after Trendsetting planned the Corp a click short. For the
  Runner it was a silent no-op.
- **The decks.** In Safety Net, two Methuselah for two T400 Memory Diamond
  and three Touchstone for three Smartware Distributor. In Hostile Bid,
  two Shackleton Grid for two Regolith Mining License. In Retirement
  Package, two Caveat Emptor for two Hedge Fund. Every card replaced is in
  another deck.
- **Fidelity limit:** Shackleton Grid's "you may" always fires. Declining
  4 meat damage is never better. A declined once-per-turn ability is not
  used (CR 9.3.6g), and a requirement spent on resolution cannot say that.
- **DSL ratio (`pool_status.py`): 20 of 71 `Effect` variants
  single-use, 2 unused**, over 229 card files, down from 21:
  `AllottedClicksNextTurn` has two cards.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included. `coverage_identical.py origin/main --expect-renames
  OnOperationPlayed=OnCardPlayed` (192 games a shape, seed 1): **the
  random seatings play the same games.** They differ only in the renamed
  effect and in the new event now being counted, 50 times by view and by
  index. The heuristic seatings move: Corp agenda wins 73 → 59 of 192,
  Runner 103 → 117. Checked, not inferred: this branch without the four
  card files (engine changes kept) is identical to `main` in all four
  shapes apart from the new event (17 in heuristic-view). So all of the
  heuristic movement is the new cards entering `determinize`'s hidden
  pools; the view now carrying Trendsetting's click moved nothing.

#### Stage 5b — a count on a moment, a Trojan's server (26 September 2026)

`feat/vp-stage-5b-stowaway-nurse-hanh`: Stowaway, Nurse Hạnh. **No new
`Effect`.** Vantage Point 47 of 66; `VP_UNIMPLEMENTED` 21 → 19.

- **A breach of Archives turning facedown cards faceup is a moment**
  (`GameEvent::ArchivesTurnedFaceup { count }`, CR 7.3.2;
  `Trigger::OnArchivesTurnedFaceup`, Nurse Hạnh). It is about a number of
  cards (`TriggerAbout::Cards`, `listeners::About::Cards`), and "2 or
  more" is `EventFilter::AtLeast(2)` in the trigger condition, where the
  printed sentence puts it. The turn log holds no number, so a first time
  narrowed by one is refused. It is dispatched once the first access is
  offered, so a reaction that parks waits beside the Runner's choice of
  card and is not overwritten by it.
- **A Trojan's "this server" is the server its host ice protects**
  (`active::runner`: a hosted program's `server` is its host's). Stowaway's
  "whenever you make a successful run on this server" is then
  `Subject::This`, as it is for an upgrade. Checked: the test fails with
  the rig card in no server.
- **The decks.** In Pay as You Go (Noise mills R&D facedown into
  Archives), two Nurse Hạnh for two Smartware Distributor. In Safety Net,
  two Stowaway for two Creative Commission. Every card replaced is in
  another deck.
- **The sweep found a bot blind spot, fixed first in its own PR (#233).**
  The 256-seed view sweep stalled at seed 120 (Paid Content against
  Borrowed Time, heuristic Runner). With a rezzed Magistrate Revontulet
  and no credits, the one-ply Runner priced faceup agendas in Archives as
  certain steals and ran Archives four times a turn until the game ran
  out of steps. The evaluator now asks the steal price
  (`continuous::steal_price`, the question the access asks). That fix is
  identical to `main` in all four shapes. Neither deck in that game
  changed on this branch, which only reached the position by another
  trajectory.
- **DSL ratio (`pool_status.py`): 20 of 71 `Effect` variants
  single-use, 2 unused**, over 231 card files.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included. `coverage_identical.py` against #233 (192 games a shape, seed
  1): **the random seatings play the same games**, differing only in the
  new event now being counted (194 by view and by index). The heuristic
  seatings move: Corp agenda wins 59 → 74 of 192. Checked, not inferred:
  without the two card files the branch is identical to its base in all
  four shapes apart from the event count (120 in the heuristic seatings),
  so all of the heuristic movement is the new cards in `determinize`'s
  pools.

#### Stage 5c — "that program", through the run it starts (27 September 2026)

`feat/vp-stage-5c-beta-build`: Beta Build. **No new `Effect`.** Vantage
Point 48 of 66; `VP_UNIMPLEMENTED` 19 → 18.

- **A text install names what it installed.** After
  `InstallRunnerCardFromGripWithDiscount` installs the resolving card, the
  rest of the resolution means that install (`ResolutionContext::
  acting_install`). The server prompt already carries the handle through
  the park (`PendingDecision::ChooseServer::source_install`), and so does
  `SetRunEndedEffect`, so "when that run ends, if that program has not
  been uninstalled" needs no field and no placeholder. Uninstalled, the
  program has no handle; reinstalled, it has another. Either way nothing
  moves.
- **`AddToBottomOfDeck` is `AddToDeck(DeckEnd)`** (five cards), and it
  takes the acting install out of the rig. `GameEvent::CardAddedToBottomOfDeck`
  is `CardAddedToDeck { top, revealed }`. `revealed` says whether the card
  came out of a zone both players see (the heap, the rig), so the opponent
  may be told which card now sits in the stack. Out of the grip the event
  is its owner's. It had been shown to the Corp whatever the zone. The
  view sweep's log check caught Beta Build's return to the stack and now
  reads the flag, as it reads a revealed `CardsSelected`.
- **"Ignoring all costs"** is a discount (`dsl::Discount::AllCosts`, with
  `Credits(n)` for Illumination and Topan), shared by the install and its
  offer (`CardFilter::InstallableRunnerCardWithDiscount`). The memory
  limit still applies, since it is not a cost.
- **"Non-virus" is `CardFilter::Not`**, definition-level.
- **"Run any server" happens whatever the search finds.** The run with the
  rider is in the search's continuation, and a plain run is the fallback
  when the stack holds no program to find. The fallback comes first,
  because it is read before the search takes anything.
- **The decks.** In Safety Net, two Beta Build for two Diesel (one Diesel
  stays; it is in no other deck).
- **Fidelity limit:** a Trojan is not offered. A text install cannot place
  one on ice yet (`InstallRunnerCardFromGrip`'s exclusion), so finding one
  would install nothing.
- **A test that checked nothing past 200 lines (#235).** The client's log
  concealment test read each action's lines as the slice the capped log
  grew by. Past `MAX_LOG_LINES` the slice was empty, so five of its six
  games went mostly unchecked, and its "the Corp advances" precondition
  hung on one seed, which this card's movement broke. It now checks every
  line.
- **DSL ratio (`pool_status.py`): 20 of 71 `Effect` variants
  single-use, 2 unused**, over 232 card files.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included. `coverage_identical.py origin/main`: **the random seatings
  play the same games**, differing only in the renamed effect and event
  (`AddToBottomOfDeck` 36 → `AddToDeck` 36, the event 8 → 8). The
  heuristic seatings move slightly (Corp flatline wins 12 → 10, Runner
  deck-outs 0 → 2). Checked: without Beta Build's file the heuristic
  seatings are byte-identical to `main`.

#### Stage 5d — who trashed it, and a look nobody else sees (27 September 2026)

`feat/vp-stage-5d-hiram`: Hiram "0mission" Svensson: Shadow of the Past,
on the new Spare Parts Sweep deck. **One new `Effect`**
(`LookAtTopOfDeck`). Vantage Point 49 of 66; `VP_UNIMPLEMENTED` 18 → 17.
Stage 5 is complete.

- **A trash says who carried it out** (`GameEvent::CardTrashed::by`, CR
  1.14.5a: a condition about an effect a player performs is met only when
  that player carries it out). Assigned by the site that trashes:
  - text: the controller of the card resolving (`ability::carried_out_by`);
  - a selection: its chooser;
  - a cost: its payer;
  - an install's like-card trash: the player installing (CR 8.5.6);
  - the Corp's basic action against a tagged Runner's resource: the Corp;
  - the rules' trashes are `None`: a host's hosted cards, the console
    limit and the unique rule (CR 3.8.5b, 10.1.1: "are trashed"), and a
    card emptied of its hosted credits in the middle of a payment.
  A parked trash carries it too (`WouldHappen::Trash::by`).
- **`Trigger::OnCardTrashed`** is heard by the player who carried the
  trash out (`Hears::OwnSide`), about the card; a `None` trash is an
  occurrence of nothing. Every other trash is dispatched where it happens
  (`ability::dispatch_trashes`) — effects, selections, a prevented-or-not
  parked trash, an install's trash, the basic action — and a cost's by its
  payer, as before. The dispatch audit found no site left out across both
  256-seed sweeps. The event does not say whether the card was installed,
  so `validate` refuses an `InstalledCard` filter on the trigger. The turn
  log counts it without the card's type (`turn_log::concealed`): a Corp
  card trashed from HQ goes facedown.
- **"Look at the top card of R&D"** is `Effect::LookAtTopOfDeck { deck,
  count }` and `GameEvent::CardsLookedAt`, masked for everyone but the
  looker. Neither player is told R&D's order, so the Corp is not shown it.
  Composition didn't work: a selection over the deck shows its chooser the
  cards, but it is a prompt to answer and a card to move. The view sweep's
  log check reads a look in a seat's own entry as that seat's to know.
- **Hiram** is two entries, one printed ability: `OnCardInstalled` and
  `OnCardTrashed`, each `when: Card(CardType(Hardware))`.
- **The deck.** Spare Parts, a Shaper Sweep deck on Hiram: hardware to
  install (T400, DZMZ, GAMEDRAGON™ Pro, Touchstone) and to trash
  (Methuselah's price as a run begins, and a second console beside it).
  Eternal and Casual, for its Core Set cards.
- **Fidelity limits.** A card that names another player to carry out its
  trash (Noise's "the Corp trashes the top card of R&D") is read as its
  controller's. Damage takes its cards as discards (`CardDiscarded`), so a
  hardware lost to damage the Runner is responsible for is not heard. The
  look is in the Runner's log, not in their view, so a bot's sample does
  not keep the card on top of R&D.
- **DSL ratio (`pool_status.py`): 21 of 72 `Effect` variants
  single-use, 2 unused**, over 233 card files (20 of 71 before).
  `LookAtTopOfDeck` is the one, with its reason on the variant; the
  vocabulary it adds (`by`, a trigger) is what the next "whenever you
  trash" card reuses.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included. `coverage_identical.py origin/main` is **identical in all four
  shapes**: Hiram is an identity, which `determinize` never samples, and
  Spare Parts is a Sweep deck, so no game in the pool hears the new moment.
  Twenty-four random games of Spare Parts against Hostile Bid (seed 1)
  show it heard: Hiram's install entry fired 47 times and its trash entry
  18, for 65 looks, beside Methuselah (203), Shackleton Grid (3) and Beta
  Build (3).

#### Stage 6a — arrange, a reveal, and the end of an encounter (27 September 2026)

`feat/vp-stage-6a-arrange-reveal-encounter-end`: Cultivate, Knowledge
Seeker and Esca, on the A Thousand Cuts Sweep deck. **No new `Effect`.**
Vantage Point 52 of 66; `VP_UNIMPLEMENTED` 17 → 14.

- **Arranging the top of a deck is a selection back onto it** (CR 8.3.3).
  A selection put onto a deck now lands with the first card chosen on top
  (it was pushed in the order chosen, so the first went deepest; every
  card in the pool had put at most one card there). Cultivate is three
  `PromptChooseCards` over `TopOfZone`: one of the top five to Archives,
  one of the top four to HQ, and up to three back onto R&D. Knowledge
  Seeker's second subroutine is the last of those for four. A card left
  unchosen stays beneath the chosen ones, which is one of the
  arrangements too, and with fewer cards in R&D the prompt still asks.
  Composition worked, so no `Effect` was added.
- **A card taken from a deck is the copy at the place chosen.** It was the
  first copy from the bottom, so choosing the top card when a lower copy
  of the same card was in R&D left the chosen card on top (AU Co.,
  Poétrï, Embedded Reporting, and now Cultivate). Neither this nor the
  new top-first order applies to a selection that shuffles afterwards: a
  shuffle's result depends on the order it starts from, and doing either
  first re-dealt every shuffled R&D in the pool (Sprint, Sleipnir, Next
  Big Thing and five more).
- **An encounter's end is a moment:** `GameEvent::EncounterEnded`, heard
  by `Trigger::OnEncounterEnded` about the ice. An encounter ends when it
  completes (CR 6.9.3e), when "end the run" ends it with the run
  (CR 6.1.4), when a subroutine moves the run, when a jack-out is paid
  as a cost, and when the ice leaves the table or is derezzed during it
  (`run::encounter_ends`, asked at each). `reconcile_ice` now dispatches
  the pass and the end it makes itself; two of its three callers handed
  them on undispatched. Knowledge Seeker's third virus counter purges
  every virus counter in play, the Runner's included, and derezzes it.
- **A reveal while accessed** (CR 1.21.7): `ContinuousKind::
  RevealedWhileAccessed` with `while: AccessingIn(RnD)`. It is a static
  ability of the card's, so a standing kind with `Scope::This` rather
  than a word on the access. Esca, Snare! and Byte! all print it; the
  latter two did not say it until now. The access emits
  `GameEvent::CardRevealed`, public, and the Corp's view names the card
  for the rest of the access (CR 1.21.6). `AccessingArchives` became
  `AccessingIn(ServerId)` when a second server was printed.
- **What the sweeps found.**
  - The log concealment check found Esca's trigger named to a spectator
    when Esca is accessed from HQ or R&D: it is the first card to fire
    from a hidden zone. It is now the Runner's and the Corp's only.
  - The same check found Knowledge Seeker's encounter events named to a
    spectator after it derezzed itself in the action that encountered
    it. Encounter events of ice now concealed from the viewer are dropped
    for that viewer. The Runner remembers the ice (`seen_by_runner`, set
    at the rez), so only a spectator loses them.
  - The client's trap classifier (`board::rez::gains_nothing`) read
    Snare!'s new standing effect as a reason to rez it. The reveal is
    about the access, so the classifier ignores it, and Esca joins the
    traps.
- **Deck.** A Thousand Cuts: two Knowledge Seeker for two Palisade, two
  Esca for two Regolith Mining License, two Cultivate for two
  Retribution.
- **Fidelity limits.** The client does not yet word an arrangement as
  "top first"; the prompt is a card selection. The arranged cards are not
  made new objects (CR 8.3.3): nothing in the engine remembers a card in
  R&D by identity.
- **DSL ratio (`pool_status.py`): 20 of 72 `Effect` variants single-use,
  2 unused**, over 236 card files (21 before: Knowledge Seeker is the
  second card to purge). The growth is one `Trigger`, one
  `ContinuousKind`, and one `EffectRequirement` generalised in place.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included. `coverage_identical.py origin/main` differs in all four
  shapes, and each difference was attributed with a temporary ref:
  - **Random seatings** moved only through the copy taken from the top
    of a deck. With that one line reverted, both random shapes differ
    from main only in the new events' counts (`EncounterEnded` 0 → 911,
    `CardRevealed` 0 → 4). On HEAD, the Runner's wins on agenda points
    go 112 → 113 and on the Corp decking 5 → 4.
  - **Heuristic seatings** moved only through `determinize` sampling the
    three new cards. With the card files removed, they differ from main
    only in event counts. They are unmoved by the top-copy fix.
- **Real play.** Twenty-four random games of A Thousand Cuts against Pay
  as You Go (seed 1): Esca's trigger fired 31 times, Cultivate 8,
  Knowledge Seeker's derez once, over 40 ended encounters and 7 reveals.

#### Stage 6b — credits only from stealth cards (27 September 2026)

`feat/vp-stage-6b-stealth-credits`: Corsair, Lampades and Baker, on the
Safety Net Sweep deck. **No new `Effect`.** Vantage Point 55 of 66;
`VP_UNIMPLEMENTED` 14 → 11.

- **"Spend credits only from stealth cards"** (CR 1.10.4b, read from the
  payer's side) is `Cost::CreditsFrom { amount, from }`: `payment::sources`
  takes a `from` filter, and only a pool on a card it matches pays —
  hosted credits by their host, the run's own by the card that began the
  run (`RunState::initiated_by`, which Aircheck will fill). Bad publicity
  is on no card and the credit pool is not a card, so the pool stays the
  last source holding nothing: the first payment that cannot reach the
  credit pool. Affordability and paying read the one scan, as the Payment
  Rule has it. A cost rather than a word on the ability, because the
  printed sentence limits the credits and a cost is where the credits
  are; `amount` is an `Amount` because Lampades's price is "the printed
  rez or play cost of the card you are accessing"
  (`Amount::AccessedCardPrintedCost`).
- **Corsair** breaks barrier subroutines for 1[credit] from anywhere, and
  its −3 strength (`ModifyStrength` for the encounter) is paid from
  stealth cards and offered only against a barrier
  (`EffectRequirement::Encountering(IceType)`, printed or gained, asked
  as a restricted break is).
- **Lampades** is an access ability (`AbilityDef::access`, as Gourmand):
  a hosted power counter and the accessed card's printed cost in stealth
  credits trash it (`TrashCurrentlyAccessedCard`), and not an agenda,
  which prints neither cost.
- **Baker**'s "[click]: Run Archives" is `InitiateRun`, and its "when you
  would approach Archives, you may pay 1[credit] to instead change the
  attacked server to HQ or R&D" is a paid ability usable when the run it
  began has passed all its ice and is about to approach Archives
  (`ThisCardStartedTheRun`, `AboutToApproach(ServerId)`), whose choice is
  Maintenance Access's `RedirectRunOnApproach`.
- **Deck.** Safety Net (Kate, 15 influence, now all spent): two Corsair
  for two Corroder, Lampades for Mayfly, Baker for a Jailbreak, beside
  its Methuselah and Touchstone, the stealth cards that pay for them.
- **Fidelity limits.** Baker's credit is paid in the last paid ability
  window before the approach (CR 6.9.4e) rather than at it, so the Corp
  acts in that window after the Runner has chosen. Nothing in the pool
  cares, but a card that did would see the order.
- **DSL ratio (`pool_status.py`): 19 of 72 `Effect` variants single-use,
  2 unused**, over 239 card files (20 before). The growth is one `Cost`,
  one `Amount` and three `EffectRequirement`s.
- **What the sweeps found.** The 256-seed view sweep stalled at seed 155
  (Fine Print against Safety Net): Doomscroll's tag went through the
  prevention window, NBN: Reality Plus heard it and parked its "gain 2
  or draw 2", and once that was answered nothing resumed the ice's later
  subroutines. `prevention::finish` now hands "the subroutines are not
  finished" to whatever the prevention let through parked. The code was
  the same on main, where a test reproduces it: fixed in its own PR,
  #239, identical to main in all four shapes.
- **Measured.** Both sweeps at 256 seeds are green, coverage gate
  included. `coverage_identical.py origin/main`: both random seatings
  **identical** (Safety Net is a Sweep deck, and the payment change moves
  nothing no card limits). The heuristic seatings moved, and with the
  three card files removed they are identical in all four shapes:
  `determinize` sampling the new cards is the whole of it.
- **Real play.** Ninety-six random games of Safety Net against Hostile Bid
  (seed 2): Corsair's and Lampades's stealth-paid abilities were used 3
  times each, Baker's 46, with 2 runs redirected to HQ or R&D. Stealth
  credits are rarely on hand in random play, which is why the two are
  rare.

#### Stage 6c — a locked credit pool (27 September 2026)

`feat/vp-stage-6c-aircheck`: Aircheck, on Safety Net. **No new `Effect`.**
Vantage Point 56 of 66; `VP_UNIMPLEMENTED` 11 → 10. Stage 6 is complete.

- **Composed.** "Place 4[credit] on this event … you can spend hosted
  credits" is Overclock's shape: `PromptChooseServer` with
  `bonus_run_credits: 4`, the run's own pool, whose card is the event that
  began the run (`RunState::initiated_by`). Aircheck is a stealth card, so
  Corsair, Lampades and Baker may spend those credits (6b's `from`).
  "Run HQ or R&D" is `allowed_servers`. "When that run ends, if it was
  successful, you may run a remote server" is an `on_success` that arms a
  `SetRunEndedEffect`: the run-end rider is set only once the run has
  succeeded, so no condition on the run's end is needed.
- **"You cannot lose or spend credits from your credit pool"** is
  `Prohibition::SpendOrLoseCreditPool`, made by the run's `on_start` for
  the run (a lingering effect, which the view carries, so a bot's sample
  sees the lock). `payment::sources` then gives the credit pool nothing,
  as it does for a payment limited to stealth cards, and
  `Effect::LoseCredits` takes nothing from it. Those are the only two
  places a Runner's credit pool goes down.
- **`PromptChooseServer::only_in: Option<ServerKind>`**: "a remote
  server", the remotes on the table. The fresh remote that "any server"
  offers is not one, and with no remote the offer fails rather than park,
  so the "may" is left with its decline.
- **Fidelity limits.** The event is taken to be active for the run it
  makes and no longer, so the second run is an ordinary one: its credits
  are the credit pool's, and the event's unspent credits are gone with
  the first run.
- **DSL ratio (`pool_status.py`): 19 of 72 `Effect` variants single-use,
  2 unused**, over 240 card files (unchanged). The growth is one
  `Prohibition` and one offer field.
- **Measured.** Both sweeps at 256 seeds are green. Against the 6b branch,
  `coverage_identical.py` has both random seatings **identical**. The
  heuristic seatings moved, and without Aircheck's card file they are
  identical in all four shapes (`determinize`).
- **Real play.** Ninety-six random games of Safety Net against Hostile Bid
  (seed 2) played Aircheck 33 times.

#### Stage 7a — one door off the table (27 September 2026)

`feat/vp-stage-7a-luana-campos`: Luana Campos, on the Hostile Bid Sweep
deck. **No new `Effect`.** Vantage Point 57 of 66; `VP_UNIMPLEMENTED`
10 → 9.

- **A Corp install leaves the table through one door**, `rules::uninstall`.
  A card is uninstalled "when it stops being installed for any reason"
  (CR 8.5.1b), and nine places each took the card out of
  `CorpState::installed` itself. Between them they are every way the pool
  uninstalls a Corp card: trashed by a card's text, after a prevention
  window, by a selection or to make room for an install; trashed or
  stolen by the Runner on access; scored; removed from the game; and
  trashed by the ◆ rule. Each still decides where
  the card goes and what leaves with it; the door only takes it off the
  table, after saying so.
- **"When this asset would be uninstalled" is a moment**,
  `Trigger::OnWouldBeUninstalled`, heard as `GameEvent::AboutToBeUninstalled`
  while the card is still in its root. A "would" is an interrupt (CR
  9.9.3d), and an interrupt is marked pending only if it is active (CR
  9.9.4b), so the door announces a card that is rezzed and prints the
  trigger and no other: an announcement nobody could hear is left out, as
  a tag or a trash about to happen is when nobody could prevent it, and a
  facedown card is never named. The ◆ rule's trash is a checkpoint's,
  which carries out no instruction and so opens no interrupt window (CR
  9.9.4); it takes the card through the door's other way out, which
  announces nothing (and which could not: the checkpoint runs inside
  `dispatch_event`, whose own checkpoint would find the older copy still
  installed). `OnCardTrashed` could not say it: it is heard after the
  card has gone, and with it the counters.
- **Hosted bad publicity is counters on the card.** "Host 1 of your bad
  publicity counters on this asset" is `RemoveBadPublicity(1)` and
  `AddCounters(1)`, behind `AmountAtLeast(BadPublicity, 1)` so that
  nothing is asked with none to host (`Amount::BadPublicity`, the
  `RunnerTags` of the other side). "Take all hosted bad publicity" is
  `GiveBadPublicity(HostedCounters)`: the effect takes an `Amount` now, as
  `GiveTags` came to, and taking none dispatches nothing, so Editorial
  Division never hears a Luana that hosted nothing.
  `CounterKind::BadPublicity` names the counters for the view; both
  clients word them and the desktop badges them with the bad publicity
  readout's mark.
- **Deck.** Hostile Bid: two Luana Campos for two PAD Campaign. Its three
  Hostile Takeovers are what she has to host.
- **Fidelity limits.** "Uninstalled" for a Corp card only: no Runner card
  in the pool interrupts its own uninstalling (Nanuq, in Parhelion, is the
  first that cares, and its words are a replacement, "remove it from the
  game"). The interrupt resolves in the dispatch that announced it; a
  second card of the Corp's that heard the same moment would be ordered
  with it as simultaneous triggers are, but no other card does.
- **DSL ratio (`pool_status.py`): 19 of 72 `Effect` variants single-use,
  2 unused**, over 241 card files (unchanged). The growth is a `Trigger`
  (37 → 38), an `Amount`, a `CounterKind` and an event.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has both random seatings **identical**. The
  heuristic seatings moved, and without Luana's card file (and the deck
  swap) they are identical in all four shapes (`determinize`).
- **Real play.** Ninety-six random games of Hostile Bid against Safety Net
  (seed 2) installed Luana 94 times and rezzed her 79; 48 rezzed Luanas
  were announced on their way out, and one handed a hosted bad publicity
  back to the Corp.

#### Stage 7b — cards hosted facedown (27 September 2026)

`feat/vp-stage-7b-runner-hosting`: Read-Write Share, on the Spare Parts
Sweep deck. **One new `Effect`**, `ShuffleHostedIntoDeck`. Vantage Point
58 of 66; `VP_UNIMPLEMENTED` 9 → 8. Hackerspace, planned beside it, is a
different mechanic (an install onto a host, which grows the action
space) and is Stage 7c.

- **A card hosted facedown is its owner's to see** (CR 1.13.7, 1.21.2a).
  `CardDefinition::hosts_facedown` says so of the host, and the mask reads
  it: the Corp's and a spectator's view of the host carries
  `hosted_unseen`, a count, where the Runner's carries the cards. Nothing
  is seeded onto the install, because every reader holds a registry. The
  hosting itself is a `PromptChooseCards` from the grip onto
  `HostedOnSource`, whose `CardsSelected` is already shown only to its
  chooser. `determinize` draws the cards a Corp sample cannot see from
  the Runner's unseen pool, as it draws a grip, and the observation counts
  them with the cards it can.
- **"Limit 4 hosted cards"** is `Not(AmountAtLeast(HostedCards, 4))` on
  both triggers, beside a grip with a card in it (`Amount::HostedCards`,
  cards where `HostedCounters` counts counters).
- **Cards hosted on a card its own cost trashes are set aside, not
  trashed** (CR 9.5.5), when the effect acts on them. "[trash]: Shuffle
  all hosted cards into your stack" took them to the heap with the
  program, and a "whenever you trash" would have heard each one.
  `engine::activate_ability` now takes them off the host before paying a
  cost that `uninstalls_its_source`, when the effect `acts_on_hosted_cards`,
  puts them on the resolution (`ResolutionContext::set_aside`), and trashes
  whatever the effect leaves. The context, not the state, because the one
  effect that reads them never parks.
- **Why a new `Effect`.** `PromptChooseCards` over `HostedOnSource` finds no
  host once the cost has trashed it, and asks a question "all" does not;
  `AddToDeck` moves the acting card. `ShuffleHostedIntoDeck` records no
  event: which cards is the Runner's to know, and the stack's count says
  how many.
- **Deck.** Spare Parts: two Read-Write Share for two Creative Commission.
- **Fidelity limits.** The cards are not "distinct groups … freely
  arranged" (CR 1.13.7d) in any sense the view shows: they are a list in
  the order hosted.
- **DSL ratio (`pool_status.py`): 20 of 73 `Effect` variants single-use,
  2 unused**, over 242 card files (was 19 of 72).
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has both random seatings **identical**. The
  heuristic seatings moved, and without Read-Write Share's card file (and
  the deck swap) they are identical in all four shapes (`determinize`).
- **Real play.** Ninety-six random games of Spare Parts against Hostile
  Bid (seed 2) installed Read-Write Share 57 times, hosted a card on
  install 53 times and trashed it for the shuffle 56 times; the random
  Runner trashes it before its turn-start trigger comes round.

#### Stage 7c — a resource installed onto a resource (27 September 2026)

`feat/vp-stage-7c-hackerspace`: Hackerspace, on the Pay as You Go Sweep
deck. **No new `Effect`.** Vantage Point 59 of 66; `VP_UNIMPLEMENTED`
8 → 7.

- **An install onto a card is a destination** (CR 8.5.1a, declared with
  the rest of it at 8.5.16b): `PlayerAction::InstallResource` carries a
  `host`, the rig card it goes onto (`InstalledRunnerCard::
  hosted_on_rig_card`, which was `hosted_on_program` until a resource
  hosted one). A field, as `trash_first` is, because it is the same
  install to another place. `ActionSpace` 2621 → 3133, **appended**: hand
  slot by rig slot, as `InstallProgramOnIce` is by the Corp's installs,
  so every recorded index keeps its meaning and a policy needs a wider
  head. The hosted card leaves with its host, as GAMEDRAGON™ Pro did.
- **Which cards may go there is a standing permission of the host's**,
  `ContinuousKind::MayHost`, about the cards a new scope admits,
  `Scope::InstallingOntoThis(filter)`. The same scope carries "each
  resource installed this way costs 1[credit] less", so the permission and
  the discount name the cards once each, in the card's own words. The
  scan is asked of a target that knows the host (`Target::InstallingOnto`):
  `continuous::may_install_onto` for the action list and the install, and
  `install_cost_onto` for the price, where `install_cost_of` is the same
  question with no host. `CardFilter::Unique` says "unique (♦)".
- **"While this resource has a hosted companion and a hosted connection"**
  is `EffectRequirement::HostsInstalled(filter)`, twice under `And`, on a
  `HandSize` of +2. **No unique companion is in the pool yet**, so the
  sweeps cannot reach it; the card test makes one.
- **Both clients** word the install ("Install Nurse Hạnh onto
  Hackerspace"), and the desktop's rig cards are drop places, so a card
  carried onto a host that takes it installs there; the smallest lit
  place under the pointer wins, so everywhere else in the rig is still
  the plain install.
- **Deck.** Pay as You Go: two Hackerspace for two T400 Memory Diamond,
  and a third Nurse Hạnh (a unique connection) for Smartware Distributor.
- **DSL ratio (`pool_status.py`): 20 of 73 `Effect` variants single-use,
  2 unused**, over 243 card files (unchanged). The growth is a kind, a
  scope, a filter word and a requirement.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has both random seatings **identical**, the
  index path included, so the wider `ActionSpace` moved no index a
  random agent draws from. The heuristic seatings moved, and without
  Hackerspace's card file (and the deck swap) they are identical in all
  four shapes (`determinize`).
- **Real play.** Ninety-six random games of Pay as You Go against
  Hostile Bid (seed 2) installed Hackerspace 26 times and installed Nurse
  Hạnh onto it 8 times.

#### Stage 7d — the score area (27 September 2026)

7 → 4.

- **A card added to a score area "as an agenda"** (CR 10.1.3) keeps
  nothing it printed and has only what the addition gives it:
  `dsl::AsAgenda`, its points and whether "You cannot forfeit this
  agenda.", carried on the copy as `ScoredAgenda::as_agenda`. So it is not
  active (`rules::active`: Word on the Street in the Corp's score area is
  a −1, not a resource), offers no ability, and was not scored (CR
  1.17.3f): `GameEvent::AddedToScoreAreaAsAgenda` is an occurrence of
  nothing. Myōshu adds itself with `Effect::AddToScoreAreaAsAgenda`, out
  of Archives, where an operation is filed before its text resolves. A
  forfeit counts only what may be forfeited, in the one list its
  affordability and its payment read.
- **A score is signed.** Word on the Street is worth −1, so
  `AgendaPoints`, the view's points and the end-of-game tally are `i32`,
  and the win check, the threat level and a client's number read one sum
  (`rules::score`, CR 1.17.1). A printed agenda is still never worth
  less than 0.
- **An additional cost to score** is `ContinuousKind::ScoreCost(Cost)`
  about `Scope::Scoring(filter)`, asked of the install being scored
  (`continuous::Target::Scoring`), so "an agenda the Corp installed this
  turn" is read off the copy. It is paid by the Corp out of the Runner's
  rig (`Cost::AddToScoreAreaAsAgenda`: the card it is printed on), with
  the score, and a checkpoint follows before the agenda moves (CR
  1.16.10b–c). A Corp that will not pay does not score (CR 1.17.3b).
  `StealCost` stayed a number: what is paid there is credits.
- **"Installed this turn" and "scored this turn" are read off copies,
  including scored ones.** A scored agenda keeps whether it was
  installed the turn it was scored (`ScoredAgenda::
  installed_on_scoring_turn`); `CardFilter::InstalledThisTurn` (the
  positive twin, since `Not` is definition-level) and `ScoredThisTurn`
  read it, in a selection (Myōshu's play requirement, a
  `ZoneHasAtLeast` over the score area) and in a trigger's `when` (Word
  on the Street's "an agenda they did not install this turn",
  `pending_choice::copy_matches`, which `listeners` and the continuous
  scan share).
- **Install only faceup** (CR 3.2.3a) is `CardDefinition::installs_faceup`,
  written as `InstalledCard::rezzed` with no rez, the flag BANGUN's flip
  already sets; the difference the rule draws is that this agenda's
  abilities are active on the table and BANGUN's are not. Because the flag
  is shared, **"rezzed" in the rules' sense is now asked, not read**
  (`InstalledCard::is_rezzed`, CR 8.1.1): a faceup agenda is neither
  rezzed nor unrezzed, so `CardFilter::Rezzed`, `Cost::Derez` and
  `Effect::DerezCard` pass it by. Before, a BANGUN-flipped agenda could be
  derezzed.
- **"The first time each turn you advance this agenda"** is counted on
  the copy. The turn's log counts classes, so a first time about one card
  had nowhere to be counted, and `validate` refused it. A Corp install
  now keeps `turn_log::CopyTurn` (dated, never reset), bumped at the same
  door as the log (`turn_log::record`) for the triggers a card can ask
  about per copy — advancing, and so far nothing else — and the verdict
  rides on `AsOf` as the turn's does. Not in the view: a bot's sample
  starts each copy's turn afresh, as it does `installed_this_turn`.
- **"You may remove 1 hosted advancement counter to do 1 meat damage"**
  is an `OfferPaidChoice` over `Cost::RemoveAdvancementCounters`, behind
  `And(OncePerTurn, Not(RunAgainstThisServer))` for "on another server";
  `GameEvent::AdvancementCountersRemoved` says it.
- **Decks.** Hostile Bid: two Sacrifice Zone Expansion for two Off the
  Books (the points unchanged), two Myōshu for a Hedge Fund and a
  Measured Response. Borrowed Time: two Word on the Street for two Sure
  Gamble.
- **DSL ratio (`pool_status.py`): 20 of 74 `Effect` variants single-use,
  2 unused**, over 246 card files. The one new `Effect` is Myōshu's; the
  rest of the growth is a kind, a scope, two costs and two filter words.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has both random seatings **identical**. The
  heuristic seatings moved, and without the three card files (and the
  deck swaps) they are identical in all four shapes (`determinize`). The
  two heuristic seatings now give one report where they gave two: on
  `main` the index path parted from the view in a single game (one
  `PurgeVirusCounters`), which the new trajectories no longer reach.
- **Real play.** Ninety-six random games of Hostile Bid against Borrowed
  Time (seed 2) installed Sacrifice Zone Expansion 102 times, paid its
  first advance 43 times and spent a counter on meat damage 16 times, and
  Word on the Street paid out 4 times. With the heuristic Corp, Myōshu was
  played 52 times and Sacrifice Zone Expansion's first advance paid 166
  times. **No Corp ever scored an agenda installed that turn with Word on
  the Street out**, so its cost is reached by the card test alone.

#### Stage 7e — a subroutine gained for an encounter (27 September 2026)

4 → 3.

- **Ice gains a subroutine** with `Effect::GainSubroutine`, said by an
  `OnEncounter` trigger that acts on its subject (Stick and Poke's "it
  gains “[subroutine] Do 1 net damage. The Runner draws 1 card.”, before
  its other subroutines, for the remainder of that encounter"). `validate`
  refuses it anywhere else, where it would find no encounter. Gained from
  another card's ability and said to come first, it is ordered ahead of
  every other subroutine, the newest first (CR 9.8.3a); the other
  categories of 9.8.3 wait for a card that prints them.
- **It lives in the run's list for the encounter**, where each
  subroutine's status already lives, not as a lingering effect: a status
  has to be broken, fired and counted toward "fully broken" with the rest.
  It is written at the front with every `id` renumbered, because a break
  and a firing name a subroutine by its place, and marked
  `EncounteredSubroutine::gained`, which `run::engine::enter_movement` —
  the one step every encounter the run survives leaves by — drops. Ice
  fully broken before it gains one stays fully broken (CR 6.5.7d).
- **Three client sites read the card's printed list by index** and would
  have named the wrong subroutine once one stood ahead of it: the tile's
  facts, the run trail's words for a break or a firing, and the
  click-break label. Each reads the run's own list now.
  `GameEvent::SubroutineGained` tells the log what the ice gained; it is
  an occurrence of nothing a card hears.
- **Deck.** Pay As You Go: two Stick and Poke for two Wildcat Strike.
- **DSL ratio (`pool_status.py`): 21 of 75 `Effect` variants single-use,
  2 unused**, over 247 card files — the one new `Effect`, single-use.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has both random seatings **identical**. The
  heuristic seatings moved, and without the card file and the deck swap
  they are identical in all four shapes (`determinize`). The two heuristic
  seatings part again (one report each), as they did before Stage 7d.
- **Real play.** Ninety-six random games of Hostile Bid against Pay As You
  Go (seed 2) installed Stick and Poke 42 times, and ice gained its
  subroutine 123 times. **The heuristic Runner never installs it** (0 in
  96 games), so the card gate is met by random seats alone.

#### Stage 7f — an ability used from HQ (27 September 2026)

3 → 2.

- **An ability that can only affect the game from a zone is active
  there** (CR 9.1.8b). Tocsin's "[click], 1[credit], reveal and trash this
  ice from HQ:" is `AbilityDef::from_hand`, and it has no ability on the
  table: `ActivateAbility` refuses it and does not offer it.
- **Its own action, `PlayerAction::ActivateHandAbility { card_id,
  ability_index }`**, because `ActivateAbility` names an install and a
  card in HQ has none. Which copy is `PlayOperation`'s question, with
  `PlayOperation`'s answer. It is an action (`classify_action`); `validate`
  holds every hand ability to one, the only kind the pool prints.
  **`ActionSpace` 3133 → 3261, appended**: each hand's slot by ability
  slot, HQ then the grip, so every recorded index keeps its meaning
  (`scripts/action_space_segments.py`, the gym's pin).
- **`Cost::RevealAndTrashSelf`**: the card leaves the hand revealed
  (`CardRevealed`), so it lands in Archives faceup (CR 4.4.6b), where
  `TrashSelf` out of HQ lands facedown.
- **"Up to 1 barrier and up to 1 sentry", shuffled once**, is two
  `PromptChooseCards` in a `Sequence`, only the second shuffling. Nested as
  a `then`, a search that found no barrier would never have looked for a
  sentry, because a prompt with nothing to choose resolves nothing after
  it.
- **Deck.** Hostile Bid: two Tocsin for two Enigma.
- **The coverage gate never saw the action in either 768-game deep
  sweep.** A random Corp holding Tocsin also has two dozen places to
  install it, and the rotation seldom gives Hostile Bid to a random Corp;
  the heuristic Corp installs and rezzes it (70 and 45 times in 96 games)
  and never uses it from HQ. So it is a reasoned rare action at 2048
  games, as `ChooseNumber` is, and the card test holds its `ActionSpace`
  round trip and its mask bit.
- **DSL ratio (`pool_status.py`): 21 of 75 `Effect` variants single-use,
  2 unused**, over 248 card files (no new `Effect`; a cost, a flag and an
  action).
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has both random seatings **identical**, the
  index path included, so the wider `ActionSpace` moved nothing a random
  agent draws. The heuristic seatings moved, and without the card file and
  the deck swap they are identical in all four shapes (`determinize`).
- **Real play.** Ninety-six random games of Hostile Bid against Pay As You
  Go (seed 2) used Tocsin from HQ 10 times, and it was installed 73 times.
  With the heuristic Corp, it was installed 70 times, rezzed 45, and never
  used from HQ.

#### Stage 7g — a run not declared successful, and a limit on accesses (27 September 2026)

2 → 1. Stage 7 is complete; Méliès U is the last card of the set.

- **A run is a target.** The continuous layer gains
  `continuous::Target::Run { server }` and a scope read from the card that
  prints it, `Scope::RunsOnThisServer` ("Runs against this server",
  "During each run against this server"). Two kinds the `ContinuousKind`
  doc had named as deferred come in on it, `CannotBeDeclaredSuccessful`
  and `AccessOthersAtMost(n)`, each asked by the run's own step and never
  stored. `validate` holds both to that scope on a card in a root.
- **Success withheld, breach kept** (CR 6.9.5a–b, 6.8.4a). `CompleteRun`
  still commits past the approach; when a card says runs there cannot be
  declared successful, it emits `GameEvent::RunNotDeclaredSuccessful` in
  place of `RunSucceeded` — so nothing that hears a successful run hears
  it, and the turn log does not count one — drops Account Siphon's "if
  successful, instead of breaching", and breaches at once. Nothing can
  park in between, because no success was dispatched, so the run state
  gains no field. The run trail remembers the difference, since the view's
  phase cannot tell it.
- **The access limit is CR 7.4.2b**, asked where the breach already prunes
  its candidates (`run::access::prune_candidates`): once the Runner has
  accessed `n` cards other than the Flagship, every candidate but the
  Flagship stops being one. It does nothing before the first access,
  never changes the random access limit, and is asked afresh at each
  offer (7.4.2a). **Persistent** is the existing persistent-upgrade path,
  whose server-only sources now read `RunsOnThisServer` too. Flagship's
  flag is card-wide, which is exact: its other ability is spent before
  the breach in which it could be trashed.
- **Found on the way: an unrezzed persistent upgrade persisted.** An
  access-trash recorded any card with the flag, rezzed or not; CR 9.12.5a
  keeps only a rezzed card's persistent abilities. AMAZE Amusements and
  Mahkota Langit Grid trashed unrezzed no longer persist.
- **"HQ or R&D only"** made `install_only_in` a list of `ServerKind`s (`Hq`
  and `RnD` join `Central` and `Remote`), asked through
  `CardDefinition::may_be_installed_in` at the five places that read it.
- **Deck.** Hostile Bid: two Flagship for two Wall of Static.
- **DSL ratio (`pool_status.py`): 21 of 75 `Effect` variants single-use,
  2 unused**, over 249 card files — no new `Effect`: two kinds, a scope, a
  target and an event.
- **Real play.** Ninety-six games of Hostile Bid against Pay As You Go
  (seed 2): with a random Corp, Flagship was installed 83 times and
  rezzed 41, and 56 runs were breached without being declared successful
  (169 against the heuristic Runner, which runs more). **The heuristic
  Corp installs Flagship and never rezzes it** (82 installs, 0 rezzes), so
  it never withholds a success: a gap in the Corp's rez choices for an
  upgrade with no printed payoff, owed to Phase 5.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has both random seatings **identical**. The
  heuristic seatings moved, and without the card file and the deck swap
  they are identical in all four shapes — a ref that keeps the persistent
  fix, so that fix moved no game in a full pass of the pool.

#### Stage 8 — Méliès U, a secret identity with three reverse sides (27 September 2026)

1 → 0. **Vantage Point is complete: 66 of 66**, and with it Startup's pool.

- **Which copy is in play is state, and secret.** CR 1.5.2b: the Corp
  brings every copy, "secretly sets" one as its identity when its discard
  phase ends, and the chosen copy always enters front side up. One card
  file holds the front and all three backs, as a flip identity holds both
  sides; `CorpState::identity_copy` says which back is the one printed on
  the card in play, and `EffectRequirement::IdentityCopy(n)` gates each
  back's text. The flip stays the one bit it was (CR 3.1.1a: only the
  faceup side is active). The copy is `None` in the Runner's view while
  the front is up and public once flipped (`PublicCorpState::
  identity_copy`) — the card turned over. It starts at 0, which no back
  side reads, rather than asking at setup: the Corp's first discard phase
  ends before the Runner's first turn, so a copy is always chosen before
  any run can flip it.
- **A secret number is a flag on the number, masked in both channels.**
  `Effect::ChooseNumber` gains `secret`, carried onto the parked decision
  and `GameEvent::NumberChosen`. The decision itself is public — the
  Runner sees the Corp is choosing, and from what range — but the answer's
  log entry is `ConcealedAction::ChoosingSecretly` for the other seat
  (read off the step's events, as `ChoosingPayment` is), and its
  `NumberChosen` is dropped for them. `Effect::SetIdentityCopy` writes
  the number; `validate` refuses it anywhere but inside a secret number's
  `then` on a Corp identity (`IdentityCopySetInTheOpen`).
- **"The Runner's" phase, on a Corp card.** "When the Runner's action
  phase ends, gain 1[credit]" and "when the Runner's discard phase ends,
  flip this identity" are the two triggers the pool prints about *your*
  phase, turned on the other player. `EventFilter::Whose(Side)` names
  whose moment a `Hears::OwnSide` trigger means, replacing its "your";
  `validate` allows it on no other trigger. **Found on the way:** the
  fire-time re-check (`listeners::when_admits`) never asked whose moment
  it was, so a card with two entries for one trigger — Méliès U's own
  discard phase and the Runner's — would have fired both at either. It
  now asks through the same function as the scan (`whose_admits`).
- **A flip is a moment.** `Trigger::OnIdentityFlipped` ("when you flip
  this identity to this side"), heard only by the side just turned up;
  `Effect::FlipIdentity` now dispatches `GameEvent::IdentityFlipped`
  (Dewi Subrotoputri and Nebula Talent Management flip through it too, and
  nothing of theirs hears it). "During a run on HQ" is
  `EffectRequirement::DuringRunOn(server)`, which counts the breach,
  unlike `DuringRun`. The back side composes: a look at the top of R&D,
  then a "may" whose yes trashes it and takes a card from Archives to HQ —
  a `PresentChoice` rather than a card selection, because a selection's
  `then` fires on a choice of none and the Archives card is "if you do".
- **Deck.** Honor Roll, a new Jinteki Sweep deck on Méliès U (Eternal and
  Casual), built from the Jinteki and neutral cards A Thousand Cuts leaves
  out, so Personal Evolution keeps its deck.
- **DSL ratio (`pool_status.py`): 21 of 76 `Effect` variants single-use,
  2 unused**, over 250 card files — `SetIdentityCopy` is new and
  single-use, and `LookAtTopOfDeck` gained its second card.
- **Real play.** Ninety-six games of Honor Roll against Pay As You Go
  (seed 2). Random seats: 769 secret choices, 345 flips on a successful
  central run, and the copy's side matched the server 128 times (37%, a
  third expected); the Corp took the "may" 67 times. Heuristic seats: 1,349
  choices, 573 flips, 198 matches — **but the heuristic Corp trashed the
  top of R&D only 12 times in 198**, and it picks its copy without reading
  where the Runner runs: both owed to Phase 5, like Flagship's rez.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has **all four shapes identical** — random and
  heuristic, by view and by index: the new deck is a Sweep deck, which
  `matchups()` never yields, and the engine changes (the whose re-check, a
  flip dispatched, a turn-log row for it) moved no game of the pool.

#### After Vantage Point — both clients show what it added (27 September 2026)

Each stage had put what a card needed into the view; nothing had made
the clients draw it. A read of every view field Stage 0 to Stage 8 added,
against what the terminal and the desktop draw, found six gaps, and one
from before (a flip identity's side, which the view had carried since Dewi
and Nebula with neither client showing it):

- **Which side of an identity is up** (`board::hud::IdentitySide`): a
  chip on the desktop's avatar disc ("Front", "Flipped", "Side 2"), a line
  on the identity's sheet, and a first line in each of the terminal's
  blocks — which named neither identity until now. Méliès U's copy is the
  Corp's to see once set and the Runner's once turned over.
- **Each scored card's worth now** (`CorpClientView::scored_worth`,
  `RunnerClientView::scored_worth`, beside `scored_agendas`): the score
  list read printed points, so a stolen Let Them Dream said 2 under a
  total that counted 1. The one core change; derived, never read back.
- **What left the game** (`hud::removed_from_game`): the Runner's pile,
  new with Vantage Point, and the Corp's, which no client had drawn — under
  the heap's and Archives' sheets on the desktop, on the identity line and
  in the card picker's zones in the terminal.
- **What is in effect for a while** (`hud::in_effect`): Aircheck's locked
  credit pool, Caveat Emptor's and Aggressive Trendsetting's next-turn
  clicks, a score lock, Tread Lightly's rez tax, Shred's hold — under the
  desktop's prompt, and under the terminal's servers. A strength is left
  out; its number is already on the card.
- **An agenda installed faceup says so** — "faceup" on its tile and in
  both clients' lists, where the terminal had called it "rezzed" and a
  Corp saw a faceup agenda the same as a facedown one.
- **A choice about a card just looked at shows that card**
  (`Prompt::card_after`): Méliès U's "You may trash that card" showed the
  identity asking, not the top of R&D, which only the looker's log names.
  The desktop's pop-up; the terminal already has the log line above it.

**Startup is whole.** Its three packs are built, so every Startup card and
identity is playable in both deck builders, which
`every_startup_card_is_playable` now holds: a card a re-sync adds to the
pool fails it until it is built. No published Startup list is shipped as a
sample deck yet — the stages were played through Sweep decks — so a person
reaches Vantage Point's cards by building a deck.

### 2. Rebellion Without Rehearsal — 65 cards (C 7 / V 43 / M 15)

#### Stage 1 — the first nine cards (27 September 2026)

`feat/rwr-stage-1-first-cards`: Friend of a Friend, Coalescence, Pressure
Spike, Valentina Ferreira Carvalho, Corporate Hospitality, Boto,
Capacitor, Seraph, The Powers That Be. **No new `Effect` or `Trigger`.**
Rebellion Without Rehearsal 9 of 65; `RWR_UNIMPLEMENTED` 65 → 56. Eye for
an Eye went to Stage 3 (above).

- **Two use words, each an `EffectRequirement`.** `OncePerRun`, Pressure
  Spike's "Use this ability only once per run": `OncePerTurn`'s use
  limit, keyed the same way (`OncePerTurnKey`), spent where it is used,
  and held by the run (`RunState::once_per_run_used`), so a new run is
  the reset and there is no clearing site to forget. `PublicRunState`
  carries it — a sample that did not would offer the +9 twice — masked
  as the Corp's once-per-turn set is. `DuringYourTurn`, Coalescence's
  "only during your turn" and Valentina's "when you install this resource
  during your turn": the asking side is the active player
  (`listeners::active_side`). `validate` holds `OncePerRun` to the two
  rules `OncePerTurn` has (never on a continuous effect; one per card).
- **A tag removal says who removed it.** `TagRemoved`, `TagsRemoved` and
  `TagsCleared` carry `by` (CR 1.14.3a: either player removes the
  Runner's tags to pay a cost): the text's controller, a cost's payer, the
  Runner's basic action. `OnTagRemoved`'s moment is the remover's, and
  `EventFilter::Whose` is now allowed on it (`Trigger::states_whose`, the
  triggers whose moment names a player), so Valentina's "whenever **you**
  remove 1 or more tags" is `when: Whose(Runner)` and does not hear
  Synapse Global's "[click], remove 1 tag" — which the log had called
  "Runner removed 1 tag(s)" and now calls the Corp's.
- **An operation resolving is not in its own Archives** (CR 8.6.7a). The
  engine files an operation into Archives as it is played, before its
  text resolves (Rules Conformance F6, recorded as changing nothing), and
  Corporate Hospitality's "Add 1 card from Archives to HQ" offered the
  Corporate Hospitality resolving it — a free replay each time. Its own
  copy is never eligible now (`pending_choice::resolving_operation_in`,
  asked by `eligible_positions`, which takes the choosing card); the
  filing itself stays early, because a resolution parked across actions
  has no marked end.
- **The rest composes.** Boto and Capacitor are `Strength` kinds with a
  `while` (threat 4; the Runner tagged); Boto's "you may trash 1 card
  from HQ to end the run" is the Corp's `OfferPaidChoice` over
  `Cost::Trash`, as Anoetic Void's is; Seraph's encounter is the
  Runner's `OfferPaidChoice` over `AnyOf` two costs; The Powers That Be
  is Ansel 1.0's install from HQ or Archives with `ignore_costs`.
- **The clients.** One view field, `PublicRunState::once_per_run_used`,
  is the engine's in `view_ledger` (a use limit the action list and
  `board::breaks` honour, as `once_per_turn_used` is). The log words a
  tag removal by its remover. The requirements reach the inspector's
  "Engine reads it as" through `prose`'s generic rendering ("once per
  run", "during your turn"). The paid choices and the install prompt are
  kinds both clients already draw. No ledger row opened.
- **Decks** — swaps into Eternal-only Sweep decks, so no Startup pin
  moves: two Friend of a Friend for two Sure Gamble and two Valentina
  added in Pay As You Go; two Coalescence and two Pressure Spike for three
  Telework Contract and a Creative Commission in Safety Net; two Corporate
  Hospitality for two Government Subsidy in Retirement Package; two
  Capacitor and two Seraph for three Ping and the Whitespace, two The
  Powers That Be for two Hedge Fund, in Paid Content; two Boto for two
  Enigma in Honor Roll. The set's identities (Sebastião, Nuvem SA,
  Thunderbolt Armaments) get Sweep decks of their own in their stages.
- **Real play**, 96 games a pairing (seed 2). Random seats use all nine:
  Friend of a Friend activated 25 times, Valentina's tag trigger 7 and
  its threat-3 install 14, Coalescence 251 counters spent, Corporate
  Hospitality played 95 times, The Powers That Be installing off 83
  scores in one heuristic pairing. **The heuristic Runner never installs
  Friend of a Friend, Valentina or Coalescence** — the Phase 5 debt of
  earlier stages (resources and economy programs it does not value).
- **Fidelity limits:** Pressure Spike's pumps last the encounter, as
  every breaker's does here. Corporate Hospitality excludes only the copy
  resolving, found as the last faceup copy of it in Archives.
- **DSL ratio (`pool_status.py`): 21 of 76 `Effect` variants single-use,
  2 unused**, over 259 card files — unchanged.
- **Measured.** Both sweeps at 256 seeds are green. Against
  `origin/main`, `coverage_identical.py` has the random seatings
  **identical**, by view and by index; the heuristic ones moved (Corp wins
  81 → 82 of 192, two deck-outs gone). Checked, not inferred: a ref with
  the engine changes and without the new cards, their tests and the deck
  swaps is **identical in all four shapes**, so the movement is
  `determinize` sampling five new Corp cards, and the tag, use-limit and
  resolving-operation changes move no game of the pool.

#### Stage 2a — tags and a cost on the Corp's basic trash action (27 September 2026)

`feat/rwr-stage-2a-tags-and-basic-trash-cost`: Sebastião Souza Pessoa:
Activist Organizer, Manuel Lattes de Moura, Privileged Access, Amanuensis,
Amelia Earhart, Juli Moreira Lee, Arruaceiras Crew. Rebellion Without
Rehearsal 16 of 65; `RWR_UNIMPLEMENTED` 56 → 49. Stage 2 was split when it
was taken: purge and bypass (Malandragem, Physarum Entangler, Heliamphora)
are Stage 2b.

- **An additional cost on the Corp's basic trash action**
  (`ContinuousKind::BasicTrashCost`, CR 5.2.6g, 1.16.10). Sebastião's "to
  trash a **connection** resource with the basic action, the Corp must
  trash 1 card from HQ" is about a resource being `Scope::Trashing`, and
  Manuel's "to trash **this** resource" is `Scope::This` under a threat 3
  `while`. `ScoreCost`'s shape: a `Cost`, asked of the install being
  trashed (`continuous::basic_trash_costs`) and paid with the action's
  [click] and 2[credit], all of it or none (1.16.10b), so a Corp with an
  empty HQ is not offered the trash — the Corp declines by not taking the
  action (1.16.10a). The card from HQ is asked by the payment's replay, one
  card at a time (`payment::could_ask` admits `TrashResource`). Both
  clients' explanation of the action now says a Runner card may add to its
  price; the question is the card-payment kind they already draw.
- **"If you had no tags" is on the event.** `TagsGiven` carries `had`, the
  tags before (both sites that give tags: the prevention door and
  `Cost::TakeTags`), read by `EffectRequirement::HadNoTags` the way
  `WasFirstAdvancementThisCard` reads `CardAdvanced`: by the time
  Sebastião's trigger resolves the state holds the tags just taken.
- **The use of an action is a moment** (`Trigger::OnActionTaken`, CR
  9.5.7b): `AbilityActivated` carries the copy used and whether the ability
  is an action (its cost begins with [click], 9.5.2a), because the listener
  scan reads no registry, and it is heard with the cost's events after the
  effect, the order every cost's events already had. Juli Moreira Lee's
  "the first time each turn you take an action on an installed resource"
  is that trigger with `when: InstalledCard(CardType(Resource))` and
  `first_each_turn`. "When your turn ends" (Amanuensis) is
  `OnDiscardPhaseEnd`, which the rules make the same step (CR 5.7.2d).
- **Two `Amount`s, one `Effect` fewer.** `CardsAccessedLastRun` is Amelia
  Earhart's "if you accessed 3 or more cards during that run" under
  `AmountAtLeast`, and took over `Effect::GainCreditsPerCardAccessedThisRun`
  — Zahya Sadeghi is `GainCreditsAmount(Runner, CardsAccessedLastRun)` now.
  `EncounteredIceStrength` (never below 0) is Arruaceiras Crew's "if its
  strength is 0 or less", `Not(AmountAtLeast(.., 1))`, with
  `CardTarget::EncounteredIce` to trash it by the run's handle. The
  heap install took a `Discount` (`InstallRunnerCardFromHeap(Discount)`,
  Scrounge and Magdalene rewritten to `Credits(0)`) for Privileged Access's
  "paying 2[credit] less", as `PlayOperation { from }` took Plutus's zone.
- **The rest composes.** Manuel's "whenever you breach HQ or R&D while you
  are tagged" is Docklands Pass's `OnSuccessfulRun` per server;
  Privileged Access is Account Siphon's replacement of the breach;
  Amanuensis and Amelia are `OfferPaidChoice`s over counter costs.
- **Deck** — **Grassroots** (`DeckCategory::Sweep`, Eternal and Casual):
  Sebastião on an Anarch rig of connections (Friend of a Friend, Manuel,
  Nurse Hạnh, Rent Rioters, Arruaceiras Crew, Hackerspace to host them),
  Privileged Access and Friend of a Friend to take the first tag, and
  Amanuensis; Juli Moreira Lee and Amelia Earhart on influence.
- **Real play**, 96 games of Fine Print against Grassroots (seed 2). Random
  seats: Sebastião's install trigger 119 times, Privileged Access played
  29, Manuel's extra access 25, Juli's click 6, Amanuensis's counter 49 and
  its draw 3, Arruaceiras Crew used 30 times; the Corp took the basic trash
  action 57 times and trashed from HQ 47 times, which only the two new
  costs do in these decks. **Amelia Earhart was installed 10 times and
  never counted a run** — three accesses on one run of HQ or R&D is rare
  for a random Runner — so her two triggers are reached by the per-card
  tests alone. The heuristic Runner installs none of the deck's
  resources (Amanuensis 23 times, Privileged Access played 6), and the
  heuristic Corp never trashes one: the Phase 5 debt again.
- **Fidelity limits:** Privileged Access's two installs are conditioned on
  the Runner being tagged after the tag, so a Decoy that prevents the
  event's tag on a Runner tagged earlier in the run still lets them
  happen; and the two "when you take a tag with this event" abilities
  resolve in printed order rather than the Runner's. Manuel's extra access
  is heard at the run's success, as every "when you breach" in the pool
  is, so a run Flagship keeps from being declared successful gets none.
- **DSL ratio (`pool_status.py`): 19 of 75 `Effect` variants single-use,
  2 unused**, over 266 card files (21 of 76 over 259 before).
- **Measured.** Both sweeps at 256 seeds are green. Against
  `origin/main`, `coverage_identical.py` has the random seatings playing
  the same games by view and by index: the one difference is the effect
  Zahya's gain is counted under (`GainCreditsAmount` 316 → 409,
  `GainCreditsPerCardAccessedThisRun` 93 → 0). The heuristic ones moved
  (Corp wins 67 → 76 of 192, one Runner deck-out). Checked, not inferred:
  a ref with the engine changes and without the seven cards and their deck
  is **the same games in all four shapes**, the rename apart, so the
  movement is `determinize` sampling seven new Runner cards, and the trash
  cost, `had`, the dispatched use of an ability and the heap discount move
  no game of the pool.

#### Stage 2b — purge and bypass (27 September 2026)

`feat/rwr-stage-2b-purge-and-bypass`: Malandragem, Physarum Entangler.
Rebellion Without Rehearsal 18 of 65; `RWR_UNIMPLEMENTED` 49 → 47.
Heliamphora went to Stage 3 (above).

- **A purge is a moment** (`Trigger::OnVirusCountersPurged`, CR 10.1.2):
  `VirusCountersPurged` was recorded and never dispatched; it is now, from
  the basic action and from Flyswatter's `Effect::PurgeVirusCounters`,
  heard by every active card whether or not it hosts counters. Physarum
  Entangler's "when the Corp purges virus counters, trash this program".
- **A credit cost reckoned as it is paid** (`Cost::CreditsAmount`, CR
  1.16.2b), over `Amount::EncounteredIceSubroutines`, printed and gained:
  Physarum's "pay 1[credit] for each subroutine it has", in an
  `OfferPaidChoice` gated on its host being encountered and not a barrier
  (`EncounteringHostIce`, `Not(Encountering(Barrier))`). `CreditsFrom`
  limits where the credits come from; this one takes them from anywhere.
- **Removal from the game out of the rig.** `Cost::RemoveSelfFromGame`
  was Spin Doctor's and read only Corp installs; it now removes a rig card
  too (what it hosted is trashed with it), which is Malandragem's threat 4
  "remove this program from the game to bypass it". Its "when it is empty,
  remove it from the game" is `Effect::RemoveFromGame(ThisCard)`, the same
  move made by the card's text, after the counter its bypass spends.
  Its strength-3 gate is `Not(AmountAtLeast(EncounteredIceStrength, 4))`.
- **A card's second trigger on one moment waits for the first** (found by
  Malandragem's tests). A card's `TriggeredEffect`s for one moment fire in
  one loop (`ability::fire_card_triggers`), and when the first parked a
  decision the second parked its own over it: at threat 4 the counter
  offer was never asked. The rest now wait on the queue behind the parked
  one, as the rest of a `Sequence` does, counted by
  `DeferredTrigger::fired`; a bypass by the first still stands the second
  down. No card before Malandragem had two that could both fire on one
  moment: Rotary's differ by server, Dewi Subrotoputri's by side and
  Méliès U's by which copy is up.
- **Deck** — Grassroots: Malandragem and two Physarum Entangler for the
  Cleaver, the Shred and a Fermenter (influence 15 of 15).
- **Real play**, 96 games of Fine Print against Grassroots (seed 2). Random
  seats: Malandragem's encounter triggers 15 times, Physarum's 12, and 12
  of the Corp's 88 purges trashed a Physarum; 7 pieces of ice bypassed.
  The heuristic Runner installs neither and the heuristic Corp never
  purges: the Phase 5 debt again.
- **Fidelity limits:** a declined Malandragem bypass has spent its "once
  per turn", which the rules say it has not (CR 9.3.6g) — the engine
  spends a trigger's use limit when it fires, as it does Zahya's; recorded
  in the conformance ledger's 9.3 row. Its two offers are asked in
  printed order, not the Runner's.
- **DSL ratio (`pool_status.py`): 19 of 76 `Effect` variants single-use,
  2 unused**, over 268 card files (19 of 75 over 266 before):
  `RemoveFromGame` is the one new variant, single-use.
- **Measured.** Both sweeps at 256 seeds are green. Against
  `origin/main`, `coverage_identical.py` has the random seatings
  **identical**, by view and by index; the heuristic ones moved (Corp
  agenda wins 76 → 67 of 192). Checked, not inferred: a ref with the
  engine changes — the purge dispatched, the new cost and effect, the
  queued second trigger — and without the two cards and the deck swap is
  **identical in all four shapes**, so the movement is `determinize`
  sampling two new Runner cards.

#### Stage 3a — the small words (27 September 2026)

`feat/rwr-stage-3a-small-words`: Boi-tatá, Meeting of Minds, “Pretty”
Mary da Silva, Ashen Epilogue. **No new `Effect`: one generalised in
place.** Rebellion Without Rehearsal 22 of 65; `RWR_UNIMPLEMENTED`
47 → 43. Stage 3 was split (above): the access abilities are 3b, the run
words and Muse 3c.

- **A trash says whether the card was installed** (`CardTrashed::
  installed`), and the turn log counts it: every `OnCardTrashed` moment
  was counted as a card out of a hand, so "installed" could not be asked.
  And a **Runner card's trash is counted by its type** — every Runner card
  trashed goes faceup to the heap, whoever trashed it and from wherever —
  while a Corp card's stays `Unseen` (`turn_log::seen_anyway`, beside
  `concealed`). Boi-tatá's "if you trashed any of your installed cards
  this turn" is then a turn-log count, `TimesThisTurnWhen { OnCardTrashed,
  InstalledCard(Program, Hardware, Resource) }` under each ability's
  `cost_discount_if`, as Marjanah's is: the moment is the trasher's, so a
  Corp trashing the Runner's resource is not counted, and a card out of
  the grip is not installed.
- **A shuffle names its zones** (`Effect::ShuffleIntoDeck(zones)`): Read-
  Write Share's `ShuffleHostedIntoDeck` took the zones when Ashen
  Epilogue's "shuffle your grip and heap into your stack" needed two more
  (`HostedOnSource`, `OwnGrip`, `OwnHeap`; the Runner's only, as no Corp
  card prints one). Its "remove the top 5 cards of your stack from the
  game" is five `RemoveFromGame(TopOfStack)`, which 2b's effect now
  resolves, and "remove this event" the existing `removed_after_play`.
- **Two amounts.** `CardsSelected`, Meeting of Minds's "gain 1[credit]
  for each card revealed this way", read in the reveal's `then` as
  `RemainingAfterSelection` is; its two halves are `PromptChooseCards` over
  the stack (shuffled, to the grip) and over the grip (revealed, kept),
  one pair per subtype behind a `PresentChoice`. `AccessLimit(server)`,
  Mary's "if you are allowed to access 2 or more cards in R&D during this
  breach": 1 plus the additional accesses granted so far (CR 7.3.5a–b),
  asked as her trigger resolves. Her "whenever you breach" is heard at the
  run's success, as every "when you breach" in the pool is; a same-moment
  bonus counts if the Runner orders it first.
- **Decks** — Grassroots: two Boi-tatá for the Hantu (every self-trash in
  the deck makes it cheaper) and an Ashen Epilogue for a Wildcat Strike.
  Spare Parts: two Mary, which The Maker's Eye and Jailbreak give a second
  R&D access to add to, and two Meeting of Minds for the connections, on
  six influence — for a Vrcation, a Telework Contract, a Diesel and an
  Overclock.
- **Real play**, 96 games of Fine Print against each (seed 2). Random
  seats: Boi-tatá installed 15 times and used 4, Ashen Epilogue played 8,
  Mary installed 29 and triggered 5, Meeting of Minds played 6. The
  heuristic Runner uses Boi-tatá (282 uses in 49 installs) and plays Ashen
  Epilogue 4 times, and never installs Mary or plays Meeting of Minds.
- **Fidelity limits:** Meeting of Minds offers every card of the subtype
  in the grip and lets the Runner pick any number, a reveal of none
  included, and the stack's search is by subtype over resources only, as
  printed.
- **DSL ratio (`pool_status.py`): 17 of 76 `Effect` variants single-use,
  2 unused**, over 272 card files (19 of 76 over 268 before): the shuffle
  and `RemoveFromGame` each have a second card.
- **Measured.** Both sweeps at 256 seeds are green. Against
  `origin/main`, `coverage_identical.py` has the random seatings
  **identical**, by view and by index; the heuristic ones moved (Corp
  agenda wins 67 → 71 of 192, flatlines 15 → 20). Checked, not inferred:
  a ref with the engine changes — the trash's `installed`, a Runner card's
  trash counted by type, the shuffle's zones, the two amounts — and
  without the four cards and the deck swaps is **identical in all four
  shapes**, so the movement is `determinize` sampling four new Runner
  cards.

#### Stage 3b — the run's event, and a card hosted as it is accessed (27 September 2026)

`feat/rwr-stage-3b-access`: Eye for an Eye, Cupellation, Heliamphora.
**No new `Effect`, `Trigger` or `PlayerAction`.** Rebellion Without
Rehearsal 25 of 65; `RWR_UNIMPLEMENTED` 43 → 40.

- **The event that started the run is active until it ends** (CR 8.6.5):
  `rules::active` has a fourth place, `Place::PlayArea`, read off
  `RunState::initiated_by` when it names an event (`run::run_event`). The
  engine files a played event in the heap as it resolves, so the play area
  is read from the run, and the event is inactive once its run is over.
  Its triggers hear the run — Eye for an Eye's "If successful, take 1 tag
  and access 1 additional card" is an `OnSuccessfulRun` on the event
  itself, `requirement: ThisCardStartedTheRun` — and its paid abilities
  are used through `InstallId::RUN_EVENT`, a third id beside the two
  identities': its "Access → Trash 1 card from your grip: Trash the card
  you are accessing" is an ordinary `ActivateAbility`, in the Runner's
  second-to-last `ActionSpace` slot, so the space's size is unchanged
  (a rig card loses its 31st slot, which no board reaches). Before this,
  no event text could outlive its play except as a rider on the run.
- **A card hosted as it is accessed** (`HostedCardOrigin::AccessedCard`,
  `run::host_currently_accessed_card`): taken from wherever it is — HQ,
  R&D, Archives, or a root through `uninstall::corp_install` — and hosted
  faceup, not installed (CR 1.13.2a); the access ends with the move (CR
  7.1.7), as a trash's does. Cupellation's "Access → 1[credit]" is an
  access ability on the program, "limit 1 hosted card" its requirement
  (`Not(AmountAtLeast(HostedCards, 1))`: no rule says more), and its
  "whenever you breach HQ … pay 1[credit] and trash this program to access
  2 additional cards" an `OfferPaidChoice` at the run's success.
- **Heliamphora hears the access** (`OnAccessed`, when it is in Archives'
  pile, not its root), and its "host it … instead" is a `PresentChoice`
  resolved before the Runner's decision about the card. Its purge half is
  2b's trigger with a new target, `CardTarget::RandomFromHq`: "they trash
  2 cards from HQ at random", facedown, twice, then itself.
- **The view carries the run's event** (`PublicRunState::initiated_by`,
  public: only a Runner card starts a run). Both clients name the event on
  its ability's button, a decision under the prompt since the event is not
  on the board (`board::action_map`, `actions::install_label`), and
  `determinize` copies it — so a sample now sees Sang Kancil's "if a run
  event is active", which it read false in every sample before.
- **Decks** — Grassroots: two Eye for an Eye for the Take a Dive and a
  Heliamphora for a Charm Offensive. Spare Parts: two Cupellation, on four
  more influence (10 of 15), for a Beta Build and a Sure Gamble.
  Cupellation was meant for Borrowed Time, the Criminal Sweep deck, and
  would have cost it Startup.
- **Real play**, 96 games of Fine Print against each (seed 2). Random
  seats: Eye for an Eye played 18 times, successful 5, its ability used
  3; Heliamphora installed 13, heard 10 Archives accesses, trashed by 4
  purges; Cupellation installed 36, hosted 22 cards, its HQ offer heard
  17 times. The heuristic Runner plays and installs none of the three.
- **Fidelity limits:** Heliamphora's "instead" is an access, then a
  host: the card is accessed first, so a card's own "when accessed" in
  Archives still resolves and it counts as accessed. A true interrupt
  would park the access before it began, an access phase and a pair of
  actions of its own. A declined host has spent its "once each time you
  breach Archives" (the 9.3.6g deviation recorded in 2b). "The Corp
  trashes" is the Runner's trash, as for Noise (conformance, 1.14).
- **DSL ratio (`pool_status.py`): 17 of 76 `Effect` variants single-use,
  2 unused**, over 275 card files (17 of 76 over 272 before).
- **Measured.** Both sweeps at 256 seeds are green, the fog gate
  included. Against `origin/main`, `coverage_identical.py` has the random
  seatings **identical**, by view and by index; the heuristic ones moved
  (Corp agenda wins 71 → 71 of 192, flatlines 20 → 9). Checked, not
  inferred: a ref with the engine changes — the run's event active and
  in the view, `RUN_EVENT`, the accessed card hosted, the random HQ
  trash — and without the three cards and the deck swaps is **identical
  in all four shapes**, so the movement is `determinize` sampling three
  new Runner cards; the samples' Sang Kancil moved no game of the pool.

#### Stage 3c — run words (27 September 2026)

`feat/rwr-stage-3c-run-words`: Trick Shot, Window of Opportunity, Alarm
Clock. **One new `Effect`.** Rebellion Without Rehearsal 28 of 65;
`RWR_UNIMPLEMENTED` 40 → 37. Muse went to 3d (above): its install onto a
host out of three zones is a stage's work of its own.

- **A run begun as the Runner's turn begins** (CR 5.7.1d): Alarm Clock's
  "When your turn begins, you may run HQ" starts a run in the
  `StartOfTurn` phase, which the run's windows and steps do not know.
  `run::start_run` runs it in the action phase's shape — the turn's start
  window taken down, the phase `Action(Runner)` — and `run::end_run`, the
  one door every run leaves by, puts the turn back when it ends: the
  phase, and the window of 5.7.1e whose closing begins the action phase.
  `RunState::begun_as_the_turn_began` says so, carried in the view
  (`PublicRunState`) so a sample ends the run where the game does. The
  basic action is still refused outside the action phase
  (`engine::initiate_run`). "The first time you encounter a piece of ice
  during that run, you may spend [click][click] to bypass it" is an
  `OnEncounter` on the hardware, `ThisCardStartedTheRun` and `OncePerRun`
  — spent at the first encounter whatever the answer, which is what "the
  first time" means.
- **Credits on the run's event** (`Effect::PlaceRunCredits`, new): the
  run's pool (`RunState::bonus_run_credits`, Overclock's), placed by a
  card's text rather than only as a run begins. Trick Shot places 4 as it
  runs R&D and 2 more on success (an `OnSuccessfulRun` on the event, 3b's
  play area), and its "When that run ends, you may run a remote server"
  carries what is left into that run: `CompletedRun::run_credits_left`,
  read as `Amount::RunCreditsLeftLastRun` by the second run's `on_start`.
  The event is still in play (CR 8.6.5), so its credits are.
- **Window of Opportunity composes.** The install is a `PresentChoice`
  over a grip selection of programs and hardware; "When that run begins,
  derez 1 piece of ice protecting that server" is the run's `on_start`, a
  selection of rezzed ice in the attacked server whose `then` derezzes it
  and sets the run's end rider as that ice: the Corp's "may rez the ice
  derezzed this way, ignoring all costs" is a `PresentChoice` resolving as
  the ice, so `RezInstalled`'s placeholder is the one derezzed.
- **Deck** — Spare Parts: two Trick Shot, an Alarm Clock and a Window of
  Opportunity, on thirteen influence of fifteen, for a Meeting of Minds, a
  Touchstone, a T400 Memory Diamond and a Diesel. None fits Borrowed Time,
  which would lose Startup.
- **Real play**, 96 games of Fine Print against Spare Parts (seed 2).
  Random seats: Alarm Clock installed 10 times, its turn-start offer heard
  43 times and its bypass offer 4; Trick Shot played 21 times, successful
  12; Window of Opportunity played 11, its derez asked 3 times. The
  heuristic Runner uses none of the three.
- **Fidelity limits:** Alarm Clock's run is asked for among the Runner's
  other "when your turn begins" abilities and resolved when it is chosen,
  so an ability ordered after it resolves after the run, as the rules have
  it; the Corp's rez window of 5.7.1e comes once, after the run. The
  heuristic's samples of a turn-start run see the run as the action
  phase's.
- **DSL ratio (`pool_status.py`): 18 of 77 `Effect` variants single-use,
  2 unused**, over 278 card files (17 of 76 over 275 before):
  `PlaceRunCredits` is the new one, Trick Shot's alone, its reason on the
  variant.
- **Measured.** Both sweeps at 256 seeds are green, the fog gate
  included. Against `origin/main`, `coverage_identical.py` has the random
  seatings **identical**, by view and by index; the heuristic ones moved
  (Corp agenda wins 71 → 67 of 192, flatlines 9 → 13). Checked, not
  inferred: a ref with the engine changes — the run begun at the turn's
  start and its return, the placed and carried run credits — and without
  the three cards and the deck swap is **identical in all four shapes**,
  so the movement is `determinize` sampling three new Runner cards.

#### Stage 3d — a search that installs onto a host (27 September 2026)

`feat/rwr-stage-3d-muse`: Muse. **One new `Effect`.** Rebellion Without
Rehearsal 29 of 65; `RWR_UNIMPLEMENTED` 37 → 36. **Stage 3 is complete.**

- **A text install onto a host** (`Effect::InstallProgramOnHost`, new):
  Muse's "search your stack, heap, or grip for 1 non-daemon program … If
  that program is a trojan, install it on a piece of ice. Otherwise,
  install it on this program." The search is a `PresentChoice` of the
  three zones, each a `PromptChooseCards` (the stack's shuffled after),
  whose `then` is the install: it resolves as the program found, with
  Muse the acting install. A program goes onto Muse
  (`InstalledRunnerCard::hosted_on_rig_card`, as Hackerspace hosts a
  resource, and trashed with it); a Trojan's ice is the Runner's to
  choose, so the effect parks a selection of ice whose `then` is itself
  with the program written in — the State Hygiene Rule's third case — and
  installs on the ice it resolves as. `engine::install_program_onto` is
  the text install's steps with a host (the memory limit's trash, the
  price, `ProgramInstalled`); `RunnerCardSource` gained the stack.
  Nothing happens when the program cannot be installed that way, as for
  every text install.
- **Deck** — Spare Parts: two Muse and a Stowaway for it to find and put
  on ice, in faction, for a DZMZ Optimizer, a Principia and a Mayfly.
- **Real play**, 96 games of Fine Print against Spare Parts (seed 2).
  Random seats install Muse 24 times, its search asked 19; the heuristic
  Runner installs none.
- **Fidelity limits:** the search offers the programs of one zone at a
  time, the zone chosen first, where the rules have the Runner search
  all three. A program hosted on Muse takes memory as any program does —
  Muse prints nothing otherwise.
- **DSL ratio (`pool_status.py`): 19 of 78 `Effect` variants single-use,
  2 unused**, over 279 card files (18 of 77 over 278 before):
  `InstallProgramOnHost` is the new one, its reason on the variant.
- **Measured.** Both sweeps at 256 seeds are green, the fog gate
  included. Against 3c, `coverage_identical.py` has the random seatings
  **identical**, by view and by index; the heuristic ones moved (Corp
  agenda wins 67 → 64 of 192, flatlines 13 → 17). Checked, not inferred:
  a ref with the host install and the stack source and without Muse and
  the deck swap is **identical in all four shapes**, so the movement is
  `determinize` sampling Muse.

#### Stage 4a — advancement counters placed and removed (27 September 2026)

`feat/rwr-stage-4a-advancement-words`: Charlotte Caçador, Cohort Guidance
Program, Logjam, Business As Usual, Kingmaking. **One new `Effect`.**
Rebellion Without Rehearsal 34 of 65; `RWR_UNIMPLEMENTED` 36 → 31. Stage 4
was split: Isaac Liberdade and Hearts and Minds are 4b, and The Holo Man,
Stoke the Embers and Janaína “JK” Dumont Kindelán are 4c, all three needing
where an install came from ("installed any cards from HQ this turn",
"install this agenda from anywhere except HQ").

- **A card in Archives turned faceup by a card's text**
  (`Effect::TurnFaceupInArchives`, new): Cohort Guidance Program's "Turn 1
  facedown card in Archives faceup", the `then` of a selection of
  `CardFilter::Facedown` cards there, heard as the breach's turning over is
  (`ArchivesTurnedFaceup`, one card). "Trash 1 card from HQ. If you do" is
  a selection whose `destination` is Archives — facedown, as a card out of
  HQ lands — and whose `then` is the gain and the draw, so it resolves only
  when a card went; "place 1 advancement counter on an installed card" is
  any installed card, advanceable or not (CR 1.18.2).
- **An agenda added to the score area** (CR 1.17.3e): Kingmaking's "You may
  add 1 agenda worth 1 or less agenda points from HQ to your score area" is
  a selection whose `destination` is the Corp's score area, which no
  selection could name before (`ability::add_agenda_to_score_area`). Worth
  what it prints, never scored (`ScoredAgenda::scored_on_turn` 0), and
  announced as `GameEvent::AgendaAddedToScoreArea`, an occurrence of
  nothing; the log and the end-of-game table count it. `CardFilter::
  AgendaPointsAtMost` is the "worth 1 or less"; "draw up to 3" is a
  `ChooseNumber`.
- **"Remove all" is an amount:** `Effect::RemoveCounters` takes an `Amount`
  (ten card files rewritten to `Fixed`), so Business As Usual's "Remove all
  virus counters from 1 installed card" is `RemoveCounters(HostedCounters)`
  on a selection of `CardFilter::HostsCounters(Virus)` — a card with none
  is never offered.
- **Business As Usual composes.** "Place 1 advancement counter on each of up
  to 2 installed cards you can advance" is a selection of one whose `then`
  places a counter and selects a second from the rest (`NotSourceCard`,
  the first being the install the second prompt acts as); each placement
  waits on a card having been chosen (`AmountAtLeast(CardsSelected, 1)`),
  because a `then` resolves with nothing chosen too, as the parking card.
  "Threat 3 → You may also resolve the other mode" is a `ResolveSomeOf` of
  two over the two modes and a decline.
- **Logjam** is Ice Wall's words and a new amount, `CardTypesAmongFaceupIn
  Archives` (types, so ice is one type whatever its subtype, CR 2.15.2):
  "place 1 advancement counter on it plus 1 … for each card type" is two
  placements. **Charlotte Caçador** composes: "you may remove 1 hosted
  advancement counter to gain 4" is a nested cost (CR 1.16.11a), an
  `OfferPaidChoice` of `Cost::RemoveAdvancementCounters`, and "[trash],
  hosted advancement counter:" is the two costs, the counter first.
- **Client:** nothing new reaches the view, so `view_ledger` is unmoved;
  both clients' logs say "added Superconducting Hub to the Corp's score
  area for 1 point(s)" (`actions`), the end-of-game table counts it
  (`tally`), and `prose` has words for the new effect, amount and filters.
- **Decks** — Honor Roll: two Cohort Guidance Program and a Charlotte
  Caçador for two Regolith Mining License and an Unleash (two PAD Campaign
  out would have made it Standard-legal, which its pin refuses). Hostile
  Bid: two Logjam for two Palisade, two Business As Usual for two Petty
  Cash. Paid Content: two Kingmaking for two Orbital Superiority, with the
  deck's Superconducting Hub the agenda it adds.
- **Real play**, 96 games a pairing (seed 2), each deck against a Runner
  Sweep deck. Honor Roll against Grassroots: random seats install Charlotte
  Caçador 19 times and trash it for credits 10, and Cohort Guidance
  Program's turn start is heard 76 times; the heuristic Corp installs both
  (30 and 66) and hears them 271 and 563 times, and never trashes Charlotte
  for its 3. Hostile Bid against Pay as You Go: random seats play Business
  As Usual 87 times; the heuristic Corp plays none, and rezzes Logjam 43
  times, whose subroutines fire 192. Paid Content against Safety Net:
  Kingmaking scored 46 times by the heuristic Corp and heard 34 — the other
  twelve were the game's last points, which end it before the trigger
  resolves.
- **Fidelity limits:** Logjam's counters are placed in two steps, heard as
  two placements by a card that listens (none does). Business As Usual's
  virus counters are the Runner's cards': no Corp card in the pool hosts
  one.
- **DSL ratio (`pool_status.py`): 18 of 79 `Effect` variants single-use,
  2 unused**, over 284 card files (19 of 78 over 279 before):
  `TurnFaceupInArchives` is the new one, its reason on the variant, and
  `DrawCardsAmount` (Kingmaking) and `ResolveSomeOf` (Business As Usual)
  each found a second card.
- **Measured.** Both sweeps at 256 seeds are green, the fog gate
  included. Against `origin/main`, `coverage_identical.py` has the random
  seatings **identical**, by view and by index; the heuristic ones moved
  (Corp agenda wins 64 → 63 of 192, flatlines 17 → 16, a Corp deck-out
  0 → 1). Checked, not inferred: a ref with the engine changes — the
  faceup turn, the score-area destination, `RemoveCounters` by amount and
  its ten card files, the three filters and the amount — and without the
  five cards and the deck swaps is **identical in all four shapes**, so the
  movement is `determinize` sampling five new Corp cards.

#### Stage 4b — an upgrade that moves, and counters that move (27 September 2026)

`feat/rwr-stage-4b-moving-upgrades`: Isaac Liberdade, Hearts and Minds.
**One new `Effect` and one new `Trigger`.** Rebellion Without Rehearsal 36
of 65; `RWR_UNIMPLEMENTED` 31 → 29.

- **A move is heard** (`Trigger::OnCardMoved`, new): Isaac Liberdade's
  "Whenever this upgrade moves to the root of a server" is
  `GameEvent::CardMoved`, which `Effect::MoveThisCardToRoot` now
  dispatches (it was recorded and heard by nothing). About the card that
  moved, printed only about the card itself, and counted by the turn log
  as unseen, since a root card moves with its rez state. "When your turn
  ends, you may move this upgrade to the root of another server" is an
  `OnDiscardPhaseEnd` offer of Lotus Haze's `PromptMoveThisCardToAnotherRoot`.
- **A standing effect on the ice of one server**
  (`Scope::IceProtectingThisServer`, new): "Each advanced piece of ice
  protecting this server gets +2 strength". Its filter is read off the
  definition and off the copy (`pending_choice::copy_matches`), so
  `CardFilter::Advanced` is asked of the ice whose strength is read.
  `Advanced` and `Unadvanced` are words of their own because `Not` is
  decided off the definition alone; `InThisServer` (the root and the ice)
  is written over as `InServer` where the acting card is, as
  `InRootOfThisServer` is — "a piece of ice protecting that server that
  has no advancement counters" is `All([Ice, InThisServer, Unadvanced])`.
- **A counter moved** (`Effect::RemoveAdvancementCounters`, new): Hearts and
  Minds' "move 1 advancement counter from an installed card to an
  installed card you can advance" is a selection of an `Advanced` card
  whose `then` removes one and selects the card it goes to — moving is not
  advancing (CR 1.18.2). The removal was only ever a cost, paid by the
  card that prints it. The offer is made only while a card you can advance
  is installed (`ZoneHasAtLeast` with a filter), so the counter always has
  somewhere to go; "If this server is not protected by ice, you may also
  place 1" reads `IceProtectingThisServer`.
- **Client:** nothing new reaches the view; `prose` has words for the
  scope, the effect and the filters.
- **Deck** — Hostile Bid: two Hearts and Minds for a PAD Campaign and a
  Government Subsidy, and an Isaac Liberdade for a Whitespace.
- **Real play**, 96 games of Hostile Bid against Pay as You Go (seed 2).
  Random seats: Isaac Liberdade installed 47 times and rezzed 20, its
  turn-end move offered 99 times and taken 40, each move heard; Hearts and
  Minds' turn start heard 176 times. The heuristic Corp installs Isaac 44
  times and never rezzes it; Hearts and Minds it rezzes 86 times and hears
  924.
- **Fidelity limits:** Hearts and Minds' source is chosen before its
  destination, so a card you can advance may be picked as both — a move
  onto itself, which changes nothing.
- **DSL ratio (`pool_status.py`): 17 of 80 `Effect` variants single-use,
  2 unused**, over 286 card files (18 of 79 over 284 before):
  `PromptMoveThisCardToAnotherRoot` found its second card (Isaac
  Liberdade). `RemoveAdvancementCounters` is Hearts and Minds' alone as an
  effect; the script reads variant names off card-file keys and counts the
  cost of the same name (Charlotte Caçador, Sacrifice Zone Expansion).
- **Measured.** Both sweeps at 256 seeds are green, the fog gate
  included. Against `origin/main` (4a), `coverage_identical.py` has the
  random seatings **identical**, by view and by index; the heuristic ones
  moved (Corp agenda wins 63 → 64 of 192, flatlines 16 → 17). Checked, not
  inferred: a ref with the engine changes — the move dispatched and heard,
  the scope, the effect, the four filters — and without the two cards and
  the deck swap is **identical in all four shapes**, so the movement is
  `determinize` sampling two new Corp cards.

#### Stage 4c — where an install came from, and a card back to HQ (27 September 2026)

`feat/rwr-stage-4c-install-origin`: The Holo Man, Stoke the Embers,
Janaína “JK” Dumont Kindelán. **No new `Effect` or `Trigger`.** Rebellion
Without Rehearsal 39 of 65; `RWR_UNIMPLEMENTED` 29 → 26. **Stage 4 is
complete.**

- **An install says where the card came from**
  (`GameEvent::CardInstalled::from_hq`): set by `engine::place_corp_card`'s
  callers — the basic action, a text install out of HQ, Archives or R&D,
  Word on the Street's agenda out of the Runner's score area — and public,
  as a card leaving HQ is. Two words read it. Stoke the Embers' "When you
  install this agenda from anywhere except HQ" is a `when`,
  `EventFilter::InstalledFromHq(false)`, read off the moment
  (`listeners::Moment::from_hq`, as `ice` is) and refused by `validate` on
  anything but a Corp card's `OnInstall`. The Holo Man's "If you have not
  installed any cards from HQ this turn" is `Amount::
  CardsInstalledFromHqThisTurn`, a sum the turn log keeps beside
  `agenda_points_scored` (the Turn History Rule: where a card came from is
  no `Class`, and every Corp install from HQ is facedown, so its cells are
  unseen anyway). The view carries it with the log.
- **Two costs** (`Cost::RevealSelf`, `Cost::AddSelfToHq`): Stoke the Embers'
  "you may reveal it. If you do, …" is a nested cost (CR 1.16.11a), an
  `OfferPaidChoice` of revealing the facedown agenda — which stays facedown
  (CR 1.21.3a) and is remembered by the Runner as an accessed card is
  (`seen_by_runner`). JK's "[click], add this asset to HQ:" goes through
  `rules::uninstall`, announced as `GameEvent::CardAddedToHand` (the card
  named only if it was faceup), and her "Take all credits from this asset"
  is `GainCreditsAmount(HostedCounters)`, read off the counters she had as
  she left (`ResolutionContext::last_known`). "You may install 1 card from
  HQ" waits on a card having been chosen, or the `then` would install JK
  herself, now in HQ, as the parking card.
- **The Holo Man** composes otherwise: "move this upgrade to the root of
  another server" is 4b's offer as the turn begins, and "1 card in the
  root of or protecting this server" is `CardFilter::InThisServer`.
- **Client:** nothing new reaches the view; both clients' logs say which
  card went back to HQ (`actions`), and `prose` has words for the costs,
  the amount and the filter.
- **Deck** — Paid Content: two Stoke the Embers for the last Orbital
  Superiority and the Offworld Office, for The Powers That Be to install
  out of Archives; The Holo Man for the Hansei Review and JK for the Hedge
  Fund.
- **Real play**, 96 games of Paid Content against Safety Net (seed 2).
  Random seats: The Holo Man installed 50 times, its turn-start move heard
  129 and its ability used 4; JK installed 58 times and returned to HQ 19;
  Stoke the Embers installed 90 times, every one from HQ. The heuristic
  Corp scores Stoke 43 times and returns JK 63, and never uses The Holo
  Man's ability. **Stoke's "from anywhere except HQ" was not reached in
  either seating**: The Powers That Be must find it in Archives as an
  agenda is scored, and the card test is its reach so far.
- **Fidelity limits:** none found beyond the pool's: no card but JK returns
  an install to HQ, and no card but Stoke the Embers is revealed where it
  lies.
- **DSL ratio (`pool_status.py`): 17 of 80 `Effect` variants single-use,
  2 unused**, over 289 card files (17 of 80 over 286 before).
- **Measured.** Both sweeps at 256 seeds are green, the fog gate
  included — after the gate learned two things the log may name to a
  seat: a faceup card going back to HQ (`CardAddedToHand { faceup: true }`,
  seed 60, JK), and a card the Runner accessed, once a view taken after
  the game ended mid access no longer holds the access (seed 176, Esca's
  net damage flatlining the Runner in HQ — masking shows every access to
  the Runner by design; the gate had no rule for the end of a game there,
  and a new card's sampling reached it). Against `origin/main` (4b),
  `coverage_identical.py` has the random seatings **identical**, by view
  and by index; the heuristic ones moved (Corp agenda wins 64 → 66 of
  192, flatlines 17 → 16). Checked, not inferred: a ref with the engine
  changes — the install's origin and its sum, the filter, the two costs
  and the event — and without the three cards and the deck swap is
  **identical in all four shapes**, so the movement is `determinize`
  sampling three new Corp cards.

#### Stage 5a — a rez's price, counters on a rez, a mandate (28 September 2026)

`feat/rwr-stage-5a-piranhas-working-prototype-sudden-commandment`:
Piranhas, Working Prototype, Sudden Commandment. **No new `Effect` or
`Trigger`.** Rebellion Without Rehearsal 42 of 65; `RWR_UNIMPLEMENTED`
26 → 23. Stage 5 was split in four when it was taken (the plan, above).

- **Piranhas's additional rez cost** is two `rez_alternatives`, as
  Plutus's is: "take 1 bad publicity" (`Cost::TakeBadPublicity`, the Corp
  twin of `TakeTags`, never prevented and heard as bad publicity taken
  through `dispatch_cost_events`) or "remove 1 tag" (`Cost::RemoveTags`).
  With one payable it is taken unasked; with both the Corp is asked
  (`payment::Ask::Alternative`), and paid with the rez (CR 1.16.10b). "End
  the run if there are more cards in HQ than in the grip" compares two
  numbers the state decides (`EffectRequirement::MoreThan` over
  `Amount::CardsInHand`), where `AmountAtLeast` compares one with a number
  the file writes.
- **Working Prototype** composes: "whenever you rez a card (including this
  asset)" is `OnRez` about `Any` — the subject of a rez hears it, so its own
  counts — and "add 1 installed resource to the top of the stack" is a
  selection out of the rig onto the Runner's deck, which lands on top.
- **Sudden Commandment's "the first mandate you played this turn"** is a
  turn-log column (`turn_log::Kind::MandateOperation`), as Synchrocyclotron's
  double operation was, read by `TimesThisTurnWhen`. It is judged as the
  operation begins to resolve and the threat level as the clause is
  reached (CR 9.3.6f, after the operation it plays): the card is two
  branches, not-first ahead of first, so a second copy played out of HQ
  neither takes the first's click nor gives one of its own. "Play 1
  non-terminal operation from HQ" waits on a card having been chosen.
- **Client:** nothing new reaches the view; `prose` has words for the cost
  and the amount.
- **Decks** — Paid Content: two Piranhas for a Funhouse and a Paywall,
  rezzed off the deck's tags, and two Sudden Commandment for two Predictive
  Planogram. Retirement Package: two Working Prototype for the NICO
  Campaign and the Government Subsidy.
- **Real play**, 96 games against Safety Net (seed 2). Paid Content,
  random seats: Piranhas installed 118 times, rezzed 14, 93 subroutines
  fired; Sudden Commandment played 64 times. Heuristic seats: Piranhas
  rezzed 24 times and 180 subroutines fired; **the heuristic Corp never
  plays Sudden Commandment**. Retirement Package: Working Prototype rezzed
  94 times with random seats and heard 300 rezzes; its abilities were used
  104 times, and 290 by the heuristic Corp.
- **Fidelity limits:** none found beyond the pool's: Sudden Commandment is
  the catalog's only mandate, and no card names a terminal operation's
  end of the action phase yet (Stage 6), so "non-terminal" only narrows
  the choice.
- **DSL ratio (`pool_status.py`): 17 of 80 `Effect` variants single-use,
  2 unused**, over 292 card files (17 of 80 over 289 before).
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (4c), `coverage_identical.py` has the random seatings **identical**, by
  view and by index; the heuristic ones moved (Corp agenda wins 66 of 192
  unchanged, flatlines 16 → 15, Runner agenda wins 109 → 111, deck-outs
  1 → 0). Checked, not inferred: a ref with the engine changes — the
  cost, the requirement and its amount, the mandate column — and without
  the three cards and the deck swaps is **identical in all four shapes**,
  so the movement is `determinize` sampling three new Corp cards.

#### Stage 5b — ice that limits its encounter (28 September 2026)

`feat/rwr-stage-5b-hammer-sorocaban-blade-cloud-eater`: Hammer, Sorocaban
Blade, Cloud Eater. **No new `Effect` or `Trigger`.** Rebellion Without
Rehearsal 45 of 65; `RWR_UNIMPLEMENTED` 23 → 20.

- **What an encounter allows is said by the ice, of itself**, as two
  standing kinds about `This` (the Continuous Effect Rule, and `validate`
  refuses either off ice). Hammer's "the Runner cannot break more than 1
  of its printed subroutines except using killers" is
  `ContinuousKind::BreakLimit { at_most, except_using }`: both break
  effects, and the bioroid click break with it, break no more printed
  subroutines than the limit leaves (`continuous::breaks_left`), a gained
  subroutine is never limited (CR 9.8.3a), and an excepted breaker's
  breaks are not counted. Sorocaban Blade's "you cannot trash more than 1
  installed Runner card with this ice during each encounter" is
  `ContinuousKind::TrashLimit`: once spent, a selection the ice's text
  makes to trash an installed Runner card offers nothing
  (`continuous::may_trash_with`). A trash is counted where it is carried
  out — the selection, or the prevention window's end — so one the Runner
  prevented is not one. Afshar and Akhet print the break limit with no
  exception and a `while`.
- **The encounter's counts are on the run** (`RunState::this_encounter`,
  an `EncounterTally`), cleared where `fully_broken` is — as an encounter
  begins and by `enter_movement` — and carried in the view, so a bot's
  sample is held to the limits the game is.
- **Cloud Eater's "if it was rezzed this turn"** is a count on the copy:
  `turn_log::CopyTurn` counts rezzing beside advancing, read by
  `Amount::TimesThisTurnOnThisCopy(OnRez)`. Its encounter-end ability is
  Seraph's shape, a nested cost the Runner pays or declines (CR 1.16.11b).
- **Client:** the view carries the tally, which the action list and
  `board::breaks` already honour (`view_ledger`: engine's); `prose` has
  words for the two kinds and the amount.
- **Decks** — Hostile Bid: two Hammer for two Kessleroid. Retirement
  Package: two Sorocaban Blade for the Palisade and the Pharos. A Thousand
  Cuts: two Cloud Eater for the Tithe and the Ballista.
- **Real play**, 96 games against Safety Net (seed 2). Hammer: rezzed 7
  times with random seats and 44 with heuristic ones, 230 subroutines
  fired and 34 broken. Sorocaban Blade: rezzed 49 and 52 times, 337 and
  189 subroutines fired. Cloud Eater costs 10 and **random seats never
  rezzed it**; heuristic seats rezzed it 8 times, and its encounter-end
  ability fired 7.
- **Fidelity limits:** a bot's sample still does not carry a copy's
  counts for the turn (`determinize` leaves `CopyTurn` empty, as it did
  for advancing), so a sample of a Cloud Eater rezzed this turn does not
  see its encounter-end ability coming.
- **DSL ratio (`pool_status.py`): 17 of 80 `Effect` variants single-use,
  2 unused**, over 295 card files (17 of 80 over 292 before).
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (5a), `coverage_identical.py` has the random seatings **identical**, by
  view and by index; the heuristic ones moved further than any stage
  before (Corp agenda wins 66 → 77 of 192, Runner agenda wins 111 → 99,
  deck-outs 0 → 1, flatlines 15 unchanged). Checked, not inferred: a ref
  with the engine changes — the two kinds and their tally, the copy's
  rez count and its amount — and without the three cards and the deck
  swaps is **identical in all four shapes**, so the movement is
  `determinize` sampling three new pieces of Corp ice, which is a
  measurement of the bots rather than of the rules.

#### Stage 5c — derez words (28 September 2026)

`feat/rwr-stage-5c-brasilia-warm-reception`: Brasília Government Grid,
Warm Reception. **No new `Effect` or `Trigger`.** Rebellion Without
Rehearsal 47 of 65; `RWR_UNIMPLEMENTED` 20 → 18. Lightning Laboratory
moved to 5d: its "When this turn ends, derez 2 pieces of ice protecting
that server" is a delayed conditional ability (CR 9.6.13), which nothing
in the engine is yet.

- **Brasília Government Grid** reacts as itself and acts on the rezzed
  ice (`acts_on_subject`, the requirement read as the grid: once per
  turn, a run against its server). "You may derez another installed piece
  of ice. If you do, …" is a nested cost (CR 1.16.11a, as Stoke the
  Embers' reveal): `Cost::Derez` over the other ice, and "the rezzed ice
  gets +3 strength for the remainder of that run" needs a strength on the
  ice resolving, which need not be encountered — a rez comes as the ice
  is approached. `ModifyStrength`'s `each_ice` bool became
  `ice: StrengthOf` (`Encountered`, `EachIce`, `This`); ezaM's file says
  `EachIce`.
- **Warm Reception:** "you may install 1 card from HQ. You cannot score
  that card this turn" is a text install with a rider about the card that
  landed (`PromptInstallCorpCard::if_installed`, beside `then`, which
  resolves as the card that offered it, and `if_rezzed`). The rider is a
  prohibition on that install alone (`Prohibit::this_install`, a
  `lingering::On::Install` — a public handle, so a facedown card is bound
  without being named), and a score asks the install
  (`continuous::cannot_install`), in the action list and the guard alike.
  "If this server is not protected by ice, you may derez this asset to
  derez another installed card" is a nested cost of its own,
  `Cost::DerezSelf`: `Cost::Derez` chooses by a filter, and
  `Not(NotSourceCard)` is read off a definition, so it would take every
  copy.
- **Client:** nothing new reaches the view (a prohibition on an install
  rides in `lingering`, which both clients draw); `hud::in_effect` says
  "the Corp cannot score the card it installed", and `prose` has words for
  the strength's ice, the prohibition and the cost.
- **Deck** — Retirement Package: two Brasília Government Grid for two
  Seamless Launch, and two Warm Reception for two Predictive Planogram.
- **Real play**, 96 games of Retirement Package against Safety Net
  (seed 2). Random seats: Brasília Government Grid rezzed 129 times (more
  than it was installed: Warm Reception derezzes it) and its rez trigger
  fired 42 times; Warm Reception's turn start fired 326 times. Heuristic
  seats: the grid's trigger fired 24 times, Warm Reception's 1,054.
- **Fidelity limits:** the grid's "once per turn" is spent when its
  trigger fires, whether its "may" is taken or not — the deviation from
  CR 9.3.6g already recorded for Malandragem (`rules-conformance.md`,
  9.3).
- **DSL ratio (`pool_status.py`): 17 of 80 `Effect` variants single-use,
  2 unused**, over 297 card files (17 of 80 over 295 before).
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (5b), `coverage_identical.py` has the random seatings **identical**, by
  view and by index; the heuristic ones moved (Corp agenda wins 77 → 79
  of 192, flatlines 15 → 14, Runner agenda wins 99 → 98). A ref with the
  engine changes — the strength's ice, the cost, the rider and the
  prohibition on an install, and ezaM's file rewritten to the new word —
  and without the two cards and the deck swap is **identical in all four
  shapes**, so the movement is `determinize` sampling two new Corp cards.

#### Stage 5d — a delayed conditional ability (28 September 2026)

`feat/rwr-stage-5d-lightning-laboratory`: Lightning Laboratory. **One new
`Effect`.** Rebellion Without Rehearsal 48 of 65; `RWR_UNIMPLEMENTED`
18 → 17.

- **"When this turn ends, derez 2 pieces of ice protecting that server"**
  resolves after the resolution that made it. Until now nothing could do
  that except a run's own end rider (`SetRunEndedEffect`, one slot on the
  run). It is a delayed conditional ability (CR 9.6.13):
  `Effect::WhenThisTurnEnds` leaves a `lingering::DelayedAbility` on
  `GameState::delayed`, dated by the turn and waiting for
  `Trigger::OnDiscardPhaseEnd` (CR 5.6.3d: the turn and the discard phase
  end at one step). "That server" is the attacked server, written in as
  the ability is made (`Effect::with_attacked_server`, which rewrites
  `CardFilter::InAttackedServer` to `InServer`), because the run is over
  by the time it resolves.
- **It is heard like a trigger** (`listeners::plan_for`): it goes in
  beside its side's reactions to the turn's end, so its controller orders
  it with them, and it resolves as the card that made it.
  `dispatcher::dispatch_event` takes it off the list as it is planned, so
  it resolves once (9.6.13c), and drops any left from an earlier turn. A
  pending decision is no obstacle: it is a queued continuation like any
  other.
- **The rest composes:**
  - "Whenever a run begins, you may remove 1 hosted agenda counter" is an
    `OfferPaidChoice` of `Cost::RemoveCounters` on the scored agenda.
  - "Rez up to 2 pieces of ice protecting the attacked server, ignoring
    all costs" is two selections of one card, each a free `RezInstalled`.
    Additional costs are ignored too (CR 1.16.5c), so Piranhas's bad
    publicity is never asked for.
  - "Derez 2" is two selections that must each take a card while one is
    there, so a server with one rezzed piece of ice derezzes that one.
- **Client:** the view carries the list (`ClientView::delayed`).
  `hud::in_effect` draws it in both clients as "when this turn ends, …",
  in `prose`'s words, and a bot's sample resolves it too.
- **Fixed on the way:** `Effect::for_each_effect` now walks a text
  install's `if_installed` rider, which 5c left out.
- **Deck** — Retirement Package: two Lightning Laboratory for two
  Offworld Office, point for point.
- **Real play**, 96 games of Retirement Package against Safety Net
  (seed 2). Random seats: scored 6 times and stolen 77, its run-start
  offer made 12 times. Heuristic seats: scored 43 times and stolen 36, the
  offer made 97 times.
- **DSL ratio (`pool_status.py`): 18 of 81 `Effect` variants single-use,
  2 unused**, over 298 card files (17 of 80 over 297 before). The new
  variant is `WhenThisTurnEnds`, which Test Run and the later sets'
  "when this turn ends" riders will reuse.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (5c), `coverage_identical.py` has the random seatings **identical**, by
  view and by index; the heuristic ones moved (Corp agenda wins 79 → 75
  of 192, flatlines 14 → 13, Runner agenda wins 98 → 103). A ref with
  the engine changes — the effect, the delayed ability and its hearing,
  the view's list — and without the card and the deck swap is
  **identical in all four shapes**, so the movement is `determinize`
  sampling a new Corp agenda.

#### Stage 5e — a resolution's end, and a trash from R&D (28 September 2026)

`feat/rwr-stage-5e-nuvem-basalt-spire`: Nuvem SA: Law of the Land, The
Basalt Spire, and Nuvem's Sweep deck, Land Grab. **No new `Effect`; two
new `Trigger`s.** Rebellion Without Rehearsal 50 of 65;
`RWR_UNIMPLEMENTED` 17 → 15. **Stage 5 is complete.**

- **"Whenever you finish resolving an operation or an action on an
  expendable card"** is a moment the engine did not have. An operation's
  play is heard before it resolves (`OnCardPlayed`), and its resolution
  may be parked across several actions, which nothing marked. CR 8.6.7h
  puts "conditions related to … finishing resolving" after the play
  abilities and the trash, and that end is now `GameEvent::
  FinishedResolving`, heard by `Trigger::OnFinishedResolving` about the
  card. `dispatcher::finished_resolving` announces it at once when
  nothing waits. Otherwise it goes on the queue behind whatever does
  (`DeferredTrigger::announce`), because the queue is where the rest of a
  parked resolution is. An operation announces it at the end of
  `engine::play_operation_card`, whether played by the basic action or by
  a card's text. An expendable card announces it when an action
  (`[click]`) ability used from HQ has resolved, which is Tocsin's today.
- **"The first time you trash a card from R&D during each of your turns"**
  is `GameEvent::CardsTrashedFromRnD { count, by }`, the R&D twin of AU
  Co.'s HQ batch, heard by `Trigger::OnCardsTrashedFromRnD` as the
  trasher's moment (CR 1.14.5). It is emitted from a mill
  (`MillRnDAmount`, which Nuvem's "you may trash that card" takes off
  the unused list), a cost (`Cost::Trash` from R&D) and a selection into
  Archives alike. Nuvem's first-time ability is `first_each_turn` over it
  with `DuringYourTurn`. The Runner's trash of a card accessed in R&D is
  no Corp trash, and is not in the batch.
- **The Basalt Spire:**
  - Its "hosted agenda counter, trash the top card of R&D:" cost is
    `RemoveCounters` with a `Cost::Trash` from R&D over
    `CardFilter::TopOfZone(1)`.
  - That found a bug: a cost that trashed out of a deck took the first
    copy of the card from the bottom, not the one at the position chosen,
    so with a duplicate lower down the wrong card went. It now takes the
    chosen copy, as a selection out of a deck already did.
  - Its steal and score abilities compose.
- **Client:** nothing new reaches the view. Both clients' logs say
  "trashed N card(s) from R&D", and a resolution's end is not narrated:
  the action line already says the operation was played.
- **Deck** — Land Grab, a new Sweep deck on Nuvem SA over Hostile Bid's
  frame: three Hostile Takeover out for two The Basalt Spire (23 points in
  50 cards), and a third copy of Hedge Fund, Measured Response, Myōshu,
  Business As Usual, Tocsin and Logjam in. It is pinned Eternal-only, and
  the sweep schedule re-rolls with a Corp deck more, as it did for
  Vantage Point's Sweep decks.
- **Real play**, 96 games of Land Grab against Safety Net (seed 2):
  - Random seats: Nuvem's look fired 154 times and its R&D gain 68.
  - Heuristic seats: the look fired 146 times and the gain 70; The Basalt
    Spire was scored 28 times.
  - **The Basalt Spire's ability was used in neither seating**, and the
    heuristic Corp never used Tocsin from HQ. The card test is their reach
    so far.
- **Fidelity limits:** only Tocsin is an expendable card in the pool, and
  its action is the one announced. An expendable card's ability that is
  not an action announces nothing, as the card says.
- **DSL ratio (`pool_status.py`): 19 of 81 `Effect` variants single-use,
  1 unused**, over 300 card files (18 of 81, 2 unused, over 298 before).
  The move is `MillRnDAmount`, from unused to used by one card.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (5d), `coverage_identical.py` has the random seatings moved **only in
  the two new events' counts** (`FinishedResolving` 0 → 395,
  `CardsTrashedFromRnD` 0 → 15; every other section of the report is
  identical, so no play moved), and the heuristic ones moved (Corp agenda
  wins 75 → 70 of 192, flatlines 13 → 11, Runner agenda wins 103 → 110).
  A ref with the engine changes and without the two cards and the deck
  differs from `main` in exactly those event counts in all four shapes
  and in nothing else, so the heuristic movement is `determinize`
  sampling two new Corp cards.

#### Stage 6a — terminal operations (28 September 2026)

`feat/rwr-stage-6a-terminal-active-policing-bring-them-home`: Active
Policing, Bring Them Home. **Two new `Effect`s.** Rebellion Without
Rehearsal 52 of 65; `RWR_UNIMPLEMENTED` 15 → 13.

- **"After you resolve this operation, your action phase ends"** is CR
  5.4.3, which nothing did until now: an action phase ended only by the
  basic pass with no clicks left. It is `Effect::EndActionPhase`, last in
  each card's resolution, which is where "after" puts it. So a decision
  the operation parks first is answered first, and so is the threat's
  offer. It announces the action phase's end (`ActionPhaseEnded`) and goes
  on to the discard step (`turn::force_action_phase_end`). Clicks left are
  not spent. They are lost with the turn (5.6.3c), and nothing can spend
  them first, because neither the discard step nor the end-of-turn window
  admits an action (5.4.3d). Zeroing the clicks instead was rejected: a
  Corp at 0 clicks still has the action phase's paid ability window, with
  its rezzes and scores, and 5.4.3a skips it. Outside its player's action
  phase it does nothing (5.4.4).
- **"If the Runner stole or trashed a Corp card during their last turn"**
  needed more of the last turn than its row totals. The Runner's trash of
  their own program is in the same row. `GameState::last_turn` is now the
  whole `TurnLog` of the turn that ended (the `LastTurn` totals type is
  gone). `Amount::TimesLastTurnWhen` narrows it as `TimesThisTurnWhen`
  narrows this turn's. `EventFilter::OwnedBy { owner, whose }` says "the
  Runner's moment, about a Corp card"; neither `Whose` nor a `Card` filter
  says both.
  - The log can count it because every card it counts `Unseen` is a Corp
    card. So the Corp's columns are those and every Corp type.
  - The requirement is stolen, trashed as accessed, or trashed by the
    Runner, written `Not(And(Not, And(Not, Not)))`, because there is no
    `Or`.
- **"Reveal and add 2 cards at random from the grip to the top of the
  stack"** is `Effect::RevealAtRandom { side, count, then }`:
  - The cards are drawn together, before any moves, and each is revealed
    (`CardRevealed`). `then` resolves as each card.
  - `validate` refuses a `then` that could park, since the cards after it
    would be dropped.
  - The revealed card is marked on the resolution context
    (`revealed_in_hand`), so `AddToDeck` takes it from the grip. Otherwise
    it searches the heap first and moves another copy. It also moves the
    card in the open.
  - The threat's "the Runner shuffles it into the stack" is `AddToDeck`
    then `ShuffleIntoDeck([])`, which now shuffles the stack alone when it
    names no zone.
- **Client:** nothing new reaches the view but the rest of the last turn's
  log, which `view_ledger` already had as the engine's. Both clients' logs
  now say "the Corp's action phase ended" when anything but the basic pass
  ends it, and name a card added to a deck in the open ("added Sure Gamble
  to the top of the stack").
- **Decks:**
  - Retirement Package: two Active Policing for the Predictive Planogram
    and the Hedge Fund.
  - A Thousand Cuts: two Bring Them Home for the Regolith Mining License
    and a Hedge Fund.
  - Pay As You Go: an Alarm Clock and a Sure Gamble for its Gordian Blade
    and a Crash Space.
- **Why Pay As You Go changed:** the 256-seed view sweep's coverage gate
  failed on Alarm Clock, never installed across 768 games once the Corp
  decks it met had changed.
  - It was one copy in Spare Parts, which has no influence left for a
    second.
  - Measured over 64 games against A Thousand Cuts, it was installed in 7
    random games and in no heuristic one. At about 15 Spare Parts seeds a
    seating, the gate had about a one-in-six chance of missing it on
    `main` too.
  - Borrowed Time, in-faction, is the one Runner Sweep deck of Startup
    cards and would lose Startup, and Grassroots has no influence either.
- **Real play**, 96 games against Safety Net (seed 2):
  - Active Policing (Retirement Package): played 40 times by random seats
    and **never by the heuristic Corp**.
  - Bring Them Home (A Thousand Cuts): played 23 times in both seatings.
- **Fidelity limits:**
  - A terminal card's end does nothing mid-run or with a window open (CR
    5.4.3b closes them, and the engine has no step for it). No terminal
    card in the pool resolves anywhere but its player's action window.
  - Nuvem SA's "finish resolving" is announced after the action phase has
    ended, not while it is ending. The two are out of faction together.
- **DSL ratio (`pool_status.py`): 20 of 83 `Effect` variants single-use,
  1 unused**, over 302 card files (19 of 81 over 300 before).
  `EndActionPhase` is used by both cards, and `RevealAtRandom` by Bring
  Them Home.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (5e), `coverage_identical.py` has both random seatings **identical**: the
  pass plays the sample decks' matchups, and every deck changed is a Sweep
  deck. The heuristic ones moved (Corp agenda wins 70 → 71 of 192,
  flatlines 11 → 14, Runner agenda wins 110 → 106). A ref with the engine
  changes and without the two cards and the decks is identical to `main`
  in all four shapes, so the engine moved nothing a game shows, and the
  heuristic movement is `determinize` sampling two new Corp cards.

#### Stage 6b — cards that stay revealed (28 September 2026)

`feat/rwr-stage-6b-burner`: Burner. **No new `Effect`.** Rebellion Without
Rehearsal 53 of 65; `RWR_UNIMPLEMENTED` 13 → 12. Cataloguer went to 6c
(above): its "[click], hosted power counter: Breach R&D" is a breach with
no run, which the access machinery, built on `RunState`, has no door for.

- **"Reveal 3 cards in HQ at random. Add 2 of the revealed cards to the
  top and/or bottom of R&D"** puts a choice after the reveal, among the
  cards it revealed. CR 1.21.6 keeps them visible "until the entire
  ability is finished resolving or the card moves", and the choosing
  parks, so the list is on the state: `GameState::revealed`, one
  `RevealedCard { side, card }` a card.
  - An entry comes off as its card moves (`AddToDeck`), and the list is
    emptied when an action ends with nothing parked or queued.
  - `CardFilter::Revealed` selects among the list. It works by card, not
    position: a hand has no order either player sees, and two copies of
    one card are the same card.
  - Burner is `SetAccessReplacement` on HQ, then `RevealAtRandom` with
    nothing to do for each card, then two selections of one revealed card.
    Each selection's `then` offers the top or the bottom of R&D.
  - A selection of at least one with nothing eligible is skipped, so a
    thinner HQ adds what it has.
- **`RevealAtRandom`'s `then` is now `each`, and optional.** 6a's
  resolution-context flag (`revealed_in_hand`) is gone. `AddToDeck` reads
  the list instead, so it takes a revealed card from the hand it was
  revealed in, and in the open, whichever effect moves it.
- **The view carries the list** (`ClientView::revealed`: shown to both, so
  the same to both). `hud::in_effect` draws it in both clients ("revealed
  in HQ: Hedge Fund"), and the chooser's pop-up already draws the
  candidates as their cards.
- **`determinize` seats each revealed card in the sampled hand.** Revealed
  is known, and a sample that drew HQ anew had nothing for Burner's
  selection to offer, so both it and `resample_hidden` put the revealed
  cards back over cards that are not.
- **Deck** — Safety Net: two Burner for the two Jailbreak, a run on HQ for
  a run on HQ or R&D.
- **Real play**, 96 games of Retirement Package against Safety Net (seed
  2): played 36 times by random seats, with 15 breaches of HQ replaced;
  **never by the heuristic Runner**.
- **DSL ratio (`pool_status.py`): 19 of 83 `Effect` variants single-use,
  1 unused**, over 303 card files (20 of 83 over 302 before):
  `RevealAtRandom` has its second card.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (6a), `coverage_identical.py` has both random seatings **identical** and
  the heuristic ones moved by one game (Corp agenda wins 71 → 70 of 192,
  Runner agenda wins 106 → 107). A ref with the engine changes and without
  Burner and the deck is identical to `main` in all four shapes, so the
  heuristic movement is `determinize` sampling a new Runner card.

#### Stage 6c — a breach with no run (28 September 2026)

`feat/rwr-stage-6c-cataloguer`: Cataloguer. **One new `Effect`.** Rebellion
Without Rehearsal 54 of 65; `RWR_UNIMPLEMENTED` 12 → 11.

- **"[click], hosted power counter: Breach R&D"** is CR 7.3.1's "card
  abilities can also directly instruct the Runner to breach a server",
  which nothing did: every breach began at a run's success step.
  `Effect::Breach(server)` (`run::start_breach`) stands the breach in a
  `RunState` flagged `breach_only`, with no ice and nothing declared
  successful, because the access machinery, 139 references to
  `access_state` across the workspace, is the run's.
  - Moving the access state out of the run was rejected for its size.
    The flag refuses what only a run has.
  - `GameState::run_in_progress` is what a card means by "during a run",
    and it is none: "during a run on", "a run against this server" and
    "once per run" ask it. `DuringRun` already excluded the access, and
    an encounter or "this card started the run" cannot hold with no ice
    and no event.
  - The breach's end is no `RunCompleted` a card hears, and it leaves
    `last_completed_run` the last *run*.
  - It is refused inside a run or another breach. CR 7.3.8 would delay
    it, and nothing in the pool breaches from inside one.
  - What blocks actions mid-access still reads `active_run`, so nothing
    else is taken while the Runner accesses.
- **"Instead of breaching R&D, you may remove 1 hosted power counter to
  look at the top 4 cards of R&D and arrange them in any order"** composes:
  - An optional `SetAccessReplacement` from a successful-run trigger.
  - The counter's removal, `LookAtTopOfDeck`, and the Runner's selection
    of the top four back onto R&D, the first chosen on top.
  - The replacement now remembers the install that set it
    (`RunState::access_replacement_install`), so the counter comes off
    the copy that offered it, not the first of two.
- "When it is empty, trash it" is Juli Moreira Lee's shape, after each
  removal.
- **Client:** the view carries the flag (`PublicRunState::breach_only`).
  Both clients' run displays say "Breach of R&D":
  - `board::phase` gives the breach a segment with its access as the one
    step, which the desktop's panel draws.
  - The terminal's run strip says it without counting ice.
- **The fog gate learned a chooser's own selection.** The 256-seed view
  sweep failed at seed 167 on the Runner's arrangement: its
  `CardsSelected` names R&D cards back in a deck the Runner's view no
  longer shows. The mask leaves a selection's names to its chooser, who
  picked each card from the list their view held (`ClientView::
  selection`), so the gate now counts them as seen, as it did the
  chooser's own action.
- **Deck** — Spare Parts: two Cataloguer for a T400 Memory Diamond and a
  Sure Gamble.
- **Real play**, 96 games of Retirement Package against Spare Parts (seed
  2): random seats installed it 30 times and breached with it 3 times,
  and the replacement was offered 15 times. **The heuristic Runner never
  installed it.**
- **Fidelity limits:**
  - A breach with no run that ends the game mid-access still records a
    `RunCompleted`, pushed and not dispatched, because the game's end has
    already taken the run.
  - The breach is shown with the run's panel and trail.
- **DSL ratio (`pool_status.py`): 20 of 84 `Effect` variants single-use,
  1 unused**, over 304 card files (19 of 83 over 303 before). `Breach` is
  Cataloguer's until the pool's other breach cards (Hades Shard, Raymond
  Flint) arrive.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (6b), `coverage_identical.py` has both random seatings **identical** and
  the heuristic ones moved (Corp agenda wins 70 → 74 of 192, flatlines
  14 → 12, Runner agenda wins 107 → 105). A ref with the engine changes
  and without Cataloguer and the deck is identical to `main` in all four
  shapes, so the heuristic movement is `determinize` sampling a new
  Runner card.

#### Stage 6d — cards set aside (28 September 2026)

`feat/rwr-stage-6d-wizards-chest`: The Wizard’s Chest. **One new `Effect`,
and one renamed to take a zone.** Rebellion Without Rehearsal 55 of 65;
`RWR_UNIMPLEMENTED` 11 → 10.

- **"Set aside cards from the top of your stack faceup until you set aside
  2 cards of the chosen type"** is CR 4.8's set-aside zone, which no card
  had used. The cards wait there while the Runner chooses one to install,
  and that choice parks, so the zone is on the state:
  `RunnerState::set_aside`.
  - The zone is shared by both players (4.8.1). Only the Runner's text
    sets anything aside yet, so only the Runner's side has a list.
  - The cards are faceup (4.8.6), so it is public: in `PublicRunnerState`,
    copied by `determinize`, and struck from the sampled pools.
  - `Effect::SetAsideFromTopUntil { filter, count }` reads down the stack
    until `count` cards match or the stack runs out, and announces
    `GameEvent::CardsSetAside`, which no card hears.
  - The rest is `CardZoneRef::OwnSetAside`: a selection over it, an
    install out of it (`RunnerCardSource::SetAside`), and
    `ShuffleIntoDeck([OwnSetAside])` for "shuffle the rest of the
    set-aside cards into your stack".
- **"You may install 1 of those 2 cards, ignoring all costs"** installs
  out of that zone. `InstallRunnerCardFromHeap(Discount)` took the zone
  and became `InstallRunnerCardFromZone { from, discount }`, as
  `PlayOperation { from }` once took Plutus's, rather than a fifth install
  variant beside the four. Scrounge's, Privileged Access's and Magdalene
  Keino-Chemutai's files say `from: OwnHeap`.
- "Choose hardware, program, or resource" is a `PresentChoice` of three.
  "Use this hardware only if you made a successful run on HQ, R&D, and
  Archives this turn" is Chain Reaction's requirement.
- **Client:** `hud::in_effect` lists the set-aside cards in both clients
  ("set aside: Sure Gamble, Corroder"). The log says what was set aside,
  and the chooser's pop-up draws the ones it offers as their cards.
- **Deck** — Grassroots: two The Wizard’s Chest for the Scrounge and a
  Sure Gamble.
- **Real play**, 96 games of Retirement Package against Grassroots (seed
  2): random seats installed it 44 times; the heuristic Runner never did.
  **Neither seating ever used its ability**, since three successful runs
  in a turn are rare, so the card test is its reach.
- **DSL ratio (`pool_status.py`): 21 of 85 `Effect` variants single-use,
  1 unused**, over 305 card files (20 of 84 over 304 before). The new one
  is `SetAsideFromTopUntil`.
- **Measured.** Both sweeps at 256 seeds are green. A ref with the engine
  changes and without the card and the deck differs from `main` (6c) in
  the rename alone, in all four shapes: `effects_seen` counts the same
  heap installs as `InstallRunnerCardFromZone` (202 in the random
  seatings, 206 in the heuristic), and nothing else moved. With the card,
  both random seatings are the same, and the heuristic ones moved by a
  game or two (Corp agenda wins 74 → 73 of 192, flatlines 12 → 11, Runner
  agenda wins 105 → 107): `determinize` sampling a new Runner card.

#### Stage 6e — an X cost, and the object that fully breaks (28 September 2026)

`feat/rwr-stage-6e-lobisomem`: Lobisomem. **No new `Effect`.** Rebellion
Without Rehearsal 56 of 65; `RWR_UNIMPLEMENTED` 10 → 9. **Stage 6 is
complete.**

- **"Interface → X[credit], hosted power counter: Break X barrier
  subroutines"** is CR 1.16.2c: the payer chooses and announces X before
  paying. It is `Cost::CreditsX { max }`, asked by the payment's own
  replay (`payment::Ask::X`, answered by `PlayerAction::ChooseNumber`, so
  `ActionSpace` is unmoved).
  - X runs from 0 to the printed bound, capped by what the payer could
    spend and by `MAX_CHOSEN_NUMBER`. Lobisomem's bound is the encountered
    ice's subroutines.
  - X is then paid from wherever credits may be.
  - X reaches the effect the way every chosen number does: written into it
    (`Effect::with_chosen_number`, the State Hygiene Rule).
    `SubroutineBreakCount::ChosenNumber` is the placeholder, and
    `activate_ability` writes the payment's first answer over it.
  - That answer is X because `validate` holds an X to the front of its
    ability's cost, which is also where 1.16.2c puts it.
- **"Whenever it fully breaks a code gate"** is CR 6.5.7b: "if all its
  subroutines were broken using abilities on a single object, that object
  also fully breaks the ice".
  - The encounter's tally now says whose breaks they were
    (`EncounterTally::broken_by`: nothing yet, one install, or mixed; a
    click is no object's). `IceFullyBroken` carries the object (`by`), and
    the moment does too (`Moment::by`).
  - `EventFilter::ByThis` says "it", and only on `OnIceFullyBroken`, the
    one moment that names an object that did it. "A code gate" is the
    trigger's `Encountering(CodeGate)`, because a `when` holds one filter
    and the moment is in the encounter.
  - A tally rather than a mark on each subroutine: the one question is
    whether every break was one install's, and about thirty subroutine
    literals would have taken a field.
- **Client:** both clients word the question ("Choose X, from 0 to 2 — you
  pay X credits"), and the pop-up's buttons are its numbers. The tally
  rides in the view as the engine's, beside the encounter's other counts.
- **Deck** — Safety Net: two Lobisomem for a Gordian Blade and a Sure
  Gamble, a decoder for a decoder.
- **Real play**, 96 games of Retirement Package against Safety Net (seed
  2):
  - Heuristic seats: installed 29 times, abilities used 169 times, and a
    code gate fully broken by it alone 101 times. The Runner chose 11
    numbers against 11 payment questions.
  - Random seats: installed 11 times, with one code gate fully broken by
    it.
- **DSL ratio (`pool_status.py`): 21 of 85 `Effect` variants single-use,
  1 unused**, over 306 card files, unchanged: the X is a cost and a
  break count, not an effect.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (6d), `coverage_identical.py` has both random seatings **identical**, and
  the heuristic ones moved (Corp agenda wins 73 → 77 of 192, flatlines
  11 → 12, Runner agenda wins 107 → 102). A ref with the engine changes
  and without Lobisomem and the deck is identical to `main` in all four
  shapes, so the heuristic movement is `determinize` sampling a new
  Runner card.

#### Stage 7a — expendable cards (28 September 2026)

`feat/rwr-stage-7a-expendable-eminent-domain-descent`: Eminent Domain,
Descent. **No new `Effect`.** Rebellion Without Rehearsal 58 of 65;
`RWR_UNIMPLEMENTED` 9 → 7.

- **"[click], 1[credit], reveal and trash this … from HQ:"** is VP Stage
  7f's action from HQ (`AbilityDef::from_hand`, `Cost::RevealAndTrashSelf`,
  `PlayerAction::ActivateHandAbility`), on an agenda for the first time.
  Nothing about it was ice-shaped.
- **"Install and rez 1 card from HQ, paying a total of 5[credit] less"** is
  Reanimation Protocol's `PromptInstallCorpCard { rez, discount }`, from
  HQ: the install takes what it can of the total and the rez the rest (CR
  1.16.2f). **"Search R&D for 1 card. Install and rez that card, ignoring
  all costs"** is the same from R&D, after a search that shuffles first
  (CR 8.7.3), as Poétrï's install from R&D is.
- **An "install and rez" may choose a card it cannot rez** (CR 8.5.13d),
  and the card is then revealed. That was new to the engine. An agenda
  (CR 8.1.2c) stays installed facedown and is revealed (`CardRevealed`,
  and the Runner remembers it as an accessed card, `seen_by_runner`);
  `rez_install` would have rezzed it. A card the Corp cannot afford to rez
  is revealed as well, which also reaches Reanimation Protocol.
- **"Reveal up to 2 agendas in HQ and/or Archives and shuffle them into
  R&D"** is a `PresentChoice` of from HQ, from Archives, one from each, or
  none, as Sleipnir's "from HQ or Archives" is. Every selection shuffles,
  so a one-from-each whose Archives half finds nothing still leaves R&D
  shuffled.
- **"When your turn begins, you may add this ice to HQ"** is Janaína's
  `Cost::AddSelfToHq` as an `OfferPaidChoice` with nothing after it. The
  add is the card's effect, not a cost, but nothing prevents a card going
  to HQ and nothing follows it, so the two read the same. The alternative
  was an `Effect` one card would use. Active only while rezzed, as an
  installed card's trigger is.
- **Client:** nothing new reaches the view. A card revealed on the table
  is drawn as a remembered Corp card, and the log already says "revealed
  Offworld Office".
- **Deck** — Land Grab, whose Nuvem SA looks at the top of R&D after each
  action on an expendable card: two Eminent Domain for a Sacrifice
  Zone Expansion and a Hedge Fund (23 agenda points still), and two
  Descent for two Ice Wall.
- **Real play**, 96 games of Land Grab against Safety Net (seed 2):
  - Random seats: Descent used from HQ 9 times and installed 102, Eminent
    Domain used from HQ 18 times and scored 3.
  - The heuristic Corp: never used either from HQ, as it never uses
    Tocsin. It scored Eminent Domain 21 times, and Descent's turn-start
    offer came 559 times.
- **DSL ratio (`pool_status.py`): 21 of 85 `Effect` variants single-use,
  1 unused**, over 308 card files, unchanged.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`
  (6e), `coverage_identical.py` has both random seatings **identical**, and
  the heuristic ones moved (Corp agenda wins 77 → 73 of 192, flatlines
  12 → 15, Runner agenda wins 102 → 103). A ref with the reveal and
  without the cards and the deck is identical to `main` in all four
  shapes, so the heuristic movement is `determinize` sampling new Corp
  cards.

#### Stage 7b — a psi game (28 September 2026)

`feat/rwr-stage-7b-psi-see-how-they-run`: See How They Run. **One new
`Effect`.** Rebellion Without Rehearsal 59 of 65; `RWR_UNIMPLEMENTED` 7 → 6.

- **"Play a Psi Game. If the bids differ, do 1 core damage. If the bids
  match, do 1 net damage"** is CR 10.14.6, which no card had needed. It is
  `Effect::PsiGame { on_match, on_differ }`, which parks a
  `PendingDecision::PsiGame`. The Corp bids first, then the Runner, each
  with `PlayerAction::ChooseNumber`, so `ActionSpace` is unmoved. The
  Runner's bid reveals both (`GameEvent::PsiBidsRevealed`), both are
  spent, the active player's first (10.14.4c), and the outcome resolves as
  the card that asked, all in that one action.
  - Each bids 0 to 2, and no more than they could spend (10.14.3,
    `payment::available`, fixed as the game begins). A player who can bid
    only 0 is not asked, so a Runner with nothing ends the game at the
    Corp's bid.
  - **The Corp's bid is hidden until the Runner has bid.** It is
    `PsiBid::Concealed` in the Runner's view and a spectator's. It is
    Méliès U's secret `NumberChosen` in the log, which hides the answer
    and drops the event for the other seat. A bot's sample guesses it
    uniformly from the bids it could be (`determinize::sample_decision`).
    The reveal teaches the Runner something (`may_teach_the_actor`), as a
    trace's bids do.
  - Composition didn't work. Two secret `ChooseNumber`s cannot compare
    their numbers, since a nested choice keeps its own placeholder.
    Branching on the Corp's bid would put the bid into the continuation
    the Runner's decision carries, which a view passes through whole.
- **Client:** both clients word each chair's side of it: "play a Psi Game
  — bid in secret, 0 to 2 credits" to the bidder, "the Corp is bidding in
  secret" and "the Corp has bid" to the Runner, and "you bid 2" to the
  Corp while the Runner bids. The buttons are the bids. The log reads
  "revealed the psi game's bids: the Corp 2[credit], the Runner
  1[credit] — the bids differ". `view_ledger` has every field of the new
  decision.
- **Deck** — A Thousand Cuts: two See How They Run for two Proprionegation,
  a 2-point, 4-advancement agenda for another.
- **Real play**, 96 games of A Thousand Cuts against Safety Net (seed 2):
  - Random seats never scored it: the random Corp flatlined in 80 games.
  - The heuristic Corp scored it once, and that game played its psi game
    to the end. The sweeps and the card tests carry the rest.
- **DSL ratio (`pool_status.py`): 22 of 86 `Effect` variants single-use,
  1 unused**, over 309 card files (21 of 85 before). `PsiGame` is used by
  See How They Run alone for now; Adrian Seis, Konjin and Hyoubu Precog
  Manifold print one too.
- **Measured.** Both sweeps at 256 seeds are green, the fog gate included.
  Against `origin/main` (7a), `coverage_identical.py` has both random
  seatings **identical**, and the heuristic ones moved (Corp agenda wins
  73 → 76 of 192, flatlines 15 → 16, Runner agenda wins 103 → 100). A ref
  with the engine, bot and client changes and without the card and the
  deck is identical to `main` in all four shapes, so the heuristic
  movement is `determinize` sampling a new Corp card.

#### Stage 7c — ice that moves as a run begins (28 September 2026)

`feat/rwr-stage-7c-tributary`: Tributary. **One new `Effect`.** Rebellion
Without Rehearsal 60 of 65; `RWR_UNIMPLEMENTED` 6 → 5.

- **"The first time each turn a run begins, you may move this ice to the
  outermost position protecting the attacked server. (The Runner will
  approach this ice.)"** is an `OnRunStart` trigger with
  `first_each_turn`, `subject: Any`, and a `PresentChoice` of
  `Effect::MoveThisIceToOutermost` or nothing.
  - The effect takes the ice out of `corp.installed` and puts it back in
    front of the attacked server's ice, which that list keeps
    outermost-first per server (CR 6.2.2a). It keeps its rez state,
    counters and install id, and `CardMoved` is emitted when it changes
    server.
  - The run follows through `run::reconcile_ice`: a run at initiation
    re-anchors on position 0, so the Runner approaches the moved ice
    (6.2.5a puts the Runner's first position at 6.9.1d, after the run has
    begun).
  - Composition didn't work: the only move of ice was `SwapInstalledIce`,
    which cannot take ice to another server or in front of a server with
    no ice.
  - The offer is made on the turn's first run even when Tributary is
    already outermost there, where accepting does nothing. Refusing it
    there would take a requirement no other card needs.
- **"You may install 1 piece of ice from HQ protecting another server,
  ignoring all costs"** is `PromptInstallCorpCard` with a new word,
  `another_server`: every destination but the acting install's server,
  as `remote_only` narrows Peer Review's. **"Each piece of ice gets +2
  strength for the remainder of this run"** is ezaM's `ModifyStrength`
  on `EachIce`.
- **Client:** nothing new reaches the view. The board draws ice where the
  view has it, the log already says "moved Tributary from R&D to HQ", and
  the run's strength is in `hud::in_effect` as ezaM's is.
- **Deck** — A Thousand Cuts: two Tributary for a Karuna and an ezaM, ice
  for ice. Tributary is banned in Standard, and the Sweep decks are
  Eternal.
- **Real play**, 96 games of A Thousand Cuts against Safety Net (seed 2):
  - Random seats: installed 49 times and rezzed 15. The run-start offer
    came 60 times and the ice moved 26 times; 130 subroutines fired.
  - Heuristic seats: installed 68 and rezzed 35. The offer came 139 times
    and the ice moved 49 times; 202 subroutines fired.
- **DSL ratio (`pool_status.py`): 23 of 87 `Effect` variants single-use,
  1 unused**, over 310 card files (22 of 86 before).
- **Measured.** Both sweeps at 256 seeds are green. Against 7b,
  `coverage_identical.py` has both random seatings **identical**, and the
  heuristic ones moved (Corp agenda wins 76 → 79 of 192, flatlines
  16 → 14, Runner agenda wins 100 → 96, Runner deck-outs 0 → 3). A ref
  with the effect, the install word and the prose and without the card
  and the deck is identical to 7b in all four shapes, so the heuristic
  movement is `determinize` sampling a new Corp card.

#### Stage 7d — an encounter repeated (28 September 2026)

`feat/rwr-stage-7d-sisyphus-protocol`: Sisyphus Protocol. **One new
`Effect`.** Rebellion Without Rehearsal 61 of 65; `RWR_UNIMPLEMENTED` 5 → 4.

- **"The first time each turn the Runner passes a rezzed code gate or
  sentry"** is `OnIcePassed` with `first_each_turn`, `subject: Any` and
  `acts_on_subject`, narrowed by a new `IceFacts` word,
  `rezzed_code_gate_or_sentry`.
  - The pass event carries the ice's type if it was rezzed as it was
    passed (`IcePassed::rezzed_as`, public), and the moment's fact is read
    off the event, as the other facts are.
  - A fact rather than a type because the facts are bits of a turn-log
    column. The fourth doubles the ice's columns to 16, still within the
    table's widest set (28), which a compile-time assertion now holds.
- **"You may pay 1[credit] or trash 1 card from HQ. If you do, the Runner
  encounters that ice again"** is an `OfferPaidChoice` of `AnyOf`, as
  Cloud Eater's is, whose `if_paid` is `Effect::ForceEncounter`: CR
  6.5.9a's forced encounter, whose own example is The Twins.
  - `run::force_encounter` puts the run back into the encounter at that
    ice's position. It is a new encounter: the ice's printed subroutines
    again, rebuilt from the install, and `lingering::sweep` for what the
    last one bought. It clears the movement phase's window, announces
    `IceEncountered` (so "when encountered" is heard), and opens the
    encounter's window.
  - `RunState::forced_encounter` makes that encounter's end a return to
    the movement phase just inward of the ice, with **no second pass**
    (`pass_current_ice`). An "end the run" ends both (6.5.9b), as any
    encounter's does.
  - Only from the movement phase, and only for ice still protecting the
    attacked server and rezzed; otherwise nothing, as 6.2.8c's "instead
    the Runner does nothing".
  - Composition didn't work: nothing moved a run back into an encounter,
    and every encounter's end was a pass.
- **Client:** the new field is drawn. The desktop's phase bar reads
  "Encounter ice 1 of 2 again" and the terminal's run strip "encountering
  it again"; the encounter panel is the one any encounter gets.
- **Deck** — Honor Roll (Méliès U): two Sisyphus Protocol for two
  Proprionegation, 2 points for 2 (20 still).
- **Real play**, 96 games of Honor Roll against Safety Net (seed 2):
  - The heuristic Corp scored it 24 times, and the offer came 28 times.
  - Random seats never scored it: it was stolen 29 times.
- **DSL ratio (`pool_status.py`): 24 of 88 `Effect` variants single-use,
  1 unused**, over 311 card files (23 of 87 before).
- **Measured.** Both sweeps at 256 seeds are green. Against 7c,
  `coverage_identical.py` has both random seatings **identical**, and the
  heuristic ones moved more than the earlier stages' (Corp agenda wins
  79 → 71 of 192, Runner agenda wins 96 → 104). A ref with the pass
  event's new field, the fact, the forced encounter and the clients, and
  without the card and the deck, is identical to 7c in all four shapes.
  So the whole move is the new card in the bots' samples (`determinize`),
  not a rule; the Sweep decks are never in `matchups()`.

#### Stage 7e — a trojan hosted by an event's ability (28 September 2026)

`feat/rwr-stage-7e-spree`: Spree. **No new `Effect`.** Rebellion Without
Rehearsal 62 of 65; `RWR_UNIMPLEMENTED` 4 → 3. **Stage 7 is complete.**

- **"Place 3 power counters on this event, then run any server"** puts
  counters on a card in the play area (CR 8.6.5), which no card had done.
  They are kept on the run (`RunState::event_counters`), because the
  engine files a played event in the heap and reads the play area off the
  run (`run::run_event`), so the counters go with the run.
  - They are placed by the rider the event leaves on its run
    (`PromptChooseServer::on_start`, `AddCounters(3)`), after the run has
    begun rather than before. Nothing can read them in between.
  - A counter cost and a counter effect find them when the resolution is
    the run event's own (`ability::acting_is_run_event`: its paid ability,
    `InstallId::RUN_EVENT`, or its rider).
- **"Hosted power counter: Host 1 installed trojan program on a piece of
  ice protecting the attacked server"** is the run event's paid ability,
  as Eye for an Eye's is: a selection of a trojan program in the rig, then
  of ice `InAttackedServer`, then GAMEDRAGON™ Pro's
  `HostRigCardOnInstall`. That effect now takes a piece of ice as its
  host (`hosted_on_ice`) as well as a rig card, rather than adding a
  second effect.
  - The trojan is chosen first, so the second selection parks as the
    trojan, and the substitution the effect already had names both
    installs.
  - Nothing withholds the ability with no trojan installed. The rules
    let a player use an ability that turns out to do nothing, and random
    seats spend counters that way.
- **Client:** `hud::in_effect` says "This run: Spree has 2 power counters"
  in both clients, since the play area has no place on the board; a
  trojan's move is the board's, and the log's "hosted".
- **Deck** — Safety Net, whose Stowaway is a trojan: two Spree for a
  Touchstone and a Decoy.
- **Real play**, 96 games of Retirement Package against Safety Net (seed
  2):
  - Random seats: Spree played 45 times and its ability used 90 times, 16
    of those with a trojan to choose.
  - The heuristic Runner never plays it, nor installs Stowaway.
- **DSL ratio (`pool_status.py`): 23 of 88 `Effect` variants single-use,
  1 unused**, over 312 card files (24 of 88 before):
  `HostRigCardOnInstall` has its second card.
- **Measured.** Both sweeps at 256 seeds are green. Against 7d,
  `coverage_identical.py` has both random seatings **identical**, and the
  heuristic ones moved (Corp agenda wins 71 → 78 of 192, flatlines
  14 → 16, Runner agenda wins 104 → 96). A ref with the event's counters,
  the ice host and the HUD line and without the card and the deck is
  identical to 7d in all four shapes, so the move is `determinize`
  sampling a new Runner card.

#### Stage 8a — a subtype chosen at rez, held while rezzed (28 September 2026)

`feat/rwr-stage-8a-lycian-multi-munition`: Lycian Multi-Munition. **One
new `Effect`.** Rebellion Without Rehearsal 63 of 65; `RWR_UNIMPLEMENTED`
3 → 2.

- **"Choose 1 or more subtypes among barrier, code gate, and sentry. This
  ice gains the chosen subtypes while it remains rezzed"** is a choice
  kept for a duration (CR 9.10.3), and its duration is "until the source
  becomes inactive" (9.10.3c). So it is a lingering effect and not a
  continuous one: a declared `GainSubtype` holds while its source is
  active, and has nowhere to keep which subtype was chosen.
  - `Lingering::GainSubtype(IceType)`, one per subtype chosen, until a
    new duration, `Until::WhileRezzed(install)`. It is made by the new
    `Effect::GainIceSubtype`, which `validate` allows only on ice and
    never of `IceType::Other`.
  - `continuous::ice_gains_subtype` reads it beside the table. That is the
    one question a typed break, Corsair's "the barrier you are
    encountering" and Lycian's own "if this ice is a code gate"
    (`Encountering(CodeGate)`, an existing requirement) all ask.
  - A rez starts a new rezzed period and drops what the last one left
    (`lingering::forget_rezzed_period`, in `engine::rez_install`). An
    entry that holds while its card is rezzed would hold again at the
    next rez, and nothing guarantees a checkpoint sweeps it in between.
- **Seven answers** (one, two or all three types) are more than the four a
  decision holds, so the choice is two decisions: one type, or "two or
  more" and then which.
- **"When a turn ends, derez this ice"** is either player's discard phase
  end: two triggers with `when: Whose(..)`, the words Méliès U uses for
  "the Runner's".
- **A pass says every type the ice has.** `GameEvent::IcePassed::rezzed_as`
  is now the list of types, printed and gained (`continuous::ice_types`),
  because Sisyphus Protocol's "passes a rezzed code gate or sentry" read
  the printed type alone and would not have heard a Lycian rezzed as a
  sentry.
- **Client**, both clients:
  - On the desktop, the ice's tile is lit as the type it gained, when it
    prints none of the three (`facts::ice_kind`).
  - Its sheet says "A barrier and sentry while it remains rezzed".
  - The in-effect list has one line per piece of ice
    (`hud::in_effect`).
  - No view field is new: the choice is in `ClientView::lingering`, which
    the view ledger already has as drawn.
- **Deck** — Retirement Package (Engineering the Future): two Lycian
  Multi-Munition for a Vertigo and a Reverb, ice for ice.
- **Real play**, 96 games of Retirement Package against Safety Net (seed
  2):
  - Random seats: rezzed 66 times, the choice made each time, derezzed
    at a turn's end 62 times; subroutines fired 202 times and broken
    twice.
  - Heuristic seats: rezzed 188 times, derezzed 175 times; subroutines
    fired 549 times and broken 15 times.
- **DSL ratio (`pool_status.py`): 24 of 89 `Effect` variants single-use,
  1 unused**, over 313 card files (23 of 88 before).
- **Measured.** Both sweeps at 256 seeds are green. Against 7e,
  `coverage_identical.py` has both random seatings **identical**. The
  heuristic ones moved: Corp agenda wins 78 → 72 of 192, Runner agenda
  wins 96 → 103, flatlines 16 → 17, deck-outs 2 → 0.
  - A ref with the lingering effect, the new effect, the pass's list of
    types and the clients, and without the card and the deck, is
    identical to 7e in all four shapes.
  - So the move is `determinize` sampling a new Corp card, not a rule.

#### Stage 8b — a subroutine gained for the rest of the run (28 September 2026)

`feat/rwr-stage-8b-thunderbolt-armaments`: Thunderbolt Armaments: Peace
Through Power, on the Deterrence Sweep deck. **No new `Effect`.**
Rebellion Without Rehearsal 64 of 65; `RWR_UNIMPLEMENTED` 2 → 1.

- **"That ice … gains “[subroutine] End the run unless the Runner trashes
  1 of their installed cards.” after its other subroutines for the
  remainder of that run"** is the case Stick and Poke's
  `Effect::GainSubroutine` said it was waiting for: "after" (CR 9.8.3e),
  and a subroutine gained for longer than an encounter. So the variant
  grows the two words, `GainSubroutine { subroutine, after, duration }`,
  rather than a second variant beside it.
  - One gained for the run is kept on the run
    (`RunState::gained_for_the_run`), not in `GameState::lingering`.
    The lingering list holds flat values a search clone copies in a few
    words, and a subroutine is an effect tree. It goes with the run, so
    nothing resets it.
  - The encounter's list is rebuilt at each encounter. So
    `run::engine::add_gained_for_the_run` adds the run's entries at both
    ways an encounter begins, the approach's `Continue` and a forced
    encounter:
    - "Before" subroutines go ahead of the ice's own, the newest first
      (9.8.3a).
    - "After" subroutines go behind them, the oldest first (9.8.3e).
    - Both are marked `gained`, so they leave with the encounter.
  - The ice need not be encountered when it gains it: it is rezzed on
    approach. If the ice is being encountered, the current encounter gets
    the subroutine as well.
  - `validate` accepts a gain for the run from a trigger that acts on its
    subject as the ice is rezzed or encountered, and refuses one for a
    turn.
- **The rest is words the engine had:**
  - "+1 strength … for the remainder of that run" is Brasília Government
    Grid's `ModifyStrength { ice: This, duration: Run }`.
  - "AP or destroyer ice" is `AnyOf` over `HasSubtype`.
  - "During a run" is the `DuringRun` requirement.
  - "Unless the Runner trashes 1 of their installed cards" is an
    `OfferPaidChoice` to the Runner of `Cost::Trash` over `OwnInstalled`.
- **Client:** `PublicRunState::gained_for_the_run` is new in the view,
  and the view ledger has it as drawn. Bot samples copy it
  (`determinize`).
  - During the encounter, it is in the encounter's list, as any gained
    subroutine is.
  - Outside an encounter, the ice's sheet lists it after the printed
    subroutines, "— for the rest of this run".
  - The in-effect list says "This run: Tithe has “…” after its other
    subroutines", in both clients.
- **Deck — Deterrence**, on Thunderbolt Armaments:
  - Retirement Package's frame after 8a, with two Tithe for two Vertigo
    and two Bumi 1.0 for two Reverb, so half its ice is AP or destroyer:
    Ansel 1.0 and 2.0, Sorocaban Blade, Lycian Multi-Munition, Bumi 1.0,
    Tithe.
  - It is Standard as well: Engineering the Future was the frame's one
    Core Set card.
- **Why two Runner Sweep decks changed:** a new Corp deck re-pairs every
  sweep seed.
  - The 256-seed view sweep's card gate then missed two single copies:
    Rotary (Borrowed Time) and Ashen Epilogue (Grassroots).
  - In 48 random games against Deterrence they were installed 7 times and
    played 5 times. At about 15 random games a Runner deck a sweep, a miss
    is sampling.
  - Each is now a two-of: Rotary for a Sell Out, Ashen Epilogue for a
    Friend of a Friend.
  - Ashen Epilogue for a Privileged Access moved the miss to Malandragem
    instead. Grassroots has no influence to double Malandragem, and no
    other Runner Sweep deck can take it: Borrowed Time would lose Startup,
    and the rest have no influence left.
  - This is a fragility of the gate, not a finding. Grassroots holds nine
    single copies, so the next Corp deck may move a miss again.
- **Real play**, 96 games of Deterrence against Safety Net (seed 2):
  - Random seats: the identity fired on 120 of the 143 rezzes of AP or
    destroyer ice; the other 23 were outside a run.
  - Heuristic seats: it fired on all 373.
- **DSL ratio (`pool_status.py`): 23 of 89 `Effect` variants single-use,
  1 unused**, over 314 card files (24 of 89 before): `GainSubroutine` has
  its second card.
- **Measured.** Both sweeps at 256 seeds are green. Against 8a,
  `coverage_identical.py` has **all four shapes identical**, heuristic
  included. An identity is not a card `determinize` samples, and
  Stick and Poke's rewritten effect plays as it did.

#### Stage 8c — a card added to the Runner's score area, and a third way to win (28 September 2026)

`feat/rwr-stage-8c-jeitinho`: Jeitinho, on the Hit List Sweep deck. **One
new `Effect`.** Rebellion Without Rehearsal 65 of 65; `RWR_UNIMPLEMENTED`
1 → 0. **Stage 8 is complete, and Rebellion Without Rehearsal with it.**

- **The Runner's score area is a list of `ScoredAgenda`**, as the Corp's
  is.
  - It was a bare `CardId` list, which had nowhere to say that a card was
    added "as an assassination agenda" (CR 10.1.3).
  - The field has the same type in the state and in the view.
  - `win::scored_value` takes a side and is the one worth for both score
    areas.
  - A steal records its turn.
- **"You may add this hardware to your score area as an assassination
  agenda worth 0 agenda points"** is Myōshu's
  `Effect::AddToScoreAreaAsAgenda`.
  - A Runner card now comes out of the rig into the Runner's score area.
    What it hosts is trashed with it, as Word on the Street's cost of the
    same name does.
  - `AsAgenda` gains `subtype`, and `GameEvent::AddedToScoreAreaAsAgenda`
    says whose score area and which subtype.
  - The turn's three successful runs are three `TimesThisTurnWhen`s over
    `OnSuccessfulRun`.
- **"Then, if you have 3 assassination agendas in your score area, you
  win the game"** is a third way to win, beside the two CR 1.7.2 lists.
  - The count is `Amount::InScoreAreaWithSubtype`. It has
    `InHeapWithSubtype`'s shape, because `Amount` is `Copy` and a
    `CardFilter` is not.
  - The win is the one new effect, `Effect::WinTheGame`, through
    `win::end_game`, the one door into `GameOver`.
  - It is announced first as `GameEvent::WonByCardText`, so
    `classify_end_reason` names the new `GameEndReason::CardText` rather
    than guessing from the scores.
  - It is immediate, as the card says, not a standing check.
- **"Threat 3 → Whenever you bypass a piece of ice, you may spend [click]
  to install this hardware from your heap"** can only affect the game
  from the heap, so it is active there (CR 9.1.8b).
  - `TriggeredEffect::from_heap` marks such a trigger.
  - A card in the Runner's heap listens for those triggers and no others
    (`Listener::in_heap`, `Heard::FromHeap`), and the same card on the
    table does not hear them. `validate` refuses one on a Corp card.
  - The install is the existing `InstallRunnerCardFromZone { from:
    OwnHeap }` on the acting card, and Threat 3 is `AmountAtLeast(
    ThreatLevel, 3)`.
- **Client**, both clients:
  - The score-area row says "Added as an assassination agenda · 0 points".
  - The log says "added Jeitinho to the Runner's score area as an
    assassination agenda …" and "the Runner wins the game by Jeitinho".
  - The desktop's end of game names the reason, and the terminal prints
    it (`CardText`).
  - The view ledger now holds the Runner's score entries field by field,
    as it holds the Corp's.
- **Deck — Hit List**, on Gabriel Santiago, the one Criminal identity no
  deck plays: Borrowed Time's frame with three Jeitinho for a Tailgate, a
  VRcation and a Pennyshaver. No Runner Sweep deck had 4 influence to
  spare, and Borrowed Time would have lost Startup.
- **The sweep schedule paired only part of the pool.**
  - `sweep_decks_for_seed` played `seed % C` against `seed % R`. Hit List
    made the lists 24 Corp and 18 Runner decks, which share a factor of
    6.
  - So 256 seeds were 72 pairings, each Runner deck against 4 Corp
    decks. The 256-seed card gate then missed Safety Net's Lobisomem,
    which is installed in about one game in four.
  - The Runner rotation now steps one deck each `lcm(C, R)` seeds
    (`coverage::pairing_cycle`), so 256 seeds are 256 pairings again.
  - For coprime lengths, as through 8b, nothing below `lcm` seeds
    changes. So 8b's two Sweep deck edits answered a full schedule, and
    their reason stands.
- **Real play**, 96 games of Hit List against Retirement Package (seed 2):
  - Random seats installed Jeitinho 69 times. They never made a
    successful run on all three centrals in one turn, and Hit List holds
    no way to bypass ice. So neither the score area nor the heap was ever
    reached.
  - The heuristic Runner never installs it.
  - Adding it to the score area, the win and the heap install are
    reached by the card's tests alone.
- **DSL ratio (`pool_status.py`): 24 of 90 `Effect` variants single-use,
  1 unused**, over 315 card files (23 of 89 before).
- **Measured.** Both sweeps at 256 seeds are green. Against 8b,
  `coverage_identical.py` has both random seatings **identical**, and the
  heuristic ones moved: Corp agenda wins 72 → 70 of 192, flatlines
  17 → 14, Runner agenda wins 103 → 107, deck-outs 0 → 1.
  - A ref with the score area, the heap listener, the win, the schedule
    and the clients, and without the card and the deck, is identical to
    8b in all four shapes.
  - So the move is `determinize` sampling a new Runner card, not a rule.

### 3. The Automata Initiative — 65 cards (C 14 / V 35 / M 16)

#### Stage 1 — nine Corp cards, and a standing "cannot" (28 September 2026)

`feat/tai-stage-1-attini-cannot-spend-credits`: Salvo Testing, Fujii Asset
Retrieval, Jaguarundi, Attini, Mindscaping, Pivot, Cybersand Harvester,
Behold! and Balanced Coverage. **No new `Effect`.** The Automata
Initiative 9 of 65; `TAI_UNIMPLEMENTED` 65 → 56.

- **Attini is the one card that did not compose.** "Threat 3 → The Runner
  cannot spend credits while subroutines on this ice are resolving" is a
  standing prohibition, and every "cannot" before it had a duration and
  was made by something that resolved (`Effect::Prohibit`, on the
  lingering list).
  - `ContinuousKind::Cannot(Prohibition)`, about the player it binds
    (`Scope::Player(Side)`; `validate` holds the two together and refuses
    `Player` on any other kind). `continuous::cannot` reads it beside the
    lingering list, through a question that walks both tables
    (`Target::Bound`), because the Corp's ice binds the Runner.
  - `Prohibition::SpendCredits`: every pool, not only the credit pool
    (Aircheck's `SpendOrLoseCreditPool`), and no word about a loss.
    `payment::sources` offers nothing while it holds, so the one door
    every payment goes through is the one place it is honoured.
  - `EffectRequirement::ResolvingThisIcesSubroutines`: the run is
    encountering this ice and a subroutine on it has resolved this
    encounter — from 6.9.3c's first resolution to the encounter's end,
    through every window a subroutine opens. CR 9.1.2b's own example is
    Attini: the Runner cannot pay for an interrupt during a subroutine's
    net damage either. Not in the break window before it, where Attini
    is broken with credits like any code gate.
  - Composing it as "do 1 net damage" at threat 3 was the alternative, and
    is right for the three "unless the Runner pays" and wrong for any
    credit-costed interrupt the pool gains.
- **The mandatory draw now waits for the turn-begins abilities to
  resolve** (CR 5.6.1d–e; Rules Conformance C1, fixed again).
  `turn::begin_turn` drew straight after dispatching "turn begins", so an
  ability that parked a decision had the draw and the window opened
  underneath it. Balanced Coverage would have looked at the card after
  the one just drawn, and AU Co.'s dig already did — its test said "the
  mandatory draw took Ice Wall; the top 3 are now…". Clearinghouse,
  Charlotte Caçador and Cohort Guidance Program park there too.
  - The rest of the step is owed while the phase is `StartOfTurn` with no
    window open, which is true only across such a park, and
    `engine::apply_action` pays it once nothing is parked or queued
    (`turn::finish_turn_beginning`). Read off the state, not a flag.
  - Four tests had the old order written into them; each now says the
    draw waited.
- **Balanced Coverage always asks.** "If that card has the chosen type,
  you may reveal it" as a selection of the matching top card parked only
  on a match, so the Runner would have learned the top of R&D from the
  Corp being asked, even when the Corp kept the agenda to itself. The
  look is followed by a choice the Corp is always offered ("you may reveal
  it and gain 2[credit]"), and the reveal checks the type
  (`ZoneHasAtLeast` with the top card's filter). Six answers (five types
  and none) are more than a decision holds, so asset, operation and
  upgrade are a second decision.
- **The rest compose.** Salvo Testing is The Powers That Be's "whenever
  you score" with a "may"; Fujii is Jinteki: Personal Evolution's two
  triggers about itself; Jaguarundi's "unless they spend [click]" is a
  nested `Cost::Clicks` (Funhouse's shape); Mindscaping's "up to 3" is two
  `EffectIf`s on the tag count; Pivot is Corporate Hospitality's
  additional click, a search, and Humanoid Resources' install or play from
  HQ behind the threat; Cybersand Harvester hosts credits for `Installing`
  and is trashed for what is left (Fermenter's `GainCreditsPerCounter`,
  read off the card its cost trashed); Behold! is Snare! with two tags.
- **Found on the way:** `board::diff`'s test named Buzzsaw to the Corp
  moving from the grip to the stack. Bring Them Home had revealed it and
  put it back in the one action, so it was in neither view; the entry's
  own `CardRevealed` now counts as seen. No leak — the edited decks had
  re-rolled seed 0 onto the path.
- **Client**, both clients: `ClientView::standing_cannot`, the engine's
  verdict with the card that says it (drawn: `hud::in_effect`, "Attini:
  the Runner cannot spend credits", both clients), so the reason an offer
  to pay has no Accept is on the screen without a client reading the
  threat level. `ClientView::cannot` reads it too. Behold! is a trap to
  `board::rez::gains_nothing`, unasked. No ledger row opened.
- **Decks** — swaps into Eternal-only Sweep decks, point for point:
  Retirement Package (Salvo Testing, Jaguarundi), A Thousand Cuts (Fujii,
  Mindscaping, Attini), Paid Content (Balanced Coverage, Behold!), Hostile
  Bid (Cybersand Harvester, Pivot). No new deck, so the sweep schedule is
  unchanged.
- **DSL ratio (`pool_status.py`): 23 of 90 `Effect` variants single-use,
  1 unused**, over 324 card files (24 of 90 before).
- **Real play**, 96 games of each edited Corp deck against Safety Net
  (seed 2):
  - Random seats use all nine: Salvo Testing scored twice (its trigger
    twice), Jaguarundi's threat-4 encounter asked 21 times, Fujii stolen
    44 times (its damage 26), Mindscaping played 43 times, Attini rezzed
    4 times with 17 subroutines fired, Balanced Coverage heard 246 turn
    starts, Behold! paid for twice, Cybersand Harvester trashed for its
    credits 43 times, Pivot played 33 times.
  - Heuristic seats: Salvo Testing scored 48 times, Attini rezzed 15
    times, Cybersand Harvester banked on 126 rezzes. The heuristic Corp
    never plays Mindscaping or Pivot and never pays for Behold! (Phase 5
    debt).
  - Whether Attini's prohibition held in play is not in the report: a
    threat of 3 is rare in these games, so it is reached by the card's
    test.
- **Measured.** Both sweeps at 256 seeds are green. Against `origin/main`,
  `coverage_identical.py` has all four seatings moved, and the move is
  attributed in three pinned steps:
  - The standing prohibition, the requirement, the view field and the
    clients, without the cards, the decks or the turn fix: **identical**
    to main in all four shapes.
  - Adding the cards and the deck swaps: both random seatings
    **identical**; the heuristic ones moved (Corp flatlines 14 → 18,
    Runner agenda wins 107 → 104, deck-outs 1 → 0, of 192) — `determinize`
    sampling the new Corp cards.
  - Adding the turn fix: every seating moved, the random ones by where the
    draw now falls — AU Co.'s turn-start dig fired 12 → 8 times and asked
    18 → 12 times by view, steps 74352 → 73018 — and the heuristic Corp
    agenda wins 70 → 71, Runner agenda wins 104 → 103. That is the rule
    change, and the one rule change in the stage.

#### Stage 2 — ten cards, and two small words (28 September 2026)

`feat/tai-stage-2-valentao-credits-armed-asset-faceup`: Joy Ride,
Shibboleth, LilyPAD, Solidarity Badge, Audrey v2, Your Digital Life, Armed
Asset Protection, Valentão, B-1001 and M.I.C. **No new `Effect`.** The
Automata Initiative 19 of 65; `TAI_UNIMPLEMENTED` 56 → 46. Five of the ten
are Corp cards: the plan's "Runner, composes" named the stage by its first
half.

- **Two words, each a sentence no existing word could say.**
  - `Amount::Credits(Side)`: Valentão's "End the run if you have more
    credits than the Runner" is `MoreThan(Credits(Corp), Credits(Runner))`,
    as Piranhas compared two hands. The credit pool, not what a player could
    spend (CR 1.10.4); `RunnerCreditsAtMost` and `CorpCreditsAtLeast`
    compare with a number the card file writes.
  - `CardFilter::Faceup`: Armed Asset Protection's "if any of those cards
    are agendas", of the faceup cards it has just counted. `Not(Facedown)`
    was the obvious spelling and matches nothing, because `Not` is read off
    a definition and which way up a card lies is not in one.
- **An operation is not in its own count of Archives.** The engine files an
  operation in Archives as it is played, and a selection already skipped
  the copy resolving (RWR Stage 1, CR 8.6.7a); the count of card types among
  faceup cards in Archives did not, so Armed Asset Protection would always
  have counted an operation — its own. The count now leaves it out the same
  way (`pending_choice::resolving_operation_in`). Logjam, the one other card
  that counts, is ice and unaffected (Rules Conformance 8.6).
- **The rest compose.**
  - Joy Ride is Jailbreak's run with R&D alone and "draw 5".
  - Shibboleth is Gordian Blade with Manuel Lattes de Moura's threat
    `while` on a −2 `Strength`.
  - LilyPAD is Pantograph's memory and "the first time each turn you
    install a program" as `OnCardInstalled` with `first_each_turn`.
  - Solidarity Badge's "the first time each turn you trash a Corp card" is
    two entries sharing one count — a trash on access, and a trash of a
    Corp card the Runner carried out (`OwnedBy`, Active Policing's word) —
    and its turn-begins "you may remove 1 hosted power counter to draw 1
    card or remove 1 tag" is Cacophony's paid choice around a choice of two.
  - Audrey v2 is Botulus's counter-cost break without a host (an AI: any
    type, strength contested) and Carnivore's grip trash as the price of
    +3 strength.
  - Your Digital Life is `GainCreditsAmount` over `CardsInHand(Corp)`.
  - Valentão is Piranhas's rez alternatives.
  - B-1001 is The Red Room's "during a run against another server" with
    Synapse Global's "remove 1 tag" as the Corp's cost.
  - M.I.C. is Event Horizon's "[trash]: … Use this ability only during a
    run on this server" with Jaguarundi's "unless the Runner spends
    [click]" as what the trash buys.
- **Client**: nothing added to the view, so no ledger line and no drawing.
  The prose names the new amount ("the Corp's credits").
- **Decks** — swaps into Eternal-only Sweep decks, count for count, each
  card taken out still in another deck the sweeps play: Safety Net (Joy
  Ride for Overclock and Diesel), Spare Parts (LilyPAD for DZMZ Optimizer
  and T400 Memory Diamond), Hit List (Shibboleth for VRcation), Grassroots
  (Solidarity Badge for Rent Rioters, Audrey v2 for Buzzsaw), Retirement
  Package (M.I.C. for two of three Sleipnir), Paid Content (Your Digital
  Life for Scapegoat, B-1001 for Public Trail and Tithe), Hostile Bid
  (Valentão for Ballista), Land Grab (Armed Asset Protection for Hedge
  Fund). No new deck, so the sweep schedule is unchanged.
- **DSL ratio (`pool_status.py`): 23 of 90 `Effect` variants single-use,
  1 unused**, over 334 card files (unchanged).
- **Real play**, 96 games of each edited deck (seed 2; the Corp decks
  against Safety Net, the Runner decks against Hostile Bid):
  - Random seats use all ten: Joy Ride played 96 times across four
    Corp decks, Shibboleth installed 60 times, LilyPAD's draw heard 16
    times, Solidarity Badge installed 48 times (its counter 22 times, its
    turn start 38), Audrey v2 installed 29 times with 10 counters from
    trashes, M.I.C. trashed for its ability 20 times, Your Digital Life
    played 37 times, Armed Asset Protection 46, Valentão rezzed 28 times
    over four matchups, B-1001 used 26 times.
  - Heuristic seats: Audrey v2 and Shibboleth are installed and used (98
    and 249 activations), Armed Asset Protection played 86 times, Your
    Digital Life 64, B-1001 used 10 times, M.I.C. and Valentão rezzed.
    The heuristic Runner never installs Solidarity Badge or plays Joy
    Ride, and the heuristic Corp never trashes M.I.C. for its ability
    (Phase 5 debt).
- **Measured.** Both sweeps at 256 seeds are green, the card gate included.
  Against `origin/main`, `coverage_identical.py` has both random seatings
  **identical** and both heuristic ones moved (Corp agenda wins 71 → 74,
  Corp flatlines 18 → 16, Runner agenda wins 103 → 102, steps 115764 →
  120976, of 192). A pinned ref with the two words and the count fix but
  none of the cards, decks or tests is **identical** to main in all four
  shapes, so the heuristic movement is `determinize` sampling the new
  cards.

#### Stage 3 — the pass, and the ice's subtypes (28 September 2026)

`feat/tai-stage-3-pass-triggers-banner`: Phoneutria, Tatu-Bola, Virtual
Service Agent, Curupira, Laser Pointer and Banner. **No new `Effect`.** The
Automata Initiative 25 of 65; `TAI_UNIMPLEMENTED` 46 → 40.

- **Which kind of breaker broke the printed subroutines is carried out on
  the pass.** Virtual Service Agent's "whenever the Runner passes this ice
  after encountering it, if they did not break its printed subroutine with a
  decoder during that encounter" is asked as the pass's trigger resolves,
  and the encounter's tally is reset as the movement phase begins, before
  that.
  - `EncounterTally::printed_broken_with`, a set of the four icebreaker
    subtypes (CR 2.16.7i) as bits, because the tally is `Copy`. Written by
    `run::break_subroutine` from the card whose ability broke a subroutine
    the ice prints; a click, or a card's text with no breaker, adds nothing.
  - The pass carries it (`GameEvent::IcePassed::printed_broken_with`), and
    `EffectRequirement::BrokePrintedSubroutineWith(subtype)` reads it off the
    triggering event, as `HadNoTags` reads `TagsGiven`. `validate` holds it
    to an `OnIcePassed` trigger and an icebreaker subtype.
  - "After encountering it" needs no word: the pass is a moment about the
    ice only while it is rezzed, and rezzed ice is encountered before it is
    passed.
- **A piece of ice's subroutines can be unable to end the run.** Banner's
  "Interface → 2[credit]: Subroutines on the barrier you are encountering
  cannot end the run for the remainder of this encounter" is `Effect::Prohibit`
  with `Prohibition::EndTheRun`, about the encountered ice
  (`Prohibit::encountered_ice`, `lingering::On::Install`) for the encounter.
  - `Effect::EndTheRun` asks it while that ice's subroutines are resolving,
    and does nothing (CR 1.2.2); the rest of the subroutines resolve (1.2.4).
    That stretch is `run::resolving_subroutines_of`, which Attini's
    `ResolvingThisIcesSubroutines` now reads too, so the two cannot disagree.
  - A cannot, not Shred's prevention: nobody uses it and it has no "first
    time". `validate` admits `Encounter` for this prohibition alone, and
    only about the encountered ice.
  - An interface ability that breaks nothing is held to CR 3.9.5g by the
    card file: `Not(MoreThan(EncounteredIceStrength, ThisCardStrength))`,
    over one new `Amount`, the acting rig card's strength. Banner is refused
    against Brân 1.0, 6 to its 5.
- **"Swap it" is said by the ice passed.** Tatu-Bola's "When the Runner
  passes this ice, you may swap it with a piece of ice from HQ. If you do,
  gain 4[credit]" is Mitra Aman's swap with `this_ice`: the install the
  selection's `then` acts as, rather than the ice being approached, which a
  pass has none of. It is offered only with ice in HQ, and the gain is in the
  same `then`, so it is paid only for a swap.
- **A swapped-in piece of ice is no longer named to the Runner by the card it
  replaced.** The swap kept the install's `seen_by_runner`, so a rezzed ice
  swapped for one from HQ would have shown the Runner the new card, facedown.
  Tatu-Bola is always rezzed as it swaps; Mitra Aman's approached ice could
  have been seen too. The swap now clears it and the copy's turn with the
  card (Rules Conformance 6.2).
- **The rest compose.** Phoneutria is Vertigo's pass with
  `AmountAtLeast(CardsInHand(Runner), 4)`; Curupira is Malandragem's paid
  bypass for 3 counters on a barrier and Lobisomem's "whenever it fully
  breaks" (`ByThis`); Laser Pointer is Malandragem's threat-4 bypass with
  its own trash as the cost and "AP, destroyer, or observer" as a `when` over
  the catalog's subtypes.
- **Client**, both clients: nothing added to the view — the new fields are on
  the run's tally and an event, and the prohibition is a lingering effect the
  view already carries. The "in effect" lines name it ("Banner: subroutines on
  Ice Wall cannot end the run, for this encounter"), and the prose names the
  new words.
- **Decks** — swaps into Eternal-only Sweep decks, count for count, each card
  taken out still in another deck the sweeps play: A Thousand Cuts
  (Phoneutria for Neurospike, Tatu-Bola for Byte), Paid Content (Virtual
  Service Agent for Paywall), Hit List (Curupira for Marjanah, Laser Pointer
  for Leech), Pay As You Go (Banner for two of three Corroder). No new deck,
  so the sweep schedule is unchanged.
- **DSL ratio (`pool_status.py`): 22 of 90 `Effect` variants single-use,
  1 unused**, over 340 card files (23 of 90 before: Tatu-Bola is the swap's
  second card).
- **Real play**, 96 games a seating (seed 2; the Corp decks against Safety
  Net, the Runner decks against Hostile Bid, Hit List against A Thousand Cuts
  too):
  - Random seats use all six: Phoneutria rezzed 28 times and its pass tagged
    once, Tatu-Bola passed and swapped 3 times, Virtual Service Agent's pass
    tagged 62 times, Curupira installed 66 times and its fully-broken counter
    placed 7 times, Laser Pointer trashed to bypass 9 times, Banner used 4
    times.
  - Heuristic seats: the Corp swapped Tatu-Bola on each of the 46 passes
    it heard, Virtual Service Agent is broken 132
    times and tags 23, and the Runner installs Curupira and bypasses with it
    (49 bypasses, 201 counters against Hostile Bid). The heuristic Runner
    never installs Laser Pointer or Banner (Phase 5 debt).
- **Measured.** Both sweeps at 256 seeds are green, the card gate included.
  Against `origin/main`, `coverage_identical.py` has both random seatings
  **identical** and both heuristic ones moved (Corp agenda wins 74 → 62,
  Corp flatlines 16 → 18, Runner agenda wins 102 → 110, deck-outs 0 → 2,
  steps 120976 → 116542, of 192). A pinned ref with every engine and client
  change and none of the cards, decks or tests is **identical** to main in
  all four shapes, the swap's cleared `seen_by_runner` included, so the
  heuristic movement is `determinize` sampling the new cards.

#### Stage 4 — Trojans, host ice, and back to the grip (28 September 2026)

`feat/tai-stage-4-trojans-host-scopes`: Monkeywrench, Saci, Slap Vandal,
Umbrella, Living Mural, Pichação, Urban Art Vernissage, Hermes and Stegodon
MK IV. **One new `Effect`** (`AddToHand`). The Automata Initiative 34 of 65;
`TAI_UNIMPLEMENTED` 40 → 31.

- **A derez is a moment, and "host ice" a filter.** Saci's "Whenever host
  ice is rezzed or derezzed" needed both.
  - `Trigger::OnDerez`, about the card turned facedown: `GameEvent::
    CardDerezzed` was an occurrence of nothing.
  - Every derez now goes through one function (`ability::derez`): a
    card's text (`Effect::DerezCard`, dispatched with `emit`) or a cost
    (`Cost::Derez`, `DerezSelf`, dispatched by the payer).
  - `EventFilter::Host`: the moment is about the listening install's
    host, asked as it is heard. Pichação's "Whenever you pass host ice"
    uses it too. `validate` admits it only on a card that installs on
    ice.
- **Once per encounter** (Slap Vandal): `EffectRequirement::
  OncePerEncounter`, the encounter's twin of `OncePerRun`.
  - It is kept on the encounter's tally
    (`EncounterTally::once_per_encounter_used`), so the tally's reset is
    the limit's end. The tally is no longer `Copy`.
- **Ice hosting a trojan** (Umbrella's "can only interface with ice
  hosting a trojan program"): `EffectRequirement::EncounteredIceHosts
  (filter)`.
  - Its "If at least 1 subroutine was broken this way" needs no word: the
    break refuses when there is nothing to break, so the ability never
    resolves without one.
  - "Each player may draw 1 card" is two `PresentChoice`s, the Runner's
    first.
- **A run in which ice was derezzed, and every installed icebreaker**
  (Stegodon MK IV's "Each run, as long as a piece of ice has been derezzed
  during that run, each installed icebreaker gets –2 strength").
  - `RunState::ice_derezzed`, set by `ability::derez` and read by
    `EffectRequirement::IceDerezzedThisRun`. It is public: carried in the
    view, copied by `determinize`, and drawn as an "in effect" line.
  - `Scope::Rig(filter)`, a continuous effect about the rig's cards.
  - Its "Once per turn → When a run begins, you may derez…" is Brasília
    Government Grid's `Cost::Derez` in an `OfferPaidChoice`. The trigger
    does not fire while no rezzed ice outside the attacked server could
    be derezzed. Declined, the turn is still spent (the 9.3.6g deviation,
    recorded).
- **"Not protecting the attacked server" matched nothing.** A `Not` around
  a word about the installed copy was decided at the card level, where
  that word always passes, so the negation always failed.
  - `CardFilter::is_about_the_copy_alone` names those words. A `Not` of
    one now passes the card-level half and is decided on the copy
    (`pending_choice`).
  - No card before Stegodon negated one.
- **Clicks gained during runs are counted, and a program goes back to the
  grip** (Pichação's "If this is not the first time you gained [click]
  during a run this turn, add this program to your grip").
  - `TurnLog::click_gains_in_runs`, a sum beside `installed_from_hq`: a
    click gained is a moment no card hears. It is recorded by
    `Effect::GainClicks` during a run, and read as `Amount::
    ClickGainsInRunsThisTurn`.
  - `Effect::AddToHand`, the acting install to its owner's grip. It is
    single-use: `PromptChooseCards` cannot say "this install", and
    `AddToDeck` moves only into a deck.
- **An install returned to a hand says so, named only to who saw it.**
  - Hermes's "add 1 unrezzed card to HQ" and Urban Art Vernissage's "add 1
    installed non-virus trojan program to your grip" are selections with
    a hand as the destination.
  - Such a move now announces `CardAddedToHand` (JK's event, masked when
    facedown).
  - A Runner's selection of the Corp's installs sent to HQ or R&D leaves
    the facedown ones out of `CardsSelected`. The mask names a selected
    card only while it is still facedown on the table, and after the move
    it is not, so Hermes would have named the card it returned.
  - Vernissage's "place 2[credit]" follows the selection in a `Sequence`,
    since a selection's `then` acts as the chosen install. It pays for
    installs (`PaysFor::Installing(Any)`).
- **A trojan's "this server" is its host's** for `RunAgainstThisServer`,
  as it already was for the listeners. Living Mural's "a sentry protecting
  this server" is that with `Encountering(Sentry)`. A boost "for the
  remainder of the turn" needs no encounter (Living Mural's threat-4 +3 on
  install).
- **Monkeywrench composes.** Its "Host ice gets −2 strength. Each other
  piece of ice protecting this server gets −1" is −1 on its host and −1 on
  its server's ice, host included. A Trojan may now say
  `IceProtectingThisServer`.
- **Found by the sweep: a host's host lost what it hosted.**
  - The view sweep's conservation check failed at seed 33 (Fine Print
    against Spare Parts, whose list this stage changed): a Muse trashed to
    the memory limit took the Cupellation it hosted to the heap, and the
    Corp card on the Cupellation to no zone.
  - The rig's cascade now follows each card it trashes down to what that
    card hosted (CR 1.13.13, "and all objects hosted on those objects").
  - It is a bug on main, reached only on the new trajectory.
- **Client**, both clients:
  - `RunState::ice_derezzed` is drawn as "This run: a piece of ice has
    been derezzed", which is why every icebreaker lost 2.
  - The tally's once-per-encounter set and the turn log's new sum are the
    engine's (the view ledger says so).
  - The log names what went back to a hand, and the prose names the new
    words.
- **Decks** — swaps into Eternal-only Sweep decks, each card taken out
  still in another deck the sweeps play:
  - Retirement Package: two Stegodon MK IV for the Offworld Office and the
    Vulture Fund, the points kept at 21.
  - Hit List: Hermes for the T400 Memory Diamond, and two Saci for two
    Rotary.
  - Grassroots: two Monkeywrench for two Leech.
  - Safety Net: two Living Mural for two Echelon, two Umbrella for two
    Unity, and two Urban Art Vernissage for two Touchstone.
  - Spare Parts: two Slap Vandal for a Gordian Blade and an Echelon, and
    two Pichação for two Jailbreak. Taking both Gordian Blades left Safety
    Net's one copy unseen by the 256-seed card gate.
- **DSL ratio (`pool_status.py`): 23 of 91 `Effect` variants single-use,
  1 unused**, over 349 card files (22 of 90 before).
- **Real play**, 96 games a seating (seed 2). The Runner decks played
  Hostile Bid; Retirement Package played Safety Net.
  - Random seats use all nine:
    - Monkeywrench installed 39 times.
    - Hermes installed 21 times, returning 61 cards to HQ.
    - Saci installed 46 times, heard 3 rezzes.
    - Living Mural installed 46 times.
    - Umbrella installed 41 times.
    - Urban Art Vernissage installed 30 times, took back 15 trojans.
    - Slap Vandal installed 55 times.
    - Pichação installed 43 times, gained 28 clicks and went back to the
      grip 5 times.
    - Stegodon MK IV scored 5 times, its run-start trigger heard 25.
  - Heuristic seats:
    - The Corp scores Stegodon MK IV 57 times, its trigger heard 147.
    - Hermes returned 52 cards to HQ.
    - Slap Vandal was used 216 times.
    - Living Mural was used 90 and 193 times.
    - Umbrella was used 7 times.
  - The heuristic Runner never installs Monkeywrench, Saci, Pichação or
    Urban Art Vernissage (Phase 5 debt, beside Laser Pointer and Banner).
  - Saci's derez was reached by its test alone: Hit List's opponents
    derez nothing.
- **Measured.** Both sweeps at 256 seeds are green, the card gate included.
  - Against `origin/main`, `coverage_identical.py` has both random
    seatings **identical**. Both heuristic ones moved, of 192 games:

    | | main | Stage 4 |
    |---|---|---|
    | Corp agenda wins | 62 | 64 |
    | Corp flatlines | 18 | 23 |
    | Runner agenda wins | 110 | 105 |
    | Deck-outs | 2 | 0 |
    | Steps | 116542 | 112281 |

  - A pinned ref with every engine and client change and none of the
    cards, decks or tests is **identical** to main in all four shapes, the
    cascade fix included.
  - So the heuristic movement is `determinize` sampling the new cards.

#### Stage 5 — Corp install and server words (28 September 2026)

`feat/tai-stage-5-corp-install-and-server-words`: Vovô Ozetti, Greasing the
Palm, Ablative Barrier, Tucana, Front Company, Federal Fundraising,
Epiphany Analytica: Nations Undivided and Lago Paranoá Shelter. **No new
`Effect`.** The Automata Initiative 42 of 65; `TAI_UNIMPLEMENTED` 31 → 23.

- **A card can say when it may be rezzed** (Front Company's "Rez only
  during your turn").
  - `CardDefinition::rez_requirement`, asked as the card by
    `engine::rez_install`, the one place a Corp card is turned faceup. So a
    card's text that would rez it on the Runner's turn rezzes nothing, as
    an unaffordable rez does (`RulesError::RezRestricted`, swallowed where
    `NotEnoughCredits` is), and the rez action is refused.
  - A field beside `play_requirement` and `install_only_in`, the card's
    other restrictions on its own way into play. Not a continuous effect:
    those are what an active card does, and a card being rezzed is not
    active yet.
- **"The first run each turn cannot be made against a remote server"**
  (Front Company).
  - `Prohibition::RunOnRemote`, a standing `Cannot` on the Runner with
    `ContinuousEffect::first_each_turn`. The first of what is the
    prohibition's own word (`Prohibition::counted_as`, the turn's runs), so
    the card writes no count.
  - Asked where the server is announced (CR 6.3.2a): `run::start_run`,
    which every run goes through, and the servers a card's text offers to
    run, which leave the remotes out. The basic action is never offered.
  - Drawn: "Front Company: the Runner cannot run on a remote server" in
    the In effect list until the turn's first run.
  - Its other sentence composes: "the first time each turn a run on
    Archives begins" is `OnRunStart` with `when: Server([Archives])` and
    `first_each_turn`; "if this server is not protected by ice" is
    `Amount::IceProtectingThisServer` under a `Not`, which Federal
    Fundraising uses too.
- **"The Corp installs a card in the root of a server"** (Lago Paranoá
  Shelter): `EventFilter::InRoot`.
  - Read off the card installed: ice is the only type installed protecting
    a server and never in a root (CR 3.4.2).
  - It says whose moment it is, as `Whose` does, since a `Card` filter on
    an "install" trigger hears its controller's installs.
  - The turn log now counts a Corp install of ice as ice, which the table
    shows (`turn_log::seen_anyway`), so "the first" of the Corp's root
    installs is the first install it counts unseen.
  - "You may trash the top card of your stack to draw 1 card" is a
    `Cost::Trash` from `TopOfOwnStack` in an `OfferPaidChoice`.
- **A persisting ability hears the whole run** (Tucana's "Persistent →
  Whenever an agenda is scored or stolen from the root of this server").
  - A persistent upgrade the Runner trashed while accessing it was heard
    only at the run's end (AMAZE Amusements). It now listens for the rest
    of the run as an active card does: it "never becomes inactive" (CR
    9.12.5b). No card but Tucana hears anything new: AMAZE Amusements'
    one trigger is the run's end, and Flagship and Mahkota Langit Grid
    have none.
  - Its "this server" is the attacked one
    (`EffectRequirement::AgendaCameFromThisCardsServer` falls back to the
    run's server for it), as in 1.12.6a's AMAZE Amusements example.
  - The search is Eminent Domain's: a selection of ice out of R&D,
    shuffled, then an install and rez paying a total of 3[credit] less.
  - `install_only_in: [Remote]` says "Remote server only".
- **A rez discount on the ice protecting a server** (Vovô Ozetti):
  `RezCost` now admits `Scope::IceProtectingThisServer`. Its threat-4
  root discount is `RootOfThisServer` with a `while`, and "When your turn
  ends, you may move this upgrade" is Lotus Haze's move, offered by the
  upgrade itself.
- **The rest composes.**
  - Greasing the Palm: an install from HQ whose `if_installed` offers
    `Cost::RemoveTags(1)` for an advancement counter on that card, asked
    only of a tagged Runner.
  - Ablative Barrier: Ping's "when you rez this ice during a run against
    this server" at threat 3, then a choice of HQ or Archives, each an
    install with `another_server`.
  - Federal Fundraising: Knowledge Seeker's arrangement of the top of R&D,
    then a draw under the "not protected by ice" condition.
  - Epiphany Analytica: two triggers sharing one first-time count ("steals
    or trashes a Corp card", the second `OwnedBy { Corp, Runner }`), and
    Poétrï's install from the top 3 of R&D behind a look.
- **Decks.**
  - Grand Opening, a new Corp Sweep deck on Epiphany Analytica (Standard,
    Eternal and Casual). Paid Content's frame, less its out-of-faction
    cards and three NBN ones, so Federal Fundraising, Vovô Ozetti, Greasing
    the Palm, Ablative Barrier and Tucana fit the fifteen influence.
  - A Thousand Cuts: two Front Company for two Urtica Cipher.
  - Grassroots: two Lago Paranoá Shelter for two Friend of a Friend.
  - Every card taken out is still in another deck the sweeps play.
- **DSL ratio (`pool_status.py`): 23 of 91 `Effect` variants single-use,
  1 unused**, over 357 card files — unmoved.
- **Real play**, 96 games a seating (seed 2), Grand Opening and A Thousand
  Cuts against Grassroots.
  - Random seats use all eight:
    - Epiphany Analytica took a counter 215 times and spent one 134 times.
    - Federal Fundraising asked 354 times as the turn began.
    - Vovô Ozetti installed 51 times, offered its move 252 times.
    - Greasing the Palm played 24 times.
    - Ablative Barrier rezzed 29 times, 9 of them at threat 3 during a run
      on its server.
    - Tucana installed 38 times; it heard a steal once and a score never.
    - Front Company rezzed 31 times, its Archives damage 20 times.
    - Lago Paranoá Shelter installed 36 times, offered 135 times.
  - Heuristic seats:
    - Federal Fundraising asked 1259 times, Vovô Ozetti 530.
    - Front Company rezzed 54 times, its Archives damage 24 times.
    - Greasing the Palm played 9 times, Ablative Barrier's rez trigger 9.
  - The heuristic Corp never spends an Epiphany Analytica counter, and the
    heuristic Runner never installs Lago Paranoá Shelter (Phase 5 debt).
  - Tucana's score trigger is reached by its test alone.
- **Measured.** Both sweeps at 256 seeds are green, the card gate included.
  - Against `origin/main`, `coverage_identical.py` has both random
    seatings **identical**. Both heuristic ones moved, of 192 games (view
    and index alike, but for main's flatline and deck-out counts, 22 and 1
    by index):

    | | main | Stage 5 |
    |---|---|---|
    | Corp agenda wins | 64 | 60 |
    | Corp flatlines | 23 | 10 |
    | Runner agenda wins | 105 | 120 |
    | Deck-outs | 0 | 2 |
    | Steps | 112281 | 111335 |

  - A pinned ref with every engine and client change and none of the
    cards or decks is **identical** to main in all four shapes, the turn
    log's count of the Corp's ice and the persisting listeners included.
  - So the heuristic movement is `determinize` sampling the new cards.

#### Stage 6 — Runner run words (28 September 2026)

`feat/tai-stage-6-runner-run-cards`: Hannah "Wheels" Pilintra, Mercury:
Chrome Libertador, Debbie "Downtown" Moreira, Bahia Bands, Chrysopoeian
Skimming, Strike Fund and The Price. **No new `Effect`**: one was
generalised in place. The Automata Initiative 49 of 65;
`TAI_UNIMPLEMENTED` 23 → 16.

- **A trash says which pile the card left** (Strike Fund's "When this
  event is trashed from your grip or stack").
  - `GameEvent::CardTrashed::from` (`dsl::TrashedFrom`: installed, a hand,
    a deck, elsewhere) replaced the flag that said only whether the card
    was installed. Boi-tatá still reads that flag (`TrashedFrom::installed`).
    `EventFilter::TrashedFrom` reads the pile off the moment, since the
    card is in the heap by the time anything hears it.
  - A trigger about the card itself (`Subject::This`) is heard whoever
    trashed it (`listeners::whose_admits`). No other card's `This` trigger
    is about something its controller's opponent does.
  - **Damage's cards are trashed, not discarded** (CR 10.4.2a, 1.19.3): a
    `CardTrashed` out of the grip by the player responsible, dispatched
    with the damage. They were `CardDiscarded`, an occurrence of nothing,
    so no card could hear what damage took. Hiram "0mission" Svensson now
    hears hardware that damage the Runner suffers by their own card takes.
    Both clients' damage line reads "… trashed" instead of "… discarded".
- **"Those cards"** (The Price's "Trash the top 4 cards of your stack.
  You may install 1 of those cards, paying 3[credit] less").
  - `MillRnDAmount`, R&D's alone, became `Effect::Mill { deck, amount,
    then }`. Its `then` has `CardFilter::TrashedThisWay` written over as
    the cards it trashed (`CardFilter::AmongCards`) before it resolves, or
    before it waits behind whatever a trashed card parked. That is the
    State Hygiene Rule's third case, a value written into the effect that
    waits.
  - Composition was tried first and failed. `TopOfZone(4)` on the heap
    found The Price itself on top: a played event lands there in the
    middle of its own resolution. A heap has no order to read the cards
    back from (CR 4.4.2) either.
  - A Strike Fund trashed this way asks its "may" before The Price's
    install (CR 9.6.5a). The engine trashes the four one at a time, so one
    third from the top resolves before the fourth is trashed; nothing in
    the pool can tell (rules conformance 9.6).
- **Two facts about a run.**
  - Whether a subroutine was broken during it (`RunState::
    subroutine_broken`, `EffectRequirement::SubroutineBrokenThisRun`):
    Mercury's "if you did not break any subroutines during that run".
    Mercury's one printed ability is one trigger on HQ or R&D, since two
    once-per-turn entries would share a use. Its access goes to the server
    being run (`DuringRunOn`). As with Stegodon MK IV, a declined "may"
    spends the turn's use (9.3.6g, recorded).
  - Whether it was unsuccessful (CR 6.8.4): `RunState::
    reached_success_phase`, set as the Runner completes the run whatever
    is declared, and `CompletedRun::unsuccessful`, which `run::end_run`
    sets unless the remote ceased to exist (6.8.4b). Hannah's "When that
    run ends, if it was unsuccessful, take 1 tag" is her run's end rider
    under `EffectRequirement::LastRunUnsuccessful`.
  - Both facts are public and in the view, so a bot's sample asks them
    where the real game does.
- **Two hosted-credit words.**
  - `PaysFor::DuringItsRun`: Debbie's "[click]: Run any server. You can
    spend hosted credits during that run". `DuringRuns` is every run; hers
    is the run whose `initiated_by` she is.
  - Bahia Bands' "Place 4[credit] on this event. You can spend hosted
    credits to pay trash costs for the remainder of this run".
    `PlaceRunCredits` took a `pays_for` (`RunState::run_credits_pay_for`),
    and the run's pool is classed by it.
  - Drawn: "This run: the 4 [credit] on Bahia Bands may be spent only to
    pay trash costs" in the In effect list, since "+4" beside the credit
    pool cannot say it.
- **The rest composes.**
  - Chrysopoeian Skimming: the Corp is always asked, so the question shows
    nothing about HQ. "Reveal" with no agenda there is the "otherwise"
    (look at the top 3 of R&D).
  - Debbie's threat 4 load and run-event credit are triggers on herself.
  - Hannah's first ability: a click gained, then a remote-only
    `PromptChooseServer`, withheld with no remote to run. Her second:
    `[click], [trash]`.
  - Bahia Bands: Key Performance Indicators' `ResolveSomeOf` in the run's
    `on_success`.
- **Deck.** Picket Line, a new Runner Sweep deck on Mercury (Standard,
  Eternal and Casual), on Stolen Goods' frame of run events, with all
  seven cards.
- **DSL ratio (`pool_status.py`): 21 of 91 `Effect` variants single-use,
  1 unused**, over 364 card files, from 23 of 91: `Mill` has two cards and
  `PlaceRunCredits` two.
- **Real play**, 96 games a seating (seed 2), Grand Opening against Picket
  Line.
  - Random seats use all seven:
    - Strike Fund played 27 times, heard its own trash 6.
    - The Price played 26 times, its install offered 25.
    - Hannah installed 16 times, used 18.
    - Mercury's trigger fired 381 times.
    - Chrysopoeian Skimming played 42 times, the Corp revealing 11.
    - Debbie installed 27 times, used 65, loaded at threat 4 9 times and
      by a run event 25.
    - Bahia Bands played 33 times.
  - Heuristic seats: Strike Fund played 34 times, Mercury's trigger fired
    493. The heuristic Runner never plays Bahia Bands, Chrysopoeian
    Skimming or The Price, and never installs Debbie or Hannah (Phase 5
    debt).
- **Measured.** Both sweeps at 256 seeds are green, the card gate included.
  - Against `origin/main`, `coverage_identical.py` reports every shape
    different. The random seatings play the same games: same outcomes and
    the same 73018 steps. They differ only in damage's cards being
    counted as trashes, not discards (`CardDiscarded` 868 → 503,
    `CardTrashed` 1686 → 2051 by view, and each card's `trashed`). The
    heuristic seatings moved, of 192 games (view; by index the steps are
    112879):

    | | main | Stage 6 |
    |---|---|---|
    | Corp agenda wins | 60 | 73 |
    | Corp flatlines | 10 | 16 |
    | Runner agenda wins | 120 | 101 |
    | Deck-outs | 2 | 2 |
    | Steps | 111335 | 112883 |

  - A pinned ref with every engine and client change and none of the
    cards or the deck differs from main in the same counters and nothing
    else, in all four shapes: every game is the same game. So the damage
    trash, the trash pile, the two run facts, the credit words and `Mill`
    move no game. The heuristic movement is `determinize` sampling the
    new cards.

#### Stage 7 — a breach as a moment, and a breach of another server (28 September 2026)

`feat/tai-stage-7-expendables-breach-redirect`: Slash and Burn
Agriculture, Tree Line, Angelique Garza Correa, Eru Ayase-Pessoa, Beatriz
Friere Gonzalez, Oppo Research, S-Dobrado and Capybara. **No new
`Effect`.** The Automata Initiative 57 of 65; `TAI_UNIMPLEMENTED` 16 → 8.

- **A breach begins as a moment of its own** (`GameEvent::BreachBegun`,
  `Trigger::OnBreach`), before any candidate is presented.
  - "Access 1 additional card" must be applied before the random access
    limit is set (CR 7.3.5b), and Rotary asks the Runner first. So the
    accesses wait: a beginning that leaves something parked or queued
    returns with `RunState::breached` set, and `engine::resume_run` takes
    the breach on after the answer, a breach with no run included.
  - **A deviation corrected.** The seven "whenever you breach" cards
    (Docklands Pass, Rotary, Manuel Lattes de Moura, "Pretty" Mary da
    Silva, Devadatta Drone, Cupellation, Mercury: Chrome Libertador) heard
    the *successful run* on the server. That was the same moment only
    while every run breached the server it attacked. They hear the breach
    now, Cataloguer's breach with no run included.
  - Mercury's one printed ability asks which server is breached
    (`EffectRequirement::Breaching`, the run's breach only), with "HQ or
    R&D" said as `Not(And(Not(..), Not(..)))`, since there is no `Or`.
- **A run's breach of another server** (Eru's "Run Archives. If
  successful, instead of breaching Archives, breach R&D"; Beatriz's run on
  HQ).
  - `SetAccessReplacement { server: Archives, effect: Breach(RnD) }`:
    `Effect::Breach` resolved as a replacement (`ResolutionContext::
    replacing_breach`) names the server the run breaches instead. The run
    stays a run on Archives, to its `RunCompleted`.
  - Beatriz's "When you do, access 1 additional card" is in the same
    replacement, ahead of the breach.
  - Eru's "Threat 3 → Whenever you breach R&D during a run on Archives" is
    `OnBreach` on R&D with `DuringRunOn(Archives)`.
  - Drawn: the log says "the Runner breached R&D" when the breach is not
    of the server the run succeeded on, and is silent otherwise.
- **The times ice was encountered during a run** (`RunState::encounters`,
  `Amount::EncountersThisRun`, in the view): S-Dobrado's "the first time"
  and "the second time you encounter a piece of ice during that run",
  heard by the run event in the play area. Capybara hears the bypass and
  derezzes the encountered ice for the cost of removing itself.
- **The rest composes.** The three Weyland cards are expendable (Tocsin's
  `from_hand` and `RevealAndTrashSelf`). Tree Line is Ice Wall's
  advanceable strength. Angelique's access is BANGUN's "while it is
  rezzed". Oppo Research is Active Policing's play requirement and
  `EndActionPhase`.
- **Decks.** Every built identity already has a deck, so the cards went
  into in-faction Sweep decks, two copies each, in place of cards other
  decks still carry:
  - Land Grab took the four Corp cards (Slash and Burn for its Let Them
    Dream, at the same points). Without its Ice Wall it is Standard-legal,
    and the pin says so.
  - Picket Line took S-Dobrado and Capybara, Grassroots took Eru, and Safety
    Net took Beatriz.
- **DSL ratio (`pool_status.py`): 20 of 91 `Effect` variants single-use,
  1 unused**, over 372 card files, from 21: `Breach` has three cards.
- **Real play**, 96 games a seating (seed 2), Land Grab against each of
  the three Runner decks.
  - Random seats:
    - Slash and Burn used from HQ 31 times, Tree Line 26.
    - Angelique used from HQ 7 times; she asked about her meat damage on
      184 accesses.
    - Oppo Research played 38 times.
    - Eru used 73 times, his threat access 16. Beatriz used 78 times.
    - S-Dobrado played 35 times but bypassed once: a random Corp rarely
      rezzes central ice.
    - **Capybara was installed 30 times and never heard a bypass.** It is
      reached by its card test alone.
  - Heuristic seats use none of the Runner cards. The Corp scores Slash and
    Burn and rezzes Tree Line, but never uses a card from HQ, never plays
    Oppo Research, and installs Angelique 10 times in 288 games (Phase 5
    debt).
- **Measured.** Both sweeps at 256 seeds are green, the card gate
  included.
  - **Attribution.** An engine ref with the breach step and none of the
    cards moved (the seven kept on `OnSuccessfulRun`) differs from
    `origin/main` only in the new `BreachBegun` count, in all four
    shapes: the breach step moves no game.
  - Moving the seven cards to `OnBreach` moves the random seatings, with
    fewer trigger-order prompts for the Runner (`ChooseTriggerToResolve`
    301 → 297). Of 192 games: steps 73018 → 72305, Corp flatlines
    75 → 74, Runner agenda wins 113 → 114. The heuristic seatings play the
    same games, their counts renamed.
  - The new cards and decks leave both random seatings identical: Sweep
    decks are in no `matchups()`. The heuristic seatings move by
    `determinize`, of 192 games:

    | | before the cards | Stage 7 |
    |---|---|---|
    | Corp agenda wins | 73 | 65 |
    | Corp flatlines | 16 | 14 |
    | Runner agenda wins | 101 | 111 |
    | Steps | 112883 | 110799 |

#### Stage 8a — a "when encountered" ability prevented, and subroutines gained by count (28 September 2026)

`feat/tai-stage-8a-starlit-knight-airbladex`: Starlit Knight and AirbladeX
(JSRF Ed.). Stage 8 is split by mechanic into four PRs (8a–8d). **No new
`Effect`.** The Automata Initiative 59 of 65; `TAI_UNIMPLEMENTED` 8 → 6.

- **Another card's ability as something to prevent** (CR 9.9.3a, 9.9.5):
  AirbladeX's "Prevent a “when encountered” ability on a piece of ice".
  - `Preventable::EncounterAbility`, `WouldHappen::EncounterAbility {
    ice }`. When the encountered ice's own `OnEncounter` trigger is about
    to resolve and somebody could prevent it, it is parked with the
    trigger itself in `PendingPrevention::waiting`
    (`prevention::encounter_ability`, asked in `dispatcher::fire_one`,
    which every fired trigger passes through). It resolves after the
    window unless it was prevented.
  - Nobody is asked when nobody could prevent it (`could_prevent`), so no
    game without an AirbladeX moves.
  - Drawn: "Prevent Paywall's “when encountered” ability?", and the log's
    "a “when encountered” ability was prevented".
- **"Use this ability only during a run" for an interrupt**
  (`EffectRequirement::RunInProgress`): AirbladeX's net damage can come
  from a card accessed. `DuringRun` stops as the breach begins, and five
  cards rely on that.
- **Subroutines gained by count** (`GainSubroutine::count`): Starlit
  Knight's X "End the run" subroutines, X the Runner's tags, read as they
  are gained.
- **A leak fixed.** `GameEvent::CardHosted` named a piece of ice a Runner
  card was installed on to a viewer who could not identify it (the
  256-seed view sweep, seed 166: Stowaway on an unrezzed Tithe, reached
  once this stage's deck swap re-paired the seeds). `host` is now
  optional and struck out for a viewer who cannot identify it.
- **Decks.** Grand Opening took two Starlit Knight for its two Capacitor.
  Safety Net took two AirbladeX for its two Methuselah. Paid Content and
  Spare Parts still carry the cards that left.
- **DSL ratio (`pool_status.py`):** unchanged at 20 of 91 `Effect`
  variants single-use, 1 unused.
- **Real play**, 96 games a seating (seed 2), Grand Opening against Safety
  Net.
  - Random seats: AirbladeX installed 32 times and used 3. Starlit
    Knight's threat-4 trigger fired 6 times, gaining 10 subroutines.
  - Heuristic seats: the Runner never installs AirbladeX (Phase 5 debt).
    Starlit Knight's trigger fired 7 times with the Runner untagged,
    gaining none.
- **Measured.** Both sweeps at 256 seeds are green, the card gate
  included.
  - A ref with every engine and client change and neither card nor deck
    swap is identical to `origin/main` in all four shapes. The prevention
    hook, the count and the masking fix move no game.
  - With the cards, the random seatings stay identical. The heuristic
    seatings move by `determinize`, of 192 games:

    | | before the cards | Stage 8a |
    |---|---|---|
    | Corp agenda wins | 65 | 55 |
    | Corp flatlines | 14 | 9 |
    | Runner agenda wins | 111 | 127 |
    | Steps | 110799 | 109018 |

#### Stage 8b — a psi game on a successful run, and costs paid in grip cards (29 September 2026)

`feat/tai-stage-8b-adrian-seis-daniela`: Adrian Seis and Daniela Jorge
Inácio. **No new `Effect`.** The Automata Initiative 61 of 65;
`TAI_UNIMPLEMENTED` 6 → 4.

- **A cost paid in grip cards** (`Cost::AddRandomFromGripToBottom`):
  Daniela's "the Runner must add 2 cards from the grip at random to the
  bottom of the stack". It cannot be paid with fewer cards than that, so
  then there is no trash or steal (CR 1.16).
- **Additional costs to steal and to trash that are not credits** (CR
  1.16.10).
  - `ContinuousKind::StealCost` is a `Cost` now, not a number of credits:
    Magistrate Revontulet's is `Credits(3)`.
  - `ContinuousKind::AdditionalTrashCost` is the trash's
    (`AccessPhase::PendingChoice::trash_also`), paid with the credits and
    drawn on the access's facts line ("Trashing it costs 2 credits and:
    add 2 cards from your grip at random to the bottom of your stack").
  - Daniela's trash cost binds only while she is rezzed
    (`CurrentlyAccessingInstalledCard { rezzed_only }` as the `while`):
    9.1.8 has no exception for a trash cost as 9.1.8e has for a steal.
  - "To steal an agenda from this server or its root" is
    `Scope::StealingFromThisServer`: accessed in a breach of her server.
    It persists for the rest of the run she is trashed in (9.12.5), read
    from the run's trashed persistent upgrades as Mahkota Langit Grid's
    trash cost is.
- **Prohibitions on access, bound to one install, for the rest of a run**
  (CR 7.4.2): Adrian's psi game. `Prohibition::AccessOthers` ("cannot
  access cards other than this upgrade") and `Prohibition::Access`
  ("cannot access this upgrade"), made by `Effect::Prohibit {
  this_install }` and asked where the candidates are pruned
  (`lingering::installs_prohibited`). Drawn in the In effect list with the
  upgrade's name. His move as the turn ends is Vovô Ozetti's.
- **Deck.** Honor Roll took two Adrian Seis for its two Mahkota Langit
  Grid, and two Daniela for its two Empiricist. Other decks still carry
  the cards that left.
- **DSL ratio:** unchanged at 20 of 91 `Effect` variants single-use.
- **Real play**, 96 games a seating (seed 2), Honor Roll against Picket
  Line.
  - Random seats: Adrian rezzed 73 times and played 44 psi games.
    Daniela rezzed 56 times and was accessed 83 times.
  - Heuristic seats: Adrian rezzed 70 times and played 73 psi games.
    Daniela rezzed 63 times and was accessed 214 times.
- **Measured.** Both sweeps at 256 seeds are green, the card gate and the
  concealment check included.
  - A ref with every engine and client change and neither card nor the
    deck swap is identical to `origin/main` in all four shapes.
    Magistrate's cost as a `Cost` moves nothing.
  - With the cards, the random seatings stay identical. The heuristic
    seatings move by `determinize`, of 192 games:

    | | before the cards | Stage 8b |
    |---|---|---|
    | Corp agenda wins | 55 | 67 |
    | Corp flatlines | 9 | 13 |
    | Runner agenda wins | 127 | 112 |
    | Steps | 109018 | 115913 |

#### Stage 8c — a limit on remote servers, and a program installed during a run (29 September 2026)

`feat/tai-stage-8c-arissana-a-teia`: A Teia: IP Recovery and Arissana Rocha
Nahu: Street Artist, each on a Sweep deck of its own. **No new `Effect`.**
The Automata Initiative 63 of 65; `TAI_UNIMPLEMENTED` 4 → 2.

- **"Limit 2 remote servers"** (CR 4.6.8f,
  `ContinuousKind::RemoteServerLimit`, on the identity's controller).
  - Every Corp install refuses a new remote past the limit
    (`engine::place_corp_card`, `RulesError::RemoteServerLimit`).
  - Every offer of a fresh remote asks the same question
    (`legal_actions::may_add_remote`): the install action,
    `corp_install_destinations` for text installs, and the one effect
    that installs into a new remote unasked.
- **"The first time each turn you install a card in the root of or
  protecting a remote server".**
  - `EventFilter::InstalledIn(ServerKind)` reads the install's server off
    the moment, as `InstalledFromHq` reads where it came from.
  - "The first time" cannot be `first_each_turn`, since the turn log's
    classes hold no server. It is a count beside the cells, as The Holo
    Man's installs from HQ are: `TurnLog::installed_in_remotes`,
    `Amount::CardsInstalledInRemotesThisTurn`, under `Not(AmountAtLeast(..,
    2))`.
  - The second card is Peer Review's install, restricted to another
    remote (`another_server` from the first card, by `acts_on_subject`),
    with Warm Reception's "you cannot score that card this turn".
- **Arissana's install during a run.**
  - "Use this ability only during a run" is `RunInProgress`, beside
    `OncePerTurn`.
  - The program chosen goes by Muse's words: a trojan onto a piece of ice
    the Runner picks (`InstallProgramOnHost`), anything else into the rig
    (`InstallRunnerCardFromGrip`) with a run-end rider trashing it.
  - `InstallProgramOnHost` no longer asks for a host until it has one to
    use. A trojan's ice is chosen first, and an identity's ability has no
    install.
  - Recorded, not built: the run has one end-of-run rider slot, so
    Arissana used on a run Hannah "Wheels" Pilintra started replaces
    Hannah's tag.
- **Decks.** Second Site (A Teia, on A Thousand Cuts' frame) and Street
  Gallery (Arissana, on Safety Net's). Both are Eternal-only, being on
  Core Set frames.
- **Real play**, 96 games a seating (seed 2), Second Site against Street
  Gallery.
  - Random seats: A Teia's trigger resolved 464 times, and Arissana's
    ability was used 714 times.
  - Heuristic seats: A Teia's trigger resolved 315 times, but the Runner
    never uses Arissana's ability (Phase 5 debt).
- **Measured.** Both sweeps at 256 seeds are green, the card gate included.
  - `coverage_identical.py` reports every shape identical against
    `origin/main`, with the cards and without them.
  - Identities are not sampled by `determinize`, and Sweep decks are in no
    `matchups()`, so the heuristic seatings do not move either.

#### Stage 8d — "the same action", and an agenda used from the Runner's score area (29 September 2026)

`feat/tai-stage-8d-wage-workers-oracle-thinktank`: Wage Workers and Oracle
Thinktank. **No new `Effect`.** **The Automata Initiative complete, 65 of
65**; `TAI_UNIMPLEMENTED` 2 → 0.

- **"The same action"** (CR 5.2.5a–b). `turn_log::SameAction` is the
  basic action, where every install is one action and every play of an
  operation or event another, or an ability by its card's handle and
  index. Two copies' abilities are different actions.
  - The turn log counts each action taken (`TurnLog::times_taken`), in a
    fixed table of twelve so the log stays `Copy`, with a sparse form on
    the wire.
  - `Amount::TimesThisActionThisTurn` reads "that action" off the
    triggering event.
  - Recorded, not built: a card in hand has no handle, so two copies'
    abilities used from HQ (Descent, Tocsin) count as one action there.
    No card in the pool asks.
- **"Whenever you finish taking an action"** (CR 5.2.2b, 5.2.2d):
  `GameEvent::ActionFinished`, heard by `Trigger::OnActionFinished`, the
  actor's moment and about nothing.
  - It is announced once everything the action parked has resolved,
    queued the way Nuvem SA's `FinishedResolving` is.
  - An action that begins a run or a breach finishes when that run or
    breach does (`RunState::finishes`), not when the click is spent.
    Bot samples leave it unset, since only the Corp's own actions are
    heard.
  - Wage Workers is `OnActionFinished` with "exactly 3" as `AmountAtLeast`
    3 and `Not` 4. The log does not narrate the event, because the
    action's own line already says it was taken.
- **An agenda's ability used from the Runner's score area** (CR 9.1.8b):
  Oracle Thinktank's "[click], remove 1 tag: Shuffle this agenda into
  R&D".
  - A stolen agenda now takes a handle of its own
    (`access::resolve_steal`, `RunnerState::find_stolen`). Every lookup
    of an ability's card asks there too: the engine, the action mask,
    the action's owner, the payment and the requirement
    `EffectRequirement::InRunnersScoreArea`.
  - `AddToDeck` takes the agenda out of the Runner's score area along
    with its points, and `ShuffleIntoDeck` of R&D alone shuffles it.
  - Drawn: the score area's rows offer the ability on a stolen agenda
    (`hud::ScoredCard::install`), and an action names the agenda by its
    title in either score area (`actions::install_label`).
- **Decks.**
  - Paid Content took two Oracle Thinktank for the Tomorrow's Headline and
    a Neurospike, point for point.
  - Deterrence took two Wage Workers for its two Corporate Hospitality.
  - Other decks still carry the cards that left.
- **DSL ratio (`pool_status.py`):** unchanged at 18 of 91 `Effect`
  variants single-use, 1 unused.
- **Real play**, 96 games a seating (seed 2), against Picket Line.
  - Paid Content: Oracle Thinktank stolen 69 times on random seats, with
    its tag given 58 times and its ability used 30. On heuristic seats it
    was stolen 52 times, with 47 tags given and 8 uses.
  - Deterrence: Wage Workers rezzed 40 times on random seats and gained a
    click 8 times. On heuristic seats it was rezzed 74 times and gained 84
    clicks.
- **Measured.** Both sweeps at 256 seeds are green, the card gate included.
  - A ref with every engine and client change and neither card nor deck
    swap differs from `origin/main` in all four shapes by the new event's
    own count and nothing else: `ActionFinished` 0 → 12147 on random
    seats, 0 → 20259 on heuristic ones. The stolen agenda's handle, the
    counts and the announcement move no game.
  - With the cards, the random seatings stay identical. The heuristic
    seatings move by `determinize`, of 192 games:

    | | before the cards | Stage 8d |
    |---|---|---|
    | Corp agenda wins | 67 | 80 |
    | Corp flatlines | 13 | 15 |
    | Runner agenda wins | 112 | 97 |
    | Steps | 115913 | 122418 |

### 4. Parhelion — 63 cards (C 19 / V 26 / M 18)

#### Stage 1 — ten Corp cards, composed (29 September 2026)

`feat/ph-stage-1-corp-composes`: Thule Subsea: Safety Below, Distributed
Tracing, Djupstad Grid, Reaper Function, Vampyronassa, Post-Truth Dividend,
Gaslight, Vera Ivanovna Shuyskaya, Nonequivalent Exchange and Shipment from
Vladisibirsk. **No new `Effect`, and no change to the engine or the
clients:** card files, decks and tests only. Parhelion 10 of 63;
`PH_UNIMPLEMENTED` 63 → 53. The survey's "composes" held for all ten.

- **What each is made of.**
  - Thule Subsea's "unless they spend [click] and 2[credit]" is
    Jaguarundi's nested paid choice with `Cost::AllOf` as its price, on
    Epiphany Analytica's `OnAgendaStolen` about any agenda.
  - Distributed Tracing is Touch-ups' additional [click] and Public
    Trail's last-turn requirement, counting `OnAgendaStolen` instead of
    the successful run.
  - Djupstad Grid is Tucana's `AgendaCameFromThisCardsServer`, without
    persistence, doing core damage.
  - Reaper Function is Clearinghouse: "you may trash this asset to" is a
    cost (`Cost::TrashSelf`).
  - Gaslight's "you may trash this asset. **If you do**, search R&D" is
    not a cost (CR 1.16.11). The trash is the first effect of the "may",
    and Pivot's search follows it, revealed and shuffled. It asks before
    the mandatory draw, which waits for it (TAI Stage 1).
  - Vera Ivanovna Shuyskaya takes two entries, scored and stolen, each a
    "may" whose yes is a selection of one card from the Runner's grip to
    their heap, revealed. That is Touch-ups' reading of "reveal the grip":
    the Corp is shown the whole grip as the selection's candidates.
  - Vampyronassa's "you may draw 1 or 2 cards" is a three-way choice: 1,
    2 or none.
  - Shipment from Vladisibirsk's "a total of 4" is four selections of one
    advanceable install, each placing 1 counter. A card may be chosen
    more than once, which is what "a total of" allows.
  - Post-Truth Dividend and Nonequivalent Exchange are a "may" each.
- **Fidelity limits.**
  - Vera's reveal is not a `CardRevealed` of each grip card. The Corp
    sees the grip through the selection, and only the trashed card is
    announced as revealed. No card in the pool reads a reveal of the
    grip.
  - Shipment's four selections cannot be answered as one. A person picks
    a card four times, where the printed card would take the whole
    distribution at once. The outcome is the same.
- **Client.** Nothing added to the view, the log or a decision, so no
  ledger line and no ledger row. Thule Subsea's price is worded by
  `prose`'s existing `AllOf` ("and"), and the pop-up shows the card's own
  clause.
- **Decks.**
  - **Undertow**, a new Corp Sweep deck on Thule Subsea, on Retirement
    Package's frame. Two Distributed Tracing, two Djupstad Grid, two
    Shipment from Vladisibirsk, a Vera Ivanovna Shuyskaya and two Reaper
    Function replace its two Brasília Government Grid, two Caveat Emptor,
    two Retirement Plan, two Synchrocyclotron and a Sleipnir. Influence
    14 of 15, 21 points. It is Standard-legal (pinned `not_startup`),
    since Engineering the Future was the frame's one Core Set card.
  - **Paid Content** took two Gaslight, two Nonequivalent Exchange, two
    Post-Truth Dividend and a third Kingmaking. They replace its
    Neurospike, two Manegarm Skunkworks, two Funhouse, a Send a Message
    and its Superconducting Hub, and the points stay at 20.
  - **A Thousand Cuts** took two Vampyronassa for its Ansel 1.0 and a
    Mindscaping.
- **DSL ratio (`pool_status.py`):** unchanged at 18 of 91 `Effect`
  variants single-use, 1 unused (`Trace`), over 390 card files.
- **Real play**, 96 games of each edited Corp deck against Safety Net
  (seed 2).
  - Undertow, random seats: Thule Subsea's trigger fired 203 times.
    Reaper Function was trashed for its damage 38 times, Vera heard 1
    score and 10 steals, Distributed Tracing was played twice and
    Shipment once.
  - Undertow, heuristic seats: Thule Subsea fired 179 times, Reaper
    Function 28 and Vera 19. The heuristic Corp never plays Distributed
    Tracing or Shipment (Phase 5 debt).
  - Djupstad Grid was installed 80 and 98 times and rezzed 46 and 13, but
    no agenda was scored from its root in either seating. Its damage is
    reached by the card's test alone.
  - Paid Content: Gaslight's turn-start "may" resolved 111 times on
    random seats and 1206 on heuristic ones. The heuristic Corp keeps it
    and never trashes it for the search, where random seats trashed it
    90 times.
  - Paid Content: Nonequivalent Exchange was played 43 times on random
    seats and never on heuristic ones (Phase 5 debt). Post-Truth Dividend
    was scored 5 and 36 times.
  - A Thousand Cuts: Vampyronassa (rez cost 7) is never rezzed on random
    seats. On heuristic seats it was rezzed 7 times, with 37 subroutines
    fired and 15 broken.
- **Measured.** Both sweeps at 256 seeds are green, the card gate
  included.
  - `coverage_identical.py` against `origin/main` has both random
    seatings **identical**.
  - Both heuristic seatings moved. The stage changes no engine code, and
    a Sweep deck is in no `matchups()`, so `determinize` sampling the ten
    new Corp cards is the one path by which they can. Of 192 games:

    | | main | Stage 1 (view) | Stage 1 (index) |
    |---|---|---|---|
    | Corp agenda wins | 80 | 82 | 81 |
    | Corp flatlines | 15 | 9 | 9 |
    | Runner agenda wins | 97 | 99 | 100 |
    | Runner deck-outs | 0 | 2 | 2 |
    | Steps | 122418 | 122046 | 122033 |

#### Stage 2a — seven Runner cards and a Weyland operation, composed (29 September 2026)

`feat/ph-stage-2a-runner-composes`: Finality, Katorga Breakout, Nga, Num,
Zenit Chip JZ-2MJ, Hippocampic Mechanocytes, Dr. Nuka Vrolyck and End of
the Line. **No new `Effect`, and one rule fix.** Parhelion 18 of 63;
`PH_UNIMPLEMENTED` 53 → 45.

Stage 2 was split by mechanic. Hostile Architecture moved to Stage 2b,
because it needs two things the engine does not have:
- One `when` that says two things: the Runner's trash of a Corp card that
  was installed.
- A rezzed card hearing its own trash under "any": "(including this
  asset)", which is CR 4.6.6i's Warroid Tracker example.

- **What each is made of.**
  - Finality is The Maker's Eye with 3 additional accesses, and its
    additional cost is `Cost::SufferDamage` as an `additional_play_cost`.
  - Katorga Breakout is Overclock's any-server run. Its `on_success`
    selects one heap card into the grip.
  - Nga and Dr. Nuka Vrolyck load counters the way Juli Moreira Lee does,
    and "when it is empty, trash it" follows the spend, as on Cataloguer.
  - Nga's "you may remove 1 hosted power counter to sabotage 1" is a paid
    choice priced `Cost::RemoveCounters`, on the first successful run each
    turn.
  - Num is a killer at strength 8 with no boost.
  - Zenit Chip's core damage is an install trigger. Its draw is Gabriel
    Santiago's first-time `OnSuccessfulRun` with `when` set to the three
    centrals.
  - Hippocampic Mechanocytes' hand size is `HandSize { per: 1, of:
    HostedCounters }`.
  - End of the Line's additional cost is `Cost::RemoveTags(1)`.
- **Fixed: a run event could find itself in the heap** (CR 8.6.5).
  - An event that begins a run stays in the play area until the run is
    over. The engine files it in the heap as it is played, and reads the
    play area off the run.
  - Katorga Breakout's "if successful, add 1 card from your heap" was
    offered the Katorga Breakout whose run it was.
  - `pending_choice::run_event_in` now keeps the run's event out of
    every heap selection, whoever is choosing. It is the sibling of
    `resolving_operation_in`, which keeps an operation out of its own
    Archives selection (CR 8.6.7a).
  - The fix is recorded under 8.6 in
    [rules-conformance.md](rules-conformance.md).
- **Found on the way:** `board::diff`'s transitions test assumed that a
  run is still in progress after the action that began it.
  - The deck swaps re-rolled seed 0 onto a game where Front Company's
    damage flatlines the Runner as the run begins. The game ends, and the
    run with it.
  - The check is now skipped when the entry holds `GameOver`. This was not
    a client bug: the transition is right, and there is no run left to
    compare it with.
- **Client.** Nothing added to the view, the log or a decision, so no
  ledger line and no ledger row.
- **Decks.**
  - Pay As You Go took two Finality, two Katorga Breakout, two Nga and two
    Num. They replace two Fermenter, two Leech, a Take a Dive, a
    Cyberfeeder, a Nurse Hạnh and a Buzzsaw, all of which another deck
    still carries.
  - Picket Line took two Zenit Chip JZ-2MJ for two Jailbreak.
  - Street Gallery took two Hippocampic Mechanocytes and two Dr. Nuka
    Vrolyck for two Joy Ride and two Living Mural, which Safety Net still
    carries.
  - Land Grab took two End of the Line for two Business As Usual. Its
    Oppo Research gives the tags that End of the Line needs to be played.
- **DSL ratio (`pool_status.py`): 17 of 91 `Effect` variants
  single-use** (18 before), 1 unused, over 398 card files.
- **Real play**, 96 games a seating (seed 2), each Runner deck against
  Retirement Package and Land Grab against Safety Net.
  - Random seats: Finality played 20 times, Katorga Breakout 27 (its heap
    choice asked 6 times), Nga installed 22 times with its successful-run
    trigger resolving 53 times, and Num installed 11 times.
  - Random seats: Zenit Chip installed 37 times with its draw heard 92
    times, Hippocampic Mechanocytes installed 50 times, and Dr. Nuka
    Vrolyck installed 27 times and used 30. End of the Line was played
    twice.
  - Heuristic seats: Finality played 21 times, Num installed 34 times and
    used 75, and End of the Line played once.
  - The heuristic Runner never plays Katorga Breakout and never installs
    Nga, Zenit Chip, Hippocampic Mechanocytes or Dr. Nuka Vrolyck (Phase 5
    debt).
- **Measured.** Both sweeps at 256 seeds are green, the card gate
  included.
  - `coverage_identical.py` against `origin/main` has both random
    seatings **identical**.
  - A ref with the heap fix alone is identical in all four shapes.
  - With the cards, the heuristic seatings moved: `determinize` samples
    End of the Line among the Corp's hidden cards. Of 192 games (view):
    Corp agenda wins 82 → 70, Corp flatlines 9 → 15, Runner agenda wins
    99 → 107, Runner deck-outs 2 → 0.

#### Stage 2b — the Runner's trash of an installed Corp card, its own included (29 September 2026)

`feat/ph-stage-2b-hostile-architecture`: Hostile Architecture. **No new
`Effect`.** Parhelion 19 of 63; `PH_UNIMPLEMENTED` 45 → 44. Stage 2 is
complete.

"The first time each turn the Runner trashes any of your installed cards
(including this asset), do 2 meat damage."

- **Two moments, one count.** The Runner's trash on access is
  `OnTrashedFromAccess`, where `when: InstalledCard(Any)` is enough (as
  for Aggressive Trendsetting). A trash by the Runner's card text is
  `OnCardTrashed`. The card's two entries are one printed ability, so
  they share one "first time" (the Turn History Rule).
- **`EventFilter::All`, a conjunction.** On `OnCardTrashed`, "the
  Runner's trash of a Corp card" is `OwnedBy` and "that was installed" is
  `TrashedFrom([Installed])`. Each says half, and a `when` held one
  filter. A variant for the pair would have been the first of one per
  pair a card prints.
  - `listeners::passes` asks every part.
  - `EventFilter::names_whose` answers whose moment it is for a
    conjunction as for its parts.
  - `validate` wants two or more parts, each fitting on its own, and no
    conjunction nested inside another. The per-filter check moved into
    `CardDefinition::filter_fits`, so a part is held to what it would be
    alone.
  - `turn_log::Occurrences::meant_by` counts the columns every part
    admits.
  - `TrashedFrom([Installed])` can now narrow a first time, because the
    log's columns already say whether the card was installed. Any other
    pile is still refused.
  - `prose` words a conjunction as its parts (`describe_when`).
- **A rezzed card hears its own trash on access** (CR 4.6.6i's Warroid
  Tracker example: a rezzed card trashed meets its own trigger
  condition).
  - A trigger about any card is heard only by active cards. Before this
    stage, a trashed Hostile Architecture heard its own trash by nothing.
  - `listeners::Moment::was_active` is read off the access, which still
    holds the install as it was presented (`pending_install_rezzed`).
    That makes the subject an active listener for that one moment.
  - `is_this` still knows it: a handle-less subject is "this" to itself
    whether it is inactive or was active as it left, and an active copy
    still on the table keeps its handle, so it is never "this".
  - **Not built:** a rezzed Hostile Architecture trashed by the Runner's
    card text. `GameEvent::CardTrashed` does not carry whether the card
    was rezzed, and adding that to its 42 sites is more than one card
    wants. Charm Offensive can do it, and no game has.
- **Client.** Nothing new in the view. The card inspector's "Engine reads
  it as" words the conjunction.
- **Decks.** Hostile Bid took two Hostile Architecture for two Hearts and
  Minds, which Land Grab still carries.
- **DSL ratio (`pool_status.py`):** unchanged at 17 of 91 `Effect`
  variants single-use, 1 unused, over 399 card files.
- **Real play**, 96 games a seating (seed 2), Hostile Bid against Safety
  Net.
  - Random seats: rezzed 8 times (it costs 5), and its trigger fired
    twice.
  - Heuristic seats: installed 90 times and never rezzed (Phase 5 debt).
- **Measured.** Both sweeps at 256 seeds are green, the card gate
  included.
  - A ref with the conjunction and the listener change but not the card
    is identical to `origin/main` in all four shapes.
  - With the card, the random seatings are identical. The heuristic ones
    move by `determinize` (view, of 192): Corp agenda wins 70 → 71, Corp
    flatlines 15 → 17, Runner agenda wins 107 → 103.


#### Stage 3a — the advancement requirement, asked (30 September 2026)

`feat/ph-stage-3a-advancement-requirement`: Ontological Dependence,
Freedom of Information, Regulatory Capture. **No new `Effect`.** Parhelion
22 of 63; `PH_UNIMPLEMENTED` 44 → 41. Stage 3 was split by mechanic when
it was taken: 3a the requirement, 3b the Corp words (Simulation Reset,
Hypoxia, Mr. Hendrik), 3c harmonic ice (Pulse, Bloop).

- **`ContinuousKind::AdvancementRequirement(Number)`**, the kind
  `dsl::continuous` had named as deferred. About `This`, on an agenda
  only (`validate`); CR 9.1.8e makes such an ability active while its
  card is not, which `Scope::This` already is. Composition didn't work:
  the requirement was the printed field, read by the score.
- **`continuous::advancement_requirement`** is the one question — printed
  plus what the card's text adds — asked by `engine::score_agenda`, by its
  dividends and by the view. **Signed and never floored** (CR 1.1.3): a
  requirement of −1 is met by no counters (CR 1.17.3a), which the card
  test scores. `RulesError::AdvancementRequirementNotMet::required` is
  signed with it.
- **Dividends** count the counters past the requirement as it stood when
  the score began, before the agenda moved (CR 10.13.2) — the number the
  score already asked, read before the uninstall.
- **`Amount::CoreDamageTaken`** reads `RunnerState::brain_damage`, which
  nothing removes, so "this game" is all of it. Freedom of Information
  reads `RunnerTags`.
- **Regulatory Capture's "for each bad publicity you have up to 4"** is
  four `−1`s, each `while AmountAtLeast(BadPublicity, k)` for k = 1…4:
  composed, rather than an `Amount` with a cap for one card.
- **The view carries the asked requirement**,
  `PublicInstalledCard::advancement_requirement`, signed, masked with the
  card (a lowered requirement names the agenda). A client holds a view,
  not a `GameState`, so it cannot ask the layer itself.
- **Client.** The desktop's tile badge ("0/1") and the sheet ("advanced 0
  of 1", and "Prints an advancement requirement of 4; its text makes it
  1") read it through `board::facts`; the terminal's server line counts an
  agenda's counters against it ("1/1 adv"), where it showed no
  requirement before. `view_ledger` says so; tested in `board::facts` and
  the terminal's `tui::tests`. The inspector's engine reading words the
  kind and the amount.
- **Bots.** `eval::corp_install_value` and `finishable_agendas` ask the
  same question (the state is now passed down), so a lowered requirement
  is a nearer score; a bot's sample carries tags, bad publicity and core
  damage, so it asks the same answer.
- **Decks.** Undertow took two Ontological Dependence for its two
  Lightning Laboratory, Paid Content two Freedom of Information for two
  Stoke the Embers, Tag You're It two Regulatory Capture for two
  Sacrifice Zone Expansion; each card they left is still in another deck.
- **DSL ratio (`pool_status.py`):** unchanged at 17 of 91 `Effect`
  variants single-use, 1 unused, over 402 card files.
- **Measured.** Both sweeps green at 256 seeds, the card gate included.
  - A ref with the engine change and no cards is identical to
    `origin/main` in all four shapes: the view's new field, the bots'
    asked requirement and the score read the printed number wherever no
    card changes it.
  - With the cards, both random seatings are identical (Sweep decks are
    not in `matchups()`); the planner seatings move by `determinize`
    sampling the new agendas (view, of 192): Corp agenda wins 61 → 58,
    Corp flatlines 28 → 27, Runner agenda wins 99 → 104, deck-outs 4 → 3.
- **Real play**, seed 2, each deck against a Runner Sweep deck (random 96
  games, planner 48): Ontological Dependence scored 5 / 32 times,
  Freedom of Information 4 / 14, Regulatory Capture 0 / 15.

#### Stage 3b — the Corp words: "that many", an operation out of the game, and every click as a price (30 September 2026)

`feat/ph-stage-3b-corp-words`: Simulation Reset, Hypoxia, Mr. Hendrik.
**No new `Effect`**; one `Cost`, one field. Parhelion 25 of 63;
`PH_UNIMPLEMENTED` 41 → 38.

- **`PromptChooseCards::count`, "that many".** Simulation Reset trashes
  up to 5 cards from HQ, then shuffles "that many" from Archives into
  R&D and draws "that many". `count` is `min` and `max` both, read when
  the prompt parks: `CardsSelected` inside the `then` of the HQ
  selection. Nothing is asked at 0, and `validate` wants `min` and `max`
  written 0 beside it (`CardValidationError::CountBesideBounds`).
  Composition didn't work: `min` and `max` are printed numbers, and the
  other way to say it was one `EffectIf` branch per count. A `then`
  resolves with nothing chosen, so the shuffle is behind
  `AmountAtLeast(CardsSelected, 1)`.
- **An operation removed from the game** (CR 8.6.6a): Hypoxia's and
  Simulation Reset's last line is `removed_after_play`, the events'
  declaration, now read by `engine::play_operation_card` too. It is filed
  as the play begins, like an operation out of Archives, so Simulation
  Reset is in no Archives selection it makes.
- **`Cost::LoseAllClicks`**, Mr. Hendrik's "If the Runner has any
  [click] remaining, they may lose all their [click] to prevent this
  damage": a nested cost (CR 1.16.11b), as Lionsmane's jack-out is,
  payable only with at least one click, which is the card's "if". It is
  the Runner's `OfferPaidChoice` inside the Corp's own "you may pay
  2[credit]". Not `LoseClicks`, whose number is printed; and not
  `EffectIf(ClicksRemaining)`, which reads the player whose action phase
  it is. **Fidelity:** a cost is asked before the damage is about to
  happen, so the Runner answers before the prevention window the damage
  then opens.
- **Hypoxia** is `IsTagged`, 1 core damage and `AllottedClicksNextTurn
  (Runner, −1)`; **Mr. Hendrik**'s trigger is Urtica Cipher's
  `OnAccessed` with `ThisCardIsInstalled`.
- **Client.** Nothing new in the view. Mr. Hendrik works face down, so
  it joins the traps whose rez gains nothing (`board::rez::gains_nothing`
  reads it off the card; the pool's list in its test grows by one): its
  rez is on its menu, never lit or waited for. `prose` words the count
  and the cost.
- **Decks.** Undertow took two Hypoxia for its two Corporate Hospitality
  (it gives tags) and two Mr. Hendrik for its two Warm Reception; Second
  Site two Simulation Reset for two of its three Mindscaping. Every card
  left is still in another deck.
- **DSL ratio (`pool_status.py`):** unchanged at 17 of 91 `Effect`
  variants single-use, 1 unused, over 405 card files.
- **Measured** against Stage 3a's tip. Both sweeps green at 256 seeds,
  the card gate included.
  - A ref with the engine change and no cards is identical in all four
    shapes.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view, of 192): Corp agenda wins 58 → 60, Corp
    flatlines 27 → 29, Runner agenda wins 104 → 99, deck-outs 3 → 4.
- **Real play**, seed 2 (random 96 games / planner 48): Hypoxia played 3
  / 5 times; Mr. Hendrik's access trigger fired 87 / 1 times (the planner
  Corp installed it 4 times); Simulation Reset played 44 / 1 times.

#### Stage 3c — harmonic ice: a count of installs, and a derez to pay for a rez (30 September 2026)

`feat/ph-stage-3c-harmonic-ice`: Pulse, Bloop. **No new `Effect`**; one
`Amount`. Parhelion 27 of 63; `PH_UNIMPLEMENTED` 38 → 36. **Stage 3 is
complete.**

- **`Amount::CorpInstalls(CardFilter)`**, Pulse's "The Runner loses
  1[credit] for each rezzed piece of harmonic ice": `All([Ice, Rezzed,
  HasSubtype(Harmonic)])`, counted as a selection over the Corp's
  installed cards reads it (`pending_choice::eligible_positions`), so
  `Rezzed` is asked of each copy. Composition didn't work: every count of
  installs was one sentence's (`OtherUnrezzedIce`,
  `IceProtectingThisServer`); `Amount` is no longer `Copy`, so it may
  carry a filter.
- **Pulse's rez** is Ping's trigger (`OnRez`,
  `RezzedDuringRunAgainstThisServer`) losing the Runner a click; its
  second subroutine is M.I.C.'s "End the run unless the Runner spends
  [click]".
- **Bloop's "As an additional cost to rez this ice, derez another piece
  of harmonic ice"** is its one `rez_alternatives` entry, `Cost::Derez`
  over harmonic ice (CR 1.16.10b). Only a rezzed card can pay it, so never
  Bloop itself; with none to derez the rez is refused and not offered.
  **Fidelity:** rezzed by a card's text (Send a Message, Mycoweb) it pays
  nothing, since `engine::rez_install` never reads `rez_alternatives` —
  the engine bug already on `ROADMAP.md`, now with a card that shows it.
- **Client.** Nothing new in the view; the derez is asked by the
  payment's own card question, already worded.
- **Decks.** Retirement Package took two Pulse for its two Vertigo and
  two Bloop for its two M.I.C., both of which Undertow still carries.
- **DSL ratio (`pool_status.py`):** unchanged at 17 of 91 `Effect`
  variants single-use, 1 unused, over 407 card files.
- **Measured** against Stage 3b's tip. Both sweeps green at 256 seeds,
  the card gate included.
  - A ref with the new `Amount` and no cards is identical in all four
    shapes.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view, of 192): Corp agenda wins 60 → 59,
    Runner agenda wins 99 → 100.
- **Real play**, seed 2, Retirement Package against Safety Net (random
  96 games / planner 48): Pulse rezzed 48 / 39 times, its rez trigger
  fired 43 / 38; Bloop rezzed 18 / 9 times, each by derezzing a Pulse.

#### Stage 4a — Runner standing words: allotted clicks, the other player's hand size, and "install only if" (1 October 2026)

`feat/ph-stage-4a-runner-standing-words`: Basilar Synthgland 2KVJ, Dr.
Vientiane Keeling, K2CP Turbine, Time Bomb. **No new `Effect`**; one
`ContinuousKind`, one card field. Parhelion 31 of 63; `PH_UNIMPLEMENTED`
36 → 32. Stage 4 was split by mechanic when it was taken: the standing
words here, the breaker words in 4b.

- **`ContinuousKind::AllottedClicks(Number)`**, Basilar Synthgland's "You
  get +1 allotted [click] for each of your turns" (`Scope::Controller`),
  asked by `turn::enter_start_of_turn` as the turn gains its clicks (CR
  1.11.2, 5.7.1a), beside the lingering `Lingering::AllottedClicks` an
  earlier turn made for this one. Composition didn't work: that one is
  made once by something that resolved and is spent on the turn it names.
  `enter_start_of_turn` now takes the registry, and so do the mulligan's
  two ways into the Corp's first turn. "When you install this hardware,
  suffer 2 core damage" is Hippocampic Mechanocytes' shape; with one card
  in the grip it flatlines the Runner (CR 1.7.2b).
- **A hand size about the other player.** Dr. Vientiane Keeling's "The
  Runner gets -1 maximum hand size for each hosted power counter" is a
  `HandSize` about `Scope::Player(Runner)`, which `validate` now admits for
  a hand size as well as a prohibition, and only for the player who does
  not control the card. `continuous::hand_size` asks it across both tables
  (`Target::Bound`), as `continuous::cannot` does, beside the side's own
  `Controller` effects (CR 5.5.3b). Its counters are an `OnRez` and an
  `OnTurnStart`, as Nico Campaign's are.
- **`CardDefinition::install_requirement`**, Time Bomb's "Install only if
  you made a successful run on a central server this turn"
  (`AmountAtLeast(TimesThisTurnWhen { OnSuccessfulRun, Server([Hq, RnD,
  Archives]) }, 1)`): a constraint on when a card may be installed is a
  restriction (CR 9.3.3b), beside `play_requirement` and
  `rez_requirement`. Asked as the card by `engine::install_into_rig`, the
  one door into the rig (`RulesError::InstallRequirementUnmet`), and by the
  gates a text install asks before it offers one
  (`can_install_runner_card_from_zone`, `can_install_program_onto`), so
  the two cannot disagree. `validate` refuses it off a Runner program,
  hardware or resource. Time Bomb's "if there are 3 or more hosted power
  counters, trash this hardware and sabotage 3. Otherwise, place 1 power
  counter" is two `EffectIf`s, the trash first and the counter guarded by
  `ThisCardIsInstalled`, so a third counter placed this turn does not go
  off until the next.
- **K2CP Turbine composes**: `Strength(+2)` about `Rig(All([Icebreaker,
  Not(HasSubtype(AI))]))`, Stegodon MK IV's scope.
- **Client.** Nothing new in the view: the allotment is the clicks the
  view already carries, the hand size is asked at the discard step, and an
  install that is refused is not offered. The engine reading words the new
  kind ("gets +1 allotted [click] each turn").
- **Decks.** Street Gallery took two Basilar Synthgland for two Coalescence
  and two K2CP Turbine for two Beta Build, all of which Safety Net still
  carries; Pay As You Go two Time Bomb for two Nurse Hạnh (Grassroots);
  A Thousand Cuts two Dr. Vientiane Keeling for two Cultivate (Second
  Site). All three stay Eternal-only.
- **DSL ratio (`pool_status.py`):** unchanged at 17 of 91 `Effect`
  variants single-use, 1 unused, over 411 card files.
- **Measured** against `main` (`77c1579`). Both sweeps green at 256 seeds,
  the card gate included.
  - A ref with the engine change and no cards is identical to `main` in
    all four shapes.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view and index alike, of 192): Corp agenda wins
    59 → 58, Corp flatlines 29 → 27, Runner agenda wins 100 → 103.
- **Real play**, seed 2, A Thousand Cuts against Pay As You Go and against
  Street Gallery (random 96 games / planner 48 each): Keeling installed
  62 + 41 / 30 + 43 times and rezzed 24 + 24 / 0, its turn-start counter
  placed 121 times on random seats; Time Bomb installed 7 / 0 times, its
  turn-start trigger fired 15; Basilar Synthgland installed 17 / 0; K2CP
  Turbine 14 / 0.
- **Bot debts:** the planner Corp installs Dr. Vientiane Keeling and never
  rezzes it; the planner Runner never installs Basilar Synthgland, K2CP
  Turbine or Time Bomb.

#### Stage 4b — breaker words: a cost a credit less per card, a break after a break, and a first time counted on the breaker (1 October 2026)

`feat/ph-stage-4b-breaker-words`: Tremolo, Poison Vial, WAKE Implant
v2A-JRJ, Abaasy. **No new `Effect`**; two `Amount`s, one
`EffectRequirement`. Parhelion 35 of 63; `PH_UNIMPLEMENTED` 32 → 28.
**Stage 4 is complete.**

- **`Amount::Reduced { amount, by }` and `Amount::RunnerInstalls(CardFilter)`**,
  Tremolo's "3[credit]: Break up to 2 barrier subroutines. This ability
  costs 1[credit] less to use for each installed piece of cybernetic
  hardware": `Cost::CreditsAmount(Reduced { Fixed(3), RunnerInstalls(All([
  CardType(Hardware), HasSubtype(Cybernetic)])) })`, floored at 0 (CR
  1.16.2a). `RunnerInstalls` is `CorpInstalls`' other side, counted as a
  selection over the rig reads its filter. Composition didn't work: every
  `Amount` was one number, and a `ContinuousKind` is about a card or a
  player, not one of a card's abilities.
- **`EffectRequirement::SubroutineBrokenThisEncounter`**, Poison Vial's
  "Use this ability only if you have already broken a subroutine during
  this encounter", read off the encounter's tally
  (`EncounterTally::broken_by`), which any break marks, a click's
  included, and which ends with the encounter. `SubroutineBrokenThisRun`
  reaches back to the run's earlier ice. Its "Hosted power counter: Break
  up to 2 subroutines" is Botulus's unconditional break, with no strength
  contest, and "When it is empty, trash it" is an `EffectIf` after the
  break, as Malandragem's is.
- **A first time counted on the breaker** (CR 6.5.7b). Abaasy's "The
  first time each turn this program fully breaks a piece of ice" is
  `OnIceFullyBroken`, `when: ByThis`, `first_each_turn`. The turn's log
  counts the ice's class and cannot say which object broke it, so a rig
  install now carries the `CopyTurn` a Corp install does
  (`InstalledRunnerCard::this_turn`), counting what the copy *did*:
  `turn_log::record` bumps the install a moment's `by` names, for the
  triggers `CopyTurn::counts_by` admits (only `OnIceFullyBroken`), and
  `listeners::is_first` judges a `ByThis` first time on that copy.
  `validate` admits `ByThis` beside `first_each_turn` only for a trigger
  the copy counts. Ice broken by two objects is fully broken by neither,
  so it counts for neither. Not in the view, as the Corp install's is
  not. Its "you may trash 1 card from your grip to draw 1 card" is an
  `OfferPaidChoice` of `Cost::Trash` from the grip.
- **WAKE Implant composes**: "Whenever you breach R&D, you may remove up
  to 3 hosted power counters to access that many additional cards" is a
  `ChooseNumber` up to 3 *of* its counters, whose `then` removes that many
  and adds that many accesses (`AddAdditionalAccessAmount`); the HQ count
  is an `OnSuccessfulRun` on HQ.
- **Client.** Nothing new in the view; the number, the paid choice and
  the abilities carry their printed clauses. The engine reading words the
  two amounts.
- **Decks.** Hit List took two WAKE Implant for two Docklands Pass, two
  Poison Vial for two Pennyshaver and two Tremolo for two Buzzsaw, all of
  which Borrowed Time still carries; Pay As You Go two Abaasy for two
  Botulus, which six sample decks carry. Both stay Eternal-only.
- **DSL ratio (`pool_status.py`):** 17 → 16 of 91 `Effect` variants
  single-use, 1 unused, over 415 card files: Poison Vial is the second
  card on `BreakSubroutinesUnconditionally`.
- **Measured** against Stage 4a's tip, every ref built by the same
  compiler (Rust 1.99). Both sweeps green at 256 seeds, the card gate
  included.
  - A ref with the engine change and no cards is identical in all four
    shapes.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view and index alike, of 192): Corp agenda wins
    58 → 59, Corp flatlines 27 → 28, Runner agenda wins 103 → 101.
- **Real play**, seed 2, Hit List and Pay As You Go against Retirement
  Package (random 96 games / planner 48 each): Tremolo installed 22 / 14
  times and used 20 / 15; Poison Vial installed 22 / 0 and used 3; WAKE
  Implant installed 32 / 0, its HQ counter placed 27 times and its R&D
  offer heard 29; Abaasy installed 28 / 25 and used 18 / 47, its
  first-time trigger fired once (random).
- **Bot debts:** the planner Runner never installs Poison Vial or WAKE
  Implant, and its Abaasy never fully broke a piece of ice on its own.

#### Stage 5a — the deckbuilding rules an identity prints (1 October 2026)

`feat/ph-stage-5a-deck-building-identities`: Nova Initiumia: Catalyst &
Impetus, Ampère: Cybernetics For Anyone. **No new `Effect`**; one card
field and its enum. Parhelion 37 of 63; `PH_UNIMPLEMENTED` 28 → 26.
Stage 5 was split by mechanic when it was taken: deck building here,
Archives in 5b, trashes in 5c, the stack and HQ in 5d.

- **`CardDefinition::deck_rules: Vec<DeckRule>`**, what an identity
  prints about the deck it leads (CR 1.4.1's "other variances from the
  standard deckbuilding rules"), on an identity only (`validate`).
  Three words, each what an identity in the pool prints:
  - `CopiesOfEachCard(1)`, Nova's and Ampère's "Your deck cannot include
    more than 1 copy of any card" — CR 1.4.7's "alternative copy
    limits". `CardDefinition::copy_limit_under(identity, default)` is the
    one statement of a card's limit (its own `deck_limit`, else three,
    never more than the identity's), read by both validators, both deck
    builders' add (the desktop's `Draft::add` now takes the identity, the
    terminal's reads it off the registry) and `determinize`'s playset.
  - `AgendasFromEachFaction(2)`, Ampère's "Your deck may include up to 2
    different agenda cards from each Corp faction": the faction rule's
    `OutOfFactionAgenda` gives way to a count of titles per faction,
    refused past two as `TooManyAgendasFromFaction`. Neither identity
    has an influence budget (the catalog's `null`, `unlimited_influence`).
  - `StarterGameOnly`, The Catalyst's and The Syndicate's "Starter game
    only." (CR 1.4.1a). `DeckFile::validate` held a deck to the published
    *Learn to Play* lists whenever its identity had no influence budget,
    which was the same two identities until Parhelion printed two more:
    read that way, every Nova deck was a starter deck no list matched.
  A test holds every identity's `deck_rules` to its printed text and its
  `unlimited_influence` to the catalog's `null`.
- **Client.** Nothing new in the view. Both builders stop at one copy
  under Nova or Ampère ("… is at its limit of 1"), and the verdict words
  the new refusal through the validator's own message.
- **Decks.** Two singleton Sweep decks, Standard-legal: **Mixtape**
  (Nova), forty Runner cards from every faction, and **Sampler**
  (Ampère), forty-five Corp cards with five neutral agendas, two
  Haas-Bioroid and one each from Jinteki, NBN and Weyland for 21 points.
  Each card is the most-played of its kind elsewhere, and every one was
  seen in the 256-seed sweep. The re-pairing left Spare Parts' one
  Meeting of Minds unseen; Spare Parts has no influence for a second, so
  Mixtape took one for a Maintenance Access.
- **DSL ratio (`pool_status.py`):** unchanged at 16 of 91 `Effect`
  variants single-use, 1 unused, over 417 card files.
- **Measured** against `main` (`0f4b531`). Both sweeps green at 256
  seeds, the card gate included, every card of both singleton decks seen.
  All four `coverage_identical.py` shapes are identical: an identity is
  not sampled, the Sweep decks are not in `matchups()`, and no matchup
  deck's identity prints a copy rule.

#### Stage 5b — Archives: an install of the copy chosen, and a subroutine of ice in Archives (1 October 2026)

`feat/ph-stage-5b-archives`: Hybrid Release, Nanisivik Grid. **No new
`Effect`**, and no new word: both cards compose. Parhelion 39 of 63;
`PH_UNIMPLEMENTED` 26 → 24.

- **Hybrid Release**'s "When you score this agenda, you may install 1
  facedown card from Archives" is a selection of `Facedown` cards in
  Archives (none: no question) whose `then` is `PromptInstallCorpCard`
  from Archives, behind Janaína's `CardsSelected` guard. **The install
  takes the copy the selection chose.** It took the first copy of the
  card, faceup or not, so with a faceup Ice Wall beside a facedown one
  the faceup one left Archives — a card Hybrid Release cannot install.
  The selection now tells its `then` which face the chosen copy had
  (`ResolutionContext::selected_facedown`), and the install looks for
  that face; two copies with one face are one card to both players.
  Mycoweb and Reanimation Protocol take the copy their Corp chose too.
- **Nanisivik Grid**'s "Whenever the Runner approaches this server, you
  may turn 1 facedown piece of ice in Archives faceup. If you do, resolve
  1 subroutine on that ice" is an `OnApproachServer` selection of
  `All([Ice, Facedown])` in Archives whose `then` is
  `TurnFaceupInArchives` (Cohort Guidance Program's) and
  `ResolveSubroutineOfSelectedIce` (Mycoweb's), resolved as the ice in
  Archives. **A subroutine resolved off an encounter is offered as the
  ice prints it**: the choice between two carried no text, so each option
  was the engine's rendering of its DSL; it is the subroutine's own text
  now (the Linked Clause Rule), for Mycoweb's too.
- **Client.** Nothing new in the view: the selection, the server choice
  and the subroutine choice are prompts both clients draw, and the last
  is now labelled with the ice's words. The Archives card turned faceup is
  `ArchivesTurnedFaceup`, as a breach's is.
- **Decks.** Honor Roll took two Hybrid Release for its two
  Superconducting Hub, a point each, and two Nanisivik Grid for its two
  Mitra Aman; a Jinteki deck's discards and trashes fill Archives with
  facedown cards for both. It stays Eternal-only.
- **Known limit.** A subroutine Nanisivik Grid resolves reads "this
  server" as unresolved, where CR 4.6.6i's example says it is Archives; no
  subroutine in the pool says "this server".
- **DSL ratio (`pool_status.py`):** 16 → 15 of 91 `Effect` variants
  single-use, 1 unused, over 419 card files: Nanisivik Grid is the second
  card on `TurnFaceupInArchives`. (Corrected in Stage 5c: this entry first
  said "unchanged at 16", a number carried from Stage 5a rather than
  taken.)
- **Measured** against Stage 5a's tip. Both sweeps green at 256 seeds,
  the card gate included.
  - A ref with the engine change and no cards is identical in all four
    shapes: no game in the pass met a faceup and a facedown copy of one
    card in Archives at a Mycoweb or a Reanimation Protocol.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view and index alike, of 192): Corp flatlines
    28 → 27, Runner agenda wins 101 → 102.
- **Real play**, seed 2, Honor Roll against Borrowed Time (random 96 games
  / planner 48): Hybrid Release scored 10 / 39 times, its install offered
  9 / 21; Nanisivik Grid rezzed 72 / 28 times and heard 52 / 33
  approaches, with facedown ice in Archives to offer on 16 / 0 of them.
- **Bot debts:** the planner Corp never had facedown ice in Archives when
  the Runner approached a Nanisivik Grid.

#### Stage 5c — trashes: the card a nested cost took, and where a trashed install stood (1 October 2026)

`feat/ph-stage-5c-trashes`: Kimberlite Field, Yakov Erikovich Avdakov,
World Tree. **No new `Effect`**; one `Amount`, two `CardFilter`s, one
`EventFilter`, and a field on `GameEvent::CardTrashed`. Parhelion 42 of
63; `PH_UNIMPLEMENTED` 24 → 21.

- **What a nested cost took** (CR 1.16.11a: "may [cost]. If you do" and
  "may [cost] to" are both nested costs). Kimberlite Field's "you may
  trash 1 of your rezzed cards. If you do, trash 1 installed Runner card
  with a printed install cost equal to or less than the printed rez cost
  of the Corp card you trashed" and World Tree's "you may trash 1 of your
  other installed cards to search your stack for 1 card of the same type"
  are `OfferPaidChoice`s of `Cost::Trash` whose `if_paid` reads the card
  the cost took. The accepted choice now puts the payment's trashed cards
  on the context (`ResolutionContext::paid_with`), read by
  `Amount::PaidCardPrintedCost` and `CardFilter::SameTypeAsPaidCard`.
  Composition didn't work: `PrintedCost` reads the acting card, which in
  an `if_paid` is the card whose text it is.
- **A filter read off the resolution.** `CardFilter::PrintedCostAtMost(
  Amount)` holds an amount, and a selection writes in the number it is as
  it is offered (`CardFilter::with_resolution`), as it writes in "this
  server" (`with_this_server`); `SameTypeAsPaidCard` becomes the type of
  the card paid. Unresolved, each matches nothing.
- **World Tree installs the card it found** out of the stack, paying
  3[credit] less (`InstallRunnerCardFromZone` from `OwnStack`, Muse's
  source); one it cannot afford stays in the shuffled stack (CR 1.16.4b).
- **Where a trashed install stood.** Yakov's "Whenever a player trashes a
  card (including this upgrade) from the root of this server or
  protecting it, except during installation, gain 2[credit]" needs three
  facts the card no longer says once it is in Archives, so
  `GameEvent::CardTrashed` carries them for a Corp install
  (`TrashedInstall { server, rezzed, installing }`), filled at every site
  that trashes one and marked `installing` at CR 8.5.16c's trash. An
  access trash reads them off the run. `EventFilter::TrashedFromThisServer`
  compares the server with the listener's, and a card hearing its own
  trash takes the one it left as "this server" — CR 4.6.6i, whose example
  is exactly this (Warroid Tracker). It names no player, so either
  player's trash is heard; Yakov is an `OnCardTrashed` and an
  `OnTrashedFromAccess`, the two kinds of trash.
- **A rezzed install trashed by a card's text hears its own trash.**
  `TrashedInstall::rezzed` makes the moment `was_active`, as an access
  trash already was — which closes Hostile Architecture's known limit
  (PH 2b): its "(including this asset)" now holds for the Runner's text
  too.
- **Client.** Nothing new in the view; the new field rides on the trash
  event both logs already word. The engine reading words the new amount
  and the server filter.
- **Decks.** Tag You're It took two Kimberlite Field for two of its three
  Orbital Superiority and two Yakov Erikovich Avdakov for its two Regolith
  Mining License; Safety Net two World Tree for its two Conduit. Both stay
  Eternal-only.
- **DSL ratio (`pool_status.py`):** unchanged at 15 of 91 `Effect`
  variants single-use, 1 unused, over 422 card files.
- **Measured** against Stage 5b's tip. Both sweeps green at 256 seeds,
  the card gate included.
  - A ref with the engine change and no cards is identical in all four
    shapes: Hostile Architecture, which now hears its own trash by text,
    is in no matchup deck.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view and index alike, of 192): Corp agenda wins
    59 → 63, Corp flatlines 27 → 28, Runner agenda wins 102 → 97.
- **Real play**, seed 2, Tag You're It against Safety Net (random 96 games
  / planner 48): Kimberlite Field scored 1 / 15 times and its offer heard
  1 / 8; Yakov rezzed 51 / 33 times and paid out 13 / 4 (12 / 4 of them
  on access trashes); World Tree installed 12 / 0 times and heard 54
  first successful runs (random).
- **Bot debts:** the planner Runner never installs World Tree; the
  planner Corp never trashed a rezzed card for Kimberlite Field.

#### Stage 5d — the stack and HQ: a search for different names, and a run named for the card whose text began it (1 October 2026)

`feat/ph-stage-5d-stack-and-hq`: Asmund Pudlat, Concerto, Reprise.
**No new `Effect`**; one `CardFilter`. Parhelion 45 of 63;
`PH_UNIMPLEMENTED` 21 → 18. **Stage 5 is complete.**

- **`CardFilter::DifferentNames`**, Asmund Pudlat's "search your stack
  for up to 2 virus or weapon cards **with different names**". Read off
  the selection in progress: a card is eligible while no *other* chosen
  card has its name, so a second copy cannot be toggled on and a chosen
  card can still be toggled off. Composition didn't work: `max` counts
  cards, and nothing read what was already chosen. Its hosted cards are
  Madani's faceup `HostedOnSource`, and "When your turn begins, you may
  add 1 hosted card to your grip. If there are no more hosted cards,
  trash this resource" is a selection followed by an `EffectIf` on the
  hosted zone.
- **A run is named for the card whose text began it** (CR 8.6.5).
  Concerto's "Reveal the top card of your stack and place credits equal
  to its printed play or install cost on this event. Add the revealed
  card to your grip. Run any server." offers its run as the reveal
  resolves, so the choice resolves *as the revealed card* — which is what
  lets `on_start`'s `PlaceRunCredits { amount: PrintedCost }` read its
  cost. The run took that card as `RunState::initiated_by`, which would
  have made a revealed event the run's active event; it is now the
  decision's `prompting_card`, the card whose text it is. Beta Build's run
  was the found program's, and is Beta Build's now; its riders still
  resolve as the program. An empty stack reveals nothing and still runs
  (`RunInProgress` guards the second branch).
- **Reprise composes**: "Play only if you stole an agenda this turn" is
  `TimesThisTurn(OnAgendaStolen)`; "Add 1 installed Corp card to HQ" is
  Hermes's selection; "You may run any server" a `PresentChoice`.
- **Client.** Nothing new in the view: the run's `initiated_by`, already
  drawn, now names the event.
- **Decks.** Hit List took two Concerto for its two Tailgate, two Reprise
  for its two Sell Out and two Asmund Pudlat for its two Verbal
  Plasticity, which finds its weapons (Jeitinho, Poison Vial, Laser
  Pointer) and its Conduit. It stays Eternal-only.
- **Approximation.** Concerto's credits are placed on the run as it
  starts, where the card places them on itself before; they pay for
  anything during that run, as the card says, and go with it.
- **DSL ratio (`pool_status.py`):** unchanged at 15 of 91 `Effect`
  variants single-use, 1 unused, over 425 card files.
- **Measured** against Stage 5c's tip. Both sweeps green at 256 seeds,
  the card gate included.
  - A ref with the engine change and no cards is identical in all four
    shapes: Beta Build, whose run is now its own, is in no matchup deck.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view and index alike, of 192): Corp agenda wins
    63 → 62, Corp flatlines 28 → 29.
- **Real play**, seed 2, Hit List against Retirement Package (random 96
  games / planner 48): Concerto played 28 / 26 times; Reprise 4 / 4;
  Asmund Pudlat installed 36 / 8 times and handed a hosted card over on
  70 / 7 turn starts.

#### Stage 6a — charge, and the first time each encounter (1 October 2026)

`feat/ph-stage-6a-charge`: Flux Capacitor, Orca. **No new `Effect`**; one
word in the trigger condition. Parhelion 47 of 63; `PH_UNIMPLEMENTED`
18 → 16. Stage 6 was split by mechanic when it was taken: 6b the mark,
6c a set-aside program and counters on a run event, 6d Nanuq.

- **Charge composes** (CR 10.10). "Charge 1 of your installed cards" is a
  selection of the Runner's installed cards with
  `CardFilter::HostsCounters(Power)`: a card whose kind is power and that
  hosts at least one, which is 10.10.1's "cards with no hosted power
  counters cannot be charged" — and the chosen card is the instruction's
  target (10.10.2). Its `then` places 1 counter on the card chosen, behind
  an `EffectIf(CardsSelected ≥ 1)`, since a `then` resolves as the parking
  card when nothing was chosen; "you may" is a minimum of 0. Middle Sun's
  charge cards (Captain Padma Isbister, Rigging Up, Daeg, Stoneship Chart
  Room) compose on the same selection.
- **`TriggeredEffect::first_each_encounter`**, Flux Capacitor's "the
  **first time** you break a subroutine during each encounter with host
  ice": `first_each_turn` over the encounter, judged in the listener scan
  against `EncounterTally::subroutines_broken`, a count kept as each break
  happens. Each `SubroutineBroken` is dispatched the moment its break is
  counted (`ability::break_pending`), so Cleaver breaking two of Brân's
  subroutines at once is heard as one first time and one second.
  Composition didn't work: `OncePerEncounter` is a use limit, which a card
  arriving mid-encounter would find unspent (the Turn History Rule's
  reason for `first_each_turn`), and a requirement reading the count is
  asked at resolution, after an ability breaking two has counted both.
  `validate` holds the word to `OnSubroutineBroken`, the one thing the
  encounter counts, never beside `first_each_turn` on one card (the two
  share the queued trigger's verdict, `DeferredTrigger::
  not_the_first_this_turn`) and never beside a `OncePerEncounter`.
- **Orca composes**: "the first time each turn this program fully breaks
  a piece of ice" is Abaasy's `ByThis` first time on the copy, and "break
  any number of sentry subroutines" is `BreakSubroutines { count: All }`.
- **Client.** Nothing new to draw: the charge is a card selection, worded
  by its trigger's printed clause in both clients, and the tally's count
  is the engine's (`view_ledger`: each break is already a line of the
  log).
- **Decks.** Safety Net took two Flux Capacitor for its two Stowaway,
  trojan for trojan, and two Orca, a killer beside its Living Mural, for
  its two Pressure Spike; both charge its Lampades, Coalescence, Lobisomem
  and AirbladeX. Street Gallery and Spare Parts still carry Stowaway, and
  Street Gallery Pressure Spike.
- **Approximation.** Orca's "any number" breaks every sentry subroutine
  pending, as "up to N" takes N.
- **The conformance ledger's count** in `ROADMAP.md` read 14 conform, 50
  read in part, 43 unreviewed, which earlier stages had left behind the
  table; with 10.10 now conforming it is 17, 49 and 42, counted off the
  table.
- **DSL ratio (`pool_status.py`):** unchanged at 15 of 91 `Effect`
  variants single-use, 1 unused, over 427 card files.
- **Measured** against Stage 5d (`32b5fb1`). Both sweeps green at 256
  seeds, the card gate included.
  - A ref with the engine change and no cards is identical in all four
    shapes.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view and index alike, of 192): Corp agenda
    wins 62 → 63, Runner agenda wins 97 → 96.
- **Real play**, seed 2, Safety Net against Retirement Package (random 96
  games / planner 48): Flux Capacitor installed 44 / 0 times and charged
  once; Orca installed 1 / 1 and charged once under the planner.

#### Stage 6b — the mark (1 October 2026)

`feat/ph-stage-6b-mark`: Tunnel Vision, Info Bounty. **One new `Effect`**
(`IdentifyMark`, which both cards use). Parhelion 49 of 63;
`PH_UNIMPLEMENTED` 16 → 14.

- **The mark is a lingering effect** (CR 10.11.4: "treated as a lingering
  effect that expires at the end of the turn"): `Lingering::Mark(server)`
  about the Runner until `Until::EndOfTurn`, so nothing resets it and a
  bot's sample copies it with the view. There is one, shared by every card
  (10.11.1a), asked with `lingering::mark`.
- **`Effect::IdentifyMark`**, "identify your mark" (10.11.2): nothing while
  there is a mark (10.11.3), otherwise a central server drawn from the
  state's RNG with one chance in three each (10.11.2a), announced as
  `GameEvent::MarkIdentified`. Composition didn't work: no effect chose a
  server at random, and the mark is a designation no effect wrote. Midnight
  Sun's mark cards (Nyusha "Sable" Sintashta, Carpe Diem, Virtuoso,
  Backstitching) use it too.
- **`EventFilter::Mark`**, Info Bounty's "a run **on your mark**", read off
  the state as the scan runs. Its first time each turn is counted on the
  mark's server class: `turn_log::first_time_on` writes the turn's mark in
  (`EventFilter::with_mark`) before a count is read, since the mark is a
  central server and each central has its own column. It replaced
  `first_time_of`, which read a card alone.
- **`EffectRequirement::EncounteringIceProtectingMark`**, Tunnel Vision's
  "break up to 2 subroutines on a piece of ice protecting your mark", read
  off where the encountered ice is installed rather than the run's server.
- **`EffectRequirement::BreachedLastRunsServer`** and
  `CompletedRun::breached`: Info Bounty's "if you breached that server
  during that run", the intervening if behind its first time — a run on the
  mark that Ice Wall ends is the turn's first and pays nothing. A
  successful run is not the same thing: a breach can be replaced by another
  server's.
- **10.11.5** ("only checks from the moment that server was designated")
  needs no word: every card in the pool identifies its mark as the turn
  begins, before anything is counted.
- **Client.** The mark is an In effect line in both clients ("Tunnel
  Vision: the Runner's mark is R&D, for the rest of this turn",
  `hud::in_effect`), and its identification a log line ("R&D became the
  Runner's mark for this turn", `actions`), each with a test there.
  `prose` words the effect and the filter.
- **Decks.** Hit List took two Tunnel Vision for two of its three
  Smartware Distributor and two Info Bounty for two of its three Red Team,
  job for job. It stays Eternal-only.
- **DSL ratio (`pool_status.py`):** 15 of 92 `Effect` variants
  single-use, 1 unused, over 429 card files.
- **Measured** against Stage 6a (`bee5405`). Both sweeps green at 256
  seeds, the card gate included. A ref with the engine change and no cards
  and the stage itself are both identical to main in all four shapes, the
  planner seatings included.
- **Real play**, seed 2, Hit List against Retirement Package (random 96
  games / planner 48): a mark identified 282 / 96 times (15 more asks
  found one already); Tunnel Vision installed 34 / 30 and used 17 / 54
  times; Info Bounty installed 31 / 0 and paid out 16 / 0 times.

#### Stage 6c — a set-aside program, and a subroutine resolving heard by a run event (1 October 2026)

`feat/ph-stage-6c-set-aside-and-run-event-counters`: Spark of
Inspiration, Raindrops Cut Stone. **No new `Effect`**; one `Trigger`.
Parhelion 51 of 63; `PH_UNIMPLEMENTED` 14 → 12.

- **Spark of Inspiration composes** on The Wizard's Chest's set-aside
  zone (RWR 6d): `SetAsideFromTopUntil` a program, a selection of it
  installable at `Discount::Credits(10)`, `InstallRunnerCardFromZone` out of
  the zone behind an `EffectIf(CardsSelected ≥ 1)`, and `ShuffleIntoDeck`
  the rest.
- **`Trigger::OnSubroutineResolved`**, Raindrops Cut Stone's "whenever a
  subroutine resolves during that run": `GameEvent::SubroutineFired`, which
  was an occurrence of nothing, is now a moment about the ice and is
  dispatched at both sites a subroutine resolves (`run::step_subroutine`
  and the unbroken subroutines' loop) **before its effect**, so "including
  a subroutine that ends the run" is heard while the run, and the event's
  place in the play area (CR 8.6.5), still stand. Appended to `Trigger`,
  so no turn-log row moved. Composition didn't work: no trigger heard a
  subroutine resolve.
- **The event's counters go with the run.** `CompletedRun` carries
  `initiated_by` and `event_counters`, and `Amount::HostedCounters` read by
  the event's own "when that run ends, draw 1 card for each hosted power
  counter" (`SetRunEndedEffect`, Hannah's shape) finds them there while no
  run is under way.
- **The sweeps' harness learned the set-aside zone.** The conservation
  check did not count `RunnerState::set_aside`, and the fog gate did not
  treat it or `CardsSetAside` as public (CR 4.8.6), so the first parked
  "you may install" out of it — Spark, seed 99 and then seed 162 of the
  view sweep, where The Wizard's Chest had never once been reached — read
  as a card lost and then as a card named that was concealed. Both were
  the test's gaps; the zone was in the view all along.
- **Client.** Nothing new to draw: the set-aside zone and the run event's
  counters were drawn by RWR 6d and 7e.
- **Decks.** Safety Net took two Spark of Inspiration for its two Beta
  Build, an event that installs a program from the stack for another
  (Spare Parts still carries Beta Build); Pay As You Go took two Raindrops
  Cut Stone for its two Sure Gamble.
- **Approximation.** Raindrops' counter is placed as the subroutine is
  announced, before its effect resolves. **Known limit:** a subroutine a
  card's text resolves (Nanisivik Grid's) is not announced and not counted.
- **DSL ratio (`pool_status.py`):** 14 of 92 `Effect` variants
  single-use, 1 unused, over 431 card files.
- **Measured** against Stage 6b (`f578d6b`). Both sweeps green at 256
  seeds, the card gate included.
  - A ref with the engine change and no cards is identical in all four
    shapes: the new dispatch moved nothing.
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize` (view and index alike, of 192): Corp agenda wins
    63 → 62, Corp flatlines 29 → 30.
- **Real play**, seed 2, against Retirement Package (random 96 games /
  planner 48): Safety Net played Spark of Inspiration 14 / 0 times; Pay
  As You Go played Raindrops Cut Stone 23 / 0 times, and a subroutine
  resolving placed a counter 12 times.

#### Stage 6d — a Runner card removed from the game as it leaves the table (1 October 2026)

`feat/ph-stage-6d-nanuq`: Nanuq. **No new `Effect`**, and no new word.
Parhelion 52 of 63; `PH_UNIMPLEMENTED` 12 → 11. **Stage 6 is complete.**

- **"When this program is uninstalled, remove it from the game"** is heard
  as the program's trash from the table (`OnCardTrashed`, `subject: This`,
  `TrashedFrom::Installed`), which the subject hears from the heap — the
  one way a program leaves the rig in the pool but by its own removal.
  `Effect::RemoveFromGame(ThisCard)` now also takes a Runner card from the
  heap when it is reacting to its own trash from the table, keyed on the
  triggering `CardTrashed` rather than on the rig, where a second copy by
  the same name may still be installed (a test holds two copies).
- **"When an agenda is scored or stolen, remove this program from the
  game"** composes: `OnAgendaScored` and `OnAgendaStolen`, Vera Ivanovna
  Shuyskaya's pair, each `RemoveFromGame(ThisCard)` out of the rig.
- **Known limit:** an uninstall to the grip or the stack would not remove
  it; no card in the pool does that to a program. The Luana Campos line
  that said Nanuq would be the first Runner interrupt of its own
  uninstalling is corrected: its words are a trigger, not an interrupt.
- **Client.** Nothing new to draw: the removed-from-game pile is drawn
  (VP, #250).
- **Decks.** Street Gallery took two Nanuq for its two Conduit; eight
  other decks still carry Conduit.
- **#331's commit message is empty** on `main` (`17a601d`, subject
  " (#331)"): the squash merge was given a message file from the wrong
  directory. Its message is the pull request's description, and its
  record is Stage 6c above.
- **DSL ratio (`pool_status.py`):** 14 of 92 `Effect` variants
  single-use, 1 unused, over 432 card files.
- **Measured** against Stage 6c (`d7a49fc`). Both sweeps green at 256
  seeds, the card gate included. A ref with the engine change and no card
  is identical in all four shapes; with Nanuq the random seatings are
  identical and the planner ones move by `determinize` with no game's end
  changed (steps 110,845 → 111,049 of 192 games).
- **Real play**, seed 2, Street Gallery against Retirement Package (random
  96 games / planner 48): Nanuq installed 9 / 34 times and used 0 / 23,
  removed from the game after its trash from the table 6 / 9 times, a
  steal 1 / 19 and a score 0 / 1.

#### Stage 7 — winning and the score area: a target a card lowers, a card that wins when it is empty, an accessed card moved by its own text, and copies turned facedown (2 October 2026)

`claude/serene-einstein-6bhlig`: Issuaq Adaptics: Sustaining Diversity,
Superdeep Borehole, Nightmare Archive, Matryoshka. **One new `Effect`**
(`TurnHostedFaceup`), one `Cost`, one `ContinuousKind`, one `CardFilter`.
Parhelion 49 of 63; `PH_UNIMPLEMENTED` 18 → 14. Taken ahead of Stage 6,
at the person's request: nothing here needs charge, mark or set aside.
Not split: four cards, each its own mechanic, measured once.

- **`ContinuousKind::AgendaPointsToWin(Number)`**, Issuaq Adaptics's "For
  each hosted power counter, you need 1 less agenda point to win the
  game" (`Scope::Controller`, `{ per: -1, of: HostedCounters }`).
  `continuous::points_to_win` is the match's threshold
  (`MatchRules::winning_agenda_points`) plus what the side's active cards
  add, and the win check asks it of each side at every checkpoint (CR
  1.7.2a, 10.3.1c). The view carries it per side
  (`CorpClientView::points_to_win`, `RunnerClientView::points_to_win`),
  so every "to win" a client prints is the engine's number
  (`hud::points_to_win`: the readout's "3/5", the score sheet's caption,
  the terminal's status line). The bots' stage
  (`netrunner_bots::eval::stage`) still reads the match rule: a reading
  with no registry, left as it was.
- **"An agenda that you did not install or advance this turn"** is
  `when: Card(All[NotInstalledThisTurn, NotAdvancedThisTurn])` on
  `OnAgendaScored`. `CardFilter::NotAdvancedThisTurn` reads the copy's own
  record (`InstalledCard::this_turn`'s `OnAdvance` count), kept on the
  scored copy as it leaves the table (`ScoredAgenda::advanced_on_scoring_turn`,
  beside `installed_on_scoring_turn`). A counter Seamless Launch places is
  not advancing (CR 1.18.2), which is the way the deck scores the Issuaq way.
- **Superdeep Borehole composes** in Juli Moreira Lee's shape (CR 10.9):
  "load 6 bad publicity counters" an `OnRez` `AddCounters`
  (`counter_kind: BadPublicity`, Luana Campos's), "take 1 bad publicity
  from this asset" a turn-start `RemoveCounters` and `GiveBadPublicity`,
  and "When it is empty, you win the game" an `EffectIf(ThisCardCountersAtMost(0),
  WinTheGame)` after the take — nothing else in the pool removes a counter
  from it. Hosted bad publicity is not "on" the Corp (CR 1.13.3, whose
  example is this card): the Runner's fund never counts it.
- **An accessed card moved by its own text** (CR 7.1.7). Nightmare
  Archive's "they may add it to their score area as an agenda worth -1
  agenda point. If they do not, do 1 core damage and remove this asset
  from the game" is a Runner's `PresentChoice` out of its `OnAccessed`
  between `AddToScoreAreaAsAgenda { points: -1 }` and core damage then
  `RemoveFromGame(ThisCard)`. Both effects, resolving as the card being
  accessed (`run::is_being_accessed`), go through
  `run::move_currently_accessed_card`: the card leaves whatever zone it
  was accessed in, lands in the Runner's score area or out of the game,
  and its access ends — at the access decision by moving the breach on,
  and before it by `enter_pending_choice` seeing it gone (`left_its_place`,
  which was `was_trashed`). Cupellation's and Heliamphora's host is the
  third destination of the same function. "Their score area" is the
  Runner's because only the Runner accesses.
- **Matryoshka's copies, hosted and turned facedown** (CR 1.13.7c–d).
  "[click]: Host a copy of Matryoshka from your grip faceup on this
  program" is Madani's selection with `AmongCards(["matryoshka"])`; the
  break is `AllOf[CreditsX { max: EncounteredIceSubroutines },
  TurnHostedFacedown]` (Lobisomem's X); "When your turn begins, turn each
  hosted card faceup" is `Effect::TurnHostedFaceup`. Which copies are
  facedown is nobody's to ask — they are copies of the host, an unordered
  group — so the state keeps a count beside the hosted cards
  (`InstalledRunnerCard::turned_facedown`, `faceup_hosted()`), public
  because each was faceup when hosted. It is not `hosts_facedown`, Read-
  Write Share's, whose cards were never faceup. "Limit 6 per deck" is the
  catalog's `deck_limit`.
- **Client.** Two new view fields, both drawn: `points_to_win` (above),
  and `PublicInstalledRunnerCard::turned_facedown`, the rig chip
  `board::rig::hosted_chip` ("3 hosted, 1 facedown") on both clients' rig
  lines — the terminal's showed no hosted cards before — and a line of
  the card's facts. Issuaq's power counters are AU Co.'s
  `identity_counters`, already drawn. Nightmare Archive is a trap to
  `board::rez::gains_nothing`, which `exactly_the_traps_gain_nothing_by_a_rez`
  now lists.
- **Decks.** **Permafrost**, a new Sweep deck on Issuaq Adaptics: A
  Thousand Cuts' frame with two Seamless Launch for its two Mindscaping.
  Hostile Bid took two Superdeep Borehole for its two Business as Usual;
  Undertow two Nightmare Archive for its two Working Prototype; Hit List
  four Matryoshka for its two Carmen and two Smartware Distributor.
- **DSL ratio (`pool_status.py`):** 15 of 92 `Effect` variants
  single-use, 1 unused, over 429 card files.
- **Measured** against the branch's previous tip (`99c46dd`). Both sweeps
  green at 256 seeds, the card gate included.
  - With the four card files taken out of the tree, the engine change is
    identical in all four `coverage_identical.py` shapes: no sample deck
    holds a card the new words reach. (Marking a card `is_playable:
    false` is not the way to take it out: `register_playable_cards`
    registers every card file, so `determinize` still draws it.)
  - With the cards, both random seatings are identical; the planner ones
    move by `determinize`, whose prior now holds four more cards (view
    and index alike, of 192): Corp agenda wins 62 → 56, Corp flatlines
    29 → 30, Runner agenda wins 97 → 98, Runner deck-outs 4 → 8.
- **Real play**, seed 2, each new Corp deck against Hit List (random 96
  games / planner 48):
  - Permafrost: Issuaq Adaptics placed a counter once in random play and
    never for the planner, which scores an agenda the turn it reaches
    its requirement.
  - Hostile Bid: Superdeep Borehole installed 94 / 56 times, rezzed 5 / 4
    and gave up a bad publicity on 12 / 0 turn starts; no game ran long
    enough for it to empty.
  - Undertow: Nightmare Archive accessed 59 / 31 times, and its choice
    fired on every access (`OnAccessed` 59 / 31). The report does not
    split the Runner's answers: Jeitinho, in Hit List, is added to a
    score area as an agenda too.
  - Hit List: Matryoshka installed 135 / 119 times over the three
    pairings and turned faceup on 605 / 754 turn starts. Random play
    hosted a copy 28 times; the planner never did, so its Matryoshka
    breaks with nothing — a bot blindness, not a rule, left for the bots'
    record.

  **Landed after Stage 6, not ahead of it** (2 October 2026). Stage 6
  merged to `main` while this stage was being built, so the commit was
  rebased onto it before landing, and three numbers above are its own
  base's rather than `main`'s. Parhelion is 56 of 63, not 49, and
  `PH_UNIMPLEMENTED` went 11 -> 7. Stage 6b had already traded two of Hit
  List's three Smartware Distributor for Tunnel Vision, so Matryoshka
  took only its two Carmen and Hit List is 48 cards, not 46. The DSL
  ratio is the merged tree's, re-measured on the area page.

#### Stage 8 — losing abilities and break restrictions: a host that keeps only its printed subroutines, a resource without its abilities, limits on breaking for a duration and on one Runner card, a server chosen for the turn, and an ability one card gives another (2 October 2026)

`claude/serene-einstein-6bhlig`: Hush, Klevetnik, Anvil, Unsmiling
Tsarevna, Hafrún, Tsakhia "Bankhar" Gantulga, ZATO City Grid. **Four new
`Effect`s** (`LoseAbilities`, `LimitBreaks`, `ChooseServer`,
`ReplaceSubroutines`), two `ContinuousKind`s, four `Lingering` kinds, one
`Prohibition`, one `EffectDuration`, two `CardFilter`s and one word on a
trigger (`granted`). Parhelion 63 of 63; `PH_UNIMPLEMENTED` 7 → 0.
**Stage 8 is complete, and Parhelion with it.** Not split: the seven
share one question — what a card can still do — and were measured once.

- **Losing abilities** (CR 9.1.9a: a lost ability "is completely
  ignored"). Hush's "Host ice cannot gain abilities and loses all
  abilities except its printed subroutines" is two `ContinuousKind`s about
  its `Host` (`LosesAbilities`, `CannotGainAbilities`); Klevetnik's "That
  resource loses all abilities until your next turn ends" is
  `Effect::LoseAbilities` on the chosen resource, a lingering
  `Lingering::LosesAbilities` until `EffectDuration::ThroughYourNextTurn`
  (the Corp's next turn, which turns alternating makes the next turn or
  the one after). **One question,** `rules::active::lost_abilities`, put
  by everything that reads what a card can do: the listener scan (a card
  that has lost its abilities hears nothing), the continuous scan (none of
  its standing effects applies, its own text included), a paid ability
  (`RulesError::AbilitiesLost`) and the window that would open for one, an
  interrupt (`prevention::could_prevent`), hosted credits
  (`payment::sources`), a recurring-credit refill, and a bioroid's
  click-break, which is the ice's own ability. It is read off the rig and
  the lingering list directly — never through the scan it gates — which is
  CR 9.12.1e's order for hosted objects and all the pool needs; 9.12.1d's
  dependency order is not built. A printed subroutine is never lost: the
  one card that takes ice's abilities keeps them.
  `Until::EndOfTurn(n)` now holds through turn n (`turn <= n`) rather than
  during it, which reads the same for every entry made before, each made
  in the turn it names.
- **Cannot gain abilities.** A card Hush is on gains no subroutine
  (`Effect::GainSubroutine`, and a run's `gained_for_the_run` at each
  encounter) and has no ability another card grants it.
- **An ability one card gives another** (CR 9.1.3b: its source is the
  card that has it). ZATO City Grid's "Each piece of ice protecting this
  server gains "When the Runner encounters this ice, …"" is ZATO's
  trigger, heard while ZATO is active, marked `TriggeredEffect::granted`:
  it acts on the ice (`acts_on_subject`, which `validate` requires beside
  it) and is not heard for ice that cannot gain abilities or has lost them
  (`active::may_have_granted`). "Protecting this server" is
  `CardFilter::InThisServer` in an `EventFilter::Card` — the listener scan
  now writes the listener's server into a `when`'s card filter, and
  `copy_matches` reads `InServer` — so no new `EventFilter`. "Choose 1
  subroutine on it. You may trash this ice to resolve that subroutine" is
  an `OfferPaidChoice` costing `TrashSelf`, as the ice, whose `if_paid` is
  Mycoweb's `ResolveSubroutineOfSelectedIce`.
- **Break restrictions with a duration and on one card** (CR 9.8.5).
  Anvil's "the Runner cannot break this ice's printed subroutines for the
  remainder of this encounter" (0, `Encounter`) and Unsmiling Tsarevna's
  "during each encounter with this ice for the remainder of that run, the
  Runner cannot break more than 1 of its printed subroutines" (1, `Run`)
  are `Effect::LimitBreaks`, Hammer's `BreakLimit` made by an ability that
  resolved (`Lingering::BreakLimit`, read by `continuous::breaks_left`
  beside the ice's own, against the same count). Hafrún's "choose 1
  installed Runner card. That card's abilities cannot break subroutines
  for the remainder of that run" is `Prohibit { BreakSubroutines,
  this_install }` on the card the selection chose
  (`Prohibition::BreakSubroutines`), asked by both break effects of the
  breaking install (`ability::breakable_now`), which then find nothing to
  break, so the ability is not offered. "You may trash 1 of your other
  installed cards. If you do" and "trash 1 card from HQ" are nested costs
  (CR 1.16.11a), `Cost::Trash`, as Kimberlite Field's.
- **Two ice types.** Hafrún is a "Barrier - Code Gate". `CardType::Ice`
  holds one type and the catalog's subtypes hold both
  (`CardDefinition::is_ice_of_type`), which `continuous::ice_gains_subtype`
  reads beside the types gained — every reader asks "the type it carries,
  or this" — and `CardFilter::IceOfType` reads too, so a decoder breaks it
  and a pass of "a rezzed code gate" sees it. A list on `CardType::Ice`
  would have touched every ice file for one card.
- **A server chosen for the turn, and a subroutine replaced.** Tsakhia's
  "When your turn begins, you may choose a server" is `Effect::ChooseServer`
  in a `PresentChoice`: a `PendingDecision::ChooseServer` that remembers
  rather than runs (`remember`), its answer a `Lingering::ChosenServer`
  that is the card's for the turn. "During the first encounter each turn
  with a piece of ice protecting the chosen server, whenever the Corp would
  resolve a subroutine, instead they resolve "[subroutine] Do 1 net
  damage."" is an `OnEncounter` trigger whose `when` is
  `Card(All[Ice, InChosenServer])` (written over with the card's choice by
  the listener scan) and whose `Effect::ReplaceSubroutines` makes
  `Lingering::SubroutinesReplaced` for the encounter and spends the choice
  — which is what makes it the first encounter. The resource prints the
  subroutine (`subroutines`), and `run::transition_subroutine`, the one
  place both sites resolve a subroutine, resolves that instead.
- **Client**, both clients through `netrunner_client`: an install's facts
  say it "Has lost all its abilities but its printed subroutines (Hush)"
  or "Has lost all its abilities (Klevetnik)" (`facts::
  lost_abilities_words`); the In effect list words each new lingering
  kind (Hafrún's "Cleaver's abilities cannot break subroutines", Anvil's
  and Tsarevna's limits, Tsakhia's chosen server and replaced
  subroutines) and an end of turn that is not this one ("until the end of
  the Corp's next turn"); the server choice reads "choose a server" and
  its buttons "Choose HQ" rather than a run; `prose` words every new
  effect and kind. One new decision field, `ChooseServer::remember`, in
  the view ledger as drawn. No new view field, event or action.
- **Decks.** Permafrost two Hafrún for two Lionsmane; Paid Content two
  Klevetnik for two Seraph and two Unsmiling Tsarevna for two Piranhas;
  Hostile Bid two Anvil for two Hammer and two ZATO City Grid for two
  Shackleton Grid; Pay as You Go two Hush for two Mayfly and two Tsakhia
  for the Leech and the Fermenter. Each card that left is still in
  another deck.
- **DSL ratio (`pool_status.py`):** 17 of 97 `Effect` variants
  single-use, 1 unused (`Trace`), over 443 card files — three single-use
  variants for seven cards (`LimitBreaks` is two cards'; Tsakhia's two
  and Klevetnik's one are each a mechanic's first card).
- **Found by the debug audit, and fixed:** a cost that trashes Luana
  Campos dispatched her "would be uninstalled" interrupt, and everything it
  did, twice — once at `uninstall::corp_install`, which must announce it
  while she is still on the table, and again in the payer's
  `ability::dispatch_cost_events`. Unreachable until a cost trash met her
  in a deck (Anvil's, in Hostile Bid); the debug schedule of
  `every_sample_deck_matchup_finishes` caught it inside a planner's
  payment replay. The payer now skips each announcement, from its
  `AboutToBeUninstalled` to the event saying that card left.
- **Measured.** Both sweeps green at 256 seeds (`netrunner_session`,
  `netrunner_single_player`, `--release`), the card gate included, so
  every one of the seven was seen in play.
  - The engine change with none of the seven cards in the tree (its own
    ref, the decks as they were) is **identical in all four
    `coverage_identical.py` shapes** against `main` (`62294da`): random
    and planner, by view and by index. Hafrún's second type read off the
    printed subtypes, `Until::EndOfTurn` holding through its turn, and
    the listener scan writing a server into a `when` reach no card in a
    sample deck.
  - With the cards and the decks, both random seatings are identical — no
    sample deck holds one, and Sweep decks are not in `matchups()` — and
    the planner seatings move by `determinize`, whose prior now holds
    seven more cards (view and index alike, of 192): Corp agenda wins 56
    → 56, Corp flatlines 30 → 31, Runner agenda wins 98 → 98, Runner
    deck-outs 8 → 7.


### 5. Midnight Sun and its Booster Pack — 65 cards (C 22 / V 26 / M 17)

#### Stage 1 — eleven Corp cards, composed (2 October 2026)

`claude/serene-einstein-6bhlig`: Anemone, Élivágar Bifurcation, Refuge
Campaign, Bladderwort, Artificial Cryptocrash, Ubiquitous Vig, Vasilisa,
Pravdivost Consulting: Political Solutions, Svyatogor Excavator,
Maskirovka and Extract. **No new `Effect`, and no change to the engine or
the clients:** card files, decks and tests only. Midnight Sun 11 of 65,
its booster pack 1 of 7; `MS_UNIMPLEMENTED` 65 → 54 and
`MSBP_UNIMPLEMENTED` 7 → 6. The survey's "composes" held for all eleven.

- **What each is made of.** Artificial Cryptocrash is Hostile Takeover's
  score trigger with Magistrate Revontulet's `LoseCredits`. Refuge Campaign
  is PAD Campaign at 2[credit]. Bladderwort is PAD Campaign with an
  `EffectIf` after the gain, `Not(AmountAtLeast(Credits(Corp), 5))`, so the
  "4[credit] or less" is read after the credit arrives, as the card orders
  it; a trigger `requirement` would have read it before. Élivágar
  Bifurcation is a `PromptChooseCards` over rezzed installs into
  `DerezCard`, Warm Reception's derez. Extract is Hedge Fund's gain and
  Kimberlite Field's `OfferPaidChoice` with a `Cost::Trash` of an install.
  Svyatogor Excavator is Anvil's "1 of your other installed cards" as a
  cost (`NotSourceCard`) on Clearinghouse's turn-start offer. Ubiquitous
  Vig is Idiosyncresis: `advancement_requirement: 0` and
  `GainCreditsAmount` of `HostedAdvancementTokens`. Maskirovka is two
  subroutines. Vasilisa is Anvil's encounter offer paid in credits, as Mr.
  Hendrik's, into Syailendra's `Advanceable` prompt and
  `PlaceAdvancementCounters`, which is placing, not advancing (CR
  1.18.2). Anemone is Hafrún's rez trigger
  (`RezzedDuringRunAgainstThisServer`, a card from HQ as the cost) with
  `DealDamage` for its payoff. Pravdivost Consulting hears
  `OnSuccessfulRun` with `subject: Any` and `first_each_turn`, NGA's and
  World Tree's word, from the Corp's side as Méliès U does.
- **A "may" over a selection is guarded.** Élivágar's and Pravdivost's
  prompts take `min: 0`, and confirming with nothing chosen ran the `then`
  with no card: `UnresolvedCardTarget` and `CardNotInstalled`, so the legal
  action probe never offered declining. Their `then` is wrapped in
  `EffectIf(AmountAtLeast(CardsSelected, 1))`, as Warm Reception's is.
  Syailendra and Key Performance Indicators carry the unguarded shape and
  are left for a change of their own, since this stage changes no card it
  did not build.
- **Anemone is the pool's first NSG reprint built:** the booster pack's
  32005 and Midnight Sun's 33043. Both lists drop it, and `built_from` is
  the earlier printing, 32005.
- **Fidelity limits:** none. Each card's clauses map one to one onto
  existing words.
- **Client.** Nothing added to the view, the log or a decision, so no
  ledger line and no ledger row.
- **Decks.** Spin Cycle, a new Sweep deck on Pravdivost Consulting, is
  Paid Content with its identity swapped, two Artificial Cryptocrash for
  two Kingmaking, two Ubiquitous Vig for its two Balanced Coverage and two
  Vasilisa for its two Capacitor. Without NBN: Making News it is
  Standard-legal, so it is pinned `not_startup`. Hostile Bid takes two
  Maskirovka for two Ice Wall and two Svyatogor Excavator for its two
  Luana Campos. Deterrence takes two Extract for its two Caveat Emptor, in
  faction where they were Haas-Bioroid's, and keeps its pin: Extract is
  Standard-legal, Svyatogor is not, and Svyatogor went to Hostile Bid,
  which is Eternal-only already. Myoshu was the first choice for Extract's
  slot and was rejected: it scores itself as two agenda points.
  Retirement Package takes two Élivágar Bifurcation for its two Stegodon
  MK IV, point for point, and two Refuge Campaign for its two
  Synchrocyclotron. A Thousand Cuts takes two Anemone for its two
  Phoneutria and two Bladderwort for its two Esca. Every card given up is
  still in another Sweep deck.
- **DSL ratio** (`pool_status.py`): unchanged at 17 of 97 `Effect`
  variants single-use, 1 unused (`Trace`), now over 454 card files.
- **Real play** (`--headless`, each edited deck against Safety Net, seed
  2; random seats 96 games, planner seats 48; each pair below is random /
  planner). Pravdivost Consulting's
  trigger fired 837 / 571 times and asked 617 / 441 times. Ubiquitous Vig
  paid at 249 / 701 turn starts. Vasilisa's encounter offer fired 64 / 133
  times, and her subroutine 73 / 120. Artificial Cryptocrash was scored 1
  / 20 times. Svyatogor Excavator's offer came at 223 / 379 turn starts.
  Maskirovka's subroutines fired 96 / 288 times. Extract was played 27 /
  48 times. Élivágar Bifurcation was scored 12 / 25 times, asking each
  time. Refuge Campaign paid at 227 / 133 turn starts. Anemone was rezzed
  on a run at its server 20 / 24 times. Bladderwort's turn start fired 63
  / 31 times. No card is a bot debt: the planner plays all eleven.
- **A planner test was pinned to one sample.**
  `a_kill_corp_plays_public_trail_because_the_runners_answer_is_priced`
  asserted that a balanced Corp does not plan Public Trail on agent seed
  1. Eleven more cards in the pool moved that seed's sample, and the
  balanced Corp tagged. Over twenty seeds, `main` before the stage already
  had the balanced Corp tag on 7 of them, and 6 after; the kill Corp
  tagged on all 20, and against a full grip on none, both before and
  after. Withholding any one of eight of the new card files flipped seed 1
  back. The test now counts the twenty seeds: the kill Corp on every one,
  against a full grip on none, the balanced Corp on fewer than half. Its
  doc comment says what the claim was and why one seed could only witness
  it. The deck builder's set test, which named Midnight Sun as the set
  with nothing built, names Uprising now, and checks that Midnight Sun is
  offered to the Corp and not yet the Runner.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included, so every new card was seen in play. `coverage_identical.py`
  against `origin/main` (7ea5faf, 192 games a report): random identical,
  view and index alike, because the sample matchups hold no Sweep deck.
  The planner seatings move by `determinize`, whose prior now holds eleven
  more cards (view and index alike, of 192): Corp agenda wins 56 → 57,
  Corp flatlines 31 → 32, Runner agenda wins 98 → 99, Runner deck-outs 7 →
  4.

#### Stage 2 — eleven Runner cards, composed (2 October 2026)

`claude/serene-einstein-6bhlig`: Revolver, Chastushka, Running Hot,
Marrow, Avgustina Ivanovskaya, PAN-Weave, No Free Lunch, Endurance,
Hyperbaric, Propeller and Environmental Testing. **No new `Effect`.**
Midnight Sun 22 of 65, its booster pack 2 of 7; `MS_UNIMPLEMENTED` 54 →
43 and `MSBP_UNIMPLEMENTED` 6 → 5. The survey's "composes" held for nine
cards; two needed an existing word widened, each for every card.

- **What each is made of.** Chastushka is Account Siphon's
  `SetAccessReplacement` on HQ, not optional, with Cacophony's
  `Sabotage(4)`. Running Hot is Finality's `additional_play_cost` of 1 core
  damage (`SufferDamage(Brain, 1)`, a cost and so never prevented) and
  `GainClicks`. Marrow is T400 Memory Diamond's `Memory` and `HandSize`,
  Zenit Chip's core damage on install and Pantograph's `OnAgendaScored`
  with `Sabotage(1)`; it is a console by its catalog subtype, so the
  checkpoint trashes the older of two (CR 3.8.5b). PAN-Weave is WAKE
  Implant's meat damage on install and Transfer of Wealth's
  `LoseCredits` then `GainCreditsAmount(CreditsLostThisResolution)` — "if
  they do" is what was lost — on NGA's first successful run each turn,
  narrowed to HQ. No Free Lunch is two `TrashSelf` abilities, Friend of a
  Friend's shape; nothing withholds the tag removal at no tags, which the
  rules allow. Endurance is Poison Vial: a counter cost into
  `BreakSubroutinesUnconditionally`, since a break printed on a card that
  is not an icebreaker is no interface ability and contests nothing (CR
  3.9.5e), with Nga's first successful run each turn placing a counter.
  Hyperbaric is Rising Tide's `Strength` over Hippocampic Mechanocytes'
  `HostedCounters`, Gordian
  Blade's interface and a 2[credit] ability that places a counter in any
  paid ability window. Propeller is Orca's pump with a counter for its
  cost. Environmental Testing is Cookbook's `OnCardInstalled` over
  `CardTypeOneOf([Program, Hardware])`, with Side Hustle's `EffectIf` on its
  own count.
- **Avgustina Ivanovskaya: a virus program is a column of the turn log.**
  "The first time each turn you install a virus program" is
  `first_each_turn` over `when: Card(All([Program, HasSubtype(Virus)]))`,
  and `validate` refused it: the log counts a card by its type, and a
  subtype only where a card needs it (double, mandate). `Kind::VirusProgram`
  is the third, `Kind::of_card` counts a virus program there, and a filter
  on programs reads both columns (`Kind::all_of`), so nothing that counts
  programs moved. The Runner installs faceup, so the column is as public as
  any program's.
- **Revolver: a breaker its own cost trashed still contests.** "Interface
  → [trash] or hosted power counter: Break 1 sentry subroutine" is two
  abilities, one per cost. The `TrashSelf` one found no breaker in the rig
  by the time `BreakSubroutines` asked for its strength, so the probe never
  offered it. `LastKnown`, which the payer takes before the cost, now holds
  a rig card's strength, and the break reads it when the install has left:
  CR 3.9.5g asks as the ability is used, the paid ability is independent of
  its source (9.5.4), and the source is remembered as it last was
  (1.12.6). A pump bought this encounter is in that strength.
- **Revolver is a booster reprint** (32002, 33018): both lists drop it and
  `built_from` is the earlier printing.
- **Fidelity limits:** Environmental Testing's "when there are 4 or more
  hosted power counters" is asked as its own trigger places a counter; a
  charge (Orca, Flux Capacitor) that brings it to 4 waits for its next
  program or hardware install, because nothing hears counters being placed.
  On the known-limits list.
- **Client.** Nothing added to the view, the log or a decision, so no
  ledger line and no ledger row. The deck builder's set test, which said
  Midnight Sun was the Corp's only, now has it offered to both sides.
- **Decks.** Pay As You Go (Noise) takes two Chastushka for its two
  Hackerspace, two Running Hot for its two Stick and Poke, and two Marrow,
  a console beside its The Toolbox, for its Alarm Clock and Buzzsaw.
  Grassroots (Sebastião) takes two Avgustina Ivanovskaya, for its six
  virus programs, for its Wildcat Strike and Charm Offensive. Safety Net
  (Kate) takes two Endurance for its two AirbladeX, two Hyperbaric for two
  Umbrella, two Propeller for two Corsair, and two Environmental Testing
  for two Urban Art Vernissage. Hit List (Gabriel) takes two Revolver for
  its two Word on the Street, two PAN-Weave for two Borrowed Goods and two
  No Free Lunch for two Mutual Favor. Every card given up is still in
  another deck.
- **DSL ratio** (`pool_status.py`): unchanged at 17 of 97 `Effect`
  variants single-use, 1 unused (`Trace`), now over 465 card files.
- **Real play** (`--headless`, each edited deck against Hostile Bid, seed
  2; random seats 96 games, planner seats 48; each pair is random /
  planner). Chastushka played 24 / 0 times; Running Hot 29 / 32; Marrow
  installed 23 / 12, its sabotage on a Corp score 4 / 25 times; Avgustina
  installed 32 / 4, sabotaging 12 / 1 times; Endurance installed 2 / 0,
  its break never used; Hyperbaric installed 34 / 21 and used 105 / 91
  times; Propeller installed 61 / 29 and used 42 / 82 times; Environmental
  Testing installed 24 / 0, counting 41 installs; Revolver installed 42 /
  14 and used 13 / 25 times; PAN-Weave installed 26 / 0, skimming 27 times;
  No Free Lunch used 45 / 25 times. **Bot debts:** the planner never plays
  Chastushka, Endurance, Environmental Testing or PAN-Weave in 48 games;
  on the bot-debts list.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included, so every new card was seen in play. `coverage_identical.py`
  against Stage 1 (d7fafdd, 192 games a report): random identical, view
  and index alike — the sample matchups hold no Sweep deck, and the two
  widened words change nothing there. The planner seatings move by
  `determinize`, whose prior holds eleven more cards (view and index
  alike): Corp agenda wins 57 → 59, Corp flatlines 32 → 31, Runner agenda
  wins 99 → 98.

#### Stage 3 — charge and core damage (2 October 2026)

`claude/serene-einstein-6bhlig`: Captain Padma Isbister: Intrepid
Explorer, Rigging Up, Daeg, First Net-Cat, Stoneship Chart Room, Into the
Depths, Esâ Afontov: Eco-Insurrectionist, Begemot, Ghosttongue, The
Twinning and Cezve. **No new `Effect`.** Midnight Sun 32 of 65;
`MS_UNIMPLEMENTED` 43 → 33. Six compose; four needed a word each, every
one in the vocabulary of *when* or *what is read* rather than of what an
effect does.

- **Charge composes on Parhelion's rule.** Padma (the first time each
  turn a run on R&D begins: `OnRunStart`, `when: Server([RnD])`,
  `first_each_turn`), Daeg (`OnAgendaScored` and `OnAgendaStolen`, one
  printed sentence, two entries), Stoneship Chart Room (two `TrashSelf`
  abilities) and Into the Depths' third option are Flux Capacitor's
  selection over `HostsCounters(Power)`. With nothing to charge, Stoneship
  is trashed and nothing is asked.
- **Rigging Up: "charge that card" (CR 10.10.3).** The install from the
  grip at 3[credit] less is Illumination's
  `InstallRunnerCardFromGripWithDiscount`, whose resolution goes on as the
  card it installed; "you may charge that card if able" is an `EffectIf`
  on that install's own counters and kind. The install is an instruction
  of its own, so a checkpoint follows it (CR 10.3.5) and Propeller's
  "when you install" places its 4 counters before the charge makes 5 —
  the engine already resolved it that way, and the test holds it.
- **Begemot and Ghosttongue compose.** Begemot is Marrow's core damage on
  install, Rising Tide's `Strength` over Ontological Dependence's
  `CoreDamageTaken`, and Orca's "any number" for barriers. Ghosttongue is
  the core damage and Tailgate's `PlayCost`, scoped to every event.
- **Esâ Afontov: damage suffered is the Runner's moment.**
  `Trigger::OnDamageSuffered`, from `GameEvent::DamageTaken`, about the
  kind of damage (`when: Damage(Brain)`) and heard by the Runner whoever
  was responsible, a cost's damage included (CR 10.4.1). Composition
  didn't work: `OnDamageDealt` is the responsible player's and
  `OnDamageAboutToResolve` a "would". "You may draw 1 card and sabotage 2"
  is one `PresentChoice`.
- **The Twinning: a spend off an installed card is a moment.**
  `Trigger::OnCreditsSpentFromInstalledCard`, read off
  `CreditsSpentFromOutsidePool`'s new `from_installed`, the Runner's in a
  run or out of one, once per payment. `OnCreditsSpentOutsidePool` is
  about the run's server and counts bad publicity's and a run event's
  credits. Out of a run the event had been nobody's, and the debug
  audit named five payers that never dispatched it — the click installs
  of a program, a trojan, hardware and a resource, and the basic action
  to remove a tag; each now dispatches its payment after the effect, as
  a text install already did. The Corp's spends stay an occurrence of
  nothing. Its breach half is WAKE Implant's `ChooseNumber` over the
  hosted counters, once for HQ and once for R&D.
- **Into the Depths: the run counts the ice it passes.**
  `RunState::ice_passed`, counted where `IcePassed` is announced (an
  unrezzed or bypassed piece of ice included, a forced encounter's end
  not), read by `Amount::IcePassedThisRun` and carried in the view for a
  sample. "For each time you passed ice, resolve 1 of the following that
  you have not yet resolved" is `ResolveSomeOf` with a count of 1, 2 or 3
  behind three exclusive `EffectIf`s. Its search widened
  `InstallableRunnerCardWithDiscount` to the stack, which the install
  already took.
- **Cezve: `PaysFor::DuringRunsOnCentralServers`**, `DuringRuns` narrowed
  to HQ, R&D and Archives, as broad as the credit pool there (CR 1.10.4c).
- **Fidelity limits:** Into the Depths' search offers only a program that
  could be installed; The Twinning hears one moment per payment. Both on
  the known-limits list, both with the printed outcome.
- **Client.** `PublicRunState::ice_passed` is the view's one new field,
  an engine's line in `view_ledger` (each pass is drawn by
  `board::trail`); `PaysFor` and `Amount` have their prose.
- **Decks.** **Burn Rate**, a new Sweep deck on Esâ Afontov: Pay As You
  Go's frame, its Running Hot and Marrow the identity's fuel, with two
  Begemot for its two Take a Dive, two Ghosttongue for two Time Bomb and
  two The Twinning for two Friend of a Friend. **Dead Reckoning**, a new
  Sweep deck on Captain Padma Isbister: Safety Net's frame, whose
  Endurance, Hyperbaric, Propeller, Environmental Testing, Flux Capacitor
  and Lampades a charge feeds, with two Rigging Up for two Spark of
  Inspiration, two Into the Depths for two Joy Ride, two Stoneship Chart
  Room for two Aircheck and two Daeg for two Spree. Hit List takes two
  Cezve for two of its three Kompromat. Every card given up is still in
  another deck; both new decks are Eternal-only, pinned beside their
  frames.
- **DSL ratio** (`pool_status.py`): unchanged at 17 of 97 `Effect`
  variants single-use, 1 unused (`Trace`), now over 475 card files.
- **Real play** (`--headless`, each edited deck against Hostile Bid, seed
  2; random seats 96 games, planner seats 48; each pair is random /
  planner). Esâ Afontov's trigger fired 104 / 67 times, its "may" offered
  56 / 43; Begemot installed 11 / 8 and used 1 / 38 times; Ghosttongue
  installed 19 / 0; The Twinning installed 11 / 0, its breach heard 34 /
  0 times and its spend off an installed card never in those 96 games —
  once in 384 random games on seed 5, where a payment off a card came 99
  times; Captain Padma Isbister's trigger fired 460 / 261 times; Rigging
  Up played 38 / 19; Daeg installed 35 / 1, hearing 87 / 1 scores and
  steals; Stoneship Chart Room installed 51 / 37 and used 51 / 32 times;
  Into the Depths played 34 / 0; Cezve installed 33 / 40. **Bot debts:**
  the planner never plays Into the Depths, Ghosttongue or The Twinning in
  48 games and installs Daeg once; on the bot-debts list.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included, so every new card was seen in play. `coverage_identical.py`
  against Stage 2 (38ca89f, 192 games a report): random identical, view
  and index alike — the sample matchups hold no Sweep deck, and the four
  new words change nothing there, the five newly dispatched payments
  included. The planner seatings move by `determinize`, whose prior holds
  ten more cards (view and index alike): Corp agenda wins 59 → 58, Runner
  agenda wins 98 → 99.

#### Stage 4 — advancement counters (2 October 2026)

`claude/serene-einstein-6bhlig`: Vladisibirsk City Grid, Drago Ivanov,
Mestnichestvo, Chekist Scion, Mutually Assured Destruction, Moon Pool,
Azef Protocol, Midnight-3 Arcology and Mavirus. **No new `Effect`.**
Midnight Sun 41 of 65, its booster pack 4 of 7; `MS_UNIMPLEMENTED` 33 →
24 and `MSBP_UNIMPLEMENTED` 5 → 3 (Vladisibirsk City Grid and Azef
Protocol are booster-pack cards reprinted in the set, built from 32006
and 32007). Six cards composed; three needed a word widened, each for
every card.

- **What each is made of.** "You can advance this" is
  `advancement_requirement: 0`, as on Ubiquitous Vig; Vladisibirsk City
  Grid is the first upgrade to print it, and the advance action asked no
  more than that. Its ability is Charlotte Caçador's
  `RemoveAdvancementCounters` as a cost, `OncePerTurn`, into Hype
  Machine's `Advanceable` and `InRootOfThisServer` prompt with
  `NotSourceCard`, so the grid is never its own target. Drago Ivanov is
  the same cost with `DuringYourTurn`, Coalescence's word, into
  `GiveTags`. Mestnichestvo is Vasilisa's encounter offer paid with a
  counter, its trigger asked only with a counter to pay, and two
  subroutines. Mutually Assured Destruction is a Double operation's
  `additional_play_cost` at two clicks and Simulation Reset's selection
  over rezzed installs into Archives, then `GiveTags(CardsSelected)`,
  guarded as Stage 1 learned. Moon Pool is Spin Doctor's
  `RemoveSelfFromGame` over a `Sequence`: an HQ trash, then two picks of
  one facedown Archives card each, revealed and shuffled into R&D, each
  followed, when `ActingCardMatches` an agenda, by an optional counter
  (Reanimation Protocol's word). Mavirus is Snare!'s reveal in R&D, a
  `PresentChoice` over `PurgeVirusCounters` and
  `CurrentlyAccessingInstalledCard { rezzed_only }` around its net
  damage, and a `TrashSelf` purge.
- **Three words widened.** `Amount::Increased`, `Reduced`'s other half:
  Chekist Scion's "1 tag plus 1 tag for each hosted advancement counter"
  is one instruction (CR 9.12.2c), one `TagsGiven` and one prevention
  window, where two `GiveTags` would have been two. `Prohibition::
  DiscardStep`: Midnight-3 Arcology's "skip your discard step this turn"
  is a lingering `Cannot` that `turn::begin_discard_step` asks before
  any hand size, going straight to the end-of-turn window (CR 5.5.4d) —
  a step forbidden for a duration is what `Lingering::Cannot` already
  is, and it rides in the view with no new field. And an agenda's own
  `ScoreCost` with `Scope::This`: `validate` admits it on an agenda
  only; the scan asks a card its own text first, active or not, so the
  facedown agenda prices its own score; `payment::pays_a_cost_that_may_ask`
  gains `ScoreAgenda`, since Azef Protocol's "trash 1 of your other
  installed cards" asks which by the payment's replay.
- **A rules bug the view sweep found.** `engine::score_agenda` took the
  agenda's position in the installed list before the score's costs were
  paid and uninstalled by it after; Azef's cost trashing a card ahead of
  the agenda left the position past the end, and the 256-seed view sweep
  panicked on it. The agenda is named by its install now, and Azef's
  test puts the agenda behind the card its cost trashes.
- **Fidelity limits:** Moon Pool's two reveals are two picks of one, each
  shuffled in and its counter answered before the next, and its counter
  is offered on the Corp's own installs; the Corp knows every card either
  way, so the outcome is the printed one (on the known-limits list).
- **Client.** No new field. `Prohibition::DiscardStep` and
  `Amount::Increased` have their words in `prose` and the HUD's
  in-effect list. Chekist Scion joins the traps whose rez earns no glow
  (`board::rez::gains_nothing`): it works face down, as Urtica Cipher
  does.
- **Decks.** Paid Content takes two Drago Ivanov, two Vladisibirsk City
  Grid and two Chekist Scion for its two Behold, two Gaslight and two
  Balanced Coverage — advanceable cards to share a root with the grid —
  and stays Eternal-only, which Drago's ban needs. Spin Cycle takes two
  Mestnichestvo for two Virtual Service Agent and keeps its pin. Tag,
  You're It takes two Mutually Assured Destruction for two Government
  Subsidy; Hostile Bid two Azef Protocol for two Let Them Dream, point
  for point; Retirement Package two Midnight-3 Arcology for two Lightning
  Laboratory, point for point; A Thousand Cuts two Moon Pool and two
  Mavirus for two Mindscaping and two Front Company. Every card given up
  is still in another Sweep deck.
- **DSL ratio** (`pool_status.py`): unchanged at 17 of 97 `Effect`
  variants single-use, 1 unused (`Trace`), now over 484 card files.
- **Real play** (`--headless`, each edited deck against Safety Net, seed
  2; random seats 96 games, planner seats 48; each pair is random /
  planner). Drago Ivanov installed 77 / 59 and used 6 / 0 times;
  Vladisibirsk City Grid installed 93 / 65 and used 14 / 0; Chekist
  Scion's access fired 36 / 56 times; Mestnichestvo installed 86 / 53,
  its encounter offer fired 35 / 0 times and its subroutines 100 / 132;
  Mutually Assured Destruction played 8 / 0; Azef Protocol scored 1 / 22,
  its meat damage fired 1 / 15 times (a score that wins ends the game
  before its trigger); Midnight-3 Arcology scored 2 / 8, its trigger 2 /
  6; Moon Pool installed 73 / 45 and used 27 / 1; Mavirus installed 61 /
  6, its access fired 78 / 42 times. **Bot debts:** the planner never
  plays Mutually Assured Destruction, never uses Drago Ivanov or
  Vladisibirsk City Grid, never advances Mestnichestvo so its offer never
  comes, uses Moon Pool once and rezzes no Mavirus in 48 games; on the
  bot-debts list.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included, so every new card was seen in play. `coverage_identical.py`
  against Stage 3 (6ad6988, 192 games a report): random identical, view
  and index alike — the sample matchups hold no Sweep deck, and the three
  widened words change nothing there. The planner seatings move by
  `determinize`, whose prior holds nine more cards (view and index
  alike): Corp agenda wins 58 → 55, Corp flatlines 31 → 27, Runner
  agenda wins 99 → 103, Runner deck-outs 4 → 7.

#### Stage 5 — ice words (2 October 2026)

`claude/serene-einstein-6bhlig`: Cat's Cradle, Ivik, Wave, Bathynomus,
Stavka, Hákarl 1.0 and Trust Operation. **No new `Effect`.** Midnight Sun
48 of 65, its booster pack 5 of 7; `MS_UNIMPLEMENTED` 24 → 17 and
`MSBP_UNIMPLEMENTED` 3 → 2 (Hákarl 1.0 is a booster-pack card reprinted in
the set, built from 32004). Four cards composed; three needed a word, each
for every card.

- **What each is made of.** Ivik's discount is Reverb's `RezCost` on its
  own text, counted over Pulse's `CorpInstalls` with `IceOfType(CodeGate)`.
  Wave is Pulse's `RezzedDuringRunAgainstThisServer` rez trigger over a
  `PresentChoice` — "you *may* search", so a declined search shuffles
  nothing — whose search is Tocsin's reveal from R&D into HQ, and its
  subroutine Pulse's count of rezzed harmonic ice as a gain. Stavka is
  Anvil's "you may trash 1 of your other installed cards" (`Cost::Trash`
  with `NotSourceCard`) as an `OnRez` offer whose `if_paid` is Brasília
  Government Grid's `ModifyStrength` on this ice for the run, under an
  `EffectIf(RunInProgress)`: rezzed by Trust Operation outside a run, the
  trash is still offered and the strength has no run to last. Trust
  Operation is Hypoxia's `play_requirement: IsTagged`, Above the Law's
  resource trash, and Reanimation Protocol's install `rez: true` with The
  Powers That Be's `ignore_costs` out of Archives; an agenda installed by
  it is revealed and left unrezzed, as the install already does. Cat's
  Cradle is Abaasy's decoder; Hákarl's break is Ansel 2.0's
  `BreakSubroutinesUnconditionally` for one click lost; Bathynomus's net
  damage and Ivik's two subroutines are words the pool already had.
- **Three words.** `Scope::Ice` takes a `CardFilter`: Cat's Cradle's "the
  rez cost of each piece of **code gate** ice" is the first standing
  effect about each piece of ice of a kind, read off the definition and
  the copy as `IceProtectingThisServer`'s filter is; Fransofia Ward and
  The Tungsten Tailor say `{"Ice": "Any"}`. `EffectRequirement::
  Protecting(ServerId)`, `ProtectingRemote`'s sibling: Bathynomus's
  "while this ice is protecting **Archives**" — `ActingCardMatches(InServer)`
  reads the definition, which has no place, and a scope over a server's
  ice is about every piece in it. `validate` holds it to ice as it holds
  `ProtectingRemote`. And `Prohibition::BioroidIceAbilities`: Hákarl's "the
  Runner cannot use paid abilities printed on bioroid ice for the
  remainder of this turn", a lingering `Cannot` that
  `engine::activate_ability` asks of a bioroid's ability the Runner would
  use (`RulesError::AbilityProhibited`), so the action list, which probes
  it, never offers one. Hákarl's derez is Kompromat's `Cost::Derez` over
  any other rezzed install, as Brasília's is over ice.
- **Fidelity limits:** "code gate ice" is a code gate as printed, so a
  subtype gained for a run is neither taxed by Cat's Cradle nor counted by
  Ivik (on the known-limits list).
- **Client.** No new view field. `Prohibition::BioroidIceAbilities` has its
  words in `prose` and the HUD's in-effect list, and a filtered
  `Scope::Ice` reads "each piece of ice (…)" in the inspector's engine
  reading.
- **Decks.** Hit List takes two Cat's Cradle for its Conduit and its
  Smartware Distributor; A Thousand Cuts two Ivik for two Tatu-Bola and
  two Bathynomus for two Lionsmane; Retirement Package two Wave for two
  Jaguarundi — a fourth harmonic ice beside Pulse, Reverb and Bloop — and
  two Hákarl 1.0 for two Brân 1.0, bioroid barrier for bioroid barrier;
  Tag, You're It two Trust Operation for two Retribution, gray op for gray
  op, both needing a tag; Hostile Bid two Stavka for two Event Horizon,
  destroyer sentry for destroyer sentry. Every card given up is still in
  another Sweep deck.
- **DSL ratio** (`pool_status.py`): unchanged at 17 of 97 `Effect`
  variants single-use, 1 unused (`Trace`), now over 491 card files.
- **Real play** (`--headless`, seed 2; random seats 96 games, planner
  seats 48; each pair random / planner; the Corp decks against Safety Net,
  Hit List against Hostile Bid). Ivik installed 74 / 37 and rezzed 0 / 6
  (seven credits less its code gates is more than a random Corp saves);
  Bathynomus rezzed 10 / 21, its subroutine fired 17 / 29 times; Wave
  rezzed 70 / 52, its rez trigger fired 66 / 52 times; Hákarl 1.0 rezzed
  33 / 18, its rez trigger 28 / 18, its click break used 25 / 30 times;
  Stavka rezzed 13 / 25, its rez offer 13 / 25 times; Trust Operation
  played 19 / 1; Cat's Cradle installed 38 / 7 and used 13 / 16 times.
  **Bot debts:** the planner plays Trust Operation once in 48 games and
  installs Cat's Cradle seven times; on the bot-debts list.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included, so every new card was seen in play. `coverage_identical.py`
  against Stage 4 (2f7c610, 192 games a report): random identical, view
  and index alike — the sample matchups hold no Sweep deck, and the three
  words change nothing there. The planner seatings move by `determinize`,
  whose prior holds seven more cards (view and index alike): Corp agenda
  wins 58 → 59, Corp flatlines 27 → 28, Runner agenda wins 101 → 99.

#### Stage 6a — the mark (3 October 2026)

`claude/serene-einstein-6bhlig`: Nyusha "Sable" Sintashta, Carpe Diem,
Backstitching and Virtuoso. **No new `Effect`.** Midnight Sun 52 of 65,
its booster pack 5 of 7; `MS_UNIMPLEMENTED` 17 → 13. Stage 6 was split by
mechanic when it was taken: 6a the mark, on Parhelion Stage 6b's rule; 6b
access outside a breach (Pinhole Threading, Deep Dive).

- **What each is made of.** Each identifies its mark as the turn begins
  (Tunnel Vision's `OnTurnStart` `IdentifyMark`; Carpe Diem identifies as
  it resolves). Nyusha's "the first time each turn you make a successful
  run on your mark, gain [click]" is Info Bounty's `when: Mark` with
  `first_each_turn`, on `OnSuccessfulRun`. Backstitching is Laser
  Pointer's `OfferPaidChoice` of `TrashSelf` for `BypassEncounteredIce` on
  `OnEncounter`. Virtuoso's `+1[mu]` is Pennyshaver's; its HQ branch is
  `AddAdditionalAccess` under `DuringRunOn(Hq)` — the trigger resolves at
  the run's success, before the breach's access limit is set — and its
  other branch is `SetRunEndedEffect(Breach(Hq))`, Cataloguer's breach with
  no run, started once the run has left `active_run`.
- **One word, `EffectRequirement::MarkIs(ServerId)`**: "your mark is this
  server". Carpe Diem's "You may run your mark" is three `EffectIf` branches,
  one per central, each Alarm Clock's `PresentChoice` of a run or nothing;
  Backstitching's "during a run on your mark" is three `EffectIf`s of
  `And(DuringRunOn(X), MarkIs(X))` around one offer, read off the run's
  server rather than where the ice is, as printed (Tunnel Vision's
  "protecting your mark" reads the ice). Composition didn't work: a run's
  target is a `ServerId`, which has no word for the mark, and
  `EncounteringIceProtectingMark` asks about an encountered ice. There is
  no `Or` requirement, so the branches are the disjunction.
- **A run's end effects are a list** (`RunState::on_end`,
  `CompletedRun::on_end`, `run::RunEndRider`), resolved in the order set,
  each taken as it starts, the rest waiting behind one that parks. It was
  one slot, and Virtuoso's breach, set as a run on the mark succeeds, would
  have overwritten what the run's own event set at its start (Raindrops
  Cut Stone's draw, Trick Shot's run). A test sets both.
- **Fidelity limit (a CR 10.11.5 deviation):** a first time on the mark is
  counted over the turn's runs on that server, so a mark Carpe Diem
  identifies mid-turn on a server already run successfully that turn
  gives Virtuoso and Nyusha no first time, where 10.11.5's own example
  says it does. On the known-limits list and the conformance row.
- **Client.** No new view field and no new words: requirements have no
  prose, and the mark is already an In effect line and a log line.
- **Decks.** Encore, a Sweep deck on Nyusha, the Criminal identity no other
  deck plays: three Carpe Diem, three Backstitching, two Virtuoso,
  Parhelion's Tunnel Vision and Info Bounty, and Picket Line's money and
  breakers (Carmen, Marjanah, Buzzsaw). Eternal-only: Nyusha is banned in
  Standard and Midnight Sun is not in Startup's pool. **Spare Parts** took
  a second Beta Build for its Sure Gamble: the 256-seed view sweep failed
  on its one copy going unseen once Encore joined the rotation (no other
  deck carries Beta Build), as Safety Net's one Gordian Blade once did.
- **DSL ratio** (`pool_status.py`): unchanged at 17 of 97 `Effect`
  variants single-use, 1 unused (`Trace`), now over 495 card files.
- **Real play** (`--headless`, seed 2, Encore against Retirement Package;
  random seats 96 games / planner 48). A mark identified 1,126 / 632
  times; Nyusha's click gained 171 / 131 times; Carpe Diem played 48 / 46;
  Backstitching installed 35 / 0, its offer heard on 108 encounters, and
  trashed 34 times by random seats; Virtuoso installed 20 / 0, its run on
  the mark heard 20 times. **Bot debts:** the planner never installs
  Backstitching or Virtuoso; on the bot-debts list.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included. `coverage_identical.py` against Stage 5 (a71d47e, 192 games a
  report): random identical, view and index alike. The planner seatings
  move by `determinize`, whose prior holds four more cards: Corp agenda
  wins 59 → 57, Runner agenda wins 99 → 101, flatlines unchanged.

#### Stage 6b — access outside a breach (3 October 2026)

`claude/serene-einstein-6bhlig`: Pinhole Threading and Deep Dive. **One
new `Effect`, which both use.** Midnight Sun 54 of 65, its booster pack 6
of 7; `MS_UNIMPLEMENTED` 13 → 11 and `MSBP_UNIMPLEMENTED` 2 → 1 (Deep Dive
is a booster-pack card reprinted in the set, built from 32003). **Stage 6
is complete.**

- **`Effect::Access { from, filter, count, then }`**: the Runner accesses
  `count` of the cards in `from` that `filter` admits, choosing each, not
  as a breach (CR 7.1.9, 7.1.10; `run::access_cards`,
  `AccessState::outside_breach`). Each access is a breach's access steps,
  through the same `present_card_for_access`; the procedure ends once the
  number named has been chosen (7.1.10), with no `BreachBegun` and no
  random access limit. The candidates are `pending_choice::
  eligible_positions`'s, so an access is offered what a selection over the
  same zone would be. Composition didn't work: every access began at a
  breach, whose candidates are a server's. **`then` is part of it**
  because an access in progress parks nothing a `Sequence` waits behind;
  it resolves as the card once the accesses end, or queues as a
  continuation behind whatever they left parked.
  - Inside a run only as the run's breach is replaced: the replacement no
    longer ends the run when it left an access, which ends the run when it
    does, never having breached (Info Bounty's "if you breached" is false).
  - Outside a run it stands in Cataloguer's run-less `RunState`, on R&D.
- **Pinhole Threading** is `PromptChooseServer` (Into the Depths' "run any
  server") whose `on_success` is a `SetAccessReplacement`: the chosen
  server is now written into it, as into `AddAdditionalAccess`, so "instead
  of breaching the attacked server" is whichever was run. The replacement
  forbids stealing or trashing agendas for the rest of the run, then
  accesses 1 root card matching `Not(InAttackedServer)`.
  **`Prohibition::StealOrTrashAgendas`**: "If that card is an agenda, you
  cannot steal or trash it during this access" — the access is the run's
  last act, so the rest of the run is this access, and made before the card
  is chosen it shows no facedown card's type in the in-effect list.
  `continuous::cannot_about` asks it beside `StealOrTrash` of an agenda,
  so the four sites that ask about a steal or a trash need no change.
- **Deep Dive** is Chain Reaction's `play_requirement`, then
  `SetAsideFromTopUntil` with a new `deck: Corp` ("the top 8 cards of R&D"
  is "until 8 of any card"), an `Access` over the new
  `CardZoneRef::OpponentSetAside` whose `then` is the "you may spend
  [click]" `OfferPaidChoice` of a second `Access`, each path ending in
  `ShuffleIntoDeck([OpponentSetAside])`, which shuffles into R&D.
  **The Corp's set-aside zone** (`CorpState::set_aside`, CR 4.8): faceup,
  so public — in the view, copied by `determinize` and struck from its
  pools, counted by the sweeps' conservation check and fog gate. A set-aside
  card is accessed by name (`AccessCandidate::SetAside`) and leaves the
  zone when stolen or trashed.
- **Fidelity limit:** Deep Dive's accesses stand on R&D, so they name R&D
  as their server and both clients say "Breach of R&D" (on the known-limits
  list).
- **Client.** `hud::in_effect` lists the Corp's set-aside cards ("set aside
  from R&D: …") in both clients; `outside_breach` is in the view and the
  ledger marks it the engine's; `prose` words `Access`, the zone, the deck
  and the prohibition, and the access pop-up names a set-aside candidate by
  its card.
- **Decks.** Encore takes three Pinhole Threading for its two Strike Fund
  and a Chrysopoeian Skimming; Safety Net two Deep Dive for its two
  Burner. Picket Line keeps the first two, Dead Reckoning and Street
  Gallery Burner. Deep Dive joins Chain Reaction on
  `CARDS_RARE_WITH_SWEEP_DECKS`, for the same reason: no agent makes three
  successful central runs in a turn, and its per-card test is what reaches
  it.
- **DSL ratio** (`pool_status.py`): 17 of 98 `Effect` variants
  single-use, 1 unused (`Trace`), over 497 card files (17 of 97 over 495
  before; `Access` is used twice).
- **Real play** (`--headless`, seed 2; random seats 96 games / planner
  48): Encore against Retirement Package, Pinhole Threading played 47 / 0
  times, its breach replaced by the access 20 times; Safety Net against
  Hostile Bid, Deep Dive played by neither. **Bot debts:** the planner
  never plays Pinhole Threading; on the bot-debts list.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included. `coverage_identical.py` against Stage 6a (30a85f4, 192 games a
  report): random identical, view and index alike. The planner seatings
  move by `determinize`'s prior, two cards larger: Corp agenda wins 57 →
  58, Corp flatlines 28 → 29, Runner agenda wins 101 → 99.

#### Stage 7a — the score area (3 October 2026)

`claude/serene-einstein-6bhlig`: Backroom Machinations, Regenesis, Blood in
the Water, Steelskin Scarring. **No new `Effect`.** Midnight Sun 58 of 65,
its booster pack 6 of 7; `MS_UNIMPLEMENTED` 11 → 7. Stage 7 was split by
mechanic when it was taken, as Stage 6 was: 7b is Big Deal and Mitosis.

- **One door into Archives.** Regenesis's "if no Corp cards have been added
  to Archives this turn" is a sum beside `agenda_points_scored` in the turn
  log (`TurnLog::added_to_archives`, the Turn History Rule), with
  `Amount::CorpCardsAddedToArchivesThisTurn` to read it. No event is common
  to every way into Archives — a trash, an operation filed, a card a prompt
  sends there, and the Corp's discard at the end of their turn, which is
  dispatched to nobody — so the eighteen pushes in the engine became one
  function, `turn_log::file_in_archives`, and a test scans the source for
  any other. The log rides in the view whole and `determinize` copies it,
  so no masking or sample change.
- **An operation is trashed once it has resolved** (CR 8.2.7). The engine
  files a played operation in Archives before its text resolves; Backroom
  Machinations leaves for the score area as it resolves, so it was never
  added, and `turn_log::unfile_resolving_operation` gives the count back.
- **Composed.** Backroom Machinations is Unleash's additional cost and
  Myōshu's `AddToScoreAreaAsAgenda`. Regenesis is Kingmaking's selection
  with a `destination` (CR 1.17.3e), over a facedown agenda in Archives,
  revealed. Steelskin Scarring is Strike Fund's trigger, so a discard
  offers nothing (CR 1.19.3). Blood in the Water is a printed 0 that
  Ontological Dependence's `ContinuousKind::AdvancementRequirement` makes
  the number of cards in the grip.
- **Client.** The X Blood in the Water prints was owed to this stage:
  `card_face::Slot::AdvancementX` and `card_face::advancement_slot`, which
  the face, the score-area sheet, the card facts and the language model's
  glossary all ask, so none says 0. The known limit is struck.
  `board::diff`'s invariant test now counts a card a prompt showed its
  chooser by name (ezaM's "look at the top card of R&D") as seen: the new
  decks put Bring Them Home there at seed 0.
- **Decks.** A Thousand Cuts takes two Regenesis and two Blood in the
  Water for two Sericulture Expansion and a Lotus Haze, six points for six;
  Spin Cycle two Backroom Machinations for two Your Digital Life; Burn Rate
  two Steelskin Scarring for two Raindrops Cut Stone. Permafrost, Paid
  Content and Pay As You Go keep what was given up.
- **DSL ratio** (`pool_status.py`): 17 of 98 `Effect` variants
  single-use, 1 unused (`Trace`), over 501 card files.
- **Real play** (`--headless`, seed 2; random seats 96 games / planner
  48), against Burn Rate: A Thousand Cuts scored Regenesis 3 / 3 times,
  its choice offered 2 / 1 times, and Blood in the Water 13 / 7; Spin
  Cycle played Backroom Machinations 7 / 3 times; Steelskin Scarring was
  played 11 / 23 times and drew again as it was trashed from the grip or
  stack 17 / 12 times (against A Thousand Cuts). **No bot debt:** the
  planner plays all four.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included. `coverage_identical.py` against Stage 6b (d1be87e, 192 games a
  report): random identical, view and index alike — the door moved no rule.
  The planner seatings move by `determinize`'s prior, four cards larger:
  Corp agenda wins 58 → 59, Corp flatlines 29 → 28, Runner agenda wins
  99 → 100, decked Runners 6 → 5.

#### Stage 7b — the turn (3 October 2026)

`claude/serene-einstein-6bhlig`: Big Deal and Mitosis. **One new `Effect`,
which Big Deal alone uses.** Midnight Sun 60 of 65, its booster pack 6 of
7; `MS_UNIMPLEMENTED` 7 → 5. **Stage 7 is complete.**

- **`Effect::Score`**: the Corp scores the card this resolves as, if able.
  Scoring was only `PlayerAction::ScoreAgenda`, the Corp's with their action
  phase's priority and no window open, and Big Deal scores in the middle of
  its own resolution (CR 1.2.1 over 1.17.3). `engine::score_agenda` is now
  the action's phase and window around `engine::score_install` — the score
  itself, from the `ScoreAgendas` lock through costs, dividends and the
  dispatched `AgendaScored` — which the effect shares, so a score by text is
  heard as any other. `engine::scorable` asks the same questions without
  paying, and **`EffectRequirement::Scorable`** puts it to the card being
  resolved, so "You may score that card, if able" is offered only when there
  is a score to take: `ActingCardMatches` reads the definition, and whether
  an agenda can be scored is the table's. `payment::could_ask` admits the
  choice's answer when any install's cost to score could ask (Azef
  Protocol's).
- **Big Deal** is a selection of 1 installed card whose `then` places 4
  advancement counters and offers the score, then `EndActionPhase`, last,
  where "after" puts it; `removed_after_play`, and the catalog's trash cost
  of 3 (CR 3.5.3).
- **`Prohibition::Rez`**: Mitosis's "You cannot score or rez either of
  those cards this turn", bound to each card it installed through
  `PromptInstallCorpCard::if_installed`, as Warm Reception's score lock is.
  Asked by `engine::rez_install`, the one place a Corp card is turned
  faceup, ahead of any way to pay, so the action, the action list's probe
  and a card's text that rezzes are all refused (`RezRestricted`, which a
  text rez treats as an unaffordable one).
- **`PromptInstallCorpCard::new_remote`**: "creating a new remote server
  each time" offers the one remote the Corp would create, and nothing when
  it may create no more. Mitosis is two of Warm Reception's selections in a
  `Sequence`, each over `Not(CardType(Operation))` in HQ, "up to" by its
  minimum of 0.
- **Client.** `prose` words the effect, the prohibition ("the Corp cannot
  rez that card") and the new remote; `hud::cannot_words` lists the
  prohibition with the rest. Nothing new reaches the view: the lingering
  list already carries it.
- **Decks.** Retirement Package takes two Big Deal for two Retirement Plan
  (Deterrence keeps it), Permafrost two Mitosis for two Seamless Launch
  (Brutal Efficiency and The Syndicate keep it). Big Deal needed no rare
  entry: the card gate counts an access, and the deep sweep accesses it.
- **DSL ratio** (`pool_status.py`): 18 of 99 `Effect` variants
  single-use, 1 unused (`Trace`), over 503 card files (17 of 98 over 501
  before; `Score` is used once).
- **Real play** (`--headless`, seed 2; random seats 96 games / planner
  48), against Burn Rate: Retirement Package played Big Deal 12 / 1 times,
  Permafrost Mitosis 13 / 2. The report does not say how many of Big Deal's
  plays scored; the per-card test does. **Bot debts:** on the list — the
  planner barely plays either.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included. `coverage_identical.py` against Stage 7a (e73ac08, 192 games a
  report): random identical, view and index alike — the scoring refactor
  moved no rule. The planner seatings move by `determinize`'s prior: Corp
  agenda wins 59 → 58, Corp flatlines 28 → 29.

#### Stage 8a — subroutine lists (3 October 2026)

`claude/serene-einstein-6bhlig`: Echo and Envelopment. **No new `Effect`.**
Midnight Sun 62 of 65, its booster pack 6 of 7; `MS_UNIMPLEMENTED` 5 → 3.

- **`ContinuousKind::Subroutines { subroutine, count, before }`**: the
  subroutines a piece of ice gains by its own static ability (CR 9.8.3b,
  "before its other subroutines"; 9.8.3d, after or in no stated order),
  `count` copies read as the ice (`HostedCounters`). About `This`, on ice,
  and nothing else: another card's grant is 9.8.3a or 9.8.3e, which
  `Effect::GainSubroutine` already says. A "when encountered" trigger with
  `GainSubroutine` was the stopgap and was rejected: AirbladeX could prevent
  it, and it would sort among another card's grants. Being a standing
  effect, it is read through the continuous scan, so Hush's host — which
  has lost its abilities — gains nothing with no word of its own.
- **Where it applies**: `run::engine::add_own_subroutines`, as each
  encounter begins (the approach's `Continue` and a forced encounter),
  ahead of `add_gained_for_the_run`, which then puts another card's grants
  outside both: 9.8.3a, b, c, d, e in CR 9.8.2's order. Marked `gained`, so
  they go with the encounter and the next one counts again. Read then and
  not when the run's ice is built, because Echo's count moves on a rez
  further out and Envelopment's as a turn begins; nothing in the pool moves
  either during an encounter.
- **CR 6.5.7c**: ice with no subroutines is fully broken as 6.9.3b begins,
  by no object. Echo with no counters is the first ice in the pool with
  none; `IceFullyBroken { by: None }` is announced after `IceEncountered`
  once the list is whole, and the pass is "after fully breaking it". Two
  engine tests whose fixture Ice Wall prints no subroutines now expect it.
- **Echo** hears every rez of harmonic ice, its own included (`OnRez`,
  `Any`, `when` harmonic ice), as Working Prototype hears its own.
  **Envelopment**: four counters as it is rezzed, one off as the Corp's
  turn begins, and its printed "Trash this ice." after them.
- **Client.** `prose` words the kind. Nothing new reaches the view: the
  run's subroutine list already carries what the ice gained.
- **Decks.** Retirement Package takes two Echo for two Sorocaban Blade
  (Deterrence and Undertow keep it), beside four harmonic ice; Hostile Bid
  two Envelopment for two Logjam (Land Grab keeps it).
- **DSL ratio** (`pool_status.py`): 18 of 99 `Effect` variants single-use,
  1 unused (`Trace`), over 505 card files (503 before).
- **Real play** (`--headless`, seed 2, against Burn Rate; random seats 96
  games / planner 48): Echo rezzed 54 / 32 times, its subroutines fired 124
  / 171; Envelopment rezzed 6 / 22, fired 10 / 32, and lost a counter at
  159 of the planner's turn starts. **Bot debt:** the evaluator reads only
  printed subroutines, so it prices Echo as ice that never ends the run.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included. `coverage_identical.py` against Stage 7b (cdc7d09, 192 games a
  report): random identical, view and index alike — no sample deck prints
  a card the stage touched. The planner seatings move by `determinize`'s
  prior and end the same: Corp agenda wins 58, flatlines 29, Runner agenda
  wins 100, decked Runners 5, before and after.

#### Stage 8b — ability layers (3 October 2026)

`claude/serene-einstein-6bhlig`: Trieste Model Bioroids and Light the Fire!.
**No new `Effect`.** Midnight Sun 64 of 65, its booster pack 7 of 7
(`MSBP_UNIMPLEMENTED` empty); `MS_UNIMPLEMENTED` 3 → 1.

- **`Prohibition::BreakSubroutinesOnIce`**: Trieste's "Runner card
  abilities cannot break subroutines on the chosen ice", bound to the ice
  its rez selection chose (`Prohibit::this_install`). Asked by both break
  effects (`ability::breakable_now`) only when the breaker is a Runner card,
  so Ansel 2.0's "Lose [click]: Break 1 subroutine on this ice" — a Corp
  card's ability the Runner uses — and a 1.0's click-break stand. Not
  Hafrún's `BreakSubroutines`, which is about the breaking install.
- **`EffectDuration::WhileRezzed`**: the choice lasts while Trieste is
  rezzed (CR 9.10.3c), resolved to `Until::WhileRezzed` of the card whose
  decision it was — `ResolutionContext::prompting_install`, set where a
  selection's `then` resolves as the chosen card, since the acting install
  there is the ice. A derez, a trash or a second rez ends it (the last by
  `forget_rezzed_period`, already Lycian's). `lingering::until` takes the
  making install for it.
- **A choice remembered by a lost ability**: the prohibition is Trieste's
  static ability reading its choice, so while Trieste has lost its
  abilities (Light the Fire! on its server) the entry holds and says
  nothing (`continuous::runner_cards_may_break`, CR 9.1.9a).
- **Light the Fire!**: `[click], [trash], suffer 1 core damage` is
  `Cost::AllOf`, the first ability in the pool to suffer damage inside one;
  the run is Hannah's `PromptChooseServer { only_in: Remote }`. Its two
  riders are words widened, not effects added:
  - `Effect::LoseAbilities::attacked_root` makes the loss about
    `lingering::On::RootOfAttackedServer`, read when asked against the run's
    server, so a card installed there mid-run loses them and a redirect is
    followed — the reason `On` has no fixed `Server`. `lingering::
    loses_abilities` and `active::installs_without_abilities` read it.
  - `CardTarget::AttackedServerRoot` is "trash all cards in the root of the
    attacked server", each by its handle, by the Runner, as the run is
    declared successful (`on_success`) and so before the breach. A
    `PromptChooseCards` with a count of every root card was rejected: a
    forced choice of everything, and prevention asks about one card. No
    Corp card in the pool prevents a trash.
- **Client.** `prose` and `hud` word the prohibition ("Runner card
  abilities cannot break subroutines on Ansel 2.0"), the duration, the root
  loss and the target. Nothing new reaches the view: the lingering list
  carries both entries.
- **Decks.** Retirement Package takes two Trieste for two Working Prototype
  (Deterrence keeps it), beside Ansel 1.0, Ansel 2.0 and Hákarl 1.0; Burn
  Rate two Light the Fire! for two Valentina Ferreira Carvalho (Pay As You
  Go keeps her).
- **DSL ratio** (`pool_status.py`): 17 of 99 `Effect` variants
  single-use, 1 unused (`Trace`), over 507 card files — `LoseAbilities`
  has a second card.
- **Real play** (`--headless`, seed 2, Burn Rate against Retirement Package
  and Hostile Bid; random seats 96 games / planner 48): Trieste rezzed 82 /
  25 times, its choice made at every rez with rezzed bioroid ice to choose;
  Light the Fire! used 15 and 22 times by random seats, **never by the
  planner** (installed once each) — on the bot-debt list.
- **Measured.** Both sweeps are green at 256 seeds, the card gate
  included. `coverage_identical.py` against Stage 8a (6d63c08, 192 games a
  report): random identical, view and index. The planner seatings move by
  `determinize`'s prior, which now holds both cards: Corp agenda wins
  58 → 54, Runner agenda wins 100 → 105, decked Runners 5 → 4, flatlines
  29 → 29.

#### Stage 8c — the trash chain (3 October 2026)

`claude/serene-einstein-6bhlig`: Ob Superheavy Logistics: Extract. Export.
Excel., on a Sweep deck of its own. **No new `Effect`.** Midnight Sun 65 of
65, its booster pack 7 of 7; `MS_UNIMPLEMENTED` empty. **Stage 8 is
complete, and Midnight Sun with it.**

- **`EventFilter::TrashedRezzed`**: "When you trash a rezzed card, except
  during installation" — the card was rezzed as it went and was not trashed
  as a step of installing another (CR 8.5.16c), both read off the event
  (`GameEvent::CardTrashed::install`, which had carried them since Yakov).
  One word for both halves, as `TrashedFromThisServer` fuses the same
  exception. "You trash" is `OnCardTrashed`'s own: a trash the Runner
  carried out (Light the Fire!, an access) is not the Corp's. Two stale
  comments that said the event did not carry the rez state are corrected.
- **`Amount::TriggeringCardPrintedCost`**: the trashed card's printed cost,
  read off the trash that fired the trigger. Making the trashed card the
  acting one (`acts_on_subject` with `PrintedCost`) was rejected: it would
  key Ob's "once per turn" and its prompt on the card in Archives.
- **`CardFilter::PrintedCostExactly`**: `PrintedCostAtMost`'s "equal to",
  resolved as the selection is offered, while the trash is still on the
  context. Ob's search is Eminent Domain's: ice, an asset or an upgrade
  (the types with a rez cost, CR 2.3.5) at exactly one less, "may" by its
  minimum of 0, R&D shuffled after. A trashed card printed at 0 has none
  to find, and the requirement `AmountAtLeast(TriggeringCardPrintedCost, 1)`
  keeps `Reduced` from flooring it to 0 — and keeps the turn's use unspent.
- **`PromptInstallCorpCard::ignore_credit_costs`** (CR 1.16.5b): every
  credit of the install and of the rez is removed and any other cost stays,
  which is CR 8.5.13c's own Ob example (Archer's forfeit). Paid as a
  discount of everything; `engine::rez_price` now reads a discount past
  `i32::MAX` as the whole price rather than wrapping.
- **Supply Chain** (Sweep, Eternal; 45 cards, Weyland and neutral, no
  influence): the fifth Midnight Sun identity's deck. Every rez cost from 1
  to 5 twice over; Envelopment, Regolith Mining License and Cybersand
  Harvester trash themselves, Svyatogor Excavator and Stavka another card,
  Kimberlite Field and Azef Protocol a card as they are scored.
- **Client.** `prose` words the filter, the amount and the install. Nothing
  new reaches the view.
- **DSL ratio** (`pool_status.py`): 17 of 99 `Effect` variants
  single-use, 1 unused (`Trace`), over 508 card files.
- **Real play** (`--headless`, Supply Chain against Burn Rate, seed 2;
  random seats 96 games / planner 48): Ob's trigger fired 171 / 31 times
  and offered a card to install 157 / 30 times. The planner's games split
  24 / 24 on agenda wins. Regolith Mining License was used 56 / 126 times,
  Envelopment rezzed 18 / 28.
- **Measured.** Both sweeps are green at 256 seeds with Supply Chain in
  the rotation, the card gate included. `coverage_identical.py` against
  Stage 8b (dceab7e, 192 games a report): **identical in all four shapes**,
  random and planner, view and index.

### 6. Uprising and its Booster Pack — 65 cards (C 10 / V 37 / M 18)

#### Stage 1 — ten cards, composed (3 October 2026)

`claude/serene-einstein-6bhlig`: Moshing, Self-modifying Code, Daily
Casts, Bass CH1R180G4, Cerebral Overwriter, Drafter, Flower Sermon, Prāna
Condenser, Bellona and Colossus. **No new `Effect`, and no change to the
engine:** card files, decks and tests, with one line each in a client test
and a planner test. Uprising 10 of 65, its booster pack 0 of 7;
`UR_UNIMPLEMENTED` 65 → 55. The survey's "composes" held for all ten.
Moshing and Bellona are Standard-banned and built anyway, as Svyatogor
Excavator was.

- **The survey re-read first.** Most of what it called vocabulary has been
  built since it was written: the turn's end heard, stealth credits, set
  aside, psi, a forced encounter, a remembered choice and additional costs
  to steal, score or trash. Still new: lockdown (CR 3.5.1c), the first card
  to start a trace (`Effect::Trace`), an encounter nested in an encounter
  (Konjin, CR 6.1.3c), a replacement as an agenda enters the Runner's score
  area (Project Vacheron), and a cost to run that changes when the identity
  flips (Earth Station, CR 6.3.2b). The stage order stands.
- **What each is made of.** Moshing is Sell Out's additional cost over
  the grip (`Cost::Trash { from: OwnGrip, count: 3 }`): the event has left
  the grip before its cost is paid, so it needs three *other* cards, and a
  grip of two refuses it. Self-modifying Code is Arruaceiras Crew's
  `AllOf[Credits 2, TrashSelf]` on Into the Depths' stack search, its
  install guarded by `EffectIf(AmountAtLeast(CardsSelected, 1))` so a
  search that finds nothing declines; trashing itself first frees its
  memory. Daily Casts is Open Market's eight credit counters, two a turn,
  trashed when empty. Bass CH1R180G4 is Humanoid Resources' cost with
  Nanomanagement's click gain. Cerebral Overwriter is Mr. Hendrik's access
  offer paid in credits into Urtica Cipher's damage per advancement counter.
  Drafter is The Basalt Spire's Archives-to-HQ prompt and The Powers That
  Be's choice of zone into an install ignoring all costs. Flower Sermon is
  five agenda counters on scoring and a once-per-turn counter ability:
  Balanced Coverage's revealed top card of R&D, two cards drawn and
  Mindscaping's card from HQ to the top of R&D. Prāna Condenser hears net
  damage about to resolve (`OnDamageAboutToResolve`, `when: Damage(Net)`,
  Net Shield's word, settled in `prevention::settle`) and offers the Corp
  the prevention, a counter and 3[credit]; it is a trigger and not an
  interrupt, because a cost-free interrupt could be used once per point.
  Its trash ability reads the counters its own cost removed
  (`last_known`, as Fermenter's). Bellona is Méliès U.'s `steal_cost` and
  a gain on scoring. Colossus is advanceable ice with strength per counter
  and Mindscaping's "instead": each subroutine is two `EffectIf`s either
  side of three counters.
- **Fidelity limits:** two, under Known limits. Prāna Condenser hears net
  damage whoever does it, since `EventFilter::Damage` reads only the kind;
  Self-modifying Code's search offers only a program that could be
  installed, as Into the Depths' does.
- **Client.** Nothing added to the view, the log or a decision, so no
  ledger line and no ledger row. Cerebral Overwriter is an ambush that
  works face down, so `board::rez::gains_nothing` now names it among the
  traps (`exactly_the_traps_gain_nothing_by_a_rez`): its rez stays legal
  and on its menu, and earns no glow. The deck builder's set test offers
  Uprising to both sides and names Downfall as the set with nothing built.
- **Decks.** Burn Rate takes two Moshing for its two Chastushka; Safety Net
  two Self-modifying Code for its two Lobisomem; Spare Parts two Daily
  Casts for its two Smartware Distributor; Retirement Package two Bass
  CH1R180G4 for its two Warm Reception, two Cerebral Overwriter for its two
  Perfect Recall and two Drafter for its two Ansel 2.0; A Thousand Cuts two
  Flower Sermon for its two See How They Run, point for point, and two
  Prāna Condenser for its two Dr. Vientiane Keeling, so the deck's own net
  damage is what Prāna prevents; Paid Content two Bellona for two Send a
  Message; Hostile Bid two Colossus for its two Maskirovka. Every card
  given up is still in another deck.
- **A planner test moved again.**
  `a_kill_corp_plays_public_trail_because_the_runners_answer_is_priced`
  counted the balanced Corp over twenty seeds since Midnight Sun Stage 1,
  asserting fewer than half tag. The planner stages since had moved it to
  9 of 20 on `main`, one under the bar, and ten more cards in the prior
  moved it to 11. Over sixty seeds it is 25 before the stage and 26 after,
  about two in five, so the balanced Corp is now counted over sixty. The
  kill Corp's two claims stay on the first twenty, where both hold exactly:
  over sixty, the kill Corp tags against three cards every time, and
  against the full grip once after the stage (seed 35, Hedge Fund first)
  and never before.
- **DSL ratio** (`pool_status.py`): 17 of 99 `Effect` variants
  single-use, 1 unused (`Trace`), over 518 card files, as on `main` over
  508.
- **Real play** (`--headless`, each edited deck against Safety Net or
  Hostile Bid — Colossus's planner seats against Spare Parts — seed 2; random seats 96 games, planner seats 48; each pair
  random / planner). Moshing played 31 / 43 times. Self-modifying Code used
  27 / 9 times. Daily Casts installed 24 / 22 times and paid at 87 / 71
  turn starts. Bass CH1R180G4 used 47 / 3 times. Cerebral Overwriter's
  access offer fired 55 / 51 times. Drafter rezzed 39 / 37 times, its
  subroutines firing 189 / 171 times. Flower Sermon scored 0 / 1 times and
  stolen 40 / 25, its ability used once. Prāna Condenser's prevention fired
  29 / 99 times and its trash ability 11 / 14. Bellona scored 0 / 28 times
  and stolen 10 / 8. Colossus rezzed 4 / 20 times, its subroutines firing
  22 / 77 times. Bass and Flower Sermon go on the bot debts.
- **Measured.** `cargo test --workspace` green (2,698) and clippy silent. Both
  sweeps are green at 256 seeds, the card gate included, so every new card
  was seen in play. `coverage_identical.py` against `origin/main`
  (95a6097, 192 games a report): random identical, view and index alike,
  because the sample matchups hold no Sweep deck. The planner seatings
  move by `determinize`'s larger prior (view and index alike, of 192):
  Corp agenda wins 60 → 58, Corp flatlines 26 → 25, Runner deck-outs 1 →
  4.

#### Stage 2 — the turn's end, `PaysFor` and subtype words (4 October 2026)

`claude/serene-einstein-6bhlig`: Mystic Maemi, Paladin Poemu, Hoshiko
Shiro: Untold Protagonist, Penumbral Toolkit, Mantle, Keiko, Cybertrooper
Talut, Odore, DreamNet, Euler and Pauleʼs Café — eleven Runner cards, and
Hoshiko's Sweep deck, Side Quest. **No new `Effect`, and one fewer:**
Madani's single-use `InstallRunnerCardFromHost` became the zone install's
`HostedOnSource`, which Pauleʼs Café needed with a discount. Uprising 21 of
65, its booster pack 2 of 7 (Maemi and Talut are reprinted there);
`UR_UNIMPLEMENTED` 55 → 44, `URBP_UNIMPLEMENTED` 7 → 5. Hoshiko Shiro is
Standard-banned and built anyway.

- **The words.** Each is a gap in a vocabulary, fixed there for every card:
  - **Hosted credits for using a card, and for playing one.**
    `PaysFor::UsingIcebreakers` became `Using(CardFilter)` (Cyberfeeder and
    The Toolbox write `Using(Icebreaker)`, Mantle
    `Using(CardTypeOneOf([Hardware, Program]))`; using a card is using its
    abilities, CR 9.1.6, and spending its credits is using it, CR 1.10.4d),
    and `Playing(CardFilter)` is Mystic Maemi's "to play events", matched
    against a new `payment::Purpose::Play` that the event's and the
    operation's plays state. Paladin Poemu's "to install non-connection
    cards" was already `Installing(Not(..))`. A pool is spent before another
    unasked when its words are *within* the other's, which was word-for-word
    equality and is now a conservative filter implication
    (`payment::filter_implies`), so The Toolbox's credit still goes before
    Mantle's on a break.
  - **A companion column in the turn log** (`Kind::CompanionResource`, the
    sixteenth kind and the last a `u32` of columns holds), and **the spend
    off an installed card is about the card** — the first a payment took
    credits from (`CreditsSpentFromOutsidePool::first_host`), so Keiko's
    "spend credits from an installed companion card" is a `when` and shares
    its first time with "install a companion card". The Twinning's entry
    now names `subject: Any`.
  - **A discount that is an amount** (`Discount::Amount`, read by
    `ability::discount_credits` at the offer and the install alike): Pauleʼs
    Café's "1[credit] less for each unique connection resource". Its "the
    first card you install this way during each of your turns" is two
    abilities on one use limit — the discounted one `And(DuringYourTurn,
    OncePerTurn)`, the plain one its `Not` — and `validate` now counts only
    the abilities that *spend* a `OncePerTurn` (`spends_once_per_turn`).
  - **A rig card's own install is counted on its copy**
    (`CopyTurn::counts_on_rig`), and `ActingCardMatches` asks the copy too,
    so Euler's "only if this program was installed this turn" is
    `ActingCardMatches(InstalledThisTurn)`.
  - **Who the Runner is:** `EffectRequirement::IdentityMatches` (DreamNet's
    "if your identity is digital") and `Amount::Link` ("at least 2[link]"),
    with the "or" written `Not(And(Not, Not))`.
  - **A random card from either hand:** `CardTarget::RandomFromHq` became
    `RandomFromHand(Side)` (Heliamphora writes `Corp`), for Mystic Maemi's
    "trash 1 card from your grip at random".
- **What each is made of.** Maemi and Poemu bank a credit at turn start and
  on a steal and answer "when your turn ends" with `OnDiscardPhaseEnd` (CR
  5.7.2d); Maemi asks the random grip card or itself, and takes itself
  unasked from an empty grip. Penumbral Toolkit is Carmen's self-discount
  on Deep Dive's "a successful run on HQ this turn" and Open Market's load,
  trashed when empty, paying during runs. Odore and Euler are breakers whose
  free break is gated: three virtual resources (`RunnerInstalls`), the
  turn of the install. Cybertrooper Talut is The Toolbox's link and
  Cookbook's "it" (`acts_on_subject`) with Living Mural's turn-long
  strength. Hoshiko is Dewi's flip with `TimesThisTurn(OnAccessed)` on each
  side's turn end.
- **Two engine gaps the stage found.** An event's and an operation's play
  price was paid and never dispatched, so credits spent on a play off a
  card reached no listener; and a psi bid paid off Methuselah's hosted
  credits during a run (the debug audit, on a seed the new deck re-paired)
  had the same gap. Both now dispatch their cost's events after the
  effect, as every payer does.
- **Fidelity limits:** under Known limits. Hoshiko's back has the front's
  subtypes and link (the catalog keeps the flip side's text alone); Keiko
  counts companion resources, and a payment's companion only when it gave
  first; Pauleʼs Café's discount is spent by a discounted use that installs
  nothing; Maemi's empty grip takes the resource unasked.
- **Client.** Nothing added to the view or a decision: the spend event's
  new field is drawn nowhere, as the event never was. `prose` reads the new
  words (`describe_pays_for`, the link, the grip's random card, an amount
  discount).
- **Decks.** Side Quest is Pay As You Go's frame on Hoshiko: two Mystic
  Maemi, two Paladin Poemu, two Keiko, two Odore and a DreamNet for its Take
  a Dive, Time Bomb, Friend of a Friend, Chastushka and a Valentina Ferreira
  Carvalho. Safety Net takes two Euler and two Mantle for its Coalescence
  and Flux Capacitor; Dead Reckoning two Cybertrooper Talut and two Pauleʼs
  Café for its Burner and Environmental Testing; Hit List two Penumbral
  Toolkit for two Underdome Irregulars; Mixtape, a singleton deck on Nova (a
  digital identity), a DreamNet for its Side Hustle. Borrowed Time, Vic's,
  was the first choice for DreamNet and is pinned Startup-legal, which an
  Uprising card is not. Every card given up is still in another deck.
- **DSL ratio** (`pool_status.py`): 16 of 98 `Effect` variants single-use,
  1 unused (`Trace`), over 529 card files — from 17 of 99 over 518.
- **Real play** (`--headless`, each edited Runner deck against Hostile Bid,
  seed 2; random seats 96 games, planner seats 48; each pair random /
  planner). Mystic Maemi installed 33 / 20 times, its turn-end clause
  37 / 25; Paladin Poemu 40 / 11, its turn-end trash 25 / 5; Keiko 23 / 17,
  its credit for an install 5 / 29 and for a spend 16 / 74; Odore 7 / 16
  installs, 2 / 16 uses; DreamNet on Hoshiko 12 / 2, on Nova 18 / 0, its
  draw 68 / 5 and 87 / 0; Hoshiko's turn-end abilities 377 / 230 and the
  flipped turn start 592 / 392; Euler 40 / 28 installs, 13 / 77 uses;
  Mantle 48 / 24 installs; Cybertrooper Talut 24 / 0, its strength 9 / 0;
  Pauleʼs Café 26 / 24 installs, 220 / 124 uses; Penumbral Toolkit 22 / 16.
  Talut and DreamNet go on the bot debts.
- **Measured.** `cargo test --workspace` green (2,652) and clippy silent.
  Both sweeps are green at 256 seeds, the card gate included.
  `coverage_identical.py` against Stage 1 (7b76d22, 192 games a report):
  random seatings differ only by the rename — Madani's 94 hosted installs
  are `InstallRunnerCardFromZone` (202 → 296), `InstallRunnerCardFromHost`
  94 → 0 — view and index alike. The planner seatings move by
  `determinize`'s larger prior, with every end reason unchanged (Corp agenda
  wins 58, flatlines 25; Runner agenda wins 105, deck-outs 4).

#### Stage 3 — Runner triggers and zones (4 October 2026)

`claude/serene-einstein-6bhlig`: Swift, Aniccam, Buffer Drive, The Back,
Prognostic Q-Loop, Simulchip, Harmony AR Therapy, Devil Charm, Bravado and
Cordyceps — ten Runner cards. **No new `Effect`.** Uprising 31 of 65, its
booster pack 3 of 7 (Swift is reprinted there); `UR_UNIMPLEMENTED` 44 →
34, `URBP_UNIMPLEMENTED` 5 → 4.

- **The words.**
  - **A played event is trashed as it finishes resolving** (CR 3.7.1):
    `TrashedFrom::PlayArea`, the rules' trash (`by: None`) and the one that
    is a moment, so Aniccam's "an event is trashed (from any location)"
    hears it. **`EventFilter::Anyone`** makes a "you trash" passive — the
    moment anyone's or nobody's — for Aniccam and for Simulchip's "if an
    installed program has already been trashed this turn"; two entries, one
    per player, would both have counted the played event's trash, so
    neither would have been the first.
  - **The Runner's grip and stack trash in batches**
    (`GameEvent::CardsTrashedFromGripOrStack`, `Trigger::
    OnCardsTrashedFromGripOrStack`, passive): one per instruction — damage,
    a mill, a card's trash, a selection to the heap, a trash cost — naming
    its cards, which are written into the trigger's effects as "those
    cards" (`CardFilter::TrashedThisWay`) as it fires, as a mill's `then`
    already had them. Buffer Drive chooses one to the bottom of the stack.
    `AddToDeck` now finds a chosen heap card when the install resolving is
    the card that parked the selection.
  - **Using any paid ability is a moment** (`Trigger::OnAbilityUsed`, CR
    9.1.6), heard as the cost is paid; `OnActionTaken` is the same moment
    narrowed to an action, and an action is never taken during a run.
  - **"Up to" an amount on a selection** (`PromptChooseCards::up_to`, The
    Back's "for each hosted power counter, choose up to 2") and **a card
    with a [trash] ability** (`CardFilter::HasTrashAbility`,
    `Cost::prints_trash`).
  - **A completed run remembers the ice it passed**
    (`CompletedRun::ice_passed`, `Amount::IcePassedLastRun`), which Bravado's
    run-end gain reads after the run has left `active_run`.
  - **Run events have a turn-log column** (`Kind::RunEvent`, and
    `DoubleRunEvent` for Maintenance Access so a double and a run event each
    still read it): the seventeenth and eighteenth kinds, which widened
    `Occurrences::columns` to a `u64`. Swift's first run event each turn.
  - **One printed ability may share a use limit across entries**: `validate`
    now lets triggers with the same printed `text` spend one `OncePerTurn`
    (The Back's two ways of using hardware).
- **What each is made of.** Swift and Aniccam are consoles with +1[mu];
  Prognostic Q-Loop is a first run each turn's look (`LookAtTopOfDeck`) and
  a once-per-turn reveal of the top card installed if it is a program or
  hardware; Simulchip is two abilities on complementary requirements, as
  Pauleʼs Café's are — `TrashSelf`, or `AllOf[TrashSelf, Trash(installed
  program)]` until an installed program has been trashed this turn — into
  Privileged Access's discounted heap install; Harmony AR Therapy is
  Asmund Pudlat's `DifferentNames` into the stack, removed from the game;
  Devil Charm is Malandragem's removal from the game into a run-long −6 on
  the encountered ice; Bravado is Kompromat's server choice with a run-end
  gain; Cordyceps is two counters on install and, once per turn on a
  central success, a counter for Sipa's swap of the ice protecting that
  server.
- **A stack the turn log grew.** The log is held twice in every state, and
  the planner's PUCT search (`dividends_over_advance`, 2,048 iterations,
  depth 16) overflowed a debug test thread's 2 MB: each recursive frame of
  `puct::simulate` kept slots for the states it cloned and stepped. Those
  are now made in a callee that is gone before it recurses
  (`puct::add_child`, never inlined); the test that needed 2.2 MB now runs
  in 1.2.
- **Fidelity limits:** under Known limits. Aniccam hears a run event's
  trash as its server is chosen, before its run, and no operation's;
  Buffer Drive's batches are per instruction; The Back's first time is a
  use limit (the second card deferred on "during a run", after Ryō
  "Phoenix" Ōno); Simulchip counts no trash the rules made.
- **Client.** Nothing added to the view or a decision. The log reads a
  played event's trash as "… is trashed" and a batch as "trashed N
  card(s) from the grip or stack"; `prose` reads the new words.
- **Decks.** Hit List takes two Bravado, a Swift, The Back and a Prognostic
  Q-Loop for its two Info Bounty, a Red Team and two Tunnel Vision; Safety
  Net two Simulchip, two Cordyceps, an Aniccam and a Harmony AR Therapy for
  its two Hyperbaric, two Propeller and two Orca; Side Quest a Devil Charm
  for its Valentina Ferreira Carvalho; Burn Rate a Buffer Drive for one of
  its two Hush. Every card given up is still in another deck.
- **DSL ratio** (`pool_status.py`): 16 of 98 `Effect` variants single-use,
  1 unused (`Trace`), over 539 card files — unmoved.
- **Real play** (`--headless`, each edited Runner deck against Hostile Bid,
  seed 2; random seats 96 games, planner seats 48; each pair random /
  planner). Bravado played 13 / 0 times; Swift installed 15 / 9, its click
  7 / 3; The Back 19 / 1, never charged (Hit List's hardware is rarely used
  in a run), its own ability used 14 / 0; Prognostic Q-Loop 13 / 2, its
  look 105 / 6 and its reveal 103 / 4; Simulchip installed 37 / 37, used
  37 / 14; Cordyceps 35 / 1, its offer 33 / 0; Aniccam 10 / 9, its draw
  3 / 7; Harmony AR Therapy played 10 / 1; Devil Charm 13 / 3, its offer
  10 / 4; Buffer Drive 9 / 0, its removal used 9 / 0. Bravado, The Back,
  Cordyceps and Buffer Drive go on the bot debts.
- **Measured.** `cargo test --workspace` green (2,662) and clippy silent.
  Both sweeps are green at 256 seeds, the card gate included.
  `coverage_identical.py` against Stage 2 (7ea5887, 192 games a report):
  random seatings differ only by the new records — every played event now
  a `CardTrashed` (2,028 → 2,506, each event's `trashed` count with it) and
  270 `CardsTrashedFromGripOrStack` — view and index alike; the planner
  seatings drift by the larger prior with every end reason unchanged.

#### Stage 4 — Corp server and advancement words (4 October 2026)

`claude/serene-einstein-6bhlig`: La Costa Grid, Cayambe Grid, Tranquility
Home Grid, Digital Rights Management, Vaporframe Fabricator, Wall to Wall,
Kakurenbo, False Lead and Cyberdex Sandbox — nine Corp cards. **One new
`Effect`** (`TurnArchivesFacedown`, Kakurenbo's), and one fewer
single-use (`AddToHand`, now Wall to Wall's too). Uprising 40 of 65, its
booster pack 6 of 7 (La Costa Grid, Digital Rights Management and Cayambe
Grid are reprinted there); `UR_UNIMPLEMENTED` 34 → 25,
`URBP_UNIMPLEMENTED` 4 → 1.

- **The words.**
  - **"The first time each turn you install a card in the root of this
    server"** (`EventFilter::InRootOfThisServer`, Tranquility Home Grid),
    **counted on the copies in that root**: `turn_log::record` bumps
    `OnInstall` on every card in the root a Corp card went into, the one
    installed included (`CopyTurn`), and the listener scan reads the
    listener's own copy. Exact, because a card in the root all turn saw
    every install into it, and one installed this turn saw its own, which
    already makes any later install the second. The turn log counts kinds
    of card and never a server, so `first_each_turn` could not narrow to
    one; a board read (`installed_this_turn` in the root) would have made a
    fast-advanced agenda's replacement the "first" again once the agenda
    left for the score area.
  - **A cost that forfeits its own agenda** (`Cost::ForfeitSelf`, False
    Lead): paid as `Forfeit` pays — out of the game with its points
    (CR 8.2.5, 4.9.3) — and its effect resolves with the agenda gone.
  - **A scored agenda's ability opens a window** (`paid_ability::
    active_cards_of` now lists the Corp's scored agendas, CR 1.8.3a,
    4.5.4): `legal_actions` already offered their abilities, but the
    window between the Runner's actions opened only for a rezzed install's,
    so False Lead could be forfeited during a run and never between the
    Runner's actions, where it is played.
  - **"This server" in a count of installs** (`Amount::CorpInstalls` writes
    the counting card's server in, as a selection already did): Cayambe
    Grid's "2[credit] for each advanced piece of ice protecting this
    server" is `CreditsAmount(Increased { n, n })` over `All([Ice,
    InThisServer, Advanced])`.
  - **"You cannot install that card in the root of this server"**
    (`PromptInstallCorpCard::not_in_root_of`, `dsl::ThisServer`,
    Vaporframe Fabricator): bars that root alone — ice may still protect
    the server — and only while the server exists (CR 4.6.8e: a remote
    nothing is left in has ceased to exist, and its number may be the new
    remote's). Heard as the asset is trashed, so "this server" is the one
    the trash names (on `CardTrashed`, or the run for an access), written
    into the install (`Effect::with_this_server`) as the selection ahead of
    it parks: a continuation keeps no triggering event. `another_server`
    would have barred the ice too and reads an install already gone.
  - **"Add this asset to HQ"** is `AddToHand` on a Corp install (Wall to
    Wall's fourth option), which moves the card as `Cost::AddSelfToHq`
    does.
  - **"Turn all cards in Archives facedown"** (`Effect::
    TurnArchivesFacedown`, Kakurenbo): nothing turned a card in Archives
    facedown — a card goes there faceup or facedown as it is trashed (CR
    4.4.6b), and the breach and `TurnFaceupInArchives` turn them the other
    way. No event: no card hears it, and the view is the record.
- **Two bugs the stage reached, fixed.** Tranquility Home Grid is the
  first card to hear a Corp install beside Engineering the Future, so the
  first to park a trigger order on one, and both bugs were in that order:
  - **An order parked inside an encounter finished nothing after a trigger
    that asked** (`pending_choice::resolve_choose_trigger_to_resolve`):
    Ansel 1.0's install into the grid's root parked the order, the grid's
    own "gain 2[credit] or draw 1 card" parked behind it with no word of
    the subroutines still to fire, and the encounter stood with its last
    subroutine pending and no player able to act — a planner game of
    Retirement Package against Safety Net, seed 2, the one stall in 48.
    The order now carries the intent onto what its trigger parks, as a
    resolved choice already did (`mark_parked_resume_subroutines`).
    Neither sweep had found it at 256 seeds; the real-play pass below did.
  - **A trigger order named a facedown card to the Runner and a
    spectator** (`masking::mask_pending_decision`): each queued entry
    carries the event it heard, and the install's event named the card.
    The entries' events are now masked as the log masks them. Found by
    the session sweep's fog gate at 256 seeds (seed 89, Retirement Package
    against Planning Ahead).
- **What each is made of.** La Costa Grid is a turn-start selection
  `InRootOfThisServer`; Cayambe Grid one over its ice and an approach
  offer asked only when some ice protecting it is advanced (paying 0 is
  always taken); Digital Rights Management is `TimesLastTurnWhen(
  OnSuccessfulRun, Server[Hq])` as its play requirement, Pivot's search
  for an agenda, Peer Review's remote-root install and `Prohibit(
  ScoreAgendas, Turn)`; Vaporframe Fabricator a once-per-turn [click]
  install from HQ ignoring all costs and the same install on its trash,
  heard both from an access (`OnTrashedFromAccess`) and from a Runner
  card's text (`OnCardTrashed`, `Whose(Runner)`); Wall to Wall is two
  `ResolveSomeOf`s on complementary conditions — at least two rezzed
  assets, itself among them, or not — the "otherwise" first, because
  adding itself to HQ is one of the options and would otherwise flip the
  condition between the two; Kakurenbo is Longevity Serum's trash from HQ,
  the facedown turn and a selection from Archives into a remote root with
  two counters (`if_installed`), removed from the game; Cyberdex Sandbox
  is `OnVirusCountersPurged` with `first_each_turn` and Mavirus's "you may
  purge" on its score.
- **Fidelity limits:** under Known limits. Wall to Wall's "up to 3" is 3 of
  the 4, the counter's ice optional.
- **Client.** Nothing added to the view or a decision. False Lead is used
  from the score area's sheet, where scored agendas' abilities already
  were; the server prompt for Vaporframe's trash leaves out its old root.
  `prose` reads the new words.
- **Decks.** Retirement Package takes two Tranquility Home Grid for its
  two Brasília Government Grid and two Vaporframe Fabricator for its two
  Active Policing; Hostile Bid two Cayambe Grid for its two Flagship, two
  Wall to Wall for its two Cybersand Harvester and two False Lead for two
  of its three Greenmail; A Thousand Cuts two La Costa Grid for its two
  The Red Room, two Kakurenbo for its two Bring Them Home and two Cyberdex
  Sandbox for its two Lotus Haze; Paid Content two Digital Rights
  Management for its two Sudden Commandment. Every card given up is still
  in another deck.
- **DSL ratio** (`pool_status.py`): 16 of 99 `Effect` variants single-use,
  1 unused (`Trace`), over 548 card files — one variant more, the same
  single-use count.
- **Real play** (`--headless`, each edited Corp deck against Safety Net,
  seed 2; random seats 96 games, planner seats 48; each pair random /
  planner). Tranquility Home Grid installed 104 / 50, its first install
  heard 44 / 70 times; Vaporframe Fabricator installed 104 / 50, its
  [click] used 96 / 69, its trash heard 10 / 0 times; Cayambe Grid
  installed 94 / 44 and rezzed 51 / 2, its approach asked 59 / 0 times;
  Wall to Wall installed 160 / 60, resolved 121 / 130 times; La Costa Grid
  installed 52 / 37 and rezzed 26 / 5; Kakurenbo played 11 / 0; Digital
  Rights Management played 73 / 0; False Lead scored 1 / 10 and forfeited
  1 / 1; Cyberdex Sandbox installed 53 / 10 and never scored (stolen
  41 / 33). Kakurenbo, Digital Rights Management, False Lead's forfeit and
  Cayambe Grid's rez go on the bot debts. Before the subroutine fix one of
  the 48 planner games of Retirement Package stalled; none does now.
- **Measured.** `cargo test --workspace` green (2,690) and clippy silent.
  Both sweeps are green at 256 seeds, the card gate and the fog gate
  included. `coverage_identical.py` against Stage 3 (b22bf2b, 192 games a
  report): every seating differs, and the random seatings by one thing —
  the scored-agenda window, which Proprionegation's "use this ability only
  during a run" now gets before the jack-out decision (6.9.4b): paid
  ability windows 20,312 → 20,344, its uses unchanged at 2, view and index
  alike; one game's end moved (Runner agenda wins 114 → 113, deck-outs
  4 → 5). The planner seatings 32,500 → 33,112 windows, Proprionegation
  used 6 → 7, and the end reasons drift by the larger sample (Corp agenda
  wins 65 → 63, Runner 96 → 102). The subroutine fix moved no report: the
  four hashes before and after it are the same.

#### Stage 5 — break triggers, and the first card to start a trace (4 October 2026)

`claude/serene-einstein-6bhlig`: Gold Farmer, Makler, Týr, F2P, GameNET:
Where Dreams are Real, Scapenet and Transport Monopoly — six Corp cards
and one Runner card — and GameNET's Sweep deck, Pay to Win. **No new
`Effect`, and none left unused:** Scapenet's trace is `Trace`'s first
card. Uprising 47 of 65, its booster pack 6 of 7; `UR_UNIMPLEMENTED`
25 → 18.

- **The words.**
  - **A card's ability making a player spend or lose credits is a moment**
    (`Trigger::OnAbilityTookCredits`, `GameEvent::AbilityTookCredits`,
    GameNET's "whenever a Corp card ability causes the Runner to spend or
    lose at least 1[credit] during a run"): `AbilityGainedCredits`'s mirror,
    announced by one test (`ability::took_credits`, "at least 1" from any
    pool) at the four places a card asks: the cost of its ability
    (`engine::activate_ability` — F2P's 2[credit], paid by the Runner), a
    paid choice it offers (`pending_choice::resolve_accept` — "end the run
    unless the Runner pays 3[credit]"), a bid in the trace it began
    (`trace::submit_runner_bid`, CR 10.8.6d) and a loss its text resolves
    (`Effect::LoseCredits` — Gold Farmer's). Pushed among the cost events,
    so it is heard after what it paid for, where the Payment Rule hears
    every cost. GameNET names whose card and whose credits with
    `EventFilter::OwnedBy { owner: Corp, whose: Runner }`, and "during a
    run" is `RunInProgress`. The moment is generic, so every breaker's
    paid use now records one.
  - **A break says whether the subroutine was printed** (`GameEvent::
    SubroutineBroken::printed`, `IceFacts::printed_subroutine`, Gold
    Farmer's "whenever the Runner breaks a printed subroutine on this
    ice"): read off the event, as the strength is, because a parked
    trigger is asked again after the run has moved on. A fifth fact
    doubles the ice facts to 32 columns, which the turn log's 36 hold.
  - **A run can be kept from being declared successful by a card used
    during it** (`Prohibition::DeclaredSuccessful` for `Run`, Transport
    Monopoly's "This run cannot be declared successful"), asked by
    `continuous::may_be_declared_successful` beside Flagship's standing
    word, so the breach still follows (CR 6.9.5b) and the run is not
    unsuccessful (6.8.4a). A standing kind with a duration would have had
    to outlive the agenda's use that made it.
- **The first trace in play.** Scapenet is the pool's first card to start
  one, so `rules/trace.rs` was read against CR 10.8 (conformance row):
  10.8.1–10.8.4 and 10.8.6 match; 10.8.5's "if unsuccessful" and
  10.8.6a's "when a trace is initiated" are unbuilt and no built card
  prints them. The trace bids left `ACTIONS_UNREACHABLE_WITH_SAMPLE_DECKS`,
  which is now empty, for `ACTIONS_RARE_WITH_SAMPLE_DECKS` at 512 games;
  the index-path sweep, which plays System Gateway alone, names them among
  what its decks cannot produce. The gate's stale-exclusion check is a
  function of the list now (`Coverage::stale_exclusions`), tested on a
  list of its own.
- **One leak the stage reached, fixed.** The fog gate at the default seed
  count named a facedown ambush to a spectator through the new moment —
  Cerebral Overwriter paid from its root (seed 26) and Esca accessed in HQ
  (seed 28) — because an ambush asks for credits face down. The event is
  withheld from whoever the card is concealed from, and from a spectator
  when the card is in HQ or R&D, as an access's `TriggerFired` is; the
  turn log counts the moment `Unseen` unless the card is the Runner's.
- **What each is made of.** Gold Farmer is two "end the run unless the
  Runner pays 3[credit]" and the break trigger; Makler a fracter with
  `OnIceFullyBroken`, `ByThis` and `first_each_turn` (Lobisomem's words);
  Týr Hákarl 1.0's Runner-only "Lose [click]" break with Aggressive
  Trendsetting's `AllottedClicksNextTurn` after it, and Ansel 1.0's trash
  with 3[credit]; F2P N-Pot's Runner-only paid break with `Not(IsTagged)`
  and Lethe's bounce; Scapenet `TimesLastTurn(OnSuccessfulRun)` as its
  play requirement and a selection of an installed chip or virtual card
  into the Runner's removed-from-game pile as `on_success`; Transport
  Monopoly Proprionegation's counters and window with `OncePerTurn`.
- **Fidelity limits:** under Known limits — GameNET's reading of
  "causes", and Transport Monopoly's `DuringRun`.
- **Client.** Nothing added to the view or a decision: F2P's and Týr's
  breaks are on the encountered ice as N-Pot's and Hákarl's are, a trace's
  bids were already actions, and Transport Monopoly is used from the score
  area's sheet. `prose` reads the new prohibition (in both its lists and
  the board's In effect line) and the printed-subroutine fact.
- **Decks.** Pay to Win is Paid Content's frame on GameNET: three Gold
  Farmer for its two Grubber and a Hype Machine, three F2P for its two
  Magistrate Revontulet and the other Hype Machine, two Scapenet for its
  two The Powers That Be (4 of 17 influence, 20 points). Retirement
  Package takes Týr for its Sleipnir; Hostile Bid and Tag You're It each a
  Transport Monopoly for their Orbital Superiority; Encore two Makler for
  its two Marjanah. Every card given up is still in another deck.
- **A planner test re-stated.** Five more Corp cards in the prior re-drew
  every R&D sample, and the kill Corp tagged a full grip with Public Trail
  on seed 2 of the twenty `a_kill_corp_plays_public_trail…` counts. Over
  sixty seeds it is 0 before the stage and 2 after (seeds 2 and 35), so
  that claim is now counted over sixty, as the balanced Corp's was at
  Stage 1; against three cards it is still twenty of twenty.
- **DSL ratio** (`pool_status.py`): 17 of 99 `Effect` variants
  single-use, none unused, over 555 card files — `Trace` moved from unused
  to single-use.
- **Real play** (`--headless`, seed 2; random seats 96 games, planner
  seats 48; each pair random / planner). Pay to Win against Safety Net:
  GameNET paid 40 / 44 times; Gold Farmer installed 174 / 64 and rezzed
  26 / 50, its subroutines fired 87 / 110 and none broken, so its break
  trigger was never heard; F2P rezzed 19 / 34, its break bought 5 / 16
  times; Scapenet played 44 / 0, its trace successful 43 times and avoided
  once. Retirement Package against Safety Net: Týr rezzed 4 / 5 and its
  click break used 4 / 5 times. Transport Monopoly (Hostile Bid and Tag
  You're It against Safety Net) scored 0 / 4 and 1 / 12, used 0 / 1 and
  2 / 1 times, and kept a run from being declared successful in three of
  those games. Makler (Encore against Hostile Bid) installed 15 / 13, used
  2 / 48 times, its credit for a full break 1 / 23. Scapenet, Gold
  Farmer's trigger and Transport Monopoly's counters go on the bot debts.
- **Measured.** `cargo test --workspace` green (2,698) and clippy silent. Both
  sweeps are green at 256 seeds, the card gate and the fog gate included;
  the view sweep's 768 games bid in 7 traces. `coverage_identical.py`
  against Stage 4 (63dee5f, 192 games a report): the random seatings are
  identical but for the new moment itself (`AbilityTookCredits` 382, view
  and index alike) — no game moved, since the pool pass plays no Sweep
  deck. The planner seatings move by the larger prior, as the Public Trail
  test did: steps 111,460 → 110,565, paid ability windows 33,112 → 32,889,
  Corp agenda wins 63 → 66 and flatlines 23 → 24, Runner agenda wins
  102 → 98.

#### Stage 6 — stealth credits on VP's rule, a remembered choice, set aside (4 October 2026)

`claude/serene-einstein-6bhlig`: Mu Safecracker, Afterimage, Penrose,
Boomerang, Engram Flush and Gachapon. **Two new `Effect`s**
(`RevealHand`, `Remember`). Uprising 53 of 65, its booster pack 6 of 7.
(Written into the archive at Stage 7a, from the stage's commit, 3c4fcf5;
the stage closed without one.)

- **The words.**
  - `Effect::Remember` (Boomerang, Engram Flush): a choice that makes
    nothing but itself, read back by another of the card's abilities (CR
    9.10.3) — Boomerang's ice until it leaves the table (9.10.3c), Engram
    Flush's card type for the encounter its text names.
  - `Effect::RevealHand` (Engram Flush): "reveal the grip" reveals every
    card. A selection over the grip showed only what could be chosen, so a
    grip with nothing of the named type was never revealed.
  - Paying a card's own conditional "you may pay" is using that card (CR
    9.1.6): `pending_choice::resolve_accept` states `Purpose::Ability`, so
    Mantle's credits pay Mu Safecracker's access. Another player's "unless
    you pay" stays `Other`.
  - A run's end says whether it was successful, where "not unsuccessful"
    admitted a run kept from success (Boomerang).
  - `CardTarget::SetAside`: what Gachapon leaves after an install and a
    shuffle is removed from the game; three or fewer are all shuffled back.
- Afterimage joins the cards whose "once per turn" is spent when its "may"
  is declined (the recorded CR 9.3.6g deviation); Boomerang breaks two
  when two are left.
- **Decks.** Hit List +Mu Safecracker, +2 Afterimage, +2 Boomerang; Spare
  Parts +2 Penrose; Side Quest +2 Gachapon; Second Site +2 Engram Flush.
  Every card given up is still in another Sweep deck.
- **A planner test re-stated.** `a_kill_corp_plays_public_trail_because_
  the_runners_answer_is_priced` counts the balanced Corp over 180 seeds:
  over sixty it stood at 27 to 30 against a bar of under 30, and which
  depended on the build (debug with or without debuginfo, release) on the
  same source before the stage as after. Over 180 it is 73 before and 78
  after, in each build.
- **Measured.** `cargo test --workspace` green (2,452 outside the desktop
  crate, 251 in it), clippy silent, both sweeps green at 256 seeds with the
  card gate. `coverage_identical.py` against Stage 5 (3f6beba): random
  identical, view and index; the planner moves by the larger prior — Corp
  agenda wins 66 → 65, Corp flatlines 24 → 23, Runner agenda wins
  98 → 100. Real play (seed 2, random 96 / planner 48): the planner never
  installs Mu Safecracker or Boomerang and uses Penrose once (bot debts);
  Afterimage bypasses 4 sentries, Gachapon is used 49 / 49 times, Engram
  Flush fires 72 / 178 subroutines.
- **DSL ratio**: 18 of 101 `Effect` variants single-use, none unused,
  over 561 card files.

#### Stage 7a — lockdown (5 October 2026)

`claude/serene-einstein-6bhlig`: SYNC Rerouting, Argus Crackdown, NAPD
Cordon, NEXT Activation Command and Hyoubu Precog Manifold — the five
lockdowns. **No new `Effect`.** Uprising 58 of 65, its booster pack 6 of
7; `UR_UNIMPLEMENTED` 12 → 7.

- **The words.**
  - **A lockdown stays in the play area** (CR 3.5.1c, 8.6.6c): "This
    operation is not trashed until your next turn begins" is a declaration
    (`CardDefinition::not_trashed_until_your_next_turn`, refused off an
    operation) that `engine::play_operation_card` reads, filing the card in
    `CorpState::play_area` with a handle from the install sequence
    (`PlayedOperation`) instead of Archives. It is active there (CR 1.8.3a:
    `rules::active` lists it, so its triggers are heard and its standing
    effects apply), its own "when you play this operation" is that copy's
    (`listeners::moments` pins the play to the handle), and it is trashed
    as the Corp's next turn begins — after the window before the turn
    (5.6.1b), before `TurnStarted` is heard (`turn::begin_turn`), as a
    trash from the play area. Public, and carried by the view and every
    bot sample.
  - **"Play only if there is no active lockdown"** is `ZoneHasAtLeast`
    over `CardZoneRef::PlayArea`, the zone both players share (CR 4.1.1b),
    with a subtype filter; nothing chooses from it.
  - **A server the card chose, and one protected by ice** are two
    `EventFilter`s on a server moment: `ChosenServer` (Hyoubu Precog
    Manifold's "a successful run on the chosen server", read off the
    lingering choice as `Mark` reads the mark) and `ProtectedByIce` (Argus
    Crackdown's, asked as the moment is heard, rezzed ice or not).
  - **A choice made as an operation is played lasts while that copy is in
    play** (CR 9.10.3c, `Until::WhileInPlay`): `ChooseServer`'s duration is
    the rules', read off where its card is — Tsakhia's, made when a turn
    begins by an ability that does nothing else, still ends with the turn
    (9.10.3b). Keyed by the copy's handle, so a second Hyoubu a turn later
    chooses afresh.
  - **A steal priced off the agenda's counters**: NAPD Cordon's
    "4[credit] plus 2[credit] for each advancement counter on that agenda"
    is `StealCost(CreditsAmount(Increased { Fixed(4), Times {
    AccessedCardAdvancementCounters, 2 } }))`, reckoned to a number of
    credits as the agenda is offered (CR 1.16.2b), so the offer the Runner
    reads is what they pay. `Amount::Times` is new: `Increased` of a count
    with itself reads as two counts where the card prints one at a rate.
  - **No card but an icebreaker breaks** (`Prohibition::
    BreakWithNonIcebreakers`, NEXT Activation Command's, standing as a
    `ContinuousKind::Cannot`): asked by both break effects of a breaker
    that is not one (`ability::breakable_now`) — Boomerang, Poison Vial, a
    bioroid's own "Lose [click]: Break". "Each piece of ice gets +2
    strength" is a `Strength` about `Scope::Ice(Any)`.
- **What each is made of.** SYNC Rerouting is Funhouse's "give the Runner
  1 tag unless they pay 4[credit]" on `OnRunStart`; Argus Crackdown 2 meat
  damage on `OnSuccessfulRun` when `ProtectedByIce`; Hyoubu Precog
  Manifold `ChooseServer` on its own `OnPlay` and Adrian Seis's psi game,
  ending the run when the bids differ.
- **Client.** The lockdown is a line of `hud::in_effect` in both clients
  ("in play until the Corp's next turn: …"), the choice's duration reads
  "while it stays in play", and `prose` reads the new prohibition, zone
  and amounts. The view ledger names the play area as drawn there.
- **Decks.** Pay to Win two SYNC Rerouting for two Nonequivalent Exchange;
  Tag You're It two Argus Crackdown for two Hedge Fund; Retirement Package
  two NEXT Activation Command and two NAPD Cordon for its two Reverb and two
  Lycian Multi-Munition; A Thousand Cuts two Hyoubu Precog Manifold for two
  Vicsek. Every card given up is still in another Sweep deck.
- **DSL ratio** (`pool_status.py`): 17 of 101 `Effect` variants
  single-use, none unused, over 566 card files — `ChooseServer` has its
  second card.
- **Real play** (`--headless`, each deck against Safety Net, seed 2;
  random seats 96 games, planner seats 48; each pair random / planner).
  SYNC Rerouting played 106 / 0, its tax asked 276 times under random;
  Argus Crackdown played 61 / 1, its damage done 28 times; NEXT Activation
  Command played 84 / 4; NAPD Cordon 69 / 0; Hyoubu Precog Manifold played
  41 / 1, its psi game 6 times. The planner's turn ends at its own, and a
  lockdown pays on the Runner's: the five go on the bot debts.
- **Measured.** `cargo test --workspace` green (2,459 outside the desktop
  crate, 251 in it) and clippy silent. Both sweeps are green at 256 seeds,
  the card gate included, so every lockdown was played. `coverage_
  identical.py` against Stage 6 (3c4fcf5, 192 games a report): random
  identical, view and index; the planner moves by the larger prior —
  Corp agenda wins 65 → 66, Runner agenda wins 100 → 99, steps
  110,002 → 110,068.

#### Stage 7b — an encounter away from the run's position (5 October 2026)

`claude/serene-einstein-6bhlig`: Konjin and Ganked!. **No new
`Effect`**: both say "The Runner encounters that ice" with Sisyphus
Protocol's `ForceEncounter`, which now has three cards. Uprising 60 of 65,
its booster pack 6 of 7; `UR_UNIMPLEMENTED` 7 → 5. Stage 7 is complete.

- **The words.**
  - **A forced encounter away from the Runner's position** (CR 6.1.3c,
    6.5.9a–c): `ForceEncounter` from inside an encounter or an access
    (`run::engine::force_encounter_elsewhere`) encounters a rezzed piece of
    ice anywhere without moving. What it interrupted — the run's ice, its
    position and the encounter or access it was in — waits on
    `RunState::suspended` while the run stands on a list of the one forced
    piece of ice, so every reader of "the ice being encountered" (fifty of
    them across the engine, the bots and the clients) reads it with no
    change. Its end puts the interrupted state back
    (`run::engine::resume_suspended`, from the pass that would have been
    and from `reconcile_ice` when the ice leaves or is derezzed), passing
    nothing: Konjin's encounter finishes, its window opened by whoever took
    the step. "End the run" ends both (6.5.9b); `end_run` restores the
    run's own ice before it snapshots the run. A list, because a forced
    encounter can force another (Ganked! into Konjin). Sisyphus
    Protocol's encounter from the movement phase keeps its own path.
  - **An access ends when its card leaves by a card's text resolved above
    its decision** (CR 7.1.7): Ganked! is trashed as the cost of the
    Corp's "you may", which resolves above the access decision already
    presented. `AccessState::left` says so for a card in HQ or R&D (an
    install says so by leaving the table), set in `ability::
    trash_this_card` and cleared at each presentation, and the breach moves
    on once nothing stands in the way (`access::move_on_if_left`, asked by
    `engine::resume_run`) — after the encounter the trash began.
  - Public: the suspended state (its ice masked as the run's is,
    `PublicSuspendedEncounter`) and `left` ride in the view, and every bot
    sample carries both, so a sample returns where the real game will.
- **What each is made of.** Konjin is Adrian Seis's psi game on its own
  `OnEncounter`, with "you may choose another rezzed piece of ice" a
  `PresentChoice` over a selection of `Ice`, `Rezzed`, `NotSourceCard`
  whose `then` is `ForceEncounter`; it has no subroutines, so it is fully
  broken as it is encountered (CR 6.5.7c). Ganked! is Mavirus's reveal in
  R&D and an `OnAccessed` `OfferPaidChoice` costing `TrashSelf`, its
  selection `Ice`, `Rezzed`, `InAttackedServer`. A known limit, on the
  conformance row: an effect lasting "the remainder of this encounter"
  made in the interrupted encounter before the forced one is swept during
  it; nothing in the pool makes one before Konjin's psi game resolves.
- **Client.** `hud::in_effect` says where a forced encounter returns to
  ("This run: after this encounter, back to the encounter with Konjin" /
  "to the breach"), in both clients; the view ledger names `suspended`
  drawn there and `left` the engine's. Ganked! joins the traps whose rez
  gains nothing (`board::rez`).
- **Decks.** A Thousand Cuts takes Konjin for its Ezam; Pay to Win two
  Ganked! for two Your Digital Life. Every card given up is still in
  another Sweep deck.
- **DSL ratio** (`pool_status.py`): 16 of 101 `Effect` variants
  single-use, none unused, over 568 card files.
- **Real play** (`--headless`, each deck against Safety Net, seed 2;
  random seats 96 games, planner seats 48; each pair random / planner).
  Ganked! installed 99 / 51 and accessed 200 / 149 times; Konjin installed
  36 / 20, rezzed 10 / 10, its psi game played 23 / 46 times. The planner
  rezzes Ganked! (51 of 51 installs), a trap whose rez gains it nothing:
  on the bot debts.
- **Measured.** `cargo test --workspace` green (2,464 outside the desktop
  crate, 251 in it) and clippy silent. Both sweeps are green at 256 seeds,
  the card gate included, so Konjin and Ganked! were played. `coverage_
  identical.py` against Stage 7a (6b2aca3, 192 games a report): random
  identical, view and index — the encounter's new path is reached by no
  game the pool pass plays; the planner moves by the larger prior — Corp
  agenda wins 66 → 70, Runner agenda wins 99 → 95, steps 110,068 →
  112,214.

#### Stage 8 — points, subroutine lists and run costs that change mid-game (5 October 2026)

`claude/serene-einstein-6bhlig`: Megaprix Qualifier, Project Vacheron,
Earth Station: SEA Headquarters, Akhet and Winchester, and Earth
Station's Sweep deck, Ground Control. **No new `Effect`.** Uprising 65 of
65, its booster pack 7 of 7; `UR_UNIMPLEMENTED` 5 → 0 and
`URBP_UNIMPLEMENTED` 1 → 0. **Uprising is complete.**

- **The words.**
  - **An agenda's worth is asked of the copy.** `continuous::Target::
    Scored` names the copy's handle, so a `while` reads its own counters:
    Megaprix Qualifier's "while this agenda has a hosted agenda counter,
    it is worth 1 more agenda point" (`AgendaPoints` on `This`, which
    `validate` now admits for an agenda in either score area) and Project
    Vacheron's "worth 0" in the Runner's. The scored-agenda lookup the
    counter effects use reads both score areas (`ability::acting_scored`),
    since a stolen agenda keeps a handle of its own.
  - **The stored score is the score** (`win::refresh_tallies`, run by
    the checkpoint beside the win check). `PlayerResources::agenda_points`
    — what the view, the bots and the tempo report read — was added to as
    an agenda landed, at what it was worth then; Megaprix's counter is
    placed by its score's own trigger and a stolen Vacheron is worth 3 again
    once its last counter is gone, so the tally would have been wrong
    after either. The win check always read the score itself.
  - **A replacement as an agenda enters the Runner's score area** (CR
    9.9.9c's own example): `CardDefinition::stolen_with_agenda_counters`,
    read by `run::access::resolve_steal` — the one way into that score
    area — which lands the agenda with its counters unless it came from
    Archives, so the checkpoint before the steal's triggers reads it worth
    nothing. `dividends`' shape and reason: what lands is decided as it
    lands. An `OnAgendaStolen` trigger would have placed the counters after
    that checkpoint, and a steal to 7 would have won on an agenda worth
    nothing.
  - **A trigger heard in the Runner's score area** (CR 4.5.4, "unless
    stated otherwise"; 1.14.4a, the Corp's): `TriggeredEffect::
    from_runner_score_area`, `from_heap`'s shape — the copy there listens
    for those triggers and nothing else (`Heard::FromRunnerScoreArea`), and
    a trigger pinned to it is still present there (`dispatcher::
    still_applies`). Vacheron's "When the Runner's turn begins, remove 1
    hosted agenda counter."
  - **An additional cost to run** (CR 6.3.2b): `ContinuousKind::RunCost`
    about `Scope::Runs(ServerKind)`, asked by `continuous::run_costs` and
    paid by `run::start_run` — the one door for the basic action and a
    card's text — as the server is announced, all at once (1.16.10b), before
    the run's bad publicity credits exist (6.3.3). A run that cannot pay
    is refused, so the basic action's probe never offers it. Earth
    Station's 1[credit] on HQ (front) and 6[credit] on a remote (flip side)
    are two effects under `while`s on which face is up; its [click] flip
    is an ability on the front and its flip back an `OnSuccessfulRun` on
    HQ on the back; "Limit 1 remote server" is A Teia's
    `RemoteServerLimit`.
  - **A trace's success that parks a choice resumes the subroutines after
    it** (`trace::submit_runner_bid` marks the parked choice, as the paid
    choice's resumer always did). Winchester's "trash 1 installed program"
    with two programs to choose from left the encounter with its later
    subroutines pending and nobody to act — a deadlock the per-card test
    found before any sweep.
- **What composes.** Akhet is Colossus's advanceable ice (`advancement_
  requirement: 0`) with a `Strength` and Hammer's `BreakLimit` under one
  `while` on its counters, and Seamless Launch's placement over any
  installed card. Winchester's subroutines are Scapenet's `Trace`, and its
  third Echo's `Subroutines` under Bathynomus's `Protecting`. Megaprix's
  "another copy in either player's score area" is two `ZoneHasAtLeast`
  over `AmongCards` joined by `Not(And(Not, Not))`, the own area counting
  this copy.
- **Client.** `ClientView::run_costs` lists what a run on each server
  costs beyond its click (the engine's answer, as `standing_cannot` is),
  and `actions::describe_action` words it on the action — "Run HQ (+1
  credit)" on the server's menu, the play helper and the terminal's list
  — and `explain_action` says it is paid as the server is announced. The
  ledger names it drawn there. A stolen Vacheron's counters and worth were
  already drawn in both score areas (`hud::ScoredCard::facts`,
  `scored_worth`), as is Earth Station's face. No ledger row opened.
- **Decks.** Ground Control, Earth Station's, on Hostile Bid's frame:
  three Akhet and three Winchester for its two Hostile Architecture, two
  Superdeep Borehole and two Svyatogor Excavator, assets that want remotes
  the identity does not allow; Hostile Bid keeps them. Retirement Package
  takes three Megaprix Qualifier and two Project Vacheron for its three
  Send a Message, the same 21 points at 47 cards; twenty decks keep Send
  a Message. Grassroots takes a third Eru Ayase-Pessoa for one of its two
  Corroder (Burn Rate, Pay As You Go and Side Quest keep it): a new Corp
  deck moves `sweep_decks_for_seed`'s schedule, and the view-path deep
  sweep then never installed Eru in 768 games, where 48 random games
  against Ground Control install it 18 times.
- **Fidelity.** A run a card's text makes that cannot pay is refused
  (1.16.10a: it would not happen), on the deviations list and the 6.3
  row; Vacheron's "worth 0" is −3 off its printed 3; Earth Station's cost
  is paid before a run's own pools exist, and GameNET does not hear it.
- **The fog gate's selection check is per candidate.** The schedule's
  move also gave seed 221 Cataloguer's look at the top of R&D naming a
  Tithe while another sat facedown on the table; the gate compared card
  names, so a card the Runner may see failed it. A candidate on an
  install the board masks must name no card (Tāo Salonga's leak, which
  the check was written for); one at a place in a zone names what the
  effect shows.
- **DSL ratio** (`pool_status.py`): 15 of 101 `Effect` variants
  single-use, none unused, over 573 card files.
- **Real play** (`--headless`, random seats, 48 games, seed 2). Ground
  Control against Grassroots: Earth Station's flip used 97 times, Akhet
  installed 78 times with 49 subroutines fired, Winchester installed 67
  with 28 fired. Retirement Package against Grassroots: Project Vacheron
  stolen 31 times and its countdown fired 68 times; Megaprix Qualifier
  scored 5 times, its counter placed once.
- **Measured.** `cargo test --workspace` green (2,478 outside the desktop
  crate, 264 in it, the desktop's built without debug info — the session's
  disk could not hold its debug binaries) and clippy silent. Both sweeps
  are green at 256 seeds, the card gate included. `coverage_identical.py`
  against `main` (d1d3857, 192 games a report): random identical, view and
  index; the planner moves by the larger prior — Corp agenda wins 63 → 64,
  Runner agenda wins 103 → 102, flatlines 22 → 23.

### 7. Downfall — 65 cards (C 19 / V 28 / M 18)

#### Stage 1 — Runner, composes (5 October 2026)

`claude/serene-einstein-6bhlig`: Isolation, Spec Work, Rezeki, Gauss, The
Artist, Az McCaffrey: Mechanical Prodigy and Stargate, and Az McCaffrey's
Sweep deck, Moonlighting. **No new `Effect`.** Downfall 7 of 65;
`DF_UNIMPLEMENTED` 65 → 58. Rezeki is Standard-banned and built anyway,
as every banned card before it.

- **The survey re-read first.** It called all seven "composes"; four
  are, and three needed one small word each, none of them an `Effect`:
  - **A use limit is the printed ability's** (CR 9.3.6g: "**an ability**
    with this flag can only be used once per turn"). The Artist prints two
    once-per-turn [click] abilities, and `OncePerTurnKey` was the card and
    which copy, so a use of one spent both — `validate` refused the card
    file for exactly that reason ("would share one use"). The key now
    names the printed paid ability too (`OncePerTurnKey::ability`, from
    `ResolutionContext::ability`, which every site that asks or spends an
    ability's requirement sets: `engine::activate_ability` and
    `activate_hand_ability`, `paid_ability::has_usable_paid_ability`,
    `prevention::could_prevent`). A card's triggers still share the card's
    one use (`ability: None`). Pauleʼs Café was the one card whose two
    entries are one printed ability — its plain install reads the
    discounted one's `OncePerTurn` through a `Not` — and it says so with
    `AbilityDef::part_of`, which `validate` holds to an earlier entry that
    names a `OncePerTurn`. The old refusal became two: two *triggers* that
    each spend the card's use, and two entries of one printed ability that
    each spend it. The alternative, a gate read off the turn log
    (`SameAction::Ability` counts each ability's actions), would have
    needed the ability's index on the context all the same, and would
    have left the key wrong for the next card with two limits that are
    not actions.
  - **The turn log counts job and connection resources apart**
    (`Kind::JobResource`, `Kind::ConnectionResource`, the sixth and seventh
    subtypes given a column; `Kind::COUNT` 18 → 20), and a first time can
    be narrowed by "A, B or C" (`kinds` reads `CardFilter::AnyOf` as the
    union of its parts). Az's "the first job resource, connection
    resource, or piece of hardware you install each turn costs 1[credit]
    less" is then Kate McCaffrey's `InstallCost` on an `Installing` scope
    with `first_each_turn`. No resource in the catalog is both a job and a
    connection, or either and a companion, so a card has one column. The
    installed columns' indices moved by two, which only a serialized log
    from before the change would misread; a match record replays its
    actions.
  - **A card a selection revealed lands in Archives faceup** (CR 4.4.6b:
    "visible to the Runner when it is trashed"). Stargate's "reveal the
    top 3 cards of R&D. Trash 1 of the revealed cards" is Cataloguer's
    selection of the Corp's R&D (`OpponentDeck`, `TopOfZone(3)`) with
    `reveal` and the Corp's discard as its destination, inside Chastushka's
    access replacement; `resolve_confirm_card_selection` filed it facedown,
    reading only whether the card was public before it moved. A cost's
    trash (`trash_as_cost`) already read `reveal`. No card before Stargate
    revealed a card into Archives by a selection.
- **What composes.** Isolation and Spec Work are Sell Out's additional
  cost (`Cost::Trash` over `OwnInstalled`) with a resource and with a
  program. Rezeki is a turn-start gain. Gauss is Living Mural's "+3
  strength for the remainder of the turn" on its own install and Rising
  Tide's barrier break, with a 2[credit] pump. The Artist's install is
  Topan's discounted install from the grip, narrowed to a program or a
  piece of hardware with `All`. Stargate's run is Conduit's [click] run
  of R&D with a once-per-turn limit.
- **Fidelity limits:** one, under Known limits. Stargate's three cards
  are shown to the Runner as the selection's candidates, and the Corp is
  shown only the card trashed.
- **Client.** Nothing added to the view, the log or a decision but the
  key's `ability`, which rides inside `once_per_turn_used` (an engine's
  line on the ledger already: the action list honours the limit, and
  `board::rig` tells copies apart by the install). No ledger row. The deck
  builder's set test offered Downfall to neither side; it is now offered
  to the Runner, and not to the Corp until Stage 2.
- **Decks.** Moonlighting is Picket Line's frame with Az for Mercury: Red
  Team a job; Smartware Distributor, Debbie “Downtown” Moreira and Hannah
  “Wheels” Pilintra connections; five kinds of hardware; and two Isolation
  for its two Strike Fund. Safety Net takes two The Artist for two Spree
  and two Gauss for two Aircheck; Dead Reckoning two Spec Work for two
  Lobisomem; Street Gallery two Rezeki for two Stowaway; Pay As You Go two
  Stargate for two Raindrops Cut Stone. Every card given up is still in
  another deck. Moonlighting is pinned Standard, Eternal and Casual, as
  Picket Line is.
- **DSL ratio** (`pool_status.py`): 15 of 101 `Effect` variants
  single-use, none unused, over 580 card files, as at Uprising's close
  over 573.
- **Measured.** `cargo test --workspace` green and clippy silent, the
  desktop crate included (its nine test targets built and run one at a
  time, because the container's disk could not hold them all at once). Both
  sweeps are green at 256 seeds, the card gate included, so every card of
  the five edited decks and Moonlighting was seen in play.
  `coverage_identical.py` against `main` (1c4c134, 192 games a report):
  random identical, view and index — the three engine words move nothing
  the sample decks reach; the planner moves by the larger prior, as at
  Uprising's close — Corp agenda wins 64 → 65, Runner agenda wins 102 →
  101, no other end reason.
- **The branch restarted from `main`.** Uprising Stage 8's PR was
  squash-merged (#360) and #361 landed after it, so the stage was carried
  onto `main` at 1c4c134 rather than stacked on the merged commit; #361
  touched only `netrunner_cli`'s diag code among the crates, which was
  tested and linted again on the new base.

#### Stage 2 — Corp, composes (5 October 2026)

`claude/serene-einstein-6bhlig`: Calvin B4L3Y, Nanoetching Matrix, CSR
Campaign, Tiered Subscription, Red Level Clearance, Roughneck Repair
Squad, Remastered Edition, Architect Deployment Test, Sandstone and SDS
Drone Deployment. **No new `Effect`, and no change to the engine:** card
files, decks and tests, and one line in a client test. Downfall 17 of 65;
`DF_UNIMPLEMENTED` 58 → 48. The survey's "composes" held for all ten.

- **What each is made of.** Calvin B4L3Y and Nanoetching Matrix are a
  once-per-turn [click] ability and Vaporframe Fabricator's "When the
  Runner trashes this asset", heard on access (`OnTrashedFromAccess`) and
  by any other trash the Runner makes (`OnCardTrashed`, `when:
  Whose(Runner)`), each a "may". CSR Campaign is a turn-start "may".
  Tiered Subscription is Tributary's first run each turn
  (`OnRunStart`, `first_each_turn`). Red Level Clearance is Key
  Performance Indicators' `ResolveSomeOf` two of four, its install
  Ablative Barrier's non-agenda install from HQ, paying the install cost.
  Roughneck Repair Squad's bad publicity is Luana Campos's
  `RemoveBadPublicity`, offered only while there is one to remove.
  Remastered Edition is Flower Sermon's agenda counter spent by a scored
  agenda's paid ability, into Key Performance Indicators' advancement
  counter. Architect Deployment Test is Hiram's look at the top of R&D
  (`LookAtTopOfDeck`, five) and Eminent Domain's install and rez ignoring
  all costs, over a selection of the top five that are not operations.
  Sandstone is Colossus's strength per counter with a −1 (`Strength { per:
  -1 }`) and a virus counter placed as it is encountered; a purge takes
  them, since its `counter_kind` is `Virus`. SDS Drone Deployment is the
  first `steal_cost` that takes a card (`Cost::Trash` over the Runner's
  installed programs, asked by the payment's replay when there is a
  choice) and Trust Operation's trash of a Runner install on scoring.
- **Fidelity limits:** one, under Known limits. Remastered Edition's
  "an installed card" offers the Corp's own installs, where the printed
  card admits the Runner's, on which a counter does nothing.
- **Client.** Nothing added to the view, the log or a decision, so no
  ledger line and no ledger row. The deck builder's set test now offers
  Downfall to both sides.
- **Decks.** Deterrence takes two Architect Deployment Test for an
  Offworld Office and a Send a Message (21 points to 20) and two
  Nanoetching Matrix for two Bran 1.0; Undertow two Calvin B4L3Y for two
  Active Policing and two Red Level Clearance for two Bran 1.0; Spin Cycle
  two Remastered Edition for two Freedom of Information, point for point,
  and two Tiered Subscription for two B-1001; Supply Chain two SDS Drone
  Deployment for two Send a Message, point for point, two Sandstone for
  two Kessleroid and two Roughneck Repair Squad for two PAD Campaign;
  Ground Control two CSR Campaign for two Wall to Wall. Every card given
  up is still in another deck, and every deck keeps the formats it was
  pinned to.
- **DSL ratio** (`pool_status.py`): 15 of 101 `Effect` variants
  single-use, none unused, over 590 card files, as over 580 at Stage 1.
- **Measured.** `cargo test --workspace` green and clippy silent, the
  desktop crate one test target at a time. Both sweeps are green at 256
  seeds, the card gate included. `coverage_identical.py` against Stage 1
  (1704593, on `main` at dc1ccb1; 192 games a report): random identical,
  view and index — no rule moved. The planner moves by the larger prior,
  more than Stage 1's seven Runner cards moved it: ten Corp cards change
  what the Runner's samples hold for every card it has not seen — Corp
  agenda wins 64 → 72, Corp wins by flatline 23 → 20, Runner agenda wins 102 →
  96, Runner deck-outs 3 → 4.

#### Stage 3 — trigger words and bad-publicity removal (5 October 2026)

`claude/serene-einstein-6bhlig`: Supercorridor, Fencer Fueno, Trickster
Taka, Congratulations!, Demolisher, Bukhgalter, Storgotic Resonator,
Masterwork (v37), Trebuchet and Increased Drop Rates. **No new `Effect`.**
Downfall 27 of 65; `DF_UNIMPLEMENTED` 48 → 38. Bukhgalter is
Standard-banned and built anyway.

- **The words.** Five, none of them an `Effect`, each what one printed
  clause needed:
  - **A card the Runner is accessing** (`Scope::Accessing`): Demolisher's
    "The trash cost of each Corp card is lowered by 1[credit]", asked about
    the accessed card itself (`continuous::trash_cost_delta`, beside the
    install's own `RootOfThisServer` question), so a card accessed out of
    HQ or R&D is cheaper too. Every other scope that reaches a Corp card
    reaches an install. `validate` admits it for a trash cost alone.
  - **A surcharge** (`Discount::Surcharge`): Masterwork (v37)'s "install 1
    piece of hardware from your grip, paying 1[credit] more". An
    effect's discount is signed now (`ability::discount_credits`,
    `engine::discounted`), read through the one door the offer and the
    install share; an extra credit paid first as a cost of its own could
    have been spent on an install the Runner then could not afford.
  - **Hosted credits for the rest of a successful run**
    (`PaysFor::DuringSuccessfulRuns`, Fencer Fueno): `DuringRuns` narrowed
    by `RunState::declared_successful`, as broad as the credit pool then.
  - **Hosted credits to use programs during runs**
    (`PaysFor::UsingDuringRuns`, Trickster Taka): `Using` *and*
    `DuringRuns`, because a card's words are alternatives and a program's
    ability used outside a run (Stargate's) is not covered. Within `Using`
    of a wider filter, for the order pools are spent in.
  - **The trashed card is of the Runner identity's faction**
    (`EffectRequirement::TriggeringCardOfRunnersFaction`, Storgotic
    Resonator), read off the triggering event; a `when` filter sees the
    card's definition alone, with no Runner identity to compare it with.
- **What composes.** Bukhgalter is Makler's first full break each turn
  (`OnIceFullyBroken`, `when: ByThis`). Congratulations! is Vertigo's pass
  trigger and a subroutine that pays both players. Demolisher's first
  trash each turn is two entries sharing one count, as Vaporframe
  Fabricator's two "the Runner trashes" are: a trash on access and any
  other trash of a Corp card the Runner makes (Active Policing's
  `OwnedBy { owner: Corp, whose: Runner }`). Fencer Fueno and Trickster
  Taka are Paladin Poemu's companion load, with Cloud Eater's paid choice
  ("pay 1[credit] or trash") and a choice of a tag or the card at three.
  Supercorridor's level credits are two `MoreThan`s under `Not`.
  Masterwork's first hardware each turn hears its own install, which CR
  9.6.5b allows: it is installed faceup, so active at the checkpoint that
  processes the install. Trebuchet is Scapenet's trace into Vertigo's
  `Prohibit { StealOrTrash, until: Run }`. Increased Drop Rates is Byte!'s
  reveal in R&D and Funhouse's "unless the Runner takes 1 tag" into Luana
  Campos's `RemoveBadPublicity`.
- **Fidelity limits:** one, under Known limits. Storgotic Resonator's
  "the first time each turn" is a use limit, because the turn log has no
  faction to narrow a first time by — the third card deferred on it, after
  Ryō "Phoenix" Ōno and The Back.
- **Client.** Nothing added to the view, the log or a decision. The new
  words are read out in `prose` (the card inspector's "Engine reads it
  as"): "paying 1 more", "for the remainder of a successful run", "to use
  a card matching program during runs", "each card the Runner accesses".
  Increased Drop Rates is an ambush that works face down, so
  `board::rez::gains_nothing` names it among the traps
  (`exactly_the_traps_gain_nothing_by_a_rez`): its rez stays legal and on
  its menu, and earns no glow. No ledger line and no ledger row.
- **Decks.** Moonlighting takes two Masterwork (v37) for two Pennyshaver;
  Encore two Bukhgalter for two Carmen (Bukhgalter is banned in Standard,
  as Nyusha is, so it went where Moonlighting's Standard legality was not
  at stake); Side Quest two Fencer Fueno for two Crash Space and two
  Trickster Taka for two Finality; Burn Rate two Demolisher for two
  Marrow; Dead Reckoning two Supercorridor for two Endurance; Spin Cycle
  two Congratulations! for two Grubber; Paid Content two Increased Drop
  Rates for two Hype Machine; Permafrost two Storgotic Resonator for two
  Front Company; Ground Control two Trebuchet for two Valentão. Every card
  given up is still in another deck, and every deck keeps its formats.
- **DSL ratio** (`pool_status.py`): 15 of 101 `Effect` variants
  single-use, none unused, over 600 card files, as over 590 at Stage 2.
- **Measured.** `cargo test --workspace` green and clippy silent, the
  desktop crate one test target at a time. Both sweeps are green at 256
  seeds, the card gate included. `coverage_identical.py` against Stage 2
  (d1fc733; 192 games a report): random identical, view and index — the
  five words move nothing the sample decks reach. The planner moves by
  the larger prior: Corp agenda wins 72 → 71, Corp wins by flatline 20 →
  21, Runner agenda wins 96 → 95, Runner deck-outs 4 → 5.

#### Stage 4 — amount, requirement and subtype words (5 October 2026)

`claude/serene-einstein-6bhlig`: Lat: Ethical Freelancer, Sting!, Daily
Quest, Fully Operational, Focus Group, Hagen, Vulnerability Audit, The
Nihilist and Blueberry!™ Diesel, and Lat's Sweep deck, Level Pegging.
**One new `Effect`** (`Repeat`). Downfall 36 of 65; `DF_UNIMPLEMENTED`
38 → 29. Sting! is Standard-banned and built anyway.

- **Game Over moved to Stage 6.** "Trash all installed non-icebreaker
  cards of the chosen type. For each card that would be trashed this way,
  the Runner may pay 3[credit] to prevent that card from being trashed"
  is one trash per card, each with its own payment, and nothing in the
  DSL loops over cards with a decision in each; Stage 6 is the stage of
  interrupts and costs to prevent, where the loop belongs.
- **The words.**
  - **`Effect::Repeat`** (Fully Operational's "Repeat this process for
    each remote server that has a card in its root and is protected by
    ice"): the count read once and the effect rewritten into a `Sequence`
    of that many copies, so each choice is made after the last one has
    resolved. A number chosen up front (`ChooseNumber`) would have decided
    every repetition before the first draw could inform the next.
  - **`Amount::InZone { zone, filter }`** (Focus Group's "the number of
    revealed cards of the chosen type"): `ZoneHasAtLeast`'s count as a
    number.
  - **`Amount::CopiesInScoreArea(side)`** (Sting!'s "copies of Sting! in
    the other player's score area"): by card, because its subtype, Ambush,
    is printed on other agendas.
  - **`Amount::CountersOnOwnInstalls(kind)`** (The Nihilist's "remove any
    2 virus counters from your installed cards", offered only when there
    are 2).
  - **`EffectRequirement::DuringYourActionPhase`** (Daily Quest's "Rez
    only during your action phase", a `rez_requirement`): not
    `DuringYourTurn`, which holds through the turn's first windows and its
    discard phase.
  - **`EffectRequirement::RunnerSucceededOnThisServerLastTurn`** and
    **`RunnerState::servers_run_successfully`** (Daily Quest's "if the
    Runner did not make a successful run on this server during their last
    turn"): a list beside `servers_run_this_turn`, cleared with it as the
    Runner's turn begins, so through the Corp's turn it is the Runner's
    last — the Turn History Rule's "which servers is a list", because the
    log counts the remotes as one class. Public, in the view, and copied
    by `determinize`.
  - **A selection's number reaches its `then`'s selection**
    (`with_chosen_number` over `PromptChooseCards::then`): Focus Group's
    X advancement counters on the card chosen after X. The gate
    `a_chosen_number_reaches_every_amount_a_card_writes` found it.
  - **An agenda can forbid its own score** (`ContinuousKind::Cannot(
    ScoreAgendas)` on `This`, `while: ActingCardMatches(InstalledThisTurn)`,
    asked by `continuous::cannot_install` of the install's own text):
    Vulnerability Audit. `validate` admits that one prohibition on `This`,
    and only on an agenda.
  - **`AddToDeck` from the stack** (Blueberry!™ Diesel's "add 1 of those
    cards to the bottom of your stack"), only for a card a selection chose
    there (`ResolutionContext::selected_in_stack`), so a copy in the grip
    or the heap is never taken in its place.
- **What composes.** Lat is Supercorridor's "same number" (two `MoreThan`s
  under `Not`) over the two hands, as the discard phase ends. Hagen is
  Sandstone's negative strength over `InstalledIcebreakerCount` and a
  trash filtered by `Not(AnyOf([Decoder, Fracter, Killer]))`. The
  Nihilist's first virus program each turn is Avgustina Ivanovskaya's
  column; its "unless the Corp trashes the top card of R&D" is a paid
  choice of `Cost::Trash` over the top of R&D. Focus Group is a choice of
  type, `RevealHand` (its second card, after Engram Flush), and
  `ChooseNumber` capped by `InZone` and the Corp's credits.
- **Fidelity limits:** one, under Known limits. Focus Group's "you may
  pay X[credit]" loses X from the credit pool rather than paying it, and
  "1 installed card" offers the Corp's own installs, as Remastered
  Edition's does.
- **Client.** The new view field is an engine's line on the ledger
  (`servers_run_successfully`: Daily Quest's gain, which the engine and a
  sample ask; every success is in the log). The new words are read out in
  `prose`. No ledger row.
- **Decks.** Level Pegging is Safety Net's frame with Lat for Kate and two
  Blueberry!™ Diesel for two of its three Net Shield. Pay As You Go takes
  two The Nihilist for two Tsakhia; A Thousand Cuts three Sting! for a
  Fujii Asset Retrieval and two Snare!, three points for three; Paid
  Content two Daily Quest for two B-1001 and two Focus Group for two
  Nonequivalent Exchange; Deterrence two Hagen for two Bumi 1.0; Undertow
  two Fully Operational for two Perfect Recall; Retirement Package two
  Vulnerability Audit for two Salvo Testing, point for point. Every card
  given up is still in another deck, and every deck keeps its formats.
- **Two fixes the schedule found.** Level Pegging moved every pairing of
  the sweep schedule, and two of the new pairings found latent bugs.
  - **A card's run offered only a server the Runner could not pay to
    run.** At seed 183 of the 256-seed view sweep (Ground Control against
    Shootin' n' Lootin'), Transfer of Wealth parked a choice of servers
    limited to HQ. Earth Station: SEA Headquarters' 1[credit] to run HQ
    could not be paid, so the Runner had no legal action. This had been
    reachable since Uprising Stage 8. `PromptChooseServer` now offers
    only servers whose additional cost can be paid
    (`run::may_pay_run_cost`, which `start_run` asks too). With none left
    it is refused the way a server-less offer is, so a "you may run"
    keeps its other option (CR 1.16.10a).
  - **The bots' pricing of a parked payment overflowed a debug stack.**
    Game 19 of the index sweep (Not So Subtle against Sabbatical) parked
    a rez whose payment asked four times in a row.
    `eval::fundamentals::through_parked_payment` held a `GameState` by
    value in every iterator adapter, about 350 KB of stack per question,
    so it overflowed the test thread's 2 MB. It is now a loop over boxed
    states, and it makes the same choice: the payer's best, the last of
    equals.
- **Measured.** `cargo test --workspace` is green and clippy is silent,
  with the desktop crate run one test target at a time. Both sweeps are
  green at 256 seeds, the card gate included. `coverage_identical.py`
  against Stage 3 (2863e18; 192 games a report) has random identical, by
  view and by index: the new words move nothing the sample decks reach.
  The planner moves because `determinize` samples the new cards: Corp
  agenda wins 71 → 65, Corp wins by flatline 21 → 22, Runner agenda wins
  95 → 102, Runner deck-outs 5 → 3.
  The purge test's Runner virus roster names The Nihilist, which hosts
  virus counters.
- **DSL ratio** (`pool_status.py`): 15 of 102 `Effect` variants
  single-use, none unused, over 609 card files — `Repeat` is single-use
  and `RevealHand` has its second card.

#### Stage 5 — encounter and ice-state words (5 October 2026)

`claude/serene-einstein-6bhlig`: Chisel, “Baklan” Bochkin, Pelangi,
Afshar, Rime, Loot Box, Public Health Portal, Secure and Protect and
Divested Trust. **No new `Effect`.** Downfall 45 of 65; `DF_UNIMPLEMENTED`
29 → 20.

- **Rejig moved to Stage 6.** "As an additional cost to play this event,
  add 1 installed program or piece of hardware to your grip. Install 1
  program or piece of hardware from your grip, paying X[credit] less. X is
  equal to the printed install cost of the card you added to your grip."
  No cost hands the card it moved to the effect it pays for:
  `ResolutionContext::paid_with` is filled only where an accepted paid
  choice resolves, and the install that reads X parks a choice, so X would
  have to be written into it before it does. Stage 6 is the stage of costs.
- **The words.**
  - **`GainIceSubtype` says which ice** (`ice`, `ModifyStrength`'s
    `StrengthOf`): Pelangi's "the ice you are encountering gains that
    subtype for the remainder of this encounter" is a `Lingering::
    GainSubtype` on the encountered ice until the encounter ends, beside
    Lycian Multi-Munition's own while it stays rezzed. A tuple variant
    became a struct one, so Lycian's file names its subtypes as fields.
    `validate` refuses "each piece of ice gains", which no card prints.
  - **`ContinuousKind::RezzedAsNonIce`** (Rime's "during runs against this
    server, you can rez this ice any time you could rez non-ice cards", a
    `while: RunAgainstThisServer`): the rez handler asks it beside the
    approach, never instead of it, and the action list's probe follows.
    CR 3.4.3a says ice is *normally* rezzed only while approached.
  - **`CardFilter::ThatCard`** (Divested Trust's "add **the stolen
    agenda** to HQ"): a placeholder written over as the card the trigger's
    moment is about when the trigger fires (`listeners::card_about`,
    `Effect::with_that_card`), so it outlives the forfeit the Corp is
    asked about first. `acts_on_subject` would have moved the forfeit onto
    the stolen agenda too. `validate` refuses it on a trigger not about a
    card.
  - **A selection takes a card out of the Runner's score area** for the
    Corp, with its points (CR 1.17.1, as a forfeit takes them).
  - **`PromptInstallCorpCard::central_only`** (Secure and Protect's
    "protecting a central server"), `remote_only`'s other half.
  - **Ice protects a server for `Scope::IceProtectingThisServer`**: Rime's
    "each piece of ice protecting this server gets +1 strength", which
    `validate` had admitted only from an asset, an upgrade or a Trojan.
- **What composes.** Chisel is Monkeywrench's host strength over
  `HostedCounters` and Arruaceiras Crew's "if its strength is 0 or less,
  trash it" on a `Host` encounter; its counter waits on `ThisCardIsInstalled`,
  since the ice it just trashed still reads as encountered. Bochkin is
  S-Dobrado's first encounter of a run (`EncountersThisRun`) and Capybara's
  derez of the encountered ice, measured against its counters as they were
  before its `[trash]` (`last_known`). Afshar is Hammer's `BreakLimit`
  under Winchester's `Protecting(Hq)`. Loot Box is Stargate's reveal of
  the top 3 turned to the Runner's stack, chosen by the Corp, into the grip
  and paid at realloc()'s `PrintedCost`, then shuffled. Public Health
  Portal is Flower Sermon's reveal of the top of R&D. Secure and Protect is
  Tucana's search under a Double's click.
- **Fidelity limits:** three, under Known limits. Loot Box shows the
  Runner only the card added to the grip, as Stargate shows the Corp only
  the card trashed; Pelangi offers barrier, code gate and sentry; Bochkin's
  X is every counter it has.
- **Client.** No new view field. The new words are read out in `prose`.
  No ledger row.
- **Decks.** Burn Rate takes two Chisel for two Hantu; Moonlighting two
  “Baklan” Bochkin for two Red Team; Dead Reckoning two Pelangi for two
  Sipa; Ground Control two Afshar for two Tocsin and two Secure and Protect
  for two Pivot; Hostile Bid two Divested Trust for two False Lead, point
  for point; Undertow two Rime for two Sorocaban Blade; Paid Content two
  Loot Box for two Capacitor; Second Site two Public Health Portal for two
  Esca. Every card given up is still in another deck, and every deck keeps
  its formats.
- **Measured.** `cargo test --workspace` is green and clippy is silent,
  with the desktop crate run one test target at a time. Both sweeps are
  green at 256 seeds, the card gate included. `coverage_identical.py`
  against Stage 4 (9fc2590; 192 games a report) has random identical, by
  view and by index: the new words move nothing the sample decks reach.
  The planner moves because `determinize` samples the new cards: Corp
  agenda wins 65 → 66, Corp wins by flatline 22 → 20, Runner deck-outs
  3 → 4. The purge test's Runner virus roster names Chisel and Pelangi.
- **DSL ratio** (`pool_status.py`): 14 of 102 `Effect` variants
  single-use, none unused, over 618 card files — `GainIceSubtype` has its
  second card. Stage 4's ratio was 15 of 102 over 609 (recorded above);
  the live roadmap had written it as 101.

#### Stage 6 — triggers created by a played card, costs to run, interrupts (6 October 2026)

`claude/serene-einstein-6bhlig`: In the Groove, Climactic Showdown, Cold
Site Server, Reduced Service, Game Over, Rejig, Utae, Lucky Charm and Flip
Switch, with Game Over (from Stage 4) and Rejig (from Stage 5) built where
they were moved. **One new `Effect`** (`ForEach`). Downfall 54 of 65;
`DF_UNIMPLEMENTED` 20 → 11.

- **The words.**
  - **`Effect::LaterThisTurn`** replaces `WhenThisTurnEnds`: a delayed
    conditional ability (CR 9.6.13) waits for any moment of the turn
    (`when`), narrowed as a trigger's condition is (`filter`, judged by
    `listeners::when_admits` as the card's controller hears it), and is
    heard once, or every time for the rest of the turn (`every_time`).
    Climactic Showdown's "the first time this turn you breach either R&D
    or HQ, access 2 additional cards" is `OnBreach` on `Server([RnD,
    Hq])`; In the Groove's "for the remainder of this turn, whenever you
    install a card with a printed install cost of 1[credit] or greater" is
    `OnInstall` on a `Card` filter, every time. Lightning Laboratory's file
    names its moment. `LaterThisTurn` has three cards where
    `WhenThisTurnEnds` had one.
  - **`Effect::ForEach`** (Game Over's "For each card that would be
    trashed this way, the Runner may pay 3[credit] to prevent that card
    from being trashed"): `effect` once for each card in an installed zone
    that the filter admits, the card named as `CardTarget::Install` of its
    handle (written over `InstallId::PLACEHOLDER`, the convention
    `HostRigCardOnInstall` follows), rewritten into a `Sequence` as
    `Repeat` is, so each paid choice parks and the rest wait. `Repeat`
    counts and names nothing; a selection's `then` acts as the card chosen,
    whose side would have been who trashed it.
  - **`CardTarget::Install`**: an install by its handle, whoever's. The
    trash goes through the prevention window like any other.
  - **`Cost::ClicksAmount`** and run costs read as their card: Cold Site
    Server's "[click] and 1[credit] for each hosted power counter" and
    Reduced Service's "2[credit] for each hosted power counter" are counted
    off the upgrade's own counters when the server is announced
    (`continuous::run_costs`), and `validate` admits a run cost on an
    upgrade's `RunsOnThisServer`.
  - **`Cost::AddInstalledToHand`** (Rejig's "As an additional cost to play
    this event, add 1 installed program or piece of hardware to your
    grip"), asked one card at a time as `Cost::Trash` is. **What an
    event's additional cost took rides on `GameEvent::EventPlayed`
    (`paid_with`)** into the event's own resolution
    (`ResolutionContext::paid_with`), and a selection writes its printed
    cost into the discount of the install it parks (`Effect::
    with_paid_card_cost`, and a `Discount::Amount` in its filter read as it
    is offered), since the install resolves on a later action.
  - **`ChooseServer::only_protected_by_ice`** (Climactic Showdown's "Choose
    a server protected by ice"), and a selection reads the chosen server
    (`CardFilter::InChosenServer`, which only an event filter had read).
  - **Two interrupts** (CR 9.9.1). `Preventable::RunEnding` (Lucky Charm):
    a Corp card's "end the run" waits in the prevention window when an
    interrupt could be used on it (`WouldHappen::RunEnds`), after Shred's
    standing prevention. `Preventable::TraceBaseStrength` (Flip Switch):
    a trace waits to be initiated (`WouldHappen::Trace`, the trace in
    `PendingPrevention::waiting`), and is initiated whatever was done, at
    base strength 0 when the interrupt was used (CR 9.9.6d).
- **A loop the first test found.** Once nobody had used Flip Switch, the
  waiting trace was resolved again as `Effect::Trace`, which asked again,
  so the window never closed: a debug test ran to 7 GB passing priority.
  The asking now initiates the trace directly (`ability::start_trace`).
- **A gap the view sweep found.** SDS Drone Deployment's steal cost
  (Stage 2's "trash 1 installed program") asks which program when two are
  installed, and the steal was not on `payment::could_ask`'s list, so the
  action was applied without the copy a question needs and the debug
  assertion in `engine::apply_action` failed. The new pairings of the
  schedule first put it against a rig of two programs. A steal now asks
  `could_ask` about its agenda's printed and standing steal costs, and the
  assertion names the action that asked.
- **What composes.** Utae is Lobisomem's X break once a run (`OncePerRun`)
  and Odore's three virtual resources. Cold Site Server's counters are a
  [click] ability and a turn-start removal; Reduced Service's are bought
  with `ChooseNumber` and placed with `Repeat`, and one goes on a
  successful run on a central (`OnSuccessfulRun` on the three centrals).
  Lucky Charm's "if you made a successful run on HQ this turn" is Paule's
  Café's `TimesThisTurnWhen`. Flip Switch's tag removal is an ordinary
  `[trash]` ability; every ability asks `DuringYourTurn`. Climactic
  Showdown removes itself, has the Runner choose, and offers the Corp the
  trash or the delayed ability.
- **Fidelity limits:** three, under Known limits. Reduced Service's "pay"
  loses the credits, as Focus Group's does; In the Groove's "first [click]"
  is the turn's first action; Flip Switch's jack-out is paid with its
  trash, as Lionsmane's is offered.
- **Client.** The waiting delayed ability is read out in the HUD in its
  own words (`prose::describe_later_this_turn`) instead of "when this turn
  ends" for every one, and the prevention prompt and log name the run's
  end and the trace's base strength. `DelayedAbility`'s two new fields
  ride in `ClientView::delayed`, already drawn; no ledger row.
- **Decks.** Pay As You Go takes two Utae for two Abaasy and two Climactic
  Showdown for two Hush; Level Pegging two In the Groove for two Joy Ride
  and two Rejig for two Spark of Inspiration; Picket Line two Lucky Charm
  for two Tread Lightly and two Flip Switch for two Pennyshaver;
  Deterrence two Cold Site Server for two Tithe; Supply Chain two Reduced
  Service for two Regolith Mining License; Pay to Win two Game Over for
  two Digital Rights Management. Every card given up is still in another
  deck, and every deck keeps its formats.
- **Measured.** `cargo test --workspace` is green and clippy is silent,
  with the desktop crate run one test target at a time. Both sweeps are
  green at 256 seeds, the card gate included. `coverage_identical.py`
  against Stage 5 (6c9f0d2; 192 games a report) has random identical, by
  view and by index. The planner moves because `determinize` samples the
  new cards, with every end reason unchanged.
- **DSL ratio** (`pool_status.py`): 13 of 103 `Effect` variants
  single-use, none unused, over 627 card files — `ForEach` is single-use,
  and `LaterThisTurn` has three cards where `WhenThisTurnEnds` had one.

#### Stage 7 — hidden information and new zones (6 October 2026)

`claude/serene-einstein-6bhlig`: Hyoubu Institute: Absolute Clarity,
Khusyuk, The Class Act, Project Yagi-Uda, Letheia Nisei and Saisentan, and
Hyoubu Institute's Sweep deck, Open Book. **No new `Effect`.** Downfall 60
of 65; `DF_UNIMPLEMENTED` 11 → 5.

- **The words.**
  - **A reveal is a moment** (`Trigger::OnCardRevealed`, Hyoubu
    Institute's "the first time each turn you reveal a card"), heard by
    whoever revealed the card, which is not always its owner:
    `GameEvent::CardRevealed` carries `by` (Engram Flush has the Corp
    reveal the Runner's grip; the Runner reveals Snare! as it is accessed),
    and a selection that reveals what it chose (`CardsSelected { revealed:
    true }`) is its chooser revealing each card. Every site that records
    one outside a cost dispatches it now (`dispatcher::emit`), as the audit
    asked of each; a cost's are dispatched by its payer.
  - **The Runner's draw is about to happen** (`WouldHappen::Draw`,
    `Trigger::OnDrawAboutToResolve`): every draw of the Runner's, by a
    card's text or the click, is announced and parked through
    `rules::prevention` as damage is, so The Class Act's "the first time
    each turn you would draw any number of cards, look at the top X cards
    of your stack. Add 1 of those cards to the bottom" resolves, and parks
    its selection, before the cards move. **The first thing parked there
    that no card prevents**: nothing is ever asked, and the parking is the
    point (CR 9.9.3). An empty stack announces nothing; the Corp's draws
    are not announced, since no card hears them. X is `Amount::
    AboutToResolve` plus 1, read into `CardFilter::TopOf` as the selection
    is offered.
  - **A swap out of HQ into a root** (Project Yagi-Uda's "swap 1 card from
    HQ with 1 card in the root of or protecting the attacked server"):
    Tatu-Bola's swap takes any Corp install now, and what may come in is
    one question for the offer and the swap (`run::swappable_into`,
    `CardFilter::SwappableIntoThis`, CR 8.8.2) — ice for ice; into a root an
    upgrade, or an agenda or asset in a remote holding no other, never a
    second region or past an upgrade's "only". A Trojan on ice swapped out
    is trashed with it (8.8.4b); it had stayed on the handle.
  - **A chosen number reaches what reads it** (`Amount::
    with_chosen_number`, `CardFilter::with_chosen_number`): Khusyuk's
    "the number of your installed cards with that printed install cost, up
    to 6" is `Reduced` sums over `InZone { filter: PrintedCostExactly(
    ChosenNumber) }`, which the substitution used to leave untouched below
    the top of an amount.
  - **`MoveRunToOutermost(None)`** is the attacked server (Letheia Nisei's
    "this server"), and **`EffectRequirement::LastDamageTrashed(filter)`**
    asks what the last damage in the resolution trashed (Saisentan's "a
    card of the chosen type", the type Engram Flush's `Remember` keeps for
    the encounter).
- **A `then` that waited behind its own trigger.** A selection whose
  resolution finds something parked for prevention queues its `then`
  behind it — right for a trash the selection itself parked, wrong for the
  draw The Class Act's selection is heard ahead of: the draw happened
  first and the chosen card went to the bottom afterwards. Only what the
  selection parked is waited behind now.
- **Karunā jacks out.** Its "The Runner may jack out" was an "end the run"
  the Runner chose, which since Stage 6 Lucky Charm could have been asked
  to prevent; it is Lionsmane's paid jack-out now, as Letheia Nisei's and
  Project Yagi-Uda's are.
- **What composes.** Hyoubu's click is a choice between Bring Them Home's
  `RevealAtRandom` and a one-card revealing selection off the top of the
  stack; Khusyuk is Stargate's run and access replacement over Deep Dive's
  set-aside and access, the X cards set aside one at a time (`Repeat`); The
  Class Act's discard-phase draw asks Euler's `ActingCardMatches(
  InstalledThisTurn)`; Project Yagi-Uda's counters are Off the Books'
  dividends; Letheia is a psi game (`PsiGame`) behind `OncePerRun`;
  Saisentan is Engram Flush's remembered card type.
- **Fidelity limits:** six, under Known limits — Hyoubu's stack reveal is a
  selection the Corp confirms; Khusyuk's cost is chosen from 1 to 10; The
  Class Act's drawn cards are never set aside and hear only the Runner's
  discard phase; Project Yagi-Uda's "past 3" is past the requirement, and
  a swapped-in card is not heard as installed; Letheia's "first time" is
  `OncePerRun`; Saisentan reads only damage dealt in the same resolution.
- **Client.** The prevention prompt and the log have arms for a draw,
  which nobody is asked about; `Amount::AboutToResolve` and the attacked
  server's move have words. `CardRevealed::by` rides in the log only, and
  `WouldHappen::Draw` in `ClientView::pending_prevention`, already drawn;
  no ledger row.
- **Decks.** Open Book is new: Hyoubu Institute with Saisentan, Letheia
  Nisei and Project Yagi-Uda among Jinteki cards that reveal (Engram
  Flush, Bring Them Home, Public Health Portal, Snare!), Eternal-legal.
  Safety Net takes two Khusyuk for two Beatriz Friere Gonzalez, and
  Moonlighting two The Class Act for two Verbal Plasticity; every card
  given up is still in another deck.
- **Measured.** `cargo test --workspace` is green and clippy is silent,
  with the desktop crate run one test target at a time. Both sweeps are
  green at 256 seeds, the card gate included. `coverage_identical.py`
  against Stage 6 (a4a70a0; 192 games a report) is identical by view and
  by index, and differs from Stage 6 in every shape, for two reasons
  taken apart by a second run with Karunā's old file: **with it, random
  differs only in `AboutToResolve` events (336 → 1525)**, the Runner's
  draws now announced, every game otherwise the same; Karunā's jack-out
  then re-rolls the random games that meet it (Runner agenda wins 113 →
  112, Corp flatlines 74 → 75). The planner moves because `determinize`
  samples the new cards (Corp agenda wins 66 → 61, flatlines 20 → 25).
- **DSL ratio** (`pool_status.py`): 12 of 103 `Effect` variants
  single-use, none unused, over 633 card files — no variant added, and
  `MoveRunToOutermost` has its second card.

#### Stage 8 — naming a card, blanking an identity, memory across runs, action kinds (6 October 2026)

`claude/serene-einstein-6bhlig`: Complete Image, Whistleblower, Direct
Access, Always Have a Backup Plan and MirrorMorph: Endless Iteration, and
MirrorMorph's Sweep deck, Endless Loop. **Two new `Effect`s.** Downfall 65
of 65; `DF_UNIMPLEMENTED` 5 → 0, and Standard joins Startup among the
complete formats (`COMPLETE_FORMATS`): every card in the Standard pool is
built.

- **The words.**
  - **A card name is chosen** (`Effect::ChooseCardName`, Complete Image's
    and Whistleblower's "name a card", CR 1.15.1b): the names offered are
    every playable card the filter admits, identities aside, sorted, and
    the chooser answers with an action of its own,
    `PlayerAction::ChooseCardName { card }`, **appended** to `ActionSpace`
    (3261 → 3773, a slot per name up to `MAX_NAME_OPTIONS`, 512; no index
    moved). The name is written into the effect that waits
    (`Effect::with_chosen_name`, `CardFilter::ChosenName`), the State
    Hygiene Rule's third case as a chosen number is, so it reaches a
    filter on a later moment — Whistleblower's "when you access the named
    agenda" — with no field anywhere. "Repeat this process" is `again_if`:
    after the `then`, the requirement is asked with the name, and the
    whole choice is offered again while it holds (Complete Image, for as
    long as the damage trashed a card of the name).
  - **A named agenda is stolen as it is accessed, ignoring all costs**
    (`Effect::StealAccessedCard`, Whistleblower's alone): the access's own
    steal (`run::steal_accessed_ignoring_costs`) with no steal cost asked,
    and an agenda gone to the score area has left its place, so the access
    moves on.
  - **Both identities lose their abilities** (`LoseAbilities { identities
    }`, Direct Access's "the Corp and the Runner lose all abilities on
    their identity cards for the remainder of this run", CR 2.1.4): a
    lingering effect on each identity's handle, which `rules::active` asks
    before an identity hears anything.
  - **A run remembers the last ice it encountered**
    (`RunState::last_encountered`, kept in `CompletedRun`), and
    `CardFilter::LastEncounteredLastRun` names it, so Always Have a Backup
    Plan's second run bypasses it (`LaterThisTurn { this_run }`, a delayed
    ability that ends with the run it was made in). The second run is on
    "that server" (`PromptChooseServer { last_run_server }`) and "ignoring
    all additional costs" (`ignore_additional_costs`, carried into the
    decision as `ChooseServer::ignore_run_costs` and run by
    `run::start_run_ignoring_costs`).
  - **An action has a kind, and the next one can be required to differ**
    (MirrorMorph's "take another different action", CR 5.2.5b):
    `Amount::ActionsThisTurn` and `DifferentActionsThisTurn` read the turn
    log (`TurnLog::different_actions`), and `Prohibition::RepeatAnAction`
    for `EffectDuration::NextAction` (`Until::ActionsFinished`) refuses an
    action already taken this turn (`RulesError::ActionRepeated`).
    `Amount::AgendaPoints(side)` is Complete Image's "the Runner has at
    least 3 agenda points".
- **Two things the deep sweep found.**
  - **A deadlock** (seed 80, Endless Loop against Tickets, please,
    random seats): the Corp has three clicks, so MirrorMorph's is nearly
    always its last, and with every action it could take one it had
    taken that turn it had no legal action at all. While the click is
    bound to a different action and none exists, the Corp may end its
    turn, the extra action forgone and the click lost with the turn (CR
    5.6.3c; `turn::forgoes_a_different_action`). Rejected: offering the
    option only when a different action exists, a lookahead on every
    `PresentChoice`; and lifting the prohibition, which would let the
    click buy a repeat.
  - **The fog gate read a name as a leak**: a list of names to choose
    from named an agenda in the Corp's hidden HQ. The names are every
    playable card the filter admits, read off the registry and not the
    game, so the gate counts the decision's own list as shown.
- **What composes.** Complete Image's "end your action phase" is Stage 6's
  `EndActionPhase`, and its damage is Saisentan's `LastDamageTrashed`;
  Whistleblower's trash is a paid choice's cost; Direct Access's shuffle
  back is a run-ended effect over its own copy in the heap.
- **Fidelity limits:** five, under Known limits — Complete Image's names
  are the playable Runner cards, not the format's; Whistleblower steals as
  the access begins; Direct Access's identities lose their abilities from
  the run's start; Always Have a Backup Plan's last encountered ice is the
  engine's alone; MirrorMorph's extra action can come after a fourth.
- **Client.** A name is labelled "Name X" and logged "named X", with
  words in `prose` for every new effect, duration, prohibition and amount.
  A list of names is too long for a row of pills, so the desktop pop-up
  shows more than twelve decisions as a drop-down (`LONG_DECISION_LIST`);
  the terminal client's list already scrolls. `PendingDecision::
  ChooseCardName` is drawn, and `ChooseServer::ignore_run_costs` is the
  engine's (the run pays nothing extra, which the readouts show by not
  changing); no ledger row.
- **Decks.** Endless Loop is new: MirrorMorph: Endless Iteration with
  Retirement Package's cards, Eternal-legal. Encore takes Whistleblower
  for Verbal Plasticity, Picket Line Always Have a Backup Plan for
  Overclock, Burn Rate Direct Access for Katorga Breakout and Open Book
  Complete Image for Cultivate; every card given up is still in another
  deck. A new Corp deck re-pairs every sweep seed, and the 256-seed card
  gate then saw no Reprise (Hit List's alone) and no Alarm Clock (Spare
  Parts' one copy), as it once missed Alarm Clock at Rebellion Without Rehearsal Stage 6a: Encore
  takes two Reprise for two Overclock, behind Whistleblower's steal, and
  Picket Line two Alarm Clock for two Docklands Pass.
- **Measured.** `cargo test --workspace` is green and clippy is silent,
  with the desktop crate run one test target at a time. Both sweeps are
  green at 256 seeds, the card gate included, once the deadlock and the
  gate's reading above were fixed. `coverage_identical.py` against
  Stage 7 (7987e90; 192 games a report) is identical by view and by
  index, and **random play is identical to Stage 7**: no sample deck
  changed, and the new decision is reached only in Sweep decks. The
  planner moves because `determinize` samples the new cards (Corp
  agenda wins 61 → 62, flatlines 25 → 26; Runner agenda wins 102 → 100).
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 638 card files — `ChooseCardName` is two
  cards', `StealAccessedCard` one's.

### 8. The reprint packs and the Core Set's remainder — 159 cards (C 95 / V 45 / M 20)

#### Stage 1a — System Update 2021's Runner cards, composed (6 October 2026)

`claude/nsg-tranche-8-qmn4v7`: Mimic, Abagnale, Legwork, Dirty Laundry,
Career Fair, Professional Contacts, Liberated Account, Earthrise Hotel,
Scrubber and Xanadu. **No new `Effect`, and no change to the engine:**
card files, decks and tests. System Update 2021 21 of 82;
`SU21_UNIMPLEMENTED` 71 → 61. The tranche's first stage was split in two
(1a, 1b) to keep a pull request near ten cards, the person's measure
(6 October 2026: a whole set in one tranche "is too much").

- **What each is made of.** Mimic is a killer with Corroder's break and no
  pump. Abagnale is Cat's Cradle's two abilities and Laser Pointer's
  `TrashSelf` bypass, asked as a paid ability under
  `Encountering(CodeGate)` (Corsair's requirement), so it is offered on a
  code gate alone. Legwork is The Maker's Eye on HQ. Dirty Laundry is
  Kompromat's `SetRunEndedEffect` under `on_success`, so an unsuccessful
  run pays nothing. Career Fair is Bahia Bands' discounted install from
  the grip, its selection narrowed to resources. Professional Contacts is
  a [click] ability with no limit. Liberated Account is Telework
  Contract's counters with no once-per-turn, and Earthrise Hotel Dr. Nuka
  Vrolyck's power counters spent as the turn begins, each trashed when
  empty. Scrubber is Azimat's two recurring credits for trash costs on a
  resource. Xanadu is Fransofia Ward's rez tax, word for word.
- **Fidelity limits:** none found.
- **Client.** Nothing added to the view, the log or a decision, so no
  ledger line and no ledger row.
- **Decks.** Pay As You Go takes two Mimic for two Chain Reaction, two
  Liberated Account for its Nga, two Scrubber for its Num and two Xanadu
  for two of its three The Toolbox; Encore two Legwork for its Tread
  Lightly, two Dirty Laundry for its Docklands Pass and two Abagnale for
  its Buzzsaw, decoder for decoder, then two Career Fair for its Red Team
  and two Earthrise Hotel for its Mutual Favor; Safety Net two
  Professional Contacts for its Gauss. Every card given up is still in
  another deck, and every deck keeps the formats it was pinned to:
  Moonlighting, Standard-legal, was the first home for Career Fair and
  Earthrise Hotel, and the format pin refused it, since no reprint pack
  is in Standard.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 648 card files, as over 638 at Downfall's
  close.
- **Measured.** `cargo test --workspace` green and clippy silent, the
  desktop crate excluded (the cloud container has no Wayland to build it
  against, and this stage touches no client code). Both sweeps are green
  at 256 seeds, the card gate included, so every new card is seen in play.
  The random `--all-matchups` report cannot move: Sweep decks are not in
  `matchups()`.

#### Stage 1b — the rest of the reprints' Runner cards that compose (6 October 2026)

`claude/nsg-tranche-8-qmn4v7`: Cache, Lucky Find, Prepaid VoicePAD,
Indexing, Retrieval Run, Labor Rights, Aesop’s Pawnshop and Emergency
Shutdown, from all three reprint packs (Aesop’s Pawnshop is a Core Set
card System Update 2021 reprints). **No new `Effect`, and no change to
the engine.** System Update 2021 25 of 82, Salvaged Memories 5 of 18,
the Magnum Opus Reprint 1 of 6; `SU21_UNIMPLEMENTED` 61 → 57,
`SM_UNIMPLEMENTED` 17 → 13, `MOR_UNIMPLEMENTED` 6 → 5.

- **What each is made of.** Cache is a virus program whose counter is the
  cost of its ability (Dr. Nuka Vrolyck's `RemoveCounters` cost), and the
  purge roster test now names it. Lucky Find is Corporate Hospitality's
  additional [click]. Prepaid VoicePAD is Mystic Maemi's
  `Playing(Event)` on one recurring credit. Indexing is Cataloguer's
  arrange as an optional access replacement on a run event; Retrieval Run
  the same replacement on Archives with Beta Build's install ignoring all
  costs, out of the heap (`InstallRunnerCardFromZone`). Labor Rights is
  The Price's `Mill` of the Runner's own stack and Harmony AR Therapy's
  shuffle from the heap, removed from the game as Kompromat is. Aesop’s
  Pawnshop is a turn-start selection of the Runner's other installs
  (`NotSourceCard`), its trash and gain behind The Price's `CardsSelected`
  guard. Emergency Shutdown is Chain Reaction's "successful run on HQ this
  turn" (`TimesThisTurnWhen`) and Maglectric Rapid's derez over rezzed
  ice.
- **Two things the tests caught before they shipped.** A selection that
  may choose nothing still resolves its `then`, and Aesop’s Pawnshop's
  "trash it" with nothing chosen trashed the pawnshop itself — the
  `then` is guarded by `AmountAtLeast(CardsSelected, 1)`, as The Price's
  is. Labor Rights' "shuffle 3 cards" is exact with fewer than three in
  the heap (all of them go back) by three `EffectIf`s on the heap's size,
  asked **smallest first**: a list's later conditions are asked after the
  earlier ones resolve, so asked largest first, a heap of four left one
  behind for the "exactly one" branch to shuffle back too.
- **Fidelity limits:** none found.
- **Client.** Nothing added to the view, the log or a decision.
- **Decks.** Burn Rate takes two Retrieval Run for two Chain Reaction, two
  Labor Rights for two of The Toolbox and two Prepaid VoicePAD for its
  Banner; Side Quest two Lucky Find for two Chain Reaction; Street Gallery
  two Indexing for its Decoy and two Aesop’s Pawnshop for two
  Sacrificial Construct; Encore two Cache for its Leech and two Emergency
  Shutdown for two Sure Gamble. Every card given up is still in another
  deck, and every edited deck was already pinned outside Standard and
  Startup.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 656 card files.

#### Stage 2 — the reprints' Corp ice and upgrades that compose (6 October 2026)

`claude/nsg-tranche-8-qmn4v7`: Rototurret, Eli 1.0, Wraparound, Pop-up
Window and Archer from System Update 2021 (Rototurret and Archer are Core
Set cards it reprints), Hokusai Grid from the same pack, and Border
Control and Embolus from the Magnum Opus Reprint. **No new `Effect`, and
no change to the engine.** System Update 2021 31 of 82, Salvaged Memories
5 of 18, the Magnum Opus Reprint 3 of 6; `SU21_UNIMPLEMENTED` 57 → 51,
`MOR_UNIMPLEMENTED` 5 → 3.

- **What each is made of.** Rototurret is Stavka's "trash 1 installed
  program" and an end the run. Eli 1.0 is Hákarl 1.0's "Lose [click]:
  break 1 subroutine", used by the Runner, over two end the runs.
  Wraparound is Akhet's continuous `Strength` on itself, its `while` a
  `Not(ZoneHasAtLeast)` over installed fracter programs. Pop-up Window is
  Paywall's subroutine with a gain on encounter in place of Paywall's
  loss. Archer is Plutus's additional rez cost — a single
  `rez_alternatives` entry forfeiting one agenda, so with nothing to
  forfeit the rez is refused — over Stavka's two program trashes. Border
  Control is M.I.C.'s self-trash during a run on its server, here ending
  the run outright, and Federal Fundraising's `IceProtectingThisServer`
  as the amount of a gain. Hokusai Grid is Adrian Seis's successful run
  on this server with a net damage. Embolus is Storgotic Resonator's
  hosted power counter as a cost, a turn-start "you may pay" in
  `OfferPaidChoice`, and a successful run anywhere (`subject: Any`)
  removing one while any is left.
- **Fidelity limits:** none found.
- **Client.** Nothing added to the view, the log or a decision.
- **Decks.** Retirement Package takes two Rototurret for its two Echo and
  two Eli 1.0 for its two Wave; Paid Content two Pop-up Window for its
  two Grubber and two Wraparound for its two Lethe; Supply Chain two
  Archer for its two Event Horizon and two Border Control for two Tithe;
  Permafrost two Hokusai Grid for its two The Red Room and two Embolus for
  its two Tributary (first tried in Open Book, whose drift left the default
  32-seed view sweep without a `ChooseCardName` — Complete Image is in that
  deck — while the 256-seed sweep still reached it). Every card given up is still in another
  deck, every new card is in its deck's faction, and every edited deck
  was already pinned outside Standard and Startup.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 664 card files.

#### Stage 3 — the reprints' Corp agendas, operations and assets that compose (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: House of Knives, License Acquisition,
Celebrity Gift, Reversed Accounts, Ronin and Corporate Town from System
Update 2021, and Sweeps Week from Salvaged Memories. **No new `Effect`,
and no change to the engine.** System Update 2021 37 of 82, Salvaged
Memories 6 of 18, the Magnum Opus Reprint 3 of 6; `SU21_UNIMPLEMENTED`
51 → 45, `SM_UNIMPLEMENTED` 13 → 12.

- **What each is made of.** House of Knives is Remastered Edition's
  agenda counters spent as a cost, behind Pressure Spike's `OncePerRun`
  (false outside a run, so "only during a run" needs no word of its own).
  License Acquisition is Ansel 1.0's "from HQ or Archives" as a
  `PresentChoice` of two selections and a decline, each installing with
  `ignore_costs` and `rez`. Sweeps Week gains `CardsInHand(Runner)`.
  Celebrity Gift is Corporate Hospitality's additional [click] and
  Meeting of Minds's reveal, its `CardsSelected` doubled with `Times`.
  Reversed Accounts and Ronin are advanceable assets (an advancement
  requirement of 0, as Clearinghouse's) whose `[click], [trash]` cost is
  paid before the effect reads the hosted counters, through `last_known`;
  Ronin's "4 or more" is the ability's requirement, asked before the
  cost. Corporate Town is Archer's forfeit as a single `rez_alternatives`
  entry and a turn-start selection that moves a resource straight to the
  heap, which no prevention window sees — the card's "cannot be
  prevented".
- **Deferred.** Project Atlas and Project Vitruvius place a counter "for
  each hosted advancement counter past 3" when scored, and a scored agenda
  keeps no count of its advancement counters (`ScoredAgenda` carries none,
  and `AgendaScored` neither), so they move to stage 6 with the amounts.
  Timely Public Release installs ice "in any position", where every
  install of ice is outermost; it moves to stage 7.
- **One engine fix, found by CI's debug build.** `payment::could_ask`
  (the necessary condition that lets `engine::apply_action` skip copying
  an action that cannot ask about a payment) admitted a server choice for
  an install-and-rez only while some facedown card on the table printed a
  way to pay its rez. The card being installed is not on the table yet:
  Ob Superheavy Logistics finding Archer or Corporate Town in Supply
  Chain asked which agenda to forfeit, and the debug assertion fired in
  `every_sample_deck_matchup_finishes`. The server-choice arm now answers
  from the parked install alone. The local runs had been `--release`,
  where the assertion is compiled out; this stage's checks were re-run in
  a debug build.
- **Fidelity limits:** none found.
- **Client.** Nothing added to the view, the log or a decision.
- **Decks.** Honor Roll takes two House of Knives for its two Hybrid
  Release; Second Site two Ronin for two Public Health Portal and two
  Celebrity Gift for two Cultivate; Paid Content two License Acquisition
  for two Oracle Thinktank, two Sweeps Week for two Your Digital Life and
  two Reversed Accounts for two Chekist Scion; Supply Chain two Corporate
  Town for two Svyatogor Excavator. Each agenda swap is a 1-pointer for a
  1-pointer, so every deck keeps its points; every card given up is still
  in another deck, every new card is in its deck's faction, and every
  edited deck was already pinned outside Standard and Startup.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 671 card files.

#### Stage 4 — trigger words: a server created, a score forbidden by another card (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: Ken “Express” Tenma, Near-Earth Hub and
Clot from System Update 2021, and Turtlebacks and Hostile Infrastructure
from Salvaged Memories. **No new `Effect`; one new `Trigger` and one new
`GameEvent`.** System Update 2021 40 of 82, Salvaged Memories 8 of 18, the
Magnum Opus Reprint 3 of 6; `SU21_UNIMPLEMENTED` 45 → 42,
`SM_UNIMPLEMENTED` 12 → 10.

- **A server's creation is a moment** (`Trigger::OnServerCreated`,
  `GameEvent::ServerCreated`). CR 8.5.16e: "If the card is to be the first
  card in the root of or protecting a new remote server, that server is
  created." Turtlebacks hears it every time and Near-Earth Hub the first
  time each turn. Both places a Corp card is installed
  (`engine::place_corp_card` and `InstallFromZoneIgnoringCost`) emit it
  just before the install's `CardInstalled`, when the destination is a
  remote nothing is installed in (`engine::creates_server`).
  - It is asked before 8.5.16c's trash. An install over the only card of a
    remote leaves a remote of one card, and that server was not created.
  - **Why not a `when` on `OnInstall`:** whether the server is new is not
    a fact of the card, and the state no longer holds it after the
    install.
  - A moment of its own is also what lets "the first time each turn"
    count creations rather than installs: the turn log counts by
    trigger.
  - It is about the server, heard by the Corp (CR 4.6.8b: only the Corp
    creates one), and public in the view and the log. The log reads
    "created Remote 1".
- **Another card can forbid a score.** Clot's "The Corp cannot score an
  agenda during the same turn they installed that agenda" is
  `Cannot(ScoreAgendas)` about `Scoring(InstalledThisTurn)`, Word on the
  Street's scope.
  - `continuous::cannot_install` now asks that scope of the agenda being
    scored, beside the agenda's own word (Vulnerability Audit).
  - The score action, its probe in the action list and `Effect::Score`
    all go through `cannot_install`, so none can disagree.
  - `validate` admits `Cannot(ScoreAgendas)` on a `Scoring` scope and
    refuses every other prohibition there.
- **What the rest is made of.**
  - Ken is Swift's first run event, as `first_each_turn` on
    `OnCardPlayed`.
  - Hostile Infrastructure is Solidarity Badge's two triggers (a trash on
    access, and the Runner's `OnCardTrashed` of a Corp card) without its
    first time. It hears its own trash while it was rezzed, as Hostile
    Architecture does.
  - Clot's purge is Physarum Entangler's.
- **A false alarm in the fog check.** Two new decks re-dealt the
  sweep's pairings. At seed 5 the check flagged a card name chosen aloud
  (Whistleblower) because the card was in HQ. A name both players heard is
  not a card shown, so the check now counts it as visible. Seed 7 found the
  trigger-order leak #376 fixed on main at the same time.
- **Fidelity limits:** Hostile Architecture's, unchanged — a rezzed card
  trashed by the Runner's card text does not hear its own trash
  (`CardTrashed` does not carry the rez state).
- **Client.** Nothing added to the view. The log has one new line, and the
  card inspector words the trigger from its name.
- **Decks.** Two new Sweep decks, both pinned `neither`, since System
  Update 2021 is in no current pool:
  - **Express Delivery** is Encore's frame under Ken: its twelve run
    events, and two Clot for its two The Price at the same four influence.
    Encore and Moonlighting keep The Price.
  - **Broadcast Hour** is Pay to Win's frame under Near-Earth Hub: two
    Turtlebacks for its two Drago Ivanov, and two Hostile Infrastructure
    for its two Chekist Scion, at ten influence of seventeen. Pay to Win
    keeps both cards given up.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 676 card files.

#### Stage 5a — breaker and ice words: a break barred by subtype, a subtype for the run (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: Quetzal, Rielle “Kit” Peddler, Swordsman
and Hortum from System Update 2021, and NEXT Bronze from Salvaged
Memories. **No new `Effect`; one new `ContinuousKind` and one word on an
existing effect.** System Update 2021 44 of 82, Salvaged Memories 9 of
18; `SU21_UNIMPLEMENTED` 42 → 38, `SM_UNIMPLEMENTED` 10 → 9.

- **A piece of ice can bar a kind of program** (`ContinuousKind::
  CannotBeBrokenUsing`). It covers Swordsman's "The Runner cannot break
  subroutines on this ice using **AI** programs", and Hortum's with a
  `while` of three hosted advancement counters.
  - It is about `This`, on ice.
  - It is asked by both break effects of the breaking card
    (`ability::breakable_now`, through `continuous::may_break_using`). They
    then find nothing to break, so the ability is not offered.
  - It covers every subroutine, a gained one too.
  - Only a program is "using a program", so a bioroid's own break and
    Quetzal's are untouched.
  - **Why not `BreakLimit`:** it counts printed subroutines and excepts a
    subtype rather than naming one.
- **The encountered ice can gain a subtype for the run.** Rielle's "it
  gains **code gate** for the remainder of this run" is Pelangi's
  `GainIceSubtype` with `for_the_run` (`Until::Run`). `validate` refuses
  the word on any ice but the encountered one.
- **What the rest is made of.**
  - Quetzal is Vic's identity ability, with Botulus's strength-free break
    (`BreakSubroutinesUnconditionally`) behind `OncePerTurn` and
    `Encountering(Barrier)`.
  - Hortum is advanceable ice (an advancement requirement of 0, CR
    1.18.3). Its "instead" subroutines are two `EffectIf`s on Akhet's
    `HostedAdvancementTokens`, and its search is Digital Rights
    Management's, shuffled after.
  - NEXT Bronze's strength counts `CorpInstalls(All[Ice, Rezzed, NEXT])`,
    itself included.
- **Moved to 5b:**
  - Atman's "you may spend any number of credits to place that many power
    counters". An X is paid only inside an activated ability's cost
    (`Cost::CreditsX`), and a "lose" would not be a spend.
  - Parasite's "When the strength of host ice is 0 or less, trash it".
    This is a conditional ability with a static condition (CR 9.6.7),
    which the checkpoint does not yet raise.
- **Client.** Nothing added to the view. The inspector words the new kind
  and the run-long gain.
- **Decks.**
  - Two new Sweep decks, both pinned `neither`:
    - **Free Spirit** is Pay As You Go's frame under Quetzal, at six
      influence of fifteen.
    - **Transhuman** is Safety Net's frame under Rielle, at nine of ten.
  - Three swaps, each in its deck's faction:
    - Second Site takes two Swordsman for its two Phoneutria, which Open
      Book and Permafrost keep.
    - Hostile Bid takes two Hortum for its two Anvil, which Ground
      Control keeps.
    - Endless Loop takes two NEXT Bronze for its two Drafter, which
      Retirement Package keeps.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 681 card files.

#### Stage 5b — a breaker of exactly equal strength, a static condition (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: Atman from System Update 2021 and
Parasite from Salvaged Memories. **No new `Effect`; one new `Trigger`, one
`Amount` and one word on a Trojan.** System Update 2021 45 of 82, Salvaged
Memories 10 of 18; `SU21_UNIMPLEMENTED` 38 → 37, `SM_UNIMPLEMENTED` 9 → 8.

- **A conditional ability with a static condition** (CR 9.6.7,
  `Trigger::WhileTrue`). Parasite's "When the strength of host ice is 0 or
  less, trash it" waits for no event: the strength falls when a virus
  counter lands at the Runner's turn start, and could fall when a
  lingering effect runs out. The condition is the entry's `requirement`.
  - `checkpoint::static_conditions` marks pending every active card whose
    condition is true (9.6.7a) and queues it as a `DeferredTrigger`, one
    instance per source (9.6.7c).
  - It is asked once, at the end of every action, after the last
    checkpoint, and what it queued is drained and checked again there.
    **Deviation, recorded:** the rules ask at every checkpoint, and the
    engine asks at the action's last one. A condition that turns true
    mid-action is still true at its end, and no card in the pool can tell
    the difference. The one ask per action also stands in for 9.6.7d: an
    ability that resolved and changed nothing is not marked again until
    the next action.
  - `validate` refuses a `WhileTrue` with no `requirement`, or with a
    `when` or a first time, which are a moment's.
- **`Amount::HostIceStrength`** is the acting Trojan's host's strength,
  never below 0 (`continuous::installed_ice_strength`). That function is
  `ice_strength` for an install named outside a run.
- **"Install only on a rezzed piece of ice"** is `host_must_be_rezzed` on
  a Trojan. The action list and `install_program_on_ice` both ask it.
  Whether ice is rezzed is public, so the narrower list leaks nothing. It
  is an install restriction, never a hosting one, so a host derezzed
  later keeps its Parasite.
- **Atman is Reduced Service's spend.** Its install trigger is a
  `ChooseNumber` capped at the Runner's credits, then that many credits
  lost and that many power counters placed. +1 strength per counter is a
  `This` strength. Its interface requires `Not(MoreThan(ThisCardStrength,
  EncounteredIceStrength))`, and the break itself refuses the other
  direction.
  - **Correction to Stage 5a,** which deferred Atman because "a lose
    would not be a spend". Reduced Service already reads "spend … to
    place" as a loss from the credit pool. Outside a run and outside an
    ability, no pool but the credit pool pays for it.
  - **Known limit:** both strengths are read clamped at 0. An Atman with
    no counters therefore interfaces with ice of negative strength.
- **Decks.** Two swaps, each in its deck's faction:
  - Level Pegging takes two Atman for its two Self-Modifying Code, which
    Safety Net keeps.
  - Side Quest takes two Parasite for its two Hantu, which three other
    decks keep.
- **Tests.**
  - Purge's real-card roster now names Parasite. Its fixture hosts
    Parasite on a piece of ice and gives each rig card an install of its
    own.
  - Three new tests: Atman's spend and its equal-strength interface,
    Parasite's rezzed host, and Parasite trashing its host (and itself)
    on the second turn on Enigma.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 683 card files.

#### Stage 6a — the Runner's amounts and effects (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: seven Runner cards. **No new `Effect`.**
One new `CardFilter` word, and one existing word widened. System Update
2021 is at 49 of 82 and Salvaged Memories at 13 of 18; the Core Set has
34 of 113 built.
- **Cards from System Update 2021:** Imp, Egret, Ice Carver and Paricia.
- **Cards from Salvaged Memories:** Cerberus “Lady” H1, Medium and e3
  Feedback Implants.

- **Ice Carver: the ice being encountered is a filter word.**
  - The card reads "While you are encountering a piece of ice, it gets
    −1 strength". That is a `Strength` on `Scope::Ice(BeingEncountered)`.
  - `CardFilter::BeingEncountered` is instance-level, like
    `LastEncounteredLastRun`. It is read off the run in
    `pending_choice::copy_matches`.
  - **Why not a `while`:** a `while` of `DuringEncounter` is asked of the
    card that prints it, so every piece of ice would have lost 1 strength
    during any encounter. Parasite's host would have been one of them.
- **Paricia: trash-cost credits say which cards' trash costs they pay.**
  - The card reads "You can spend hosted credits to pay trash costs of
    **assets**". That is `PaysFor::TrashCosts(CardType(Asset))`.
  - The word now carries a filter. Azimat, Scrubber and Bahia Bands's
    run credits write `Any`.
  - `Purpose::TrashCost` now names the card being accessed, so the filter
    is read off it in `payment::covers`.
  - A narrower trash-cost pool is spent before a wider one, unasked
    (`word_within`), as `Installing` and `Playing` pools already are.
- **Egret** is Chromatophores on `host_must_be_rezzed`, the Trojan word
  from Stage 5b.
- **Medium** is Conduit's counters on R&D runs. Its breach is a
  `ChooseNumber` of up to the hosted counters less 1 (`Reduced`), then
  `AddAdditionalAccessAmount` with the number chosen.
- **e3 Feedback Implants** offers 1[credit] on every `OnSubroutineBroken`
  to break one more subroutine on that ice with
  `BreakSubroutinesUnconditionally`. A break it pays for is a break too,
  so the offer comes again, as the printed "whenever" says.
- **Cerberus “Lady” H1** has power counters as its break cost, and **Imp**
  has Carnivore's access ability with a counter as its cost.
- **Moved to stage 8: Networking.** Its "you may pay 1[credit] to add this
  event to your grip" decides where a played event goes. Only a
  declaration says that today (`removed_after_play`), and a declaration
  cannot depend on a paid choice.
- **Decks: seven swaps,** each in its deck's faction. Every card given up
  is still in at least one other deck.
  - Transhuman: two Cerberus for two World Tree.
  - Dead Reckoning: two Egret for two Living Mural.
  - Street Gallery: two Paricia for two Sipa.
  - Burn Rate: Imp for Hush.
  - Grassroots: two Medium for two Audrey v2.
  - Free Spirit: two Ice Carver for two Crash Space.
  - Encore: two e3 Feedback Implants for two Capybara.
- **Tests.** Seven new tests, one per card. Purge's real-card roster now
  names Imp and Medium.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 690 card files.

#### Stage 6b — the Corp's amounts and effects (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: seven Corp cards. **No new `Effect`**, and
no new word: one event carries a number it was dropping. System Update
2021 is at 55 of 82 and Salvaged Memories at 14 of 18; the Core Set has
37 of 113 built.
- **Cards from System Update 2021:** Project Atlas, Project Vitruvius,
  Nisei MK II, Oaktown Renovation, Archived Memories and Biotic Labor
  (the last three Core Set reprints).
- **Cards from Salvaged Memories:** Executive Boot Camp.

- **Project Atlas and Project Vitruvius: a score's trigger reads the
  counters the agenda was scored with.**
  - The card reads "When you score this agenda, place 1 agenda counter on
    it for each hosted advancement counter past 3". That is a `Repeat` of
    `AddCounters` 1, `Reduced` by a fixed 3.
  - The counters go back to the bank as the agenda moves (CR 1.17.5), and
    a trigger of the score uses the last known number (CR 1.17.8).
    `GameEvent::AgendaScored` now carries it (`advancement_tokens`, as
    `CardAdvanced` carries its count), and `Amount::HostedAdvancementTokens`
    reads it there when the scored agenda's own trigger asks.
  - **Why not dividends:** `dividends` counts past the requirement as the
    table stands (CR 10.13.1), and "past 3" is a printed 3. The two part
    as soon as a card changes the requirement.
  - Why the stage moved them out of stage 3: nothing had remembered the
    counters once the agenda left the table.
- **Nisei MK II** is House of Knives with one counter, used only during a
  run to end it.
- **Oaktown Renovation** is Sacrifice Zone Expansion's `installs_faceup`
  with two `EffectIf`s on its `OnAdvance`: 2[credit] below 5 counters, 3 at
  5 or more, read after the counter is placed.
- **Executive Boot Camp** offers a rez at turn start through Mycoweb's
  `RezInstalled` with a discount of 1, over any unrezzed card that is not
  an agenda; its `[trash]` ability searches R&D for an asset.
- **Archived Memories** and **Biotic Labor** compose from Drafter's and
  BASS's words.
- **Moved to stage 7:** Tollbooth (a payment the Runner must make if
  able), Lotus Field (a strength that cannot be lowered), Excalibur (no
  more runs this turn), Trick of Light (advancement counters moved), and
  the rest of the Corp's cards in the three packs.
- **Decks: seven swaps,** each in its deck's faction and point for point
  where an agenda goes. Every card given up is still in another deck.
  - Ground Control: two Project Atlas for two Azef Protocol, two Oaktown
    Renovation for two Sacrifice Zone Expansion.
  - Tag, You're It: two Executive Boot Camp for two Clearinghouse.
  - Endless Loop: two Project Vitruvius for two Midnight-3 Arcology, two
    Biotic Labor for two Corporate Hospitality.
  - Retirement Package: two Archived Memories for two Caveat Emptor.
  - Second Site: two Nisei MK II for two See How They Run.
- **Tests.** Seven new tests; Project Atlas is scored at 5 counters (2
  agenda counters) and at exactly 3 (none).
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 697 card files.

#### Stage 7a — no more runs, and a strength that cannot be lowered (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: three Corp cards. **No new `Effect`.** One
`Prohibition` and one `ContinuousKind`. System Update 2021 is at 57 of
82 and Salvaged Memories at 15 of 18; the Core Set has 37 of 113 built.
- **Cards from System Update 2021:** Lotus Field and Crisium Grid.
- **Cards from Salvaged Memories:** Excalibur.

- **Excalibur: the Runner can be kept from making another run.**
  - Its subroutine reads "The Runner cannot make another run this turn".
    That is `Prohibit { what: Run, until: Turn }`.
  - `Prohibition::Run` is asked in `run::check_run_may_begin`, which both
    doors to a run share: the basic action and a card's text (Red Team's
    server choice). It now takes the registry, and refuses with
    `RulesError::RunsProhibited`. The action list's probe then offers no
    run.
  - "Another" needs no word of its own: the run under way has begun, and
    nothing asks again.
- **Lotus Field: a strength that cannot be lowered.**
  - The card reads "The strength of this ice cannot be lowered". That is
    `ContinuousKind::StrengthCannotBeLowered` on `This`, ice only.
  - `continuous::installed_ice_strength` is the one question a strength is
    put to. On such ice it counts each term, standing or lingering, only
    if it raises: a raise beside a lowering still raises, and the lowering
    does nothing to this ice (CR 1.2.2). `lingering::ice_strength_terms`
    gives the lingering terms one at a time for it.
  - **Why not a `Strength` of the opposite sign:** that would be a raise,
    not a floor.
- **Crisium Grid** is Flagship's first line, `CannotBeDeclaredSuccessful`
  on `RunsOnThisServer`. Its region subtype comes from the catalog, so the
  limit of one region per server is already the install's.
- **Moved to stage 7b:** Tollbooth ("must pay 3[credit], if able" is a
  payment, not an offer, and no word says it yet) and Punitive
  Counterstrike (the printed points stolen last turn, which the turn log
  does not sum).
- **Decks: three swaps,** each in its deck's faction or neutral. Every
  card given up is still in another deck.
  - Permafrost: two Lotus Field for two Knowledge Seeker.
  - Hostile Bid: two Crisium Grid for two Cayambe Grid.
  - Tag, You're It: two Excalibur for two Palisade.
- **Tests.** Three new tests. Excalibur's checks the run under way goes
  on, the next run is refused and not offered, and the next turn is free.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 700 card files.

#### Stage 7b — a payment made if able, and the points stolen last turn (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: two Corp cards. **No new `Effect`.** One
field on `OfferPaidChoice`, one turn-log sum and one `Amount`. System
Update 2021 is at 59 of 82; the Core Set has 38 of 113 built.
- **Cards from System Update 2021:** Tollbooth (a Core Set reprint) and
  Punitive Counterstrike.

- **Tollbooth: a payment that is an obligation.**
  - The card reads "When the Runner encounters this ice, they must pay
    3[credit], if able. If they do not, end the run."
  - `OfferPaidChoice::if_able` makes the offer an obligation. Nobody is
    asked. The engine takes the accept a choice would take, on a copy of
    the state; it keeps the copy if the cost was paid and declines if it
    could not be.
  - Paying goes through `pending_choice::resolve_accept`, so the Payment
    Rule holds: every pool that covers the payment is read, and a split
    that must be asked still asks, by replay.
  - **Why not an `EffectIf` on the credit pool:** that is the
    affordability sum the Payment Rule forbids. It would have missed bad
    publicity's credits on the run.
  - **Why not a parked choice with its decline refused:** that asks a
    question with one answer.
- **Punitive Counterstrike: the printed points stolen last turn.**
  - The card reads "Trace[5]. If successful, do X meat damage. X is equal
    to the sum of the printed agenda points on all agendas the Runner
    stole during their last turn."
  - The turn log now keeps `agenda_points_stolen` beside
    `agenda_points_scored`. It is recorded at `AgendaStolen` through the
    log's one door, and read off `last_turn` by
    `Amount::AgendaPointsStolenLastTurn`.
  - The points are printed, as the card says, so they are read off the
    definition. A stolen Project Vacheron is worth nothing for a while and
    prints 3.
- **Moved to stage 7c:** Trick of Light, Ravana 1.0, Haas-Bioroid:
  Architects of Tomorrow and Timely Public Release.
- **Decks: two swaps,** each in its deck's faction. Every card given up
  is still in another deck.
  - Broadcast Hour: two Tollbooth for two Virtual Service Agent.
  - Hostile Bid: two Punitive Counterstrike for two Myōshu (Land Grab,
    which is Standard-legal, could not take it).
- **Tests.** Two new tests. Tollbooth takes 3[credit] from a Runner with
  5[credit] and asks nothing; with 2[credit], the run ends and nothing is
  taken. Punitive Counterstrike does 3 meat damage for a 2-point and a
  1-point steal.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 702 card files.

#### Stage 7c — ice in any position, and counters moved (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: two Corp cards. **No new `Effect`.** One
field on `PromptInstallCorpCard` and one on a parked number decision. System
Update 2021 is at 60 of 82 and Magnum Opus Reprint at 4 of 6.
- **Cards:** Timely Public Release (Magnum Opus Reprint) and Trick of
  Light (System Update 2021, a Core Set reprint).

- **Timely Public Release: ice installed in any position.**
  - The card reads "Hosted agenda counter: Install 1 piece of ice from HQ
    or Archives in any position protecting a server, ignoring all costs."
  - CR 6.2.2d lets the new position be outward of every position, inward
    of every one, or between two. Every install of ice was outermost.
  - `PromptInstallCorpCard::any_position` asks the Corp a number once the
    server is chosen and has ice: how many of its pieces of ice stay
    outward of the new one, 0 for the outermost and the count for the
    innermost. A server with no ice has one position, and nobody is asked.
  - The number is a `ChooseNumber` decision carrying the parked install
    (`PendingDecision::ChooseNumber::install`), answered by the
    `PlayerAction::ChooseNumber` every numeric choice uses. `ActionSpace`
    is unchanged.
  - **Why asked before the card lands, never as a move after it:** the
    position is part of the destination (CR 6.2.2: created "when the Corp
    declares an install destination", step 8.5.16b). A move after the
    install would have let "when installed" triggers see it outermost.
  - `engine::place_corp_card` takes the position (`inward`); every other
    install passes 0, the outermost.
  - Its scoring trigger places 1 agenda counter, and the ability is Project
    Atlas's shape.
- **Trick of Light: counters moved between two cards.**
  - The card reads "Choose 1 installed card you can advance. Move up to 2
    advancement counters from 1 other card to the chosen card."
  - It composes from Hearts and Minds' words. The card with counters is
    chosen first, then how many, at most 2 and at most what it hosts
    (`Reduced` twice is the smaller of the two), then the card you can
    advance (`NotSourceCard`, so never the same card).
  - **Fidelity limit:** the printed order chooses the card you can advance
    first. The choices and their outcomes are the same either way. A
    card's counters are taken only when another card you can advance
    exists, so none is lost.
- **Moved to stage 7d:** Ravana 1.0 and Haas-Bioroid: Architects of
  Tomorrow, the bioroid pair, to keep this unit small.
- **Decks: two swaps,** both in Open Book, point for point where an agenda
  goes. Every card given up is still in another deck.
  - Two Trick of Light for two Mitosis (Permafrost keeps Mitosis).
  - Two Timely Public Release for two Sericulture Expansion (eight other
    decks keep it).
- **Client.** The position is asked by the number prompt both clients
  already draw. Its text says what the number means (`pending_choice::
  ANY_POSITION_PROMPT`), and the view ledger says so of the new field.
- **Tests.** Two new tests. Timely Public Release installs an Enigma
  between two Ice Walls on HQ, then innermost, for no credits, and asks
  nothing on R&D, which has no ice. Trick of Light moves 2 of 3 counters
  from ice to an agenda, offers at most 1 from a card hosting 1, and takes
  nothing when no other card can be advanced.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 704 card files.

#### Stage 7d — the bioroid pair (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: two Corp cards and one Sweep deck. **No new
`Effect`.** One ice fact, one field on a pass of ice. System Update 2021
is at 62 of 82.
- **Cards from System Update 2021:** Ravana 1.0 and Haas-Bioroid:
  Architects of Tomorrow.

- **Ravana 1.0 composes.** Its "Lose [click]: Break 1 subroutine" is Eli
  1.0's ability, and "Resolve 1 subroutine on another rezzed bioroid ice"
  is Mycoweb's selection (`ResolveSubroutineOfSelectedIce`) over rezzed
  bioroid ice that is not Ravana itself (`NotSourceCard`).
- **Architects of Tomorrow: a pass of a rezzed bioroid.**
  - The card reads "The first time each turn the Runner passes a rezzed
    piece of bioroid ice, you may rez 1 bioroid card, paying 4[credit]
    less."
  - `GameEvent::IcePassed` now says whether the ice was a rezzed bioroid
    (`rezzed_bioroid`). It is read off the printed subtype, since bioroid
    is a subtype `rezzed_as` (the ice types) does not list and nothing in
    the pool grants it.
  - "The first time each turn" counts the moment in the turn log, which
    counts ice moments by their facts (`turn_log::Class::Ice`). So
    `IceFacts::rezzed_bioroid` is a sixth fact, beside Sisyphus
    Protocol's `rezzed_code_gate_or_sentry`.
  - **It shares a column bit with `at_most_zero_strength`.** The log
    keeps one row per trigger, and no trigger states both (that one is a
    break's, this a pass's), so within a row the bit means one fact. A
    test holds every trigger to that.
  - **Why not a sixth bit of its own:** the facts would have taken 64
    columns, past the table's 40. Every row of a log that is copied with
    every `GameState` would have grown with it.
  - The rez is Executive Boot Camp's `RezInstalled` with a discount of 4,
    over unrezzed bioroid cards.
- **Decks.** Architects of Tomorrow is a new identity, so it gets a Sweep
  deck of its own. **Assembly Line** is Retirement Package's frame under
  Architects of Tomorrow (Eli 1.0, Hákarl 1.0, Ansel 1.0 and Týr are
  bioroids), with two Ravana 1.0 for its two Pulse. Endless Loop keeps
  Pulse. It is Eternal and Casual, as Retirement Package is.
- **Client.** The new fact has its words in the engine reading ("a rezzed
  piece of bioroid ice"). Nothing new reaches the view.
- **Tests.** Two new tests and one guard. Ravana resolves Eli 1.0's "End
  the run", offers neither itself nor a non-bioroid Enigma, and offers
  nothing with no other rezzed bioroid. Architects of Tomorrow rezzes
  Ravana for free once Eli 1.0 is passed, not a second time that turn,
  and not for an unrezzed Eli. The guard holds every trigger to stating
  at most one of the two facts that share a bit.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 706 card files.

#### Stage 8a — four Runner cards that compose (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: four Runner cards. **No new `Effect` and
no change to the engine.** Stage 8 of the survey is split three ways, so
each PR stays small: 8a composes, 8b needs words for two remembered
choices, and 8c builds the two X costs. System Update 2021 is at 66 of 82
and the Core Set at 40 of 113.
- **Cards from System Update 2021** (each also in the Core Set): Inside
  Job, Femme Fatale, Test Run and Networking.

- **Inside Job** runs any server and leaves a delayed ability for the run
  (`LaterThisTurn`, `when: OnEncounter`, `this_run`), heard once, as
  Always Have a Backup Plan's is heard every time.
- **Femme Fatale** is Boomerang's choice (`Remember { what:
  SelectedCard, until: WhileInstalled }`) read back by
  `EncounteringChosenIce`, and Physarum Entangler's offer (`Cost::
  CreditsAmount(EncounteredIceSubroutines)`, then `BypassEncounteredIce`).
- **Test Run** is Beta Build's search and free install from either pile
  (a `PresentChoice` between the stack, shuffled after, and the heap),
  with the install remembered by a delayed ability for the turn's end
  (`LaterThisTurn`, `when: OnDiscardPhaseEnd`, `AddToDeck(Top)`). The
  install is the "if": a program uninstalled since has no handle, and
  nothing moves. A trojan is not offered, since the free install never
  takes one.
- **Networking: a known approximation.** "Then, you may pay 1[credit] to
  add this event to your grip" is a paid choice whose answer moves a
  Networking from the heap to the grip. A played event is filed in the
  heap as its `OnPlay` is dispatched (`engine::play_event`), before a
  choice it parks is answered, so Networking is trashed and then
  returned. By the rules it never reaches the heap, so Aniccam's "an
  event is trashed" hears a Networking played this way; no Sweep deck
  holds both. A played event waiting in the play area until its parked
  choices are answered is the fix, and it is the engine's, not this
  card's.
- **Decks.** Hit List takes two Femme Fatale for its two Revolver (a
  killer for a killer), two Inside Job for its two Reprise and two
  Networking for its two Bravado; Revolver and Reprise are in three
  other decks and Bravado in two. Safety Net takes two Test Run for its
  two Deep Dive, which three other decks keep.
- **Client.** Nothing new reaches the view.
- **Tests.** Four new tests. Inside Job bypasses the first ice and the
  second's "End the run" ends the run. Femme Fatale chooses Enigma,
  bypasses it for 2[credit], is not offered on another Enigma, and breaks
  no code gate. Test Run installs Corroder from the heap for nothing and
  puts it on top of the stack at the turn's end, and finds nothing in a
  stack without a program. Networking removes a tag and returns to the
  grip for 1[credit], or stays in the heap.
- **DSL ratio** (`pool_status.py`): 13 of 105 `Effect` variants
  single-use, none unused, over 710 card files.

#### Stage 8b — a chosen server per copy, and a chosen subtype (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: two Runner cards. **One new `Effect`,
`SpendChosenServer`.** System Update 2021 is at 68 of 82.
- **Cards from System Update 2021:** Security Testing and Chameleon.

- **A chosen server is the copy's.** Tsakhia's "you may choose a server"
  was remembered as the card's (`Lingering::ChosenServer`, about the
  player, its card the `source`). Tsakhia is unique, and a lockdown's
  choice lives on the copy in the play area. Security Testing is not
  unique, so two copies keyed by card id would read one choice and spend
  both.
  - A choice made by an install is now about that install
    (`On::Install`). `lingering::chosen_server` and
    `spend_chosen_server` take the asking copy.
  - The listener scan, a selection's "that server" and Tsakhia's
    replacement pass their own install.
  - A lockdown's choice stays the card's.
- **Security Testing: the first successful run on the chosen server.**
  - The trigger hears a successful run on the chosen server
    (`EventFilter::ChosenServer`).
  - It replaces the breach and spends the choice
    (`Effect::SpendChosenServer`), the way Tsakhia's
    `ReplaceSubroutines` spends Tsakhia's. A second run on that server
    finds no choice.
  - **Why not `first_each_turn`:** the turn log counts successful runs
    by class, and the remotes are one class. The first successful run on
    a chosen remote could not be told from the first on any remote.
  - **Why not `OncePerTurn`:** the house rules keep that for a printed
    "Once per turn →".
- **A breach replaced can be the attacked server's.**
  `SetAccessReplacement::server` is optional, and none means the
  attacked server, read as the replacement is made. Security Testing's
  chosen server may be a remote, which no card file can name. A run
  event that chooses its server still writes it in
  (`substitute_chosen_server`).
- **Chameleon: a remembered ice subtype.**
  - "Choose barrier, code gate, or sentry" is a `PresentChoice` of three
    `Remember { what: IceType(..), until: WhileInstalled }`
    (`Lingering::ChosenIceType`, `lingering::chosen_ice_type`).
  - The break is an unrestricted `BreakSubroutines` under
    `EffectRequirement::EncounteringChosenIceType`, asked the way
    `Encountering` is, so a subtype gained counts.
  - "When your discard phase ends, add this program to your grip" is
    `OnDiscardPhaseEnd` with `AddToHand`.
- **Decks.**
  - Encore takes two Security Testing for its two Debbie “Downtown”
    Moreira, a resource for a resource; three other decks keep Debbie.
  - Safety Net takes two Chameleon for its two Euler, an icebreaker for
    an icebreaker; two other decks keep Euler.
  - Both decks were already Eternal and Casual.
- **Client.** The chosen subtype has its line on the board's list of
  what is in effect ("the chosen subtype is code gate"), and both new
  words have their engine reading. Nothing new reaches the view: a
  lingering effect already does.
- **Tests.** Three new tests.
  - Security Testing chooses a remote as the turn begins, and gains
    2[credit] instead of the first breach there. The second run breaches
    normally. A run on HQ leaves the choice standing.
  - Two copies each keep and spend their own server.
  - Chameleon chooses code gate, breaks Enigma and not Ice Wall, and
    returns to the grip as the discard phase ends.
  - Tsakhia's test reads its choice by its copy.
  - The client's run-trail test met a Virtuoso in Encore's new
    trajectories: "breach HQ when the run ends" begins a breach with no
    run in the action that completes the run, so the view's run slot is
    filled while the trail has its outcome. A breach with no run is not
    a run the trail follows, and the test now says so.
- **DSL ratio** (`pool_status.py`): 14 of 106 `Effect` variants
  single-use, none unused, over 712 card files. The one added is
  `SpendChosenServer`, with its reason on the variant.

#### Stage 8c — the X costs (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: two Corp cards with no new `Effect`.
System Update 2021 is at 70 of 82, and the Core Set at 42 of 113.
- **Cards from System Update 2021:** Corporate Troubleshooter and
  Psychographics.

- **An operation can cost X to play** (CR 1.16.2c: X is chosen before
  anything is paid).
  - Psychographics prints a cost of X, which the catalog reads as 0. Its
    X is its additional cost to play, `Cost::CreditsX { max: RunnerTags }`,
    asked by the payment's replay and answered with `ChooseNumber`, as
    Utae's X is. Its credits are spent to play (`Purpose::Play`).
  - `play_operation_card` reads X before paying takes it, as
    `activate_ability` does, and the play's event carries it
    (`GameEvent::OperationPlayed::x`). The card's own `OnPlay` has it
    written in as its chosen number when it resolves.
  - **Why on the event:** a play resolves by a dispatch, which rebuilds
    its context from the event it heard; the context the play built is
    gone by then. Rejig's `EventPlayed::paid_with` is the precedent. X is
    credits spent in the open, so it is public and needs no mask.
  - `validate` admits an X only first in an operation's additional cost,
    and admits `ChosenNumber` in that card's `OnPlay`.
- **Corporate Troubleshooter: X[credit], [trash].**
  - The cost is `AllOf[CreditsX, TrashSelf]`; X is bounded by what the
    Corp can pay.
  - "A rezzed piece of ice protecting this server" is `InThisServer`,
    resolved from the card's last-known server after its own cost has
    trashed it, as Hype Machine's root is.
  - "+X strength for the remainder of the turn" is X repetitions of +1
    lingering on the chosen ice (`Repeat` of `ModifyStrength` on
    `This`). `ModifyStrength`'s delta is a printed number in every card
    file, so an `Amount` there would have rewritten them all for one
    card.
- **Decks.**
  - Brutal Efficiency takes two Corporate Troubleshooter for its two
    Mahkota Langit Grid, an upgrade for an upgrade; six other decks keep
    Mahkota Langit Grid.
  - Fine Print takes a Psychographics for its IP Enforcement, an
    operation for an operation; Gimbatul keeps IP Enforcement.
  - Both decks were already Eternal and Casual.
- **Client.** Nothing new reaches the view. X is asked by the payment
  question both clients already draw for Utae.
- **A match thread's stack.** The new decks moved the client's onward
  test into a planner line that overflowed a thread's default 2 MiB in a
  debug build: the opponent's paid choice answered twice, inside a parked
  payment's replay. The frames are large unoptimised
  (`Search::opponents_answer` 290 KB, `settle` 170 KB, `best_line`
  273 KB, the engine's `replay_with` 227 KB). The test passes on `main`.
  A match and a lesson thread now get 16 MiB (`play::MATCH_STACK`), since
  a dev build of either client plans its bots there, and the test plays
  on one.
- **Tests.** Two new tests.
  - Corporate Troubleshooter asks X, is trashed paying it, offers only
    the rezzed ice protecting its own server, and gives it +3 for the
    turn alone.
  - Psychographics refuses an X above the Runner's tags, places X
    counters, carries X on its event, and plays for 0 with no tags.
- **DSL ratio** (`pool_status.py`): 14 of 106 `Effect` variants
  single-use, none unused, over 714 card files.

#### Stage 9a — a trash that may go to R&D instead (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one Corp card, composed, with no new
`Effect` and no change to the engine. System Update 2021 is at 71 of 82.
- **Card from System Update 2021:** Marilyn Campaign.

- **"Load 8[credit]… take 2[credit]… when it is empty, trash it"** is
  Nico Campaign's shape: counters loaded as it is rezzed, two taken as
  the turn begins, and a trash of itself when none are left. Nothing else
  takes them, so the turn's beginning is the only moment it can empty.
- **"[interrupt] → When this asset would be trashed, you may shuffle it
  into R&D instead of adding it to Archives."**
  - It is Luana Campos's interrupt (`OnWouldBeUninstalled`), announced by
    the one door a Corp install leaves the table through, and only for a
    rezzed card. That is the interrupt's activeness (CR 9.9.4b): an
    unrezzed Marilyn the Runner trashes goes to Archives, and a copy in HQ
    or R&D was never active.
  - The choice is a `PresentChoice` of `AddToDeck(Top)` then
    `ShuffleIntoDeck([OwnRAndD])`, Oracle Thinktank's shuffle, or
    nothing.
  - **The known approximation:** the choice parks, and the trash it
    interrupts finishes first, so the card reaches Archives and the
    choice takes it from there. "It is still considered trashed" holds:
    every trash event is heard. What differs is the turn log, which counts
    it as a Corp card added to Archives (Regenesis reads that), and an
    uninstall that is not a trash would offer the shuffle too.
- **Decks.** Retirement Package takes two Marilyn Campaign for its two
  Refuge Campaign, a campaign for a campaign; Assembly Line and Endless
  Loop keep Refuge Campaign. The deck was already Eternal and Casual.
- **Client.** Nothing new reaches the view.
- **Tests.** Two new tests.
  - Emptied as the turn begins, it pays its last 2[credit], is trashed,
    and goes into R&D (where the turn's draw may find it), or to Archives
    when the Corp declines.
  - Trashed by the Runner on access, a rezzed copy may go into R&D, and an
    unrezzed one goes to Archives with nothing asked.
- **DSL ratio** (`pool_status.py`): 14 of 106 `Effect` variants
  single-use, none unused, over 715 card files.

#### Stage 9b — a draw that grows (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one Corp card and one new `Effect`. System
Update 2021 is at 72 of 82.
- **Card from System Update 2021:** Daily Business Show.

- **"[interrupt] → The first time each turn you would draw any number of
  cards"** is The Class Act's moment, `OnDrawAboutToResolve` with
  `first_each_turn`, heard by the side that draws. The Corp's draws were
  never announced, because nothing heard them. Now every one is, through
  `ability::corp_would_draw`: a card's draw, the click draw and the
  mandatory draw (CR 5.6.1e). From an empty R&D the draw is still the
  deck-out it was (CR 1.7.2c), with nothing to announce.
- **"Increase the number of cards you will draw by 1"** changes how many,
  which `Prevent` cannot say: it only lowers what is parked.
  `IncreaseAboutToResolve { by, then }` adds to the parked draw and
  refuses when none is parked.
- **"When you draw those cards, add 1 of them to the bottom of R&D"**
  happens after the draw. The `then` waits in `PendingPrevention::waiting`
  with the new count written over `Amount::ChosenNumber`, and
  `prevention::finish` draws first and resolves it after. It is a
  selection from HQ through `TopOf(ChosenNumber)`. The drawn cards are the
  end of HQ, so only they are offered.
- **The mandatory draw opens the start-of-turn window even when a
  decision is still waiting.** The window is how
  `turn::finish_turn_beginning` knows the draw was made. Opening it after
  the decision would have drawn a second time. The decision is answered
  first, as one a paid ability parks inside a window is.
- **Decks.** Paid Content takes two Daily Business Show for The Holo Man
  and Janaína “JK” Dumont Kindelán. Both are still in other decks.
- **Client.** Nothing new reaches the view. The prose for the new
  `Effect` is in `netrunner_client::prose`.
- **Tests.** Two new tests, and two event lists that now begin with the
  draw's announcement.
  - The mandatory draw of 1 draws 2, and only those 2 can go to the bottom
    of R&D. The turn goes on, and a click draw later that turn draws 1.
  - Sprint's draw of 3 draws 4, and Sprint's own shuffle follows the
    bottomed card. The Runner's click draw is never heard.
- **DSL ratio** (`pool_status.py`): 15 of 107 `Effect` variants
  single-use, none unused, over 716 card files.

#### Stage 9c — changing agenda values (7 October 2026)

`claude/nsg-tranche-8-qmn4v7`: two Corp cards with no new `Effect`.
System Update 2021 is at 74 of 82, Salvaged Memories at 16 of 18, and
the Core Set at 43 of 113.
- **Cards:** Project Beale (System Update 2021), and SanSan City Grid
  (Core Set, Salvaged Memories and System Update 2021).

- **"Place 1 agenda counter on it for every 2 hosted advancement counters
  past 3"** is Project Atlas's score trigger with a rate. `Amount::Every
  { amount, every }` is 1 for every `every` of `amount`, rounded down,
  and the other half of `Times`. A ladder of `EffectIf`s, one per
  threshold, would stop at the last rung written.
- **"Worth 1 more agenda point for each hosted agenda counter"** is
  Megaprix Qualifier's `AgendaPoints` about `This`, at a rate of
  `HostedCounters`. The score asks it, so the tally and the win check
  agree.
- **"Each agenda in the root of this server gets −1 advancement
  requirement"** is Ontological Dependence's `AdvancementRequirement`,
  about `RootOfThisServer(Agenda)`. `validate` now admits that scope on
  an upgrade. The scan already read the root of a source's server, and
  only while the grid is rezzed. "Limit 1 region per server" is the
  Region subtype from the catalog, already enforced by the install's
  trash.
- **Decks.** Paid Content takes two Project Beale for its two Freedom of
  Information, 2 points for 2, and two SanSan City Grid for its two
  Magistrate Revontulet. Both are still in other decks.
- **Client.** Nothing new reaches the view: the requirement and the
  points shown are already the ones the engine asks. The prose for
  `Every` is in `netrunner_client::prose`.
- **Tests.** Two new tests.
  - Project Beale scored at 4, 5, 6 and 7 counters gets 0, 1, 1 and 2
    agenda counters and is worth 2, 3, 3 and 4.
  - A rezzed SanSan City Grid lets an agenda needing 3 be scored with 2.
    An unrezzed grid, or one in another server, does not.
- **DSL ratio** (`pool_status.py`): 15 of 107 `Effect` variants
  single-use, none unused, over 718 card files.

#### Stage 9d — subroutines for each NEXT ice (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one Corp card, composed, with no new
`Effect` and no change to the engine. Salvaged Memories is at 17 of 18.
- **Card from Salvaged Memories:** NEXT Silver.

- **"This ice gains '[subroutine] End the run.' for each rezzed piece of
  NEXT ice"** is Echo's `Subroutines` grant, counted with NEXT Bronze's
  amount: `CorpInstalls(All[Ice, Rezzed, HasSubtype(NEXT)])`. A rezzed
  NEXT Silver counts itself.
- **Decks.** Endless Loop takes two NEXT Silver for its two Hákarl 1.0,
  barrier for barrier, beside the NEXT Bronze it already runs. Hákarl 1.0
  is still in other decks.
- **Client.** Nothing new reaches the view.
- **Tests.** One new test. Alone, a rezzed NEXT Silver has one "End the
  run". With NEXT Bronze rezzed it has two, and with NEXT Bronze unrezzed
  it still has one.
- **DSL ratio** (`pool_status.py`): 15 of 107 `Effect` variants
  single-use, none unused, over 719 card files.

#### Stage 9e — a resource that comes back from the heap (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one Runner card, composed, with no new
`Effect` and no change to the engine. Magnum Opus Reprint is at 5 of 6.
- **Card from Magnum Opus Reprint:** Crowdfunding.

- **"Load 3[credit]… take 1[credit]… When it is empty, trash it and draw
  1 card"** is Daily Casts' shape, with the draw after the trash in the
  same `EffectIf`.
- **"When your turn ends, if you made at least 3 successful runs this turn
  and this card is in your heap, you may install it, ignoring all
  costs":**
  - It listens from the heap, as Jeitinho's install does (`from_heap`,
    CR 9.1.8b).
  - "Your turn ends" is `OnDiscardPhaseEnd`, Jeitinho's other trigger.
  - "3 successful runs" is the turn log's count of `OnSuccessfulRun`.
  - The install is `InstallRunnerCardFromZone` from the heap with
    `Discount::AllCosts`. The card's own install trigger loads it again.
- **Decks.** Encore takes two Crowdfunding for its two Earthrise Hotel.
  Express Delivery keeps Earthrise Hotel. Encore is pinned
  Eternal-only, which Crowdfunding needs. Picket Line, the first choice,
  is pinned Standard as well.
- **Client.** Nothing new reaches the view.
- **Tests.** Two new tests.
  - After 3 successful runs, the turn's end offers the install from the
    heap. It costs nothing, and the card comes in loaded with 3. After 2
    runs, nothing is asked.
  - With 1[credit] left, the turn's beginning takes it, trashes the card
    and draws 1.
- **DSL ratio** (`pool_status.py`): 15 of 107 `Effect` variants
  single-use, none unused, over 720 card files.

#### Stage 9f — an operation that comes back from Archives (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one Corp card with no new `Effect`.
**Salvaged Memories is complete (18 of 18).** System Update 2021 is at 75
of 82.
- **Card from System Update 2021 and Salvaged Memories:** Subliminal
  Messaging.

- **"Gain 1[credit]"** is the play.
- **"The first time each turn you play a copy of Subliminal Messaging,
  gain [click]"** is a second `OnPlay` entry with `OncePerTurn`. The turn
  log counts a card's type, never its title, so `first_each_turn` would
  count the turn's first operation of any name. `OncePerTurn` is exact
  here for the reason Ryō's is: the entry has no other condition, so the
  first copy played always spends it, and every copy shares the key
  (`OncePerTurnKey` with no install). This is the second card held to a
  first time narrowed by a title.
- **"When your turn begins, if this card is in Archives and the Runner did
  not initiate any runs during their last turn, you may reveal this card
  and add it to HQ":**
  - A card in Archives now listens, as a card in the heap does (CR
    9.1.8b). `TriggeredEffect::from_heap` is renamed `from_discard`, with
    `Heard::FromDiscard`: one word for both discard piles, since it is one
    rule. `validate` no longer refuses it on a Corp card.
  - Only a **faceup** card in Archives listens. The Runner may not know a
    facedown one is there (CR 4.4.6c), and a trigger heard, asked about or
    declined would tell them. Like the heap, Archives gives one listener a
    card, so two faceup copies return one a turn.
  - "The Runner did not initiate any runs during their last turn" is
    `Not(AmountAtLeast(TimesLastTurn(OnRunStart), 1))`. As the Corp's turn
    begins, the turn that ended most recently is the Runner's.
  - "Add it to HQ" is `AddToHand`. A Corp card acting with no install acts
    from Archives, so its last faceup copy there goes to HQ. This is
    recorded as a revealed `CardsSelected`, which Hyoubu Institute hears
    as a reveal. A `PromptChooseCards` over Archives could not do it: it
    never offers the last faceup copy of an operation that is resolving
    (`pending_choice::resolving_operation_in`), and after a parked "may"
    nothing tells that apart from a card heard from Archives.
- **Decks.** Open Book (Hyoubu Institute, Eternal-only, which Subliminal
  Messaging needs) takes two Subliminal Messaging for two of its three
  Hedge Fund. Hedge Fund is in many decks.
- **Client.** Nothing new reaches the view.
- **Tests.** Two new tests.
  - Two copies played in one turn gain 1[credit] each, and only the first
    gains [click].
  - After a Runner turn with no run, the Corp's turn begins with the offer,
    and taking it moves the card to HQ. After a run, nothing is asked. A
    facedown copy is not heard.
- **DSL ratio** (`pool_status.py`): 15 of 107 `Effect` variants
  single-use, none unused, over 721 card files.

#### Stage 9g — an ice that reads the top of the stack (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one Corp card and one new `Effect`.
**Magnum Opus Reprint is complete (6 of 6).**
- **Card from Magnum Opus Reprint:** Slot Machine.

- **"When the Runner encounters this ice, they put the top card of the
  stack on the bottom, then you reveal the top 3 cards of the stack"** is
  two `TopOfDeck`s:
  - The first takes 1 unrevealed card, with `each: AddToDeck(Bottom)`.
  - The second reveals 3. Each card is a `CardRevealed` by the Corp.
  - `TopOfDeck` is new. Composition didn't work: `LookAtTopOfDeck` shows
    cards to one player and acts on none, and `RevealAtRandom` draws from
    a hand. Its `each` must not park, as `RevealAtRandom`'s must not.
- **"If you revealed 2 (3) or more cards that share a type when this
  encounter began"** is `AmountAtLeast(RevealedThisEncounterSharingAType,
  N)`.
  - A card revealed during an encounter is now kept on the encounter's
    tally (`EncounterTally::revealed`) until the encounter ends. The
    subroutines resolve on a later action than the reveal, and
    `GameState::revealed` is emptied when the revealing action ends.
  - Every reveal records there (`ability::reveal`), whatever revealed it.
    Cards revealed from a deck stay off `GameState::revealed`, which lists
    cards revealed in a hand.
  - The amount is the largest group sharing a card type, by
    discriminant, as `CardTypesAmongFaceupInArchives` counts types.
- **"Place 3 advancement tokens on an installed card"** chooses among the
  Corp's installed cards. The engine places advancement counters on no
  Runner card.
- **Decks.** Paid Content (NBN: Making News, Eternal-only, which Slot
  Machine needs) takes two Slot Machine for its two Virtual Service Agent,
  code gate for code gate. Grand Opening and Pay to Win keep Virtual
  Service Agent.
- **Client.** The encounter's revealed cards reach the view inside the
  tally, and the view ledger says why they need no picture: each reveal
  is a line of the log.
- **Tests.** One new test, over three stacks: three events revealed (lose
  3, gain 3, place 3), two (lose 3, gain 3, nothing to place), and no
  shared type (lose 3 only). In all three the top card goes to the bottom.
- **DSL ratio** (`pool_status.py`): 16 of 108 `Effect` variants
  single-use, none unused, over 722 card files.

#### Stage 9h — an ice that blanks what it hosts (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one Corp card, no new `Effect`.
- **Card from System Update 2021:** Magnet.

- **"When you rez this ice, choose 1 installed program hosted on a piece
  of ice. Host that program on this ice"** is an `OnRez` trigger: the Corp
  chooses a trojan program from the rig (`All[Program, HasSubtype(Trojan)]`,
  since only a trojan is hosted on ice), then `HostRigCardOnInstall`.
  - The substitution in `pending_choice` now reads which end parks the
    choice. A Runner card hosts itself on the install it chose (GAMEDRAGON™
    Pro, Spree's trojan); a piece of ice hosts the program it chose. A piece
    of ice is never hosted.
  - With no trojan installed there is nothing to choose, and the rez goes
    ahead.
- **"Each hosted program loses all abilities and cannot gain abilities"**
  is `LosesAbilities` and `CannotGainAbilities` about a new `Scope`,
  `Hosted`: each card hosted on this one. It is Hush's `Host` read from
  the other end. `validate` admits it only on ice.
  - `rules::active::lost_abilities` and `may_have_granted` ask it
    (`host_takes`), and so does `installs_without_abilities`, so the
    listeners, the continuous scan and the paid abilities all read it.
  - An unrezzed Magnet takes nothing (CR 9.1).
  - **A Hush on a Magnet wins.** Each would take the other's abilities,
    and CR 9.12.1e settles that loop: "treat effects from hosted objects as
    if they did not depend on effects from the objects they are hosted
    on". So Hush's effect applies first, Magnet loses its abilities, and
    Hush keeps its own. The ice's standing is asked of what it hosts, and
    the hosted card's never of its host, so the two questions cannot ask
    each other.
- **Decks.** Retirement Package (Engineering the Future, Eternal-only)
  takes two Magnet for its two Rototurret, ice for ice. Assembly Line keeps
  Rototurret.
- **Client.** A blanked program's sheet says so ("Has lost all its
  abilities (Magnet)"), unless a Hush on the same ice has taken Magnet's
  first. The prose reads `Hosted` as "each card hosted on this".
- **Tests.** One new test: a rez with no trojan asks nothing. Egret moved
  from Ice Wall to Magnet is blanked, so neither ice gains a sentry's type.
  A Hush on Magnet takes Magnet's abilities and keeps its own `[click]`
  ability.
- **DSL ratio** (`pool_status.py`): 16 of 108 `Effect` variants
  single-use, none unused, over 723 card files.

#### Stage 10a — expose (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: two Runner cards and one new `Effect`.
- **Cards from the Core Set:** Infiltration, Lemuria Codecracker.

- **"Expose 1 card"** (CR 1.21.4: "to expose a card is to reveal it,
  except that only installed, unrezzed cards can be exposed") is a
  selection over the Corp's `Unrezzed` installs whose `then` is the new
  `Expose`. It acts on the card chosen.
  - The card is revealed by the Runner (`CardRevealed { by: Runner }`), so
    a card that hears a reveal hears it. It stays facedown, and the Runner
    remembers it (`InstalledCard::seen_by_runner`), so the Runner's view
    names it from then on, as it names a card they accessed.
  - Nothing happens if the card was rezzed or left the table while the
    choice waited.
  - Composition didn't work: a selection's `reveal` names the card to its
    chooser, but the log mask hides a facedown install from the Runner,
    and nothing marked it seen.
- **Infiltration**: "Gain 2[credit] or expose 1 card" is a `PresentChoice`.
- **Lemuria Codecracker**: "[click], 1[credit]: Expose 1 card. Use this
  ability only if you have made a successful run on HQ this turn" is a
  paid ability behind `TimesThisTurnWhen(OnSuccessfulRun, Server[Hq])`, as
  Cataloguer's is behind R&D.
- **Not yet preventable.** Zaibatsu Loyalty is the only card that prevents
  an expose, and it is Stage 10b. It adds `Preventable::Expose` and routes
  `Expose` through `prevention::would`, so neither is vocabulary no card
  uses.
- **Decks.** Pay As You Go takes two Infiltration for its two Running Hot;
  Free Spirit and Burn Rate keep Running Hot. Hit List (Gabriel Santiago,
  who runs HQ) takes two Lemuria Codecracker for its two Cezve; two
  tournament lists keep Cezve.
- **The observation vocabulary.** Infiltration is the first Core Set card
  built after the packs' reserved blocks, and as `core` it would have
  sorted into the middle of the legacy pool and moved Elevation's slots.
  The Core Set now has a block of its own after the packs'
  (`RESERVED_BLOCKS`, slots 758..=870, by printed code), and the 19 legacy
  Core cards are named (`CORE_FIRST_WAVE`) and keep their slots. No slot a
  model was trained against moves; `CARD_VOCAB` stays 1024.
- **Tests.** Two new tests. Infiltration gains 2, or exposes the one
  unrezzed card (a rezzed one is not offered), and the Runner's view then
  names it. Lemuria's ability is refused before a successful run on HQ,
  then costs [click] and 1[credit] and exposes.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 725 card files.

#### Stage 10b — an expose prevented (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one Corp asset and no new `Effect`.
- **Card from the Core Set:** Zaibatsu Loyalty.

- **An expose is about to happen before it happens.** `Effect::Expose`
  now parks `WouldHappen::Expose { install }` through `prevention::would`,
  like damage and a draw, and announces it whether or not anybody can
  prevent it (`GameEvent::AboutToResolve`, heard as the new
  `Trigger::OnExposeAboutToResolve`, about nothing a filter narrows). What
  is left happens through `ability::expose`, the reveal Stage 10a wrote.
- **"Prevent 1 card from being exposed"** is `Preventable::Expose`, one use
  preventing the one expose. "1[credit] or [trash]:" is two paid
  abilities, as Revolver's "[trash] or hosted power counter" is, each
  quoting the clause with a note.
- **Heard facedown.** "When a card would be exposed, you may rez this
  asset" changes when its card can be rezzed, so it is active while the
  card is not (CR 9.1.8c). A trigger says so with
  `TriggeredEffect::while_unrezzed`, heard by each unrezzed install of the
  card (`Heard::WhileUnrezzed`) and by nothing else, `from_discard`'s
  shape a third time. `validate` refuses it off an installed Corp card.
  The rez is a `PresentChoice` around `RezInstalled` on the card itself,
  which costs 0; the window then opens because the rezzed asset could
  prevent the expose.
- **What the Runner learns.** The Corp is asked only when a Zaibatsu
  Loyalty is installed facedown, so the Runner sees that something heard
  the expose. That is the card's own cost, written on the field.
- **Decks.** A Thousand Cuts (Personal Evolution, a Sweep deck) takes two
  Zaibatsu Loyalty for its two Bladderwort, an asset for an asset; the Au
  Co Clones list keeps Bladderwort.
- **Tests.** Two new tests. A facedown Zaibatsu Loyalty is asked on an
  Infiltration expose, rezzes for 0 and pays 1[credit] to prevent it, and
  the PAD Campaign is never seen; its [trash] half prevents it too, and a
  declined rez lets the expose through.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 726 card files.

#### Stage 11a — the Core Set's Corp operations that compose (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: eight Corp operations, no new vocabulary
and no change to the engine.
- **Cards from the Core Set:** Beanstalk Royalties, Anonymous Tip, Closed
  Accounts, Aggressive Negotiation, Neural EMP, SEA Source, Precognition,
  Shipment from Kaguya.

- **Play requirements are the turn log's.** "Play only if you scored an
  agenda this turn" is `TimesThisTurn(OnAgendaScored)`; "the Runner made
  a run during their last turn" is `TimesLastTurn(OnRunStart)`, and "a
  successful run" `TimesLastTurn(OnSuccessfulRun)`, as Public Trail's;
  "the Runner is tagged" is `IsTagged`.
- **Closed Accounts**: "loses all credits in their credit pool" is
  `LoseCreditsAmount` of the Runner's `Credits`.
- **SEA Source**: `Trace` at base 3 with a tag on success, as Scapenet's.
- **Precognition**: Federal Fundraising's arranging of the top of R&D,
  five deep.
- **Shipment from Kaguya**: Business as Usual's two picks, the second
  refusing the first (`NotSourceCard`), so the two cards are different.
- **Decks.** Supply Chain (OBSH) takes two Beanstalk Royalties and an
  Aggressive Negotiation for its three Hedge Fund; Hostile Bid two
  Shipment from Kaguya for its two Pivot (Supply Chain keeps Pivot); Tag,
  You're It two SEA Source for two of its Public Trail and a Closed
  Accounts for its Hedge Fund (Ad Nihilum keeps Public Trail); Honor Roll
  two Anonymous Tip for two Hedge Fund; Open Book a Precognition for its
  Hedge Fund; A Thousand Cuts a Neural EMP for its Retribution (Permafrost
  keeps it). Each is pinned to Eternal, where the Core Set is legal.
- **Tests.** Five new tests: the three plain operations; Closed Accounts
  refused untagged and then emptying the pool; Aggressive Negotiation
  refused before a score and searching after one; Neural EMP and SEA
  Source refused without the Runner's run and resolving after one;
  Shipment from Kaguya's second pick offering only a different card.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 734 card files.

#### Stage 11b — the Core Set's Runner cards that compose (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: eight Runner cards, no new vocabulary and
no change to the engine.
- **Cards from the Core Set:** Easy Mark, Special Order, Wyldside, Access
  to Globalsec, Akamatsu Mem Chip, Desperado, Armitage Codebusting,
  Magnum Opus.

- **Declared numbers.** "+1[link]" and "+1[mu]" are `ContinuousKind::Link`
  and `Memory` on the controller, as The Toolbox's are; Desperado is a
  console by its printed subtype, so the checkpoint's console limit reads
  it with no line of its own.
- **Wyldside**: Earthrise Hotel's turn-start draw with VRcation's lost
  [click].
- **Armitage Codebusting**: Liberated Account's loaded credits, 12 taken 2
  at a time, trashed when empty.
- **Special Order**: a search of the stack for an `Icebreaker`, revealed,
  as GameDragon Pro's filter reads one.
- **Data Dealer waits.** "[click], forfeit 1 agenda" is the first Runner
  card that forfeits, and `Cost::Forfeit` is the Corp's alone; it joins a
  later stage rather than this one.
- **Decks.** Pay As You Go takes two Wyldside for its two Crash Space;
  Hit List Desperado for its Hermes, a console for a console, and Easy
  Mark and Special Order for its two Concerto, an event for an event; Safety Net an Akamatsu Mem Chip for its Aniccam
  and two Magnum Opus for its two Professional Contacts; Level Pegging two
  Access to Globalsec for its two Decoy and two Armitage Codebusting for
  its two Environmental Testing. Every card given up stays in another
  deck.
- **Tests.** Four new tests: Easy Mark and Special Order (only the
  icebreaker offered); Wyldside on the next turn; the link and memory
  three cards add, and Desperado's credit on a successful run; Armitage
  emptied in six clicks and trashed, and Magnum Opus.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 742 card files.

#### Stage 11c — the Core Set's ice that composes (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: nine pieces of Corp ice, no new
vocabulary and no change to the engine.
- **Cards from the Core Set:** Heimdall 1.0, Ichi 1.0, Viktor 1.0, Neural
  Katana, Wall of Thorns, Data Mine, Hunter, Hadrian's Wall, Shadow.

- **The bioroids** carry Eli 1.0's "Lose [click]: Break 1 subroutine on
  this ice. Only the Runner can use this ability." Their core damage is
  `DealDamage(Brain, n)`, and Ichi's trace does core damage and a tag on
  success.
- **Data Mine** is ice of none of the three types (`IceType::Other`, a
  Trap): "Do 1 net damage. Trash Data Mine." is one subroutine, a
  `Sequence` ending in `TrashCard(ThisCard)`, as Envelopment's is.
- **Hadrian's Wall and Shadow** are advanceable and declare Ice Wall's
  `Strength` per hosted advancement token.
- **Hunter and Shadow** trace at 3 for a tag, as SEA Source does.
- **Decks.** Assembly Line takes two Ichi 1.0 for its two Drafter and two
  Heimdall 1.0 for its two Hákarl 1.0; Retirement Package two Viktor 1.0
  for its two Pulse; A Thousand Cuts two Neural Katana for its two Cloud
  Eater; Open Book two Wall of Thorns for its two Tatu-Bola and two Data
  Mine for its two Phoneutria; Hostile Bid two Hadrian's Wall for its two
  Envelopment and two Shadow for its two Stavka; Tag, You're It two Hunter
  for its two Tithe. Each is ice for ice of the same kind, and every card
  given up stays in another deck.
- **Tests.** Four new tests: the three net-damage ice (Wall of Thorns
  ending the run, Data Mine trashing itself); Viktor's core damage and
  Heimdall broken for a [click]; Hunter's trace and Ichi's, whose success
  is a core damage and a tag; Hadrian's Wall and Shadow advanced and one
  stronger.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 751 card files.

#### Stage 11d — the Core Set's breakers, viruses and rig that compose (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: seven Runner cards, no new vocabulary and
no change to the engine.
- **Cards from the Core Set:** Aurora, Ninja, Battering Ram, Pipeline,
  Yog.0, Datasucker, Grimoire.

- **The breakers** are Corroder's and Gordian Blade's two abilities at
  their own prices: a pump for the encounter (Aurora, Ninja) or for the
  run (Battering Ram, Pipeline), and Yog.0's break for 0[credit] with no
  pump at all.
- **Datasucker** is Leech with Datasucker's words: a virus counter for
  each successful run on a central server, spent for -1 strength on the
  encountered ice until the encounter ends.
- **Grimoire** is +2[mu] and Cookbook's trigger without its "may": every
  virus program installed gets a counter.
- **Déjà Vu waits.** A played event is filed in the heap after its
  `OnPlay` has parked a choice, so Déjà Vu's search of the heap would
  offer the copy resolving it; Networking's "add this event to your grip"
  is written on that filing, so the fix is the play area's, not this
  card's.
- **Decks.** Hit List takes two Aurora for its two Curupira and two Ninja
  for two of its four Matryoshka; Safety Net two Pipeline for its two
  Living Mural; Level Pegging two Battering Ram for its two Gauss; Pay As
  You Go two Yog.0 for its two Utae, two Datasucker for its two Stargate
  and a Grimoire for one of its two Marrow. Each is in faction, and every
  card given up stays in another deck.
- **Tests.** Three new tests: every breaker breaks its own kind at its
  price, after its pump, and not another kind; Datasucker fed by a run on
  Archives and spent on an Ice Wall; Grimoire's memory and its counter on
  a virus and not on a breaker.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 758 card files.

#### Stage 11e — the Core Set's Corp assets and upgrades that compose (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: eight Corp cards, no new vocabulary and
no change to the engine.
- **Cards from the Core Set:** Melange Mining Corp., Adonis Campaign,
  Research Station, Experiential Data, Akitaro Watanabe, Ghost Branch,
  Project Junebug, Security Subcontract.

- **The economy.** Melange Mining Corp. is a three-[click] ability for
  7[credit]; Adonis Campaign is NICO Campaign's shape with 12[credit]
  loaded and nothing drawn when it empties.
- **The upgrades** are declarations: Research Station's
  `install_only_in` HQ and +2 `HandSize`, Experiential Data's `Strength`
  and Akitaro Watanabe's `RezCost` over `IceProtectingThisServer`, as
  Rime's and Vovô Ozetti's are.
- **The ambushes** are heard on access, installed, as Cerebral
  Overwriter is: Ghost Branch's "may" gives a tag per advancement token,
  and Project Junebug's paid choice does twice that in net damage
  (`Amount::Times`).
- **Security Subcontract**'s rezzed piece of ice is a `Cost::Trash` from
  the Corp's installs, filtered `All([Ice, Rezzed])`.
- **Decks.** Hostile Bid takes two Security Subcontract for its two
  Svyatogor Excavator and a Research Station for its Isaac Liberdade;
  Retirement Package two Adonis Campaign for its two Trieste Model
  Bioroids and two Experiential Data for its two Tranquility Home Grid; A
  Thousand Cuts two Project Junebug for its two Moon Pool and an Akitaro
  Watanabe for one of its two Mavirus; Paid Content two Ghost Branch for
  its two Drago Ivanov and a Melange Mining Corp. for one of its two The
  Powers That Be. Each is in faction or neutral for neutral, and every
  card given up stays in another deck.
- **Tests.** Five new tests: Melange's 7[credit] and Adonis Campaign's
  3[credit] a turn until it is trashed empty; Research Station refused
  outside HQ and its hand size; Experiential Data's strength and
  Akitaro's rez discount, on their server only; Ghost Branch's tags and
  Junebug's damage on access; Security Subcontract refused with no
  rezzed ice and paid with one.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 766 card files.

#### Stage 11f — the Core Set's agendas and tracer ice that compose (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: seven Corp cards, no new vocabulary, and
one engine fix the sweep found.
- **Cards from the Core Set:** Priority Requisition, Private Security
  Force, Posted Bounty, AstroScript Pilot Program, Breaking News, Matrix
  Analyzer, Data Raven.

- **Priority Requisition** is Send a Message's scored "may": a piece of
  unrezzed ice rezzed ignoring all costs.
- **Private Security Force** is a scored agenda's [click] ability for a
  meat damage, `IsTagged` its requirement, as False Lead's forfeit is
  used from the score area.
- **Posted Bounty**'s "you may forfeit it. If you do" is Divested Trust's
  paid choice with `ForfeitSelf`, a tag and a bad publicity behind it.
- **AstroScript Pilot Program** is Project Atlas's counter and Hype
  Machine's placement: a hosted agenda counter for an advancement counter
  on any card that can be advanced.
- **Breaking News** tags twice as it is scored and removes them when the
  discard phase ends, if it was scored this turn
  (`ThisAgendaScoredThisTurn`, Witch Hunt's requirement). The trigger
  hears the Corp's own discard phase, which is the first to end after a
  score in the Corp's turn; no card in the pool scores in the Runner's.
- **A stolen agenda is inactive** unless its text says otherwise (CR
  3.2.3, 4.5.4). `engine::activate_ability` found an ability on an agenda
  in the Runner's score area for Oracle Thinktank, whose text does say so,
  and so offered the Corp every stolen agenda's ability; the 32-seed
  sweep stopped on a stolen Private Security Force, whose owner the action
  list could not name. The ability is now refused there unless its
  requirement holds `InRunnersScoreArea`
  (`EffectRequirement::works_from_runners_score_area`), and the test of
  Private Security Force asks it stolen.
- **Matrix Analyzer** offers its 1[credit] advancement as it is
  encountered and traces at 2 for a tag; **Data Raven** puts "take 1 tag
  or end the run" to the Runner as it is encountered, traces at 3 for a
  power counter, and spends one on a tag.
- **Decks.** Paid Content takes two Breaking News for its two Post-Truth
  Dividend, two AstroScript Pilot Program for two of its three Kingmaking
  and two Matrix Analyzer for its two Unsmiling Tsarevna; Pay to Win two
  Data Raven for its two Lethe; Tag, You're It a Posted Bounty for one of
  its three Hostile Takeover, a Private Security Force for its Above the
  Law and a Priority Requisition for its The Basalt Spire. Points are
  kept point for point, ice sentry for sentry, and every card given up
  stays in another deck.
- **Tests.** Four new tests: Priority Requisition's free rez and Posted
  Bounty forfeited or kept; AstroScript's advancement and Breaking News's
  tags gone as the turn's discard phase ends; Private Security Force
  refused untagged and used tagged; Matrix Analyzer's paid advancement and
  trace, and Data Raven's two answers and its counter.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 773 card files.

#### Stage 11g — Core Set cards that compose with what the pool has (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: five cards, no new vocabulary and no change
to the engine.
- **Cards from the Core Set:** Modded, Wyrm, Red Herrings, Cell Portal,
  Shipment from MirrorMorph.

- **Modded** is Career Fair with a program or a piece of hardware in
  place of a resource: an install from the grip for 3[credit] less.
- **Wyrm** is three paid abilities: Leech's "-1 strength for the
  remainder of this encounter" for 1[credit], a pump, and a break at
  3[credit] whose requirement is the encountered ice at 0 strength or
  less (`Not(AmountAtLeast(EncounteredIceStrength, 1))`, Chisel's test).
- **Red Herrings** is Daniela Jorge Inácio's persistent steal cost with
  5[credit] in place of two grip cards (`StealingFromThisServer`).
- **Cell Portal** is Letheia Nisei's move to the outermost position and
  offer to jack out, then a `DerezCard` of itself, which happens whether
  or not the Runner leaves.
  It is the first card the sweeps' decks carry that derezzes a piece of
  ice in the middle of a run, and the client's run trail
  (`board::trail::RunTrail::sync`) kept naming it to the Runner after the
  view had turned it facedown; the trail now forgets a name the view no
  longer gives, which `a_trail_follows_real_runs_as_both_viewers` found.
- **Shipment from MirrorMorph** is Humanoid Resources' install offer three
  times: each a card from HQ that is not an operation, installed paying
  its costs, one at a time.
- **Deferred:** Aggressive Secretary, because a "that many" selection
  (`PromptChooseCards::count`) does nothing when fewer cards qualify,
  where "trash 1 program for each advancement token" trashes every program
  there is; Tinkering, because no effect gives a chosen piece of ice
  subtypes until the end of the turn.
- **Decks.** Safety Net takes two Modded for its two Spark of
  Inspiration; Pay as You Go two Wyrm for its two Take a Dive; A Thousand
  Cuts two Cell Portal for its two Knowledge Seeker, code gate for code
  gate; Retirement Package two Shipment from MirrorMorph for its two
  Archived Memories; Fine Print two Red Herrings for its two Amaze
  Amusements. Every card given up stays in another deck.
- **Tests.** Five new tests: Modded offering a program and not a
  resource; Wyrm refused at 1 strength and breaking at 0; Red Herrings
  stealable with 5[credit] and not with 4; Cell Portal derezzed whether
  the Runner stays or jacks out; Shipment installing two cards and never
  offering an operation.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 778 card files.

#### Stage 11h — "that many" is as many as there are (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: three cards, no new vocabulary, and one
change to how a selection counts.
- **Cards from the Core Set:** Aggressive Secretary, Demolition Run,
  Rabbit Hole.

- **"That many" when fewer qualify.** A selection whose number is an
  amount (`PromptChooseCards::count`) did nothing at all when fewer cards
  qualified than the amount. CR 1.2.4 says as much of an instruction as
  possible is carried out, so it now chooses every card there is. Simulation
  Reset, the one card that used it before, is unchanged: it shuffles back
  the cards it has just trashed, which are always there.
- **Aggressive Secretary** is Project Junebug's paid access with a trash
  of one program for each advancement token, deferred in Stage 11g on
  exactly the case above: three tokens against two programs trash both.
  It works facedown, so the client's list of traps whose rez gains
  nothing (`board::rez`) names it beside Project Junebug.
- **Demolition Run** is Jailbreak's choice of HQ or R&D and Eye for an
  Eye's access ability, at 0[credit].
- **Rabbit Hole** is Access to Globalsec's +1[link] and Self-modifying
  Code's search of the stack, narrowed to another copy of Rabbit Hole and
  paid for. The copy installed hears its own install and may fetch a third.
- **Decks.** Retirement Package takes two Aggressive Secretary for its two
  Cerebral Overwriter; Pay as You Go two Demolition Run for its two
  Chastushka; Safety Net two Rabbit Hole for its two Simulchip. Every card
  given up stays in another deck.
- **Tests.** Three new tests: Aggressive Secretary with one token against
  two programs and three against two; Demolition Run refused on Archives
  and trashing an operation for nothing; Rabbit Hole fetching the one copy
  in the stack, paying for it, for +2[link].
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 781 card files.

#### Stage 11i — a remote server, and the chosen ice for the turn (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: two cards and two words, with no new
`Effect`.
- **Cards from the Core Set:** Bank Job, Tinkering.

- **"A remote server" in a trigger's condition** (`EventFilter::
  ServerKind`). `EventFilter::Server` names servers, and its doc said a
  remote server, whose number no card knows, would need a variant here;
  Bank Job is the first card to print it. The turn log counts it as the
  columns of the servers of that kind, so "the first time each turn" can
  narrow by it too.
- **Bank Job** is Armitage Codebusting's loaded credits and Account
  Siphon's optional replacement of the breach, with a number from 0 to
  what is left on it; emptied, it trashes itself. A run on a central is
  breached as usual.
- **A chosen ice's subtypes for the turn** (`GainIceSubtype::
  for_the_turn`). `This` was Lycian Multi-Munition's own ice while it
  stays rezzed; inside a selection's `then` it is the ice chosen, rezzed or
  not, and the word makes the gain last until the end of the turn and
  credits it to the card whose text it is. `validate` refuses the word on
  the encountered ice or beside `for_the_run`.
- **Tinkering** chooses a piece of ice, which gains sentry, code gate and
  barrier until the end of the turn.
- **Decks.** Encore takes two Bank Job for its two Info Bounty;
  Level Pegging two Tinkering for its two Deep Dive. Every card given up
  stays in another deck.
- **Tests.** Two new tests: Bank Job loading 8[credit], breaching a
  central as usual, taking 3 from a remote run and then the other 5, which
  trashes it; Tinkering making a facedown Ice Wall all three types, which
  are gone the next turn.
- **DSL ratio** (`pool_status.py`): 16 of 109 `Effect` variants
  single-use, none unused, over 783 card files.

#### Stage 11j — a redirect taken as the run would be declared successful (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one card, in the Core Set and System
Update 2021, and one new `Effect`.
- **Card:** Sneakdoor Beta, which leaves System Update 2021's list of
  unbuilt cards (5 remain).

- **"If that run would be declared successful, change the attacked
  server to HQ"** (`Effect::RedirectRunOnSuccess`). Maintenance Access's
  redirect moves the run as it would approach Archives, so Archives is
  never approached; Sneakdoor Beta's approaches Archives, and what acts
  on that approach acts, before the run moves. The run carries the
  redirect with a flag saying when it is taken
  (`RunState::redirect_at_success`, public in the view), and
  `engine::complete_run` takes it before the declaration, so what is
  declared successful and breached is the run on HQ, and a card saying
  runs on HQ cannot be declared successful withholds it. Both redirects
  share `run::redirect_to`. Composition didn't work: the one redirect
  there was skipped the approach the card prints.
- **Decks.** Encore takes two Sneakdoor Beta for its two Legwork, which
  stay in Express Delivery.
- **Tests.** Sneakdoor Beta approaching Archives, then declared
  successful on HQ, breaching it past HQ's ice without encountering it.
- **DSL ratio** (`pool_status.py`): 17 of 110 `Effect` variants
  single-use, none unused, over 784 card files.

#### Stage 11k — rez the chosen ice, or trash it (8 October 2026)

`claude/nsg-tranche-8-qmn4v7`: one card, in the Core Set and System
Update 2021, with no new `Effect`.
- **Card:** Forged Activation Orders, which leaves System Update 2021's
  list of unbuilt cards (4 remain).

- **"The Corp may rez that ice. If they do not, they trash it."** The
  Runner chooses an unrezzed piece of ice, and the selection's `then` is
  the Corp's choice: rez it, paying, or trash it. A rez the Corp cannot
  pay resolves to nothing (`Effect::RezInstalled`), so the rez option is
  followed by "if it is still unrezzed, trash it", which is the printed
  sentence and keeps an unaffordable rez from saving the ice.
- **An install's copy can be asked whether it is unrezzed**
  (`pending_choice::copy_matches`, `CardFilter::Unrezzed`). The
  definition's half of `ActingCardMatches` passes an instance filter, and
  the copy's half did not know this word; no card used it there before,
  so nothing else changes.
- **Decks.** Encore takes two Forged Activation Orders for its two
  Emergency Shutdown, which stay in Express Delivery.
- **Tests.** Forged Activation Orders rezzing the Ice Wall for 1[credit],
  trashing it when the Corp declines, and trashing it when the Corp
  chooses a rez it cannot pay.
- **DSL ratio** (`pool_status.py`): 17 of 110 `Effect` variants
  single-use, none unused, over 785 card files.
