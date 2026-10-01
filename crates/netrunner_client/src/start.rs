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
pub use netrunner_bots::{Level, Plan, Style};
use netrunner_core::cards::CardRegistry;
use netrunner_core::format::NsgFormat;
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
    pub style: Vec<String>,
    pub identity: String,
    pub saved: bool,
    /// Why the deck cannot start a game in the format the form was
    /// opened for — the validators' first complaint — or `None` for a
    /// legal deck. A saved deck may be illegal (a builder saves whatever
    /// is built), and the form lists it anyway, marked, rather than
    /// leaving a person wondering where their deck went; Start then
    /// refuses it with this reason (`StartMenu::choice_problem`).
    pub problem: Option<String>,
}

impl DeckRow {
    pub fn label(&self) -> String {
        let style = if self.style.is_empty() { "balanced".to_string() } else { self.style.join("+") };
        let saved = if self.saved { " (saved)" } else { "" };
        let problem = if self.problem.is_some() { " — not playable here" } else { "" };
        format!("{} · {style} · {}{saved}{problem}", self.name, self.identity)
    }
}

/// The panes, in Tab order. Chair first because it decides what the
/// others offer. `Opponent` is offered only when the settings hold a
/// model opponent (`StartMenu::order`), and `Level` and `Style` only
/// while the built-in bot is chosen: a model has no rung, and the rung a
/// choice then carries is the chair's suggestion, which plays whatever
/// the model does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chair,
    Opponent,
    Level,
    Style,
    OpponentDeck,
    OwnDeck,
}

/// Who sits in the other chair: the ladder's bot, or a model opponent by
/// its profile name — a name, which the client resolves to the profile
/// and its key when the game starts (`play::Opponent::Model`), so the
/// form holds no key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpponentChoice {
    BuiltIn,
    Model(String),
}

