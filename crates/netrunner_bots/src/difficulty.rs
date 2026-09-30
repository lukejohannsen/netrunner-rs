//! A calibrated ladder of opponents, so a player can raise the challenge
//! by one notch instead of assembling a bot out of flags.
//!
//! Every bot in this crate is built for *strength*, and everything that
//! measures one (`bench`, the roadmap's chair figures) asks how strong it
//! is. Neither answers the question a person actually has, which is
//! "give me something I can nearly beat." That needs three things the
//! rest of the crate does not provide: an **order**, a **name** a player
//! can ask for, and a guarantee the order is real.
//!
//! **A rung is the strongest bot handicapped, not a weaker bot strained,
//! and that is a measurement.** The first cut of this table dialled the
//! search budget down for its lower rungs, and calibrating it showed
//! that does not work: rungs 2-4 scored 0.562 / 0.500 / 0.458 as the
//! Corp against the same fixed opponent — flat, and sloping the wrong
//! way (Phase 5 §1) — because search converts differently on the two
//! chairs and overtook one ply on neither until far past the budget a
//! decision can wait for. So a rung is `(base bot, epsilon)` over
//! `HandicapAgent`, monotone by construction rather than by hope: the
//! same base bot with more `epsilon` is strictly worse, and only *how
//! much* worse has to be measured.
//!
//! **Both chairs are one base bot at five handicaps, and the base is the
//! turn planner** (`PlanningAgent`, Phase 5 §25 Stage 8). The base has
//! moved three times, each time because a measurement found the stronger
//! bot: `puct@512` at the Corp's top until every Corp profile built a
//! fort and one ply outscored it (§24, the fort pays off past a search's
//! horizon); `mcts@128` at the Runner's top until the evaluator priced
//! what a run finds and one ply outscored that too (Phase 2 §5a); and
//! the one-ply chooser on both chairs until the planner beat it on both
//! — the Corp by 0.12 and 0.07 of win share over two seeds, the Runner
//! by 0.07 and 0.11 (§25 Stages 5–7) — at which point the reference was
//! deleted and the ladder re-taken on the planner. `LevelKind`'s search
//! variants stay, because the day a search beats the planner on either
//! chair its top rungs go back to it, and the tests below say what that
//! would owe.
//!
//! **Per chair, because the game is not symmetric and neither is the
//! bot.** `netrunner_rating` rates Corp and Runner separately — "one
//! number would average two different skills" — and the `epsilon`
//! curves differ: a Runner's win rate falls about linearly in the share
//! of decisions thrown away, a Corp's falls steeply near zero (one
//! decision in twenty costs it a quarter of its margin) and flattens
//! toward random. So the same rung is a different handicap on each side,
//! read off each chair's measured curve (`Level::spec`; the calibration
//! tables are in `docs/roadmap/phase-5-difficulty-ladder.md` §2).
//!
//! **A style is not the difficulty dial, and deliberately so.** The
//! seven `Plan`s a `Style` stacks (`plans`) are a *style* axis, and each
//! is written for one chair — a Runner plan seated as the Corp "touches
//! only the shared terms, which is harmless and useless" (`plans`' own
//! doc comment). A ladder built on them would be asymmetric between the
//! chairs for a reason that has nothing to do with how hard the opponent
//! is, which is how the first draft of this table shipped a rung that
//! was conservative as the Runner and unchanged as the Corp. Every rung
//! here is balanced; what climbs is the handicap. "A glacier Corp at
//! level 4" is a second axis crossed with this one, not a replacement.
//! The cross keeps the order, and since §24 it keeps the spacing too:
//! every Corp style measured on one `epsilon` curve, so no style carries
//! a handicap of its own (three once did; `LevelSpec::with_style`
//! records why each went). A Runner rung with no style plays its
//! identity's faction's plan, as every planner Runner does.
//!
//! **Machine-independence is a prerequisite, not a detail.** A rung whose
//! strength moved with the host's core count would not be a rung at all;
//! `MctsAgent::DEFAULT_TREES` became a fixed number for this reason
//! (item 41), and every spec here states its tree count outright.

use netrunner_core::rules::Side;
use serde::{Deserialize, Serialize};

