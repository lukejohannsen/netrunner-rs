//! The static position score every determinizing bot reads — one ply's
//! leaf, a rollout's, the uniform PUCT value head's and the gym's shaped
//! reward — as a sum of modules (Phase 5 §25 Stage 3, 29 September 2026).
//!
//! **The shape.** `Weights` and the constants that document each term's
//! measurement live here; `evaluate_state_with` adds the shared prefix
//! (`fundamentals`: points, the parked decision, credits) and then one
//! side's arm (`corp::score`, `runner::score`), **in the order the one
//! 4,300-line file added them**, so the split is byte-identical to it —
//! a floating-point sum follows its order, and `coverage_identical.py`
//! in all four shapes is what says the games did not move. `read` holds
//! what both arms read off the cards and the board (break costs, the
//! trap recognisers, rig coverage, the fort's ice), `identities` what
//! both identities print about a moment a leaf prices ahead of time (a
//! run's success, breach, accesses and end — Phase 5 §34), and `stage` reads
//! where the game is off the board — the reading `diag precepts` bins
//! by, kept here so the report and any term that later conditions on it
//! share one definition.
//!
//! **The stage dial is gone, and its record stays in the archive.**
//! `runner_stage`, `stage_weights`, `lerp`, `STAGE_GAIN` and
//! `Weights::stage_gain` interpolated the Runner's weights between two
//! personalities by how built its rig was, and lost on both chairs
//! twice (Phase 5 §6–§8, §19): what a leg scores follows where it spends
//! the game, not when it moves. A stage is a condition inside a term,
//! never a switch between weight sets, and `stage::Stage` is what such a
//! term will read.
//!
//! **Economy at the guide's rate (Stage 5)** is eight terms after the
//! old sum, every one zero in `Weights::default()` and set by
//! `Weights::at_the_guides_rate`, which the turn planner scores with:
//! a click is a credit; a card's declared economy is read off the DSL
//! (`read::declared_income`) for what a play nets and what an installed
//! economy card will still pay over the turns the stage expects
//! (`stage::horizon` — the first term to read the stage, and the stage
//! is a condition inside it); the Corp keeps the rez that stops a run
//! and a face-down ICE it cannot rez is worth half; the Corp reads the
//! Runner's credits against its scoring remote (the taxing window) and
//! the Runner reads the Corp's against a rez and an unfinished install.
//! The one-ply reference scores with the default, so it has not moved
//! and every ladder rung with it; Stage 8 makes the eight the default.
//!
//! **The Corp's plans (Stage 6) and the Runner's (Stage 7)** are two
//! more blocks of the same shape, switched on by `Weights::with_plans`
//! for the plans a deck's `Style` stacks: every Corp's four and every
//! Runner's four, then each plan's own. The Runner's block is where the
//! evaluator first reads an identity — the Corp's across the table, for
//! the last-click and feared-flatline terms (`read::corp_faction`), and
//! the Runner's own, which names its plan when the deck does not
//! (`plans::Style::or_faction`).

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{
    card_matches_filter, Amount, CardDefinition, CardFilter, CardType, CardZoneRef, Cost, Effect, EffectRequirement, IceType,
    SubroutineBreakCount, Trigger,
};
use netrunner_core::rules::continuous;
use netrunner_core::rules::{
    current_actor, GamePhase, GameState, InstalledCard, InstalledRunnerCard, PendingDecision, RunIce, RunPhase,
    RunState, Side, SubroutineStatus,
};

pub mod corp;
pub mod fundamentals;
pub mod identities;
pub mod read;
pub mod runner;
pub mod stage;
#[cfg(test)]
mod test_support;

// Each module reads the others' facts through `use super::*`, so the
// module tree's own items are one namespace here: a term in `runner`
// prices a Corp install with `corp_install_value` the way the one file
// did, and the names are the same names.
use corp::*;
use fundamentals::*;
use read::*;

pub use read::{breaker_coverage, covers, damage_grows_with_advancement, is_hand_trap, is_lure_trap, is_unrezzed_threat, punishes_access_with_damage, punishes_runs, server_break_cost};
pub use stage::{horizon, stage, Stage};
pub(crate) use fundamentals::{picked_before, searched_answers};

// ---------------------------------------------------------------------
// Economy at the guide's rate (Phase 5 §25 Stage 5). **Every constant in
// this block is zero in `Weights::default()` and set by
// `Weights::at_the_guides_rate`**, which the turn planner scores with.
// The default was the one-ply reference's until Stage 8 deleted it, and
// it stays the base every profile and every term is a delta from: the
// evaluator's other readers (`MctsAgent`, the uniform PUCT evaluator, the
// gym's reward) score with it, and every constant's record is a
// measurement against it. The strategy guide's sentences are quoted
// where a constant is theirs.
// ---------------------------------------------------------------------

/// Each click its owner has left this turn. "A click can always be
/// turned into 1 credit or 1 card with a basic action, so a click is
/// worth at least one of either" — so a click is priced at the credit it
/// would buy, and every action is judged against that rate: a free
/// action (a rez, a score) keeps the click's worth, a card that gives
/// clicks (Nanomanagement) is worth the clicks, and one that takes them
/// (Creative Commission) pays for them. For the planner it is the floor
/// a line's unspent clicks were already priced at (`planner::click_floor`,
/// which now adds only what the evaluator does not carry); for the
/// chooser it moves nothing between two click actions.
const CLICK_WEIGHT: f64 = 0.4;
/// Each credit an active economy card will still pay over the turns the
/// stage expects (`stage::horizon`, `read::future_credits`) — an
/// installed PAD Campaign, Regolith Mining License's counters, Telework
/// Contract's, a Fermenter's cashout — and the same credits read off a
/// card in hand for what installing it would be worth.
///
/// **Why five-eighths of a credit now (0.25 against 0.4).** The term
/// banks the card's future, so using the card has to beat banking it:
/// taking 3[c] off Regolith for a click is +1.2 − 0.4 for the click −
/// what the stock loses (2 net credits at this weight, 0.5), +0.3 over
/// the credit click, and at the full credit weight it would be −0.4 and
/// the Corp would never take what it installed. Clicking 3 counters onto
/// Smartware Distributor is 3 × 0.25 − 0.4 = +0.35, so a click that
/// buys three credits over three turns beats one that buys one, which
/// is the guide's "only good if it beats that rate". And a PAD Campaign
/// installed early (9 turns) is 2.25 against the 1.6 its install and rez
/// cost, so it goes down and is rezzed, while the same card late (2
/// turns, 0.5) is not worth the credits. Hedge Fund stays the best
/// economy play at +1.2: "four credits for one click".
const FUTURE_CREDIT_WEIGHT: f64 = 0.25;
/// An event in the Runner's grip, at this fraction of what its play is
/// worth (`fundamentals::play_value`) — the Runner's `HELD_CARD_WEIGHT`
/// shape for the cards that term leaves at zero, and half for the same
/// reason: the card still has to be played, and the other half is what
/// playing it earns. Nothing for a card that declares no economy.
///
/// **The Corp's hand is not priced held, and that is a measurement.**
/// The term was written for both hands — a Hedge Fund in HQ at half its
/// play, so the Corp would draw toward it — and the Corp leg of the
/// bench said no: with it on, the planner Corp lost 0.068 of win share
/// to the one-ply reference on seed 1, with it off it *gained* 0.031,
/// and the precepts report says why. A planner sees a draw's value
/// through the line that plays what it draws, so a held value is not
/// a reason to draw but a reason to hold: the Corp kept the Hedge
/// Funds it could not afford at 4[c] and played 2.7 economy operations
/// a game where the reference played 2.8 and the same planner without
/// the term 3.6, and its HQ was full at a third more of the Runner's HQ
/// runs. The Runner's events kept the term: +0.02 on the Runner leg,
/// inside the band, recorded as such. A flat value per card in HQ was
/// a switch for the same reason one stage earlier (`RD_DRAW_RESERVE`).
const DECLARED_VALUE_WEIGHT: f64 = 0.5;
/// Each credit the Corp is short of its rez reserve
/// (`read::rez_reserve`: the dearest unrezzed piece of ICE it has
/// installed). The Corp's `SAVINGS_SHORTFALL_WEIGHT`, at the same
/// weight, and the same shape for the same reason: a penalty on the
/// shortfall vanishes the moment the rez is affordable, so it never
/// fights the rez it exists to enable; a bonus on credits held would.
/// Stage 4 found the planner Corp at its turn start able to rez what it
/// held 0.46 of the time against the chooser's 0.70, because every click
/// of a line that drew and installed beat the credit click and nothing
/// priced the credits an install would need to be turned face up. At
/// 0.3 a credit click closing the gap is +0.7, ahead of an install at
/// +1.0 only once the ICE in hand would raise the reserve, and behind
/// advancing (+1.1) always.
const REZ_RESERVE_WEIGHT: f64 = 0.3;
/// Each unrezzed piece of ICE whose printed rez cost the Corp does not
/// hold, subtracted from the `UNREZZED_INSTALL_WEIGHT` it is paid: half
/// of it, so a piece the Corp cannot turn face up is worth half of one it
/// can. **The term the first measurement of this stage asked for.** With
/// the reserve alone the planner Corp arrived at its turn with 4.2
/// credits where Stage 4's had 7.8 and the chooser 15: it installed
/// every ICE it drew (15.5 installs a game, the chooser's count, in
/// three-quarters of the turns) because a face-down card was worth the
/// same 1.0 whether or not its rez was in the bank, so "draw, install"
/// was worth 0.5 a click against a credit's 0.4 for as long as R&D
/// held ICE, and the reserve only taxed the spending that followed.
/// At 0.5, installing a Pharos on 2[c] where no fort term wants it is
/// +0.5 less the reserve it raises, under the credit click; the first
/// piece on an open central still goes down, as a bluff, because the
/// fort term carries it; and with the seven in hand every install is
/// +1.0 as it always was. The rez itself is priced as before — this is
/// the price of the promise, not of the rez. The Runner cannot read it: a face-down card's cost is
/// hidden, and `visible_install_value` reads none.
const UNAFFORDABLE_ICE_WEIGHT: f64 = 0.5;
/// Each installed agenda in a server the Runner cannot afford to break
/// into right now, were the Corp to rez what its credits cover
/// (`read::taxing_cost` against the Runner's credits) — the guide's
/// taxing window, precept 2: "a Runner who spent everything cannot run
/// your next one". The Corp reading the Runner's credits, which are
/// public. Stage 1 measured the window happening 0.49 of scores with no
/// term reading it; this is the term. One advancement token's worth: an
/// agenda worth installing behind the fort while the window is open,
/// never worth installing naked (the window is shut on a server with
/// no ICE, where the break costs nothing).
const TAXING_WINDOW_WEIGHT: f64 = 1.5;
/// Each credit the Corp would spend rezzing the unrezzed ICE ahead of the
/// Runner in the run — `TYPICAL_REZ_COST` a piece, or what the Corp has
/// left — at this weight: the Runner reading the Corp's credits against
/// its unrezzed rez costs (precept 8, "make the Corp rez": "every rez
/// costs the Corp credits it wanted for scoring"). The Corp's credit is
/// worth 0.2 to the Runner, and Stage 1 measured a run into unrezzed ICE
/// drawing a rez 0.64 of the time; 0.12 is the product. A piece the
/// Corp cannot afford to rez costs it nothing and is worth nothing here.
const FORCED_REZ_WEIGHT: f64 = 0.12;
/// A face-down card in the root of the server the Runner is running,
/// when the Corp holds the credits to finish advancing it next turn
/// (`TYPICAL_ADVANCEMENT_REQUIREMENT` less its tokens): "an unadvanced
/// card is only a threat if the Corp can afford to finish it next turn".
/// Half a token's worth (`ADVANCED_CARD_PROSPECT_WEIGHT`), on top of the
/// hidden access, so a fresh install in front of a rich Corp is worth a
/// remote run over a central and one in front of a broke Corp is not.
const FINISHABLE_INSTALL_WEIGHT: f64 = 0.5;
/// What a face-down piece of ICE costs to rez when its cost cannot be
/// read: the mean printed rez cost over the pool's 62 ICE is 4.4.
const TYPICAL_REZ_COST: u32 = 4;
/// What a face-down card needs to be scored when its requirement cannot
/// be read: the pool's 35 agendas need 2–5, median 3 — the low side, so
/// the Runner errs toward the run.
const TYPICAL_ADVANCEMENT_REQUIREMENT: u32 = 3;

// ---------------------------------------------------------------------
// The Corp's plans (Phase 5 §25 Stage 6). **Every constant in this block
// is zero in `Weights::default()` and in every `Plan::weights`**, and set
// by `Weights::with_plans` for the plans a style stacks, which the turn
// planner scores with (`Style::planned_weights`) — the same pinning as
// the block above, for the same reason. The first four are every Corp's,
// because the guide's sentences are in its "Playing the Corp" chapter,
// which every Corp plays; the rest are one plan's.
// ---------------------------------------------------------------------

