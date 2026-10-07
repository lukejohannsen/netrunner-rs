use serde::{Deserialize, Serialize};

/// When (or how) an ability fires. Automatic variants name a rules-flow
/// moment; `Paid` marks an ability that never fires on its own and must be
/// explicitly activated by a player paying its `AbilityDef::cost`.
///
/// **Who hears a trigger is not a property of the variant.** Every active
/// card that declares it is asked, and the card says which occurrences it
/// means (`Subject`, `hears`) — see `rules::listeners`. Where a variant's
/// note below names an audience ("the Corp identity", "the Runner's rig"),
/// it records the card that first needed the trigger, from when each
/// audience was written by hand in `rules::dispatcher`; it is not a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    OnPlay,
    OnRunStart,
    /// Renamed from `OnIceEncountered` — folded into a single "OnX"
    /// vocabulary with the other automatic variants. On a piece of ice,
    /// "when the Runner encounters this ice"; on a Runner card or
    /// identity, "whenever you encounter a piece of ice" (Fransofia Ward)
    /// — the dispatcher fires the Runner's side first, as the active
    /// player's, and skips the ice's own reactions when the Runner bypassed
    /// it.
    OnEncounter,
    /// Renamed from `StartOfTurn`, to disambiguate from the unrelated
    /// `rules::state::GamePhase::StartOfTurn(Side)` variant, which names a
    /// turn sub-phase, not a card-ability trigger condition.
    OnTurnStart,
    /// Fires once, the moment this card is presented as the Runner's
    /// current access choice (when `GameEvent::CardAccessed` fires for it)
    /// — Ambush-style "when accessed" abilities (Snare!). Always fires
    /// unconditionally and cannot itself be paid off. A card that needs a
    /// payable "avoid this" reaction (Fetal AI) models that separately via
    /// `dsl::ability::InteractiveOnAccess`/`CardDefinition::interactive_on_access`,
    /// resolved *before* `OnAccessed` via `rules::run::state::AccessPhase::
    /// PendingInteractiveTrigger`.
    OnAccessed,
    /// Fires when this card is trashed via `PlayerAction::
    /// TrashAccessedCard` specifically — not other trash paths (a
    /// subroutine's `Effect::TrashCard`, a normal Corp trash action, etc.).
    /// Shock!-style "when trashed by the Runner accessing it" abilities.
    OnTrashedFromAccess,
    /// Fires when a run completes successfully — distinct from
    /// `OnRunStart`, which fires at initiation, not resolution. *Which*
    /// server is the card's to say, in `TriggeredEffect::when`: Gabriel
    /// Santiago's "on HQ", Conduit's "on R&D", Leech's "on a central
    /// server". Each of those was a variant of its own here
    /// (`OnSuccessfulRunOnHq`, `…OnRnD`, `…OnCentralServer`) while a trigger
    /// matched by equality alone, so one `RunSucceeded` was an occurrence of
    /// up to three triggers and a fourth server condition would have been a
    /// fourth variant.
    OnSuccessfulRun,
    /// Not a moment in time — marks an ability that only resolves when a
    /// player explicitly activates it and pays `AbilityDef::cost`. Should
    /// only ever appear as an `AbilityDef::trigger`, never inside a
    /// `TriggeredEffect` (which has no `Cost` field to pay).
    Paid,
    /// Two distinct dispatch sources, per install kind:
    /// - `PlayerAction::InstallCard` (Corp only): fires against the
    ///   *installing side's identity card* only, not the installed card
    ///   itself — e.g. Haas-Bioroid: Engineering the Future's "first
    ///   install each turn" bonus, which is `first_each_turn` on the
    ///   identity's entry.
    /// - `PlayerAction::InstallResource` (Runner only): fires against the
    ///   just-installed Resource card itself *and* the Runner's identity —
    ///   e.g. Red Team/Telework Contract's own "when you install this
    ///   resource, load N credits onto it."
    ///
    /// `ProgramInstalled`/`HardwareInstalled` reach the just-installed card
    /// the same way (Botulus's counter; GAMEDRAGON™ Pro's "when you install
    /// this hardware ... you may host it"). Hardware was the last type
    /// widened — no System Gateway hardware reacted to its own install.
    OnInstall,
    /// Fires when the Corp scores an agenda (`PlayerAction::ScoreAgenda`) —
    /// dispatched twice: once against the scored agenda's own `CardId` (its
    /// own "on score" text, e.g. Hostile Takeover), then against the Corp's
    /// identity if one is set (a reactive identity ability, e.g. Jinteki:
    /// Personal Evolution).
    OnAgendaScored,
    /// Fires against the Corp's identity card when the Runner steals an
    /// agenda (`run::resolve_steal`) — e.g. Jinteki: Personal Evolution
    /// reacting to either scoring or a steal.
    OnAgendaStolen,
    /// "When you would suffer damage": fires the instant damage is parked
    /// for prevention (`GameEvent::AboutToResolve` about
    /// `WouldHappen::Damage`), before the players are asked — for a card's
    /// own triggered reaction. An interrupt a player *uses* is a `Paid`
    /// ability instead, activated while they are asked
    /// (`rules::prevention`). The one "would" with a trigger: a tag or a
    /// trash about to happen gets one the day a card listens for it
    /// (`OnTrashAboutToResolve` was here for years with no card).
    OnDamageAboutToResolve,
    /// Fires against the card itself the instant it's rezzed
    /// (`GameEvent::IceRezzed` — despite the name, already fired for any
    /// Corp rez, not just ICE, per `engine::rez_ice`'s own doc comment) —
    /// e.g. Ping's "when you rez this ice during a run against this
    /// server, give the Runner 1 tag." Combine with
    /// `EffectRequirement::RezzedDuringRunAgainstThisServer` to scope it to
    /// a rez that happens mid-run against the card's own server. On a
    /// Runner card or identity, "whenever the Corp rezzes a card" — Barry
    /// "Baz" Wong narrows it to ice with `when: Card(CardType(Ice))`.
    OnRez,
    /// Fires against every rezzed Corp Root-slot install in a server the
    /// Runner has just approached (`GameEvent::ServerApproached`) — the
    /// approach-server step, before the run is successful, so an ability
    /// here that ends the run (Anoetic Void, Manegarm Skunkworks) denies
    /// every "when your run is successful" trigger. Used to share
    /// `RunSucceeded`, which fired at the same moment and made every run
    /// successful before these could stop it (ROADMAP Rules Audit T9).
    OnApproachServer,
    /// Fires against the Runner's identity and every Runner rig card when
    /// a run concludes — `GameEvent::RunCompleted`'s "normal" ending only
    /// (see `dispatcher::dispatch_event`'s doc comment on this arm for why
    /// jack-out/effect-ended runs are out of scope for now) — e.g. Mayfly's
    /// "when this run ends, trash this program," Zahya Sadeghi's "when a
    /// run on HQ or R&D ends, you may gain 1 credit for each card
    /// accessed."
    OnRunEnded,
    /// Fires against the Runner's rig/resources when the Runner takes the
    /// *basic* click-to-draw action specifically (`GameEvent::
    /// BasicDrawActionTaken`) — not `Effect::DrawCards`, which never
    /// dispatches this. e.g. Verbal Plasticity's "the first time each turn
    /// you take the basic action to draw 1 card, instead draw 2 cards."
    OnBasicDrawAction,
    /// Fires against the Corp's identity whenever the Runner gains a tag
    /// (`GameEvent::TagsGiven { side: Side::Runner, .. }`) — e.g. NBN:
    /// Reality Plus's "the first time each turn the Runner takes a tag,
    /// gain 2 credits or draw 2 cards."
    OnTagsGiven,
    /// Fires against the Corp's identity whenever any installed card is
    /// advanced (`GameEvent::CardAdvanced`) — e.g. Weyland Consortium:
    /// Built to Last's "whenever you advance a card, gain 2 credits if it
    /// had no advancement counters" (combine with `EffectRequirement::
    /// WasFirstAdvancementThisCard` for the "had no counters" half).
    /// Advancing is `engine::advance`, the one place a counter is placed
    /// *as an advancement* (CR 1.18.1) — the basic action's and any card
    /// ability's, should one ever print it — and never a placement
    /// (CR 1.18.2, `Effect::PlaceAdvancementCounters`), which Built to Last
    /// does not hear.
    OnAdvance,
    /// The named side's discard phase has just ended — including when it was
    /// skipped entirely because they were already within hand size, since
    /// the phase still "ends" in rules terms. Fires against that side's
    /// identity only. e.g. Jinteki: Restoring Humanity's "when your discard
    /// phase ends, if there is a facedown card in Archives, gain 1
    /// credit." Deliberately not `OnTurnStart`: the discard phase ends
    /// before control passes, and the two are observably different moments.
    OnDiscardPhaseEnd,
    /// "When your action phase ends" — fired from `turn::end_turn`, before
    /// the end-of-turn paid-ability window, for the ending side's identity
    /// and its rig (Runner) or rezzed installs (Corp): Cacophony's
    /// sabotage; Mercia B4LL4RD and Nebula Talent Management later. A
    /// separate trigger from `OnDiscardPhaseEnd`, which is a later step.
    OnActionPhaseEnd,
    /// "Whenever you install a card" — fired for every Runner install
    /// against the whole rig and the identity, with the *reacting* card as
    /// the acting card and the install in the triggering event. Distinct
    /// from `OnInstall`, which reaches the just-installed card itself (and
    /// the identity) and could not be widened without every "when you
    /// install this" card firing on every install. Bling. Noise and
    /// Cookbook narrow it to a virus program with `when:
    /// Card(HasSubtype(Virus))`, which was the variant `OnVirusInstalled`.
    OnCardInstalled,
    /// "Whenever you do damage" — fired against the *dealing* side's
    /// identity when `GameEvent::DamageTaken` resolves. Only the Corp deals
    /// damage in this pool, so the dealer is the damaged side's opponent.
    /// AU Co.: The Gold Standard in Clones counts it. Distinct from
    /// `OnDamageAboutToResolve`, which fires before the damage lands, while
    /// it is parked for prevention.
    OnDamageDealt,
    /// "Whenever you trash 1 or more cards from HQ" — fired against the
    /// Corp's identity once per batch (`GameEvent::CardsTrashedFromHq`),
    /// not once per card, which is what "1 or more" asks for. AU Co.
    /// again; Hansei Review is what trashes from HQ in its deck.
    OnCardsTrashedFromHq,
    /// "Whenever you gain credits through an ability on a card" — fired
    /// against the gaining side's identity from
    /// `GameEvent::AbilityGainedCredits`, which the credit-gaining effects
    /// emit alongside `CreditsGained` when a card is resolving. The Zwicky
    /// Group: Invisible Hands narrows it to "an agenda or operation" with
    /// `TriggeredEffect::when`. A separate event rather
    /// than a field on `CreditsGained`: the field would have had to be
    /// threaded through twenty-six construction sites, most of which have
    /// no card to name (a click for credits, a trace payout).
    OnAbilityGainedCredits,
    /// "When you forfeit this agenda" — fired against the forfeited agenda
    /// itself as it leaves the score area (`GameEvent::AgendaForfeited`).
    /// Greenmail pays 4[c] for being spent this way.
    OnForfeit,
    /// "Whenever the Runner approaches a piece of ice protecting this
    /// server" — fired against that server's rezzed root installs when
    /// `GameEvent::IceApproached` is emitted, the same audience
    /// `OnApproachServer` uses one step later. Mitra Aman swaps the ice
    /// being approached out. Distinct from `OnEncounter`, which needs the
    /// ice rezzed and the Runner committed to it.
    OnIceApproached,
    /// "Whenever you play an operation", "the first time each turn you play
    /// an event" — every `GameEvent::OperationPlayed` and
    /// `GameEvent::EventPlayed`, heard by the player who played it, whatever
    /// the card's subtype: Nebula Talent Management: Making Stars reads
    /// every operation. Weyland Consortium: Building a Better World reads
    /// the transactions, with `when: Card(HasSubtype(Transaction))` — it
    /// had a variant of its own, `OnTransactionPlayed`, until a trigger
    /// could take a filter.
    ///
    /// Was `OnOperationPlayed`, heard of operations alone. Touchstone is
    /// the first card to hear an event played by a card other than the
    /// event, and `OnPlay` could not be it: `OnPlay` is how the DSL spells
    /// a play's *resolution*, a step of its own that nobody orders
    /// (`dispatcher::dispatch_event`), so a Touchstone hearing it would
    /// have been ordered against the event it heard. The Corp only ever
    /// plays operations and the Runner events, so the two cards that
    /// heard it before hear exactly what they did.
    OnCardPlayed,
    /// "Whenever a tag is removed" — heard when the Runner loses a tag by
    /// any route (`TagRemoved`, `TagsRemoved`, `TagsCleared`). Synapse
    /// Global: Faster than Thought installs off it, including off its own
    /// remove-a-tag ability. The moment is the remover's, so "whenever
    /// **you** remove 1 or more tags" (Valentina Ferreira Carvalho) is this
    /// trigger with `when: Whose(Runner)` (`Trigger::states_whose`).
    OnTagRemoved,
    /// "The first time each turn you take bad publicity" — fired against
    /// the Corp's cards when the Corp takes bad publicity by any route
    /// (`GameEvent::BadPublicityGiven`): Editorial Division: Ad Nihilum.
    /// "Take" and "give" are one event, the Corp's (CR 1.14.2e: the Corp
    /// controls every bad publicity counter), so Witch Hunt's "take 1 bad
    /// publicity" and Take a Dive's "give the Corp 1 bad publicity" are
    /// both heard. Composition didn't work: no existing trigger's moment
    /// is a counter landing on the Corp, and `OnTagsGiven` is the Runner's.
    OnBadPublicityTaken,
    /// "When the Runner passes this ice" (Vertigo), "the first time each
    /// turn you pass the outermost piece of ice … after fully breaking it"
    /// (Sipa) — `GameEvent::IcePassed`, about the ice passed. What was
    /// true of the ice as it was passed (outermost, fully broken during
    /// the encounter just ended, CR 6.1.3f) is the moment's to say
    /// (`EventFilter::Ice`), because both are conditions on the event and
    /// neither is a card's type. Composition didn't work: no moment was a
    /// pass, and `OnIceApproached` is about the server, one step inward.
    ///
    /// **A facedown ice is not "this" to itself here.** An unrezzed piece
    /// of ice is passed without being encountered, and its "when the
    /// Runner passes this ice" is inactive (CR 9.1.7) — none of the
    /// exceptions in CR 9.1.8 is a pass — so the moment is about the ice
    /// only while it is rezzed (`listeners::moments`).
    OnIcePassed,
    /// "The first time each turn you break a subroutine on a piece of ice
    /// with 0 or less strength" (The Tungsten Tailor) —
    /// `GameEvent::SubroutineBroken`, about the ice. The strength it was
    /// broken at is on the event (`EventFilter::Ice`).
    OnSubroutineBroken,
    /// "Whenever the Runner … fully breaks this ice" (Lethe): the first
    /// time during an encounter that every subroutine on the ice is broken
    /// (CR 6.5.7a), `GameEvent::IceFullyBroken`. A moment of its own, not
    /// a filter on `OnSubroutineBroken`, because the rules name it and it
    /// happens once an encounter however many subroutines break together.
    OnIceFullyBroken,
    /// "Whenever the Runner bypasses … this ice" (Lethe), CR 6.5.8 —
    /// `GameEvent::IceBypassed`.
    OnIceBypassed,
    /// "Whenever an encounter with this ice ends" (Knowledge Seeker) —
    /// `GameEvent::EncounterEnded`, about the ice, however the encounter
    /// ended (CR 6.9.3e, and CR 6.1.4 for an encounter "end the run" ends
    /// with the run). Composition didn't work: `OnIcePassed` misses the
    /// commonest ending of all for ice that ends the run, and nothing else
    /// happens after the last subroutine.
    OnEncounterEnded,
    /// "When the Runner spends credits from outside their credit pool
    /// during a run against this server" (Shackleton Grid) —
    /// `GameEvent::CreditsSpentFromOutsidePool`, once per payment, about
    /// the server of the run it was made in, so "this server" is
    /// `Subject::This` as it is for every other run moment. A spend
    /// outside a run, and any spend of the Corp's, is an occurrence of
    /// nothing until a card listens for one (`listeners::moments`).
    /// Composition didn't work: no moment was a payment, and the three
    /// spend events are one per pool.
    OnCreditsSpentOutsidePool,
    /// "Whenever 2 or more facedown cards in Archives are turned faceup"
    /// (Nurse Hạnh) — `GameEvent::ArchivesTurnedFaceup`, about how many
    /// (`TriggerAbout::Cards`), which a `when` narrows
    /// (`EventFilter::AtLeast`). Composition didn't work: no moment was
    /// Archives turning over, and the breach did it without a word.
    OnArchivesTurnedFaceup,
    /// "Whenever you trash a piece of hardware (from any location)" (Hiram
    /// "0mission" Svensson) — `GameEvent::CardTrashed`, heard by the player
    /// who carried the trash out (CR 1.14.5a: "these conditions are only
    /// met when that player is the one to carry out the relevant effect"),
    /// about the card trashed. A trash the rules make (`by: None`) is an
    /// occurrence of nothing. Which card is the `when`'s to say; where it
    /// was is `TrashedFrom`'s, and how an install stood as it went
    /// (`TrashedRezzed`, `TrashedFromThisServer`) the event's `install`.
    /// `InstalledCard` is refused, because the card is no longer installed.
    /// Composition didn't work: `OnTrashedFromAccess` is one kind of trash
    /// and `OnCardsTrashedFromHq` a batch from one zone.
    OnCardTrashed,
    /// "[interrupt] → When this asset would be uninstalled" (Luana
    /// Campos) — `GameEvent::AboutToBeUninstalled`, announced by
    /// `rules::uninstall`, the one door a Corp install leaves the table
    /// through (a card stops being installed "for any reason", CR 8.5.1b),
    /// before the card goes: a "would" is an interrupt, marked pending
    /// while the instruction is imminent and only if it is active (CR
    /// 9.9.3d, 9.9.4b). About the card, and only ever printed about the
    /// card itself. Composition didn't work: `OnCardTrashed` is
    /// heard after the card has gone, and with it what it hosted, and a
    /// trash is one way of being uninstalled among several.
    OnWouldBeUninstalled,
    /// "When you flip this identity to this side" (Méliès U) —
    /// `GameEvent::IdentityFlipped`, dispatched by `Effect::FlipIdentity`.
    /// Heard by the side just turned up, which is the flipped identity's
    /// only active text (CR 3.1.1a), so a back side's
    /// `EffectRequirement::IdentityFlipped` is the "to this side". About
    /// the controller's own identity and nothing a card could point at.
    /// Composition didn't work: nothing else is heard at a flip, and
    /// Méliès U's back sides do something only then.
    OnIdentityFlipped,
    /// "The first time each turn you take an action on an installed
    /// resource" (Juli Moreira Lee) — `GameEvent::AbilityActivated` for an
    /// ability that is an action (CR 9.5.2a: its cost begins with [click]),
    /// heard by the player who took it, about the card whose ability it
    /// was. Which card is the `when`'s to say (`InstalledCard(CardType(
    /// Resource))`). Heard as the cost is paid, "when used" (CR 9.5.7b),
    /// with the cost's events. Composition didn't work: no moment was the
    /// use of an ability, and `OnAbilityGainedCredits` is one effect.
    OnActionTaken,
    /// "When the Corp purges virus counters" (Physarum Entangler) —
    /// `GameEvent::VirusCountersPurged`, by the basic action or a card's
    /// text (Flyswatter), heard by every active card whatever it hosts: a
    /// program that holds no counters still hears the purge (CR 10.1.2).
    /// The Corp's moment, about nothing a card could point at.
    /// Composition didn't work: no moment was a purge, which was recorded
    /// and never dispatched.
    OnVirusCountersPurged,
    /// "Whenever this upgrade moves to the root of a server" (Isaac
    /// Liberdade) — `GameEvent::CardMoved`, a root card moved to another
    /// server's root by a card's text (`Effect::MoveThisCardToRoot`, and
    /// the prompt that resolves as it), about the card that moved.
    /// Composition didn't work: a move is not an install (it has no
    /// `CardInstalled`), and was an occurrence of nothing.
    OnCardMoved,
    /// "Whenever you finish resolving an operation or an action on an
    /// **expendable** card" (Nuvem SA: Law of the Land) —
    /// `GameEvent::FinishedResolving`, announced once everything the
    /// resolution parked has resolved, about the card resolved. Composition
    /// didn't work: `OnCardPlayed` is the play, heard before the operation
    /// resolves, and nothing marked a resolution's end.
    OnFinishedResolving,
    /// "Whenever you trash a card from R&D" (Nuvem SA's "the first time you
    /// trash a card from R&D during each of your turns") —
    /// `GameEvent::CardsTrashedFromRnD`, once per batch, the R&D twin of
    /// `OnCardsTrashedFromHq`. The trasher's moment (`CardsTrashedFromRnD::
    /// by`), about nothing a card could point at: the trashed cards are
    /// facedown. Composition didn't work: `OnCardTrashed` names no zone,
    /// and the turn log counts it by the card's type.
    OnCardsTrashedFromRnD,
    /// "Whenever host ice is rezzed **or derezzed**" (Saci) —
    /// `GameEvent::CardDerezzed`, about the card turned facedown, whoever
    /// did it: a card's text (Tranquilizer, Maglectric Rapid) or a cost
    /// (Brasília Government Grid, Stegodon MK IV). Composition didn't work:
    /// a derez was an occurrence of nothing, so no card could hear one.
    OnDerez,
    /// "Whenever you breach R&D" (Rotary, Pretty Mary da Silva, Devadatta
    /// Drone), "the first time each turn you breach HQ" (Docklands Pass),
    /// "whenever you breach R&D during a run on Archives" (Eru
    /// Ayase-Pessoa) — `GameEvent::BreachBegun`, about the server breached,
    /// which is not always the one run (CR 7.3.1). Composition didn't
    /// work: these cards heard `OnSuccessfulRun`, and a run on HQ that
    /// breaches R&D instead (Beatriz Friere Gonzalez) is a success on HQ.
    /// What they add is "access additional cards", which must be applied
    /// before the random access limit is set (CR 7.3.5b), so the breach
    /// waits for them.
    OnBreach,
    /// "Whenever you finish taking an action" (Wage Workers) —
    /// `GameEvent::ActionFinished`, announced once everything the action
    /// parked has resolved, heard by the player who took it and about no
    /// card: the moment names which action it was (`SameAction`), for
    /// "that action" to be counted by (`Amount::TimesThisActionThisTurn`).
    /// Composition didn't work: `OnActionTaken` is an ability's use, heard
    /// as its cost is paid, and a basic action was an occurrence of
    /// nothing.
    OnActionFinished,
    /// "Whenever a subroutine resolves during that run" (Raindrops Cut
    /// Stone) — `GameEvent::SubroutineFired`, about the ice whose
    /// subroutine it was. Announced before the subroutine's effect, so a
    /// run event hears a subroutine that ends its run ("including a
    /// subroutine that ends the run") while the run, and the event's place
    /// in the play area, still stand.
    OnSubroutineResolved,
    /// "The first time each turn you suffer core damage" (Esâ Afontov:
    /// Eco-Insurrectionist) — `GameEvent::DamageTaken`, heard by the
    /// player who suffered it, about the kind of damage, which a `when`
    /// narrows (`EventFilter::Damage`). Composition didn't work:
    /// `OnDamageDealt` is the responsible player's ("whenever you do
    /// damage"), and `OnDamageAboutToResolve` is a "would", before any
    /// card is lost and while the damage can still be prevented.
    OnDamageSuffered,
    /// "The first time each turn you spend credits from an installed card"
    /// (The Twinning) — `GameEvent::CreditsSpentFromOutsidePool` with some
    /// of its credits off a card's hosted pool (`from_installed`), heard by
    /// the spender, once per payment however many cards paid, and about
    /// the first card they came off (`first_host`), which Keiko's "an
    /// installed companion card" narrows with a `when`. Composition
    /// didn't work: `OnCreditsSpentOutsidePool` is about the server of the
    /// run, for Shackleton Grid, and counts bad publicity's and a run
    /// event's credits, which come off no card.
    OnCreditsSpentFromInstalledCard,
    /// "1 or more cards are trashed from your grip or stack" (Buffer
    /// Drive's "the first time each turn") — `GameEvent::
    /// CardsTrashedFromGripOrStack`, once per instruction however many
    /// cards it trashed, which the event names so the trigger's effects
    /// can choose among "those cards" (`CardFilter::TrashedThisWay`,
    /// written in as the trigger fires). Passive, so heard whoever did it:
    /// damage the Corp does, a mill, the Runner's own cost. Composition
    /// didn't work: `OnCardTrashed` is one moment per card, so a first time
    /// is one card and not the batch, the turn log counts it by type and
    /// not by pile, and the Corp's twins (`OnCardsTrashedFromHq`,
    /// `OnCardsTrashedFromRnD`) are each one pile and name no cards.
    OnCardsTrashedFromGripOrStack,
    /// "You use a piece of hardware" (The Back) — `GameEvent::
    /// AbilityActivated`, any paid ability, an action or not, heard by the
    /// player who used it, about the card (CR 9.1.6: resolving an optional
    /// ability is using it and its card). Heard as the cost is paid, as
    /// `OnActionTaken` is, which is the same moment narrowed to an action.
    /// Composition didn't work: an action is never taken during a run
    /// (CR 5.2.2a), and "during a run" is The Back's.
    OnAbilityUsed,
    /// "Whenever a Corp card ability causes the Runner to spend or lose at
    /// least 1[credit]" (GameNET: Where Dreams are Real) —
    /// `GameEvent::AbilityTookCredits`, heard by the player who spent or
    /// lost, about the card whose ability made them: a loss a resolving
    /// card's text causes (Gold Farmer's), the cost of that card's ability
    /// (F2P's "2[credit]: Break 1 subroutine", which the Runner pays), a
    /// paid choice it offers ("end the run unless the Runner pays") and a
    /// bid in the trace it began (CR 10.8.6d). GameNET names the card's
    /// side and the player with `EventFilter::OwnedBy`. Composition didn't
    /// work: no moment was a loss, and the three spend events say which
    /// pool paid, never whose ability asked.
    OnAbilityTookCredits,
    /// "The first time each turn **you reveal** a card" (Hyoubu Institute:
    /// Absolute Clarity) — `GameEvent::CardRevealed`, heard by the player
    /// who revealed it (`CardRevealed::by`), whoever owns the card, and a
    /// selection that reveals what it chose (`CardsSelected::revealed`),
    /// heard by its chooser. About the card revealed, which both players
    /// saw. Composition didn't work: no moment was a reveal, and the
    /// event said whose card it was, never who showed it.
    OnCardRevealed,
    /// "The first time each turn **you would draw** any number of cards"
    /// (The Class Act) — `GameEvent::AboutToResolve` of a
    /// `WouldHappen::Draw`, the draw parked by `rules::prevention` so that
    /// what hears it resolves before the cards are drawn, as
    /// `OnDamageAboutToResolve` resolves before the damage. Announced for
    /// every draw of the Runner's and none of the Corp's, which no card in
    /// the pool hears. Composition didn't work: a draw was not a moment
    /// until it had happened (`CardDrawn`, heard by nothing).
    OnDrawAboutToResolve,
    /// "Whenever you create a server" (Turtlebacks), and "the first time
    /// each turn you create a remote server" (Near-Earth Hub: Broadcast
    /// Center) — `GameEvent::ServerCreated`, emitted where an install's
    /// destination is a remote nothing was installed in (CR 8.5.16e: "If
    /// the card is to be the first card in the root of or protecting a new
    /// remote server, that server is created"), about that server and
    /// heard by the Corp, the only player who creates one (4.6.8b).
    /// Composition didn't work: `OnInstall` is about the card, and whether
    /// its server is new is not a fact of the card nor one the state still
    /// holds afterwards — an install over the only card of a remote leaves
    /// a remote of one card that was not created. A moment of its own, so
    /// "the first time each turn" counts creations, not installs.
    OnServerCreated,
    /// Not a moment: a conditional ability with a *static* condition (CR
    /// 9.6.7) — Parasite's "When the strength of host ice is 0 or less,
    /// trash it". The condition is the entry's `requirement`, and no event
    /// is an occurrence of it (`listeners::moments` never names it): the
    /// checkpoint looks for an active card whose condition is true and
    /// marks the ability pending (CR 9.6.7a, `checkpoint::static_conditions`),
    /// one instance per source at a time (9.6.7c). Composition didn't work:
    /// every other trigger waits for an event, and the strength of a piece
    /// of ice changes with no event at all — a virus counter on the Trojan,
    /// a lingering effect running out.
    WhileTrue,
}

