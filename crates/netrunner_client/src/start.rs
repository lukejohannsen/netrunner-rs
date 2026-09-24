//! The new-game form's state: chair, rung, style and decks, for a player
//! who never touches a flag. Lifted from `netrunner_cli::tui::start` for
//! Phase 7 §3 so the desktop's form and the terminal's are one state
//! machine with two faces — the same lists, the same suggestion rule, the
//! same "everything follows from the chair". The terminal keeps its key
//! bindings, its drawing and the fold into `Config` (`StartChoice` there
//! is this one); the desktop draws each pane as a drop-down.
//!
//! **A plain struct driven by an `Intent`**, tested without a terminal or
//! a window; the choice it produces is in the vocabulary of the flags, so
//! `run_local` runs exactly as it does for `--runner-level 3 --corp-deck
//! brick_stack` (there is one seating rule, one record rule and one deck
//! resolver, and this screen fills in their inputs).

use std::path::Path;

/// Re-exported: the form is *about* a rung and a style, and a client
/// that holds the form should not have to name the bots crate for them.
pub use netrunner_bots::{Level, Personality};
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::Side;

use crate::deck_store::{self, Origin};
use crate::record::LocalRecord;

/// The decks a flag-less game plays: the terminal's `--corp-deck` and
/// `--runner-deck` defaults, and where the desktop's cursors start. One
/// pair, so the two clients cannot default to different games.
pub const DEFAULT_CORP_DECK: &str = "discretion_advised";
pub const DEFAULT_RUNNER_DECK: &str = "stolen_goods";

/// One deck the screen can offer, with what a player needs to tell decks
/// apart: the name, the style a bot would play it in, the identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckRow {
    pub id: String,
    pub name: String,
    pub style: Option<String>,
    pub identity: String,
    pub saved: bool,
}

impl DeckRow {
    pub fn label(&self) -> String {
        let style = self.style.as_deref().unwrap_or("balanced");
        let saved = if self.saved { " (saved)" } else { "" };
        format!("{} · {style} · {}{saved}", self.name, self.identity)
    }
}

/// The panes, in Tab order. Chair first because it decides what the
/// other four offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chair,
    Level,
    Style,
    OpponentDeck,
    OwnDeck,
}

impl Pane {
    const ALL: [Pane; 5] = [Pane::Chair, Pane::Level, Pane::Style, Pane::OpponentDeck, Pane::OwnDeck];

    pub fn next(self) -> Pane {
        let index = Pane::ALL.iter().position(|pane| *pane == self).expect("a pane");
        Pane::ALL[(index + 1) % Pane::ALL.len()]
    }

    pub fn previous(self) -> Pane {
        let index = Pane::ALL.iter().position(|pane| *pane == self).expect("a pane");
        Pane::ALL[(index + Pane::ALL.len() - 1) % Pane::ALL.len()]
    }
}

/// What the player chose, in the vocabulary of the flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartChoice {
    pub human: Side,
    pub level: Level,
    /// `None` is the deck's own style, the same as an unset
    /// `--corp-personality`.
    pub style: Option<Personality>,
    pub corp_deck: String,
    pub runner_deck: String,
}

/// What a client can do to the form. The terminal maps keys onto these;
/// the desktop's drop-downs set a cursor directly (`StartMenu::set_cursor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    NextPane,
    PrevPane,
    /// Up or down within the current pane, wrapping.
    Move(i32),
}

/// The screen's state: a cursor per pane and the lists they move over.
#[derive(Debug, Clone)]
pub struct StartMenu {
    pub pane: Pane,
    chair: usize,
    level: usize,
    style: usize,
    opponent_deck: usize,
    own_deck: usize,
    /// Corp decks, then Runner decks.
    decks: [Vec<DeckRow>; 2],
    /// The rung `record::LocalRecord::suggest` points the player at, as
    /// Corp and as Runner. The level cursor starts here and returns here
    /// when the chair changes.
    suggested: [Level; 2],
    /// The default deck ids from the flags, so the deck cursors start on
    /// what a flag-less run would have played.
    defaults: [String; 2],
}

const SIDES: [Side; 2] = [Side::Corp, Side::Runner];

fn side_index(side: Side) -> usize {
    match side {
        Side::Corp => 0,
        Side::Runner => 1,
    }
}

