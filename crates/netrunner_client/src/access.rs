//! The card being accessed, as the person deciding about it reads it:
//! the printed card, what it would cost to trash, whether it must be
//! stolen, and how much of the breach is still to come.
//!
//! **A name is not a card.** Both clients used to say `Accessing Send a
//! Message` and stop there — the terminal did not even say that much,
//! because an access is not a `PendingDecision` and so
//! `prose::decision_prompt` returned `None` and the actions pane kept its
//! default title. A person who has not memorised the card pool cannot
//! answer "steal, trash or pass?" from a name, and unlike every other
//! card in the game the accessed one has no tile on the board to click:
//! it is in HQ or R&D, face down, and the only place it exists is the
//! prompt. So the prompt shows the card.
//!
//! The engine already publishes everything needed —
//! `PublicAccessPhase::PendingChoice` carries the accessed card's
//! `CardId`, masked by `rules::masking` to the Runner (and to the Corp on
//! Archives, which is a public zone) — so this module reads the view and
//! looks the card up in the registry the client already holds. Nothing
//! here asks the engine for anything new.
//!
//! **Only the side being asked sees it.** The mask also names an Archives
//! card to the Corp, so a Corp-side modal was possible; it was rejected
//! because the modal exists to help someone *decide*, and the Corp has no
//! decision to make at a `PendingChoice`. They watch the run lane and the
//! log, as they do for every other Runner decision. The one access the
//! Corp can be asked — a `PendingInteractiveTrigger` whose `decider` is
//! the Corp — does show them the card.
//!
//! **`SelectNextCard` has no face here.** Choosing *which* of several
//! cards to access is a choice between cards rather than a decision about
//! one, and the buttons already name each candidate. A face per candidate
//! is the third list's item 5 (the right side carrying the card being
//! read), not this.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, Cost, Prohibition};
use netrunner_core::rules::{MaskedZone, PlayerAction, PublicAccessPhase, ServerId, Side};
use netrunner_core::view::ClientView;

use crate::card_face::Face;
use crate::prose;

/// Which access decision is parked, and what it is asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// The card is being accessed: steal it, trash it, or pass.
    Choice,
    /// The card is asking someone to pay before the access resolves
    /// (Snare!, Fetal AI). `can_pay` is the engine's own read of whether
    /// they can afford it.
    Trigger { cost: Cost, decider: Side, can_pay: bool },
}

/// One card being accessed, as its decider reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Access {
    pub server: ServerId,
    pub card: CardId,
    /// The printed card, laid out. [`crate::card_face::Face`] is the
    /// authority on where a number sits; this module never re-decides it.
    pub face: Face,
    pub stage: Stage,
    pub trash_cost: Option<u32>,
    pub steal_cost: Option<Cost>,
    pub mandatory_steal: bool,
    /// `ClientView::cannot(StealOrTrash)` — Ansel 1.0 and friends. Carried because it is the only thing that explains why a
    /// card with a printed trash cost is offering nothing but "pass",
    /// which is otherwise unreadable.
    pub steal_and_trash_blocked: bool,
    /// Cards still to come in this breach after this one.
    pub remaining: usize,
}

impl Access {
    /// The access the viewer is being asked about, or `None` — no run, no
    /// breach, the `SelectNextCard` step, a card the mask does not name
    /// to this viewer, or somebody else's decision.
    pub fn of(view: &ClientView, registry: &CardRegistry) -> Option<Access> {
        let run = view.active_run.as_ref()?;
        let access = run.access_state.as_ref()?;
        let (card, stage) = match &access.phase {
            PublicAccessPhase::SelectNextCard { .. } => return None,
            PublicAccessPhase::PendingChoice { card, .. } => (card.clone()?, Stage::Choice),
            PublicAccessPhase::PendingInteractiveTrigger { card, cost, decider, can_pay } => {
                (card.clone()?, Stage::Trigger { cost: cost.clone(), decider: *decider, can_pay: *can_pay })
            }
        };
        let asked = match &stage {
            Stage::Choice => Side::Runner,
            Stage::Trigger { decider, .. } => *decider,
        };
        if !view.viewer.is(asked) {
            return None;
        }
        let definition = registry.get(&card)?;
        let (trash_cost, steal_cost, mandatory_steal) = match &access.phase {
            PublicAccessPhase::PendingChoice { trash_cost, steal_cost, mandatory_steal, .. } => {
                (*trash_cost, steal_cost.clone(), *mandatory_steal)
            }
            _ => (None, None, false),
        };
        Some(Access {
            server: run.server,
            card,
            face: Face::of(definition),
            stage,
            trash_cost,
            steal_cost,
            mandatory_steal,
            steal_and_trash_blocked: view.cannot(Prohibition::StealOrTrash),
            remaining: zone_len(&access.unaccessed_cards),
        })
    }

    /// The heading over the card.
    pub fn title(&self) -> String {
        match &self.stage {
            Stage::Choice => format!("Accessing {}", self.face.title),
            Stage::Trigger { cost, decider, .. } => {
                format!("{} asks the {} to pay {}", self.face.title, side_word(*decider), prose::describe_cost(cost))
            }
        }
    }

