use crate::dsl::CardId;
use crate::rules::event::GameEvent;
use crate::rules::run::state::ServerId;
use crate::rules::state::{GameState, InstallSlot};

/// Determine which `CardId`s become accessible when a run against `server`
/// concludes successfully.
///
/// Scoped narrowly: only determines *which* cards are accessed, not the
/// effects of accessing them (stealing an Agenda, paying to trash an
/// Asset/Upgrade). Resolving those needs each card's `CardType`, and no
/// `CardRegistry` is wired into the engine yet (see `PlayerAction::RezIce`'s
/// doc comment for the same gap).
///
/// Takes `&mut GameState` (rather than just `&CorpState`) because HQ access
/// needs `GameState::next_u64` to pick a pseudo-random index — the RNG step
/// lives on `GameState`, so advancing it requires mutable access.
///
/// Never fails: an empty zone simply yields zero events.
pub fn access_server(state: &mut GameState, server: ServerId) -> Vec<GameEvent> {
    let accessed: Vec<CardId> = match server {
        // Real rules access one *randomly* chosen HQ card. `next_u64` is
        // `GameState`'s deterministic pseudo-random source (no external RNG,
        // per AGENTS.md's purity requirement) — the roll is reduced modulo
        // `hq.len()` to pick an index.
        ServerId::Hq => {
            if state.corp.hq.is_empty() {
                Vec::new()
            } else {
                let roll = state.next_u64();
                let index = (roll as usize) % state.corp.hq.len();
                state.corp.hq.get(index).cloned().into_iter().collect()
            }
        }
        // Real rules access one card too, but R&D isn't randomized — it's
        // drawn from a fixed deck order. `.last()` mirrors
        // `RunnerState::stack`'s "top of deck is the end of the Vec"
        // convention (see `engine.rs::draw_card_click`'s `stack.pop()`).
        ServerId::RnD => state.corp.r_and_d.last().cloned().into_iter().collect(),
        // Archives is fully public; a successful run accesses all of it.
        ServerId::Archives => state.corp.archives.clone(),
        // Accesses only Root (non-ICE) installs on the remote — ICE is
        // excluded via `InstalledCard::slot`, which the installing action
        // declares explicitly (see `InstallSlot`'s doc comment for why this
        // doesn't need a full `CardRegistry`).
        ServerId::Remote(_) => state
            .corp
            .installed
            .iter()
            .filter(|installed| installed.server == server && installed.slot == InstallSlot::Root)
            .map(|installed| installed.card.clone())
            .collect(),
    };

    accessed
        .into_iter()
        .map(|card| GameEvent::CardAccessed { card, server })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::{
        AgendaPoints, Clicks, Credits, InstalledCard, MemoryUnits, PlayerResources, RunnerState,
        Side,
    };
    use std::collections::HashSet;

    fn game_state(
        hq: Vec<CardId>,
        r_and_d: Vec<CardId>,
        archives: Vec<CardId>,
        installed: Vec<InstalledCard>,
        seed: u64,
    ) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                hq,
                r_and_d,
                archives,
                installed,
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
            },
            active_turn: Side::Corp,
            active_run: None,
            seed,
            rng_step: 0,
        }
    }

    #[test]
    fn accessing_hq_with_one_card_yields_that_card() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        assert_eq!(
            access_server(&mut state, ServerId::Hq),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::Hq,
            }]
        );
        // The RNG step still advances even with only one possible index.
        assert_eq!(state.rng_step, 1);
    }

    #[test]
    fn accessing_hq_is_deterministic_for_a_given_seed() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];
        let mut state_a = game_state(hq.clone(), Vec::new(), Vec::new(), Vec::new(), 42);
        let mut state_b = game_state(hq, Vec::new(), Vec::new(), Vec::new(), 42);

        let events_a = access_server(&mut state_a, ServerId::Hq);
        let events_b = access_server(&mut state_b, ServerId::Hq);

        assert_eq!(events_a, events_b);
        assert_eq!(events_a.len(), 1);
    }

    #[test]
    fn accessing_hq_yields_varied_indices_across_different_seeds() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];

        let accessed_cards: HashSet<CardId> = (0..20u64)
            .map(|seed| {
                let mut state = game_state(hq.clone(), Vec::new(), Vec::new(), Vec::new(), seed);
                match access_server(&mut state, ServerId::Hq).into_iter().next() {
                    Some(GameEvent::CardAccessed { card, .. }) => card,
                    other => panic!("expected a CardAccessed event, got {other:?}"),
                }
            })
            .collect();

        assert!(
            accessed_cards.len() > 1,
            "expected varied indices across seeds, got only {accessed_cards:?}"
        );
    }

    #[test]
    fn accessing_rnd_yields_the_last_card() {
        let mut state = game_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            0,
        );
        assert_eq!(
            access_server(&mut state, ServerId::RnD),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::RnD,
            }]
        );
    }

    #[test]
    fn accessing_archives_yields_every_card_in_it() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        assert_eq!(
            access_server(&mut state, ServerId::Archives),
            vec![
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Archives
                },
                GameEvent::CardAccessed {
                    card: CardId("ice_wall".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
    }

    #[test]
    fn accessing_remote_skips_installed_ice_and_yields_only_root_installs() {
        let installed = vec![
            InstalledCard {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Root,
                rezzed: false,
            },
            InstalledCard {
                card: CardId("enigma".to_string()),
                server: ServerId::Remote(1),
                slot: InstallSlot::Ice,
                rezzed: true,
            },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        assert_eq!(
            access_server(&mut state, ServerId::Remote(0)),
            vec![GameEvent::CardAccessed {
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0)
            }]
        );
    }

    #[test]
    fn accessing_remote_with_only_ice_yields_no_events() {
        let installed = vec![InstalledCard {
            card: CardId("ice_wall".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Ice,
            rezzed: true,
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        assert_eq!(access_server(&mut state, ServerId::Remote(0)), Vec::new());
    }

    #[test]
    fn accessing_an_empty_zone_yields_no_events() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        assert_eq!(access_server(&mut state, ServerId::Hq), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::RnD), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::Archives), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::Remote(0)), Vec::new());
    }
}
