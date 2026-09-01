//! Rules coverage: what a batch of real matches actually exercised.
//!
//! Every per-card test in the workspace was green while no program could be
//! installed, any subroutine could be broken for free, and stolen agendas
//! stayed in the Corp's deck (ROADMAP, "Rules Audit"). Those bugs shared a
//! shape — a rule that silently never ran — and per-card tests cannot see
//! that shape: they script `apply_action` calls to a card and check the
//! card. Only real play, counted, shows that a whole class of action or a
//! whole card was never reached. This module is that count.
//!
//! It is fed from `MatchHistory`, which `Session::apply` already records
//! for every applied action, so it needs no hook inside the loop and no
//! change to the engine. It is homed here rather than in `netrunner_core`
//! (which owns rules, not reports) or `netrunner_bots` (which owns
//! players): the session owns the match record, and this aggregates it.
//!
//! Two consumers, deliberately with the same numbers:
//!
//! - `netrunner_cli --headless --report` renders a table and writes a
//!   sorted JSON file, so a rules fix can be measured before and after by
//!   `diff`ing two reports — the way the memory-cost fix was measured as
//!   "0 program installs → 3".
//! - Both agent-driven sweeps accumulate a `Coverage` across their seeds
//!   and then apply the gates at the bottom of this file, so an action
//!   class or a sample-deck card that stops being reachable fails
//!   `cargo test` rather than waiting for someone to count by hand.
//!
//! Variant names come from the `Debug` rendering (`PlayerAction::
//! variant_name`, `Effect::variant_name`, and `variant_name` here for
//! events), so a new variant is counted the day it is added. The keys are
//! all `BTreeMap<String, _>` so the JSON is sorted and diffable.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, CardType, Effect, Trigger};
use netrunner_core::rules::{GameEvent, PlayerAction, ServerId};

use crate::history::{HistoryEntry, MatchHistory};
use crate::session::{SessionStep, StallReason};

/// The variant name of any `Debug`-rendered enum value: everything up to
/// the first payload delimiter.
pub fn variant_name(rendered: &str) -> String {
    rendered.split(['(', '{', ' ']).next().unwrap_or(rendered).to_string()
}

/// What happened to one card across the batch. A card is *seen* when any
/// counter is non-zero — the bar the card gate applies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardCoverage {
    pub installed: u64,
    pub rezzed: u64,
    pub played: u64,
    pub activated: u64,
    pub accessed: u64,
    pub stolen: u64,
    pub scored: u64,
    pub trashed: u64,
    pub subroutines_fired: u64,
    pub subroutines_broken: u64,
    /// Subroutines broken by `PlayerAction::BreakSubroutineWithClick`
    /// specifically — the bioroid click-break, which is the one break path
    /// that owes nothing to an icebreaker.
    pub click_broken: u64,
}

impl CardCoverage {
    pub fn seen(&self) -> bool {
        *self != Self::default()
    }
}

