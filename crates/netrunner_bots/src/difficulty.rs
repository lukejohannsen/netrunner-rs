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
//! **The rungs are a strong bot handicapped, not a weak bot strained,
//! and that is a measurement.** The first cut of this table dialled the
//! search budget down for its lower rungs, and calibrating it showed
//! that does not work on the Corp chair: rungs 2-4 scored **0.562 /
//! 0.500 / 0.458** against the same fixed opponent — flat, and sloping
//! the wrong way (Phase 5 §1). The cause is on the record independently:
//! `puct` as Corp scores 0.458 at 32 simulations, 0.581 at 128 and 0.714
//! at 512, while the one-ply heuristic Corp scores 0.583 (Phase 2 §5
//! item 34), so **search does not overtake one ply on that chair until
//! past 128**. The bots here are three strengths, not five. Rungs are
//! therefore `(base bot, epsilon)` pairs over `HandicapAgent`, which is
//! monotone by construction rather than by hope: the same base bot with
//! more `epsilon` is strictly worse, and only *how much* worse has to be
//! measured.
//!
//! **Per chair, because the game is not symmetric and neither are the
//! bots.** `netrunner_rating` already rates Corp and Runner separately —
//! "one number would average two different skills" — and the two chairs
//! convert search budget completely differently (ROADMAP Phase 2 §5 items
//! 34, 35): over 16 → 128 simulations the Corp chair gains **+0.208** and
//! is still climbing at 512, while the Runner chair gains +0.076 and has
//! **saturated by 64**. So the same rung is a different bot on each side,
//! and the strongest Corp (`puct` at depth) is not the strongest Runner
//! (`mcts` at four determinizations, which is where that chair's gains
//! actually come from).
//!
//! **The top Runner rung is capped, the cap is measured, and it is the
//! ladder's one real weakness.** From one ply to the best bot available
//! is **0.229** of win rate on the Corp chair and **0.104** on the
//! Runner's, so the top of the Runner ladder has half the room to hold
//! two rungs in. Nothing in this workspace makes a Runner much stronger
//! than `mcts@128`: five roadmap entries (36, 38 and 40 negatively; 34
//! and 35 positively) put that chair's ceiling in hidden-information
//! sampling, which no deeper search or better leaf has yet recovered.
//! **Item 43 then took the sampling itself as far as it goes and it is
//! not the ceiling either** — which corrects this paragraph's earlier
//! reading that more of it was the way up: 4 → 32 trees and 128 → 1,024
//! simulations all land between 0.604 and 0.622 against a fixed one-ply
//! Corp, and a deeper playout is *worse* (0.547 at 32 plies). So
//! `veteran` and `elite` are about 0.06 apart as Runners against 0.19 as
//! Corps. What is left to try is the playout policy rather than a bigger
//! search, and that is Phase 2 §5's Runner-chair work, not a spacing
//! problem this table can solve.
//!
//! **Personality is not the difficulty dial, and deliberately so.** The
//! six `Personality` profiles are a *style* axis, and each is written for
//! one chair — a Runner profile seated as the Corp "touches only the
//! shared terms, which is harmless and useless" (`personality`'s own doc
//! comment). A ladder built on them would be asymmetric between the
//! chairs for a reason that has nothing to do with how hard the opponent
//! is, which is how the first draft of this table shipped a rung that
//! was conservative as the Runner and unchanged as the Corp. Every rung
//! here is `Balanced`; what climbs is the search. "A glacier Corp at
//! level 4" is a second axis crossed with this one, not a replacement.
//!
//! **Machine-independence is a prerequisite, not a detail.** A rung whose
//! strength moved with the host's core count would not be a rung at all;
//! `MctsAgent::DEFAULT_TREES` became a fixed number for this reason
//! (item 41), and every spec here states its tree count outright.

use netrunner_core::rules::Side;