/// What a run's moment about a piece of ice says of it beyond the card —
/// the words a card prints about the ice passed or broken: "the outermost
/// piece of ice protecting a server" (CR 4.6.9b), "after fully breaking
/// it" (CR 6.1.3f), "a piece of ice with 0 or less strength". Each is
/// public (a strength is on the table during its encounter) and each is
/// read off the event, never the state, so a trigger asked again where it
/// fires (`listeners::when_admits`) gets the answer it was planned on.
///
/// As a filter (`EventFilter::Ice`), a `true` is required and a `false` is
/// "either"; as what a moment was (`listeners::Moment::ice`), each is what
/// held. Booleans rather than a number because "the first time each
/// turn" counts them in the turn log (`turn_log::Class::Ice`), which keys a
/// fixed-size table and cannot hold a strength.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IceFacts {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outermost: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub after_fully_breaking: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub at_most_zero_strength: bool,
    /// "Passes a **rezzed code gate or sentry**" — Sisyphus Protocol, the
    /// one card that asks. Rezzed because an unrezzed piece of ice's type
    /// is not public; and one fact rather than a type, because the facts
    /// are bits of a turn-log column and a type would multiply them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rezzed_code_gate_or_sentry: bool,
    /// "Breaks a **printed** subroutine on this ice" — Gold Farmer: the
    /// subroutine broken was printed on the ice, not gained (CR 6.5.7d's
    /// "gains" — Echo's, Envelopment's). Read off the break
    /// (`GameEvent::SubroutineBroken::printed`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub printed_subroutine: bool,
    /// "Passes a **rezzed** piece of **bioroid** ice" — Haas-Bioroid:
    /// Architects of Tomorrow. Bioroid is a subtype, not one of the ice
    /// types `rezzed_code_gate_or_sentry` reads, and rezzed for the same
    /// reason. Read off the pass (`GameEvent::IcePassed::rezzed_bioroid`).
    ///
    /// **It shares a column bit with `at_most_zero_strength`** (`bits`):
    /// the log counts each trigger in a row of its own, and no trigger's
    /// moment states both (`stated_by`: this is a pass's, that a break's),
    /// so within any row the bit means one fact. A sixth bit of its own
    /// would have made the facts 64 columns, past the table's 40, and
    /// widened every row of a log that is copied with every `GameState`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rezzed_bioroid: bool,
}