/// How runs on one kind of server ended. Keyed by `"Hq"`, `"RnD"`,
/// `"Archives"` and `"Remote"` — every remote folded together, since which
/// remote number a run targeted is not a rules question.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOutcomes {
    pub initiated: u64,
    /// `GameEvent::ServerApproached` — reached the server; may still be
    /// ended there by an approach ability, or jacked out of.
    pub approached_server: u64,
    pub succeeded: u64,
    pub jacked_out: u64,
    pub ended_by_effect: u64,
    /// `GameEvent::RunCompleted` — the run's access step finished.
    pub completed: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub games: u64,
    pub steps: u64,
    /// `"Ended/Corp/AgendaThreshold"`, `"Stalled/BudgetExhausted"`,
    /// `"Stalled/NoLegalActions/Runner"`, or `"Unfinished"` for a match
    /// absorbed before it reached a conclusion.
    pub end_reasons: BTreeMap<String, u64>,
    /// `PlayerAction` variant → applied count.
    pub actions: BTreeMap<String, u64>,
    /// `"Corp/DrawCardClick"` → applied count.
    pub actions_by_side: BTreeMap<String, u64>,
    /// `GameEvent` variant → emitted count.
    pub events: BTreeMap<String, u64>,
    pub cards: BTreeMap<String, CardCoverage>,
    pub runs: BTreeMap<String, RunOutcomes>,
    /// `Effect` variant → count of resolutions this report could see.
    /// Exact for subroutines (`GameEvent::SubroutineFired` carries the
    /// effect); for activated abilities it is inferred by walking the
    /// registry's declared effect tree, so a branch of an `EffectIf` that
    /// did not fire is still counted. An exact `GameEvent::TriggerFired`
    /// is the honest v2 — see ROADMAP.
    pub effects_seen: BTreeMap<String, u64>,
    /// `"card_id/OnPlay"` → count of times that card's *declared* trigger
    /// had its event happen to that card. Named *eligible*, not *fired*:
    /// it proves the trigger's moment arrived, not that its requirement
    /// passed. Self-referential triggers only (the event names the card);
    /// board-wide triggers like `OnTurnStart` need state this report does
    /// not have.
    pub triggers_eligible: BTreeMap<String, u64>,
}

fn bump(map: &mut BTreeMap<String, u64>, key: impl Into<String>) {
    *map.entry(key.into()).or_default() += 1;
}

fn server_key(server: &ServerId) -> &'static str {
    match server {
        ServerId::Hq => "Hq",
        ServerId::RnD => "RnD",
        ServerId::Archives => "Archives",
        ServerId::Remote(_) => "Remote",
    }
}

impl Coverage {
    /// Records one match's history and how it ended.
    pub fn absorb_match(&mut self, history: &MatchHistory, registry: &CardRegistry, outcome: &SessionStep) {
        self.games += 1;
        let reason = match outcome {
            SessionStep::Ended { winner, reason } => format!("Ended/{winner:?}/{reason:?}"),
            SessionStep::Stalled(StallReason::NoLegalActions { side }) => format!("Stalled/NoLegalActions/{side:?}"),
            SessionStep::Stalled(reason) => format!("Stalled/{reason:?}"),
            SessionStep::Applied { .. } | SessionStep::Awaiting { .. } => "Unfinished".to_string(),
        };
        bump(&mut self.end_reasons, reason);
        for entry in history.entries() {
            self.absorb_entry(entry, registry);
        }
    }

    /// Records one applied action and the events it produced.
    pub fn absorb_entry(&mut self, entry: &HistoryEntry, registry: &CardRegistry) {
        self.steps += 1;
        let action = entry.action.variant_name();
        bump(&mut self.actions_by_side, format!("{:?}/{action}", entry.side));
        bump(&mut self.actions, action);

        let click_break = matches!(entry.action, PlayerAction::BreakSubroutineWithClick { .. });
        for event in &entry.events {
            bump(&mut self.events, variant_name(&format!("{event:?}")));
            self.absorb_event(event, registry, click_break);
        }
    }

    fn card(&mut self, card: &CardId) -> &mut CardCoverage {
        self.cards.entry(card.0.clone()).or_default()
    }

    fn run(&mut self, server: &ServerId) -> &mut RunOutcomes {
        self.runs.entry(server_key(server).to_string()).or_default()
    }

    fn note_effect_tree(&mut self, effect: &Effect) {
        let mut names = Vec::new();
        effect.for_each_effect(&mut |e| names.push(e.variant_name()));
        for name in names {
            bump(&mut self.effects_seen, name);
        }
    }

    /// Counts the card's declared `trigger` as having had its moment, if
    /// the card declares it at all.
    fn note_trigger_eligible(&mut self, card: &CardId, trigger: Trigger, registry: &CardRegistry) {
        let declares = registry.get(card).is_some_and(|def| def.triggers.iter().any(|t| t.trigger == trigger));
        if declares {
            bump(&mut self.triggers_eligible, format!("{}/{trigger:?}", card.0));
        }
    }

