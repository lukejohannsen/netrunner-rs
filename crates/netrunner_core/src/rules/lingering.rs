//! Effects with a duration — "+2 strength for the remainder of this run",
//! "-1 strength for the remainder of this encounter", "you cannot score
//! agendas for the remainder of the turn".
//!
//! A continuous effect (`dsl::continuous`) is declared on a card and holds
//! for as long as the card is active, so it is scanned and never stored. A
//! *lingering* effect is created once, by something that resolved, and
//! outlives it: the card that made it can leave play and the effect stays.
//! It has to be on `GameState` — it survives a parked decision, which is the
//! State Hygiene Rule's test — and it is **resolved when it is created**:
//! a flat number or a prohibition, about an install, each piece of ice or a
//! player ([`On`]), with no `Amount` and no `Box`, so a search clone copies
//! a few words and an empty list allocates nothing.
//!
//! There were three of these lists already, spelled as fields:
//! `encounter_strength_buff`, `run_strength_buff` and `turn_strength_buff`
//! on every rig card, each with a `reset_*` that five call sites had to
//! remember (Rules Audit T8 was one that forgot: pumps carried into the
//! next run). And one that was not a list at all: `Effect::ModifyStrength`
//! wrote Leech's -1 into `RunIce::current_strength` (a field that is gone:
//! an ice's strength is `continuous::ice_strength`, asked of the table),
//! where nothing could take it back out, so "for the remainder of this
//! encounter" lasted the run. And three that were flags: the Corp's score
//! lock, the Runner's steal-or-trash bar and the run's rez-cost modifier
//! ([`Lingering::Cannot`], [`Lingering::RezCost`]), two of them on fields no
//! view carried, so no bot sample was bound by what the game was.
//!
//! **Whether an entry still holds is a question about the state, asked at
//! every read** ([`LingeringEffect::holds`]): the run it was made in is
//! over, the encounter it was made in is not the one happening. So nothing
//! depends on *when* the list is swept — `checkpoint::expire_durations`
//! sweeps it at every checkpoint and `run::engine::end_run` when a run is
//! taken down, and both are garbage collection. The one thing a derived
//! answer cannot tell apart is two runs, which is why `end_run` sweeps.

use serde::{Deserialize, Serialize};

use crate::dsl::{CardId, EffectDuration, EndRunPrevention, Prohibition};
use crate::rules::run::RunPhase;
use crate::rules::state::{GameState, InstallId, InstalledRunnerCard};
use crate::rules::{RulesError, Side};

/// A delayed conditional ability (CR 9.6.13): an ability a resolution
/// leaves waiting for a moment, which resolves the next time that moment
/// comes and then expires (9.6.13c) — Lightning Laboratory's "When this
/// turn ends, derez 2 pieces of ice protecting that server", made by
/// `Effect::WhenThisTurnEnds`. Kept beside the lingering effects because
/// the rules keep it there (9.6.13: "maintained by a lingering effect"),
/// and like them it is made once, outlives what made it, and survives any
/// parked decision.
///
/// **It is heard like a card's trigger** (`listeners::plan_for`): its
/// resolution is one of the moment's reactions, its controller's to order
/// among the others, and it is taken off this list when it is planned, so
/// it resolves once. `turn` dates it: "this turn" is `GameState::turn` as it
/// was made, and one left from an earlier turn is never heard. Its effect
/// has already had what it named written in — "that server" is the
/// attacked server, `Effect::with_attacked_server` — because the run is
/// over by the time it resolves. Public, as the card that made it was
/// seen to resolve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelayedAbility {
    /// The moment it waits for: `Trigger::OnDiscardPhaseEnd` for "when this
    /// turn ends" (CR 5.6.3d: the turn and the discard phase end at one
    /// step).
    pub when: crate::dsl::Trigger,
    pub turn: u32,
    pub effect: crate::dsl::Effect,
    /// What resolves it, as the card that made it would.
    pub card: CardId,
    pub install: Option<InstallId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LingeringEffect {
    pub what: Lingering,
    pub on: On,
    pub until: Until,
    /// The card whose text made it, for whoever shows it. Public to both
    /// players: every source is a card both watched resolve.
    pub source: CardId,
}

