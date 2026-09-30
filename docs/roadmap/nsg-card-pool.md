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
| 4 | Parhelion (`ph`) | 63 | 27 | 36 | **Stage 4 next** — Runner standing and breaker words: Basilar Synthgland 2KVJ, Dr. Vientiane Keeling, K2CP Turbine, Tremolo, Time Bomb, Poison Vial, WAKE Implant v2A-JRJ, Abaasy |
| 5 | Midnight Sun (`ms`) + Booster (`msbp`) | 65 + 7 | 0 | 65 + 7 | not started |
| 6 | Uprising (`ur`) + Booster (`urbp`) | 65 + 7 | 0 | 65 + 7 | not started |
| 7 | Downfall (`df`) | 65 | 0 | 65 | not started |
| 8 | System Update 2021 (`su21`), Salvaged Memories (`sm`), Magnum Opus Reprint (`mor`) | 82, 18, 6 | 11, 1, 0 | 71, 17, 6 | not started (the built ones are Core reprints) |

**DSL ratio** (`pool_status.py`, the DSL Growth Rule's number): **17 of 91
`Effect` variants single-use, 1 unused (`Trace`), over 399 card files** (29
September 2026). The baseline at Stage 0a was 26 of 70 single-use, 3 unused,
over 184 files: 215 cards later the single-use count is *lower*, because the
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

Both are closed; see [Progress](#progress) for what is next.


**Closed** — the record is in [the archive](archive/nsg-card-pool.md) under the same headings:

- **Stage 0a** — the catalog from NetrunnerDB by script (`scripts/catalog_sync.py`), fifteen packs embedded, a completeness gate per pack, `CARD_VOCAB` grown once to 1024 with a rank reserved per pack, and `scripts/pool_status.py` (`feat/nsg-pool-stage-0a`, 26 September 2026).
- **Stage 0b** — formats from NetrunnerDB (Standard, Startup, Eternal, Casual), a card banned in one and legal in another, a deck's legality judged per format on its printings, in both clients (`feat/nsg-pool-stage-0b`, 26 September 2026).

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
| Credits spendable only from stealth cards (a restriction on the *payer*, the reverse of `PaysFor`) | VP: Corsair, Lampades, Baker | UR: Mu Safecracker, Afterimage, Penrose | CR 1.10.4b | VP 6b |
| Additional subroutines, ordered | VP: Stick and Poke | RWR Thunderbolt Armaments; TAI Starlit Knight; MS Echo, Envelopment; UR Winchester | CR 9.8.2, CR 9.8.3, CR 6.5.7d | VP 7e; RWR 8b (for the run); TAI 8a (by count) |
| Arrange | VP: Cultivate, Knowledge Seeker | RWR Cataloguer; TAI Federal Fundraising | CR 8.3.1 | VP 6a |
| Reveal as a step other cards can read | VP: Esca, Perfect Recall, Tocsin | RWR Burner, Bring Them Home | CR 1.21.3 | VP 6a; RWR 6b (stays revealed) |
| Abilities active outside play (Expendable from HQ; from Archives or the heap) | VP: Tocsin | RWR Eminent Domain, Descent; TAI Slash and Burn Agriculture, Tree Line, Angelique; reprints Subliminal Messaging, Crowdfunding | CR 9.1.8b | VP 7f; RWR 7a; TAI 7 |
| A card added to a score area "as an agenda" | VP: Word on the Street, Myōshu | RWR Jeitinho, Kingmaking; PH Nightmare Archive; MS Regenesis, Backroom Machinations | CR 1.17.3f | VP 7d; RWR 8c (the Runner's) |
| Hosting in general: install onto a card, facedown hosted cards, host limits | VP: Hackerspace, Read-Write Share, Luana Campos | RWR Spree; PH Matryoshka | CR 1.13.5, CR 1.13.7 | VP 7b, 7c; RWR 3d (onto a host); TAI 4 |
| Runner cards removed from the game | VP: Take a Dive, Kompromat | TAI Capybara; PH Nanuq; UR Devil Charm, The Back, Buffer Drive | CR 4.9 | RWR 2b (`RemoveFromGame`); TAI 7 |
| Additional costs imposed by another card (steal, score, run, trash) | VP: Magistrate Revontulet | RWR Sebastião Souza Pessoa; TAI Daniela Jorge Inácio; MS Azef Protocol; UR NAPD Cordon, Earth Station: SEA Headquarters; DF Cold Site Server, Reduced Service | CR 1.16.10, CR 6.3.2b | VP 3a (steal, score); RWR 2a (trash); TAI 8b |
| Terminal: the action phase is forced to end | RWR: Active Policing, Bring Them Home | TAI Oppo Research; MS Big Deal | CR 5.4.3 | RWR 6a |
| Psi game: a simultaneous secret bid | RWR: See How They Run | TAI Adrian Seis; UR Konjin, Hyoubu Precog Manifold | CR 10.14.6 | RWR 7b; TAI 8b |
| Set aside | RWR: The Wizard’s Chest | PH Spark of Inspiration; MS Deep Dive; UR Gachapon | CR 4.8 | RWR 6d (faceup only) |
| X costs | RWR: Lobisomem | PH Matryoshka; DF Utae; reprints Corporate Troubleshooter, Psychographics | CR 1.16.2c | RWR 6e |
| Forced or repeated encounter | RWR: Sisyphus Protocol | UR Konjin, Ganked! | CR 6.1.3 | RWR 7d |
| Losing abilities | PH: Hush, Klevetnik | MS Light the Fire! | CR 9.1.9a | — |
| Break restrictions ("cannot be broken", "only by …") | PH: Anvil, Unsmiling Tsarevna, Hafrún | MS Trieste Model Bioroids; UR Akhet, NEXT Activation Command | CR 9.8.5 | — |
| Charge | PH: Flux Capacitor, Orca | MS Captain Padma Isbister, Rigging Up, “Daeg, First Net-Cat”, Stoneship Chart Room | CR 10.10 | — |
| Mark | PH: Tunnel Vision, Info Bounty | MS Nyusha "Sable" Sintashta, Carpe Diem, Virtuoso, Backstitching | CR 10.11 | — |
| An agenda's points or advancement requirement changing | VP: Let Them Dream | PH Ontological Dependence, Freedom of Information, Regulatory Capture; UR Megaprix Qualifier, Project Vacheron; reprints Project Beale, SanSan City Grid | CR 3.2.2, CR 3.2.3b | VP 3a (points); PH 3a (requirement, a card's own) |
| A choice remembered for a duration (a server, an ice, a subtype, a card's name) | RWR: Lycian Multi-Munition | MS Trieste Model Bioroids; UR Boomerang, Engram Flush; DF Whistleblower, Complete Image, Saisentan; reprints Femme Fatale, Security Testing, Chameleon | CR 9.10.3 | RWR 8a |
| A triggered ability created by a card that resolved ("when your next run ends…") | DF: In the Groove, Climactic Showdown, Always Have a Backup Plan | reprints Inside Job, Test Run | CR 9.10 | RWR 5d in part (a delayed conditional ability); DF's cards — |
| Lockdown | UR: SYNC Rerouting, Argus Crackdown, NAPD Cordon, NEXT Activation Command, Hyoubu Precog Manifold | — | CR 3.5.1c | — |

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

- **Blood in the Water** (Midnight Sun): prints its advancement requirement as X, which NetrunnerDB records as none; the face draws no circle for it until the stage that builds a variable requirement (`card_face`'s layout test names the card).
- **Bloop** (PH 3c): rezzed by a card's text (Send a Message, Mycoweb), it is rezzed without its additional cost — `engine::rez_install` never reads `rez_alternatives` (the engine bug named in `ROADMAP.md`), so no harmonic ice is derezzed.
- **Hostile Architecture** (PH 2b): a *rezzed* copy trashed by the Runner's card text is not heard — `GameEvent::CardTrashed` does not say whether the card was rezzed, and adding that to its 42 sites is more than one card wants. Charm Offensive can reach it; no game has.
- **Hafrún** (PH Stage 8): two ice types, which `CardType::Ice(IceType)` cannot say.
- **Nova Initiumia, Ampère** (PH Stage 5): change deck-building rules the validator does not model (`deck_limit` only).
- **Docklands Pass, Rotary**: "whenever you breach HQ or R&D" is a successful run on either; a breach without a run (Cataloguer, RWR 6c) does not fire them.
- **Manuel Lattes de Moura**: the extra access is heard at the run's success, as every "when you breach" in the pool is, so a run Flagship keeps from being declared successful gets none.
- **Kessleroid**: "cannot trash" is unenforced.
- **Embedded Reporting**: does not shuffle. **Next Big Thing** from the Runner's score area is unmodelled. **Detente**'s access outside a run is unmodelled.
- **Luana Campos** (VP 7a): "uninstalled" is announced for a Corp card only; no Runner card in the pool interrupts its own uninstalling (Nanuq, PH Stage 6, will be the first, and its words are a replacement). The interrupt resolves in the dispatch that announced it.
- **Shred**: intercepts `EndTheRun` only (a lingering `PreventRunEnding`).

### Recorded deviations from the Comprehensive Rules

Each is also a note on its section's row in [rules-conformance.md](rules-conformance.md).

- **A use limit is spent when a trigger fires, whether its "may" is taken or not** (CR 9.3.6g says a declined ability is not used): Zahya Sadeghi, Shackleton Grid, Malandragem, Heliamphora, Brasília Government Grid. `TriggeredEffect::requirement` is consumed at fire; the 9.3 row carries it.
- **"The Corp trashes" is read as the controller's trash** (CR 1.14): Noise, Heliamphora.
- **A terminal card's end does nothing mid-run or with a window open** (CR 5.4.3b): Active Policing, Bring Them Home; no terminal in the pool resolves anywhere but its player's action window. Nuvem SA's "finish resolving" is announced after the action phase has ended, not while it is ending.
- **Arranged cards are not made new objects** (CR 8.3.3): Cultivate, Knowledge Seeker — nothing in the engine remembers a card in R&D by identity. The client does not yet word an arrangement as "top first".
- **Hosted cards are a list in the order hosted**, not "distinct groups … freely arranged" (CR 1.13.7d): Read-Write Share.
- **Baker's credit is paid in the last paid-ability window before the approach** (CR 6.9.4e), not at it, so the Corp acts in that window after the Runner has chosen.
- **A breach with no run that ends the game mid-access** still records a `RunCompleted`, pushed and not dispatched (Cataloguer); the breach is shown with the run's panel and trail.

### Approximations whose outcome is the printed one

- **Two of N as two choices of one**, each resolved before the next (CR 9.12.2c names realloc()): realloc(), Chain Reaction's "trash 2", Logjam's counters (heard as two placements), Shipment from Vladisibirsk's four picks. Editorial Division shuffles R&D when its search is declined; its "total" discount is never asked — the Corp is given the division that costs least.
- **A zone is chosen before the card**: Sleipnir ("from HQ or Archives"), Let Them Dream ("HQ, R&D or Archives"), Muse (one zone's programs at a time, where the rules search all three; a hosted program takes memory as any does), Lethe (top or bottom before the card).
- **"Up to N" takes N where fewer is never better**: Ansel 2.0 breaks two when two are left; Shackleton Grid's "may" always fires (declining 4 meat damage is never better).
- **Order of two abilities from one card is the printed order, not the player's**: Privileged Access's two "when you take a tag" abilities; Malandragem's two offers. Privileged Access's installs are conditioned on being tagged *after* the tag, so a Decoy preventing the event's tag on a Runner tagged earlier in the run still lets them happen.
- **Heliamphora's "instead" is an access, then a host**: the card's own "when accessed" in Archives still resolves and it counts as accessed. A true interrupt would park the access before it began.
- **Alarm Clock's run** is asked for among the Runner's other "when your turn begins" abilities and resolved when chosen; the Corp's rez window of 5.7.1e comes once, after the run.
- **Hearts and Minds** chooses its source before its destination, so an advanceable card may be picked as both — a move onto itself.
- **Pressure Spike's pumps last the encounter**, as every breaker's does here. **Corporate Hospitality** excludes only the copy resolving, found as the last faceup copy in Archives.
- **Aircheck's event is active for the run it makes and no longer**; the second run is ordinary, and the event's unspent credits are gone with the first.
- **Vera Ivanovna Shuyskaya's reveal** is not a `CardRevealed` of each grip card; the Corp sees the grip through the selection and only the trashed card is announced. No card in the pool reads a reveal of the grip.
- **Business As Usual's virus counters are the Runner's cards'**: no Corp card in the pool hosts one.
- **Tocsin** is the pool's only expendable whose ability is an action, so it is the one announced; an expendable ability that is not an action announces nothing.
- **Nihilo Agent's "load … when it is empty, trash it"** is three counters and a self-trash after its own removal; nothing else in the pool removes its counters.
- **Meeting of Minds** offers every card of the subtype in the grip and lets the Runner pick any number, a reveal of none included.
- **Hiram Svensson**: damage takes its cards as discards (`CardDiscarded`), so a hardware lost to damage the Runner is responsible for is not heard. **Hiram, ezaM, MuslihaT**: the look is in the Runner's log, not their view (MuslihaT's is shown only when it matches), so a bot's sample does not keep the card on top of R&D.
- **System Gateway and Elevation** (Phase 1 §8, the audits): Scrounge's optional half chains through `then`; Bling covers basic actions, not ability installs; Touch-ups' type choice is blind; two Mycowebs can loop; Scatter Field's strength is fixed at encounter; a determinized run lacks `initiated_by` and `ice_bypassed`; Mutual Favor's unaffordable found breaker installs to a no-op rather than being withheld; Conduit's counter is placed at run success rather than run end; Tāo Salonga's and Ballista's do-nothing options are offerable and no-op; Karunā's jack-out is recorded as `RunEndedByEffect`, as is Account Siphon's replaced breach; Snare!'s "must reveal it" in R&D is implicit in the access model; The Maker's Eye's access bonus timing likewise; a bare "+1 strength" (Corroder) is encounter-long by Null Signal Games' own default, unlike Gordian Blade's run-long — not an approximation, noted so nobody "fixes" it.

### Bot debts — cards the heuristic never plays (Phase 5 §25's list)

The heuristic seats in the sweeps and `coverage_identical.py` reach these cards through random seats and their tests alone. Each is a blindness of the evaluator, not of the engine, and is the raw material for the precepts work. **Since Phase 5 §25 Stage 1 (29 September 2026) the list is measured, not kept by hand:** `netrunner_cli diag precepts --deck-styles --games 192 --report …` ends with every card a seat used over the pass and the cards in the decks played that no seat ever used (`reach.unused_in_pass`), for the format `--format` names. The entries below are the hand list as it stood, kept for the *reasons* they record (which the report cannot say); a card the report names that is not here is a new debt, and a card here the report no longer names has been paid.

- **Economy resources and programs it does not value**: Friend of a Friend, Valentina Ferreira Carvalho, Coalescence; Laser Pointer, Banner; Monkeywrench, Saci, Pichação, Urban Art Vernissage; Lago Paranoá Shelter; AirbladeX (JSRF Ed.); the Core Set interrupts Decoy, Net Shield and Sacrificial Construct.
- **Abilities it never uses**: M.I.C.'s trash, Arissana Rocha Nahu's, Epiphany Analytica's counter; identity and multi-click abilities generally (Phase 1 §8); over-advancing for Dividends.
- **Corp cards it never plays or rezzes**: Distributed Tracing, Shipment from Vladisibirsk, Nonequivalent Exchange (played only by random seats), Hostile Architecture (installed 90 times, never rezzed); it never trashes Amanuensis or Privileged Access, never purges (Malandragem, Physarum Entangler).
- **Heap installs, hosted credits and hardware** (Phase 1 §8); the Corp undervalues paying for Byte! (Rules Audit, Masking).
- **A sample does not carry a copy's turn counts** (`determinize` leaves `CopyTurn` empty), so a sample of a Cloud Eater rezzed this turn does not see its encounter-end ability coming.
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
   names) **and Corp words**, split by mechanic when it was taken (30
   September 2026): **3a**, the requirement (built): Ontological
   Dependence, Freedom of Information, Regulatory Capture; **3b**, the
   Corp words (built): Simulation Reset, Hypoxia, Mr. Hendrik; **3c**, harmonic
   ice (built): Pulse, Bloop. Stage 3 is complete.
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

**Closed stages** — one line each; the record is in [the archive](archive/nsg-card-pool.md) under the same heading.

- **Stage 1** — ten Corp cards, composed (`feat/ph-stage-1-corp-composes`, 29 September 2026).
- **Stage 2a** — seven Runner cards and a Weyland operation, composed (`feat/ph-stage-2a-runner-composes`, 29 September 2026).
- **Stage 2b** — the Runner's trash of an installed Corp card, its own included (`feat/ph-stage-2b-hostile-architecture`, 29 September 2026).
- **Stage 3a** — the advancement requirement, asked (`feat/ph-stage-3a-advancement-requirement`, 30 September 2026).
- **Stage 3b** — the Corp words: "that many", an operation out of the game, and every click as a price (`feat/ph-stage-3b-corp-words`, 30 September 2026).
- **Stage 3c** — harmonic ice: a count of installs, and a derez to pay for a rez (`feat/ph-stage-3c-harmonic-ice`, 30 September 2026).

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