/// Corp only, every plan: each agenda point the Runner's breach would
/// reach, subtracted while the run is one the Runner can break
/// (`read::run_stakes`, on `run_is_breakable`'s gate) — the agenda in
/// the remote's root, HQ's agendas at one access in its size, R&D's at
/// the deck's agenda density, every agenda in Archives. **What "rez it
/// when the run matters" means**: the reference's run term is flat
/// (`ACTIVE_RUN_AGAINST_WEIGHT` 1.5 for a run on anything), so a rez that
/// stops a run on an empty remote is worth the same to it as one that
/// stops a run on a 3-point agenda. With the stakes priced, a rez that
/// turns the run unbreakable recovers them, and one that does not
/// recovers nothing. Two a point: a 2-point agenda's run is 4.0 on top
/// of the flat term, an HQ run at one agenda in five 0.8, and an empty
/// remote's 0.0. Far under a point of agenda (20.0), since a run is a
/// chance and not a steal. **The tax rides with it**: the credits the
/// Runner still has to spend breaking the rezzed ICE ahead
/// (`read::remaining_break_cost`) are counted as the Corp's at
/// `OPPONENT_CREDIT_WEIGHT`, because they are about to be — which is what
/// makes a rez that only taxes worth making when the tax is real (Brân
/// 1.0 against a Cleaver) and not when it is a credit (Palisade).
const RUN_STAKES_WEIGHT: f64 = 2.0;
/// Corp only, every plan: each face-down piece of ICE the Corp could
/// rez at its printed cost **in the server the Runner is running**,
/// kept face down ("Rez late. An unrezzed piece of ICE costs the Runner
/// nothing to pass, but it also tells them nothing. Rez it when the run
/// matters, not the moment it is approached"). The reference rezzes
/// every affordable piece at its approach, because the rez is worth
/// `REZZED_ICE_WEIGHT` and its subroutines whether or not the run
/// matters: Palisade's rez is +1.2 on an empty remote against a rig
/// that breaks it for a credit. This is what the rez gives up — the
/// Runner's ignorance — and the stakes, the tax and the stop are what
/// win it back: at 1.5 that Palisade stays face down (+1.2 + 0.2 of
/// tax − 1.5), the same piece approached by a rig with no fracter is
/// rezzed (+1.2 + 1.2 for what the rig cannot break + 1.5 for the run it
/// ends + the stakes − 1.5), and Brân 1.0 against a Cleaver is rezzed
/// for the tax it takes. **Only in the server being run**, because the
/// decision is made at the approach and the term has to be on the
/// unrezzed side of it: paid on every face-down piece it was also a
/// reason to install every affordable piece in hand, and the Corp does
/// not install during a run.
///
/// **Zero, and that is the measurement.** At 1.5 the term does what it
/// says — the planner's rezzes a game fell 10.4 → 9.3 and the share of
/// its rezzes the Runner could break 0.46 → 0.41 in the precepts pass —
/// and it costs the chair: on the balanced Corp legs against the
/// reference, 384 games a seed, the planner with every term on was
/// +0.109 / +0.021, and with this one term off +0.122 / +0.073, so the
/// rez held is −0.013 and −0.052, the second past the seed band, on the
/// same games. What it holds is the tax: a rez the Runner breaks for
/// two credits is two credits the reference's Corp takes and this one
/// lets pass, and over a game of runs the tax is the wall. The reading
/// stays, at zero, like `UNREZZED_THREAT_WEIGHT`: "rez late" wants a
/// finer sentence than a flat value on the face-down piece, and the
/// measurement is the record of what the flat one is worth.
const REZ_HELD_WEIGHT: f64 = 0.0;
/// Corp only, every plan: each pair of adjacent pieces on one server
/// where the outer piece has no run-ending subroutine and the inner
/// piece has one, subtracted ("ICE that ends the run on the outside, so
/// the Runner must have the right breaker just to get in, and ICE that
/// punishes on the inside"). A new piece always goes outermost, so the
/// term reads the install that would put a taxing piece outside a
/// stopping one and sends it to another server instead. Small — under
/// the fort terms and under a credit — because the guide calls the habit
/// "a starting point, not a law".
const ICE_ORDER_WEIGHT: f64 = 0.3;
/// Corp only, every plan: each installed, unscored agenda the Corp could
/// finish next turn (`read::next_turn_advancements`: its clicks at a
/// token and a credit each, and the advancement its held operations
/// declare — Seamless Launch's two for a click and a credit), added, and
/// each it could not, subtracted: "Count the credits before you install:
/// an agenda you cannot finish next turn is an agenda sitting in the
/// open for a second turn." The never-advance line's condition, which
/// Stage 5 measured at 0.26 unadvanced installs a turn with nothing
/// reading whether the install was one the Corp could finish. One
/// advancement token's worth, the same as the taxing window: an
/// install the Corp can finish next turn is worth making now, and one it
/// cannot waits in HQ for the credits.
const NEVER_ADVANCE_WEIGHT: f64 = 1.0;
/// Corp only, `Plan::Kill`: the Runner's grip is smaller than the damage
/// the Corp could deal next turn from what it holds
/// (`read::damage_in_reach`: the fixed damage of the operations in HQ
/// whose play requirement is met now — Scorched Earth's "if the Runner
/// is tagged" — within its clicks and credits). "The Runner has to
/// choose between leaving your agendas alone and taking the risk." Not
/// the win, which the planner already sees when the line kills inside
/// its own turn (`WIN_SCORE`), but the threat, which the Runner's next
/// turn is spent answering: a tag cleared is a click and 2[c], a grip
/// drawn up is a click a card. Sized as a turn of the Runner's
/// (three clicks and a few credits at the Corp's rate for them), and
/// well under a point of agenda.
const LETHAL_THREAT_WEIGHT: f64 = 3.0;
/// Corp only, every plan (Phase 5 §51): each tag on the Runner, up to
/// two, while the Corp holds a card whose text asks for one
/// (`read::punishes_tags`: Scorched Earth, Retribution, Bigger Picture,
/// Orbital Superiority's scored half). A tag is worth only what follows
/// it, which is why a Corp holding no follow-up does not pay Public
/// Trail's 4[c] for one; a Corp holding it does, at 1.5 a tag, because
/// the tag is also the Runner's click and 2[c] to clear, or their turn
/// under the threat. Capped at two because the third tag punishes
/// nothing the second did not.
///
/// **It was the kill plan's term** (Stage 6), and the gate was always
/// the card in HQ: the plan said whether to read it, the card said
/// whether it was there. Fine Print, Hyper Velocity, Gimbatul and Quick
/// and Dirty are fast-advance decks holding Bigger Picture, IP
/// Enforcement, Retribution and Orbital Superiority — the guide's "tag
/// and punish" mixed into another plan — and the planner playing them
/// had Public Trail legal on 116 turns of 96 games, a punisher in HQ on
/// 78 of them, and played it 0 times (random seats 34). The deck says
/// which follow-up it holds by holding it; a style flag said it twice.
const TAG_LEVERAGE_WEIGHT: f64 = 1.5;
/// Corp only, `Plan::Traps`: one face-down card that is not an agenda in
/// the scoring remote ("Put an asset in your scoring server now and
/// then, so that a Runner has to pay the full price to find out"). The
/// fort term reads that remote as the fort still (`fort_value`: a bluffed
/// root keeps the fort), so the asset goes down behind the wall when no
/// agenda is in hand, and the agenda that comes goes in over it. One,
/// because a second asset in the fort is a second card the Corp will
/// not score. About an install's worth over a naked remote, so the
/// bluff is where the asset goes and never a reason to install one.
const BLUFF_WEIGHT: f64 = 1.0;

// ---------------------------------------------------------------------
// The Runner's plans, and the identity both chairs read (Phase 5 §25
// Stage 7). **Every constant in this block is zero in
// `Weights::default()` and in every `Plan::weights`**, and set by
// `Weights::with_plans` for a Runner seat — the same pinning as the two
// blocks above. The first four are every Runner's, because their
// sentences are in the guide's "Playing the Runner" chapter, which every
// Runner plays; the last three are one faction's plan's. What the Runner
// reads here that it could not before is the identity across the table
// (`read::corp_faction`) and its own (`Style::or_faction`).
// ---------------------------------------------------------------------

/// Runner only, every plan: a run begun on the Runner's last click
/// against a Corp that punishes runs, subtracted — "Don't run on your
/// last click against a Corp that punishes runs — Jinteki with its net
/// damage, NBN with its tags. Keep a click to draw back up or to clear a
/// tag afterwards." A Corp punishes runs when its identity is one of
/// those two (`read::corp_faction`, public) or when a piece of ICE it
/// has rezzed tags or damages (`read::corp_punishes_runs`), and only
/// while there is something unknown in the way: face-down ICE ahead, or
/// a card the breach would show for the first time
/// (`read::unknown_ahead`). Read at the run's initiation and nowhere
/// later (`read::last_click_run`), which is where the planner prices the
/// run as a leaf, so a run already under way is never worth jacking out
/// of for it. Stage 4 found the planner running on the last click 0.48
/// of its runs against the reference's 0.14, because "credit, credit,
/// credit, run" and "run, credit, credit, credit" price the same at the
/// leaf and the credits first make the run affordable; this is the term
/// the Stage 4 and 5 entries deferred to here.
///
/// **Zero, and that is the measurement.** At a click and what it buys
/// back (1.0) the same run one click earlier wins by more than the
/// jitter, and a run only the last credit made affordable is not made —
/// and that second decision is the one the bench says is wrong against
/// this Corp: on the planner's Runner legs against the reference, 384
/// games a seed, the term cost 0.02 and 0.03 of win share with every
/// other term on, 0.02 and 0.02 with the Runner's plans as levers alone,
/// and at 0.4 (a click alone) 0.00 and 0.03 — six legs, every one
/// negative. The runs it forbids are the poor Runner's, for which the
/// third credit is the run, and the reference Corp punishes too few of
/// them for the click kept to pay. The reading stays, at zero, like
/// `UNREZZED_THREAT_WEIGHT`: the guide's sentence wants a Corp that
/// punishes, and the record is what the flat fear of one is worth.
const LAST_CLICK_RUN_WEIGHT: f64 = 0.0;
/// Runner only, every plan: the grip is no larger than the damage one
/// access could deal (`read::damage_feared`: the most a known trap on
/// the table or face up in Archives would do, and against a Jinteki
/// Corp at least `TYPICAL_NET_DAMAGE`), subtracted — "Keep your grip
/// larger than the damage you could take. You flatline when you suffer
/// more damage than you have cards in grip, and an access to a trap can
/// hit you before you see it coming." The known trap in the breach is
/// priced exactly (`LETHAL_TRAP_WEIGHT`); this is the fear of the one
/// not yet seen, read off the identity, and it is paid on the table and
/// not on the run, so the draw comes before the run and a run under way
/// is not worth leaving for it. The Corp's `LETHAL_THREAT_WEIGHT`, at
/// the same size: a turn of the Runner's, and well under a point of
/// agenda, so a Runner at the fear draws one card before it runs and
/// does not stop running.
const FEARED_FLATLINE_WEIGHT: f64 = 3.0;
/// The net damage a face-down card against a Jinteki Corp is feared to
/// deal: Snare!'s three, the most the pool's traps deal without
/// advancement; Urtica Cipher's two plus its tokens is read exactly
/// once seen.
const TYPICAL_NET_DAMAGE: usize = 3;
/// Runner only, every plan: each agenda point a breach of HQ or R&D is
/// expected to reach, as the Runner can count it
/// (`read::agenda_points_expected`): R&D at the deck's density — the
/// points a deck its size must hold (CR 1.4.6) over its cards — and HQ
/// at what the Corp has drawn less what it has played, scored, lost and
/// shown, spread over the drawn cards the Runner has not seen. "HQ when
/// the Corp is holding cards without scoring, and the turn before you
/// expect them to score": a Corp that draws and does not score is
/// holding its agendas, and this is that sentence as arithmetic — the
/// mirror of the Corp's `RUN_STAKES_WEIGHT`, which counts the real
/// cards. Precept 9, which Stage 1 measured at 0.32 of HQ runs made into
/// a full hand with no term reading it. The Corp's affordability to
/// score is not a gate here, on purpose: an agenda the Corp cannot yet
/// afford to score is an agenda it holds *longer*, which is more reason
/// to run HQ, not less.
///
/// **Zero, and that is the measurement.** At a point (an R&D access at
/// the pool's density 0.4 on top of the flat 0.6, an HQ access into a
/// hand holding two agendas' worth 0.8) the term cost the planner's
/// Runner legs 0.03 and 0.03 of win share against the reference, 384
/// games a seed; at half a point, 0.01 and 0.03. By the Runner's
/// faction at half a point the Anarch decks gained 0.06 and 0.04 and
/// the Criminal and Shaper decks lost 0.01 / 0.08 and 0.02 / 0.06,
/// on both seeds, and the split is not explained: a stake counted once
/// a run rather than once an access played byte-identical games, so it
/// is not the multi-access cards those decks hold (a run event's extra
/// accesses resolve after the initiation window, past the leaf the
/// planner prices). The reading stays, at zero, with the record.
const RUNNER_STAKES_WEIGHT: f64 = 0.0;
/// Runner only, every plan: each ICE subtype the rig covers that the
/// Corp has never shown — no piece of it rezzed on the table, none once
/// rezzed and now face down, none face up in Archives
/// (`read::ice_shown`) — subtracted from the `BREAKER_COVERAGE_WEIGHT`
/// it earns, on the table and in hand alike (`install_delta`), and a
/// breaker for an unshown subtype is not saved for
/// (`breaker_savings_shortfall`). "Install breakers for the ICE the Corp
/// has actually rezzed rather than for ICE they might have. A breaker
/// that sits unused is memory and credits you could have spent
/// running." Half the coverage: Cleaver for a Barrier nobody has seen
/// is +0.8 rather than +2.3, still ahead of a credit and behind an
/// economy card at the guide's rate (Telework Contract's install is
/// +1.2), which is "economy first, then breakers"; the same Cleaver once
/// a Barrier is rezzed anywhere is +2.3, as it was. Precept 11.
const UNSHOWN_BREAKER_WEIGHT: f64 = 1.5;
/// Runner only, every plan: what each server's breach is worth at the
/// share its rezzed ICE shuts the rig out of it (`runner::shut_doors`),
/// subtracted — the Runner's reading of where the Corp's ICE stands, off
/// a run (Phase 5 §38), which is what orders Tāo Salonga's swaps.
///
/// **A tenth of the breach's own rate, and that is the measurement.** At
/// one the reading also pulled every Runner toward installs (+0.3 install
/// clicks a game, +0.1 breakers, 3 to 5 fewer credits summed over its
/// turn starts), and the Startup pass moved toward the Corp: +50 games to
/// −32 over three seeds of 90 (z +1.99; 0.544 / 0.578 / 0.511 → 0.656 /
/// 0.622 / 0.556). At a tenth it is +30 to −29 (z +0.13), and Tāo swaps as
/// often (135, 157, 120, 117 swaps in four pairings of 48 games at one;
/// 137, 146, 133, 104 at a tenth): a swap is taken on the sign of the
/// reading, and the median gain a swap was taken on, 0.44 at one, is still
/// forty times the planner's jitter at a tenth. A quarter measured as
/// close (0.567 / 0.544 / 0.544).
const SHUT_DOOR_WEIGHT: f64 = 0.1;
/// Runner only, `Plan::Pressure`: each fresh HQ access a breakable run
/// would make, on top of the hidden access — "they punish an exposed
/// HQ". A credit's worth, so a Criminal runs HQ (1.0 an access, plus the
/// stakes) over R&D (0.6, plus the density) whenever both are open, and
/// still runs the remote the Corp is scoring out of, which is worth two
/// tokens' more. The run-money events the chapter names (Account
/// Siphon, Transfer of Wealth) are priced by the line only as the HQ
/// run they start: their gain is at the breach, past the leaf, and a
/// reading of an access replacement is a later stage's if the report
/// says they are held.
const HQ_PRESSURE_WEIGHT: f64 = 0.4;
/// Runner only, `Plan::Dismantle`: each rezzed asset or upgrade on the
/// Corp's table, subtracted, on top of `OPPONENT_BOARD_WEIGHT`'s share of
/// what it is worth — "Anarchs treat the Corp's board as something to
/// dismantle. They trash what they access." Half a credit's worth over
/// a click: the balanced Runner trashes a 2[c] asset (+0.2) and not a
/// 3[c] one (−0.2); an Anarch trashes at 3[c] (+0.3) and stops at 4[c]
/// (−0.1), and the run that reaches a rezzed asset is worth starting for
/// it (`access_prospect`'s trash gain). The same reading Carnivore's and
/// Gourmand's access abilities are priced on.
const DISMANTLE_WEIGHT: f64 = 0.5;
/// Runner only, `Plan::Rig`: each R&D access beyond the first that a
/// breakable R&D run makes (The Maker's Eye's two, Conduit's counters),
/// on top of the hidden access, and each R&D access a card on the table
/// promises (`read::rd_accesses`: a fixed count, or the counters it
/// hosts) — "once it is complete their runs are cheap and exact — above
/// all on R&D." A credit's worth, so The Maker's Eye at 2[c] is +0.8 for
/// its accesses over the plain run and a counter on Conduit is worth
/// taking. Breakers that scale (Echelon, Unity, Principia) need no term:
/// `continuous::breaker_strength` reads the rig, so a scaling breaker
/// prices itself through the cheaper breaks it makes.
const RD_ACCESS_WEIGHT: f64 = 0.4;

