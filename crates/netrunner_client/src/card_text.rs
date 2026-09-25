//! The printed text as a card face draws it: runs of words, the symbols
//! Netrunner prints as icons, and line breaks.
//!
//! NetrunnerDB writes the icons as tokens — `1[credit]`, `[click]:`,
//! `[subroutine] End the run.` — and `CardDefinition::printed_text`
//! keeps them that way, because the card files quote them verbatim
//! (the Linked Clause Rule). A terminal shows the tokens as they are;
//! a graphical face wants the icon. This module is the split: a face
//! walks [`segments`] and draws a [`Segment::Symbol`] with a glyph, and
//! everything else as text. No rule reads any of it.
//!
//! **Glyphs, and what happens without them.** [`Symbol::glyph`] is a
//! code point in Noto Sans Symbols 2 (the desktop's second font, checked
//! against its character map when chosen — Noto Sans itself has no
//! arrows, geometric shapes or dingbats, so `◆` and `●` would be tofu in
//! it). [`Symbol::fallback`] is what a face draws when that font is not
//! loaded: Latin-1 where a glyph reads as the icon (`¢`, `»`), a short
//! word where none does. Both are strings rather than `char`s because a
//! recurring credit is two glyphs.
//!
//! **The real icons.** NetrunnerDB's own icon font (committed with the
//! desktop client as `assets/fonts/NetrunnerDB-Icons.ttf`) draws every printed
//! symbol as the card prints it, plus each faction's mark and each set's.
//! [`Symbol::icon`], [`faction_icon`] and [`set_icon`] are its code
//! points, read off the site's `netrunnerfont.css` (private-use block
//! U+E900–U+E935, the whole of the font's character map). A face draws
//! these when the font is loaded, the Noto glyphs when it is not, and
//! the fallbacks when neither font is.

use netrunner_core::card::Faction;

/// An icon Netrunner prints inline with its rules text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbol {
    Credit,
    Click,
    Subroutine,
    Trash,
    Mu,
    RecurringCredit,
    Link,
    Interrupt,
}

impl Symbol {
    pub const ALL: [Symbol; 8] = [
        Symbol::Credit,
        Symbol::Click,
        Symbol::Subroutine,
        Symbol::Trash,
        Symbol::Mu,
        Symbol::RecurringCredit,
        Symbol::Link,
        Symbol::Interrupt,
    ];

    /// The token NetrunnerDB prints, brackets included.
    pub fn token(self) -> &'static str {
        match self {
            Symbol::Credit => "[credit]",
            Symbol::Click => "[click]",
            Symbol::Subroutine => "[subroutine]",
            Symbol::Trash => "[trash]",
            Symbol::Mu => "[mu]",
            Symbol::RecurringCredit => "[recurring-credit]",
            Symbol::Link => "[link]",
            Symbol::Interrupt => "[interrupt]",
        }
    }

    /// The glyph in Noto Sans Symbols 2. Each was checked against the
    /// font's character map: U+00A2 `¢`, U+23F1 `⏱`, U+25B8 `▸`, U+1F5D1
    /// `🗑`, U+25A3 `▣`, U+2B6E `⭮`, U+26D3 `⛓`, U+26A1 `⚡`. The
    /// arrows block is *not* in that font (nor in Noto Sans), which is why
    /// the subroutine is a triangle rather than the hooked arrow the card
    /// prints.
    pub fn glyph(self) -> &'static str {
        match self {
            Symbol::Credit => "¢",
            Symbol::Click => "⏱",
            Symbol::Subroutine => "▸",
            Symbol::Trash => "🗑",
            Symbol::Mu => "▣",
            Symbol::RecurringCredit => "¢⭮",
            Symbol::Link => "⛓",
            Symbol::Interrupt => "⚡",
        }
    }

    /// The glyph in NetrunnerDB's icon font: `.icon-subroutine` is
    /// U+E900, `.icon-mu` U+E904, `.icon-trash` U+E905, `.icon-link`
    /// U+E908, `.icon-click` U+E909, `.icon-recurring-credit` U+E90A,
    /// `.icon-credit` U+E90B, `.icon-interrupt` U+E92B.
    pub fn icon(self) -> char {
        match self {
            Symbol::Credit => '\u{e90b}',
            Symbol::Click => '\u{e909}',
            Symbol::Subroutine => '\u{e900}',
            Symbol::Trash => '\u{e905}',
            Symbol::Mu => '\u{e904}',
            Symbol::RecurringCredit => '\u{e90a}',
            Symbol::Link => '\u{e908}',
            Symbol::Interrupt => '\u{e92b}',
        }
    }

    /// What to draw with only Noto Sans: Latin-1 where it reads as the
    /// icon, a word where it does not.
    pub fn fallback(self) -> &'static str {
        match self {
            Symbol::Credit => "¢",
            Symbol::Click => "click",
            Symbol::Subroutine => "»",
            Symbol::Trash => "trash",
            Symbol::Mu => "MU",
            Symbol::RecurringCredit => "¢ recurring",
            Symbol::Link => "link",
            Symbol::Interrupt => "interrupt",
        }
    }

    fn from_token(token: &str) -> Option<Symbol> {
        Symbol::ALL.into_iter().find(|symbol| symbol.token() == token)
    }
}

