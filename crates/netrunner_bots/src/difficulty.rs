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
//! **Then the leaf moved and the cap inverted** (15 September 2026):
//! with the Runner's evaluator pricing what a run can find and what a
//! card in grip is worth (Phase 2 §5a, reopened), one ply scores 0.865
//! against the fixed heuristic Corp where `mcts@128` scores 0.677 and
//! `puct@128` 0.760. A ladder that seated `mcts` above one ply would run
//! backwards at the top, so the Runner chair is now **one base bot at
//! five handicaps** — 1.0 / 0.75 / 0.50 / 0.25 / 0.0 — monotone by
//! construction and cheap at every rung. The Corp chair is unchanged: its
//! evaluator did not move and `puct@512` still converts depth there.
//!
//! **Both chairs now climb at every step, and the calibration is what
//! says so** (15 September 2026, 768 games a cell over two seeds; the
//! table is in `docs/roadmap/phase-5-difficulty-ladder.md` §2). The two
//! paragraphs above are the history of a cap that has moved twice, and
//! the standing consequence is on the *other* chair now: `puct@512` wins
//! **0.266** against the un-handicapped one-ply Runner, where the first
//! calibration measured that cell at 0.714. The Corp ladder is evenly
//! spaced and tops out too low, which is the Runner chair's old problem
//! transferred — and like it, the lever is that chair's evaluator rather
//! than this table.
//!
//! **Then the Corp's did the same** (23 September 2026, Phase 5 §24).
//! Once every Corp profile built a fort (§19, §23) the one-ply Corp scored
//! about 0.41 against the un-handicapped one-ply Runner, where `puct@512`
//! scored 0.33 at `elite` and 0.14 at `veteran` — the fort pays off over
//! turns, past the search's horizon. So **both chairs are now one base bot
//! at five handicaps**: no rung searches, the order is structural, and only
//! the spacing is measured. `LevelKind`'s search variants stay, because the
//! day a search beats one ply again on either chair its top rungs go back
//! to it, and the monotonicity tests below still say what that would owe.
//!
//! **Personality is not the difficulty dial, and deliberately so.** The
//! six `Personality` profiles are a *style* axis, and each is written for
//! one chair — a Runner profile seated as the Corp "touches only the
//! shared terms, which is harmless and useless" (`personality`'s own doc
//! comment). A ladder built on them would be asymmetric between the
//! chairs for a reason that has nothing to do with how hard the opponent
//! is, which is how the first draft of this table shipped a rung that
//! was conservative as the Runner and unchanged as the Corp. Every rung
//! here is `Balanced`; what climbs is the handicap. "A glacier Corp at
//! level 4" is a second axis crossed with this one, not a replacement.
//! The cross keeps the order, and since §24 it keeps the spacing too:
//! four Corp styles measured on one `epsilon` curve, so no style carries
//! a handicap of its own (three once did; `LevelSpec::with_personality`
//! records why each went).
//!
//! **Machine-independence is a prerequisite, not a detail.** A rung whose
//! strength moved with the host's core count would not be a rung at all;
//! `MctsAgent::DEFAULT_TREES` became a fixed number for this reason
//! (item 41), and every spec here states its tree count outright.