const WIN_SCORE: f64 = 1000.0;
const AGENDA_POINT_WEIGHT: f64 = 20.0;
const OWN_CREDIT_WEIGHT: f64 = 0.4;
const OPPONENT_CREDIT_WEIGHT: f64 = 0.2;
const BAD_PUBLICITY_WEIGHT: f64 = 3.0;
const TAG_WEIGHT: f64 = 4.0;
/// Both chairs, every plan: each point of core damage the Runner has
/// taken, subtracted by the Runner and added by the Corp — "core damage
/// is permanent: each point lowers your maximum hand size for the rest of
/// the game" (CR 5.5.3b). The card it discards is the state's already;
/// this is the hand size, which nothing read before Phase 5 §36, so
/// Thule Subsea's "do 1 core damage unless they spend [click] and
/// 2[credit]" was a click and two credits against nothing and the Runner
/// took the damage 114–118 times in 48 games and paid 6–14. Above what
/// the guide prices a click and two credits at (1.2), so a Runner who can
/// pay does; under a point of agenda by far, so a click it buys that
/// steals one is still worth a point of it (Running Hot).
const CORE_DAMAGE_WEIGHT: f64 = 1.5;
const BOARD_PRESENCE_WEIGHT: f64 = 1.0;
const MEMORY_WEIGHT: f64 = 0.5;