/// A faction's mark in NetrunnerDB's icon font. Both neutrals are the
/// one Null Signal Games mark, as the site draws them (`.icon-neutral-corp`
/// and `.icon-neutral-runner` are the same code point).
pub fn faction_icon(faction: Faction) -> char {
    match faction {
        Faction::Anarch => '\u{e91a}',
        Faction::Criminal => '\u{e919}',
        Faction::Shaper => '\u{e91b}',
        Faction::HaasBioroid => '\u{e918}',
        Faction::Jinteki => '\u{e916}',
        Faction::Nbn => '\u{e915}',
        Faction::WeylandConsortium => '\u{e917}',
        Faction::NeutralCorp | Faction::NeutralRunner => '\u{e92f}',
    }
}

/// A set's mark in NetrunnerDB's icon font, for the pack codes the
/// embedded catalog carries (`core`, `sg`, `elev`) and the two other
/// codes that name their icon outright (`core2`, `sc19`). The font has
/// a mark per *cycle*, and a pack code that is not a cycle name needs a
/// table this crate does not have, so a set it cannot place gets none
/// rather than a guess.
pub fn set_icon(set_code: &str) -> Option<char> {
    match set_code {
        "core" => Some('\u{e914}'),
        "core2" => Some('\u{e924}'),
        "sc19" => Some('\u{e928}'),
        "sg" => Some('\u{e92d}'),
        "elev" => Some('\u{e934}'),
        _ => None,
    }
}

/// One piece of a printed text, in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Symbol(Symbol),
    /// A trace strength: NetrunnerDB writes `Trace[3]` for the raised
    /// `³` the card prints after the word.
    Superscript(u32),
    Break,
}

/// The digits `0`–`9` raised, as a face draws a trace strength. `¹²³`
/// are Latin-1 and the rest U+2074–2079; Noto Sans has them all.
pub fn superscript(n: u32) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    n.to_string().chars().map(|c| DIGITS[c.to_digit(10).unwrap_or(0) as usize]).collect()
}