    /// The facts under the card, one line each: what the decision costs,
    /// what the rules are forcing, and how much breach is left. Only what
    /// the card's own face does not already say — its printed trash cost
    /// is in the face's corner, so the line here is the *live* one, which
    /// a grid (Mahkota Langit) may have raised.
    pub fn facts(&self) -> Vec<String> {
        let mut facts = Vec::new();
        match &self.stage {
            Stage::Trigger { can_pay, .. } if !can_pay => facts.push("You cannot afford it".to_string()),
            Stage::Trigger { .. } => {}
            Stage::Choice => {
                if self.mandatory_steal {
                    facts.push("An agenda — it must be stolen".to_string());
                } else if let Some(cost) = &self.steal_cost {
                    facts.push(format!("Stealing it costs {}", prose::describe_cost(cost)));
                }
                if let Some(trash) = self.trash_cost {
                    facts.push(format!("Trashing it costs {trash} credits"));
                }
                if self.steal_and_trash_blocked {
                    facts.push("You may not steal or trash for the rest of this run".to_string());
                }
            }
        }
        if self.remaining > 0 {
            facts.push(format!("{} more card{} to access", self.remaining, if self.remaining == 1 { "" } else { "s" }));
        }
        facts
    }
}

/// The access actions among `actions`, in the order a person should read
/// them: what the access is *for* first, then the ways out, with passing
/// last because it is the one that gives the card up — a pass under the
/// cursor is a pass taken by a stray Enter.
///
/// Every other action is dropped: the caller is drawing the access
/// decision, and a client that needs the whole legal list still has it
/// (the terminal's actions pane, the desktop's play helper). This does
/// not filter what may be *submitted* — that is the engine's, per the
/// Session Rule.
pub fn ordered(actions: &[PlayerAction]) -> Vec<PlayerAction> {
    let mut found: Vec<PlayerAction> = actions.iter().filter(|action| rank(action).is_some()).cloned().collect();
    found.sort_by_key(|action| rank(action).unwrap_or(u8::MAX));
    found
}

/// `None` for anything that is not one of the six access actions.
fn rank(action: &PlayerAction) -> Option<u8> {
    match action {
        PlayerAction::StealAgenda { .. } => Some(0),
        PlayerAction::TrashAccessedCard { .. } => Some(1),
        PlayerAction::PayAccessTrigger { .. } => Some(2),
        PlayerAction::DeclineAccessTrigger { .. } => Some(3),
        PlayerAction::PassAccessedCard { .. } => Some(4),
        _ => None,
    }
}

fn zone_len(zone: &MaskedZone) -> usize {
    match zone {
        MaskedZone::Visible(cards) => cards.len(),
        MaskedZone::Hidden { count } => *count as usize,
    }
}