/// A rezzed piece of ICE, on top of `BOARD_PRESENCE_WEIGHT`. Set so that
/// rezzing a mid-cost ICE during an approach (−`cost` × 0.4 credits,
/// +1.0 presence, +this, −`UNREZZED_INSTALL_WEIGHT`) comes out ahead of
/// passing: the rez delta is 1.4 − 0.4 × cost, so Palisade at 3 is +0.2
/// and a 6-cost bioroid is −1.0 and stays unrezzed until the Corp is
/// richer. Before this term a one-ply Corp rezzed only ICE costing ≤ 1.
/// Moved 1.0 → 1.4 in step with `UNREZZED_INSTALL_WEIGHT` 0.6 → 1.0 so
/// that delta did not change.
const REZZED_ICE_WEIGHT: f64 = 1.4;
/// A rezzed asset or upgrade, on top of `BOARD_PRESENCE_WEIGHT`. Without
/// it a rezzed non-ICE card was worth `BOARD_PRESENCE_WEIGHT −
/// UNREZZED_INSTALL_WEIGHT` = +0.4 minus its rez cost × 0.4, so nothing
/// costing 2 or more was ever rezzed: Nico Campaign and Manegarm Skunkworks
/// were installed 33 and 34 times in 96 heuristic-vs-heuristic games and
/// rezzed never. Rezzing is now 1.0 − 0.4 × cost — +0.2 for a 2-cost
/// asset — and anything above zero gets rezzed eventually, because a rez
/// costs no click and competes with `EndTurn` at 0 once the clicks are
/// spent.
const REZZED_ASSET_WEIGHT: f64 = 1.0;
/// An unrezzed install. Installing was worth exactly nothing to a one-ply
/// evaluator (the rezzed count did not move), so `GainCreditClick`'s flat
/// +0.4 always won and the heuristic Corp never installed ICE — which is
/// why heuristic-vs-random play never once produced an `IceEncountered`
/// event (ROADMAP Rules Audit §0). Raised 0.6 → 1.0 for `HQ_FLOOR`: a
/// draw below the floor has to beat a credit (`HQ_SHORTFALL_WEIGHT` >
/// 0.4), and installing *from* the floor then loses that same weight, so
/// at 0.6 the Corp would have stalled at the floor clicking for credits —
/// the rhythm it already had. At 1.0 an install from the floor is +0.5
/// and still wins. A welcome side effect: a second ICE on a server (1[c]
/// to install) is now +0.6 and happens, where it was +0.2 and never did;
/// a third at +0.2 still does not.
const UNREZZED_INSTALL_WEIGHT: f64 = 1.0;
/// One advancement token on an installed card, counted only up to the
/// card's `advancement_requirement`. Advancing costs a click and a credit
/// (−0.4) against `GainCreditClick`'s +0.4, so a token must be worth more
/// than 0.8 to be preferred; scoring at `AGENDA_POINT_WEIGHT` per point
/// still dominates once the requirement is met. Tokens past the
/// requirement are worth nothing *unless the agenda pays Dividends* —
/// see `AGENDA_COUNTER_WEIGHT` — so the Corp still scores rather than
/// piling on where piling on buys nothing.
const ADVANCEMENT_WEIGHT: f64 = 1.5;
/// One agenda counter, on a scored agenda or promised by an advancement
/// token past a Dividends agenda's requirement (`CardDefinition::
/// dividends` counters per excess token, `engine::score_agenda`).
///
/// **Why the term exists at all**: `puct@32` over the whole 192-matchup
/// pool advanced 3,270 times, scored the two Dividends agendas 47 times
/// and fired their trigger *zero* times (ROADMAP Phase 3 §1). Not a
/// horizon problem — a search evaluates its leaves with this same
/// function, so an over-advanced token worth zero at every depth is a
/// move no search budget can find.
///
/// **Why 2.0.** The decision it has to win is "score now" against
/// "advance once more, then score" — scoring costs no click, so both end
/// with the agenda scored and the second is one click and one credit
/// dearer. The counter therefore has to beat a credit-click (0.4) plus
/// the credit spent advancing (0.4), and to stay well under a point of
/// agenda (20.0) so that a Dividends agenda never looks better held than
/// taken. 2.0 is what a counter buys, conservatively: *Sericulture
/// Expansion* spends one for two advancement tokens (+3.0 at
/// `ADVANCEMENT_WEIGHT`), *Off the Books* for a searched card installed
/// free.
const AGENDA_COUNTER_WEIGHT: f64 = 2.0;
/// Each agenda point in Archives, to the Corp: an agenda it trashed or
/// discarded is one it can never score and one a single Archives run
/// steals. Nothing read it before Phase 5 §37, so a choice of which card
/// to trash or discard was the jitter's whenever one was an agenda: once
/// the planner took AU Co.'s "look at the top 3 cards of R&D. Trash 1 of
/// those cards and add the rest to HQ", the Corp trashed agendas 58 → 80
/// and 22 → 58 times in 48 games and scored 65 → 38. It was the Corp's
/// everywhere, not AU Co.'s: over a planner pass of the pool (192 games,
/// seed 1) the Corp trashed 68 agendas with no weight and 10 with this
/// one, and the planner's self-pairing moved +0.078 toward the Corp on
/// both seeds (z +2.65, +2.76) for it alone. Half a point: the Runner
/// still has to run Archives for it, and any weight at all decides those
/// choices; it is not meant to decide anything a point would.
const ARCHIVED_AGENDA_WEIGHT: f64 = 10.0;
/// One advancement token on an *ambush* — a card that is not an agenda
/// but whose own text reads its advancement tokens to deal damage
/// (*Clearinghouse*, *Urtica Cipher*).
///
/// **These were worth exactly zero**, the same shape of bug the Dividends
/// term above fixes: an ambush carries `advancement_requirement: Some(0)`,
/// so `advancement_tokens.min(required)` counted every token as nothing
/// and the Corp never advanced one. Clearinghouse's turn-start trigger
/// fired 274 times in a 192-game heuristic report and dealt no damage at
/// all, because its damage *is* its token count. Unlike Dividends this
/// needs no search to reach: there is no competing action that banks the
/// same value one ply later, so a one-ply Corp can see it.
///
/// 1.0 rather than `ADVANCEMENT_WEIGHT`: advancing costs a click and a
/// credit against `GainCreditClick` (0.8), so at 1.0 an ambush token is
/// worth taking when there is nothing better and never worth more than
/// an agenda token.
const AMBUSH_ADVANCEMENT_WEIGHT: f64 = 1.0;
/// How many of an ambush's tokens are counted. **A cap is what makes the
/// term safe**: an agenda self-limits — past its requirement the Corp
/// scores — while an ambush has no requirement to stop at, so an uncapped
/// weight above the credit-click would have the Corp advancing one
/// forever. Three is past the point where more damage stops being the
/// cheapest thing to buy with a click, and it is where the `Trap` profile
/// found its wins.
const AMBUSH_ADVANCEMENT_CAP: u32 = 3;
/// An installed, unrezzed non-agenda that punishes access with damage,
/// over and above `UNREZZED_INSTALL_WEIGHT`. **Zero by default, like
/// `opponent_grip_weight`**: the balanced Corp does not play for the
/// flatline, and an ambush it will not follow up on is just an install.
/// It exists for `Personality::Trap`, which ROADMAP Phase 3 §1 records as
/// not working *because this lever was missing* — the profile valued the
/// Runner's grip being thin without any way to recognise the cards that
/// would thin it.
const AMBUSH_WEIGHT: f64 = 0.0;
/// Each piece of ICE on a central server, up to `CENTRAL_ICE_CAP` on HQ
/// and R&D and one on Archives. See `fort_value`, which says why the three
/// fort terms exist; these are `glacier`'s values from Phase 5 §19, where
/// they took that profile 0.247 → 0.462.
///
/// **Every Corp profile carries them** (Phase 5 §23). They were
/// `glacier`'s alone, and the one-ply style matrix put `glacier` at 0.466
/// against 0.100–0.150 for every other Corp style, on every deck group and
/// against every Runner style — the random Runner (0.954 against
/// 0.76–0.85) and `puct@128` (0.596 against 0.15–0.23) too, so it was not
/// an exploit of the one-ply Runner. Added unchanged and alone to
/// `Balanced`, `trap` and `rush`, over the same 1,536 games a cell: 0.138
/// → 0.421, 0.150 → 0.427, 0.100 → 0.439. Where a fort goes is how a
/// Corp plays, not a style.
///
/// They replaced `LURE_ICE_WEIGHT`, one piece in front of an unseen lure
/// trap for a Corp with no fort term (§20): `fort_value` counts such a
/// remote as the fort, so with a fort term in every profile the lure
/// term could not fire.
const CENTRAL_ICE_WEIGHT: f64 = 2.0;
const CENTRAL_ICE_CAP: usize = 2;
/// Each piece of ICE, up to `FORT_CAP`, on the scoring remote. See
/// `CENTRAL_ICE_WEIGHT` and `fort_value`.
const FORT_WEIGHT: f64 = 1.5;
const FORT_CAP: usize = 2;
/// Subtracted, per installed agenda, for each piece short of `FORT_CAP`
/// in front of it: an agenda in a new remote is 0.8 − 2 × 5.0 at one ply,
/// where the credit click is 0.5, so it waits in HQ for the fort. The term
/// that carries the other two (§19: at 0.0 they are worth −0.039). See
/// `CENTRAL_ICE_WEIGHT`.
const EXPOSED_AGENDA_WEIGHT: f64 = 5.0;
/// Each hand trap (Snare!, Byte! — `is_hand_trap`) held in HQ. **A trap
/// that cannot be advanced lures nothing from a remote**: it sits there
/// with no tokens and no reason for the Runner to come, while in HQ and
/// R&D the Runner meets it while looking for agendas, which is what those
/// cards are for. On `main` every one of them was installed and then
/// rezzed (147 of 147, Phase 5 §20), because `ambush_weight` paid for any
/// face-down trap.
///
/// Small: it has to beat the bare `unrezzed_install_weight` an install
/// would pay (1.0, `glacier` 0.8) minus the card leaving HQ, and lose to
/// anything the Corp would rather do with the card.
const HELD_TRAP_WEIGHT: f64 = 1.5;
/// Each card the Runner's grip is short of `OPPONENT_GRIP_FLOOR`, to the
/// Corp. **This is the term the "no Corp damage term" note above says to
/// add once a lever exists, and the lever now exists**: a hand trap is a
/// paid interaction (`CorpPaysToApply`), so springing one *is* a Corp
/// action that makes the grip smaller. Without it the Corp weighed 4
/// credits against nothing and declined every time: over 216 games it
/// could afford to spring 0.11 a game and sprang **0** (Phase 5 §20).
///
/// A shortfall below a floor rather than a linear count, for the reason
/// that note gives: the value of non-lethal damage is at the bottom of
/// the grip, and a lethal hit needs no term because the state it makes is
/// `GameOver`. At 1.0 with a floor of 5, Byte!'s three net damage into a
/// grip of five is worth 3.0 against the 1.6 the 4 credits cost, and into
/// a grip of eight it is worth nothing — which is the right answer: that
/// Runner discards the difference anyway.
const OPPONENT_GRIP_SHORTFALL_WEIGHT: f64 = 1.0;
const OPPONENT_GRIP_FLOOR: usize = 5;
/// A trap turned face up, subtracted, on top of the rez undoing
/// everything a rezzed card is otherwise worth. **A trap is never rezzed:**
/// Urtica Cipher, Snare! and Byte! fire on access face down (the subject
/// of an event always hears it), so a rez does nothing but show the
/// Runner what to avoid. `REZZED_ASSET_WEIGHT` made every rez worth +1.0
/// at a rez cost of 0, and outside `Trap` (whose `ambush_weight` happened
/// to outweigh it) every trap was rezzed at the Corp's first window: 116
/// of 116 Urticas in the glacier deck, accessed 7 times, and 147 of 147
/// Byte!s (Phase 5 §20). Priced in the Corp's own board rather than in
/// `corp_install_value`, which is also the Runner's reading of a rezzed
/// Corp card: what the Runner gains from trashing a face-up trap is not
/// the Corp's preference to keep it hidden. Any positive value decides
/// the rez; 1.0 makes it lose to `EndTurn` by a clear margin rather than
/// by jitter.
const REVEALED_TRAP_WEIGHT: f64 = 1.0;
/// Each piece of ICE, up to two, protecting the server an installed
/// agenda sits in. Every install was worth the same flat
/// `UNREZZED_INSTALL_WEIGHT` wherever it went, so the one-ply Corp put an
/// agenda in a fresh naked remote as readily as behind ICE, and never
/// preferred to ICE the remote it was scoring out of. Read on the agenda,
/// not the ICE, so it does both jobs: installing an agenda into a
/// one-ICE remote is +0.5 over a naked one, and installing ICE in front of
/// an installed agenda is +0.5 on top of the install — enough for a
/// second ICE there (1[c]) to beat one elsewhere. Capped at two so the
/// Corp does not stack a fourth ICE on one remote instead of scoring.
const AGENDA_PROTECTION_WEIGHT: f64 = 0.5;
/// ICE beyond this many on an agenda's server earns nothing more.
const AGENDA_PROTECTION_CAP: usize = 2;
/// Each ICE subtype (Barrier, Code Gate, Sentry) the Runner has an
/// installed breaker for, counted once per subtype. A breaker's value is
/// entirely in the runs it enables, which a one-ply evaluator cannot see;
/// this is the static proxy. Cleaver (3[c], 1 MU) installs at
/// −1.2 − 0.5 + 1.0 + 3.0 = +2.3 against a click's +0.4; a second Barrier
/// breaker adds no coverage and is not installed. Before this term only a
/// 0-cost program ever cleared the bar (ROADMAP Phase 2 §5). Raised 2.0 →
/// 3.0 for Carmen (5[c]): at 2.0 her install was +0.5, under an open
/// run's +0.6, and from an at-floor grip (−`GRIP_SHORTFALL_WEIGHT`) it was
/// −0.2 — she was never installed in any heuristic seating. At 3.0 she is
/// +1.5, or +0.8 from the floor, and every breaker in the pool clears a
/// run from anywhere.
const BREAKER_COVERAGE_WEIGHT: f64 = 3.0;
/// A piece of ICE whose subtype the Runner's rig has **no breaker for**,
/// on top of whatever `corp_install_value` already pays for it. The exact
/// mirror of `BREAKER_COVERAGE_WEIGHT`, and it exists because the Corp
/// half of this evaluator could not see across the table at all: every
/// `state.runner` read in `evaluate_state_with` sat in the Runner arm, so
/// the Corp priced ICE by its rez cost (`REZZED_ICE_WEIGHT` − 0.4 × cost)
/// and by nothing else. Cheap ICE therefore always looked best, and cheap
/// ICE is exactly what a developed rig walks through: over 192
/// heuristic-vs-heuristic games the Runner completed **86% of 3,781 runs**
/// (HQ 0.912, R&D 0.924) and stole 615 agendas to the Corp's 158 scored.
///
/// Reads `rig_coverage` — the same three flags the Runner's own term
/// counts — so the two chairs agree on what "covered" means, and an AI
/// breaker shuts the term off on all three subtypes at once the way it
/// opens all three for the Runner. **Only the rig, never the grip**: an
/// installed program is public, so this is the Corp reading what it is
/// entitled to see. The mirror-image line is `UNREZZED_THREAT_WEIGHT`,
/// which is the *Runner* reading a face-down Corp card and is off at zero
/// for that reason.
///
/// **Rezzed ICE only**, which is an arithmetic rule rather than a
/// visibility one: paid on the face-down card as well, the term sits on
/// both sides of the rez and cancels out of the single decision it exists
/// to win. It was built that way first and measured before it was
/// reasoned about — `RezIce` 1,073 → 1,053, Corp wins 31 → 29 of 192, a
/// term that changed nothing.
///
/// **Why 1.2.** The decision it has to win is rezzing an expensive piece
/// of ICE the rig cannot break. That rez is worth 1.4 − 0.4 × cost today,
/// so a 6-cost bioroid sits at −1.0 and stays face-down forever however
/// well it would hold; at 1.2 it is +0.2 and gets rezzed, while a 3-cost
/// the rig already covers is unchanged. The ceiling is
/// `ADVANCEMENT_WEIGHT` (1.5): above that the Corp would rather build ICE
/// than advance the agenda behind it, which is the trade this term must
/// not make. **The value itself is not swept**: with this term and
/// `ETR_SUBROUTINE_WEIGHT` both in, every affordable rez already happens,
/// and `REZZED_ICE_WEIGHT` 1.4 → 100.0 produces byte-identical games — so
/// there is nothing left for a larger weight here to flip either. What
/// 1.2 has to be is inside the bracket above, and it is.
const UNBREAKABLE_ICE_WEIGHT: f64 = 1.2;
/// Each subroutine on a **rezzed** piece of ICE that can end the run
/// (`Effect::can_end_the_run`, the same recogniser `is_unrezzed_threat`
/// uses). The term that makes a rez worth paying for, and the reason it
/// has to exist is that `REZZED_ICE_WEIGHT − 0.4 × cost` is the wrong
/// *shape*: it falls with cost, so the Corp rezzed its weakest ICE and
/// left its best face-down. Over 192 heuristic-vs-heuristic games it
/// installed 1,201 ICE and rezzed 493, and the split ran exactly backwards
/// — Tithe (1[c], no ETR subroutine) installed 143 and rezzed 71, Pharos
/// (7[c], strength 5, two ETR) installed 43 and rezzed **11**, Brân 1.0
/// (6[c], strength 6, two ETR) installed 59 and rezzed **14**. A run met
/// half a piece of ICE: 2,050 `IceApproached` and 1,246 `IceEncountered`
/// across 3,781 runs, and 86% of those runs completed.
///
/// **Why ETR rather than every subroutine.** A subroutine that tags or
/// deals damage is worth something, but the quantity this evaluator is
/// short of is the one that *stops a run*, and pricing all subroutines
/// alike would pay Tithe's two (net 1[c] and 1 net damage) the same as
/// Pharos's two ETR. `PENDING_SUBROUTINE_WEIGHT` is the Runner's side of
/// this same fact and is likewise about what is still in the way.
///
/// **Why 1.0.** Rezzing becomes `1.4 − 0.4 × cost + this × etr`, which
/// has to be positive for every piece of ICE worth having and ordered
/// sensibly among them. At 1.0: Palisade (3[c], one ETR) +1.2, Brân
/// (6[c], two) +1.0, Tithe (1[c], none) +1.0, Pharos (7[c], two) +0.6 —
/// the cheap filler still rezzes when there is nothing better, and the
/// ICE that holds a server now rezzes at all, which it did not.
const ETR_SUBROUTINE_WEIGHT: f64 = 1.0;
/// Each card the breach would show the Runner for the first time this
/// turn (`hidden_accesses`), while the Runner is mid-run *and can afford
/// to break every rezzed ICE still ahead of it* (`run_is_breakable`).
/// `InitiateRun` costs a click and changes nothing a static evaluator can
/// see, so with no run term at all a one-ply Runner never ran — in 96
/// heuristic-Runner games, `RunInitiated` was 0 and 86 ended by the Corp
/// decking itself. The term was first unconditional, and +0.6 against
/// `GainCreditClick`'s +0.4 on every click meant the Runner ran into
/// rezzed ICE it could not break rather than saving for a breaker: 0.7
/// programs installed per game and 75 subroutines broken against 1,025
/// fired across 96 heuristic-vs-heuristic games (ROADMAP Phase 2 §5).
/// Gating on *affordability* rather than on owning a matching breaker is
/// deliberate — Cleaver at 0 credits breaks nothing — and only rezzed ICE
/// counts, because an unrezzed card's identity in a determinized sample
/// is a guess the real Runner cannot see and the Corp may never rez.
/// Jacking out still forfeits the term, so a breakable run is worth
/// finishing.
///
/// **Per hidden access rather than flat, since September 2026.** Flat,
/// the term paid 0.6 for a run on *any* server, and nothing else in the
/// evaluator read what the breach would find, so a one-ply Runner spent
/// nine clicks in ten running — 3,102 runs to 361 credit clicks over 96
/// heuristic-vs-heuristic games — and 538 of those runs were on Archives,
/// most of them into a pile it had already turned face-up. A person
/// playing the Corp watched it run a face-up Urtica Cipher three times
/// and flatline (ROADMAP Phase 7 §3). Now the run is worth this much per
/// card it has not yet seen this turn (`access_prospect`): an R&D, HQ or
/// unrezzed-remote run is worth exactly what it was, a known Archives is
/// worth nothing and loses to the credit, and a second run on the same
/// server this turn is worth nothing except on HQ, where every access is a
/// fresh random card. (The old `Aggressive` profile's 1.2 and `Cautious`'s
/// 0.4 kept their meaning, per card; both are gone with the reference.) The successful-run term below is unchanged: it is
/// the search's door term, sized by its own sweep, and it does not read
/// the server either.
const ACTIVE_RUN_WEIGHT: f64 = 0.6;
/// Each advancement token on an unrezzed card in the root of the server
/// the Runner is running, on top of `ACTIVE_RUN_WEIGHT` for the card
/// itself, and zero once the server has been run this turn. A face-down
/// card the Corp has advanced is the agenda about to score — or the
/// ambush a trap Corp wants run, which is Netrunner's mind game and not
/// this term's to resolve. At one token the run is 1.6 and already beats a
/// central's 0.6; at two it is 2.6, so the one-ply Runner goes where the
/// Corp is scoring before it goes anywhere else, which is the check a
/// person expects the Runner to make.
const ADVANCED_CARD_PROSPECT_WEIGHT: f64 = 1.0;
/// Each card the breach would access that the Runner *knows* punishes
/// access with damage (`punishes_access_with_damage`, the recogniser the
/// Corp's ambush term uses): face-up in Archives, or rezzed in the root.
/// Subtracted from the run's prospect. Sized above one hidden access
/// (0.6) plus a credit (0.4) together, so a face-down card in Archives
/// beside a face-up Urtica Cipher is not worth the run — the exact
/// position the person watched the bot lose from — while a run on a
/// server with two hidden cards and one known ambush is still marginally
/// worth it.
///
/// **Known is what the Runner has seen**: rezzed, or face down and
/// accessed before (`InstalledCard::seen_by_runner`), whose identity the
/// view carries and a determinized sample therefore keeps. A face-down
/// card never seen is still not read: in a sample its identity is a
/// guess. Before the flag a sprung Urtica Cipher went back to being a
/// hidden card worth 1.0 more per token, and the heuristic Runner ran
/// into one it had already hit 0.43 times per install once the Corp
/// stopped rezzing traps (Phase 5 §20). On top of this flat term the run
/// pays `KNOWN_TRAP_DAMAGE_WEIGHT` per point of damage.
const KNOWN_AMBUSH_WEIGHT: f64 = 1.5;
/// Each point of damage a known trap in the breach would do, subtracted
/// on top of `KNOWN_AMBUSH_WEIGHT`: an Urtica Cipher with three tokens is
/// five net damage, not the same 1.5 as a bare one. A trap that would
/// take more cards than the grip holds is a flatline, priced at
/// `LETHAL_TRAP_WEIGHT` instead. 0.5 a point: a card from the grip is
/// worth about a click, and two of them outweigh a hidden access.
const KNOWN_TRAP_DAMAGE_WEIGHT: f64 = 0.5;
/// A known trap whose damage would flatline the Runner: not a trade, the
/// game. Well short of `WIN_SCORE`, because this is a leaf's reading of a
/// run not yet over — a prevention in the grip or a card drawn first can
/// still change it — but far past anything the breach could pay.
const LETHAL_TRAP_WEIGHT: f64 = 50.0;
/// The Runner's view of the Corp's board: each Corp install, valued by
/// what the Runner can see of it (`visible_install_value`), subtracted at
/// this fraction. **The Runner branch had no term over the Corp's board
/// at all**, so `TrashAccessedCard` cost credits and gained nothing, and
/// no heuristic Runner ever paid to trash what it accessed: a rezzed
/// Nico Campaign could be run every click and left standing every time.
/// For a one-ply Runner the term's only differential effect is at
/// access — trashing a rezzed asset at 2[c] is 0.5 × 2.0 − 0.8 = +0.2
/// and taken, at 3[c] −0.2 and left — and the same arithmetic is what
/// `access_prospect` credits a run on a trashable card with, so the run
/// that ends the loop is worth starting. Half, not one: at the Corp's own
/// weights a rezzed asset is worth 2.0, and pricing its removal at that
/// would send the Runner across the table to spend 5[c] on a 4-cost
/// trash rather than on the breaker in its grip.
const OPPONENT_BOARD_WEIGHT: f64 = 0.5;
/// A run reached its server this turn
/// (`GameState::this_turn`, `rules::turn_log`): the run term, kept for
/// the rest of the turn, so that **breaching keeps it and only leaving
/// forfeits it.**
///
/// `ACTIVE_RUN_WEIGHT` alone is returned however the run ends, and that
/// made the approach-server step — rule 6.1.5's last chance to jack out —
/// a coin flip for a search that looks past the breach: `JackOut` and
/// `CompleteRun` both lose the 0.6 and differ only by what the breach
/// finds, which a single determinized sample resolves to one card. An
/// agenda one time in four, otherwise nothing or a trash prompt, so a
/// traced PUCT Runner read the two within 0.03 of each other and walked
/// away from the door 1,213 times in 192 games against the heuristic
/// Corp — which, seeing one ply, keeps the run term through the access
/// and left 25 times. (Since Rules Audit item 7 the door is the movement
/// phase before the server is approached, `JackOut` against
/// `ContinueRun`; there is no jack-out at the approach any more, and the
/// breach is a few plies further from it. The term is read the same way.) Two other shapes were measured first and rejected
/// (ROADMAP Phase 3 §1): valuing the breach honestly in the search, as a
/// chance node over redraws of the hidden cards
/// (`PuctConfig::breach_outcomes`), was *worse* at every fan-out
/// (0.339 / 0.328 / 0.307 at 2 / 4 / 8), because the registry pool it
/// draws from is a quarter agendas and the Runner went to the centrals;
/// and a prior on accesses made, read off `last_completed_run`, either
/// reset at every determinized root and rewarded churning open servers or,
/// carried, punished starting any run that might end early. This term
/// does neither: it is paid once a turn on a flag both players watched
/// get set, cannot be lost once earned, and does not follow the run's
/// contents — a stolen agenda is still `AGENDA_POINT_WEIGHT`'s.
///
/// **Sized by the sweep, not the decision**: PUCT Runner against the
/// heuristic Corp over 192 games, 0.411 without it, then 0.432 / 0.469 /
/// **0.516** / 0.490 at 0.6 / 1.2 / 2.0 / 3.0. Two is five credits'
/// worth: enough that the door is never close (jack-outs 1,213 → 575) and
/// that a run counts as a turn's work over clicking for credits
/// (2,387 → 1,722), and still a tenth of a point of agenda. Beyond it the
/// Runner starts leaving breakers uninstalled (`SubroutineBroken` 292 →
/// 236 at 3.0). Runner-only; the heuristic reads it too and moves inside
/// the seed band.
const SUCCESSFUL_RUN_WEIGHT: f64 = 2.0;
// Worth more than the run it came from, so the door is never close, and
// far under what the breach may find.
const _: () = assert!(SUCCESSFUL_RUN_WEIGHT > ACTIVE_RUN_WEIGHT && SUCCESSFUL_RUN_WEIGHT < AGENDA_POINT_WEIGHT / 5.0);
/// The Corp's counterpart, subtracted while the Runner is mid-run and can
/// afford to break every rezzed ICE still ahead of it. **The Corp branch
/// had no run term at all**: its score was identical whether the Runner
/// was one ICE from a remote holding a nearly-scored agenda or the board
/// was quiet, which is why `continuation_upside` could not price
/// `EndTheRun` and *Anoetic Void*'s "discard 2 from HQ to end the run" was
/// declined every time it was offered (ROADMAP Phase 3 §1).
///
/// Gated on `run_is_breakable`, the same predicate the Runner's term uses,
/// so the two sides agree on when a run is real — a run into ICE the
/// Runner cannot get through is not something the Corp should pay to stop.
///
/// **Why 1.5.** The decision it has to win is Anoetic Void's offer, and
/// that arithmetic is exact: accepting costs 2 credits (−0.8) and parks a
/// selection (−`UNRESOLVED_DECISION_WEIGHT`, −2.0), against declining's
/// zero, with `PENDING_DECISION_UPSIDE_WEIGHT` (+1.5) credited beside the
/// continuation once it prices above zero. So the term must clear about
/// 1.3 for a Corp with a healthy HQ to take the offer. It also has to stay
/// far under a point of agenda (20.0), so ending runs never competes with
/// scoring, and it lands at one advancement token — the Corp gives up
/// about one advance's worth of tempo to turn a live run away.
const ACTIVE_RUN_AGAINST_WEIGHT: f64 = 1.5;
/// Corp only: what holding a paid end-the-run is worth over paying for
/// it now, until the run is past its last paid-ability window
/// (`corp::answered_by_a_held_end_the_run`, Phase 5 §43). Waiting is
/// never worse — the Runner may yet be stopped by the ICE ahead or jack
/// out, and the ability is as good at the last window (CR 6.9.4e) as at
/// the first — so a run that threatens more than the price is charged
/// the price less this, and paying is the worse of the two by exactly
/// this much. Only an edge: it has to beat the planner's tie-break
/// jitter (1e-3) and nothing else, as `KEPT_CLICK_EDGE` does for a
/// click kept.
const HELD_END_THE_RUN_EDGE: f64 = 0.05;
// **There is no Corp damage term, and that is a measurement.**
//
// `opponent_grip_weight` — a value per card in the Runner's grip — was
// removed here rather than retuned. It had measured inert for
// `Personality::Trap`, the archetype built around it (460 games to 460),
// and the obvious diagnosis was its shape: linear priced five cards to
// four the same as two to one, when the whole value of non-lethal damage
// sits at the bottom of the grip. A *lethal* hit needs no term at all, as
// the resulting state is `GamePhase::GameOver` and already scores
// `WIN_SCORE`.
//
// So it was rebuilt as a shortfall below a floor, mirroring the Runner's
// own `GRIP_SHORTFALL_WEIGHT`, and measured over 480 heuristic-vs-heuristic
// games: **byte-identical to having no term at all, on all five seeds** —
// not one decision changed. At four times the weight, still nothing
// (ROADMAP Phase 3 §1).
//
// The shape was never the problem. A one-ply evaluator ranks *actions*,
// so a term only differentiates where an action changes the quantity it
// reads, and **no Corp action in the sample pool makes the Runner's grip
// smaller**. Neurospike, the one direct-damage Corp operation in the pool
// with no play requirement, deals damage equal to the agenda points
// scored *this turn* — zero in the general case, so no weight makes it
// worth 3 credits. Every other damage source in the pool (ambushes, ice
// subroutines, agenda-scored triggers) fires from a card's own text on
// access or on a trigger, never as the action being ranked. The Corp does
// not choose to deal this damage; the Runner walks into it.
//
// A damage term needs a lever before it needs a weight: a search deep
// enough to see the follow-up kill, or a card pool where dealing damage
// is a Corp action.
/// Each still-pending subroutine on the ICE the Runner is encountering.
/// Breaking a subroutine has no visible effect on the board, so without
/// this a Runner with a rig of breakers would never pay to use one and
/// would let every subroutine fire. At 1.0 a break costing up to two
/// credits is worth taking, and a Runner facing three unbroken
/// subroutines with no breaker prefers jacking out (+3.0 − 0.6) to walking
/// into them.
const PENDING_SUBROUTINE_WEIGHT: f64 = 1.0;
/// A decision `side` still owes — a parked card selection, choice or paid
/// choice. Toggling a card in and out of a selection changes nothing a
/// static evaluator scores, so a one-ply agent random-walked
/// `ToggleCardSelection` until `MAX_STEPS` ran out (view sweep, seed 82,
/// a Runner selection during access). With the parked state itself
/// costing something, `ConfirmCardSelection` is progress the moment it is
/// legal, and the toggles leading up to it are a short walk, not a
/// wander. **It must outweigh whatever confirming gives up**: at 0.5 it
/// exactly cancelled the `HQ_SHORTFALL_WEIGHT` of the card a Corp
/// selection from an at-floor HQ hands over, confirm tied with a toggle,
/// and the tie-break jitter wandered for 10,000 steps (heuristic Corp vs
/// random Runner, seed 77, Discretion Advised vs Planning Ahead). Set
/// well above the largest per-card term on either side (`GRIP_SHORTFALL_
/// WEIGHT` 0.7, `HQ_SHORTFALL_WEIGHT` 0.5, a rig card's 1.0 presence).
const UNRESOLVED_DECISION_WEIGHT: f64 = 2.0;
/// Credited to a parked `PendingDecision` whose continuation this
/// evaluator can already price, on top of that continuation's own
/// lower-bound value (`pending_decision_upside`). Without it the Corp
/// accepted **none of 332** paid choices in 192 heuristic-vs-heuristic
/// games, at every one of five seeds, and the arithmetic is why:
/// `AcceptPendingPaidChoice` pays a real cost and hands the payer a
/// `PromptChooseCards`, so the successor owes
/// `UNRESOLVED_DECISION_WEIGHT` while `DeclinePendingPaidChoice`
/// resolves to `Sequence []` and owes nothing. PT Untaian's "pay 1[c],
/// put an advancement token on an installed card" scored −0.4 − 2.0 =
/// −2.4 against declining's 0.0, and the token it buys is one ply the
/// other side of the prompt.
///
/// **Why an allowance on top of the priced continuation, and why it has
/// to stay under `UNRESOLVED_DECISION_WEIGHT`.** Writing `P` for that
/// penalty, `C` for the accept cost and `F` for what resolving really
/// delivers: accepting wins when `upside + this > P + C`, and the agent
/// still prefers *resolving* the prompt to sitting in it when
/// `F + P > upside + this`. The second is the toggle-walk the penalty
/// exists to prevent, and because `upside` is a **lower bound** on `F`
/// it holds for every card whenever this constant is below `P` — safe by
/// construction rather than by measurement, which is the whole reason
/// the upside is a bound and not an estimate. The first then flips any
/// accept whose priced upside beats `P + C − this` = 0.5 + `C`. A flat
/// bonus large enough to flip an accept on its own would have to exceed
/// `P` and would re-create the wander.
const PENDING_DECISION_UPSIDE_WEIGHT: f64 = 1.5;
/// Each point by which the encountered ICE's strength exceeds the best
/// matching breaker's. Pumping strength changes nothing else a static
/// evaluator sees, so a Runner holding Cleaver against a strength-4
/// Barrier never paid to pump and never got to break: once the free
/// break was deleted, heuristic-vs-heuristic broke 100 subroutines in 96
/// games and let 1,151 fire. At 0.9 a 2[c] pump (−0.8) closes one point
/// of shortfall and comes out ahead; a pump past the ICE's strength is
/// worth nothing.
const STRENGTH_SHORTFALL_WEIGHT: f64 = 0.9;

