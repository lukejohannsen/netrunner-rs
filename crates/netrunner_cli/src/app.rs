//! The interactive TUI's state: the human seat's latest `ClientView`
//! (received over a channel from a background `netrunner_server::
//! MatchSession` task — never the raw `GameState`), UI selection state, and
//! the most recent rejection/game-end notice.
//!
//! Exactly one side is the human seat; the other is always bot-controlled
//! (see `config::Config::corp`'s doc comment) — under real per-side
//! masking there's no coherent way for a single local terminal to
//! represent "both sides, simultaneously, from each one's own point of
//! view," so this app doesn't try to.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{InstallId, InstallSlot, PendingDecision, PlayerAction, Side};
use netrunner_core::view::ClientView;
use netrunner_server::protocol::GameEndReason;
use netrunner_server::{ClientMessage, HistoryEntry, ServerMessage};

pub struct App {
    pub registry: CardRegistry,
    pub human_side: Side,
    tx: mpsc::UnboundedSender<ClientMessage>,
    rx: mpsc::UnboundedReceiver<ServerMessage>,
    /// `None` until the first `StateUpdate` arrives from the match session
    /// (should be near-instant — the session broadcasts its initial state
    /// before waiting on anything).
    pub view: Option<ClientView>,
    pub selected: usize,
    pub should_quit: bool,
    pub last_rejection: Option<String>,
    pub game_ended: Option<(Side, GameEndReason)>,
    /// Rendered log of every resolved action, from `ServerMessage::
    /// ActionLog`. Remote play had no log at all until the match driver
    /// grew a `MatchHistory` the server could forward.
    pub action_log: Vec<String>,
}

/// Cap on retained log lines, shared by both TUI paths.
pub const MAX_LOG_LINES: usize = 200;

/// Appends one resolved action to a capped log. Shared by the remote path
/// (`App`, fed by `ServerMessage::ActionLog`) and the local one
/// (`tui::LocalUiState`, fed straight off `Session`'s history) so the two
/// render identically.
pub fn push_log_line(log: &mut Vec<String>, entry: &HistoryEntry, registry: &CardRegistry, view: Option<&ClientView>) {
    log.push(format!(
        "[turn {}] {:?}: {}",
        entry.turn_number,
        entry.side,
        describe_action(&entry.action, registry, view)
    ));
    if log.len() > MAX_LOG_LINES {
        let excess = log.len() - MAX_LOG_LINES;
        log.drain(0..excess);
    }
}

impl App {
    pub fn new(
        registry: CardRegistry,
        human_side: Side,
        tx: mpsc::UnboundedSender<ClientMessage>,
        rx: mpsc::UnboundedReceiver<ServerMessage>,
    ) -> Self {
        let mut app = App {
            registry,
            human_side,
            tx,
            rx,
            view: None,
            selected: 0,
            should_quit: false,
            last_rejection: None,
            game_ended: None,
            action_log: Vec::new(),
        };
        app.drain_messages();
        app
    }

    /// Non-blocking drain of every message the match session has sent
    /// since the last poll — called once at construction and once per TUI
    /// render tick, mirroring the ~100ms `event::poll` cadence the render
    /// loop already uses for keyboard input.
    pub fn drain_messages(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                ServerMessage::StateUpdate(view) => {
                    if self.selected >= view.legal_actions.len() {
                        self.selected = 0;
                    }
                    self.view = Some(*view);
                    self.last_rejection = None;
                }
                ServerMessage::ActionLog(entry) => {
                    push_log_line(&mut self.action_log, &entry, &self.registry, self.view.as_ref())
                }
                ServerMessage::ActionRejected { reason } => self.last_rejection = Some(reason),
                ServerMessage::GameEnded { winner, reason } => self.game_ended = Some((winner, reason)),
                ServerMessage::MatchJoined { .. } => {}
            }
        }
    }

    pub fn is_game_over(&self) -> bool {
        self.game_ended.is_some()
    }

    pub fn legal_actions(&self) -> &[PlayerAction] {
        self.view.as_ref().map_or(&[], |view| view.legal_actions.as_slice())
    }

    fn submit_selected_action(&mut self) {
        let Some(action) = self.legal_actions().get(self.selected).cloned() else { return };
        let _ = self.tx.send(ClientMessage::SubmitAction(action));
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.is_game_over() {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.should_quit = true;
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.submit_selected_action(),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.legal_actions().len();
        if len == 0 {
            return;
        }
        let next = (self.selected as i32 + delta).rem_euclid(len as i32);
        self.selected = next as usize;
    }
}

