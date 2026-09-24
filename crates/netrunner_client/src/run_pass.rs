//! The Corp's "no more this run": every priority window of the run is
//! passed for them until it ends (Phase 7 §8 item 6, from jinteki's
//! `toggle-auto-no-action`).
//!
//! **What it buys.** `play::lone_pass` already takes a pass that is the
//! only thing offered. A Corp holding unrezzed ICE and the credits to rez
//! it is offered a rez beside the pass in every window of the Runner's
//! run — at each approach, after each encounter, before the breach — so a
//! bot's run through a three-ICE server stops the person half a dozen
//! times to ask a question they have already answered. This is the one
//! press that answers it for the run.
//!
//! **A client policy, not a rule — `lone_pass`'s shape.** The engine still
//! opens every window and hears every pass; the client submits the pass
//! the view lists instead of asking. Nothing here is an action: `Session`,
//! the bots, the sweeps and `ActionSpace` never see it.
//!
//! **Only a window is passed, never a question.** A pass is taken when the
//! view is in a run's paid-ability window with nothing parked: a prompt,
//! a trace, a payment split or a prevention window still asks. A prompt
//! is a card's text wanting an answer, which "no more actions" did not
//! give; and the Corp is asked in a prevention window only when it could
//! prevent something (`prevention::could_prevent`), so that is a choice
//! the person has not seen yet.
//!
//! **It lasts the run, and ends by itself.** The first view with no run
//! turns it off (`RunPass::see`), so the next run asks again: a pass held
//! across runs would wave through a run on a server the person meant to
//! defend. jinteki keeps its flag on the run for the same reason. It also
//! ends on a take-back, or the pass it took would be taken again at once
//! (`RunPass::stop`), and the person may turn it off mid-run.
//!
//! **Not taken: jinteki's "pass on rez"**, a standing setting that passes
//! as soon as the Corp rezzes a piece of ICE. It gives up the rest of a
//! window the person just used — a second rez, an upgrade — without being
//! asked, and nobody has asked for it here.

use netrunner_core::rules::{PlayerAction, Side};
use netrunner_core::view::ClientView;

/// Whether the Corp has said "no more this run". Held by the client
/// beside its view, never sent anywhere.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunPass {
    on: bool,
}

impl RunPass {
    /// Whether the rest of the run is being passed.
    pub fn is_on(self) -> bool {
        self.on
    }

    /// Whether `view` is a moment to offer it: the viewer is the Corp,
    /// in a run's window, with a pass to take. Off, or it would be
    /// offered while it is already on.
    pub fn offered(self, view: &ClientView) -> bool {
        !self.on && view.viewer.side() == Some(Side::Corp) && window_pass(view).is_some()
    }

    /// The press: turned on, and the pass for this window returned to
    /// submit. `None`, and still off, when `view` is not a moment to
    /// offer it.
    pub fn start(&mut self, view: &ClientView) -> Option<PlayerAction> {
        if !self.offered(view) {
            return None;
        }
        self.on = true;
        window_pass(view)
    }

    /// Turned off: the person pressed Stop, or took a move back.
    pub fn stop(&mut self) {
        self.on = false;
    }

    /// Every view the client receives goes through here, the opponent's
    /// moves included: the first with no run turns it off.
    pub fn see(&mut self, view: &ClientView) {
        if view.active_run.is_none() {
            self.on = false;
        }
    }

    /// The pass to take for the person on `view`, when it is on and the
    /// view is a run's window (the module doc's "only a window"). A read,
    /// so a client can ask it in a match guard: a flag left on past its
    /// run passes nothing, because a view with no run has no run window,
    /// and `see` turns it off.
    pub fn pass(self, view: &ClientView) -> Option<PlayerAction> {
        if !self.on {
            return None;
        }
        window_pass(view)
    }
}

/// The Corp's pass, when `view` is a run's paid-ability window with
/// nothing parked in it.
fn window_pass(view: &ClientView) -> Option<PlayerAction> {
    let parked = view.pending_decision.is_some()
        || view.pending_paid_choice.is_some()
        || view.pending_prevention.is_some()
        || view.pending_payment.is_some()
        || view.active_trace.is_some();
    if view.active_run.is_none() || view.paid_ability_window.is_none() || parked {
        return None;
    }
    view.legal_actions.iter().find(|action| matches!(action, PlayerAction::PassPriority { side: Side::Corp })).cloned()
}
