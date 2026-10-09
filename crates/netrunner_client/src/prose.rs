//! Words for the engine's DSL, for the one reader who cannot run it: the
//! person at the keyboard.
//!
//! A `PresentChoice` hands the client a list of `Effect`s and asks for an
//! index, and the client used to label them `Option 0 | Option 1` — a
//! menu with no words on it, for a decision the card text explains. This
//! module is the missing sentence: `describe_effect` says what an option
//! does, `describe_cost` what it costs, and `decision_prompt` names the
//! card asking and what it asks, for the actions pane's title.
//!
//! **The printed card is the authority; this is the engine's reading of
//! it, and the fallback.** Where a card author has linked a DSL node to
//! the printed clause it implements (`PresentChoice::texts`,
//! `OfferPaidChoice::text`, `AbilityDef::text`, `SubroutineDef::text`),
//! the client shows the card's own words and never this module's. This
//! module is used in two places only: as the label for a card that has
//! no clause linked (homebrew, a test fixture), and in the card
//! inspector's "Engine reads it as" section, where the DSL's rendering
//! sits beside the printed clause so a person can see whether the two
//! agree — which is how a card whose implementation has drifted from its
//! text, by a bug or by an erratum, gets noticed at the table.
//!
//! **Prose, not a second rules engine.** Every function here reads the
//! DSL and produces a string; none decides anything. Where a variant's
//! meaning depends on state the client cannot see, the string says what
//! the card *would* do ("gain credits per counter"), not what will
//! happen. A variant with no sentence yet falls back to its debug form,
//! which is still a word rather than a number.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{DeckEnd, Discount, 
    Amount, EffectDuration, CardDefinition, CardFilter, CardId, CardTarget, CardZoneRef, ContinuousEffect, ContinuousKind, Cost, DamageType, Effect, EventFilter, Number, PaysFor, Preventable, Prohibition,
    Scope, SubroutineBreakCount, TrashedFrom,
};
use netrunner_core::rules::{PendingDecision, Pool, ServerId, Side};
use netrunner_core::view::ClientView;

fn title(id: &CardId, registry: &CardRegistry) -> String {
    registry.get(id).map_or_else(|| id.0.clone(), |card| card.title.clone())
}

fn plural(n: impl Into<u64>, one: &str, many: &str) -> String {
    let n = n.into();
    format!("{n} {}", if n == 1 { one } else { many })
}

fn who(side: Side) -> &'static str {
    match side {
        Side::Corp => "the Corp",
        Side::Runner => "the Runner",
    }
}

pub fn describe_server(server: ServerId) -> String {
    match server {
        ServerId::Hq => "HQ".to_string(),
        ServerId::RnD => "R&D".to_string(),
        ServerId::Archives => "Archives".to_string(),
        ServerId::Remote(n) => format!("remote {n}"),
    }
}

pub fn describe_amount(amount: &Amount) -> String {
    match amount {
        Amount::Fixed(n) => n.to_string(),
        Amount::AgendaPointsScoredThisTurn => "the agenda points scored this turn".to_string(),
        Amount::AgendaPointsStolenLastTurn => "the printed agenda points the Runner stole last turn".to_string(),
        Amount::TimesThisTurn(trigger) => format!("the times \"{}\" has happened this turn", humanize(format!("{trigger:?}"))),
        Amount::TimesThisTurnWhen { trigger, when } => {
            format!("the times \"{}\" has happened this turn ({})", humanize(format!("{trigger:?}")), humanize(format!("{when:?}")).to_lowercase())
        }
        Amount::TimesLastTurn(trigger) => format!("the times \"{}\" happened last turn", humanize(format!("{trigger:?}"))),
        Amount::TimesLastTurnWhen { trigger, when } => {
            format!("the times \"{}\" happened last turn ({})", humanize(format!("{trigger:?}")), humanize(format!("{when:?}")).to_lowercase())
        }
        Amount::HostedCounters => "the counters on this card".to_string(),
        Amount::HostedCards => "the cards hosted on this card".to_string(),
        Amount::HostedAdvancementTokens => "the advancement tokens on this card".to_string(),
        Amount::InstalledIcebreakerCount => "the number of installed icebreakers".to_string(),
        Amount::FacedownCardsInArchives => "the number of facedown cards in Archives".to_string(),
        Amount::RevealedThisEncounterSharingAType => "the most cards revealed this encounter that share a type".to_string(),
        Amount::CardTypesAmongFaceupInArchives => "the number of card types among faceup cards in Archives".to_string(),
        Amount::CardsInstalledFromHqThisTurn => "the cards installed from HQ this turn".to_string(),
        Amount::CardsInstalledInRemotesThisTurn => "the cards installed in remote servers this turn".to_string(),
        Amount::ClickGainsInRunsThisTurn => "the times you gained [click] during a run this turn".to_string(),
        Amount::CorpCardsAddedToArchivesThisTurn => "the Corp cards added to Archives this turn".to_string(),
        Amount::TimesThisActionThisTurn => "the times you have taken that action this turn".to_string(),
        Amount::CreditsLostThisResolution => "the credits just lost".to_string(),
        Amount::ClicksRemaining => "the clicks remaining".to_string(),
        Amount::PrintedCost => "its printed cost".to_string(),
        Amount::AccessedCardPrintedCost => "the printed cost of the card being accessed".to_string(),
        Amount::PaidCardPrintedCost => "the printed cost of the card trashed to pay".to_string(),
        Amount::TriggeringCardPrintedCost => "the printed cost of the trashed card".to_string(),
        Amount::ProtectedRemotesWithRootCards => "the number of remote servers with a card in the root and protected by ice".to_string(),
        Amount::RemainingAfterSelection(n) => format!("{n} less the cards chosen"),
        Amount::CardsSelected => "the cards chosen".to_string(),
        Amount::RunCreditsLeftLastRun => "the credits left on it from that run".to_string(),
        Amount::AccessLimit(server) => format!("the cards you may access in {}", describe_server(*server)),
        Amount::EncountersThisRun => "the times you have encountered ice this run".to_string(),
        Amount::IcePassedThisRun => "the times you have passed ice this run".to_string(),
        Amount::IcePassedLastRun => "the times you passed ice during that run".to_string(),
        Amount::ThreatLevel => "the threat level".to_string(),
        Amount::RunnerTags => "the Runner's tags".to_string(),
        Amount::BadPublicity => "the Corp's bad publicity".to_string(),
        Amount::CorpInstalls(filter) => format!("the Corp's installed cards ({})", humanize(format!("{filter:?}")).to_lowercase()),
        Amount::RunnerInstalls(filter) => format!("the Runner's installed cards ({})", humanize(format!("{filter:?}")).to_lowercase()),
        Amount::Reduced { amount, by } => format!("{} less {}, at least 0", describe_amount(amount), describe_amount(by)),
        Amount::Increased { amount, by } => format!("{} plus {}", describe_amount(amount), describe_amount(by)),
        Amount::Times { amount, times } => format!("{times} for each of {}", describe_amount(amount)),
        Amount::Every { amount, every } => format!("1 for every {every} of {}", describe_amount(amount)),
        Amount::AccessedCardAdvancementCounters => "the advancement counters on the card being accessed".to_string(),
        Amount::CoreDamageTaken => "the core damage the Runner has taken this game".to_string(),
        Amount::ChosenNumber => "the number chosen".to_string(),
        Amount::InHeapWithSubtype(subtype) => format!("the number of {} cards in the heap", subtype.printed().to_lowercase()),
        Amount::InScoreAreaWithSubtype(subtype) => format!("the number of {} agendas in your score area", subtype.printed().to_lowercase()),
        Amount::IceProtectingThisServer => "the number of pieces of ice protecting this server".to_string(),
        Amount::IceProtecting(server) => format!("the number of pieces of ice protecting {}", describe_server(*server)),
        Amount::OtherUnrezzedIce => "the number of other unrezzed pieces of ice".to_string(),
        Amount::Link => "the Runner's link".to_string(),
        Amount::CardsAccessedLastRun => "the cards accessed during that run".to_string(),
        Amount::EncounteredIceStrength => "the strength of the ice being encountered".to_string(),
        Amount::HostIceStrength => "the strength of host ice".to_string(),
        Amount::EncounteredIceSubroutines => "the subroutines on the ice being encountered".to_string(),
        Amount::CardsInHand(Side::Corp) => "the cards in HQ".to_string(),
        Amount::CardsInHand(Side::Runner) => "the cards in the grip".to_string(),
        Amount::InZone { zone, filter } => format!("the cards in {} ({})", describe_zone(zone), humanize(format!("{filter:?}")).to_lowercase()),
        Amount::CopiesInScoreArea(side) => format!("the copies of this card in the {side:?}'s score area"),
        Amount::CountersOnOwnInstalls(kind) => format!("the {} counters on your installed cards", humanize(format!("{kind:?}")).to_lowercase()),
        Amount::Credits(Side::Corp) => "the Corp's credits".to_string(),
        Amount::Credits(Side::Runner) => "the Runner's credits".to_string(),
        Amount::ThisCardStrength => "this program's strength".to_string(),
        Amount::TimesThisTurnOnThisCopy(trigger) => format!("the times \"{}\" has happened to this card this turn", humanize(format!("{trigger:?}"))),
        Amount::AboutToResolve => "the number about to happen".to_string(),
        Amount::AgendaPoints(side) => format!("the {side:?}'s agenda points"),
        Amount::ActionsThisTurn => "the actions taken this turn".to_string(),
        Amount::DifferentActionsThisTurn => "the different actions taken this turn".to_string(),
    }
}

