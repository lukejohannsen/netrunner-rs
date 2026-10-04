//! `netrunner_cli diag precepts`: how far each chair plays the strategy
//! guide's precepts, counted per game off the log and the states a
//! session produced — the instrument Phase 5 §25 asks for before any term
//! is written, as `diag tempo` was §5's instrument before the stance dial.
//!
//! **One counter per precept, and a precept is a behaviour, not a win.**
//! A win rate cannot tell "plays more like the guide and wins" from "wins
//! and plays no differently", and the bot rebuild (§25) has both as its
//! bar: a change that wins without playing more like the guide, or plays
//! like it and loses, must be visible as such. So every one of the
//! guide's fourteen precepts (§25's list) is a number here — the Corp's
//! centrals iced before its first remote install, its scores taken while
//! the Runner could not afford the remote, the Runner's runs on
//! unprotected centrals, its breakers installed after the ICE type they
//! answer was shown — grouped by each chair's style and faction, so a
//! plan's signature is read where the plan is.
//!
//! **A stage is read off the board, never the turn count** (the guide's
//! own rule, and §6–§8's finding that a turn clock never beat the board):
//! `stage` is the guide's three signals reduced to one reading, and the
//! clicks of each side are binned by it so "plays toward the stage it is
//! in" is a column and not a feeling. The reading is the evaluator's own
//! (`netrunner_bots::eval::stage`, Stage 3), so a term that conditions on
//! a stage and this report can never disagree about which one it is.
//!
//! **Reach is the card-testing product.** The report ends with every card
//! a seat *used* — played, installed, rezzed, activated, advanced or
//! scored by its owner — over the pass, and the cards in the decks played
//! that no seat ever used. A card no bot reaches is exercised by random
//! seats and its own tests alone, which is the list `nsg-card-pool.md`
//! kept by hand as "Bot debts"; it is regenerated from here now, and a
//! new set's stage can say which of its cards the bots play.
//!
//! **Counts, not ratios, are stored; ratios are derived once at report
//! time** (`DERIVED`), each with its numerator and denominator named, so
//! a group of games is summed and divided rather than averaged over
//! averages, and a per-game JSON record can be re-grouped by a script
//! without knowing what a ratio meant.
//!
//! Reading `GameState` directly is allowed here for the reason in `diag`'s
//! module doc: a diagnostic is not a client.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::Serialize;

use netrunner_bots::eval::{covers, punishes_runs, server_break_cost};
use netrunner_bots::{determinize, Knowledge, Style};
use rand::rngs::StdRng;
use rand::SeedableRng;
use netrunner_core::card::Faction;
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks as core_decks;
use netrunner_core::dsl::{CardDefinition, CardId, CardType, DamageType, Effect};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{GameEvent, GameState, InstallId, InstallSlot, PlayerAction, ServerId, Side};
use netrunner_session::{HistoryEntry, Seat, Session, SessionStep};

use crate::bots;
use crate::config::{BotSpec, Config};
use netrunner_client::decks;

pub struct PreceptsArgs {
    pub corp: BotSpec,
    pub runner: BotSpec,
    pub games: u32,
    pub seed: u64,
    pub simulations: usize,
    pub determinizations: Option<usize>,
    pub threads: Option<usize>,
    /// Seat each chair in its deck's own style (`Style::for_deck`),
    /// as play does, instead of the style the bot spec names — the
    /// shipped seating, and the one the by-style groups are for.
    pub deck_styles: bool,
    pub report: Option<PathBuf>,
}

// The stage is the evaluator's reading (`netrunner_bots::eval::stage`),
// so a term that conditions on it and this report bin by one definition.
use netrunner_bots::eval::{stage, Stage};

fn ice_count(state: &GameState, server: ServerId) -> usize {
    state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Ice).count()
}

/// The ice protecting `server`, innermost first — `corp.installed` keeps
/// install order, and a new piece goes outermost (CR 8.5.11a).
fn ice_on(state: &GameState, server: ServerId) -> impl Iterator<Item = &netrunner_core::rules::InstalledCard> {
    state.corp.installed.iter().filter(move |card| card.server == server && card.slot == InstallSlot::Ice)
}

fn ends_the_run(def: &CardDefinition) -> bool {
    def.subroutines.iter().any(|sub| sub.effect.can_end_the_run())
}

/// A card whose declared text makes its owner money or cards: a
/// `GainCredits`/`DrawCards` (or their `Amount` forms) anywhere in its
/// abilities or triggers, or printed recurring credits. What "trash the
/// Corp's economy" and "economy first" are about.
fn is_economy(def: &CardDefinition) -> bool {
    if def.recurring_credits.is_some() {
        return true;
    }
    let mut found = false;
    let mut look = |effect: &Effect| {
        if matches!(
            effect,
            Effect::GainCredits(side, _) | Effect::GainCreditsAmount(side, _) | Effect::DrawCards(side, _) | Effect::DrawCardsAmount(side, _)
                if *side == def.side
        ) {
            found = true;
        }
    };
    for ability in &def.abilities {
        ability.effect.for_each_effect(&mut look);
    }
    for trigger in &def.triggers {
        for effect in &trigger.effects {
            effect.for_each_effect(&mut look);
        }
    }
    found
}

fn is_breaker(def: &CardDefinition) -> bool {
    covers(def).iter().any(|c| *c)
}

/// Whether some rezzed ICE on the table is of a type `breaker` breaks —
/// "install breakers for the ICE the Corp has actually rezzed".
fn answers_shown_ice(state: &GameState, breaker: &CardDefinition, registry: &CardRegistry) -> bool {
    let covered = covers(breaker);
    state.corp.installed.iter().filter(|card| card.slot == InstallSlot::Ice && card.rezzed).any(|card| {
        registry.get(&card.card).is_some_and(|def| match def.card_type {
            CardType::Ice(subtype) => slot_of(subtype).is_none_or(|slot| covered[slot]),
            _ => false,
        })
    })
}

fn slot_of(subtype: netrunner_core::dsl::IceType) -> Option<usize> {
    use netrunner_core::dsl::IceType;
    match subtype {
        IceType::Barrier => Some(0),
        IceType::CodeGate => Some(1),
        IceType::Sentry => Some(2),
        IceType::Other => None,
    }
}

/// The remote the Corp scores out of: the one with the most ICE in front
/// of it, if any has ICE at all.
fn scoring_remote(state: &GameState) -> Option<ServerId> {
    let mut best: Option<(usize, ServerId)> = None;
    for card in state.corp.installed.iter().filter(|card| card.slot == InstallSlot::Ice) {
        if let ServerId::Remote(_) = card.server {
            let count = ice_count(state, card.server);
            if best.is_none_or(|(n, _)| count > n) {
                best = Some((count, card.server));
            }
        }
    }
    best.map(|(_, server)| server)
}

/// One game's counts, every one a sum a group can add up. Keys are
/// `side.what[.detail]`; a pair `x` and `x.games` (or another
/// denominator) is what `DERIVED` turns into a share or a mean.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts(BTreeMap<&'static str, f64>);

