//! The strategy guide both clients show beside Learn to Play: how a game
//! tends to unfold, what each side is after at each stage, and the deck
//! styles and factions — the advice the rules leave to the player.
//!
//! **One text, in the repository, read by both clients.** The guide is
//! `docs/strategy-guide.md`, embedded here at compile time, so the page a
//! person reads on GitHub and the one they read in either client cannot
//! drift apart. It is written in the few pieces of Markdown this module
//! reads — `#` for the title, `##` for a chapter, `###` for a section,
//! paragraphs, `- ` bullets, `**strong**` and `[text](url)` — rather than
//! a Markdown library, because a client needs the text as spans it can
//! draw and nothing more, and a construct outside that list is refused by
//! a test rather than shown as punctuation.
//!
//! **A card is a link to its NetrunnerDB page**, so GitHub renders it as
//! one and a client knows which card is meant (`Span::Card`). The test
//! below holds every such link to a card in this client's playable pool,
//! under the title the pool prints: a guide that recommended a card the
//! deck builder cannot find would send a new player looking for nothing.

use netrunner_core::card::CardId;
use netrunner_core::cards::CardRegistry;

/// The guide's source, as committed.
pub const SOURCE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/strategy-guide.md"));

/// Where a link to a card's page on NetrunnerDB starts; the card's code
/// follows it.
const CARD_URL: &str = "https://netrunnerdb.com/en/card/";

/// The guide: its title, what it says before the first chapter, and the
/// chapters in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guide {
    pub title: String,
    pub intro: Vec<Block>,
    pub chapters: Vec<Chapter>,
}

/// One `##` chapter: the unit a client lists and shows one at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A `###` section heading within a chapter.
    Heading(String),
    Paragraph(Vec<Span>),
    /// One `- ` item. A run of them is a list; a client need not group them.
    Bullet(Vec<Span>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Span {
    Text(String),
    Strong(String),
    /// A card, by the title the guide prints and its NetrunnerDB code.
    Card { title: String, code: CardId },
    /// A link to anything else; a client shows its text.
    Link { text: String, url: String },
}

impl Span {
    /// The words the span shows, whatever its kind.
    pub fn text(&self) -> &str {
        match self {
            Span::Text(text) | Span::Strong(text) => text,
            Span::Card { title, .. } => title,
            Span::Link { text, .. } => text,
        }
    }
}

impl Block {
    /// The block's words as one string, for a client that draws plain
    /// text and for tests.
    pub fn plain(&self) -> String {
        match self {
            Block::Heading(text) => text.clone(),
            Block::Paragraph(spans) | Block::Bullet(spans) => spans.iter().map(Span::text).collect(),
        }
    }
}

/// The committed guide, read.
pub fn guide() -> Guide {
    parse(SOURCE)
}

/// Reads a guide. It never fails: a construct this module does not read
/// comes through as text, and `every_line_is_a_construct_the_reader_knows`
/// is what keeps the committed guide from containing one.
pub fn parse(source: &str) -> Guide {
    let mut guide = Guide { title: String::new(), intro: Vec::new(), chapters: Vec::new() };
    let mut paragraph: Vec<&str> = Vec::new();
    let flush = |paragraph: &mut Vec<&str>, guide: &mut Guide| {
        if !paragraph.is_empty() {
            let block = Block::Paragraph(spans(&paragraph.join(" ")));
            paragraph.clear();
            blocks_of(guide).push(block);
        }
    };
    for line in source.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            flush(&mut paragraph, &mut guide);
        } else if let Some(title) = line.strip_prefix("### ") {
            flush(&mut paragraph, &mut guide);
            blocks_of(&mut guide).push(Block::Heading(title.trim().to_string()));
        } else if let Some(title) = line.strip_prefix("## ") {
            flush(&mut paragraph, &mut guide);
            guide.chapters.push(Chapter { title: title.trim().to_string(), blocks: Vec::new() });
        } else if let Some(title) = line.strip_prefix("# ") {
            flush(&mut paragraph, &mut guide);
            guide.title = title.trim().to_string();
        } else if let Some(item) = line.strip_prefix("- ") {
            flush(&mut paragraph, &mut guide);
            blocks_of(&mut guide).push(Block::Bullet(spans(item.trim())));
        } else {
            paragraph.push(line.trim());
        }
    }
    flush(&mut paragraph, &mut guide);
    guide
}

/// Where the next block goes: the last chapter, or the intro before one.
fn blocks_of(guide: &mut Guide) -> &mut Vec<Block> {
    match guide.chapters.last_mut() {
        Some(chapter) => &mut chapter.blocks,
        None => &mut guide.intro,
    }
}

