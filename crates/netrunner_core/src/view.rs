//! `ClientView` — everything one player is entitled to know about a
//! `GameState`, plus the legal actions they specifically may submit.
//!
//! This is a thin adapter over `rules::masking`'s existing zone-masking
//! primitives (`mask_state_for_player`, `MaskedZone`, `PublicInstalledCard`,
//! ...) reshaped into a friendlier wire format — no zone-masking logic is
//! reimplemented here. The one deliberate policy difference from
//! `PublicGameState`: R&D/Stack (draw decks) are reported as a plain count
//! for *every* viewer, including the owner, whereas `PublicGameState`
//! reveals a side's own deck order to itself. `ClientView` is a stricter,
//! newer projection — nothing about "you don't know your own deck order"
//! contradicts real Netrunner/Null Signal Games rules, and `PublicGameState`
//! keeps its existing (tested, intentional) behavior for its own callers.

use serde::{Deserialize, Serialize};

use crate::cards::CardRegistry;
use crate::dsl::CardId;
use crate::rules::{
    legal_actions_for, mask_state_for_player, GamePhase, GameState, InstallSlot, MaskedZone, PaidAbilityWindow,
    PendingPrevention, PlayerAction, PublicArchivedCard, PublicInstalledCard, PublicInstalledRunnerCard, PublicRunState, ServerId, Side,
    TraceState,
};

