// The two stance endpoints are the archetypes themselves rather than a
// second copy of their numbers, so `stage_weights` reads up a layer to
// `personality`. The alternative — restating seven constants here — would
// have left two definitions of `Builder` to keep in step.
use crate::personality::Personality;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{
    card_matches_filter, Amount, CardDefinition, CardFilter, CardType, CardZoneRef, Cost, Effect, IceType,
    SubroutineBreakCount, Trigger,
};
use netrunner_core::rules::continuous;
use netrunner_core::rules::{
    current_actor, GamePhase, GameState, InstalledCard, InstalledRunnerCard, PendingDecision, RunIce, RunPhase,
    RunState, Side, SubroutineStatus,
};

const WIN_SCORE: f64 = 1000.0;
const AGENDA_POINT_WEIGHT: f64 = 20.0;
const OWN_CREDIT_WEIGHT: f64 = 0.4;
const OPPONENT_CREDIT_WEIGHT: f64 = 0.2;
const BAD_PUBLICITY_WEIGHT: f64 = 3.0;
const TAG_WEIGHT: f64 = 4.0;
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
/// fresh random card. `Aggressive`'s 1.2 and `Cautious`'s 0.4 keep their
/// meaning, per card. The successful-run term below is unchanged: it is
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
/// worth it. Unrezzed ambushes are not read: in a determinized sample
/// their identity is a guess, and the Runner is meant not to know.
const KNOWN_AMBUSH_WEIGHT: f64 = 1.5;
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

/// How far a position's *stage* is allowed to move the weights it is
/// scored with: 0.0 is no movement at all and every number this repo has
/// recorded, 1.0 is the full travel from the build archetype to the
/// pressure one. See `stage_weights`.
///
/// **Zero by default, and that is the discipline rather than the answer.**
/// ROADMAP Phase 2 §5 items 38 and 40 both shipped a term at weight 0.0 —
/// apparatus with its reason recorded, byte-identical to the baseline —
/// and this is built the same way round: the mechanism lands first, the
/// weight is whatever the measurement earns, and a flat result is recorded
/// rather than tuned until it moves.
///
/// **Measured twice, and zero both times for different reasons.** On
/// §6's endpoints the travel cost the Runner chair 0.169 (the balanced
/// Corp's win share 0.140 → 0.309 at 1.0, four seeds × 384), because the
/// build endpoint was the worst Runner profile in the pool. With
/// `Personality::Builder` repaired (§7) the cost is gone — 0.149 at 1.0,
/// inside the seed-spread band — and the travel still buys nothing,
/// because the repaired build endpoint is now the *best* profile and
/// every leg that moves off it loses, in either direction: standing on it
/// all game 0.113, travelling to balanced 0.125, balanced travelling to it
/// 0.137, travelling to `Aggressive` 0.149. What a leg scores follows
/// where it spends the game, not when it moves.
///
/// **Nor does a clock** (§8). The fallback §4(c) kept to beat — the same
/// travel on the Runner's own turn count, full at turn 4, 8, 12 or 16 —
/// never beats `runner_stage` beyond noise on either destination (best
/// −0.014, z 1.29), loses to it outright when the clock is short (turn 4:
/// +0.040, z 3.56), and never beats standing on `Builder` (best +0.003).
const STAGE_GAIN: f64 = 0.0;

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
/// `stage_gain` is a fourth and is not a term at all — it scales the other
/// weights rather than the score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub agenda_point_weight: f64,
    pub own_credit_weight: f64,
    pub opponent_credit_weight: f64,
    pub bad_publicity_weight: f64,
    pub tag_weight: f64,
    pub board_presence_weight: f64,
    pub memory_weight: f64,
    pub rezzed_ice_weight: f64,
    pub rezzed_asset_weight: f64,
    pub unrezzed_install_weight: f64,
    pub advancement_weight: f64,
    pub agenda_counter_weight: f64,
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
    /// How far the position's stage may move every weight above, from the
    /// build archetype toward the pressure one. Zero is the static
    /// evaluator this repo has always had. See `STAGE_GAIN` and
    /// `stage_weights`.
    pub stage_gain: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            agenda_point_weight: AGENDA_POINT_WEIGHT,
            own_credit_weight: OWN_CREDIT_WEIGHT,
            opponent_credit_weight: OPPONENT_CREDIT_WEIGHT,
            bad_publicity_weight: BAD_PUBLICITY_WEIGHT,
            tag_weight: TAG_WEIGHT,
            board_presence_weight: BOARD_PRESENCE_WEIGHT,
            memory_weight: MEMORY_WEIGHT,
            rezzed_ice_weight: REZZED_ICE_WEIGHT,
            rezzed_asset_weight: REZZED_ASSET_WEIGHT,
            unrezzed_install_weight: UNREZZED_INSTALL_WEIGHT,
            advancement_weight: ADVANCEMENT_WEIGHT,
            agenda_counter_weight: AGENDA_COUNTER_WEIGHT,
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
            stage_gain: STAGE_GAIN,
        }
    }
}

/// A rough static evaluation of `state` from `side`'s perspective: positive
/// favors `side`, negative favors the opponent. Shared by `HeuristicAgent`'s
/// one-ply scoring, `MctsAgent`'s rollout/leaf evaluation, the uniform
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

/// `w`, moved toward the stance the position calls for.
///
/// **The defect this exists for.** `evaluate_state_with` is otherwise a
/// static function of the position: it scores a board identically on turn
/// 1 and turn 20, so a bot interleaves building and pressuring every turn
/// instead of doing one and then the other. `diag tempo`'s baseline
/// (ROADMAP Phase 5 §5) measured both chairs doing exactly that, and found
/// the Runner's stance *inverted* — 0.82 installs on turn 1 falling to
/// 0.07, against 1.85 runs on turn 1 rising to 2.20 and then decaying,
/// with rig coverage never past 1.43 of 3. It is most aggressive when its
/// rig is emptiest.
///
/// **Why an interpolation rather than new terms.** Four of the seven
/// personalities describe themselves with a temporal word they cannot act
/// on — `Glacier` "build the fort, *then* score behind it", `Rush` "score
/// early, protect late", `Builder` "the rig first… *before* the runs
/// start", `Cautious` "a full rig *before* a run" — and the pairs move the
/// *same fields in opposite directions*. They are the two ends of one
/// dial, and the game is played standing still on it. Adding six new
/// `Weights` terms instead would have grown the surface for the same claim
/// and left the profiles still unable to sequence.
///
/// **A reading this comment used to give, now withdrawn.** It said
/// balanced beating both `Builder` and `Aggressive` was "what a fixed
/// midpoint of a dial that should be moving looks like against its own two
/// ends". It was what a broken `Builder` looks like: a flat presence bonus
/// had made it the worst Runner profile in the pool, and repaired it beats
/// balanced (ROADMAP Phase 5 §7). With that endpoint fixed no leg of the
/// travel beats standing on it — see `STAGE_GAIN`.
///
/// **Continuous, never a switch.** `UniformPolicyEvaluator::evaluate_from`
/// scores leaf minus root, so a stage that jumped inside one search would
/// make two leaves of the same tree incomparable in a way no other term
/// is. Every input below moves by one card or one credit at a time.
///
/// The Corp arm is deliberately not staged yet: Phase 5 §5's baseline
/// relocated its signal (it is rich and under-rezzing *late*, not
/// credit-starved throughout as §3 read it), and one chair at a time is
/// what keeps a pool-wide effect attributable.
fn stage_weights(state: &GameState, side: Side, registry: &CardRegistry, w: &Weights) -> Weights {
    if w.stage_gain == 0.0 || side == Side::Corp {
        return *w;
    }
    let stance = lerp(
        &Personality::Builder.weights(),
        &Personality::Aggressive.weights(),
        runner_stage(state, registry),
    );
    lerp(w, &stance, w.stage_gain)
}

/// How far through its own game plan the Runner is, in `0.0..=1.0`: 0 is
/// "nothing to run with", 1 is "run now".
///
/// Two readings, and the larger wins. **Readiness** is the rig, over the
/// evaluator's own `breaker_coverage` rather than a second definition of
/// it — a Runner with no breakers has no business making runs, and one
/// that covers all three subtypes has no more building to do. **Urgency**
/// is the Corp's clock: at five of seven points the rig no longer matters
/// and the Runner has to contest whatever is on the table. Taking the
/// larger rather than the sum is what makes urgency an override instead of
/// a bonus, so a Runner that never finds its breakers still plays rather
/// than banking credits until it decks.
///
/// **Nothing here reads a sampled card.** Coverage is the Runner's own
/// rig, agenda points are public, and the win threshold is
/// `MatchRules::winning_agenda_points` rather than a hard-coded 7 (the
/// starter format plays to 6). That keeps the scalar on the safe side of
/// the determinization line, where every term but
/// `unrezzed_threat_weight` already sits.
fn runner_stage(state: &GameState, registry: &CardRegistry) -> f64 {
    let readiness = breaker_coverage(state, registry) as f64 / 3.0;
    let target = state.rules.winning_agenda_points.max(1);
    let urgency = f64::from(state.corp.resources.agenda_points.0) / f64::from(target);
    readiness.max(urgency).clamp(0.0, 1.0)
}

/// `a` at `t == 0`, `b` at `t == 1`. The counts round rather than
/// truncate, so a `grip_floor` travelling 3 → 2 crosses at the halfway
/// point rather than at the very end.
///
/// `stage_gain` is taken from `a` and never interpolated: it is the dial's
/// own setting, not one of the weights the dial moves, and blending it
/// would make the travel depend on where the travel had already got to.
fn lerp(a: &Weights, b: &Weights, t: f64) -> Weights {
    let f = |x: f64, y: f64| x + (y - x) * t;
    let c = |x: usize, y: usize| (x as f64 + (y as f64 - x as f64) * t).round().max(0.0) as usize;
    let n = |x: u32, y: u32| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round().max(0.0) as u32;
    Weights {
        agenda_point_weight: f(a.agenda_point_weight, b.agenda_point_weight),
        own_credit_weight: f(a.own_credit_weight, b.own_credit_weight),
        opponent_credit_weight: f(a.opponent_credit_weight, b.opponent_credit_weight),
        bad_publicity_weight: f(a.bad_publicity_weight, b.bad_publicity_weight),
        tag_weight: f(a.tag_weight, b.tag_weight),
        board_presence_weight: f(a.board_presence_weight, b.board_presence_weight),
        memory_weight: f(a.memory_weight, b.memory_weight),
        rezzed_ice_weight: f(a.rezzed_ice_weight, b.rezzed_ice_weight),
        rezzed_asset_weight: f(a.rezzed_asset_weight, b.rezzed_asset_weight),
        unrezzed_install_weight: f(a.unrezzed_install_weight, b.unrezzed_install_weight),
        advancement_weight: f(a.advancement_weight, b.advancement_weight),
        agenda_counter_weight: f(a.agenda_counter_weight, b.agenda_counter_weight),
        ambush_advancement_weight: f(a.ambush_advancement_weight, b.ambush_advancement_weight),
        ambush_advancement_cap: n(a.ambush_advancement_cap, b.ambush_advancement_cap),
        ambush_weight: f(a.ambush_weight, b.ambush_weight),
        agenda_protection_weight: f(a.agenda_protection_weight, b.agenda_protection_weight),
        agenda_protection_cap: c(a.agenda_protection_cap, b.agenda_protection_cap),
        breaker_coverage_weight: f(a.breaker_coverage_weight, b.breaker_coverage_weight),
        unbreakable_ice_weight: f(a.unbreakable_ice_weight, b.unbreakable_ice_weight),
        etr_subroutine_weight: f(a.etr_subroutine_weight, b.etr_subroutine_weight),
        active_run_weight: f(a.active_run_weight, b.active_run_weight),
        advanced_card_prospect_weight: f(a.advanced_card_prospect_weight, b.advanced_card_prospect_weight),
        known_ambush_weight: f(a.known_ambush_weight, b.known_ambush_weight),
        opponent_board_weight: f(a.opponent_board_weight, b.opponent_board_weight),
        successful_run_weight: f(a.successful_run_weight, b.successful_run_weight),
        pending_subroutine_weight: f(a.pending_subroutine_weight, b.pending_subroutine_weight),
        unresolved_decision_weight: f(a.unresolved_decision_weight, b.unresolved_decision_weight),
        pending_decision_upside_weight: f(a.pending_decision_upside_weight, b.pending_decision_upside_weight),
        strength_shortfall_weight: f(a.strength_shortfall_weight, b.strength_shortfall_weight),
        unrezzed_threat_weight: f(a.unrezzed_threat_weight, b.unrezzed_threat_weight),
        savings_shortfall_weight: f(a.savings_shortfall_weight, b.savings_shortfall_weight),
        grip_shortfall_weight: f(a.grip_shortfall_weight, b.grip_shortfall_weight),
        grip_floor: c(a.grip_floor, b.grip_floor),
        held_card_weight: f(a.held_card_weight, b.held_card_weight),
        hq_shortfall_weight: f(a.hq_shortfall_weight, b.hq_shortfall_weight),
        hq_floor: c(a.hq_floor, b.hq_floor),
        rd_draw_reserve: c(a.rd_draw_reserve, b.rd_draw_reserve),
        active_run_against_weight: f(a.active_run_against_weight, b.active_run_against_weight),
        installed_agenda_weight: f(a.installed_agenda_weight, b.installed_agenda_weight),
        stage_gain: a.stage_gain,
    }
}

/// A payment parked on its payer's answer (`netrunner_core::rules::
/// PendingPayment`), priced as the payment made: each answer is applied,
/// any further question answered the same way, and the payer's best result
/// by the payer's own reckoning is the state scored.
///
/// A parked payment is the untouched state with a question on it — the
/// action it waits on has not happened at all, by construction (the
/// engine unwinds and replays) — so without this the action that parks one
/// scores as if it did nothing. That is not a small error: when the
/// Corp's accept of Anoetic Void's offer began asking which two cards of
/// HQ to trash, the one accept the heuristic made in a 192-game pass
/// became a decline, because "accept" now looked like "nothing" beside a
/// run continuing. The same shape as `pending_decision_upside` pricing a
/// parked prompt's continuation, and exact rather than a bound: the
/// payment's own answers are finite and each is a legal action. `None`
/// when nothing is parked, or no answer goes through.
fn through_parked_payment(state: &GameState, registry: &CardRegistry, w: &Weights) -> Option<GameState> {
    let payment = state.pending_payment.as_ref()?;
    let payer = payment.side;
    payment
        .question
        .answers()
        .into_iter()
        .filter_map(|value| netrunner_core::rules::apply_action(state, registry, payment.question.action_for(value)).ok())
        .map(|(next, _)| through_parked_payment(&next, registry, w).unwrap_or(next))
        .map(|paid| (evaluate_state_with(&paid, payer, registry, w), paid))
        .max_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, paid)| paid)
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

    // Before the shared prefix, not inside the per-side arm: a stance moves
    // `opponent_credit_weight` too (`Aggressive` doubles it), and that term
    // is read above the `match`. Hoisted once for the same reason
    // `rig_coverage` is — `corp_install_value` runs per installed card.
    let staged;
    let w = if w.stage_gain == 0.0 {
        w
    } else {
        staged = stage_weights(state, side, registry, w);
        &staged
    };

    let own = state.resources(side);
    let opponent = state.resources(side.other());
    let mut score = (own.agenda_points.0 as f64 - opponent.agenda_points.0 as f64) * w.agenda_point_weight;
    if state.is_resolution_blocked() && current_actor(state) == Some(side) {
        score -= w.unresolved_decision_weight;
        let upside = pending_decision_upside(state, side, registry, w);
        if upside > 0.0 {
            score += upside + w.pending_decision_upside_weight;
        }
    }
    score += own.credits.0 as f64 * w.own_credit_weight;
    score -= opponent.credits.0 as f64 * w.opponent_credit_weight;

    match side {
        Side::Corp => {
            score -= state.corp.bad_publicity as f64 * w.bad_publicity_weight;
            // Computed once and handed down rather than read per install:
            // the rig does not change between two cards on the same board,
            // and `corp_install_value` is called for every one of them.
            let rig = rig_coverage(state, registry);
            for installed in &state.corp.installed {
                score += corp_install_value(installed, registry, w, rig);
            }
            score += f64::from(scored_agenda_counters(state)) * w.agenda_counter_weight;
            score += protected_agenda_ice(state, registry, w.agenda_protection_cap) as f64 * w.agenda_protection_weight;
            if w.installed_agenda_weight != 0.0 {
                score += installed_agendas(state, registry) as f64 * w.installed_agenda_weight;
            }
            if state.corp.r_and_d.len() >= w.rd_draw_reserve {
                score -= w.hq_floor.saturating_sub(state.corp.hq.len()) as f64 * w.hq_shortfall_weight;
            }
            // The mirror of the Runner's `active_run_weight`, on the same
            // `run_is_breakable` gate. Jacking out or ending the run
            // returns it, which is what makes `EndTheRun` priceable in
            // `continuation_upside`.
            if let Some(run) = &state.active_run
                && run_is_breakable(state, run, registry)
            {
                score -= w.active_run_against_weight;
            }
        }
        Side::Runner => {
            score -= state.runner.tags as f64 * w.tag_weight;
            score += state.runner.rig.len() as f64 * w.board_presence_weight;
            score += state.runner.memory_units.0 as f64 * w.memory_weight;
            score += breaker_coverage(state, registry) as f64 * w.breaker_coverage_weight;
            score -= breaker_savings_shortfall(state, registry) as f64 * w.savings_shortfall_weight;
            score -= w.grip_floor.saturating_sub(state.runner.grip.len()) as f64 * w.grip_shortfall_weight;
            score += held_cards_value(state, registry, w) * w.held_card_weight;
            score -= visible_corp_board(state, registry, w) * w.opponent_board_weight;
            if state.this_turn.times(Trigger::OnSuccessfulRun) > 0 {
                score += w.successful_run_weight;
            }
            if let Some(run) = &state.active_run {
                if run_is_breakable(state, run, registry) {
                    score += access_prospect(state, run, registry, w);
                }
                score -= pending_subroutines(run) as f64 * w.pending_subroutine_weight;
                score -= strength_shortfall(state, run, registry) as f64 * w.strength_shortfall_weight;
                if w.unrezzed_threat_weight != 0.0 {
                    score -= unbreakable_unrezzed_ice(state, run, registry) as f64 * w.unrezzed_threat_weight;
                }
            }
        }
    }
    score
}