/// A line's inline pieces: `**strong**` and `[text](url)`, the rest text.
fn spans(line: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut rest = line;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("**")
            && let Some(end) = after.find("**")
        {
            push_text(&mut out, &mut text);
            out.push(Span::Strong(after[..end].to_string()));
            rest = &after[end + 2..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('[')
            && let Some((label, url, tail)) = link(after)
        {
            push_text(&mut out, &mut text);
            out.push(match url.strip_prefix(CARD_URL).and_then(|code| code.parse::<u32>().ok()) {
                Some(code) => Span::Card { title: label.to_string(), code: CardId(code) },
                None => Span::Link { text: label.to_string(), url: url.to_string() },
            });
            rest = tail;
            continue;
        }
        let next = rest.chars().next().expect("rest is not empty");
        text.push(next);
        rest = &rest[next.len_utf8()..];
    }
    push_text(&mut out, &mut text);
    out
}

/// `text](url)` and what follows it, from just after the `[`.
fn link(after: &str) -> Option<(&str, &str, &str)> {
    let close = after.find("](")?;
    let tail = &after[close + 2..];
    let end = tail.find(')')?;
    Some((&after[..close], &tail[..end], &tail[end + 1..]))
}

fn push_text(out: &mut Vec<Span>, text: &mut String) {
    if !text.is_empty() {
        out.push(Span::Text(std::mem::take(text)));
    }
}

/// Every card the guide names, in order, with repeats.
pub fn cards(guide: &Guide) -> Vec<(&str, CardId)> {
    guide
        .intro
        .iter()
        .chain(guide.chapters.iter().flat_map(|chapter| chapter.blocks.iter()))
        .flat_map(|block| match block {
            Block::Paragraph(spans) | Block::Bullet(spans) => spans.as_slice(),
            Block::Heading(_) => &[],
        })
        .filter_map(|span| match span {
            Span::Card { title, code } => Some((title.as_str(), *code)),
            _ => None,
        })
        .collect()
}

/// The cards the guide names that this client cannot build with, each
/// with why: no card has the code, the card is not playable, or the
/// guide prints a title other than the card's.
pub fn unplayable_cards(guide: &Guide, registry: &CardRegistry) -> Vec<String> {
    cards(guide)
        .into_iter()
        .filter_map(|(title, code)| match registry.get_by_numeric_id(code) {
            None => Some(format!("{title} ({}): no card has this code", code.0)),
            Some(card) if !card.is_playable => Some(format!("{title} ({}): not playable", code.0)),
            Some(card) if card.title != title => Some(format!("{title} ({}): the card is titled {:?}", code.0, card.title)),
            Some(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::default();
        netrunner_core::cards::register_playable_cards(&mut registry);
        registry
    }

    #[test]
    fn the_guide_has_a_title_and_its_chapters() {
        let guide = guide();
        assert_eq!(guide.title, "Strategy Guide");
        assert!(!guide.intro.is_empty(), "the guide says what it is before its first chapter");
        let titles: Vec<&str> = guide.chapters.iter().map(|chapter| chapter.title.as_str()).collect();
        for expected in ["The fundamentals", "How a game moves", "Playing the Corp", "Playing the Runner", "Corp deck styles", "The Corp factions", "The Runner factions"] {
            assert!(titles.contains(&expected), "no chapter {expected:?} in {titles:?}");
        }
        assert!(guide.chapters.iter().all(|chapter| !chapter.blocks.is_empty()), "an empty chapter");
    }

    #[test]
    fn every_card_the_guide_names_is_in_the_playable_pool_under_its_own_title() {
        let guide = guide();
        assert!(cards(&guide).len() > 40, "the guide names its examples as cards");
        let bad = unplayable_cards(&guide, &registry());
        assert!(bad.is_empty(), "the guide names cards a player cannot build with:\n{}", bad.join("\n"));
    }

    #[test]
    fn a_card_the_pool_lacks_is_named_as_one() {
        let guide = parse("## Chapter\n\n[Biotic Labor](https://netrunnerdb.com/en/card/01057) and [Hedge Fund](https://netrunnerdb.com/en/card/30075).");
        let bad = unplayable_cards(&guide, &registry());
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].starts_with("Biotic Labor"), "{bad:?}");
    }

    /// The reader knows six constructs; anything else in the committed
    /// guide would reach a player as stray punctuation.
    #[test]
    fn every_line_is_a_construct_the_reader_knows() {
        for (number, line) in SOURCE.lines().enumerate() {
            let number = number + 1;
            for refused in ["|", "```", "* ", "1. ", "> ", "<"] {
                assert!(!line.trim_start().starts_with(refused), "line {number} starts with {refused:?}, which the reader does not know: {line}");
            }
            assert!(!line.starts_with("####"), "line {number}: only #, ## and ### are headings");
        }
        let guide = guide();
        let blocks = guide.intro.iter().chain(guide.chapters.iter().flat_map(|chapter| chapter.blocks.iter()));
        for block in blocks {
            let plain = block.plain();
            for leftover in ["*", "](", "[[", "_"] {
                assert!(!plain.contains(leftover), "{leftover:?} survived the reader in: {plain}");
            }
        }
    }

    #[test]
    fn inline_pieces_are_read() {
        let spans = spans("Run **HQ** with [Jailbreak](https://netrunnerdb.com/en/card/30028), see [this](https://example.com).");
        assert_eq!(
            spans,
            vec![
                Span::Text("Run ".into()),
                Span::Strong("HQ".into()),
                Span::Text(" with ".into()),
                Span::Card { title: "Jailbreak".into(), code: CardId(30028) },
                Span::Text(", see ".into()),
                Span::Link { text: "this".into(), url: "https://example.com".into() },
                Span::Text(".".into()),
            ]
        );
    }

    #[test]
    fn a_paragraph_joins_its_lines_and_a_bullet_stands_alone() {
        let guide = parse("# T\n\nintro\n\n## A\n\none\ntwo\n\n- item\n### S\nthree");
        assert_eq!(guide.intro, vec![Block::Paragraph(vec![Span::Text("intro".into())])]);
        assert_eq!(
            guide.chapters[0].blocks,
            vec![
                Block::Paragraph(vec![Span::Text("one two".into())]),
                Block::Bullet(vec![Span::Text("item".into())]),
                Block::Heading("S".into()),
                Block::Paragraph(vec![Span::Text("three".into())]),
            ]
        );
    }
}
