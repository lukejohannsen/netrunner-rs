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
  FFG plan ([ffg-card-pool.md](ffg-card-pool.md)).
- **The Core Set's remainder rides with the reprint packs** (6 October
  2026, the person's). The 2012 Core Set is the one FFG set embedded in
  the catalog from the start, and 68 of its cards are neither built nor
  reprinted in a reprint pack, so no tranche reached them. Tranche 8 takes
  them, which leaves the embedded catalog complete before the FFG sets are
  brought in.
- **The observation vocabulary grows once, up front** (Stage 0), with a
  rank reserved per pack. The alternative was one reshape and one retrain
  per set.

## Progress

Where every pack stands, from `scripts/pool_status.py` (quote it, never a hand
count). A stage's last step updates this table and moves its entry to
[the archive](archive/nsg-card-pool.md); a tranche's own list below names the
cards each stage takes.

| # | Pack | Printed | Built | `UNIMPLEMENTED` | Standing |
|---|---|---|---|---|---|
| 1 | Vantage Point (`vp`) | 66 | 66 | 0 | complete (27 September 2026) |
| 2 | Rebellion Without Rehearsal (`rwr`) | 65 | 65 | 0 | complete (28 September 2026) |
| 3 | The Automata Initiative (`tai`) | 65 | 65 | 0 | complete (29 September 2026) |
| 4 | Parhelion (`ph`) | 63 | 63 | 0 | complete (2 October 2026) |
| 5 | Midnight Sun (`ms`) + Booster (`msbp`) | 65 + 7 | 65 + 7 | 0 | complete (3 October 2026) |
| 6 | Uprising (`ur`) + Booster (`urbp`) | 65 + 7 | 65 + 7 | 0 | complete (5 October 2026) |
| 7 | Downfall (`df`) | 65 | 65 | 0 | complete (6 October 2026) |
| 8 | System Update 2021 (`su21`), Salvaged Memories (`sm`), Magnum Opus Reprint (`mor`), Core Set (`core`) | 82, 18, 6, 113 | 11, 1, 0, 27 | 71, 17, 6, — | not started (the built ones are Core reprints; `core` has no gate yet) |