/// What it is about. Only what a card in the pool says for a duration.
///
/// There is no `Server`, though the plan for this list had one for Tread
/// Lightly: the card says "the rez cost of **each piece of ice**", and a
/// server named when the run began is the wrong server once the run is
/// redirected (`RunState::redirect_on_approach`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum On {
    /// One installed card — a rig card or a piece of ice; install handles
    /// are one sequence, so the handle says which.
    Install(InstallId),
    /// Each piece of ice, wherever it is installed.
    EachIce,
    /// A player: who a prohibition binds.
    Player(Side),
    /// Every copy of a card: a prohibition that binds its player only about
    /// those — Perfect Recall's "the Runner cannot steal or trash copies of
    /// that card for the remainder of this run". A copy is the same card
    /// by id; a revealed card, so the id rides in a view unmasked.
    CopiesOf(CardId),
    /// Every card in the root of the attacked server, read whenever it is
    /// asked (`active_run.server`) — Light the Fire!'s "cards in the root of
    /// the attacked server lose all abilities". The server is not fixed when
    /// the effect is made, which is the reason there is no `Server`: a
    /// redirect moves it. Nothing outside a run.
    RootOfAttackedServer,
}

/// What changes. Only what a card in the pool does for a duration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lingering {
    Strength(i32),
    /// Tread Lightly's "During that run, the rez cost of each piece of ice
    /// is increased by 3[credit]". Was `RunState::ice_rez_cost_modifier`,
    /// which no view carried — every bot sample priced the rez 3 short —
    /// and which `engine::rez_price` added to whatever was being rezzed in
    /// the attacked server, an asset in its root included.
    RezCost(i32),
    /// Was a flag per prohibition, each with its own reset:
    /// `CorpState::cannot_score_agendas_this_turn` (cleared by the turn
    /// code, carried by no view) and `RunState::runner_cannot_steal_or_trash`.
    Cannot(Prohibition),
    /// Shred's "The first time the Corp would end that run, prevent the run
    /// from ending unless…" — a prevention that stands for a duration
    /// rather than one a player uses, so it is here and not an interrupt
    /// (`rules::prevention::run_ending` asks it and takes it: "the first
    /// time" is the one use it has). Was `RunState::end_run_prevention`, a
    /// third field no view carried: every bot sample taken during a Shred
    /// run believed the next "End the run" would end it.
    PreventRunEnding(EndRunPrevention),
    /// A change to a player's click allotment for their next turn —
    /// Aggressive Trendsetting's "+1 allotted [click] for your next turn",
    /// Caveat Emptor's "−1 allotted [click] for their next turn" — taken
    /// by `turn::enter_start_of_turn` as it assigns the allotment
    /// ([`take_allotted_clicks`]). Was `CorpState::extra_clicks_next_turn`,
    /// a field no view carried.
    AllottedClicks(i32),
    /// A piece of ice has this subtype on top of what it prints — Lycian
    /// Multi-Munition's "choose 1 or more subtypes among barrier, code
    /// gate, and sentry. This ice gains the chosen subtypes while it
    /// remains rezzed", one entry per subtype chosen, made by
    /// `Effect::GainIceSubtype` and read with the table's by
    /// `continuous::ice_gains_subtype`, so a typed breaker, a "the barrier
    /// you are encountering" and a pass of "a rezzed code gate or sentry"
    /// all see it. Lingering rather than continuous: the subtype was
    /// *chosen*, once, and a declared effect has nowhere to keep a choice.
    GainSubtype(crate::dsl::IceType),
    /// The Runner's mark (CR 10.11): a server designated by "identify your
    /// mark" (`Effect::IdentifyMark`), "treated as a lingering effect that
    /// expires at the end of the turn" (10.11.4), so it is one — about the
    /// Runner, until `Until::EndOfTurn` — and never a field with a reset.
    /// There is only ever one (10.11.1a), asked with [`mark`]. Public: the
    /// rules give it no owner to hide it from, and both players saw the
    /// random central server chosen.
    Mark(crate::rules::ServerId),
    /// The install loses all its abilities (CR 9.1.9a) — Klevetnik's "That
    /// resource loses all abilities until your next turn ends", made by
    /// `Effect::LoseAbilities`. Read with Hush's standing loss by
    /// `rules::active::lost_abilities`, the one question every reader of a
    /// card's abilities puts. Public: both players watched the resource
    /// chosen.
    LosesAbilities,
    /// During each encounter with the ice, the Runner cannot break more
    /// than this many of its printed subroutines — Anvil's 0 for the
    /// encounter, Unsmiling Tsarevna's 1 for the run (`Effect::
    /// LimitBreaks`). Read by `continuous::breaks_left` beside the ice's
    /// own `BreakLimit`, against the same count of the encounter's breaks.
    BreakLimit(u32),
    /// The server a card chose this turn (`Effect::ChooseServer`) — Tsakhia
    /// "Bankhar" Gantulga's "you may choose a server" — about the player
    /// who chose, its card the entry's `source`. Read back as "the chosen
    /// server" (`chosen_server`, `CardFilter::InChosenServer`) and spent by
    /// the card's replacement (`Effect::ReplaceSubroutines`). Public: the
    /// rules make a choice like it open, and the Corp has to know which
    /// ice will do net damage.
    ChosenServer(crate::rules::ServerId),
    /// Whenever the Corp would resolve a subroutine on the ice, it resolves
    /// instead the subroutine the entry's `source` prints — Tsakhia's
    /// "[subroutine] Do 1 net damage." (`Effect::ReplaceSubroutines`,
    /// read by `run::transition_subroutine`).
    SubroutinesReplaced,
    /// The install a card chose and refers back to (`Effect::Remember`,
    /// `Remembered::SelectedCard`) — Boomerang's "choose 1 installed piece
    /// of ice. Use this hardware only during encounters with that ice",
    /// about the chooser's install, read with [`chosen_card`]. Public: the
    /// rules make the choice open, and an install handle rides in a view
    /// unmasked.
    ChosenCard(InstallId),
    /// The card type a card chose and refers back to (`Effect::Remember`,
    /// `Remembered::CardType`) — Engram Flush's "choose a card type. For the
    /// remainder of the encounter…", about the chooser's install, read with
    /// [`chosen_card_type`]. Public: the Runner is told the type before
    /// anything is revealed.
    ChosenCardType(crate::dsl::CardType),
}

