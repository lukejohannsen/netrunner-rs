use crate::dsl::CardId;
use crate::rules::event::GameEvent;
use crate::rules::run::state::ServerId;
use crate::rules::state::{GamePhase, GameState, InstallSlot};
use crate::rules::win::{agenda_value, check_win_conditions};

/// Determine which `CardId`s become accessible when a run against `server`
/// concludes successfully, then resolve the one access effect the engine
/// currently understands: stealing an Agenda (via the `win::agenda_value`
/// placeholder lookup — see its doc comment) awards the Runner its points
/// and checks win conditions. Every other access effect (paying to trash an
/// Asset/Upgrade, "on access" triggers) is still unresolved — those need
/// each card's full `CardType`/ability data, and no `CardRegistry` is wired
/// into the engine yet (see `PlayerAction::RezIce`'s doc comment for the
/// same gap).
///
/// Takes `&mut GameState` (rather than just `&CorpState`) because HQ access
/// needs `GameState::next_u64` to pick a pseudo-random index, and a stolen
/// Agenda needs to mutate `runner.resources.agenda_points` and possibly
/// `phase` — both live on `GameState`, so advancing/mutating either requires
/// mutable access.
///
/// Never fails: an empty zone simply yields zero events.
pub fn access_server(state: &mut GameState, server: ServerId) -> Vec<GameEvent> {
    // Root (non-ICE) installs on `server` — ICE is excluded via
    // `InstalledCard::slot`, which the installing action declares explicitly
    // (see `InstallSlot`'s doc comment for why this doesn't need a full
    // `CardRegistry`). A successful run accesses these alongside whatever
    // else that server's arm below yields, since Upgrades can be installed
    // on central servers (Hq/RnD) as well as Remote ones.
    let root_installs_on = |state: &GameState, server: ServerId| -> Vec<CardId> {
        state
            .corp
            .installed
            .iter()
            .filter(|installed| installed.server == server && installed.slot == InstallSlot::Root)
            .map(|installed| installed.card.clone())
            .collect()
    };

    let accessed: Vec<CardId> = match server {
        // Real rules access one *randomly* chosen HQ card. `next_u64` is
        // `GameState`'s deterministic pseudo-random source (no external RNG,
        // per AGENTS.md's purity requirement) — the roll is reduced modulo
        // `hq.len()` to pick an index.
        ServerId::Hq => {
            let mut accessed = if state.corp.hq.is_empty() {
                Vec::new()
            } else {
                let roll = state.next_u64();
                let index = (roll as usize) % state.corp.hq.len();
                state.corp.hq.get(index).cloned().into_iter().collect()
            };
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        // Real rules access one card too, but R&D isn't randomized — it's
        // drawn from a fixed deck order. `.last()` mirrors
        // `RunnerState::stack`'s "top of deck is the end of the Vec"
        // convention (see `engine.rs::draw_card_click`'s `stack.pop()`).
        ServerId::RnD => {
            let mut accessed: Vec<CardId> = state.corp.r_and_d.last().cloned().into_iter().collect();
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        // Archives is fully public; a successful run accesses all of it.
        ServerId::Archives => state.corp.archives.clone(),
        ServerId::Remote(_) => root_installs_on(state, server),
    };

    let mut events = Vec::new();
    for card in accessed {
        events.push(GameEvent::CardAccessed { card: card.clone(), server });

        if let Some(agenda_points) = agenda_value(&card) {
            state.runner.resources.agenda_points =
                state.runner.resources.agenda_points.gain(agenda_points);
            events.push(GameEvent::AgendaStolen { card, agenda_points });

            check_win_conditions(state);
            if let GamePhase::GameOver(winner) = state.phase {
                events.push(GameEvent::GameOver { winner });
                // The game just ended — don't keep accessing/stealing
                // further cards in this same batch (e.g. a second Agenda in
                // Archives or a Remote's Root).
                break;
            }
        }
    }
    events
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
                heap: Vec::new(),
            },
            phase: crate::rules::state::GamePhase::Action(Side::Corp),
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
    fn accessing_hq_yields_hq_card_and_root_installed_upgrades() {
        let installed = vec![
            InstalledCard {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Hq,
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                card: CardId("ash_2_0".to_string()),
                server: ServerId::Hq,
                slot: InstallSlot::Root,
                rezzed: false,
            },
        ];
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            installed,
            42,
        );
        assert_eq!(
            access_server(&mut state, ServerId::Hq),
            vec![
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Hq,
                },
                GameEvent::CardAccessed {
                    card: CardId("ash_2_0".to_string()),
                    server: ServerId::Hq,
                },
            ]
        );
    }

    #[test]
    fn accessing_rnd_yields_rnd_card_and_root_installed_upgrades() {
        let installed = vec![
            InstalledCard {
                card: CardId("wraparound".to_string()),
                server: ServerId::RnD,
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                card: CardId("crisium_grid".to_string()),
                server: ServerId::RnD,
                slot: InstallSlot::Root,
                rezzed: false,
            },
        ];
        let mut state = game_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            installed,
            0,
        );
        assert_eq!(
            access_server(&mut state, ServerId::RnD),
            vec![
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::RnD,
                },
                GameEvent::CardAccessed {
                    card: CardId("crisium_grid".to_string()),
                    server: ServerId::RnD,
                },
            ]
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

    #[test]
    fn stealing_an_agenda_that_reaches_seven_points_ends_the_game_with_a_runner_win() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("priority_requisition".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.agenda_points = AgendaPoints(4);

        let events = access_server(&mut state, ServerId::Archives);

        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(7));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed {
                    card: CardId("priority_requisition".to_string()),
                    server: ServerId::Archives,
                },
                GameEvent::AgendaStolen {
                    card: CardId("priority_requisition".to_string()),
                    agenda_points: 3,
                },
                GameEvent::GameOver { winner: Side::Runner },
            ]
        );
    }

    #[test]
    fn stealing_a_second_agenda_in_the_same_batch_after_winning_is_never_processed() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![
                CardId("priority_requisition".to_string()),
                CardId("hostile_takeover".to_string()),
            ],
            Vec::new(),
            0,
        );
        state.runner.resources.agenda_points = AgendaPoints(4);

        let events = access_server(&mut state, ServerId::Archives);

        // Capped at the winning threshold, not 8 — the second agenda
        // (worth 1 more point) was never reached.
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(7));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed {
                    card: CardId("priority_requisition".to_string()),
                    server: ServerId::Archives,
                },
                GameEvent::AgendaStolen {
                    card: CardId("priority_requisition".to_string()),
                    agenda_points: 3,
                },
                GameEvent::GameOver { winner: Side::Runner },
            ]
        );
    }
}
