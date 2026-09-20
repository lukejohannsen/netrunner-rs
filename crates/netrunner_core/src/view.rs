//! `ClientView` — everything one viewer is entitled to know about a
//! `GameState`, plus the legal actions they specifically may submit: a
//! player's own hand and the actions on offer, or for a
//! `Viewer::Spectator` neither hand, no deck, no unrezzed identity and an
//! empty action list (see `masking::Viewer`).
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
    legal_actions_for, mask_state_for_player, GamePhase, GameState, InstallId, InstallSlot, MaskedZone, PaidAbilityWindow,
    PendingPrevention, PlayerAction, PublicArchivedCard, PublicGameState, PublicInstalledCard, PublicInstalledRunnerCard, PublicRunState, ScoredAgenda, ServerId, Side,
    TraceState, Viewer,
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
    pub scored_agendas: Vec<ScoredAgenda>,
    /// Public — see `PublicCorpState::removed_from_game`.
    #[serde(default)]
    pub removed_from_game: Vec<CardId>,
    /// Power counters on the Corp's identity (AU Co.) — public, and
    /// carried so a determinized sample can pay a cost that spends them.
    #[serde(default)]
    pub identity_counters: u32,
    /// Public — a flip identity's side is visible to both players.
    #[serde(default)]
    pub identity_flipped: bool,
    /// The Corp's identity card — public, see
    /// `masking::PublicCorpState::identity`. A client can name the opponent
    /// from it; a determinizing bot narrows the hidden zones to that
    /// identity's published decklists.
    #[serde(default)]
    pub identity: Option<CardId>,
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
    /// Servers run this turn, oldest first — public, see
    /// `PublicRunnerState::servers_run_this_turn`.
    #[serde(default)]
    pub servers_run_this_turn: Vec<ServerId>,
    /// `PublicRunnerState::made_successful_run_this_turn`.
    #[serde(default)]
    pub made_successful_run_this_turn: bool,
    /// Cards discarded to hand size in the Runner's last discard phase —
    /// public, see `PublicRunnerState::discarded_this_discard_phase`.
    #[serde(default)]
    pub discarded_this_discard_phase: Vec<CardId>,
    /// `PublicRunnerState::identity_flipped`.
    #[serde(default)]
    pub identity_flipped: bool,
    /// The Runner's identity card — public, see `CorpClientView::identity`.
    #[serde(default)]
    pub identity: Option<CardId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientView {
    /// Who this projection is for. Was `side: Side` until spectators
    /// existed; a client that needs a seat reads `viewer.side()`.
    pub viewer: Viewer,
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
    /// `GameState::actions_taken_this_turn` verbatim — public, like
    /// `turn`: both players watched every action. Carried so a
    /// determinized search state agrees with the real one about Petty
    /// Cash's play condition.
    #[serde(default)]
    pub actions_taken_this_turn: u32,
    /// `GameState::rules` verbatim — a client has to be able to say how
    /// many agenda points win this match.
    #[serde(default)]
    pub rules: crate::rules::MatchRules,
    pub corp: CorpClientView,
    pub runner: RunnerClientView,
    pub active_run: Option<PublicRunState>,
    pub paid_ability_window: Option<PaidAbilityWindow>,
    pub active_trace: Option<TraceState>,
    pub pending_prevention: Option<PendingPrevention>,
    pub pending_paid_choice: Option<crate::rules::PendingPaidChoice>,
    pub pending_decision: Option<crate::rules::PendingDecision>,
    /// `PublicGameState::lingering` verbatim — the boosts and Leech's -1
    /// that hold right now, which the strengths in this view already
    /// include. A client may say where a number came from; a search
    /// rebuilding a state has to, or a pump it carried as printed strength
    /// would outlast its run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lingering: Vec<crate::rules::lingering::LingeringEffect>,
    /// The cards a parked `PendingDecision::ChooseCards` is choosing
    /// between, one per position the chooser can still name — so a client
    /// can say "Select Hedge Fund" where it said "Toggle selection of card
    /// 3". **Empty for every viewer but the chooser**: an opponent and a
    /// spectator see that a choice is being made, never among what.
    ///
    /// The positions are the ones `legal_actions` can toggle plus the ones
    /// already `selected`, in position order; nothing else in the zone is
    /// listed, so a search of R&D publishes the cards the filter admits and
    /// not the deck. Each card is shown to the chooser because the card
    /// asking says so — "search R&D", "look at the top card of your stack",
    /// "reveal the grip" — which is why this is not the R&D and stack
    /// count-only rule above being broken: it is the one case where a
    /// card's text grants the look. The exception is an opponent's
    /// unrezzed install (Tāo Salonga swapping ICE the Runner cannot
    /// identify), whose `card` is `None` on exactly the condition
    /// `PublicInstalledCard::card` is — read off the masked board, so the
    /// two rules cannot drift.
    #[serde(default)]
    pub selection: Vec<SelectionCandidate>,
    /// `legal_actions_for(state, registry, side)` — only the actions this
    /// viewer may actually submit. Empty for a spectator, who may submit
    /// nothing.
    pub legal_actions: Vec<PlayerAction>,
}

