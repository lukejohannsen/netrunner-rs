//! The strategy guide in the terminal: the chapters down the left, the
//! chapter being read on the right, one at a time — the same text the
//! desktop's guide screen shows (`netrunner_client::guide`, the committed
//! `docs/strategy-guide.md`).
//!
//! A plain struct with a `key` function, like the rest of the menu, so it
//! is tested without a terminal. Card names are drawn in cyan and strong
//! words in bold; a section heading is yellow, as the Learn screen draws
//! its headings.

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span as TextSpan};
use ratatui::widgets::{Block as Panel, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_client::guide::{self, Block, Guide, Span};

#[derive(Debug, Clone)]
pub struct GuideReader {
    guide: Guide,
    chapter: usize,
    scroll: u16,
}

/// What a key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideKey {
    Continue,
    Back,
}

impl GuideReader {
    pub fn new() -> Self {
        GuideReader { guide: guide::guide(), chapter: 0, scroll: 0 }
    }

    pub fn chapter_title(&self) -> &str {
        &self.guide.chapters[self.chapter].title
    }

    /// Left and Right (or Tab) turn the chapter and go to its top; Up and
    /// Down, Page Up and Page Down scroll it; Esc goes back.
    pub fn key(&mut self, key: KeyCode) -> GuideKey {
        let chapters = self.guide.chapters.len();
        match key {
            KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => self.turn((self.chapter + 1) % chapters),
            KeyCode::Left | KeyCode::BackTab | KeyCode::Char('h') => self.turn((self.chapter + chapters - 1) % chapters),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll = self.scroll.saturating_add(10),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::Esc | KeyCode::Char('q') => return GuideKey::Back,
            _ => {}
        }
        GuideKey::Continue
    }

    fn turn(&mut self, chapter: usize) {
        self.chapter = chapter;
        self.scroll = 0;
    }

    /// The chapter being read, as the lines the page draws: the guide's
    /// introduction above the first chapter, a blank line between blocks.
    pub fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let intro = if self.chapter == 0 { self.guide.intro.as_slice() } else { &[] };
        for block in intro.iter().chain(&self.guide.chapters[self.chapter].blocks) {
            if !lines.is_empty() {
                lines.push(Line::default());
            }
            lines.push(line(block));
        }
        lines
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let [contents, page] = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Length(28), Constraint::Min(0)]).areas(area);
        let items: Vec<ListItem> = self.guide.chapters.iter().map(|chapter| ListItem::new(chapter.title.clone())).collect();
        let mut state = ListState::default();
        state.select(Some(self.chapter));
        frame.render_stateful_widget(
            List::new(items)
                .block(Panel::default().borders(Borders::ALL).title(self.guide.title.clone()))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            contents,
            &mut state,
        );
        frame.render_widget(
            Paragraph::new(self.lines())
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0))
                .block(Panel::default().borders(Borders::ALL).title(format!("{} — Left/Right chapter, Up/Down scroll, Esc goes back", self.chapter_title()))),
            page,
        );
    }
}

fn line(block: &Block) -> Line<'static> {
    match block {
        Block::Heading(text) => Line::from(TextSpan::styled(text.clone(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Block::Paragraph(spans) => Line::from(spans.iter().map(span).collect::<Vec<_>>()),
        Block::Bullet(spans) => Line::from(std::iter::once(TextSpan::raw("• ")).chain(spans.iter().map(span)).collect::<Vec<_>>()),
    }
}

fn span(span: &Span) -> TextSpan<'static> {
    let style = match span {
        Span::Card { .. } => Style::default().fg(Color::Cyan),
        Span::Strong(_) => Style::default().add_modifier(Modifier::BOLD),
        Span::Text(_) | Span::Link { .. } => Style::default(),
    };
    TextSpan::styled(span.text().to_string(), style)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(reader: &GuideReader) -> String {
        reader.lines().iter().map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>()).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn a_chapter_is_read_whole_and_turning_goes_to_the_next_ones_top() {
        let mut reader = GuideReader::new();
        assert!(text(&reader).starts_with("Learn to Play teaches the rules"), "the introduction opens the first chapter");
        reader.key(KeyCode::Down);
        reader.key(KeyCode::Down);
        assert_eq!(reader.scroll, 2);
        reader.key(KeyCode::Right);
        assert_eq!((reader.chapter, reader.scroll), (1, 0));
        assert!(!text(&reader).contains("Learn to Play teaches the rules"), "the introduction is the first chapter's alone");
        reader.key(KeyCode::Left);
        reader.key(KeyCode::Left);
        assert_eq!(reader.chapter, reader.guide.chapters.len() - 1, "Left from the first wraps to the last");
        assert_eq!(reader.key(KeyCode::Esc), GuideKey::Back);
    }

    #[test]
    fn a_card_is_drawn_in_its_colour() {
        let reader = GuideReader::new();
        let hedge = reader.lines().into_iter().flat_map(|line| line.spans).find(|span| span.content == "Hedge Fund").expect("the first chapter names Hedge Fund");
        assert_eq!(hedge.style.fg, Some(Color::Cyan));
    }
}
