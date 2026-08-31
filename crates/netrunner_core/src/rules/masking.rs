use serde::{Deserialize, Serialize};

use crate::dsl::{CardId, Cost, IceType};
use crate::rules::run::{AccessPhase, AccessState, EncounteredSubroutine, RunIce, RunPhase, RunState, ServerId};
use crate::rules::state::{ArchivedCard, CorpState, GamePhase, GameState, InstallSlot, InstalledCard, InstalledRunnerCard, MemoryUnits, PaidAbilityWindow,
    PendingPrevention, PlayerResources, RunnerState, Side, TraceState,
};

/// A card zone whose contents are secret to everyone but its owner. The
/// count is always public (both players can see how many cards are in HQ or
/// R&D); only card identity is masked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskedZone {
    Visible(Vec<CardId>),
    Hidden { count: u32 },
}

/// An installed card as seen by a particular viewer: presence, server, and
/// rez status are always public, but an unrezzed card's identity is `None`
/// unless the viewer is its owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInstalledCard {
    pub server: ServerId,
    /// Never masked — whether a card occupies a server's ICE-protection
    /// slot or its root (content) slot is visible to both sides regardless
    /// of the card's identity.
    pub slot: InstallSlot,
    pub rezzed: bool,
    pub card: Option<CardId>,
    /// Never masked — advancement tokens are public info on the physical
    /// card, same as `server`/`rezzed`.
    pub advancement_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicCorpState {
    pub resources: PlayerResources,
    pub hq: MaskedZone,
    pub r_and_d: MaskedZone,
    /// Partially masked: the Runner always sees how many cards are in
    /// Archives and which way up each one is, but a facedown card's
    /// identity is hidden from them. The Corp always sees its own zone in
    /// full. See `PublicArchivedCard`.
    pub archives: Vec<PublicArchivedCard>,
    pub installed: Vec<PublicInstalledCard>,
    /// Never masked — scored Agendas sit in a fully public score area.
    pub scored_agendas: Vec<CardId>,
    /// Never masked — Bad Publicity is public information in the real game,
    /// same treatment as `scored_agendas`.
    pub bad_publicity: u32,
}

/// A Runner rig card as seen by any viewer: never hidden (see
/// `PublicRunnerState::rig`'s doc comment), including its current
/// (possibly pumped) strength — real Netrunner/Null Signal Games treats an
/// installed icebreaker's current strength as visible public information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicInstalledRunnerCard {
    pub card: CardId,
    pub current_strength: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunnerState {
    pub resources: PlayerResources,
    pub memory_units: MemoryUnits,
    /// Never masked — Brain damage count, like `memory_units`, is plain
    /// public information (it visibly shrinks the Runner's max hand size).
    pub brain_damage: usize,
    /// Never masked — tags are plain public information in the real game.
    pub tags: u32,
    pub grip: MaskedZone,
    pub stack: MaskedZone,
    /// Never masked — Rig cards are always face-up once installed.
    pub rig: Vec<PublicInstalledRunnerCard>,
    /// Never masked — like Corp's `archives`, a Runner's discard pile is a
    /// fully public zone in the real game.
    pub heap: Vec<CardId>,
    /// Never masked — stolen Agendas sit in a fully public score area.
    pub scored_agendas: Vec<CardId>,
    /// Never masked — static link strength, like `tags`, is plain public
    /// information (relevant to both sides during a trace).
    pub link_strength: u32,
}

/// A run's ICE as seen by a particular viewer: `rezzed` is always public,
/// but a face-down (unrezzed) ICE reveals *nothing* else — not just its
/// identity, but every identity-derived field (`current_strength`/
/// `ice_type`/`subroutines` are all printed on the hidden card face, same
/// as a real physical card) — unless the viewer is the Corp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunIce {
    pub rezzed: bool,
    pub identity: Option<PublicRunIceIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunIceIdentity {
    pub card: CardId,
    pub current_strength: i32,
    pub ice_type: IceType,
    pub subroutines: Vec<EncounteredSubroutine>,
}

/// A pending per-card access decision as seen by a particular viewer —
/// masking mirrors `PublicAccessState::unaccessed_cards`/`resolved_cards`:
/// the card being decided on is identity-visible to the Runner always, and
/// to the Corp only when accessing (fully public) Archives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicAccessPhase {
    SelectNextCard { selectable_cards: MaskedZone },
    PendingInteractiveTrigger { card: Option<CardId>, cost: Cost, can_pay: bool },
    PendingChoice { card: Option<CardId>, can_trash: bool, trash_cost: Option<u32>, mandatory_steal: bool, steal_cost: Option<Cost> },
}