use netrunner_core::rules::Side;
use serde::{Deserialize, Serialize};

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
    /// for — the measured floor is 0.012 as Corp and 0.125 as Runner
    /// against an un-handicapped one ply.
    Novice,
    /// One ply, blundering about one decision in five as the Corp and
    /// three in four as the Runner: it takes the obvious line — advance,
    /// rez, break, run an open server — and then throws a click away. The
    /// rung where a new player's mistakes stop being the only ones on the
    /// table.
    Apprentice,
    /// One ply, throwing away about one decision in ten as the Corp and
    /// every second one as the Runner. No plan beyond the current turn.
    Operator,
    /// One ply with a rare blunder — one decision in twenty as the Corp,
    /// one in four as the Runner: a real opponent with a visible crack in
    /// it.
    Veteran,
    /// The strongest bot measured on each chair, playing every decision.
    /// On both chairs today that is one ply; see the module docs.
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
    /// Both chairs are one ply at five handicaps (the module docs say how
    /// each got there); the handicaps differ because the chairs' `epsilon`
    /// curves do.
    pub fn spec(self, side: Side) -> LevelSpec {
        // Anchors, all against a fixed *un-handicapped one-ply* opponent
        // on the other chair: random 0.012 / 0.125 (Corp / Runner win
        // rate), one ply 0.43 / 0.833 (Corp re-taken 23 September 2026,
        // §24, 768 games a cell; Runner 15 September). Every rung is one
        // of those endpoints mixed toward random by `epsilon`, so the
        // order needs no measurement and the *spacing* is what Phase 5 §2
        // calibrates. The figures the first cut used (0.042 / 0.167,
        // 0.562 / 0.438, `puct@512` 0.714, `mcts@128` 0.792) are still in
        // the roadmap and are not comparable to these: they were taken on
        // the pre-`access_prospect` Runner evaluator, which moved the
        // whole pool — the same 25 cells run 0.471 Corp on this spec
        // (0.385 on the one it replaced) against the engine's 0.548.
        let (kind, simulations, samples, epsilon) = match (self, side) {
            // One ply at `epsilon` 1.0 rather than `LevelKind::Random`,
            // and the two are identical in play: `HandicapAgent` never
            // consults the inner agent at 1.0, so no heuristic ever runs.
            // Writing it this way makes the bottom three rungs one base
            // family with strictly falling handicap — 1.0, 0.25, 0.0 as
            // the Corp, 1.0, 0.75, 0.50 as the Runner — which is what
            // `each_rung_is_the_one_below_it_with_less_handicap_or_a_
            // better_base` can actually check.
            (Level::Novice, _) => (LevelKind::Heuristic, 0, 1, 1.0),
            // **The Corp's whole ladder is one ply at five handicaps** since
            // Phase 5 §24 (23 September 2026), the Runner ladder's shape and
            // `glacier`'s since §21. Once every Corp profile built a fort
            // (§23), one ply played straight scored 0.417 (`Balanced`) and
            // 0.409 (`trap`) against the one-ply balanced Runner, where
            // `puct@512` scored 0.326 / 0.339 at `elite` and 0.146 / 0.138 at
            // `veteran` — the fort pays off over turns, past a 512-simulation
            // horizon, so the search rungs ran *below* `operator`. Until then
            // the Corp's top two rungs were `puct@512` at `epsilon` 0.20 and
            // 0.0, and §22 had found that no handicap could space them.
            //
            // The handicaps are `glacier`'s from §21, read off a measured
            // curve that is steep near 0 and nowhere near linear. §24
            // re-measured it in every other Corp style and found the same
            // curve (ε 0.22 / 0.11 / 0.05 / 0.03: `Balanced` 0.128 / 0.225 /
            // 0.331 / 0.368, `trap` 0.109 / 0.221 / 0.320 / 0.359, `rush`
            // 0.139 / 0.246 / 0.358 / 0.402; `glacier` 0.121 / — / 0.311 /
            // 0.365), so one table serves all four and no style carries a
            // handicap of its own. One ply is also the cheap base: no Corp
            // rung searches, so `elite` answers as fast as `operator`.
            (Level::Apprentice, Side::Corp) => (LevelKind::Heuristic, 0, 1, 0.22),
            (Level::Operator, Side::Corp) => (LevelKind::Heuristic, 0, 1, 0.11),
            (Level::Veteran, Side::Corp) => (LevelKind::Heuristic, 0, 1, 0.05),
            (Level::Elite, Side::Corp) => (LevelKind::Heuristic, 0, 1, 0.0),
            // The Runner's whole ladder is this one base, so its five
            // handicaps carry the whole span and are spaced evenly; see
            // the note below the table.
            (Level::Apprentice, Side::Runner) => (LevelKind::Heuristic, 0, 1, 0.75),
            (Level::Operator, Side::Runner) => (LevelKind::Heuristic, 0, 1, 0.50),
            // **The Runner ladder is five handicaps of one ply** since
            // 15 September 2026 (Phase 2 §5a, reopened): once the
            // evaluator priced what a run can find and what a card in
            // grip is worth, the one-ply Runner scored **0.865** against
            // the fixed heuristic Corp over 192 games, against 0.760 for
            // `puct@128` and 0.677 for `mcts@128` — the search Runners
            // inherit the new leaf but not the policy that plays the
            // plies before it, and their random playouts wash most of it
            // out. Seating `mcts` above one ply would make rung 5 easier
            // than rung 3. So the Runner's `operator` is no longer the
            // un-handicapped bot: that is `elite`. The order is
            // structural and the spacing is now measured — the four steps
            // are even by construction of the note below the table.
            // When a search Runner beats one ply again —
            // a one-ply playout policy is the recorded next lever — the
            // top two rungs go back to it.
            (Level::Veteran, Side::Runner) => (LevelKind::Heuristic, 0, 1, 0.25),
            (Level::Elite, Side::Runner) => (LevelKind::Heuristic, 0, 1, 0.0),
        };
        // **Both chairs are one base bot at five handicaps, and only the
        // Runner's are evenly spaced in `epsilon`, because only its curve
        // is a line.** Calibrated 15 September 2026 at 768 games a cell
        // (two seeds × 384, Phase 5 §2): the Runner's first cut, 1.0 / 0.5
        // / 0.25 / 0.10 / 0.0, crammed its top three rungs, scoring 0.125 /
        // 0.449 / 0.634 / 0.789 / 0.833 against a fixed one-ply Corp — steps
        // of +0.324, +0.185, +0.155 and **+0.044**, the last one `flat` on
        // one of the two seeds, which is a level selector a player cannot
        // feel. `epsilon` turned out to be near-linear in win rate on that
        // chair (w ≈ 0.833 − 0.70ε fits all five points to 0.034), so
        // interpolating the measured curve for four even steps of 0.177
        // gives 0.73 / 0.46 / 0.23 — these round numbers, within 0.03. The
        // Corp's curve is steep near 0 (one decision in twenty thrown away
        // costs it a quarter of its margin), so its handicaps crowd toward
        // the top instead.
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
    /// `Level::spec` always hands back `Balanced` because a rung is named
    /// before a deck's style is resolved. A profile is a *bias* on the same
    /// evaluator — it ranks the same legal moves differently, never more or
    /// less deeply — so the rung's strength order survives it (a
    /// handicapped bot with a style is still the same bot with less
    /// handicap at the rung above). Before this existed,
    /// `--corp-level 4 --corp-personality rush` played `Balanced` and said
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
                let _: Box<dyn BotAgent> = spec.agent(7);
                assert!(spec.label().starts_with(level.name()));
                assert!(!spec.describe().is_empty());
            }
        }
    }

    /// The style axis crosses the difficulty axis without touching its
    /// base: a rung with a personality is the same rung — kind, budget,
    /// and handicap unless the style has a measured one of its own — with
    /// a different evaluator bias, and a `Balanced` request is exactly
    /// `Level::spec`.
    #[test]
    fn a_personality_changes_the_evaluator_and_nothing_else_about_a_rung() {
        for side in [Side::Corp, Side::Runner] {
            for level in Level::ALL {
                let plain = level.spec(side);
                assert_eq!(plain.personality, Personality::Balanced, "the calibrated rung is Balanced");
                assert_eq!(plain.with_personality(Personality::Balanced), plain);
                let styled = plain.with_personality(Personality::Aggressive);
                assert_eq!(styled.personality, Personality::Aggressive);
                assert_eq!(
                    (styled.level, styled.side, styled.kind, styled.simulations, styled.samples, styled.epsilon),
                    (plain.level, plain.side, plain.kind, plain.simulations, plain.samples, plain.epsilon)
                );
                let _: Box<dyn BotAgent> = styled.agent(3);
            }
        }
    }

    /// Every Corp style climbs the same one-ply ladder (Phase 5 §24):
    /// the style is the only thing a personality changes, and the
    /// handicaps are the table §21 read off `glacier`'s curve.
    #[test]
    #[allow(clippy::float_cmp)]
    fn every_corp_style_is_one_ply_at_the_same_five_handicaps() {
        for personality in Personality::ALL {
            let table = Level::ALL.map(|level| {
                let spec = level.spec(Side::Corp).with_personality(personality);
                assert_eq!(
                    (spec.kind, spec.simulations, spec.samples, spec.personality),
                    (LevelKind::Heuristic, 0, 1, personality),
                    "{level} {personality:?}"
                );
                assert_eq!(spec.with_personality(Personality::Balanced), level.spec(Side::Corp), "{level}");
                spec.epsilon
            });
            assert_eq!(table, [1.0, 0.22, 0.11, 0.05, 0.0], "{personality:?}");
        }
        let veteran = Level::Veteran.spec(Side::Corp).with_personality(Personality::Trap);
        assert_eq!(veteran.describe(), "looks one move ahead, and throws away about one decision in 20");
    }

    /// Neither chair's `elite` searches: on each, one ply played straight
    /// is the strongest bot measured (see the module docs), and the rung
    /// below it is the same bot with a handicap.
    #[test]
    #[allow(clippy::float_cmp)]
    fn the_top_rung_is_one_ply_played_straight_on_both_chairs() {
        for side in [Side::Corp, Side::Runner] {
            let elite = Level::Elite.spec(side);
            assert_eq!((elite.kind, elite.simulations, elite.samples, elite.epsilon), (LevelKind::Heuristic, 0, 1, 0.0), "{side:?}");
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
        for (side, personality) in [Side::Corp, Side::Runner].into_iter().flat_map(|side| Personality::ALL.map(|p| (side, p))) {
            for pair in Level::ALL.windows(2) {
                let (lower, upper) = (pair[0].spec(side).with_personality(personality), pair[1].spec(side).with_personality(personality));
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