    fn absorb_event(&mut self, event: &GameEvent, registry: &CardRegistry, click_break: bool) {
        match event {
            GameEvent::CardInstalled { card, .. }
            | GameEvent::HardwareInstalled { card, .. }
            | GameEvent::ProgramInstalled { card, .. }
            | GameEvent::ResourceInstalled { card, .. } => {
                self.card(card).installed += 1;
                self.note_trigger_eligible(card, Trigger::OnInstall, registry);
            }
            GameEvent::IceRezzed { card, .. } => {
                self.card(card).rezzed += 1;
                self.note_trigger_eligible(card, Trigger::OnRez, registry);
            }
            GameEvent::EventPlayed { card, .. } | GameEvent::OperationPlayed { card, .. } => {
                self.card(card).played += 1;
                self.note_trigger_eligible(card, Trigger::OnPlay, registry);
            }
            GameEvent::AbilityActivated { card_id, ability_index, .. } => {
                self.card(card_id).activated += 1;
                if let Some(effect) =
                    registry.get(card_id).and_then(|def| def.abilities.get(*ability_index)).map(|a| a.effect.clone())
                {
                    self.note_effect_tree(&effect);
                }
            }
            GameEvent::CardAccessed { card, .. } => {
                self.card(card).accessed += 1;
                self.note_trigger_eligible(card, Trigger::OnAccessed, registry);
            }
            GameEvent::IceEncountered { card_id, .. } => {
                self.note_trigger_eligible(card_id, Trigger::OnEncounter, registry);
            }
            GameEvent::AgendaStolen { card, .. } => {
                self.card(card).stolen += 1;
                self.note_trigger_eligible(card, Trigger::OnAgendaStolen, registry);
            }
            GameEvent::AgendaScored { card, .. } => {
                self.card(card).scored += 1;
                self.note_trigger_eligible(card, Trigger::OnAgendaScored, registry);
            }
            GameEvent::CardTrashed { card, .. } | GameEvent::CardRemovedFromGame { card, .. } => {
                self.card(card).trashed += 1;
            }
            GameEvent::CardTrashedFromAccess { card, .. } => {
                self.card(card).trashed += 1;
                self.note_trigger_eligible(card, Trigger::OnTrashedFromAccess, registry);
            }
            GameEvent::SubroutineFired { card_id, effect, .. } => {
                self.card(card_id).subroutines_fired += 1;
                self.note_effect_tree(effect);
            }
            GameEvent::SubroutineBroken { card_id, .. } => {
                let card = self.card(card_id);
                card.subroutines_broken += 1;
                if click_break {
                    card.click_broken += 1;
                }
            }
            GameEvent::RunInitiated { server } => self.run(server).initiated += 1,
            GameEvent::ServerApproached { server } => self.run(server).approached_server += 1,
            GameEvent::RunSucceeded { server } => self.run(server).succeeded += 1,
            GameEvent::RunJackedOut { server } => self.run(server).jacked_out += 1,
            GameEvent::RunEndedByEffect { server } => self.run(server).ended_by_effect += 1,
            GameEvent::RunCompleted { server } => self.run(server).completed += 1,
            _ => {}
        }
    }

