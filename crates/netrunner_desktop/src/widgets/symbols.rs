//! A printed symbol in any label is drawn as the symbol, not its token.
//!
//! An ability's label is the card's own clause (the Linked Clause Rule),
//! so it carries NetrunnerDB's tokens verbatim: Cleaver's menu entry read
//! "Break up to 2 barrier subroutines — 1[credit]", and so did the
//! lesson coach that quotes it. The card face already splits its text at
//! the icons (`netrunner_client::card_text::segments`); this is the same
//! split for every other `Text`, done once after the frame's labels are
//! spawned rather than at each of the places that spawn one — a button,
//! a menu entry, a decision, a prompt, the coach — because every one of
//! them would have to remember, and the one that forgot is the bug this
//! answers.
//!
//! **How:** a `Text` whose words hold a symbol keeps the words before the
//! first one, and the rest become `TextSpan` children at the front of its
//! span list, each symbol in the face [`Theme::symbol`] picks (the three
//! asset tiers) at the label's own size and colour. The label is still
//! one `Text`, so it wraps and centres as it did. A label rewritten later
//! is split again: [`Split`] remembers what this system wrote, so its own
//! write is not mistaken for a new label, and the spans it made are
//! replaced rather than left behind.
//!
//! The printed arrow is the one other thing a label is split for: no
//! bundled font has it, so it is drawn as [`ARROW`]'s stand-in.
//!
//! The log is the one place this does not reach, because a log line is
//! already a row of spans (a name each), and `spawn_log_line` splits its
//! words itself with [`spans`].

use bevy::prelude::*;
use netrunner_client::card_text::{segments, superscript, Segment};

use crate::theme::Theme;

/// On a `Text` this system split: the words it left on the root.
#[derive(Component)]
pub struct Split {
    shown: String,
}

/// A span this system made, so a relabel takes exactly these away.
#[derive(Component)]
pub struct SymbolSpan;

/// The arrow a breaker prints after its type ("Interface → 1[credit]").
/// Neither bundled font has the arrows block (`Symbol::glyph` records
/// it), so it was a box on every breaker's menu entry; `›` is in Noto
/// Sans and reads the same way across, as the card browser already
/// draws the engine reading's arrow.
pub const ARROW: char = '→';

/// `words` with the printed arrow drawn as a glyph the font has.
pub fn arrowless(words: &str) -> String {
    words.replace(ARROW, "›")
}

/// `words` as `(text, font)` pieces, each symbol in its own face at
/// `font`'s size: what a label or a log span draws. `None` when there
/// is no symbol in it, which is nearly always.
pub fn spans(theme: &Theme, words: &str, font: &TextFont) -> Option<Vec<(String, TextFont)>> {
    if !words.contains('[') && !words.contains(ARROW) {
        return None;
    }
    let segments = segments(words);
    if !words.contains(ARROW) && !segments.iter().any(|segment| matches!(segment, Segment::Symbol(_))) {
        return None;
    }
    let mut out: Vec<(String, TextFont)> = Vec::new();
    for segment in segments {
        let plain = |text: String| (text, font.clone());
        out.push(match segment {
            Segment::Text(text) => plain(arrowless(&text)),
            Segment::Superscript(n) => plain(superscript(n)),
            Segment::Break => plain("\n".to_string()),
            Segment::Symbol(symbol) => {
                let (glyph, mut face) = theme.symbol(symbol, 0.0);
                face.font_size = font.font_size;
                (glyph, face)
            }
        });
    }
    Some(out)
}

