//! The Runner's rig as the table lays it out: three rows — programs,
//! hardware, resources — with programs the row nearest the ICE they
//! break and resources the row nearest the Runner.
//!
//! The rules give an installed Runner card no position ("does not
//! matter" — the learn-to-play guide). The table does, and a person
//! reading a rig reads it by row: what breaks, what it runs on, what
//! pays for it. jinteki.net draws the same three rows. Both clients take
//! their order from [`rows_top_down`], so no screen decides it — the rule
//! [`table_servers`](super::table_servers) is for the servers.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardType;
use netrunner_core::rules::{PublicInstalledRunnerCard, Side};
use netrunner_core::view::ClientView;

/// Which of the rig's rows a card sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigRow {
    Programs,
    Hardware,
    /// Resources, and anything else the Runner installs.
    Resources,
}

impl RigRow {
    /// The row a card of `card_type` sits in.
    pub fn of(card_type: &CardType) -> Self {
        match card_type {
            CardType::Program => RigRow::Programs,
            CardType::Hardware => RigRow::Hardware,
            _ => RigRow::Resources,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RigRow::Programs => "Programs",
            RigRow::Hardware => "Hardware",
            RigRow::Resources => "Resources",
        }
    }
}

/// The rig's rows top to bottom as the chair sees them. From the
/// Runner's chair the ICE is above the rig and the hand below it, so
/// programs come first; from the Corp's the rig sits across the table
/// with the ICE below it, so the order turns over and programs are still
/// the row next to the ICE.
pub fn rows_top_down(chair: Side) -> [RigRow; 3] {
    match chair {
        Side::Runner => [RigRow::Programs, RigRow::Hardware, RigRow::Resources],
        Side::Corp => [RigRow::Resources, RigRow::Hardware, RigRow::Programs],
    }
}

/// The rig's cards in their rows, in [`rows_top_down`] order for `chair`,
/// each row in the view's order. Every row is listed, empty or not. A
/// card the registry does not know goes nowhere, as it has no face to
/// draw.
pub fn rows<'a>(view: &'a ClientView, registry: &CardRegistry, chair: Side) -> [(RigRow, Vec<&'a PublicInstalledRunnerCard>); 3] {
    rows_top_down(chair).map(|row| (row, view.runner.rig.iter().filter(|card| registry.get(&card.card).is_some_and(|def| RigRow::of(&def.card_type) == row)).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Programs are the row next to the ICE from either chair, and
    /// resources the row next to the Runner.
    #[test]
    fn the_rows_are_the_table_seen_from_the_chair() {
        assert_eq!(rows_top_down(Side::Runner), [RigRow::Programs, RigRow::Hardware, RigRow::Resources]);
        assert_eq!(rows_top_down(Side::Corp), [RigRow::Resources, RigRow::Hardware, RigRow::Programs]);
        assert_eq!(RigRow::of(&CardType::Program), RigRow::Programs);
        assert_eq!(RigRow::of(&CardType::Hardware), RigRow::Hardware);
        assert_eq!(RigRow::of(&CardType::Resource), RigRow::Resources);
    }
}