/// The Runner's mark this turn, if one has been identified (CR 10.11.1a:
/// one server, shared by every card that refers to a mark).
pub fn mark(state: &GameState) -> Option<crate::rules::ServerId> {
    state.lingering.iter().filter(|effect| effect.holds(state)).find_map(|effect| match effect.what {
        Lingering::Mark(server) => Some(server),
        _ => None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Until {
    /// The end of the encounter with this ice.
    EndOfEncounter(InstallId),
    EndOfRun,
    /// The end of the turn numbered so, by `GameState::turn`: this turn
    /// for every duration but one, and a later one for "until your next
    /// turn ends" (Klevetnik's, `EffectDuration::ThroughYourNextTurn`),
    /// which holds through the turns before it too.
    EndOfTurn(u32),
    /// Until `side`'s next turn begins, where the effect is taken
    /// (`Lingering::AllottedClicks`): it holds until then, and nothing
    /// but that turn's start ends it.
    NextTurnOf(Side),
    /// For as long as this install stays rezzed: "while it remains rezzed"
    /// (Lycian Multi-Munition). It stops holding when the card is derezzed
    /// or leaves the table, and a rez of the same install starts a new
    /// period, so `engine::rez_install` drops what an earlier one left
    /// (`forget_rezzed_period`) — otherwise a card derezzed and rezzed
    /// inside one action, with no checkpoint to sweep between, would carry
    /// its last choice into the next.
    WhileRezzed(InstallId),
    /// For as long as this install stays on the table, rezzed or not —
    /// a rig card's "while it is active" (CR 9.10.3c): Boomerang's chosen
    /// ice. Install handles are never reused, so a card trashed and
    /// installed again is a new install and keeps no earlier choice.
    WhileInstalled(InstallId),
    /// For as long as this played operation stays in the play area (CR
    /// 9.10.3c, the source becoming inactive) — Hyoubu Precog Manifold's
    /// "When you play this operation, choose a server", kept until the
    /// lockdown is trashed as the Corp's next turn begins
    /// (`CorpState::play_area`, `PlayedOperation::handle`).
    WhileInPlay(InstallId),
}

impl LingeringEffect {
    /// Whether the duration is still running, read off the state.
    pub fn holds(&self, state: &GameState) -> bool {
        match self.until {
            Until::EndOfEncounter(ice) => state.active_run.as_ref().is_some_and(|run| {
                run.phase == RunPhase::EncounterIce && run.ice.get(run.position).is_some_and(|encountered| encountered.install_id == ice)
            }),
            Until::EndOfRun => state.active_run.is_some(),
            Until::EndOfTurn(turn) => state.turn <= turn,
            Until::NextTurnOf(_) => true,
            Until::WhileRezzed(install) => state.corp.installed.iter().any(|card| card.install_id == install && card.rezzed),
            Until::WhileInstalled(install) => {
                state.runner.rig.iter().any(|card| card.install_id == install) || state.corp.installed.iter().any(|card| card.install_id == install)
            }
            Until::WhileInPlay(handle) => state.corp.play_area.iter().any(|played| played.handle == handle),
        }
    }
}

/// When a duration a card names ends, fixed at the moment the effect is
/// made: *this* encounter, *this* turn, `controller`'s next turn, or while
/// `made_by` — the install whose text it is — stays rezzed.
pub(crate) fn until(state: &GameState, duration: EffectDuration, controller: Side, made_by: Option<InstallId>) -> Result<Until, RulesError> {
    let run = state.active_run.as_ref();
    match duration {
        // Turns alternate (`GameState::turn` counts both players'), so the
        // controller's next turn is the next one, or the one after when
        // this one is already theirs.
        EffectDuration::ThroughYourNextTurn => {
            let theirs_now = crate::rules::listeners::active_side(state) == controller;
            Ok(Until::EndOfTurn(state.turn + if theirs_now { 2 } else { 1 }))
        }
        EffectDuration::Encounter => run
            .filter(|run| run.phase == RunPhase::EncounterIce)
            .and_then(|run| run.ice.get(run.position))
            .map(|ice| Until::EndOfEncounter(ice.install_id))
            .ok_or(RulesError::NotInEncounter),
        EffectDuration::Run => run.map(|_| Until::EndOfRun).ok_or(RulesError::NoActiveRun),
        EffectDuration::Turn => Ok(Until::EndOfTurn(state.turn)),
        EffectDuration::WhileRezzed => made_by.map(Until::WhileRezzed).ok_or(RulesError::UnresolvedCardTarget),
        EffectDuration::WhileInstalled => made_by.map(Until::WhileInstalled).ok_or(RulesError::UnresolvedCardTarget),
    }
}

/// What the lingering effects that still hold add to the strength of `on`.
pub fn strength(state: &GameState, on: InstallId) -> i32 {
    state
        .lingering
        .iter()
        .filter(|effect| effect.on == On::Install(on) && effect.holds(state))
        .map(|effect| match effect.what {
            Lingering::Strength(delta) => delta,
            Lingering::RezCost(_) | Lingering::Cannot(_) | Lingering::PreventRunEnding(_) | Lingering::AllottedClicks(_) | Lingering::GainSubtype(_) | Lingering::Mark(_) | Lingering::LosesAbilities | Lingering::BreakLimit(_) | Lingering::ChosenServer(_) | Lingering::SubroutinesReplaced | Lingering::ChosenCard(_) | Lingering::ChosenCardType(_) => 0,
        })
        .sum()
}

/// What the lingering effects that still hold add to the strength of the
/// piece of ice `on`: its own, and what each piece of ice gets (ezaM).
/// Apart from `strength`, which a rig card asks too, because `EachIce` is
/// never about an icebreaker.
pub fn ice_strength(state: &GameState, on: InstallId) -> i32 {
    strength(state, on)
        + state
            .lingering
            .iter()
            .filter(|effect| effect.on == On::EachIce && effect.holds(state))
            .map(|effect| match effect.what {
                Lingering::Strength(delta) => delta,
                Lingering::RezCost(_) | Lingering::Cannot(_) | Lingering::PreventRunEnding(_) | Lingering::AllottedClicks(_) | Lingering::GainSubtype(_) | Lingering::Mark(_) | Lingering::LosesAbilities | Lingering::BreakLimit(_) | Lingering::ChosenServer(_) | Lingering::SubroutinesReplaced | Lingering::ChosenCard(_) | Lingering::ChosenCardType(_) => 0,
            })
            .sum::<i32>()
}

/// What the lingering effects that still hold add to the rez cost of a
/// piece of ice.
pub fn ice_rez_cost(state: &GameState) -> i32 {
    state
        .lingering
        .iter()
        .filter(|effect| effect.on == On::EachIce && effect.holds(state))
        .map(|effect| match effect.what {
            Lingering::RezCost(delta) => delta,
            Lingering::Strength(_) | Lingering::Cannot(_) | Lingering::PreventRunEnding(_) | Lingering::AllottedClicks(_) | Lingering::GainSubtype(_) | Lingering::Mark(_) | Lingering::LosesAbilities | Lingering::BreakLimit(_) | Lingering::ChosenServer(_) | Lingering::SubroutinesReplaced | Lingering::ChosenCard(_) | Lingering::ChosenCardType(_) => 0,
        })
        .sum()
}

/// Whether a lingering effect that still holds gives the ice `on` the
/// subtype `subtype`. `continuous::ice_gains_subtype` asks it beside the
/// table.
pub fn gains_subtype(state: &GameState, on: InstallId, subtype: crate::dsl::IceType) -> bool {
    gained(&state.lingering, on, subtype, |effect| effect.holds(state))
}

/// [`gains_subtype`] over a list already filtered to what holds — the one a
/// `ClientView` carries, for a client drawing the ice's type.
pub fn listed_subtype(held: &[LingeringEffect], on: InstallId, subtype: crate::dsl::IceType) -> bool {
    gained(held, on, subtype, |_| true)
}

fn gained(list: &[LingeringEffect], on: InstallId, subtype: crate::dsl::IceType, holds: impl Fn(&LingeringEffect) -> bool) -> bool {
    list.iter().any(|effect| effect.what == Lingering::GainSubtype(subtype) && effect.on == On::Install(on) && holds(effect))
}

/// Whether a lingering effect that still holds takes away the abilities
/// of `install` (Klevetnik's). `rules::active::lost_abilities` asks it
/// beside Hush's standing loss.
pub fn loses_abilities(state: &GameState, install: InstallId) -> bool {
    state.lingering.iter().any(|effect| effect.what == Lingering::LosesAbilities && effect.holds(state) && reaches(state, &effect.on, install))
}

/// Whether an entry's `on` is about the install `install`: that install,
/// or a card in the root of the attacked server when that is what it says.
fn reaches(state: &GameState, on: &On, install: InstallId) -> bool {
    match on {
        On::Install(about) => *about == install,
        On::RootOfAttackedServer => in_attacked_root(state).any(|card| card == install),
        On::EachIce | On::Player(_) | On::CopiesOf(_) => false,
    }
}

/// The installs in the root of the attacked server right now; none outside
/// a run.
pub(crate) fn in_attacked_root(state: &GameState) -> impl Iterator<Item = InstallId> + '_ {
    let server = state.active_run.as_ref().map(|run| run.server);
    state
        .corp
        .installed
        .iter()
        .filter(move |card| Some(card.server) == server && card.slot == crate::rules::state::InstallSlot::Root)
        .map(|card| card.install_id)
}

/// The fewest printed subroutines a lingering limit lets the Runner break
/// on the ice `install` this encounter (Anvil, Unsmiling Tsarevna), or
/// `None` with none standing.
pub fn break_limit(state: &GameState, install: InstallId) -> Option<u32> {
    state
        .lingering
        .iter()
        .filter(|effect| effect.on == On::Install(install) && effect.holds(state))
        .filter_map(|effect| match effect.what {
            Lingering::BreakLimit(at_most) => Some(at_most),
            _ => None,
        })
        .min()
}

/// The server `card` chose this turn (`Effect::ChooseServer`), if it chose
/// one and has not spent it.
pub fn chosen_server(state: &GameState, card: &CardId) -> Option<crate::rules::ServerId> {
    state.lingering.iter().filter(|effect| &effect.source == card && effect.holds(state)).find_map(|effect| match effect.what {
        Lingering::ChosenServer(server) => Some(server),
        _ => None,
    })
}

/// The install the install `chooser` chose and refers back to
/// (`Effect::Remember`), while the choice holds — Boomerang's ice.
pub fn chosen_card(state: &GameState, chooser: InstallId) -> Option<InstallId> {
    state.lingering.iter().filter(|effect| effect.on == On::Install(chooser) && effect.holds(state)).find_map(|effect| match effect.what {
        Lingering::ChosenCard(chosen) => Some(chosen),
        _ => None,
    })
}

/// The card type the install `chooser` chose (`Effect::Remember`), while
/// the choice holds — Engram Flush's, for the encounter.
pub fn chosen_card_type(state: &GameState, chooser: InstallId) -> Option<&crate::dsl::CardType> {
    state.lingering.iter().filter(|effect| effect.on == On::Install(chooser) && effect.holds(state)).find_map(|effect| match &effect.what {
        Lingering::ChosenCardType(card_type) => Some(card_type),
        _ => None,
    })
}

/// Spends `card`'s chosen server: Tsakhia's replacement is for the first
/// encounter with ice protecting it.
pub(crate) fn spend_chosen_server(state: &mut GameState, card: &CardId) {
    state.lingering.retain(|effect| !(matches!(effect.what, Lingering::ChosenServer(_)) && &effect.source == card));
}

/// The card whose printed subroutine resolves instead of each subroutine on
/// the ice `install` (Tsakhia), while that holds.
pub fn subroutines_replaced_by(state: &GameState, install: InstallId) -> Option<&CardId> {
    state
        .lingering
        .iter()
        .find(|effect| effect.what == Lingering::SubroutinesReplaced && effect.on == On::Install(install) && effect.holds(state))
        .map(|effect| &effect.source)
}

/// Drops what an earlier rezzed period of `install` left: a rez starts a
/// new one (`Until::WhileRezzed`).
pub(crate) fn forget_rezzed_period(state: &mut GameState, install: InstallId) {
    state.lingering.retain(|effect| effect.until != Until::WhileRezzed(install));
}

/// Whether a prohibition is in force. Who it binds is the prohibition's
/// own (`Prohibition::binds`), which is what the entry's `on` says.
pub fn prohibits(state: &GameState, what: Prohibition) -> bool {
    in_force(&state.lingering, what, |effect| effect.holds(state))
}

/// [`prohibits`] over a list already filtered to what holds — the one a
/// `ClientView` carries.
pub fn listed(held: &[LingeringEffect], what: Prohibition) -> bool {
    in_force(held, what, |_| true)
}

fn in_force(list: &[LingeringEffect], what: Prohibition, holds: impl Fn(&LingeringEffect) -> bool) -> bool {
    list.iter().any(|effect| effect.what == Lingering::Cannot(what) && effect.on == On::Player(what.binds()) && holds(effect))
}

/// Whether a prohibition is in force about `card`: bound on its player, or
/// on copies of `card` (Perfect Recall). The question an access asks.
pub fn prohibits_about(state: &GameState, what: Prohibition, card: &CardId) -> bool {
    in_force_about(&state.lingering, what, card, |effect| effect.holds(state))
}

/// [`prohibits_about`] over a view's list.
pub fn listed_about(held: &[LingeringEffect], what: Prohibition, card: &CardId) -> bool {
    in_force_about(held, what, card, |_| true)
}

fn in_force_about(list: &[LingeringEffect], what: Prohibition, card: &CardId, holds: impl Fn(&LingeringEffect) -> bool) -> bool {
    in_force(list, what, &holds)
        || list.iter().any(|effect| effect.what == Lingering::Cannot(what) && effect.on == On::CopiesOf(card.clone()) && holds(effect))
}

/// Whether a prohibition is in force about the install `install`: bound on
/// its player, or on that install alone (Warm Reception's "you cannot
/// score that card this turn"). The question a score asks.
pub fn prohibits_install(state: &GameState, what: Prohibition, install: InstallId) -> bool {
    in_force(&state.lingering, what, |effect| effect.holds(state))
        || state.lingering.iter().any(|effect| effect.what == Lingering::Cannot(what) && effect.on == On::Install(install) && effect.holds(state))
}

/// The installs a prohibition bound to one install holds for right now
/// (`On::Install`) — Adrian Seis's "the Runner cannot access cards other
/// than this upgrade".
pub fn installs_prohibited(state: &GameState, what: Prohibition) -> Vec<InstallId> {
    state
        .lingering
        .iter()
        .filter(|effect| effect.what == Lingering::Cannot(what) && effect.holds(state))
        .filter_map(|effect| match effect.on {
            On::Install(install) => Some(install),
            _ => None,
        })
        .collect()
}

/// A rig card's strength before the table is asked: what it prints (as
/// installed) and the pumps still running on it.
pub fn rig_strength(state: &GameState, card: &InstalledRunnerCard) -> i32 {
    card.base_strength + strength(state, card.install_id)
}

/// What the allotment changes waiting for `side`'s turn add up to, taken
/// off the list: the turn they were for has begun.
pub(crate) fn take_allotted_clicks(state: &mut GameState, side: Side) -> i32 {
    let mut total = 0;
    state.lingering.retain(|effect| match (&effect.what, &effect.until) {
        (Lingering::AllottedClicks(delta), Until::NextTurnOf(whose)) if *whose == side => {
            total += delta;
            false
        }
        _ => true,
    });
    total
}

/// Drops what no longer holds.
pub(crate) fn sweep(state: &mut GameState) {
    if state.lingering.is_empty() {
        return;
    }
    let mut lingering = std::mem::take(&mut state.lingering);
    lingering.retain(|effect| effect.holds(state));
    state.lingering = lingering;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::run::{RunIce, RunState};

    fn effect(on: u32, amount: i32, until: Until) -> LingeringEffect {
        LingeringEffect { what: Lingering::Strength(amount), on: On::Install(InstallId(on)), until, source: CardId("source".to_string()) }
    }

    fn ice(install: u32) -> RunIce {
        RunIce {
            card_id: CardId("ice".to_string()),
            install_id: InstallId(install),
            ice_type: crate::dsl::IceType::Barrier,
            subroutines: Vec::new(),
            rezzed: true,
        }
    }

    fn encountering(ice_list: Vec<RunIce>, position: usize) -> GameState {
        GameState {
            active_run: Some(RunState { phase: RunPhase::EncounterIce, ice: ice_list, position, ..Default::default() }),
            ..Default::default()
        }
    }

    /// Each duration is a question about the state, so there is no moment
    /// at which one has to be remembered: the encounter pump is gone the
    /// instant the encounter is, whatever ended it.
    #[test]
    fn an_effect_holds_for_as_long_as_the_state_says_its_duration_is_running() {
        let mut state = encountering(vec![ice(10), ice(11)], 0);
        state.lingering = vec![
            effect(1, 1, Until::EndOfEncounter(InstallId(10))),
            effect(1, 2, Until::EndOfRun),
            effect(1, 4, Until::EndOfTurn(state.turn)),
            effect(2, 8, Until::EndOfRun),
        ];
        assert_eq!(strength(&state, InstallId(1)), 7, "all three, and not the other card's");

        state.active_run.as_mut().unwrap().phase = RunPhase::ApproachIce;
        assert_eq!(strength(&state, InstallId(1)), 6, "the encounter is over");

        let run = state.active_run.as_mut().unwrap();
        (run.phase, run.position) = (RunPhase::EncounterIce, 1);
        assert_eq!(strength(&state, InstallId(1)), 6, "and an encounter with the next ice is not that encounter");

        state.active_run = None;
        assert_eq!(strength(&state, InstallId(1)), 4);
        state.turn += 1;
        assert_eq!(strength(&state, InstallId(1)), 0);
    }

    /// An entry is about one install, each piece of ice, or a player, and
    /// each question reads only its own: a prohibition is not a strength,
    /// and Tread Lightly's +3 is nobody's pump. All of them end the way a
    /// pump does — by the state, with no reset to run.
    #[test]
    fn a_rez_cost_and_a_prohibition_hold_like_any_other_entry_and_answer_only_their_own_question() {
        let source = || CardId("source".to_string());
        let mut state = encountering(vec![ice(10)], 0);
        state.lingering = vec![
            LingeringEffect { what: Lingering::RezCost(3), on: On::EachIce, until: Until::EndOfRun, source: source() },
            LingeringEffect { what: Lingering::Cannot(Prohibition::StealOrTrash), on: On::Player(Side::Runner), until: Until::EndOfRun, source: source() },
            LingeringEffect { what: Lingering::Cannot(Prohibition::ScoreAgendas), on: On::Player(Side::Corp), until: Until::EndOfTurn(state.turn), source: source() },
        ];
        assert_eq!(ice_rez_cost(&state), 3);
        assert!(prohibits(&state, Prohibition::StealOrTrash) && prohibits(&state, Prohibition::ScoreAgendas));
        assert_eq!(strength(&state, InstallId(10)), 0, "none of them is a strength");
        assert!(listed(&state.lingering, Prohibition::StealOrTrash), "a view's list, already filtered, answers the same");

        state.active_run = None;
        assert_eq!(ice_rez_cost(&state), 0, "\"during that run\"");
        assert!(!prohibits(&state, Prohibition::StealOrTrash), "\"for the remainder of this run\"");
        assert!(prohibits(&state, Prohibition::ScoreAgendas), "the turn is not over");
        state.turn += 1;
        assert!(!prohibits(&state, Prohibition::ScoreAgendas));
    }

    /// A duration is fixed when the effect is made: *this* encounter,
    /// *this* turn — and "this run" needs a run.
    #[test]
    fn a_duration_is_resolved_against_the_state_it_was_named_in() {
        let mut state = encountering(vec![ice(10)], 0);
        assert_eq!(until(&state, EffectDuration::Encounter, Side::Runner, None), Ok(Until::EndOfEncounter(InstallId(10))));
        assert_eq!(until(&state, EffectDuration::Run, Side::Runner, None), Ok(Until::EndOfRun));
        assert_eq!(until(&state, EffectDuration::Turn, Side::Runner, None), Ok(Until::EndOfTurn(state.turn)));
        state.active_run.as_mut().unwrap().phase = RunPhase::ApproachIce;
        assert_eq!(until(&state, EffectDuration::Encounter, Side::Runner, None), Err(RulesError::NotInEncounter));
        state.active_run = None;
        assert_eq!(until(&state, EffectDuration::Run, Side::Runner, None), Err(RulesError::NoActiveRun));
        // "Until your next turn ends", made on the Runner's turn: the Corp's
        // is the next one; made on the Corp's own, the one after that.
        state.phase = crate::rules::GamePhase::Action(Side::Runner);
        assert_eq!(until(&state, EffectDuration::ThroughYourNextTurn, Side::Corp, None), Ok(Until::EndOfTurn(state.turn + 1)));
        state.phase = crate::rules::GamePhase::Action(Side::Corp);
        assert_eq!(until(&state, EffectDuration::ThroughYourNextTurn, Side::Corp, None), Ok(Until::EndOfTurn(state.turn + 2)));
    }

    #[test]
    fn a_sweep_drops_exactly_what_no_longer_holds() {
        let mut state = encountering(vec![ice(10)], 0);
        state.lingering = vec![effect(1, 1, Until::EndOfEncounter(InstallId(10))), effect(1, 2, Until::EndOfTurn(state.turn))];
        sweep(&mut state);
        assert_eq!(state.lingering.len(), 2);

        state.active_run = None;
        sweep(&mut state);
        assert_eq!(state.lingering, vec![effect(1, 2, Until::EndOfTurn(state.turn))]);
    }

    /// A state recorded before the list existed, or with nothing on it,
    /// reads and writes as it always did.
    #[test]
    fn an_empty_list_is_not_serialized() {
        let json = serde_json::to_string(&GameState::default()).unwrap();
        assert!(!json.contains("lingering"));
        assert!(serde_json::from_str::<GameState>(&json).unwrap().lingering.is_empty());
    }
}
