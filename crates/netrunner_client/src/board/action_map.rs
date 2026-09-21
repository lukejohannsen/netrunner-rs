//! Every legal action, labelled and tied to the thing on the board a
//! person would click to mean it.
//!
//! **The flat list is the contract; the board is a convenience.** A
//! client must be able to offer every element of `view.legal_actions`
//! without the board, because a card the layout could not place is still
//! playable (AGENTS.md §5). So the map is the legal list in order, each
//! entry with its label (`actions::describe_action`, the words the
//! terminal uses) and the targets it belongs to — a hand card, an
//! installed card, a server, one of the Runner's piles, a selection
//! position, or nothing on the board. Clicking a target is a *second*
//! route to the same entry; the `for_*` lookups are indices into the one
//! list, never a second list.
//!
//! **The entries no target reaches are split two ways.** The basic
//! actions a player takes every turn — a credit, a draw, ending the
//! turn, purging or removing a tag, passing priority, and the run's
//! continue / jack out / complete — are a fixed [`Control`] bar, drawn
//! always and greyed when the engine does not list them, so a person
//! never hunts for "End turn". Everything else with no target is a
//! [`ActionMap::decisions`]: the mulligan, a trace bid, a paid choice,
//! an access decision, a selection to confirm — the thing the prompt is
//! asking, listed under it. Together with the targeted entries that is
//! every index exactly once — except a second copy of a card in a
//! selection, which the button for the first copy stands for
//! (`selection` says why) — and a flat panel of the whole list (the
//! "play helper") is an aid a person turns on, not the way in.
//!
//! Nothing here decides legality: an action is in the map because the
//! engine put it in `legal_actions`, and `MatchHandle::submit` sends it
//! back unfiltered.

use std::collections::BTreeSet;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{
    GamePhase, InstallId, PendingDecision, PlayerAction, PublicAccessPhase, RunPhase, ServerId, Side, WouldHappen,
};
use netrunner_core::view::{ClientView, ServerView};

use crate::actions::{card_title, describe_action, explain_action};
use crate::board::affordance::{self, Affordance};
use crate::prose;
use crate::placement::Placement;
use crate::selection::Selection;

/// The thing on the board an entry belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A card in the viewer's own hand.
    HandCard(CardId),
    /// An installed card, by the handle both sides can see.
    Install(InstallId),
    /// A side's identity card: its abilities are activated by the
    /// engine's identity handle, which is not on the board.
    Identity(Side),
    /// A server: a run on it, an install into it, a choice of it.
    Server(ServerId),
    /// A position in the zone a `ChooseCards` prompt selects from.
    Position(usize),
    /// The Runner's stack or heap, or a side's score area — a zone a
    /// click opens, as the Corp's centrals are opened through `Server`.
    Pile(Pile),
}

/// The piles that are not servers: the Runner's two, and each side's
/// score area. The Corp's deck and discard are servers
/// (`ServerId::RnD`, `ServerId::Archives`) and need no second name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pile {
    Stack,
    Heap,
    /// The agendas a side has scored (the Corp) or stolen (the Runner),
    /// opened from the HUD's Agendas readout. Nothing in the action map
    /// targets it: a scored agenda's ability is on `Target::Install`,
    /// by the handle the agenda kept.
    Agendas(Side),
}

impl Pile {
    pub fn name(self) -> &'static str {
        match self {
            Pile::Stack => "Stack",
            Pile::Heap => "Heap",
            Pile::Agendas(Side::Corp) => "Agendas scored",
            Pile::Agendas(Side::Runner) => "Agendas stolen",
        }
    }
}

/// A basic action with a fixed place on the board's control bar.
///
/// Which side has which, and in what order, is decided here rather than
/// in a screen so both clients draw the same bar and a test can check
/// that every control resolves to at most one entry. The run trio is on
/// the Runner's bar outside a run too — greyed, so the bar never
/// reflows when a run starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    GainCredit,
    Draw,
    EndTurn,
    PurgeViruses,
    RemoveTag,
    PassPriority,
    ContinueRun,
    JackOut,
    CompleteRun,
}

impl Control {
    const CORP: [Control; 5] = [Control::GainCredit, Control::Draw, Control::PurgeViruses, Control::EndTurn, Control::PassPriority];
    const RUNNER: [Control; 8] =
        [Control::GainCredit, Control::Draw, Control::RemoveTag, Control::EndTurn, Control::PassPriority, Control::ContinueRun, Control::JackOut, Control::CompleteRun];

    /// The bar for `side`, in order.
    pub fn for_side(side: Side) -> &'static [Control] {
        match side {
            Side::Corp => &Self::CORP,
            Side::Runner => &Self::RUNNER,
        }
    }

    /// The button's word. Shorter than `describe_action`'s label, which
    /// names the side and the click cost; the bar has no room and the
    /// side is the viewer's own.
    pub fn label(self) -> &'static str {
        match self {
            Control::GainCredit => "Take 1 credit",
            Control::Draw => "Draw a card",
            Control::EndTurn => "End turn",
            Control::PurgeViruses => "Purge viruses",
            Control::RemoveTag => "Remove a tag",
            Control::PassPriority => "Pass priority",
            Control::ContinueRun => "Continue run",
            Control::JackOut => "Jack out",
            Control::CompleteRun => "Complete run",
        }
    }

    /// Whether `action` is what this control means. The side field is
    /// ignored: the bar is the viewer's, and the map holds only the
    /// viewer's legal actions.
    pub fn matches(self, action: &PlayerAction) -> bool {
        matches!(
            (self, action),
            (Control::GainCredit, PlayerAction::GainCreditClick { .. })
                | (Control::Draw, PlayerAction::DrawCardClick { .. })
                | (Control::EndTurn, PlayerAction::EndTurn)
                | (Control::PurgeViruses, PlayerAction::PurgeVirusCounters)
                | (Control::RemoveTag, PlayerAction::RemoveTag)
                | (Control::PassPriority, PlayerAction::PassPriority { .. })
                | (Control::ContinueRun, PlayerAction::ContinueRun)
                | (Control::JackOut, PlayerAction::JackOut)
                | (Control::CompleteRun, PlayerAction::CompleteRun)
        )
    }

    /// Whether some control, on either bar, means `action`.
    fn any_matches(action: &PlayerAction) -> bool {
        Self::RUNNER.iter().chain(Self::CORP.iter()).any(|control| control.matches(action))
    }
}