/// Whether the Runner can pay to break every pending subroutine on each
/// rezzed ICE it has not yet passed, out of its own credits plus the run's
/// bad-publicity pool. ICE before `run.position` is already behind the
/// Runner; unrezzed ICE is treated as passable (see `ACTIVE_RUN_WEIGHT`).
/// One rezzed ICE no rig card can break makes the whole run unbreakable —
/// a run that stops at the third ICE is worth no more than one that stops
/// at the first.
fn run_is_breakable(state: &GameState, run: &RunState, registry: &CardRegistry) -> bool {
    let mut total = 0;
    for ice in run.ice.iter().skip(run.position).filter(|ice| ice.rezzed) {
        let Some(cost) = cheapest_break_cost(state, ice, registry) else { return false };
        total += cost;
    }
    total <= state.runner.resources.credits.0 + run.bad_publicity_credits
}

/// What the breach of `run.server` is worth to the Runner, read only off
/// what its `ClientView` shows: hidden accesses at `active_run_weight`
/// apiece, advancement tokens on the target's face-down root cards,
/// known ambushes subtracted, and the net gain of trashing what it could
/// afford to trash there. The caller gates it on `run_is_breakable`, as
/// the flat run term was.
///
/// **Nothing here reads a sampled identity.** An unrezzed card counts by
/// its position and its tokens; a face-down Archives card counts once; a
/// card is read through the registry only when it is rezzed or face-up,
/// which is when the real Runner could read it too. That is the line
/// `UNREZZED_THREAT_WEIGHT` alone crosses, on purpose and at weight zero.
///
/// **"Seen this turn" is `servers_run_this_turn`.** A second R&D run
/// finds the same top card, a second Archives run a pile already turned
/// face-up, a second remote run the same face-down card — so those count
/// nothing. HQ is the one server a repeat can pay on, because each access
/// is a random card, and it pays exactly its chance of a card not yet
/// seen: after `k` earlier HQ runs this turn, an access from `n` cards is
/// fresh with probability `((n − 1) / n)^k`. Counting every repeat at
/// full value was measured first and the Runner ran HQ 1,252 times in
/// 96 games, thirteen a game, into a hand of one or two cards it had
/// already read. At four cards the second run is 0.45 and the third 0.34,
/// so the Runner runs HQ twice and then clicks for a credit. Every
/// `run::start_run` pushes the target at initiation, so the active run
/// is already on the list and "earlier" means a second entry; one
/// card-started path (`Effect::InitiateRun` through `run/engine.rs`'s
/// deduplicating push) records a repeat run once, and that run is priced
/// as if it were the first — the cheaper direction.
fn access_prospect(state: &GameState, run: &RunState, registry: &CardRegistry, w: &Weights) -> f64 {
    use netrunner_core::rules::{InstallSlot, ServerId};
    let server = run.server;
    let earlier = runs_earlier_this_turn(state, server);
    let seen = earlier > 0;
    let credits = state.runner.resources.credits.0;
    let mut hidden = 0.0_f64;
    let mut tokens = 0u32;
    let mut ambushes = 0usize;
    let mut trash_gain = 0.0;
    for installed in state.corp.installed.iter().filter(|card| card.server == server && card.slot == InstallSlot::Root) {
        if installed.rezzed {
            let Some(def) = registry.get(&installed.card) else { continue };
            if punishes_access_with_damage(def) {
                ambushes += 1;
            }
            if let Some(cost) = def.trash_cost
                && cost <= credits
            {
                let removed = visible_install_value(installed, registry, w) * w.opponent_board_weight;
                trash_gain += (removed - f64::from(cost) * w.own_credit_weight).max(0.0);
            }
        } else if !seen {
            hidden += 1.0;
            tokens += installed.advancement_tokens;
        }
    }
    match server {
        ServerId::Hq => {
            let held = state.corp.hq.len();
            let accesses = (1 + run.additional_hq_access as usize).min(held);
            let fresh = if held == 0 { 0.0 } else { ((held - 1) as f64 / held as f64).powi(earlier as i32) };
            hidden += accesses as f64 * fresh;
        }
        ServerId::RnD if !seen => hidden += (1 + run.additional_rd_access as usize).min(state.corp.r_and_d.len()) as f64,
        ServerId::RnD | ServerId::Remote(_) => {}
        ServerId::Archives => {
            for archived in &state.corp.archives {
                if archived.facedown {
                    if !seen {
                        hidden += 1.0;
                    }
                } else if let Some(def) = registry.get(&archived.card) {
                    if punishes_access_with_damage(def) {
                        ambushes += 1;
                    }
                    // A face-up agenda in Archives is a steal the breach
                    // cannot miss, so it is worth the points outright
                    // rather than a hidden access's 0.6 — the one reason
                    // to run a pile the Runner has already read.
                    if def.card_type == CardType::Agenda {
                        trash_gain += f64::from(def.agenda_points.unwrap_or(0)) * w.agenda_point_weight;
                    }
                }
            }
        }
    }
    hidden * w.active_run_weight + f64::from(tokens) * w.advanced_card_prospect_weight
        - ambushes as f64 * w.known_ambush_weight
        + trash_gain
}

/// How many times the Runner ran `server` this turn before the run in
/// progress — see `access_prospect` for why the active run's own entry
/// is discounted.
fn runs_earlier_this_turn(state: &GameState, server: netrunner_core::rules::ServerId) -> usize {
    let entries = state.runner.servers_run_this_turn.iter().filter(|ran| **ran == server).count();
    let own = usize::from(state.active_run.as_ref().is_some_and(|run| run.server == server));
    entries.saturating_sub(own)
}

/// The Corp's board as the Runner's evaluation reads it, summed over
/// `visible_install_value`; subtracted at `opponent_board_weight`.
fn visible_corp_board(state: &GameState, registry: &CardRegistry, w: &Weights) -> f64 {
    state.corp.installed.iter().map(|installed| visible_install_value(installed, registry, w)).sum()
}

/// `corp_install_value` for a viewer who cannot see under a face-down
/// card. A rezzed install is public and priced exactly as the Corp
/// prices it; an unrezzed one is its install weight plus every token on
/// it at `advancement_weight`, because the requirement that would cap
/// those tokens is on a card the Runner has not seen. Two determinized
/// samples that put different cards under the same install therefore
/// score identically here, which is what keeps this term honest for the
/// search (`the_corp_board_term_reads_no_hidden_identity`).
///
/// **`UNBREAKABLE_ICE_WEIGHT` is switched off here** — the `[true; 3]` —
/// and not because the Runner may not look at its own rig. It is that the
/// Runner already pays `BREAKER_COVERAGE_WEIGHT` for covering a subtype;
/// letting the same rig fact also shrink the Corp board it subtracts would
/// count one thing twice in one score. This term prices the Corp's
/// material, and the rig is priced where the rig lives.
/// **The rig is forced to `[true; 3]` and nothing else is.** The reason
/// is double counting, not visibility: the Runner already pays
/// `BREAKER_COVERAGE_WEIGHT` for covering a subtype, so letting the same
/// rig fact also shrink the Corp board it subtracts would count one thing
/// twice in one score. This term prices the Corp's material, and the rig
/// is priced where the rig lives.
///
/// `ETR_SUBROUTINE_WEIGHT` is deliberately left alone here, and the line
/// is preference against fact. The rig flags say what a rig *can do*;
/// how many run-ending subroutines a rezzed piece of ICE has is a
/// property of the card, public the moment it is face up, and more of
/// them really is more material on the table. So the Runner's reading of
/// the Corp board moves with that term — measured on the ladder square,
/// the Runner column falls 0.000 to 0.010 a rung — which is why a Corp
/// column taken on this build is not strictly comparable with one taken
/// before it: the reference Runner is not byte-identical either.
fn visible_install_value(installed: &InstalledCard, registry: &CardRegistry, w: &Weights) -> f64 {
    if installed.rezzed {
        corp_install_value(installed, registry, w, [true; 3])
    } else {
        w.unrezzed_install_weight + f64::from(installed.advancement_tokens) * w.advancement_weight
    }
}

/// Unrezzed ICE still ahead of the Runner that the Corp could rez right
/// now and no rig card could then break. The counterpart to
/// `run_is_breakable`, which deliberately looks only at rezzed ICE — see
/// `UNREZZED_THREAT_WEIGHT` for why this is a separate weighted term
/// rather than a wider predicate.
///
/// **This is the evaluator's one window onto hidden information.** In a
/// determinized state `ice.card_id` is the *sampled* card, so two samples
/// that put different ICE behind the same install disagree here and
/// nowhere else. In a real `GameState` it is the true card, which is
/// correct for the Corp's own reasoning and is why the gym's shaped
/// reward should not turn this on for a Runner it computes from the
/// authoritative state.
///
/// Rez affordability is the printed cost against the Corp's credits,
/// ignoring what is added to it (`continuous::rez_cost_delta` — Tread
/// Lightly's +3, Fransofia Ward's +1) and any discount: over-estimating
/// what the Corp can pay only makes the Runner one ICE more cautious,
/// the same direction `breaker_savings_shortfall` already errs in.
fn unbreakable_unrezzed_ice(state: &GameState, run: &RunState, registry: &CardRegistry) -> usize {
    run.ice.iter().skip(run.position).filter(|ice| is_unrezzed_threat(state, ice, registry)).count()
}

/// Whether this one ICE is what `unbreakable_unrezzed_ice` counts:
/// unrezzed, affordable to the Corp at its printed cost, breakable by
/// nothing in the rig, **and carrying a subroutine that ends the run**.
///
/// That last clause is ROADMAP Phase 2 §5 item 39 and it halves what the
/// term counts. `cheapest_break_cost` returning `None` says no rig card
/// can break the ICE, which is not the same claim as "it stops you": an
/// ICE that tags, or does damage, or drains credits costs the Runner
/// something and then lets the run continue. Measured over six 96-game
/// legs, of the counted ICE the Corp rezzed, the run stopped there
/// **0.904** of the time with an ETR subroutine and **0.077** without,
/// and the two classes were an even split of what was counted. Item 38
/// read that null as a missing *probability* — how likely the Corp is to
/// rez — and item 39 measured the rez rate at 0.937 against the Corp
/// actually seated, which left nothing to discount.
///
/// Public because `diag rez-rate` measures how often a real Corp rezzes
/// exactly the ICE this predicate flags, and a copy of the predicate in
/// the CLI would be free to drift from the one the evaluator uses — the
/// whole point of that measurement is that it prices *this* term.
pub fn is_unrezzed_threat(state: &GameState, ice: &RunIce, registry: &CardRegistry) -> bool {
    if ice.rezzed {
        return false;
    }
    let Some(definition) = registry.get(&ice.card_id) else { return false };
    definition.cost <= state.corp.resources.credits.0
        // Every subroutine of an unrezzed ICE is `Pending` by
        // construction, so this reads the same set `cheapest_break_cost`
        // prices.
        && ice.subroutines.iter().any(|subroutine| subroutine.definition.effect.can_end_the_run())
        && cheapest_break_cost(state, ice, registry).is_none()
}

/// The fewest credits any rig card needs to pump up to `ice`'s strength
/// and break all of its pending subroutines; `None` when no rig card can.
/// An ICE with nothing pending costs nothing whatever the rig holds.
fn cheapest_break_cost(state: &GameState, ice: &RunIce, registry: &CardRegistry) -> Option<u32> {
    if pending_on(ice) == 0 {
        return Some(0);
    }
    state.runner.rig.iter().filter_map(|card| break_cost(state, card, ice, registry)).min()
}

/// What `card` would spend to break `ice` outright: pump credits to close
/// any strength shortfall, then break credits for every pending
/// subroutine, both read off its `Paid` abilities. Only credit-costed
/// abilities are priced — Botulus's counter-costed
/// `BreakSubroutinesUnconditionally` is not a spend this term is about.
/// `None` if the card has no break matching `ice`'s subtype, or a
/// shortfall and no pump. `BoostStrengthAmount` (Unity's +X) is priced as
/// +1 per activation: X counts Unity itself so it is at least 1, and
/// over-estimating a cost only makes the Runner save one click longer.
fn break_cost(state: &GameState, card: &InstalledRunnerCard, ice: &RunIce, registry: &CardRegistry) -> Option<u32> {
    let def = registry.get(&card.card)?;
    let pending = pending_on(ice);
    // The number the break contest uses — a pump bought inside the search
    // and what the table adds (Echelon, Rising Tide) included.
    let shortfall = (continuous::ice_strength(state, registry, ice) - continuous::breaker_strength(state, registry, card)).max(0) as u32;
    let mut cheapest_break: Option<u32> = None;
    let mut cheapest_pump: Option<u32> = None;
    let keep_min = |slot: &mut Option<u32>, cost: u32| *slot = Some(slot.map_or(cost, |c| c.min(cost)));
    for ability in def.abilities.iter().filter(|a| a.trigger == Trigger::Paid) {
        let credits = match ability.cost {
            None => 0,
            Some(Cost::Credits(c)) => c,
            Some(_) => continue,
        };
        ability.effect.for_each_effect(&mut |effect| match effect {
            Effect::BreakSubroutines { count, restrict_to } if restrict_to.is_none_or(|r| ice_is(state, ice, r, registry)) => {
                let activations = match count {
                    SubroutineBreakCount::Fixed(n) => pending.div_ceil((*n).max(1)),
                    SubroutineBreakCount::All => 1,
                };
                keep_min(&mut cheapest_break, credits * activations);
            }
            Effect::BoostStrength { amount, .. } => {
                keep_min(&mut cheapest_pump, credits * shortfall.div_ceil((*amount).max(1)));
            }
            Effect::BoostStrengthAmount { .. } => keep_min(&mut cheapest_pump, credits * shortfall),
            _ => {}
        });
    }
    let pump = if shortfall == 0 { 0 } else { cheapest_pump? };
    Some(cheapest_break? + pump)
}

/// Subroutines on `ice` still waiting to be broken or resolved.
fn pending_on(ice: &RunIce) -> u32 {
    ice.subroutines.iter().filter(|s| s.status == SubroutineStatus::Pending).count() as u32
}

/// How far the strongest rig breaker able to break the encountered ICE's
/// subtype falls short of its strength — zero when a breaker matches or
/// exceeds it, when no breaker matches at all (there is nothing to pump),
/// or outside an encounter.
fn strength_shortfall(state: &GameState, run: &RunState, registry: &CardRegistry) -> i32 {
    if run.phase != RunPhase::EncounterIce {
        return 0;
    }
    let Some(ice) = run.ice.get(run.position) else { return 0 };
    let best = state
        .runner
        .rig
        .iter()
        .filter(|card| breaks_subtype(state, card, ice, registry))
        .map(|card| continuous::breaker_strength(state, registry, card))
        .max();
    best.map_or(0, |strength| (continuous::ice_strength(state, registry, ice) - strength).max(0))
}

/// Whether `ice` is a `subtype` right now: the one it prints, or one the
/// table gives it (a hosted GAMEDRAGON™ Pro's "host ice gains barrier") —
/// `BreakSubroutines`' own test, so a break the engine would accept is one
/// this prices.
fn ice_is(state: &GameState, ice: &RunIce, subtype: IceType, registry: &CardRegistry) -> bool {
    ice.ice_type == subtype || continuous::ice_gains_subtype(state, registry, ice.install_id, subtype)
}