use crate::agent::BotAgent;
use crate::handicap::HandicapAgent;
use crate::planner::PlanningAgent;
use crate::knowledge::Knowledge;
use crate::mcts::MctsAgent;
use crate::plans::Style;
#[cfg(test)]
use crate::plans::Plan;
use crate::policy::UniformPolicyEvaluator;
use crate::puct::{PuctAgent, PuctConfig};

/// One rung. Ordered weakest to strongest, and the order is *measured* —
/// `scripts/ladder_report.py` demands a rise, not merely the absence of
/// a fall, which is the check the first calibration was missing.
///
/// Five rather than ten: each rung has to be distinguishable from its
/// neighbours by a person playing a handful of games, and the measured
/// spread between the floor and the ceiling is **0.708** of win rate on
/// the Runner chair and **0.254** on the Corp's. Ten rungs would put
/// neighbours inside the noise of a single evening's play — and the Corp
/// chair already spaces its five 0.064 apart, which is about the limit.
/// Serialized by its name (`"veteran"`), the word a player asks for, so a
/// match record names the rung the way `--corp-level` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Legal moves, chosen at random. Loses to a first-time player who
    /// has understood the rules, which is exactly what a first rung is
    /// for.
    Novice,
    /// The planner, blundering often: it plans the guide's line —
    /// install, advance, score; economy, then breakers for the ICE shown
    /// — and then throws a click away. The rung where a new player's
    /// mistakes stop being the only ones on the table.
    Apprentice,
    /// The planner, throwing away about one decision in ten as the Corp
    /// and one in three as the Runner.
    Operator,
    /// The planner with a rare blunder: a real opponent with a visible
    /// crack in it.
    Veteran,
    /// The strongest bot measured on each chair, playing every decision:
    /// the planner, in the deck's style or its faction's plan.
    Elite,
}

/// What a rung *is*, on one chair.
///
/// A record rather than a constructor call so the ladder can be printed,
/// diffed and calibrated without building an agent: `bench` needs the
/// label, the CLI needs the description, and the monotonicity test needs
/// to seat two of them against each other.
/// `Eq` is deliberately absent: `epsilon` is an `f64`, and a spec is
/// compared for "is this the rung I asked for", which `PartialEq` on the
/// exact table constants answers exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelSpec {
    pub level: Level,
    pub side: Side,
    pub kind: LevelKind,
    /// Search iterations per decision; ignored by the planner, which has
    /// a budget of its own (`planner::PLAN_BUDGET`).
    pub simulations: usize,
    /// Root-parallel trees (`mcts`) or hidden-state samples (`puct`) —
    /// one dial under two names, always stated rather than defaulted.
    pub samples: usize,
    /// Share of decisions replaced by a uniformly random legal action
    /// (`HandicapAgent`). This is what spaces the ladder; see the module
    /// docs for why it is not the search budget.
    pub epsilon: f64,
    pub style: Style,
}

/// Which search a rung uses. Deliberately not `crate::…::Agent`: a rung
/// is data, and `netrunner_cli::bots::BotKind` is a *user's* choice of
/// bot, which is a different thing that happens to overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelKind {
    /// `PlanningAgent`: the whole turn planned, one ply inside a run.
    Planner,
    Mcts,
    Puct,
}

impl Level {
    /// Every rung, weakest first.
    pub const ALL: [Level; 5] = [Level::Novice, Level::Apprentice, Level::Operator, Level::Veteran, Level::Elite];