impl StartMenu {
    /// Reads the saved-deck directory and the record. A record file
    /// that cannot be read suggests the middle rung rather than blocking
    /// the screen; the game itself will refuse it later. `defaults` are
    /// the deck ids a flag-less run would play (Corp, then Runner), so the
    /// deck cursors start there.
    pub fn open(
        decks_dir: &Path,
        record_path: Option<&Path>,
        player: &str,
        registry: &CardRegistry,
        defaults: [String; 2],
    ) -> Result<Self, String> {
        let mut decks: [Vec<DeckRow>; 2] = [Vec::new(), Vec::new()];
        for stored in deck_store::list(decks_dir)? {
            // A `Sweep` deck is a test deck, legal only in Eternal: listed
            // here it would be a choice the game then refuses under the
            // default format. The deck builder still shows it, with its
            // legality, and a copy of one is the person's own deck.
            if stored.deck.category == netrunner_core::decks::DeckCategory::Sweep {
                continue;
            }
            let identity =
                registry.get(&stored.deck.identity).map_or_else(|| stored.deck.identity.0.clone(), |card| card.title.clone());
            decks[side_index(stored.deck.side)].push(DeckRow {
                id: stored.deck.id.clone(),
                name: stored.deck.name.clone(),
                style: stored.deck.style.clone(),
                identity,
                saved: !matches!(stored.origin, Origin::Embedded),
            });
        }
        let suggested = match record_path.map(LocalRecord::load) {
            Some(Ok(log)) => SIDES.map(|side| log.suggest(player, side)),
            _ => [Level::Operator, Level::Operator],
        };
        Ok(Self::with_decks(decks, suggested, defaults))
    }

    /// Puts the cursors back on a game just played — same chair, decks
    /// and style — except the rung, which goes to the
    /// suggestion re-read after that game. So after a game Enter is "play
    /// again", and it is at the rung the game-over modal just named.
    pub fn resume_from(&mut self, last: &StartChoice) {
        self.chair = side_index(last.human);
        self.defaults = [last.corp_deck.clone(), last.runner_deck.clone()];
        self.reset_for_chair();
        self.style = self.styles().iter().position(|style| *style == last.style).unwrap_or(0);
    }

    /// The state without the filesystem, for tests and for `open`.
    pub fn with_decks(decks: [Vec<DeckRow>; 2], suggested: [Level; 2], defaults: [String; 2]) -> Self {
        let mut menu =
            Self { pane: Pane::Chair, chair: 0, level: 0, style: 0, opponent_deck: 0, own_deck: 0, decks, suggested, defaults };
        menu.reset_for_chair();
        menu
    }

    pub fn human(&self) -> Side {
        SIDES[self.chair]
    }

    pub fn bot(&self) -> Side {
        self.human().other()
    }

    pub fn level(&self) -> Level {
        Level::ALL[self.level]
    }

    /// The styles offered for the bot's chair: the deck's own first, then
    /// every profile written for that chair, then balanced.
    pub fn styles(&self) -> Vec<Option<Personality>> {
        let mut styles = vec![None];
        styles.extend(Personality::ALL.iter().copied().filter(|p| p.side() == Some(self.bot())).map(Some));
        styles.push(Some(Personality::Balanced));
        styles
    }

    pub fn style(&self) -> Option<Personality> {
        self.styles()[self.style]
    }

    /// The rung the record suggests for the chair now chosen.
    pub fn suggested(&self) -> Level {
        self.suggested[self.chair]
    }

    pub fn opponent_decks(&self) -> &[DeckRow] {
        &self.decks[side_index(self.bot())]
    }

    pub fn own_decks(&self) -> &[DeckRow] {
        &self.decks[side_index(self.human())]
    }

    /// Everything but the chair follows from the chair: the level cursor
    /// goes to that chair's suggestion, the style list is the bot's
    /// chair's, and the deck cursors go to the flag defaults.
    fn reset_for_chair(&mut self) {
        self.level = Level::ALL.iter().position(|l| *l == self.suggested[self.chair]).unwrap_or(2);
        self.style = 0;
        let default_of = |decks: &[DeckRow], id: &str| decks.iter().position(|d| d.id == id).unwrap_or(0);
        self.opponent_deck = default_of(self.opponent_decks(), &self.defaults[side_index(self.bot())]);
        self.own_deck = default_of(self.own_decks(), &self.defaults[side_index(self.human())]);
    }

    fn move_cursor(&mut self, delta: i32) {
        let (cursor, len) = match self.pane {
            Pane::Chair => (&mut self.chair, SIDES.len()),
            Pane::Level => (&mut self.level, Level::ALL.len()),
            Pane::Style => {
                let len = self.styles().len();
                (&mut self.style, len)
            }
            Pane::OpponentDeck => {
                let len = self.opponent_decks().len();
                (&mut self.opponent_deck, len)
            }
            Pane::OwnDeck => {
                let len = self.own_decks().len();
                (&mut self.own_deck, len)
            }
        };
        if len == 0 {
            return;
        }
        *cursor = (*cursor as i32 + delta).rem_euclid(len as i32) as usize;
        if self.pane == Pane::Chair {
            self.reset_for_chair();
        }
    }