impl IceFacts {
    /// Whether every fact `self` requires holds in `moment`.
    pub fn admits(self, moment: IceFacts) -> bool {
        (!self.outermost || moment.outermost)
            && (!self.after_fully_breaking || moment.after_fully_breaking)
            && (!self.at_most_zero_strength || moment.at_most_zero_strength)
            && (!self.rezzed_code_gate_or_sentry || moment.rezzed_code_gate_or_sentry)
            && (!self.printed_subroutine || moment.printed_subroutine)
            && (!self.rezzed_bioroid || moment.rezzed_bioroid)
    }

    /// The facts `trigger`'s moment states; a filter may ask no others,
    /// since the rest are always `false` there (`CardDefinition::validate`).
    pub fn stated_by(trigger: Trigger) -> IceFacts {
        match trigger {
            Trigger::OnIcePassed => IceFacts { outermost: true, after_fully_breaking: true, rezzed_code_gate_or_sentry: true, rezzed_bioroid: true, ..IceFacts::default() },
            Trigger::OnSubroutineBroken => IceFacts { at_most_zero_strength: true, printed_subroutine: true, ..IceFacts::default() },
            _ => IceFacts::default(),
        }
    }

    /// How many columns of the turn log the facts take: five bits, the
    /// sixth fact sharing one (`rezzed_bioroid`).
    pub const COLUMNS: usize = 32;