impl Counts {
    fn add(&mut self, key: &'static str, n: f64) {
        *self.0.entry(key).or_insert(0.0) += n;
    }

    fn bump(&mut self, key: &'static str) {
        self.add(key, 1.0);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GamePrecepts {
    pub game: u32,
    pub seed: u64,
    pub corp_deck: String,
    pub runner_deck: String,
    pub corp_style: String,
    pub runner_style: String,
    pub corp_faction: String,
    pub runner_faction: String,
    pub winner: Option<String>,
    pub end: String,
    pub turns: u32,
    pub counts: Counts,
    /// Cards each side used, by id: what reach is summed from.
    pub used: BTreeMap<String, u32>,
}

/// A run in progress, closed when the state has no active run.
struct RunCtx {
    server: ServerId,
    accesses: u32,
    rezzes: u32,
}

/// Turns states and entries into one game's counts.
struct Watcher<'a> {
    registry: &'a CardRegistry,
    corp_faction: Option<Faction>,
    counts: Counts,
    used: BTreeMap<String, u32>,
    last_turn_opened: u32,
    corp_turns: u32,
    runner_turns: u32,
    /// The stage the current turn opened in — the bin its clicks land in.
    turn_stage: Stage,
    hq_iced: Option<u32>,
    rnd_iced: Option<u32>,
    first_remote_root: Option<u32>,
    first_breaker: Option<u32>,
    /// `state.turn` each installed agenda was installed on.
    agenda_install_turn: BTreeMap<InstallId, u32>,
    /// Agendas that ended their install turn with no advancement.
    unadvanced_after_install_turn: BTreeSet<InstallId>,
    run: Option<RunCtx>,
}

impl<'a> Watcher<'a> {
    fn new(registry: &'a CardRegistry, corp_faction: Option<Faction>) -> Self {
        Watcher {
            registry,
            corp_faction,
            counts: Counts::default(),
            used: BTreeMap::new(),
            last_turn_opened: 0,
            corp_turns: 0,
            runner_turns: 0,
            turn_stage: Stage::Early,
            hq_iced: None,
            rnd_iced: None,
            first_remote_root: None,
            first_breaker: None,
            agenda_install_turn: BTreeMap::new(),
            unadvanced_after_install_turn: BTreeSet::new(),
            run: None,
        }
    }

    fn used(&mut self, card: &CardId) {
        *self.used.entry(card.0.clone()).or_insert(0) += 1;
    }

    fn def(&self, card: &CardId) -> Option<&'a CardDefinition> {
        self.registry.get(card)
    }

    /// Opens the turn `state` is in if it has not been opened: the side's
    /// turn count, the stage, the credits it starts with, and what each
    /// side could afford at that moment.
    fn open_turn(&mut self, state: &GameState) {
        if state.turn == 0 || state.turn == self.last_turn_opened {
            return;
        }
        self.last_turn_opened = state.turn;
        // Odd turns are the Corp's, even the Runner's (`GameState::turn`).
        let side = if state.turn % 2 == 1 { Side::Corp } else { Side::Runner };
        self.turn_stage = stage(state, self.registry);
        let credits = f64::from(state.resources(side).credits.0);
        match side {
            Side::Corp => {
                self.corp_turns += 1;
                self.counts.bump("corp.turns");
                self.counts.bump(stage_key(Side::Corp, self.turn_stage, "turns"));
                self.counts.add("corp.credits_at_turn_start", credits);
                let unrezzed: u32 = state
                    .corp
                    .installed
                    .iter()
                    .filter(|card| card.slot == InstallSlot::Ice && !card.rezzed)
                    .filter_map(|card| self.def(&card.card))
                    .map(|def| def.cost)
                    .sum();
                self.counts.add("corp.turn_starts.unrezzed_rez_cost", f64::from(unrezzed));
                if f64::from(unrezzed) <= credits {
                    self.counts.bump("corp.turn_starts.rez_all_affordable");
                }
            }
            Side::Runner => {
                self.runner_turns += 1;
                self.counts.bump("runner.turns");
                self.counts.bump(stage_key(Side::Runner, self.turn_stage, "turns"));
                self.counts.add("runner.credits_at_turn_start", credits);
                // Only a remote with rezzed ICE has a price to set against:
                // unrezzed ICE costs nothing to pass, so a remote of it is
                // "affordable" in a way that says nothing about the window.
                if let Some(remote) = scoring_remote(state)
                    && ice_on(state, remote).any(|card| card.rezzed)
                    && let Some(cost) = server_break_cost(state, remote, self.registry)
                {
                    self.counts.bump("runner.turn_starts.remote_priced");
                    self.counts.add("runner.turn_starts.remote_break_cost", f64::from(cost));
                    if f64::from(cost) <= credits {
                        self.counts.bump("runner.turn_starts.can_afford_remote");
                    }
                }
            }
        }
    }

    fn record(&mut self, before: &GameState, entry: &HistoryEntry, after: &GameState) {
        let side = entry.side;
        self.action(before, entry);
        for event in &entry.events {
            self.event(before, after, side, event);
        }
        // A run is over when the state has none: `RunCompleted` is not the
        // only way out (a jack-out, an ended run, a redirect).
        if after.active_run.is_none()
            && let Some(run) = self.run.take()
        {
            if run.rezzes > 0 {
                self.counts.bump("runner.runs.forced_rez");
            }
            if run.server == ServerId::RnD {
                self.counts.add("runner.rnd_runs.accesses", f64::from(run.accesses));
                if run.accesses >= 2 {
                    self.counts.bump("runner.rnd_runs.multi_access");
                }
            }
        }
    }

