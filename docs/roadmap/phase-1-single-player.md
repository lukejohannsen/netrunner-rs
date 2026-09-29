# Phase 1 — Single-Player Completeness (and Phase 1.5, Session Unification)

Area roadmap; the index is `ROADMAP.md`. Section numbers are addresses used by code comments ("ROADMAP Phase 1 §3", "Phase 1 §8 Stage 5") and never change.

**Where it stands (29 September 2026).** §1–§8 and Phase 1.5 are closed: *System Gateway* 77/77 and
*Elevation* 82/82 with all 28 published decks. **§9, the whole NetrunnerDB pool, is open** and planned
set by set in [nsg-card-pool.md](nsg-card-pool.md), which also carries every card's known limit
(Elevation's per-stage limits moved there). The closed record is in
[the archive](archive/phase-1-single-player.md).

## Closed — one line each

- **§1** — saved decks end to end: `DeckFile`, the deck store, `DeckCategory` with `matchups()` over `Sample` only.
- **§2** — the game state legible to a UI: `ClientView` and the masked log; the TUI draws the player's own hand (13 September 2026).
- **§3** — the rules gaps worth closing: purge, dynamic discard, simultaneous and player-ordered triggers, the post-action window, breakers need an encounter, unspent clicks lost, `numeric_id` on every card, Snare!, `InstallId`, and the memory-cost bug that had made no program installable.
- **§4** — engine hygiene: `ability::ResolutionContext` (the State Hygiene Rule); recurring credits deferred until Azimat.
- **§5** — format support: points, legality before match start, ban lists and rotation as mechanisms — **superseded 26 September 2026** by NSG pool Stage 0b, whose tables are NetrunnerDB's.
- **§6** — card data and ingestion from NetrunnerDB.
- **§7** — *System Gateway*, 77 of 77, behind `every_system_gateway_card_is_implemented_or_explicitly_excluded`.
- **§8** — *Elevation*, 82 of 82 in ten stages, 2–3 September 2026: the sweeps play `sweep_decks_for_seed`, the card gate demands every deck's cards at eight seeds, and the DSL ratio was watched stage by stage (29 of 74 single-use at the end).
- **Phase 1.5** — session unification: one `Session`, one `MAX_STEPS`, two seat kinds; "a run can outlive the game" fixed twice over.

## 8. Elevation Card Set — DONE; what the set left open

**Still open after the set (bot, not coverage):** the heuristic's card-specific blindnesses (heap installs, hosted credits and hardware, identity and multi-click abilities, over-advancing for Dividends); three cards reached only by their own tests (*Measured Response*, *Off the Books*, *Sericulture Expansion*) — Phase 3 §1 split this: search plays Measured Response, and never over-advanced because the evaluator zeroed the token (since fixed). The daemon's filler fixture is closed (Phase 4 §3).

## 9. The whole NetrunnerDB pool — OPEN (25 September 2026)

Asked for by the person: every card NetrunnerDB lists, expressed in the DSL, with the deck builder and deck legality working over the whole corpus. Today the engine plays three packs — *System Gateway* and *Elevation* complete (§7, §8), the *Core Set* catalog embedded with only some of its cards implemented — so Startup is the only format with a complete pool, and the deck builder's "Not playable yet" filter is most of any larger one.

What the item covers, so none of it is discovered halfway:

- **Cards, set by set, gated as §7 and §8 were.** Each set gets an `every_<set>_card_is_implemented_or_explicitly_excluded` gate over an `<SET>_UNIMPLEMENTED` list that only shrinks, and its printed values checked by `printed_values_agree_with_the_netrunnerdb_catalog`. The unit of merge stays "a deck is buildable". Order to be decided when the item is taken — Null Signal Games' Standard pool first is the likely one, because that is what a person arriving from jinteki.net will bring — and the older Fantasy Flight Games cycles after it.
- **The DSL has to scale, not grow a variant per card.** The DSL Growth Rule's ratio is the check at every stage; a set whose single-use variants approach its card count means a composition primitive is owed first. Mechanics the pool has and the engine does not (expose, traces started by a card, bad publicity removal, and whatever the older cycles print) are each a Rules Conformance re-read of the chapter before they are built. The Linked Clause Rule's quote gate applies to every new card file.
- **The catalog and the formats come from NetrunnerDB, not by hand.** `netrunner_card_sync` fetches the full catalog, the formats, card pools and restriction lists; `netrunner_core` stays I/O-free, so the data is embedded by `build.rs` as the three packs are now, with `fs-loader` unchanged. §5's seed tables (`allowed_packs`, `banned`, restriction points) are filled from that feed, and rotation by cycle — dropped in §5 because the pool was two packs — comes back with it. `no_shipped_format_restricts_anything_yet` is retired deliberately when it does.
- **The deck builder and legality over the whole pool.** Every format's legality judged on real card pools and restriction lists, the set filter and release order over every pack, and a pool of a few thousand cards that the browser and builder stay fast over.
- **What moves elsewhere, measured.** `CARD_VOCAB` (192) and the observation encoding grow, so a trained network is retrained (new cards ranked as a later wave where possible, so existing indices hold). Every new mechanic goes through both agent-driven sweeps and the coverage gate, with `DeckCategory::Sweep` decks for cards no published list reaches; published decklists for the new sets join the sample pool, which grows `decks::matchups()` and every per-card coverage run with it.

**Planned for the NSG sets (26 September 2026): [nsg-card-pool.md](nsg-card-pool.md).** The order is Vantage Point, which completes Startup, then Standard from newest to oldest, then the three reprint packs: 454 + 91 unbuilt cards. The decks are built here as `Sweep` lists, because NSG published none for these sets. The observation vocabulary is grown once in a Stage 0 that also brings the catalog and the formats in from NetrunnerDB. Stage entries are recorded there, not here.

