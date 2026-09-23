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
//! The cross keeps the order but not the spacing, so a style measured to
//! break a step carries its own handicap in `LevelSpec::with_personality`
//! — so far the `veteran` Corp in `trap` (Phase 5 §14). A style whose
//! search rungs do not climb at all plays `Balanced` there instead:
//! `rush`'s `veteran` and `elite` Corp (§15). And a style whose best bot
//! is not a search at all has a ladder of its own base: `glacier`'s Corp
//! is one ply at five handicaps, like the Runner's (§21).
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
    /// One ply, blundering a third of the time as the Corp and three
    /// decisions in four as the Runner: it takes the obvious line —
    /// advance, rez, break, run an open server — and then throws a click
    /// away. The rung where a new player's mistakes stop being the only
    /// ones on the table. The two chairs differ because the Runner's
    /// whole ladder is handicaps of one base bot and so carries the whole
    /// span in `epsilon`, where the Corp's changes base at rung 4.
    Apprentice,
    /// One ply, played straight as the Corp; as the Runner, one ply
    /// throwing away every second decision, because on that chair one ply
    /// *is* the top of the ladder and all five rungs have to fit under it
    /// (see `spec`). No plan beyond the current turn.
    Operator,
    /// The strongest bot on this chair, blundering one decision in five as
    /// the Corp and one in four as the Runner — a real opponent with a
    /// visible crack in it.
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
    /// The two chairs diverge at the top for the reason in the module
    /// docs: depth is what the Corp chair converts, and samples are what
    /// the Runner chair converts, so `Elite` is `puct` at 512 on one side
    /// and `mcts` at four determinizations on the other.
    pub fn spec(self, side: Side) -> LevelSpec {
        // Anchors, all against a fixed *un-handicapped one-ply* opponent
        // on the other chair, re-taken 15 September 2026 at 768 games a
        // cell: random 0.012 / 0.125 (Corp / Runner win rate), one ply
        // 0.167 / 0.833, `puct@512` as Corp 0.266. Every rung is one of
        // those endpoints mixed toward random by `epsilon`, so the order
        // needs no measurement and the *spacing* is what Phase 5 §2
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
            // One ply is the cheap base, and the bottom of the ladder has
            // to stay cheap: handicapping `puct@512` here would think for
            // as long as the top rung while playing badly.
            //
            // **The Corp's `apprentice` is 0.25 since Phase 5 §22** (23
            // September 2026): re-taken after the trap terms (§20) and #128,
            // 0.35 scored 0.064 against `operator`'s 0.155, below the
            // midpoint (0.083). The curve is flat and noisy here — 0.064 /
            // 0.062 / 0.082 / 0.077 / 0.091 at 0.35 / 0.30 / 0.25 / 0.20 /
            // 0.15, `trap` within 0.006 of it at each — so 0.25 is the
            // first value that reaches the midpoint, not a fitted one.
            // Confirmed on the shipped table: `Balanced` 0.081, `trap`
            // 0.086, `rush` 0.051, every step below `operator` a rise on
            // each seed alone.
            (Level::Apprentice, Side::Corp) => (LevelKind::Heuristic, 0, 1, 0.25),
            (Level::Operator, Side::Corp) => (LevelKind::Heuristic, 0, 1, 0.0),
            // The Runner's whole ladder is this one base, so its five
            // handicaps carry the whole span and are spaced evenly; see
            // the note below the table.
            (Level::Apprentice, Side::Runner) => (LevelKind::Heuristic, 0, 1, 0.75),
            (Level::Operator, Side::Runner) => (LevelKind::Heuristic, 0, 1, 0.50),
            // The top two rungs share a base and differ only in epsilon,
            // which is the one step in the ladder guaranteed monotone
            // without measuring anything at all.
            //
            // **The epsilon is per chair because the span is.** From one
            // ply to the best bot is 0.229 of win rate on the Corp chair
            // and only 0.104 on the Runner's, and the handicap is steep:
            // 0.17 cost the Corp 0.187, nearly the whole span, landing
            // `veteran` on top of `operator` (Phase 5 §2). 0.10 was that
            // measurement inverted to put the rung mid-span.
            //
            // **It is 0.20 since Phase 5 §14** (16 September 2026, 768
            // games a cell against the one-ply balanced Runner): once the
            // pool moved, 0.10 scored 0.236 against `operator` 0.125 and
            // `elite` 0.258, a top step of +0.022 a person could not feel.
            // The measured curve — 0.236 / 0.219 / 0.176 / 0.180 / 0.146 /
            // 0.111 at 0.10 / 0.15 / 0.18 / 0.20 / 0.25 / 0.30 — falls
            // steeply between 0.15 and 0.18 and is flat to 0.20, so 0.20
            // is the round number on the midpoint (0.192): steps of +0.055
            // and +0.078, each a rise on both seeds alone. By 0.25 the
            // lower step is flat and at 0.30 it inverts.
            //
            // **§22 left it at 0.20 on purpose, and it no longer spaces
            // anything.** Re-taken after the trap terms, 0.20 scores 0.156
            // against `operator` 0.155 and `elite` 0.223. No handicap
            // splits that 0.068: the curve is 0.160 / 0.168 / 0.181 /
            // 0.167 / 0.194 / 0.197 at 0.12 / 0.10 / 0.08 / 0.05 / 0.03 /
            // 0.02, a smaller budget is worse (`puct@128` 0.161, `@256`
            // 0.169), and 0.03 on the shipped table read 0.208 — level
            // with `elite`. A fifth rung needs a stronger `elite`, not a
            // handicap (§22 records the style matrix that points at one).
            (Level::Veteran, Side::Corp) => (LevelKind::Puct, 512, 1, 0.20),
            (Level::Elite, Side::Corp) => (LevelKind::Puct, 512, 1, 0.0),
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
        // **The Runner's handicaps are evenly spaced and the Corp's are
        // not, because only one of the two chairs is a single base bot.**
        // Calibrated 15 September 2026 at 768 games a cell (two seeds ×
        // 384, Phase 5 §2): the first cut's 1.0 / 0.5 / 0.25 / 0.10 / 0.0
        // crammed its top three rungs, scoring 0.125 / 0.449 / 0.634 /
        // 0.789 / 0.833 against a fixed one-ply Corp — steps of +0.324,
        // +0.185, +0.155 and **+0.044**, the last one `flat` on one of
        // the two seeds, which is a level selector a player cannot feel.
        // `epsilon` turned out to be near-linear in win rate on this
        // chair (w ≈ 0.833 − 0.70ε fits all five points to 0.034), so
        // interpolating the measured curve for four even steps of 0.177
        // gives 0.73 / 0.46 / 0.23 — these round numbers, within 0.03.
        // The Corp's 1.0 / 0.35 / 0.0 (0.25 since §22) were left alone: that chair changes
        // *base* between rungs 3 and 4, and measured at 0.012 / 0.078 /
        // 0.167 / 0.221 / 0.266 it is already even to within 0.025 of its
        // own step.
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
    /// is still the same bot with less handicap at the rung above). Before
    /// this existed, `--corp-level 4 --corp-personality rush` played
    /// `Balanced` and said nothing about it.
    ///
    /// **The order survives a style and the spacing does not**, so a style
    /// whose spacing was measured to break carries its own `epsilon` here.
    /// Everything but the style is read back off `Level::spec` — the base
    /// as well as the handicap, since `glacier` changes the base — so
    /// styling a styled rung again is the same as styling the
    /// plain one.
    pub fn with_personality(self, personality: Personality) -> Self {
        let personality = match (self.level, self.side, personality) {
            // **A `rush` Corp's top two rungs play `Balanced`** (Phase 5
            // §15), the one pairing where the style is not the style
            // asked for. §12 found search worth +0.012 to `rush` over one
            // ply, and on the six rush decks alone it is worth *less* than
            // nothing — one ply 0.101, `puct@512` 0.062 at `veteran` and
            // 0.035 at `elite` — so no handicap could make those rungs
            // climb. Of §12's three routes (repair the profile, seat
            // another style above `operator`, say so on the start screen)
            // this is the second: `Balanced`'s rungs are already calibrated
            // and read 0.100 / 0.180 / 0.258 from `operator` (0.101 /
            // 0.149 / 0.271 on rush decks), where `glacier`'s would jump
            // straight to 0.309. The cost is that the top of a rush deck's
            // ladder does not play like a rush; nothing on the start screen
            // says so yet, because its rung list is drawn before a style
            // is resolved.
            (Level::Veteran | Level::Elite, Side::Corp, Personality::Rush) => Personality::Balanced,
            _ => personality,
        };
        // **A `glacier` Corp's whole ladder is one ply at five handicaps**
        // (Phase 5 §21), the Runner ladder's shape and for the same reason:
        // the style's best bot is not a search. Once `glacier` built the
        // fort it is named for (§19), its one ply scored 0.443 against the
        // one-ply balanced Runner (768 games, two seeds) where `puct@512`
        // scored 0.343 at `veteran` and 0.372 at `elite` — the fort pays off
        // over turns, past a 512-simulation horizon, so the search rungs
        // ran *below* `operator`. More handicap on `puct` could only push
        // them further down; nothing on it lifts `elite` above one ply.
        // The measured curve (ε 0 / .03 / .04 / .05 / .10 / .12 / .15 / .20 /
        // .22 / .25 / .30 / .40 / .50 / .75: 0.443 / 0.365 / 0.329 / 0.311 /
        // 0.251 / 0.208 / 0.189 / 0.135 / 0.121 / 0.096 / 0.079 / 0.049 /
        // 0.030 / 0.016) is steep near 0, so these are read off it for four
        // even steps from random's 0.012, not interpolated from a line.
        // Confirmed on the shipped table: 0.012 / 0.142 / 0.223 / 0.329 /
        // 0.430, every step a rise on each seed alone. `veteran` at 0.04
        // read 0.372 there, leaving a top step of +0.058 at 1.7 sd a seed,
        // which is why it is 0.05. The top of the ladder is also the cheap
        // one now: no `glacier` rung searches.
        if self.side == Side::Corp && personality == Personality::Glacier {
            let epsilon = match self.level {
                Level::Novice => 1.0,
                Level::Apprentice => 0.22,
                Level::Operator => 0.11,
                Level::Veteran => 0.05,
                Level::Elite => 0.0,
            };
            return Self { personality, epsilon, kind: LevelKind::Heuristic, simulations: 0, samples: 1, ..self.level.spec(self.side) };
        }
        let epsilon = match (self.level, self.side, personality) {
            // `trap` as the Corp (§14). §13 read its flat top step as
            // `Balanced`'s own and expected it to follow `Balanced`'s
            // re-spacing; it did not — 0.20 costs `trap` 0.085 against
            // `Balanced`'s 0.056, and `operator → veteran` went flat
            // (+0.010). Measured at 0.10 / 0.12 / 0.15 / 0.20: 0.284 /
            // 0.263 / 0.232 / 0.199, target 0.243. 0.15 is nearest, steps
            // +0.043 (z 2.1) and +0.065 (z 2.9) pooled; each candidate
            // leaves one step flat on one seed alone, because the style's
            // whole `operator → elite` span is 0.108.
            (Level::Veteran, Side::Corp, Personality::Trap) => 0.15,
            _ => self.level.spec(self.side).epsilon,
        };
        Self { personality, epsilon, ..self.level.spec(self.side) }
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

    /// A `rush` Corp's search rungs are `Balanced`'s rungs exactly, and
    /// every other rung and chair keeps the style it was asked for —
    /// including a `rush` Corp's one-ply rungs, which are where the style
    /// still plays.
    #[test]
    fn rush_corp_search_rungs_play_balanced_and_nothing_else_is_restyled() {
        for side in [Side::Corp, Side::Runner] {
            for level in Level::ALL {
                for personality in Personality::ALL {
                    let styled = level.spec(side).with_personality(personality);
                    if side == Side::Corp && personality == Personality::Rush && matches!(level, Level::Veteran | Level::Elite) {
                        assert_eq!(styled, level.spec(side), "{level} rush Corp is Balanced's rung");
                    } else {
                        assert_eq!(styled.personality, personality, "{level} {side:?} {personality:?}");
                    }
                }
            }
        }
    }

    /// The Corp style measured to break `veteran`'s spacing carries its
    /// own handicap, which only it carries, and which a second styling
    /// cannot leak into another style. `glacier`'s Corp is the other
    /// exception and has a test of its own.
    #[test]
    #[allow(clippy::float_cmp)]
    fn styled_veteran_corps_have_their_own_handicap_and_no_other_rung_does() {
        let balanced = Level::Veteran.spec(Side::Corp);
        assert_eq!(balanced.epsilon, 0.20);
        assert_eq!(balanced.describe(), "searches 512 positions per decision, and throws away about 2 decisions in 10");
        let trap = balanced.with_personality(Personality::Trap);
        assert_eq!(trap.epsilon, 0.15);
        assert_eq!(trap.with_personality(Personality::Balanced), balanced);
        assert_eq!(Level::Veteran.spec(Side::Runner).with_personality(Personality::Trap).epsilon, 0.25);
        for side in [Side::Corp, Side::Runner] {
            for level in Level::ALL {
                for personality in Personality::ALL {
                    let own = side == Side::Corp
                        && (personality == Personality::Glacier || (level == Level::Veteran && personality == Personality::Trap));
                    if !own {
                        let styled = level.spec(side).with_personality(personality);
                        assert_eq!(styled.epsilon, level.spec(side).epsilon, "{level} {side:?} {personality:?}");
                    }
                }
            }
        }
    }

    /// A `glacier` Corp is one ply at every rung, its handicap falling to
    /// nothing at `elite` (Phase 5 §21); restyling it gives the calibrated
    /// rung back, and the Runner chair is untouched by the style.
    #[test]
    #[allow(clippy::float_cmp)]
    fn a_glacier_corp_is_one_ply_at_five_handicaps() {
        let table = Level::ALL.map(|level| {
            let spec = level.spec(Side::Corp).with_personality(Personality::Glacier);
            assert_eq!((spec.kind, spec.simulations, spec.samples, spec.personality), (LevelKind::Heuristic, 0, 1, Personality::Glacier));
            assert_eq!(spec.with_personality(Personality::Balanced), level.spec(Side::Corp), "{level}");
            assert_eq!(level.spec(Side::Runner).with_personality(Personality::Glacier).epsilon, level.spec(Side::Runner).epsilon);
            spec.epsilon
        });
        assert_eq!(table, [1.0, 0.22, 0.11, 0.05, 0.0]);
        let veteran = Level::Veteran.spec(Side::Corp).with_personality(Personality::Glacier);
        assert_eq!(veteran.describe(), "looks one move ahead, and throws away about one decision in 20");
    }

    /// The two chairs convert different resources, so the top rung is
    /// deliberately a different bot on each — see the module docs. This
    /// pins that asymmetry so it cannot be "tidied up" into one spec:
    /// the Corp's is the deep search, the Runner's is one ply played
    /// straight, and the Runner's `operator` is that same bot handicapped.
    #[test]
    fn the_top_rung_is_a_different_bot_on_each_chair() {
        let corp = Level::Elite.spec(Side::Corp);
        let runner = Level::Elite.spec(Side::Runner);
        assert_eq!((corp.kind, corp.simulations, corp.samples), (LevelKind::Puct, 512, 1));
        assert_eq!((runner.kind, runner.simulations, runner.samples, runner.epsilon), (LevelKind::Heuristic, 0, 1, 0.0));
        assert_eq!(Level::Operator.spec(Side::Corp).epsilon, 0.0, "the Corp's operator is one ply played straight");
        assert!(Level::Operator.spec(Side::Runner).epsilon > 0.0, "the Runner's is not, because one ply is the top");
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