    fn action(&mut self, before: &GameState, entry: &HistoryEntry) {
        let side = entry.side;
        let kind = match &entry.action {
            PlayerAction::GainCreditClick { .. } => Some("credit"),
            PlayerAction::DrawCardClick { .. } => Some("draw"),
            PlayerAction::InstallCard { .. }
            | PlayerAction::InstallHardware { .. }
            | PlayerAction::InstallProgram { .. }
            | PlayerAction::InstallResource { .. }
            | PlayerAction::InstallProgramOnIce { .. } => Some("install"),
            PlayerAction::AdvanceCard { .. } => Some("advance"),
            PlayerAction::PlayOperation { .. } | PlayerAction::PlayEvent { .. } => Some("play"),
            PlayerAction::InitiateRun { .. } => Some("run"),
            PlayerAction::RemoveTag | PlayerAction::PurgeVirusCounters | PlayerAction::TrashResource { .. } => Some("other"),
            _ => None,
        };
        if let Some(kind) = kind {
            self.counts.bump(click_key(side, kind));
            self.counts.bump(stage_click_key(side, self.turn_stage, kind));
        }
        match &entry.action {
            PlayerAction::InitiateRun { .. } => {
                if before.runner.resources.clicks.0 == 1 {
                    self.counts.bump("runner.runs.last_click");
                    if punishes_runs(self.corp_faction) {
                        self.counts.bump("runner.runs.last_click_vs_punisher");
                    }
                }
            }
            PlayerAction::RemoveTag => self.counts.bump("runner.tags_cleared"),
            PlayerAction::TrashResource { .. } => self.counts.bump("corp.tagged_resource_trashes"),
            PlayerAction::EndTurn => match side {
                Side::Runner => {
                    self.counts.bump("runner.turn_ends");
                    self.counts.add("runner.grip_at_turn_end", before.runner.grip.len() as f64);
                    if before.runner.tags > 0 {
                        self.counts.bump("runner.turns_ended_tagged");
                    }
                }
                Side::Corp => {
                    for card in &before.corp.installed {
                        let agenda = self.def(&card.card).is_some_and(|def| def.card_type == CardType::Agenda);
                        if card.slot == InstallSlot::Root && agenda && card.advancement_tokens == 0 {
                            self.counts.bump("corp.agendas_unadvanced_at_turn_end");
                            if self.agenda_install_turn.get(&card.install_id) == Some(&before.turn) {
                                self.unadvanced_after_install_turn.insert(card.install_id);
                            }
                        }
                    }
                }
            },
            _ => {}
        }
    }