/// Whether `card`'s abilities include a `BreakSubroutines` that applies to
/// `ice` — restricted to a subtype it has, or unrestricted.
fn breaks_subtype(state: &GameState, card: &netrunner_core::rules::InstalledRunnerCard, ice: &RunIce, registry: &CardRegistry) -> bool {
    let Some(def) = registry.get(&card.card) else { return false };
    let mut found = false;
    for ability in &def.abilities {
        ability.effect.for_each_effect(&mut |effect| {
            if let Effect::BreakSubroutines { restrict_to, .. } = effect
                && restrict_to.is_none_or(|r| ice_is(state, ice, r, registry))
            {
                found = true;
            }
        });
    }
    found
}

/// Subroutines on the currently encountered ICE that have neither been
/// broken nor resolved. Zero outside an encounter.
fn pending_subroutines(run: &RunState) -> usize {
    if run.phase != RunPhase::EncounterIce {
        return 0;
    }
    run.ice.get(run.position).map_or(0, |ice| pending_on(ice) as usize)
}

/// Every effect the card declares anywhere — triggers, abilities and its
/// access interaction — so the recognisers below read a card's *text*
/// rather than its name. That distinction is the whole point: a card
/// named here would be a preference, and `personality` forbids those; a
/// shape recognised here values the next card with the same text for
/// free.
fn for_each_declared_effect(def: &CardDefinition, f: &mut impl FnMut(&Effect)) {
    let declared = def
        .triggers
        .iter()
        .flat_map(|trigger| trigger.effects.iter())
        .chain(def.abilities.iter().map(|ability| &ability.effect))
        .chain(def.interactive_on_access.iter().flat_map(|access| access.effects.iter()));
    for effect in declared {
        effect.for_each_effect(f);
    }
}

/// An ambush that grows: its damage *is* its advancement-token count.
fn damage_grows_with_advancement(def: &CardDefinition) -> bool {
    let mut found = false;
    for_each_declared_effect(def, &mut |effect| {
        if matches!(effect, Effect::DealDamageAmount(_, Amount::HostedAdvancementTokens)) {
            found = true;
        }
    });
    found
}

/// An ambush at all: a non-agenda that answers being accessed with
/// damage, whether through an `OnAccessed` trigger (*Urtica Cipher*) or
/// a paid access interaction (*Snare!*).
fn punishes_access_with_damage(def: &CardDefinition) -> bool {
    // An agenda that hurts on access is not an ambush — the Runner takes
    // the points anyway — and an identity is never an install, so neither
    // is a card this term should price.
    if matches!(def.card_type, CardType::Agenda | CardType::Identity) {
        return false;
    }
    let deals_damage = |effect: &Effect| matches!(effect, Effect::DealDamage(..) | Effect::DealDamageAmount(..));
    let mut found = false;
    let on_access = def
        .triggers
        .iter()
        .filter(|trigger| trigger.trigger == Trigger::OnAccessed)
        .flat_map(|trigger| trigger.effects.iter())
        .chain(def.interactive_on_access.iter().flat_map(|access| access.effects.iter()));
    for effect in on_access {
        effect.for_each_effect(&mut |e| found |= deals_damage(e));
    }
    found
}

fn corp_install_value(installed: &InstalledCard, registry: &CardRegistry, w: &Weights, rig: [bool; 3]) -> f64 {
    let def = registry.get(&installed.card);
    let is_ice = def.is_some_and(|d| matches!(d.card_type, CardType::Ice(_)));
    let mut value = if installed.rezzed {
        w.board_presence_weight + if is_ice { w.rezzed_ice_weight } else { w.rezzed_asset_weight }
    } else {
        w.unrezzed_install_weight
    };
    // What the rig cannot break is what holds — and only once it is face
    // up, which is the whole point: a term paid on the face-down card too
    // is present on both sides of the rez and cancels out of the decision
    // it exists to win. Measured that way first, and it moved nothing
    // (`RezIce` 1,073 → 1,053, Corp 31 → 29 of 192). See
    // `UNBREAKABLE_ICE_WEIGHT`.
    if installed.rezzed
        && let Some(def) = def
        && let CardType::Ice(subtype) = &def.card_type
    {
        if !rig[subtype_slot(*subtype)] {
            value += w.unbreakable_ice_weight;
        }
        let etr = def.subroutines.iter().filter(|sub| sub.effect.can_end_the_run()).count();
        value += etr as f64 * w.etr_subroutine_weight;
    }
    if let Some(required) = def.and_then(|d| d.advancement_requirement) {
        value += installed.advancement_tokens.min(required) as f64 * w.advancement_weight;
        // Past the requirement a token is worth what it will *become* at
        // score time — `dividends` agenda counters each — and nothing at
        // all on an agenda that pays none. Valuing the promise rather
        // than adding a second constant keeps the two halves of one
        // mechanic on one weight: the same token is counted here while
        // the agenda is installed and by `scored_agenda_counters` after.
        let dividends = def.and_then(|d| d.dividends).unwrap_or(0);
        if dividends > 0 {
            let excess = installed.advancement_tokens.saturating_sub(required);
            value += f64::from(excess * dividends) * w.agenda_counter_weight;
        }
    }
    if let Some(def) = def {
        // An ambush's `advancement_requirement` is `Some(0)`, so the line
        // above counted every one of its tokens as nothing — see
        // `AMBUSH_ADVANCEMENT_WEIGHT`. Capped rather than open-ended
        // because nothing else stops the Corp advancing it.
        if damage_grows_with_advancement(def) {
            let counted = installed.advancement_tokens.min(w.ambush_advancement_cap);
            value += f64::from(counted) * w.ambush_advancement_weight;
        }
        // Face-down and dangerous. Only while unrezzed: once it is turned
        // up it is a known quantity and the Runner simply stops running
        // at it.
        if w.ambush_weight != 0.0 && !installed.rezzed && punishes_access_with_damage(def) {
            value += w.ambush_weight;
        }
    }
    value
}

/// A **lower bound** on what resolving the `PendingDecision` `side`
/// currently owes will add to this same evaluation — zero whenever the
/// decision's continuation is one this evaluator cannot price, which is
/// most of them.
///
/// A bound rather than an estimate, and that is the whole design: the
/// caller credits this alongside `PENDING_DECISION_UPSIDE_WEIGHT`, and
/// over-crediting a parked prompt makes sitting in it better than
/// resolving it — the wander `UNRESOLVED_DECISION_WEIGHT` exists to
/// stop. Everything below therefore prices the *worst* resolution the
/// decision allows, never the one the agent would like, and
/// `continuation_upside` is a **whitelist**: an effect it does not
/// recognise makes the whole continuation unpriceable rather than
/// contributing zero to a sum. Measured, on the branch that added this:
/// with a catch-all instead, *Touch-ups* stalled the view sweep at seed
/// 7 for the full 10,000 actions. Its `then` advances a card by two
/// (priced, +3.0) and *then* parks a `PresentChoice` (worth zero to this
/// term but −`UNRESOLVED_DECISION_WEIGHT` to the state), so confirming
/// the selection was 1.5 worse than sitting in it and the Corp toggled
/// position 4 on and off until the budget ran out. A continuation that
/// parks a further decision therefore has to be unpriceable, and a
/// whitelist gets that right for every future `Effect` variant without
/// anyone remembering to classify it.
///
/// Only `ChooseCards` is read. `ChooseEffect` gates a set of options
/// this would have to price one by one and take the worst of;
/// `ChooseServer` and `ChooseTriggerOrder` gate a run target and an
/// ordering rather than a value. All three keep the plain penalty they
/// have always had.
fn pending_decision_upside(state: &GameState, side: Side, registry: &CardRegistry, w: &Weights) -> f64 {
    let Some(PendingDecision::ChooseCards { side: chooser, source, filter, min, destination, then, .. }) =
        &state.pending_decision
    else {
        return 0.0;
    };
    if *chooser != side {
        return 0.0;
    }
    // Cards this selection takes out of R&D, checked where the HQ term
    // is: that term switches off entirely below `rd_draw_reserve`, so a
    // bound that moved two cards into HQ without noticing R&D had
    // crossed the reserve would over-credit exactly the draw it prices.
    let rd_spent = if matches!(source, CardZoneRef::OwnRAndD) && destination.is_some() { *min as usize } else { 0 };
    let moved = match destination {
        // No destination means no cards move — the selection names
        // targets for the `then` and leaves them where they are.
        None => 0.0,
        Some(destination) => {
            zone_size_value(state, side, w, destination, i64::from(*min), rd_spent)
                + zone_size_value(state, side, w, source, -i64::from(*min), rd_spent)
        }
    };
    match then {
        None => moved,
        Some(then) => match continuation_upside(state, side, registry, w, source, filter, then, rd_spent) {
            Some(value) => moved + value,
            None => 0.0,
        },
    }
}

/// What the `then` of a parked selection is worth, or `None` when any
/// part of it is an effect this evaluator does not price — see
/// `pending_decision_upside` for why an unrecognised effect has to
/// poison the whole continuation rather than count as zero. `EndTheRun`
/// was the conspicuous absentee until the Corp got a run term at all
/// (`ACTIVE_RUN_AGAINST_WEIGHT`); it is priced here now, which is what
/// lets *Anoetic Void*'s "discard 2 from HQ to end the run" be accepted.
#[allow(clippy::too_many_arguments)]
fn continuation_upside(
    state: &GameState,
    side: Side,
    registry: &CardRegistry,
    w: &Weights,
    source: &CardZoneRef,
    filter: &CardFilter,
    effect: &Effect,
    rd_spent: usize,
) -> Option<f64> {
    match effect {
        Effect::Sequence(effects) => effects
            .iter()
            .map(|effect| continuation_upside(state, side, registry, w, source, filter, effect, rd_spent))
            .sum(),
        Effect::GainCredits(gains, amount) if *gains == side => Some(f64::from(*amount) * w.own_credit_weight),
        Effect::DrawCards(draws, amount) if *draws == side => Some(zone_size_value(
            state,
            side,
            w,
            &own_hand_zone(side),
            i64::from(*amount),
            rd_spent + *amount as usize,
        )),
        Effect::PlaceAdvancementCounters(amount) => {
            Some(advancement_upside(state, side, registry, w, source, filter, *amount))
        }
        // Exact rather than a bound, unusually for this function: ending
        // the run removes precisely the penalty the Corp branch applies,
        // so the continuation is worth the term and nothing else. Zero
        // when there is no run the Corp is paying for — an offer to end a
        // run the Runner cannot finish anyway buys nothing.
        Effect::EndTheRun if side == Side::Corp => Some(
            state
                .active_run
                .as_ref()
                .filter(|run| run_is_breakable(state, run, registry))
                .map_or(0.0, |_| w.active_run_against_weight),
        ),
        _ => None,
    }
}

fn own_hand_zone(side: Side) -> CardZoneRef {
    match side {
        Side::Corp => CardZoneRef::OwnHq,
        Side::Runner => CardZoneRef::OwnGrip,
    }
}

/// What `delta` cards arriving in (or leaving) `zone` are worth. Only the
/// two hands are worth anything at all here, and only through the
/// shortfall terms that already price them — a card above the floor is
/// worth nothing to this evaluator, so a draw into a healthy hand is
/// correctly credited zero rather than optimistically.
fn zone_size_value(
    state: &GameState,
    side: Side,
    w: &Weights,
    zone: &CardZoneRef,
    delta: i64,
    rd_spent: usize,
) -> f64 {
    let (held, floor, weight) = match (side, zone) {
        // Mirrors the live term, `rd_draw_reserve` guard included.
        (Side::Corp, CardZoneRef::OwnHq) => {
            if state.corp.r_and_d.len() < w.rd_draw_reserve + rd_spent {
                return 0.0;
            }
            (state.corp.hq.len(), w.hq_floor, w.hq_shortfall_weight)
        }
        (Side::Runner, CardZoneRef::OwnGrip) => (state.runner.grip.len(), w.grip_floor, w.grip_shortfall_weight),
        _ => return 0.0,
    };
    let after = held.saturating_add_signed(delta as isize);
    let before_shortfall = floor.saturating_sub(held) as f64;
    let after_shortfall = floor.saturating_sub(after) as f64;
    (before_shortfall - after_shortfall) * weight
}

/// What another advancement token is worth on the *worst* card the parked
/// selection could put it on. `card_matches_filter` is the
/// definition-level half of the filter — its instance-level clauses
/// (`Unrezzed`, `TopOfZone`) pass everything — so the cards walked here
/// are a **superset** of the eligible ones, which is exactly what keeps
/// the minimum over them a lower bound.
///
/// The evaluator cannot see which card the selection will land on: a
/// toggle changes `selected`, which nothing here scores. So the worst
/// eligible target is not pessimism, it is the honest reading — an
/// agenda already at its requirement or an ambush at
/// `ambush_advancement_cap` gains nothing from another token, and where
/// one of those is eligible this term is correctly zero.
fn advancement_upside(
    state: &GameState,
    side: Side,
    registry: &CardRegistry,
    w: &Weights,
    source: &CardZoneRef,
    filter: &CardFilter,
    amount: u32,
) -> f64 {
    if side != Side::Corp || !matches!(source, CardZoneRef::OwnInstalled) {
        return 0.0;
    }
    let mut worst: Option<f64> = None;
    // Advancing does not change what the rig covers, so the unbreakable
    // term is the same on both sides of the delta and cancels; the real
    // coverage is passed anyway rather than a stand-in that only happens
    // to cancel today.
    let rig = rig_coverage(state, registry);
    for installed in &state.corp.installed {
        let Some(def) = registry.get(&installed.card) else { return 0.0 };
        if !card_matches_filter(def, filter) {
            continue;
        }
        let mut advanced = installed.clone();
        advanced.advancement_tokens += amount;
        let delta = corp_install_value(&advanced, registry, w, rig) - corp_install_value(installed, registry, w, rig);
        worst = Some(worst.map_or(delta, |worst: f64| worst.min(delta)));
    }
    worst.unwrap_or(0.0).max(0.0)
}

/// Agenda counters sitting on the Corp's scored agendas — Dividends,
/// spent by the agendas' own discard-phase triggers.
///
/// Without this the term above would evaporate at the moment it paid
/// off: a search comparing "score now" with "advance once more, then
/// score" sees the installed agenda gone in both branches, so unless the
/// counters survive into the score area the two branches are worth the
/// same and the extra click is pure cost.
fn scored_agenda_counters(state: &GameState) -> u32 {
    state.corp.scored_agendas.iter().map(|scored| scored.agenda_counters).sum()
}

/// Installed, unscored agendas — what `installed_agenda_weight` counts.
fn installed_agendas(state: &GameState, registry: &CardRegistry) -> usize {
    use netrunner_core::rules::InstallSlot;
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Root)
        .filter(|card| registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda))
        .count()
}

/// ICE in front of each installed, unscored agenda, each server's count
/// capped at `cap` (`Weights::agenda_protection_cap`), summed over agendas.
fn protected_agenda_ice(state: &GameState, registry: &CardRegistry, cap: usize) -> usize {
    use netrunner_core::rules::InstallSlot;
    state
        .corp
        .installed
        .iter()
        .filter(|card| card.slot == InstallSlot::Root)
        .filter(|card| registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda))
        .map(|agenda| {
            state
                .corp
                .installed
                .iter()
                .filter(|ice| ice.slot == InstallSlot::Ice && ice.server == agenda.server)
                .count()
                .min(cap)
        })
        .sum()
}

/// How many of the three ICE subtypes the rig can break: a rig card whose
/// abilities contain `Effect::BreakSubroutines` covers its `restrict_to`
/// subtype, or all three when unrestricted (an AI breaker).
///
/// Public for the same reason `is_unrezzed_threat` is: `netrunner_cli diag
/// tempo` reports how far along its rig the Runner is when it starts
/// running, and a diagnostic that re-derived "coverage" itself would be
/// measuring its own copy rather than the term the Runner actually reads.
pub fn breaker_coverage(state: &GameState, registry: &CardRegistry) -> usize {
    rig_coverage(state, registry).iter().filter(|c| **c).count()
}

/// The subtypes the rig can break, as `covers` flags OR-ed over every rig
/// card.
fn rig_coverage(state: &GameState, registry: &CardRegistry) -> [bool; 3] {
    let mut covered = [false; 3];
    for card in &state.runner.rig {
        let Some(def) = registry.get(&card.card) else { continue };
        for (slot, flag) in covers(def).into_iter().enumerate() {
            covered[slot] |= flag;
        }
    }
    covered
}

/// Which ICE subtypes `def` can break — indexed Barrier, Code Gate,
/// Sentry. `IceType` is not `Hash`, and three flags say it more plainly
/// than a set would anyway. Shared with `observation`'s rig and coverage
/// blocks so the network is shown exactly what this evaluator counts.
pub(crate) fn covers(def: &CardDefinition) -> [bool; 3] {
    let mut covered = [false; 3];
    for ability in &def.abilities {
        ability.effect.for_each_effect(&mut |effect| {
            if let Effect::BreakSubroutines { restrict_to, .. } = effect {
                match restrict_to {
                    Some(subtype) => covered[subtype_slot(*subtype)] = true,
                    None => covered = [true; 3],
                }
            }
        });
    }
    covered
}

