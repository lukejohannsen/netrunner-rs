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
use netrunner_core::dsl::{
    Amount, EffectDuration, CardDefinition, CardId, CardTarget, CardZoneRef, ContinuousEffect, ContinuousKind, Cost, DamageType, Effect, EventFilter, Number, Preventable, Prohibition,
    Scope, SubroutineBreakCount,
};
use netrunner_core::rules::{PendingDecision, ServerId, Side};
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
        Amount::TimesThisTurn(trigger) => format!("the times \"{}\" has happened this turn", humanize(format!("{trigger:?}"))),
        Amount::TimesLastTurn(trigger) => format!("the times \"{}\" happened last turn", humanize(format!("{trigger:?}"))),
        Amount::HostedCounters => "the counters on this card".to_string(),
        Amount::HostedAdvancementTokens => "the advancement tokens on this card".to_string(),
        Amount::InstalledIcebreakerCount => "the number of installed icebreakers".to_string(),
        Amount::FacedownCardsInArchives => "the number of facedown cards in Archives".to_string(),
        Amount::CreditsLostThisResolution => "the credits just lost".to_string(),
        Amount::ClicksRemaining => "the clicks remaining".to_string(),
        Amount::PrintedInstallCost => "its printed install cost".to_string(),
        Amount::RemainingAfterSelection(n) => format!("{n} less the cards chosen"),
        Amount::ThreatLevel => "the threat level".to_string(),
        Amount::RunnerTags => "the Runner's tags".to_string(),
        Amount::InHeapWithSubtype(subtype) => format!("the number of {} cards in the heap", humanize(format!("{subtype:?}")).to_lowercase()),
        Amount::IceProtectingThisServer => "the number of pieces of ice protecting this server".to_string(),
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
        CardZoneRef::OpponentInstalled => "the opponent's installed cards",
        CardZoneRef::OpponentDiscard => "the opponent's discard pile",
        CardZoneRef::OwnInstalled => "your installed cards",
        CardZoneRef::HostedOnSource => "the cards hosted here",
        CardZoneRef::TopOfOwnStack => "the top of the stack",
        CardZoneRef::OpponentHand => "the opponent's hand",
        CardZoneRef::OpponentDeck => "the opponent's deck",
        CardZoneRef::OpponentScoreArea => "the opponent's score area",
    }
}

pub fn describe_cost(cost: &Cost) -> String {
    match cost {
        Cost::Credits(n) => plural(*n, "credit", "credits"),
        Cost::Clicks(n) => plural(*n, "click", "clicks"),
        Cost::TrashSelf => "trash this card".to_string(),
        Cost::ClearTags => "remove all tags".to_string(),
        Cost::TakeTags(n) => format!("take {}", plural(*n, "tag", "tags")),
        Cost::RemoveCounters(n) => format!("remove {}", plural(*n, "counter", "counters")),
        Cost::RemoveSelfFromGame => "remove this card from the game".to_string(),
        Cost::TrashRandomFromHq(n) => format!("trash {} at random from HQ", plural(*n, "card", "cards")),
        Cost::AnyOf(options) => options.iter().map(describe_cost).collect::<Vec<_>>().join(" or "),
        Cost::AllOf(parts) => parts.iter().map(describe_cost).collect::<Vec<_>>().join(" and "),
    }
}

fn describe_target(target: &CardTarget, registry: &CardRegistry) -> String {
    match target {
        CardTarget::ThisCard => "this card".to_string(),
        CardTarget::CorpInstalled { card, server } => format!("{} in {}", title(card, registry), describe_server(*server)),
        CardTarget::RunnerRig(card) => title(card, registry),
        CardTarget::TopOfStack { side, .. } => format!("the top card of {}'s deck", who(*side)),
        CardTarget::HostIce => "the host ice".to_string(),
        CardTarget::HostedOnThisCard => "the card hosted here".to_string(),
    }
}

