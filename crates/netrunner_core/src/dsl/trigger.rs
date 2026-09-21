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
    /// "Whenever you play an operation" — fired against the Corp's identity
    /// for every `GameEvent::OperationPlayed`, whatever the operation's
    /// subtype: Nebula Talent Management: Making Stars reads every
    /// operation. Weyland Consortium: Building a Better World reads the
    /// transactions, with `when: Card(HasSubtype(Transaction))` — it had a
    /// variant of its own, `OnTransactionPlayed`, until a trigger could take
    /// a filter.
    OnOperationPlayed,
    /// "Whenever a tag is removed" — fired against the Corp's identity
    /// when the Runner loses a tag by any route (`TagRemoved`,
    /// `TagsRemoved`, `TagsCleared`). Synapse Global: Faster than Thought
    /// installs off it, including off its own remove-a-tag ability.
    OnTagRemoved,
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
}

impl Trigger {
    /// Every trigger, in declaration order — the rows of
    /// `rules::turn_log::TurnLog`, which counts a turn's moments in a flat
    /// array and so needs a trigger to be a number. `index` is that number;
    /// `every_trigger_is_listed_at_its_own_index` holds the two together,
    /// and its exhaustive `match` is what stops a new variant compiling
    /// until it is listed here.
    pub const ALL: [Trigger; 28] = [
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
        Trigger::OnOperationPlayed,
        Trigger::OnTagRemoved,
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
            | Trigger::OnOperationPlayed
            | Trigger::OnAccessed
            | Trigger::OnTrashedFromAccess
            | Trigger::OnAgendaScored
            | Trigger::OnAgendaStolen
            | Trigger::OnForfeit
            | Trigger::OnRez
            | Trigger::OnAdvance
            | Trigger::OnEncounter
            | Trigger::OnAbilityGainedCredits => TriggerAbout::Card,
            Trigger::OnRunStart | Trigger::OnIceApproached | Trigger::OnApproachServer | Trigger::OnSuccessfulRun | Trigger::OnRunEnded => {
                TriggerAbout::Server
            }
            // A phase, a count or a player, never a card.
            Trigger::OnTurnStart
            | Trigger::OnActionPhaseEnd
            | Trigger::OnDiscardPhaseEnd
            | Trigger::OnBasicDrawAction
            | Trigger::OnTagsGiven
            | Trigger::OnTagRemoved
            | Trigger::OnDamageDealt
            | Trigger::OnCardsTrashedFromHq
            | Trigger::OnDamageAboutToResolve
            | Trigger::Paid => TriggerAbout::Nothing,
        }
    }

    /// Whether a `TriggeredEffect` using this trigger must say which
    /// occurrences it hears (`TriggeredEffect::subject`) — and one using any
    /// other trigger must not, because there is no "this one" for a turn
    /// beginning.
    pub fn names_a_subject(self) -> bool {
        self.about() != TriggerAbout::Nothing
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
            | Trigger::OnOperationPlayed
            | Trigger::OnAdvance
            | Trigger::OnAbilityGainedCredits
            | Trigger::OnDamageDealt
            | Trigger::OnCardsTrashedFromHq => Hears::OwnSide,
            // `OnPlay` and `OnForfeit` are only ever printed about the card
            // itself, so `Subject::This` already says whose they are.
            Trigger::OnPlay
            | Trigger::OnForfeit
            | Trigger::OnAccessed
            | Trigger::OnTrashedFromAccess
            | Trigger::OnAgendaScored
            | Trigger::OnAgendaStolen
            | Trigger::OnRez
            | Trigger::OnEncounter
            | Trigger::OnRunStart
            | Trigger::OnIceApproached
            | Trigger::OnApproachServer
            | Trigger::OnSuccessfulRun
            | Trigger::OnRunEnded
            | Trigger::OnTagsGiven
            | Trigger::OnTagRemoved
            | Trigger::OnDamageAboutToResolve
            | Trigger::Paid => Hears::Everyone,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_trigger_is_listed_at_its_own_index() {
        for (position, trigger) in Trigger::ALL.iter().enumerate() {
            assert_eq!(trigger.index(), position, "{trigger:?}");
        }
        // Exhaustive, so a new variant stops here until it is added to
        // `Trigger::ALL` — the turn log indexes a fixed array by it.
        let listed = |trigger: Trigger| match trigger {
            Trigger::OnPlay | Trigger::OnRunStart | Trigger::OnEncounter | Trigger::OnTurnStart | Trigger::OnAccessed | Trigger::OnTrashedFromAccess | Trigger::OnSuccessfulRun | Trigger::Paid | Trigger::OnInstall | Trigger::OnAgendaScored | Trigger::OnAgendaStolen | Trigger::OnDamageAboutToResolve | Trigger::OnRez | Trigger::OnApproachServer | Trigger::OnRunEnded | Trigger::OnBasicDrawAction | Trigger::OnTagsGiven | Trigger::OnAdvance | Trigger::OnDiscardPhaseEnd | Trigger::OnActionPhaseEnd | Trigger::OnCardInstalled | Trigger::OnDamageDealt | Trigger::OnCardsTrashedFromHq | Trigger::OnAbilityGainedCredits | Trigger::OnForfeit | Trigger::OnIceApproached | Trigger::OnOperationPlayed | Trigger::OnTagRemoved => Trigger::ALL.contains(&trigger),
        };
        assert!(Trigger::ALL.iter().all(|trigger| listed(*trigger)));
    }
}