/// A subtype's index into a `covers`/`rig_coverage` flag array. Shared so
/// that the Corp's `UNBREAKABLE_ICE_WEIGHT` indexes the same array the
/// Runner's `BREAKER_COVERAGE_WEIGHT` fills, rather than each end keeping
/// its own copy of the order.
fn subtype_slot(subtype: IceType) -> usize {
    match subtype {
        IceType::Barrier => 0,
        IceType::CodeGate => 1,
        IceType::Sentry => 2,
    }
}

/// What the Runner's grip is worth, summed over `install_delta` and
/// floored at zero per card; scaled by `held_card_weight`.
fn held_cards_value(state: &GameState, registry: &CardRegistry, w: &Weights) -> f64 {
    let rig = rig_coverage(state, registry);
    state
        .runner
        .grip
        .iter()
        .filter_map(|card| registry.get(card))
        .map(|def| install_delta(def, rig, w).max(0.0))
        .sum()
}

/// What this evaluator would credit the Runner for installing `def` from
/// its grip, credits and memory spent included: presence, plus coverage
/// for each ICE subtype the card breaks that the rig (`rig`) cannot,
/// minus the printed cost and the memory it takes. The same arithmetic
/// the install itself scores (the grip term aside), so a card is "live"
/// in hand exactly when the Runner would install it — leaving the memory
/// out made a held Cleaver worth more than the installed one and the
/// install a net loss. Zero for anything that is not a program, hardware
/// or resource.
fn install_delta(def: &CardDefinition, rig: [bool; 3], w: &Weights) -> f64 {
    if !matches!(def.card_type, CardType::Program | CardType::Hardware | CardType::Resource) {
        return 0.0;
    }
    let new_coverage = covers(def).iter().zip(rig).filter(|(grip, rig)| **grip && !rig).count();
    w.board_presence_weight + new_coverage as f64 * w.breaker_coverage_weight
        - f64::from(def.cost) * w.own_credit_weight
        - f64::from(def.memory_cost.unwrap_or(0)) * w.memory_weight
}