    /// The facts as a number, 0..`COLUMNS` — a column of the turn log's row
    /// for the trigger. `rezzed_bioroid` and `at_most_zero_strength` are
    /// the same bit, which no one trigger states both of.
    pub fn bits(self) -> usize {
        usize::from(self.outermost)
            | usize::from(self.after_fully_breaking) << 1
            | usize::from(self.at_most_zero_strength || self.rezzed_bioroid) << 2
            | usize::from(self.rezzed_code_gate_or_sentry) << 3
            | usize::from(self.printed_subroutine) << 4
    }

    /// Every combination of the six, the sixth in the top bit of the index.
    pub const ALL: [IceFacts; 64] = {
        let none = IceFacts { outermost: false, after_fully_breaking: false, at_most_zero_strength: false, rezzed_code_gate_or_sentry: false, printed_subroutine: false, rezzed_bioroid: false };
        let mut all = [none; 64];
        let mut bits = 0;
        while bits < 64 {
            all[bits] = IceFacts {
                outermost: bits & 1 != 0,
                after_fully_breaking: bits & 2 != 0,
                at_most_zero_strength: bits & 4 != 0,
                rezzed_code_gate_or_sentry: bits & 8 != 0,
                printed_subroutine: bits & 16 != 0,
                rezzed_bioroid: bits & 32 != 0,
            };
            bits += 1;
        }
        all
    };
}