/// Each unrezzed ICE ahead of the Runner that the Corp can afford to rez
/// and no rig card can break at any price.
///
/// **Zero, and that is the measurement result rather than a placeholder.**
/// The term works — with it on, the Runner's static leaf stops being
/// blind to the sample (`diag leaf-sensitivity` at depth 0: root argmax
/// agreement across 16 determinizations 0.998 → 0.968, paired hidden-state
/// cost +0.002 n.s. → **+0.032, z = +4.49**). It is simply not worth
/// anything: over 192 games a pairing at 128 simulations against a fixed
/// heuristic Corp — fixed for real, since this term is Runner-arm only —
/// PUCT's Runner chair gains **+0.016 and +0.010** on two seeds at 0.6,
/// both inside Phase 3's 0.026–0.047 seed-spread band and both n.s., and
/// **loses 0.036 and 0.057** at 1.5. A one-ply Runner is hurt outright:
/// `heuristic` vs `heuristic` moves +0.057/+0.062 to the Corp at both
/// weights on both seeds. See ROADMAP Phase 2 §5 item 38 for why —
/// the term prices an unrezzed ICE as certain to be rezzed, and a real
/// Corp often declines.
///
/// This is the term ROADMAP "next" item 1 asked for. Item 36 showed the
/// Runner's leaf cannot read a determinization at all: every term reads
/// public counts, board state or the Runner's own zones, and
/// `run_is_breakable` says outright that unrezzed ICE is treated as
/// passable. So PUCT at four samples averages four identical numbers,
/// and the hidden information the Runner most needs to integrate over —
/// what is behind the ICE — is the one thing its evaluator declines to
/// look at.
///
/// **A weighted term rather than a flip of `run_is_breakable`'s bool.**
/// Folding unrezzed ICE into that predicate would gate `active_run_weight`
/// on it, which is all-or-nothing and untunable; a term beside
/// `strength_shortfall` can be measured at several strengths and set to
/// zero if it loses. It counts unbreakable ICE rather than pricing the
/// shortfall in credits because an ICE no rig card can break is a
/// different thing from an expensive one — `strength_shortfall` already
/// prices the expensive case, for the ICE actually being encountered.
///
/// Depends on ROADMAP Phase 2 §5 item 37: until that landed, every
/// unrezzed ICE in a sample was a `Barrier` with no subroutines, so
/// `cheapest_break_cost` returned `Some(0)` for all of them and this
/// term would have counted zero however the ICE was set.
///
/// Item 39 then narrowed what it counts to ICE with a subroutine that
/// ends the run — see `is_unrezzed_threat`. The weight below is the
/// term's strength *after* that narrowing, and the two are not
/// comparable: the same number prices about half as many ICE as it did
/// at item 38's measurement.
const UNREZZED_THREAT_WEIGHT: f64 = 0.0;
/// Each credit the Runner is short of the cheapest breaker in grip that
/// would cover an ICE subtype the rig does not (`breaker_savings_shortfall`).
/// A one-ply evaluator cannot see the install it is saving for, and once
/// the run term stopped paying for runs into unbreakable ICE the Runner
/// simply ran open servers instead (+0.6 beats a credit's +0.4 every
/// click): `ProgramInstalled` stayed at 66 across 96 heuristic-vs-heuristic
/// games. This is a penalty on the *shortfall*, not a bonus on credits
/// held, and the difference is the whole design: a bonus would vanish the
/// moment the breaker is installed and fight the install it exists to
/// enable (Cleaver's +1.3 would lose 3 × the weight). The shortfall is zero
/// once the card is affordable, so installing keeps its full margin, while
/// every click that closes the gap is worth 0.4 + 0.3 = +0.7 — ahead of an
/// open-server run — and every credit spent elsewhere while short costs
/// 0.3 more. (Carmen's install is +0.5, already below a run at +0.6; a
/// pre-existing weight interaction this term does not touch.)
const SAVINGS_SHORTFALL_WEIGHT: f64 = 0.3;
/// Each card the Runner's grip is below `GRIP_FLOOR`. A card in hand was
/// worth nothing to the evaluator, so `DrawCardClick` never beat anything
/// and the heuristic Runner clicked it once in 96 games: its grip was the
/// opening hand plus event draws, which is what capped program installs
/// at 66 per 96 games after the run and savings terms — a breaker in the
/// opening hand was installed on turn one, and no other breaker was ever
/// drawn (ROADMAP Phase 2 §5). Shaped as a shortfall below a floor rather
/// than a value per card so it does not tax installs from a healthy hand:
/// below the floor a draw is +0.7, ahead of an open-server run's +0.6 and
/// a credit's +0.4, and installing from a 3-card grip loses 0.7 of
/// Cleaver's +1.3 — so the Runner draws one card first, then installs at
/// full margin; at or above the floor the term is flat and runs resume.
/// The floor is below the hand limit so a draw never has to be discarded.
/// Runner-only: the Corp's mandatory draw hides the same blindness, and
/// what a heuristic Corp should hold is a different question.
const GRIP_SHORTFALL_WEIGHT: f64 = 0.7;
/// Cards in grip below which `GRIP_SHORTFALL_WEIGHT` applies.
const GRIP_FLOOR: usize = 3;
/// Each card in the Runner's grip, at this fraction of what installing it
/// would be worth to this same evaluator (`install_delta`: presence, new
/// breaker coverage, minus its cost in credits), and nothing for a card
/// whose install would be worth nothing. **A card in hand was worth
/// exactly zero above the floor**, so from a five-card opening hand the
/// Runner installed what it could, settled at three cards and never drew
/// again — 126 draw clicks in 96 heuristic-vs-heuristic games, most of
/// them refilling after damage — and "never expands beyond its opening
/// hand" was a person's verdict on it (ROADMAP Phase 7 §3).
///
/// **A fraction of the install's own value, not a flat value per card.**
/// A flat value is the shape `GRIP_SHORTFALL_WEIGHT` deliberately avoids:
/// it taxes every install by that amount and a cheap card stops being
/// worth putting on the table. At a fraction, installing a live card is
/// still worth the other half of its delta, and a dead card — a second
/// Barrier breaker, a resource whose cost outweighs its presence — is
/// worth nothing held and nothing drawn. What that makes a draw worth is
/// half the *sampled* top card: the one-ply agent applies `DrawCardClick`
/// to its determinized stack, so a breaker for a subtype the rig cannot
/// break (delta 2.8 at 3[c]) draws at +1.4 against a credit's 0.4, and a
/// 2-cost resource (+0.2) does not. In effect the Runner draws at the
/// rate its stack holds cards it would install, which is the honest
/// answer to "should I draw" for a Runner that reads no further than
/// that. Events are worth nothing here: the one-ply already plays an
/// affordable event when its effect scores, and pricing one unplayed is
/// a card-reading term of its own.
const HELD_CARD_WEIGHT: f64 = 0.5;
/// The Corp's `GRIP_SHORTFALL_WEIGHT`: each card HQ is below `HQ_FLOOR`.
/// The heuristic Corp never clicked to draw (0 in every heuristic
/// seating) and deployed about one card a turn — the mandatory draw —
/// spending the rest of its clicks on credits. Below the floor a draw is
/// +0.5 against a credit's +0.4; an install from the floor is
/// `UNREZZED_INSTALL_WEIGHT` − 0.5 = +0.5 (see that constant for why it
/// had to rise first), so the Corp alternates draw and install rather
/// than stalling; at the floor a draw is worth 0 and the clicks go to
/// installs and advancement.
const HQ_SHORTFALL_WEIGHT: f64 = 0.5;
/// Cards in HQ below which `HQ_SHORTFALL_WEIGHT` applies. Three, the
/// same floor as the Runner's `GRIP_FLOOR`, and measured there: the
/// stall that set `UNRESOLVED_DECISION_WEIGHT` was found at this floor
/// (heuristic Corp vs random Runner, seed 77), so the pair was tuned
/// together. The pull the other way is real — the mandatory draw
/// already feeds HQ every turn and every card held there is one more an
/// HQ run can steal — which is why the floor sits at three rather than
/// higher.
const HQ_FLOOR: usize = 3;
/// R&D size below which the HQ term switches off. A Corp that draws its
/// deck out loses at the next mandatory draw, and a one-ply evaluator
/// sees that only one action too late. R&D's size is public, so the brake
/// reads nothing the Corp's `ClientView` does not show.
///
/// **There is deliberately no Corp `HELD_CARD_WEIGHT` above this floor,
/// and it was measured rather than assumed.** The symmetry argument is
/// inviting — the term that fixed the Runner chair in September 2026 was
/// exactly this, and the heuristic Corp clicks to draw 1.1 times a game
/// against the Runner's 3.6 — but it is wrong about what the Corp is
/// short of. Tried at 0.42 and 0.48 over 384 games a point — separate
/// binaries, checked by hash — both produced the *same* 1,623 draws
/// against a baseline 425 and the same 0.083 win rate against 0.182:
/// above `OWN_CREDIT_WEIGHT` the term is a switch, not a dial, and 0.5
/// (on the tie with installing) was worse again at 0.062. Installs did not move at all (4,959 → 4,961),
/// so the extra cards did not reach the table; the clicks came out of
/// `GainCreditClick`, rezzes fell 2,156 → 1,583 and `IceApproached` 3,703
/// → 3,319. The Corp was never card-starved — it installs 13 cards a game
/// off 11.4 turns — it is credit-starved, and cannot pay to rez the ICE
/// it has already installed. A card in HQ that the Corp cannot afford to
/// turn face up is worth less than the credit that would have paid for
/// one.
const RD_DRAW_RESERVE: usize = 5;