    fn event(&mut self, before: &GameState, after: &GameState, side: Side, event: &GameEvent) {
        match event {
            GameEvent::ClickSpent { side } => self.counts.bump(side_key(*side, "clicks")),
            GameEvent::CreditsGained { side, amount } => self.counts.add(side_key(*side, "credits_gained"), f64::from(*amount)),
            GameEvent::CreditsSpent { side, amount } => self.counts.add(side_key(*side, "credits_spent"), f64::from(*amount)),
            GameEvent::CardDrawn { side } => self.counts.bump(side_key(*side, "cards_drawn")),
            GameEvent::RunInitiated { server } => {
                self.counts.bump("runner.runs");
                self.counts.bump(stage_key(Side::Runner, self.turn_stage, "runs"));
                if punishes_runs(self.corp_faction) {
                    self.counts.bump("runner.runs.vs_punisher");
                }
                let ice = ice_count(before, *server);
                match server {
                    ServerId::Hq => {
                        self.counts.bump("runner.runs.hq");
                        self.counts.add("runner.hq_runs.corp_hand", before.corp.hq.len() as f64);
                        if before.corp.hq.len() >= 5 {
                            self.counts.bump("runner.hq_runs.corp_hand_full");
                        }
                    }
                    ServerId::RnD => self.counts.bump("runner.runs.rnd"),
                    ServerId::Archives => self.counts.bump("runner.runs.archives"),
                    ServerId::Remote(_) => self.counts.bump("runner.runs.remote"),
                }
                if matches!(server, ServerId::Hq | ServerId::RnD) {
                    self.counts.bump("runner.runs.central");
                    if ice == 0 {
                        self.counts.bump("runner.runs.central_unprotected");
                    }
                }
                if ice_on(before, *server).any(|card| !card.rezzed) {
                    self.counts.bump("runner.runs.into_unrezzed");
                }
                self.run = Some(RunCtx { server: *server, accesses: 0, rezzes: 0 });
            }
            GameEvent::RunSucceeded { .. } => self.counts.bump("runner.runs.successful"),
            GameEvent::IceRezzed { card, .. } => {
                self.used(card);
                self.counts.bump("corp.rezzes");
                if let Some(run) = self.run.as_mut() {
                    run.rezzes += 1;
                    self.counts.bump("corp.rezzes.in_run");
                    let breakable = self.def(card).is_some_and(|def| match def.card_type {
                        CardType::Ice(subtype) => {
                            let rig = before.runner.rig.iter().filter_map(|c| self.registry.get(&c.card)).map(covers).fold([false; 3], |acc, c| {
                                [acc[0] || c[0], acc[1] || c[1], acc[2] || c[2]]
                            });
                            slot_of(subtype).map_or(rig.iter().all(|c| *c), |slot| rig[slot])
                        }
                        _ => true,
                    });
                    if breakable {
                        self.counts.bump("corp.rezzes.runner_could_break");
                    }
                } else {
                    self.counts.bump("corp.rezzes.outside_run");
                }
            }
            GameEvent::CardInstalled { side: Side::Corp, install, server, .. } => {
                let Some(installed) = after.corp.installed.iter().find(|c| c.install_id == *install) else { return };
                let card = installed.card.clone();
                self.used(&card);
                let Some(def) = self.def(&card) else { return };
                match installed.slot {
                    InstallSlot::Ice => {
                        self.counts.bump("corp.ice_installs");
                        match server {
                            ServerId::Hq => {
                                self.counts.bump("corp.ice_installs.central");
                                self.hq_iced.get_or_insert(self.corp_turns);
                            }
                            ServerId::RnD => {
                                self.counts.bump("corp.ice_installs.central");
                                self.rnd_iced.get_or_insert(self.corp_turns);
                            }
                            ServerId::Archives => self.counts.bump("corp.ice_installs.central"),
                            ServerId::Remote(_) => self.counts.bump("corp.ice_installs.remote"),
                        }
                        // The piece it now stands outside: the last installed before it.
                        if let Some(inner) = ice_on(before, *server).last()
                            && let Some(inner_def) = self.def(&inner.card)
                        {
                            self.counts.bump("corp.ice_installs.over_existing");
                            match (ends_the_run(def), ends_the_run(inner_def)) {
                                (true, false) => self.counts.bump("corp.ice_installs.etr_over_non_etr"),
                                (false, true) => self.counts.bump("corp.ice_installs.non_etr_over_etr"),
                                _ => {}
                            }
                        }
                    }
                    InstallSlot::Root => {
                        let ice = ice_count(before, *server) as f64;
                        if let ServerId::Remote(_) = server
                            && self.first_remote_root.is_none()
                        {
                            self.first_remote_root = Some(self.corp_turns);
                            if self.hq_iced.is_some() && self.rnd_iced.is_some() {
                                self.counts.bump("corp.centrals_iced_before_first_remote");
                            }
                        }
                        match def.card_type {
                            CardType::Agenda => {
                                self.counts.bump("corp.agenda_installs");
                                self.counts.add("corp.agenda_installs.ice", ice);
                                if ice == 0.0 {
                                    self.counts.bump("corp.agenda_installs.naked");
                                }
                                if ice >= 2.0 {
                                    self.counts.bump("corp.agenda_installs.behind_2");
                                }
                                self.agenda_install_turn.insert(*install, after.turn);
                            }
                            CardType::Asset => {
                                if let ServerId::Remote(_) = server
                                    && ice >= 1.0
                                {
                                    self.counts.bump("corp.asset_installs_in_iced_remote");
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            GameEvent::ProgramInstalled { card, .. } | GameEvent::HardwareInstalled { card, .. } | GameEvent::ResourceInstalled { card, .. } => {
                self.used(card);
                let Some(def) = self.def(card) else { return };
                if is_breaker(def) {
                    self.counts.bump("runner.breaker_installs");
                    if answers_shown_ice(before, def, self.registry) {
                        self.counts.bump("runner.breaker_installs.type_shown");
                    }
                    if self.first_breaker.is_none() {
                        self.first_breaker = Some(self.runner_turns);
                    }
                } else if is_economy(def) {
                    self.counts.bump("runner.economy_installs");
                    if self.first_breaker.is_none() {
                        self.counts.bump("runner.economy_installs.before_first_breaker");
                    }
                }
            }
            GameEvent::EventPlayed { card, .. } => {
                self.used(card);
                self.counts.bump("runner.events");
                if self.def(card).is_some_and(is_economy) {
                    self.counts.bump("runner.events.economy");
                }
            }
            GameEvent::OperationPlayed { card, .. } => {
                self.used(card);
                self.counts.bump("corp.operations");
                if self.def(card).is_some_and(is_economy) {
                    self.counts.bump("corp.operations.economy");
                }
            }
            GameEvent::AbilityActivated { card_id, .. } => self.used(card_id),
            GameEvent::CardAccessed { card, .. } => {
                self.counts.bump("runner.accesses");
                if let Some(run) = self.run.as_mut() {
                    run.accesses += 1;
                }
                if self.def(card).is_some_and(|def| matches!(def.card_type, CardType::Asset | CardType::Upgrade) && is_economy(def)) {
                    self.counts.bump("runner.accesses.economy_assets");
                }
            }
            GameEvent::CardTrashedFromAccess { card, .. } => {
                self.counts.bump("runner.trashes.on_access");
                if self.def(card).is_some_and(|def| matches!(def.card_type, CardType::Asset | CardType::Upgrade) && is_economy(def)) {
                    self.counts.bump("runner.trashes.economy_assets");
                }
                if after.runner.resources.credits.0 < 3 {
                    self.counts.bump("runner.trashes.left_under_3");
                }
            }
            GameEvent::AgendaScored { card, server, .. } => {
                self.used(card);
                self.counts.bump("corp.scores");
                self.counts.bump(stage_key(Side::Corp, self.turn_stage, "scores"));
                self.counts.add("corp.scores.ice", ice_count(before, *server) as f64);
                if let Some(installed) = before.corp.installed.iter().find(|c| c.server == *server && c.card == *card) {
                    if installed.installed_this_turn {
                        self.counts.bump("corp.scores.same_turn_install");
                    }
                    if self.unadvanced_after_install_turn.contains(&installed.install_id) {
                        self.counts.bump("corp.scores.after_unadvanced_install_turn");
                    }
                }
                // Priced only where every rezzed piece has a rig card
                // that can pay to break it right now — a breaker whose
                // stock is spent (a Matryoshka with no copy hosted) is
                // no breaker here, since Phase 5 §28.
                if let Some(cost) = server_break_cost(before, *server, self.registry) {
                    self.counts.bump("corp.scores.priced");
                    if before.runner.resources.credits.0 < cost {
                        self.counts.bump("corp.scores.runner_could_not_afford");
                    }
                }
            }
            GameEvent::AgendaStolen { .. } => self.counts.bump("runner.steals"),
            GameEvent::AdvancementCountersPlaced { .. } => self.counts.bump("corp.advancement_by_cards"),
            GameEvent::CardAdvanced { install, .. } if side == Side::Corp => {
                if let Some(installed) = after.corp.installed.iter().find(|c| c.install_id == *install) {
                    self.used(&installed.card);
                    self.counts.bump("corp.advances");
                    if self.def(&installed.card).is_some_and(|def| def.card_type != CardType::Agenda) {
                        self.counts.bump("corp.advances.non_agenda");
                    }
                }
            }
            GameEvent::TagsGiven { side: Side::Runner, amount, .. } => {
                self.counts.add("corp.tags_given", f64::from(*amount));
                self.counts.add("runner.tags_taken", f64::from(*amount));
            }
            GameEvent::DamageTaken { damage_type, amount, .. } => {
                let key = match damage_type {
                    DamageType::Net => "corp.damage.net",
                    DamageType::Meat => "corp.damage.meat",
                    DamageType::Brain => "corp.damage.core",
                };
                self.counts.add(key, *amount as f64);
            }
            GameEvent::RunnerFlatlined => self.counts.bump("runner.flatlined"),
            _ => {}
        }
    }

    /// Scores one sample of `side`'s opponent's hidden cards against the
    /// real ones: how many were drawn, how many are cards the opponent's
    /// deck holds at all, and how many match the hidden cards themselves
    /// as a multiset. The hidden zones are the ones the seat cannot see —
    /// the other side's hand and deck, and for the Runner the Corp's
    /// facedown installs and Archives too.
    fn guess(&mut self, side: Side, kind: &'static str, sample: &GameState, real: &GameState, opponent: &netrunner_core::rules::Deck) {
        let hidden = |state: &GameState| -> Vec<CardId> {
            match side {
                Side::Corp => state.runner.grip.iter().chain(&state.runner.stack).cloned().collect(),
                Side::Runner => state
                    .corp
                    .hq
                    .iter()
                    .chain(&state.corp.r_and_d)
                    .chain(state.corp.archives.iter().filter(|a| a.facedown).map(|a| &a.card))
                    .chain(state.corp.installed.iter().filter(|c| !c.rezzed && !c.seen_by_runner).map(|c| &c.card))
                    .cloned()
                    .collect(),
            }
        };
        let guessed = hidden(sample);
        let real_cards = hidden(real);
        let mut truth: BTreeMap<&CardId, usize> = BTreeMap::new();
        for card in &real_cards {
            *truth.entry(card).or_insert(0) += 1;
        }
        let in_deck = |card: &CardId| opponent.cards.iter().any(|(id, _)| id == card);
        let mut overlap = 0usize;
        for card in &guessed {
            if let Some(left) = truth.get_mut(card).filter(|left| **left > 0) {
                *left -= 1;
                overlap += 1;
            }
        }
        let name = side_name(side);
        self.counts.add(leak(format!("{name}.{kind}.sampled")), guessed.len() as f64);
        self.counts.add(leak(format!("{name}.{kind}.in_deck")), guessed.iter().filter(|card| in_deck(card)).count() as f64);
        self.counts.add(leak(format!("{name}.{kind}.overlap")), overlap as f64);
    }

    fn finish(mut self) -> (Counts, BTreeMap<String, u32>) {
        if let Some(turn) = self.hq_iced {
            self.counts.bump("corp.hq_iced.games");
            self.counts.add("corp.hq_iced.turn", f64::from(turn));
        }
        if let Some(turn) = self.rnd_iced {
            self.counts.bump("corp.rnd_iced.games");
            self.counts.add("corp.rnd_iced.turn", f64::from(turn));
        }
        if let Some(turn) = self.first_remote_root {
            self.counts.bump("corp.first_remote_root.games");
            self.counts.add("corp.first_remote_root.turn", f64::from(turn));
        }
        if let Some(turn) = self.first_breaker {
            self.counts.bump("runner.first_breaker.games");
            self.counts.add("runner.first_breaker.turn", f64::from(turn));
        }
        (self.counts, self.used)
    }
}

fn side_key(side: Side, what: &str) -> &'static str {
    leak(format!("{}.{what}", side_name(side)))
}

fn click_key(side: Side, kind: &str) -> &'static str {
    leak(format!("{}.clicks.{kind}", side_name(side)))
}

fn stage_key(side: Side, stage: Stage, what: &str) -> &'static str {
    leak(format!("{}.stage.{}.{what}", side_name(side), stage.name()))
}

fn stage_click_key(side: Side, stage: Stage, kind: &str) -> &'static str {
    leak(format!("{}.stage.{}.clicks.{kind}", side_name(side), stage.name()))
}

/// Keys are built from a few dozen fixed parts, so interning them once
/// per distinct string keeps `Counts` a map of `&'static str` (which
/// serializes as plain keys) without a table of every combination written
/// out by hand. A process never makes more than a few hundred.
fn leak(key: String) -> &'static str {
    use std::sync::Mutex;
    static INTERNED: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut interned = INTERNED.lock().expect("key table");
    if let Some(existing) = interned.get(key.as_str()) {
        return existing;
    }
    let leaked: &'static str = Box::leak(key.into_boxed_str());
    interned.insert(leaked);
    leaked
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Corp => "corp",
        Side::Runner => "runner",
    }
}