    /// The choice as it stands, or `None` while a deck list is empty.
    pub fn choice(&self) -> Option<StartChoice> {
        let opponent = self.opponent_decks().get(self.opponent_deck)?;
        let own = self.own_decks().get(self.own_deck)?;
        let (corp_deck, runner_deck) = match self.human() {
            Side::Corp => (own.id.clone(), opponent.id.clone()),
            Side::Runner => (opponent.id.clone(), own.id.clone()),
        };
        Some(StartChoice { human: self.human(), level: self.level(), style: self.style(), corp_deck, runner_deck })
    }

    /// One change to the form. The keys that mean these are each
    /// client's: the terminal binds Tab and the arrows in `tui::start`,
    /// the desktop's drop-downs call `set_cursor` directly.
    pub fn apply(&mut self, intent: Intent) {
        match intent {
            Intent::NextPane => self.pane = self.pane.next(),
            Intent::PrevPane => self.pane = self.pane.previous(),
            Intent::Move(delta) => self.move_cursor(delta),
        }
    }

    /// The cursor of `pane`, for a client that shows each pane as its
    /// own control rather than moving a highlight between them.
    pub fn cursor(&self, pane: Pane) -> usize {
        match pane {
            Pane::Chair => self.chair,
            Pane::Level => self.level,
            Pane::Style => self.style,
            Pane::OpponentDeck => self.opponent_deck,
            Pane::OwnDeck => self.own_deck,
        }
    }

    /// Puts `pane`'s cursor on `index`, clamped to its list; a chair
    /// change resets the rest, as a move does.
    pub fn set_cursor(&mut self, pane: Pane, index: usize) {
        let len = self.panes().into_iter().find(|(p, ..)| *p == pane).map_or(0, |(_, _, rows, _)| rows.len());
        if len == 0 {
            return;
        }
        let index = index.min(len - 1);
        match pane {
            Pane::Chair => {
                self.chair = index;
                self.reset_for_chair();
            }
            Pane::Level => self.level = index,
            Pane::Style => self.style = index,
            Pane::OpponentDeck => self.opponent_deck = index,
            Pane::OwnDeck => self.own_deck = index,
        }
    }