**DSL ratio** (`pool_status.py`, the DSL Growth Rule's number): **13 of 105
`Effect` variants single-use, none unused, over 638 card files** (6 October
2026, with Downfall Stage 8, which closed the set and added two:
`ChooseCardName`, which Complete Image and Whistleblower share, and
`StealAccessedCard`, Whistleblower's alone; Stage 7, at 12 of 103 over
633, added no variant and gave
`MoveRunToOutermost` its second card, Letheia Nisei; Stage 6, at 13 of 103
over 627, added `ForEach` and generalised
`WhenThisTurnEnds` into `LaterThisTurn`, now three cards'; Stage 5, at 14
of 102 over 618, added no variant and gave `GainIceSubtype` its second
card, Pelangi; Stage 4, at 15 of 102 over 609 —
written here as 101 at its close, which `Repeat` had made 102 — added
`Repeat` and gave `RevealHand` its second card; Stage 3, at 15 of 101 over 600, added no variant and five
words beside it; Stage 2, at 15 of 101 over 590, added none and changed no engine
code; Stage 1, at 15 of 101 over 580, added none; Uprising Stage 8, at
15 of 101 over 573, added none and closed the set;
Stage 7b, at 16 of 101 over 568, added none and gave Sisyphus
Protocol's `ForceEncounter` two more cards, Konjin and Ganked!; Stage 7a,
at 17 of 101 over 566, added none and gave `ChooseServer` its second card,
Hyoubu Precog Manifold; Stage 6, at 18 of 101 over 561,
added `RevealHand`, Engram Flush's alone,
and `Remember`, which Boomerang and Engram Flush share; Stage 5 added no
variant and gave `Trace`, the last unused one, its first card, Scapenet, at
17 of 99 over 555; Stage 4 added
`TurnArchivesFacedown` and gave `AddToHand` its second card, at 16 of 99, 1
unused, over 548; Stage 3 added none, at 16 of 98 over 539;
Stage 2 took Madani's
single-use `InstallRunnerCardFromHost` into the zone install, at 16 of 98
over 529; Stage 1 added none, at 17
of 99 over 518; Midnight Sun closed at
17 of 99 over 508, measured on `main` at 95a6097 — the 18 recorded here at
its Stage 7b was not re-measured at its close). The baseline at Stage 0a was 26
of 70 single-use, 3 unused, over 184 files: 334 cards later the single-use count is *lower*, because the
growth went into `Trigger`, `EventFilter`, `Amount`, `CardFilter`, `Cost` and
`ContinuousKind` words rather than into what an effect does.

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
| 8 | NSG reprint packs and the Core Set's remainder | `su21` 31001–31082, `sm` 29001–29018, `mor` 28001–28006, `core` 01001–01113 | 219 | 91 + 68 | 47 / 44 + 39 / 29 | 8 + 0 | — (not in Standard) |

**454 unbuilt cards for Standard and 91 for the reprints.** Standard 2026
(`standard_2026_vantage_point`, 613 cards) is System Gateway, Ashes
(Downfall, Uprising and its booster), Borealis (Midnight Sun and its
booster, Parhelion), Liberation (The Automata Initiative, Rebellion Without
Rehearsal), Elevation and Vantage Point. Midnight Sun and Uprising each
reprint their own booster pack's 7 cards, so each tranche counts 7 fewer
than it prints. In the reprint packs, 11 System Update 2021 titles (The
Maker’s Eye and two identities among them) and Salvaged Memories'
Scorched Earth are already built. The Core Set's 68 are the cards it
alone prints and nobody built: the 18 it shares with a reprint pack are
counted there, and its 27 built ones include all seven identities.

A card on a ban list is still built, because Eternal allows it. Within its
set it goes last.

## Stage 0 — groundwork, once, before tranche 1

No cards. Split in two when it was taken (26 September 2026), because the
formats half reaches into both clients and the rest does not:

- **Stage 0a — the catalog, the gates, the vocabulary, the status script.**
- **Stage 0b — formats from NetrunnerDB, and a deck's legality per
  format.**

Both are closed; see [Progress](#progress) for what is next.


**Closed** — the record is in [the archive](archive/nsg-card-pool.md) under the same headings:

- **Stage 0a** — the catalog from NetrunnerDB by script (`scripts/catalog_sync.py`), fifteen packs embedded, a completeness gate per pack, `CARD_VOCAB` grown once to 1024 with a rank reserved per pack, and `scripts/pool_status.py` (`feat/nsg-pool-stage-0a`, 26 September 2026).
- **Stage 0b** — formats from NetrunnerDB (Standard, Startup, Eternal, Casual), a card banned in one and legal in another, a deck's legality judged per format on its printings, in both clients (`feat/nsg-pool-stage-0b`, 26 September 2026).
- **Stage 0c** — the sixteen card files whose ids were not NetrunnerDB v3's card ids are renamed to them: fourteen identities now carry their whole title, plus M.I.C. and Maglectric Rapid (`fix/v3-card-ids`, 30 September 2026).
- **Stage 0d** — the catalog is NetrunnerDB v3's: cards by id, printings by code beside them (`cards::catalog`), sets with release dates, formats and the deckbuilding validator by card, `numeric_id` became `built_from`, and the v2 live sync deleted (`feat/v3-catalog`, 30 September 2026).


## The recipe — every stage of every tranche

1. **Read the rule before the card.** For every mechanic a stage adds,
   `grep -n` its section in `rules/comprehensive-rules.md`, and add or
   update its row in [rules-conformance.md](rules-conformance.md), and
   the mechanic's `Built in` cell in the table below. A
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
4. **Each card file** takes its card's NetrunnerDB v3 id as its `id` and
   carries its `built_from` — the NSG printing's code, which is what places
   it in its pack's vocabulary block (Stage 0a; named `numeric_id` until
   Stage 0d) — and every choice and
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
     test in `crates/netrunner_desktop/tests/`, and a screenshot is taken
     on a virtual compositor ([below](#client-ledger--what-the-clients-owe)),
     never in a window on the person's screen.
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
   fidelity limits (each also a line under [Known limits, by
   card](#known-limits-by-card)); the ratio. Then update the
   [Progress](#progress) row, mark the stage in its tranche's list, and
   **move the entry to [the archive](archive/nsg-card-pool.md)** under the
   tranche's heading, leaving one line in the tranche's closed list. A
   tranche closes when its list is empty.
   Its packs then join the deck builder's legal pool for their formats,
   and the `ROADMAP.md` row moves.

## Client ledger — what the clients owe

Every view field a person should see and cannot yet, and every client
change a card update needs that has not shipped. A row is added by the
stage that opens it (and marked **OWED** in
`crates/netrunner_client/src/view_ledger.rs`), and removed by the change
that draws it, which says so in its own entry.

**No open rows** (27 September 2026). The ledger's first pass found four
fields older than Vantage Point drawn nowhere, and #250 left two; all six
were cleared before Rebellion Without Rehearsal:

- AU Co.'s power counters and Making News' recurring credits
  (`identity_counters`, `recurring_credits`): `hud::identity_facts` on the
  identity's sheet and the terminal's identity line, and
  `hud::identity_chip` on the avatar's foot ("3 power counters", "1 of 2")
  for an identity that does not flip.
- To the Corp, which face-down cards the Runner has already seen
  (`seen_by_runner`): "seen" on the tile, a line on its sheet, "seen by
  the Runner" on the terminal's server line (`facts::seen_face_down`),
  never to the Runner, who knows what they saw.
- Maintenance Access's destination (`redirect_on_approach`): the first
  In effect line during its run (`hud::in_effect`).
- The terminal's card after a look: `Prompt::looked_at`, drawn over the
  board as "You looked at …" from the last masked entry each terminal
  surface now keeps (`RenderableView::last_entry`).
- A real-screen look at the desktop: taken on a virtual compositor, off
  the person's screen (below), with Méliès U's chip on the avatar's foot.

**Screenshots never open a window on the person's screen.** Run the
desktop's dev hooks on KDE's virtual compositor, with scratch settings so
the person's own are not read or written:

```bash
env -u DISPLAY kwin_wayland --virtual --socket netrunner-shot --width 2560 --height 1600 &
env -u DISPLAY WAYLAND_DISPLAY=netrunner-shot XDG_DATA_HOME=<scratch>/data XDG_CONFIG_HOME=<scratch>/config \
  NETRUNNER_GAME=corp NETRUNNER_AUTOPLAY=40 NETRUNNER_WINDOW=2560x1600 NETRUNNER_SCREENSHOT=<scratch>/shot.png \
  cargo run -q -p netrunner_desktop
```

A deck the default format refuses (a Sweep deck, Eternal-only) leaves the
board at "No game in progress"; copy it into a scratch
`NETRUNNER_DECKS_DIR` as a saved deck, and the scratch settings' default
format (Casual) plays it.

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
building. **`Built in` is the stage that built it** — the card↔rule map
this file keeps; a stage that builds or widens a mechanic updates the cell.

| Mechanic | First needed | Also in | Rule | Built in |
|---|---|---|---|---|
| Credits spendable only from stealth cards (a restriction on the *payer*, the reverse of `PaysFor`) | VP: Corsair, Lampades, Baker | UR: Mu Safecracker, Afterimage, Penrose | CR 1.10.4b | VP 6b; UR 6 (on a card's own "you may pay", which is using it: CR 9.1.6) |
| Additional subroutines, ordered | VP: Stick and Poke | RWR Thunderbolt Armaments; TAI Starlit Knight; MS Echo, Envelopment; UR Winchester | CR 9.8.2, CR 9.8.3, CR 6.5.7d | VP 7e; RWR 8b (for the run); TAI 8a (by count); MS 8a (the ice's own, counted, 9.8.3b and d: `ContinuousKind::Subroutines`; and 6.5.7c, none at all) |
| Arrange | VP: Cultivate, Knowledge Seeker | RWR Cataloguer; TAI Federal Fundraising | CR 8.3.1 | VP 6a |
| Reveal as a step other cards can read | VP: Esca, Perfect Recall, Tocsin | RWR Burner, Bring Them Home | CR 1.21.3 | VP 6a; RWR 6b (stays revealed) |
| Abilities active outside play (Expendable from HQ; from Archives or the heap) | VP: Tocsin | RWR Eminent Domain, Descent; TAI Slash and Burn Agriculture, Tree Line, Angelique; reprints Subliminal Messaging, Crowdfunding | CR 9.1.8b | VP 7f; RWR 7a; TAI 7 |
| A card added to a score area "as an agenda" | VP: Word on the Street, Myōshu | RWR Jeitinho, Kingmaking; PH Nightmare Archive; MS Regenesis, Backroom Machinations | CR 1.17.3f | VP 7d; RWR 8c (the Runner's); PH 7 (an accessed Corp card, into the Runner's) |
| Hosting in general: install onto a card, facedown hosted cards, host limits | VP: Hackerspace, Read-Write Share, Luana Campos | RWR Spree; PH Matryoshka | CR 1.13.5, CR 1.13.7 | VP 7b, 7c; RWR 3d (onto a host); TAI 4; PH 7 (turned facedown) |
| Runner cards removed from the game | VP: Take a Dive, Kompromat | TAI Capybara; PH Nanuq; UR Devil Charm, The Back, Buffer Drive | CR 4.9 | RWR 2b (`RemoveFromGame`); TAI 7 |
| Additional costs imposed by another card (steal, score, run, trash) | VP: Magistrate Revontulet | RWR Sebastião Souza Pessoa; TAI Daniela Jorge Inácio; MS Azef Protocol; UR NAPD Cordon, Earth Station: SEA Headquarters; DF Cold Site Server, Reduced Service | CR 1.16.10, CR 6.3.2b | VP 3a (steal, score); RWR 2a (trash); TAI 8b; MS 4 (an agenda's own, to score it) |
| Terminal: the action phase is forced to end | RWR: Active Policing, Bring Them Home | TAI Oppo Research; MS Big Deal | CR 5.4.3 | RWR 6a |
| Psi game: a simultaneous secret bid | RWR: See How They Run | TAI Adrian Seis; UR Konjin, Hyoubu Precog Manifold | CR 10.14.6 | RWR 7b; TAI 8b |
| Set aside | RWR: The Wizard’s Chest | PH Spark of Inspiration; MS Deep Dive; UR Gachapon | CR 4.8 | RWR 6d (faceup only); PH 6c (Spark composes); UR 6 (the rest removed from the game: `CardTarget::SetAside`) |
| X costs | RWR: Lobisomem | PH Matryoshka; DF Utae; reprints Corporate Troubleshooter, Psychographics | CR 1.16.2c | RWR 6e; PH 7 |
| Forced or repeated encounter | RWR: Sisyphus Protocol | UR Konjin, Ganked! | CR 6.1.3 | RWR 7d; UR 7b (away from the Runner's position, from an encounter or an access: `RunState::suspended`) |
| Losing abilities | PH: Hush, Klevetnik | MS Light the Fire! | CR 9.1.9a | PH 8 (`ContinuousKind::LosesAbilities`, `Lingering::LosesAbilities`; one question, `active::lost_abilities`); MS 8b (the root of the attacked server, read when asked: `lingering::On::RootOfAttackedServer`) |
| Break restrictions ("cannot be broken", "only by …") | PH: Anvil, Unsmiling Tsarevna, Hafrún | MS Trieste Model Bioroids; UR Akhet, NEXT Activation Command | CR 9.8.5 | RWR 5b (Hammer's, standing); PH 8 (for a duration, `Lingering::BreakLimit`; one Runner card's, `Prohibition::BreakSubroutines`); MS 8b (one piece of ice, against Runner cards only: `Prohibition::BreakSubroutinesOnIce`); UR 7a (no card but an icebreaker breaks, standing: `Prohibition::BreakWithNonIcebreakers`) |
| Charge | PH: Flux Capacitor, Orca | MS Captain Padma Isbister, Rigging Up, “Daeg, First Net-Cat”, Stoneship Chart Room | CR 10.10 | PH 6a (composes: a selection of `HostsCounters(Power)`) |
| Mark | PH: Tunnel Vision, Info Bounty | MS Nyusha "Sable" Sintashta, Carpe Diem, Virtuoso, Backstitching | CR 10.11 | PH 6b (`Lingering::Mark`, `Effect::IdentifyMark`) |
| An agenda's points or advancement requirement changing | VP: Let Them Dream | PH Ontological Dependence, Freedom of Information, Regulatory Capture; UR Megaprix Qualifier, Project Vacheron; reprints Project Beale, SanSan City Grid | CR 3.2.2, CR 3.2.3b | VP 3a (points); PH 3a (requirement, a card's own) |
| A choice remembered for a duration (a server, an ice, a subtype, a card's name) | RWR: Lycian Multi-Munition | MS Trieste Model Bioroids; UR Boomerang, Engram Flush, Hyoubu Precog Manifold; DF Whistleblower, Complete Image, Saisentan; reprints Femme Fatale, Security Testing, Chameleon | CR 9.10.3 | RWR 8a; PH 8 (a server, for the turn: Tsakhia's `Effect::ChooseServer`); MS 8b (a piece of ice, while the chooser is rezzed: `EffectDuration::WhileRezzed`); UR 6 (a card while the chooser is installed, a card type for the encounter: `Effect::Remember`); UR 7a (a server while a lockdown is in play: `Until::WhileInPlay`) |
| A triggered ability created by a card that resolved ("when your next run ends…") | DF: In the Groove, Climactic Showdown, Always Have a Backup Plan | reprints Inside Job, Test Run | CR 9.10 | RWR 5d in part (a delayed conditional ability); DF's cards — |
| Lockdown | UR: SYNC Rerouting, Argus Crackdown, NAPD Cordon, NEXT Activation Command, Hyoubu Precog Manifold | — | CR 3.5.1c | UR 7a (`CorpState::play_area`, active there; trashed as the Corp's next turn begins; "no active lockdown" is `ZoneHasAtLeast` over `CardZoneRef::PlayArea`) |

Two more mechanics, each first needed by a single card:
- **Méliès U's secret, three-sided identity** (CR 1.5.2b). Built in VP
  Stage 8: the flip stays one bit, and which copy is in play is a second
  number, hidden from the Runner until the back side shows it.
- **A run that "cannot be declared successful"** (Flagship, VP; CR 6.8.4a),
  which every consumer of a successful run has to hear. Crisium Grid in the
  reprints reuses it.

## Known limits, by card

Every deliberate shortfall between a printed card and what the engine does,
one line each, gathered here from the stage entries' "Fidelity limits", from
Phase 1 §8's Elevation record and from the System Gateway and Core Set
fidelity audits (`docs/archive/`). A limit is carried until a stage removes
it; a stage that removes one strikes its line. **Do not "fix" one without a
reason**: each was chosen, and several are the rules' own reading. A limit
that a later rule already closed is not here — its removal is noted in the
PR that made this list (29 September 2026).

### Owed — a card whose printed text is not yet built

- ~~**Blood in the Water** (Midnight Sun): prints its advancement requirement as X, which NetrunnerDB records as none; the face draws no circle for it until the stage that builds a variable requirement.~~ Built in MS Stage 7a: a printed 0 its own text makes the size of the grip, printed X by both clients (`card_face::advancement_slot`).
- **Nanisivik Grid** (PH 5b): a subroutine it resolves reads "this server" as unresolved, where CR 4.6.6i's example makes it Archives; no subroutine in the pool says "this server".
- **Docklands Pass, Rotary**: "whenever you breach HQ or R&D" is a successful run on either; a breach without a run (Cataloguer, RWR 6c) does not fire them.
- **Manuel Lattes de Moura**: the extra access is heard at the run's success, as every "when you breach" in the pool is, so a run Flagship keeps from being declared successful gets none.
- **Kessleroid**: "cannot trash" is unenforced.
- **Environmental Testing** (MS 2): "when there are 4 or more hosted power counters" is asked as its own install trigger places a counter; a charge (Orca, Flux Capacitor) that brings it to 4 is not heard until its next program or hardware install, because no trigger hears counters placed.
- **Embedded Reporting**: does not shuffle. **Next Big Thing** from the Runner's score area is unmodelled. **Detente**'s access outside a run is unmodelled.
- **Luana Campos** (VP 7a): "uninstalled" is announced for a Corp card only, and no Runner card in the pool interrupts its own uninstalling. The interrupt resolves in the dispatch that announced it.
- **Nanuq** (PH 6d): "when this program is uninstalled" is heard as its trash from the table — the only way a program leaves the rig in the pool but by its own removal — so an uninstall to the grip or the stack would not remove it.
- **Shred**: intercepts `EndTheRun` only (a lingering `PreventRunEnding`).
- **Raindrops Cut Stone** (PH 6c): a subroutine a card's text resolves (Nanisivik Grid's) is not announced, so it is not counted; only an encountered piece of ice's are.
- **Tsakhia "Bankhar" Gantulga** (PH 8): the chosen server lasts the Runner's turn it was chosen in, so a run on the Corp's turn after it finds none. Its replacement is made as the encounter begins, by a trigger the Runner orders with the encounter's other abilities, and outlives Tsakhia leaving the table mid-encounter. A subroutine a card's text resolves off the encounter (ZATO City Grid's, Mycoweb's) is not replaced.
- **Cat's Cradle, Ivik** (MS 5): "code gate ice" is a code gate as printed (`CardFilter::IceOfType`, a definition word), so ice that gained the subtype for a run is neither taxed by Cat's Cradle nor counted by Ivik; no card in the pool gives ice a subtype before it is rezzed, and Ivik counts rezzed ice outside any run.
- **Carpe Diem with Virtuoso or Nyusha "Sable" Sintashta** (MS 6a): a first successful run on the mark is counted over the whole turn's runs on that server (`turn_log::with_mark`), so a mark Carpe Diem identifies mid-turn on a server already run successfully that turn gives neither card its first time, where CR 10.11.5's example says it does. Every other mark card in the pool identifies as the turn begins.
- **Deep Dive** (MS 6b): its accesses stand in a run-less `RunState` on R&D, as Cataloguer's breach does, so the access names R&D as its server (`GameEvent::CardAccessed`, `AccessingIn(RnD)`) and both clients' run displays say "Breach of R&D"; the set-aside cards are in no server. No card in the pool reads the server of a set-aside card's access.
- **Hush, Klevetnik** (PH 8): CR 9.12.1d's order of dependent effects is not built; a card's loss is read off the cards hosted on it and the lingering list directly, which is the order 9.12.1e gives hosted objects and all the pool needs.

- **Prāna Condenser** (UR 1): "whenever **you** would do net damage" hears every net damage about to be suffered, because `EventFilter::Damage` reads the kind and not who does it; a Runner card that does net damage to its own Runner would be offered to the Corp to prevent.
- **Earth Station: SEA Headquarters** (UR 8): its additional cost to run is paid as the server is announced, before the run's own pools exist, so neither bad publicity's credits (rightly, CR 6.3.3) nor a run event's (Overclock's, placed as its run begins here) nor a card's "during runs" credits pay it. GameNET's "causes the Runner to spend" does not hear it, as it hears no standing effect's price.
- **Stargate** (DF 1): "reveal the top 3 cards of R&D" shows them to the Runner as the selection's candidates, and the Corp is shown only the card trashed (`CardsSelected { revealed }`, faceup in Archives); the two left on R&D are not announced to the Corp. No card in the pool reads a reveal of R&D.
- **Loot Box** (DF 5): "Reveal the top 3 cards of the stack" shows them to the Corp as the selection's candidates, and the Runner is shown only the card added to the grip (`CardsSelected { revealed }`), as Stargate's reveal shows the Corp only the card trashed.
- **Pelangi** (DF 5): "Choose an ice subtype" offers barrier, code gate and sentry, the three a breaker reads (`IceType`); a subtype no breaker reads (trap, AP, bioroid) is not offered.
- **“Baklan” Bochkin** (DF 5): "X hosted power counters" is every counter on it — the card is trashed with them, so a smaller X could only derez less.
- **Reduced Service** (DF 6): "you may pay up to 4[credit]" loses the credits from the credit pool rather than paying them, as Focus Group's X does.
- **In the Groove** (DF 6): "Play only as your first [click]" is the turn's first action (`NoActionTakenThisTurn`, Petty Cash's), which every first action in the pool spends a click on.
- **Complete Image** (DF 8): the names offered are every playable Runner card, not the format's pool; and "repeat this process" reads only damage dealt in the same resolution, so damage an interrupt held for a later action ends the process.
- **Whistleblower** (DF 8): the names offered are the playable agendas; and the steal is made as the agenda's access begins, before any other "when accessed" ability the agenda has would be asked about.
- **Direct Access** (DF 8): the identities lose their abilities from the moment the run begins until it ends, not from the play of the event; and "this event" is a copy of it in the heap, which is where the event is by then.
- **Always Have a Backup Plan** (DF 8): the second run is offered as a choice of one server; and the ice the first run encountered last is the engine's alone (`RunState::last_encountered`), so a bot's sample of the second run does not bypass it.
- **MirrorMorph: Endless Iteration** (DF 8): "take another different action, paying [click] less" is a click gained and the next action refused if it is one already taken this turn, which lets a fourth click's action come before the extra one where the rules have the extra one first; nothing in the pool can tell the two apart. With no different action left to take, the Corp may end its turn with the click unspent, the extra action forgone.
- **Hyoubu Institute** (DF 7): the top card of the stack is revealed by a one-card selection the Corp confirms, which shows it to the Corp a moment before the Runner.
- **Khusyuk** (DF 7): the install cost is chosen from 1 to 10, Orca's printed 10 being the highest of a Runner card in the pool.
- **The Class Act** (DF 7): drawn cards go straight to the grip once the interrupt has resolved, never into a set-aside zone first (CR 8.4.2, 8.4.5); "when a discard phase ends" hears the Runner's own, so a copy installed during the Corp's turn draws nothing at the Corp's; and a draw made while something else is parked for prevention is not announced (the Prevention Rule's one-slot rule), so it is not "the first time".
- **Project Yagi-Uda** (DF 7): "for each hosted advancement counter past 3" is Dividends, counted past the requirement as the score began, which is 3 unless a card has changed it; and the card swapped in is not heard as installed (CR 8.8.4b's install trigger conditions), as Mitra Aman's and Tatu-Bola's are not.
- **Letheia Nisei** (DF 7): "the first time … during each run" is a use limit (`OncePerRun`), spent when it resolves; the Runner approaches the server again only once Letheia has trashed itself, so no card in the pool can tell the two apart.
- **Saisentan** (DF 7): the extra net damage reads what the subroutine's own damage trashed in the same resolution (`LastDamageTrashed`), so damage parked for an interrupt and dealt on a later action is not read for its type, as Diviner's is not.
- **Flip Switch** (DF 6): "[trash]: Jack out" pays the jack-out as part of the cost (`Cost::JackOut`, which Lionsmane offers), so it is made with the trash rather than after it; nothing can come between the two.
- **Hoshiko Shiro: Untold Protagonist** (UR 2): the catalog folds the flip side's text into the front's and keeps none of its other numbers, so a flipped Hoshiko has the front's subtypes (Natural) and link (0). DreamNet's "if your identity is digital" reads her front either way (`EffectRequirement::IdentityMatches`).

### Recorded deviations from the Comprehensive Rules

Each is also a note on its section's row in [rules-conformance.md](rules-conformance.md).

- **A use limit is spent when a trigger fires, whether its "may" is taken or not** (CR 9.3.6g says a declined ability is not used): Zahya Sadeghi, Shackleton Grid, Malandragem, Heliamphora, Brasília Government Grid, Afterimage. `TriggeredEffect::requirement` is consumed at fire; the 9.3 row carries it.
- **"The Corp trashes" is read as the controller's trash** (CR 1.14): Noise, Heliamphora.
- **A terminal card's end does nothing mid-run or with a window open** (CR 5.4.3b): Active Policing, Bring Them Home; no terminal in the pool resolves anywhere but its player's action window. Nuvem SA's "finish resolving" is announced after the action phase has ended, not while it is ending.
- **Arranged cards are not made new objects** (CR 8.3.3): Cultivate, Knowledge Seeker — nothing in the engine remembers a card in R&D by identity. The client does not yet word an arrangement as "top first".
- **Hosted cards are a list in the order hosted**, not "distinct groups … freely arranged" (CR 1.13.7d): Read-Write Share.
- **Baker's credit is paid in the last paid-ability window before the approach** (CR 6.9.4e), not at it, so the Corp acts in that window after the Runner has chosen.
- **A breach with no run that ends the game mid-access** still records a `RunCompleted`, pushed and not dispatched (Cataloguer); the breach is shown with the run's panel and trail.
- **A run a card's text makes is refused when its additional cost cannot be paid** (CR 1.16.10a says the run simply does not happen), and a Runner who can pay is never offered to decline (Earth Station: SEA Headquarters, UR 8). A run event that cannot pay is not offered; a "you may run" keeps its other option. The 6.3 row carries it.

### Approximations whose outcome is the printed one

- **Two of N as two choices of one**, each resolved before the next (CR 9.12.2c names realloc()): realloc(), Chain Reaction's "trash 2", Logjam's counters (heard as two placements), Shipment from Vladisibirsk's four picks. Editorial Division shuffles R&D when its search is declined; its "total" discount is never asked — the Corp is given the division that costs least.
- **A zone is chosen before the card**: Sleipnir ("from HQ or Archives"), Let Them Dream ("HQ, R&D or Archives"), Muse (one zone's programs at a time, where the rules search all three; a hosted program takes memory as any does), Lethe (top or bottom before the card).
- **"Up to N" takes N where fewer is never better**: Ansel 2.0 and Boomerang break two when two are left; Orca's "break any number of sentry subroutines" breaks every one pending (PH 6a); Shackleton Grid's "may" always fires (declining 4 meat damage is never better).
- **Order of two abilities from one card is the printed order, not the player's**: Privileged Access's two "when you take a tag" abilities; Malandragem's two offers. Privileged Access's installs are conditioned on being tagged *after* the tag, so a Decoy preventing the event's tag on a Runner tagged earlier in the run still lets them happen.
- **Heliamphora's "instead" is an access, then a host**: the card's own "when accessed" in Archives still resolves and it counts as accessed. A true interrupt would park the access before it began.
- **Alarm Clock's run** is asked for among the Runner's other "when your turn begins" abilities and resolved when chosen; the Corp's rez window of 5.7.1e comes once, after the run.
- **Hearts and Minds** chooses its source before its destination, so an advanceable card may be picked as both — a move onto itself.
- **Pressure Spike's pumps last the encounter**, as every breaker's does here. **Corporate Hospitality** excludes only the copy resolving, found as the last faceup copy in Archives.
- **Raindrops Cut Stone's counters** (PH 6c) are placed as a subroutine is announced, before its effect resolves, so one that ends the run is counted while the run, and the event's place in the play area, still stand — the outcome the card's "(including a subroutine that ends the run)" prints.
- **ZATO City Grid** (PH 8): the subroutine is chosen after the ice is trashed, not before; the Corp makes both choices knowing everything, so the outcome is the printed one. "1 subroutine on it" offers what the ice prints.
- **Klevetnik, Unsmiling Tsarevna** (PH 8): "you may have the Runner gain 2[credit]. If you do, …" is a choice whose first option gives the credits — a nested cost (CR 1.16.11a) no `Cost` word says, with the printed outcome.
- **Concerto's credits** (PH 5d) are placed on the run as it starts, where the card places them on itself before it; they pay for anything during that run and go with it.
- **Aircheck's event is active for the run it makes and no longer**; the second run is ordinary, and the event's unspent credits are gone with the first.
- **Vera Ivanovna Shuyskaya's reveal** is not a `CardRevealed` of each grip card; the Corp sees the grip through the selection and only the trashed card is announced. No card in the pool reads a reveal of the grip.
- **Business As Usual's virus counters are the Runner's cards'**: no Corp card in the pool hosts one.
- **Tocsin** is the pool's only expendable whose ability is an action, so it is the one announced; an expendable ability that is not an action announces nothing.
- **Nihilo Agent's "load … when it is empty, trash it"** is three counters and a self-trash after its own removal; nothing else in the pool removes its counters.
- **Into the Depths' search** (MS 3) offers only a program that could be installed, as Privileged Access's heap does; the printed search may find one it then cannot install, and either way nothing is installed and the stack is shuffled.
- **Moon Pool's reveal** (MS 4) is two choices of one card, each shuffled into R&D, and its counter answered, before the next is chosen; the counter is offered on the Corp's own installed cards, where the printed "an installed card" admits the Runner's, on which a counter does nothing. The Corp knows every card either way, so the outcome is the printed one.
- **The Twinning's "spend credits from an installed card"** (MS 3) is one moment per payment, however many cards paid toward it, about the first of them since UR 2 (Keiko); only the first each turn places a counter, so the count is the printed one.
- **Meeting of Minds** offers every card of the subtype in the grip and lets the Runner pick any number, a reveal of none included.
- **Hiram Svensson**: damage takes its cards as discards (`CardDiscarded`), so a hardware lost to damage the Runner is responsible for is not heard. **Hiram, ezaM, MuslihaT**: the look is in the Runner's log, not their view (MuslihaT's is shown only when it matches), so a bot's sample does not keep the card on top of R&D.
- **System Gateway and Elevation** (Phase 1 §8, the audits): Scrounge's optional half chains through `then`; Bling covers basic actions, not ability installs; Touch-ups' type choice is blind; two Mycowebs can loop; Scatter Field's strength is fixed at encounter; a determinized run lacks `initiated_by` and `ice_bypassed`; Mutual Favor's unaffordable found breaker installs to a no-op rather than being withheld; Conduit's counter is placed at run success rather than run end; Tāo Salonga's and Ballista's do-nothing options are offerable and no-op; Karunā's jack-out is recorded as `RunEndedByEffect`, as is Account Siphon's replaced breach; Snare!'s "must reveal it" in R&D is implicit in the access model; The Maker's Eye's access bonus timing likewise; a bare "+1 strength" (Corroder) is encounter-long by Null Signal Games' own default, unlike Gordian Blade's run-long — not an approximation, noted so nobody "fixes" it.

- **Self-modifying Code's search** (UR 1) offers only a program that could be installed at no discount, as Into the Depths' does; the printed search may find one it then cannot install.
- **Keiko** (UR 2): "you install a companion card" counts companion *resources* (the turn log's column, `Kind::CompanionResource`), and the pool's one other companion is Keiko itself, which hears its own install as no companion; and "spend credits from an installed companion card" is about the first card a payment took credits from (`CreditsSpentFromOutsidePool::first_host`), the narrowest pool, so a companion paying second toward one payment beside another card's credits is not heard. Every companion's credits in the pool are narrower than the other pools they could share a payment with.
- **Pauleʼs Café** (UR 2): "the first card you install this way during each of your turns" is the discounted ability, offered on the Runner's turn until it is used, and the plain one is offered only after it (`Not(And(DuringYourTurn, OncePerTurn))`); a discounted use that installs nothing spends the turn's discount.
- **Mystic Maemi** (UR 2): an empty grip takes the resource without asking, since the random trash could not be made.
- **Aniccam** (UR 3): a played event is recorded trashed (`TrashedFrom::PlayArea`, the rules' trash and the one that is a moment) as its play returns — for a run event, as its server is chosen and before its run, where the engine has always moved it to the heap — and an operation's is not recorded. Every other trash nobody carries out (a host leaving, uniqueness, the console limit) is still an occurrence of nothing; none trashes an event in the pool.
- **Buffer Drive** (UR 3): "1 or more cards are trashed" is one instruction's batch (`CardsTrashedFromGripOrStack`): damage, a mill, a card's trash, a selection to the heap and a trash cost each make one; a card paying its own [trash] cost from the grip makes none.
- **The Back** (UR 3): "the first time each turn you use a piece of hardware during a run" is a use limit spent as the trigger resolves during a run (`And(RunInProgress, OncePerTurn)`), because the turn log has no "during a run" to narrow a first time by — the second card deferred on it, after Ryō "Phoenix" Ōno — so a Back installed after the turn's first such use hears the next one. Using a piece of hardware is a paid ability of it (`OnAbilityUsed`) or credits spent off it first in a payment; a hardware's "may" trigger resolved (Devil Charm's) is not heard.
- **Simulchip** (UR 3): "an installed program has already been trashed this turn" counts the trashes a player carried out, either player's (`EventFilter::Anyone`); one the rules made (a trojan leaving with its ice) is not counted.
- **Wall to Wall** (UR 4): "resolve up to 3 in any order" is 3 of the 4, the counter's piece of ice optional, so a Corp that wants neither the draw nor the asset back in HQ must take one of them.
- **GameNET: Where Dreams are Real** (UR 5): "a Corp card ability causes the Runner to spend or lose" is read as four things, without a ruling to hand (NetrunnerDB's rulings were out of reach): a loss the card's text resolves (Gold Farmer's), the cost of an ability the card prints for the Runner (F2P's 2[credit]), a paid choice it offers ("end the run unless the Runner pays") and a bid in a trace it began (CR 10.8.6d). Credits a Corp card's standing effect makes the Runner pay — a raised cost, an additional cost — are no ability's and are not heard.
- **Transport Monopoly** (UR 5): "use this ability only during a run" is Proprionegation's `DuringRun`, the run from its initiation to the server's approach; after that the run's success is already decided.
- **Focus Group** (DF 4): "you may pay X[credit]" loses X from the credit pool rather than paying it (no Corp pool pays for an operation's effect), and "1 installed card" offers the Corp's own installs, where the printed card admits the Runner's, on which a counter does nothing.
- **Storgotic Resonator** (DF 3): "the first time each turn you trash a card that matches the faction of the Runner's identity" is a use limit spent as the trigger resolves (`And(TriggeringCardOfRunnersFaction, OncePerTurn)`), because the turn log has no faction to narrow a first time by — the third card deferred on it, after Ryō "Phoenix" Ōno and The Back — so a Resonator installed after the turn's first such trash hears the next.
- **Remastered Edition** (DF 2): "Place 1 advancement counter on an installed card" offers the Corp's own installs, where the printed card admits the Runner's, on which a counter does nothing (as Moon Pool's).
- **Project Vacheron** (UR 8): "it is worth 0 agenda points" is 3 taken off its printed 3 (`AgendaPoints` of −3 under the `while`); nothing else in the pool changes what an agenda in the Runner's score area is worth but Let Them Dream's own text.

### Bot debts — cards the heuristic never plays (Phase 5 §25's list)

The planner seats in the sweeps and `coverage_identical.py` reach these cards through random seats and their tests alone. Each is a blindness of the evaluator, not of the engine, and is the raw material for the precepts work. **The list is measured, not kept by hand, and since Phase 5 §29 (2 October 2026) it is the difference between seatings:** `scripts/blind_cards.py random.json planner.json` over two `diag precepts --deck-styles --report` passes on the same games — random seats, then planner seats — prints every card the random seats used at least five times that the planner never did, per side. "No seat used it" (`reach.unused_in_pass`, §25 Stage 1's reading) could not see a card the planner installs and never *uses* — an installed Matryoshka counted as used while the planner never hosted a copy on it (§28) — and the random seat's count is what says the engine offers the card in ordinary play, so the planner's zero is a judgment and not a card the decks never draw. `--coverage` runs the same diff over two `--headless --report` files and adds the prompts a card's text opened (`prompts_offered`), the instrument for a card whose use is an ability.

**Blind on `main` at #341 (2 October 2026; casual 192 games and startup 90, seeds 1 and 2, planner both chairs; random's count per pass, planner 0 in every pass it appears in):**

- *Runner* — ~~Madani~~ (115 / 163 casual, 71 / 77 startup — **paid**, Phase 5 §30, 2 October 2026: installed beside programs that wait, hosted on and installed from; off the list at 99 / 70 and 116 / 5 uses), ~~Pennyshaver~~ (35 / 80, 21 / 38), ~~Topan "Ormas" Leader's install~~ (61 / 60, 20 / 18 — **paid**, Phase 5 §33, 2 October 2026: samples carry the identities, so an identity's ability is a step of a plan; off the list on every pass at 46 / 58 / 37 / 30 uses), ~~Overclock~~ (36 / 53, 19 / 21), ~~Red Team~~ (43 / 50, 20 / 20), Tread Lightly (48 / 47, 19 / 24 — read since §31 but worth little at the forced-rez term's weight; off three of four passes), ~~Conduit~~ (25 / 40, 18 / 7), ~~Shred~~ (16 / 27, 11 / 14), ~~Clean Getaway~~ (18 / 26, 8 / 10), ~~Maintenance Access~~ (21 / 20, 9 / 15) — **the four struck through paid**, Phase 5 §31, 2 October 2026: a run a card began is priced with what the card put on it, and they are off the list on every pass — ~~Docklands Pass~~ (25 / 14, 6 / 5) — **Pennyshaver, Red Team, Conduit and Docklands Pass paid**, Phase 5 §32, 2 October 2026: a card that pays on a run is worth what its runs will pay, and the four are off the list on every pass (Conduit installed by the rig plan only, which alone reads an R&D access beyond the first) — Carnivore (12 / 27, – / 6), GAMEDRAGON™ Pro (15 / 13, 7 / –), Tranquilizer (10 / 16), Maglectric Rapid 748 Mod (– / 11, 7 / 5), Cacophony (10 / 5), Verbal Plasticity (startup 8 / 10), Cookbook (casual 15), Botulus (startup 22), Détente (startup 5).
- *Corp* — Byte! (casual seed 2, 51), Public Trail (11 / 14), ~~Leo Construction Labor Solutions~~ (12 / 10), ~~Synapse Global's ability~~ (7 / 6, startup 9) — **the two identities paid**, Phase 5 §33 with Topan: off the list on every pass, Synapse Global used 15 / 18 / 16 / 13 times, LEO Construction 4 / 6 and used badly (below) — Bigger Picture (12), Neurospike (9).

A card on the list is a term to write or a reading to repair, never a card to take out of a deck; the entries below are the hand list as it stood, kept for the *reasons* they record (which the report cannot say). A card the script names that is not here is a new debt; a card here the script no longer names has been paid.

- **Runner hardware and programs it never installs**: Basilar Synthgland 2KVJ, K2CP Turbine, Time Bomb (PH 4a); Poison Vial, WAKE Implant v2A-JRJ (PH 4b); World Tree (PH 5c); Flux Capacitor (PH 6a; Orca, at 10[credit], once in 48 games).
- **Resources it does not value**, PH 6b: Info Bounty (installed 31 times by random seats in 96 games, never by the planner in 48).
- **Events it never plays**, PH 6c: Spark of Inspiration and Raindrops Cut Stone (played 14 and 23 times by random seats in 96 games, never by the planner in 48).
- **Midnight Sun Stage 2's cards it never plays** (each Runner deck against Hostile Bid, seed 2, 48 planner games): Endurance (8[credit]; random seats installed it twice in 96 games and never used its break, which only its test reaches), Environmental Testing, PAN-Weave (its meat damage) and Chastushka (a sabotage the Corp chooses).
- **Midnight Sun Stage 3's cards it never plays** (Burn Rate and Dead Reckoning against Hostile Bid, seed 2, 48 planner games): Into the Depths (34 plays by random seats in 96 games), Ghosttongue, The Twinning; Daeg, First Net-Cat installed once.
- **Midnight Sun Stage 4's cards it never plays** (each edited Corp deck against Safety Net, seed 2, 48 planner games): Mutually Assured Destruction; Drago Ivanov's and Vladisibirsk City Grid's abilities (installed 59 and 65 times, never used); Mestnichestvo's encounter offer, because it never advances the ice; Moon Pool used once; Mavirus never rezzed.
- **Midnight Sun Stage 5's cards it barely plays** (the Corp decks against Safety Net, Hit List against Hostile Bid, seed 2, 48 planner games): Trust Operation played once (19 times by random seats in 96 games); Cat's Cradle installed seven times (38).
- **Midnight Sun Stage 6a's cards it never installs** (Encore against Retirement Package, seed 2, 48 planner games): Backstitching and Virtuoso (35 and 20 installs by random seats in 96 games). It plays Carpe Diem (46) and gains Nyusha's click (131 times).
- **Midnight Sun Stage 7b's cards it barely plays** (each Corp deck against Burn Rate, seed 2, 48 planner games): Big Deal once (Retirement Package; random seats 12 times in 96 games) — 17[credit] is rarely the planner's best click — and Mitosis twice (Permafrost; 13).
- **Midnight Sun Stage 8b's resource it never uses** (Burn Rate against Retirement Package and Hostile Bid, seed 2, 48 planner games each): Light the Fire! installed once in each and never used (random seats used it 15 and 22 times in 96 games). A click, the card and a core damage for a run on a remote is a price no term reads as buying the root. The planner rezzes Trieste Model Bioroids (25 of 35 installs) and makes its choice.
- **Midnight Sun Stage 8a's ice it prices short** (each Corp deck against Burn Rate, seed 2): the planner rezzes Echo (32 times in 48 games; random seats 54 in 96) and Envelopment (22; 6) and their subroutines fire, but the evaluator reads a piece of ice's printed subroutines (`eval::corp`'s end-the-run count, `eval::read`'s "can end the run"), so Echo reads as ice that never ends the run and Envelopment as ice that only trashes itself. A reading to repair: ask `continuous::own_subroutines` beside the printed list.
- **Midnight Sun Stage 6b's cards it never plays** (seed 2, 48 planner games): Pinhole Threading (Encore against Retirement Package; random seats played it 47 times in 96 games, 20 of them reaching the access). Deep Dive is played by no seat, as Chain Reaction is: three successful central runs in one turn (on the sweeps' rare list).
- **Economy resources and programs it does not value**: Friend of a Friend, Valentina Ferreira Carvalho, Coalescence; Laser Pointer, Banner; Monkeywrench, Saci, Pichação, Urban Art Vernissage; Lago Paranoá Shelter; AirbladeX (JSRF Ed.); the Core Set interrupts Decoy, Net Shield and Sacrificial Construct.
- **Abilities it never uses**: M.I.C.'s trash, Arissana Rocha Nahu's, Epiphany Analytica's counter; identity and multi-click abilities generally (Phase 1 §8); over-advancing for Dividends.
- **Breaks the evaluator could not price** — paid: Matryoshka's hosted copies, Lobisomem's and Audrey v2's counters, Hantu's counter pump and Tremolo's reduced cost are read as the engine charges them since Phase 5 §28 (2 October 2026), and the planner hosts a copy of Matryoshka ahead of a run worth the two clicks (0 → 11 hosts over 144 Hit List games). **Still owed:** Botulus and Poison Vial, whose `BreakSubroutinesUnconditionally` no evaluator reading prices (Botulus's would also need its host-ICE requirement read); a standing value for stock on the rig, so a copy is hosted on a turn with no run worth both clicks. Madani's hosted programs are paid (Phase 5 §30).
- **A run a card's text began** — paid, Phase 5 §31 (2 October 2026): the leaf reads the run's own credits, the rider on success, the server the run approaches, an armed prevention and a rez tax. **A card that pays on a run** — paid, Phase 5 §32 (2 October 2026): Red Team's rider per use, Pennyshaver's credit a successful run, Docklands Pass's breach access at the leaf and Conduit's counters as R&D accesses promised. **Still owed:** a trigger narrowed to a server or behind a condition or a cost is read as nothing (Gabriel Santiago's HQ credits, Stowaway, Rotary, Manuel Lattes de Moura, Devadatta Drone, Cupellation); Conduit is worth nothing to a plan other than the rig's.
- **Identities' text, on both chairs** — owed first, in four stages (Phase 5 Open, 3 October 2026). **The run is paid** (Phase 5 §34): what both identities print about a run's success, breach, accesses, a trash and its end is read at the leaf and in the Corp's run term — Gabriel Santiago, Zahya Sadeghi, Dewi Subrotoputri, René "Loup" Arcemont, Mercury Chrome, BANGUN's punishment of a faceup agenda. **The turn's end is paid** (§35): a decision the seat's own discard step parks on it is planned — PT Untaian's advance, Magdalene Keino-Chemutai's install, Méliès U.'s number; Nebula Talent Management's and Jinteki: Restoring Humanity's credits were already in the line. **Steals, scores and tags are paid** (§36): Jinteki: Personal Evolution's and Thule Subsea's steal costs and NBN: Reality Plus's first tag read at the leaf, core damage a hand size on both chairs, Synapse Global's and Poétrï's installs planned, BANGUN's faceup agenda worth its punishment. **Standing effects and hosted counters are paid** (§37): Issuaq Adaptics' counters as points and its agenda held for one, AU Co.'s, Epiphany Analytica's and the scored agendas' counters at half of what spending them buys, AU Co.'s turn-start search planned, Kate "Mac" McCaffrey's discount in a held card's price; Precision Design's hand size was already the engine's. **Tāo Salonga's swap is paid** (§38): the Runner reads the doors the Corp's rezzed ICE shuts, off a run, and the swap is planned as whole pairs. **The "may" ahead of an identity's selection of its own cards is paid** (§39): Precision Design's, Méliès U.'s, Barry "Baz" Wong's, Magdalene Keino-Chemutai's and Sebastião Souza Pessoa's yes is planned — Barry's install taken 0 → 144 times in 96 games. **Still owed:** why that install costs Barry's deck games over a game, which the decision played out both ways does not show.
- **An identity's ability** — paid, Phase 5 §33 (2 October 2026): samples carried no identity, so no identity's ability was a step of any plan. **LEO Construction's end-the-run is paid**, Phase 5 §43 (5 October 2026): it waits for the run's last window and pays only for what the breach would take, with Mercia B4LL4RD's turn-by-turn install read and the fort read between runs — uses 28 → 17 and at a run's initiation 20 → 1 in 96 Agency games. **Still owed:** 4 of the 17 are one-ply ties on a parked choice of the Corp's own, and two read a fort gained or a beaten wall as a gain (Phase 5 Open).
- **Corp cards it never plays or rezzes**: Distributed Tracing, Shipment from Vladisibirsk, Nonequivalent Exchange (played only by random seats), Hostile Architecture (installed 90 times, never rezzed), Dr. Vientiane Keeling (installed, never rezzed); it never trashes Amanuensis or Privileged Access, never purges (Malandragem, Physarum Entangler).
- **Uprising Stage 5's cards it barely plays** (each edited deck, seed 2, 48 planner games): Scapenet never played (Pay to Win against Safety Net; random seats 44 times in 96 games) — a trace's bid is a contest no line prices, so the operation is a card for nothing; Gold Farmer's break trigger never heard by either seat in that matchup, whose Runner never broke it (its subroutines fired 87 / 110 times); Transport Monopoly used once in each Corp deck (scored 4 and 12 times), its counters otherwise left on the agenda.
- **Uprising Stage 7b's Ganked! it rezzes** (Pay to Win against Safety Net, seed 2, 48 planner games): every Ganked! the planner installs it rezzes (51 of 51), which shows the Runner a trap that works face down — the client's `board::rez::gains_nothing` knows the rez is idle, and the planner's rez pricing does not.
- **Uprising Stage 7a's lockdowns it barely plays** (each edited deck against Safety Net, seed 2, random 96 / planner 48 games): SYNC Rerouting and NAPD Cordon never played (random 106 and 69 times), Argus Crackdown and Hyoubu Precog Manifold once (61 and 41), NEXT Activation Command 4 times (84) — a lockdown pays on the Runner's turn, past the end of the turn the planner plans, so its play is priced as a click and nothing.
- **Uprising Stage 6's cards it barely plays** (each edited deck, seed 2, random 96 / planner 48 games): the planner never installs Mu Safecracker or Boomerang in Hit List against Hostile Bid (random seats 14 and 22 times; Mu Safecracker's access offered 49 times), and uses Penrose once in Spare Parts (installed 28 times; random once in 14) — a bypass and a remembered ice are worth nothing to a line that prices only breaks. Afterimage is installed 9 times and bypasses 4 sentries for the planner; Gachapon is installed and used 49 times under both seats, with 150 cards removed from the game; Engram Flush fires 178 subroutines for the planner's Corp in Second Site against Safety Net (72 for random).
- **Uprising Stage 4's cards it barely plays** (each edited Corp deck against Safety Net, seed 2, 48 planner games): Kakurenbo and Digital Rights Management never played (random seats 11 and 73 times in 96 games) — a turn spent setting up a fast advance is priced as the turn it costs; False Lead scored 10 times and forfeited once, since a forfeit's two clicks are a term no line reads; Cayambe Grid rezzed 2 of 44 installs and La Costa Grid 5 of 37; Cyberdex Sandbox never scored by either seat.
- **Uprising Stage 3's cards it barely plays** (Hit List, Safety Net and Burn Rate against Hostile Bid, seed 2, 48 planner games): Bravado never played (random seats 13 times in 96 games) — its credits come when the run ends, which no line prices before it; The Back installed once and never charged; Cordyceps installed once, its swap never offered; Buffer Drive never installed.
- **Uprising Stage 2's cards it barely plays** (Dead Reckoning and Mixtape against Hostile Bid, seed 2, 48 planner games): Cybertrooper Talut never installed (random seats 24 times in 96 games) — a link and a strength for the turn are terms no line reads at the install; DreamNet never installed on Nova (twice on Hoshiko), a draw on a run the evaluator does not price before the run.
- **Uprising Stage 1's cards it barely plays** (Retirement Package and A Thousand Cuts against Safety Net, seed 2, 48 planner games): Bass CH1R180G4 used 3 times (random seats 47 times in 96 games) — a click and the card for two clicks is a click ahead, which no term reads as worth the card; Flower Sermon scored once (stolen 25 times), its counters used once.
- **Heap installs, hosted credits and hardware** (Phase 1 §8); the Corp undervalues paying for Byte! (Rules Audit, Masking).
- **A sample does not carry a copy's turn counts** (`determinize` leaves `CopyTurn` empty), so a sample of a Cloud Eater rezzed this turn does not see its encounter-end ability coming, a sample taken after Abaasy has fully broken ice this turn expects its first time still to come, and a sample never has an Euler installed this turn (UR 2), so its 0[credit] break is never part of a planned line.
- **Chain Reaction** needs successful runs on all three centrals in one turn with a click to spare, which no agent plans: on `CARDS_RARE_WITH_SWEEP_DECKS` with that reason.

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
   Market (built). **2b — ice with no Barrier, Code Gate or
   Sentry:** Vicsek, alone, with the change to `CardType::Ice` it needs,
   which the rest of the unmodelable ice (Loot Box, Rime, Konjin,
   Excalibur, Lycian Multi-Munition) then reuse (built).
3. **Turn-log counts and standing kinds** (3a and 3b built) (`PlayCost`, `AgendaPoints` and
   `StealCost` are the `ContinuousKind` words, each a new kind held to the
   DSL Growth Rule): Chain Reaction, Underdome Irregulars, Reverb, Hype
   Machine, Tailgate, Perfect Recall, The Red Room, Lotus Haze, Magistrate
   Revontulet, Let Them Dream, and Synchrocyclotron, moved here from Stage
   2 because "the first double operation you play each turn costs [click]
   less" is a `PlayCost` on a click, with the "first each turn" of a price
   (`TurnLog::none_yet`).
4. **Ice and run triggers:** **4a**, the run's moments about ice
   (built): Vertigo, Sipa, Lethe, ezaM, The Tungsten Tailor.
   **4b** (built): Lionsmane, Event Horizon, Ansel 2.0.
5. **Runner trigger and payment words.** **5a** (built):
   Methuselah, Touchstone, Caveat Emptor, and Shackleton Grid, moved here
   from Stage 4: "when the Runner spends credits from outside their
   credit pool" is a moment the payment sites have to dispatch. **5b**
   (built): Stowaway (a Trojan's "this server" is its host's),
   Nurse Hạnh (Archives' facedown cards turned faceup, two or more).
   **5c** (built): Beta Build (a run-end rider about the program it
   installed). **5d** (built): Hiram (a trash the Runner carries
   out, from any location, is a moment, CR 1.14.5a; "look at the top card
   of R&D" shows one player a hidden card). Stage 5 is complete.
6. **Arrange, reveal, stealth credits.** **6a** (built): Cultivate,
   Knowledge Seeker, Esca — arranging the top of R&D, a reveal while
   accessed in R&D, an encounter's end as a moment. **6b:** Corsair, Baker,
   Lampades — credits spendable only from stealth cards (built).
   **6c** (built): Aircheck,
   which locks the credit pool and so breaks the invariant that a payment
   may always reach the pool. Stage 6 is complete.
7. **Hosting, the score area and access limits**, split by mechanic.
   **7a** (built): Luana Campos, a card leaving the table as a
   moment it can interrupt. **7b** (built): Read-Write Share,
   cards hosted facedown. **7c** (built): Hackerspace, a resource
   installed onto a resource. **7d** (built): Myōshu, Word on the
   Street and Sacrifice Zone Expansion, the score area. **7e** (built): Stick and Poke, a subroutine gained for an encounter. **7f**
   (built): Tocsin, an ability used from HQ. **7g** (built): Flagship, a run that cannot be declared successful and a limit
   on accesses. Stage 7 is complete.
8. **Méliès U, alone:** hidden setup state, three sides, and masking
   (built). **Vantage Point is complete, 66 of 66.**

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

**Closed stages** — one line each; the record is in [the archive](archive/nsg-card-pool.md) under the same heading.

- **Stage 1a** — every printed subtype, read from the catalog (`feat/vp-stage-1-subtypes`, 26 September 2026).
- **Stage 1b** — the first thirteen cards (`feat/vp-stage-1b-first-cards`, 26 September 2026).
- **Stage 2** — bad publicity, tags and costs (`feat/vp-stage-2-bad-publicity`, 26 September 2026).
- **Stage 2b** — ice with none of the three types (`feat/vp-stage-2b-ice-with-no-breaker-type`, 26 September 2026).
- **Stage 3a** — standing kinds: play, steal and score prices (`feat/vp-stage-3a-standing-kinds`, 26 September 2026).
- **Stage 3b** — turn-log counts and hosted counters (`feat/vp-stage-3b-turn-counts-and-counters`, 26 September 2026).
- **Stage 4a** — the run's moments about ice (`feat/vp-stage-4a-ice-moments`, 26 September 2026).
- **Stage 4b** — two costs and a break on this ice (`feat/vp-stage-4b-lionsmane-event-horizon-ansel`, 26 September 2026).
- **Stage 5a** — credits during runs, a spend from outside the pool, a next turn's clicks (`feat/vp-stage-5a-methuselah-touchstone-shackleton-caveat`, 26 September 2026).
- **Stage 5b** — a count on a moment, a Trojan's server (`feat/vp-stage-5b-stowaway-nurse-hanh`, 26 September 2026).
- **Stage 5c** — "that program", through the run it starts (`feat/vp-stage-5c-beta-build`, 27 September 2026).
- **Stage 5d** — who trashed it, and a look nobody else sees (`feat/vp-stage-5d-hiram`, 27 September 2026).
- **Stage 6a** — arrange, a reveal, and the end of an encounter (`feat/vp-stage-6a-arrange-reveal-encounter-end`, 27 September 2026).
- **Stage 6b** — credits only from stealth cards (`feat/vp-stage-6b-stealth-credits`, 27 September 2026).
- **Stage 6c** — a locked credit pool (`feat/vp-stage-6c-aircheck`, 27 September 2026).
- **Stage 7a** — one door off the table (`feat/vp-stage-7a-luana-campos`, 27 September 2026).
- **Stage 7b** — cards hosted facedown (`feat/vp-stage-7b-runner-hosting`, 27 September 2026).
- **Stage 7c** — a resource installed onto a resource (`feat/vp-stage-7c-hackerspace`, 27 September 2026).
- **Stage 7d** — the score area (27 September 2026).
- **Stage 7e** — a subroutine gained for an encounter (27 September 2026).
- **Stage 7f** — an ability used from HQ (27 September 2026).
- **Stage 7g** — a run not declared successful, and a limit on accesses (27 September 2026).
- **Stage 8** — Méliès U, a secret identity with three reverse sides (27 September 2026).
- **After Vantage Point** — both clients show what it added (27 September 2026).

### 2. Rebellion Without Rehearsal — 65 cards (C 7 / V 43 / M 15)

**Decks:** Sweep decks on its three identities.

**Stages:**
1. **Composes, plus small requirements** (built): Friend of a
   Friend, Corporate Hospitality, Boto, Capacitor, Seraph, The Powers That
   Be, Coalescence, Valentina Ferreira Carvalho, Pressure Spike. Eye for
   an Eye moved to Stage 3 when the stage was taken: its "Access →" is an
   ability on an *event*, active through the run it starts (CR 8.6.5),
   which no card has needed yet.
2. **Tags, purge, and a cost on the Corp's basic trash action**, in two
   parts when it was taken: **2a**, tags and the trash cost (built):
   Sebastião Souza Pessoa, Manuel Lattes de Moura, Privileged Access,
   Amanuensis, Amelia Earhart, Juli Moreira Lee, Arruaceiras Crew; **2b**,
   purge and bypass (built): Malandragem, Physarum Entangler.
   Heliamphora moved to Stage 3 when 2b was taken: its "whenever you
   would access a card in Archives, you may host it faceup on this
   program instead" is an interrupt at an access, and hosting the card
   being accessed is Cupellation's too; its purge half is 2b's trigger.
3. **Runner access and run words**, in three parts when it was taken:
   **3a**, the small words (built): Boi-tatá, Meeting of Minds,
   “Pretty” Mary da Silva, Ashen Epilogue; **3b**, access abilities not
   on an installed card, and a card hosted as it is accessed (built): Eye for an Eye, Cupellation, Heliamphora; **3c**, run words
   (built): Trick Shot, Window of Opportunity, Alarm Clock; **3d**,
   a search that installs onto a host (built): Muse. Stage 3 is
   complete.
4. **Corp advancement and movement words**, in three parts when it was
   taken: **4a**, advancement counters placed and removed (built):
   Charlotte Caçador, Cohort Guidance Program, Logjam, Business As Usual,
   Kingmaking; **4b**, an upgrade that moves and counters that move
   (built): Isaac Liberdade, Hearts and Minds; **4c**, where an
   install came from, and a card returned to HQ as a cost (built):
   The Holo Man, Stoke the Embers, Janaína “JK” Dumont Kindelán. Stage 4
   is complete.
5. **Corp ice and rez words**, in five parts when it was taken: **5a**, a
   rez's price, counters on a rez and a mandate (built): Piranhas,
   Working Prototype, Sudden Commandment; **5b**, ice that limits what
   happens in its encounter (built): Hammer, Sorocaban Blade, Cloud
   Eater; **5c**, derez words (built): Brasília Government Grid,
   Warm Reception; **5d**, a delayed conditional ability, "when this turn
   ends" (CR 9.6.13, built): Lightning Laboratory, moved out of 5c
   when it was taken; **5e**, a Weyland identity and its agenda (built): Nuvem
   SA: Law of the Land, The Basalt Spire, on a Sweep deck of Nuvem's own.
   Stage 5 is complete.
6. **Terminal, reveal, set aside, arrange, X**, in five parts when it was
   taken: **6a**, terminal operations (built): Active Policing,
   Bring Them Home; **6b**, cards that stay revealed while the Runner
   chooses among them (built): Burner; **6c**, a breach outside a
   run (built): Cataloguer, moved out of 6b when it was taken; **6d**, cards set
   aside (built): The Wizard’s Chest; **6e**, an X cost (built): Lobisomem. Stage 6 is
   complete.
7. **Expendable, moving ice, psi, re-encounter**, in five parts when it was
   taken: **7a**, expendable cards (built): Eminent Domain, Descent;
   **7b**, a psi game (built): See How They Run; **7c**, ice that moves as a run
   begins (built): Tributary; **7d**, an encounter repeated (built): Sisyphus Protocol;
   **7e**, a trojan hosted by an event's ability (built): Spree.
   Stage 7 is complete.
8. **A card's identity changes**, in three parts when it was taken: **8a**,
   a subtype chosen at rez (built): Lycian Multi-Munition; **8b**, a
   subroutine gained for the rest of the run (built): Thunderbolt
   Armaments: Peace Through Power, on a Sweep deck of its own; **8c**, a card added to the
   Runner's score area as an agenda, and a third way to win (built): Jeitinho, on a Sweep deck of its own. Stage 8 is complete, and
   with it Rebellion Without Rehearsal, 65 of 65.

**Riskiest:**
- Jeitinho: a third way to win (`rules/win.rs`).
- Sisyphus Protocol: an encounter re-entered, when the run's state machine
  only moves forward.
- Tributary: ice moved mid-run, reconciled with `RunState::ice`.
- Lycian Multi-Munition: a subtype chosen at rez that persists.
- See How They Run.

**Banned:** Tributary and Trick Shot, on Standard's list.

**Closed stages** — one line each; the record is in [the archive](archive/nsg-card-pool.md) under the same heading.

- **Stage 1** — the first nine cards (`feat/rwr-stage-1-first-cards`, 27 September 2026).
- **Stage 2a** — tags and a cost on the Corp's basic trash action (`feat/rwr-stage-2a-tags-and-basic-trash-cost`, 27 September 2026).
- **Stage 2b** — purge and bypass (`feat/rwr-stage-2b-purge-and-bypass`, 27 September 2026).
- **Stage 3a** — the small words (`feat/rwr-stage-3a-small-words`, 27 September 2026).
- **Stage 3b** — the run's event, and a card hosted as it is accessed (`feat/rwr-stage-3b-access`, 27 September 2026).
- **Stage 3c** — run words (`feat/rwr-stage-3c-run-words`, 27 September 2026).
- **Stage 3d** — a search that installs onto a host (`feat/rwr-stage-3d-muse`, 27 September 2026).
- **Stage 4a** — advancement counters placed and removed (`feat/rwr-stage-4a-advancement-words`, 27 September 2026).
- **Stage 4b** — an upgrade that moves, and counters that move (`feat/rwr-stage-4b-moving-upgrades`, 27 September 2026).
- **Stage 4c** — where an install came from, and a card back to HQ (`feat/rwr-stage-4c-install-origin`, 27 September 2026).
- **Stage 5a** — a rez's price, counters on a rez, a mandate (`feat/rwr-stage-5a-piranhas-working-prototype-sudden-commandment`, 28 September 2026).
- **Stage 5b** — ice that limits its encounter (`feat/rwr-stage-5b-hammer-sorocaban-blade-cloud-eater`, 28 September 2026).
- **Stage 5c** — derez words (`feat/rwr-stage-5c-brasilia-warm-reception`, 28 September 2026).
- **Stage 5d** — a delayed conditional ability (`feat/rwr-stage-5d-lightning-laboratory`, 28 September 2026).
- **Stage 5e** — a resolution's end, and a trash from R&D (`feat/rwr-stage-5e-nuvem-basalt-spire`, 28 September 2026).
- **Stage 6a** — terminal operations (`feat/rwr-stage-6a-terminal-active-policing-bring-them-home`, 28 September 2026).
- **Stage 6b** — cards that stay revealed (`feat/rwr-stage-6b-burner`, 28 September 2026).
- **Stage 6c** — a breach with no run (`feat/rwr-stage-6c-cataloguer`, 28 September 2026).
- **Stage 6d** — cards set aside (`feat/rwr-stage-6d-wizards-chest`, 28 September 2026).
- **Stage 6e** — an X cost, and the object that fully breaks (`feat/rwr-stage-6e-lobisomem`, 28 September 2026).
- **Stage 7a** — expendable cards (`feat/rwr-stage-7a-expendable-eminent-domain-descent`, 28 September 2026).
- **Stage 7b** — a psi game (`feat/rwr-stage-7b-psi-see-how-they-run`, 28 September 2026).
- **Stage 7c** — ice that moves as a run begins (`feat/rwr-stage-7c-tributary`, 28 September 2026).
- **Stage 7d** — an encounter repeated (`feat/rwr-stage-7d-sisyphus-protocol`, 28 September 2026).
- **Stage 7e** — a trojan hosted by an event's ability (`feat/rwr-stage-7e-spree`, 28 September 2026).
- **Stage 8a** — a subtype chosen at rez, held while rezzed (`feat/rwr-stage-8a-lycian-multi-munition`, 28 September 2026).
- **Stage 8b** — a subroutine gained for the rest of the run (`feat/rwr-stage-8b-thunderbolt-armaments`, 28 September 2026).
- **Stage 8c** — a card added to the Runner's score area, and a third way to win (`feat/rwr-stage-8c-jeitinho`, 28 September 2026).

### 3. The Automata Initiative — 65 cards (C 14 / V 35 / M 16)

**Decks:** Sweep decks on its four identities (Az McCaffrey's,
Moonlighting, at Stage 1; Lat's, Level Pegging, at Stage 4).

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

**Closed stages** — one line each; the record is in [the archive](archive/nsg-card-pool.md) under the same heading.

- **Stage 1** — nine Corp cards, and a standing "cannot" (`feat/tai-stage-1-attini-cannot-spend-credits`, 28 September 2026).
- **Stage 2** — ten cards, and two small words (`feat/tai-stage-2-valentao-credits-armed-asset-faceup`, 28 September 2026).
- **Stage 3** — the pass, and the ice's subtypes (`feat/tai-stage-3-pass-triggers-banner`, 28 September 2026).
- **Stage 4** — Trojans, host ice, and back to the grip (`feat/tai-stage-4-trojans-host-scopes`, 28 September 2026).
- **Stage 5** — Corp install and server words (`feat/tai-stage-5-corp-install-and-server-words`, 28 September 2026).
- **Stage 6** — Runner run words (`feat/tai-stage-6-runner-run-cards`, 28 September 2026).
- **Stage 7** — a breach as a moment, and a breach of another server (`feat/tai-stage-7-expendables-breach-redirect`, 28 September 2026).
- **Stage 8a** — a "when encountered" ability prevented, and subroutines gained by count (`feat/tai-stage-8a-starlit-knight-airbladex`, 28 September 2026).
- **Stage 8b** — a psi game on a successful run, and costs paid in grip cards (`feat/tai-stage-8b-adrian-seis-daniela`, 29 September 2026).
- **Stage 8c** — a limit on remote servers, and a program installed during a run (`feat/tai-stage-8c-arissana-a-teia`, 29 September 2026).
- **Stage 8d** — "the same action", and an agenda used from the Runner's score area (`feat/tai-stage-8d-wage-workers-oracle-thinktank`, 29 September 2026).

### 4. Parhelion — 63 cards (C 19 / V 26 / M 18)

**Decks:** Sweep decks on its four identities. Nova Initiumia and Ampère
change deck-building rules (`DeckRule`, Stage 5a).

**Stages:**
1. **Corp, composes:** Thule Subsea: Safety Below, Distributed Tracing, Djupstad Grid,
   Reaper Function, Vampyronassa, Post-Truth Dividend, Gaslight, Vera
   Ivanovna Shuyskaya, Nonequivalent Exchange, Shipment from Vladisibirsk.
2. **Mixed, composes:** Hostile Architecture, End of the Line, Finality,
   Katorga Breakout, Nga, Num, Zenit Chip JZ-2MJ, Hippocampic Mechanocytes, Dr. Nuka
   Vrolyck.
3. **Advancement requirement** (the deferred `ContinuousKind` its doc
   names) **and Corp words**, split by mechanic when it was taken (30
   September 2026): **3a**, the requirement (built): Ontological
   Dependence, Freedom of Information, Regulatory Capture; **3b**, the
   Corp words (built): Simulation Reset, Hypoxia, Mr. Hendrik; **3c**, harmonic
   ice (built): Pulse, Bloop. Stage 3 is complete.
4. **Runner standing and breaker words**, split by mechanic when it was
   taken (1 October 2026): **4a**, the standing words (built): Basilar
   Synthgland 2KVJ, Dr. Vientiane Keeling, K2CP Turbine, Time Bomb;
   **4b**, the breaker words (built): Tremolo, Poison Vial, WAKE Implant
   v2A-JRJ, Abaasy. Stage 4 is complete.
5. **Zones, selection and deck building**, split by mechanic when it was
   taken (1 October 2026): **5a**, deck building (built): Nova Initiumia:
   Catalyst & Impetus, Ampère: Cybernetics For Anyone; **5b**, Archives (built):
   Hybrid Release, Nanisivik Grid; **5c**, trashes (built): Kimberlite Field, Yakov
   Erikovich Avdakov, World Tree; **5d**, the stack and HQ (built): Asmund
   Pudlat, Concerto, Reprise. Stage 5 is complete.
6. **Charge, mark, set aside, Runner removal from game, counters on a run
   event**, split by mechanic when it was taken (1 October 2026): **6a**,
   charge (built): Flux Capacitor, Orca; **6b**, the mark (built): Tunnel
   Vision, Info Bounty; **6c**, a set-aside program and counters on a run event
   (built): Spark of Inspiration, Raindrops Cut Stone; **6d**, a Runner card removed
   from the game as it is uninstalled (built): Nanuq. Stage 6 is complete.
7. **Winning and the score area** (built, 2 October 2026): Issuaq Adaptics:
   Sustaining Diversity, Superdeep Borehole, Nightmare Archive, Matryoshka
   (X cost). Stage 7 is complete.
8. **Losing abilities and break restrictions** (built, 2 October 2026):
   Hush, Klevetnik, Anvil, Unsmiling Tsarevna, Hafrún, Tsakhia "Bankhar"
   Gantulga, ZATO City Grid. Stage 8 is complete, and Parhelion with it.

**Riskiest:**
- Hush.
- Tsakhia: replaces every subroutine's resolution for an encounter.
- Hafrún.

**Banned:** Dr. Vientiane Keeling, K2CP Turbine, Matryoshka, Nanisivik
Grid, Tsakhia, World Tree.

**Closed stages** — one line each; the record is in [the archive](archive/nsg-card-pool.md) under the same heading.

- **Stage 1** — ten Corp cards, composed (`feat/ph-stage-1-corp-composes`, 29 September 2026).
- **Stage 2a** — seven Runner cards and a Weyland operation, composed (`feat/ph-stage-2a-runner-composes`, 29 September 2026).
- **Stage 2b** — the Runner's trash of an installed Corp card, its own included (`feat/ph-stage-2b-hostile-architecture`, 29 September 2026).
- **Stage 3a** — the advancement requirement, asked (`feat/ph-stage-3a-advancement-requirement`, 30 September 2026).
- **Stage 3b** — the Corp words: "that many", an operation out of the game, and every click as a price (`feat/ph-stage-3b-corp-words`, 30 September 2026).
- **Stage 3c** — harmonic ice: a count of installs, and a derez to pay for a rez (`feat/ph-stage-3c-harmonic-ice`, 30 September 2026).
- **Stage 4a** — Runner standing words: allotted clicks, the other player's hand size, and "install only if" (`feat/ph-stage-4a-runner-standing-words`, 1 October 2026).
- **Stage 4b** — breaker words: a cost a credit less per card, a break after a break, and a first time counted on the breaker (`feat/ph-stage-4b-breaker-words`, 1 October 2026).
- **Stage 5a** — the deckbuilding rules an identity prints (`feat/ph-stage-5a-deck-building-identities`, 1 October 2026).
- **Stage 5b** — Archives: an install of the copy chosen, and a subroutine of ice in Archives (`feat/ph-stage-5b-archives`, 1 October 2026).
- **Stage 5c** — trashes: the card a nested cost took, and where a trashed install stood (`feat/ph-stage-5c-trashes`, 1 October 2026).
- **Stage 5d** — the stack and HQ: a search for different names, and a run named for the card whose text began it (`feat/ph-stage-5d-stack-and-hq`, 1 October 2026).
- **Stage 6a** — charge, and the first time each encounter (`feat/ph-stage-6a-charge`, 1 October 2026).
- **Stage 6b** — the mark (`feat/ph-stage-6b-mark`, 1 October 2026).
- **Stage 6c** — a set-aside program, and a subroutine resolving heard by a run event (`feat/ph-stage-6c-set-aside-and-run-event-counters`, 1 October 2026).
- **Stage 6d** — a Runner card removed from the game as it leaves the table (`feat/ph-stage-6d-nanuq`, 1 October 2026).
- **Stage 7** — winning and the score area: a target a card lowers, a card that wins when it is empty, an accessed card moved by its own text, and copies turned facedown (`claude/serene-einstein-6bhlig`, 2 October 2026).
- **Stage 8** — losing abilities and break restrictions: a host that keeps only its printed subroutines, a resource without its abilities, limits on breaking for a duration and on one Runner card, a server chosen for the turn, and an ability one card gives another (`claude/serene-einstein-6bhlig`, 2 October 2026). **Parhelion is complete: 63 of 63.**

### 5. Midnight Sun and its Booster Pack — 65 cards (C 22 / V 26 / M 17)

**Decks:** Sweep decks on its five identities.

**Stages:**
1. **Corp, composes** (built, 2 October 2026): Anemone, Élivágar Bifurcation,
   Refuge Campaign, Bladderwort, Artificial Cryptocrash, Ubiquitous Vig,
   Vasilisa, Pravdivost Consulting: Political Solutions, Svyatogor Excavator,
   Maskirovka, Extract. Stage 1 is complete.
2. **Runner, composes** (built, 2 October 2026): Revolver, Chastushka, Running Hot, Marrow,
   Avgustina Ivanovskaya, PAN-Weave, No Free Lunch, Endurance, Hyperbaric, Propeller, Environmental Testing.
   Stage 2 is complete.
3. **Charge on PH's rule, core damage words** (built, 2 October 2026): Captain Padma Isbister, Rigging Up,
   “Daeg, First Net-Cat”, Stoneship Chart Room, Into the Depths, Esâ Afontov: Eco-Insurrectionist, Begemot,
   Ghosttongue, The Twinning, Cezve. Stage 3 is complete.
4. **Advancement counters** (built, 2 October 2026): Vladisibirsk City Grid, Drago Ivanov,
   Mestnichestvo, Chekist Scion, Mutually Assured Destruction, Moon Pool, Azef Protocol,
   Midnight-3 Arcology, Mavirus. Stage 4 is complete.
5. **Ice words** (built, 2 October 2026): Cat's Cradle, Ivik, Wave, Bathynomus, Stavka, Hákarl 1.0,
   Trust Operation. Stage 5 is complete.
6. **Mark on PH's rule, then access outside a breach:** split by mechanic
   when it was taken. **6a, the mark** (built, 3 October 2026): Nyusha "Sable"
   Sintashta, Carpe Diem, Backstitching, Virtuoso. **6b, access outside a
   breach** (built, 3 October 2026): Pinhole Threading, Deep Dive. Stage 6
   is complete.
7. **The score area and the turn:** split by mechanic when it was taken.
   **7a, the score area** (built, 3 October 2026): Backroom Machinations,
   Regenesis, Blood in the Water, Steelskin Scarring. **7b, the turn** (built,
   3 October 2026): Big Deal, Mitosis. Stage 7 is complete.
8. **Subroutine lists and ability layers:** split by mechanic when it was
   taken. **8a, subroutine lists** (built, 3 October 2026): Echo,
   Envelopment. **8b, ability layers** (built, 3 October 2026): Trieste
   Model Bioroids, Light the Fire!. **8c, the trash chain** (built, 3 October
   2026): Ob Superheavy Logistics, on a Sweep deck of its own, Supply Chain.
   Stage 8 is complete, and Midnight Sun with it, 65 of 65 and its booster
   pack 7 of 7.

**Riskiest:**
- Ob Superheavy Logistics: a general trash trigger, a search keyed on the
  trashed card, install-and-rez, and it chains.
- Light the Fire!
- Echo and Envelopment.
- Virtuoso: a second breach when the run ends.
- Pinhole Threading.

**Banned:** Drago Ivanov, Endurance, Nyusha "Sable" Sintashta, Svyatogor
Excavator.

**Closed stages** — one line each; the record is in [the archive](archive/nsg-card-pool.md) under the same heading.

- **Stage 1** — eleven Corp cards, composed (`claude/serene-einstein-6bhlig`, 2 October 2026).
- **Stage 2** — eleven Runner cards, composed, with two words widened: a virus program counted apart in the turn log, and a breaker's strength remembered past its own trash cost (`claude/serene-einstein-6bhlig`, 2 October 2026).
- **Stage 3** — charge and core damage: ten Runner cards with no new `Effect`, a damage suffered and a spend off an installed card as moments, a count of ice passed this run, a payment word for runs on central servers, and two Sweep decks, Burn Rate (Esâ Afontov) and Dead Reckoning (Captain Padma Isbister) (`claude/serene-einstein-6bhlig`, 2 October 2026).
- **Stage 4** — advancement counters: nine Corp cards with no new `Effect`, an agenda's own additional cost to score, a discard step skipped for a turn, and one amount plus another (`claude/serene-einstein-6bhlig`, 2 October 2026).
- **Stage 5** — ice words: seven cards with no new `Effect`, a scope over each piece of ice of a kind, ice protecting a named server, and the Runner kept off the paid abilities printed on bioroid ice (`claude/serene-einstein-6bhlig`, 2 October 2026).
- **Stage 6a** — the mark: four Criminal cards with no new `Effect`, the mark asked by server, a run's end effects kept as a list, and a Sweep deck on Nyusha "Sable" Sintashta, Encore (`claude/serene-einstein-6bhlig`, 3 October 2026).
- **Stage 6b** — access outside a breach: Pinhole Threading and Deep Dive, one new `Effect` both use (`Access`), the Corp's set-aside zone, and the Runner kept from stealing or trashing an agenda (`claude/serene-einstein-6bhlig`, 3 October 2026).
- **Stage 7a** — the score area: four cards with no new `Effect`, one door into Archives counted in the turn log, and an agenda printed with an X (`claude/serene-einstein-6bhlig`, 3 October 2026).
- **Stage 7b** — the turn: Big Deal and Mitosis, one new `Effect` (`Score`, the action's scoring shared with a card's text), a card kept from being rezzed for a turn, and an install into a new remote only (`claude/serene-einstein-6bhlig`, 3 October 2026).
- **Stage 8a** — subroutine lists: Echo and Envelopment with no new `Effect`, the subroutines a piece of ice gains by its own static ability, counted, in the rules' order, and ice with none fully broken as it is encountered (`claude/serene-einstein-6bhlig`, 3 October 2026).
- **Stage 8b** — ability layers: Trieste Model Bioroids and Light the Fire! with no new `Effect`, a piece of ice Runner cards cannot break while the card that chose it is rezzed, the root of the attacked server losing its abilities for a run, and every card in it trashed (`claude/serene-einstein-6bhlig`, 3 October 2026).
- **Stage 8c** — the trash chain: Ob Superheavy Logistics with no new `Effect`, a trash heard as of a rezzed card outside an install, the trashed card's printed cost, an exact printed cost, and an install-and-rez ignoring credit costs alone; Ob's Sweep deck, Supply Chain (`claude/serene-einstein-6bhlig`, 3 October 2026). **Midnight Sun is complete: 65 of 65, its booster pack 7 of 7.**

### 6. Uprising and its Booster Pack — 65 cards (C 10 / V 37 / M 18)

**Decks:** Sweep decks on its three identities (Hoshiko Shiro's, Side
Quest, at Stage 2; GameNET's, Pay to Win, at Stage 5; Earth Station's,
Ground Control, at Stage 8).

**Re-read at Stage 1** (3 October 2026, against the DSL at Midnight Sun's
close). Most of what the survey called vocabulary is now built: the turn's
end is heard (`OnDiscardPhaseEnd`, Adrian Seis), stealth credits are VP
6b's, set aside RWR 6d's and MS 6b's, psi RWR 7b's, a forced encounter
RWR 7d's, a remembered choice RWR 8a's, PH 8's and MS 8b's, and an
additional cost to steal, score or trash VP 3a's, RWR 2a's and MS 4's.
**Still new:** lockdown (CR 3.5.1c, Stage 7), the first card to start a
trace (`Effect::Trace`, the one unused variant, Stage 5), an encounter
nested inside an encounter (Konjin, CR 6.1.3c), a replacement as an agenda
enters the Runner's score area (Project Vacheron), and an additional cost
to *run* that changes when the identity flips (Earth Station, CR 6.3.2b).
The stage order below stands.

**Stages:**
1. **Composes** (built, 3 October 2026): Moshing, Self-modifying Code,
   Daily Casts, Bass CH1R180G4, Cerebral Overwriter, Drafter, Flower Sermon,
   Prāna Condenser, Bellona, Colossus. Stage 1 is complete.
2. **The turn's end, `PaysFor` and subtype words** (built, 4 October 2026):
   Mystic Maemi, Paladin Poemu, Hoshiko Shiro, Penumbral Toolkit, Mantle,
   Keiko, Cybertrooper Talut, Odore, DreamNet, Euler, Pauleʼs Café. Stage 2
   is complete.
3. **Runner triggers and zones** (built, 4 October 2026): Swift, Aniccam,
   Buffer Drive, The Back, Prognostic Q-Loop, Simulchip, Harmony AR Therapy,
   Devil Charm, Bravado, Cordyceps. Stage 3 is complete.
4. **Corp server and advancement words** (built, 4 October 2026): La Costa
   Grid, Cayambe Grid, Tranquility Home Grid, Digital Rights Management,
   Vaporframe Fabricator, Wall to Wall, Kakurenbo, False Lead, Cyberdex
   Sandbox. Stage 4 is complete.
5. **Break triggers, and the first card to start a trace** (built, 4
   October 2026): Gold Farmer, Makler, Týr, F2P, GameNET: Where Dreams are
   Real, Scapenet (`Effect::Trace` and `rules/trace.rs`, reached by no card
   until now), Transport Monopoly. Stage 5 is complete.
6. **Stealth credits on VP's rule, a remembered choice, set aside** (built,
   4 October 2026): Mu Safecracker, Afterimage, Penrose, Boomerang, Engram
   Flush, Gachapon. Stage 6 is complete.
7. **Lockdown, then psi, then forced encounters**, in two halves:
   - 7a. **Lockdown** (built, 5 October 2026): SYNC Rerouting, Argus
     Crackdown, NAPD Cordon, NEXT Activation Command, Hyoubu Precog
     Manifold. Stage 7a is complete.
   - 7b. **An encounter away from the run's position** (built, 5 October
     2026; CR 6.5.9a): Konjin (from inside another encounter, CR 6.1.3c)
     and Ganked! (from inside an access). Stage 7 is complete.
8. **Points, subroutine lists and run costs that change mid-game** (built,
   5 October 2026): Megaprix Qualifier, Project Vacheron, Winchester,
   Akhet, Earth Station. Uprising is complete.

**Riskiest:**
- ~~Project Vacheron: a replacement on entering the Runner's score area, with
  points read off its counters.~~ Built in UR 8.
- ~~Konjin: an encounter nested inside an encounter (CR 6.1.3c).~~ Built in UR 7b.
- ~~Lockdown operations: they stay in play across turns.~~ Built in UR 7a.
- ~~Earth Station: SEA Headquarters: an additional cost to run (CR 6.3.2b) that changes when the
  identity flips.~~ Built in UR 8.
- ~~Stealth credits, if VP's rule has not already built them.~~ Built in VP 6b.

**Banned:** Bellona, Cayambe Grid, Cyberdex Sandbox, Engram Flush, Gold
Farmer, Hoshiko Shiro, Moshing, Project Vacheron.

**Closed stages** — one line each; the record is in [the archive](archive/nsg-card-pool.md) under the same heading.

- **Stage 1** — ten cards, composed, with no new `Effect` and no change to the engine (`claude/serene-einstein-6bhlig`, 3 October 2026).
- **Stage 2** — eleven Runner cards and Hoshiko Shiro's Sweep deck, Side Quest, with no new `Effect` and one fewer: hosted credits for using a card and for playing one, a companion column in the turn log, a spend about the card it came off, a discount that is an amount, and a rig card's own install counted (`claude/serene-einstein-6bhlig`, 4 October 2026).
- **Stage 3** — ten Runner cards with no new `Effect`: a played event is trashed as it finishes resolving, a trash can be heard as anyone's, the Runner's grip and stack trash in batches a card can choose from, using any paid ability is a moment, a completed run remembers the ice it passed, run events have a turn-log column, and a selection can be "up to" an amount (`claude/serene-einstein-6bhlig`, 4 October 2026).
- **Stage 4** — nine Corp cards and one new `Effect` (`TurnArchivesFacedown`): the first install each turn into the root of this server, counted on the copies in that root; a cost that forfeits its own agenda, and a window for a scored agenda's ability; "this server" in a count of installs; an install barred from the root of the server a trashed card was in; and a Corp install added to HQ by its own text (`claude/serene-einstein-6bhlig`, 4 October 2026).
- **Stage 5** — break triggers and the first trace: seven cards and GameNET's Sweep deck, Pay to Win, with no new `Effect` and none left unused (Scapenet's trace is `Trace`'s first card): a card's ability making a player spend or lose credits is a moment, a break says whether the subroutine was printed, and a run can be kept from being declared successful by a card used during it (`claude/serene-einstein-6bhlig`, 4 October 2026).
- **Stage 6** — four Criminal and Shaper cards on stealth credits, Boomerang's and Engram Flush's remembered choices and Gachapon's set-aside six, with two new `Effect`s (`RevealHand`, `Remember`): a card remembers a card or a card type it chose, reads it back as the ice it may be used on or the type its subroutines may trash, paying a card's own "you may pay" is using it, a run's end knows whether it was successful, and what is left set aside can leave the game (`claude/serene-einstein-6bhlig`, 4 October 2026).
- **Stage 7a** — lockdown: five operations that stay in the play area, active, until the Corp's next turn begins, with no new `Effect`: "no active lockdown" counted over the play area, a successful run on the chosen server or on one protected by ice, a steal priced off the agenda's counters, and a standing bar on breaking with anything but an icebreaker (`claude/serene-einstein-6bhlig`, 5 October 2026).
- **Stage 7b** — an encounter away from the run's position: Konjin and Ganked!, with no new `Effect` — `ForceEncounter` from inside an encounter or an access keeps what it interrupted on the run and returns to it, and an access ends when its card leaves by a card's text resolved above its decision (`claude/serene-einstein-6bhlig`, 5 October 2026).
- **Stage 8** — five cards and Earth Station's Sweep deck, Ground Control, with no new `Effect`, closing the set: an agenda's worth is asked of the copy and the stored score is the score; a replacement lands a stolen agenda with counters; a trigger heard in the Runner's score area; an additional cost to run, paid as the server is announced; and a trace's success that parks a choice resumes the subroutines after it (`claude/serene-einstein-6bhlig`, 5 October 2026).

### 7. Downfall — 65 cards (C 19 / V 28 / M 18)

**Decks:** Sweep decks on its four identities.

**Re-read at Stage 1** (5 October 2026, against the DSL at Uprising's
close). Of the seven "composes", three needed a word each and no `Effect`:
a use limit per printed ability (The Artist, CR 9.3.6g), job and
connection columns in the turn log (Az McCaffrey), and a revealed card
trashed faceup (Stargate, CR 4.4.6b). The stage order below stands.

**Stages:**
1. **Runner, composes** (built, 5 October 2026): Isolation, Spec Work,
   Rezeki, Gauss, The Artist, Az McCaffrey, Stargate. Stage 1 is
   complete.
2. **Corp, composes** (built, 5 October 2026): Calvin B4L3Y, Nanoetching
   Matrix, CSR Campaign, Tiered Subscription, Red Level Clearance,
   Roughneck Repair Squad, Remastered Edition, Architect Deployment Test,
   Sandstone, SDS Drone Deployment (a `steal_cost` of `Cost::Trash`).
   Stage 2 is complete.
3. **Trigger words and bad-publicity removal** (built, 5 October 2026):
   Supercorridor, Fencer Fueno, Trickster Taka, Congratulations!,
   Demolisher, Bukhgalter, Storgotic Resonator, Masterwork (v37),
   Trebuchet, Increased Drop Rates. Stage 3 is complete.
4. **Amount, requirement and subtype words** (built, 5 October 2026): Lat,
   Sting!, Daily Quest, Fully Operational, Focus Group, Hagen,
   Vulnerability Audit, The Nihilist, Blueberry!™ Diesel. Game Over moved
   to Stage 6: its "for each card … the Runner may pay 3[credit] to
   prevent" is a loop of decisions over cards. Stage 4 is complete.
5. **Encounter and ice-state words** (built, 5 October 2026): Chisel,
   “Baklan” Bochkin, Pelangi, Afshar, Rime, Loot Box, Public Health Portal,
   Secure and Protect, Divested Trust. Rejig moved to Stage 6: its discount
   is the printed cost of the card its additional cost returned to the
   grip, which no cost hands to the effect it pays for. Stage 5 is
   complete.
6. **Triggers created by a played card, costs to run, interrupts** (built,
   6 October 2026): In the Groove, Climactic Showdown, Cold Site Server,
   Reduced Service, Game Over (a cost to prevent each trash, moved from
   Stage 4), Rejig (a card an additional cost moved, read by the effect it
   pays for, moved from Stage 5), Utae (X), Lucky Charm (an interrupt on
   "end the run"), Flip Switch (a jack-out, paid as a cost, and a lower
   trace base). Stage 6 is complete.
7. **Hidden information and new zones** (built, 6 October 2026): Hyoubu
   Institute (a reveal as a moment), Khusyuk (a set-aside count by printed
   install cost), The Class Act (an interrupt on a draw, not a prevention),
   Project Yagi-Uda (a swap out of HQ into a root as well as ice), Letheia
   Nisei, Saisentan. Stage 7 is complete.
8. **Naming a card, blanking an identity, memory across runs, action
   kinds** (built, 6 October 2026): Whistleblower, Complete Image, Direct
   Access, Always Have a Backup Plan, MirrorMorph. Stage 8 is complete, and
   Downfall with it — and so is Standard, whose last unbuilt cards were
   Downfall's.

**Riskiest:**
- Always Have a Backup Plan: the last ice of one run acted on in a second
  run.
- MirrorMorph: kinds of action in the turn log, and an extra action.
- Direct Access: both identities blanked for a run.
- The Class Act: the first replacement that is not a prevention.

**Banned:** Bukhgalter, Rezeki, Sting!.

**Closed stages** — one line each; the record is in [the archive](archive/nsg-card-pool.md) under the same heading.

- **Stage 1** — seven Runner cards and Az McCaffrey's Sweep deck, Moonlighting, with no new `Effect`: a once-per-turn use is the printed ability's, the turn log counts job and connection resources, and a card a selection revealed is trashed faceup (`claude/serene-einstein-6bhlig`, 5 October 2026).
- **Stage 2** — ten Corp cards, composed, with no new `Effect` and no change to the engine; SDS Drone Deployment is the first steal cost that takes a card (`claude/serene-einstein-6bhlig`, 5 October 2026).
- **Stage 4** — nine cards and Lat's Sweep deck, Level Pegging, with one new `Effect` (`Repeat`): three amounts, an action-phase rez, the Runner's successful servers last turn, an agenda that forbids its own score, and a card returned to the bottom of the stack (`claude/serene-einstein-6bhlig`, 5 October 2026).
- **Stage 8** — five cards and MirrorMorph's Sweep deck, Endless Loop, with two new `Effect`s (`ChooseCardName`, `StealAccessedCard`): a card name is chosen among the pool's playable cards and written into what follows, "repeat this process" resolves the choice again, both identities lose their abilities for a run, a run remembers the last ice it encountered for the next to bypass, a delayed ability can last a run, and the next action can be required to be a different one (`claude/serene-einstein-6bhlig`, 6 October 2026). **Downfall is complete: 65 of 65**, and Standard with it.
- **Stage 7** — six cards and Hyoubu Institute's Sweep deck, Open Book, with no new `Effect`: a reveal is a moment heard by whoever revealed, the Runner's draw is announced and parked so an interrupt resolves before it, a card from HQ may be swapped into a root as well as for ice, a chosen number reaches the amounts and filters it is read in, and the run moves to the outermost position of the server it is on (`claude/serene-einstein-6bhlig`, 6 October 2026).
- **Stage 6** — nine cards with one new `Effect` (`ForEach`): a delayed ability waits for any moment of the turn, once or every time; an upgrade's run cost counts its counters; a cost returns an installed card to the grip and the event reads its printed cost; and two interrupts, on a Corp card's "end the run" and on a trace's base strength (`claude/serene-einstein-6bhlig`, 6 October 2026).
- **Stage 5** — nine cards with no new `Effect`: the encountered ice gains a subtype for the encounter, a piece of ice rezzed whenever a non-ice card could be on a run against its server, an install from a card's text limited to a central, and the card a trigger heard named in a choice parked behind a paid one (`claude/serene-einstein-6bhlig`, 5 October 2026).
- **Stage 3** — ten cards with no new `Effect`: a trash cost asked of any card being accessed, a surcharge on an install, hosted credits for the rest of a successful run and for programs during runs, and a requirement on the trashed card's faction (`claude/serene-einstein-6bhlig`, 5 October 2026).

### 8. The reprint packs and the Core Set's remainder — 159 cards (C 95 / V 45 / M 20)

System Update 2021 (68 unbuilt), Salvaged Memories (17) and the Magnum
Opus Reprint (6), then the 68 Core Set cards no reprint pack carries
(C 60 / V 5 / M 3, surveyed 6 October 2026 at `3cec1a7`, below). All are
FFG designs, and none is in Standard. The tranche closes with a `core`
gate (`every_core_set_card_is_implemented_or_explicitly_excluded`, a
`CORE_UNIMPLEMENTED` list that only shrinks, and `core` in
`pool_status.py`'s gated packs), so every set the catalog embeds is
gated and Eternal's in-catalog count reads complete.

**Decks:** Sweep decks on the eight System Update 2021 identities not yet
built. The Core Set's cards go into the Sweep decks the seven Core
identities already head (Pay As You Go, Hit List, Safety Net, A Thousand
Cuts, Retirement Package, Paid Content, Hostile Bid), each card a swap
for one other decks still carry.

The survey's eleven stages, in order (the Core Set's cards join the first
nine by kind):
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
10. **Expose** (CR 1.21.4): Infiltration, Lemuria Codecracker and Zaibatsu
    Loyalty, from the Core Set.

Most of what this tranche needs has been built by the time it arrives; the
exceptions are Magnet, abilities that work from Archives or the heap, and
expose.

**The Core Set's remainder** — read against the DSL at `3cec1a7`, where
most of what these 2012 cards print has since been built for a later
card:
- **C (60), each joining the reprint stage of its kind:**
  - Runner economy and rig: Access to Globalsec, Akamatsu Mem Chip,
    Armitage Codebusting, Bank Job (an access replacement, as Stargate's),
    Data Dealer, Desperado, Easy Mark, Grimoire (Cookbook's "it"), Magnum
    Opus, Rabbit Hole, The Personal Touch (`Scope::Host`), Wyldside.
  - Runner events: Déjà Vu, Demolition Run (an access ability on the
    run's event), Modded, Special Order, Tinkering (a subtype gained for a
    duration).
  - Breakers and a virus: Aurora, Battering Ram, Ninja, Pipeline, Yog.0;
    Datasucker (a lingering −1 strength for the encounter).
  - Corp ice: Cell Portal (`MoveRunToOutermost`, a jack-out, a derez of
    itself), Data Mine, Data Raven (an encounter's choice and a hosted
    power counter's ability), Hadrian's Wall and Shadow (advanceable,
    Ice Wall's strength), Heimdall 1.0, Ichi 1.0 and Viktor 1.0 (the
    bioroid click break), Hunter, Matrix Analyzer, Neural Katana, Wall of
    Thorns.
  - Corp assets, upgrades and agendas: Adonis Campaign (Daily Casts'
    counter pool), Aggressive Secretary, Ghost Branch and Project Junebug
    (ambushes paid on access, as Snare!), Akitaro Watanabe and
    Experiential Data (`Scope::IceProtectingThisServer`), Melange Mining
    Corp., Red Herrings (persistent, an additional cost to steal),
    Research Station, Security Subcontract (`Cost::Trash`); Accelerated
    Beta Test, AstroScript Pilot Program, Breaking News, Posted Bounty,
    Priority Requisition, Private Security Force.
  - Corp operations: Aggressive Negotiation, Anonymous Tip, Beanstalk
    Royalties, Closed Accounts, Neural EMP, Precognition (VP's arranging
    of R&D), SEA Source (a trace), Shipment from Kaguya, Shipment from
    MirrorMorph.
- **V (5), to stage 5 or stage 6:**
  - Chum: a delayed ability on the next encounter, with a strength boost
    for that ice and "if the Runner did not fully break that ice".
  - Crypsis: "if you used this program to break a subroutine during that
    encounter", heard as the encounter ends.
  - Djinn: hosted programs whose memory does not count against the limit
    (a `ContinuousKind` beside `MayHost`).
  - Stimhack: damage that "cannot be prevented" (CR 9.3.3g), the first
    in the pool.
  - Wyrm: a break limited to ice of strength 0 or less.
- **M (3), a stage of their own after stage 9: expose** (CR 1.21.4).
  Infiltration, Lemuria Codecracker and Zaibatsu Loyalty. To expose a
  card is to reveal an installed, unrezzed card, and the reveal is a
  moment, which Stage 7 of Downfall built. Zaibatsu Loyalty is the
  interrupt that has waited on it since the Prevention Rule (a
  `Preventable::Expose` and a `WouldHappen` arm).

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
`pool_status.py` replaces this with a join on `numeric_id`, and Stage 0d's
with a join on the card id.