pub fn draw(
    mut commands: Commands,
    theme: Res<Theme>,
    mut texts: Query<(Entity, &mut Text, &TextFont, &TextColor, Option<&Split>, Option<&Children>), Changed<Text>>,
    made: Query<(), With<SymbolSpan>>,
) {
    for (entity, mut text, font, colour, split, children) in &mut texts {
        if let Some(split) = split {
            if split.shown == text.0 {
                continue;
            }
            for child in children.into_iter().flatten().filter(|child| made.contains(**child)) {
                commands.entity(*child).despawn();
            }
            commands.entity(entity).remove::<Split>();
        }
        let Some(pieces) = spans(&theme, &text.0, font) else { continue };
        let mut pieces = pieces.into_iter();
        // The root keeps the first piece when it is words in the label's
        // own face; a label that opens on a symbol leaves the root empty.
        let shown = match pieces.as_slice().first() {
            Some((words, face)) if face.font == font.font => {
                let words = words.clone();
                pieces.next();
                words
            }
            _ => String::new(),
        };
        let made: Vec<Entity> = pieces.map(|(words, face)| commands.spawn((TextSpan::new(words), face, *colour, SymbolSpan)).id()).collect();
        commands.entity(entity).insert(Split { shown: shown.clone() }).insert_children(0, &made);
        text.0 = shown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_with_no_symbol_is_left_alone() {
        let theme = Theme::default();
        assert!(spans(&theme, "Install Cleaver", &theme.font(14.0)).is_none());
        assert!(spans(&theme, "a [bracketed] word", &theme.font(14.0)).is_none());
        let arrow = spans(&theme, "Interface → break", &theme.font(14.0)).expect("the arrow is not in the font");
        assert_eq!(arrow[0].0, "Interface › break");
    }

    #[test]
    fn a_symbol_is_drawn_at_the_labels_size() {
        let theme = Theme::default();
        let font = theme.font(14.0);
        let pieces = spans(&theme, "Break 1 subroutine — 1[credit]", &font).expect("a credit is a symbol");
        let words: Vec<&str> = pieces.iter().map(|(words, _)| words.as_str()).collect();
        let (glyph, _) = theme.symbol(netrunner_client::card_text::Symbol::Credit, 14.0);
        assert_eq!(words, ["Break 1 subroutine — 1", glyph.as_str()]);
        assert!(pieces.iter().all(|(_, face)| face.font_size == font.font_size));
    }

    fn drawn(app: &mut App, label: Entity) -> Vec<String> {
        let world = app.world();
        let mut out = vec![world.get::<Text>(label).unwrap().0.clone()];
        for child in world.get::<Children>(label).into_iter().flatten() {
            out.extend(world.get::<TextSpan>(*child).map(|span| span.0.clone()));
        }
        out
    }

    /// Every symbol a card prints, on a live label: a click, a credit, a
    /// trash, a memory unit — and a label rewritten later is split again,
    /// with no span of the old one left behind.
    #[test]
    fn every_printed_symbol_on_a_label_is_drawn_and_a_relabel_replaces_it() {
        use netrunner_client::card_text::Symbol;
        let mut app = App::new();
        app.insert_resource(Theme::default()).add_systems(Update, draw);
        let theme = Theme::default();
        let glyph = |symbol| theme.symbol(symbol, 14.0).0;
        let label = app.world_mut().spawn((Text::new("[click], [trash]: Gain 2[credit] and +1[mu]."), theme.font(14.0), TextColor::WHITE)).id();
        app.update();
        let words = drawn(&mut app, label);
        assert_eq!(words.concat(), format!("{}, {}: Gain 2{} and +1{}.", glyph(Symbol::Click), glyph(Symbol::Trash), glyph(Symbol::Credit), glyph(Symbol::Mu)));
        assert!(!words.concat().contains('['), "{words:?}");

        app.world_mut().get_mut::<Text>(label).unwrap().0 = "Pay 3[recurring-credit]".into();
        app.update();
        assert_eq!(drawn(&mut app, label).concat(), format!("Pay 3{}", glyph(Symbol::RecurringCredit)));

        app.world_mut().get_mut::<Text>(label).unwrap().0 = "Install Cleaver".into();
        app.update();
        assert_eq!(drawn(&mut app, label), ["Install Cleaver"]);
    }
}