    /// The five lists as `(title, rows, cursor)`, for `draw` and for tests
    /// that check what a player would see.
    pub fn panes(&self) -> Vec<(Pane, String, Vec<String>, usize)> {
        let bot = self.bot();
        let chair_rows = SIDES.iter().map(|side| format!("{side:?}")).collect();
        let level_rows = Level::ALL
            .iter()
            .map(|level| {
                let spec = level.spec(bot);
                let mark = if *level == self.suggested[self.chair] { "  ◆ suggested" } else { "" };
                format!("{}. {} — {}{mark}", level.rung(), level.name(), spec.describe())
            })
            .collect();
        let style_rows = self
            .styles()
            .into_iter()
            .map(|style| match style {
                None => "the deck's own style".to_string(),
                Some(personality) => personality.name().to_string(),
            })
            .collect();
        let deck_rows = |decks: &[DeckRow]| decks.iter().map(DeckRow::label).collect::<Vec<_>>();
        vec![
            (Pane::Chair, "Your chair".to_string(), chair_rows, self.chair),
            (Pane::Level, format!("Opponent ({bot:?}) level"), level_rows, self.level),
            (Pane::Style, "Opponent style".to_string(), style_rows, self.style),
            (Pane::OpponentDeck, format!("Opponent's deck ({bot:?})"), deck_rows(self.opponent_decks()), self.opponent_deck),
            (Pane::OwnDeck, format!("Your deck ({:?})", self.human()), deck_rows(self.own_decks()), self.own_deck),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, side: Side) -> DeckRow {
        DeckRow {
            id: id.to_string(),
            name: id.replace('_', " "),
            style: Some(if side == Side::Corp { "rush" } else { "aggressive" }.to_string()),
            identity: "Someone".to_string(),
            saved: false,
        }
    }

    fn menu() -> StartMenu {
        StartMenu::with_decks(
            [vec![row("brick_stack", Side::Corp), row("discretion_advised", Side::Corp)], vec![row("stolen_goods", Side::Runner), row("dashing_mad", Side::Runner)]],
            [Level::Apprentice, Level::Veteran],
            ["discretion_advised".to_string(), "stolen_goods".to_string()],
        )
    }

    #[test]
    fn the_cursors_start_on_the_suggested_rung_and_the_default_decks() {
        let menu = menu();
        assert_eq!(menu.human(), Side::Corp);
        assert_eq!(menu.level(), Level::Apprentice, "the Corp chair's suggestion");
        let choice = menu.choice().unwrap();
        assert_eq!((choice.corp_deck.as_str(), choice.runner_deck.as_str()), ("discretion_advised", "stolen_goods"));
        assert_eq!(choice.style, None, "the deck's own style by default");
    }

    #[test]
    fn changing_chair_swaps_the_lists_and_re_reads_the_suggestion() {
        let mut menu = menu();
        menu.apply(Intent::Move(1));
        assert_eq!(menu.human(), Side::Runner);
        assert_eq!(menu.level(), Level::Veteran, "the Runner chair's suggestion");
        let panes = menu.panes();
        assert!(panes[1].1.contains("Corp"), "the opponent is now the Corp: {}", panes[1].1);
        assert!(panes[2].2.contains(&"rush".to_string()) && !panes[2].2.contains(&"aggressive".to_string()), "{:?}", panes[2].2);
        let choice = menu.choice().unwrap();
        assert_eq!(choice.human, Side::Runner);
        assert_eq!((choice.corp_deck.as_str(), choice.runner_deck.as_str()), ("discretion_advised", "stolen_goods"));
    }

    #[test]
    fn the_choice_is_read_from_any_pane() {
        let mut menu = menu();
        menu.apply(Intent::NextPane);
        assert_eq!(menu.pane, Pane::Level);
        menu.apply(Intent::Move(1));
        assert_eq!(menu.level(), Level::Operator);
        menu.apply(Intent::NextPane);
        menu.apply(Intent::Move(1));
        assert_eq!(menu.style(), Some(Personality::Aggressive), "the first profile written for the Runner");
        menu.apply(Intent::NextPane);
        menu.apply(Intent::Move(1));
        let choice = menu.choice().expect("a choice from any pane");
        assert_eq!(choice.level, Level::Operator);
        assert_eq!(choice.style, Some(Personality::Aggressive));
        assert_eq!(choice.runner_deck, "dashing_mad");
    }

    #[test]
    fn resuming_keeps_the_game_just_played_but_takes_the_new_suggestion() {
        let mut menu = menu();
        let last = StartChoice {
            human: Side::Runner,
            level: Level::Novice,
            style: Some(Personality::Glacier),
            corp_deck: "brick_stack".to_string(),
            runner_deck: "dashing_mad".to_string(),
        };
        menu.resume_from(&last);
        let choice = menu.choice().unwrap();
        assert_eq!(choice.level, Level::Veteran, "the suggestion, not the rung last played");
        assert_eq!(choice, StartChoice { level: Level::Veteran, ..last });
    }

    #[test]
    fn the_panes_wrap_in_both_directions() {
        let mut menu = menu();
        menu.apply(Intent::PrevPane);
        assert_eq!(menu.pane, Pane::OwnDeck);
        menu.apply(Intent::NextPane);
        assert_eq!(menu.pane, Pane::Chair);
        menu.apply(Intent::Move(-1));
        assert_eq!(menu.human(), Side::Runner, "the chair list wraps too");
    }

    /// The desktop sets a pane's cursor from a drop-down: a chair chosen
    /// that way resets the rest exactly as a key move does, and an index
    /// past the list lands on its last entry rather than panicking.
    #[test]
    fn a_cursor_set_directly_behaves_like_a_move() {
        let mut menu = menu();
        menu.set_cursor(Pane::Chair, 1);
        assert_eq!(menu.human(), Side::Runner);
        assert_eq!(menu.level(), Level::Veteran, "the chair change re-read the suggestion");
        menu.set_cursor(Pane::OwnDeck, 99);
        assert_eq!(menu.cursor(Pane::OwnDeck), 1, "clamped to the last deck");
        assert_eq!(menu.choice().unwrap().runner_deck, "dashing_mad");
        menu.set_cursor(Pane::Level, 0);
        assert_eq!(menu.level(), Level::Novice);
    }

    #[test]
    fn the_suggested_rung_is_marked() {
        let panes = menu().panes();
        let marked: Vec<&String> = panes[1].2.iter().filter(|row| row.contains("suggested")).collect();
        assert_eq!(marked.len(), 1);
        assert!(marked[0].starts_with("2. apprentice"), "{}", marked[0]);
    }

    #[test]
    fn an_empty_deck_list_cannot_start() {
        let mut empty = StartMenu::with_decks([Vec::new(), Vec::new()], [Level::Operator; 2], [String::new(), String::new()]);
        empty.apply(Intent::Move(1));
        assert_eq!(empty.choice(), None);
    }
}
