//! A card's printings as a strip of pictures, each the button that draws
//! the card with its art (Phase 7 §10, Stage 3).
//!
//! **The card is its own button**, as a choice between cards is on the
//! board: a strip of each printing's scan, the one the card is drawn with
//! outlined, under the set's mark and the printing's number, rather than
//! a list of set names with "Use this art" beside each. The person sees
//! what they are choosing. **The newest printing is the default**, so
//! choosing it forgets the choice instead of keeping it: a card left on
//! its newest is drawn as a reprint to come when the catalog gains one,
//! and there is no third button for "default" beside two pictures.
//!
//! The strip is drawn under the card browser's inspector and under a card
//! opened to read (`reader`) — on the Cards screen, the deck builder and
//! the decks screen — and nowhere on the board: art is chosen while
//! looking at a card, not in the middle of a game, and a sheet on the
//! board carries no buttons. It is the one set of buttons the reader
//! carries, because a choice of art is a preference, not something a card
//! does. A press is taken here, for every screen: the settings file is
//! written and [`ArtChanged`] tells whichever screen is up to redraw the
//! faces it holds.
//!
//! **Never sent anywhere.** The choice is the settings file's
//! (`netrunner_client::art`); no server and no `ClientView` sees it.

use bevy::prelude::*;

use netrunner_client::art::{self, Art, Picture};
use netrunner_client::card_face::Face;
use netrunner_core::card::PrintingId;
use netrunner_core::cards::catalog;
use netrunner_core::dsl::{CardDefinition, CardId};

use crate::card_images::CardImages;
use crate::core::{ClientCore, Notices};
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_face, FaceSize};

/// A printing's picture in the strip; pressing it draws `card` with it.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ArtButton {
    pub card: CardId,
    pub printing: PrintingId,
}

/// A card's art changed: a screen redraws whatever faces of it it holds.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct ArtChanged(pub CardId);

/// How wide each printing is drawn in the strip.
const STRIP_FACE: FaceSize = FaceSize::Board(84);

/// The width of a column holding the strip one printing to a row — the
/// reader's, beside the card.
pub const STRIP_COLUMN: f32 = 84.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_message::<ArtChanged>().add_systems(Update, choose);
}

/// The strip for `card`: a heading and one picture per printing, newest
/// first, the one it is drawn with outlined. Nothing for a card with one
/// printing, which has no art to choose.
pub fn spawn_strip(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, card: &CardDefinition) {
    if !has_art_to_choose(&card.id) {
        return;
    }
    let printings: Vec<_> = catalog::printings_of(&card.id).collect();
    let shown = art::printing_for(card, &core.settings.art);
    let face = Face::of(card, &core.settings.art);
    parent.spawn(crate::widgets::label(theme, "Art"));
    parent.spawn((Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(8), row_gap: px(8), flex_shrink: 0.0, ..default() },)).with_children(|row| {
        for printing in printings {
            let picture = Picture::Printing(printing.id);
            row.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(4), width: px(STRIP_FACE.width()), ..default() },)).with_children(|cell| {
                let drawn = Face { picture: Some(picture.clone()), ..face.clone() };
                let image = images.face(&picture, STRIP_FACE);
                let entity = spawn_face(cell, theme, &drawn, STRIP_FACE, image, (Button, ArtButton { card: card.id.clone(), printing: printing.id }));
                if shown == Some(printing.id) {
                    cell.commands().entity(entity).insert(Outline { width: px(3), offset: px(1), color: theme.accent });
                }
                cell.spawn((Text::new(""), theme.font(size::SMALL), TextColor(theme.text_dim))).with_children(|spans| {
                    if let Some((mark, font)) = theme.set_icon(&printing.set, size::SMALL) {
                        spans.spawn((TextSpan::new(format!("{mark} ")), font, TextColor(theme.text_dim)));
                    }
                    spans.spawn((TextSpan::new(format!("#{}", printing.id)), theme.font(size::SMALL), TextColor(theme.text_dim)));
                });
            });
        }
    });
}

/// Whether `card` is printed more than once, so has a strip at all.
pub fn has_art_to_choose(card: &CardId) -> bool {
    catalog::printings_of(card).nth(1).is_some()
}

/// The choice a press on `printing` makes for `card`: that printing, or
/// none when it is the newest, which is what is drawn without one.
pub fn choice_for(card: &CardId, printing: PrintingId) -> Option<Art> {
    let newest = catalog::latest_printing(card).map(|newest| newest.id);
    (newest != Some(printing)).then_some(Art::Printing(printing))
}

/// Takes a press on a printing: keeps the choice in the settings file and
/// says so, unless it is the choice already made.
fn choose(
    pressed: Query<(&Interaction, &ArtButton), Changed<Interaction>>,
    core: Option<ResMut<ClientCore>>,
    notices: Option<ResMut<Notices>>,
    mut changed: MessageWriter<ArtChanged>,
) {
    let (Some(mut core), Some(mut notices)) = (core, notices) else { return };
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let choice = choice_for(&button.card, button.printing);
        if core.settings.art.get(&button.card) == choice.as_ref() {
            continue;
        }
        core.settings.art.set(button.card.clone(), choice);
        if let Err(error) = core.save_settings() {
            notices.push(format!("Settings not saved: {error}"));
        }
        changed.write(ArtChanged(button.card.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Choosing an older printing keeps it; choosing the newest is the
    /// default, and keeps nothing.
    #[test]
    fn choosing_the_newest_printing_is_choosing_nothing() {
        let hedge_fund = CardId("hedge_fund".to_string());
        assert_eq!(choice_for(&hedge_fund, PrintingId(1110)), Some(Art::Printing(PrintingId(1110))));
        assert_eq!(choice_for(&hedge_fund, PrintingId(30075)), None);
    }
}
