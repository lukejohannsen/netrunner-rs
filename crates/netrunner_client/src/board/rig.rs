//! The Runner's rig as the table lays it out: three rows — programs,
//! hardware, resources — with programs the row nearest the ICE they
//! break and resources the row nearest the Runner.
//!
//! The rules give an installed Runner card no position ("does not
//! matter" — the learn-to-play guide). The table does, and a person
//! reading a rig reads it by row: what breaks, what it runs on, what
//! pays for it. jinteki.net draws the same three rows. Both clients take
//! their order from [`rows_top_down`], so no screen decides it — the rule
//! [`table_servers`](super::table_servers) is for the servers.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardType;
use netrunner_core::rules::{InstallId, OncePerTurnKey, PublicInstalledRunnerCard, Side};
use netrunner_core::view::ClientView;

use super::action_map::{ActionMap, Target};

/// Which of the rig's rows a card sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigRow {
    Programs,
    Hardware,
    /// Resources, and anything else the Runner installs.
    Resources,
}

impl RigRow {
    /// The row a card of `card_type` sits in.
    pub fn of(card_type: &CardType) -> Self {
        match card_type {
            CardType::Program => RigRow::Programs,
            CardType::Hardware => RigRow::Hardware,
            _ => RigRow::Resources,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RigRow::Programs => "Programs",
            RigRow::Hardware => "Hardware",
            RigRow::Resources => "Resources",
        }
    }
}

/// The rig's rows top to bottom as the chair sees them. From the
/// Runner's chair the ICE is above the rig and the hand below it, so
/// programs come first; from the Corp's the rig sits across the table
/// with the ICE below it, so the order turns over and programs are still
/// the row next to the ICE.
pub fn rows_top_down(chair: Side) -> [RigRow; 3] {
    match chair {
        Side::Runner => [RigRow::Programs, RigRow::Hardware, RigRow::Resources],
        Side::Corp => [RigRow::Resources, RigRow::Hardware, RigRow::Programs],
    }
}

/// The rig's cards in their rows, in [`rows_top_down`] order for `chair`,
/// each row in the view's order. Every row is listed, empty or not. A
/// card the registry does not know goes nowhere, as it has no face to
/// draw.
pub fn rows<'a>(view: &'a ClientView, registry: &CardRegistry, chair: Side) -> [(RigRow, Vec<&'a PublicInstalledRunnerCard>); 3] {
    rows_top_down(chair).map(|row| (row, view.runner.rig.iter().filter(|card| registry.get(&card.card).is_some_and(|def| RigRow::of(&def.card_type) == row)).collect()))
}

/// The Trojans hosted on the piece of ice `ice`, in the rig's order.
///
/// **A Trojan is drawn where it is: on its ice**, as a button of its own
/// on the ice's tile, and its copy in the program row is a *ghost*
/// ([`is_ghost`]) — drawn faintly, and still the same click, so the rig
/// still reads as the rig and a Trojan has two ways in to one menu
/// (jinteki.net's ghost Trojans; Phase 7 §8 item 8). Both are doors to the
/// actions the engine already lists on the Trojan's install, never an
/// action of their own. Two alternatives were rejected: an **inert** ghost,
/// jinteki's, which would make one card in the row a picture where every
/// other card is a click; and a mark on the ice with the real card left in
/// the row, which says where a Trojan is without putting it there.
///
/// The host is an `InstallId`, never masked (`hosted_on_ice`), so an
/// unrezzed host lists its Trojans to both chairs as the table would.
pub fn hosted_on(view: &ClientView, ice: InstallId) -> Vec<&PublicInstalledRunnerCard> {
    view.runner.rig.iter().filter(|card| card.hosted_on_ice == Some(ice)).collect()
}

/// The ice a Trojan is on, in the words its ghost's line says after
/// "on": its title when the viewer may name it, and otherwise the server
/// it protects ("unrezzed ice on HQ") — never a card the view withholds.
pub fn host_label(view: &ClientView, registry: &CardRegistry, ice: InstallId) -> String {
    let found = view.corp.servers.iter().find_map(|server| server.ice.iter().find(|card| card.install_id == ice).map(|card| (server.server, card)));
    match found {
        Some((_, card)) if card.card.is_some() => crate::actions::install_label(&ice, registry, Some(view)),
        Some((server, _)) => format!("unrezzed ice on {}", super::action_map::server_name(server)),
        None => crate::actions::install_label(&ice, registry, Some(view)),
    }
}

/// Whether `card`'s place in the program row is a ghost: its home is on
/// the ice that hosts it (see [`hosted_on`]).
pub fn is_ghost(card: &PublicInstalledRunnerCard) -> bool {
    card.hosted_on_ice.is_some()
}

/// Copies of one card that nothing tells apart, drawn as one card with
/// a count (jinteki.net's stacked rig). `cards` is never empty, and its
/// first card stands for the stack: a click on the stack is a click on
/// that copy, which is every copy's click because [`stacked`] only puts
/// together copies with the same offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack<'a> {
    pub cards: Vec<&'a PublicInstalledRunnerCard>,
}