/// `run::AccessState` as seen by a particular viewer. In the real game the
/// Runner sees exactly which card they're accessing the instant access
/// begins, but the Corp doesn't learn which HQ/R&D card was hit unless it's
/// since landed in a public zone (Archives, a score area) — so identity
/// here is visible to the Runner unconditionally, and to the Corp only when
/// `server == ServerId::Archives` (Archives is always a public zone).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicAccessState {
    pub server: ServerId,
    pub unaccessed_cards: MaskedZone,
    pub resolved_cards: MaskedZone,
    pub phase: PublicAccessPhase,
}

/// `run::RunState` as seen by a particular viewer. Drops
/// `bad_publicity_credits`/`additional_rd_access`/`additional_hq_access`/
/// `access_replacement` from the projection — none carry card identity,
/// none are consumed by any current renderer, and omitting a field is
/// always leak-safe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunState {
    pub server: ServerId,
    pub phase: RunPhase,
    pub ice: Vec<PublicRunIce>,
    pub position: usize,
    pub access_state: Option<PublicAccessState>,
    pub jack_out_permitted: bool,
}

/// `GameState` as visible to one player: hidden zones are collapsed to a
/// count, and unrezzed installed cards have their identity stripped unless
/// the viewer owns them. `phase` is never masked — turn structure is public.
/// `paid_ability_window` is likewise never masked — both players always see
/// whose priority it is and the current pass count. `active_trace` is
/// likewise never masked — both sides always see the trace strength and
/// whose bid is pending, matching the real game. `pending_prevention` gets
/// the same treatment, same rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicGameState {
    pub corp: PublicCorpState,
    pub runner: PublicRunnerState,
    pub phase: GamePhase,
    pub active_run: Option<PublicRunState>,
    pub paid_ability_window: Option<PaidAbilityWindow>,
    pub active_trace: Option<TraceState>,
    pub pending_prevention: Option<PendingPrevention>,
    /// Fully public — a pending paid choice/decision (who's offered it, its
    /// cost/options) carries no hidden information, same treatment as
    /// `active_trace`/`pending_prevention`.
    pub pending_paid_choice: Option<crate::rules::state::PendingPaidChoice>,
    pub pending_decision: Option<crate::rules::state::PendingDecision>,
}

pub fn mask_state_for_player(state: &GameState, player: Side) -> PublicGameState {
    PublicGameState {
        corp: mask_corp_state(&state.corp, player == Side::Corp),
        runner: mask_runner_state(&state.runner, player == Side::Runner),
        phase: state.phase,
        active_run: state.active_run.as_ref().map(|run| mask_run_state(run, player)),
        paid_ability_window: state.paid_ability_window.clone(),
        active_trace: state.active_trace.clone(),
        pending_prevention: state.pending_prevention.clone(),
        pending_paid_choice: state.pending_paid_choice.clone(),
        pending_decision: state.pending_decision.clone(),
    }
}

fn mask_run_ice(ice: &RunIce, owner_view: bool) -> PublicRunIce {
    let identity_visible = owner_view || ice.rezzed;
    PublicRunIce {
        rezzed: ice.rezzed,
        identity: identity_visible.then(|| PublicRunIceIdentity {
            card: ice.card_id.clone(),
            current_strength: ice.current_strength,
            ice_type: ice.ice_type,
            subroutines: ice.subroutines.clone(),
        }),
    }
}

fn mask_access_phase(phase: &AccessPhase, card_visible: bool) -> PublicAccessPhase {
    match phase {
        AccessPhase::SelectNextCard { selectable_cards } => {
            PublicAccessPhase::SelectNextCard { selectable_cards: mask_zone(selectable_cards, card_visible) }
        }
        AccessPhase::PendingInteractiveTrigger { card_id, cost, can_pay } => PublicAccessPhase::PendingInteractiveTrigger {
            card: card_visible.then(|| card_id.clone()),
            cost: cost.clone(),
            can_pay: *can_pay,
        },
        AccessPhase::PendingChoice { card_id, can_trash, trash_cost, mandatory_steal, steal_cost } => PublicAccessPhase::PendingChoice {
            card: card_visible.then(|| card_id.clone()),
            can_trash: *can_trash,
            trash_cost: *trash_cost,
            mandatory_steal: *mandatory_steal,
            steal_cost: steal_cost.clone(),
        },
    }
}

