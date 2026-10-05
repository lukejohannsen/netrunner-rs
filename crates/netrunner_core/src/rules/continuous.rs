//! The one scan that reads `CardDefinition::continuous`.
//!
//! Every question a standing effect can change — how strong is this
//! icebreaker, what does this install cost, how much memory is there — is put
//! here, about a `Target`, and answered by walking the active cards
//! (`rules::active`) and asking each effect three things: is it the kind
//! being asked about, is the target one it applies to, is it on.
//!
//! **Who is asked is the Listener Rule's sentence again**: a card's text
//! about *itself* (`Scope::This`) applies wherever the card is — Carmen
//! prices herself from the grip, where she is not active — and every other
//! effect needs an active source. One more source is read off the run, for
//! the same reason `listeners` reads it: a persistent upgrade trashed during
//! a run on its server "still applies for the remainder of this run".
//!
//! **Derived, never cached.** There were six of these scans, one per field,
//! each in the handler that needed it; there is no seventh place to keep in
//! sync, and nothing here is written to `GameState`. If a profile ever says
//! the scan costs too much, memoise it for one `apply_action`, not on the
//! state a search clones.

use crate::cards::CardRegistry;
use crate::dsl::{card_matches_filter, CardDefinition, CardId, CardType, ContinuousEffect, ContinuousKind, Cost, IceType, Number, Prohibition, Scope};
use crate::rules::ability::{self, ResolutionContext};
use crate::rules::active::{self, ActiveCard};
use crate::rules::lingering;
use crate::rules::turn_log;
use crate::rules::run::{RunIce, ServerId};
use crate::rules::state::{GameState, InstallId, InstallSlot, InstalledRunnerCard, Side};