/// One legal action, as the panel lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionEntry {
    pub action: PlayerAction,
    /// `describe_action`'s label.
    pub label: String,
    /// The targets a click could mean this by; empty for an action that
    /// belongs to nothing on the board (end turn, keep hand, pass).
    pub targets: Vec<Target>,
}

/// `view.legal_actions`, labelled and indexed by target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActionMap {
    pub entries: Vec<ActionEntry>,
    /// The card-selection prompt, when the viewer is choosing — what orders
    /// its decisions (`Selection::rank`).
    selection: Option<Selection>,
    /// The selection positions another decision button already stands for
    /// — a second copy of a card in the same place (`Selection::hidden`).
    /// Still entries, so the play helper lists them; not decisions.
    collapsed: BTreeSet<usize>,
    /// Whether this view is a moment that will pass, which is what the
    /// two dual actions read to know their mood (`affordance`). Kept here
    /// rather than asked of the view per card, so every target of one
    /// view is judged against the same moment.
    passing: bool,
}

impl ActionMap {
    pub fn build(view: &ClientView, registry: &CardRegistry) -> Self {
        let entries = view
            .legal_actions
            .iter()
            .map(|action| ActionEntry { action: action.clone(), label: describe_action(action, registry, Some(view)), targets: targets_of(action, view) })
            .collect();
        let selection = Selection::of(view, registry);
        let collapsed = selection.as_ref().map(|selection| selection.hidden()).unwrap_or_default();
        Self { entries, selection, collapsed, passing: affordance::in_a_passing_moment(view) }
    }

    /// Adds to each entry's label what its action goes on to ask
    /// (`preview::Asks`). Apart from `build` because `build` is cheap
    /// enough to call every frame, which the terminal client does, and a
    /// preview is an `apply_action` per playable card: a client annotates
    /// the one map it keeps for a view.
    pub fn annotate(&mut self, asks: &super::preview::Asks) {
        for entry in &mut self.entries {
            entry.label = asks.label(&entry.action, std::mem::take(&mut entry.label));
        }
    }

    /// The card selection this map was built over, when the viewer is
    /// choosing — what a pop-up reads to draw each candidate's card.
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Whether entry `index` is a selection button another one already
    /// stands for (see `collapsed`).
    pub fn is_collapsed(&self, index: usize) -> bool {
        matches!(self.entries.get(index).map(|e| &e.action), Some(PlayerAction::ToggleCardSelection { position }) if self.collapsed.contains(position))
    }

    /// One sentence on what entry `index` does, for a tooltip or a
    /// coaching line.
    pub fn explain(&self, index: usize, registry: &CardRegistry, view: &ClientView) -> Option<String> {
        self.entries.get(index).map(|entry| explain_action(&entry.action, registry, Some(view)))
    }

    fn with_target(&self, wanted: &Target) -> Vec<usize> {
        self.entries.iter().enumerate().filter(|(_, entry)| entry.targets.contains(wanted)).map(|(i, _)| i).collect()
    }

    pub fn for_hand_card(&self, card: &CardId) -> Vec<usize> {
        self.with_target(&Target::HandCard(card.clone()))
    }

    pub fn for_install(&self, id: InstallId) -> Vec<usize> {
        self.with_target(&Target::Install(id))
    }

    pub fn for_server(&self, server: ServerId) -> Vec<usize> {
        self.with_target(&Target::Server(server))
    }

    pub fn for_identity(&self, side: Side) -> Vec<usize> {
        self.with_target(&Target::Identity(side))
    }

    pub fn for_position(&self, position: usize) -> Vec<usize> {
        self.with_target(&Target::Position(position))
    }

    pub fn for_pile(&self, pile: Pile) -> Vec<usize> {
        self.with_target(&Target::Pile(pile))
    }

    /// The entries on any target, by the target itself. The `for_*`
    /// lookups above are this with the target spelled out; a screen that
    /// already holds a [`Target`] — because a click produced one — wants
    /// this one, and both clients had written the same six-armed `match`
    /// to get here.
    pub fn for_target(&self, target: &Target) -> Vec<usize> {
        self.with_target(target)
    }

    /// The mood `target` earns from its legal actions, or `None` when the
    /// engine offers nothing on it — which is what the board draws no
    /// glow for.
    ///
    /// The caller gates on priority first: a view a person cannot act in
    /// has no `legal_actions`, so this is `None` throughout, but a client
    /// that is holding the last view while the opponent thinks must not
    /// keep lighting it (`Game::awaiting` in the desktop).
    pub fn affordance(&self, target: &Target) -> Option<Affordance> {
        self.for_target(target)
            .into_iter()
            .map(|index| affordance::affordance_of(&self.entries[index].action, self.passing))
            .reduce(Affordance::stronger)
    }

    /// The mood of one entry by index, for a surface that lists entries
    /// rather than targets — the decision buttons under a prompt, the
    /// control bar, the play helper's rows.
    pub fn affordance_of_entry(&self, index: usize) -> Option<Affordance> {
        self.entries.get(index).map(|entry| affordance::affordance_of(&entry.action, self.passing))
    }

    /// Where a card in hand may be taken: every place on the board an
    /// entry of that card also names — a server to install into (a remote
    /// the Corp has not made yet included, which the engine lists and the
    /// board has no column for until a drag asks for one), or the ice a
    /// trojan hosts on. A card with no destination — an operation, an
    /// event, a Runner's own install — has none, and is played rather
    /// than placed.
    pub fn destinations_for_hand_card(&self, card: &CardId) -> Vec<Target> {
        let mut places: Vec<Target> = Vec::new();
        for index in self.for_hand_card(card) {
            for target in &self.entries[index].targets {
                if !matches!(target, Target::HandCard(_)) && !places.contains(target) {
                    places.push(target.clone());
                }
            }
        }
        places
    }