/// Installed cards on one server, split by slot — generalizes the grouping
/// `netrunner_cli`'s TUI used to do by hand into a reusable core type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerView {
    pub server: ServerId,
    pub ice: Vec<PublicInstalledCard>,
    pub root: Vec<PublicInstalledCard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpClientView {
    pub credits: u32,
    pub clicks: u32,
    pub agenda_points: u32,
    pub bad_publicity: u32,
    /// See `masking::PublicCorpState::recurring_credits` — public, and
    /// carried so a determinized sample can reproduce what the Corp can
    /// actually pay with.
    pub recurring_credits: u32,
    pub recurring_credits_max: u32,
    pub hq_count: usize,
    /// `Some` only when the viewer is the Corp.
    pub hq_cards: Option<Vec<CardId>>,
    /// Deck order/contents are never revealed, even to their owner — see
    /// this module's doc comment.
    pub rd_count: usize,
    /// Archives as this viewer sees it: always the full pile shape, with a
    /// facedown card's identity hidden from the Runner. See
    /// `masking::PublicArchivedCard`.
    pub archives: Vec<PublicArchivedCard>,
    pub servers: Vec<ServerView>,
    pub scored_agendas: Vec<CardId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerClientView {
    pub credits: u32,
    pub clicks: u32,
    pub agenda_points: u32,
    pub memory_units: u32,
    pub tags: u32,
    pub brain_damage: usize,
    pub grip_count: usize,
    /// `Some` only when the viewer is the Runner.
    pub grip_cards: Option<Vec<CardId>>,
    /// Deck order/contents are never revealed, even to their owner — see
    /// this module's doc comment.
    pub stack_count: usize,
    pub heap: Vec<CardId>,
    pub rig: Vec<PublicInstalledRunnerCard>,
    pub link_strength: u32,
    pub scored_agendas: Vec<CardId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientView {
    pub side: Side,
    /// Whose turn this nominally is — total and phase-derived (`Action`/
    /// `Discard`/`Mulligan`'s side, or `GameOver`'s winner), distinct from
    /// `rules::current_actor` (which can momentarily differ mid-window,
    /// e.g. the Corp holding priority during the Runner's own turn to rez
    /// ICE — `legal_actions` below already accounts for that).
    pub active_player: Side,
    pub phase: GamePhase,
    /// `GameState::turn` verbatim — public information, and counted per
    /// side's turn rather than per round (see that field's doc comment).
    pub turn: u32,
    pub corp: CorpClientView,
    pub runner: RunnerClientView,
    pub active_run: Option<PublicRunState>,
    pub paid_ability_window: Option<PaidAbilityWindow>,
    pub active_trace: Option<TraceState>,
    pub pending_prevention: Option<PendingPrevention>,
    pub pending_paid_choice: Option<crate::rules::PendingPaidChoice>,
    pub pending_decision: Option<crate::rules::PendingDecision>,
    /// `legal_actions_for(state, registry, side)` — only the actions this
    /// viewer may actually submit.
    pub legal_actions: Vec<PlayerAction>,
}

fn zone_count(zone: &MaskedZone) -> usize {
    match zone {
        MaskedZone::Visible(cards) => cards.len(),
        MaskedZone::Hidden { count } => *count as usize,
    }
}

/// The visible-card list for an owner-viewable hand zone (`hq`/`grip`), or
/// `None` for a `Hidden` (opponent's) zone.
fn zone_cards(zone: &MaskedZone) -> Option<Vec<CardId>> {
    match zone {
        MaskedZone::Visible(cards) => Some(cards.clone()),
        MaskedZone::Hidden { .. } => None,
    }
}

fn group_by_server(installed: &[PublicInstalledCard]) -> Vec<ServerView> {
    let mut servers: Vec<ServerId> = installed.iter().map(|card| card.server).collect();
    servers.sort_by_key(server_sort_key);
    servers.dedup();

    servers
        .into_iter()
        .map(|server| {
            let (ice, root): (Vec<_>, Vec<_>) =
                installed.iter().filter(|card| card.server == server).cloned().partition(|card| card.slot == InstallSlot::Ice);
            ServerView { server, ice, root }
        })
        .collect()
}

fn server_sort_key(server: &ServerId) -> (u8, u32) {
    match server {
        ServerId::Hq => (0, 0),
        ServerId::RnD => (1, 0),
        ServerId::Archives => (2, 0),
        ServerId::Remote(n) => (3, *n),
    }
}

fn active_player(phase: GamePhase) -> Side {
    match phase {
        GamePhase::Mulligan(side)
        | GamePhase::StartOfTurn(side)
        | GamePhase::Action(side)
        | GamePhase::Discard { side, .. }
        | GamePhase::GameOver(side) => side,
    }
}

pub fn build_client_view(state: &GameState, registry: &CardRegistry, side: Side) -> ClientView {
    let public = mask_state_for_player(state, side);

    let corp = CorpClientView {
        credits: public.corp.resources.credits.0,
        clicks: public.corp.resources.clicks.0,
        agenda_points: public.corp.resources.agenda_points.0,
        bad_publicity: public.corp.bad_publicity,
        recurring_credits: public.corp.recurring_credits,
        recurring_credits_max: public.corp.recurring_credits_max,
        hq_count: zone_count(&public.corp.hq),
        hq_cards: zone_cards(&public.corp.hq),
        rd_count: zone_count(&public.corp.r_and_d),
        archives: public.corp.archives,
        servers: group_by_server(&public.corp.installed),
        scored_agendas: public.corp.scored_agendas,
    };

    let runner = RunnerClientView {
        credits: public.runner.resources.credits.0,
        clicks: public.runner.resources.clicks.0,
        agenda_points: public.runner.resources.agenda_points.0,
        memory_units: public.runner.memory_units.0,
        tags: public.runner.tags,
        brain_damage: public.runner.brain_damage,
        grip_count: zone_count(&public.runner.grip),
        grip_cards: zone_cards(&public.runner.grip),
        stack_count: zone_count(&public.runner.stack),
        heap: public.runner.heap,
        rig: public.runner.rig,
        link_strength: public.runner.link_strength,
        scored_agendas: public.runner.scored_agendas,
    };

    ClientView {
        side,
        active_player: active_player(state.phase),
        turn: state.turn,
        phase: public.phase,
        corp,
        runner,
        active_run: public.active_run,
        paid_ability_window: public.paid_ability_window,
        active_trace: public.active_trace,
        pending_prevention: public.pending_prevention,
        pending_paid_choice: public.pending_paid_choice,
        pending_decision: public.pending_decision,
        legal_actions: legal_actions_for(state, registry, side),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::InstallId;
    use crate::rules::test_support::install_of;
    use crate::dsl::{CardDefinition, CardType, IceType};
    use crate::rules::{
        AccessPhase, AccessState, AgendaPoints, Clicks, CorpState, Credits, InstalledCard, MemoryUnits, PlayerResources,
        PublicAccessPhase, RunIce, RunPhase, RunState, RunnerState, WindowCheckpoint,
    };

    fn blank_card(id: &str, side: Side, card_type: CardType) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type,
            is_playable: true,
            ..Default::default()
        }
    }

    fn empty_corp() -> CorpState {
        CorpState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            hq: vec![CardId("hedge_fund".to_string())],
            r_and_d: vec![CardId("ice_wall".to_string()), CardId("enigma".to_string())],
            installed: vec![
                InstalledCard {
                    install_id: InstallId(1071),
                    card: CardId("ice_wall".to_string()),
                    slot: InstallSlot::Ice,
                    ..Default::default()
                },
                InstalledCard {
                    install_id: InstallId(1072),
                    card: CardId("pad_campaign".to_string()),
                    server: ServerId::Remote(0),
                    rezzed: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn empty_runner() -> RunnerState {
        RunnerState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(4), agenda_points: AgendaPoints(0) },
            memory_units: MemoryUnits(4),
            grip: vec![CardId("sure_gamble".to_string())],
            stack: vec![CardId("diesel".to_string())],
            ..Default::default()
        }
    }

    fn base_state() -> GameState {
        let mut state = GameState::new(1);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.phase = GamePhase::Action(Side::Corp);
        state
    }

    #[test]
    fn hq_and_grip_are_owner_visible_only() {
        let state = base_state();
        let registry = CardRegistry::new();

        let corp_view = build_client_view(&state, &registry, Side::Corp);
        assert_eq!(corp_view.corp.hq_count, 1);
        assert_eq!(corp_view.corp.hq_cards, Some(vec![CardId("hedge_fund".to_string())]));
        assert_eq!(corp_view.runner.grip_count, 1);
        assert_eq!(corp_view.runner.grip_cards, None);

        let runner_view = build_client_view(&state, &registry, Side::Runner);
        assert_eq!(runner_view.corp.hq_cards, None);
        assert_eq!(runner_view.runner.grip_cards, Some(vec![CardId("sure_gamble".to_string())]));
    }

    #[test]
    fn rd_and_stack_are_count_only_for_every_viewer_including_the_owner() {
        let state = base_state();
        let registry = CardRegistry::new();

        for side in [Side::Corp, Side::Runner] {
            let view = build_client_view(&state, &registry, side);
            assert_eq!(view.corp.rd_count, 2);
            assert_eq!(view.runner.stack_count, 1);
        }
    }

    #[test]
    fn unrezzed_installed_card_identity_is_hidden_from_the_runner_but_structurally_present() {
        let state = base_state();
        let registry = CardRegistry::new();

        let runner_view = build_client_view(&state, &registry, Side::Runner);
        let hq_server = runner_view.corp.servers.iter().find(|s| s.server == ServerId::Hq).unwrap();
        assert_eq!(hq_server.ice.len(), 1);
        assert!(!hq_server.ice[0].rezzed);
        assert_eq!(hq_server.ice[0].card, None);

        let remote = runner_view.corp.servers.iter().find(|s| s.server == ServerId::Remote(0)).unwrap();
        assert_eq!(remote.root.len(), 1);
        assert_eq!(remote.root[0].card, Some(CardId("pad_campaign".to_string())));
    }

    #[test]
    fn legal_actions_never_contains_an_action_the_viewer_cannot_own() {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![RunIce {
                install_id: install_of(&state, "ice_wall"),
                card_id: CardId("ice_wall".to_string()),
                current_strength: 1,
                ice_type: IceType::Barrier,
                subroutines: Vec::new(),
                rezzed: false,
            }],
            ..Default::default()
        });
        state.paid_ability_window =
            Some(PaidAbilityWindow { active_priority: Side::Runner, consecutive_passes: 0, return_phase: Box::new(state.phase), checkpoint: WindowCheckpoint::Run });

        let registry = CardRegistry::from_cards(vec![blank_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier))]);
        let rez = PlayerAction::RezIce { ice: install_of(&state, "ice_wall") };

        let corp_view = build_client_view(&state, &registry, Side::Corp);
        assert!(corp_view.legal_actions.contains(&rez));

        let runner_view = build_client_view(&state, &registry, Side::Runner);
        assert!(!runner_view.legal_actions.contains(&rez));
        assert!(runner_view.legal_actions.contains(&PlayerAction::PassPriority { side: Side::Runner }));
    }

    #[test]
    fn active_player_reports_gameover_winner() {
        let mut state = base_state();
        state.phase = GamePhase::GameOver(Side::Runner);
        let registry = CardRegistry::new();
        let view = build_client_view(&state, &registry, Side::Corp);
        assert_eq!(view.active_player, Side::Runner);
        assert!(view.legal_actions.is_empty());
    }

    #[test]
    fn accessed_hq_card_identity_follows_run_masking_rules() {
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState {
            phase: RunPhase::AccessingCard,
            access_state: Some(AccessState {
                phase: AccessPhase::PendingChoice {
                    card_id: CardId("hedge_fund".to_string()),
                    trash_cost: None,
                    mandatory_steal: false,
                    steal_cost: None,
                },
                ..Default::default()
            }),
            jack_out_permitted: true,
            ..Default::default()
        });

        let registry = CardRegistry::new();
        let corp_view = build_client_view(&state, &registry, Side::Corp);
        let corp_phase = &corp_view.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase;
        assert!(matches!(corp_phase, PublicAccessPhase::PendingChoice { card: None, .. }));

        let runner_view = build_client_view(&state, &registry, Side::Runner);
        let runner_phase = &runner_view.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase;
        assert!(matches!(runner_phase, PublicAccessPhase::PendingChoice { card: Some(_), .. }));
    }

    #[test]
    fn build_client_view_smoke_with_registered_cards() {
        let mut registry = CardRegistry::new();
        registry.insert(blank_card("hedge_fund", Side::Corp, CardType::Operation));
        let state = base_state();
        let view = build_client_view(&state, &registry, Side::Corp);
        assert_eq!(view.side, Side::Corp);
    }
}
