use serde::{Deserialize, Serialize};

use crate::dsl::card::CardId;
use crate::rules::{ServerId, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageType {
    Net,
    Meat,
    Brain,
}

/// Which ordered deck zone a `TrashCard(CardTarget::TopOfStack)` effect
/// mills from — the only two zones in `GameState` that have a meaningful
/// "top."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackZone {
    RAndD,
    Stack,
}

/// What an `Effect::TrashCard` targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardTarget {
    /// The card this ability/subroutine/trigger is itself printed on. Must
    /// be resolved to a concrete target by the dispatch layer before
    /// reaching `evaluate_effect` — that function has no "which card is
    /// resolving" context on its own.
    ThisCard,
    /// A Corp card installed on a server, identified the same way
    /// `state::InstalledCard` already identifies one (`CardId` +
    /// `ServerId`).
    CorpInstalled { card: CardId, server: ServerId },
    /// A Runner card in the Rig — no server/slot component, since
    /// `RunnerState::rig` is a flat `Vec<CardId>` with no per-card
    /// location metadata.
    RunnerRig(CardId),
    /// The top card of an ordered deck zone, without needing to name it —
    /// covers "mill" effects (trash without revealing).
    TopOfStack { side: Side, zone: StackZone },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    /// `Side` is explicit — even though most cards only ever grant
    /// credits to their own controller (and `Card::side` already implies
    /// that), an explicit target lets a card affect the opponent instead.
    GainCredits(Side, u32),
    /// Renamed from `InflictDamage`. `usize` (not `u32`) matches
    /// `damage::apply_damage`'s existing signature exactly. No `Side`
    /// param: damage in this engine's model always targets the Runner,
    /// same as `apply_damage` itself.
    DealDamage(DamageType, usize),
    /// Never side-ambiguous — always targets whatever ICE the current run
    /// is encountering.
    BreakSubroutine(u32),
    ModifyStrength(i32),
    /// `Side`-explicit for the same reason as `GainCredits`.
    DrawCards(Side, u32),
    /// Ends whatever run is in `GameState::active_run`. No payload — there
    /// is exactly one active run at a time.
    EndTheRun,
    /// Deliberately no `Side` param, unlike `GainCredits`/`DrawCards` —
    /// tags exist solely on `RunnerState` in this data model, so
    /// `Side::Corp` would never be a legal target.
    GiveTags(u32),
    TrashCard(CardTarget),
}