/// A ratio the report derives from two summed counters: `numerator /
/// denominator` over a group's games, or a per-game mean when the
/// denominator is `GAMES`.
struct Derived {
    name: &'static str,
    numerator: &'static str,
    denominator: &'static str,
}

const GAMES: &str = "games";

macro_rules! derived {
    ($($name:literal = $num:literal / $den:expr),* $(,)?) => {
        &[$(Derived { name: $name, numerator: $num, denominator: $den }),*]
    };
}

/// The precepts as ratios, numbered as §25 numbers them, then the
/// economy and the stage bins.
const DERIVED: &[Derived] = derived![
    "corp.01.hq_iced_turn" = "corp.hq_iced.turn" / "corp.hq_iced.games",
    "corp.01.rnd_iced_turn" = "corp.rnd_iced.turn" / "corp.rnd_iced.games",
    "corp.01.first_remote_root_turn" = "corp.first_remote_root.turn" / "corp.first_remote_root.games",
    "corp.01.centrals_iced_before_first_remote" = "corp.centrals_iced_before_first_remote" / "corp.first_remote_root.games",
    "corp.01.ice_per_agenda_install" = "corp.agenda_installs.ice" / "corp.agenda_installs",
    "corp.01.naked_agenda_installs" = "corp.agenda_installs.naked" / "corp.agenda_installs",
    "corp.01.agenda_installs_behind_2" = "corp.agenda_installs.behind_2" / "corp.agenda_installs",
    "corp.02.scores_runner_could_not_afford" = "corp.scores.runner_could_not_afford" / "corp.scores.priced",
    "corp.02.ice_per_score" = "corp.scores.ice" / "corp.scores",
    "corp.03.scores_after_unadvanced_install_turn" = "corp.scores.after_unadvanced_install_turn" / "corp.scores",
    "corp.03.advancement_by_cards_per_game" = "corp.advancement_by_cards" / GAMES,
    "corp.04.scores_same_turn_as_install" = "corp.scores.same_turn_install" / "corp.scores",
    "corp.04.scores_per_game" = "corp.scores" / GAMES,
    "corp.05.advance_clicks_on_non_agendas" = "corp.advances.non_agenda" / "corp.advances",
    "corp.05.asset_installs_in_iced_remote_per_game" = "corp.asset_installs_in_iced_remote" / GAMES,
    "corp.05.agendas_unadvanced_at_turn_end_per_turn" = "corp.agendas_unadvanced_at_turn_end" / "corp.turns",
    "corp.06.etr_installed_over_non_etr" = "corp.ice_installs.etr_over_non_etr" / "corp.ice_installs.over_existing",
    "corp.06.non_etr_installed_over_etr" = "corp.ice_installs.non_etr_over_etr" / "corp.ice_installs.over_existing",
    "corp.06.rezzes_in_a_run" = "corp.rezzes.in_run" / "corp.rezzes",
    "corp.06.rezzes_the_runner_could_break" = "corp.rezzes.runner_could_break" / "corp.rezzes.in_run",
    "corp.06.rezzes_per_game" = "corp.rezzes" / GAMES,
    "corp.07.tags_given_per_game" = "corp.tags_given" / GAMES,
    "corp.07.tagged_resource_trashes_per_game" = "corp.tagged_resource_trashes" / GAMES,
    "corp.07.net_damage_per_game" = "corp.damage.net" / GAMES,
    "corp.07.meat_damage_per_game" = "corp.damage.meat" / GAMES,
    "corp.07.flatlines" = "runner.flatlined" / GAMES,
    "runner.08.central_runs_on_unprotected" = "runner.runs.central_unprotected" / "runner.runs.central",
    "runner.08.runs_into_unrezzed_ice" = "runner.runs.into_unrezzed" / "runner.runs",
    "runner.08.runs_that_forced_a_rez" = "runner.runs.forced_rez" / "runner.runs.into_unrezzed",
    "runner.08.runs_per_game" = "runner.runs" / GAMES,
    "runner.08.successful_runs" = "runner.runs.successful" / "runner.runs",
    "runner.09.hq_runs_share" = "runner.runs.hq" / "runner.runs",
    "runner.09.corp_hand_at_hq_run" = "runner.hq_runs.corp_hand" / "runner.runs.hq",
    "runner.09.hq_runs_into_a_full_hand" = "runner.hq_runs.corp_hand_full" / "runner.runs.hq",
    "runner.10.rnd_runs_share" = "runner.runs.rnd" / "runner.runs",
    "runner.10.rnd_accesses_per_run" = "runner.rnd_runs.accesses" / "runner.runs.rnd",
    "runner.10.rnd_runs_with_multi_access" = "runner.rnd_runs.multi_access" / "runner.runs.rnd",
    "runner.11.first_breaker_turn" = "runner.first_breaker.turn" / "runner.first_breaker.games",
    "runner.11.breaker_installs_per_game" = "runner.breaker_installs" / GAMES,
    "runner.11.breakers_for_ice_already_shown" = "runner.breaker_installs.type_shown" / "runner.breaker_installs",
    "runner.11.economy_installs_per_game" = "runner.economy_installs" / GAMES,
    "runner.11.economy_installs_before_first_breaker" = "runner.economy_installs.before_first_breaker" / "runner.economy_installs",
    "runner.12.economy_assets_trashed_when_accessed" = "runner.trashes.economy_assets" / "runner.accesses.economy_assets",
    "runner.12.trashes_on_access_per_game" = "runner.trashes.on_access" / GAMES,
    "runner.12.trashes_that_left_under_3_credits" = "runner.trashes.left_under_3" / "runner.trashes.on_access",
    "runner.13.runs_on_the_last_click" = "runner.runs.last_click" / "runner.runs",
    "runner.13.last_click_runs_vs_jinteki_or_nbn" = "runner.runs.last_click_vs_punisher" / "runner.runs.vs_punisher",
    "runner.13.tags_cleared_per_tag_taken" = "runner.tags_cleared" / "runner.tags_taken",
    "runner.13.turns_ended_tagged" = "runner.turns_ended_tagged" / "runner.turn_ends",
    "runner.13.grip_at_turn_end" = "runner.grip_at_turn_end" / "runner.turn_ends",
    "runner.13.flatlined" = "runner.flatlined" / GAMES,
    "runner.14.remote_runs_share" = "runner.runs.remote" / "runner.runs",
    "runner.14.economy_events_per_game" = "runner.events.economy" / GAMES,
    "runner.14.steals_per_game" = "runner.steals" / GAMES,
    "knowledge.corp.guess_in_deck" = "corp.guess.in_deck" / "corp.guess.sampled",
    "knowledge.corp.guess_overlap" = "corp.guess.overlap" / "corp.guess.sampled",
    "knowledge.corp.naive_in_deck" = "corp.naive.in_deck" / "corp.naive.sampled",
    "knowledge.corp.naive_overlap" = "corp.naive.overlap" / "corp.naive.sampled",
    "knowledge.runner.guess_in_deck" = "runner.guess.in_deck" / "runner.guess.sampled",
    "knowledge.runner.guess_overlap" = "runner.guess.overlap" / "runner.guess.sampled",
    "knowledge.runner.naive_in_deck" = "runner.naive.in_deck" / "runner.naive.sampled",
    "knowledge.runner.naive_overlap" = "runner.naive.overlap" / "runner.naive.sampled",
    "economy.corp.credits_gained_per_click" = "corp.credits_gained" / "corp.clicks",
    "economy.corp.credit_click_share" = "corp.clicks.credit" / "corp.clicks",
    "economy.corp.draw_click_share" = "corp.clicks.draw" / "corp.clicks",
    "economy.corp.credits_at_turn_start" = "corp.credits_at_turn_start" / "corp.turns",
    "economy.corp.turn_starts_able_to_rez_everything" = "corp.turn_starts.rez_all_affordable" / "corp.turns",
    "economy.corp.economy_operations_per_game" = "corp.operations.economy" / GAMES,
    "economy.runner.credits_gained_per_click" = "runner.credits_gained" / "runner.clicks",
    "economy.runner.credit_click_share" = "runner.clicks.credit" / "runner.clicks",
    "economy.runner.draw_click_share" = "runner.clicks.draw" / "runner.clicks",
    "economy.runner.credits_at_turn_start" = "runner.credits_at_turn_start" / "runner.turns",
    "economy.runner.turn_starts_able_to_break_the_remote" = "runner.turn_starts.can_afford_remote" / "runner.turn_starts.remote_priced",
    "economy.runner.remote_break_cost_at_turn_start" = "runner.turn_starts.remote_break_cost" / "runner.turn_starts.remote_priced",
    "stage.corp.early_turns" = "corp.stage.early.turns" / "corp.turns",
    "stage.corp.middle_turns" = "corp.stage.middle.turns" / "corp.turns",
    "stage.corp.late_turns" = "corp.stage.late.turns" / "corp.turns",
    "stage.corp.early.credit_clicks_per_turn" = "corp.stage.early.clicks.credit" / "corp.stage.early.turns",
    "stage.corp.early.install_clicks_per_turn" = "corp.stage.early.clicks.install" / "corp.stage.early.turns",
    "stage.corp.early.advance_clicks_per_turn" = "corp.stage.early.clicks.advance" / "corp.stage.early.turns",
    "stage.corp.middle.credit_clicks_per_turn" = "corp.stage.middle.clicks.credit" / "corp.stage.middle.turns",
    "stage.corp.middle.install_clicks_per_turn" = "corp.stage.middle.clicks.install" / "corp.stage.middle.turns",
    "stage.corp.middle.advance_clicks_per_turn" = "corp.stage.middle.clicks.advance" / "corp.stage.middle.turns",
    "stage.corp.late.credit_clicks_per_turn" = "corp.stage.late.clicks.credit" / "corp.stage.late.turns",
    "stage.corp.late.install_clicks_per_turn" = "corp.stage.late.clicks.install" / "corp.stage.late.turns",
    "stage.corp.late.advance_clicks_per_turn" = "corp.stage.late.clicks.advance" / "corp.stage.late.turns",
    "stage.corp.early.scores" = "corp.stage.early.scores" / "corp.scores",
    "stage.corp.middle.scores" = "corp.stage.middle.scores" / "corp.scores",
    "stage.corp.late.scores" = "corp.stage.late.scores" / "corp.scores",
    "stage.runner.early_turns" = "runner.stage.early.turns" / "runner.turns",
    "stage.runner.middle_turns" = "runner.stage.middle.turns" / "runner.turns",
    "stage.runner.late_turns" = "runner.stage.late.turns" / "runner.turns",
    "stage.runner.early.runs_per_turn" = "runner.stage.early.runs" / "runner.stage.early.turns",
    "stage.runner.early.install_clicks_per_turn" = "runner.stage.early.clicks.install" / "runner.stage.early.turns",
    "stage.runner.early.credit_clicks_per_turn" = "runner.stage.early.clicks.credit" / "runner.stage.early.turns",
    "stage.runner.middle.runs_per_turn" = "runner.stage.middle.runs" / "runner.stage.middle.turns",
    "stage.runner.middle.install_clicks_per_turn" = "runner.stage.middle.clicks.install" / "runner.stage.middle.turns",
    "stage.runner.middle.credit_clicks_per_turn" = "runner.stage.middle.clicks.credit" / "runner.stage.middle.turns",
    "stage.runner.late.runs_per_turn" = "runner.stage.late.runs" / "runner.stage.late.turns",
    "stage.runner.late.install_clicks_per_turn" = "runner.stage.late.clicks.install" / "runner.stage.late.turns",
    "stage.runner.late.credit_clicks_per_turn" = "runner.stage.late.clicks.credit" / "runner.stage.late.turns",
];