impl<'a> Stack<'a> {
    /// The copy that stands for the stack.
    pub fn first(&self) -> &'a PublicInstalledRunnerCard {
        self.cards[0]
    }

    pub fn count(&self) -> usize {
        self.cards.len()
    }

    pub fn install_ids(&self) -> impl Iterator<Item = InstallId> + '_ {
        self.cards.iter().map(|card| card.install_id)
    }
}

/// [`rows`], with identical copies in a row made one [`Stack`], each
/// stack where its first copy was.
///
/// **Identical means nothing a person could read or do differs** — the
/// rule `selection` folds a second copy's button by. The same card, the
/// same strength and counters, the same once-per-turn state, nothing
/// hosted on it, not hosted itself, and, when `actions` is given, the
/// same entries in the map. Stacking by card alone was the rejected
/// alternative: a Cyberfeeder with its credit spent would sit under one
/// with it unspent, and a click would have meant whichever the stack
/// happened to show. A copy that becomes different leaves its stack on
/// the next view and rejoins it when it stops being different.
///
/// `actions` is `None` where nobody acts (a replay's reading, a test);
/// the offer is then not compared, and what the view shows still is.
pub fn stacked<'a>(view: &'a ClientView, registry: &CardRegistry, chair: Side, actions: Option<&ActionMap>) -> [(RigRow, Vec<Stack<'a>>); 3] {
    rows(view, registry, chair).map(|(row, cards)| {
        let mut stacks: Vec<Stack<'a>> = Vec::new();
        for card in cards {
            match stacks.iter_mut().find(|stack| identical(view, actions, stack.first(), card)) {
                Some(stack) => stack.cards.push(card),
                None => stacks.push(Stack { cards: vec![card] }),
            }
        }
        (row, stacks)
    })
}

/// Whether `a` and `b` are copies nothing tells apart (see [`stacked`]).
fn identical(view: &ClientView, actions: Option<&ActionMap>, a: &PublicInstalledRunnerCard, b: &PublicInstalledRunnerCard) -> bool {
    let used = |card: &PublicInstalledRunnerCard| view.runner.once_per_turn_used.iter().any(|key: &OncePerTurnKey| key.install == Some(card.install_id));
    let a_host = |host: InstallId| view.runner.rig.iter().any(|other| other.hosted_on_program == Some(host));
    let alone = |card: &PublicInstalledRunnerCard| card.hosted_on_ice.is_none() && card.hosted_on_program.is_none() && card.hosted_cards.is_empty() && !a_host(card.install_id);
    a.card == b.card
        && a.current_strength == b.current_strength
        && a.counters == b.counters
        && alone(a)
        && alone(b)
        && used(a) == used(b)
        && actions.is_none_or(|map| same_offer(map, a.install_id, b.install_id))
}

/// Whether the map offers the same on `a` as on `b`: the same actions,
/// by kind and by the words the menu would show, and the same mood. The
/// words are what a person tells two entries apart by, so two copies
/// whose menus read the same are one menu; an action on a single copy
/// (a target named by its handle) splits them.
fn same_offer(map: &ActionMap, a: InstallId, b: InstallId) -> bool {
    let offer = |id: InstallId| {
        let mut entries: Vec<(std::mem::Discriminant<_>, &str)> = map.for_install(id).into_iter().map(|index| (std::mem::discriminant(&map.entries[index].action), map.entries[index].label.as_str())).collect();
        entries.sort_by(|x, y| x.1.cmp(y.1));
        entries
    };
    offer(a) == offer(b) && map.affordance(&Target::Install(a)) == map.affordance(&Target::Install(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Programs are the row next to the ICE from either chair, and
    /// resources the row next to the Runner.
    #[test]
    fn the_rows_are_the_table_seen_from_the_chair() {
        assert_eq!(rows_top_down(Side::Runner), [RigRow::Programs, RigRow::Hardware, RigRow::Resources]);
        assert_eq!(rows_top_down(Side::Corp), [RigRow::Resources, RigRow::Hardware, RigRow::Programs]);
        assert_eq!(RigRow::of(&CardType::Program), RigRow::Programs);
        assert_eq!(RigRow::of(&CardType::Hardware), RigRow::Hardware);
        assert_eq!(RigRow::of(&CardType::Resource), RigRow::Resources);
    }

    fn view() -> (ClientView, CardRegistry) {
        use netrunner_core::rules::GameState;
        use netrunner_session::{sweep_decks_for_seed, Seat, Session};
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        let view = Session::new(state, registry.clone(), Seat::External, Seat::External).view_for(Side::Runner);
        (view, registry)
    }

    fn install(card: &str, id: u32) -> PublicInstalledRunnerCard {
        PublicInstalledRunnerCard {
            card: netrunner_core::dsl::CardId(card.to_string()),
            install_id: InstallId(id),
            current_strength: 0,
            hosted_on_ice: None,
            hosted_on_program: None,
            hosted_cards: Vec::new(),
            hosted_cards_playable: false,
            counters: 0,
        }
    }

    /// The hardware row's stacks, as (card, how many) pairs.
    fn hardware(view: &ClientView, registry: &CardRegistry, actions: Option<&ActionMap>) -> Vec<(String, usize)> {
        let [_, (row, stacks), _] = stacked(view, registry, Side::Runner, actions);
        assert_eq!(row, RigRow::Hardware);
        stacks.iter().map(|stack| (stack.first().card.0.clone(), stack.count())).collect()
    }

    #[test]
    fn copies_nothing_tells_apart_are_one_stack_where_the_first_was() {
        let (mut view, registry) = view();
        assert!(registry.get(&netrunner_core::dsl::CardId("cyberfeeder".into())).is_some() && registry.get(&netrunner_core::dsl::CardId("docklands_pass".into())).is_some());
        view.runner.rig = vec![install("docklands_pass", 1), install("cyberfeeder", 2), install("docklands_pass", 3), install("cyberfeeder", 4)];
        assert_eq!(hardware(&view, &registry, None), [("docklands_pass".to_string(), 2), ("cyberfeeder".to_string(), 2)]);
        let [_, (_, stacks), _] = stacked(&view, &registry, Side::Runner, None);
        assert_eq!(stacks[0].install_ids().collect::<Vec<_>>(), [InstallId(1), InstallId(3)], "the first copy stands for the stack");
    }

    #[test]
    fn a_copy_that_reads_differently_leaves_its_stack() {
        let (mut view, registry) = view();
        let mut counted = install("cyberfeeder", 2);
        counted.counters = 1;
        view.runner.rig = vec![install("cyberfeeder", 1), counted, install("cyberfeeder", 3)];
        assert_eq!(hardware(&view, &registry, None), [("cyberfeeder".to_string(), 2), ("cyberfeeder".to_string(), 1)], "a counter on one copy");

        view.runner.rig = vec![install("cyberfeeder", 1), install("cyberfeeder", 2)];
        view.runner.once_per_turn_used = vec![OncePerTurnKey { card: Some(netrunner_core::dsl::CardId("cyberfeeder".into())), install: Some(InstallId(2)) }];
        assert_eq!(hardware(&view, &registry, None).len(), 2, "one copy's once-per-turn spent");

        view.runner.once_per_turn_used.clear();
        let mut hosting = install("cyberfeeder", 2);
        hosting.hosted_cards = vec![netrunner_core::dsl::CardId("sure_gamble".into())];
        view.runner.rig = vec![install("cyberfeeder", 1), hosting];
        assert_eq!(hardware(&view, &registry, None).len(), 2, "a card hosted on one copy");

        let mut elsewhere = install("cyberfeeder", 2);
        elsewhere.hosted_on_program = Some(InstallId(9));
        view.runner.rig = vec![install("cyberfeeder", 1), elsewhere];
        assert_eq!(hardware(&view, &registry, None).len(), 2, "one copy hosted on another card");
    }

    /// The offer is the last word: an action the engine lists on one
    /// copy and not the other splits them, so a stack never hides one.
    #[test]
    fn an_action_on_one_copy_splits_the_stack() {
        use netrunner_core::rules::PlayerAction;
        let (mut view, registry) = view();
        view.runner.rig = vec![install("docklands_pass", 1), install("docklands_pass", 2)];
        view.legal_actions = Vec::new();
        let quiet = ActionMap::build(&view, &registry);
        assert_eq!(hardware(&view, &registry, Some(&quiet)), [("docklands_pass".to_string(), 2)]);
        view.legal_actions = vec![PlayerAction::ActivateAbility { target: InstallId(2), ability_index: 0 }];
        let offered = ActionMap::build(&view, &registry);
        assert_eq!(hardware(&view, &registry, Some(&offered)).len(), 2);
    }

    /// A Trojan is listed under its own host and no other, even beside a
    /// second copy of the same ice.
    #[test]
    fn a_trojan_is_listed_under_the_ice_that_hosts_it_and_no_other() {
        let (mut view, _) = view();
        let mut first = install("botulus", 1);
        first.hosted_on_ice = Some(InstallId(20));
        let mut second = install("botulus", 2);
        second.hosted_on_ice = Some(InstallId(21));
        view.runner.rig = vec![first, install("cyberfeeder", 3), second];
        let ids = |ice| hosted_on(&view, InstallId(ice)).iter().map(|card| card.install_id).collect::<Vec<_>>();
        assert_eq!(ids(20), [InstallId(1)]);
        assert_eq!(ids(21), [InstallId(2)]);
        assert!(ids(22).is_empty());
    }

    /// The host is named by its title when the viewer may name it, and
    /// by the server it protects when it may not.
    #[test]
    fn a_trojans_host_is_named_as_the_viewer_may_name_it() {
        use netrunner_core::dsl::CardId;
        use netrunner_core::rules::{InstallSlot, PublicInstalledCard, ServerId};
        use netrunner_core::view::ServerView;
        let (mut view, registry) = view();
        let ice = |id: u32, card: Option<&str>| PublicInstalledCard { install_id: InstallId(id), position: 0, server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: card.is_some(), card: card.map(|c| CardId(c.into())), advancement_tokens: 0, counters: None, seen_by_runner: card.is_some() };
        view.corp.servers.retain(|s| s.server != ServerId::Hq);
        view.corp.servers.push(ServerView { server: ServerId::Hq, ice: vec![ice(20, None), ice(21, Some("ice_wall"))], root: Vec::new() });
        assert_eq!(host_label(&view, &registry, InstallId(20)), "unrezzed ice on HQ");
        assert_eq!(host_label(&view, &registry, InstallId(21)), "Ice Wall");
    }

    /// The row still lists a hosted Trojan, as a ghost, and alone.
    #[test]
    fn a_hosted_trojan_is_a_ghost_in_the_program_row() {
        let (mut view, registry) = view();
        let mut hosted = install("botulus", 1);
        hosted.hosted_on_ice = Some(InstallId(20));
        view.runner.rig = vec![hosted, install("botulus", 2)];
        let [(row, stacks), _, _] = stacked(&view, &registry, Side::Runner, None);
        assert_eq!(row, RigRow::Programs);
        assert_eq!(stacks.iter().map(|stack| (stack.first().install_id, is_ghost(stack.first()), stack.count())).collect::<Vec<_>>(), [(InstallId(1), true, 1), (InstallId(2), false, 1)]);
    }
}