/// The printed text split at its symbols, trace strengths and line
/// breaks. A bracketed word that is none of those — there are none on
/// record, but a homebrew card may print one — stays as text, brackets
/// and all.
pub fn segments(printed: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut Vec<Segment>| {
        if !text.is_empty() {
            out.push(Segment::Text(std::mem::take(text)));
        }
    };
    let mut rest = printed;
    while let Some(ch) = rest.chars().next() {
        match ch {
            '\n' => {
                flush(&mut text, &mut out);
                out.push(Segment::Break);
                rest = &rest[1..];
            }
            '[' => match rest.find(']').and_then(|end| bracketed(&rest[..=end]).map(|segment| (segment, end))) {
                Some((segment, end)) => {
                    flush(&mut text, &mut out);
                    out.push(segment);
                    rest = &rest[end + 1..];
                }
                None => {
                    text.push('[');
                    rest = &rest[1..];
                }
            },
            _ => {
                text.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
    }
    flush(&mut text, &mut out);
    out
}

/// A `[token]` as a segment: a symbol, or a trace strength's digits.
fn bracketed(token: &str) -> Option<Segment> {
    if let Some(symbol) = Symbol::from_token(token) {
        return Some(Segment::Symbol(symbol));
    }
    token.strip_prefix('[').and_then(|t| t.strip_suffix(']')).and_then(|digits| digits.parse().ok()).map(Segment::Superscript)
}

/// The segments joined back into one string, each symbol as its glyph
/// or, with `glyphs` false, its fallback. What a face's text reads as
/// end to end — the terminal's inspector and the desktop's tests both
/// want it.
pub fn render(segments: &[Segment], glyphs: bool) -> String {
    let mut out = String::new();
    for segment in segments {
        match segment {
            Segment::Text(text) => out.push_str(text),
            Segment::Symbol(symbol) => out.push_str(if glyphs { symbol.glyph() } else { symbol.fallback() }),
            Segment::Superscript(n) => out.push_str(&superscript(*n)),
            Segment::Break => out.push('\n'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every icon is inside the font's character map (U+E900–U+E935,
    /// per `fc-query`), the symbols are distinct from one another and
    /// from the factions, and the two neutrals share one mark.
    #[test]
    fn the_icons_are_inside_the_fonts_map_and_distinct() {
        let in_map = |c: char| ('\u{e900}'..='\u{e935}').contains(&c);
        let mut seen = std::collections::HashSet::new();
        for symbol in Symbol::ALL {
            assert!(in_map(symbol.icon()), "{symbol:?}");
            assert!(seen.insert(symbol.icon()), "{symbol:?} shares an icon");
        }
        for faction in [Faction::Anarch, Faction::Criminal, Faction::Shaper, Faction::HaasBioroid, Faction::Jinteki, Faction::Nbn, Faction::WeylandConsortium, Faction::NeutralCorp] {
            assert!(in_map(faction_icon(faction)), "{faction:?}");
            assert!(seen.insert(faction_icon(faction)), "{faction:?} shares an icon");
        }
        assert_eq!(faction_icon(Faction::NeutralRunner), faction_icon(Faction::NeutralCorp));
        for set in ["core", "sg", "elev"] {
            assert!(set_icon(set).is_some_and(in_map), "{set}");
        }
        assert_eq!(set_icon("homebrew"), None);
    }

    #[test]
    fn tithe_splits_into_symbols_text_and_a_break() {
        let segments = segments("[subroutine] Do 1 net damage.\n[subroutine] Gain 1[credit].");
        assert_eq!(
            segments,
            vec![
                Segment::Symbol(Symbol::Subroutine),
                Segment::Text(" Do 1 net damage.".to_string()),
                Segment::Break,
                Segment::Symbol(Symbol::Subroutine),
                Segment::Text(" Gain 1".to_string()),
                Segment::Symbol(Symbol::Credit),
                Segment::Text(".".to_string()),
            ]
        );
        assert_eq!(render(&segments, false), "» Do 1 net damage.\n» Gain 1¢.");
        assert_eq!(render(&segments, true), "▸ Do 1 net damage.\n▸ Gain 1¢.");
    }

    /// Ichi 1.0: `Trace[1]` is the word and a raised digit.
    #[test]
    fn a_trace_strength_is_a_superscript() {
        let segments = segments("Trace[1]. If successful, do 1 core damage.");
        assert_eq!(segments[0], Segment::Text("Trace".to_string()));
        assert_eq!(segments[1], Segment::Superscript(1));
        assert_eq!(render(&segments, true), "Trace¹. If successful, do 1 core damage.");
        assert_eq!(superscript(10), "¹⁰");
    }

    #[test]
    fn an_unknown_bracket_stays_literal_and_a_bare_bracket_does_not_panic() {
        assert_eq!(segments("[foo] and [ open"), vec![Segment::Text("[foo] and [ open".to_string())]);
        assert_eq!(segments(""), vec![]);
        assert_eq!(segments("[credit]"), vec![Segment::Symbol(Symbol::Credit)]);
    }

    /// Every token has a glyph and a fallback, and no two tokens share a
    /// glyph — a face that drew two icons the same would mislead.
    #[test]
    fn every_symbol_round_trips_through_its_token() {
        let mut glyphs = std::collections::HashSet::new();
        for symbol in Symbol::ALL {
            assert_eq!(segments(symbol.token()), vec![Segment::Symbol(symbol)]);
            assert!(!symbol.glyph().is_empty() && !symbol.fallback().is_empty());
            assert!(glyphs.insert(symbol.glyph()), "{symbol:?} shares a glyph");
        }
    }

    /// The whole catalog parses: no card text carries a bracket the
    /// splitter does not know, which would show up as a literal token on
    /// a face.
    #[test]
    fn every_catalog_text_has_only_known_tokens() {
        let catalog = netrunner_core::cards::load_embedded_netrunnerdb_sets().unwrap();
        for card in catalog.iter() {
            let Some(text) = &card.printed_text else { continue };
            for segment in segments(text) {
                if let Segment::Text(text) = segment {
                    assert!(!text.contains('['), "{}: unknown token in {text:?}", card.title);
                }
            }
        }
    }
}