/// Every tunable term of `evaluate_state`, as one value. `Default` is the
/// constants above, so `evaluate_state` is `evaluate_state_with(..,
/// &Weights::default())` and every existing caller scores exactly as it
/// did; a `Personality` is a `Weights` that differs from the default in a
/// few documented places. The constants keep the reasoning — each one's
/// doc comment records the measurement that set it — and the struct
/// carries the numbers, so a profile can move one without restating the
/// rest. Three terms exist only for profiles or for a measurement and
/// default to zero, so the balanced evaluator is unchanged by their
/// existence: `ambush_weight` and `installed_agenda_weight` (a Corp that
/// plays for the flatline, and one that wants the agenda on the table
/// before the ICE in front of it), and `unrezzed_threat_weight` (Phase 2
/// §5 item 40's leaf window onto hidden state, measured and shipped off).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub agenda_point_weight: f64,
    pub own_credit_weight: f64,
    pub opponent_credit_weight: f64,
    pub bad_publicity_weight: f64,
    pub tag_weight: f64,
    pub core_damage_weight: f64,
    pub board_presence_weight: f64,
    pub memory_weight: f64,
    pub rezzed_ice_weight: f64,
    pub rezzed_asset_weight: f64,
    pub unrezzed_install_weight: f64,
    pub advancement_weight: f64,
    pub agenda_counter_weight: f64,
    pub archived_agenda_weight: f64,
    pub ambush_advancement_weight: f64,
    pub ambush_advancement_cap: u32,
    pub ambush_weight: f64,
    pub agenda_protection_weight: f64,
    pub agenda_protection_cap: usize,
    pub breaker_coverage_weight: f64,
    /// Corp only: each installed ICE whose subtype the rig cannot break.
    /// See `UNBREAKABLE_ICE_WEIGHT`.
    pub unbreakable_ice_weight: f64,
    /// Corp only: each run-ending subroutine on a rezzed piece of ICE.
    /// See `ETR_SUBROUTINE_WEIGHT`.
    pub etr_subroutine_weight: f64,
    /// Runner only: each card the current run would show the Runner for
    /// the first time this turn. See `ACTIVE_RUN_WEIGHT`.
    pub active_run_weight: f64,
    /// Runner only: each advancement token on an unrezzed card in the
    /// run's target root. See `ADVANCED_CARD_PROSPECT_WEIGHT`.
    pub advanced_card_prospect_weight: f64,
    /// Runner only: each known ambush the run would access, subtracted.
    /// See `KNOWN_AMBUSH_WEIGHT`.
    pub known_ambush_weight: f64,
    /// Corp only: each hand trap held in HQ. See `HELD_TRAP_WEIGHT`.
    pub held_trap_weight: f64,
    /// Corp only: each card the Runner's grip is short of
    /// `opponent_grip_floor`. See `OPPONENT_GRIP_SHORTFALL_WEIGHT`.
    pub opponent_grip_shortfall_weight: f64,
    pub opponent_grip_floor: usize,
    /// Runner only: each point of damage those known traps would do. See
    /// `KNOWN_TRAP_DAMAGE_WEIGHT`.
    pub known_trap_damage_weight: f64,
    /// Runner only: a known trap in the breach that would flatline. See
    /// `LETHAL_TRAP_WEIGHT`.
    pub lethal_trap_weight: f64,
    /// Corp only: each of its own traps face up, subtracted, beyond the
    /// rez gaining nothing. See `REVEALED_TRAP_WEIGHT`.
    pub revealed_trap_weight: f64,
    /// Runner only: the Corp's board, as the Runner sees it, subtracted at
    /// this fraction. See `OPPONENT_BOARD_WEIGHT`.
    pub opponent_board_weight: f64,
    /// Runner only: a run reached its server this turn. See
    /// `SUCCESSFUL_RUN_WEIGHT`.
    pub successful_run_weight: f64,
    pub pending_subroutine_weight: f64,
    pub unresolved_decision_weight: f64,
    pub pending_decision_upside_weight: f64,
    pub strength_shortfall_weight: f64,
    /// Runner only: unbreakable unrezzed ICE ahead, subtracted. See
    /// `UNREZZED_THREAT_WEIGHT` — this is the only term in the evaluator
    /// that reads a determinized card, so it is also the only one a
    /// search's extra hidden-state samples can disagree about.
    pub unrezzed_threat_weight: f64,
    pub savings_shortfall_weight: f64,
    pub grip_shortfall_weight: f64,
    pub grip_floor: usize,
    /// Runner only: each grip card at this fraction of its install value.
    /// See `HELD_CARD_WEIGHT`.
    pub held_card_weight: f64,
    pub hq_shortfall_weight: f64,
    pub hq_floor: usize,
    pub rd_draw_reserve: usize,
    /// Corp only: the Runner is mid-run and can break what is left of it.
    /// See `ACTIVE_RUN_AGAINST_WEIGHT`.
    pub active_run_against_weight: f64,
    /// Corp only: each installed, unscored agenda, added. Zero by default
    /// — every unrezzed install is worth the same flat
    /// `unrezzed_install_weight`, which is why the balanced Corp has no
    /// preference between putting down the agenda and the ICE — and
    /// positive for a `Personality::Rush`, for whom the agenda comes
    /// first and the ICE if there is time. Measured before it existed:
    /// a rush profile built from the other terms alone advanced *less*
    /// than balanced (1,045 → 1,032 `CardAdvanced` in 96 games), because
    /// the one-ply Corp already advances every agenda it has installed
    /// and the profile had no way to say "install it".
    pub installed_agenda_weight: f64,
    /// Corp only: each piece of ICE on a central server, up to
    /// `central_ice_cap` on HQ and R&D and one on Archives. See
    /// `CENTRAL_ICE_WEIGHT`.
    pub central_ice_weight: f64,
    pub central_ice_cap: usize,
    /// Corp only: each piece of ICE, up to `fort_cap`, on the Corp's
    /// deepest remote whose root is empty or holds an agenda — the
    /// scoring remote, priced before there is an agenda to put in it.
    /// See `FORT_WEIGHT`.
    pub fort_weight: f64,
    pub fort_cap: usize,
    /// Corp only: subtracted, per installed agenda, for each piece of ICE
    /// short of `fort_cap` in front of it. See `EXPOSED_AGENDA_WEIGHT`.
    pub exposed_agenda_weight: f64,
    /// Each click the side has left this turn. See `CLICK_WEIGHT`. Zero
    /// by default, with the six below: the guide's-rate terms, set by
    /// `at_the_guides_rate` for the planner.
    pub click_weight: f64,
    /// Each future credit an economy card declares. See
    /// `FUTURE_CREDIT_WEIGHT`.
    pub future_credit_weight: f64,
    /// A card in hand at this fraction of its declared economy. See
    /// `DECLARED_VALUE_WEIGHT`.
    pub declared_value_weight: f64,
    /// Corp only: each credit short of the rez reserve. See
    /// `REZ_RESERVE_WEIGHT`.
    pub rez_reserve_weight: f64,
    /// Corp only: each unrezzed piece of ICE it cannot afford to rez,
    /// subtracted. See `UNAFFORDABLE_ICE_WEIGHT`.
    pub unaffordable_ice_weight: f64,
    /// Corp only: each installed agenda under the taxing window. See
    /// `TAXING_WINDOW_WEIGHT`.
    pub taxing_window_weight: f64,
    /// Runner only: each credit the Corp would spend rezzing ahead of the
    /// run. See `FORCED_REZ_WEIGHT`.
    pub forced_rez_weight: f64,
    /// Runner only: a face-down root card the Corp can afford to finish.
    /// See `FINISHABLE_INSTALL_WEIGHT`.
    pub finishable_install_weight: f64,
    /// Corp only: each agenda point a breakable run would reach,
    /// subtracted. See `RUN_STAKES_WEIGHT`. Zero by default, with the
    /// seven below: the plans' terms (Stage 6), set by `with_plans` for
    /// the planner.
    pub run_stakes_weight: f64,
    /// Corp only: each affordable face-down ICE, kept face down. See
    /// `REZ_HELD_WEIGHT`.
    pub rez_held_weight: f64,
    /// Corp only: each taxing piece outside a stopping one, subtracted.
    /// See `ICE_ORDER_WEIGHT`.
    pub ice_order_weight: f64,
    /// Corp only: each installed agenda the Corp can finish next turn,
    /// and each it cannot, subtracted. See `NEVER_ADVANCE_WEIGHT`.
    pub never_advance_weight: f64,
    /// Corp only, the kill plan: the Runner's grip is under the damage
    /// the Corp holds. See `LETHAL_THREAT_WEIGHT`.
    pub lethal_threat_weight: f64,
    /// Corp only, the kill plan: each tag the Corp holds a punishment
    /// for. See `TAG_LEVERAGE_WEIGHT`.
    pub tag_leverage_weight: f64,
    /// Corp only, the traps plan: a face-down non-agenda in the fort.
    /// See `BLUFF_WEIGHT`.
    pub bluff_weight: f64,
    /// Corp only, "glacier then fast advance": the fort terms fall away
    /// once the Runner's rig breaks every subtype of ICE and their
    /// credits cover the break into the fort (`read::fort_beaten`). A condition inside
    /// the fort terms, never a switch between weight sets: with it off
    /// the fort is priced all game, which is glacier alone.
    pub fort_until_beaten: bool,
    /// Runner only: a run begun on the last click against a Corp that
    /// punishes runs, subtracted. See `LAST_CLICK_RUN_WEIGHT`. Zero by
    /// default, with the six below: the Runner's plan terms (Stage 7),
    /// set by `with_plans` for a planner Runner seat.
    pub last_click_run_weight: f64,
    /// Runner only: the grip is no larger than the damage one access is
    /// feared to deal, subtracted. See `FEARED_FLATLINE_WEIGHT`.
    pub feared_flatline_weight: f64,
    /// Runner only: each agenda point a central breach is expected to
    /// reach. See `RUNNER_STAKES_WEIGHT`.
    pub runner_stakes_weight: f64,
    /// Runner only: each covered subtype the Corp has never shown,
    /// subtracted from its coverage. See `UNSHOWN_BREAKER_WEIGHT`.
    pub unshown_breaker_weight: f64,
    /// Runner only: each server's breach at the share its rezzed ICE
    /// shuts the rig out, subtracted. See `SHUT_DOOR_WEIGHT`.
    pub shut_door_weight: f64,
    /// Runner only, the pressure plan: each fresh HQ access. See
    /// `HQ_PRESSURE_WEIGHT`.
    pub hq_pressure_weight: f64,
    /// Runner only, the dismantle plan: each rezzed Corp asset or
    /// upgrade, subtracted. See `DISMANTLE_WEIGHT`.
    pub dismantle_weight: f64,
    /// Runner only, the rig plan: each R&D access beyond the first, made
    /// or promised. See `RD_ACCESS_WEIGHT`.
    pub rd_access_weight: f64,
}

