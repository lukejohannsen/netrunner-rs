# The Fantasy Flight Games pool, card by card (FFG plan Stage 0c)

One row per card the FFG plan builds, read against the DSL on `main` on
11 October 2026, in each tranche's stage order, with the tranche's new
mechanics after its table. The classes are C (composes), V (needs a
vocabulary word) and M (needs machinery), as in
[ffg-card-pool.md](ffg-card-pool.md#the-survey-stage-0c), which holds the
stages, the decks and the mechanics shared across tranches. A class is one
reader's judgement, and the first stage of each tranche re-reads it.

## F1. Revised Core Set — 38 cards (C 32 / V 6 / M 0)

| Card | Class | What it needs |
|---|---|---|
| Force of Nature | C | Paid break of 2 code gate subroutines plus a strength pump (Corroder's shape) |
| Morning Star | C | `SubroutineBreakCount::All` on barriers (Orca's "any number") |
| Peacock | C | Code gate break plus a +3 pump, as Gordian Blade |
| Faerie | C | Crypsis's `OnEncounterEnded` + `BrokeASubroutineThatEncounter`, trash self |
| Darwin | C | `ContinuousKind::Strength` from `HostedCounters(Virus)`; turn-start `OfferPaidChoice` for a counter; printed X strength |
| Hemorrhage | C | `OnSuccessfulRun` counter; `Cost::RemoveCounters` + Corp-side `PromptChooseCards` from HQ (The Nihilist's "Corp trashes") |
| Dinosaurus | C | Djinn's `MayHost`/`HostsMemory`, `Scope::Hosted` strength, console limit |
| Dyson Mem Chip | C | `ContinuousKind::Memory` + `Link` (The Toolbox) |
| Chaos Theory: Wünderkind | C | Identity with `ContinuousKind::Memory` |
| All-nighter | C | `Cost::Clicks` + `TrashSelf`, `GainClicks` (Running Hot) |
| Doppelgänger | C | `OnRunEnded` + `LastRunSuccessful`, `OncePerTurn`, a "you may run" (Always Have a Backup Plan) |
| Fall Guy | C | `Prevent(Preventable::Trash)` interrupt (Sacrificial Construct); `TrashSelf` for 2 credits |
| HQ Interface | C | `OnBreach` HQ + `AddAdditionalAccess` |
| Mr. Li | C | Draw 2, then a selection over the grip's `TopOf(2)` to the bottom (Daily Business Show's "those cards") |
| Notoriety | C | Deep Dive's three `TimesThisTurnWhen` requirements; `AddToScoreAreaAsAgenda` (Word on the Street) |
| Singularity | C | `additional_play_cost` click, remote `PromptChooseServer`, `SetAccessReplacement` trashing `AttackedServerRoot` |
| Underworld Contact | C | `OnTurnStart` with `AmountAtLeast(Link, 2)` |
| Caduceus | C | Two `Trace` subroutines (Data Raven, Matrix Analyzer) |
| Flare | C | `Trace` → trash hardware, `Unpreventable(DealDamage Meat 2)`, `EndTheRun` |
| Himitsu-Bako | C | Paid ability (1 credit): add this ice to HQ (`AddSelfToHq` words) |
| Hive | C | MS 8a's `ContinuousKind::Subroutines`, negative by `AgendaPoints(Corp)` |
| Yagura | C | `LookAtTopOfDeck` + optional `AddToDeck(Bottom)`; net damage subroutine |
| Haas-Bioroid: Stronger Together | C | `ContinuousKind::Strength` with `Scope::Ice(HasSubtype(Bioroid))` |
| Ash 2X3ZB9CY | C | `OnSuccessfulRun` this server, `Trace` → `Prohibit(AccessOthers)` bound to this install (Adrian Seis) |
| Braintrust | C | Score counters by `Amount::Every` past 3 (Project Atlas); `RezCost` on `Scope::Ice` by hosted counters |
| Dedicated Response Team | C | `OnRunEnded` + `LastRunSuccessful` + `IsTagged`, 2 meat damage |
| Elizabeth Mills | C | `OnRez` `RemoveBadPublicity`; `[click]`+`TrashSelf`: trash a location resource, take bad publicity |
| Green Level Clearance | C | Gain 3, draw 1 (Red Level Clearance's parts) |
| GRNDL Refinery | C | Advanceable asset; `TrashSelf` + credits by `last_known` advancement tokens |
| Project Ares | C | Runner-side selection of own installs, count past 4 (Project Yagi-Uda's counting), then `EffectIf` bad publicity |
| Strongbox | C | `ContinuousKind::StealCost` (`StealingFromThisServer`, a click) + `persistent_after_trash` |
| TGTBT | C | `RevealedWhileAccessed` (Nightmare Archive) + `OnAccessed` give tag |
| Bernice Mai | V | `Effect::Trace` "if unsuccessful" branch (trash this) |
| Spinal Modem | V | `Trigger` on `TraceSuccessful` during a run; `PaysFor::Using(Icebreaker)` and recurring credits exist |
| Hudson 1.0 | V | Lingering "cannot access more than 1 card" for the run; Eli 1.0's runner-only click break |
| Whirlpool | V | `Prohibition` on jacking out for the run |
| The Cleaners | V | `IncreaseAboutToResolve` over parked meat damage (draw only today) |
| Pheromones | V | Recurring credits as an `Amount` (hosted virus counters); `PaysFor` "during runs on HQ" |

**New mechanics (M):**

None. Every card is C or V. The nearest things to machinery are listed so the first stage can re-read them:

- **A trace's failure, and its success as a moment** (V, not M: `GameEvent::TraceAvoided` and `TraceSuccessful` are already emitted, and no trigger hears them). Cards: Bernice Mai, Spinal Modem. CR 10.8.5, CR 10.8.6e. Near relative: `Effect::Trace` (Core Set Stage 11f's tracer ice, UR Stage 5's Scapenet). This is not a row in "Mechanics shared across tranches".
- **Parked damage made larger** (V: a widening of `IncreaseAboutToResolve`, which today grows only a parked draw). Card: The Cleaners. CR 1.3.9, CR 9.9.7e. Near relative: the Prevention Rule's window and Daily Business Show's `IncreaseAboutToResolve` (Tranche 8, Stage 9b).

## F2. Reign and Reverie — 58 cards (C 41 / V 10 / M 7)

| Card | Class | What it needs |
|---|---|---|
| Bankroll | C | `OnSuccessfulRun` may `AddCounters(Credit)`; `TrashSelf`: `TakeAllCountersAsCredits` |
| Cradle | C | `ContinuousKind::Strength` by `CardsInHand` (negative); any-number code gate break |
| Tycoon | C | Faerie/Crypsis `OnEncounterEnded` + `BrokeASubroutineThatEncounter`, Corp gains 2 |
| Ika | C | Trojan plus Hush's "host this program on another piece of ice" ability |
| Kyuban | C | Trojan `OnIcePassed` with `EventFilter::Host`, gain 2 |
| Guinea Pig | C | Selection of the whole grip to the heap (count `InZone`), gain 10 |
| Hot Pursuit | C | Run HQ, on success gain 9 and `GiveTags` |
| Office Supplies | C | `ContinuousKind::PlayCost` by `Amount::Link`; `PresentChoice` credits or cards |
| Insight | C | `additional_play_cost` click; Corp arranges top 4 (Cultivate's arrange), then reveal |
| Paragon | C | First successful run each turn: gain 1, `LookAtTopOfDeck` stack, optional bottom; `Memory` |
| Akiko Nisei: Head Case | C | `OnBreach` R&D, `PsiGame` → `AddAdditionalAccess` |
| Liza Talking Thunder: Prominent Legislator | C | `OnSuccessfulRun` `ServerKind(Central)`, `first_each_turn`, draw 2 and take a tag |
| Nathaniel "Gnat" Hall: One-of-a-Kind | C | `OnTurnStart` with a grip count of at most 2 (`CardsInHand`) |
| Algernon | C | Turn-start `OfferPaidChoice` gain [click]; `LaterThisTurn` trash unless `TimesThisTurn(OnSuccessfulRun)` |
| Hijacked Router | C | `OnServerCreated` Corp loses 1; Archives success may `TrashSelf`, Corp loses 3 |
| Miss Bones | C | 12 hosted credits, trash when empty, `PaysFor::TrashCosts` of installed cards (Paricia) |
| Psych Mike | C | `OnRunEnded` R&D success, `first_each_turn`, credits by `CardsAccessedLastRun` |
| Blockchain | C | `ContinuousKind::Subroutines` before its others, `Every(2)` faceup transactions in Archives |
| Hydra | C | `EffectIf IsTagged` per subroutine |
| Meridian | C | `OfferPaidChoice` with `Cost::AddToScoreAreaAsAgenda` (−1 point), otherwise gain 4 and end the run |
| Peeping Tom | C | `OnEncounter`: `Remember(CardType)`, `RevealHand(Runner)`, `GainSubroutine` for the run, counted by `OfChosenCardType` |
| Otoroshi | C | `PlaceAdvancementCounters` on a remote root card, then `OfferPaidChoice` or `Access` |
| Thimblerig | C | `OnTurnStart` + `OnIcePassed` (this) may `SwapInstalledIce` |
| Attitude Adjustment | C | Draw 2, reveal up to 2 agendas in HQ/Archives (zone first), credits by `CardsSelected`, `ShuffleIntoDeck` |
| Building Blocks | C | Reveal a barrier from HQ, install and rez ignoring costs (Accelerated Beta Test) |
| Divert Power | C | Selection of rezzed cards to `DerezCard`, then `RezInstalled` discounted 3 per `CardsSelected` |
| Game Changer | C | `GainClicks` by agendas in the Runner's score area; removes itself from the game (Labor Rights) |
| Hangeki | C | `TimesLastTurn(OnCardTrashed)` Corp card; `Access` by the Runner's choice; else `AddToScoreAreaAsAgenda` (−1) in the Runner's |
| Too Big to Fail | C | `Not(CorpCreditsAtLeast 10)`; gain 7, bad publicity |
| Under the Bus | C | `TimesLastTurn(OnAccessed)`; trash a connection, take bad publicity |
| API-S Keeper Isobel | C | `OnTurnStart` may `RemoveAdvancementCounters` from an install, gain 3 |
| Arella Salvatore | C | `OnAgendaScored` from this server (`AgendaCameFromThisCardsServer`), `PromptInstallCorpCard` free, `PlaceAdvancementCounters` |
| Broad Daylight | C | Optional bad publicity, counters by `Amount::BadPublicity`; `OncePerTurn` click+counter: 2 meat damage |
| Fly on the Wall | C | `OnAgendaScored` this, give a tag |
| Giordano Memorial Field | C | Cayambe Grid's "end the run unless they pay 2 for each", counted over the Runner's score area |
| Hyperloop Extension | C | Scored or stolen: Corp gains 3 |
| Jumon | C | `When your turn ends` (Adrian Seis), `PlaceAdvancementCounters` 2 on a remote root card |
| Neurostasis | C | `CorpPaysToApply` 3 on access; select Runner installs by `HostedAdvancementTokens`, `ShuffleIntoDeck` |
| SIU | C | `OnTurnStart` may `TrashSelf` to `Trace(3)` → tag |
| Sportsmetal: Go Big or Go Home | C | `OnAgendaScored`/`OnAgendaStolen`, `PresentChoice` gain 2 or draw 2 |
| The Outfit: Family Owned and Operated | C | `OnBadPublicityTaken`, gain 3 |
| District 99 | V | `CardFilter` "matches your identity's faction"; `OnCardTrashed` program/hardware `first_each_turn` |
| Mâché | V | `Amount` for the triggering card's printed trash cost (`OnTrashedFromAccess`) |
| Thunder Art Gallery | V | `Trigger` for a tag avoided (prevented), beside `OnTagRemoved`; discounted grip install |
| Mind's Eye | V | A breach that excludes the root (`Breach(RnD)` with no root candidates); power counters on R&D success |
| Divide and Conquer | V | The same root-less breach, HQ then R&D after Archives' breach |
| Gatekeeper | V | A "rezzed this turn" condition on a `Strength` effect; subroutine reveal-and-shuffle composes |
| Drudge Work | V | `Amount` for a selected card's printed agenda points; power counters, trash when empty |
| Lady Liberty | V | `CardFilter` "agenda points exactly N" (`HostedCounters`); add an agenda from HQ to the score area |
| Saraswati Mnemonics: Endless Exploration | V | A remembered install that cannot be scored or rezzed until the next turn (`Remember` + `Prohibition`) |
| Fast Break | V | A text install of up to X cards bound to one chosen remote (`ChooseServer` + `PromptInstallCorpCard`) |
| Eavesdrop | M | Condition counters: an operation installed onto ice with text of its own |
| Acme Consulting: The Truth You Need | M | A tag the Runner is considered to have, only during some encounters |
| Formicary | M | Ice moved to the innermost position of another server mid-run, the Runner moved to it and encountering it |
| Daruma | M | A swap of a root card with another root or with a card in HQ, then a jack-out at the server's approach |
| Patchwork | M | An interrupt on a play or install that pays part of its cost with a grip trash |
| Reboot | M | Runner cards installed facedown (from the heap) |
| DJ Fenris | M | An identity brought from outside the deck, hosted, its text gained |

**New mechanics (M):**

- **Condition counters**: an operation that turns itself into a hosted counter with rules text, active while it is hosted. Card: Eavesdrop. CR 1.9.5j, CR 1.13.6d, CR 3.5.1a, CR 1.13.14. Near relatives: Lockdown (UR 7a, an operation that stays in `CorpState::play_area` and is active there) and Hosting in general (VP 7b, 7c; PH 7). Neither hosts an operation on a card.
- **Counters a player is "considered to have"**: Acme Consulting's extra tag during encounters with the outermost ice protecting any server. Card: Acme Consulting: The Truth You Need. CR 1.9.4, CR 10.5.2. Near relative: none in the table. The closest pattern is the Payment Rule's "a standing number is asked, never kept" (`continuous::link`), which tags do not follow yet.
- **Ice moved into the attacked server, and the Runner moved to it**: a rez at the server's approach, a move to the innermost position, the Runner's position changed and the ice encountered. Card: Formicary. CR 6.2.2, CR 6.2.2b, CR 6.2.7a, CR 6.2.8. Near relative: "Forced or repeated encounter" (RWR 7d; UR 7b, `RunState::suspended`); `MoveThisIceToOutermost` and `MoveRunToOutermost` are the existing moves.
- **A swap of a root card across servers or with HQ, and a granted jack-out**: Card: Daruma. CR 8.8.3, CR 8.8.4, CR 8.8.4b, CR 6.1.5. Near relative: none in the table. `SwapApproachedIceWithCard` (Tatu-Bola, ice with HQ) and `SwapInstalledIce` are the existing swaps. A jack-out after the server is approached has no counterpart (`PermitJackOut` was deleted in the SG fidelity audit).
- **An interrupt that changes a play or install cost as it is paid**: "when you would play or install a card", trash a grip card to pay 2 [credit] less. Card: Patchwork. CR 1.3.9, CR 1.16.4a, CR 1.16.10b. Near relative: "Additional costs imposed by another card" (VP 3a; RWR 2a; TAI 8b; MS 4), and the install-trash replay (`payment::Ask::Install`).
- **Runner cards installed facedown**: blank, inactive, distinct, turned faceup in the heap. Card: Reboot. CR 8.1.4, CR 8.1.4a, CR 8.1.4b, CR 8.1.4c, CR 8.1.4e. Near relative: Hosting, PH 7 (a hosted card turned facedown, `Cost::TurnHostedFacedown`). There is no facedown card in the rig itself.
- **Identities from outside the deck**: a resource that hosts a g-mod identity of another faction, gains its text, and removes it from the game when uninstalled. Card: DJ Fenris. CR 1.5.4, CR 1.5.4a, CR 1.5.4b, CR 4.9.2. Near relative: Méliès U.'s identity copies brought along with the deck (VP 8, `SetIdentityCopy`, CR 1.5.2b), which are the Corp's own identity and not hosted.

## F3. Kitara — 120 cards (C 80 / V 33 / M 7)

| Card | Class | What it needs |
|---|---|---|
| Aimor | C | Subroutine: `Mill` the stack 3 and `TrashCard` ThisCard, as the trap ice already built do |
| Envelope | C | `DealDamage` net and `EndTheRun` subroutines |
| Endless EULA | C | Six "end the run unless the Runner pays 1" subroutines (`OfferPaidChoice` to the Runner) |
| Mlinzi | C | `PresentChoice` for the Runner between net damage and a `Mill` of their stack |
| Najja 1.0 | C | `click_breakable`, as Eli 1.0; two `EndTheRun` |
| Kamali 1.0 | C | `click_breakable`; core damage unless the Runner trashes a card of a type (`PresentChoice` + `PromptChooseCards`) |
| Sandman | C | `AddToHand` of a chosen installed Runner card, twice |
| Nightdancer | C | `LoseClicks` on the Runner and `AllottedClicksNextTurn` for the Corp |
| Anansi | C | `OnEncounterEnded` + `IceFullyBrokenThatEncounter`; `LookAtTopOfDeck` arrange (VP 6a); draws with a Runner `OfferPaidChoice` |
| Sadaka | C | Arrange or shuffle R&D (`PresentChoice`), draw; trash from HQ then a resource, `TrashCard` ThisCard |
| Mganga | C | `PsiGame` with `on_match` 1 net and `on_differ` 2 net, then trash itself |
| Masvingo | C | `advancement_requirement: 0` (Hadrian's Wall), `OnRez` advancement counter, `ContinuousKind::Subroutines` by hosted counters as Echo |
| Oduduwa | C | `OnEncounter` `PlaceAdvancementCounters` on itself, then on another ice by `Amount::HostedAdvancementTokens` |
| NEXT Diamond | C | `ContinuousKind::RezCost` lowered per other rezzed NEXT ice, as NEXT Bronze/Silver count them |
| NEXT Sapphire | C | "Up to X" selections (`up_to`) with X the rezzed NEXT ice count |
| Forced Connection | C | `RevealedWhileAccessed`; `OnAccessed` not in Archives (`AccessingIn`) + fixed `Trace` giving tags |
| Intake | C | Same shape as Forced Connection; success `AddToHand` a program or virtual resource |
| Tempus | C | Same shape; success `PresentChoice` lose [click][click] or core damage |
| Bio Vault | C | `advancement_requirement: 0`, `install_only_in` remote, cost `TrashSelf` + `RemoveAdvancementCounters(2)`, `DuringRun` |
| Calibration Testing | C | `TrashSelf` cost, `PlaceAdvancementCounters` on a card `InRootOfThisServer` |
| NGO Front | C | Advanceable asset, two `TrashSelf` + advancement-counter abilities |
| Gene Splicer | C | Advanceable ambush, `OnAccessed` `DealDamageAmount` by tokens, `AddToScoreAreaAsAgenda` (VP 7d) |
| False Flag | C | As Gene Splicer: tags per 2 tokens (`Amount::Every`), score as 3 points |
| Echo Chamber | C | [click][click][click] cost and `AddToScoreAreaAsAgenda` worth 1 |
| City Works Project | C | `installs_faceup`; `OnAccessed` meat damage by `AccessedCardAdvancementCounters` |
| Reconstruction Contract | C | `OnDamageSuffered` with `when: Damage(Meat)`; `ChooseNumber` then remove and place advancement counters |
| Urban Renewal | C | Power counters on rez, `OnTurnStart` remove one, `WhileTrue` empty → trash and meat damage |
| Overseer Matrix | C | `persistent_after_trash` (Daniela Jorge Inácio); `OnCardTrashed` `TrashedFromThisServer`, pay 1 for a tag |
| Armed Intimidation | C | `OnAgendaScored` `PresentChoice` for the Runner: 5 meat or 2 tags |
| Bacterial Programming | C | `OnAgendaScored`/`OnAgendaStolen`, `LookAtTopOfDeck` 7, selections to HQ and to Archives, arrange the rest (VP 6a) |
| Better Citizen Program | C | `OnCardPlayed` (run event) and `OnCardInstalled` (icebreaker), one shared `first_each_turn` |
| Ikawah Project | C | `steal_cost` [click] and 2[credit] (VP 3a) |
| Remote Enforcement | C | `PromptInstallCorpCard` from R&D protecting a remote, rezzed free, shuffle |
| SSL Endorsement | C | Hosted credits on score/steal; `OnTurnStart` take 3, heard in the Runner's score area (`from_runner_score_area`, UR 8) |
| Viral Weaponization | C | `LaterThisTurn` at the turn's end (Lightning Laboratory), `DealDamageAmount` by `CardsInHand` |
| Personalized Portal | C | `OnTurnStart`: Runner `DrawCards`, gain by `Amount::Every` 2 cards in the grip |
| Rashida Jaheem | C | `OnTurnStart` optional `TrashSelf` → credits and draws |
| TechnoCo | C | `ContinuousKind::InstallCost` on `Installing(filter)` for both sides' kinds; `OnCardInstalled` gain 1 |
| Malia Z0L0K4 | C | `OnRez` choose a non-virtual resource, `LoseAbilities` as Klevetnik (PH 8) |
| Distract the Masses | C | Runner gains 2, `PromptChooseCards` HQ → Archives, Archives → R&D, `removed_after_play` |
| Genotyping | C | `Mill` R&D 2, shuffle up to 4 from Archives, `removed_after_play` |
| Economic Warfare | C | `play_requirement` `TimesLastTurn(OnSuccessfulRun)`; `EffectIf` Runner has 4 → `LoseCredits` 4 |
| High-Profile Target | C | `IsTagged`; `DealDamageAmount` `Times(RunnerTags, 2)` meat |
| Market Forces | C | `IsTagged`; Runner loses 3 per tag, gain `CreditsLostThisResolution` |
| Riot Suppression | C | Last turn's Corp-card trash as play requirement; Runner's choice of core damage or `AllottedClicksNextTurn` −3 (Caveat Emptor); `removed_after_play` |
| Self-Growth Program | C | `IsTagged`; `AddToHand` 2 installed Runner cards |
| Standard Procedure | C | Name a card type (`Remember`), `RevealHand`, gain by `InZone` with `OfChosenCardType` |
| Threat Assessment | C | Last-turn trash requirement; `PresentChoice` for the Runner: 2 tags or `TopOfDeck` |
| Wake Up Call | C | Last-turn trash requirement; Runner chooses trash the chosen card or 4 meat |
| 419: Amoral Scammer | C | `OnCardInstalled` by the Corp, `first_each_turn`, `OfferPaidChoice` to the Corp, else `Expose` |
| Because I Can | C | `InitiateRun` remote + `SetAccessReplacement`: `ShuffleIntoDeck` the root cards |
| By Any Means | C | `NoActionTakenThisTurn`; `LaterThisTurn` every time on access not in Archives: `TrashCurrentlyAccessedCard` + meat |
| Contaminate | C | `AddCounters` 3 virus on a card `Not(HostsCounters(Virus))` |
| Credit Kiting | C | `TimesThisTurn` successful central run; `InstallRunnerCardFromGripWithDiscount` 8 and a tag |
| Diversion of Funds | C | `additional_play_cost` [click]; HQ access replacement, Corp loses up to 5, gain `CreditsLostThisResolution` |
| Embezzle | C | Remembered card type, `RevealAtRandom` 2 from HQ, trash those of the type, gain 4 each |
| Falsified Credentials | C | Remembered card type, `Expose` a remote card, `EffectIf` its type |
| Glut Cipher | C | Archives access replacement: Corp `PromptChooseCards` exactly 5 to HQ, then random trash from HQ |
| Wari | C | First successful HQ run, `TrashSelf`, chosen ice type, `Expose` ice, `AddToHand` if the type matches |
| Amina | C | Decoder; `OnIceFullyBroken` `ByThis`, `first_each_turn`, Corp `LoseCredits` |
| Cyberdelia | C | `Memory`; `OnIceFullyBroken` `first_each_turn` gain 1 |
| Engolo | C | `OnEncounter` `OncePerTurn` `OfferPaidChoice` → `GainIceSubtype` code gate for the encounter |
| Laamb | C | As Engolo, barrier |
| Takobi | C | `OnIceFullyBroken` power counter; `Cost::RemoveCounters(2)` → strength on a chosen icebreaker for the encounter |
| Kongamato | C | `TrashSelf`: `BreakSubroutines` `Fixed(1)`, which breaks lowest-id first |
| Logic Bomb | C | `TrashSelf`: `BypassEncounteredIce` and `LoseClicks` all |
| Trypano | C | Trojan (`installs_on_ice`), `OnTurnStart` counter, `WhileTrue` 5 counters → trash `HostIce` |
| Consume | C | `OnCardTrashed` Corp card → virus counter; `GainCreditsPerCounter` ×2 then `RemoveCounters` all |
| Crypt | C | Successful Archives run → counter; `TrashSelf` + `RemoveCounters(3)`: search stack for a virus, install, shuffle |
| Friday Chip | C | Counter on Corp-card trash; `OnTurnStart` `RemoveCounters` + `AddCounters` on a virus program |
| eXer | C | `OnBreach` R&D `AddAdditionalAccess` 1; `OnVirusCountersPurged` trash self |
| Nyashia | C | Power counters on install; R&D breach, optional `RemoveCounters(1)` for `AddAdditionalAccess` |
| Upya | C | R&D success → power counter; `OncePerTurn`, [click] + 3 counters: `GainClicks` 2 |
| Zer0 | C | `OncePerTurn`, [click] + `Cost::SufferDamage` net 1: credit and 2 draws |
| Lewi Guilherme | C | `ContinuousKind::HandSize` −1 on the Corp; `OnTurnStart` `PresentChoice` lose 1 or trash |
| PAD Tap | C | `OnAbilityGainedCredits` by the Corp, `first_each_turn`; Corp-usable trash ability as Rotary |
| Rogue Trading | C | 18 hosted credits on install, `trash_when_empty`, [click][click]: take 6, tag |
| Reclaim | C | [click], `TrashSelf`, `Cost::Trash` from the grip; `InstallRunnerCardFromZone` heap |
| Jackpot! | C | `OnTurnStart` hosted credit; agenda into the score area → `TakeAllCountersAsCredits` + trash |
| Kasi String | C | `OnRunEnded` remote, `first_each_turn`, breached and stole nothing; `WhileTrue` 4 → `AddToScoreAreaAsAgenda` (RWR 8c) |
| Amani Senai | V | A trace base that is an `Amount` (the triggering agenda's advancement requirement); `Trace.base` is a fixed `u32` |
| Kuwinda K4H1U3 | V | Trace base `HostedCounters`, and an "if unsuccessful" branch (`Trace` has only `on_success`) |
| Surveyor | V | Trace base and strength both 2 × `IceProtectingThisServer` |
| Trojan Horse | V | An `Amount` for the margin by which trace strength beat link, read by `PrintedCostAtMost` |
| Black Hat | V | A trace forced by a Runner card, its unsuccessful branch, and extra access on every HQ/R&D breach this turn (`LaterThisTurn` + `AddAdditionalAccess`) |
| White Hat | V | Runner-forced trace's unsuccessful branch; `RevealHand`, Runner picks 2 to shuffle into R&D |
| No One Home | V | `Prevent` all tags or all net damage, gated on an unsuccessful trace[0] started inside the prevention window |
| Musaazi | V | A `Cost` paid in virus counters from any of the Runner's cards ("Any virus counter"), not only the acting card's |
| Yusuf | V | Same cost word as Musaazi |
| Freedom Khumalo: Crypto-Anarchist | V | The same cost, X counters with X = `AccessedCardPrintedCost`, as an access ability like Imp's |
| Puffer | V | A `ContinuousKind` for a program's own memory cost, + `Strength` by its power counters |
| Knobkierie | V | Memory usable only by virus programs (a restricted `Memory`, near `HostsMemory`) |
| Acacia | V | An `Amount` for the virus counters a purge removed, on `OnVirusCountersPurged` |
| Reverse Infection | V | The same purge `Amount`, `Every` 3 → `Mill` the stack |
| Compile | V | Install from stack or heap free at the first encounter, and a run-end effect on *that* installed card (bottom of stack) |
| Kabonesa Wu: Netspace Thrillseeker | V | Install from stack with a discount, then `LaterThisTurn` turn end acting on that card: `RemoveFromGame` |
| Marathon | V | A `Prohibition` on running one named server this turn; event returned to the grip on success, gain [click] |
| Flame-out | V | Credits that pay only for the hosted program (`PaysFor` about a hosted card) and a turn-end trash if they were spent |
| Plague | V | A chosen server remembered while the card is installed (`ChooseServer` lasts the turn today) |
| RNG Key | V | A chosen number compared with the next accessed card's rez/play cost or advancement requirement |
| Gbahali | V | Break the *last* subroutine (`SubroutineBreakCount` breaks lowest-id first) |
| Hippo | V | `OnIceFullyBroken` filtered to the outermost ice of the attacked server; `RemoveSelfFromGame` cost, trash the ice |
| Gebrselassie | V | Host an installed hardware onto an icebreaker by [click] (Hush moves a host); `BoostsLastTheRun` for the turn |
| Zamba | V | A moment for a Corp card exposed (an `EventFilter` telling an expose from a reveal on `OnCardRevealed`) |
| Emergent Creativity | V | An `Amount` summing the install costs of cards trashed this way, as the install's discount |
| Jinja City Grid | V | A Trigger for a card drawn (with the card); reveal and install protecting this server at 4 less |
| Asa Group: Security Through Vigilance | V | First install each turn → install from HQ "in the same server" (a server read off the triggering install) |
| Mwanza City Grid | V | `AddAdditionalAccess` 3 on breach, plus a breach-end moment and an `Amount` of cards accessed in that breach |
| SSO Industries: Fueling Innovation | V | An `Amount` for agenda points on installed faceup agendas; ice with no tokens (`Unadvanced`) |
| Warden Fatuma | V | `ContinuousKind::Subroutines` granted to other ice by subtype, before the printed ones (Scope beyond `This`) |
| Azmari EdTech: Shaping the Future | V | A card type named at turn end and remembered until renamed; `OnCardPlayed`/`OnCardInstalled` with `OfChosenCardType` |
| Jua | V | A `Prohibition` on installing cards for the turn; subroutine Runner picks 1 of 2 to `TopOfDeck` |
| Degree Mill | V | A `Cost` word for shuffling installed cards into the stack, as a `steal_cost` |
| Corporate "Grant" | M | A current: a Runner event that stays in play until another current or a score |
| Death and Taxes | M | A current operation; plus `OnCardInstalled`/`OnCardTrashed` gains |
| Kill Switch | M | A current; plus `RevealedWhileAccessed` for agendas and a trace on access or score |
| Code Replicator | M | The Runner approaches the passed ice again without moving (CR 6.1.3b) |
| Mti Mwekundu: Life Improved | M | Ice installed innermost at the server approach and the Runner moved back to approach it |
| Slipstream | M | The Runner moved to a same-position ice on a central server (another server) mid-run |
| Assimilator | M | Facedown installed Runner cards in the rig, turned faceup (CR 8.1.4) |

**New mechanics (M):**

- **Currents**: an operation or event that is not trashed when it
  resolves, stays active, and is trashed when another current is played
  or (for an operation) an agenda is stolen, or (for an event) an agenda is
  scored. Cards: Corporate "Grant" (Runner event), Death and Taxes, Kill
  Switch (Corp operations). Rules: CR 3.5.1b, CR 3.7.1b, CR 8.6.6c. Near
  relative: Lockdown (UR 7a): `not_trashed_until_your_next_turn` files an
  operation in `CorpState::play_area`, active there. A current needs a
  different expiry (another current, a score or a steal) and a Runner-side
  play area for the event.
- **Moving the Runner to a position other than the outermost, or
  approaching the same ice again**: Code Replicator (approach the passed
  ice again, without moving), Mti Mwekundu: Life Improved (ice installed
  innermost during the server approach, then the Runner moves to it and
  approaches it), Slipstream (move to the ice in the same position on a
  central server, which changes the attacked server). Rules: CR 6.1.3b
  (Code Replicator by name), CR 6.2.2b, CR 6.2.6, CR 6.2.6b. Near
  relatives: Forced or repeated encounter (RWR 7d, UR 7b:
  `ForceEncounter`, `RunState::suspended`) and `MoveRunToOutermost`
  (Proprionegation, Letheia Nisei). The second rebuilds the run at the
  outermost position with nothing passed, but no effect sets an arbitrary
  position.
- **Facedown installed Runner cards in the rig**: Assimilator (turn one
  faceup, and trash it if it is an event). Rules: CR 4.6.4d, CR 8.1.4a,
  CR 8.1.4e. Near relative: Hosting in general (PH 7, "turned facedown"):
  `hosts_facedown`, `Cost::TurnHostedFacedown` and `TurnHostedFaceup`
  (Matryoshka) cover hosted cards only. No card in Kitara installs a
  Runner card facedown. Apex (F6) is the first.

## F4. Red Sand — 120 cards (C 63 / V 48 / M 9)

| Card | Class | What it needs |
|---|---|---|
| Battlement | C | two `EndTheRun` subroutines |
| Authenticator | C | `OnEncounter` + `OfferPaidChoice` with `Cost::TakeTags(1)` → `BypassEncounteredIce`; Corp credits and `EndTheRun` subroutines |
| Kakugo | C | `OnIcePassed` (subject `This`) → `DealDamage(Net, 1)` |
| Seidr Adaptive Barrier | C | `ContinuousKind::Strength` per `Amount::IceProtectingThisServer` |
| Self-Adapting Code Wall | C | `ContinuousKind::StrengthCannotBeLowered` |
| Conundrum | C | +3 strength `while` the rig holds `HasSubtype(Ai)`; trash a program, `LoseClicks`, `EndTheRun` |
| Data Loop | C | `OnEncounter` Runner `PromptChooseCards` over the grip → `AddToDeck` top; `EffectIf(IsTagged)` ETR |
| Free Lunch | C | Corp ability, `Cost::RemoveCounters(1)` → `LoseCredits(Runner, 1)`; subroutines `AddCounters` |
| Owl | C | subroutine: choose an installed program, `AddToDeck` top of the stack |
| Watchtower | C | search R&D to HQ and shuffle (Executive Boot Camp's, Tocsin's search) |
| NEXT Opal | C | NEXT Silver's `ContinuousKind::Subroutines` counted over rezzed NEXT ice; subroutine `PromptInstallCorpCard` |
| NEXT Wave 2 | C | `OnAgendaScored` `EffectIf` rezzed NEXT ice → optional core damage |
| Nerine 2.0 | C | Ansel 2.0's "Lose [click][click]: break up to 2"; core damage + may draw |
| Synth DNA Modification | C | `OnSubroutineBroken` + `first_each_turn`, `when` ice `HasSubtype(Ap)` → net damage |
| Henry Phillips | C | `OnSubroutineBroken` during a run on this server, requirement `IsTagged` → gain 2 |
| Fractal Threat Matrix | C | `OnIceFullyBroken` `when` ice `InThisServer` → `Mill` the stack 2 |
| Traffic Analyzer | C | `OnRez` `when` ice `InThisServer` → `Trace` [2], gain 1 |
| Warroid Tracker | C | `OnCardTrashed` `when TrashedFromThisServer` → `Trace` [4], Runner trashes 2 installed |
| Threat Level Alpha | C | `additional_play_cost` click; `Trace` → `EffectIf(IsTagged)` `GiveTags(RunnerTags)` else 1 |
| CFC Excavation Contract | C | `OnAgendaScored` → `GainCreditsAmount` `Times(CorpInstalls(rezzed Bioroid), 2)` |
| Audacity | C | `play_requirement` HQ ≥ 3; trash HQ; two single-counter `PlaceAdvancementCounters` on `Advanceable` installs |
| Mass Commercialization | C | `GainCreditsAmount` `Times(CorpInstalls(Advanced), 2)` |
| Priority Construction | C | `additional_play_cost` click; `PromptInstallCorpCard` remote-only ignoring costs, `if_installed` place 3 |
| Replanting | C | `additional_play_cost` click; installed card `AddToHand`; two installs from HQ ignoring costs |
| Shipment from Tennin | C | `play_requirement` `Not(AmountAtLeast(TimesLastTurn(OnSuccessfulRun), 1))`; place 2 counters |
| Oberth Protocol | C | additional rez cost `Cost::Forfeit(1)` (Plutus's); `OnAdvance` + `first_each_turn` `when InThisServer` → place 1 more |
| Load Testing | C | `AllottedClicksNextTurn(Runner, -1)` |
| MCA Austerity Policy | C | `OncePerTurn` click: `AddCounters` + `AllottedClicksNextTurn(Runner, -1)`; `AllOf` cost with `RemoveCounters(3)` → `GainClicks(Corp, 4)` |
| O₂ Shortage | C | Runner `PresentChoice`: `TrashCard(RandomFromHand(Runner))` or Corp `GainClicks(2)` |
| Meteor Mining | C | `OnAgendaScored` `PresentChoice`: gain 7, or `EffectIf` tags ≥ 2 → 7 meat |
| Biased Reporting | C | `PresentChoice` of type; Runner `PromptChooseCards` over their installs of it, gain per `CardsSelected`; Corp gains per `RunnerInstalls` |
| AR-Enhanced Security | C | `OnCardTrashed` + `first_each_turn` `when` a Corp card trashed by the Runner → `GiveTags(1)` |
| Escalate Vitriol | C | scored agenda's ability window (UR 4), `OncePerTurn` click → `GainCreditsAmount(RunnerTags)` |
| Obokata Protocol | C | `steal_cost` `SufferDamage(Net, 4)` |
| Water Monopoly | C | `ContinuousKind::InstallCost` +1, `Installing` resources not `HasSubtype(Virtual)`, active in the score area |
| Ben Musashi | C | `ContinuousKind::StealCost` `StealingFromThisServer` + `persistent_after_trash` |
| Bamboo Dome | C | region root of R&D; reveal top 3, choose 1 to HQ, arrange the rest (VP 6a) |
| Breached Dome | C | `RevealedWhileAccessed`; `OnAccessed` → meat 1 + `Mill` the stack 1 |
| Clyde Van Rite | C | Corp `OnTurnStart` → Runner `OfferPaidChoice` 1 credit or `Mill` the stack 1 |
| Defense Construct | C | advanceable (Shadow's `advancement_requirement` 0); `TrashSelf` + `DuringRunOn(Archives)` → `Repeat` over `last_known` tokens: facedown Archives card to HQ |
| Whampoa Reclamation | C | `OncePerTurn`, `Cost::Trash` from HQ → Archives card `AddToDeck` bottom |
| Alice Merchant: Clan Agitator | C | `OnSuccessfulRun` + `first_each_turn` `when Server([Archives])` → Corp trashes 1 from HQ |
| Los: Data Hijacker | C | `OnRez` + `first_each_turn` `when Card(Ice)` → gain 2 |
| Build Script | C | gain 1, draw 2 |
| Spot the Prey | C | `Expose` a non-ice card, then `InitiateRun` |
| Exploit | C | `play_requirement` successful runs on HQ, R&D and Archives this turn (`TimesThisTurnWhen`); `DerezCard` up to 3 |
| Mining Accident | C | central successful run this turn; Corp `OfferPaidChoice` 5 or `GiveBadPublicity`; `removed_after_play` |
| Mad Dash | C | `SetRunEndedEffect` with `StoleAgendaDuringLastRun` → `AddToScoreAreaAsAgenda` (RWR 8c) else meat 1 |
| Möbius | C | Always Have a Backup Plan's second run on run end; gain 4 if it succeeds |
| Mars for Martians | C | `NoActionTakenThisTurn` (In the Groove's); draw per clan resource, gain per tag |
| Reshape | C | `SwapInstalledIce` over two `UnrezzedIce` (Tāo Salonga's Runner-side swap) |
| Berserker | C | `OnEncounter` `when` barrier → `BoostStrengthAmount(EncounteredIceSubroutines, Encounter)`; break up to 2 barrier |
| Flashbang | C | interface 6 credits → `DerezCard(EncounteredIce)` restricted to sentry; pump |
| God of War | C | `OnTurnStart` `OfferPaidChoice` `TakeTags(1)` → 2 virus counters; break cost `RemoveCounters(1)` |
| Maven | C | `ContinuousKind::Strength` per `RunnerInstalls(Program)` |
| Customized Secretary | C | install trigger hosts revealed programs (`HostCardOnThisCard`); Madani's "install a hosted program" |
| Keros Mcintyre | C | `OnDerez` + `first_each_turn` `when` ice → gain 2 |
| Bloo Moose | C | `OnTurnStart` optional heap card `RemoveFromGame`, then gain 2 |
| Dadiana Chacon | C | `OnTurnStart` `RunnerCreditsAtMost(5)`; `WhileTrue` `RunnerCreditsAtMost(0)` → trash self, meat 3 |
| Bug Out Bag | C | X install cost (RWR 6e), X counters; `OnDiscardPhaseEnd` with empty grip → draw per `HostedCounters`, trash self |
| Clan Vengeance | C | `OnDamageSuffered` → `AddCounters`; `TrashSelf` → `Repeat` (`last_known` counters) random HQ trash |
| Caldera | C | two interrupts, each 3 credits: `Prevent(Damage{Core,1})` and `Prevent(Damage{Net,1})` |
| Dummy Box | C | interrupt, `Cost::Trash` a grip card → `Prevent(Trash(SameTypeAsPaidCard))` |
| Aeneas Informant | V | a `Trigger` for an access that ended with the card not trashed (not in Archives, has a trash cost), then reveal + gain |
| Maw | V | the same access-ended `Trigger`, "not stolen or trashed", `first_each_turn` → random HQ trash |
| Aumakua | V | the access-ended moment at the breach's end ("finished breaching"), and an expose `when` on `OnCardRevealed`; strength per `HostedCounters` composes |
| Rip Deal | V | an access replacement of HQ's cards only (root still accessed), counted off `AccessLimit(Hq)`, heap → grip |
| Counter Surveillance | V | breach replaced by paying X = tags and accessing a chosen number across the server and its root (`Access` with `ChooseNumber`) |
| SYNC BRE | V | a lingering "access 1 fewer card" for each breach this run |
| Severnius Stim Implant | V | a cost "trash 2 or more grip cards" (a count chosen), carried into the run as `Every(CardsSelected, 2)` extra accesses |
| Corporate Defector | V | `OnBasicDrawAction` heard as the Corp's (`Whose(Corp)`), and a reveal of "that card" drawn |
| Open Forum | V | a `Trigger` for after the mandatory draw (`OnDrawAboutToResolve` is before it) |
| CPC Generator | V | a filter for the basic click-for-credit action on `OnActionFinished`, `first_each_turn` |
| Jemison Astronautics: Sacrifice. Audacity. Success. | V | `OnForfeit` heard by the identity for any agenda (`Subject::Any`) + an `Amount` for the forfeited agenda's points |
| Sacrifice | V | `Amount`: the forfeited agenda's points (after `Cost::Forfeit`); `RemoveBadPublicity` as an amount, gain per removed |
| Quarantine System | V | the forfeited-points `Amount` as a rez discount on up to 3 `RezInstalled` |
| Success | V | an `Amount` for the forfeited agenda's advancement requirement; "advance X times" as advances, not placements |
| Red Planet Couriers | V | moving all advancement counters onto 1 card (`CountersOnOwnInstalls(Advancement)` exists; a move word) |
| Tithonium | V | "cannot host cards" (a `Cannot` about hosting); forfeit rez alternative is Biawak's |
| Standoff | V | an alternating "repeat this process" loop between the two players (Complete Image repeats one player's) |
| Reeducation | V | `AddRandomFromGripToBottom` as an effect with an `Amount` (`CardsSelected`), behind "at least X in the grip" |
| AgInfusion: New Miracles for a New World | V | a `Cost` "trash the approached unrezzed ice"; `MoveRunToOutermost` to a chosen server other than the attacked |
| Sand Storm | V | `MoveThisIceToOutermost` to a chosen other server with the run following it, then trash this ice |
| Mirāju | V | a pass replaced (CR 6.1.3e): on `OnEncounterEnded` if its printed subroutine was broken → `MoveRunToOutermost(Archives)`, jack-out offer, derez self |
| Bloom | V | an install position "directly inward from this ice" beside `any_position`; first subroutine is Tributary's `another_server` |
| Mandatory Seed Replacement | V | "rearrange any number of ice" across servers as repeated optional `SwapInstalledIce` (CR 8.3.2) |
| Metamorph | V | a swap of two installed non-ice Corp cards between servers (region limit, CR 3.6.5e); the ice swap composes |
| Network Exchange | V | an `InstallCost` scope for ice installed outside the innermost position (onto a server that has ice) |
| Inversificator | V | an `EventFilter` on `OnIcePassed`: passed after an encounter where this program fully broke it (CR 6.1.3f); then `SwapInstalledIce` |
| Persephone | V | an `Amount`: subroutines on that sentry that resolved in the encounter just passed |
| Lean and Mean | V | +2 strength to every icebreaker for the run (a lingering `Rig(Icebreaker)` scope), behind a program count |
| Pushing the Envelope | V | the same run-long icebreaker boost, behind a grip count |
| Helheim Servers | V | a `StrengthOf` for "ice protecting this server" for the run (`EachIce` is every ice) |
| Na'Not'K | V | an `Amount`: ice protecting the attacked server, during runs |
| Daredevil | V | an `EventFilter` on `OnRunStart`: the server is protected by 2 or more ice (a count beside `ProtectedByIce`) |
| Leave No Trace | V | a `CardFilter` "rezzed during this run" for `DerezCard` at run end |
| Rubicon Switch | V | a `CardFilter` "rezzed this turn" with `PrintedCostExactly(X)` off `CreditsX` |
| Diana's Hunt | V | a `CardFilter` "installed by this card this run" for the run-end trash; install on each encounter while `RunEventActive` |
| Signal Jamming | V | a `Prohibition` on installing, `until` the run's end |
| Zed 2.0 | V | an `EffectRequirement`: the Runner lost [click] to break a subroutine this run; click-break and hardware trash compose |
| Bio-Modeled Network | V | a `Preventable::Damage` word "all but 1" |
| Jarogniew Mercs | V | a `Cannot` "the Corp trashes this resource while another resource is installed"; counters, `WhileTrue` empty trash and meat prevent compose |
| Net Analytics | V | a moment for a tag avoided (prevented), beside `OnTagRemoved` |
| Bioroid Work Crew | V | a timing word "only during the next paid ability window after an operation resolves" |
| Salvaged Vanadis Armory | V | the same next-window timing word after damage; an `Amount` for damage suffered this turn (not occurrences) |
| Respirocytes | V | "the first time each turn you have no cards in your grip": a `WhileTrue` with a first time; 3-counter trash composes |
| Khondi Plaza | V | recurring credits as an `Amount` (number of remotes); `PaysFor` rezzing ice protecting this server |
| Māui | V | recurring credits as an `Amount` (ice protecting HQ); `PaysFor` during runs on HQ |
| Rosetta 2.0 | V | a `Cost` "remove an installed program from the game"; search stack and install with `Discount::Amount(PaidCardPrintedCost)` |
| Restore | V | install and rez from Archives paying costs; a `CardFilter` for the other copies of that card's title, `RemoveFromGame` |
| The Archivist | V | `Trace` with an `on_failure` branch, initiated by a Runner card for the Corp; `OnAgendaScored` `when` initiative or security |
| Adjusted Matrix | M | a paid (interface) ability granted to the host icebreaker, plus a Runner card gaining a subtype (AI) |
| Dedicated Processor | M | a paid ability granted to the host icebreaker ("2[credit]: +4 strength") |
| Loki | M | ice copying another ice's subtypes and subroutines for the run, ordered before its own |
| Mass-Driver | M | the first 3 subroutines of the next encounter this run do not resolve |
| MCA Informant | M | condition counter on a Runner resource: +1 considered tag, grants the Corp a click ability on it; Terminal |
| Rover Algorithm | M | condition counter on rezzed ice: host strength per counter, counter on each pass |
| Transparency Initiative | M | condition counter on an agenda turned faceup in its server: gains public, gain on advance |
| Rolling Brownout | M | Corp current; play cost +1 for all operations and events, first event each turn → gain 1 |
| System Seizure | M | Runner current; interrupt on a strength increase about to happen, boosts last the run (`BoostsLastTheRun`) |

**New mechanics (M):**

- **A paid ability granted to another card** (a host icebreaker gains an
  interface or pump ability, and a Runner card gains a subtype): Adjusted
  Matrix, Dedicated Processor. Governed by CR 9.1.9 (gaining abilities),
  CR 9.1.3b (the source of a granted ability is the object that has it)
  and CR 9.5.2d (interface abilities). Near relative: none in the shared
  table. "Losing abilities" (PH 8: `ContinuousKind::LosesAbilities`,
  `active::lost_abilities`) is the reverse. Outside the table,
  `TriggeredEffect::granted` (ZATO City Grid) grants a *triggered*
  ability, and `installs_onto` (The Personal Touch) gets the mod onto
  the breaker.
- **Ice copying another ice's subtypes and subroutines for a run**: Loki.
  Governed by CR 9.8.2 and CR 9.8.3a (where gained subroutines go in the
  order), CR 9.8.4b (subroutines gained during an encounter come into
  existence unbroken) and CR 9.1.9. Near relative: "Additional
  subroutines, ordered" (VP 7e; RWR 8b for the run, `GainSubroutine`
  with `before`/`after`), and `GainIceSubtype`. Neither copies a whole
  list off another card.
- **Subroutines that do not resolve in a later encounter**: Mass-Driver
  ("the first 3 subroutines of the next encounter this run do not
  resolve"). Governed by CR 6.5.5 and CR 6.9.3c (unbroken subroutines
  resolve). Near relative: "Losing abilities" (PH 8, CR 9.1.9a: a lost
  subroutine is ignored) and "Break restrictions" (`Lingering::BreakLimit`).
  This one is a lingering effect created for an encounter that has not
  begun.
- **Condition counters** (an operation that turns itself into a counter
  with rules text and installs onto another card): MCA Informant (on a
  Runner connection, with "considered to have 1 additional tag" and a
  click ability the host gains for the Corp), Rover Algorithm (on rezzed
  ice), Transparency Initiative (on an installed agenda it turns faceup,
  which then gains public). Governed by CR 1.9.5j, CR 1.13.6d, CR 1.13.14
  and CR 3.5.1a. Near relative: "Hosting in general" (VP 7b, 7c; RWR 3d
  onto a host), and "Lockdown" (UR 7a), the one place an operation stays
  active after resolving (`CorpState::play_area`). MCA Informant also
  needs "Terminal" (RWR 6a, already built).
- **Currents** (an operation or event that stays active until another
  current is played, or until an agenda is stolen or scored): Rolling
  Brownout, System Seizure. Governed by CR 3.5.1b and CR 3.7.1b. Near
  relative: "Lockdown" (UR 7a). It keeps an operation active in the play
  area until a set time, but its end is a timing point and a current's
  end is an event, and the Runner has no play area. System Seizure also
  needs a moment for a strength increase *about to happen*, so that its
  interrupt can attach `BoostsLastTheRun` (an existing `ContinuousKind`)
  to that breaker.

## F5. Flashpoint, and Terminal Directive Cards — 168 cards (C 108 / V 41 / M 19)

| Card | Class | What it needs |
|---|---|---|
| Aaron Marrón | C | Transport Monopoly's scored-or-stolen counters; `Cost::RemoveCounters`, `RemoveTags` + `DrawCards` |
| Algo Trading | C | `ChooseNumber` + `Repeat` of `AddCounters`, `ThisCardCountersAtLeast`, `TakeAllCountersAsCredits` (Crowdfunding's credit counters) |
| Beth Kilrain-Chang | C | `OnTurnStart` under `CorpCreditsAtLeast` and `Not`, `GainClicks` |
| Blockade Runner | C | `DrawCards` then a grip selection to the stack with `shuffle_after` |
| Laguna Velasco District | C | `OnBasicDrawAction` / `IncreaseAboutToResolve` (Daily Business Show) |
| LLDS Memory Diamond | C | `ContinuousKind::Memory`, `Link`, `HandSize` |
| Ubax | C | `OnTurnStart` draw; console limit (checkpoint) |
| Process Automation | C | `GainCredits` + `DrawCards` |
| Peace in Our Time | C | `NoActionTakenThisTurn`, `TimesLastTurn(OnAgendaScored)`, `Prohibition::Run` for the turn |
| Temüjin Contract | C | `ChooseServer` on install, Crowdfunding's loaded credits and self-trash, Security Testing's chosen server |
| Biometric Spoofing | C | `Prevent(Damage)` behind `Cost::TrashSelf` (Net Shield, Decoy) |
| Misdirection | C | `Cost::CreditsX` + `RemoveTags(ChosenNumber)` |
| First Responders | C | ability under `AmountAtLeast(TimesThisTurn(OnDamageSuffered))` |
| Officer Frank | C | `TimesThisTurnWhen` meat damage, `Repeat` of `TrashCard(RandomFromHand)` |
| Reaver | C | `OnCardTrashed` `when: InstalledCard`, `first_each_turn` (Aggressive Trendsetting) |
| Tapwrm | C | install requirement on `TimesThisTurnWhen` a central run, `Amount::Every`, `OnVirusCountersPurged` |
| Obelus | C | `HandSize` by `RunnerTags`, `OnRunEnded` `first_each_turn` with `CardsAccessedLastRun` |
| Equivocation | C | `TopOfDeck` reveal of R&D, then `DrawCards(Corp)` behind a choice |
| Frantic Coding | C | Into the Depths' search of the top 10, a 5[credit] discount, the rest milled |
| Levy Advanced Research Lab | C | `TopOfDeck` reveal of 4, a program to the grip, the rest arranged to the bottom |
| Black Orchestra | C | `from_discard` encounter trigger (Crowdfunding's heap install), `BoostStrength` then `BreakSubroutines` code gate |
| MKUltra | C | as Black Orchestra, sentry |
| Paperclip | C | as Black Orchestra, barrier, with `Cost::CreditsX` (Lobisomem) |
| Demara | C | barrier breaker; `TrashSelf` + `BypassEncounteredIce` under `Encountering(Barrier)` |
| Lustig | C | sentry breaker; `TrashSelf` + `BypassEncounteredIce` under `Encountering(Sentry)` |
| Nfr | C | `OnIceFullyBroken` `ByThis`, `Strength` by `HostedCounters` |
| Sūnya | C | as Nfr, sentry |
| Dai V | C | `Cost::CreditsFrom` stealth (Corsair), break all |
| Mammon | C | `ChooseNumber` + `Repeat` of `AddCounters`, `OnDiscardPhaseEnd` removal, counter-cost break (Botulus) |
| Dhegdheer | C | Djinn's `MayHost`, `HostsMemory`, `InstallingOntoThis` discount |
| Ele "Smoke" Scovak: Cynosure of the Net | C | `recurring_credits` 1 with `PaysFor::Using(Icebreaker)` |
| Khan: Savvy Skiptracer | C | `OnIcePassed` `first_each_turn`, `InstallRunnerCardFromGripWithDiscount` |
| Null: Whistleblower | C | `OncePerTurn` encounter offer, grip `Cost::Trash`, `ModifyStrength` −2 for the run |
| Omar Keung: Conspiracy Theorist | C | `InitiateRun(Archives)` + a choice of two `RedirectRunOnSuccess` (Sneakdoor Beta) |
| Data Breach | C | `SetRunEndedEffect` / `LaterThisTurn` with a second `InitiateRun` (Always Have a Backup Plan) |
| Dean Lister | C | icebreaker selection whose `then` boosts by `CardsInHand` for the run |
| Deuces Wild | C | `ResolveSomeOf` 2 of 4, `Expose` then a run |
| GPI Net Tap | C | `OnIceApproached` `Expose`, then `Cost::JackOut` + `TrashSelf` (Flip Switch) |
| Careful Planning | C | first click, a selection whose `then` is `Prohibit { Rez, this_install }` for the turn |
| SYN Attack | C | `additional_play_cost` click, `PresentChoice` chooser Corp: discard 2 / draw 4 |
| Aiki | C | `PsiGame` `on_differ` Runner draws 2 (Konjin) |
| Bloodletter | C | Runner's `PresentChoice`: program `Trash` or `Mill` 2 |
| Bulwark | C | `OnRez` bad publicity, `OnEncounter` under `ZoneHasAtLeast` AI program |
| Chiyashi | C | `OnSubroutineBroken` subject This under an AI requirement, `Mill` 2 |
| Data Ward | C | `OnEncounter` `OfferPaidChoice` 3[credit] or a tag, `EffectIf(IsTagged, EndTheRun)` |
| DNA Tracker | C | net damage + `LoseCredits` |
| IP Block | C | encounter tag under an AI requirement, `Trace`, tagged ETR |
| Tapestry | C | `LoseClicks`, Corp draw, HQ card to top of R&D |
| Tribunal | C | Runner trashes an installed card ×3 |
| Weir | C | `LoseClicks`, a grip trash |
| Veritas | C | credits, `LoseCredits`, `Trace` tag |
| Eli 2.0 | C | Eli 1.0's `Cost::LoseClicks` break for the Runner |
| Fairchild | C | `OfferPaidChoice` with `Credits`, `Trash`, `SufferDamage` costs |
| Fairchild 1.0 | C | Eli 1.0's break; `PresentChoice` pay or trash |
| Fairchild 2.0 | C | as Fairchild 1.0, core damage |
| Fairchild 3.0 | C | as Fairchild 2.0; Corp's `PresentChoice` core damage or ETR |
| Sherlock 2.0 | C | Eli 1.0's break; `Trace` → program `AddToDeck(Bottom)` |
| Executive Functioning | C | `Trace` → core damage |
| Holmegaard | C | `Trace` → `Prohibition::Access` for the run; icebreaker trash |
| Macrophage | C | `Trace` → `PurgeVirusCounters`, virus trash, heap `RemoveFromGame`, ETR |
| Hailstorm | C | heap selection `RemoveFromGame`, ETR |
| Mausolus | C | advanceable ice (Mestnichestvo), `EffectIf` `HostedAdvancementTokens` ≥ 3 |
| Thoth | C | encounter tag; `Trace` → `DealDamageAmount`/`LoseCreditsAmount` by `RunnerTags` |
| Ark Lockdown | C | `ChooseCardName` + `ForEach` heap `ChosenName` → `RemoveFromGame` |
| Best Defense | C | installed selection by `PrintedCostAtMost` the Runner's tags |
| BOOM! | C | `play_requirement` on `RunnerTags`, click `additional_play_cost`, 7 meat |
| Financial Collapse | C | Runner's `OfferPaidChoice` resource `Trash` or `LoseCreditsAmount` per resource |
| Friends in High Places | C | terminal (`EndActionPhase`), `PromptInstallCorpCard` from Archives ×2 |
| Hard-Hitting News | C | terminal, `TimesLastTurn(OnRunStart)`, `Trace` → 4 tags |
| Hatchet Job | C | `Trace` → `AddToHand` of a non-virtual install |
| Hunter Seeker | C | `TimesLastTurn(OnAgendaStolen)`, trash an installed card |
| IPO | C | terminal, gain 13 |
| Liquidation | C | rezzed selection trashed, `GainCreditsAmount(Times(3, CardsSelected))` |
| Observe and Destroy | C | `Cost::RemoveTags` as play cost, `RunnerCreditsAtMost` |
| Preemptive Action | C | terminal, `ShuffleIntoDeck` 3 from Archives, removed from game |
| Psychokinesis | C | terminal, `PromptInstallCorpCard` from the top 5 of R&D into a remote |
| Special Report | C | HQ selection `ShuffleIntoDeck`, `DrawCardsAmount(CardsSelected)` |
| Stock Buy-Back | C | terminal, `InZone(OpponentScoreArea)` × 3 |
| Ultraviolet Clearance | C | two-click play cost, credits, draw, one `PromptInstallCorpCard` from HQ |
| Violet Level Clearance | C | terminal, credits and draw |
| Brain Rewiring | C | `ChooseNumber`, `Repeat` of `AddToDeck(Bottom)` on `RandomFromHand`, Runner draws 1 |
| Crisis Management | C | scored agenda's `OnTurnStart` under `IsTagged` |
| Elective Upgrade | C | scored counters, `OncePerTurn` click + counter: `GainClicks` 2 |
| Graft | C | R&D selection up to 3, revealed, to HQ, shuffled |
| Illicit Sales | C | optional `GiveBadPublicity`, `Times(3, BadPublicity)` |
| Paper Trail | C | `Trace` → `ForEach` connection and job resources |
| Project Kusanagi | C | counters from the score's advancement counters past 2 (Project Atlas), `GainSubroutine` for the run (Thunderbolt) |
| Show of Force | C | on-score meat damage |
| Sensor Net Activation | C | `RezInstalled` bioroid ignoring costs, `LaterThisTurn` `DerezCard` |
| Anson Rose | C | Trick of Light's counter move, `OnRez` of ice |
| C.I. Fund | C | as Algo Trading, Corp side |
| Estelle Moon | C | `OnCardInstalled` in a remote root, `[trash]` reads `last_known` counters |
| Illegal Arms Factory | C | `OnTurnStart`; `OnCardTrashed` `TrashedRezzed` by the Runner → bad publicity |
| Long-Term Investment | C | counters; ability under `ThisCardCountersAtLeast` 8, `ChooseNumber` take |
| Mr. Stone | C | `OnTagsGiven` → meat damage |
| NASX | C | `OnAbilityGainedCredits` (not this card), `ChooseNumber`, `GainCreditsPerCounter` |
| Honeyfarm | C | `RevealedWhileAccessed`, `OnAccessed` `LoseCredits` |
| Prisec | C | `OnAccessed` while installed, Corp `OfferPaidChoice` tag + meat |
| Sandburg | C | `Strength` on `Scope::Ice` by `Every(5, Credits)` while `CorpCreditsAtLeast` 10 |
| Watchdog | C | `RezCost` on ice, `first_each_turn` (Reina Roja), by `RunnerTags` |
| Bryan Stinson | C | ability under `RunnerCreditsAtMost` 5, `PlayOperation` from Archives (Plutus) |
| Black Level Clearance | C | `OnSuccessfulRun` this server, Runner's `OfferPaidChoice` `Cost::JackOut` or core damage |
| Drone Screen | C | `OnRunStart` this server under `IsTagged`, `Trace` → `Unpreventable` meat |
| Georgia Emelyov | C | `OnRunEnded` `LastRunUnsuccessful` this server; `PromptMoveThisCardToAnotherRoot` |
| Manta Grid | C | `OnRunEnded` successful on this server, `Not(And(Not, Not))` for the or, `AllottedClicksNextTurn`; region limit |
| Mason Bellamy | C | `OnEncounterEnded` `ProtectedByIce` this server, `BrokeASubroutineThatEncounter`, `LoseClicks` |
| Jinteki: Potential Unleashed | C | `OnDamageSuffered` `when: Damage(Net)`, `Mill` 1 |
| NBN: Controlling the Message | C | `OnCardTrashed` `InstalledCard` `first_each_turn`, `Trace` → `Unpreventable` tag |
| Adept | V | an `Amount` for the Runner's unused MU, read in `ContinuousKind::Strength` |
| Savant | V | the unused-MU `Amount` |
| Deep Data Mining | V | the unused-MU `Amount` capped at 4 (a minimum of two amounts) |
| Blackstone | V | a `Cost` word: credits of which at least 1 comes from a stealth card |
| Fawkes | V | the at-least-1-stealth `Cost`, with X |
| Houdini | V | the at-least-1-stealth `Cost` |
| Net Mercur | V | a first-per-run `OnCreditsSpentFromInstalledCard` on stealth; hosted credits that pay for anything (a `PaysFor` word) |
| Golden | V | a `Cost` returning this program to the grip (the Runner's `AddSelfToHq`); "fully broke during this encounter" by this program |
| Peregrine | V | as Golden |
| Saker | V | as Golden |
| Citadel Sanctuary | V | a trace's "if unsuccessful" branch; a `Cost` that trashes the whole grip |
| Hellion Beta Test | V | a trace's "if unsuccessful" branch; `TimesLastTurn(OnTrashedFromAccess)` |
| Net Quarantine | V | the Runner's link treated as 0 for the turn's first trace; a `Trigger` on the Runner's link bid |
| Fumiko Yamamori | V | a `Trigger` on revealed psi bids (`GameEvent::PsiBidsRevealed`), `when` the bids differ |
| Hyoubu Research Facility | V | the psi-bid `Trigger` with `first_each_turn`, an `Amount` for the Corp's revealed bid |
| Enforcing Loyalty | V | a `CardFilter` for a card not of the Runner's identity's faction |
| K. P. Lynn | V | a `Trigger` for passing all the ice protecting a server (or approaching it after ice) |
| Polyhistor | V | the all-ice-passed `Trigger` on HQ, `first_each_turn` |
| The Gauntlet | V | an `Amount` for ice protecting HQ fully broken this run |
| Chief Slee | V | an `Amount` for the encountered ice's unbroken subroutines at `OnEncounterEnded` |
| Seidr Laboratories: Destiny Defined | V | a `Trigger` for the Runner's click lost or spent during a run, `first_each_turn` |
| Weyland Consortium: Builder of Nations | V | `first_each_turn` on `OnEncounterEnded` narrowed to advanced ice (a turn-log class or fact) |
| Şifr | V | a hand-size `Lingering` until the next turn begins; ice strength set to 0 for the encounter |
| Spear Phishing | V | an `EffectRequirement` for the innermost ice of the run's server (Inside Job's bypass) |
| Charlatan | V | a paid bypass promised at approach of the first rezzed ice, by `EncounteredIceStrength` |
| Injection Attack | V | a boost on a chosen icebreaker for the run the event then makes |
| Top Hat | V | a breach replacement accessing one chosen card of the top 5 of R&D (by position, unseen) |
| Credit Crash | V | a run rider trashing the first non-agenda accessed, which the Corp may pay its printed cost to prevent |
| Recon Drone | V | a `Prevent(Damage)` limited to damage from the card being accessed, X cost |
| Tracker | V | a `Preventable` for a subroutine about to resolve, for the first one in the run (Shred's lingering prevention shape) |
| Cold Read | V | a `CardFilter` for programs used during the run, at the run's end, unpreventable trash |
| Maxwell James | V | an `EffectRequirement`: the paid ability window after a successful HQ run ended |
| Mirror | V | an effect replacing one spent recurring credit (`payment::place_recurring` for one) |
| Find the Truth | V | a moment for each card the Runner draws, revealed; R&D look on `first_each_turn` successful run |
| The Shadow Net | V | an effect playing an event from the heap ignoring costs; `Cost::Forfeit` |
| Brute-Force-Hack | V | an event with an X play cost (Corporate Troubleshooter's X is an operation's), a filter by rez cost ≤ X |
| Hernando Cortez | V | a rez-cost increase on ice by its own subroutine count, while the Corp has 10[credit] |
| Hasty Relocation | V | a `Cost` trashing the top card of R&D as an additional play cost |
| Nihongai Grid | V | a swap between a card among R&D's top 5 and a card in HQ |
| Alexa Belsky | V | a Runner-paid number keeping that many random HQ cards out of a shuffle |
| Successful Field Test | V | an install from HQ ignoring costs repeated until the Corp stops |
| Enforced Curfew | M | current (Corp): `HandSize` −1 while it stays in play |
| Scarcity of Resources | M | current (Corp): `InstallCost` +2 on resources |
| Service Outage | M | current (Corp): `RunCost` 1[credit] on the turn's first run |
| Door to Door | M | current (Corp): Runner turn-start `Trace` with a tagged branch |
| Interdiction | M | current (Runner): no non-ice rez on the Runner's turn |
| Rumor Mill | M | current (Runner): unique non-region assets and upgrades lose their abilities |
| System Outage | M | current (Runner): Corp draws after the first each turn cost 1[credit] |
| Government Investigations | M | current (Runner): a bid of 2 forbidden |
| Another Day, Another Paycheck | M | current (Runner); a trace the Runner's card forces, its "if unsuccessful" branch |
| On the Lam | M | an event converted into a condition counter hosted on a resource, with an interrupt |
| Wetwork Refit | M | an operation converted into a condition counter on bioroid ice, granting a first subroutine |
| Chrysalis | M | an accessed, uninstalled ice encountered from HQ or R&D |
| Herald | M | encountering accessed ice; `ChooseNumber` advancement placement |
| Sapper | M | encountering accessed ice |
| Armored Servers | M | an additional cost to jack out and to break a subroutine, for the run |
| Mind Game | M | a psi game moving the run to a chosen other server; an additional cost to jack out |
| Baba Yaga | M | a program gaining the paid abilities of the icebreakers it hosts |
| Skorpios Defense Systems: Persuasive Power | M | an interrupt replacing any Runner trash with set aside, then remove 1 from the game |
| Encore | M | an additional turn |

**New mechanics (M):**

- **Currents: a played card that stays until another current is played, or an agenda is scored (a Runner current) or stolen (a Corp current).** Cards: Enforced Curfew, Scarcity of Resources, Service Outage, Door to Door, Interdiction, Rumor Mill, System Outage, Government Investigations, Another Day, Another Paycheck. Rules: CR 3.5.1b, CR 3.7.1b, CR 8.6.6c. Near relative built: Lockdown (UR 7a: `CorpState::play_area`, active there, `not_trashed_until_your_next_turn`); a current also needs a Runner play area and a trash on another current's play or a score/steal. What each current says once it stands composes (`HandSize`, `InstallCost`, `RunCost`, `Prohibition::Rez`, `LosesAbilities`), except Government Investigations' forbidden bid of 2 (psi games, RWR 7b) and Another Day's trace with an "if unsuccessful" branch.
- **Condition counters: an event or operation converted into a counter hosted on a card, with abilities of its own.** Cards: On the Lam (on a resource; an interrupt preventing 3 tags or 3 damage), Wetwork Refit (on rezzed bioroid ice; a subroutine before the printed ones). Rules: CR 1.9.5j, CR 1.13.6d, CR 3.5.1a, CR 3.7.1a, CR 8.6.6b, CR 10.1.4. Near relatives built: Hosting in general (VP 7b, 7c; RWR 3d), and for Wetwork Refit's granted subroutine, Additional subroutines, ordered (VP 7e; MS 8a, CR 9.8.2).
- **Encountering ice that is accessed and not installed.** Cards: Chrysalis, Herald, Sapper (each "when the Runner accesses this ice anywhere except in Archives, they encounter it"; the reveal in R&D is `RevealedWhileAccessed`, already built). Rules: CR 9.1.8h, CR 6.1.3. Near relative built: Forced or repeated encounter (RWR 7d; UR 7b, `RunState::suspended`, Ganked!'s encounter from an access), which encounters an installed piece of ice.
- **An additional cost to jack out, and to break a subroutine, for the remainder of a run.** Cards: Armored Servers (trash 1 grip card to jack out or break), Mind Game (add 1 installed card to the bottom of the stack to jack out; it also moves the run to a chosen other server, which is `MoveRunToOutermost` with Tsakhia's chosen server). Rules: CR 1.16.10, CR 6.1.5. Near relative built: Additional costs imposed by another card (VP 3a steal and score; RWR 2a trash; UR 8 run), none on a jack-out or a break.
- **A card gaining the paid abilities of the cards it hosts.** Card: Baba Yaga. Rules: CR 9.1.9, CR 1.13.10. Near relatives built: Losing abilities (PH 8, `ContinuousKind::LosesAbilities`; DF 8 Direct Access) for the reverse, and program hosting (Djinn's `MayHost`, Matryoshka's hosted copies, PH 7) for the host.
- **Replacing the trash of Runner cards with a set-aside, then removing one from the game.** Card: Skorpios Defense Systems: Persuasive Power (an interrupt on any batch of Runner cards about to be trashed, from any zone, once per turn when it removes a card). Rules: CR 4.8.1, CR 4.9.2, CR 9.2.9. Near relatives built: Set aside (RWR 6d; UR 6 `CardTarget::SetAside`), Runner cards removed from the game (RWR 2b), and the interrupt window of `rules::prevention` — which prevents a trash of an install by a card's text, not every trash.
- **An additional turn.** Card: Encore. Rules: the committed CR has no rule naming an extra turn; the turn structure is CR 5.1.2 and CR 5.1.3. No near relative built; MirrorMorph's extra action (DF 8) is the closest, inside one turn.

## F6. Mumbad, and Data and Destiny — 167 cards (C 87 / V 55 / M 25)

| Card | Class | What it needs |
|---|---|---|
| Akshara Sareen | C | two `ContinuousKind::AllottedClicks` under `Scope::Player`, one per side (Basilar Synthgland's word) |
| Bhagat | C | `OnSuccessfulRun` `when` HQ, `first_each_turn`, `Mill` of R&D |
| Councilman | C | `OnRez` heard by the Runner, `OfferPaidChoice` of `CreditsAmount(TriggeringCardPrintedCost)`, `DerezCard`, `Prohibit` `Rez` on that install for the turn (Mitosis) |
| Dr. Lovegood | C | `OnTurnStart`, a selection over own installs, `LoseAbilities` until `Turn` (Klevetnik) |
| Emptied Mind | C | `OnTurnStart`, `EffectIf` on `CardsInHand` 0, `GainClicks` |
| Populist Rally | C | `play_requirement` a seedy install, `AllottedClicksNextTurn(Corp, -1)` (Caveat Emptor) |
| Sports Hopper | C | `Link` 1, `[trash]` cost, `DrawCards` 3 |
| Technical Writer | C | `OnCardInstalled` over own hardware and programs, `AddCounters` credit, `TakeAllCountersAsCredits` |
| Temple of the Liberated Mind | C | power counters, `Cost::RemoveCounters`, `GainClicks`, `DuringYourTurn` and `OncePerTurn` |
| Windfall | C | `ShuffleIntoDeck`, `Mill` own stack, gain the trashed card's printed cost (at most an `Amount` over `TrashedThisWay`) |
| Ankusa | C | `OnIceFullyBroken` `ByThis` over a barrier, `AddToHand` of the ice (Lobisomem's moment) |
| Brahman | C | `OnEncounterEnded`, `BrokeASubroutineThatEncounter`, a selection of a non-virus program to the top of the stack |
| Brain Chip | C | `Memory` and `HandSize` with `Number` of `Amount::AgendaPoints`; console limit |
| Heartbeat | C | `Prevent(Damage)` paid by `Cost::Trash` of an own install (Net Shield's window); console |
| Multithreader | C | `recurring_credits` 2, `PaysFor::Using(CardType Program)` (The Toolbox's word) |
| NetChip | C | `MayHost` and `HostsMemory` (Djinn), the program filter at most a new memory-cost filter counted by copies |
| Security Chip | C | `[trash]` during a run, a selection of one icebreaker or any number of cloud icebreakers, `BoostStrengthAmount` of `Amount::Link` for the run |
| Globalsec Security Clearance | C | `install_requirement` on `Link`, `OnTurnStart` `OfferPaidChoice` of `LoseClicks`, `LookAtTopOfDeck` |
| Bazaar | C | `OnCardInstalled` of own hardware from the grip, `InstallRunnerCardFromGrip` of the same name (at most a filter) |
| Chatterjee University | C | power counters, `InstallRunnerCardFromGripWithDiscount` by `HostedCounters`, `RemoveCounters` |
| CBI Raid | C | `SetAccessReplacement` on HQ with a `Repeat` of a Corp selection from HQ to the top of R&D (the order of their choice) |
| Fear the Masses | C | `SetAccessReplacement`, a revealing selection of copies from the grip, `Mill` R&D by 1 + `CardsSelected` |
| High-Stakes Job | C | a run on a server with unrezzed ice (at most a server filter beside Kompromat's protected-by-ice), Bravado's run-end rider on success |
| Exclusive Party | C | `DrawCards`, `GainCreditsAmount` of `InZone` heap copies of this card (at most a same-name filter) |
| Making an Entrance | C | `NoActionTakenThisTurn` (In the Groove), arrange the top 6 with a trash (VP 6a's arrange) |
| Out of the Ashes | C | `InitiateRun`; a `from_discard` turn-start trigger with `Cost::RemoveSelfFromGame` (Subliminal Messaging's) |
| Patron | C | Security Testing exactly: `ChooseServer`, `SetAccessReplacement` drawing 2, `SpendChosenServer` |
| The Turning Wheel | C | `OnRunEnded` over HQ or R&D, `Not(StoleAgendaDuringLastRun)`, counters; `AddAdditionalAccess` by choice |
| Spy Camera | C | arrange top X of the stack (X = `RunnerInstalls` copies); `[trash]`: `LookAtTopOfDeck` R&D |
| Independent Thinking | C | an up-to-5 selection of own installs trashed, draws by `CardsSelected`, doubled under `EffectIf` on a directive among `TrashedThisWay` |
| Artist Colony | C | `Cost::Forfeit`, a search of the stack for a program, resource or hardware, installed |
| Jesminder Sareen: Girl Behind the Curtain | C | `Prevent(Tags)` of 1 under `OncePerRun`; Sweep deck |
| Liberated Chela | C | five clicks and `Cost::Forfeit`, the Corp's `OfferPaidChoice` of `Forfeit`: `RemoveFromGame` or `AddToScoreAreaAsAgenda` worth 2 |
| Nero Severn: Information Broker | C | `OnEncounter` `when` a sentry, `OncePerTurn`, `OfferPaidChoice` of `Cost::JackOut` (Karunā); Sweep deck |
| Political Operative | C | `install_requirement` on `TimesThisTurn` of a successful HQ run, `[trash]` + `CreditsX`, a trash-cost-equals-X filter |
| Wasteland | C | `OnCardTrashed` `when` an own `InstalledCard`, `first_each_turn` |
| Sunny Lebeau: Security Specialist | C | no text, link 2; Sweep deck |
| Assassin | C | two `Trace` subroutines, net damage and a program trash |
| Bailiff | C | `OnSubroutineBroken` `subject: This`, `GainCredits` |
| Brainstorm | C | Starlit Knight exactly: `OnEncounter` `GainSubroutine` for the run with `count` `CardsInHand` |
| Chetana | C | `PsiGame` (Konjin) with `DealDamageAmount` of `CardsInHand`; both players gain 2 |
| Cobra | C | program trash and net damage |
| Red Tape | C | `ModifyStrength` of `EachIce` for the run |
| Resistor | C | `Strength` of `Number` per `RunnerTags`; `Trace` to end the run |
| Special Offer | C | `GainCredits` 5, `TrashCard` this |
| Turnpike | C | `OnEncounter` `LoseCredits`; `Trace` for a tag |
| Upayoga | C | `PsiGame`; `ResolveSubroutineOfSelectedIce` over rezzed psi ice (Mycoweb) |
| Vanilla | C | `EndTheRun` |
| 15 Minutes | C | Oracle Thinktank's ability from the Runner's score area (`InRunnersScoreArea`), `AddToDeck` + `ShuffleIntoDeck` |
| Advanced Concept Hopper | C | `OnRunStart` heard from the score area, `first_each_turn`, `PresentChoice` |
| Corporate Sales Team | C | `OnAgendaScored` credit counters, `OnTurnStart` for both sides taking 1 |
| Global Food Initiative | C | `AgendaPoints` −1 under `InRunnersScoreArea` (Project Vacheron's shape) |
| Improved Protein Source | C | `OnAgendaScored` and `OnAgendaStolen`, `GainCredits` Runner 4 |
| Merger | C | `AgendaPoints` +1 under `InRunnersScoreArea` |
| New Construction | C | `installs_faceup`, `OnAdvance` subject This, `PromptInstallCorpCard` into a new remote, rez under `EffectIf` on `HostedAdvancementTokens` |
| Puppet Master | C | `OnSuccessfulRun` heard from the score area, `PlaceAdvancementCounters` on an `Advanceable` card |
| Remote Data Farm | C | `HandSize` +2 (Precision Design) |
| Voting Machine Initiative | C | agenda counters on score, the Runner's `OnTurnStart` heard by the Corp, `LoseClicks` |
| Bio-Ethics Association | C | `OnTurnStart`, `Not(AmountAtLeast(IceProtectingThisServer, 1))`, net damage |
| Clone Suffrage Movement | C | the same condition, a selection of an operation from Archives to HQ |
| Commercial Bankers Group | C | the same condition, `GainCredits` 3 |
| Sensie Actors Union | C | the same condition, `DrawCards` 3 then a card from HQ to the bottom of R&D |
| Launch Campaign | C | credit counters on rez, take 2 each turn, trash when empty (Adonis Campaign) |
| Reality Threedee | C | `OnRez` `GiveBadPublicity`, `OnTurnStart` `EffectIf` `IsTagged` |
| Pālanā Agroplex | C | `OnTurnStart` `DrawCards` for both sides |
| Kala Ghoda Real TV | C | `LookAtTopOfDeck` the stack; `[trash]`: `Mill` the stack |
| Mumbad Construction Co. | C | `PlaceAdvancementCounters` on itself; move one to a faceup card (Trick of Light's move) |
| Advanced Assembly Lines | C | `OnRez` gain 3; `[trash]` `PromptInstallCorpCard` from HQ under `Not(RunInProgress)` |
| Ad Blitz | C | X operation cost (Corporate Troubleshooter), `additional_play_cost` click, `Repeat` of an install-and-rez of an advertisement from HQ or Archives |
| Dedication Ceremony | C | `PlaceAdvancementCounters` on a `Faceup` card, `Prohibit` `ScoreAgendas` on that install for the turn (Mitosis) |
| Election Day | C | a selection of all of HQ to Archives (`play_requirement` HQ ≥ 2 with this card), `DrawCards` 5 |
| Lateral Growth | C | `GainCredits` 4, an optional `PromptInstallCorpCard` |
| Localized Product Line | C | a search of R&D for one card and its copies (at most a same-name filter), to HQ, shuffle |
| Subcontract | C | `play_requirement` `IsTagged`, `PlayOperation { from: OwnHq }` twice |
| The All-Seeing I | C | `IsTagged`, the Runner's `PresentChoice` of `RemoveBadPublicity` or a `ForEach` trash of resources |
| Voter Intimidation | C | `ZoneHasAtLeast` the Runner's score area, `PsiGame`, a resource trash |
| Mumbad City Hall | C | a search of R&D for an alliance card, `PlayOperation` or `PromptInstallCorpCard` from R&D, shuffle |
| Lily Lockwell | C | `OnRez` `DrawCards` 3; `[click]` + `RemoveTags` cost, a revealed search of R&D to its top |
| Archangel | C | `RevealedWhileAccessed`, Ganked!'s `ForceEncounter` from an access behind a paid choice, `Trace` returning an install to the grip |
| Disposable HQ | C | `RevealedWhileAccessed`, `OnAccessed` selection of any number from HQ to the bottom of R&D |
| Keegan Lane | C | `[trash]` + `RemoveTags` cost, program trash, `DuringRunOn` this server |
| Mumbad City Grid | C | `OnIcePassed` over ice protecting this server, `SwapInstalledIce` (Sipa) |
| Surat City Grid | C | `OnRez` `when` `InThisServer`, `RezInstalled` with a 2-credit `Discount` |
| News Team | C | Nightmare Archive exactly: take 2 tags or `AddToScoreAreaAsAgenda` worth −1 |
| Victoria Jenkins | C | `AllottedClicks` −1 for the Runner; `OnTrashedFromAccess` into the Runner's score area worth 2 (PH 7) |
| Zealous Judge | C | `rez_requirement` `IsTagged`; `[click]`, 1 credit: `GiveTags` |
| Spark Agency: Worldswide Reach | C | `OnRez` `when` an advertisement, `first_each_turn`, `LoseCredits` Runner; Sweep deck |
| Consulting Visit | V | alliance waiver; otherwise composes (`additional_play_cost` click, `PlayOperation` from R&D) |
| Executive Search Firm | V | alliance waiver; a revealed search of R&D by subtype |
| Heritage Committee | V | alliance waiver; draw 3, one card from HQ to the top of R&D |
| Ibrahim Salem | V | alliance waiver; a `rez_alternatives` cost of `Forfeit` (Bloop's shape), a named type trashed from the grip (Vera Ivanovna Shuyskaya) |
| Jeeves Model Bioroids | V | alliance waiver; Wage Workers' `TimesThisActionThisTurn` at 3, `first_each_turn` |
| Museum of History | V | alliance waiver (deck size 50+); shuffle a card from Archives into R&D |
| PAD Factory | V | alliance waiver (3 PAD Campaigns); Dedication Ceremony's place-and-no-score |
| Product Recall | V | alliance waiver; trash a rezzed asset or upgrade, gain its trash cost (at most an `Amount`) |
| Salem's Hospitality | V | alliance waiver; `ChooseCardName`, `RevealHand`, trash revealed cards of that name |
| Mumba Temple | V | alliance waiver (15 or fewer ice); a `PaysFor` word for rezzing anywhere (only `RezzingInThisServer` exists) |
| Aryabhata Tech | V | a `Trigger` for a successful trace (any trace) |
| Improved Tracers | V | a `ContinuousKind` for base trace strength; tracer ice +1 is `Strength` over `Ice(HasSubtype Tracer)` |
| Rutherford Grid | V | the same base trace strength word, `Scope::RunsOnThisServer` |
| Waiver | V | an `Amount` for the margin of the trace's success, read in `on_success`; `RevealHand` and a cost filter |
| Security Nexus | V | `Trace` with an "if unsuccessful" branch (it has `on_success` only), started from a Runner card; bypass |
| Vikram 1.0 | V | a `Prohibition` on using programs for the run; Ansel's lose-a-click break; two traces |
| Jak Sinclair | V | the same programs `Prohibition` for the run it makes; install cost less per `Link` |
| Navi Mumbai City Grid | V | a standing `Prohibition` on paid abilities of installed Runner cards but icebreakers' and mid-access ones, on runs on this server |
| EMP Device | V | a `Prohibition` on rezzing more than one piece of ice for the run (a rez count, beside `Rez`) |
| The Price of Freedom | V | a `Prohibition` on advancing for the Corp's next turn; additional cost of a connection trash; removed from the game |
| The Black File | V | a `Prohibition` on the Corp winning but by flatline (CR 1.7.2); turn-start counters, removed at 3 |
| Always Be Running | V | a `Cannot` on the first click unless a run or run event (Front Company's `first_each_turn` shape); `LoseClicks` break |
| Lakshmi Smartfabrics | V | a `Cost` of X hosted counters with X read by the effect; a `Prohibition` on stealing an agenda by name for the turn |
| Interrupt 0 | V | a lingering additional cost on icebreaker breaks for the run (a `Lingering` cost beside `RezCost`) |
| Diwan | V | an additional install cost for the Corp in a chosen server remembered while installed (`Remember` a server); trash on purge |
| Pālanā Foods: Sustainable Growth | V | a `Trigger` for cards drawn (only the basic draw action and the about-to-resolve are heard), `first_each_turn`; Sweep deck |
| Political Dealings | V | the drawn-cards `Trigger` with `when` an agenda, reveal and install that card |
| Personality Profiles | V | a `Trigger` for the Runner searching the stack, beside an install from the heap (an `EventFilter` like `InstalledFromHq`) |
| Shannon Claire | V | a draw from the bottom of R&D; `[trash]` search R&D or Archives for an agenda to R&D's bottom |
| Harvester | V | an `Amount` for maximum hand size ("discard down to" as a selection of `CardsInHand` reduced by it) |
| Safety First | V | the maximum hand size `Amount`; `HandSize` −2; a turn-end draw |
| Reflection | V | a `Trigger` for a jack-out; `RevealAtRandom` from HQ; console |
| Tech Trader | V | an `EventFilter` on `OnAbilityUsed` for an ability whose cost has a [trash] |
| Guru Davinder | V | a `Trigger` for damage this card prevented; `Prevent(Damage)` of all net or meat |
| Maya | V | a `Trigger` for finishing an access in R&D, moving that card to R&D's bottom with a tag; console |
| GS Sherman M3 | V | a `ContinuousKind` for a program's own memory cost (0 while `Link` ≥ 2, grip included); break all barrier subroutines |
| GS Shrike M2 | V | the memory cost word; sentries |
| GS Striker M1 | V | the memory cost word; code gates |
| Mongoose | V | a break limit of one piece of ice per run on the breaker (`BreakLimit` is the ice's, per encounter) |
| Endless Hunger | V | a filter for an "End the run" subroutine on a break; `Cost::Trash` of an install |
| Panchatantra | V | a subtype choice beyond barrier, code gate and sentry (`IceType` has three and `Other`; Pelangi's known limit) |
| TL;DR | V | the next encounter's ice gains a copy of each of its subroutines (a `GainSubroutine` that copies, armed for the next encounter) |
| Rebranding Team | V | a `ContinuousKind` giving a card subtype to non-ice (`GainSubtype` takes an `IceType`) |
| Indian Union Stock Exchange | V | a `CardFilter` for out-of-faction cards; `OnRez` and `OnCardPlayed` |
| Run Amok | V | a `CardFilter` for ice rezzed during this run, read at its end |
| Neutralize All Threats | V | an access word: must trash, paying, if able; reveal; first time each turn; +1 HQ access |
| Mumbad Virtual Tour | V | the must-trash access word, while installed; alliance waiver (7 assets) |
| Salsette Slums | V | an access-time paid ability that pays the trash cost to remove the card from the game instead |
| Quantum Predictive Model | V | an accessed agenda added to the Corp's score area (`AddToScoreAreaAsAgenda` goes to the Runner's); `IsTagged` |
| Exchange of Information | V | an `Effect` swapping agendas between score areas (CR 8.8.4c), no new machinery |
| Prey | V | a once-this-run pass trigger made by the event (RWR 5d), `Cost::Trash` of N installs with N the passed ice's strength |
| The Noble Path | V | a lingering prevention of all damage for the run (`Lingering::PreventRunEnding`'s shape for damage); trash the grip |
| Ramujan-reliant 550 BMI | V | an `Amount` for damage prevented, X from installed copies; `Mill` own stack |
| Rigged Results | V | a secret spend the Corp guesses (a `PsiGame` sibling decision); Femme Fatale's bypass of the chosen ice |
| SYNC: Everything, Everywhere | V | a basic-action cost word for removing a tag beside `BasicTrashCost`, and a reduction of that one; `FlipIdentity`; Sweep deck |
| "Clones are not People" | M | a current; on score, into the score area as an agenda worth 1 |
| Corporate Scandal | M | a current; the Corp considered to have 1 more bad publicity |
| Employee Strike | M | a current; the Corp identity loses its printed abilities |
| "Freedom Through Equality" | M | a current; on steal, into the score area as an agenda worth 1 |
| Surveillance Sweep | M | a current; the Runner bids first in a trace during a run |
| News Hound | M | reads "a current is active" to gain an end-the-run subroutine |
| New Angeles Sol: Your News | M | plays a current from HQ or Archives on a score or steal; Sweep deck |
| Media Blitz | M | a current that gains the text of an agenda in the Runner's score area |
| Apex: Invasive Predator | M | Runner cards installed facedown; non-virtual resources barred; Sweep deck |
| Harbinger | M | turned facedown instead of trashed, still trashed |
| Hunting Grounds | M | install the top 3 of the stack facedown; its prevention is `Preventable::EncounterAbility` (AirbladeX) |
| Apocalypse | M | turn every installed Runner card facedown; the Corp half is a `ForEach` trash |
| Aghora | M | swap an installed program with one from the grip; a printed-cost filter on the break |
| Sadyojata | M | the deva swap; an `Amount` for the encountered ice's subtype count |
| Vamadeva | M | the deva swap; the break reads `EncounteredIceSubroutines` |
| Adam: Compulsive Hacker | M | three directives from outside the deck begin installed; Sweep deck |
| Rebirth | M | switch the identity for another of the faction, from a pile outside the game |
| Dedicated Neural Net | M | the Corp chooses which HQ cards are accessed; psi on the first successful HQ run |
| Information Sifting | M | the Corp splits HQ into two piles, the Runner accesses one |
| Street Magic | M | the Runner orders the unbroken subroutines |
| 24/7 News Cycle | M | resolve another agenda's "when scored" ability; forfeit as an additional cost |
| Political Graffiti | M | an event converted into a condition counter hosted on a scored agenda |
| Full Immersion RecStudio | M | a Corp asset hosting installed assets and agendas; trash cost by `HostedCards` |
| Harishchandra Ent.: Where You're the Star | M | the grip revealed while the Runner is tagged, a standing change to the view's masking; Sweep deck |
| Raman Rai | M | swap a drawn card with a card in Archives; alliance waiver |

**New mechanics (M):**

- **Currents: a played card that stays active until another current is played or an agenda is scored (event) or stolen (operation).** "Clones are not People", Corporate Scandal, Employee Strike, "Freedom Through Equality", Surveillance Sweep, Media Blitz; News Hound reads "a current is active"; New Angeles Sol: Your News plays one from HQ or Archives. CR 3.5.1b, 3.7.1b, 8.6.6c. Near relative: **Lockdown** (UR 7a, `CorpState::play_area`, active there) — a current needs the Runner's side of the play area too, and an end heard as an event (a current played, a score, a steal) rather than as the Corp's next turn.
- **Gaining another card's text.** Media Blitz ("gains the text of that agenda"). CR 9.1.9. Near relative: **Losing abilities** (PH 8, `ContinuousKind::LosesAbilities`) is the reverse; nothing gains a printed ability set.
- **The Runner spends first in a trace.** Surveillance Sweep. CR 10.8.2, 10.8.3 (the Corp acts first by default). Near relative: none in the table; `Effect::Trace` and `rules/trace.rs` (UR 5) fix the order.
- **Runner cards installed or turned facedown** (inactive, no name or type while facedown). Apex: Invasive Predator, Harbinger, Hunting Grounds, Apocalypse. CR 8.1.4, 4.6.4d, 1.8.4b. Near relative: **Hosting in general** (VP 7b/7c; PH 7 "turned facedown") covers facedown *hosted* cards only; nothing installs a Runner card facedown in the rig.
- **Swapping an installed program with one in the grip** (the deva ability). Aghora, Sadyojata, Vamadeva. CR 8.2.11, 8.8.4b (the uninstalled one's hosted cards trashed, the other installed). Near relative: none in the table; Project Yagi-Uda (DF 7) swaps a Corp card from HQ into a server, and is the shape to follow.
- **Directives: cards from outside the deck that begin the game installed.** Adam: Compulsive Hacker. CR 1.5.3, 1.5.3a, 1.5.3b, 1.5.3c, 1.5.3d. Near relative: Méliès U's copies outside the game (VP 8, CR 1.5.2b), and Ayla's pre-game set-aside (reprints 11w) for a step before the starting hand.
- **Switching the Runner's identity mid-game** for one from a pile outside the game. Rebirth. CR 1.5.4, 1.5.4a, 1.5.4d. Near relative: Méliès U's secret identity copy (`SetIdentityCopy`, VP 8), which swaps among copies of one card.
- **The Corp chooses which cards are accessed from HQ.** Dedicated Neural Net. CR 7.3.4b (and 7.3.4a for the default). Near relative: none; its trigger is a psi game (RWR 7b), built.
- **An access of a set of cards made without a breach, from piles the Corp makes.** Information Sifting. CR 7.1.9. Near relative: none in the table; `Effect::Access` and Cataloguer's run-less breach (RWR 6c) are the access steps it would reuse.
- **The Runner orders unbroken subroutines.** Street Magic. CR 9.8.7d, 9.8.7b. Near relative: **Additional subroutines, ordered** (VP 7e … MS 8a) orders what is *added*; resolution order is fixed.
- **Resolving another card's "when scored" ability.** 24/7 News Cycle. CR 9.6.14c, 9.6.14d. Near relative: none in the table; Mycoweb's `ResolveSubroutineOfSelectedIce` is the subroutine counterpart. Its forfeit is `additional_play_cost` with `Cost::Forfeit`, built.
- **An event converted into a condition counter hosted on a scored agenda.** Political Graffiti. CR 1.13.6d, 1.9.5j, 10.1.4. Near relatives: **A card added to a score area "as an agenda"** (VP 7d, a conversion) and **Hosting in general**; nothing converts a card into a counter.
- **A Corp card hosting installed Corp assets and agendas** (installed onto it, advanced and scored there, trashed with it). Full Immersion RecStudio. CR 1.13.5. Near relative: **Hosting in general** (VP 7b/7c), which is the Runner's rig; no Corp card hosts an installed Corp card.
- **The grip revealed while the Runner is tagged** — a standing exception to the hand's secrecy, read by the masking. Harishchandra Ent.: Where You're the Star. CR 4.3.2, 1.21.3. Near relative: **Reveal as a step other cards can read** (VP 6a; RWR 6b "stays revealed") and UR 6's `RevealHand`, which reveal for the length of one ability.
- **Swapping a drawn card with a card in Archives.** Raman Rai. CR 8.4.3b, 8.8.4. Near relative: Daily Business Show's parked draw (reprints 9b, `IncreaseAboutToResolve`), where the drawn cards wait set aside; a swap into that set is new.

## F7. SanSan, and Order and Chaos — 170 cards (C 104 / V 45 / M 21)

| Card | Class | What it needs |
|---|---|---|
| Amped Up | C | GainClicks and Stimhack's unpreventable core damage |
| Day Job | C | additional_play_cost of three clicks, GainCredits |
| Fisk Investment Seminar | C | NoActionTakenThisTurn (In the Groove), DrawCards for each side |
| Power to the People | C | NoActionTakenThisTurn; LaterThisTurn on OnAccessed, agenda filter, once |
| Uninstall | C | PromptChooseCards over own installs, program or hardware, to the grip |
| Drive By | C | additional_play_cost click; Expose then EffectIf asset or upgrade, TrashCard |
| Forked | C | InitiateRun; LaterThisTurn this_run on OnIceFullyBroken, sentry filter, trash it |
| Knifed | C | as Forked, barrier |
| Spooned | C | as Forked, code gate |
| Wanton Destruction | C | SetAccessReplacement (Account Siphon), ChooseNumber of clicks, random trashes from HQ |
| Beach Party | C | OnTurnStart LoseClicks; HandSize +5 |
| Drug Dealer | C | two OnTurnStart triggers, one heard on the Corp's turn |
| Enhanced Vision | C | OnSuccessfulRun first_each_turn, RevealAtRandom from HQ |
| Fan Site | C | OnAgendaScored heard by the Runner, AddToScoreAreaAsAgenda worth 0 (Jeitinho) |
| Human First | C | OnAgendaScored and OnAgendaStolen, Subject Any, GainCreditsAmount of AgendaPoints |
| Investigative Journalism | C | install_requirement on BadPublicity; four clicks and TrashSelf, GiveBadPublicity |
| Symmetrical Visage | C | OnBasicDrawAction first_each_turn, GainCredits |
| Synthetic Blood | C | OnDamageSuffered first_each_turn, DrawCards |
| Stim Dealer | C | OnTurnStart EffectIf on ThisCardCountersAtLeast(2): counters, unpreventable core damage, or a click |
| Wireless Net Pavilion | C | BasicTrashCost of 2 credits about This (Manuel Lattes de Moura's kind) |
| Bookmark | C | hosts_facedown; PromptChooseCards from the grip to HostedOnSource; AddToHand of hosted cards |
| Street Peddler | C | HostCardOnThisCard TopOfStack facedown; TrashSelf, InstallRunnerCardFromZone with a discount |
| London Library | C | InstallProgramOnHost ignoring cost; trash hosted at the turn's end; hosted to grip |
| Off-Campus Apartment | C | MayHost connections (Hackerspace); OnInstall with an EventFilter::Host, DrawCards |
| Rolodex | C | LookAtTopOfDeck arrange (Cultivate); OnCardTrashed subject This, Mill own stack 3 |
| Sacrificial Clone | C | interrupt Prevent all damage with TrashSelf, then a Sequence of trashes, credit loss and RemoveTags |
| Tyson Observatory | C | two-click ability, PromptChooseCards hardware from the stack, shuffle |
| Spoilers | C | OnAgendaScored heard by the Runner, Mill R&D 1 |
| Chop Bot 3000 | C | OnTurnStart OfferPaidChoice with Cost::Trash of another install, PresentChoice draw or RemoveTags |
| Brain Cage | C | HandSize +3; OnInstall core damage |
| Skulljack | C | Demolisher's TrashCost on Scope::Accessing; OnInstall core damage |
| Net-Ready Eyes | C | OnInstall meat damage; OnRunStart choose an icebreaker, BoostStrength for the run |
| Forger | C | Link +1; interrupt TrashSelf Prevent(Tags); TrashSelf RemoveTags; console |
| Edward Kim: Humanity's Hammer | C | OnAccessed first_each_turn, operation filter, TrashCurrentlyAccessedCard at no cost |
| Laramy Fisk: Savvy Investor | C | OnSuccessfulRun on a central first_each_turn, optional DrawCards(Corp) |
| MaxX: Maximum Punk Rock | C | OnTurnStart Mill own stack 2, DrawCards 1 |
| Valencia Estevez: The Angel of Cayambe | C | before_starting_hand GiveBadPublicity |
| Analog Dreamers | C | InitiateRun R&D, SetAccessReplacement, PromptChooseCards unrezzed non-ice unadvanced, ShuffleIntoDeck |
| DaVinci | C | OnSuccessfulRun AddCounters; TrashSelf, install from the grip with PrintedCostAtMost(HostedCounters) via last_known |
| Faust | C | break and pump paid with Cost::Trash from the grip |
| Gravedigger | C | OnCardTrashed, InstalledCard Corp filter, AddCounters; click and counter, Mill R&D 1 |
| Hyperdriver | C | OnTurnStart optional RemoveFromGame self and GainClicks 3 |
| Study Guide | C | Strength per HostedCounters; pump that places a counter |
| Trope | C | OnTurnStart AddCounters; Cost::RemoveSelfFromGame, heap cards per last_known counters, ShuffleIntoDeck |
| Virus Breeding Ground | C | OnTurnStart AddCounters; RemoveCounters then AddCounters on a card HostsCounters(Virus) |
| Asteroid Belt | C | advancement_requirement 0 (Ice Wall), RezCost per HostedAdvancementTokens |
| Nebula | C | as Asteroid Belt; trash a program |
| Wormhole | C | as Asteroid Belt; ResolveSubroutineOfSelectedIce (Ravana 1.0) |
| Orion | C | as Asteroid Belt; two GainSubtype entries for its other types; ResolveSubroutineOfSelectedIce |
| Fire Wall | C | Ice Wall's Strength per HostedAdvancementTokens |
| Spiderweb | C | three EndTheRun |
| Meru Mati | C | Strength while Protecting HQ |
| Gutenberg | C | Strength while Protecting R&D; Trace for a tag |
| Crick | C | Strength while Protecting Archives; PromptInstallCorpCard from Archives, paying |
| Quicksand | C | OnEncounter AddCounters; Strength per HostedCounters |
| Bandwidth | C | GiveTags; LaterThisTurn this_run on OnSuccessfulRun, RemoveTags |
| Checkpoint | C | OnRez bad publicity; Trace, LaterThisTurn this_run on OnSuccessfulRun, meat damage |
| Clairvoyant Monitor | C | PsiGame, PlaceAdvancementCounters, EndTheRun |
| Enforcer 1.0 | C | Archer's forfeit to rez; click_breakable; trash program, console, all virtual resources |
| Lab Dog | C | Runner-side PromptChooseCards over hardware, TrashCard self |
| Little Engine | C | EndTheRun twice, GainCredits(Runner, 5) |
| Negotiator | C | F2P's Runner-only 2[credit] break |
| NEXT Gold | C | NEXT Silver's count of rezzed NEXT ice |
| Pachinko | C | EffectIf IsTagged, EndTheRun |
| Tour Guide | C | ContinuousKind::Subroutines counted per rezzed asset (Echo) |
| Turing | C | Strength while ProtectingRemote; CannotBeBrokenUsing(Ai); OfferPaidChoice of three clicks |
| Award Bait | C | RevealedWhileAccessed; OnAccessed, ChooseNumber up to 2, PlaceAdvancementCounters |
| Explode-a-palooza | C | RevealedWhileAccessed; OnAccessed optional GainCredits |
| Firmware Updates | C | OnAgendaScored AddCounters 3; OncePerTurn RemoveCounters ability, advance ice |
| Genetic Resequencing | C | OnAgendaScored, selection in OwnScoreArea, AddCounters |
| Government Takeover | C | click ability from the score area; deck_limit 1 |
| High-Risk Investment | C | agenda counter ability, GainCreditsAmount of Credits(Runner) |
| Hollywood Renovation | C | installs_faceup; OnAdvance This, EffectIf counters at least 6, PlaceAdvancementCounters |
| Research Grant | C | OnAgendaScored, PromptChooseCards over installed copies by name, Score |
| Self-Destruct Chips | C | HandSize on Player(Runner) from the score area |
| The Future is Now | C | OnAgendaScored, PromptChooseCards from R&D to HQ, shuffle |
| Underway Renovation | C | installs_faceup; OnAdvance This, Mill the Runner's stack 1 or 2 |
| Vanity Project | C | no text |
| Allele Repression | C | advanceable asset; TrashSelf, Repeat of a swap read off last_known counters |
| Back Channels | C | PromptChooseCards in a remote root, GainCreditsAmount per HostedAdvancementTokens, TrashCard (realloc()'s acting card) |
| Capital Investors | C | click ability, GainCredits |
| Constellation Protocol | C | OnTurnStart, Trick of Light's move of advancement counters |
| Contract Killer | C | advanceable; ability behind AmountAtLeast(HostedAdvancementTokens, 2), PresentChoice |
| Early Premiere | C | OnTurnStart OfferPaidChoice 1[credit], PlaceAdvancementCounters on a root card |
| Exposé | C | advanceable; TrashSelf, RemoveBadPublicity repeated per last_known counters |
| Test Ground | C | advanceable; TrashSelf, Repeat DerezCard |
| Cybernetics Court | C | HandSize +4 |
| Public Support | C | OnRez AddCounters 3; OnTurnStart RemoveCounters, EffectIf none left, AddToScoreAreaAsAgenda |
| Ronald Five | C | OnCardTrashed, a Corp card trashed by the Runner, LoseClicks |
| Team Sponsorship | C | OnAgendaScored, PromptInstallCorpCard from Archives or HQ ignoring cost (Sleipnir's zone first) |
| Tech Startup | C | OnTurnStart optional TrashSelf, search R&D for an asset, install it, shuffle |
| Recruiting Trip | C | X cost (Corporate Troubleshooter), DifferentNames sysops from R&D |
| Traffic Accident | C | play_requirement on RunnerTags, meat damage |
| Breaker Bay Grid | C | RezCost on RootOfThisServer; region limit already enforced |
| Expo Grid | C | OnTurnStart EffectIf a rezzed asset in the root, GainCredits |
| Oaktown Grid | C | TrashCost on RootOfThisServer |
| Cyberdex Virus Suite | C | RevealedWhileAccessed; OnAccessed optional PurgeVirusCounters; TrashSelf purge |
| Product Placement | C | RevealedWhileAccessed; OnAccessed Not(AccessingIn Archives), GainCredits |
| Space Camp | C | RevealedWhileAccessed; OnAccessed optional PlaceAdvancementCounters |
| Marcus Batty | C | TrashSelf, PsiGame, ResolveSubroutineOfSelectedIce, DuringRunOn this server |
| Ryon Knight | C | TrashSelf core damage, RunAgainstThisServer and Not(RunnerClicksAtLeast(1)) |
| Argus Security: Protection Guaranteed | C | OnAgendaStolen, PresentChoice chooser Runner, tag or meat damage |
| Cybernetics Division: Humanity Upgraded | C | HandSize -1 on Controller and on Player(Runner) |
| Titan Transnational: Investing In Your Future | C | OnAgendaScored Subject Any, optional AddCounters on it |
| Adjusted Chronotype | V | a Trigger for a click lost, excepting a paid ability's cost, with first_each_turn |
| Ancestral Imager | V | a jack-out moment: RunJackedOut is heard only as OnRunEnded |
| Armand "Geist" Walker: Tech Lord | V | an EventFilter on OnAbilityUsed for an ability whose cost has [trash] |
| It's a Trap! | V | a Trigger for this card being exposed (only OnExposeAboutToResolve exists) |
| I've Had Worse | V | its own trash from the grip by net or meat damage heard (an EventFilter on the reason) |
| Mark Yale | V | a Trigger for an agenda counter spent, and a Cost removing a counter from another card |
| Chrome Parlor | V | an EventFilter on damage's source: a cybernetic's install ability |
| Muertos Gang Member | V | a Runner card's uninstalling heard (OnWouldBeUninstalled is Corp-only, Known limits) |
| Hayley Kaplan: Universal Scholar | V | a CardFilter for the same type as the triggering card |
| Paige Piper | V | a CardFilter for the same title as the triggering card |
| The Twins | V | the same title as the passed ice, as a cost; then ForceEncounter (Sisyphus Protocol) |
| Student Loans | V | PlayCost on Scope::Playing with a filter for a copy in the heap, as an additional cost |
| Franchise City | V | RevealedWhileAccessed about any agenda (Scope::Accessing), and OnAccessed Subject Any |
| The Board | V | AgendaPoints over every agenda in the Runner's score area; to the Runner's score area when trashed on access |
| Dedicated Technician Team | V | a PaysFor word for installing ice protecting this server |
| Brain-Taping Warehouse | V | an Amount for the Runner's unspent clicks (ClicksRemaining is the active side's) |
| Cortex Lock | V | an Amount for the Runner's unused MU |
| Data Folding | V | the same unused-MU Amount under AmountAtLeast |
| Game Day | V | an Amount for maximum hand size, less CardsInHand |
| Vigil | V | the same hand-size Amount, the Corp's, against CardsInHand(Corp) |
| Searchlight | V | Effect::Trace with an Amount base (it is a u32) |
| Satellite Grid | V | a ContinuousKind adding advancement counters that are not placed, read by HostedAdvancementTokens |
| DDoS | V | a Prohibition on rezzing the outermost ice during runs, for the turn |
| Eater | V | a Prohibition on accessing any card for the run (Access binds one install) |
| Haarpsichord Studios: Entertainment Unleashed | V | a Prohibition on stealing, under a while on TimesThisTurn(OnAgendaStolen) |
| Lockdown | V | a Prohibition on the Runner drawing, for the turn |
| Genetics Pavilion | V | the draw prohibition past 2 draws a turn, on a parked draw (The Class Act) |
| Blacklist | V | a Prohibition on cards leaving the heap, asked at every move out of it |
| Underway Grid | V | Prohibitions on bypassing and exposing, about this server |
| Old Hollywood Grid | V | a steal-only Prohibition about this server, excepting a title in the score area; persistent |
| Valley Grid | V | a lingering HandSize until the Corp's next turn (no Lingering hand size) |
| Dorm Computer | V | a lingering prevention of every tag for the run it makes |
| Qianju PT | V | a lingering prevention of the first tag until the next turn |
| Progenitor | V | a Preventable for virus counters purged; Djinn's MayHost and HostsMemory |
| Crowbar | V | a ContinuousKind for a program's own memory cost, read from the grip |
| Shiv | V | the same memory-cost kind |
| Spike | V | the same memory-cost kind |
| Gagarin Deep Space: Expanding the Horizon | V | a ContinuousKind for an additional cost to access a card in a remote root |
| Comet | V | a Runner half of PlayOperation: play an event from the grip without a click |
| Jinteki Biotech: Life Imagined | V | Méliès U's identity copies chosen openly before the first turn; FlipIdentity with three sides |
| Showing Off | V | a breach word: R&D accessed from the bottom (CR 7.4.7b) |
| Gang Sign | V | Breach of HQ outside a run with the root's cards not candidates |
| Builder | V | MoveThisIceToOutermost of a chosen server, not the attacked one |
| Surfer | V | SwapInstalledIce of the encountered barrier and a neighbour, the run's position following it |
| Film Critic | V | host an accessed agenda (HostedCardOrigin::AccessedCard), then add a hosted agenda to the score area (CR 1.17.3e) |
| Defective Brainchips | M | a current; IncreaseAboutToResolve on the first core damage each turn |
| Hacktivist Meeting | M | a current; an additional rez cost on non-ice paid from HQ at random |
| Housekeeping | M | a current; OnCardInstalled first_each_turn, the Runner trashes from the grip |
| Itinerant Protesters | M | a current; HandSize per BadPublicity |
| Predictive Algorithm | M | a current; StealCost 2[credit] |
| Traffic Jam | M | a current; AdvancementRequirement of every agenda per copies in the Corp's score area |
| Casting Call | M | an operation turned condition counter on the agenda it installs faceup |
| Patch | M | condition counter on rezzed ice |
| Sub Boost | M | condition counter on rezzed ice granting a subtype and a subroutine |
| Glenn Station | M | a Corp card (scored) hosting a facedown card from HQ |
| Worlds Plaza | M | a Corp asset hosting installed, rezzed assets |
| Archives Interface | M | an access replaced: removed from the game instead, once a breach |
| Immolation Script | M | an access of ice in Archives replaced by trashing a rezzed copy |
| Chronos Protocol: Selective Mind-mapping | M | the Corp chooses the grip card the first net damage trashes |
| Titanium Ribs | M | the Runner chooses the grip cards damage trashes |
| Gene Conditioning Shoppe | M | genetics' first-time triggers also met the second time |
| Hivemind | M | its virus counters counted and spent as if on every virus program |
| MemStrips | M | [mu] usable only by virus programs: a restricted memory pool |
| Paparazzi | M | tagged without a tag, wherever being tagged is asked |
| Turntable | M | swapping a stolen agenda with one in the Corp's score area |
| An Offer You Can’t Refuse | M | a Corp operation offering the Runner a run, with no jack-out, on the Corp's turn |

**New mechanics (M):**

One bullet per M mechanic: the cards in F7 that need it, the rules that govern it, and the nearest relative the NSG plan's "Mechanics shared across tranches" table (or the reprint tranche) already built.

- **Currents: a played card that stays in the play area until another current is played or an agenda is scored (Runner's) or stolen (Corp's), then is trashed at the next checkpoint.** Defective Brainchips, Hacktivist Meeting, Housekeeping, Itinerant Protesters, Predictive Algorithm, Traffic Jam. CR 3.5.1b, CR 3.7.1b, CR 8.6.6c. No row in the table. Nearest relatives: Déjà Vu's play-area wait (reprints Stage 11m: a played event parked in the play area, shown in both clients) and the "abilities active outside play" row; the lockdown row (UR 7a: `CorpState::play_area`, active there, trashed as a turn begins) is the closest standing card in the play area, and is Corp-only. What each current *does* is mostly existing words (`StealCost`, `HandSize` per `BadPublicity`, `IncreaseAboutToResolve`, `AdvancementRequirement`); Hacktivist Meeting also needs an additional rez cost imposed by another card, and Traffic Jam a scope over every agenda counted against its own copies in the Corp's score area.

- **Condition counters: an operation that turns itself into a counter with text and installs hosted on a Corp card.** Casting Call (on the agenda it installs faceup), Patch, Sub Boost (on rezzed ice). CR 1.9.5j, CR 1.13.6d, CR 3.5.1a, CR 10.1.4, CR 1.13.14. Near relative: "Hosting in general" (VP 7b, 7c; RWR 3d; TAI 4; PH 7), built for Runner hosts only — `InstalledCard` (the Corp's install) carries no hosted list. Sub Boost's granted subroutine and subtype compose once it is hosted (`ContinuousKind::Subroutines`'s 9.8.3 categories, `GainSubtype` on `Scope::Host`).

- **Corp cards that host cards.** Glenn Station (a scored agenda hosting a facedown card from HQ, CR 1.13.1a allows a score area), Worlds Plaza (an asset hosting up to 3 installed, rezzed assets). CR 1.13.1a, CR 1.13.5, CR 1.13.6b, CR 1.13.7. Near relative: "Hosting in general" (Runner side), and the condition counters above, which need the same Corp-side list.

- **An access replaced ("whenever you would access … you may instead").** Archives Interface (removed from the game instead, once per breach of Archives), Immolation Script (a faceup ice in Archives: trash a rezzed copy instead). CR 7.3.6 (both cards are its examples), CR 7.4.3. Near relatives: Heliamphora's "instead", approximated as an access then a host (Known limits, Approximations); `SetAccessReplacement` replaces a whole breach, not one card's access. Here the approximation would be wrong: the replaced card is never accessed, so its "when accessed" must not resolve.

- **Who chooses the cards damage trashes.** Chronos Protocol (the Corp, for the first net damage each turn, looking at the grip), Titanium Ribs (the Runner, for every damage). CR 10.4.2a, CR 10.4.3 (random by default). No relative: damage's discards are random wherever they are made; the Prevention Rule's parked damage (`WouldHappen`) is where a choice would be asked.

- **"First time" abilities also met the second time.** Gene Conditioning Shoppe, over the genetics in this tranche (Adjusted Chronotype, Enhanced Vision, Symmetrical Visage, Synthetic Blood) and any in the pool. CR 9.3.2b (trigger conditions); no rule names it. Near relative: the turn log's `first_each_turn` verdict (`turn_log::AsOf`, `DeferredTrigger::not_the_first_this_turn`), which would read a count of 2 while the Shoppe is active.

- **Counters on one card counted and spent as if on others.** Hivemind ("considered to be hosted on all other virus programs … and can be spent as if on them"). CR 1.13.10, CR 1.13.11. No relative: `Amount::HostedCounters` and `Cost::RemoveCounters` read the acting card alone.

- **Restricted memory units.** MemStrips (+3[mu] only for virus programs). CR 1.20.4, CR 1.20.4a, CR 1.20.4b. Near relative: Djinn's `HostsMemory` (reprints 11t), which leaves hosted programs out of the limit rather than assigning [mu] to a kind of program; `memory::memory_balance` is a sum where 1.20.4b asks for a mapping.

- **Tagged without a tag.** Paparazzi ("You are tagged"). CR 10.5.2. No relative: being tagged is `RunnerState::is_tagged`, a count of tag counters, asked by the basic actions, `IsTagged` and the evaluator alike.

- **Swapping agendas between score areas.** Turntable (a stolen agenda for one in the Corp's score area). CR 8.8.4c, CR 1.17.3e. Near relatives: "A card added to a score area as an agenda" (VP 7d, RWR 8c, PH 7) and `InstallAgendaFromRunnerScoreArea`; neither moves an agenda out of the Corp's score area, and the points each side holds change with it.

- **A run the Corp's card offers, on the Corp's turn, with no jack-out.** An Offer You Can't Refuse (the Runner may run a central the Corp chose; if they do not, the operation joins the Corp's score area as a 1-point agenda). CR 6.3.1, CR 6.9.4c. Near relatives: "A card added to a score area as an agenda" for the fallback, and Alarm Clock's run made from a trigger; every run in the pool is the Runner's, made in the Runner's turn, and no `Prohibition` forbids a jack-out.

## F8. Lunar, and Honor and Profit — 156 cards (C 82 / V 59 / M 15)

| Card | Class | What it needs |
|---|---|---|
| Cerberus "Cuj.0" H3 | C | Cerberus "Lady" H1's shape: counters placed on install, `Cost::RemoveCounters` break of 2 sentry subroutines |
| Cerberus "Rex" H2 | C | Cerberus "Lady" H1's shape, `restrict_to: CodeGate` |
| D4v1d | C | `BreakSubroutinesUnconditionally` for a hosted power counter, `AmountAtLeast(EncounteredIceStrength, 5)` |
| Refractor | C | stealth-only credits on the boost (VP 6b, Corsair's restriction) |
| Switchblade | C | stealth-only credits (VP 6b), `SubroutineBreakCount::All` |
| Ghost Runner | C | hosted credits, `PaysFor::DuringRuns`, `trash_when_empty` (Penumbral Toolkit) |
| Collective Consciousness | C | `OnRez` with `when: Card(Ice)`, `Hears::Everyone`, `DrawCards` |
| Lamprey | C | `OnSuccessfulRun` on HQ, `LoseCredits`; trashed on `OnVirusCountersPurged` |
| Fester | C | `OnVirusCountersPurged`, `EffectIf(CorpCreditsAtLeast 2)`, `LoseCredits` |
| Incubator | C | `OnTurnStart` `AddCounters`; `[click], [trash]` cost with `last_known` counters moved by `AddCounters` onto a selected virus |
| Leprechaun | C | `MayHost`/`HostsMemory` (Djinn, reprints 11t) |
| Box-E | C | `ContinuousKind::Memory` and `HandSize`, console limit |
| Astrolabe | C | `Trigger::OnServerCreated` (reprints 4), `Memory` |
| Logos | C | `Memory`, `HandSize`, `OnAgendaScored` heard by the Runner, `PromptChooseCards` from the stack |
| Origami | C | `HandSize` per `RunnerInstalls` filtered `AmongCards([origami])` |
| Q-Coherence Chip | C | `OnCardTrashed` `when: Card(Program)` installed, `TrashCard` this |
| Cybsoft MacroDrive | C | `recurring_credits`, `PaysFor::Installing(Program)` |
| Public Terminal | C | `recurring_credits`, `PaysFor::Playing(HasSubtype Run)` |
| Rachel Beckman | C | `AllottedClicks`, `WhileTrue(IsTagged)` trash (reprints 5b) |
| Tri-maf Contact | C | `OncePerTurn` click ability; `OnCardTrashed` subject `This`, meat damage |
| Zona Sul Shipping | C | `OnTurnStart` `AddCounters(Credit)`, `TakeAllCountersAsCredits`, `WhileTrue(IsTagged)` trash |
| Unregistered S&W '35 | C | requirement `AmountAtLeast(TimesThisTurn(OnSuccessfulRun), 1)` on HQ, trash a rezzed card by `HasSubtype` in a remote root |
| The Supplier | C | `HostCardOnThisCard` from the grip, `OnTurnStart` `InstallRunnerCardFromZone(HostedOnSource)` with a 2 discount |
| LLDS Energy Regulator | C | `Prevent(Preventable::Trash)` of hardware, `Cost::AnyOf(Credits 3, TrashSelf)` (Sacrificial Construct) |
| Oracle May | C | `Remember(CardType)` (Gachapon/Engram Flush), `TopOfDeck` reveal, `OncePerTurn` |
| Duggar's | C | a four-click `Paid` ability, `DrawCards(10)` |
| Leela Patel: Trained Pragmatist | C | `OnAgendaScored`/`OnAgendaStolen`, `PromptChooseCards` over the Corp's unrezzed installs to HQ |
| Iain Stirling: Retired Spook | C | `OnTurnStart` with `MoreThan` over agenda points |
| Silhouette: Stealth Operative | C | `OnSuccessfulRun` on HQ, `first_each_turn`, `Expose` (reprints 10a) |
| Calling in Favors | C | `GainCreditsAmount(RunnerInstalls(HasSubtype Connection))` |
| Early Bird | C | `play_requirement: NoActionTakenThisTurn` (Petty Cash), `GainClicks`, `InitiateRun` |
| Express Delivery | C | `LookAtTopOfDeck` 4, add 1 to the grip, shuffle |
| Paper Tripping | C | `NoActionTakenThisTurn`, `RemoveTags` all |
| Three Steps Ahead | C | `LaterThisTurn` at the turn's end, `TimesThisTurn(OnSuccessfulRun)` |
| Mass Install | C | `Repeat` 3 of `InstallRunnerCardFromGrip` over programs, each optional |
| Trade-In | C | `additional_play_cost` `Cost::Trash` hardware, `Amount::Every(2)` of `PaidCardPrintedCost`, search the stack |
| Inject | C | `TopOfDeck` 4 revealed, trash programs, `CardsSelected` credits, rest to the grip |
| Code Siphon | C | `SetAccessReplacement` on R&D, `InstallRunnerCardFromZone` stack with a discount of `IceProtecting(RnD)` times 3, `GiveTags` |
| Feint | C | Inside Job's bypass (reprints 8a) for `EncountersThisRun` at most 2, a replacement of the breach with nothing |
| Cyber Threat | C | Forged Activation Orders' Corp rez choice (reprints 11k), `InitiateRun`, `Prohibit(Rez)` for the run |
| Ashigaru | C | `ContinuousKind::Subroutines` by count of `CardsInHand(Corp)` (TAI 8a, MS 8a) |
| Errand Boy | C | three `PresentChoice` subroutines (gain 1 or draw 1) |
| Information Overload | C | `OnEncounter` `Trace{2}` tag; `Subroutines` by `RunnerTags` |
| IQ | C | `ContinuousKind::RezCost` and `Strength` on `This` per `CardsInHand(Corp)` |
| Komainu | C | `OnEncounter` `GainSubroutine` for the run, counted by the grip (RWR 8b) |
| Mamba | C | `PsiGame`, `AddCounters(Power)`, a `RemoveCounters` paid ability `DuringRun` |
| Markus 1.0 | C | `click_breakable` (the bioroid "Lose [click]" break) |
| Pup | C | `OfferPaidChoice` for the Runner, net damage otherwise |
| Troll | C | `OnEncounter` `Trace{2}`, Runner's `PresentChoice` lose a click or `EndTheRun` |
| Universal Connectivity Fee | C | `EffectIf(IsTagged)`, `LoseCredits`, trash this ice |
| Blue Sun: Powering the Future | C | `OnTurnStart` optional, rezzed card to HQ, `GainCreditsAmount(PrintedCost)` |
| Harmony Medtech: Biomedical Pioneer | C | `ContinuousKind::AgendaPointsToWin` −1 for each `Player` |
| Tennin Institute: The Secrets Within | C | `OnTurnStart`, `Not(AmountAtLeast(TimesLastTurn(OnSuccessfulRun),1))`, `PlaceAdvancementCounters` |
| Chronos Project | C | `OnAgendaScored` subject `This`, `RemoveFromGame` every heap card |
| Domestic Sleepers | C | a scored agenda's click ability (UR 4's window), `AgendaPoints` while a counter is hosted |
| Encrypted Portals | C | `Strength` over `Ice(IceOfType CodeGate)`, credits per rezzed code gate |
| Superior Cyberwalls | C | `Strength` over `Ice(IceOfType Barrier)`, credits per rezzed barrier |
| Hades Fragment | C | `OnTurnStart` optional, `PromptChooseCards` Archives to the bottom of R&D |
| Helium-3 Deposit | C | `PromptChooseCards` over `HostsCounters(Power)`, `AddCounters` up to 2 |
| Philotic Entanglement | C | `DealDamageAmount` of the Runner's scored agendas |
| Utopia Fragment | C | `ContinuousKind::StealCost` per `AccessedCardAdvancementCounters` (Argus Crackdown's price off counters) |
| Cerebral Cast | C | `TimesLastTurn(OnSuccessfulRun)` play requirement, `PsiGame`, the Runner's choice of core damage or a tag |
| Diversified Portfolio | C | `GainCreditsAmount` over remotes with a root card (`ProtectedRemotesWithRootCards`' neighbour, at most a new amount) |
| Fast Track | C | `PromptChooseCards` R&D agenda, reveal, to HQ, shuffle |
| Medical Research Fundraiser | C | `GainCredits` both sides |
| Peak Efficiency | C | `GainCreditsAmount(CorpInstalls(Ice, Rezzed))` |
| Reuse | C | `additional_play_cost` click, `PromptChooseCards` any number from HQ, `CardsSelected` times 2 |
| Shoot the Moon | C | `PromptChooseCards` up to `RunnerTags` ice, `RezInstalled` ignoring costs |
| Snatch and Grab | C | `Trace{3}`, the Runner's `OfferPaidChoice` of a tag, trash a connection |
| Chairman Hiro | C | `HandSize` for `Player(Runner)` (Dr. Vientiane Keeling); `OnTrashedFromAccess` to the Runner's score area worth 2 (PH 7) |
| Industrial Genomics: Growing Solutions | C | `ContinuousKind::TrashCost` per `FacedownCardsInArchives` |
| Mental Health Clinic | C | `OnTurnStart` credit, `HandSize` for `Player(Runner)` |
| Primary Transmission Dish | C | `recurring_credits` 3, `PaysFor::TraceAttempts` |
| Sealed Vault | C | `ChooseNumber` credits onto it, `TakeAllCountersAsCredits`-style take of a chosen number |
| Shell Corporation | C | `OncePerTurn` over both abilities, `AddCounters(Credit)`, `TakeAllCountersAsCredits` |
| Eliza's Toybox | C | three-click `Paid` ability, `RezInstalled` ignoring all costs |
| IT Department | C | `AddCounters`; a hosted-counter ability giving `Lingering::Strength` per `last_known` counters |
| NeoTokyo Grid | C | `OnAdvance` `InRootOfThisServer`, `first_each_turn` (reprints 4's root count) |
| Shattered Remains | C | `AccessInteraction::CorpPaysToApply`, trash hardware per `HostedAdvancementTokens` |
| Tenma Line | C | `SwapInstalledIce` |
| Tori Hanzō | C | a Corp interrupt `Prevent(Damage net)` then `DealDamage` core, 2[c], `OncePerRun` on this server |
| Alias | V | a filter for the encountered ice protecting a remote (`ProtectingRemote` asks the acting card, not the ice) |
| Breach | V | the same remote-protection filter on its interface |
| Passport | V | the same remote-protection filter on its interface |
| Gingerbread | V | `restrict_to` by `CardSubtype` (Tracer), not only `IceType` |
| Grappling Hook | V | `SubroutineBreakCount` "all but 1" |
| Overmind | V | an `Amount` of unused MU (counters placed after its own MU applies) |
| Sage | V | the unused-MU `Amount` as a `Strength` |
| BlacKat | V | a condition "a credit from a stealth card paid this ability" |
| Au Revoir | V | a `Trigger` on jacking out (`GameEvent::RunJackedOut` has no moment) |
| Port Anson Grid | V | an additional cost to jack out on runs on this server (`RunCost`'s twin) |
| Labyrinthine Servers | V | `Preventable::JackOut` and a jack-out `Prohibition` for the run |
| Inazuma | V | `Lingering::BreakLimit` for the next encounter; a jack-out `Prohibition` until the next encounter begins |
| Susanoo-no-Mikoto | V | `MoveRunToOutermost(Archives)` (Proprionegation) plus the jack-out `Prohibition` until an encounter |
| Gemini | V | an `Amount` (or condition) for the trace strength reached |
| Sagittarius | V | the trace-strength word |
| Taurus | V | the trace-strength word |
| Virgo | V | the trace-strength word |
| Self-destruct | V | `Trace` with a base that is an `Amount` (cards trashed this way) |
| Power Tap | V | a `Trigger` on a trace initiated (CR 10.8.6a) |
| Changeling | V | a `ContinuousKind` that removes an ice type, under an odd-advancement condition |
| Lycan | V | the type-loss word and the odd-advancement condition |
| Wendigo | V | the type-loss word; a `Prohibition` on using a chosen program for the run |
| Rainbow | V | ice printed with three types (`CardType::Ice` holds one) |
| Guard | V | a "cannot be bypassed" word asked by `BypassEncounteredIce` |
| Architect | V | a "cannot be trashed" word; its subroutines compose (Accelerated Beta Test, `PromptInstallCorpCard`) |
| Galahad | V | gaining the printed subroutines of revealed HQ cards for the run, in the Corp's order |
| Lancelot | V | the grail word |
| Merlin | V | the grail word |
| Bifrost Array | V | an effect resolving another scored agenda's when-scored trigger (`ResolveSubroutineOfSelectedIce`'s twin) |
| Shiro | V | `Breach` that excludes the root; arrange composes |
| Psychic Field | V | a `Trigger` on this card being exposed |
| Nisei Division: The Next Generation | V | a `Trigger` on bids revealed (CR 10.14.5) |
| The Future Perfect | V | a steal bar for this access only, on access from HQ or R&D |
| Ekomind | V | a memory limit set to an amount (`CardsInHand`) rather than added to |
| Theophilius Bagbiter | V | a hand size set to an amount (`Credits`) |
| Bad Times | V | a `Lingering::Memory` for the turn |
| Medical Breakthrough | V | an `AdvancementRequirement` scope over every copy, active from either score area |
| Docklands Crackdown | V | an `Installing` scope over the other player's installs, `first_each_turn` |
| Donut Taganes | V | a `Playing` scope over both players' plays |
| Midway Station Grid | V | an additional cost to use an icebreaker's break, on runs on this server |
| Eden Fragment | V | "ignore the install cost" as a continuous `Installing` discount, first ice each turn |
| The Root | V | `PaysFor` advancing (and rezzing anywhere) |
| Order of Sol | V | a standing condition "no credits in pool" with `first_each_turn` (`WhileTrue` counted once) |
| Autoscripter | V | an install-from-the-grip `EventFilter` (`InstalledFromHq` is the Corp's) |
| Bug | V | the drawn card on the Corp's draw moment, revealed for 2[c] |
| Ixodidae | V | a `Trigger` on the Corp losing credits |
| Heinlein Grid | V | a `Trigger` on the Runner spending or losing a click during a run on this server |
| Nasir Meidan: Cyber Explorer | V | an `EventFilter` for ice rezzed during the approach before this encounter |
| Will-o'-the-Wisp | V | a `CardFilter` for an icebreaker that broke a subroutine this run |
| The Foundry: Refining the Process | V | a `CardFilter` for another copy of the triggering card |
| Social Engineering | V | a remembered ice and a turn-long trigger on its rez (`LaterThisTurn` with `OnRez`) |
| Window | V | a draw from the bottom of the stack |
| Mushin No Shin | V | a per-card bar on score and rez through the Corp's next turn |
| Mutate | V | install in the trashed ice's position, read off `last_known` (any-position install, reprints 7c) |
| Plan B | V | a `CardFilter` for advancement requirement at most an amount, then `Score` |
| Shi.Kyū | V | `AddToScoreAreaAsAgenda` worth −1 (a negative worth) |
| Planned Assault | V | a Runner event played from the stack (`PlayOperation { from }`'s twin), additional costs ignored |
| Angel Arena | V | an install cost of X written into its counters (X costs, RWR 6e, on an install) |
| Bribery | V | `Lingering::RezCost` for the first unrezzed ice approached only |
| Lag Time | M | currents |
| Paywall Implementation | M | currents |
| Manhunt | M | currents (and its trace composes) |
| Cerebral Static | M | currents (`LosesAbilities` on the identity composes) |
| Enhanced Login Protocol | M | currents, and a first-run-each-turn click `RunCost` |
| Targeted Marketing | M | currents (`ChooseCardName` composes) |
| The News Now Hour | C | `Cannot` over `Playing(HasSubtype Current)`; built with the currents it bars |
| Net Celebrity | M | Runner currents (play area on the Runner's side) |
| Scrubbed | M | Runner currents, and the first ice encountered each turn −2 for the run |
| Unscheduled Maintenance | M | Runner currents, and a one-ice-a-turn install limit |
| Eden Shard | M | Shards: a trigger heard from the grip that installs in place of a breach |
| Hades Shard | M | Shards; its trash breaches Archives without the root |
| Utopia Shard | M | Shards |
| Kitsune | M | a breach of HQ whose first access is the Corp's chosen card |
| Mother Goddess | M | subtypes read off other cards (all of them, not only ice types), with dependency |
| Push Your Luck | M | a secret spend against an even/odd guess |

**New mechanics (M):**

- **Currents**: an event or operation that stays in a play area, active, until another current is played or, for the Corp's, an agenda is stolen or, for the Runner's, an agenda is scored. It is trashed at the next checkpoint. Cards: Lag Time, Paywall Implementation, Manhunt, Cerebral Static, Enhanced Login Protocol, Targeted Marketing (Corp); Net Celebrity, Scrubbed, Unscheduled Maintenance (Runner). The News Now Hour composes, but only matters once currents exist. CR 3.5.1b, CR 3.7.1b, CR 8.6.6c. Near relative: **Lockdown** (UR 7a, `CorpState::play_area`, active there, trashed when the Corp's next turn begins), which is Corp-side only and has no trash on another card's play.
- **Shards: installed from the grip in place of a breach**: a card in the grip hears a successful run on its central and may replace the breach with its own free install. Cards: Eden Shard, Hades Shard, Utopia Shard. Hades Shard's trash also breaches Archives without the root, the same word as Shiro's (V). CR 9.1.8b (abilities active in a zone they can only meet their conditions in). Near relatives: **abilities active outside play** (VP 7f; RWR 7a; TAI 7: Expendable from HQ, triggers from the heap or Archives) and `SetAccessReplacement`. Neither hears a trigger from the grip.
- **Subtypes copied from other cards**: a card gains every subtype of every other rezzed ice, not only the three ice types. Card: Mother Goddess. CR 2.16.5, CR 9.12.1b, CR 9.12.1d (its example is Mother Goddess). Near relative: `ContinuousKind::GainSubtype`/`Lingering::GainSubtype`, which hold one `IceType` (built for Swordsman and Tinkering, reprints 5a and 11i).
- **A secret spend against a guess**: the Runner secretly spends any number of credits and the Corp guesses even or odd. Card: Push Your Luck. CR 10.14.1, CR 10.14.5. Near relative: **Psi game** (RWR 7b; TAI 8b, `Effect::PsiGame`), which is a simultaneous bid of 0 to 2 compared for equality, not a single bid against a guess.
- **A breach whose first access is chosen by the Corp**: the Runner breaches HQ with the root excluded, and the first card accessed must be the one the Corp picked, not a random one. Card: Kitsune. CR 7.3.4a, CR 7.3.4b. Near relative: `Effect::Breach` (Cataloguer) and `Effect::Access` (Pinhole Threading, Deep Dive). The table has no row for it, and the engine has no Dedicated Neural Net.

## F9. Creation and Control — 47 cards (C 28 / V 15 / M 4)

| Card | Class | What it needs |
|---|---|---|
| Bastion | C | a plain barrier, `EndTheRun` (Ice Wall) |
| Datapike | C | `OfferPaidChoice::if_able` to the Runner, declined ends the run (Tollbooth) |
| Heimdall 2.0 | C | `Paid` ability `used_by: Runner`, `Cost::LoseClicks(2)`, break up to 2 (Heimdall 1.0); core damage subroutines |
| Ichi 2.0 | C | Heimdall 1.0's lose-click break; trash a program subroutines (Ichi 1.0); `Trace` with damage and a tag |
| Viktor 2.0 | C | lose-click break; `Trace` placing a power counter; a Corp paid ability costing `Cost::RemoveCounters` (Data Raven) |
| Minelayer | C | `PromptInstallCorpCard` from HQ protecting this server, ignoring the install cost |
| Project Wotan | C | agenda counters on score; a hosted-counter ability giving `GainSubroutine { after, duration: Run }` to the approached rezzed bioroid ice |
| Sentinel Defense Program | C | `OnDamageSuffered` with `when: Damage(Brain)`, then `DealDamage(Net, 1)` |
| Gila Hands Arcology | C | a scored agenda's `[click][click]` paid ability (Private Security Force) |
| Alix T4LB07 | C | `OnCardInstalled` placing a power counter; `[click]`+`TrashSelf` gain per counter read off `last_known` (Fermenter) |
| Director Haas | C | `ContinuousKind::AllottedClicks`; `OnTrashedFromAccess` with `AddToScoreAreaAsAgenda` worth 2 (Nightmare Archive) |
| Thomas Haas | C | `advancement_requirement: 0` to be advanceable (Hadrian's Wall); `TrashSelf` gain 2 per `HostedAdvancementTokens` via `last_known` |
| Levy University | C | `PromptChooseCards` over R&D for ice, revealed, to HQ, shuffle |
| Server Diagnostics | C | `OnTurnStart` gain; `OnCardInstalled` with `when: Card(Ice)` trashing itself |
| Cerebral Imaging: Infinite Frontiers | C | two `HandSize` entries on the identity, `+1 per Credits(Corp)` and `-5` (`Number::per` is signed); re-read if the order of "equal to" against other modifiers matters |
| Borrowed Satellite | C | `ContinuousKind::Link` and `HandSize` (The Toolbox; Hackerspace's hand size) |
| Cloak | C | `recurring_credits: 1`, `PaysFor::Using(Icebreaker)` (Cyberfeeder) |
| Dagger | C | a killer whose pump spends only stealth credits (VP 6b, Corsair) |
| Inti | C | a fracter whose pump lasts the run (`BoostStrength` with a run duration) |
| Ice Analyzer | C | `OnRez` with `when: Card(Ice)` placing a credit; `PaysFor::Installing(Program)` |
| Sahasrara | C | `recurring_credits: 2`, `PaysFor::Installing(Program)` |
| Clone Chip | C | `TrashSelf` then `InstallRunnerCardFromZone` from the heap, paying (Retrieval Run) |
| Feedback Filter | C | two interrupts, `Prevent(Damage)` net for 3[credit] and core for `TrashSelf` (Net Shield, Decoy) |
| Monolith | C | `Memory`; on install `Repeat` of `InstallRunnerCardFromGripWithDiscount`; an interrupt costing `Cost::Trash` from the grip; console limit (CR 3.8.5b) |
| Exploratory Romp | C | `SetAccessReplacement` with `RemoveAdvancementCounters` up to 3 on a card in the attacked server |
| Freelance Coding Contract | C | `PromptChooseCards` up to 5 programs from the grip to the heap, gain 2 per `CardsSelected` |
| Levy AR Lab Access | C | `ShuffleIntoDeck` grip and heap, draw 5, `removed_after_play` (Labor Rights) |
| Scavenge | C | `additional_play_cost` trashing a program; install from grip or heap discounted by `PaidCardPrintedCost` |
| Custom Biotics: Engineered for Success | V | a `DeckRule` word: no cards of one named faction (Jinteki), asked by both validators |
| The Professor: Keeper of Knowledge | V | a `DeckRule` word: the first copy of each program costs no influence (CR 1.4.5) |
| NEXT Design: Guarding the Net | V | a step "before taking your first turn" (CR 1.6.7a) beside `before_starting_hand`: up to 3 ice, one per server, then draw to 5 |
| Exile: Streethawk | V | an `EventFilter` on `OnCardInstalled` for a card installed from the heap (the Runner's half of `InstalledFromHq`) |
| Chakana | V | a `Scope` reaching every agenda, for `AdvancementRequirement` while `ThisCardCountersAtLeast(3)` |
| The Source | V | the same all-agendas `Scope`; `StealCost` with `Stealing(Any)`; trash on `OnAgendaScored`/`OnAgendaStolen` heard by anyone |
| Efficiency Committee | V | a `Prohibition` on advancing cards, made for the turn by `Effect::Prohibit` |
| Haas Arcology AI | V | a card advanceable only while unrezzed (shared with F11's Tyrant, Woodcutter, Salvage, rezzed only); the ability composes |
| Cyber-Cypher | V | a breaker usable only during runs on the server its copy chose on install: a requirement over `Effect::ChooseServer`'s choice (Tsakhia, Security Testing) |
| Zed 1.0 | V | a requirement: the Runner lost [click] to break a subroutine during this run (a run fact beside `SubroutineBrokenThisRun`) |
| Successful Demonstration | V | an unsuccessful run as a turn-log class, read by `TimesLastTurnWhen` (`LastRunUnsuccessful` is about one run) |
| Same Old Thing | V | `PlayOperation { from }` widened to the Runner's events, from the heap (Plutus plays from Archives) |
| Omni-drive | V | a host limit of one program of 1[mu] or less with `HostsMemory` (Djinn), and a `PaysFor` for using the program it hosts |
| Director Haas' Pet Project | V | `PromptInstallCorpCard` from HQ or Archives where every install after the first goes to the server the first created (`OnServerCreated` exists); `deck_limit: 1` |
| Howler | V | an install of ice "directly inward from this ice" (CR 6.2.2c) beside `any_position`; `SetRunEndedEffect` to trash itself and derez the new ice |
| Bioroid Efficiency Research | M | an operation that becomes a hosted condition counter with rules text (CR 1.13.6d); `RezInstalled` ignoring costs composes |
| Awakening Center | M | ice installed onto an upgrade, not protecting anything, rezzed at a discount and encountered off the server's ice, trashed as the run ends |
| Escher | M | rearrange ice across every server, keeping each server's count (CR 8.3.1, CR 8.5.3), as an access replacement |
| Tyr's Hand | M | a subroutine about to be broken as a preventable event, and a rez heard facedown at that moment |

**New mechanics (M):**

- **Condition counters** (an operation or event that converts itself into a counter with rules text, hosted on a card, its abilities active while hosted): Bioroid Efficiency Research. CR 1.9.5j, CR 1.13.6d, CR 3.5.1a. Near relative: hosting in general (VP 7b/7c, PH 7 turned facedown) and lockdowns in `CorpState::play_area` (UR 7a), which keep an operation in play; neither hosts a counter. F11's Oversight AI needs the same.
- **Ice hosted on a non-ice card and encountered from there**: Awakening Center. CR 1.13.6, CR 6.1.3, CR 6.2.7a. Near relative: forced encounter away from the Runner's position (UR 7b, `RunState::suspended`) and `MayHost` with `InstallingOntoThis`; neither installs Corp ice onto a card.
- **Rearranging installed ice** across servers: Escher. CR 8.3.1, CR 8.5.3, CR 6.2.2f. Near relative: `SwapInstalledIce` (two ice) and Arrange of R&D (VP 6a); a rearrangement might compose as repeated swaps, which the stage should test first. F11's Sunset needs it within one server.
- **Preventing a subroutine from being broken**: Tyr's Hand. CR 9.8.5, CR 9.9.1b, CR 9.3.6d. Near relative: generic prevention (`Preventable`, Rules Audit item 4) and a facedown card hearing the moment that lets it rez (`TriggeredEffect::while_unrezzed`, reprints 10b); break restrictions (PH 8) forbid a break but never interrupt one.

## F10. Spin — 97 cards (C 70 / V 20 / M 7)

| Card | Class | What it needs |
|---|---|---|
| Quandary | C | a plain code gate, `EndTheRun` |
| Paper Wall | C | `OnIceFullyBroken` about this ice, `TrashCard(ThisCard)` |
| Tsurugi | C | `OfferPaidChoice` to the Corp (1[credit] or end the run) and net damage subroutines |
| Wotan | C | four `OfferPaidChoice` subroutines to the Runner: `Cost::Clicks(2)`, `Credits(3)`, `Cost::Trash` of a program, `SufferDamage(Brain, 1)` |
| RSVP | C | `Effect::Prohibit { what: SpendCredits, until: Run }` (Prohibition::SpendCredits, Attini's word) |
| Marker | C | `LaterThisTurn { when: OnEncounter, this_run }` giving the encountered ice `GainSubroutine { after, duration: Run }` (Chum's shape) |
| Swarm | C | `advancement_requirement: 0`; `ContinuousKind::Subroutines` counted per `HostedAdvancementTokens` (Echo); `OnRez` bad publicity |
| Fenris | C | `OnRez` about this ice, `GiveBadPublicity(1)`; core damage and `EndTheRun` |
| Grim | C | `OnRez` bad publicity; trash a program subroutine |
| Muckraker | C | `OnRez` bad publicity; three `Trace` subroutines; `EffectIf(IsTagged, EndTheRun)` |
| Shinobi | C | `OnRez` bad publicity; `Trace` subroutines with net damage |
| Ireress | C | `ContinuousKind::Subroutines` counted per `Amount::BadPublicity` (Echo) |
| Geothermal Fracking | C | agenda counters on score; `[click]` + hosted counter: gain 7, bad publicity |
| Profiteering | C | `ChooseNumber` 0–3, `GiveBadPublicity(ChosenNumber)`, gain 5 per (Bigger Picture's shape) |
| Clone Retirement | C | `OnAgendaScored` may `RemoveBadPublicity(1)`; `OnAgendaStolen` bad publicity |
| Vulcan Coverup | C | `OnAgendaScored` meat damage; `OnAgendaStolen` bad publicity |
| GRNDL: Power Unleashed | C | `before_starting_hand` gaining 5[credit] and 1 bad publicity (CR 1.6.1a); no card is active yet to hear the bad publicity |
| Veterans Program | C | `OnAgendaScored`, `RemoveBadPublicity(2)` (the first card to use it) |
| Witness Tampering | C | double operation (`additional_play_cost`), `RemoveBadPublicity(2)` |
| Restoring Face | C | `PromptChooseCards` over own installs with `AnyOf` sysop, executive, clone; reveal and trash; `RemoveBadPublicity(2)` |
| Rex Campaign | C | power counters on rez; `OnTurnStart` removes one; empty: trash and `PresentChoice` (Adonis Campaign) |
| Restructure | C | `GainCredits(Corp, 15)` |
| Cyberdex Trial | C | `PurgeVirusCounters` |
| Corporate Shuffle | C | double; `ShuffleIntoDeck` of HQ, draw 5 |
| Blue Level Clearance | C | double; gain 5, draw 2 |
| Shipment from SanSan | C | double; `PlaceAdvancementCounters` up to 2 on an `Advanceable` install |
| Interns | C | double; `PromptInstallCorpCard` from HQ or Archives, non-operation, ignoring the install cost |
| Character Assassination | C | `OnAgendaScored`, `Unpreventable(TrashCard)` of a resource (Stimhack's word) |
| City Surveillance | C | `OnTurnStart` heard on the Runner's turn, `OfferPaidChoice` 1[credit] or a tag |
| Isabel McGuire | C | `[click]`: `AddToHand` of one of the Corp's installs |
| Jackson Howard | C | `[click]`: draw 2; `Cost::RemoveSelfFromGame`: `ShuffleIntoDeck` up to 3 from Archives |
| Market Research | C | `OnAgendaScored` with `EffectIf(IsTagged)` placing a counter; `AgendaPoints` while `ThisCardCountersAtLeast(1)` |
| NAPD Contract | C | `AdvancementRequirement` per `BadPublicity` (Freedom of Information); `StealCost` on `This` |
| Panic Button | C | `install_only_in` HQ; `1[credit]`: draw, `requirement: DuringRunOn(HQ)` |
| Power Shutdown | C | `ChooseNumber` and `Mill`; the Runner trashes a program or hardware with `PrintedCostAtMost(ChosenNumber)`; `TimesLastTurn(OnRunStart)` |
| Reclamation Order | C | double; `ChooseCardName`, then `ChosenName` copies from Archives to HQ |
| Shock! | C | `RevealedWhileAccessed`; `OnAccessed` net damage (Snare!'s shape) |
| Caprice Nisei | C | `OnApproachServer` about this server (the step after passing all its ice, CR 6.9.4g), `PsiGame` ending the run on differing bids (See How They Run) |
| Snoop | C | `OnEncounter` `RevealHand(Runner)`; a hosted-power-counter ability that reveals and trashes 1; `Trace` placing a counter |
| Garrote | C | a killer, 1[credit] break and pump |
| Leviathan | C | a decoder breaking up to 3 for 3[credit] |
| Torch | C | a decoder, 1[credit] break and pump |
| Sharpshooter | C | `TrashSelf` break any number of destroyer subroutines (a `BreakSubroutines` filtered by subtype) |
| Dyson Fractal Generator | C | `recurring_credits: 1`, `PaysFor::Using(HasSubtype(Fracter))` |
| Lockpick | C | `recurring_credits: 1`, `PaysFor::Using(HasSubtype(Decoder))` |
| Silencer | C | `recurring_credits: 1`, `PaysFor::Using(HasSubtype(Killer))` |
| CyberSolutions Mem Chip | C | `ContinuousKind::Memory` +2 |
| LLDS Processor | C | `OnCardInstalled` with `when: Card(Icebreaker)`, `acts_on_subject`, a turn-long strength boost |
| Scheherazade | C | `MayHost` programs; `InstallProgram` with a host (Djinn); `OnCardInstalled` with `when: Host` gaining 1 |
| Savoir-faire | C | `OncePerTurn` on a `2[credit]` install-a-program ability |
| Activist Support | C | `OnTurnStart` on each side's turn with `EffectIf` on `RunnerTags` and `BadPublicity` at 0 |
| Grifter | C | `OnDiscardPhaseEnd`, `EffectIf(TimesThisTurn(OnSuccessfulRun))` with an `otherwise` trashing itself |
| Hard at Work | C | `OnTurnStart` gain 2 and `LoseClicks(1)` |
| Motivation | C | `OnTurnStart` `LookAtTopOfDeck` of the stack, optional |
| Starlight Crusade Funding | C | `OnTurnStart` lose a click; `ContinuousKind::PlayClicks` -1 over `Scope::Playing(HasSubtype(Double))` |
| Tallie Perrault | C | `OnCardPlayed` by the Corp with `when: Card(AnyOf gray ops, black ops)`; `TrashSelf` draw per `BadPublicity` |
| Woman in the Red Dress | C | `OnTurnStart` reveal the top of R&D (`TopOfDeck`), `PresentChoice { chooser: Corp }` to draw it |
| False Echo | C | `OnIcePassed` about unrezzed ice, `TrashSelf`, the Corp rezzes it or it goes to HQ (Forged Activation Orders' shape) |
| Record Reconstructor | C | an Archives access replacement, may, adding a faceup card to the top of R&D (`AddToDeck(Top)`) |
| Blackmail | C | `play_requirement` on `BadPublicity`; `InitiateRun` with `Prohibit { what: Rez, until: Run }` |
| Eureka! | C | double; `TopOfDeck` revealed, install with a 10[credit] `Discount`, `otherwise` trash |
| Executive Wiretaps | C | double; `RevealHand(Corp)` |
| Expert Schedule Analyzer | C | `[click]` run HQ with an access replacement of `RevealHand(Corp)` |
| Frame Job | C | double; the Runner forfeits an agenda (Data Dealer), `GiveBadPublicity(1)` |
| Hostage | C | double; `PromptChooseCards` a connection from the stack, revealed, may install it, shuffle |
| Keyhole | C | `[click]` run R&D with an access replacement: look at the top 3, trash 1, the Corp shuffles R&D |
| Lawyer Up | C | double; `RemoveTags` up to 2, draw 3 |
| Power Nap | C | double; gain 2 plus `InHeapWithSubtype(Double)` |
| Quest Completed | C | `play_requirement` of successful runs on HQ, R&D and Archives this turn (`TimesThisTurnWhen`); `Access` of one installed non-ice card |
| Recon | C | a run with `LaterThisTurn { when: OnEncounter, this_run }` offering `Cost::JackOut` (Flip Switch's jack-out as a cost) |
| Hellion Alpha Test | V | an "if unsuccessful" branch on `Effect::Trace` (CR 10.8.5); the resource top-decked with `AddToDeck(Top)` |
| Invasion of Privacy | V | the trace's unsuccessful branch and an `Amount` for the margin by which the trace beat the link (CR 10.8.4) |
| Broadcast Square | V | a `Preventable` for bad publicity (a `WouldHappen` arm) whose interrupt is a `Trace` |
| Alpha | V | an interface requirement: the encountered ice is the outermost protecting its server |
| Omega | V | the same requirement for the innermost (CR 4.6.9b) |
| Curtain Wall | V | a `while` on this ice being the outermost protecting its server, for `Strength` |
| Off the Grid | V | a `Cannot` about runs initiated on this server (a `Prohibition` asked of `RunsOnThisServer`); `install_only_in` remote; trash on a successful HQ run |
| Gyri Labyrinth | V | a `Lingering` hand size for the Runner until the Corp's next turn begins |
| Running Interference | V | a rez surcharge for the run equal to each ice's own rez cost (`Lingering::RezCost` is one flat number) |
| Unorthodox Predictions | V | a break prohibition over ice of a chosen type until the Corp's next turn (PH 8's `BreakLimit` and a remembered ice type) |
| Accelerated Diagnostics | V | `PlayOperation` from among the top 3 of R&D looked at, ignoring additional costs |
| Sundew | V | a `Trigger` for the Runner spending clicks, and a delayed ability for a run on this server begun during that action |
| Blackguard | V | a moment for an expose (after it, beside `OnExposeAboutToResolve`); the forced rez if able composes (Forged Activation Orders) |
| Capstone | V | a `CardFilter` for a card with another copy installed, counted over the cards trashed |
| Gorman Drip v1 | V | an `EventFilter` or trigger for the Corp's basic gain-a-credit action beside `OnBasicDrawAction` |
| John Masanori | V | an unsuccessful run as a turn-log class, so `first_each_turn` can count it (successful composes) |
| Leverage | V | a lingering prevention of all damage until the Runner's next turn (`Lingering::PreventRunEnding`'s shape); the Corp's choice composes |
| Paintbrush | V | `GainIceSubtype` lasting until the end of the next run this turn (it has run and turn durations) |
| Queen's Gambit | V | `Prohibition::Access` bound to the selected card for the turn, not the acting install |
| Raymond Flint | V | `Breach(HQ)` that skips the root of HQ; `OnBadPublicityTaken` and the expose cost compose |
| Bishop | M | a Trojan that re-hosts itself onto other ice by its own `[click]` ability, central to remote and back |
| Knight | M | Caïssa re-hosting, not onto adjacent ice; its break is host-ice only (Botulus composes) |
| Rook | M | Caïssa re-hosting to the same server or the same position (CR 6.2.3); the rez tax composes |
| Pawn | M | Caïssa moving inward on each successful run, then trashing itself and installing another Caïssa |
| Deep Red | M | memory usable only by Caïssa programs (CR 1.20.4) and a free use of an installed Caïssa's `[click]` ability |
| Copycat | M | the Runner's position moved to a copy of the passed ice on another server, the run continuing there |
| Toshiyuki Sakai | M | an accessed install swapped with an agenda or asset from HQ, keeping its counters, and the access retargeted or declined |

**New mechanics (M):**

- **Caïssa: a hosted program that re-hosts itself on other ice** (with restrictions by server kind, adjacency, same position) and memory restricted to Caïssa programs: Bishop, Knight, Rook, Pawn, Deep Red. CR 1.13.6c, CR 6.2.3, CR 1.20.4, CR 1.20.4b. Near relative: Trojans (`installs_on_ice`, `hosted_on_ice`) and Magnet's re-hosting (reprints 9h); `HostsMemory` (reprints 11t) is memory outside the limit, not memory restricted to a kind.
- **The run moves to another server mid-run**: Copycat. CR 6.2.7d, CR 6.1.3. Near relative: `MoveThisIceToOutermost` (Tributary, moves ice within the attacked server), `MoveRunToOutermost` (Cell Portal) and redirects (`RedirectRunOnApproach`, Sneakdoor Beta's on success); none changes the attacked server from a pass. F11's Bullfrog needs the same.
- **Swapping an accessed install with a card from HQ**: Toshiyuki Sakai. CR 7.1.7 (a card that moves while accessed ends that access), CR 7.2.1. Near relative: `SwapApproachedIceWithCard` (Tatu-Bola), which swaps ice from HQ during a run but not an accessed card.

## F11. Genesis — 76 cards (C 52 / V 21 / M 3)

| Card | Class | What it needs |
|---|---|---|
| Burke Bugs | C | `Trace` with base 0, the Runner trashes a program |
| Hourglass | C | three `LoseClicks` subroutines on the Runner, if able |
| Janus 1.0 | C | the lose-click bioroid break (Heimdall 1.0), four core damage subroutines |
| Sherlock 1.0 | C | lose-click break; `Trace` adding a program to the top of the stack (`AddToDeck(Top)`) |
| Snowflake | C | `PsiGame` ending the run on differing bids |
| Uroboros | C | `Trace` with `Prohibit { what: Run, until: Turn }` (Excalibur); `Trace` ending the run |
| Viper | C | `Trace` making the Runner lose a click; `Trace` ending the run |
| Dracō | C | on rez, `ChooseNumber` of credits paid and placed as power counters (Lobisomem's X); `Strength` per `HostedCounters` |
| Midori | C | `OnIceApproached` in this server, `SwapApproachedIceWithCard` from HQ, `OncePerRun`; the Runner may jack out |
| Amazon Industrial Zone | C | `OnCardInstalled` ice in this server, may `RezInstalled` 3[credit] less; region limit (CR 3.6.5, `install_trash`) |
| Big Brother | C | `play_requirement: IsTagged`, `GiveTags(2)` |
| Commercialization | C | choose a piece of ice, gain per `HostedAdvancementTokens` of it |
| Corporate War | C | `EffectIf(CorpCreditsAtLeast(7))` with an `otherwise` losing all credits |
| Edge of World | C | `InteractiveOnAccess::CorpPaysToApply` 3[credit], core damage per `IceProtectingThisServer` |
| Eve Campaign | C | 16 credits loaded on rez, `OnTurnStart` takes 2, empty: trash (Adonis Campaign) |
| Executive Retreat | C | on score an agenda counter and `ShuffleIntoDeck` of HQ; `[click]` + counter: draw 5 |
| Fetal AI | C | `RevealedWhileAccessed`; `OnAccessed` outside Archives, net damage; `StealCost` on `This` |
| Foxfire | C | `Trace[7]`, trash a resource with `AnyOf` virtual or link subtype |
| Freelancer | C | `play_requirement: IsTagged`, trash up to 2 resources |
| Government Contracts | C | a scored agenda's `[click][click]` paid ability (Private Security Force) |
| Jinteki: Replicating Perfection | C | `Cannot(RunOnRemote)` with a `while` that no run on a central was made this turn (`TimesThisTurnWhen`) |
| Mandatory Upgrades | C | `ContinuousKind::AllottedClicks` from the score area (`Scope::ScoreArea(Corp)`) |
| Marked Accounts | C | `[click]` places 3 credits on it; `OnTurnStart` takes 1 if able |
| NBN: The World is Yours* | C | `HandSize` +1 on the identity |
| Private Contracts | C | 14 credits loaded on rez; `[click]` take 2; empty: trash |
| Restructured Datapool | C | a scored agenda's `[click]` `Trace` giving a tag |
| Rework | C | `PromptChooseCards` 1 from HQ, `ShuffleIntoDeck` R&D |
| Ruhr Valley | C | `ContinuousKind::RunCost(Clicks(1))` over `RunsOnThisServer` (Earth Station's shape); region limit |
| Crescentus | C | `TrashSelf` requiring `IceFullyBrokenThatEncounter`, `DerezCard(EncounteredIce)` |
| Deep Thought | C | `OnSuccessfulRun` on R&D placing a virus counter; an `OnTurnStart` look at R&D whose requirement is `ThisCardCountersAtLeast(3)` |
| Deus X | C | `TrashSelf` break any number of AP subroutines; `TrashSelf` interrupt preventing any net damage (Net Shield's word) |
| Disrupter | C | `TrashSelf` interrupt, `Prevent(TraceBaseStrength)` (Flip Switch) |
| Nerve Agent | C | `OnSuccessfulRun` on HQ counter; `OnBreach` HQ, `ChooseNumber` below the counters, `AddAdditionalAccessAmount` |
| Snitch | C | `OnIceApproached` about unrezzed ice, `OncePerRun`, `Expose`, then may jack out (`Cost::JackOut`) |
| Snowball | C | `OnSubroutineBroken` with `when: ByThis`, a run-long strength boost to itself |
| Muresh Bodysuit | C | an interrupt on `OnDamageAboutToResolve` with `when: Damage(Meat)` and `first_each_turn` (Net Shield) |
| Plascrete Carapace | C | power counters on install, a hosted-counter interrupt preventing meat damage, empty: trash |
| R&D Interface | C | `OnBreach` R&D, `AddAdditionalAccess` 1 (Rotary's shape) |
| Whizzard: Master Gamer | C | `recurring_credits: 3`, `PaysFor::TrashCosts(Any)` (Paricia's word) |
| Compromised Employee | C | `recurring_credits: 1`, `PaysFor::TraceAttempts`; `OnRez` ice gain 1 |
| Cortez Chip | C | `TrashSelf`: a `Lingering::RezCost` +2 on the chosen ice (`lingering::On::Install`) for the turn |
| Data Leak Reversal | C | `install_requirement` of a successful central run this turn; an ability requiring `IsTagged` that `Mill`s R&D by 1 |
| Inside Man | C | `recurring_credits: 2`, `PaysFor::Installing(CardType(Hardware))` |
| Joshua B. | C | `OnTurnStart` may gain a click, then `LaterThisTurn { when: OnDiscardPhaseEnd }` a tag |
| Kati Jones | C | two `[click]` abilities sharing one `OncePerTurn` key; place 3 credits, take all (`TakeAllCountersAsCredits`) |
| New Angeles City Hall | C | a `2[credit]` interrupt `Prevent(Tags(1))`; trash on `OnAgendaStolen` |
| Public Sympathy | C | `HandSize` +2 |
| The Helpful AI | C | `Link` +1; `TrashSelf` a turn-long +2 strength on a chosen icebreaker |
| Kraken | C | `play_requirement` an agenda stolen this turn; choose a server, the Corp trashes 1 of its ice |
| Quality Time | C | `DrawCards(Runner, 5)` |
| Satellite Uplink | C | `Expose` as the `then` of a selection of up to 2 unrezzed installs (Infiltration) |
| Vamp | C | run HQ with an access replacement: `ChooseNumber`, pay it, the Corp loses it, a tag if more than 0 |
| TMI | V | an "if unsuccessful" branch on `Effect::Trace` (CR 10.8.5), derezzing itself; an `OnRez` trace |
| Data Hound | V | an `Amount` for the margin by which the trace beat the link (CR 10.8.4); look, trash 1 and arrange the rest of the stack (Arrange, VP 6a) |
| Midseason Replacements | V | the trace-margin `Amount`, as a number of tags; the stolen-last-turn requirement composes |
| Power Grid Overload | V | the trace-margin `Amount`, as `PrintedCostAtMost` for the hardware trashed |
| ChiLo City Grid | V | a `Trigger` for a successful trace, with a `when` about a run on this server |
| Net Police | V | `recurring_credits` as an `Amount` (the Runner's link), with `PaysFor::TraceAttempts` |
| Tyrant | V | ice advanceable only while rezzed; `Subroutines` counted per `HostedAdvancementTokens` composes (Echo) |
| Woodcutter | V | advanceable only while rezzed; net damage `Subroutines` per counter |
| Salvage | V | advanceable only while rezzed; tracer `Subroutines` per counter |
| Weyland Consortium: Because We Built It | V | a `PaysFor` word for advancing ice (the basic advance action as a `Purpose`) |
| Simone Diego | V | the same advancing `PaysFor`, about cards in the root of or protecting this server |
| Chimera | V | `GainIceSubtype` lasting until the ice is derezzed (it has run and turn durations); derez on `OnDiscardPhaseEnd` of either turn composes |
| Dedicated Server | V | a `PaysFor` word for rezzing ice anywhere (`RezzingInThisServer` is one server's) |
| Encryption Protocol | V | a `Scope` reaching every installed Corp card for `TrashCost` (`Accessing` reaches a card in HQ or R&D too) |
| Sensei | V | a run-long grant to every other ice encountered (`LaterThisTurn` fires once; `GainSubroutine` is per ice) |
| Andromeda: Dispossessed Ristie | V | an identity's starting hand size (CR 1.6.6), honoured by the mulligan's redraw |
| Creeper | V | a `ContinuousKind` for a program's own memory cost, with a `while` on `Amount::Link`, read from the grip |
| ZU.13 Key Master | V | the same memory-cost word as Creeper |
| Replicator | V | a `CardFilter` for another copy of the triggering card, on `OnCardInstalled` hardware including itself |
| Surge | V | a requirement and a target for the program a virus counter was placed on this turn |
| Personal Workshop | V | counters on cards hosted uninstalled (Hackerspace's hosting), and an install of a hosted card when its counters run out |
| Oversight AI | M | an operation that becomes a hosted condition counter on ice (CR 1.13.6d), trashing its host when the ice is fully broken |
| Sunset | M | arrange the ice protecting one server in any order (CR 8.3.1) |
| Bullfrog | M | the encountered ice moved to the outermost position of another server and the run continuing there (CR 6.2.7d) |

**New mechanics (M):**

- **Condition counters** (an operation that converts itself into a counter with rules text and hosts itself on a card): Oversight AI. CR 1.9.5j, CR 1.13.6d, CR 3.5.1a. Near relative: lockdowns in `CorpState::play_area` (UR 7a) and hosting in general (VP 7b/7c); neither is a hosted counter. F9's Bioroid Efficiency Research needs the same.
- **Rearranging installed ice**: Sunset (one server). CR 8.3.1, CR 8.5.3, CR 6.2.2f. Near relative: `SwapInstalledIce` and Arrange (VP 6a, R&D only); a rearrangement may compose as repeated swaps of two ice of the chosen server. F9's Escher needs it across servers.
- **The run moves to another server mid-run**: Bullfrog. CR 6.2.7d (whose example is Bullfrog), CR 6.2.2a. Near relative: `MoveThisIceToOutermost` (Tributary), which moves ice to the attacked server's outermost position, and `MoveRunToOutermost` (Cell Portal). F10's Copycat needs the same.

## F12. The rest (NAPD Multiplayer, Magnum Opus, System Core 2019) — 3 cards (C 1 / V 0 / M 2)

| Card | Class | What it needs |
|---|---|---|
| Hired Help | C | `ContinuousKind::RunCost` on this server (Earth Station: SEA Headquarters), paid by `Cost::Trash` from `OwnScoreArea`; `while` is not `TimesThisTurnWhen` a successful HQ run; `deck_limit` 1 |
| Watch the World Burn | M | A game-long ability created on resolution, keyed to a remembered card name, removing accessed copies from the game; terminal and remote run compose |
| Cyber Bureau: Keeping the Peace | M | Setup before the first turn: a 10-card starting hand, 5 free installs, a pooled rez discount of 20, then `FlipIdentity`; the back's text composes |

**New mechanics (M):**

- **An ability a resolved card creates that lasts until the game ends**: "until the game ends, whenever you access a copy of that card, remove it from the game", keyed to a name chosen during the run. The first non-agenda card accessed is removed from the game as it is accessed. Card: Watch the World Burn. CR 9.10.1, CR 9.10.2, CR 9.10.3, CR 4.9.2, CR 5.4.3. Near relatives: "A triggered ability created by a card that resolved" (RWR 5d in part; DF's cards not yet), "A choice remembered for a duration" (UR 6, `Effect::Remember`; `ChooseCardName`), "Runner cards removed from the game" (RWR 2b), and "Terminal" (RWR 6a).
- **An identity's setup before the first turn, with a changed starting hand and a pooled discount**: a 10-card starting hand, up to 5 installs ignoring install costs, any number of rezzes sharing one discount of 20, then a flip. Card: Cyber Bureau: Keeping the Peace. CR 1.6.6, CR 1.6.7a. Near relatives: none in the table. The closest are Ayla "Bios" Rahim's setup before the starting hand (Tranche 8 Stage 11w) and `FlipIdentity` (both sides, one bit).
