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
5. **The clients, in the same stage.** What a card adds to the view, the
   log or a decision, a person must be able to see in both clients, and
   the stage that adds it ships it (the person's decision, 27 September
   2026, after Vantage Point's view had to be caught up in one go, #250).
   - A new field in anything a seat receives fails
     `netrunner_client::view_ledger` until it has a line there: **drawn**
     (where, in both clients), **engine's** (and why a person needs no
     picture of it), or **OWED**.
   - A new `GameEvent`, `PlayerAction`, `Lingering` or decision is worded
     where those are already exhaustive (`actions`, `board::affordance`,
     `board::hud::in_effect`, `board::Prompt`) — read the words it gets,
     not only that it compiles.
   - Draw it through a `netrunner_client` module both clients stand on,
     and hold it with a test there; a desktop change also gets a headless
     test in `crates/netrunner_desktop/tests/` (never a window opened on
     the person's screen to check it).
   - What cannot be drawn in the stage is **OWED**: a row in the [client
     ledger](#client-ledger--what-the-clients-owe) below, named in the
     stage entry. A tranche does not close with a row it added still open
     unless its entry says why it waits.
6. **Before merging:**
   - `cargo test --workspace` green and `cargo clippy --workspace
     --all-targets` silent.
   - Both sweeps at `NETRUNNER_SWEEP_SEEDS=256`, `--release`.
   - A random-vs-random `--all-matchups` report sized to at least one full
     pass of the cross product.
   - `pool_status.py`'s ratio, and its pack counts.
   If a stage's single-use `Effect` variants approach its card count, stop
   and build a composition primitive first (the DSL Growth Rule).
7. **Record** a stage entry under its tranche below: the cards; each new
   primitive and why composition failed; what the clients now show, and
   any ledger row it opened or closed; what the sweeps found; the
   fidelity limits; the ratio. A tranche closes when its list is empty.
   Its packs then join the deck builder's legal pool for their formats,
   and the `ROADMAP.md` row moves.

## Client ledger — what the clients owe

Every view field a person should see and cannot yet, and every client
change a card update needs that has not shipped. A row is added by the
stage that opens it (and marked **OWED** in
`crates/netrunner_client/src/view_ledger.rs`), and removed by the change
that draws it, which says so in its own entry. Open rows, 27 September
2026. Vantage Point's own gaps were closed by #250 and by the ledger's PR
(a run's own credits — the bad publicity fund Vantage Point's cards fill —
beside the Runner's credits); the first four rows predate it, and the
ledger found them:

| Field or need | What a person misses | Opened by |
|---|---|---|
| `CorpClientView::identity_counters` | AU Co.'s power counters on the Corp identity (spent by its own ability) | Elevation |
| `CorpClientView::recurring_credits`, `recurring_credits_max` | an identity's recurring credits left this turn (NBN: Making News) | Core Set |
| `PublicInstalledCard::seen_by_runner` | to the Corp, which of its facedown cards the Runner has already seen | Phase 5 §20 (trap play) |
| `PublicRunState::redirect_on_approach` | during a Maintenance Access run, the server it will be redirected to | Elevation |
| The terminal's card in question after a look | Méliès U's "You may trash that card" shows no card in the terminal; the "looked at" log line is above it (the desktop pop-up shows it, `Prompt::card_after`) | Vantage Point (#250) |
| A real-screen look at #250's desktop pieces | the identity chip, the In effect lines and the removed-from-game rows are held by headless tests only | Vantage Point (#250) |

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
- **Méliès U's secret, three-sided identity** (CR 1.5.2b). Built in VP
  Stage 8: the flip stays one bit, and which copy is in play is a second
  number, hidden from the Runner until the back side shows it.
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
   **4b** (built, below): Lionsmane, Event Horizon, Ansel 2.0.
5. **Runner trigger and payment words.** **5a** (built, below):
   Methuselah, Touchstone, Caveat Emptor, and Shackleton Grid, moved here
   from Stage 4: "when the Runner spends credits from outside their
   credit pool" is a moment the payment sites have to dispatch. **5b**
   (built, below): Stowaway (a Trojan's "this server" is its host's),
   Nurse Hạnh (Archives' facedown cards turned faceup, two or more).
   **5c** (built, below): Beta Build (a run-end rider about the program it
   installed). **5d** (built, below): Hiram (a trash the Runner carries
   out, from any location, is a moment, CR 1.14.5a; "look at the top card
   of R&D" shows one player a hidden card). Stage 5 is complete.
6. **Arrange, reveal, stealth credits.** **6a** (built, below): Cultivate,
   Knowledge Seeker, Esca — arranging the top of R&D, a reveal while
   accessed in R&D, an encounter's end as a moment. **6b:** Corsair, Baker,
   Lampades — credits spendable only from stealth cards (built, below).
   **6c** (built, below): Aircheck,
   which locks the credit pool and so breaks the invariant that a payment
   may always reach the pool. Stage 6 is complete.
7. **Hosting, the score area and access limits**, split by mechanic.
   **7a** (built, below): Luana Campos, a card leaving the table as a
   moment it can interrupt. **7b** (built, below): Read-Write Share,
   cards hosted facedown. **7c** (built, below): Hackerspace, a resource
   installed onto a resource. **7d** (built, below): Myōshu, Word on the
   Street and Sacrifice Zone Expansion, the score area. **7e** (built,
   below): Stick and Poke, a subroutine gained for an encounter. **7f**
   (built, below): Tocsin, an ability used from HQ. **7g** (built,
   below): Flagship, a run that cannot be declared successful and a limit
   on accesses. Stage 7 is complete.
8. **Méliès U, alone:** hidden setup state, three sides, and masking
   (built, below). **Vantage Point is complete, 66 of 66.**

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
