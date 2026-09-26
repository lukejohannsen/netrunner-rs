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
  In progress. Its record is below.
- **Stage 0b — formats from NetrunnerDB, and a deck's legality per
  format.** Next. Its specification is below.

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
  floor that a missing pack could have passed.
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

### Stage 0b — formats, and a deck's legality per format (next)

**The person's decision (26 September 2026):**
- A card can be banned in one format and legal in others.
- A deck is legal or illegal *per format*, and the builder keeps track of
  which formats each deck is legal in.
- A deck holding a banned card is not refused. It is illegal in that
  format and can still be played casually. If that needs a Casual format
  (no pool, no list), add one.

So the thirteen shipped decks that Startup's current list catches (Party
Hard, Planning Ahead, both Catalyst decks, Agency, Brutal Efficiency,
Fashion Lab, Quick Returns, Discretion Advised, Hyper Velocity, Fine Print,
both Syndicate decks) are recorded as not Startup-legal, and stay playable.

1. **Formats come from NetrunnerDB's v3 API** (`formats`, `card_pools`,
   `restrictions`), by a script in the shape of `catalog_sync.py` that
   writes a committed data file `build.rs` embeds. `FormatRules` in
   `format.rs` is filled from it: Standard's cycles, Startup's packs, each
   ban list, and rotation by cycle, which §5 dropped when the pool was two
   packs. `no_shipped_format_restricts_anything_yet` is retired on purpose.
2. **A Casual format**: every pack, no list. It is what a bot game or a
   sample deck plays when the deck is legal nowhere else, and the reason a
   ban never refuses a deck.
3. **A deck's legality is the set of formats it is legal in.** It is
   computed, never stored, from both validators. The builder already shows
   `legal_in` (`deck_builder::status`); starting a game refuses only a deck
   the engine cannot run, or one illegal in the format the table was set to
   (Casual admits everything the engine can run). The server's lobbies are
   per format (Phase 4 §7) and hold their format's rules.
   `every_shipped_format_can_actually_serve_the_sample_pool` becomes
   "every shipped deck is legal in Casual, and the formats it is legal in
   are pinned".

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

Both land as **Vantage Point Stage 1**, ahead of any card that needs them.
They are not in Stage 0, because Stage 0 adds no rules surface.

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
1. **Groundwork: events heard and subtypes listed**, above, with the
   no-new-word cards: Virtual Intelligence P.I., Sell Out, Borrowed Goods,
   Rotary, Méliès City Luxury Line (the first `steal_cost`), Sleipnir,
   Retirement Plan, Nihilo Agent, Grubber, Paywall, Scapegoat, Flywheel,
   Vulture Fund.
2. **Bad publicity, tags and costs:** Editorial Division, Witch Hunt, Take
   a Dive, Kompromat, Vicsek, Reanimation Protocol, Unleash,
   Synchrocyclotron, realloc(), Flood the Market.
3. **Turn-log counts and standing kinds** (`PlayCost`, `AgendaPoints` and
   `StealCost` are the `ContinuousKind` words, each a new kind held to the
   DSL Growth Rule): Chain Reaction, Underdome Irregulars, Reverb, Hype
   Machine, Tailgate, Perfect Recall, The Red Room, Lotus Haze, Magistrate
   Revontulet, Let Them Dream.
4. **Ice and run triggers:** Vertigo, Sipa, Lethe, ezaM, The Tungsten
   Tailor, Lionsmane, Event Horizon, Shackleton Grid, Ansel 2.0.
5. **Runner trigger and payment words** (`PaysFor` during a run; the
   Runner's allotted clicks): Hiram, Touchstone, Methuselah, Beta Build,
   Stowaway, Nurse Hạnh, Caveat Emptor.
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