    /// Adds another report's counts to this one — for a driver that runs
    /// games in parallel and merges per-worker reports.
    pub fn merge(&mut self, other: &Coverage) {
        fn merge_counts(into: &mut BTreeMap<String, u64>, from: &BTreeMap<String, u64>) {
            for (key, count) in from {
                *into.entry(key.clone()).or_default() += count;
            }
        }
        self.games += other.games;
        self.steps += other.steps;
        merge_counts(&mut self.end_reasons, &other.end_reasons);
        merge_counts(&mut self.actions, &other.actions);
        merge_counts(&mut self.actions_by_side, &other.actions_by_side);
        merge_counts(&mut self.events, &other.events);
        merge_counts(&mut self.effects_seen, &other.effects_seen);
        merge_counts(&mut self.triggers_eligible, &other.triggers_eligible);
        for (card, from) in &other.cards {
            let into = self.cards.entry(card.clone()).or_default();
            into.installed += from.installed;
            into.rezzed += from.rezzed;
            into.played += from.played;
            into.activated += from.activated;
            into.accessed += from.accessed;
            into.stolen += from.stolen;
            into.scored += from.scored;
            into.trashed += from.trashed;
            into.subroutines_fired += from.subroutines_fired;
            into.subroutines_broken += from.subroutines_broken;
            into.click_broken += from.click_broken;
        }
        for (server, from) in &other.runs {
            let into = self.runs.entry(server.clone()).or_default();
            into.initiated += from.initiated;
            into.approached_server += from.approached_server;
            into.succeeded += from.succeeded;
            into.jacked_out += from.jacked_out;
            into.ended_by_effect += from.ended_by_effect;
            into.completed += from.completed;
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Coverage holds only maps of strings and integers")
    }

    /// A human-readable report. Every `PlayerAction` variant is listed —
    /// zero-count rows first, because an action nobody ever took is the
    /// finding — followed by end reasons, runs, and the cards `universe`
    /// names (again unseen first).
    pub fn render_table(&self, universe: &[CardId]) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "games: {}  steps: {}", self.games, self.steps);

        let _ = writeln!(out, "\nend reasons:");
        for (reason, count) in &self.end_reasons {
            let _ = writeln!(out, "  {count:>7}  {reason}");
        }

        let _ = writeln!(out, "\nactions (never applied first):");
        let mut rows: Vec<(u64, &str)> =
            PlayerAction::VARIANT_NAMES.iter().map(|name| (self.actions.get(*name).copied().unwrap_or(0), *name)).collect();
        rows.sort();
        for (count, name) in rows {
            let flag = if count == 0 { "  NEVER" } else { "" };
            let _ = writeln!(out, "  {count:>7}  {name}{flag}");
        }

        let _ = writeln!(out, "\nruns:");
        for (server, runs) in &self.runs {
            let _ = writeln!(
                out,
                "  {server:<8} initiated {:>6}  approached {:>6}  succeeded {:>6}  jacked out {:>6}  ended by effect {:>6}  completed {:>6}",
                runs.initiated, runs.approached_server, runs.succeeded, runs.jacked_out, runs.ended_by_effect, runs.completed
            );
        }

        let _ = writeln!(out, "\ncards (unseen first):");
        let mut cards: Vec<(bool, &str)> =
            universe.iter().map(|id| (self.cards.get(&id.0).is_some_and(CardCoverage::seen), id.0.as_str())).collect();
        cards.sort();
        for (seen, id) in cards {
            if !seen {
                let _ = writeln!(out, "  NEVER    {id}");
                continue;
            }
            let c = &self.cards[id];
            let _ = writeln!(
                out,
                "  {:<32} inst {:>4} rez {:>4} play {:>4} act {:>4} acc {:>4} steal {:>4} score {:>4} trash {:>4} subs fired {:>4} broken {:>4}",
                id,
                c.installed,
                c.rezzed,
                c.played,
                c.activated,
                c.accessed,
                c.stolen,
                c.scored,
                c.trashed,
                c.subroutines_fired,
                c.subroutines_broken
            );
        }