/// What a question is about.
#[derive(Clone, Copy)]
pub(crate) enum Target<'a> {
    /// A player — their memory, their maximum hand size.
    Player(Side),
    /// A card that is not installed: one being installed, priced from a hand.
    Card(&'a CardDefinition),
    /// A card being installed onto the rig card `host` (Hackerspace). What
    /// `Target::Card` is asked too, and also what a host's
    /// `Scope::InstallingOntoThis` reaches.
    InstallingOnto { card: &'a CardDefinition, host: InstallId },
    /// A card in the rig.
    Rig { card: &'a CardDefinition, install: InstallId },
    /// A Corp install, in the root of `server` or protecting it.
    Corp { card: &'a CardDefinition, install: InstallId, server: ServerId, root: bool },
    /// An agenda in `side`'s score area — the copy, when there is one, so a
    /// `while` reads its own counters (Megaprix Qualifier's "while this
    /// agenda has a hosted agenda counter"). `None` asks what the card is
    /// worth there before any copy has counters: an agenda landing.
    Scored { card: &'a CardDefinition, side: Side, install: Option<InstallId> },
    /// The Corp install `install`, an agenda the Corp is scoring (Word on
    /// the Street's additional cost). The copy, not only the card, because
    /// "an agenda the Corp installed this turn" is a fact about the copy.
    Scoring { card: &'a CardDefinition, install: InstallId },
    /// The rig install `install`, a resource the Corp is trashing with the
    /// basic action (Sebastião Souza Pessoa's additional cost).
    Trashing { card: &'a CardDefinition, install: InstallId },
    /// The run against `server`: whether it may be declared successful,
    /// how many cards it may access.
    Run { server: ServerId },
    /// A player, as a prohibition binds them (`Scope::Player`). Apart from
    /// `Player`, which walks only that player's own cards because it is
    /// asked after every action (`memory::refresh`): the Corp's ice binds
    /// the Runner (Attini), so this walks both tables.
    Bound(Side),
}

impl<'a> Target<'a> {
    fn card(&self) -> Option<&'a CardDefinition> {
        match self {
            Target::Player(_) | Target::Run { .. } | Target::Bound(_) => None,
            Target::Card(card)
            | Target::InstallingOnto { card, .. }
            | Target::Rig { card, .. }
            | Target::Corp { card, .. }
            | Target::Scored { card, .. }
            | Target::Scoring { card, .. }
            | Target::Trashing { card, .. } => Some(card),
        }
    }

    fn install(&self) -> Option<InstallId> {
        match self {
            Target::Rig { install, .. } | Target::Corp { install, .. } | Target::Scoring { install, .. } | Target::Trashing { install, .. } => Some(*install),
            Target::Scored { install, .. } => *install,
            Target::Player(_) | Target::Card(_) | Target::InstallingOnto { .. } | Target::Run { .. } | Target::Bound(_) => None,
        }
    }

    /// The Corp install `install`, as a target.
    pub(crate) fn corp_install(state: &GameState, registry: &'a CardRegistry, install: InstallId) -> Option<Self> {
        let installed = state.corp.installed.iter().find(|c| c.install_id == install)?;
        let card = registry.get(&installed.card)?;
        Some(Target::Corp { card, install, server: installed.server, root: installed.slot == InstallSlot::Root })
    }
}

/// A source of effects: a card, and where it stands.
struct Source<'a> {
    side: Side,
    card: &'a CardId,
    install: Option<InstallId>,
    server: Option<ServerId>,
    /// Only what the card says about itself is read — a source that is not
    /// active.
    own_text_only: bool,
    /// A persistent upgrade the Runner trashed this run: only what it says
    /// about its server survives it.
    server_text_only: bool,
}

impl<'a> From<ActiveCard<'a>> for Source<'a> {
    fn from(active: ActiveCard<'a>) -> Self {
        Source { side: active.side, card: active.card, install: active.install, server: active.server, own_text_only: false, server_text_only: false }
    }
}

/// Calls `found` with every effect of a kind `asked` about that applies to
/// `target` right now, its number resolved as its source.
///
/// **The kind is asked first, and an effect of another kind is never asked
/// whether it is on.** `asked` was not a parameter: every caller matched the
/// kind inside `found`, after the scan had evaluated the effect's `while`.
/// A `while` is a question of its own, and "threat 4" is one put back to
/// this scan — `Amount::ThreatLevel` is the greater score, a score is what
/// each card in a score area is worth (`agenda_points_in`), and that asked
/// the card's own text, whatever the text was about. So a Boto in a score
/// area — which no game reaches, and a bot's sample did — asked its "+2
/// strength" whether threat was 4 in order to be told it was not an agenda-
/// point effect, and never came back. With the kind first, a score reads
/// only what changes a score.
fn for_each_applying<'a>(
    state: &'a GameState,
    registry: &'a CardRegistry,
    target: Target<'a>,
    asked: impl Fn(&ContinuousKind) -> bool,
    mut found: impl FnMut(&'a ContinuousEffect, &Source<'a>, &ResolutionContext<'a>),
) {
    // A card that has lost its abilities applies none of them (CR 9.1.9a):
    // Hush's host, Klevetnik's resource. Read once for the scan.
    let lost = active::installs_without_abilities(state, registry);
    let mut ask = |source: Source<'a>| {
        let Some(definition) = registry.get(source.card) else { return };
        if definition.continuous.is_empty() {
            return;
        }
        if source.install.is_some_and(|install| lost.contains(&install)) {
            return;
        }
        let ctx = match source.install {
            Some(install) => ResolutionContext::for_install(install, source.card),
            None => ResolutionContext::for_card(Some(source.card)),
        };
        for effect in &definition.continuous {
            if !asked(&effect.kind) {
                continue;
            }
            let is_own_text = effect.applies_to.is_own_text();
            if source.own_text_only != is_own_text {
                continue;
            }
            if source.server_text_only && !matches!(effect.applies_to, Scope::RootOfThisServer(_) | Scope::RunsOnThisServer | Scope::StealingFromThisServer) {
                continue;
            }
            if !applies(state, &source, &effect.applies_to, &target) {
                continue;
            }
            if effect.first_each_turn && !first_this_turn(state, &source, effect) {
                continue;
            }
            if let Some(condition) = &effect.condition
                && ability::check_requirement(state, condition, source.side, &ctx, registry).is_err()
            {
                continue;
            }
            found(effect, &source, &ctx);
        }
    };

    // The card itself first, active or not.
    if let Some(card) = target.card() {
        ask(Source { side: card.side, card: &card.id, install: target.install(), server: None, own_text_only: true, server_text_only: false });
    }
    // A question about a player reaches only that player's own cards
    // (`Scope::Controller`), and it is the one asked after every action
    // (`memory::refresh`), so it does not walk the other side's table.
    match target {
        Target::Player(Side::Runner) => active::runner(state, registry).for_each(|active| ask(active.into())),
        Target::Player(Side::Corp) => active::corp(state, registry).for_each(|active| ask(active.into())),
        _ => active::active_cards(state, registry).for_each(|active| ask(active.into())),
    }
    if let Some(run) = &state.active_run {
        for card in &run.persistent_trashed_upgrades {
            ask(Source { side: Side::Corp, card, install: None, server: Some(run.server), own_text_only: false, server_text_only: true });
        }
    }
}

/// Whether the install or play being priced would be the turn's first that
/// `effect`'s `Scope::Installing` or `Scope::Playing` filter matches
/// (`ContinuousEffect::first_each_turn`). Asked of the turn, not of the
/// card: none counted yet, because a price is asked before the install or
/// the play.
fn first_this_turn(state: &GameState, source: &Source<'_>, effect: &ContinuousEffect) -> bool {
    let occurrences = match (&effect.applies_to, &effect.kind) {
        (Scope::Installing(filter), _) => turn_log::Occurrences::installs(filter, source.side),
        (Scope::Playing(filter), _) => turn_log::Occurrences::plays(filter, source.side),
        // "The first run each turn cannot be made…": asked before the run
        // is announced, so none counted yet.
        (Scope::Player(_), ContinuousKind::Cannot(what)) => match what.counted_as() {
            Some(trigger) => turn_log::Occurrences::meant_by(trigger, None, source.side),
            None => return true,
        },
        _ => return true,
    };
    occurrences.is_ok_and(|occurrences| state.this_turn.none_yet(&occurrences))
}

/// Whether `scope`, read from `source`, reaches `target`. `Scope::This` is
/// matched by who is asked (`Source::own_text_only`), not here.
fn applies(state: &GameState, source: &Source<'_>, scope: &Scope, target: &Target<'_>) -> bool {
    match (scope, target) {
        (Scope::This, _) => source.own_text_only,
        (Scope::ScoreArea(side), Target::Scored { side: scored_in, .. }) => source.own_text_only && side == scored_in,
        (Scope::ScoreArea(_), _) => false,
        (Scope::Host, target) => {
            let Some(host) = target.install() else { return false };
            let Some(install) = source.install else { return false };
            state
                .runner
                .rig
                .iter()
                .find(|card| card.install_id == install)
                .is_some_and(|card| card.hosted_on_rig_card == Some(host) || card.hosted_on_ice == Some(host))
        }
        (Scope::Controller, Target::Player(side)) => source.side == *side,
        (Scope::Installing(filter), Target::Card(card) | Target::InstallingOnto { card, .. }) => {
            card.side == source.side && card_matches_filter(card, filter)
        }
        (Scope::InstallingOntoThis(filter), Target::InstallingOnto { card, host }) => {
            source.install == Some(*host) && card.side == source.side && card_matches_filter(card, filter)
        }
        (Scope::Playing(filter), Target::Card(card)) => {
            card.side == source.side && matches!(card.card_type, CardType::Event | CardType::Operation) && card_matches_filter(card, filter)
        }
        (Scope::Stealing(filter), Target::Card(card)) => card.card_type == CardType::Agenda && card_matches_filter(card, filter),
        (Scope::Accessing(filter), Target::Card(card)) => card.side != source.side && card_matches_filter(card, filter),
        (Scope::StealingFromThisServer, Target::Card(card)) => {
            card.card_type == CardType::Agenda
                && source.server.is_some()
                && state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).is_some_and(|access| Some(access.server) == source.server)
        }
        (Scope::Scoring(filter), Target::Scoring { card, install }) => {
            card.card_type == CardType::Agenda
                && card_matches_filter(card, filter)
                && crate::rules::pending_choice::copy_matches(state, filter, Some(*install))
        }
        (Scope::Trashing(filter), Target::Trashing { card, .. }) => card.card_type == CardType::Resource && card_matches_filter(card, filter),
        (Scope::Ice(filter), Target::Corp { card, root: false, install, .. }) => {
            matches!(card.card_type, CardType::Ice(_))
                && card_matches_filter(card, filter)
                && crate::rules::pending_choice::copy_matches(state, filter, Some(*install))
        }
        (Scope::RootOfThisServer(filter), Target::Corp { card, server, root: true, .. }) => {
            source.server == Some(*server) && card_matches_filter(card, filter)
        }
        (Scope::IceProtectingThisServer(filter), Target::Corp { card, server, root: false, install }) => {
            source.server == Some(*server)
                && matches!(card.card_type, CardType::Ice(_))
                && card_matches_filter(card, filter)
                && crate::rules::pending_choice::copy_matches(state, filter, Some(*install))
        }
        (Scope::Rig(filter), Target::Rig { card, .. }) => card_matches_filter(card, filter),
        (Scope::RunsOnThisServer, Target::Run { server }) => source.server == Some(*server),
        (Scope::Runs(kind), Target::Run { server }) => kind.admits(*server),
        (Scope::Player(side), Target::Bound(bound)) => side == bound,
        _ => false,
    }
}

/// The sum of every applying effect `pick` reads a number off.
pub(crate) fn sum(state: &GameState, registry: &CardRegistry, target: Target<'_>, pick: fn(&ContinuousKind) -> Option<&Number>) -> i32 {
    let mut total = 0;
    for_each_applying(state, registry, target, |kind| pick(kind).is_some(), |effect, _, ctx| {
        if let Some(number) = pick(&effect.kind) {
            total += number.per * ability::resolve_amount(&number.of, ctx, state, registry) as i32;
        }
    });
    total
}

/// Whether any applying effect is one `is` accepts.
pub(crate) fn any(state: &GameState, registry: &CardRegistry, target: Target<'_>, is: impl Fn(&ContinuousKind) -> bool) -> bool {
    let mut found = false;
    for_each_applying(state, registry, target, is, |_, _, _| found = true);
    found
}

/// Whether the run against `server` may be declared successful (CR 6.9.5a):
/// no active card — and no persistent upgrade trashed this run — says runs
/// there cannot be (`ContinuousKind::CannotBeDeclaredSuccessful`), and
/// nothing used during it said this one cannot (`Prohibition::
/// DeclaredSuccessful`, Transport Monopoly).
pub(crate) fn may_be_declared_successful(state: &GameState, registry: &CardRegistry, server: ServerId) -> bool {
    !any(state, registry, Target::Run { server }, |kind| matches!(kind, ContinuousKind::CannotBeDeclaredSuccessful))
        && !cannot(state, registry, Prohibition::DeclaredSuccessful)
}

/// The most remote servers the Corp may have (A Teia: IP Recovery), or
/// `None` with no limit.
pub fn remote_server_limit(state: &GameState, registry: &CardRegistry) -> Option<u32> {
    let mut limit: Option<u32> = None;
    for_each_applying(state, registry, Target::Player(Side::Corp), |kind| matches!(kind, ContinuousKind::RemoteServerLimit(_)), |effect, _, _| {
        if let ContinuousKind::RemoteServerLimit(most) = effect.kind {
            limit = Some(limit.map_or(most, |limit| limit.min(most)));
        }
    });
    limit
}

/// Every "cannot access more than N cards other than this one" standing on
/// the run against `server` (`ContinuousKind::AccessOthersAtMost`): the
/// number, the card that says it, and its install while it is still on
/// the table. A persistent upgrade the Runner trashed this run keeps its
/// limit and has no install (CR 9.12.5).
pub(crate) fn access_limits(state: &GameState, registry: &CardRegistry, server: ServerId) -> Vec<(u32, CardId, Option<InstallId>)> {
    let mut limits = Vec::new();
    for_each_applying(state, registry, Target::Run { server }, |kind| matches!(kind, ContinuousKind::AccessOthersAtMost(_)), |effect, source, _| {
        if let ContinuousKind::AccessOthersAtMost(count) = effect.kind {
            limits.push((count, source.card.clone(), source.install));
        }
    });
    limits
}

fn strength(kind: &ContinuousKind) -> Option<&Number> {
    match kind {
        ContinuousKind::Strength(number) => Some(number),
        _ => None,
    }
}

fn install_cost(kind: &ContinuousKind) -> Option<&Number> {
    match kind {
        ContinuousKind::InstallCost(number) => Some(number),
        _ => None,
    }
}

/// Memory the Runner's active cards grant, on top of the base.
pub(crate) fn memory(state: &GameState, registry: &CardRegistry) -> i32 {
    sum(state, registry, Target::Player(Side::Runner), |kind| match kind {
        ContinuousKind::Memory(number) => Some(number),
        _ => None,
    })
}

/// What `side`'s active cards add to its maximum hand size: an identity
/// (Haas-Bioroid: Precision Design), a scored agenda (Superconducting Hub),
/// a piece of hardware (T400 Memory Diamond) — for as long as each is one.
/// And what the other side's active cards say about it (`Scope::Player`):
/// Dr. Vientiane Keeling's "The Runner gets -1 maximum hand size for each
/// hosted power counter" is the Corp's asset binding the Runner, so it is
/// asked as a prohibition is, across both tables (`Target::Bound`).
pub(crate) fn hand_size(state: &GameState, registry: &CardRegistry, side: Side) -> i32 {
    fn hand_size(kind: &ContinuousKind) -> Option<&Number> {
        match kind {
            ContinuousKind::HandSize(number) => Some(number),
            _ => None,
        }
    }
    sum(state, registry, Target::Player(side), hand_size) + sum(state, registry, Target::Bound(side), hand_size)
}

/// What `side`'s active cards add to the clicks it gains as its turn
/// begins (Basilar Synthgland 2KVJ), asked as the allotment is assigned.
pub(crate) fn allotted_clicks(state: &GameState, registry: &CardRegistry, side: Side) -> i32 {
    sum(state, registry, Target::Player(side), |kind| match kind {
        ContinuousKind::AllottedClicks(number) => Some(number),
        _ => None,
    })
}

/// The agenda points `side` needs to win right now (CR 1.7.2a, 10.3.1c):
/// the match's threshold — 7, or 6 in a starter game — and what `side`'s
/// active cards change it by (`ContinuousKind::AgendaPointsToWin`, Issuaq
/// Adaptics's "you need 1 less agenda point to win the game" for each
/// power counter it hosts). **Asked, never kept:** the win check and the
/// view put it. The bots' stage (`netrunner_bots::eval::stage`) still reads
/// the match rule: it is a reading of the board, and has no registry. Signed and unbounded below, since nothing
/// in the rules stops it: a player who needs 0 or fewer has a score that
/// meets it at the next checkpoint.
pub fn points_to_win(state: &GameState, registry: &CardRegistry, side: Side) -> i32 {
    let threshold = i32::try_from(state.rules.winning_agenda_points).unwrap_or(i32::MAX);
    threshold.saturating_add(sum(state, registry, Target::Player(side), |kind| match kind {
        ContinuousKind::AgendaPointsToWin(number) => Some(number),
        _ => None,
    }))
}

/// The Runner's link right now: what their identity prints and what their
/// active cards add (The Toolbox) for as long as each is installed. **Asked,
/// never kept** — the trace, the view and the bots' encoding all ask here.
/// It was `RunnerState::link_strength`, written once at setup from the
/// identity alone. Never below 0.
pub fn link(state: &GameState, registry: &CardRegistry) -> u32 {
    let printed = state.runner.identity.as_ref().and_then(|identity| registry.get(identity)).and_then(|definition| definition.base_link).unwrap_or(0);
    let table = sum(state, registry, Target::Player(Side::Runner), |kind| match kind {
        ContinuousKind::Link(number) => Some(number),
        _ => None,
    });
    (i64::from(printed) + i64::from(table)).max(0) as u32
}

/// A rig card's strength right now: what is stored on the install (printed,
/// plus the boosts paid for) and what the table adds. **The one number** —
/// the break contest, the view and the bots' pricing all read this, where
/// the view used to show the stored half alone.
pub fn breaker_strength(state: &GameState, registry: &CardRegistry, card: &InstalledRunnerCard) -> i32 {
    let stored = lingering::rig_strength(state, card);
    let Some(definition) = registry.get(&card.card) else { return stored };
    stored + sum(state, registry, Target::Rig { card: definition, install: card.install_id }, strength)
}

/// The strength of a piece of ice in the run right now: what it prints,
/// what the table adds — its own text first (Ice Wall's counters, Palisade's
/// remote), rezzed or not, as wherever a card speaks about itself — and what
/// is lingering on it. **The same one number** as [`breaker_strength`]: the
/// break contest, `IceEncountered`, the view and the bots' pricing all ask
/// here. It was a number stored on `RunIce` when the run's ice was built,
/// which a counter placed mid-run (Syailendra on an Ice Wall) never reached.
pub fn ice_strength(state: &GameState, registry: &CardRegistry, ice: &RunIce) -> i32 {
    let printed = registry.get(&ice.card_id).and_then(|definition| definition.strength).unwrap_or(0);
    let table = Target::corp_install(state, registry, ice.install_id).map_or(0, |target| sum(state, registry, target, strength));
    printed + table + lingering::ice_strength(state, ice.install_id)
}

/// How many more of the encountered `ice`'s printed subroutines `breaker`
/// may break this encounter (`ContinuousKind::BreakLimit`, Hammer's
/// "cannot break more than 1 of its printed subroutines except using
/// killers"), or `None` when nothing limits it: no limit stands, or every
/// one that does excepts this breaker. A click breaks as no icebreaker
/// (`None` breaker), so an exception never covers one.
///
/// A limit made for a duration (Anvil's 0 for the encounter, Unsmiling
/// Tsarevna's 1 for the run, `lingering::break_limit`) is read beside the
/// ice's own, against the same count, and excepts no breaker.
pub(crate) fn breaks_left(state: &GameState, registry: &CardRegistry, ice: &RunIce, breaker: Option<&CardDefinition>) -> Option<u32> {
    let broken = state.active_run.as_ref().map_or(0, |run| run.this_encounter.limited_breaks);
    let mut left: Option<u32> = lingering::break_limit(state, ice.install_id).map(|at_most| at_most.saturating_sub(broken));
    let Some(target) = Target::corp_install(state, registry, ice.install_id) else { return left };
    for_each_applying(state, registry, target, |kind| matches!(kind, ContinuousKind::BreakLimit { .. }), |effect, _, _| {
        if let ContinuousKind::BreakLimit { at_most, except_using } = effect.kind {
            let excepted = except_using.is_some_and(|subtype| breaker.is_some_and(|def| def.subtypes.contains(&subtype)));
            if !excepted {
                let room = at_most.saturating_sub(broken);
                left = Some(left.map_or(room, |other| other.min(room)));
            }
        }
    });
    left
}

/// The subroutines the ice `install` gains by its own static ability
/// (`ContinuousKind::Subroutines`, Echo's and Envelopment's), as copies to
/// put before its printed ones (CR 9.8.3b) and after them (9.8.3d), each
/// count read as the ice right now. Within a category every copy is the
/// same subroutine, so the order among them is nothing a player can see.
pub(crate) fn own_subroutines(state: &GameState, registry: &CardRegistry, install: InstallId) -> (Vec<crate::dsl::SubroutineDef>, Vec<crate::dsl::SubroutineDef>) {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    let Some(target) = Target::corp_install(state, registry, install) else { return (before, after) };
    for_each_applying(state, registry, target, |kind| matches!(kind, ContinuousKind::Subroutines { .. }), |effect, _, ctx| {
        if let ContinuousKind::Subroutines { subroutine, count, before: first } = &effect.kind {
            let copies = (count.per * ability::resolve_amount(&count.of, ctx, state, registry) as i32).max(0) as usize;
            let into = if *first { &mut before } else { &mut after };
            into.extend(std::iter::repeat_n((**subroutine).clone(), copies));
        }
    });
    (before, after)
}

/// Whether a Runner card's abilities may break subroutines on the ice
/// `ice` (`Prohibition::BreakSubroutinesOnIce`, Trieste Model Bioroids'
/// "Runner card abilities cannot break subroutines on the chosen ice").
/// The choice is remembered on the lingering list for as long as Trieste
/// is rezzed, but the sentence that reads it is Trieste's static ability,
/// and a Trieste that has lost its abilities (Light the Fire! on its
/// server, CR 9.1.9a) says nothing: so an entry whose source has lost them
/// does not hold here while the loss does.
pub(crate) fn runner_cards_may_break(state: &GameState, registry: &CardRegistry, ice: InstallId) -> bool {
    !state.lingering.iter().any(|effect| {
        effect.what == lingering::Lingering::Cannot(Prohibition::BreakSubroutinesOnIce)
            && effect.on == lingering::On::Install(ice)
            && effect.holds(state)
            && !matches!(effect.until, lingering::Until::WhileRezzed(source) if active::lost_abilities(state, registry, source))
    })
}

/// Whether the Corp may still trash an installed Runner card with the text
/// of `install` (`ContinuousKind::TrashLimit`, Sorocaban Blade's "you
/// cannot trash more than 1 installed Runner card with this ice during
/// each encounter"): always, unless `install` is the ice being encountered
/// and its limit is spent. The count is of this encounter, so the limit is
/// about nothing outside one.
pub(crate) fn may_trash_with(state: &GameState, registry: &CardRegistry, install: Option<InstallId>) -> bool {
    let Some(run) = state.active_run.as_ref() else { return true };
    let encountered = run.phase == crate::rules::run::RunPhase::EncounterIce && run.ice.get(run.position).is_some_and(|ice| Some(ice.install_id) == install);
    let Some(target) = install.filter(|_| encountered).and_then(|install| Target::corp_install(state, registry, install)) else { return true };
    let trashed = run.this_encounter.runner_cards_trashed;
    !any(state, registry, target, |kind| matches!(kind, ContinuousKind::TrashLimit(at_most) if trashed >= *at_most))
}

/// Counts an installed Runner card trashed by the text of `install`, when
/// that is the ice being encountered — what `may_trash_with` reads.
pub(crate) fn note_runner_card_trashed_by(state: &mut GameState, install: Option<InstallId>) {
    let Some(run) = state.active_run.as_mut() else { return };
    let encountered = run.phase == crate::rules::run::RunPhase::EncounterIce && run.ice.get(run.position).is_some_and(|ice| Some(ice.install_id) == install);
    if encountered {
        run.this_encounter.runner_cards_trashed += 1;
    }
}

/// Whether a boost to the icebreaker `install` lasts the run rather than the
/// encounter.
pub(crate) fn boosts_last_the_run(state: &GameState, registry: &CardRegistry, install: InstallId) -> bool {
    let Some(card) = state.runner.rig.iter().find(|card| card.install_id == install).and_then(|card| registry.get(&card.card)) else { return false };
    any(state, registry, Target::Rig { card, install }, |kind| matches!(kind, ContinuousKind::BoostsLastTheRun))
}

/// Whether `card`, being accessed now, is revealed while it is (CR 1.21.7:
/// Esca's, Snare!'s and Byte!'s "while the Runner is accessing this asset
/// in R&D, they must reveal it"). Asked of the card as it sits in its zone:
/// what a card says about itself applies wherever it is.
pub(crate) fn revealed_while_accessed(state: &GameState, registry: &CardRegistry, card: &CardId) -> bool {
    let Some(definition) = registry.get(card) else { return false };
    any(state, registry, Target::Card(definition), |kind| matches!(kind, ContinuousKind::RevealedWhileAccessed))
}

/// Whether the ice `install` has `subtype` beyond the one type a run's ice
/// carries (`RunIce::ice_type`), which the caller already knows: a second
/// type it prints (Hafrún's "Barrier - Code Gate", `CardDefinition::
/// is_ice_of_type`), one the table gives it (a declared `GainSubtype`), or
/// one from a choice that is still holding (Lycian Multi-Munition's,
/// `lingering::gains_subtype`). Every reader asks "the type it carries, or
/// this", so the printed second type is here rather than at each of them.
pub fn ice_gains_subtype(state: &GameState, registry: &CardRegistry, install: InstallId, subtype: IceType) -> bool {
    if crate::rules::lingering::gains_subtype(state, install, subtype) {
        return true;
    }
    let Some(target) = Target::corp_install(state, registry, install) else { return false };
    if target.card().is_some_and(|card| card.is_ice_of_type(subtype)) {
        return true;
    }
    any(state, registry, target, |kind| *kind == ContinuousKind::GainSubtype(subtype))
}

/// Every type the run's ice `ice` has now: what it prints, unless that is
/// none of the three, and each of the three it has gained. The one list a
/// pass is recorded with (`GameEvent::IcePassed::rezzed_as`).
pub fn ice_types(state: &GameState, registry: &CardRegistry, ice: &crate::rules::run::RunIce) -> Vec<IceType> {
    [IceType::Barrier, IceType::CodeGate, IceType::Sentry]
        .into_iter()
        .filter(|subtype| ice.ice_type == *subtype || ice_gains_subtype(state, registry, ice.install_id, *subtype))
        .collect()
}

/// What installing `card` costs the Runner right now. The one question,
/// for a price shown and for the install that pays it: "the first program
/// you install each turn" is read off the turn
/// (`ContinuousEffect::first_each_turn`), so there is nothing to spend.
/// There was a `pay_install_cost_of` beside this — a second scan at the
/// real install that spent each discount's `OncePerTurn` — and a discount
/// that was a use of the card is what let a DZMZ Optimizer installed after
/// the turn's first program lower the second. Public for the bots, whose
/// reading of a held card's price is this one (Kate "Mac" McCaffrey's
/// discount, Phase 5 §37).
pub fn install_cost_of(state: &GameState, registry: &CardRegistry, card: &CardDefinition) -> u32 {
    install_cost_onto(state, registry, card, None)
}

/// `install_cost_of`, for an install onto the rig card `host` when there is
/// one: the same question, asked of a target that knows where the card is
/// going, so a host's own discount (Hackerspace's "each resource installed
/// this way costs 1[credit] less") is in it and every other is too.
pub(crate) fn install_cost_onto(state: &GameState, registry: &CardRegistry, card: &CardDefinition, host: Option<InstallId>) -> u32 {
    let target = match host {
        Some(host) => Target::InstallingOnto { card, host },
        None => Target::Card(card),
    };
    (card.cost as i32 + sum(state, registry, target, install_cost)).max(0) as u32
}

/// Whether `card` may be installed onto the rig card `host` — a host that
/// says so of it (`ContinuousKind::MayHost`, Hackerspace). The one
/// question, for the action list and for the install that is refused
/// without it.
pub(crate) fn may_install_onto(state: &GameState, registry: &CardRegistry, card: &CardDefinition, host: InstallId) -> bool {
    any(state, registry, Target::InstallingOnto { card, host }, |kind| matches!(kind, ContinuousKind::MayHost))
}


/// What playing the event or operation `card` costs in credits right now:
/// its printed play cost and what the table adds (Tailgate's own "lowered
/// by 1[credit] for each piece of ice protecting HQ"), never below 0. The
/// one question, for the play and for the offer, as `install_cost_of` is
/// for an install.
pub(crate) fn play_cost_of(state: &GameState, registry: &CardRegistry, card: &CardDefinition) -> u32 {
    (card.cost as i32 + sum(state, registry, Target::Card(card), |kind| match kind {
        ContinuousKind::PlayCost(number) => Some(number),
        _ => None,
    }))
    .max(0) as u32
}

/// The additional cost of playing `card` right now: its printed
/// `additional_play_cost`, less the clicks the table takes off it
/// (Synchrocyclotron). `None` when nothing is left to pay. A click
/// discount lowers only the Double's additional click — the only click a
/// card in the pool lowers — never the action's own.
pub(crate) fn additional_play_cost_of(state: &GameState, registry: &CardRegistry, card: &CardDefinition) -> Option<Cost> {
    let printed = card.additional_play_cost.clone()?;
    let Cost::Clicks(clicks) = printed else { return Some(printed) };
    let delta = sum(state, registry, Target::Card(card), |kind| match kind {
        ContinuousKind::PlayClicks(number) => Some(number),
        _ => None,
    });
    let clicks = (clicks as i32 + delta).max(0) as u32;
    (clicks > 0).then_some(Cost::Clicks(clicks))
}

/// What stealing the agenda `card` costs the Runner, if anything: its
/// printed cost to steal and what the table adds (Magistrate Revontulet's
/// 3[c]), one price paid together (CR 1.16.10b), either of which lets the
/// Runner decline (1.17.3d). The one question the access asks and a bot
/// pricing a run asks: the heuristic Runner counted a faceup agenda in
/// Archives as a steal it could not miss, ran it four times a turn with
/// no credits under a rezzed Magistrate, and never clicked for one.
pub fn steal_price(state: &GameState, registry: &CardRegistry, card: &CardDefinition) -> Option<Cost> {
    let added = if card.agenda_points.is_some() { steal_costs_added(state, registry, card) } else { Vec::new() };
    let mut parts: Vec<Cost> = card.steal_cost.clone().into_iter().chain(added).collect();
    match parts.len() {
        0 => None,
        1 => parts.pop(),
        _ => Some(Cost::AllOf(parts)),
    }
}

/// The costs the table adds to stealing the agenda `card` (Magistrate
/// Revontulet's credits, Daniela Jorge Inácio's grip cards), in the order
/// their cards are asked. A number of credits the state decides (NAPD
/// Cordon's "plus 2[credit] for each advancement counter on that agenda")
/// is reckoned here, as the agenda is offered (CR 1.16.2b), so the offer
/// the Runner reads is the price they pay.
pub(crate) fn steal_costs_added(state: &GameState, registry: &CardRegistry, card: &CardDefinition) -> Vec<Cost> {
    let mut costs = Vec::new();
    for_each_applying(state, registry, Target::Card(card), |kind| matches!(kind, ContinuousKind::StealCost(_)), |effect, _, ctx| {
        if let ContinuousKind::StealCost(cost) = &effect.kind {
            costs.push(match cost {
                Cost::CreditsAmount(amount) => Cost::Credits(ability::resolve_amount(amount, ctx, state, registry)),
                other => other.clone(),
            });
        }
    });
    costs
}

/// The additional costs to run `server` (CR 6.3.2b: Earth Station: SEA
/// Headquarters), in the order their cards are asked, paid together as the
/// server is announced (`run::start_run`).
pub(crate) fn run_costs(state: &GameState, registry: &CardRegistry, server: ServerId) -> Vec<Cost> {
    let mut costs = Vec::new();
    for_each_applying(state, registry, Target::Run { server }, |kind| matches!(kind, ContinuousKind::RunCost(_)), |effect, _, _| {
        if let ContinuousKind::RunCost(cost) = &effect.kind {
            costs.push(cost.clone());
        }
    });
    costs
}

/// What the Runner must pay beside the trash cost to trash the card they
/// are accessing (Daniela Jorge Inácio), or `None`.
pub(crate) fn additional_trash_cost(state: &GameState, registry: &CardRegistry, card: &CardDefinition) -> Option<Cost> {
    let mut costs = Vec::new();
    for_each_applying(state, registry, Target::Card(card), |kind| matches!(kind, ContinuousKind::AdditionalTrashCost(_)), |effect, _, _| {
        if let ContinuousKind::AdditionalTrashCost(cost) = &effect.kind {
            costs.push(cost.clone());
        }
    });
    match costs.len() {
        0 => None,
        1 => costs.pop(),
        _ => Some(Cost::AllOf(costs)),
    }
}

/// Every additional cost to score the Corp install `install` (Word on the
/// Street), each with the card that imposes it and that card's install:
/// the payer is the Corp, and a cost like "add this resource to their score
/// area" is about the card that prints it (CR 1.16.10).
pub(crate) fn score_costs(state: &GameState, registry: &CardRegistry, install: InstallId) -> Vec<(Cost, CardId, Option<InstallId>)> {
    let Some(installed) = state.find_corp_install(install) else { return Vec::new() };
    let Some(card) = registry.get(&installed.card) else { return Vec::new() };
    let mut costs = Vec::new();
    for_each_applying(state, registry, Target::Scoring { card, install }, |kind| matches!(kind, ContinuousKind::ScoreCost(_)), |effect, source, _| {
        if let ContinuousKind::ScoreCost(cost) = &effect.kind {
            costs.push((cost.clone(), source.card.clone(), source.install));
        }
    });
    costs
}

/// Every additional cost to trash the rig install `install` with the basic
/// action (Sebastião Souza Pessoa, Manuel Lattes de Moura), each with the
/// card that imposes it and that card's install, as [`score_costs`] has
/// them: the payer is the Corp.
pub(crate) fn basic_trash_costs(state: &GameState, registry: &CardRegistry, install: InstallId) -> Vec<(Cost, CardId, Option<InstallId>)> {
    let Some(installed) = state.find_rig_install(install) else { return Vec::new() };
    let Some(card) = registry.get(&installed.card) else { return Vec::new() };
    let mut costs = Vec::new();
    for_each_applying(state, registry, Target::Trashing { card, install }, |kind| matches!(kind, ContinuousKind::BasicTrashCost(_)), |effect, source, _| {
        if let ContinuousKind::BasicTrashCost(cost) = &effect.kind {
            costs.push((cost.clone(), source.card.clone(), source.install));
        }
    });
    costs
}

/// The agenda points the agenda `card` is worth in `side`'s score area:
/// what it prints and what its own text changes there (Let Them Dream),
/// never below 0. `install` is the copy, whose own counters a `while` may
/// read (Megaprix Qualifier, Project Vacheron); `None` asks of the card
/// alone, as an agenda about to land is asked.
pub fn agenda_points_in(state: &GameState, registry: &CardRegistry, card: &CardDefinition, side: Side, install: Option<InstallId>) -> u32 {
    let printed = card.agenda_points.unwrap_or(0) as i32;
    (printed + sum(state, registry, Target::Scored { card, side, install }, |kind| match kind {
        ContinuousKind::AgendaPoints(number) => Some(number),
        _ => None,
    }))
    .max(0) as u32
}

/// The advancement requirement of the Corp install `install` right now:
/// what it prints and what its own text changes (Ontological Dependence,
/// `ContinuousKind::AdvancementRequirement`), `None` for a card that prints
/// none. Signed and not floored (CR 1.1.3): a requirement of −1 is met by
/// no counters at all, which is what CR 1.17.3a's "greater than or equal
/// to" says. The one question the score, its dividends and the view ask,
/// so none of them reads the printed number where the table has moved it.
pub fn advancement_requirement(state: &GameState, registry: &CardRegistry, install: InstallId) -> Option<i32> {
    let target = Target::corp_install(state, registry, install)?;
    let printed = target.card()?.advancement_requirement? as i32;
    Some(printed + sum(state, registry, target, |kind| match kind {
        ContinuousKind::AdvancementRequirement(number) => Some(number),
        _ => None,
    }))
}

/// What is added to the printed cost of rezzing the Corp install
/// `install`: what the table adds while it stands (Fransofia Ward's
/// "+1[c] to rez each piece of ice") and what is lingering (Tread Lightly's
/// "+3[c] during that run") — the same two halves as [`ice_strength`].
///
/// The lingering half is about **ice**, as the card says. It was a number
/// on the run, added by `engine::rez_price` to anything rezzed in the
/// attacked server — so an asset rezzed in that server's root during a
/// Tread Lightly run cost 3 more than it prints.
pub(crate) fn rez_cost_delta(state: &GameState, registry: &CardRegistry, install: InstallId) -> i32 {
    let Some(target) = Target::corp_install(state, registry, install) else { return 0 };
    let is_ice = matches!(target, Target::Corp { card, .. } if matches!(card.card_type, CardType::Ice(_)));
    let table = sum(state, registry, target, |kind| match kind {
        ContinuousKind::RezCost(number) => Some(number),
        _ => None,
    });
    table + if is_ice { lingering::ice_rez_cost(state) } else { 0 }
}

/// Whether something a player would do is prohibited right now — the one
/// predicate the guard in `apply_action` and the action list both ask, so
/// the two cannot disagree.
///
/// Every prohibition in the pool is *lingering* — made by something that
/// resolved, for a duration (`Effect::Prohibit`) — so this reads
/// `rules::lingering` and the registry goes unused. A card that prints a
/// standing one ("while this is rezzed, the Runner cannot…") is a
/// `ContinuousKind` no card needs yet (named on that enum), and this is
/// where its scan joins: the callers already ask here.
pub fn cannot(state: &GameState, registry: &CardRegistry, what: Prohibition) -> bool {
    lingering::prohibits(state, what) || !standing_prohibitions(state, registry, what).is_empty()
}

/// The active cards whose standing `Cannot(what)` is on right now — Attini
/// while its subroutines resolve at threat 3 — as a view carries them
/// (`ClientView::standing_cannot`), so a client can say which card forbids
/// it. Beside the lingering list, never on it: nothing resolved to make
/// it, and it ends when its `while` stops holding, with nothing to sweep.
pub fn standing_prohibitions<'a>(state: &'a GameState, registry: &'a CardRegistry, what: Prohibition) -> Vec<&'a CardId> {
    let mut found = Vec::new();
    for_each_applying(state, registry, Target::Bound(what.binds()), |kind| *kind == ContinuousKind::Cannot(what), |_, source, _| {
        if !found.contains(&source.card) {
            found.push(source.card);
        }
    });
    found
}

/// [`cannot`] about one card: also what binds only copies of it (Perfect
/// Recall). An access asks this, of the card accessed. A steal or trash of
/// an agenda is also what `StealOrTrashAgendas` forbids (Pinhole
/// Threading), so the four sites that ask about a steal or a trash need no
/// second question.
pub fn cannot_about(state: &GameState, registry: &CardRegistry, what: Prohibition, card: &CardId) -> bool {
    lingering::prohibits_about(state, what, card)
        || (what == Prohibition::StealOrTrash
            && registry.get(card).is_some_and(|definition| definition.card_type == crate::dsl::CardType::Agenda)
            && lingering::prohibits_about(state, Prohibition::StealOrTrashAgendas, card))
}

/// [`cannot`] about one install: also what binds only it (Warm
/// Reception). A score asks this, of the agenda scored.
pub fn cannot_install(state: &GameState, _registry: &CardRegistry, what: Prohibition, install: InstallId) -> bool {
    lingering::prohibits_install(state, what, install)
}

/// What the table adds to the cost of trashing the accessed Corp card
/// `card`: what is said about the install, when it is one (`install` —
/// Mahkota Langit Grid's root), and what is said about any card accessed
/// (`Scope::Accessing` — Demolisher's "each Corp card"), wherever it is.
pub(crate) fn trash_cost_delta(state: &GameState, registry: &CardRegistry, card: &CardDefinition, install: Option<InstallId>) -> i32 {
    fn trash_cost(kind: &ContinuousKind) -> Option<&Number> {
        match kind {
            ContinuousKind::TrashCost(number) => Some(number),
            _ => None,
        }
    }
    let installed = install.and_then(|install| Target::corp_install(state, registry, install)).map_or(0, |target| sum(state, registry, target, trash_cost));
    let mut accessed = 0;
    for_each_applying(state, registry, Target::Card(card), |kind| trash_cost(kind).is_some(), |effect, _, ctx| {
        if let (Scope::Accessing(_), Some(number)) = (&effect.applies_to, trash_cost(&effect.kind)) {
            accessed += number.per * ability::resolve_amount(&number.of, ctx, state, registry) as i32;
        }
    });
    installed + accessed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{Amount, CardFilter};
    use crate::rules::run::RunState;
    use crate::rules::state::{GamePhase, InstalledCard};

    fn flat(per: i32) -> Number {
        Number { per, of: Amount::Fixed(1) }
    }

    fn prints(id: &str, side: Side, card_type: CardType, kind: ContinuousKind, applies_to: Scope) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type,
            continuous: vec![ContinuousEffect { kind, applies_to, condition: None, first_each_turn: false, text: None }],
            ..Default::default()
        }
    }

    fn blank(id: &str, side: Side, card_type: CardType, cost: u32) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), title: id.to_string(), side, card_type, cost, ..Default::default() }
    }

    fn in_the_rig(id: &str, install: u32) -> InstalledRunnerCard {
        InstalledRunnerCard { install_id: InstallId(install), card: CardId(id.to_string()), ..Default::default() }
    }

    fn on_the_table(id: &str, install: u32, server: ServerId, slot: InstallSlot, rezzed: bool) -> InstalledCard {
        InstalledCard { install_id: InstallId(install), card: CardId(id.to_string()), server, slot, rezzed, ..Default::default() }
    }

    /// A question reads only effects of its own kind. Boto's "Threat 4 →
    /// this ice gets +2 strength" sat in a score area in a bot's sample (a
    /// masked access the sample named wrongly, then stole), and the score
    /// asked that effect whether threat was 4 — which is the score — before
    /// finding it was not about agenda points: `bench --bots planner,mcts
    /// --pairing mcts/planner --games 192 --seed 2` aborted with a stack
    /// overflow. The card counts nothing there and the question returns;
    /// on the table the same text still reads the threat.
    #[test]
    fn a_score_never_asks_an_effect_that_is_not_about_agenda_points_whether_it_is_on() {
        let mut wall = prints("threat_wall", Side::Corp, CardType::Ice(IceType::Barrier), ContinuousKind::Strength(flat(2)), Scope::This);
        wall.strength = Some(4);
        wall.continuous[0].condition = Some(crate::dsl::EffectRequirement::AmountAtLeast(Amount::ThreatLevel, 4));
        let mut agenda = blank("four_points", Side::Corp, CardType::Agenda, 0);
        agenda.agenda_points = Some(4);
        let registry = CardRegistry::from_cards(vec![wall.clone(), agenda.clone()]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.runner.scored_agendas =
            vec![crate::rules::ScoredAgenda::plain(agenda.id.clone()), crate::rules::ScoredAgenda::plain(wall.id.clone())];

        assert_eq!(agenda_points_in(&state, &registry, &wall, Side::Runner, None), 0, "not an agenda: worth nothing, and asked nothing");
        assert_eq!(crate::rules::win::score(&state, &registry, Side::Runner), 4);

        state.corp.installed = vec![on_the_table("threat_wall", 1, ServerId::Hq, InstallSlot::Ice, true)];
        let target = Target::corp_install(&state, &registry, InstallId(1)).unwrap();
        assert_eq!(sum(&state, &registry, target, strength), 2, "and its own question still reads the threat");
    }

    /// The Listener Rule's sentence, for standing effects: what a card says
    /// about itself applies from the grip, where it is not active, and what
    /// it says about anything else does not.
    #[test]
    fn a_cards_own_text_applies_wherever_it_is_and_nothing_else_of_it_does() {
        let mut cheap = prints("cheap", Side::Runner, CardType::Program, ContinuousKind::InstallCost(flat(-2)), Scope::This);
        cheap.cost = 5;
        let mut generous =
            prints("generous", Side::Runner, CardType::Hardware, ContinuousKind::InstallCost(flat(-1)), Scope::Installing(CardFilter::Any));
        generous.cost = 3;
        let registry = CardRegistry::from_cards(vec![cheap.clone(), generous.clone()]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.runner.grip = vec![cheap.id.clone(), generous.id.clone()];

        assert_eq!(install_cost_of(&state, &registry, &cheap), 3, "its own discount, from the grip");
        assert_eq!(install_cost_of(&state, &registry, &generous), 3, "a card in the grip discounts nothing else — itself included");

        state.runner.rig = vec![in_the_rig("generous", 1)];
        assert_eq!(install_cost_of(&state, &registry, &cheap), 2, "installed, it is active");
    }

    /// A facedown upgrade is not active; a rezzed one reaches its own root
    /// and no other; and a persistent one the Runner trashed this run still
    /// reaches the server being run.
    #[test]
    fn an_upgrade_reaches_its_own_root_while_rezzed_and_for_the_run_it_was_trashed_in() {
        let grid =
            prints("grid", Side::Corp, CardType::Upgrade, ContinuousKind::TrashCost(flat(2)), Scope::RootOfThisServer(CardFilter::CardType(CardType::Asset)));
        let registry = CardRegistry::from_cards(vec![blank("asset", Side::Corp, CardType::Asset, 0), grid]);
        let mut state = GameState::default();
        state.corp.installed = vec![
            on_the_table("grid", 1, ServerId::Remote(0), InstallSlot::Root, false),
            on_the_table("asset", 2, ServerId::Remote(0), InstallSlot::Root, false),
            on_the_table("asset", 3, ServerId::Remote(1), InstallSlot::Root, false),
        ];
        let (beside_it, elsewhere) = (InstallId(2), InstallId(3));
        let asset = registry.get(&CardId("asset".to_string())).expect("registered");

        assert_eq!(trash_cost_delta(&state, &registry, asset, Some(beside_it)), 0, "facedown");
        state.corp.installed[0].rezzed = true;
        assert_eq!(trash_cost_delta(&state, &registry, asset, Some(beside_it)), 2);
        assert_eq!(trash_cost_delta(&state, &registry, asset, Some(elsewhere)), 0, "another server's root");

        state.corp.installed.remove(0);
        assert_eq!(trash_cost_delta(&state, &registry, asset, Some(beside_it)), 0, "gone, and no run to persist through");
        state.active_run =
            Some(RunState { server: ServerId::Remote(0), persistent_trashed_upgrades: vec![CardId("grid".to_string())], ..Default::default() });
        assert_eq!(trash_cost_delta(&state, &registry, asset, Some(beside_it)), 2, "persistent, for the remainder of the run");
        assert_eq!(trash_cost_delta(&state, &registry, asset, Some(elsewhere)), 0);
    }

    /// `Scope::Host` is a relation between two installs, not two cards: the
    /// second copy of the host gets nothing.
    #[test]
    fn a_hosted_card_reaches_the_install_it_is_hosted_on_and_not_its_twin() {
        let mut breaker = blank("breaker", Side::Runner, CardType::Program, 0);
        breaker.strength = Some(1);
        let pad = prints("pad", Side::Runner, CardType::Hardware, ContinuousKind::Strength(flat(1)), Scope::Host);
        let registry = CardRegistry::from_cards(vec![breaker, pad]);
        let mut state = GameState::default();
        let strong = |install: u32| InstalledRunnerCard { base_strength: 1, ..in_the_rig("breaker", install) };
        state.runner.rig = vec![strong(1), strong(2), InstalledRunnerCard { hosted_on_rig_card: Some(InstallId(2)), ..in_the_rig("pad", 3) }];

        assert_eq!(breaker_strength(&state, &registry, &state.runner.rig[0]), 1);
        assert_eq!(breaker_strength(&state, &registry, &state.runner.rig[1]), 2);
        assert!(!boosts_last_the_run(&state, &registry, InstallId(2)), "a kind nobody prints is not found");
    }

    /// Two copies of "the first program you install each turn costs 1[c]
    /// less" are two abilities about one fact: the turn's first program
    /// takes both, and the next pays in full because it is not the first.
    /// The field this replaced returned the first source it found; the
    /// `OncePerTurn` that replaced the field gave each copy a use of its
    /// own, which a copy installed mid-turn still had.
    #[test]
    fn two_copies_of_a_first_install_discount_stack_and_neither_outlives_the_first_install() {
        let mut optimizer =
            prints("optimizer", Side::Runner, CardType::Hardware, ContinuousKind::InstallCost(flat(-1)), Scope::Installing(CardFilter::CardType(CardType::Program)));
        optimizer.continuous[0].first_each_turn = true;
        let program = blank("program", Side::Runner, CardType::Program, 3);
        let hardware = blank("hardware", Side::Runner, CardType::Hardware, 3);
        let registry = CardRegistry::from_cards(vec![optimizer, program.clone(), hardware.clone()]);
        let mut state = GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() };
        state.runner.rig = vec![in_the_rig("optimizer", 1), in_the_rig("optimizer", 2)];

        assert_eq!(install_cost_of(&state, &registry, &hardware), 3, "not a program");
        assert_eq!(install_cost_of(&state, &registry, &program), 1);
        assert_eq!(install_cost_of(&state, &registry, &program), 1, "asking spends nothing");
        let installed = crate::rules::GameEvent::HardwareInstalled { side: Side::Runner, card: CardId("hardware".to_string()), credits_paid: 3 };
        crate::rules::turn_log::record(&mut state, &registry, &installed);
        assert_eq!(install_cost_of(&state, &registry, &program), 1, "a piece of hardware is not the turn's first program");
        let installed = crate::rules::GameEvent::ProgramInstalled { side: Side::Runner, card: CardId("program".to_string()), memory_cost: 1, credits_paid: 1 };
        crate::rules::turn_log::record(&mut state, &registry, &installed);
        assert_eq!(install_cost_of(&state, &registry, &program), 3, "the turn has had its first program");
    }
}