pub fn describe_zone(zone: &CardZoneRef) -> &'static str {
    match zone {
        CardZoneRef::OwnHq => "HQ",
        CardZoneRef::OwnArchives => "Archives",
        CardZoneRef::OwnRAndD => "R&D",
        CardZoneRef::OwnStack => "the stack",
        CardZoneRef::OwnGrip => "the grip",
        CardZoneRef::OwnHeap => "the heap",
        CardZoneRef::OwnSetAside => "the cards set aside",
        CardZoneRef::OpponentSetAside => "the Corp's cards set aside",
        CardZoneRef::OpponentInstalled => "the opponent's installed cards",
        CardZoneRef::OpponentDiscard => "the opponent's discard pile",
        CardZoneRef::OwnInstalled => "your installed cards",
        CardZoneRef::HostedOnSource => "the cards hosted here",
        CardZoneRef::TopOfOwnStack => "the top of the stack",
        CardZoneRef::OpponentHand => "the opponent's hand",
        CardZoneRef::OpponentDeck => "the opponent's deck",
        CardZoneRef::OpponentScoreArea => "the opponent's score area",
        CardZoneRef::OwnScoreArea => "your score area",
        CardZoneRef::OpponentRemovedFromGame => "out of the game",
        CardZoneRef::PlayArea => "the play area",
    }
}

pub fn describe_cost(cost: &Cost) -> String {
    match cost {
        Cost::Credits(n) => plural(*n, "credit", "credits"),
        Cost::CreditsFrom { amount: Amount::Fixed(n), from } => {
            format!("{} from {}", plural(*n, "credit", "credits"), humanize(format!("{from:?}")).to_lowercase())
        }
        Cost::CreditsAmount(amount) => format!("credits equal to {}", describe_amount(amount)),
        Cost::ClicksAmount(amount) => format!("clicks equal to {}", describe_amount(amount)),
        Cost::CreditsX { .. } => "X credits".to_string(),
        Cost::CreditsFrom { amount, from } => format!("credits equal to {}, from {}", describe_amount(amount), humanize(format!("{from:?}")).to_lowercase()),
        Cost::Clicks(n) => plural(*n, "click", "clicks"),
        Cost::LoseClicks(n) => format!("lose {}", plural(*n, "click", "clicks")),
        Cost::LoseAllClicks => "lose every click".to_string(),
        Cost::JackOut => "jack out".to_string(),
        Cost::TrashSelf => "trash this card".to_string(),
        Cost::ClearTags => "remove all tags".to_string(),
        Cost::RemoveTags(n) => format!("remove {}", plural(*n, "tag", "tags")),
        Cost::SufferDamage(kind, n) => format!("suffer {n} {} damage", damage_word(kind)),
        Cost::Forfeit(n) => format!("forfeit {}", plural(*n, "agenda", "agendas")),
        Cost::ForfeitSelf => "forfeit this agenda".to_string(),
        Cost::Derez { count, .. } => format!("derez {}", plural(*count, "card", "cards")),
        Cost::Trash { from, count, .. } => format!("trash {} from {}", plural(*count, "card", "cards"), describe_zone(from)),
        Cost::TakeTags(n) => format!("take {}", plural(*n, "tag", "tags")),
        Cost::TakeBadPublicity(n) => format!("take {n} bad publicity"),
        Cost::RemoveCounters(n) => format!("remove {}", plural(*n, "counter", "counters")),
        Cost::RemoveAdvancementCounters(n) => format!("remove {}", plural(*n, "hosted advancement counter", "hosted advancement counters")),
        Cost::AddToScoreAreaAsAgenda(as_agenda) => format!("add this card to the Corp's score area as {}", as_an_agenda(as_agenda)),
        Cost::RemoveSelfFromGame => "remove this card from the game".to_string(),
        Cost::RevealAndTrashSelf => "reveal and trash this card from your hand".to_string(),
        Cost::RevealSelf => "reveal this card".to_string(),
        Cost::DerezSelf => "derez this card".to_string(),
        Cost::AddInstalledToHand { filter, count } => format!("add {} installed {} to your hand", count, humanize(format!("{filter:?}")).to_lowercase()),
        Cost::AddSelfToHq => "add this card to HQ".to_string(),
        Cost::TrashRandomFromHq(n) => format!("trash {} at random from HQ", plural(*n, "card", "cards")),
        Cost::TurnHostedFacedown => "turn 1 hosted card facedown".to_string(),
        Cost::AddRandomFromGripToBottom(n) => format!("add {} from your grip at random to the bottom of your stack", plural(*n, "card", "cards")),
        Cost::AnyOf(options) => options.iter().map(describe_cost).collect::<Vec<_>>().join(" or "),
        Cost::AllOf(parts) => parts.iter().map(describe_cost).collect::<Vec<_>>().join(" and "),
    }
}

/// "an agenda worth −1 agenda points that cannot be forfeited" — what a
/// card added to a score area is (CR 10.1.3).
/// "an assassination agenda", "a security agenda": the subtype a card was
/// added to a score area as (`AsAgenda::subtype`).
pub fn a_subtype_agenda(subtype: &netrunner_core::dsl::CardSubtype) -> String {
    let word = subtype.printed().to_lowercase();
    let article = if word.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
    format!("{article} {word} agenda")
}

fn as_an_agenda(as_agenda: &netrunner_core::dsl::AsAgenda) -> String {
    let points = if as_agenda.points < 0 { format!("−{}", as_agenda.points.unsigned_abs()) } else { as_agenda.points.to_string() };
    let kind = as_agenda.subtype.as_ref().map_or_else(|| "an agenda".to_string(), a_subtype_agenda);
    let worth = format!("{kind} worth {points} agenda point{}", if as_agenda.points.abs() == 1 { "" } else { "s" });
    if as_agenda.cannot_forfeit { format!("{worth} that cannot be forfeited") } else { worth }
}

fn describe_target(target: &CardTarget, registry: &CardRegistry) -> String {
    match target {
        CardTarget::ThisCard => "this card".to_string(),
        CardTarget::CorpInstalled { card, server } => format!("{} in {}", title(card, registry), describe_server(*server)),
        CardTarget::RunnerRig(card) => title(card, registry),
        CardTarget::Install(_) => "that card".to_string(),
        CardTarget::TopOfStack { side, .. } => format!("the top card of {}'s deck", who(*side)),
        CardTarget::HostIce => "the host ice".to_string(),
        CardTarget::HostedOnThisCard => "the card hosted here".to_string(),
        CardTarget::EncounteredIce => "the ice being encountered".to_string(),
        CardTarget::RandomFromHand(Side::Corp) => "a random card from HQ".to_string(),
        CardTarget::RandomFromHand(Side::Runner) => "a random card from the grip".to_string(),
        CardTarget::AttackedServerRoot => "every card in the root of the attacked server".to_string(),
        CardTarget::SetAside => "every card still set aside".to_string(),
    }
}

/// The rules' word for a kind of damage: core, not the engine's `Brain`.
pub(crate) fn damage_word(kind: &DamageType) -> &'static str {
    match kind {
        DamageType::Net => "net",
        DamageType::Meat => "meat",
        DamageType::Brain => "core",
    }
}

fn duration(d: &EffectDuration) -> &'static str {
    match d {
        EffectDuration::Encounter => "for this encounter",
        EffectDuration::Run => "for this run",
        EffectDuration::Turn => "for this turn",
        EffectDuration::ThroughYourNextTurn => "until your next turn ends",
        EffectDuration::WhileRezzed => "while this card is rezzed",
        EffectDuration::WhileInstalled => "while this card is installed",
        EffectDuration::NextAction => "for the next action",
    }
}

