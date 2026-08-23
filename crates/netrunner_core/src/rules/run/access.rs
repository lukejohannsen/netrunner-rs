use crate::dsl::CardId;
use crate::rules::event::GameEvent;
use crate::rules::run::state::ServerId;
use crate::rules::state::CorpState;

/// Determine which `CardId`s become accessible when a run against `server`
/// concludes successfully.
///
/// Scoped narrowly: only determines *which* cards are accessed, not the
/// effects of accessing them (stealing an Agenda, paying to trash an
/// Asset/Upgrade). Resolving those needs each card's `CardType`, and no
/// `CardRegistry` is wired into the engine yet (see `PlayerAction::RezIce`'s
/// doc comment for the same gap).
///
/// Takes `&CorpState` rather than `&GameState`, mirroring
/// `advance_run(&RunState, ...)`'s narrow-slice pattern.
///
/// Never fails: an empty zone simply yields zero events.
pub fn access_server(corp: &CorpState, server: ServerId) -> Vec<GameEvent> {
    let accessed: Vec<CardId> = match server {
        // Real rules access one *randomly* chosen HQ card; this engine has
        // no RNG source (must stay deterministic per AGENTS.md), so this
        // deterministically picks the first card as a stand-in.
        ServerId::Hq => corp.hq.first().cloned().into_iter().collect(),
        // Same determinism caveat as HQ. `.last()` mirrors
        // `RunnerState::stack`'s "top of deck is the end of the Vec"
        // convention (see `engine.rs::draw_card_click`'s `stack.pop()`).
        ServerId::RnD => corp.r_and_d.last().cloned().into_iter().collect(),
        // Archives is fully public; a successful run accesses all of it.
        ServerId::Archives => corp.archives.clone(),
        // Accesses every installed card on the remote, ICE and non-ICE
        // alike. Known over-approximation: real rules only let a Runner
        // access non-ICE installs. Distinguishing them needs a CardType
        // lookup via a CardRegistry, which doesn't exist yet.
        ServerId::Remote(_) => corp
            .installed
            .iter()
            .filter(|installed| installed.server == server)
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
    use crate::rules::state::{AgendaPoints, Clicks, Credits, InstalledCard, PlayerResources};

    fn corp_state(
        hq: Vec<CardId>,
        r_and_d: Vec<CardId>,
        archives: Vec<CardId>,
        installed: Vec<InstalledCard>,
    ) -> CorpState {
        CorpState {
            resources: PlayerResources {
                credits: Credits(0),
                clicks: Clicks(0),
                agenda_points: AgendaPoints(0),
            },
            hq,
            r_and_d,
            archives,
            installed,
        }
    }

    #[test]
    fn accessing_hq_yields_the_first_card() {
        let corp = corp_state(
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            access_server(&corp, ServerId::Hq),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::Hq,
            }]
        );
    }

    #[test]
    fn accessing_rnd_yields_the_last_card() {
        let corp = corp_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            access_server(&corp, ServerId::RnD),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::RnD,
            }]
        );
    }

    #[test]
    fn accessing_archives_yields_every_card_in_it() {
        let corp = corp_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
        );
        assert_eq!(
            access_server(&corp, ServerId::Archives),
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
    fn accessing_remote_yields_all_installed_cards_on_it_ice_and_non_ice_alike() {
        let installed = vec![
            InstalledCard {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Remote(0),
                rezzed: true,
            },
            InstalledCard {
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0),
                rezzed: false,
            },
            InstalledCard {
                card: CardId("enigma".to_string()),
                server: ServerId::Remote(1),
                rezzed: true,
            },
        ];
        let corp = corp_state(Vec::new(), Vec::new(), Vec::new(), installed);
        assert_eq!(
            access_server(&corp, ServerId::Remote(0)),
            vec![
                GameEvent::CardAccessed {
                    card: CardId("ice_wall".to_string()),
                    server: ServerId::Remote(0)
                },
                GameEvent::CardAccessed {
                    card: CardId("pad_campaign".to_string()),
                    server: ServerId::Remote(0)
                },
            ]
        );
    }

    #[test]
    fn accessing_an_empty_zone_yields_no_events() {
        let corp = corp_state(Vec::new(), Vec::new(), Vec::new(), Vec::new());
        assert_eq!(access_server(&corp, ServerId::Hq), Vec::new());
        assert_eq!(access_server(&corp, ServerId::RnD), Vec::new());
        assert_eq!(access_server(&corp, ServerId::Archives), Vec::new());
        assert_eq!(access_server(&corp, ServerId::Remote(0)), Vec::new());
    }
}