    /// The entries that take `card` to `place`: what a drop there could
    /// mean. More than one when a server offers a card two ways — an
    /// agenda installed over what is already in the root, say — and the
    /// drop then asks rather than guessing.
    pub fn for_hand_card_at(&self, card: &CardId, place: &Target) -> Vec<usize> {
        let wanted = Target::HandCard(card.clone());
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.targets.contains(&wanted) && entry.targets.contains(place))
            .map(|(i, _)| i)
            .collect()
    }

    /// The entries no click on the board reaches: the control bar's and
    /// the decisions together.
    pub fn globals(&self) -> Vec<usize> {
        self.entries.iter().enumerate().filter(|(_, entry)| entry.targets.is_empty()).map(|(i, _)| i).collect()
    }

    /// The entry `control` would submit, if the engine lists it. At most
    /// one: a side has one `EndTurn` and one `PassPriority` at a time.
    pub fn for_control(&self, control: Control) -> Option<usize> {
        self.entries.iter().position(|entry| control.matches(&entry.action))
    }

    /// What the prompt is asking: every entry that is on no target and on
    /// no control — the mulligan, a bid, a choice, an access decision —
    /// plus the selection positions, which the board does not place
    /// (a `ChooseCards` prompt's zone is not drawn as clickable cards yet),
    /// less the ones a button for an identical copy already stands for, and
    /// in the order a selection reads (`Selection::rank`) — and a card
    /// effect's choice of server, which is on a server column too but is
    /// the answer to the prompt, and whose one free option, a new remote,
    /// has no column to click.
    pub fn decisions(&self) -> Vec<usize> {
        let mut decisions: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(i, entry)| {
                // `all` is vacuously true of no targets, hence the guard.
                (entry.targets.is_empty() && !Control::any_matches(&entry.action))
                    || (!entry.targets.is_empty() && entry.targets.iter().all(|t| matches!(t, Target::Position(_))) && !self.is_collapsed(*i))
                    || matches!(entry.action, PlayerAction::ChooseServerForPendingDecision { .. })
            })
            .map(|(i, _)| i)
            .collect();
        if let Some(selection) = &self.selection {
            decisions.sort_by_key(|i| selection.rank(&self.entries[*i].action));
        }
        decisions
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Where a click could mean `action`. Two targets where the action names
/// two things (an install names the card and the server; a trojan the
/// card and its host), so either click offers it. A draw is on the deck
/// it draws from, so the zone's sheet offers it beside the control bar.
///
/// An operation is played from where the engine takes it: HQ if it is
/// there, otherwise Archives (Petty Cash's own permission, the engine's
/// `operation_comes_from_archives`). Mapped to the hand card regardless,
/// a Petty Cash in Archives named a card the hand did not hold, so no
/// click on the board offered it — only the play helper's flat list did.
fn targets_of(action: &PlayerAction, view: &ClientView) -> Vec<Target> {
    match action {
        PlayerAction::PlayOperation { card_id } if !view.corp.hq_cards.as_ref().is_some_and(|hq| hq.contains(card_id)) => {
            vec![Target::Server(ServerId::Archives)]
        }
        PlayerAction::DrawCardClick { side: Side::Corp } => vec![Target::Server(ServerId::RnD)],
        PlayerAction::DrawCardClick { side: Side::Runner } => vec![Target::Pile(Pile::Stack)],
        PlayerAction::InstallCard { card_id, zone, .. } => vec![Target::HandCard(card_id.clone()), Target::Server(*zone)],
        PlayerAction::InstallProgramOnIce { card_id, host, .. } => vec![Target::HandCard(card_id.clone()), Target::Install(*host)],
        PlayerAction::PlayEvent { card_id }
        | PlayerAction::PlayOperation { card_id }
        | PlayerAction::InstallHardware { card_id }
        | PlayerAction::InstallProgram { card_id, .. }
        | PlayerAction::InstallResource { card_id }
        | PlayerAction::DiscardCard { card_id } => vec![Target::HandCard(card_id.clone())],
        PlayerAction::ActivateAbility { target, .. } if *target == InstallId::CORP_IDENTITY => vec![Target::Identity(Side::Corp)],
        PlayerAction::ActivateAbility { target, .. } if *target == InstallId::RUNNER_IDENTITY => vec![Target::Identity(Side::Runner)],
        PlayerAction::RezIce { ice: target }
        | PlayerAction::ActivateAbility { target, .. }
        | PlayerAction::AdvanceCard { target }
        | PlayerAction::ScoreAgenda { target }
        | PlayerAction::TrashResource { target } => vec![Target::Install(*target)],
        PlayerAction::InitiateRun { server } | PlayerAction::ChooseServerForPendingDecision { server } => vec![Target::Server(*server)],
        PlayerAction::ToggleCardSelection { position } => vec![Target::Position(*position)],
        // The basic actions are the control bar's; the run and access
        // decisions belong under the prompt, which names the accessed
        // card — neither is on the board.
        PlayerAction::GainCreditClick { .. }
        | PlayerAction::ContinueRun
        | PlayerAction::JackOut
        | PlayerAction::CompleteRun
        | PlayerAction::BreakSubroutineWithClick { .. }
        | PlayerAction::EndTurn
        | PlayerAction::KeepHand
        | PlayerAction::TakeMulligan
        | PlayerAction::RemoveTag
        | PlayerAction::PurgeVirusCounters
        | PlayerAction::ChooseTriggerToResolve { .. }
        | PlayerAction::SelectCardToAccess { .. }
        | PlayerAction::StealAgenda { .. }
        | PlayerAction::TrashAccessedCard { .. }
        | PlayerAction::PassAccessedCard { .. }
        | PlayerAction::PayAccessTrigger { .. }
        | PlayerAction::DeclineAccessTrigger { .. }
        | PlayerAction::PassPriority { .. }
        | PlayerAction::SubmitCorpTraceBid { .. }
        | PlayerAction::SubmitRunnerTraceBid { .. }
        | PlayerAction::ChooseNumber { .. }
        | PlayerAction::AcceptPendingPaidChoice { .. }
        | PlayerAction::DeclinePendingPaidChoice
        | PlayerAction::ResolvePendingChoice { .. }
        | PlayerAction::ConfirmCardSelection => Vec::new(),
    }
}