fn damage_word(kind: &DamageType) -> &'static str {
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
        Effect::DealDamageAmount(kind, amount) => format!("do {} damage equal to {}", damage_word(kind), describe_amount(amount)),
        Effect::ModifyStrength(n) if *n >= 0 => format!("+{n} strength"),
        Effect::ModifyStrength(n) => format!("{n} strength"),
        Effect::DrawCards(side, n) => format!("{} draws {}", who(*side), plural(*n, "card", "cards")),
        Effect::DrawCardsAmount(side, amount) => format!("{} draws cards equal to {}", who(*side), describe_amount(amount)),
        Effect::EndTheRun => "end the run".to_string(),
        Effect::GiveTags(n) => format!("give the Runner {}", plural(*n, "tag", "tags")),
        Effect::RemoveTags(n) => format!("remove {}", plural(*n, "tag", "tags")),
        Effect::GiveBadPublicity(n) => format!("the Corp takes {}", plural(*n, "bad publicity", "bad publicity")),
        Effect::RemoveBadPublicity(n) => format!("remove {}", plural(*n, "bad publicity", "bad publicity")),
        Effect::TrashCard(target) => format!("trash {}", describe_target(target, registry)),
        Effect::DerezCard(target) => format!("derez {}", describe_target(target, registry)),
        Effect::BoostStrength { amount, duration: d } => format!("+{amount} strength {}", duration(d)),
        Effect::BoostStrengthAmount { amount, duration: d } => format!("strength +{} {}", describe_amount(amount), duration(d)),
        Effect::BreakSubroutines { count, restrict_to } => {
            let which = match count {
                SubroutineBreakCount::Fixed(n) => plural(*n, "subroutine", "subroutines"),
                SubroutineBreakCount::All => "all subroutines".to_string(),
            };
            match restrict_to {
                Some(kind) => format!("break {which} on a {kind:?}"),
                None => format!("break {which}"),
            }
        }
        Effect::BreakSubroutinesUnconditionally { count } => match count {
            SubroutineBreakCount::Fixed(n) => format!("break {}", plural(*n, "subroutine", "subroutines")),
            SubroutineBreakCount::All => "break all subroutines".to_string(),
        },
        Effect::Trace { base, on_success } => format!("trace {base}: if successful, {}", describe_effect(on_success, registry)),
        Effect::AddAdditionalAccess { server, count } => {
            format!("access {} additional from {}", plural(*count, "card", "cards"), describe_server(*server))
        }
        Effect::AddAdditionalAccessAmount { server, amount } => {
            format!("access additional cards from {} equal to {}", describe_server(*server), describe_amount(amount))
        }
        Effect::SetAccessReplacement { server, effect, .. } => {
            format!("instead of accessing {}, {}", describe_server(*server), describe_effect(effect, registry))
        }
        Effect::LoseClicks(n) => format!("lose {}", plural(*n, "click", "clicks")),
        Effect::GainClicks(side, n) => format!("{} gains {}", who(*side), plural(*n, "click", "clicks")),
        Effect::GainClicksNextTurn(side, n) => format!("{} gains {} next turn", who(*side), plural(*n, "click", "clicks")),
        Effect::InitiateRun(server) => format!("run {}", describe_server(*server)),
        Effect::Prevent(Preventable::Damage { kind, up_to }) => match kind {
            Some(kind) => format!("prevent up to {up_to} {} damage", format!("{kind:?}").to_lowercase()),
            None => format!("prevent up to {up_to} damage"),
        },
        Effect::Prevent(Preventable::Tags(n)) => format!("prevent {n} tag{}", if *n == 1 { "" } else { "s" }),
        Effect::Prevent(Preventable::Trash(filter)) => format!("prevent 1 installed card from being trashed ({})", humanize(format!("{filter:?}")).to_lowercase()),
        Effect::AddCounters(n) => format!("place {}", plural(*n, "counter", "counters")),
        Effect::RemoveCounters(n) => format!("remove {}", plural(*n, "counter", "counters")),
        Effect::RefillCountersTo(n) => format!("refill to {}", plural(*n, "counter", "counters")),
        Effect::EffectIf { condition, effect } => format!("if {}: {}", humanize(format!("{condition:?}")), describe_effect(effect, registry)),
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
        Effect::ResolveSomeOf { chooser, count, options, .. } => format!(
            "{} chooses {} of: {}",
            who(*chooser),
            count,
            options.iter().map(|option| describe_effect(option, registry)).collect::<Vec<_>>().join(" / ")
        ),
        Effect::GainCreditsPerCardAccessedThisRun(side) => format!("{} gains 1 credit per card accessed this run", who(*side)),
        Effect::PromptChooseCards { side, source, min, max, .. } => {
            let how_many = if min == max { plural(*min, "card", "cards") } else { format!("{min} to {max} cards") };
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
        Effect::PromptInstallCorpCard { .. } => "install a card".to_string(),
        Effect::InstallRunnerCardFromGrip => "install a card from the grip".to_string(),
        Effect::InstallRunnerCardFromHeap => "install a card from the heap".to_string(),
        Effect::InstallRunnerCardFromGripWithDiscount(n) => format!("install a card from the grip, paying {n} less"),
        Effect::InstallRunnerCardFromHost => "install the hosted card".to_string(),
        Effect::RedirectRunOnApproach(server) => format!("redirect the run to {}", describe_server(*server)),
        Effect::SetRunEndedEffect(effect) => format!("when the run ends, {}", describe_effect(effect, registry)),
        Effect::ArmRunEndPrevention(_) => "the run cannot be ended by the next end-the-run effect".to_string(),
        Effect::Sabotage(n) => format!("sabotage {n}"),
        Effect::MillRnDAmount(amount) => format!("trash cards from the top of R&D equal to {}", describe_amount(amount)),
        Effect::HostCardOnThisCard(_) => "host a card on this card".to_string(),
        Effect::BypassEncounteredIce => "bypass this ice".to_string(),
        Effect::PurgeVirusCounters => "purge virus counters".to_string(),
        Effect::FlipIdentity => "flip the identity".to_string(),
        Effect::AddToBottomOfStack => "put it on the bottom of the stack".to_string(),
        Effect::HostRigCardOnInstall { .. } => "host it on an installed card".to_string(),
        Effect::Prohibit { what, until } => {
            let what = match what {
                Prohibition::StealOrTrash => "the Runner cannot steal or trash cards",
                Prohibition::ScoreAgendas => "the Corp cannot score agendas",
            };
            format!("{what} {}", duration(until))
        }
        Effect::PlaceAdvancementCounters(n) => format!("place {}", plural(*n, "advancement token", "advancement tokens")),
        Effect::MoveThisCardToRoot(server) => format!("move this card to {}", describe_server(*server)),
        Effect::PlayOperation { .. } => "play an operation".to_string(),
        Effect::ResolveSubroutineOfSelectedIce => "resolve a subroutine of the chosen ice".to_string(),
        Effect::ForfeitAgendas(n) => format!("forfeit {}", plural(*n, "agenda", "agendas")),
        Effect::MoveRunToOutermost(server) => format!("move the run to the outermost ice of {}", describe_server(*server)),
        Effect::InstallAgendaFromRunnerScoreArea => "install an agenda from the Runner's score area".to_string(),
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
        match &trigger.when {
            Some(EventFilter::Server(servers)) => {
                let servers: Vec<String> = servers.iter().map(|server| describe_server(*server)).collect();
                when = format!("{when}, on {}", servers.join(" or "));
            }
            Some(EventFilter::Card(filter)) => when = format!("{when}, of {}", humanize(format!("{filter:?}"))),
            Some(EventFilter::InstalledCard(filter)) => when = format!("{when}, of an installed card ({})", humanize(format!("{filter:?}"))),
            None => {}
        }
        if trigger.first_each_turn {
            when = format!("the first time each turn: {when}");
        }
        match &trigger.text {
            Some(text) => lines.push(format!("• [{when}] \"{}\" → {reading}", text.trim_end_matches('.'))),
            None => lines.push(format!("• [{when}] → {reading}")),
        }
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

/// A standing effect as a sentence: whom it is about, what changes, and
/// when it is on. None of the fields this replaced was ever read out — a
/// console's "+1[mu]" was invisible to the inspector.
pub fn describe_continuous(effect: &ContinuousEffect) -> String {
    let lower = |debug: String| humanize(debug).to_lowercase();
    let whom = match &effect.applies_to {
        Scope::This => "this card".to_string(),
        Scope::Host => "the card this is hosted on".to_string(),
        Scope::Controller => "its controller".to_string(),
        Scope::Installing(filter) if effect.first_each_turn => format!("the first card its controller installs each turn ({})", lower(format!("{filter:?}"))),
        Scope::Installing(filter) => format!("a card its controller installs ({})", lower(format!("{filter:?}"))),
        Scope::Ice => "each piece of ice".to_string(),
        Scope::RootOfThisServer(filter) => format!("each card in the root of this server ({})", lower(format!("{filter:?}"))),
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
        ContinuousKind::InstallCost(number) => format!("costs {} to install", signed(number)),
        ContinuousKind::RezCost(number) => format!("costs {} to rez", signed(number)),
        ContinuousKind::TrashCost(number) => format!("costs {} to trash", signed(number)),
        ContinuousKind::GainSubtype(subtype) => format!("gains {}", lower(format!("{subtype:?}"))),
        ContinuousKind::BoostsLastTheRun => "keeps its strength boosts for the rest of the run".to_string(),
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
        | PendingDecision::ChooseServer { source_card, prompting_card, .. } => prompting_card.as_ref().or(source_card.as_ref()),
        PendingDecision::ChooseTriggerOrder { .. } => None,
    }
}

/// What the actions pane is asking, when a card is asking it: `Bigger
/// Picture asks — choose one`. `None` for an ordinary turn, where the
/// pane keeps its usual title.
pub fn decision_prompt(view: &ClientView, registry: &CardRegistry) -> Option<String> {
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
        PendingDecision::ChooseServer { install, .. } => match crate::placement::Placement::of(view, registry) {
            Some(placement) => asks(placement.question()),
            None if install.is_some() => asks("installing a card".to_string()),
            None => asks("choose a server to run".to_string()),
        },
        PendingDecision::ChooseTriggerOrder { .. } => Some("Choose which trigger resolves first".to_string()),
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
        assert_eq!(d(&Effect::GiveTags(1)), "give the Runner 1 tag");
        assert_eq!(d(&Effect::Sequence(vec![])), "do nothing");
        assert_eq!(d(&Effect::Sequence(vec![Effect::GainCredits(Side::Corp, 2), Effect::EndTheRun])), "the Corp gains 2 credits, then end the run");
        assert_eq!(d(&Effect::DealDamage(DamageType::Net, 1)), "do 1 net damage");
        assert_eq!(
            d(&Effect::OfferPaidChoice {
                side: Side::Runner,
                cost: Cost::Credits(8),
                if_paid: Box::new(Effect::Sequence(vec![])),
                if_declined: Box::new(Effect::GiveTags(1)),
                text: None,
            }),
            "the Runner may pay 8 credits to do nothing; otherwise give the Runner 1 tag"
        );
        assert_eq!(
            d(&Effect::EffectIf { condition: EffectRequirement::IsTagged, effect: Box::new(Effect::DealDamage(DamageType::Meat, 4)) }),
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
