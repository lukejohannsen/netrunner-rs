use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::{RunState, ServerId};
use crate::rules::state::{
    CorpState, GamePhase, GameState, InstalledCard, InstalledRunnerCard, MemoryUnits, PaidAbilityWindow,
    PlayerResources, RunnerState, Side, TraceState,
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
    /// Never masked — Archives is a fully public zone in the real game.
    pub archives: Vec<CardId>,
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

/// `GameState` as visible to one player: hidden zones are collapsed to a
/// count, and unrezzed installed cards have their identity stripped unless
/// the viewer owns them. `phase` is never masked — turn structure is public.
/// `paid_ability_window` is likewise never masked — both players always see
/// whose priority it is and the current pass count. `active_trace` is
/// likewise never masked — both sides always see the trace strength and
/// whose bid is pending, matching the real game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicGameState {
    pub corp: PublicCorpState,
    pub runner: PublicRunnerState,
    pub phase: GamePhase,
    pub active_run: Option<RunState>,
    pub paid_ability_window: Option<PaidAbilityWindow>,
    pub active_trace: Option<TraceState>,
}

pub fn mask_state_for_player(state: &GameState, player: Side) -> PublicGameState {
    PublicGameState {
        corp: mask_corp_state(&state.corp, player == Side::Corp),
        runner: mask_runner_state(&state.runner, player == Side::Runner),
        phase: state.phase,
        active_run: state.active_run.clone(),
        paid_ability_window: state.paid_ability_window.clone(),
        active_trace: state.active_trace.clone(),
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
        rezzed: installed.rezzed,
        card: identity_visible.then(|| installed.card.clone()),
        advancement_tokens: installed.advancement_tokens,
    }
}

fn mask_corp_state(corp: &CorpState, owner_view: bool) -> PublicCorpState {
    PublicCorpState {
        resources: corp.resources.clone(),
        hq: mask_zone(&corp.hq, owner_view),
        r_and_d: mask_zone(&corp.r_and_d, owner_view),
        archives: corp.archives.clone(),
        installed: corp
            .installed
            .iter()
            .map(|card| mask_installed_card(card, owner_view))
            .collect(),
        scored_agendas: corp.scored_agendas.clone(),
        bad_publicity: corp.bad_publicity,
    }
}

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
            runner: RunnerState { identity: None,
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
                scored_agendas: Vec::new(),
                link_strength: 0, first_hq_run_used_this_turn: false, first_install_discount_used_this_turn: false,
            },
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            seed: 0,
            rng_step: 0,
        }
    }

    fn corp_state_with_cards() -> CorpState {
        CorpState { identity: None, bad_publicity: 0, first_install_used_this_turn: false, recurring_credits: 0, recurring_credits_max: 0,
            resources: PlayerResources {
                credits: Credits(5),
                clicks: Clicks(3),
                agenda_points: AgendaPoints(0),
            },
            hq: vec![CardId("hedge_fund".to_string())],
            r_and_d: vec![CardId("ice_wall".to_string()), CardId("enigma".to_string())],
            archives: vec![CardId("cyberdex_trial".to_string())],
            installed: vec![
                InstalledCard {
                    card: CardId("ice_wall".to_string()),
                    server: ServerId::Hq,
                    slot: InstallSlot::Ice,
                    rezzed: false,
                    advancement_tokens: 0,
                },
                InstalledCard {
                    card: CardId("enigma".to_string()),
                    server: ServerId::RnD,
                    slot: InstallSlot::Ice,
                    rezzed: true,
                    advancement_tokens: 2,
                },
            ],
            scored_agendas: vec![CardId("hostile_takeover".to_string())],
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
        RunnerState { identity: None,
            resources: PlayerResources {
                credits: Credits(5),
                clicks: Clicks(3),
                agenda_points: AgendaPoints(0),
            },
            memory_units: MemoryUnits(4),
            brain_damage: 0,
            tags: 0,
            grip: vec![CardId("sure_gamble".to_string())],
            stack: vec![CardId("modded".to_string()), CardId("clone_chip".to_string())],
            rig: vec![InstalledRunnerCard {
                card: CardId("gordian_blade".to_string()),
                base_strength: 2,
                encounter_strength_buff: 1,
                turn_strength_buff: 0,
            }],
            heap: vec![CardId("easy_mark".to_string())],
            scored_agendas: vec![CardId("priority_requisition".to_string())],
            link_strength: 0, first_hq_run_used_this_turn: false, first_install_discount_used_this_turn: false,
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
    fn corp_archives_is_never_masked() {
        let state = game_state(corp_state_with_cards());
        let masked_for_corp = mask_state_for_player(&state, Side::Corp);
        let masked_for_runner = mask_state_for_player(&state, Side::Runner);

        let expected = vec![CardId("cyberdex_trial".to_string())];
        assert_eq!(masked_for_corp.corp.archives, expected);
        assert_eq!(masked_for_runner.corp.archives, expected);
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
}