impl Weights {
    /// These weights for a planner seat on `side`, with the plans `style`
    /// stacks switched on (Phase 5 §25 Stage 6): every Corp's four (the
    /// stakes, the rez held, the ICE order, the never-advance line) for
    /// any Corp seat, balanced included; the kill plan's lethal check and
    /// tag leverage, the traps plan's bluff, and the fort's yielding when
    /// glacier is followed by fast advance; and every Runner's five (the
    /// last click, the feared flatline, the stakes as the Runner counts
    /// them, the unshown breaker, the doors the Corp's ICE shuts — §38)
    /// for any Runner seat, with the pressure
    /// plan's HQ, the dismantle plan's trash and the rig plan's R&D
    /// (Stage 7). **A plan after the first adds its own levers** — the
    /// terms that are zero in the default and exist for it: `Traps`'
    /// ambush terms, `FastAdvance`'s installed agenda, and each Runner
    /// plan's one term — and not its profile's dials on the shared
    /// terms, which are the first plan's (`Style::weights`). What
    /// `Style::planned_weights` gives the planner; the reference never
    /// calls this.
    pub fn with_plans(self, side: Side, style: &crate::plans::Style) -> Self {
        use crate::plans::Plan;
        let mut w = self;
        match side {
            Side::Corp => {
                w.run_stakes_weight = RUN_STAKES_WEIGHT;
                w.rez_held_weight = REZ_HELD_WEIGHT;
                w.ice_order_weight = ICE_ORDER_WEIGHT;
                w.never_advance_weight = NEVER_ADVANCE_WEIGHT;
                w.tag_leverage_weight = TAG_LEVERAGE_WEIGHT;
            }
            Side::Runner => {
                w.last_click_run_weight = LAST_CLICK_RUN_WEIGHT;
                w.feared_flatline_weight = FEARED_FLATLINE_WEIGHT;
                w.runner_stakes_weight = RUNNER_STAKES_WEIGHT;
                w.unshown_breaker_weight = UNSHOWN_BREAKER_WEIGHT;
                w.shut_door_weight = SHUT_DOOR_WEIGHT;
            }
        }
        for plan in style.plans().skip(1) {
            let own = plan.weights();
            match plan {
                Plan::Traps => {
                    w.ambush_weight = own.ambush_weight;
                    w.ambush_advancement_weight = own.ambush_advancement_weight;
                    w.ambush_advancement_cap = own.ambush_advancement_cap;
                }
                Plan::FastAdvance => w.installed_agenda_weight = own.installed_agenda_weight,
                Plan::Glacier | Plan::Kill | Plan::Dismantle | Plan::Pressure | Plan::Rig => {}
            }
        }
        if style.has(Plan::Kill) {
            w.lethal_threat_weight = LETHAL_THREAT_WEIGHT;
        }
        if style.has(Plan::Traps) {
            w.bluff_weight = BLUFF_WEIGHT;
        }
        if style.has(Plan::Glacier) && style.has(Plan::FastAdvance) {
            w.fort_until_beaten = true;
        }
        if style.has(Plan::Pressure) {
            w.hq_pressure_weight = HQ_PRESSURE_WEIGHT;
        }
        if style.has(Plan::Dismantle) {
            w.dismantle_weight = DISMANTLE_WEIGHT;
        }
        if style.has(Plan::Rig) {
            w.rd_access_weight = RD_ACCESS_WEIGHT;
        }
        w
    }

    /// These weights with the economy terms at the guide's rate (Phase 5
    /// §25 Stage 5): the click, the future credit, the card in hand, the
    /// rez reserve, the unaffordable ICE, the taxing window, the forced
    /// rez and the finishable install. What `planner::PlanningAgent` scores with over any
    /// personality; the one-ply reference keeps the default, so the
    /// planner is measured against a chooser that has not moved.
    pub fn at_the_guides_rate(self) -> Self {
        Weights {
            click_weight: CLICK_WEIGHT,
            future_credit_weight: FUTURE_CREDIT_WEIGHT,
            declared_value_weight: DECLARED_VALUE_WEIGHT,
            rez_reserve_weight: REZ_RESERVE_WEIGHT,
            unaffordable_ice_weight: UNAFFORDABLE_ICE_WEIGHT,
            taxing_window_weight: TAXING_WINDOW_WEIGHT,
            forced_rez_weight: FORCED_REZ_WEIGHT,
            finishable_install_weight: FINISHABLE_INSTALL_WEIGHT,
            ..self
        }
    }
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            agenda_point_weight: AGENDA_POINT_WEIGHT,
            own_credit_weight: OWN_CREDIT_WEIGHT,
            opponent_credit_weight: OPPONENT_CREDIT_WEIGHT,
            bad_publicity_weight: BAD_PUBLICITY_WEIGHT,
            tag_weight: TAG_WEIGHT,
            core_damage_weight: CORE_DAMAGE_WEIGHT,
            board_presence_weight: BOARD_PRESENCE_WEIGHT,
            memory_weight: MEMORY_WEIGHT,
            rezzed_ice_weight: REZZED_ICE_WEIGHT,
            rezzed_asset_weight: REZZED_ASSET_WEIGHT,
            unrezzed_install_weight: UNREZZED_INSTALL_WEIGHT,
            advancement_weight: ADVANCEMENT_WEIGHT,
            agenda_counter_weight: AGENDA_COUNTER_WEIGHT,
            archived_agenda_weight: ARCHIVED_AGENDA_WEIGHT,
            ambush_advancement_weight: AMBUSH_ADVANCEMENT_WEIGHT,
            ambush_advancement_cap: AMBUSH_ADVANCEMENT_CAP,
            ambush_weight: AMBUSH_WEIGHT,
            agenda_protection_weight: AGENDA_PROTECTION_WEIGHT,
            agenda_protection_cap: AGENDA_PROTECTION_CAP,
            breaker_coverage_weight: BREAKER_COVERAGE_WEIGHT,
            unbreakable_ice_weight: UNBREAKABLE_ICE_WEIGHT,
            etr_subroutine_weight: ETR_SUBROUTINE_WEIGHT,
            active_run_weight: ACTIVE_RUN_WEIGHT,
            advanced_card_prospect_weight: ADVANCED_CARD_PROSPECT_WEIGHT,
            known_ambush_weight: KNOWN_AMBUSH_WEIGHT,
            held_trap_weight: HELD_TRAP_WEIGHT,
            opponent_grip_shortfall_weight: OPPONENT_GRIP_SHORTFALL_WEIGHT,
            opponent_grip_floor: OPPONENT_GRIP_FLOOR,
            known_trap_damage_weight: KNOWN_TRAP_DAMAGE_WEIGHT,
            lethal_trap_weight: LETHAL_TRAP_WEIGHT,
            revealed_trap_weight: REVEALED_TRAP_WEIGHT,
            opponent_board_weight: OPPONENT_BOARD_WEIGHT,
            successful_run_weight: SUCCESSFUL_RUN_WEIGHT,
            pending_subroutine_weight: PENDING_SUBROUTINE_WEIGHT,
            unresolved_decision_weight: UNRESOLVED_DECISION_WEIGHT,
            pending_decision_upside_weight: PENDING_DECISION_UPSIDE_WEIGHT,
            strength_shortfall_weight: STRENGTH_SHORTFALL_WEIGHT,
            unrezzed_threat_weight: UNREZZED_THREAT_WEIGHT,
            savings_shortfall_weight: SAVINGS_SHORTFALL_WEIGHT,
            grip_shortfall_weight: GRIP_SHORTFALL_WEIGHT,
            grip_floor: GRIP_FLOOR,
            held_card_weight: HELD_CARD_WEIGHT,
            hq_shortfall_weight: HQ_SHORTFALL_WEIGHT,
            hq_floor: HQ_FLOOR,
            rd_draw_reserve: RD_DRAW_RESERVE,
            active_run_against_weight: ACTIVE_RUN_AGAINST_WEIGHT,
            installed_agenda_weight: 0.0,
            central_ice_weight: CENTRAL_ICE_WEIGHT,
            central_ice_cap: CENTRAL_ICE_CAP,
            fort_weight: FORT_WEIGHT,
            fort_cap: FORT_CAP,
            exposed_agenda_weight: EXPOSED_AGENDA_WEIGHT,
            click_weight: 0.0,
            future_credit_weight: 0.0,
            declared_value_weight: 0.0,
            rez_reserve_weight: 0.0,
            unaffordable_ice_weight: 0.0,
            taxing_window_weight: 0.0,
            forced_rez_weight: 0.0,
            finishable_install_weight: 0.0,
            run_stakes_weight: 0.0,
            rez_held_weight: 0.0,
            ice_order_weight: 0.0,
            never_advance_weight: 0.0,
            lethal_threat_weight: 0.0,
            tag_leverage_weight: 0.0,
            bluff_weight: 0.0,
            fort_until_beaten: false,
            last_click_run_weight: 0.0,
            feared_flatline_weight: 0.0,
            runner_stakes_weight: 0.0,
            unshown_breaker_weight: 0.0,
            shut_door_weight: 0.0,
            hq_pressure_weight: 0.0,
            dismantle_weight: 0.0,
            rd_access_weight: 0.0,
        }
    }
}

/// A rough static evaluation of `state` from `side`'s perspective: positive
/// favors `side`, negative favors the opponent. Shared by `PlanningAgent`'s
/// line and one-ply scoring, `MctsAgent`'s rollout/leaf evaluation, the uniform
/// PUCT evaluator's value head, and the gym's shaped reward.
///
/// Reads `PlayerResources::agenda_points` directly rather than re-deriving
/// it from `scored_agendas`/`CardRegistry`: `engine::score_agenda` and
/// `run::access::resolve_steal` already keep that field in sync on every
/// point-scoring action. The registry is used for what only a card's text
/// can say — which installs are ICE, how far an agenda is from scoring,
/// which rig cards break which ICE.
pub fn evaluate_state(state: &GameState, side: Side, registry: &CardRegistry) -> f64 {
    evaluate_state_with(state, side, registry, &Weights::default())
}