        let _ = writeln!(out, "\nevents:");
        for (event, count) in &self.events {
            let _ = writeln!(out, "  {count:>7}  {event}");
        }
        let _ = writeln!(out, "\neffects seen:");
        for (effect, count) in &self.effects_seen {
            let _ = writeln!(out, "  {count:>7}  {effect}");
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Gates
// ---------------------------------------------------------------------------

/// `PlayerAction` variants no game on the sample decks can reach, each with
/// the reason. The gate accepts a zero count for these and nothing else.
///
/// Add an entry only with a reason a reader can check against the card
/// pool. The bar is the one `SG_UNIMPLEMENTED` sets for card coverage: an
/// exclusion is a claim, not a silence.
pub const ACTIONS_UNREACHABLE_WITH_SAMPLE_DECKS: &[(&str, &str)] = &[
    ("SubmitCorpTraceBid", "no System Gateway card carries a Trace, so no trace is ever initiated"),
    ("SubmitRunnerTraceBid", "no System Gateway card carries a Trace, so no trace is ever initiated"),
    ("PayAccessTrigger", "Snare! is the only interactive-on-access card and is in no sample deck"),
    ("DeclineAccessTrigger", "Snare! is the only interactive-on-access card and is in no sample deck"),
];

/// `PlayerAction` variants that real play on the sample decks reaches, but
/// rarely: `(name, why it is rare, games before the gate demands it)`.
///
/// Measured on random-vs-random play, the seating that reaches the most:
/// `TrashResource` needs a tagged Runner with an installed resource on the
/// Corp's turn (4 per 192 games); `BreakSubroutineWithClick` needs an
/// encounter with a rezzed bioroid (10 per 192); `RemoveTag` needs a tag
/// the Runner survives to their own turn with 2 credits (27 per 192). Only
/// a third of a sweep's games are random-vs-random, so the thresholds are
/// several times the expected gap. Below the threshold a zero is accepted;
/// at `NETRUNNER_SWEEP_SEEDS=256` every one is demanded — which is why
/// AGENTS.md's Testing Rule runs the sweeps deep before merging engine work.
pub const ACTIONS_RARE_WITH_SAMPLE_DECKS: &[(&str, &str, u64)] = &[
    ("TrashResource", "needs a tagged Runner with a resource installed, on the Corp's turn", 512),
    // Measured 10 in one 192-game random sample and 0 in another of 96: a
    // random Corp seldom holds the 6[c] a bioroid rezzes for, and while the
    // free `BreakSubroutine` exists (Rules Audit T1) it dilutes the random
    // Runner's choice N-to-1 at every encounter. Expect this to drop back
    // to the hundreds once T1 is deleted; re-measure then.
    ("BreakSubroutineWithClick", "needs an encounter with a rezzed Ansel 1.0 or Brân 1.0", 1024),
    ("RemoveTag", "needs a tag the Runner still has on their own turn, with 2 credits", 128),
];

/// Cards in the sample decks that the sweep is permitted never to see in
/// play, each with the reason. Empty: every sample-deck card is expected
/// to be drawn and used somewhere across the seeds.
pub const UNREACHED_IN_SAMPLE_PLAY: &[(&str, &str)] = &[];

/// Events whose absence across a whole sweep means a mechanic is dead, not
/// merely rare. Curated rather than all of `GameEvent`: many variants are
/// legitimately card-specific.
pub const LOAD_BEARING_EVENTS: &[&str] = &[
    "SubroutineFired",
    "SubroutineBroken",
    "IceRezzed",
    "IceEncountered",
    "ServerApproached",
    "RunSucceeded",
    "RunEndedByEffect",
    "RunJackedOut",
    "AgendaStolen",
    "AgendaScored",
    "CardTrashedFromAccess",
    "ProgramInstalled",
    "HardwareInstalled",
    "ResourceInstalled",
    "DamageTaken",
    "TagsGiven",
    "CardsSelected",
    "PendingChoiceResolved",
    "VirusCountersPurged",
];

/// Every non-identity card the sample decks (`decks::matchups()`) contain,
/// deduplicated and sorted — the universe the card gate checks.
pub fn sample_pool_card_ids(registry: &CardRegistry) -> Vec<CardId> {
    let mut ids: Vec<CardId> = netrunner_core::decks::matchups()
        .into_iter()
        .flat_map(|(corp, runner)| corp.cards.into_iter().chain(runner.cards))
        .map(|entry| entry.card)
        .filter(|id| registry.get(id).is_none_or(|def| def.card_type != CardType::Identity))
        .collect();
    ids.sort_by(|a, b| a.0.cmp(&b.0));
    ids.dedup();
    ids
}

impl Coverage {
    /// Every gate at once: the failures, empty when all pass. Both sweeps
    /// call this so a failure reads identically whichever agent shape found
    /// it. Each line names what was never reached, so the fix — engine bug,
    /// bot blindness, or a reasoned allowlist entry — can start from the
    /// message.
    ///
    /// Sweeps are seeded, so a failure is deterministic rather than flaky.
    /// If an engine change reshuffles RNG paths and a rare card drops to
    /// zero at the default seed count, rerun at `NETRUNNER_SWEEP_SEEDS=256`:
    /// seen there means raise the default, not silence the gate.
    pub fn gate_failures(&self, card_universe: &[CardId]) -> Vec<String> {
        let mut failures = Vec::new();

        for name in PlayerAction::VARIANT_NAMES {
            let unreachable = ACTIONS_UNREACHABLE_WITH_SAMPLE_DECKS.iter().any(|(n, _)| n == name);
            let too_rare_to_demand = ACTIONS_RARE_WITH_SAMPLE_DECKS
                .iter()
                .any(|(n, _, min_games)| n == name && self.games < *min_games);
            if !unreachable && !too_rare_to_demand && self.actions.get(*name).copied().unwrap_or(0) == 0 {
                failures.push(format!("PlayerAction::{name} was never applied in {} games", self.games));
            }
        }
        for (name, _) in ACTIONS_UNREACHABLE_WITH_SAMPLE_DECKS {
            if self.actions.get(*name).copied().unwrap_or(0) > 0 {
                failures.push(format!(
                    "PlayerAction::{name} is listed as unreachable but was applied — remove it from \
                     ACTIONS_UNREACHABLE_WITH_SAMPLE_DECKS"
                ));
            }
        }

        for card in card_universe {
            let allowed = UNREACHED_IN_SAMPLE_PLAY.iter().any(|(id, _)| *id == card.0);
            if !allowed && !self.cards.get(&card.0).is_some_and(CardCoverage::seen) {
                failures.push(format!("card {} was never installed, played, rezzed, accessed or trashed", card.0));
            }
        }

        for event in LOAD_BEARING_EVENTS {
            if self.events.get(*event).copied().unwrap_or(0) == 0 {
                failures.push(format!("GameEvent::{event} was never emitted"));
            }
        }

        failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::cards::register_playable_cards;
    use netrunner_core::dsl::{CardDefinition, DamageType, TriggeredEffect};
    use netrunner_core::rules::{InstallId, Side};

    use crate::GameEndReason;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        registry
    }

    fn entry(side: Side, action: PlayerAction, events: Vec<GameEvent>) -> HistoryEntry {
        HistoryEntry { turn_number: 1, side, action, events }
    }

    #[test]
    fn counts_actions_events_cards_and_runs_from_one_entry() {
        let mut coverage = Coverage::default();
        let ice = CardId("palisade".to_string());
        coverage.absorb_entry(
            &entry(
                Side::Runner,
                PlayerAction::InitiateRun { server: ServerId::Remote(2) },
                vec![
                    GameEvent::ClickSpent { side: Side::Runner },
                    GameEvent::RunInitiated { server: ServerId::Remote(2) },
                    GameEvent::SubroutineFired { card_id: ice.clone(), index: 0, effect: Effect::EndTheRun },
                ],
            ),
            &registry(),
        );

        assert_eq!(coverage.steps, 1);
        assert_eq!(coverage.actions["InitiateRun"], 1);
        assert_eq!(coverage.actions_by_side["Runner/InitiateRun"], 1);
        assert_eq!(coverage.events["ClickSpent"], 1);
        assert_eq!(coverage.events["SubroutineFired"], 1);
        assert_eq!(coverage.runs["Remote"].initiated, 1, "remotes fold into one key");
        assert_eq!(coverage.cards["palisade"].subroutines_fired, 1);
        assert_eq!(coverage.effects_seen["EndTheRun"], 1);
    }

    #[test]
    fn a_click_break_is_counted_separately_from_a_breaker_break() {
        let mut coverage = Coverage::default();
        let ice = CardId("bran_1_0".to_string());
        let broken = |i| GameEvent::SubroutineBroken { card_id: ice.clone(), index: i };
        coverage.absorb_entry(
            &entry(
                Side::Runner,
                PlayerAction::BreakSubroutineWithClick { ice_id: ice.clone(), subroutine_index: 0 },
                vec![broken(0)],
            ),
            &registry(),
        );
        coverage.absorb_entry(
            &entry(
                Side::Runner,
                PlayerAction::ActivateAbility { target: InstallId::PLACEHOLDER, ability_index: 0 },
                vec![broken(1)],
            ),
            &registry(),
        );
        let card = &coverage.cards["bran_1_0"];
        assert_eq!((card.subroutines_broken, card.click_broken), (2, 1));
    }

    #[test]
    fn an_activated_ability_walks_its_declared_effect_tree() {
        let mut coverage = Coverage::default();
        // Cleaver's ability 0 is `BreakSubroutines`; the registry is what
        // the walk reads, not the event.
        coverage.absorb_entry(
            &entry(
                Side::Runner,
                PlayerAction::ActivateAbility { target: InstallId::PLACEHOLDER, ability_index: 0 },
                vec![GameEvent::AbilityActivated {
                    side: Side::Runner,
                    card_id: CardId("cleaver".to_string()),
                    ability_index: 0,
                }],
            ),
            &registry(),
        );
        assert_eq!(coverage.cards["cleaver"].activated, 1);
        assert_eq!(coverage.effects_seen["BreakSubroutines"], 1);
    }

    #[test]
    fn a_trigger_is_eligible_only_when_the_card_declares_it() {
        let mut registry = CardRegistry::new();
        registry.insert(CardDefinition {
            id: CardId("reactive".to_string()),
            title: "Reactive".to_string(),
            side: Side::Corp,
            card_type: CardType::Operation,
            triggers: vec![TriggeredEffect {
                trigger: Trigger::OnPlay,
                effects: vec![Effect::GainCredits(Side::Corp, 1)],
                requirement: None,
            }],
            ..CardDefinition::default()
        });
        registry.insert(CardDefinition {
            id: CardId("inert".to_string()),
            title: "Inert".to_string(),
            side: Side::Corp,
            card_type: CardType::Operation,
            ..CardDefinition::default()
        });

        let mut coverage = Coverage::default();
        for id in ["reactive", "inert"] {
            coverage.absorb_entry(
                &entry(
                    Side::Corp,
                    PlayerAction::PlayOperation { card_id: CardId(id.to_string()) },
                    vec![GameEvent::OperationPlayed { side: Side::Corp, card: CardId(id.to_string()) }],
                ),
                &registry,
            );
        }
        assert_eq!(coverage.triggers_eligible.get("reactive/OnPlay"), Some(&1));
        assert!(!coverage.triggers_eligible.contains_key("inert/OnPlay"));
        assert_eq!(coverage.cards["inert"].played, 1, "the card is still seen");
    }

    #[test]
    fn end_reasons_distinguish_wins_from_each_stall_kind() {
        let mut coverage = Coverage::default();
        let history = MatchHistory::new();
        let registry = registry();
        coverage.absorb_match(
            &history,
            &registry,
            &SessionStep::Ended { winner: Side::Corp, reason: GameEndReason::AgendaThreshold },
        );
        coverage.absorb_match(&history, &registry, &SessionStep::Stalled(StallReason::BudgetExhausted));
        coverage.absorb_match(
            &history,
            &registry,
            &SessionStep::Stalled(StallReason::NoLegalActions { side: Side::Runner }),
        );
        assert_eq!(coverage.games, 3);
        assert_eq!(coverage.end_reasons["Ended/Corp/AgendaThreshold"], 1);
        assert_eq!(coverage.end_reasons["Stalled/BudgetExhausted"], 1);
        assert_eq!(coverage.end_reasons["Stalled/NoLegalActions/Runner"], 1);
    }

    #[test]
    fn merge_adds_every_counter() {
        let registry = registry();
        let mut a = Coverage::default();
        let mut b = Coverage::default();
        let event = |side| entry(side, PlayerAction::EndTurn, vec![GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 1 }]);
        a.absorb_entry(&event(Side::Corp), &registry);
        b.absorb_entry(&event(Side::Runner), &registry);
        b.absorb_entry(&event(Side::Runner), &registry);
        a.merge(&b);
        assert_eq!(a.steps, 3);
        assert_eq!(a.actions["EndTurn"], 3);
        assert_eq!(a.actions_by_side["Runner/EndTurn"], 2);
        assert_eq!(a.events["DamageTaken"], 3);
    }

    #[test]
    fn json_round_trips_and_is_sorted() {
        let mut coverage = Coverage::default();
        coverage.absorb_entry(&entry(Side::Corp, PlayerAction::EndTurn, vec![]), &registry());
        coverage.absorb_entry(&entry(Side::Corp, PlayerAction::DrawCardClick, vec![]), &registry());
        let json = coverage.to_json();
        let back: Coverage = serde_json::from_str(&json).expect("round trip");
        assert_eq!(back, coverage);
        assert!(json.find("\"DrawCardClick\"").unwrap() < json.find("\"EndTurn\"").unwrap(), "BTreeMap keys sort");
    }

    /// The gate's whole purpose: an action class that is never applied is
    /// named, an allowlisted one is not, and an allowlisted one that *is*
    /// applied is called out as a stale exclusion.
    #[test]
    fn gate_names_unreached_actions_and_stale_exclusions() {
        let registry = registry();
        let mut coverage = Coverage::default();
        for name in PlayerAction::VARIANT_NAMES {
            if *name != "InstallProgram" {
                bump(&mut coverage.actions, *name);
            }
        }
        for event in LOAD_BEARING_EVENTS {
            bump(&mut coverage.events, *event);
        }
        coverage.games = 10_000;
        let failures = coverage.gate_failures(&[]);
        assert!(failures.iter().any(|f| f.contains("PlayerAction::InstallProgram was never applied")), "{failures:?}");
        assert!(
            failures.iter().any(|f| f.contains("SubmitCorpTraceBid is listed as unreachable but was applied")),
            "{failures:?}"
        );
        assert!(!failures.iter().any(|f| f.contains("PayAccessTrigger was never")), "{failures:?}");
        let _ = registry;
    }

    /// A rare action is demanded only once the batch is large enough for
    /// its absence to mean something.
    #[test]
    fn gate_demands_rare_actions_only_above_their_game_threshold() {
        let mut coverage = Coverage::default();
        for name in PlayerAction::VARIANT_NAMES {
            if *name != "TrashResource" {
                bump(&mut coverage.actions, *name);
            }
        }
        for event in LOAD_BEARING_EVENTS {
            bump(&mut coverage.events, *event);
        }
        coverage.games = 100;
        assert!(coverage.gate_failures(&[]).iter().all(|f| !f.contains("TrashResource")), "not demanded at 100 games");
        coverage.games = 10_000;
        assert!(coverage.gate_failures(&[]).iter().any(|f| f.contains("TrashResource was never applied")));
    }

    #[test]
    fn gate_names_unseen_cards_and_missing_events() {
        let coverage = Coverage::default();
        let universe = vec![CardId("cleaver".to_string())];
        let failures = coverage.gate_failures(&universe);
        assert!(failures.iter().any(|f| f.contains("card cleaver was never")), "{failures:?}");
        assert!(failures.iter().any(|f| f == "GameEvent::AgendaStolen was never emitted"), "{failures:?}");
    }

    #[test]
    fn the_sample_pool_excludes_identities_and_has_no_duplicates() {
        let registry = registry();
        let ids = sample_pool_card_ids(&registry);
        assert!(ids.len() > 50, "seven sample decks should span dozens of cards, got {}", ids.len());
        let mut sorted = ids.clone();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "no duplicate ids");
        for id in &ids {
            let def = registry.get(id).unwrap_or_else(|| panic!("{} is in a sample deck but not the registry", id.0));
            assert_ne!(def.card_type, CardType::Identity, "{}", id.0);
        }
    }
}
