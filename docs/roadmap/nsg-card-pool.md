# The Null Signal Games card pool — set by set (Phase 1 §9, NSG half)

Phase 1 §9 ([phase-1-single-player.md](phase-1-single-player.md)) asks for
every card NetrunnerDB lists, expressed in the DSL. This file is its plan
for the sets Null Signal Games published, and the record each stage adds to
as it lands. The Fantasy Flight Games cycles get a plan of their own after
this one closes.

**Written 26 September 2026, before any of it is built.** The inventory,
the pools and the ban lists below were read off NetrunnerDB that day. The
commands are under [How the numbers were taken](#how-the-numbers-were-taken)
so the next session re-takes them rather than trusting them. The per-set
surveys are one reading of each card's text against the DSL as it stood at
`f81b77c`. Each set's first stage re-reads its survey before building on it.

## Decisions (26 September 2026, the person's)

- **Order: Vantage Point first, then Standard from newest to oldest, then
  the reprint packs.** Vantage Point alone completes Startup (System
  Gateway + Elevation + Vantage Point). The Liberation sets come next
  because their threat and Trojans are already in the engine, so the
  cheapest sets go first.
- **Decks: NSG's published lists where they exist, and lists this project
  builds everywhere else.** NSG publishes sample lists only for System
  Gateway alone and System Gateway + Elevation, both already shipped. No
  NSG set in this plan has any. Each set is therefore reached by
  `DeckCategory::Sweep` decks built here. When Standard is complete,
  current Standard tournament lists from NetrunnerDB join the `Sample` pool
  and `decks::tests::PUBLISHED` pins them.
- **The reprint packs are the last NSG tranche.** System Update 2021,
  Salvaged Memories and the Magnum Opus Reprint are NSG products of FFG
  designs, and none of them is in Standard. They are the bridge into the
  FFG plan.
- **The observation vocabulary grows once, up front** (Stage 0), with a
  rank reserved per pack. The alternative was one reshape and one retrain
  per set.

## Inventory and order

"Unbuilt" counts unique titles with no card file, less reprints of cards
already built. Where two tranches share a title, it is counted in the
earlier tranche.

| # | Tranche | Packs (codes) | Printed | Unbuilt | Corp / Runner | Identities | Standard-banned |
|---|---|---|---|---|---|---|---|
| 1 | Vantage Point | `vp` 36001–36066 | 66 | 66 | 41 / 25 | 4 | — |
| 2 | Rebellion Without Rehearsal | `rwr` 34066–34130 | 65 | 65 | 35 / 30 | 3 | Tributary, Trick Shot |
| 3 | The Automata Initiative | `tai` 34001–34065 | 65 | 65 | 35 / 30 | 4 | Cybersand Harvester |
| 4 | Parhelion | `ph` 33066–33128 | 63 | 63 | 34 / 29 | 4 | Dr. Vientiane Keeling, K2CP Turbine, Matryoshka, Nanisivik Grid, Tsakhia "Bankhar" Gantulga, World Tree |
| 5 | Midnight Sun + Booster Pack | `msbp` 32001–32007, `ms` 33001–33065 | 72 | 65 | 35 / 30 | 5 | Drago Ivanov, Endurance, Nyusha "Sable" Sintashta, Svyatogor Excavator |
| 6 | Uprising + Booster Pack | `urbp` 27001–27007, `ur` 26066–26130 | 72 | 65 | 35 / 30 | 3 | Bellona, Cayambe Grid, Cyberdex Sandbox, Engram Flush, Gold Farmer, Hoshiko Shiro, Moshing, Project Vacheron |
| 7 | Downfall | `df` 26001–26065 | 65 | 65 | 35 / 30 | 4 | Bukhgalter, Rezeki, Sting! |
| 8 | NSG reprint packs | `su21` 31001–31082, `sm` 29001–29018, `mor` 28001–28006 | 106 | 91 | 47 / 44 | 8 | — (not in Standard) |

**454 unbuilt cards for Standard and 91 for the reprints.** Standard 2026
(`standard_2026_vantage_point`, 613 cards) is System Gateway, Ashes
(Downfall, Uprising and its booster), Borealis (Midnight Sun and its
booster, Parhelion), Liberation (The Automata Initiative, Rebellion Without
Rehearsal), Elevation and Vantage Point. Midnight Sun and Uprising each
reprint their own booster pack's 7 cards, so each tranche counts 7 fewer
than it prints. In the reprint packs, 11 System Update 2021 titles (The
Maker’s Eye and two identities among them) and Salvaged Memories'
Scorched Earth are already built.

A card on a ban list is still built, because Eternal allows it. Within its
set it goes last.

## Stage 0 — groundwork, once, before tranche 1

No cards. Split in two when it was taken (26 September 2026), because the
formats half reaches into both clients and the rest does not:

- **Stage 0a — the catalog, the gates, the vocabulary, the status script.**
- **Stage 0b — formats from NetrunnerDB, and a deck's legality per
  format.**

Both are recorded below. Tranche 1, Vantage Point, is next.

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

## The recipe — every stage of every tranche

1. **Read the rule before the card.** For every mechanic a stage adds,
   `grep -n` its section in `rules/comprehensive-rules.md`, and add or
   update its row in [rules-conformance.md](rules-conformance.md). A
   mechanic is built in the first tranche that needs it (see [Mechanics
   shared across tranches](#mechanics-shared-across-tranches)), in the
   place AGENTS.md's rules already say it goes:
   - Listener: an event condition is a `when`, never a `Trigger`.
   - Continuous Effect: a standing effect is a `ContinuousKind`, never a
     field.
   - Prevention: a new "would" is a `Preventable`.
   - Payment: a new restriction on credits is a `Purpose` or `PaysFor`.
   - Turn History: "this turn" is the log.
2. **A stage is one mechanic, then the cards that compose on it.** That is
   8–10 cards plus the Sweep decks that reach them. Each stage is branch
   `feat/<set>-stage-<n>-<subject>` with one commit: the subject says what
   is now true, and the body carries the numbers (AGENTS.md Git Hygiene).
   The survey's stage order is where to start, not a contract.
3. **Decks.** Build Sweep decks from the tranche's cards plus the pool
   already built. They are Eternal-legal, at least one per faction the set
   prints, and use identities that no other deck uses where the set has
   them. The card gate (`played_pool_card_ids`, eight seeds) then demands
   every card. Sweep decks never enter `matchups()`, so no training number
   moves while the tranches land.
4. **Each card file** carries its `numeric_id` — the NSG printing's code,
   which is what places it in its pack's vocabulary block (Stage 0a) — and every choice and
   ability carries its printed clause (the Linked Clause Rule's quote
   gate). Each card gets a per-card test. The stage shrinks the set's
   `UNIMPLEMENTED` list.
5. **Before merging:**
   - `cargo test --workspace` green and `cargo clippy --workspace
     --all-targets` silent.
   - Both sweeps at `NETRUNNER_SWEEP_SEEDS=256`, `--release`.
   - A random-vs-random `--all-matchups` report sized to at least one full
     pass of the cross product.
   - `pool_status.py`'s ratio, and its pack counts.
   If a stage's single-use `Effect` variants approach its card count, stop
   and build a composition primitive first (the DSL Growth Rule).
6. **Record** a stage entry under its tranche below: the cards; each new
   primitive and why composition failed; what the sweeps found; the
   fidelity limits; the ratio. A tranche closes when its list is empty.
   Its packs then join the deck builder's legal pool for their formats,
   and the `ROADMAP.md` row moves.

## The surveys

Each unbuilt card was read against the DSL at `f81b77c` and put in one of
three classes:

- **C**: composes from existing variants, at most a new filter or amount.
- **V**: needs vocabulary, meaning a new `Trigger`, `EventFilter`,
  `Amount`, `CardFilter`, `Cost`, `ContinuousKind` or `PaysFor` word and
  no new rules machinery.
- **M**: needs a mechanic the engine does not model.

The class counts are one reader's judgement. The first stage of each
tranche re-reads them.

**What the survey found already there:**
- `GiveBadPublicity`, and `RemoveBadPublicity` (unused).
- `DerezCard`, `SwapInstalledIce`, `SwapApproachedIceWithCard`,
  `BypassEncounteredIce`, `Sabotage`, `FlipIdentity` (both sides, one
  bit), `Amount::ThreatLevel`, `AddAdditionalAccess`, `PurgeVirusCounters`.
- `GainClicksNextTurn`, which applies to the Corp only
  (`rules/ability.rs`).
- Trojans (`installs_on_ice`), `HostRigCardOnInstall`, `HostCardOnThisCard`.
- `steal_cost`, which no card sets yet; `additional_play_cost`;
  `persistent_after_trash`; `rez_alternatives`.
- `Effect::Trace`, which no card uses yet.

"Interface →" needs no word, because a break is already `Paid` +
`DuringEncounter`.

**What every tranche needs first — two groundwork gaps, not mechanics:**

1. **Events a card cannot hear yet.** `GameEvent::IcePassed`,
   `IceBypassed`, `SubroutineBroken`, `EventPlayed`, `CardTrashed`,
   `CreditsSpent` and the turn's end all exist, and `listeners::moments`
   gives them no trigger. Every tranche has cards on them: Vertigo, Sipa
   and Lethe (VP); Amanuensis and Physarum Entangler (RWR); Phoneutria and
   Curupira (TAI); Abaasy (PH); Mystic Maemi and Gold Farmer (UR). They are
   `moments` arms and `Trigger` words, each held to the Listener Rule
   (conditions go in `when`), and the dispatch audit names every site that
   emits one without dispatching it.
2. **`CardSubtype` is a closed list of about ten words.** Every tranche
   prints more: AP, Destroyer, Observer, Harmonic, Liability, Stealth,
   Virtual, Companion, Daemon, Decoder, Killer, Weapon, Expendable,
   Terminal and others. CR 2.16.7 lists them all. Taking the whole list
   once, checked against the catalog's `keywords`, costs nothing per card
   later.

The subtypes land as **Vantage Point Stage 1a**, ahead of any card. **The
events do not** (corrected when Stage 1 was taken, 26 September 2026): a
`Trigger` no card uses is vocabulary the DSL Growth Rule refuses, so each
event is heard in the stage that builds the first card listening for it —
Vertigo, Sipa and Lethe in Stage 4 for passing and breaking ice.

### Mechanics shared across tranches

Each is built once, in the first tranche (in this file's order) that
prints it, and every later tranche composes on it. Read the rule before
building.

| Mechanic | First needed | Also in | Rule |
|---|---|---|---|
| Credits spendable only from stealth cards (a restriction on the *payer*, the reverse of `PaysFor`) | VP: Corsair, Lampades, Baker | UR: Mu Safecracker, Afterimage, Penrose | CR 1.10.4b |
| Additional subroutines, ordered | VP: Stick and Poke | RWR Thunderbolt Armaments; TAI Starlit Knight; MS Echo, Envelopment; UR Winchester | CR 9.8.2, CR 9.8.3, CR 6.5.7d |
| Arrange | VP: Cultivate, Knowledge Seeker | RWR Cataloguer; TAI Federal Fundraising | CR 8.3.1 |
| Reveal as a step other cards can read | VP: Esca, Perfect Recall, Tocsin | RWR Burner, Bring Them Home | CR 1.21.3 |
| Abilities active outside play (Expendable from HQ; from Archives or the heap) | VP: Tocsin | RWR Eminent Domain, Descent; TAI Slash and Burn Agriculture, Tree Line, Angelique; reprints Subliminal Messaging, Crowdfunding | CR 9.1.8b |
| A card added to a score area "as an agenda" | VP: Word on the Street, Myōshu | RWR Jeitinho, Kingmaking; PH Nightmare Archive; MS Regenesis, Backroom Machinations | CR 1.17.3f |
| Hosting in general: install onto a card, facedown hosted cards, host limits | VP: Hackerspace, Read-Write Share, Luana Campos | RWR Spree; PH Matryoshka | CR 1.13.5, CR 1.13.7 |
| Runner cards removed from the game | VP: Take a Dive, Kompromat | TAI Capybara; PH Nanuq; UR Devil Charm, The Back, Buffer Drive | CR 4.9 |
| Additional costs imposed by another card (steal, score, run, trash) | VP: Magistrate Revontulet | RWR Sebastião Souza Pessoa; TAI Daniela Jorge Inácio; MS Azef Protocol; UR NAPD Cordon, Earth Station: SEA Headquarters; DF Cold Site Server, Reduced Service | CR 1.16.10, CR 6.3.2b |
| Terminal: the action phase is forced to end | RWR: Active Policing, Bring Them Home | TAI Oppo Research; MS Big Deal | CR 5.4.3 |
| Psi game: a simultaneous secret bid | RWR: See How They Run | TAI Adrian Seis; UR Konjin, Hyoubu Precog Manifold | CR 10.14.6 |
| Set aside | RWR: The Wizard’s Chest | PH Spark of Inspiration; MS Deep Dive; UR Gachapon | CR 4.8 |
| X costs | RWR: Lobisomem | PH Matryoshka; DF Utae; reprints Corporate Troubleshooter, Psychographics | CR 1.16.2c |
| Forced or repeated encounter | RWR: Sisyphus Protocol | UR Konjin, Ganked! | CR 6.1.3 |
| Losing abilities | PH: Hush, Klevetnik | MS Light the Fire! | CR 9.1.9a |
| Break restrictions ("cannot be broken", "only by …") | PH: Anvil, Unsmiling Tsarevna, Hafrún | MS Trieste Model Bioroids; UR Akhet, NEXT Activation Command | CR 9.8.5 |
| Charge | PH: Flux Capacitor, Orca | MS Captain Padma Isbister, Rigging Up, “Daeg, First Net-Cat”, Stoneship Chart Room | CR 10.10 |
| Mark | PH: Tunnel Vision, Info Bounty | MS Nyusha "Sable" Sintashta, Carpe Diem, Virtuoso, Backstitching | CR 10.11 |
| An agenda's points or advancement requirement changing | VP: Let Them Dream | PH Ontological Dependence, Freedom of Information, Regulatory Capture; UR Megaprix Qualifier, Project Vacheron; reprints Project Beale, SanSan City Grid | CR 3.2.2, CR 3.2.3b |
| A choice remembered for a duration (a server, an ice, a subtype, a card's name) | RWR: Lycian Multi-Munition | MS Trieste Model Bioroids; UR Boomerang, Engram Flush; DF Whistleblower, Complete Image, Saisentan; reprints Femme Fatale, Security Testing, Chameleon | CR 9.10.3 |
| A triggered ability created by a card that resolved ("when your next run ends…") | DF: In the Groove, Climactic Showdown, Always Have a Backup Plan | reprints Inside Job, Test Run | CR 9.10 |
| Lockdown | UR: SYNC Rerouting, Argus Crackdown, NAPD Cordon, NEXT Activation Command, Hyoubu Precog Manifold | — | CR 3.5.1c |

Two more mechanics, each first needed by a single card:
- **Méliès U's secret, three-sided identity** (CR 1.5.2b). The flip is one
  bit today.
- **A run that "cannot be declared successful"** (Flagship, VP; CR 6.8.4a),
  which every consumer of a successful run has to hear. Crisium Grid in the
  reprints reuses it.

### 1. Vantage Point — 66 cards (C 13 / V 36 / M 17)

**Decks:** none published. Build Sweep decks, one per faction, on the four
new identities: Vic, Hiram "0mission" Svensson, Méliès U and Editorial
Division.

**Stages** (card lists from the survey):
1. **1a — every subtype, read from the catalog** (below). **1b — the
   no-new-word cards:** Virtual Intelligence P.I., Sell Out, Borrowed
   Goods, Rotary, Méliès City Luxury Line (the first `steal_cost`),
   Sleipnir, Retirement Plan, Nihilo Agent, Grubber, Paywall, Scapegoat,
   Flywheel, Vulture Fund.
2. **Bad publicity, tags and costs:** Editorial Division, Witch Hunt, Take
   a Dive, Kompromat, Reanimation Protocol, Unleash, realloc(), Flood the
   Market (built, below). **2b — ice with no Barrier, Code Gate or
   Sentry:** Vicsek, alone, with the change to `CardType::Ice` it needs,
   which the rest of the unmodelable ice (Loot Box, Rime, Konjin,
   Excalibur, Lycian Multi-Munition) then reuse (built, below).
3. **Turn-log counts and standing kinds** (3a and 3b built, below) (`PlayCost`, `AgendaPoints` and
   `StealCost` are the `ContinuousKind` words, each a new kind held to the
   DSL Growth Rule): Chain Reaction, Underdome Irregulars, Reverb, Hype
   Machine, Tailgate, Perfect Recall, The Red Room, Lotus Haze, Magistrate
   Revontulet, Let Them Dream, and Synchrocyclotron, moved here from Stage
   2 because "the first double operation you play each turn costs [click]
   less" is a `PlayCost` on a click, with the "first each turn" of a price
   (`TurnLog::none_yet`).
4. **Ice and run triggers:** **4a**, the run's moments about ice
   (built, below): Vertigo, Sipa, Lethe, ezaM, The Tungsten Tailor.
   **4b**: Lionsmane, Event Horizon, Ansel 2.0.
5. **Runner trigger and payment words** (`PaysFor` during a run; the
   Runner's allotted clicks): Hiram, Touchstone, Methuselah, Beta Build,
   Stowaway, Nurse Hạnh, Caveat Emptor, and Shackleton Grid, moved here
   from Stage 4: "when the Runner spends credits from outside their
   credit pool" is a moment every payment site would have to dispatch,
   which is this stage's payment work.
6. **Arrange, reveal, stealth credits:** Cultivate, Knowledge Seeker, Esca,
   Corsair, Baker, Lampades, Aircheck. Aircheck locks the credit pool,
   which breaks the invariant that a payment may always reach the pool.
7. **Hosting, the score area and access limits:** Hackerspace, Read-Write
   Share, Luana Campos, Stick and Poke, Word on the Street, Myōshu,
   Flagship, Sacrifice Zone Expansion, Tocsin.
8. **Méliès U, alone:** hidden setup state, three sides, and masking.

**Riskiest:**
- Méliès U.
- Flagship: "not declared successful" reaches every successful-run
  listener.
- Word on the Street: a Runner card scored as a −1 Corp agenda through an
  additional score cost.
- Aircheck.
- Stick and Poke: gained subroutines change a subroutine's index and what
  "fully broken" means.

**Banned:** Let Them Dream, on Startup's list only.

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

### 2. Rebellion Without Rehearsal — 65 cards (C 7 / V 43 / M 15)

**Decks:** Sweep decks on its three identities.

**Stages:**
1. **Composes, plus small requirements:** Eye for an Eye, Friend of a
   Friend, Corporate Hospitality, Boto, Capacitor, Seraph, The Powers That
   Be, Coalescence, Valentina Ferreira Carvalho, Pressure Spike.
2. **Tags, purge, and a cost on the Corp's basic trash action:**
   Sebastião Souza Pessoa, Manuel Lattes de Moura, Privileged Access, Amanuensis, Amelia Earhart,
   Juli Moreira Lee, Arruaceiras Crew, Malandragem, Physarum Entangler,
   Heliamphora.
3. **Runner access and run words:** “Pretty” Mary da Silva, Cupellation, Trick Shot,
   Window of Opportunity, Alarm Clock, Meeting of Minds, Muse, Ashen
   Epilogue, Boi-tatá.
4. **Corp advancement and movement words:** Charlotte Caçador, Cohort
   Guidance Program, Hearts and Minds, Logjam, Isaac Liberdade, The Holo
   Man, Business As Usual, Janaína “JK” Dumont Kindelán, Kingmaking, Stoke the Embers.
5. **Corp ice and rez words:** Lightning Laboratory, Warm Reception,
   Working Prototype, Brasília Government Grid, Sorocaban Blade, Hammer,
   Cloud Eater, Piranhas, Sudden Commandment, Nuvem SA: Law of the Land, The Basalt Spire.
6. **Terminal, reveal, set aside, arrange, X:** Active Policing, Bring
   Them Home, Burner, The Wizard’s Chest, Cataloguer, Lobisomem.
7. **Expendable, moving ice, psi, re-encounter:** Eminent Domain, Descent,
   Tributary, See How They Run, Sisyphus Protocol, Spree.
8. **A card's identity changes:** Thunderbolt Armaments: Peace Through Power, Lycian
   Multi-Munition, Jeitinho.

**Riskiest:**
- Jeitinho: a third way to win (`rules/win.rs`).
- Sisyphus Protocol: an encounter re-entered, when the run's state machine
  only moves forward.
- Tributary: ice moved mid-run, reconciled with `RunState::ice`.
- Lycian Multi-Munition: a subtype chosen at rez that persists.
- See How They Run.

**Banned:** Tributary and Trick Shot, on Standard's list.

### 3. The Automata Initiative — 65 cards (C 14 / V 35 / M 16)

**Decks:** Sweep decks on its four identities.

**Stages:**
1. **Corp, composes:** Salvo Testing, Fujii Asset Retrieval, Jaguarundi, Attini,
   Mindscaping, Pivot, Cybersand Harvester, Behold!, Balanced Coverage.
2. **Runner, composes, plus small words:** Joy Ride, Shibboleth, LilyPAD,
   Solidarity Badge, Audrey v2, Your Digital Life, Armed Asset Protection,
   Valentão, B-1001, M.I.C.
3. **Pass and fully-broken triggers, ice subtypes:** Phoneutria, Tatu-Bola,
   Virtual Service Agent, Curupira, Laser Pointer, Banner.
4. **Trojan and host scopes, returning an install to the grip:**
   Monkeywrench, Saci, Slap Vandal, Umbrella, Living Mural, Pichação, Urban
   Art Vernissage, Hermes, Stegodon MK IV.
5. **Corp install and server words:** Vovô Ozetti, Greasing the Palm,
   Ablative Barrier, Tucana, Front Company, Federal Fundraising, Epiphany
   Analytica: Nations Undivided, Lago Paranoá Shelter.
6. **Runner run words:** Hannah "Wheels" Pilintra, Mercury: Chrome Libertador, Debbie "Downtown" Moreira, Bahia Bands,
   Chrysopoeian Skimming, Strike Fund, The Price.
7. **Expendable, breaching another server, terminal, Runner removal from
   game:** Slash and Burn Agriculture, Tree Line, Angelique, Eru Ayase-Pessoa, Beatriz Friere Gonzalez, Oppo
   Research, S-Dobrado, Capybara.
8. **Each needs machinery of its own:** Adrian Seis, Daniela Jorge Inácio, Starlit
   Knight, AirbladeX (JSRF Ed.) (prevents a "when encountered" ability, a new
   `Preventable`), Arissana Rocha Nahu (a Trojan installed by an effect, which
   `InstallRunnerCardFromGrip` excludes today), A Teia: IP Recovery, Wage Workers,
   Oracle Thinktank.

**Riskiest:**
- Adrian Seis.
- Daniela Jorge Inácio: a cost on another card's steal or trash, which persists after
  Daniela is trashed.
- Starlit Knight.
- Eru Ayase-Pessoa and Beatriz Friere Gonzalez: breaching R&D inside an Archives run.
- Wage Workers: "the same action" defined across basic actions and
  abilities.

**Banned:** Cybersand Harvester.

### 4. Parhelion — 63 cards (C 19 / V 26 / M 18)

**Decks:** Sweep decks on its four identities. Nova Initiumia and Ampère
change deck-building rules, which the validator does not model yet
(`deck_limit` only).

**Stages:**
1. **Corp, composes:** Thule Subsea: Safety Below, Distributed Tracing, Djupstad Grid,
   Reaper Function, Vampyronassa, Post-Truth Dividend, Gaslight, Vera
   Ivanovna Shuyskaya, Nonequivalent Exchange, Shipment from Vladisibirsk.
2. **Mixed, composes:** Hostile Architecture, End of the Line, Finality,
   Katorga Breakout, Nga, Num, Zenit Chip JZ-2MJ, Hippocampic Mechanocytes, Dr. Nuka
   Vrolyck.
3. **Advancement requirement** (the deferred `ContinuousKind` its doc
   names) **and Corp words:** Ontological Dependence, Freedom of
   Information, Regulatory Capture, Pulse, Bloop, Simulation Reset,
   Hypoxia, Mr. Hendrik.
4. **Runner standing and breaker words:** Basilar Synthgland 2KVJ, Dr.
   Vientiane Keeling, K2CP Turbine, Tremolo, Time Bomb, Poison Vial, WAKE
   Implant v2A-JRJ, Abaasy.
5. **Zones, selection and deck building:** Hybrid Release, Nanisivik Grid,
   Kimberlite Field, World Tree, Asmund Pudlat, Concerto, Reprise, Yakov Erikovich Avdakov,
   Nova Initiumia: Catalyst & Impetus, Ampère: Cybernetics For Anyone.
6. **Charge, mark, set aside, Runner removal from game, counters on a run
   event:** Flux Capacitor, Orca, Tunnel Vision, Info Bounty, Spark of
   Inspiration, Nanuq, Raindrops Cut Stone.
7. **Winning and the score area:** Issuaq Adaptics: Sustaining Diversity, Superdeep Borehole, Nightmare
   Archive, Matryoshka (X cost).
8. **Losing abilities and break restrictions** (last, because they touch
   every read of an ability): Hush, Klevetnik, Anvil, Unsmiling Tsarevna,
   Hafrún (two ice types, which `CardType::Ice(IceType)` cannot say), Tsakhia,
   ZATO City Grid.

**Riskiest:**
- Hush.
- Tsakhia: replaces every subroutine's resolution for an encounter.
- Matryoshka.
- Nightmare Archive: a non-agenda worth −1 in a score area.
- Hafrún.

**Banned:** Dr. Vientiane Keeling, K2CP Turbine, Matryoshka, Nanisivik
Grid, Tsakhia, World Tree.

### 5. Midnight Sun and its Booster Pack — 65 cards (C 22 / V 26 / M 17)

**Decks:** Sweep decks on its five identities.

**Stages:**
1. **Corp, composes:** Anemone, Élivágar Bifurcation, Refuge Campaign, Bladderwort,
   Artificial Cryptocrash, Ubiquitous Vig, Vasilisa, Pravdivost
   Consulting: Political Solutions, Svyatogor Excavator, Maskirovka, Extract.
2. **Runner, composes:** Revolver, Chastushka, Running Hot, Marrow,
   Avgustina Ivanovskaya, PAN-Weave, No Free Lunch, Endurance, Hyperbaric, Propeller, Environmental Testing.
3. **Charge on PH's rule, core damage words:** Captain Padma Isbister, Rigging Up,
   “Daeg, First Net-Cat”, Stoneship Chart Room, Into the Depths, Esâ Afontov: Eco-Insurrectionist, Begemot,
   Ghosttongue, The Twinning, Cezve.
4. **Advancement counters:** Vladisibirsk City Grid, Drago Ivanov,
   Mestnichestvo, Chekist Scion, Mutually Assured Destruction, Moon Pool, Azef Protocol,
   Midnight-3 Arcology, Mavirus.
5. **Ice words:** Cat's Cradle, Ivik, Wave, Bathynomus, Stavka, Hákarl 1.0,
   Trust Operation.
6. **Mark on PH's rule, then access outside a breach:** Nyusha "Sable"
   Sintashta, Carpe Diem, Backstitching, Virtuoso, Pinhole Threading, Deep
   Dive.
7. **The score area and the turn:** Backroom Machinations, Regenesis, Big
   Deal, Mitosis, Blood in the Water, Steelskin Scarring.
8. **Subroutine lists and ability layers:** Echo, Envelopment, Light the
   Fire!, Trieste Model Bioroids, Ob Superheavy Logistics.

**Riskiest:**
- Ob Superheavy Logistics: a general trash trigger, a search keyed on the
  trashed card, install-and-rez, and it chains.
- Light the Fire!
- Echo and Envelopment.
- Virtuoso: a second breach when the run ends.
- Pinhole Threading.

**Banned:** Drago Ivanov, Endurance, Nyusha "Sable" Sintashta, Svyatogor
Excavator.

### 6. Uprising and its Booster Pack — 65 cards (C 10 / V 37 / M 18)

**Decks:** Sweep decks on its three identities.

**Stages:**
1. **Composes:** Moshing, Self-modifying Code, Daily Casts, Bass CH1R180G4,
   Cerebral Overwriter, Drafter, Flower Sermon, Prāna Condenser, Bellona,
   Colossus.
2. **The turn's end, `PaysFor` and subtype words:** Mystic Maemi, Paladin
   Poemu, Hoshiko Shiro, Penumbral Toolkit, Mantle, Keiko, Cybertrooper
   Talut, Odore, DreamNet, Euler, Pauleʼs Café.
3. **Runner triggers and zones:** Swift, Aniccam, Buffer Drive, The Back,
   Prognostic Q-Loop, Simulchip, Harmony AR Therapy, Devil Charm, Bravado, Cordyceps.
4. **Corp server and advancement words:** La Costa Grid, Cayambe Grid,
   Tranquility Home Grid, Digital Rights Management, Vaporframe Fabricator,
   Wall to Wall, Kakurenbo, False Lead, Cyberdex Sandbox.
5. **Break triggers, and the first card to start a trace:** Gold Farmer,
   Makler, Týr, F2P, GameNET: Where Dreams are Real, Scapenet (`Effect::Trace` and `rules/trace.rs`,
   reached by no card until now), Transport Monopoly.
6. **Stealth credits on VP's rule, a remembered choice, set aside:** Mu
   Safecracker, Afterimage, Penrose, Boomerang, Engram Flush, Gachapon.
7. **Lockdown, then psi, then forced encounters:** SYNC Rerouting, Argus
   Crackdown, NAPD Cordon, NEXT Activation Command, Hyoubu Precog Manifold,
   Konjin, Ganked!.
8. **Points, subroutine lists and run costs that change mid-game:**
   Megaprix Qualifier, Project Vacheron, Winchester, Akhet, Earth Station.

**Riskiest:**
- Project Vacheron: a replacement on entering the Runner's score area, with
  points read off its counters.
- Konjin: an encounter nested inside an encounter (CR 6.1.3c).
- Lockdown operations: they stay in play across turns.
- Earth Station: SEA Headquarters: an additional cost to run (CR 6.3.2b) that changes when the
  identity flips.
- Stealth credits, if VP's rule has not already built them.

**Banned:** Bellona, Cayambe Grid, Cyberdex Sandbox, Engram Flush, Gold
Farmer, Hoshiko Shiro, Moshing, Project Vacheron.

### 7. Downfall — 65 cards (C 19 / V 28 / M 18)

**Decks:** Sweep decks on its four identities.

**Stages:**
1. **Runner, composes:** Isolation, Spec Work, Rezeki, Gauss, The Artist,
   Az McCaffrey, Stargate.
2. **Corp, composes:** Calvin B4L3Y, Nanoetching Matrix, CSR Campaign,
   Tiered Subscription, Red Level Clearance, Roughneck Repair Squad,
   Remastered Edition, Architect Deployment Test, Sandstone, SDS Drone
   Deployment (a `steal_cost` of `Cost::Trash`).
3. **Trigger words and bad-publicity removal:** Supercorridor, Fencer
   Fueno, Trickster Taka, Congratulations!, Demolisher, Bukhgalter,
   Storgotic Resonator, Masterwork (v37), Trebuchet, Increased Drop Rates.
4. **Amount, requirement and subtype words:** Lat, Sting!, Daily Quest,
   Fully Operational, Focus Group, Game Over, Hagen, Vulnerability Audit,
   The Nihilist, Blueberry!™ Diesel.
5. **Encounter and ice-state words:** Chisel, “Baklan” Bochkin, Pelangi,
   Afshar, Rime, Loot Box, Public Health Portal, Secure and Protect, Rejig,
   Divested Trust.
6. **Triggers created by a played card, costs to run, interrupts:** In the
   Groove, Climactic Showdown, Cold Site Server, Reduced Service, Utae (X),
   Lucky Charm (an interrupt on "end the run"), Flip Switch (a jack-out as
   an effect, and a lower trace base).
7. **Hidden information and new zones:** Hyoubu Institute, Khusyuk, The
   Class Act (a replacement on draw), Project Yagi-Uda, Letheia Nisei,
   Saisentan.
8. **Naming a card, blanking an identity, memory across runs, action
   kinds:** Whistleblower, Complete Image, Direct Access, Always Have a
   Backup Plan, MirrorMorph.

**Riskiest:**
- Always Have a Backup Plan: the last ice of one run acted on in a second
  run.
- MirrorMorph: kinds of action in the turn log, and an extra action.
- Direct Access: both identities blanked for a run.
- The Class Act: the first replacement that is not a prevention.

**Banned:** Bukhgalter, Rezeki, Sting!.

### 8. The reprint packs — 91 cards (C 35 / V 40 / M 17)

System Update 2021 (68 unbuilt), Salvaged Memories (17) and the Magnum
Opus Reprint (6). All are FFG designs, and none is in Standard.

**Decks:** Sweep decks on the eight System Update 2021 identities not yet
built.

The survey's ten stages, in order:
1. Runner, composes (two stages).
2. Corp ice and upgrades, composes.
3. Corp agendas and operations, composes.
4. Trigger words: Ken “Express” Tenma, Near-Earth Hub, Turtlebacks,
   Hostile Infrastructure, Clot.
5. Breaker and ice words: Quetzal, Abagnale, Atman, Swordsman, Hortum,
   Rielle “Kit” Peddler, NEXT Bronze, Parasite.
6. Amounts and effects.
7. The Corp's remaining vocabulary.
8. Remembered choices, triggers a card creates, and X costs: Inside Job,
   Femme Fatale, Security Testing, Chameleon, Test Run, Corporate
   Troubleshooter, Psychographics.
9. **The machinery the pool has not needed until now:**
   - Ayla “Bios” Rahim: set aside.
   - Marilyn Campaign and Daily Business Show: replacements.
   - Project Beale and SanSan City Grid: changing agenda values.
   - NEXT Silver: added subroutines.
   - Magnet: a program re-hosted and blanked.
   - Subliminal Messaging and Crowdfunding: abilities from Archives and
     the heap.
   - Slot Machine.

Most of what this tranche needs has been built by the time it arrives; the
exceptions are Magnet, and abilities that work from Archives or the heap.

**Riskiest:**
- Magnet: cuts across Trojan hosting and the continuous layer.
- Crisium Grid: "not declared successful", which VP's Flagship builds
  first.
- Subliminal Messaging and Crowdfunding: listeners outside the active
  cards.
- Parasite: a strength threshold that trashes, re-read at every checkpoint.

## How the numbers were taken

```bash
# packs, cycles, every card (v2)
curl -s https://netrunnerdb.com/api/2.0/public/packs
curl -s https://netrunnerdb.com/api/2.0/public/cycles
curl -s https://netrunnerdb.com/api/2.0/public/cards
# formats, their active pool and restriction list (v3)
curl -s https://api.netrunnerdb.com/api/v3/public/formats
curl -s 'https://api.netrunnerdb.com/api/v3/public/cards?filter%5Bcard_pool_id%5D=standard_2026_vantage_point&page%5Bsize%5D=1000'
curl -s https://api.netrunnerdb.com/api/v3/public/restrictions/standard_ban_list_26_03
curl -s https://api.netrunnerdb.com/api/v3/public/restrictions/startup_balance_update_26_03
```

"Built" is a title match against the `title` of every file under
`crates/netrunner_core/data/{corp,runner}/`, with curly and straight
apostrophes treated as the same character (the first count missed The
Maker’s Eye). A title's first printing is
read off `date_release` to split new cards from reprints. Stage 0's
`pool_status.py` replaces this with a join on `numeric_id`.