/// One card a `ChooseCards` prompt may select, as its chooser sees it. See
/// `ClientView::selection`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionCandidate {
    /// The `ToggleCardSelection` position this card is.
    pub position: usize,
    /// `None` only for an opponent's install the board conceals.
    pub card: Option<CardId>,
    /// The install, when the prompt selects from installed cards — public,
    /// like `PublicInstalledCard::install_id`, and what lets a client say
    /// where a card it cannot name sits.
    pub install: Option<InstallId>,
}

/// `ClientView::selection` for `viewer`: the parked selection's candidates
/// when the viewer is its chooser, with a Corp install's or an Archives
/// card's identity taken from the masked board rather than decided here.
fn selection_for(state: &GameState, registry: &CardRegistry, viewer: Viewer, public: &PublicGameState) -> Vec<SelectionCandidate> {
    let Some(parked) = crate::rules::pending_choice::selection_positions(state, registry) else { return Vec::new() };
    if !viewer.is(parked.chooser) {
        return Vec::new();
    }
    parked
        .candidates
        .into_iter()
        .map(|(position, card, install)| {
            // A Corp install resolves through the masked board; a rig card
            // is never masked, and `installed` does not hold one.
            let masked = match install {
                Some(id) => public.corp.installed.iter().find(|c| c.install_id == id).map(|c| c.card.clone()),
                None if parked.corp_archives => public.corp.archives.get(position).map(|a| a.card.clone()),
                None => None,
            };
            SelectionCandidate { position, card: masked.unwrap_or(Some(card)), install }
        })
        .collect()
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

pub fn build_client_view(state: &GameState, registry: &CardRegistry, viewer: impl Into<Viewer>) -> ClientView {
    let viewer = viewer.into();
    let public = mask_state_for_player(state, registry, viewer);
    let selection = selection_for(state, registry, viewer, &public);

    let corp = CorpClientView {
        credits: public.corp.resources.credits.0,
        clicks: public.corp.resources.clicks.0,
        agenda_points: public.corp.resources.agenda_points.0,
        bad_publicity: public.corp.bad_publicity,
        recurring_credits: public.corp.recurring_credits,
        identity_counters: public.corp.identity_counters,
        identity_flipped: public.corp.identity_flipped,
        identity: public.corp.identity,
        recurring_credits_max: public.corp.recurring_credits_max,
        hq_count: zone_count(&public.corp.hq),
        hq_cards: zone_cards(&public.corp.hq),
        rd_count: zone_count(&public.corp.r_and_d),
        archives: public.corp.archives,
        servers: group_by_server(&public.corp.installed),
        scored_agendas: public.corp.scored_agendas,
        removed_from_game: public.corp.removed_from_game,
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
        servers_run_this_turn: public.runner.servers_run_this_turn.clone(),
        made_successful_run_this_turn: public.runner.made_successful_run_this_turn,
        discarded_this_discard_phase: public.runner.discarded_this_discard_phase.clone(),
        identity_flipped: public.runner.identity_flipped,
        identity: public.runner.identity,
        scored_agendas: public.runner.scored_agendas,
    };

    ClientView {
        viewer,
        active_player: active_player(state.phase),
        turn: state.turn,
        actions_taken_this_turn: state.actions_taken_this_turn,
        rules: state.rules,
        phase: public.phase,
        corp,
        runner,
        active_run: public.active_run,
        paid_ability_window: public.paid_ability_window,
        active_trace: public.active_trace,
        pending_prevention: public.pending_prevention,
        pending_paid_choice: public.pending_paid_choice,
        pending_decision: public.pending_decision,
        lingering: public.lingering,
        selection,
        legal_actions: viewer.side().map(|side| legal_actions_for(state, registry, side)).unwrap_or_default(),
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
            access_state: Some(AccessState { pending_install: None, resolved_installs: Vec::new(),
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
        assert_eq!(view.viewer, Viewer::Player(Side::Corp));
    }

    /// The intersection: a spectator gets the Runner's view of the Corp
    /// and the Corp's view of the Runner, and nothing to submit.
    #[test]
    fn a_spectator_sees_neither_hand_and_has_no_legal_actions() {
        let mut registry = CardRegistry::new();
        registry.insert(blank_card("hedge_fund", Side::Corp, CardType::Operation));
        let state = base_state();
        let view = build_client_view(&state, &registry, Viewer::Spectator);
        assert_eq!(view.viewer, Viewer::Spectator);
        assert_eq!(view.corp.hq_cards, None);
        assert_eq!(view.runner.grip_cards, None);
        assert!(view.legal_actions.is_empty());
        let corp_sees = build_client_view(&state, &registry, Side::Corp);
        let runner_sees = build_client_view(&state, &registry, Side::Runner);
        assert_eq!(view.corp, runner_sees.corp, "the Corp's board as the Runner sees it");
        assert_eq!(view.runner, corp_sees.runner, "the Runner's board as the Corp sees it");
    }

    fn choose_cards(side: Side, source: crate::dsl::CardZoneRef, filter: crate::dsl::CardFilter, max: u32, selected: Vec<usize>) -> crate::rules::PendingDecision {
        crate::rules::PendingDecision::ChooseCards {
            side,
            source,
            filter,
            min: 1,
            max,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected,
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: crate::rules::PendingChoiceResume::None,
        }
    }

    fn toggles(view: &ClientView) -> Vec<usize> {
        view.legal_actions
            .iter()
            .filter_map(|action| match action {
                PlayerAction::ToggleCardSelection { position } => Some(*position),
                _ => None,
            })
            .collect()
    }

    /// Malapert's "search R&D for 1 non-agenda card": the Corp is shown
    /// the cards the filter admits, by name, at exactly the positions its
    /// toggles name — and not the agenda, and not to anyone else. R&D is
    /// count-only everywhere else in the view; the card's text is what
    /// grants this look.
    #[test]
    fn a_search_of_rd_shows_the_chooser_what_the_filter_admits_and_no_one_else_anything() {
        let registry = CardRegistry::from_cards(vec![
            blank_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier)),
            blank_card("hedge_fund", Side::Corp, CardType::Operation),
            CardDefinition { agenda_points: Some(2), advancement_requirement: Some(3), ..blank_card("hostile_takeover", Side::Corp, CardType::Agenda) },
        ]);
        let mut state = base_state();
        state.corp.r_and_d = vec![CardId("ice_wall".to_string()), CardId("hostile_takeover".to_string()), CardId("hedge_fund".to_string())];
        state.pending_decision = Some(choose_cards(Side::Corp, crate::dsl::CardZoneRef::OwnRAndD, crate::dsl::CardFilter::NonAgenda, 1, Vec::new()));

        let corp = build_client_view(&state, &registry, Side::Corp);
        assert_eq!(
            corp.selection,
            vec![
                SelectionCandidate { position: 0, card: Some(CardId("ice_wall".to_string())), install: None },
                SelectionCandidate { position: 2, card: Some(CardId("hedge_fund".to_string())), install: None },
            ]
        );
        assert_eq!(corp.selection.iter().map(|c| c.position).collect::<Vec<_>>(), toggles(&corp), "one candidate per toggle");
        assert_eq!(corp.corp.rd_count, 3, "the deck itself stays a count");

        for other in [Viewer::Player(Side::Runner), Viewer::Spectator] {
            assert!(build_client_view(&state, &registry, other).selection.is_empty(), "{other:?} sees that a choice is made, not among what");
        }
    }

    /// A chosen card stays listed after its toggle, so the prompt can say
    /// what is selected — and when the selection is full, that is the only
    /// toggle left and still named.
    #[test]
    fn a_selected_card_stays_a_candidate() {
        let registry = CardRegistry::from_cards(vec![blank_card("hedge_fund", Side::Corp, CardType::Operation)]);
        let mut state = base_state();
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("hedge_fund".to_string())];
        state.pending_decision = Some(choose_cards(Side::Corp, crate::dsl::CardZoneRef::OwnHq, crate::dsl::CardFilter::Any, 1, vec![1]));
        let corp = build_client_view(&state, &registry, Side::Corp);
        assert_eq!(corp.selection.iter().map(|c| c.position).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(toggles(&corp), vec![1], "full at one: only the deselect is legal");
    }

    /// The Runner choosing among the Corp's installs (Tāo Salonga's swap)
    /// is told where each is and, for an unrezzed one, nothing more: the
    /// same `None` the board gives it.
    #[test]
    fn an_opponents_unrezzed_install_is_a_candidate_by_place_and_never_by_name() {
        let registry = CardRegistry::from_cards(vec![
            blank_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier)),
            blank_card("pad_campaign", Side::Corp, CardType::Asset),
        ]);
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.pending_decision = Some(choose_cards(Side::Runner, crate::dsl::CardZoneRef::OpponentInstalled, crate::dsl::CardFilter::Any, 1, Vec::new()));
        let runner = build_client_view(&state, &registry, Side::Runner);
        assert_eq!(
            runner.selection,
            vec![
                SelectionCandidate { position: 0, card: None, install: Some(InstallId(1071)) },
                SelectionCandidate { position: 1, card: Some(CardId("pad_campaign".to_string())), install: Some(InstallId(1072)) },
            ]
        );
        assert!(build_client_view(&state, &registry, Side::Corp).selection.is_empty());
    }

    /// A facedown Archives card is hidden from the Runner by the board, and
    /// a selection over the Corp's Archives hides it the same way. No card
    /// selects from there for the Runner today; the rule is here so the
    /// first one cannot leak.
    #[test]
    fn a_facedown_archives_card_is_a_candidate_the_runner_cannot_name() {
        let registry = CardRegistry::from_cards(vec![blank_card("hedge_fund", Side::Corp, CardType::Operation)]);
        let mut state = base_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.archives = vec![crate::rules::ArchivedCard::faceup(CardId("hedge_fund".to_string())), crate::rules::ArchivedCard { card: CardId("hedge_fund".to_string()), facedown: true }];
        state.pending_decision = Some(choose_cards(Side::Runner, crate::dsl::CardZoneRef::OpponentDiscard, crate::dsl::CardFilter::Any, 1, Vec::new()));
        let runner = build_client_view(&state, &registry, Side::Runner);
        assert_eq!(runner.selection.iter().map(|c| c.card.is_some()).collect::<Vec<_>>(), vec![true, false]);
    }

    /// A view serialized before the field existed still reads.
    #[test]
    fn a_view_without_a_selection_field_deserializes() {
        let view = build_client_view(&base_state(), &CardRegistry::new(), Side::Corp);
        let mut json = serde_json::to_value(&view).unwrap();
        json.as_object_mut().unwrap().remove("selection");
        let back: ClientView = serde_json::from_value(json).unwrap();
        assert_eq!(back, view);
    }
}