/// Which occurrences of its trigger a `TriggeredEffect` hears: the one that
/// happens to the card itself, or every one.
///
/// One printed word decides it — "when you score **this agenda**" against
/// "whenever **an agenda** is scored" — and until this existed the word was
/// not in the card data at all. `Trigger::OnAgendaScored` meant *this one*
/// on Hostile Takeover and *any* on Jinteki: Personal Evolution, and what
/// told them apart was `rules::dispatcher` handing the event to the scored
/// agenda and to the identity in two separate, hand-written steps. That is
/// a card rule kept in Rust: a rezzed asset printed "whenever you score
/// another agenda" could be written in JSON and would never be asked.
///
/// For an event about a server (an approach, a run ending) `This` reads
/// "this server": the card is installed in or protecting it — Anoetic
/// Void's "whenever the Runner approaches **this server**".
///
/// Not a `CardFilter`, and deliberately two-valued: "is it me" needs the
/// listener's own install handle, which no filter over the *other* card can
/// answer. Narrowing `Any` by what the other card is belongs to
/// `TriggeredEffect::when` (`EventFilter::Card`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Subject {
    This,
    Any,
}

/// Whose occurrences of a moment a trigger hears — the "you" in its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hears {
    /// The trigger is phrased about its controller ("when **your** turn
    /// begins", "whenever **you** install"): a card hears it only when the
    /// moment is its own side's.
    OwnSide,
    /// Phrased about the game ("whenever an agenda is scored or stolen",
    /// "whenever the Runner approaches"): either side's cards hear it.
    Everyone,
}

/// What a `Trigger`'s moment is about — see `Trigger::about`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerAbout {
    Nothing,
    Card,
    Server,
    /// A kind of damage — "you would suffer **net** damage". Nothing a card
    /// can be "this" of, so no `subject` goes with it.
    Damage,
    /// A number of cards, none of which is named — "2 or more facedown
    /// cards in Archives". Nothing to be "this" of either; a `when` may ask
    /// how many (`EventFilter::AtLeast`).
    Cards,
}