/// What the game is asking the viewer right now, in one line, with the
/// card's own words where a card is asking (the Linked Clause Rule: the
/// printed clause, never a rendering of the DSL, when the card has one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub title: String,
    /// The clause or the detail under the title; empty when the title is
    /// the whole prompt.
    pub detail: String,
}

impl Prompt {
    /// The prompt for `view`, or `None` when it is an ordinary action
    /// phase and the panel needs no heading.
    pub fn of(view: &ClientView, registry: &CardRegistry) -> Option<Prompt> {
        let title = |card: &Option<CardId>| card.as_ref().map_or_else(|| "A card".to_string(), |id| card_title(id, registry));
        let asked_by = |prompting: &Option<CardId>, source: &Option<CardId>| title(if prompting.is_some() { prompting } else { source });
        // Ahead of everything: the engine answers a parked payment before
        // whatever is parked beneath it, so the buttons on show are the
        // payment's, and a card's own question here would be asked over
        // them. The payer's view has the payment; the other seat is told
        // who is deciding, which is public, and why the table has paused.
        if let Some(payment) = &view.pending_payment {
            return Some(match &payment.own {
                Some(netrunner_core::rules::PendingPayment { question: netrunner_core::rules::PaymentAsk::Pools(question), amount, .. }) => Prompt {
                    title: format!("Pay {} credit{}", amount, if *amount == 1 { "" } else { "s" }),
                    detail: format!(
                        "More than one place could pay. How many come from {}? The rest comes from the others.",
                        prose::pool_name(question.pool, view, registry)
                    ),
                },
                // A cost that takes cards asks for one at a time, so the
                // buttons under this are the cards it could take.
                Some(netrunner_core::rules::PendingPayment { question: netrunner_core::rules::PaymentAsk::Card(question), .. }) => Prompt {
                    title: format!("Trash a card from {} to pay", prose::describe_zone(&question.zone)),
                    detail: if question.remaining == 1 {
                        "The last card the cost takes.".to_string()
                    } else {
                        format!("The cost takes {} more.", question.remaining)
                    },
                },
                Some(netrunner_core::rules::PendingPayment { question: netrunner_core::rules::PaymentAsk::Alternative { card, .. }, .. }) => Prompt {
                    title: format!("Rez {}", title_of(Some(card), registry)),
                    detail: "It prints more than one way to pay for it. Which?".to_string(),
                },
                // An install trashing like cards (CR 8.5.6) asks one card
                // at a time, so the buttons under this are the cards.
                Some(netrunner_core::rules::PendingPayment { question: netrunner_core::rules::PaymentAsk::Install(question), .. }) => Prompt {
                    title: format!("Install {}: trash which first?", title_of(Some(&question.card), registry)),
                    detail: crate::prose::install_trash_detail(question),
                },
                None => Prompt { title: format!("The {:?} is choosing how to pay", payment.side), detail: String::new() },
            });
        }
        if let Some(decision) = &view.pending_decision {
            return Some(match decision {
                PendingDecision::ChooseEffect { source_card, prompting_card, .. } => {
                    Prompt { title: format!("{}: choose", asked_by(prompting_card, source_card)), detail: String::new() }
                }
                PendingDecision::ChooseCards { min, max, selected, source_card, prompting_card, .. } => {
                    let range = if min == max { format!("{min}") } else { format!("{min} to {max}") };
                    // The chosen cards by name; a count only for a viewer
                    // who is not choosing and so has no names to be told.
                    let detail = match Selection::of(view, registry) {
                        Some(selection) => selection.summary(),
                        None => format!("{} selected", selected.len()),
                    };
                    Prompt { title: format!("{}: choose {range} card{}", asked_by(prompting_card, source_card), if *max == 1 { "" } else { "s" }), detail }
                }
                PendingDecision::ChooseTriggerOrder { .. } => Prompt { title: "Choose which triggers first".to_string(), detail: String::new() },
                // The card's clause is the question, the range its detail.
                PendingDecision::ChooseNumber { text, min, max, source_card, prompting_card, .. } => Prompt {
                    title: format!("{}: {}", asked_by(prompting_card, source_card), if text.is_empty() { "choose a number" } else { text.as_str() }),
                    detail: format!("{min} to {max}"),
                },
                PendingDecision::ChooseServer { source_card, prompting_card, install, .. } => match Placement::of(view, registry) {
                    Some(placement) => Prompt { title: format!("{}: {}", asked_by(prompting_card, source_card), placement.question()), detail: placement.detail() },
                    None if install.is_some() => Prompt { title: format!("{}: installing a card", asked_by(prompting_card, source_card)), detail: String::new() },
                    None => Prompt { title: format!("{}: choose a server to run", asked_by(prompting_card, source_card)), detail: String::new() },
                },
            });
        }
        if let Some(paid) = &view.pending_paid_choice {
            let detail = match &paid.text {
                Some(text) => text.clone(),
                None => format!("Pay {} to {}", prose::describe_cost(&paid.cost), prose::describe_effect(&paid.if_paid, registry)),
            };
            return Some(Prompt { title: format!("{}: pay?", asked_by(&paid.prompting_card, &paid.source_card)), detail });
        }
        if let Some(prevention) = &view.pending_prevention {
            let left = prevention.what.amount().saturating_sub(prevention.prevented);
            let title = match &prevention.what {
                WouldHappen::Damage { kind, .. } => format!("Prevent {left} {} damage?", format!("{kind:?}").to_lowercase()),
                WouldHappen::Tags { .. } => format!("Prevent {left} tag{}?", if left == 1 { "" } else { "s" }),
                WouldHappen::Trash { owner, install } => {
                    let card = match owner {
                        Side::Runner => view.runner.rig.iter().find(|c| c.install_id == *install).map(|c| &c.card),
                        Side::Corp => None,
                    };
                    format!("Prevent the trash of {}?", title_of(card, registry))
                }
            };
            return Some(Prompt { title, detail: format!("{} offers to", title_of(prevention.source_card.as_ref(), registry)) });
        }
        if let Some(trace) = &view.active_trace {
            let detail = match trace.corp_bid {
                Some(bid) => format!("The Corp bid {bid}; beat it with link and credits"),
                None => "The Corp bids first".to_string(),
            };
            return Some(Prompt { title: format!("Trace, base strength {}", trace.base_strength), detail });
        }
        if let Some(run) = &view.active_run {
            if let Some(access) = &run.access_state {
                // The words of an access belong to `access::Access`, which
                // is also what the pop-up and the terminal's panel draw —
                // the rail and the panel said the same thing two ways
                // ("An agenda: it must be stolen" against "An agenda — it
                // must be stolen") until this deferred to it. It answers
                // only for the side being asked, so the arms below stay
                // for everyone else.
                if let Some(access) = crate::access::Access::of(view, registry) {
                    return Some(Prompt { title: access.title(), detail: access.facts().join(" · ") });
                }
                match &access.phase {
                    PublicAccessPhase::PendingChoice { card, trash_cost, mandatory_steal, .. } => {
                        let mut detail = String::new();
                        if let Some(cost) = trash_cost {
                            detail = format!("Trash cost {cost}");
                        }
                        if *mandatory_steal {
                            detail = "An agenda: it must be stolen".to_string();
                        }
                        return Some(Prompt { title: format!("Accessing {}", title(card)), detail });
                    }
                    PublicAccessPhase::PendingInteractiveTrigger { card, cost, decider, .. } => {
                        return Some(Prompt {
                            title: format!("{} asks {decider:?} to pay {}", title(card), prose::describe_cost(cost)),
                            detail: String::new(),
                        });
                    }
                    PublicAccessPhase::SelectNextCard { .. } => {
                        return Some(Prompt { title: format!("Breaching {}: choose a card to access", server_name(run.server)), detail: String::new() });
                    }
                }
            }
            let phase = match run.phase {
                RunPhase::Initiation => "starting",
                RunPhase::ApproachIce => "approaching ice",
                RunPhase::EncounterIce => "encountering ice",
                RunPhase::Movement => "moving on",
                RunPhase::AccessingCard => "accessing",
                RunPhase::Success => "successful",
                RunPhase::Ended => "over",
            };
            return Some(Prompt { title: format!("Run on {}: {phase}", server_name(run.server)), detail: String::new() });
        }
        if let Some(window) = &view.paid_ability_window {
            return Some(Prompt { title: format!("Paid ability window: {:?} has priority", window.active_priority), detail: String::new() });
        }
        match view.phase {
            GamePhase::Mulligan(side) if Some(side) == view.viewer.side() => Some(Prompt { title: "Keep this hand, or mulligan once".to_string(), detail: String::new() }),
            GamePhase::Discard { side, .. } if Some(side) == view.viewer.side() => Some(Prompt { title: "Discard down to your hand size".to_string(), detail: String::new() }),
            _ => None,
        }
    }
}

