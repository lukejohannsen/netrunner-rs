use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::run::{RunState, ServerId};
use crate::rules::state::{CorpState, GameState, InstalledCard, MemoryUnits, PlayerResources, RunnerState, Side};

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicCorpState {
    pub resources: PlayerResources,
    pub hq: MaskedZone,
    pub r_and_d: MaskedZone,
    pub installed: Vec<PublicInstalledCard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRunnerState {
    pub resources: PlayerResources,
    pub memory_units: MemoryUnits,
    pub grip_size: u32,
    pub stack_size: u32,
}

/// `GameState` as visible to one player: hidden zones are collapsed to a
/// count, and unrezzed installed cards have their identity stripped unless
/// the viewer owns them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicGameState {
    pub corp: PublicCorpState,
    pub runner: PublicRunnerState,
    pub active_turn: Side,
    pub active_run: Option<RunState>,
}

pub fn mask_state_for_player(state: &GameState, player: Side) -> PublicGameState {
    PublicGameState {
        corp: mask_corp_state(&state.corp, player == Side::Corp),
        runner: mask_runner_state(&state.runner),
        active_turn: state.active_turn,
        active_run: state.active_run.clone(),
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
    }
}

fn mask_corp_state(corp: &CorpState, owner_view: bool) -> PublicCorpState {
    PublicCorpState {
        resources: corp.resources.clone(),
        hq: mask_zone(&corp.hq, owner_view),
        r_and_d: mask_zone(&corp.r_and_d, owner_view),
        installed: corp
            .installed
            .iter()
            .map(|card| mask_installed_card(card, owner_view))
            .collect(),
    }
}

// RunnerState currently only tracks placeholder grip/stack counts (no card
// identity), so there is nothing to mask here yet — this is a pass-through
// until Runner hand/deck contents gain real card identity.
fn mask_runner_state(runner: &RunnerState) -> PublicRunnerState {
    PublicRunnerState {
        resources: runner.resources.clone(),
        memory_units: runner.memory_units,
        grip_size: runner.grip_size,
        stack_size: runner.stack_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::{AgendaPoints, Clicks, Credits};

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
                grip_size: 0,
                stack_size: 0,
            },
            active_turn: Side::Corp,
            active_run: None,
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
            installed: vec![
                InstalledCard {
                    card: CardId("ice_wall".to_string()),
                    server: ServerId::Hq,
                    rezzed: false,
                },
                InstalledCard {
                    card: CardId("enigma".to_string()),
                    server: ServerId::RnD,
                    rezzed: true,
                },
            ],
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
}