/// Credits the Runner is short of the cheapest grip breaker worth
/// installing: one that covers a subtype the rig cannot break and fits in
/// free memory. Zero with no such card, or once it is affordable. Printed
/// cost, ignoring install discounts — over-estimating the target only
/// makes the Runner save one click longer.
fn breaker_savings_shortfall(state: &GameState, registry: &CardRegistry) -> u32 {
    let rig = rig_coverage(state, registry);
    let target = state
        .runner
        .grip
        .iter()
        .filter_map(|card| registry.get(card))
        .filter(|def| def.memory_cost.unwrap_or(0) <= state.runner.memory_units.0)
        .filter(|def| covers(def).iter().zip(rig).any(|(grip, rig)| *grip && !rig))
        .map(|def| def.cost)
        .min();
    target.map_or(0, |cost| cost.saturating_sub(state.runner.resources.credits.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{AbilityDef, CardDefinition, CardId, DamageType, SubroutineBreakCount, Trigger, TriggeredEffect};
    use netrunner_core::rules::{AgendaPoints, Credits, GameState, InstallId, InstalledRunnerCard};

    fn empty() -> CardRegistry {
        CardRegistry::new()
    }

    /// A payment parked on a card question is scored as the payment made,
    /// with the pick its payer likes best — here the Runner keeps the
    /// program (held, it is worth something) and trashes the event — not
    /// as the untouched board the parked state is.
    #[test]
    fn a_parked_card_payment_is_scored_as_the_payers_best_answer() {
        use netrunner_core::dsl::{Cost, CardFilter, CardZoneRef, Effect};
        use netrunner_core::rules::{apply_action, Clicks, GamePhase, PlayerAction};
        let card = |id: &str, card_type: CardType| CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Runner,
            card_type,
            is_playable: true,
            ..Default::default()
        };
        let mut trasher = card("trasher", CardType::Resource);
        trasher.abilities = vec![AbilityDef {
            trigger: Trigger::Paid,
            cost: Some(Cost::Trash { from: CardZoneRef::OwnGrip, filter: CardFilter::Any, count: 1, reveal: false }),
            text: None,
            requirement: None,
            effect: Effect::GainCredits(Side::Runner, 5),
            cost_discount_if: None,
            used_by: None,
            access: false,
        }];
        let registry = CardRegistry::from_cards(vec![trasher, card("an_event", CardType::Event), card("a_program", CardType::Program)]);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(4);
        state.runner.grip = vec![CardId("an_event".to_string()), CardId("a_program".to_string())];
        state.runner.rig = vec![InstalledRunnerCard { card: CardId("trasher".to_string()), install_id: InstallId(2001), ..Default::default() }];

        let (parked, _) = apply_action(&state, &registry, PlayerAction::ActivateAbility { target: InstallId(2001), ability_index: 0 }).expect("asks");
        assert!(parked.pending_payment.is_some());
        let w = Weights::default();
        let scores: Vec<f64> = (0..2)
            .map(|position| {
                let (paid, _) = apply_action(&parked, &registry, PlayerAction::ToggleCardSelection { position }).expect("pays");
                evaluate_state_with(&paid, Side::Runner, &registry, &w)
            })
            .collect();
        let best = scores.iter().copied().fold(f64::MIN, f64::max);
        assert_eq!(evaluate_state_with(&parked, Side::Runner, &registry, &w), best);
        assert!(evaluate_state_with(&parked, Side::Runner, &registry, &w) > evaluate_state_with(&state, Side::Runner, &registry, &w), "5 credits, paid for");
    }

    #[test]
    fn game_over_returns_win_or_loss_constant_regardless_of_other_fields() {
        let mut state = GameState::new(0);
        state.phase = GamePhase::GameOver(Side::Corp);
        state.runner.tags = 10;
        state.corp.bad_publicity = 10;

        assert_eq!(evaluate_state(&state, Side::Corp, &empty()), WIN_SCORE);
        assert_eq!(evaluate_state(&state, Side::Runner, &empty()), -WIN_SCORE);
    }

    #[test]
    fn agenda_point_lead_favors_the_leading_side() {
        let mut state = GameState::new(0);
        state.corp.resources.agenda_points = AgendaPoints(4);

        assert!(evaluate_state(&state, Side::Corp, &empty()) > 0.0);
        assert!(evaluate_state(&state, Side::Runner, &empty()) < 0.0);
    }

    #[test]
    fn corp_bad_publicity_lowers_the_corp_score() {
        let clean = GameState::new(0);
        let mut dirty = GameState::new(0);
        dirty.corp.bad_publicity = 3;

        assert!(evaluate_state(&clean, Side::Corp, &empty()) > evaluate_state(&dirty, Side::Corp, &empty()));
    }

    #[test]
    fn runner_tags_lower_the_runner_score() {
        let clean = GameState::new(0);
        let mut tagged = GameState::new(0);
        tagged.runner.tags = 2;

        assert!(evaluate_state(&clean, Side::Runner, &empty()) > evaluate_state(&tagged, Side::Runner, &empty()));
    }

    fn ice(id: &str, cost: u32) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::Barrier),
            cost,
            is_playable: true,
            ..Default::default()
        }
    }

    fn breaker(id: &str, restrict_to: Option<IceType>) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Runner,
            card_type: CardType::Program,
            abilities: vec![AbilityDef {
                text: None,
                trigger: Trigger::Paid,
                cost: None,
                requirement: None,
                effect: Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to },
                cost_discount_if: None, used_by: None, access: false }],
            is_playable: true,
            ..Default::default()
        }
    }

    fn rig_card(id: &str) -> InstalledRunnerCard {
        InstalledRunnerCard { card: CardId(id.to_string()), ..Default::default() }
    }

    /// The whole point of the breaker term: a first breaker for a subtype
    /// is worth installing over a credit; a second for the same subtype
    /// adds nothing.
    #[test]
    fn breaker_coverage_counts_each_ice_subtype_once() {
        let registry = CardRegistry::from_cards(vec![
            breaker("cleaver", Some(IceType::Barrier)),
            breaker("corroder", Some(IceType::Barrier)),
            breaker("carmen", Some(IceType::Sentry)),
            breaker("mayfly", None),
        ]);
        let mut state = GameState::new(0);
        assert_eq!(breaker_coverage(&state, &registry), 0);
        state.runner.rig = vec![rig_card("cleaver")];
        assert_eq!(breaker_coverage(&state, &registry), 1);
        state.runner.rig.push(rig_card("corroder"));
        assert_eq!(breaker_coverage(&state, &registry), 1, "a second Barrier breaker covers nothing new");
        state.runner.rig.push(rig_card("carmen"));
        assert_eq!(breaker_coverage(&state, &registry), 2);
        state.runner.rig.push(rig_card("mayfly"));
        assert_eq!(breaker_coverage(&state, &registry), 3, "an AI breaker covers everything");
    }

    #[test]
    fn a_first_breaker_beats_a_credit_click_and_a_duplicate_does_not() {
        let registry = CardRegistry::from_cards(vec![breaker("cleaver", Some(IceType::Barrier))]);
        let mut clicked = GameState::new(0);
        clicked.runner.resources.credits = Credits(6);
        let mut installed = GameState::new(0);
        installed.runner.resources.credits = Credits(2); // paid 3 for Cleaver, no click credit
        installed.runner.rig = vec![rig_card("cleaver")];
        assert!(evaluate_state(&installed, Side::Runner, &registry) > evaluate_state(&clicked, Side::Runner, &registry));

        let mut second = installed.clone();
        second.runner.resources.credits = Credits(0);
        second.runner.rig.push(rig_card("cleaver"));
        let mut clicked_instead = installed.clone();
        clicked_instead.runner.resources.credits = Credits(3);
        assert!(evaluate_state(&clicked_instead, Side::Runner, &registry) > evaluate_state(&second, Side::Runner, &registry));
    }

    /// Two agendas, identical but for `dividends`, at the same tokens.
    fn advancement_value_at(dividends: Option<u32>) -> impl Fn(u32) -> f64 {
        let mut agenda = ice("offworld_office", 0);
        agenda.card_type = CardType::Agenda;
        agenda.advancement_requirement = Some(3);
        agenda.dividends = dividends;
        let registry = CardRegistry::from_cards(vec![agenda]);
        move |tokens| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId("offworld_office".to_string()),
                install_id: InstallId(1),
                advancement_tokens: tokens,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        }
    }

    #[test]
    fn advancement_is_valued_up_to_the_requirement_and_no_further() {
        let at = advancement_value_at(None);
        assert!(at(1) > at(0));
        assert!(at(3) > at(2));
        assert_eq!(at(4), at(3), "tokens past the requirement are worth nothing — score instead");
    }

    /// The exception, and the reason `agenda_counter_weight` exists: on a
    /// Dividends agenda an excess token is not waste, it is one counter
    /// per `dividends` at score time. It must be worth more than the
    /// credit-click it costs (0.4 + 0.4) and far less than a point of
    /// agenda, so the Corp still takes the points.
    #[test]
    fn a_token_past_the_requirement_is_worth_its_dividends_and_no_more() {
        let plain = advancement_value_at(None);
        let dividend = advancement_value_at(Some(1));
        assert_eq!(dividend(3), plain(3), "up to the requirement the two agendas are identical");
        assert!(dividend(4) > dividend(3) + 0.8, "an excess token must beat the click and credit it costs");
        assert!(dividend(4) - dividend(3) < AGENDA_POINT_WEIGHT, "and never rival taking the points");
        assert_eq!(dividend(5) - dividend(4), dividend(4) - dividend(3), "each excess token is worth the same");

        let double = advancement_value_at(Some(2));
        assert!(
            (double(4) - double(3) - 2.0 * (dividend(4) - dividend(3))).abs() < f64::EPSILON,
            "two counters per token is worth twice one"
        );
    }

    /// The ambush recogniser reads a card's *text*, so it must fire on a
    /// card that grows its damage and not on one that merely deals it.
    #[test]
    fn an_ambush_is_recognised_by_its_text_and_a_plain_damage_card_is_not() {
        let mut growing = ice("urtica", 0);
        growing.card_type = CardType::Asset;
        growing.advancement_requirement = Some(0);
        growing.triggers = vec![TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false,
            text: None,
            trigger: Trigger::OnAccessed,
            effects: vec![Effect::DealDamageAmount(DamageType::Net, Amount::HostedAdvancementTokens)],
            requirement: None,
        }];
        assert!(damage_grows_with_advancement(&growing));
        assert!(punishes_access_with_damage(&growing));

        let mut flat = growing.clone();
        flat.triggers[0].effects = vec![Effect::DealDamage(DamageType::Net, 3)];
        assert!(!damage_grows_with_advancement(&flat), "a fixed amount does not grow");
        assert!(punishes_access_with_damage(&flat), "but it is still an ambush");

        let mut elsewhere = growing.clone();
        elsewhere.triggers[0].trigger = Trigger::OnTurnStart;
        assert!(damage_grows_with_advancement(&elsewhere));
        assert!(!punishes_access_with_damage(&elsewhere), "damage on your own turn is not an access punish");

        let mut agenda = growing.clone();
        agenda.card_type = CardType::Agenda;
        assert!(!punishes_access_with_damage(&agenda), "an agenda that hurts on access is not an ambush to install");
    }

    /// The zero this term exists to remove. An ambush carries
    /// `advancement_requirement: Some(0)`, so every token it held was
    /// counted by `.min(required)` as nothing and no Corp ever advanced
    /// one — while the card's damage *is* that count.
    #[test]
    fn advancing_an_ambush_is_worth_more_than_the_click_it_costs_and_stops_at_the_cap() {
        let mut ambush = ice("clearinghouse", 0);
        ambush.card_type = CardType::Asset;
        ambush.advancement_requirement = Some(0);
        ambush.triggers = vec![TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false,
            text: None,
            trigger: Trigger::OnTurnStart,
            effects: vec![Effect::DealDamageAmount(DamageType::Meat, Amount::HostedAdvancementTokens)],
            requirement: None,
        }];
        let registry = CardRegistry::from_cards(vec![ambush]);
        let at = |tokens| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId("clearinghouse".to_string()),
                install_id: InstallId(1),
                advancement_tokens: tokens,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        let cap = Weights::default().ambush_advancement_cap;
        assert!(at(1) > at(0) + 0.8, "advancing must beat the click and credit it costs");
        assert_eq!(at(cap + 1), at(cap), "and stop at the cap — nothing else would stop it");
    }

    /// `ambush_weight` is a profile term, off for the balanced Corp, and
    /// it applies only while the card is face down.
    #[test]
    fn a_face_down_ambush_is_worth_more_only_to_a_corp_that_plays_for_damage() {
        let mut ambush = ice("snare", 0);
        ambush.card_type = CardType::Asset;
        ambush.triggers = vec![TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false,
            text: None,
            trigger: Trigger::OnAccessed,
            effects: vec![Effect::DealDamage(DamageType::Net, 3)],
            requirement: None,
        }];
        let registry = CardRegistry::from_cards(vec![ambush]);
        let value = |rezzed, w: &Weights| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId("snare".to_string()),
                install_id: InstallId(1),
                rezzed,
                ..Default::default()
            }];
            evaluate_state_with(&state, Side::Corp, &registry, w)
        };
        let base = Weights::default();
        assert_eq!(value(false, &base), value(false, &base), "balanced is indifferent — the term is zero");
        let trap = crate::Personality::Trap.weights();
        assert!(value(false, &trap) > value(false, &base) + base.unrezzed_install_weight - trap.unrezzed_install_weight);
        let rezzed_trap = value(true, &trap);
        assert!(rezzed_trap < value(false, &trap), "face up it is a known quantity, not a threat");
    }

    /// The half that makes the first half survive being cashed in: a
    /// search comparing "score now" with "advance once more, then score"
    /// sees the install gone either way, so the counters have to be worth
    /// something in the score area or the extra click is pure cost.
    #[test]
    fn counters_on_a_scored_agenda_outlive_the_install_that_carried_them() {
        use netrunner_core::rules::ScoredAgenda;
        let registry = CardRegistry::from_cards(vec![]);
        let scored = |agenda_counters| {
            let mut state = GameState::new(0);
            state.corp.scored_agendas = vec![ScoredAgenda {
                card: CardId("offworld_office".to_string()),
                install_id: InstallId(1),
                agenda_counters,
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(scored(1) > scored(0) + 0.8, "a counter carried into the score area beats the click that bought it");
        assert_eq!(scored(2) - scored(1), scored(1) - scored(0));
    }

    /// Rezzing a mid-cost ICE at approach must beat passing, and installing
    /// must beat clicking for a credit; that is what makes heuristic play
    /// reach an encounter at all.
    #[test]
    fn rezzing_a_three_cost_ice_beats_passing_and_installing_beats_a_credit() {
        let registry = CardRegistry::from_cards(vec![ice("palisade", 3)]);
        let installed = |rezzed, credits| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.installed = vec![InstalledCard {
                card: CardId("palisade".to_string()),
                install_id: InstallId(1),
                rezzed,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(installed(true, 2) > installed(false, 5), "paying 3 to rez Palisade is progress");
        let mut in_hand = GameState::new(0);
        in_hand.corp.resources.credits = Credits(6);
        in_hand.corp.hq = vec![CardId("palisade".to_string())];
        assert!(installed(false, 5) > evaluate_state(&in_hand, Side::Corp, &registry), "installing beats a credit");
    }

    /// Starting a run must beat a credit, jacking out must lose the run
    /// bonus, and unbroken subroutines in an encounter must count against
    /// the Runner — that is what makes a heuristic Runner run at all, and
    /// use the breakers it installs.
    #[test]
    fn a_run_in_progress_beats_a_credit_and_pending_subroutines_count_against_it() {
        use netrunner_core::rules::{EncounteredSubroutine, RunIce, ServerId};
        let registry = CardRegistry::new();
        let mut clicked = GameState::new(0);
        clicked.runner.resources.credits = Credits(1);
        let mut running = GameState::new(0);
        running.corp.hq = corp_cards("hq", 1);
        clicked.corp.hq = corp_cards("hq", 1);
        running.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
        assert!(evaluate_state(&running, Side::Runner, &registry) > evaluate_state(&clicked, Side::Runner, &registry));

        let sub = |id| EncounteredSubroutine {
            id,
            definition: netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
            status: SubroutineStatus::Pending,
        };
        let mut encountering = running.clone();
        encountering.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            position: 0,
            ice: vec![RunIce {
                install_id: netrunner_core::rules::InstallId::PLACEHOLDER,
                card_id: CardId("ice_wall".to_string()),
                ice_type: IceType::Barrier,
                subroutines: vec![sub(0), sub(1), sub(2)],
                rezzed: true,
            }],
            ..Default::default()
        });
        let mut jacked_out = GameState::new(0);
        jacked_out.active_run = None;
        assert!(
            evaluate_state(&jacked_out, Side::Runner, &registry) > evaluate_state(&encountering, Side::Runner, &registry),
            "three unbroken subroutines are worse than no run"
        );
        let mut broke_one = encountering.clone();
        broke_one.runner.resources.credits = Credits(0); // paid 1 for it
        encountering.runner.resources.credits = Credits(1);
        broke_one.active_run.as_mut().unwrap().ice[0].subroutines[0].status = SubroutineStatus::Broken;
        assert!(
            evaluate_state(&broke_one, Side::Runner, &registry) > evaluate_state(&encountering, Side::Runner, &registry),
            "paying a credit to break a subroutine is worth it"
        );
    }

    /// A parked decision the side must resolve scores below the same
    /// board with nothing parked, so confirming a selection is preferred
    /// to toggling it back and forth.
    #[test]
    fn an_unresolved_decision_of_ones_own_costs_something() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision};
        let registry = CardRegistry::new();
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Runner);
        let mut parked = clear.clone();
        parked.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Runner,
            source: CardZoneRef::OwnGrip,
            filter: CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected: Vec::new(),
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        assert!(evaluate_state(&parked, Side::Runner, &registry) < evaluate_state(&clear, Side::Runner, &registry));
        assert_eq!(
            evaluate_state(&parked, Side::Corp, &registry),
            evaluate_state(&clear, Side::Corp, &registry),
            "the opponent's parked decision is not the Corp's problem"
        );
    }

    /// The structural guarantee behind `PENDING_DECISION_UPSIDE_WEIGHT`,
    /// asserted on the constants rather than inferred from a position:
    /// while the allowance stays under the penalty it is credited
    /// alongside, and the upside it accompanies is a lower bound on what
    /// resolving delivers, resolving a prompt always beats sitting in it.
    /// Raise this above `UNRESOLVED_DECISION_WEIGHT` and the toggle-walk
    /// that weight exists to stop comes back.
    #[test]
    fn the_prompt_allowance_stays_under_the_penalty_it_offsets() {
        let w = Weights::default();
        assert!(w.pending_decision_upside_weight < w.unresolved_decision_weight);
    }

    fn advanceable(id: &str, required: u32) -> CardDefinition {
        CardDefinition {
            card_type: CardType::Agenda,
            advancement_requirement: Some(required),
            agenda_points: Some(2),
            ..ice(id, 0)
        }
    }

    /// PT Untaian's shape: a parked selection over the Corp's own
    /// advanceable installs whose `then` puts a token on whichever it
    /// picks. Before this term the prompt was pure cost, so the paid
    /// choice that hands it over was declined every time it was offered
    /// (332 of 332 across 192 heuristic-vs-heuristic games).
    fn parked_advancement_prompt(installed: Vec<InstalledCard>) -> GameState {
        use netrunner_core::rules::PendingChoiceResume;
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.installed = installed;
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnInstalled,
            filter: CardFilter::All(vec![CardFilter::Advanceable, CardFilter::Unrezzed]),
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: Some(Box::new(Effect::PlaceAdvancementCounters(1))),
            selected: Vec::new(),
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        state
    }

    fn under_requirement() -> InstalledCard {
        InstalledCard { card: CardId("under".to_string()), install_id: InstallId(1), ..Default::default() }
    }

    #[test]
    fn a_parked_prompt_that_will_advance_a_card_is_worth_entering_and_still_worth_resolving() {
        let registry = CardRegistry::from_cards(vec![advanceable("under", 3)]);
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Corp);
        clear.corp.installed = vec![under_requirement()];
        let parked = parked_advancement_prompt(vec![under_requirement()]);
        let mut resolved = clear.clone();
        resolved.corp.installed[0].advancement_tokens = 1;

        let score = |state: &GameState| evaluate_state(state, Side::Corp, &registry);
        assert!(
            score(&parked) > score(&clear),
            "a prompt that will land an advancement token is worth being handed"
        );
        assert!(
            score(&resolved) > score(&parked),
            "and resolving it still beats sitting in it — the toggle-walk guarantee"
        );
    }

    /// The bound is over the *worst* target the selection allows, because
    /// the evaluator cannot see which one a toggle will pick. One agenda
    /// already at its requirement among the eligible cards is enough to
    /// price the whole prompt at nothing.
    #[test]
    fn a_prompt_whose_worst_target_gains_nothing_keeps_the_full_penalty() {
        let registry = CardRegistry::from_cards(vec![advanceable("under", 3), advanceable("done", 1)]);
        let finished = InstalledCard {
            card: CardId("done".to_string()),
            install_id: InstallId(2),
            advancement_tokens: 1,
            ..Default::default()
        };
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Corp);
        clear.corp.installed = vec![under_requirement(), finished.clone()];
        let parked = parked_advancement_prompt(vec![under_requirement(), finished]);

        let score = |state: &GameState| evaluate_state(state, Side::Corp, &registry);
        assert!(score(&parked) < score(&clear), "the worst eligible target gains nothing, so neither does the prompt");
    }

    /// *Touch-ups*' shape, and the regression this whole design is
    /// built around: a continuation that advances a card (priced) and
    /// *then* parks a `PresentChoice` (not priced, but charged the
    /// penalty the moment it lands). Crediting the priced half alone
    /// made confirming the selection worse than sitting in it, and the
    /// heuristic Corp toggled one position on and off for the whole
    /// 10,000-action budget (view sweep, seed 7, gimbatul vs
    /// professional_opportunities). An unrecognised effect anywhere in
    /// the continuation has to make the whole thing unpriceable.
    #[test]
    fn a_continuation_that_parks_a_further_decision_is_not_priced_at_all() {
        let registry = CardRegistry::from_cards(vec![advanceable("under", 3)]);
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Corp);
        clear.corp.installed = vec![under_requirement()];

        let mut parked = parked_advancement_prompt(vec![under_requirement()]);
        let priced = evaluate_state(&parked, Side::Corp, &registry);
        let Some(PendingDecision::ChooseCards { then, .. }) = &mut parked.pending_decision else { unreachable!() };
        *then = Some(Box::new(Effect::Sequence(vec![
            Effect::PlaceAdvancementCounters(1),
            Effect::PresentChoice {
                chooser: Side::Corp,
                options: vec![Effect::Sequence(Vec::new()), Effect::Sequence(Vec::new())],
                texts: Vec::new(),
            },
        ])));

        assert!(evaluate_state(&parked, Side::Corp, &registry) < priced);
        assert_eq!(
            evaluate_state(&clear, Side::Corp, &registry) - evaluate_state(&parked, Side::Corp, &registry),
            UNRESOLVED_DECISION_WEIGHT,
            "a continuation that parks another decision keeps the plain penalty"
        );
    }

    /// A prompt whose continuation this evaluator cannot price — most of
    /// them, `EndTheRun` included — is charged the plain penalty exactly
    /// as it was before the term existed.
    #[test]
    fn an_unpriceable_prompt_is_charged_exactly_the_old_penalty() {
        use netrunner_core::rules::PendingChoiceResume;
        let registry = CardRegistry::new();
        let mut clear = GameState::new(0);
        clear.phase = GamePhase::Action(Side::Corp);
        let mut parked = clear.clone();
        parked.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnHq,
            filter: CardFilter::Any,
            min: 2,
            max: 2,
            reveal: false,
            shuffle_after: false,
            destination: Some(CardZoneRef::OwnArchives),
            then: Some(Box::new(Effect::EndTheRun)),
            selected: Vec::new(),
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        assert_eq!(
            evaluate_state(&clear, Side::Corp, &registry) - evaluate_state(&parked, Side::Corp, &registry),
            UNRESOLVED_DECISION_WEIGHT
        );
    }

    /// Pumping a matching breaker up to the ICE's strength is worth its
    /// credits; a breaker for the wrong subtype leaves nothing to pump.
    #[test]
    fn a_strength_shortfall_against_a_matching_breaker_is_worth_pumping() {
        use netrunner_core::rules::{RunIce, ServerId};
        // The ice prints 4: a run's ice stores no strength of its own.
        let wall = CardDefinition {
            id: CardId("palisade".to_string()),
            side: Side::Corp,
            card_type: CardType::Ice(IceType::Barrier),
            strength: Some(4),
            ..CardDefinition::default()
        };
        let registry = CardRegistry::from_cards(vec![breaker("cleaver", Some(IceType::Barrier)), wall]);
        let encountering = |strength: i32, credits: u32| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(credits);
            state.runner.rig = vec![InstalledRunnerCard {
                card: CardId("cleaver".to_string()),
                base_strength: strength,
                ..Default::default()
            }];
            state.active_run = Some(RunState {
                server: ServerId::Hq,
                phase: RunPhase::EncounterIce,
                position: 0,
                ice: vec![RunIce {
                    install_id: netrunner_core::rules::InstallId::PLACEHOLDER,
                    card_id: CardId("palisade".to_string()),
                    ice_type: IceType::Barrier,
                    subroutines: Vec::new(),
                    rezzed: true,
                }],
                ..Default::default()
            });
            state
        };
        let short = encountering(3, 2);
        let pumped = encountering(4, 0); // paid 2 to pump one point
        assert!(evaluate_state(&pumped, Side::Runner, &registry) > evaluate_state(&short, Side::Runner, &registry));
        let over = encountering(5, 0);
        assert_eq!(
            evaluate_state(&over, Side::Runner, &registry),
            evaluate_state(&pumped, Side::Runner, &registry),
            "strength past the ICE's is worth nothing more"
        );
    }

    /// A breaker priced like a real one: `break_cost` per activation
    /// breaking `break_count` subroutines, `pump_cost` per `pump_amount`
    /// strength — Cleaver is `(1, 2), (2, 1)`.
    fn priced_breaker(
        id: &str,
        restrict_to: Option<IceType>,
        (break_cost, break_count): (u32, u32),
        (pump_cost, pump_amount): (u32, u32),
    ) -> CardDefinition {
        use netrunner_core::dsl::EffectDuration;
        let mut def = breaker(id, restrict_to);
        def.abilities[0].cost = Some(Cost::Credits(break_cost));
        def.abilities[0].effect = Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(break_count), restrict_to };
        def.abilities.push(AbilityDef {
            text: None,
            trigger: Trigger::Paid,
            cost: Some(Cost::Credits(pump_cost)),
            requirement: None,
            effect: Effect::BoostStrength { amount: pump_amount, duration: EffectDuration::Encounter },
            cost_discount_if: None, used_by: None, access: false });
        def
    }

    /// One piece of ICE on a run, all subroutines pending. Its strength is
    /// what its card prints — a run's ice stores none — so the card is
    /// named for the number and `with_printed_ice` registers it.
    fn run_ice(strength: i32, ice_type: IceType, subroutines: usize, rezzed: bool) -> RunIce {
        use netrunner_core::rules::{EncounteredSubroutine, InstallId};
        RunIce {
            install_id: InstallId::PLACEHOLDER,
            card_id: CardId(format!("ice{strength}")),
            ice_type,
            subroutines: (0..subroutines)
                .map(|id| EncounteredSubroutine {
                    id,
                    definition: netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
                    status: SubroutineStatus::Pending,
                })
                .collect(),
            rezzed,
        }
    }

    /// `registry` with a card for each of `run_ice`'s pieces that prints the
    /// strength it was asked for.
    fn with_printed_ice(registry: &CardRegistry, ice: &[RunIce]) -> CardRegistry {
        let mut registry = registry.clone();
        for ice in ice {
            if let Some(strength) = ice.card_id.0.strip_prefix("ice").and_then(|n| n.parse().ok()) {
                registry.insert(CardDefinition {
                    id: ice.card_id.clone(),
                    title: ice.card_id.0.clone(),
                    side: Side::Corp,
                    card_type: CardType::Ice(ice.ice_type),
                    strength: Some(strength),
                    ..CardDefinition::default()
                });
            }
        }
        registry
    }

    /// `evaluate_state` for the Runner, `credits` in hand, approaching the
    /// outermost ICE of a run over `ice` — minus the same board with no run,
    /// so the result is exactly what the run term contributed.
    fn run_term(rig: Vec<InstalledRunnerCard>, credits: u32, ice: Vec<RunIce>, position: usize, registry: &CardRegistry) -> f64 {
        use netrunner_core::rules::ServerId;
        let registry = &with_printed_ice(registry, &ice);
        let mut idle = GameState::new(0);
        idle.runner.resources.credits = Credits(credits);
        idle.runner.rig = rig;
        // One card in HQ, so the breach has one hidden access to be worth.
        idle.corp.hq = corp_cards("hq", 1);
        let mut running = idle.clone();
        running.active_run = Some(RunState { server: ServerId::Hq, ice, position, ..Default::default() });
        let term = evaluate_state(&running, Side::Runner, registry) - evaluate_state(&idle, Side::Runner, registry);
        (term * 1000.0).round() / 1000.0 // the other terms cancel, up to float noise
    }

    /// The whole point of the conditional run term: with no breaker for a
    /// rezzed ICE, a run is worth nothing (so a credit click wins); with a
    /// breaker and the credits to use it, the run is worth taking.
    #[test]
    fn a_run_into_rezzed_ice_with_no_matching_breaker_is_not_worth_a_click() {
        let registry = CardRegistry::from_cards(vec![priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        let ice = || vec![run_ice(1, IceType::Barrier, 1, true)];
        assert_eq!(run_term(vec![], 5, ice(), 0, &registry), 0.0, "nothing in the rig breaks a Barrier");
        let wrong_subtype = priced_breaker("carmen", Some(IceType::Sentry), (1, 1), (2, 3));
        let registry = CardRegistry::from_cards(vec![wrong_subtype, priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        assert_eq!(run_term(vec![rig_card("carmen")], 5, ice(), 0, &registry), 0.0, "a Sentry breaker does not break a Barrier");
        let cleaver = InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") };
        assert_eq!(run_term(vec![cleaver], 5, ice(), 0, &registry), ACTIVE_RUN_WEIGHT);
    }

    /// Owning the breaker is not enough: the run is credited only when the
    /// pump and break credits are actually in hand (bad-publicity credits
    /// count — they are spendable on exactly this).
    #[test]
    fn a_run_is_credited_only_when_the_breaks_are_affordable() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::from_cards(vec![priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        let cleaver = || vec![InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") }];
        // Strength 4 against Cleaver's 3: one 2[c] pump, then one 1[c] break covers both subroutines.
        let ice = || vec![run_ice(4, IceType::Barrier, 2, true)];
        assert_eq!(run_term(cleaver(), 2, ice(), 0, &registry), 0.0, "3 credits needed, 2 held");
        assert_eq!(run_term(cleaver(), 3, ice(), 0, &registry), ACTIVE_RUN_WEIGHT);
        // Two rezzed ICE are paid for together.
        let two = || vec![run_ice(4, IceType::Barrier, 2, true), run_ice(1, IceType::Barrier, 3, true)];
        assert_eq!(run_term(cleaver(), 4, two(), 0, &registry), 0.0, "3 + 2 credits needed, 4 held");
        assert_eq!(run_term(cleaver(), 5, two(), 0, &registry), ACTIVE_RUN_WEIGHT);

        let registry = with_printed_ice(&registry, &ice());
        let mut idle = GameState::new(0);
        idle.runner.resources.credits = Credits(2);
        idle.runner.rig = cleaver();
        idle.corp.hq = corp_cards("hq", 1);
        let mut running = idle.clone();
        running.active_run =
            Some(RunState { server: ServerId::Hq, ice: ice(), bad_publicity_credits: 1, ..Default::default() });
        let term = evaluate_state(&running, Side::Runner, &registry) - evaluate_state(&idle, Side::Runner, &registry);
        assert_eq!((term * 1000.0).round() / 1000.0, ACTIVE_RUN_WEIGHT, "a bad-publicity credit closes the gap");
    }

    /// The evaluator's one window onto hidden information. Everything
    /// else it reads is public, the searching side's own zones, or a
    /// count `determinize` preserves — see ROADMAP Phase 2 §5 item 36.
    #[test]
    fn unbreakable_unrezzed_ice_counts_what_the_corp_can_rez_and_the_rig_cannot_break() {
        use netrunner_core::rules::ServerId;
        fn ice_card(id: &str, ice_type: IceType, cost: u32) -> CardDefinition {
            CardDefinition {
                id: CardId(id.to_string()),
                title: id.to_string(),
                side: Side::Corp,
                card_type: CardType::Ice(ice_type),
                cost,
                strength: Some(5),
                ..CardDefinition::default()
            }
        }
        let registry = CardRegistry::from_cards(vec![
            ice_card("ice", IceType::Sentry, 4),
            priced_breaker("carmen", Some(IceType::Sentry), (1, 2), (2, 1)),
        ]);
        let run = |rezzed, position| RunState {
            server: ServerId::Hq,
            ice: vec![RunIce { card_id: CardId("ice".to_string()), ..run_ice(5, IceType::Sentry, 1, rezzed) }],
            position,
            ..Default::default()
        };

        let mut state = GameState::new(0);
        state.corp.resources.credits = Credits(4);
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(false, 0), &registry), 1, "rezzable and unbreakable");

        state.corp.resources.credits = Credits(3);
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(false, 0), &registry), 0, "an ICE the Corp cannot pay to rez is not a threat");

        state.corp.resources.credits = Credits(4);
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(true, 0), &registry), 0, "a rezzed ICE is `run_is_breakable`'s job, not this one");
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(false, 1), &registry), 0, "ICE the run has already passed is behind the Runner");

        // A breaker that covers the subtype and can reach the strength
        // makes it breakable at a price, so it stops being a threat.
        state.runner.rig = vec![InstalledRunnerCard { base_strength: 5, ..rig_card("carmen") }];
        assert_eq!(unbreakable_unrezzed_ice(&state, &run(false, 0), &registry), 0, "the rig covers it");

        // ROADMAP Phase 2 §5 item 39: unbreakable is not the same claim
        // as "it stops you". An ICE the rig cannot touch, that the Corp
        // can afford, whose subroutine only tags, is a toll and not a
        // wall — and counting it was half of what this term counted.
        state.runner.rig = Vec::new();
        let mut tagging = run(false, 0);
        tagging.ice[0].subroutines[0].definition.effect = Effect::GiveTags(1);
        assert_eq!(unbreakable_unrezzed_ice(&state, &tagging, &registry), 0, "no subroutine ends the run");
        tagging.ice[0].subroutines[0].definition.effect =
            Effect::Sequence(vec![Effect::GiveTags(1), Effect::EndTheRun]);
        assert_eq!(unbreakable_unrezzed_ice(&state, &tagging, &registry), 1, "buried in a sequence still ends it");

        // The term is off by default, so none of this moves a score
        // until a `Weights` says otherwise.
        assert_eq!(Weights::default().unrezzed_threat_weight, 0.0);
    }

    /// An unrezzed ICE's identity in a determinized sample is a guess the
    /// real Runner cannot see, so it never blocks the run term; nor does
    /// ICE the run has already passed. `unbreakable_unrezzed_ice` is the
    /// term that may price it instead, and is weighted to zero here.
    #[test]
    fn unrezzed_and_already_passed_ice_never_block_the_run_term() {
        let registry = CardRegistry::new();
        let unrezzed = vec![run_ice(9, IceType::Barrier, 3, false)];
        assert_eq!(run_term(vec![], 0, unrezzed, 0, &registry), ACTIVE_RUN_WEIGHT);
        let passed = vec![run_ice(9, IceType::Barrier, 3, true)];
        assert_eq!(run_term(vec![], 0, passed, 1, &registry), ACTIVE_RUN_WEIGHT);
        let no_subroutines = vec![run_ice(9, IceType::Barrier, 0, true)];
        assert_eq!(run_term(vec![], 0, no_subroutines, 0, &registry), ACTIVE_RUN_WEIGHT, "nothing to break costs nothing");
    }

    /// An unrestricted (AI) breaker prices a run over any subtype; a
    /// breaker short on strength with no pump cannot break at any price.
    #[test]
    fn an_ai_breaker_covers_any_subtype_and_a_pumpless_shortfall_is_unbreakable() {
        let registry = CardRegistry::from_cards(vec![breaker("mayfly", None)]);
        let sentry = || vec![run_ice(1, IceType::Sentry, 2, true)];
        let mayfly = |strength| vec![InstalledRunnerCard { base_strength: strength, ..rig_card("mayfly") }];
        assert_eq!(run_term(mayfly(1), 0, sentry(), 0, &registry), ACTIVE_RUN_WEIGHT, "a free AI break costs nothing");
        assert_eq!(run_term(mayfly(0), 9, sentry(), 0, &registry), 0.0, "one point short and no pump ability");
    }

    /// A breaker with a printed install cost and memory cost, for the
    /// savings term.
    fn costed_breaker(id: &str, restrict_to: Option<IceType>, cost: u32) -> CardDefinition {
        CardDefinition { cost, memory_cost: Some(1), ..breaker(id, restrict_to) }
    }

    /// The whole point of the savings term: with an unaffordable breaker
    /// in grip, a credit click beats a run on an open server; once the
    /// breaker is affordable the penalty is gone and credits are worth
    /// only their usual weight.
    #[test]
    fn a_credit_click_beats_an_open_run_while_a_grip_breaker_is_unaffordable() {
        use netrunner_core::rules::{MemoryUnits, ServerId};
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3)]);
        let saving = |credits: u32| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(credits);
            state.runner.memory_units = MemoryUnits(4);
            state.runner.grip = vec![CardId("cleaver".to_string())];
            state.corp.hq = corp_cards("hq", 1);
            state
        };
        let clicked = saving(2);
        let mut ran = saving(1);
        ran.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
        assert!(evaluate_state(&clicked, Side::Runner, &registry) > evaluate_state(&ran, Side::Runner, &registry));

        let affordable = evaluate_state(&saving(3), Side::Runner, &registry);
        let one_more = evaluate_state(&saving(4), Side::Runner, &registry);
        assert!(((one_more - affordable) - OWN_CREDIT_WEIGHT).abs() < 1e-9, "no penalty left to close");
        let short_by_one = evaluate_state(&saving(2), Side::Runner, &registry);
        assert!(
            ((affordable - short_by_one) - (OWN_CREDIT_WEIGHT + SAVINGS_SHORTFALL_WEIGHT)).abs() < 1e-9,
            "the last credit of the gap is worth its weight plus the shortfall"
        );
    }

    /// Nothing to save for: a grip breaker for a subtype the rig already
    /// covers, or one that does not fit in free memory.
    #[test]
    fn a_covered_subtype_or_a_breaker_that_does_not_fit_in_memory_is_not_saved_for() {
        use netrunner_core::rules::MemoryUnits;
        let registry = CardRegistry::from_cards(vec![
            costed_breaker("cleaver", Some(IceType::Barrier), 3),
            costed_breaker("corroder", Some(IceType::Barrier), 2),
        ]);
        let mut state = GameState::new(0);
        state.runner.resources.credits = Credits(0);
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("corroder".to_string())];
        assert_eq!(breaker_savings_shortfall(&state, &registry), 2);
        state.runner.rig = vec![rig_card("cleaver")];
        assert_eq!(breaker_savings_shortfall(&state, &registry), 0, "Barrier is already covered");
        state.runner.rig.clear();
        state.runner.memory_units = MemoryUnits(0);
        assert_eq!(breaker_savings_shortfall(&state, &registry), 0, "no memory to install it into");
    }

    /// The reason this is a penalty on the shortfall and not a bonus on
    /// credits held: installing the breaker saved for must still beat
    /// clicking once the credits are there.
    #[test]
    fn installing_the_breaker_saved_for_still_beats_clicking() {
        use netrunner_core::rules::MemoryUnits;
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3)]);
        let mut clicked = GameState::new(0);
        clicked.runner.resources.credits = Credits(4);
        clicked.runner.memory_units = MemoryUnits(4);
        clicked.runner.grip = vec![CardId("cleaver".to_string())];
        let mut installed = GameState::new(0);
        installed.runner.resources.credits = Credits(0);
        installed.runner.memory_units = MemoryUnits(3);
        installed.runner.rig = vec![rig_card("cleaver")];
        assert!(evaluate_state(&installed, Side::Runner, &registry) > evaluate_state(&clicked, Side::Runner, &registry));
    }

    /// The grip term is a shortfall below a floor: each card up to the
    /// floor is worth `GRIP_SHORTFALL_WEIGHT`, cards past it nothing.
    #[test]
    fn a_grip_below_the_floor_costs_something_and_a_full_one_does_not() {
        let registry = CardRegistry::new();
        let with_grip = |cards: usize| {
            let mut state = GameState::new(0);
            state.runner.grip = (0..cards).map(|i| CardId(format!("card_{i}"))).collect();
            evaluate_state(&state, Side::Runner, &registry)
        };
        assert!((with_grip(1) - with_grip(0) - GRIP_SHORTFALL_WEIGHT).abs() < 1e-9);
        assert!((with_grip(GRIP_FLOOR) - with_grip(GRIP_FLOOR - 1) - GRIP_SHORTFALL_WEIGHT).abs() < 1e-9);
        assert_eq!(with_grip(GRIP_FLOOR + 1), with_grip(GRIP_FLOOR), "cards past the floor are worth nothing");
    }

    /// The door: a run that reached its server this turn is worth the run
    /// term for the rest of the turn, once, and only to the Runner.
    #[test]
    fn a_successful_run_this_turn_keeps_the_run_term_once() {
        let registry = CardRegistry::new();
        let mut breached = GameState::new(0);
        // Counted the way the engine counts it: there is no flag to set.
        netrunner_core::rules::dispatch_event(&mut breached, &registry, &netrunner_core::rules::GameEvent::RunSucceeded { server: netrunner_core::rules::ServerId::Archives }).expect("a run succeeds");
        let fresh = GameState::new(0);
        let delta = evaluate_state(&breached, Side::Runner, &registry) - evaluate_state(&fresh, Side::Runner, &registry);
        assert!((delta - SUCCESSFUL_RUN_WEIGHT).abs() < 1e-9);
        assert_eq!(
            evaluate_state(&breached, Side::Corp, &registry),
            evaluate_state(&fresh, Side::Corp, &registry),
            "Runner-only"
        );
    }

    /// Why the weight sits above the run term: with a thin grip a draw
    /// beats an open-server run; with the floor met, the run wins again.
    #[test]
    fn drawing_beats_an_open_run_below_the_grip_floor_and_not_above_it() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::new();
        let holding = |cards: usize, running: bool| {
            let mut state = GameState::new(0);
            state.runner.grip = (0..cards).map(|i| CardId(format!("card_{i}"))).collect();
            state.corp.hq = corp_cards("hq", 1);
            if running {
                state.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
            }
            evaluate_state(&state, Side::Runner, &registry)
        };
        assert!(holding(GRIP_FLOOR, false) > holding(GRIP_FLOOR - 1, true), "draw up to the floor first");
        assert!(holding(GRIP_FLOOR, true) > holding(GRIP_FLOOR + 1, false), "then run rather than keep drawing");
    }

    fn asset(id: &str, cost: u32) -> CardDefinition {
        CardDefinition { card_type: CardType::Asset, ..ice(id, cost) }
    }

    fn corp_cards(prefix: &str, n: usize) -> Vec<CardId> {
        (0..n).map(|i| CardId(format!("{prefix}_{i}"))).collect()
    }

    /// The whole point of `REZZED_ASSET_WEIGHT`: a 2-cost asset is worth
    /// its rez.
    #[test]
    fn a_two_cost_asset_is_worth_rezzing() {
        let registry = CardRegistry::from_cards(vec![asset("nico_campaign", 2)]);
        let nico = |rezzed, credits| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.installed = vec![InstalledCard {
                card: CardId("nico_campaign".to_string()),
                install_id: InstallId(1),
                rezzed,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(nico(true, 3) > nico(false, 5), "paying 2 to rez Nico Campaign is progress");
    }

    /// `UNREZZED_INSTALL_WEIGHT`'s side effect: paying the 1[c] to put a
    /// second ICE on a server beats a credit click, from an HQ above the
    /// floor (at the floor the card's `HQ_SHORTFALL_WEIGHT` tips it back).
    #[test]
    fn a_second_ice_on_a_server_beats_a_credit_click() {
        let registry = CardRegistry::from_cards(vec![ice("palisade", 3)]);
        let board = |installed_count, credits, hq| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE);
            state.corp.hq = corp_cards("palisade", hq);
            state.corp.installed = (0..installed_count)
                .map(|i| InstalledCard { card: CardId("palisade".to_string()), install_id: InstallId(i), ..Default::default() })
                .collect();
            evaluate_state(&state, Side::Corp, &registry)
        };
        // Second ICE: paid 1 to install it from an above-floor HQ, versus clicking for a credit.
        assert!(board(2, 4, HQ_FLOOR) > board(1, 6, HQ_FLOOR + 1));
    }

    /// The two terms that make the Corp rez the ICE that holds rather
    /// than the ICE that is cheap. Both are paid **only once the card is
    /// face up**, which is the arithmetic the first cut got wrong: paid
    /// on the face-down card too they sit on both sides of the rez and
    /// cancel out of the decision they exist to win.
    #[test]
    fn the_corp_prices_ice_by_what_it_stops_and_only_once_it_is_face_up() {
        use netrunner_core::dsl::SubroutineDef;
        use netrunner_core::rules::InstallSlot;
        let mut pharos = ice("pharos", 7);
        pharos.subroutines = vec![
            SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
            SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None },
        ];
        let mut tithe = ice("tithe", 1);
        tithe.subroutines =
            vec![SubroutineDef { text: String::new(), effect: Effect::GainCredits(Side::Corp, 1), only_breakable_by: None }];
        let registry = CardRegistry::from_cards(vec![pharos, tithe, breaker("fracter", Some(IceType::Barrier))]);

        let board = |id: &str, rezzed, rig: Vec<InstalledRunnerCard>| {
            let mut state = GameState::new(0);
            state.runner.rig = rig;
            state.corp.installed = vec![InstalledCard {
                card: CardId(id.to_string()),
                install_id: InstallId(1),
                slot: InstallSlot::Ice,
                rezzed,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Corp, &registry)
        };
        let naked = || vec![];
        let fracter = || vec![rig_card("fracter")];

        // Face down, the two pieces are worth the same: nothing about
        // what they stop has been revealed, and nothing may cancel.
        assert_eq!(board("pharos", false, naked()), board("tithe", false, naked()));
        assert_eq!(board("pharos", false, naked()), board("pharos", false, fracter()));

        // Face up, two run-enders against none is worth exactly two of
        // this weight, and a rig with no fracter is worth one more.
        let rez_gain = |id: &str, rig: Vec<InstalledRunnerCard>| board(id, true, rig) - board(id, false, naked());
        assert!((rez_gain("pharos", naked()) - rez_gain("tithe", naked()) - 2.0 * ETR_SUBROUTINE_WEIGHT).abs() < 1e-9);
        assert!((rez_gain("pharos", naked()) - rez_gain("pharos", fracter()) - UNBREAKABLE_ICE_WEIGHT).abs() < 1e-9);

        // The decision the pair exists to win: a 7-cost run-ender the rig
        // cannot break is worth rezzing, where `REZZED_ICE_WEIGHT` minus
        // its cost alone leaves it face-down forever.
        let paid = rez_gain("pharos", naked()) - 7.0 * OWN_CREDIT_WEIGHT;
        assert!(paid > 0.0, "an expensive run-ender the rig cannot break is worth rezzing, got {paid}");
    }

    /// The HQ term is a shortfall below a floor, and a thin R&D switches
    /// it off so the Corp does not draw itself to a deck-out.
    #[test]
    fn an_hq_below_the_floor_costs_something_and_a_thin_r_and_d_switches_it_off() {
        let registry = CardRegistry::new();
        let holding = |hq, rd| {
            let mut state = GameState::new(0);
            state.corp.hq = corp_cards("hq", hq);
            state.corp.r_and_d = corp_cards("rd", rd);
            evaluate_state(&state, Side::Corp, &registry)
        };
        let stocked = RD_DRAW_RESERVE + 10;
        assert!((holding(1, stocked) - holding(0, stocked) - HQ_SHORTFALL_WEIGHT).abs() < 1e-9);
        assert_eq!(holding(HQ_FLOOR + 1, stocked), holding(HQ_FLOOR, stocked), "cards past the floor are worth nothing");
        assert_eq!(holding(0, RD_DRAW_RESERVE - 1), holding(HQ_FLOOR, RD_DRAW_RESERVE - 1), "no draw pressure on a thin R&D");
    }

    /// Below the floor the Corp draws rather than clicking for a credit;
    /// at the floor it still installs rather than stalling — the trap the
    /// `UNREZZED_INSTALL_WEIGHT` bump exists to avoid.
    #[test]
    fn the_corp_draws_below_the_hq_floor_and_still_installs_from_it() {
        let registry = CardRegistry::from_cards(vec![asset("clearinghouse", 0)]);
        let state = |hq, credits, installed| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE + 10);
            state.corp.hq = corp_cards("clearinghouse", hq);
            state.corp.installed = (0..installed)
                .map(|i| InstalledCard { card: CardId("clearinghouse".to_string()), install_id: InstallId(i), ..Default::default() })
                .collect();
            evaluate_state(&state, Side::Corp, &registry)
        };
        assert!(state(HQ_FLOOR, 5, 0) > state(HQ_FLOOR - 1, 6, 0), "draw up to the floor rather than click for a credit");
        assert!(state(HQ_FLOOR - 1, 5, 1) > state(HQ_FLOOR, 6, 0), "install from the floor rather than click for a credit");
    }

    /// The stall that set `UNRESOLVED_DECISION_WEIGHT`'s size: resolving
    /// a parked selection must beat keeping it parked even when confirming
    /// costs a card from an at-floor hand, on either side.
    #[test]
    fn resolving_a_decision_outweighs_the_card_it_costs_from_an_at_floor_hand() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision};
        let registry = CardRegistry::new();
        let parked = |side: Side, source: CardZoneRef| PendingDecision::ChooseCards {
            side,
            source,
            filter: CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected: Vec::new(),
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        };
        let mut corp_parked = GameState::new(0);
        corp_parked.phase = GamePhase::Action(Side::Corp);
        corp_parked.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE + 10);
        corp_parked.corp.hq = corp_cards("hq", HQ_FLOOR);
        let mut corp_resolved = corp_parked.clone();
        corp_resolved.corp.hq.pop();
        corp_parked.pending_decision = Some(parked(Side::Corp, CardZoneRef::OwnHq));
        assert!(evaluate_state(&corp_resolved, Side::Corp, &registry) > evaluate_state(&corp_parked, Side::Corp, &registry));

        let mut runner_parked = GameState::new(0);
        runner_parked.phase = GamePhase::Action(Side::Runner);
        runner_parked.runner.grip = corp_cards("grip", GRIP_FLOOR);
        let mut runner_resolved = runner_parked.clone();
        runner_resolved.runner.grip.pop();
        runner_parked.pending_decision = Some(parked(Side::Runner, CardZoneRef::OwnGrip));
        assert!(evaluate_state(&runner_resolved, Side::Runner, &registry) > evaluate_state(&runner_parked, Side::Runner, &registry));
    }

    /// The Carmen case behind `BREAKER_COVERAGE_WEIGHT`'s size: a 5-cost
    /// breaker is worth installing over an open run. **From a grip above
    /// the floor**, since `HELD_CARD_WEIGHT`: from an at-floor grip the
    /// install now loses the card's held value *and* the grip shortfall,
    /// and the open run wins — the Runner draws first (a live top card is
    /// worth more than the run) or runs, and installs Carmen the click
    /// after. That trade was measured rather than avoided: with the held
    /// term the Runner draws more than twice as often and Carmen is
    /// installed as often as before, so the old at-floor claim is not
    /// worth a lower held weight (one that would keep it, 0.13, leaves
    /// no draw worth a credit).
    #[test]
    fn a_five_cost_breaker_beats_an_open_run_from_a_grip_above_the_floor() {
        use netrunner_core::rules::{MemoryUnits, ServerId};
        let registry = CardRegistry::from_cards(vec![costed_breaker("carmen", Some(IceType::Sentry), 5)]);
        let position = |grip: usize| {
            let mut ran = GameState::new(0);
            ran.runner.resources.credits = Credits(5);
            ran.runner.memory_units = MemoryUnits(4);
            ran.corp.hq = corp_cards("hq", 1);
            ran.runner.grip = corp_cards("grip", grip);
            ran.runner.grip.push(CardId("carmen".to_string()));
            let mut installed = ran.clone();
            installed.runner.grip.pop();
            installed.runner.resources.credits = Credits(0);
            installed.runner.memory_units = MemoryUnits(3);
            installed.runner.rig = vec![rig_card("carmen")];
            ran.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
            (evaluate_state(&installed, Side::Runner, &registry), evaluate_state(&ran, Side::Runner, &registry))
        };
        let (installed, ran) = position(GRIP_FLOOR);
        assert!(installed > ran, "above the floor Carmen beats the run: {installed} vs {ran}");
        let (installed, ran) = position(GRIP_FLOOR - 1);
        assert!(installed < ran, "at the floor the run comes first: {installed} vs {ran}");
    }

    /// An installed agenda is worth more for each ICE in front of it, up
    /// to the cap; an asset behind the same ICE gets nothing.
    #[test]
    fn an_installed_agenda_is_worth_more_behind_ice_up_to_the_cap() {
        use netrunner_core::rules::{InstallSlot, ServerId};
        let mut agenda = ice("offworld_office", 0);
        agenda.card_type = CardType::Agenda;
        let registry = CardRegistry::from_cards(vec![agenda, asset("nico_campaign", 2), ice("palisade", 3)]);
        let board = |root: &str, ice_count: usize| {
            let mut state = GameState::new(0);
            state.corp.r_and_d = corp_cards("rd", RD_DRAW_RESERVE + 10);
            state.corp.hq = corp_cards("hq", HQ_FLOOR);
            state.corp.installed = vec![InstalledCard {
                card: CardId(root.to_string()),
                install_id: InstallId(0),
                server: ServerId::Remote(0),
                ..Default::default()
            }];
            for i in 0..ice_count {
                state.corp.installed.push(InstalledCard {
                    card: CardId("palisade".to_string()),
                    install_id: InstallId(i as u32 + 1),
                    server: ServerId::Remote(0),
                    slot: InstallSlot::Ice,
                    ..Default::default()
                });
            }
            evaluate_state(&state, Side::Corp, &registry)
        };
        let per_ice = |root: &str, n| board(root, n) - board(root, n - 1) - UNREZZED_INSTALL_WEIGHT;
        assert!((per_ice("offworld_office", 1) - AGENDA_PROTECTION_WEIGHT).abs() < 1e-9);
        assert!((per_ice("offworld_office", AGENDA_PROTECTION_CAP) - AGENDA_PROTECTION_WEIGHT).abs() < 1e-9);
        assert!(per_ice("offworld_office", AGENDA_PROTECTION_CAP + 1).abs() < 1e-9, "past the cap an ICE is just an ICE");
        assert!(per_ice("nico_campaign", 1).abs() < 1e-9, "an asset is not protected by this term");
    }

    // ----- the run's prospect: what the breach can find -----

    /// An access-punishing asset the Runner can read: Urtica Cipher's shape.
    fn ambush(id: &str) -> CardDefinition {
        let mut def = asset(id, 0);
        def.triggers = vec![TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false,
            text: None,
            trigger: Trigger::OnAccessed,
            effects: vec![Effect::DealDamage(DamageType::Net, 2)],
            requirement: None,
        }];
        def
    }

    /// The run term alone — the Runner's score in a breakable run on
    /// `server` minus the same board idle.
    fn prospect(state: &GameState, server: netrunner_core::rules::ServerId, registry: &CardRegistry) -> f64 {
        let mut running = state.clone();
        running.active_run = Some(RunState { server, ..Default::default() });
        let term = evaluate_state(&running, Side::Runner, registry) - evaluate_state(state, Side::Runner, registry);
        (term * 1000.0).round() / 1000.0
    }

    /// The person's complaint, as a test: a pile the Runner has already
    /// turned face-up is not worth a click, a face-down card in it is, and
    /// a face-up agenda in it is the steal the breach cannot miss.
    #[test]
    fn an_archives_run_is_worth_its_face_down_cards_and_face_up_agendas_and_nothing_else() {
        use netrunner_core::rules::{ArchivedCard, ServerId};
        let registry = CardRegistry::from_cards(vec![asset("nico_campaign", 2), advanceable("offworld_office", 3)]);
        let mut state = GameState::new(0);
        state.corp.archives = vec![ArchivedCard::faceup(CardId("nico_campaign".to_string())); 3];
        assert_eq!(prospect(&state, ServerId::Archives, &registry), 0.0, "three known assets: nothing to find");
        state.corp.archives.push(ArchivedCard { card: CardId("nico_campaign".to_string()), facedown: true });
        assert_eq!(prospect(&state, ServerId::Archives, &registry), ACTIVE_RUN_WEIGHT, "one card the Runner has not seen");
        state.corp.archives.push(ArchivedCard::faceup(CardId("offworld_office".to_string())));
        assert_eq!(
            prospect(&state, ServerId::Archives, &registry),
            ACTIVE_RUN_WEIGHT + 2.0 * AGENDA_POINT_WEIGHT,
            "a face-up agenda is worth its points"
        );
    }

    /// The position the person watched the bot lose from: Urtica Cipher
    /// face-up in Archives beside one face-down card. The face-down card
    /// alone would be worth a run; the known ambush makes the run worth
    /// less than the credit click it displaces.
    #[test]
    fn a_known_ambush_in_archives_makes_the_run_worth_less_than_a_credit() {
        use netrunner_core::rules::{ArchivedCard, ServerId};
        let registry = CardRegistry::from_cards(vec![ambush("urtica_cipher"), asset("nico_campaign", 2)]);
        let mut state = GameState::new(0);
        state.corp.archives = vec![
            ArchivedCard::faceup(CardId("urtica_cipher".to_string())),
            ArchivedCard { card: CardId("nico_campaign".to_string()), facedown: true },
        ];
        let term = prospect(&state, ServerId::Archives, &registry);
        assert!((term - (ACTIVE_RUN_WEIGHT - KNOWN_AMBUSH_WEIGHT)).abs() < 1e-9, "{term}");
        assert!(term < OWN_CREDIT_WEIGHT, "a credit click wins");
        // The same ambush, still face down, is not read — its identity in
        // a determinized sample is a guess.
        state.corp.archives[0].facedown = true;
        assert_eq!(prospect(&state, ServerId::Archives, &registry), 2.0 * ACTIVE_RUN_WEIGHT);
    }

    /// The same top card, the same pile, the same face-down remote: a
    /// second run this turn finds nothing new anywhere but HQ.
    #[test]
    fn a_second_run_on_the_same_server_this_turn_is_worth_nothing_except_on_hq() {
        use netrunner_core::rules::{ArchivedCard, ServerId};
        let registry = CardRegistry::from_cards(vec![asset("nico_campaign", 2)]);
        let mut state = GameState::new(0);
        state.corp.hq = corp_cards("hq", 4);
        state.corp.r_and_d = corp_cards("rd", 10);
        state.corp.archives = vec![ArchivedCard { card: CardId("nico_campaign".to_string()), facedown: true }];
        state.corp.installed = vec![InstalledCard {
            card: CardId("nico_campaign".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            ..Default::default()
        }];
        for server in [ServerId::Hq, ServerId::RnD, ServerId::Archives, ServerId::Remote(0)] {
            assert_eq!(prospect(&state, server, &registry), ACTIVE_RUN_WEIGHT, "first run on {server:?}");
        }
        // `start_run` records the target at initiation, so a run in
        // progress is already listed once; a repeat is a second entry.
        let mut again = state.clone();
        for server in [ServerId::Hq, ServerId::RnD, ServerId::Archives, ServerId::Remote(0)] {
            again.runner.servers_run_this_turn = vec![server, server];
            // HQ holds four cards, so the second access is fresh three
            // times in four; everywhere else a repeat finds what the first
            // run found.
            let expected = if server == ServerId::Hq { 0.75 * ACTIVE_RUN_WEIGHT } else { 0.0 };
            assert!((prospect(&again, server, &registry) - expected).abs() < 1e-9, "second run on {server:?}");
            again.runner.servers_run_this_turn = vec![server];
            assert_eq!(prospect(&again, server, &registry), ACTIVE_RUN_WEIGHT, "the run's own entry is not a repeat");
        }
        // The repeat's value falls with each run and with a thinner HQ:
        // a third run at four cards is under a credit click, and a second
        // run on a one-card HQ finds the card it already saw.
        again.runner.servers_run_this_turn = vec![ServerId::Hq; 3];
        let third = prospect(&again, ServerId::Hq, &registry);
        assert!(third < OWN_CREDIT_WEIGHT && third > 0.0, "{third}");
        again.corp.hq = corp_cards("hq", 1);
        again.runner.servers_run_this_turn = vec![ServerId::Hq; 2];
        assert_eq!(prospect(&again, ServerId::Hq, &registry), 0.0);
    }

    /// Where the Corp is scoring is where the Runner goes first: each
    /// token on a face-down root card adds to the run, and past one token
    /// the remote is worth more than any central.
    #[test]
    fn an_advanced_face_down_remote_card_is_worth_more_than_a_central() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::from_cards(vec![advanceable("offworld_office", 3)]);
        let mut state = GameState::new(0);
        state.corp.hq = corp_cards("hq", 4);
        state.corp.installed = vec![InstalledCard {
            card: CardId("offworld_office".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            advancement_tokens: 2,
            ..Default::default()
        }];
        let remote = prospect(&state, ServerId::Remote(0), &registry);
        assert!((remote - (ACTIVE_RUN_WEIGHT + 2.0 * ADVANCED_CARD_PROSPECT_WEIGHT)).abs() < 1e-9, "{remote}");
        assert!(remote > prospect(&state, ServerId::Hq, &registry));
        // The tokens are read off the install, not the card: an ambush
        // advanced twice is worth exactly the same run, because the Runner
        // cannot tell the two apart.
        let trap = CardRegistry::from_cards(vec![ambush("offworld_office")]);
        assert_eq!(prospect(&state, ServerId::Remote(0), &trap), remote);
    }

    /// The trash lever, both halves: a rezzed asset the Runner can afford
    /// to trash makes the run worth starting, and once accessed, trashing
    /// it beats leaving it — at 2[c], and not at 4[c].
    #[test]
    fn a_trashable_rezzed_asset_is_worth_the_run_and_the_trash() {
        use netrunner_core::rules::{ArchivedCard, ServerId};
        let priced = |trash_cost: u32| {
            let mut nico = asset("nico_campaign", 2);
            nico.trash_cost = Some(trash_cost);
            CardRegistry::from_cards(vec![nico])
        };
        let mut state = GameState::new(0);
        state.runner.resources.credits = Credits(5);
        state.corp.installed = vec![InstalledCard {
            card: CardId("nico_campaign".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        let removed = (BOARD_PRESENCE_WEIGHT + REZZED_ASSET_WEIGHT) * OPPONENT_BOARD_WEIGHT;
        let cheap = prospect(&state, ServerId::Remote(0), &priced(2));
        assert!((cheap - (removed - 2.0 * OWN_CREDIT_WEIGHT)).abs() < 1e-9, "{cheap}");
        assert!(cheap > 0.0, "the run that ends the loop is worth starting");
        assert_eq!(prospect(&state, ServerId::Remote(0), &priced(4)), 0.0, "too dear to trash: a known card, nothing to find");
        assert_eq!(prospect(&state, ServerId::Remote(0), &priced(2)), cheap, "the prospect does not depend on having run before");

        let mut trashed = state.clone();
        trashed.corp.installed.clear();
        trashed.corp.archives = vec![ArchivedCard::faceup(CardId("nico_campaign".to_string()))];
        trashed.runner.resources.credits = Credits(3);
        assert!(evaluate_state(&trashed, Side::Runner, &priced(2)) > evaluate_state(&state, Side::Runner, &priced(2)));
        assert!(
            (evaluate_state(&trashed, Side::Corp, &priced(2)) - evaluate_state(&state, Side::Corp, &priced(2))
                - (-(BOARD_PRESENCE_WEIGHT + REZZED_ASSET_WEIGHT) + 2.0 * OPPONENT_CREDIT_WEIGHT))
                .abs()
                < 1e-9,
            "the Corp's own reading of the same trash is unchanged"
        );
        let mut dear = trashed.clone();
        dear.runner.resources.credits = Credits(1);
        assert!(evaluate_state(&dear, Side::Runner, &priced(4)) < evaluate_state(&state, Side::Runner, &priced(4)));
    }

    /// The property that keeps the board term usable by a search over
    /// determinized samples: two samples that put different cards under
    /// the same face-down install score the same for the Runner.
    #[test]
    fn the_corp_board_term_reads_no_hidden_identity() {
        use netrunner_core::rules::ServerId;
        let registry = CardRegistry::from_cards(vec![advanceable("offworld_office", 3), ambush("urtica_cipher"), ice("palisade", 3)]);
        let under = |card: &str, rezzed: bool| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId(card.to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                advancement_tokens: 2,
                rezzed,
                ..Default::default()
            }];
            evaluate_state(&state, Side::Runner, &registry)
        };
        assert_eq!(under("offworld_office", false), under("urtica_cipher", false));
        assert_eq!(under("offworld_office", false), under("palisade", false));
        assert!(under("offworld_office", false) < evaluate_state(&GameState::new(0), Side::Runner, &registry), "and it counts");
        assert_ne!(under("urtica_cipher", true), under("palisade", true), "rezzed, the card is public and priced as itself");
    }

    // ----- a card in grip: worth half of what installing it would be -----

    /// The draw the person never saw: at the floor, a draw that brings a
    /// breaker for an uncovered subtype beats a credit, and a draw that
    /// brings a card the Runner would never install does not.
    #[test]
    fn drawing_a_breaker_for_an_uncovered_subtype_beats_a_credit_at_the_floor() {
        let mut dead = costed_breaker("dead", Some(IceType::Barrier), 3);
        dead.card_type = CardType::Resource;
        dead.abilities.clear();
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3), dead]);
        let holding = |drawn: Option<&str>| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(if drawn.is_some() { 5 } else { 6 });
            state.runner.grip = corp_cards("filler", GRIP_FLOOR);
            if let Some(card) = drawn {
                state.runner.grip.push(CardId(card.to_string()));
            }
            evaluate_state(&state, Side::Runner, &registry)
        };
        let delta = BOARD_PRESENCE_WEIGHT + BREAKER_COVERAGE_WEIGHT - 3.0 * OWN_CREDIT_WEIGHT - MEMORY_WEIGHT;
        assert!((holding(Some("cleaver")) - holding(None) - (delta * HELD_CARD_WEIGHT - OWN_CREDIT_WEIGHT)).abs() < 1e-9);
        assert!(holding(Some("cleaver")) > holding(None), "the draw beats the credit");
        assert!(holding(Some("dead")) < holding(None), "a 3-cost card with no coverage is not worth drawing for");
        assert!(
            (holding(Some("dead")) - holding(None) + OWN_CREDIT_WEIGHT).abs() < 1e-9,
            "and is worth exactly nothing held — the term never goes negative"
        );
    }

    /// The reason the term is a fraction of the install rather than a
    /// value per card: the live card still goes on the table, and a
    /// second breaker for a covered subtype is dead in hand.
    #[test]
    fn a_live_card_is_still_installed_and_a_covered_breaker_is_dead_in_hand() {
        use netrunner_core::rules::MemoryUnits;
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3)]);
        let mut held = GameState::new(0);
        held.runner.resources.credits = Credits(3);
        held.runner.memory_units = MemoryUnits(4);
        held.runner.grip = corp_cards("filler", GRIP_FLOOR);
        held.runner.grip.push(CardId("cleaver".to_string()));
        let mut installed = held.clone();
        installed.runner.grip.pop();
        installed.runner.resources.credits = Credits(0);
        installed.runner.memory_units = MemoryUnits(3);
        installed.runner.rig = vec![rig_card("cleaver")];
        assert!(evaluate_state(&installed, Side::Runner, &registry) > evaluate_state(&held, Side::Runner, &registry));

        let mut duplicate = installed.clone();
        duplicate.runner.grip.push(CardId("cleaver".to_string()));
        let mut without = installed.clone();
        without.runner.grip.push(CardId("filler_9".to_string()));
        assert_eq!(
            evaluate_state(&duplicate, Side::Runner, &registry),
            evaluate_state(&without, Side::Runner, &registry),
            "Barrier is covered, so a second Cleaver is worth what an unknown card is"
        );
    }

    // ----- `Weights`: the terms only a personality moves -----

    #[test]
    fn the_default_weights_are_the_constants_and_score_identically() {
        let mut state = GameState::new(0);
        state.corp.resources.agenda_points = AgendaPoints(2);
        state.runner.grip = corp_cards("g", 4);
        for side in [Side::Corp, Side::Runner] {
            assert_eq!(evaluate_state(&state, side, &empty()), evaluate_state_with(&state, side, &empty(), &Weights::default()));
        }
        assert_eq!(Weights::default().installed_agenda_weight, 0.0, "the balanced Corp has no install preference by type");
        assert_eq!(Weights::default().stage_gain, 0.0, "and the evaluator is static until a measurement earns otherwise");
    }

    /// The identity that makes every number recorded before the stage
    /// existed still stand: at gain 0 nothing is interpolated at all, on
    /// either chair and whatever the board looks like.
    #[test]
    fn a_stage_gain_of_zero_is_the_static_evaluator() {
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3)]);
        let mut state = GameState::new(0);
        state.corp.resources.agenda_points = AgendaPoints(5);
        state.runner.rig = vec![rig_card("cleaver")];
        state.runner.grip = corp_cards("g", 4);
        let zero = Weights { stage_gain: 0.0, ..Weights::default() };
        for side in [Side::Corp, Side::Runner] {
            assert_eq!(
                evaluate_state_with(&state, side, &registry, &zero),
                evaluate_state_with(&state, side, &registry, &Weights::default()),
                "{side:?} scores the same with the dial off"
            );
            assert_eq!(stage_weights(&state, side, &registry, &zero), zero);
        }
    }

    /// The Corp arm is not staged yet, so a Corp seat handed a gain plays
    /// byte-identically to one handed none. That is what lets one
    /// `--stage-gain` flag isolate the Runner chair (`bots::AgentSetup`).
    #[test]
    fn the_corp_is_not_staged_so_one_flag_isolates_the_runner_chair() {
        let registry = CardRegistry::from_cards(vec![costed_breaker("cleaver", Some(IceType::Barrier), 3)]);
        let mut state = GameState::new(0);
        state.corp.resources.agenda_points = AgendaPoints(5);
        state.runner.rig = vec![rig_card("cleaver")];
        let full = Weights { stage_gain: 1.0, ..Weights::default() };
        assert_eq!(
            evaluate_state_with(&state, Side::Corp, &registry, &full),
            evaluate_state_with(&state, Side::Corp, &registry, &Weights::default()),
        );
        assert_ne!(
            evaluate_state_with(&state, Side::Runner, &registry, &full),
            evaluate_state_with(&state, Side::Runner, &registry, &Weights::default()),
            "the Runner chair does move, or the dial is wired to nothing"
        );
    }

    /// `lerp` lands on its endpoints, is linear between them, and rounds a
    /// count rather than truncating it — `grip_floor` travelling 3 → 2 has
    /// to cross at the halfway point, not at the very end.
    #[test]
    fn lerp_lands_on_its_endpoints_and_rounds_its_counts() {
        let build = Personality::Builder.weights();
        let pressure = Personality::Aggressive.weights();
        assert_eq!(lerp(&build, &pressure, 0.0), build);
        assert_eq!(lerp(&build, &pressure, 1.0), pressure);

        let half = lerp(&build, &pressure, 0.5);
        let midpoint = (build.active_run_weight + pressure.active_run_weight) / 2.0;
        assert!((half.active_run_weight - midpoint).abs() < 1e-9);

        // No profile's floor differs from another's since Phase 5 §16 put
        // `Aggressive`'s back at 3, so the count under test is set here.
        let thin = Weights { grip_floor: 2, ..pressure };
        assert_eq!(build.grip_floor, 3);
        assert_eq!(lerp(&build, &thin, 0.49).grip_floor, 3);
        assert_eq!(lerp(&build, &thin, 0.51).grip_floor, 2);

        // The dial's own setting is the caller's and is never blended, or
        // how far the travel goes would depend on how far it had got.
        let geared = Weights { stage_gain: 0.6, ..build };
        assert_eq!(lerp(&geared, &pressure, 1.0).stage_gain, 0.6);
    }

    /// The scalar is the rig, overridden by the Corp's clock. A Runner
    /// with no breakers has no business running; one that covers all three
    /// subtypes has no more building to do; and at five of seven points
    /// against it, neither reading matters and it has to contest.
    #[test]
    fn the_runner_stage_reads_the_rig_and_is_overridden_by_the_corps_clock() {
        let registry = CardRegistry::from_cards(vec![
            breaker("cleaver", Some(IceType::Barrier)),
            breaker("carmen", Some(IceType::Sentry)),
            breaker("unity", Some(IceType::CodeGate)),
        ]);
        let mut state = GameState::new(0);
        assert_eq!(runner_stage(&state, &registry), 0.0, "no rig, no clock: all build");

        state.runner.rig = vec![rig_card("cleaver")];
        assert!((runner_stage(&state, &registry) - 1.0 / 3.0).abs() < 1e-9);

        state.runner.rig.push(rig_card("carmen"));
        state.runner.rig.push(rig_card("unity"));
        assert_eq!(runner_stage(&state, &registry), 1.0, "a complete rig is all pressure");

        // The override, on a board with nothing built.
        let mut losing = GameState::new(0);
        losing.corp.resources.agenda_points = AgendaPoints(5);
        let target = f64::from(losing.rules.winning_agenda_points);
        assert!((runner_stage(&losing, &registry) - 5.0 / target).abs() < 1e-9);
        assert!(runner_stage(&losing, &registry) > 0.5, "five points against is not a building position");
    }

    /// The decision the dial exists to win, and the one `diag tempo`
    /// measured the Runner getting backwards: with an empty rig and a
    /// breaker in grip, building beats running; with the rig complete and
    /// the same choice, running beats building.
    #[test]
    fn a_staged_runner_builds_on_an_empty_rig_and_runs_on_a_full_one() {
        let registry = CardRegistry::from_cards(vec![
            costed_breaker("cleaver", Some(IceType::Barrier), 3),
            costed_breaker("carmen", Some(IceType::Sentry), 3),
            costed_breaker("unity", Some(IceType::CodeGate), 3),
        ]);
        use netrunner_core::rules::{MemoryUnits, ServerId};
        let staged = Weights { stage_gain: 1.0, ..Weights::default() };
        // The same two positions at both ends of the dial: one credit
        // spent on a run in progress, against one spent holding the rig
        // together. What moves between them is only the stance.
        let run_value = |rig: Vec<&str>| {
            let mut state = GameState::new(0);
            state.runner.resources.credits = Credits(6);
            state.runner.grip = corp_cards("g", 4);
            state.runner.rig = rig.iter().map(|id| rig_card(id)).collect();
            state.runner.memory_units = MemoryUnits(4);
            // `access_prospect` counts what the breach would *show*, so an
            // empty HQ makes the run worth nothing at either end of the
            // dial and the comparison vacuous.
            state.corp.hq = corp_cards("hq", 3);
            let idle = evaluate_state_with(&state, Side::Runner, &registry, &staged);
            state.active_run = Some(RunState { server: ServerId::Hq, ..Default::default() });
            evaluate_state_with(&state, Side::Runner, &registry, &staged) - idle
        };
        let empty_rig = run_value(vec![]);
        let full_rig = run_value(vec!["cleaver", "carmen", "unity"]);
        assert!(
            full_rig > empty_rig,
            "a run is worth more once the rig is built: {empty_rig} with nothing, {full_rig} with everything"
        );
    }

    #[test]
    fn an_installed_agenda_weight_prefers_the_agenda_on_the_table_to_the_ice() {
        let mut agenda = ice("agenda", 0);
        agenda.card_type = CardType::Agenda;
        agenda.advancement_requirement = Some(3);
        let registry = CardRegistry::from_cards(vec![agenda, ice("wall", 0)]);
        let install = |id: &str, slot: netrunner_core::rules::InstallSlot| {
            let mut state = GameState::new(0);
            state.corp.installed = vec![InstalledCard {
                card: CardId(id.to_string()),
                install_id: InstallId(1),
                slot,
                server: netrunner_core::rules::ServerId::Remote(0),
                ..Default::default()
            }];
            state
        };
        let with_agenda = install("agenda", netrunner_core::rules::InstallSlot::Root);
        let with_ice = install("wall", netrunner_core::rules::InstallSlot::Ice);
        let balanced = Weights::default();
        assert_eq!(
            evaluate_state_with(&with_agenda, Side::Corp, &registry, &balanced),
            evaluate_state_with(&with_ice, Side::Corp, &registry, &balanced),
            "balanced: an unrezzed install is an unrezzed install"
        );
        let rush = Weights { installed_agenda_weight: 1.0, ..balanced };
        assert!(evaluate_state_with(&with_agenda, Side::Corp, &registry, &rush) > evaluate_state_with(&with_ice, Side::Corp, &registry, &rush));
    }

    /// The Corp's mirror of `active_run_weight`, on the same gate: a run
    /// the Runner can finish costs the Corp; one they cannot get through
    /// does not, because there is nothing there for the Corp to pay to
    /// stop.
    #[test]
    fn a_breakable_run_costs_the_corp_and_an_unbreakable_one_does_not() {
        use netrunner_core::rules::{RunIce, ServerId};
        let registry = CardRegistry::from_cards(vec![priced_breaker("cleaver", Some(IceType::Barrier), (1, 2), (2, 1))]);
        let w = Weights::default();

        // The run term against the *same* board with no run, so every
        // other term cancels — credits especially, which the Corp scores
        // through `opponent_credit_weight`.
        let run_term = |ice: Vec<RunIce>, credits: u32, rig: Vec<InstalledRunnerCard>| {
            let registry = with_printed_ice(&registry, &ice);
            let mut idle = GameState::new(0);
            idle.runner.resources.credits = Credits(credits);
            idle.runner.rig = rig;
            let mut running = idle.clone();
            running.active_run = Some(RunState { server: ServerId::Hq, ice, position: 0, ..Default::default() });
            evaluate_state_with(&running, Side::Corp, &registry, &w)
                - evaluate_state_with(&idle, Side::Corp, &registry, &w)
        };

        // Nothing rezzed in the way: the run is breakable by definition.
        let delta = run_term(Vec::new(), 0, Vec::new());
        assert!((delta + w.active_run_against_weight).abs() < 1e-9, "an open run costs the Corp the term: {delta}");

        // A rezzed Barrier the Runner has no breaker for: not the Corp's
        // problem, and the same predicate the Runner's own term uses.
        let delta = run_term(vec![run_ice(1, IceType::Barrier, 1, true)], 5, Vec::new());
        assert_eq!(delta, 0.0, "a run into ice the Runner cannot break costs the Corp nothing");

        // The same ice with a breaker and the credits to use it: the Corp
        // is paying again, which is the gate doing its job in both
        // directions.
        let armed = InstalledRunnerCard { base_strength: 3, ..rig_card("cleaver") };
        let delta = run_term(vec![run_ice(1, IceType::Barrier, 1, true)], 5, vec![armed]);
        assert!((delta + w.active_run_against_weight).abs() < 1e-9, "a breakable run costs the Corp: {delta}");
    }

    /// The decision the run term was sized to win, and the reason it
    /// exists: *Anoetic Void* offers the Corp "pay 2, discard 2 from HQ,
    /// end the run", and before the term the continuation priced at zero
    /// so it was declined every time (ROADMAP Phase 3 §1).
    ///
    /// Checked through `pending_decision_upside`, which is what the
    /// evaluator credits beside `pending_decision_upside_weight` when the
    /// Corp is sitting on the parked selection.
    #[test]
    fn ending_a_run_is_worth_exactly_the_term_it_removes() {
        use netrunner_core::rules::{PendingChoiceResume, RunIce, ServerId};
        let registry = CardRegistry::new();
        let w = Weights::default();

        let parked = |ice: Vec<RunIce>, hq: usize| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.corp.hq = (0..hq).map(|i| CardId(format!("hq{i}"))).collect();
            state.corp.r_and_d = (0..20).map(|i| CardId(format!("rd{i}"))).collect();
            state.active_run = Some(RunState { server: ServerId::Hq, ice, position: 0, ..Default::default() });
            state.pending_decision = Some(PendingDecision::ChooseCards {
                side: Side::Corp,
                source: CardZoneRef::OwnHq,
                filter: CardFilter::Any,
                min: 2,
                max: 2,
                reveal: false,
                shuffle_after: false,
                destination: Some(CardZoneRef::OwnArchives),
                then: Some(Box::new(Effect::EndTheRun)),
                selected: Vec::new(),
                source_card: None,
                prompting_card: None,
                source_install: None,
                resume: PendingChoiceResume::None,
            });
            state
        };

        // A live run, and an HQ well above the floor so the two discards
        // are free: the upside is the run term and nothing else.
        let state = parked(Vec::new(), 8);
        let upside = pending_decision_upside(&state, Side::Corp, &registry, &w);
        assert!(
            (upside - w.active_run_against_weight).abs() < 1e-9,
            "ending the run recovers exactly the term: {upside}"
        );
        // And it clears the bar that decides the offer: the parked
        // decision costs `unresolved_decision_weight` against
        // `pending_decision_upside_weight` plus this.
        assert!(
            upside + w.pending_decision_upside_weight > w.unresolved_decision_weight,
            "the Corp has to come out ahead for Anoetic Void to be accepted"
        );

        // No run to end: the continuation is worth nothing, and the offer
        // goes back to being declined.
        let mut no_run = parked(Vec::new(), 8);
        no_run.active_run = None;
        assert_eq!(pending_decision_upside(&no_run, Side::Corp, &registry, &w), 0.0);
    }


}