impl Prompt {
    /// The one card a pop-up shows beside the prompt, so a choice is made
    /// looking at the card it is about rather than at its name: the card an
    /// install from a card's text is placing, else the card whose text is
    /// asking (a "gain 2 or draw 1", a run on a server of the Runner's
    /// choice, a "pay to…", a prevention's offer). **Not** a card
    /// selection's — that pop-up shows every candidate, which is the choice
    /// itself (`selection::Selection`) — and not an access's, which
    /// `access::Access` shows. `None` when nothing names a card, and (where
    /// the decision says who is asked) for a viewer who is not.
    ///
    /// The asking card is `prompting_card`, falling back to `source_card`
    /// as `Prompt::of`'s title does: `source_card` is what the decision
    /// resolves *as*, which after a selection is the selected card.
    pub fn card(view: &ClientView, registry: &CardRegistry) -> Option<CardId> {
        let asked_by = |prompting: &Option<CardId>, source: &Option<CardId>| prompting.clone().or_else(|| source.clone());
        // No one card asks where a payment comes from — and it is asked
        // ahead of a decision parked beneath it, whose card must not be
        // shown over the payment's buttons.
        if view.pending_payment.is_some() {
            return None;
        }
        if let Some(decision) = &view.pending_decision {
            return match decision {
                PendingDecision::ChooseEffect { chooser, source_card, prompting_card, .. }
                | PendingDecision::ChooseNumber { chooser, source_card, prompting_card, .. } => {
                    view.viewer.is(*chooser).then(|| asked_by(prompting_card, source_card)).flatten()
                }
                PendingDecision::ChooseServer { chooser, source_card, prompting_card, install, .. } => match Placement::of(view, registry) {
                    Some(placement) => placement.card().cloned().or_else(|| asked_by(prompting_card, source_card)),
                    None if install.is_none() && view.viewer.is(*chooser) => asked_by(prompting_card, source_card),
                    None => None,
                },
                PendingDecision::ChooseCards { .. } | PendingDecision::ChooseTriggerOrder { .. } => None,
            };
        }
        if let Some(paid) = &view.pending_paid_choice {
            return view.viewer.is(paid.side).then(|| asked_by(&paid.prompting_card, &paid.source_card)).flatten();
        }
        if let Some(prevention) = &view.pending_prevention {
            // The prevention carries no chooser: the card offering it is
            // its owner's, and only its owner is ever asked.
            return prevention.source_card.clone();
        }
        None
    }
}

fn title_of(card: Option<&CardId>, registry: &CardRegistry) -> String {
    card.map_or_else(|| "A card".to_string(), |id| card_title(id, registry))
}