    /// The name a player asks for, and what `bench` labels the rung.
    pub fn name(self) -> &'static str {
        match self {
            Level::Novice => "novice",
            Level::Apprentice => "apprentice",
            Level::Operator => "operator",
            Level::Veteran => "veteran",
            Level::Elite => "elite",
        }
    }

    /// The id a person's record against this rung is kept under
    /// (`netrunner_client::record`): `bot:veteran`. One id per rung rather
    /// than per style because the rung is the strength claim — a record
    /// against it should span the styles it plays — and the chair is the
    /// record's own key. It is not a rating id: nobody rates a game
    /// against a bot, and `bench` names a rung `level:veteran`.
    pub fn record_id(self) -> String {
        format!("bot:{}", self.name())
    }

    /// 1-5, for a UI that would rather show a number.
    pub fn rung(self) -> u8 {
        match self {
            Level::Novice => 1,
            Level::Apprentice => 2,
            Level::Operator => 3,
            Level::Veteran => 4,
            Level::Elite => 5,
        }
    }

    /// What this rung is on `side`.
    ///
    /// Both chairs are the planner at five handicaps (the module docs say
    /// how each got there); the handicaps differ because the chairs'
    /// `epsilon` curves do.
    pub fn spec(self, side: Side) -> LevelSpec {
        // Anchors, all against the un-handicapped planner on the other
        // chair, each seat in its deck's own style (Phase 5 §25 Stage 8,
        // 30 September 2026, 768 games a cell): random 0.018 / 0.034
        // (Corp / Runner win rate), the planner played straight 0.401 /
        // 0.599. Every rung is one of those endpoints mixed toward random
        // by `epsilon`, so the order needs no measurement and the
        // *spacing* is what Phase 5 §2 calibrates. Every earlier figure in
        // the roadmap (§2, §21, §24) was taken against the one-ply
        // reference and is not comparable to these.
        let (kind, simulations, samples, epsilon) = match (self, side) {
            // The planner at `epsilon` 1.0 rather than `LevelKind::Random`,
            // and the two are identical in play: `HandicapAgent` never
            // consults the inner agent at 1.0, so no plan is ever made.
            // Writing it this way makes the whole ladder one base family
            // with strictly falling handicap, which is what
            // `each_rung_is_the_one_below_it_with_less_handicap_or_a_
            // better_base` can actually check.
            (Level::Novice, _) => (LevelKind::Planner, 0, 1, 1.0),
            // **The Corp's handicaps are §21's, read off `glacier`'s
            // one-ply curve and kept.** That curve is steep near 0 and
            // nowhere near linear, and the planner's is the same shape:
            // against the planner Runner it scores 0.018 / 0.143 / 0.224 /
            // 0.315 / 0.401 on these five (seed 1) and 0.008 / 0.122 /
            // 0.224 / 0.328 / 0.469 (seed 2) — steps of +0.125, +0.081,
            // +0.091, +0.086 and +0.115, +0.102, +0.104, +0.141 against an
            // even 0.096 and 0.115, every one a rise on each seed — so the
            // table stands. §24 had found the one-ply curve the same in
            // every Corp style (ε 0.22 / 0.11 / 0.05: `Balanced` 0.128 /
            // 0.225 / 0.331, `trap` 0.109 / 0.221 / 0.320, `rush` 0.139 /
            // 0.246 / 0.358, `glacier` 0.121 / — / 0.311), so one table
            // serves every style and no style carries a handicap of its
            // own.
            (Level::Apprentice, Side::Corp) => (LevelKind::Planner, 0, 1, 0.22),
            (Level::Operator, Side::Corp) => (LevelKind::Planner, 0, 1, 0.11),
            (Level::Veteran, Side::Corp) => (LevelKind::Planner, 0, 1, 0.05),
            (Level::Elite, Side::Corp) => (LevelKind::Planner, 0, 1, 0.0),
            // **The Runner's handicaps were re-spaced for the planner.**
            // One ply's Runner curve was a line (w ≈ 0.833 − 0.70ε, §2),
            // so its rungs sat at even `epsilon` — 0.75 / 0.50 / 0.25 —
            // and the planner's is not: on that table it scored 0.034 /
            // 0.063 / 0.141 / 0.310 / 0.599 (seed 1) and 0.039 / 0.089 /
            // 0.180 / 0.328 / 0.531 (seed 2) against the planner Corp,
            // steps of +0.029, +0.078, +0.169, +0.289 and +0.049, +0.091,
            // +0.148, +0.203 — crammed at the bottom, the first of them
            // flat. A plan a random action breaks is planned again from
            // the board it left, so a blunder costs the planner more than
            // it cost a chooser that never looked past one action, and the
            // curve is steep near 0 like the Corp's. Interpolating the
            // measured curve for four even steps gives 0.45 / 0.25 / 0.10;
            // re-measured on the same seeds, 0.034 / 0.188 / 0.344 / 0.435
            // / 0.599 and 0.039 / 0.188 / 0.320 / 0.424 / 0.531 — steps of
            // +0.154, +0.156, +0.091, +0.164 and +0.148, +0.133, +0.104,
            // +0.107, every one a rise on each seed at 2.6 sd or more, all
            // within 0.05 of even — with exactly the ten cells whose Runner
            // is `novice` or `elite` byte-identical between the two runs.
            (Level::Apprentice, Side::Runner) => (LevelKind::Planner, 0, 1, 0.45),
            (Level::Operator, Side::Runner) => (LevelKind::Planner, 0, 1, 0.25),
            (Level::Veteran, Side::Runner) => (LevelKind::Planner, 0, 1, 0.10),
            (Level::Elite, Side::Runner) => (LevelKind::Planner, 0, 1, 0.0),
        };
        LevelSpec { level: self, side, kind, simulations, samples, epsilon, style: Style::BALANCED }
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::str::FromStr for Level {
    type Err = String;

    /// Accepts the name or the rung number, so `--corp-level 3` and
    /// `--corp-level operator` are the same request.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lowered = s.trim().to_ascii_lowercase();
        Level::ALL
            .into_iter()
            .find(|level| level.name() == lowered || level.rung().to_string() == lowered)
            .ok_or_else(|| {
                let names = Level::ALL.map(|level| format!("{} ({})", level.name(), level.rung())).join(", ");
                format!("unknown level {s:?}; expected one of {names}")
            })
    }
}