fn mask_access_state(access: &AccessState, card_visible: bool) -> PublicAccessState {
    PublicAccessState {
        server: access.server,
        unaccessed_cards: mask_zone(&access.unaccessed_cards, card_visible),
        resolved_cards: mask_zone(&access.resolved_cards, card_visible),
        phase: mask_access_phase(&access.phase, card_visible),
    }
}

fn mask_run_state(run: &RunState, player: Side) -> PublicRunState {
    // The Corp only ever sees accessed-card identities once the accessed
    // server is Archives (an always-public zone) — otherwise the Runner
    // alone knows what they hit until it lands in a public zone.
    let card_visible = player == Side::Runner || run.server == ServerId::Archives;
    PublicRunState {
        server: run.server,
        phase: run.phase,
        ice: run.ice.iter().map(|ice| mask_run_ice(ice, player == Side::Corp)).collect(),
        position: run.position,
        access_state: run.access_state.as_ref().map(|access| mask_access_state(access, card_visible)),
        jack_out_permitted: run.jack_out_permitted,
    }
}

fn mask_zone(cards: &[CardId], owner_view: bool) -> MaskedZone {
    if owner_view {
        MaskedZone::Visible(cards.to_vec())
    } else {
        MaskedZone::Hidden {
            count: cards.len() as u32,
        }
    }
}

fn mask_installed_card(installed: &InstalledCard, owner_view: bool) -> PublicInstalledCard {
    let identity_visible = owner_view || installed.rezzed;
    PublicInstalledCard {
        server: installed.server,
        slot: installed.slot,
        rezzed: installed.rezzed,
        card: identity_visible.then(|| installed.card.clone()),
        advancement_tokens: installed.advancement_tokens,
    }
}

/// One Archives card as seen by a given viewer. `facedown` is public to
/// both sides — everyone can see the shape of the pile — but `card` is
/// `None` for a facedown card viewed by the Runner, who has never seen it.
/// The Corp, looking at its own zone, always gets `Some`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicArchivedCard {
    /// `None` only when the card is facedown and the viewer is the Runner.
    pub card: Option<CardId>,
    pub facedown: bool,
}

fn mask_archived_card(archived: &ArchivedCard, owner_view: bool) -> PublicArchivedCard {
    let visible = owner_view || !archived.facedown;
    PublicArchivedCard {
        card: visible.then(|| archived.card.clone()),
        facedown: archived.facedown,
    }
}

fn mask_corp_state(corp: &CorpState, owner_view: bool) -> PublicCorpState {
    PublicCorpState {
        resources: corp.resources.clone(),
        hq: mask_zone(&corp.hq, owner_view),
        r_and_d: mask_zone(&corp.r_and_d, owner_view),
        archives: corp.archives.iter().map(|a| mask_archived_card(a, owner_view)).collect(),
        installed: corp
            .installed
            .iter()
            .map(|card| mask_installed_card(card, owner_view))
            .collect(),
        scored_agendas: corp.scored_agendas.clone(),
        bad_publicity: corp.bad_publicity,
    }
}

/// Deliberately still `effective_strength()`, not `ability::
/// computed_runner_strength` — `mask_state_for_player`'s whole call chain has
/// no `CardRegistry` parameter today (a much wider signature change than
/// this milestone's actual cards justify: it would ripple into every
/// consumer crate's `mask_state_for_player` call site). A card with a
/// `StrengthModifier` (e.g. Echelon) therefore displays its strength here
/// without that live bonus — the *mechanical* result (`Effect::
/// BreakSubroutines`'s strength contest, which does call
/// `computed_runner_strength`) is unaffected and always correct; only this
/// masked-view number can lag behind it. Revisit if a real UI consumer ever
/// needs the displayed number to match.
fn mask_installed_runner_card(card: &InstalledRunnerCard) -> PublicInstalledRunnerCard {
    PublicInstalledRunnerCard { card: card.card.clone(), current_strength: card.effective_strength() }
}