use crate::agent::BotAgent;
use crate::handicap::HandicapAgent;
use crate::heuristic::HeuristicAgent;
use crate::mcts::MctsAgent;
use crate::personality::Personality;
use crate::policy::UniformPolicyEvaluator;
use crate::puct::{PuctAgent, PuctConfig};

/// One rung. Ordered weakest to strongest, and the order is *measured* —
/// `scripts/ladder_report.py` demands a rise, not merely the absence of
/// a fall, which is the check the first calibration was missing.
///
/// Five rather than ten: each rung has to be distinguishable from its
/// neighbours by a person playing a handful of games, and the measured
/// spread between the floor and the ceiling is about 0.65 of win rate on
/// the Corp chair. Ten rungs would put neighbours inside the noise of a
/// single evening's play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Legal moves, chosen at random. Loses to a first-time player who
    /// has understood the rules, which is exactly what a first rung is
    /// for — the measured floor is 0.042 as Corp and 0.167 as Runner
    /// against one ply.
    Novice,
    /// One ply, blundering a third of the time: it takes the obvious
    /// line — advance, rez, break, run an open server — and then throws
    /// a click away. The rung where a new player's mistakes stop being
    /// the only ones on the table.
    Apprentice,
    /// One ply, played straight. No plan beyond the current turn, but no
    /// gifts either.
    Operator,
    /// The strongest bot on this chair, blundering one decision in six —
    /// a real opponent with a visible crack in it.
    Veteran,
    /// The strongest configuration measured on each chair, playing every
    /// decision. The two chairs are different bots; see the module docs.
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
    /// Search iterations per decision; ignored by the two rungs with no
    /// search.
    pub simulations: usize,
    /// Root-parallel trees (`mcts`) or hidden-state samples (`puct`) —
    /// one dial under two names, always stated rather than defaulted.
    pub samples: usize,
    /// Share of decisions replaced by a uniformly random legal action
    /// (`HandicapAgent`). This is what spaces the ladder; see the module
    /// docs for why it is not the search budget.
    pub epsilon: f64,
    pub personality: Personality,
}