fn side_word(side: Side) -> &'static str {
    match side {
        Side::Corp => "Corp",
        Side::Runner => "Runner",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{GamePhase, GameState, PublicAccessState, PublicRunState, RunPhase, Viewer};
    use netrunner_core::rules::lingering::{Lingering, LingeringEffect, On, Until};
    use netrunner_core::view::build_client_view;

    fn registry() -> CardRegistry {
        crate::decks::sample_deck_registry()
    }

    /// A view parked at an access of `card`, seen by `viewer`.
    fn accessing(card: &str, server: ServerId, viewer: Viewer, registry: &CardRegistry) -> ClientView {
        let state = GameState::new(7);
        let mut view = build_client_view(&state, registry, viewer);
        view.phase = GamePhase::Action(Side::Runner);
        let definition = registry.get(&CardId(card.to_string())).expect("sample card");
        view.active_run = Some(PublicRunState {
            server,
            phase: RunPhase::AccessingCard,
            ice: Vec::new(),
            position: 0,
            access_state: Some(PublicAccessState {
                server,
                unaccessed_cards: MaskedZone::Hidden { count: 0 },
                resolved_cards: MaskedZone::Hidden { count: 0 },
                pending_install: None,
                phase: PublicAccessPhase::PendingChoice {
                    card: Some(CardId(card.to_string())),
                    trash_cost: definition.trash_cost,
                    mandatory_steal: definition.agenda_points.is_some(),
                    steal_cost: None,
                },
            }),
            jack_out_permitted: false,
            bad_publicity_credits: 0,
            bonus_run_credits: 0,
            redirect_on_approach: None,
        });
        view
    }

    /// The whole point: the card, not its name. An agenda says it must be
    /// stolen; an asset says what trashing costs.
    #[test]
    fn an_access_carries_the_printed_card_and_what_the_decision_costs() {
        let registry = registry();
        let view = accessing("send_a_message", ServerId::Hq, Viewer::Player(Side::Runner), &registry);
        let access = Access::of(&view, &registry).expect("the Runner is being asked");
        assert_eq!(access.title(), "Accessing Send a Message");
        assert!(access.mandatory_steal);
        assert!(access.facts().contains(&"An agenda — it must be stolen".to_string()));
        // The face is the printed card, so the numbers and the text are
        // all there without this module deciding any of it.
        assert_eq!(access.face.title, "Send a Message");
        assert!(access.face.numbers_line().contains("Advancement"), "{}", access.face.numbers_line());
        assert!(!access.face.lines(false).is_empty());

        let view = accessing("nico_campaign", ServerId::Hq, Viewer::Player(Side::Runner), &registry);
        let access = Access::of(&view, &registry).expect("the Runner is being asked");
        assert!(!access.mandatory_steal);
        let facts = access.facts();
        assert!(facts.iter().any(|fact| fact.starts_with("Trashing it costs")), "{facts:?}");
    }

    /// The modal is for whoever must decide. The Corp watching a breach
    /// is not deciding, even on Archives where the mask names the card.
    #[test]
    fn only_the_side_being_asked_is_shown_the_card() {
        let registry = registry();
        for server in [ServerId::Hq, ServerId::Archives] {
            let view = accessing("send_a_message", server, Viewer::Player(Side::Corp), &registry);
            assert!(Access::of(&view, &registry).is_none(), "the Corp has no decision at {server:?}");
        }
        let view = accessing("send_a_message", ServerId::Hq, Viewer::Spectator, &registry);
        assert!(Access::of(&view, &registry).is_none(), "a spectator decides nothing");
    }

    /// A card the mask did not name cannot be drawn, and the
    /// choose-which-card step is not an access of one.
    #[test]
    fn a_masked_card_and_the_selection_step_show_nothing() {
        let registry = registry();
        let mut view = accessing("send_a_message", ServerId::Hq, Viewer::Player(Side::Runner), &registry);
        let access = view.active_run.as_mut().unwrap().access_state.as_mut().unwrap();
        access.phase = PublicAccessPhase::PendingChoice { card: None, trash_cost: None, mandatory_steal: false, steal_cost: None };
        assert!(Access::of(&view, &registry).is_none(), "no card named, nothing to draw");

        let access = view.active_run.as_mut().unwrap().access_state.as_mut().unwrap();
        access.phase = PublicAccessPhase::SelectNextCard { selectable_cards: MaskedZone::Hidden { count: 2 } };
        assert!(Access::of(&view, &registry).is_none(), "choosing between cards is not accessing one");

        view.active_run = None;
        assert!(Access::of(&view, &registry).is_none());
    }

    /// How much breach is left, and the blocked-steal explanation that is
    /// otherwise invisible.
    #[test]
    fn the_facts_say_what_is_left_and_why_nothing_may_be_taken() {
        let registry = registry();
        let mut view = accessing("nico_campaign", ServerId::Hq, Viewer::Player(Side::Runner), &registry);
        // As the view carries it: a lingering effect, already known to hold.
        view.lingering.push(LingeringEffect {
            what: Lingering::Cannot(Prohibition::StealOrTrash),
            on: On::Player(Side::Runner),
            until: Until::EndOfRun,
            source: CardId("ansel_1_0".to_string()),
        });
        let run = view.active_run.as_mut().unwrap();
        run.access_state.as_mut().unwrap().unaccessed_cards = MaskedZone::Hidden { count: 1 };
        let access = Access::of(&view, &registry).expect("still the Runner's decision");
        assert_eq!(access.remaining, 1);
        let facts = access.facts();
        assert!(facts.contains(&"1 more card to access".to_string()), "{facts:?}");
        assert!(facts.iter().any(|fact| fact.contains("may not steal or trash")), "{facts:?}");
    }

    /// An interactive trigger names its payer, and says when they cannot.
    #[test]
    fn an_interactive_trigger_names_who_must_pay() {
        let registry = registry();
        let mut view = accessing("send_a_message", ServerId::Hq, Viewer::Player(Side::Runner), &registry);
        let access = view.active_run.as_mut().unwrap().access_state.as_mut().unwrap();
        access.phase = PublicAccessPhase::PendingInteractiveTrigger {
            card: Some(CardId("send_a_message".to_string())),
            cost: Cost::Credits(2),
            decider: Side::Runner,
            can_pay: false,
        };
        let access = Access::of(&view, &registry).expect("the Runner is the decider");
        assert_eq!(access.title(), "Send a Message asks the Runner to pay 2 credits");
        assert!(access.facts().contains(&"You cannot afford it".to_string()));
    }

    /// Pass sorts last so a stray Enter cannot give a card away, and
    /// nothing that is not an access action survives.
    #[test]
    fn the_access_actions_read_in_order_with_pass_last() {
        let card_id = CardId("send_a_message".to_string());
        let actions = vec![
            PlayerAction::PassAccessedCard { card_id: card_id.clone() },
            PlayerAction::EndTurn,
            PlayerAction::TrashAccessedCard { card_id: card_id.clone() },
            PlayerAction::StealAgenda { card_id: card_id.clone() },
        ];
        assert_eq!(
            ordered(&actions),
            vec![
                PlayerAction::StealAgenda { card_id: card_id.clone() },
                PlayerAction::TrashAccessedCard { card_id: card_id.clone() },
                PlayerAction::PassAccessedCard { card_id },
            ]
        );
        assert!(ordered(&[PlayerAction::EndTurn]).is_empty());
    }
}