/// What `tui::{draw_header, draw_board, draw_actions}` need to render one
/// frame — implemented by `App` (the remote/channel-backed path) and by
/// `tui::LocalUiState` (the local `netrunner_single_player`-backed path),
/// so both share the same rendering code instead of duplicating it.
pub trait RenderableView {
    fn registry(&self) -> &CardRegistry;
    fn human_side(&self) -> Side;
    fn view(&self) -> Option<&ClientView>;
    fn selected(&self) -> usize;
    /// Labels for the actions on offer, in the order they are listed.
    /// Under a lesson this is the gated subset, not all of
    /// `view.legal_actions` — see `tui::LocalUiState::offered_actions`.
    fn legal_action_labels(&self) -> Vec<String>;
    /// The action the highlight sits on, for the coaching panel to explain.
    fn selected_action(&self) -> Option<PlayerAction>;
    /// The running action log. Both paths have one now, so both render the
    /// same four-region layout.
    fn action_log(&self) -> &[String];
    /// The engine's reason for refusing the last submission, until the
    /// next state arrives. `None` on a path that has none to show.
    fn last_rejection(&self) -> Option<&str> {
        None
    }
    /// Lesson coaching to render beside the board; `None` outside a lesson,
    /// which is the only case the remote path ever has.
    fn coaching(&self) -> Option<&Coaching> {
        None
    }
    /// An open popup, which owns the keyboard until dismissed.
    fn modal(&self) -> Option<&Modal> {
        None
    }
}

/// A popup that owns the keyboard until dismissed: lesson intros and
/// outros, the game-over notice. The one modal the TUI had before this —
/// game over — was a `Clear` + bordered `Paragraph` with its own key loop
/// and no routing; `tui::draw_modal` generalises the drawing and
/// `tui::prompt_human` routes keys to an open one first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modal {
    pub title: String,
    pub body: String,
    pub footer: String,
}

impl Modal {
    pub fn new(title: &str, body: &str, footer: &str) -> Self {
        Self { title: title.to_string(), body: body.to_string(), footer: footer.to_string() }
    }
}

/// What the coaching panel shows during one lesson step — a projection of
/// `netrunner_core::tutorial::Step` plus the two UI facts it needs (which
/// step this is, and whether the escape hatch is open).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coaching {
    pub title: String,
    /// One-based, for display.
    pub step: usize,
    pub total: usize,
    pub prose: String,
    pub hint: Option<String>,
    /// Whether the step's filter matched at least one legal action. When
    /// it did not, the list falls back to every legal action and the panel
    /// says so — the lesson's gate can narrow the list, never empty it.
    pub gated: bool,
    /// The escape hatch is open: the player asked to see every legal
    /// action rather than the step's subset.
    pub showing_all: bool,
}

impl RenderableView for App {
    fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    fn human_side(&self) -> Side {
        self.human_side
    }