/// The derived lines every group prints; the JSON has them all.
const HEADLINE: &[&str] = &[
    "corp.01.centrals_iced_before_first_remote",
    "corp.01.ice_per_agenda_install",
    "corp.02.scores_runner_could_not_afford",
    "corp.03.scores_after_unadvanced_install_turn",
    "corp.04.scores_same_turn_as_install",
    "corp.05.advance_clicks_on_non_agendas",
    "corp.06.etr_installed_over_non_etr",
    "corp.06.rezzes_the_runner_could_break",
    "corp.07.tags_given_per_game",
    "runner.08.central_runs_on_unprotected",
    "runner.08.runs_that_forced_a_rez",
    "runner.09.hq_runs_into_a_full_hand",
    "runner.10.rnd_runs_with_multi_access",
    "runner.11.breakers_for_ice_already_shown",
    "runner.12.economy_assets_trashed_when_accessed",
    "runner.13.runs_on_the_last_click",
    "runner.14.remote_runs_share",
    "knowledge.corp.guess_in_deck",
    "knowledge.runner.guess_in_deck",
];

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub key: String,
    pub games: usize,
    pub corp_win_share: f64,
    /// Every counter summed over the group's games, divided by games.
    pub per_game: BTreeMap<String, f64>,
    /// `DERIVED`, over the group's summed counters; a ratio whose
    /// denominator is zero is left out rather than shown as a number.
    pub derived: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reach {
    /// Cards used by their owner over the pass, and how often.
    pub used: BTreeMap<String, u32>,
    /// Cards in the decks played (identities included) that no seat ever
    /// used — reached by nothing but the other seating and their tests.
    pub unused_in_pass: Vec<String>,
    pub cards_in_pass: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreceptsReport {
    pub corp: String,
    pub runner: String,
    pub format: NsgFormat,
    pub games: u32,
    pub seed: u64,
    pub matchups: usize,
    pub deck_styles: bool,
    pub stage_rule: &'static str,
    pub groups: Vec<Group>,
    pub reach: Reach,
    pub per_game: Vec<GamePrecepts>,
}

const STAGE_RULE: &str = "late: either side within 2 points of the target; early: HQ or R&D unprotected, or no remote with ICE; middle: otherwise";

pub fn run(args: &PreceptsArgs, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::sample_deck_registry();
    let matchups = config.matchups(&registry)?;
    let format: NsgFormat = config.format.into();
    let pool = match args.threads {
        Some(threads) => rayon::ThreadPoolBuilder::new().num_threads(threads).build()?,
        None => rayon::ThreadPoolBuilder::new().build()?,
    };
    let styled = |spec| if args.deck_styles { format!("{}, deck styles", describe(spec)) } else { describe(spec) };
    let corp = styled(args.corp);
    let runner = styled(args.runner);
    println!(
        "playing {} games over {} {format:?} matchups, {corp} Corp vs {runner} Runner, seed {}...",
        args.games,
        matchups.len(),
        args.seed
    );

    let games: Vec<GamePrecepts> = pool.install(|| {
        (0..args.games)
            .into_par_iter()
            .map(|game| play(game, args, &registry, &matchups, config))
            .collect::<Result<Vec<_>, String>>()
    })?;

    let report = summarise(corp, runner, format, matchups.len(), args, games, &registry, &matchups);
    print_report(&report);
    if let Some(path) = &args.report {
        fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("\nreport written to {}", path.display());
    }
    Ok(())
}

fn play(
    game: u32,
    args: &PreceptsArgs,
    registry: &CardRegistry,
    matchups: &[(core_decks::DeckFile, core_decks::DeckFile)],
    config: &Config,
) -> Result<GamePrecepts, String> {
    let seed = args.seed.wrapping_add(u64::from(game));
    let (corp_deck, runner_deck) = &matchups[game as usize % matchups.len()];
    let (state, _events) =
        GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), registry, seed).map_err(|e| format!("{e:?}"))?;
    let style = |spec: BotSpec, deck: &core_decks::DeckFile| -> Result<Style, String> {
        if args.deck_styles { Style::for_deck(deck) } else { Ok(spec.style) }
    };
    let corp_style = style(args.corp, corp_deck)?;
    let runner_style = style(args.runner, runner_deck)?;
    let setup = |style: Style, deck: &core_decks::DeckFile| bots::AgentSetup {
        simulations: args.simulations,
        determinizations: args.determinizations,
        style,
        knowledge: config.knowledge(deck),
        ..bots::AgentSetup::new(args.simulations)
    };
    let corp = bots::make_seat_agent(args.corp.level, args.corp.kind, Side::Corp, seed, setup(corp_style, corp_deck), &config.model)?
        .ok_or("the Corp seat must be a bot that can take one")?;
    let runner = bots::make_seat_agent(
        args.runner.level,
        args.runner.kind,
        Side::Runner,
        seed.wrapping_add(1),
        setup(runner_style, runner_deck),
        &config.model,
    )?
    .ok_or("the Runner seat must be a bot that can take one")?;
    let faction = |deck: &core_decks::DeckFile| registry.get(&deck.identity).and_then(|def| def.faction);
    let corp_faction = faction(corp_deck);
    let runner_faction = faction(runner_deck);

    let mut session = Session::new(state, registry.clone(), Seat::Agent(corp), Seat::Agent(runner));
    let mut watcher = Watcher::new(registry, corp_faction);
    // What each seat knows, kept beside the bot and shown the same views
    // (`Session::last_view`), so the guess-quality line samples with the
    // memory the seat had; `naive` is a seat that knows nothing — Casual,
    // no deck, no memory — sampled from the same views, the line Stage 2
    // is measured against.
    let mut knowledge = [config.knowledge(corp_deck), config.knowledge(runner_deck)];
    let naive = Knowledge::default();
    let decks = [corp_deck.to_deck(), runner_deck.to_deck()];
    let mut guess_rng = StdRng::seed_from_u64(seed ^ 0x6E55_0000_0000_0000);
    let mut turn_sampled = 0;
    let (winner, end) = loop {
        let before = session.state().clone();
        watcher.open_turn(&before);
        match session.step() {
            SessionStep::Applied { side } => {
                if let Some(entry) = session.last_entry() {
                    watcher.record(&before, entry, session.state());
                }
                if let Some(view) = session.last_view() {
                    knowledge[side as usize].observe(view);
                    // Once a turn, on the turn side's first decision: how
                    // good a guess the seat's sample of the opponent's
                    // hidden cards is, against the cards really there.
                    let turn_side = if before.turn % 2 == 1 { Side::Corp } else { Side::Runner };
                    if before.turn > 0 && before.turn != turn_sampled && side == turn_side {
                        turn_sampled = before.turn;
                        let opponent = &decks[side.other() as usize];
                        let sample = determinize(view, registry, &knowledge[side as usize], &mut guess_rng);
                        watcher.guess(side, "guess", &sample, &before, opponent);
                        let sample = determinize(view, registry, &naive, &mut guess_rng);
                        watcher.guess(side, "naive", &sample, &before, opponent);
                    }
                }
            }
            SessionStep::Ended { winner, reason } => break (Some(winner), format!("{reason:?}")),
            SessionStep::Stalled(reason) => break (None, format!("stalled: {reason:?}")),
            SessionStep::Awaiting { .. } => break (None, "awaiting".to_string()),
        }
    };
    let turns = session.state().turn;
    let (mut counts, used) = watcher.finish();
    match winner {
        Some(Side::Corp) => counts.bump("corp.wins"),
        Some(Side::Runner) => counts.bump("runner.wins"),
        None => {}
    }
    Ok(GamePrecepts {
        game,
        seed,
        corp_deck: corp_deck.id.clone(),
        runner_deck: runner_deck.id.clone(),
        corp_style: corp_style.to_string(),
        runner_style: runner_style.to_string(),
        corp_faction: faction_name(corp_faction),
        runner_faction: faction_name(runner_faction),
        winner: winner.map(|side| format!("{side:?}")),
        end,
        turns,
        counts,
        used,
    })
}