/// `evaluate_state` under a particular `Weights` — what a `Personality`
/// gives the heuristic, MCTS and the uniform PUCT evaluator.
pub fn evaluate_state_with(state: &GameState, side: Side, registry: &CardRegistry, w: &Weights) -> f64 {
    if let GamePhase::GameOver(winner) = state.phase {
        return if winner == side { WIN_SCORE } else { -WIN_SCORE };
    }
    if let Some(paid) = through_parked_payment(state, registry, w) {
        return evaluate_state_with(&paid, side, registry, w);
    }

    let own = state.resources(side);
    let opponent = state.resources(side.other());
    let mut score = (own.agenda_points.0 as f64 - opponent.agenda_points.0 as f64) * w.agenda_point_weight;
    // A point a side's cards spare it of the target is a point it has —
    // Issuaq Adaptics' "for each hosted power counter, you need 1 less
    // agenda point to win the game" (Phase 5 §37) — read off the engine's
    // own target (`continuous::points_to_win`), which the win check asks.
    let spared = |side: Side| f64::from(state.rules.winning_agenda_points as i32 - continuous::points_to_win(state, registry, side));
    score += (spared(side) - spared(side.other())) * w.agenda_point_weight;
    if state.is_resolution_blocked() && current_actor(state) == Some(side) {
        score -= w.unresolved_decision_weight;
        let upside = pending_decision_upside(state, side, registry, w);
        if upside > 0.0 {
            score += upside + w.pending_decision_upside_weight;
        }
    }
    score += own.credits.0 as f64 * w.own_credit_weight;
    score -= opponent.credits.0 as f64 * w.opponent_credit_weight;
    // The guide's-rate terms are added after the whole of the old sum,
    // on both arms, so at zero weight the sum's order — and its rounding
    // — is the one `coverage_identical.py` pinned.
    let horizon = horizon(stage(state, registry));
    match side {
        Side::Corp => corp::score(state, registry, w, horizon, &mut score),
        Side::Runner => runner::score(state, registry, w, horizon, &mut score),
    }
    if w.click_weight != 0.0 {
        score += f64::from(own.clicks.0) * w.click_weight;
    }
    if w.declared_value_weight != 0.0 {
        score += held_declared_value(state, side, registry, w) * w.declared_value_weight;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::test_support::*;
    use netrunner_core::rules::{AgendaPoints, GameState};

    /// Core damage is the hand size it takes, on both chairs (§36): the
    /// card it discards is the state's already.
    #[test]
    fn core_damage_is_a_hand_size_both_chairs_read() {
        use netrunner_core::rules::{Clicks, Credits};
        let w = crate::plans::Style::BALANCED.planned_weights(Side::Runner);
        let mut state = GameState::new(0);
        state.runner.grip = corp_cards("g", 4);
        let before = (evaluate_state_with(&state, Side::Runner, &empty(), &w), evaluate_state_with(&state, Side::Corp, &empty(), &w));
        state.runner.brain_damage = 1;
        assert!((evaluate_state_with(&state, Side::Runner, &empty(), &w) - before.0 + w.core_damage_weight).abs() < 1e-9);
        assert!((evaluate_state_with(&state, Side::Corp, &empty(), &w) - before.1 - w.core_damage_weight).abs() < 1e-9);
        // Thule Subsea's "do 1 core damage unless they spend [click] and
        // 2[credit]": a Runner with them spends them.
        let mut base = GameState::new(0);
        base.runner.grip = corp_cards("g", 4);
        base.runner.resources.credits = Credits(5);
        base.runner.resources.clicks = Clicks(2);
        let mut paid = base.clone();
        paid.runner.resources.credits = Credits(3);
        paid.runner.resources.clicks = Clicks(1);
        let mut took = base.clone();
        took.runner.grip.pop();
        took.runner.brain_damage = 1;
        let score = |state: &GameState| evaluate_state_with(state, Side::Runner, &empty(), &w);
        assert!(score(&paid) > score(&took), "paid {} against took {}", score(&paid), score(&took));
    }

    /// A hand size the rig gives back is the core damage it answers, on
    /// both chairs (§59): Marrow's "+3 maximum hand size" is worth more
    /// than the core damage its install costs, where it was read as the
    /// damage alone and never installed.
    #[test]
    fn a_hand_size_the_rig_gives_is_the_core_damage_it_answers() {
        let w = crate::plans::Style::BALANCED.planned_weights(Side::Runner);
        let registry = pool();
        let mut bare = GameState::new(0);
        bare.runner.grip = corp_cards("g", 4);
        let mut marrow = bare.clone();
        marrow.runner.rig.push(rig_card("marrow"));
        marrow.runner.brain_damage = 1;
        let runner = |state: &GameState| evaluate_state_with(state, Side::Runner, &registry, &w);
        let corp = |state: &GameState| evaluate_state_with(state, Side::Corp, &registry, &w);
        assert!(runner(&marrow) > runner(&bare), "Marrow {} against none {}", runner(&marrow), runner(&bare));
        assert!(corp(&marrow) < corp(&bare), "the Corp reads the same hand size the other way");
    }

    #[test]
    fn the_default_weights_are_the_constants_and_score_identically() {
        let mut state = GameState::new(0);
        state.corp.resources.agenda_points = AgendaPoints(2);
        state.runner.grip = corp_cards("g", 4);
        for side in [Side::Corp, Side::Runner] {
            assert_eq!(evaluate_state(&state, side, &empty()), evaluate_state_with(&state, side, &empty(), &Weights::default()));
        }
        assert_eq!(Weights::default().installed_agenda_weight, 0.0, "the balanced Corp has no install preference by type");
    }

    /// The reference is pinned: every guide's-rate term is zero in the
    /// default weights and in every plan's profile, so the one-ply
    /// chooser and every ladder rung score exactly as they did before
    /// Stage 5, and `at_the_guides_rate` is what moves.
    #[test]
    fn the_guides_rate_terms_are_off_in_every_weights_the_reference_scores_with() {
        let off = |w: &Weights| {
            [w.click_weight, w.future_credit_weight, w.declared_value_weight, w.rez_reserve_weight, w.unaffordable_ice_weight, w.taxing_window_weight, w.forced_rez_weight, w.finishable_install_weight]
        };
        assert_eq!(off(&Weights::default()), [0.0; 8]);
        for plan in crate::plans::Plan::ALL {
            assert_eq!(off(&plan.weights()), [0.0; 8], "{plan:?}");
        }
        let guide = Weights::default().at_the_guides_rate();
        assert!(off(&guide).iter().all(|w| *w > 0.0));
        assert_eq!(Weights { click_weight: 0.0, future_credit_weight: 0.0, declared_value_weight: 0.0, rez_reserve_weight: 0.0, unaffordable_ice_weight: 0.0, taxing_window_weight: 0.0, forced_rez_weight: 0.0, finishable_install_weight: 0.0, ..guide }, Weights::default(), "the guide's rate is those eight and nothing else");
        assert_eq!(guide.click_weight, guide.own_credit_weight, "a click is worth the credit it would buy");
        assert!(guide.future_credit_weight < guide.own_credit_weight, "a credit later is worth less than one now");
    }

    /// The same pin for the plans' terms (Stage 6): zero in the default
    /// and in every profile, set by `with_plans` for the plans a style
    /// stacks, and a plan after the first adds only its own levers.
    #[test]
    fn the_plans_terms_are_off_in_every_weights_the_reference_scores_with() {
        use crate::plans::{Plan, Style};
        let off = |w: &Weights| {
            [w.run_stakes_weight, w.rez_held_weight, w.ice_order_weight, w.never_advance_weight, w.lethal_threat_weight, w.tag_leverage_weight, w.bluff_weight]
        };
        assert_eq!(off(&Weights::default()), [0.0; 7]);
        assert!(!Weights::default().fort_until_beaten);
        for plan in Plan::ALL {
            assert_eq!(off(&plan.weights()), [0.0; 7], "{plan:?}");
            assert!(!plan.weights().fort_until_beaten, "{plan:?}");
            assert_eq!(Style::of(plan).weights(), plan.weights(), "the reference scores with the profile alone");
        }
        // Every Corp seat carries the five general terms, balanced
        // included — the tag's leverage among them since §51, gated on
        // the punisher held and not on a plan; a Runner seat carries
        // none of the Corp's.
        let glacier = Weights::default().with_plans(Side::Corp, &Style::of(Plan::Glacier));
        assert!(glacier.run_stakes_weight > 0.0 && glacier.ice_order_weight > 0.0 && glacier.never_advance_weight > 0.0);
        assert_eq!(glacier.rez_held_weight, REZ_HELD_WEIGHT, "measured, and shipped at what it measured");
        assert_eq!(glacier.tag_leverage_weight, TAG_LEVERAGE_WEIGHT, "a glacier holding Scorched Earth reads the tag");
        assert_eq!((glacier.lethal_threat_weight, glacier.bluff_weight, glacier.fort_until_beaten), (0.0, 0.0, false));
        assert_eq!(Weights::default().with_plans(Side::Corp, &Style::BALANCED).run_stakes_weight, glacier.run_stakes_weight, "a balanced Corp plays the Corp's chapter");
        let runner_off = |w: &Weights| [w.run_stakes_weight, w.rez_held_weight, w.ice_order_weight, w.never_advance_weight, w.lethal_threat_weight, w.tag_leverage_weight, w.bluff_weight];
        assert_eq!(runner_off(&Weights::default().with_plans(Side::Runner, &Style::of(Plan::Pressure))), [0.0; 7], "a Runner seat switches on no Corp term");
        assert_eq!(runner_off(&Weights::default().with_plans(Side::Runner, &Style::BALANCED)), [0.0; 7]);
        // A plan's own terms.
        let kill = Weights::default().with_plans(Side::Corp, &Style::of(Plan::Kill));
        assert!(kill.lethal_threat_weight > 0.0 && kill.bluff_weight == 0.0);
        let traps = Weights::default().with_plans(Side::Corp, &Style::of(Plan::Traps));
        assert!(traps.bluff_weight > 0.0 && traps.lethal_threat_weight == 0.0);
        // The stack: glacier then fast advance yields the fort; either
        // alone does not — the pair is the condition, and the order says
        // which profile leads.
        let stacked = Style::new(&[Plan::Glacier, Plan::FastAdvance]).unwrap();
        let planned = stacked.planned_weights(Side::Corp);
        assert!(planned.fort_until_beaten);
        assert_eq!(planned.agenda_protection_weight, Plan::Glacier.weights().agenda_protection_weight, "the first plan's dials");
        assert_eq!(planned.installed_agenda_weight, Plan::FastAdvance.weights().installed_agenda_weight, "the second plan's own lever");
        assert!(!Style::of(Plan::Glacier).planned_weights(Side::Corp).fort_until_beaten);
        assert!(!Style::of(Plan::FastAdvance).planned_weights(Side::Corp).fort_until_beaten);
        // Traps behind the wall: glacier's dials, the ambush terms on.
        let behind = Style::new(&[Plan::Glacier, Plan::Traps]).unwrap().planned_weights(Side::Corp);
        assert_eq!(behind.ambush_weight, Plan::Traps.weights().ambush_weight);
        assert_eq!(behind.agenda_protection_weight, Plan::Glacier.weights().agenda_protection_weight);
        assert!(!behind.fort_until_beaten);
    }

    /// The same pin for the Runner's plan terms (Stage 7): zero in the
    /// default and in every profile; every Runner seat carries the five
    /// general terms, balanced included, and each plan its own; a Corp
    /// seat carries none of them.
    #[test]
    fn the_runners_plan_terms_are_off_in_every_profile_and_on_for_a_planner_runner() {
        use crate::plans::{Plan, Style};
        let off = |w: &Weights| [w.last_click_run_weight, w.feared_flatline_weight, w.runner_stakes_weight, w.unshown_breaker_weight, w.shut_door_weight, w.hq_pressure_weight, w.dismantle_weight, w.rd_access_weight];
        assert_eq!(off(&Weights::default()), [0.0; 8]);
        for plan in Plan::ALL {
            assert_eq!(off(&plan.weights()), [0.0; 8], "{plan:?}");
        }
        let balanced = Weights::default().with_plans(Side::Runner, &Style::BALANCED);
        assert!(balanced.feared_flatline_weight > 0.0 && balanced.unshown_breaker_weight > 0.0 && balanced.shut_door_weight > 0.0, "every Runner plays the Runner's chapter");
        assert_eq!((balanced.last_click_run_weight, balanced.runner_stakes_weight), (LAST_CLICK_RUN_WEIGHT, RUNNER_STAKES_WEIGHT), "measured, and shipped at what they measured");
        assert_eq!((balanced.hq_pressure_weight, balanced.dismantle_weight, balanced.rd_access_weight), (0.0, 0.0, 0.0));
        assert_eq!(off(&Weights::default().with_plans(Side::Corp, &Style::of(Plan::Glacier))), [0.0; 8], "a Corp seat switches on no Runner term");
        let pressure = Style::of(Plan::Pressure).planned_weights(Side::Runner);
        assert!(pressure.hq_pressure_weight > 0.0 && pressure.dismantle_weight == 0.0 && pressure.rd_access_weight == 0.0);
        assert_eq!(pressure.active_run_weight, Weights::default().active_run_weight, "no Runner plan has a profile");
        assert_eq!(Weights { hq_pressure_weight: 0.0, ..pressure }, Style::BALANCED.planned_weights(Side::Runner), "a Runner plan is its lever alone to the planner");
        let dismantle = Style::of(Plan::Dismantle).planned_weights(Side::Runner);
        assert!(dismantle.dismantle_weight > 0.0 && dismantle.hq_pressure_weight == 0.0);
        let rig = Style::of(Plan::Rig).planned_weights(Side::Runner);
        assert!(rig.rd_access_weight > 0.0 && rig.dismantle_weight == 0.0);
        // A stacked Runner deck: the first plan's dials, the second's own
        // lever, and nothing of the second's profile.
        let stacked = Style::new(&[Plan::Rig, Plan::Dismantle]).unwrap().planned_weights(Side::Runner);
        assert_eq!(stacked.breaker_coverage_weight, Weights::default().breaker_coverage_weight);
        assert!(stacked.rd_access_weight > 0.0 && stacked.dismantle_weight > 0.0);
        let stacked = Style::new(&[Plan::Pressure, Plan::Rig]).unwrap().planned_weights(Side::Runner);
        assert!(stacked.rd_access_weight > 0.0 && stacked.hq_pressure_weight > 0.0);
        assert!(stacked.unshown_breaker_weight < stacked.breaker_coverage_weight, "a breaker for unshown ICE is still worth installing");
        assert!(stacked.feared_flatline_weight < AGENDA_POINT_WEIGHT / 5.0, "the fear is under what a breach may find");
    }

    /// A click left is worth the credit it would buy, at the guide's
    /// rate and not before, so a free action keeps what a click action
    /// spends.
    #[test]
    fn a_click_left_is_worth_a_credit_at_the_guides_rate() {
        use netrunner_core::rules::Clicks;
        let registry = empty();
        let mut three = GameState::new(0);
        three.phase = GamePhase::Action(Side::Corp);
        three.corp.resources.clicks = Clicks(3);
        let mut two = three.clone();
        two.corp.resources.clicks = Clicks(2);
        assert_eq!(evaluate_state(&three, Side::Corp, &registry), evaluate_state(&two, Side::Corp, &registry), "the reference does not price a click");
        let guide = Weights::default().at_the_guides_rate();
        let delta = evaluate_state_with(&three, Side::Corp, &registry, &guide) - evaluate_state_with(&two, Side::Corp, &registry, &guide);
        assert!((delta - guide.own_credit_weight).abs() < 1e-9, "{delta}");
        assert_eq!(evaluate_state_with(&three, Side::Runner, &registry, &guide), evaluate_state_with(&two, Side::Runner, &registry, &guide), "the Corp's clicks are not the Runner's");
    }
}