    fn view(&self) -> Option<&ClientView> {
        self.view.as_ref()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn legal_action_labels(&self) -> Vec<String> {
        self.legal_actions().iter().map(|action| describe_action(action, &self.registry, self.view.as_ref())).collect()
    }

    fn selected_action(&self) -> Option<PlayerAction> {
        self.legal_actions().get(self.selected).cloned()
    }

    fn action_log(&self) -> &[String] {
        &self.action_log
    }

    fn last_rejection(&self) -> Option<&str> {
        self.last_rejection.as_deref()
    }
}

/// Human-readable label for a `PlayerAction`, resolving `CardId`s to
/// registry titles where available.
///
/// `view` is what resolves an `InstallId` to something a person can read.
/// It is optional because the action log has no view to hand — see
/// `install_label` for what each case renders.
pub fn describe_action(action: &PlayerAction, registry: &CardRegistry, view: Option<&ClientView>) -> String {
    let title = |card_id: &netrunner_core::dsl::CardId| -> String {
        registry.get(card_id).map(|c| c.title.clone()).unwrap_or_else(|| card_id.0.clone())
    };

    // An `InstallId` names a position on the table, not a card, so this
    // renders whatever the *viewer* is entitled to see there:
    //
    // - a card they can identify → its title;
    // - a card the view masks (an unrezzed Corp install) → where it sits,
    //   never its title. Naming it here would reintroduce, in the UI, the
    //   exact leak `InstallId` exists to close;
    // - no view, or an install no longer on the table (a scored agenda) →
    //   the bare id. Only the action log hits this, and only for a card
    //   that has already left; a log line naming it would need the
    //   recorded `GameEvent`s rather than the action alone.
    let install_label = |id: &InstallId| -> String {
        let Some(view) = view else { return format!("install #{}", id.0) };
        for server in &view.corp.servers {
            for card in server.ice.iter().chain(server.root.iter()) {
                if card.install_id != *id {
                    continue;
                }
                return match &card.card {
                    Some(card_id) => title(card_id),
                    None => {
                        let kind = if card.slot == InstallSlot::Ice { "ice" } else { "card" };
                        format!("the unrezzed {kind} at {:?}", server.server)
                    }
                };
            }
        }
        match view.runner.rig.iter().find(|c| c.install_id == *id) {
            Some(rig_card) => title(&rig_card.card),
            None => format!("install #{}", id.0),
        }
    };

    match action {
        PlayerAction::GainCreditClick { side } => format!("Gain 1 credit ({side:?})"),
        PlayerAction::DrawCardClick { .. } => "Draw a card".to_string(),
        PlayerAction::InstallCard { card_id, zone, slot } => {
            format!("Install {} into {:?} ({:?})", title(card_id), zone, slot)
        }
        PlayerAction::RezIce { ice } => format!("Rez {}", install_label(ice)),
        PlayerAction::InitiateRun { server } => format!("Run {server:?}"),
        PlayerAction::ContinueRun => "Continue run".to_string(),
        PlayerAction::JackOut => "Jack out".to_string(),
        PlayerAction::CompleteRun => "Complete run".to_string(),
        PlayerAction::PlayEvent { card_id } => format!("Play {}", title(card_id)),
        PlayerAction::PlayOperation { card_id } => format!("Play {}", title(card_id)),
        PlayerAction::InstallHardware { card_id } => format!("Install {}", title(card_id)),
        PlayerAction::InstallProgram { card_id, .. } => format!("Install {}", title(card_id)),
        PlayerAction::InstallResource { card_id } => format!("Install {}", title(card_id)),
        PlayerAction::InstallProgramOnIce { card_id, host, .. } => {
            format!("Install {} onto {}", title(card_id), install_label(host))
        }
        PlayerAction::BreakSubroutineWithClick { ice_id, subroutine_index } => {
            format!("Break subroutine {subroutine_index} on {} (spend a click)", title(ice_id))
        }
        PlayerAction::EndTurn => "End turn".to_string(),
        PlayerAction::DiscardCard { card_id } => format!("Discard {}", title(card_id)),
        PlayerAction::KeepHand => "Keep hand".to_string(),
        PlayerAction::TakeMulligan => "Mulligan".to_string(),
        PlayerAction::ActivateAbility { target, ability_index } => {
            format!("Activate ability {ability_index} on {}", install_label(target))
        }
        PlayerAction::AdvanceCard { target } => format!("Advance {}", install_label(target)),
        PlayerAction::ScoreAgenda { target } => format!("Score {}", install_label(target)),
        PlayerAction::RemoveTag => "Remove a tag".to_string(),
        // Spells out the click cost: it is the Corp's whole turn, which is
        // not obvious from the name alone at the point of choosing it.
        PlayerAction::PurgeVirusCounters => "Purge virus counters (3 clicks)".to_string(),
        PlayerAction::TrashResource { target } => format!("Trash {}", install_label(target)),
        PlayerAction::SelectCardToAccess { card_id } => format!("Access {}", title(card_id)),
        PlayerAction::StealAgenda { card_id } => format!("Steal {}", title(card_id)),
        PlayerAction::TrashAccessedCard { card_id } => format!("Trash {}", title(card_id)),
        PlayerAction::PassAccessedCard { card_id } => format!("Pass on {}", title(card_id)),
        PlayerAction::PayAccessTrigger { card_id } => format!("Pay to avoid {}'s trigger", title(card_id)),
        PlayerAction::DeclineAccessTrigger { card_id } => format!("Decline {}'s trigger", title(card_id)),
        PlayerAction::PassPriority { side } => format!("Pass priority ({side:?})"),
        PlayerAction::SubmitCorpTraceBid { amount } => format!("Bid {amount} (Corp trace)"),
        PlayerAction::SubmitRunnerTraceBid { amount } => format!("Bid {amount} (Runner trace)"),
        PlayerAction::AcceptPendingPaidChoice { cost_option_index: None } => "Accept".to_string(),
        PlayerAction::AcceptPendingPaidChoice { cost_option_index: Some(i) } => format!("Accept (option {i})"),
        PlayerAction::DeclinePendingPaidChoice => "Decline".to_string(),
        PlayerAction::ResolvePendingChoice { option_index } => format!("Choose option {option_index}"),
        // A position, deliberately not resolved to a card: the zone it
        // indexes may hold cards this viewer cannot identify, and the
        // selection prompt renders the zone alongside this list anyway.
        PlayerAction::ToggleCardSelection { position } => format!("Toggle selection of card {position}"),
        PlayerAction::ConfirmCardSelection => "Confirm selection".to_string(),
        PlayerAction::ChooseServerForPendingDecision { server } => format!("Choose {server:?}"),
        // The action is a position into the parked trigger list, so the
        // card and trigger it names come from the view's `pending_decision`
        // — which is pass-through for this variant and holds only the
        // chooser's own cards, so nothing is shown here that the viewer was
        // not already shown. The trigger is named because one card can
        // have several pending at once.
        PlayerAction::ChooseTriggerToResolve { index } => {
            let due = view.and_then(|v| match &v.pending_decision {
                Some(PendingDecision::ChooseTriggerOrder { pending, .. }) => pending.get(*index),
                _ => None,
            });
            match due {
                Some(due) => format!("Resolve {} ({:?}) first", title(&due.card), due.trigger),
                None => format!("Resolve trigger #{index} first"),
            }
        }
    }
}

/// One sentence on what an action *does* and what it costs — the
/// pedagogical sibling of `describe_action`, which only labels. Shown in a
/// lesson's coaching panel for whichever action the highlight sits on, so a
/// new player can read the list without already knowing the game.
///
/// Exhaustive over every variant on purpose: a new action that ships
/// without an explanation is a compile error, not a silent gap in the
/// tutorial. `PurgeVirusCounters`' click-cost label in `describe_action`
/// was the precedent for spelling out a cost at the point of choosing.
pub fn explain_action(action: &PlayerAction, registry: &CardRegistry, view: Option<&ClientView>) -> String {
    let title = |card_id: &netrunner_core::dsl::CardId| -> String {
        registry.get(card_id).map(|c| c.title.clone()).unwrap_or_else(|| card_id.0.clone())
    };
    let _ = view;
    match action {
        PlayerAction::GainCreditClick { .. } => "Spend 1 click to take 1 credit from the bank. Always available; the slowest way to make money.".to_string(),
        PlayerAction::DrawCardClick { side } => match side {
            Side::Corp => "Spend 1 click to draw the top card of R&D into HQ.".to_string(),
            Side::Runner => "Spend 1 click to draw the top card of your stack into your grip.".to_string(),
        },
        PlayerAction::InstallCard { card_id, zone, slot } => {
            let what = match slot {
                InstallSlot::Ice => "as ice protecting",
                InstallSlot::Root => "face down in the root of",
            };
            format!(
                "Spend 1 click to install {} {what} {zone:?}. Installing a new remote server creates it. Ice costs 1 credit per piece already protecting that server; the card stays unrezzed (and hidden) until you pay to rez it.",
                title(card_id)
            )
        }
        PlayerAction::RezIce { .. } => "Pay the card's rez cost to turn it face up. Ice only stops the Runner once it is rezzed, and you usually rez it as they approach it.".to_string(),
        PlayerAction::InitiateRun { server } => format!(
            "Spend 1 click to run {server:?}: approach each piece of ice protecting it in turn, and if you get past them all, breach the server and access its cards."
        ),
        PlayerAction::ContinueRun => "Move to the next phase of the run: approach the next piece of ice, or approach the server if there is none left.".to_string(),
        PlayerAction::JackOut => "End the run voluntarily, keeping your credits. You can jack out after passing a piece of ice or on reaching the server; you cannot while encountering ice, or once the run is successful.".to_string(),
        PlayerAction::CompleteRun => "Commit to the server: the run becomes successful, and you breach it and access its cards. This is the point of no return — jack out before it, not after.".to_string(),
        PlayerAction::PlayEvent { card_id } => format!(
            "Spend 1 click and the play cost to play {}: resolve its text, then it goes to the heap.",
            title(card_id)
        ),
        PlayerAction::PlayOperation { card_id } => format!(
            "Spend 1 click and the play cost to play {}: resolve its text, then it goes to Archives.",
            title(card_id)
        ),
        PlayerAction::InstallHardware { card_id } => format!("Spend 1 click and its install cost to install {} in your rig. Hardware stays in play.", title(card_id)),
        PlayerAction::InstallProgram { card_id } => format!(
            "Spend 1 click and its install cost to install {}. Programs take memory (MU); you have 4 MU by default, and an icebreaker is how you get through ice.",
            title(card_id)
        ),
        PlayerAction::InstallResource { card_id } => format!(
            "Spend 1 click and its install cost to install {}. Resources stay in play but can be trashed by the Corp if you are tagged.",
            title(card_id)
        ),
        PlayerAction::InstallProgramOnIce { card_id, .. } => format!("Install {} hosted on a piece of ice — a trojan works on the ice it lives on.", title(card_id)),
        PlayerAction::BreakSubroutineWithClick { .. } => "Spend a click to break one subroutine on this ice (a card ability allows it here). A broken subroutine does not fire.".to_string(),
        PlayerAction::EndTurn => "End your turn. Unspent clicks are lost; if you hold more cards than your hand size (5), you discard down first.".to_string(),
        PlayerAction::DiscardCard { card_id } => format!("Discard {} to get down to your maximum hand size.", title(card_id)),
        PlayerAction::KeepHand => "Keep these 5 cards as your opening hand.".to_string(),
        PlayerAction::TakeMulligan => "Shuffle this hand back and draw 5 new cards. You only get one mulligan, and you keep whatever comes.".to_string(),
        PlayerAction::ActivateAbility { .. } => "Use one of this card's paid abilities, paying its cost — an icebreaker's abilities boost its strength or break subroutines during an encounter.".to_string(),
        PlayerAction::AdvanceCard { .. } => "Spend 1 click and 1 credit to place an advancement token. An agenda scores once it has as many tokens as its advancement requirement.".to_string(),
        PlayerAction::ScoreAgenda { .. } => "Score this fully advanced agenda: its points go to your score area, and 7 points (6 in the starter game) wins.".to_string(),
        PlayerAction::RemoveTag => "Spend 1 click and 2 credits to remove a tag. While tagged, the Corp can trash your resources.".to_string(),
        PlayerAction::PurgeVirusCounters => "Spend all 3 clicks to remove every virus counter in play.".to_string(),
        PlayerAction::ChooseTriggerToResolve { .. } => "Several of your cards want to trigger at once; choose which resolves first.".to_string(),
        PlayerAction::TrashResource { .. } => "Spend 1 click and 2 credits to trash one of the tagged Runner's resources.".to_string(),
        PlayerAction::SelectCardToAccess { card_id } => format!("Look at {} — accessing a card means seeing it, and stealing it if it is an agenda.", title(card_id)),
        PlayerAction::StealAgenda { card_id } => format!("Steal {}: an accessed agenda goes to your score area and its points count for you.", title(card_id)),
        PlayerAction::TrashAccessedCard { card_id } => format!("Pay the trash cost to send {} to Archives instead of leaving it where it is.", title(card_id)),
        PlayerAction::PassAccessedCard { card_id } => format!("Leave {} where it is and move on.", title(card_id)),
        PlayerAction::PayAccessTrigger { card_id } => format!("Pay the cost {} asks for on access.", title(card_id)),
        PlayerAction::DeclineAccessTrigger { card_id } => format!("Decline to pay for {}'s access ability.", title(card_id)),
        PlayerAction::PassPriority { .. } => "You have nothing to do in this paid-ability window; let play continue.".to_string(),
        PlayerAction::SubmitCorpTraceBid { amount } => format!("Spend {amount} credits to raise your trace strength."),
        PlayerAction::SubmitRunnerTraceBid { amount } => format!("Spend {amount} credits to raise your link strength against the trace."),
        PlayerAction::AcceptPendingPaidChoice { .. } => "Pay the offered cost to get the card's effect.".to_string(),
        PlayerAction::DeclinePendingPaidChoice => "Do not pay; the optional effect does not happen.".to_string(),
        PlayerAction::ResolvePendingChoice { .. } => "Pick this option for the card that is asking you to choose.".to_string(),
        PlayerAction::ToggleCardSelection { .. } => "Add or remove this card from the selection a card effect is asking for.".to_string(),
        PlayerAction::ConfirmCardSelection => "Confirm the cards you selected.".to_string(),
        PlayerAction::ChooseServerForPendingDecision { server } => format!("Choose {server:?} as the server this card effect applies to."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::RandomAgent;
    use netrunner_core::dsl::CardId;
    use netrunner_core::rules::ServerId;

    /// One instance per variant, mirroring `PlayerAction::VARIANT_NAMES`'
    /// own test: an explanation is required for every action a lesson can
    /// offer, and it has to say more than the label does.
    #[test]
    fn every_action_has_an_explanation_that_says_more_than_its_label() {
        let card = || CardId("hedge_fund".to_string());
        let install = InstallId(1);
        let actions = vec![
            PlayerAction::GainCreditClick { side: Side::Corp },
            PlayerAction::DrawCardClick { side: Side::Runner },
            PlayerAction::InstallCard { card_id: card(), zone: ServerId::Remote(1), slot: InstallSlot::Ice },
            PlayerAction::RezIce { ice: install },
            PlayerAction::InitiateRun { server: ServerId::Hq },
            PlayerAction::ContinueRun,
            PlayerAction::JackOut,
            PlayerAction::CompleteRun,
            PlayerAction::PlayEvent { card_id: card() },
            PlayerAction::PlayOperation { card_id: card() },
            PlayerAction::InstallHardware { card_id: card() },
            PlayerAction::InstallProgram { card_id: card() },
            PlayerAction::InstallResource { card_id: card() },
            PlayerAction::InstallProgramOnIce { card_id: card(), host: install },
            PlayerAction::BreakSubroutineWithClick { ice_id: card(), subroutine_index: 0 },
            PlayerAction::EndTurn,
            PlayerAction::DiscardCard { card_id: card() },
            PlayerAction::KeepHand,
            PlayerAction::TakeMulligan,
            PlayerAction::ActivateAbility { target: install, ability_index: 0 },
            PlayerAction::AdvanceCard { target: install },
            PlayerAction::ScoreAgenda { target: install },
            PlayerAction::RemoveTag,
            PlayerAction::PurgeVirusCounters,
            PlayerAction::ChooseTriggerToResolve { index: 0 },
            PlayerAction::TrashResource { target: install },
            PlayerAction::SelectCardToAccess { card_id: card() },
            PlayerAction::StealAgenda { card_id: card() },
            PlayerAction::TrashAccessedCard { card_id: card() },
            PlayerAction::PassAccessedCard { card_id: card() },
            PlayerAction::PayAccessTrigger { card_id: card() },
            PlayerAction::DeclineAccessTrigger { card_id: card() },
            PlayerAction::PassPriority { side: Side::Corp },
            PlayerAction::SubmitCorpTraceBid { amount: 2 },
            PlayerAction::SubmitRunnerTraceBid { amount: 1 },
            PlayerAction::AcceptPendingPaidChoice { cost_option_index: None },
            PlayerAction::DeclinePendingPaidChoice,
            PlayerAction::ResolvePendingChoice { option_index: 0 },
            PlayerAction::ToggleCardSelection { position: 0 },
            PlayerAction::ConfirmCardSelection,
            PlayerAction::ChooseServerForPendingDecision { server: ServerId::RnD },
        ];
        assert_eq!(actions.len(), PlayerAction::VARIANT_NAMES.len(), "one instance per variant");
        let registry = CardRegistry::new();
        for action in &actions {
            let explanation = explain_action(action, &registry, None);
            let label = describe_action(action, &registry, None);
            assert!(explanation.len() > label.len(), "{action:?}: {explanation:?} should say more than {label:?}");
            assert!(explanation.ends_with('.'), "{action:?}: an explanation is a sentence: {explanation:?}");
        }
    }

    use netrunner_core::rules::GameState;
    use netrunner_server::{MatchSession, PlayerSlot};

    use crate::decks;

    fn setup() -> (GameState, CardRegistry) {
        let registry = decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").expect("built-in deck").to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").expect("built-in deck").to_deck();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 1).expect("legal decks set up cleanly");
        (state, registry)
    }

    /// The action is a position; the label has to say which card and which
    /// of its triggers that position means, or two entries for one card
    /// render as identical rows.
    #[test]
    fn a_trigger_order_pick_is_labelled_with_its_card_and_trigger() {
        use netrunner_core::dsl::{CardId, Trigger};
        use netrunner_core::rules::DeferredTrigger;
        use netrunner_core::view::build_client_view;

        let (mut state, registry) = setup();
        let due = |trigger: Trigger| DeferredTrigger { install: None, target_install: None,
            card: CardId("docklands_pass".to_string()),
            trigger,
            target: None,
            event: None,
        };
        state.pending_decision = Some(PendingDecision::ChooseTriggerOrder {
            chooser: Side::Runner,
            pending: vec![due(Trigger::OnSuccessfulRun), due(Trigger::OnSuccessfulRunOnHq)],
            resume: netrunner_core::rules::PendingChoiceResume::None,
        });
        let view = build_client_view(&state, &registry, Side::Runner);

        assert_eq!(
            describe_action(&PlayerAction::ChooseTriggerToResolve { index: 1 }, &registry, Some(&view)),
            "Resolve Docklands Pass (OnSuccessfulRunOnHq) first"
        );
        assert_eq!(
            describe_action(&PlayerAction::ChooseTriggerToResolve { index: 1 }, &registry, None),
            "Resolve trigger #1 first",
            "with no view (the action log) the position is all there is to show"
        );
    }

    fn spawn_session(state: GameState, registry: CardRegistry, corp_slot: PlayerSlot, runner_slot: PlayerSlot) {
        let session = MatchSession::new(state, registry, corp_slot, runner_slot);
        tokio::spawn(session.run());
    }

    #[tokio::test]
    async fn human_seat_receives_its_own_view_and_can_submit_an_action() {
        let (state, registry) = setup();
        let (server_tx, app_rx) = mpsc::unbounded_channel();
        let (app_tx, server_rx) = mpsc::unbounded_channel();
        let corp_slot = PlayerSlot::Channel { tx: server_tx, rx: server_rx };
        let runner_slot = PlayerSlot::Bot(Box::new(RandomAgent::new(2)));

        spawn_session(state, registry.clone(), corp_slot, runner_slot);

        let mut app = App::new(registry, Side::Corp, app_tx, app_rx);
        // Give the background task a moment to deliver the initial view.
        for _ in 0..50 {
            if app.view.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            app.drain_messages();
        }

        let (initial_phase, initial_credits) = {
            let view = app.view.as_ref().expect("initial view delivered");
            assert_eq!(view.side, Side::Corp);
            (view.phase, view.corp.credits)
        };
        assert!(!app.legal_actions().is_empty());
        assert!(app.legal_actions().iter().all(|action| matches!(action, PlayerAction::KeepHand | PlayerAction::TakeMulligan)));

        app.handle_key(KeyEvent::from(KeyCode::Enter));
        for _ in 0..50 {
            app.drain_messages();
            if app.view.as_ref().is_some_and(|v| v.phase != initial_phase || v.corp.credits != initial_credits) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
}