fn faction_name(faction: Option<Faction>) -> String {
    faction.map_or_else(|| "unknown".to_string(), |faction| format!("{faction:?}").to_lowercase())
}

fn group(key: String, games: &[&GamePrecepts]) -> Group {
    let n = games.len().max(1) as f64;
    let mut sums: BTreeMap<&'static str, f64> = BTreeMap::new();
    for game in games {
        for (k, v) in &game.counts.0 {
            *sums.entry(k).or_insert(0.0) += v;
        }
    }
    let total = |key: &str| -> f64 {
        if key == GAMES { games.len() as f64 } else { sums.get(key).copied().unwrap_or(0.0) }
    };
    let derived = DERIVED
        .iter()
        .filter_map(|d| {
            let den = total(d.denominator);
            (den > 0.0).then(|| (d.name.to_string(), total(d.numerator) / den))
        })
        .collect();
    Group {
        key,
        games: games.len(),
        corp_win_share: games.iter().filter(|g| g.winner.as_deref() == Some("Corp")).count() as f64 / n,
        per_game: sums.iter().map(|(k, v)| ((*k).to_string(), v / n)).collect(),
        derived,
    }
}

#[allow(clippy::too_many_arguments)]
fn summarise(
    corp: String,
    runner: String,
    format: NsgFormat,
    matchup_count: usize,
    args: &PreceptsArgs,
    games: Vec<GamePrecepts>,
    registry: &CardRegistry,
    matchups: &[(core_decks::DeckFile, core_decks::DeckFile)],
) -> PreceptsReport {
    let all: Vec<&GamePrecepts> = games.iter().collect();
    let mut groups = vec![group("all".to_string(), &all)];
    let by = |label: &str, pick: fn(&GamePrecepts) -> &String| -> Vec<Group> {
        let keys: BTreeSet<&String> = games.iter().map(pick).collect();
        keys.into_iter()
            .map(|key| {
                let mine: Vec<&GamePrecepts> = games.iter().filter(|g| pick(g) == key).collect();
                group(format!("{label}:{key}"), &mine)
            })
            .collect()
    };
    groups.extend(by("corp_style", |g| &g.corp_style));
    groups.extend(by("runner_style", |g| &g.runner_style));
    groups.extend(by("corp_faction", |g| &g.corp_faction));
    groups.extend(by("runner_faction", |g| &g.runner_faction));

    let mut used: BTreeMap<String, u32> = BTreeMap::new();
    for game in &games {
        for (card, n) in &game.used {
            *used.entry(card.clone()).or_insert(0) += n;
        }
    }
    // Game n plays `matchups[n % len]`, so the decks of the pass are the
    // first `games` matchups, or all of them once the pass is a full one.
    let mut in_pass: BTreeSet<String> = BTreeSet::new();
    for (corp_deck, runner_deck) in matchups.iter().take(games.len().min(matchups.len())) {
        for deck in [corp_deck, runner_deck] {
            // The identity is a card a seat uses too (its ability), so it
            // is in the pass beside the list.
            for card in deck.cards.iter().map(|entry| &entry.card).chain(std::iter::once(&deck.identity)) {
                if registry.get(card).is_some() {
                    in_pass.insert(card.0.clone());
                }
            }
        }
    }
    let unused_in_pass: Vec<String> = in_pass.iter().filter(|card| !used.contains_key(*card)).cloned().collect();
    PreceptsReport {
        corp,
        runner,
        format,
        games: args.games,
        seed: args.seed,
        matchups: matchup_count,
        deck_styles: args.deck_styles,
        stage_rule: STAGE_RULE,
        groups,
        reach: Reach { used, unused_in_pass, cards_in_pass: in_pass.len() },
        per_game: games,
    }
}