fn mask_runner_state(runner: &RunnerState, owner_view: bool) -> PublicRunnerState {
    PublicRunnerState {
        resources: runner.resources.clone(),
        memory_units: runner.memory_units,
        brain_damage: runner.brain_damage,
        tags: runner.tags,
        grip: mask_zone(&runner.grip, owner_view),
        stack: mask_zone(&runner.stack, owner_view),
        rig: runner.rig.iter().map(mask_installed_runner_card).collect(),
        heap: runner.heap.clone(),
        scored_agendas: runner.scored_agendas.clone(),
        link_strength: runner.link_strength,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::{AgendaPoints, Clicks, Credits, InstallSlot};

    fn game_state(corp: CorpState) -> GameState {
        GameState {
            corp,
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            pending_prevention: None, pending_paid_choice: None, pending_decision: None, last_discarded_cards: Vec::new(), last_completed_run: None, last_advancement_was_first: false, deferred_triggers: Vec::new(),
            seed: 0,
            rng_step: 0,
        }
    }

    fn corp_state_with_cards() -> CorpState {
        CorpState {
            resources: PlayerResources {
                credits: Credits(5),
                clicks: Clicks(3),
                agenda_points: AgendaPoints(0),
            },
            hq: vec![CardId("hedge_fund".to_string())],
            r_and_d: vec![CardId("ice_wall".to_string()), CardId("enigma".to_string())],
            archives: vec![ArchivedCard::facedown(CardId("cyberdex_trial".to_string()))],
            installed: vec![
                InstalledCard {
                    card: CardId("ice_wall".to_string()),
                    slot: InstallSlot::Ice,
                    ..Default::default()
                },
                InstalledCard {
                    card: CardId("enigma".to_string()),
                    server: ServerId::RnD,
                    slot: InstallSlot::Ice,
                    rezzed: true,
                    advancement_tokens: 2,
                    ..Default::default()
                },
            ],
            scored_agendas: vec![CardId("hostile_takeover".to_string())],
            ..Default::default()
        }
    }

    #[test]
    fn corp_view_shows_own_hq_and_rd_contents() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Corp);

        assert_eq!(
            masked.corp.hq,
            MaskedZone::Visible(vec![CardId("hedge_fund".to_string())])
        );
        assert_eq!(
            masked.corp.r_and_d,
            MaskedZone::Visible(vec![
                CardId("ice_wall".to_string()),
                CardId("enigma".to_string())
            ])
        );
    }

    #[test]
    fn runner_view_hides_corp_hq_and_rd_contents_but_shows_counts() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Runner);

        assert_eq!(masked.corp.hq, MaskedZone::Hidden { count: 1 });
        assert_eq!(masked.corp.r_and_d, MaskedZone::Hidden { count: 2 });
    }

    #[test]
    fn runner_view_hides_unrezzed_installed_card_identity() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Runner);

        let unrezzed = &masked.corp.installed[0];
        assert_eq!(unrezzed.server, ServerId::Hq);
        assert!(!unrezzed.rezzed);
        assert_eq!(unrezzed.card, None);
    }

    #[test]
    fn runner_view_reveals_rezzed_installed_card_identity() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Runner);

        let rezzed = &masked.corp.installed[1];
        assert!(rezzed.rezzed);
        assert_eq!(rezzed.card, Some(CardId("enigma".to_string())));
    }

    #[test]
    fn corp_view_shows_own_installed_cards_even_unrezzed() {
        let state = game_state(corp_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Corp);

        let unrezzed = &masked.corp.installed[0];
        assert!(!unrezzed.rezzed);
        assert_eq!(unrezzed.card, Some(CardId("ice_wall".to_string())));
    }

    fn runner_state_with_cards() -> RunnerState {
        RunnerState {
            resources: PlayerResources {
                credits: Credits(5),
                clicks: Clicks(3),
                agenda_points: AgendaPoints(0),
            },
            memory_units: MemoryUnits(4),
            grip: vec![CardId("sure_gamble".to_string())],
            stack: vec![CardId("modded".to_string()), CardId("clone_chip".to_string())],
            rig: vec![InstalledRunnerCard {
                card: CardId("gordian_blade".to_string()),
                base_strength: 2,
                encounter_strength_buff: 1,
                ..Default::default()
            }],
            heap: vec![CardId("easy_mark".to_string())],
            scored_agendas: vec![CardId("priority_requisition".to_string())],
            ..Default::default()
        }
    }

    fn game_state_with_runner(runner: RunnerState) -> GameState {
        GameState {
            corp: corp_state_with_cards(),
            runner,
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            pending_prevention: None, pending_paid_choice: None, pending_decision: None, last_discarded_cards: Vec::new(), last_completed_run: None, last_advancement_was_first: false, deferred_triggers: Vec::new(),
            seed: 0,
            rng_step: 0,
        }
    }

    #[test]
    fn runner_view_shows_own_grip_and_stack_contents() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Runner);

        assert_eq!(
            masked.runner.grip,
            MaskedZone::Visible(vec![CardId("sure_gamble".to_string())])
        );
        assert_eq!(
            masked.runner.stack,
            MaskedZone::Visible(vec![
                CardId("modded".to_string()),
                CardId("clone_chip".to_string())
            ])
        );
    }

    #[test]
    fn corp_view_hides_runner_grip_and_stack_contents_but_shows_counts() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked = mask_state_for_player(&state, Side::Corp);

        assert_eq!(masked.runner.grip, MaskedZone::Hidden { count: 1 });
        assert_eq!(masked.runner.stack, MaskedZone::Hidden { count: 2 });
    }

    #[test]
    fn a_facedown_archives_card_hides_its_identity_from_the_runner_but_not_the_corp() {
        let state = game_state(corp_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        // The Corp always sees its own zone in full.
        assert_eq!(
            masked_for_corp.corp.archives,
            vec![PublicArchivedCard { card: Some(CardId("cyberdex_trial".to_string())), facedown: true }]
        );
        // The Runner sees the pile's shape — one card, facedown — but never
        // learns which card it is.
        assert_eq!(masked_for_runner.corp.archives, vec![PublicArchivedCard { card: None, facedown: true }]);
    }

    #[test]
    fn a_faceup_archives_card_is_visible_to_both_sides() {
        let mut corp = corp_state_with_cards();
        corp.archives = vec![ArchivedCard::faceup(CardId("hedge_fund".to_string()))];
        let state = game_state(corp);

        let expected = vec![PublicArchivedCard { card: Some(CardId("hedge_fund".to_string())), facedown: false }];
        assert_eq!(mask_state_for_player(&state, Side::Corp).corp.archives, expected);
        assert_eq!(mask_state_for_player(&state, Side::Runner).corp.archives, expected);
    }

    #[test]
    fn corp_bad_publicity_is_never_masked() {
        let mut corp = corp_state_with_cards();
        corp.bad_publicity = 3;
        let state = game_state(corp);

        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        assert_eq!(masked_for_corp.corp.bad_publicity, 3);
        assert_eq!(masked_for_runner.corp.bad_publicity, 3);
    }

    #[test]
    fn runner_rig_is_never_masked() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        let expected = vec![PublicInstalledRunnerCard {
            card: CardId("gordian_blade".to_string()),
            current_strength: 3,
        }];
        assert_eq!(masked_for_corp.runner.rig, expected);
        assert_eq!(masked_for_runner.runner.rig, expected);
    }

    #[test]
    fn runner_rig_current_strength_is_never_masked() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        // base_strength 2 + encounter_strength_buff 1 = 3, from
        // runner_state_with_cards().
        assert_eq!(masked_for_corp.runner.rig[0].current_strength, 3);
        assert_eq!(masked_for_runner.runner.rig[0].current_strength, 3);
    }

    #[test]
    fn runner_heap_is_never_masked() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        let expected = vec![CardId("easy_mark".to_string())];
        assert_eq!(masked_for_corp.runner.heap, expected);
        assert_eq!(masked_for_runner.runner.heap, expected);
    }

    #[test]
    fn runner_tags_and_brain_damage_are_never_masked() {
        let mut runner = runner_state_with_cards();
        runner.tags = 2;
        runner.brain_damage = 1;
        let state = game_state_with_runner(runner);

        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        assert_eq!(masked_for_corp.runner.tags, 2);
        assert_eq!(masked_for_runner.runner.tags, 2);
        assert_eq!(masked_for_corp.runner.brain_damage, 1);
        assert_eq!(masked_for_runner.runner.brain_damage, 1);
    }

    #[test]
    fn scored_agendas_are_never_masked() {
        let state = game_state_with_runner(runner_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        let expected_corp = vec![CardId("hostile_takeover".to_string())];
        let expected_runner = vec![CardId("priority_requisition".to_string())];
        assert_eq!(masked_for_corp.corp.scored_agendas, expected_corp);
        assert_eq!(masked_for_runner.corp.scored_agendas, expected_corp);
        assert_eq!(masked_for_corp.runner.scored_agendas, expected_runner);
        assert_eq!(masked_for_runner.runner.scored_agendas, expected_runner);
    }

    #[test]
    fn advancement_tokens_are_never_masked() {
        let state = game_state(corp_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        // installed[1] ("enigma") is rezzed with 2 advancement tokens.
        assert_eq!(masked_for_corp.corp.installed[1].advancement_tokens, 2);
        assert_eq!(masked_for_runner.corp.installed[1].advancement_tokens, 2);
    }

    use crate::dsl::{Effect, SubroutineDef};
    use crate::rules::run::SubroutineStatus;

    fn run_ice(id: &str, rezzed: bool) -> RunIce {
        RunIce {
            card_id: CardId(id.to_string()),
            current_strength: 3,
            ice_type: IceType::Barrier,
            subroutines: if rezzed {
                vec![EncounteredSubroutine {
                    id: 0,
                    definition: SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun },
                    status: SubroutineStatus::Pending,
                }]
            } else {
                Vec::new()
            },
            rezzed,
        }
    }

    fn run_state(server: ServerId, ice: Vec<RunIce>, access_state: Option<AccessState>) -> RunState {
        RunState {
            server,
            phase: if access_state.is_some() { RunPhase::AccessingCard } else { RunPhase::ApproachIce },
            ice,
            access_state,
            jack_out_permitted: true,
            ..Default::default()
        }
    }

    fn state_with_run(run: RunState) -> GameState {
        let mut state = game_state_with_runner(runner_state_with_cards());
        state.active_run = Some(run);
        state
    }

    #[test]
    fn unrezzed_ice_identity_is_hidden_from_runner_but_visible_to_corp() {
        let run = run_state(ServerId::Hq, vec![run_ice("ice_wall", false)], None);
        let state = state_with_run(run);

        let for_runner = mask_state_for_player(&state, Side::Runner);
        let ice = &for_runner.active_run.as_ref().unwrap().ice[0];
        assert!(!ice.rezzed);
        assert_eq!(ice.identity, None);

        let for_corp = mask_state_for_player(&state, Side::Corp);
        let ice = &for_corp.active_run.as_ref().unwrap().ice[0];
        assert_eq!(ice.identity.as_ref().unwrap().card, CardId("ice_wall".to_string()));
    }

    #[test]
    fn rezzed_ice_identity_and_subroutines_are_visible_to_both_sides() {
        let run = run_state(ServerId::Hq, vec![run_ice("enigma", true)], None);
        let state = state_with_run(run);

        for side in [Side::Corp, Side::Runner] {
            let masked = mask_state_for_player(&state, side);
            let identity = masked.active_run.as_ref().unwrap().ice[0].identity.as_ref().unwrap();
            assert_eq!(identity.card, CardId("enigma".to_string()));
            assert_eq!(identity.subroutines.len(), 1);
        }
    }

    #[test]
    fn accessed_hq_card_identity_is_hidden_from_corp_but_visible_to_runner() {
        let access = AccessState {
            unaccessed_cards: vec![CardId("agenda".to_string())],
            phase: AccessPhase::PendingChoice {
                card_id: CardId("hedge_fund".to_string()),
                can_trash: false,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            },
            ..Default::default()
        };
        let run = run_state(ServerId::Hq, Vec::new(), Some(access));
        let state = state_with_run(run);

        let for_corp = mask_state_for_player(&state, Side::Corp);
        let corp_access = for_corp.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(corp_access.unaccessed_cards, MaskedZone::Hidden { count: 1 });
        assert!(matches!(&corp_access.phase, PublicAccessPhase::PendingChoice { card: None, .. }));

        let for_runner = mask_state_for_player(&state, Side::Runner);
        let runner_access = for_runner.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(runner_access.unaccessed_cards, MaskedZone::Visible(vec![CardId("agenda".to_string())]));
        assert!(matches!(
            &runner_access.phase,
            PublicAccessPhase::PendingChoice { card: Some(id), .. } if *id == CardId("hedge_fund".to_string())
        ));
    }

    #[test]
    fn accessed_archives_card_identity_is_visible_to_both_sides() {
        let access = AccessState {
            server: ServerId::Archives,
            phase: AccessPhase::PendingChoice {
                card_id: CardId("cyberdex_trial".to_string()),
                can_trash: false,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            },
            ..Default::default()
        };
        let run = run_state(ServerId::Archives, Vec::new(), Some(access));
        let state = state_with_run(run);

        for side in [Side::Corp, Side::Runner] {
            let masked = mask_state_for_player(&state, side);
            let access = masked.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
            assert!(matches!(
                &access.phase,
                PublicAccessPhase::PendingChoice { card: Some(id), .. } if *id == CardId("cyberdex_trial".to_string())
            ));
        }
    }
}