/// One effect as a sentence fragment, lower-case, no full stop, so it
/// reads after a card name and a colon: `Bigger Picture: give the Runner
/// 1 tag`.
pub fn describe_effect(effect: &Effect, registry: &CardRegistry) -> String {
    match effect {
        Effect::Sequence(steps) if steps.is_empty() => "do nothing".to_string(),
        Effect::Sequence(steps) => steps.iter().map(|step| describe_effect(step, registry)).collect::<Vec<_>>().join(", then "),
        Effect::GainCredits(side, n) => format!("{} gains {}", who(*side), plural(*n, "credit", "credits")),
        Effect::GainCreditsAmount(side, amount) => format!("{} gains credits equal to {}", who(*side), describe_amount(amount)),
        Effect::LoseCredits(side, n) => format!("{} loses {}", who(*side), plural(*n, "credit", "credits")),
        Effect::LoseCreditsAmount(side, amount) => format!("{} loses credits equal to {}", who(*side), describe_amount(amount)),
        Effect::DealDamage(kind, n) => format!("do {} {} damage", n, damage_word(kind)),
        Effect::PsiGame { on_match, on_differ } => format!(
            "play a Psi Game: if the bids match, {}; if they differ, {}",
            describe_effect(on_match, registry),
            describe_effect(on_differ, registry)
        ),
        Effect::DealDamageAmount(kind, amount) => format!("do {} damage equal to {}", damage_word(kind), describe_amount(amount)),
        Effect::ModifyStrength { delta, ice, duration } => {
            let which = match ice {
                netrunner_core::dsl::StrengthOf::Encountered => "the encountered ice gets",
                netrunner_core::dsl::StrengthOf::EachIce => "each piece of ice gets",
                netrunner_core::dsl::StrengthOf::This => "this ice gets",
            };
            let sign = if *delta >= 0 { "+" } else { "" };
            let until = match duration {
                EffectDuration::Encounter => "this encounter",
                EffectDuration::Run => "this run",
                EffectDuration::Turn => "this turn",
                EffectDuration::ThroughYourNextTurn => "your next turn",
                EffectDuration::WhileRezzed => "the time this card is rezzed",
                EffectDuration::WhileInstalled => "the time this card is installed",
                EffectDuration::NextAction => "the next action",
            };
            format!("{which} {sign}{delta} strength for the remainder of {until}")
        }
        Effect::DrawCards(side, n) => format!("{} draws {}", who(*side), plural(*n, "card", "cards")),
        Effect::DrawCardsAmount(side, amount) => format!("{} draws cards equal to {}", who(*side), describe_amount(amount)),
        Effect::EndTheRun => "end the run".to_string(),
        Effect::GiveTags(Amount::Fixed(n)) => format!("give the Runner {}", plural(*n, "tag", "tags")),
        Effect::GiveTags(amount) => format!("give the Runner tags equal to {}", describe_amount(amount)),
        Effect::RemoveTags(Amount::Fixed(n)) => format!("remove {}", plural(*n, "tag", "tags")),
        Effect::RemoveTags(amount) => format!("remove tags equal to {}", describe_amount(amount)),
        Effect::GiveBadPublicity(Amount::Fixed(n)) => format!("the Corp takes {}", plural(*n, "bad publicity", "bad publicity")),
        Effect::GiveBadPublicity(amount) => format!("the Corp takes bad publicity equal to {}", describe_amount(amount)),
        Effect::RemoveBadPublicity(n) => format!("remove {}", plural(*n, "bad publicity", "bad publicity")),
        Effect::RemoveFromGame(target) => format!("remove {} from the game", describe_target(target, registry)),
        Effect::TrashCard(target) => format!("trash {}", describe_target(target, registry)),
        Effect::DerezCard(target) => format!("derez {}", describe_target(target, registry)),
        Effect::BoostStrength { amount, duration: d } => format!("+{amount} strength {}", duration(d)),
        Effect::BoostStrengthAmount { amount, duration: d } => format!("strength +{} {}", describe_amount(amount), duration(d)),
        Effect::BreakSubroutines { count, restrict_to } => {
            let which = match count {
                SubroutineBreakCount::Fixed(n) => plural(*n, "subroutine", "subroutines"),
                SubroutineBreakCount::All => "all subroutines".to_string(),
                SubroutineBreakCount::ChosenNumber => "X subroutines".to_string(),
            };
            match restrict_to {
                Some(kind) => format!("break {which} on a {kind:?}"),
                None => format!("break {which}"),
            }
        }
        Effect::BreakSubroutinesUnconditionally { count } => match count {
            SubroutineBreakCount::Fixed(n) => format!("break {}", plural(*n, "subroutine", "subroutines")),
            SubroutineBreakCount::All => "break all subroutines".to_string(),
            SubroutineBreakCount::ChosenNumber => "break X subroutines".to_string(),
        },
        Effect::Trace { base, on_success } => format!("trace {base}: if successful, {}", describe_effect(on_success, registry)),
        Effect::AddAdditionalAccess { server, count } => {
            format!("access {} additional from {}", plural(*count, "card", "cards"), describe_server(*server))
        }
        Effect::AddAdditionalAccessAmount { server, amount } => {
            format!("access additional cards from {} equal to {}", describe_server(*server), describe_amount(amount))
        }
        Effect::SetAccessReplacement { server, effect, .. } => {
            let server = server.map_or_else(|| "the attacked server".to_string(), describe_server);
            format!("instead of accessing {server}, {}", describe_effect(effect, registry))
        }
        Effect::LoseClicks(n) => format!("lose {}", plural(*n, "click", "clicks")),
        Effect::GainClicks(side, n) => format!("{} gains {}", who(*side), plural(*n, "click", "clicks")),
        Effect::AllottedClicksNextTurn(side, n) => {
            format!("{} gets {}{} allotted {} next turn", who(*side), if *n < 0 { "−" } else { "+" }, n.unsigned_abs(), if n.unsigned_abs() == 1 { "click" } else { "clicks" })
        }
        Effect::InitiateRun(server) => format!("run {}", describe_server(*server)),
        Effect::Prevent(Preventable::Damage { kind, up_to }) => match kind {
            Some(kind) => format!("prevent up to {up_to} {} damage", format!("{kind:?}").to_lowercase()),
            None => format!("prevent up to {up_to} damage"),
        },
        Effect::Prevent(Preventable::Tags(n)) => format!("prevent {n} tag{}", if *n == 1 { "" } else { "s" }),
        Effect::Prevent(Preventable::EncounterAbility) => "prevent a \"when encountered\" ability on a piece of ice".to_string(),
        Effect::Prevent(Preventable::RunEnding) => "prevent a Corp card ability from ending the run".to_string(),
        Effect::Prevent(Preventable::Expose) => "prevent 1 card from being exposed".to_string(),
        Effect::Prevent(Preventable::TraceBaseStrength) => "reduce the base trace strength of a trace to 0".to_string(),
        Effect::Prevent(Preventable::Trash(filter)) => format!("prevent 1 installed card from being trashed ({})", humanize(format!("{filter:?}")).to_lowercase()),
        Effect::IncreaseAboutToResolve { by, then } => {
            let grow = format!("draw {} more", describe_amount(by));
            match then {
                Some(then) => format!("{grow}; once drawn: {}", describe_effect(then, registry)),
                None => grow,
            }
        }
        Effect::AddCounters(n) => format!("place {}", plural(*n, "counter", "counters")),
        Effect::RemoveCounters(Amount::Fixed(n)) => format!("remove {}", plural(*n, "counter", "counters")),
        Effect::RemoveCounters(Amount::HostedCounters) => "remove all its counters".to_string(),
        Effect::RemoveCounters(amount) => format!("remove counters equal to {}", describe_amount(amount)),
        Effect::EffectIf { condition, effect, otherwise: None } => format!("if {}: {}", humanize(format!("{condition:?}")), describe_effect(effect, registry)),
        Effect::EffectIf { condition, effect, otherwise: Some(otherwise) } => format!(
            "if {}: {}; otherwise {}",
            humanize(format!("{condition:?}")),
            describe_effect(effect, registry),
            describe_effect(otherwise, registry)
        ),
        Effect::OfferPaidChoice { side, cost, if_paid, if_declined, .. } => format!(
            "{} may pay {} to {}; otherwise {}",
            who(*side),
            describe_cost(cost),
            describe_effect(if_paid, registry),
            describe_effect(if_declined, registry)
        ),
        Effect::PresentChoice { chooser, options, .. } => format!(
            "{} chooses: {}",
            who(*chooser),
            options.iter().map(|option| describe_effect(option, registry)).collect::<Vec<_>>().join(" / ")
        ),
        Effect::ChooseNumber { chooser, min, max, of, then, secret, .. } => format!(
            "{} {}chooses a number from {min} to {}{}, then: {}",
            who(*chooser),
            if *secret { "secretly " } else { "" },
            describe_amount(max),
            of.as_ref().map(|of| format!(" (at most {})", describe_amount(of))).unwrap_or_default(),
            describe_effect(then, registry)
        ),
        Effect::Repeat { times, effect } => format!("{}, as many times as {}", describe_effect(effect, registry), describe_amount(times)),
        Effect::ForEach { filter, effect, .. } => {
            format!("for each installed card ({}): {}", humanize(format!("{filter:?}")).to_lowercase(), describe_effect(effect, registry))
        }
        Effect::ResolveSomeOf { chooser, count, options, .. } => format!(
            "{} chooses {} of: {}",
            who(*chooser),
            count,
            options.iter().map(|option| describe_effect(option, registry)).collect::<Vec<_>>().join(" / ")
        ),
        Effect::PromptChooseCards { side, source, min, max, count, up_to, .. } => {
            let how_many = match (count, up_to) {
                (Some(count), _) => format!("as many cards as {}", describe_amount(count)),
                (None, Some(up_to)) => format!("up to as many cards as {}", describe_amount(up_to)),
                (None, None) if min == max => plural(*min, "card", "cards"),
                (None, None) => format!("{min} to {max} cards"),
            };
            format!("{} chooses {how_many} from {}", who(*side), describe_zone(source))
        }
        Effect::PromptChooseServer { .. } => "choose a server".to_string(),
        Effect::RezInstalled { .. } => "rez a card".to_string(),
        Effect::TakeAllCountersAsCredits(side) => format!("{} takes the counters as credits", who(*side)),
        Effect::TrashCurrentlyAccessedCard => "trash the accessed card".to_string(),
        Effect::GainCreditsPerCounter { side, credits_per_counter } => {
            format!("{} gains {} per counter", who(*side), plural(*credits_per_counter, "credit", "credits"))
        }
        Effect::SwapInstalledIce(..) => "swap two pieces of ice".to_string(),
        Effect::InstallFromZoneIgnoringCost { .. } => "install a card, ignoring its cost".to_string(),
        Effect::PromptInstallCorpCard { new_remote: true, .. } => "install a card in a new remote server".to_string(),
        Effect::PromptInstallCorpCard { not_in_root_of: Some(_), .. } => "install a card, not in the root of this server".to_string(),
        Effect::PromptInstallCorpCard { .. } => "install a card".to_string(),
        Effect::InstallRunnerCardFromGrip => "install a card from the grip".to_string(),
        Effect::InstallRunnerCardFromZone { from, discount: Discount::Credits(0) } => format!("install a card from {}", describe_zone(from)),
        Effect::InstallRunnerCardFromZone { from, discount: Discount::Credits(n) } => format!("install a card from {}, paying {n} less", describe_zone(from)),
        Effect::InstallRunnerCardFromZone { from, discount: Discount::AllCosts } => format!("install a card from {}, ignoring all costs", describe_zone(from)),
        Effect::InstallRunnerCardFromZone { from, discount: Discount::Amount(amount) } => {
            format!("install a card from {}, paying 1 less for each of {}", describe_zone(from), describe_amount(amount))
        }
        Effect::InstallRunnerCardFromZone { from, discount: Discount::Surcharge(n) } => format!("install a card from {}, paying {n} more", describe_zone(from)),
        Effect::SetAsideFromTopUntil { filter: CardFilter::Any, count, deck: Side::Corp } => format!("the Corp sets aside the top {count} cards of R&D faceup"),
        Effect::SetAsideFromTopUntil { filter, count, deck } => {
            let from = if *deck == Side::Corp { "R&D" } else { "the stack" };
            format!("set aside cards from the top of {from} until {count} ({}) are set aside", humanize(format!("{filter:?}")).to_lowercase())
        }
        Effect::InstallRunnerCardFromGripWithDiscount(Discount::Credits(n)) => format!("install a card from the grip, paying {n} less"),
        Effect::InstallRunnerCardFromGripWithDiscount(Discount::AllCosts) => "install a card from the grip, ignoring all costs".to_string(),
        Effect::InstallRunnerCardFromGripWithDiscount(Discount::Amount(amount)) => {
            format!("install a card from the grip, paying 1 less for each of {}", describe_amount(amount))
        }
        Effect::InstallRunnerCardFromGripWithDiscount(Discount::Surcharge(n)) => format!("install a card from the grip, paying {n} more"),
        Effect::RedirectRunOnApproach(server) => format!("redirect the run to {}", describe_server(*server)),
        Effect::RedirectRunOnSuccess(server) => format!("if the run would be declared successful, redirect it to {}", describe_server(*server)),
        Effect::SetRunEndedEffect(effect) => format!("when the run ends, {}", describe_effect(effect, registry)),
        Effect::Unpreventable(effect) => format!("{}, which cannot be prevented", describe_effect(effect, registry)),
        Effect::LaterThisTurn { when, filter, every_time, effect, this_run: false } => describe_later_this_turn(*when, filter.as_ref(), *every_time, effect, registry),
        Effect::LaterThisTurn { when, filter, every_time, effect, this_run: true } => {
            describe_later_this_turn(*when, filter.as_ref(), *every_time, effect, registry).replace("this turn", "this run")
        }
        Effect::ChooseCardName { then, again_if, .. } => {
            let first = format!("choose a card name, then {}", describe_effect(then, registry));
            if again_if.is_some() { format!("{first}; if that trashed a card with the chosen name, do it again") } else { first }
        }
        Effect::StealAccessedCard => "steal it, ignoring all costs".to_string(),
        Effect::EndActionPhase => "your action phase ends".to_string(),
        Effect::Score => "score that card, if able".to_string(),
        Effect::Breach(server) => format!("breach {}", describe_server(*server)),
        Effect::Access { from, filter, count, then } => {
            let which = match filter {
                CardFilter::Any => String::new(),
                filter => format!(" ({})", humanize(format!("{filter:?}")).to_lowercase()),
            };
            let access = format!("access {count} of {}{which}, not as a breach", describe_zone(from));
            match then {
                Some(then) => format!("{access}, then {}", describe_effect(then, registry)),
                None => access,
            }
        }
        Effect::TopOfDeck { deck, count, reveal, each } => {
            let pile = if *deck == Side::Corp { "R&D" } else { "the stack" };
            let take = if *reveal { format!("reveal the top {count} card(s) of {pile}") } else { format!("take the top {count} card(s) of {pile}") };
            match each {
                Some(each) => format!("{take}, and for each: {}", describe_effect(each, registry)),
                None => take,
            }
        }
        Effect::RevealAtRandom { side, count, each } => {
            let hand = if *side == Side::Corp { "HQ" } else { "the grip" };
            match each {
                Some(each) => format!("reveal {count} card(s) in {hand} at random, and for each: {}", describe_effect(each, registry)),
                None => format!("reveal {count} card(s) in {hand} at random"),
            }
        }
        Effect::ArmRunEndPrevention(_) => "the run cannot be ended by the next end-the-run effect".to_string(),
        Effect::Sabotage(n) => format!("sabotage {n}"),
        Effect::Mill { deck, amount, then } => {
            let pile = match deck {
                Side::Corp => "R&D",
                Side::Runner => "the stack",
            };
            let trash = format!("trash cards from the top of {pile} equal to {}", describe_amount(amount));
            match then {
                Some(then) => format!("{trash}, then, of those cards: {}", describe_effect(then, registry)),
                None => trash,
            }
        }
        Effect::HostCardOnThisCard(netrunner_core::dsl::HostedCardOrigin::AccessedCard) => "host the card being accessed on this card".to_string(),
        Effect::HostCardOnThisCard(_) => "host a card on this card".to_string(),
        Effect::BypassEncounteredIce => "bypass this ice".to_string(),
        Effect::PurgeVirusCounters => "purge virus counters".to_string(),
        Effect::TurnFaceupInArchives => "turn it faceup in Archives".to_string(),
        Effect::TurnArchivesFacedown => "turn every card in Archives facedown".to_string(),
        Effect::FlipIdentity => "flip the identity".to_string(),
        Effect::IdentifyMark => "identify your mark".to_string(),
        Effect::SetIdentityCopy(copy) => format!("make copy {} of the identity the one in play", describe_amount(copy)),
        Effect::AddToDeck(DeckEnd::Bottom) => "put it on the bottom of its owner's deck".to_string(),
        Effect::AddToDeck(DeckEnd::Top) => "put it on top of its owner's deck".to_string(),
        Effect::AddToHand => "add it to its owner's grip".to_string(),
        Effect::InstallProgramOnHost { from, .. } => {
            format!("install that program from {} on this card, or a trojan on a piece of ice", describe_zone(from))
        }
        Effect::PlaceRunCredits { amount, pays_for: None } => format!("place {} [credit] on this card, to spend during the run", describe_amount(amount)),
        Effect::PlaceRunCredits { amount, pays_for: Some(word) } => {
            format!("place {} [credit] on this card, to spend {} for the rest of the run", describe_amount(amount), describe_pays_for(word))
        }
        Effect::ShuffleIntoDeck(zones) => {
            let zones: Vec<&str> = zones.iter().map(|zone| if *zone == CardZoneRef::HostedOnSource { "all hosted cards" } else { describe_zone(zone) }).collect();
            format!("shuffle {} into your stack", zones.join(" and "))
        }
        Effect::AddToScoreAreaAsAgenda(as_agenda) => format!("add this card to your score area as {}", as_an_agenda(as_agenda)),
        Effect::GainSubroutine { subroutine, after, duration, count } => {
            let order = if *after { "after" } else { "before" };
            let copies = match count {
                Some(amount) => format!("{} copies of ", describe_amount(amount)),
                None => String::new(),
            };
            let how_long = match duration {
                netrunner_core::dsl::EffectDuration::Encounter => "the rest of the encounter",
                netrunner_core::dsl::EffectDuration::Run => "the rest of the run",
                netrunner_core::dsl::EffectDuration::Turn => "the rest of the turn",
                netrunner_core::dsl::EffectDuration::ThroughYourNextTurn => "the rest of your next turn",
                netrunner_core::dsl::EffectDuration::WhileRezzed => "as long as this card is rezzed",
                netrunner_core::dsl::EffectDuration::WhileInstalled => "as long as this card is installed",
                netrunner_core::dsl::EffectDuration::NextAction => "the next action",
            };
            format!("the ice gains {copies}\u{201c}{}\u{201d} {order} its other subroutines, for {how_long}", subroutine.text.trim_end_matches('.'))
        }
        Effect::WinTheGame => "you win the game".to_string(),
        Effect::TurnHostedFaceup => "turn each hosted card faceup".to_string(),
        Effect::GainIceSubtype { subtype, for_the_turn: true, .. } => {
            format!("that ice gains {} until the end of the turn", crate::board::facts::ice_type_words(&[*subtype]))
        }
        Effect::GainIceSubtype { subtype, ice, for_the_run, .. } => match ice {
            netrunner_core::dsl::StrengthOf::Encountered => format!(
                "the ice you are encountering gains {} for the remainder of this {}",
                crate::board::facts::ice_type_words(&[*subtype]),
                if *for_the_run { "run" } else { "encounter" }
            ),
            netrunner_core::dsl::StrengthOf::EachIce => format!("each piece of ice gains {}", crate::board::facts::ice_type_words(&[*subtype])),
            netrunner_core::dsl::StrengthOf::This => format!("this ice gains {} while it remains rezzed", crate::board::facts::ice_type_words(&[*subtype])),
        },
        Effect::LookAtTopOfDeck { deck, count } => {
            format!("look at the top {} of {}", plural(*count, "card", "cards"), if *deck == Side::Corp { "R&D" } else { "the stack" })
        }
        Effect::HostRigCardOnInstall { .. } => "host it on an installed card".to_string(),
        Effect::Prohibit { what, until, copies_of_it, this_install, encountered_ice } => {
            let what = match (what, copies_of_it) {
                (Prohibition::EndTheRun, _) if *encountered_ice => "subroutines on the ice being encountered cannot end the run",
                (Prohibition::ScoreAgendas, _) if *this_install => "the Corp cannot score that card",
                (Prohibition::StealOrTrash, _) if *this_install => "the Runner cannot steal or trash that card",
                (Prohibition::StealOrTrash, false) => "the Runner cannot steal or trash cards",
                (Prohibition::StealOrTrash, true) => "the Runner cannot steal or trash copies of that card",
                (Prohibition::ScoreAgendas, false) => "the Corp cannot score agendas",
                (Prohibition::ScoreAgendas, true) => "the Corp cannot score copies of that agenda",
                (Prohibition::SpendOrLoseCreditPool, _) => "the Runner cannot lose or spend credits from their credit pool",
                (Prohibition::SpendCredits, _) => "the Runner cannot spend credits",
                (Prohibition::EndTheRun, _) => "subroutines cannot end the run",
                (Prohibition::RunOnRemote, _) => "the Runner cannot run on a remote server",
                (Prohibition::Run, _) => "the Runner cannot make another run",
                (Prohibition::AccessOthers, _) => "the Runner cannot access cards other than this card",
                (Prohibition::Access, _) => "the Runner cannot access this card",
                (Prohibition::BreakSubroutines, _) if *this_install => "that card's abilities cannot break subroutines",
                (Prohibition::BreakSubroutines, _) => "the Runner's abilities cannot break subroutines",
                (Prohibition::DiscardStep, _) => "the Corp skips their discard step",
                (Prohibition::BioroidIceAbilities, _) => "the Runner cannot use paid abilities printed on bioroid ice",
                (Prohibition::StealOrTrashAgendas, _) => "the Runner cannot steal or trash agendas",
                (Prohibition::Rez, _) => "the Corp cannot rez that card",
                (Prohibition::BreakSubroutinesOnIce, _) => "Runner card abilities cannot break subroutines on that ice",
                (Prohibition::DeclaredSuccessful, _) => "this run cannot be declared successful",
                (Prohibition::BreakWithNonIcebreakers, _) => "the Runner cannot use non-icebreaker cards to break subroutines",
                (Prohibition::RepeatAnAction, _) => "the next action must be one not taken this turn",
            };
            format!("{what} {}", duration(until))
        }
        Effect::PlaceAdvancementCounters(Amount::Fixed(n)) => format!("place {}", plural(*n, "advancement token", "advancement tokens")),
        Effect::PlaceAdvancementCounters(amount) => format!("place advancement tokens equal to {}", describe_amount(amount)),
        Effect::RemoveAdvancementCounters(Amount::Fixed(n)) => format!("remove {}", plural(*n, "advancement token", "advancement tokens")),
        Effect::RemoveAdvancementCounters(amount) => format!("remove advancement tokens equal to {}", describe_amount(amount)),
        Effect::MoveThisCardToRoot(server) => format!("move this card to {}", describe_server(*server)),
        Effect::PromptMoveThisCardToAnotherRoot => "move it to the root of another server".to_string(),
        Effect::ForceEncounter => "the Runner encounters that ice again".to_string(),
        Effect::MoveThisIceToOutermost => "move this ice to the outermost position protecting the attacked server".to_string(),
        Effect::PlayOperation { .. } => "play an operation".to_string(),
        Effect::ResolveSubroutineOfSelectedIce => "resolve a subroutine of the chosen ice".to_string(),
        Effect::LoseAbilities { until, identities: true, .. } => format!("each player's identity loses all abilities {}", duration(until)),
        Effect::LoseAbilities { until, attacked_root: false, .. } => format!("it loses all abilities {}", duration(until)),
        Effect::LoseAbilities { until, attacked_root: true, .. } => format!("cards in the root of the attacked server lose all abilities {}", duration(until)),
        Effect::LimitBreaks { at_most: 0, until } => format!("the Runner cannot break this ice's printed subroutines {}", duration(until)),
        Effect::LimitBreaks { at_most, until } => {
            format!("during each encounter with this ice, the Runner cannot break more than {at_most} of its printed subroutines, {}", duration(until))
        }
        Effect::ChooseServer { only_protected_by_ice: false } => "choose a server".to_string(),
        Effect::ChooseServer { only_protected_by_ice: true } => "choose a server protected by ice".to_string(),
        Effect::RevealHand(Side::Corp) => "reveal HQ".to_string(),
        Effect::RevealHand(Side::Runner) => "reveal the grip".to_string(),
        Effect::Expose => "expose it".to_string(),
        Effect::Remember { what: netrunner_core::dsl::Remembered::SelectedCard, until } => format!("remember the chosen card {}", duration(until)),
        Effect::Remember { what: netrunner_core::dsl::Remembered::CardType(card_type), until } => {
            format!("the chosen card type is {} {}", humanize(format!("{card_type:?}")).to_lowercase(), duration(until))
        }
        Effect::Remember { what: netrunner_core::dsl::Remembered::IceType(ice_type), until } => {
            format!("the chosen subtype is {} {}", crate::board::facts::ice_type_words(&[*ice_type]), duration(until))
        }
        Effect::ReplaceSubroutines => "for this encounter, the Corp resolves this card's subroutine instead of each subroutine on the ice".to_string(),
        Effect::SpendChosenServer => "the chosen server is spent for this turn".to_string(),
        Effect::MoveRunToOutermost(Some(server)) => format!("move the run to the outermost ice of {}", describe_server(*server)),
        Effect::MoveRunToOutermost(None) => "move the run to the outermost ice of the attacked server".to_string(),
        Effect::InstallAgendaFromRunnerScoreArea => "install an agenda from the Runner's score area".to_string(),
        Effect::SwapApproachedIceWithCard { this_ice: true, .. } => "swap this ice with a card".to_string(),
        Effect::SwapApproachedIceWithCard { .. } => "swap the approached ice with a card".to_string(),
    }
}