impl LevelSpec {
    /// The same rung, played in `style`.
    ///
    /// Difficulty and style are two axes and this is where they cross.
    /// `Level::spec` always hands back balanced because a rung is named
    /// before a deck's style is resolved. A profile is a *bias* on the same
    /// evaluator — it ranks the same legal moves differently, never more or
    /// less deeply — so the rung's strength order survives it (a
    /// handicapped bot with a style is still the same bot with less
    /// handicap at the rung above). Before this existed,
    /// `--corp-level 4 --corp-style fast-advance` played balanced and said
    /// nothing about it.
    ///
    /// **It changes the style and nothing else, on either chair** (Phase 5
    /// §24). It used to carry three exceptions on the Corp's, each a style
    /// whose ladder a measurement had broken: `trap`'s own `veteran`
    /// handicap (§14), `rush`'s top two rungs played as `Balanced` because
    /// search bought `rush` nothing (§15), and `glacier` as one ply at its
    /// own five handicaps because search bought it less than nothing
    /// (§21). Once every Corp profile built §19's fort (§23), all four
    /// styles lay on `glacier`'s curve and every search rung ran below
    /// `operator`, so the Corp table became `glacier`'s and the exceptions
    /// had nothing left to except.
    pub fn with_style(self, style: Style) -> Self {
        Self { style, ..self }
    }

    /// The agent this rung seats, sampling from what `knowledge` admits —
    /// the format's pool and the seat's own deck, which the driver that
    /// seats a rung always has.
    ///
    /// Built here rather than in the CLI because a rung is a *bot policy*
    /// decision and those live in this crate — the CLI's `make_agent`
    /// maps a user's explicit bot choice, which is the other direction.
    pub fn agent(self, seed: u64, knowledge: Knowledge) -> Box<dyn BotAgent> {
        let inner = self.base_agent(seed, knowledge);
        if self.epsilon == 0.0 {
            return inner;
        }
        // Seeded off the rung's own seed, so two rungs sharing a base bot
        // do not share a blunder sequence.
        Box::new(HandicapAgent::new(inner, self.epsilon, seed ^ 0x5AD0_D1FF))
    }

    fn base_agent(self, seed: u64, knowledge: Knowledge) -> Box<dyn BotAgent> {
        match self.kind {
            LevelKind::Planner => {
                Box::new(PlanningAgent::with_style(self.side, seed, self.style).with_knowledge(knowledge))
            }
            LevelKind::Mcts => Box::new(
                MctsAgent::with_trees(self.side, seed, self.simulations, self.samples)
                    .with_style(self.style)
                    .with_knowledge(knowledge),
            ),
            LevelKind::Puct => Box::new(
                PuctAgent::with_config(
                    self.side,
                    seed,
                    UniformPolicyEvaluator::with_style(self.side, self.style),
                    PuctConfig { iterations: self.simulations, samples: self.samples, ..PuctConfig::default() },
                )
                .with_knowledge(knowledge),
            ),
        }
    }

    /// How this rung is named in a report or a ladder — `elite/corp`
    /// rather than `puct@512`, because the point of the ladder is that a
    /// player never has to know the second form.
    pub fn label(self) -> String {
        format!("{}/{}", self.level.name(), if self.side == Side::Corp { "corp" } else { "runner" })
    }