fn describe(spec: BotSpec) -> String {
    match spec.level {
        Some(level) => format!("level:{level:?}:{}", spec.style).to_lowercase(),
        None => format!("{:?}:{}", spec.kind, spec.style).to_lowercase(),
    }
}

fn print_report(report: &PreceptsReport) {
    println!(
        "\n{} games over {} {:?} matchups, {} Corp vs {} Runner, seed {}",
        report.games, report.matchups, report.format, report.corp, report.runner, report.seed
    );
    println!("stage: {}", report.stage_rule);
    for group in &report.groups {
        let headline_only = group.key != "all";
        println!("\n[{}] {} games, Corp win share {:.3}", group.key, group.games, group.corp_win_share);
        for (name, value) in &group.derived {
            if headline_only && !HEADLINE.contains(&name.as_str()) {
                continue;
            }
            println!("  {name:<52} {value:>8.3}");
        }
    }
    println!(
        "\nreach: {} of the {} cards in the pass were used by a seat; {} never were",
        report.reach.used.len(),
        report.reach.cards_in_pass,
        report.reach.unused_in_pass.len()
    );
    if !report.reach.unused_in_pass.is_empty() {
        println!("  unused: {}", report.reach.unused_in_pass.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every ratio's numerator and denominator is a key some counter
    /// writes, or `GAMES` — a typo here would be a line that silently
    /// never appears.
    #[test]
    fn every_derived_ratio_reads_keys_the_watcher_writes() {
        let source = include_str!("precepts.rs");
        for d in DERIVED {
            for key in [d.numerator, d.denominator] {
                if key == GAMES {
                    continue;
                }
                let written = source.contains(&format!("\"{key}\""));
                let built = key.contains(".stage.") || key.starts_with("corp.clicks") || key.starts_with("runner.clicks")
                    || key.ends_with(".clicks") || key.ends_with("credits_gained") || key.ends_with("credits_spent") || key.ends_with("cards_drawn");
                assert!(written || built, "{key} is read by {} and written nowhere", d.name);
            }
        }
        for name in HEADLINE {
            assert!(DERIVED.iter().any(|d| d.name == *name), "{name} is not a derived ratio");
        }
    }

    /// Keys built from parts intern to one `&'static str` each, so the
    /// counts map does not grow a new key per turn.
    #[test]
    fn built_keys_intern() {
        let a = stage_click_key(Side::Corp, Stage::Early, "credit");
        let b = stage_click_key(Side::Corp, Stage::Early, "credit");
        assert!(std::ptr::eq(a, b));
        assert_eq!(a, "corp.stage.early.clicks.credit");
    }
}