/// Which occurrences of its trigger a `TriggeredEffect` means, read off
/// what the occurrence is about: "a successful run **on HQ**", "whenever you
/// play **a transaction**", "whenever you install **a virus program**".
///
/// **A pure function of the event, and part of the trigger condition — not
/// an `EffectRequirement`.** The two look alike and are different things on
/// the printed card. "Whenever you make a successful run on HQ" is *when*:
/// a run on R&D is not an occurrence of it, nothing is pending, there is
/// nothing to order and nothing that could become true later. "…if you have
/// not already this turn" is an intervening *if*: the trigger met its
/// condition and is asked, and the answer depends on the state at the
/// moment it resolves — which is why an idle one is still queued behind the
/// ones a player orders. `rules::listeners` evaluates `when` in the scan, so
/// a card whose filter fails was never a listener.
///
/// Until this existed the only way to narrow a trigger by its event was a
/// variant: `OnSuccessfulRunOnHq`, `OnSuccessfulRunOnRnD`,
/// `OnSuccessfulRunOnCentralServer`, `OnTransactionPlayed` and
/// `OnVirusInstalled` were five names for three triggers, and the next
/// "whenever you install a piece of hardware" would have been a sixth.
///
/// Two variants, one per thing a moment can be about (`TriggerAbout`), and
/// `CardDefinition::validate` refuses the wrong one for the trigger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventFilter {
    /// The card the moment is about matches — the installed card, the
    /// operation played, the card rezzed, the card whose ability paid out.
    /// Decided off the card's definition alone: a `CardFilter` about the
    /// installed *instance* (`Rezzed`, `NotInstalledThisTurn`) passes
    /// everything here, as it does wherever only a definition is in hand.
    /// This took over `EffectRequirement::TriggeringCardMatches`, which
    /// said the same thing as an "if": Barry "Baz" Wong was a listener on
    /// every rez, queued and asked, for a rez of an asset that his text is
    /// not about.
    Card(crate::dsl::CardFilter),
    /// The server the moment is about is one of these. "A central server"
    /// is the three of them by name; nothing in the pool is printed about
    /// "a remote server", which a list of ids could not say and a variant
    /// here would.
    Server(Vec<crate::rules::ServerId>),
    /// The server the moment is about is the Runner's mark (CR 10.11) —
    /// Info Bounty's "the first time each turn a run **on your mark**
    /// ends". Read off the state as the scan runs (`lingering::mark`), and
    /// matching nothing while there is no mark. Composition didn't work: a
    /// `Server` list is written in the card file, and the mark is chosen at
    /// random each turn.
    Mark,
    /// The server the moment is about is the one the listening card chose
    /// (`Effect::ChooseServer`, `lingering::chosen_server`) — Hyoubu
    /// Precog Manifold's "whenever the Runner makes a successful run on
    /// **the chosen server**". Matching nothing while there is no choice.
    /// `Mark`'s shape for the same reason: the server is the state's, not
    /// the card file's. `CardFilter::InChosenServer` says it of a card.
    ChosenServer,
    /// The server the moment is about is protected by ice — Argus
    /// Crackdown's "a successful run on **a server protected by ice**",
    /// asked as the moment is heard (CR 9.6.5c), rezzed or not (CR 8.1.2:
    /// unrezzed ice protects its server too). Composition didn't work: a
    /// `Server` list names servers, and this is a fact about whichever
    /// server it is.
    ProtectedByIce,
    /// The card the moment is about matches **and was installed when it
    /// happened** — Aggressive Trendsetting's "the first time the Runner
    /// trashes an **installed** Corp card", against a card trashed out of
    /// HQ or R&D. Composition didn't work: `Card` is decided off the
    /// definition alone, and the state half ("was it on the table") was an
    /// intervening if (`CurrentlyAccessingInstalledCard`), which is the
    /// wrong place for it once "the first time each turn" counts what the
    /// condition admits — a trash out of HQ would have been the turn's
    /// first. Read off the moment (`listeners::About::Card::installed`),
    /// which the event states, so it cannot have changed since.
    InstalledCard(crate::dsl::CardFilter),
    /// The damage the moment is about is of this kind — Net Shield's "the
    /// first time each turn you would suffer **net** damage". The third
    /// thing a moment can be about (`TriggerAbout::Damage`); composition
    /// didn't work because neither of the other two is a kind of damage,
    /// and as an intervening if a point of meat damage would have been the
    /// turn's first (the Turn History Rule).
    Damage(crate::dsl::DamageType),
    /// What was true of the ice a run's moment is about (`IceFacts`):
    /// Sipa's "the outermost piece of ice … after fully breaking it", The
    /// Tungsten Tailor's "a piece of ice with 0 or less strength".
    /// Composition didn't work: `Card` is decided off the definition, and
    /// each of these is a fact about the ice at that moment; as an
    /// intervening if, "the first time each turn" would have counted every
    /// pass and every break (the Turn History Rule). Only on a trigger
    /// about a piece of ice in a run (`Trigger::is_about_ice_in_a_run`).
    Ice(IceFacts),
    /// At least this many cards — Nurse Hạnh's "whenever **2 or more**
    /// facedown cards in Archives are turned faceup". Only on a trigger
    /// about a number of cards (`TriggerAbout::Cards`). Composition didn't
    /// work: no other filter reads a number, and as an intervening if the
    /// condition would sit where the printed sentence does not put it.
    AtLeast(u32),
    /// The moment is this player's — "when **the Runner's** discard phase
    /// ends", printed on a Corp identity (Méliès U). Only on a trigger
    /// whose moment names a player (`Trigger::states_whose`): on one
    /// phrased about its controller (`Hears::OwnSide`) it replaces the
    /// "your", and `listeners` hears the named side's occurrences instead
    /// of the controller's; on a tag's removal it is "whenever **you**
    /// remove", which a trigger about the game cannot otherwise say. Composition didn't work: `OwnSide` was decided
    /// before `when` was read, so no filter could reach the other side's
    /// phase, and a `Trigger` per "the Runner's" would be the variant the
    /// Listener Rule forbids.
    Whose(crate::rules::Side),
    /// The moment is anyone's, or nobody's — Aniccam's "the first time each
    /// turn an event **is trashed**", Simulchip's "if an installed program
    /// **has already been trashed** this turn": a trigger phrased about its
    /// controller ("you trash", `Hears::OwnSide`) made passive. Only on a
    /// trigger whose moment names a player (`Trigger::states_whose`), as
    /// `Whose`. Composition didn't work: `Whose` names one player, and two
    /// entries, one per player, both count a moment nobody carried out (a
    /// played event's trash, `TrashedFrom::PlayArea`), so neither would be
    /// the first.
    Anyone,
    /// The Corp install came out of HQ, or did not — Stoke the Embers'
    /// "When you install this agenda **from anywhere except HQ**"
    /// (`InstalledFromHq(false)`). Read off the moment, which the event
    /// states (`GameEvent::CardInstalled::from_hq`). Only on `OnInstall`.
    /// Composition didn't work: no filter read where a card came from, and
    /// as an intervening if it would sit where the printed sentence does
    /// not put it.
    InstalledFromHq(bool),
    /// The Corp installed the card in the root of or protecting a server
    /// of this kind — A Teia: IP Recovery's "install a card in the root of
    /// or protecting **a remote server**". Read off the moment, which the
    /// event states (`GameEvent::CardInstalled::server`), and only on the
    /// Corp's `OnInstall`, as `InstalledFromHq` is. Composition didn't
    /// work: `Server` names servers, and a remote's number is not known to
    /// the card.
    InstalledIn(crate::dsl::ServerKind),
    /// The card the moment is about is `owner`'s, **and** the moment is
    /// `whose` — Active Policing's "the Runner … trashed a Corp card",
    /// where the Runner's trash of their own program is not one. Neither
    /// half alone says it: `Whose` admits every card, and a `Card` filter
    /// on a trigger phrased about its controller (`OnCardTrashed`) hears
    /// the controller's moments, which for a Corp card are the Corp's own
    /// trashes. Counted in the log because a card's owner is public
    /// wherever its type is not: every card the log counts `Unseen` is a
    /// Corp card (`turn_log::Occurrences::meant_by`).
    OwnedBy { owner: crate::rules::Side, whose: crate::rules::Side },
    /// The moment was this card's own doing — Lobisomem's "whenever **it**
    /// fully breaks a code gate": the ice fully broken by abilities on this
    /// install alone (CR 6.5.7b, `GameEvent::IceFullyBroken::by`). Only on
    /// `OnIceFullyBroken`, the one moment that names an object that did it.
    /// Composition didn't work: the moment is about the ice, and nothing
    /// else a filter reads says which breaker broke it.
    ByThis,
    /// The card the moment is about is the one this card is hosted on — a
    /// Trojan's "whenever **host ice** is rezzed or derezzed" (Saci),
    /// "whenever you pass **host ice**" (Pichação). Asked of the listening
    /// install, where it is hosted as the moment is heard
    /// (`InstalledRunnerCard::hosted_on_ice`). Composition didn't work: a
    /// `Card` filter is about the kind of card, and `Subject::This` is the
    /// Trojan itself; the one requirement that named a host
    /// (`EncounteringHostIce`) is an intervening if about the run, which a
    /// rez is not.
    Host,
    /// The Corp installed the card in the root of a server — Lago Paranoá
    /// Shelter's "The first time each turn **the Corp installs a card in
    /// the root of a server**". Read off the card installed: ice is the only
    /// type installed protecting a server and is never installed in a root
    /// (CR 3.4.2), so every other Corp install is in one. It says whose
    /// moment it is, as `Whose` does, and only on `OnInstall`. The turn log
    /// counts a Corp install of ice as ice, which the table shows, and every
    /// other Corp install unseen, so "the first" is the first install it
    /// counts unseen (`turn_log::seen_anyway`). Composition didn't work:
    /// `Whose(Corp)` admits the Corp's ice as well, and a `Card` filter on a
    /// trigger about its controller hears the controller's installs, which
    /// for a Runner card are the Runner's.
    InRoot,
    /// The Corp installed the card in the root of the server this card is
    /// in — Tranquility Home Grid's "The first time each turn **you install
    /// a card in the root of this server**". Read off the moment (the card
    /// is not ice, and `GameEvent::CardInstalled::server` is the listener's
    /// server), and only on the Corp's `OnInstall`. **Its first time is
    /// counted on the copies in that root** (`turn_log::record` bumps every
    /// card in the root, the one installed included, `CopyTurn`), not on
    /// the turn, whose log counts kinds of card and never which server: a
    /// card that was in the root all turn saw every install into it, and
    /// one installed this turn saw its own, which already makes any later
    /// install the second. Composition didn't work: `InRoot` is any
    /// server's root, `InstalledIn` a kind of server, and a `Card` filter
    /// naming this server is finer than any column the turn keeps, so
    /// `first_each_turn` could not have counted it.
    InRootOfThisServer,
    /// The card was trashed from one of these places — Strike Fund's
    /// "When this event is trashed **from your grip or stack**". Read off
    /// the moment, which the event states (`GameEvent::CardTrashed::from`),
    /// since the card is in its owner's discard pile by the time anything
    /// hears it. Only on `OnCardTrashed`. Composition didn't work: the
    /// event said only whether the card had been installed, and a card in
    /// a hand and one on top of a deck were the same trash.
    TrashedFrom(Vec<TrashedFrom>),
    /// The card was trashed from the root of, or protecting, the server
    /// this card is in, and not as a step of installing another (CR
    /// 8.5.16c) — Yakov Erikovich Avdakov's "whenever a player trashes a
    /// card (including this upgrade) **from the root of this server or
    /// protecting it, except during installation**". Read off the moment
    /// (`GameEvent::CardTrashed::install`, or the run for an access trash),
    /// since the card is in Archives when its trash is heard, and "this
    /// server" is the listener's — the one it was trashed out of, for the
    /// upgrade hearing its own trash. It names no player, so either
    /// player's trash is heard. Only on `OnCardTrashed` and
    /// `OnTrashedFromAccess`. Composition didn't work: `Server` is about a
    /// moment that is about a server, and a trash is about a card.
    TrashedFromThisServer,
    /// The card was rezzed as it was trashed, and was not trashed as a
    /// step of installing another (CR 8.5.16c) — Ob Superheavy Logistics'
    /// "When you trash **a rezzed card, except during installation**".
    /// Read off the moment (`GameEvent::CardTrashed::install`), since the
    /// card is in Archives when its trash is heard. The two halves are one
    /// word, as `TrashedFromThisServer` fuses the same exception: the one
    /// card that asks whether a trashed card was rezzed asks both. Only on
    /// `OnCardTrashed`. Composition didn't work: a `Card` filter's
    /// `Rezzed` asks the card as it stands now, which is in Archives.
    TrashedRezzed,
    /// Every one of these — Hostile Architecture's "the Runner trashes any
    /// of **your installed** cards", on a trash by a card's text: the card
    /// is the Corp's and the moment the Runner's (`OwnedBy`), **and** it
    /// was installed (`TrashedFrom([Installed])`). Composition didn't work:
    /// a `when` held one filter, each of these says half, and a variant for
    /// the pair would be the first of one per pair a card prints. The
    /// turn log counts the columns every part admits
    /// (`turn_log::Occurrences::meant_by`), so a first time narrowed by
    /// the conjunction is counted as the card means it.
    All(Vec<EventFilter>),
}