    /// One line of "what am I about to play", for a client that offers
    /// the choice: how the rung decides, the plans it plays when the seat
    /// has a style — a balanced Runner rung plays its identity's faction's
    /// plan, which no spec knows before the deck is dealt, so it names
    /// none — and how often it blunders.
    pub fn describe(self) -> String {
        // At full handicap the base bot is never asked, so describing it
        // would be a lie about what the player is facing.
        if self.epsilon >= 1.0 {
            return "plays legal moves at random".to_string();
        }
        let plans = self.style.plans().map(|plan| plan.name()).collect::<Vec<_>>().join(" then ");
        let play = match self.kind {
            LevelKind::Planner if plans.is_empty() => "plans its whole turn".to_string(),
            LevelKind::Planner => format!("plans its whole turn as {plans}"),
            LevelKind::Mcts => {
                format!("searches {} playouts over {} readings of your hidden cards", self.simulations, self.samples)
            }
            LevelKind::Puct => format!("searches {} positions per decision", self.simulations),
        };
        if self.epsilon == 0.0 {
            return play;
        }
        // Out of ten would round a rare blunder down to "about 0".
        if self.epsilon < 0.1 {
            return format!("{play}, and throws away about one decision in {}", (1.0 / self.epsilon).round());
        }
        format!("{play}, and throws away about {} decisions in 10", (self.epsilon * 10.0).round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_is_named_by_word_or_by_rung() {
        assert_eq!("operator".parse::<Level>().unwrap(), Level::Operator);
        assert_eq!("3".parse::<Level>().unwrap(), Level::Operator);
        assert_eq!(" ELITE ".parse::<Level>().unwrap(), Level::Elite);
        let error = "brutal".parse::<Level>().unwrap_err();
        assert!(error.contains("novice (1)") && error.contains("elite (5)"), "{error}");
    }

    #[test]
    fn the_rungs_are_ordered_and_every_one_seats_on_both_chairs() {
        assert_eq!(Level::ALL.map(Level::rung), [1, 2, 3, 4, 5]);
        assert_eq!(Level::Veteran.record_id(), "bot:veteran");
        assert!(Level::Novice < Level::Elite, "the enum's own order is the ladder's order");
        for level in Level::ALL {
            for side in [Side::Corp, Side::Runner] {
                let spec = level.spec(side);
                assert_eq!(spec.level, level);
                let _: Box<dyn BotAgent> = spec.agent(7, Knowledge::default());
                assert!(spec.label().starts_with(level.name()));
                assert!(!spec.describe().is_empty());
            }
        }
    }

    /// The style axis crosses the difficulty axis without touching its
    /// base: a rung with a style is the same rung — kind, budget,
    /// and handicap unless the style has a measured one of its own — with
    /// a different evaluator bias, and a balanced request is exactly
    /// `Level::spec`.
    #[test]
    fn a_style_changes_the_evaluator_and_nothing_else_about_a_rung() {
        for side in [Side::Corp, Side::Runner] {
            for level in Level::ALL {
                let plain = level.spec(side);
                assert_eq!(plain.style, Style::BALANCED, "the calibrated rung is balanced");
                assert_eq!(plain.with_style(Style::BALANCED), plain);
                let styled = plain.with_style(Style::of(Plan::Pressure));
                assert_eq!(styled.style, Style::of(Plan::Pressure));
                assert_eq!(
                    (styled.level, styled.side, styled.kind, styled.simulations, styled.samples, styled.epsilon),
                    (plain.level, plain.side, plain.kind, plain.simulations, plain.samples, plain.epsilon)
                );
                let _: Box<dyn BotAgent> = styled.agent(3, Knowledge::default());
            }
        }
    }

    /// Every Corp style climbs the same one-ply ladder (Phase 5 §24):
    /// the style is the only thing a plan changes, and the handicaps
    /// are the table §21 read off `glacier`'s curve.
    #[test]
    #[allow(clippy::float_cmp)]
    fn every_corp_style_is_one_ply_at_the_same_five_handicaps() {
        for plan in Plan::ALL {
            let style = Style::of(plan);
            let table = Level::ALL.map(|level| {
                let spec = level.spec(Side::Corp).with_style(style);
                assert_eq!(
                    (spec.kind, spec.simulations, spec.samples, spec.style),
                    (LevelKind::Planner, 0, 1, style),
                    "{level} {plan:?}"
                );
                assert_eq!(spec.with_style(Style::BALANCED), level.spec(Side::Corp), "{level}");
                spec.epsilon
            });
            assert_eq!(table, [1.0, 0.22, 0.11, 0.05, 0.0], "{plan:?}");
        }
        let veteran = Level::Veteran.spec(Side::Corp).with_style(Style::of(Plan::Traps));
        assert_eq!(veteran.describe(), "plans its whole turn as traps, and throws away about one decision in 20");
        assert_eq!(Level::Veteran.spec(Side::Corp).describe(), "plans its whole turn, and throws away about one decision in 20");
        let stacked = Level::Elite.spec(Side::Corp).with_style(Style::new(&[Plan::Glacier, Plan::FastAdvance]).unwrap());
        assert_eq!(stacked.describe(), "plans its whole turn as glacier then fast-advance");
    }

    /// Neither chair's `elite` searches: on each, one ply played straight
    /// is the strongest bot measured (see the module docs), and the rung
    /// below it is the same bot with a handicap.
    #[test]
    #[allow(clippy::float_cmp)]
    fn the_top_rung_is_one_ply_played_straight_on_both_chairs() {
        for side in [Side::Corp, Side::Runner] {
            let elite = Level::Elite.spec(side);
            assert_eq!((elite.kind, elite.simulations, elite.samples, elite.epsilon), (LevelKind::Planner, 0, 1, 0.0), "{side:?}");
            assert!(Level::Veteran.spec(side).epsilon > 0.0, "{side:?}");
        }
    }

    /// The property the first cut of this table did not have. Every rung
    /// is either a strictly better base bot than the one below it, or the
    /// same base bot with strictly less handicap — so "higher is
    /// stronger" holds structurally, and calibration only has to say by
    /// how much. Without this, spacing the ladder is guesswork checked
    /// after the fact by 1,200 games.
    #[test]
    fn each_rung_is_the_one_below_it_with_less_handicap_or_a_better_base() {
        // In every style, because a style may carry its own handicap.
        for (side, style) in [Side::Corp, Side::Runner].into_iter().flat_map(|side| Plan::ALL.map(|p| (side, Style::of(p)))) {
            for pair in Level::ALL.windows(2) {
                let (lower, upper) = (pair[0].spec(side).with_style(style), pair[1].spec(side).with_style(style));
                let same_base = (lower.kind, lower.simulations, lower.samples) == (upper.kind, upper.simulations, upper.samples);
                if same_base {
                    assert!(upper.epsilon < lower.epsilon, "{side:?}: {} and {} share a base but not less handicap", lower.label(), upper.label());
                } else {
                    // A different base has to be one the roadmap measured
                    // as stronger *on this chair*; the epsilon may go
                    // either way, which is why the ladder is calibrated
                    // and not merely asserted.
                    assert!(
                        upper.epsilon <= lower.epsilon || upper.simulations > lower.simulations,
                        "{side:?}: {} is a new base with more handicap and no more search",
                        upper.label()
                    );
                }
            }
        }
    }

    /// The cheap rungs must stay cheap: a player at rung 2 should not
    /// wait as long as one at rung 5. Handicapping the deep search at the
    /// bottom of the ladder is the mistake this pins against.
    #[test]
    fn the_lower_rungs_do_not_run_the_deep_search() {
        for side in [Side::Corp, Side::Runner] {
            for level in [Level::Novice, Level::Apprentice, Level::Operator] {
                assert_eq!(level.spec(side).simulations, 0, "{side:?} {level} should not be searching at all");
            }
        }
    }

    /// Search budget must not shrink as the ladder climbs. Strength is
    /// measured, not asserted (see the calibration in ROADMAP Phase 5),
    /// but a rung that searches *less* than the one below it would be a
    /// typo, and this catches that.
    #[test]
    #[allow(clippy::float_cmp)]
    fn search_budget_never_falls_as_the_ladder_climbs() {
        for side in [Side::Corp, Side::Runner] {
            let searched: Vec<usize> = Level::ALL
                .into_iter()
                .map(|level| level.spec(side))
                .filter(|spec| matches!(spec.kind, LevelKind::Mcts | LevelKind::Puct))
                .map(|spec| spec.simulations)
                .collect();
            assert!(searched.windows(2).all(|pair| pair[0] <= pair[1]), "{side:?}: {searched:?}");
        }
    }
}