/// Which search a rung uses. Deliberately not `crate::…::Agent`: a rung
/// is data, and `netrunner_cli::bots::BotKind` is a *user's* choice of
/// bot, which is a different thing that happens to overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelKind {
    Heuristic,
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

    /// The participant id this rung is rated under, on every track and
    /// in every consumer: `bot:veteran`. One id per rung rather than per
    /// chair because `netrunner_rating` already keeps a Corp and a Runner
    /// record per participant, and rather than per style because the
    /// rung is the strength claim — a human's rating against it should
    /// span the styles it plays. The daemon's `bot:heuristic` ids are the
    /// same shape, so a local file and a daemon file are comparable.
    pub fn rating_id(self) -> String {
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
    /// The two chairs diverge at the top for the reason in the module
    /// docs: depth is what the Corp chair converts, and samples are what
    /// the Runner chair converts, so `Elite` is `puct` at 512 on one side
    /// and `mcts` at four determinizations on the other.
    pub fn spec(self, side: Side) -> LevelSpec {
        // Anchors, all against a fixed one-ply opponent on the other
        // chair: random 0.042 / 0.167 (Corp / Runner win rate), one ply
        // 0.562 / 0.438, `puct@512` as Corp 0.714, `mcts@128` at four
        // determinizations as Runner 0.792. Every rung below is one of
        // those two endpoints mixed toward random by `epsilon`, so the
        // order needs no measurement and the *spacing* is what Phase 5
        // §2 calibrates.
        let (kind, simulations, samples, epsilon) = match (self, side) {
            // One ply at `epsilon` 1.0 rather than `LevelKind::Random`,
            // and the two are identical in play: `HandicapAgent` never
            // consults the inner agent at 1.0, so no heuristic ever runs.
            // Writing it this way makes the bottom three rungs one base
            // family with strictly falling handicap — 1.0, 0.35, 0.0 —
            // which is what `each_rung_is_the_one_below_it_with_less_
            // handicap_or_a_better_base` can actually check.
            (Level::Novice, _) => (LevelKind::Heuristic, 0, 1, 1.0),
            // One ply is the cheap base, and the bottom of the ladder has
            // to stay cheap: handicapping `puct@512` here would think for
            // as long as the top rung while playing badly.
            (Level::Apprentice, _) => (LevelKind::Heuristic, 0, 1, 0.35),
            (Level::Operator, _) => (LevelKind::Heuristic, 0, 1, 0.0),
            // The top two rungs share a base and differ only in epsilon,
            // which is the one step in the ladder guaranteed monotone
            // without measuring anything at all.
            //
            // **The epsilon is per chair because the span is.** From one
            // ply to the best bot is 0.229 of win rate on the Corp chair
            // and only 0.104 on the Runner's, and the handicap is steep:
            // 0.17 cost the Corp 0.187, nearly the whole span, landing
            // `veteran` on top of `operator` (Phase 5 §2). 0.10 is that
            // measurement inverted — 0.17 x (span/2) / 0.187 — to put the
            // rung mid-span at about 0.61. The Runner keeps 0.17 because
            // mid-span there is 0.55 and 0.17 measured 0.542, which is
            // already it.
            (Level::Veteran, Side::Corp) => (LevelKind::Puct, 512, 1, 0.10),
            (Level::Elite, Side::Corp) => (LevelKind::Puct, 512, 1, 0.0),
            (Level::Veteran, Side::Runner) => (LevelKind::Mcts, 128, 4, 0.17),
            (Level::Elite, Side::Runner) => (LevelKind::Mcts, 128, 4, 0.0),
        };
        LevelSpec { level: self, side, kind, simulations, samples, epsilon, personality: Personality::Balanced }
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
    /// The same rung, played in `personality`'s style.
    ///
    /// Difficulty and style are two axes and this is where they cross.
    /// `Level::spec` always hands back `Balanced` because every number in
    /// the calibration (Phase 5 §2) was taken on `Balanced`, and a rung
    /// that quietly changed its evaluator would not be the rung anyone
    /// measured. A profile is a *bias* on the same evaluator — it ranks the
    /// same legal moves differently, never more or less deeply — so the
    /// rung's strength order survives it (a handicapped bot with a style
    /// is still the same bot with less handicap at the rung above), and
    /// only the exact calibration figures are for `Balanced`. Before this
    /// existed, `--corp-level 4 --corp-personality rush` played `Balanced`
    /// and said nothing about it.
    pub fn with_personality(self, personality: Personality) -> Self {
        Self { personality, ..self }
    }

    /// The agent this rung seats.
    ///
    /// Built here rather than in the CLI because a rung is a *bot policy*
    /// decision and those live in this crate — the CLI's `make_agent`
    /// maps a user's explicit bot choice, which is the other direction.
    pub fn agent(self, seed: u64) -> Box<dyn BotAgent> {
        let inner = self.base_agent(seed);
        if self.epsilon == 0.0 {
            return inner;
        }
        // Seeded off the rung's own seed, so two rungs sharing a base bot
        // do not share a blunder sequence.
        Box::new(HandicapAgent::new(inner, self.epsilon, seed ^ 0x5AD0_D1FF))
    }

    fn base_agent(self, seed: u64) -> Box<dyn BotAgent> {
        match self.kind {
            LevelKind::Heuristic => Box::new(HeuristicAgent::with_personality(self.side, seed, self.personality)),
            LevelKind::Mcts => Box::new(
                MctsAgent::with_trees(self.side, seed, self.simulations, self.samples).with_personality(self.personality),
            ),
            LevelKind::Puct => Box::new(PuctAgent::with_config(
                self.side,
                seed,
                UniformPolicyEvaluator::with_personality(self.side, self.personality),
                PuctConfig { iterations: self.simulations, samples: self.samples, ..PuctConfig::default() },
            )),
        }
    }

    /// How this rung is named in a report or a ladder — `elite/corp`
    /// rather than `puct@512`, because the point of the ladder is that a
    /// player never has to know the second form.
    pub fn label(self) -> String {
        format!("{}/{}", self.level.name(), if self.side == Side::Corp { "corp" } else { "runner" })
    }

    /// One line of "what am I about to play", for a client that offers
    /// the choice.
    pub fn describe(self) -> String {
        // At full handicap the base bot is never asked, so describing it
        // would be a lie about what the player is facing.
        if self.epsilon >= 1.0 {
            return "plays legal moves at random".to_string();
        }
        let play = match self.kind {
            LevelKind::Heuristic => "looks one move ahead".to_string(),
            LevelKind::Mcts => {
                format!("searches {} playouts over {} readings of your hidden cards", self.simulations, self.samples)
            }
            LevelKind::Puct => format!("searches {} positions per decision", self.simulations),
        };
        if self.epsilon == 0.0 {
            return play;
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
        assert_eq!(Level::Veteran.rating_id(), "bot:veteran");
        assert!(Level::Novice < Level::Elite, "the enum's own order is the ladder's order");
        for level in Level::ALL {
            for side in [Side::Corp, Side::Runner] {
                let spec = level.spec(side);
                assert_eq!(spec.level, level);
                let _: Box<dyn BotAgent> = spec.agent(7);
                assert!(spec.label().starts_with(level.name()));
                assert!(!spec.describe().is_empty());
            }
        }
    }

    /// The style axis crosses the difficulty axis without touching it: a
    /// rung with a personality is the same rung — kind, budget, handicap —
    /// with a different evaluator bias, and a `Balanced` request is
    /// exactly `Level::spec`.
    #[test]
    fn a_personality_changes_the_evaluator_and_nothing_else_about_a_rung() {
        for side in [Side::Corp, Side::Runner] {
            for level in Level::ALL {
                let plain = level.spec(side);
                assert_eq!(plain.personality, Personality::Balanced, "the calibrated rung is Balanced");
                assert_eq!(plain.with_personality(Personality::Balanced), plain);
                let styled = plain.with_personality(Personality::Rush);
                assert_eq!(styled.personality, Personality::Rush);
                assert_eq!(
                    (styled.level, styled.side, styled.kind, styled.simulations, styled.samples, styled.epsilon),
                    (plain.level, plain.side, plain.kind, plain.simulations, plain.samples, plain.epsilon)
                );
                let _: Box<dyn BotAgent> = styled.agent(3);
            }
        }
    }

    /// The two chairs convert different resources, so the top rung is
    /// deliberately a different bot on each — see the module docs. This
    /// pins that asymmetry so it cannot be "tidied up" into one spec.
    #[test]
    fn the_top_rung_is_a_different_bot_on_each_chair() {
        let corp = Level::Elite.spec(Side::Corp);
        let runner = Level::Elite.spec(Side::Runner);
        assert_eq!((corp.kind, corp.simulations, corp.samples), (LevelKind::Puct, 512, 1));
        assert_eq!((runner.kind, runner.simulations, runner.samples), (LevelKind::Mcts, 128, 4));
    }

    /// The property the first cut of this table did not have. Every rung
    /// is either a strictly better base bot than the one below it, or the
    /// same base bot with strictly less handicap — so "higher is
    /// stronger" holds structurally, and calibration only has to say by
    /// how much. Without this, spacing the ladder is guesswork checked
    /// after the fact by 1,200 games.
    #[test]
    fn each_rung_is_the_one_below_it_with_less_handicap_or_a_better_base() {
        for side in [Side::Corp, Side::Runner] {
            for pair in Level::ALL.windows(2) {
                let (lower, upper) = (pair[0].spec(side), pair[1].spec(side));
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