/// The Corp's servers in the order both clients draw them, from either
/// chair: Archives, R&D, HQ — each listed even with nothing installed,
/// since a run on an empty central is still a click on it — then the
/// remotes in the order they were made.
///
/// **One order for both chairs, not the table mirrored.** The desktop
/// once showed the Runner the table as it lies across from them —
/// remotes, HQ, R&D, Archives — which is how the cards sit, but every
/// remote the Corp made pushed the three centrals a column to the right,
/// so the servers a Runner hits most were in a new place each time. The
/// person asked for the centrals to stay put; with remotes growing to
/// the right they are fixed for the whole game from either chair. The
/// engine's own `ServerView` order (HQ first) is a sort key for grouping,
/// not a layout, and is left alone.
pub fn table_servers(view: &ClientView) -> Vec<ServerView> {
    let mut servers: Vec<ServerView> = view.corp.servers.clone();
    for central in [ServerId::Archives, ServerId::RnD, ServerId::Hq] {
        if !servers.iter().any(|s| s.server == central) {
            servers.push(ServerView { server: central, ice: Vec::new(), root: Vec::new() });
        }
    }
    servers.sort_by_key(|s| match s.server {
        ServerId::Archives => (0, 0),
        ServerId::RnD => (1, 0),
        ServerId::Hq => (2, 0),
        ServerId::Remote(n) => (3, n),
    });
    servers
}