/// A `Debug` rendering made readable: `NoActionTakenThisTurn` → `no action
/// taken this turn`, `RunnerCreditsAtMost(3)` → `runner credits at most
/// 3`. The fallback for the parts of the DSL with no sentence of their
/// own — requirements mostly, which name themselves well.
pub fn humanize(debug: String) -> String {
    let mut out = String::with_capacity(debug.len() + 8);
    let mut prev_lower = false;
    for ch in debug.chars() {
        match ch {
            'A'..='Z' => {
                if prev_lower {
                    out.push(' ');
                }
                out.push(ch.to_ascii_lowercase());
                prev_lower = false;
            }
            '(' | '{' => {
                out.push(' ');
                prev_lower = false;
            }
            ')' | '}' | '"' => prev_lower = false,
            ',' => {
                out.push(',');
                prev_lower = false;
            }
            _ => {
                out.push(ch);
                prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
/// A delayed ability's sentence (`Effect::LaterThisTurn`): "when this turn
/// ends, …" for the turn's end, and otherwise the moment it waits for, once
/// or every time, and what it will do. The hud reads a waiting one the same
/// way.
pub fn describe_later_this_turn(when: netrunner_core::dsl::Trigger, filter: Option<&EventFilter>, every_time: bool, effect: &Effect, registry: &CardRegistry) -> String {
    let what = describe_effect(effect, registry);
    if when == netrunner_core::dsl::Trigger::OnDiscardPhaseEnd && filter.is_none() {
        return format!("when this turn ends, {what}");
    }
    let mut moment = humanize(format!("{when:?}")).to_lowercase();
    if let Some(filter) = filter {
        moment = format!("{moment}, {}", describe_when(filter));
    }
    if every_time {
        format!("for the rest of this turn, every time ({moment}): {what}")
    } else {
        format!("the next time this turn ({moment}): {what}")
    }
}

/// The words for a trigger's `when`, after the trigger's own: "on HQ", "of
/// a virus program". A conjunction says each of its parts.
fn describe_when(filter: &EventFilter) -> String {
    match filter {
        EventFilter::Mark => "on your mark".to_string(),
        EventFilter::ChosenServer => "on the chosen server".to_string(),
        EventFilter::ProtectedByIce => "on a server protected by ice".to_string(),
        EventFilter::Server(servers) => {
            let servers: Vec<String> = servers.iter().map(|server| describe_server(*server)).collect();
            format!("on {}", servers.join(" or "))
        }
        EventFilter::Card(filter) => format!("of {}", humanize(format!("{filter:?}"))),
        EventFilter::InstalledCard(filter) => format!("of an installed card ({})", humanize(format!("{filter:?}"))),
        EventFilter::Damage(kind) => format!("of {} damage", format!("{kind:?}").to_lowercase()),
        EventFilter::AtLeast(least) => format!("{least} or more"),
        EventFilter::Whose(side) => format!("the {side:?}'s"),
        EventFilter::Anyone => "anyone's".to_string(),
        EventFilter::OwnedBy { owner, whose } => format!("the {whose:?}'s, of a {owner:?} card"),
        EventFilter::ByThis => "by this card".to_string(),
        EventFilter::Host => "of host ice".to_string(),
        EventFilter::InRoot => "by the Corp, in the root of a server".to_string(),
        EventFilter::InRootOfThisServer => "by the Corp, in the root of this server".to_string(),
        EventFilter::TrashedFromThisServer => "from the root of this server or protecting it, except during installation".to_string(),
        EventFilter::TrashedRezzed => "a rezzed card, except during installation".to_string(),
        EventFilter::TrashedFrom(places) => {
            let places: Vec<&str> = places
                .iter()
                .map(|place| match place {
                    TrashedFrom::Installed => "the table",
                    TrashedFrom::Hand => "a hand",
                    TrashedFrom::Deck => "a deck",
                    TrashedFrom::Elsewhere => "anywhere else",
                    TrashedFrom::PlayArea => "the play area",
                })
                .collect();
            format!("from {}", places.join(" or "))
        }
        EventFilter::InstalledFromHq(true) => "from HQ".to_string(),
        EventFilter::InstalledFromHq(false) => "from anywhere except HQ".to_string(),
        EventFilter::ServerKind(kind) => format!("on {}", match kind {
            netrunner_core::dsl::ServerKind::Remote => "a remote server",
            netrunner_core::dsl::ServerKind::Central => "a central server",
            netrunner_core::dsl::ServerKind::Hq => "HQ",
            netrunner_core::dsl::ServerKind::RnD => "R&D",
        }),
        EventFilter::InstalledIn(kind) => format!("in {}", match kind {
            netrunner_core::dsl::ServerKind::Remote => "a remote server",
            netrunner_core::dsl::ServerKind::Central => "a central server",
            netrunner_core::dsl::ServerKind::Hq => "HQ",
            netrunner_core::dsl::ServerKind::RnD => "R&D",
        }),
        EventFilter::Ice(facts) => {
            let words: Vec<&str> = [
                (facts.outermost, "the outermost ice"),
                (facts.after_fully_breaking, "after fully breaking it"),
                (facts.at_most_zero_strength, "on ice with 0 or less strength"),
                (facts.rezzed_code_gate_or_sentry, "a rezzed code gate or sentry"),
                (facts.printed_subroutine, "a printed subroutine"),
                (facts.rezzed_bioroid, "a rezzed piece of bioroid ice"),
            ]
            .into_iter()
            .filter_map(|(holds, word)| holds.then_some(word))
            .collect();
            words.join(", ")
        }
        EventFilter::All(parts) => parts.iter().map(describe_when).collect::<Vec<_>>().join(", "),
    }
}


/// One line per trigger, ability and subroutine: `[when] clause → engine
/// reading`. The clause is the printed text the author linked
/// (`TriggeredEffect::text`, `AbilityDef::text`, `SubroutineDef::text`),
/// and the reading is the DSL rendered by this module. Both clients'
/// inspectors show these under "Engine reads it as", beside the printed
/// text, so a person can see whether the two agree.
pub fn engine_reading(card: &CardDefinition, registry: &CardRegistry) -> Vec<String> {
    let mut lines = Vec::new();
    for trigger in &card.triggers {
        let reading = trigger.effects.iter().map(|e| describe_effect(e, registry)).collect::<Vec<_>>().join("; ");
        let mut when = humanize(format!("{:?}", trigger.trigger));
        // "On HQ" and "a virus" used to be in the trigger's name; they are
        // the card's own filter now, and the reading still has to say them.
        if let Some(filter) = &trigger.when {
            when = format!("{when}, {}", describe_when(filter));
        }
        if trigger.first_each_turn {
            when = format!("the first time each turn: {when}");
        }
        match &trigger.text {
            Some(text) => lines.push(format!("• [{when}] \"{}\" → {reading}", text.trim_end_matches('.'))),
            None => lines.push(format!("• [{when}] → {reading}")),
        }
    }
    // What the card says about credits hosted on it. Neither was ever read
    // out: what hosted credits pay for had no words here at all, and the
    // refill was two trigger lines ("On install", "On turn start") that
    // said when it happened and not what the credits were for.
    if let Some(credits) = card.recurring_credits {
        lines.push(format!(
            "• [recurring] → when this becomes active, and before your turn begins, refill to {}",
            plural(credits, "hosted credit", "hosted credits")
        ));
    }
    if !card.pays_for.is_empty() {
        let purposes: Vec<String> = card.pays_for.iter().map(describe_pays_for).collect();
        lines.push(format!("• [hosted credits] → may be spent {}", purposes.join(", or ")));
    }
    for effect in &card.continuous {
        let reading = describe_continuous(effect);
        match &effect.text {
            Some(text) => lines.push(format!("• [while active] \"{}\" → {reading}", text.trim_end_matches('.'))),
            None => lines.push(format!("• [while active] → {reading}")),
        }
    }
    for ability in &card.abilities {
        let reading = describe_effect(&ability.effect, registry);
        let cost = ability.cost.as_ref().map(|c| format!("{}: ", describe_cost(c))).unwrap_or_default();
        match &ability.text {
            Some(text) => lines.push(format!("• [ability] \"{}\" → {cost}{reading}", text.trim_end_matches('.'))),
            None => lines.push(format!("• [ability] → {cost}{reading}")),
        }
    }
    for sub in &card.subroutines {
        lines.push(format!("• [subroutine] \"{}\" → {}", sub.text.trim_end_matches('.'), describe_effect(&sub.effect, registry)));
    }
    lines
}

/// A place a payment can be taken from, as a person would name it: the card
/// whose credits they are, or the run's. Read off the view, so an install
/// the viewer cannot identify is named as that — though a pool is always on
/// an active card, which both players can see.
pub fn pool_name(pool: Pool, view: &ClientView, registry: &CardRegistry) -> String {
    let hosted = |install| {
        let runner = view.runner.rig.iter().find(|card| card.install_id == install).map(|card| &card.card);
        let corp = view.corp.servers.iter().flat_map(|server| server.root.iter().chain(&server.ice)).find(|card| card.install_id == install).and_then(|card| card.card.as_ref());
        runner.or(corp).map(|card| title(card, registry))
    };
    match pool {
        Pool::Hosted(install) => hosted(install).unwrap_or_else(|| "an installed card".to_string()),
        Pool::Identity => view.corp.identity.as_ref().map(|card| title(card, registry)).unwrap_or_else(|| "the identity".to_string()),
        Pool::BadPublicity => "bad publicity".to_string(),
        Pool::Run => "the run".to_string(),
        Pool::Wallet => "the credit pool".to_string(),
    }
}

/// What a card's hosted credits may be spent on (`dsl::PaysFor`), as the
/// end of the sentence "may be spent …". Exhaustive, so a new word does not
/// compile until it has one here.
pub fn describe_pays_for(word: &PaysFor) -> String {
    match word {
        PaysFor::TrashCosts(CardFilter::Any) => "to pay trash costs".to_string(),
        PaysFor::TrashCosts(filter) => format!("to pay the trash cost of a card matching {}", humanize(format!("{filter:?}"))),
        PaysFor::Installing(filter) => format!("to install a card matching {}", humanize(format!("{filter:?}"))),
        PaysFor::RezzingInThisServer => "to rez assets in the root of this server and ice protecting it".to_string(),
        PaysFor::TraceAttempts => "during trace attempts".to_string(),
        PaysFor::Using(CardFilter::Icebreaker) => "to pay for using icebreakers".to_string(),
        PaysFor::Using(filter) => format!("to use a card matching {}", humanize(format!("{filter:?}"))),
        PaysFor::Playing(filter) => format!("to play a card matching {}", humanize(format!("{filter:?}"))),
        PaysFor::RemovingTags => "to take the basic action to remove a tag".to_string(),
        PaysFor::DuringRuns => "during runs".to_string(),
        PaysFor::DuringItsRun => "during the run this card began".to_string(),
        PaysFor::DuringRunsOnCentralServers => "during runs on central servers".to_string(),
        PaysFor::DuringSuccessfulRuns => "for the remainder of a successful run".to_string(),
        PaysFor::UsingDuringRuns(filter) => format!("to use a card matching {} during runs", humanize(format!("{filter:?}"))),
    }
}

/// A standing effect as a sentence: whom it is about, what changes, and
/// when it is on. None of the fields this replaced was ever read out — a
/// console's "+1[mu]" was invisible to the inspector.
pub fn describe_continuous(effect: &ContinuousEffect) -> String {
    let lower = |debug: String| humanize(debug).to_lowercase();
    let whom = match &effect.applies_to {
        Scope::This => "this card".to_string(),
        Scope::Host => "the card this is hosted on".to_string(),
        Scope::Hosted => "each card hosted on this".to_string(),
        Scope::Controller => "its controller".to_string(),
        Scope::Installing(filter) if effect.first_each_turn => format!("the first card its controller installs each turn ({})", lower(format!("{filter:?}"))),
        Scope::Installing(filter) => format!("a card its controller installs ({})", lower(format!("{filter:?}"))),
        Scope::InstallingOntoThis(filter) => format!("a card its controller installs onto this card ({})", lower(format!("{filter:?}"))),
        Scope::Ice(CardFilter::Any) => "each piece of ice".to_string(),
        Scope::Ice(filter) => format!("each piece of ice ({})", lower(format!("{filter:?}"))),
        Scope::Rig(filter) => format!("each installed card of the Runner's ({})", lower(format!("{filter:?}"))),
        Scope::RootOfThisServer(filter) => format!("each card in the root of this server ({})", lower(format!("{filter:?}"))),
        Scope::IceProtectingThisServer(filter) => format!("each piece of ice protecting this server ({})", lower(format!("{filter:?}"))),
        Scope::Playing(filter) if effect.first_each_turn => format!("the first card its controller plays each turn ({})", lower(format!("{filter:?}"))),
        Scope::Playing(filter) => format!("a card its controller plays ({})", lower(format!("{filter:?}"))),
        Scope::Stealing(filter) => format!("an agenda the Runner steals ({})", lower(format!("{filter:?}"))),
        Scope::StealingFromThisServer => "an agenda the Runner steals from this server".to_string(),
        Scope::Accessing(CardFilter::Any) => "each card the Runner accesses".to_string(),
        Scope::Accessing(filter) => format!("each card the Runner accesses ({})", lower(format!("{filter:?}"))),
        Scope::Scoring(filter) => format!("an agenda the Corp scores ({})", lower(format!("{filter:?}"))),
        Scope::ScoreArea(side) => format!("this agenda, in the {side:?}'s score area"),
        Scope::RunsOnThisServer => "each run against this server".to_string(),
        Scope::Runs(kind) => format!("each run against {}", match kind {
            netrunner_core::dsl::ServerKind::Hq => "HQ",
            netrunner_core::dsl::ServerKind::RnD => "R&D",
            netrunner_core::dsl::ServerKind::Central => "a central server",
            netrunner_core::dsl::ServerKind::Remote => "a remote server",
        }),
        Scope::Trashing(filter) => format!("a resource the Corp trashes with the basic action ({})", lower(format!("{filter:?}"))),
        Scope::Player(side) => format!("the {side:?}"),
    };
    let signed = |number: &Number| {
        let each = if number.per < 0 { format!("−{}", number.per.unsigned_abs()) } else { format!("+{}", number.per) };
        match number.of {
            Amount::Fixed(1) => each,
            ref of => format!("{each} for each of {}", describe_amount(of)),
        }
    };
    let what = match &effect.kind {
        ContinuousKind::Strength(number) => format!("gets {} strength", signed(number)),
        ContinuousKind::Memory(number) => format!("gets {} memory", signed(number)),
        ContinuousKind::HandSize(number) => format!("gets {} maximum hand size", signed(number)),
        ContinuousKind::HostsMemory(number) => format!("hosts up to {} memory of programs, outside the memory limit", signed(number).trim_start_matches('+')),
        ContinuousKind::AgendaPointsToWin(number) => format!("needs {} agenda points to win", signed(number)),
        ContinuousKind::AllottedClicks(number) => format!("gets {} allotted [click] each turn", signed(number)),
        ContinuousKind::Link(number) => format!("gets {} link", signed(number)),
        ContinuousKind::InstallCost(number) => format!("costs {} to install", signed(number)),
        ContinuousKind::RezCost(number) => format!("costs {} to rez", signed(number)),
        ContinuousKind::TrashCost(number) => format!("costs {} to trash", signed(number)),
        ContinuousKind::PlayCost(number) => format!("costs {} to play", signed(number)),
        ContinuousKind::PlayClicks(number) => format!("costs {} [click] to play", signed(number)),
        ContinuousKind::StealCost(cost) => format!("to steal it, also {}", describe_cost(cost)),
        ContinuousKind::RunCost(cost) => format!("costs the Runner \"{}\" to make, as the server is announced", describe_cost(cost)),
        ContinuousKind::AdditionalTrashCost(cost) => format!("to trash it, also {}", describe_cost(cost)),
        ContinuousKind::RemoteServerLimit(n) => format!("may have no more than {}", plural(*n, "remote server", "remote servers")),
        ContinuousKind::ScoreCost(cost) => format!("costs \"{}\" to score", describe_cost(cost)),
        ContinuousKind::BasicTrashCost(cost) => format!("costs the Corp \"{}\" to trash with the basic action", describe_cost(cost)),
        ContinuousKind::AgendaPoints(number) => format!("is worth {} agenda points", signed(number)),
        ContinuousKind::AdvancementRequirement(number) => format!("gets {} advancement requirement", signed(number)),
        ContinuousKind::GainSubtype(subtype) => format!("gains {}", lower(format!("{subtype:?}"))),
        ContinuousKind::BoostsLastTheRun => "keeps its strength boosts for the rest of the run".to_string(),
        ContinuousKind::RevealedWhileAccessed => "is revealed while it is accessed".to_string(),
        ContinuousKind::RezzedAsNonIce => "can be rezzed any time a non-ice card could be".to_string(),
        ContinuousKind::MayHost => "may be installed there".to_string(),
        ContinuousKind::CannotBeDeclaredSuccessful => "cannot be declared successful".to_string(),
        ContinuousKind::AccessOthersAtMost(n) => format!("accesses at most {} other than this card", plural(*n, "card", "cards")),
        ContinuousKind::BreakLimit { at_most, except_using: None } => {
            format!("at most {} of its printed subroutines can be broken each encounter", at_most)
        }
        ContinuousKind::BreakLimit { at_most, except_using: Some(subtype) } => format!(
            "at most {} of its printed subroutines can be broken each encounter, except with {} icebreakers",
            at_most,
            subtype.printed().to_lowercase()
        ),
        ContinuousKind::CannotBeBrokenUsing(subtype) => format!("its subroutines cannot be broken using {} programs", subtype.printed()),
        ContinuousKind::StrengthCannotBeLowered => "its strength cannot be lowered".to_string(),
        ContinuousKind::TrashLimit(n) => format!("trashes at most {} each encounter", plural(*n, "installed Runner card", "installed Runner cards")),
        ContinuousKind::Cannot(what) => match what {
            Prohibition::ScoreAgendas => "cannot score agendas",
            Prohibition::StealOrTrash => "cannot steal or trash cards",
            Prohibition::StealOrTrashAgendas => "cannot steal or trash agendas",
            Prohibition::SpendOrLoseCreditPool => "cannot lose or spend credits from their credit pool",
            Prohibition::SpendCredits => "cannot spend credits",
            Prohibition::EndTheRun => "cannot end the run with a subroutine",
            Prohibition::RunOnRemote if effect.first_each_turn => "cannot make the first run each turn on a remote server",
            Prohibition::RunOnRemote => "cannot run on a remote server",
            Prohibition::Run => "cannot make another run",
            Prohibition::AccessOthers => "cannot access cards other than this card",
            Prohibition::Access => "cannot access this card",
            Prohibition::BreakSubroutines => "cannot break subroutines",
            Prohibition::DiscardStep => "skips their discard step",
            Prohibition::BioroidIceAbilities => "cannot use paid abilities printed on bioroid ice",
            Prohibition::Rez => "cannot rez this card",
            Prohibition::BreakSubroutinesOnIce => "cannot break subroutines on this ice with Runner card abilities",
            Prohibition::DeclaredSuccessful => "cannot be declared successful",
            Prohibition::BreakWithNonIcebreakers => "cannot use non-icebreaker cards to break subroutines",
            Prohibition::RepeatAnAction => "cannot take an action already taken this turn",
        }
        .to_string(),
        ContinuousKind::Subroutines { subroutine, count, before } => format!(
            "gains \"[subroutine] {}\"{} {}",
            subroutine.text,
            if *before { " before its other subroutines" } else { "" },
            match (&count.of, count.per) {
                (Amount::Fixed(1), per) => format!("{per} times"),
                (of, 1) => format!("once for each of {}", describe_amount(of)),
                (of, per) => format!("{per} times for each of {}", describe_amount(of)),
            }
        ),
        ContinuousKind::LosesAbilities => "loses all abilities except its printed subroutines".to_string(),
        ContinuousKind::CannotGainAbilities => "cannot gain abilities".to_string(),
    };
    match &effect.condition {
        Some(condition) => format!("{whom} {what}, while {}", lower(format!("{condition:?}"))),
        None => format!("{whom} {what}"),
    }
}

/// The card that parked a decision, if the view names one: the card whose
/// text the prompt came from, else the card being resolved.
pub fn decision_card(view: &ClientView) -> Option<&CardId> {
    if let Some(paid) = &view.pending_paid_choice {
        return paid.prompting_card.as_ref().or(paid.source_card.as_ref());
    }
    match view.pending_decision.as_ref()? {
        PendingDecision::ChooseEffect { source_card, prompting_card, .. }
        | PendingDecision::ChooseCards { source_card, prompting_card, .. }
        | PendingDecision::ChooseServer { source_card, prompting_card, .. }
        | PendingDecision::ChooseNumber { source_card, prompting_card, .. }
        | PendingDecision::ChooseCardName { source_card, prompting_card, .. }
        | PendingDecision::PsiGame { source_card, prompting_card, .. } => prompting_card.as_ref().or(source_card.as_ref()),
        PendingDecision::ChooseTriggerOrder { .. } => None,
    }
}

/// One of a card's printed ways to pay for its rez
/// (`CardDefinition::rez_alternatives`), as its button reads: what it
/// costs beside the credits, and what it takes off them — "Forfeit 1
/// agenda, 10 credits off", "Pay the full rez cost".
pub fn rez_alternative_label(card: &netrunner_core::dsl::CardId, index: usize, registry: &CardRegistry) -> String {
    let Some(alternative) = registry.get(card).and_then(|def| def.rez_alternatives.get(index)) else {
        return format!("Choose option {}", index + 1);
    };
    let mut words = match &alternative.cost {
        Some(cost) => describe_cost(cost),
        None if alternative.discount == 0 => "pay the rez cost".to_string(),
        None => "pay the full rez cost".to_string(),
    };
    if alternative.discount > 0 {
        words = format!("{words}, {} off", plural(alternative.discount, "credit", "credits"));
    }
    if let Some(first) = words.get(0..1) {
        words.replace_range(0..1, &first.to_uppercase());
    }
    words
}

/// The line under an install's question (CR 8.5.6): why a card has to go,
/// or that the rest is up to the installer.
pub fn install_trash_detail(question: &netrunner_core::rules::InstallQuestion) -> String {
    match (question.memory_short, question.may_stop) {
        (0, true) => "Trash another, or install it now.".to_string(),
        (0, false) => "Trash at least one.".to_string(),
        (short, _) => format!("It needs {short} more MU: trash a program to make room."),
    }
}

/// What the actions pane is asking, when a card is asking it: `Bigger
/// Picture asks — choose one`. `None` for an ordinary turn, where the
/// pane keeps its usual title.
pub fn decision_prompt(view: &ClientView, registry: &CardRegistry) -> Option<String> {
    // A parked payment is asked ahead of anything parked beneath it, as the
    // engine answers it. Only the payer's view has one to word.
    if let Some(payment) = view.pending_payment.as_ref().and_then(|payment| payment.own.as_ref()) {
        return Some(match &payment.question {
            netrunner_core::rules::PaymentAsk::Pools(question) => format!(
                "Pay {} — how many from {}?",
                plural(payment.amount, "credit", "credits"),
                pool_name(question.pool, view, registry)
            ),
            netrunner_core::rules::PaymentAsk::Card(question) => format!(
                "Pay the cost — trash which card from {}? ({} to go)",
                describe_zone(&question.zone),
                question.remaining
            ),
            netrunner_core::rules::PaymentAsk::Alternative { card, .. } => format!("Rez {} — how will you pay?", title(card, registry)),
            netrunner_core::rules::PaymentAsk::X { max } => format!("Choose X, from 0 to {max} — you pay X credits"),
            netrunner_core::rules::PaymentAsk::Install(question) => {
                format!("Install {} — trash which first? {}", title(&question.card, registry), install_trash_detail(question))
            }
        });
    }
    let name = decision_card(view).map(|id| title(id, registry));
    let asks = |what: String| Some(match &name { Some(card) => format!("{card} asks — {what}"), None => what });
    if let Some(paid) = &view.pending_paid_choice {
        return asks(format!("pay {} to {}?", describe_cost(&paid.cost), describe_effect(&paid.if_paid, registry)));
    }
    match view.pending_decision.as_ref()? {
        PendingDecision::ChooseEffect { .. } => asks("choose one".to_string()),
        PendingDecision::ChooseCards { source, min, max, .. } => {
            let how_many = if min == max { plural(*min, "card", "cards") } else { format!("{min} to {max} cards") };
            // The chooser is told what they have chosen so far, by name —
            // the one line a terminal pane has for it.
            let chosen = crate::selection::Selection::of(view, registry).map(|s| format!(" ({})", s.summary())).unwrap_or_default();
            asks(format!("choose {how_many} from {}{chosen}", describe_zone(source)))
        }
        PendingDecision::ChooseServer { install, remember, .. } => match crate::placement::Placement::of(view, registry) {
            Some(placement) => asks(placement.question()),
            None if install.is_some() => asks("installing a card".to_string()),
            None if *remember => asks("choose a server".to_string()),
            None => asks("choose a server to run".to_string()),
        },
        PendingDecision::ChooseTriggerOrder { .. } => Some("Choose which trigger resolves first".to_string()),
        // The card's own clause is the question (the Linked Clause Rule):
        // "Bigger Picture asks — Remove any number of tags (0 to 3)".
        PendingDecision::ChooseNumber { text, min, max, .. } if !text.is_empty() => asks(format!("{text} ({min} to {max})")),
        PendingDecision::ChooseNumber { min, max, .. } => asks(format!("choose a number from {min} to {max}")),
        PendingDecision::ChooseCardName { text, names, .. } => asks(format!("{} ({} names)", if text.is_empty() { "choose a card name" } else { text.as_str() }, names.len())),
        PendingDecision::PsiGame { corp_bid: netrunner_core::rules::PsiBid::Awaiting, corp_max, .. } => {
            asks(format!("play a Psi Game: the Corp bids 0 to {corp_max} credits, in secret"))
        }
        PendingDecision::PsiGame { runner_max, .. } => asks(format!("play a Psi Game: the Corp has bid; the Runner bids 0 to {runner_max} credits")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::EffectRequirement;

    #[test]
    fn effects_read_as_sentences() {
        let registry = crate::decks::sample_deck_registry();
        let d = |effect: &Effect| describe_effect(effect, &registry);
        assert_eq!(d(&Effect::GiveTags(Amount::Fixed(1))), "give the Runner 1 tag");
        assert_eq!(d(&Effect::Sequence(vec![])), "do nothing");
        assert_eq!(d(&Effect::Sequence(vec![Effect::GainCredits(Side::Corp, 2), Effect::EndTheRun])), "the Corp gains 2 credits, then end the run");
        assert_eq!(d(&Effect::DealDamage(DamageType::Net, 1)), "do 1 net damage");
        assert_eq!(
            d(&Effect::OfferPaidChoice {
                side: Side::Runner,
                cost: Cost::Credits(8),
                if_paid: Box::new(Effect::Sequence(vec![])),
                if_declined: Box::new(Effect::GiveTags(Amount::Fixed(1))),
                text: None,
                if_able: false,
            }),
            "the Runner may pay 8 credits to do nothing; otherwise give the Runner 1 tag"
        );
        assert_eq!(
            d(&Effect::EffectIf { condition: EffectRequirement::IsTagged, effect: Box::new(Effect::DealDamage(DamageType::Meat, 4)), otherwise: None }),
            "if is tagged: do 4 meat damage"
        );
        assert_eq!(describe_cost(&Cost::AnyOf(vec![Cost::Credits(2), Cost::TrashSelf])), "2 credits or trash this card");
    }

    #[test]
    fn humanize_splits_camel_case_and_drops_brackets() {
        assert_eq!(humanize("NoActionTakenThisTurn".to_string()), "no action taken this turn");
        assert_eq!(humanize("RunnerCreditsAtMost(3)".to_string()), "runner credits at most 3");
        assert_eq!(humanize("InHeapWithSubtype(\"x\")".to_string()), "in heap with subtype x");
    }

    /// "On HQ" left the trigger's name for the card's own filter, and the
    /// inspector is where a person checks the engine against the card.
    #[test]
    fn the_engine_reading_says_which_occurrences_a_trigger_means() {
        let registry = crate::decks::sample_deck_registry();
        let reading = |id: &str| engine_reading(registry.get(&CardId(id.to_string())).expect(id), &registry).join("\n");
        assert!(reading("leech").contains("[on successful run, on HQ or R&D or Archives]"), "{}", reading("leech"));
        assert!(reading("nbn_reality_plus").contains("[the first time each turn: on tags given]"), "{}", reading("nbn_reality_plus"));
        assert!(reading("cookbook").contains("[on card installed, of has subtype virus]"), "{}", reading("cookbook"));
    }

    /// Every choice a sample-deck card can present has a sentence — no
    /// option on a deck people play falls back to its debug form.
    #[test]
    fn every_sample_deck_choice_has_words() {
        let registry = crate::decks::sample_deck_registry();
        let mut checked = 0;
        for deck in netrunner_core::decks::embedded_decks() {
            for entry in &deck.cards {
                let Some(card) = registry.get(&entry.card) else { continue };
                let mut effects: Vec<&Effect> = card.triggers.iter().flat_map(|t| t.effects.iter()).collect();
                effects.extend(card.abilities.iter().map(|a| &a.effect));
                effects.extend(card.subroutines.iter().map(|s| &s.effect));
                for effect in effects {
                    let text = describe_effect(effect, &registry);
                    assert!(!text.is_empty() && !text.contains("::") && !text.contains("{ "), "{}: {text}", card.title);
                    checked += 1;
                }
            }
        }
        assert!(checked > 100, "{checked} effects checked");
    }
}