impl EventFilter {
    /// `CardFilter::ChosenName` written over as `card`, in a filter on the
    /// card a moment is about (`Effect::with_chosen_name`).
    pub fn with_chosen_name(self, card: &crate::dsl::CardId) -> EventFilter {
        match self {
            EventFilter::Card(filter) => EventFilter::Card(filter.with_chosen_name(card)),
            other => other,
        }
    }

    /// Whether this filter says whose moment it is — "the Runner's", "the
    /// Corp installs" — which `listeners` then reads instead of the
    /// trigger's own "you".
    /// This filter with "your mark" written as the server it is — or as no
    /// server, while there is no mark — for a count that has to name a
    /// column (`turn_log::first_time_on`).
    pub(crate) fn with_mark(&self, mark: Option<crate::rules::ServerId>) -> EventFilter {
        match self {
            EventFilter::Mark => EventFilter::Server(mark.into_iter().collect()),
            EventFilter::All(parts) => EventFilter::All(parts.iter().map(|part| part.with_mark(mark)).collect()),
            other => other.clone(),
        }
    }

    pub(crate) fn names_whose(&self) -> bool {
        match self {
            EventFilter::Whose(_)
            | EventFilter::Anyone
            | EventFilter::OwnedBy { .. }
            | EventFilter::InRoot
            | EventFilter::InRootOfThisServer
            | EventFilter::TrashedFromThisServer => true,
            EventFilter::All(parts) => parts.iter().any(EventFilter::names_whose),
            _ => false,
        }
    }
}

/// Where a card was when it was trashed (`GameEvent::CardTrashed::from`).
/// It replaced a flag saying only whether the card was installed, which
/// Boi-tatá's "your installed cards" still reads (`installed`); Strike
/// Fund's "from your grip or stack" is the first card to ask which pile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrashedFrom {
    /// On the table: in a server, in the rig, or hosted there as an
    /// install.
    Installed,
    /// A player's hand — HQ or the grip.
    Hand,
    /// A player's deck — R&D or the stack.
    Deck,
    /// Anywhere else: hosted on a card without being installed, set aside,
    /// or a card being accessed that was in none of the above.
    Elsewhere,
    /// The play area: an event trashed as it finishes resolving (CR 3.7.1,
    /// 4.6.4e) — the trash Aniccam's "an event is trashed (from any
    /// location)" hears most often. The rules' trash (`by: None`), and the
    /// one rules' trash that is a moment (`listeners::moments`), since an
    /// event is the only card that leaves the play area this way and the
    /// listeners that would hear it ask what it is. An operation is filed
    /// in Archives as it resolves with no trash recorded: no card hears it.
    /// A lockdown, which stays in the play area (CR 3.5.1c), is trashed
    /// from it as the Corp's next turn begins, and that trash is recorded.
    PlayArea,
}

impl TrashedFrom {
    /// Whether the card was on the table (CR 8.5) — the one fact the event
    /// carried before it said which pile.
    pub fn installed(self) -> bool {
        self == TrashedFrom::Installed
    }
}

impl Trigger {
    /// Every trigger, in declaration order — the rows of
    /// `rules::turn_log::TurnLog`, which counts a turn's moments in a flat
    /// array and so needs a trigger to be a number. `index` is that number;
    /// `every_trigger_is_listed_at_its_own_index` holds the two together,
    /// and its exhaustive `match` is what stops a new variant compiling
    /// until it is listed here.
    pub const ALL: [Trigger; 57] = [
        Trigger::OnPlay,
        Trigger::OnRunStart,
        Trigger::OnEncounter,
        Trigger::OnTurnStart,
        Trigger::OnAccessed,
        Trigger::OnTrashedFromAccess,
        Trigger::OnSuccessfulRun,
        Trigger::Paid,
        Trigger::OnInstall,
        Trigger::OnAgendaScored,
        Trigger::OnAgendaStolen,
        Trigger::OnDamageAboutToResolve,
        Trigger::OnRez,
        Trigger::OnApproachServer,
        Trigger::OnRunEnded,
        Trigger::OnBasicDrawAction,
        Trigger::OnTagsGiven,
        Trigger::OnAdvance,
        Trigger::OnDiscardPhaseEnd,
        Trigger::OnActionPhaseEnd,
        Trigger::OnCardInstalled,
        Trigger::OnDamageDealt,
        Trigger::OnCardsTrashedFromHq,
        Trigger::OnAbilityGainedCredits,
        Trigger::OnForfeit,
        Trigger::OnIceApproached,
        Trigger::OnCardPlayed,
        Trigger::OnTagRemoved,
        Trigger::OnBadPublicityTaken,
        Trigger::OnIcePassed,
        Trigger::OnSubroutineBroken,
        Trigger::OnIceFullyBroken,
        Trigger::OnIceBypassed,
        Trigger::OnEncounterEnded,
        Trigger::OnCreditsSpentOutsidePool,
        Trigger::OnArchivesTurnedFaceup,
        Trigger::OnCardTrashed,
        Trigger::OnWouldBeUninstalled,
        Trigger::OnIdentityFlipped,
        Trigger::OnActionTaken,
        Trigger::OnVirusCountersPurged,
        Trigger::OnCardMoved,
        Trigger::OnFinishedResolving,
        Trigger::OnCardsTrashedFromRnD,
        Trigger::OnDerez,
        Trigger::OnBreach,
        Trigger::OnActionFinished,
        Trigger::OnSubroutineResolved,
        Trigger::OnDamageSuffered,
        Trigger::OnCreditsSpentFromInstalledCard,
        Trigger::OnCardsTrashedFromGripOrStack,
        Trigger::OnAbilityUsed,
        Trigger::OnAbilityTookCredits,
        Trigger::OnCardRevealed,
        Trigger::OnDrawAboutToResolve,
        Trigger::OnServerCreated,
        Trigger::WhileTrue,
    ];

    /// This trigger's position in `ALL`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// What this trigger's moment is *about* — a card, a server, or nothing
    /// a card could point at. It decides two things a card file is held to
    /// by `CardDefinition::validate`: whether a `TriggeredEffect` must say
    /// which occurrences it hears (`names_a_subject`), and which kind of
    /// `EventFilter` its `when` may carry, since a server filter on an
    /// install would parse and then never pass.
    ///
    /// Exhaustive on purpose: a new `Trigger` does not compile until someone
    /// decides.
    pub fn about(self) -> TriggerAbout {
        match self {
            Trigger::OnPlay
            | Trigger::OnInstall
            | Trigger::OnCardInstalled
            | Trigger::OnCardPlayed
            | Trigger::OnAccessed
            | Trigger::OnTrashedFromAccess
            | Trigger::OnAgendaScored
            | Trigger::OnAgendaStolen
            | Trigger::OnForfeit
            | Trigger::OnRez
            | Trigger::OnAdvance
            | Trigger::OnEncounter
            | Trigger::OnAbilityGainedCredits
            | Trigger::OnIcePassed
            | Trigger::OnSubroutineBroken
            | Trigger::OnSubroutineResolved
            | Trigger::OnIceFullyBroken
            | Trigger::OnIceBypassed
            | Trigger::OnEncounterEnded
            | Trigger::OnCardTrashed
            | Trigger::OnWouldBeUninstalled
            | Trigger::OnCardMoved
            | Trigger::OnFinishedResolving
            | Trigger::OnDerez
            | Trigger::OnActionTaken
            | Trigger::OnAbilityUsed
            | Trigger::OnAbilityTookCredits
            | Trigger::OnCardRevealed
            | Trigger::OnCreditsSpentFromInstalledCard => TriggerAbout::Card,
            Trigger::OnRunStart
            | Trigger::OnIceApproached
            | Trigger::OnApproachServer
            | Trigger::OnSuccessfulRun
            | Trigger::OnBreach
            | Trigger::OnRunEnded
            | Trigger::OnCreditsSpentOutsidePool
            | Trigger::OnServerCreated => TriggerAbout::Server,
            // A phase, a count or a player, never a card.
            Trigger::OnTurnStart
            | Trigger::OnActionPhaseEnd
            | Trigger::OnDiscardPhaseEnd
            | Trigger::OnBasicDrawAction
            | Trigger::OnTagsGiven
            | Trigger::OnTagRemoved
            | Trigger::OnDamageDealt
            | Trigger::OnCardsTrashedFromHq
            | Trigger::OnCardsTrashedFromRnD
            | Trigger::OnBadPublicityTaken
            | Trigger::OnIdentityFlipped
            | Trigger::OnVirusCountersPurged
            | Trigger::OnActionFinished
            | Trigger::OnCardsTrashedFromGripOrStack
            | Trigger::OnDrawAboutToResolve
            | Trigger::WhileTrue
            | Trigger::Paid => TriggerAbout::Nothing,
            // `OnDamageDealt` would be the second, the day a card prints
            // "whenever you do **meat** damage"; none does.
            Trigger::OnDamageAboutToResolve | Trigger::OnDamageSuffered => TriggerAbout::Damage,
            Trigger::OnArchivesTurnedFaceup => TriggerAbout::Cards,
        }
    }