/// A server as a person names it. Remotes keep the engine's numbering
/// — `Remote 0` is the one every action label and log line calls
/// `Remote(0)` — because a header that says 1 over a panel that says
/// `Run Remote(0)` is two names for one server.
pub fn server_name(server: ServerId) -> String {
    match server {
        ServerId::Hq => "HQ".to_string(),
        ServerId::RnD => "R&D".to_string(),
        ServerId::Archives => "Archives".to_string(),
        ServerId::Remote(n) => format!("Remote {n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::GameState;
    use netrunner_session::{sweep_decks_for_seed, Seat, Session, SessionStep};

    /// A real parked payment, reached by real actions: a Runner with Azimat
    /// installed plays Overclock, breaches a remote and trashes a PAD
    /// Campaign for 4 — Azimat's 2 against the run's 5.
    fn a_parked_payment() -> (CardRegistry, GameState) {
        use netrunner_core::dsl::CardId;
        use netrunner_core::rules::{apply_action, Clicks, Credits, GamePhase, InstallId, InstalledCard, InstalledRunnerCard, ServerId};
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let id = |name: &str| CardId(name.to_string());
        let mut state = GameState::new(7);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.credits = Credits(1);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = vec![id("overclock")];
        state.runner.rig = vec![InstalledRunnerCard { install_id: InstallId(901), card: id("azimat"), counters: 2, ..Default::default() }];
        state.corp.installed =
            vec![InstalledCard { install_id: InstallId(902), card: id("pad_campaign"), server: ServerId::Remote(0), rezzed: true, ..Default::default() }];
        state.corp.r_and_d = (0..4).map(|i| id(&format!("filler_{i}"))).collect();
        for action in [
            PlayerAction::PlayEvent { card_id: id("overclock") },
            PlayerAction::ChooseServerForPendingDecision { server: ServerId::Remote(0) },
            // No ice: into the movement phase, then on past it; the window
            // it opens is passed below, and the server approached.
            PlayerAction::ContinueRun,
            PlayerAction::ContinueRun,
            PlayerAction::CompleteRun,
            PlayerAction::TrashAccessedCard { card_id: id("pad_campaign") },
        ] {
            state = apply_action(&state, &registry, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}")).0;
            // Both seats pass any window the step opened, as play would.
            while let Some(side) = state.paid_ability_window.as_ref().map(|window| window.active_priority) {
                state = apply_action(&state, &registry, PlayerAction::PassPriority { side }).expect("a pass").0;
            }
        }
        assert!(state.pending_payment.is_some(), "the trash parked a payment");
        (registry, state)
    }

    /// Both clients get every word of a parked payment from here, so this
    /// is the one place they are checked: the prompt, a button per number
    /// naming the card the credits come off, and no card shown as the one
    /// asking.
    #[test]
    fn a_parked_payment_is_a_prompt_with_a_button_for_each_split_naming_the_card_it_comes_off() {
        let (registry, state) = a_parked_payment();

        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Runner);
        let map = ActionMap::build(&view, &registry);
        let labels: Vec<&str> = map.decisions().into_iter().map(|index| map.entries[index].label.as_str()).collect();
        assert_eq!(labels, vec!["0 from Azimat, 4 from the rest", "1 from Azimat, 3 from the rest", "2 from Azimat, 2 from the rest"]);
        assert_eq!(map.entries.len(), 3, "and nothing else is on offer while it is parked");
        let prompt = Prompt::of(&view, &registry).expect("the payer is prompted");
        assert_eq!(prompt.title, "Pay 4 credits");
        assert_eq!(Prompt::card(&view, &registry), None, "no one card asks where a payment comes from");
        assert_eq!(crate::prose::decision_prompt(&view, &registry).as_deref(), Some("Pay 4 credits — how many from Azimat?"));

        // The other chair: nothing to press, and told why the table paused.
        let view = netrunner_core::view::build_client_view(&state, &registry, Side::Corp);
        assert!(ActionMap::build(&view, &registry).is_empty());
        assert_eq!(Prompt::of(&view, &registry).expect("told who is deciding").title, "The Runner is choosing how to pay", "the other chair cannot see whether it is credits or cards");
    }

    /// A text choice shows the card whose text is asking, and only to the
    /// side being asked; the asking card wins over the one the decision
    /// resolves as.
    #[test]
    fn a_text_choice_shows_the_card_asking_to_the_one_asked() {
        use netrunner_core::rules::PendingChoiceResume;
        let registry = CardRegistry::default();
        let mut view = netrunner_core::view::build_client_view(&GameState::new(1), &registry, Side::Corp);
        view.pending_decision = Some(PendingDecision::ChooseEffect {
            chooser: Side::Corp,
            options: Vec::new(),
            option_texts: Vec::new(),
            source_card: Some(CardId("resolves_as".into())),
            prompting_card: Some(CardId("asking".into())),
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        assert_eq!(Prompt::card(&view, &registry), Some(CardId("asking".into())));
        view.viewer = Side::Runner.into();
        assert_eq!(Prompt::card(&view, &registry), None, "the Runner is not being asked");
        view.pending_decision = None;
        assert_eq!(Prompt::card(&view, &registry), None, "no prompt, no card");
    }

    /// A card in hand names the places it may go, and each place the
    /// entries a drop there could mean — including a remote the Corp has
    /// not made yet, which the engine offers and no column shows.
    #[test]
    fn a_hand_card_names_where_it_may_go() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
        let mut agents = [RandomAgent::new(0), RandomAgent::new(1)];
        // Play on until the Corp is asked something with an install in it.
        loop {
            match session.step() {
                SessionStep::Awaiting { side, view } => {
                    let map = ActionMap::build(&view, &registry);
                    let install = view.legal_actions.iter().find_map(|action| match action {
                        PlayerAction::InstallCard { card_id, zone, .. } => Some((card_id.clone(), *zone)),
                        _ => None,
                    });
                    if let Some((card, zone)) = install {
                        let places = map.destinations_for_hand_card(&card);
                        assert!(places.contains(&Target::Server(zone)), "{places:?}");
                        assert!(places.iter().all(|place| !matches!(place, Target::HandCard(_))), "the card itself is not a place");
                        let entries = map.for_hand_card_at(&card, &Target::Server(zone));
                        assert!(!entries.is_empty());
                        for index in &entries {
                            assert!(map.entries[*index].targets.contains(&Target::Server(zone)));
                            assert!(map.entries[*index].targets.contains(&Target::HandCard(card.clone())));
                        }
                        // A card the engine offers nowhere is played, not placed.
                        let operation = view.legal_actions.iter().find_map(|action| match action {
                            PlayerAction::PlayOperation { card_id, .. } => Some(card_id.clone()),
                            _ => None,
                        });
                        if let Some(card) = operation {
                            assert!(map.destinations_for_hand_card(&card).is_empty(), "an operation has no place on the board");
                        }
                        return;
                    }
                    use netrunner_bots::BotAgent;
                    let index = usize::from(side == Side::Runner);
                    let action = agents[index].select_action(&view, &registry);
                    session.submit(action).unwrap();
                }
                SessionStep::Applied { .. } => {}
                SessionStep::Ended { .. } => panic!("the game ended before an install was offered"),
                SessionStep::Stalled(reason) => panic!("{reason:?}")
            }
        }
    }

    /// The centrals are always there and always first, Archives to HQ,
    /// and the remotes follow by number, whatever order the engine gave.
    #[test]
    fn the_table_is_archives_rnd_hq_then_the_remotes() {
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let registry = crate::decks::sample_deck_registry();
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let mut view = Session::new(state, registry, Seat::External, Seat::External).view_for(Side::Runner);
        view.corp.servers.clear();
        let order = |view: &ClientView| table_servers(view).iter().map(|s| s.server).collect::<Vec<_>>();
        assert_eq!(order(&view), [ServerId::Archives, ServerId::RnD, ServerId::Hq], "an empty table still has its centrals");
        for server in [ServerId::Remote(1), ServerId::Hq, ServerId::Remote(0)] {
            view.corp.servers.push(ServerView { server, ice: Vec::new(), root: Vec::new() });
        }
        assert_eq!(order(&view), [ServerId::Archives, ServerId::RnD, ServerId::Hq, ServerId::Remote(0), ServerId::Remote(1)]);
    }

    /// The whole map, over real games: every legal action is exactly one
    /// entry, in the engine's order, and every target names something
    /// the viewer's own view shows — a hand card in the visible hand, an
    /// install on the board, a server that exists or a central.
    #[test]
    fn every_legal_action_is_one_entry_and_every_target_is_on_the_board() {
        let mut targeted = 0;
        let mut prompts = 0;
        for seed in 0..4u64 {
            let (corp_deck, runner_deck) = sweep_decks_for_seed(seed);
            let registry = crate::decks::sample_deck_registry();
            let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
            let mut agents = [RandomAgent::new(seed), RandomAgent::new(seed + 100)];
            loop {
                match session.step() {
                    SessionStep::Awaiting { side, view } => {
                        let map = ActionMap::build(&view, &registry);
                        assert_eq!(map.entries.len(), view.legal_actions.len());
                        for (entry, action) in map.entries.iter().zip(&view.legal_actions) {
                            assert_eq!(&entry.action, action, "in the engine's order");
                            assert!(!entry.label.is_empty());
                            for target in &entry.targets {
                                targeted += 1;
                                match target {
                                    Target::HandCard(card) => {
                                        let hand = match side {
                                            Side::Corp => view.corp.hq_cards.as_ref(),
                                            Side::Runner => view.runner.grip_cards.as_ref(),
                                        };
                                        assert!(hand.is_some_and(|hand| hand.contains(card)), "seed {seed}: {action:?} targets a card not in the visible hand");
                                    }
                                    Target::Install(id) => {
                                        let on_board = view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).any(|c| c.install_id == *id)
                                            || view.runner.rig.iter().any(|c| c.install_id == *id);
                                        assert!(on_board, "seed {seed}: {action:?} targets an install the view does not show");
                                    }
                                    Target::Server(server) => {
                                        // An install, or a card that installs, may name the
                                        // remote it would create.
                                        let exists = !matches!(server, ServerId::Remote(_))
                                            || view.corp.servers.iter().any(|s| s.server == *server)
                                            || matches!(action, PlayerAction::InstallCard { .. } | PlayerAction::ChooseServerForPendingDecision { .. });
                                        assert!(exists, "seed {seed}: {action:?} targets a server that does not exist");
                                    }
                                    Target::Position(_) => assert!(
                                        matches!(view.pending_decision, Some(PendingDecision::ChooseCards { .. }))
                                            || view.pending_payment.as_ref().is_some_and(|payment| payment.own.is_some()),
                                        "seed {seed}: {action:?} names a position with nothing asking for one"
                                    ),
                                    Target::Pile(_) => assert_eq!(side, Side::Runner, "seed {seed}: {action:?} targets a pile the Corp does not have"),
                                    Target::Identity(owner) => {
                                        let shown = match owner {
                                            Side::Corp => view.corp.identity.is_some(),
                                            Side::Runner => view.runner.identity.is_some(),
                                        };
                                        assert!(shown, "seed {seed}: {action:?} targets an identity the view does not carry");
                                    }
                                }
                            }
                        }
                        assert_eq!(map.for_hand_card(&CardId("no_such_card".to_string())), Vec::<usize>::new());
                        if Prompt::of(&view, &registry).is_some() {
                            prompts += 1;
                        }
                        // The bar, the decisions and the targeted entries
                        // cover every index (a draw is on the bar and on
                        // the deck): nothing is reachable only from the
                        // flat panel — save a collapsed copy, which a
                        // shown decision with the same words stands for.
                        let mut seen: Vec<usize> = map.decisions();
                        for index in (0..map.entries.len()).filter(|i| map.is_collapsed(*i)) {
                            let label = &map.entries[index].label;
                            assert!(
                                map.decisions().iter().any(|d| map.entries[*d].label == *label),
                                "seed {seed}: collapsed {:?} has no shown button saying {label:?}",
                                map.entries[index].action
                            );
                            seen.push(index);
                        }
                        for control in Control::for_side(side) {
                            if let Some(index) = map.for_control(*control) {
                                seen.push(index);
                            }
                        }
                        seen.extend(map.entries.iter().enumerate().filter(|(_, e)| e.targets.iter().any(|t| !matches!(t, Target::Position(_)))).map(|(i, _)| i));
                        seen.sort_unstable();
                        seen.dedup();
                        assert_eq!(seen, (0..map.entries.len()).collect::<Vec<_>>(), "seed {seed}, {side:?}: {:?}", map.entries.iter().map(|e| &e.action).collect::<Vec<_>>());
                        for control in Control::for_side(side.other()) {
                            if !Control::for_side(side).contains(control) {
                                assert_eq!(map.for_control(*control), None, "seed {seed}: {control:?} is the other side's");
                            }
                        }
                        let index = match side {
                            Side::Corp => 0,
                            Side::Runner => 1,
                        };
                        use netrunner_bots::BotAgent;
                        let action = agents[index].select_action(&view, &registry);
                        session.submit(action).unwrap();
                    }
                    SessionStep::Applied { .. } => {}
                    SessionStep::Ended { .. } => break,
                    SessionStep::Stalled(reason) => panic!("seed {seed}: {reason:?}"),
                }
            }
        }
        assert!(targeted > 100, "{targeted} targeted entries: the board route is reachable");
        assert!(prompts > 10, "{prompts} prompts: the mulligan alone is two a game");
    }

    /// A click on a hand card and a click on a server reach the same
    /// install entry, and the panel's index is the one list's.
    #[test]
    fn petty_cash_played_from_archives_is_reached_from_archives() {
        let id = |name: &str| CardId(name.to_string());
        let registry = crate::decks::sample_deck_registry();
        let mut state = GameState::new(0);
        state.phase = netrunner_core::rules::GamePhase::Action(Side::Corp);
        state.corp.resources.clicks = netrunner_core::rules::Clicks(3);
        state.corp.resources.credits = netrunner_core::rules::Credits(3);
        state.corp.archives = vec![netrunner_core::rules::ArchivedCard::faceup(id("petty_cash"))];
        state.corp.playable_from_archives = vec![id("petty_cash")];
        let view = Session::new(state, registry.clone(), Seat::External, Seat::External).view_for(Side::Corp);
        let play = PlayerAction::PlayOperation { card_id: id("petty_cash") };
        assert!(view.legal_actions.contains(&play), "the engine offers it: {:?}", view.legal_actions);
        let map = ActionMap::build(&view, &registry);
        let entry = map.entries.iter().find(|entry| entry.action == play).unwrap();
        assert_eq!(entry.targets, vec![Target::Server(ServerId::Archives)], "the card is in Archives, not the hand");
    }

    #[test]
    fn an_install_is_reached_from_its_card_and_from_its_server() {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let mut session = Session::new(state, registry.clone(), Seat::External, Seat::External);
        // Keep both hands, then take the first legal action until the Corp
        // is offered an install (an opening hand can be five operations).
        let view = loop {
            match session.step() {
                SessionStep::Awaiting { view, .. } if matches!(view.phase, GamePhase::Mulligan(_)) => session.submit(PlayerAction::KeepHand).unwrap(),
                SessionStep::Awaiting { side: Side::Corp, view } if view.legal_actions.iter().any(|a| matches!(a, PlayerAction::InstallCard { .. })) => break view,
                SessionStep::Awaiting { view, .. } => session.submit(view.legal_actions[0].clone()).unwrap(),
                SessionStep::Applied { .. } => {}
                other => panic!("{other:?}"),
            }
        };
        let map = ActionMap::build(&view, &registry);
        let install = map.entries.iter().position(|e| matches!(e.action, PlayerAction::InstallCard { .. })).expect("a Corp can install something");
        let PlayerAction::InstallCard { card_id, zone, .. } = &map.entries[install].action else { unreachable!() };
        assert!(map.for_hand_card(card_id).contains(&install));
        assert!(map.for_server(*zone).contains(&install));
        // The Corp has clicks, so End turn is not legal (CR 5.6.2b) and
        // its button is greyed; the bar's other buttons are on the map.
        assert_eq!(map.for_control(Control::EndTurn), None, "no End turn with clicks left");
        let draw = map.for_control(Control::Draw).expect("a draw is on the bar");
        assert!(map.for_server(ServerId::RnD).contains(&draw), "and on R&D");
        assert!(!map.decisions().contains(&draw), "a bar action is not a decision");
        assert_eq!(map.for_control(Control::JackOut), None, "no run, no jack out");
        assert!(map.explain(install, &registry, &view).unwrap().contains("install"));
        assert_eq!(Prompt::of(&view, &registry), None, "an ordinary action phase has no heading");
    }
}