/// What the player chose, in the vocabulary of the flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartChoice {
    pub human: Side,
    pub opponent: OpponentChoice,
    /// The rung: the bot's, or the planner's behind a model.
    pub level: Level,
    /// `None` is the deck's own style, the same as an unset
    /// `--corp-style`.
    pub style: Option<Style>,
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
    /// 0 is the built-in bot; `n` is `opponents[n - 1]`.
    opponent: usize,
    /// The model opponents the settings hold, by profile name. Empty on
    /// the terminal, which plays one by flag.
    opponents: Vec<String>,
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
    ///
    /// A deck file that cannot be read is left out rather than emptying
    /// the form (`deck_store::list_lenient`); each deck is checked in
    /// `format`, the one a game will be started in.
    pub fn open(
        decks_dir: &Path,
        record_path: Option<&Path>,
        player: &str,
        registry: &CardRegistry,
        format: NsgFormat,
        defaults: [String; 2],
        opponents: Vec<String>,
    ) -> Result<Self, String> {
        let mut decks: [Vec<DeckRow>; 2] = [Vec::new(), Vec::new()];
        let (stored_decks, _unreadable) = deck_store::list_lenient(decks_dir);
        for stored in stored_decks {
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
                problem: stored.deck.validate(registry, format).err().map(|error| error.to_string()),
            });
        }
        let suggested = match record_path.map(LocalRecord::load) {
            Some(Ok(log)) => SIDES.map(|side| log.suggest(player, side)),
            _ => [Level::Operator, Level::Operator],
        };
        Ok(Self::with_decks(decks, suggested, defaults).with_opponents(opponents))
    }

    /// The model opponents to offer, by name. The cursor stays on the
    /// built-in bot.
    pub fn with_opponents(mut self, opponents: Vec<String>) -> Self {
        self.opponents = opponents;
        self.opponent = 0;
        self
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
        // A model still offered is kept; one removed since is the bot.
        self.opponent = match &last.opponent {
            OpponentChoice::BuiltIn => 0,
            OpponentChoice::Model(name) => self.opponents.iter().position(|offered| offered == name).map_or(0, |i| i + 1),
        };
        if self.panes().iter().all(|(pane, ..)| *pane != self.pane) {
            self.pane = Pane::Chair;
        }
    }

    /// The state without the filesystem, for tests and for `open`.
    pub fn with_decks(decks: [Vec<DeckRow>; 2], suggested: [Level; 2], defaults: [String; 2]) -> Self {
        let mut menu = Self {
            pane: Pane::Chair,
            chair: 0,
            opponent: 0,
            opponents: Vec::new(),
            level: 0,
            style: 0,
            opponent_deck: 0,
            own_deck: 0,
            decks,
            suggested,
            defaults,
        };
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

    /// Who sits in the other chair.
    pub fn opponent(&self) -> OpponentChoice {
        match self.opponent.checked_sub(1).and_then(|i| self.opponents.get(i)) {
            Some(name) => OpponentChoice::Model(name.clone()),
            None => OpponentChoice::BuiltIn,
        }
    }

    /// The Opponent pane's rows: the built-in bot, then each model.
    pub fn opponent_rows(&self) -> Vec<String> {
        std::iter::once("The built-in bot".to_string()).chain(self.opponents.iter().map(|name| format!("{name} (AI model)"))).collect()
    }

    /// The panes on offer, in Tab order: the Opponent pane only when a
    /// model is configured, the rung and style only for the built-in bot.
    fn order(&self) -> Vec<Pane> {
        let mut order = vec![Pane::Chair];
        if !self.opponents.is_empty() {
            order.push(Pane::Opponent);
        }
        if self.opponent() == OpponentChoice::BuiltIn {
            order.extend([Pane::Level, Pane::Style]);
        }
        order.extend([Pane::OpponentDeck, Pane::OwnDeck]);
        order
    }

    fn step_pane(&mut self, delta: i32) {
        let order = self.order();
        let index = order.iter().position(|pane| *pane == self.pane).unwrap_or(0);
        self.pane = order[(index as i32 + delta).rem_euclid(order.len() as i32) as usize];
    }

    /// The styles offered for the bot's chair: the deck's own first, then
    /// each plan written for that chair on its own, then balanced. A
    /// stacked style is a deck's to name (`DeckFile::style`), not the
    /// form's: the form picks one word.
    pub fn styles(&self) -> Vec<Option<Style>> {
        let mut styles = vec![None];
        styles.extend(Plan::for_side(self.bot()).map(|plan| Some(Style::of(plan))));
        styles.push(Some(Style::BALANCED));
        styles
    }

    pub fn style(&self) -> Option<Style> {
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
            Pane::Opponent => (&mut self.opponent, self.opponents.len() + 1),
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
        // Behind a model the rung is the chair's suggestion: the planner
        // that plays what the model does not should be the one the
        // person would have faced.
        let level = match self.opponent() {
            OpponentChoice::BuiltIn => self.level(),
            OpponentChoice::Model(_) => self.suggested(),
        };
        Some(StartChoice { human: self.human(), opponent: self.opponent(), level, style: self.style(), corp_deck, runner_deck })
    }

    /// Why the chosen decks cannot start a game, naming the deck — or
    /// `None` when both can. The desktop's Start and the terminal's Enter
    /// ask this first, so an illegal saved deck is refused with its
    /// reason at the form rather than by the resolver after it.
    pub fn choice_problem(&self) -> Option<String> {
        [self.own_decks().get(self.own_deck), self.opponent_decks().get(self.opponent_deck)]
            .into_iter()
            .flatten()
            .find_map(|deck| deck.problem.as_ref().map(|problem| format!("{} cannot be played in this format: {problem}", deck.name)))
    }

    /// One change to the form. The keys that mean these are each
    /// client's: the terminal binds Tab and the arrows in `tui::start`,
    /// the desktop's drop-downs call `set_cursor` directly.
    pub fn apply(&mut self, intent: Intent) {
        match intent {
            Intent::NextPane => self.step_pane(1),
            Intent::PrevPane => self.step_pane(-1),
            Intent::Move(delta) => self.move_cursor(delta),
        }
    }

    /// The cursor of `pane`, for a client that shows each pane as its
    /// own control rather than moving a highlight between them.
    pub fn cursor(&self, pane: Pane) -> usize {
        match pane {
            Pane::Chair => self.chair,
            Pane::Opponent => self.opponent,
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
            Pane::Opponent => self.opponent = index,
            Pane::Level => self.level = index,
            Pane::Style => self.style = index,
            Pane::OpponentDeck => self.opponent_deck = index,
            Pane::OwnDeck => self.own_deck = index,
        }
    }

    /// The lists on offer as `(pane, title, rows, cursor)`, in Tab order
    /// (`order`), for `draw` and for tests that check what a player would
    /// see. Five with no model configured, which is what the terminal's
    /// fixed layout draws.
    pub fn panes(&self) -> Vec<(Pane, String, Vec<String>, usize)> {
        let bot = self.bot();
        let chair_rows: Vec<String> = SIDES.iter().map(|side| format!("{side:?}")).collect();
        let level_rows: Vec<String> = Level::ALL
            .iter()
            .map(|level| {
                let spec = level.spec(bot);
                let mark = if *level == self.suggested[self.chair] { "  ◆ suggested" } else { "" };
                format!("{}. {} — {}{mark}", level.rung(), level.name(), spec.describe())
            })
            .collect();
        let style_rows: Vec<String> = self
            .styles()
            .into_iter()
            .map(|style| match style {
                None => "the deck's own style".to_string(),
                Some(style) => style.to_string(),
            })
            .collect();
        let deck_rows = |decks: &[DeckRow]| decks.iter().map(DeckRow::label).collect::<Vec<_>>();
        self.order()
            .into_iter()
            .map(|pane| match pane {
                Pane::Chair => (pane, "Your chair".to_string(), chair_rows.clone(), self.chair),
                Pane::Opponent => (pane, format!("Opponent ({bot:?})"), self.opponent_rows(), self.opponent),
                Pane::Level => (pane, format!("Opponent ({bot:?}) level"), level_rows.clone(), self.level),
                Pane::Style => (pane, "Opponent style".to_string(), style_rows.clone(), self.style),
                Pane::OpponentDeck => (pane, format!("Opponent's deck ({bot:?})"), deck_rows(self.opponent_decks()), self.opponent_deck),
                Pane::OwnDeck => (pane, format!("Your deck ({:?})", self.human()), deck_rows(self.own_decks()), self.own_deck),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, side: Side) -> DeckRow {
        DeckRow {
            id: id.to_string(),
            name: id.replace('_', " "),
            style: vec![if side == Side::Corp { "fast-advance" } else { "pressure" }.to_string()],
            identity: "Someone".to_string(),
            saved: false,
            problem: None,
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
        assert!(panes[2].2.contains(&"fast-advance".to_string()) && !panes[2].2.contains(&"pressure".to_string()), "{:?}", panes[2].2);
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
        assert_eq!(menu.style(), Some(Style::of(Plan::Dismantle)), "the first plan written for the Runner");
        menu.apply(Intent::NextPane);
        menu.apply(Intent::Move(1));
        let choice = menu.choice().expect("a choice from any pane");
        assert_eq!(choice.level, Level::Operator);
        assert_eq!(choice.style, Some(Style::of(Plan::Dismantle)));
        assert_eq!(choice.runner_deck, "dashing_mad");
    }

    #[test]
    fn resuming_keeps_the_game_just_played_but_takes_the_new_suggestion() {
        let mut menu = menu();
        let last = StartChoice {
            human: Side::Runner,
            opponent: OpponentChoice::BuiltIn,
            level: Level::Novice,
            style: Some(Style::of(Plan::Glacier)),
            corp_deck: "brick_stack".to_string(),
            runner_deck: "dashing_mad".to_string(),
        };
        menu.resume_from(&last);
        let choice = menu.choice().unwrap();
        assert_eq!(choice.level, Level::Veteran, "the suggestion, not the rung last played");
        assert_eq!(choice, StartChoice { level: Level::Veteran, ..last });
    }

    /// The terminal draws five fixed areas and zips them with `panes()`,
    /// so with no model configured the list is exactly those five.
    #[test]
    fn with_no_model_the_panes_are_the_five_in_order() {
        let panes: Vec<Pane> = menu().panes().into_iter().map(|(pane, ..)| pane).collect();
        assert_eq!(panes, [Pane::Chair, Pane::Level, Pane::Style, Pane::OpponentDeck, Pane::OwnDeck]);
        assert_eq!(menu().opponent(), OpponentChoice::BuiltIn);
        assert_eq!(menu().choice().unwrap().opponent, OpponentChoice::BuiltIn);
    }

    /// A model chosen takes the rung and style panes away — a model has
    /// no rung — and the choice carries the chair's suggested rung as the
    /// planner behind it.
    #[test]
    fn choosing_a_model_hides_the_rung_and_style_and_carries_the_suggested_rung() {
        let mut menu = menu().with_opponents(vec!["claude".to_string(), "local".to_string()]);
        let panes: Vec<Pane> = menu.panes().into_iter().map(|(pane, ..)| pane).collect();
        assert_eq!(panes, [Pane::Chair, Pane::Opponent, Pane::Level, Pane::Style, Pane::OpponentDeck, Pane::OwnDeck]);
        assert_eq!(menu.opponent_rows(), ["The built-in bot", "claude (AI model)", "local (AI model)"]);
        menu.apply(Intent::NextPane);
        assert_eq!(menu.pane, Pane::Opponent, "in Tab order when there are models");
        menu.set_cursor(Pane::Level, 4);
        menu.apply(Intent::Move(2));
        assert_eq!(menu.opponent(), OpponentChoice::Model("local".to_string()));
        let panes: Vec<Pane> = menu.panes().into_iter().map(|(pane, ..)| pane).collect();
        assert_eq!(panes, [Pane::Chair, Pane::Opponent, Pane::OpponentDeck, Pane::OwnDeck]);
        let choice = menu.choice().unwrap();
        assert_eq!(choice.opponent, OpponentChoice::Model("local".to_string()));
        assert_eq!(choice.level, Level::Apprentice, "the Corp chair's suggestion, not the rung set while the bot was chosen");
        menu.apply(Intent::NextPane);
        assert_eq!(menu.pane, Pane::OpponentDeck, "Tab skips the hidden panes");
        menu.pane = Pane::Opponent;
        menu.apply(Intent::Move(1));
        assert_eq!(menu.opponent(), OpponentChoice::BuiltIn, "the list wraps back to the bot");
        assert_eq!(menu.choice().unwrap().level, Level::Elite, "and the rung set earlier is back");
        // A chair change keeps the opponent chosen.
        menu.set_cursor(Pane::Opponent, 1);
        menu.set_cursor(Pane::Chair, 1);
        assert_eq!(menu.opponent(), OpponentChoice::Model("claude".to_string()));
        assert_eq!(menu.choice().unwrap().level, Level::Veteran);
    }

    #[test]
    fn resuming_keeps_a_model_still_offered_and_drops_one_removed() {
        let mut menu = menu().with_opponents(vec!["claude".to_string()]);
        let last = StartChoice {
            human: Side::Corp,
            opponent: OpponentChoice::Model("claude".to_string()),
            level: Level::Novice,
            style: None,
            corp_deck: "brick_stack".to_string(),
            runner_deck: "stolen_goods".to_string(),
        };
        menu.resume_from(&last);
        assert_eq!(menu.opponent(), OpponentChoice::Model("claude".to_string()));
        let mut menu = menu.with_opponents(Vec::new());
        menu.pane = Pane::Opponent;
        menu.resume_from(&last);
        assert_eq!(menu.opponent(), OpponentChoice::BuiltIn, "the profile is gone");
        assert_eq!(menu.pane, Pane::Chair, "and the cursor is not left on a pane that is not there");
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

    /// An illegal deck is listed, marked, and named when chosen.
    #[test]
    fn a_deck_with_a_problem_is_listed_and_refused_by_name() {
        let mut illegal = row("my_deck", Side::Runner);
        illegal.problem = Some("deck has 3 cards".to_string());
        let mut menu = StartMenu::with_decks(
            [vec![row("discretion_advised", Side::Corp)], vec![row("stolen_goods", Side::Runner), illegal]],
            [Level::Operator; 2],
            ["discretion_advised".to_string(), "stolen_goods".to_string()],
        );
        assert_eq!(menu.choice_problem(), None);
        assert!(menu.panes()[4].2.is_empty() || menu.own_decks().len() == 1, "the Corp chair's own list");
        menu.set_cursor(Pane::OpponentDeck, 1);
        let problem = menu.choice_problem().expect("the illegal deck is chosen");
        assert!(problem.contains("my deck") && problem.contains("3 cards"), "{problem}");
        assert!(menu.opponent_decks()[1].label().contains("not playable"));
    }

    #[test]
    fn an_empty_deck_list_cannot_start() {
        let mut empty = StartMenu::with_decks([Vec::new(), Vec::new()], [Level::Operator; 2], [String::new(), String::new()]);
        empty.apply(Intent::Move(1));
        assert_eq!(empty.choice(), None);
    }
}