    /// Whether this trigger's moment is about a piece of ice in a run, so
    /// that it says what was true of the ice (`IceFacts`) and a `when` may
    /// ask it (`EventFilter::Ice`).
    pub fn is_about_ice_in_a_run(self) -> bool {
        matches!(self, Trigger::OnIcePassed | Trigger::OnSubroutineBroken | Trigger::OnSubroutineResolved | Trigger::OnIceFullyBroken | Trigger::OnIceBypassed)
    }

    /// Whether a `TriggeredEffect` using this trigger must say which
    /// occurrences it hears (`TriggeredEffect::subject`) — and one using any
    /// other trigger must not, because there is no "this one" for a turn
    /// beginning.
    pub fn names_a_subject(self) -> bool {
        matches!(self.about(), TriggerAbout::Card | TriggerAbout::Server)
    }

    /// Whether this trigger's moment names a player a card may ask for with
    /// `EventFilter::Whose`: every trigger phrased about "you", and a tag's
    /// removal, whose moment is the remover's (CR 1.14.3a) though Synapse
    /// Global's "whenever a tag is removed" hears anyone's. Other triggers
    /// about the game carry a player (a run is the Runner's) that no card
    /// asks for, and a `Whose` on one would read as a restriction it is not.
    pub fn states_whose(self) -> bool {
        self.hears() == Hears::OwnSide || self == Trigger::OnTagRemoved
    }

    /// Whose moment this trigger hears. Read off how the pool's cards print
    /// it, and exhaustive for the same reason as `about`.
    pub fn hears(self) -> Hears {
        match self {
            Trigger::OnTurnStart
            | Trigger::OnActionPhaseEnd
            | Trigger::OnDiscardPhaseEnd
            | Trigger::OnBasicDrawAction
            | Trigger::OnInstall
            | Trigger::OnCardInstalled
            | Trigger::OnCardPlayed
            | Trigger::OnAdvance
            | Trigger::OnAbilityGainedCredits
            | Trigger::OnDamageDealt
            | Trigger::OnCardsTrashedFromHq
            | Trigger::OnCardsTrashedFromRnD
            | Trigger::OnFinishedResolving
            | Trigger::OnBadPublicityTaken
            | Trigger::OnIdentityFlipped
            | Trigger::OnCardTrashed
            | Trigger::OnActionTaken
            | Trigger::OnActionFinished
            | Trigger::OnDamageSuffered
            | Trigger::OnAbilityUsed
            | Trigger::OnAbilityTookCredits
            | Trigger::OnCardRevealed
            | Trigger::OnDrawAboutToResolve
            | Trigger::OnServerCreated
            | Trigger::OnCreditsSpentFromInstalledCard => Hears::OwnSide,
            // `OnPlay` and `OnForfeit` are only ever printed about the card
            // itself, so `Subject::This` already says whose they are.
            Trigger::OnPlay
            | Trigger::OnForfeit
            | Trigger::OnWouldBeUninstalled
            // Printed only about the card itself, as "this upgrade".
            | Trigger::OnCardMoved
            | Trigger::OnAccessed
            | Trigger::OnTrashedFromAccess
            | Trigger::OnAgendaScored
            | Trigger::OnAgendaStolen
            | Trigger::OnRez
            | Trigger::OnDerez
            | Trigger::OnEncounter
            // Only the Runner passes, breaks and bypasses ice, so a
            // Runner card's "you pass" and a Corp card's "the Runner
            // passes this ice" hear the one moment.
            | Trigger::OnIcePassed
            | Trigger::OnSubroutineBroken
            | Trigger::OnSubroutineResolved
            | Trigger::OnIceFullyBroken
            | Trigger::OnIceBypassed
            | Trigger::OnEncounterEnded
            | Trigger::OnRunStart
            | Trigger::OnIceApproached
            | Trigger::OnApproachServer
            | Trigger::OnSuccessfulRun
            | Trigger::OnBreach
            | Trigger::OnRunEnded
            | Trigger::OnCreditsSpentOutsidePool
            | Trigger::OnArchivesTurnedFaceup
            | Trigger::OnTagsGiven
            | Trigger::OnTagRemoved
            | Trigger::OnDamageAboutToResolve
            | Trigger::OnVirusCountersPurged
            | Trigger::OnCardsTrashedFromGripOrStack
            | Trigger::WhileTrue
            | Trigger::Paid => Hears::Everyone,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rezzed_bioroid` and `at_most_zero_strength` share a column bit,
    /// which is sound only while no trigger's moment states both: the log
    /// keeps a row per trigger, so within a row the bit means one fact.
    #[test]
    fn no_trigger_states_both_facts_that_share_a_column() {
        for trigger in Trigger::ALL {
            let stated = IceFacts::stated_by(trigger);
            assert!(!(stated.rezzed_bioroid && stated.at_most_zero_strength), "{trigger:?} states both");
        }
        assert!(IceFacts::ALL.iter().all(|facts| facts.bits() < IceFacts::COLUMNS));
    }

    #[test]
    fn every_trigger_is_listed_at_its_own_index() {
        for (position, trigger) in Trigger::ALL.iter().enumerate() {
            assert_eq!(trigger.index(), position, "{trigger:?}");
        }
        // Exhaustive, so a new variant stops here until it is added to
        // `Trigger::ALL` — the turn log indexes a fixed array by it.
        let listed = |trigger: Trigger| match trigger {
            Trigger::OnPlay | Trigger::OnRunStart | Trigger::OnEncounter | Trigger::OnTurnStart | Trigger::OnAccessed | Trigger::OnTrashedFromAccess | Trigger::OnSuccessfulRun | Trigger::Paid | Trigger::OnInstall | Trigger::OnAgendaScored | Trigger::OnAgendaStolen | Trigger::OnDamageAboutToResolve | Trigger::OnRez | Trigger::OnApproachServer | Trigger::OnRunEnded | Trigger::OnBasicDrawAction | Trigger::OnTagsGiven | Trigger::OnAdvance | Trigger::OnDiscardPhaseEnd | Trigger::OnActionPhaseEnd | Trigger::OnCardInstalled | Trigger::OnDamageDealt | Trigger::OnCardsTrashedFromHq | Trigger::OnAbilityGainedCredits | Trigger::OnForfeit | Trigger::OnIceApproached | Trigger::OnCardPlayed | Trigger::OnTagRemoved | Trigger::OnBadPublicityTaken | Trigger::OnIcePassed | Trigger::OnSubroutineBroken | Trigger::OnIceFullyBroken | Trigger::OnIceBypassed | Trigger::OnEncounterEnded | Trigger::OnCreditsSpentOutsidePool | Trigger::OnArchivesTurnedFaceup | Trigger::OnCardTrashed | Trigger::OnWouldBeUninstalled | Trigger::OnIdentityFlipped | Trigger::OnActionTaken | Trigger::OnVirusCountersPurged | Trigger::OnCardMoved | Trigger::OnFinishedResolving | Trigger::OnCardsTrashedFromRnD | Trigger::OnDerez | Trigger::OnBreach | Trigger::OnActionFinished | Trigger::OnSubroutineResolved | Trigger::OnDamageSuffered | Trigger::OnCreditsSpentFromInstalledCard | Trigger::OnCardsTrashedFromGripOrStack | Trigger::OnAbilityUsed | Trigger::OnAbilityTookCredits | Trigger::OnCardRevealed | Trigger::OnDrawAboutToResolve | Trigger::OnServerCreated | Trigger::WhileTrue => Trigger::ALL.contains(&trigger),
        };
        assert!(Trigger::ALL.iter().all(|trigger| listed(*trigger)));
    }
}
